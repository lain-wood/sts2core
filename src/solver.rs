//! L2 —— 单回合出牌顺序求解器。
//!
//! **这一层不实现任何游戏规则。** 它只会做三件事：调 [`legal_actions`] 问有哪些
//! 动作、调 [`step`] 走一步、调 [`end_turn_with_incoming`] 把这回合收尾。
//! 规则只有 L1 一处，改了 L1 这里自动跟着变；反过来，这里算出来的东西只在
//! L1 已经对拍验证过的范围内可信。
//!
//! ## 三条设计决定（`sts2core/CLAUDE.md` 的「L2 的接口：先定死」）
//!
//! 1. **[`Threat`]（敌人这回合干什么）是输入，不是求解器算出来的。**
//!    今天从观测到的意图标签来 —— 标签已实测是**最终伤害值**，攻击方力量和
//!    防御方乘区都算在里面了（`docs/trace-format.md` 约束 4）。以后跨回合
//!    rollout 换成 `EnemyDef` 预测，**换提供者不用改求解器**。
//! 2. **`score` 是参数。** 单回合的目标函数（"这回合净收益"）和跨回合的
//!    （"存活率优先"）不是一回事，做成参数就消掉了将来重写的风险。
//! 3. **单回合求解器是跨回合 rollout 的内层循环**，不是一次性的东西。
//!
//! ## 为什么值得搜，而不是照贪心打
//!
//! 出牌顺序真的会改变结果，而且这些机制**全部已经和真实游戏对拍验证过**：
//!
//! | 机制 | 顺序为什么要紧 |
//! |---|---|
//! | 踩踏 `cost_minus_attacks` | 先打攻击牌，它才便宜 |
//! | 拆卸 `DamageIfVuln` | 先上易伤，它才打两下 |
//! | 灰烬打击 `Scale::PerExhaust` | 先消耗，它才涨伤害 |
//! | 乘区累乘取整 | 易伤/缓慢/虚弱同时在场时，先后顺序改变取整的位置 |
//! | 缓慢按牌累加 | 打得越靠后的攻击吃到的加成越高 |
//! | 杀死敌人 | 死人不出手 —— 这回合的威胁直接少一份 |
//!
//! ## 打分发生在**挨完打之后**
//!
//! 叶子节点不是"这回合打完的样子"，而是 [`end_turn_with_incoming`] 之后、
//! 下回合开始时的样子。这不是实现上的偶然，是刻意的：
//!
//! * **格挡按它真正挡下的伤害计价，不按面板值。** 12 点格挡挡 8 点攻击只值
//!   8 点；一点不挨打的回合里它值 0。任何"格挡 = 好"的启发式都会在这里犯错。
//! * 回合结束/回合开始的结算（覆甲、绯红披风、灼伤、滚石、状态衰减）
//!   全都算进去了，不用在这一层重写一遍。
//!
//! ## 已知的两处不确定，别当成解
//!
//! * **抽牌是猜的。** 抽牌堆顺序在观测里已经丢了（约束 2），线里一旦有抽牌，
//!   后面就是内核 RNG 的一个样本。[`Line::drew`] 把这件事标出来，不藏。
//! * **预算用光就退化成启发式排名。** [`Solved::complete`] 为 false 时，
//!   这个结果和旧 Python 模拟器满表的 `approx=yes` 是同一个成色 —— 看这个
//!   字段，别看分数。

use std::collections::HashSet;

use crate::content::card;
use crate::state::{State, MAX_ENEMIES};
use crate::step::{legal_actions, step, Action, NO_INCOMING};

/// 一条线最多几个动作。一回合能打出的牌被能量卡着，实战里 6-8 张已经是上限，
/// 24 留足了余量（烙印那种子选择也占格子）。
pub const MAX_LINE: usize = 24;

/// 全部药水槽都允许动用。见 [`solve_turn_potions`]。
///
/// **2026-08-25 修**：这里原来写死 `0b111`（3 位），而 `MAX_POTIONS` 早就从 3
/// 改成了 10。后果是**槽 3 以后的药水永远进不了搜索** —— `--live` 会把它们
/// 列在药水定价那张表里（`advise_potions` 按 `potion_slots` 扫），
/// 但主线搜索一次都不会考虑喝它们。玩家有 5 个槽，也就是**后两格形同虚设**，
/// 而且不报错、只是安静地少一个选项。宽度跟着 `MAX_POTIONS` 走。
pub const ALL_POTIONS: u16 = (1u16 << crate::state::MAX_POTIONS) - 1;

/// 默认节点预算。一个节点约等于一次 [`step`]（实测 ~135 ns），所以 200k
/// 大约是 30 ms —— 实战驱动里一回合等 30 ms 完全可以接受，而这个量级足够把
/// 绝大多数真实手牌**穷尽**搜完（见 [`Solved::complete`]）。
pub const DEFAULT_BUDGET: u32 = 200_000;

/// 这回合敌人会做什么。求解器只消费它，不关心它从哪来。
///
/// | 提供者 | 什么时候用 |
/// |---|---|
/// | 观测到的意图标签 | 实战驱动。标签已实测是最终伤害值，不用再过乘区 |
/// | `EnemyDef` 预测 | 跨回合 rollout。基线见 `verify --predict-enemy`（95%）|
///
/// 下标是**内核的敌人槽位**，和 `State::enemies` 一一对应（实战驱动时由
/// `replay::Replayer` 的 `combat_id -> slot` 表给出 —— `entity_id` 会重新编号，
/// 不能拿它当键，见 `docs/trace-format.md` 约束 1）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Threat {
    /// 每个敌人的 `(每次伤害, 次数)`。
    ///
    /// **两种口径，由 `live` 区分**：
    /// * `live == false`（默认）：**最终值**，就是观测到的意图标签。
    ///   叶子照打，不过任何乘区 —— 这是对拍和"敌人认不出来"时唯一站得住的口径。
    /// * `live == true`：**面板基础值**（不含力量、不过乘区）。叶子在结算那一刻
    ///   现算一遍，于是"我这一手改了敌人打多少"才看得见。
    pub incoming: [(i32, i32); MAX_ENEMIES],
    /// `incoming` 里装的是不是基础值。见上面两种口径。
    ///
    /// **它不是精度开关，是能不能看见一类决策的开关**：凌虐减 10 力量、
    /// 给敌人上虚弱、巨像减半、转身改朝向、我自己吃污染 —— 这几样在
    /// `live == false` 下**全部是空操作**（2026-08-27 第 2 幕 Boss 实战撞到的）。
    pub live: bool,
}

impl Threat {
    /// 什么都不打进来。用它打出来的线是"不考虑挨打的最优攻击线"，
    /// 只在确知敌人这回合不出手时才对。
    pub const NONE: Threat = Threat { incoming: NO_INCOMING, live: false };

    pub fn new() -> Threat {
        Threat::NONE
    }

    /// 给某个槽位设一手攻击。`per_hit` 是**最终值**（意图标签上的数字）。
    pub fn set(&mut self, slot: usize, per_hit: i32, hits: i32) -> &mut Threat {
        if slot < MAX_ENEMIES {
            self.incoming[slot] = (per_hit, hits);
        }
        self
    }

    /// 给某个槽位设一手攻击，`base_per_hit` 是**面板基础值**（不含力量、不过乘区），
    /// 并把整份威胁切到"现算"口径。
    ///
    /// **整份威胁只能有一种口径**：混着装会让一半敌人现算、一半照打，
    /// 而调用方无从分辨自己拿到的是哪种。要么全知道基础值，要么整份退回标签。
    pub fn set_live(&mut self, slot: usize, base_per_hit: i32, hits: i32) -> &mut Threat {
        if slot < MAX_ENEMIES {
            self.incoming[slot] = (base_per_hit, hits);
        }
        self.live = true;
        self
    }

    /// 叶子收尾：按当前口径把这一回合结掉。
    ///
    /// 所有算叶子的地方都必须走它 —— 直接调 `end_turn_with_incoming` 会把
    /// "现算"那一档静默降级回冻住的标签，而那种错**不会报错，只会让求解器
    /// 又看不见减伤**。
    #[inline]
    pub fn end_turn(&self, s: State) -> State {
        if self.live {
            crate::step::end_turn_with_live_incoming(s, &self.incoming)
        } else {
            crate::step::end_turn_with_incoming(s, &self.incoming)
        }
    }

    /// 面板上的总来袭伤害。**只是个上界**：谁被打死了、格挡吃掉多少，
    /// 都要等真跑一遍才知道 —— 这个数只配拿来给人看。
    pub fn face_total(&self) -> i32 {
        self.incoming.iter().map(|(d, h)| d * h).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.incoming.iter().all(|(d, h)| *d == 0 || *h == 0)
    }
}

impl Default for Threat {
    fn default() -> Threat {
        Threat::NONE
    }
}

