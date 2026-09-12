r"""量一件事：实战路径上那条 D=2 的线，有多少是**采样噪声**。

    & "D:\game mod\sts2sim\.venv\Scripts\python.exe" "D:\game mod\sts2core\tools\plan_seed_sweep.py"

做法：拿已录的 trace，逐个**回合起点**截出一个「到此为止」的单帧局面
（`--live` 读最后一帧，前面那些帧是每回合计数器的来路），
对同一个局面跑 N 个 `--plan-seed`，看那条 D=2 的线变不变。

为什么这是判据：机会节点的抽牌样本由 `Plan::seed` 和局面指纹一起决定
（CRN），**换种子 = 换一批抽牌样本，局面一个字没变**。
所以同一个局面下 D=2 给出几条不同的线，就是这一层估计噪声的直接读数 ——
单回合求解器在这件事上是 0（它在回合内是穷尽的、确定的）。

只用标准库。
"""

import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SOLVE = os.path.join(ROOT, "target", "release", "solve.exe")
if not os.path.exists(SOLVE):
    SOLVE = os.path.join(ROOT, "target", "release", "solve")
TMP = os.path.join(ROOT, "traces", "_seed_sweep.json")

SEEDS = [0x5EED_0825, 1, 2, 3, 4, 5, 6, 7]

RE_SINGLE = re.compile(r"^\s+单回合\([^)]*\)\s+(.*)$")
RE_PLAN = re.compile(r"^\s+跨回合\(D=\d\)\s+(.*)$")
RE_SAME = re.compile(r"与单回合线\*\*一致\*\*")
RE_BOTH_END = re.compile(r"两条线都能这一回合打完")


def turn_start_indices(frames):
    """回合起点 = 上一帧的动作是 `end`（或者第 0 帧）。"""
    out = []
    for i, f in enumerate(frames):
        obs = f.get("obs") or {}
        if not obs.get("is_play_phase", obs.get("play_phase", False)):
            continue
        if i == 0:
            out.append(i)
            continue
        prev = frames[i - 1].get("action") or {}
        if prev.get("kind") == "end_turn":
            out.append(i)
    return out


def run(path, seed, extra=()):
    p = subprocess.run(
        [SOLVE, "--live", path, "--plan", "--plan-seed", str(seed), *extra],
        capture_output=True, text=True, encoding="utf-8", cwd=ROOT,
    )
    single = plan = None
    same = False
    for line in (p.stdout or "").splitlines():
        if RE_SAME.search(line):
            same = True
        m = RE_SINGLE.match(line)
        if m:
            single = m.group(1).strip()
        m = RE_PLAN.match(line)
        if m:
            plan = m.group(1).strip()
    return single, plan, same


def main(argv):
    # 带一个值的开关**原样透传给 solve.exe**，其余当路径。
    # **这类 A/B 必须同语料跑两遍** —— 起点数从 198 涨到 252 之后，
    # 拿新读数去比历史读数是在比分母。
    #
    # 一开始只认了 `--power-reserve`，加 `--leaf` 的时候忘了这里，
    # 于是 `--leaf eval` 被当成文件路径，当场 traceback。改成表驱动。
    PASSTHROUGH = (
        "--power-reserve",
        "--leaf",
        "--plan-k",
        "--budget",
        "--deep-score",
        "--window",
        # 阶段 3 那三个旋钮走 `--plan-set "k=v,…"` 一个口子（键的解析在
        # `Plan::apply`，三个验收台共用）。**加新旋钮不用再回来改这张表** ——
        # `--leaf` 那次忘了改，`--leaf eval` 被当成文件路径当场 traceback。
        "--plan-set",
    )
    extra = []
    rest = []
    argv = list(argv)
    i = 0
    while i < len(argv):
        if argv[i] in PASSTHROUGH and i + 1 < len(argv):
            extra += [argv[i], argv[i + 1]]
            i += 2
        else:
            rest.append(argv[i])
            i += 1
    argv = rest
    paths = argv or [
        os.path.join(ROOT, "traces", f)
        for f in sorted(os.listdir(os.path.join(ROOT, "traces")))
        if f.startswith("act") and f.endswith(".json")
    ]
    n_pos = 0
    n_stable = 0
    n_agree_all = 0
    rows = []
    for path in paths:
        with open(path, encoding="utf-8") as f:
            t = json.load(f)
        frames = t.get("frames") or []
        for i in turn_start_indices(frames):
            sub = dict(t)
            sub["frames"] = frames[: i + 1]
            with open(TMP, "w", encoding="utf-8") as f:
                json.dump(sub, f, ensure_ascii=False)
            lines = set()
            singles = set()
            agree = 0
            for sd in SEEDS:
                single, plan, same = run(TMP, sd, extra)
                if single is None and same:
                    # 两条线一致时不印那两行，用单回合线本身当标签
                    agree += 1
                    lines.add("=SAME=")
                    continue
                if plan is None:
                    continue
                lines.add(plan)
                if single:
                    singles.add(single)
            if not lines:
                continue
            n_pos += 1
            if len(lines) == 1:
                n_stable += 1
            if agree == len(SEEDS):
                n_agree_all += 1
            rows.append((os.path.basename(path), i, len(lines), agree))
    print()
    tag = " ".join(extra) if extra else "默认 power_reserve"
    print(f"回合起点 {n_pos} 个 · 每个跑 {len(SEEDS)} 个种子（{tag}）")
    print(f"  D=2 的线**换种子不变**：{n_stable}/{n_pos}")
    print(f"  D=2 **每个种子都和单回合线一致**：{n_agree_all}/{n_pos}")
    unstable = [r for r in rows if r[2] > 1]
    if unstable:
        print(f"  不稳定的 {len(unstable)} 个（种子一换就换线）:")
        for name, i, k, agree in unstable[:20]:
            print(f"    {name} 帧{i}: {k} 条不同的线（其中 {agree}/{len(SEEDS)} 个种子和单回合线一致）")
    print()
    print("怎么读：单回合求解器在同一个局面下是**确定**的（回合内穷尽搜索），")
    print("所以上面每一个「不稳定」都只能来自 D≥2 那层的机会节点采样。")
    print("它不说明 planner 错，只说明**这一条具体建议的可重复性**有多少。")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
