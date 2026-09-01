//! 极速 Rollout 策略与全场战斗期望模拟器（MCTS 评估层）。
//!
//! 这一层利用 L1 纯函数 POD 架构每秒 740 万步的吞吐量，
//! 从当前回合结束状态快速推演到整场战斗结束（combat_over），
//! 用「整场战斗最终结算生命值期望」评估当前决策的长期战略收益。
//!
//! # 它有一个硬前提：**局面里的敌人必须是认得出来的**
//!
//! 推演靠 `EnemyDef::moves` + 出招指针推导敌人每一手。而 `replay::sync`
//! 同步出来的局面里，敌人一律是 `enemy::UNKNOWN`（约束 4：对拍时内核不预测
//! 敌人）—— 它唯一的一手是 `EOp::Nothing`。
//!
//! 直接在这种局面上推演，得到的是**一场敌人永远不出手的仗**。实测（2026-08-17）：
//!
//! ```text
//! 同步局面（敌人=UNKNOWN） 我 12 血 vs 200 血敌人 -> rollout 认为最终 HP = 12
//! 同一局面但敌人=小啃兽（有出招表）             -> rollout 认为最终 HP = 3
//! ```
//!
//! 12 血打 200 血一滴不掉。这不是"近似"，是敌人不存在 —— 正是
//! `sts2core/CLAUDE.md` 开头那条教训（"搜的是一个不存在的游戏"）。
//!
//! 所以：**调用方必须先过 [`can_rollout`]**，认不出敌人就拒绝作答，
//! 不要拿一个自信的数字去做决策。认敌人走
//! `replay::Replayer::identify_enemies`。

use crate::content::{card, enemy_def, playable};
use crate::ops::{EOp, Kind};
use crate::solver::Threat;
use crate::state::{Pending, State, St};
use crate::step::{effective_cost, step, Action};

/// 轻量启发式快速出牌：在一回合内打出较为合理的牌序（微秒级）。
///
/// 优先级：
/// 1. 斩杀扫描：若某张攻击牌能直接击杀活着的敌人，优先打出完成减员；
/// 2. 能力牌优先：早打早享受长期收益（恶魔形态、薪火、无惧疼痛等）；
/// 3. 防御扫描：计算敌人意图来袭伤害，若有威胁且格挡不足，优先打防御牌；
/// 4. 剩余能量输出：打出剩余最高伤害的攻击牌/技能牌。
pub fn fast_play_turn(s: &mut State) {
    fast_play_turn_rec(s, &mut None);
}

/// **把开着的子选择闭掉。** 两个策略共用这一份。

