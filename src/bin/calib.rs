//! **P6 标定样本导出器**：把「叶局面 → 真实结局」的对照表倒出来。
//!
//! ```text
//! cargo run --release --bin calib -- traces/act*.json --out calib.csv
//! ```
//!
//! # 它要回答的问题
//!
//! 限深 expectimax 的叶子停在「回合开始」，那里的值只能**估**。
//! 估得准不准，唯一诚实的判法是：**拿同一个局面真的推到底，看估值和结局对不对得上。**
//! 这个工具就是把这批对照数据倒出来 —— 它既是叶评估的验收（P6），
//! 也是设计它的依据。**先有数据再写评估函数，不是反过来。**
//!
//! # 一个局面导出两条标签，差值本身就是结论
//!
//! | 标签 | 怎么来的 | 它是什么 |
//! |---|---|---|
//! | `drawn_*` | 从实录那一帧的**真实手牌**推到底 | 「这手牌打下去会怎样」 |
//! | `undrawn_*` | 把手牌**放回抽牌堆重抽**再推到底 | 「这个牌组在这个局面下会怎样」 |
//!
//! 限深搜索的叶子放在**未抽牌**的状态（抽牌是机会节点，不该在叶子里再摊一次），
//! 所以叶评估要预测的是 `undrawn_*`。而 `drawn_* − undrawn_*` 的分布
//! **就是「这一手抽得好不好」的量** —— 它有多大，叶评估就有多少信息注定拿不到。
//! 这个数如果很大，「牌组画像够用」这个前提就不成立。
//!
//! # 三条基线，叶评估必须打得过它们
//!
//! `hp` / `hp + block − 来袭` / 现成的 `score::survive_first`。
//! 一个赢不过「直接看血量」的评估函数不配存在 —— 这条判据写在
//! `tools/calib_report.py` 里。
//!
//! # 标签是**推演**给的，不是实战给的
//!
//! 叶评估要预测的是「从这里按我们自己的策略打下去会怎样」，所以标签只能来自
//! 推演。实战真实结局（`human_end_hp`）口径不同（胜利遗物这些内核在 trace 里
//! 看不到），只做参照列，**不做标签**。实战那一侧的方向判据归 `bin/rollout` 的 P2。

use std::process::ExitCode;

use sts2core::content::{card_ops, playable};
use sts2core::ops::{Kind, Op};
use sts2core::replay::{lookup_card, parse_trace, turn_segments, Act, Replayer};
use sts2core::rollout::{
    can_rollout, predicted_threat, sample_outcomes, Outcome, Policy, ROLLOUT_BUDGET,
};
use sts2core::solver::{replay_line, score, solve_turn_topk};
use sts2core::step::{end_turn_before_draw, open_hand};
use sts2core::state::{State, St};

/// 把手牌放回抽牌堆、重洗、再抽同样多张。
///
/// **这是「回合开始、未抽牌」那个叶局面的构造方式。** 直接把手牌清空是不行的：
/// 那样策略会在一手空牌上结束回合、白挨敌人一整手，量到的是另一件事。
///
/// 两个已知的近似，读数据时要记得：
/// * **保留(retain) / 均衡留下来的手牌会被一起洗回去**，被当成「这回合抽到的」。
/// * 抽牌堆**顶部已知**的那几张（头槌/破灭那类放上去的）也一并打乱了。
///   真正的 planner 里那部分不该当随机 —— 抽牌堆是「已知前缀 + 未知多重集」。
fn redraw_hand(s: &mut State) {
    let n = s.n_hand as usize;
    if n == 0 {
        return;
    }
    for i in 0..n {
        let c = s.hand[i];
        s.draw[s.n_draw as usize] = c;
        s.n_draw += 1;
    }
    s.n_hand = 0;
    // 和 `reshuffle_discard_into_draw` 同一个套路：先按卡牌指纹规范化，再 Fisher-Yates。
    // 相同的牌堆多重集在相同种子下必须洗出相同结果（CRN）。
    let m = s.n_draw as usize;
    let cards = s.cards;
    s.draw[..m].sort_unstable_by_key(|&ix| {
        let c = cards[ix as usize];
        ((c.id as u64) << 32) | ((c.flags as u64) << 16) | (c.bonus as u16 as u64)
    });
    for i in (1..m).rev() {
        let j = sts2core::state::next_below(&mut s.rng.shuffle, i + 1);
        s.draw.swap(i, j);
    }
    // 洗过了，顶上一张确定的都没有。**这里直接写数组，绕开了 `State` 的方法**，
    // 所以维护 `n_draw_known` 的责任落在这一行上。
    s.n_draw_known = 0;
    s.draw_n(n as i32);
}

