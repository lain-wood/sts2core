#!/usr/bin/env python3
r"""从反编译源码里读出每件遗物的**钩子面**，用来判断内核还欠它们什么。

和 `dump_relics.py` 的分工：那个导**权威静态表**（名字/稀有度/描述，来自
跑着的游戏）；这个读**行为**（重写了哪些钩子、调了哪些 Cmd、卡面数值是多少），
只有反编译源码给得了。两个都不做判断 —— 分类和结论在
`docs/relic-hook-taxonomy.md`，那是判断，会过时，要人来维护。

为什么要能重跑：遗物表**会变**。2026-05→06 一个月里游戏就新增了 8 件遗物
（钓鱼竿 / 沉重石板 / 万花筒 / 涅奥骨骰 / 涅奥护符 / 药瓶皮套 / 柔顺发丝 /
羽翼之靴）。游戏更新之后重跑反编译 + 这个脚本，才知道审计还算不算数。

用法（无参数）：
    & "D:\game mod\sts2sim\.venv\Scripts\python.exe" tools/relic_hooks.py
"""
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEC = os.path.join(ROOT, "decompiled", "MegaCrit.Sts2.Core.Models.Relics")
CAT = os.path.join(ROOT, "traces", "relics_catalog.json")
CONTENT = os.path.join(ROOT, "src", "content.rs")

# 钩子重写。RelicModel 继承 AbstractModel，虚方法有 100 个，所以这里不枚举
# 白名单 —— 凡是 override 的方法都报出来，人来看哪些是战斗层。
#
# **返回类型不能写死。** 第一版只认 `Task|void|bool|int`，于是漏掉了
# `public override decimal ModifyDamageAdditive(...)` 这一整族"修饰"钩子，
# 把打击木偶/纸蛙/准备背包三件误判成「没有战斗钩子」。它们恰恰是最需要
# 建模的那一类。教训和「入口看着简单不代表这张牌简单」是同一条：
# **过滤条件写窄了，漏掉的东西不会报错，只会变成一个自信的错结论。**
#
# 也不要写成又懒又贪的字符类（`[\w.<>?\[\], ]+?\s`）：那一版在这些文件上会
# 灾难性回溯，跑不出结果。返回类型里没有空格，所以 `[^\s(]+` 就够，而且线性。
HOOK = re.compile(r"\boverride\s+(?:async\s+)?[^\s(]+\s+(\w+)\s*\(")
# 效果词汇：`XxxCmd.Yyy<Zzz>`。这是"它到底做了什么"最紧凑的表示。
CMD = re.compile(r"(\w+Cmd)\.(\w+)(?:<(\w+)>)?")
# 卡面数值。ValueProp 决定吃不吃力量，一并留着。
VAR = re.compile(r"new (\w+Var)(?:<(\w+)>)?\((-?[\d.]+)m")


def match_file(rid, files):
    """遗物 id -> .cs 文件名。id 是 SCREAMING_SNAKE，类名是 PascalCase。"""
    key = rid.replace("_", "").lower()
    if key in files:
        return files[key]
    # 所有格的 s 在 id 里可能被吃掉（PAELS_WING / PaelsWing），放宽一次
    for k, f in files.items():
        if k.replace("s", "") == key.replace("s", ""):
            return f
    return None


def modelled_ids():
    """内核 `content::RELICS` 里已经登记的 id 和它们的 modelled 标志。"""
    src = open(CONTENT, encoding="utf-8").read()
    out = {}
    for m in re.finditer(r'id:\s*"([A-Z_]+)"[^}]*?modelled:\s*(true|false)', src, re.S):
        out[m.group(1)] = m.group(2) == "true"
    return out


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    if not os.path.isdir(DEC):
        sys.exit(f"没有反编译源码：{DEC}\n先跑 ilspycmd，命令见 docs/relic-hook-taxonomy.md")

    cat = json.load(open(CAT, encoding="utf-8"))["relics"]
    files = {f[:-3].lower(): f for f in os.listdir(DEC) if f.endswith(".cs")}
    known = modelled_ids()

    missing = []
    for rid, meta in cat.items():
        f = match_file(rid, files)
        if not f:
            missing.append(rid)
            continue
        src = open(os.path.join(DEC, f), encoding="utf-8").read()
        hooks = HOOK.findall(src)
        cmds = sorted({f"{a}.{b}" + (f"<{c}>" if c else "") for a, b, c in CMD.findall(src)})
        vars_ = [f"{a}{'<' + b + '>' if b else ''}={c}" for a, b, c in VAR.findall(src)]
        if rid in known:
            tag = "[内核已建模]" if known[rid] else "[内核已登记·未建模]"
        else:
            tag = "[内核表里没有]"
        print(f"### {meta['name']} [{rid}] {meta['rarity']} {tag}")
        print(f"    desc : {meta['description']}")
        print(f"    hooks: {' '.join(hooks) or '-'}")
        print(f"    cmds : {' '.join(cmds) or '-'}")
        print(f"    vars : {' '.join(vars_) or '-'}")

    print()
    print(f"权威表 {len(cat)} 件；配上 .cs {len(cat) - len(missing)} 件；"
          f"内核登记 {len(known)} 件，其中建模 {sum(known.values())} 件")
    if missing:
        # 配不上通常意味着遗物被改名或被删 —— 那本身就是要看的信号
        print(f"**配不上 .cs**：{missing} —— 反编译过期，或者这几件已被游戏删掉")


if __name__ == "__main__":
    main()
