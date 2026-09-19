//! **跨回合 planner 的审计台。** 它不判对错，它答四个问题里的前三个。
//!
//! ```text
//! cargo run --release --bin plan_audit -- traces/act*.json          # 读数 1 和 3
//! cargo run --release --bin plan_audit -- traces/act*.json --inner  # 加读数 2（慢很多）
//! ```
//!
//! # 为什么单独有这么一个东西
//!
//! 既有的七条验收对 planner 的**结构性**问题全部沉默，原因很具体：
//! 它们的裁判和被测对象用的是同一套口径。`bin/solve` 那条自检问的是
//! "实战线赢不赢得过穷尽搜索"，`bin/rollout` 的 P3 问的是"策略线赢不赢得过
//! `solve_turn`" —— 两条都在**单回合**里比，而 planner 出问题的地方全在
//! 单回合之外：候选被剪掉了、深层用了另一个目标函数、确定性的层数没搜。
//! P5 倒是量整场血量，但它只能配对**深度**（`--depth-sweep`），
//! 同深度换内部实现它答不了（那个通道是 `bin/rollout --alt`，2026-09-04 加的）。
//!
//! # 三个读数，各自独占一类失败
//!
//! | 读数 | 独占什么 | 坏掉的样子 |
//! |---|---|---|
//! | **候选覆盖** | 最优根线在不在候选集里 | 叶评估和目标函数对它**一概沉默** —— 不在集合里的线，评得再准也评不到 |
//! | **深层内层最优性** | 深层那一手是不是真口径下的最优 | 分得开"预算不够"、"目标函数不一致"、"`Threat` 丢信息"三个原因 |
//! | **到边界后悔** | 默认建议比窗口内最优差多少 | 含 win / 不含 win 分两列 —— `win = 100000` 等于 1000 点血，混在一起读不出真实差距 |
//!
//! 第四个读数（**单次可重复性**：同一个局面只换 `Plan::seed` 会不会换线）
//! 归 `tools/plan_seed_sweep.py`，不在这里 —— 它要跑 `bin/solve`，是另一条路。
//!
//! # 只在"确定性起点"上量前两个
//!
//! 判据是**牌序整堆已知（≥5 张）且场上没有带随机分支的敌人**。
//! 理由不是图方便：只有那种局面里"最优"才是一个**有定义**的东西 ——
//! 抽牌和敌人都确定，洗牌之前这一段是有限确定性博弈，全枚举出来的最优就是最优。
//! 有随机分支时"最优"依赖于期望怎么取，那时这三个读数会变成在比两个估计量。
//!
//! # 五个口径，逐条评同一批线
//!
//! ```text
//! A0 旧口径   damage_first @ budgets[1]     ← 2026-09-04 之前深层选线用的
//! A  单层     deep_score   @ budgets[1]     ← 2026-09-05 之前（没有确定性窗口）
//! A2 现行     Plan::at_root 的尺子 @ Plan::budget(层)，确定性窗口一路借到边界
//!                                           ← `Plan::default()` 现在在做的
//!                                             （阶段 4 之后窄根上那把尺子是
//!                                              `window_score`，不是 `deep_score`）
//! B  预算给足 deep_score   @ 200000         ← A→B 的差 = 预算
//! C  到边界   deep_score   @ 200000，一路搜到洗牌边界 ← B→C 的差 = 深度
//! ```
//!
//! **A→A2 是阶段 2（确定性窗口）那一栏，A2→C 是它还差多少。**
//! 两者差的只有预算（A2 按 `Plan::budget(层)`，深层 500；C 一律 200000）
//! 和停法（A2 用 planner 的双重确定判据，C 只看单孩子）——
//! 后者绑不绑得住有独立读数，见报告里的「双重确定判据比单孩子早停」。
//!
//! **A0/A/B/C 四列的叶子都是 `score::leaf(end_turn_before_draw(·))`** —— **真敌人回合**，
//! 不是 `threat.end_turn`。这是"真口径"：`Threat` 只装攻击伤害，
//! 内容表 204 手招里 103 手含它装不下的 op（敌人加格挡 / 自增益 /
//! 给我上 debuff / 召唤 / 塞牌），拿它当裁判等于让被测对象自己判自己。
//!
//! **A2 是唯一一列不用这把尺子的**，而且必须不用：它答的问题是
//! 「planner 会挑哪条根线」，所以它得用 planner 真的在用的那把
//! （[`Plan::at_root`]，阶段 4 之后窄根上是 `window_score`）。
//! **后悔仍然一律在 C 那把尺子上量** —— 换的是「被测对象挑了什么」，不是裁判。
//!
//! # 它自己的自检
//!
//! B 和 C 两列是 2026-09-04 那两个一次性探针量过的同一件事。
//! 建成之后第一次跑，**逐个数对上了**（64 条实录）：
//! `B 漏最优 17/45` · `C 漏最优 20/45` · `满预算→到边界 13/45` ·
//! `后悔>0 22/45，其中 4 例含 win，其余中位 915 分`。
//! 一个新工具第一次跑就该拿它去复现一份已知读数 —— 对不上说明工具错了，
//! 而**工具错了的样子和被测对象错了的样子长得一模一样**。
use std::process::ExitCode;

