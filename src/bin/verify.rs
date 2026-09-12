//! 对拍验证器 CLI：读 trace，跑内核，报第一处不一致。
//!
//! ```text
//! cargo run --release --bin verify -- traces/act1_f1.json
//! cargo run --release --bin verify -- traces/*.json --emit-missing missing.json
//! ```
//!
//! 退出码 0 = 没有 MISMATCH。`UNKNOWN_CONTENT` 不影响退出码 —— 内容缺失是
//! 覆盖率问题，不是正确性问题。

use std::process::ExitCode;

use sts2core::replay::{
    parse_trace, verify, verify_enemy_ai, verify_last, verify_per_turn, EnemyAiReport, Report, Verdict,
};

/// 敌人 AI 对拍的报告。四种结局分得很开，因为它们要修的地方完全不同：
///   未知     —— `content.rs` 里没这个敌人
///   对不齐   —— 表里没有任何一手长得像观测到的第一个意图
///   数字不对 —— 招式对上了，但伤害算错（或者力量没跟上）
///   猜错招   —— 出招循环本身是错的
fn print_enemy_ai(r: &EnemyAiReport) {
    for row in &r.rows {
        if row.unknown {
            println!("  [未知  ] {:<14} 内容表里没有这个敌人（或它没有招式）", row.name);
            continue;
        }
        if row.no_alignment {
            println!(
                "  [对不齐] {:<14} 表里没有任何一手对得上它第一次露出的意图",
                row.name
            );
            continue;
        }
        let tag = if row.exact == row.predicted {
            "全中  "
        } else if row.exact + row.kind_only == row.predicted {
            "招对了"
        } else {
            "有猜错"
        };
        println!(
            "  [{}] {:<14} {}/{} 完全一致，{} 只对上类型",
            tag, row.name, row.exact, row.predicted, row.kind_only
        );
        if let Some(d) = &row.first_divergence {
            println!("            首处分歧 {d}");
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("用法: verify <trace.json>... [--emit-missing <out.json>] [--all]");
        eprintln!("  --emit-missing  把「待导入」的牌/敌人写成 JSON，喂给第 3 步灌内容");
        eprintln!("  --all           列出每一帧，不止不一致的那些");
        eprintln!("  --per-turn      整回合预测：一段连续出牌只在开头同步一次，只比末态");
        eprintln!("  --predict-enemy 敌人 AI 对拍：用 EnemyDef 预测下一手意图，和观测比");
        eprintln!("  --last          在线即时对拍：只验最新一帧，不一致时非零退出");
        return ExitCode::from(2);
    }

    let mut paths = Vec::new();
    let mut emit: Option<String> = None;
    let mut show_all = false;
    let mut per_turn = false;
    let mut predict_enemy = false;
    let mut last_only = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--emit-missing" => match it.next() {
                Some(p) => emit = Some(p.clone()),
                None => {
                    eprintln!("--emit-missing 后面要跟一个路径");
                    return ExitCode::from(2);
                }
            },
            "--all" => show_all = true,
            "--per-turn" => per_turn = true,
            "--predict-enemy" => predict_enemy = true,
            "--last" => last_only = true,
            _ => paths.push(a.clone()),
        }
    }

    if last_only {
        let mut had_mismatch = false;
        for p in &paths {
            let src = match std::fs::read_to_string(p) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("读不了 {p}: {e}");
                    return ExitCode::from(2);
                }
            };
            let t = match parse_trace(&src) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("{p}: {e}");
                    return ExitCode::from(2);
                }
            };
            if let Some(res) = verify_last(&t) {
                let tag = match res.verdict {
                    Verdict::Match => "MATCH  ",
                    Verdict::Partial => "PARTIAL",
                    Verdict::Mismatch => "MISMATCH",
                    Verdict::UnknownContent => "UNKNOWN",
                    Verdict::Skipped => "SKIP   ",
                };
                let mark = if res.verdict == Verdict::Mismatch { "[x]" } else { "[OK]" };
                println!("{mark} [{tag}] 帧{:<3} {}", res.i, res.action);
                for n in &res.notes {
                    println!("       · {n}");
                }
                for d in &res.diffs {
                    let dmark = if d.hard { "x" } else { "~" };
                    println!("       {dmark} {:<28} 游戏={:<18} 内核={}", d.field, d.game, d.kernel);
                }
                if res.verdict == Verdict::Mismatch {
                    had_mismatch = true;
                }
            }
        }
        return if had_mismatch {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        };
    }

    // (完全一致, 只对上类型, 总预测数, 对不齐/未知的敌人数, 允许集合大小之和)
    let mut ai_total = (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut reports = Vec::new();
    for p in &paths {
        let src = match std::fs::read_to_string(p) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("读不了 {p}: {e}");
                return ExitCode::from(2);
            }
        };
        let t = match parse_trace(&src) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{p}: {e}");
                return ExitCode::from(2);
            }
        };
        if predict_enemy {
            println!("\n=== {p} — {} ===", t.run);
            let r = verify_enemy_ai(&t);
            print_enemy_ai(&r);
            for row in &r.rows {
                ai_total.0 += row.exact;
                ai_total.1 += row.kind_only;
                ai_total.2 += row.predicted;
                ai_total.4 += row.set_size_sum;
                if row.no_alignment || row.unknown {
                    ai_total.3 += 1;
                }
            }
            continue;
        }
        println!("\n=== {p} — {} — {} 帧{} ===", t.run, t.frames.len(),
            if per_turn { "（整回合）" } else { "" });
        let r = if per_turn { verify_per_turn(&t) } else { verify(&t) };
        print_report(&r, show_all);
        reports.push(r);
    }

    if predict_enemy {
        let (exact, kind, total, blind, setsum) = ai_total;
        let pct = if total > 0 { exact * 100 / total } else { 0 };
        println!(
            "\n出招预测总计：{exact}/{total} 完全一致（{pct}%），另有 {kind} 只对上类型，\
             {blind} 只敌人对不齐/未知"
        );
        // **这一行必须和上面那行一起读。** 指标是「观测到的这一手在不在允许
        // 集合里」，集合越大越容易命中 —— 一个把集合开到全表的实现能拿 100%，
        // 而它什么都没预测。平均 1.00 = 完全确定，超出的部分就是这个分数的水分。
        let avg = if total > 0 { setsum as f64 / total as f64 } else { 0.0 };
        println!(
            "允许集合平均 {avg:.2} 手/次（1.00 = 完全确定；这个数越大，上面那个百分比越不值钱）"
        );
        println!(
            "\n两条已知局限，读数时要扣掉：\
             \n  * 「只对上类型」多半是**我方带易伤**那几回合 —— 意图标签含防御方乘区，\
             \n    而这个模式拿不到当时的玩家 status，不是内核算错\
             \n  * 「对不齐」多半是内容表里那些占位的 `EOp::Nothing`：它表达不出\
             \n    意图类型，沉睡和「有一手但没建模」在这里长得一样\
             \n\n这里验的是「下一手是什么」（出招循环），不是「这一手打多少」——\
             \n后者由默认模式的注入检验（那条已全绿）。两者要分开看。"
        );
        return ExitCode::SUCCESS;
    }

    if let Some(out) = emit {
        match write_missing(&out, &reports) {
            Ok(n) => println!("\n待导入内容已写入 {out}（{n} 项）"),
            Err(e) => eprintln!("写 {out} 失败: {e}"),
        }
    }

    let bad: usize = reports.iter().map(|r| r.count(Verdict::Mismatch)).sum();
    println!();
    if bad == 0 {
        println!("对拍通过：没有 MISMATCH。");
        ExitCode::SUCCESS
    } else {
        println!("对拍失败：{bad} 帧不一致。");
        ExitCode::FAILURE
    }
}