/// 一张牌的**卡面**输出，只看直给的那几个 op。
///
/// 条件牌、X 费、生成类一概按 0 计 —— 这不是漏，是**画像的定义**：
/// 它要的是「这副牌平均一回合能打出多少」，不是精确求解。
/// 精确那一半是 `solve_turn` 的活，画像只在叶子上顶班。
fn face_of(id: u16, upgraded: bool, bonus: i16) -> (i32, i32) {
    let mut dmg = 0;
    let mut blk = 0;
    for op in card_ops(id, upgraded) {
        match *op {
            Op::Damage { base, hits, .. } | Op::DamageAll { base, hits, .. } => {
                dmg += (base + bonus as i32).max(0) * hits
            }
            Op::DamageIfVuln { base, .. } => dmg += (base + bonus as i32).max(0),
            Op::Block { base } => blk += base,
            _ => {}
        }
    }
    (dmg, blk)
}

#[derive(Default)]
struct Profile {
    cycle: i32,
    distinct: i32,
    exhausted: i32,
    attacks: i32,
    skills: i32,
    powers: i32,
    statuses: i32,
    unplayable: i32,
    dmg: i32,
    blk: i32,
    cost: i32,
}

/// 牌组画像。**数的是「还会转回来的那些牌」**（抽牌堆 + 弃牌堆 + 手牌）；
/// 消耗堆单独记 —— 消耗掉的牌不再参与循环，算进平均值会系统性高估。
fn profile(s: &State) -> Profile {
    let mut p = Profile::default();
    let mut ids: Vec<u64> = Vec::new();
    let visit = |ix: u8, p: &mut Profile, ids: &mut Vec<u64>| {
        let c = s.cards[ix as usize];
        let d = sts2core::content::card(c.id);
        p.cycle += 1;
        ids.push((c.id as u64) << 32 | (c.flags as u64) << 16 | (c.bonus as u16 as u64));
        match d.kind {
            Kind::Attack => p.attacks += 1,
            Kind::Skill => p.skills += 1,
            Kind::Power => p.powers += 1,
            _ => p.statuses += 1,
        }
        if !playable(c.id) {
            p.unplayable += 1;
        }
        let (dm, bk) = face_of(c.id, c.upgraded(), c.bonus);
        p.dmg += dm;
        p.blk += bk;
        p.cost += (if c.upgraded() { d.cost_upg } else { d.cost }).max(0);
    };
    for i in 0..s.n_draw as usize {
        visit(s.draw[i], &mut p, &mut ids);
    }
    for i in 0..s.n_disc as usize {
        visit(s.disc[i], &mut p, &mut ids);
    }
    for i in 0..s.n_hand as usize {
        visit(s.hand[i], &mut p, &mut ids);
    }
    p.exhausted = s.n_exh as i32;
    ids.sort_unstable();
    ids.dedup();
    p.distinct = ids.len() as i32;
    p
}

struct Stats {
    mean: f64,
    p10: i32,
    p50: i32,
    p90: i32,
    death_permil: i32,
    turns: i32,
    trunc: i32,
}

