//! L2 验收：拿已录的 trace 当样本，比较**求解器给的线**和**实战打的线**。
//!
//! ```text
//! cargo run --release --bin solve -- traces/act1_f17_boss.json
//! cargo run --release --bin solve -- traces/*.json --score hp
//! ```
//!
//! ## 它在验什么
//!
//! 每个回合都从同一个同步出来的局面出发，用**同一个目标函数**、**同一份威胁**
//! 给两条线打分：
//!
//! * 实战线 —— trace 里我真的打出去的那串牌
//! * 求解线 —— `solve_turn` 搜出来的那串牌
//!
//! 于是有一条硬性的自检：**搜索穷尽时，实战线永远不可能比求解线高分。**
//! 真出现了，那不是"人打得比机器好"，而是求解器有 bug（换序合并撞车、
//! 记录和评估走岔、动作生成漏了一种）。这一行是本工具的主要价值，
//! 比"求解器多打了几点伤害"重要得多。
//!
//! ## 威胁从哪来
//!
//! 从**这一回合开头那一帧**观测到的意图标签来。标签已实测是最终伤害值
//! （攻击方力量、防御方乘区都算进去了，见 `docs/trace-format.md` 约束 4），
//! 所以直接照抄，不再过任何乘区 —— 和 `replay::inject_enemy_attacks` 同一个依据。
//!
//! ## 什么样的回合会被跳过（一律报出来，不静默）
//!
//! * 手牌里有内容表还没有的牌 —— 求解器手上的选项比我当时少，比了不算数
//! * 意图标签解析不出数字 —— 建不出威胁
//! * 手牌/药水里有内容表还没有的东西 —— 求解器的选项集比我当时窄，比了不算数
//! * 走了选牌界面 —— v1 不对拍选牌
//! * 段内抽过牌导致实战线重放不出来 —— 抽牌堆顺序已经丢了（约束 2）

use std::process::ExitCode;

use sts2core::replay::{
    lookup_card, parse_trace, sync_latest, Act, Frame, Obs, Replayer, Synced, Trace,
};
use sts2core::solver::{
    advise_potions, explain, score, score_line, solve_turn_budget, solve_turn_potions,
    PotionPolicy, PotionVerdict, Threat,
};
use sts2core::state::State;
use sts2core::step::Action;

/// 一个回合的比较结果。
enum Turn {
    /// 比不了，附原因
    Skipped { round: i32, why: String },
    Compared(Box<Cmp>),
}

struct Cmp {
    round: i32,
    threat_face: i32,
    human: Vec<String>,
    human_score: i32,
    solver: Vec<String>,
    solver_score: i32,
    baseline: i32,
    nodes: u32,
    complete: bool,
    solver_drew: bool,
    /// 手牌里有内核不认识的牌 —— 求解器的选项集比实战窄，读数时要扣掉
    hand_incomplete: bool,
}

/// 一个回合比出来的结论。分得这么细是因为**要修的地方完全不同**：
/// 「换序」说明我选牌是对的、顺序打错了；「另选」说明我选的牌就不对；
/// 「并列」说明这个回合怎么打都一样，不值得看。
#[derive(PartialEq, Eq, Clone, Copy)]
enum Verdict {
    /// 两条线逐字相同
    Same,
    /// 分数一样，只是另一条同样好的线
    Tie,
    /// 同一堆牌，换个顺序就更好 —— 这正是单回合求解器的看家本领
    Reorder,
    /// 求解器选了不同的牌，而且更好
    Different,
    /// 实战线在穷尽搜索下仍然更高分。**这只可能是 bug。**
    Impossible,
}

