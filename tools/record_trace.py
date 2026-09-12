#!/usr/bin/env python3
r"""录制对拍 trace：把真实对局的 `观测 -> 动作 -> 观测` 写成 JSON。

只用标准库（urllib），不依赖 httpx / MCP —— 录制器必须能在游戏跑着、
MCP server 也跑着的时候独立工作，不跟它抢连接。

解释器：这台机器 PATH 上的 `python` 是 Windows Store 的假壳（无输出、退出 9009），
`py` 也没有。用项目自带的 venv，本脚本只依赖标准库，用哪个 venv 都行：

    $PY = "D:\game mod\sts2sim\.venv\Scripts\python.exe"

用法（每次调用做一个动作，把帧追加进同一个文件）:

    & $PY record_trace.py --trace t.json state          # 只看，不动
    & $PY record_trace.py --trace t.json init           # 开录，写下第一帧观测
    & $PY record_trace.py --trace t.json play 0 --target jaw_worm_0
    & $PY record_trace.py --trace t.json play 2
    & $PY record_trace.py --trace t.json end            # 结束回合
    & $PY record_trace.py --trace t.json select 1       # 选牌界面选一张
    & $PY record_trace.py --trace t.json confirm
    & $PY record_trace.py --trace t.json finish         # 收尾

每次调用都会把结算后的观测打印成一行行紧凑文本，供人/agent 决定下一步。
无状态：进程不需要常驻，状态全在 trace 文件里。

## 计划模式（--plan）

上面那套的问题是**命令字符串每次都不一样**，于是每出一张牌都要过一次权限确认。
计划模式把「变的东西」全挪进一个固定路径的文本文件 `sts2core/traces/_plan.txt`，
命令就永远是同一条、可以一次性放行：

    & $PY record_trace.py --plan

`_plan.txt` 每行一个步骤，语法和上面的子命令完全一样，`#` 开头是注释：

    trace traces/act1_f12_seventh.json    # 后续步骤写进哪条 trace（相对 sts2core/）
    raw-dir traces/raw/f12                # 可选
    play 0 --target CROSSBOW_RUBY_RAIDER_0
    play 2 --target CROSSBOW_RUBY_RAIDER_0
    end
    note 斧手意图对照实验：挂 debuff 前 Attack:5

语义：
* 顺序执行，每步执行前打印 `[k/n]`，每步结束后立刻落盘 trace（中途崩了不丢帧）。
* **任何一步失败就停**，并明确报出是第几步、原文是什么、什么原因。
* 跑完（无论成败）都会重写 `_plan.txt`：保留 `trace`/`raw-dir` 设定，
  **其余步骤一律注释掉**。所以重复执行同一条命令是安全的空操作——
  要再做事必须显式重写计划文件。

格式见 docs/trace-format.md。
"""

from __future__ import annotations

import argparse
import datetime as _dt
import json
import os
import shlex
import subprocess
import sys
import time
import urllib.error
import urllib.request

BASE_URL = os.environ.get("STS2_MCP_URL", "http://localhost:15526")
SP_URL = BASE_URL + "/api/v1/singleplayer"

# 所有相对路径都锚在 sts2core/ 上，而不是当前工作目录——固定命令行的前提是
# 命令里不带路径，那路径就不能依赖「从哪儿调用的」。
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def run_verify_last(trace_path: str | None, mode: str = "driven") -> None:
    """在线对拍钩子：每步落盘后调用 verify.exe --last 校验最新一帧。
    耗时约 30ms，若发现不一致当场标红报警。
    mode: 'driven'（主动录制，动作绝对精确）或 'inferred'（被动反推，动作可能反推错）
    """
    if not trace_path or not os.path.exists(trace_path):
        return

    verify_exe = os.path.join(ROOT, "target", "release", "verify.exe")
    if not os.path.exists(verify_exe):
        verify_exe = os.path.join(ROOT, "target", "release", "verify")
    if not os.path.exists(verify_exe):
        verify_exe = os.path.join(ROOT, "target", "debug", "verify.exe")
    if not os.path.exists(verify_exe):
        return

    try:
        res = subprocess.run(
            [verify_exe, trace_path, "--last"],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=5.0,
        )
        if res.returncode != 0:
            RED = "\033[1;31m"
            YELLOW = "\033[1;33m"
            RESET = "\033[0m"
            print(f"\n{RED}{'='*65}{RESET}")
            if mode == "driven":
                print(f"{RED}[!] 【在线对拍报警 [x] driven】动作精确已知，规则必定算错！{RESET}")
            else:
                print(f"{YELLOW}[!] 【在线对拍报警 [x] inferred】动作系反推，“读它的红先怀疑动作”{RESET}")
            print(f"{RED}{'='*65}{RESET}")
            if res.stdout.strip():
                print(res.stdout.strip())
            if res.stderr.strip():
                print(res.stderr.strip())
            print(f"{RED}{'='*65}{RESET}\n")
            sys.stdout.flush()
        else:
            out = res.stdout.strip()
            if out:
                print(f"  [对拍] {out}")
                sys.stdout.flush()
    except Exception:
        pass
PLAN_PATH = os.path.join(ROOT, "traces", "_plan.txt")


