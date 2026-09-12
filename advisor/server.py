"""MCP server: 构筑建议（`sts2-advisor`）。**2026-09-12 起由 sts2core 的 L3 供货。**

工具名和签名一个没动（父目录 `CLAUDE.md` 的实战流程不用改），**内部实现整个换了**：

    以前：sts2sim（Python 模拟器）—— 固定牌组 + 一张边际改动，打一场"代表性的仗"
    现在：sts2core 的 L3 —— 同一副牌组走**一整幕**，死亡率在前、血量在后

换掉的理由写在 `sts2core/docs/roadmap.md`：旧的那套**只认 35 张牌**、
条件牌拒绝评分、敌人模型只有血量和每回合伤害，而它给出的 ΔHP
**在死亡处被截断** —— 一场两边都会死的仗里每个候选都塌向 0，
排名恰好在最要紧的时候失声。

## 这一层只做翻译

脏活（读实况、认牌名、挑候选、拼请求）在 `tools/advise_core.py`，
算账在 `target/release/advise`。**这个文件里一条游戏规则都没有** ——
它只是把 MCP 的八个工具名映射到那边的三个问法（`deck` / `fight` / `act`）。

## 为什么它住在 sts2core 里（2026-09-12 从 `sts2sim/` 搬过来）

它是 L3 接口的**消费者**：`bin/advise` 的请求格式一变，这个文件就得跟着变。
放在另一个目录（而且是另一个没有版本控制的目录）时，没有任何东西把这两件事
绑在一起 —— 没有测试、没有 CI、没有一次提交。搬进来之后：

* `from tools import advise_core` 是**同仓库内的普通 import**，
  不再有 `sys.path` 注入和「兄弟目录在哪」的猜测；
* 接口和它的消费者在**同一次提交**里改；
* CI 能跑 `tools/advise_core.py --selftest`（拿两份手搓的实况 JSON 当 fixture）
  —— 在这之前 Python 这一层**一条守卫都没有**。

**依赖的分界线**：`mcp` 这个包**只出现在本目录**。`tools/` 那十几个脚本
（两个录制器 / `solve_now.py` / `advise_core.py` / 各种 dump）
**一律只用标准库** —— 那条规矩没变，它保证的是"录制和实战入口不依赖 MCP"。

## 旧签名里那两个参数

* **`seeds` / `samples`**：现在是「**走几条整幕链**」。默认从 10–16 抬到 256
  （`CHAINS`），因为判决量换成了死亡率 —— 而那是这套东西里噪声最大的一维
  （`synth::eval` 模块头量过）。一条链几毫秒，抬得起。
* **`node_budget`**：**新内核里没有对应物**（L2 的单回合搜索是穷尽的，
  预算用光会自报 `complete=false`）。**传了会说一句，然后忽略** ——
  悄悄吃掉一个参数，下一个人会以为它生效了。
* **`fight`**（`advise_removal` / `advise_upgrade`）：旧版拿它选"打哪种仗"。
  新版判的是**整幕**，所以它只剩一个用处：`boss` 让路线只留 Boss 那一间。
  真要换路线用新加的 `rooms`（可选参数，加在末尾，不动既有调用）。
"""

from __future__ import annotations

import argparse

from mcp.server.fastmcp import FastMCP

from tools import advise_core as core

mcp = FastMCP("sts2-advisor")

#: 默认走几条整幕链。见模块头「旧签名里那两个参数」。**改这个数要同步改模块头。**
CHAINS = 256


def _budget_note(node_budget: int) -> str:
    if not node_budget:
        return ""
    return (
        f"\n[忽略] `node_budget={node_budget}`：新内核里没有这个旋钮 —— "
        "L2 的单回合搜索是穷尽的，预算用光它自己会报 `complete=false`。"
    )


def _run(**kw) -> str:
    """所有工具的唯一出口。`AdviseError` 的消息**本身就是给用户看的**。"""
    try:
        return core.advise(**kw)
    except core.AdviseError as e:
        return f"✗ {e}"
    except Exception as e:  # noqa: BLE001  MCP 那边看不到 traceback，至少给个类型
        return f"✗ 评估挂了：{type(e).__name__}: {e}"


def _rooms_for_fight_kind(fight: str, rooms: str) -> str:
    """旧的 `fight` 参数今天只剩一个用处。见模块头。"""
    if rooms:
        return rooms
    if (fight or "").lower() == "boss":
        return "B"
    return ""


# ---------------------------------------------------------------------------
# 两个诊断工具
# ---------------------------------------------------------------------------


