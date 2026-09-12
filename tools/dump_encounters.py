#!/usr/bin/env python3
r"""导出**遭遇表**（幕 → 遭遇 → 怪物）和**英文类名 ↔ 内核敌人**的连接键。

这是 L3（构筑顾问）的数据地基：L3 要评估「牌组 vs 一幕的遭遇分布」，
而"一幕会遇到什么"今天在仓库里一个字都没有。

## 三个来源，三个档次，**别混着读**

| 输出 | 来源 | 档次 |
|---|---|---|
| `data/encounters.json` 的 `acts` / `encounters` / `monsters` | `decompiled/` 的 `Models.Acts` · `Models.Encounters` · `Models.Monsters` | **[源码]** |
| `data/enemy_ids.json` 的 `observed_names` | 实录 trace 里的 `entity_id` 前缀 + 同一条里的 `name` | **[实录]** |
| 覆盖率那份报告 | 上面两份的交集 | 派生量，**会随着打得多而涨** |

英文类名和内核的中文名之间**没有第三方词典** —— 唯一的连接键是实录：
观测里 `entity_id` 是 `NIBBIT_0`、`name` 是「小啃兽」，两个字段在同一条记录里。
**没见过的怪物留 `null`，不猜。**

## 它必须敢于放弃

`GenerateMonsters()` 有三种形状，只有前两种给得出确定的怪物多重集：

* **数组字面量**（`SlumberingBeetleNormal`）—— 逐个 `ModelDb.Monster<X>()`，确定
* **单元素**（`AeonglassBoss`）—— 确定
* **带随机/循环**（`SlimesNormal` 掷 `Rng.NextBool()`、`RubyRaidersNormal` 抽 3 次）
  —— **标 `exact: false`，只记 `all_possible`，绝不编一个自洽的多重集**

第三种今天有几个、各自卡在哪，报告里逐条印出来。这是「编译器必须敢于放弃」
那条规矩在数据侧的同一个形状：**半份遭遇表比没有更危险**。

## 三条会让它整个停下来的自检

1. `src/replay.rs` 里的别名表读不到 —— 那张表是名字解析的唯一权威
   （`replay::enemy_id`），这里只**读**它，不复制一份。读不到就意味着它被改动过，
   而这个脚本的解析结果会静默地变差
2. `src/content.rs` 的 `ENEMIES` 块读不到
3. 内核的 `max_hp` 落在 [源码] 的血量区间之外 —— 两个来源打架，报出来（不停）

第 3 条是 `dump_bestiary.py` 那条强制自检的同一招：**两份独立数据对不上时，
要么其中一份错了，要么解析错了，两种都必须看得见。**

## 用法（无参数；游戏**不用**开着）

    & "D:\game mod\sts2core\.venv\Scripts\python.exe" tools/dump_encounters.py
    & "D:\game mod\sts2core\.venv\Scripts\python.exe" tools/dump_encounters.py --md

`--md` 吐覆盖率那张表的 markdown，好整块贴进文档（和 `count_content.py --md` 同一个约定）。
"""

from __future__ import annotations

import glob
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEC = os.path.join(ROOT, "decompiled")
ACTS_DIR = os.path.join(DEC, "MegaCrit.Sts2.Core.Models.Acts")
ENC_DIR = os.path.join(DEC, "MegaCrit.Sts2.Core.Models.Encounters")
MON_DIR = os.path.join(DEC, "MegaCrit.Sts2.Core.Models.Monsters")
ASC_FILE = os.path.join(DEC, "MegaCrit.Sts2.Core.Entities.Ascension", "AscensionLevel.cs")
CONTENT = os.path.join(ROOT, "src", "content.rs")
REPLAY = os.path.join(ROOT, "src", "replay.rs")
DATA = os.path.join(ROOT, "data")
OUT_ENC = os.path.join(DATA, "encounters.json")
OUT_IDS = os.path.join(DATA, "enemy_ids.json")
OVERRIDES = os.path.join(DATA, "encounters_overrides.json")

# 反编译出来的编译器合成类型名，`ModelDb.Monster<X>` 之外没别的用处，滤掉噪声用
_SYNTH = re.compile(r"^_003C_003E")


