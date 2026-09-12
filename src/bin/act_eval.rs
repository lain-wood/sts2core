//! **L3 阶段 3 的验收台** —— 整幕链式评估 + **回溯检验**。
//!
//! ```text
//! cargo run --release --bin act_eval -- traces/act*.json
//! cargo run --release --bin act_eval -- traces/act*.json --rooms "MMRMMMREMMRB" --samples 128
//! cargo run --release --bin act_eval -- traces/act*.json --upgrade-check
//! ```
//!
//! # 两栏，问的不是同一件事
//!
//! | 栏 | 问什么 | 判据 |
//! |---|---|---|
//! | **回溯检验** | 单场的**战损分布**包不包得住实测那一场 | 覆盖率 + **偏的方向**（路线图给阶段 3 的独占判据）|
//! | **整幕链** | 拿这副牌组走完这一幕会不会死 | 截断率 0（硬判据，判红）|
//!
//! 前一栏是后一栏的**前提**：整幕链就是把单场的战损一场一场接起来，
//! 单场那个分布要是系统性偏了，接起来只会把偏差乘上场数。
//!
//! # 回溯检验为什么直到阶段 3 才做
//!
//! `bin/fight_eval` 的模块头写着它**故意不做**，两个理由今天仍然成立，
//! 所以这里是**带着它们读**，而不是假装它们不存在：
//!
//! 1. **trace 末帧不保证是战斗结束**（录到一半的有的是）——
//!    所以这里只认末帧落在**战利品/选牌界面**且场上没有敌人的那些，
//!    其余逐条报"为什么不算"，不当成 0。
//! 2. **实测那一场是玩家打的，这里跑的是我们这套策略** ——
//!    所以覆盖率**不是**"内核对不对"的判据，它是
//!    「**玩家 + 我们这套策略**」两个差的合量。把它读成前者就错了。
//!
//! 那它判得了什么？**判得了偏的方向和大小。** 实测战损系统性落在预测分布的
//! 下沿 ⇒ 我们这套策略比玩家差（或者内核高估了敌人）；落在上沿 ⇒ 反过来。
//! 而整幕链的结论**正比于这个偏差乘以场数** —— 一场偏 3 点血，走十场就是 30 点。
//!
//! # PIT（概率积分变换）：为什么不只报"在不在区间里"
//!
//! 「实测落在 p10–p90 里」是个二值判据，它分不清"刚好在边上"和"正中间"。
//! 这里另报**分位数排名**（实测战损在预测样本里排第几）的直方图：
//! 分布要是标定好的，那个排名应该**均匀**分布在 0–1 上。
//! 堆在低位 = 预测系统性偏高（我们打得更费血），堆在高位 = 反过来。
//!
//! **它不是显著性检验**，样本量也不够做一个 —— 它是一张**形状**图。

use std::collections::BTreeMap;
use std::process::ExitCode;

use sts2core::replay::{parse_trace, Obs, Trace};
use sts2core::rollout::Policy;
use sts2core::synth::act::{
    evaluate_act, paired_act_delta, parse_rooms, upgrade_basics, ActCfg, ActDelta, ActEval,
    ActPlan, Room,
};
use sts2core::synth::encounters::Table;
use sts2core::synth::eval::{evaluate, Dist, EvalCfg, DEFAULT_SAMPLES, EVAL_MAX_TURNS};
use sts2core::synth::from_obs::{corrected_hp, extract, is_fight_start, probe_boss_room, Extracted};
use sts2core::synth::FightSpec;

/// **默认路线**。这是一个 `[判断]`，不是一份实录 —— 打印在报告最上面，
/// `--rooms` 随时换掉。
///
/// 为什么不从语料里数：**录制是抽查的**（父目录的家规「精英 + Boss 必录，
/// 杂兵抽查」），所以 trace 的条数系统性**少于**真实战斗数，
/// 拿它当"这一幕打几场"会低估一大截。
///
/// 这个形状的两个依据：[源码] 每幕 13–15 间房（`BaseNumberOfRooms`），
/// 战斗占几间由路线定；家规的路线优先级是**「休息处最多、杂兵最少」**。
/// 于是取 8 场杂兵 + 1 场精英 + 3 处休息 + Boss。
pub const DEFAULT_ROOMS: &str = "MMRMMMREMMRB";