/// 一条出牌线：从给定局面开始，按顺序执行的动作。
///
/// 动作里的 `hand` 是**执行到那一步时**的手牌下标，会随着前面的牌离手而漂移，
/// 所以只能从同一个起点按顺序重放。[`explain`] 负责把它翻译成人能读的牌名。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Line {
    acts: [Action; MAX_LINE],
    pub n: u8,
    /// 这条线走完、挨完打之后的分数。和 [`Solved::baseline`] 比才有意义。
    pub score: i32,
    /// 线里抽过牌 —— 抽到什么是内核 RNG 猜的，真实牌序不可知（约束 2）。
    /// **这一条的分数是一个样本，不是期望。**
    pub drew: bool,
}

impl Line {
    pub const EMPTY: Line =
        Line { acts: [Action::EndTurn; MAX_LINE], n: 0, score: i32::MIN, drew: false };

    pub fn acts(&self) -> &[Action] {
        &self.acts[..self.n as usize]
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    fn push(&mut self, a: Action) {
        if (self.n as usize) < MAX_LINE {
            self.acts[self.n as usize] = a;
            self.n += 1;
        }
    }
}

/// 求解结果 + **诊断**。诊断不是可选的：不知道搜索有没有搜完，分数就没法读。
#[derive(Clone, Copy, Debug)]
pub struct Solved {
    pub line: Line,
    /// 展开过的节点数（≈ [`step`] 调用次数）
    pub nodes: u32,
    /// 搜索是否**穷尽**。false = 预算用光，返回的是当时找到的最好的那条 ——
    /// 一个启发式排名，不是解。
    pub complete: bool,
    /// 什么都不打、直接结束回合的分数。**这是 Δ=0 的基线**，
    /// `line.score - baseline` 才是"这条线值多少"。
    pub baseline: i32,
}

impl Solved {
    /// 相对"什么都不做"的净收益。这个仓库一贯的读法：比较，不看绝对值。
    pub fn gain(&self) -> i32 {
        self.line.score - self.baseline
    }
}

// ---------------------------------------------------------------------------
// 目标函数
// ---------------------------------------------------------------------------

/// 目标函数的权重。
///
/// **这些数字是判断，不是实测。** 本仓库的规矩是欠定就留空，但目标函数没有
/// "留空"这个选项 —— 排序总得有个依据。折中办法是把每个数字的理由写在这里，
/// 并且让它们**可替换**（`score` 是参数），而不是散落在代码里假装是事实。
///
/// 单位随便取，只有比值有意义。基准取"我的 1 点 HP = 100"。
pub struct Weights {
    /// 我的 1 点 HP
    pub hp: i32,
    /// 敌人剩余的 1 点 HP。比我的血便宜 —— 打死敌人的价值是通过"少挨几回合打"
    /// 间接兑现的，而单回合求解器看不到那几个回合。
    ///
    /// 取 1:3（30 : 100）是有意偏防守的：玩家指出过我**防御端偏弱、过于偏进攻**，
    /// 而上一局正是死在一套几乎没有防御牌的构筑上。想打竞速把它调高。
    pub enemy_hp: i32,
    /// 打赢这场仗
    pub win: i32,
    /// 死了。压倒一切 —— 「死亡率优先，ΔHP 其次」是这个仓库既定的读法。
    pub death: i32,
    /// 敌人身上每层易伤（只数活着的敌人，最多数 2 层：它每回合掉 1 层，
    /// 堆得再多下回合也用不上）
    pub enemy_vuln: i32,
    /// 敌人身上每层虚弱（同样最多数 2 层）
    pub enemy_weak: i32,
    /// 我身上每点力量。它跨回合留着，所以比一次性的伤害值钱。
    pub strength: i32,
    /// **能力状态跨回合价值的折扣**，按百分比（100 = 原值，0 = 完全不计）。
    ///
    /// [`power_horizon_value`] 已经把能力的产出换算进了这套币值，理论上系数
    /// 就该是 100。留这个折扣是因为那个换算里含着两处乐观：
    /// **仗真的能打满 H 个回合**、以及**这些产出真的都用得上**。
    /// 这个数**要标定，不许拍**（`bin/calib --siblings`，和 `enemy_hp = 75`
    /// 同一条流程）。
    ///
    /// **只有 [`Weights::LEAF`] 该非零。** 线的目标函数（`survive_first` /
    /// `damage_first`）是给**单回合**求解器排序用的，给它加跨回合项会直接
    /// 改掉驾驶路径，还会污染 `bin/solve` 那条"实战线赢不过穷尽搜索"的自检。
    pub power: i32,
}

/// 一层易伤/虚弱最多按几层算 —— 它们每回合掉一层，堆过头是浪费。
const DEBUFF_COUNT_CAP: i32 = 2;

impl Weights {
    /// 存活优先。**默认目标函数。**
    pub const SURVIVE_FIRST: Weights = Weights {
        hp: 100,
        enemy_hp: 30,
        win: 100_000,
        death: -1_000_000,
        // 一层易伤 ≈ 下回合多打 50% 伤害。按一回合打 12 点估，多打 6 点，
        // 折成敌人 HP 就是 6×30 = 180 —— 但只有在我下回合真的打它时才兑现，
        // 打个对折。
        enemy_vuln: 90,
        // 一层虚弱 ≈ 它少打 25%。按一手 12 点估，少挨 3 点 = 300，同样打对折。
        enemy_weak: 150,
        strength: 60,
        // 单回合目标函数**不看跨回合的账**，见 `Weights::power`。
        power: 0,
    };

    /// 竞速：敌人的血更值钱。**只在"这一幕必须抢在被磨死之前打完"时用**，
    /// 平时用它就是在复制上一局那套玻璃大炮。
    pub const DAMAGE_FIRST: Weights =
        Weights { hp: 40, enemy_hp: 60, ..Weights::SURVIVE_FIRST };

    /// **跨回合搜索的叶评估。** 和 `SURVIVE_FIRST` 只差一处：敌人的血更值钱。
    ///
    /// 这个 75 不是拍的，是 2026-08-25 标定出来的（`bin/calib --siblings`）：
    /// 拿 103 组兄弟叶子（同一个根下的 K 条候选线各走完之后的局面）和它们
    /// **真的推到底**的结局对照，在 `hp / block / enemy_hp / threat` 上搜权重，
    /// 留一条 trace 交叉验证，30 折里 **27 折**选中 `enemy_hp/hp = 0.75`。
    ///
    /// 两条同时量出来的结论：
    /// * **下一回合的来袭伤害权重是 0** —— 兄弟们面对的是同一只敌人同一手意图，
    ///   这个量组内几乎是常数，对排序毫无贡献。所以这里不加它。
    /// * 玩家格挡也几乎不值钱（叶子在回合开始，格挡马上要被清掉），
    ///   `eval` 本来就不数它，正好。
    ///
    /// 效果：留出集上每次决策的"后悔"从 `SURVIVE_FIRST` 的 1.61 血降到 1.51 血
    /// （随便挑是 4 血上下，oracle 是 0，标签噪声地板 0.07）。
    /// **`power: 100` 是 2026-08-31 加的，取的是公式的理论值，不是标出来的 115。**
    ///
    /// [`power_horizon_value`] 本来就把能力的产出换算进了这套币值，
    /// 所以"不打折"就是 100。标定（`tools/calib_power.py`，18 组能说话的兄弟）
    /// 选中 115，留出后悔 2.825 vs 关掉的 3.229 —— **方向对，但样本撑不住
    /// 那个精度**：18 组、留一条 trace，训练集之间重叠 17/18，
    /// "14/15 折一致"几乎是自动的，不是 P6 当年 78 组 30 折那种独立确认。
    /// 所以取整到理论值，那 15 不假装是标出来的。
    /// 真正的判据是端到端的 P5 配对比较（14144 对），读数见 CLAUDE.md。
    pub const LEAF: Weights =
        Weights { enemy_hp: 75, power: 100, ..Weights::SURVIVE_FIRST };

