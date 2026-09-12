r"""标定叶评估里能力那一项的折扣 `Weights::power`。

    & "D:\game mod\sts2core\.venv\Scripts\python.exe" "D:\game mod\sts2core\tools\calib_power.py" <sib.csv>

判据沿用 P6 那一套，**一个字没改**：兄弟组内排序 + 平均后悔（血/次决策）。
不用全局相关 —— 限深搜索只在兄弟之间比较，全局相关会被"第 1 幕 90 血
vs 第 3 幕 20 血"主导，而那种比较搜索一次都不做。

被标的只有一个标量：`leaf_survive_like + power/100 * leaf_power`。
`leaf_power` 是 `bin/calib` 导出的**未打折原值**。

只用标准库。
"""
from __future__ import annotations

import csv
import io
import sys
from collections import defaultdict

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")


def regret(groups, w, base_col):
    """按 `base + w/100 * leaf_power` 选一条，和组内最优的标签差多少血。"""
    tot = 0.0
    for rows in groups:
        best = max(rows, key=lambda r: r[base_col] + w / 100.0 * r["leaf_power"])
        tot += max(r["label_mean"] for r in rows) - best["label_mean"]
    return tot / len(groups) if groups else 0.0


def main(argv):
    path = argv[0] if argv else "sib.csv"
    rows = []
    for r in csv.DictReader(io.open(path, encoding="utf-8")):
        for k in ("leaf_power", "leaf_horizon", "leaf_extra_energy", "base_survive",
                  "base_damage", "base_hponly", "base_hp"):
            r[k] = float(r[k])
        r["label_mean"] = float(r["label_mean"])
        rows.append(r)

    g = defaultdict(list)
    for r in rows:
        g[r["group"]].append(r)
    groups = [v for v in g.values() if len(v) >= 2]

    def span(v):
        return max(r["label_mean"] for r in v) - min(r["label_mean"] for r in v)

    # P6 的口径：只看"决策真的重要"的那些组
    big = [v for v in groups if span(v) >= 5]
    varies = [v for v in big if len({r["leaf_power"] for r in v}) > 1]

    print(f"组 {len(groups)} · 跨度>=5血 {len(big)} · 其中 leaf_power 组内有变化 {len(varies)}")
    print()
    print("**只有那 %d 组能对这个参数说话** —— 其余组里 leaf_power 组内恒定，" % len(varies))
    print("加多大的权重都不改变组内排序，把它们算进平均只会把信号稀释掉。")
    print()

    # `base_survive` 就是 score::survive_first；LEAF 差的只是 enemy_hp 权重，
    # CSV 里没导，所以这里用 survive 当底，量的是**能力项的增量**。
    for name, subset in (("全部跨度>=5血的组", big), ("只看 leaf_power 有变化的组", varies)):
        print(f"--- {name}（n={len(subset)}）---")
        print("  power=   后悔（血/次决策）")
        for w in (0, 10, 25, 50, 75, 100, 150, 200, 400):
            print(f"   {w:>4}     {regret(subset, w, 'base_survive'):.3f}")
        # oracle / 地板
        floor = sum(span(v) for v in subset) / len(subset) if subset else 0
        print(f"  组内跨度均值（随便挑的量级参考）: {floor:.2f}")
        print()

    # 留一条 trace 交叉验证：每折在其余 trace 上选最优 w，在留出的那条上量后悔
    traces = sorted({r["trace"] for r in rows})
    grid = list(range(0, 401, 5))
    picks, held = [], []
    for t in traces:
        tr = [v for v in varies if v[0]["trace"] != t]
        te = [v for v in varies if v[0]["trace"] == t]
        if not tr or not te:
            continue
        best_w = min(grid, key=lambda w: regret(tr, w, "base_survive"))
        picks.append(best_w)
        held.append(regret(te, best_w, "base_survive"))
    if picks:
        print(f"--- 留一条 trace 交叉验证（{len(picks)} 折，只用有变化的组）---")
        from collections import Counter
        c = Counter(picks).most_common(6)
        print("  各折选中的 power:", c)
        print(f"  留出集平均后悔: {sum(held)/len(held):.3f}")
        print(f"  同一批留出集、power=0 的后悔: "
              f"{sum(regret([v for v in varies if v[0]['trace']==t], 0, 'base_survive') for t in traces if any(v[0]['trace']==t for v in varies))/max(1,len(picks)):.3f}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
