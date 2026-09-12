//! **L3 阶段 2 的验收台** —— 把语料里每一场仗**重新打 N 次**，报分布。
//!
//! ```text
//! cargo run --release --bin fight_eval -- traces/act*.json traces/synthetic_*.json
//! cargo run --release --bin fight_eval -- traces/act*.json --samples 128 --all
//! ```
//!
//! # 它和 `synth_audit` 是一对，问的不是同一件事
//!
//! | 台子 | 问什么 | 判据 |
//! |---|---|---|
//! | `synth_audit` | 构造器搭出来的**局面**对不对 | 和 `sync` 逐字段一致（内容覆盖，不判红）|
//! | **本台子** | 那个局面**打得完吗、打成什么样** | **截断率必须 0**（判红）|
//!
//! 阶段 1 结束时审计台的四栏是「逐字段一致 79 · 有差 0 · 区间外 0 · 集合外 0」。
//! **那四个 0 正是本台子的前提**：单场评估要是算错了，不会是因为局面搭错了。
//!
//! # 唯一的硬判据：截断率
//!
//! 撞上回合上限的推演**战斗没有分出胜负**，而它照样会报一个血量 ——
//! 那是本仓库最忌讳的"自信地算错"。所以本台子在截断 > 0 时**退非零码**，
//! 并把那几条的 `enemy_hp_left` 中位印出来：**「差一口气」和「僵住了」
//! 是两个完全不同的问题**（前者抬上限有用，后者是策略选错了目标函数）。
//!
//! # 它**不做**回溯检验，这是有意的
//!
//! 「预测分布包不包得住实测战损」是**阶段 3** 的判据，不是这里的。
//! 两个原因：trace 的最后一帧不保证是战斗结束（录到一半的有的是），
//! 而且实测那一场是**玩家**打的、本台子跑的是**我们这套策略** ——
//! 两个差混在一个数里，那个数什么都判不了。
//! 拿一个半对的对照栏冒充验证，比没有对照更糟。

use std::collections::BTreeMap;
use std::process::ExitCode;

use sts2core::replay::parse_trace;
use sts2core::rollout::Policy;
use sts2core::synth::encounters::Table;
use sts2core::synth::eval::{evaluate, paired_delta, EvalCfg, FightEval, PairedDelta, DEFAULT_SAMPLES, EVAL_MAX_TURNS};
use sts2core::synth::from_obs::{corrected_hp, extract, is_fight_start, probe_boss_room};
use sts2core::content::card;
use sts2core::state::F_UPGRADED;
use sts2core::synth::FightSpec;

struct Row {
    file: String,
    skip: Option<String>,
    enc: String,
    fight: String,
    withheld: Vec<String>,
    ev: Option<FightEval>,
    /// `--upgrade-check`：把基础打击/防御全升一遍，配对比
    upg: Option<PairedDelta>,
}

fn label(table: Option<&Table>, defs: &[Option<u16>]) -> String {
    let Some(t) = table else { return String::new() };
    if !defs.iter().all(|d| d.is_some()) {
        return "遭遇 有敌人内核不认识".to_string();
    }
    let ids: Vec<u16> = defs.iter().filter_map(|d| *d).collect();
    let (exact, loose) = t.identify(&ids);
    match (exact.len(), loose.len()) {
        (1, _) => format!("遭遇 {}", exact[0]),
        (0, 0) => "遭遇 表里没有".to_string(),
        (0, _) => format!("遭遇 {}（构成含随机）", loose[0]),
        _ => format!("遭遇 {} 条都对得上", exact.len()),
    }
}

