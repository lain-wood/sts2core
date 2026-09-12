#!/usr/bin/env python3
r"""实战：读当前局面，问 L2 求解器"这回合怎么打"。

**只有一条固定命令，不带任何参数**（和录制器同一个理由：命令字符串一变就要
重新过一次权限确认）：

    & "D:\game mod\sts2core\.venv\Scripts\python.exe" "D:\game mod\sts2core\tools\solve_now.py"

它做四件事：

1. 从 mod 的 HTTP 端点读当前状态
2. 用 `record_trace.normalize()` 归一化成 trace 里的 Obs —— **复用录制器那一份**，
   实战和对拍因此永远看到同一个形状的观测
3. 写成单帧 trace `traces/_live.json`
4. 调 `target/release/solve --live` 打印建议

## 它算得准吗

准的部分：出牌顺序、伤害/格挡结算、这回合挨完打还剩多少血 —— 这些规则已经
和真实游戏逐帧对拍过。

不准的部分（**求解器完全看不见**）：遗物、药水、复活、多阶段 Boss、
内容表还没有的牌（会在输出里点名）。敌人下一手也不预测 —— 威胁一律取
**当前显示的意图标签**，所以这是一个单回合的建议，不是整场战斗的计划。

## 用法约定

出牌请照建议的顺序**一张一张打**，每打完一张就重新跑一次这条命令：
`entity_id` 每帧都会重新编号（`docs/trace-format.md` 约束 1），
而且抽牌之后局面就变了。
"""

from __future__ import annotations

import json
import os
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import record_trace as rec  # noqa: E402  复用录制器的 HTTP + normalize

ROOT = rec.ROOT
LIVE_PATH = os.path.join(ROOT, "traces", "_live.json")
SOLVE_EXE = os.path.join(ROOT, "target", "release", "solve.exe")
if not os.path.exists(SOLVE_EXE):  # 非 Windows 兜底
    SOLVE_EXE = os.path.join(ROOT, "target", "release", "solve")

# 战斗态之外没什么可解的
COMBAT_STATES = getattr(rec, "COMBAT_STATES", ("battle", "combat"))


def _active_trace_path() -> str | None:
    """录制器当前在往哪条 trace 写。

    `traces/_plan.txt` 里的 `trace <路径>` 行就是答案 —— 录制器跑完会把动作行
    注释掉但保留这一行，所以它一直有效。读它比让用户再传一遍参数好：
    这条命令是**固定的、不带参数的**（命令字符串一变就要重新过权限确认）。
    """
    try:
        with open(rec.PLAN_PATH, encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if line.startswith("#") or not line:
                    continue
                parts = line.split(None, 1)
                if len(parts) == 2 and parts[0] == "trace":
                    return rec.resolve(parts[1].strip())
    except OSError:
        pass
    return None


def main() -> int:
    # Windows 控制台默认是 GBK，求解器输出里的 ⚠ / ★ / ✓ 直接编不出去。
    # 实战第一次跑就炸在这儿 —— 强制 UTF-8。
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8", errors="replace")
        except Exception:  # noqa: BLE001  老版本或被重定向时忽略
            pass

    try:
        raw = rec.get_state()
    except Exception as e:  # noqa: BLE001
        print(f"✗ 第 1 步（读状态）失败：{e}")
        print("  游戏没开、或者 mod 的端点还没起来。启动流程见 D:\\game mod\\CLAUDE.md")
        return 2

    obs = rec.normalize(raw)
    if obs.get("state_type") not in COMBAT_STATES:
        print(f"✗ 现在不在战斗里（state_type={obs.get('state_type')}），L2 只管局内。")
        return 1
    if not obs.get("is_play_phase"):
        print("✗ 现在不是我的出牌阶段（敌方回合还没演完？）。等一下再跑。")
        return 1

    if not os.path.exists(SOLVE_EXE):
        print(f"✗ 第 3 步（找求解器）失败：{SOLVE_EXE} 不存在")
        print('  先构建：cd "D:\\game mod\\sts2core"; cargo build --release --bins')
        return 2

    # 观测里**没有**每回合计数器（踩踏的"已打出几张攻击牌"、怨恨的"已失去生命"、
    # 邪眼的"已消耗几张"）。单帧推不出来，只能从这一回合的开头一路走过来。
    #
    # 正在录 trace 的话，那条 trace 就是现成的历史 —— 直接交给 `--live`，
    # 计数器由 Rust 侧的 `replay::sync_latest` 走**和对拍完全相同**的那套规则带出来。
    # 在 Python 里再实现一遍计数规则是错的：那等于把规则抄成两份。
    live_path = LIVE_PATH
    src = "单帧（无历史）"
    active = _active_trace_path()
    if active and os.path.exists(active):
        try:
            with open(active, encoding="utf-8") as f:
                rec_trace = json.load(f)
            frames = rec_trace.get("frames") or []
            # 最后一帧必须**就是现在** —— 对不上说明有动作绕过了录制器，
            # 那条历史就不可信了，宁可退回单帧也不要拿错的计数去算。
            if frames and frames[-1].get("obs") == obs:
                live_path = active
                src = f"录制中的 trace（{len(frames)} 帧历史）"
        except Exception:  # noqa: BLE001  历史读不了就退回单帧，不该因此挂掉
            pass

    if live_path == LIVE_PATH:
        run = rec.run_info(raw)
        trace = {
            "version": rec.VERSION,
            "run": run,
            "frames": [{"i": 0, "obs": obs, "action": None}],
            "notes": "solve_now.py 写的单帧局面，只给 --live 用；每次覆盖",
        }
        with open(LIVE_PATH, "w", encoding="utf-8") as f:
            json.dump(trace, f, ensure_ascii=False, indent=1)
    print(f"[历史来源] {src}")

    try:
        p = subprocess.run(
            # `--plan` 让它在单回合建议之后**多报一条跨回合 planner 的线**。
            # 它是被测对象不是驾驶员（`Policy::Plan` 至今只在验收里跑过），
            # 两条线不一致的回合就是要复盘的样本。命令字符串仍然固定无参数。
            [SOLVE_EXE, "--live", live_path, "--plan"],
            capture_output=True,
            text=True,
            encoding="utf-8",
            cwd=ROOT,
        )
    except Exception as e:  # noqa: BLE001
        print(f"✗ 第 4 步（跑求解器）失败：{e}")
        return 2
    if p.stdout:
        print(p.stdout, end="")
    if p.returncode != 0:
        print(f"✗ 第 4 步（跑求解器）退出码 {p.returncode}")
        if p.stderr:
            print(p.stderr, end="")
        return p.returncode
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
