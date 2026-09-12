#!/usr/bin/env python3
r"""导出**进阶数值**：`data/ascension.json`（全部事实）+ `src/asc.rs`（内核用的表）。

## 进阶不只改血量

[源码] 全表 409 处 `AscensionHelper.GetValueIfAscension`，其中 **396 处在
`Models.Monsters` 里**，分两档：

| 档 | 进阶 | 改什么 |
|---|---|---|
| `ToughEnemies` | **A8** | 主要是**血量**（含多形态 / 孵化），另有**格挡**、**覆甲**、几个 status 量和少数伤害 |
| `DeadlyEnemies` | **A9** | 主要是**伤害**，另有**段数**（`WithHitCount`）、**力量成长**、部分格挡、debuff 量 |

剩下 13 处和战斗无关（商店移除价 `Inflation`／卡牌稀有度与升级概率 `Scarcity`），
连同 `AscensionManager.ApplyEffectsTo` 的两条（A4 少一个药水槽 · A5 塞一张诅咒）
和几条直接 `HasAscension` 的（A1 精英数 ×1.6 · A2 远古回血 ×0.8 · A3 金币 ×0.75 ·
**A10 只有最后一幕加第二个 Boss**）—— 那些一律是 L3 / 局外的，**不进这张表**。

## 连接键：按 (种类, 低进阶值) 去对内核的 op

[源码] 那边的名字是属性名（`ButtDamage`），内核这边是 `EnemyMove.ops` 里的一个数。
中间没有词典，靠三步对：

1. 类名 -> 内核敌人：走 `data/enemy_ids.json`（`dump_encounters.py` 建的）
2. 属性用在什么地方 -> 种类：读它出现在 `DamageCmd.Attack` / `WithHitCount` /
   `GainBlock` / `PowerCmd.Apply<XPower>` / `AddToCombatAndPreview<X>` 的哪一个里
3. 种类 + **低进阶值** -> 内核的 op：低进阶值就是内核今天写着的那个数
   （内核的数是 A1/A2 实测钉下来的）

**对不上就留空并报出来**，不猜：候选个数和 [源码] 里的用点数对不上时整条跳过。
这条规矩和遭遇表那边一样 —— 半张表比没有更危险。

## 这张表验不了

全部 70 条实录都是 A1/A2，**A8 以上没有任何观测**。所以生成出来的高进阶值是
`[源码]` 档，不是 `[实测]` 档。内核侧对应的安全性质是：
**`ascension < 8` 时这张表一个字节都不读**，于是既有对拍逐字节不变。

## 用法（无参数，游戏不用开）

    & "D:\game mod\sts2sim\.venv\Scripts\python.exe" tools/dump_ascension.py
    & "D:\game mod\sts2sim\.venv\Scripts\python.exe" tools/dump_ascension.py --md
"""

from __future__ import annotations

import collections
import glob
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MON_DIR = os.path.join(ROOT, "decompiled", "MegaCrit.Sts2.Core.Models.Monsters")
ASC_FILE = os.path.join(ROOT, "decompiled", "MegaCrit.Sts2.Core.Entities.Ascension",
                        "AscensionLevel.cs")
CONTENT = os.path.join(ROOT, "src", "content.rs")
IDS = os.path.join(ROOT, "data", "enemy_ids.json")
OUT_JSON = os.path.join(ROOT, "data", "ascension.json")
OUT_RS = os.path.join(ROOT, "src", "asc.rs")
OVERRIDES = os.path.join(ROOT, "data", "ascension_overrides.json")

# `GetValueIfAscension(level, ascensionValue, fallbackValue)` ——
# **第一个数是高进阶值**，没到那一档返回第二个。写反了整张表系统性偏低。
ASC = re.compile(
    r"(\w+)\s*(?:=>|=)\s*AscensionHelper\.GetValueIfAscension\(AscensionLevel\.(\w+),\s*"
    r"(-?\d+)[mf]?,\s*(-?\d+)[mf]?\)")