/// 选哪一张在推演里不重要（推演本来就是个快速估计），**闭掉才重要**：
/// `Pending != None` 时 `legal_actions` 只生成 `Choose`，`Action::EndTurn`
/// 是非法动作，而 `step` 对非法动作**原样返回**。
/// 于是"没闭掉的子选择"会让 [`rollout_outcome_with`] 的主循环空转到回合上限：
/// 每一圈都什么都不做，`turns` 照加，最后报一个「没打完」。
///
/// 2026-08-25 求解器策略刚接上时就是这么红的：P4 从 0/8320 变成 34/8320，
/// 而且**把上限从 40 抬到 300 那些数字一个字都不变** —— 一场真的打不完的仗
/// 会随上限变化，空转不会。那正是分辨这两件事的判据。
/// （`fast_play_turn` 一直有这段代码，新写的求解器策略漏了。）
///
/// # `combat_over` 那一条不是可选的
///
/// **战斗结束之后 `step` 对任何动作都原样返回**（它第一句就是
/// `if s.combat_over { return s; }`）。所以"打出最后一张牌的同时立了一个标记"
/// 这种局面下，这个循环会**一直转下去** —— 不是慢，是挂死。
///
/// 2026-08-25 实测：第 3 幕 Boss 那条 trace 的 64 个样本里有一个卡在
/// `DiscardToDrawTop { remaining: 1 }`（`n_hand=4 n_disc=7 n_draw=3`，
/// 局面完全正常，就是 `combat_over` 已经是 true 了），
/// 16 个样本 0.18 秒、64 个样本跑 4 分钟不出结果。
pub(crate) fn close_pending(s: &mut State, rec: &mut Option<Vec<Action>>) {
    // `!s.combat_over` 是**真正的那条**，理由见函数头。
    while s.pending != Pending::None && !s.combat_over {
        // **每一圈都必须真的推动局面。** 这条是兜底：`combat_over` 是已知的
        // 那个原因，而"`Choose` 打不动局面"这件事本身就该停 ——
        // 这个 while 在 `rollout_outcome_with` 的回合循环**里面**，
        // `max_turns` 拦不住它，挂死的代价太大。
        let before = *s;
        match s.pending {
            Pending::None => break,
            // 头槌那类"从弃牌堆选一张"的子选择：rollout 只要把它闭掉，
            // 选哪一张不影响这里的用途（它本来就只是个快速推演）。
            Pending::DiscardToDrawTop { .. } => {
                if s.n_disc > 0 {
                    take(s, Action::Choose { hand: 0 }, rec);
                } else {
                    s.pending = Pending::None;
                }
            }
            Pending::ExhaustFromHand { .. } => {
                if s.n_hand > 0 {
                    take(s, Action::Choose { hand: s.n_hand - 1 }, rec);
                } else {
                    s.pending = Pending::None;
                }
            }
            Pending::FetchFromDiscard { .. } => {
                if s.n_disc > 0 {
                    take(s, Action::Choose { hand: 0 }, rec);
                } else {
                    s.pending = Pending::None;
                }
            }
            Pending::PutToDrawPile { .. } => {
                if s.n_hand > 0 {
                    take(s, Action::Choose { hand: 0 }, rec);
                } else {
                    s.pending = Pending::None;
                }
            }
            Pending::UpgradeInHand { .. } => {
                if s.n_hand > 0 {
                    take(s, Action::Choose { hand: 0 }, rec);
                } else {
                    s.pending = Pending::None;
                }
            }
        }
        if *s == before {
            s.pending = Pending::None;
            break;
        }
    }

}