struct Row {
    file: String,
    skip: Option<String>,
    /// 回溯检验那一栏
    back: Option<Back>,
    back_skip: Option<String>,
    /// 整幕链那一栏
    act_name: String,
    ev: Option<ActEval>,
    upg: Option<ActDelta>,
}

/// 一条语料的回溯检验。
struct Back {
    enc: String,
    /// 实测战损（进场前血量 − 打完之后的血量，两边都含回血）
    observed: i32,
    /// 预测的战损分布（**只有打完并活下来的样本**）
    pred: Dist,
    /// 预测的死亡率 —— 实测那一场玩家是活着走出来的，
    /// 所以这个数高本身就是一条读数
    death_rate: f64,
    /// 实测战损在预测样本里的分位数排名（0–1）。预测一个活样本都没有时 `None`
    rank: Option<f64>,
    n_pred: usize,
}

impl Back {
    /// 判语。**四种，分开数** —— 合成一个"覆盖率"会把方向丢掉。
    fn verdict(&self) -> &'static str {
        match self.rank {
            None => "预测全灭",
            Some(_) if self.observed < self.pred.p10 => "实测更省（低于 p10）",
            Some(_) if self.observed > self.pred.p90 => "实测更费（高于 p90）",
            Some(_) => "在 p10–p90 里",
        }
    }
}

/// 末帧是不是「这场仗打完了、人还活着」。
///
/// **只认战利品/选牌界面且场上没有敌人**。录到一半的 trace 末帧还在
/// `monster` / `boss` 里，那时的血量不是战斗结果 —— 和 `Outcome::truncated`
/// 是同一类东西，混进来就是拿一个没打完的数当结局。
fn finished_fight(o: &Obs) -> Result<i32, String> {
    if !o.enemies.is_empty() {
        return Err(format!("末帧场上还有 {} 只敌人 —— 录到一半", o.enemies.len()));
    }
    if o.state_type != "rewards" && o.state_type != "card_reward" {
        return Err(format!("末帧停在 `{}`，不是战利品界面", o.state_type));
    }
    // `parse_obs` 在没有 `player` 块时给 0/0 —— 那不是"血被打光了"，
    // 是这一帧根本没报玩家。**分不开就不要算**。
    if o.max_hp <= 0 {
        return Err("末帧没有 player 块（血量读不出来）".to_string());
    }
    Ok(o.hp)
}

/// 这条语料属于哪一幕 —— 从**它打的那一场遭遇**反查。
///
/// trace 的 `run.act` 只有 1/2/3，而同一个序号下有两幕
/// （第 1 幕可以是 Overgrowth 也可以是 Underdocks），**那个数认不出幕名**。
/// 遭遇 key 认得出：它在 [源码] 里就是逐幕列的。
fn act_of(table: &Table, defs: &[Option<u16>]) -> Option<String> {
    let ids: Vec<u16> = defs.iter().filter_map(|d| *d).collect();
    if ids.len() != defs.len() || ids.is_empty() {
        return None;
    }
    let (exact, loose) = table.identify(&ids);
    // 唯一命中优先；构成含随机的那些也认得出幕（`all_possible` 也是逐幕列的）
    let key = exact.first().or_else(|| loose.first())?;
    table.act_of_encounter(key).map(|a| a.name.clone())
}

fn label(table: &Table, defs: &[Option<u16>]) -> String {
    if !defs.iter().all(|d| d.is_some()) {
        return "有敌人内核不认识".to_string();
    }
    let ids: Vec<u16> = defs.iter().filter_map(|d| *d).collect();
    let (exact, loose) = table.identify(&ids);
    match (exact.len(), loose.len()) {
        (1, _) => exact[0].clone(),
        (0, 0) => "表里没有".to_string(),
        (0, _) => format!("{}（构成含随机）", loose[0]),
        _ => format!("{} 条都对得上", exact.len()),
    }
}