impl Cmp {
    fn verdict(&self) -> Verdict {
        let delta = self.solver_score - self.human_score;
        if delta < 0 && self.complete {
            return Verdict::Impossible;
        }
        if self.human == self.solver {
            return Verdict::Same;
        }
        if delta <= 0 {
            return Verdict::Tie;
        }
        let mut a = self.human.clone();
        let mut b = self.solver.clone();
        a.sort();
        b.sort();
        if a == b {
            Verdict::Reorder
        } else {
            Verdict::Different
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("用法: solve <trace.json>... [--score survive|damage|hp] [--budget N] [--all]");
        eprintln!("  --score   目标函数（默认 survive）。两个目标函数给出同一条线时，");
        eprintln!("            这条线的可信度比任何权重讨论都高 —— 值得都跑一遍");
        eprintln!("  --budget  节点预算（默认 200000）。搜不完会在输出里标出来");
        eprintln!("  --all     连「两条线一模一样」的回合也列出来");
        eprintln!("  --plan [D] 只对 --live 有效：额外报一条跨回合 planner 的线（默认 D=2）。");
        eprintln!("            **它是被测对象不是驾驶员** —— 驾驶仍然看上面那条单回合线");
        eprintln!("  --deep-score  深层选线的目标函数（默认 leaf，和叶评估同口径）。");
        eprintln!("            `--deep-score damage` = 2026-09-04 之前的行为，A/B 用它；");
        eprintln!("            `tools/plan_seed_sweep.py` 透传它，那条可重复性读数才配得成对");
        eprintln!("  --window  确定性窗口最多免费借几层（默认 6）。`--window 0` = 2026-09-05");
        eprintln!("            之前的行为，A/B 用它；同样由 `plan_seed_sweep.py` 透传");
        eprintln!("  --plan-set \"k=v,...\"  planner 的任意配置键，和 `bin/rollout --alt` /");
        eprintln!("            `bin/plan_audit --set` **同一份解析**（`Plan::apply`）。");
        eprintln!("            退回阶段 4 之前：--plan-set \"window-score=leaf\"");
        eprintln!("            退回阶段 3 之前：--plan-set \"cand-score=damage,k-certain=off,tt=off\"");
        return ExitCode::from(2);
    }

    let mut paths = Vec::new();
    let mut live = false;
    let mut budget = sts2core::solver::DEFAULT_BUDGET;
    let mut scorer: fn(&State) -> i32 = score::survive_first;
    let mut scorer_name = "survive";
    let mut show_all = false;
    let mut plan_depth: Option<u8> = None;
    let mut plan_seed: Option<u64> = None;
    let mut plan_k: Option<usize> = None;
    let mut plan_power_reserve: Option<usize> = None;
    // 方案 C：`--leaf rollout[:samples:turns]`。量可重复性用它。
    let mut plan_leaf: Option<String> = None;
    // 深层选线的目标函数（`Plan::deep_score`）。**A/B 用它**：
    // `--deep-score damage` = 2026-09-04 之前的行为。
    // 它必须有一个手柄，否则 `tools/plan_seed_sweep.py` 那条"单次可重复性"
    // 就只能拿今天的读数去比一个记在文档里的历史值 —— 那是在比两次运行，
    // 不是配对比较。
    let mut plan_deep_score: Option<fn(&State) -> i32> = None;
    // 确定性窗口（`Plan::window`）。**A/B 用它**：`--window 0` = 2026-09-05
    // 之前的行为。和 `--deep-score` 同一个理由 —— 没有手柄，
    // `tools/plan_seed_sweep.py` 那条可重复性读数就配不成对。
    let mut plan_window: Option<u8> = None;
    // 阶段 3 那三个旋钮（`cand_score` / `k_certain` / `tt_bits`）**不再各开一个
    // 长选项**：键的解析已经收口到 `Plan::apply`，三个验收台共用一份。
    // 再在这里手抄一遍就等于第二份定义。
    let mut plan_set: Option<String> = None;
    let mut plan_explain = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--all" => show_all = true,
            "--live" => live = true,
            "--plan" => plan_depth = Some(2),
            "--plan-explain" => {
                plan_depth = plan_depth.or(Some(2));
                plan_explain = true;
            }
            "--plan-k" => match it.next().and_then(|v| v.parse::<usize>().ok()) {
                Some(n) if n >= 1 => plan_k = Some(n),
                _ => {
                    eprintln!("--plan-k 后面要跟一个 >=1 的数字");
                    return ExitCode::from(2);
                }
            },
            // 能力线保底名额。`--power-reserve 0` = 2026-08-31 之前的行为。
            // 配 `--plan-explain` 用：保底段里的线就是"靠单回合分数进不来"的那些。
            "--leaf" => match it.next() {
                Some(v) => plan_leaf = Some(v.clone()),
                None => {
                    eprintln!("--leaf 后面要跟 eval 或 rollout[:samples:turns]");
                    return ExitCode::from(2);
                }
            },
            // **在这里就判**，不留到 `--plan` 那个分支里 —— 没带 `--plan` 时
            // 一个写错的值会静默不生效，而那种错的表现是"读数看着很正常"。
            "--deep-score" => match it.next().map(|v| v.as_str()) {
                Some("survive") => plan_deep_score = Some(score::survive_first),
                Some("damage") => plan_deep_score = Some(score::damage_first),
                Some("hp") => plan_deep_score = Some(score::hp_only),
                Some("leaf") => plan_deep_score = Some(score::leaf),
                other => {
                    eprintln!("--deep-score 只认 survive/damage/hp/leaf，收到 {other:?}");
                    return ExitCode::from(2);
                }
            },
            // 同上，**在参数解析处就判值**。
            "--window" => match it.next().and_then(|v| v.parse::<u8>().ok()) {
                Some(n) if (n as usize) <= sts2core::plan::HARD_DEPTH_CAP => plan_window = Some(n),
                _ => {
                    eprintln!(
                        "--window 后面要跟 0..={} 的数字（0 = 关掉确定性窗口）",
                        sts2core::plan::HARD_DEPTH_CAP
                    );
                    return ExitCode::from(2);
                }
            },
            "--power-reserve" => match it.next().and_then(|v| v.parse::<usize>().ok()) {
                Some(n) => plan_power_reserve = Some(n),
                _ => {
                    eprintln!("--power-reserve 后面要跟一个数字");
                    return ExitCode::from(2);
                }
            },
            // 任意配置键，解析和 `bin/rollout --alt` / `bin/plan_audit --set`
            // 是**同一份**（`Plan::apply`）。**在参数解析处就判**，
            // 免得一个写错的键名跑到半路才炸。
            "--plan-set" => match it.next() {
                Some(v) => {
                    let mut probe = sts2core::plan::Plan::default();
                    if let Err(e) = probe.apply(v) {
                        eprintln!("--plan-set: {e}");
                        return ExitCode::from(2);
                    }
                    plan_set = Some(v.clone());
                }
                None => {
                    eprintln!("--plan-set 后面要跟一串 key=value（逗号分隔），见 --help");
                    return ExitCode::from(2);
                }
            },
            // 换一个 planner 的全局种子。机会节点的采样（CRN）由它和局面指纹
            // 一起决定，所以**同一个局面换种子 = 换一批抽牌样本**。
            // 它是量"这条 D=2 的线里有多少是采样噪声"的唯一手柄。
            "--plan-seed" => match it.next().and_then(|v| v.parse::<u64>().ok()) {
                Some(n) => plan_seed = Some(n),
                None => {
                    eprintln!("--plan-seed 后面要跟一个数字");
                    return ExitCode::from(2);
                }
            },
            "--plan-depth" => match it.next().and_then(|v| v.parse::<u8>().ok()) {
                Some(n) if n >= 1 => plan_depth = Some(n),
                _ => {
                    eprintln!("--plan-depth 后面要跟一个 >=1 的数字");
                    return ExitCode::from(2);
                }
            },
            "--budget" => match it.next().and_then(|v| v.parse().ok()) {
                Some(n) => budget = n,
                None => {
                    eprintln!("--budget 后面要跟一个数字");
                    return ExitCode::from(2);
                }
            },
            "--score" => match it.next().map(|s| s.as_str()) {
                Some("survive") => {}
                Some("damage") => {
                    scorer = score::damage_first;
                    scorer_name = "damage";
                }
                Some("hp") => {
                    scorer = score::hp_only;
                    scorer_name = "hp";
                }
                Some("mcts") => {
                    scorer = score::mcts_rollout;
                    scorer_name = "mcts";
                }
                other => {
                    eprintln!("--score 只认 survive / damage / hp / mcts，收到 {other:?}");
                    return ExitCode::from(2);
                }
            },
            _ => paths.push(a.clone()),
        }
    }

    if live {
        return match paths.first() {
            Some(p) => {
                live_advise(
                    p,
                    scorer,
                    scorer_name,
                    budget,
                    plan_depth,
                    plan_seed,
                    plan_k,
                    plan_power_reserve,
                    plan_leaf,
                    plan_deep_score,
                    plan_window,
                    plan_set,
                    plan_explain,
                )
            }
            None => {
                eprintln!("--live 要跟一个单帧观测文件");
                ExitCode::from(2)
            }
        };
    }

    println!("目标函数 {scorer_name}，节点预算 {budget}\n");

    let mut n_cmp = 0usize;
    let mut n_same = 0usize;
    let mut n_tie = 0usize;
    let mut n_reorder = 0usize;
    let mut n_better = 0usize;
    let mut n_skip = 0usize;
    let mut n_incomplete = 0usize;
    let mut max_nodes = 0u32;
    // **这个数必须是 0。** 见文件头。
    let mut n_impossible = 0usize;
    let mut skip_why: std::collections::BTreeMap<String, usize> = Default::default();

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
        println!("=== {p} — {} ===", t.run);

        for turn in solve_trace(&t, scorer, budget) {
            match turn {
                Turn::Skipped { round, why } => {
                    n_skip += 1;
                    *skip_why.entry(why.clone()).or_insert(0) += 1;
                    if show_all {
                        println!("  [跳过] 回合{round:<3} {why}");
                    }
                }
                Turn::Compared(c) => {
                    n_cmp += 1;
                    if !c.complete {
                        n_incomplete += 1;
                    }
                    let v = c.verdict();
                    match v {
                        Verdict::Same => n_same += 1,
                        Verdict::Tie => n_tie += 1,
                        Verdict::Reorder => n_reorder += 1,
                        Verdict::Different => n_better += 1,
                        Verdict::Impossible => n_impossible += 1,
                    }
                    max_nodes = max_nodes.max(c.nodes);
                    if show_all || !matches!(v, Verdict::Same) {
                        print_cmp(&c, v);
                    }
                }
            }
        }
        println!();
    }

    println!("---");
    println!(
        "比了 {n_cmp} 个回合：\n\
         \x20 一致   {n_same:>3}  两条线逐字相同\n\
         \x20 并列   {n_tie:>3}  另一条线，同分（这回合怎么打都一样）\n\
         \x20 换序   {n_reorder:>3}  同一堆牌，换个顺序更好 ← 单回合求解器的看家本领\n\
         \x20 另选   {n_better:>3}  求解器选了不同的牌，而且更高分\n\
         \x20 跳过   {n_skip:>3}"
    );
    if n_incomplete > 0 {
        println!(
            "  其中 {n_incomplete} 个回合**没搜完**（预算 {budget}）—— 这些行是启发式排名，不是解"
        );
    } else {
        println!(
            "  全部**穷尽搜完**，最大一个回合 {max_nodes} 节点（预算 {budget}）——\n\
             \x20 真实手牌的规模远没到需要近似的地步，这一层不存在旧模拟器那种满表 approx"
        );
    }
    if !skip_why.is_empty() {
        println!("  跳过的原因：");
        for (why, n) in &skip_why {
            println!("    {n:>3} × {why}");
        }
    }
    if n_impossible > 0 {
        println!(
            "\n✗ 有 {n_impossible} 个回合，实战线在**穷尽搜索**下仍然比求解线高分。\n\
             这不可能是人打得更好，只能是求解器的 bug（换序合并撞车 / 记录和评估走岔 /\n\
             动作生成漏了一种）。先修这个，别读上面的任何结论。"
        );
        return ExitCode::FAILURE;
    }
    println!("\n✓ 没有任何回合的实战线赢过穷尽搜索 —— 求解器至少是自洽的。");
    println!(
        "注意这**不等于**求解器打得对：它只在 L1 已经对拍验证过的规则范围内可信，\n\
         而遗物、复活、多阶段这些 L1 还没有的东西，它同样看不见。"
    );
    ExitCode::SUCCESS
}

