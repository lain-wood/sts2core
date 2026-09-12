//! **合成战斗构造器** —— `(牌组, 遗物, 血量, 遭遇, 种子) -> State`。
//!
//! 这是 L3（构筑顾问）的第一块地基：要评估「这副牌组走完这一幕会不会死」，
//! 首先得能**凭空搭出一场仗**来 —— 对拍那条路上的每一个局面都是从观测同步
//! 出来的（`Replayer::sync`），而 L3 手上没有观测，它问的是**还没发生的仗**。
//!
//! ```text
//! 牌组 ─┐
//! 遗物 ─┤
//! 血量 ─┼─► build ─► State ─► begin_combat 之后的第 1 回合起点
//! 遭遇 ─┤            （L2 从这里接手：solve_turn / rollout / plan）
//! 种子 ─┘
//! ```
//!
//! # 它自己不实现任何游戏规则
//!
//! 不变量说 L2/L3 只能经由 L1 的入口。这里用的是 `State::new` + `add_card` +
//! `add_enemy` + [`crate::step::grant_relic`] + [`crate::begin_combat`]，
//! 外加两张数据表（`content::RELICS` / `asc::ASC_HP`）。
//! **一条 `if 牌名` / `if 遗物名` 都没有** —— 有的话就说明表里缺东西。
//!
//! # 产出是信任，不是代码
//!
//! 验收台是 `bin/synth_audit`：拿实录的**第 0 帧**当测试集，让构造器搭同一场仗，
//! 和 `Replayer::sync` 出来的局面逐字段比。对不上的就是构造器（或它读的那几张
//! 表）缺的东西。今天的读数写在 `docs/verification-log.md`。
//!
//! # 三样它**故意**不猜
//!
//! | 拿不到的东西 | 处置 | 为什么不猜 |
//! |---|---|---|
//! | 遗物的跨战斗计数器（摆动球的相位） | 调用方不给就用 0，**并且报 [`Gap::RelicCounterMissing`]** | 0 是个具体的相位，猜错就是整场早/晚一个回合抽牌 —— 静默地错 |
//! | 敌人的初始血量 | 调用方不给就从 [`crate::asc::hp_range`] 的区间里掷 | 区间是 [源码] 档的真实分布；表里没有这只就退回 `EnemyDef::max_hp` 并报 [`Gap::NoHpRange`] |
//! | 内核不认识的牌 / 遗物 | 照放，但逐条报 [`Gap`] | 「不认识」和「没效果」是两件事，静默当成后者就是自信地算错 |

use crate::asc;
use crate::content::{card, enemy_def, relic_by_id, Arm, ENCHANTS};
use crate::ops::RelicDef;
use crate::state::{next_below, CardInst, St, State, F_UPGRADED, MAX_CARDS, MAX_ENEMIES, MAX_POTIONS};
use crate::step::grant_relics;

/// **阶段 3：整幕链式评估** —— 按 [源码] 的分布抽一条遭遇序列，血量跨场累计
pub mod act;
pub mod encounters;
/// **阶段 2：单场评估** —— 把这场仗打完 N 次，报 (死亡率, 血量分布, 截断率)
pub mod eval;
/// 观测 -> [`FightSpec`]（两个台子共用的那一步）
pub mod from_obs;

/// 牌组里的一张牌。**和 [`CardInst`] 同构，但少了两样战斗内才有的东西**：
/// `cost_delta`（狂乱逃离每打一次自己 +1 费）和 `F_FREE_THIS_TURN`
/// ——那两样都是"这一场打到一半才会有"的量，开局一律是 0。
///
/// `bonus` **留着**：假升级（`凋萎+1`）是跟着牌组走的，不是这一场攒的。
/// （痛殴那种"本场越打越强"也用同一个字段，但它开局是 0。）
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct DeckCard {
    pub id: u16,
    /// 关键字位（`F_*`）。升级和附魔给的关键字都落在这里
    pub flags: u8,
    /// 假升级（`凋萎+1`）攒下来的加值
    pub bonus: i16,
    /// `content::ENCHANTS` 的下标 **+1**，0 = 没附魔
    pub ench: u8,
    pub ench_amt: i8,
}