MOVESTATE = re.compile(
    r'new MoveState\(\s*"([A-Z0-9_]+)"\s*,\s*(\w+)\s*(?:,([^;]*?))?\)\s*[;,)]', re.S)

HP_PROPS = ("MinInitialHp", "MaxInitialHp")


class Fatal(Exception):
    pass


def read(path: str) -> str:
    with open(path, encoding="utf-8", errors="replace") as f:
        return f.read()


def strip_comments(s: str) -> str:
    return re.sub(r"//[^\n]*", "", s)


# ---------------------------------------------------------------------------
# 内核侧：把 ENEMIES 拆成 (敌人 -> 招式 -> op)，**op 要按源码顺序**
# ---------------------------------------------------------------------------

# 一条 alternation，一次扫完，才拿得到 op 在 `ops: &[..]` 里的真实下标
KERNEL_OP = re.compile(
    r"EOp::(?P<kind>Attack|AttackPlusStackHits) \{ base: (?P<base>-?\d+), hits: (?P<hits>-?\d+)"
    r"|EOp::Block\((?P<block>-?\d+)\)"
    r"|EOp::(?P<sk>SelfStatus|PlayerStatus) \{ st: St::(?P<st>\w+), amt: (?P<amt>-?\d+)"
    r"|EOp::AddCardToDiscard \{[^}]*?count: (?P<n>-?\d+)")


def kernel_enemies() -> dict:
    src = read(CONTENT)
    m = re.search(r"pub static ENEMIES: &\[EnemyDef\] = &\[(.*?)\n\];", src, re.S)
    if not m:
        raise Fatal("content.rs 里的 ENEMIES 块读不到 —— 表的形状变了")
    body = strip_comments(m.group(1))
    out: dict[str, dict] = {}
    for ix, blk in enumerate(re.split(r"EnemyDef \{", body)[1:]):
        nm = re.search(r'name:\s*"([^"]+)"', blk)
        if not nm:
            raise Fatal("有一条 EnemyDef 解析不出 name —— 解析规则该改了")
        name = nm.group(1)
        hp = re.search(r"max_hp:\s*([^,]+),", blk)
        start = [(a, int(b)) for a, b in
                 re.findall(r"\(St::(\w+),\s*(-?\d+)\)", re.search(
                     r"start_status:\s*&\[(.*?)\]", blk, re.S).group(1)
                     if re.search(r"start_status:\s*&\[(.*?)\]", blk, re.S) else "")]
        moves = []
        for mv in re.finditer(r'EnemyMove \{\s*name:\s*"([^"]+)"[^}]*?ops:\s*&\[', blk):
            # 从 `ops: &[` 起，按括号深度找到配对的 `]`
            i = mv.end()
            depth, j = 1, i
            while j < len(blk) and depth:
                if blk[j] == "[":
                    depth += 1
                elif blk[j] == "]":
                    depth -= 1
                j += 1
            ops_src = blk[i:j - 1]
            ops = []
            # **下标要数全部 op，不能只数认得的那几种。**
            # 第一版拿 `enumerate(KERNEL_OP.finditer(...))` 当下标，于是
            # 一手里只要夹着一个不认识的 op（活雾「膨胀」的 `EOp::Summon`），
            # 后面每一条的下标都往前平移 —— 而平移**不会报错**，
            # 只会在高进阶下把另一个 op 的数值改掉。
            # `asc_ops_low_value_still_matches_the_kernel_op` 抓到的就是这个。
            all_ops = [m.start() for m in re.finditer(r"EOp::", ops_src)]
            for om in KERNEL_OP.finditer(ops_src):
                k = all_ops.index(om.start())
                d = om.groupdict()
                if d["kind"]:
                    ops.append(("damage", int(d["base"]), k))
                    ops.append(("hits", int(d["hits"]), k))
                elif d["block"] is not None:
                    ops.append(("block", int(d["block"]), k))
                elif d["sk"]:
                    ops.append(("status:" + d["st"], int(d["amt"]), k))
                elif d["n"] is not None:
                    ops.append(("cards", int(d["n"]), k))
            moves.append({"name": mv.group(1), "ops": ops})
        out[name] = {"ix": ix, "hp": hp.group(1).strip() if hp else "?",
                     "start_status": start, "moves": moves}
    if not out:
        raise Fatal("ENEMIES 解析出 0 条")
    return out