def resolve(path: str) -> str:
    return path if os.path.isabs(path) else os.path.normpath(os.path.join(ROOT, path))


def relpath(path: str) -> str:
    try:
        return os.path.relpath(path, ROOT).replace("\\", "/")
    except ValueError:  # 跨盘符
        return path

# settle 判据：连续 STABLE_POLLS 次观测完全相同才算结算完毕。
#
# 稳定窗口 = POLL_INTERVAL_MS × (STABLE_POLLS - 1)，必须**大于多段攻击两次
# 命中之间的间隔**，否则会在中途就判定结算完毕。实测（2026-08-15）：拆卸+
# 攻击两次，原来 3×40ms=120ms 的窗口在第一次命中后就返回了，读到 49→34，
# 而真实结果是 49→19。现在给到 500ms。
#
# 代价是每个动作多等半秒，一场战斗十几秒。换来的是观测可信——中途状态不只
# 污染 trace，还会污染基于它做出的下一手决策。
POLL_INTERVAL_MS = 50
STABLE_POLLS = 11
TIMEOUT_MS = 6000
# 结束回合要等敌方整个回合演完，实测单个动作就要 400ms 上下，给足余量
END_TURN_TIMEOUT_MS = 20000

VERSION = 1


# --------------------------------------------------------------------------
# HTTP
# --------------------------------------------------------------------------


def _request(method: str, url: str, body: dict | None = None, timeout: float = 10.0) -> dict:
    data = None
    headers = {"Accept": "application/json"}
    if body is not None:
        data = json.dumps(body).encode("utf-8")
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    with urllib.request.urlopen(req, timeout=timeout) as r:
        raw = r.read().decode("utf-8")
    if not raw.strip():
        return {}
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        # 动作接口有时回纯文本确认消息，不是错误
        return {"_text": raw}


def get_state() -> dict:
    return _request("GET", SP_URL + "?format=json")


def post_action(body: dict) -> dict:
    return _request("POST", SP_URL, body)


def wait_for_game(timeout_s: float = 180.0) -> None:
    """轮询直到 mod 的 HTTP 端点应答。游戏冷启动要一会儿。"""
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        try:
            get_state()
            return
        except (urllib.error.URLError, OSError):
            time.sleep(1.0)
    raise SystemExit(f"错误：{SP_URL} 在 {timeout_s:.0f}s 内没有应答，游戏没起来或没装 mod")


# --------------------------------------------------------------------------
# 观测归一化
# --------------------------------------------------------------------------

# 战斗类 state_type。hand_select 是战斗中开着选牌界面。
COMBAT_STATES = {"monster", "elite", "boss", "hand_select"}


def _status_map(status_list) -> dict:
    """BuildPowersState 的列表 -> {power_id: amount}。"""
    out: dict[str, int] = {}
    for p in status_list or []:
        pid = p.get("id")
        if pid is None:
            continue
        amt = p.get("amount")
        # DisplayAmount 对无层数的 power 可能是 null，用 1 表示"存在"
        out[str(pid)] = int(amt) if isinstance(amt, (int, float)) else 1
    return out


def _hand_card(c: dict) -> dict:
    return {
        "slot": c.get("index"),
        "id": c.get("id"),
        "name": c.get("name"),
        "cost": c.get("cost"),
        "type": c.get("type"),
        "rarity": c.get("rarity"),
        "upgraded": bool(c.get("is_upgraded")),
        # **附魔**（2026-09-01 给 mod 加的字段）。不带它的话，一张带「灵巧」的
        # 耸肩无视在观测里只有 `description` 写着 10、而卡表说 8，
        # 内核照 id 模拟就会静默用错数值 —— 骇鳗那一场的帧 0 就是这么红的。
        # 老 trace 没有这个键，读的时候按 None 处理。
        "enchantment": c.get("enchantment"),
        "can_play": c.get("can_play"),
        "target_type": c.get("target_type"),
        "description": c.get("description"),
    }


def _pile_card(c: dict) -> dict:
    # 牌堆里的牌没有 id / is_upgraded（见 trace-format.md 约束 3），弱身份。
    # **附魔是例外**：它是逐实例的、名字里看不出来（带灵巧的耸肩无视给 10 点
    # 而卡表说 8），所以牌堆里也要留着 —— 从弃牌堆捞牌的头槌/好勇斗狠不留它
    # 就会捞回一张没附魔的。老 mod 没这个键，留 None。
    return {"name": c.get("name"), "cost": c.get("cost"), "enchantment": c.get("enchantment")}


def _pending(raw: dict) -> dict | None:
    """选牌界面。战斗内是 hand_select，overlay 是 card_select。"""
    for key in ("hand_select", "card_select"):
        blk = raw.get(key)
        if isinstance(blk, dict):
            cards = blk.get("cards") or blk.get("hand") or []
            return {
                "kind": key,
                "prompt": blk.get("prompt") or blk.get("instructions_title"),
                "can_confirm": blk.get("can_confirm"),
                "can_cancel": blk.get("can_cancel"),
                "selected": blk.get("selected_cards"),
                "candidates": [_hand_card(c) for c in cards if isinstance(c, dict)],
            }
    return None


