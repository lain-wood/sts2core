#!/usr/bin/env python3
r"""导出权威卡表：discovered 卡 id -> 卡面文本（基础版 + 升级版）。

内容表最终要变成生成产物（见 sts2core/CLAUDE.md 第 3 步）。这是那条流水线的
第一段：把权威数据落盘，后面的转换脚本读文件，不再打扰运行中的游戏。

数据从哪来（都是 mod 的只读 HTTP 接口，见 STS2MCP/docs/raw-full.md）：

* `GET /api/v1/compendium` —— `card_library.discovered_ids`。
  **它只有 id 和统计，没有卡面文本**，所以光靠它灌不了内容。
* `GET /api/v1/wiki?query=...&item_type=card` —— 有卡面文本，而且
  **同时给 base 和 upgraded 两个变体**。`content.rs` 里那些靠初代记忆
  占位的 `ops_upg`（御血术基础版、主宰+、预备打击+）正是缺这个。

wiki 是模糊搜索且只搜「本档案已发现」的牌，所以这里逐个 id 去查，再用
精确 id 匹配收结果 —— 模糊排序的第一名不一定是要的那张。

    & $PY tools/dump_catalog.py
    & $PY tools/dump_catalog.py --out traces/cards_catalog.json
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.parse

from record_trace import BASE_URL, ROOT, _request

COMPENDIUM_URL = BASE_URL + "/api/v1/compendium"
WIKI_URL = BASE_URL + "/api/v1/wiki"


def discovered_card_ids() -> list[str]:
    data = _request("GET", COMPENDIUM_URL, timeout=60.0)
    sections = (data or {}).get("sections") or {}
    lib = sections.get("card_library") or {}
    ids = lib.get("discovered_ids") or []
    if not ids:
        raise SystemExit(
            "错误：compendium 的 card_library.discovered_ids 是空的。"
            f"（status={lib.get('status')}）原始响应键={list((data or {}).keys())}"
        )
    return [str(i) for i in ids]


def wiki_lookup(card_id: str) -> dict | None:
    """按 id 查一张牌。模糊匹配的第一名不一定对，所以按 id 精确收。"""
    q = urllib.parse.urlencode({"query": card_id, "item_type": "card", "limit": 8})
    data = _request("GET", f"{WIKI_URL}?{q}", timeout=30.0)
    for r in (data or {}).get("results") or []:
        if str(r.get("id")) == card_id:
            return r
    return None


def slim(r: dict) -> dict:
    """只留内容表用得上的字段，扔掉 score 之类。

    **keywords 一定要留**：像「覆甲」「虚无」「重放」这些，卡面正文只写
    "获得4层覆甲"，覆甲到底干什么只在 keyword 的 description 里。
    第一版把它扔了，结果建模时无从下手。
    """
    out = {
        "id": r.get("id"),
        "name": r.get("name"),
        "rarity": r.get("rarity"),
        "type": r.get("type"),
        "is_upgradable": r.get("is_upgradable"),
    }
    for variant in ("base", "upgraded"):
        v = r.get(variant)
        if isinstance(v, dict):
            out[variant] = {
                "cost": v.get("cost"),
                "description": v.get("description"),
                "keywords": [
                    {"name": k.get("name"), "description": k.get("description")}
                    for k in (v.get("keywords") or [])
                    if isinstance(k, dict)
                ],
            }
    return out


def main() -> None:
    ap = argparse.ArgumentParser(description="导出权威卡表")
    ap.add_argument("--out", default=os.path.join(ROOT, "traces", "cards_catalog.json"))
    args = ap.parse_args()

    ids = discovered_card_ids()
    print(f"档案已发现 {len(ids)} 张牌，逐个查 wiki ...")

    cards: dict[str, dict] = {}
    missed: list[str] = []
    for n, cid in enumerate(ids, 1):
        r = wiki_lookup(cid)
        if r is None:
            missed.append(cid)
        else:
            cards[cid] = slim(r)
        if n % 20 == 0 or n == len(ids):
            print(f"  {n}/{len(ids)} ...")

    out = {
        "source": "GET /api/v1/compendium (discovered ids) + GET /api/v1/wiki (rules text)",
        "note": "wiki 只覆盖本档案已发现的牌，不是全游戏目录。没查到的 id 见 not_found。",
        "count": len(cards),
        "not_found": missed,
        "cards": cards,
    }
    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    with open(args.out, "w", encoding="utf-8") as f:
        json.dump(out, f, ensure_ascii=False, indent=1)

    print(f"\n已写入 {args.out}：{len(cards)} 张牌，{len(missed)} 张没查到")
    if missed:
        print("  没查到：" + " ".join(missed), file=sys.stderr)


if __name__ == "__main__":
    main()