# ---------------------------------------------------------------------------
# [源码] 侧
# ---------------------------------------------------------------------------

def method_body(src: str, name: str) -> str:
    m = re.search(r"\b" + re.escape(name) + r"\s*\([^)]*\)\s*\n\t\{(.*?)\n\t\}", src, re.S)
    return m.group(1) if m else ""


def kinds_for(prop: str, text: str) -> set[str]:
    """这个属性在这段代码里被当成什么用。判不出来就返回空集（不猜）。"""
    out: set[str] = set()
    p = re.escape(prop)
    if (re.search(r"DamageCmd\.Attack\(\s*" + p + r"\b", text)
            or re.search(r"WithDamage\(\s*" + p + r"\b", text)
            or re.search(r"\wAttackIntent\(\s*" + p + r"\b", text)):
        out.add("damage")
    if (re.search(r"WithHitCount\(\s*" + p + r"\b", text)
            or re.search(r"MultiAttackIntent\(\s*\w+\s*,\s*" + p + r"\b", text)):
        out.add("hits")
    if (re.search(r"GainBlock\([^;]*?\b" + p + r"\b", text)
            or re.search(r"BlockIntent\(\s*" + p + r"\b", text)):
        out.add("block")
    if re.search(r"PowerCmd\.Apply<(\w+)>\([^;]*?\b" + p + r"\b", text) or \
       re.search(r"StatusIntent\(\s*" + p + r"\b", text):
        out.add("status")
    if re.search(r"AddToCombatAndPreview<\w+>\([^;]*?\b" + p + r"\b", text):
        out.add("cards")
    return out


def parse_monsters() -> dict:
    out: dict[str, dict] = {}
    base_of: dict[str, str] = {}
    for path in sorted(glob.glob(os.path.join(MON_DIR, "*.cs"))):
        cls = os.path.basename(path)[:-3]
        src = strip_comments(read(path))
        m = re.search(r"class\s+%s\s*:\s*([A-Za-z0-9_]+)" % re.escape(cls), src)
        if m:
            base_of[cls] = m.group(1)
        props = {m.group(1): (m.group(2), int(m.group(4)), int(m.group(3)))
                 for m in ASC.finditer(src)}
        if not props:
            # **自己一条进阶属性都没有，也要留个空位** —— 它可能是个只继承
            # 基类血量的子类（`DecimillipedeSegmentFront : DecimillipedeSegment`），
            # 下面那个继承循环要能找到它。留空位不会凭空多出任何一行：
            # `hp` 是 None、`values` 是空，join 的时候什么都不产生。
            out[cls] = {"hp": None, "values": [], "base": base_of.get(cls)}
            continue
        # 属性 -> 用到它的招式（招式 id + 那一段代码）
        segs: dict[str, list[tuple[str, str]]] = collections.defaultdict(list)
        for mv in MOVESTATE.finditer(src):
            move_id, handler, intents = mv.group(1), mv.group(2), mv.group(3) or ""
            seg = intents + method_body(src, handler)
            for p in props:
                if re.search(r"\b" + re.escape(p) + r"\b", seg):
                    segs[p].append((move_id, seg))
        rows = []
        for p, (gate, lo, hi) in props.items():
            if p in HP_PROPS:
                continue
            text = "".join(s for _, s in segs.get(p, [])) or src
            rows.append({
                "prop": p, "gate": gate, "low": lo, "high": hi,
                "kinds": sorted(kinds_for(p, text) or kinds_for(p, src)),
                "moves": [m for m, _ in segs.get(p, [])],
            })
        hp = None
        if "MinInitialHp" in props and "MaxInitialHp" in props:
            g1, lo1, hi1 = props["MinInitialHp"]
            g2, lo2, hi2 = props["MaxInitialHp"]
            hp = {"gate": g1 if g1 == g2 else f"{g1}/{g2}",
                  "low": [lo1, lo2], "high": [hi1, hi2]}
        elif "MinInitialHp" in props:
            g1, lo1, hi1 = props["MinInitialHp"]
            hp = {"gate": g1, "low": [lo1, lo1], "high": [hi1, hi1],
                  "note": "只有 MinInitialHp 被进阶门控"}
        out[cls] = {"hp": hp, "values": rows, "base": base_of.get(cls)}
    if not out:
        raise Fatal(f"一只带进阶数值的怪都没解析出来，检查 {MON_DIR}")

    # **血量继承**：`DecimillipedeSegmentFront : DecimillipedeSegment` 那种 ——
    # 三节共用基类里的那对 `MinInitialHp/MaxInitialHp`，子类一个字都不写。
    # 不顺着继承链走的话它们在 `ASC_HP` 里永远缺席（而观测里天天见）。
    # **只继承血量**，招式数值不继承：那些每个子类各写各的。
    for cls, row in out.items():
        if row["hp"]:
            continue
        seen, cur = {cls}, base_of.get(cls)
        while cur and cur not in seen:
            seen.add(cur)
            b = out.get(cur)
            if b and b["hp"]:
                row["hp"] = dict(b["hp"], note=f"血量继承自基类 {cur}")
                break
            cur = base_of.get(cur)
    return out