fn print_cmp(c: &Cmp, v: Verdict) {
    let delta = c.solver_score - c.human_score;
    let mark = match v {
        Verdict::Same => "一致",
        Verdict::Tie => "并列",
        Verdict::Reorder => "换序",
        Verdict::Different => "另选",
        Verdict::Impossible => "✗不可能",
    };
    println!(
        "  [{mark}] 回合{:<3} 来袭 {} 点{}{}",
        c.round,
        c.threat_face,
        if c.complete { "" } else { "（未搜完）" },
        if c.hand_incomplete { "（手牌有未知牌，求解器选项更少）" } else { "" }
    );
    println!(
        "         实战 {:>7}  {}",
        c.human_score - c.baseline,
        if c.human.is_empty() { "（没出牌）".to_string() } else { c.human.join(" -> ") }
    );
    println!(
        "         求解 {:>7}  {}{}",
        c.solver_score - c.baseline,
        if c.solver.is_empty() { "（没出牌）".to_string() } else { c.solver.join(" -> ") },
        if c.solver_drew { "  [线里有抽牌，分数是一个样本]" } else { "" }
    );
    if delta != 0 {
        println!("         差 {:+}（{} 节点）", delta, c.nodes);
    }
}

// ---------------------------------------------------------------------------
// 实战模式：读一帧观测，给这一回合的出牌线
// ---------------------------------------------------------------------------

