#!/usr/bin/env python3
r"""导出权威药水表 -> `traces/potions_catalog.json`。

用法（游戏要开着）：

    & "D:\game mod\sts2sim\.venv\Scripts\python.exe" "D:\game mod\sts2core\tools\dump_potions.py"

## 它和 `dump_relics.py` / `dump_catalog.py` **走的不是同一条路**

那两个都靠 `GET /api/v1/wiki`。**药水走不了那条路** —— wiki 端点自己说了：

```
"scope": "active_profile_discovered_cards_and_relics"
"error": "item_type must be one of: all, card, relic."
```

compendium 的 `potion_lab` 也只给 id：

```
"limitation": "Profile exposes discovered potion IDs;
               per-potion rules text and lab UI metadata are not exposed"
```

**但"没有一个端点专门给药水文本"不等于"游戏不给文本"。** 游戏在两个地方
把药水的规则文本原样吐出来，都实测过（2026-08-21）：

| 在哪 | 什么时候有 |
|---|---|
| `player.potions[]` 的 `name`/`description`/`target_type` | 身上带着这瓶的时候 |
| `rewards.items[]` 的 `potion_name`/`potion_description` | 战斗奖励里掉了一瓶的时候 |

**而 `record_trace.py::normalize()` 把这些字段全丢了**（只留了 id/name/slot），
所以它们一直躺在 `traces/raw_*/` 里没人用。这已经是同一个模式的第三次了 ——
前两次是 `max_potion_slots` 和 `draw_pile`。规矩再写一遍：
**说"观测里没有"之前先去 `traces/raw_*/` 看一眼。**

## 所以这张表是攒出来的，不是一次导完的

* **id 全表**：compendium `potion_lab.discovered_ids`（游戏权威，本档案已发现的）
* **文本**：能捡到多少捡多少 —— 当前游戏状态 + 全部 `traces/raw_*/` 历史帧
* **只增不减**：已经捡到文本的条目不会被空值覆盖（和 `bestiary.json` 同一个规矩）

捡不到文本的那些**留空，并且在输出里点名**。留空是结论的一部分：
内核那边它们就该继续缺着，而不是照初代记忆填一个数。
"""

from __future__ import annotations

import glob
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from record_trace import BASE_URL, ROOT, _request  # noqa: E402

COMPENDIUM_URL = BASE_URL + "/api/v1/compendium"
STATE_URL = BASE_URL + "/api/v1/singleplayer?format=json"
OUT = os.path.join(ROOT, "traces", "potions_catalog.json")
RAW_GLOB = os.path.join(ROOT, "traces", "raw_*", "*.json")

# compendium 里药水那一区叫 `potion_lab`（2026-08-21 实测；**不是**照
# `relic_collection` 猜的 `potion_collection`）。后面几个是备用，
# 万一 mod 改名了不至于当场报"找不到"。
SECTION_CANDIDATES = ("potion_lab", "potion_collection", "potion_library", "potions")


def discovered_ids() -> tuple[list[str], str]:
    d = _request("GET", COMPENDIUM_URL, timeout=60.0)
    sections = d.get("sections") or {}
    for key in SECTION_CANDIDATES:
        sec = sections.get(key)
        if isinstance(sec, dict):
            ids = sec.get("discovered_ids") or sec.get("ids") or []
            if ids:
                return list(ids), key
    raise RuntimeError(
        "compendium 里找不到药水区。见过的区：" + " ".join(sorted(sections))
        + "\n  —— 把对的那个名字加进 SECTION_CANDIDATES，别去猜 id"
    )


def harvest(obj, into: dict[str, dict], where: str) -> None:
    """把任何一段 JSON 里的药水文本捡出来。

    两种形状（都实测过），**按 id 收口**，所以同一瓶从哪儿捡到的都能合并。
    """
    if isinstance(obj, dict):
        pid = obj.get("id")
        if (
            isinstance(pid, str)
            and obj.get("can_use_in_combat") is not None
            and obj.get("name")
        ):
            row = into.setdefault(pid, {})
            row.setdefault("name", obj.get("name"))
            row.setdefault("description", obj.get("description") or "")
            row.setdefault("target_type", obj.get("target_type"))
            row.setdefault("can_use_in_combat", obj.get("can_use_in_combat"))
            row.setdefault("text_from", where)
        if obj.get("potion_id"):
            row = into.setdefault(obj["potion_id"], {})
            row.setdefault("name", obj.get("potion_name"))
            row.setdefault("description", obj.get("potion_description") or "")
            row.setdefault("text_from", where + "(奖励屏)")
        for v in obj.values():
            harvest(v, into, where)
    elif isinstance(obj, list):
        for v in obj:
            harvest(v, into, where)