def parse_levels() -> dict[int, str]:
    m = re.search(r"enum AscensionLevel\s*\{(.*?)\}", read(ASC_FILE), re.S)
    if not m:
        raise Fatal("AscensionLevel 枚举读不到")
    names = [n.strip() for n in m.group(1).split(",") if n.strip()]
    return {i: n for i, n in enumerate(names) if i > 0}


# ---------------------------------------------------------------------------
# 对表
# ---------------------------------------------------------------------------

def kind_matches(kernel_kind: str, src_kinds: list[str]) -> bool:
    if kernel_kind.startswith("status:"):
        return "status" in src_kinds
    return kernel_kind in src_kinds


def match(mons: dict, kern: dict, ids: dict, levels: dict) -> tuple[list, list, list, dict]:
    gate_num = {v: k for k, v in levels.items()}
    hp_rows, op_rows, misses = [], [], []
    stat = collections.Counter()

    over = {}
    if os.path.exists(OVERRIDES):
        over = (json.load(open(OVERRIDES, encoding="utf-8")).get("values") or {})

    for cls, m in sorted(mons.items()):
        kname = (ids.get(cls) or {}).get("kernel_enemy")
        k = kern.get(kname) if kname else None
        if k is None:
            stat["敌人没进内核"] += len(m["values"]) + (1 if m["hp"] else 0)
            continue

        if m["hp"]:
            g = m["hp"]["gate"]
            if g in gate_num:
                hp_rows.append({"def": k["ix"], "name": kname, "cls": cls,
                                "gate": gate_num[g], "low": m["hp"]["low"],
                                "high": m["hp"]["high"]})
                stat["血量"] += 1
                # 自检：内核那个点值该落在低进阶区间里
                if k["hp"].isdigit() and not (m["hp"]["low"][0] <= int(k["hp"]) <= m["hp"]["low"][1]):
                    misses.append((cls, kname, "MaxInitialHp", "内核血量落在 [源码] 低进阶区间外",
                                   f"内核 {k['hp']}，区间 {m['hp']['low']}"))

        for v in m["values"]:
            stat["招式数值总计"] += 1
            key = f"{cls}.{v['prop']}"
            if key in over:
                for tgt in over[key]:
                    op_rows.append({"def": k["ix"], "name": kname, "cls": cls,
                                    "mv": tgt["mv"], "mv_name": k["moves"][tgt["mv"]]["name"],
                                    "op": tgt["op"], "field": tgt["field"],
                                    "gate": gate_num[v["gate"]], "low": v["low"],
                                    "high": v["high"], "prop": v["prop"],
                                    "src": "override"})
                stat["override"] += 1
                continue
            if not v["kinds"]:
                misses.append((cls, kname, v["prop"], "判不出种类", ""))
                stat["判不出种类"] += 1
                continue
            cands = []
            for mi, mv in enumerate(k["moves"]):
                for kind, val, op_ix in mv["ops"]:
                    if val == v["low"] and kind_matches(kind, v["kinds"]):
                        cands.append((mi, op_ix, kind))
            n_sites = max(len(v["moves"]), 1)
            if len(cands) == 1 or (len(cands) == n_sites and len(cands) > 1):
                for mi, op_ix, kind in cands:
                    op_rows.append({"def": k["ix"], "name": kname, "cls": cls,
                                    "mv": mi, "mv_name": k["moves"][mi]["name"],
                                    "op": op_ix,
                                    "field": "hits" if kind == "hits" else "value",
                                    "gate": gate_num[v["gate"]], "low": v["low"],
                                    "high": v["high"], "prop": v["prop"], "src": "matched"})
                stat["对上"] += 1
            elif not cands:
                misses.append((cls, kname, v["prop"], "内核里找不到这个值",
                               f"{v['kinds']} = {v['low']}"))
                stat["找不到"] += 1
            else:
                misses.append((cls, kname, v["prop"], f"{len(cands)} 个候选 / 源码 {n_sites} 个用点",
                               str([(c[0], c[1]) for c in cands])))
                stat["多个候选"] += 1
    return hp_rows, op_rows, misses, stat


