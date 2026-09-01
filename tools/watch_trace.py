#!/usr/bin/env python3
"""被动录制器：玩家手动打，我只在旁边看着记。

和 record_trace.py 的分工：那个是**驱动器**（它执行动作，所以动作是已知的），
这个是**旁观者**（玩家自己点，动作只能从前后两帧的 diff 反推）。

于是有一条硬规矩：**这里写进 trace 的每个 action 都带 inferred: true。**
反推不出来的就写 kind="unknown"，绝不挑一个自洽的解填进去 ——
一个自信地填错的动作，比一个诚实的 unknown 更难查。

配置走固定路径 traces/_watch.txt（和 _plan.txt 同一个套路：
命令字符串不变就不用反复过权限确认）。两行：

    trace   traces/xxx.json
    raw-dir traces/raw_xxx
"""
from __future__ import annotations

import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from record_trace import (  # noqa: E402
    POLL_INTERVAL_MS,
    _control_returned,
    STABLE_POLLS,
    get_state,
    load_trace,
    new_trace,
    normalize,
    resolve,
    save_raw,
    save_trace,
    wait_for_game,
)

WATCH_FILE = resolve("traces/_watch.txt")
IDLE_STOP_S = 90.0


def _names(cards) -> list[str]:
    return sorted((c or {}).get("name") or "" for c in (cards or []))


def _multiset_removed(before, after) -> list[str]:
    rest = list(_names(after))
    out = []
    for n in _names(before):
        if n in rest:
            rest.remove(n)
        else:
            out.append(n)
    return out


def _changed_enemies(a: dict, b: dict) -> list[str]:
    out = []
    for k, eb in (b.get("enemies") or {}).items():
        ea = (a.get("enemies") or {}).get(k)
        if ea is None:
            continue
        if (ea.get("hp"), ea.get("block"), ea.get("status")) != (
            eb.get("hp"), eb.get("block"), eb.get("status")
        ):
            out.append(eb.get("entity_id") or k)
    return out


def infer_action(a: dict, b: dict) -> dict:
    """a -> b 这一步是什么动作。**只在有把握时给出 kind，否则 unknown。**"""
    base = {"inferred": True}

    # 1) 回合切换。判据取 mod 报的回合数/行动方/出牌阶段。
    if (
        a.get("round") != b.get("round")
        or a.get("side") != b.get("side")
        or (a.get("is_play_phase") and not b.get("is_play_phase"))
    ):
        return {**base, "kind": "end_turn"}

    # 2) 药水。槽位是稳定键；槽号在喝掉之后会重排，所以比集合不比长度。
    sa = {p.get("slot") for p in (a.get("potions") or [])}
    sb = {p.get("slot") for p in (b.get("potions") or [])}
    gone = sa - sb
    if len(gone) == 1 and len(sa) - len(sb) == 1:
        slot = next(iter(gone))
        name = next(
            (p.get("name") for p in (a.get("potions") or []) if p.get("slot") == slot), None
        )
        act = {**base, "kind": "use_potion", "slot": slot, "potion_name": name}
        tgt = _changed_enemies(a, b)
        if len(tgt) == 1:
            act["target"] = tgt[0]
        return act

    # 3) 选牌界面开合：分不出 select 还是 confirm，不猜。
    if bool(a.get("pending")) != bool(b.get("pending")):
        return {**base, "kind": "unknown", "why": "选牌界面开合，分不出 select 还是 confirm"}

    # 4) 打牌：手牌**恰好**少一张，且它进了弃牌/消耗堆，或能量掉了。
    #    "恰好一张" 是关键——抽牌/生成牌会让手牌同时增减，那种情况不硬猜。
    removed = _multiset_removed(a.get("hand"), b.get("hand"))
    if len(removed) == 1:
        name = removed[0]
        went = (
            name in _names(b.get("discard")) and name not in _names(a.get("discard"))
        ) or (
            name in _names(b.get("exhaust")) and name not in _names(a.get("exhaust"))
        )
        energy_dropped = (a.get("energy") or 0) > (b.get("energy") or 0)
        if went or energy_dropped:
            # **下标是必填的**：验证器按 slot 从内核手牌里取牌，只给名字它会
            # 默认 0 号位，于是内核打出的是另一张牌 —— 2026-08-23 这条 trace
            # 第一次跑就是这么红的 27 帧（游戏打邪眼+、内核打究极打击+）。
            # 下标取**动作发生之前**那一帧的手牌位置，那正是游戏当时用的下标。
            slot, cid = None, None
            for i, c in enumerate(a.get("hand") or []):
                if (c or {}).get("name") == name:
                    slot, cid = i, (c or {}).get("id")
                    break
            act = {**base, "kind": "play_card", "card_name": name}
            if slot is not None:
                act["slot"] = slot
                act["card_id"] = cid
            tgt = _changed_enemies(a, b)
            if len(tgt) == 1:
                act["target"] = tgt[0]
            elif len(tgt) > 1:
                act["target"] = None
            return act

    return {**base, "kind": "unknown", "why": "手牌/能量/牌堆的变化对不上任何单一动作"}


