//! **整场战斗的最小战损**：跨回合束搜索（beam search）。
//!
//! # 它和 `bin/rollout` / `plan` 的分工
//!
//! | | 答的问题 | 谁在用 |
//! |---|---|---|
//! | `rollout` | 「**这个策略**打完这场仗剩多少血」 | 验收 P2/P5 |
//! | `plan` | 「**限深两回合**内，这一手该打什么」 | 实战并排报的那条线 |
//! | **本文件** | 「这场仗**最好能打成什么样**」 | 上限 / 诊断 |
//!
//! 前两个都是**策略**，受限于深度和预算；这个是**离线的上界估计** ——
//! 它可以花几分钟去搜一条 planner 在 200ms 里搜不到的线。
//!
//! **它给的是下界不是真最优**：束宽有限、候选集来自 `solve_turn_topk`
//! （单回合口径），所以真正的最优只会**更好**。报出来的数字要这么读：
//! 「至少存在一条打成这样的线」。
//!
//! # 随机性怎么处理
//!
//! 一场仗里有三条随机流（洗牌 / 敌人 / 生成）。抽牌堆开局整堆已知
//! （`draw_pile_order` 补丁），但**洗牌之后就不可知**了，而这副牌里还有
//! 添柴+（消耗手牌、生成等量随机牌）这种大方差的牌。
//!
//! 所以「最小战损」只在**给定一条随机轨迹**下才有定义。做法是：
//! 每个种子各搜一遍，报**分布**（最好 / 中位 / 最差），而不是一个数。
//! 同一个种子下束搜索是确定的。
//!
//! ```text
//! bestline <trace.json> [--frame N] [--beam W] [--cands K] [--seeds S]
//!                       [--budget B] [--max-turns T] [--verbose]
//! ```

use std::process::ExitCode;

use sts2core::plan;
use sts2core::replay::{parse_trace, sync_latest};
use sts2core::rollout;
use sts2core::solver::{self, score, Line};
use sts2core::state::State;
use sts2core::step::{step, Action};

/// 束里的一个节点：局面 + 走到这里的每回合出牌。
#[derive(Clone)]
struct Node {
    s: State,
    /// 每个回合一行，已经渲染成人话 —— `Line` 里的 `hand` 下标出了那一帧就没意义了。
    log: Vec<String>,
}

fn render(s: &State, line: &Line) -> String {
    let mut cur = *s;
    let mut parts: Vec<String> = Vec::new();
    for a in line.acts() {
        match *a {
            Action::PlayCard { hand, .. } => {
                let name = if (hand as usize) < cur.n_hand as usize {
                    let inst = cur.cards[cur.hand[hand as usize] as usize];
                    let mut n = sts2core::content::card(inst.id).name.to_string();
                    if inst.upgraded() {
                        n.push('+');
                    }
                    n
                } else {
                    "?".into()
                };
                parts.push(name);
            }
            Action::UsePotion { slot, .. } => parts.push(format!("药[{slot}]")),
            Action::Choose { .. } => parts.push("选".into()),
            Action::EndTurn => {}
        }
        cur = step(cur, *a);
    }
    if parts.is_empty() {
        "（不出牌）".into()
    } else {
        parts.join(" -> ")
    }
}

/// 走完一个回合：照 `line` 出牌 -> 闭掉子选择 -> `step(EndTurn)` 让 `EnemyDef` 真的打。
///
/// **回合边界不用注入威胁**（那是搜索叶子的事）。这里要的是真实结算，
/// 和 `rollout::rollout_to_state` 同一条路 —— 两边分岔的话这个上界就不是
/// 同一个游戏里的上界了。
fn advance(mut s: State, line: &Line) -> State {
    for a in line.acts() {
        s = step(s, *a);
        if s.combat_over {
            return s;
        }
    }
    rollout::close_pending(&mut s, &mut None);
    if s.combat_over {
        return s;
    }
    step(s, Action::EndTurn)
}