    /// 纯 HP 差。什么都不加权，用来做对照 —— 当两个目标函数给出同一条线时，
    /// 这条线的可信度比任何权重讨论都高。
    pub const HP_ONLY: Weights = Weights {
        hp: 100,
        enemy_hp: 0,
        win: 100_000,
        death: -1_000_000,
        enemy_vuln: 0,
        enemy_weak: 0,
        strength: 0,
        power: 0,
    };
}

/// 「下一回合最多能打出多少伤害」的**乐观界**。
///
/// 叶子在未抽牌，所以这个界是对**抽牌**取的最优：假设正好抽到牌堆里
/// 伤害最高的 5 张、能量管够、敌人没有格挡。
///
/// 三处刻意的乐观（都只影响触发频率，不影响正确性）：
/// * 不看费用（`满能量 + 手牌全打`）
/// * 不看敌人格挡和减伤
/// * 只数 `Op::Damage` / `Op::DamageAll`，不数生成牌、连打那类能超出 5 张的
///
/// 反过来说它**不是严格上界**（最后一条会让它偏小），所以它只配当触发条件，
/// 不能拿来剪枝。
pub(crate) fn optimistic_damage(s: &State) -> i32 {
    let str_bonus = s.player.get(crate::state::St::Strength);
    // 牌堆里够得着的牌：抽牌堆 + 弃牌堆（抽空了会洗回来）
    let mut vals: Vec<i32> = Vec::new();
    let mut push = |ix: u8| {
        let inst = s.cards[ix as usize];
        let ops = crate::content::card_ops(inst.id, inst.upgraded());
        let mut d = 0;
        for op in ops {
            match *op {
                crate::ops::Op::Damage { base, hits, .. } => {
                    d += (base + inst.bonus as i32 + str_bonus).max(0) * hits
                }
                crate::ops::Op::DamageAll { base, hits, .. } => {
                    d += (base + inst.bonus as i32 + str_bonus).max(0) * hits
                }
                _ => {}
            }
        }
        if d > 0 {
            vals.push(d);
        }
    };
    for i in 0..s.n_draw as usize {
        push(s.draw[i]);
    }
    for i in 0..s.n_disc as usize {
        push(s.disc[i]);
    }
    for i in 0..s.n_hand as usize {
        push(s.hand[i]);
    }
    vals.sort_unstable_by(|a, b| b.cmp(a));
    vals.iter().take(5).sum()
}

/// 场上还活着的敌人的总血量（含格挡）。
pub(crate) fn enemy_wall(s: &State) -> i32 {
    (0..s.n_enemies as usize)
        .filter(|&e| s.enemies[e].alive())
        .map(|e| s.enemies[e].hp.max(0) + s.enemies[e].block)
        .sum()
}

/// 场上还活着的敌人的总血量（含还没出场的形态，含格挡）。
pub(crate) fn enemy_wall_all_forms(s: &State) -> i32 {
    (0..s.n_enemies as usize)
        .filter(|&e| s.enemies[e].alive() || s.enemies[e].get(crate::state::St::Adaptable) > 0)
        .map(|e| crate::content::remaining_hp_including_revives(&s.enemies[e]) + s.enemies[e].block)
        .sum()
}

/// 铁甲战士的基础能量。
///
/// > **不要拿 `base_energy - BASE_ENERGY` 反推"这是薪火之源给的"。**
/// > `Replayer::sync` 是 `s.base_energy = obs.max_energy`，**直接从观测灌** ——
/// > 身上任何一件内核没建模的、给最大能量的东西（遗物 / 事件 / 附魔）
/// > 都会混进那个差值里。归因要用 [`pyre_energy`]，理由写在那里。
pub const BASE_ENERGY: i32 = 3;

/// **内核能归因到薪火之源的那部分额外最大能量。**
///
/// # 为什么不能用 `base_energy - BASE_ENERGY`
///
/// 那个差值是**观测量**（`sync` 从 `obs.max_energy` 直接灌），不是内核自己的
/// 记账。身上有一件内核没建模的 +1 最大能量遗物时，它会被当成"打过一张
/// 薪火之源"记进 [`power_horizon_value`] 的第一项。
///
/// **而那一项乘着 [`horizon`]，敌人血越少 `h` 越小** —— 于是这笔凭空的加分
/// 会随着我打死敌人而缩水，净效果是**奖励留着敌人不杀**。
/// 这和 [`eval`] 那个"拿当前敌人血量当进度、于是宁可站着挨打也不收人头"
/// 是**同一个失效模式**，只是来源不同。
///
/// 实测（2026-09-01）：一件假想的 +1 能量遗物，能让"打掉 80 点敌人血"
/// 在叶分数里**贬值 18 点血**（h 从 7 掉到 3）。
///
/// # 为什么 `St::Pyre` 的层数就是答案
///
/// 薪火之源同一行表里挂印记、给能量，两个数一样
/// （`Op::Status{Pyre, n}` + `Op::GainMaxEnergy(n)`），
/// `pyre_marker_tracks_the_energy_it_granted` 钉着这条。
/// 观测侧也成立：语料里 200 帧带 `PYRE_POWER` 的观测**无一例外**满足
/// `max_energy - 3 == PYRE_POWER`（4/1 共 60 帧，5/2 共 140 帧）。
///
/// 再和 `base_energy` 的实际盈余取 `min`：两边**都**认账才计价。
/// 方向是保守的（宁可少算），和这个模块其余各处一致。
#[inline]
pub fn pyre_energy(s: &State) -> i32 {
    s.player.get(crate::state::St::Pyre).min(s.base_energy - BASE_ENERGY).max(0)
}

/// 地平线的上限。**它是个闸，不是一个模型** —— 真实仗长在 2~10 个回合，
/// 不封顶的话残血硬怪那种 `enemy_wall / dpt` 会算出几十回合，
/// 把能力项放大到压倒一切。
pub const HORIZON_CAP: i32 = 8;

/// **预计这场仗还要打几个我的回合。**
///
/// `敌人血墙 / 我一回合够得着的伤害`，夹在 `[1, HORIZON_CAP]`。
/// 两个量都是内核里现成的、已经各自有理由的东西，一个新常数都没发明。
///
/// **方向是保守的**：`optimistic_damage` 按"正好抽到最狠的 5 张、能量管够、
/// 敌人没格挡"取，偏大 ⇒ 算出来的回合数偏小 ⇒ 能力项被低估。
/// 这正是想要的那一边 —— 高估能力牌比低估更危险（会去打一张这场仗根本
/// 兑现不了的能力牌）。
pub fn horizon(s: &State) -> i32 {
    Horizon::of(enemy_wall_all_forms(s), optimistic_damage(s)).turns()
}

/// 地平线的**精确有理表示**：`num / den` 个回合，恒在 `[1, HORIZON_CAP]`。
#[derive(Clone, Copy, Debug)]
pub(crate) struct Horizon {
    num: i64,
    den: i64,
}

impl Horizon {
    pub(crate) fn of(wall: i32, dpt: i32) -> Horizon {
        let w = wall.max(0) as i64;
        let d = dpt.max(1) as i64;
        if w <= d {
            return Horizon { num: 1, den: 1 };
        }
        if w >= HORIZON_CAP as i64 * d {
            return Horizon { num: HORIZON_CAP as i64, den: 1 };
        }
        Horizon { num: w, den: d }
    }

    fn cap_at(self, num: i64, den: i64) -> Horizon {
        debug_assert!(den > 0);
        let (num, den) = if num < 0 { (0, 1) } else { (num, den) };
        if num * self.den < self.num * den {
            if num <= den {
                Horizon { num: 1, den: 1 }
            } else {
                Horizon { num, den }
            }
        } else {
            self
        }
    }