fn print_report(r: &Report, show_all: bool) {
    for res in &r.results {
        let interesting = res.verdict == Verdict::Mismatch || show_all;
        if !interesting {
            continue;
        }
        let tag = match res.verdict {
            Verdict::Match => "MATCH  ",
            Verdict::Partial => "PARTIAL",
            Verdict::Mismatch => "MISMATCH",
            Verdict::UnknownContent => "UNKNOWN",
            Verdict::Skipped => "SKIP   ",
        };
        println!("  [{tag}] 帧{:<3} {}", res.i, res.action);
        for n in &res.notes {
            println!("           · {n}");
        }
        for d in &res.diffs {
            let mark = if d.hard { "✗" } else { "~" };
            println!("           {mark} {:<28} 游戏={:<18} 内核={}", d.field, d.game, d.kernel);
        }
        // 第一处不一致就够了：后面的多半是同一个原因的回响
        if res.verdict == Verdict::Mismatch && !show_all {
            println!("           (只报第一处不一致，加 --all 看全部)");
            break;
        }
    }

    println!(
        "  统计: 一致 {} / 部分 {} / 不一致 {} / 内容缺失 {} / 跳过 {}",
        r.count(Verdict::Match),
        r.count(Verdict::Partial),
        r.count(Verdict::Mismatch),
        r.count(Verdict::UnknownContent),
        r.count(Verdict::Skipped),
    );

    if !r.missing_cards.is_empty() {
        let names: Vec<&str> = r.missing_cards.values().map(|c| c.name.as_str()).collect();
        println!("  待导入的牌 ({}): {}", names.len(), names.join(" "));
    }
    if !r.missing_enemies.is_empty() {
        let names: Vec<String> =
            r.missing_enemies.iter().map(|(n, hp)| format!("{n}({hp}HP)")).collect();
        println!("  待导入的敌人 ({}): {}", names.len(), names.join(" "));
    }
    if !r.missing_potions.is_empty() {
        let names: Vec<String> =
            r.missing_potions.iter().map(|(n, c)| format!("{n}×{c}")).collect();
        println!("  用过的药水（内核无药水模型）: {}", names.join(" "));
    }
    // **没建全的附魔**。这一栏 2026-09-06 之前只往 `Report` 里写、没人印出来 ——
    // 和「没映射的 status」是同一类静默洞：字段有、看不见。
    if !r.unknown_enchantments.is_empty() {
        let ids: Vec<&str> = r.unknown_enchantments.iter().map(|s| s.as_str()).collect();
        println!("  没建全的附魔（这几张牌会被算错，补 content::ENCHANTS）:");
        println!("    {}", ids.join(" "));
    }
    if !r.unmapped_status.is_empty() {
        let ids: Vec<String> =
            r.unmapped_status.iter().map(|(id, n)| format!("{id}×{n}")).collect();
        println!("  没映射的 status（这些字段没被检查，补 replay.rs::map_status）:");
        println!("    {}", ids.join(" "));
    }

    if !r.seen_intents.is_empty() {
        println!("  见过的意图标签（多段攻击的真实格式看这里）:");
        for (ty, labels) in &r.seen_intents {
            let shown: Vec<&str> =
                labels.iter().map(|l| if l.is_empty() { "<空>" } else { l.as_str() }).collect();
            println!("    {ty:<14} {}", shown.join(" "));
        }
    }

    // 时序：这是「对拍到底能不能信」的直接证据
    print!("  时序: 最长结算 {}ms", r.settle_max_ms);
    if r.settle_unstable > 0 {
        print!("；{} 帧超时未稳定 —— 这些帧不可信", r.settle_unstable);
    }
    if r.settle_intermediate > 0 {
        print!("；{} 帧观察到动画中途状态 —— settle 等待是必需的", r.settle_intermediate);
    } else {
        print!("；未观察到动画中途状态");
    }
    println!();
}

