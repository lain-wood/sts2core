//! 跨回合 rollout 的验收。**L2 单回合那条硬自检的跨回合对应物。**
//!
//! ```text
//! cargo run --release --bin rollout -- traces/act*.json
//! ```
//!
//! ## 为什么需要它
//!
//! `--score mcts` 的两个地基（敌人出招状态机、抽牌堆重建）2026-08-20 都修好了，
//! 但"能拿来做决策"这个结论仍然不成立，因为**没有验收**。L2 单回合有一条
//! "搜索穷尽时实战线不可能比求解线高分"，跨回合没有对应的东西 —— 于是
//! `--score mcts` 和 `--score survive` 分歧时，分不清那是权重差异还是 bug。
//!
//! ## 它不是一条检验，是四条 —— 因为 rollout 比已验证的 L1 多了三样东西
//!
//! | 多出来的 | 谁验它 |
//! |---|---|
//! | 敌人这一手是**预测**的，不是观测注入的 | **P1 机器面** |
//! | 抽牌堆顺序是**采样**的 | P1 的方差 + P2 的分布 |
//! | 出牌由一条**策略**决定（`--policy`，见下） | **P3 政策面** |
//! | 整场推到底 | **P2 整场** + **P4 终止性** |
//!
//! 把它们分开是这条验收的全部价值。一条笼统的"最终 HP 差多少"没法归因：
//! 差 20 血既可能是策略弱，也可能是敌人打错了，两者要修的地方完全不同。
//!
//! ## 四条各自能证伪什么
//!
//! * **P1 机器面（硬）** —— 从回合开头出发，**强制走实战真的打出去的那条线**，
//!   然后 `step(EndTurn)` 让 `EnemyDef` **自己**打这一手，把结果和**下一回合
//!   开头那一帧的观测**比血量。策略被钉死了，所以差出来的全是机器：
//!   出招预测错、伤害算错、回合边界结算错。
//!   这一条是 `verify` 的正下方 —— `verify` 注入敌人伤害，它**不注入**。
//! * **P2 整场（软，看方向）** —— 从每个回合出发推到战斗结束，多次采样，
//!   和实战真实结局比最终血量和回合数。**只看方向**：策略比实战线弱
//!   （P3 会量出来弱多少），所以推演的血量**系统性地不该高于**实战。
//!   高了只有两种可能，而且都致命：内核漏了一个让仗变难的机制，或者机器算错。
//!
//!   > **这条判据只在 `--policy fast` 下上膛**（2026-08-25）。默认的
//!   > `--policy solver` 在 P3 上 86/103 不差于实战线，"比人弱"这个前提没了。
//!   > 换策略换掉的不只是策略，还有这条判据 —— 要用它就专门跑一遍 fast。
//! * **P3 政策面（硬）** —— 同一份威胁、同一个目标函数下，给三条线打分：
//!   实战线 / **策略线**（`--policy` 选的那条）/ `solve_turn` 穷尽搜出来的线。
//!   **穷尽搜索被启发式打败是不可能的**，出现了就是 bug —— 和 L2 验收那条
//!   自检一模一样，只是换了个被审的对象。
//! * **P4 终止性（硬）** —— `rollout_single` 撞上回合上限时会把"没打完"
//!   报成"活着结束"（返回当时的血量）。这是个**凭空捏造的存活数字**。
//!   截断率必须是 0，否则上面所有数字里都掺着假存活。
//!
//! ## 随机性怎么处理
//!
//! 推演里有两条随机流（抽牌顺序 `rng.shuffle`、敌人随机分支 `rng.enemy`），
//! 所以**单次结果没有意义**。这里对每个起点采 `--samples` 次，
//! **两条流各自换种子**，报分位数而不是均值。
//!
//! > **注意 `rollout_combat` 只换了 `rng.shuffle`。** 它的 30 个样本共用同一条
//! > 敌人随机流，也就是说 `score::mcts_rollout` 根本没有对敌人的随机分支采样。
//! > 这条验收自己两条都换，并把差别报出来。

use std::process::ExitCode;

use sts2core::replay::{
    lookup_card, parse_trace, turn_segments, Act, Frame, Obs, Replayer, Synced, Trace,
};
use sts2core::rollout::{
    can_rollout, play_turn_rec, sample_outcomes, Outcome, Policy, PRODUCTION_MAX_TURNS,
    ROLLOUT_BUDGET,
};
use sts2core::solver::{explain, score, score_line, solve_turn_budget, Threat};
use sts2core::state::State;
use sts2core::step::Action;

/// 推演的回合上限。**故意比实战见过的最长一场（7 回合）宽得多** ——
/// 上限撞不撞得到本身就是 P4 要量的东西，卡得太紧等于自己把结论做出来。
const DEFAULT_MAX_TURNS: usize = 40;

// ---------------------------------------------------------------------------
// 每个回合抽出来的原始观察
// ---------------------------------------------------------------------------

struct TurnRow {
    round: i32,
    /// 距离战斗结束还有几个回合（实战口径）。1 = 这就是最后一个回合。
    to_end: i32,
    /// P1：强制实战线 + 预测敌人之后，和下一帧观测比出来的差
    machine: Option<MachineCmp>,
    /// P3：三条线的分数
    policy: Option<PolicyCmp>,
    /// P2：从这个回合出发的整场推演样本
    samples: Vec<Outcome>,
    /// P5 深度对照：**同一批种子**下另一个深度的样本（`--depth-sweep`）。
    ///
    /// 配对比较（CRN）比比两组的中位数敏感得多：同一手牌、同一条敌人随机流，
    /// 差出来的**只可能是深度**。
    alt_samples: Vec<Outcome>,
    skip: Option<String>,
}

struct MachineCmp {
    /// 内核算的下一回合开头玩家血量 - 观测到的
    hp_delta: i32,
    /// 每只敌人的血量差（内核 - 观测），只留非零的
    enemy_delta: Vec<(String, i32)>,
}

struct PolicyCmp {
    human: i32,
    fast: i32,
    solver: i32,
    complete: bool,
    /// `fast_play_turn` 走了一步 `legal_actions` 不认的动作。**这是 L1 的 bug。**
    illegal: Vec<String>,
    /// 实战线里有药水（`fast`/`solver` 这两条都不喝药水，人机不可比）
    human_used_potion: bool,
    fast_names: Vec<String>,
    solver_names: Vec<String>,
}

