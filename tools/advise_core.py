#!/usr/bin/env python3
r"""L3 构筑顾问的**脏活那一半**：读实况 -> 拼一份请求 -> 交给 `advise.exe`。

内核那一侧（`src/bin/advise.rs`）只认一份 JSON：牌组、遗物、血量、这一幕、
一条路线、几个候选。**把游戏的实况翻译成那份 JSON 就是这里的全部工作** ——
和 `solve_now.py` 对 `solve --live` 的分工一模一样，理由也同一条：
内核不该认识 HTTP，Python 不该实现任何游戏规则。

    游戏 ──HTTP──► 本模块 ──JSON/stdin──► advise.exe ──► 一份人读的报告

只用标准库（`record_trace` 的 HTTP 那一份），所以 MCP server 挂不挂都能跑：

    & "D:\game mod\sts2sim\.venv\Scripts\python.exe" "D:\game mod\sts2core\tools\advise_core.py" deck
    & "D:\game mod\sts2sim\.venv\Scripts\python.exe" "D:\game mod\sts2core\tools\advise_core.py" act
    & "D:\game mod\sts2sim\.venv\Scripts\python.exe" "D:\game mod\sts2core\tools\advise_core.py" reward

## 牌组从哪来：**mod 的第四个本地补丁**

`player.deck` 是主牌组，**每个界面都报**（父目录 `CLAUDE.md` 的补丁表）。
在它之前，牌组只在战斗里看得到，于是「拿牌 / 移除 / 升级」这些**全都发生在
战斗外**的决策只能靠一份出手就过期的缓存。所以这里**优先读 `player.deck`**，
读不到才退回"手牌 + 三个牌堆"（战斗中才凑得齐），两条都没有就**报错而不是
拿旧缓存顶上** —— 一份过期的牌组会给出一个自信的错答案。

## 「这是哪一幕」为什么要缓存

内核的遭遇表按**幕名**索引（`Underdocks` / `Overgrowth` / `Hive` / `Glory`），
而 `run.act` 只有 1/2/3 —— **第 1 幕有两个**（Overgrowth 和 Underdocks 同序号）。
唯一能把它们分开的是**地图屏报的 Boss id**（`map.boss.id`，[源码]
`ActModel.BossEncounter`），而卡牌奖励屏上没有地图块。

所以：看得见地图就记一份到 `traces/_advise_ctx.json`，看不见就用那份缓存
（`run.act` 一变就作废）。第 2/3 幕靠序号就认得出，不需要缓存。
**认不出来就说认不出来**，别挑一个自洽的幕名。
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import record_trace as rec  # noqa: E402  复用录制器的 HTTP（只用标准库）

ROOT = rec.ROOT
CTX_PATH = os.path.join(ROOT, "traces", "_advise_ctx.json")
ENCOUNTERS_PATH = os.path.join(ROOT, "data", "encounters.json")

ADVISE_EXE = os.path.join(ROOT, "target", "release", "advise.exe")
if not os.path.exists(ADVISE_EXE):  # 非 Windows 兜底
    ADVISE_EXE = os.path.join(ROOT, "target", "release", "advise")

# 默认路线。**和 `bin/act_eval::DEFAULT_ROOMS` / `bin/advise` 是同一个 `[判断]`**：
# 8 杂兵 + 1 精英 + 3 休息 + Boss。依据是 [源码] 每幕 13–15 间房，外加家规的
# 「休息处最多、杂兵最少」。**它不是实录**，所以每份报告都会把它印出来。
DEFAULT_ROOMS = "MMRMMMREMMRB"

COMBAT_STATES = getattr(rec, "COMBAT_STATES", ("battle", "combat"))


class AdviseError(RuntimeError):
    """拿不到评估所需的输入。**消息本身就是给用户看的**，别包一层。"""


# --------------------------------------------------------------------------
# 实况 -> 请求的几个字段
# --------------------------------------------------------------------------


def get_state() -> dict:
    try:
        return rec.get_state()
    except Exception as e:  # noqa: BLE001
        raise AdviseError(
            f"读不到游戏状态：{e}\n  游戏没开、或者 mod 的端点还没起来。启动流程见 D:\\game mod\\CLAUDE.md"
        ) from e


def _card(entry) -> dict:
    """一张牌 -> 请求里的那个对象。

    名字自带升级标记（`打击+`），附魔是**每张实例**的（mod 的第二、三个补丁），
    两样都原样传过去 —— 认牌名那一步在内核里（`synth::card_from_name`），
    这边一条规则都不写。
    """
    if isinstance(entry, str):
        return {"name": entry}
    name = entry.get("name") or ""
    ench = entry.get("enchantment") or {}
    out = {"name": name, "upgraded": bool(entry.get("is_upgraded", name.endswith("+")))}
    if ench.get("id"):
        out["enchant_id"] = ench.get("id")
        out["enchant_amount"] = int(ench.get("amount") or 0)
    return out


def deck_from_state(raw: dict) -> tuple[list[dict], str]:
    """主牌组。返回 (牌, 它是从哪读来的)。"""
    player = raw.get("player") or {}
    deck = player.get("deck")
    if deck:
        return [_card(c) for c in deck], "player.deck（mod 的牌组补丁）"
    # 退路：战斗中把四个牌区拼起来 —— 那就是这场仗开局的整副牌。
    piles = [player.get("hand"), player.get("draw_pile_order") or player.get("draw_pile"),
             player.get("discard_pile"), player.get("exhaust_pile")]
    cards = [_card(c) for bucket in piles for c in (bucket or [])]
    if cards:
        return cards, "战斗中的手牌+三个牌堆（mod 没报 player.deck）"
    raise AdviseError(
        "牌组读不出来：`player.deck` 没有，也不在战斗里。\n"
        "  装的 DLL 可能不带牌组补丁（父目录 CLAUDE.md 的第四个补丁）——\n"
        "  **不拿缓存顶上**：一份过期的牌组会给出一个自信的错答案。"
    )


def relics_from_state(raw: dict) -> list[dict]:
    out = []
    for r in (raw.get("player") or {}).get("relics") or []:
        if isinstance(r, dict) and r.get("id"):
            out.append({"id": r["id"], "counter": r.get("counter")})
    return out


def potions_from_state(raw: dict) -> tuple[list, int]:
    """按槽位排好的药水 id（空槽是 `None`）+ 槽位数。

    **槽号每次都要重读**（父目录 CLAUDE.md：喝掉一瓶之后槽号会重排）。
    """
    player = raw.get("player") or {}
    slots = int(player.get("max_potion_slots") or 3)
    out: list = [None] * max(slots, 3)
    for p in player.get("potions") or []:
        i = int(p.get("slot") or 0)
        if 0 <= i < len(out):
            out[i] = p.get("id") or p.get("name")
    return out, slots


# --------------------------------------------------------------------------
# 「这是哪一幕」
# --------------------------------------------------------------------------


def _encounters() -> dict:
    with open(ENCOUNTERS_PATH, encoding="utf-8") as f:
        return json.load(f)


def _act_of_boss(table: dict, boss_id: str) -> str | None:
    for name, act in (table.get("acts") or {}).items():
        if boss_id in (act.get("encounters") or []):
            return name
    return None


def _acts_by_index(table: dict, index: int) -> list[str]:
    return sorted(
        name for name, act in (table.get("acts") or {}).items() if act.get("index") == index
    )


def _load_ctx() -> dict:
    try:
        with open(CTX_PATH, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return {}


def _save_ctx(ctx: dict) -> None:
    try:
        with open(CTX_PATH, "w", encoding="utf-8") as f:
            json.dump(ctx, f, ensure_ascii=False, indent=1)
    except OSError:
        pass  # 缓存写不进去不该让评估失败


def act_context(raw: dict) -> dict:
    """这一幕叫什么、Boss 是谁、还剩几间房。

    三个来源，可信度递减，**每份报告都印出用的是哪一个**：

    1. **地图屏**（`map.boss.id` + `current_position`）—— 当场读到的事实
    2. `traces/_advise_ctx.json` 里上一次看见地图时记的那份（`run.act` 一变就作废）
    3. 幕序号 —— 第 2/3 幕唯一，**第 1 幕认不出来**（Overgrowth / Underdocks 同序号）
    """
    run = raw.get("run") or {}
    act_no = int(run.get("act") or 0)
    table = _encounters()
    ctx = {
        "act": act_no,
        # 记下楼层：下一次看不见地图时，"还剩几间"要按走了几层往下减
        "floor": run.get("floor"),
        "act_name": None,
        "boss": None,
        "second_boss": None,
        "rooms_left": None,
        "source": None,
        "note": None,
    }

    m = raw.get("map") or {}
    boss = (m.get("boss") or {}).get("id")
    if boss:
        ctx["act_name"] = _act_of_boss(table, boss)
        ctx["boss"] = boss
        bosses = [b.get("id") for b in (m.get("bosses") or []) if b.get("id")]
        if len(bosses) > 1:
            ctx["second_boss"] = bosses[1]
        cur = m.get("current_position") or {}
        boss_row = (m.get("boss") or {}).get("row")
        if isinstance(boss_row, int) and isinstance(cur.get("row"), int):
            # 一行一间房：走到 Boss 那一行还要经过 `boss_row - 当前行` 间。
            ctx["rooms_left"] = max(boss_row - int(cur["row"]), 1)
        ctx["source"] = "地图屏（当场读的）"
        _save_ctx(ctx)
        return ctx

    # 缓存作废的两条：**换了一幕**，或者**楼层倒退**（那是新开了一局 ——
    # 楼层在一局之内只增不减）。少了第二条，上一局的幕名会安静地喂给这一局。
    cached = _load_ctx()
    floor = run.get("floor")
    fresh = cached.get("act") == act_no and cached.get("act_name")
    if fresh and isinstance(floor, int) and isinstance(cached.get("floor"), int):
        fresh = floor >= int(cached["floor"])
    if fresh:
        cached["source"] = "上一次看见地图时记的（traces/_advise_ctx.json）"
        # 缓存里的"还剩几间"会随着走路过期，**按楼层往下减**：
        # `run.floor` 每进一间涨 1。减不出来就留空，不猜。
        if isinstance(cached.get("rooms_left"), int) and isinstance(run.get("floor"), int):
            walked = int(run["floor"]) - int(cached.get("floor") or run["floor"])
            cached["rooms_left"] = max(int(cached["rooms_left"]) - max(walked, 0), 1)
        return cached

    names = _acts_by_index(table, act_no - 1)
    if len(names) == 1:
        ctx["act_name"] = names[0]
        ctx["source"] = f"幕序号（第 {act_no} 幕只有一个）"
    else:
        ctx["note"] = (
            f"**认不出是哪一幕**：第 {act_no} 幕有 {len(names)} 个（{', '.join(names)}），"
            "而序号分不开它们。到地图屏上再调一次（那时 Boss id 在状态里），"
            "或者显式传 act=。"
        )
    return ctx


def rooms_for(ctx: dict, rooms: str | None) -> tuple[str, str]:
    """这次评估走哪几间。返回 (路线, 它是怎么来的)。

    **路线是输入不是结论**（`synth::act` 模块头：局外决策，这一层不建）。
    调用方给了就用调用方的；没给就拿默认那条**按剩下几间截尾** ——
    截的是尾巴，因为末尾那间是 Boss，而"还剩几间"是从地图上数出来的。
    """
    if rooms:
        return rooms, "调用方给的"
    left = ctx.get("rooms_left")
    if isinstance(left, int) and 0 < left < len(DEFAULT_ROOMS):
        return DEFAULT_ROOMS[-left:], f"默认那条 `[判断]` 截到剩下的 {left} 间"
    return DEFAULT_ROOMS, "默认那条 `[判断]`（整幕）"


# --------------------------------------------------------------------------
# 请求
# --------------------------------------------------------------------------


def build_request(
    raw: dict,
    *,
    question: str = "act",
    candidates: list[dict] | None = None,
    rooms: str | None = None,
    act: str | None = None,
    samples: int | None = None,
    seed: int | None = None,
    encounter: str | None = None,
    enemies: list[str] | None = None,
    top: int = 0,
) -> tuple[dict, list[str]]:
    """实况 -> `advise.exe` 的请求。返回 (请求, 要印在报告前面的几行来源说明)。"""
    player = raw.get("player") or {}
    run = raw.get("run") or {}
    deck, deck_src = deck_from_state(raw)
    potions, slots = potions_from_state(raw)
    ctx = act_context(raw)
    lines = [f"[牌组] {len(deck)} 张，来自 {deck_src}"]

    req: dict = {
        "question": question,
        "hp": int(player.get("hp") or 0),
        "max_hp": int(player.get("max_hp") or 0),
        "ascension": int(run.get("ascension") or 0),
        "potion_slots": slots,
        "potions": potions,
        "relics": relics_from_state(raw),
        "deck": deck,
        "candidates": candidates or [],
        "top": top,
    }
    if seed is not None:
        req["seed"] = seed
    if samples:
        req["samples"] = samples

    # 能量上限只有战斗里报（`max_energy` 在 mod 的战斗分支里）。
    # 战斗外拿不到 ⇒ 用 3 并说一句，**不从遗物反推**。
    max_energy = player.get("max_energy")
    if isinstance(max_energy, int) and max_energy > 0:
        req["base_energy"] = max_energy
    else:
        lines.append("[能量] 战斗外读不到能量上限，按 3 算（战斗里那一份才是观测量）")

    if question == "fight":
        if encounter:
            req["encounter"] = encounter
        elif enemies:
            req["enemies"] = enemies
        else:
            battle = raw.get("battle") or {}
            names = [e.get("name") for e in battle.get("enemies") or [] if e.get("name")]
            if not names:
                raise AdviseError("不在战斗里，也没给 encounter / enemies —— 这一问要有敌人")
            req["enemies"] = names
            lines.append(f"[这一场] 场上的 {len(names)} 只敌人（从开局重打，不是从当前局面续）")
        return req, lines

    if question == "deck":
        return req, lines

    # 整幕链：幕名 + 路线 + 钉 Boss
    name = act or ctx.get("act_name")
    if not name:
        raise AdviseError(ctx.get("note") or "认不出是哪一幕")
    req["act"] = name
    rooms_str, rooms_src = rooms_for(ctx, rooms)
    req["rooms"] = rooms_str
    # 路线是怎么定的，内核不知道（它只看见一串字母）—— 这一句由这边给，
    # `bin/advise` 原样印在报告最上面。
    req["rooms_note"] = rooms_src
    if ctx.get("boss") and not act:
        req["boss"] = ctx["boss"]
        if ctx.get("second_boss"):
            req["second_boss"] = ctx["second_boss"]
            req["double_boss"] = True
    lines.append(f"[这一幕] {name} —— {ctx.get('source') or '调用方指定'}")
    lines.append(f"[路线] {rooms_str} —— {rooms_src}")
    return req, lines


def call(req: dict) -> tuple[int, str]:
    """跑 `advise.exe`，请求走 stdin。返回 (退出码, 报告)。"""
    if not os.path.exists(ADVISE_EXE):
        raise AdviseError(
            f"{ADVISE_EXE} 不存在 —— 先构建：\n"
            '  cd "D:\\game mod\\sts2core"; cargo build --release --bins'
        )
    p = subprocess.run(
        [ADVISE_EXE, "-"],
        input=json.dumps(req, ensure_ascii=False),
        capture_output=True,
        text=True,
        encoding="utf-8",
        cwd=ROOT,
    )
    out = p.stdout or ""
    if p.stderr:
        out += ("\n" if out else "") + p.stderr
    return p.returncode, out


def advise(raw: dict | None = None, **kw) -> str:
    """实况 -> 报告。MCP 那一侧只调这一个函数。"""
    raw = raw if raw is not None else get_state()
    req, lines = build_request(raw, **kw)
    code, out = call(req)
    head = "\n".join(lines)
    if code == 2:
        return f"{head}\n\n[评估跑不起来]\n{out}"
    return f"{head}\n\n{out}"


# --------------------------------------------------------------------------
# 四个决策各自的候选
# --------------------------------------------------------------------------


def reward_candidates(raw: dict) -> list[dict]:
    """卡牌奖励屏上那几张：每张一条「拿它」的候选。基准就是**跳过**。"""
    offered = (raw.get("card_reward") or {}).get("cards") or []
    out = []
    for c in offered:
        card = _card(c)
        out.append({"label": f"拿「{card['name']}」", "add": [card]})
    return out


def _distinct(deck: list[dict]) -> list[tuple[int, str, int]]:
    """牌组里**不同的牌**：返回 (第一次出现的下标, 名字, 有几张)。

    候选按**下标**寻址（内核那边也是），所以重复的牌只评第一张 ——
    三张打击移掉哪一张都一样，评三遍只是把表撑长。
    """
    seen: dict[str, list[int]] = {}
    for i, c in enumerate(deck):
        key = json.dumps(c, ensure_ascii=False, sort_keys=True)
        seen.setdefault(key, []).append(i)
    out = []
    for key, ixs in seen.items():
        out.append((ixs[0], json.loads(key)["name"], len(ixs)))
    out.sort(key=lambda t: t[0])
    return out


def removal_candidates(deck: list[dict]) -> list[dict]:
    return [
        {"label": f"移除「{name}」" + (f"（{n} 张之一）" if n > 1 else ""), "remove": [ix]}
        for ix, name, n in _distinct(deck)
    ]


def upgrade_candidates(deck: list[dict]) -> list[dict]:
    """升级候选。**已经升过的那些不生成** —— 内核会把它们拒掉，
    但在这里就不生成更干净（报告里不会多出一堆"不评分"）。"""
    out = []
    for ix, name, n in _distinct(deck):
        if deck[ix].get("upgraded") or name.endswith("+"):
            continue
        out.append(
            {"label": f"升级「{name}」" + (f"（{n} 张之一）" if n > 1 else ""), "upgrade": [ix]}
        )
    return out


def elite_candidates(rooms: str) -> list[dict]:
    """走不走精英：把路线的**第一间**从精英换成杂兵。

    这一对比的是两条路线，CRN 只共享「这一幕抽到哪几场遭遇」那一维
    （见 `paired_act_delta_across_routes`），所以噪声比换牌组那种大。
    """
    rest = rooms[1:] if rooms else ""
    return [{"label": "绕开精英（换成杂兵）", "rooms": "M" + rest}]


# --------------------------------------------------------------------------
# CLI（冒烟用；实战入口是 MCP 的 sts2-advisor）
# --------------------------------------------------------------------------


def main() -> int:
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8", errors="replace")
        except Exception:  # noqa: BLE001
            pass
    ap = argparse.ArgumentParser(description="L3 构筑顾问：读实况 -> advise.exe")
    ap.add_argument("what", nargs="?", default="deck",
                    choices=["deck", "act", "fight", "reward", "removal", "upgrade", "elite"])
    ap.add_argument("--rooms", default=None)
    ap.add_argument("--act", default=None)
    ap.add_argument("--encounter", default=None)
    ap.add_argument("--samples", type=int, default=None)
    ap.add_argument("--top", type=int, default=0)
    ap.add_argument("--state", default=None, help="拿一份存下来的实况 JSON 跑（离线冒烟用）")
    args = ap.parse_args()

    try:
        if args.state:
            with open(args.state, encoding="utf-8") as f:
                raw = json.load(f)
        else:
            raw = get_state()
        kw: dict = {"rooms": args.rooms, "act": args.act, "samples": args.samples, "top": args.top}
        if args.what == "deck":
            print(advise(raw, question="deck"))
        elif args.what == "fight":
            print(advise(raw, question="fight", encounter=args.encounter, samples=args.samples))
        elif args.what == "act":
            print(advise(raw, question="act", **kw))
        else:
            deck, _ = deck_from_state(raw)
            cands = {
                "reward": lambda: reward_candidates(raw),
                "removal": lambda: removal_candidates(deck),
                "upgrade": lambda: upgrade_candidates(deck),
                "elite": lambda: elite_candidates(
                    rooms_for(act_context(raw), args.rooms)[0]
                ),
            }[args.what]()
            if not cands:
                print("没有候选可比（奖励屏上没牌？牌组里没有可升级的牌？）")
                return 1
            if args.what == "elite":
                kw["rooms"] = kw["rooms"] or rooms_for(act_context(raw), None)[0]
                if not kw["rooms"].startswith("E"):
                    kw["rooms"] = "E" + kw["rooms"][1:]
            print(advise(raw, question="act", candidates=cands, **kw))
    except AdviseError as e:
        print(f"✗ {e}")
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