fn run(path: &str, table: Option<&Table>, cfg: &EvalCfg, seed: u64, upgrade_check: bool) -> Row {
    let mut row = Row {
        file: path.rsplit(['/', '\\']).next().unwrap_or(path).to_string(),
        skip: None,
        enc: String::new(),
        fight: String::new(),
        withheld: Vec::new(),
        ev: None,
        upg: None,
    };
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            row.skip = Some(format!("读不了: {e}"));
            return row;
        }
    };
    let t = match parse_trace(&src) {
        Ok(t) => t,
        Err(e) => {
            row.skip = Some(e);
            return row;
        }
    };
    let Some(f0) = t.frames.first() else {
        row.skip = Some("没有帧".to_string());
        return row;
    };
    if let Err(e) = is_fight_start(&f0.obs) {
        row.skip = Some(e);
        return row;
    }

    let ex = extract(&t, &f0.obs, probe_boss_room(table, &f0.obs));
    row.enc = label(table, &ex.enemy_defs);
    row.fight = ex
        .enemy_names
        .iter()
        .zip(&ex.enemies)
        .map(|(n, e)| format!("{n} {}血", e.hp.unwrap_or(0)))
        .collect::<Vec<_>>()
        .join("+");
    row.withheld = ex.withheld.clone();

    let relics = ex.relic_specs();
    // **开局回血倒推**（见 `from_obs::corrected_hp`）：`FightSpec::hp` 的语义是
    // 「进这场仗之前」，而第 0 帧的观测里小血瓶那 2 点已经回过了。
    let hp = corrected_hp(&ex, seed);
    let spec = FightSpec { hp, ..ex.fight_spec(&relics, seed) };
    let ev = evaluate(&spec, cfg);
    if upgrade_check && ev.refused.is_none() {
        // **单调性自检**：把基础打击/防御全升一遍（5 -> 8，严格更强的一张牌），
        // 配对之后不许更差。只动这两张 —— 它们的升级**无条件更好**，
        // 而"每张牌都升"里混着改费用、改关键字的那些，方向不干净。
        let upg: Vec<_> = ex
            .deck
            .iter()
            .map(|c| {
                let basic = c.id == card::STRIKE || c.id == card::DEFEND;
                if basic { c.upgraded() } else { *c }
            })
            .collect();
        let b = evaluate(&FightSpec { deck: &upg, ..spec }, cfg);
        row.upg = paired_delta(&ev, &b);
    }
    row.ev = Some(ev);
    row
}

/// 给牌组里的一张牌打上升级位。**不新建一张牌** —— 附魔和假升级要原样带过去。
trait DeckCardExt {
    fn upgraded(&self) -> sts2core::synth::DeckCard;
}
impl DeckCardExt for sts2core::synth::DeckCard {
    fn upgraded(&self) -> sts2core::synth::DeckCard {
        sts2core::synth::DeckCard { flags: self.flags | F_UPGRADED, ..*self }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("用法: fight_eval <trace.json>... [--samples N] [--seed S] [--max-turns N] [--all] [--data 目录]");
        eprintln!("  --samples    每场仗采几次（默认 {DEFAULT_SAMPLES}）。**单次结果没有意义**");
        eprintln!("  --seed       样本种子的基（同一个基 ⇒ 候选之间配对可比）");
        eprintln!("  --max-turns  回合上限（默认 {EVAL_MAX_TURNS}）。**它是判据的一部分，别为了消红调大**");
        eprintln!("  --all        逐条印缺口和已知偏差");
        eprintln!("  --upgrade-check  把基础打击/防御全升一遍，配对比 —— **单调性自检**（慢一倍）");
        eprintln!("  --data       遭遇表所在目录（默认 data）");
        return ExitCode::from(2);
    }
    let mut paths = Vec::new();
    let mut show_all = false;
    let mut upgrade_check = false;
    let mut data_dir = "data".to_string();
    let mut cfg = EvalCfg::default();
    let mut seed = 0xA10D_0000_5EEDu64;
    let mut it = args.iter();
    macro_rules! num {
        ($t:ty) => {
            match it.next().and_then(|v| v.parse::<$t>().ok()) {
                Some(v) => v,
                None => {
                    eprintln!("参数要跟一个数");
                    return ExitCode::from(2);
                }
            }
        };
    }
    while let Some(a) = it.next() {
        match a.as_str() {
            "--all" => show_all = true,
            "--upgrade-check" => upgrade_check = true,
            "--samples" => cfg.samples = num!(usize),
            "--max-turns" => cfg.max_turns = num!(usize),
            "--seed" => seed = num!(u64),
            "--data" => match it.next() {
                Some(p) => data_dir = p.clone(),
                None => {
                    eprintln!("--data 后面要跟一个目录");
                    return ExitCode::from(2);
                }
            },
            _ => paths.push(a.clone()),
        }
    }

    let table = match Table::load(&data_dir) {
        Ok(t) => Some(t),
        Err(e) => {
            println!("[!] 遭遇表读不了（{e}）—— 遭遇那一栏整栏空着");
            None
        }
    };