struct TraceRow {
    path: String,
    run: String,
    turns: Vec<TurnRow>,
    /// 实战的结局，**按内核口径**：从最后一个出牌回合的开头同步，
    /// 把实战那几张牌照打一遍，读内核算出来的血量。
    ///
    /// 为什么不直接用终帧观测的血量：胜利遗物（燃烧之血 +6 / 带骨肉 +12）
    /// 在终帧里是算过的，而 12 条第 1 幕的 trace 录在录制器加 `relics` 字段
    /// 之前 —— 内核不知道身上有那两件遗物，推演自然也不会回那 6/18 点。
    /// 拿终帧去比等于给每条 trace 加一个已知的固定偏移，把 P2 的方向判断污染掉。
    /// 两个数都报出来，差值就是内核的遗物盲区，明摆着不藏。
    actual_hp: Option<i32>,
    /// 终帧观测里的血量（游戏的真值，含它自己的胜利回血）
    observed_end_hp: Option<i32>,
    actual_turns: i32,
    reference_why: Option<String>,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!(
            "用法: rollout <trace.json>... [--samples N] [--seed S] [--score survive|damage|hp]"
        );
        eprintln!("             [--budget N] [--max-turns N] [--policy fast|solver] [--all]");
        eprintln!("  --policy    推演里那个模拟玩家怎么出牌（默认 solver）。");
        eprintln!("              fast   = 手写启发式 fast_play_turn（2026-08-25 之前的行为）");
        eprintln!("              solver = 每回合调一次 solve_turn，威胁由 EnemyDef 预测");
        eprintln!("              plan   = 限深 expectimax（--plan-depth N，默认 2）");
        eprintln!("              **换策略会动摇 P2 的判据**，见 P2 那一段的说明");
        eprintln!("  --policy-score  推演策略自己的目标函数（默认 damage）。");
        eprintln!("              **和 --score 不是一回事**：那个是给三条线打分的，这个是策略自己用的。");
        eprintln!("              用 survive 跑跑看就知道为什么不是它：模拟玩家只挡不打，P4 当场变红");
        eprintln!("  --samples   每个起点采几次（默认 64）。单次结果没有意义 —— 推演里有两条随机流");
        eprintln!("  --seed      随机种子（默认 20260821）。换种子结论不该变，变了说明样本不够");
        eprintln!("  --max-turns 推演回合上限（默认 40）。撞上上限的推演会被 P4 判红");
        eprintln!("  --depth-sweep N  再拿**同一批种子**跑一遍 D=N 的 plan 策略，配对比较（P5）。");
        eprintln!("              配对比中位数敏感得多：同一手牌、同一条敌人随机流，差的只可能是深度");
        eprintln!("  --set \"k=v,...\"  改**主策略**的配置（键同 --alt）。单独跑一遍量耗时用它 ——");
        eprintln!("              一个 --alt 进程里两条 arm 的耗时是混在一起的，分不开");
        eprintln!("  --alt \"k=v,...\"  **同深度 A/B**：对照策略从主策略的配置出发，只改点名的那几个键。");
        eprintln!("              没有它，P5 只答得了「D=2 比 D=1 好多少」，答不了「新 D=2 比旧 D=2 好多少」。");
        eprintln!("              键：depth / score / cand-score / deep-score / k / k-certain / tt /");
        eprintln!("                  power-reserve / damage-reserve / budgets=a:b:c:d / widths=a:b:c:d /");
        eprintln!("                  exact-threshold / ext=none|lethal+spike / ext-width / window=on|off|N /");
        eprintln!("                  window-score /");
        eprintln!("                  leaf=eval|rollout[:samples:turns] / seed");
        eprintln!("              例：--alt \"deep-score=damage\"  = 退回 2026-09-04 之前的深层口径");
        eprintln!("                  --alt \"window=off\"         = 退回 2026-09-05 之前（没有确定性窗口）");
        eprintln!("                  --alt \"window-score=leaf\"   = 退回阶段 4 之前（窗口内不换目标函数）");
        eprintln!("                  --alt \"cand-score=damage,k-certain=off,tt=off\"");
        eprintln!("                                              = 退回阶段 3 之前（候选生成三步全关）");
        return ExitCode::from(2);
    }

    let mut paths = Vec::new();
    let mut samples = 64usize;
    let mut seed = 20_260_821u64;
    let mut budget = sts2core::solver::DEFAULT_BUDGET;
    let mut max_turns = DEFAULT_MAX_TURNS;
    let mut scorer: fn(&State) -> i32 = score::survive_first;
    let mut scorer_name = "survive";
    let mut policy = Policy::default();
    let mut policy_budget = ROLLOUT_BUDGET;
    // 推演策略自己的目标函数，**和 `--score`（验收给三条线打分用的那个）
    // 不是一回事**。默认 damage，理由见 `Policy::Solver::score`。
    let mut policy_score: fn(&State) -> i32 = score::damage_first;
    let mut policy_score_name = "damage";
    let mut plan_depth = 2u8;
    // P5：跑完主策略再拿同一批种子跑一遍这个深度，配对比较
    let mut sweep_depth: Option<u8> = None;
    // P5 的**同深度** A/B：对照策略从主策略配置出发，只改点名的键。见 `apply_alt`。
    let mut alt_spec: Option<String> = None;
    // **主策略**的覆盖键，语法和 `--alt` 一模一样（共用 `apply_alt`）。
    // 没有它，任何配置只当得了对照 arm —— 而"新配置自己跑一遍要多久"
    // 是个独立的问题（同一个 `--alt` 进程里两条 arm 的耗时是混在一起的）。
    let mut set_spec: Option<String> = None;
    // 延伸开关，做消融用。`--ext none` / `--ext lethal,boundary` …
    let mut ext_spec: Option<String> = None;
    let mut ext_width: Option<u16> = None;
    let mut power_reserve: Option<usize> = None;
    // 方案 C：叶子换成截断推演。`--leaf rollout[:samples:turns]`
    let mut leaf_spec: Option<String> = None;
    let mut show_all = false;
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
            "--all" => show_all = true,
            "--samples" => samples = num!(usize),
            "--seed" => seed = num!(u64),
            "--budget" => budget = num!(u32),
            "--max-turns" => max_turns = num!(usize),
            "--policy-budget" => policy_budget = num!(u32),
            "--policy-score" => match it.next().map(|s| s.as_str()) {
                Some("survive") => {
                    policy_score = score::survive_first;
                    policy_score_name = "survive";
                }
                Some("damage") => {
                    policy_score = score::damage_first;
                    policy_score_name = "damage";
                }
                Some("hp") => {
                    policy_score = score::hp_only;
                    policy_score_name = "hp";
                }
                other => {
                    eprintln!("--policy-score 只认 survive / damage / hp，收到 {other:?}");
                    return ExitCode::from(2);
                }
            },
            "--plan-depth" => plan_depth = num!(u8),
            "--depth-sweep" => sweep_depth = Some(num!(u8)),
            "--set" => match it.next() {
                Some(v) => set_spec = Some(v.clone()),
                None => {
                    eprintln!("--set 后面要跟一串 key=value（逗号分隔），键和 --alt 一样");
                    return ExitCode::from(2);
                }
            },
            "--alt" => match it.next() {
                Some(v) => alt_spec = Some(v.clone()),
                None => {
                    eprintln!("--alt 后面要跟一串 key=value（逗号分隔），见 --help");
                    return ExitCode::from(2);
                }
            },
            "--ext-width" => ext_width = Some(num!(u16)),
            // 能力线保底名额。`--power-reserve 0` 就是 2026-08-31 之前的行为，
            // A/B 用它。
            "--power-reserve" => power_reserve = Some(num!(usize)),
            // `--leaf eval`（默认，静态评估）/ `--leaf rollout` /
            // `--leaf rollout:2:4`（2 条样本、最多推 4 个回合）。
            "--leaf" => match it.next() {
                Some(v) => leaf_spec = Some(v.clone()),
                None => {
                    eprintln!("--leaf 后面要跟 eval 或 rollout[:samples:turns]");
                    return ExitCode::from(2);
                }
            },
            "--ext" => match it.next() {
                Some(v) => ext_spec = Some(v.clone()),
                None => {
                    eprintln!("--ext 后面要跟 none 或 lethal/spike/boundary 的逗号列表");
                    return ExitCode::from(2);
                }
            },
            "--policy" => match it.next().map(|s| s.as_str()) {
                Some("fast") => policy = Policy::Fast,
                Some("plan") => policy = Policy::Plan(sts2core::plan::Plan::default()),
                Some("solver") => {
                    policy = Policy::Solver { budget: policy_budget, score: policy_score }
                }
                other => {
                    eprintln!("--policy 只认 fast / solver / plan，收到 {other:?}");
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
                other => {
                    eprintln!("--score 只认 survive / damage / hp，收到 {other:?}");
                    return ExitCode::from(2);
                }
            },
            _ => paths.push(a.clone()),
        }
    }

    // `--policy solver` 出现在 `--policy-budget` 之前时，上面那条分支拿到的是
    // 默认预算。这里统一按最终的 `policy_budget` 重建一次，免得参数顺序影响结果。
    if let Policy::Solver { .. } = policy {
        policy = Policy::Solver { budget: policy_budget, score: policy_score };
    }
    // 延伸开关 + 能力线保底。**主策略和对照策略要用同一套** —— 消融要比的是
    // 深度或延伸其中一件事，两件一起变就归不了因。
    // `--power-reserve` 也必须走这里：只给主策略开、对照不开的话，P5 那一栏
    // 差出来的就是"深度 + 保底"的合计，而它印出来的标题是"深度"。
    let apply_ext = |c: &mut sts2core::plan::Plan| {
        if let Some(spec) = &ext_spec {
            let on = |k: &str| spec.split(',').any(|x| x.trim() == k);
            c.ext_lethal = on("lethal");
            c.ext_spike = on("spike");
            c.ext_boundary = on("boundary");
        }
        if let Some(w) = ext_width {
            c.ext_width = w;
        }
        if let Some(r) = power_reserve {
            c.power_reserve = r;
        }
        if let Some(spec) = &leaf_spec {
            let mut it = spec.split(':');
            match it.next() {
                Some("rollout") => {
                    let samples = it.next().and_then(|v| v.parse().ok()).unwrap_or(2u16);
                    let turns = it.next().and_then(|v| v.parse().ok()).unwrap_or(4u16);
                    c.leaf = sts2core::plan::Leaf::Rollout {
                        samples,
                        turns,
                        tail: sts2core::solver::score::leaf,
                    };
                }
                _ => c.leaf = sts2core::plan::Leaf::Eval(sts2core::solver::score::leaf),
            }
        }
    };
    if let Policy::Plan(_) = policy {
        let mut c = sts2core::plan::Plan::default();
        c.depth = plan_depth.max(1);
        c.score = policy_score;
        // 叶评估**不跟 `--policy-score` 走**：标定出来它该用自己那套权重
        // （`Weights::LEAF`），见 `plan::Plan::leaf`。
        c.leaf = sts2core::plan::Leaf::Eval(sts2core::solver::score::leaf);
        apply_ext(&mut c);
        // 主策略的覆盖**最后应用**，这样它盖得住上面每一个开关。
        if let Some(spec) = &set_spec {
            if let Err(e) = apply_alt(&mut c, spec) {
                eprintln!("--set: {e}");
                return ExitCode::from(2);
            }
        }
        policy = Policy::Plan(c);
    }
    let policy_name = match policy {
        Policy::Fast => "fast（手写启发式）".to_string(),
        Policy::Solver { budget, .. } => format!(
            "solver（每回合 solve_turn，预算 {budget}，目标函数 {policy_score_name}）"
        ),
        Policy::Plan(c) => plan_name(&c, policy_score_name),
    };
    println!(
        "目标函数 {scorer_name} · 每个起点 {samples} 次采样（两条随机流各自换种子）· 种子 {seed}\n\
         推演回合上限 {max_turns} · 验收节点预算 {budget}\n\
         推演策略 {policy_name}\n"
    );

    // 对照策略：**从主策略的配置出发，只改点名的那几个键** ——
    // 没点名的键逐字相同，差出来的才只可能是点名的那几个。
    //
    // `--depth-sweep N` 是 `--alt "depth=N"` 的别名，两条路建出来的配置逐字节相同：
    // 主策略本来就是 `Plan::default()` + `score` + `leaf` + `apply_ext`，
    // 老代码那份"从 default 重建一次"和"拿主策略改掉 depth"是同一个东西。
    // `--policy` 不是 plan 时没有主配置可继承，退回老路（从 default 建）。
    let alt_base = |depth_override: Option<u8>| {
        let mut c = match policy {
            Policy::Plan(c) => c,
            _ => {
                let mut c = sts2core::plan::Plan::default();
                c.depth = plan_depth.max(1);
                c.score = policy_score;
                // 叶评估**不跟 `--policy-score` 走**：标定出来它该用自己那套权重
                // （`Weights::LEAF`），见 `plan::Plan::leaf`。
                c.leaf = sts2core::plan::Leaf::Eval(sts2core::solver::score::leaf);
                apply_ext(&mut c);
                c
            }
        };
        if let Some(d) = depth_override {
            c.depth = d.max(1);
        }
        c
    };
    let alt_policy = if sweep_depth.is_some() || alt_spec.is_some() {
        let mut c = alt_base(sweep_depth);
        if let Some(spec) = &alt_spec {
            if let Err(e) = apply_alt(&mut c, spec) {
                eprintln!("--alt: {e}");
                return ExitCode::from(2);
            }
        }
        Some(Policy::Plan(c))
    } else {
        None
    };
    if let Some(Policy::Plan(c)) = alt_policy {
        println!("对照策略 {}\n", plan_name(&c, policy_score_name));
    }

    let mut rows = Vec::new();
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
        rows.push(analyse(p, &t, scorer, budget, samples, seed, max_turns, policy, alt_policy));
    }

    report(&rows, show_all, samples, max_turns, policy)
}