impl DeckCard {
    pub fn new(id: u16, upgraded: bool) -> DeckCard {
        DeckCard { id, flags: if upgraded { F_UPGRADED } else { 0 }, ..DeckCard::default() }
    }
}

/// 身上的一件遗物。`counter` 是**面板上那个跨战斗保留的计数器**
/// （游戏的 `DisplayAmount`），拿得到就传 —— 见 [`Gap::RelicCounterMissing`]。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RelicSpec<'a> {
    pub id: &'a str,
    pub counter: Option<i32>,
}

impl<'a> RelicSpec<'a> {
    pub fn new(id: &'a str) -> RelicSpec<'a> {
        RelicSpec { id, counter: None }
    }
}

/// 场上的一只敌人。`hp` 给 `None` 就从 [`crate::asc::hp_range`] 掷一个。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EnemySpec {
    pub def: u16,
    pub hp: Option<i32>,
}

impl EnemySpec {
    /// 血量从区间里掷（L3 的默认口径）
    pub fn rolled(def: u16) -> EnemySpec {
        EnemySpec { def, hp: None }
    }
    /// 血量指定（审计台拿观测到的血搭同一场仗时用）
    pub fn with_hp(def: u16, hp: i32) -> EnemySpec {
        EnemySpec { def, hp: Some(hp) }
    }
}

/// 一场仗的全部输入。
#[derive(Clone, Copy, Debug)]
pub struct FightSpec<'a> {
    pub deck: &'a [DeckCard],
    pub relics: &'a [RelicSpec<'a>],
    pub hp: i32,
    pub max_hp: i32,
    /// 开局**自带**的 status —— 遗物推不出来的那些（壶铃在休息处换的力量、
    /// 局外事件给的诅咒效果…）。挂在 `begin_combat` 之前，和遗物同一批。
    ///
    /// 审计台**故意一条都不传**：这样"遗物解释不了的那几个 status"
    /// 会原样出现在逐字段差里，而不是被一个万能参数填平。
    pub start_status: &'a [(St, i32)],
    /// **上一个房间是不是休息处。** 两个古茶具靠它武装（`content::CONDITIONAL_START`）
    /// —— 那是**局外状态**，战斗观测里没有，而整幕链自己知道在模拟哪个房间。
    ///
    /// 这是「L1 缺的信息在 L3 手上」的第一条：内核不该猜，调用方直接给。
    pub after_rest: bool,
    /// **这一场是不是 Boss 战**（缩放仪开局回 25 血）。
    /// 遭遇表里那一栏就是它（`encounters.json` 的 `room_type`）。
    pub boss_room: bool,
    pub enemies: &'a [EnemySpec],
    /// 药水槽的内容（`content::potion` 的 id，`potion::NONE` = 空槽）
    pub potions: &'a [u8],
    pub potion_slots: u8,
    /// 每回合的能量上限。**默认 3** —— 薪火之源那种"打出来才涨"的不算，
    /// 它是战斗内的 `Op::GainMaxEnergy`。这一栏装的是**开局就有**的那个数。
    pub base_energy: i32,
    pub ascension: u8,
    pub seed: u64,
}

impl<'a> FightSpec<'a> {
    /// 最小构造：只有牌组、血量、敌人。其余取默认（3 个药水槽、无遗物、A0）。
    pub fn new(deck: &'a [DeckCard], hp: i32, enemies: &'a [EnemySpec], seed: u64) -> FightSpec<'a> {
        FightSpec {
            deck,
            relics: &[],
            hp,
            max_hp: hp,
            start_status: &[],
            after_rest: false,
            boss_room: false,
            enemies,
            potions: &[],
            potion_slots: 3,
            base_energy: crate::solver::BASE_ENERGY,
            ascension: 0,
            seed,
        }
    }
}

