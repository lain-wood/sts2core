#!/usr/bin/env python3
r"""从已录的 trace 里聚合每个敌人的观测：HP、意图标签、身上的 status。

`verify --emit-missing` 只给 `max_hp`，写不出 `EnemyDef.moves`。出招得从
意图标签里读，而标签散在每一帧里，得按敌人归并才看得出规律。

**HP 是区间不是常数**：同名敌人在不同房间的 max_hp 会变（实录里
树枝史莱姆（小）出现过 7 和 10）。所以这里报的是**见过的所有取值**，
`EnemyDef.max_hp` 只是个代表值，实际战斗一律用观测到的 HP 建状态。

    & $PY tools/enemy_report.py
    & $PY tools/enemy_report.py --json traces/enemies_observed.json
"""

from __future__ import annotations

import argparse
import glob
import json
import os
import sys
from collections import OrderedDict

from record_trace import ROOT


def main() -> None:
    ap = argparse.ArgumentParser(description="聚合 trace 里的敌人观测")
    # 默认扫**所有**幕。曾经写死成 act1_*.json，于是第 2 幕的敌人被静默漏掉，
    # 而这个文件正是补内容表时要读的那份汇总。
    ap.add_argument("--traces", default=os.path.join(ROOT, "traces", "act*.json"))
    ap.add_argument("--json", help="同时写一份 JSON")
    args = ap.parse_args()

    # name -> {hp: set, intents: {type: {label: count}}, status: set, floors: set}
    agg: dict[str, dict] = OrderedDict()

    for path in sorted(glob.glob(args.traces)):
        with open(path, encoding="utf-8") as f:
            trace = json.load(f)
        floor = (trace.get("run") or {}).get("floor")
        for frame in trace.get("frames") or []:
            for e in (frame.get("obs") or {}).get("enemies", {}).values():
                name = e.get("name")
                if not name:
                    continue
                a = agg.setdefault(
                    name,
                    {"hp": set(), "intents": {}, "status": set(), "floors": set()},
                )
                if e.get("max_hp") is not None:
                    a["hp"].add(e["max_hp"])
                a["floors"].add(floor)
                a["status"].update(e.get("status") or {})
                for i in e.get("intents") or []:
                    t = i.get("type") or "?"
                    lab = (i.get("label") or "").strip() or "<空>"
                    a["intents"].setdefault(t, {})
                    a["intents"][t][lab] = a["intents"][t].get(lab, 0) + 1

    out = {}
    for name, a in sorted(agg.items(), key=lambda kv: -max(kv[1]["hp"] or {0})):
        hp = sorted(a["hp"])
        print(f"{name}  HP {hp}  楼层 {sorted(x for x in a['floors'] if x)}")
        for t, labs in sorted(a["intents"].items()):
            shown = " ".join(f"{k}×{v}" for k, v in sorted(labs.items()))
            print(f"    {t:<12} {shown}")
        if a["status"]:
            print(f"    status: {' '.join(sorted(a['status']))}")
        out[name] = {
            "max_hp_seen": hp,
            "floors": sorted(x for x in a["floors"] if x),
            "intents": {t: labs for t, labs in sorted(a["intents"].items())},
            "status_seen": sorted(a["status"]),
        }

    if args.json:
        with open(args.json, "w", encoding="utf-8") as f:
            json.dump(out, f, ensure_ascii=False, indent=1)
        print(f"\n已写入 {args.json}", file=sys.stderr)


if __name__ == "__main__":
    main()