/// 和 [`fast_play_turn`] 是**同一段代码**，多一个可选的动作记录。
///
/// 分成两个函数、而不是抄一份，是因为验收要拿这条策略线去和穷尽搜索比分数：
/// 抄一份的话"推演里真正打出去的牌"和"验收里评的牌"就是两份实现，
/// 迟早长歪，而长歪的那天验收会安静地开始验一个不存在的策略。
/// 热路径传 `None`，只多一个空判断。
pub fn fast_play_turn_rec(s: &mut State, rec: &mut Option<Vec<Action>>) {
    if s.combat_over {
        return;
    }

    // 子选择先闭环。两个策略共用 `close_pending`，见那个函数的注释。
    close_pending(s, rec);

    // 估算本回合敌人攻击总威胁
    let mut threat_incoming = 0;
    for e in 0..s.n_enemies as usize {
        if !s.enemies[e].alive() {
            continue;
        }
        let def = enemy_def(s.enemy_def[e]);
        let mv = &def.moves[crate::step::current_move_ix(def, s, e)];
        for op in mv.ops {
            if let EOp::Attack { base, hits } = *op {
                let face = base + s.enemies[e].get(St::Strength);
                threat_incoming += face.max(0) * hits;
            }
        }
    }

    let mut guard_loop = 0;
    // `cards_locked` 是和 `legal_actions` **共用的同一个谓词** —— 见它的注释。
    while s.energy > 0 && !s.combat_over && guard_loop < 16 && !crate::step::cards_locked(s) {
        guard_loop += 1;
        let mut best_action: Option<Action> = None;

        // 1. 优先斩杀扫描
        for i in 0..s.n_hand as usize {
            let inst = s.cards[s.hand[i] as usize];
            if !playable(inst.id) || effective_cost(s, i) > s.energy {
                continue;
            }
            let def = card(inst.id);
            if matches!(def.kind, Kind::Attack) {
                for e in 0..s.n_enemies as usize {
                    if s.enemies[e].alive() {
                        let rough_dmg = 6 + s.player.get(St::Strength);
                        if s.enemies[e].hp + s.enemies[e].block <= rough_dmg {
                            best_action = Some(Action::PlayCard { hand: i as u8, target: e as u8 });
                            break;
                        }
                    }
                }
                if best_action.is_some() {
                    break;
                }
            }
        }

        // 2. 优先打出能力牌（Power）
        if best_action.is_none() {
            for i in 0..s.n_hand as usize {
                let inst = s.cards[s.hand[i] as usize];
                if !playable(inst.id) || effective_cost(s, i) > s.energy {
                    continue;
                }
                let def = card(inst.id);
                if matches!(def.kind, Kind::Power) {
                    best_action = Some(Action::PlayCard { hand: i as u8, target: 0 });
                    break;
                }
            }
        }

        // 3. 有威胁时且自身格挡不足，优先打出防御牌（Skill/Block）
        if best_action.is_none() && s.player.block < threat_incoming {
            for i in 0..s.n_hand as usize {
                let inst = s.cards[s.hand[i] as usize];
                if !playable(inst.id) || effective_cost(s, i) > s.energy {
                    continue;
                }
                let def = card(inst.id);
                if def.name.contains("防") || def.name.contains("障") || def.name.contains("耸肩") {
                    best_action = Some(Action::PlayCard { hand: i as u8, target: 0 });
                    break;
                }
            }
        }

        // 4. 打出任意可用攻击牌或技能牌
        if best_action.is_none() {
            for i in 0..s.n_hand as usize {
                let inst = s.cards[s.hand[i] as usize];
                if !playable(inst.id) || effective_cost(s, i) > s.energy {
                    continue;
                }
                let _def = card(inst.id);
                let tgt = s.first_alive().unwrap_or(0);
                best_action = Some(Action::PlayCard { hand: i as u8, target: tgt as u8 });
                break;
            }
        }

        if let Some(act) = best_action {
            let next_s = step(*s, act);
            if next_s == *s {
                break;
            }
            *s = next_s;
            if let Some(v) = rec.as_mut() {
                v.push(act);
            }
        } else {
            break;
        }
    }
}

/// 走一步并（可选地）记下来。见 [`fast_play_turn_rec`]。
fn take(s: &mut State, a: Action, rec: &mut Option<Vec<Action>>) {
    *s = step(*s, a);
    if let Some(v) = rec.as_mut() {
        v.push(a);
    }
}

// ---------------------------------------------------------------------------
// 策略：这一回合怎么打
// ---------------------------------------------------------------------------