/// 构造器**知道自己缺了什么**的那一份清单。
///
/// 这是本仓库那条老规矩在 L3 侧的同一个形状：**「不认识」和「没效果」是两件事**。
/// 每一条都指向一个具体的补法（补内容表 / 补一个调用方参数），
/// 而不是一个笼统的"可能不准"。
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Gap {
    /// 牌组里第 `ix` 张是内核不认识的牌（`card::UNKNOWN`）。它打不出来，
    /// 于是这副牌组在推演里**比真的弱**。
    /// （名字不在这里 —— 构造器只拿到 id，认得名字的是调用方）
    UnknownCard { ix: usize },
    /// `content::RELICS` 里没有这件遗物 —— 它的效果**一概不存在**
    UnknownRelic(String),
    /// 表里有，但 `modelled: false`：进表不等于建模
    UnmodelledRelic(String),
    /// 表里有、`modelled: true`，但那一列问的是**对拍**路径 ——
    /// 这一件在**合成**路径上还欠东西（`content::SYNTH_ONLY_GAPS`）。
    /// 开局手牌被升级、开局回血、开局塞药水那一类：对拍时它们是观测量，
    /// 合成时没有观测可抄
    SynthUnmodelledRelic { id: String, why: &'static str },
    /// 这件遗物有跨战斗计数器而调用方没给值，只好按 0 算。
    /// **方向不定** —— 摆动球按 0 起相位就是整场早/晚一个回合抽牌
    RelicCounterMissing(String),
    /// `asc::hp_range` 里没有这只敌人，退回 `EnemyDef::max_hp`（一个定值，
    /// 而真实血量是区间）
    NoHpRange { def: u16, name: &'static str },
    /// 牌组超过 `MAX_CARDS`，多出来的**没进去**
    DeckTruncated { kept: usize, dropped: usize },
    /// 敌人超过 `MAX_ENEMIES`，多出来的**没进去**
    EnemiesTruncated { kept: usize, dropped: usize },
}

impl std::fmt::Display for Gap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Gap::UnknownCard { ix } => write!(f, "牌组第 {ix} 张内核不认识"),
            Gap::UnknownRelic(id) => write!(f, "遗物 {id} 不在内容表里"),
            Gap::UnmodelledRelic(id) => write!(f, "遗物 {id} 进表了但没建模"),
            Gap::SynthUnmodelledRelic { id, why } => {
                write!(f, "遗物 {id} 只在合成路径上欠：{why}")
            }
            Gap::RelicCounterMissing(id) => write!(f, "遗物 {id} 的跨战斗计数器没给，按 0 算"),
            Gap::NoHpRange { def, name } => write!(f, "敌人「{name}」（def {def}）没有血量区间"),
            Gap::DeckTruncated { kept, dropped } => {
                write!(f, "牌组只放得下 {kept} 张，丢了 {dropped} 张")
            }
            Gap::EnemiesTruncated { kept, dropped } => {
                write!(f, "场上只放得下 {kept} 只敌人，丢了 {dropped} 只")
            }
        }
    }
}

/// 构造结果：局面 + 它自己知道的缺口。
pub struct Built {
    pub state: State,
    pub gaps: Vec<Gap>,
}

impl Built {
    /// 一条缺口都没有
    pub fn clean(&self) -> bool {
        self.gaps.is_empty()
    }
}

/// **敌人血量的掷点用一条独立的随机流。**
///
/// 不用 `State::rng` 的三条流里的任何一条，理由和不变量 4 里"三条分流"是同一句：
/// 掷血量发生在**开仗之前**，让它扰动战斗内的流，就等于"换一只敌人的血"会
/// 顺手改掉整局的抽牌顺序 —— 配对随机数比较（CRN）当场失效，而 L3 的候选
/// 之间正是靠 CRN 比的。
fn hp_stream(seed: u64) -> u64 {
    seed ^ 0x2545_F491_4F6C_DD1D
}

