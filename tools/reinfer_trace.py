#!/usr/bin/env python3
"""把被动录制的 trace 里的动作**重新反推一遍**。

`watch_trace.py` 存的是「观测 + 反推出来的动作」，而观测是不可再生的证据、
动作只是推断。所以反推规则改进之后，不需要重打一场 —— 拿存下来的观测重算即可。

**只重写带 `inferred: true` 的动作**，驱动式录制器（record_trace.py）
写进去的真实动作一个都不碰。

用法（固定命令，无参数；目标取 traces/_watch.txt 里的 trace 那一行）：

    & "D:\game mod\sts2core\.venv\Scripts\python.exe" "D:\game mod\sts2core\tools\reinfer_trace.py"
"""
from __future__ import annotations

import json
import io
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from record_trace import _control_returned  # noqa: E402
from watch_trace import infer_action, read_watch_file  # noqa: E402


def main() -> None:
    path, _ = read_watch_file()
    d = json.load(io.open(path, encoding="utf-8"))
    fs = d["frames"]

    # 1) 先压掉敌方回合的中间态帧。留着的话一次「结束回合」会被反推成两个
    #    动作，第二个凭空多出来 —— 2026-08-23 沙漏那条 trace 14 处不一致里
    #    有 11 处是这么来的。**只压被动录制的帧**，真实录的一个都不动。
    dropped = 0
    if len(fs) > 2:
        keep = [fs[0]]
        for f in fs[1:-1]:
            if not _control_returned(f["obs"]) and (f.get("action") or {}).get("inferred"):
                dropped += 1
                continue
            keep.append(f)
        keep.append(fs[-1])
        fs = keep
        for i, f in enumerate(fs):
            f["i"] = i
        d["frames"] = fs

    changed = 0
    kept = 0
    for i in range(len(fs) - 1):
        act = fs[i].get("action") or {}
        if not act.get("inferred"):
            kept += 1
            continue  # 真实录的动作，不碰
        # 手打的第一步可能跨了开录之前的多个回合，那种已经人工标成 unknown
        # 且带 why，重推会把它变回一个自信的错标签 —— 保留人工判定。
        if act.get("kind") == "unknown" and "开录之前" in (act.get("why") or ""):
            kept += 1
            continue
        new = infer_action(fs[i]["obs"], fs[i + 1]["obs"])
        if new != act:
            changed += 1
        fs[i]["action"] = new

    io.open(path, "w", encoding="utf-8").write(json.dumps(d, ensure_ascii=False, indent=1))
    kinds: dict[str, int] = {}
    for f in fs:
        k = (f.get("action") or {}).get("kind", "<末帧>")
        kinds[k] = kinds.get(k, 0) + 1
    print(f"{path}\n  重推 {changed} 个动作，保留 {kept} 个（真实录的或人工判定的）")
    print(f"  动作分布: {kinds}")


if __name__ == "__main__":
    main()