use sts2core::content::enemy_def;
use sts2core::ops::{EOp, Next};
use sts2core::plan::{chance_children, plan_report, window_is_certain, Plan};
use sts2core::replay::{
    lookup_card, parse_trace, sync_latest, turn_segments, Obs, Replayer, Trace,
};
use sts2core::rollout::{can_rollout, predicted_threat};
use sts2core::solver::{
    explain, replay_line, score, solve_turn_potions, solve_turn_topk_split, Line, Threat,
};
use sts2core::state::State;
use sts2core::step::{current_move_ix, end_turn_before_draw};

/// 全枚举时给单回合搜索的预算。**它就是 `bin/solve` 那条自检用的数** ——
/// 两边不一致的话，"这一回合有哪些不同终局"在两条验收里指的就不是同一件事。
const FULL_BUDGET: u32 = 200_000;

/// 到边界最多再搜几层。**只在机会节点是单孩子时才往下走**，所以正常情况下
/// 碰不到它；它防的是"某个局面永远抽不完"那种写漏。
const MAX_WINDOW_LAYERS: u8 = 6;

/// 深层单回合搜索有没有搜完的计数。**用全局计数器而不是穿参数**：
/// `window_value` 是递归的、又被 `root_value` 和四个口径各调一遍，
/// 穿一个 `&mut` 进去会把签名搞脏，而这个数只是诊断、不参与任何判定。
///
/// 它原来是**阶段 3.5（预算）的动机读数**：预算一到就退化成"DFS 按手牌顺序碰到的线"，
/// 那不是解，是启发式排名 —— 而不同根候选留下的手牌复杂度不同，偏差还不均匀。
/// **那条改动 2026-09-06 量过上界之后判死了**（端到端 +0.1 血），
/// 所以这个数今天只是诊断，不再是谁的施工单：22% 这个比例难看，但它值 0.1 点血。
static SOLVES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static SOLVES_INCOMPLETE: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

fn note_solve(complete: bool) {
    use std::sync::atomic::Ordering::Relaxed;
    SOLVES.fetch_add(1, Relaxed);
    if !complete {
        SOLVES_INCOMPLETE.fetch_add(1, Relaxed);
    }
}

/// A2 那一列里，**单孩子成立、而双重确定不成立**因此早停的层数。
///
/// 它答的是一个很具体的问题：planner 那半条"每只活敌人的下一手唯一"的判据，
/// 在这批确定性起点上**绑不绑得住**。起点是按「机器里没有 `Next::Rand`」筛的，
/// 所以理论上不该绑住 —— 真绑住了只可能来自 `Next::Cond` 判不出条件、
/// 或者窗口里召唤出了一只带随机分支的敌人。**这个数不为 0 时，
/// A2 比 C 浅的那部分就不全是预算。**
static DUAL_STOPS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// 深层单回合搜索的预算口径。
#[derive(Clone, Copy)]
enum Budget {
    /// 每层都给这么多。**A0/A/B/C 四列的口径，它们是尺子，别动。**
    Flat(u32),
    /// 按 `Plan::budget(层)` —— **planner 真的在用的那一份**（深层只有 500）。
    PerDepth,
}

impl Budget {
    fn at(&self, cfg: &Plan, depth: usize) -> u32 {
        match *self {
            Budget::Flat(b) => b,
            Budget::PerDepth => cfg.budget(depth),
        }
    }
}

/// 什么时候停下来就地评估。
#[derive(Clone, Copy, PartialEq)]
enum Stop {
    /// 机会节点只有一个 `p = 1.0` 的孩子就继续。**C 那把尺子用的，别动。**
    SingleChild,
    /// planner 的**双重确定**判据（`plan::window_is_certain`）：
    /// 机会节点确定 **且** 每只活敌人的下一手唯一。A2 用它。
    Dual,
}

/// 分数里带着 `Weights::win`（+100000）或 `death`（−1000000）的那一档。
/// 后悔要按这条线拆成两列报：**一个 win 等于 1000 点血**，混在一起
/// 均值会被四五个样本完全带走（实测：45 个起点均值 10478 分，而中位是 0）。
const WIN_SCALE: i32 = 50_000;

// ---------------------------------------------------------------------------
// 局面筛选
// ---------------------------------------------------------------------------

/// 观测到的意图 -> `Threat`。**和 `bin/solve` / 探针同一份**：
/// 意图标签是最终值，所以走 `Threat::set` 不走 `set_live`。
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

/// 场上有没有敌人**可能**走随机分支。
///
/// 读的是 `EnemyDef::machine` 里有没有 `Next::Rand` —— 比运行时的
/// `allowed_next` 保守（那只答"下一手"，这里问的是"这一段窗口里会不会分岔"）。
/// **保守的方向是对的**：漏判一只随机敌人会让"最优"这个词在那个起点上失去定义。
fn enemies_random(s: &State) -> bool {
    for e in 0..s.n_enemies as usize {
        if !s.enemies[e].alive() {
            continue;
        }
        let def = enemy_def(s.enemy_def[e]);
        if let Some(m) = def.machine {
            if matches!(m.start, Next::Rand(_))
                || m.after.iter().any(|n| matches!(n, Next::Rand(_)))
            {
                return true;
            }
        }
    }
    false
}

