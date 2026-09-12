#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""内容清单重数 —— 唯一权威的计数口径。

`docs/roadmap.md` 和这份文件的注释都记着同一条教训：**建了多少牌/遗物/药水
这类计数手改必错**（roadmap 里错过两次、verification-log 尾部的文件清单停在
「70 张牌」时内核已经 114 张）。所以：

    这些数只有一个家 —— `CLAUDE.md` 的「内容清单」，由本脚本重数。
    别在任何别的文档里手写它们。

用法（无参数）：

    "D:\game mod\sts2core\.venv\Scripts\python.exe" tools/count_content.py

`--md` 直接吐出 CLAUDE.md 那张表的 markdown，贴过去就行。

两个坑，都踩过：
  * **别 `grep -c "EnemyDef {"`** —— 文件末尾的 `pub fn enemy_def(...)` 也会被数
    进去（老口径因此把 61 写成 62）。本脚本按表的起止行界定范围。
  * 权威表是「**这个存档发现了多少**」的快照，会随着打得多而涨。分母变大不是退步，
    分子（建了几件）才是工作量。表要定期用 `dump_*.py` 重导。
"""
import io, json, os, re, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = os.path.join(ROOT, "src")
TR = os.path.join(ROOT, "traces")


def read(p):
    return io.open(p, encoding="utf-8").read().split("\n")


def table_block(lines, decl):
    """取 `pub static NAME: &[T] = &[` 到配对的 `];` 之间的行。"""
    start = next(i for i, l in enumerate(lines) if l.startswith(decl))
    end = next(i for i in range(start + 1, len(lines)) if lines[i].rstrip() == "];")
    return lines[start + 1 : end]


def count_entries(block, ctor):
    """数结构体字面量的个数。注释行不算 —— 表里注释常提到别的表的名字。"""
    n = 0
    for l in block:
        s = l.strip()
        if s.startswith("//"):
            continue
        n += s.count(ctor + " {")
    return n


def count_field(block, needle):
    """数某个字段值出现了几次。**注释行不算** —— 和 `count_entries` 同一个坑：
    表头的分节注释里常常原样写着 `modelled: false` 这种字样
    （2026-09-06 就因此把附魔数成 5 真 / 18 假，而表里一共只有 22 条）。"""
    return sum(1 for l in block if not l.strip().startswith("//") and needle in l)


def names(block):
    out = []
    for l in block:
        s = l.strip()
        if s.startswith("//"):
            continue
        m = re.search(r'\bname:\s*"([^"]*)"', s)
        if m:
            out.append(m.group(1))
    return out


def catalog(fn, key):
    p = os.path.join(TR, fn)
    if not os.path.exists(p):
        return None, None
    d = json.load(io.open(p, encoding="utf-8"))
    items = d.get(key) or {}
    return d.get("count", len(items)), items


def main():
    content = read(os.path.join(SRC, "content.rs"))
    ops = read(os.path.join(SRC, "ops.rs"))

    cards_b = table_block(content, "pub static CARDS:")
    enemies_b = table_block(content, "pub static ENEMIES:")
    powers_b = table_block(content, "pub static POWERS:")
    relics_b = table_block(content, "pub static RELICS:")
    unmod_b = table_block(content, "pub static KNOWN_UNMODELLED:")
    synth_b = table_block(content, "pub static SYNTH_ONLY_GAPS:")
    ench_b = table_block(content, "pub static ENCHANTS:")
    potions_b = table_block(ops, "pub const POTIONS:")

    n_cards = count_entries(cards_b, "CardDef")
    n_enemies = count_entries(enemies_b, "EnemyDef")
    n_powers = count_entries(powers_b, "PowerDef")
    n_relics = count_entries(relics_b, "RelicDef")
    n_potions = count_entries(potions_b, "PotionDef")
    n_unmod = sum(1 for l in unmod_b if l.strip().startswith("St::"))
    # 只在**合成路径**上欠账的遗物。和 `modelled` 那一列问的不是同一件事：
    # 那个问对拍路径够不够，这个问 `synth::build` 够不够（没有观测可抄）。
    n_synth_gap = sum(1 for l in synth_b if l.strip().startswith('("'))
    n_ench = count_entries(ench_b, "EnchantDef")

    # 遗物：进表 != 建模
    modelled = count_field(relics_b, "modelled: true")
    not_modelled = count_field(relics_b, "modelled: false")
    # 附魔同理：22 种全部进表，`modelled` 那一列才是建了几个
    ench_modelled = count_field(ench_b, "modelled: true")
    ench_not = count_field(ench_b, "modelled: false")

    # 权威表（快照，会涨）
    cat_cards, cards_j = catalog("cards_catalog.json", "cards")
    cat_relics, _ = catalog("relics_catalog.json", "relics")
    cat_potions, _ = catalog("potions_catalog.json", "potions")
    cat_enemies, _ = catalog("enemies_wiki.json", "enemies")

    # 覆盖：按牌名求差集（CLAUDE.md 记着的口径）
    missing = []
    if cards_j:
        have = set(names(cards_b))
        want = {v.get("name") for v in cards_j.values() if v.get("name")}
        missing = sorted(want - have)
        covered = len(want) - len(missing)
    else:
        covered = None

    # 语料
    def globcount(pred):
        return sum(1 for f in os.listdir(TR) if f.endswith(".json") and pred(f))

    n_act = globcount(lambda f: f.startswith("act"))
    n_syn = globcount(lambda f: f.startswith("synthetic_"))
    n_pas = globcount(lambda f: f.startswith("passive_"))

    if "--md" in sys.argv:
        print("| 表 | 内核 | 权威表 |")
        print("|---|---|---|")
        cov = "覆盖 **%d/%d**，缺 %d 张" % (covered, len(want), len(missing)) if covered is not None else "—"
        print("| `CARDS` | **%d** 项 | %s —— %s |" % (n_cards, cat_cards, cov))
        print("| `ENEMIES` | **%d** 项 | wiki %s 只 |" % (n_enemies, cat_enemies))
        print("| `POWERS` | **%d** 条触发规则 | — |" % n_powers)
        print("| `RELICS` | **%d** 件（`modelled` %d 真 / %d 假）| %s 件 |" % (n_relics, modelled, not_modelled, cat_relics))
        print("| `POTIONS` | **%d** 个槽 | 已发现 %s 瓶 |" % (n_potions, cat_potions))
        print("| `ENCHANTS` | **%d** 种（`modelled` %d 真 / %d 假）| [源码] 22 种 |"
              % (n_ench, ench_modelled, ench_not))
        print("| `KNOWN_UNMODELLED` | **%d** 条豁免 | — |" % n_unmod)
        print("| `SYNTH_ONLY_GAPS` | **%d** 件遗物只在合成路径上欠 | 见 `synth::Gap` |" % n_synth_gap)
        print("| 语料 | **%d** 条实录 + %d 条合成 + %d 条被动 | 实录**不可再生** |" % (n_act, n_syn, n_pas))
        return

    print("内容清单（现数，%s）" % os.path.basename(SRC))
    print("  CARDS              %4d   权威表 %s   覆盖 %s   缺 %d 张"
          % (n_cards, cat_cards, covered, len(missing)))
    print("  ENEMIES            %4d   wiki %s" % (n_enemies, cat_enemies))
    print("  POWERS             %4d" % n_powers)
    print("  RELICS             %4d   权威表 %s   modelled %d 真 / %d 假"
          % (n_relics, cat_relics, modelled, not_modelled))
    print("  POTIONS            %4d   已发现 %s" % (n_potions, cat_potions))
    print("  ENCHANTS           %4d   [源码] 22 种   modelled %d 真 / %d 假"
          % (n_ench, ench_modelled, ench_not))
    print("  KNOWN_UNMODELLED   %4d" % n_unmod)
    print("  SYNTH_ONLY_GAPS    %4d   只在合成路径上欠的遗物" % n_synth_gap)
    print("  语料               %4d 实录 + %d 合成 + %d 被动" % (n_act, n_syn, n_pas))
    if missing:
        print("\n缺的 %d 张牌：" % len(missing))
        print("  " + " / ".join(missing))
    print("\n提醒：权威表是「本档案已发现」的快照，分母会涨。定期跑 dump_*.py 重导。")


if __name__ == "__main__":
    main()