def normalize(raw: dict) -> dict:
    """完整状态 -> trace 里的 Obs。只留内核有对应概念的字段 + 身份信息。"""
    player = raw.get("player") or {}
    battle = raw.get("battle") or {}

    enemies: dict[str, dict] = {}
    for e in battle.get("enemies") or []:
        # combat_id 是稳定键；下标会因为敌人死亡而平移（约束 1）
        cid = e.get("combat_id")
        key = str(cid) if cid is not None else str(e.get("entity_id"))
        enemies[key] = {
            "entity_id": e.get("entity_id"),
            "name": e.get("name"),
            "hp": e.get("hp"),
            "max_hp": e.get("max_hp"),
            "block": e.get("block"),
            "status": _status_map(e.get("status")),
            "intents": [
                {"type": i.get("type"), "label": i.get("label"), "description": i.get("description")}
                for i in (e.get("intents") or [])
            ],
        }

    return {
        "state_type": raw.get("state_type"),
        "round": battle.get("round"),
        "side": battle.get("turn"),
        "is_play_phase": battle.get("is_play_phase"),
        "energy": player.get("energy"),
        "max_energy": player.get("max_energy"),
        "player": {
            "hp": player.get("hp"),
            "max_hp": player.get("max_hp"),
            "block": player.get("block"),
            "status": _status_map(player.get("status")),
            # 药水**槽位数**。这一局之内就会变（药水腰带 +2 / 炼金宝匣 +4 /
            # 药瓶皮套 +1），而且高进阶把初始值从 3 改成 2 —— 进阶等级不在
            # 战斗观测里，所以**不能从遗物反推**，必须照抄游戏报的这个数。
            # 内核读它进 `State::potion_slots`；缺了会退回"最大槽位号 + 1"。
            "max_potion_slots": player.get("max_potion_slots"),
        },
        "enemies": enemies,
        "hand": [_hand_card(c) for c in (player.get("hand") or [])],
        "potions": [
            {
                "slot": p.get("slot"),
                "id": p.get("id"),
                "name": p.get("name"),
                "description": p.get("description"),
            }
            for p in (player.get("potions") or [])
        ],
        # 遗物：**只记身份，不记描述**。描述是渲染文本（会带计数器/状态），
        # 而内核认的是 id。见 `content::RELICS`。
        "relics": [
            {"id": r.get("id"), "name": r.get("name"), "counter": r.get("counter")}
            for r in (player.get("relics") or [])
        ],
        "draw_count": player.get("draw_pile_count"),
        # 抽牌堆的**内容**。顺序在 mod 那边已经按稀有度+id 排过了
        # （trace-format 约束 2），所以**只有内容可信，顺序不可信** ——
        # 内核那边会重新洗一次，当成 determinization 的一个采样。
        #
        # 一直有 `draw_count`，但只有张数；rollout 需要的是内容，
        # 而内容一直就在原始观测里，是这一步归一化把它丢了。
        # 和 `max_potion_slots` 是同一个模式：**别以为拿不到，先去看 raw**。
        "draw": [_pile_card(c) for c in (player.get("draw_pile") or [])],
        # **真实牌序**（2026-08-29 起）。`draw_pile` 那个字段在 mod 里被按
        # 稀有度+id 重排过（约束 2），真实顺序被扔掉了；`draw_pile_order` 是
        # 本地给 mod 打的补丁加的，**下标 0 是牌堆顶**
        # （[源码] `CardPileCmd.Draw` 取 `drawPile.Cards.FirstOrDefault()`，
        #  `CardPile.MoveToTopInternal` 是 `_cards.Insert(0, card)`）。
        #
        # 拿到它，内核最大的结构性不确定（"抽牌只猜顺序"）对**已知前缀**消失。
        # 老 mod / 老 trace 没有这个键 —— 那时是 `None`，内核退回原来的行为。
        "draw_order": player.get("draw_pile_order"),
        # 和 `draw_order` **逐位置配对**的附魔（同长度、同下标，没附魔是 null）。
        # 挂在 `draw_order` 上而不是 `draw` 上，因为后者被 mod 按稀有度+id 重排过
        # （约束 2），下标已经不指向同一张实体牌了。老 mod / 老 trace 是 None。
        "draw_order_enchant": player.get("draw_pile_order_enchantments"),
        "discard": [_pile_card(c) for c in (player.get("discard_pile") or [])],
        "exhaust": [_pile_card(c) for c in (player.get("exhaust_pile") or [])],
        "pending": _pending(raw),
    }


def run_info(raw: dict) -> dict:
    run = raw.get("run") or {}
    player = raw.get("player") or {}
    return {
        "act": run.get("act"),
        "floor": run.get("floor"),
        "ascension": run.get("ascension"),
        "character": player.get("character"),
        "room_type": raw.get("state_type"),
    }


# --------------------------------------------------------------------------
# settle：等结算完毕，并把等待过程本身记成证据
# --------------------------------------------------------------------------


def _control_returned(obs: dict) -> bool:
    """控制权是否回到玩家手上。

    敌方回合中，敌人两次出招之间的停顿可以轻松超过 stable_polls×interval，
    光看"状态连续几次不变"会在敌方回合**中途**就判定结算完毕——实录第一次
    结束回合就踩了这个坑，录下了一帧 side=enemy 的半截状态。
    所以结束回合必须额外等到玩家能操作为止。

    战斗结束（进入奖励/游戏结束）也算控制权回来了，否则会一直等到超时。
    """
    if obs.get("state_type") not in COMBAT_STATES:
        return True
    return bool(obs.get("is_play_phase")) and obs.get("side") == "player"