    let policy_name = match cfg.policy {
        Policy::Fast => "fast",
        Policy::Solver { .. } => "solver+damage_first",
        Policy::Plan(_) => "plan",
    };
    println!(
        "策略 {policy_name} · 每场 {} 次采样 · 回合上限 {} · 种子基 {seed:#x}\n\
         **报的是「这副牌组在我们这套策略手里」的成色** —— 策略评不到的东西（引擎牌）\n\
         方向是低估，逐条印在下面。\n",
        cfg.samples, cfg.max_turns
    );

    let t0 = std::time::Instant::now();
    let rows: Vec<Row> =
        paths.iter().map(|p| run(p, table.as_ref(), &cfg, seed, upgrade_check)).collect();
    let elapsed = t0.elapsed();

    for r in &rows {
        if let Some(s) = &r.skip {
            println!("[跳过] {:<44} {s}", r.file);
            continue;
        }
        let ev = r.ev.as_ref().expect("没跳过就一定评过");
        let tag = if ev.refused.is_some() {
            "拒绝"
        } else if ev.truncated > 0 {
            "截断"
        } else {
            "评估"
        };
        println!("[{tag}] {:<44} {:<28} {}", r.file, r.enc, ev.headline());
        if ev.truncated > 0 {
            // **截断了就要能分清是「差一口气」还是「僵住了」** —— 判据是
            // 那几条推演结束时敌人还剩多少血（`Outcome::enemy_hp_left`）。
            let mut left: Vec<i32> =
                ev.samples.iter().filter(|o| o.truncated).map(|o| o.enemy_hp_left).collect();
            left.sort_unstable();
            println!(
                "       ! {} 条撞上 {} 回合上限，敌人剩余血中位 {} —— {}",
                ev.truncated,
                cfg.max_turns,
                left[left.len() / 2],
                r.fight
            );
        }
        if let Some(d) = &r.upg {
            println!(
                "       + 全升基础牌：死亡 {:+}（只有原版死 {} / 只有升级版死 {}）· 两边都活下来的 {} 对血量均值 {:+.1}（更好 {} / 更差 {} / 平 {}）",
                d.deaths_delta, d.only_a_died, d.only_b_died, d.hp.n, d.hp.mean, d.b_better, d.a_better, d.tie
            );
        }
        if show_all {
            for c in ev.caveats() {
                println!("       ~ {c}");
            }
            for g in &ev.gaps {
                println!("       · 缺口 {g}");
            }
            if !r.withheld.is_empty() {
                println!(
                    "       · 没喂给构造器（第 0 帧的牌组已经被它改过）：{}",
                    r.withheld.join(" ")
                );
            }
        }
    }

    // ---- 汇总 ----
    let done: Vec<&Row> = rows.iter().filter(|r| r.skip.is_none()).collect();
    let evals: Vec<&FightEval> = done.iter().filter_map(|r| r.ev.as_ref()).collect();
    let refused: Vec<&&Row> = done.iter().filter(|r| {
        r.ev.as_ref().map(|e| e.refused.is_some()).unwrap_or(false)
    }).collect();
    let live: Vec<&&FightEval> = evals.iter().filter(|e| e.refused.is_none()).collect();
    let n_samples: usize = live.iter().map(|e| e.n).sum();
    let n_trunc: usize = live.iter().map(|e| e.truncated).sum();
    let n_deaths: usize = live.iter().map(|e| e.deaths).sum();

    println!();
    println!(
        "== {} 条语料：评估 {} · 拒绝作答 {} · 跳过 {}（{:.1} 秒）",
        rows.len(),
        live.len(),
        refused.len(),
        rows.len() - done.len(),
        elapsed.as_secs_f64()
    );

    println!(
        "-- **截断（硬判据）**：{n_trunc}/{n_samples} 条{}",
        if n_trunc == 0 {
            "  ← 0 才算过"
        } else {
            "  ← 这些推演的血量是假的，本台子因此退非零码"
        }
    );