// ---------------------------------------------------------------------------
// 策略配置：命名和 `--alt` 覆盖
// ---------------------------------------------------------------------------

/// 一个 `Plan` 配置的一行说明，和 `--alt` 的键解析，**两个都住在库里**
/// （`Plan::describe` / `Plan::apply`）—— `bin/plan_audit --set` 和
/// `bin/solve` 要用同一份。各写一遍的话，"同一个 `--set` 字符串"在三条验收里
/// 指的就不是同一个配置，而那种错**不报错，只是量的不是同一个东西**。
fn plan_name(c: &sts2core::plan::Plan, policy_score_name: &str) -> String {
    let _ = policy_score_name;
    c.describe()
}

fn apply_alt(c: &mut sts2core::plan::Plan, spec: &str) -> Result<(), String> {
    c.apply(spec)
}

// ---------------------------------------------------------------------------
// 分析一条 trace
// ---------------------------------------------------------------------------

/// 把 trace 切成回合。和 `bin/solve.rs::solve_trace` 是同一套切法 ——
/// 两边不一致的话，"同一个回合"在两条验收里指的就不是同一件事。

#[allow(clippy::too_many_arguments)]
fn analyse(
    path: &str,
    t: &Trace,
    scorer: fn(&State) -> i32,
    budget: u32,
    samples: usize,
    seed: u64,
    max_turns: usize,
    policy: Policy,
    alt_policy: Option<Policy>,
) -> TraceRow {
    let segs = turn_segments(t);
    let n_seg = segs.len();
    let mut r = Replayer::for_trace(t);
    let mut turns: Vec<TurnRow> = Vec::new();
    let mut actual_hp = None;
    let mut reference_why = None;
    let mut actual_turns = 0;

    // 上一个回合处理到哪一帧了。**必须把中间那些帧喂给 `Replayer::advance`**，
    // 否则观测里没有的每回合计数器（`hp_lost_this_turn` / `attacks_played` /
    // `exhausted_this_turn` / `free_attack`）在回合开头一律是 0。
    //
    // 这不是洁癖，是被数据抓到的：`act2_f31` 第 3、4 回合，绯红披风在回合开始
    // 扣的那 1 点血让**怨恨攻击两次**。少带这个计数器，内核的怨恨只打一下，
    // 敌人血量当场差 7 和 10 —— 正好就是 P1 一开始报出来的那两条。
    // （`replay::sync_latest` 的文档注释里记着同一个坑，这里是第二次踩。）
    let mut fed = 0usize;

    for (k, &(i, j)) in segs.iter().enumerate() {
        while fed < i {
            r.advance(&t.frames[fed]);
            fed += 1;
        }
        let start = &t.frames[i];
        let acts = &t.frames[i..j.min(t.frames.len())];
        let round = start.obs.round;
        actual_turns = actual_turns.max(round);
        let to_end = (n_seg - k) as i32;
        let mut row =
            TurnRow { round, to_end, machine: None, policy: None, samples: Vec::new(), alt_samples: Vec::new(), skip: None };

        let skip = |why: &str| Some(why.to_string());
        if !start.obs.is_play_phase {
            row.skip = skip("这一帧不是出牌阶段");
            turns.push(row);
            continue;
        }
        if start.obs.enemies.iter().any(|e| e.intent_unparsed) {
            row.skip = skip("有意图标签解析不出数字，建不出威胁");
            turns.push(row);
            continue;
        }
        if acts.iter().any(|f| matches!(f.action, Some(Act::SelectCard { .. }) | Some(Act::Confirm)))
        {
            row.skip = skip("走了选牌界面（v1 不对拍选牌）");
            turns.push(row);
            continue;
        }
        let mut sy = r.sync(&start.obs);
        if start.obs.hand.iter().any(|c| lookup_card(&c.name).is_none()) {
            row.skip = skip("手牌里有内容表还没有的牌");
            turns.push(row);
            continue;
        }
        let ident = r.identify_enemies(&mut sy.state, &start.obs);
        if !can_rollout(&sy.state) {
            let blind: Vec<&str> =
                ident.iter().filter(|x| !x.usable()).map(|x| x.name.as_str()).collect();
            row.skip = skip(&format!("跨回合推演认不出敌人（{}）", blind.join(" ")));
            turns.push(row);
            continue;
        }

        // --- P2：从这个回合出发，整场推到底 ---
        row.samples = sample_outcomes(&sy.state, samples, seed, max_turns, policy);
        // P5：同一批种子再跑一遍另一个深度
        if let Some(alt) = alt_policy {
            row.alt_samples = sample_outcomes(&sy.state, samples, seed, max_turns, alt);
        }

        // --- 实战线（P1 和 P3 都要用它） ---
        let human = human_line(&sy, &r, acts);

        // --- P1 机器面：强制实战线 + 让 EnemyDef 自己打这一手 ---
        // 只有"下一回合的开头"才有可比的观测。最后一个回合没有下一帧，跳过。
        if let (Ok(h), Some(next)) = (&human, segs.get(k + 1).map(|&(ni, _)| &t.frames[ni])) {
            if next.obs.is_play_phase {
                row.machine = machine_cmp(&sy.state, &r, h, &next.obs);
            }
        }

        // --- P3 政策面：三条线同一份威胁、同一个目标函数 ---
        let threat = threat_of(&r, &start.obs);
        let mut fast_state = sy.state;
        let mut rec: Option<Vec<Action>> = Some(Vec::new());
        // **P3 审的就是推演里真正会走的那条策略线**，所以这里跟着 `--policy` 走。
        // 换掉策略而 P3 还审着老策略，等于验收和被审对象分家 —— 那正是
        // 2026-08-21 那一轮把 `fast_play_turn` 照出来的原因（设计说 `solve_turn`，
        // 代码是启发式），不能在同一个地方再犯一次。
        play_turn_rec(&mut fast_state, policy, &mut rec);
        let fast_acts = rec.unwrap_or_default();
        let fast_names = explain(&sy.state, &fast_acts);
        let illegal = illegal_steps(&sy.state, &fast_acts);
        let fast_score = score_line(&sy.state, &threat, scorer, &fast_acts);
        let sol = solve_turn_budget(&sy.state, &threat, scorer, budget);
        let solver_names = explain(&sy.state, sol.line.acts());
        let human_score =
            human.as_ref().ok().and_then(|h| score_line(&sy.state, &threat, scorer, h));
        if let Some(f) = fast_score {
            row.policy = Some(PolicyCmp {
                human: human_score.unwrap_or(i32::MIN),
                fast: f,
                solver: sol.line.score,
                complete: sol.complete,
                // 实战线重放不出来、或者实战喝了药水时，人机那一栏不可比
                // （`fast`/`solver` 两条线都不动药水）—— 但"启发式赢不过穷尽
                // 搜索"那条硬判据和人无关，仍然要验。
                human_used_potion: human_score.is_none()
                    || acts.iter().any(|f| matches!(f.action, Some(Act::UsePotion { .. }))),
                illegal,
                fast_names,
                solver_names,
            });
        }

        // --- 参照结局：最后一个出牌回合，把实战那几张牌照打一遍 ---
        if k + 1 == n_seg {
            match &human {
                Ok(h) => {
                    let mut st = sy.state;
                    let mut ok = true;
                    for &a in h {
                        let ns = sts2core::step(st, a);
                        if ns == st {
                            ok = false;
                            break;
                        }
                        st = ns;
                    }
                    if !ok {
                        reference_why = Some("最后一个回合的实战线重放不出来".into());
                    } else if st.combat_over && !st.player_dead {
                        actual_hp = Some(st.player.hp);
                    } else {
                        reference_why =
                            Some("最后一个回合打完战斗还没结束（trace 录到一半）".into());
                    }
                }
                Err(why) => reference_why = Some(why.clone()),
            }
        }
        turns.push(row);
    }

    let observed_end_hp = t.frames.last().map(|f| f.obs.hp);
    TraceRow {
        path: path.to_string(),
        run: t.run.clone(),
        turns,
        actual_hp,
        observed_end_hp,
        actual_turns,
        reference_why,
    }
}