/// **标定用的目标函数族**：`Weights::clock`（即死倒计时的折扣）取不同的值。
///
/// `Policy::Solver` 要的是 `fn(&State) -> i32` 这个函数指针类型，
/// 所以只能一个刻度一个函数 —— 这是**诊断台的代码，不是内核的**：
/// 内核那一份权重只有一套（`Weights::SURVIVE_FIRST` 等），
/// 这里是拿它做 A/B，量出来之后才回填。
macro_rules! clock_scores {
    ($($name:ident = $base:ident, $pct:expr;)*) => {
        $(fn $name(s: &State) -> i32 {
            solver::eval(s, &solver::Weights { clock: $pct, ..solver::Weights::$base })
        })*
    };
}
clock_scores! {
    dmg_c0   = DAMAGE_FIRST,   0;
    dmg_c25  = DAMAGE_FIRST,  25;
    dmg_c50  = DAMAGE_FIRST,  50;
    dmg_c100 = DAMAGE_FIRST, 100;
    dmg_c200 = DAMAGE_FIRST, 200;
    srv_c0   = SURVIVE_FIRST,   0;
    srv_c25  = SURVIVE_FIRST,  25;
    srv_c50  = SURVIVE_FIRST,  50;
    srv_c100 = SURVIVE_FIRST, 100;
    srv_c200 = SURVIVE_FIRST, 200;
}

/// `--sweep` 扫的那张表。名字就是报告里那一列。
const SWEEP: &[(&str, fn(&State) -> i32)] = &[
    ("竞速 clock=0", dmg_c0),
    ("竞速 clock=25", dmg_c25),
    ("竞速 clock=50", dmg_c50),
    ("竞速 clock=100", dmg_c100),
    ("竞速 clock=200", dmg_c200),
    ("存活 clock=0", srv_c0),
    ("存活 clock=25", srv_c25),
    ("存活 clock=50", srv_c50),
    ("存活 clock=100", srv_c100),
    ("存活 clock=200", srv_c200),
];