/// 推演里那个"模拟玩家"用什么策略出牌。
///
/// # 为什么它是个参数而不是一个固定实现
///
/// `../CLAUDE.md` 一直写着「跨回合就是同一个 `solve_turn` 套在循环里换个
/// 威胁提供者」，而 `rollout.rs` 里那个 [`fast_play_turn`] **从来不是那句话
/// 说的东西** —— 设计和代码分岔了，2026-08-21 的 P3 把它照了出来：
/// 手写启发式逐回合只有 40/102 追平实战线。
///
/// 2026-08-25 把 [`Policy::Solver`] 接上了。**两个都留着**，因为它们各有各的用处：
///
/// | | 一回合的代价 | 谁在用 |
/// |---|---|---|
/// | [`Policy::Fast`] | 几十步 | `score::mcts_rollout`（它自己就长在搜索的叶子上，再套一层搜索是平方级）|
/// | [`Policy::Solver`] | 几百到几千个节点 | `bin/rollout` 的验收、以及任何"这场仗大概怎么走"的独立推演 |
///
/// **换策略会动摇 P2 的判据**：那条"推演不该比实战活得好"成立的前提是
/// 策略比人弱。用 `Solver` 跑出来的推演**可以**比实战线活得好，那不是 bug。
/// 详见 `bin/rollout` 的 P2 一节。
// **不 derive `PartialEq`**：里面有个函数指针，比地址没有意义（编译器也会警告）。
#[derive(Clone, Copy, Debug)]
pub enum Policy {
    /// 手写启发式 [`fast_play_turn`]。斩杀扫描写死 `rough_dmg = 6`、
    /// 防御牌靠 `name.contains("防")` —— 便宜，但成色只有 40/102。
    Fast,
    /// 每个回合调一次 [`crate::solver::solve_turn_potions`]，
    /// 目标函数固定 `score::survive_first`。
    ///
    /// 目标函数见 `score` 那个字段的注释 —— **它不是随便填的**。
///
/// **不许碰药水**（`allowed_potions = 0`）。两个理由：
    /// 1. 药水不要能量，只要边际收益 > 0 求解器就喝 —— 一场 30 回合的推演
    ///    会在第 1 回合把整包药喝光，推出来的血量凭空高一截；
    /// 2. `fast_play_turn` 本来也不喝，**两条策略要可比**。
    ///    药水的机会成本在搜索外面比（`advise_potions`），那是 L2 的事。
    Solver {
        /// 每回合的节点预算。**它不是 `DEFAULT_BUDGET`**：推演要跑几十个回合
        /// 乘几十个样本，200k 一回合是跑不动的。用光了就退化成启发式排名
        /// （`Solved::complete = false`），推演里读不到这个标记 ——
        /// 所以这个数选小了，`Solver` 会安静地退化成"另一个启发式"。
        budget: u32,
        /// 每个回合用哪个目标函数。**这个参数不是可选的，选错了推演会打不完仗。**
        ///
        /// [`score::survive_first`](crate::solver::score::survive_first) 里
        /// 我的 1 点血值 100、敌人的 1 点血值 30 —— 单回合看，挡下 1 点永远比
        /// 打出 1 点值钱 3.3 倍。**把这个偏好一个回合一个回合地迭代下去，
        /// 就是一个只挡不打、永远打不完的模拟玩家。** 2026-08-25 实测：
        /// 用它跑验收，`act3_f37_owl_magistrate` 那两个起点 64 条推演里有 20 条
        /// 撞上 40 回合上限，**一条都没死**（血量 p50 62）—— 活得好好的，
        /// 就是不赢。P4 从 0/8320 变成 52/8320 报的正是这件事。
        ///
        /// 这不是权重没调好，是**用单回合目标函数当跨回合策略**的结构性后果：
        /// 打掉的血要靠"少挨几回合打"兑现，而单回合视角看不见那几个回合。
        /// 所以推演策略默认用
        /// [`score::damage_first`](crate::solver::score::damage_first)。
        score: fn(&State) -> i32,
    },
    /// 限深 expectimax（`plan.rs`）。**S1 阶段：D=2，叶评估还是现成的 `eval`，
    /// 牌组画像没做。** 它比 `Solver` 强多少是个要用数据回答的问题 ——
    /// `bin/rollout --depth-sweep` 就是问这个的。
    Plan(crate::plan::Plan),
}

/// 推演默认的每回合预算。
///
/// **这个数是掂量出来的，不是实测最优**：`bin/rollout` 的 P2 每个起点采
/// 64 个样本、每个样本最多 30 个回合，2000 节点/回合的量级下一条推演约
/// 几万到几十万次 `step`，在 4.87 M steps/s 上是毫秒级。
/// 调大它先去看 `bin/rollout` 跑多久，别凭感觉。
pub const ROLLOUT_BUDGET: u32 = 2_000;

impl Default for Policy {
    fn default() -> Policy {
        Policy::Solver { budget: ROLLOUT_BUDGET, score: crate::solver::score::damage_first }
    }
}