def settle(want_play_phase: bool = False, timeout_ms: int = TIMEOUT_MS) -> tuple[dict, dict, dict]:
    """轮询到状态稳定。返回 (归一化观测, settle 证据, 原始状态)。

    `intermediate_differs` 是"存在动画中途状态"的直接证据——它决定了
    对拍到底能不能信 POST 之后立刻读到的状态。**实测为真**，所以这个等待
    不是保险，是必需品。

    `want_play_phase=True` 时，还要求控制权已回到玩家（见 `_control_returned`）。
    """
    start = time.time()
    deadline = start + timeout_ms / 1000.0
    interval = POLL_INTERVAL_MS / 1000.0

    first_obs: dict | None = None
    last_obs: dict | None = None
    last_raw: dict = {}
    stable = 0
    polls = 0

    while time.time() < deadline:
        raw = get_state()
        obs = normalize(raw)
        polls += 1
        if first_obs is None:
            first_obs = obs
        if last_obs is not None and obs == last_obs and (not want_play_phase or _control_returned(obs)):
            stable += 1
            if stable >= STABLE_POLLS - 1:
                last_raw = raw
                break
        else:
            stable = 0
        last_obs = obs
        last_raw = raw
        time.sleep(interval)

    elapsed_ms = int((time.time() - start) * 1000)
    unstable = stable < STABLE_POLLS - 1
    evidence = {
        "polls": polls,
        "ms": elapsed_ms,
        "unstable": unstable,
        "intermediate_differs": first_obs != last_obs,
    }
    return (last_obs or {}), evidence, last_raw


# --------------------------------------------------------------------------
# trace 文件
# --------------------------------------------------------------------------


def load_trace(path: str) -> dict | None:
    if not os.path.exists(path):
        return None
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def save_trace(path: str, trace: dict) -> None:
    d = os.path.dirname(os.path.abspath(path))
    if d:
        os.makedirs(d, exist_ok=True)
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(trace, f, ensure_ascii=False, indent=1)
    os.replace(tmp, path)


def new_trace(raw: dict) -> dict:
    return {
        "version": VERSION,
        "recorded_at": _dt.datetime.now().astimezone().isoformat(timespec="seconds"),
        "run": run_info(raw),
        "settle": {
            "poll_interval_ms": POLL_INTERVAL_MS,
            "stable_polls": STABLE_POLLS,
            "timeout_ms": TIMEOUT_MS,
        },
        "frames": [],
        "notes": "",
    }


def save_raw(raw_dir: str | None, i: int, raw: dict) -> None:
    if not raw_dir:
        return
    os.makedirs(raw_dir, exist_ok=True)
    with open(os.path.join(raw_dir, f"{i:04d}.json"), "w", encoding="utf-8") as f:
        json.dump(raw, f, ensure_ascii=False, indent=1)


# --------------------------------------------------------------------------
# 打印：让人/agent 一眼看懂现在该出什么
# --------------------------------------------------------------------------


def _fmt_status(st: dict) -> str:
    return " ".join(f"{k}={v}" for k, v in sorted(st.items())) or "-"


def show(obs: dict) -> None:
    if obs.get("state_type") not in COMBAT_STATES:
        print(f"[非战斗] state_type={obs.get('state_type')}")
        return
    p = obs["player"]
    print(
        f"回合 {obs.get('round')} / {obs.get('side')} "
        f"play_phase={obs.get('is_play_phase')}  能量 {obs.get('energy')}/{obs.get('max_energy')}"
    )
    print(f"  我: HP {p['hp']}/{p['max_hp']}  格挡 {p['block']}  状态 {_fmt_status(p['status'])}")
    for cid, e in obs["enemies"].items():
        intents = ", ".join(f"{i['type']}:{i['label']}" for i in e["intents"]) or "-"
        print(
            f"  敌[{cid}] {e['name']}({e['entity_id']}): HP {e['hp']}/{e['max_hp']} "
            f"格挡 {e['block']} 状态 {_fmt_status(e['status'])} 意图 {intents}"
        )
    for c in obs["hand"]:
        mark = "*" if c["upgraded"] else " "  # 卡名本身已带 +，再补一个会看成双升级
        playable = "" if c["can_play"] else "  <不可出>"
        print(f"  手[{c['slot']}] {c['cost']}费 {c['name']}{mark} ({c['id']}){playable}")
    print(
        f"  抽 {obs.get('draw_count')}  弃 {len(obs.get('discard') or [])}  "
        f"消耗 {len(obs.get('exhaust') or [])}"
    )
    for p in obs.get("potions") or []:
        print(f"  药[{p['slot']}] {p['name']} ({p['id']})")
    if obs.get("pending"):
        pend = obs["pending"]
        print(f"  !! 选牌界面: {pend.get('prompt')} 可确认={pend.get('can_confirm')}")
        for c in pend.get("candidates") or []:
            print(f"     候选[{c['slot']}] {c['name']} ({c['id']})")