/// 这个局面下，敌人**这一手**里有没有 `Threat` 装不下的 op。
///
/// `injected_enemy_turn` 只跑 `take_attack_hit`；真 `enemy_turn` 跑 13 种 `EOp`。
/// 这个计数回答的是"读数 2 里那部分残差有没有解释得通的来源"。
fn move_has_non_attack_ops(s: &State) -> bool {
    for e in 0..s.n_enemies as usize {
        if !s.enemies[e].alive() {
            continue;
        }
        let def = enemy_def(s.enemy_def[e]);
        let Some(mv) = def.moves.get(current_move_ix(def, s, e)) else { continue };
        // **这一处不用过 `asc::adjust`，是有意的**：它只看 op 的**种类**，
        // 而进阶只改数值，从来不换 `EOp` 的变体。读 `ops` 的八处里唯一一个例外。
        for op in mv.ops {
            match op {
                EOp::Attack { .. }
                | EOp::AttackPlusStackHits { .. }
                | EOp::AttackPlusSelfStatus { .. }
                | EOp::Nothing => {}
                _ => return true,
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// 真口径估值
// ---------------------------------------------------------------------------

/// 一条线在**真口径**下值多少：`replay` 之后走真敌人回合，再 `score::leaf`。
///
/// **敌人的随机分支用固定种子**，兄弟之间共用同一份运气（CRN）。
/// 不钉住的话，排序里混进的是掷骰子的差异而不是决策差异 ——
/// 和 `plan::sample_children` 那条注释是同一件事。
fn true_value(s: &State, line: &Line) -> i32 {
    let Some(after) = replay_line(s, line.acts()) else { return i32::MIN / 2 };
    if after.combat_over {
        return score::leaf(&after);
    }
    let mut a = after;
    a.rng.enemy = 0x5EED_0825;
    score::leaf(&end_turn_before_draw(a))
}

/// 从「敌人打完、未抽牌」出发，**只在这一层确定时继续往下**（`stop` 说了算
/// 什么叫确定），否则就地叶评估。这就是"搜到洗牌边界为止"。
/// 返回 `(值, 实际搜到的层数)`。
///
/// 不做斩杀延伸 —— 这里量的是"深度买到了多少"，延伸是另一个变量。
///
/// **A2 那一列因此是 planner 的下界，不是它的复制品**，两处都往浅了差：
/// planner 会斩杀延伸；planner 的 `left` 在窗口里一分没花（确定层不扣），
/// 窗口尽头那一层它还会**采样**着再搜一层，而这里到边界就停。
/// 到边界就停是刻意的 —— 采样一层就把"最优"从事实变回估计量，
/// 而这三个读数的全部价值就在于它们量的是事实。
#[allow(clippy::too_many_arguments)]
fn window_value(
    s1: &State,
    cfg: &Plan,
    left: u8,
    budget: Budget,
    sc: fn(&State) -> i32,
    depth: usize,
    stop: Stop,
    lf: fn(&State) -> i32,
) -> (i32, u8) {
    if s1.combat_over || s1.player_dead || left == 0 {
        return (lf(s1), 0);
    }
    let kids = chance_children(s1, cfg, depth);
    if kids.len() != 1 || (kids[0].p - 1.0).abs() > 1e-9 {
        return (lf(s1), 0);
    }
    // 单孩子成立，但 planner 还要问第二半：敌人的下一手唯一吗。
    if stop == Stop::Dual && !window_is_certain(s1) {
        DUAL_STOPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        return (lf(s1), 0);
    }
    let kid = &kids[0].state;
    let th = predicted_threat(kid);
    let solved = solve_turn_potions(kid, &th, sc, budget.at(cfg, depth), 0);
    note_solve(solved.complete);
    let line = solved.line;
    match replay_line(kid, line.acts()) {
        Some(after) if !after.combat_over => {
            let s2 = end_turn_before_draw(after);
            let (v, d) = window_value(&s2, cfg, left - 1, budget, sc, depth + 1, stop, lf);
            (v, d + 1)
        }
        Some(after) => (lf(&after), 1),
        None => (lf(kid), 0),
    }
}

/// 一条**根**候选线在某个口径下值多少。
///
/// `lf` 是**这一列停下来时用的叶尺子**。A0/A/B/C 四列一律 `score::leaf`
/// —— 它们是尺子，别动。只有 A2（“planner 今天真的在做的事”）跟着
/// [`Plan::at_root`] 走：阶段 4 之后窄根上那把尺子是 `window_score`，
/// 审计台不跟着换的话，量的就不再是被测对象了。
///
/// **后悔仍然一律在 C 那把尺子上量**（`v_win`），这里换的只是“planner 会挑哪条”。
#[allow(clippy::too_many_arguments)]
fn root_value(
    s: &State,
    line: &Line,
    cfg: &Plan,
    left: u8,
    budget: Budget,
    sc: fn(&State) -> i32,
    stop: Stop,
    lf: fn(&State) -> i32,
) -> (i32, u8) {
    let Some(after) = replay_line(s, line.acts()) else { return (i32::MIN / 2, 0) };
    if after.combat_over {
        return (lf(&after), 0);
    }
    let s1 = end_turn_before_draw(after);
    window_value(&s1, cfg, left, budget, sc, 1, stop, lf)
}

fn argmax(v: &[i32]) -> usize {
    let mut b = 0;
    for i in 1..v.len() {
        if v[i] > v[b] {
            b = i;
        }
    }
    b
}

// ---------------------------------------------------------------------------
// 累计
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Cover {
    starts: usize,
    distinct_sum: usize,
    layers: [usize; 8],
    /// A2（planner 现在真的搜到的深度）实际搜到的层数分布
    layers_a2: [usize; 8],
    /// 牌序整堆已知、**但场上有带随机分支的敌人**因此没进这三个读数的起点。
    ///
    /// 它是「判据为什么要双重确定」那条论据的分母：这些起点上机会节点是
    /// 单孩子（抽牌定死了），可敌人下一手要掷骰 —— 只看单孩子的话，
    /// 确定性窗口会一路借到它们头上。
    dropped_random: usize,
    /// 各口径下的最优根线**不在**默认候选集里
    miss_a: usize,
    miss_a2: usize,
    miss_b: usize,
    miss_c: usize,
    /// 同一候选全集下，换口径换不换根建议
    flip_old_to_cur: usize,
    flip_cur_to_a2: usize,
    flip_cur_to_full: usize,
    flip_full_to_window: usize,
    /// planner 会选的那条，在到边界口径下的后悔。**三个口径各一份** ——
    /// 一次跑完就能读出"换深层目标函数"和"加确定性窗口"各买回了多少，
    /// 不用跑三遍二进制。
    regret_old: Vec<i32>,
    regret: Vec<i32>,
    regret_a2: Vec<i32>,
    /// **planner 真跑一遍**（`plan::plan_report`）留下的账。
    ///
    /// 上面那些读数是审计台**自己搭**的五个口径；这一组是被测对象自己报的，
    /// 两者对不上就说明审计台搭的候选集和 planner 真用的不是一个
    /// —— 而那种错**长得和"planner 有问题"一模一样**。
    plan_cands_sum: usize,
    plan_cands_mismatch: usize,
    narrow: usize,
    tt_probes: u64,
    tt_hits: u64,
}

#[derive(Default)]
struct Inner {
    kids: usize,
    non_attack: usize,
    distinct: Vec<usize>,
    /// 各口径的选择比真口径最优差多少（0 = 已经是最优）
    gap_old: Vec<i32>,
    gap_cur: Vec<i32>,
    gap_full: Vec<i32>,
    n_gap_old: usize,
    n_gap_cur: usize,
    n_gap_full: usize,
    death_cur: usize,
    death_avoidable: usize,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("用法: plan_audit <trace.json>... [--inner] [--all] [--set \"k=v,...\"]");
        eprintln!("  --set    改被测配置。键和 `bin/rollout --alt` 是同一份");
        eprintln!("           （`Plan::apply`）。退回阶段 4 之前用：");
        eprintln!("             --set \"window-score=leaf\"");
        eprintln!("           退回阶段 3 之前（三个旋钮 + 阶段 4）用：");
        eprintln!(
            "             --set \"cand-score=damage,k-certain=off,tt=off,window-score=leaf\""
        );
        eprintln!("  --inner  加量「深层内层最优性」。**慢很多**（要对每个机会节点的孩子");
        eprintln!("           全枚举这一回合的不同终局），第 2/3 幕语料上约 25 分钟");
        eprintln!("  --all    逐回合打印，不只打印有分歧的那些");
        eprintln!();
        eprintln!("  glob 靠 shell 展开，别写 traces/*.json（那目录里还有权威数据表）");
        return ExitCode::from(2);
    }
    let mut paths = Vec::new();
    let mut want_inner = false;
    let mut show_all = false;
    let mut set_spec: Option<String> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--inner" => want_inner = true,
            "--all" => show_all = true,
            "--set" => match it.next() {
                Some(v) => set_spec = Some(v.clone()),
                None => {
                    eprintln!("--set 后面要跟一串 key=value（逗号分隔），键同 bin/rollout --alt");
                    return ExitCode::from(2);
                }
            },
            _ => paths.push(a.clone()),
        }
    }

    let mut cfg = Plan { depth: 2, ..Default::default() };
    if let Some(spec) = &set_spec {
        if let Err(e) = cfg.apply(spec) {
            eprintln!("--set: {e}");
            return ExitCode::from(2);
        }
    }
    let cfg = cfg;
    let cur_budget = cfg.budgets[1];
    let cur_score = cfg.deep_score;
    let old_score = score::damage_first;

    println!("被测配置：{}", cfg.describe());
    println!(
        "口径：A0 旧 = damage_first@{cur_budget} · A 单层 = deep_score@{cur_budget} · \
         A2 现行 = Plan::at_root 的尺子@Plan::budget(层) + 确定性窗口 {} 层 · \
         B 预算给足 = deep_score@{FULL_BUDGET} · C 到边界 = B + 搜到洗牌边界",
        cfg.window
    );
    println!(
        "叶尺子：A0/A/B/C 一律 score::leaf（含 win = 100000）· \
         A2 跟 planner 走（窄根上是窗口目标函数）。**后悔一律在 C 那把尺子上量**"
    );
    println!(
        "根候选集 = solve_turn_topk(cand_score, K=Plan::k_at(局面)+{}能力+{}伤害)，\
         叶子一律 score::leaf(真敌人回合)\n",
        cfg.power_reserve, cfg.damage_reserve
    );

    let mut cover = Cover::default();
    let mut inner = Inner::default();

    for path in &paths {
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("读不了 {path}: {e}");
                return ExitCode::from(2);
            }
        };
        let t: Trace = match parse_trace(&src) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{path}: {e}");
                return ExitCode::from(2);
            }
        };
        let name = std::path::Path::new(path)
            .file_name()
            .map(|x| x.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());

        for &(i, _j) in turn_segments(&t).iter() {
            let start = &t.frames[i];
            if !start.obs.is_play_phase || start.obs.pending {
                continue;
            }
            // 手里有内核不认识的牌 ⇒ 搜的是另一副牌，不能拿来量
            if start.obs.hand.iter().any(|c| lookup_card(&c.name).is_none()) {
                continue;
            }
            let sub = Trace {
                version: t.version,
                run: t.run.clone(),
                ascension: t.ascension,
                frames: t.frames[..=i].to_vec(),
            };
            let Some((r, mut sy, _)) = sync_latest(&sub) else { continue };
            if sy.state.pending != sts2core::state::Pending::None {
                continue;
            }
            let _ = r.identify_enemies(&mut sy.state, &start.obs);
            if !can_rollout(&sy.state) {
                continue;
            }
            let s = sy.state;
            let threat = threat_of(&r, &start.obs);

            // ---- 读数 2：深层内层最优性（每个机会节点的孩子各一条） ----
            if want_inner {
                let (main, pr, dr) = solve_turn_topk_split(
                    &s,
                    &threat,
                    cfg.cand_score,
                    FULL_BUDGET,
                    0,
                    cfg.k_at(&s),
                    cfg.power_reserve,
                    cfg.damage_reserve,
                );
                let cands: Vec<Line> = main.into_iter().chain(pr).chain(dr).collect();
                for c in &cands {
                    let Some(after) = replay_line(&s, c.acts()) else { continue };
                    if after.combat_over {
                        continue;
                    }
                    let s1 = end_turn_before_draw(after);
                    if s1.combat_over || s1.player_dead {
                        continue;
                    }
                    for kid in chance_children(&s1, &cfg, 1) {
                        audit_inner(&kid.state, cur_score, cur_budget, old_score, &mut inner);
                    }
                }
            }

            // ---- 读数 1 和 3：只在确定性起点上 ----
            let known_full = s.n_draw >= 5 && s.n_draw_known == s.n_draw;
            if !known_full {
                continue;
            }
            if enemies_random(&s) {
                cover.dropped_random += 1;
                continue;
            }
            audit_cover(
                &name,
                start.obs.round,
                &s,
                &threat,
                &cfg,
                cur_score,
                cur_budget,
                old_score,
                show_all,
                &mut cover,
            );
        }
    }

    report(&cover, &inner, want_inner);
    ExitCode::SUCCESS
}

