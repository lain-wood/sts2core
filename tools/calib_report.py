#!/usr/bin/env python3
"""P6 标定报告：`bin/calib` 导出的样本长什么样，基线有多强。

    & "D:\\game mod\\sts2core\\.venv\\Scripts\\python.exe" "D:\\game mod\\sts2core\\tools\\calib_report.py" <calib.csv>

# 它在回答三个问题，顺序不能反

1. **标签本身有没有信息量？** 如果绝大多数起点的结局都是同一个数
   （仗已经定了），那这批样本区分不出任何评估函数的好坏，先去补语料。
2. **`drawn - undrawn` 有多大？** 这是「这一手抽得好不好」的量。
   限深搜索的叶子停在未抽牌处，所以叶评估**注定拿不到**这部分信息。
   它大到一定程度，「牌组画像够用」这个前提就不成立 —— 那才是要先知道的事。
3. **基线有多强？** 叶评估要打的是 `hp` / `hp+格挡-来袭` / `score::survive_first`。
   赢不过"直接看血量"的评估函数不配存在。

只用标准库：这台机器的 venv 里没有 numpy，而且这点统计不值得引依赖。
秩相关自己算（并列取平均秩），Pearson 也是。
"""
from __future__ import annotations

import csv
import io
import math
import sys
from collections import defaultdict

# 这台机器的控制台是 GBK，中文和 U+2212 之类的字符会当场 UnicodeEncodeError。
# 报告是给人看的，编码问题不该变成"脚本跑不了"。
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")


def pearson(xs, ys):
    n = len(xs)
    if n < 2:
        return float("nan")
    mx = sum(xs) / n
    my = sum(ys) / n
    sx = math.sqrt(sum((x - mx) ** 2 for x in xs))
    sy = math.sqrt(sum((y - my) ** 2 for y in ys))
    if sx == 0 or sy == 0:
        return float("nan")
    return sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / (sx * sy)


def ranks(v):
    """并列取平均秩 —— 这批数据里"血量相同"很常见，不处理并列会把相关系数算高。"""
    order = sorted(range(len(v)), key=lambda i: v[i])
    out = [0.0] * len(v)
    i = 0
    while i < len(order):
        j = i
        while j + 1 < len(order) and v[order[j + 1]] == v[order[i]]:
            j += 1
        avg = (i + j) / 2.0 + 1.0
        for k in range(i, j + 1):
            out[order[k]] = avg
        i = j + 1
    return out


def spearman(xs, ys):
    return pearson(ranks(xs), ranks(ys))


def q(v, p):
    if not v:
        return float("nan")
    s = sorted(v)
    return s[min(len(s) - 1, max(0, int(round((len(s) - 1) * p))))]


def hist(v, lo, hi, buckets=10, width=40):
    """一行一个桶的直方图。看分布形状比看均值重要 —— 这批标签是双峰的。"""
    if not v:
        return []
    step = (hi - lo) / buckets if hi > lo else 1.0
    counts = [0] * buckets
    for x in v:
        k = int((x - lo) / step) if step else 0
        counts[min(buckets - 1, max(0, k))] = counts[min(buckets - 1, max(0, k))] + 1
    top = max(counts) or 1
    out = []
    for i, c in enumerate(counts):
        a = lo + i * step
        b = a + step
        bar = "#" * int(round(c / top * width))
        out.append(f"  [{a:6.1f},{b:6.1f})  {c:4d} {bar}")
    return out