# ---------------------------------------------------------------------------
# 生成 Rust
# ---------------------------------------------------------------------------

RS_HEADER = '''//! 进阶数值表 —— **生成产物，别手改**。
//!
//! 重新生成：
//! ```text
//! & "D:\\game mod\\sts2sim\\.venv\\Scripts\\python.exe" tools/dump_ascension.py
//! ```
//!
//! 来源和口径写在 `tools/dump_ascension.py` 的文件头。三条要点：
//!
//! * **`[源码]` 档，验不了。** 全部实录都是 A1/A2，A8 以上没有任何观测。
//!   对应的安全性质是 [`adjust`] 在 `asc < 8` 时提前 return ——
//!   既有对拍逐字节不变，`ascension_below_8_is_a_no_op` 守着。
//! * **`def` / `mv` 是下标，`name` / `mv_name` 是它的校验位。**
//!   有人往 `ENEMIES` 中间插一条，下标就全歪了，而歪掉不会报错 ——
//!   `asc_table_indices_still_point_at_the_named_enemy` 守着。
//! * **对不上的没进表。** 生成器按 (种类, 低进阶值) 去认内核的 op，
//!   候选个数和 [源码] 的用点数对不上就整条跳过并报出来。
//!   欠的那些在 `data/ascension.json` 里逐条写着卡在哪。

use crate::ops::EOp;

/// 一只敌人的血量区间，两档。`low` 是 A8 以下，`high` 是 A8 及以上。
#[derive(Clone, Copy, Debug)]
pub struct AscHp {
    pub def: u16,
    pub name: &'static str,
    /// 到这一档才换值（8 = `ToughEnemies`）
    pub gate: u8,
    pub low: (i32, i32),
    pub high: (i32, i32),
}

/// 一个招式里的一个数值，两档。
#[derive(Clone, Copy, Debug)]
pub struct AscOp {
    pub def: u16,
    pub name: &'static str,
    pub mv: u8,
    pub mv_name: &'static str,
    /// `EnemyMove::ops` 里的下标
    pub op: u8,
    /// `true` = 改的是段数，`false` = 改的是数值
    pub hits: bool,
    /// 8 = `ToughEnemies` · 9 = `DeadlyEnemies`
    pub gate: u8,
    pub low: i32,
    pub high: i32,
    /// [源码] 里那个属性叫什么，出问题时回去查它
    pub prop: &'static str,
}

'''