/// 单帧观测 -> 这一回合怎么打。
///
/// 输入是 `tools/solve_now.py` 写的**单帧 trace**（复用同一个解析器和同一段
/// 同步代码，免得实战和验收跑在两套实现上）。
fn live_advise(
    path: &str,
    scorer: fn(&State) -> i32,
    name: &str,
    budget: u32,
    plan_depth: Option<u8>,
    plan_seed: Option<u64>,
    plan_k: Option<usize>,
    plan_power_reserve: Option<usize>,
    plan_leaf: Option<String>,
    plan_deep_score: Option<fn(&State) -> i32>,
    plan_window: Option<u8>,
    plan_set: Option<String>,
    plan_explain: bool,
) -> ExitCode {
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("读不了 {path}: {e}");
            return ExitCode::from(2);
        }
    };
    let t = match parse_trace(&src) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(2);
        }
    };
    // **拿最后一帧**：`solve_now.py` 传进来的可能是整条正在录的 trace，
    // 最后一帧才是"现在"。前面那些帧不是摆设 —— 观测里没有的每回合计数器
    // 只能从这一回合的开头一路走过来（见 `replay::sync_latest`）。
    let Some(f) = t.frames.last() else {
        eprintln!("这个文件里一帧都没有");
        return ExitCode::from(2);
    };
    let obs = &f.obs;
    if !obs.is_play_phase && !obs.pending {
        println!("现在不是出牌阶段（state_type={}），没什么可解的。", obs.state_type);
        return ExitCode::SUCCESS;
    }

    let Some((r, mut sy, history)) = sync_latest(&t) else {
        eprintln!("这个文件里一帧都没有");
        return ExitCode::from(2);
    };

    // 游戏开着选牌界面（烙印/发掘/战吼/武装那类）时：
    // 若有历史且内核已走进该 Pending，则正常求解输出选择建议；
    // 仅在单帧无历史（同步未还原 Pending）时才拒绝作答。
    if obs.pending && sy.state.pending == sts2core::state::Pending::None {
        println!(
            "游戏正开着一个选牌界面。**这一层不作答** —— 同步未还原选牌状态（无历史走进该界面），
             硬算出来的线游戏不会接受。请先在游戏里把这个选择做完，再跑一次。"
        );
        return ExitCode::SUCCESS;
    }
    let threat = threat_of(&r, obs);
    // 认出敌人是谁。**单回合求解用不着**（威胁是观测输入），但跨回合推演
    // （`--score mcts`）必须知道对面是谁，否则推的是一场敌人不出手的仗。
    let ident = r.identify_enemies(&mut sy.state, obs);
    // **威胁升级**：认得出招式就把标签换成面板基础值，让叶子在结算那一刻现算。
    // 失败会带回一句原因，下面照实印出来 —— 静默降级正是本仓库最忌讳的那种。
    let (threat, live_why) = upgrade_threat_live(&sy.state, &ident, threat);

    // 槽位 -> 观测里的身份。出牌接口收的是 `entity_id`，而且它**每帧都会重新
    // 编号**（约束 1），所以这里给出的 id 只对**这一帧**有效 —— 打完一张牌
    // 必须重读状态再打下一张。
    let mut slot_id = vec![String::new(); sts2core::state::MAX_ENEMIES];
    let mut slot_name = vec![String::new(); sts2core::state::MAX_ENEMIES];
    for e in &obs.enemies {
        if let Some(s) = r.slot_of_existing(&e.combat_id) {
            slot_id[s] = e.entity_id.clone();
            slot_name[s] = e.name.clone();
        }
    }

    println!(
        "局面  {} | 回合{} | 我 {}/{} 血 格挡 {} | 能量 {}/{}",
        t.run, obs.round, obs.hp, obs.max_hp, obs.block, obs.energy, obs.max_energy
    );
    let unknown: Vec<&str> = obs
        .hand
        .iter()
        .filter(|c| lookup_card(&c.name).is_none())
        .map(|c| c.name.as_str())
        .collect();
    if !unknown.is_empty() {
        println!(
            "      ⚠ 手牌里有内容表还没有的牌：{} —— 求解器**看不见它们**，\
             建议里不会出现，请自己判断",
            unknown.join(" ")
        );
    }
    let mut threat_parts = Vec::new();
    let mut threat_total = 0;
    for s in 0..sts2core::state::MAX_ENEMIES {
        let (d, h) = threat.incoming[s];
        if d > 0 && h > 0 {
            // **现算口径下 `incoming` 装的是基础值**，直接印会把 30 印成 18。
            // 这里按"如果现在就结算"算一遍最终值再印 —— 印出来的数必须是
            // 驾驶员看得懂的那个（和游戏意图标签同一个口径）。
            let shown = if threat.live {
                let face = d + sy.state.enemies[s].get(sts2core::state::St::Strength);
                sts2core::damage::apply_modifiers(face, &sy.state.enemies[s], &sy.state.player)
            } else {
                d
            };
            threat_total += shown * h;
            threat_parts.push(format!("{}({}) {}×{}", slot_name[s], slot_id[s], shown, h));
        }
    }
    if threat.live {
        println!(
            "威胁  **现算口径**：这一回合的来袭会跟着我打的牌变（凌虐/虚弱/巨像/转身都算得进去）"
        );
    } else if let Some(why) = &live_why {
        println!("威胁  冻住的标签（升级失败：{why}）—— 改敌人这一击的手段这一层看不见");
    }
    println!(
        "威胁  {}  合计 {} 点",
        if threat_parts.is_empty() { "（这回合没有攻击意图）".into() } else { threat_parts.join(" | ") },
        threat_total
    );

    // 遗物：内核对哪几件有话可说、哪几件没有，必须当面讲清楚。
    // 现在遗物建模是增量的，**沉默地漏掉一件遗物**是最坏的情况 ——
    // 那正是臂甲今天的处境（游戏给 16 点格挡，内核算 8，而没人被告知）。
    if !obs.relics.is_empty() {
        let mut blind: Vec<String> = Vec::new();
        for r in &obs.relics {
            match sts2core::content::relic_by_id(&r.id) {
                Some(d) if d.modelled => {}
                Some(d) => blind.push(format!("{}（{}）", d.name, d.note)),
                None => blind.push(format!("{}（内容表里没有）", r.name)),
            }
        }
        if blind.is_empty() {
            println!("遗物  {} 件，内核全都认得", obs.relics.len());
        } else {
            println!(
                "遗物  {} 件里有 {} 件**内核看不见**，下面的数字没算上它们：",
                obs.relics.len(),
                blind.len()
            );
            for b in &blind {
                println!("        · {b}");
            }
        }
    }

    // 手牌上**没建全的附魔**。和遗物那一栏同一条理由：沉默地少算一张牌的数值
    // 是最坏的情况。判据是 `EnchantDef::modelled`（表里没有 = 同样没建全）。
    {
        let mut blind: Vec<String> = Vec::new();
        for c in &obs.hand {
            if c.enchant_id.is_empty() || sts2core::replay::enchant_fully_modelled(&c.enchant_id) {
                continue;
            }
            let why = sts2core::content::enchant_by_id(&c.enchant_id.to_ascii_uppercase())
                .map_or("内容表里没有这一种".to_string(), |(_, d)| d.note.to_string());
            blind.push(format!("{}（{} {}）—— {}", c.name, c.enchant_id, c.enchant_amount, why));
        }
        if !blind.is_empty() {
            println!("附魔  手里有 {} 张牌的附魔**内核没建全**，它们的数值会算错：", blind.len());
            for b in &blind {
                println!("        · {b}");
            }
        }
    }

    // 敌人身上**内核看不见的 status**。和上面的遗物是同一条理由，
    // 而这一栏一直是缺的 —— 2026-08-30 实战撞到才补：第 3 幕第 35 层，
    // 活体盾带「盾墙 25」（每个我方回合开始给高塔炮手 25 点格挡），
    // 内核完全不建模，而输出里**一个字都没有**。那一场怎么打全靠它，
    // 求解器却当它不存在。
    //
    // 两类分开讲，因为处置不一样：
    //   · `KNOWN_UNMODELLED`：认得出来、故意没建 —— 说得出卡在哪
    //   · 没映射的：连是什么都不知道 —— 那是内容表的洞
    {
        let mut blind: Vec<String> = Vec::new();
        for (si, e) in obs.enemies.iter().enumerate() {
            let _ = si;
            for (id, amt) in &e.status {
                match sts2core::replay::map_status(id) {
                    Some(st) if sts2core::content::KNOWN_UNMODELLED.contains(&st) => blind
                        .push(format!("{} 的 {}（{}）—— 认得，但内核没建模", e.name, id, amt)),
                    None => blind
                        .push(format!("{} 的 {}（{}）—— 内核不认识这个 status", e.name, id, amt)),
                    _ => {}
                }
            }
        }
        if !blind.is_empty() {
            println!("敌方  有 {} 个 status **内核看不见**，下面的数字没算上它们：", blind.len());
            for b in &blind {
                println!("        · {b}");
            }
        }
    }

    // 每回合计数器的来路要说清楚 —— 它们不在观测里，只能从这一回合走过来。
    // 说不清就等于让人去信一个来路不明的数字。
    let c = &sy.state;
    let counters_live = c.attacks_played != 0
        || c.hp_lost_this_turn != 0
        || c.exhausted_this_turn != 0
        || c.free_attack != 0;
    if history > 0 {
        if counters_live {
            println!(
                "计数  本回合已打出 {} 张攻击牌、失去 {} 点生命、消耗 {} 张牌（走了 {} 帧历史）",
                c.attacks_played, c.hp_lost_this_turn, c.exhausted_this_turn, history
            );
        }
    } else {
        // 没有历史时这些计数一律从 0 起算。手上有读它们的牌就必须点名，
        // 否则踩踏会被算成满费、怨恨会被算成只打一次。
        let sensitive: Vec<&str> = obs
            .hand
            .iter()
            .filter_map(|hc| lookup_card(&hc.name).map(|id| (id, hc)))
            .filter(|(id, _)| depends_on_turn_counters(*id))
            .map(|(_, hc)| hc.name.as_str())
            .collect();
        if !sensitive.is_empty() {
            println!(
                "计数  ⚠ 没有本回合的历史，`本回合已打出/已失去生命/已消耗` 一律按 0 算。
                       手里这些牌读的正是它们：{} —— 数字会**偏保守**（低估自己）。
                       想要准的：用 record_trace.py 边打边录，solve_now.py 会自动接上历史。",
                sensitive.join(" ")
            );
        }
    }

    // 敌人身份：单回合用不着，跨回合推演必须有。认不出就**拒绝作答**。
    let rollout_ok = sts2core::rollout::can_rollout(&sy.state);
    let blind: Vec<&str> =
        ident.iter().filter(|i| !i.usable()).map(|i| i.name.as_str()).collect();
    if !blind.is_empty() {
        println!(
            "敌人  {} 只里认出 {} 只；**没认出**：{} —— 跨回合推演不可用",
            ident.len(),
            ident.len() - blind.len(),
            blind.join(" ")
        );
    }
    if std::ptr::eq(scorer as *const (), score::mcts_rollout as *const ()) && !rollout_ok {
        println!(
            "
✗ 拒绝作答：`--score mcts` 要跨回合推演，而这个局面里有敌人认不出来。
                在认不出的敌人身上推演 = 假设它永远不出手，那个数字比没有更危险。
                改用 `--score survive`（它的威胁来自观测意图，不需要认敌人）。"
        );
        return ExitCode::from(3);
    }

    // 药水单独定价：**机会成本不进目标函数**，见 solver::advise_potions。
    // 默认建议线是「不喝药水」的那条；够门槛的药水再单独列出来。
    let policy = PotionPolicy::HOUSE_RULE;
    let (sol, advice) = advise_potions(&sy.state, &threat, scorer, budget, &policy);
    // 内容表不认识的瓶子。**必须单独数出来并点名** —— `advise_potions` 只返回
    // 认得的那些，拿它的长度当"身上有几瓶"会把不认识的静默吞掉（实战抓到过：
    // 手上 3 瓶印成 2/3）。这和遗物那边"内核必须能说出我不认识它"是同一条。
    let unknown_potions: Vec<String> = obs
        .potions
        .iter()
        .filter(|p| {
            sts2core::replay::map_potion(if p.id.is_empty() { &p.name } else { &p.id })
                == sts2core::state::potion::UNKNOWN
        })
        .map(|p| p.name.clone())
        .collect();
    println!(
        "\n建议（{}，{}，相对不出牌 {:+}）",
        name,
        if sol.complete { format!("穷尽 {} 节点", sol.nodes) } else { format!("**没搜完**，{} 节点，这是启发式排名不是解", sol.nodes) },
        sol.gain()
    );
    if sol.line.is_empty() {
        println!("  （什么都不打最好 —— 手上这些牌打出去都是负收益）");
    }
    // 逐步打印：出第几张、打谁。`hand` 下标随出牌漂移，所以这里报的是
    // **执行到那一步时**的槽位，逐张按顺序打就是对的。
    let mut st = sy.state;
    // **同步那一刻有多少张卡实例。** 下标越过它的，就是内核在这条线里
    // **自己生成出来的牌**（添柴 / 地狱之刃 / 惊逃那类）—— 它们不在牌组里，
    // 而且是从 `GEN_POOL`（真实生成池的一个子集）里掷出来的一个样本。
    // 和"抽牌"完全不是一回事，所以要分开标；混在一起印会让人以为
    // 求解器建议了一张牌组里根本没有的牌。判据和 `replay::current_card_name`
    // 用的是同一条。
    let n_synced = sy.state.n_cards as usize;
    let mut generated = 0usize;
    for (k, a) in sol.line.acts().iter().enumerate() {
        match *a {
            Action::PlayCard { hand, target } => {
                let cix = st.hand[hand as usize] as usize;
                let inst = st.cards[cix];
                let def = sts2core::content::card(inst.id);
                let upg = if inst.upgraded() { "+" } else { "" };
                let tgt = if def.targeted {
                    format!(" -> {} ({})", slot_name[target as usize], slot_id[target as usize])
                } else {
                    String::new()
                };
                let gen = if cix >= n_synced {
                    generated += 1;
                    "   ← **内核生成的牌，你牌组里没有这张**"
                } else {
                    ""
                };
                println!("  {}. 手牌[{}] {}{}{}{}", k + 1, hand, def.name, upg, tgt, gen);
            }
            // **候选集在哪个牌堆，是 `Pending` 说了算，不是永远是手牌。**
            //
            // 2026-08-30 实战撞到：头槌+ 的选牌走 `Pending::DiscardToDrawTop`
            // （候选是**弃牌堆**），而这里一律按手牌下标去查名字 ——
            // 于是印出了「选牌:巨像」，可弃牌堆里那时只有御血术一张。
            // 名字错了不会让内核算错，但会让读输出的人（我）判断错，
            // 而这一行的全部意义就是给人读。
            Action::Choose { hand } => {
                let i = hand as usize;
                let cix = match st.pending {
                    // 候选是弃牌堆
                    sts2core::state::Pending::FetchFromDiscard { .. } | sts2core::state::Pending::DiscardToDrawTop { .. } => {
                        (i < st.n_disc as usize).then(|| st.disc[i])
                    }
                    // 候选是手牌
                    sts2core::state::Pending::ExhaustFromHand { .. }
                    | sts2core::state::Pending::PutToDrawPile { .. }
                    | sts2core::state::Pending::UpgradeInHand { .. } => (i < st.n_hand as usize).then(|| st.hand[i]),
                    // 没挂 Pending 却出现了 Choose：不该发生，但不猜
                    sts2core::state::Pending::None => None,
                };
                match cix {
                    Some(c) => println!(
                        "  {}. 选牌[{}] {}",
                        k + 1,
                        hand,
                        sts2core::content::card(st.cards[c as usize].id).name
                    ),
                    None => println!("  {}. 选牌[{}] <越界>", k + 1, hand),
                }
            }
            Action::UsePotion { slot, target } => {
                let pot_id = if (slot as usize) < sts2core::state::MAX_POTIONS {
                    st.potions[slot as usize]
                } else {
                    0
                };
                let pdef = sts2core::ops::potion_def(pot_id);
                let tgt = if pdef.targeted {
                    format!(" -> {} ({})", slot_name[target as usize], slot_id[target as usize])
                } else {
                    String::new()
                };
                println!("  {}. 使用药水[槽位{}] {}{}", k + 1, slot, pdef.name, tgt);
            }
            Action::EndTurn => println!("  {}. 结束回合", k + 1),
        }
        st = sts2core::step(st, *a);
    }
    if sol.line.drew {
        // **牌序知不知道，是两种完全不同的提示**（2026-08-29 起）。
        // mod 打过 `draw_pile_order` 补丁时抽牌堆整堆都是确定的，
        // `sync` 会把 `n_draw_known` 拉满 —— 这一行照旧说"顺序不可知"就是**假警报**。
        // 假警报的代价很实际：它让人白白重跑，还让人不敢照着线往下打。
        //
        // 判据是「跑完这条线之后，已知前缀还盖得住整个抽牌堆吗」——
        // 中途洗过牌就盖不住了（`reshuffle_discard_into_draw` 把它归 0），
        // 那之后抽到什么确实又变回采样。
        if st.n_draw_known == st.n_draw {
            println!(
                "  · 这条线里有抽牌，但**牌序是确定的**（mod 报了 `draw_pile_order`）——\n\
                 \x20   抽到哪几张不是采样，不用为此重跑。"
            );
        } else {
            println!(
                "  ⚠ 这条线里有抽牌，而且**抽穿了已知牌序**（中途洗过牌）——\n\
                 \x20   洗牌之后的顺序内核不可知，那之后抽到哪几张只是一个样本。\n\
                 \x20   洗牌那一步之后请重跑一次"
            );
        }
    }
    if generated > 0 {
        println!(
            "  ⚠ 这条线里有 {generated} 张**生成的牌**（上面标了的那几行）。\n\
             \x20   它们和抽牌不是一回事：**那些牌不在你的牌组里**，是内核从生成池\n\
             \x20   掷出来的一个样本，而这个池子还只是真实池子的一个子集。\n\
             \x20   **生成之后的那几步不要照着打** —— 先把生成那张打出去，看游戏\n\
             \x20   真给了什么，再重跑一次。"
        );
    }

    let after = threat.end_turn(st);
    if after.player_dead {
        println!("\n结果  ✗ 这样打仍然会死。上面那条线只是死得最慢的一条。");
        // **会死的时候，再搜一条"药水随便喝"的线。**
        //
        // 逐瓶定价（`advise_potions`）回答的是"这瓶值不值门槛"，
        // 它**答不了"两瓶一起喝能不能不死"** —— 而那恰恰是 Boss 战里
        // 最常见的救命组合。2026-08-27 第 2 幕 Boss：单喝任何一瓶都还是死，
        // 敏捷药水（多 2 点格挡×N 张）+ 虚弱药水（削 25%）一起喝才活得下来，
        // 我当时是手算出来的。
        //
        // 只在**已经判死**时才搜，所以不影响正常回合的输出，也不会诱导囤药水。
        let all_slots: u16 = (1u16 << sts2core::state::MAX_POTIONS) - 1;
        let rescue = solve_turn_potions(&sy.state, &threat, scorer, budget, all_slots);
        let alive = sts2core::solver::replay_line(&sy.state, rescue.line.acts())
            .map(|e| !threat.end_turn(e).player_dead)
            .unwrap_or(false);
        if alive {
            println!(
                "        ★ **但把药水放开就有活线**（下面这条不受门槛限制，\n                 \x20         因为「不喝会死」压倒一切）："
            );
            for (k, n) in explain(&sy.state, rescue.line.acts()).iter().enumerate() {
                println!("          {}. {}", k + 1, n);
            }
            let end = sts2core::solver::replay_line(&sy.state, rescue.line.acts())
                .map(|e| threat.end_turn(e));
            if let Some(e) = end {
                println!("          挨完这一手后：我 {} 血", e.player.hp);
            }
        } else {
            println!("        （药水全放开也没有活线 —— 这一回合是真的守不住）");
        }
    } else if after.combat_over {
        println!("\n结果  ✓ 这一回合能打完（敌人全清）");
    } else {
        let lost = obs.hp - after.player.hp;
        let mut ep = Vec::new();
        for s in 0..st.n_enemies as usize {
            if slot_name[s].is_empty() {
                continue;
            }
            ep.push(if after.enemies[s].alive() {
                format!("{} {}", slot_name[s], after.enemies[s].hp.max(0))
            } else {
                format!("{} 死", slot_name[s])
            });
        }
        println!(
            "\n结果  挨完这一手后：我 {} 血（{}）| {}",
            after.player.hp,
            if lost > 0 { format!("-{lost}") } else { "不掉血".into() },
            ep.join(" | ")
        );
    }

    // 药水：逐瓶报价 + 判决。数字和门槛都摆出来，让人能自己推翻它。
    if !advice.is_empty() || !unknown_potions.is_empty() {
        // **不是 `advice.len()`**：那只数了内核认得的瓶子，身上真有几瓶是另一回事。
        // 2026-08-21 实战抓到：手上 3 瓶（其中瓶中精灵内容表没有），印成了「2/3」——
        // 一个静默漏报，正好是本仓库最忌讳的那种"沉默地少算一件东西"。
        let held = advice.len() + unknown_potions.len();
        println!(
            "
药水  门槛 {} 点血（身上 {}/{} 瓶）",
            policy.reserve(),
            held,
            // **不是 `MAX_POTIONS`**（那是数组容量 10）。槽位数是观测量，
            // 3/5 和 3/3 是两个局面：满仓才谈得上"要不要清仓"。
            st.potion_slots
        );
        for a in &advice {
            let tag = match a.verdict {
                PotionVerdict::SaveMyLife => "★ 喝！不喝这回合就死",
                PotionVerdict::Drink => "✓ 建议喝",
                PotionVerdict::Hold => "· 留着",
                PotionVerdict::CrossTurn => "? 不评分（跨回合药水）",
                PotionVerdict::ThreatIsFixed => "? 不评分（威胁是观测常数，这层看不见）",
                PotionVerdict::Automatic => "— 自动触发，喝不了（将要死时才发作）",
            };
            let saved = match a.verdict {
                PotionVerdict::CrossTurn
                | PotionVerdict::ThreatIsFixed
                | PotionVerdict::Automatic => "      —".to_string(),
                _ => format!("省 {:>3} 血", a.hp_saved),
            };
            println!("        [槽{}] {:<8} {}  {}", a.slot, a.name, saved, tag);
            if matches!(a.verdict, PotionVerdict::Drink | PotionVerdict::SaveMyLife) {
                println!(
                    "                 喝了的线: {}",
                    explain(&sy.state, a.line.acts()).join(" -> ")
                );
            }
        }
        for n in &unknown_potions {
            println!("        [槽?] {n:<8}       —  ? 不评分（内容表里没有这瓶）");
        }
        if advice.iter().any(|a| matches!(a.verdict, PotionVerdict::CrossTurn)) {
            println!("        跨回合药水（力量/敏捷/再生）的价值在后面几个回合，");
            println!("        单回合求解器结构性看不见 —— 给个自信的低分比不给分更危险，所以不评。");
        }
        if advice.iter().any(|a| matches!(a.verdict, PotionVerdict::ThreatIsFixed)) {
            println!("        虚弱这类「改敌人这一手打多少」的药水，这一层**结构性看不见** ——");
            println!("        威胁是观测到的意图标签，是个已经定死的常数（见 solver.rs）。");
            println!("        它报的不是「没用」，是「我算不了」，别当成 0 收益。");
        }
    }

    // 三个目标函数的对照。**它们给出同一条线时，这条线的可信度比任何
    // 权重讨论都高** —— 因为那说明结论不依赖我拍脑袋定的那几个权重。
    let names_of = |sc: fn(&State) -> i32| {
        // 同样按「不喝药水」的口径，才和上面那条建议线可比
        explain(&sy.state, solve_turn_potions(&sy.state, &threat, sc, budget, 0).line.acts())
    };
    let a = names_of(score::survive_first);
    let b = names_of(score::damage_first);
    let c = names_of(score::hp_only);
    if a == b && b == c {
        println!("对照  存活优先 / 竞速 / 纯HP 三个目标函数给出同一条线 ✓ 结论不依赖权重");
    } else {
        println!("对照  三个目标函数不一致 —— 这一手是**权衡**，不是唯一解：");
        println!("        存活优先 {}", a.join(" -> "));
        println!("        竞速     {}", b.join(" -> "));
        println!("        纯HP     {}", c.join(" -> "));
    }
    // 跨回合 planner 的线。**只报，不驾驶** —— `Policy::Plan` 至今只在验收里
    // 跑过（P5 配对实测 D=2 比 D=1 好 +2.8 血/场），实战成色没有验过。
    // 报出来是为了攒实战里的分歧样本：分歧本身就是「跨回合的账算不算得出来」
    // 这个问题的证据，而局面录在 trace 里，事后可以逐个复盘。
    if let Some(d) = plan_depth {
        println!();
        if !rollout_ok {
            println!("跨回合  x 不作答：深层要靠 EnemyDef 预测敌人，而这个局面里有敌人认不出来。");
        } else {
            let mut cfg = sts2core::plan::Plan { depth: d, ..Default::default() };
            if let Some(seed) = plan_seed {
                cfg.seed = seed;
            }
            if let Some(k) = plan_k {
                cfg.k = k;
            }
            if let Some(r) = plan_power_reserve {
                cfg.power_reserve = r;
            }
            // 深层选线的目标函数。默认 `score::leaf`（和叶评估同口径），
            // `--deep-score damage` 退回 2026-09-04 之前。值在参数解析处就判过了。
            if let Some(f) = plan_deep_score {
                cfg.deep_score = f;
            }
            // 确定性窗口。`--window 0` 退回 2026-09-05 之前。
            if let Some(w) = plan_window {
                cfg.window = w;
            }
            if let Some(spec) = &plan_leaf {
                let mut lt = spec.split(':');
                if let Some("rollout") = lt.next() {
                    cfg.leaf = sts2core::plan::Leaf::Rollout {
                        samples: lt.next().and_then(|v| v.parse().ok()).unwrap_or(4),
                        turns: lt.next().and_then(|v| v.parse().ok()).unwrap_or(12),
                        tail: sts2core::solver::score::leaf,
                    };
                }
            }
            // `--plan-set` **最后应用**：它是"覆盖"，谁点名谁说了算。
            // 值在参数解析处已经判过了，这里再判一次只是不想吞掉错误。
            if let Some(spec) = &plan_set {
                if let Err(e) = cfg.apply(spec) {
                    eprintln!("--plan-set: {e}");
                    return ExitCode::from(2);
                }
            }
            println!("配置  {}", cfg.describe());
            if plan_explain {
                // 诊断：根候选线 + 各自的深层估值。**它分得开两件事** ——
                // 「这条线根本没进候选」和「进了候选但分低」。
                println!(
                    "候选  K={} 条（窄根 {}）+ 能力保底 {} 条（按 solve_turn_topk 的顺序），\
                     后面是 D={d} 的深层估值：",
                    cfg.k_at(&sy.state),
                    sts2core::plan::root_is_narrow(&sy.state),
                    cfg.power_reserve
                );
                let (cands, n_main) = sts2core::plan::plan_candidates(&sy.state, &cfg, &threat);
                let best = cands.iter().map(|(_, v)| *v).max().unwrap_or(0);
                for (i, (line, v)) in cands.iter().enumerate() {
                    let names = explain(&sy.state, line.acts());
                    let mark = if *v == best { " <= 深层最高" } else { "" };
                    // 保底段：这条线靠自己的单回合分数进不了前 K 条
                    let tag = if i >= n_main { "保底 " } else { "     " };
                    println!(
                        "        {tag}[{i}] 根分 {:>7} · 深层 {:>7}{}  {}",
                        line.score,
                        v,
                        mark,
                        if names.is_empty() { "（什么都不打）".to_string() } else { names.join(" -> ") }
                    );
                }
            }
            let t0 = std::time::Instant::now();
            let rep = sts2core::plan::plan_report(&sy.state, &cfg, &threat);
            let pline = rep.line;
            let ms = t0.elapsed().as_millis();
            // 置换表的**唯一证据**：key 写错的样子就是命中率掉到 0，
            // 而那不会让任何东西变红。
            if rep.tt_probes > 0 {
                println!(
                    "        候选 {} 条 · 置换表 {}/{} = {:.1}% 命中 · 窗口借了 {} 层 · 最深 {} 层",
                    rep.n_cands,
                    rep.tt_hits,
                    rep.tt_probes,
                    100.0 * rep.tt_hits as f64 / rep.tt_probes as f64,
                    rep.stat.window_fired,
                    rep.stat.max_depth
                );
            }
            let pnames = explain(&sy.state, pline.acts());
            let snames = explain(&sy.state, sol.line.acts());
            println!(
                "跨回合  planner D={d}（{ms}ms）—— **被测对象，不是驾驶员**：根回合的威胁用观测意图，"
            );
            println!(
                "        深层用 EnemyDef 预测；目标函数 damage_first、叶评估 Weights::LEAF，"
            );
            println!("        和验收里那条线同一套参数。");
            // 两条线都能把这一仗打完时，分歧**不重要** —— 敌人清空之后
            // 后面几个回合根本不存在，D=2 多看的那一层是空的。不做这个判断
            // 会在每个斩杀回合印一条假分歧，把真正值得看的样本淹掉。
            let both_end = [sol.line.acts(), pline.acts()].iter().all(|acts| {
                sts2core::solver::replay_line(&sy.state, acts)
                    .map(|st| sts2core::end_turn_with_incoming(st, &threat.incoming).combat_over)
                    .unwrap_or(false)
            });
            if pnames == snames {
                println!("        与单回合线**一致** ✓");
            } else if both_end {
                println!("        与单回合线不同，但**两条线都能这一回合打完** —— 分歧不重要：");
                println!("          单回合({name}) {}", snames.join(" -> "));
                println!("          跨回合(D={d})   {}", pnames.join(" -> "));
            } else {
                println!("        与单回合线**不同** —— 这是一个值得复盘的分歧样本：");
                println!("          单回合({name}) {}", snames.join(" -> "));
                println!("          跨回合(D={d})   {}", pnames.join(" -> "));
                println!(
                    "        单回合看不见「留张牌下回合用 / 现在多挨一刀换以后少挨三刀」，"
                );
                println!("        而 D={d} 看不见第 {} 回合以后。要照哪条打自己判断。", d + 1);
            }
        }
    }

    println!(
        "\n提醒  遗物/复活/多阶段 L1 还没有，求解器看不见它们；\n\
         \x20     每打出一张牌都要重读状态（entity_id 会重新编号）。"
    );
    ExitCode::SUCCESS
}