class Fatal(Exception):
    """让整个脚本停下来的自检失败。"""


# ---------------------------------------------------------------------------
# 小工具
# ---------------------------------------------------------------------------

def read(path: str) -> str:
    with open(path, encoding="utf-8", errors="replace") as f:
        return f.read()


def strip_comments(src: str) -> str:
    """去掉 `//` 行注释。块注释在反编译输出里只出现在 XML doc 上，不影响解析。"""
    return re.sub(r"//[^\n]*", "", src)


def member_body(src: str, name: str) -> str | None:
    """取一个方法体（从签名那一行到同缩进的 `\\t}`）。

    反编译输出的缩进是稳定的一个 tab，所以 `\\n\\t}` 就是方法结束。
    找不到返回 None —— 调用方负责决定"找不到"是不是致命。
    """
    m = re.search(r"[\w>\?\]]\s+" + re.escape(name) + r"\s*\([^)]*\)\s*\n\t\{(.*?)\n\t\}", src, re.S)
    return m.group(1) if m else None


def expr_property(src: str, name: str) -> str | None:
    """取一个表达式体属性 `... Name => <expr>;` 的右边。"""
    m = re.search(re.escape(name) + r"\s*=>\s*(.*?);", src, re.S)
    return m.group(1).strip() if m else None


def property_body(src: str, name: str) -> str | None:
    """属性的值：先试表达式体，再试 `{ get { ... } }` 的整块。"""
    e = expr_property(src, name)
    if e is not None:
        return e
    m = re.search(re.escape(name) + r"\s*\n\t\{(.*?)\n\t\}", src, re.S)
    return m.group(1) if m else None


def generics(text: str, outer: str) -> list[str]:
    """`ModelDb.Monster<Nibbit>` -> ["Nibbit"]，按出现顺序，**不去重**。"""
    return [g for g in re.findall(r"ModelDb\." + outer + r"<(\w+)>", text) if not _SYNTH.match(g)]


def screaming(cls: str) -> str:
    """PascalCase -> SCREAMING_SNAKE（`HunterKiller` -> `HUNTER_KILLER`）。

    游戏的 `entity_id` 就是这个形状，实录里逐帧看得到。
    """
    return re.sub(r"(?<!^)(?=[A-Z])", "_", cls).upper()


# ---------------------------------------------------------------------------
# [源码] 幕
# ---------------------------------------------------------------------------

def parse_acts() -> dict:
    out: dict[str, dict] = {}
    for path in sorted(glob.glob(os.path.join(ACTS_DIR, "*.cs"))):
        cls = os.path.basename(path)[:-3]
        if cls == "DeprecatedAct":
            continue
        src = strip_comments(read(path))
        body = member_body(src, "GenerateAllEncounters")
        if body is None:
            continue
        idx = expr_property(src, "public override int Index")
        weak = expr_property(src, "protected override int NumberOfWeakEncounters")
        rooms = expr_property(src, "protected override int BaseNumberOfRooms")
        default = expr_property(src, "public override bool IsDefault")
        unlock = member_body(src, "IsUnlocked")
        out[cls] = {
            # `Index` 0/1/2 = 第几幕。同一个 index 可以有多个幕（备选幕）。
            "index": int(idx) if idx and idx.strip().lstrip("-").isdigit() else None,
            "is_default": (default or "").strip() == "true",
            # `ActModel.NumberOfWeakEncounters` 基类默认 3
            "weak_encounters": int(weak) if weak and weak.strip().isdigit() else 3,
            "weak_encounters_source": "override" if weak else "ActModel 基类默认",
            # 不含 Boss 房和远古房；`GetNumberOfFloors = 本数 + 2`
            "base_rooms": int(rooms) if rooms and rooms.strip().isdigit() else None,
            "unlock": " ".join((unlock or "return true;").split()),
            "boss_discovery_order": generics(property_body(src, "BossDiscoveryOrder") or "", "Encounter"),
            "encounters": generics(body, "Encounter"),
        }
    if not out:
        raise Fatal(f"一个幕都没解析出来，检查 {ACTS_DIR}")
    return out