/// 回溯检验：真牌组 + 真遭遇 + 真战损。
fn backtest(ex: &Extracted, t: &Trace, table: &Table, cfg: &EvalCfg, seed: u64) -> Result<Back, String> {
    let last = t.frames.last().ok_or("没有帧")?;
    let final_hp = finished_fight(&last.obs)?;
    let hp = corrected_hp(ex, seed);
    let observed = hp - final_hp;
    let relics = ex.relic_specs();
    let spec = FightSpec { hp, ..ex.fight_spec(&relics, seed) };
    let ev = evaluate(&spec, cfg);
    if let Some(r) = &ev.refused {
        return Err(format!("拒绝作答：{r}"));
    }
    // 预测的战损：**只有打完并活下来的样本**（和 `FightEval::loss` 同一个分母）
    let mut pred: Vec<i32> =
        ev.samples.iter().filter(|o| !o.truncated && !o.died).map(|o| hp - o.final_hp).collect();
    let n_pred = pred.len();
    let dist = Dist::of(&mut pred);
    let rank = if n_pred == 0 {
        None
    } else {
        let below = pred.iter().filter(|&&v| v < observed).count() as f64;
        let eq = pred.iter().filter(|&&v| v == observed).count() as f64;
        Some((below + 0.5 * eq) / n_pred as f64)
    };
    Ok(Back {
        enc: label(table, &ex.enemy_defs),
        observed,
        pred: dist,
        death_rate: ev.death_rate(),
        rank,
        n_pred,
    })
}