def warn_settle(ev: dict) -> None:
    if ev["unstable"]:
        print(
            f"  !! 警告：{ev['ms']}ms 内状态没稳定下来，这一帧不可信 "
            f"(polls={ev['polls']})",
            file=sys.stderr,
        )
    elif ev["intermediate_differs"]:
        print(
            f"  ~ note: 观察到动画中途状态（首次轮询 != 稳定态），"
            f"settle 生效，耗时 {ev['ms']}ms",
            file=sys.stderr,
        )


# --------------------------------------------------------------------------
# 主流程
# --------------------------------------------------------------------------


def refresh_last_obs(trace: dict) -> dict:
    """用当前实际状态校正最后一帧的观测，返回它。

    必须在校验槽位**之前**调用：录制器之外发生过的事（或上一次 settle 提前
    返回留下的半截状态）会让最后一帧的手牌和现实对不上，拿它去校验槽位会
    误判成"trace 损坏"。
    """
    frames = trace["frames"]
    if not frames:
        raise SystemExit("错误：trace 还没 init，先跑 `init`")
    last = frames[-1]
    pre = normalize(get_state())
    if pre != last["obs"]:
        print(
            "  !! 上一帧观测与实际不符，已用实际状态覆盖"
            "（上次结算提前返回，或有动作绕过了录制器）",
            file=sys.stderr,
        )
        last["obs"] = pre
    return pre


def _in_choose_screen() -> bool:
    """当前是不是「从生成出来的候选里选一张」那种界面。

    判据取 mod 自己报的 `state_type`（`McpMod.StateBuilder` 里
    `result["card_select"] = BuildChooseCardState(...)` 那一支），
    不去猜 —— 猜错的后果就是上面那条注释里记的：动作没生效还报成功。

    **2026-08-31 一次错误的"修复"记在这里，别再犯**：涅奥之怒的
    「从弃牌堆挑 2 张」(`NCombatPileCardSelectScreen`) 选不动，我当时读
    `ExecuteSelectCard` 只看见 `NCardGridSelectionScreen` /
    `NChooseACardSelectionScreen` 两个分支，就断定它不认这个屏，
    把判据改成"带 Pile 的走 combat_select_card"。**两处都错了**：
    `NCombatPileCardSelectScreen` **继承自** `NCardGridSelectionScreen`，
    类型判断本来就匹配；而 `combat_select_card` 要求
    `NPlayerHand.IsInCardSelection`，那才是真的不成立。
    真正的原因见 `do_action` 里那段关于**选中状态不可观测**的注释。
    """
    try:
        return (get_state() or {}).get("state_type") == "card_select"
    except Exception:
        return False


def do_action(
    trace: dict,
    action: dict,
    body: dict,
    raw_dir: str | None,
    want_play_phase: bool = False,
    timeout_ms: int = TIMEOUT_MS,
) -> dict:
    """执行一个动作：把它填进最后一帧，等结算，追加新帧。"""
    frames = trace["frames"]
    if not frames:
        raise SystemExit("错误：trace 还没 init，先跑 `init`")
    last = frames[-1]
    if last["action"] is not None:
        raise SystemExit("错误：最后一帧已经有动作了，trace 状态损坏")

    # 出手前确认游戏还停在我们以为的地方；漂了说明有人在录制器之外动过游戏
    pre_raw = get_state()
    pre = normalize(pre_raw)
    if pre != last["obs"]:
        print(
            "  !! 警告：出手前的实际状态和上一帧记录的不一致——"
            "有动作绕过了录制器。已用实际状态覆盖该帧。",
            file=sys.stderr,
        )
        last["obs"] = pre

    resp = post_action(body)
    if isinstance(resp, dict) and isinstance(resp.get("_text"), str):
        txt = resp["_text"].strip()
        if txt.lower().startswith("error"):
            raise SystemExit(f"游戏拒绝了这个动作: {txt}")
    # **mod 自己会说成没成**（`{"status":"ok"}` / `{"status":"error","error":…}`，
    # [源码] `McpMod.Helpers.Error`）。这个信号比"观测变没变"可靠得多 ——
    # 下面那条差分判据只是它的兜底。
    if isinstance(resp, dict) and resp.get("status") == "error":
        raise SystemExit(f"游戏拒绝了这个动作: {resp.get('error')}")
    _mod_said_ok = isinstance(resp, dict) and resp.get("status") == "ok"

    obs, ev, raw = settle(want_play_phase, timeout_ms)

    # 游戏**默默拒绝**一个动作时，mod 不一定回 error 文本，状态就原封不动。
    # 实测（2026-08-17，第2幕31层）：指向性攻击牌漏了 `--target`，游戏拒收，
    # 而录制器照样报「全部成功」，一整回合 47 点伤害只落地 4 点，
    # 后面两步还打到了别的牌上。
    #
    # 判据：**打出一张牌必然改变状态**（那张牌至少要离开手牌）。
    # 状态一个字节没变 = 这一步没生效，必须当场喊停，不能记进 trace ——
    # 记进去还会让 verify 把它报成内核算错。
    # 2026-08-22 扩到选牌/确认：攻击药水那种「从 3 张里选 1 张」的界面
    # 走的是 `select_card` / `confirm_selection`，而录制器原来写死了
    # `combat_select_card`（那是**从手牌里选**，烙印用的）。端点不对时
    # mod 回的是 error 文本，但状态一个字节没变，于是录制器照报「全部成功」，
    # 往 trace 里塞了两个**从未发生过的动作**。判据和出牌同一条。
    # **网格选牌屏上"选中"这件事是不可观测的**（2026-08-31 查清）：
    # mod 的 `BuildCardSelectState` 不报 `selected_cards`（那个字段只给
    # 手牌选择屏构造，见 [源码] `McpMod.StateBuilder`），而 `select_card`
    # 在这类屏上只是 `EmitSignal(HolderPressed)` 切换选中态 ——
    # **动作成功了，观测却一个字不变**。
    #
    # 涅奥之怒那次就是被下面这条差分判据误判成"被拒"的，而我照着那个假阴性
    # 去改端点（改成 `combat_select_card`），改出了一个真的错误 ——
    # **假阴性引出的修复比原问题更糟**，这条教训值这几行注释。
    #
    # 正确的判据是 mod 自己回的 `status`（上面 `_mod_said_ok`）：
    # 它明确说了成没成，不用去猜观测该不该变。差分只在 mod **没**给出
    # 明确成功信号时兜底 —— 那才是"默默拒绝"真正会发生的地方。
    # **豁免只给 `select_card`**：出牌那条差分判据必须留着。
    # 2026-08-17 那个坑（指向性攻击牌漏 `--target`，游戏拒收而 mod 没回 error）
    # 正是靠它抓住的 —— 把豁免开给 `play_card` 等于把那道防线拆了。
    if _mod_said_ok and action.get("kind") == "select_card" and obs == last["obs"]:
        pass
    elif action.get("kind") in ("play_card", "select_card", "confirm_selection") and obs == last["obs"]:
        raise SystemExit(
            f"游戏拒绝了这个动作（状态没有任何变化）：{action}。"
            f"  出牌最常见的原因是**指向性攻击牌没给 --target**；"
            f"选牌最常见的原因是**选牌界面的种类和用的端点对不上**"
            f"（手牌内选 combat_select_card / 生成候选里选 select_card）。"
            f"这一步没有记进 trace。"
        )

    last["action"] = action
    last["settle"] = ev
    frames.append({"i": len(frames), "obs": obs, "action": None})
    save_raw(raw_dir, len(frames) - 1, raw)
    warn_settle(ev)
    return obs