/// 这张牌的行为**依赖观测里没有的每回合计数器**吗？
///
/// 只有这类牌会因为"没有本回合历史"而算错，所以只对它们发警告 ——
/// 满屏警告和没有警告一样没用。
fn depends_on_turn_counters(id: u16) -> bool {
    use sts2core::ops::Op;
    let d = sts2core::content::card(id);
    if d.cost_minus_attacks {
        return true; // 踩踏：每打出一张攻击牌减 1 费
    }
    // 怨恨 / 邪眼 / 被遗忘的仪式：`Op::Conditional` 读的就是这些计数器
    fn has_cond(ops: &[Op]) -> bool {
        ops.iter().any(|o| match o {
            Op::Conditional { then, .. } => {
                let _ = then;
                true
            }
            _ => false,
        })
    }
    has_cond(d.ops) || has_cond(d.ops_upg)
}

/// 把一条 trace 切成回合，逐个回合比。
fn solve_trace(t: &Trace, scorer: fn(&State) -> i32, budget: u32) -> Vec<Turn> {
    let mut out = Vec::new();
    let mut r = Replayer::for_trace(t);
    let n = t.frames.len();
    let mut i = 0usize;
    // 走过的帧要喂给 `Replayer::advance`，**否则观测里没有的每回合计数器在
    // 回合开头一律是 0**。`replay::sync_latest` 的文档注释里记着这个坑，
    // 这里原来漏了：`act2_f31` 第 3、4 回合，绯红披风在回合开始扣的那 1 点血
    // 让怨恨攻击两次；不带这个计数器，两条线的怨恨都只打一下。
    // 两边同样错，所以那条"实战线赢不过穷尽搜索"的自检不受影响 ——
    // 但比的是一个游戏里不存在的局面。2026-08-21 补上。
    let mut fed = 0usize;

    while i + 1 < n {
        if t.frames[i].action.is_none() {
            break;
        }
        while fed < i {
            r.advance(&t.frames[fed]);
            fed += 1;
        }
        // 一个回合 = 从第 i 帧起，到第一个「结束回合」为止（含）。
        // 战斗可能在回合中间就打完了，那时走到 trace 末尾也算一段。
        let mut j = i;
        while j < n && !matches!(t.frames[j].action, Some(Act::EndTurn) | None) {
            j += 1;
        }
        let acts = &t.frames[i..j.min(n)];
        let round = t.frames[i].obs.round;

        out.push(compare_turn(&mut r, &t.frames[i], acts, round, scorer, budget));

        // 跳过「结束回合」那一帧，从下一回合的第一帧继续
        i = j + 1;
    }
    out
}