RS_TAIL = '''
/// 这一招的这个 op，在进阶 `asc` 下是什么样。
///
/// **这是内核里唯一一处把进阶算进 `EOp` 的地方。** 读 `EnemyMove::ops` 的
/// 八个地方（执行、两个威胁预测、planner、`move_signature`、两个验收台）
/// 全部经由它 —— 漏掉一个的表现是那条路径**静默地**按低进阶算。
///
/// `asc < 8` 时逐字返回原 op，一次表都不查。
#[inline]
pub fn adjust(def: u16, mv: usize, op_ix: usize, asc: u8, op: EOp) -> EOp {
    if asc < 8 {
        return op;
    }
    for r in ASC_OPS {
        if r.def == def && r.mv as usize == mv && r.op as usize == op_ix && asc >= r.gate {
            return patch(op, r.high, r.hits);
        }
    }
    op
}

/// 把新值写回 `EOp` 的对应字段。**认不出的 op 原样返回** ——
/// 生成器只会为它认识的那几种 op 建行，这里是兜底。
fn patch(op: EOp, v: i32, hits_field: bool) -> EOp {
    match op {
        EOp::Attack { base, hits } => {
            if hits_field {
                EOp::Attack { base, hits: v }
            } else {
                EOp::Attack { base: v, hits }
            }
        }
        EOp::AttackPlusStackHits { base, hits, per } => {
            if hits_field {
                EOp::AttackPlusStackHits { base, hits: v, per }
            } else {
                EOp::AttackPlusStackHits { base: v, hits, per }
            }
        }
        EOp::Block(_) => EOp::Block(v),
        EOp::SelfStatus { st, .. } => EOp::SelfStatus { st, amt: v },
        EOp::PlayerStatus { st, .. } => EOp::PlayerStatus { st, amt: v },
        EOp::AddCardToDiscard { card, .. } => EOp::AddCardToDiscard { card, count: v },
        other => other,
    }
}

/// 这只敌人在进阶 `asc` 下的血量区间。表里没有就返回 `None`
/// —— 调用方该退回 `EnemyDef::max_hp`（那是 A1/A2 实测的点值）。
pub fn hp_range(def: u16, asc: u8) -> Option<(i32, i32)> {
    for r in ASC_HP {
        if r.def == def {
            return Some(if asc >= r.gate { r.high } else { r.low });
        }
    }
    None
}
'''


def emit_rs(hp_rows: list, op_rows: list, levels: dict) -> str:
    def esc(s: str) -> str:
        return s.replace("\\", "\\\\").replace('"', '\\"')

    out = [RS_HEADER]
    out.append("/// 进阶档位名（`AscensionLevel` 的枚举序号就是进阶数）。\n")
    out.append("pub static LEVEL_NAMES: &[(u8, &str)] = &[\n")
    for i, n in sorted(levels.items()):
        out.append(f'    ({i}, "{n}"),\n')
    out.append("];\n\n")

    out.append(f"/// 血量两档，{len(hp_rows)} 只。\n")
    out.append("pub static ASC_HP: &[AscHp] = &[\n")
    for r in sorted(hp_rows, key=lambda r: r["def"]):
        out.append(f'    AscHp {{ def: {r["def"]}, name: "{esc(r["name"])}", gate: {r["gate"]}, '
                   f'low: ({r["low"][0]}, {r["low"][1]}), high: ({r["high"][0]}, {r["high"][1]}) }},'
                   f'  // {r["cls"]}\n')
    out.append("];\n\n")

    out.append(f"/// 招式数值两档，{len(op_rows)} 条。\n")
    out.append("pub static ASC_OPS: &[AscOp] = &[\n")
    for r in sorted(op_rows, key=lambda r: (r["def"], r["mv"], r["op"])):
        out.append(
            f'    AscOp {{ def: {r["def"]}, name: "{esc(r["name"])}", mv: {r["mv"]}, '
            f'mv_name: "{esc(r["mv_name"])}", op: {r["op"]}, '
            f'hits: {"true" if r["field"] == "hits" else "false"}, gate: {r["gate"]}, '
            f'low: {r["low"]}, high: {r["high"]}, prop: "{r["prop"]}" }},\n')
    out.append("];\n")
    out.append(RS_TAIL)
    return "".join(out)