/// 从血量区间里掷一个。区间是**闭区间**（`asc::ASC_HP` 的 `(low, high)`
/// 就是 [源码] 那两个端点），所以 `high - low + 1` 个取值等概率。
fn roll_hp(def: u16, asc: u8, stream: &mut u64) -> Option<i32> {
    let (lo, hi) = asc::hp_range(def, asc)?;
    let span = (hi - lo).max(0) as usize + 1;
    Some(lo + next_below(stream, span) as i32)
}

/// `(牌组, 遗物, 血量, 遭遇, 种子) -> State`。
///
/// 出来的局面是**第 1 回合的起点**：洗过牌、发过手牌、回合开始的钩子跑过了
/// （赤牛的活力、绯红披风的格挡、佩尔之血多抽的那一张都在里面）。
/// L2 从这里直接接手。
///
/// 顺序不能换：**遗物必须在 [`crate::begin_combat`] 之前挂上**，
/// 因为它跑第 1 回合的 `TurnStart`，而好几件遗物的效果就挂在那个钩子上。
pub fn build(spec: &FightSpec) -> Built {
    let mut gaps: Vec<Gap> = Vec::new();
    let mut s = State::new(spec.hp, spec.seed);
    s.player.max_hp = spec.max_hp;
    s.ascension = spec.ascension;
    // 能量上限。`State::new` 给的是 3，这里照调用方说的来 ——
    // `begin_combat` 里 `start_player_turn` 会拿它回满。
    s.base_energy = spec.base_energy;
    s.energy = spec.base_energy;

    // 药水。槽位数是**一局的量**（进阶会减、药水腰带会加），所以由调用方给。
    s.potion_slots = spec.potion_slots.clamp(1, MAX_POTIONS as u8);
    for (i, p) in spec.potions.iter().enumerate().take(MAX_POTIONS) {
        s.potions[i] = *p;
    }

    // 牌组：全部进抽牌堆，`begin_combat` 会洗。
    let room = MAX_CARDS.saturating_sub(s.n_cards as usize);
    if spec.deck.len() > room {
        gaps.push(Gap::DeckTruncated { kept: room, dropped: spec.deck.len() - room });
    }
    for (ix, c) in spec.deck.iter().enumerate().take(room) {
        let slot = s.add_card(c.id, c.flags, c.bonus);
        s.cards[slot as usize].ench = c.ench;
        s.cards[slot as usize].ench_amt = c.ench_amt;
        if c.id == card::UNKNOWN {
            gaps.push(Gap::UnknownCard { ix });
        }
    }

    // 敌人。血量：给了就用，没给就掷。
    let mut stream = hp_stream(spec.seed);
    if spec.enemies.len() > MAX_ENEMIES {
        gaps.push(Gap::EnemiesTruncated {
            kept: MAX_ENEMIES,
            dropped: spec.enemies.len() - MAX_ENEMIES,
        });
    }
    for e in spec.enemies.iter().take(MAX_ENEMIES) {
        let def = enemy_def(e.def);
        let hp = match e.hp {
            Some(h) => h,
            None => match roll_hp(e.def, spec.ascension, &mut stream) {
                Some(h) => h,
                None => {
                    gaps.push(Gap::NoHpRange { def: e.def, name: def.name });
                    def.max_hp
                }
            },
        };
        s.add_enemy(e.def, hp);
    }

    // 开局自带的 status，和遗物同一批（都在 `begin_combat` 之前）。
    for (st, v) in spec.start_status {
        s.player.add(*st, *v);
    }

    // 遗物。**`begin_combat` 之前**，见函数头。
    // 先全部认出来再一起交给 L1 —— `grant_relics` 要走两趟（修饰器先就位），
    // 逐件调用会让结果依赖遗物在列表里的顺序，见那个函数的文档。
    let mut defs: Vec<(&RelicDef, Option<i32>)> = Vec::new();
    for r in spec.relics {
        match relic_by_id(r.id) {
            Some(def) => {
                note_relic_gaps(def, r, &mut gaps);
                defs.push((def, r.counter));
            }
            None => gaps.push(Gap::UnknownRelic(r.id.to_string())),
        }
    }
    grant_relics(&mut s, &defs);
    // **条件武装**（`content::CONDITIONAL_START`）：挂不挂取决于局外信息。
    // 放在 `grant_relics` 之后、`begin_combat` 之前 ——
    // 这几条规则都挂在第 1 回合的钩子上。
    for (def, counter) in &defs {
        let Some((arm, st, v)) = crate::content::conditional_start(def.id) else { continue };
        let armed = match arm {
            Arm::AfterRest => spec.after_rest,
            // 「还剩几场」游戏连面板都不显示（`ShowCounter => false`），
            // 拿不到就**当成没武装**（方向是低估自己），并报成缺口。
            Arm::RelicCounter => counter.unwrap_or(0) > 0,
            Arm::BossRoom => spec.boss_room,
        };
        if armed {
            s.player.add(st, v);
        } else if matches!(arm, Arm::RelicCounter) && counter.is_none() {
            gaps.push(Gap::RelicCounterMissing(def.id.to_string()));
        }
    }

    Built { state: crate::begin_combat(s), gaps }
}