fn compare_turn(
    r: &mut Replayer,
    start: &Frame,
    acts: &[Frame],
    round: i32,
    scorer: fn(&State) -> i32,
    budget: u32,
) -> Turn {
    // `score` 的签名是定死的 `fn(&State) -> i32`，带不出"我需要推演"这个信息，
    // 只能在这里按函数地址认。丑，但比让 rollout 在瞎的局面上评分强。
    let needs_rollout = std::ptr::eq(scorer as *const (), score::mcts_rollout as *const ());
    let skip = |why: &str| Turn::Skipped { round, why: why.to_string() };

    // 内核不认识的动作：一整个回合作废，理由要具体
    for f in acts {
        match &f.action {
            // 药水已经进内核了，不再整回合跳过。只有**内容表不认识的那瓶**
            // 才跳 —— 那种情况求解器手上根本没有这个选项，比了不算数。
            Some(Act::UsePotion { name, slot, .. }) => {
                let known = *slot < sts2core::state::MAX_POTIONS
                    && start
                        .obs
                        .potions
                        .iter()
                        .find(|p| p.slot == *slot)
                        .map(|p| {
                            sts2core::replay::map_potion(
                                if p.id.is_empty() { &p.name } else { &p.id },
                            ) != sts2core::state::potion::UNKNOWN
                        })
                        .unwrap_or(false);
                if !known {
                    return skip(&format!("内容表里没有这瓶药水（{name}）"));
                }
            }
            _ => {}
        }
    }
    if !start.obs.is_play_phase {
        return skip("这一帧不是出牌阶段");
    }
    if start.obs.enemies.iter().any(|e| e.intent_unparsed) {
        return skip("有意图标签解析不出数字，建不出威胁");
    }

    let mut sy = r.sync(&start.obs);
    let threat = threat_of(r, &start.obs);
    // 认出敌人：只有跨回合推演（`--score mcts`）用得着，单回合的威胁来自观测。
    // 认不出的回合在下面直接跳过，而不是让 rollout 在"敌人不出手"的幻想里评分。
    let ident = r.identify_enemies(&mut sy.state, &start.obs);
    let rollout_ok = sts2core::rollout::can_rollout(&sy.state);
    let hand_incomplete = start.obs.hand.iter().any(|c| lookup_card(&c.name).is_none());

    // 实战线：按牌名在内核手牌里找位置（观测的 slot 在段内会漂移，
    // `verify_per_turn` 踩过同一个坑）
    let human_acts = match human_line(&sy, r, acts) {
        Ok(a) => a,
        Err(why) => return skip(&why),
    };
    let human_names = explain(&sy.state, &human_acts);
    let Some(human_score) = score_line(&sy.state, &threat, scorer, &human_acts) else {
        return skip("实战线在内核里重放不出来（多半是段内抽过牌）");
    };

    if needs_rollout && !rollout_ok {
        let blind: Vec<&str> =
            ident.iter().filter(|i| !i.usable()).map(|i| i.name.as_str()).collect();
        return skip(&format!("跨回合推演认不出敌人（{}）", blind.join(" ")));
    }

    let sol = solve_turn_budget(&sy.state, &threat, scorer, budget);
    let solver_names = explain(&sy.state, sol.line.acts());

    Turn::Compared(Box::new(Cmp {
        round,
        threat_face: threat.face_total(),
        human: human_names,
        human_score,
        solver: solver_names,
        solver_score: sol.line.score,
        baseline: sol.baseline,
        nodes: sol.nodes,
        complete: sol.complete,
        solver_drew: sol.line.drew,
        hand_incomplete,
    }))
}