def main() -> None:
    path = sys.argv[1] if len(sys.argv) > 1 else "calib.csv"
    with io.open(path, encoding="utf-8", newline="") as f:
        rows = list(csv.DictReader(f))
    if not rows:
        print("空表")
        return
    num = lambda r, k: float(r[k])

    print(f"=== P6 标定样本：{len(rows)} 条（{path}）===\n")

    # ---- 1. 标签有没有信息量 ----
    un = [num(r, "undrawn_mean") for r in rows]
    dr = [num(r, "drawn_mean") for r in rows]
    spread = [num(r, "undrawn_p90") - num(r, "undrawn_p10") for r in rows]
    dead = [num(r, "undrawn_death_permil") / 10.0 for r in rows]
    flat = sum(1 for s in spread if s == 0)
    print("1) 标签本身")
    print(f"   undrawn 结局血量  min {min(un):.0f} / p50 {q(un,0.5):.0f} / max {max(un):.0f}")
    print(f"   p90-p10 分位差    p50 {q(spread,0.5):.0f} · **{flat}/{len(rows)} 条方差为 0**"
          "（仗已经定了，区分不出评估函数）")
    print(f"   死亡率            p50 {q(dead,0.5):.0f}% · >50% 的起点 "
          f"{sum(1 for d in dead if d > 50)} 条 · =0% 的 {sum(1 for d in dead if d == 0)} 条")
    for line in hist(un, 0, max(un) + 1e-9):
        print(line)

    # ---- 2. 抽牌值多少血 ----
    gap = [d - u for d, u in zip(dr, un)]
    absgap = [abs(g) for g in gap]
    print("\n2) `drawn - undrawn`：这一手抽得好不好值多少血")
    print("   （叶评估停在未抽牌处，这部分信息它**注定拿不到**）")
    print(f"   中位 {q(gap,0.5):+.1f} · 绝对值 p50 {q(absgap,0.5):.1f} / p90 {q(absgap,0.9):.1f}"
          f" / max {max(absgap):.1f}")
    print(f"   与结局尺度对比：结局血量的 p50 是 {q(un,0.5):.0f}")
    for line in hist(gap, min(gap), max(gap) + 1e-9):
        print(line)

    # ---- 3. 基线 ----
    print("\n3) 基线（目标 = undrawn_mean，叶评估必须打得过这三条）")
    base = {
        "hp": [num(r, "hp") for r in rows],
        "hp+格挡-来袭": [max(0.0, num(r, "hp") + num(r, "block") - num(r, "threat_face"))
                        for r in rows],
        "eval_survive": [num(r, "eval_survive") for r in rows],
        # 参照：拿"另一条标签"当预测器，这是任何叶评估的**上界**
        "drawn_mean(上界)": dr,
    }
    print(f"   {'基线':<18} {'Pearson':>8} {'Spearman':>9} {'MAE':>7}")
    for name, v in base.items():
        p = pearson(v, un)
        s = spearman(v, un)
        # MAE 只对同尺度的基线有意义
        mae = (sum(abs(a - b) for a, b in zip(v, un)) / len(un)) if name != "eval_survive" else float("nan")
        mae_s = f"{mae:7.1f}" if mae == mae else "      —"
        print(f"   {name:<18} {p:8.3f} {s:9.3f} {mae_s}")

    # ---- 3b. 只看「还没定」的那些 ----
    #
    # **这才是评估函数真正要工作的那批局面。** 已经定了的起点（128 次采样
    # 结局一模一样）不管用什么评估函数都排得对，把它们算进相关系数是给分。
    live = [r for r in rows if num(r, "undrawn_p90") - num(r, "undrawn_p10") > 0]
    if len(live) > 2:
        lu = [num(r, "undrawn_mean") for r in live]
        ld = [num(r, "drawn_mean") for r in live]
        print(f"\n3b) **只看还没定的 {len(live)} 条**（p90>p10）—— 评估函数真正要工作的地方")
        lb = {
            "hp": [num(r, "hp") for r in live],
            "hp+格挡-来袭": [max(0.0, num(r, "hp") + num(r, "block") - num(r, "threat_face"))
                            for r in live],
            "eval_survive": [num(r, "eval_survive") for r in live],
            "drawn_mean(上界)": ld,
        }
        print(f"   {'基线':<18} {'Pearson':>8} {'Spearman':>9} {'MAE':>7}")
        for name, v in lb.items():
            p = pearson(v, lu)
            s = spearman(v, lu)
            mae = (sum(abs(a2 - b2) for a2, b2 in zip(v, lu)) / len(lu))                 if name != "eval_survive" else float("nan")
            mae_s = f"{mae:7.1f}" if mae == mae else "      -"
            print(f"   {name:<18} {p:8.3f} {s:9.3f} {mae_s}")
        lg = [abs(a2 - b2) for a2, b2 in zip(ld, lu)]
        dg = [abs(num(r, "drawn_mean") - num(r, "undrawn_mean"))
              for r in rows if num(r, "undrawn_p90") - num(r, "undrawn_p10") == 0]
        print(f"   |抽牌差| 在这批里 p50 {q(lg,0.5):.1f} / p90 {q(lg,0.9):.1f}"
              f"（已定的那批只有 p50 {q(dg,0.5):.1f}）")
        print("   ↑ 两个数一起读：**手牌的信息量集中在还没定的局面上**，"
              "而那正是叶评估拿不到它的地方")

    # ---- 4. 分幕看 ----
    print("\n4) 按幕分桶（样本少的桶别当结论）")
    by = defaultdict(list)
    for r in rows:
        act = r["run"].split(" ")[0]
        by[act].append(r)
    print(f"   {'幕':<8} {'条数':>4} {'结局p50':>8} {'死亡率p50':>9} {'|抽牌差|p50':>11} {'hp的Spearman':>13}")
    for act, rs in sorted(by.items()):
        u = [num(r, "undrawn_mean") for r in rs]
        d = [num(r, "drawn_mean") for r in rs]
        dd = [num(r, "undrawn_death_permil") / 10.0 for r in rs]
        g = [abs(a - b) for a, b in zip(d, u)]
        h = [num(r, "hp") for r in rs]
        sp = spearman(h, u) if len(rs) > 2 else float("nan")
        print(f"   {act:<8} {len(rs):4d} {q(u,0.5):8.0f} {q(dd,0.5):8.0f}% {q(g,0.5):11.1f} {sp:13.3f}")

    # ---- 5. 截断 ----
    trunc = sum(1 for r in rows if float(r["drawn_trunc"]) > 0)
    if trunc:
        print(f"\n! {trunc} 条样本里有推演撞上回合上限 —— 那些标签掺着假存活，先去看 P4")


if __name__ == "__main__":
    main()