# ---------------------------------------------------------------------------
# [源码] 遭遇
# ---------------------------------------------------------------------------

# 出现任何一个就说明这一场的构成不是常量
_RANDOM_MARKS = ("Rng", "NextItem", "NextBool", "NextInt", "Shuffle")


def parse_generate_monsters(body: str) -> tuple[list[str] | None, str | None]:
    """解析 `GenerateMonsters()` 的返回值。

    返回 `(怪物列表, 放弃的理由)`；确定时理由是 None，放弃时列表是 None。
    """
    for mark in _RANDOM_MARKS:
        if mark in body:
            return None, f"构成含随机（`{mark}`）"
    if re.search(r"\bfor\s*\(|\bwhile\s*\(|\bforeach\s*\(", body):
        return None, "构成含循环"

    # 局部变量：`Nibbit nibbit = (Nibbit)ModelDb.Monster<Nibbit>().ToMutable();`
    locals_: dict[str, str] = {}
    for var, cls in re.findall(r"\b\w+\s+(\w+)\s*=\s*\(?\w*\)?\s*ModelDb\.Monster<(\w+)>", body):
        locals_[var] = cls

    # 单元素形式
    m = re.search(r"ReadOnlySingleElementList<\(MonsterModel, string\)>\((.*)\)\s*;", body, re.S)
    if m:
        entries = [m.group(1)]
    else:
        # 数组字面量：`new(MonsterModel, string)[N] { (a, "x"), (b, null) }`
        m = re.search(r"new\(MonsterModel, string\)\[(\d+)\]\s*\{(.*)\}", body, re.S)
        if m:
            want = int(m.group(1))
            entries = split_entries(m.group(2))
            if len(entries) != want:
                return None, f"数组声明 {want} 项、实际切出 {len(entries)} 项（解析不可靠）"
        else:
            # 直线 `list.Add((ModelDb.Monster<X>().ToMutable(), "slot"));`
            # —— 上面已经排除过随机和循环，所以这里的 Add 序列就是构成本身。
            entries = re.findall(r"\.Add\(\((.*?)\)\s*\)\s*;", body, re.S)
            if not entries:
                return None, "返回值不是数组字面量、单元素，也不是直线 Add 序列"

    mons: list[str] = []
    for e in entries:
        g = generics(e, "Monster")
        if len(g) == 1:
            mons.append(g[0])
            continue
        if g:
            return None, "一项里出现多个 `ModelDb.Monster<>`"
        ident = re.match(r"\s*\(?\s*(\w+)", e)
        if ident and ident.group(1) in locals_:
            mons.append(locals_[ident.group(1)])
            continue
        return None, "有一项解析不出是哪只怪"
    return mons, None


def split_entries(text: str) -> list[str]:
    """把 `(a, "x"), (b, null)` 切成逐项。按括号深度切，别用逗号裸切。"""
    out, depth, cur = [], 0, ""
    for ch in text:
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        if ch == "," and depth == 0:
            if cur.strip():
                out.append(cur)
            cur = ""
            continue
        cur += ch
    if cur.strip():
        out.append(cur)
    return out


def choice_fields(src: str) -> dict[str, list[str]]:
    """类里那些装着怪物的字段（`_mediumSlimes` / `_raiderValidCounts`）。

    构成解析放弃时，它是手写 override 的原料 —— 一个人要判断
    「这一场到底是哪几只」时，第一眼要看的就是这些候选集。
    """
    out: dict[str, list[str]] = {}
    for name, init in re.findall(
            r"(?:private|internal|public)\s+(?:static\s+)?(?:readonly\s+)?[\w<>,\[\]\? ]+?\s+"
            r"(_\w+)\s*=\s*(new[^;]*?);", src, re.S):
        g = generics(init, "Monster")
        if g:
            out[name] = g
    return out