/// 两条随机流各自换种子的多次采样。
///
/// **不复用 `rollout::rollout_combat`**：那个函数只换 `rng.shuffle`，
/// 30 个样本共用同一条敌人随机流，等于对敌人的随机分支一次都没采样。
/// 验收自己采两条，好把"分布有多宽"这件事量准。

/// P1：强制实战线，然后让 `EnemyDef` 自己打这一手，和下一帧观测比血量。
fn machine_cmp(s: &State, r: &Replayer, human: &[Action], next: &Obs) -> Option<MachineCmp> {
    let mut st = *s;
    for &a in human {
        let ns = sts2core::step(st, a);
        if ns == st {
            return None;
        }
        st = ns;
    }
    if st.combat_over {
        return None;
    }
    // **不注入**：敌人这一手完全由 `EnemyDef` 决定。这正是它和 `verify` 的分界线。
    st = sts2core::step(st, Action::EndTurn);
    if st.combat_over {
        return None;
    }
    let mut enemy_delta = Vec::new();
    for e in &next.enemies {
        if let Some(slot) = r.slot_of_existing(&e.combat_id) {
            let d = st.enemies[slot].hp - e.hp;
            if d != 0 {
                enemy_delta.push((e.name.clone(), d));
            }
        }
    }
    Some(MachineCmp { hp_delta: st.player.hp - next.hp, enemy_delta })
}