fn note_relic_gaps(def: &RelicDef, spec: &RelicSpec, gaps: &mut Vec<Gap>) {
    if !def.modelled {
        gaps.push(Gap::UnmodelledRelic(def.id.to_string()));
    } else if let Some(why) = crate::content::synth_gap(def.id) {
        gaps.push(Gap::SynthUnmodelledRelic { id: def.id.to_string(), why });
    }
    if def.counter_to.is_some() && spec.counter.is_none() {
        gaps.push(Gap::RelicCounterMissing(def.id.to_string()));
    }
}

/// 牌**名字**（升级带 `+`，假升级带 `+N`）+ 观测到的附魔 -> [`DeckCard`]。
///
/// 这是「游戏里的牌 -> 内核的牌」那一步，和 `replay::sync` 的 `push`
/// **必须给出同一个 `CardInst`** —— 两边分岔的话，审计台比出来的差就说不清
/// 是构造器错了还是这一步错了。所以这里一条规则都不自己写：
/// `lookup_card` / `split_fake_upgrade` / `lookup_enchant` / `ENCHANTS.keywords`
/// 四个都是 `replay` / `content` 那边现成的。
///
/// `upgraded` 单独传，因为观测里有两个来源：手牌有 `is_upgraded` 字段，
/// 牌堆里没有（约束 3），只能看名字末尾那个 `+`。**照抄 `sync` 的分工**，
/// 不在这里替调用方决定。
///
/// 认不出的牌落成 `card::UNKNOWN`（[`build`] 会把它点名成 [`Gap::UnknownCard`]），
/// 认不出的附魔当没附魔算 —— 两个回退方向都是**低估这副牌组**。
pub fn card_from_name(name: &str, upgraded: bool, enchant_id: &str, enchant_amount: i32) -> DeckCard {
    let id = crate::replay::lookup_card(name).unwrap_or(card::UNKNOWN);
    let bonus = crate::replay::split_fake_upgrade(name).1 as i16 * crate::replay::FAKE_UPGRADE_STEP;
    let (ench, amt) = crate::replay::lookup_enchant(enchant_id, enchant_amount);
    let mut flags = if upgraded { F_UPGRADED } else { 0 };
    if ench != 0 {
        if let Some(def) = ENCHANTS.get(ench as usize - 1) {
            flags |= def.keywords;
        }
    }
    DeckCard { id, flags, bonus, ench, ench_amt: amt.clamp(-127, 127) as i8 }
}

/// 一张牌在**牌组身份**这个口径下的指纹：审计台拿它比多重集。
///
/// 不含 `cost_delta` / `F_FREE_THIS_TURN` 那两个战斗内才有的量 —— 开局它们
/// 一律是 0，进来只会让"这一格为什么不一样"变得难读。
pub fn card_ident(c: &CardInst) -> (u16, u8, i16, u8, i8) {
    (c.id, c.flags, c.bonus, c.ench, c.ench_amt)
}