/// 读数 2：这一个机会节点的孩子上，四个口径各选了什么线、差真口径最优多少。
#[allow(clippy::too_many_arguments)]
fn audit_inner(
    k: &State,
    cur_score: fn(&State) -> i32,
    cur_budget: u32,
    old_score: fn(&State) -> i32,
    acc: &mut Inner,
) {
    let th = predicted_threat(k);
    acc.kids += 1;
    if move_has_non_attack_ops(k) {
        acc.non_attack += 1;
    }
    let pick = |sc: fn(&State) -> i32, b: u32| {
        true_value(k, &solve_turn_potions(k, &th, sc, b, 0).line)
    };
    let v_old = pick(old_score, cur_budget);
    let v_cur = pick(cur_score, cur_budget);
    let v_full = pick(cur_score, FULL_BUDGET);

    // 真口径的最优：全枚举这一回合的不同终局，逐条评
    let (all, _, _) =
        solve_turn_topk_split(k, &th, cur_score, FULL_BUDGET, 0, 100_000, 0, 0);
    acc.distinct.push(all.len());
    let mut best = i32::MIN / 2;
    for l in &all {
        best = best.max(true_value(k, l));
    }

    for (v, n, g) in [
        (v_old, &mut acc.n_gap_old, &mut acc.gap_old),
        (v_cur, &mut acc.n_gap_cur, &mut acc.gap_cur),
        (v_full, &mut acc.n_gap_full, &mut acc.gap_full),
    ] {
        if best > v {
            *n += 1;
            g.push(best - v);
        }
    }
    // 分低于 −500000 只可能来自 `Weights::death`
    if v_cur < -500_000 {
        acc.death_cur += 1;
        if best > -500_000 {
            acc.death_avoidable += 1;
        }
    }
}