fn stats(o: &[Outcome]) -> Stats {
    // **死了按 0 血算，不按负数。** `final_hp` 在死亡时是当时的血量，
    // 混进均值会让"死得惨"和"活得少"分不开 —— 标定要的是"最后剩多少"。
    let mut hp: Vec<i32> = o.iter().map(|x| if x.died { 0 } else { x.final_hp.max(0) }).collect();
    let mut tn: Vec<i32> = o.iter().map(|x| x.turns as i32).collect();
    hp.sort_unstable();
    tn.sort_unstable();
    let pick = |v: &Vec<i32>, q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
    Stats {
        mean: hp.iter().map(|&x| x as f64).sum::<f64>() / hp.len() as f64,
        p10: pick(&hp, 0.10),
        p50: pick(&hp, 0.50),
        p90: pick(&hp, 0.90),
        death_permil: (o.iter().filter(|x| x.died).count() as f64 / o.len() as f64 * 1000.0).round()
            as i32,
        turns: pick(&tn, 0.50),
        trunc: o.iter().filter(|x| x.truncated).count() as i32,
    }
}

/// 兄弟叶子里给能力线留几个保底名额。见下面 `sibling_rows` 里的长注释。
const SIB_POWER_RESERVE: usize = 2;
/// 兄弟叶子里给伤害线留几个保底名额。
const SIB_DAMAGE_RESERVE: usize = 2;

/// 算 `leaf_power` 那一列用的币值。取 `LEAF` 是因为要标的就是它的 `power`；
/// 导出的是**未打折的原值**，所以 `LEAF.power` 现在是多少都不影响这一列。
const LEAF_W: sts2core::solver::Weights = sts2core::solver::Weights::LEAF;

/// 兄弟叶子表的表头。**一行 = 一个候选线走完之后的那个叶局面。**
const SIB_HEADER: &str = "group,trace,round,cand,line_len,line,leaf_hp,leaf_block,leaf_enemy_hp,leaf_threat,leaf_power,leaf_horizon,leaf_extra_energy,base_hp,base_hp_minus_threat,base_survive,base_damage,base_hponly,base_leaf,label_mean,label_p50,label_death_permil";