/// 束里排序用的分数。**终局用真结局，中途用叶评估。**
///
/// 中途那一半只能用代理量（`score::leaf` 是标定过的那把），
/// 但终局不需要代理：赢了就是最终血量，死了就是死。
fn node_score(s: &State) -> i64 {
    if s.player_dead {
        return -1_000_000_000;
    }
    if s.combat_over {
        // 赢下来的局面：**最终血量就是答案本身**，不需要代理量。
        // 加一个大常数把所有赢线排在所有没打完的线前面。
        return 1_000_000_000 + s.player.hp as i64;
    }
    score::leaf(s) as i64
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("用法: bestline <trace.json> [选项]");
        eprintln!("  --frame N     从第 N 帧的观测开始（默认 0）");
        eprintln!("  --beam W      束宽（默认 64）");
        eprintln!("  --cands K     每个局面取几条候选线（默认 12）");
        eprintln!("  --seeds S     跑几个随机种子（默认 8）");
        eprintln!("  --budget B    单回合搜索的节点预算（默认 200000）");
        eprintln!("  --max-turns T 回合上限（默认 40）");
        eprintln!("  --verbose     把每个种子的最优线逐回合印出来");
        eprintln!("  --policies    同一批种子上并排跑单回合策略和 planner，量差距");
        eprintln!("  --pol-budget B 上面那两条策略每回合的节点预算（默认 2000，生产口径）");
        eprintln!("  --explain N   把第 N 个种子上两条策略的整场逐回合印出来");
        eprintln!("  --sweep       扫 Weights::clock 的刻度，量赢面和平均血量");
        eprintln!("  --pol-plan S  给 --policies/--explain 里的 planner 传一串 key=value（同 --alt）");
        return ExitCode::from(2);
    }
    let mut path = String::new();
    let mut frame = 0usize;
    let mut beam = 64usize;
    let mut cands = 12usize;
    let mut seeds = 8usize;
    let mut budget = 200_000u32;
    let mut max_turns = 40usize;
    let mut verbose = false;
    let mut policies = false;
    let mut pol_budget = rollout::ROLLOUT_BUDGET;
    let mut explain: Option<usize> = None;
    let mut sweep = false;
    let mut plan_set: Option<String> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        fn num<T: std::str::FromStr>(it: &mut std::slice::Iter<String>) -> Option<T> {
            it.next().and_then(|v| v.parse().ok())
        }
        match a.as_str() {
            "--frame" => frame = num::<usize>(&mut it).unwrap_or(0),
            "--beam" => beam = num::<usize>(&mut it).unwrap_or(64),
            "--cands" => cands = num::<usize>(&mut it).unwrap_or(12),
            "--seeds" => seeds = num::<usize>(&mut it).unwrap_or(8),
            "--budget" => budget = num::<u32>(&mut it).unwrap_or(200_000),
            "--max-turns" => max_turns = num::<usize>(&mut it).unwrap_or(40),
            "--verbose" => verbose = true,
            "--policies" => policies = true,
            "--pol-budget" => pol_budget = num::<u32>(&mut it).unwrap_or(rollout::ROLLOUT_BUDGET),
            "--explain" => explain = num::<usize>(&mut it),
            "--sweep" => sweep = true,
            "--pol-plan" => plan_set = it.next().cloned(),
            other => path = other.to_string(),
        }
    }

    let src = match std::fs::read_to_string(&path) {
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
    // 只留到 `frame` 为止 —— `sync_latest` 拿的是最后一帧，而观测里没有的
    // 每回合计数器只能从这一回合的开头一路走过来。
    let mut cut = t.clone();
    cut.frames.truncate(frame + 1);
    let Some((r, mut sy, _hist)) = sync_latest(&cut) else {
        eprintln!("这个文件里一帧都没有");
        return ExitCode::from(2);
    };
    let obs = &cut.frames[frame].obs;
    let ident = r.identify_enemies(&mut sy.state, obs);
    let start = sy.state;

    println!("=== {path} 帧{frame} — {} ===", cut.run);
    println!(
        "起点  我 {}/{} 血 格挡 {} | 能量 {}/{} | 回合 {}",
        start.player.hp,
        start.player.max_hp,
        start.player.block,
        start.energy,
        start.base_energy,
        start.turn
    );
    for id in &ident {
        println!("敌人  槽{} {} def={:?} 对齐={}", id.slot, id.name, id.def, id.aligned);
    }
    if !rollout::can_rollout(&start) {
        println!("\n**认不出敌人，跨回合推演不可用** —— 这个上界没有意义，停在这里。");
        return ExitCode::from(2);
    }
    if let Some(n) = solver::death_clock(&start) {
        println!("倒计时 沙坑 {n} 层 —— 还剩 {n} 个我的回合，归零直接死");
    }
    println!(
        "配置  束宽 {beam} · 每局面 {cands} 条候选 · {seeds} 个种子 · 预算 {budget} · 上限 {max_turns} 回合\n"
    );

    // `--sweep`：`Weights::clock` 的标定台。同一批种子上把每个刻度各推一场，
    // 报**赢几个 / 平均最终血量**。这两个数才是判据 —— 中间任何"结构上更对"
    // 的读数都不能替代它们（这条教训是 planner 阶段 1/2 留下的）。
    if sweep {
        println!("{:<16} {:>6} {:>10} {:>10}", "目标函数", "赢/总", "平均血量", "平均回合");
        for (name, sc) in SWEEP {
            let mut won = 0usize;
            let mut hp = 0i32;
            let mut turns = 0u32;
            for si in 0..seeds {
                let seed = 0x9E37_79B9_7F4A_7C15u64.wrapping_mul(si as u64 + 1);
                let mut s0 = start;
                s0.rng.shuffle ^= seed;
                s0.rng.gen ^= seed.rotate_left(17);
                let o = rollout::rollout_outcome_with(
                    s0,
                    max_turns,
                    rollout::Policy::Solver { budget: pol_budget, score: *sc },
                );
                if o.won {
                    won += 1;
                    hp += o.final_hp;
                }
                turns += o.turns;
            }
            println!(
                "{name:<16} {:>3}/{:<2} {:>10.1} {:>10.1}",
                won,
                seeds,
                hp as f32 / seeds as f32,
                turns as f32 / seeds as f32
            );
        }
        return ExitCode::SUCCESS;
    }

    // `--explain N`：把第 N 个种子上**单回合策略和 D=2** 的整场逐回合印出来。
    // 「它为什么输」这个问题只有逐回合的读数答得了 —— 是没买够回合（沙坑），
    // 还是买够了但打不动（伤害）。
    if let Some(si) = explain {
        let seed = 0x9E37_79B9_7F4A_7C15u64.wrapping_mul(si as u64 + 1);
        for (label, pl) in [
            (
                "单回合",
                rollout::Policy::Solver { budget: pol_budget, score: score::damage_first },
            ),
            ("D=2", {
                let mut cfg = plan::Plan { depth: 2, ..plan::Plan::default() };
                cfg.seed = seed;
                cfg.budgets[0] = pol_budget;
                if let Some(spec) = &plan_set {
                    cfg.apply(spec).expect("--pol-plan 解析失败");
                }
                rollout::Policy::Plan(cfg)
            }),
        ] {
            let mut s = start;
            s.rng.shuffle ^= seed;
            s.rng.gen ^= seed.rotate_left(17);
            println!("--- 种子 {si} · 策略 {label} ---");
            for turn in 1..=max_turns {
                if s.combat_over {
                    break;
                }
                let before = s;
                let mut rec = Some(Vec::new());
                rollout::play_turn_rec(&mut s, pl, &mut rec);
                let line = {
                    let mut l = Line::EMPTY;
                    for a in rec.unwrap_or_default() {
                        l.push_pub(a);
                    }
                    l
                };
                println!(
                    "  回合{turn:<3} 我{:>3}血 沙坑{:<2} 敌{:>4}血 | {}",
                    before.player.hp,
                    solver::death_clock(&before).unwrap_or(0),
                    before.enemies[0].hp,
                    render(&before, &line)
                );
                if s.combat_over {
                    break;
                }
                s = step(s, Action::EndTurn);
            }
            println!(
                "  结局：{} 我{}血 敌{}血
",
                if s.player_dead { "**死**" } else if s.combat_over { "赢" } else { "没打完" },
                s.player.hp,
                s.enemies[0].hp.max(0)
            );
        }
        return ExitCode::SUCCESS;
    }

    let mut results: Vec<(u64, i32, bool, Vec<String>)> = Vec::new();
    // **同一批种子**上的三条策略读数（`--policies`）。配对比较才说明问题 ——
    // 不同种子之间这场仗的难度差着 20 点血。
    let mut pol: Vec<(i32, i32, i32)> = Vec::new();
    let mut poldiag: Vec<(u32, i32, u32, i32)> = Vec::new();
    for si in 0..seeds {
        let seed = 0x9E37_79B9_7F4A_7C15u64.wrapping_mul(si as u64 + 1);
        let mut s0 = start;
        s0.rng.shuffle ^= seed;
        s0.rng.gen ^= seed.rotate_left(17);
        if policies {
            let one = rollout::rollout_outcome_with(
                s0,
                max_turns,
                rollout::Policy::Solver {
                    budget: pol_budget,
                    score: score::damage_first,
                },
            );
            let mut cfg = plan::Plan { depth: 2, ..plan::Plan::default() };
            cfg.seed = seed;
            cfg.budgets[0] = pol_budget;
            if let Some(spec) = &plan_set {
                if let Err(e) = cfg.apply(spec) {
                    eprintln!("--pol-plan: {e}");
                    return ExitCode::from(2);
                }
            }
            let two = rollout::rollout_outcome_with(s0, max_turns, rollout::Policy::Plan(cfg));
            poldiag.push((one.turns, one.enemy_hp_left, two.turns, two.enemy_hp_left));
            pol.push((
                if one.won { one.final_hp } else { -1 },
                if two.won { two.final_hp } else { -1 },
                0,
            ));
        }
        // **敌人流不动。** 这只 Boss 的出招是纯确定的定环，换它只会让
        // 不同种子之间不可比；真正的方差来自洗牌和生成。
        let mut nodes = vec![Node { s: s0, log: Vec::new() }];
        let mut best: Option<Node> = None;

        for _turn in 0..max_turns {
            let mut next: Vec<Node> = Vec::new();
            for n in &nodes {
                let threat = rollout::predicted_threat(&n.s);
                let lines = solver::solve_turn_topk(
                    &n.s,
                    &threat,
                    score::leaf,
                    budget,
                    0,
                    cands,
                    2,
                    2,
                );
                let lines = if lines.is_empty() { vec![Line::EMPTY] } else { lines };
                for l in &lines {
                    let after = advance(n.s, l);
                    let mut log = n.log.clone();
                    log.push(render(&n.s, l));
                    next.push(Node { s: after, log });
                }
            }
            // 去重：同一个局面从不同顺序走到，留一条就够。`plan::key` 含
            // 跨回合会变的那几样（`solver::key` 省掉了它们，见 plan.rs）。
            next.sort_by_key(|n| std::cmp::Reverse(node_score(&n.s)));
            let mut seen = std::collections::HashSet::new();
            next.retain(|n| seen.insert(plan::key(&n.s)));
            for n in &next {
                if n.s.combat_over && !n.s.player_dead {
                    let better = best.as_ref().is_none_or(|b| b.s.player.hp < n.s.player.hp);
                    if better {
                        best = Some(n.clone());
                    }
                }
            }
            next.retain(|n| !n.s.combat_over);
            next.truncate(beam);
            if next.is_empty() {
                break;
            }
            nodes = next;
        }

        let tail = if policies {
            let (a, b, _) = *pol.last().unwrap();
            let f = |v: i32| if v < 0 { "死".to_string() } else { format!("{v}") };
            let (t1, e1, t2, e2) = *poldiag.last().unwrap();
            format!("   | 单回合 {}(第{t1}回合,敌剩{e1}) · D=2 {}(第{t2}回合,敌剩{e2})", f(a), f(b))
        } else {
            String::new()
        };
        match best {
            Some(b) => {
                println!(
                    "种子 {si}  **赢**  最终 {} 血（战损 {}）· {} 个回合{tail}",
                    b.s.player.hp,
                    start.player.hp - b.s.player.hp,
                    b.log.len()
                );
                results.push((seed, b.s.player.hp, true, b.log));
            }
            None => {
                println!("种子 {si}  ✗ 束宽 {beam} 之内**没有找到活线**{tail}");
                results.push((seed, -1, false, Vec::new()));
            }
        }
    }
    if policies {
        let n = pol.len().max(1) as i32;
        let alive = |v: &Vec<(i32, i32, i32)>, f: fn(&(i32, i32, i32)) -> i32| {
            v.iter().filter(|x| f(x) >= 0).count()
        };
        let mean = |v: &Vec<(i32, i32, i32)>, f: fn(&(i32, i32, i32)) -> i32| {
            v.iter().map(|x| f(x).max(0)).sum::<i32>() as f32 / n as f32
        };
        let orc: Vec<(i32, i32, i32)> =
            results.iter().map(|r| (if r.2 { r.1 } else { -1 }, 0, 0)).collect();
        println!("
策略对照（同一批种子，配对）");
        println!("  {:<10} {:>6} {:>10}", "", "赢几个", "平均血量");
        println!("  {:<10} {:>6} {:>10.1}", "束搜索", alive(&orc, |x| x.0), mean(&orc, |x| x.0));
        println!("  {:<10} {:>6} {:>10.1}", "单回合", alive(&pol, |x| x.0), mean(&pol, |x| x.0));
        println!("  {:<10} {:>6} {:>10.1}", "D=2", alive(&pol, |x| x.1), mean(&pol, |x| x.1));
        println!(
            "  **束搜索是上界，不是策略** —— 它看得见这条随机轨迹的未来，"
        );
        println!("  两条策略看不见。差出来的那一截是「还能再搜到多少」的量度。");
    }

    let wins: Vec<i32> = results.iter().filter(|r| r.2).map(|r| r.1).collect();
    println!("\n---");
    if wins.is_empty() {
        println!("**{seeds} 个种子一条活线都没搜到。**");
        println!("这不等于「这场仗打不赢」—— 束宽/候选数/内容缺口都可能是原因。");
        return ExitCode::SUCCESS;
    }
    let mut sorted = wins.clone();
    sorted.sort_unstable();
    let best_hp = *sorted.last().unwrap();
    let worst_hp = sorted[0];
    let med = sorted[sorted.len() / 2];
    println!(
        "赢下来 {}/{seeds} 个种子 · 最终血量 最好 {best_hp} / 中位 {med} / 最差 {worst_hp}",
        wins.len()
    );
    println!(
        "**战损**（起点 {} 血）：最小 {} / 中位 {} / 最大 {}",
        start.player.hp,
        start.player.hp - best_hp,
        start.player.hp - med,
        start.player.hp - worst_hp
    );

    if verbose {
        for (seed, hp, won, log) in &results {
            if !won {
                continue;
            }
            println!("\n--- 种子 {seed:#x} 最终 {hp} 血 ---");
            for (i, l) in log.iter().enumerate() {
                println!("  回合{:<3} {l}", i + 1);
            }
        }
    } else if let Some((_, hp, _, log)) = results.iter().filter(|r| r.2).max_by_key(|r| r.1) {
        println!("\n--- 最好的那条线（最终 {hp} 血）---");
        for (i, l) in log.iter().enumerate() {
            println!("  回合{:<3} {l}", i + 1);
        }
    }
    ExitCode::SUCCESS
}