    pub(crate) fn turns(self) -> i32 {
        ((self.num + self.den - 1) / self.den) as i32
    }
}

/// **这副牌一点能量能换几点伤害。**
///
/// 数的是抽牌堆 + 弃牌堆 + 手牌里**带伤害的牌**：面板伤害总和 ÷ 费用总和。
/// 和 [`optimistic_damage`] 是两个东西 —— 那个是"下回合最多打多少"的乐观界
/// （取最狠的 5 张、不看费用），这个是"一点能量的兑换率"。
///
/// 三处近似，方向都写在这里：
/// * **只数 `Op::Damage` / `Op::DamageAll` 的面板值**，条件牌/X 费/生成类按 0 ——
///   和 `calib::face_of` 同一个口径，偏**保守**。
/// * 0 费的伤害牌不进分母（费用为 0），所以它们会把兑换率**抬高**。
/// * 不含力量加成 —— 和 `optimistic_damage` 不同，这里要的是牌组的稳定属性，
///   而力量是局面量，混进来会让同一副牌在不同回合给出不同兑换率。
///
/// 兜底 1：一副一张伤害牌都没有的牌组（纯格挡流）不该让能量项爆成 0 或除零。
fn damage_per_energy(s: &State) -> i32 {
    let mut dmg = 0i32;
    let mut cost = 0i32;
    let mut push = |ix: u8| {
        let inst = s.cards[ix as usize];
        let d = card(inst.id);
        let mut face = 0;
        for op in crate::content::card_ops(inst.id, inst.upgraded()) {
            match *op {
                crate::ops::Op::Damage { base, hits, .. }
                | crate::ops::Op::DamageAll { base, hits, .. } => {
                    face += (base + inst.bonus as i32).max(0) * hits
                }
                _ => {}
            }
        }
        if face > 0 {
            dmg += face;
            cost += (if inst.upgraded() { d.cost_upg } else { d.cost }).max(0);
        }
    };
    for i in 0..s.n_draw as usize {
        push(s.draw[i]);
    }
    for i in 0..s.n_disc as usize {
        push(s.disc[i]);
    }
    for i in 0..s.n_hand as usize {
        push(s.hand[i]);
    }
    if dmg == 0 {
        return 1;
    }
    (dmg / cost.max(1)).max(1)
}

/// **能力状态的跨回合价值**，换算进 [`Weights`] 已有的币值。返回的是**未打折**
/// 的原值，折扣由 `Weights::power` 施加（见 [`eval`]）。
pub fn power_horizon_value(s: &State, w: &Weights) -> i32 {
    use crate::state::St;

    let extra_energy = pyre_energy(s);
    let ramp = s.player.get(St::DemonForm);
    let boulder = s.player.get(St::RollingBoulder);
    if extra_energy <= 0 && ramp == 0 && boulder == 0 {
        return 0;
    }

    let dpt = optimistic_damage(s);
    let h = Horizon::of(enemy_wall_all_forms(s), dpt);
    let n_active = (extra_energy > 0) as i64 + (ramp > 0) as i64 + (boulder > 0) as i64;
    let safe_hp = (w.enemy_hp as i64 * 99) / 100;
    let mut v = 0;

    // 1. 额外能量
    if extra_energy > 0 {
        let rate = (extra_energy as i64 * damage_per_energy(s) as i64)
            .min((dpt.max(1) as i64 * 99 / 100) / n_active);
        let total = rate * h.num * safe_hp / h.den;
        let share = enemy_hp_left(s) as i64 * safe_hp / n_active;
        v += total.min(share) as i32;
    }

    // 2. 力量爬坡
    if ramp > 0 {
        let h2 = if w.strength > 0 {
            h.cap_at(
                2 * dpt.max(1) as i64 * safe_hp
                    - n_active * ramp as i64 * w.strength as i64,
                2 * n_active * ramp as i64 * w.strength as i64,
            )
        } else {
            h
        };
        v += (ramp as i64 * h2.num * (h2.num + h2.den) * w.strength as i64
            / (2 * h2.den * h2.den)) as i32;
    }

    // 3. 滚石
    if boulder > 0 {
        for e in 0..s.n_enemies as usize {
            let en = &s.enemies[e];
            if !en.alive() {
                continue;
            }
            let left = crate::content::remaining_hp_including_revives(en);
            let he = Horizon::of(left + en.block, dpt)
                .cap_at(2 * dpt as i64 * 99 / 100 - n_active * (2 * boulder as i64 - 5), 10 * n_active);
            let val = (2 * boulder as i64 * he.num * he.den + 5 * he.num * (he.num - he.den))
                * safe_hp
                / (2 * he.den * he.den);
            let share = left as i64 * safe_hp / n_active;
            v += val.min(share) as i32;
        }
    }

    v
}
/// 场上活着的敌人还剩多少血，**含还没出场的形态**。
///
/// 和 `eval` 里那一项同一个口径（`remaining_hp_including_revives`）——
/// 用当前血量的话，会复活的敌人身上会出现"砍到最后几点反而更差"的荒唐激励。
/// 这里只当并列判据的第二关键字，不进分数。
///
/// > **口径契约：入参必须是「敌人这一手已经打完」的局面**（`Threat::end_turn`
/// > 之后），不是「我这条线刚打完」的局面。两者在有回合边界伤害的场上**真的
/// > 不一样** —— 荆棘 5 差 5 点、滚石 10 差 10 点（2026-09-01 实测）。
/// >
/// > 跨回合 planner 曾经在这里用错口径：它拿的是敌人还没动的局面，
/// > 而注释却写着"和 `consider_stopping_here` 一个口径"。
/// > **`plan::tiebreak_left` 现在是唯一的入口**，别在别处自己拼一遍。
pub(crate) fn enemy_hp_left(s: &State) -> i32 {
    (0..s.n_enemies as usize)
        .filter(|&e| s.enemies[e].alive())
        .map(|e| crate::content::remaining_hp_including_revives(&s.enemies[e]))
        .sum()
}

/// 给一个**回合已经结束、敌人已经打完**的局面打分。
pub fn eval(s: &State, w: &Weights) -> i32 {
    use crate::state::St;

    let mut v = 0;
    if s.player_dead {
        // 死了就是死了，但仍然给一点排序依据：注定要死的局面里，
        // "死之前多打掉一点"总比"死之前什么都没做"强一点点。
        // 差距远小于 `death` 和活着之间的鸿沟，不会反过来诱导送死。
        v += w.death;
    }
    v += s.player.hp * w.hp;
    if s.combat_over && !s.player_dead {
        // 赢了。场上可能还剩着爪牙（主人一死它们跟着消失），所以不数敌人血量。
        return v + w.win;
    }
    for e in 0..s.n_enemies as usize {
        let en = &s.enemies[e];
        if !en.alive() {
            continue;
        }
        // **HP 要夹到 0**：`absorb` 允许血量变成负数，不夹的话超杀会被算成收益，
        // 求解器会去挑"打得最过头"的那条线。
        //
        // **数的是"还剩多少血才算真打完"，含还没出场的形态**（见
        // `content::remaining_hp_including_revives`）。用当前血量的话，
        // 会复活的敌人身上会出现一个荒唐的激励：砍掉最后 4 点让这一项
        // 从 4 跳到 200，于是求解器宁可站着挨打也不收人头。
        // 2026-08-30 第 3 幕 Boss 实测过这一幕。
        v -= crate::content::remaining_hp_including_revives(en) * w.enemy_hp;
        v += en.get(St::Vulnerable).min(DEBUFF_COUNT_CAP) * w.enemy_vuln;
        v += en.get(St::Weak).min(DEBUFF_COUNT_CAP) * w.enemy_weak;
    }
    v += s.player.get(St::Strength) * w.strength;
    // 能力状态的跨回合价值。`w.power == 0` 时 `power_horizon_value` 连算都不算
    // （它自己有早退），所以单回合那三个目标函数一分钱不多花。
    if w.power != 0 {
        v += power_horizon_value(s, w) * w.power / 100;
    }
    v
}

/// 现成的目标函数。签名是定死的 `fn(&State) -> i32`（不是闭包），
/// 所以权重只能在这里绑死 —— 要新权重就在这里再加一个 `pub fn`。
pub mod score {
    use super::{eval, Weights};
    use crate::state::State;

    /// 存活优先，其次看 HP。**默认。**
    pub fn survive_first(s: &State) -> i32 {
        eval(s, &Weights::SURVIVE_FIRST)
    }

    /// 竞速。见 [`Weights::DAMAGE_FIRST`] 的警告。
    pub fn damage_first(s: &State) -> i32 {
        eval(s, &Weights::DAMAGE_FIRST)
    }

    /// 纯 HP 差，做对照用。
    pub fn hp_only(s: &State) -> i32 {
        eval(s, &Weights::HP_ONLY)
    }

    /// 跨回合搜索的**叶评估**。见 [`Weights::LEAF`] —— 它的权重是标定出来的。
    pub fn leaf(s: &State) -> i32 {
        eval(s, &Weights::LEAF)
    }