/// 读数 1 和 3：这一个确定性起点上，四个口径各推荐哪条根线。
#[allow(clippy::too_many_arguments)]
fn audit_cover(
    name: &str,
    round: i32,
    s: &State,
    threat: &Threat,
    cfg: &Plan,
    cur_score: fn(&State) -> i32,
    cur_budget: u32,
    old_score: fn(&State) -> i32,
    show_all: bool,
    acc: &mut Cover,
) {
    // 默认候选集（planner 真的会深搜的那几条）。
    // **排序用 `cand_score`、K 用 `k_at`** —— 和 `plan::plan_report` 逐字同一套。
    // 两边不一致的话，这个读数量的就不是 planner 的候选集，而它的全部价值
    // 就在于回答"最优那条线在不在 planner 的候选集里"。
    let (main, pr, dr) = solve_turn_topk_split(
        s,
        threat,
        cfg.cand_score,
        FULL_BUDGET,
        0,
        cfg.k_at(s),
        cfg.power_reserve,
        cfg.damage_reserve,
    );
    let default_cands: Vec<Line> = main.into_iter().chain(pr).chain(dr).collect();
    // **被测对象自己跑一遍**：候选集条数要和上面这份对得上，
    // 置换表的命中率也只有它报得出来（审计台自己那五个口径不走 planner 的入口）。
    let rep = plan_report(s, cfg, threat);
    acc.plan_cands_sum += rep.n_cands;
    acc.plan_cands_mismatch += (rep.n_cands != default_cands.len()) as usize;
    acc.narrow += rep.narrow as usize;
    acc.tt_probes += rep.tt_probes;
    acc.tt_hits += rep.tt_hits;
    // 这一回合**全部**不同终局。K 开到 100000 = 不剪。
    let (all, _, _) = solve_turn_topk_split(s, threat, cfg.score, FULL_BUDGET, 0, 100_000, 0, 0);
    if all.is_empty() {
        return;
    }

    let flat = Budget::Flat(cur_budget);
    let full = Budget::Flat(FULL_BUDGET);
    let one = Stop::SingleChild;
    // **A0/A/B/C 的叶尺子一律 `score::leaf`** —— 它们是尺子，别动。
    let ruler: fn(&State) -> i32 = score::leaf;
    let v_old: Vec<i32> =
        all.iter().map(|l| root_value(s, l, cfg, 1, flat, old_score, one, ruler).0).collect();
    let v_cur: Vec<i32> =
        all.iter().map(|l| root_value(s, l, cfg, 1, flat, cur_score, one, ruler).0).collect();
    let v_full: Vec<i32> =
        all.iter().map(|l| root_value(s, l, cfg, 1, full, cur_score, one, ruler).0).collect();
    // **A2 = planner 今天真的在做的事**：计划的那一层 + 确定性窗口白送的几层，
    // 预算按 `Plan::budget(层)`，停法用双重确定判据。
    // `1 + cfg.window` 是 planner 的深度上限（确定层不扣 `left`，见 `Plan::window`）。
    // **A2 跟着 planner 真的在用的那把尺子**（阶段 4：窄根上是 `window_score`）。
    // 这一列的用处是“planner 会挑哪条根线”，尺子不跟着换就答错了这个问题。
    let eff = cfg.at_root(s);
    let a2: Vec<(i32, u8)> = all
        .iter()
        .map(|l| {
            root_value(
                s,
                l,
                cfg,
                1 + cfg.window,
                Budget::PerDepth,
                eff.deep_score,
                Stop::Dual,
                eff.leaf.score_fn(),
            )
        })
        .collect();
    let v_a2: Vec<i32> = a2.iter().map(|x| x.0).collect();
    let win: Vec<(i32, u8)> = all
        .iter()
        .map(|l| root_value(s, l, cfg, MAX_WINDOW_LAYERS, full, cur_score, one, ruler))
        .collect();
    let v_win: Vec<i32> = win.iter().map(|x| x.0).collect();
    let layers = win.iter().map(|x| x.1).max().unwrap_or(0);
    let layers_a2 = a2.iter().map(|x| x.1).max().unwrap_or(0);

    // 名字比对：`Line` 里的手牌下标会漂移，只有翻译成牌名才可比
    let names: Vec<Vec<String>> = all.iter().map(|l| explain(s, l.acts())).collect();
    let default_names: Vec<Vec<String>> =
        default_cands.iter().map(|c| explain(s, c.acts())).collect();
    let in_default = |ix: usize| default_names.iter().any(|d| *d == names[ix]);

    // planner 实际会选的那条 = 默认候选集里按某个口径最优的。
    // **`A0 旧` 那份要一起算**：两条后悔的差就是换深层目标函数买回来的东西。
    let pick_default = |v: &Vec<i32>| -> usize {
        let mut b = None::<usize>;
        for ix in 0..all.len() {
            if in_default(ix) && b.map_or(true, |x| v[ix] > v[x]) {
                b = Some(ix);
            }
        }
        b.unwrap_or(0)
    };
    let best_def = pick_default(&v_cur);
    let best_def_old = pick_default(&v_old);
    let best_def_a2 = pick_default(&v_a2);

    let (a_old, a_cur, a_a2, a_full, a_win) =
        (argmax(&v_old), argmax(&v_cur), argmax(&v_a2), argmax(&v_full), argmax(&v_win));

    acc.starts += 1;
    acc.distinct_sum += all.len();
    acc.layers[(layers as usize).min(7)] += 1;
    acc.layers_a2[(layers_a2 as usize).min(7)] += 1;
    let miss_a = !in_default(a_cur);
    let miss_b = !in_default(a_full);
    let miss_c = !in_default(a_win);
    acc.miss_a += miss_a as usize;
    acc.miss_a2 += !in_default(a_a2) as usize;
    acc.miss_b += miss_b as usize;
    acc.miss_c += miss_c as usize;
    let f1 = names[a_old] != names[a_cur];
    let f1b = names[a_cur] != names[a_a2];
    let f2 = names[a_cur] != names[a_full];
    let f3 = names[a_full] != names[a_win];
    acc.flip_old_to_cur += f1 as usize;
    acc.flip_cur_to_a2 += f1b as usize;
    acc.flip_cur_to_full += f2 as usize;
    acc.flip_full_to_window += f3 as usize;
    // **后悔一律在 C 那把尺子上量**，三列差的是"planner 会选哪条"，不是尺子。
    let regret = v_win[a_win] - v_win[best_def_a2];
    acc.regret_a2.push(regret);
    acc.regret.push(v_win[a_win] - v_win[best_def]);
    acc.regret_old.push(v_win[a_win] - v_win[best_def_old]);

    if show_all || miss_a || miss_c || f1 || f1b || f2 || f3 || regret > 0 {
        println!(
            "{name} 回合{round} | n_draw={} 终局={} 候选={} 窗口层数 A2:{}/C:{} | \
             漏最优 A:{} C:{} | 翻转 旧→单层:{} 单层→加窗口:{} 现→满预算:{} 满预算→到边界:{} | \
             后悔 {regret} 分",
            s.n_draw,
            all.len(),
            default_cands.len(),
            layers_a2,
            layers,
            miss_a,
            miss_c,
            f1,
            f1b,
            f2,
            f3
        );
    }
    if show_all || miss_c || regret > 0 {
        println!("    planner 会选 = {}", names[best_def_a2].join(">"));
        println!("    加窗口之前   = {}", names[best_def].join(">"));
        println!("    到边界最优   = {}", names[a_win].join(">"));
    }
}