/// `fast_play_turn` 走过的每一步，`legal_actions` 认不认？
///
/// **L1 有两个客户，它们问的方式不一样**：`solve_turn` 先问 `legal_actions`
/// 再 `step`，`fast_play_turn` 直接 `step`。两边对"什么算合法"必须一致 ——
/// 不一致意味着搜索在一个比真实规则更窄（或更宽）的动作集上求最优，
/// 而**分数是看不出这件事的**：漏掉的动作只要在这一回合不划算，
/// 分数比较就一声不吭。
///
/// 实测过：把 `legal_actions` 改成不生成能力牌，P3 的分数比较**一点反应都没有**
/// （能力牌在单回合目标函数下本来就不划算），而这条检查当场点名。
fn illegal_steps(s: &State, acts: &[Action]) -> Vec<String> {
    let mut st = *s;
    let mut out = Vec::new();
    for &a in acts {
        let ns = sts2core::step(st, a);
        // `step` 是全函数：非法动作原样返回。原地不动的那一步 `fast_play_turn`
        // 自己也会停下（它有 `if next_s == *s { break }`），不算数。
        if ns == st {
            break;
        }
        // **按到达的局面比，不按动作元组比。** 同一件事有多种写法：
        // 非目标牌（防御、闪电霹雳）的 `target` 字节谁都可以填，
        // `legal_actions` 填 0，`fast_play_turn` 填 `first_alive()`。
        // 按元组比会把这种纯表示差异报成 8 条假阳性 —— 实测过。
        let (legal, n) = sts2core::step::legal_actions(&st);
        if !legal[..n].iter().any(|&b| sts2core::step(st, b) == ns) {
            out.push(explain(&st, &[a]).join(""));
        }
        st = ns;
    }
    out
}

/// 从观测的意图标签建威胁。和 `bin/solve.rs::threat_of` 必须一致。
fn threat_of(r: &Replayer, obs: &Obs) -> Threat {
    let mut t = Threat::new();
    for e in &obs.enemies {
        let Some(slot) = r.slot_of_existing(&e.combat_id) else { continue };
        let hits: i32 = e.attacks.iter().map(|(_, h)| h).sum();
        let total: i32 = e.attacks.iter().map(|(d, h)| d * h).sum();
        if hits > 0 {
            t.set(slot, (total + hits - 1) / hits, hits);
        }
    }
    t
}

