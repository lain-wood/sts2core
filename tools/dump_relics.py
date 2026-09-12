#!/usr/bin/env python3
r"""导出权威遗物表 -> `traces/relics_catalog.json`。

和 `dump_catalog.py`（导卡表）**同构**，因为数据来源是同一条路：

* `GET /api/v1/compendium` 的 `relic_collection.discovered_ids` 给 id 全表
* `GET /api/v1/wiki?query=<ID>&item_type=relic` 给权威 name / rarity / description

两条都实测过（2026-08-17）。**限制和卡表一样：只有本档案「已发现」的遗物**
查得到，没见过的查不出来 —— 这不是缺陷，正好对上本仓库"欠定就留空"的规矩。

用法（游戏要开着）：

    & "D:\game mod\sts2core\.venv\Scripts\python.exe" "D:\game mod\sts2core\tools\dump_relics.py"
"""

from __future__ import annotations

import json
import os
import sys
import urllib.parse

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from record_trace import BASE_URL, ROOT, _request  # noqa: E402

COMPENDIUM_URL = BASE_URL + "/api/v1/compendium"
WIKI_URL = BASE_URL + "/api/v1/wiki"
OUT = os.path.join(ROOT, "traces", "relics_catalog.json")


def discovered_ids() -> list[str]:
    d = _request("GET", COMPENDIUM_URL, timeout=60.0)
    sec = (d.get("sections") or {}).get("relic_collection") or {}
    return list(sec.get("discovered_ids") or [])


def wiki_lookup(relic_id: str) -> dict | None:
    """逐个 id 精确查。wiki 是模糊搜索，所以要在结果里挑 id 完全相等的那条。"""
    q = urllib.parse.urlencode({"query": relic_id, "item_type": "relic", "limit": 5})
    data = _request("GET", f"{WIKI_URL}?{q}", timeout=30.0)
    for r in data.get("results") or []:
        if r.get("id") == relic_id:
            return r
    return None


def main() -> int:
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8", errors="replace")
        except Exception:  # noqa: BLE001
            pass
    try:
        ids = discovered_ids()
    except Exception as e:  # noqa: BLE001
        print(f"✗ 第 1 步（读 compendium）失败：{e}")
        print("  游戏没开、或者 mod 端点还没起来。")
        return 2
    if not ids:
        print("✗ compendium 里一个已发现的遗物都没有 —— 档案是空的？")
        return 1

    out: dict[str, dict] = {}
    missing: list[str] = []
    for rid in ids:
        try:
            hit = wiki_lookup(rid)
        except Exception as e:  # noqa: BLE001
            print(f"✗ 第 2 步（查 {rid}）失败：{e}")
            return 2
        if hit is None:
            missing.append(rid)
            continue
        out[rid] = {
            "id": rid,
            "name": hit.get("name"),
            "rarity": hit.get("rarity"),
            "description": hit.get("description", ""),
        }

    doc = {
        "source": "GET /api/v1/wiki?item_type=relic（逐个 id 精确匹配）"
        " + /api/v1/compendium 的 relic_collection.discovered_ids",
        "note": "只含本档案**已发现**的遗物；没见过的查不到，属于正常留空。",
        "count": len(out),
        "not_found": missing,
        "relics": out,
    }
    with open(OUT, "w", encoding="utf-8") as f:
        json.dump(doc, f, ensure_ascii=False, indent=1)
    print(f"已导出 {len(out)}/{len(ids)} 个遗物 -> {OUT}")
    if missing:
        print(f"  wiki 查不到的 {len(missing)} 个：{' '.join(missing)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