/// 从**预测的**敌人意图建一份 [`Threat`]。
///
/// 这是「跨回合就是换个威胁提供者」那句话里的**那个提供者**：
/// 实战驱动的威胁来自观测到的意图标签，推演里没有观测，只能拿
/// `EnemyDef` 推。两者的契约是同一个 —— [`Threat`] 里存的是**最终值**。
///
/// 所以这里必须过一遍伤害管线（`damage::apply_modifiers`），
/// 而不是像 [`fast_play_turn`] 那样只做 `base + 力量`：
/// 我身上带着易伤的时候，那个粗估会**低估 50%**，
/// 而低估来袭伤害的策略会少挡、少活。
///
/// 和游戏那个意图标签一样，它是**这一回合开头冻住的一个常数**。
pub fn predicted_threat(s: &State) -> Threat {
    let mut t = Threat::new();
    for e in 0..s.n_enemies as usize {
        if !s.enemies[e].alive() {
            continue;
        }
        let def = enemy_def(s.enemy_def[e]);
        let ix = crate::step::current_move_ix(def, s, e);
        let Some(mv) = def.moves.get(ix) else { continue };
        let mut hits = 0i32;
        let mut per_hit = 0i32;
        for op in mv.ops {
            if let EOp::Attack { base, hits: h } = *op {
                // **给的是面板基础值，不在这里过乘区。**
                //
                // 以前这里先 `apply_modifiers` 再塞进 `Threat`，等于把"这一击打多少"
                // 在搜索之前就冻住了 —— 于是搜索里那些**改敌人这一击**的手段
                //（凌虐减力量 / 给它上虚弱 / 巨像减半 / 转身改朝向）全是空操作。
                // 现在交给 `Threat::set_live`，叶子在结算那一刻自己算。
                //
                // 力量也不加：`injected_enemy_turn` 现算时会取**当时**的力量，
                // 那才是对的（凌虐正是在这两个时点之间把力量削掉的）。
                per_hit = base;
                hits += h;
            }
        }
        if hits > 0 {
            t.set_live(e, per_hit, hits);
        }
    }
    t
}

/// 用 [`solve_turn_potions`](crate::solver::solve_turn_potions) 打完这一回合。
///
/// 和 [`fast_play_turn_rec`] 是**同一个位置的两个实现**，签名也一样，
/// 所以 [`play_turn_rec`] 能在两者之间切换而调用方什么都不用改。
pub fn solver_play_turn_rec(
    s: &mut State,
    budget: u32,
    score: fn(&State) -> i32,
    rec: &mut Option<Vec<Action>>,
) {
    if s.combat_over {
        return;
    }
    // **进来先闭环。** 搜索本身能处理 `Pending`（`legal_actions` 会只给
    // `Choose`），但上一回合留下来的子选择该在回合边界闭掉，
    // 和 `fast_play_turn` 一致。
    close_pending(s, rec);
    let threat = predicted_threat(s);
    let sol = crate::solver::solve_turn_potions(s, &threat, score, budget, 0);
    for &a in sol.line.acts() {
        let ns = step(*s, a);
        // **走不动就停**，别硬走完整条线。线是在这个局面上搜出来的，
        // 正常情况下每一步都合法；真出现原地不动，那是搜索和 `step` 对
        // "什么算合法"有分歧 —— 那种情况下继续照着线走只会越走越偏。
        // （`bin/rollout` 的 P3(a) 专门盯这件事。）
        if ns == *s {
            break;
        }
        *s = ns;
        if let Some(v) = rec.as_mut() {
            v.push(a);
        }
        if s.combat_over {
            break;
        }
    }
    // **出去也要闭环，这一条是硬的。** 搜出来的线可能停在一个还开着子选择的
    // 局面上（预算用光、或者那条线的最后一步正好立了标记）。留着它出去，
    // 主循环里的 `EndTurn` 就是个非法动作 —— `step` 原样返回，推演空转到上限。
    close_pending(s, rec);
}

