#!/usr/bin/env python3
r"""从 sts2.wiki 扒敌人出招表 -> `traces/enemies_wiki.json`，并**拿实录自检**。

## 为什么要这个

敌人 AI 是内核里唯一一块从来没被对拍碰过的规则：`replay` 一律用
`enemy::UNKNOWN`，敌人打了多少由观测反推后注入。于是 `content.rs` 里每条
出招循环都还是从三五个回合猜的 —— 循环长度、条件、冷却全靠脑补。
这是 L2 跨回合搜索的地基问题。

mod 自带的 `search_wiki` 只覆盖**卡牌和遗物**，没有敌人（它的 `item_type`
只有 all/card/relic），所以只能走外部 wiki。

## 但 wiki 不是权威，游戏才是

这一点必须守住 —— 本仓库的核心教训就是"一个自信地算错的模拟器比没有
模拟器更危险"。所以这个脚本做两件事，缺一不可：

1. 扒数据，落盘成 `enemies_wiki.json`（每条都带 `source: "wiki"`）
2. **拿已录的 trace 自检**：把 wiki 的 HP 和意图标签和实测逐个对，
   打印一张对照表。对不上的地方**以实测为准**。

真正的判决要等 `verify --predict-enemy`（还没做）。在那之前，
wiki 的价值是"有结构的假设"，不是"事实"。

## 用法

    & $PY tools/dump_bestiary.py            # 扒 + 自检
    & $PY tools/dump_bestiary.py --no-fetch # 只用缓存，不联网

详情页会缓存到 `traces/.wiki_cache/`，重跑不重复请求。
"""

from __future__ import annotations

import argparse
import glob
import html
import json
import os
import re
import sys
import time
import urllib.request

from record_trace import ROOT

BASE = "https://sts2.wiki"
LIST_URL = BASE + "/enemies/"
UA = "sts2core-bestiary-dump/1.0 (personal simulator validation)"
DELAY_S = 0.4


def fetch(url: str, cache_dir: str, allow_net: bool) -> str:
    os.makedirs(cache_dir, exist_ok=True)
    key = re.sub(r"[^a-z0-9]+", "_", url.lower()).strip("_") + ".html"
    path = os.path.join(cache_dir, key)
    if os.path.exists(path):
        with open(path, encoding="utf-8") as f:
            return f.read()
    if not allow_net:
        raise SystemExit(f"错误：--no-fetch 但缓存里没有 {url}")
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=30) as r:
        raw = r.read().decode("utf-8", "replace")
    with open(path, "w", encoding="utf-8") as f:
        f.write(raw)
    time.sleep(DELAY_S)
    return raw


# --------------------------------------------------------------------------
# 解析。页面是 Astro 生成的，标记非常规整，正则够用（不引入第三方依赖）。
# --------------------------------------------------------------------------

RE_ARTICLE = re.compile(
    r'<article class="atlas-browser-card atlas-browser-card--enemy"(.*?)</article>',
    re.S,
)
RE_HREF = re.compile(r'href="/enemies/([^"/]+)/"')
RE_SUBHEAD = re.compile(r'class="atlas-browser-card__subhead">([^<]*)<')

RE_ROW = re.compile(
    r'<tr>\s*<td>\s*<span class="atlas-intent[^"]*">([^<]*)</span>.*?'
    r'<strong class="atlas-plain-line">([^<]*)</strong>\s*'
    r'<span class="atlas-muted">([^<]*)</span>.*?'
    r'<p class="atlas-effect">(.*?)</p>',
    re.S,
)
RE_FLOW = re.compile(r'<span class="atlas-flow__step">(.*?)</span>', re.S)


def attr(blob: str, name: str) -> str:
    m = re.search(rf'{name}="([^"]*)"', blob)
    return html.unescape(m.group(1)) if m else ""


def strip_tags(s: str) -> str:
    return html.unescape(re.sub(r"<[^>]+>", " ", s)).strip()


def parse_list(raw: str) -> list[dict]:
    out = []
    for blob in RE_ARTICLE.findall(raw):
        href = RE_HREF.search(blob)
        sub = RE_SUBHEAD.search(blob)
        out.append(
            {
                "slug": href.group(1) if href else "",
                "name": attr(blob, "data-name"),
                "category": attr(blob, "data-category"),
                "acts": attr(blob, "data-acts"),
                "hp_text": html.unescape(sub.group(1)).strip() if sub else "",
            }
        )
    return out


def parse_detail(raw: str) -> dict:
    moves = []
    for intent, name, mid, effect in RE_ROW.findall(raw):
        moves.append(
            {
                "intent": strip_tags(intent),
                "name": strip_tags(name),
                "id": strip_tags(mid),
                "effect": strip_tags(effect),
            }
        )
    pattern = " ".join(strip_tags(p) for p in RE_FLOW.findall(raw))
    return {"moves": moves, "pattern": pattern}


# --------------------------------------------------------------------------
# 自检：拿已录的 trace 和 wiki 对
# --------------------------------------------------------------------------


def slug_of(entity_id: str) -> str:
    """`KIN_PRIEST_0` -> `kin-priest`。trace 里的 entity_id 本来就是英文内部 ID，
    不用靠中文名去猜映射 —— 这是这条路走得通的关键。"""
    s = re.sub(r"_\d+$", "", entity_id or "")
    return s.lower().replace("_", "-")


