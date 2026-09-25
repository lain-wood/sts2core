#!/usr/bin/env python3
r"""从反编译源码导出**全部遗物的游戏内部 id**（`data/relic_ids.json`）。

## 为什么要有这张表

`relic_ids_exist_in_the_authoritative_catalog` 守着「`content::RELICS` 里的 id
没有拼错」—— 拼错是静默失效（`relic_by_id` 永远查不到）。它原来只认两个权威：

* `traces/relics_catalog.json`：游戏导的，但只含**这个存档已经发现的**
* 实录里的 `relics[].id`：只含**真带着打过的**

**预先补**一件还没捡到过的遗物时两个都找不到（2026-09-25 那一批 12 件就是这样）。
这张表是第三个权威，档次是 **[源码]**：游戏自己就是这么算 id 的 ——
`ModelDb.GetEntry(type) => StringHelper.Slugify(type.Name)`，而 `Slugify` 是

    CamelCaseRegex  ([A-Za-z0-9]|\G(?!^))([A-Z])  ->  "$1_$2"
    再 ToUpperInvariant、空白换 `_`、删掉 [^A-Z0-9_]

下面的 `slugify` 逐句照抄。

## 自检（会停下来的那种）

`traces/relics_catalog.json` 里的每一个 id 都必须能在这里算出来 ——
那一份是游戏当场报的，算不出来就是 `slugify` 抄错了，这张表整张不可信。

用法（无参数）：
    & "D:\game mod\sts2core\.venv\Scripts\python.exe" tools/dump_relic_ids.py
"""
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEC = os.path.join(ROOT, "decompiled")
RELICS = os.path.join(DEC, "MegaCrit.Sts2.Core.Models.Relics")
POOLS = os.path.join(DEC, "MegaCrit.Sts2.Core.Models.RelicPools")
CATALOG = os.path.join(ROOT, "traces", "relics_catalog.json")
OUT = os.path.join(ROOT, "data", "relic_ids.json")


def slugify(name):
    # `\G(?!^)` 让连续大写逐个断开（"ABC" -> "A_B_C"）。Python 的 re 没有 \G，
    # 等价写法：一个大写字母，前面紧挨着任意字母或数字，就在它前面插 `_`。
    text = re.sub(r"(?<=[A-Za-z0-9])(?=[A-Z])", "_", name.strip())
    text = re.sub(r"\s+", "_", text.upper())
    return re.sub(r"[^A-Z0-9_]", "", text)


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    if not os.path.isdir(RELICS):
        sys.exit(f"没有反编译源码：{RELICS}")

    pool_of = {}
    for f in sorted(os.listdir(POOLS)):
        src = open(os.path.join(POOLS, f), encoding="utf-8").read()
        for cls in re.findall(r"Relic<(\w+)>", src):
            pool_of.setdefault(cls, f.replace("RelicPool.cs", ""))

    out = {}
    for f in sorted(os.listdir(RELICS)):
        if not f.endswith(".cs"):
            continue
        cls = f[:-3]
        src = open(os.path.join(RELICS, f), encoding="utf-8").read()
        m = re.search(r"RelicRarity\.(\w+)", src)
        out[slugify(cls)] = {
            "class": cls,
            "pool": pool_of.get(cls),
            "rarity": m.group(1) if m else None,
        }

    # 自检：游戏报过的 id 必须全部算得出来
    if os.path.exists(CATALOG):
        seen = json.load(open(CATALOG, encoding="utf-8"))["relics"]
        missing = sorted(set(seen) - set(out))
        if missing:
            sys.exit(f"✗ 游戏报过、这里算不出来的 id：{missing} —— slugify 抄错了，或者反编译过期")
        print(f"自检：游戏导出的 {len(seen)} 个 id 全部算得出来")

    doc = {
        "note": "[源码] 遗物 id = StringHelper.Slugify(类名)，由 tools/dump_relic_ids.py 从 decompiled/ 生成。"
                "只用来证明 content::RELICS 的 id 没拼错；pool 是 RelicPools 里第一个收它的池，null = 哪个池都没收。",
        "relics": out,
    }
    with open(OUT, "w", encoding="utf-8", newline="\n") as fh:
        json.dump(doc, fh, ensure_ascii=False, indent=1, sort_keys=True)
        fh.write("\n")
    print(f"写了 {len(out)} 件 -> {os.path.relpath(OUT, ROOT)}")


if __name__ == "__main__":
    main()