def parse_encounters() -> dict:
    out: dict[str, dict] = {}
    for path in sorted(glob.glob(os.path.join(ENC_DIR, "*.cs"))):
        cls = os.path.basename(path)[:-3]
        if cls in ("DeprecatedEncounter",):
            continue
        src = strip_comments(read(path))
        room = expr_property(src, "public override RoomType RoomType")
        weak = expr_property(src, "public override bool IsWeak")
        tags = re.findall(r"EncounterTag\.(\w+)", property_body(src, "Tags") or "")
        fields = choice_fields(src)
        allp_src = property_body(src, "AllPossibleMonsters") or ""
        allp = sorted(set(generics(allp_src, "Monster")))
        allp_note = None
        if not allp:
            # **`AllPossibleMonsters` 可以是对字段的间接引用**
            # （`=> _raiderValidCounts.Keys`）。直接扫泛型会扫出空表，而
            # "空表"和"这一场没怪"长得一模一样 —— 这类静默洞正是本仓库最怕的。
            named = [f for f in fields if f in allp_src]
            if named:
                allp = sorted({m for f in named for m in fields[f]})
                allp_note = f"从字段 {'/'.join(named)} 解出来的（属性本身是间接引用）"
            else:
                allp_note = "解析不出来 —— 空表不代表这一场没怪"
        body = member_body(src, "GenerateMonsters")
        if body is None:
            mons, why = None, "没有 `GenerateMonsters` 方法体"
        else:
            mons, why = parse_generate_monsters(body)
        out[cls] = {
            "room_type": (room or "").replace("RoomType.", "").strip() or None,
            # `EncounterModel.IsWeak` 基类默认 false
            "is_weak": (weak or "false").strip() == "true",
            "tags": [t for t in tags if t != "None"],
            "monsters": mons,
            "exact": mons is not None,
            "source": "decompiled" if mons is not None else None,
            "note": why,
            "all_possible": allp,
            "all_possible_note": allp_note,
            # 文件里的怪物候选集字段（`_mediumSlimes` 那种）。**这是原料不是构成** ——
            # 给手写 override 用的，`monsters` 为 null 时才有意义。
            "choice_fields": fields if mons is None else {},
        }
    if not out:
        raise Fatal(f"一个遭遇都没解析出来，检查 {ENC_DIR}")
    return out


# ---------------------------------------------------------------------------
# [源码] 怪物血量（**是个区间，而且分进阶两档**）
# ---------------------------------------------------------------------------

_ASC_CALL = re.compile(
    r"AscensionHelper\.GetValueIfAscension\(AscensionLevel\.(\w+),\s*(-?\d+),\s*(-?\d+)\)")


def parse_hp(expr: str | None) -> dict | None:
    """`GetValueIfAscension(level, ascensionValue, fallbackValue)`。

    [源码] `AscensionHelper`：**没到那一档返回 `fallbackValue`**，所以
    第一个数是高进阶值、第二个是低进阶值。写反了整张表会系统性偏低。
    """
    if not expr:
        return None
    m = _ASC_CALL.search(expr)
    if m:
        return {"low": int(m.group(3)), "high": int(m.group(2)), "gate": m.group(1)}
    m = re.fullmatch(r"\s*(-?\d+)\s*", expr)
    if m:
        return {"low": int(m.group(1)), "high": int(m.group(1)), "gate": None}
    return None