    /// MCTS 整场战损期望评估：快速模拟至战斗结束，以最终期望生命值为目标。
    ///
    /// **用之前必须先过 `rollout::can_rollout`。** 局面里的敌人认不出来时
    /// （`replay::sync` 出来的局面默认全是 `enemy::UNKNOWN`），推演的是一场
    /// 敌人永远不出手的仗，这个函数会返回一个**看着很正常、实际毫无意义**的数字。
    ///
    /// 这里不做守卫的原因是签名定死了 `fn(&State) -> i32`，返回不了"拒绝"。
    /// 所以守卫在调用方：`bin/solve.rs` 用 `--score mcts` 时先查 `can_rollout`，
    /// 认不出敌人就拒绝作答而不是降级。
    pub fn mcts_rollout(s: &State) -> i32 {
        let exp_hp = crate::rollout::rollout_combat(s, 30, 0x9E37_79B9_7F4A_7C15);
        (exp_hp * 100.0) as i32
    }
}

// ---------------------------------------------------------------------------
// 搜索
// ---------------------------------------------------------------------------

/// 局面指纹，用来把**换序到达同一个局面**的分支合并掉。
///
/// 出牌顺序的排列数是阶乘级的，但真正不同的局面少得多（打击→防御 和
/// 防御→打击 到达的是同一个局面）。没有这一步，6 张手牌就能把预算烧光。
///
/// 哪些字段进指纹，是按"它会不会改变**未来**"来定的：
/// * 手牌按**多重集**（排序后）算 —— 手牌顺序不影响任何规则（`legal_actions`
///   里那条去重注释已经论证过这一点，加"弃最左边一张"这类牌时两处都要撤）
/// * 抽牌堆**按顺序**算 —— 它决定后面抽到什么
/// * 弃牌堆按**多重集**算 —— 洗牌是随机的，顺序不同的两个弃牌堆洗出来的结果
///   在期望上没有区别，而真实牌序本来就不可知
/// * 消耗堆只算**张数** —— 只有灰烬打击读它，读的是张数
/// 一个 64 位的雪崩混合（splitmix64 的 finalizer）。
///
/// 它就是**按需生成的 Zobrist 表**：Zobrist 要的是"每个 (位置, 取值) 一个
/// 互相独立的随机数"，而一个好的整数哈希从任意 key 现算一个，效果一样，
/// 还不用背一张表、不用担心表的初始化顺序影响复现性。
#[inline(always)]
fn z(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// 一张牌实例的完整身份：id / 升级位 / 腐化位 / 暴走加值 / 本场改过的费用。
///
/// **`cost_delta` 也在里面**：狂乱逃离打一次自己 +1 费，两张同名不同费的牌
/// 是两张不同的牌，混为一谈会让搜索把它们合并掉。
#[inline(always)]
fn card_ident(s: &State, ix: u8) -> u64 {
    let c = s.cards[ix as usize];
    // 高 8 位放附魔的格挡加值：不带它的话，一张带灵巧的防御和一张普通防御
    // 会被搜索当成同一张牌合并掉。
    (c.block_bonus as u8 as u64) << 56
        | (c.id as u64) << 40
        | (c.flags as u64) << 24
        | (c.bonus as u16 as u64) << 8
        | (c.cost_delta as u8 as u64)
}

/// 局面指纹（Zobrist 风格）。
///
/// **手牌和弃牌堆按多重集**（可交换地累加，顺序无关），
/// **抽牌堆按顺序**（混进下标）—— 回合内抽牌是按顺序发的，顺序换了就是
/// 另一个局面。这个区分从 FNV 那一版就是对的，这里只换了组合方式。
///
/// # 为什么从"排序 + FNV"换成"哈希后相加"（2026-08-25）
///
/// 老版本给手牌和弃牌堆各开一个 `[i64; MAX_CARDS]`（各 1 KB）再排序。
/// 实测 **137 ns/次**，而一个搜索节点 ≈ 一次 `step` ≈ 209 ns —— 指纹占了
/// 四成。planner 每个节点都要算一次，这个比例撑不住。
///
/// 换成可交换组合之后不用数组、不用排序：**多重集的顺序无关性由结合律保证**，
/// 不是靠先排序排出来的。
///
/// **用加法不用异或**：异或会让相同的两张牌互相抵消（一手两张打击和零张打击
/// 哈希相同），多重数就丢了。加法不抵消。
///
/// `pub` 是给 `bin/bench` 量它自己开销用的；planner 之外没有别的调用方。
pub fn key(s: &State) -> u64 {
    let mut h: u64 = 0x9E37_79B9_7F4A_7C15;
    // 顺序敏感的那一半：一个数一个数往里揉
    let mix = |x: u64, h: &mut u64| {
        *h = z(*h ^ x);
    };

    let mix_entity = |e: &crate::state::Entity, h: &mut u64| {
        mix(e.hp as u64, h);
        mix(e.block as u64, h);
        // status 是定长数组，下标本身就是身份，顺序敏感地揉进去
        for (i, v) in e.status.iter().enumerate() {
            if *v != 0 {
                mix(z((i as u64) << 32 ^ (*v as u16 as u64)), h);
            }
        }
    };
    mix_entity(&s.player, &mut h);
    mix(s.energy as u64, &mut h);
    mix(s.base_energy as u64, &mut h);
    mix(s.n_enemies as u64, &mut h);
    for e in 0..s.n_enemies as usize {
        mix_entity(&s.enemies[e], &mut h);
        mix(s.enemy_move[e] as u64, &mut h);
    }

    // 手牌 / 弃牌堆：**多重集**，可交换累加
    let mut bag: u64 = 0;
    for i in 0..s.n_hand as usize {
        bag = bag.wrapping_add(z(card_ident(s, s.hand[i])));
    }
    mix(bag, &mut h);
    mix(s.n_hand as u64, &mut h);

    let mut bag: u64 = 0;
    for i in 0..s.n_disc as usize {
        // 和手牌用**不同的盐**，否则"牌在手上"和"牌在弃牌堆"哈希不出区别
        bag = bag.wrapping_add(z(card_ident(s, s.disc[i]) ^ 0xD15C_A2D5_D15C_A2D5));
    }
    mix(bag, &mut h);
    mix(s.n_disc as u64, &mut h);

    // 抽牌堆：**有序**，下标进哈希
    for i in 0..s.n_draw as usize {
        mix(z(card_ident(s, s.draw[i]) ^ ((i as u64) << 56)), &mut h);
    }
    mix(s.n_draw as u64, &mut h);
    mix(s.n_exh as u64, &mut h);

    // 每回合计数器：条件牌读的就是它们（踩踏/无情猛攻/怨恨/邪眼）
    mix(s.cards_played as u64, &mut h);
    mix(s.attacks_played as u64, &mut h);
    mix(s.exhausted_this_turn as u64, &mut h);
    mix(s.hp_lost_this_turn as u64, &mut h);
    mix(s.free_attack as u64, &mut h);
    mix(s.last_damage as u64, &mut h);
    for p in s.potions.iter() {
        mix(*p as u64, &mut h);
    }
    mix(
        match s.pending {
            crate::state::Pending::None => 0,
            crate::state::Pending::ExhaustFromHand { remaining } => 1 + remaining as u64,
            crate::state::Pending::FetchFromDiscard { remaining } => 10 + remaining as u64,
            crate::state::Pending::PutToDrawPile { remaining } => 20 + remaining as u64,
            crate::state::Pending::DiscardToDrawTop { remaining, .. } => 40 + remaining as u64,
            crate::state::Pending::UpgradeInHand { remaining } => 30 + remaining as u64,
        },
        &mut h,
    );
    mix(s.combat_over as u64, &mut h);
    h
}

/// **抽牌堆 + 弃牌堆的多重集**指纹，顺序无关。
///
/// 这是机会节点的 CRN 种子要用的那个数（`hash(牌堆多重集, 深度, 全局种子)`）：
/// 同一个"还没抽的牌是这些"的局面，不管内核把它们洗成什么顺序，
/// **都要采到同一组手牌** —— 否则兄弟动作之间的比较里混进的是洗牌噪声，
/// 而不是决策差异。
///
/// **为什么把弃牌堆也算进去**：抽牌堆抽空会把弃牌堆洗回来，所以"接下来能抽到
/// 什么"由两者的并集决定。只看抽牌堆的话，一个周期的末尾（抽牌堆快空了）
/// 会得到一个几乎不带信息的种子。
///
/// **它不含手牌**：叶子停在"回合开始、未抽牌"，手牌那时是空的。
pub fn draw_multiset_key(s: &State) -> u64 {
    let mut bag: u64 = 0;
    for i in 0..s.n_draw as usize {
        bag = bag.wrapping_add(z(card_ident(s, s.draw[i])));
    }
    for i in 0..s.n_disc as usize {
        bag = bag.wrapping_add(z(card_ident(s, s.disc[i])));
    }
    z(bag ^ ((s.n_draw as u64) << 32) ^ (s.n_disc as u64))
}

struct Search<'a> {
    threat: &'a Threat,
    score: fn(&State) -> i32,
    /// 允许动用的药水槽位（bit i = 槽 i）。**这是策略，不是规则** ——
    /// 规则那边（`legal_actions`）照样把三瓶都算合法动作，是 L2 自己决定
    /// 这一次搜索里不碰它们。药水的机会成本因此被留在搜索**外面**，
    /// 见 [`advise_potions`]。
    allowed_potions: u16,
    seen: HashSet<u64>,
    nodes: u32,
    budget: u32,
    complete: bool,
    cur: Line,
    best: Line,
    /// `best` 那条线打完之后敌人还剩多少血（含没出场的形态）。
    /// 并列判据的**第二**关键字，见 `consider_stopping_here`。
    best_enemy_left: i32,
    /// 要留几条**候选**线（跨回合 planner 的 shortlist）。0 = 不留，
    /// 这时下面那个 `top` 一个字节都不动 —— 现有的 `solve_turn` 一分钱不多花。
    top_k: usize,
    /// `(分数, 敌人剩余HP, 这一回合打完的局面指纹, 线)`。
    ///
    /// **按"打完之后的局面"去重，不是按动作串。** 同一堆牌换个顺序到达同一个
    /// 局面，往下深搜是同一件事，占两个名额纯属浪费；而 planner 的 K 很小
    /// （每多一条就多 N1 次深搜），名额很贵。
    top: Vec<(i32, i32, u64, Line)>,
    /// 给**打出过能力牌的线**额外留几个名额。0 = 关掉，`top_power` 一个字节都不动。
    /// 理由见 [`solve_turn_topk`] 的「能力线保底」。
    power_reserve: usize,
    /// 和 `top` 同构，但只收**打出过能力牌**的线。
    top_power: Vec<(i32, i32, u64, Line)>,
    /// 当前这条线上打出过几张能力牌。随 `dfs` 的下降/回溯增减，
    /// 和 `cur` 是同一个生命周期。
    cur_powers: u32,
    /// 给**打掉敌人血量最多（高伤害/抢节奏）的线**额外留几个名额。0 = 关掉。
    /// 解决单回合分数与深搜叶评估的权重倒挂。
    damage_reserve: usize,
    /// 伤害保底表，优先收**剩余敌人HP最低**（即打掉敌人血量最多）的线。
    top_damage: Vec<(i32, i32, u64, Line)>,
}

/// 这个动作是不是"打出一张能力牌"。
///
/// **判据读的是 `CardDef.kind` 这张数据表，不是牌名** —— 内容表里加一张新能力牌
/// 不用回来改这里（不变量 3：卡牌是数据不是代码）。
#[inline]
fn plays_power_card(s: &State, a: Action) -> bool {
    match a {
        Action::PlayCard { hand, .. } => {
            let i = hand as usize;
            i < s.n_hand as usize
                && card(s.cards[s.hand[i] as usize].id).kind == crate::ops::Kind::Power
        }
        _ => false,
    }
}

/// 往一张候选表里塞一条线：同一个局面只留一个代表，超额就挤掉最差的。
///
/// 抽出来是因为 `top` / `top_power` / `top_damage` 的规矩必须**逐字相同** ——
/// 写成多份迟早会长歪，而长歪的样子是"保底那几条的去重口径和主表不一样"，
/// 不会报错、只会安静地少几条候选。
fn remember_into(
    top: &mut Vec<(i32, i32, u64, Line)>,
    cap: usize,
    ek: u64,
    v: i32,
    left: i32,
    line: Line,
) {
    if let Some(slot) = top.iter_mut().find(|(_, _, k, _)| *k == ek) {
        // 同一个局面已经有代表了：留更优的（分数高 -> 敌人血少 -> 长度短）
        let better = v > slot.0
            || (v == slot.0 && (left < slot.1 || (left == slot.1 && line.n < slot.3.n)));
        if better {
            *slot = (v, left, ek, line);
        }
        return;
    }
    top.push((v, left, ek, line));
    // 小 K，直接选择排序式地把最差的挤掉，不值得上堆
    if top.len() > cap {
        let mut worst = 0usize;
        for i in 1..top.len() {
            let worse = top[i].0 < top[worst].0
                || (top[i].0 == top[worst].0
                    && (top[i].1 > top[worst].1
                        || (top[i].1 == top[worst].1 && top[i].3.n > top[worst].3.n)));
            if worse {
                worst = i;
            }
        }
        top.swap_remove(worst);
    }
}

/// 往伤害保底表里塞一条线：优先留打掉敌人血量最多的线（left 越小越好），
/// 并列时比分数高（HP 损失少），再比线的长短。
fn remember_into_damage(
    top: &mut Vec<(i32, i32, u64, Line)>,
    cap: usize,
    ek: u64,
    v: i32,
    left: i32,
    line: Line,
) {
    if let Some(slot) = top.iter_mut().find(|(_, _, k, _)| *k == ek) {
        let better = left < slot.1
            || (left == slot.1 && (v > slot.0 || (v == slot.0 && line.n < slot.3.n)));
        if better {
            *slot = (v, left, ek, line);
        }
        return;
    }
    top.push((v, left, ek, line));
    if top.len() > cap {
        let mut worst = 0usize;
        for i in 1..top.len() {
            let worse = top[i].1 > top[worst].1
                || (top[i].1 == top[worst].1
                    && (top[i].0 < top[worst].0
                        || (top[i].0 == top[worst].0 && top[i].3.n > top[worst].3.n)));
            if worse {
                worst = i;
            }
        }
        top.swap_remove(worst);
    }
}

impl Search<'_> {
    /// 在这个局面上"就此收手"能得几分，并和当前最好的线比一比。
    fn consider_stopping_here(&mut self, s: &State, drew: bool) {
        self.nodes += 1;
        let end = self.threat.end_turn(*s);
        let v = (self.score)(&end);
        // 并列判据：**先比打掉的敌人血，再比线的长短**。
        //
        // 2026-08-31 之前只有"同分取更短"，理由写的是「少一次未建模的交互，
        // **而且手牌留着下回合还能用**」。**后半句是错的** —— 手牌回合末就弃掉，
        // 内核自己就是这么建的。于是这条判据会在并列时主动扔掉真实伤害。
        //
        // 实战抓到的样子（第 1 幕骇鳗，第 5 回合）：`打击->打击` 和 `打击`
        // 深层估值**逐字相等**（叶子把两边都判成胜利，而 `eval` 在胜利分支上
        // 提前返回、不数敌人血量），于是短的那条赢，白扔 6 点伤害。
        //
        // 剩下那半句理由（少一次未建模的交互）是真的，所以长度仍然是**第三**
        // 关键字 —— 只有在打掉的血也一样时才用它。
        let left = enemy_hp_left(&end);
        let better = v > self.best.score
            || (v == self.best.score
                && (left < self.best_enemy_left
                    || (left == self.best_enemy_left && self.cur.n < self.best.n)));
        if better {
            self.best = self.cur;
            self.best.score = v;
            self.best.drew = drew;
            self.best_enemy_left = left;
        }
        if self.top_k > 0 {
            self.remember_candidate(&end, v, left, drew);
        }
    }