def _add_steps(sub) -> None:
    """一个「步骤」的语法。命令行和计划文件共用同一套，避免两处漂移。"""
    sub.add_parser("state", help="只打印当前状态，不记录、不动作")
    sub.add_parser("init", help="开一条新 trace，记下第一帧观测")
    p_play = sub.add_parser("play", help="出牌")
    p_play.add_argument("slot", type=int)
    p_play.add_argument("--target", help="敌人 entity_id，如 jaw_worm_0")
    sub.add_parser("end", help="结束回合")
    p_sel = sub.add_parser("select", help="选牌界面选一张")
    p_sel.add_argument("slot", type=int)
    sub.add_parser("confirm", help="确认选择")
    p_pot = sub.add_parser("potion", help="喝药水")
    p_pot.add_argument("slot", type=int)
    p_pot.add_argument("--target", help="敌人 entity_id，敌方目标药水才需要")
    sub.add_parser("finish", help="收尾：补一次观测并落盘")
    # 下面三个只在计划模式里有意义：命令行版本用 --trace / --raw-dir 传
    p_tr = sub.add_parser("trace", help="[计划] 设定后续步骤写进哪条 trace")
    p_tr.add_argument("path")
    p_rd = sub.add_parser("raw-dir", help="[计划] 设定原始状态旁路目录")
    p_rd.add_argument("path")
    # 正文整行照抄进来（见 `read_plan`），所以这里是单个位置参数而不是
    # `nargs="+"` —— 后者会把正文里的 `-1` 当成选项
    p_note = sub.add_parser("note", help="[计划] 追加一行 notes 到 trace")
    p_note.add_argument("text")


class Ctx:
    """一次运行里跨步骤共享的东西。trace 只在内存里改，每步结束落盘。"""

    def __init__(self, trace_path: str | None = None, raw_dir: str | None = None):
        self.trace_path = trace_path
        self.raw_dir = raw_dir
        self.trace: dict | None = None

    def need_trace(self) -> dict:
        if self.trace_path is None:
            raise SystemExit("错误：还没设定 trace 路径（计划里加一行 `trace <路径>`）")
        if self.trace is None:
            self.trace = load_trace(self.trace_path)
        if self.trace is None:
            raise SystemExit(f"错误：{self.trace_path} 不存在，先跑 `init`")
        return self.trace

    def save(self) -> None:
        if self.trace is not None and self.trace_path is not None:
            save_trace(self.trace_path, self.trace)


