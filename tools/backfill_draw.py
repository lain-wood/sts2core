#!/usr/bin/env python3
r"""把抽牌堆**内容**从保留的原始帧回填进老 trace。

## 为什么这不是造数据

`draw_pile` 一直都在游戏的观测里，是 `record_trace.py` 的 `normalize()`
只留了 `draw_count`（张数）、把内容丢了。原始帧完整保存在
`traces/raw_<name>/NNNN.json`，所以这里做的是**把丢掉的字段捡回来**，
不是凭空生成 —— 和重新打一场是两回事。

「实录语料不可再生」那条规矩仍然成立，所以这个脚本：

* **只增 `draw` 这一个键**，其它字段一个都不碰；
* 已经有 `draw` 的帧不覆盖（重复执行是空操作）；
* **按指纹配对**，配不上的帧留空，绝不猜。

用法（无参数）：
    & "D:\game mod\sts2core\.venv\Scripts\python.exe" tools/backfill_draw.py
"""
import glob
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def pile_card(c):
    """和 `record_trace.py::_pile_card` 保持一致：牌堆是弱身份，只有名字和费用。"""
    return {"name": c.get("name"), "cost": c.get("cost")}


def sig_trace(obs):
    return (
        (obs.get("player") or {}).get("hp"),
        (obs.get("player") or {}).get("block"),
        obs.get("round"),
        obs.get("draw_count"),
        len(obs.get("hand") or []),
    )


def sig_raw(raw):
    p = raw.get("player") or {}
    b = raw.get("battle") or {}
    return (
        p.get("hp"),
        p.get("block"),
        b.get("round"),
        len(p.get("draw_pile") or []),
        len(p.get("hand") or []),
    )


def collect(trace, raws):
    """**按指纹逐帧配对**，不是按下标。

    按下标是不够的：`raw_act1_f2` 里多了一帧动画中途状态，而那一场 HP 全程 80，
    所以"逐帧比 HP"完全没抓到，**是抽牌堆张数那一项抓到的**。
    教训：对齐校验里必须有一个**会变的、而且和要回填的东西直接相关的**量。

    指纹 = (HP, 格挡, 回合, 抽牌堆张数, 手牌张数)。指针只向前走；
    配不上的帧留空 —— 战斗结束帧本来就没有抽牌堆，那是正常的。
    """
    out = [None] * len(trace["frames"])
    j = 0
    matched = 0
    for i, f in enumerate(trace["frames"]):
        want = sig_trace(f["obs"])
        k = j
        while k < len(raws):
            raw = json.load(open(raws[k], encoding="utf-8"))
            if sig_raw(raw) == want:
                pile = (raw.get("player") or {}).get("draw_pile")
                if pile is not None:
                    out[i] = [pile_card(c) for c in pile]
                    matched += 1
                j = k + 1
                break
            k += 1
    return out, matched


def raw_dir_for(path):
    """traces/act1_f2_first.json -> traces/raw_act1_f2"""
    m = re.match(r"(act\d+_f\d+)", os.path.basename(path))
    return os.path.join(ROOT, "traces", "raw_" + m.group(1)) if m else None


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    done = skipped = 0
    for path in sorted(glob.glob(os.path.join(ROOT, "traces", "act*.json"))):
        name = os.path.basename(path)[:-5]
        d = raw_dir_for(path)
        raws = sorted(glob.glob(os.path.join(d, "*.json"))) if d else []
        if not raws:
            print(f"{name:30} 跳过：没有原始帧目录")
            skipped += 1
            continue
        trace = json.load(open(path, encoding="utf-8"))
        draws, matched = collect(trace, raws)
        if matched == 0:
            print(f"{name:30} 跳过：一帧都配不上")
            skipped += 1
            continue
        n = 0
        for f, dr in zip(trace["frames"], draws):
            if dr is None or "draw" in f["obs"]:
                continue
            f["obs"]["draw"] = dr
            n += 1
        total = len(trace["frames"])
        with open(path, "w", encoding="utf-8", newline="\n") as fh:
            json.dump(trace, fh, ensure_ascii=False, indent=1)
            fh.write("\n")
        miss = total - sum(1 for x in draws if x is not None)
        print(f"{name:30} 回填 {n}/{total} 帧" + (f"（{miss} 帧没有抽牌堆或配不上）" if miss else ""))
        done += 1
    print(f"\n回填 {done} 条，跳过 {skipped} 条")


if __name__ == "__main__":
    main()