    /// 把这条线记进候选表（只在 `top_k > 0` 时调用）。
    ///
    /// **三张表是并联的，不是二选一**：一条能力线或伤害线照样要参加主表的竞争。
    /// 保底表只负责在它输掉那场竞争时把它捞回来。
    fn remember_candidate(&mut self, end: &State, v: i32, left: i32, drew: bool) {
        let ek = key(end);
        let mut line = self.cur;
        line.score = v;
        line.drew = drew;
        remember_into(&mut self.top, self.top_k, ek, v, left, line);
        if self.cur_powers > 0 && self.power_reserve > 0 {
            remember_into(&mut self.top_power, self.power_reserve, ek, v, left, line);
        }
        if self.damage_reserve > 0 {
            remember_into_damage(&mut self.top_damage, self.damage_reserve, ek, v, left, line);
        }
    }

    fn dfs(&mut self, s: &State, drew: bool) {
        self.consider_stopping_here(s, drew);
        if s.combat_over || self.cur.n as usize >= MAX_LINE {
            return;
        }
        let (acts, n) = legal_actions(s);
        for &a in &acts[..n] {
            if let Action::UsePotion { slot, .. } = a {
                if self.allowed_potions & (1u16 << slot) == 0 {
                    continue;
                }
            }
            if matches!(a, Action::EndTurn) {
                // 结束回合不是一个要展开的分支 —— 每个节点都已经在
                // `consider_stopping_here` 里评过"到此为止"了。
                continue;
            }
            if self.nodes >= self.budget {
                self.complete = false;
                return;
            }
            let ns = step(*s, a);
            self.nodes += 1;
            // `step` 是全函数：非法动作原样返回。真遇到了说明 `legal_actions`
            // 和 `step` 的判定不一致 —— 那是 L1 的 bug，这里只能跳过。
            if ns == *s {
                continue;
            }
            if !self.seen.insert(key(&ns)) {
                continue;
            }
            let drew = drew || ns.n_draw != s.n_draw;
            // 能力牌要在 `step` **之前**认（牌打出去就离手了）。
            // `power_reserve == 0` 时这一句短路，主线一分钱不多花。
            let pw = (self.power_reserve > 0 && plays_power_card(s, a)) as u32;
            self.cur.push(a);
            self.cur_powers += pw;
            self.dfs(&ns, drew);
            self.cur_powers -= pw;
            self.cur.n -= 1;
        }
    }
}

/// 单回合求解：给定局面和威胁，搜这回合怎么出牌最好。
///
/// `score` 是参数（见模块头第 2 条）。默认目标函数是
/// [`score::survive_first`]。
pub fn solve_turn(s: &State, threat: &Threat, score: fn(&State) -> i32) -> Line {
    solve_turn_budget(s, threat, score, DEFAULT_BUDGET).line
}

/// 带预算和诊断的版本。**实战驱动应该用这个** —— [`Solved::complete`]
/// 决定了返回的东西是"解"还是"启发式排名"，这个区别不能被吞掉。
pub fn solve_turn_budget(
    s: &State,
    threat: &Threat,
    score: fn(&State) -> i32,
    budget: u32,
) -> Solved {
    solve_turn_potions(s, threat, score, budget, ALL_POTIONS)
}

