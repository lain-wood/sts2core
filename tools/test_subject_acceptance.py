"""Capture the full required regression suite without shell glob ambiguity."""
from pathlib import Path
import subprocess
import sys
import json

root = Path(__file__).resolve().parents[1]
out = root / "traces" / "reports_test_subject_fix_2026-09-15" / sys.argv[1]
out.mkdir(parents=True, exist_ok=True)
real = sorted((root / "traces").glob("act*.json"))
all_traces = real + sorted((root / "traces").glob("synthetic_*.json"))
suite = [
    ("verify", "verify", all_traces, []),
    ("per-turn", "verify", all_traces, ["--per-turn"]),
    ("enemy", "verify", all_traces, ["--predict-enemy"]),
    ("solve", "solve", real, []),
    ("rollout", "rollout", real, []),
    ("rollout-fast", "rollout", real, ["--policy", "fast"]),
    ("synth-audit", "synth_audit", all_traces, []),
    ("fight-eval", "fight_eval", all_traces, []),
    ("act-eval", "act_eval", real, []),
]
results = {}
for label, binary, traces, flags in suite:
    cmd = [str(root / "target" / "release" / (binary + ".exe")), *map(str, traces), *flags]
    with (out / (label + ".txt")).open("w", encoding="utf-8") as log:
        result = subprocess.run(cmd, cwd=root, stdout=log, stderr=subprocess.STDOUT)
    results[label] = result.returncode
    (out / "exit-codes.json").write_text(json.dumps(results, indent=2), encoding="utf-8")
    print(f"{label}: exit {result.returncode}", flush=True)