#[allow(clippy::too_many_arguments)]
fn run(
    path: &str,
    table: &Table,
    ecfg: &EvalCfg,
    acfg: &ActCfg,
    bcfg: &ActCfg,
    rooms: &[Room],
    forced_act: Option<&str>,
    seed: u64,
    do_back: bool,
    do_chain: bool,
    upgrade_check: bool,
) -> Row {
    let mut row = Row {
        file: path.rsplit(['/', '\\']).next().unwrap_or(path).to_string(),
        skip: None,
        back: None,
        back_skip: None,
        act_name: String::new(),
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
    let ex = extract(&t, &f0.obs, probe_boss_room(Some(table), &f0.obs));

    if do_back {
        match backtest(&ex, &t, table, ecfg, seed) {
            Ok(b) => row.back = Some(b),
            Err(e) => row.back_skip = Some(e),
        }
    }

    if do_chain {
        let act = match forced_act {
            Some(a) => Some(a.to_string()),
            None => act_of(table, &ex.enemy_defs),
        };
        match act {
            None => {
                row.act_name = "认不出是哪一幕".to_string();
            }
            Some(a) => {
                row.act_name = a.clone();
                let relics = ex.relic_specs();
                let hp = corrected_hp(&ex, seed);
                let spec = FightSpec { hp, ..ex.fight_spec(&relics, seed) };
                let plan = ActPlan::new(&a, rooms);
                let ev = evaluate_act(&spec, &plan, table, acfg);
                if upgrade_check && ev.refused.is_none() {
                    let upg = upgrade_basics(&ex.deck);
                    // **B 臂的 cfg 可能带 `crn_salt`** —— `--crn-off` 那条 A/B，
                    // 见 `ActCfg::crn_salt`。默认 0 ⇒ 和 A 臂逐字共享。
                    let b = evaluate_act(&FightSpec { deck: &upg, ..spec }, &plan, table, bcfg);
                    row.upg = paired_act_delta(&ev, &b);
                }
                row.ev = Some(ev);
            }
        }
    }
    row
}

fn usage() {
    eprintln!("用法: act_eval <trace.json>... [--samples N] [--seed S] [--max-turns N]");
    eprintln!("            [--rooms \"MMRMMMREMMRB\"] [--act 幕名] [--data 目录]");
    eprintln!("            [--all] [--upgrade-check] [--no-chain] [--no-backtest]");
    eprintln!("  --samples       每条链/每场仗采几次（默认 {DEFAULT_SAMPLES}）");
    eprintln!("  --seed          样本种子的基（同一个基 ⇒ 候选之间配对可比）");
    eprintln!("  --max-turns     单场推演的回合上限（默认 {EVAL_MAX_TURNS}）**别为了消红调大**");
    eprintln!("  --rooms         这一幕走哪几间（M 杂兵 / E 精英 / B Boss / R 休息），默认 {DEFAULT_ROOMS}");
    eprintln!("  --act           强制指定幕名（默认从这一场遭遇反查）");
    eprintln!("  --all           逐条印缺口、已知偏差、跳过的原因");
    eprintln!("  --upgrade-check 基础打击/防御全升，**整幕**配对比（慢一倍）");
    eprintln!("  --crn-off       **故意关掉 CRN**（B 臂换一条随机流）—— 归因用，和上一行一起跑");
    eprintln!("  --no-chain      只跑回溯检验");
    eprintln!("  --no-backtest   只跑整幕链");
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        usage();
        return ExitCode::from(2);
    }
    let mut paths = Vec::new();
    let (mut show_all, mut upgrade_check) = (false, false);
    let mut crn_off = false;
    let (mut do_back, mut do_chain) = (true, true);
    let mut data_dir = "data".to_string();
    let mut rooms_str = DEFAULT_ROOMS.to_string();
    let mut forced_act: Option<String> = None;
    let mut ecfg = EvalCfg::default();
    let mut acfg = ActCfg::default();
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
    macro_rules! text {
        ($what:expr) => {
            match it.next() {
                Some(v) => v.clone(),
                None => {
                    eprintln!("{} 后面要跟一个值", $what);
                    return ExitCode::from(2);
                }
            }
        };
    }
    while let Some(a) = it.next() {
        match a.as_str() {
            "--all" => show_all = true,
            "--upgrade-check" => upgrade_check = true,
            "--crn-off" => crn_off = true,
            "--no-chain" => do_chain = false,
            "--no-backtest" => do_back = false,
            "--samples" => {
                let n = num!(usize);
                ecfg.samples = n;
                acfg.samples = n;
            }
            "--max-turns" => {
                let n = num!(usize);
                ecfg.max_turns = n;
                acfg.max_turns = n;
            }
            "--seed" => seed = num!(u64),
            "--rooms" => rooms_str = text!("--rooms"),
            "--act" => forced_act = Some(text!("--act")),
            "--data" => data_dir = text!("--data"),
            _ => paths.push(a.clone()),
        }
    }
    // **`--crn-off` 只动 B 臂**：A 臂照旧，两臂因此差的正好是"共享不共享"。
    // 盐是个定值（不是随机的）—— 同一条命令跑两遍要给出同一个数。
    let bcfg = ActCfg { crn_salt: if crn_off { 0x9E37_79B9_7F4A_7C15 } else { 0 }, ..acfg };
    let rooms = match parse_rooms(&rooms_str) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("--rooms 解析不了：{e}");
            return ExitCode::from(2);
        }
    };
    // **遭遇表读不了就整个停下来**，不像 `fight_eval` 那样空着一栏跑 ——
    // 这个台子的两栏都要它（回溯那一栏要认遭遇，整幕链更是整条建在它上面）。
    let table = match Table::load(&data_dir) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[!] 遭遇表读不了（{e}）—— 这个台子没有它什么都做不了");
            return ExitCode::from(2);
        }
    };

    let policy_name = match acfg.policy {
        Policy::Fast => "fast",
        Policy::Solver { .. } => "solver+damage_first",
        Policy::Plan(_) => "plan",
    };
    println!(
        "策略 {policy_name} · 每条链/每场 {} 次采样 · 回合上限 {} · 种子基 {seed:#x}",
        acfg.samples, acfg.max_turns
    );
    println!(
        "路线 `{}`（{} 间：杂兵 {} · 精英 {} · Boss {} · 休息 {}）—— **这是一个 `[判断]`，不是实录**，\n\
         录制是抽查的，trace 数不等于战斗数。换路线用 `--rooms`。",
        rooms.iter().map(Room::letter).collect::<String>(),
        rooms.len(),
        rooms.iter().filter(|r| **r == Room::Monster).count(),
        rooms.iter().filter(|r| **r == Room::Elite).count(),
        rooms.iter().filter(|r| **r == Room::Boss).count(),
        rooms.iter().filter(|r| **r == Room::Rest).count(),
    );
    println!(
        "**报的是「这副牌组在我们这套策略手里」的成色** —— 策略评不到的东西（引擎牌）方向是低估。\n"
    );

    let t0 = std::time::Instant::now();
    let rows: Vec<Row> = paths
        .iter()
        .map(|p| {
            run(
                p,
                &table,
                &ecfg,
                &acfg,
                &bcfg,
                &rooms,
                forced_act.as_deref(),
                seed,
                do_back,
                do_chain,
                upgrade_check,
            )
        })
        .collect();
    let elapsed = t0.elapsed();

    // ---- 逐条 ----
    for r in &rows {
        if let Some(s) = &r.skip {
            println!("[跳过] {:<44} {s}", r.file);
            continue;
        }
        if let Some(b) = &r.back {
            println!(
                "[回溯] {:<44} {:<26} 实测战损 {:>4} · 预测 {} (均值 {:.1}, n={}) · 预测死亡 {:.0}% · {}",
                r.file,
                b.enc,
                b.observed,
                b.pred,
                b.pred.mean,
                b.n_pred,
                b.death_rate * 100.0,
                b.verdict(),
            );
        } else if let Some(w) = &r.back_skip {
            if show_all {
                println!("[回溯-] {:<43} {w}", r.file);
            }
        }
        if let Some(ev) = &r.ev {
            let tag = if ev.refused.is_some() {
                "拒绝"
            } else if ev.truncated > 0 {
                "截断"
            } else {
                "整幕"
            };
            println!("[{tag}] {:<44} {:<26} {}", r.file, r.act_name, ev.headline());
            if ev.deaths > 0 {
                // **死在第几间**：一个死亡率读不出"被 Boss 打死"和"一路磨死"的差别
                let by: Vec<String> = ev
                    .deaths_by_room
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| **n > 0)
                    .map(|(k, n)| format!("#{}{}={}", k + 1, ev.rooms[k].letter(), n))
                    .collect();
                println!("       ! 死在：{}", by.join(" "));
            }
            if let Some(d) = &r.upg {
                println!(
                    "       + 全升基础牌：死亡 {:+}（只有原版死 {} / 只有升级版死 {}）· 两边都走完的 {} 条链血量均值 {:+.1}（更好 {} / 更差 {} / 平 {}）",
                    d.deaths_delta, d.only_a_died, d.only_b_died, d.hp.n, d.hp.mean, d.b_better, d.a_better, d.tie
                );
            }
            if show_all {
                for c in ev.caveats() {
                    println!("       ~ {c}");
                }
                if let Some(m) = ev.missing_fights() {
                    println!(
                        "       ~ 平均每条链**漏掉 {m:.1} 场**（内核开不出那几场遭遇）—— 方向是**乐观**"
                    );
                }
                for s in &ev.skipped {
                    println!("       · 没打成 {s}");
                }
                for g in &ev.gaps {
                    println!("       · 缺口 {g}");
                }
            }
        } else if do_chain && !r.act_name.is_empty() && r.ev.is_none() {
            println!("[整幕-] {:<43} {}", r.file, r.act_name);
        }
    }

    let done: Vec<&Row> = rows.iter().filter(|r| r.skip.is_none()).collect();
    println!();
    println!(
        "== {} 条语料：可用 {} · 跳过 {}（{:.1} 秒）",
        rows.len(),
        done.len(),
        rows.len() - done.len(),
        elapsed.as_secs_f64()
    );

    // ---- 回溯检验汇总 ----
    let backs: Vec<&Back> = done.iter().filter_map(|r| r.back.as_ref()).collect();
    if do_back {
        let n_skip = done.iter().filter(|r| r.back.is_none()).count();
        println!(
            "\n-- **回溯检验**（路线图给阶段 3 的独占判据）：{} 条打完了的实录 · {} 条不算",
            backs.len(),
            n_skip
        );
        let with_rank: Vec<&&Back> = backs.iter().filter(|b| b.rank.is_some()).collect();
        let inside = with_rank
            .iter()
            .filter(|b| b.observed >= b.pred.p10 && b.observed <= b.pred.p90)
            .count();
        let span = with_rank
            .iter()
            .filter(|b| b.observed >= b.pred.min && b.observed <= b.pred.max)
            .count();
        let below = with_rank.iter().filter(|b| b.observed < b.pred.p10).count();
        let above = with_rank.iter().filter(|b| b.observed > b.pred.p90).count();
        let allded = backs.len() - with_rank.len();
        println!(
            "   包得住：p10–p90 {inside}/{} · 整个 [min,max] {span}/{}",
            with_rank.len(),
            with_rank.len()
        );
        println!(
            "   包不住的方向：**实测更省血 {below}**（预测偏高 ⇒ 我们这套策略比玩家费血）· 实测更费血 {above} · 预测全灭而玩家活下来 {allded}"
        );
        if !with_rank.is_empty() {
            let mean_obs: f64 =
                with_rank.iter().map(|b| b.observed as f64).sum::<f64>() / with_rank.len() as f64;
            let mean_pred: f64 =
                with_rank.iter().map(|b| b.pred.mean).sum::<f64>() / with_rank.len() as f64;
            println!(
                "   **每场偏 {:+.1} 血**（预测均值 {mean_pred:.1} − 实测均值 {mean_obs:.1}）—— 整幕链的结论正比于它乘以场数",
                mean_pred - mean_obs
            );
            // PIT 直方图：标定好的话应该是**均匀**的
            let mut hist = [0usize; 10];
            for b in &with_rank {
                let r = b.rank.unwrap();
                hist[((r * 10.0) as usize).min(9)] += 1;
            }
            let bar = |n: usize| "#".repeat(n);
            println!("   分位数排名直方图（标定好 ⇒ 均匀；堆在左边 = 预测偏高）：");
            for (i, n) in hist.iter().enumerate() {
                println!("     {:.1}–{:.1} {:>3} {}", i as f64 / 10.0, (i + 1) as f64 / 10.0, n, bar(*n));
            }
        }
        if show_all {
            for r in &done {
                if let Some(w) = &r.back_skip {
                    println!("   不算：{:<40} {w}", r.file);
                }
            }
        }
    }

    // ---- 整幕链汇总 ----
    let evs: Vec<&ActEval> = done.iter().filter_map(|r| r.ev.as_ref()).collect();
    let live: Vec<&&ActEval> = evs.iter().filter(|e| e.refused.is_none()).collect();
    let n_chain: usize = live.iter().map(|e| e.n).sum();
    let n_trunc: usize = live.iter().map(|e| e.truncated).sum();
    let n_deaths: usize = live.iter().map(|e| e.deaths).sum();
    if do_chain {
        let unknown = done.iter().filter(|r| r.ev.is_none() && r.skip.is_none()).count();
        println!(
            "\n-- **整幕链**：{} 副牌组走完这一幕 × {} 条链 = {n_chain} 条 · 认不出是哪一幕 {unknown}",
            live.len(),
            acfg.samples
        );
        println!(
            "-- **截断（硬判据）**：{n_trunc}/{n_chain} 条链{}",
            if n_trunc == 0 { "  ← 0 才算过" } else { "  ← 这些链的血量是假的，本台子因此退非零码" }
        );
        println!("-- 走完这一幕死掉：{n_deaths}/{n_chain} 条链");
        let mut buckets: BTreeMap<&str, usize> = BTreeMap::new();
        for e in &live {
            let r = e.death_rate();
            let k = if r == 0.0 {
                "0%"
            } else if r < 0.25 {
                "0-25%"
            } else if r < 0.5 {
                "25-50%"
            } else if r < 0.75 {
                "50-75%"
            } else if r < 1.0 {
                "75-100%"
            } else {
                "100%"
            };
            *buckets.entry(k).or_insert(0) += 1;
        }
        if !buckets.is_empty() {
            println!(
                "   逐副牌组的死亡率：{}",
                buckets.iter().map(|(k, v)| format!("{k} 的 {v} 副")).collect::<Vec<_>>().join(" · ")
            );
        }
        // **覆盖率必须和结论并排** —— 路线图点名的那一条
        let mut cov: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        for (r, e) in done.iter().filter_map(|r| r.ev.as_ref().map(|e| (r, e))) {
            cov.insert(r.act_name.clone(), e.coverage);
        }
        if !cov.is_empty() {
            println!("-- **这次评估覆盖了这一幕多少**（分子是内核开得出的场数）：");
            for (a, (ok, all)) in &cov {
                println!("   {a:<14} {ok}/{all}");
            }
            let miss: f64 = live.iter().map(|e| e.unsimulated.mean).sum::<f64>() / live.len().max(1) as f64;
            println!(
                "   平均每条链**漏掉 {miss:.1} 场**（抽到了但内核开不出来）—— 那几场没挨打，\n   **死亡率因此是乐观的**，和上面每个数一起读。"
            );
        }
        let mut skip_count: BTreeMap<String, usize> = BTreeMap::new();
        for e in &live {
            for s in &e.skipped {
                *skip_count.entry(s.to_string()).or_insert(0) += 1;
            }
        }
        if !skip_count.is_empty() {
            println!("-- 没打成的那几场（分母 {} 副牌组）：", live.len());
            let mut v: Vec<_> = skip_count.into_iter().collect();
            v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            for (s, n) in v.iter().take(12) {
                println!("   {n:>3}  {s}");
            }
        }
        let ups: Vec<&ActDelta> = done.iter().filter_map(|r| r.upg.as_ref()).collect();
        if !ups.is_empty() {
            // **整幕单调性自检** —— 阶段 2 那条判据的整幕版本。
            // 路线图说死亡率是噪声最大的那一维，这里正是量它的地方：
            // 整幕链多共享了「抽到哪几场遭遇」，死亡那一维应该比单场干净。
            let saved: usize = ups.iter().map(|d| d.only_a_died).sum();
            let lost: usize = ups.iter().map(|d| d.only_b_died).sum();
            let with_hp: Vec<&&ActDelta> = ups.iter().filter(|d| d.hp.n > 0).collect();
            let mean: f64 =
                with_hp.iter().map(|d| d.hp.mean).sum::<f64>() / with_hp.len().max(1) as f64;
            let better = with_hp.iter().filter(|d| d.hp.mean > 0.0).count();
            let worse = with_hp.iter().filter(|d| d.hp.mean < 0.0).count();
            println!(
                "-- **整幕单调性自检**（基础打击/防御全升）：{} 副牌组 —— 救回来的链 {saved} / 反而死掉的 {lost}（比值 {:.1}:1）",
                ups.len(),
                if lost == 0 { f64::INFINITY } else { saved as f64 / lost as f64 }
            );
            println!(
                "   两边都走完的那些链：{} 副里血量均值为正 {better} · 为负 {worse} · 逐副均值再平均 {mean:+.2}",
                with_hp.len()
            );
            println!("   **死亡那一维才是 L3 要判的东西** —— 单场版本（`fight_eval --upgrade-check`）那里是 1.6～2.4:1。");
            if crn_off {
                println!("   **这一轮 `--crn-off`**：B 臂换了一条随机流（遭遇序列和敌人血量都不共享）——");
                println!("   和不带这个开关的那一轮比，差出来的就是**共享那一维买到了多少**。");
            }
        }
    }

    if n_trunc > 0 {
        println!(
            "\n[红] 截断 {n_trunc} 条链。**别抬 --max-turns 去消它** —— 撞上上限的那一场\n\
             没有分出胜负，而它的血量会被带进下一场，后面每一场都建在一个假前提上。"
        );
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