/// trace 里那串动作 -> 内核动作序列。和 `bin/solve.rs::human_line` 同一套规矩。
fn human_line(sy: &Synced, r: &Replayer, acts: &[Frame]) -> Result<Vec<Action>, String> {
    let mut st = sy.state;
    let mut out = Vec::new();
    for f in acts {
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
        let Some(Act::Play { card_name, target, .. }) = &f.action else { continue };
        if lookup_card(card_name).is_none() {
            return Err(format!("实战打了内容表里没有的牌（{card_name}）"));
        }
        let Some(h) = (0..st.n_hand as usize)
            .find(|&h| sy.names.get(st.hand[h] as usize).map(|s| s.as_str()) == Some(card_name))
        else {
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

// ---------------------------------------------------------------------------
// 报告
// ---------------------------------------------------------------------------

fn pct(v: &mut [i32], p: f64) -> i32 {
    if v.is_empty() {
        return 0;
    }
    v.sort_unstable();
    let ix = (((v.len() - 1) as f64) * p).round() as usize;
    v[ix]
}

fn report(
    rows: &[TraceRow],
    show_all: bool,
    samples: usize,
    max_turns: usize,
    policy: Policy,
) -> ExitCode {
    // P2 的方向判据只在**弱策略**下成立，见下面那一大段。
    let p2_gate_armed = matches!(policy, Policy::Fast);
    let short = match policy {
        Policy::Fast => "fast",
        Policy::Solver { .. } => "solver",
        Policy::Plan(_) => "plan",
    };
    // ---------------- P1 ----------------
    let mut p1_total = 0usize;
    let mut p1_exact = 0usize;
    let mut p1_err: Vec<i32> = Vec::new();
    let mut p1_bad: Vec<String> = Vec::new();
    // ---------------- P3 ----------------
    let mut p3_total = 0usize;
    let mut p3_fast_beats_solver = 0usize;
    let mut p3_fast_ge_human = 0usize;
    let mut p3_human_cmp = 0usize;
    let mut p3_impossible: Vec<String> = Vec::new();
    let mut p3_illegal: Vec<String> = Vec::new();
    // ---------------- P4 ----------------
    let mut p4_runs = 0usize;
    let mut p4_trunc = 0usize;
    // 用 `score::mcts_rollout` 真正的那个上限去数，才是生产口径
    let mut p4_prod_trunc = 0usize;
    // ---------------- P2 ----------------
    let mut p2_lines: Vec<String> = Vec::new();
    let mut p2_optimistic: Vec<String> = Vec::new();
    let mut p2_by_dist: std::collections::BTreeMap<i32, Vec<i32>> = Default::default();
    // 距结束 ≤ `P2_NEAR` 个回合的起点：(哪一条, 偏差)。**P2 的硬判据在这里** ——
    // 那几个回合策略几乎没有选择余地，偏差还大就只能是机器错。
    let mut p2_near: Vec<(String, i32)> = Vec::new();
    let mut skip_why: std::collections::BTreeMap<String, usize> = Default::default();

    for tr in rows {
        let tag = tr.path.replace("traces/", "").replace("traces\\", "");
        println!("=== {tag} — {} ===", tr.run);
        match (tr.actual_hp, tr.observed_end_hp) {
            (Some(h), Some(o)) => println!(
                "  实战结局  {actual} 回合，内核口径 {h} 血；终帧观测 {o} 血{note}",
                actual = tr.actual_turns,
                note = if o != h {
                    format!("（差 {:+}：胜利遗物，内核这条 trace 里看不到）", o - h)
                } else {
                    String::new()
                }
            ),
            _ => println!(
                "  实战结局  拿不到参照：{}",
                tr.reference_why.clone().unwrap_or_else(|| "未知".into())
            ),
        }

        let n_turn = tr.turns.len() as i32;
        for t in &tr.turns {
            if let Some(w) = &t.skip {
                *skip_why.entry(w.clone()).or_insert(0) += 1;
                if show_all {
                    println!("  [跳过] 回合{:<3} {w}", t.round);
                }
                continue;
            }
            // P1
            if let Some(m) = &t.machine {
                p1_total += 1;
                p1_err.push(m.hp_delta.abs());
                if m.hp_delta == 0 && m.enemy_delta.is_empty() {
                    p1_exact += 1;
                } else {
                    let d: Vec<String> =
                        m.enemy_delta.iter().map(|(n, d)| format!("{n}{d:+}")).collect();
                    p1_bad.push(format!(
                        "{tag} 回合{}: 我方血量 {:+}{}",
                        t.round,
                        m.hp_delta,
                        if d.is_empty() { String::new() } else { format!("，敌人 {}", d.join(" ")) }
                    ));
                }
            }
            // P3
            if let Some(p) = &t.policy {
                p3_total += 1;
                for a in &p.illegal {
                    p3_illegal.push(format!("{tag} 回合{}: `fast_play_turn` 走了 {a}，而 `legal_actions` 不生成它", t.round));
                }
                if p.fast > p.solver && p.complete {
                    p3_fast_beats_solver += 1;
                    p3_impossible.push(format!(
                        "{tag} 回合{}: 启发式 {} > 穷尽搜索 {}\n         启发式 {}\n         穷尽   {}",
                        t.round,
                        p.fast,
                        p.solver,
                        p.fast_names.join(" -> "),
                        p.solver_names.join(" -> ")
                    ));
                }
                if !p.human_used_potion {
                    p3_human_cmp += 1;
                    if p.fast >= p.human {
                        p3_fast_ge_human += 1;
                    }
                }
            }
            // P4 + P2
            if t.samples.is_empty() {
                continue;
            }
            p4_runs += t.samples.len();
            p4_trunc += t.samples.iter().filter(|o| o.truncated).count();
            p4_prod_trunc += t
                .samples
                .iter()
                .filter(|o| o.truncated || o.turns as usize >= PRODUCTION_MAX_TURNS)
                .count();
            let mut hp: Vec<i32> = t.samples.iter().map(|o| o.final_hp).collect();
            let mut tn: Vec<i32> = t.samples.iter().map(|o| o.turns as i32).collect();
            let deaths = t.samples.iter().filter(|o| o.died).count();
            let p10 = pct(&mut hp, 0.10);
            let p50 = pct(&mut hp, 0.50);
            let p90 = pct(&mut hp, 0.90);
            let tmed = pct(&mut tn, 0.50);
            if let Some(a) = tr.actual_hp {
                p2_by_dist.entry(t.to_end).or_default().push(p50 - a);
                if t.to_end <= P2_NEAR {
                    p2_near.push((format!("{tag} 回合{}(距{})", t.round, t.to_end), p50 - a));
                }
                if p10 > a {
                    p2_optimistic.push(format!(
                        "{tag} 回合{}（距结束 {}）: 推演 p10={p10} > 实战 {a}",
                        t.round, t.to_end
                    ));
                }
            }
            if show_all || t.to_end == n_turn {
                p2_lines.push(format!(
                    "  {tag} 回合{:<2}(距结束{}) 血量 p10/p50/p90 = {p10:>3}/{p50:>3}/{p90:>3}  \
                     实战 {:>3} | 回合中位 {tmed:>2}（实战剩 {}）| 死 {}/{} 截断 {}",
                    t.round,
                    t.to_end,
                    tr.actual_hp.map(|a| a.to_string()).unwrap_or_else(|| "—".into()),
                    t.to_end,
                    deaths,
                    t.samples.len(),
                    t.samples.iter().filter(|o| o.truncated).count()
                ));
                // 截断了就把「停在哪儿」一起印出来：**「差一口气」和「僵住了」
                // 要修的地方完全不同**，只报一个条数分不出来。
                let mut left: Vec<i32> =
                    t.samples.iter().filter(|o| o.truncated).map(|o| o.enemy_hp_left).collect();
                if !left.is_empty() {
                    let n = left.len();
                    let med = pct(&mut left, 0.50);
                    p2_lines.push(format!(
                        "        └ 截断的 {n} 条停在敌人还剩 {med} 血（中位）"
                    ));
                }
            }
        }
        println!();
    }

    println!("=======================================================================");
    println!("P1 · 机器面 —— 强制实战线 + 让 EnemyDef 自己打这一手，比下一帧观测");
    println!("     （策略被钉死了，所以差出来的只可能是机器：出招预测 / 伤害 / 回合边界）");
    if p1_total == 0 {
        println!("     没有可比的回合。");
    } else {
        p1_err.sort_unstable();
        println!(
            "     {p1_exact}/{p1_total} 逐字一致（血量和每只敌人的血都对上）\n\
             \x20    血量绝对误差 中位 {} / p90 {} / 最大 {}",
            p1_err[p1_err.len() / 2],
            pct(&mut p1_err.clone(), 0.90),
            p1_err.last().copied().unwrap_or(0)
        );
        for b in p1_bad.iter().take(if show_all { usize::MAX } else { 12 }) {
            println!("       ✗ {b}");
        }
        if !show_all && p1_bad.len() > 12 {
            println!("       …… 还有 {} 条（--all 全看）", p1_bad.len() - 12);
        }
    }

    println!("\nP3 · 政策面 —— 同一份威胁、同一个目标函数下三条线的分数");
    println!(
        "     比了 {p3_total} 个回合：
              (a) 策略线走了 `legal_actions` 不认的动作 {} 次（**必须是 0**）
              (b) 策略线打赢穷尽搜索 {p3_fast_beats_solver} 次（**必须是 0**）",
        p3_illegal.len()
    );
    if p3_human_cmp > 0 {
        println!(
            "     策略线（{short}）≥ 实战线：{p3_fast_ge_human}/{p3_human_cmp} \
             —— 弱策略才守得住 P2 的方向判据，见那一段"
        );
    }
    for b in p3_illegal.iter().take(if show_all { usize::MAX } else { 8 }) {
        println!("       ✗ {b}");
    }
    if !show_all && p3_illegal.len() > 8 {
        println!("       …… 还有 {} 条（--all 全看）", p3_illegal.len() - 8);
    }
    for b in &p3_impossible {
        println!("       ✗ {b}");
    }

    // ---------------- P5 深度对照 ----------------
    //
    // **配对比较，不是比两组的中位数。** 同一个起点、同一批种子（CRN），
    // 两个深度看到的是同一手牌、同一条敌人随机流，差出来的只可能是深度。
    // 比中位数要几十个样本才看得出的差别，配对几个样本就够。
    //
    // 判据是**方向**：搜得更深不该活得更差。真出现了，要么叶评估有问题
    // （深搜把一个错误的估值当真了），要么搜索本身有 bug。
    let mut p5_pairs: Vec<i32> = Vec::new();
    let mut p5_win = 0usize;
    let mut p5_lose = 0usize;
    let mut p5_tie = 0usize;
    let mut p5_worst: Vec<(String, i32)> = Vec::new();
    for tr in rows {
        let tag = tr.path.replace("traces/", "").replace("traces\\", "");
        for t in &tr.turns {
            if t.alt_samples.len() != t.samples.len() || t.samples.is_empty() {
                continue;
            }
            let mut here = 0i32;
            for (a, b) in t.samples.iter().zip(t.alt_samples.iter()) {
                // 死了按 0 算，不然"死得晚一点"会被当成进步
                let x = if a.died { 0 } else { a.final_hp };
                let y = if b.died { 0 } else { b.final_hp };
                let d = x - y;
                p5_pairs.push(d);
                here += d;
                match d.cmp(&0) {
                    std::cmp::Ordering::Greater => p5_win += 1,
                    std::cmp::Ordering::Less => p5_lose += 1,
                    std::cmp::Ordering::Equal => p5_tie += 1,
                }
            }
            p5_worst.push((format!("{tag} 回合{}", t.round), here / t.samples.len() as i32));
        }
    }
    if !p5_pairs.is_empty() {
        println!("
P5 · 深度对照 —— 同一批种子，主策略 vs 对照策略（配对比较）");
        let mut v = p5_pairs.clone();
        let med = pct(&mut v, 0.50);
        let mean = p5_pairs.iter().map(|&x| x as f64).sum::<f64>() / p5_pairs.len() as f64;
        println!(
            "     {} 对样本：主策略 − 对照 的血量差 中位 {med} / 均值 {mean:.1}",
            p5_pairs.len()
        );
        println!("     主策略更好 {p5_win} · 更差 {p5_lose} · 打平 {p5_tie}");
        p5_worst.sort_by_key(|(_, d)| *d);
        for (name, d) in p5_worst.iter().take(5) {
            println!("       ! {name} 平均 {d:+}");
        }
        println!(
            "     **这不是一条硬判据**：深一层看到的下回合是叶评估估的，",
        );
        println!(
            "     它的权重虽然标定过（`Weights::LEAF`），估歪了深搜照样会把它当真。",
        );
    }

    println!("\nP4 · 终止性 —— 撞上回合上限的推演会把「没打完」报成「活着结束」");
    println!(
        "     验收口径（上限 {max_turns} 回合）：{p4_trunc}/{p4_runs} 条截断{}",
        if p4_trunc == 0 { "" } else { "  ← 这些推演的血量是假的" }
    );
    println!(
        "     **生产口径**（`score::mcts_rollout` 用的 {PRODUCTION_MAX_TURNS} 回合）：{p4_prod_trunc}/{p4_runs} 条会被截断{}",
        if p4_prod_trunc == 0 {
            ""
        } else {
            "  ← `--score mcts` 的期望里就掺着这么多假存活"
        }
    );

    println!("\nP2 · 整场 —— 从每个回合推到战斗结束（每个起点 {samples} 次采样）");
    if !p2_gate_armed {
        println!(
            "     **这一轮 P2 的方向判据没有上膛。** 它成立的前提是模拟玩家比人弱，
     而 {short} 策略在 P3 上并不比实战线差。要用这条判据守「内核有没有漏掉
     让仗变难的机制」，专门跑一遍 `--policy fast`。"
        );
    }
    for l in &p2_lines {
        println!("{l}");
    }
    if !p2_by_dist.is_empty() {
        println!("     按「距战斗结束还有几回合」看中位偏差（推演 p50 − 实战）：");
        for (d, v) in p2_by_dist.iter().rev() {
            let mut v2 = v.clone();
            let med = pct(&mut v2, 0.50);
            println!("       距 {d:>2} 回合  n={:<3} 中位偏差 {med:+}", v.len());
        }
        println!(
            "     偏差该随着距离缩小而收敛到 0 —— 距 1 回合还差很多的话是机器问题，不是策略问题。"
        );
    }
    if !p2_near.is_empty() {
        let mut ab: Vec<i32> = p2_near.iter().map(|(_, d)| d.abs()).collect();
        let mad = pct(&mut ab, 0.50);
        let worst: Vec<String> = {
            let mut v: Vec<&(String, i32)> = p2_near.iter().collect();
            v.sort_by_key(|(_, d)| -d.abs());
            v.iter().take(5).map(|(n, d)| format!("{n}{d:+}")).collect()
        };
        println!(
            "     距结束 ≤{P2_NEAR} 回合的 {} 个起点，**绝对**偏差中位 {mad}（分布是双峰的，见下）\n\
             \x20    偏得最多的五个：{}",
            p2_near.len(),
            worst.join("  ")
        );
        println!(
            "     这个绝对值**不是判据**：最后一两个回合恰恰是策略杠杆最大的地方 ——\n\
             \x20    `fast_play_turn` 的斩杀扫描要么找到致命一击（偏差 0），要么找不到、\n\
             \x20    白挨一整个回合（偏差 -13 到 -26）。所以它是双峰的，中位数量的是策略，\n\
             \x20    不是机器。机器归 P1 管，P2 的硬判据只看**方向**（见下一行）。"
        );
    }
    println!(
        "     推演 p10 高于实战结局的起点：{} 处{}",
        p2_optimistic.len(),
        if !p2_gate_armed {
            "  ← **这不是判据**：求解器策略本来就不比人差，见 P2 的说明"
        } else if p2_optimistic.is_empty() {
            "（弱策略不该比强策略活得好，0 是应该的）"
        } else {
            "  ← 见下"
        }
    );
    for b in p2_optimistic.iter().take(if show_all { usize::MAX } else { 10 }) {
        println!("       ! {b}");
    }
    if !show_all && p2_optimistic.len() > 10 {
        println!("       …… 还有 {} 条（--all 全看）", p2_optimistic.len() - 10);
    }

    if !skip_why.is_empty() {
        println!("\n跳过的回合：");
        for (w, n) in &skip_why {
            println!("    {n:>3} × {w}");
        }
    }

    // ---------------- 判决 ----------------
    println!("\n=======================================================================");
    let mut fail = Vec::new();
    if !p3_illegal.is_empty() {
        fail.push(format!(
            "P3(a): 有 {} 步动作 `fast_play_turn` 走得出来、`legal_actions` 却不生成。
                    L1 的两个客户对「什么算合法」意见不一致 —— 搜索在一个错的动作集上
                    求最优，而分数比较看不出这件事。",
            p3_illegal.len()
        ));
    }
    if p3_fast_beats_solver > 0 {
        fail.push(format!(
            "P3: 有 {p3_fast_beats_solver} 个回合启发式策略在**穷尽搜索**下仍然更高分。\n\
             \x20   这不可能是策略好，只能是求解器或动作生成有 bug。先修这个。"
        ));
    }
    if p4_prod_trunc > 0 {
        fail.push(format!(
            "P4: 按 `score::mcts_rollout` 实际用的 {PRODUCTION_MAX_TURNS} 回合上限，有 \
             {p4_prod_trunc}/{p4_runs} 条推演没打完就被截断，\n\
             \x20   而 `rollout_single` 把它们当成「活着结束」报了出去 —— 那是个凭空捏造的\n\
             \x20   存活数字，`--score mcts` 的期望里就掺着它们。"
        ));
    }
    // P2 的硬判据：**方向**，不是绝对值。
    //
    // 它成立的前提是**模拟玩家比人弱**：`fast_play_turn` 逐回合只在 40/102 个
    // 回合上追平或超过实战线，一条更弱的策略推一整场，血量**不该系统性地高于**
    // 实战真实结局。高了只有两种可能，而且都致命：内核漏了一个让仗变难的机制，
    // 或者机器算错。它专门抓最危险的那一类 bug（推演说"你没事"，实际会死）。
    //
    // 反方向（推演比实战惨）是策略弱的正常表现，由 P1 而不是 P2 兜底。
    //
    // **换成 `Policy::Solver` 之后这个前提没了**（2026-08-25）：求解器策略在
    // 86/103 个回合上不差于实战线，推演活得比我好是**应该的**，不是 bug。
    // 所以这条判据**只在 `--policy fast` 下上膛**。想用它守"内核有没有漏掉
    // 让仗变难的机制"，就得专门跑一遍 `--policy fast` —— 这是换策略实打实的
    // 代价，写在这里免得下一个人以为它还在守着。
    let worst_bias = p2_by_dist
        .iter()
        .map(|(d, v)| {
            let mut v2 = v.clone();
            (pct(&mut v2, 0.50), *d)
        })
        .max();
    if let Some((bias, d)) = worst_bias {
        if bias > P2_BIAS_TOL && p2_gate_armed {
            fail.push(format!(
                "P2: 距结束 {d} 回合的起点，推演中位血量比实战**高** {bias}（上限 {P2_BIAS_TOL}）。\n\
                 \x20   `fast_play_turn` 是条更弱的策略（P3: {p3_fast_ge_human}/{p3_human_cmp} 个回合才追平实战），\n\
                 \x20   它不该活得更好 —— 要么内核漏了一个让仗变难的机制，要么机器算错。"
            ));
        }
    }
    if p1_total > 0 {
        let exact_rate = p1_exact as f64 / p1_total as f64;
        if exact_rate < P1_MIN_EXACT {
            fail.push(format!(
                "P1: 只有 {p1_exact}/{p1_total}（{:.0}%）的回合在预测敌人之后还能对上下一帧观测。\n\
                 \x20   策略是钉死的，所以这是机器错，不是策略差。",
                exact_rate * 100.0
            ));
        }
    }
    if fail.is_empty() {
        println!("✓ 硬判据全过。");
        println!(
            "  但这**只说明 rollout 自洽**，不说明它打得对：{}。\n\
             \x20 遗物 / 复活 / 多阶段 L1 还没有的东西它同样看不见。",
            match policy {
                Policy::Fast => "策略仍然是个手写启发式",
                Policy::Solver { .. } => "策略是逐回合的穷尽搜索，但目标函数只看这一回合",
                Policy::Plan(_) => {
                    "策略往前看了几个回合，但叶评估还没做牌组画像（S1），深度值不值这个钱要看 --depth-sweep"
                }
            }
        );
        ExitCode::SUCCESS
    } else {
        for f in &fail {
            println!("✗ {f}");
        }
        println!("\n在这些修好之前，**不要拿 `--score mcts` 做决策**。");
        ExitCode::FAILURE
    }
}

/// 「快打完了」是指距离战斗结束还有几个回合。只用来做**报告**分组，
/// 不是判据 —— 为什么不是，见上面那段"分布是双峰的"。
const P2_NEAR: i32 = 2;

/// P2 的方向容差（点血）。**上限是"推演比实战高多少"，不是绝对值。**
/// 基线量出来每个距离桶都是 -7 到 0，留 +2 是给抽牌采样的方差。
const P2_BIAS_TOL: i32 = 2;

/// P1 的及格线。**这几个数字都是被基线定的，不是拍的** —— 先跑一遍看基线
/// 落在哪，再卡在"基线掉下来就红"的位置。
///
/// 基线 36/38 = 95%，两个例外都点得出名字（雾菇的复活、第2幕 Boss 的朝向乘区，
/// 两条都在 roadmap 的「故意没做」里）。0.90 = 再坏一个回合还能过，坏两个就红。
/// 理由和完整数据见 `docs/verification-log.md` 的「跨回合 rollout 的验收」。
const P1_MIN_EXACT: f64 = 0.90;