@mcp.tool()
def snapshot_deck() -> str:
    """记下当前牌组，并报出**内核看见了什么**。

    mod 打了牌组补丁之后主牌组每个界面都报，所以这不再是一份"战斗里才拿得到、
    之后全靠缓存"的快照 —— 它现在是个**诊断**：这副牌里有几张内核不认识、
    遗物缺什么、药水认不认得。**信任任何建议之前先看这一份。**

    顺带刷新「这是哪一幕 / Boss 是谁」的缓存（地图屏上才读得到）。
    """
    try:
        raw = core.get_state()
    except core.AdviseError as e:
        return f"✗ {e}"
    out = _run(raw=raw, question="deck")
    try:
        ctx = core.act_context(raw)
    except Exception:  # noqa: BLE001  诊断不该因为缓存失败而整个失败
        return out
    if ctx.get("act_name"):
        out += f"\n[这一幕] {ctx['act_name']} · Boss {ctx.get('boss') or '还不知道'} —— {ctx.get('source')}"
        if isinstance(ctx.get("rooms_left"), int):
            out += f" · 离 Boss 还有 {ctx['rooms_left']} 间"
    elif ctx.get("note"):
        out += f"\n[这一幕] {ctx['note']}"
    return out


@mcp.tool()
def inspect_fight() -> str:
    """把**场上这场仗**从开局重打 N 次，报死亡率和血量分布。

    旧版报的是模拟器推断出的 HP / 每回合伤害 / 四个 trait；新版直接把仗打完 ——
    内核认不认得这些敌人、它们的出招表建没建，都会当场暴露（认不得就拒绝作答）。

    **它不是局内建议**：这一份是"从开局重打"，不是"从现在这个局面续"。
    这一回合逐张怎么出牌问 `tools/solve_now.py`（L2）。
    """
    return _run(question="fight", samples=CHAINS)


# ---------------------------------------------------------------------------
# 牌组层
# ---------------------------------------------------------------------------


@mcp.tool()
def inspect_deck(samples: int = CHAINS) -> str:
    """这副牌组**走得完这一幕吗** —— 不带任何候选的整幕评估。

    旧版报的是"每回合打多少伤害 / 多少格挡 / 稀释度"，那几个数是从一副固定牌组
    里估出来的边际量。新版报的是判决量本身：走完这一幕死多少次、死在哪一间、
    活下来还剩多少血，外加这次评估**漏掉了几场**（内核开不出的遭遇）。

    Args:
        samples: 走几条整幕链（每条几毫秒）。
    """
    return _run(question="act", samples=samples)


@mcp.tool()
def boss_requirement(turns: int = 8) -> str:
    """这一幕的 Boss，拿现在这副牌组去打会怎样。

    Boss 是谁**从地图屏读**（`map.boss.id`），所以先在地图上调用过一次
    任意工具，缓存里才有它。旧版给的是"伤害/格挡缺口"，那是从一个参数化的
    Boss 原型算的；新版是把那一只 Boss 真打 N 次。

    Args:
        turns: **旧参数，忽略** —— 新内核把仗打到底，没有"几回合内"这个概念。
    """
    try:
        raw = core.get_state()
        ctx = core.act_context(raw)
    except core.AdviseError as e:
        return f"✗ {e}"
    boss = ctx.get("boss")
    if not boss:
        return (
            "✗ 还不知道这一幕的 Boss 是谁。**到地图屏上再调一次任意工具** ——\n"
            "  Boss id 只有地图块里有（`map.boss.id`），卡牌奖励屏上没有。"
            + (f"\n  {ctx['note']}" if ctx.get("note") else "")
        )
    note = "" if turns == 8 else f"\n[忽略] `turns={turns}`：新内核把仗打到底，没有这个旋钮。"
    return _run(raw=raw, question="fight", encounter=boss, samples=CHAINS) + note


# ---------------------------------------------------------------------------
# 四个决策
# ---------------------------------------------------------------------------


@mcp.tool()
def advise_card_reward(seeds: int = CHAINS, node_budget: int = 0, rooms: str = "") -> str:
    """卡牌奖励屏上那几张，各自值不值得拿。

    每张牌一条候选（"拿它"），基准是**跳过**。判据：走完这一幕的死亡率先比，
    活下来的样本再比终点血量。

    Args:
        seeds: 走几条整幕链（配对比较，两边共享同一批随机）。
        node_budget: 旧参数，新内核里没有对应物，传了会说一句然后忽略。
        rooms: 路线（`M` 杂兵 / `E` 精英 / `B` Boss / `R` 休息）。
            不给就照默认那条 `[判断]` 截到地图上剩下的间数。
    """
    try:
        raw = core.get_state()
        cands = core.reward_candidates(raw)
    except core.AdviseError as e:
        return f"✗ {e}"
    if not cands:
        return "✗ 屏幕上没有卡牌奖励。"
    return _run(raw=raw, question="act", candidates=cands, samples=seeds, rooms=rooms or None) + _budget_note(node_budget)