// ---------------------------------------------------------------------------
// 报告
// ---------------------------------------------------------------------------

fn stats(v: &[i32]) -> String {
    if v.is_empty() {
        return "n=0".into();
    }
    let mut s = v.to_vec();
    s.sort_unstable();
    let mean = v.iter().map(|&x| x as f64).sum::<f64>() / v.len() as f64;
    format!(
        "n={} 均值 {:.0} 分 (≈{:.2} 血) · 中位 {} · 最大 {}",
        v.len(),
        mean,
        mean / 100.0,
        s[s.len() / 2],
        s[s.len() - 1]
    )
}

fn report(c: &Cover, i: &Inner, want_inner: bool) {
    let n = c.starts.max(1);
    println!("\n==== 读数 1 · 候选覆盖 ====");
    println!(
        "确定性起点 {} 个（牌序整堆已知 ≥5 张 且 场上无随机分支敌人）· \
         根回合平均不同终局 {:.1}",
        c.starts,
        c.distinct_sum as f64 / n as f64
    );
    println!(
        "另有 {} 个起点牌序整堆已知、**但场上有带随机分支的敌人**，没进这三个读数 ——\n\
  它们正是「判据必须是双重确定」的那批：抽牌定死了，敌人下一手却要掷骰。",
        c.dropped_random
    );
    println!("到洗牌边界实际搜到的层数分布（下标 = 层数）: {:?}", c.layers);
    println!(
        "planner（A2，确定性窗口）实际搜到的层数分布:       {:?}\n\
  两行的差 = 窗口还没吃到的深度。**A2 那行全堆在 0/1 上就说明窗口一次都没开** ——\n\
  那时下面所有 A→A2 的读数都不构成证据。",
        c.layers_a2
    );
    {
        use std::sync::atomic::Ordering::Relaxed;
        let d = DUAL_STOPS.load(Relaxed);
        println!(
            "  双重确定判据比单孩子早停 {d} 层（起点是按「机器里没有 Next::Rand」筛的，\n\
    所以这个数该很小；不为 0 的来源只有 `Next::Cond` 判不出条件、\n\
    或者窗口里召唤出了带随机分支的敌人 —— 那时 A2 比 C 浅的那部分就不全是预算）"
        );
    }
    {
        use std::sync::atomic::Ordering::Relaxed;
        let (n, bad) = (SOLVES.load(Relaxed), SOLVES_INCOMPLETE.load(Relaxed));
        println!(
            "  深层单回合搜索 {} 次，**没搜完** {} 次 = {:.0}%（五个口径合计；             没搜完的那条不是解，是 DFS 按手牌顺序碰到的线）",
            n,
            bad,
            100.0 * bad as f64 / n.max(1) as f64
        );
    }
    println!(
        "planner 自己报的账（`plan::plan_report`，被测对象而不是审计台搭的口径）:\n  \
         窄根 {}/{} · 候选集平均 {:.1} 条 · **和审计台搭的那份条数对不上 {} 次**\n  \
         置换表 probes {} · hits {} = {:.1}%\n  \
         **命中率为 0 就是 `plan::key` 写错了** —— 它不会让任何东西变红。",
        c.narrow,
        c.starts,
        c.plan_cands_sum as f64 / n as f64,
        c.plan_cands_mismatch,
        c.tt_probes,
        c.tt_hits,
        100.0 * c.tt_hits as f64 / c.tt_probes.max(1) as f64
    );
    println!("  A  单层口径的最优根线**不在**候选集: {}/{}", c.miss_a, c.starts);
    println!("  A2 现行（加窗口）的最优根线不在候选集: {}/{}", c.miss_a2, c.starts);
    println!("  B  预算给足口径的最优根线不在候选集: {}/{}", c.miss_b, c.starts);
    println!("  C  到边界口径的最优根线不在候选集: {}/{}", c.miss_c, c.starts);
    println!(
        "  **这一栏对目标函数和叶评估免疫** —— 不在集合里的线，评得再准也评不到。\n\
     \x20 换口径换不换根建议（同一候选全集）: 旧→单层 {}/{} · **单层→加窗口 {}/{}** · \
       现→满预算 {}/{} · 满预算→到边界 {}/{}",
        c.flip_old_to_cur,
        c.starts,
        c.flip_cur_to_a2,
        c.starts,
        c.flip_cur_to_full,
        c.starts,
        c.flip_full_to_window,
        c.starts
    );

    println!("\n==== 读数 3 · 到边界后悔（planner 会选的那条 vs 窗口内最优）====");
    println!(
        "  **这一栏有个下限，别拿它和 0 比**：到边界最优根线不在候选集时后悔必然 > 0，\n\
    所以「后悔>0」最低只能降到 miss_c = **{}/{}**。低于这个数只可能是量错了。\n\
    A2（现行默认）离这个下限还有 **{}** 个起点 —— 那才是目标函数和深度还能动的部分。",
        c.miss_c,
        c.starts,
        c.regret_a2.iter().filter(|&&x| x > 0).count().saturating_sub(c.miss_c)
    );
    for (tag, v) in [
        ("A0 旧口径    ", &c.regret_old),
        ("A  单层      ", &c.regret),
        ("A2 现行+窗口 ", &c.regret_a2),
    ] {
        let pos: Vec<i32> = v.iter().copied().filter(|&x| x > 0).collect();
        let (big, small): (Vec<i32>, Vec<i32>) = pos.iter().partition(|&&x| x >= WIN_SCALE);
        println!(
            "  {tag} 后悔>0 {}/{} · 含 win 那一档（≥{WIN_SCALE} 分，差的是「早一回合打完」）{} 例 {:?}",
            pos.len(),
            c.starts,
            big.len(),
            big
        );
        println!("            不含 win: {}", stats(&small));
    }
    println!(
        "  **两列必须分开读**：一个 win 是 100000 分 = 1000 点血，\n\
     \x20 混在一起均值会被四五个样本完全带走（而中位是 0）。"
    );
    println!(
        "  **阶段 4 要按这条分界读这两列**：裁判（C）是 `score::leaf`，它自己带着\n\
     \x20 那个 win = 100000。所以\n\
     \x20 · **不含 win 那一列是口径中立的** —— 两条线要么都赢要么都不赢，\n\
     \x20   `Weights::LEAF` 和 `Weights::WINDOW` 给它们的**排序逐字相同**\n\
     \x20   （两把尺子只在「赢下来的终局」上差一个常数）。这一列可以直接比较。\n\
     \x20 · **含 win 那一列是两把尺子打架的地方**，不是一个可以直接比较的数：\n\
     \x20   裁判说「在边界之前打完值 1000 血」，而阶段 4 说那是地平线人造物。\n\
     \x20   它涨了不等于变差 —— 真正的裁判是 P5 端到端的血量和 P4 截断率。"
    );

    if !want_inner {
        println!("\n（读数 2「深层内层最优性」没跑 —— 加 --inner）");
        return;
    }
    let m = i.kids.max(1);
    let mut d = i.distinct.clone();
    d.sort_unstable();
    println!("\n==== 读数 2 · 深层内层最优性 ====");
    println!(
        "深层机会节点的孩子 {} 个 · 这一回合不同终局 中位 {} / 均值 {:.0} / 最大 {}",
        i.kids,
        d.get(d.len() / 2).copied().unwrap_or(0),
        i.distinct.iter().sum::<usize>() as f64 / m as f64,
        d.last().copied().unwrap_or(0)
    );
    println!(
        "敌人这一手含 `Threat` 装不下的 op: {}/{} = {:.0}%",
        i.non_attack,
        i.kids,
        100.0 * i.non_attack as f64 / m as f64
    );
    println!("  A0 旧口径的选择不是真口径最优: {}/{}；{}", i.n_gap_old, i.kids, stats(&i.gap_old));
    println!("  A  现行口径不是真口径最优:     {}/{}；{}", i.n_gap_cur, i.kids, stats(&i.gap_cur));
    println!("  B  预算给足后仍然不是最优:     {}/{}；{}", i.n_gap_full, i.kids, stats(&i.gap_full));
    println!(
        "  **B 那一行是 `Threat` 只装攻击伤害的残差** —— 内层取 argmax 的是\n\
     \x20 `threat.end_turn(·)`（只挨攻击、已抽牌），父节点评的是\n\
     \x20 `end_turn_before_draw(·)`（真敌人回合、未抽牌），不是同一个局面。\n\
     \x20 换目标函数修不掉它，那是另一条改动。"
    );
    println!(
        "  现行口径选出的线在真口径下判死: {}/{}，其中全枚举能不死: {}",
        i.death_cur, i.kids, i.death_avoidable
    );
}