/// 用 [`crate::plan::plan_line`] 打完这一回合。
///
/// 和 [`solver_play_turn_rec`] 逐字同构（进出都闭子选择、走不动就停），
/// 差别只在那条线是谁搜出来的。
pub fn plan_play_turn_rec(s: &mut State, cfg: &crate::plan::Plan, rec: &mut Option<Vec<Action>>) {
    if s.combat_over {
        return;
    }
    close_pending(s, rec);
    let line = crate::plan::plan_line(s, cfg);
    for &a in line.acts() {
        let ns = step(*s, a);
        if ns == *s {
            break;
        }
        *s = ns;
        if let Some(v) = rec.as_mut() {
            v.push(a);
        }
        if s.combat_over {
            break;
        }
    }
    close_pending(s, rec);
}

/// 按策略打完这一回合。**推演里所有"该出牌了"的地方都走这里。**
pub fn play_turn_rec(s: &mut State, policy: Policy, rec: &mut Option<Vec<Action>>) {
    match policy {
        Policy::Fast => fast_play_turn_rec(s, rec),
        Policy::Solver { budget, score } => solver_play_turn_rec(s, budget, score, rec),
        Policy::Plan(cfg) => plan_play_turn_rec(s, &cfg, rec),
    }
}

/// 这个局面能不能做跨回合推演。
///
/// 判据：**每一只活着的敌人都得有真出招表**。只要有一只是 `enemy::UNKNOWN`
/// （或表里没有招），推演出来的就是一场它不出手的仗，整个数字作废 ——
/// 一只认不出来就足以毁掉结论，所以这里是"全票通过"而不是"多数通过"。
pub fn can_rollout(s: &State) -> bool {
    let mut any = false;
    for e in 0..s.n_enemies as usize {
        if !s.enemies[e].alive() {
            continue;
        }
        any = true;
        let def_id = s.enemy_def[e];
        if def_id == crate::content::enemy::UNKNOWN {
            return false;
        }
        if enemy_def(def_id).moves.is_empty() {
            return false;
        }
    }
    any
}

/// `score::mcts_rollout` 实际用的回合上限。
///
/// **它不是一个安全网，它是一条会说谎的边界。** 撞上它的推演战斗没打完，
/// 而 [`rollout_single`] 照样返回当时的血量 —— 于是一场没打赢的仗被当成
/// "活着结束"记进期望里。`bin/rollout` 的 P4 专门量这件事。
///
/// # 15 → 30（2026-08-22，玩家定的）
///
/// 15 是按**第 1 幕杂兵**定的（`fast_play_turn` 打完一场的中位回合数 5-10）。
/// 那个口径在 Boss 面前立刻失效：**第 1 幕 Boss 仪式兽实战就打了 15 个回合**，
/// 于是 P4 报 `2/6656` 条截断 —— 不是内核算错，是这条边界画得太近。
/// 幕数越深战斗越长，15 只会越来越不够。
///
/// **抬高它是有代价的，代价是时间不是正确性**：`score::mcts_rollout` 每次
/// 采 30 个样本，上限翻倍 ⇒ 最坏情况下每次评估的步数翻倍。真实开销远小于
/// 翻倍，因为**绝大多数推演在 10 个回合内就分出胜负、提前返回**，
/// 只有原本被截断的那几条会真的跑满。
///
/// 30 仍然低于验收口径的 `DEFAULT_MAX_TURNS = 40`，所以 P4 还留着
/// "验收看得见、生产撞不上"的余量 —— 这个次序不能反过来，
/// 反过来 P4 就永远报不出截断了。
pub const PRODUCTION_MAX_TURNS: usize = 30;