def parse_monsters() -> dict:
    out: dict[str, dict] = {}
    base_of: dict[str, str] = {}
    for path in sorted(glob.glob(os.path.join(MON_DIR, "*.cs"))):
        cls = os.path.basename(path)[:-3]
        src = strip_comments(read(path))
        lo_expr = expr_property(src, "public override int MinInitialHp")
        hi_expr = expr_property(src, "public override int MaxInitialHp")
        lo = parse_hp(lo_expr)
        hi = parse_hp(hi_expr)
        # **`MaxInitialHp => MinInitialHp` 是最常见的写法**（53 个类这么写）：
        # 血量不是区间、就是一个定值。不认这条的话它们全都落成"没解析出来"，
        # `asc::hp_range` 里一个都没有 —— 2026-09-09 `bin/synth_audit` 的
        # 血量区间那一栏就是这么把它们照出来的。
        # **只认这一个精确写法**（互相指的那两句），别去求值任意表达式。
        if hi is None and (hi_expr or "").strip() == "MinInitialHp":
            hi = lo
        if lo is None and (lo_expr or "").strip() == "MaxInitialHp":
            lo = hi
        # 基类：`class X : Y` / `class X : Y, IFoo`。**只记一层**，
        # 下面那个循环会顺着链往上走。
        m = re.search(r"class\s+%s\s*:\s*([A-Za-z0-9_]+)" % re.escape(cls), src)
        if m:
            base_of[cls] = m.group(1)
        out[cls] = {
            "key": screaming(cls),
            "min_initial_hp": lo,
            "max_initial_hp": hi,
            "hp_note": None if (lo and hi) else "血量属性没解析出来（继承或计算得来）",
        }
    if not out:
        raise Fatal(f"一只怪物都没解析出来，检查 {MON_DIR}")

    # **血量继承**：`DecimillipedeSegmentFront : DecimillipedeSegment` 那种
    # ——三节共用基类里的一对 `MinInitialHp/MaxInitialHp`，子类一个字都不写。
    # 不顺着继承链走的话它们永远是 null，而 `asc::hp_range` 里就永远没有它们
    # （`bin/synth_audit` 的血量区间那一栏 2026-09-09 把这 6 只报了出来）。
    #
    # **只在自己解析不出来时才继承**，而且**照抄的那一档要标出来**
    # （`hp_note` 写清楚是从谁继承的）—— 别让"抄来的数"和"自己有的数"长得一样。
    for cls, row in out.items():
        if row["min_initial_hp"] and row["max_initial_hp"]:
            continue
        seen = {cls}
        cur = base_of.get(cls)
        while cur and cur in out and cur not in seen:
            seen.add(cur)
            b = out[cur]
            if b["min_initial_hp"] and b["max_initial_hp"]:
                row["min_initial_hp"] = b["min_initial_hp"]
                row["max_initial_hp"] = b["max_initial_hp"]
                row["hp_note"] = f"血量继承自基类 {cur}"
                break
            cur = base_of.get(cur)
    return out


def parse_ascension() -> dict[int, str]:
    """`AscensionLevel` 枚举的**序号就是进阶数**（`HasLevel => _level >= (int)level`）。"""
    src = read(ASC_FILE)
    m = re.search(r"enum AscensionLevel\s*\{(.*?)\}", src, re.S)
    if not m:
        raise Fatal("AscensionLevel 枚举读不到")
    names = [n.strip() for n in m.group(1).split(",") if n.strip()]
    return {i: n for i, n in enumerate(names) if i > 0}


# ---------------------------------------------------------------------------
# 内核侧：ENEMIES 和别名表
# ---------------------------------------------------------------------------

def kernel_enemies() -> dict[str, str]:
    """内核 `content::ENEMIES` 的 `name -> max_hp 表达式`，按表内顺序。"""
    src = read(CONTENT)
    m = re.search(r"pub static ENEMIES: &\[EnemyDef\] = &\[(.*?)\n\];", src, re.S)
    if not m:
        raise Fatal("content.rs 里的 ENEMIES 块读不到 —— 表的形状变了")
    body = strip_comments(m.group(1))
    out: dict[str, str] = {}
    for blk in re.split(r"EnemyDef \{", body)[1:]:
        n = re.search(r"name:\s*\"([^\"]+)\"", blk)
        h = re.search(r"max_hp:\s*([^,]+),", blk)
        if not n:
            raise Fatal("有一条 EnemyDef 解析不出 name —— 解析规则该改了")
        out[n.group(1)] = h.group(1).strip() if h else "?"
    return out