/// **一个根下面的兄弟叶子**：搜索真正要在它们之间排序的那一组。
///
/// # 为什么量的是"组内排序"而不是全局相关
///
/// 限深搜索里叶评估**永远只在兄弟之间比较** —— 同一回合、同一场仗、
/// 不同的候选线。全局相关会被"第 1 幕 90 血 vs 第 3 幕 20 血"这种差异主导，
/// 而那种比较搜索一次都不做：一个全局相关 0.9、组内排序全反的评估函数，
/// 会漂亮地通过全局那个门，然后在搜索里指错每一步。
///
/// # 标签怎么来
///
/// 叶子是**未抽牌**的（`end_turn_before_draw` 之后），所以它的值是
/// **对下一手抽牌取期望**：每个样本各自 `open_hand` 再推到底。
/// **同一组兄弟共用同一批种子**（CRN）—— 配对之后，组内的差值里
/// 抽牌噪声抵消掉，剩下的才是候选线本身的差别。
fn sibling_rows(
    s: &State,
    tag: &str,
    round: i32,
    group: usize,
    k: usize,
    samples: usize,
    seed: u64,
    max_turns: usize,
    policy: Policy,
) -> Vec<String> {
    const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;
    let threat = predicted_threat(s);
    // **保底名额 2026-08-31 从 0 打开了**，因为要标定的正是能力那一项。
    //
    // 关着的时候能力线进不了兄弟集合，`leaf_power` 在组内**恒零** ——
    // 那样标定出来的"没信号"是假的，理由和 P6 当年否掉牌组画像的理由长得一样，
    // 但性质完全不同：牌组画像是**结构性**地组内恒定（兄弟共用一副牌组），
    // 而能力状态本来就随"这条线打没打那张能力牌"变，是被剪枝**人为**抹平的。
    // 两者混为一谈就会用错误的理由重演 S2 的结论。
    //
    // 代价照实说：兄弟叶子的构成变了，`Weights::LEAF` 那 30 折的历史读数
    // **不能和今天的直接比**。这是为了标定新参数必须付的钱。
    let cands = solve_turn_topk(
        s,
        &threat,
        score::damage_first,
        ROLLOUT_BUDGET,
        0,
        k,
        SIB_POWER_RESERVE,
        SIB_DAMAGE_RESERVE,
    );
    let mut out = Vec::new();
    for (ci, line) in cands.iter().enumerate() {
        let Some(after) = replay_line(s, line.acts()) else { continue };
        // 打完就结束了的线没有"下一个回合的叶子"，不参与排序
        if after.combat_over {
            continue;
        }
        let leaf = end_turn_before_draw(after);
        if leaf.combat_over {
            continue;
        }
        let lt = predicted_threat(&leaf);
        let leaf_enemy_hp: i32 = (0..leaf.n_enemies as usize)
            .filter(|&e| leaf.enemies[e].alive())
            .map(|e| leaf.enemies[e].hp)
            .sum();

        // 标签：对抽牌取期望。**兄弟共用种子**，见函数头。
        let one = |i: usize| {
            let mut sim = leaf;
            // **必须真的洗一次未知区**：`draw_one` 从数组末尾取牌、不洗牌，
            // 只改 `rng.shuffle` 的话 64 个样本抽到的是同一手牌，
            // 量出来的就不是"叶子的值"而是"某一条固定抽牌序列下的结局"。
            let mut sd = seed ^ (i as u64).wrapping_mul(GOLDEN);
            sts2core::plan::shuffle_unknown(&mut sim, &mut sd);
            sim.rng.shuffle = sd;
            sim.rng.enemy = seed ^ (i as u64 ^ 0xA5A5_A5A5).wrapping_mul(GOLDEN);
            open_hand(&mut sim);
            sts2core::rollout::rollout_outcome_with(sim, max_turns, policy)
        };
        let threads =
            std::thread::available_parallelism().map(|p| p.get()).unwrap_or(1).min(samples.max(1));
        let obs: Vec<Outcome> = if threads <= 1 {
            (0..samples).map(one).collect()
        } else {
            let mut buf: Vec<Option<Outcome>> = vec![None; samples];
            let per = samples.div_ceil(threads);
            std::thread::scope(|sc| {
                for (b, chunk) in buf.chunks_mut(per).enumerate() {
                    let one = &one;
                    sc.spawn(move || {
                        for (j, slot) in chunk.iter_mut().enumerate() {
                            *slot = Some(one(b * per + j));
                        }
                    });
                }
            });
            buf.into_iter().map(|o| o.expect("每个下标都该被写到")).collect()
        };
        let st = stats(&obs);
        let names = sts2core::solver::explain(s, line.acts()).join(" ");
        // 能力那一项的**原值**（未打折）。折扣 `Weights::power` 就是要从这一列
        // 和标签的关系里标出来的，所以这里导原值，不导 `score::leaf` 的结果。
        let leaf_power = sts2core::solver::power_horizon_value(&leaf, &LEAF_W);
        out.push(format!(
            "{group},{tag},{round},{ci},{},{},{},{},{leaf_enemy_hp},{},{leaf_power},{},{},{},{},{},{},{},{},{:.2},{},{}",
            line.acts().len(),
            names.replace(',', " "),
            leaf.player.hp,
            leaf.player.block,
            lt.face_total(),
            sts2core::solver::horizon(&leaf),
            leaf.base_energy - sts2core::solver::BASE_ENERGY,
            // ---- 三条基线 ----
            leaf.player.hp,
            leaf.player.hp + leaf.player.block - lt.face_total(),
            score::survive_first(&leaf),
            score::damage_first(&leaf),
            score::hp_only(&leaf),
            // **能力项要加在这个底座上，不是 `base_survive`**：`Weights::LEAF`
            // 是 `SURVIVE_FIRST` 但 `enemy_hp: 75`，两者在"打残敌人值多少"上
            // 差 2.5 倍。加错底座标出来的折扣不作数。
            score::leaf(&leaf),
            st.mean,
            st.p50,
            st.death_permil,
        ));
    }
    // 少于两条就没有"排序"可言
    if out.len() < 2 {
        return Vec::new();
    }
    out
}

