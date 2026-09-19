"""Summarize captured acceptance outputs; never modifies recorded traces."""
from pathlib import Path
import re

root = Path(__file__).resolve().parents[1]
reports = root / "traces/reports_test_subject_fix_2026-09-15"

def read(path):
    data = path.read_bytes()
    return data.decode("utf-16" if data.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig")

for folder in ("before", "before-resume", "final"):
    print("\n" + folder)
    for label in ("verify", "per-turn", "enemy", "solve", "rollout", "rollout-fast", "synth-audit", "fight-eval", "act-eval"):
        path = reports / folder / (label + ".txt")
        if not path.exists():
            continue
        text = read(path)
        if label in ("verify", "per-turn"):
            rows = re.findall(r"统计: 一致 (\d+) / 部分 (\d+) / 不一致 (\d+) / 内容缺失 (\d+) / 跳过 (\d+)", text)
            print(label, [sum(int(r[i]) for r in rows) for i in range(5)])
        else:
            lines = [l for l in text.splitlines() if
                     l.startswith(("出招预测总计", "✓", "-- **截断", "-- 死亡", "== "))]
            print(label, " | ".join(lines[-4:]))