def kernel_aliases() -> dict[str, str]:
    """读 `replay::enemy_id` 里的别名表。**只读不抄** —— 那张表是唯一权威。"""
    src = read(REPLAY)
    m = re.search(r"fn enemy_id\(name: &str\).*?let alias = match lower\.as_str\(\) \{(.*?)\n\s*\};",
                  src, re.S)
    if not m:
        raise Fatal("replay.rs 里的别名表读不到 —— `enemy_id` 的形状变了，"
                    "这个脚本的名字解析会静默地变差")
    out: dict[str, str] = {}
    for line in m.group(1).splitlines():
        arm = re.match(r'\s*((?:"[^"]*"\s*\|\s*)*"[^"]*")\s*=>\s*"([^"]*)"', line)
        if arm:
            for a in re.findall(r'"([^"]*)"', arm.group(1)):
                out[a.lower()] = arm.group(2)
    if not out:
        raise Fatal("别名表解析出 0 条 —— 匹配臂的写法变了")
    return out


def resolve(name: str, enemies: dict[str, str], alias: dict[str, str]) -> str | None:
    """镜像 `replay::enemy_id` 的三步：直配 -> 截 `#` 后缀 -> 别名表。"""
    n = name.strip()
    if n in enemies:
        return n
    if "#" in n:
        head = n[: n.index("#")].rstrip()
        if head and head in enemies:
            return head
    a = alias.get(n.lower())
    if a and a in enemies:
        return a
    return None


# ---------------------------------------------------------------------------
# [实录] 英文类名 -> 游戏里显示的名字
# ---------------------------------------------------------------------------

def observed_names() -> dict[str, list[str]]:
    """扫全部实录，攒 `entity_id` 前缀 -> 见过的 `name`。

    **被动语料也算**：它的动作是反推的、证据档次低一档，但 `entity_id` 和 `name`
    是原样观测，和驱动语料同一档。
    """
    seen: dict[str, set[str]] = {}
    files = glob.glob(os.path.join(ROOT, "traces", "act*.json"))
    files += glob.glob(os.path.join(ROOT, "traces", "passive_*.json"))
    for path in files:
        try:
            with open(path, encoding="utf-8") as f:
                trace = json.load(f)
        except (OSError, ValueError):
            continue
        for frame in trace.get("frames") or []:
            for e in ((frame.get("obs") or {}).get("enemies") or {}).values():
                eid = (e.get("entity_id") or "").strip()
                if not eid:
                    continue
                # `NIBBIT_0` / `TEST_SUBJECT_0` -> 去掉末尾的槽号
                key = re.sub(r"_\d+$", "", eid).upper()
                seen.setdefault(key, set()).add((e.get("name") or "").strip())
    return {k: sorted(x for x in v if x) for k, v in sorted(seen.items())}


# ---------------------------------------------------------------------------
# 组装 + 报告
# ---------------------------------------------------------------------------

def apply_overrides(encs: dict, warn: list[str]) -> int:
    """并入手写的构成 override。

    和内容编译器那条规矩同一个理由（roadmap「文本推不出来的语义走 overrides
    文件，重新生成不会冲掉」）：解析器**必须敢于放弃**，放弃掉的那几场由人
    读一遍 [源码] 填进来，每条都要写清为什么确定。

    **给一场解析器已经解出来的仗写 override 会被点名** —— 要么解析器进步了、
    override 过期了，要么两者有一个是错的。两种都不该静默。
    """
    if not os.path.exists(OVERRIDES):
        return 0
    with open(OVERRIDES, encoding="utf-8") as f:
        blob = json.load(f)
    n = 0
    for name, ov in (blob.get("encounters") or {}).items():
        e = encs.get(name)
        if e is None:
            warn.append(f"override 里的 `{name}` 不在遭遇表里 —— 反编译目录过期或名字写错")
            continue
        if e["exact"]:
            warn.append(f"override 覆盖了一场解析器已经解出来的仗：`{name}` —— "
                        f"要么解析器进步了、这条 override 该删，要么两者有一个错")
            continue
        e["note_decompiled"] = e["note"]
        e["monsters"] = list(ov["monsters"])
        e["exact"] = True
        e["source"] = "override"
        e["note"] = ov.get("why")
        n += 1
    return n