def exec_step(ctx: Ctx, args) -> None:
    """执行一个步骤。失败一律 raise SystemExit（带能定位问题的中文原因）。"""
    cmd = args.cmd

    if cmd == "trace":
        ctx.trace_path = resolve(args.path)
        ctx.trace = None
        print(f"  trace -> {ctx.trace_path}")
        return
    if cmd == "raw-dir":
        ctx.raw_dir = resolve(args.path)
        print(f"  raw-dir -> {ctx.raw_dir}")
        return
    if cmd == "note":
        trace = ctx.need_trace()
        line = args.text
        trace["notes"] = (trace["notes"] + "\n" + line).strip() if trace["notes"] else line
        ctx.save()
        print(f"  note: {line}")
        return

    if cmd == "state":
        show(normalize(get_state()))
        return

    if cmd == "init":
        if ctx.trace_path is None:
            raise SystemExit("错误：还没设定 trace 路径（计划里加一行 `trace <路径>`）")
        raw = get_state()
        obs = normalize(raw)
        if obs.get("state_type") not in COMBAT_STATES:
            raise SystemExit(
                f"错误：现在不在战斗里 (state_type={obs.get('state_type')})，没什么可录的"
            )
        ctx.trace = new_trace(raw)
        ctx.trace["frames"].append({"i": 0, "obs": obs, "action": None})
        save_raw(ctx.raw_dir, 0, raw)
        ctx.save()
        print(f"开录 -> {ctx.trace_path}")
        show(obs)
        return

    trace = ctx.need_trace()

    if cmd == "finish":
        obs, ev, raw = settle(want_play_phase=True, timeout_ms=END_TURN_TIMEOUT_MS)
        trace["frames"][-1]["obs"] = obs
        save_raw(ctx.raw_dir, len(trace["frames"]) - 1, raw)
        ctx.save()
        n_act = sum(1 for f in trace["frames"] if f["action"])
        print(f"收尾：{len(trace['frames'])} 帧，{n_act} 个动作 -> {ctx.trace_path}")
        show(obs)
        return

    if cmd == "play":
        hand = refresh_last_obs(trace).get("hand") or []
        card = next((c for c in hand if c["slot"] == args.slot), None)
        if card is None:
            raise SystemExit(f"错误：手牌里没有第 {args.slot} 位（共 {len(hand)} 张）")
        action = {
            "kind": "play_card",
            "slot": args.slot,
            "card_id": card["id"],
            "card_name": card["name"],
            "target": args.target,
        }
        body = {"action": "play_card", "card_index": args.slot}
        if args.target:
            body["target"] = args.target
        obs = do_action(trace, action, body, ctx.raw_dir)
    elif cmd == "end":
        # 敌方整个回合都要跑完，比出牌慢得多，且必须等控制权回到玩家
        obs = do_action(
            trace,
            {"kind": "end_turn"},
            {"action": "end_turn"},
            ctx.raw_dir,
            want_play_phase=True,
            timeout_ms=END_TURN_TIMEOUT_MS,
        )
    elif cmd == "select":
        # **选牌有两种界面，端点不一样**，靠当前 `state_type` 分：
        #   * `card_select`（screen_type=choose）：从**生成出来的候选**里选一张
        #     —— 攻击药水/技能药水那类，走 `select_card`
        #   * 其余（战斗中手牌选择，如烙印/武装）：走 `combat_select_card`
        #     （mod 那边要求 `NPlayerHand.Instance.IsInCardSelection`）
        # 2026-08-22 第2幕 Boss 用攻击药水时踩到：写死 combat_select_card，
        # mod 回 error、状态没变，而录制器报了成功。
        body = (
            {"action": "select_card", "index": args.slot}  # 注意：这个端点的参数叫 index，不是 card_index
            if _in_choose_screen()
            else {"action": "combat_select_card", "card_index": args.slot}
        )
        obs = do_action(trace, {"kind": "select_card", "slot": args.slot}, body, ctx.raw_dir)
    elif cmd == "confirm":
        body = (
            {"action": "confirm_selection"}
            if _in_choose_screen()
            else {"action": "combat_confirm_selection"}
        )
        obs = do_action(trace, {"kind": "confirm_selection"}, body, ctx.raw_dir)
    elif cmd == "potion":
        # 药水在内核里完全没建模，对拍会把这一帧记成"内容缺失"而不是算错。
        # 录它是为了拿到那些只有药水能造出来的局面（力量、大额格挡）。
        pot = next(
            (p for p in refresh_last_obs(trace).get("potions") or [] if p["slot"] == args.slot),
            None,
        )
        action = {
            "kind": "use_potion",
            "slot": args.slot,
            "potion_id": pot["id"] if pot else None,
            "potion_name": pot["name"] if pot else None,
            "target": args.target,
        }
        body = {"action": "use_potion", "slot": args.slot}
        if args.target:
            body["target"] = args.target
        obs = do_action(trace, action, body, ctx.raw_dir)
    else:
        raise SystemExit(f"未知步骤 {cmd}")

    ctx.save()
    run_verify_last(ctx.trace_path, mode="driven")
    show(obs)


# --------------------------------------------------------------------------
# 计划模式
# --------------------------------------------------------------------------