/// 一次推演的**结局**，而不是一个分数。
///
/// 多出来的三个 bool 全是判据，其中 `truncated` 最要紧：撞上回合上限的推演
/// 战斗**没有分出胜负**，而 [`rollout_single`] 会把它的血量当成"活着结束"报出去。
/// 那是一个凭空捏造的存活数字 —— 正是本仓库最忌讳的"自信地算错"。
/// 拿它做决策之前必须知道有多少条推演是这么来的，所以这个字段不是可选的。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// 推演结束时的玩家血量（赢了的话**含**内核认得的胜利遗物回血）
    pub final_hp: i32,
    /// 走完了几个回合（`EndTurn` 的次数）
    pub turns: u32,
    pub won: bool,
    pub died: bool,
    /// 撞上 `max_turns`，**没打完**。此时 `final_hp` 不是战斗结果。
    pub truncated: bool,
    /// 结束时敌人还剩多少血（活着的那些加起来）。
    ///
    /// **它是给 `truncated` 那条用的诊断量**：截断了要能分清是
    /// 「差一口气」还是「僵住了」。2026-08-25 换策略之后 P4 第一次变红，
    /// 正是靠这个数看出来那 22 条推演停在 400 血上下 —— 不是慢，是打不动。
    pub enemy_hp_left: i32,
}

/// 从给定状态开始，模拟到整场战斗结束，返回**结局**。
///
/// `policy` 决定那个"模拟玩家"怎么出牌，见 [`Policy`]。
/// **回合边界仍然走 `step(EndTurn)`**，也就是让 `EnemyDef` 真的打这一手 ——
/// 策略里那份 [`predicted_threat`] 只用来给搜索的叶子打分，不参与结算。
/// 两者分开是有意的：机器面（P1 已验收 76/78）和策略面要能分别归因。
/// 推到战斗结束、或推到 `max_turns` 为止，**把最终局面原样还给调用方**。
///
/// [`rollout_outcome_with`] 就是它外面包一层结局统计。**分出来是给截断叶评估
/// 用的**：撞上回合上限时"还剩多少血"不是战斗结果（见 [`Outcome::truncated`]），
/// 那时候唯一诚实的做法是**拿最终局面再过一遍静态评估兜底**，
/// 而不是把一个凭空的存活血量报出去。
pub fn rollout_to_state(mut s: State, max_turns: usize, policy: Policy) -> (State, u32) {
    let mut turns = 0u32;
    while !s.combat_over && (turns as usize) < max_turns {
        play_turn_rec(&mut s, policy, &mut None);
        if s.combat_over {
            break;
        }
        s = step(s, Action::EndTurn);
        turns += 1;
    }
    (s, turns)
}

pub fn rollout_outcome_with(s: State, max_turns: usize, policy: Policy) -> Outcome {
    let (s, turns) = rollout_to_state(s, max_turns, policy);
    let died = s.player_dead || s.player.hp <= 0;
    let enemy_hp_left =
        (0..s.n_enemies as usize).filter(|&e| s.enemies[e].alive()).map(|e| s.enemies[e].hp).sum();
    Outcome {
        final_hp: s.player.hp,
        turns,
        won: s.combat_over && !died,
        died,
        truncated: !s.combat_over,
        enemy_hp_left,
    }
}

/// [`rollout_outcome_with`] 的旧签名，策略取默认值（[`Policy::Solver`]）。
pub fn rollout_outcome(s: State, max_turns: usize) -> Outcome {
    rollout_outcome_with(s, max_turns, Policy::default())
}

/// 从给定状态开始，模拟到整场战斗结束，返回最终玩家生命值（若死亡返回负分）。
///
/// **它读不出"没打完"**（撞上 `max_turns` 时照样返回当时的血量），
/// 要判这件事用 [`rollout_outcome_with`]。
pub fn rollout_single_with(s: State, max_turns: usize, policy: Policy) -> i32 {
    let o = rollout_outcome_with(s, max_turns, policy);
    if o.died {
        -1_000_000 + o.final_hp
    } else {
        o.final_hp
    }
}