def build() -> tuple[dict, dict, list[str]]:
    acts = parse_acts()
    encs = parse_encounters()
    mons = parse_monsters()
    asc = parse_ascension()
    enemies = kernel_enemies()
    alias = kernel_aliases()
    obs = observed_names()

    warn: list[str] = []
    n_over = apply_overrides(encs, warn)

    # 英文类名 -> 内核敌人
    ids: dict[str, dict] = {}
    for cls, m in mons.items():
        key = m["key"]
        names = obs.get(key, [])
        rng_lo, rng_hi = m["min_initial_hp"], m["max_initial_hp"]

        cands = []
        for n in names:
            r = resolve(n, enemies, alias)
            if r and r not in cands:
                cands.append(r)
        if not cands:
            # 没在实录里见过，但别名表可能直接收了英文 id（`thieving_hopper`）
            r = resolve(key.lower(), enemies, alias)
            if r:
                cands = [r]

        hit, note = (cands[0] if len(cands) == 1 else None), None
        if len(cands) > 1:
            # **同一个 entity_id 报过多个名字。** 原地变身的怪会这样
            # （结实的卵孵成幼虫，`entity_id` 一个字没变）。
            # 拿 [源码] 的血量区间当裁判：只有一个落在区间里就取它，否则留空。
            fit = [c for c in cands
                   if rng_lo and rng_hi and enemies.get(c, "").isdigit()
                   and rng_lo["low"] <= int(enemies[c]) <= rng_hi["low"]]
            if len(fit) == 1:
                hit = fit[0]
                note = f"观测里这个 entity_id 报过多个名字（{'/'.join(cands)}），按 [源码] 血量区间判定"
            else:
                note = f"观测里这个 entity_id 报过多个名字（{'/'.join(cands)}），血量分不开 —— 留空不猜"
                warn.append(f"`{key}` 观测到多个名字且分不开：{'/'.join(cands)}")

        ids[cls] = {
            "key": key,
            "observed_names": names,
            "kernel_enemy": hit,
            "source": ("trace" if names else "alias") if hit else None,
            "note": note,
        }
        # 自检 3：内核血量 vs [源码] 血量区间
        if hit and rng_lo and rng_hi:
            khp = enemies.get(hit, "?")
            if khp.isdigit():
                k = int(khp)
                if not (rng_lo["low"] <= k <= rng_hi["low"]):
                    warn.append(
                        f"血量对不上：{cls}/{hit} 内核 {k}，[源码] 低进阶区间 "
                        f"{rng_lo['low']}–{rng_hi['low']}（高进阶 {rng_lo['high']}–{rng_hi['high']}）")

    # 实录里见过、却一只怪物类都对不上的前缀（多半是解析或命名假设错了）
    known = {m["key"] for m in mons.values()}
    for key in obs:
        if key not in known:
            warn.append(f"实录见过 `{key}`，但 `Models.Monsters` 里没有同名类 —— "
                        f"名字假设或反编译目录过期")

    enc_out = {
        "note": "[源码] 从 decompiled/ 解析。别手改，跑 tools/dump_encounters.py。",
        "source": {
            "acts": os.path.relpath(ACTS_DIR, ROOT).replace("\\", "/"),
            "encounters": os.path.relpath(ENC_DIR, ROOT).replace("\\", "/"),
            "monsters": os.path.relpath(MON_DIR, ROOT).replace("\\", "/"),
        },
        "ascension_levels": {str(k): v for k, v in asc.items()},
        "ascension_note": "序号即进阶数，`HasLevel => _level >= (int)level`。"
                          "A8 ToughEnemies 抬敌人耐久 · A9 DeadlyEnemies 抬敌人输出 · "
                          "A10 DoubleBoss **只给最后一幕**加第二个 Boss"
                          "（[源码] RunManager：`i == State.Acts.Count - 1`）。"
                          "逐条数值见 data/ascension.json。",
        "room_generation": "[源码] ActModel.GenerateRooms：前 weak_encounters 场从 AllWeakEncounters 抽，"
                           "其余抽到 base_rooms 场；精英预抽 15 场；Boss 在 AllBossEncounters 里均匀取一个。"
                           "抽取用 GrabBag（抽完才补袋）且 AddWithoutRepeatingTags 禁止相邻同 tag。",
        "overrides": os.path.relpath(OVERRIDES, ROOT).replace("\\", "/")
                     + f"（手写，解析器放弃的那几场；这次并入 {n_over} 条）",
        "acts": acts,
        "encounters": encs,
        "monsters": mons,
    }
    ids_out = {
        "note": "[实录] 英文 MonsterModel 类名 ↔ 内核 content::ENEMIES 的名字。"
                "连接键只有实录（观测里 entity_id 和 name 在同一条记录里）。"
                "`kernel_enemy: null` = 还没见过或内核还没建，**不猜**。",
        "resolution": "镜像 replay::enemy_id：直配中文名 -> 截 `#` 后缀 -> replay.rs 的别名表。",
        "monsters": ids,
    }
    return enc_out, ids_out, warn