# ---------------------------------------------------------------------------

def main() -> int:
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8", errors="replace")
        except Exception:  # noqa: BLE001
            pass
    md = "--md" in sys.argv[1:]
    try:
        levels = parse_levels()
        mons = parse_monsters()
        kern = kernel_enemies()
        ids = json.load(open(IDS, encoding="utf-8"))["monsters"]
        hp_rows, op_rows, misses, stat = match(mons, kern, ids, levels)
    except Fatal as e:
        print(f"✗ 自检失败：{e}")
        return 2
    except FileNotFoundError as e:
        print(f"✗ 读不到 {e.filename} —— 先跑一次 tools/dump_encounters.py")
        return 2

    blob = {
        "note": "[源码] 进阶数值。别手改，跑 tools/dump_ascension.py。",
        "levels": {str(k): v for k, v in sorted(levels.items())},
        "combat_relevant": {"ToughEnemies": 8, "DeadlyEnemies": 9},
        "outside_combat": {
            "1 SwarmingElites": "地图精英数 ×1.6（`MapPointTypeCounts.NumOfElites`）",
            "2 WearyTraveler": "远古事件的回满血 ×0.8（`AncientEventModel`）",
            "3 Poverty": "战斗金币奖励 ×0.75（`EncounterModel.Min/MaxGoldReward`）",
            "4 TightBelt": "药水槽 −1（`AscensionManager.ApplyEffectsTo`）",
            "5 AscendersBane": "开局往牌组塞一张升华诅咒（同上）",
            "6 Inflation": "商店移除费 75->100，每次涨价 25->50",
            "7 Scarcity": "卡牌稀有度与升级概率变差（`CardRarityOdds` / `CardFactory`）",
            "10 DoubleBoss": "**只有最后一幕**加第二个 Boss（`RunManager`，`i == Acts.Count - 1`）",
        },
        "monsters": mons,
        "unmatched": [{"cls": c, "kernel": k, "prop": p, "why": w, "detail": d}
                      for c, k, p, w, d in misses],
    }
    os.makedirs(os.path.dirname(OUT_JSON), exist_ok=True)
    with open(OUT_JSON, "w", encoding="utf-8") as f:
        json.dump(blob, f, ensure_ascii=False, indent=1)
        f.write("\n")
    with open(OUT_RS, "w", encoding="utf-8") as f:
        f.write(emit_rs(hp_rows, op_rows, levels))

    if md:
        print("| 口径 | 条数 |")
        print("|---|---|")
        for k in ("血量", "对上", "override", "多个候选", "找不到", "判不出种类", "敌人没进内核"):
            if stat.get(k):
                print(f"| {k} | {stat[k]} |")
        return 0

    print(f"写好了 {os.path.relpath(OUT_JSON, ROOT)} / {os.path.relpath(OUT_RS, ROOT)}")
    print(f"血量两档 {len(hp_rows)} 只 · 招式数值 {len(op_rows)} 条")
    print("口径：" + " · ".join(f"{k} {v}" for k, v in stat.most_common()))
    print()
    if misses:
        print(f"没进表的 {len(misses)} 条（**欠定就留空**，逐条卡在哪）：")
        by = collections.Counter(w for _, _, _, w, _ in misses)
        for w, n in by.most_common():
            print(f"  · {w}：{n} 条")
        print()
        for c, k, p, w, d in misses[:40]:
            print(f"    {c}.{p} ({k})  {w}  {d}")
        if len(misses) > 40:
            print(f"    …… 还有 {len(misses) - 40} 条，全表在 data/ascension.json 的 unmatched")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