/// 从观测的意图标签建威胁。下标是**内核槽位**，不是观测里的顺序 ——
/// `entity_id` 会重新编号，只有 `combat_id` 是稳的（约束 1）。
/// 把"冻住的意图标签"升级成**可现算**的威胁（面板基础值）。
///
/// 升级成功之后，搜索里那些**改敌人这一击**的手段才看得见：
/// 凌虐减 10 力量、给它上虚弱、巨像减半、转身改朝向、我自己吃污染。
/// 2026-08-27 第 2 幕 Boss 那一场，求解器就是因为看不见这些而报「必死」，
/// 而实际存在一条把 36 点压到 24 的活线。
///
/// **三条前提缺一不可，缺了就整份退回标签**（不许一半现算一半照打）：
/// 1. 每一只**有攻击意图的**敌人都在内容表里、而且出招指针对齐了；
/// 2. 对齐到的那一手里真的有 `EOp::Attack`（否则我们不知道基础值）；
/// 3. 现算出来的数**和观测标签对得上** —— 这一条是自检：对不上说明我们
///    要么认错了招式、要么漏了一个乘区，那就不该拿这份基础值去搜索。
///
/// 第 3 条特别重要：它把"升级"变成一个**当场可证伪**的动作，而不是一次乐观假设。
fn upgrade_threat_live(
    s: &sts2core::state::State,
    ident: &[sts2core::replay::Identified],
    frozen: Threat,
) -> (Threat, Option<String>) {
    use sts2core::ops::EOp;
    let mut live = Threat::new();
    for id in ident {
        let (Some(def_id), Some(mv)) = (id.def, id.move_ix) else {
            // 有攻击意图却认不出来 ⇒ 整份退回
            if frozen.incoming[id.slot].1 > 0 {
                return (frozen, Some(format!("{} 认不出/没对齐", id.name)));
            }
            continue;
        };
        let def = sts2core::content::enemy_def(def_id);
        let Some(m) = def.moves.get(mv) else { continue };
        let mut hits = 0i32;
        let mut base = 0i32;
        for (oi, op) in m.ops.iter().enumerate() {
            // 进阶收口，见 `asc::adjust`。漏了它高进阶下面那条"现算一遍必须和
            // 观测标签逐字相同"的自检会失败，于是整份退回冻住的标签 ——
            // 安全，但看不见任何改敌人这一击的手段。
            if let EOp::Attack { base: b, hits: h } =
                sts2core::asc::adjust(s.enemy_def[id.slot], mv, oi, s.ascension, *op)
            {
                base = b;
                hits += h;
            }
        }
        if hits == 0 {
            // 这一手不打人：观测那边也该是 0，否则说明认错了招
            if frozen.incoming[id.slot].1 > 0 {
                return (frozen, Some(format!("{} 对齐到的那一手不含攻击", id.name)));
            }
            continue;
        }
        // 自检：拿基础值现算一遍，必须和观测标签逐字相同
        let face = base + s.enemies[id.slot].get(sts2core::state::St::Strength);
        let recomputed = sts2core::damage::apply_modifiers(face, &s.enemies[id.slot], &s.player);
        let (obs_per_hit, obs_hits) = frozen.incoming[id.slot];
        if obs_hits != hits || obs_per_hit != recomputed {
            return (
                frozen,
                Some(format!(
                    "{} 现算 {}×{} 对不上观测 {}×{}",
                    id.name, recomputed, hits, obs_per_hit, obs_hits
                )),
            );
        }
        live.set_live(id.slot, base, hits);
    }
    if !live.live {
        // 一只出手的敌人都没有：两种口径等价，用冻住的那份（省得下游多一条分支）
        return (frozen, None);
    }
    (live, None)
}