def coverage(enc: dict, ids: dict) -> list[tuple]:
    """每一幕：内核今天真的开得出几场仗。"""
    rows = []
    for act, a in sorted(enc["acts"].items(), key=lambda kv: (kv[1]["index"] is None, kv[1]["index"], kv[0])):
        ok, uncertain, missing = [], [], []
        for name in a["encounters"]:
            e = enc["encounters"].get(name)
            if e is None:
                missing.append((name, ["<遭遇不在表里>"]))
                continue
            if not e["exact"]:
                uncertain.append((name, e["note"]))
                continue
            gaps = sorted({m for m in e["monsters"]
                           if not (ids["monsters"].get(m) or {}).get("kernel_enemy")})
            (ok if not gaps else missing).append((name, gaps))
        rows.append((act, a, ok, uncertain, missing))
    return rows


def main() -> int:
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8", errors="replace")
        except Exception:  # noqa: BLE001
            pass
    md = "--md" in sys.argv[1:]
    try:
        enc, ids, warn = build()
    except Fatal as e:
        print(f"✗ 自检失败：{e}")
        return 2

    os.makedirs(DATA, exist_ok=True)
    for path, blob in ((OUT_ENC, enc), (OUT_IDS, ids)):
        with open(path, "w", encoding="utf-8") as f:
            json.dump(blob, f, ensure_ascii=False, indent=1)
            f.write("\n")

    rows = coverage(enc, ids)
    n_exact = sum(1 for e in enc["encounters"].values() if e["exact"])
    mapped = sum(1 for m in ids["monsters"].values() if m["kernel_enemy"])

    if md:
        print("| 幕 | index | 遭遇池 | 内核开得出 | 构成不确定 | 缺敌人 |")
        print("|---|---|---|---|---|---|")
        for act, a, ok, unc, miss in rows:
            print(f"| {act} | {a['index']} | {len(a['encounters'])} | "
                  f"**{len(ok)}** | {len(unc)} | {len(miss)} |")
        return 0

    print(f"写好了 {os.path.relpath(OUT_ENC, ROOT)} / {os.path.relpath(OUT_IDS, ROOT)}")
    print(f"[源码] 幕 {len(enc['acts'])} · 遭遇 {len(enc['encounters'])}"
          f"（构成确定 {n_exact}）· 怪物类 {len(enc['monsters'])}")
    print(f"[实录] 怪物类落到内核敌人的 {mapped}/{len(ids['monsters'])}")
    print()
    for act, a, ok, unc, miss in rows:
        star = " ★默认" if a["is_default"] else ""
        print(f"--- {act}（index={a['index']}{star}，{a['base_rooms']} 房 / "
              f"前 {a['weak_encounters']} 场弱怪）遭遇池 {len(a['encounters'])} ---")
        print(f"    内核开得出 {len(ok)}/{len(a['encounters'])}")
        if unc:
            print("    构成不确定（不猜）：")
            for n, why in unc:
                print(f"      · {n}：{why}")
        if miss:
            print("    缺敌人：")
            for n, gaps in miss:
                print(f"      · {n}：{', '.join(gaps)}")
        if a["unlock"] != "return true;":
            print(f"    解锁条件：{a['unlock']}")
        print()

    if warn:
        print(f"⚠ 自检提醒 {len(warn)} 条：")
        for w in warn:
            print(f"  · {w}")
    else:
        print("✓ 血量交叉检验和实录前缀检验都没有异常")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