const HEADER: &str = "trace,run,round,to_end,hp,max_hp,block,energy,str,dex,vuln,weak,regen,\
plated,n_enemies,enemy_hp,enemy_str,threat_face,cycle,distinct,exhausted,attacks,skills,powers,\
statuses,unplayable,deck_dmg,deck_blk,deck_cost,eval_survive,drawn_mean,drawn_p10,drawn_p50,\
drawn_p90,drawn_turns,drawn_trunc,drawn_death_permil,undrawn_mean,undrawn_p10,undrawn_p50,\
undrawn_p90,undrawn_death_permil,human_end_hp";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!(
            "用法: calib <trace.json>... [--samples N] [--seed S] [--max-turns N] [--out f.csv]"
        );
        eprintln!("  导出「叶局面 -> 真实结局」的标定样本（P6）。默认 --samples 128。");
        eprintln!("  每个回合起点导两条标签：drawn（真实手牌）/ undrawn（放回重抽）。");
        eprintln!("  --siblings  改导**兄弟叶子**：一个根下的 K 条候选线各走完之后的叶局面。");
        eprintln!("              搜索只在兄弟之间比较，所以要量的是**组内排序**，不是全局相关。");
        eprintln!("  --k N       兄弟模式下每个根留几条候选线（默认 6）");
        return ExitCode::from(2);
    }
    let mut paths = Vec::new();
    let mut samples = 128usize;
    let mut seed = 20_260_825u64;
    let mut max_turns = 40usize;
    let mut out = String::from("calib.csv");
    // 兄弟叶子模式：一个根导 K 行，量的是**组内排序**，见 `sibling_rows`
    let mut siblings = false;
    let mut topk = 6usize;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        macro_rules! num {
            ($t:ty) => {
                match it.next().and_then(|v| v.parse::<$t>().ok()) {
                    Some(n) => n,
                    None => {
                        eprintln!("{a} 后面要跟一个数字");
                        return ExitCode::from(2);
                    }
                }
            };
        }
        match a.as_str() {
            "--siblings" => siblings = true,
            "--k" => topk = num!(usize),
            "--samples" => samples = num!(usize),
            "--seed" => seed = num!(u64),
            "--max-turns" => max_turns = num!(usize),
            "--out" => match it.next() {
                Some(v) => out = v.clone(),
                None => {
                    eprintln!("--out 后面要跟文件名");
                    return ExitCode::from(2);
                }
            },
            _ => paths.push(a.clone()),
        }
    }

    // 标签用的策略就是**验收里默认的那条**。叶评估将来要顶替的正是它的结局，
    // 换了策略标签就得重导 —— 这一条写进 CSV 的文件名说不清，只能靠这里的注释。
    let policy = Policy::Solver { budget: ROLLOUT_BUDGET, score: score::damage_first };
    let mut rows: Vec<String> = Vec::new();
    let mut skipped: std::collections::BTreeMap<String, u32> = Default::default();

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
                eprintln!("{p} 解析失败: {e}");
                return ExitCode::from(2);
            }
        };
        let tag = std::path::Path::new(p)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| p.clone());
        let human_end = t.frames.last().map(|f| f.obs.hp).unwrap_or(-1);
        let segs = turn_segments(&t);
        let n_seg = segs.len();
        let mut r = Replayer::new(&t.run);
        let mut fed = 0usize;

        for (k, &(i, j)) in segs.iter().enumerate() {
            while fed < i {
                r.advance(&t.frames[fed]);
                fed += 1;
            }
            let start = &t.frames[i];
            let acts = &t.frames[i..j.min(t.frames.len())];
            let mut skip = |why: &str| {
                *skipped.entry(why.to_string()).or_insert(0) += 1;
            };
            if !start.obs.is_play_phase {
                skip("这一帧不是出牌阶段");
                continue;
            }
            if start.obs.enemies.iter().any(|e| e.intent_unparsed) {
                skip("意图标签解析不出数字");
                continue;
            }
            if acts
                .iter()
                .any(|f| matches!(f.action, Some(Act::SelectCard { .. }) | Some(Act::Confirm)))
            {
                skip("走了选牌界面");
                continue;
            }
            let mut sy = r.sync(&start.obs);
            if start.obs.hand.iter().any(|c| lookup_card(&c.name).is_none()) {
                skip("手牌里有内容表还没有的牌");
                continue;
            }
            r.identify_enemies(&mut sy.state, &start.obs);
            if !can_rollout(&sy.state) {
                skip("认不出敌人");
                continue;
            }

            let s = sy.state;
            let pr = profile(&s);
            let threat = predicted_threat(&s);
            let enemy_hp: i32 = (0..s.n_enemies as usize)
                .filter(|&e| s.enemies[e].alive())
                .map(|e| s.enemies[e].hp)
                .sum();
            let enemy_str: i32 = (0..s.n_enemies as usize)
                .filter(|&e| s.enemies[e].alive())
                .map(|e| s.enemies[e].get(St::Strength))
                .sum();

            if siblings {
                let g = rows.len();
                let more =
                    sibling_rows(&s, &tag, start.obs.round, g, topk, samples, seed, max_turns, policy);
                if more.is_empty() {
                    skip("候选线不足两条，排不了序");
                } else {
                    rows.extend(more);
                }
                continue;
            }

            let d = stats(&sample_outcomes(&s, samples, seed, max_turns, policy));
            let mut u = s;
            // 重抽用**另一条**种子，免得"这一手"和"重抽的那一手"撞成同一手
            u.rng.shuffle = seed ^ 0x5DEE_CE66_D5A1_1F3B;
            redraw_hand(&mut u);
            let ud = stats(&sample_outcomes(&u, samples, seed ^ 0xA5A5, max_turns, policy));

            let cols: Vec<String> = vec![
                tag.clone(),
                // `Trace::run` 是一句话（"第3幕 第48层 A0 铁甲战士"），
                // 里面可能有逗号，进 CSV 前换成空格。
                t.run.replace(',', " "),
                start.obs.round.to_string(),
                ((n_seg - k) as i32).to_string(),
                s.player.hp.to_string(),
                s.player.max_hp.to_string(),
                s.player.block.to_string(),
                s.energy.to_string(),
                s.player.get(St::Strength).to_string(),
                s.player.get(St::Dexterity).to_string(),
                s.player.get(St::Vulnerable).to_string(),
                s.player.get(St::Weak).to_string(),
                s.player.get(St::Regen).to_string(),
                s.player.get(St::PlatedArmor).to_string(),
                s.n_enemies.to_string(),
                enemy_hp.to_string(),
                enemy_str.to_string(),
                threat.face_total().to_string(),
                pr.cycle.to_string(),
                pr.distinct.to_string(),
                pr.exhausted.to_string(),
                pr.attacks.to_string(),
                pr.skills.to_string(),
                pr.powers.to_string(),
                pr.statuses.to_string(),
                pr.unplayable.to_string(),
                pr.dmg.to_string(),
                pr.blk.to_string(),
                pr.cost.to_string(),
                score::survive_first(&s).to_string(),
                format!("{:.2}", d.mean),
                d.p10.to_string(),
                d.p50.to_string(),
                d.p90.to_string(),
                d.turns.to_string(),
                d.trunc.to_string(),
                d.death_permil.to_string(),
                format!("{:.2}", ud.mean),
                ud.p10.to_string(),
                ud.p50.to_string(),
                ud.p90.to_string(),
                ud.death_permil.to_string(),
                human_end.to_string(),
            ];
            rows.push(cols.join(","));
        }
    }

    let head = if siblings { SIB_HEADER } else { HEADER };
    let body = format!("{head}\n{}\n", rows.join("\n"));
    if let Err(e) = std::fs::write(&out, body) {
        eprintln!("写不了 {out}: {e}");
        return ExitCode::from(2);
    }
    if siblings {
        println!("导出 {} 行兄弟叶子 -> {out}（每行 {samples} 次采样）", rows.len());
    } else {
        println!("导出 {} 条样本 -> {out}（每条 {samples} 次采样 ×2）", rows.len());
    }
    if !skipped.is_empty() {
        println!("跳过的回合：");
        for (w, n) in &skipped {
            println!("  {n:>3} × {w}");
        }
    }
    ExitCode::SUCCESS
}