@mcp.tool()
def advise_removal(fight: str = "elite", seeds: int = CHAINS, node_budget: int = 0,
                   top: int = 8, rooms: str = "") -> str:
    """牌组里**移掉哪一张**最划算（商店的那个决策）。

    每张不同的牌各一条候选（重复的牌只评第一张 —— 三张打击移掉哪一张都一样）。

    Args:
        fight: 旧参数。今天只剩一个用处：`"boss"` 把路线缩成只有 Boss 那一间。
        seeds: 走几条整幕链。
        node_budget: 旧参数，忽略。
        top: 报告里印几行（**全部都评过**，这个数只管印）。
        rooms: 路线，见 `advise_card_reward`。
    """
    try:
        raw = core.get_state()
        deck, _ = core.deck_from_state(raw)
        cands = core.removal_candidates(deck)
    except core.AdviseError as e:
        return f"✗ {e}"
    return _run(raw=raw, question="act", candidates=cands, samples=seeds, top=top,
                rooms=_rooms_for_fight_kind(fight, rooms) or None) + _budget_note(node_budget)


@mcp.tool()
def advise_upgrade(fight: str = "elite", seeds: int = CHAINS, node_budget: int = 0,
                   top: int = 8, rooms: str = "") -> str:
    """篝火**打铁打哪一张**。

    已经升过的牌不生成候选。内核不认识的牌会被点名拒绝评分 ——
    **拒绝不是低分**，那一行要自己判断。

    Args:
        fight: 旧参数，见 `advise_removal`。
        seeds: 走几条整幕链。
        node_budget: 旧参数，忽略。
        top: 报告里印几行。
        rooms: 路线，见 `advise_card_reward`。
    """
    try:
        raw = core.get_state()
        deck, _ = core.deck_from_state(raw)
        cands = core.upgrade_candidates(deck)
    except core.AdviseError as e:
        return f"✗ {e}"
    if not cands:
        return "✗ 牌组里没有还能升级的牌。"
    return _run(raw=raw, question="act", candidates=cands, samples=seeds, top=top,
                rooms=_rooms_for_fight_kind(fight, rooms) or None) + _budget_note(node_budget)


@mcp.tool()
def advise_elite(seeds: int = CHAINS, node_budget: int = 0, rooms: str = "") -> str:
    """下一间走不走精英 —— 两条**路线**的配对比较。

    基准是「下一间打精英」，候选是「换成杂兵」。精英掉的那件遗物**不建模**，
    所以这里给的仍然只是"它的血价"，判断权在你。

    这一对的 CRN 只共享「这一幕抽到哪几场遭遇」那一维（逐场敌人血量在路线错位
    的那一刻就分岔了），**比换牌组那种噪声大** —— 报告里那一行会说。

    Args:
        seeds: 走几条整幕链。
        node_budget: 旧参数，忽略。
        rooms: 路线。不给就照默认那条截到剩下的间数，并把**第一间换成精英**。
    """
    try:
        raw = core.get_state()
        ctx = core.act_context(raw)
    except core.AdviseError as e:
        return f"✗ {e}"
    base = rooms or core.rooms_for(ctx, None)[0]
    if not base.startswith("E"):
        base = "E" + base[1:]
    return _run(raw=raw, question="act", candidates=core.elite_candidates(base),
                samples=seeds, rooms=base) + _budget_note(node_budget)


def main() -> None:
    p = argparse.ArgumentParser(description="STS2 构筑建议 MCP server（L3 由 sts2core 供货）")
    p.add_argument("--host", default="localhost")
    p.add_argument("--port", type=int, default=15526)
    p.add_argument("--no-trust-env", action="store_true")
    args = p.parse_args()
    # HTTP 那一份在 `sts2core/tools/record_trace.py`（只用标准库），
    # 两个录制器和 `solve_now.py` 走的是同一个端点常量。
    core.rec.BASE_URL = f"http://{args.host}:{args.port}"
    core.rec.SP_URL = core.rec.BASE_URL + "/api/v1/singleplayer"
    mcp.run()


if __name__ == "__main__":
    main()