def read_watch_file() -> tuple[str, str | None]:
    if not os.path.exists(WATCH_FILE):
        raise SystemExit(f"缺配置文件 {WATCH_FILE}")
    trace_path, raw_dir = None, None
    with open(WATCH_FILE, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            key, _, val = line.partition(" ")
            val = val.strip()
            if key == "trace":
                trace_path = resolve(val)
            elif key == "raw-dir":
                raw_dir = resolve(val)
    if not trace_path:
        raise SystemExit(f"{WATCH_FILE} 里没有 trace 那一行")
    return trace_path, raw_dir


def brief(obs: dict) -> str:
    p = obs.get("player") or {}
    es = []
    for e in (obs.get("enemies") or {}).values():
        es.append(f"{e.get('name')} {e.get('hp')}(护{e.get('block')})")
    return (
        f"回合{obs.get('round')} {obs.get('side')} 能量{obs.get('energy')}"
        f" | 我 {p.get('hp')} 护{p.get('block')} | " + " ".join(es)
    )


def main() -> None:
    trace_path, raw_dir = read_watch_file()
    wait_for_game()

    trace = load_trace(trace_path)
    if trace is None:
        trace = new_trace(get_state())
        print(f"新建 trace -> {trace_path}")
    frames = trace["frames"]
    if not frames:
        raw = get_state()
        frames.append({"i": 0, "obs": normalize(raw), "action": None})
        save_raw(raw_dir, 0, raw)
        save_trace(trace_path, trace)
    print(f"被动录制中 -> {trace_path}（已有 {len(frames)} 帧）。Ctrl-C 收工。")
    print(f"起点：{brief(frames[-1]['obs'])}")
    sys.stdout.flush()

    stable_obs, stable_raw, stable_n = None, None, 0
    last_change = time.time()
    interval = POLL_INTERVAL_MS / 1000.0

    while True:
        time.sleep(interval)
        try:
            raw = get_state()
        except Exception:
            continue
        obs = normalize(raw)

        if obs == stable_obs:
            stable_n += 1
        else:
            stable_obs, stable_raw, stable_n = obs, raw, 1
        if stable_n != STABLE_POLLS:
            if time.time() - last_change > IDLE_STOP_S:
                print(f"\n{IDLE_STOP_S:.0f} 秒没有新动作，收工。共 {len(frames)} 帧。")
                return
            continue

        # **敌方回合的中间态不存帧。** 敌人两次出招之间的停顿轻松超过稳定窗口，
        # 存下来的话一次「结束回合」会被记成两个动作，第二个是凭空多出来的。
        # 驱动式录制器靠 want_play_phase=True 跨过整个敌方回合，这里等价。
        # （record_trace._control_returned 的注释里已经记着这个坑，我又踩了一次。）
        if not _control_returned(obs):
            if time.time() - last_change > IDLE_STOP_S:
                print(f"{IDLE_STOP_S:.0f} 秒没有新动作，收工。共 {len(frames)} 帧。")
                return
            continue

        prev = frames[-1]["obs"]
        if obs == prev:
            continue

        action = infer_action(prev, obs)
        frames[-1]["action"] = action
        frames.append({"i": len(frames), "obs": obs, "action": None})
        save_raw(raw_dir, len(frames) - 1, stable_raw)
        save_trace(trace_path, trace)
        last_change = time.time()

        mark = "?" if action["kind"] == "unknown" else " "
        detail = action.get("card_name") or action.get("potion_name") or ""
        tgt = action.get("target")
        arrow = f" -> {tgt}" if tgt else ""
        print(f"[{len(frames) - 1:3d}]{mark} {action['kind']:16s} {detail}{arrow}")
        print(f"      {brief(obs)}")
        if action["kind"] == "unknown":
            print(f"      ! 反推不出动作：{action.get('why')}")
        sys.stdout.flush()


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\n收工。")