def main() -> int:
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8", errors="replace")
        except Exception:  # noqa: BLE001
            pass

    try:
        ids, section = discovered_ids()
    except Exception as e:  # noqa: BLE001
        print(f"✗ 第 1 步（读 compendium 的药水区）失败：{e}")
        print("  游戏没开、或者 mod 端点还没起来。")
        return 2

    text: dict[str, dict] = {}

    # 第 2 步：先吃已有的产物（只增不减）
    if os.path.exists(OUT):
        try:
            with open(OUT, encoding="utf-8") as f:
                for pid, row in (json.load(f).get("potions") or {}).items():
                    if row.get("description"):
                        text[pid] = row
        except Exception as e:  # noqa: BLE001
            print(f"✗ 第 2 步（读旧的 {os.path.basename(OUT)}）失败：{e}")
            return 2

    # 第 3 步：当前游戏状态（身上带的药水 / 奖励屏上的药水）
    try:
        harvest(_request("GET", STATE_URL, timeout=30.0), text, "当前局面")
    except Exception as e:  # noqa: BLE001
        print(f"! 第 3 步（读当前局面）失败：{e} —— 跳过，继续扫历史帧")

    # 第 4 步：全部历史原始帧
    n_raw = 0
    for path in sorted(glob.glob(RAW_GLOB)):
        try:
            with open(path, encoding="utf-8") as f:
                harvest(json.load(f), text, "raw 历史帧")
            n_raw += 1
        except Exception:  # noqa: BLE001
            continue

    # 只保留真的是药水的（`potion_id` 那条路捡得准，`can_use_in_combat` 那条
    # 会把遗物也捞进来 —— 药水腰带就被捞过一次）。**判据是 id 在已发现表里。**
    known = set(ids)
    stray = sorted(k for k in text if k not in known)
    for k in stray:
        text.pop(k)

    out = {}
    for pid in ids:
        row = dict(text.get(pid) or {})
        row["id"] = pid
        out[pid] = row
    no_text = [p for p in ids if not out[p].get("description")]

    doc = {
        "source": f"id 来自 /api/v1/compendium 的 sections.{section}（游戏权威）；"
        "文本来自 player.potions[] / rewards.items[]（当前局面 + traces/raw_*/ 历史帧）",
        "why_not_wiki": "GET /api/v1/wiki 的 scope 是 "
        "active_profile_discovered_cards_and_relics，item_type 只认 all/card/relic —— "
        "药水查不到。compendium 的 potion_lab 也自己声明只给 id、不给规则文本。",
        "note": "只含本档案**已发现**的药水。有文本的那些是**游戏原文**（[wiki] 一档）；"
        "没文本的留空 —— 内核那边就该继续缺着，不要照初代记忆填。"
        "数值要升到 [实测] 得在实战里喝一次录进 trace，verify 会自动比。",
        "count": len(ids),
        "with_text": len(ids) - len(no_text),
        "no_text": no_text,
        "raw_frames_scanned": n_raw,
        "potions": out,
    }
    with open(OUT, "w", encoding="utf-8") as f:
        json.dump(doc, f, ensure_ascii=False, indent=1)

    print(f"已发现 {len(ids)} 瓶（compendium 区：{section}），扫了 {n_raw} 个原始帧")
    print(f"  拿到游戏原文的 {len(ids) - len(no_text)} 瓶 -> {OUT}")
    if no_text:
        print(f"  还没有文本的 {len(no_text)} 瓶（留空，别猜）：{' '.join(no_text)}")
    if stray:
        print(f"  捡到但不在已发现表里、已剔除的 {len(stray)} 条：{' '.join(stray)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