fn threat_of(r: &Replayer, obs: &Obs) -> Threat {
    let mut t = Threat::new();
    for e in &obs.enemies {
        let Some(slot) = r.slot_of_existing(&e.combat_id) else { continue };
        // 一只敌人一手里可能挂着**多组**攻击意图，而威胁表一个槽位只放一组
        // `(每次伤害, 次数)`（接口是定死的）。各组数值相同时合并是精确的；
        // 不同时按**向上取整**摊平 —— 宁可把威胁估高一点，让求解器偏防守，
        // 也不要估低。这跟本仓库"选不会高估玩家的那一边"是同一条规矩。
        let hits: i32 = e.attacks.iter().map(|(_, h)| h).sum();
        let total: i32 = e.attacks.iter().map(|(d, h)| d * h).sum();
        if hits > 0 {
            t.set(slot, (total + hits - 1) / hits, hits);
        }
    }
    t
}

/// trace 里那串出牌动作 -> 内核动作序列。
fn human_line(sy: &Synced, r: &Replayer, acts: &[Frame]) -> Result<Vec<Action>, String> {
    let mut st = sy.state;
    let mut out = Vec::new();
    for f in acts {
        // 喝药水也是实战线的一部分。内核有药水模型了，不能再跳过 ——
        // 跳过等于把「我当时喝了药水」丢掉，再拿一条残缺的线去和求解器比，
        // 比出来的差全是假的。
        if let Some(Act::UsePotion { name, slot, target }) = &f.action {
            let a = Action::UsePotion {
                slot: *slot as u8,
                target: target
                    .as_ref()
                    .and_then(|t| f.obs.enemies.iter().find(|e| &e.entity_id == t))
                    .and_then(|e| r.slot_of_existing(&e.combat_id))
                    .unwrap_or(0) as u8,
            };
            let ns = sts2core::step(st, a);
            if ns == st {
                return Err(format!("内核拒绝了实战喝下的「{name}」（槽{slot}）"));
            }
            st = ns;
            out.push(a);
            continue;
        }
        if let Some(Act::SelectCard { slot }) = &f.action {
            let a = Action::Choose { hand: *slot as u8 };
            let ns = sts2core::step(st, a);
            if ns == st {
                return Err(format!("内核拒绝了实战选牌（槽{slot}）"));
            }
            st = ns;
            out.push(a);
            continue;
        }
        if let Some(Act::Confirm) = &f.action {
            if st.pending != sts2core::state::Pending::None {
                st.pending = sts2core::state::Pending::None;
            }
            continue;
        }
        let Some(Act::Play { card_name, target, .. }) = &f.action else { continue };
        if lookup_card(card_name).is_none() {
            return Err(format!("实战打了内容表里没有的牌（{card_name}）"));
        }
        // **必须按当前名字找**，不是快照名 —— 武装+ 会原地把手牌升级，
        // 拿快照去找 `突破+` 会找不到（快照里还叫 `突破`），
        // 这一整个回合就被静默跳过了。见 `replay::current_card_name`。
        let Some(h) = (0..st.n_hand as usize).find(|&h| {
            sts2core::replay::current_card_name(&sy.names, &st.cards, st.hand[h] as usize)
                == *card_name
        }) else {
            return Err("实战线在内核里重放不出来（多半是段内抽过牌）".to_string());
        };
        let tgt = target
            .as_ref()
            .and_then(|t| f.obs.enemies.iter().find(|e| &e.entity_id == t))
            .and_then(|e| r.slot_of_existing(&e.combat_id))
            .unwrap_or(0);
        let a = Action::PlayCard { hand: h as u8, target: tgt as u8 };
        let ns = sts2core::step(st, a);
        if ns == st {
            return Err(format!("内核拒绝了实战打出的「{card_name}」"));
        }
        st = ns;
        out.push(a);
    }
    Ok(out)
}