/// 只允许动用 `allowed_potions` 里那几个槽位的求解。
///
/// `ALL_POTIONS` 就是"全部槽位都能动"；`0` 表示**这次搜索不许碰药水**
///（跨回合推演就传 0，见 `rollout::Policy::Solver`）。
/// 两者的差就是"药水这一手值多少"，[`advise_potions`] 靠它定价。
/// 搜出**最多 `k` 条候选线**（按分数从高到低），外加 `power_reserve` 条
/// **打出过能力牌**的保底线，以及 `damage_reserve` 条**打掉最多敌人血量**的伤害保底线。
///
/// 给跨回合 planner 的 shortlist 用：根回合有几十条线，逐条深搜是搜不动的
/// （每条要乘上机会节点的宽度），所以先用单回合分数把候选剪到 K 条。
///
/// **按"这一回合打完的局面"去重**：换序到达同一局面的线只留一条。
///
/// `k == 0` 时返回空表。`solve_turn` 那条路**完全不受影响** ——
/// 候选表只在 `k > 0` 时才维护。
///
/// # 权重倒挂与保底体系（`power_reserve` & `damage_reserve`）
///
/// 剪枝用的是**单回合分数**（如 `survive_first`），而深搜叶评估用的是 `Weights::LEAF`。
/// 两种典型倒挂：
/// 1. 能力牌当回合产出为 0，单回合必然垫底 ⇒ 由 `power_reserve` 保底捞回；
/// 2. 强攻/破防/斩杀线单回合吃少量擦伤而被单回合防守线挤出 Top-K ⇒ 由 `damage_reserve` 保底捞回。
///
/// **保底是纯增量的**：只补那些没能挤进 top-K 的线，一条原有候选都不挤掉，
/// 原有 K 条的**顺序也一个字不动**。
pub fn solve_turn_topk(
    s: &State,
    threat: &Threat,
    score: fn(&State) -> i32,
    budget: u32,
    allowed_potions: u16,
    k: usize,
    power_reserve: usize,
    damage_reserve: usize,
) -> Vec<Line> {
    let (mut main, power_res, dmg_res) = solve_turn_topk_split(
        s,
        threat,
        score,
        budget,
        allowed_potions,
        k,
        power_reserve,
        damage_reserve,
    );
    main.extend(power_res);
    main.extend(dmg_res);
    main
}

/// 同 [`solve_turn_topk`]，但**把三段分开返回**：`(主表, 能力保底, 伤害保底)`。
pub fn solve_turn_topk_split(
    s: &State,
    threat: &Threat,
    score: fn(&State) -> i32,
    budget: u32,
    allowed_potions: u16,
    k: usize,
    power_reserve: usize,
    damage_reserve: usize,
) -> (Vec<Line>, Vec<Line>, Vec<Line>) {
    if k == 0 {
        return (Vec::new(), Vec::new(), Vec::new());
    }
    let mut sr = Search {
        threat,
        score,
        allowed_potions,
        seen: HashSet::new(),
        nodes: 0,
        budget,
        complete: true,
        cur: Line::EMPTY,
        best: Line::EMPTY,
        best_enemy_left: i32::MAX,
        top_k: k,
        top: Vec::new(),
        power_reserve,
        top_power: Vec::new(),
        cur_powers: 0,
        damage_reserve,
        top_damage: Vec::new(),
    };
    sr.cur.score = 0;
    sr.dfs(s, false);
    sr.top.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.3.n.cmp(&b.3.n))
    });
    sr.top_power.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.3.n.cmp(&b.3.n))
    });
    sr.top_damage.sort_by(|a, b| {
        a.1.cmp(&b.1)
            .then_with(|| b.0.cmp(&a.0))
            .then_with(|| a.3.n.cmp(&b.3.n))
    });
    let main: Vec<Line> = sr.top.iter().map(|(_, _, _, l)| *l).collect();
    let mut rescued_power = Vec::new();
    for (_, _, ek, l) in sr.top_power.iter() {
        if sr.top.iter().any(|(_, _, mk, _)| mk == ek) {
            continue;
        }
        rescued_power.push(*l);
    }
    let mut rescued_damage = Vec::new();
    for (_, _, ek, l) in sr.top_damage.iter() {
        if sr.top.iter().any(|(_, _, mk, _)| mk == ek)
            || sr.top_power.iter().any(|(_, _, pk, _)| pk == ek)
        {
            continue;
        }
        rescued_damage.push(*l);
    }
    (main, rescued_power, rescued_damage)
}

pub fn solve_turn_potions(
    s: &State,
    threat: &Threat,
    score: fn(&State) -> i32,
    budget: u32,
    allowed_potions: u16,
) -> Solved {
    let baseline = score(&threat.end_turn(*s));
    let mut sr = Search {
        threat,
        score,
        allowed_potions,
        seen: HashSet::new(),
        nodes: 0,
        budget,
        complete: true,
        cur: Line::EMPTY,
        best: Line::EMPTY,
        best_enemy_left: i32::MAX,
        top_k: 0,
        top: Vec::new(),
        power_reserve: 0,
        top_power: Vec::new(),
        cur_powers: 0,
        damage_reserve: 0,
        top_damage: Vec::new(),
    };
    sr.cur.score = 0;
    sr.dfs(s, false);
    Solved { line: sr.best, nodes: sr.nodes, complete: sr.complete, baseline }
}

// ---------------------------------------------------------------------------
// 药水策略
// ---------------------------------------------------------------------------
//
// 药水是**全局资源，花在局部决策里**，而单回合目标函数是局部的。药水又不要
// 能量，所以只要边际收益 > 0 求解器就会喝 —— 实测过：满血 80/80、第 1 回合、
// 一只打 8 点的颚虫，它会把三瓶一次喝光。
//
// **机会成本不进目标函数。** 塞进 `eval` 里的话：数字会消失（只剩一个分数，
// 看不到"这瓶省了 12 血、门槛是 10"）、会污染验收器那条"实战线赢不过穷尽搜索"
// 的自检、而且把一个拍脑袋的价格混进了已经对拍验证过的规则里。
// 所以定价放在搜索**外面**：解 n+1 次，报差值，门槛在外面比。

/// 药水的机会成本。**和 [`Threat`] 一样是输入，不是求解器自己算的东西。**
///
/// 今天的提供者是玩家的家规（省 10 点血以上才值得喝）；将来 L3 能算出
/// "这一幕还剩几场仗、Boss 有多凶"，换个提供者就行，求解器不用改。
#[derive(Clone, Copy, Debug)]
pub struct PotionPolicy {
    /// 低于这个收益就别喝。单位是**血**，不是分 —— 玩家给的原话是
    /// "能减少 10 点以上血量损失就可以考虑使用"。
    pub reserve_hp: i32,
    /// 这场仗打完就没机会用了（Boss / 本幕最后一场）。为真时门槛归零。
    /// L2 不可能自己知道这件事，必须驱动方给。
    pub last_fight: bool,
}

impl PotionPolicy {
    /// 玩家 2026-08-16 给的家规。
    pub const HOUSE_RULE: PotionPolicy = PotionPolicy { reserve_hp: 10, last_fight: false };

    /// 实际用的门槛。
    ///
    /// **只有 `last_fight` 会改它。** 一度想加"满仓时降价"（药水槽有限，
    /// 攥着不喝会挡住下一瓶掉落 —— 玩家纠正过我囤药水），但按 3/3 打三折
    /// 试出来的效果是：门槛掉到 3 点血，于是满血第 1 回合又开始为省 3 点血
    /// 喝掉 12 点格挡药水 —— **正好把要解决的浪费问题原样搬了回来**。
    ///
    /// 根子在于"满仓要不要降价"取决于**这一幕还会不会再掉药水**，那是 L3
    /// 的知识，L2 编不出来。所以按本仓库的规矩：欠定就不编，留成输入。
    /// 真到了满仓想清仓，驱动方直接把 `reserve_hp` 调低就是了。
    pub fn reserve(&self) -> i32 {
        if self.last_fight {
            0
        } else {
            self.reserve_hp
        }
    }
}