/// 两条随机流各自换种子的多次采样。**验收和标定共用这一份。**
///
/// **不复用 [`rollout_combat`]**：那个函数只换 `rng.shuffle`，
/// 30 个样本共用同一条敌人随机流，等于对敌人的随机分支一次都没采样。
/// 这里两条都换，好把"分布有多宽"这件事量准。
/// # 它是并行的，而且**结果和串行逐字相同**
///
/// 每个样本的两条随机流都由 `seed` 和样本下标 `i` 完全决定，样本之间不共享
/// 任何状态（`State` 是 POD + `Copy`），所以分给哪个线程算都一样。
/// 结果按下标写回原位，**顺序也和串行一致** —— 这一条不能松：
/// 分位数是排序出来的，顺序一变复现性就没了。
///
/// 2026-08-25 加的。换上求解器策略之后这条验收从 3 秒变成好几分钟
/// （第 3 幕 Boss 那条 trace 一条就占了大头：512 血、仗长、每回合还要搜一次），
/// 慢到不会有人跑的验收等于没有验收。
pub fn sample_outcomes(
    s: &State,
    n: usize,
    seed: u64,
    max_turns: usize,
    policy: Policy,
) -> Vec<Outcome> {
    const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;
    let one = |i: usize| {
        let mut sim = *s;
        sim.rng.shuffle = seed ^ (i as u64).wrapping_mul(GOLDEN);
        sim.rng.enemy = seed ^ (i as u64 ^ 0xA5A5_A5A5).wrapping_mul(GOLDEN);
        rollout_outcome_with(sim, max_turns, policy)
    };
    let threads = std::thread::available_parallelism().map(|p| p.get()).unwrap_or(1).min(n.max(1));
    if threads <= 1 || n <= 1 {
        return (0..n).map(one).collect();
    }
    let mut out: Vec<Option<Outcome>> = vec![None; n];
    // 按 chunk 切，每个线程拿一段连续下标，写回自己那一段 —— 不用锁。
    let per = n.div_ceil(threads);
    std::thread::scope(|sc| {
        for (k, chunk) in out.chunks_mut(per).enumerate() {
            let one = &one;
            sc.spawn(move || {
                for (j, slot) in chunk.iter_mut().enumerate() {
                    *slot = Some(one(k * per + j));
                }
            });
        }
    });
    out.into_iter().map(|o| o.expect("每个下标都该被写到")).collect()
}

/// 多样本蒙特卡洛战损期望评估（MCTS Rollout）。
///
/// 使用固定的 seed 分流采样，保证比较两个候选动作时使用完全相同的随机数序列（Common Random Numbers），
/// 消除由于洗牌随机性带来的比较方差。
pub fn rollout_single(s: State, max_turns: usize) -> i32 {
    rollout_single_with(s, max_turns, Policy::default())
}

/// [`rollout_combat`] 的带策略版本。
pub fn rollout_combat_with(start: &State, samples: usize, seed: u64, policy: Policy) -> f32 {
    if start.combat_over {
        return if start.player_dead { -1_000_000.0 } else { start.player.hp as f32 };
    }

    let mut total_hp = 0.0;
    let n = samples.max(1);
    for i in 0..n {
        let mut sim = *start;
        // 为每次采样提供独立的 shuffle 随机数流
        sim.rng.shuffle = seed.wrapping_add((i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let final_hp = rollout_single_with(sim, PRODUCTION_MAX_TURNS, policy);
        total_hp += final_hp as f32;
    }
    total_hp / (n as f32)
}

/// 旧签名。**它用的是 [`Policy::Fast`]，不是默认策略** ——
/// 唯一的调用方是 `score::mcts_rollout`，而那个函数长在 `solve_turn` 的叶子上：
/// 每评估一个叶子就跑 30 条推演，每条推演每个回合再搜一次，是平方级的。
/// 想在那里也用求解器策略，得先解决"搜索里套搜索"的预算问题，
/// 那是另一件事，不在这一轮里。
pub fn rollout_combat(start: &State, samples: usize, seed: u64) -> f32 {
    rollout_combat_with(start, samples, seed, Policy::Fast)
}
