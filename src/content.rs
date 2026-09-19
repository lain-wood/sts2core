//! Card / enemy data tables.
//!
//! This is deliberately a *table*, not code. Phase 3 generates it from the
//! mod's `get_compendium` dump instead of hand-writing it.

use crate::ops::*;
use crate::state::{St, F_INNATE, F_RETAIN};

/// [源码] CardTag.Strike for the cards supported by this kernel.
pub const STRIKE_CARDS: &[u16] = &[
    card::STRIKE, card::ASH_STRIKE, card::SETUP_STRIKE, card::POMMEL_STRIKE,
    card::TWIN_STRIKE, card::ULTIMATE_STRIKE, card::PERFECTED_STRIKE,
];
pub fn is_strike(id: u16) -> bool { STRIKE_CARDS.contains(&id) }

pub mod card {
    pub const STRIKE: u16 = 0;
    pub const DEFEND: u16 = 1;
    pub const BASH: u16 = 2;
    pub const DISMANTLE: u16 = 3;
    pub const ASH_STRIKE: u16 = 4;
    pub const DEMON_FLAME: u16 = 5;
    pub const LIGHTNING: u16 = 6;
    pub const BULLY: u16 = 7;
    pub const BRAND: u16 = 8;
    pub const DOMINATE: u16 = 9;
    pub const STAMPEDE: u16 = 10;
    pub const DEMON_FORM: u16 = 11;
    pub const RUTHLESS: u16 = 12;
    pub const WOUND: u16 = 13;
    /// 占位：trace 里出现了内容表还没有的牌。没有任何效果，且不可打出，
    /// 所以它进入模拟时**不会伪造行为**——对拍器靠它把"我不认识"和"我算错了"
    /// 分开（见 `replay.rs`）。抽牌堆里内容未知的牌也用它填充。
    pub const UNKNOWN: u16 = 14;
    pub const HEMOKINESIS: u16 = 15;
    pub const SETUP_STRIKE: u16 = 16;
    // 触发式能力牌。规则在 `POWERS` 表里，这里只负责把 status 挂上去。
    pub const PYRE: u16 = 17;
    pub const FEEL_NO_PAIN: u16 = 18;
    pub const DARK_EMBRACE: u16 = 19;
    pub const RUPTURE: u16 = 20;
    pub const CRIMSON_MANTLE: u16 = 21;
    pub const ROLLING_BOULDER: u16 = 22;
    pub const VICIOUS: u16 = 23;
    // 24-52：靠现有原语 + 这一轮新加的叶子 Op 灌进来的一批。
    // 卡面全部来自权威卡表 `traces/cards_catalog.json`，**均未实战对拍**。
    pub const COMBUST: u16 = 24;
    pub const IRON_WAVE: u16 = 25;
    pub const SHRUG_IT_OFF: u16 = 26;
    pub const HEAVY_BLADE: u16 = 27;
    pub const UPPERCUT: u16 = 28;
    pub const POMMEL_STRIKE: u16 = 29;
    pub const TWIN_STRIKE: u16 = 30;
    pub const FLEX_TAUNT: u16 = 31;
    pub const BOULDER: u16 = 32;
    pub const IMMOLATE: u16 = 33;
    pub const MASTER_PLAN: u16 = 34;
    pub const ULTIMATE_STRIKE: u16 = 35;
    pub const GOOD_PLAN: u16 = 36;
    pub const SHIVER: u16 = 37;
    pub const SLIMED: u16 = 38;
    pub const BLOOD_WALL: u16 = 39;
    pub const BREAKTHROUGH: u16 = 40;
    pub const BLOODLETTING: u16 = 41;
    pub const FABRICATE: u16 = 42;
    pub const OFFERING: u16 = 43;
    pub const SHOCKWAVE: u16 = 44;
    pub const NOT_YET: u16 = 45;
    pub const BODY_SLAM: u16 = 46;
    pub const PUGILISM: u16 = 47;
    pub const ROCK_ARMOR: u16 = 48;
    pub const MOLTEN_FIST: u16 = 49;
    pub const OMNI_SLASH: u16 = 50;
    pub const DARK_SHACKLES: u16 = 51;
    pub const TORTURE: u16 = 52;
    // 53-61：触发式的最后一批 + 手牌垃圾牌
    pub const FRENZY: u16 = 53;
    pub const FLAME_BARRIER: u16 = 54;
    pub const BURN: u16 = 55;
    pub const INFECTION: u16 = 56;
    pub const DECAY: u16 = 57;
    pub const SHAME: u16 = 58;
    pub const DISINTEGRATION: u16 = 59;
    pub const DAZED: u16 = 60;
    pub const CLUMSY: u16 = 61;
    // 62-70：条件牌（读 State 早就有的每回合计数器）+ 三张规则修饰牌
    pub const EVIL_EYE: u16 = 62;
    pub const FORGOTTEN_RITE: u16 = 63;
    pub const RESENTMENT: u16 = 64;
    pub const IMPATIENCE: u16 = 65;
    pub const PERFECTED_STRIKE: u16 = 66;
    pub const RAMPAGE: u16 = 67;
    pub const BARRICADE: u16 = 68;
    pub const ENTRENCH: u16 = 69;
    pub const ALL_OR_NOTHING: u16 = 70;
    // 第一批铁甲补牌（2026-08-19），来源 [源码]
    pub const SWORD_BOOMERANG: u16 = 71;
    pub const FEED: u16 = 72;
    pub const EXPECT_A_FIGHT: u16 = 73;
    pub const JUGGERNAUT: u16 = 74;
    pub const COLOSSUS: u16 = 75;
    // 第二批（2026-08-19）
    pub const SECOND_WIND: u16 = 76;
    pub const BATTLE_TRANCE: u16 = 77;
    pub const BURNING_PACT: u16 = 78;
    pub const ARMAMENTS: u16 = 79;
    pub const HEADBUTT: u16 = 80;
    pub const ANGER: u16 = 81;
    // 牌生成那两张（2026-08-19）
    pub const STOKE: u16 = 82;
    pub const INFERNAL_BLADE: u16 = 83;
    // 「牌打牌」那批（2026-08-19）
    pub const HAVOC: u16 = 84;
    pub const STAMPEDE_CARD: u16 = 85;
    pub const ONE_TWO_PUNCH: u16 = 86;
    pub const JUGGLING: u16 = 87;
    pub const AGGRESSION: u16 = 88;
    // 最后 7 张（2026-08-19）
    pub const CINDER: u16 = 89;
    pub const TRUE_GRIT: u16 = 90;
    pub const THRASH: u16 = 91;
    pub const PILLAGE: u16 = 92;
    pub const PRIMAL_FORCE: u16 = 93;
    pub const WHIRLWIND: u16 = 94;
    pub const CASCADE: u16 = 95;
    pub const SPORE_MIND: u16 = 96;
    pub const SPOILS_MAP: u16 = 97;
    /// 狂乱逃离（第 2 幕 Boss 塞的状态牌）。**内核里唯一会改自己费用的牌**，
    /// 见 `CardInst::cost_delta`。
    pub const FRANTIC_ESCAPE: u16 = 98;
    /// 凋萎（[源码] `Wither`）。第 3 幕 Boss 永世沙漏的状态牌。
    /// 不可打出；回合结束时还在手上就吃 3 点（`Unpowered | Move`）。
    /// **它的伤害会被 Boss 的「剧烈增强」每轮 +3**，那一半内核没建。
    pub const WITHER: u16 = 99;
    /// 至亮之焰（[源码] `BrightestFlame`）。遗物「故事书」拾起时塞进牌组的那张。
    pub const BRIGHTEST_FLAME: u16 = 100;
    /// 贪婪之手（[源码] `HandOfGreed`）。**金币那一半 L1 不管** —— 见 CardDef 注释。
    pub const HAND_OF_GREED: u16 = 101;
    // ---- 2026-08-30 补的一批。权威表刷新（106 -> 128 张）之后逐张读源码建的 ----
    /// 亮剑（[源码] `FlashOfSteel`）
    pub const FLASH_OF_STEEL: u16 = 102;
    /// 啄击（[源码] `Peck`）。升级加的是**段数**不是伤害
    pub const PECK: u16 = 103;
    /// 岿然不动（[源码] `Impervious`）
    pub const IMPERVIOUS: u16 = 104;
    /// 究极防御（[源码] `UltimateDefend`）
    pub const ULTIMATE_DEFEND: u16 = 105;
    /// 箭雨（[源码] `Salvo`）。保留手牌那一半和均衡是**同一个** power
    pub const SALVO: u16 = 106;
    /// 毒素（[源码] `Toxic`）。手牌里发作的状态牌，但**能花 1 费打出去消耗掉**
    pub const TOXIC: u16 = 107;
    /// 贪婪（[源码] `Greed`）。诅咒，不可打出 + 永恒
    pub const GREED: u16 = 108;
    /// 债务（[源码] `Debt`）。诅咒，不可打出；回合末扣金币 —— **金币是局外量**
    pub const DEBT: u16 = 109;
    /// 坚定不移（[源码] `Unmovable`）
    pub const UNMOVABLE: u16 = 110;
    /// 吹哨（[源码] `Whistle`）。遗物「坦克斯的哨子」拾起时塞进牌组的那张
    pub const WHISTLE: u16 = 111;
    /// 涅奥之怒。遗物「涅奥的苦痛」拾起时塞进牌组的那张（2026-08-31 首见）
    pub const NEOW_WRATH: u16 = 112;
    /// 战鼓（[源码] `DrumOfBattle`）。消耗时获得能量
    pub const DRUM_OF_BATTLE: u16 = 113;
    /// 扯碎（[源码] `TearAsunder`）。添柴生成的铁甲攻击牌（2026-09-03 实战首见）
    pub const TEAR_ASUNDER: u16 = 114;
    /// 彼岸咆哮（[源码] `HowlFromBeyond`）。**在消耗堆里每回合自己再打一次**，
    /// 规则在 `EXHAUST_END_AUTOPLAY`
    pub const HOWL_FROM_BEYOND: u16 = 115;
    /// 呼唤（[源码] `Beckon`）。灵魂异鱼塞给我的**状态牌**：
    /// 回合末留在手上失去 6 点生命（不可格挡）。规则在 `HAND_END`。
    ///
    /// **别和「应急按钮」搞混** —— 那是另一张牌（技能，30 格挡 + 两回合
    /// 不能从卡牌获得格挡），内核还没有，`verify` 里照旧报「待导入的牌」。
    /// 第一版把这张 status 命名成了应急按钮，`verify` 当场多红一帧：
    /// 名字对不上 ⇒ `lookup_card` 查不到 ⇒ 回合末那 6 点没发作。
    pub const BECKON: u16 = 116;
}

/// **不可打出的牌。** 判据是 [源码] `CardKeyword.Unplayable`
/// （`MegaCrit.Sts2.Core.Models.Cards` 里 31 张带这个关键字），
/// 不是"这张牌有没有 `ops`"。
///
/// 这张表是 2026-08-22 被**孢子心灵**逼出来的。在它之前 `playable()` 靠
/// 「Status/Curse 且 ops 为空 ⇒ 不可打出」推断，理由写着"不能打出的那些卡面
/// 本来就没有可执行的效果"。孢子心灵推翻了它：`SporeMind` 是 Curse、
/// **没有任何 `OnPlay`**、却只带 `Exhaust` 关键字而**不带** `Unplayable`
/// —— 游戏里它真的能花 1 费打出来把自己消耗掉（第2幕第19层实录帧9 逐帧证实：
/// 能量 3→2、消耗堆 0→1、其余零变化）。
///
/// 换表之后**现有内容一个字节都没变**：当时表里所有 ops 为空的状态/诅咒牌
/// 恰好就是这张表里的这些，黏液（有 ops）恰好不在。所以这次改动对既有语料
/// 是恒等变换，五项对拍应当原样保持 —— 对不上就说明表填错了。
///
/// 和 `X_COST_CARDS` / `RULE_MODIFIERS` 同一个风格：少数牌才有的性质用表登记。
pub static UNPLAYABLE_CARDS: &[u16] = &[
    card::WITHER,
    card::WOUND,
    // 未知牌：内核不认识的一律当作不可打出（保守，宁可少给自己一个动作）
    card::UNKNOWN,
    card::BURN,
    card::INFECTION,
    card::DECAY,
    card::SHAME,
    card::DISINTEGRATION,
    card::DAZED,
    card::CLUMSY,
    card::SPOILS_MAP,
    // 2026-08-30：两张诅咒。[源码] 两张都带 `CardKeyword.Unplayable`
    card::GREED,
    card::DEBT,
];

#[inline]
pub fn unplayable(id: u16) -> bool {
    let mut i = 0;
    while i < UNPLAYABLE_CARDS.len() {
        if UNPLAYABLE_CARDS[i] == id {
            return true;
        }
        i += 1;
    }
    false
}
/// **X 费牌。** 打出时把当前能量全部花光，X = 花掉的那个数
/// （[源码] `CardCmd`：`CapturedXValue = playerCombatState.Energy`）。
///
/// 做成一张可枚举的小表而不是给 `CardDef` 加字段，和 `RULE_MODIFIERS` /
/// `TURN_SCOPED` / `GEN_POOL` 同一个风格：这类"少数牌才有的性质"用表登记，
/// 加一张就是加一行，完整性测试也好写。
///
/// **X 费不需要给 `Action` 加参数** —— 它不是玩家的选择，是局面决定的。
/// 我一度以为这是个接口改动、要单独一轮，读了源码才发现想多了。
pub static X_COST_CARDS: &[u16] = &[card::WHIRLWIND, card::CASCADE];

#[inline]
pub fn costs_x(id: u16) -> bool {
    let mut i = 0;
    while i < X_COST_CARDS.len() {
        if X_COST_CARDS[i] == id {
            return true;
        }
        i += 1;
    }
    false
}


const NO_OPS: &[Op] = &[];

pub static CARDS: &[CardDef] = &[
    // 0 打击
    CardDef {
        name: "打击",
        cost: 1,
        kind: Kind::Attack,
        targeted: true,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Damage { base: 6, hits: 1, scale: Scale::None }],
        ops_upg: &[Op::Damage { base: 9, hits: 1, scale: Scale::None }],
        cost_upg: 1,
    },
    // 1 防御
    CardDef {
        name: "防御",
        cost: 1,
        kind: Kind::Skill,
        targeted: false,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Block { base: 5 }],
        ops_upg: &[Op::Block { base: 8 }],
        cost_upg: 1,
    },
    // 2 痛击 —— 基础值 8 已由实测确认（2026-08-15 第1幕第6层，无 debuff 打出 8 点）
    //
    // 这里踩过一次坑，留作记录：先前在第2层看到痛击只打 5 点、打击只打 4 点，
    // 就假设 `SHRINK_POWER` 是加性 -2，反推出基础值 7 并改了这张表。错了。
    // 第6层拿到无 debuff 的干净样本后才发现 SHRINK 是**乘性 ×2/3**：
    //     打击 6×2/3=4 ✓   痛击 8×2/3=5.33→5 ✓   打击带易伤 6×2/3×1.5=6 ✓
    // 三个观测能同时满足加性和乘性两套解，是第四个观测把它们分开的。
    //
    // 教训就是本仓库自己写过的那句：**以对拍结果为准，不要凭直觉改数字**。
    // 手上只有欠定的数据点时，正确做法是让 verify 报 UNKNOWN_CONTENT 等更多样本，
    // 而不是挑一组自洽的解写进表里。
    CardDef {
        name: "痛击",
        cost: 2,
        kind: Kind::Attack,
        targeted: true,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[
            Op::Damage { base: 8, hits: 1, scale: Scale::None },
            Op::Status { tgt: Tgt::Enemy, st: St::Vulnerable, amt: 2 },
        ],
        ops_upg: &[
            Op::Damage { base: 10, hits: 1, scale: Scale::None },
            Op::Status { tgt: Tgt::Enemy, st: St::Vulnerable, amt: 3 },
        ],
        cost_upg: 2,
    },
    // 3 拆卸 — attacks twice if the target is Vulnerable
    CardDef {
        name: "拆卸",
        cost: 1,
        kind: Kind::Attack,
        targeted: true,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[
            Op::Damage { base: 8, hits: 1, scale: Scale::None },
            Op::DamageIfVuln { base: 8, scale: Scale::None },
        ],
        ops_upg: &[
            Op::Damage { base: 10, hits: 1, scale: Scale::None },
            Op::DamageIfVuln { base: 10, scale: Scale::None },
        ],
        cost_upg: 1,
    },
    // 4 灰烬打击 — scales with the exhaust pile
    CardDef {
        name: "灰烬打击",
        cost: 1,
        kind: Kind::Attack,
        targeted: true,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Damage { base: 6, hits: 1, scale: Scale::PerExhaust(3) }],
        ops_upg: &[Op::Damage { base: 6, hits: 1, scale: Scale::PerExhaust(4) }],
        cost_upg: 1,
    },
    // 5 恶魔之焰
    CardDef {
        name: "恶魔之焰",
        cost: 2,
        kind: Kind::Attack,
        targeted: true,
        exhausts: true,
        cost_minus_attacks: false,
        ops: &[Op::ExhaustHandDamage { per: 7 }],
        ops_upg: &[Op::ExhaustHandDamage { per: 10 }],
        cost_upg: 2,
    },
    // 6 闪电霹雳
    CardDef {
        name: "闪电霹雳",
        cost: 1,
        kind: Kind::Attack,
        targeted: false,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[
            Op::DamageAll { base: 4, hits: 1, scale: Scale::None },
            Op::Status { tgt: Tgt::AllEnemies, st: St::Vulnerable, amt: 1 },
        ],
        ops_upg: &[
            Op::DamageAll { base: 7, hits: 1, scale: Scale::None },
            Op::Status { tgt: Tgt::AllEnemies, st: St::Vulnerable, amt: 1 },
        ],
        cost_upg: 1,
    },
    // 7 欺凌
    CardDef {
        name: "欺凌",
        cost: 0,
        kind: Kind::Attack,
        targeted: true,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Damage { base: 4, hits: 1, scale: Scale::PerTargetVuln(2) }],
        ops_upg: &[Op::Damage { base: 4, hits: 1, scale: Scale::PerTargetVuln(3) }],
        cost_upg: 0,
    },
    // 8 烙印
    CardDef {
        name: "烙印",
        cost: 0,
        kind: Kind::Skill,
        targeted: false,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[
            Op::LoseHp(1),
            Op::ExhaustChoose(1),
            Op::Status { tgt: Tgt::Me, st: St::Strength, amt: 1 },
        ],
        ops_upg: &[
            Op::LoseHp(1),
            Op::ExhaustChoose(1),
            Op::Status { tgt: Tgt::Me, st: St::Strength, amt: 2 },
        ],
        cost_upg: 0,
    },
    // 9 主宰
    CardDef {
        name: "主宰",
        cost: 1,
        kind: Kind::Skill,
        targeted: true,
        exhausts: true,
        cost_minus_attacks: false,
        ops: &[
            Op::Status { tgt: Tgt::Enemy, st: St::Vulnerable, amt: 1 },
            Op::StrengthPerTargetVuln,
        ],
        // 升级版给 **2** 层易伤 —— 卡面原文（2026-08-15 第1幕第4层奖励）：
        // "给予2层易伤。 敌人身上每有一层易伤，就获得1点力量。 消耗。"
        // 这是读到的卡面而不是初代记忆，但**没在战斗里实测过**。
        // 之前 `ops_upg` 是空的，会回退到基础版的 1 层，拿到主宰+ 就会算错。
        ops_upg: &[
            Op::Status { tgt: Tgt::Enemy, st: St::Vulnerable, amt: 2 },
            Op::StrengthPerTargetVuln,
        ],
        cost_upg: 1,
    },
    // 10 踩踏 —— 卡面已核对（2026-08-15 第1幕第8层奖励），游戏 id 是 `STOMP`：
    // "对所有敌人造成12点伤害。 你在本回合中每打出过一张攻击牌，其耗能减少1。"
    // 和这里的 3 费 / AoE 12 / cost_minus_attacks 完全吻合。
    //
    // 一度误判：看到商店里的 `STAMPEDE` 是 1 费 Power「惊逃」，就以为这张
    // 「踩踏」是照初代记忆捏造的。两者是不同的牌，`STOMP` 才是踩踏。
    // 查表走中文名，本来就匹配正确 —— 教训是**别拿 id 猜牌**。
    CardDef {
        name: "踩踏",
        cost: 3,
        kind: Kind::Attack,
        targeted: false,
        exhausts: false,
        cost_minus_attacks: true,
        ops: &[Op::DamageAll { base: 12, hits: 1, scale: Scale::None }],
        ops_upg: &[Op::DamageAll { base: 15, hits: 1, scale: Scale::None }],
        cost_upg: 3,
    },
    // 11 恶魔形态 — a Power, so it does NOT trigger 激怒
    CardDef {
        name: "恶魔形态",
        cost: 3,
        kind: Kind::Power,
        targeted: false,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Status { tgt: Tgt::Me, st: St::DemonForm, amt: 2 }],
        ops_upg: &[Op::Status { tgt: Tgt::Me, st: St::DemonForm, amt: 3 }],
        cost_upg: 3,
    },
    // 12 无情猛攻
    CardDef {
        name: "无情猛攻",
        cost: 2,
        kind: Kind::Attack,
        targeted: true,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[
            Op::Damage { base: 14, hits: 1, scale: Scale::None },
            Op::FreeNextAttack,
        ],
        ops_upg: &[
            Op::Damage { base: 20, hits: 1, scale: Scale::None },
            Op::FreeNextAttack,
        ],
        cost_upg: 2,
    },
    // 13 伤口
    CardDef {
        name: "伤口",
        cost: 0,
        kind: Kind::Status,
        targeted: false,
        exhausts: false,
        cost_minus_attacks: false,
        ops: NO_OPS,
        ops_upg: NO_OPS,
        cost_upg: 0,
    },
    // 14 未知牌 —— 对拍占位，见 card::UNKNOWN
    CardDef {
        name: "<未知牌>",
        cost: 0,
        kind: Kind::Status,
        targeted: false,
        exhausts: false,
        cost_minus_attacks: false,
        ops: NO_OPS,
        ops_upg: NO_OPS,
        cost_upg: 0,
    },
    // 15 御血术 —— 升级版实测（2026-08-15 第1幕第6层）：敌人 57→37 共 20 点，
    // 自身 68→66 失 2 血，与卡面"失去2点生命。造成20点伤害。"一致。
    // 基础版的 15 当初是照初代数值猜的，现在已由权威卡表证实
    // （`traces/cards_catalog.json`："失去2点生命。 造成15点伤害。"）。
    CardDef {
        name: "御血术",
        cost: 1,
        kind: Kind::Attack,
        targeted: true,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::LoseHp(2), Op::Damage { base: 15, hits: 1, scale: Scale::None }],
        ops_upg: &[Op::LoseHp(2), Op::Damage { base: 20, hits: 1, scale: Scale::None }],
        cost_upg: 1,
    },
    // 16 预备打击 —— 卡面"造成7点伤害。 在本回合内获得2点力量。"
    // 实测（2026-08-15 第1幕第13层）：对 127 血无 debuff 的雕像打出 **7**，
    // 之后玩家身上同时出现 `STRENGTH_POWER=2` 和 `SETUP_STRIKE_POWER=2`。
    //
    // op 顺序要紧：伤害在加力量**之前**（打出的是 7 不是 9）。
    // 升级版"造成9点伤害。 在本回合内获得3点力量。"来自权威卡表
    // （`traces/cards_catalog.json`，GET /api/v1/wiki），**未实测**。
    CardDef {
        name: "预备打击",
        cost: 1,
        kind: Kind::Attack,
        targeted: true,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[
            Op::Damage { base: 7, hits: 1, scale: Scale::None },
            Op::Status { tgt: Tgt::Me, st: St::Strength, amt: 2 },
            Op::Status { tgt: Tgt::Me, st: St::TempStrength, amt: 2 },
        ],
        ops_upg: &[
            Op::Damage { base: 9, hits: 1, scale: Scale::None },
            Op::Status { tgt: Tgt::Me, st: St::Strength, amt: 3 },
            Op::Status { tgt: Tgt::Me, st: St::TempStrength, amt: 3 },
        ],
        cost_upg: 1,
    },
    // ---- 17-22 触发式能力牌 ----
    //
    // 这六张是触发器系统的验收样本：**每一张都只是一行 `Op::Status`**，
    // `step.rs` 里没有为它们加过任何一行代码，行为全在 `POWERS` 表里。
    // 卡面数值来自 `traces/cards_catalog.json`，**尚未实战对拍**。
    //
    // 17 薪火之源「在回合开始时，获得1能量。」升级 2 能量。
    //
    // **实际是能量上限 +1，不是回合开始给 1 点。** 实测（第1幕 Boss）：
    // 打出的瞬间显示就从 `3/3` 变成 `1/4`。按卡面建成 TurnStart 钩子会和
    // `sync` 拿到的 `max_energy` 重复计数，对拍当场报出 `能量 游戏=4 内核=5`。
    // `PYRE_POWER` 这个 status 只是显示印记，见 `MARKER_STATUSES`。
    CardDef {
        name: "薪火之源",
        cost: 2,
        kind: Kind::Power,
        targeted: false,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Status { tgt: Tgt::Me, st: St::Pyre, amt: 1 }, Op::GainMaxEnergy(1)],
        ops_upg: &[Op::Status { tgt: Tgt::Me, st: St::Pyre, amt: 2 }, Op::GainMaxEnergy(2)],
        cost_upg: 2,
    },
    // 18 无惧疼痛「每当有一张牌被消耗时，获得3点格挡。」升级 4
    CardDef {
        name: "无惧疼痛",
        cost: 1,
        kind: Kind::Power,
        targeted: false,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Status { tgt: Tgt::Me, st: St::FeelNoPain, amt: 3 }],
        ops_upg: &[Op::Status { tgt: Tgt::Me, st: St::FeelNoPain, amt: 4 }],
        cost_upg: 1,
    },
    // 19 黑暗之拥「每当有一张牌被消耗时，抽1张牌。」升级只降费（2 -> 1）
    CardDef {
        name: "黑暗之拥",
        cost: 2,
        kind: Kind::Power,
        targeted: false,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Status { tgt: Tgt::Me, st: St::DarkEmbrace, amt: 1 }],
        ops_upg: &[Op::Status { tgt: Tgt::Me, st: St::DarkEmbrace, amt: 1 }],
        cost_upg: 1,
    },
    // 20 撕裂「每当你在你的回合失去生命值时，获得1点力量。」升级 2
    CardDef {
        name: "撕裂",
        cost: 1,
        kind: Kind::Power,
        targeted: false,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Status { tgt: Tgt::Me, st: St::Rupture, amt: 1 }],
        ops_upg: &[Op::Status { tgt: Tgt::Me, st: St::Rupture, amt: 2 }],
        cost_upg: 1,
    },
    // 21 绯红披风「在你的回合开始时，失去1点生命并获得8点格挡。」升级 10
    CardDef {
        name: "绯红披风",
        cost: 1,
        kind: Kind::Power,
        targeted: false,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Status { tgt: Tgt::Me, st: St::CrimsonMantle, amt: 8 }],
        ops_upg: &[Op::Status { tgt: Tgt::Me, st: St::CrimsonMantle, amt: 10 }],
        cost_upg: 1,
    },
    // 22 滚石「在你的回合开始时，对所有敌人造成5点伤害，然后将该伤害增加5点。」
    // 升级起始 10。层数会被 `TOp::GrowSelf` 自己改大，是唯一一个层数不恒定的 power。
    CardDef {
        name: "滚石",
        cost: 3,
        kind: Kind::Power,
        targeted: false,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Status { tgt: Tgt::Me, st: St::RollingBoulder, amt: 5 }],
        ops_upg: &[Op::Status { tgt: Tgt::Me, st: St::RollingBoulder, amt: 10 }],
        cost_upg: 3,
    },
    // 23 凶恶「每当你给予易伤时，抽1张牌。」升级 2 张。
    // 判定按**每个吃到易伤的敌人各一次**：闪电霹雳打 3 个敌人就抽 3 张。
    CardDef {
        name: "凶恶",
        cost: 1,
        kind: Kind::Power,
        targeted: false,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Status { tgt: Tgt::Me, st: St::Vicious, amt: 1 }],
        ops_upg: &[Op::Status { tgt: Tgt::Me, st: St::Vicious, amt: 2 }],
        cost_upg: 1,
    },

    // ======================================================================
    // 24-52 批量灌入。卡面全部逐字来自权威卡表 `traces/cards_catalog.json`
    //（`GET /api/v1/wiki`，含升级版），**没有一张在实战里对拍过**。
    //
    // `targeted` 是按卡面文本判的（"给予N层易伤" = 单体，"所有敌人" = 全体），
    // 卡表里**没有 target_type 字段**，所以这一列是推断不是数据。判错的话
    // `legal_actions` 会生成不该有的动作，实战第一次打出来就会暴露。
    // ======================================================================

    // 24 燃烧「获得2点力量。」—— 是 Power 牌但效果是立即的，没有触发器
    CardDef {
        name: "燃烧", cost: 1, kind: Kind::Power, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Status { tgt: Tgt::Me, st: St::Strength, amt: 2 }],
        ops_upg: &[Op::Status { tgt: Tgt::Me, st: St::Strength, amt: 3 }],
        cost_upg: 1,
    },
    // 25 铁斩波「获得5点格挡。 造成5点伤害。」—— 先格挡后伤害，顺序照卡面
    CardDef {
        name: "铁斩波", cost: 1, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Block { base: 5 }, Op::Damage { base: 5, hits: 1, scale: Scale::None }],
        ops_upg: &[Op::Block { base: 7 }, Op::Damage { base: 7, hits: 1, scale: Scale::None }],
        cost_upg: 1,
    },
    // 26 耸肩无视「获得8点格挡。 抽1张牌。」
    CardDef {
        name: "耸肩无视", cost: 1, kind: Kind::Skill, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Block { base: 8 }, Op::Draw(1)],
        ops_upg: &[Op::Block { base: 11 }, Op::Draw(1)],
        cost_upg: 1,
    },
    // 27 重锤「造成32点伤害。」
    CardDef {
        name: "重锤", cost: 3, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Damage { base: 32, hits: 1, scale: Scale::None }],
        ops_upg: &[Op::Damage { base: 42, hits: 1, scale: Scale::None }],
        cost_upg: 3,
    },
    // 28 上勾拳「造成13点伤害。 给予1层虚弱。 给予1层易伤。」升级只加层数
    CardDef {
        name: "上勾拳", cost: 2, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[
            Op::Damage { base: 13, hits: 1, scale: Scale::None },
            Op::Status { tgt: Tgt::Enemy, st: St::Weak, amt: 1 },
            Op::Status { tgt: Tgt::Enemy, st: St::Vulnerable, amt: 1 },
        ],
        ops_upg: &[
            Op::Damage { base: 13, hits: 1, scale: Scale::None },
            Op::Status { tgt: Tgt::Enemy, st: St::Weak, amt: 2 },
            Op::Status { tgt: Tgt::Enemy, st: St::Vulnerable, amt: 2 },
        ],
        cost_upg: 2,
    },
    // 29 剑柄打击「造成9点伤害。 抽1张牌。」
    CardDef {
        name: "剑柄打击", cost: 1, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Damage { base: 9, hits: 1, scale: Scale::None }, Op::Draw(1)],
        ops_upg: &[Op::Damage { base: 10, hits: 1, scale: Scale::None }, Op::Draw(2)],
        cost_upg: 1,
    },
    // 30 双重打击「造成5点伤害两次。」两次共用同一个缓慢层数（拆卸+ 已实测）
    CardDef {
        name: "双重打击", cost: 1, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Damage { base: 5, hits: 2, scale: Scale::None }],
        ops_upg: &[Op::Damage { base: 7, hits: 2, scale: Scale::None }],
        cost_upg: 1,
    },
    // 31 挑衅「获得7点格挡。 给予1层易伤。」
    CardDef {
        name: "挑衅", cost: 1, kind: Kind::Skill, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Block { base: 7 }, Op::Status { tgt: Tgt::Enemy, st: St::Vulnerable, amt: 1 }],
        ops_upg: &[Op::Block { base: 8 }, Op::Status { tgt: Tgt::Enemy, st: St::Vulnerable, amt: 2 }],
        cost_upg: 1,
    },
    // 32 巨石「造成16点伤害。」
    CardDef {
        name: "巨石", cost: 1, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Damage { base: 16, hits: 1, scale: Scale::None }],
        ops_upg: &[Op::Damage { base: 20, hits: 1, scale: Scale::None }],
        cost_upg: 1,
    },
    // 33 焚烧「对所有敌人造成2点伤害4次。」
    // 内核是"打完一个敌人的 4 下再打下一个"，真实游戏可能是轮流打。
    // 只有在中途有东西变化时才有差别（比如敌人死了），暂记于此。
    CardDef {
        name: "焚烧", cost: 1, kind: Kind::Attack, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::DamageAll { base: 2, hits: 4, scale: Scale::None }],
        ops_upg: &[Op::DamageAll { base: 2, hits: 5, scale: Scale::None }],
        cost_upg: 1,
    },
    // 34 战略大师「抽3张牌。 消耗。」
    CardDef {
        name: "战略大师", cost: 0, kind: Kind::Skill, targeted: false, exhausts: true,
        cost_minus_attacks: false,
        ops: &[Op::Draw(3)],
        ops_upg: &[Op::Draw(4)],
        cost_upg: 0,
    },
    // 35 究极打击「造成14点伤害。」
    CardDef {
        name: "究极打击", cost: 1, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Damage { base: 14, hits: 1, scale: Scale::None }],
        ops_upg: &[Op::Damage { base: 20, hits: 1, scale: Scale::None }],
        cost_upg: 1,
    },
    // 36 妙计「获得4点格挡。 抽1张牌。」
    CardDef {
        name: "妙计", cost: 0, kind: Kind::Skill, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Block { base: 4 }, Op::Draw(1)],
        ops_upg: &[Op::Block { base: 7 }, Op::Draw(1)],
        cost_upg: 0,
    },
    // 37 战栗「给予3层易伤。 消耗。」
    CardDef {
        name: "战栗", cost: 1, kind: Kind::Skill, targeted: true, exhausts: true,
        cost_minus_attacks: false,
        ops: &[Op::Status { tgt: Tgt::Enemy, st: St::Vulnerable, amt: 3 }],
        ops_upg: &[Op::Status { tgt: Tgt::Enemy, st: St::Vulnerable, amt: 4 }],
        cost_upg: 1,
    },
    // 38 黏液「抽1张牌。 消耗。」1 费，是**能打出来的状态牌** ——
    // 和伤口不一样，见 `playable()`
    CardDef {
        name: "黏液", cost: 1, kind: Kind::Status, targeted: false, exhausts: true,
        cost_minus_attacks: false,
        ops: &[Op::Draw(1)],
        ops_upg: NO_OPS,
        cost_upg: 1,
    },
    // 39 血墙「失去2点生命。 获得16点格挡。」
    CardDef {
        name: "血墙", cost: 2, kind: Kind::Skill, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::LoseHp(2), Op::Block { base: 16 }],
        ops_upg: &[Op::LoseHp(2), Op::Block { base: 20 }],
        cost_upg: 2,
    },
    // 40 突破「失去1点生命。 对所有敌人造成9点伤害。」
    CardDef {
        name: "突破", cost: 1, kind: Kind::Attack, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::LoseHp(1), Op::DamageAll { base: 9, hits: 1, scale: Scale::None }],
        ops_upg: &[Op::LoseHp(1), Op::DamageAll { base: 13, hits: 1, scale: Scale::None }],
        cost_upg: 1,
    },
    // 41 放血「失去3点生命。 获得2能量。」
    CardDef {
        name: "放血", cost: 0, kind: Kind::Skill, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::LoseHp(3), Op::GainEnergy(2)],
        ops_upg: &[Op::LoseHp(3), Op::GainEnergy(3)],
        cost_upg: 0,
    },
    // 42 生产制造「获得2能量。 消耗。」
    CardDef {
        name: "生产制造", cost: 0, kind: Kind::Skill, targeted: false, exhausts: true,
        cost_minus_attacks: false,
        ops: &[Op::GainEnergy(2)],
        ops_upg: &[Op::GainEnergy(3)],
        cost_upg: 0,
    },
    // 43 祭品「失去6点生命。 获得2能量。 抽3张牌。 消耗。」升级只多抽 2 张
    CardDef {
        name: "祭品", cost: 0, kind: Kind::Skill, targeted: false, exhausts: true,
        cost_minus_attacks: false,
        ops: &[Op::LoseHp(6), Op::GainEnergy(2), Op::Draw(3)],
        ops_upg: &[Op::LoseHp(6), Op::GainEnergy(2), Op::Draw(5)],
        cost_upg: 0,
    },
    // 44 震荡波「给予所有敌人3层虚弱和易伤。 消耗。」
    CardDef {
        name: "震荡波", cost: 2, kind: Kind::Skill, targeted: false, exhausts: true,
        cost_minus_attacks: false,
        ops: &[
            Op::Status { tgt: Tgt::AllEnemies, st: St::Weak, amt: 3 },
            Op::Status { tgt: Tgt::AllEnemies, st: St::Vulnerable, amt: 3 },
        ],
        ops_upg: &[
            Op::Status { tgt: Tgt::AllEnemies, st: St::Weak, amt: 5 },
            Op::Status { tgt: Tgt::AllEnemies, st: St::Vulnerable, amt: 5 },
        ],
        cost_upg: 2,
    },
    // 45 时候未到「回复10点生命。 消耗。」
    CardDef {
        name: "时候未到", cost: 2, kind: Kind::Skill, targeted: false, exhausts: true,
        cost_minus_attacks: false,
        ops: &[Op::Heal(10)],
        ops_upg: &[Op::Heal(13)],
        cost_upg: 2,
    },
    // 46 全身撞击「造成你当前格挡值的伤害。」升级降到 0 费。
    // **格挡不消耗**（玩家确认）。
    CardDef {
        name: "全身撞击", cost: 1, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Damage { base: 0, hits: 1, scale: Scale::PlayerBlock }],
        ops_upg: &[Op::Damage { base: 0, hits: 1, scale: Scale::PlayerBlock }],
        cost_upg: 0,
    },
    // 47 拳斗「造成7点伤害。 获得等量于所造成伤害的格挡。」
    // "所造成伤害"是**过完乘区**的数字（玩家确认：带易伤时 7 -> 10，给 10 点格挡）
    CardDef {
        name: "拳斗", cost: 1, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[
            Op::Damage { base: 7, hits: 1, scale: Scale::None },
            Op::BlockEqualToLastDamage,
        ],
        ops_upg: &[
            Op::Damage { base: 9, hits: 1, scale: Scale::None },
            Op::BlockEqualToLastDamage,
        ],
        cost_upg: 1,
    },
    // 48 岩石铠甲「获得4层覆甲。」覆甲的规则见 `POWERS`（两条）
    CardDef {
        name: "岩石铠甲", cost: 1, kind: Kind::Power, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Status { tgt: Tgt::Me, st: St::PlatedArmor, amt: 4 }],
        ops_upg: &[Op::Status { tgt: Tgt::Me, st: St::PlatedArmor, amt: 6 }],
        cost_upg: 1,
    },
    // 49 熔融之拳「造成10点伤害。 将该敌人身上的易伤层数翻倍。 消耗。」
    // 翻倍在伤害之后（照卡面顺序）。对这张牌自己的伤害没影响 ——
    // 易伤是"有/无"的二值乘区，层数不改倍率（玩家指出）。
    CardDef {
        name: "熔融之拳", cost: 1, kind: Kind::Attack, targeted: true, exhausts: true,
        cost_minus_attacks: false,
        ops: &[Op::Damage { base: 10, hits: 1, scale: Scale::None }, Op::DoubleTargetVuln],
        ops_upg: &[Op::Damage { base: 14, hits: 1, scale: Scale::None }, Op::DoubleTargetVuln],
        cost_upg: 1,
    },
    // 50 万向斩「造成8点伤害。 对所有其他敌人造成等量的伤害。」
    // "等量" = 对主目标实际打出的数字（玩家确认），其他敌人不再各自过乘区
    CardDef {
        name: "万向斩", cost: 0, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[
            Op::Damage { base: 8, hits: 1, scale: Scale::None },
            Op::DamageOthersEqualToLast,
        ],
        ops_upg: &[
            Op::Damage { base: 11, hits: 1, scale: Scale::None },
            Op::DamageOthersEqualToLast,
        ],
        cost_upg: 0,
    },
    // 51 黑暗镣铐「使一名敌人在本回合失去9点力量。 消耗。」
    // "本回合"用 `TempStrength` 表达：存 -9，回合结束时 `strip_temp_strength`
    // 会把它加回来。和预备打击共用同一条路，只是方向相反。
    CardDef {
        name: "黑暗镣铐", cost: 0, kind: Kind::Skill, targeted: true, exhausts: true,
        cost_minus_attacks: false,
        ops: &[
            Op::Status { tgt: Tgt::Enemy, st: St::Strength, amt: -9 },
            Op::Status { tgt: Tgt::Enemy, st: St::TempStrength, amt: -9 },
        ],
        ops_upg: &[
            Op::Status { tgt: Tgt::Enemy, st: St::Strength, amt: -15 },
            Op::Status { tgt: Tgt::Enemy, st: St::TempStrength, amt: -15 },
        ],
        cost_upg: 0,
    },
    // 52 凌虐「造成15点伤害。 敌人在本回合失去10点力量。」
    CardDef {
        name: "凌虐", cost: 3, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[
            Op::Damage { base: 15, hits: 1, scale: Scale::None },
            Op::Status { tgt: Tgt::Enemy, st: St::Strength, amt: -10 },
            Op::Status { tgt: Tgt::Enemy, st: St::TempStrength, amt: -10 },
        ],
        ops_upg: &[
            Op::Damage { base: 20, hits: 1, scale: Scale::None },
            Op::Status { tgt: Tgt::Enemy, st: St::Strength, amt: -15 },
            Op::Status { tgt: Tgt::Enemy, st: St::TempStrength, amt: -15 },
        ],
        cost_upg: 3,
    },

    // ---- 53-61 触发式的最后一批 + 手牌垃圾牌 ----

    // 53 狂怒「打出此牌后，你在这个回合内每打出一张攻击牌，获得3点格挡。」
    CardDef {
        name: "狂怒", cost: 0, kind: Kind::Skill, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Status { tgt: Tgt::Me, st: St::Frenzy, amt: 3 }],
        ops_upg: &[Op::Status { tgt: Tgt::Me, st: St::Frenzy, amt: 5 }],
        cost_upg: 0,
    },
    // 54 火焰屏障「获得12点格挡。 你在这个回合每受到一次攻击，都会对攻击者造成4点伤害。」
    CardDef {
        name: "火焰屏障", cost: 2, kind: Kind::Skill, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Block { base: 12 }, Op::Status { tgt: Tgt::Me, st: St::FlameBarrier, amt: 4 }],
        ops_upg: &[Op::Block { base: 16 }, Op::Status { tgt: Tgt::Me, st: St::FlameBarrier, amt: 6 }],
        cost_upg: 2,
    },
    // 55-61 手牌垃圾牌。效果全在 `HAND_END` 表里，`ops` 为空 =>
    // `playable()` 判定为不可打出，正好对上卡面的"不能被打出"。
    CardDef {
        name: "灼伤", cost: 0, kind: Kind::Status, targeted: false, exhausts: false,
        cost_minus_attacks: false, ops: NO_OPS, ops_upg: NO_OPS, cost_upg: 0,
    },
    CardDef {
        name: "感染", cost: 0, kind: Kind::Status, targeted: false, exhausts: false,
        cost_minus_attacks: false, ops: NO_OPS, ops_upg: NO_OPS, cost_upg: 0,
    },
    CardDef {
        name: "腐朽", cost: 0, kind: Kind::Curse, targeted: false, exhausts: false,
        cost_minus_attacks: false, ops: NO_OPS, ops_upg: NO_OPS, cost_upg: 0,
    },
    CardDef {
        name: "羞耻", cost: 0, kind: Kind::Curse, targeted: false, exhausts: false,
        cost_minus_attacks: false, ops: NO_OPS, ops_upg: NO_OPS, cost_upg: 0,
    },
    CardDef {
        name: "瓦解", cost: 0, kind: Kind::Status, targeted: false, exhausts: false,
        cost_minus_attacks: false, ops: NO_OPS, ops_upg: NO_OPS, cost_upg: 0,
    },
    CardDef {
        name: "晕眩", cost: 0, kind: Kind::Status, targeted: false, exhausts: false,
        cost_minus_attacks: false, ops: NO_OPS, ops_upg: NO_OPS, cost_upg: 0,
    },
    CardDef {
        name: "笨拙", cost: 0, kind: Kind::Curse, targeted: false, exhausts: false,
        cost_minus_attacks: false, ops: NO_OPS, ops_upg: NO_OPS, cost_upg: 0,
    },

    // ---- 62-65 条件牌。读的全是 `State` 里早就有的每回合计数器 ----
    // 旧的 Python 模拟器**拒绝给这类牌评分**，理由就是"没地方放这些计数"。

    // 62 邪眼「获得8点格挡。 如果你在本回合消耗过卡牌，则额外获得8点格挡。」
    CardDef {
        name: "邪眼", cost: 1, kind: Kind::Skill, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[
            Op::Block { base: 8 },
            Op::Conditional { cond: Cond::ExhaustedThisTurn, then: &[Op::Block { base: 8 }] },
        ],
        ops_upg: &[
            Op::Block { base: 11 },
            Op::Conditional { cond: Cond::ExhaustedThisTurn, then: &[Op::Block { base: 11 }] },
        ],
        cost_upg: 1,
    },
    // 63 被遗忘的仪式「如果你在本回合消耗过卡牌，则获得3能量。 消耗。」
    // 它自己的消耗发生在结算之后，所以不会自己满足自己的条件。
    CardDef {
        name: "被遗忘的仪式", cost: 1, kind: Kind::Skill, targeted: false, exhausts: true,
        cost_minus_attacks: false,
        ops: &[Op::Conditional { cond: Cond::ExhaustedThisTurn, then: &[Op::GainEnergy(3)] }],
        ops_upg: &[Op::Conditional { cond: Cond::ExhaustedThisTurn, then: &[Op::GainEnergy(4)] }],
        cost_upg: 1,
    },
    // 64 怨恨「造成5点伤害。 如果你在本回合失去过生命值，则攻击2次。」
    // "攻击2次"是**总共**2次，所以条件成立时再补 1 下；升级版补 2 下。
    CardDef {
        name: "怨恨", cost: 0, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[
            Op::Damage { base: 5, hits: 1, scale: Scale::None },
            Op::Conditional {
                cond: Cond::LostHpThisTurn,
                then: &[Op::Damage { base: 5, hits: 1, scale: Scale::None }],
            },
        ],
        ops_upg: &[
            Op::Damage { base: 5, hits: 1, scale: Scale::None },
            Op::Conditional {
                cond: Cond::LostHpThisTurn,
                then: &[Op::Damage { base: 5, hits: 2, scale: Scale::None }],
            },
        ],
        cost_upg: 0,
    },
    // 65 急躁「如果你的手牌中没有攻击牌，抽2张牌。」
    CardDef {
        name: "急躁", cost: 0, kind: Kind::Skill, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Conditional { cond: Cond::NoAttackInHand, then: &[Op::Draw(2)] }],
        ops_upg: &[Op::Conditional { cond: Cond::NoAttackInHand, then: &[Op::Draw(3)] }],
        cost_upg: 0,
    },
    // 66 完美打击「造成6点伤害。 你每有一张名字中含有"打击"的牌，伤害+2。」
    // 数的是整个牌组（玩家确认）。注意它自己也含"打击"，会数到自己。
    CardDef {
        name: "完美打击", cost: 2, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Damage { base: 6, hits: 1, scale: Scale::PerStrikeCard(2) }],
        ops_upg: &[Op::Damage { base: 6, hits: 1, scale: Scale::PerStrikeCard(3) }],
        cost_upg: 2,
    },
    // 67 暴走「造成9点伤害。 将这张牌在本场战斗中的伤害增加5。」
    // 变强的是**这一张牌实例**（写进 `CardInst.bonus`），不是同名的所有牌。
    CardDef {
        name: "暴走", cost: 1, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Damage { base: 9, hits: 1, scale: Scale::None }, Op::GrowThisCard(5)],
        ops_upg: &[Op::Damage { base: 9, hits: 1, scale: Scale::None }, Op::GrowThisCard(9)],
        cost_upg: 1,
    },

    // ---- 68-70 规则修饰牌。消费点是 `step.rs` 里三个窄 `if`，不是 POWERS ----

    // 68 壁垒「格挡不再在你的回合开始时消失。」
    CardDef {
        name: "壁垒", cost: 3, kind: Kind::Power, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Status { tgt: Tgt::Me, st: St::Barricade, amt: 1 }],
        ops_upg: &[Op::Status { tgt: Tgt::Me, st: St::Barricade, amt: 1 }],
        cost_upg: 2,
    },
    // 69 均衡「获得13点格挡。 在本回合保留你的手牌。」
    CardDef {
        name: "均衡", cost: 2, kind: Kind::Skill, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Block { base: 13 }, Op::Status { tgt: Tgt::Me, st: St::Entrench, amt: 1 }],
        ops_upg: &[Op::Block { base: 16 }, Op::Status { tgt: Tgt::Me, st: St::Entrench, amt: 1 }],
        cost_upg: 2,
    },
    // 70 孤注一掷「获得50点格挡。 如果你在本场战斗中受到未被格挡的攻击伤害，
    // 则立刻死亡。」—— 那个死亡条件必须建模，否则内核会把它当成白送 50 格挡。
    CardDef {
        name: "孤注一掷", cost: 0, kind: Kind::Skill, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Block { base: 50 }, Op::Status { tgt: Tgt::Me, st: St::AllOrNothing, amt: 1 }],
        ops_upg: &[Op::Block { base: 75 }, Op::Status { tgt: Tgt::Me, st: St::AllOrNothing, amt: 1 }],
        cost_upg: 0,
    },
    // ---------------------------------------------------------------------
    // 71-75 第一批铁甲补牌（2026-08-19）。数值全部来自**反编译源码**
    // （`decompiled/MegaCrit.Sts2.Core.Models.Cards/*.cs`），档次 `[源码]` ——
    // 比 wiki 高一档（它就是游戏的代码），但**仍然低于 `[实测]`**：
    // 那份反编译没有版本串，而拿它和实时游戏导出的权威卡表对了 105 张，
    // 实质一致 104 张、`巨像` 的稀有度有一处真冲突（源码 Rare / 游戏 Uncommon），
    // 说明它**不是当前线上版本**。冲突时以游戏为准。
    // 这五张一张都没在实战里打出去过，第一次打出时对拍会当场判它们。
    // ---------------------------------------------------------------------

    // 71 [源码] 飞剑回旋镖「造成3点伤害，随机3次。」升级：4 次（伤害不变）
    CardDef {
        name: "飞剑回旋镖",
        cost: 1,
        kind: Kind::Attack,
        // TargetType.RandomEnemy —— **不指定目标**，所以 targeted = false。
        // 这一列以前是从卡面文本推的，现在有权威的 `TargetType` 可对（69/69 全中）。
        targeted: false,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::DamageRandom { base: 3, hits: 3, scale: Scale::None }],
        ops_upg: &[Op::DamageRandom { base: 3, hits: 4, scale: Scale::None }],
        cost_upg: 1,
    },
    // 72 [源码] 狂宴「造成10点伤害。若此牌杀死了敌人，永久提升3点最大生命。消耗。」
    // 升级：伤害 12、最大生命 4（`OnUpgrade` 两个值都涨）
    CardDef {
        name: "狂宴",
        cost: 1,
        kind: Kind::Attack,
        targeted: true,
        exhausts: true,
        cost_minus_attacks: false,
        ops: &[
            Op::Damage { base: 10, hits: 1, scale: Scale::None },
            Op::Conditional { cond: Cond::LastAttackKilled, then: &[Op::GainMaxHp(3)] },
        ],
        ops_upg: &[
            Op::Damage { base: 12, hits: 1, scale: Scale::None },
            Op::Conditional { cond: Cond::LastAttackKilled, then: &[Op::GainMaxHp(4)] },
        ],
        cost_upg: 1,
    },
    // 73 [游戏+源码] 跃跃欲试「你的手牌中每有一张攻击牌，就获得1点能量。
    //     **你在本回合内不能再获得能量。**」升级：费用 2 -> 1
    //
    // **第二句 2026-09-06 才补上**，在此之前内核只建了第一句 —— 方向是**乐观**：
    // 求解器以为可以先跃跃欲试拿 3 点，再被遗忘的仪式拿 3 点，而游戏给 0。
    // 抓到它的是 `act3_f46_elite_soul_nexus`（录完 3 分钟就跑了验收）报的
    // 「没映射的 status: NO_ENERGY_GAIN_POWER×5」—— **那一栏不是红，是静默的洞**：
    // 没映射的字段根本不参与比较。顺着它翻源码才看见 `Apply<NoEnergyGainPower>`。
    //
    // **两条 op 的顺序就是规则**：先拿能量再上禁令（[源码] `OnPlay` 里
    // `GainEnergy(...)` 在 `Apply<NoEnergyGainPower>` 之前）。反过来写这张牌给 0 能量。
    CardDef {
        name: "跃跃欲试",
        cost: 2,
        kind: Kind::Skill,
        targeted: false,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[
            Op::EnergyPerAttackInHand { per: 1 },
            Op::Status { tgt: Tgt::Me, st: St::NoEnergyGain, amt: 1 },
        ],
        ops_upg: &[
            Op::EnergyPerAttackInHand { per: 1 },
            Op::Status { tgt: Tgt::Me, st: St::NoEnergyGain, amt: 1 },
        ],
        cost_upg: 1,
    },
    // 74 [源码] 势不可当「每当你获得格挡时，对随机敌人造成5点伤害。」升级：7
    CardDef {
        name: "势不可当",
        cost: 2,
        kind: Kind::Power,
        targeted: false,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Status { tgt: Tgt::Me, st: St::Juggernaut, amt: 5 }],
        ops_upg: &[Op::Status { tgt: Tgt::Me, st: St::Juggernaut, amt: 7 }],
        cost_upg: 2,
    },
    // 75 [源码] 巨像「获得5点格挡。本回合内，攻击你的带易伤的敌人伤害减半。」升级：格挡 8
    //
    // **稀有度那处版本冲突就是这张牌**（源码 Rare / 实时游戏 Uncommon）。
    // 稀有度不进内容表、也不影响战斗层，记在这里是为了别忘了它。
    CardDef {
        name: "巨像",
        cost: 1,
        kind: Kind::Skill,
        targeted: false,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Block { base: 5 }, Op::Status { tgt: Tgt::Me, st: St::Colossus, amt: 1 }],
        ops_upg: &[Op::Block { base: 8 }, Op::Status { tgt: Tgt::Me, st: St::Colossus, amt: 1 }],
        cost_upg: 1,
    },

    // ---------------------------------------------------------------------
    // 76-81 第二批铁甲补牌（2026-08-19）。
    // **数值取自实时游戏的权威卡表**（`cards_catalog.json`），
    // **语义取自反编译源码**（哪个先哪个后、复制的是实例还是牌名这类
    // 卡面文本读不出来的东西）。两个来源冲突时以游戏为准 —— 添柴就是
    // 这么被拦下来的，见 roadmap。
    // ---------------------------------------------------------------------

    // 76 [源码+游戏] 重振精神「消耗手牌中所有非攻击牌，每张获得5点格挡。」升级 7
    CardDef {
        name: "重振精神", cost: 1, kind: Kind::Skill, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::ExhaustNonAttacksForBlock { per: 5 }],
        ops_upg: &[Op::ExhaustNonAttacksForBlock { per: 7 }],
        cost_upg: 1,
    },
    // 77 [源码+游戏] 战斗专注「抽3张牌。你在本回合内不能再抽任何牌。」升级 抽4
    //
    // 顺序要紧：**先抽再上 NoDraw**。反过来写这张牌就一张都抽不到 ——
    // 这是那种"写反了也能编译、还很难在单元测试外发现"的顺序错误。
    CardDef {
        name: "战斗专注", cost: 0, kind: Kind::Skill, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Draw(3), Op::Status { tgt: Tgt::Me, st: St::NoDraw, amt: 1 }],
        ops_upg: &[Op::Draw(4), Op::Status { tgt: Tgt::Me, st: St::NoDraw, amt: 1 }],
        cost_upg: 0,
    },
    // 78 [源码+游戏] 燃烧契约「消耗1张牌。抽2张牌。」升级 抽3
    //
    // **已知偏差**：`ExhaustChoose` 走 `Pending`，而抽牌在同一串 ops 里紧跟其后，
    // 于是内核是"先抽后选消耗"，真实游戏是"先选完再抽"。这和烙印那条已知偏差
    // （「烙印的力量是立即结算的，真实游戏是选完牌之后」）是同一个成因，
    // 记在「故意没做」清单里。张数不受影响，只有"抽上来的牌能不能被这次消耗选中"
    // 这一种边角情形会差。
    CardDef {
        name: "燃烧契约", cost: 1, kind: Kind::Skill, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::ExhaustChoose(1), Op::Draw(2)],
        ops_upg: &[Op::ExhaustChoose(1), Op::Draw(3)],
        cost_upg: 1,
    },
    // 79 [源码+游戏] 武装「获得5点格挡。升级你手牌中的一张牌。」升级：升级**所有**
    // 格挡值升级前后都是 5（`OnUpgrade` 只改了升级范围，没改格挡）——
    // 这条容易想当然写成"升级也涨格挡"。
    CardDef {
        name: "武装", cost: 1, kind: Kind::Skill, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Block { base: 5 }, Op::UpgradeInHand(1)],
        ops_upg: &[Op::Block { base: 5 }, Op::UpgradeAllInHand],
        cost_upg: 1,
    },
    // 80 [源码+游戏] 头槌「造成9点伤害。将你弃牌堆中的一张牌放到抽牌堆顶部。」升级 12
    CardDef {
        name: "头槌", cost: 1, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[
            Op::Damage { base: 9, hits: 1, scale: Scale::None },
            Op::DiscardToTopOfDraw(1),
        ],
        ops_upg: &[
            Op::Damage { base: 12, hits: 1, scale: Scale::None },
            Op::DiscardToTopOfDraw(1),
        ],
        cost_upg: 1,
    },
    // 81 [源码+游戏] 愤怒「造成6点伤害。将一张此牌的复制品加入你的弃牌堆。」升级 8
    //
    // 复制的是**这一张实例**（[源码] `CreateClone()`）—— 升级过的愤怒
    // 复制出来也是升级的，于是它会自我繁殖成一手升级牌。卡面文本读不出这条。
    CardDef {
        name: "愤怒", cost: 0, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[
            Op::Damage { base: 6, hits: 1, scale: Scale::None },
            Op::AddCopyOfThisToDiscard,
        ],
        ops_upg: &[
            Op::Damage { base: 8, hits: 1, scale: Scale::None },
            Op::AddCopyOfThisToDiscard,
        ],
        cost_upg: 0,
    },

    // 82 [游戏] 添柴「消耗所有手牌。每消耗一张牌，将1张随机牌加入你的手牌。」
    // 升级：加入的是**已升级**的牌（费用不变，仍是 1）
    //
    // **这张牌的两个来源冲突过**，按规矩以游戏为准：
    //   反编译 [源码]  `Draw(cardCount)`  —— 抽同样多的牌
    //   实时游戏卡表    「将1张随机牌加入你的手牌」—— 生成
    // 说明那份反编译不是当前线上版本（和巨像的稀有度冲突是同一个证据）。
    // 玩家也确认了是**生成**不是抽牌，并给了顺序判定：先消耗、再生成。
    //
    // 生成 ≠ 抽牌：抽牌是黑暗之拥那条规则（`TOp::OwnerDraw`）。
    CardDef {
    // **它自己不消耗**：[源码] `Stoke` 没有 `Exhaust` 关键字，`OnPlay` 消耗的是
    // **手牌堆**（此时添柴已经离开手牌，所以烧不到自己），打完照常进弃牌堆。
    // 内核原来写的 `exhausts: true` 是错的，2026-08-22 第2幕第20层第一次
    // 打出添柴+ 当场被对拍抓到（消耗堆多一张、弃牌堆少一张）。
        name: "添柴", cost: 1, kind: Kind::Skill, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::ExhaustHandGenerate { upgraded: false }],
        ops_upg: &[Op::ExhaustHandGenerate { upgraded: true }],
        cost_upg: 1,
    },
    // 83 [游戏+源码] 地狱之刃「将一张随机攻击牌加入你的手牌。那张牌在本回合内
    // 可以免费打出。消耗。」升级：费用 1 -> 0
    CardDef {
        name: "地狱之刃", cost: 1, kind: Kind::Skill, targeted: false, exhausts: true,
        cost_minus_attacks: false,
        ops: &[Op::GenerateFree(Kind::Attack)],
        ops_upg: &[Op::GenerateFree(Kind::Attack)],
        cost_upg: 0,
    },

    // ---------------------------------------------------------------------
    // 84-88 「牌打牌」那一批（2026-08-19）。数值取自实时游戏卡表，
    // 语义取自 [源码]。共用的原语是 `step::auto_play_card` ——
    // 玩家判定：**自动打出的牌不扣能量，但计入**（`cards_played` /
    // `attacks_played` 照加）。「计入」不是另写的一条，是它和 `play_card`
    // 共用 `resolve_played_card` 天然得到的。
    //
    // **三张能力牌的「固有」升级效果内核没做**（`F_INNATE` 的位有了、
    // `step` 还没用），所以杂耍+/好勇斗狠+ 目前和基础版行为相同。
    // 这在「切片里故意没做」清单上，不是漏写。
    // ---------------------------------------------------------------------

    // 84 [游戏+源码] 破灭「打出抽牌堆顶部的牌并将其消耗。」升级：费用 1 -> 0
    CardDef {
        name: "破灭", cost: 1, kind: Kind::Skill, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::AutoPlayFromDrawTop { count: 1, force_exhaust: true }],
        ops_upg: &[Op::AutoPlayFromDrawTop { count: 1, force_exhaust: true }],
        cost_upg: 0,
    },
    // 85 [游戏+源码] 惊逃「在你的回合结束时，随机打出你手牌中的1张攻击牌攻击
    // 随机敌人。」升级：费用 2 -> 1（**张数不变**，升级只降费）
    CardDef {
        name: "惊逃", cost: 2, kind: Kind::Power, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Status { tgt: Tgt::Me, st: St::Stampede, amt: 1 }],
        ops_upg: &[Op::Status { tgt: Tgt::Me, st: St::Stampede, amt: 1 }],
        cost_upg: 1,
    },
    // 86 [游戏+源码] 连环拳「在这个回合，你打出的下1张攻击牌会被额外打出一次。」
    // 升级：下 2 张
    //
    // 层数 = **还剩几张攻击牌会被额外打一次**，每张攻击牌消耗一层
    // （[源码] `AfterModifyingCardPlayCount` 里立刻 `Decrement`）。
    // 消费点在 `step::resolve_played_card`，不在 `POWERS` —— 它改的是
    // 「这张牌打几次」，属于规则修饰，不是"在某个时点多做一件事"。
    CardDef {
        name: "连环拳", cost: 1, kind: Kind::Skill, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Status { tgt: Tgt::Me, st: St::OneTwoPunch, amt: 1 }],
        ops_upg: &[Op::Status { tgt: Tgt::Me, st: St::OneTwoPunch, amt: 2 }],
        cost_upg: 1,
    },
    // 87 [游戏+源码] 杂耍「将你在每回合打出的第三张攻击牌的复制品加入你的手牌。」
    // 升级：固有（内核没做，见上面那段说明）
    CardDef {
        name: "杂耍", cost: 1, kind: Kind::Power, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Status { tgt: Tgt::Me, st: St::Juggling, amt: 1 }],
        ops_upg: &[Op::Status { tgt: Tgt::Me, st: St::Juggling, amt: 1 }],
        cost_upg: 1,
    },
    // 88 [游戏+源码] 好勇斗狠「在你的回合开始时，将你弃牌堆的一张随机攻击牌
    // 放入你的手牌并将其升级。」升级：固有（内核没做）
    CardDef {
        name: "好勇斗狠", cost: 1, kind: Kind::Power, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Status { tgt: Tgt::Me, st: St::Aggression, amt: 1 }],
        ops_upg: &[Op::Status { tgt: Tgt::Me, st: St::Aggression, amt: 1 }],
        cost_upg: 1,
    },

    // ---------------------------------------------------------------------
    // 89-95 最后 7 张（2026-08-19）。数值取自实时游戏卡表，语义取自 [源码]。
    //
    // **余烬的数值两个来源对不上**：源码 17（升级 +5 = 22），
    // 实时游戏 18 / 24。按规矩以游戏为准 —— 这是那份反编译不是当前线上版本的
    // 第三个证据（前两个：巨像的稀有度、添柴的效果）。
    // ---------------------------------------------------------------------

    // 89 [游戏+源码] 余烬「造成18点伤害。随机消耗1张牌。」升级：24
    CardDef {
        name: "余烬", cost: 2, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[
            Op::Damage { base: 18, hits: 1, scale: Scale::None },
            Op::ExhaustRandomFromHand(1),
        ],
        ops_upg: &[
            Op::Damage { base: 24, hits: 1, scale: Scale::None },
            Op::ExhaustRandomFromHand(1),
        ],
        cost_upg: 2,
    },
    // 90 [游戏+源码] 坚毅「获得7点格挡。**随机**消耗1张牌。」
    // 升级：格挡 9，且改成**你自己选**一张消耗
    //
    // 基础版随机、升级版你挑 —— 这条卡面文本读不出来（两版都只写"消耗1张牌"），
    // 是 [源码] `if (IsUpgraded) { CardSelectCmd... } else { Rng...NextItem }` 定的。
    // 两者估值差很远：能挑的时候会挑垃圾牌。
    CardDef {
        name: "坚毅", cost: 1, kind: Kind::Skill, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Block { base: 7 }, Op::ExhaustRandomFromHand(1)],
        ops_upg: &[Op::Block { base: 9 }, Op::ExhaustChoose(1)],
        cost_upg: 1,
    },
    // 91 [游戏+源码] 痛殴「造成4点伤害两次。消耗你手牌中随机一张攻击牌，
    // 并将它的伤害添加给这张牌。」升级：6点两次
    //
    // 「添加给这张牌」是**永久**的，写 `CardInst.bonus`（和暴走同一个机制），
    // 所以是这一张实例变强，不是这个牌名变强。
    CardDef {
        name: "痛殴", cost: 1, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[
            Op::Damage { base: 4, hits: 2, scale: Scale::None },
            Op::ExhaustRandomAttackAddDamage,
        ],
        ops_upg: &[
            Op::Damage { base: 6, hits: 2, scale: Scale::None },
            Op::ExhaustRandomAttackAddDamage,
        ],
        cost_upg: 1,
    },
    // 92 [游戏+源码] 劫掠「造成6点伤害。抽牌直到你抽到一张非攻击牌。」升级：9
    CardDef {
        name: "劫掠", cost: 1, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[
            Op::Damage { base: 6, hits: 1, scale: Scale::None },
            Op::DrawUntilNonAttack,
        ],
        ops_upg: &[
            Op::Damage { base: 9, hits: 1, scale: Scale::None },
            Op::DrawUntilNonAttack,
        ],
        cost_upg: 1,
    },
    // 93 [游戏+源码] 原始力量「将手牌中的所有攻击牌变化为巨石。」升级：巨石+
    //
    // **变形不是"消耗再生成"**：牌实例还是那一张，只是身份换了。
    // 写成消耗+生成会触发无惧疼痛/黑暗之拥，那是错的。
    CardDef {
        name: "原始力量", cost: 0, kind: Kind::Skill, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::TransformAttacksInHand { into: card::BOULDER, upgraded: false }],
        ops_upg: &[Op::TransformAttacksInHand { into: card::BOULDER, upgraded: true }],
        cost_upg: 0,
    },
    // 94 [游戏+源码] 旋风斩「对所有敌人造成5点伤害X次。」升级：8点
    // X 费，见 `X_COST_CARDS`
    CardDef {
        name: "旋风斩", cost: 0, kind: Kind::Attack, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::DamageAllXTimes { base: 5 }],
        ops_upg: &[Op::DamageAllXTimes { base: 8 }],
        cost_upg: 0,
    },
    // 95 [游戏+源码] 倾泻「打出你抽牌堆顶部的X张牌。」升级：X+1
    // X 费。[源码] `forceExhaust: false` —— 和破灭不一样，打完照常进弃牌堆。
    CardDef {
        name: "倾泻", cost: 0, kind: Kind::Skill, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::AutoPlayFromDrawTopX { plus: 0, force_exhaust: false }],
        ops_upg: &[Op::AutoPlayFromDrawTopX { plus: 1, force_exhaust: false }],
        cost_upg: 0,
    },
    // 96 [源码+实测] 孢子心灵「消耗。」1 费诅咒牌。
    // [源码] `SporeMind`: `base(1, CardType.Curse, CardRarity.Curse, TargetType.None)`,
    // `CanonicalKeywords = { Exhaust }`, `MaxUpgradeLevel => 0`,
    // **一个 `OnPlay` 都没有** —— 所以它就是"花 1 费把自己烧掉"，别无效果。
    // `CanBeGeneratedByModifiers => false`，所以不进 `GEN_POOL`。
    // [实测] 第2幕第19层帧9：能量 3→2、消耗堆 0→1、血量/格挡/敌人全零变化。
    // 它是 `UNPLAYABLE_CARDS` 那张表的**存在理由**，见那里。
    CardDef {
        name: "孢子心灵", cost: 1, kind: Kind::Curse, targeted: false, exhausts: true,
        cost_minus_attacks: false,
        ops: NO_OPS,
        ops_upg: NO_OPS,
        cost_upg: 1,
    },
    // 97 [源码] 藏宝图「不能被打出。 在下一阶段的地图上，标记一个有600额外金币的地点。」
    // [源码] `SpoilsMap`: `base(-1, CardType.Quest, CardRarity.Quest, TargetType.Self)`,
    // `CanonicalKeywords = { Unplayable }`, `MaxUpgradeLevel => 0`。
    // 效果**全在局外**（`ModifyGeneratedMap` / `AfterMapGenerated` 改地图，
    // `OnQuestComplete` 给 600 金并把自己移出牌组），战斗里它就是一张废牌。
    // 建它不是为了效果，是为了**别再落进 `card::UNKNOWN`** —— 落进去
    // `solve --live` 每回合都要报一次"手牌里有内容表还没有的牌"，
    // 而真正该被那行点名的是内核不知道效果的牌，不是它。
    // 费用取游戏 API 的 0（源码那个 -1 是"不可打出"的编码，见 verification-log
    // 「反编译不是当前版本」一节里那 10 张状态/诅咒牌的同一处差异）。
    CardDef {
        name: "藏宝图", cost: 0, kind: Kind::Quest, targeted: false, exhausts: false,
        ops: NO_OPS,
        ops_upg: NO_OPS,
        cost_minus_attacks: false,
        cost_upg: 0,
    },
    // 98 [源码] 狂乱逃离，1 费状态牌，**第 2 幕 Boss 无厌沙虫塞进牌组的**（一次 6 张）
    //
    // [源码] `FranticEscape.OnPlay` 做两件事：
    //   1. 给场上那只带沙坑的敌人 `ModifyAmount(sandpit, +1)` —— **推迟一回合死亡**
    //   2. `base.EnergyCost.AddThisCombat(1)` —— **这一张实例**的费用永久 +1
    //
    // 第 2 条是 `CardInst::cost_delta` 的第一个消费者。**涨价是按实例算的**，
    // 所以 6 张各自第一次都只要 1 费。我一开始按"打一次全部涨价"估过，
    // 那会得出"买不起时间"的错结论 —— 实战正是靠 1 费一张连买了三个回合。
    //
    // 第 1 条 2026-09-05 之前**只建了层数、没建后果**，于是在内核眼里这是
    // "1 费给敌人加一层没人消费的 status"，求解器永远不会主动打它 ——
    // 而那一局 AI 驾驶正是这么被吞掉的。现在归零即死建全了：
    // **沙坑 == 1 的回合，这张牌是唯一的活路，单回合求解器自己就看得见**
    //（结束回合 -> 敌人回合开始 -> 减到 0 -> 死）。
    // 沙坑 ≥ 2 时它的价值仍然在单回合视角之外，由叶评估的
    // `solver::clock_value`（`Weights::clock`）计价，见那里。
    CardDef {
        name: "狂乱逃离", cost: 1, kind: Kind::Status, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[
            Op::Status { tgt: Tgt::AllEnemies, st: St::Sandpit, amt: 1 },
            Op::GrowThisCardCost(1),
        ],
        ops_upg: NO_OPS,
        cost_upg: 1,
    },
    // 99 [源码] 凋萎 `Wither`。第 3 幕 Boss 永世沙漏的状态牌。
    // `CardKeyword.Unplayable` + `HasTurnEndInHandEffect`，效果在 `HAND_END` 里。
    // 卡面费用是 -1（不可打出），内核统一按 0 存，判据走 `UNPLAYABLE_CARDS`。
    CardDef {
        name: "凋萎", cost: 0, kind: Kind::Status, targeted: false, exhausts: false,
        cost_minus_attacks: false, ops: NO_OPS, ops_upg: NO_OPS, cost_upg: 0,
    },
    // 100 至亮之焰「获得2点能量。抽2张牌。失去1点最大生命。」
    //
    // [源码] `BrightestFlame.OnPlay` 三条命令的**顺序就是这个**：
    // `GainEnergy` -> `Draw` -> `LoseMaxHp(1, isFromCard: true)`。
    // 顺序有观测后果：百年积木是被最后那条触发的，它抽的牌排在那 2 张之后。
    // 升级 [源码] 是 +1 能量 +1 张牌（**最大生命那 1 点不变**）。
    //
    // [实测] 2026-08-27 第 2 幕第 19 层：手牌 5 -> 9（-1 打出、+2 抽、
    // +3 百年积木），HP 80/80 -> 79/79。补这张之前那一帧是个真 MISMATCH：
    // 内核没触发百年积木，于是它晚了一张牌才发作。
    CardDef {
        name: "至亮之焰", cost: 0, kind: Kind::Skill, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::GainEnergy(2), Op::Draw(2), Op::LoseMaxHp(1)],
        ops_upg: &[Op::GainEnergy(3), Op::Draw(3), Op::LoseMaxHp(1)],
        cost_upg: 0,
    },
    // 101 贪婪之手「造成20点伤害。斩杀时，获得20金币。」升级 25/25。
    //
    // [源码] `HandOfGreed`：`DamageVar(20, Move)` + `DynamicVar("Gold", 20)`，
    // 2 费攻击，升级只动这两个数字（费用不变）。
    //
    // **金币那一半故意不建**：金币是局外资源，L1 里根本没有这个字段，
    // 而这张牌在战斗里的行为**完全由伤害那一半决定** ——
    // 所以这不是"建了一半"，是"另一半不属于这一层"。
    // （对比百年积木那次：那才是真的建了一半，触发路径漏了一条。）
    CardDef {
        name: "贪婪之手", cost: 2, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Damage { base: 20, hits: 1, scale: Scale::None }],
        ops_upg: &[Op::Damage { base: 25, hits: 1, scale: Scale::None }],
        cost_upg: 2,
    },
    // ---- 102-110：2026-08-30 补的一批 ----
    //
    // 触发点是这一局第 3 幕的牌组里出现了两张内核没有的牌（吹哨、坚定不移），
    // 顺手重导权威表，发现它从 106 涨到了 **128 张**（档案又发现了 22 张）。
    // 下面 9 张是**现有原语就能表达**的那些，每张都读了 [源码]，
    // 卡面和源码不一致时以源码为准（坚定不移就是一例，见它自己的注释）。
    // 剩下 18 张各自卡在一个还没有的机制上，逐条记在 docs/content.md。

    // 102 [源码] 亮剑「造成5点伤害。抽1张牌。」升级 8 点
    //     `FlashOfSteel`：`DamageCmd.Attack(5)` 然后 `CardPileCmd.Draw(1)`，
    //     升级只动伤害（`UpgradeValueBy(3)`）。
    CardDef {
        name: "亮剑", cost: 0, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Damage { base: 5, hits: 1, scale: Scale::None }, Op::Draw(1)],
        ops_upg: &[Op::Damage { base: 8, hits: 1, scale: Scale::None }, Op::Draw(1)],
        cost_upg: 0,
    },
    // 103 [源码] 啄击「造成2点伤害3次。」升级 **4 次**
    //     `Peck`：`OnUpgrade` 里是 `DynamicVars.Repeat.UpgradeValueBy(1)` ——
    //     **升级加的是段数不是伤害**。这条卡面读得出来，但很容易顺手写成 3 点×3。
    //     段数要紧：荆棘/火焰屏障按段触发，缓慢也按段累加。
    CardDef {
        name: "啄击", cost: 1, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Damage { base: 2, hits: 3, scale: Scale::None }],
        ops_upg: &[Op::Damage { base: 2, hits: 4, scale: Scale::None }],
        cost_upg: 1,
    },
    // 104 [源码] 岿然不动「获得30点格挡。消耗。」升级 40
    CardDef {
        name: "岿然不动", cost: 2, kind: Kind::Skill, targeted: false, exhausts: true,
        cost_minus_attacks: false,
        ops: &[Op::Block { base: 30 }],
        ops_upg: &[Op::Block { base: 40 }],
        cost_upg: 2,
    },
    // 105 [源码] 究极防御「获得11点格挡。」升级 15
    //     **它带 `CardTag.Defend`**（源码里专门写了注释说明这一点），
    //     所以勒紧那类"防御牌 +N 格挡"的效果吃得到它。勒紧还没建，
    //     等建的时候别忘了这条 —— 内核目前没有牌名/标签谓词。
    CardDef {
        name: "究极防御", cost: 1, kind: Kind::Skill, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Block { base: 11 }],
        ops_upg: &[Op::Block { base: 15 }],
        cost_upg: 1,
    },
    // 106 [源码] 箭雨「造成12点伤害。在本回合保留你的手牌。」升级 16
    //     保留手牌那一半挂的是 `RetainHandPower` —— **和均衡是同一个 power**
    //     （均衡的 `OnPlay` 里也是 `PowerCmd.Apply<RetainHandPower>`）。
    //     所以直接复用 `St::Entrench`，不需要新 status。
    //     这条卡面读不出来：两张牌的措辞不一样，源码才看得到是同一个。
    CardDef {
        name: "箭雨", cost: 1, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[
            Op::Damage { base: 12, hits: 1, scale: Scale::None },
            Op::Status { tgt: Tgt::Me, st: St::Entrench, amt: 1 },
        ],
        ops_upg: &[
            Op::Damage { base: 16, hits: 1, scale: Scale::None },
            Op::Status { tgt: Tgt::Me, st: St::Entrench, amt: 1 },
        ],
        cost_upg: 1,
    },
    // 107 [源码] 毒素「在你的回合结束时，如果这张牌在你的手牌中，你受到5点伤害。消耗。」
    //
    //     **它和灼伤那一组不一样：毒素是可以打出来的。**
    //     `Toxic` 没有 `OnPlay`，但也**没有** `CardKeyword.Unplayable`，
    //     只有 `Exhaust` —— 所以花 1 费打出去 = 把它消耗掉，是正经解法。
    //     这正是孢子心灵教过的那一课（不能靠"ops 空就是不可打出"推断），
    //     所以它不进 `UNPLAYABLE_CARDS`。
    //
    //     手牌里发作那一半在 `HAND_END` 表里：`Unpowered | Move` 走格挡。
    CardDef {
        name: "毒素", cost: 1, kind: Kind::Status, targeted: false, exhausts: true,
        cost_minus_attacks: false, ops: NO_OPS, ops_upg: NO_OPS, cost_upg: 1,
    },
    // 108 [源码] 贪婪「不能被打出。永恒。」
    //     `Eternal`（不能被移除）是**局外**性质，L1 不管。战斗层就是一张废牌。
    CardDef {
        name: "贪婪", cost: 0, kind: Kind::Curse, targeted: false, exhausts: false,
        cost_minus_attacks: false, ops: NO_OPS, ops_upg: NO_OPS, cost_upg: 0,
    },
    // 109 [源码] 债务「不能被打出。在你的回合结束时，如果这张牌在你的手牌中，你失去10金币。」
    //     金币是**局外资源**，和贪婪之手的金币那一半同一个判断：
    //     不属于 L1，所以战斗层它就是一张占位的废牌。**这不是"建了一半"**。
    CardDef {
        name: "债务", cost: 0, kind: Kind::Curse, targeted: false, exhausts: false,
        cost_minus_attacks: false, ops: NO_OPS, ops_upg: NO_OPS, cost_upg: 0,
    },
    // 110 [源码] 坚定不移「翻倍你每回合第一次从卡牌中获得的格挡。」升级降费到 1
    //
    //     **卡面写的和源码不一样，以源码为准**：
    //     `UnmovablePower.ModifyBlockMultiplicative` 数的是本回合此前已经
    //     获得过几次格挡（`num >= Amount` 才停），所以两张就是**前两次**都翻倍，
    //     不是"永远只有第一次"。层数语义写在 `St::Unmovable` 上。
    //
    //     实现和臂甲同形：层数放 `St::Unmovable`，本回合余额放
    //     `St::UnmovableCharge`（回合开始重置），消费点在 `damage::card_block`。
    CardDef {
        name: "坚定不移", cost: 2, kind: Kind::Power, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Status { tgt: Tgt::Me, st: St::Unmovable, amt: 1 }],
        ops_upg: &[Op::Status { tgt: Tgt::Me, st: St::Unmovable, amt: 1 }],
        cost_upg: 1,
    },
    // 111 [源码] 吹哨「造成33点伤害。击晕该敌人。消耗。」升级 44
    //
    //     遗物「坦克斯的哨子」拾起时塞进牌组的那张，2026-08-30 建。
    //     在此之前它是内容表里唯一一张**牌组里真有、内核却不认识**的牌 ——
    //     `--live` 每一帧都在报它，而求解器等于在一副少一张大牌的手上求最优。
    //
    //     **顺序要紧：先伤害后击晕**，源码的 `OnPlay` 就是这个顺序，
    //     而 `StunInternal` 对死人是空操作 ⇒ **斩杀的那一下白给一个击晕**。
    //     写反了会变成"打死了还能击晕"，那是个不存在的收益。
    //     击晕本身的三条语义在 `St::Stunned` 上。
    CardDef {
        name: "吹哨", cost: 3, kind: Kind::Attack, targeted: true, exhausts: true,
        cost_minus_attacks: false,
        ops: &[Op::Damage { base: 33, hits: 1, scale: Scale::None }, Op::StunEnemy],
        ops_upg: &[Op::Damage { base: 44, hits: 1, scale: Scale::None }, Op::StunEnemy],
        cost_upg: 3,
    },
    // 112 涅奥之怒 [游戏] 「造成10点伤害。将你弃牌堆中的至多2张牌放入你的手牌。消耗。」
    //
    // 遗物「涅奥的苦痛」拾起时塞进牌组的那张。**升级版还没见过** ——
    // 按本仓库的规矩不猜，`ops_upg` 留空 ⇒ 回落到基础版。
    //
    // 「至多 2 张」走现成的 `Op::FetchFromDiscard`（`Pending::FetchFromDiscard`），
    // 弃牌堆不够时它自己就少选几张，不用特判。
    CardDef {
        name: "涅奥之怒", cost: 1, kind: Kind::Attack, targeted: true, exhausts: true,
        cost_minus_attacks: false,
        ops: &[Op::Damage { base: 10, hits: 1, scale: Scale::None }, Op::FetchFromDiscard(2)],
        ops_upg: &[],
        cost_upg: 1,
    },
    // 113 [源码] 战鼓「抽2张牌。当这张牌被消耗时，获得2能量。」升级「获得3能量。」
    // [源码] `DrumOfBattle`：1 费 Skill，`CardPileCmd.Draw(2)`。
    // 打出自身不消耗（`exhausts: false`）。被消耗时的给能规则在 `step::exhaust_card` 里。
    CardDef {
        name: "战鼓", cost: 1, kind: Kind::Skill, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::Draw(2)],
        ops_upg: &[Op::Draw(2)],
        cost_upg: 1,
    },
    // 114 [源码] 扯碎 `TearAsunder`「造成5点伤害。在本场战斗中，你每失去过一次
    // 生命值，这张牌就额外造成一次伤害。」
    //
    // 2 费 Attack / Rare / `TargetType.AnyEnemy`；`OnUpgrade` 只 `UpgradeValueBy(2)`
    // ⇒ **升级只加伤害（5 -> 7），费用不变**。
    //
    // **段数是算出来的，不是常数**：`WithHitCount(Calculate(target))`，而
    // `Calculate = CalculationBase(0) + CalculationExtra(1) × (1 + M)` = 1 + M，
    // M = 本场战斗里我受到过几次未被完全格挡的伤害（`St` 之外的量，见
    // `State::hp_loss_hits`）。所以走 `Op::DamagePerHpLossHit`。
    //
    // **第一版照卡面写死了 `hits: 3`，那是错的** —— 游戏把算好的段数直接渲染
    // 进卡面文本（`（命中3次）`），录到 trace 里的那句话是**当时那一刻的快照**。
    // [实测] `act2_f28_decimillipede` 帧31 卡面「命中3次」，而在那之前我只有
    // 一个回合边界掉过血（55 -> 44）—— 那一手是残杀千足虫的多段攻击，
    // **两段打穿了 8 点格挡**，所以 M=2、段数 3。逐帧数掉血的**回合数**会得到
    // M=1，那是错的：数的是伤害**次数**。
    // [实测] `act1_f17_waterfall_giant` 帧63 同一张牌卡面「命中8次」。
    CardDef {
        name: "扯碎", cost: 2, kind: Kind::Attack, targeted: true, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::DamagePerHpLossHit { base: 5 }],
        ops_upg: &[Op::DamagePerHpLossHit { base: 7 }],
        cost_upg: 2,
    },
    // 115 [源码] 彼岸咆哮 `HowlFromBeyond`「对所有敌人造成16点伤害。
    // 在你的回合结束时，如果这张牌在你的消耗牌堆中，则将其打出。」
    //
    // **它自己不消耗。** 2026-09-06 建的时候写成了 `exhausts: true`，
    // 依据是权威卡表 `keywords` 里有「消耗」—— 那一栏是**描述文本里出现过的
    // 名词表**（这里的"消耗"来自「消耗**牌堆**」四个字），不是这张牌的关键字。
    // [源码] `HowlFromBeyond` 根本没有 `CanonicalKeywords`；
    // [实测] 2026-09-09 `act1_f7_sewer_clam` 帧1→2：打出去之后它进的是**弃牌堆**，
    // 消耗堆一张没动。
    //
    // > **别把卡面的关键字词表当成这张牌的关键字。** 表里另外 12 张
    // > 描述里提到"消耗"的牌恰好都写对了（`exhausts: false`），只有这一张翻了车。
    //
    // 3 费 Attack / Uncommon / `TargetType.AllEnemies`；`OnUpgrade` 是
    // `UpgradeValueBy(5)` ⇒ **升级只加伤害（16 -> 21），费用不变**。
    //
    // 第二句是这张牌的额外价值：**被消耗掉之后**，回合末从消耗堆里自己再打一次。
    // **但只有一次** —— [源码] `CardModel.GetResultPileTypeForCardPlay()` 只看
    // 这张牌自己带不带 `Exhaust`，不管从哪个堆打出来的，所以打完它进**弃牌堆**。
    // （2026-09-06 建的时候写成"每回合永远"，那是把 `exhausts: true` 那个错
    //  一路推下去的结果。）
    // 只建第一句的话内核会把这张牌看成一张 3 费的烂 AOE ——
    // 所以两句一起建，规则在 `EXHAUST_END_AUTOPLAY`（时点由 [源码] 定死：
    // `AutoPostPlay` 阶段在 `Hook.BeforeTurnEnd` **之前**，见 `CombatManager`）。
    //
    // 2026-09-06 补进来。在此之前它是「缺的 17 张」之一，同步成 `<未知牌>` ——
    // 于是 `act3_f46` 帧35 红了一帧：原始力量+ 该把它变成巨石+，而 `<未知牌>`
    // 的 `kind` 不是攻击，内核没变它。**内容缺失会以"规则错了"的样子露头。**
    CardDef {
        name: "彼岸咆哮", cost: 3, kind: Kind::Attack, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::DamageAll { base: 16, hits: 1, scale: Scale::None }],
        ops_upg: &[Op::DamageAll { base: 21, hits: 1, scale: Scale::None }],
        cost_upg: 3,
    },
    // 116 呼唤（[源码] `Beckon`）——第 1 幕 Boss 灵魂异鱼塞给我的状态牌。
    //
    // **它是能打出来的**：`Beckon()` 的基类构造是 `base(1, CardType.Status, …)`，
    // 而且**不带 `CardKeyword.Unplayable`**（眩晕那张带）。打出去什么都不发生
    // —— 但那正是它的用处：花 1 费把它从手里挪走，就不用在回合末挨那 6 点。
    // 判据和孢子心灵那次是同一条：**不可打出看关键字，不看有没有 ops**。
    CardDef {
        name: "呼唤", cost: 1, kind: Kind::Status, targeted: false, exhausts: false,
        cost_minus_attacks: false,
        ops: NO_OPS,
        ops_upg: NO_OPS,
        cost_upg: 1,
    },
];

/// **回合结束时，如果这张牌在消耗堆里，就把它自己打出来。**
///
/// [源码] `HowlFromBeyond.AfterAutoPostPlayPhaseEntered` 判 `Pile.Type == Exhaust`。
/// 时点由 [源码] `CombatManager.EndPlayerTurnPhaseOneInternal` 定死：
/// `AutoPostPlay` 阶段跑在 `Hook.BeforeTurnEnd`（内核的 `Hook::TurnEnd`）
/// **之前**，也在弃手牌之前 —— 所以消费点在 `step::end_turn_impl` 的最前面。
///
/// 打完它自带消耗、回到消耗堆，于是**每个回合都发作一次**。
///
/// 和 `HAND_END` 是姊妹表：那张管"回合末还在手上才发生"，这张管消耗堆。
/// 今天各只有一张牌在用。
pub static EXHAUST_END_AUTOPLAY: &[u16] = &[card::HOWL_FROM_BEYOND];

// ---------------- powers（触发式能力）----------------
//
// 这张表是「加一张能力牌 = 加一行表」的兑现处。`step.rs` 里不再有任何
// 按 status 名字写的特判 —— 恶魔形态和激怒原来的两处硬编码都收编到这里了。
//
// 层数 = 每次触发的效果值。所以 `ops_upg` 的差别通常只是 `Op::Status` 的 amt 变大，
// 不需要在这张表里再开一行。
//
// **数值全部来自权威卡表 `traces/cards_catalog.json`（GET /api/v1/wiki），
// 除恶魔形态外都还没在实战里对拍过。**
pub static POWERS: &[PowerDef] = &[
    // ---- 遗物：第 4 期的六件。数值和条件全部取自 [源码]，**不是卡面** ----
    //
    // 三处卡面读不出来、源码里才有的：
    //   灯笼      `TurnNumber <= 1`，不是 `== 1`
    //   精致折扇  `% 3 == 0`，即第 3/6/9… 张都给，不是只有第 3 张
    //   摆动球    相位跨战斗保留（`[SavedProperty]`），不是每场从 0 数
    PowerDef {
        st: St::OrnamentalFan,
        hook: Hook::PlayerAttack,
        ops: &[TOp::If {
            cond: TCond::EveryNthAttackThisTurn(3),
            // 格挡走 `OwnerBlock`（不过 `card_block`）—— [源码] 是
            // `GainBlock(..., null)`，没有牌来源，所以不吃脆弱、也不吃臂甲翻倍
            then: &[TOp::OwnerBlock(Amt::Stacks)],
        }],
    },
    PowerDef {
        st: St::MercuryHourglass,
        hook: Hook::TurnStart,
        // [源码] `ValueProp.Unpowered` —— 不吃力量。和滚石同一条路。
        ops: &[TOp::DamageAllEnemies(Amt::Stacks)],
    },
    PowerDef {
        st: St::Lantern,
        hook: Hook::TurnStart,
        ops: &[TOp::If { cond: TCond::TurnAtMost(1), then: &[TOp::OwnerEnergy(Amt::Stacks)] }],
    },
    // 风箱 / 骨茶：[源码] 两件都是 `TurnNumber <= 1` 时 `CardCmd.Upgrade(手牌)`。
    // **钩子是 `HandDrawn` 不是 `TurnStart`** —— 源码那两个方法是
    // `AfterPlayerTurnStart`，跑在抽牌之后；挂 `TurnStart` 会去升级一手空牌。
    PowerDef {
        st: St::UpgradeOpeningHand,
        hook: Hook::HandDrawn,
        ops: &[TOp::If { cond: TCond::TurnAtMost(1), then: &[TOp::UpgradeHand] }],
    },
    // 宝石面具：[源码] `JeweledMask.BeforeHandDraw`，`TurnNumber <= 1` ——
    // **抽牌之前**，所以挂 `TurnStart`（内核的 `TurnStart` 就在抽牌之前）。
    PowerDef {
        st: St::JeweledMask,
        hook: Hook::TurnStart,
        ops: &[TOp::If {
            cond: TCond::TurnAtMost(1),
            then: &[TOp::MoveRandomPowerFromDrawToHandFree],
        }],
    },
    // 碎石者：[源码] `StoneCracker.AfterRoomEntered(CombatRoom)` ——
    // 开局把抽牌堆里随机 2 张可升级的牌升级。层数 = 几张。
    // 房间刚进来 = 第 1 回合开始，而且要在**抽牌之前**（升级的是牌堆里的牌，
    // 抽上来的那几张也该是升好的）。
    PowerDef {
        st: St::StoneCracker,
        hook: Hook::TurnStart,
        ops: &[TOp::If { cond: TCond::TurnAtMost(1), then: &[TOp::UpgradeRandomInDraw(2)] }],
    },
    // 小血瓶：[源码] `BloodVial.AfterPlayerTurnStartLate`，`TurnNumber <= 1` -> 回 N 血。
    PowerDef {
        st: St::BloodVial,
        hook: Hook::TurnStart,
        ops: &[TOp::If { cond: TCond::TurnAtMost(1), then: &[TOp::OwnerHeal(Amt::Stacks)] }],
    },
    // 缩放仪：[源码] `Pantograph.BeforeCombatStart`，条件是
    // `CurrentRoom.RoomType == Boss` -> 回 25 血。房间类型是遭遇的属性，
    // 武装走 `CONDITIONAL_START`（调用方给），效果本身和小血瓶同构。
    PowerDef {
        st: St::Pantograph,
        hook: Hook::TurnStart,
        ops: &[TOp::If { cond: TCond::TurnAtMost(1), then: &[TOp::OwnerHeal(Amt::Stacks)] }],
    },
    // 古茶具：[源码] `VenerableTeaSet.AfterEnergyReset` —— 上一个房间是休息处
    // （`GainEnergyInNextCombat`，`[SavedProperty]` 跨战斗）时 +2 能量，然后**清标记**。
    //
    // **和灯笼逐字同构**，差别全在"什么时候挂得上这个 status"：
    // 灯笼是遗物在身上就有，茶具要**局外状态**才武装（`content::REST_ARMED`）。
    // `AfterEnergyReset` 每回合都跑，但标记只有一次 —— 所以 `TurnAtMost(1)`
    // 加 `ClearSelf` 和源码同义（第 1 回合的能量重置就是本场第一次）。
    PowerDef {
        st: St::TeaSet,
        hook: Hook::TurnStart,
        ops: &[TOp::If {
            cond: TCond::TurnAtMost(1),
            then: &[TOp::OwnerEnergy(Amt::Stacks), TOp::ClearSelf],
        }],
    },
    // 赤牛：[源码] `Akabeko.AfterSideTurnStart`，条件 `TurnNumber <= 1`，
    // 效果 `PowerCmd.Apply<VigorPower>(..., 8)`。
    // **和灯笼逐字同构**（同一个 `<= 1` 而不是 `== 1`），所以照着抄一行表。
    // 层数 = 给多少活力；活力本身的规则在 `St::Vigor` + `step::spend_vigor`。
    // 高压：[源码] `HighVoltagePower.AfterSideTurnEnd` —— 敌人回合结束时
    // 给**自己**加等于层数的力量。`Counter` 型，不衰减，所以一路涨。
    // `Hook::EnemyTurnEnd` 就是为它加的，目前只有它一个消费者。
    PowerDef {
        st: St::HighVoltage,
        hook: Hook::EnemyTurnEnd,
        ops: &[TOp::OwnerStatus { st: St::Strength, amt: Amt::Stacks }],
    },
    // 领地意识：[源码] `TerritorialPower.AfterSideTurnEnd` —— 和高压**逐字同构**，
    // 所以就是同一行表换个 `st`。多尼斯异鸟出场自带 1 层。
    PowerDef {
        st: St::Territorial,
        hook: Hook::EnemyTurnEnd,
        ops: &[TOp::OwnerStatus { st: St::Strength, amt: Amt::Stacks }],
    },
    // 招架盾：[源码] `ParryingShield.AfterSideTurnEnd` —— 我的回合结束时，
    // 格挡 ≥ 10 就对**随机一只**敌人打 6 点。两个数都是 `ValueProp.Unpowered`
    // （不吃力量），而 `TOp::DamageRandomEnemy` 走的正是 `hit_enemy_unpowered`。
    PowerDef {
        st: St::ParryingShield,
        hook: Hook::TurnEnd,
        ops: &[TOp::If {
            cond: TCond::OwnerBlockAtLeast(10),
            then: &[TOp::DamageRandomEnemy(Amt::Stacks)],
        }],
    },
    // 尖叫酒壶：[源码] `ScreamingFlagon.BeforeSideTurnEnd` —— 我的回合结束、
    // 手牌为空 -> 对**所有**敌人打 20 点。`CanonicalVars` 是
    // `DamageVar(20m, ValueProp.Unpowered)`，而 `TOp::DamageAllEnemies` 走的
    // 正是 `hit_enemy_unpowered`，两边对上。
    //
    // **钩子必须是 `TurnEnd`**（源码那个方法就叫 `BeforeSideTurnEnd`）：
    // 弃手牌发生在它之后，所以判的是"我自己把手牌打空了"。挪到弃牌之后就恒真。
    PowerDef {
        st: St::ScreamingFlagon,
        hook: Hook::TurnEnd,
        ops: &[TOp::If {
            cond: TCond::HandEmpty,
            then: &[TOp::DamageAllEnemies(Amt::Stacks)],
        }],
    },
    // 斗篷扣：[源码] `CloakClasp.BeforeSideTurnEnd` —— 我的回合结束时
    // `(int)(手牌张数 × Block)` 点格挡（`BlockVar(1m, Unpowered)`）。
    // 层数 = **每张给几点**，所以 `Amt::HandCardsTimesStacks`。
    //
    // **时点和尖叫酒壶同一个**（都是 `BeforeSideTurnEnd`）：弃手牌在它之后，
    // 所以数的是"我没打出去的那几张"。挪到弃牌之后恒为 0。
    PowerDef {
        st: St::CloakClasp,
        hook: Hook::TurnEnd,
        ops: &[TOp::OwnerBlock(Amt::HandCardsTimesStacks)],
    },
    // 号角靴钉：[源码] `HornCleat.AfterBlockCleared` 且 `TurnNumber == 2` -> 14 格挡。
    // 内核的 `Hook::TurnStart` 就在**清完格挡之后**（`start_player_turn_before_draw`
    // 先清格挡再点火），和源码那个时点同相，所以不需要新钩子。
    PowerDef {
        st: St::HornCleat,
        hook: Hook::TurnStart,
        ops: &[TOp::If { cond: TCond::TurnIs(2), then: &[TOp::OwnerBlock(Amt::Stacks)] }],
    },
    // 百年积木：[源码] `CentennialPuzzle.AfterDamageReceived`，条件
    // `UnblockedDamage > 0 && !UsedThisCombat`，效果抽 3。
    // `ClearSelf` 就是那个 `UsedThisCombat`：清零之后规则不再匹配。
    // ---- 2026-08-27 第二批遗物的规则。每条都对着 [源码] 抄，条件写在 TCond 里 ----
    // 锚：[源码] `BeforeCombatStart` 给 10 点格挡（Unpowered）。内核挂在
    // 第 1 回合的 `TurnStart` 上 —— 战斗开始给的格挡本来就是带进第 1 回合的，
    // 和灯笼那条 `TurnAtMost(1)` 同一个写法。
    PowerDef {
        st: St::Anchor,
        hook: Hook::TurnStart,
        ops: &[TOp::If { cond: TCond::TurnAtMost(1), then: &[TOp::OwnerBlock(Amt::Stacks)] }],
    },
    // 弹珠袋 / 红面具：[源码] 两件逐字同构，只差挂的 status。
    // 条件是 `TurnNumber <= 1`（**不是 == 1**），照抄。
    PowerDef {
        st: St::BagOfMarbles,
        hook: Hook::TurnStart,
        ops: &[TOp::If {
            cond: TCond::TurnAtMost(1),
            then: &[TOp::AllEnemiesStatus { st: St::Vulnerable, amt: Amt::Stacks }],
        }],
    },
    PowerDef {
        st: St::RedMask,
        hook: Hook::TurnStart,
        ops: &[TOp::If {
            cond: TCond::TurnAtMost(1),
            then: &[TOp::AllEnemiesStatus { st: St::Weak, amt: Amt::Stacks }],
        }],
    },
    // 准备背包：[源码] 改的是第 1 回合的抽牌张数。`Hook::TurnStart` 在**抽牌之前**
    // 点火（见钩子表），所以这里多抽 2 张就等价于「起手多 2 张」。
    PowerDef {
        st: St::BagOfPreparation,
        hook: Hook::TurnStart,
        ops: &[TOp::If { cond: TCond::TurnAtMost(1), then: &[TOp::OwnerDraw(Amt::Stacks)] }],
    },
    // 烛台：[源码] `TurnNumber == 2`，**是等于不是大于等于**。
    PowerDef {
        st: St::Candelabra,
        hook: Hook::TurnStart,
        ops: &[TOp::If { cond: TCond::TurnIs(2), then: &[TOp::OwnerEnergy(Amt::Stacks)] }],
    },
    // 开心小花 / 花粉核心：每 n 回合一次。**不带相位** —— 它们的计数器每场从 0 数，
    // 和摆动球那个跨战斗保留的不是一回事，见 `TCond::TurnMultipleOf`。
    PowerDef {
        st: St::HappyFlower,
        hook: Hook::TurnStart,
        ops: &[TOp::If { cond: TCond::TurnMultipleOf(3), then: &[TOp::OwnerEnergy(Amt::Stacks)] }],
    },
    // 假商人版：周期 5 回合（真品 3）。**必须单独一条规则** ——
    // 周期写死在 `TurnMultipleOf` 里，复用真品那条会每 3 回合就多给 1 能量。
    PowerDef {
        st: St::FakeHappyFlower,
        hook: Hook::TurnStart,
        ops: &[TOp::If { cond: TCond::TurnMultipleOf(5), then: &[TOp::OwnerEnergy(Amt::Stacks)] }],
    },
    PowerDef {
        st: St::PollinousCore,
        hook: Hook::TurnStart,
        ops: &[TOp::If { cond: TCond::TurnMultipleOf(4), then: &[TOp::OwnerDraw(Amt::Stacks)] }],
    },
    // 佩尔之血：[源码] `ModifyHandDraw => count + Cards(1)`，**无条件、每回合**。
    // 这一族里唯一一个连 `TCond` 都不需要的。
    //
    // **为什么建成"回合开始多抽"而不是"把 5 改成 6"**：`Hook::TurnStart` 在
    // `open_hand` **之前**跑（`start_player_turn` = `start_player_turn_before_draw`
    // + `open_hand`），先抽 1 再抽 5 和一次抽 6 从牌堆顶取到的是同一批牌，
    // 而且和准备背包/花粉核心逐字同一个形状 —— 不必为它新开一条"抽牌数修饰器"。
    // 手牌上限也不用另管：`draw_one` 自己拦 `MAX_HAND`。
    //
    // [实测] 2026-09-06 `act3_f46_elite_soul_nexus`：内核每个回合比游戏少发 1 张
    //（`~ 回合开始手牌张数 游戏=7 内核=6`，把王室认证的保留建好之后还差这一张）。
    // 补上之后 5 + 1(本条) + 摆动球那一张 = 7，和观测对上。
    PowerDef { st: St::PaelsBlood, hook: Hook::TurnStart, ops: &[TOp::OwnerDraw(Amt::Stacks)] },
    // 佩尔之肉：[源码] `if (TurnNumber < 3) return;` —— 第 3 回合起**每回合**都给。
    PowerDef {
        st: St::PaelsFlesh,
        hook: Hook::TurnStart,
        ops: &[TOp::If { cond: TCond::TurnAtLeast(3), then: &[TOp::OwnerEnergy(Amt::Stacks)] }],
    },
    // 苦无：[源码] `AttacksPlayedThisTurn % 3 == 0` —— 和精致折扇同一个条件，
    // 第 3/6/9… 张都给，不是只有第 3 张。
    PowerDef {
        st: St::Kunai,
        hook: Hook::PlayerAttack,
        ops: &[TOp::If {
            cond: TCond::EveryNthAttackThisTurn(3),
            then: &[TOp::OwnerStatus { st: St::Dexterity, amt: Amt::Stacks }],
        }],
    },
    // 扑翼：[源码] `FlutterPower.AfterDamageReceived` —— **每挨一次有源攻击掉一层**
    // （条件是 `UnblockedDamage != 0 && IsPoweredAttack()`）。伤害减半那一半在
    // `damage.rs`，这里只管掉层。
    //
    // **两处刻意的近似，方向都是不高估自己**：
    //   * 内核的 `EnemyDamaged` 挨到就发，不看有没有被格挡吃掉（和蜷身/耕地同一条）
    //   * **层数掉到 0 时的眩晕没建**（[源码] 会 `CreatureCmd.Stun` 让它空过一手）。
    //     少算敌人少打一手 = 内核以为自己会多挨一顿，偏保守。
    //     要建得先给它加一手"眩晕"并让出招表能跳过去，见 roadmap。
    PowerDef {
        st: St::Flutter,
        hook: Hook::EnemyDamaged,
        ops: &[TOp::GrowSelf(-1)],
    },
    // 逃脱大师：[源码] `EscapeArtistPower.AfterSideTurnEnd` —— 敌人回合结束减 1，
    // **减到 1 就停**（`if (Amount > 1)`）。它自己只是个计时器，
    // 真正的逃跑写在出招表里；建它是为了让那一列进对拍。
    PowerDef {
        st: St::EscapeArtist,
        hook: Hook::EnemyTurnEnd,
        ops: &[TOp::If { cond: TCond::SelfStacksAtLeast(2), then: &[TOp::GrowSelf(-1)] }],
    },
    // 娇弱：[源码] `TenderPower`。**一对规则，必须一起读**：
    //   `AfterCardPlayed`  -> 力量 −1、敏捷 −1（每打一张）
    //   `AfterSideTurnEnd` -> 力量 +本回合出牌数、敏捷 +本回合出牌数（一次性还回来）
    // 所以它罚的是"一回合里打很多张"，跨回合不留伤。
    // 少建下面那条"还回去"，内核会以为力量被永久削光 —— 那是**过度悲观**，
    // 会让求解器放弃一切多牌回合。
    PowerDef {
        st: St::Tender,
        hook: Hook::CardPlayed,
        ops: &[
            TOp::PlayerStatus { st: St::Strength, amt: Amt::Fixed(-1) },
            TOp::PlayerStatus { st: St::Dexterity, amt: Amt::Fixed(-1) },
        ],
    },
    PowerDef {
        st: St::Tender,
        hook: Hook::TurnEnd,
        ops: &[
            TOp::PlayerStatus { st: St::Strength, amt: Amt::CardsPlayed },
            TOp::PlayerStatus { st: St::Dexterity, amt: Amt::CardsPlayed },
        ],
    },
    // 光耀：[源码] `RadiancePower.AfterEnergyReset` -> `GainEnergy(1)` + `Decrement`。
    // 层数 = 还能给几个回合，所以给的是**固定 1 点**、掉的是自己那一层。
    PowerDef {
        st: St::Radiance,
        hook: Hook::TurnStart,
        ops: &[TOp::OwnerEnergy(Amt::Fixed(1)), TOp::GrowSelf(-1)],
    },
    // 开信刀：[源码] `LetterOpener` —— 同回合每 **3 张技能牌**，对**全体**敌人
    // 打 5 点（`Unpowered`，所以走 `DamageAllEnemies` 那条不吃力量的路）。
    // 条件和精致折扇逐字同构，只是数技能牌，见 `TCond::EveryNthSkillThisTurn`。
    //
    // **必须挂 `PlayerSkill`，不能挂 `CardPlayed`。** 第一版挂错了：
    // `CardPlayed` 每张牌都触发，而条件只看"技能数是不是 3 的倍数" ——
    // 打满 3 张技能之后，**后面每打一张牌它都再发作一次**。
    // 2026-08-27 第 2 幕精英那条实录当帧就红了（内核多杀一只、多回 6 血）。
    // 精致折扇挂的是 `PlayerAttack` 而不是 `CardPlayed`，本来就是这个道理。
    PowerDef {
        st: St::LetterOpener,
        hook: Hook::PlayerSkill,
        ops: &[TOp::If {
            cond: TCond::EveryNthSkillThisTurn(3),
            then: &[TOp::DamageAllEnemies(Amt::Stacks)],
        }],
    },
    // 蟹之怒：[源码] `CrabRagePower` —— **有盟友死亡时**，持有者 +6 力量 +99 格挡。
    // 挂 `Hook::AllyDied`（**不是 `EnemyDied`**）：后者的语义是"死掉的那只自己
    // 触发"（寄生物召唤自己那 4 只），拿它会让死人给自己加力量、而幸存者没反应。
    // 第一版就是这么写的，测试当场红。
    //
    // [实测] 2026-08-27 第 2 幕 Boss：碾碎爪死的那一帧火箭 `力量 2 -> 8`、
    // `格挡 0 -> 99`，**下一帧它自己回合开始 99 清零**（`begin_enemy_turn` 清格挡）。
    // 所以那 99 只吃"杀掉同伴之后到它出手之前"这一段的伤害 —— 建出来是为了让
    // 求解器知道**杀完别再往它身上砸伤害**，而不是让它怕那个 99。
    PowerDef {
        st: St::CrabRage,
        hook: Hook::AllyDied,
        ops: &[
            TOp::OwnerStatus { st: St::Strength, amt: Amt::Fixed(6) },
            TOp::OwnerBlock(Amt::Fixed(99)),
        ],
    },
    // 贪食（噬尸蛞蝓，[源码] `RavenousPower.AfterDeath`）：**同伴死掉时**
    // 加 N 点力量，并且**被自己的进食动作击晕一回合**
    // （`CreatureCmd.Stun(base.Owner, StunnedMove)`）。
    //
    // 两件事都建了：力量走 `OwnerStatus`，击晕走 `St::Stunned`
    // —— `step::take_stun_if_stunned` 在敌人回合开头认它，跳过那一手并把
    // 出招指针退回去，正是源码里"进食那一回合它不出手"的形状。
    //
    // 和蟹之怒（`Hook::AllyDied`）逐字同构，只差数值和那个击晕。
    PowerDef {
        st: St::Ravenous,
        hook: Hook::AllyDied,
        ops: &[
            TOp::OwnerStatus { st: St::Strength, amt: Amt::Stacks },
            TOp::OwnerStatus { st: St::Stunned, amt: Amt::Fixed(1) },
        ],
    },
    PowerDef {
        st: St::CentennialPuzzle,
        hook: Hook::PlayerDamaged,
        ops: &[TOp::OwnerDraw(Amt::Stacks), TOp::ClearSelf],
    },
    PowerDef {
        st: St::Akabeko,
        hook: Hook::TurnStart,
        ops: &[TOp::If {
            cond: TCond::TurnAtMost(1),
            then: &[TOp::OwnerStatus { st: St::Vigor, amt: Amt::Stacks }],
        }],
    },
    PowerDef {
        st: St::Pendulum,
        hook: Hook::TurnStart,
        ops: &[TOp::If {
            cond: TCond::EveryNTurns { n: 3, phase: St::PendulumPhase },
            then: &[TOp::OwnerDraw(Amt::Stacks)],
        }],
    },
    PowerDef {
        st: St::StoneCalendar,
        hook: Hook::TurnEnd,
        ops: &[TOp::If { cond: TCond::TurnIs(7), then: &[TOp::DamageAllEnemies(Amt::Stacks)] }],
    },
    // 奥利哈钢两段。**第一段只写标记，不给格挡** —— 那正是它要的语义：
    // 判定发生在覆甲给格挡之前，结算发生在之后。
    PowerDef {
        st: St::Orichalcum,
        hook: Hook::TurnEndVeryEarly,
        ops: &[TOp::If {
            cond: TCond::OwnerHasNoBlock,
            then: &[TOp::OwnerStatus { st: St::OrichalcumArmed, amt: Amt::Fixed(1) }],
        }],
    },
    PowerDef {
        st: St::Orichalcum,
        hook: Hook::TurnEnd,
        ops: &[TOp::If {
            cond: TCond::OwnerHas(St::OrichalcumArmed),
            then: &[
                TOp::OwnerBlock(Amt::Stacks),
                TOp::OwnerStatus { st: St::OrichalcumArmed, amt: Amt::Fixed(-1) },
            ],
        }],
    },
    // ---- 战斗胜利结算。两条规则，**顺序由钩子定，不由表里的先后定** ----
    // 带骨肉先（Early）、燃烧之血后 —— [源码] 的 `AfterCombatVictoryEarly`
    // 早于 `AfterCombatVictory`。这不是可有可无的细节：50% 阈值要拿
    // 结束那一刻的血量判，实录 act2_f31（36/80 → 54）判过。
    PowerDef {
        st: St::MeatOnTheBone,
        hook: Hook::CombatVictoryEarly,
        ops: &[TOp::OwnerHealIfHpAtMost { pct: 50, amt: Amt::Stacks }],
    },
    PowerDef {
        st: St::BurningBlood,
        hook: Hook::CombatVictory,
        ops: &[TOp::OwnerHeal(Amt::Stacks)],
    },
    // 恶魔形态：回合开始 +N 力量。原先硬编码在 start_player_turn
    PowerDef {
        st: St::DemonForm,
        hook: Hook::TurnStart,
        ops: &[TOp::OwnerStatus { st: St::Strength, amt: Amt::Stacks }],
    },
    // 活力火花：**敌人**持有；我每打出一张技能牌，它给**我** N 层污染。
    // [源码] `VitalSparkPower` 真正做的是给我牌组里每张技能牌挂 `Tainted` 附魔，
    // 打出时才生效 —— 牌级附魔内核没有，降成"打出技能牌时直接上 status"，
    // 对可观测的量完全等价（和轰鸣同一处降维手法）。
    //
    // **只建层数这一半，伤害那一半故意不建。**
    // 污染的作用是「我挨的每一次攻击 +层数」，而默认对拍路径上敌人伤害是
    // **注入观测到的意图标签**的，标签**已经含了污染**（实测：打出一张技能后
    // 标签当场 15 -> 17）。再在内核里加一次就是重复计数 —— 和古茶具那 2 点
    // 能量、薪火之源那 1 点能量是同一个坑。理由和后果写在 `St::Tainted`。
    PowerDef {
        st: St::VitalSpark,
        hook: Hook::PlayerSkill,
        ops: &[TOp::PlayerStatus { st: St::Tainted, amt: Amt::Stacks }],
    },
    // 激怒：**敌人**持有；玩家每打出一张技能牌，它 +N 力量。
    // 原先硬编码在 play_card。能力牌不触发它 —— 这条是靠 `Kind::Power`
    // 不等于 `Kind::Skill` 天然保证的，不是特判。
    PowerDef {
        st: St::Rage,
        hook: Hook::PlayerSkill,
        ops: &[TOp::OwnerStatus { st: St::Strength, amt: Amt::Stacks }],
    },
    // 无惧疼痛：每消耗一张牌获得 N 格挡
    PowerDef {
        st: St::FeelNoPain,
        hook: Hook::CardExhausted,
        ops: &[TOp::OwnerBlock(Amt::Stacks)],
    },
    // 黑暗之拥：每消耗一张牌抽 N 张
    PowerDef {
        st: St::DarkEmbrace,
        hook: Hook::CardExhausted,
        ops: &[TOp::OwnerDraw(Amt::Stacks)],
    },
    // 撕裂：我的回合内每次失去生命，+N 力量
    PowerDef {
        st: St::Rupture,
        hook: Hook::PlayerLoseHp,
        ops: &[TOp::OwnerStatus { st: St::Strength, amt: Amt::Stacks }],
    },
    // 绯红披风：回合开始失去 1 点生命并获得 N 点格挡。
    // 两个数不一样，正是 `Amt::Fixed` 存在的理由。
    PowerDef {
        st: St::CrimsonMantle,
        hook: Hook::TurnStart,
        ops: &[TOp::OwnerLoseHp(Amt::Fixed(1)), TOp::OwnerBlock(Amt::Stacks)],
    },
    // 滚石：回合开始对全体造成 N 伤害，然后 N += 5
    PowerDef {
        st: St::RollingBoulder,
        hook: Hook::TurnStart,
        ops: &[TOp::DamageAllEnemies(Amt::Stacks), TOp::GrowSelf(5)],
    },
    // 凶恶：每给一个敌人上易伤抽 N 张。闪电霹雳给全体易伤 => 触发 N 次
    PowerDef { st: St::Vicious, hook: Hook::ApplyVuln, ops: &[TOp::OwnerDraw(Amt::Stacks)] },
    // 覆甲 —— **四条规则，玩家两条敌人两条**。关键词原文（面向玩家）：
    // 「在你的回合结束时获得格挡。覆甲会在你的回合开始时减少1层。」
    //
    // **同一个 status，两边的时序完全不同**，2026-08-30 第 3 幕第 43 层
    // 青蛙骑士（开局自带覆甲 15）实战撞出来的。[源码] `PlatingPower`：
    //
    //     BeforeSideTurnStart:   side==Player && Owner.IsEnemy && RoundNumber<=1
    //                            => GainBlock(Amount)      // 开局那一次
    //     BeforeSideTurnEndEarly: participants.Contains(Owner)
    //                            => GainBlock(Amount)      // 持有者**自己**的回合末
    //     AfterSideTurnStart:    participants.Contains(Owner) && !(敌人 && RoundNumber==1)
    //                            => Amount -= 1            // 持有者**自己**的回合开始
    //
    // 关键在 `participants.Contains(Owner)`：给格挡的是**持有者那一边**的回合末。
    // 玩家持有 ⇒ 我的回合末（敌人出手之前，所以真的挡得住）；
    // 敌人持有 ⇒ **它自己的回合末**，于是那堵墙立在我的下一个回合面前。
    //
    // 改之前内核只有玩家那两条，而 `fire` 对 `TurnEnd` 两边都发 ——
    // 敌人在**我的**回合末拿到的格挡，紧接着就被 `begin_enemy_turn` 清零了，
    // 于是敌人的覆甲是**一颗哑弹**：191 血的青蛙骑士每回合白送 15 点墙，
    // 内核一点都看不见。方向是**乐观**的（低估敌人有效血量整整一堵墙/回合），
    // 正是最危险的那种。表里当时没有敌人带覆甲，所以语料从没碰过它。
    //
    // 敌人那条为什么用 `TurnAtLeast(2)` 而不是"上膛"（仪式那种）：
    // [源码] 判的**字面就是** `RoundNumber != 1`，而内核的 `s.turn`
    // 在我的回合开始时 +1、敌人回合结束时还没 +，两者逐帧同相。
    // 能照抄就别造一个等价物。
    PowerDef {
        st: St::PlatedArmor,
        hook: Hook::TurnEnd,
        ops: &[TOp::If { cond: TCond::OwnerIsPlayer, then: &[TOp::OwnerBlock(Amt::Stacks)] }],
    },
    // 我的回合开始，两边各干各的 —— **必须写成一条**：
    // `every_power_card_has_a_rule_and_no_rule_is_claimed_twice` 禁止
    // 同一个 `(status, 钩子)` 出现两条规则（那正是"靠表内顺序接力"的味道）。
    // 这里两支按持有者互斥，写成一条之后顺序不再是隐患。
    //   · 玩家持有：掉 1 层（关键词原文「覆甲会在你的回合开始时减少1层」）
    //   · 敌人持有：只在第 1 回合给一次开局格挡。源码注释原话是
    //     "We want enemies that start with Plating to also start combat with block"。
    //     敌人持有、却挂在**玩家侧**钩子上 —— 和盾墙同形。
    PowerDef {
        st: St::PlatedArmor,
        hook: Hook::TurnStart,
        ops: &[
            // **玩家侧也要 `TurnAtLeast(2)`**，和敌人那半条同一个来源：
            // [源码] `PlatingPower.AfterSideTurnStart` 一个条件管两边 ——
            // `(Owner.Player == null || Owner.Player.PlayerCombatState.TurnNumber != 1)
            //  && (Owner.Side != Enemy || combatState.RoundNumber != 1)`。
            // 敌人那半 2026-08-30 就照抄了，**玩家这半漏了**，而它在对拍路径上
            // 结构性地看不见：`sync` 每帧把 `PLATING_POWER` 从观测重灌，
            // 内核第 1 回合多掉的那一层当帧就被冲掉。
            // [实测] 2026-09-09 `bin/synth_audit` 在 14 条带护喉甲的语料上逐条报出来：
            // 第 0 帧游戏 4、内核 3；`act2_f28_decimillipede` 逐回合 4/9/8/7/6/5/4/3
            // （第 2 回合那个 9 是 4+6 的岩石铠甲减 1）—— 掉层是从第 2 回合开始的。
            TOp::If {
                cond: TCond::OwnerIsPlayer,
                then: &[TOp::If { cond: TCond::TurnAtLeast(2), then: &[TOp::GrowSelf(-1)] }],
            },
            TOp::If {
                cond: TCond::OwnerIsEnemy,
                then: &[TOp::If {
                    cond: TCond::TurnAtMost(1),
                    then: &[TOp::OwnerBlock(Amt::Stacks)],
                }],
            },
        ],
    },
    // 敌人：它自己的回合**开始**掉 1 层，回合**末**按掉完的层数给格挡。
    //
    // **两条必须分在两个钩子上，不能压成一条**：`Amt::Stacks` 是
    // 触发那一刻的**快照**，同一条规则里 `GrowSelf(-1)` 之后再
    // `OwnerBlock(Amt::Stacks)` 拿到的还是旧值。靠"表里前后两条"来接力
    // 也不行 —— 奥利哈钢那条注释明写着不许依赖 `POWERS` 的表内顺序。
    // 按源码的真实位置拆开，两个问题一起没了。
    //
    // 实测钉死的是这个等式：层数 15/15/14/13 与格挡 15/15/14/13
    // **逐帧同值** ⇒ 回合末给的就是已经掉过层的那个数。
    PowerDef {
        st: St::PlatedArmor,
        hook: Hook::EnemyTurnStart,
        ops: &[TOp::If {
            cond: TCond::OwnerIsEnemy,
            then: &[TOp::If { cond: TCond::TurnAtLeast(2), then: &[TOp::GrowSelf(-1)] }],
        }],
    },
    //
    // **给格挡挂在 `EnemyTurnEndEarly` 上**（[源码] `BeforeSideTurnEndEarly`），不是 `EnemyTurnEnd`。
    // 2026-09-14 熟睡甲虫逼出来的：熟睡在 `AfterSideTurnEnd` 减到 0 醒来时把覆甲整个移除，
    // 而那一回合的格挡**已经给过了**。两条压在同一个钩子上，先后就只剩表内顺序。
    PowerDef {
        st: St::PlatedArmor,
        hook: Hook::EnemyTurnEndEarly,
        ops: &[TOp::If { cond: TCond::OwnerIsEnemy, then: &[TOp::OwnerBlock(Amt::Stacks)] }],
    },
    // 沙坑（第 2 幕 Boss 无厌沙虫）：**一条即死倒计时**。
    //
    // [源码] `SandpitPower.AfterSideTurnStartLate(Enemy)` -> `PowerCmd.Decrement`；
    // 计数器归 0 ⇒ power 被移除 ⇒ `AfterRemoved` -> `CreatureCmd.Kill(玩家, force: true)`。
    //
    // 用 `EnemyTurnStart` 而不是另加一个 `EnemyTurnStartLate`：**这两档之间
    // 内核没有任何东西**（游戏侧那一档的区别只是别的 power 的 `AfterSideTurnStart`
    // 先跑），而"加钩子必须连着消费者一起加"这条规矩反过来也成立 ——
    // 不为一个区分不出来的时序差别加第二个钩子。
    //
    // **两条 op 的顺序是规则本身**：先减层、再判 0。`TCond::SelfStacksAtMost`
    // 读的是**当前值**（不是触发那一刻的快照），凋萎存在那条走的是同一条路。
    //
    // [实测] 2026-09-05 两份实录逐帧对上这条时序：
    // 液化地面挂 4 层的那个敌人回合**自己不减**（power 是在那一手里才挂上的），
    // 之后每个敌人回合减 1 ——
    // 驱动语料 `act2_f33_boss_crusher`：r2=4 r3=3 r4=2（打了狂乱逃离 ->3）
    // r5=2（再打一张 ->3）r6=2 r7=1，第 8 个敌人回合归零暴毙。
    //
    // **玩家回合开始时看到 1，就意味着这一回合不打狂乱逃离就会死** ——
    // 结束回合 ⇒ 敌人回合开始 ⇒ 减到 0 ⇒ 被吞。
    PowerDef {
        st: St::Sandpit,
        hook: Hook::EnemyTurnStart,
        ops: &[
            TOp::GrowSelf(-1),
            TOp::If { cond: TCond::SelfStacksAtMost(0), then: &[TOp::KillPlayer] },
        ],
    },
    // 缠绕（[源码] `ConstrictPower`）：**我的回合结束**时受到等于层数的伤害。
    // 走格挡（和灼伤同构，见 `St::Constrict` 的注释）。
    // **不衰减** —— 所以它不在 `decay()` 那三个里，也不在 `TURN_SCOPED` 里。
    PowerDef { st: St::Constrict, hook: Hook::TurnEnd, ops: &[TOp::OwnerTakeDamage(Amt::Stacks)] },
    // 耕地（[源码] `PlowPower.AfterDamageReceived`）：挨打之后，如果
    // **血量已经 ≤ 层数**（150），就清光力量、被打进眩晕、并移除自己。
    //
    // 挂在 `EnemyDamaged` 上：那个钩子的语义正好是「这一下伤害落地之后，
    // 只有挨打的那一只触发」，和源码逐字对应。
    //
    // **一处简化**：源码还要求 `result.UnblockedDamage > 0`，而内核的
    // `EnemyDamaged` 挨到就发、不管有没有被挡住。仪式兽整场不给自己加格挡，
    // 所以这个差别在它身上碰不到；换成会加格挡的敌人就得先给内核
    // 一个「这一下有没有打穿」的条件。
    PowerDef {
        st: St::Shriek,
        hook: Hook::EnemyDamaged,
        // 骇鳗的尖叫。[源码] `ShriekPower.AfterDamageReceived`：
        // `result.UnblockedDamage > 0 && target.CurrentHp <= Amount`
        // ⇒ `CreatureCmd.Stun(owner, TerrorState.StateId)` + 移除自身。
        //
        // **和巨像的耕地闸门是同一个形状**（血量阈值 -> 换招 -> 清掉自己），
        // 所以照抄那条的结构，只是不清力量。
        // 下标 2 = 骇鳗 `moves` 里的「击晕」，见 `M_TERROR_EEL` 的下标表。
        //
        // **`UnblockedDamage > 0` 这一半内核对不齐**：`Hook::EnemyDamaged` 在
        // 伤害落地之后触发，拿不到"这一下有没有被格挡吃掉"。差别只在
        // 「敌人有格挡且这一下全被挡掉、同时血量已经在阈值以下」这一种局面，
        // 而骇鳗自己不会加格挡 —— 记在这里，别当它已经忠实。
        ops: &[TOp::If {
            cond: TCond::OwnerHpAtMostStacks,
            then: &[TOp::OwnerForceMove(2), TOp::ClearSelf],
        }],
    },
    PowerDef {
        st: St::Plow,
        hook: Hook::EnemyDamaged,
        ops: &[TOp::If {
            cond: TCond::OwnerHpAtMostStacks,
            then: &[
                TOp::OwnerClearStatus(St::Strength),
                TOp::OwnerClearStatus(St::TempStrength),
                // 2 = 眩晕那一手，见 `M_BEAST` 的下标表
                TOp::OwnerForceMove(2),
                TOp::ClearSelf,
            ],
        }],
    },
    // 人体蜂房（蜂群术士，[源码] `PersonalHivePower.AfterDamageReceived`）：
    // `target == Owner && dealer != null && props.IsPoweredAttack()` ⇒ 往**我的抽牌堆随机位置**
    // 塞 `Amount` 张晕眩（`CardPilePosition.Random`）。
    //
    // 三条都是源码定的，别凭卡面改：
    //   · **每一段攻击各一次**：`AfterDamageReceived` 逐段调用，多段牌付几倍的晕眩
    //   · **不看打没打穿**：门里没有 `UnblockedDamage`
    //   · **只认攻击**：药水 / 遗物 / 荆棘 / 能力牌的伤害是 `Unpowered`，不塞。
    //     [实测] 2026-08-22 `act2_f27_elite_entomancer`（蜂房 1 层）：BASH / ULTIMATE_STRIKE /
    //     HEMOKINESIS / POMMEL_STRIKE / MOLTEN_FIST 各让抽牌堆多 1 张晕眩；
    //     FIRE_POTION（20 点）和 OROBIC_ACID 一张都没塞
    //
    // 挂在 `EnemyDamaged` 上，所以**打死它的那一下不塞**（源码塞，但仗已经打完了，不影响任何结果）。
    PowerDef {
        st: St::PersonalHive,
        hook: Hook::EnemyDamaged,
        ops: &[TOp::If {
            cond: TCond::LastHitWasAttack,
            then: &[TOp::AddCardToDraw { card: card::DAZED, amt: Amt::Stacks }],
        }],
    },
    // 熟睡（熟睡甲虫，[源码] `SlumberPower`）。两处减层，减到 0 就醒：
    //
    //   AfterDamageReceived: target == Owner && UnblockedDamage != 0 ⇒ −1；归零 ⇒
    //       CreatureCmd.Stun(owner, WakeUpMove, "ROLL_OUT_MOVE")   —— 击晕换招，下一手「醒来」
    //   AfterSideTurnEnd（持有者那一边）: −1；归零 ⇒ WakeUpMove（当场移除覆甲，不击晕）
    //
    // **被打穿**不分是不是攻击（门里没有 `IsPoweredAttack`）：药水打穿也算，**被格挡吃掉的不算**。
    // 回合末那条挂在 `EnemyTurnEnd`，而敌人覆甲给格挡在 `EnemyTurnEndEarly` ——
    // 醒来那一回合的格挡已经给过了（[源码] `BeforeSideTurnEndEarly` 早于 `AfterSideTurnEnd`）。
    //
    // 回合末醒的那一次**不改指针**：打鼾之后接什么，内核在打鼾出完那一刻就按
    // 「掷招那一刻还睡不睡」定好了（`M_SLUMBERING_BEETLE` 那个 ≥ 2 的阈值）。
    PowerDef {
        st: St::Slumber,
        hook: Hook::EnemyDamaged,
        ops: &[TOp::If {
            cond: TCond::LastHitUnblocked,
            then: &[
                TOp::GrowSelf(-1),
                // 2 = 醒来（被击晕的那一手），之后固定出击
                TOp::If { cond: TCond::SelfStacksAtMost(0), then: &[TOp::OwnerForceMove(2)] },
            ],
        }],
    },
    PowerDef {
        st: St::Slumber,
        hook: Hook::EnemyTurnEnd,
        ops: &[
            TOp::GrowSelf(-1),
            TOp::If { cond: TCond::SelfStacksAtMost(0), then: &[TOp::OwnerClearStatus(St::PlatedArmor)] },
        ],
    },
    // 沉睡（乐加维林族母，[源码] `AsleepPower`）。三条钩子，**和熟睡（上面那两条）逐条不同**，
    // 别照着改 —— 两个 power 只是长得像：
    //
    //   AfterDamageReceived: target == Owner && UnblockedDamage != 0 ⇒
    //       Remove<PlatingPower> + Stun(owner, WakeUpMove, "SLASH_MOVE") + Remove<self>
    //       —— 打穿一下**整条沉睡直接没了**（熟睡是 −1 层）
    //   BeforeSideTurnEndVeryEarly: Amount <= 1 && 有覆甲 ⇒ Remove<PlatingPower>
    //       —— **最早一档**，排在覆甲给格挡（`BeforeSideTurnEndEarly`）之前，
    //          所以最后一个睡眠回合它**拿不到那堵墙**。`Hook::EnemyTurnEndVeryEarly`
    //          就是为这一条加的，它是唯一的消费者
    //   AfterSideTurnEnd: −1；归零 ⇒ WakeUpMove，而 [源码] 的 `WakeUpMove` 只播动画、
    //       **不碰覆甲**（覆甲在上一档已经摘掉了）⇒ 内核这一条只剩减层
    //
    // 「打穿」不分是不是攻击（门里没有 `IsPoweredAttack`），被格挡吃掉的不算 ——
    // 和熟睡同一条判据（`TCond::LastHitUnblocked`）。开局覆甲 12，打不穿就醒不了。
    PowerDef {
        st: St::Asleep,
        hook: Hook::EnemyDamaged,
        ops: &[TOp::If {
            cond: TCond::LastHitUnblocked,
            then: &[
                TOp::OwnerClearStatus(St::PlatedArmor),
                // 5 = 醒来（被击晕的那一手），之后固定接斩击（[源码] `Stun(.., "SLASH_MOVE")`）
                TOp::OwnerForceMove(5),
                // **必须是最后一条**：`ClearSelf` 之后这条规则的层数就是 0 了
                TOp::ClearSelf,
            ],
        }],
    },
    PowerDef {
        st: St::Asleep,
        hook: Hook::EnemyTurnEndVeryEarly,
        ops: &[TOp::If {
            cond: TCond::SelfStacksAtMost(1),
            then: &[TOp::OwnerClearStatus(St::PlatedArmor)],
        }],
    },
    // 减层。归零之后 `fire` 那道 `n > 0` 的门自然不再发它，所以不用夹 0。
    // **不改指针**：睡到自然醒的那一次游戏侧没有击晕，下一手接什么在族母出完
    // 沉睡那一刻就按「掷招那一刻还睡不睡」定好了（`M_LAGAVULIN_MATRIARCH` 那个 ≥ 2 的阈值）。
    PowerDef { st: St::Asleep, hook: Hook::EnemyTurnEnd, ops: &[TOp::GrowSelf(-1)] },
    // 滑溜（墨影幻灵 / 墨宝，[源码] `SlipperyPower.AfterDamageReceived`）：
    // `target == Owner && result.UnblockedDamage >= 1` ⇒ `Decrement`。
    //
    // **封顶那一半不在这张表里**，它在 `damage::absorb`（掉血封顶 1，不是伤害封顶 1）——
    // 为什么分在两处、以及和无实体差在哪，写在 `St::Slippery`。
    //
    // 判据用 `LastHitUnblocked` 和熟睡同一条。**封顶在减层之前发生**，所以
    // `UnblockedDamage` 已经是压完的 1 —— 两种读法在"够不够 1"上同真同假，
    // 这条门因此不受封顶顺序影响。
    PowerDef {
        st: St::Slippery,
        hook: Hook::EnemyDamaged,
        ops: &[TOp::If { cond: TCond::LastHitUnblocked, then: &[TOp::GrowSelf(-1)] }],
    },
    // 硬化外壳（鬼祟珊瑚群，[源码] `HardenedShellPower.BeforeSideTurnStart`）：
    // **任何一边**的回合开始把 `damageReceivedThisTurn` 清零 —— 内核存的是余额，于是「清零」就是
    // 「余额回满到上限」。持有者是上限那个私有标记（`St::HardenedShellCap`），层数就是上限 20。
    //
    // **封顶那一半不在这张表里**，在 `damage::absorb`（和滑溜同一格，扣完格挡之后）。
    // 钩子为什么是新开的 `SideTurnStart` 而不是 `TurnStart`：见那个钩子的文档（水银沙漏的 3 点
    // 该算进我这个回合的额度，压在同一个钩子上就只剩表内顺序）。
    PowerDef {
        st: St::HardenedShellCap,
        hook: Hook::SideTurnStart,
        ops: &[TOp::OwnerSetStatus { st: St::HardenedShell, amt: Amt::Stacks }],
    },
    // 吮吸（化石追踪者，[源码] `SuckPower.AfterAttack`）：它这一次攻击里**打穿了几段**，
    // 就 +`Amount` × 段数 力量。内核逐段给（`AttackUnblocked` 本来就逐段点火）——
    // 等价的理由在 `St::Suck`：同一手的面板值开头只算一次，逐段给的力量进不了下一段。
    // 门和纸伤难愈同一个（`UnblockedDamage > 0`，被格挡吃光的那段不算）。
    PowerDef {
        st: St::Suck,
        hook: Hook::AttackUnblocked,
        ops: &[TOp::OwnerStatus { st: St::Strength, amt: Amt::Stacks }],
    },
    // 意外（地精佣兵，[源码] `SurprisePower.AfterDeath`）：先召唤卑鄙地精、再召唤胖地精。
    // 和寄生物同一个形状（`EnemyDied` 只让死的那一只触发、`summon_one` 往后开新格，
    // 尸体留在原格 —— 所以打死它那一刻场上就有活敌人，战斗不结束，
    // 等于 [源码] 的 `ShouldStopCombatFromEnding => true`）。
    //
    // **血量是 `[判断]`**：两只的 [源码] 区间是 10–14 / 13–17（`ToughEnemies` 各 +1），
    // `SummonN` 只收一个数，取中位 12 / 15 —— 和寄生物的扭动虫取 19（17–21 的中位）同一个做法。
    // 偷来的金币（`HeistPower`）战斗层没有，不挂。
    PowerDef {
        st: St::Surprise,
        hook: Hook::EnemyDied,
        ops: &[
            TOp::SummonN { def: enemy::SNEAKY_GREMLIN, hp: 12, count: 1 },
            TOp::SummonN { def: enemy::FAT_GREMLIN, hp: 15, count: 1 },
        ],
    },
    // 轰鸣：在**我的回合结束**时自己消失（[源码] `AfterSideTurnEnd`）。
    // 它是敌人在**它的**回合挂上来的，所以撑过我的下一个回合再消失 ——
    // 这正是它能限制我一整个回合的原因。放 `TURN_SCOPED` 会在回合**开始**
    // 就清掉，等于这条 debuff 从来没生效过。
    PowerDef { st: St::Ringing, hook: Hook::TurnEnd, ops: &[TOp::ClearSelf] },
    // 缠结（藤蔓蹒跚者）：同一个形状 —— 敌人回合里挂上、撑过我的下一个回合、**我的**回合结束摘掉
    // （[源码] `TangledPower.AfterSideTurnEnd`，`participants.Contains(Owner)`；敌人回合结束时 participants 是敌人，不摘）。
    // 挂 `TurnEndLate` 而不是轰鸣那个 `TurnEnd`：[源码] 这一句在 `FlushPlayerHand` **之后**，
    // `TurnEndLate` 是内核里离它最近的一档（`AfterSideTurnEnd` 和 `...Late` 之间没有消费者分得开）。
    // 两档之间没有任何东西读攻击牌的费用，所以这个选择改不了一个数；写近的那个只是为了不留一条假的时点。
    PowerDef { st: St::Tangled, hook: Hook::TurnEndLate, ops: &[TOp::ClearSelf] },
    // 寄生物「死亡时，召唤……某种东西」= [源码] `InfestedPower.AfterDeath`：
    // 4 只 Wriggler，每只 `StartStunned = true`（所以它们的机器从眩晕那一手开始）。
    // 层数 4 就是只数，但这里写 `Fixed(4)` 而不是 `Stacks` —— `SummonN` 的
    // count 是编译期字段，不吃 `Amt`。层数变了要回来改这一行。
    PowerDef {
        st: St::Infested,
        hook: Hook::EnemyDied,
        ops: &[TOp::SummonN { def: enemy::WRIGGLER, hp: 19, count: 4 }],
    },
    // 适生力（实验体自带 1 层）：**被击杀时不死，回满血进入下一个形态**。
    //
    // [源码] `AdaptablePower.AfterDeath` -> `TestSubject.TriggerDeadState()`
    // -> `SetMoveImmediate(DeadState)`；`RespawnMove` 里按 `Respawns` 分两支：
    //     case 1: Revive(SecondFormHp=200) + Apply<PainfulStabsPower>(1)
    //     case 2: Revive(ThirdFormHp=300)  + Apply<NemesisPower>(1)
    //             + Remove<AdaptablePower> + Remove<PainfulStabsPower>
    // 外加 `ShouldStopCombatFromEnding() => true`、
    // `ShouldCreatureBeRemovedFromCombatAfterDeath() => false`。
    //
    // **阶段判据用最大生命，不用私有计数器**：三个形态是 100/200/300，
    // 而最大生命是**观测量**、每帧同步进来 —— 复苏一次它自己就变了。
    // 拿它判阶段，从战斗中途 `sync` 接进来也不会错相。
    //
    // **`ClearOwnerStatusesExcept` 那一条是这只 Boss 的要害**：
    // 死亡剥离连激怒和攒下来的力量一起清掉（[源码] 那个虚方法默认 true，
    // 只有适生力/剧痛刺击重写成 false）。所以
    // **只有第一条命打技能牌才涨力量** —— 玩家先给的判定，源码在这里对上了。
    // 我给它挂的易伤/虚弱同样一起没。
    //
    // **一处刻意的时序简化**：游戏里回血发生在它自己回合的「复苏」那一手，
    // 内核在**死亡当帧**就回满（否则 0 血 = 死，`enemy_turn` 会跳过它、
    // 复苏那一手永远轮不到）。对求解器没有影响（复苏那一手本来就不打人），
    // 但对拍时"我砍死它的那一帧"内核会比游戏早一步显示新形态的血量。
    PowerDef {
        st: St::Adaptable,
        hook: Hook::EnemyDied,
        ops: &[
            TOp::ClearOwnerStatusesExcept(&[St::Adaptable, St::PainfulStabs]),
            // **两支的顺序是要害，别调回来。** `ops` 是顺序执行的，而这两支的
            // 判据（最大生命）正是第一支要改的东西 —— 先判「形态 1」的话，
            // 它把 100 改成 200 之后，「形态 2」那支的 `AtLeast(151)` 当场成立，
            // 两支一起发作、直接跳到 300。**写这条时就是这么错了一次**，
            // 测试报「换形态 2：left 300 / right 200」。
            //
            // 先判 300 那支就没这个问题：它把值改大之后，
            // 另一支的 `AtMost(150)` 必然不成立。
            //
            // 形态 2 -> 3：适生力和剧痛刺击都摘掉，换上复仇宿敌 ⇒ 第三条命是最后一条
            TOp::If {
                cond: TCond::OwnerMaxHpAtLeast(TEST_SUBJECT_FORM1_MAX + 1),
                then: &[
                    TOp::OwnerSetMaxHp(TEST_SUBJECT_FORM_HP[2]),
                    TOp::OwnerStatus { st: St::Nemesis, amt: Amt::Fixed(1) },
                    // **适生力和剧痛刺击不在这里摘** —— 源码里那两个 Remove 在
                    // RespawnMove（它自己的回合）。在死亡当帧摘掉的话，
                    // "还在场上"的判据当场失效、战斗直接判结束，
                    // 第三条命根本不会出场。挪到下面回血那条规则里。
                ],
            },
            // 形态 1 -> 2
            TOp::If {
                cond: TCond::OwnerMaxHpAtMost(TEST_SUBJECT_FORM1_MAX),
                then: &[
                    TOp::OwnerSetMaxHp(TEST_SUBJECT_FORM_HP[1]),
                    TOp::OwnerStatus { st: St::PainfulStabs, amt: Amt::Fixed(1) },
                ],
            },
            // 下一手是「复苏」（[源码] `SetMoveImmediate(DeadState)`）
            TOp::OwnerForceMove(0),
        ],
    },
    // 复苏的第二半：**它自己回合开始时才回满血**。
    //
    // 拆成两半是照抄时序：砍死那一帧只定下一个形态（上面那条），
    // 血量留在 0 —— 于是它在我这个回合剩下的时间里是死的、打不到，
    // 和游戏一致（[实测] 那一帧观测里 enemies 是空的）。
    // 合成一步的话求解器会以为砍完还能接着输出，方向是乐观的。
    //
    // 这条规则**必须在"还是死的"时候跑得起来**，所以 fire 的 allow_dead
    // 扩到了 EnemyTurnStart。
    PowerDef {
        st: St::Adaptable,
        hook: Hook::EnemyTurnStart,
        ops: &[TOp::If {
            cond: TCond::OwnerIsDead,
            then: &[
                TOp::OwnerHealToFull,
                // 形态 3 才把适生力和剧痛刺击摘掉（[源码] RespawnMove 的 case 2）
                // ⇒ 第三条命死了就真的结束。**摘除必须在回血之后**，
                // 否则"还在场上"这个判据会在它出场之前就失效。
                TOp::If {
                    cond: TCond::OwnerMaxHpAtLeast(TEST_SUBJECT_FORM_HP[2]),
                    then: &[
                        TOp::OwnerClearStatus(St::Adaptable),
                        TOp::OwnerClearStatus(St::PainfulStabs),
                    ],
                },
            ],
        }],
    },
    // 蒸汽喷发（瀑布巨兽）：**被击杀时不算死 —— 锁成 999999999 血，下一手「即将爆发」**。
    //
    // [源码] `SteamEruptionPower.AfterDeath`（`!wasRemovalPrevented && creature == Owner`）
    // -> `WaterfallGiant.TriggerAboutToBlowState()`：`SetMaxAndCurrentHp(999999999m)` +
    // `SetMoveImmediate(AboutToBlowState, forceTransition: true)`。这个 power 自己
    // `ShouldPowerBeRemovedAfterOwnerDeath => false`，别的 power 照常随死亡摘掉。
    //
    // 和适生力的区别是**血当场就回来了**（[实测] 砍死那一帧之后观测里就是 999999999），
    // 所以它从死的那一刻起就是活的：打得到、上得了 debuff（虚弱真的压得低爆炸那一下），
    // 「还在场上」那三处口径一处都不用碰。它没有第二条命：「即将爆发」移除蒸汽喷发之后，
    // 「爆炸」里的 `EOp::KillSelf` 再死一次就不会再触发这条。
    //
    // 叶评估那一侧：锁血期间 `remaining_hp_including_revives` 数 0（`hp_left_this_form`）。
    PowerDef {
        st: St::SteamEruption,
        hook: Hook::EnemyDied,
        ops: &[
            TOp::ClearOwnerStatusesExcept(&[St::SteamEruption]),
            TOp::OwnerSetMaxHp(ABOUT_TO_BLOW_HP),
            TOp::OwnerHealToFull,
            TOp::OwnerForceMove(WATERFALL_ABOUT_TO_BLOW_MOVE),
        ],
    },
    // 幻象（寄生惧魔自带）：**被击杀时不移出战斗，下一手回满血**。
    //
    // [源码] `IllusionPower`：
    //   `ShouldCreatureBeRemovedFromCombatAfterDeath(owner) => false`  尸体留在场上
    //   `AfterDeath` -> `SetMoveImmediate(REVIVE_MOVE)`，
    //                   `FollowUpStateId` 指回它本来要出的那一手
    //   `ReviveMove`  -> `Heal(MaxHp - CurrentHp)`  回满
    //   `AfterApplied` -> 顺带挂上 `MinionPower`（所以主人一死它跟着消失）
    //
    // **和适生力是同一个形状，但有一处关键的不同**：适生力换形态、幻象**不换**，
    // 所以这里没有那两支按最大生命分岔的 `If`。复活次数**无上限** ——
    // `no_master_left` 才是这场仗的终止条件，不是它的血量。
    //
    // **死亡剥离的方向和适生力正好相反，别抄错**：
    // [源码] `ShouldPowerBeRemovedOnDeath(power)` = `power.Type == Debuff && !(power is ITemporaryPower)`
    // —— **只剥非临时的 debuff，buff 全留着**（注释原文：
    // "Illusions keep their buffs (including IllusionPower itself) after dying"）。
    // 而适生力那条是"默认全剥、只留名单里那两个"。
    // 所以保留名单要写成「幻象 + 爪牙 + 力量 + 临时力量」：
    //   · 幻象/爪牙 —— 它自己那两个标记
    //   · 力量 —— 哀嚎给的那 3 点，源码明说 buff 留着
    //   · 临时力量 —— 黑暗镣铐那种记账量，源码特意放过临时 debuff
    //     好让回合末还得回去（内核把它记成"力量变负 + TempStrength 记债"）
    // 我给它上的易伤/虚弱**会**被剥掉，和源码一致。
    //
    // **`[源码]`：没有任何一条实录碰过复活这一段** —— 唯一那场实录里
    // 幻象活到最后（1 血），本体先死。第一次真砍死它的时候对拍才会判这条。
    PowerDef {
        st: St::Illusion,
        hook: Hook::EnemyDied,
        ops: &[
            TOp::ClearOwnerStatusesExcept(&[
                St::Illusion,
                St::Minion,
                St::Strength,
                St::TempStrength,
            ]),
            // 下一手是「复苏」（0 号），和实验体走同一个 op
            TOp::OwnerForceMove(0),
        ],
    },
    // 复苏的第二半，和适生力逐字同构：**它自己回合开始时才回满血**。
    // 拆成两半同样是照抄时序 —— 死那一帧它是 0 血、打不到，
    // 合成一步会让求解器以为"砍完还能接着输出"，方向是乐观的。
    PowerDef {
        st: St::Illusion,
        hook: Hook::EnemyTurnStart,
        ops: &[TOp::If { cond: TCond::OwnerIsDead, then: &[TOp::OwnerHealToFull] }],
    },
    // 接续（残杀千足虫，[源码] `ReattachPower`，每节出场自带 25）：
    // **一节被砍死不移出战斗，死后第二个敌人回合回 25 血。**
    //
    //   `ShouldCreatureBeRemovedFromCombatAfterDeath(owner) => false`  尸体留在场上
    //   `ShouldAllowHitting => !IsReviving`                          尸体打不到（内核：血量 0 本来就打不到）
    //   `ShouldOwnerDeathTriggerFatal => AreAllOtherSegmentsDead()`  别的节全死了才结束
    //   `AfterDeath` -> `SetMoveImmediate(DeadState)`：`DEAD_MOVE` 什么都不做，
    //   后继 `REATTACH_MOVE` -> `DoReattach` -> `Heal(Amount)`，再往后等权随机三选一
    //
    // **战斗结束的判据一个字不用改**：别的节全死 = 所有非爪牙都死了 = `no_master_left`。
    // 所以接续**不进** `any_enemy_present`（这是它和适生力的区别），而 `DoReattach` 里那个
    // `if (!AreAllOtherSegmentsDead())` 在内核里恒真 —— 能走到回血那一步，战斗就还没结束。
    //
    // **死亡剥离连力量一起清**（那个虚方法默认 true，只有接续自己重写成 false）。
    // [实测] `act2_f28_decimillipede`：节 1 死前缠绕的标签是 `Attack:10`（8 + 力量 2），
    // 接续回来之后第一手缠绕是 `Attack:8`。
    //
    // [实测] 同一份实录两次复活，时序逐帧对上：第 3 回合砍死 -> 第 4 回合观测里没有 ->
    // 第 5 回合 25/44 回来；第 5 回合砍死 -> 第 7 回合 25/40 回来。
    // **窗口因此是两个我方回合**：砍死那一回合 + 下一回合结束之前把别的节全砍掉，它就回不来。
    PowerDef {
        st: St::Reattach,
        hook: Hook::EnemyDied,
        ops: &[
            TOp::ClearOwnerStatusesExcept(&[St::Reattach]),
            TOp::OwnerSetStatus { st: St::ReattachDue, amt: Amt::Fixed(2) },
        ],
    },
    // 接续的第二半：倒计时。敌人回合开始减 1，减到 0 回 `Reattach` 层数那么多血，
    // 并把这一手换成「重接」（它自己不做事，意图是 Heal）。
    //
    // **回血在回合开始、不在它那一手里**，和适生力 / 幻象同一个简化：死人不出手，
    // 要等它活过来这一手才轮得到。`DEAD_MOVE` 那一个敌人回合就是倒计时从 2 数到 1。
    //
    // **强制换招放在复活这一刻，不放在死亡那一刻**（幻象是后者）：对拍路径上尸体的
    // `EnemyDef` 是 UNKNOWN、出招指针也没对齐，死的时候定下的指针传不到这里；
    // 复活这一刻由规则自己定，两条路就都对。
    // **紧跟着清掉 `MoveForcedThisTurn`**：那个标记的意思是"同步那一刻观测到的意图过期了"，
    // 而敌人回合开始时没有任何观测会过期（尸体本来就没有意图）。留着它的话，推演路径上
    // 这一节活过来之后的那个我方回合，注入威胁会把它的伤害跳过一次。
    PowerDef {
        st: St::ReattachDue,
        hook: Hook::EnemyTurnStart,
        ops: &[
            TOp::GrowSelf(-1),
            TOp::If {
                cond: TCond::SelfStacksAtMost(0),
                then: &[
                    TOp::OwnerHeal(Amt::OwnerStacksOf(St::Reattach)),
                    // 3 = 重接，见 `M_DECIMILLIPEDE` 的下标表
                    TOp::OwnerForceMove(3),
                    TOp::OwnerClearStatus(St::MoveForcedThisTurn),
                ],
            },
        ],
    },
    // 瓦解（知识恶魔的诅咒给**我**挂的，[源码] `DisintegrationPower.AfterSideTurnEndLate`）：
    // 我的回合结束时受 `Amount` 点 `Unpowered` 伤害。
    //
    // **时点是我的回合末最后一步**：[源码] `AfterSideTurnEndLate` 在 `Hook.AfterTurnEnd` 里，
    // 第二阶段弃完手牌之后才跑 —— 晚于灼伤那类手牌发作、晚于奥利哈钢，**早于敌人出手**。
    // 所以这一回合剩下的格挡先吃它，吃剩的才去挡敌人那一手。
    // [玩家判定] 2026-09-14：「回合末剩的格挡真的挡得住，并且是先结算」。
    //
    // 走 `TOp::DamagePlayer`（`Unpowered`、过难以杀灭/无实体、走格挡、不是攻击），和流电同一条路。
    PowerDef { st: St::Disintegration, hook: Hook::TurnEndLate, ops: &[TOp::DamagePlayer(Amt::Stacks)] },
    // 复仇宿敌（实验体阶段 3）：**每个敌人回合末交替获得/摘掉 1 层无实体**。
    //
    // [源码] `NemesisPower.AfterSideTurnEnd` 就是一个
    // `_shouldApplyIntangible = !_shouldApplyIntangible` 的开关。
    // 内核直接翻转无实体本身，不另造私有标记。
    //
    // **战术后果**：阶段 3 有一半的回合我的伤害全部降成 1，
    // 也就是那 300 血实际要**双倍的回合数**去啃。不建它是**乐观**的。
    PowerDef {
        st: St::Nemesis,
        hook: Hook::EnemyTurnEnd,
        ops: &[TOp::OwnerToggleStatus(St::Intangible)],
    },
    // 库存（巨斧机器人开局自带 2 层）：**被击杀时换上一个"库存少一个的我"**。
    //
    // [源码] `StockPower.AfterDeath`：
    //     if (!wasRemovalPrevented && target == Owner && Amount > 0) {
    //         axebot.StockAmount = base.Amount - 1;  CreatureCmd.Add(axebot, ..., Owner.SlotName);
    //     }
    // 外加 `ShouldStopCombatFromEnding() => true`：打死带库存的那一具不算赢。
    //
    // **血量每一具重掷**（源码区间 70-78），[实测] 74 / 71 / 77。
    // 内核的 `hp` 是个定值，取区间中点 74 —— 这一条是近似，写在这儿备查。
    //
    // 层数减到 0 的那一具身上就没有这个 status 了（`SummonCarryingSelfMinusOne`
    // 用 `set`），而 `fire` 对 0 层的 status 本来就不触发 ⇒
    // **"第三具死了战斗就结束"是自动成立的**。
    PowerDef {
        st: St::Stock,
        hook: Hook::EnemyDied,
        ops: &[TOp::SummonCarryingSelfMinusOne { def: enemy::AXEBOT, hp: 74 }],
    },
    // 抢夺力量 / 抢夺速度（失落之物 / 遗忘之物）：**自己死时把偷走的还给我**。
    //
    // [源码] `PossessStrengthPower.AfterDeath`：`creature == Owner` 且没被阻止移除时，
    // 对字典里每个受害者 `Apply<StrengthPower>(-stolen)`。偷的那一半在它们的招式里
    // （`EOp::PlayerStatus{-2}` + `EOp::SelfStatus{+2}`）。
    //
    // 退还量为什么读它**自己的**力量而不是一个私有计数器、两者在哪两种情况下分岔，
    // 见 `Amt::OwnerStacksOf`。`EnemyDied` 只让死的那一只自己触发（`fire_ctx` 的
    // `only_ctx`），所以杀掉遗忘之物不会把力量还回来。
    PowerDef {
        st: St::PossessStrength,
        hook: Hook::EnemyDied,
        ops: &[TOp::PlayerStatus { st: St::Strength, amt: Amt::OwnerStacksOf(St::Strength) }],
    },
    PowerDef {
        st: St::PossessSpeed,
        hook: Hook::EnemyDied,
        ops: &[TOp::PlayerStatus { st: St::Dexterity, amt: Amt::OwnerStacksOf(St::Dexterity) }],
    },
    // 流电（电球头开局 6 层）：我每打出一张**能力牌**，挨层数那么多点。
    //
    // [源码] `GalvanicPower.AfterCardPlayed` -> `CreatureCmd.Damage(owner, Amount,
    // ValueProp.Unpowered | Move)`。时点在**结算之后**，正是 `Hook::CardPlayed`。
    // 这 6 点**走格挡**（没有 `Unblockable`）、不是攻击 —— 走 `TOp::DamagePlayer`。
    //
    // 对求解器的后果是实在的：这场仗里一张能力牌等于再付 6 点血，
    // 薪火之源/恶魔形态那类引擎牌的账要重算。
    PowerDef {
        st: St::Galvanic,
        hook: Hook::CardPlayed,
        ops: &[TOp::If {
            cond: TCond::LastPlayedKindIs(Kind::Power),
            then: &[TOp::DamagePlayer(Amt::Stacks)],
        }],
    },
    // 纸伤难愈（咬人卷轴开局 2 层）：它的攻击**每打穿一段**，我 −2 最大生命。
    //
    // [源码] `PaperCutsPower.AfterDamageGiven`：`dealer == Owner && target.IsPlayer
    // && props.IsPoweredAttack() && result.UnblockedDamage > 0` ->
    // `CreatureCmd.LoseMaxHp(target, Amount)`。四卷咀嚼 5×2 全打穿就是一回合 −16 上限，
    // **而且带出这场仗** —— 整幕链会把它传给下一场（`synth::act`）。
    PowerDef {
        st: St::PaperCuts,
        hook: Hook::AttackUnblocked,
        ops: &[TOp::PlayerLoseMaxHp(Amt::Stacks)],
    },
    // 剧痛刺击（实验体阶段 2 自带 1 层）：它的攻击每打穿一段，往我弃牌堆塞 1 张伤口。
    //
    // [源码] `PainfulStabsPower.AfterAttack`：`command.Attacker == Owner` 且是攻击，
    // 数这条攻击命令里 `UnblockedDamage > 0` 的段数，塞 `Amount × 段数` 张伤口进弃牌堆。
    // 逐段各塞 `Amount` 张和它给出同一个总数。
    //
    // 2026-09-13 之前它在 `KNOWN_UNMODELLED` 里挂着「欠『这一下打穿了多少』传进钩子」——
    // 纸伤难愈要的正是同一个钩子（`Hook::AttackUnblocked`），它是那个钩子的第二个消费者。
    PowerDef {
        st: St::PainfulStabs,
        hook: Hook::AttackUnblocked,
        ops: &[TOp::AddCardToDiscard { card: card::WOUND, count: 1 }],
    },
    // 恶咒（幽灵骑士）：[源码] `HexPower.AfterDeath` —— **施咒者**死了（`creature == Applier`）
    // 就把恶咒从我身上摘掉，邪咒随之清掉、牌不再虚无。
    //
    // 施咒者是谁观测里没有（恶咒挂在**我**身上，`Applier` 不报），所以规则挂在骑士身上
    // 一个**私有标记**上（`St::HexCaster`，按名字从 `ENEMY_PRIVATE_MARKERS` 来）：
    // `EnemyDied` 只让死的那一只自己触发，于是杀掉连枷骑士不会解咒。
    // 一场只有一只幽灵骑士，"施咒者死了"和"幽灵骑士死了"是同一件事。
    PowerDef {
        st: St::HexCaster,
        hook: Hook::EnemyDied,
        ops: &[TOp::PlayerClearStatus(St::Hex)],
    },
    // 抑制（魔法骑士）：[源码] `DampenPower.AfterDeath` —— 施咒者集合空了就移除，
    // `AfterRemoved` 把降过级的牌逐张升回去。一场只有一只魔法骑士，集合就是它自己。
    PowerDef {
        st: St::DampenCaster,
        hook: Hook::EnemyDied,
        ops: &[TOp::RestoreDampenedCards, TOp::PlayerClearStatus(St::Dampen)],
    },
    // 抱抱先生「在你的回合开始时，对所有敌人造成等量于当前回合数的伤害」
    // [源码] `MrStruggles.AfterPlayerTurnStart`：
    // `CreatureCmd.Damage(HittableEnemies, TurnNumber, ValueProp.Unpowered, ...)`
    //
    // 数值用 `Amt::TurnNumber` 而不是「层数 1 + GrowSelf(1)」，理由写在
    // `Amt::TurnNumber` 的注释里（相位不该靠跨帧携带）。
    //
    // `Unpowered` 有两个后果，都实测过（第2幕第27层）：**不吃力量**，
    // 而且**不算"攻击"** —— 所以它不触发蜂群术士的人体蜂房。
    // 内核这边前者自动成立（`DamageAllEnemies` 走 `hit_enemy_with` 的平值路径），
    // 后者暂时无所谓，因为人体蜂房本来就没建模。
    PowerDef {
        st: St::MrStruggles,
        hook: Hook::TurnStart,
        ops: &[TOp::DamageAllEnemies(Amt::TurnNumber)],
    },
    // 舵盘「在你的第三回合开始时，获得18点格挡」
    // [源码] `CaptainsWheel.AfterBlockCleared`：`TurnNumber == 3` ⇒
    // `GainBlock(18, ValueProp.Unpowered)`。
    //
    // 挂 `TurnStart` 是对的：格挡正是在我的回合开始时清零的，而源码的钩子
    // 就叫 `AfterBlockCleared`。走 `TOp::OwnerBlock` 而不是 `card_block`，
    // 于是 `Unpowered` 的两个后果（**不吃脆弱、也不吃臂甲的首次翻倍**）
    // 自动成立 —— 和药水格挡同一条路。
    // [实测] 第2幕第29层回合3：格挡从 0 变 18。
    PowerDef {
        st: St::CaptainsWheel,
        hook: Hook::TurnStart,
        ops: &[TOp::If { cond: TCond::TurnIs(3), then: &[TOp::OwnerBlock(Amt::Stacks)] }],
    },
    // 地精之角「每当有一名敌人死亡时，获得1点能量并抽1张牌」
    // [源码] `GremlinHorn`：`EnergyVar(1)` + `CardsVar(1)`，条件是
    // `target.Side != Owner.Side`（死的是敌方）—— 内核的 `EnemyDied` 只在
    // 敌人身上发，条件自动成立。
    // 层数固定 1，所以数值写 `Fixed` 不是 `Stacks`。
    PowerDef {
        st: St::GremlinHorn,
        hook: Hook::EnemyDied,
        ops: &[TOp::OwnerEnergy(Amt::Fixed(1)), TOp::OwnerDraw(Amt::Fixed(1))],
    },
    // 再生 —— 和覆甲同形状：回合结束时结算，然后自己掉 1 层。
    // 关键词原文：「再生会在你的回合结束时回复相应生命。每回合再生的数值会减少1。」
    // 回血在敌人出手**之前**（`TurnEnd` 就在 `enemy_turn` 前面）。
    // 实录（第1幕 Boss 第5回合）两种时序算出来的血量一样，分不开，按卡面写。
    PowerDef {
        st: St::Regen,
        hook: Hook::TurnEnd,
        ops: &[TOp::OwnerHeal(Amt::Stacks), TOp::GrowSelf(-1)],
    },
    // 狂怒：本回合内每打出一张攻击牌获得 N 格挡
    PowerDef { st: St::Frenzy, hook: Hook::PlayerAttack, ops: &[TOp::OwnerBlock(Amt::Stacks)] },
    // 蜷身（**敌人**持有）：第一次被命中获得 N 点格挡，然后自己消失。
    //
    // [实测] 2026-08-17 第2幕31层 虱虫之祖：`CURL_UP_POWER=14`，
    // 闪电霹雳+ 打了 7 点（134→127）**之后**格挡才变 14，且状态当场消失。
    // 所以：伤害先落地、层数就是格挡值、一次性。
    //
    // `ClearSelf` 放在最后一条 —— 清零之后 `Stacks` 就取不到值了。
    //
    // **两处未实测，都选了不高估玩家的那边**：
    // 1. 非攻击伤害（滚石/火焰屏障反伤）算不算"被命中"—— 这里算，
    //    于是敌人更早拿到格挡，对玩家更不利。
    // 2. 伤害被格挡完全吃掉时算不算 —— 这里算（`hit_enemy_with` 一进来就触发）。
    PowerDef {
        st: St::CurlUp,
        hook: Hook::EnemyDamaged,
        ops: &[TOp::OwnerBlock(Amt::Stacks), TOp::ClearSelf],
    },
    // 惊逃：回合结束随机打出手牌里 N 张攻击牌。
    // [源码] `StampedePower.BeforeTurnEnd` —— 在**我的**回合结束、敌人行动之前，
    // 所以打出来的伤害这一回合就落地。
    PowerDef {
        st: St::Stampede,
        hook: Hook::TurnEnd,
        ops: &[TOp::AutoPlayRandomAttacksFromHand(Amt::Stacks)],
    },
    // 杂耍：每回合第 3 张攻击牌打出时复制 N 份进手牌。
    // [源码] 是 `== 3` 的相等判断（不是每 3 张），且计数每回合清零，
    // 所以一回合只触发一次 —— `attacks_played` 正好是每回合清零的那个计数器。
    PowerDef {
        st: St::Juggling,
        hook: Hook::PlayerAttack,
        ops: &[TOp::CloneLastAttackToHandAt { nth: 3, amt: Amt::Stacks }],
    },
    // 好勇斗狠：回合开始从弃牌堆随机取 N 张攻击牌进手牌并升级。
    PowerDef {
        st: St::Aggression,
        hook: Hook::TurnStart,
        ops: &[TOp::FetchRandomAttacksFromDiscardUpgraded(Amt::Stacks)],
    },
    // 势不可当：每次获得格挡（>0）对随机一个敌人造成 N 点伤害。
    // [源码] `JuggernautPower.AfterBlockGained`：`amount <= 0` 不触发、
    // 目标是 `Rng.CombatTargets.NextItem(HittableEnemies)`（随机）、
    // 伤害走 `ValueProp.Unpowered`（**不吃力量**）。
    //
    // 这张牌以前进不了表，理由记在「玩家给的判定」里：
    // 「什么算一次『获得格挡』没定论，所以这张牌没进表」。源码把它定死了。
    PowerDef {
        st: St::Juggernaut,
        hook: Hook::GainBlock,
        ops: &[TOp::DamageRandomEnemy(Amt::Stacks)],
    },
    // 巨像的减伤**不在这张表里** —— 它不是「在某个时点多做一件事」，
    // 是改判定，属于伤害管线。消费点在 `damage::apply_modifiers`。
    // 这里只放它的掉层规则：敌人回合结束掉 1 层（[源码] `AfterTurnEnd`
    // 且 `side == CombatSide.Enemy`，所以是**敌人**回合结束，不是我的）。
    //
    // 内核没有「敌人回合结束」这个钩子，最接近的语义是我的下一个回合开始 ——
    // 两者之间没有任何我方动作，效果等价。
    PowerDef {
        st: St::Colossus,
        hook: Hook::TurnStart,
        ops: &[TOp::GrowSelf(-1)],
    },
    // 火焰屏障：本回合内每挨一次攻击就反伤攻击者 N 点
    PowerDef {
        st: St::FlameBarrier,
        hook: Hook::Attacked,
        ops: &[TOp::DamageContextEnemy(Amt::Stacks)],
    },
    // 坚定不移**没有触发器**，它是「规则修饰」那一类（见 `RULE_MODIFIERS`）：
    // 消费点是 `damage::card_block` 里那个窄 `if`，和臂甲同一处。
    //
    // 这里原来挂过一条 `TurnStart` 规则（把余额置回层数），**是错的** ——
    // 源码数的是"本回合此前拿过几次卡牌格挡"这个**往上数的计数器**，
    // 不是回合开始上膛的余额，于是回合中途才打出这张牌时它一次都不翻倍。
    // 2026-08-30 第 3 幕第 43 层实录帧 18 抓到，经过写在 `damage::card_block`。
    // 计数器改由 `TURN_SCOPED` 无条件清零。
    // 仪式（虔诚雕刻师的禁忌咒语给自己 9 层）：**每个敌人回合末转化成等量力量**，
    // 而且**层数不减** —— 所以是永久 +9/回合，攻击 12 -> 21 -> 30 -> 39...
    //
    // [源码] `RitualPower.AfterSideTurnEnd`，但真正要紧的是它上面那半：
    //
    //     AfterApplied: if (Owner.IsEnemy) WasJustAppliedByEnemy = true;
    //     AfterSideTurnEnd: if (WasJustAppliedByEnemy) { 清掉; return; }   // 跳过这一次
    //                       else Apply<StrengthPower>(Amount);
    //
    // **施加它的那一个回合末不结算。** 这一条卡面和意图标签都读不出来，
    // 而它**决定第 2 回合挨 12 还是 21**（差一整回合的伤害）。
    //
    // 内核把它反过来表达成"上膛"：回合末先看上没上膛（上了才给力量），
    // 然后无条件上膛。第一次必然没上膛 ⇒ 自动跳过，和源码等价，
    // 而且不需要"else"（`TOp::If` 没有 else）。
    //
    // 2026-08-30 建的。在此之前它在 `KNOWN_UNMODELLED` 里，方向是**乐观**的：
    // 内核会以为这只怪永远只打 12。162 血的怪配 +9/回合，那个误差能打死人。
    PowerDef {
        st: St::Ritual,
        hook: Hook::EnemyTurnEnd,
        ops: &[
            TOp::If {
                cond: TCond::OwnerHas(St::RitualArmed),
                then: &[TOp::OwnerStatus { st: St::Strength, amt: Amt::Stacks }],
            },
            TOp::OwnerSetStatus { st: St::RitualArmed, amt: Amt::Fixed(1) },
        ],
    },
    // 盾墙（活体盾开局自带 25 层）：**我的**回合开始时给场上所有高塔炮手加格挡。
    //
    // [源码] `RampartPower.AfterSideTurnStart`，`side == CombatSide.Player` 才动，
    // 而且筛的是 `c.Monster is TurretOperator` —— **写死的怪物种类，不是"所有队友"**。
    // 所以杀掉活体盾就等于拆掉炮手的格挡引擎，这一场怎么打全靠这条。
    //
    // 2026-08-30 建的。在此之前它在 `KNOWN_UNMODELLED` 里，
    // 实战撞到才发现：`--live` 连提都不提，求解器当它不存在。
    PowerDef {
        st: St::Rampart,
        hook: Hook::TurnStart,
        ops: &[TOp::BlockEnemiesOfDef { def: enemy::TURRET_OPERATOR, amt: Amt::Stacks }],
    },
    // 荆棘。**两条规则，因为两侧的钩子不是同一个**：铜质鳞片挂在我身上、
    // 多刺蟾蜍和蝌蚪挂在它们身上，而"我挨打"和"敌人挨打"是两个时点。
    //
    // 2026-08-29 补的。在此之前 `St::Thorns` 是一颗**哑弹** ——
    // 铜质鳞片（3 层）和多刺蟾蜍（5 层）都在挂它，内核里却没有任何规则读它。
    // 是追第 2 幕 Boss 那一帧时发现的：游戏里碾碎爪挨完打掉 3 血，内核里一点没掉。
    //
    // [源码] `ThornsPower.BeforeDamageReceived`：
    //   门 `props.IsPoweredAttack()` ⇒ **只有攻击算**，药水/遗物/能力牌的伤害不算；
    //   钩子在 `DamageBlockInternal` **之前** ⇒ **格挡挡住也照样反弹**；
    //   反弹 `CreatureCmd.Damage(dealer, Amount, ValueProp.Unpowered)` ⇒ **不吃力量**。
    // 卡面上一条都读不出来。
    //
    // **`StackType = Counter`**（和活力、力量同类），所以是层数不是回合数，
    // 不会自己掉层。
    PowerDef {
        st: St::Thorns,
        hook: Hook::Attacked,
        ops: &[TOp::DamageAttacker(Amt::Stacks)],
    },
    PowerDef {
        st: St::Thorns,
        hook: Hook::EnemyAttacked,
        ops: &[TOp::DamageAttacker(Amt::Stacks)],
    },
    // 凋萎存在（永世沙漏开局挂在**它自己**身上 6 层，凋萎塞进我的手牌）。
    // [源码] `WitheringPresencePower`：
    //
    //     CardsLeft--;
    //     if (CardsLeft <= 0) { AddToCombat<Wither>(Hand, 1); CardsLeft = 6; }
    //
    // 三条按源码逐字照抄，顺序不能动：
    //   1. 先减 1 —— 所以**打出的那第 6 张牌自己也算数**
    //   2. 判 `<= 0` 而不是 `== 0`
    //   3. 塞的是**手牌**（剧烈增强塞的是弃牌堆，两条别搞混）
    //
    // 层数是观测量（游戏面板上那个数），所以它同时进了 `replay::ALL_ST` ——
    // 那一列守着"每打一张牌该减 1"这一半，`AddCardToHand` 守着另一半。
    PowerDef {
        st: St::WitheringPresence,
        hook: Hook::CardPlayed,
        ops: &[
            TOp::GrowSelf(-1),
            TOp::If {
                cond: TCond::SelfStacksAtMost(0),
                then: &[
                    TOp::AddCardToHand { card: card::WITHER, count: 1 },
                    TOp::SetSelf(6),
                ],
            },
        ],
    },
];

/// 只在「这个回合」内有效的 status，在**我的下一个回合开始时**清零。
///
/// 清的时机不能提前到回合结束：火焰屏障要挡的正是敌人回合那几下。

/// **战斗中可以被"生成"出来的牌**（添柴、地狱之刃、技能药水那一类）。
///
/// 规则照 [源码] `CardFactory.FilterForCombat`：
/// ```csharp
/// c.CanBeGeneratedInCombat && Rarity != Basic && Rarity != Ancient && Rarity != Event
/// ```
/// 再叠上 `Character.CardPool` —— 生成池是**角色自己的牌池**，
/// 所以无色牌、状态/诅咒牌都不在里面。
///
/// 按这条规则，铁甲战士的真实生成池是 **81 张**
/// （全集 87 − Basic 3 − Ancient 2 − 狂宴 1）。狂宴自己
/// `CanBeGeneratedInCombat => false`，否则可以无限刷最大生命。
///
/// **这张表是那 81 张和内核已实现的牌的交集，目前 54 张 —— 这是一个已知近似。**
/// 内核认识的牌越多它越接近真实池子，补牌会让它自己收敛。在补满之前，
/// 生成出来的牌**分布偏向已实现的那些**，用它估值时要知道这一点。
///
/// 过滤的是**规则**不是**数值**，所以它比数值稳定：数值会被平衡改动，
/// 「Basic 不能被生成」这种规则不会。
pub static GEN_POOL: &[u16] = &[
    card::DISMANTLE,           // 拆卸
    card::ASH_STRIKE,          // 灰烬打击
    card::DEMON_FLAME,         // 恶魔之焰
    card::LIGHTNING,           // 闪电霹雳
    card::BULLY,               // 欺凌
    card::BRAND,               // 烙印
    card::DOMINATE,            // 主宰
    card::STAMPEDE,            // 踩踏
    card::DEMON_FORM,          // 恶魔形态
    card::RUTHLESS,            // 无情猛攻
    card::HEMOKINESIS,         // 御血术
    card::SETUP_STRIKE,        // 预备打击
    card::PYRE,                // 薪火之源
    card::FEEL_NO_PAIN,        // 无惧疼痛
    card::DARK_EMBRACE,        // 黑暗之拥
    card::RUPTURE,             // 撕裂
    card::CRIMSON_MANTLE,      // 绯红披风
    card::VICIOUS,             // 凶恶
    card::COMBUST,             // 燃烧
    card::IRON_WAVE,           // 铁斩波
    card::SHRUG_IT_OFF,        // 耸肩无视
    card::HEAVY_BLADE,         // 重锤
    card::UPPERCUT,            // 上勾拳
    card::POMMEL_STRIKE,       // 剑柄打击
    card::TWIN_STRIKE,         // 双重打击
    card::FLEX_TAUNT,          // 挑衅
    card::IMMOLATE,            // 焚烧
    card::SHIVER,              // 战栗
    card::BLOOD_WALL,          // 血墙
    card::BREAKTHROUGH,        // 突破
    card::BLOODLETTING,        // 放血
    card::OFFERING,            // 祭品
    card::BODY_SLAM,           // 全身撞击
    card::ROCK_ARMOR,          // 岩石铠甲
    card::MOLTEN_FIST,         // 熔融之拳
    card::TORTURE,             // 凌虐
    card::FRENZY,              // 狂怒
    card::FLAME_BARRIER,       // 火焰屏障
    card::EVIL_EYE,            // 邪眼
    card::FORGOTTEN_RITE,      // 被遗忘的仪式
    card::RESENTMENT,          // 怨恨
    card::PERFECTED_STRIKE,    // 完美打击
    card::RAMPAGE,             // 暴走
    card::BARRICADE,           // 壁垒
    card::SWORD_BOOMERANG,     // 飞剑回旋镖
    card::EXPECT_A_FIGHT,      // 跃跃欲试
    card::JUGGERNAUT,          // 势不可当
    card::COLOSSUS,            // 巨像
    card::SECOND_WIND,         // 重振精神
    card::BATTLE_TRANCE,       // 战斗专注
    card::BURNING_PACT,        // 燃烧契约
    card::ARMAMENTS,           // 武装
    card::HEADBUTT,            // 头槌
    card::ANGER,               // 愤怒
    card::STOKE,               // 添柴
    card::INFERNAL_BLADE,      // 地狱之刃
    card::HAVOC,               // 破灭
    card::STAMPEDE_CARD,       // 惊逃
    card::ONE_TWO_PUNCH,       // 连环拳
    card::JUGGLING,            // 杂耍
    card::AGGRESSION,          // 好勇斗狠
    card::CINDER,              // 余烬
    card::TRUE_GRIT,           // 坚毅
    card::THRASH,              // 痛殴
    card::PILLAGE,             // 劫掠
    card::PRIMAL_FORCE,        // 原始力量
    card::WHIRLWIND,           // 旋风斩
    card::CASCADE,             // 倾泻
    card::DRUM_OF_BATTLE,      // 战鼓
    card::TEAR_ASUNDER,        // 扯碎
];

/// 从生成池里随机取一张。`attack_only` 给地狱之刃用。
///
/// **不追求和游戏的随机序列一致，也做不到。** 游戏用的是
/// `System.Random(seed + hash(流名))` 外加一个存进存档的 `Counter`，
/// 而 `System.Random` 的算法在 .NET 版本之间换过实现；再加上 `Counter`
/// 是观测里没有的运行时状态。仓库本来就承认「抽牌/洗牌结构性不可对拍」，
/// 生成牌是同一类东西。
///
/// 这不影响对拍：`replay::sync` 每帧用观测覆盖手牌，游戏生成了什么就是什么。
/// 它只影响 L2 的前向推演，而那本来就是**一个样本**（`Line::drew` 会标出来）。
/// 从生成池里随机取一张**指定类型**的牌。
///
/// `GEN_POOL` 是真实生成池（[源码] `FilterForCombat` 算出来 81 张）与内核
/// **已实现**的交集，所以按类型筛之后池子更小 —— 生成的分布偏向已实现的那些，
/// 这一点在 `GEN_POOL` 的注释里已经写着，按类型取只是让它更明显。
pub fn random_generated_card_of(rng: &mut u64, kind: Kind) -> Option<u16> {
    let mut n = 0usize;
    for &id in GEN_POOL {
        if card(id).kind == kind {
            n += 1;
        }
    }
    if n == 0 {
        return None;
    }
    let mut k = crate::state::next_below(rng, n);
    for &id in GEN_POOL {
        if card(id).kind == kind {
            if k == 0 {
                return Some(id);
            }
            k -= 1;
        }
    }
    None
}

pub fn random_generated_card(rng: &mut u64, attack_only: bool) -> Option<u16> {
    let mut n = 0usize;
    for &id in GEN_POOL {
        if !attack_only || matches!(card(id).kind, Kind::Attack) {
            n += 1;
        }
    }
    if n == 0 {
        return None;
    }
    let mut k = crate::state::next_below(rng, n);
    for &id in GEN_POOL {
        if !attack_only || matches!(card(id).kind, Kind::Attack) {
            if k == 0 {
                return Some(id);
            }
            k -= 1;
        }
    }
    None
}

pub static TURN_SCOPED: &[St] = &[
    St::Frenzy,
    St::FlameBarrier,
    St::Entrench,
    St::NoDraw,
    St::OneTwoPunch,
    // 污染：[源码] `TaintedPower.AfterSideTurnEnd` 在**敌人回合结束时整个移除**
    //（不是每回合掉一层）。`TURN_SCOPED` 清的时点是**我的回合开始**，
    // 紧接在敌人回合之后 —— 对一切可观测的量完全等价。
    St::Tainted,
    // 坚定不移的计数器（本回合已经从卡牌拿过几次格挡）。
    // **必须无条件清零** —— 只在身上有坚定不移时才重置的话，上一回合攒下的
    // 数会跨进"这一回合中途才拿到坚定不移"的那个回合。见 `damage::card_block`。
    St::UnmovableCharge,
    // 胆小（花园幽灵鳗）每回合重置
    St::SkittishTriggered,
    // 跃跃欲试的「本回合不能再获得能量」。[源码] 是 `AfterSideTurnEnd` 移除，
    // 这里清在我的下一个回合开始 —— 等价，理由见 `St::NoEnergyGain`。
    St::NoEnergyGain,
];

/// **规则修饰**：不是触发器，消费点是 `step.rs` 里写死的窄 `if`。
///
/// 触发器只能"在某个时点多做一件事"，表达不了"不要做某件事"（壁垒不清格挡、
/// 均衡不弃手牌）或"直接改判定"（孤注一掷立刻死）。
///
/// 这张表**不带行为**，它的作用是让这个类别可枚举：完整性测试靠它区分
/// "故意写死的规则修饰"和"忘了写规则的哑弹能力牌"。加一个就要在
/// `step.rs` 里同步加它的消费点，否则测试会放行一张什么都不做的牌。
pub static RULE_MODIFIERS: &[St] = &[
    St::RedSkull,
    St::RedSkullActive,
    // 知识恶魔的两个诅咒（2026-09-14）。心灵腐化：`step::open_hand` 少抽；
    // 懒惰：`step::cards_locked` + `step` 的 `PlayCard` 拒绝第 N+1 张。
    St::MindRot,
    St::Sloth,
    St::Barricade,
    St::Entrench,
    St::AllOrNothing,
    St::NoDraw,
    St::OneTwoPunch,
    // 臂甲的充能。消费点在 `damage::card_block` 那个窄 `if`（不在 `step.rs`，
    // 但类别是同一个：写死的判定，不是触发器）。
    //
    // **2026-08-29 才登记进来**：它一直是这一类，只是没人登记 ——
    // 老的完整性测试只问「能力牌挂的 status 有没有规则」，而臂甲是遗物，
    // 从那个口子漏出去了。`no_status_handed_out_by_a_relic_or_an_enemy_is_a_dud`
    // 把口径放宽之后当场把它点出来了（同一条测试点出的另一个是真哑弹：荆棘）。
    St::VambraceCharge,
    // 坚定不移的本体和它的计数器（「本回合已经从卡牌拿过几次格挡」）。
    // 消费点和臂甲是同一处（`damage::card_block` 那个窄 `if`），不是触发器 ——
    // 所以本体登记在这里而不是 `POWERS`。
    St::Unmovable,
    St::UnmovableCharge,
    // 失衡：持有者造成的伤害被完全挡住时置位 `OffBalance`。
    // 消费点在 `step.rs::take_attack_hit` 的窄 if
    St::Imbalanced,
    // 「它这次真的被完全挡住了」。消费点是敌人 AI 的 `ECond::OffBalance`
    St::OffBalance,
    // 胆小：受到未被格挡的卡牌攻击时，获得等量格挡。消费点在 `step.rs::hit_enemy_with`
    St::Skittish,
    St::SkittishTriggered,
    // 钻地（地道虫）：敌人回合开始格挡不清零，格挡被破时眩晕。消费点在 `step.rs::begin_enemy_turn` / `step.rs::hit_enemy_with`
    St::Burrowed,
    // 钢笔尖的在场标记和跨战斗攻击计数器。消费点是
    // `step::resolve_played_card` 里出牌**结算之前**那个窄 `if`（数第 10 张攻击、
    // 挂翻倍标记）—— 和臂甲/坚定不移同一类：写死的判定，不是触发器。
    // 翻倍标记本身在 `PIPELINE_STATUSES` 里（它是伤害管线的一个乘区）。
    St::PenNib,
    St::PenNibCount,
    // 损毁头盔：本场**第一次**获得力量时层数 ×2。消费点是 `step::apply_status`
    // 里一个只认 status 的窄 `if` —— 和臂甲/坚定不移同一类（`ModifyXxx` 那一族，
    // 遗物把状态交给玩家实体，规则那一侧一个 `if 遗物` 都没有）。
    St::RuinedHelmet,
    // 恶咒（幽灵骑士挂在我身上）：手里每一张没有回合末发作效果的牌都是虚无。
    // 消费点是 `step::resolve_hand_end` 的窄判断 —— 它改的是"回合末消不消耗"这条判定。
    St::Hex,
];

/// **伤害管线读的 status**：乘区、上限、朝向、格挡翻倍。
///
/// 它们不是触发器（`POWERS` 里没有它们的规则），消费点全在
/// [`crate::damage`] 的 `apply_modifiers` / `card_block` 和 `step.rs` 里
/// 那几个窄 `if`。伤害管线的顺序在 `docs/design-l1.md` 里，**改数字先去看那张表**。
///
/// 这张表和 `RULE_MODIFIERS` 一样**不带行为**，作用是让这个类别可枚举 ——
/// `no_status_handed_out_by_a_relic_or_an_enemy_is_a_dud` 靠它区分
/// 「写死在管线里的判定」和「忘了写规则的哑弹」。
///
/// **2026-08-29 建这张表，是因为荆棘那颗哑弹**：老的完整性测试只问
/// 「能力牌挂的 status 有没有规则」，遗物和敌人挂的从那个口子漏出去了。
/// 把口径放宽之后，这一组当场被点名 —— 它们都**有**消费点，
/// 只是从来没人登记过。加一个进来之前先确认：管线里真的有人读它吗？
pub static PIPELINE_STATUSES: &[St] = &[
    St::StrikeDummy,
    St::Shrink,        // 攻击方缩小 ×2/3（damage.rs）
    St::Slow,          // 防御方缓慢 ×(1 + 0.1×本回合此牌之前打出的牌数)
    St::SlowSource,    // 「谁带缓慢、每张牌几个百分点」，step.rs 的窄 if
    St::Colossus,      // 攻击方带易伤时伤害减半
    St::Surrounded,    // 遭到包围：背后打来的 ×1.5
    St::BackAttackLeft,  // 敌人在我的哪一侧
    St::BackAttackRight,
    St::FacingRight,   // 我朝哪边（内核私有，游戏不报，`replay::infer_facing` 反推）
    St::Flutter,       // 振翅
    St::Soar,          // 翱翔
    St::Tainted,       // 污染
    // 钢笔尖的翻倍标记。`damage::apply_modifiers` 里和上面这些同一个循环，
    // 上下标记怎么来的见 `St::PenNibArmed`
    St::PenNibArmed,
    // 硬化外壳的余额：`damage::absorb` 里封掉血、扣余额（回满那一半是 `POWERS` 里上限那条规则）
    St::HardenedShell,
];

/// **纯标记 status**：游戏把它显示成一个 power，但效果在**打出那一刻就结算完了**，
/// 之后这个 status 不再做任何事。
///
/// 薪火之源就是这样：它改的是能量上限（`Op::GainMaxEnergy`），
/// `PYRE_POWER` 只是个显示用的印记。内核照样把它挂上去，纯粹为了和观测对齐 ——
/// 不挂的话 status 逐字段比较会报出一个和规则无关的差异。
///
/// 加进这张表之前先确认：它真的什么都不做吗？还是你**漏了**一条规则？
/// 完整性测试靠这张表放行它们，放错了就等于给自己开了个后门。
pub static MARKER_STATUSES: &[St] = &[
    St::Pyre,
    // 抑制（魔法骑士挂在我身上）。效果在**挂上的那一刻**就结算完了
    // （`EOp::DowngradeUpgradedCards` 降级并打上 `F_DAMPENED`），之后它只是个印记；
    // 解除走施咒者身上 `St::DampenCaster` 那条规则。
    St::Dampen,
    // 虚脱（知识恶魔的诅咒）。效果在**选中的那一刻**就落在 `State::base_energy` 上了
    // （`EOp::CurseOfKnowledge`，和薪火之源同一个做法），之后它只是个印记。
    St::WasteAway,
    // 偷窃 / 盗窃（地精佣兵一族）。[源码] 两个都只动**金币**（`PlayerCmd.LoseGold` /
    // 死时 `AddExtraReward(GoldReward)`），而金币不在战斗层 —— 对一场仗的结局它们真的什么都不做。
    // 挂着纯粹为了和观测对齐（不映射的话整帧的 status 报「没检查」）。
    St::Thievery,
    St::Heist,
];

/// **已知但故意不建模**的 status。
///
/// 它们存在的唯一理由是让观测里的这些 power 从「未知 status」变成
/// 「已知但不建模」—— 未映射的 status 会把整帧降级成 `UNKNOWN_CONTENT`，
/// 于是同一帧里的**真错误**跟着一起被藏起来（第 1 幕 Boss 那一帧就是这么
/// 盖住两个真 bug 的）。声明见 `state.rs` 里那一段。
///
/// **这张表是欠账清单，不是白名单。** 每加一条就是承认一处内核看不见的机制；
/// 加之前先问一句：是真的不建，还是只是还没建？
/// 荆棘在这张表里躺过 —— 挂着、观测对得上、没有任何规则读它，
/// 内核因此少算了每一次反弹伤害，而**没有任何测试会发现**。
/// 2026-08-29 建掉了。
pub static KNOWN_UNMODELLED: &[St] = &[
    // 只是个标签：影响斩杀能不能触发、召唤者死爪牙跟着死。两条都没建。
    // （`step.rs` 里读它只是为了「非爪牙全死光就结束战斗」和选目标。）
    St::Minion,
    St::EscapeArtist,
    St::Swipe,
    // ---- 下面这一组是 2026-08-29 这条测试第一次点出来的。
    //      它们**各自卡在一个还没有的机制上**，说明写在 `state.rs` 的声明里。
    //      在此之前这份欠账**不可枚举** —— 只有读遍 state.rs 才知道欠了什么。
    // 结实的卵：掉到 0 层原地变成幼虫。欠 `EOp::TransformSelf`
    St::Hatch,
    // 侵蚀（活雾）：打出技能牌时让技能牌染上瓦斯。欠「手牌/牌库卡牌附魔修饰」
    St::Smoggy,
];

/// 回合结束时还留在手牌里才发作的牌。见 `HandEndDef`。
///
/// 伤害全部走 `Op::TakeDamage`（**过格挡**，玩家确认），
/// 不是 `Op::LoseHp`（直接扣血、能唤醒撕裂）。
pub static HAND_END: &[HandEndDef] = &[
    // 灼伤「不能被打出。 在你的回合结束时，如果这张牌在你的手牌中，你受到2点伤害。」
    HandEndDef { card: card::BURN, ops: &[Op::TakeDamage(2)], void: false },
    // 呼唤 6 点。**它和灼伤那一族不是同一条 op**：
    // [源码] `Beckon.OnTurnEndInHand` 是 `CreatureCmd.Damage(6, Unblockable |
    // Unpowered | Move)`，而 `Burn.OnTurnEndInHand` 那一句**没有 `Unblockable`**。
    // 所以灼伤走 `Op::TakeDamage`（格挡吃得掉），呼唤走 `Op::LoseHp`（吃不掉）。
    // [实测] 2026-09-09 `act1_f17_boss_soul_fysh` 帧11：身上 8 点格挡，
    // 游戏照样掉 6 血 —— 第一版写成 `TakeDamage` 当场红一帧。
    HandEndDef { card: card::BECKON, ops: &[Op::LoseHp(6)], void: false },
    // 感染 —— 同形状，3 点
    HandEndDef { card: card::INFECTION, ops: &[Op::TakeDamage(3)], void: false },
    // 毒素 5 点（[源码] `Toxic.OnTurnEndInHand` -> `CreatureCmd.Damage`，
    // `Unpowered | Move` ⇒ **过格挡**，和灼伤同路）。
    // `void: false` —— 发作之后它**留在手上**，下回合还会再来一次；
    // 卡面那个「消耗」是**打出时**的关键字，不是发作时的。
    HandEndDef { card: card::TOXIC, ops: &[Op::TakeDamage(5)], void: false },
    // 腐朽（诅咒）2 点
    HandEndDef { card: card::DECAY, ops: &[Op::TakeDamage(2)], void: false },
    // 羞耻（诅咒）给自己 1 层脆弱
    HandEndDef {
        card: card::SHAME,
        ops: &[Op::Status { tgt: Tgt::Me, st: St::Frail, amt: 1 }],
        void: false,
    },
    // 瓦解 6 点。**卡面没写"如果这张牌在你的手牌中"**（其他四张都写了），
    // 这里仍按在手牌里处理 —— 状态牌本来就是靠占手牌恶心人的。未实测。
    HandEndDef { card: card::DISINTEGRATION, ops: &[Op::TakeDamage(6)], void: false },
    // 虚无：「如果这张牌在这个回合结束时留在你的手牌中，则将其消耗。」
    // 凋萎「不能被打出。 在你的回合结束时，如果这张牌在你的手牌中，你受到3点伤害。」
    // [源码] `Wither.OnTurnEndInHand` -> `CreatureCmd.Damage(自己, 3, Unpowered|Move)`。
    // **不虚无**（`HasTurnEndInHandEffect` 只管发作、不管消失），所以 `void: false`
    // —— 和灼伤同构，它会一直在牌组里循环。
    //
    // **这个 3 是基础值，实际伤害是 `3 + CardInst::bonus`**：永世沙漏的
    // 剧烈增强会把每一张凋萎 `FakeUpgrade()`（各 +3），游戏把层数写进牌名
    // （`凋萎+1` / `凋萎+2`），`replay::lookup_card` 解析它并灌进 `bonus`。
    // 表里其它几张 `TakeDamage` 的状态牌 bonus 恒为 0，不受影响。
    HandEndDef { card: card::WITHER, ops: &[Op::TakeDamage(3)], void: false },
    HandEndDef { card: card::DAZED, ops: &[], void: true },
    HandEndDef { card: card::CLUMSY, ops: &[], void: true },
];

// ---------------- 遗物 ----------------
//
// **这张表只放"内核对它有话可说"的遗物**，不是 55 个全灌。
// 权威全表在 `traces/relics_catalog.json`（`tools/dump_relics.py` 导出，
// 55/55 全部拿到），要建模时从那里抄，不用再联网。
//
// 为什么不全灌：上一次一次性灌 54 张牌，结果本文档自己写着
// 「没验证过的内容已经是验证过的三倍多，这个比例本身就是风险」。
// 遗物按"这一局身上有的 + 以后真遇到的"增量加。
//
// 表里没有的遗物**不会被当成没效果** —— `--live` 会拿观测里的名字把它们
// 点名报出来（见 `bin/solve.rs`）。内核必须能说出"我不认识这个"。

pub static RELICS: &[RelicDef] = &[
    RelicDef {
        id: "MANGO", name: "芒果",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "[源码] 拾取时最大生命 +14；战斗观测已含，不重复施加",
    },
    RelicDef {
        id: "BING_BONG", name: "宾邦",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "[源码] 新加入主牌组的牌复制一张；局外效果，主牌组快照已含",
    },
    // ---- 局外：不属于战斗层，内核不欠它们什么，所以 modelled = true ----
    RelicDef {
        id: "SILVER_CRUCIBLE", name: "白银熔炉",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：前3次卡牌奖励升级 + 第一个宝箱为空。战斗层无关",
    },
    RelicDef {
        id: "PAELS_WING", name: "佩尔之翼",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：卡牌奖励献祭换遗物。战斗层无关",
    },
    RelicDef {
        id: "ARCANE_SCROLL", name: "奥术卷轴",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：拾起时把一张随机稀有牌加进牌组。战斗层无关",
    },
    RelicDef {
        id: "WAR_PAINT", name: "战纹涂料",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：拾起时随机升级 2 张技能牌。升级结果进牌组，战斗层无关",
    },
    RelicDef {
        id: "WHETSTONE", name: "磨刀石",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：拾起时随机升级 2 张攻击牌。升级结果进牌组，战斗层无关",
    },
    RelicDef {
        id: "MEAL_TICKET", name: "餐券",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：进商店回 15 点生命。战斗层无关",
    },
    RelicDef {
        id: "STORYBOOK", name: "故事书",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：拾起时把 1 张至亮之焰加进牌组。那张牌本身在内容表里",
    },
    // [源码] `LetterOpener`：同回合每 3 张技能牌 -> 全体 5 点（Unpowered）。
    // 层数 = 伤害；「每 3 张」写在规则的 TCond 里。
    RelicDef {
        id: "LETTER_OPENER", name: "开信刀",
        start_status: &[], private_status: &[(St::LetterOpener, 5)],
        counter_to: None, modelled: true,
        note: "同回合每 3 张技能牌对全体 5 点。层数=伤害，计数在 State::skills_played",
    },
    // ---- 2026-08-27 第二批：战斗层，规则在 POWERS ----
    RelicDef {
        id: "ANCHOR", name: "锚",
        start_status: &[], private_status: &[(St::Anchor, 10)],
        counter_to: None, modelled: true,
        note: "战斗开始 10 点格挡（Unpowered）。层数=格挡值",
    },
    // 假商人卖的「锚？？？」。**id 不同**（`FAKE_ANCHOR`），数值也不同：
    // [源码] `FakeAnchor` 是 `BlockVar(4)`，真锚是 10。规则完全一样，
    // 所以复用同一个 status，只是层数填 4 —— 层数就是效果值，这正是它的用处。
    //
    // **假遗物必须单独进表，不能当成本体**：名字带「？？？」、id 带 `FAKE_`，
    // 数值一律更差。把它映射到真锚会让内核每场多算 6 点格挡。
    RelicDef {
        id: "FAKE_ANCHOR", name: "锚？？？",
        start_status: &[], private_status: &[(St::Anchor, 4)],
        counter_to: None, modelled: true,
        note: "假商人版：战斗开始 4 点格挡（真锚是 10）",
    },
    // 2026-08-30：权威表重导（93 -> 94）之后补齐的 5 件。
    // 四件假货 + 坦克斯的哨子（这一局身上就带着，`--live` 一直在报"内容表里没有"）。
    RelicDef {
        id: "TANXS_WHISTLE", name: "坦克斯的哨子",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：拾起时把 1 张吹哨加进牌组。那张牌进牌组之后就是普通牌，战斗层无关",
    },
    RelicDef {
        id: "FAKE_BLOOD_VIAL", name: "小血瓶？？？",
        start_status: &[], private_status: &[(St::BloodVial, 1)],
        counter_to: None, modelled: true,
        note: "假商人版：开局回 1 血（真品 2）。和真品同一条规则，层数不同",
    },
    RelicDef {
        id: "FAKE_HAPPY_FLOWER", name: "开心小花？？？",
        start_status: &[], private_status: &[(St::FakeHappyFlower, 1)],
        counter_to: None, modelled: true,
        note: "假商人版：每 5 回合 +1 能量（真品每 3 回合）。周期不同，所以单独一条规则",
    },
    RelicDef {
        id: "FAKE_MANGO", name: "芒果？？？",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：拾起时最大生命 +3（真品 +5）。战斗层无关",
    },
    RelicDef {
        id: "FAKE_SNECKO_EYE", name: "异蛇之眼？？？",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "假商人版：每场战斗开始获得混乱（真品还多抽 2 张）。欠**混乱** ——                「手牌费用随机化」要给 CardInst 加一条本场随机费用，内核没有这个概念",
    },
    RelicDef {
        id: "FAKE_VENERABLE_TEA_SET", name: "古茶具套装？？？",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "假商人版：休息后第一场战斗 +1 能量（真品 +2）。和真品同一条路，见 REST_ARMED",
    },
    RelicDef {
        id: "BAG_OF_MARBLES", name: "弹珠袋",
        start_status: &[], private_status: &[(St::BagOfMarbles, 1)],
        counter_to: None, modelled: true,
        note: "第1回合给全体易伤 1",
    },
    RelicDef {
        id: "RED_MASK", name: "红面具",
        start_status: &[], private_status: &[(St::RedMask, 1)],
        counter_to: None, modelled: true,
        note: "第1回合给全体虚弱 1",
    },
    RelicDef {
        id: "BAG_OF_PREPARATION", name: "准备背包",
        start_status: &[], private_status: &[(St::BagOfPreparation, 2)],
        counter_to: None, modelled: true,
        note: "第1回合多抽 2 张",
    },
    RelicDef {
        id: "CANDELABRA", name: "烛台",
        start_status: &[], private_status: &[(St::Candelabra, 2)],
        counter_to: None, modelled: true,
        note: "第2回合开始 +2 能量（源码是 ==2，不是 >=2）",
    },
    RelicDef {
        id: "HAPPY_FLOWER", name: "开心小花",
        start_status: &[], private_status: &[(St::HappyFlower, 1)],
        counter_to: None, modelled: true,
        note: "每 3 回合 +1 能量",
    },
    RelicDef {
        id: "POLLINOUS_CORE", name: "花粉核心",
        start_status: &[], private_status: &[(St::PollinousCore, 2)],
        counter_to: None, modelled: true,
        note: "每 4 回合多抽 2 张",
    },
    RelicDef {
        id: "PAELS_FLESH", name: "佩尔之肉",
        start_status: &[], private_status: &[(St::PaelsFlesh, 1)],
        counter_to: None, modelled: true,
        note: "第3回合起每回合 +1 能量",
    },
    RelicDef {
        id: "PAELS_BLOOD", name: "佩尔之血",
        start_status: &[], private_status: &[(St::PaelsBlood, 1)],
        counter_to: None, modelled: true,
        note: "[源码] `ModifyHandDraw => count + 1`：每回合起手多抽 1 张，无条件",
    },
    RelicDef {
        id: "KUNAI", name: "苦无",
        start_status: &[], private_status: &[(St::Kunai, 1)],
        counter_to: None, modelled: true,
        note: "同回合每 3 张攻击牌 +1 敏捷（第3/6/9张都给）",
    },
    // ---- 2026-08-27 第二批：局外，战斗层不欠它们什么 ----
    RelicDef {
        id: "BOWLER_HAT", name: "圆顶礼帽",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：金币 +25%",
    },
    RelicDef {
        id: "DRAGON_FRUIT", name: "火龙果",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：获得金币时 +1 最大生命",
    },
    RelicDef {
        id: "ETERNAL_FEATHER", name: "永恒羽毛",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：休息处按牌组张数回血",
    },
    RelicDef {
        id: "FUR_COAT", name: "皮草大衣",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：拾起时标记 7 处战斗，那些敌人只有 1 血。改的是遭遇不是规则",
    },
    RelicDef {
        id: "GIRYA", name: "壶铃",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：休息处换力量（最多3次）。换来的力量是观测量，同步就带进来了",
    },
    RelicDef {
        id: "JUZU_BRACELET", name: "佛珠手链",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：问号房不再遇到常规战斗",
    },
    RelicDef {
        id: "LEES_WAFFLE", name: "李家华夫饼",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：拾起时 +7 最大生命并回满",
    },
    RelicDef {
        id: "NEOWS_BONES", name: "涅奥骨骰",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：拾起时给 2 件涅奥遗物 + 1 张随机诅咒",
    },
    RelicDef {
        id: "ORRERY", name: "星系仪",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：拾起时给 5 次卡牌奖励",
    },
    RelicDef {
        id: "PAELS_HORN", name: "佩尔之角",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：拾起时塞 2 张放松。**那张牌内核还没有**，见缺牌清单",
    },
    RelicDef {
        id: "PANTOGRAPH", name: "缩放仪",
        start_status: &[], private_status: &[],
        counter_to: None, modelled: true,
        note: "Boss 房开局回 25 血。规则在 POWERS；'这一场是不是 Boss'走 CONDITIONAL_START",
    },
    RelicDef {
        id: "PHIAL_HOLSTER", name: "药瓶皮套",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：拾起时 +1 药水栏 + 2 瓶随机药水",
    },
    RelicDef {
        id: "REGAL_PILLOW", name: "皇家枕头",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：休息多回 15 血",
    },
    RelicDef {
        id: "SHOVEL", name: "铲子",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：休息处挖遗物",
    },
    RelicDef {
        id: "STRAWBERRY", name: "草莓",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：拾起时 +7 最大生命",
    },
    RelicDef {
        id: "SWORD_OF_STONE", name: "石之剑",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：打完 5 个精英后变成别的遗物",
    },
    RelicDef {
        id: "WINGED_BOOTS", name: "羽翼之靴",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：3 次无视路线",
    },
    RelicDef {
        id: "WING_CHARM", name: "羽翼护符",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：卡牌奖励里随机一张附魔迅捷1",
    },
    RelicDef {
        id: "DELICATE_FROND", name: "娇嫩蕨草",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：每场战斗开始把空药水栏填满。药水本身在内核里，填栏位是局外经济",
    },
    RelicDef {
        id: "CHOSEN_CHEESE", name: "天选芝士",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：战斗结束 +1 最大生命。结算在战斗之后，L1 不欠",
    },
    // ---- 2026-08-27 第二批：战斗层但**还没建**，每条写明卡在哪 ----
    RelicDef {
        id: "BELT_BUCKLE", name: "腰带扣",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "没有药水时 +2 敏捷。欠「身上还有没有药水」这个**动态**条件（喝完当场生效）",
    },
    RelicDef {
        id: "CHEMICAL_X", name: "化学物X",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "X 费牌效果 +2。欠给 X 加常数的那一处（X 在 step 里当场算）",
    },
    RelicDef {
        id: "GHOST_SEED", name: "幽灵种子",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "打击和防御获得虚无。欠**附魔系统**（改的是牌不是玩家）",
    },
    // 下面三件是同一个形状：**效果全在局外**（拾起的那一刻给牌附魔），
    // 战斗层由 `CardInst` 上的附魔承载，而附魔是**观测量**（mod 每帧都报）。
    // 所以内核该不该"建"这几件遗物，取决于它给的那种附魔建全了没有。
    RelicDef {
        id: "GNARLED_HAMMER", name: "扭曲锤子",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：拾起时给至多 3 张攻击牌附魔锋利3。战斗层由 `ENCHANTS` 的 SHARP 承载",
    },
    RelicDef {
        id: "KIFUDA", name: "木札",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "局外：拾起时附魔娴熟（`ADROIT`）—— 而那一种还没建全（欠附魔侧的 OnPlay）",
    },
    RelicDef {
        id: "ROYAL_STAMP", name: "王室印章",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：拾起时附魔王室认证。战斗层由 `ENCHANTS` 的 ROYALLY_APPROVED 承载（固有+保留）",
    },
    RelicDef {
        id: "MYSTIC_LIGHTER", name: "神秘打火机",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "有附魔的攻击牌 +9 伤害。欠附魔系统",
    },
    RelicDef {
        id: "INTIMIDATING_HELMET", name: "骇人头盔",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "打出费用≥2的牌 +4 格挡。欠「刚打出的这张牌费用是多少」这个条件",
    },
    RelicDef {
        id: "PERMAFROST", name: "永冻冰晶",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "本场第一次打出能力牌 +7 格挡。欠「这张是能力牌」的条件（一次性用 ClearSelf 已经能表达）",
    },
    // 钢笔尖：`ModifyXxx` 那一族（"改一个正在算的数值"）**第一件建掉的**。
    //
    // 路线图当初写着这一族"在现有三类表里没有位置，硬加就得往 damage.rs 塞
    // `if 有没有某遗物`，正是不变量 3 禁止的方向"。**出路是不往那边走**：
    // 遗物把自己的状态交给玩家实体（三个私有 status），`damage.rs` 照旧
    // 只认 status —— 和它已有的十个乘区一模一样，一个 `if 遗物` 都没有。
    //
    // 三个 status 分工见 `St::PenNib` / `PenNibCount` / `PenNibArmed`；
    // 计数在 `step::resolve_played_card`（结算之前），乘区在 `damage::apply_modifiers`。
    // 计数器**跨战斗**，所以走 `counter_to` 从观测灌（和摆动球同一条路）。
    RelicDef {
        id: "PEN_NIB", name: "钢笔尖",
        start_status: &[], private_status: &[(St::PenNib, 1)],
        counter_to: Some(St::PenNibCount), modelled: true,
        note: "[源码] 每第 10 张攻击牌 ×2（`ModifyDamageMultiplicative`，带 IsPoweredAttack 门）",
    },
    RelicDef {
        id: "PAPER_PHROG", name: "纸蛙",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "易伤改成 +75%。欠改伤害管线乘区的开关（damage.rs 那处不许随手改）",
    },
    RelicDef {
        id: "RAZOR_TOOTH", name: "剃刀牙",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "打出的攻击/技能牌本场升级。欠「把这张牌实例本场升级」（升级是换 ops，不是加 bonus）",
    },
    RelicDef {
        id: "RED_SKULL", name: "红头骨",
        start_status: &[], private_status: &[(St::RedSkull, 3)], counter_to: None, modelled: true,
        note: "[源码+实测] 血量≤50% 时 +3 力量；回血越过阈值时移除。观测力量已含加成",
    },
    // ---- 2026-09-09 第三批：新一局第 1 幕捡到的六件 ----
    // 三件局外（战斗层不欠它们什么），两件战斗层，一件只在合成路径上欠。
    RelicDef {
        id: "LEAFY_POULTICE", name: "叶子药膏",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：[源码] AfterObtained 最大生命 −12，把一张打击和一张防御各变形成别的牌。
               产物进牌组，战斗层无关",
    },
    RelicDef {
        id: "FROZEN_EGG", name: "冰冻蛋",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：[源码] 卡牌奖励/商店里的**能力牌**自动升级。升级结果进牌组，战斗层无关",
    },
    RelicDef {
        id: "SIGNET_RING", name: "图章戒指",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：[源码] AfterObtained 给 999 金币。战斗层无关",
    },
    // 斗篷扣：[源码] `CloakClasp.BeforeSideTurnEnd` -> `(int)(手牌张数 × 1)` 点格挡。
    // 规则在 POWERS 的 `Hook::TurnEnd`（和尖叫酒壶同一个时点：弃手牌之前）。
    RelicDef {
        id: "CLOAK_CLASP", name: "斗篷扣",
        start_status: &[], private_status: &[(St::CloakClasp, 1)],
        counter_to: None, modelled: true,
        note: "回合结束时每张手牌给 1 点格挡。层数=每张给几点，规则在 POWERS 的 TurnEnd",
    },
    // 号角靴钉：[源码] `HornCleat.AfterBlockCleared` 且 `TurnNumber == 2` -> 14 格挡。
    RelicDef {
        id: "HORN_CLEAT", name: "号角靴钉",
        start_status: &[], private_status: &[(St::HornCleat, 14)],
        counter_to: None, modelled: true,
        note: "第 2 回合开始获得 14 点格挡。层数=格挡，规则在 POWERS 的 TurnStart + TurnIs(2)",
    },
    // 碎石者：[源码] `StoneCracker.AfterRoomEntered(CombatRoom)` ——
    // 从**抽牌堆**随机挑 2 张可升级的牌升级。和风箱同类：对拍路径上牌是观测量
    // （`sync` 照抄升级态），**合成路径没有观测可抄** ⇒ `SYNTH_ONLY_GAPS`。
    RelicDef {
        id: "STONE_CRACKER", name: "碎石者",
        start_status: &[], private_status: &[(St::StoneCracker, 1)],
        counter_to: None, modelled: true,
        note: "[源码] AfterRoomEntered(CombatRoom)：抽牌堆里随机 2 张可升级的牌升级。
               规则在 POWERS 的 TurnStart；对拍路径上牌是观测量，那边不挂",
    },
    RelicDef {
        id: "REPTILE_TRINKET", name: "爬行动物饰品",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "用药水时本回合 +3 力量。欠 Hook::PotionUsed",
    },
    // 损毁头盔 2026-09-09 建了。它欠的「施加 status 时插一手」不需要新钩子 ——
    // 出路和钢笔尖同一条：**遗物把状态交给玩家实体**，规则那一侧只认 status
    // （`step::modify_status_amount_received`，登记在 `RULE_MODIFIERS`）。
    // 「一场只用一次」由 `spent_once_per_combat` 认（用掉当场清零）。
    RelicDef {
        id: "RUINED_HELMET", name: "损毁头盔",
        start_status: &[], private_status: &[(St::RuinedHelmet, 1)],
        counter_to: None, modelled: true,
        note: "[源码] 本场第一次获得力量时层数 ×2（只认给自己的、只认正数）。
               [实测] 2026-09-09 金刚杵 1 点 -> 观测到 2 点",
    },
    RelicDef {
        id: "SLING_OF_COURAGE", name: "勇气投石索",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "精英战 +2 力量。欠「这场是不是精英」——房间类型不在 L1 里",
    },
    RelicDef {
        id: "STRIKE_DUMMY", name: "打击木偶",
        start_status: &[], private_status: &[(St::StrikeDummy, 3)], counter_to: None, modelled: true,
        note: "[源码+实测] CardTag.Strike 的有源攻击 +3，位于乘区之前",
    },
    RelicDef {
        id: "THE_ABACUS", name: "算盘",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "每次洗抽牌堆 +6 格挡。欠 Hook::Shuffle（洗牌已收口在 regularize_and_shuffle_draw，加钩子不难）",
    },
    RelicDef {
        id: "TOOLBOX", name: "工具箱",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "战斗开始从 3 张无色牌里挑 1 张。欠「从生成的候选里选」那种 Pending，和无色药水同一个",
    },
    RelicDef {
        id: "UNSETTLING_LAMP", name: "不安油灯",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "本场第一次给负面状态时效果翻倍。欠「施加 status 时插一手」的钩子",
    },
    // 古茶具 2026-09-09 建了。它欠的「上一个房间是不是休息处」**不在 L1 里，
    // 但在 L3 手上** —— 整幕链自己知道在模拟哪个房间。所以规则进 `POWERS`，
    // 武装那一步走 `REST_ARMED` + `synth::FightSpec::after_rest`。
    // **对拍路径一个字节不变**：那边 `energy` 从观测灌，这个 status 不挂。
    RelicDef {
        id: "VENERABLE_TEA_SET", name: "古茶具套装",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "休息后的第一场战斗 +2 能量。规则在 POWERS 的 TurnStart；
               武装要调用方给「上一场是不是休息处」，见 REST_ARMED",
    },
    // ---- 战斗层，**已建模** ----
    // [源码] `ParryingShield`：我的回合结束、格挡 ≥ 10 -> 随机一只敌人 6 点。
    // 两个数都 Unpowered。规则在 POWERS 的 `Hook::TurnEnd`。
    RelicDef {
        id: "PARRYING_SHIELD", name: "招架盾",
        start_status: &[], private_status: &[(St::ParryingShield, 6)],
        counter_to: None, modelled: true,
        note: "回合结束时格挡≥10 则随机敌人 6 点。层数=伤害，门槛在 TCond 里",
    },
    // [源码] `ScreamingFlagon`：我的回合结束、手牌为空 -> 全体敌人 20 点（Unpowered）。
    // 规则在 POWERS 的 `Hook::TurnEnd`，层数 = 伤害。
    //
    // **[实测] 2026-09-06 第 3 幕 Boss 永世沙漏第 5 回合**：恶魔之焰+ 清空手牌之后，
    // 回合末 Boss 掉 26 = 20（本条）+ 6（荆棘 3 × 眼部激光两段）。
    // 建之前它在 `solve --live` 的「内核看不见」那一栏里挂了很久，
    // 而它和均衡+/添柴+/恶魔之焰+ 的交互恰好是这副牌组每回合都在做的事。
    RelicDef {
        id: "SCREAMING_FLAGON", name: "尖叫酒壶",
        start_status: &[], private_status: &[(St::ScreamingFlagon, 20)],
        counter_to: None, modelled: true,
        note: "回合结束手牌为空则全体敌人 20 点。层数=伤害，时点门槛在 TCond::HandEmpty",
    },
    // [源码] `CentennialPuzzle`：本场**第一次**真掉血 -> 抽 3。
    // **内核只在攻击伤害那条路上点火**（`take_attack_hit`），比游戏窄一点，
    // 方向是保守的（少抽牌）。理由写在 `Hook::PlayerDamaged` 上。
    RelicDef {
        id: "CENTENNIAL_PUZZLE", name: "百年积木",
        start_status: &[], private_status: &[(St::CentennialPuzzle, 3)],
        counter_to: None, modelled: true,
        note: "本场第一次掉血抽 3 张。一次性靠规则里的 ClearSelf",
    },
    // ---- 战斗层，但**还没建模**。每条写明卡在哪 ----
    // [实测] act2_f31 帧1：防御+（基础 8）给出 16。
    // 充能放在 `St::VambraceCharge`，`damage::card_block` 里消耗 —— 那是
    // 「从卡牌获得格挡」的唯一入口，所以药水/能力牌的格挡自动不吃翻倍。
    RelicDef {
        id: "VAMBRACE", name: "臂甲",
        // 充能是**内核私有**的：游戏不报"这场臂甲用过没有"，所以必须 carry。
        start_status: &[], private_status: &[(St::VambraceCharge, 1)],
        counter_to: None,
        modelled: true,
        note: "首次从卡牌获得的格挡翻倍。**和脆弱的先后顺序未实测**，取了不高估玩家的那边",
    },
    // [实测] act1_f14_new_sixth 帧1：熔融之拳+ 打死闪光贾克斯果之后，
    // 能量 0 -> 1、手牌 3 -> 4。补这件之前那一帧是个真 MISMATCH。
    RelicDef {
        id: "GREMLIN_HORN", name: "地精之角",
        // 游戏不把它报成 status ⇒ 私有量，逐帧 carry，不进 ALL_ST
        start_status: &[], private_status: &[(St::GremlinHorn, 1)],
        counter_to: None, modelled: true,
        note: "敌人死亡时 +1 能量并抽 1 张牌。规则在 POWERS 的 Hook::EnemyDied",
    },
    // [实测] 第2幕第24/27层：回合1打1点、回合2打2点、回合3打3点，
    // 对场上**每一只**敌人（含刚被召唤出来的三只卵）。
    RelicDef {
        id: "MR_STRUGGLES", name: "抱抱先生",
        // 游戏不把遗物报成 status ⇒ 私有量，逐帧 carry，不进 ALL_ST
        start_status: &[], private_status: &[(St::MrStruggles, 1)],
        counter_to: None, modelled: true,
        note: "回合开始对全体造成等于回合数的伤害。规则在 POWERS 的 Hook::TurnStart",
    },
    RelicDef {
        id: "CAPTAINS_WHEEL", name: "舵盘",
        start_status: &[], private_status: &[(St::CaptainsWheel, 18)],
        counter_to: None, modelled: true,
        note: "第3回合开始 +18 格挡。Unpowered，所以不吃脆弱也不吃臂甲翻倍",
    },
    RelicDef {
        id: "POTION_BELT", name: "药水腰带",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "+2 药水栏位。槽位数现在是**观测量** `State::potion_slots`（游戏在 \n               player.max_potion_slots 里直接报），所以这件遗物不需要战斗层建模 —— \n               改槽位的还有炼金宝匣 +4、药瓶皮套 +1，高进阶初始 3→2，反推一定会错",
    },
    // ---- 战斗胜利结算（2026-08-20 建模）。规则在 `POWERS`，层数 = 回血量 ----
    //
    // 「验证面为空」这条**已经不成立了**：战斗结束帧一直都录在 trace 里
    // （`state_type: "rewards"`），只是 `verify` 整帧跳过。改成只比血量之后，
    // 13 条实录里凡是录了 relics 的都能判它们。
    RelicDef {
        id: "BURNING_BLOOD", name: "燃烧之血",
        start_status: &[], private_status: &[(St::BurningBlood, 6)],
        counter_to: None,
        modelled: true,
        note: "战斗胜利回6。铁甲战士自带，每局开局就有。[源码] AfterCombatVictory，玩家死了不触发",
    },
    RelicDef {
        id: "MEAT_ON_THE_BONE", name: "带骨肉",
        start_status: &[], private_status: &[(St::MeatOnTheBone, 12)],
        counter_to: None,
        modelled: true,
        note: "战斗胜利时血量≤max/2（向下取整）则回12。**在燃烧之血之前结算**，所以阈值不含那6点",
    },

    // ---- 第 4 期：战斗内触发式。规则在 `POWERS`，层数 = 每次触发的效果值 ----
    RelicDef {
        id: "ORNAMENTAL_FAN", name: "精致折扇",
        start_status: &[], private_status: &[(St::OrnamentalFan, 4)], counter_to: None, modelled: true,
        note: "同回合每第3张攻击牌给4格挡。[源码] 是 %3==0，第6/9张也给；格挡 Unpowered，不吃脆弱/臂甲",
    },
    RelicDef {
        id: "MERCURY_HOURGLASS", name: "水银沙漏",
        start_status: &[], private_status: &[(St::MercuryHourglass, 3)], counter_to: None, modelled: true,
        note: "回合开始对全体3点。[源码] Unpowered，不吃力量",
    },
    RelicDef {
        id: "LANTERN", name: "灯笼",
        start_status: &[], private_status: &[(St::Lantern, 1)], counter_to: None, modelled: true,
        note: "第一回合+1能量。[源码] 是 TurnNumber <= 1，不是 == 1",
    },
    // 赤牛：每场战斗开始（[源码] 实为**第一回合开始**，`TurnNumber <= 1`）获得 8 活力。
    // 私有量：游戏把活力报成 `VIGOR_POWER`（那个进 ALL_ST），但"赤牛给几点"
    // 这个配置值游戏不报，和灯笼的 1 点能量同一类。
    RelicDef {
        id: "AKABEKO", name: "赤牛",
        start_status: &[], private_status: &[(St::Akabeko, 8)], counter_to: None, modelled: true,
        note: "第一回合开始 +8 活力。活力=下一张攻击牌的加法加值，**多段牌每段都吃满**（[源码] 钩子在多段循环外）",
    },
    // 石化蟾蜍：[源码] `PetrifiedToad.BeforeCombatStartLate` -> `PotionCmd.TryToProcure<PotionShapedRock>`。
    //
    // **不欠战斗层任何东西**，理由和药水腰带一模一样：它的产物是一瓶药水，
    // 而药水槽是**观测量**（`player.potions[]`）。内核照着观测灌就行，
    // 自己再塞一瓶只会重复计数。真正要建的是那瓶药水本身 ——
    // 见 `ops.rs::POTIONS` 的 `药水形状的石头`。
    RelicDef {
        id: "PETRIFIED_TOAD", name: "石化蟾蜍",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "开局塞一瓶药水形状的石头。药水槽是观测量，所以战斗层无关；那瓶药水在 POTIONS 里",
    },
    RelicDef {
        id: "PENDULUM", name: "摆动球",
        start_status: &[], private_status: &[(St::Pendulum, 1)],
        counter_to: Some(St::PendulumPhase),
        modelled: true,
        note: "每3回合抽1张。**相位跨战斗保留**（[源码] TurnsSeen 带 SavedProperty），所以从观测的计数器灌相位，别假设从0开始",
    },
    RelicDef {
        id: "STONE_CALENDAR", name: "历石",
        start_status: &[], private_status: &[(St::StoneCalendar, 52)], counter_to: None, modelled: true,
        note: "第7回合结束对全体52点",
    },
    RelicDef {
        id: "ORICHALCUM", name: "奥利哈钢",
        start_status: &[], private_status: &[(St::Orichalcum, 6)], counter_to: None, modelled: true,
        note: "回合结束无格挡则+6。**两段式**：TurnEndVeryEarly 快照、TurnEnd 结算，因为覆甲在第二段给格挡",
    },

    // ---- 战斗开始挂 status。**注意：这几件在对拍路径上收益为零** ----
    //
    // 它们给的都是游戏**也会报**的量，`sync` 从观测抄过来就已经算对了。
    // 建它们是为了内核**自己开一场仗**的时候（L3 推演、合成 trace）——
    // 那时没有观测可抄。所以这一栏是 `start_status` 不是 `private_status`，
    // 而且 `relic_carry` **不会**碰它们（碰了就会把力量钉死，见 RelicDef 的注释）。
    RelicDef {
        id: "VAJRA", name: "金刚杵",
        start_status: &[(St::Strength, 1)], private_status: &[], counter_to: None, modelled: true,
        note: "每场战斗开始获得1点力量",
    },
    RelicDef {
        id: "GORGET", name: "护喉甲",
        start_status: &[(St::PlatedArmor, 4)], private_status: &[], counter_to: None, modelled: true,
        note: "每场战斗开始获得4层覆甲",
    },
    RelicDef {
        id: "ODDLY_SMOOTH_STONE", name: "意外光滑的石头",
        start_status: &[(St::Dexterity, 1)], private_status: &[], counter_to: None, modelled: true,
        note: "每场战斗开始获得1点敏捷",
    },
    RelicDef {
        id: "BRONZE_SCALES", name: "铜质鳞片",
        start_status: &[(St::Thorns, 3)], private_status: &[], counter_to: None, modelled: true,
        note: "每场战斗开始获得3点荆棘。规则 2026-08-29 才建（在那之前 St::Thorns 没有任何消费点，两侧都不生效）。**这条注释原来写的是「只在敌人身上验过」，是错的** —— 敌人侧同样没建",
    },
    // ---- 2026-08-22 这一局遇到的六件。**没有一件欠战斗层建模**，
    //      但"为什么不用建"分两类，混起来会让下一个人白查一遍源码 ----
    //
    // 甲、**局外遗物**（和 白银熔炉 / 佩尔之翼 同一档）
    RelicDef {
        id: "LOST_COFFER", name: "失物盒",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：[源码] AfterObtained 给 1 次卡牌奖励 + 1 瓶药水。战斗层无关",
    },
    RelicDef {
        id: "PLANISPHERE", name: "活动星图",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：[源码] AfterRoomEntered 且当前是 ? 房间时回 5 血。战斗层无关",
    },
    RelicDef {
        id: "YUMMY_COOKIE", name: "美味饼干",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：拾起时升级 4 张牌。战斗层无关",
    },
    // 乙、**被观测遮住**的三件：效果都发生在回合 1 开始，而内核的手牌和血量
    //     每帧从观测同步 —— 建了要么空转要么重复计数。
    //     这是古茶具那 2 点能量、薪火之源那 1 点能量踩过的同一个坑：
    //     **`sync` 直接照抄观测的地方，不能再在内核里加一遍。**
    RelicDef {
        id: "BELLOWS", name: "风箱",
        start_status: &[], private_status: &[(St::UpgradeOpeningHand, 1)],
        counter_to: None, modelled: true,
        note: "[源码] AfterPlayerTurnStart 且 TurnNumber<=1 升级手牌。规则在 POWERS 的 HandDrawn；
               对拍路径上手牌是观测量（sync 照抄升级态），那边不挂这个 status",
    },
    RelicDef {
        id: "BONE_TEA", name: "骨茶",
        start_status: &[], private_status: &[],
        counter_to: None, modelled: true,
        note: "[源码] 接下来 N 场战斗开局升级初始手牌（和风箱同一个 status）。
               '还剩几场'是局外状态而且 ShowCounter=false，走 CONDITIONAL_START 由调用方给",
    },
    RelicDef {
        id: "BLOOD_VIAL", name: "小血瓶",
        start_status: &[], private_status: &[(St::BloodVial, 2)],
        counter_to: None, modelled: true,
        note: "[源码] AfterPlayerTurnStartLate 且 TurnNumber<=1 回 2 血。规则在 POWERS；
               对拍路径上血量是观测量（开局 66->68 已经含它），那边不挂",
    },
    RelicDef {
        id: "PEAR", name: "梨子",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：[源码] 拾起时最大生命值 +10。战斗层无关",
    },
    RelicDef {
        id: "JEWELED_MASK", name: "宝石面具",
        start_status: &[], private_status: &[(St::JeweledMask, 1)],
        counter_to: None, modelled: true,
        note: "[源码] BeforeHandDraw 且 TurnNumber<=1：抽牌堆里随机一张能力牌进手牌 + SetToFreeThisTurn。
               规则在 POWERS 的 TurnStart（抽牌之前）；对拍路径上手牌和费用是观测量，那边不挂",
    },
    RelicDef {
        id: "NEOWS_TORMENT", name: "涅奥的苦痛",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：[源码] 拾起时将 1 张涅奥之怒加入牌组。战斗层无关",
    },
    RelicDef {
        id: "AMETHYST_AUBERGINE", name: "紫水晶茄子",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：[源码] 敌人额外掉落 15 金币。战斗层无关",
    },
];

/// 这个**遗物私有量**是不是「一场只用得掉一次」的。
///
/// `Replayer::sync` 从战斗**中途**接进来时要用它：那时前面发生过什么不可知，
/// 一场只用一次的量要当成**已经用掉**（低估自己），
/// 而"我身上有这件遗物"这种常数标记任何时候恢复都对。
///
/// **判据是数据不是名单**：它自己那条规则里有没有 `TOp::ClearSelf`
/// —— 那正是 [源码] 里的 `UsedThisCombat`。外加两个**在代码里**被花掉的充能
/// （臂甲 / 坚定不移，消费点在 `damage::card_block`，没有 `PowerDef` 认领它们，
/// 所以扫表扫不到）。
pub fn spent_once_per_combat(st: St) -> bool {
    if matches!(st, St::VambraceCharge | St::UnmovableCharge | St::RuinedHelmet) {
        return true;
    }
    fn clears_self(ops: &[TOp]) -> bool {
        ops.iter().any(|op| match op {
            TOp::ClearSelf => true,
            TOp::If { then, .. } => clears_self(then),
            _ => false,
        })
    }
    POWERS.iter().any(|p| p.st == st && clears_self(p.ops))
}

/// 这个 status 是不是某条 `TCond::EveryNTurns` 的**相位输入**。
///
/// 遗物面板上那个计数器（`RelicDef::counter_to`）有两种，灌进内核的方式不同：
///
/// * **回合相位**（摆动球的 `TurnsSeen`）：[源码] `AfterPlayerTurnStart` 里
///   `TurnsSeen = (TurnsSeen + 1) % n`，所以观测到的是**这一场已经加过
///   `round` 次之后**的值；而 `TCond::EveryNTurns` 算的是 `phase + turn`，
///   要的是**战斗开始那一刻**的相位 ⇒ 灌进去要把 `round` 减回去。
/// * **别的计数器**（钢笔尖数打出过几张攻击牌）：和回合数没关系，原样灌。
///
/// 判据是数据（谁在 `EveryNTurns` 里当 `phase`），不是名单。
pub fn is_turn_phase(st: St) -> bool {
    fn refs(ops: &[TOp], st: St) -> bool {
        ops.iter().any(|op| match op {
            TOp::If { cond: TCond::EveryNTurns { phase, .. }, then } => *phase == st || refs(then, st),
            TOp::If { then, .. } => refs(then, st),
            _ => false,
        })
    }
    POWERS.iter().any(|p| refs(p.ops, st))
}

/// 按游戏内部 id 查遗物。查不到 = 内容表里没有这一件。
pub fn relic_by_id(id: &str) -> Option<&'static RelicDef> {
    RELICS.iter().find(|r| r.id == id)
}

/// **只在合成路径上欠账的遗物** —— `RelicDef::modelled` 那一列照不到的那批。
///
/// # 两条路，两个覆盖率
///
/// `modelled` 问的是「**对拍**路径上够不够」。那条路上手牌、血量、药水槽
/// 全是**观测量**：`sync` 每帧照抄，内核再建一遍就是重复计数
/// （风箱和小血瓶的 `note` 里写着这件事，都是踩过的）。
///
/// **L3 走的是另一条路。** `synth::build` 凭牌组和遗物**搭**一场仗 ——
/// 没有观测可抄，于是这批"靠观测兜底"的遗物在那条路上**一件都不生效**。
/// 两个覆盖率因此是两个数，这张表就是差集。
///
/// # 每条写的是「开局那一刻它少做了什么」
///
/// 判据统一：**效果发生在开战那一刻、或者依赖 `FightSpec` 装不下的局外状态**。
/// 纯局外的那些（磨刀石升级两张牌、梨子 +10 最大生命）**不在这里** ——
/// 它们的产物已经在 L3 的输入里（牌组、最大生命），构造器不欠它们什么。
///
/// 消费者是 `synth::build`（报成 `synth::Gap::SynthUnmodelledRelic`）。
/// `synth_only_gaps_name_real_relics` 守着这里的 id 都真在 `RELICS` 里 ——
/// 打错一个字的后果是这条**永远不会被报出来**，而那正是这张表要防的东西。
pub static SYNTH_ONLY_GAPS: &[(&str, &str)] = &[
    // **开局塞药水那两件**：药水不是 status，`begin_combat` 里没有它的位置，
    // 而 `FightSpec::potions` 是调用方给的一份**战前**清单 —— 那瓶石头是
    // 开战那一刻才拿到的。要建得先决定「L3 的药水账本长什么样」，
    // 那是阶段 2/3 的事。
    ("PETRIFIED_TOAD", "开局塞一瓶药水形状的石头"),
    ("DELICATE_FROND", "开局把空药水栏填满"),
    // 壶铃：给多少力量取决于**在休息处换过几次**（`TimesLifted`，`[SavedProperty]`）。
    // 内核不该猜，调用方走 `FightSpec::start_status` 传进来。
    ("GIRYA", "开局给力量（换过几次是局外状态，走 FightSpec::start_status）"),
    // 皮草大衣：拾起时**在地图上标记 7 场仗**，那几场的敌人只有 1 血。
    // 「哪几场」是路线信息，L3 的整幕链要自己记（阶段 3）。
    ("FUR_COAT", "标记过的 7 场仗里敌人只有 1 血（哪几场是局外状态）"),
];

/// 这件遗物在**合成路径**上欠什么。`None` = 不欠（或者本来就 `modelled: false`，
/// 那条走 `synth::Gap::UnmodelledRelic`）。
pub fn synth_gap(id: &str) -> Option<&'static str> {
    SYNTH_ONLY_GAPS.iter().find(|(k, _)| *k == id).map(|(_, why)| *why)
}

/// **条件武装**：这几件遗物挂不挂 status 取决于 `FightSpec` 里的局外信息。
///
/// # 为什么不能进 `RelicDef::private_status`
///
/// 那一栏是**无条件**挂的（`grant_relics` 和 `replay::sync` 都照挂），而这几件
/// 的条件在**战斗观测里根本没有**：上一个房间是不是休息处、骨茶还剩几场、
/// 这一场是不是 Boss。挂成无条件就是"每一场都当作刚休息过"，凭空多 2 点能量。
///
/// 两条路的处置不同，各自都对：
/// * **合成路径**：调用方知道自己在模拟哪个房间（`synth::FightSpec`），照这张表武装
/// * **对拍路径**：那几个量（能量、手牌升级态、血量）**本来就在观测里**，
///   `sync` 直接照抄 ⇒ **一条都不挂**
///
/// `conditional_start_names_resolve` 守着这里的 id 都真在 `RELICS` 里。
pub static CONDITIONAL_START: &[(&str, Arm, St, i32)] = &[
    ("VENERABLE_TEA_SET", Arm::AfterRest, St::TeaSet, 2),
    ("FAKE_VENERABLE_TEA_SET", Arm::AfterRest, St::TeaSet, 1),
    // 骨茶：[源码] `CombatsLeft` 带 `[SavedProperty]`，而且 `ShowCounter => false`
    // —— **游戏连面板都不显示**，所以观测里一定拿不到，只能由调用方给。
    ("BONE_TEA", Arm::RelicCounter, St::UpgradeOpeningHand, 1),
    ("PANTOGRAPH", Arm::BossRoom, St::Pantograph, 25),
];

/// 一条「条件武装」的条件。**每一条都有一件真遗物在用**，没有预留的。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Arm {
    /// 上一个房间是休息处（两个古茶具）
    AfterRest,
    /// 这件遗物自己的**局外计数器** > 0（骨茶的「还剩几场」）
    RelicCounter,
    /// 这一场是 Boss 战（缩放仪）
    BossRoom,
}

/// 这件遗物在什么条件下武装什么。查不到就是不武装。
pub fn conditional_start(id: &str) -> Option<(Arm, St, i32)> {
    CONDITIONAL_START.iter().find(|(k, _, _, _)| *k == id).map(|(_, a, st, v)| (*a, *st, *v))
}

/// **全部 22 种附魔**（[源码] `MegaCrit.Sts2.Core.Models.Enchantments` 那个
/// 命名空间里的每一个非抽象类，2026-09-06 逐个读过）。
///
/// id 是类名转大写下划线（`RoyallyApproved` -> `ROYALLY_APPROVED`），
/// 和 mod 报的 `enchantment.id` 逐字对齐 —— 观测里见过的两个
/// （`NIMBLE` / `ROYALLY_APPROVED`）都对上了，剩下 20 个是**推的**，
/// 对不上会被 `Report::unknown_enchantments` 点名，不会静默算错。
///
/// **`modelled` 那一列才是进度**。今天 8/22 建全了；其余各自缺一个内核还没有的
/// 机制，`note` 里点名。这和 `RELICS` 是同一个套路 —— 进表不等于建模。
///
/// 顺序**不重要**（查表走 `enchant_by_id`），但 `CardInst::ench` 存的是
/// **下标 + 1**，所以**只许往后加，不许插队**。
pub static ENCHANTS: &[EnchantDef] = &[
    // ---- 建全了的 ----
    // [源码] `Nimble.EnchantBlockAdditive => Amount`，`CanEnchant` 要 `card.GainsBlock`。
    // [实测] 2026-09-01 起语料里 76 次：带灵巧2 的耸肩无视给 10 而卡表 8。
    EnchantDef {
        id: "NIMBLE", name: "灵巧",
        block_add: EnchVal::Amount, damage_add: EnchVal::Zero, damage_mul: (1, 1),
        keywords: 0, modelled: true, note: "",
    },
    // [源码] `RoyallyApproved.OnEnchant` 加 `Innate` + `Retain`，没有任何数值钩子。
    // [实测] 2026-09-06 `act3_f46_elite_soul_nexus` 两个回合边界：均衡+ 留在手上、
    // 同一手的邻座（添柴+）被弃掉。王室印章给的就是这个。
    EnchantDef {
        id: "ROYALLY_APPROVED", name: "王室认证",
        block_add: EnchVal::Zero, damage_add: EnchVal::Zero, damage_mul: (1, 1),
        keywords: F_INNATE | F_RETAIN, modelled: true, note: "",
    },
    // [源码] `Steady.OnEnchant` 加 `Retain`。
    EnchantDef {
        id: "STEADY", name: "沉稳",
        block_add: EnchVal::Zero, damage_add: EnchVal::Zero, damage_mul: (1, 1),
        keywords: F_RETAIN, modelled: true, note: "",
    },
    // [源码] `Sharp.EnchantDamageAdditive => Amount`（带 `IsPoweredAttack` 门，
    // 而内核的伤害加值本来就只走卡牌那条路，药水传 0）。扭曲锤子给的就是这个。
    EnchantDef {
        id: "SHARP", name: "锋利",
        block_add: EnchVal::Zero, damage_add: EnchVal::Amount, damage_mul: (1, 1),
        keywords: 0, modelled: true, note: "",
    },
    // [源码] `Instinct.EnchantDamageMultiplicative => 2m`。
    EnchantDef {
        id: "INSTINCT", name: "直觉",
        block_add: EnchVal::Zero, damage_add: EnchVal::Zero, damage_mul: (2, 1),
        keywords: 0, modelled: true, note: "",
    },
    // ---- 差一个口子的（`modelled: false`，缺什么写在 note 里）----
    // [源码] 伤害 ×1.5 建了，缺的是 `OnPlay` 那 2 点**不可格挡**自伤
    //（`ValueProp.Unblockable | Unpowered`）—— 内核今天没有"绕过格挡"这一档。
    // 乘区本身和老的 `F_CORRUPT` 逐字同源，见 `damage::card_face_damage`。
    EnchantDef {
        id: "CORRUPTED", name: "腐化",
        block_add: EnchVal::Zero, damage_add: EnchVal::Zero, damage_mul: (3, 2),
        keywords: 0, modelled: false,
        note: "伤害 ×1.5 建了；缺 OnPlay 的 2 点不可格挡自伤（内核没有 Unblockable）",
    },
    // [源码] `Goopy`：`OnEnchant` 加消耗、`EnchantBlockAdditive => Amount-1`、
    // 每打出一次 `Amount++`（**跨战斗累加**，写回 `DeckVersion`）。
    EnchantDef {
        id: "GOOPY", name: "黏糊糊",
        block_add: EnchVal::AmountMinus1, damage_add: EnchVal::Zero, damage_mul: (1, 1),
        keywords: 0, modelled: false,
        note: "格挡加值建了；缺「这一张实例加消耗关键字」和「每打出一次 Amount++」",
    },
    // [源码] `Vigorous.EnchantDamageAdditive => Amount`，但 `AfterCardPlayed`
    // 把自己 `Disabled` —— **一场只吃一次**。内核没有"附魔本场用过了"这个位。
    EnchantDef {
        id: "VIGOROUS", name: "旺盛",
        block_add: EnchVal::Zero, damage_add: EnchVal::Zero, damage_mul: (1, 1),
        keywords: 0, modelled: false,
        note: "欠「本场只生效一次」的实例位；按 0 算 ⇒ 低估第一次那一下的伤害",
    },
    // [源码] `Momentum.OnPlay` 里 `ExtraDamage += Amount`，加值**每打出一次涨一次**。
    // 和暴走同构（`CardInst::bonus`），但那是牌自己的 op，附魔这一侧还没有对应的钩子。
    EnchantDef {
        id: "MOMENTUM", name: "势能",
        block_add: EnchVal::Zero, damage_add: EnchVal::Zero, damage_mul: (1, 1),
        keywords: 0, modelled: false,
        note: "欠「打出时把加值写回这一张实例」（和暴走同构，缺的是附魔侧的 OnPlay）",
    },
    // [源码] `Inky`：`EnchantDamageAdditive => 1`，外加 `OnPlay` 给目标 1 层虚弱。
    EnchantDef {
        id: "INKY", name: "墨迹",
        block_add: EnchVal::Zero, damage_add: EnchVal::Fixed(1), damage_mul: (1, 1),
        keywords: 0, modelled: false,
        note: "+1 伤害建了；缺 OnPlay 给目标上 1 层虚弱（欠附魔侧的 OnPlay）",
    },
    // [源码] `TezcatarasEmber.OnEnchant` 把费用降到 0 并加 `Eternal`，
    // `EnchantDamageAdditive => 3`。
    EnchantDef {
        id: "TEZCATARAS_EMBER", name: "特兹卡塔拉的余烬",
        block_add: EnchVal::Zero, damage_add: EnchVal::Fixed(3), damage_mul: (1, 1),
        keywords: 0, modelled: false,
        note: "+3 伤害建了；缺「把这一张实例的费用改成 0」和永恒关键字",
    },
    // ---- 一个口子都没建的（各自欠一个内核根本没有的机制）----
    EnchantDef {
        id: "ADROIT", name: "娴熟",
        block_add: EnchVal::Zero, damage_add: EnchVal::Zero, damage_mul: (1, 1),
        keywords: 0, modelled: false,
        note: "打出时额外获得 Amount 点格挡。欠附魔侧的 OnPlay",
    },
    EnchantDef {
        id: "SOWN", name: "播种",
        block_add: EnchVal::Zero, damage_add: EnchVal::Zero, damage_mul: (1, 1),
        keywords: 0, modelled: false,
        note: "本场第一次打出时 +Amount 能量。欠附魔侧的 OnPlay + 「本场用过了」的位",
    },
    EnchantDef {
        id: "SWIFT", name: "迅捷",
        block_add: EnchVal::Zero, damage_add: EnchVal::Zero, damage_mul: (1, 1),
        keywords: 0, modelled: false,
        note: "本场第一次打出时抽 Amount 张。欠附魔侧的 OnPlay + 「本场用过了」的位",
    },
    EnchantDef {
        id: "GLAM", name: "魅影",
        block_add: EnchVal::Zero, damage_add: EnchVal::Zero, damage_mul: (1, 1),
        keywords: 0, modelled: false,
        note: "本场第一次打出时多打出一次。欠 EnchantPlayCount（和未掘宝石的重放同一个洞）",
    },
    EnchantDef {
        id: "SPIRAL", name: "螺旋",
        block_add: EnchVal::Zero, damage_add: EnchVal::Zero, damage_mul: (1, 1),
        keywords: 0, modelled: false,
        note: "每次打出都多打出一次。欠 EnchantPlayCount",
    },
    EnchantDef {
        id: "SOULS_POWER", name: "灵魂之力",
        block_add: EnchVal::Zero, damage_add: EnchVal::Zero, damage_mul: (1, 1),
        keywords: 0, modelled: false,
        note: "去掉这张牌的消耗关键字。欠「按实例改消耗位」（`CardDef.exhausts` 是按牌名的）",
    },
    EnchantDef {
        id: "SLITHER", name: "滑行",
        block_add: EnchVal::Zero, damage_add: EnchVal::Zero, damage_mul: (1, 1),
        keywords: 0, modelled: false,
        note: "每次被抽到时费用随机成 0..3。欠「抽牌时改这一张实例的费用」的钩子",
    },
    EnchantDef {
        id: "SLUMBERING_ESSENCE", name: "沉睡精华",
        block_add: EnchVal::Zero, damage_add: EnchVal::Zero, damage_mul: (1, 1),
        keywords: 0, modelled: false,
        note: "没打出就一直 −1 费。欠 `BeforeFlush` 钩子 + 「打出前一直累积」的费用修饰",
    },
    EnchantDef {
        id: "IMBUED", name: "灌注",
        block_add: EnchVal::Zero, damage_add: EnchVal::Zero, damage_mul: (1, 1),
        keywords: 0, modelled: false,
        note: "开局沉到牌堆底 + 自动预打出阶段。欠 `ShouldStartAtBottomOfDrawPile`",
    },
    EnchantDef {
        id: "PERFECT_FIT", name: "完美契合",
        block_add: EnchVal::Zero, damage_add: EnchVal::Zero, damage_mul: (1, 1),
        keywords: 0, modelled: false,
        note: "改洗牌顺序（`ModifyShuffleOrder`）。欠洗牌钩子",
    },
    EnchantDef {
        id: "CLONE", name: "复制",
        block_add: EnchVal::Zero, damage_add: EnchVal::Zero, damage_mul: (1, 1),
        keywords: 0, modelled: false,
        note: "[源码] 类体是空的（钩子全在别处/未启用）。**先查清楚再动**",
    },
];

/// 按游戏内部 id 查附魔。查不到 = 表里没有这一种（**不是"没效果"**）。
pub fn enchant_by_id(id: &str) -> Option<(u8, &'static EnchantDef)> {
    ENCHANTS.iter().position(|e| e.id == id).map(|i| (i as u8 + 1, &ENCHANTS[i]))
}

/// 这一张卡实例带的附魔。`CardInst::ench` 是**下标 + 1**，0 = 没有。
#[inline]
pub fn enchant_of(inst: &crate::state::CardInst) -> Option<&'static EnchantDef> {
    if inst.ench == 0 {
        None
    } else {
        ENCHANTS.get(inst.ench as usize - 1)
    }
}

/// 这一张实例的**格挡加值**（`EnchantBlockAdditive`）。
#[inline]
pub fn ench_block_add(inst: &crate::state::CardInst) -> i32 {
    enchant_of(inst).map_or(0, |e| e.block_add.eval(inst.ench_amt as i32))
}

/// 这一张实例的**伤害加值**（`EnchantDamageAdditive`）。和锋利同一档，
/// 加在基础值上、在力量之前。
#[inline]
pub fn ench_damage_add(inst: &crate::state::CardInst) -> i32 {
    enchant_of(inst).map_or(0, |e| e.damage_add.eval(inst.ench_amt as i32))
}

/// 这一张实例的**伤害乘区**（`EnchantDamageMultiplicative`），`(分子, 分母)`。
#[inline]
pub fn ench_damage_mul(inst: &crate::state::CardInst) -> (i32, i32) {
    enchant_of(inst).map_or((1, 1), |e| e.damage_mul)
}

#[inline]
pub fn card(id: u16) -> &'static CardDef {
    &CARDS[id as usize]
}

#[inline]
/// 这张牌能不能被升级（[源码] `CardModel.IsUpgradable`）。
///
/// 源码是 `CurrentUpgradeLevel < MaxUpgradeLevel`，默认上限 1，
/// 而**上限为 0 的那 38 张逐个查过：无一例外全是 Status / Curse / Quest**
/// （`grep "MaxUpgradeLevel => 0"`）。
///
/// 2026-08-22 第14层实录钉死：武装+ 把手里两张防御都升了，
/// **没有**升藏宝图（Quest）。在此之前内核对手牌一视同仁地全升。
///
/// 藏宝图当时落在 `card::UNKNOWN`（`kind = Status`），所以这条判据碰巧是对的；
/// 现在它有了自己的表项和 `Kind::Quest`，判据里就要显式带上那一类。
pub fn upgradable(id: u16) -> bool {
    !matches!(card(id).kind, Kind::Status | Kind::Curse | Kind::Quest)
}

pub fn card_ops(id: u16, upgraded: bool) -> &'static [Op] {
    let d = card(id);
    if upgraded && !d.ops_upg.is_empty() {
        d.ops_upg
    } else {
        d.ops
    }
}

/// 状态牌/诅咒牌/任务牌**不是一律不能打**。
///
/// 伤口的卡面是"不能被打出。"，而黏液是"抽1张牌。 消耗。"、1 费、真的能打。
///
/// **判据是 `UNPLAYABLE_CARDS`（[源码] `CardKeyword.Unplayable`），
/// 不再是"有没有 ops"** —— 后者被孢子心灵推翻了，理由写在那张表的头上。
#[inline]
pub fn playable(id: u16) -> bool {
    let d = card(id);
    if matches!(d.kind, Kind::Status | Kind::Curse | Kind::Quest) {
        return !unplayable(id);
    }
    true
}

// ---------------- enemies ----------------

pub mod enemy {
    pub const DUMMY: u16 = 0;
    pub const EXOSKELETON: u16 = 1;
    pub const TEST_SUBJECT: u16 = 2;
    /// 占位：什么都不做的敌人。对拍时**一律**用它，因为敌人 AI 本来就在
    /// 「故意没做」清单里，让内核的循环出招参与验证只会制造假阳性。
    /// 敌人的真实行动由观测反推，不由内核预测。
    pub const UNKNOWN: u16 = 3;
    /// 小啃兽。**唯一一个行动是实测的敌人**，用来守住"敌人格挡跨回合存活"
    /// 这条不变量 —— 需要一个真的会加格挡的敌人才测得出来。
    pub const NIBBIT: u16 = 4;
    /// 旧日雕像（第1幕精英）。**唯一一个自带缓慢的已知敌人**，
    /// `St::SlowSource` 这条规则就是靠它取的数。
    pub const BYGONE_EFFIGY: u16 = 5;
    // 以下由 `tools/enemy_report.py` 从实录 trace 聚合而来，
    // 汇总见 `traces/enemies_observed.json`。读表前先看下面那段说明。
    pub const SHRINK_BEETLE: u16 = 6;
    pub const PILLAR_CONSTRUCT: u16 = 7;
    pub const FUZZY_CRAWLER: u16 = 8;
    pub const LEAF_SLIME_S: u16 = 9;
    pub const LEAF_SLIME_M: u16 = 10;
    pub const TWIG_SLIME_S: u16 = 11;
    pub const TWIG_SLIME_M: u16 = 12;
    pub const RAIDER_CROSSBOW: u16 = 13;
    pub const RAIDER_AXE: u16 = 14;
    pub const RAIDER_TRACKER: u16 = 15;
    /// 飞蝇菌子。**证明"意图标签含防御方乘区"的那只敌人**，见 EnemyDef 注释。
    pub const FLYCONID: u16 = 16;
    /// 雾菇。会召唤，而且召唤物会复活 —— 两条内核都没有的机制，见 EnemyDef 注释。
    pub const FOGMOG: u16 = 17;
    /// 利齿之眼。雾菇的爪牙。
    pub const EYE_WITH_TEETH: u16 = 18;
    /// 第1幕 Boss。它的两个信徒开局就带爪牙标记，神官一死全场结束。
    pub const KIN_PRIEST: u16 = 19;
    pub const KIN_FOLLOWER: u16 = 20;
    // 第2幕开始的敌人
    pub const BOWLBUG_ROCK: u16 = 21;
    pub const BOWLBUG_EGG: u16 = 22;
    pub const THIEVING_HOPPER: u16 = 23;
    pub const BOWLBUG_NECTAR: u16 = 24;
    pub const BOWLBUG_SILK: u16 = 25;
    pub const CHOMPER: u16 = 26;
    pub const LOUSE_PROGENITOR: u16 = 27;
    /// 异螨（第 2 幕杂兵，`MytesNormal` 两只同场）。2026-09-14 照 [源码] 原地重建 ——
    /// 这个下标原来是一条错的 [wiki]「螨虫」。起手按站位，浓毒往手牌塞毒素。
    pub const MYTE: u16 = 28;
    pub const OVICOPTER: u16 = 29;
    pub const SPINY_TOAD: u16 = 30;
    pub const QUEEN: u16 = 31;
    pub const DOORMAKER: u16 = 32;
    /// 知识恶魔（**第 2 幕 Boss**）。2026-09-14 按 [源码] 重建（原来的 [wiki] 定义是错的），
    /// 三次二选一的诅咒走 `State::curse_policy`，见 `ENEMIES` 那一条。
    pub const KNOWLEDGE_DEMON: u16 = 33;
    /// 第2幕 Boss 双怪。**它们所在的战斗有三条内核没有的机制**，见 `ENEMIES` 里
    /// 那两条的注释 —— 拿它们做跨回合推演之前先读那段。
    pub const CRUSHER: u16 = 34;
    pub const ROCKET: u16 = 35;
    // ---- 2026-08-22 实战新遇到的，按追加顺序编号 ----
    pub const MAWLER: u16 = 36;
    pub const SNAPPING_JAXFRUIT: u16 = 37;
    pub const SLITHERING_STRANGLER: u16 = 38;
    /// 异蛙寄生虫死后冒出来的那 4 只。**它不是爪牙** ——
    /// 给它挂 `St::Minion` 会让 `no_master_left` 在宿主一死就判战斗结束，
    /// 正好把这场精英的第二阶段抹掉。
    pub const WRIGGLER: u16 = 39;
    pub const PHROG_PARASITE: u16 = 40;
    /// 第 1 幕 Boss。**名字用游戏显示的「仪式兽」**，见 `WRIGGLER` 那条。
    pub const CEREMONIAL_BEAST: u16 = 41;
    /// 外骨骼虫（第2幕）。**难以杀灭 9 的第一个真实来源** ——
    /// 在它之前那条乘区一次都没被数据碰过。
    pub const EXOSKELETON_ROACH: u16 = 42;
    /// 蜂群术士（第2幕精英）。带**人体蜂房**：我每一段攻击命中它，抽牌堆多层数那么多张晕眩
    /// （2026-09-14 建的，见 `St::PersonalHive`）。喷射信息素拆成两手，见 `M_ENTOMANCER`。
    pub const ENTOMANCER: u16 = 43;
    /// 感染棱柱（第2幕精英）。带**活力火花** —— 它让我每张技能牌都带污染，
    /// 而污染让我挨的每一次攻击 +1。两个都只映射不建模，见 `St::Tainted`。
    pub const INFESTED_PRISM: u16 = 44;
    pub const TOUGH_EGG: u16 = 45;
    pub const HATCHLING: u16 = 46;
    /// 无厌沙虫（**第 2 幕 Boss**）。带**沙坑**即死倒计时，
    /// 2026-09-05 建全（层数 + 每个敌人回合减 1 + 归零即死），见 `St::Sandpit`。
    pub const THE_INSATIABLE: u16 = 47;
    pub const DEVOTED_SCULPTOR: u16 = 48;
    /// 永世沙漏（**第 3 幕 Boss**）。**名字是「永世」不是「永恒」** ——
    /// 拼错过一次，后果是内核完全认不出它，见 `ENEMIES` 里那条注释。
    pub const AEONGLASS: u16 = 49;
    pub const LIVING_SHIELD: u16 = 50;
    pub const TURRET_OPERATOR: u16 = 51;
    pub const OWL_MAGISTRATE: u16 = 52;
    // 第 3 幕第 45 层「组装师」那一场（2026-08-22 实战录了 19 帧）。
    // 组装师造两种池子各 2 只，四只造物全部进表 —— 只建见过的两只的话，
    // 另外两只一出场内核就瞎，而它们是 50/50 的。
    pub const FABRICATOR: u16 = 53;
    pub const NOISEBOT: u16 = 54;
    pub const ZAPBOT: u16 = 55;
    pub const STABBOT: u16 = 56;
    pub const GUARDBOT: u16 = 57;
    /// 多尼斯异鸟（第 1 幕精英）。[实测] 2026-08-27 第9层 82 血。
    pub const BYRDONIS: u16 = 58;
    /// 猎人杀手（第 2 幕）。[实测] 2026-08-27 第22层 121 血。
    pub const HUNTER_KILLER: u16 = 59;
    /// 残杀千足虫的一节（第 2 幕精英，三节共用同一个 `EnemyDef`）。
    /// [实测] 2026-08-27 第30层 42 / 40 / 44 血。
    pub const DECIMILLIPEDE_SEGMENT: u16 = 60;
    /// 青蛙骑士（第 3 幕杂兵）。**第一只真正带覆甲的敌人** ——
    /// 敌人侧覆甲那四条规则就是它逼出来的。
    pub const FROG_KNIGHT: u16 = 61;
    /// 巨斧机器人（第 3 幕杂兵）。**三具身体共用这一个 def** ——
    /// 名字到 def 是 `position(|d| d.name == n)`，第一个匹配就返回，
    /// 所以同名的三个 def 会互相盖住。差异全部由「库存」的层数驱动，
    /// 见 `St::Stock`。
    pub const AXEBOT: u16 = 62;
    /// 灵魂枢纽（第 3 幕精英）。[实测] 2026-08-30 第46层 234 血。
    pub const SOUL_NEXUS: u16 = 63;
    /// 实验体（第 3 幕 Boss）。三阶段总计 636 血（111 + 212 + 313）。
    pub const TEST_SUBJECT_BOSS: u16 = 64;
    /// 骇鳗（第 1 幕精英，2026-08-31 首遇）
    pub const TERROR_EEL: u16 = 65;
    /// 活雾（第 1 幕杂兵）
    pub const LIVING_FOG: u16 = 66;
    /// 瓦斯炸弹（第 1 幕活雾召唤物）
    pub const GAS_BOMB: u16 = 67;
    /// 花园幽灵鳗（第 1 幕精英）
    pub const PHANTASMAL_GARDENER: u16 = 68;
    /// 瀑布巨兽（第 1 幕 Boss）
    pub const WATERFALL_GIANT: u16 = 69;
    /// 地道虫（第 2 幕杂兵）
    pub const TUNNELER: u16 = 70;
    /// 胧光怪（第 2 幕杂兵）。开局召唤一只**会无限复活的**寄生惧魔。
    pub const THE_OBSCURA: u16 = 71;
    /// 寄生惧魔（胧光怪的幻象，爪牙）。带 `St::Illusion`：**杀不掉**，
    /// 死了下一手回满血 —— 主人死了才跟着消失。
    pub const PARAFRIGHT: u16 = 72;
    // ---- 2026-09-09 第 1 幕新一局那一批。**只许往后加，不许插队** ——
    // `asc::ASC_HP` / `ASC_OPS` 是按这个下标索引的，中间插一条整张表就歪了
    // （`asc_table_indices_still_point_at_the_named_enemy` 守着，但那是事后报警，
    // 不是拦截）。
    pub const SLUDGE_SPINNER: u16 = 73;
    pub const CORPSE_SLUG: u16 = 74;
    pub const SEAPUNK: u16 = 75;
    pub const CALCIFIED_CULTIST: u16 = 76;
    pub const DAMP_CULTIST: u16 = 77;
    pub const SEWER_CLAM: u16 = 78;
    /// 双尾鼠。会**呼叫增援**召唤同类（一只一辈子一次）
    pub const TWO_TAILED_RAT: u16 = 79;
    pub const PUNCH_CONSTRUCT: u16 = 80;
    /// 第 1 幕 Boss 灵魂异鱼。隔几手给自己无实体 2，塞的状态牌是应急按钮
    pub const SOUL_FYSH: u16 = 81;
    // ---- 2026-09-13 第 3 幕补敌人（批 1 + 批 2）。**全部 [源码]，一条实录都没有**。
    // 中文名取自游戏本地化表（`SlayTheSpire2.pck` 里的 `*.name`），不是推的。
    pub const SLIMED_BERSERKER: u16 = 82;
    pub const MECHA_KNIGHT: u16 = 83;
    pub const GLOBE_HEAD: u16 = 84;
    /// 咬人卷轴。普通遭遇 4 卷、弱遭遇 3 卷**共用一个 def**，起手按槽位错开。
    pub const SCROLL_OF_BITING: u16 = 85;
    pub const THE_LOST: u16 = 86;
    pub const THE_FORGOTTEN: u16 = 87;
    // ---- 2026-09-14 第 3 幕骑士团（批 3，`KnightsElite` 三只同场）。**全部 [源码]**。
    pub const FLAIL_KNIGHT: u16 = 88;
    /// 幽灵骑士。**恶咒**：带着它时我回合末没打出去的牌全部消耗，它死了才解。
    pub const SPECTRAL_KNIGHT: u16 = 89;
    /// 魔法骑士。**抑制**：本场已升级的牌全部降级，它死了才升回来。
    pub const MAGI_KNIGHT: u16 = 90;
    // ---- 2026-09-14 第 2 幕（批 4）。**全部 [源码]**。
    /// 熟睡甲虫。开局覆甲 15 + 熟睡 3；**被打穿一下就少睡一回合**，醒了之后每手出击 16 + 力量 2。
    pub const SLUMBERING_BEETLE: u16 = 91;
    // ---- 2026-09-17 第 1 幕（批 2）。**全部 [源码]**。
    /// 乐加维林族母（**第 1 幕暗港 Boss**）。开局覆甲 12 + 沉睡 3；
    /// **打穿一下就醒**（覆甲当场没、晕一回合再斩击），醒后四手定环。
    pub const LAGAVULIN_MATRIARCH: u16 = 92;
    // ---- 2026-09-17 第 1 幕（批 3）。**全部 [源码]**。
    /// 墨影幻灵（**第 1 幕密林 Boss**）。开局**滑溜 8** —— 前 8 次打穿各只掉 1 血，
    /// 所以**段数是货币**，见 `St::Slippery`。四手定环。
    pub const VANTOM: u16 = 93;
    /// 墨宝（第 1 幕密林杂兵，`InkletsNormal` 三只同场）。滑溜 1，中间那只起手旋风。
    pub const INKLET: u16 = 94;
    // ---- 2026-09-19 第 1 幕（批 4）。**全部 [源码]**。
    /// 鬼祟珊瑚群（**第 1 幕暗港精英**）。开局**硬化外壳 20**：每个回合（双方各算各的）
    /// 最多掉 20 血，见 `St::HardenedShell`。四手定环。
    pub const SKULKING_COLONY: u16 = 95;
    // ---- 2026-09-19 第 1 幕（批 5，暗港杂兵）。**全部 [源码]**。
    pub const HAUNTED_SHIP: u16 = 96;
    /// 蟾蜍蝌蚪（`ToadpolesWeak` 两只同场）。**起手按站位**：前面那只先长刺，后面那只先旋转。
    pub const TOADPOLE: u16 = 97;
    /// 化石追踪者。开局**吮吸 3**：它的攻击每打穿一段 +3 力量。
    pub const FOSSIL_STALKER: u16 = 98;
    /// 地精佣兵。开局**意外**：它死时召唤卑鄙地精 + 胖地精（打死它不算赢）。
    pub const GREMLIN_MERC: u16 = 99;
    /// 卑鄙地精（地精佣兵死时召唤）：醒来 -> 一直冲撞。
    pub const SNEAKY_GREMLIN: u16 = 100;
    /// 胖地精（地精佣兵死时召唤）：醒来 -> 逃跑（内核近似成原地不动，见它的 def）。
    pub const FAT_GREMLIN: u16 = 101;
    // ---- 2026-09-19 第 1 幕（批 6，密林杂兵）。**全部 [源码]**。
    /// 藤蔓蹒跚者（`VineShamblerNormal` 单怪）。紧绕藤蔓给我挂**缠结**：下一个我方回合攻击牌 +1 费。
    pub const VINE_SHAMBLER: u16 = 102;
    /// 劫掠者刺客（`RubyRaidersNormal` 五选三之一）。一招：致命射击。
    pub const RAIDER_ASSASSIN: u16 = 103;
    /// 劫掠者暴徒（同上）。殴打 / 怒吼（+3 力量）交替。
    pub const RAIDER_BRUTE: u16 = 104;
}

// ===========================================================================
// 敌人表
// ===========================================================================
//
// 数值来源分三档，**每条都标出来**，因为可信度差很远：
//   [实测]  实录 trace 量出来的，最可信
//   [wiki]  sts2.wiki 扒的（`traces/enemies_wiki.json`），是假设不是事实
//   [合成]  我为测试造的假敌人，游戏里不存在
//
// **实测和 wiki 冲突时以实测为准。** 已知一处：旧日雕像的挥砍，
// wiki 写 15，而实录里标签 23 / 力量 10 ⇒ 基础 13，且实打就是 23。
//
// `intent` 是**游戏观测里的字符串**，不是 wiki 的 intent 列 ——
// 后者不可靠（把 12 点伤害的 Big Swing 标成 Utility）。
//
// 判决机制是 `verify --predict-enemy`，改完这张表就该重跑它。
// ---------------- 敌人出招机器 ----------------
//
// 只给**源码里不是纯确定性**的敌人挂。121 只反编译出招表里 82 只是纯确定性的，
// 那些继续用 `loop_from` 循环 —— 循环表对它们就是对的结构。
//
// 每一台都逐条对着 [源码] 的 `GenerateMoveStateMachine()` 抄，
// 下标映射写在各自的注释里。**改这里之前先去看那个函数。**

/// 树叶史莱姆（小）[源码] `LeafSlimeS`：两手互相不能连出，等权。
static M_LEAF_SLIME_S: Machine = Machine {
    start: Next::Rand(&[
        Branch { to: 0, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
        Branch { to: 1, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
    ]),
    after: &[
        Next::Rand(&[
            Branch { to: 0, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
            Branch { to: 1, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
        ]),
        Next::Rand(&[
            Branch { to: 0, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
            Branch { to: 1, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
        ]),
    ],
};

/// 树枝史莱姆（中）[源码] `TwigSlimeM`：**开局固定吐黏液**
/// （initialState 是 STICKY_SHOT，不是那个随机分支 —— 这一条循环表表达不了）。
/// 之后随机：团射最多连出 2 次，吐黏液不能连出。
/// 内核下标：0 = 吐黏液(STICKY_SHOT)，1 = 团射(POKEY_POUNCE)。
static M_TWIG_SLIME_M: Machine = Machine {
    start: Next::Go(0),
    after: &[
        Next::Rand(&[
            Branch { to: 1, weight: 1, repeat: Repeat::AtMost(2), cooldown: 0 },
            Branch { to: 0, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
        ]),
        Next::Rand(&[
            Branch { to: 1, weight: 1, repeat: Repeat::AtMost(2), cooldown: 0 },
            Branch { to: 0, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
        ]),
    ],
};

/// 雾菇 [源码] `Fogmog`：幻象 → 挥爪 → 分支{挥爪(0.4) | 头槌(0.6)}；
/// 分支里那个挥爪走完接头槌，头槌走完回挥爪。
///
/// **源码里有两个挥爪**（`SWIPE_MOVE` / `SWIPE_RANDOM_MOVE`，动作相同但
/// 后继不同），所以内核也得有两条 —— 3 号就是"分支版挥爪"。
/// 合并成一条会让状态图长歪：那两条的后继一个是分支、一个是头槌。
/// 权重 0.4 : 0.6 取整数比 2 : 3。
static M_FOGMOG: Machine = Machine {
    start: Next::Go(0),
    after: &[
        Next::Go(1),
        Next::Rand(&[
            Branch { to: 3, weight: 2, repeat: Repeat::NotTwice, cooldown: 0 },
            Branch { to: 2, weight: 3, repeat: Repeat::NotTwice, cooldown: 0 },
        ]),
        Next::Go(1),
        Next::Go(2),
    ],
};

/// 飞蝇菌子 [源码] `Flyconid`。**这里那几个数字是 cooldown 不是权重** ——
/// 重载是 `AddBranch(state, int cooldown, MoveRepeatType)`，权重默认 1。
/// 我第一次读成权重 3:2:1，错了。
///
/// 开局分支只有脆弱孢子和撞击（易伤孢子不在开局池里）。
/// 内核下标：0 = 脆弱孢子，1 = 撞击，2 = 易伤孢子。
static M_FLYCONID: Machine = Machine {
    start: Next::Rand(&[
        Branch { to: 0, weight: 1, repeat: Repeat::NotTwice, cooldown: 2 },
        Branch { to: 1, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
    ]),
    after: &[FLYCONID_RAND, FLYCONID_RAND, FLYCONID_RAND],
};
const FLYCONID_RAND: Next = Next::Rand(&[
    Branch { to: 2, weight: 1, repeat: Repeat::NotTwice, cooldown: 3 },
    Branch { to: 0, weight: 1, repeat: Repeat::NotTwice, cooldown: 2 },
    Branch { to: 1, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
]);

/// 蛮兽 [源码] `Mawler`：三手共用同一个随机分支，起始态是爪击（2 号）。
/// 咆哮那一支是 `UseOnlyOnce` —— 内核第一个 [`Repeat::Once`] 的消费者。
static M_MAWLER: Machine = Machine {
    start: Next::Go(2),
    after: &[MAWLER_RAND, MAWLER_RAND, MAWLER_RAND],
};
const MAWLER_RAND: Next = Next::Rand(&[
    Branch { to: 0, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
    Branch { to: 1, weight: 1, repeat: Repeat::Once, cooldown: 0 },
    Branch { to: 2, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
]);

/// 仪式兽（第 1 幕 Boss）[源码] `CeremonialBeast`。**两个阶段，靠血量闸门切换。**
///
/// 下标：0 耕地 · 1 践地 · 2 眩晕 · 3 兽吼 · 4 践踏 · 5 碾碎
///
/// * 第一阶段：践地(挂耕地150) → 耕地 → 耕地 → …（`PLOW_MOVE` 自指），
///   每次耕地 18 点伤害**并给自己 +2 力量**，所以它一路涨。
/// * 血量掉到 **≤150** 时 `PlowPower` 把它打进 2 号眩晕并**清空全部力量**，
///   规则在 `POWERS` 里，不在这台机器里 —— 机器只负责"眩晕之后去哪"。
/// * 第二阶段：兽吼 → 践踏 → 碾碎 → 兽吼 → …
///
/// 源码里眩晕那一手带 `MustPerformOnceBeforeTransitioning`，内核不用特判：
/// 强制改招把当前手设成 2，`after[2] = Go(3)` 保证它打完眩晕才走。
/// 猎人杀手 [源码] `HunterKiller`：嫩化黏液起手，之后每手都过同一个随机分支。
///
/// 下标：0 嫩化黏液（Debuff，给我挂娇弱）· 1 撕咬 17 · 2 穿刺 7×3
///
/// [源码] `AddBranch(BITE, MoveRepeatType.CannotRepeat)` +
/// `AddBranch(PUNCTURE, 2)`。**那个 2 是 `maxRepeats` 不是权重** ——
/// 两参数重载是 `AddBranch(state, int maxRepeats)`，权重默认 1。
/// （飞蝇菌子那次我把 cooldown 读成了权重，这次专门去翻了重载表。）
/// 所以两手等权，区别只在"能连出几次"。
///
/// **嫩化黏液只在开局出一次**：它不在分支里，`FollowUpState` 指向分支，
/// 而分支只通向撕咬和穿刺。
/// 啃咬机 [源码] `Chomper`：钳夹 <-> 尖啸 严格交替，起手由 `_screamFirst` 掷。
/// 同族信徒 [源码] `KinFollower`：快斩 -> 回旋镖 -> 战舞 定环，
/// **起手是哪一手由站位定** —— `TheKinBoss.GenerateMonsters` 里
/// 第一只 `StartsWithDance = true`（起手战舞），第二只不设（起手快斩）。
///
/// **内核没有站位的概念**（两只共用一个 `EnemyDef`，见「同名的多个 EnemyDef
/// 会互相盖住」），所以这里按**集合**建：开局允许 {快斩, 战舞}。
/// 这不是把"确定的事"当成随机 —— 它陈述的是"这两手都可能"，
/// 而那正是 `allowed_initial` / `--predict-enemy` 消费的那个口径。
/// [实测] 2026-09-09 `act1_f17_boss` 第 0 帧一只信徒摆的正是 Buff。
static M_KIN_FOLLOWER: Machine = Machine {
    start: Next::Rand(&[
        Branch { to: 0, weight: 1, repeat: Repeat::Forever, cooldown: 0 },
        Branch { to: 2, weight: 1, repeat: Repeat::Forever, cooldown: 0 },
    ]),
    after: &[Next::Go(1), Next::Go(2), Next::Go(0)],
};

/// 淤泥旋螺 [源码] `SludgeSpinner`：起手**喷油**，之后三选一（各自不能连出）。
static M_SLUDGE_SPINNER: Machine = Machine {
    start: Next::Go(0),
    after: &[
        Next::Rand(&M_SPINNER_BRANCH),
        Next::Rand(&M_SPINNER_BRANCH),
        Next::Rand(&M_SPINNER_BRANCH),
    ],
};
static M_SPINNER_BRANCH: [Branch; 3] = [
    Branch { to: 0, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
    Branch { to: 1, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
    Branch { to: 2, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
];

/// 噬尸蛞蝓 [源码] `CorpseSlug`：鞭击 -> 吞噬 -> 黏液 定环，
/// **起手由 `StarterMoveIdx % 3` 定**（遭遇按站位给），内核按集合建。
static M_CORPSE_SLUG: Machine = Machine {
    start: Next::Rand(&[
        Branch { to: 0, weight: 1, repeat: Repeat::Forever, cooldown: 0 },
        Branch { to: 1, weight: 1, repeat: Repeat::Forever, cooldown: 0 },
        Branch { to: 2, weight: 1, repeat: Repeat::Forever, cooldown: 0 },
    ]),
    after: &[Next::Go(1), Next::Go(2), Next::Go(0)],
};

/// 双尾鼠 [源码] `TwoTailedRat`：抓挠/病咬/尖啸/呼叫增援。
///
/// **权重是条件的**：`CanSummon()` 为真时召唤 0.75、其余三手各 1/12
/// （⇒ 9 : 1 : 1 : 1），为假时召唤 0、其余各 1。内核的 `Branch::weight` 是常数，
/// 取**能召唤那一档**（9:1:1:1）—— 方向是**高估敌人**（更常召唤），
/// 而 `Repeat::Once` 已经把"一只鼠一辈子只叫一次增援"锁死了。
/// **欠的是 `TurnsUntilSummonable` 那个前摇**（头几回合叫不出来）。
///
/// 尖啸那条 [源码] 写的是 `AddBranch(state, 3, CannotRepeat, …)` ——
/// 那个 3 是**冷却**不是权重（飞蝇菌子踩过这个坑，见 `Branch::cooldown`）。
static M_TWO_TAILED_RAT: Machine = Machine {
    start: Next::Rand(&[
        Branch { to: 0, weight: 1, repeat: Repeat::Forever, cooldown: 0 },
        Branch { to: 1, weight: 1, repeat: Repeat::Forever, cooldown: 0 },
        Branch { to: 2, weight: 1, repeat: Repeat::Forever, cooldown: 0 },
    ]),
    after: &[
        Next::Rand(&M_RAT_BRANCH),
        Next::Rand(&M_RAT_BRANCH),
        Next::Rand(&M_RAT_BRANCH),
        Next::Rand(&M_RAT_BRANCH),
    ],
};
static M_RAT_BRANCH: [Branch; 4] = [
    Branch { to: 0, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
    Branch { to: 1, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
    Branch { to: 2, weight: 1, repeat: Repeat::NotTwice, cooldown: 3 },
    Branch { to: 3, weight: 9, repeat: Repeat::Once, cooldown: 0 },
];

/// 拳击构装体 [源码] `PunchConstruct`：蓄力 -> 快拳 -> 重拳 定环，
/// 起手由 `StartsWithFastPunch` 定（按站位），内核按集合建 {蓄力, 快拳}。
static M_PUNCH_CONSTRUCT: Machine = Machine {
    start: Next::Rand(&[
        Branch { to: 0, weight: 1, repeat: Repeat::Forever, cooldown: 0 },
        Branch { to: 1, weight: 1, repeat: Repeat::Forever, cooldown: 0 },
    ]),
    after: &[Next::Go(1), Next::Go(2), Next::Go(0)],
};

static M_CHOMPER: Machine = Machine {
    start: Next::Rand(&[
        Branch { to: 0, weight: 1, repeat: Repeat::Forever, cooldown: 0 },
        Branch { to: 1, weight: 1, repeat: Repeat::Forever, cooldown: 0 },
    ]),
    after: &[Next::Go(1), Next::Go(0)],
};

/// 残杀千足虫的一节 [源码] `DecimillipedeSegment`（三节共用一个类）。
///
/// 下标：0 扭动 5×2 · 1 壮硕 6+自身力量2 · 2 缠绕 8+给我 debuff ·
/// **3 重接**（接续复活那一手，不在循环里 —— 只有 `St::ReattachDue` 那条规则的
/// `OwnerForceMove(3)` 进得来）
///
/// **平时是纯三循环** `缠绕 -> 壮硕 -> 扭动 -> 缠绕`（`FollowUpState` 首尾相接），
/// 起手由 `StarterMoveIdx % 3` 决定（0 扭动 · 1 壮硕 · 2 缠绕，和内核下标同号），
/// 而 [源码] `DecimillipedeElite` 给 Front / Middle / Back 三节 `num / num+1 / num+2`
/// （`num = Rng.NextInt(3)`）—— **三节两两不同，而且是同一个方向的轮换**。
///
/// **起手写成 `ECond::SlotRep`**（2026-09-15，见 verified-rules §2.14）：
/// 单看一节三手都可能（`allowed_initial` 三手全开）；合成一场仗时按槽位错开
/// （`initial_move` 取 `num = 0`：扭动 / 壮硕 / 缠绕）。槽位 0/1/2 = Front/Middle/Back，
/// 是遭遇表的出场顺序，也是观测里的顺序。
/// [实测] 两条实录的开局都是这个方向的轮换：`act2_f28` 壮硕/缠绕/扭动（`num = 1`）·
/// `act2_f30` 扭动/壮硕/缠绕（`num = 0`）；反方向（`num / num+2 / num+1`）两条都对不上。
/// 在这之前是等权 `Rand`，`initial_move` 取最低位 ⇒ 合成路径上三节全从扭动起、整场同相。
/// **没用 `SlotIs`（咬人卷轴那个写法）**：它会让 `act2_f28` 三节在 `synth_audit` 的开局第一手全报集合外。
///
/// **死了之后**（2026-09-14 建，见 `St::Reattach`）：[源码] `DEAD_MOVE` -> `REATTACH_MOVE`
/// -> 等权随机三选一（`CannotRepeat`）-> 回到上面的循环。`DEAD_MOVE` 内核不建成一手：
/// 死人不出手，那一个敌人回合由 `St::ReattachDue` 的倒计时数掉。
/// [实测] `act2_f28_decimillipede` 两次复活之后的第一手分别是缠绕和壮硕，之后照循环走。
static M_DECIMILLIPEDE: Machine = Machine {
    start: Next::Cond(&[
        (ECond::SlotRep(0), 0),
        (ECond::SlotRep(1), 1),
        (ECond::SlotRep(2), 2),
    ]),
    after: &[
        Next::Go(2),
        Next::Go(0),
        Next::Go(1),
        // 重接之后。`CannotRepeat` 比的是上一手（重接），所以三条都放行
        Next::Rand(&[
            Branch { to: 0, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
            Branch { to: 1, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
            Branch { to: 2, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
        ]),
    ],
};

/// 知识恶魔 [源码] `KnowledgeDemon.GenerateMoveStateMachine`
///
/// ```text
/// 知识的诅咒 -> 抽打 -> 知识过载 -> 思考 -> 条件（诅咒出过不到 3 次 ? 知识的诅咒 : 抽打）
/// 起点 = 知识的诅咒
/// ```
///
/// 所以第 1 / 5 / 9 回合是诅咒，第 13 回合起只剩 抽打 -> 知识过载 -> 思考 三手循环。
/// 条件读的是 [源码] 私有的 `_curseOfKnowledgeCounter`，内核从我身上的诅咒 status 反推
/// （`content::curses_taken`），不另存计数器。
static M_KNOWLEDGE_DEMON: Machine = Machine {
    start: Next::Go(0),
    after: &[
        Next::Go(1),
        Next::Go(2),
        Next::Go(3),
        Next::Cond(&[
            (ECond::CursesTakenBelow(&KNOWLEDGE_CURSES, 3), 0),
            (ECond::CursesTakenAtLeast(&KNOWLEDGE_CURSES, 3), 1),
        ]),
    ],
};

static M_HUNTER_KILLER: Machine = Machine {
    start: Next::Go(0),
    after: &[
        Next::Rand(&M_HK_BRANCH),
        Next::Rand(&M_HK_BRANCH),
        Next::Rand(&M_HK_BRANCH),
    ],
};

static M_HK_BRANCH: [Branch; 2] = [
    Branch { to: 1, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
    Branch { to: 2, weight: 1, repeat: Repeat::AtMost(2), cooldown: 0 },
];

/// 骇鳗（[源码] `TerrorEel.GenerateMoveStateMachine`）。
///
/// 四手全是确定的，一个随机分支都没有：
/// 撞击 <-> 乱舞 两手互相咬着循环，击晕 -> 恐惧 -> 撞击 回到主循环。
/// **击晕那一手进不了循环**，只能由尖叫的 `OwnerForceMove(2)` 打进去。
static M_TERROR_EEL: Machine = Machine {
    start: Next::Go(0),
    after: &[
        Next::Go(1), // 0 撞击 -> 乱舞
        Next::Go(0), // 1 乱舞 -> 撞击
        Next::Go(3), // 2 击晕 -> 恐惧
        Next::Go(0), // 3 恐惧 -> 撞击
    ],
};

static M_BEAST: Machine = Machine {
    start: Next::Go(1),
    after: &[
        // 耕地 -> 耕地，**除非血量已经过了闸门**。
        // 第二个分支是 `Unknown`（求值返回 `None`），在第一个不成立时
        // 把耕地收进集合 —— 等价于"否则继续耕地"。
        Next::Cond(&[(ECond::HpAtMost(150), 2), (ECond::Unknown, 0)]),
        Next::Go(0), // 践地 -> 耕地
        Next::Go(3), // 眩晕 -> 兽吼
        Next::Go(4), // 兽吼 -> 践踏
        Next::Go(5), // 践踏 -> 碾碎
        Next::Go(3), // 碾碎 -> 兽吼
    ],
};

/// 扭动虫 [源码] `Wriggler`：眩晕 → (按槽位分岔) → 啃咬 ⇄ 扭动。
///
/// **分岔条件内核判不了**：源码按 `SlotName == "wriggler1".."wriggler4"` 分，
/// 奇数槽先啃咬、偶数槽先扭动。内核没有"我是第几号召唤物"的概念，
/// 所以用 `ECond::Unknown` —— 允许集合退化成两条，弱，但不会自信地错。
///
/// 眩晕放在 0 号是**有意的**：`summon_one` 把 `enemy_move` 置 0，
/// 于是"召唤出来先眩晕一回合"不依赖任何额外代码。
static M_WRIGGLER: Machine = Machine {
    start: Next::Go(0),
    after: &[
        Next::Cond(&[(ECond::Unknown, 1), (ECond::Unknown, 2)]),
        Next::Go(2),
        Next::Go(1),
    ],
};

/// 异蛙寄生虫 [源码] `PhrogParasite`：感染 ⇄ 鞭击 严格交替，起始是感染。
///
/// 源码里那个 `RandomBranchState` **建了但没接进来**（没有任何 `FollowUpState`
/// 指向它），两手互指才是实际的状态图。照抄图，不照抄声明。
static M_PHROG: Machine = Machine {
    start: Next::Go(0),
    after: &[Next::Go(1), Next::Go(0)],
};

/// 蛇行扼杀者 [源码] `SlitheringStrangler`：缠绕 → {重击|鞭击} → 缠绕 → …
/// 起始态是缠绕（`new MonsterMoveStateMachine(list, moveState)`）。
/// 两条攻击手都 `CanRepeatForever`，所以那个分支的允许集合恒为两条。
static M_STRANGLER: Machine = Machine {
    start: Next::Go(0),
    after: &[
        Next::Rand(&[
            Branch { to: 1, weight: 1, repeat: Repeat::Forever, cooldown: 0 },
            Branch { to: 2, weight: 1, repeat: Repeat::Forever, cooldown: 0 },
        ]),
        Next::Go(0),
        Next::Go(0),
    ],
};

/// 小啃兽 [源码] `Nibbit`：**开场那一手看站位**，之后固定 撞击→啃咬→嘶鸣 循环。
///
/// 站位是**遭遇**在开局钉死的（`NibbitsWeak` 设 IsAlone、`NibbitsNormal`
/// 给第一只设 IsFront），内核用"场上只剩一只非爪牙"和"槽位号最小"近似。
/// 内核下标：0 = 撞击，1 = 啃咬并戒备，2 = 嘶鸣。
static M_NIBBIT: Machine = Machine {
    start: Next::Cond(&[(ECond::Alone, 0), (ECond::NotFront, 2), (ECond::Front, 1)]),
    after: &[Next::Go(1), Next::Go(2), Next::Go(0)],
};

/// 盛碗虫（石）[源码] `BowlbugRock`：头槌 → 如果**失衡**就晕一回合，否则接着头槌。
///
/// 失衡怎么来的见 [`St::Imbalanced`]：**它的攻击被你完全挡住**就会失衡。
/// 所以这只怪的正确打法是"挡满"，而这件事内核原来完全不知道
///（那个 status 的注释写着"语义完全未知"）。
/// 组装师 [源码] `Fabricator.GenerateMoveStateMachine`。
///
/// ```text
/// ConditionalBranch(初始 & 每手之后)
///   ├ CanFabricate  -> RandomBranch{ FABRICATE 权1, FABRICATING_STRIKE 权1 }
///   └ !CanFabricate -> DISINTEGRATE
/// CanFabricate = 同一边活着的生物（含自己）< 4
/// ```
///
/// **`Next` 表达不了「条件分支套随机分支」**（`Cond` 的每一支只能指向一手），
/// 所以这里换了个等价的写法：把随机的那两支写成两条 `ECond::Unknown`。
/// `eval_cond` 对 `Unknown` 返回 `None`，调用方把它当成"可能成立"收进允许集合
/// —— 于是造得动的时候允许集合正好是 {0,1}，造不动的时候是 {2}。
/// 这**不是**打补丁：那两支本来就是随机的，"下一手是哪一个"按不变量 4
/// 本来就预测不了，允许集合正是表达这件事的地方。
///
/// 注意顺序：`AlliesAliveAtLeast(4)` 必须排在两条 `Unknown` **前面**。
/// `Next::Cond` 是"第一个**确定成立**的就是答案"，排后面的话
/// 前面的 `Unknown` 会先把 0/1 收进集合，2 那一支就永远混在里面。
///
/// **一处已知的弱点**：`pick_next` 对 `Next::Cond` 取的是集合里**最小**的下标
///（`trailing_zeros`），所以 rollout 里组装师造得动时永远走 FABRICATE、
/// 一次都不会采样到 FABRICATING_STRIKE（那一手还带 18 点伤害）。
/// **rollout 因此低估这只怪**。对拍和 L2 都不受影响（前者注入观测、
/// 后者用观测意图当威胁），而 rollout 的策略面本来就在"很弱"那一档。
static M_FABRICATOR: Machine = Machine {
    start: Next::Cond(&[
        (ECond::AlliesAliveAtLeast(4), 2),
        (ECond::Unknown, 0),
        (ECond::Unknown, 1),
    ]),
    after: &[
        Next::Cond(&[
            (ECond::AlliesAliveAtLeast(4), 2),
            (ECond::Unknown, 0),
            (ECond::Unknown, 1),
        ]),
        Next::Cond(&[
            (ECond::AlliesAliveAtLeast(4), 2),
            (ECond::Unknown, 0),
            (ECond::Unknown, 1),
        ]),
        Next::Cond(&[
            (ECond::AlliesAliveAtLeast(4), 2),
            (ECond::Unknown, 0),
            (ECond::Unknown, 1),
        ]),
    ],
};

/// 青蛙骑士 [源码] `FrogKnight.GenerateMoveStateMachine`。
///
/// ```text
/// 起始态 = TONGUE_LASH（`new MonsterMoveStateMachine(list, moveState3)`）
/// 舌鞭     -> 除恶
/// 除恶     -> 为了女王
/// 为了女王 -> HALF_HEALTH 分支
/// HALF_HEALTH: 舌鞭     if HasBeetleCharged || CurrentHp >= MaxHp / 2
///              甲虫冲锋 if !HasBeetleCharged && CurrentHp <  MaxHp / 2
/// 甲虫冲锋 -> 舌鞭
/// ```
///
/// 内核下标：0 = 舌鞭，1 = 除恶，2 = 为了女王，3 = 甲虫冲锋。
///
/// **分支写成一支而不是两支**：源码那两个 lambda 是「先判第一条，
/// 不成立才轮到第二条」，而第二条是第一条的严格否定 ——
/// 所以只需要把**甲虫冲锋**那一支的复合条件写准，另一支用 `Unknown`
/// 兜底当 else（和 `M_BEAST` 的耕地闸门同一个写法）。
/// 复合条件必须用 `ECond::All`：拆成两支表达的是"或"，那会让
/// 冲过一次之后每轮都可能再冲，**高估**这只怪的伤害。
///
/// `MaxHp / 2` 是 C# 的整数除法：191 / 2 = 95，所以
/// 「hp < 95」就是 `HpAtMost(94)`。**别写成 95**。
static M_FROG_KNIGHT: Machine = Machine {
    start: Next::Go(0),
    after: &[
        Next::Go(1), // 舌鞭 -> 除恶
        Next::Go(2), // 除恶 -> 为了女王
        // 为了女王 -> 分支
        Next::Cond(&[
            (ECond::All(&[ECond::MoveUnused(3), ECond::HpAtMost(94)]), 3),
            (ECond::Unknown, 0),
        ]),
        Next::Go(0), // 甲虫冲锋 -> 舌鞭
    ],
};

/// 巨斧机器人 [源码] `Axebot.GenerateMoveStateMachine`。
///
/// ```text
/// 启动 -> 锤击上勾拳
/// 锤击上勾拳 <-> 一二连击        （两手互指，无限交替）
/// initialState: _stockOverrideAmount.HasValue ? 启动 : 锤击上勾拳
/// ```
///
/// 最后那一行是要害：**原装那只从锤击上勾拳起手，重生的那些从启动起手**。
/// `_stockOverrideAmount` 只有 `StockPower.AfterDeath` 造出来的那只才有值。
///
/// 内核下标：**0 = 启动**（有意的），1 = 锤击上勾拳，2 = 一二连击。
/// 0 号放启动是因为 `step::summon_one` 把 `enemy_move` 置 0 ——
/// 于是"重生的从启动起手"不依赖任何额外代码，而 `Machine::start` 只在
/// `begin_combat` 走一次、正好只管原装那只。**和扭动虫把眩晕放 0 号是同一招。**
/// 启动没有任何入边，所以原装那只永远走不到它（它的 +0 力量因此不可观测）。
static M_AXEBOT: Machine = Machine {
    start: Next::Go(1),
    after: &[
        Next::Go(1), // 启动 -> 锤击上勾拳
        Next::Go(2), // 锤击上勾拳 -> 一二连击
        Next::Go(1), // 一二连击 -> 锤击上勾拳
    ],
};

/// 灵魂枢纽 [源码] `SoulNexus.GenerateMoveStateMachine`。
///
/// ```text
/// 灵魂灼烧(29) -> RAND
/// 大漩涡(6x4)   -> RAND
/// 汲取生命(18+易伤2+虚弱2) -> RAND
/// RAND = { 灵魂灼烧, 大漩涡, 汲取生命 }，等权，三条都 CannotRepeat
/// 起始态 = 灵魂灼烧
/// ```
///
/// 下标：0 = 灵魂灼烧，1 = 大漩涡，2 = 汲取生命。
/// 初始手是灵魂灼烧（29 点），之后在另外两手里等权随机（不能连出同一手）。
static M_SOUL_NEXUS: Machine = Machine {
    start: Next::Go(0),
    after: &[SOUL_NEXUS_RAND, SOUL_NEXUS_RAND, SOUL_NEXUS_RAND],
};

const SOUL_NEXUS_RAND: Next = Next::Rand(&[
    Branch { to: 0, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
    Branch { to: 1, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
    Branch { to: 2, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
]);

/// 实验体 [源码] `TestSubject.GenerateMoveStateMachine`。
///
/// ```text
/// Phase 1: 啃咬(1) <-> 头槌猛击(2)
/// Phase 2: 连环爪击(3) -> 连环爪击(3) -> ...
/// Phase 3: 撕裂(4) -> 猛扑(5) -> 灼热咆哮(6) -> 撕裂(4) ...
/// 复苏(0): 阶段转换过渡手
/// 起始态 = 啃咬(1)
/// ```
static M_TEST_SUBJECT: Machine = Machine {
    start: Next::Go(1),
    after: &[
        // 0: 复苏 -> 按形态分岔（[源码] REVIVE_BRANCH: Respawns<2 -> 连环爪击，>=2 -> 撕裂）。
        // 内核拿**最大生命**当阶段指示器：200 = 形态 2，300 = 形态 3。
        Next::Cond(&[(ECond::MaxHpAtMost(250), 3), (ECond::Unknown, 4)]),
        Next::Go(2), // 1: 啃咬 -> 头槌猛击
        Next::Go(1), // 2: 头槌猛击 -> 啃咬
        Next::Go(3), // 3: 连环爪击 -> 连环爪击
        Next::Go(5), // 4: 撕裂 -> 猛扑
        Next::Go(6), // 5: 猛扑 -> 灼热咆哮
        Next::Go(4), // 6: 灼热咆哮 -> 撕裂
    ],
};

static M_BOWLBUG_ROCK: Machine = Machine {
    start: Next::Go(0),
    after: &[Next::Cond(&[(ECond::OffBalance, 1), (ECond::NotOffBalance, 0)]), Next::Go(0)],
};

/// 外骨骼虫 [源码] `Exoskeleton.GenerateMoveStateMachine`。
///
/// ```text
/// INIT(按站位)  first -> 疾走 / second -> 大颚 / third -> 激怒 / fourth -> RAND
/// 疾走 -> RAND
/// 大颚 -> 激怒          ← 固定，不是随机
/// 激怒 -> RAND
/// RAND = { 疾走, 大颚 }，等权，两条都 CannotRepeat
/// ```
///
/// **一处刻意的过近似**：`Next::Cond` 的分支只能指向一个**招式下标**，
/// 指不到 `RAND` 那个随机分支节点。所以第 4 格（`fourth`）在表里没有对应项，
/// 落到 `resolve_set` 的兜底 —— 允许集合退化成 {疾走, 大颚, 激怒}，
/// 比真实的 {疾走, 大颚} 大一个。**宁可集合大一点，也不猜一个**
/// （`ECond` 的文档注释里写着这条）。三只的遭遇（`ExoskeletonsWeak`）
/// 用不到第 4 格，四只的（`ExoskeletonsNormal`）还没遇到过。
static M_EXOSKELETON: Machine = Machine {
    start: Next::Cond(&[
        (ECond::SlotIs(0), 0),
        (ECond::SlotIs(1), 1),
        (ECond::SlotIs(2), 2),
    ]),
    after: &[
        // 疾走 -> RAND
        Next::Rand(&[
            Branch { to: 0, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
            Branch { to: 1, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
        ]),
        // 大颚 -> 激怒（[源码] `moveState2.FollowUpState = moveState3`，固定边）
        Next::Go(2),
        // 激怒 -> RAND
        Next::Rand(&[
            Branch { to: 0, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
            Branch { to: 1, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
        ]),
    ],
};

/// 蜂群术士 [源码] `Entomancer.GenerateMoveStateMachine`：一个三手定环，
/// **起点是蜂群**（`new MonsterMoveStateMachine(list, moveState2)`）。
///
/// ```text
/// 蜜——蜂——！(3x7) -> 矛击！(18) -> 喷射信息素(强化) -> 蜜——蜂——！ -> …
/// ```
///
/// **喷射信息素在内核里拆成两手**（下标 2 / 3）。[源码] 是同一个 `MoveState`，分支写在
/// `SpitMove` 的方法体里：`if (蜂房 < 3) { 蜂房 +1; 力量 +1 } else { 力量 +2 }`。
/// 拆开之后分支由机器的条件边表达，读自身层数（`ECond::SelfStatusBelow`）。
/// **等价的前提**：内核在矛击出完那一刻判（`advance_move`），游戏在喷射信息素**执行**那一刻判 ——
/// 两者之间蜂房的层数没有任何东西会改（只有喷射信息素自己加它）。
///
/// 两手的意图签名逐字相同（`Buff`），实况对齐靠 `step::move_reachable_now` 挑出
/// 「当前层数下走得到的那一手」，见 `Replayer::identify_enemies`。
static M_ENTOMANCER: Machine = Machine {
    start: Next::Go(0),
    after: &[
        Next::Go(1),
        Next::Cond(&[
            (ECond::SelfStatusBelow(St::PersonalHive, 3), 2),
            (ECond::SelfStatusAtLeast(St::PersonalHive, 3), 3),
        ]),
        Next::Go(0),
        Next::Go(0),
    ],
};

/// 异螨 [源码] `Myte.GenerateMoveStateMachine`
///
/// ```text
/// 起点 = 条件（SlotName == "first" ? 浓毒 : SlotName == "second" ? 吸吮）
/// 浓毒 -> 啃咬 -> 吸吮 -> 浓毒
/// ```
///
/// 站位名内核没有，用**槽位号**近似（`ECond::SlotIs`，和外骨骼虫同一个近似）：
/// `MytesNormal` 只有 first / second 两格，出场顺序就是槽位 0 / 1。
static M_MYTE: Machine = Machine {
    start: Next::Cond(&[(ECond::SlotIs(0), 0), (ECond::SlotIs(1), 2)]),
    after: &[Next::Go(1), Next::Go(2), Next::Go(0)],
};

/// 熟睡甲虫 [源码] `SlumberingBeetle.GenerateMoveStateMachine`
///
/// ```text
/// 打鼾 -> 条件（HasPower<SlumberPower> ? 打鼾 : 出击）
/// 出击 -> 出击
/// 起点 = 打鼾
/// ```
///
/// 第三手「醒来」不在源码的机器里（`CreatureCmd.Stun` 临时造的态），只能被熟睡那条规则
/// 强制打进去，之后固定接出击（`Stun(.., "ROLL_OUT_MOVE")`）。
///
/// **条件边的阈值是 2 不是「还有没有熟睡」—— 这是时点换算，不是改规则**：
/// [源码] 在**我方回合开始**才掷下一手（`CombatManager.StartTurn` -> `PrepareForNextTurn`），
/// 那时敌人回合末的熟睡 −1 已经发生过；内核在打鼾出完那一刻就推进指针（`advance_move`），
/// 早了那一次 −1。于是「掷的那一刻还有熟睡」⇔「推进那一刻熟睡 ≥ 2」。
/// `--predict-enemy` 拿我方回合开头的观测判下一手，那一刻的层数也还没减，所以两条路是同一个阈值。
/// 写成 ≥ 1 的话，回合末醒来之后它会**多睡一回合**（乐观）。
static M_SLUMBERING_BEETLE: Machine = Machine {
    start: Next::Go(0),
    after: &[
        Next::Cond(&[
            (ECond::SelfStatusAtLeast(St::Slumber, 2), 0),
            (ECond::SelfStatusBelow(St::Slumber, 2), 1),
        ]),
        Next::Go(1),
        Next::Go(1),
    ],
};

/// 乐加维林族母 [源码] `LagavulinMatriarch.GenerateMoveStateMachine`
///
/// ```text
/// 沉睡 -> 条件（HasPower<AsleepPower> ? 沉睡 : 斩击）
/// 斩击 -> 开膛破肚 -> 斩击2 -> 灵魂汲取 -> 斩击 -> …
/// 起点 = 沉睡
/// ```
///
/// 第六手「醒来」不在源码的机器里（`CreatureCmd.Stun(owner, WakeUpMove, "SLASH_MOVE")`
/// 临时造的态），只能被沉睡那条规则强制打进去，之后固定接**斩击**。
///
/// **条件边的阈值是 2 不是「还有没有沉睡」** —— 和熟睡甲虫逐字同一条时点换算，
/// 见 `M_SLUMBERING_BEETLE` 那段：[源码] 在我方回合开始才掷下一手，那时敌人回合末的
/// −1 已经发生；内核在沉睡出完那一刻就推进指针，早了那一次 −1。
/// 写成 ≥ 1 的话它会**多睡一回合**（乐观）。
static M_LAGAVULIN_MATRIARCH: Machine = Machine {
    start: Next::Go(0),
    after: &[
        Next::Cond(&[
            (ECond::SelfStatusAtLeast(St::Asleep, 2), 0),
            (ECond::SelfStatusBelow(St::Asleep, 2), 1),
        ]),
        Next::Go(2),
        Next::Go(3),
        Next::Go(4),
        Next::Go(1),
        Next::Go(1),
    ],
};

/// 墨宝 [源码] `Inklet.GenerateMoveStateMachine`
///
/// ```text
/// 刺击 -> 随机（锐利凝视 | 旋风，等权，各 CannotRepeat）
/// 旋风 -> 刺击 ;  锐利凝视 -> 刺击
/// 起点 = MiddleInklet ? 旋风 : 刺击
/// ```
///
/// 所以是**刺击和那个二选一交替**。两条随机边的 `CannotRepeat` 在这台机器上
/// 其实永远拦不住（上一手必然是刺击），照抄源码留着。
///
/// **起点按槽位，而且是确定的**：[源码] `InkletsNormal` 造三只、**给中间那只**
/// `MiddleInklet = true`，出场顺序就是槽位 0/1/2 —— 遭遇本身一个随机数都没掷。
/// 所以用 `ECond::SlotIs` 不是 `SlotRep`（后者是"遭遇掷了个数、几只按槽位错开"那种，
/// 见千足虫）。
///
/// 源码里还有一个 `INIT_RAND` 分支**是死代码**：它既没进 `list`、也不是 initialState。
static M_INKLET: Machine = Machine {
    start: Next::Cond(&[(ECond::SlotIs(1), 1), (ECond::SlotIs(0), 0), (ECond::SlotIs(2), 0)]),
    after: &[
        Next::Rand(&[
            Branch { to: 2, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
            Branch { to: 1, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
        ]),
        Next::Go(0),
        Next::Go(0),
    ],
};

/// 蟾蜍蝌蚪 [源码] `Toadpole.GenerateMoveStateMachine`
///
/// ```text
/// 起点 = 条件（IsFront ? 带刺 : 旋转）
/// 旋转 -> 带刺 -> 吐刺 -> 旋转
/// ```
///
/// **起点按站位，而且是确定的**：[源码] `ToadpolesWeak.GenerateMonsters` 造两只，
/// 第一只 `IsFront = true`、第二只 `false` —— 遭遇一个随机数都没掷，所以是 `ECond::SlotIs`
/// （和墨宝同一个理由，不是千足虫那种 `SlotRep`）。`IsFront` 是遭遇设的字段、开局就定死，
/// **不是** `ECond::Front`（「槽位最小的活着的那只」）那种会随死亡变的近似 —— 这里用不着它：
/// 起点只判一次。
static M_TOADPOLE: Machine = Machine {
    start: Next::Cond(&[(ECond::SlotIs(0), 2), (ECond::SlotIs(1), 1)]),
    after: &[Next::Go(1), Next::Go(2), Next::Go(0)],
};

/// 化石追踪者 [源码] `FossilStalker.GenerateMoveStateMachine`
///
/// ```text
/// 起点 = 缠上
/// 每一手之后 -> 随机（缠上 | 冲撞 | 甩动，等权，各 CanRepeatXTimes(2)）
/// ```
///
/// `AddBranch(state, 2)` 是 `(state, maxRepeats)` 那个重载（权重默认 1）⇒ `Repeat::AtMost(2)`：
/// **同一手最多连出两次**。[源码] 判的是 `StateLog` 最后两条是不是都是它，
/// 而 `StateLog` 只记招式（两种分支态 `ShouldAppearInLogs => false`），开局那一手缠上也记进去了 ——
/// 和内核的 `enemy_hist` 同一个口径。
static M_FOSSIL_STALKER: Machine = Machine {
    start: Next::Go(1),
    after: &[
        Next::Rand(&FOSSIL_STALKER_RAND),
        Next::Rand(&FOSSIL_STALKER_RAND),
        Next::Rand(&FOSSIL_STALKER_RAND),
    ],
};
static FOSSIL_STALKER_RAND: [Branch; 3] = [
    Branch { to: 1, weight: 1, repeat: Repeat::AtMost(2), cooldown: 0 },
    Branch { to: 0, weight: 1, repeat: Repeat::AtMost(2), cooldown: 0 },
    Branch { to: 2, weight: 1, repeat: Repeat::AtMost(2), cooldown: 0 },
];

/// 结实的卵 [源码] `ToughEgg`：孵化 → 啃咬 → 啃咬 → …（`moveState2` 自指）
static M_TOUGH_EGG: Machine =
    Machine { start: Next::Go(0), after: &[Next::Go(1), Next::Go(1)] };

/// 直飞产卵虫 [源码] `Ovicopter.GenerateMoveStateMachine`
///
/// ```text
/// 产卵 -> 摧毁 ;  营养糊 -> 摧毁 ;  摧毁 -> 嫩化 ;  嫩化 -> 条件( CanLay ? 产卵 : 营养糊 )
/// 起点 = 产卵
/// ```
///
/// **条件用 `ECond::Unknown` 留空**：[源码] `CanLay` 判的是
/// `GetTeammatesOf(Creature).Count(c => c.IsAlive) <= 3`，而内核没有"队友数"
/// 这个条件（`ECond::Alone` 只判"只剩一只"，不是同一件事）。
/// 按 `ECond` 的文档注释，判不出来就退化成**允许集合含两支** ——
/// 集合大一点只是弱，猜一个是自信地错。
static M_OVICOPTER: Machine = Machine {
    start: Next::Go(0),
    after: &[
        Next::Go(1),                                     // 产卵 -> 摧毁
        Next::Go(2),                                     // 摧毁 -> 嫩化
        Next::Cond(&[(ECond::Unknown, 0), (ECond::Unknown, 3)]), // 嫩化 -> 产卵 / 营养糊
        Next::Go(1),                                     // 营养糊 -> 摧毁
    ],
};

/// 胧光怪 [源码] `TheObscura.GenerateMoveStateMachine`
///
/// ```text
/// 起点 幻象 -> RAND{ 穿刺凝视 | 哀嚎 | 硬化打击 }，三条都是 CannotRepeat
/// ```
///
/// **幻象只出现一次**：它是起点，而 `RandomBranchState` 的三条边一条都不指回它
/// —— 和无厌沙虫的液化地面同一个形状。
///
/// 三条边**等权**（[源码] `AddBranch(state, MoveRepeatType.CannotRepeat)`
/// 那个重载权重默认 1，没有 cooldown 参数 —— 飞蝇菌子那次把 cooldown 读成权重
/// 的坑在这里不存在，因为这里根本没有那个数字）。
/// [wiki] 的 pattern 逐字说的也是这个：
/// "chooses randomly between Piercing Gaze, Wail, and Hardening Strike
/// (allowed options equally likely). Cannot use any move twice in a row."
static M_OBSCURA: Machine = Machine {
    start: Next::Go(0),
    after: &[
        // 幻象 / 穿刺凝视 / 哀嚎 / 硬化打击 —— 四手之后都回到同一个随机分支
        Next::Rand(&M_OBSCURA_RAND),
        Next::Rand(&M_OBSCURA_RAND),
        Next::Rand(&M_OBSCURA_RAND),
        Next::Rand(&M_OBSCURA_RAND),
    ],
};

/// 上面那个分支的三条边。**抽成常量是因为四个后继共用同一份** ——
/// 抄四遍的话改一条忘三条是迟早的事（雾菇那两个挥爪就是抄出来的教训）。
static M_OBSCURA_RAND: [Branch; 3] = [
    Branch { to: 1, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
    Branch { to: 2, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
    Branch { to: 3, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
];

/// **两只幻象爪牙共用**：寄生惧魔（胧光怪召的）和利齿之眼（雾菇召的）。
/// [源码] 两只的出招表都是**单手自循环**（`moveState.FollowUpState = moveState`），
/// 两只的 `AfterAddedToRoom` 也都是同一句 `Apply<IllusionPower>`。
///
/// **0 号「复苏」不在这张图里**，它是 `IllusionPower.AfterDeath` 用
/// `SetMoveImmediate` 临时插进来的一手（`MustPerformOnceBeforeTransitioning`），
/// 走完回到原来那一手 —— 内核用 `TOp::OwnerForceMove(0)` 表达，
/// 而这里 0 号的后继写成「回 1 号」正对应源码那个 `FollowUpStateId`。
///
/// **起点是 1 不是 0**：`loop_from` 单独做不到这件事（`move_index` 会让第一手
/// 落在 0 号，也就是开局先复苏一次），所以这两只必须挂机器。
static M_ILLUSION_MINION: Machine =
    Machine { start: Next::Go(1), after: &[Next::Go(1), Next::Go(1)] };

/// 无厌沙虫 [源码] `TheInsatiable.GenerateMoveStateMachine`，**第 2 幕 Boss**
///
/// ```text
/// 起点 液化地面 -> 鞭挞 -> 猛扑撕咬 -> 垂涎 -> 鞭挞2 -> 鞭挞 -> …
/// ```
///
/// 液化地面**只出现一次**（没有任何一手指回它），之后是
/// `鞭挞 → 撕咬 → 垂涎 → 鞭挞2` 的四手定环 —— 实战九个回合逐条对上。
/// 两个鞭挞是**源码里就分开的两个 `MoveState`**（`THRASH_MOVE` /
/// `THRASH_MOVE_2`，动作相同但后继不同），合并会让图长歪。
static M_INSATIABLE: Machine = Machine {
    start: Next::Go(0),
    after: &[
        Next::Go(1), // 液化地面 -> 鞭挞
        Next::Go(2), // 鞭挞 -> 猛扑撕咬
        Next::Go(3), // 猛扑撕咬 -> 垂涎
        Next::Go(4), // 垂涎 -> 鞭挞2
        Next::Go(1), // 鞭挞2 -> 鞭挞
    ],
};

/// 活体盾 [源码] `LivingShield.GenerateMoveStateMachine`：
/// 起手固定盾牌猛击(0)；之后看是否有队友存活：有队友则继续盾牌猛击(0)，独活则进入猛砸(1)并无限循环。
///
/// **条件用 `ECond::Unknown` 留空**：队友数在单怪沙盒对拍里判不准，
/// 退化为允许集合包含 `{盾牌猛击, 猛砸}`。
static M_LIVING_SHIELD: Machine = Machine {
    start: Next::Go(0),
    after: &[
        Next::Cond(&[(ECond::Unknown, 0), (ECond::Unknown, 1)]),
        Next::Go(1),
    ],
};

/// 活雾 [源码] `LivingFog.GenerateMoveStateMachine`：
/// 1: 高阶瓦斯(0, 8点+侵蚀) -> 2: 膨胀(1, 召唤+5点) -> 3: 超级瓦斯冲击(2, 8点) -> 2 -> 3 -> 2...
static M_LIVING_FOG: Machine = Machine {
    start: Next::Go(0),
    after: &[
        Next::Go(1), // 0: 高阶瓦斯 -> 1: 膨胀
        Next::Go(2), // 1: 膨胀 -> 2: 超级瓦斯冲击
        Next::Go(1), // 2: 超级瓦斯冲击 -> 1: 膨胀
    ],
};

/// 花园幽灵鳗 [源码] `PhantasmalGardener.GenerateMoveStateMachine`：
/// 0: 啃咬 (5点) -> 1: 鞭击 (7点) -> 2: 乱舞 (1x3) -> 3: 巨大化 (Buff 3力量) -> 0...
/// 开局由 slot 决定：
/// slot 0 (first) -> 2 (乱舞 1x3)
/// slot 1 (second) -> 0 (啃咬 5)
/// slot 2 (third) -> 1 (鞭击 7)
/// slot 3 (fourth) -> 3 (巨大化 Buff)
static M_PHANTASMAL_GARDENER: Machine = Machine {
    start: Next::Cond(&[
        (ECond::SlotIs(0), 2),
        (ECond::SlotIs(1), 0),
        (ECond::SlotIs(2), 1),
        (ECond::SlotIs(3), 3),
    ]),
    after: &[
        Next::Go(1), // 0: 啃咬 -> 1: 鞭击
        Next::Go(2), // 1: 鞭击 -> 2: 乱舞
        Next::Go(3), // 2: 乱舞 -> 3: 巨大化
        Next::Go(0), // 3: 巨大化 -> 0: 啃咬
    ],
};

/// 瀑布巨兽 [源码] `WaterfallGiant.GenerateMoveStateMachine`：
/// 0: 加压 (Buff) -> 1: 重踏 (15点+虚弱) -> 2: 撞击 (10点) -> 3: 虹吸 (回血10) -> 4: 高压枪 (20点起，每打一次 +5) -> 5: 升压 (13点) -> 1 ...
///
/// 6 / 7 是**死后**那两手，只能由死亡规则强制进来（[源码] `TriggerAboutToBlowState` ->
/// `SetMoveImmediate(AboutToBlowState, forceTransition: true)`，规则在 `POWERS` 的蒸汽喷发）：
/// 6: 即将爆发（Stun，把蒸汽喷发层数记成爆炸伤害）-> 7: 爆炸（打那么多，然后自杀）。
/// `AboutToBlowState.MustPerformOnceBeforeTransitioning = true` 那一半在 `step::enemy_turn`：
/// 它在**自己出招途中**被反伤打死时，指针停在 6，不跟着刚打完的这一手推进。
static M_WATERFALL_GIANT: Machine = Machine {
    start: Next::Go(0),
    after: &[
        Next::Go(1), // 0: 加压 -> 1: 重踏
        Next::Go(2), // 1: 重踏 -> 2: 撞击
        Next::Go(3), // 2: 撞击 -> 3: 虹吸
        Next::Go(4), // 3: 虹吸 -> 4: 高压枪
        Next::Go(5), // 4: 高压枪 -> 5: 升压
        Next::Go(1), // 5: 升压 -> 1: 重踏
        Next::Go(7), // 6: 即将爆发 -> 7: 爆炸
        Next::Go(7), // 7: 爆炸（打完就死了；[源码] FollowUpState 指向自己）
    ],
};

/// 地道虫 [源码] `Tunneler.GenerateMoveStateMachine`：
/// 0: 咬击 (13点) -> 1: 钻地 (Buff+Defend: 格挡32 + 钻地1) -> 2: 地底突袭 (23点) -> 2: 地底突袭 ...
/// 3: 眩晕 (Stun) -> 0: 咬击
/// 当钻地格挡被破时被打进 3: 眩晕
static M_TUNNELER: Machine = Machine {
    start: Next::Go(0),
    after: &[
        Next::Go(1), // 0: 咬击 -> 1: 钻地
        Next::Go(2), // 1: 钻地 -> 2: 地底突袭
        Next::Go(2), // 2: 地底突袭 -> 2: 地底突袭
        Next::Go(0), // 3: 眩晕 -> 0: 咬击
    ],
};

/// 咬人卷轴 [源码] `ScrollOfBiting.GenerateMoveStateMachine`：
/// 0: 大啃 (14) -> 2: 更多牙齿 (+2 力量) -> 1: 咀嚼 (5×2) -> RAND{ 大啃 `CannotRepeat` | 咀嚼 `CanRepeatXTimes(2)` }
///
/// **两个 `AddBranch` 重载别读反**：`AddBranch(chomp, MoveRepeatType.CannotRepeat)` 权重 1；
/// `AddBranch(chew, 2)` 走的是 `(state, int maxRepeats)` —— **2 是最多连出几次，不是权重**。
///
/// **起手按槽位**：[源码] 遭遇里 `StarterMoveIdx` 前三卷是 `num / num+1 / num+2`
/// （`num = Rng.NextInt(3)`，三卷共用），**第四卷写死 2**；机器按 `% 3` 选 大啃 / 咀嚼 / 更多牙齿。
/// 内核固定 `num = 0`（和花园幽灵鳗的 `SlotIs` 起手同一个形状）。**这不是精确等价**，差在两处：
/// * **出手顺序**：槽位小的先出手。`num = 0` 时第 1 回合总是先大啃后咀嚼
///   （真实分布里是 2/3）。纸伤难愈按**打穿的段数**算，格挡够挡一部分时
///   先挨大啃会多被打穿几段 ⇒ 这条近似**偏悲观**
/// * `bin/synth_audit` 的「开局第一手」拿它当允许集合 —— 真实录像上约 2/3 会报集合外。
///   那是这条近似的代价，不是规则错
static M_SCROLL_OF_BITING: Machine = Machine {
    start: Next::Cond(&[
        (ECond::SlotIs(0), 0),
        (ECond::SlotIs(1), 1),
        (ECond::SlotIs(2), 2),
        (ECond::SlotIs(3), 2),
    ]),
    after: &[
        Next::Go(2), // 0: 大啃 -> 2: 更多牙齿
        Next::Rand(&[
            Branch { to: 0, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
            Branch { to: 1, weight: 1, repeat: Repeat::AtMost(2), cooldown: 0 },
        ]), // 1: 咀嚼 -> RAND
        Next::Go(1), // 2: 更多牙齿 -> 1: 咀嚼
    ],
};

/// 连枷骑士 [源码] `FlailKnight.GenerateMoveStateMachine`：
/// 起手 0: 撞击 (15)；三手打完都回到**同一个** RAND —— 战争吟唱 `CannotRepeat` ·
/// 连枷 `AddBranch(flail, 2)` · 撞击 `AddBranch(ram, 2)`，权重都是 1。
/// 那两个 2 是**最多连出几次**不是权重（重载读法见 `M_SCROLL_OF_BITING`）。
const FLAIL_KNIGHT_RAND: Next = Next::Rand(&[
    Branch { to: 2, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
    Branch { to: 1, weight: 1, repeat: Repeat::AtMost(2), cooldown: 0 },
    Branch { to: 0, weight: 1, repeat: Repeat::AtMost(2), cooldown: 0 },
]);
static M_FLAIL_KNIGHT: Machine = Machine {
    start: Next::Go(0),
    after: &[FLAIL_KNIGHT_RAND, FLAIL_KNIGHT_RAND, FLAIL_KNIGHT_RAND],
};

/// 幽灵骑士 [源码] `SpectralKnight.GenerateMoveStateMachine`：
/// 0: 恶咒 -> 1: 灵魂斩击 (15) -> RAND{ 灵魂斩击 `AddBranch(slash, 2)`（最多连出两次）|
/// 灵魂火焰 3×3 `CannotRepeat` }，火焰打完也回到这个 RAND。**恶咒只出一次**（没有边指回它）。
const SPECTRAL_KNIGHT_RAND: Next = Next::Rand(&[
    Branch { to: 1, weight: 1, repeat: Repeat::AtMost(2), cooldown: 0 },
    Branch { to: 2, weight: 1, repeat: Repeat::NotTwice, cooldown: 0 },
]);
static M_SPECTRAL_KNIGHT: Machine = Machine {
    start: Next::Go(0),
    after: &[Next::Go(1), SPECTRAL_KNIGHT_RAND, SPECTRAL_KNIGHT_RAND],
};

pub static ENEMIES: &[EnemyDef] = &[
    // 0 [合成] 沙包：每回合打 12
    EnemyDef {
        name: "<合成:沙包>", max_hp: 100, start_status: &[], loop_from: 0, machine: None,
        moves: &[EnemyMove { name: "攻势", intent: "Attack", ops: &[EOp::Attack { base: 12, hits: 1 }] }],
    },
    // 1 [合成] 难以杀灭 9 的测试用敌人
    //
    // **它原来就叫「外骨骼虫」，那是个真敌人的名字，而名字就是连接键。**
    // 于是 2026-08-22 第2幕第20层真遇到外骨骼虫时，内核静默地把它当成了
    // 这个 22 血、每回合 6 点、只有一招的假人 —— `--predict-enemy` 报
    // 「对不齐」，`solve --live` 报「没认出」。和「始祖虱虫」「Wriggler」
    // 是同一类错，只是方向反过来：**假敌人穿了真名字**。
    // `enemy_names_are_unique_and_no_synthetic_squats_on_a_real_name` 守着这条。
    EnemyDef {
        name: "<合成:难以杀灭9>", max_hp: 22, start_status: &[(St::DamageCap, 9)], loop_from: 0,
        machine: None,
        moves: &[EnemyMove { name: "攻势", intent: "Attack", ops: &[EOp::Attack { base: 6, hits: 1 }] }],
    },
    // 2 [合成] 激怒测试：玩家每打一张技能牌它 +2 力量。
    //
    // 它原来叫「实验体#C8」—— **那是第 3 幕 Boss 的名字**，而那只 Boss
    // 有三阶段 / 隔回合无实体 / 激怒，和这个 100 血单招假人毫无关系。
    // 真打到第 3 幕时会重演外骨骼虫那次的静默错认，所以提前改掉。
    // 同理 0 号原来叫「训练假人」（[源码] 有 `BigDummy`）。
    EnemyDef {
        name: "<合成:激怒2>", max_hp: 100, start_status: &[(St::Rage, 2)], loop_from: 0, machine: None,
        moves: &[EnemyMove { name: "攻势", intent: "Attack", ops: &[EOp::Attack { base: 15, hits: 1 }] }],
    },
    // 3 对拍占位，见 enemy::UNKNOWN
    EnemyDef {
        name: "<未知敌人>", max_hp: 0, start_status: &[], loop_from: 0, machine: None,
        moves: &[EnemyMove { name: "无", intent: "", ops: &[EOp::Nothing] }],
    },
    // 4 [实测+wiki] 小啃兽。实测标签 14/6/8 在力量 2 下和 wiki 的
    // Butt 12 / Slice 6 完全对得上（12+2=14，6，6+2=8）。
    // 两只同场时前后错开起手，内核表达不了，按单只的循环写。
    EnemyDef {
        name: "小啃兽", max_hp: 44, start_status: &[], loop_from: 0,
        machine: Some(&M_NIBBIT),
        moves: &[
            EnemyMove { name: "撞击", intent: "Attack", ops: &[EOp::Attack { base: 12, hits: 1 }] },
            EnemyMove { name: "啃咬并戒备", intent: "Attack",
                ops: &[EOp::Attack { base: 6, hits: 1 }, EOp::Block(5)] },
            EnemyMove { name: "嘶鸣", intent: "Buff", ops: &[EOp::SelfStatus { st: St::Strength, amt: 2 }] },
        ],
    },
    // 5 [实测+wiki] 旧日雕像（第1幕精英）。自带缓慢，我每打出一张牌 +10%。
    // **挥砍取 13 不是 wiki 的 15**：实录标签 23、力量 10、实打 23。
    // `loop_from = 2`：沉睡和苏醒是开场手，之后一直挥砍。
    EnemyDef {
        name: "旧日雕像", max_hp: 127, start_status: &[(St::SlowSource, 10)], loop_from: 2, machine: None,
        moves: &[
            EnemyMove { name: "沉睡", intent: "Sleep", ops: &[EOp::Nothing] },
            EnemyMove { name: "苏醒", intent: "Buff", ops: &[EOp::SelfStatus { st: St::Strength, amt: 10 }] },
            EnemyMove { name: "挥砍", intent: "Attack", ops: &[EOp::Attack { base: 13, hits: 1 }] },
        ],
    },
    // 6 [实测+wiki] 缩小甲虫。缩小是攻击方 **×0.7**（[游戏文本+源码]，2026-08-21
    //   从 ×2/3 改过来）。原来那句"实测四个数据点"是错的 —— 那四个样本在
    //   0.7 和 2/3 下取整后完全相同，从来没分开过。详见 `damage.rs` 里那段注释。
    EnemyDef {
        name: "缩小甲虫", max_hp: 40, start_status: &[], loop_from: 1, machine: None,
        moves: &[
            EnemyMove { name: "缩小", intent: "DebuffStrong",
                ops: &[EOp::PlayerStatus { st: St::Shrink, amt: -1 }] },
            EnemyMove { name: "啃咬", intent: "Attack", ops: &[EOp::Attack { base: 7, hits: 1 }] },
            EnemyMove { name: "踩踏", intent: "Attack", ops: &[EOp::Attack { base: 13, hits: 1 }] },
        ],
    },
    // 7 [实测+wiki] 立柱构造体。开局带人工制品 1 层。
    // 连发基础 7 是实测反推的（力量 2 时标签 9、力量 4 时标签 11）——
    // 这也是"标签含攻击方力量"最早的证据。
    //
    // 连发**同时给自己加力量**：实录里它的意图是 `Attack:9, Buff:` 两个，
    // 是 `--predict-enemy` 报出来的（wiki 那条写着 "No effect documented"）。
    // **只有"有这么个 Buff"是观测到的，+2 这个数是照充能推的** ——
    // 数值错了会让后面几手的伤害标签对不上，那时再改。
    EnemyDef {
        name: "立柱构造体", max_hp: 65, start_status: &[(St::Artifact, 1)], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "充能", intent: "Buff", ops: &[EOp::SelfStatus { st: St::Strength, amt: 2 }] },
            EnemyMove { name: "连发", intent: "Attack",
                ops: &[EOp::Attack { base: 7, hits: 1 }, EOp::SelfStatus { st: St::Strength, amt: 2 }] },
            EnemyMove { name: "连发", intent: "Attack",
                ops: &[EOp::Attack { base: 7, hits: 1 }, EOp::SelfStatus { st: St::Strength, amt: 2 }] },
            EnemyMove { name: "排射", intent: "Attack", ops: &[EOp::Attack { base: 5, hits: 2 }] },
            EnemyMove { name: "潜伏", intent: "Defend", ops: &[EOp::Block(15)] },
        ],
    },
    // 8 [实测+wiki] 毛绒伏地虫。吸气给 7 点力量 —— 之前我记成"效果未观测"。
    EnemyDef {
        name: "毛绒伏地虫", max_hp: 56, start_status: &[], loop_from: 1, machine: None,
        moves: &[
            EnemyMove { name: "酸液", intent: "Attack", ops: &[EOp::Attack { base: 4, hits: 1 }] },
            EnemyMove { name: "吸气", intent: "Buff", ops: &[EOp::SelfStatus { st: St::Strength, amt: 7 }] },
            EnemyMove { name: "酸液", intent: "Attack", ops: &[EOp::Attack { base: 4, hits: 1 }] },
            EnemyMove { name: "酸液", intent: "Attack", ops: &[EOp::Attack { base: 4, hits: 1 }] },
        ],
    },
    // 9-12 [实测+wiki] 史莱姆。`StatusCard` 往**弃牌堆**塞黏液（实测确认）。
    EnemyDef {
        name: "树叶史莱姆（小）", max_hp: 13, start_status: &[], loop_from: 0,
        machine: Some(&M_LEAF_SLIME_S),
        moves: &[
            EnemyMove { name: "撞击", intent: "Attack", ops: &[EOp::Attack { base: 3, hits: 1 }] },
            EnemyMove { name: "吐黏液", intent: "StatusCard",
                ops: &[EOp::AddCardToDiscard { card: card::SLIMED, count: 1 }] },
        ],
    },
    EnemyDef {
        name: "树叶史莱姆（中）", max_hp: 34, start_status: &[], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "吐黏液", intent: "StatusCard",
                ops: &[EOp::AddCardToDiscard { card: card::SLIMED, count: 2 }] },
            EnemyMove { name: "团射", intent: "Attack", ops: &[EOp::Attack { base: 8, hits: 1 }] },
        ],
    },
    EnemyDef {
        name: "树枝史莱姆（小）", max_hp: 10, start_status: &[], loop_from: 0, machine: None,
        moves: &[EnemyMove { name: "撞击", intent: "Attack", ops: &[EOp::Attack { base: 4, hits: 1 }] }],
    },
    // 树枝史莱姆（中）开场两手固定，之后是**随机**在两手里选（不能连出黏液）。
    // 内核只能按固定交替近似，`--predict-enemy` 在它身上会偏。
    EnemyDef {
        name: "树枝史莱姆（中）", max_hp: 28, start_status: &[], loop_from: 0,
        machine: Some(&M_TWIG_SLIME_M),
        moves: &[
            EnemyMove { name: "吐黏液", intent: "StatusCard",
                ops: &[EOp::AddCardToDiscard { card: card::SLIMED, count: 1 }] },
            EnemyMove { name: "团射", intent: "Attack", ops: &[EOp::Attack { base: 11, hits: 1 }] },
        ],
    },
    // 13-15 [实测+wiki] 劫掠者三人组（第1幕第12层）
    EnemyDef {
        name: "劫掠者弩手", max_hp: 18, start_status: &[], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "装填", intent: "Defend", ops: &[EOp::Block(3)] },
            EnemyMove { name: "射击", intent: "Attack", ops: &[EOp::Attack { base: 14, hits: 1 }] },
        ],
    },
    // 斧手的挥砍**实测是"攻击 5 + 获得 5 格挡"同一手**（意图同时显示
    // Attack:5 和 Defend:，事后它身上有 5 点格挡）—— wiki 没写那个格挡。
    EnemyDef {
        name: "劫掠者斧手", max_hp: 20, start_status: &[], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "劈砍并戒备", intent: "Attack",
                ops: &[EOp::Attack { base: 5, hits: 1 }, EOp::Block(5)] },
            EnemyMove { name: "劈砍并戒备", intent: "Attack",
                ops: &[EOp::Attack { base: 5, hits: 1 }, EOp::Block(5)] },
            EnemyMove { name: "重劈", intent: "Attack", ops: &[EOp::Attack { base: 12, hits: 1 }] },
        ],
    },
    // 追踪手：先挂脆弱 2，之后一直放狗（实测标签 `1×8`）
    EnemyDef {
        name: "劫掠者追踪手", max_hp: 24, start_status: &[], loop_from: 1, machine: None,
        moves: &[
            EnemyMove { name: "追踪", intent: "Debuff",
                ops: &[EOp::PlayerStatus { st: St::Frail, amt: 2 }] },
            EnemyMove { name: "放狗", intent: "Attack", ops: &[EOp::Attack { base: 1, hits: 8 }] },
        ],
    },
    // 16 [实测+wiki] 飞蝇菌子 —— 证明"意图标签含防御方易伤"的那只。
    // **它的出招是随机的**（不能连出同一手，孢子还有冷却），内核只能按固定
    // 循环近似，`--predict-enemy` 在它身上永远不会满分。
    EnemyDef {
        name: "飞蝇菌子", max_hp: 47, start_status: &[], loop_from: 0,
        machine: Some(&M_FLYCONID),
        moves: &[
            EnemyMove { name: "脆弱孢子", intent: "Attack",
                ops: &[EOp::Attack { base: 8, hits: 1 }, EOp::PlayerStatus { st: St::Frail, amt: 2 }] },
            EnemyMove { name: "撞击", intent: "Attack", ops: &[EOp::Attack { base: 11, hits: 1 }] },
            EnemyMove { name: "易伤孢子", intent: "Debuff",
                ops: &[EOp::PlayerStatus { st: St::Vulnerable, amt: 2 }] },
        ],
    },
    // 17 [实测+wiki] 雾菇。第一手是**幻象**（召唤利齿之眼），不是普通召唤。
    // 挥爪 8 点并给自己 +1 力量 —— 这解释了实录里头槌标签是 15（14+1）。
    // `loop_from = 1`：幻象只在开场，之后挥爪/头槌交替（真实是带概率的）。
    EnemyDef {
        name: "雾菇", max_hp: 74, start_status: &[], loop_from: 1,
        machine: Some(&M_FOGMOG),
        moves: &[
            EnemyMove { name: "幻象", intent: "Summon",
                ops: &[EOp::Summon { def: enemy::EYE_WITH_TEETH, hp: 6 }] },
            EnemyMove { name: "挥爪", intent: "Attack",
                ops: &[EOp::Attack { base: 8, hits: 1 }, EOp::SelfStatus { st: St::Strength, amt: 1 }] },
            EnemyMove { name: "头槌", intent: "Attack", ops: &[EOp::Attack { base: 14, hits: 1 }] },
            // 3 = 分支版挥爪（[源码] `SWIPE_RANDOM_MOVE`）。动作和 1 完全一样，
            // **只有后继不同**：这一条走完接头槌，1 号走完进随机分支。
            EnemyMove { name: "挥爪", intent: "Attack",
                ops: &[EOp::Attack { base: 8, hits: 1 }, EOp::SelfStatus { st: St::Strength, amt: 1 }] },
        ],
    },
    // 18 [源码+实测] 利齿之眼 `EyeWithTeeth` —— 雾菇的幻象爪牙。
    // **塞的是晕眩不是黏液**（我原先猜错了），每回合 3 张进弃牌堆。
    //
    // **和寄生惧魔逐字同构**（[源码] 两只的 `AfterAddedToRoom` 是同一句
    // `PowerCmd.Apply<IllusionPower>(.., 1m, ..)`，出招表都是单手自循环），
    // 所以共用 `M_ILLUSION_MINION`，0 号也一样是那手插进来的「复苏」。
    //
    // **「是复活还是雾菇重新召唤」这个欠定 2026-09-06 结掉了**，两半证据：
    //   [源码] `IllusionPower.AfterDeath` -> `SetMoveImmediate(REVIVE_MOVE)` -> 回满
    //   [实测] `act1_f15_ninth` 帧5 闪电霹雳+ 打死它（帧6/7 观测里**整只消失**），
    //          帧8 它以 6/6 带着两个 power 回来 —— 而雾菇**那两个回合的意图是
    //          `Attack:8, Buff:` 和 `Attack:15`，都不是 Summon**。
    //          没有第二次召唤，所以只能是它自己复活的。
    EnemyDef {
        name: "利齿之眼", max_hp: 6,
        start_status: &[(St::Minion, 1), (St::Illusion, 1)],
        loop_from: 1,
        machine: Some(&M_ILLUSION_MINION),
        moves: &[
            EnemyMove { name: "复苏", intent: "Heal", ops: &[EOp::Nothing] },
            EnemyMove { name: "扰乱", intent: "StatusCard",
                ops: &[EOp::AddCardToDiscard { card: card::DAZED, count: 3 }] },
        ],
    },
    // 19-20 [实测+wiki] 第1幕 Boss。信徒开局就带爪牙标记 ——
    // 神官一死全场结束，所以要打的是 190 而不是 307。
    // 神官的四手循环是 wiki 给的，我原先只写了三手且顺序错（`--predict-enemy` 报 1/6）。
    EnemyDef {
        name: "同族神官", max_hp: 190, start_status: &[], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "脆弱法球", intent: "Attack",
                ops: &[EOp::Attack { base: 8, hits: 1 }, EOp::PlayerStatus { st: St::Frail, amt: 1 }] },
            EnemyMove { name: "虚弱法球", intent: "Attack",
                ops: &[EOp::Attack { base: 8, hits: 1 }, EOp::PlayerStatus { st: St::Weak, amt: 1 }] },
            EnemyMove { name: "光束", intent: "Attack", ops: &[EOp::Attack { base: 3, hits: 3 }] },
            EnemyMove { name: "仪式", intent: "Buff", ops: &[EOp::SelfStatus { st: St::Strength, amt: 2 }] },
        ],
    },
    // 两个信徒同场时**起手位置不同**（实录里一个先 Buff 一个先攻击）——
    // [源码] `TheKinBoss` 里第一只 `StartsWithDance = true`。
    // 2026-09-09 补了机器（起手按集合建，见 `M_KIN_FOLLOWER`）；
    // 在那之前内核每只都从第 0 手开始，`bin/synth_audit` 的开局第一手那一栏
    // 把它报成**唯一一例「落在允许集合外」**。
    EnemyDef {
        name: "同族信徒", max_hp: 59, start_status: &[(St::Minion, 1)], loop_from: 0,
        machine: Some(&M_KIN_FOLLOWER),
        moves: &[
            EnemyMove { name: "快斩", intent: "Attack", ops: &[EOp::Attack { base: 5, hits: 1 }] },
            EnemyMove { name: "回旋镖", intent: "Attack", ops: &[EOp::Attack { base: 2, hits: 2 }] },
            EnemyMove { name: "战舞", intent: "Buff", ops: &[EOp::SelfStatus { st: St::Strength, amt: 2 }] },
        ],
    },
    // 21-22 [实测+wiki] 盛碗虫（第2幕）。
    // `IMBALANCED_POWER` 的语义 wiki 给了：**被完全格挡就眩晕一回合**。
    // 内核没建模这条（那是个条件分支），所以石这条是低估的。
    EnemyDef {
        name: "盛碗虫（石）", max_hp: 45, start_status: &[(St::Imbalanced, 1)], loop_from: 0,
        machine: Some(&M_BOWLBUG_ROCK),
        moves: &[
            EnemyMove { name: "头槌", intent: "Attack", ops: &[EOp::Attack { base: 15, hits: 1 }] },
            // 1 = 晕眩（[源码] `DIZZY_MOVE`，`StunIntent`）。这一手什么都不做，
            // 并清掉失衡标记（源码 `DizzyMove` 里 `IsOffBalance = false`）。
            // **intent 字符串没实测过** —— 从没见过它晕，因为内核原来根本
            // 不知道"挡满会让它失衡"。第一次真晕的时候对拍会判这个字符串。
            EnemyMove { name: "晕眩", intent: "Stun",
                ops: &[EOp::ClearSelfStatus(St::OffBalance)] },
        ],
    },
    EnemyDef {
        name: "盛碗虫（卵）", max_hp: 22, start_status: &[], loop_from: 0, machine: None,
        moves: &[EnemyMove { name: "撕咬并戒备", intent: "Attack",
            ops: &[EOp::Attack { base: 7, hits: 1 }, EOp::Block(7)] }],
    },
    // 23 [实测+wiki] 偷窃草蜢。实测循环 Thievery(17) -> Flutter -> Hat Trick(21) -> Nab(14) -> Escape
    EnemyDef {
        // [源码] `ThievingHopper.GenerateMoveStateMachine`：五手，纯链式
        // 偷盗 -> 扑翼 -> 帽子戏法 -> 抓取 -> 逃跑，而**逃跑自指**
        // （`moveState5.FollowUpState = moveState5`）—— 所以 `loop_from` 指向逃跑。
        // 原来那条"逃跑准备"是从实录反推的占位，源码里没有这一手，去掉。
        // 伤害 [源码]：偷盗 17 / 帽子戏法 21 / 抓取 14（`DeadlyEnemies` 档 19/23/16）。
        // HP 79（`ToughEnemies` 档 84），[实测] 2026-08-27 正好 79。
        name: "偷窃草蜢", max_hp: 79, start_status: &[(St::EscapeArtist, 5)], loop_from: 4, machine: None,
        moves: &[
            // 偷盗：攻击 + **偷走一张牌**（`CardDebuffIntent`）。
            EnemyMove { name: "偷盗", intent: "Attack",
                ops: &[EOp::Attack { base: 17, hits: 1 }, EOp::StealCard(1)] },
            EnemyMove { name: "扑翼", intent: "Buff", ops: &[EOp::SelfStatus { st: St::Flutter, amt: 5 }] },
            EnemyMove { name: "帽子戏法", intent: "Attack", ops: &[EOp::Attack { base: 21, hits: 1 }] },
            EnemyMove { name: "抓取", intent: "Attack", ops: &[EOp::Attack { base: 14, hits: 1 }] },
            EnemyMove { name: "逃跑", intent: "Escape", ops: &[EOp::Nothing] },
        ],
    },
    // 24 [源码+实测] 盛碗虫（蜜）
    //
    // **名字原来照 wiki 写成「盛碗虫（花蜜）」，是错的** —— 游戏里叫
    // 「盛碗虫（蜜）」。名字就是 `enemy_id` 的连接键，写错等于这只怪没进表：
    // 2026-08-22 第2幕第29层真遇到它，`solve --live` 报「3 只里认出 1 只」。
    // 和「始祖虱虫」「Wriggler」是同一类错，**第三次了**。规矩再写一遍：
    // **敌人名字照游戏抄，不照 wiki 译。**
    //
    // 血量 [源码] A0 是区间 35-38（进阶 36-39），wiki 写的 38 是上界；
    // 实测这一只是 36。表里只能填一个数，取上界 38（不高估玩家）。
    EnemyDef {
        name: "盛碗虫（蜜）", max_hp: 38, start_status: &[], loop_from: 1, machine: None,
        moves: &[
            EnemyMove { name: "猛击", intent: "Attack", ops: &[EOp::Attack { base: 3, hits: 1 }] },
            EnemyMove { name: "强化", intent: "Buff", ops: &[EOp::SelfStatus { st: St::Strength, amt: 15 }] },
            EnemyMove { name: "猛击", intent: "Attack", ops: &[EOp::Attack { base: 3, hits: 1 }] },
        ],
    },
    // 25 [源码+实测] 盛碗虫（丝）
    //
    // 名字同上，原来照 wiki 写成「盛碗虫（蚕丝）」，游戏里叫「盛碗虫（丝）」。
    // 血量 [源码] A0 区间 40-43（进阶 41-44），实测这一只 42，取上界 43。
    EnemyDef {
        name: "盛碗虫（丝）", max_hp: 43, start_status: &[], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "毒唾液", intent: "Debuff", ops: &[EOp::PlayerStatus { st: St::Weak, amt: 1 }] },
            EnemyMove { name: "猛击", intent: "Attack", ops: &[EOp::Attack { base: 8, hits: 1 }] },
        ],
    },
    // 26 [源码+实测] 啃咬机（`Chomper`）。**2026-08-27 首次实战，三处按实测改了**：
    //
    //   1. **名字**：表里原来是 wiki 的「大啃兽」，游戏里显示的是**啃咬机** ——
    //      名字对不上，`identify_enemies` 就认不出它，跨回合推演直接不可用。
    //      这是 wiki 那一档数据最典型的坑：数值大致对，名字不是游戏用的那个。
    //   2. **出场自带 `ArtifactPower 2`**（[源码] `AfterAddedToRoom`）——
    //      免疫 2 次负面效果，wiki 那条完全没提。少了它，求解器会以为
    //      易伤/虚弱挂得上，一整套削弱牌的估值全是错的。
    //   3. **两手严格交替**，起手由 `_screamFirst` 每只各掷各的
    //      （[实测] 同屏两只正好一只先钳夹、一只先尖啸）。原来没有机器 = 固定循环。
    //
    // HP 60-64（`ToughEnemies` 档 63-67），[实测] 两只 62 / 61。
    EnemyDef {
        name: "啃咬机", max_hp: 62, start_status: &[(St::Artifact, 2)], loop_from: 0,
        machine: Some(&M_CHOMPER),
        moves: &[
            EnemyMove { name: "钳夹", intent: "Attack", ops: &[EOp::Attack { base: 8, hits: 2 }] },
            EnemyMove { name: "尖啸", intent: "StatusCard", ops: &[EOp::AddCardToDiscard { card: card::DAZED, count: 3 }] },
        ],
    },
    // 27 [实测] 虱虫之祖（第2幕）—— 整条循环由 `act2_f31_louse.json` 逐回合钉死
    //
    // **这条曾经叫「始祖虱虫」**（照 wiki 的 `louse-progenitor` 直译），而游戏里显示的
    // 是**虱虫之祖**。名字是 `enemy_id()` 唯一的连接键，所以那个译名让
    // `--predict-enemy` 一直把它报成「未知敌人」、`--emit-missing` 一直把它列进
    // 待导入 —— 而表里明明有它。**加敌人时名字要照游戏抄，不要照 wiki 译。**
    //
    // 逐回合证据（意图标签已含它自己的力量）：
    //   回合1 蛛网大炮 `Attack:9, Debuff:`  -> 回合2 玩家 FRAIL_POWER=2    基础 9 + 脆弱 2
    //   回合2 蜷缩生长 `Defend:, Buff:`     -> 回合3 blk=14, STRENGTH=5    格挡 14 + 力量 5
    //   回合3 扑击     `Attack:19`          = 14 + 5 力量
    //   回合4 蛛网大炮 `Attack:14, Debuff:` = 9 + 5 力量                   ⇒ loop_from = 0
    //   回合5 蜷缩生长 `Defend:, Buff:`                                    循环确认
    //
    // 两处按实测改掉了原来照 wiki 填的值：
    //   * `max_hp` 136 -> **134**。wiki 写 "134 - 136"，内核只有一个字段，取实测那个。
    //   * 蜷缩生长的 intent `Buff` -> **`Defend`**。游戏显示 `Defend:, Buff:`，
    //     而 `replay::move_signature` 是「主意图在前、副作用意图在后」。写成 Buff
    //     会让签名变成 `Buff, Defend`，顺序对不上，整只敌人判「对不齐」。
    //
    // 开局的**蜷身 14** 是 status 不是招式，规则在 `POWERS` 表里（`St::CurlUp`）。
    EnemyDef {
        name: "虱虫之祖", max_hp: 134, start_status: &[(St::CurlUp, 14)], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "蛛网大炮", intent: "Attack", ops: &[EOp::Attack { base: 9, hits: 1 }, EOp::PlayerStatus { st: St::Frail, amt: 2 }] },
            EnemyMove { name: "蜷缩生长", intent: "Defend", ops: &[EOp::Block(14), EOp::SelfStatus { st: St::Strength, amt: 5 }] },
            EnemyMove { name: "扑击", intent: "Attack", ops: &[EOp::Attack { base: 14, hits: 1 }] },
        ],
    },
    // 28 [源码] 异螨 `Myte`（2026-09-14 批 4 原地重建；**一条实录都没有**）
    //
    // 这个下标原来是一条 [wiki] 档的「螨虫」，**是错的**（浓毒建成了「虚弱 2」），09-14 先摘成占位、
    // 同一天照 [源码] 填回来。名字（怪物和招式）取自游戏本地化表：`MYTE.name` = 异螨。
    //
    // 血量 61–67（`ToughEnemies` 64–69），表里取上界 67，合成路径从 `asc::hp_range` 掷。
    // 定环 浓毒 -> 啃咬 -> 吸吮 -> 浓毒；**起手按站位**（[源码] 初始态是读 `SlotName` 的
    // `ConditionalBranchState`）：`first` 先浓毒、`second` 先吸吮，见 `M_MYTE`。
    //   浓毒：往我**手牌**塞 2 张毒素（`StatusIntent(2)`，写死的 `_toxicCount`，不吃进阶；手满溢出进弃牌堆）
    //   啃咬：13（`DeadlyEnemies` 15）
    //   吸吮：4 + 自身力量 2（`DeadlyEnemies` 6 / 3）
    // 毒素那张牌早就在内核里（留在手里到回合末受 5 点，走格挡；打出去 1 费消耗）。
    EnemyDef {
        name: "异螨", max_hp: 67, start_status: &[], loop_from: 0,
        machine: Some(&M_MYTE),
        moves: &[
            EnemyMove { name: "浓毒", intent: "StatusCard",
                ops: &[EOp::AddCardToHand { card: card::TOXIC, count: 2 }] },
            EnemyMove { name: "啃咬", intent: "Attack", ops: &[EOp::Attack { base: 13, hits: 1 }] },
            EnemyMove { name: "吸吮", intent: "Attack",
                ops: &[EOp::Attack { base: 4, hits: 1 }, EOp::SelfStatus { st: St::Strength, amt: 2 }] },
        ],
    },
    // 29 [源码+实测] 直飞产卵虫 `Ovicopter`（2026-08-22 第2幕第24层首遇）
    //
    // > **这一条原来叫「直升虫」，是照 wiki 译的名字，游戏里叫「直飞产卵虫」。**
    // > 名字是 `enemy_id` 的连接键，写错等于这只怪没进表 —— 实战当场报
    // > 「1 只里认出 0 只」。这是这个坑的**第四次**（始祖虱虫 / Wriggler /
    // > 盛碗虫（花蜜）(蚕丝) / 这条）。规矩再写一遍：**照游戏抄，别照 wiki 译。**
    // >
    // > 同一次按源码改掉了 wiki 那条的三处错：召唤的是**结实的卵**不是盛碗虫（卵）、
    // > 一次召 **3 只**不是 1 只、以及出招是**带条件的状态机**不是固定循环。
    // > 数值 wiki 蒙对了，但那不构成"它可信"。
    //
    // A0：血量 124-130（进阶 126-132；实测 130，取上界）；
    // 摧毁 16（进阶 17）· 嫩化 7 + 易伤 2（进阶 8）· 营养糊 自身力量 +3（进阶 4）。
    // 出招见 `M_OVICOPTER`（`CanLay` 那条条件为什么留空写在那里）。
    EnemyDef {
        name: "直飞产卵虫", max_hp: 130, start_status: &[], loop_from: 0,
        machine: Some(&M_OVICOPTER),
        moves: &[
            EnemyMove { name: "产卵", intent: "Summon",
                ops: &[EOp::Summon { def: enemy::TOUGH_EGG, hp: 18 },
                       EOp::Summon { def: enemy::TOUGH_EGG, hp: 18 },
                       EOp::Summon { def: enemy::TOUGH_EGG, hp: 18 }] },
            EnemyMove { name: "摧毁", intent: "Attack", ops: &[EOp::Attack { base: 16, hits: 1 }] },
            // 观测到的意图签名是 `Attack, Debuff`，主意图是 Attack
            EnemyMove { name: "嫩化", intent: "Attack",
                ops: &[EOp::Attack { base: 7, hits: 1 },
                       EOp::PlayerStatus { st: St::Vulnerable, amt: 2 }] },
            EnemyMove { name: "营养糊", intent: "Buff",
                ops: &[EOp::SelfStatus { st: St::Strength, amt: 3 }] },
        ],
    },
    // 30 [源码+实测] 棘蟾 `SpinyToad`（第 2 幕杂兵）
    //
    // A0/A2：血量 118（116~119，进阶 ToughEnemies 121~124）。
    // 出招：
    // 0: 突刺荆棘：BuffIntent，自身获得 5 层荆棘 (Thorns)
    // 1: 尖刺爆炸：23 点伤害，失去 5 层荆棘
    // 2: 舌刺：17 点伤害
    EnemyDef {
        name: "棘蟾", max_hp: 118, start_status: &[], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "突刺荆棘", intent: "Buff", ops: &[EOp::SelfStatus { st: St::Thorns, amt: 5 }] },
            EnemyMove { name: "尖刺爆炸", intent: "Attack", ops: &[EOp::Attack { base: 23, hits: 1 }, EOp::SelfStatus { st: St::Thorns, amt: -5 }] },
            EnemyMove { name: "舌刺", intent: "Attack", ops: &[EOp::Attack { base: 17, hits: 1 }] },
        ],
    },
    // 31 [wiki] 蜂后 —— **第 3 幕** Boss `QueenBoss` 的一半（原来标成「Act 2 Boss」，标错了）。
    // **这条定义已知是错的**：缺「你是我的了」、「为我燃烧」的力量给队友不给自己、
    // 出招按火炬头死没死分叉。见 roadmap「第 3 幕还剩的一批」。
    EnemyDef {
        name: "蜂后", max_hp: 400, start_status: &[], loop_from: 2, machine: None,
        moves: &[
            EnemyMove { name: "傀儡丝线", intent: "Debuff", ops: &[EOp::PlayerStatus { st: St::Weak, amt: 3 }] },
            EnemyMove { name: "为我燃烧", intent: "Buff", ops: &[EOp::Block(20), EOp::SelfStatus { st: St::Strength, amt: 1 }] },
            EnemyMove { name: "斩首", intent: "Attack", ops: &[EOp::Attack { base: 3, hits: 5 }] },
            EnemyMove { name: "处决", intent: "Attack", ops: &[EOp::Attack { base: 15, hits: 1 }] },
            EnemyMove { name: "狂怒", intent: "Buff", ops: &[EOp::SelfStatus { st: St::Strength, amt: 2 }] },
        ],
    },
    // 32 [wiki] 造门者 —— **不在任何一幕的遭遇池里**（`data/encounters.json` 四幕都没有它，
    // `data/enemy_ids.json` 也没有类名连到这里）。原来标成「Act 2 Boss」，标错了；
    // 大概是 wiki 上一个旧版本的 Boss。留着只因为删掉会让后面的下标整体平移。
    EnemyDef {
        name: "造门者", max_hp: 489, start_status: &[], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "射线", intent: "Attack", ops: &[EOp::Attack { base: 31, hits: 1 }] },
            EnemyMove { name: "退回门中", intent: "Attack", ops: &[EOp::Attack { base: 40, hits: 1 }, EOp::SelfStatus { st: St::Strength, amt: 5 }] },
        ],
    },
    // 33 [源码] 知识恶魔 `KnowledgeDemon`，**第 2 幕 Boss**（2026-09-14 按源码重建，**未实测**）
    //
    // 原来这一条是 [wiki] 档的，三处错（诅咒建成「虚弱 2 + 脆弱 2」· 思考不回血 ·
    // `loop_from = 1` 让诅咒整场只出一次）。2026-09-14 先换成占位，同一天按源码重建。
    // 名字取自游戏本地化表：知识恶魔 · 知识的诅咒 / 抽打 / 知识过载 / 思考；
    // 四个诅咒是 瓦解 / 心灵腐化 / 懒惰 / 虚脱（`*_POWER.title`）。
    //
    // A0：血量 379（A8 399，不是区间）· 抽打 17（A9 18）· 知识过载 8×3（9×3）·
    // 思考 11（13）+ **回 30 血** + 自身力量 2（3）。出招见 `M_KNOWLEDGE_DEMON`。
    //
    // **这场仗的核心是知识的诅咒**：不打人，弹一个**不能跳过**的二选一（`canSkip` 默认 false），
    // 三次各是 瓦解 6 / 心灵腐化 1 · 瓦解 7 / 懒惰 3 · 瓦解 8 / 虚脱 1，全是永久的。
    // 选哪边是**玩家的决策**，内核照 `State::curse_policy` 执行：L3 把 8 种选法配对比一遍，
    // L2 不给它定价（价值全在后面几个回合）。
    EnemyDef {
        name: "知识恶魔", max_hp: 379, start_status: &[], loop_from: 0,
        machine: Some(&M_KNOWLEDGE_DEMON),
        moves: &[
            EnemyMove { name: "知识的诅咒", intent: "Debuff",
                ops: &[EOp::CurseOfKnowledge(&KNOWLEDGE_CURSES)] },
            EnemyMove { name: "抽打", intent: "Attack", ops: &[EOp::Attack { base: 17, hits: 1 }] },
            EnemyMove { name: "知识过载", intent: "Attack", ops: &[EOp::Attack { base: 8, hits: 3 }] },
            // 意图是 `SingleAttackIntent, HealIntent, BuffIntent` —— 签名按 ops 的顺序生成副作用意图
            EnemyMove { name: "思考", intent: "Attack",
                ops: &[EOp::Attack { base: 11, hits: 1 },
                       EOp::Heal(30),
                       EOp::SelfStatus { st: St::Strength, amt: 2 }] },
        ],
    },

    // ---------------------------------------------------------------------
    // 34/35 [实测] 第2幕 Boss 双怪：碾碎爪 + 火箭
    //
    // 出招表整条由 `act2_f33_boss_crusher.json` 七个回合逐条确认，**但这场仗里有
    // 三条内核完全没有的机制**，加它们之前先知道会错在哪：
    //
    //   1. **遭到包围 / 背后攻击**（玩家 `SURROUNDED_POWER`，敌人
    //      `BACK_ATTACK_LEFT_POWER` / `BACK_ATTACK_RIGHT_POWER`）：
    //      背后打来的伤害 **×1.5**，而打出一张指向性牌会转身、改变谁在背后。
    //      **意图标签含这个乘区** —— 实测同一手在转身前后从 49 变 33。
    //   2. **蟹之怒**（`CRAB_RAGE_POWER`）：一只死了，另一只 +6 力量 +99 格挡。
    //   3. 进阶两个轴（2026-09-14 读 [源码] `Crusher` / `Rocket` 解开的；这里原来写着
    //      「两个括号不是同一个变体轴，没分辨出来」）：**血量走 A8 `ToughEnemies`**
    //      （209→219 / 199→209），**伤害和力量走 A9 `DeadlyEnemies`**。实录全是 A0/A1，
    //      所以两列都是低档，下面写的就是低档。碾碎爪有三个 A9 数（摧折 / 戒备打击 14、
    //      适应 +3）生成器按值认不出（和别的 op 撞值），手填在 `data/ascension_overrides.json`。
    //
    // 后果：默认对拍模式**不受影响**（它注入观测到的标签，乘区已经在标签里）；
    // 但 `--predict-enemy` 算出来的数字会在背后攻击的回合偏低 1/3，落进
    // 「只对上类型」。那不是内核算错，是这个模式看不见朝向 —— 和它看不见玩家
    // 易伤是同一类局限。**跨回合 rollout 在朝向建模之前不要用这两只。**
    //
    // 下面每一手的基础值都是从标签反推的，附了判据：
    //
    //   碾碎爪  回合1 `Attack:18`          = 12 × 1.5（背后）      摧折 12
    //           回合2 `Attack:4`           = 4（正面）             增幅打击 4
    //           回合3 `Attack:9×2, Debuff:`= 6 × 1.5（背后）       虫刺 6×2
    //                 -> 回合4 玩家 WEAK=2 且 FRAIL=2              虚弱2 + 脆弱2 都落地
    //           回合4 `Buff:`              -> 回合5 自身 STRENGTH=2  适应 +2 力量
    //           回合5 `Attack:21, Defend:` = (12+2) × 1.5（背后）  戒备打击 12 + 格挡 18
    //                 -> 回合6 自身 blk=18                        格挡 18 实测
    //           回合6 `Attack:14`          = 12+2（正面）          回到摧折 ⇒ loop_from = 0
    //           回合7 `Attack:6`           = 4+2（正面）           增幅打击，循环确认
    //
    //   火箭    回合1 `Attack:3`           = 3（正面）             瞄准镜 3
    //           回合2 `Attack:27`          = 18 × 1.5（背后）      精准光束 18
    //           回合3 `Buff:`              -> 回合4 自身 STRENGTH=2  充能 +2 力量
    //           回合4 `Attack:49`          = (31+2) × 1.5 = 49.5 → 49  激光 31
    //           回合5 `Sleep:`                                     充电（什么都不做）
    //           回合6 `Attack:7`           = (3+2) × 1.5 = 7.5 → 7 回到瞄准镜 ⇒ loop_from = 0
    //           回合7 `Attack:30`          = (18+2) × 1.5          精准光束，循环确认
    //
    // 招式的**中文名是照 wiki 的英文 id 译的**（trace 只记意图类型和数字，不记招名），
    // 纯显示用，不参与任何比较。数字和 intent 才是实测的。
    //
    // 火箭「充电」的 intent 实测是 **`Sleep`**，而 wiki 那一列写的是 `Utility` ——
    // 又一个「不要照抄 wiki 的 intent 列」的例子（见 `EnemyMove::intent` 的文档）。
    //
    // `start_status` 2026-09-09 补上了。**原来这里写着「包围/背后攻击/蟹之怒
    // 三个 status 内核都没有」—— 那句话过期了**：三个 status 后来都建了
    // （`Surrounded` 进伤害管线的背后 ×1.5、`BackAttackLeft/Right` 是站位、
    // `CrabRage` 有 `AllyDied` 规则），只有这张表没跟着改。
    // 对拍路径上看不见，因为 `sync` 每帧从观测重灌这三个；
    // **合成路径没有观测**，`bin/synth_audit` 在 `act2_f33_boss_crusher` 上
    // 一次报了 5 处（玩家 Surrounded + 两只各两个）。
    //
    // [源码] `Crusher.AfterAddedToRoom`：`BackAttackLeftPower(1)` + `CrabRagePower(1)`
    // 都挂自己身上；`Rocket` 同形但是 `BackAttackRightPower`，**外加给对面挂
    // `SurroundedPower(1)`** —— 最后那个挂在玩家身上，`EnemyDef::start_status`
    // 装不下，走 `ENEMY_START_PLAYER_STATUS`。
    EnemyDef {
        name: "碾碎爪", max_hp: 209,
        start_status: &[(St::BackAttackLeft, 1), (St::CrabRage, 1)],
        loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "摧折", intent: "Attack", ops: &[EOp::Attack { base: 12, hits: 1 }] },
            EnemyMove { name: "增幅打击", intent: "Attack", ops: &[EOp::Attack { base: 4, hits: 1 }] },
            EnemyMove { name: "虫刺", intent: "Attack", ops: &[EOp::Attack { base: 6, hits: 2 }, EOp::PlayerStatus { st: St::Weak, amt: 2 }, EOp::PlayerStatus { st: St::Frail, amt: 2 }] },
            EnemyMove { name: "适应", intent: "Buff", ops: &[EOp::SelfStatus { st: St::Strength, amt: 2 }] },
            EnemyMove { name: "戒备打击", intent: "Attack", ops: &[EOp::Attack { base: 12, hits: 1 }, EOp::Block(18)] },
        ],
    },
    EnemyDef {
        name: "火箭", max_hp: 199,
        start_status: &[(St::BackAttackRight, 1), (St::CrabRage, 1)],
        loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "瞄准镜", intent: "Attack", ops: &[EOp::Attack { base: 3, hits: 1 }] },
            EnemyMove { name: "精准光束", intent: "Attack", ops: &[EOp::Attack { base: 18, hits: 1 }] },
            EnemyMove { name: "充能", intent: "Buff", ops: &[EOp::SelfStatus { st: St::Strength, amt: 2 }] },
            EnemyMove { name: "激光", intent: "Attack", ops: &[EOp::Attack { base: 31, hits: 1 }] },
            EnemyMove { name: "充电", intent: "Sleep", ops: &[EOp::Nothing] },
        ],
    },
    // 蛮兽 [源码] `Mawler`（2026-08-22 补，实录 `act1_f5_new_fourth` 里
    // 它是第一只内核不认识的敌人）。
    //
    // 三手都从一个共享的随机分支出来，起始态是**爪击**
    // （[源码] `new MonsterMoveStateMachine(list, moveState3)`）：
    // * 撕裂 14 单段  —— `CannotRepeat`
    // * 咆哮 易伤 3   —— **`UseOnlyOnce`**，这是内核第一个用 [`Repeat::Once`] 的
    // * 爪击 4×2      —— `CannotRepeat`
    //
    // A0 血量 72（`ToughEnemies` 才是 76）；伤害取的也是非 `DeadlyEnemies`
    // 那一档（14/4，进阶档是 16/5）。**这一局是 A0，别把进阶值填进来。**
    //
    // 实录逐帧对得上：R1 爪击 `4×2`、R2 撕裂 `14`、R3 咆哮 `Debuff`、
    // R4 爪击标签 `6×2` = 4×1.5（咆哮给的易伤已经算进标签里，
    // 意图标签是最终值）。
    EnemyDef {
        name: "蛮兽", max_hp: 72, start_status: &[], loop_from: 0,
        machine: Some(&M_MAWLER),
        moves: &[
            EnemyMove { name: "撕裂", intent: "Attack", ops: &[EOp::Attack { base: 14, hits: 1 }] },
            EnemyMove { name: "咆哮", intent: "Debuff",
                ops: &[EOp::PlayerStatus { st: St::Vulnerable, amt: 3 }] },
            EnemyMove { name: "爪击", intent: "Attack", ops: &[EOp::Attack { base: 4, hits: 2 }] },
        ],
    },
    // 闪光贾克斯果 [源码] `SnappingJaxfruit`（2026-08-22 实战第14层首遇）。
    //
    // **只有一手，自己接自己**（`moveState.FollowUpState = moveState`）：
    // 攻击 3 + 给自己力量 2，所以它的攻击每回合涨 2。
    // A0 血量 31-33（观测 31）；`ToughEnemies` 才是 34-36。
    // 伤害取非 `DeadlyEnemies` 那一档的 3（进阶是 4）。
    //
    // 意图是**两个标签**（`SingleAttackIntent` + `BuffIntent`），
    // 观测里就是 `Attack:3, Buff:` —— `intent` 这一栏按主意图填 Attack
    // （意图签名的规矩：主意图在前）。
    EnemyDef {
        name: "闪光贾克斯果", max_hp: 31, start_status: &[], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "能量球", intent: "Attack",
                ops: &[EOp::Attack { base: 3, hits: 1 }, EOp::SelfStatus { st: St::Strength, amt: 2 }] },
        ],
    },
    // 蛇行扼杀者 [源码] `SlitheringStrangler`（2026-08-22 实战第14层首遇）。
    //
    // 起始态是**缠绕**，之后 缠绕 → {重击 | 鞭击} → 缠绕 → … 交替：
    // 两条攻击手的 `FollowUpState` 都指回缠绕，缠绕的后继才是那个随机分支。
    // 分支两条都是 `CanRepeatForever`，所以允许集合恒为 {重击, 鞭击}。
    //
    // A0 血量 53-55（观测 55）；重击 7 + 给自己 5 格挡，鞭击 12。
    // 缠绕 3 层，规则见 `St::Constrict`。
    EnemyDef {
        name: "蛇行扼杀者", max_hp: 55, start_status: &[], loop_from: 0,
        machine: Some(&M_STRANGLER),
        moves: &[
            EnemyMove { name: "缠绕", intent: "Debuff",
                ops: &[EOp::PlayerStatus { st: St::Constrict, amt: 3 }] },
            EnemyMove { name: "重击", intent: "Attack",
                ops: &[EOp::Attack { base: 7, hits: 1 }, EOp::Block(5)] },
            EnemyMove { name: "鞭击", intent: "Attack", ops: &[EOp::Attack { base: 12, hits: 1 }] },
        ],
    },
    // 39 扭动虫 [源码] `Wriggler`（异蛙寄生虫死后的第二阶段，2026-08-22 实战第15层精英）
    //
    // **名字必须是游戏里显示的「扭动虫」，不是源码类名 `Wriggler`** ——
    // 名字就是敌人的连接键。我第一版照类名写，实战里当场报
    // 「4 只里认出 0 只」。和「始祖虱虫」那个乌龙是同一类错误。
    // A0 血量 17-21，取中位 19 —— **实战里以观测为准**，这个数只在内核
    // 自己推演（rollout / L3）时用得上。
    // 扭动 = 给我 1 张感染进弃牌堆 + 自己力量 +2（[源码] 两件事一起做）。
    EnemyDef {
        name: "扭动虫", max_hp: 19, start_status: &[], loop_from: 0,
        machine: Some(&M_WRIGGLER),
        moves: &[
            EnemyMove { name: "眩晕", intent: "Stun", ops: &[EOp::Nothing] },
            EnemyMove { name: "啃咬", intent: "Attack", ops: &[EOp::Attack { base: 6, hits: 1 }] },
            EnemyMove { name: "扭动", intent: "Buff",
                ops: &[
                    EOp::AddCardToDiscard { card: card::INFECTION, count: 1 },
                    EOp::SelfStatus { st: St::Strength, amt: 2 },
                ] },
        ],
    },
    // 40 异蛙寄生虫 [源码] `PhrogParasite`（第1幕精英，2026-08-22 首遇）
    //
    // A0 血量 61-64（观测 64）。开局自带寄生物 4 层 —— 那是**死亡时召唤
    // 4 只 Wriggler**，规则在 `POWERS` 的 `Hook::EnemyDied`。
    // 打死它**不算赢**（[源码] `ShouldStopCombatFromEnding`）。
    //
    // 感染那一手是 `StatusIntent(3)`，往我的**弃牌堆**塞 3 张感染
    // （[源码] `CardPileCmd.AddToCombatAndPreview<Infection>(.., PileType.Discard, 3, ..)`）。
    EnemyDef {
        name: "异蛙寄生虫", max_hp: 64, start_status: &[(St::Infested, 4)],
        loop_from: 0, machine: Some(&M_PHROG),
        moves: &[
            EnemyMove { name: "感染", intent: "StatusCard",
                ops: &[EOp::AddCardToDiscard { card: card::INFECTION, count: 3 }] },
            EnemyMove { name: "鞭击", intent: "Attack", ops: &[EOp::Attack { base: 4, hits: 4 }] },
        ],
    },
    // 41 仪式兽 [源码] `CeremonialBeast`，**第 1 幕 Boss**（2026-08-22 首遇）
    //
    // A0 血量 252（`ToughEnemies` 是 262）。伤害都取非 `DeadlyEnemies` 档：
    // 耕地 18 / 践踏 15 / 碾碎 17，碾碎的力量 3（进阶 4），耕地的力量固定 2。
    // 耕地闸门 150（进阶 160）。
    //
    // **兽吼给我挂轰鸣 = 那一回合只能出 1 张牌**，消费点在 `legal_actions`。
    // 真实机制是牌级附魔，内核降成玩家 status，见 `St::Ringing`。
    EnemyDef {
        name: "仪式兽", max_hp: 252, start_status: &[], loop_from: 0,
        machine: Some(&M_BEAST),
        moves: &[
            EnemyMove { name: "耕地", intent: "Attack",
                ops: &[EOp::Attack { base: 18, hits: 1 },
                       EOp::SelfStatus { st: St::Strength, amt: 2 }] },
            EnemyMove { name: "践地", intent: "Buff",
                ops: &[EOp::SelfStatus { st: St::Plow, amt: 150 }] },
            EnemyMove { name: "眩晕", intent: "Stun", ops: &[EOp::Nothing] },
            EnemyMove { name: "兽吼", intent: "Debuff",
                ops: &[EOp::PlayerStatus { st: St::Ringing, amt: 1 }] },
            EnemyMove { name: "践踏", intent: "Attack", ops: &[EOp::Attack { base: 15, hits: 1 }] },
            EnemyMove { name: "碾碎", intent: "Attack",
                ops: &[EOp::Attack { base: 17, hits: 1 },
                       EOp::SelfStatus { st: St::Strength, amt: 3 }] },
        ],
    },
    // 42 外骨骼虫 [源码] `Exoskeleton`（2026-08-22 第2幕第20层首遇，三只）
    //
    // 数值全取 A0 档（源码里每个数都带一个进阶分支，别抄错那一列）：
    //   血量   `MinInitialHp` 24 / `MaxInitialHp` 28（进阶 `ToughEnemies` 是 25/29）
    //   疾走   1 点 × **3** 段（进阶 `DeadlyEnemies` 是 4 段）
    //   大颚   **8** 点（进阶 9）
    //   激怒   自身力量 +2（没有进阶分支）
    //   开局   `AfterAddedToRoom` 挂 `HardToKillPower` **9**
    //
    // **血量是个区间，表里只能填一个数。** 实测三只 24 / 25 / 28，
    // 取上界 28 —— 和内核别处一样选**不高估玩家**的那边（敌人当作更硬）。
    // 对拍那条路不受影响：`sync` 用的是观测到的血量。
    //
    // 出招见 `M_EXOSKELETON`。
    EnemyDef {
        name: "外骨骼虫", max_hp: 28, start_status: &[(St::DamageCap, 9)], loop_from: 0,
        machine: Some(&M_EXOSKELETON),
        moves: &[
            EnemyMove { name: "疾走", intent: "Attack", ops: &[EOp::Attack { base: 1, hits: 3 }] },
            EnemyMove { name: "大颚", intent: "Attack", ops: &[EOp::Attack { base: 8, hits: 1 }] },
            EnemyMove { name: "激怒", intent: "Buff",
                ops: &[EOp::SelfStatus { st: St::Strength, amt: 2 }] },
        ],
    },
    // 43 蜂群术士 [源码] `Entomancer`，**第 2 幕精英**（2026-08-22 首遇并通关）
    //
    // A0 档：血量 `MinInitialHp == MaxInitialHp == 145`（进阶 155，**不是区间**）；
    // 蜜——蜂——！3 点 × 7 段（进阶 8 段）；矛击！18（进阶 20）。招式名取自游戏本地化表
    // （2026-09-14 之前是照 wiki 译的 蜂群 / 长矛 / 信息素喷吐）。
    //
    // **开局自带人体蜂房 1 层**（`AfterAddedToRoom`）：我每一段攻击命中它，
    // 抽牌堆就多层数那么多张晕眩。规则在 `POWERS`，2026-09-14 建的。
    //
    // 喷射信息素（`SpitMove`）按蜂房层数分两支，内核拆成下标 2 / 3 两手（见 `M_ENTOMANCER`）：
    //   2 = 蜂房 < 3：蜂房 +1、力量 +1
    //   3 = 蜂房 ≥ 3：力量 +2
    // 两支的数都是 [源码] 写死的 `1m` / `2m`，**不吃进阶**。所以晕眩是 1 -> 2 -> 3 张/段，
    // 第三次喷射起每次 +2 力量。2026-09-14 之前内核恒走 +2 那一支（高估敌人伤害；
    // 那条注释记过「首遇那一场第一次喷吐是力量 +1」）。
    EnemyDef {
        name: "蜂群术士", max_hp: 145, start_status: &[(St::PersonalHive, 1)], loop_from: 0,
        machine: Some(&M_ENTOMANCER),
        moves: &[
            EnemyMove { name: "蜜——蜂——！", intent: "Attack", ops: &[EOp::Attack { base: 3, hits: 7 }] },
            EnemyMove { name: "矛击！", intent: "Attack", ops: &[EOp::Attack { base: 18, hits: 1 }] },
            EnemyMove { name: "喷射信息素", intent: "Buff",
                ops: &[EOp::SelfStatus { st: St::PersonalHive, amt: 1 },
                       EOp::SelfStatus { st: St::Strength, amt: 1 }] },
            EnemyMove { name: "喷射信息素", intent: "Buff",
                ops: &[EOp::SelfStatus { st: St::Strength, amt: 2 }] },
        ],
    },
    // 44 感染棱柱 [源码] `InfestedPrism`，**第 2 幕精英**（2026-08-22 首遇并通关）
    //
    // A0 档：血量 `MinInitialHp == MaxInitialHp == 161`（进阶 171，**不是区间**）。
    // 戳刺 15（进阶 17）· 辐射 11 伤害 + 11 格挡（进阶 13/13）·
    // 旋风 5×3（进阶 6×3，段数固定 3）· 脉动 8 伤害 + 20 格挡 + 活力火花 +2
    //（进阶伤害 10 / 格挡 22 / 火花 3）。
    //
    // **纯定环**（四手各自 `FollowUpState` 写死），所以用 `loop_from = 0` 的
    // 固定循环就是对的结构，不需要机器。实战四个回合逐条对上：
    // `Attack:15` → `Attack:11, Defend:` → `Attack:5×3` → `Attack:8, Buff:, Defend:`。
    //
    // **开局自带活力火花 2**（`AfterAddedToRoom`）：它给我牌组里每张技能牌挂污染，
    // 而污染让我挨的每一次攻击 +1。（这里原来写着「两个 status 内核都只映射不建模」，
    // 2026-09-14 核对时已经过期了。）今天的样子：活力火花**降维建了** ——
    // 打出技能牌时直接给我上污染（`POWERS` 里 `St::VitalSpark` 那条）；污染的伤害
    // **只在预测路径上加**（`damage::apply_modifiers` 那个加法项），注入路径照打观测标签，
    // 因为标签本来就含污染 —— 理由写在 `St::Tainted` 的注释里。
    // **读这只怪的求解结果时仍然要记得**：单回合的 `Threat` 是同步那一刻冻住的，
    // 线里每多打一张技能牌实际来袭就多一截，求解器会高估技能牌。
    EnemyDef {
        name: "感染棱柱", max_hp: 161, start_status: &[(St::VitalSpark, 2)], loop_from: 0,
        machine: None,
        moves: &[
            EnemyMove { name: "戳刺", intent: "Attack", ops: &[EOp::Attack { base: 15, hits: 1 }] },
            EnemyMove { name: "辐射", intent: "Attack",
                ops: &[EOp::Attack { base: 11, hits: 1 }, EOp::Block(11)] },
            EnemyMove { name: "旋风", intent: "Attack", ops: &[EOp::Attack { base: 5, hits: 3 }] },
            // 意图签名是 `Attack, Buff, Defend`（实测），所以给自己加 status 的 op
            // 要排在加格挡前面 —— 签名是按 ops 顺序生成副作用意图的
            EnemyMove { name: "脉动", intent: "Attack",
                ops: &[EOp::Attack { base: 8, hits: 1 },
                       EOp::SelfStatus { st: St::VitalSpark, amt: 2 },
                       EOp::Block(20)] },
        ],
    },
    // 45 [源码+实测] 结实的卵 `ToughEgg`（直飞产卵虫产的，带爪牙标记）
    //
    // A0：血量 14-18（进阶 15-19），啃咬 4（进阶 `DeadlyEnemies` 5）。
    // 实测三只 15/16/18，表里取上界 18（不高估玩家）。
    // 开局自带 `HatchPower 1`，每个回合末掉 1 层，掉到 0 就孵化。
    //
    // **孵化那一手内核只建了"这一手是什么"，没建"它变成了什么"**：
    // [源码] `HatchMove` 走 `CreatureCmd.SetMaxAndCurrentHp(19-22)` —— 同一只怪
    // 原地换名字（结实的卵 -> 幼虫）并把血量重掷成满血。内核没有
    // "敌人原地变形"这种 op，加它要动 `EOp`。
    // **默认对拍路径上无所谓**（名字和血量都是观测量，`sync` 每帧照抄），
    // 只有预测路径会停在卵的血量上 —— 记在这里，别当成建完了。
    EnemyDef {
        name: "结实的卵", max_hp: 18, start_status: &[(St::Hatch, 1)], loop_from: 1,
        machine: Some(&M_TOUGH_EGG),
        moves: &[
            EnemyMove { name: "孵化", intent: "Summon", ops: &[EOp::Nothing] },
            EnemyMove { name: "啃咬", intent: "Attack", ops: &[EOp::Attack { base: 4, hits: 1 }] },
        ],
    },
    // 46 [源码+实测] 幼虫 —— **和结实的卵是同一只怪孵化后的形态**
    //（[源码] `ToughEgg.Title` 在 `_hatched` 之后换成 `HATCHLING.name`）。
    // 内核靠**名字**连接观测和内容表，所以它必须是独立的一条。
    // A0 血量 19-22（进阶 20-23），实测 20，取上界 22。孵化后固定啃咬。
    EnemyDef {
        name: "幼虫", max_hp: 22, start_status: &[], loop_from: 0, machine: None,
        moves: &[EnemyMove { name: "啃咬", intent: "Attack", ops: &[EOp::Attack { base: 4, hits: 1 }] }],
    },
    // 47 [源码+实测] 无厌沙虫 `TheInsatiable`，**第 2 幕 Boss**
    //（2026-08-22 首遇并通关，9 个回合 41 帧全录）
    //
    // A0：血量 `MinInitialHp == MaxInitialHp == 321`（进阶 341，不是区间）；
    // 鞭挞 8×2（进阶 9×2）· 猛扑撕咬 28（进阶 31）· 垂涎 自身力量 +2（进阶 3）。
    //
    // **这只 Boss 的核心不是血量，是液化地面挂的沙坑：一条即死倒计时。**
    // 2026-09-05 建全了：层数 + 每个敌人回合开始减 1 + 归零即死，
    // 规则在 `POWERS` 的 `St::Sandpit` 那一条。在此之前内核只建了层数，
    // 于是求解器眼里这场仗只有血量 —— 而实际决定胜负的是回合数。
    //
    // [源码] 那 6 张狂乱逃离是 **3 张进抽牌堆、3 张进弃牌堆**
    //（`i < 3 ? PileType.Draw : PileType.Discard`，位置都随机）。
    // 原来全塞弃牌堆（"保守的那边"），**那条推理是错的**：进抽牌堆的那 3 张
    // 正是第 2 回合就能打出去买时间的那几张，晚一点出现不是"保守"，
    // 是把唯一的活路藏起来。实录第 2 回合手上就有一张（两份语料都有）。
    EnemyDef {
        name: "无厌沙虫", max_hp: 321, start_status: &[], loop_from: 1,
        machine: Some(&M_INSATIABLE),
        moves: &[
            // **沙坑挂在它自己身上，不是挂在我身上**：[源码]
            // `PowerCmd.Apply(..., sandpitPower, base.Creature, 4m, base.Creature, null)`
            // —— owner 是这只怪，只有 `sandpitPower.Target` 指向我。
            // 实录也是这么显示的（boss 身上 `SANDPIT_POWER=4`）。
            // 一开始建成给玩家上 status，意图签名多出一个 `Debuff`，
            // `--predict-enemy` 当场报「对不齐」—— 那条判据值回票价的一次。
            EnemyMove { name: "液化地面", intent: "Buff",
                ops: &[EOp::SelfStatus { st: St::Sandpit, amt: 4 },
                       EOp::AddCardToDraw { card: card::FRANTIC_ESCAPE, count: 3 },
                       EOp::AddCardToDiscard { card: card::FRANTIC_ESCAPE, count: 3 }] },
            EnemyMove { name: "鞭挞", intent: "Attack", ops: &[EOp::Attack { base: 8, hits: 2 }] },
            EnemyMove { name: "猛扑撕咬", intent: "Attack", ops: &[EOp::Attack { base: 28, hits: 1 }] },
            EnemyMove { name: "垂涎", intent: "Buff",
                ops: &[EOp::SelfStatus { st: St::Strength, amt: 2 }] },
            EnemyMove { name: "鞭挞2", intent: "Attack", ops: &[EOp::Attack { base: 8, hits: 2 }] },
        ],
    },
    // 48 [源码+实测] 虔诚雕刻师 `DevotedSculptor`（第 3 幕杂兵）
    //
    // A0：血量 162（进阶 ToughEnemies 172，不是区间）。
    // 禁忌咒语 `Buff` 给自己 9 仪式（`RITUAL_POWER`）；
    // 狂暴 12 伤害（进阶 DeadlyEnemies 15）。
    // 之后固定重复狂暴（`moveState2.FollowUpState = moveState2`）。
    //
    // 每回合仪式在回合末转化成 9 点力量，攻击力每回合 +9（12 -> 21 -> 30 -> 39...）。
    EnemyDef {
        name: "虔诚雕刻师", max_hp: 162, start_status: &[], loop_from: 1, machine: None,
        moves: &[
            EnemyMove { name: "禁忌咒语", intent: "Buff",
                ops: &[EOp::SelfStatus { st: St::Ritual, amt: 9 }] },
            EnemyMove { name: "狂暴", intent: "Attack",
                ops: &[EOp::Attack { base: 12, hits: 1 }] },
        ],
    },
    // 49 [源码] 永世沙漏 `Aeonglass`（**第 3 幕 Boss**）
    //
    // **名字 2026-08-22 从「永恒沙漏」改成「永世沙漏」** —— 游戏报的是后者。
    // 名字就是连接键，写错等于内核完全认不出这只 Boss：`solve --live` 当场报
    // 「1 只里认出 0 只 / 跨回合推演不可用」。和外骨骼虫那次同一类错，
    // 只是这次错在**真敌人自己的名字拼错**，不是假人占了真名字。
    //
    // A0：血量 512（进阶 ToughEnemies 535）。
    // 开局自带人工制品 3（`Artifact 3`），并给玩家挂枯萎之影（每出 6 张牌塞 1 张枯萎到手牌）。
    //
    // 严格 3 回合定环（Ebb -> EyeLasers -> IncreasingIntensity -> Ebb）：
    // 1. 退潮：26 伤害 + 33 格挡（进阶 32 / 33）
    // 2. 眼部激光：11×2 伤害（进阶 12×2）
    // 3. 剧烈增强：升级所有枯萎牌（伤害+3）、往玩家弃牌堆塞 1 张枯萎（进阶 2）、自身力量 +3（每次递增 1）
    EnemyDef {
        // [源码] `Aeonglass.AfterAddedToRoom`：`ArtifactPower(3)` 给自己，
        // 外加一个 `WitheringPresencePower(6)` —— 那个的 `target` 是玩家
        // （凋萎塞进**我的**手牌），但 `PowerCmd.Apply` 的 target 参数是
        // `base.Creature`，**power 本身挂在 Boss 身上**。
        // [实测] 两条永世沙漏语料的第 0 帧都是 `WITHERING_PRESENCE_POWER: 6`
        // 记在敌人那一栏，玩家身上没有。
        name: "永世沙漏", max_hp: 512,
        start_status: &[(St::Artifact, 3), (St::WitheringPresence, 6)],
        loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "退潮", intent: "Attack",
                ops: &[EOp::Attack { base: 26, hits: 1 }, EOp::Block(33)] },
            EnemyMove { name: "眼部激光", intent: "Attack",
                ops: &[EOp::Attack { base: 11, hits: 2 }] },
            // [源码] `IncreasingIntensityMove` 做三件事，**顺序就是下面这个顺序**：
            //   1. 把我拥有的**每一张**凋萎 `FakeUpgrade()`（各 +3 伤害）
            //   2. `WitherUpgradeCount++`，再往我的**弃牌堆**塞
            //      `WitherAmount`(A0 = 1) 张凋萎 —— 新的那张会被
            //      `AfterCardGeneratedForCombat` 拉到同一个层数
            //   3. 自己 +`IncreasingIntensityBaseStrength`(A0 = 3) + `AdditionalStrength` 力量
            //
            // 1 和 2 都建了（2026-08-25）。**内核这两条 op 的顺序和源码相反，
            // 是故意的**：源码「先升级已有的、再生成一张已经匹配好层数的」，
            // 内核没有那个 `WitherUpgradeCount` 计数器（它不是观测量），
            // 靠的是 `step::spawn_card` 从场上同名牌抄层数。于是要
            // **先生成、再把包括新的那张在内的全部一起 +3**，两条路才等价。
            //
            // 反过来写（先升级再生成）在**场上一张凋萎都没有**时会错：源码那边
            // 新生成的那张仍然拿到 `count+1` 层，而内核抄不到任何东西、给 0 层。
            // 两种写法在「场上已有凋萎」时给出相同的数 —— 实录里的两次剧烈增强
            // 恰好都是这种情况，**所以对拍分不开它们**，判据只有源码。
            // （这一条是 2026-08-25 故意改错验成色时抓到的：把顺序对调，
            // 五项对拍和当时那个测试全都不红。）
            //
            // **第 3 件只建了 base，没建 `AdditionalStrength` 的递增**：
            // 真实是 +3 / +4 / +5 …（每用一次多 1），内核每次都是 +3。
            // 欠的是一个"这一招用过几次"的计数器，而那**不是观测量** ——
            // `sync` 每帧从观测重建敌人 status，携带不过来。
            // **偏的方向：长仗里低估这只 Boss 的力量，也就是乐观。**
            EnemyMove { name: "剧烈增强", intent: "StatusCard",
                ops: &[
                    EOp::AddCardToDiscard { card: card::WITHER, count: 1 },
                    EOp::UpgradeAllCopies { card: card::WITHER, by: 3 },
                    EOp::SelfStatus { st: St::Strength, amt: 3 },
                ] },
        ],
    },
    // 50 [源码+实测] 活体盾 `LivingShield`（第 3 幕杂兵）
    //
    // A0：血量 55（进阶 ToughEnemies 65）。
    // 开局自带护壁 25（`RAMPART_POWER`，玩家回合开始时给场上所有高塔炮手 25 格挡）。
    // 只要有队友在场就一直盾牌猛击 6；独活时切换到猛砸 16（进阶 18）且每次 +3 力量无限循环。
    EnemyDef {
        name: "活体盾", max_hp: 55, start_status: &[(St::Rampart, 25)], loop_from: 0,
        machine: Some(&M_LIVING_SHIELD),
        moves: &[
            EnemyMove { name: "盾牌猛击", intent: "Attack",
                ops: &[EOp::Attack { base: 6, hits: 1 }] },
            EnemyMove { name: "猛砸", intent: "Attack",
                ops: &[EOp::Attack { base: 16, hits: 1 }, EOp::SelfStatus { st: St::Strength, amt: 3 }] },
        ],
    },
    // 51 [源码+实测] 高塔炮手 `TurretOperator`（第 3 幕杂兵）
    //
    // A0：血量 41（进阶 ToughEnemies 51）。
    // 严格 3 回合定环（Unload -> Unload -> Reload）：
    // 1. 倾泻火力：3×5 伤害（进阶 4×5）
    // 2. 倾泻火力2：3×5 伤害（进阶 4×5）
    // 3. 装填：Buff 获得 1 力量
    EnemyDef {
        name: "高塔炮手", max_hp: 41, start_status: &[], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "倾泻火力", intent: "Attack",
                ops: &[EOp::Attack { base: 3, hits: 5 }] },
            EnemyMove { name: "倾泻火力2", intent: "Attack",
                ops: &[EOp::Attack { base: 3, hits: 5 }] },
            EnemyMove { name: "装填", intent: "Buff",
                ops: &[EOp::SelfStatus { st: St::Strength, amt: 1 }] },
        ],
    },
    // 52 [源码+实测] 猫头鹰法官 `OwlMagistrate`（第 3 幕杂兵/中型怪）
    //
    // A0：血量 231（进阶 ToughEnemies 247）。
    // 严格 4 回合定环（Scrutiny -> PeckAssault -> JudicialFlight -> Verdict）：
    // 1. 审视：单体攻击 16（进阶 17）
    // 2. 啄击突袭：多段攻击 4×6
    // 3. 司法飞行：Buff，获得 1 层翱翔（`SoarPower`，受到有源攻击伤害减少 50%）
    // 4. 判决：单体攻击 33（进阶 36）+ 给玩家 4 层易伤，并移除翱翔
    EnemyDef {
        name: "猫头鹰法官", max_hp: 231, start_status: &[], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "审视", intent: "Attack",
                ops: &[EOp::Attack { base: 16, hits: 1 }] },
            EnemyMove { name: "啄击突袭", intent: "Attack",
                ops: &[EOp::Attack { base: 4, hits: 6 }] },
            EnemyMove { name: "司法飞行", intent: "Buff",
                ops: &[EOp::SelfStatus { st: St::Soar, amt: 1 }] },
            EnemyMove { name: "判决", intent: "Attack",
                ops: &[
                    EOp::Attack { base: 33, hits: 1 },
                    EOp::PlayerStatus { st: St::Vulnerable, amt: 4 },
                ] },
        ],
    },
    // ---- 53-57 组装师和它的四种造物（[源码] + 2026-08-22 第3幕第45层实录）----
    //
    // 53 [源码+实测] 组装师。`MinInitialHp == MaxInitialHp == 150`（A0），血量不随机。
    // 三手：
    //   0 制造        召唤 1 防御型 + 1 攻击型，自己不出手（意图 Summon）
    //   1 制造打击    18 点 + 召唤 1 攻击型（意图 Attack + Summon 两个）
    //   2 分解        11 点（只有"造不动了"的时候才出）
    // 实测：第1/2回合意图 Summon、第3回合场上 4 只时意图 Attack:11 —— 见 M_FABRICATOR。
    //
    // **召唤池只能挑一只是个已知近似。** [源码] 攻击型 {Zapbot, Stabbot}、
    // 防御型 {Guardbot, Noisebot}，且 `_lastSpawned` 那只本次排除（两只的池子里
    // 等于**必定和上次不同**）。内核的 `EOp::Summon` 只能写死一个 def，
    // 表达不了"两选一 + 排除上次"，所以各取实测见过的那只。
    // 对拍不受影响（敌人由观测同步），受影响的是 rollout 和 L2 对"召唤出什么"的预期。
    EnemyDef {
        name: "组装师", max_hp: 150, start_status: &[], loop_from: 0,
        machine: Some(&M_FABRICATOR),
        moves: &[
            EnemyMove { name: "制造", intent: "Summon", ops: &[
                EOp::Summon { def: enemy::NOISEBOT, hp: 23 },
                EOp::Summon { def: enemy::ZAPBOT, hp: 23 },
            ] },
            EnemyMove { name: "制造打击", intent: "Attack", ops: &[
                EOp::Attack { base: 18, hits: 1 },
                EOp::Summon { def: enemy::ZAPBOT, hp: 23 },
            ] },
            EnemyMove { name: "分解", intent: "Attack", ops: &[EOp::Attack { base: 11, hits: 1 }] },
        ],
    },
    // 54 [源码+实测] 噪音机器人。18-23 血（实测见过 22/23）。只有一手，永远重复。
    //
    // **两张眩晕去两个不同的牌堆**：[源码] 一张 `PileType.Discard`、
    // 一张 `PileType.Draw` + `CardPilePosition.Random`。写成两张都进弃牌堆是错的
    // —— 进抽牌堆的那张这一局就可能抽到，影响的是当前回合。
    EnemyDef {
        name: "噪音机器人", max_hp: 23, start_status: &[(St::Minion, 1)], loop_from: 0,
        machine: None,
        moves: &[EnemyMove { name: "噪音", intent: "StatusCard", ops: &[
            EOp::AddCardToDiscard { card: card::DAZED, count: 1 },
            EOp::AddCardToDraw { card: card::DAZED, count: 1 },
        ] }],
    },
    // 55 [源码+实测] 电击机器人。18-23 血（实测见过 20/21）。电击基础 14。
    //
    // **开局自带高压 2**（[源码] `AfterAddedToRoom` 里 `Apply<HighVoltagePower>(2)`）：
    // 每个敌人回合结束给自己 +2 力量，所以它的伤害一路涨。
    // [实测] 出场后下一回合意图 `Attack:16` = 14 + 力量 2，对上。
    EnemyDef {
        name: "电击机器人", max_hp: 23,
        start_status: &[(St::Minion, 1), (St::HighVoltage, 2)], loop_from: 0,
        machine: None,
        moves: &[EnemyMove { name: "电击", intent: "Attack", ops: &[EOp::Attack { base: 14, hits: 1 }] }],
    },
    // 56 [源码] 刺击机器人。18-23 血。11 点 + 1 层脆弱。**这一局没见过**，
    // 进表是因为它和电击机器人在同一个攻击型池子里，50/50。
    EnemyDef {
        name: "刺击机器人", max_hp: 23, start_status: &[(St::Minion, 1)], loop_from: 0,
        machine: None,
        moves: &[EnemyMove { name: "刺击", intent: "Attack", ops: &[
            EOp::Attack { base: 11, hits: 1 },
            EOp::PlayerStatus { st: St::Frail, amt: 1 },
        ] }],
    },
    // 57 [源码] 护卫机器人。16-20 血。**给场上每只组装师 15 点格挡，不是给自己** ——
    // 谓词换成"非爪牙"的理由见 `EOp::BlockNonMinions`。**这一局没见过**。
    EnemyDef {
        name: "护卫机器人", max_hp: 20, start_status: &[(St::Minion, 1)], loop_from: 0,
        machine: None,
        moves: &[EnemyMove { name: "护卫", intent: "Defend", ops: &[EOp::BlockNonMinions(15)] }],
    },
    // 58 [源码+实测] 多尼斯异鸟（第 1 幕精英）。
    //
    // [源码] `Byrdonis`：HP 81-84（`ToughEnemies` 档 90），出场
    // `PowerCmd.Apply<TerritorialPower>(..., 1)`；状态机只有两手、**互为后继**
    // （`SwoopMove.FollowUpState = PeckMove`，反过来也是），起始手是俯冲 ——
    // 也就是一个长度 2 的纯循环，用不着 `Machine`（那是给分支状态机准备的）。
    // 俯冲 17（`DeadlyEnemies` 档 19）、啄击 3×3（`DeadlyEnemies` 档 4×3）。
    //
    // [实测] 2026-08-27 第9层 A1：82 血（落在 81-84 内）；
    // 三帧意图 17 -> 4×3 -> 19，正好是 `17+力量0` / `3×3+力量1` / `17+力量2`，
    // 同时把顺序（俯冲起手、交替）和领地意识的 +1/回合一起钉死。
    EnemyDef {
        name: "多尼斯异鸟", max_hp: 82, start_status: &[(St::Territorial, 1)],
        loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "俯冲", intent: "Attack", ops: &[EOp::Attack { base: 17, hits: 1 }] },
            EnemyMove { name: "啄击", intent: "Attack", ops: &[EOp::Attack { base: 3, hits: 3 }] },
        ],
    },
    // 59 [源码+实测] 猎人杀手（第 2 幕）。121 血（`ToughEnemies` 档 126）。
    //
    // [实测] 2026-08-27 第22层：121 血；四手依次是
    // Debuff -> 7×3 -> 5×3 -> 17。中间那个 5×3 是穿刺**吃了我的虚弱**
    // （7 × 3/4 = 5.25 -> 5），不是另一手 —— 意图标签含防御方乘区这条又印证一次。
    //
    // 嫩化黏液给我挂**娇弱**：本回合每打一张牌 −1 力量 −1 敏捷，回合末还回来。
    // 规则在 POWERS 的两条（`CardPlayed` / `TurnEnd`），见 `St::Tender`。
    EnemyDef {
        name: "猎人杀手", max_hp: 121, start_status: &[], loop_from: 0,
        machine: Some(&M_HUNTER_KILLER),
        moves: &[
            EnemyMove { name: "嫩化黏液", intent: "Debuff",
                ops: &[EOp::PlayerStatus { st: St::Tender, amt: 1 }] },
            EnemyMove { name: "撕咬", intent: "Attack", ops: &[EOp::Attack { base: 17, hits: 1 }] },
            EnemyMove { name: "穿刺", intent: "Attack", ops: &[EOp::Attack { base: 7, hits: 3 }] },
        ],
    },
    // 60 [源码+实测] 残杀千足虫的一节（第 2 幕精英，三节共用一个 `EnemyDef`）。
    // 40-46 血（`ToughEnemies` 档 46-52），[实测] 三节 42 / 40 / 44。
    // 出场自带 `ReattachPower 25`。
    //
    // **接续 2026-09-14 建了**（死后第二个敌人回合回 25 血，规则见 `POWERS` 的 `St::Reattach`）。
    // 在那之前内核低估这一场。[实测] 2026-08-27 那一局我靠"三节要死就同一回合死"
    // 绕过了它；`act2_f28_decimillipede` 那一局没绕开，两节各复活了一次 —— 那两次就是这条规则的证据，
    // 也说明**窗口其实是两个我方回合**，不是同一回合。
    EnemyDef {
        name: "残杀千足虫", max_hp: 42, start_status: &[(St::Reattach, 25)], loop_from: 0,
        machine: Some(&M_DECIMILLIPEDE),
        moves: &[
            EnemyMove { name: "扭动", intent: "Attack", ops: &[EOp::Attack { base: 5, hits: 2 }] },
            EnemyMove { name: "壮硕", intent: "Attack",
                ops: &[EOp::Attack { base: 6, hits: 1 },
                       EOp::SelfStatus { st: St::Strength, amt: 2 }] },
            EnemyMove { name: "缠绕", intent: "Attack",
                ops: &[EOp::Attack { base: 8, hits: 1 },
                       EOp::PlayerStatus { st: St::Weak, amt: 1 }] },
            // 3 = 重接（[源码] `REATTACH_MOVE`，`HealIntent`）。回血那一下在 `St::ReattachDue`
            // 的回合开始规则里已经做完了，这一手本身什么都不做 —— 和幻象的「复苏」同构。
            EnemyMove { name: "重接", intent: "Heal", ops: &[EOp::Nothing] },
        ],
    },
    // 61 [源码+实测] 青蛙骑士 `FrogKnight`（第 3 幕杂兵，遭遇 `FrogKnightNormal`）
    //
    // A1 [实测] 血量 191（`ToughEnemies` 档 199 —— `MinInitialHp == MaxInitialHp`，
    // 不是区间）。**开局自带覆甲 15**（`AfterAddedToRoom` 直接
    // `PowerCmd.Apply<PlatingPower>`，`ToughEnemies` 档 19）。
    //
    // 伤害都是 `DeadlyEnemies` 分档，A1 取低档：除恶 21（高档 23）、
    // 舌鞭 13（14）、甲虫冲锋 35（40）。
    //
    // 三件事让它比数字上难对付得多，**内核以前一件都看不见**：
    //   1. **覆甲每回合补满**：我的每个回合都要先啃 15/15/14/13… 的墙，
    //      实际有效血量是 191 + 一堵每回合重建的墙。
    //   2. **为了女王每 3 回合 +5 力量**，且**永久** —— 伤害 13/21 会滚成
    //      18/26、23/31、28/36…，仗拖长了是致命的。
    //   3. **甲虫冲锋 35，一场只冲一次**，触发条件是血量掉到一半以下。
    //      2026-08-30 实战我用吹哨把「为了女王」那一手直接抹掉
    //      （`SetMoveImmediate` 是**换掉**当前 MoveState，不是推迟），
    //      于是它整场没拿到那 +5，也没来得及走到冲锋。
    //
    // [实测] 2026-08-30 第 3 幕第 43 层，`traces/act3_f43_frog_knight.json`：
    // 起手舌鞭 13 + 脆弱 2、第二手除恶 21、第三手 Buff，与状态机逐帧对上；
    // 覆甲层数 15/15/14/13 与格挡 15/15/14/13 同值。
    EnemyDef {
        name: "青蛙骑士", max_hp: 191, start_status: &[(St::PlatedArmor, 15)], loop_from: 0,
        machine: Some(&M_FROG_KNIGHT),
        moves: &[
            // 脆弱是 `PowerCmd.Apply<FrailPower>` 跟在攻击后面，
            // 所以意图签名是 `Attack, Debuff` —— `intent` 只写主意图。
            EnemyMove { name: "舌鞭", intent: "Attack",
                ops: &[EOp::Attack { base: 13, hits: 1 },
                       EOp::PlayerStatus { st: St::Frail, amt: 2 }] },
            EnemyMove { name: "除恶", intent: "Attack",
                ops: &[EOp::Attack { base: 21, hits: 1 }] },
            EnemyMove { name: "为了女王", intent: "Buff",
                ops: &[EOp::SelfStatus { st: St::Strength, amt: 5 }] },
            EnemyMove { name: "甲虫冲锋", intent: "Attack",
                ops: &[EOp::Attack { base: 35, hits: 1 }] },
        ],
    },
    // 62 [源码+实测] 巨斧机器人 `Axebot`（第 3 幕杂兵，遭遇 `AxebotsNormal`，单只）
    //
    // A1 血量 70-78 **每一具重掷**（`ToughEnemies` 档 76-86），
    // [实测] 三具 74 / 71 / 77。内核取中点 74。
    // 伤害都是 `DeadlyEnemies` 分档，A1 取低档：
    // 一二连击 9×2（高档 10×2）、锤击上勾拳 12（14）、启动格挡 10（15）。
    //
    // **这只怪的全部难度在「库存」上，见 `St::Stock`**：死了在同一格换上一具
    // 全新的、库存 −1，所以一场要打三具 ≈ 222 血；而启动那一手给
    // `3 × (2 − 库存)` 力量 ⇒ 第二具 +3、第三具 +6，**越死越强**。
    // 溢出伤害全废（新的一具是满血），所以"算准最后一刀"在这只身上特别值钱。
    //
    // [实测] 2026-08-30 第 3 幕第 45 层，`traces/act3_f45_axebot.json`：
    // 原装那只起手就是锤击上勾拳（12 + 虚弱2 + 脆弱2，意图 `Attack:12, Debuff:`），
    // 之后一二连击 9×2；两具重生的都从启动起手（意图 `Defend:, Buff:`，
    // 10 格挡 + 力量 3 / 6），与状态机逐帧对上。
    EnemyDef {
        name: "巨斧机器人", max_hp: 74, start_status: &[(St::Stock, 2)], loop_from: 0,
        machine: Some(&M_AXEBOT),
        moves: &[
            // 0 号是启动：`summon_one` 把 `enemy_move` 置 0，重生的那些正好从这里起手。
            // 力量 = 6 − 3×库存，两条 op 合起来表达 [源码] 的 `3 * (2 - StockAmount)`。
            EnemyMove { name: "启动", intent: "Defend",
                ops: &[EOp::Block(10),
                       EOp::SelfStatus { st: St::Strength, amt: 6 },
                       EOp::SelfStatusPerStack { st: St::Strength, amt: -3, per: St::Stock }] },
            // 虚弱 2 + 脆弱 2 一起给，而意图只显示一个 `Debuff:`（同类型只显示一个）
            EnemyMove { name: "锤击上勾拳", intent: "Attack",
                ops: &[EOp::Attack { base: 12, hits: 1 },
                       EOp::PlayerStatus { st: St::Weak, amt: 2 },
                       EOp::PlayerStatus { st: St::Frail, amt: 2 }] },
            EnemyMove { name: "一二连击", intent: "Attack",
                ops: &[EOp::Attack { base: 9, hits: 2 }] },
        ],
    },
    // 63 [源码+实测] 灵魂枢纽 `SoulNexus`（第 3 幕精英，遭遇 `SoulNexusElite`）
    //
    // A1 血量 234（`ToughEnemies` 档 254，`MinInitialHp == MaxInitialHp`）。
    // 伤害都是 `DeadlyEnemies` 分档，A1 取低档：
    // 灵魂灼烧 29（高档 31）、大漩涡 6×4（7×4）、汲取生命 18（19）+ 易伤 2 + 虚弱 2。
    //
    // 起手固定灵魂灼烧 29，之后在三手里等权随机（不能连出同一手）。
    //
    // [实测] 2026-08-30 第 3 幕第 46 层，`traces/act3_f46_soul_nexus.json`：
    // 起手灵魂灼烧 29、第二手汲取生命 18（挂易伤2+虚弱2）、
    // 第三手大漩涡（易伤下显示 9×4）、第四手汲取生命（易伤下显示 27），
    // 与状态机逐帧对上。
    EnemyDef {
        name: "灵魂枢纽", max_hp: 234, start_status: &[], loop_from: 0,
        machine: Some(&M_SOUL_NEXUS),
        moves: &[
            EnemyMove { name: "灵魂灼烧", intent: "Attack",
                ops: &[EOp::Attack { base: 29, hits: 1 }] },
            EnemyMove { name: "大漩涡", intent: "Attack",
                ops: &[EOp::Attack { base: 6, hits: 4 }] },
            EnemyMove { name: "汲取生命", intent: "Attack",
                ops: &[EOp::Attack { base: 18, hits: 1 },
                       EOp::PlayerStatus { st: St::Vulnerable, amt: 2 },
                       EOp::PlayerStatus { st: St::Weak, amt: 2 }] },
        ],
    },
    // 64 [源码] 实验体 `TestSubject`（第 3 幕 Boss，遭遇 `TestSubjectBoss`）
    //
    // [实测 A1/A3] 三阶段 100 / 200 / 300，总计 600 HP。
    // [源码] ToughEnemies 档 111 / 212 / 313（复活血量的进阶映射尚未接入）。
    //
    // 伤害与效果（A1 档）：
    //   Phase 1: 啃咬 20、头槌猛击 14 + 易伤 1（两者交替）
    //   Phase 2: 连环爪击 10×N（基础 3 段，每次出招段数 +1，持续自循环）
    //   Phase 3: 撕裂 10×3 -> 猛扑 45 -> 灼热咆哮（塞 3 灼伤 + 2 力量）-> 撕裂
    //
    // Powers:
    //   开局自带激怒 2（仅第一阶段，每打 1 张技能 +2 力量）、适生力 1（死后复活进下一阶段）。
    //   阶段 2 获得剧痛刺击 1（未格挡伤害塞伤口）。
    //   阶段 3 获得复仇宿敌 1（每隔一回合获得 1 层无实体）。
    EnemyDef {
        name: "实验体", max_hp: TEST_SUBJECT_FORM_HP[0], start_status: &[(St::Rage, 2), (St::Adaptable, 1)], loop_from: 0,
        machine: Some(&M_TEST_SUBJECT),
        moves: &[
            // 0: 复苏（阶段转换过渡）
            EnemyMove { name: "复苏", intent: "Defend", ops: &[EOp::Nothing] },
            // 1: 啃咬 20
            EnemyMove { name: "啃咬", intent: "Attack",
                ops: &[EOp::Attack { base: 20, hits: 1 }] },
            // 2: 头槌猛击 14 + 易伤 1
            EnemyMove { name: "头槌猛击", intent: "Attack",
                ops: &[EOp::Attack { base: 14, hits: 1 },
                       EOp::PlayerStatus { st: St::Vulnerable, amt: 1 }] },
            // 3: 连环爪击 10×3 (基础 3 段)
            // 段数 = 3 + 已增长；`ClawGrowth` 的 +1 在攻击**之后**，顺序照抄源码
            EnemyMove { name: "连环爪击", intent: "Attack",
                ops: &[EOp::AttackPlusStackHits { base: 10, hits: 3, per: St::ClawGrowth },
                       EOp::SelfStatus { st: St::ClawGrowth, amt: 1 }] },
            // 4: 撕裂 10×3
            EnemyMove { name: "撕裂", intent: "Attack",
                ops: &[EOp::Attack { base: 10, hits: 3 }] },
            // 5: 猛扑 45
            EnemyMove { name: "猛扑", intent: "Attack",
                ops: &[EOp::Attack { base: 45, hits: 1 }] },
            // 6: 灼热咆哮 (塞 3 灼伤 + 2 力量)
            EnemyMove { name: "灼热咆哮", intent: "StatusCard",
                ops: &[EOp::AddCardToDiscard { card: card::BURN, count: 3 },
                       EOp::SelfStatus { st: St::Strength, amt: 2 }] },
        ],
    },
    // 65 骇鳗 [源码] `TerrorEel`，**第 1 幕精英**（2026-08-31 首遇，第 8 层）
    //
    // 血量 140（`ToughEnemies` 150）。伤害取非 `DeadlyEnemies` 档：
    // 撞击 16（进阶 18）、乱舞 3x3（进阶 4x3）。尖叫阈值 70（进阶 75）。
    // 实测那一帧：140/140、意图「攻势 16」、`尖叫 (70)` —— 三个都对上了。
    //
    // **两处要紧的**：
    // * 乱舞打完给**自己** 6 活力（[源码] `PowerCmd.Apply<VigorPower>(.., 6m, ..)`），
    //   所以紧接着那一手撞击是 16+6=22。敌人侧的活力 2026-08-31 才补上
    //   （`step.rs` 的 `EOp::Attack`），在那之前这一条会让内核**低估**它。
    // * 恐惧给玩家 **99 层易伤** —— 不是笔误，[源码] 就是 `99m`。
    //   等于这一场后面全程易伤（每回合掉 1 层，掉不完）。
    EnemyDef {
        name: "骇鳗", max_hp: 140, start_status: &[(St::Shriek, 70)], loop_from: 0,
        machine: Some(&M_TERROR_EEL),
        moves: &[
            // 0: 撞击
            EnemyMove { name: "撞击", intent: "Attack", ops: &[EOp::Attack { base: 16, hits: 1 }] },
            // 1: 乱舞 —— 3x3 之后给自己 6 活力（下一手撞击因此是 22）
            EnemyMove { name: "乱舞", intent: "Attack",
                ops: &[EOp::Attack { base: 3, hits: 3 },
                       EOp::SelfStatus { st: St::Vigor, amt: 6 }] },
            // 2: 击晕 —— 只能被尖叫的 `OwnerForceMove(2)` 打进来
            EnemyMove { name: "击晕", intent: "Stun", ops: &[EOp::Nothing] },
            // 3: 恐惧 —— 99 层易伤
            EnemyMove { name: "恐惧", intent: "Debuff",
                ops: &[EOp::PlayerStatus { st: St::Vulnerable, amt: 99 }] },
        ],
    },
    // 66 [源码+实测] 活雾 `LivingFog`（第 1 幕杂兵）
    //
    // A0/A2：血量 80（进阶 ToughEnemies 82）。
    // 出招：
    // 0: 高阶瓦斯：8 点伤害 + 侵蚀
    // 1: 膨胀：召唤 1 个瓦斯炸弹 + 5 点伤害
    // 2: 超级瓦斯冲击：8 点伤害
    // 循环：0 -> 1 -> 2 -> 1 -> 2 ...
    EnemyDef {
        name: "活雾", max_hp: 80, start_status: &[], loop_from: 0,
        machine: Some(&M_LIVING_FOG),
        moves: &[
            EnemyMove { name: "高阶瓦斯", intent: "Attack", ops: &[EOp::Attack { base: 8, hits: 1 }, EOp::PlayerStatus { st: St::Smoggy, amt: 1 }] },
            EnemyMove { name: "膨胀", intent: "Attack", ops: &[EOp::Summon { def: enemy::GAS_BOMB, hp: 7 }, EOp::Attack { base: 5, hits: 1 }] },
            EnemyMove { name: "超级瓦斯冲击", intent: "Attack", ops: &[EOp::Attack { base: 8, hits: 1 }] },
        ],
    },
    // 67 [源码] 气态炸弹 `GasBomb`（第 1 幕活雾召唤物）
    //
    // A0/A2：血量 7（进阶 ToughEnemies 8）。
    // 0: 自爆：8 点伤害（DeathBlow），然后自杀。
    EnemyDef {
        name: "气态炸弹", max_hp: 7, start_status: &[(St::Minion, 1)], loop_from: 0,
        machine: None,
        moves: &[
            EnemyMove { name: "自爆", intent: "DeathBlow", ops: &[EOp::Attack { base: 8, hits: 1 }] },
        ],
    },
    // 68 [源码] 花园幽灵鳗 `PhantasmalGardener`（第 1 幕精英）
    //
    // A0/A2：血量 27（区间 26-31）。开局自带胆小 6。
    // 出招：
    // 0: 啃咬：5 点伤害
    // 1: 鞭击：7 点伤害
    // 2: 乱舞：1 点伤害 × 3 段
    // 3: 巨大化：自身力量 +3（A0 为 +2）
    // 循环：0 -> 1 -> 2 -> 3 -> 0 ...
    EnemyDef {
        name: "花园幽灵鳗", max_hp: 27, start_status: &[(St::Skittish, 6)], loop_from: 0,
        machine: Some(&M_PHANTASMAL_GARDENER),
        moves: &[
            EnemyMove { name: "啃咬", intent: "Attack", ops: &[EOp::Attack { base: 5, hits: 1 }] },
            EnemyMove { name: "鞭击", intent: "Attack", ops: &[EOp::Attack { base: 7, hits: 1 }] },
            EnemyMove { name: "乱舞", intent: "Attack", ops: &[EOp::Attack { base: 1, hits: 3 }] },
            EnemyMove { name: "巨大化", intent: "Buff", ops: &[EOp::SelfStatus { st: St::Strength, amt: 3 }] },
        ],
    },
    // 69 [源码+实测] 瀑布巨兽 `WaterfallGiant`（第 1 幕 Boss）
    //
    // A0/A2：血量 240（ToughEnemies 为 250）。6 / 7 两手的名字取自游戏本地化表（2026-09-15），
    // 前六手沿用旧名（游戏里叫 增压 / 践踏 / 撞击 / 虹吸 / 压力炮 / 增压）。
    // 出招：
    // 0: 加压：蒸汽喷发 +15（A9 +20）
    // 1: 重踏：15 点伤害 + 玩家虚弱 1 + 蒸汽喷发 +3
    // 2: 撞击：10 点伤害 + 蒸汽喷发 +3
    // 3: 虹吸：自身回血 10（A8 15）+ 蒸汽喷发 +3
    // 4: 高压枪：20 点伤害（A9 23）+ 蒸汽喷发 +3；打完之后这门炮**永久** +5
    // 5: 升压：13 点伤害 + 蒸汽喷发 +3
    // 6: 即将爆发（死后）：不打人，把蒸汽喷发层数记成爆炸伤害、移除蒸汽喷发
    // 7: 爆炸：打记下的那么多，然后自杀
    // 循环：0 -> 1 -> 2 -> 3 -> 4 -> 5 -> 1 ...；死亡规则把它强制进 6（`POWERS` 的蒸汽喷发）
    //
    // [实测] `act1_f17_waterfall_giant`（A2）：第 4 -> 5 回合 174 -> 184（虹吸 +10）· 高压枪第 5 回合 20、
    // 第 10 回合 25 · 砍死之后 999999999 血、第 11 回合意图 Stun、第 12 回合 DeathBlow 42（= 15 + 9 × 3）。
    // 2026-09-15 之前内核这三处都没建（回血 / +5 / 死后），方向全是乐观。
    EnemyDef {
        name: "瀑布巨兽", max_hp: 240, start_status: &[], loop_from: 0,
        machine: Some(&M_WATERFALL_GIANT),
        moves: &[
            EnemyMove { name: "加压", intent: "Buff", ops: &[EOp::SelfStatus { st: St::SteamEruption, amt: 15 }] },
            EnemyMove { name: "重踏", intent: "Attack", ops: &[EOp::Attack { base: 15, hits: 1 }, EOp::PlayerStatus { st: St::Weak, amt: 1 }, EOp::SelfStatus { st: St::SteamEruption, amt: 3 }] },
            EnemyMove { name: "撞击", intent: "Attack", ops: &[EOp::Attack { base: 10, hits: 1 }, EOp::SelfStatus { st: St::SteamEruption, amt: 3 }] },
            EnemyMove { name: "虹吸", intent: "Heal", ops: &[EOp::Heal(10), EOp::SelfStatus { st: St::SteamEruption, amt: 3 }] },
            // 攻击在前、+5 在后（[源码] `CurrentPressureGunDamage += PressureGunIncrease` 写在 `DamageCmd` 之后）
            EnemyMove { name: "高压枪", intent: "Attack", ops: &[EOp::AttackPlusSelfStatus { base: 20, hits: 1, per: St::PressureGunGrowth }, EOp::SelfStatus { st: St::PressureGunGrowth, amt: 5 }, EOp::SelfStatus { st: St::SteamEruption, amt: 3 }] },
            EnemyMove { name: "升压", intent: "Attack", ops: &[EOp::Attack { base: 13, hits: 1 }, EOp::SelfStatus { st: St::SteamEruption, amt: 3 }] },
            EnemyMove { name: "即将爆发", intent: "Stun", ops: &[EOp::SelfStatusPerStack { st: St::EruptionDamage, amt: 1, per: St::SteamEruption }, EOp::ClearSelfStatus(St::SteamEruption)] },
            // 爆炸是**攻击**（`DamageCmd.Attack(..).FromMonster(this)`）：它身上的虚弱、我的格挡都照常算
            EnemyMove { name: "爆炸", intent: "DeathBlow", ops: &[EOp::AttackPlusSelfStatus { base: 0, hits: 1, per: St::EruptionDamage }, EOp::KillSelf] },
        ],
    },
    // 70 [源码+实测] 地道虫 `Tunneler`（第 2 幕杂兵）
    //
    // A0/A2：血量 87（进阶 ToughEnemies 92）。
    // 出招：
    // 0: 咬击：13 点伤害
    // 1: 钻地：Buff+Defend，获得 32 点格挡并附加 1 层钻地（BurrowedPower）
    // 2: 地底突袭：23 点伤害（持续自循环）
    // 3: 眩晕：被破盾打进眩晕后空过一回合，随后回到 0: 咬击
    EnemyDef {
        name: "地道虫", max_hp: 87, start_status: &[], loop_from: 0,
        machine: Some(&M_TUNNELER),
        moves: &[
            EnemyMove { name: "咬击", intent: "Attack", ops: &[EOp::Attack { base: 13, hits: 1 }] },
            EnemyMove { name: "钻地", intent: "Buff", ops: &[EOp::Block(32), EOp::SelfStatus { st: St::Burrowed, amt: 1 }] },
            EnemyMove { name: "地底突袭", intent: "Attack", ops: &[EOp::Attack { base: 23, hits: 1 }] },
            EnemyMove { name: "眩晕", intent: "Stun", ops: &[EOp::Nothing] },
        ],
    },
    // 71 [源码+实测] 胧光怪 `TheObscura`（第 2 幕杂兵，遭遇 `TheObscuraNormal`）
    //
    // 血量 123（进阶 `ToughEnemies` 129）。伤害取非 `DeadlyEnemies` 档：
    // 穿刺凝视 10（进阶 11）· 硬化打击 6 伤害 + 6 格挡（进阶 7/7）。
    //
    // **它的要害不是自己的血量，是那只幻象**：开局召唤一只寄生惧魔，
    // 而幻象带 `St::Illusion` —— **杀不掉**，砍死了下一手就回满 21 血。
    // 唯一的解法是**无视幻象，直接打死本体**：本体一死，爪牙跟着消失
    // （`step::no_master_left`，实录最后一帧正是这样收的场）。
    //
    // [实测] 2026-09-05 第2幕第30层：
    //   123/123 · 第 1 回合意图 `Summon:` · 第 2 回合 `Attack:6, Defend:` ——
    //   两个意图签名逐字对上（硬化打击那一手的 `Defend:` 由 `EOp::Block` 生成）。
    // **哀嚎和穿刺凝视这一场没出过**，那两条是 [源码]，还没被数据碰过。
    EnemyDef {
        name: "胧光怪", max_hp: 123, start_status: &[], loop_from: 1,
        machine: Some(&M_OBSCURA),
        moves: &[
            EnemyMove { name: "幻象", intent: "Summon",
                ops: &[EOp::Summon { def: enemy::PARAFRIGHT, hp: 21 }] },
            EnemyMove { name: "穿刺凝视", intent: "Attack",
                ops: &[EOp::Attack { base: 10, hits: 1 }] },
            // 哀嚎（[源码] 的状态叫 `SAIL_MOVE`，方法叫 `WailMove`，
            // wiki 抄的是前者的拼写 "Sail" 并且**没有效果文本**）：
            // 给自己这一侧每一只加 3 力量，**含它自己**（`GetTeammatesOf` 含自己，
            // 见 `EOp::TeamStatus`）。带着幻象时这一手值 3 力量 × 2 只。
            EnemyMove { name: "哀嚎", intent: "Buff",
                ops: &[EOp::TeamStatus { st: St::Strength, amt: 3 }] },
            // 意图签名是 `Attack:6, Defend:`（[实测]）—— 副意图由 `EOp::Block` 生成
            EnemyMove { name: "硬化打击", intent: "Attack",
                ops: &[EOp::Attack { base: 6, hits: 1 }, EOp::Block(6)] },
        ],
    },
    // 72 [源码+实测] 寄生惧魔 `Parafright`（胧光怪召唤出来的幻象）
    //
    // 血量 **21，不随进阶变**（`MinInitialHp => 21` 写死，没有 `AscensionHelper`）。
    // 猛撞 16（进阶 `DeadlyEnemies` 17），[源码] 出招表只有这一手且自循环。
    //
    // 开局自带两个 status，都来自 [源码] `IllusionPower.AfterApplied`：
    // 幻象（复活）+ 爪牙（`PowerCmd.Apply<MinionPower>`）。
    // [实测] 那一帧观测里正是 `ILLUSION_POWER: 1, MINION_POWER: 1` 两个都在。
    //
    // **`hp` 参数和 `max_hp` 必须一致**：`EOp::Summon` 传的 21 是当前血量，
    // 而复活要回到 `max_hp` —— 两个数不一样的话，复活一次它就变强/变弱了。
    EnemyDef {
        name: "寄生惧魔", max_hp: 21,
        start_status: &[(St::Illusion, 1), (St::Minion, 1)],
        loop_from: 1,
        machine: Some(&M_ILLUSION_MINION),
        moves: &[
            // 0 = 复苏。**不在出招循环里**，只有 `TOp::OwnerForceMove(0)` 进得来。
            // 意图 `Heal`（[源码] `new HealIntent()`；那个字符串在瀑布巨兽身上实测过）。
            EnemyMove { name: "复苏", intent: "Heal", ops: &[EOp::Nothing] },
            EnemyMove { name: "猛撞", intent: "Attack",
                ops: &[EOp::Attack { base: 16, hits: 1 }] },
        ],
    },
    // ---- 2026-09-09 第 1 幕新一局那一批（八只 + 一个 Boss）。
    // 全部照 [源码] 抄：血量取 `MinInitialHp` 那一档（A0），
    // 招式数值取 `AscensionHelper.GetValueIfAscension(...)` 的**第二个参数**
    // （那才是低进阶值 —— 读反了整批会系统性偏高）。
    // 意图签名逐只和第 0 帧的观测对过（`bin/synth_audit` 的开局第一手那一栏）。----
    //
    // 淤泥旋螺：喷油（8 + 虚弱1）· 猛砸（11）· 狂怒（6 + 自身力量3）
    EnemyDef {
        name: "淤泥旋螺", max_hp: 38, start_status: &[], loop_from: 0,
        machine: Some(&M_SLUDGE_SPINNER),
        moves: &[
            EnemyMove { name: "喷油", intent: "Attack",
                ops: &[EOp::Attack { base: 8, hits: 1 },
                       EOp::PlayerStatus { st: St::Weak, amt: 1 }] },
            EnemyMove { name: "猛砸", intent: "Attack", ops: &[EOp::Attack { base: 11, hits: 1 }] },
            EnemyMove { name: "狂怒", intent: "Attack",
                ops: &[EOp::Attack { base: 6, hits: 1 },
                       EOp::SelfStatus { st: St::Strength, amt: 3 }] },
        ],
    },
    // 噬尸蛞蝓：开局自带**贪食**（同伴死 -> +4 力量并被击晕一回合）。
    // 三手定环，起手按站位 —— 内核按集合建。
    EnemyDef {
        name: "噬尸蛞蝓", max_hp: 25, start_status: &[(St::Ravenous, 4)], loop_from: 0,
        machine: Some(&M_CORPSE_SLUG),
        moves: &[
            EnemyMove { name: "鞭击", intent: "Attack", ops: &[EOp::Attack { base: 3, hits: 2 }] },
            EnemyMove { name: "吞噬", intent: "Attack", ops: &[EOp::Attack { base: 8, hits: 1 }] },
            EnemyMove { name: "黏液", intent: "Debuff",
                ops: &[EOp::PlayerStatus { st: St::Frail, amt: 2 }] },
        ],
    },
    // 海洋混混：海踢（11）-> 旋踢（2×4）-> 泡泡嗝（格挡7 + 自身力量1）定环。
    EnemyDef {
        name: "海洋混混", max_hp: 46, start_status: &[], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "海踢", intent: "Attack", ops: &[EOp::Attack { base: 11, hits: 1 }] },
            EnemyMove { name: "旋踢", intent: "Attack", ops: &[EOp::Attack { base: 2, hits: 4 }] },
            EnemyMove { name: "泡泡嗝", intent: "Buff",
                ops: &[EOp::Block(7), EOp::SelfStatus { st: St::Strength, amt: 1 }] },
        ],
    },
    // 两只邪教徒 [源码] 逐字同构（吟唱一次 -> 之后永远黑暗打击），
    // 只差**仪式的层数**和打击的基础值。仪式本身内核已经有规则
    // （`Hook::EnemyTurnEnd` 每回合给自己加力量）。
    EnemyDef {
        name: "钙化邪教徒", max_hp: 41, start_status: &[], loop_from: 1, machine: None,
        moves: &[
            EnemyMove { name: "吟唱", intent: "Buff",
                ops: &[EOp::SelfStatus { st: St::Ritual, amt: 2 }] },
            EnemyMove { name: "黑暗打击", intent: "Attack", ops: &[EOp::Attack { base: 9, hits: 1 }] },
        ],
    },
    // 潮湿邪教徒：仪式 5、打击只有 1 —— [实测] 观测到的 1/6/11/16 正是
    // 「1 + 每回合 +5 力量」那条等差数列，两个数因此互相印证。
    EnemyDef {
        name: "潮湿邪教徒", max_hp: 51, start_status: &[], loop_from: 1, machine: None,
        moves: &[
            EnemyMove { name: "吟唱", intent: "Buff",
                ops: &[EOp::SelfStatus { st: St::Ritual, amt: 5 }] },
            EnemyMove { name: "黑暗打击", intent: "Attack", ops: &[EOp::Attack { base: 1, hits: 1 }] },
        ],
    },
    // 下水道蚌：开局 8 层覆甲（[源码] `AfterAddedToRoom`），
    // **起手是喷射不是加压**（`initialState` 是第二个 MoveState）。
    // [实测] 第 0 帧格挡 8 —— 敌人持有的覆甲开局给一次格挡那条规则接上了。
    EnemyDef {
        name: "下水道蚌", max_hp: 56, start_status: &[(St::PlatedArmor, 8)], loop_from: 0,
        machine: None,
        moves: &[
            EnemyMove { name: "喷射", intent: "Attack", ops: &[EOp::Attack { base: 10, hits: 1 }] },
            EnemyMove { name: "加压", intent: "Buff",
                ops: &[EOp::SelfStatus { st: St::Strength, amt: 4 }] },
        ],
    },
    // 双尾鼠：会**召唤同类**（一只一辈子一次）。见 `M_TWO_TAILED_RAT` 的两条欠账。
    EnemyDef {
        name: "双尾鼠", max_hp: 18, start_status: &[], loop_from: 0,
        machine: Some(&M_TWO_TAILED_RAT),
        moves: &[
            EnemyMove { name: "抓挠", intent: "Attack", ops: &[EOp::Attack { base: 8, hits: 1 }] },
            EnemyMove { name: "病咬", intent: "Attack", ops: &[EOp::Attack { base: 6, hits: 1 }] },
            EnemyMove { name: "尖啸", intent: "Debuff",
                ops: &[EOp::PlayerStatus { st: St::Frail, amt: 1 }] },
            EnemyMove { name: "呼叫增援", intent: "Summon",
                ops: &[EOp::Summon { def: enemy::TWO_TAILED_RAT, hp: 18 }] },
        ],
    },
    // 拳击构装体：开局 1 层人工制品。蓄力（格挡10）-> 快拳（5×2 + 脆弱1）-> 重拳（14）。
    EnemyDef {
        name: "拳击构装体", max_hp: 55, start_status: &[(St::Artifact, 1)], loop_from: 0,
        machine: Some(&M_PUNCH_CONSTRUCT),
        moves: &[
            EnemyMove { name: "蓄力", intent: "Defend", ops: &[EOp::Block(10)] },
            EnemyMove { name: "快拳", intent: "Attack",
                ops: &[EOp::Attack { base: 5, hits: 2 },
                       EOp::PlayerStatus { st: St::Frail, amt: 1 }] },
            EnemyMove { name: "重拳", intent: "Attack", ops: &[EOp::Attack { base: 14, hits: 1 }] },
        ],
    },
    // 第 1 幕 Boss 灵魂异鱼：五手定环，中间那一手给自己**无实体 2**。
    // 呼唤/凝视塞的是**应急按钮**（回合末在手上 -6 血，见 `HAND_END`）。
    // [实测] 观测到的 24 / 10 是 16 / 7 吃了我身上的易伤（×1.5），不是别的招。
    EnemyDef {
        name: "灵魂异鱼", max_hp: 211, start_status: &[], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "呼唤", intent: "StatusCard",
                ops: &[EOp::AddCardToDraw { card: card::BECKON, count: 1 },
                       EOp::AddCardToDiscard { card: card::BECKON, count: 1 }] },
            EnemyMove { name: "排气", intent: "Attack", ops: &[EOp::Attack { base: 16, hits: 1 }] },
            EnemyMove { name: "凝视", intent: "Attack",
                ops: &[EOp::Attack { base: 7, hits: 1 },
                       EOp::AddCardToDiscard { card: card::BECKON, count: 1 }] },
            EnemyMove { name: "淡出", intent: "Buff",
                ops: &[EOp::SelfStatus { st: St::Intangible, amt: 2 }] },
            EnemyMove { name: "尖啸", intent: "Attack",
                ops: &[EOp::Attack { base: 13, hits: 1 },
                       EOp::PlayerStatus { st: St::Vulnerable, amt: 3 }] },
        ],
    },
    // ---- 2026-09-13 第 3 幕补敌人（批 1 + 批 2）--------------------------------
    // **全部 [源码]，没有一条实录**：数值取低进阶档（`GetValueIfAscension(level, 高, 低)`
    // 的第二个数），第一次实战打出时对拍才判它。名字（怪物名和招式名）取自游戏本地化表；
    // 图鉴里隐藏的招式没有译名，照抄 [源码] 的 id。
    //
    // 82 [源码] 史莱姆狂战士 `SlimedBerserker`（第 3 幕杂兵，`SlimedBerserkerNormal`）
    // 血量 261（`ToughEnemies` 281）—— 2026-08-14 旧 advisor 记过一次 261，对得上。
    // 四手定环，**起手就是 10 张黏液进弃牌堆**；连击 5×4、窒息 33 是 `DeadlyEnemies` 档。
    // 黏液 1 费能打、打了消耗（见可打出性那条测试）—— 10 张等于往牌组里掺一副垃圾。
    EnemyDef {
        name: "史莱姆狂战士", max_hp: 261, start_status: &[], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "喷吐脓水", intent: "StatusCard",
                ops: &[EOp::AddCardToDiscard { card: card::SLIMED, count: 10 }] },
            EnemyMove { name: "狂怒连击", intent: "Attack", ops: &[EOp::Attack { base: 4, hits: 4 }] },
            // `WeakPower` 的施加者是 null，对内核没有区别（没有"谁挂的虚弱"这回事）
            EnemyMove { name: "汲取之拥", intent: "Debuff",
                ops: &[EOp::PlayerStatus { st: St::Weak, amt: 3 },
                       EOp::SelfStatus { st: St::Strength, amt: 3 }] },
            EnemyMove { name: "SMOTHER", intent: "Attack", ops: &[EOp::Attack { base: 30, hits: 1 }] },
        ],
    },
    // 83 [源码] 机甲骑士 `MechaKnight`（第 3 幕精英，`MechaKnightElite`）
    // 血量 300（`ToughEnemies` 320）—— 2026-08-14 旧 advisor 记过 300。开局人工制品 3。
    // 冲锋 25 只出一次，之后 喷火器(4 张灼伤进**手牌**) -> 举起蓄力(15 格挡 + 5 力量) -> 重斩 35 定环。
    // **力量每三手 +5 永久叠**，重斩 35/40/45…；灼伤在手上回合末各烫 2 点（`HAND_END`），
    // 手满了溢出进弃牌堆（见 `EOp::AddCardToHand`）。伤害 `DeadlyEnemies` 档 30 / 40。
    EnemyDef {
        name: "机甲骑士", max_hp: 300, start_status: &[(St::Artifact, 3)], loop_from: 1, machine: None,
        moves: &[
            EnemyMove { name: "冲锋", intent: "Attack", ops: &[EOp::Attack { base: 25, hits: 1 }] },
            EnemyMove { name: "喷火器", intent: "StatusCard",
                ops: &[EOp::AddCardToHand { card: card::BURN, count: 4 }] },
            EnemyMove { name: "举起蓄力", intent: "Defend",
                ops: &[EOp::Block(15), EOp::SelfStatus { st: St::Strength, amt: 5 }] },
            EnemyMove { name: "重斩", intent: "Attack", ops: &[EOp::Attack { base: 35, hits: 1 }] },
        ],
    },
    // 84 [源码] 电球头 `GlobeHead`（第 3 幕杂兵，`GlobeHeadNormal`）
    // 血量 148（`ToughEnemies` 158）。开局**流电 6**：我每打出一张能力牌挨 6 点（见 `St::Galvanic`）。
    // 三手定环：电击掌 13 + 脆弱 2 -> 生成闪电 6×3 -> 电流爆发 16 + 自身力量 2 -> …
    // 伤害 `DeadlyEnemies` 档 14 / 7 / 17。
    EnemyDef {
        name: "电球头", max_hp: 148, start_status: &[(St::Galvanic, 6)], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "电击掌", intent: "Attack",
                ops: &[EOp::Attack { base: 13, hits: 1 },
                       EOp::PlayerStatus { st: St::Frail, amt: 2 }] },
            EnemyMove { name: "生成闪电", intent: "Attack", ops: &[EOp::Attack { base: 6, hits: 3 }] },
            EnemyMove { name: "GALVANIC_BURST", intent: "Attack",
                ops: &[EOp::Attack { base: 16, hits: 1 },
                       EOp::SelfStatus { st: St::Strength, amt: 2 }] },
        ],
    },
    // 85 [源码] 咬人卷轴 `ScrollOfBiting`（第 3 幕，`ScrollsOfBitingNormal` 4 卷 / `ScrollsOfBitingWeak` 3 卷）
    // 血量 30–37 **每卷重掷**（`ToughEnemies` 33–39），内核点值取 33。
    // 开局**纸伤难愈 2**：每被它打穿一段，我 −2 最大生命，而且带出这场仗（见 `St::PaperCuts`）。
    // 出招机器和「起手按槽位错开」那条近似见 `M_SCROLL_OF_BITING`。伤害 `DeadlyEnemies` 档 16 / 6×2。
    EnemyDef {
        name: "咬人卷轴", max_hp: 33, start_status: &[(St::PaperCuts, 2)], loop_from: 0,
        machine: Some(&M_SCROLL_OF_BITING),
        moves: &[
            EnemyMove { name: "大啃", intent: "Attack", ops: &[EOp::Attack { base: 14, hits: 1 }] },
            EnemyMove { name: "咀嚼", intent: "Attack", ops: &[EOp::Attack { base: 5, hits: 2 }] },
            EnemyMove { name: "更多牙齿", intent: "Buff",
                ops: &[EOp::SelfStatus { st: St::Strength, amt: 2 }] },
        ],
    },
    // 86 [源码] 失落之物 `TheLost`（第 3 幕杂兵，和遗忘之物同场 `TheLostAndForgottenNormal`）
    // 血量 93（`ToughEnemies` 99）。开局**抢夺力量**：两手定环
    //   致残雾霾（我 −2 力量、它 +2 力量）-> 眼部激光 4×2（`DeadlyEnemies` 5×2）-> …
    // **它死了才把偷走的力量还给我**（见 `St::PossessStrength` / `Amt::OwnerStacksOf`）。
    // 我带人工制品时那 −2 被挡掉、它照样 +2（[源码] 两句 `Apply` 互不相干）。
    EnemyDef {
        name: "失落之物", max_hp: 93, start_status: &[(St::PossessStrength, 1)], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "致残雾霾", intent: "Debuff",
                ops: &[EOp::PlayerStatus { st: St::Strength, amt: -2 },
                       EOp::SelfStatus { st: St::Strength, amt: 2 }] },
            EnemyMove { name: "眼部激光", intent: "Attack", ops: &[EOp::Attack { base: 4, hits: 2 }] },
        ],
    },
    // 87 [源码] 遗忘之物 `TheForgotten`（和失落之物同场）
    // 血量 106（`ToughEnemies` 111）。开局**抢夺速度**：两手定环
    //   瘴气（我 −2 敏捷 -> 它 8 格挡 -> 它 +2 敏捷）-> 恐惧 13 + 它自己的敏捷 -> …
    // **op 的顺序就是 [源码] 的顺序，别调**：格挡在加敏捷**之前**，所以格挡 8 / 10 / 12…
    // 而恐惧在加敏捷**之后**，15 / 17 / 19…（`DeadlyEnemies` 基础 15）。
    // 意图签名是 `Debuff, Defend, Buff` 和 `Attack`。
    EnemyDef {
        name: "遗忘之物", max_hp: 106, start_status: &[(St::PossessSpeed, 1)], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "瘴气", intent: "Debuff",
                ops: &[EOp::PlayerStatus { st: St::Dexterity, amt: -2 },
                       EOp::Block(8),
                       EOp::SelfStatus { st: St::Dexterity, amt: 2 }] },
            EnemyMove { name: "恐惧", intent: "Attack",
                ops: &[EOp::AttackPlusSelfStatus { base: 13, hits: 1, per: St::Dexterity }] },
        ],
    },
    // ---- 2026-09-14 第 3 幕骑士团（批 3）------------------------------------------
    // **全部 [源码]，没有一条实录。** 三只同场（`KnightsElite`，站位 first/second/third =
    // 连枷 / 幽灵 / 魔法）。2026-08-14 旧 advisor 记过的血量 101 / 93 / 82 对得上。
    // 名字（怪物和招式）取自游戏本地化表；图鉴里隐藏的招式照抄 [源码] id。
    //
    // 88 [源码] 连枷骑士 `FlailKnight`：血量 101（`ToughEnemies` 108）。
    // 起手撞击 15，之后三手随机：战争吟唱（+3 力量，不连出）/ 连枷 9×2 / 撞击 15（各最多连出两次）。
    // 伤害 `DeadlyEnemies` 档 10×2 / 17。**力量永久叠** —— 三只里越拖越疼的是它。
    EnemyDef {
        name: "连枷骑士", max_hp: 101, start_status: &[], loop_from: 0,
        machine: Some(&M_FLAIL_KNIGHT),
        moves: &[
            EnemyMove { name: "撞击", intent: "Attack", ops: &[EOp::Attack { base: 15, hits: 1 }] },
            EnemyMove { name: "连枷", intent: "Attack", ops: &[EOp::Attack { base: 9, hits: 2 }] },
            EnemyMove { name: "战争吟唱", intent: "Buff",
                ops: &[EOp::SelfStatus { st: St::Strength, amt: 3 }] },
        ],
    },
    // 89 [源码] 幽灵骑士 `SpectralKnight`：血量 93（`ToughEnemies` 97）。
    // 起手**恶咒**（我身上挂 `St::Hex`：从下个回合末起，没打出去的牌**全部消耗**），
    // 然后灵魂斩击 15，之后随机：灵魂斩击（最多连出两次）/ 灵魂火焰 3×3（不连出）。
    // 伤害 `DeadlyEnemies` 档 17 / 4×3。**它死了才解咒**（`St::HexCaster`）——
    // 这一只的威胁不在伤害上，在牌组被一回合一回合吃掉。
    EnemyDef {
        name: "幽灵骑士", max_hp: 93, start_status: &[], loop_from: 0,
        machine: Some(&M_SPECTRAL_KNIGHT),
        moves: &[
            EnemyMove { name: "恶咒", intent: "Debuff",
                ops: &[EOp::PlayerStatus { st: St::Hex, amt: 2 }] },
            EnemyMove { name: "灵魂斩击", intent: "Attack", ops: &[EOp::Attack { base: 15, hits: 1 }] },
            EnemyMove { name: "灵魂火焰", intent: "Attack", ops: &[EOp::Attack { base: 3, hits: 3 }] },
        ],
    },
    // 90 [源码] 魔法骑士 `MagiKnight`：血量 82（`ToughEnemies` 89）。
    // 定环：强力护盾（6 + 自己 5 格挡）-> 抑制 -> 撞击 10 -> PREP（5 格挡）-> 魔法炸弹 35 -> 撞击 -> …
    // **抑制**把我本场所有已升级的牌降级，它死了才升回来（`EOp::DowngradeUpgradedCards`）。
    // **魔法炸弹前一手总是 PREP**（加格挡）—— 看到它加格挡，下个敌人回合就是 35。
    // 伤害 `DeadlyEnemies` 档 7 / 11 / 40；两处格挡 `ToughEnemies` 档 9。
    EnemyDef {
        name: "魔法骑士", max_hp: 82, start_status: &[], loop_from: 2, machine: None,
        moves: &[
            EnemyMove { name: "强力护盾", intent: "Attack",
                ops: &[EOp::Attack { base: 6, hits: 1 }, EOp::Block(5)] },
            EnemyMove { name: "抑制", intent: "Debuff",
                ops: &[EOp::PlayerStatus { st: St::Dampen, amt: 1 }, EOp::DowngradeUpgradedCards] },
            EnemyMove { name: "撞击", intent: "Attack", ops: &[EOp::Attack { base: 10, hits: 1 }] },
            EnemyMove { name: "PREP", intent: "Defend", ops: &[EOp::Block(5)] },
            EnemyMove { name: "魔法炸弹", intent: "Attack", ops: &[EOp::Attack { base: 35, hits: 1 }] },
        ],
    },
    // ---- 2026-09-14 第 2 幕（批 4）----------------------------------------------------
    // 91 [源码] 熟睡甲虫 `SlumberingBeetle`（**全部 [源码]，没有实录**；名字取自游戏本地化表）
    //
    // 只在 `SlumberingBeetleNormal` 出现，和盛碗虫（石）+ 盛碗虫（丝）同场（站位 first / second / third）。
    // 血量 86（`ToughEnemies` 89，不是区间）。开局 **覆甲 15**（`ToughEnemies` 18 ——
    // 生成器认不出开局 status 里的数，和青蛙骑士同一个欠账，见 `data/ascension.json`）+ **熟睡 3**。
    //
    //   0 打鼾（`SleepIntent`）：什么都不做
    //   1 出击（`SingleAttackIntent` + `BuffIntent`）：16（`DeadlyEnemies` 18）+ 自身力量 2（写死）
    //   2 醒来（被打醒的那一手，`StunIntent`）：移除覆甲。[源码] 是 `CreatureCmd.Stun(WakeUpMove,
    //     "ROLL_OUT_MOVE")` 临时造的 `STUNNED` 态，不在 `GenerateMoveStateMachine` 里，
    //     只能被熟睡那条规则强制打进去（`TOp::OwnerForceMove(2)`）。意图字符串没实测过
    //
    // 两种醒法都**移除覆甲**（`WakeUpMove`），差在醒来的那个敌人回合打不打人：
    //   · 自己回合末熟睡减到 0：当场醒（覆甲在同一个回合末已经给过格挡），下一个敌人回合直接出击
    //   · 被打穿减到 0：被击晕，下一个敌人回合「醒来」（不打人），再下一个起出击
    // 醒了之后**永远出击**，力量每手 +2。**打法是先清两只盛碗虫、别碰它** —— 打穿它一下就少睡一回合。
    EnemyDef {
        name: "熟睡甲虫", max_hp: 86, start_status: &[(St::PlatedArmor, 15), (St::Slumber, 3)],
        loop_from: 0, machine: Some(&M_SLUMBERING_BEETLE),
        moves: &[
            EnemyMove { name: "打鼾", intent: "Sleep", ops: &[EOp::Nothing] },
            EnemyMove { name: "出击", intent: "Attack",
                ops: &[EOp::Attack { base: 16, hits: 1 }, EOp::SelfStatus { st: St::Strength, amt: 2 }] },
            EnemyMove { name: "醒来", intent: "Stun", ops: &[EOp::ClearSelfStatus(St::PlatedArmor)] },
        ],
    },
    // ---- 2026-09-17 第 1 幕（批 2）------------------------------------------------
    // 92 [源码] 乐加维林族母 `LagavulinMatriarch`（**全部 [源码]，没有实录**；
    // 名字和招式名取自游戏本地化表 `LAGAVULIN_MATRIARCH.*`）
    //
    // `LagavulinMatriarchBoss` 单怪，**第 1 幕暗港三个 Boss 之一** ⇒ 约占所有局的 1/6。
    // 血量 222（`ToughEnemies` 233，`Min == Max` 不是区间）。开局 **覆甲 12 + 沉睡 3**
    // （[源码] `AfterAddedToRoom -> Sleep()`；两个数都不吃进阶）。
    //
    //   0 沉睡（`SleepIntent`）：什么都不做
    //   1 斩击（`SingleAttackIntent`）：19（`DeadlyEnemies` 21）
    //   2 开膛破肚（`MultiAttackIntent`）：9×2（`DeadlyEnemies` 10×2）
    //   3 斩击2（`SingleAttackIntent` + `DefendIntent`）：12（`DeadlyEnemies` 14）
    //     + 格挡 12（`ToughEnemies` 14）。**本地化表里没有 SLASH2 这个 key**
    //     （图鉴只列 SLASH），名字照无厌沙虫「鞭挞2」的老办法取
    //   4 灵魂汲取（`DebuffIntent` + `BuffIntent`）：我 −2 力量 −2 敏捷、它 +2 力量（都不吃进阶）
    //   5 醒来（被打醒的那一手，`StunIntent`）：**什么都不做** —— [源码] `WakeUpMove` 只播动画，
    //     覆甲是沉睡自己摘的。和熟睡甲虫的「醒来」不一样，那一手真的清覆甲。意图字符串没实测过
    //
    // 两种醒法，差的是**那一回合的覆甲墙**和**要不要白打一手**：
    //   · 自己回合末沉睡减到 0：覆甲在同一个回合末的**更早一档**就被摘了 ⇒ 那一回合**没有墙**，
    //     下一个敌人回合直接斩击
    //   · 被打穿：覆甲当场没（我方回合里就没了），被击晕 ⇒ 下一个敌人回合「醒来」不打人，再之后斩击
    // 所以**打穿它是划算的**：省掉一堵墙，还白赚一个不挨打的回合。难点是开局那 12 点覆甲
    // 每个敌人回合末都补满（覆甲自己每个敌人回合开始 −1：12/11/10…）。
    EnemyDef {
        name: "乐加维林族母", max_hp: 222,
        start_status: &[(St::PlatedArmor, 12), (St::Asleep, 3)],
        loop_from: 0, machine: Some(&M_LAGAVULIN_MATRIARCH),
        moves: &[
            EnemyMove { name: "沉睡", intent: "Sleep", ops: &[EOp::Nothing] },
            EnemyMove { name: "斩击", intent: "Attack", ops: &[EOp::Attack { base: 19, hits: 1 }] },
            EnemyMove { name: "开膛破肚", intent: "Attack", ops: &[EOp::Attack { base: 9, hits: 2 }] },
            EnemyMove { name: "斩击2", intent: "Attack",
                ops: &[EOp::Attack { base: 12, hits: 1 }, EOp::Block(12)] },
            EnemyMove { name: "灵魂汲取", intent: "Debuff",
                ops: &[EOp::PlayerStatus { st: St::Strength, amt: -2 },
                       EOp::PlayerStatus { st: St::Dexterity, amt: -2 },
                       EOp::SelfStatus { st: St::Strength, amt: 2 }] },
            EnemyMove { name: "醒来", intent: "Stun", ops: &[EOp::Nothing] },
        ],
    },
    // ---- 2026-09-17 第 1 幕（批 3）------------------------------------------------
    // 93 [源码] 墨影幻灵 `Vantom`（**全部 [源码]，没有实录**；名字取自 `VANTOM.*`）
    //
    // `VantomBoss` 单怪，**第 1 幕密林三个 Boss 之一** ⇒ 约占所有局的 1/6。
    // 血量 173（`ToughEnemies` 183，`Min == Max`）。开局 **滑溜 8**（`ToughEnemies` 9 ——
    // 开局 status 里的数进阶生成器认不出，和青蛙骑士 / 熟睡甲虫同一个欠账，见 `data/ascension.json`）。
    //
    //   0 墨迹（`SingleAttackIntent`）：7（`DeadlyEnemies` 8）
    //   1 墨水长枪（`MultiAttackIntent`）：6×2（`DeadlyEnemies` 7×2）
    //   2 肢解（`SingleAttackIntent` + `StatusIntent(3)`）：26（`DeadlyEnemies` 30）+ 3 张伤口进**弃牌堆**
    //   3 准备（`BuffIntent`）：自身力量 +2（写死）
    // 四手定环，起点墨迹 —— 纯确定性，所以不用机器。
    //
    // **这场仗的节奏是滑溜，不是血量**：前 8 次打穿各只掉 1 血（`St::Slippery`），
    // 所以开局那 8 层是一堵 8 次命中的墙，多段牌把它拆得最快 —— 一段 26 点和一段 2 点
    // 付的价钱一样。清完之后才是 165 血的普通 Boss。
    EnemyDef {
        name: "墨影幻灵", max_hp: 173, start_status: &[(St::Slippery, 8)],
        loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "墨迹", intent: "Attack", ops: &[EOp::Attack { base: 7, hits: 1 }] },
            EnemyMove { name: "墨水长枪", intent: "Attack", ops: &[EOp::Attack { base: 6, hits: 2 }] },
            EnemyMove { name: "肢解", intent: "Attack",
                ops: &[EOp::Attack { base: 26, hits: 1 },
                       EOp::AddCardToDiscard { card: card::WOUND, count: 3 }] },
            EnemyMove { name: "准备", intent: "Buff",
                ops: &[EOp::SelfStatus { st: St::Strength, amt: 2 }] },
        ],
    },
    // 94 [源码] 墨宝 `Inklet`（**全部 [源码]，没有实录**；名字取自 `INKLET.*`）
    //
    // `InkletsNormal` **三只同场**，中间那只起手旋风（见 `M_INKLET`）。
    // 血量 11–17（`ToughEnemies` 12–18）—— **是区间**，这里的 `max_hp` 取中位 14，
    // 真实血量合成路径从 `asc::hp_range` 掷、对拍路径从观测灌。
    // 开局 **滑溜 1**：每只都要多挨一次命中才开始掉血，三只就是三次。
    //
    //   0 刺击（`SingleAttackIntent`）：3（`DeadlyEnemies` 4）
    //   1 旋风（`MultiAttackIntent`）：2×3（`DeadlyEnemies` 3×3）
    //   2 锐利凝视（`SingleAttackIntent`）：10（`DeadlyEnemies` 11）—— 图鉴里隐藏的那一手
    EnemyDef {
        name: "墨宝", max_hp: 14, start_status: &[(St::Slippery, 1)],
        loop_from: 0, machine: Some(&M_INKLET),
        moves: &[
            EnemyMove { name: "刺击", intent: "Attack", ops: &[EOp::Attack { base: 3, hits: 1 }] },
            EnemyMove { name: "旋风", intent: "Attack", ops: &[EOp::Attack { base: 2, hits: 3 }] },
            EnemyMove { name: "锐利凝视", intent: "Attack", ops: &[EOp::Attack { base: 10, hits: 1 }] },
        ],
    },
    // ---- 2026-09-19 第 1 幕（批 4）------------------------------------------------
    // 95 [源码] 鬼祟珊瑚群 `SkulkingColony`（**全部 [源码]，没有实录**；名字取自 `SKULKING_COLONY.*`）
    //
    // `SkulkingColonyElite` 单怪，**第 1 幕暗港精英**。血量 75（`ToughEnemies` 80，`Min == Max`）。
    // 开局 **硬化外壳 20**（`AfterAddedToRoom`，写死、不吃进阶）：
    // **每个回合最多掉 20 血，双方各自的回合各算一份**（见 `St::HardenedShell`）。
    // 余额挂 `start_status`（开局那一刻 = 上限），上限走 `ENEMY_PRIVATE_MARKERS`（两条路径都要）。
    //
    //   0 猛冲（`SingleAttackIntent`）：14（`DeadlyEnemies` 16）
    //   1 猛冲2：同上。[源码] `ZOOM_MOVE_2` 和 `ZOOM_MOVE` 调的是同一个 `ZoomMove`；
    //     **本地化表里没有 ZOOM_2 这个 key**，名字照「鞭挞2」「斩击2」的老办法取
    //   2 惯性（`SingleAttackIntent` + `BuffIntent`）：9（A9 11）+ 自身力量 2（A9 4）
    //   3 穿刺戳击（`MultiAttackIntent`）：7×2（A9 8×2）
    // 四手定环、起点猛冲 —— 纯确定性，不用机器（和墨影幻灵同一个处置）。
    //
    // **这场仗的节奏是回合数，不是血量**：75 血至少要 4 个我方回合（20+20+20+15），
    // 一个回合打出去的第 21 点起全是白打。**所以每回合打满 20 就该转去挡**。
    // 敌人回合里荆棘 / 火焰屏障反弹的伤害吃的是敌人回合那一份额度，**不占**我下一回合的 20。
    EnemyDef {
        name: "鬼祟珊瑚群", max_hp: 75, start_status: &[(St::HardenedShell, 20)],
        loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "猛冲", intent: "Attack", ops: &[EOp::Attack { base: 14, hits: 1 }] },
            EnemyMove { name: "猛冲2", intent: "Attack", ops: &[EOp::Attack { base: 14, hits: 1 }] },
            EnemyMove { name: "惯性", intent: "Attack",
                ops: &[EOp::Attack { base: 9, hits: 1 }, EOp::SelfStatus { st: St::Strength, amt: 2 }] },
            EnemyMove { name: "穿刺戳击", intent: "Attack", ops: &[EOp::Attack { base: 7, hits: 2 }] },
        ],
    },
    // ---- 2026-09-19 第 1 幕（批 5，暗港杂兵）----------------------------------------
    // **全部 [源码]，没有实录**；名字取自游戏本地化表（`*.name` / `*.moves.*.title`）。
    //
    // 96 [源码] 幽灵船 `HauntedShip`（`HauntedShipNormal` 单怪）。血量 63（`ToughEnemies` 67，`Min == Max`）。
    //   0 纠缠（`DebuffIntent` + `StatusIntent(5)`）：我虚弱 3 + **5 张晕眩进弃牌堆**（都不吃进阶）
    //   1 扫击（`SingleAttackIntent`）：13（A9 14）
    //   2 践踏（`MultiAttackIntent`）：4×3（A9 5×3）
    // 起点纠缠，之后扫击 / 践踏交替（[源码] `HAUNT -> SWIPE -> STOMP -> SWIPE`）⇒ `loop_from: 1`。
    EnemyDef {
        name: "幽灵船", max_hp: 63, start_status: &[], loop_from: 1, machine: None,
        moves: &[
            EnemyMove { name: "纠缠", intent: "Debuff",
                ops: &[EOp::PlayerStatus { st: St::Weak, amt: 3 },
                       EOp::AddCardToDiscard { card: card::DAZED, count: 5 }] },
            EnemyMove { name: "扫击", intent: "Attack", ops: &[EOp::Attack { base: 13, hits: 1 }] },
            EnemyMove { name: "践踏", intent: "Attack", ops: &[EOp::Attack { base: 4, hits: 3 }] },
        ],
    },
    // 97 [源码] 蟾蜍蝌蚪 `Toadpole`（`ToadpolesWeak` **两只同场**，第 1 幕暗港的弱遭遇）。
    // 血量 21–25（`ToughEnemies` 22–26）—— 区间，`max_hp` 取中位 23，合成路径从 `asc::hp_range` 掷。
    //   0 吐刺（`MultiAttackIntent`）：**先**给自己 −2 荆棘，再 3×3（A9 4×3）
    //   1 旋转（`SingleAttackIntent`）：7（A9 8）
    //   2 带刺（`BuffIntent`）：自身荆棘 +2（不吃进阶）
    // 环是 旋转 -> 带刺 -> 吐刺 -> 旋转，起点按站位（前面那只带刺、后面那只旋转），见 `M_TOADPOLE`。
    // 所以**带刺之后的那个我方回合打它要吃 2 点反弹**，吐刺出完刺就收回去了。
    // 荆棘归零时 [源码] 整个移除（`ThornsPower` 不许负数）；环里吐刺永远接在带刺后面，
    // 内核的 `SelfStatus` 不夹 0 在这里碰不到。
    EnemyDef {
        name: "蟾蜍蝌蚪", max_hp: 23, start_status: &[], loop_from: 0, machine: Some(&M_TOADPOLE),
        moves: &[
            EnemyMove { name: "吐刺", intent: "Attack",
                ops: &[EOp::SelfStatus { st: St::Thorns, amt: -2 }, EOp::Attack { base: 3, hits: 3 }] },
            EnemyMove { name: "旋转", intent: "Attack", ops: &[EOp::Attack { base: 7, hits: 1 }] },
            EnemyMove { name: "带刺", intent: "Buff", ops: &[EOp::SelfStatus { st: St::Thorns, amt: 2 }] },
        ],
    },
    // 98 [源码] 化石追踪者 `FossilStalker`（`FossilStalkerNormal` 单怪）。
    // 血量 51–53（`ToughEnemies` 54–56），`max_hp` 取中位 52。
    // 开局 **吮吸 3**（写死）：它的攻击每打穿一段 +3 力量 ⇒ **多段的甩动最要挡**。
    //   0 冲撞（`SingleAttackIntent` + `DebuffIntent`）：9（A9 11）+ 我脆弱 1
    //   1 缠上（`SingleAttackIntent`）：12（A9 14）—— 起手这一手
    //   2 甩动（`MultiAttackIntent`）：3×2（A9 4×2）
    // 之后每一手三选一等权、同一手最多连出两次，见 `M_FOSSIL_STALKER`。
    EnemyDef {
        name: "化石追踪者", max_hp: 52, start_status: &[(St::Suck, 3)],
        loop_from: 0, machine: Some(&M_FOSSIL_STALKER),
        moves: &[
            EnemyMove { name: "冲撞", intent: "Attack",
                ops: &[EOp::Attack { base: 9, hits: 1 }, EOp::PlayerStatus { st: St::Frail, amt: 1 }] },
            EnemyMove { name: "缠上", intent: "Attack", ops: &[EOp::Attack { base: 12, hits: 1 }] },
            EnemyMove { name: "甩动", intent: "Attack", ops: &[EOp::Attack { base: 3, hits: 2 }] },
        ],
    },
    // 99 [源码] 地精佣兵 `GremlinMerc`（`GremlinMercNormal`：开局只有它一只）。
    // 血量 47–49（`ToughEnemies` 51–53），`max_hp` 取中位 48。
    // 开局 **意外 1**（死时召唤卑鄙地精 + 胖地精，规则在 `POWERS`）+ **偷窃 20**（只偷金币，印记）。
    //
    // **它的三个伤害数挂的是 `ToughEnemies`（A8）不是 `DeadlyEnemies`（A9）**——
    // [源码] `GimmeDamage` / `DoubleSmashDamage` / `HeheDamage` 三个都写的
    // `GetValueIfAscension(AscensionLevel.ToughEnemies, …)`，照抄，`dump_ascension.py` 会按源码的门认。
    //   0 拿来（`MultiAttackIntent`）：7×2（A8 8×2）+ 偷金币
    //   1 双重猛击（`MultiAttackIntent` + `DebuffIntent`）：6×2（A8 7×2）+ 我虚弱 2
    //   2 嘿嘿（`SingleAttackIntent` + `BuffIntent`）：8（A8 9）+ 自身力量 2（写死）
    // 三手定环、起点拿来 —— 纯确定性，不用机器。
    EnemyDef {
        name: "地精佣兵", max_hp: 48, start_status: &[(St::Surprise, 1), (St::Thievery, 20)],
        loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "拿来", intent: "Attack", ops: &[EOp::Attack { base: 7, hits: 2 }] },
            EnemyMove { name: "双重猛击", intent: "Attack",
                ops: &[EOp::Attack { base: 6, hits: 2 }, EOp::PlayerStatus { st: St::Weak, amt: 2 }] },
            EnemyMove { name: "嘿嘿", intent: "Attack",
                ops: &[EOp::Attack { base: 8, hits: 1 }, EOp::SelfStatus { st: St::Strength, amt: 2 }] },
        ],
    },
    // 100 [源码] 卑鄙地精 `SneakyGremlin`（只由地精佣兵的意外召唤出来）。
    // 血量 10–14（`ToughEnemies` 11–15）；召唤时的血量写在意外那条规则里（中位 12），这里的 `max_hp` 同值。
    //   0 醒来（`StunIntent`）：什么都不做 —— 被召唤出来的第一个敌人回合
    //   1 冲撞（`SingleAttackIntent`）：9（A9 10），之后**一直**冲撞（`FollowUpState` 指向自己）
    EnemyDef {
        name: "卑鄙地精", max_hp: 12, start_status: &[], loop_from: 1, machine: None,
        moves: &[
            EnemyMove { name: "醒来", intent: "Stun", ops: &[EOp::Nothing] },
            EnemyMove { name: "冲撞", intent: "Attack", ops: &[EOp::Attack { base: 9, hits: 1 }] },
        ],
    },
    // 101 [源码] 胖地精 `FatGremlin`（只由地精佣兵的意外召唤出来）。
    // 血量 13–17（`ToughEnemies` 14–18）；召唤血量中位 15，同上。
    //   0 醒来（`StunIntent`）：什么都不做
    //   1 逃跑（`EscapeIntent`）：[源码] `CreatureCmd.Escape` —— **离场**，身上的盗窃（偷走的金币）跟着带走。
    //     **内核近似成原地不动**（`EOp::Nothing`，之后一直重复），和偷窃草蜢的逃跑同一个处置：
    //     内核没有「离场」这个动作，而用 `KillSelf` 会让它「死」一次，点燃地精之角那类死亡触发，是错的。
    //     **方向**：它不打人，只是要我多花 15 点伤害收掉它才算打完（悲观，只费回合不费血 ——
    //     除非卑鄙地精还活着，那时多拖的每个回合都多挨一下 9）。
    EnemyDef {
        name: "胖地精", max_hp: 15, start_status: &[], loop_from: 1, machine: None,
        moves: &[
            EnemyMove { name: "醒来", intent: "Stun", ops: &[EOp::Nothing] },
            EnemyMove { name: "逃跑", intent: "Escape", ops: &[EOp::Nothing] },
        ],
    },
    // ---- 2026-09-19 第 1 幕批 6（密林杂兵）。**全部 [源码]，一条实录都没有**；名字取自本地化表
    // （`VINE_SHAMBLER.*` / `ASSASSIN_RUBY_RAIDER.*` / `BRUTE_RUBY_RAIDER.*`，简中那一份）。
    //
    // 102 [源码] 藤蔓蹒跚者 `VineShambler`（`VineShamblerNormal` 单怪）。血量 61（A8 64，`Min == Max`）。
    // 三手定环，**起点是挥击**（`new MonsterMoveStateMachine(list, moveState2)`，不是列表里的第一个）：
    //   0 挥击（`MultiAttackIntent`）：6×2（A9 7×2）
    //   1 紧绕藤蔓（`SingleAttackIntent` + `CardDebuffIntent`）：8（A9 9），**打完**给我缠结 1（不吃进阶）
    //     —— 下一个我方回合攻击牌 +1 费，见 `St::Tangled`
    //   2 大啃（`SingleAttackIntent`）：16（A9 18）
    EnemyDef {
        name: "藤蔓蹒跚者", max_hp: 61, start_status: &[], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "挥击", intent: "Attack", ops: &[EOp::Attack { base: 6, hits: 2 }] },
            EnemyMove { name: "紧绕藤蔓", intent: "Attack",
                ops: &[EOp::Attack { base: 8, hits: 1 }, EOp::PlayerStatus { st: St::Tangled, amt: 1 }] },
            EnemyMove { name: "大啃", intent: "Attack", ops: &[EOp::Attack { base: 16, hits: 1 }] },
        ],
    },
    // 103 [源码] 劫掠者刺客 `AssassinRubyRaider`（`RubyRaidersNormal` 五选三）。
    // 血量 18–23（A8 19–24），`max_hp` 取偏低的中位 20（合成路径从 `asc::hp_range` 掷，这个数只是兜底）。
    //   0 致命射击（`SingleAttackIntent`）：10（A9 11），`FollowUpState` 指向自己 —— 一直这一手
    EnemyDef {
        name: "劫掠者刺客", max_hp: 20, start_status: &[], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "致命射击", intent: "Attack", ops: &[EOp::Attack { base: 10, hits: 1 }] },
        ],
    },
    // 104 [源码] 劫掠者暴徒 `BruteRubyRaider`（同上）。血量 30–33（A8 31–34），`max_hp` 取 31。
    //   0 殴打（`SingleAttackIntent`）：7（A9 8）—— 起手这一手
    //   1 怒吼（`BuffIntent`）：自身力量 +3（`_roarStrength` 常量，不吃进阶），不打人
    // 两手交替 ⇒ 殴打 7 / 10 / 13 …… 每两个回合涨 3。
    EnemyDef {
        name: "劫掠者暴徒", max_hp: 31, start_status: &[], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "殴打", intent: "Attack", ops: &[EOp::Attack { base: 7, hits: 1 }] },
            EnemyMove { name: "怒吼", intent: "Buff",
                ops: &[EOp::SelfStatus { st: St::Strength, amt: 3 }] },
        ],
    },
];

/// 实验体三个形态的最大生命（A1 非进阶档；`ToughEnemies` 是 111/212/313）。
/// **复活规则和 [`remaining_hp_including_revives`] 共用这三个数**，别在别处再抄。
pub const TEST_SUBJECT_FORM_HP: [i32; 3] = [100, 200, 300];

/// [源码] RespawnMove grants PainfulStabs in form 2, Nemesis in form 3.
/// These observed powers distinguish the two identical 10x3 intent labels.
pub fn move_matches_form(def: u16, mv: usize, e: &crate::state::Entity) -> bool {
    if def != enemy::TEST_SUBJECT_BOSS { return true; }
    if !e.alive() { return mv == 0; }
    if e.get(St::Nemesis) > 0 { return (4..=6).contains(&mv); }
    if e.get(St::PainfulStabs) > 0 { return mv == 3; }
    (1..=2).contains(&mv)
}

/// 区分形态 1 和形态 2 的门槛（形态 1 的最大生命 ≤ 这个数）。
/// 取在 100 和 200 中间，进阶档的 111/212 也照样分得开。
pub const TEST_SUBJECT_FORM1_MAX: i32 = 150;

/// 知识恶魔「知识的诅咒」的三次二选一（[源码] `KnowledgeDemon._curseOfKnowledgeSets` +
/// `_disintegrationDamageValues = {6, 7, 8}`，都不吃进阶）。
/// 第 k 次按 `State::curse_policy` 的第 k 位挑：1 = 瓦解，0 = 另一边。
pub static KNOWLEDGE_CURSES: [CurseSet; 3] = [
    CurseSet { disintegration: 6, other: St::MindRot, other_amt: 1, other_max_energy: 0 },
    CurseSet { disintegration: 7, other: St::Sloth, other_amt: 3, other_max_energy: 0 },
    CurseSet { disintegration: 8, other: St::WasteAway, other_amt: 1, other_max_energy: -1 },
];

/// `State::curse_policy` 的默认值：**第 2 次选瓦解，第 1、3 次选另一边**。
///
/// [玩家判定] 2026-09-14：「第一个都可以，第二一般选瓦解，第三个看情况」。
/// 所以只有第 2 位是判定，第 1、3 位是 `[判断]`（第 1 位随手取了心灵腐化；第 3 位要看局面，
/// 那正是 `advise` 单场问法把 8 种选法全摆出来的原因）。
/// **它只决定没人问的时候怎么打**：整幕链、跨回合推演、`fight_eval` 这类验收台。
pub const DEFAULT_CURSE_POLICY: u8 = 0b010;

/// 知识的诅咒**已经落下过几次** —— 从玩家身上的诅咒 status 反推，不另存计数器。
///
/// 反推得出来是因为三件事同时成立：诅咒**按顺序**发生（已经出过的是一段前缀）·
/// 三组的「另一边」是三个不同的 status · 瓦解三档 6 / 7 / 8 的子集和互不相同。
/// 于是和身上的量对得上的前缀长度**是唯一的**。
///
/// 对不上的只有两种情况：**人工制品挡掉过一次**（四个都是 debuff），或者瓦解另有来源（今天没有）。
/// 那时退回「身上看得见的最后一次」—— 源码的计数器照样加了一，内核会少数一次，
/// 下一次诅咒重复同一组（已知近似）。
pub fn curses_taken(p: &crate::state::Entity, sets: &[CurseSet]) -> usize {
    let dis = p.get(St::Disintegration);
    let other = |k: usize| p.get(sets[k].other) > 0;
    for m in (0..=sets.len()).rev() {
        if (m..sets.len()).any(|k| other(k)) {
            continue;
        }
        let want: i32 = (0..m).filter(|&k| !other(k)).map(|k| sets[k].disintegration).sum();
        if want == dis {
            return m;
        }
    }
    let last_other = (0..sets.len()).rev().find(|&k| other(k)).map_or(0, |k| k + 1);
    last_other.max(usize::from(dis > 0))
}

/// 这只敌人的出招里有没有「知识的诅咒」那种由玩家二选一的招（`advise` 靠它决定要不要摆 8 种选法）。
pub fn offers_curse_choice(def: u16) -> bool {
    enemy_def(def).moves.iter().any(|m| m.ops.iter().any(|op| matches!(op, EOp::CurseOfKnowledge(_))))
}

/// 瀑布巨兽死后锁的血量（[源码] `WaterfallGiant.TriggerAboutToBlowState`：`SetMaxAndCurrentHp(999999999m)`）。
///
/// **它本身就是「锁血等自爆」这个阶段的判据**：最大生命是观测量，从战斗中途同步进来也不会错相 ——
/// 和实验体拿最大生命判形态是同一个做法，不另开私有标记。
pub const ABOUT_TO_BLOW_HP: i32 = 999_999_999;
/// 瀑布巨兽「即将爆发」在 `moves` 里的下标（死亡规则强制进这一手，见 `M_WATERFALL_GIANT`）。
pub const WATERFALL_ABOUT_TO_BLOW_MOVE: u8 = 6;

/// 这只敌人是不是在**锁血等自爆**（瀑布巨兽死后那两手）。
#[inline]
pub fn about_to_blow(e: &crate::state::Entity) -> bool {
    e.max_hp >= ABOUT_TO_BLOW_HP
}

/// 「**这一个形态**还剩多少血」。锁血等自爆时是 **0**：它会自己炸死，那 999999999 不是要打的血 ——
/// 当成血量的话，叶评估会以为砍死它**亏了**十亿血（求解器宁可不收人头，和下面那条是同一个坑），
/// L3 那边「两边都死的仗谁把敌人打得更残」也会被这一个数淹掉。
#[inline]
pub fn hp_left_this_form(e: &crate::state::Entity) -> i32 {
    if about_to_blow(e) {
        0
    } else {
        e.hp.max(0)
    }
}

/// 这只敌人**死的那一刻会召出来多少血**：它身上挂着的 `Hook::EnemyDied` 召唤规则
/// （寄生物 4 × 19 · 意外 12 + 15 · 库存 k 层 ⇒ 后面还有 k 具 × 74）。**死了的不算**
/// —— 尸体的召唤已经发生过了，召出来的那几只自己在场上。
///
/// 数字**从 `POWERS` 读**（召唤那条规则里的 `hp` / `count`），不在这里再抄一遍：
/// 改召唤血量的人只改一处。表在第一次调用时筛一遍存下来 —— `eval` 每个叶子都要走这里，
/// 不该每次都把整张 `POWERS` 扫一遍。
pub fn death_summon_hp(e: &crate::state::Entity) -> i32 {
    static RULES: std::sync::OnceLock<Vec<(St, TOp)>> = std::sync::OnceLock::new();
    if !e.alive() {
        return 0;
    }
    let rules = RULES.get_or_init(|| {
        let mut v = Vec::new();
        for p in POWERS.iter().filter(|p| p.hook == Hook::EnemyDied) {
            for op in p.ops {
                if matches!(op, TOp::SummonN { .. } | TOp::SummonCarryingSelfMinusOne { .. }) {
                    v.push((p.st, *op));
                }
            }
        }
        v
    });
    let mut n = 0;
    for (st, op) in rules {
        let k = e.get(*st);
        if k <= 0 {
            continue;
        }
        n += match *op {
            TOp::SummonN { hp, count, .. } => hp * count,
            // 库存 k 的这一具死了换一具库存 k−1 的，…… 一直到库存 0 那一具死了才真死 ⇒ 还有 k 具
            TOp::SummonCarryingSelfMinusOne { hp, .. } => hp * k,
            _ => 0,
        };
    }
    n
}

/// 「这只敌人还剩多少血才算**真的**打完」—— **含还没出场的形态**。
///
/// 两类会让这个数大于当前血量：实验体的适生力（`St::Adaptable`，复活成下一形态）和
/// **死时召唤**（[`death_summon_hp`]：异蛙寄生虫 / 地精佣兵 / 巨斧机器人）；
/// 瀑布巨兽锁血等自爆时它是 0（`hp_left_this_form`）。
///
/// > **死时召唤那一类 2026-09-19 才补**，补之前是同一个陷阱的另一种形状：砍死宿主那一下，
/// > 这一项从「宿主剩的几点」跳到「召出来的那几只的满血」，求解器于是**不肯收掉宿主**。
/// > 是建地精佣兵时照出来的（起手牌组打它 10 个回合、掉 76 血；补上之后 4 个回合、30 血），
/// > 回头一查**早就在的异蛙寄生虫一直是这样**：实录 `act1_f12_elite_phrog` 玩家这一场 −1 血，
/// > 内核重打 64 次死 57 次（89%）、11 个回合；补上之后 0 死、5 个回合、p50 掉 18。
/// > 数字和归因见 verification-log 2026-09-19。
///
/// **为什么 L2 的叶评估必须用这个数**：`eval` 拿"当前敌人血量"当进度，
/// 而复活是**必然要来的** —— 用当前血量的话，砍掉最后那几点会让这一项
/// 从 4 跳到 200，于是求解器**宁可站着挨打也不肯收人头**。
///
/// [实测] 2026-08-30 第 3 幕 Boss 第 2 回合：Boss 剩 4 血、我 3 能量，
/// 求解器报「什么都不打最好 —— 手上这些牌打出去都是负收益」，
/// 单回合和 D=2 **两条线还一致**。而正解是砍掉那 4 血 ——
/// 换来的形态 2 第一手是复苏，根本不打人。
///
/// 这条是**游戏知识**，所以放在 `content` 而不是 L2（不变量：L2 不实现规则）。
pub fn remaining_hp_including_revives(e: &crate::state::Entity) -> i32 {
    let hp = hp_left_this_form(e) + death_summon_hp(e);
    if e.get(St::Adaptable) <= 0 {
        return hp;
    }
    if e.max_hp <= TEST_SUBJECT_FORM1_MAX {
        hp + TEST_SUBJECT_FORM_HP[1] + TEST_SUBJECT_FORM_HP[2]
    } else {
        hp + TEST_SUBJECT_FORM_HP[2]
    }
}

/// 敌人在**开战那一刻给玩家**挂的 status。
///
/// `EnemyDef::start_status` 只装挂在**它自己**身上的那些 —— 给对面挂的没有位置，
/// 而这类东西是存在的：[源码] `Rocket.AfterAddedToRoom` 里
/// `PowerCmd.Apply<SurroundedPower>(…, GetOpponentsOf(base.Creature), 1m, …)`。
///
/// # 为什么是一张按**名字**索引的侧表，而不是 `EnemyDef` 的一个新字段
///
/// 今天只有一条。给 `EnemyDef` 加一栏要动全表 73 条，而这一栏 72 条都是空的 ——
/// 那是把一个稀疏事实摊成一列噪声。按名字（不是下标）索引则**不会随表的增删漂移**，
/// `enemy_start_player_status_names_resolve` 守着每条都指得到真敌人。
///
/// 消费者是 [`crate::step::begin_combat`]（**唯一**）。对拍那条路一个字节不动：
/// `sync` 一律安 `enemy::UNKNOWN`、也从不走 `begin_combat`，这几个 status
/// 在那边本来就是从观测灌的。**它是给合成路径用的**（`synth::build`）。
pub static ENEMY_START_PLAYER_STATUS: &[(&str, St, i32)] = &[
    // 第 2 幕 Boss 的右半边。包围让**从背后打来**的攻击 ×1.5（`damage.rs`），
    // 少了它 L3 会系统性低估这场仗 —— 方向是最危险的那种（乐观）。
    ("火箭", St::Surrounded, 1),
];

/// 这只敌人开战时给玩家挂什么。查不到就是空。
pub fn enemy_start_player_status(name: &str) -> impl Iterator<Item = (St, i32)> + '_ {
    ENEMY_START_PLAYER_STATUS.iter().filter(move |(n, _, _)| *n == name).map(|(_, st, v)| (*st, *v))
}

/// 敌人身上**游戏不报、规则要读**的身份标记，第三栏是层数。
///
/// 今天三条。两条是「施咒者」（层数 1）：恶咒和抑制挂在**我**身上，而它们要在**施咒者死时**解除 ——
/// "施咒者是谁"（`PowerModel.Applier` / `DampenPower` 的 casters 集合）观测里没有。
/// 标记挂在骑士身上，解除规则就能靠 `Hook::EnemyDied` 只让死的那一只触发（见 `POWERS`）。
///
/// 第三条是鬼祟珊瑚群的**硬化外壳上限**（层数 20）：mod 报的是 `DisplayAmount`（本回合余额），
/// 而 `Amount` 本身观测里没有。[源码] `AfterAddedToRoom` 里写死 20、不吃进阶，所以按名字挂得出来。
/// 层数 2026-09-19 为它从「恒为 1」放宽成一栏。
///
/// # 为什么是按名字索引的侧表，而不是 `EnemyDef::start_status`
///
/// **两个消费者，两条路径**：`step::begin_combat`（合成路径，按 def 名）和
/// `replay::Replayer::sync`（对拍 / 实战路径，按观测到的名字）。后者一律把敌人安成
/// `enemy::UNKNOWN`、读不到 `start_status` —— 放进 `start_status` 的话，对拍路径上
/// 砍死幽灵骑士永远不解咒。和 `sync` 从观测补 `St::SlowSource` 是同一件事：
/// **"谁是施咒者"是身份，不是预测。**
pub static ENEMY_PRIVATE_MARKERS: &[(&str, St, i32)] = &[
    ("幽灵骑士", St::HexCaster, 1),
    ("魔法骑士", St::DampenCaster, 1),
    ("鬼祟珊瑚群", St::HardenedShellCap, 20),
];

/// 这只敌人身上该挂哪些私有标记（和层数）。查不到就是空。
pub fn enemy_private_markers(name: &str) -> impl Iterator<Item = (St, i32)> + '_ {
    ENEMY_PRIVATE_MARKERS.iter().filter(move |(n, _, _)| *n == name).map(|(_, st, v)| (*st, *v))
}

#[inline]
pub fn enemy_def(id: u16) -> &'static EnemyDef {
    &ENEMIES[id as usize]
}