def resolve_slug(slug: str, wiki: dict[str, dict]) -> tuple[str | None, str]:
    """把 entity_id 推出来的 slug 对到 wiki 的 slug 上，返回 (命中, 靠哪条规则)。

    实测到的四类差异，每一条都对应下面一条规则 —— **规则要报出来**，
    不然对错了也看不见：

        axe-ruby-raider   -> ruby-raider-axe          词序不同
        leaf-slime-m      -> leaf-slime-medium        大小写缩写
        kin-follower      -> kin-follower-minion      召唤物带后缀
        eye-with-teeth    -> eye-with-teeth-minion    同上
    """
    if slug in wiki:
        return slug, "精确"
    if f"{slug}-minion" in wiki:
        return f"{slug}-minion", "补 -minion 后缀"

    expand = {"m": "medium", "s": "small", "l": "large"}
    toks = [expand.get(t, t) for t in slug.split("-")]
    if "-".join(toks) in wiki:
        return "-".join(toks), "展开大小写缩写"

    want = set(toks)
    for cand in wiki:
        ctoks = set(cand.split("-")) - {"minion"}
        if ctoks == want:
            return cand, "词序不同（按词集合匹配）"
    return None, ""


def observed_from_traces() -> dict[str, dict]:
    """从所有 trace 聚合：slug -> {name, max_hp, intents}。"""
    agg: dict[str, dict] = {}
    for path in sorted(glob.glob(os.path.join(ROOT, "traces", "act*.json"))):
        with open(path, encoding="utf-8") as f:
            trace = json.load(f)
        for frame in trace.get("frames") or []:
            for e in (frame.get("obs") or {}).get("enemies", {}).values():
                slug = slug_of(e.get("entity_id"))
                if not slug:
                    continue
                a = agg.setdefault(slug, {"name": e.get("name"), "hp": set(), "labels": set()})
                if e.get("max_hp"):
                    a["hp"].add(e["max_hp"])
                for i in e.get("intents") or []:
                    lab = (i.get("label") or "").strip()
                    if lab:
                        a["labels"].add(f"{i.get('type')}:{lab}")
    return agg


def cross_check(wiki: dict[str, dict]) -> int:
    """打印实测 vs wiki 的对照表，返回对不上的条数。"""
    obs = observed_from_traces()
    if not obs:
        print("（没有已录的 trace，跳过自检）")
        return 0

    print("\n=== 自检：实录 vs wiki（wiki 只是假设，对不上以实测为准）===")
    bad = 0
    for slug, o in sorted(obs.items()):
        hit, how = resolve_slug(slug, wiki)
        w = wiki.get(hit) if hit else None
        hp_seen = sorted(o["hp"])
        if w is None:
            print(f"  ?  {o['name']:<12} ({slug}) 实测 HP {hp_seen} —— wiki 里没有这一条")
            bad += 1
            continue
        via = "" if how == "精确" else f"  [{how} -> {hit}]"
        # wiki 的 HP 文本形如 "74 (78)" 或 "47 - 49 (51 - 53)"，取升华前的区间
        nums = [int(n) for n in re.findall(r"\d+", w["hp_text"].split("(")[0])]
        ok = bool(nums) and all(min(nums) <= h <= max(nums) for h in hp_seen)
        mark = "OK " if ok else "!! "
        if not ok:
            bad += 1
        print(f"  {mark}{o['name']:<12} ({slug}) 实测 HP {hp_seen} / wiki {w['hp_text']}{via}")
        preview = " | ".join(f"{m['name']} {m['effect']}" for m in w["moves"])
        print(f"       wiki 招式: {preview}")
        print(f"       实测标签: {' '.join(sorted(o['labels'])) or '(无)'}")
        if w["pattern"]:
            print(f"       wiki 循环: {w['pattern']}")
    return bad


def main() -> None:
    ap = argparse.ArgumentParser(description="扒敌人出招表并拿实录自检")
    ap.add_argument("--out", default=os.path.join(ROOT, "traces", "enemies_wiki.json"))
    ap.add_argument("--cache", default=os.path.join(ROOT, "traces", ".wiki_cache"))
    ap.add_argument("--no-fetch", action="store_true", help="只用缓存，不联网")
    ap.add_argument("--limit", type=int, help="只抓前 N 个（调试用）")
    args = ap.parse_args()
    allow_net = not args.no_fetch

    print(f"拉列表页 {LIST_URL} ...")
    entries = parse_list(fetch(LIST_URL, args.cache, allow_net))
    if not entries:
        raise SystemExit("错误：列表页一条都没解析出来，页面结构可能变了")
    if args.limit:
        entries = entries[: args.limit]
    print(f"列表页解析出 {len(entries)} 个敌人，逐个拉详情页 ...")

    wiki: dict[str, dict] = {}
    for n, e in enumerate(entries, 1):
        if not e["slug"]:
            continue
        try:
            detail = parse_detail(fetch(f"{BASE}/enemies/{e['slug']}/", args.cache, allow_net))
        except Exception as exc:
            print(f"  !! {e['slug']} 拉取失败：{type(exc).__name__}: {exc}", file=sys.stderr)
            continue
        wiki[e["slug"]] = {**e, **detail, "source": "wiki"}
        if n % 25 == 0 or n == len(entries):
            print(f"  {n}/{len(entries)} ...")

    out = {
        "source": f"{LIST_URL}（社区 wiki，**不是权威**；权威是游戏本身）",
        "note": "每条都是假设。判决要靠 verify --predict-enemy 和实录 trace。",
        "count": len(wiki),
        "enemies": wiki,
    }
    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    with open(args.out, "w", encoding="utf-8") as f:
        json.dump(out, f, ensure_ascii=False, indent=1)
    print(f"\n已写入 {args.out}：{len(wiki)} 个敌人")

    bad = cross_check(wiki)
    if bad:
        print(f"\n!! 有 {bad} 条对不上或查不到 —— 这些**不要**照抄进 content.rs", file=sys.stderr)
    else:
        print("\n自检通过：所有实测过的敌人，wiki 的 HP 都落在实测值上。")


if __name__ == "__main__":
    main()