def read_plan(path: str) -> list[tuple[int, str, list[str]]]:
    """计划文件 -> [(行号, 原文, 分词)]。`#` 起头和空行跳过。"""
    if not os.path.exists(path):
        raise SystemExit(f"错误：计划文件不存在：{path}")
    with open(path, encoding="utf-8") as f:
        text = f.read()
    steps = []
    for lineno, line in enumerate(text.splitlines(), 1):
        s = line.strip()
        if not s or s.startswith("#"):
            continue
        # `note` 的文本**整行照抄，不分词**。
        #
        # 走 shlex + argparse 的话，正文里一个 `-1`（"HP 精确 -1"）会被当成
        # 选项，整条计划中止 —— 实录时踩过一次。注释是自由文本，本来就不该
        # 被解析；引号、破折号、负数都得能原样写进去。
        if s == "note" or s.startswith("note "):
            steps.append((lineno, s, ["note", s[4:].strip()]))
            continue
        try:
            toks = shlex.split(s)
        except ValueError as exc:  # 引号没配对
            raise SystemExit(f"错误：计划第 {lineno} 行分词失败（{exc}）：{s}")
        if toks:
            steps.append((lineno, s, toks))
    return steps


def rewrite_plan(path: str, ctx: Ctx, done: int, steps, failed_at: int | None) -> None:
    """跑完后重写计划：保留 trace/raw-dir 设定，其余全注释掉。

    这样「同一条命令」重复执行是空操作，不会因为手滑重放一遍动作。
    """
    out = [
        "# 由 record_trace.py --plan 在执行后自动重写。",
        f"# {_dt.datetime.now().astimezone().isoformat(timespec='seconds')}："
        + (f"{done} 步成功" if failed_at is None else f"{done} 步成功后，第 {failed_at} 步失败"),
        "# 下面的动作行已被注释；要继续录制请重写本文件。",
    ]
    if ctx.trace_path:
        out.append(f"trace {relpath(ctx.trace_path)}")
    if ctx.raw_dir:
        out.append(f"raw-dir {relpath(ctx.raw_dir)}")
    for idx, (_lineno, src, toks) in enumerate(steps, 1):
        if toks[0] in ("trace", "raw-dir"):
            continue
        mark = "#!" if idx == failed_at else ("#." if idx <= done else "#?")
        out.append(f"{mark} {src}")
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(out) + "\n")


def run_plan(path: str) -> int:
    steps = read_plan(path)
    ctx = Ctx()
    if not steps:
        print(f"计划文件里没有可执行的步骤：{path}")
        return 0

    n = len(steps)
    done = 0
    failed_at = None
    reason = ""
    parser = build_step_parser()
    for idx, (lineno, src, toks) in enumerate(steps, 1):
        print(f"[{idx}/{n}] (第 {lineno} 行) {src}")
        try:
            args = parser.parse_args(toks)
            exec_step(ctx, args)
        except SystemExit as exc:
            # argparse 的用法错误也走这里（code=2），照样能报出是第几步
            if exc.code in (None, 0):
                failed_at, reason = idx, "该步骤请求了退出"
            else:
                failed_at, reason = idx, str(exc.code) if not isinstance(exc.code, int) else "见上面的错误行"
            break
        except Exception as exc:  # 网络/JSON 之类
            failed_at, reason = idx, f"{type(exc).__name__}: {exc}"
            break
        done += 1

    rewrite_plan(path, ctx, done, steps, failed_at)
    if failed_at is not None:
        lineno, src, _ = steps[failed_at - 1]
        print(
            f"\n!! 计划中止：第 {failed_at}/{n} 步失败（计划文件第 {lineno} 行）\n"
            f"   步骤原文：{src}\n"
            f"   原因：{reason}\n"
            f"   已成功执行 {done} 步，trace 已落盘到 {ctx.trace_path}",
            file=sys.stderr,
        )
        return 1
    print(f"\n计划执行完毕：{done}/{n} 步全部成功")
    return 0


def build_step_parser() -> argparse.ArgumentParser:
    ap = argparse.ArgumentParser(prog="step", add_help=False)
    sub = ap.add_subparsers(dest="cmd", required=True)
    _add_steps(sub)
    return ap


def main() -> None:
    ap = argparse.ArgumentParser(description="录制 STS2 对拍 trace")
    ap.add_argument("--trace", help="trace JSON 路径（非计划模式下必填）")
    ap.add_argument("--raw-dir", help="同时把每帧原始状态存到这个目录")
    ap.add_argument("--wait", action="store_true", help="先等游戏 HTTP 端点起来")
    ap.add_argument(
        "--plan",
        nargs="?",
        const=PLAN_PATH,
        help=f"计划模式：按行执行计划文件里的步骤（默认 {PLAN_PATH}）",
    )
    sub = ap.add_subparsers(dest="cmd", required=False)
    _add_steps(sub)

    args = ap.parse_args()
    if args.wait:
        wait_for_game()

    if args.plan:
        if args.cmd:
            raise SystemExit("错误：--plan 和子命令不能同时用，步骤写进计划文件里")
        raise SystemExit(run_plan(resolve(args.plan)))

    if not args.cmd:
        ap.error("要么给一个子命令，要么用 --plan")
    if args.cmd in ("trace", "raw-dir", "note"):
        ap.error(f"`{args.cmd}` 只在计划模式里有意义")
    if not args.trace:
        ap.error("--trace 是必填的（或者用 --plan）")
    exec_step(Ctx(resolve(args.trace), args.raw_dir), args)


if __name__ == "__main__":
    main()