/// 输出给第 3 步灌内容用。手写 JSON，因为整个 crate 零依赖。
fn write_missing(path: &str, reports: &[Report]) -> std::io::Result<usize> {
    let mut cards = std::collections::BTreeMap::new();
    let mut enemies = std::collections::BTreeMap::new();
    for r in reports {
        for (k, v) in &r.missing_cards {
            let e = cards.entry(k.clone()).or_insert_with(|| v.clone());
            if !std::ptr::eq(e, v) {
                e.count = e.count.max(v.count);
            }
        }
        for (k, v) in &r.missing_enemies {
            enemies.insert(k.clone(), *v);
        }
    }

    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
    let mut out = String::from("{\n \"cards\": {\n");
    for (i, (name, c)) in cards.iter().enumerate() {
        let comma = if i + 1 == cards.len() { "" } else { "," };
        let cost = c.cost.map(|c| c.to_string()).unwrap_or_else(|| "null".into());
        out.push_str(&format!(
            "  \"{}\": {{ \"id\": \"{}\", \"cost\": {}, \"type\": \"{}\", \"seen\": {}, \"description\": \"{}\" }}{}\n",
            esc(name),
            esc(&c.id),
            cost,
            esc(&c.kind),
            c.count,
            esc(&c.description),
            comma
        ));
    }
    out.push_str(" },\n \"enemies\": {\n");
    for (i, (name, hp)) in enemies.iter().enumerate() {
        let comma = if i + 1 == enemies.len() { "" } else { "," };
        out.push_str(&format!("  \"{}\": {{ \"max_hp\": {} }}{}\n", esc(name), hp, comma));
    }
    out.push_str(" }\n}\n");
    std::fs::write(path, out)?;
    Ok(cards.len() + enemies.len())
}