/// 一瓶药水的定价结论。
#[derive(Clone, Debug)]
pub struct PotionAdvice {
    pub slot: usize,
    pub name: &'static str,
    /// 喝它能少掉几点血（和"这回合完全不用药水的最优线"比）。
    ///
    /// **纯血量差**，不折算打掉的敌人血：那部分价值跨回合兑现，L2 看不见，
    /// 宁可不算。打死敌人省下来的那一手伤害**已经**含在里面了 ——
    /// 威胁是按敌人死没死结算的。
    pub hp_saved: i32,
    pub verdict: PotionVerdict,
    /// 喝了它之后的最优线
    pub line: Line,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PotionVerdict {
    /// 不喝会死、喝了能活。门槛在这里不起作用。
    SaveMyLife,
    /// 收益 ≥ 门槛
    Drink,
    /// 收益 < 门槛，留着
    Hold,
    /// **不参与定价**：这瓶喝不了（[源码] `PotionUsage.Automatic`，瓶中精灵）。
    /// 它不是一个决策 —— 该不该"用"它由局面决定，不由我决定。
    /// 和 `CrossTurn` / `ThreatIsFixed` 的区别：那两个是"算不了"，这个是
    /// **"没有可算的东西"**。但它**照常占一个瓶位**，所以仍然列出来。
    Automatic,
    /// **拒绝评分**：跨回合药水（力量/敏捷/再生）的价值在后面几个回合，
    /// 单回合求解器结构性看不见。给一个自信的低分比不给分危险 ——
    /// 这是本仓库"拒绝比瞎猜好"的老规矩。
    CrossTurn,
    /// **拒绝评分**：这瓶药水改的是**敌人这一手打多少**，而 [`Threat`] 是
    /// 观测意图标签、是个**已经定死的常数**（`injected_enemy_turn` 直接拿
    /// `base` 去打，不过任何乘区）。所以给敌人上虚弱在这一层**完全没有效果**，
    /// 定价必然算出 0。
    ///
    /// 那个 0 不是"没用"，是"这一层看不见" —— 两者天差地别，
    /// 所以必须报成拒绝而不是 `Hold`。2026-08-21 实战抓到：
    /// 蛮兽来袭 14 点、手上虚弱药水，报「省 0 血 · 留着」，
    /// 而实际喝下去能把 14 削到 10。
    ///
    /// **2026-08-27 起这个判决只在冻住的口径下出现。** 威胁能现算的时候
    /// （`Threat::live`），给敌人上虚弱是算得出来的，照常定价。
    ThreatIsFixed,
}

/// 跨回合药水：单回合求解器**没有能力**给它们定价。
///
/// 力量/敏捷是乘在后面每一回合的输出上的，再生是每回合回血。单回合视角下
/// 力量药水只值 `2 × strength权重` ≈ 1.2 血，而一场五回合的仗它可能值 20 点伤害。
///
/// **判据是"价值在不在这一回合之内"，不是"给不给 status"**，两瓶新药水正好
/// 各站一边（2026-08-21）：
///
/// * 鱼油（力量1+敏捷1）、铁心（覆甲7，每个回合末都给格挡）—— **跨回合，拒绝评分**
/// * 屈伸药剂（`TempStrength` 5）—— 回合结束就被 `strip_temp_strength` 减回去，
///   **价值全在这一回合**，所以照常定价。把它一并塞进拒绝名单是错的。
/// * 爆炸安瓿（10 点全体伤害）—— 即时，照常定价。
fn is_cross_turn_potion(id: u8) -> bool {
    use crate::state::potion;
    matches!(
        id,
        potion::STRENGTH | potion::DEXTERITY | potion::REGEN | potion::FYSH_OIL | potion::HEART_OF_IRON
    )
}

/// 改「敌人这一手打多少」的药水：这一层**结构性地看不见**它们。
///
/// [`Threat`] 来自观测到的意图标签，标签已经是**最终伤害值**，而
/// `injected_enemy_turn` 拿着那个常数直接打、不过任何乘区。于是给敌人上虚弱
/// 在这一层是个空操作，定价永远得 0。
///
/// **报 0 是错的，报拒绝才对。** 这和跨回合药水是同一条规矩，只是原因不同：
/// 那边是"价值在后面几个回合"，这边是"威胁是个冻住的输入"。
///
/// 真要给它定价，得让威胁跟着 status 重算 —— 那就等于把敌人的伤害管线搬进
/// L2，正是"`Threat` 是输入"这条设计要避免的事。**留空，别猜。**
fn is_threat_shaping_potion(id: u8) -> bool {
    use crate::state::potion;
    matches!(id, potion::WEAK)
}

/// 挨完这一手之后还剩多少血。定价用的就是它 —— 纯血量，不含任何权重。
fn hp_after(s: &State, threat: &Threat, line: &Line) -> i32 {
    let Some(end) = replay_line(s, line.acts()) else { return s.player.hp };
    let after = threat.end_turn(end);
    if after.player_dead {
        // 死了按 0 算，好让"不喝会死、喝了剩 5 血"算出 +5 的收益
        0
    } else {
        after.player.hp
    }
}

/// 逐瓶给药水定价。
///
/// 返回 `(不用药水的最优线, 每一瓶的结论)`。解 n+1 次，n ≤ 3，
/// 实测一个回合最大 8624 节点，四次加起来还在几十毫秒。
pub fn advise_potions(
    s: &State,
    threat: &Threat,
    score: fn(&State) -> i32,
    budget: u32,
    policy: &PotionPolicy,
) -> (Solved, Vec<PotionAdvice>) {
    use crate::ops::potion_def;
    use crate::state::{potion, MAX_POTIONS};

    // 基准：这回合**一瓶都不喝**的最优线
    let dry = solve_turn_potions(s, threat, score, budget, 0);
    let dry_hp = hp_after(s, threat, &dry.line);
    let dry_dead = replay_line(s, dry.line.acts())
        .map(|e| threat.end_turn(e).player_dead)
        .unwrap_or(false);

    let reserve = policy.reserve();

    let mut out = Vec::new();
    // 同样按真实槽位数。这里比 `legal_actions` 还敏感：药水定价是**解 n+1 次**，
    // 按容量扫就是解 11 次而不是 4 次。
    for slot in 0..(s.potion_slots as usize).min(MAX_POTIONS) {
        let id = s.potions[slot];
        if id == potion::NONE || id == potion::UNKNOWN {
            continue;
        }
        // 喝不了的药水**不解那一次**：它进不了 `legal_actions`，
        // 解出来必然和基准线逐字相同，白花一次搜索还印出一个误导性的"省 0 血"。
        if crate::ops::potion_is_automatic(id) {
            out.push(PotionAdvice {
                slot,
                name: potion_def(id).name,
                hp_saved: 0,
                verdict: PotionVerdict::Automatic,
                line: Line::EMPTY,
            });
            continue;
        }
        let wet = solve_turn_potions(s, threat, score, budget, 1u16 << slot);
        let hp = hp_after(s, threat, &wet.line);
        let saved = hp - dry_hp;
        let alive = replay_line(s, wet.line.acts())
            .map(|e| !threat.end_turn(e).player_dead)
            .unwrap_or(false);
        let verdict = if dry_dead && alive {
            PotionVerdict::SaveMyLife
        } else if is_cross_turn_potion(id) {
            PotionVerdict::CrossTurn
        } else if is_threat_shaping_potion(id) && !threat.live {
            // **只有冻住的口径下才拒绝评分。** 现算口径里敌人这一击是结算那一刻
            // 才算的，给它上虚弱当然看得见 —— 2026-08-27 之前这里是无条件拒绝，
            // 那是"威胁是常数"这个前提的产物，前提没了这条也就不该留着。
            PotionVerdict::ThreatIsFixed
        } else if saved >= reserve {
            PotionVerdict::Drink
        } else {
            PotionVerdict::Hold
        };
        out.push(PotionAdvice {
            slot,
            name: potion_def(id).name,
            hp_saved: saved,
            verdict,
            line: wet.line,
        });
    }
    (dry, out)
}

/// 按顺序重放一条线，返回末态。动作在中途变得非法就返回 `None` ——
/// 这正是"这条线是从别的局面算出来的"的信号，不要静默地跳过它。
pub fn replay_line(s: &State, acts: &[Action]) -> Option<State> {
    let mut st = *s;
    for &a in acts {
        let ns = step(st, a);
        if ns == st {
            return None;
        }
        st = ns;
    }
    Some(st)
}

/// 给一条**外部给定**的线打分（用同一个目标函数、同一个威胁）。
///
/// 对拍求解器和人的打法时，两边必须走完全同一条评分路径，否则比出来的差
/// 说不清是策略差还是评分差。
pub fn score_line(
    s: &State,
    threat: &Threat,
    score: fn(&State) -> i32,
    acts: &[Action],
) -> Option<i32> {
    let end = replay_line(s, acts)?;
    Some(score(&threat.end_turn(end)))
}

/// 把一条线翻译成人能读的一串描述（牌名 + 目标槽位）。
///
/// 必须重放才能翻译：动作里的 `hand` 是执行到那一步时的下标。
/// 收 `&[Action]` 而不是 `&Line`，这样实战线（从 trace 来的）和求解线
/// 走同一个格式化路径，两行输出可以直接对着看。
pub fn explain(s: &State, acts: &[Action]) -> Vec<String> {
    let mut out = Vec::new();
    let mut st = *s;
    for &a in acts {
        match a {
            Action::PlayCard { hand, target } => {
                let name = if (hand as usize) < st.n_hand as usize {
                    let inst = st.cards[st.hand[hand as usize] as usize];
                    let up = if inst.upgraded() { "+" } else { "" };
                    format!("{}{}", card(inst.id).name, up)
                } else {
                    "<越界>".to_string()
                };
                // 多个敌人时才标目标 —— 单敌人时"@槽0"是纯噪音。
                // 用 `@` 不用箭头：箭头已经是线里两张牌之间的分隔符了。
                if st.n_enemies > 1 {
                    out.push(format!("{name}@槽{target}"));
                } else {
                    out.push(name);
                }
            }
            Action::Choose { hand } => {
                let name = if (hand as usize) < st.n_hand as usize {
                    card(st.cards[st.hand[hand as usize] as usize].id).name.to_string()
                } else {
                    "<越界>".to_string()
                };
                out.push(format!("选牌:{name}"));
            }
            Action::UsePotion { slot, target } => {
                let pot_id = if (slot as usize) < crate::state::MAX_POTIONS {
                    st.potions[slot as usize]
                } else {
                    0
                };
                let name = crate::ops::potion_def(pot_id).name;
                if st.n_enemies > 1 {
                    out.push(format!("药水[{slot}]:{name}@槽{target}"));
                } else {
                    out.push(format!("药水[{slot}]:{name}"));
                }
            }
            Action::EndTurn => out.push("结束回合".to_string()),
        }
        st = step(st, a);
    }
    out
}