    println!(
        "-- 死亡：{n_deaths}/{n_samples} 条样本（分母是样本不是语料）",
    );
    let mut buckets: BTreeMap<&str, usize> = BTreeMap::new();
    for e in &live {
        let r = e.death_rate();
        let k = if r == 0.0 {
            "0%"
        } else if r < 0.10 {
            "0-10%"
        } else if r < 0.50 {
            "10-50%"
        } else if r < 1.0 {
            "50-100%"
        } else {
            "100%"
        };
        *buckets.entry(k).or_insert(0) += 1;
    }
    println!(
        "   逐场死亡率：{}",
        buckets.iter().map(|(k, v)| format!("{k} 的 {v} 场")).collect::<Vec<_>>().join(" · ")
    );
    let mut worst: Vec<(f64, &str, String)> = done
        .iter()
        .filter_map(|r| {
            let e = r.ev.as_ref()?;
            if e.refused.is_some() || e.deaths == 0 {
                return None;
            }
            Some((e.death_rate(), r.file.as_str(), e.headline()))
        })
        .collect();
    worst.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    for (_, f, h) in worst.iter().take(8) {
        println!("   死过的：{f:<40} {h}");
    }

    if !refused.is_empty() {
        println!("-- 拒绝作答（**不是 0 分，是没有分**）：");
        for r in &refused {
            let why = r.ev.as_ref().and_then(|e| e.refused.clone());
            println!(
                "   {:<40} {}",
                r.file,
                why.map(|w| w.to_string()).unwrap_or_default()
            );
        }
    }

    // 已知偏差：**按语料条数报**，好知道"这批读数里有多少条是偏的"。
    let mut caveat_count: BTreeMap<&'static str, usize> = BTreeMap::new();
    for e in &live {
        for c in e.caveats() {
            *caveat_count.entry(c.kind()).or_insert(0) += 1;
        }
    }
    if !caveat_count.is_empty() {
        println!("-- 已知偏差覆盖（分母 {} 场）：", live.len());
        let mut v: Vec<_> = caveat_count.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        for (k, n) in v {
            println!("   {n:>3} 场 {k}");
        }
    }

    let ups: Vec<&PairedDelta> = done.iter().filter_map(|r| r.upg.as_ref()).collect();
    if !ups.is_empty() {
        // **单调性自检**（阶段 3 那条判据的单场版本）：打击/防御 5 -> 8 是
        // 严格更强的一张牌，**一批仗上的方向**必须是正的。
        // 逐场的正负读不了 —— CRN 只共享起点，不共享序列（见 `eval` 模块头）。
        let better = ups.iter().filter(|d| d.hp.mean > 0.0).count();
        let worse = ups.iter().filter(|d| d.hp.mean < 0.0).count();
        let saved: usize = ups.iter().map(|d| d.only_a_died).sum();
        let lost: usize = ups.iter().map(|d| d.only_b_died).sum();
        let with_hp: Vec<&&PairedDelta> = ups.iter().filter(|d| d.hp.n > 0).collect();
        let mean: f64 =
            with_hp.iter().map(|d| d.hp.mean).sum::<f64>() / with_hp.len().max(1) as f64;
        println!(
            "-- **单调性自检**（基础打击/防御全升）：{} 场 —— 救回来的样本 {saved} / 反而死掉的 {lost}",
            ups.len()
        );
        println!(
            "   两边都活下来的那些对：{} 场里血量均值为正 {better} · 为负 {worse} · 逐场均值再平均 {mean:+.2}",
            with_hp.len()
        );
        println!(
            "   **判据是这一批的方向，不是某一场的正负** —— CRN 只共享起点（同一批敌人血量），"
        );
        println!("   牌组一换洗牌流当场分岔，逐场噪声是结构性的。");
    }

    let mut gap_count: BTreeMap<String, usize> = BTreeMap::new();
    for e in &live {
        for g in &e.gaps {
            *gap_count.entry(g.to_string()).or_insert(0) += 1;
        }
    }
    if !gap_count.is_empty() {
        println!("-- 构造器报的缺口（分母 {} 场）：", live.len());
        let mut v: Vec<_> = gap_count.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        for (g, n) in v.iter().take(12) {
            println!("   {n:>3}  {g}");
        }
    }

    if n_trunc > 0 {
        println!(
            "\n[红] 截断 {n_trunc} 条。**别抬 --max-turns 去消它** —— 先看上面每条的\n\
             「敌人剩余血中位」：接近 0 是「差一口气」，几百血是「策略僵住了」。"
        );
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
