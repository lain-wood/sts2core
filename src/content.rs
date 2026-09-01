//! Card / enemy data tables.
//!
//! This is deliberately a *table*, not code. Phase 3 generates it from the
//! mod's `get_compendium` dump instead of hand-writing it.

use crate::ops::*;
use crate::state::St;

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
    // 73 [源码] 跃跃欲试「手牌中每有一张攻击牌，获得1点能量。」升级：费用 2 -> 1
    CardDef {
        name: "跃跃欲试",
        cost: 2,
        kind: Kind::Skill,
        targeted: false,
        exhausts: false,
        cost_minus_attacks: false,
        ops: &[Op::EnergyPerAttackInHand { per: 1 }],
        ops_upg: &[Op::EnergyPerAttackInHand { per: 1 }],
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
    // 第 1 条**只建了层数、没建后果**：沙坑归零即死走的是"敌人回合开始"，
    // 内核没有那个钩子（见 `St::Sandpit`）。所以在内核眼里这是
    // "1 费给敌人加一层没人消费的 status"，求解器**永远不会主动打它** ——
    // 那恰好是对的：它的价值（多活一个回合）本来就在单回合视角之外，
    // 该由人来判。**别把求解器不打它读成"不该打"。**
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
    // 剩下 18 张各自卡在一个还没有的机制上，逐条记在 CLAUDE.md。

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
];

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
            TOp::If { cond: TCond::OwnerIsPlayer, then: &[TOp::GrowSelf(-1)] },
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
    PowerDef {
        st: St::PlatedArmor,
        hook: Hook::EnemyTurnEnd,
        ops: &[TOp::If { cond: TCond::OwnerIsEnemy, then: &[TOp::OwnerBlock(Amt::Stacks)] }],
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
    // 轰鸣：在**我的回合结束**时自己消失（[源码] `AfterSideTurnEnd`）。
    // 它是敌人在**它的**回合挂上来的，所以撑过我的下一个回合再消失 ——
    // 这正是它能限制我一整个回合的原因。放 `TURN_SCOPED` 会在回合**开始**
    // 就清掉，等于这条 debuff 从来没生效过。
    PowerDef { st: St::Ringing, hook: Hook::TurnEnd, ops: &[TOp::ClearSelf] },
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
    // 凋萎存在（永世沙漏开局给玩家挂 6 层）。[源码] `WitheringPresencePower`：
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
    // 蒸汽喷发（瀑布巨兽计数器）
    St::SteamEruption,
];

/// **伤害管线读的 status**：乘区、上限、朝向、格挡翻倍。
///
/// 它们不是触发器（`POWERS` 里没有它们的规则），消费点全在
/// [`crate::damage`] 的 `apply_modifiers` / `card_block` 和 `step.rs` 里
/// 那几个窄 `if`。伤害管线的顺序在 `CLAUDE.md` 里，**改数字先去看那张表**。
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
pub static MARKER_STATUSES: &[St] = &[St::Pyre];

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
    // 语义未确定：怀疑是复活，但没能和「雾菇重新召唤」区分开。内核不建模复活。
    St::Illusion,
    St::EscapeArtist,
    St::Swipe,
    // ---- 下面这一组是 2026-08-29 这条测试第一次点出来的。
    //      它们**各自卡在一个还没有的机制上**，说明写在 `state.rs` 的声明里。
    //      在此之前这份欠账**不可枚举** —— 只有读遍 state.rs 才知道欠了什么。
    // 结实的卵：掉到 0 层原地变成幼虫。欠 `EOp::TransformSelf`
    St::Hatch,
    // 私有蜂巢：挨打往抽牌堆塞牌。欠「把 powered 传进 EnemyDamaged」+「随机位置塞 N 张」
    St::PersonalHive,
    // 重接（千足虫）：一节死了不移出战斗，隔一手回满 25 血。
    // 欠「尸体留在场上 + 死后仍然走出招表」。**方向是乐观的：内核低估这一场**
    St::Reattach,
    // 沙坑：欠「敌人回合**开始**」这个钩子（现有的 TurnStart 是我的回合开始）。
    // **战术后果：打那只 Boss 时"还剩几个回合"由它决定，不由血量决定**
    St::Sandpit,
    // 剧痛刺击（实验体阶段 2）：未格挡伤害塞伤口。
    // 欠「把"这一下打穿了多少"传进钩子」—— 现有的 Attacked 只说挨了一下。
    // 方向是**乐观**的（内核少给我塞一堆伤口）。
    St::PainfulStabs,
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
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "假商人版：战斗开始回 1 血（真品 2）。和真品同理 —— 血量是观测量，               建成 TurnStart 钩子会重复计数",
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
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "假商人版：休息后第一场战斗 +1 能量（真品 +2）。和真品卡在同一处：欠「上一个房间是不是休息处」",
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
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：Boss 战开始回 25 血。开打前就结算完了，观测里已经体现",
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
    RelicDef {
        id: "GNARLED_HAMMER", name: "扭曲锤子",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "拾起时给至多 3 张攻击牌附魔锋利3。欠附魔系统",
    },
    RelicDef {
        id: "KIFUDA", name: "木札",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "拾起时给至多 3 张牌附魔伶俐。欠附魔系统",
    },
    RelicDef {
        id: "ROYAL_STAMP", name: "王室印章",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "拾起时给一张牌附魔王室认证。欠附魔系统",
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
    RelicDef {
        id: "PEN_NIB", name: "钢笔尖",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "每第 10 张攻击牌双倍伤害。欠**跨战斗**的攻击计数 + 伤害翻倍修饰",
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
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "血量≤50% 时 +3 力量。欠**持续条件**（掉到一半以下要即时生效，不是一次性触发）",
    },
    RelicDef {
        id: "REPTILE_TRINKET", name: "爬行动物饰品",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "用药水时本回合 +3 力量。欠 Hook::PotionUsed",
    },
    RelicDef {
        id: "RUINED_HELMET", name: "损毁头盔",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "本场第一次获得力量翻倍。欠「施加 status 时插一手」的钩子",
    },
    RelicDef {
        id: "SLING_OF_COURAGE", name: "勇气投石索",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "精英战 +2 力量。欠「这场是不是精英」——房间类型不在 L1 里",
    },
    RelicDef {
        id: "STRIKE_DUMMY", name: "打击木偶",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "名字带「打击」的牌 +3 伤害。欠按**牌名**分组（内核只有 id，没有名字谓词）",
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
    RelicDef {
        id: "VENERABLE_TEA_SET", name: "古茶具套装",
        start_status: &[], private_status: &[], counter_to: None, modelled: false,
        note: "休息后的第一场战斗 +2 能量。欠「上一个房间是不是休息处」——不在 L1 里",
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
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "[源码] AfterPlayerTurnStart 且 TurnNumber<=1 升级手牌。手牌是观测量 —— \n               第2幕第33层开局五张全带 +，内核照抄，再建一遍就是重复升级",
    },
    RelicDef {
        id: "BONE_TEA", name: "骨茶",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "[源码] 接下来 N 场战斗开局升级初始手牌，N 带 SavedProperty。和风箱同理； \n               而且'还剩几场'是局外状态，战斗观测里根本没有，和古茶具一样判不了",
    },
    RelicDef {
        id: "BLOOD_VIAL", name: "小血瓶",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "[源码] AfterPlayerTurnStartLate 且 TurnNumber<=1 回 2 血。血量是观测量 —— \n               第2幕第31层开局 66->68 已经含它，建成 TurnStart 钩子会重复计数",
    },
    RelicDef {
        id: "PEAR", name: "梨子",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "局外：[源码] 拾起时最大生命值 +10。战斗层无关",
    },
    RelicDef {
        id: "JEWELED_MASK", name: "宝石面具",
        start_status: &[], private_status: &[], counter_to: None, modelled: true,
        note: "[源码] TurnNumber<=1 抽取牌堆中一张随机能力牌并使其当回合免费。手牌和费用是观测量，sync 直接同步",
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

/// 按游戏内部 id 查遗物。查不到 = 内容表里没有这一件。
pub fn relic_by_id(id: &str) -> Option<&'static RelicDef> {
    RELICS.iter().find(|r| r.id == id)
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
    pub const MYTE: u16 = 28;
    pub const OVICOPTER: u16 = 29;
    pub const SPINY_TOAD: u16 = 30;
    pub const QUEEN: u16 = 31;
    pub const DOORMAKER: u16 = 32;
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
    /// 蜂群术士（第2幕精英）。带**人体蜂房** —— 那个 status 只映射了名字，
    /// 行为没建模，见 `St::PersonalHive`。
    pub const ENTOMANCER: u16 = 43;
    /// 感染棱柱（第2幕精英）。带**活力火花** —— 它让我每张技能牌都带污染，
    /// 而污染让我挨的每一次攻击 +1。两个都只映射不建模，见 `St::Tainted`。
    pub const INFESTED_PRISM: u16 = 44;
    pub const TOUGH_EGG: u16 = 45;
    pub const HATCHLING: u16 = 46;
    /// 无厌沙虫（**第 2 幕 Boss**）。带**沙坑**即死倒计时 —— 那条内核只建了
    /// 层数、没建后果，见 `St::Sandpit`。
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
static M_CHOMPER: Machine = Machine {
    start: Next::Rand(&[
        Branch { to: 0, weight: 1, repeat: Repeat::Forever, cooldown: 0 },
        Branch { to: 1, weight: 1, repeat: Repeat::Forever, cooldown: 0 },
    ]),
    after: &[Next::Go(1), Next::Go(0)],
};

/// 残杀千足虫的一节 [源码] `DecimillipedeSegment`（三节共用一个类）。
///
/// 下标：0 扭动 5×2 · 1 壮硕 6+自身力量2 · 2 缠绕 8+给我 debuff
///
/// **平时是纯三循环** `缠绕 -> 壮硕 -> 扭动 -> 缠绕`（`FollowUpState` 首尾相接），
/// 起手由 `StarterMoveIdx % 3` 决定 —— 也就是**三节各从循环的不同点开始**
/// （[实测] 同屏三节正好一节 5×2、一节 6+Buff、一节 8+Debuff，一一对上）。
/// 内核不知道自己是第几节，所以起手用等权 `Rand`，和啃咬机的 `_screamFirst` 同一个处理。
///
/// **死亡之后那两手（DEAD -> REATTACH）没建**：见 `St::Reattach`。
/// 内核里这一节死了就是死了，所以它**低估这一场**。
static M_DECIMILLIPEDE: Machine = Machine {
    start: Next::Rand(&[
        Branch { to: 0, weight: 1, repeat: Repeat::Forever, cooldown: 0 },
        Branch { to: 1, weight: 1, repeat: Repeat::Forever, cooldown: 0 },
        Branch { to: 2, weight: 1, repeat: Repeat::Forever, cooldown: 0 },
    ]),
    after: &[Next::Go(2), Next::Go(0), Next::Go(1)],
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
/// 蜂群(3x7) -> 长矛(18) -> 信息素喷吐(强化) -> 蜂群 -> …
/// ```
///
/// 纯确定性，所以其实用 `loop_from = 0` 的固定循环也表达得了；
/// 写成机器是为了让"这三条边是从源码逐条抄的"这件事留在类型里。
static M_ENTOMANCER: Machine =
    Machine { start: Next::Go(0), after: &[Next::Go(1), Next::Go(2), Next::Go(0)] };

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
/// 0: 加压 (Buff) -> 1: 重踏 (15点+虚弱) -> 2: 撞击 (10点) -> 3: 虹吸 (回血10) -> 4: 高压枪 (20点) -> 5: 升压 (13点) -> 1 ...
static M_WATERFALL_GIANT: Machine = Machine {
    start: Next::Go(0),
    after: &[
        Next::Go(1), // 0: 加压 -> 1: 重踏
        Next::Go(2), // 1: 重踏 -> 2: 撞击
        Next::Go(3), // 2: 撞击 -> 3: 虹吸
        Next::Go(4), // 3: 虹吸 -> 4: 高压枪
        Next::Go(5), // 4: 高压枪 -> 5: 升压
        Next::Go(1), // 5: 升压 -> 1: 重踏
    ],
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
    // 18 [实测+wiki] 利齿之眼 —— 雾菇的幻象爪牙。
    // **塞的是晕眩不是黏液**（我原先猜错了），每回合 3 张进弃牌堆。
    // `ILLUSION_POWER` 内核不建模：实录里它被打死后满血回来过。
    EnemyDef {
        name: "利齿之眼", max_hp: 6,
        start_status: &[(St::Minion, 1), (St::Illusion, 1)], loop_from: 0, machine: None,
        moves: &[EnemyMove { name: "扰乱", intent: "StatusCard",
            ops: &[EOp::AddCardToDiscard { card: card::DAZED, count: 3 }] }],
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
    // 两个信徒同场时**起手位置不同**（实录里一个先 Buff 一个先攻击），
    // 内核每只都从第 0 手开始，表达不了这个错位。
    EnemyDef {
        name: "同族信徒", max_hp: 59, start_status: &[(St::Minion, 1)], loop_from: 0, machine: None,
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
    // 28 [wiki] 螨虫
    EnemyDef {
        name: "螨虫", max_hp: 67, start_status: &[], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "剧毒", intent: "Debuff", ops: &[EOp::PlayerStatus { st: St::Weak, amt: 2 }] },
            EnemyMove { name: "撕咬", intent: "Attack", ops: &[EOp::Attack { base: 13, hits: 1 }] },
            EnemyMove { name: "吸血", intent: "Attack", ops: &[EOp::Attack { base: 4, hits: 1 }, EOp::SelfStatus { st: St::Strength, amt: 2 }] },
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
    // 30 [wiki] 多刺蟾蜍
    EnemyDef {
        name: "多刺蟾蜍", max_hp: 119, start_status: &[(St::Thorns, 5)], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "尖刺突刺", intent: "Buff", ops: &[EOp::SelfStatus { st: St::Thorns, amt: 5 }] },
            EnemyMove { name: "尖刺爆发", intent: "Attack", ops: &[EOp::Attack { base: 23, hits: 1 }] },
            EnemyMove { name: "长舌鞭击", intent: "Attack", ops: &[EOp::Attack { base: 17, hits: 1 }] },
        ],
    },
    // 31 [wiki] 蜂后（Act 2 Boss）
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
    // 32 [wiki] 造门者（Act 2 Boss）
    EnemyDef {
        name: "造门者", max_hp: 489, start_status: &[], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "射线", intent: "Attack", ops: &[EOp::Attack { base: 31, hits: 1 }] },
            EnemyMove { name: "退回门中", intent: "Attack", ops: &[EOp::Attack { base: 40, hits: 1 }, EOp::SelfStatus { st: St::Strength, amt: 5 }] },
        ],
    },
    // 33 [wiki] 知识恶魔（Act 2 Boss）
    EnemyDef {
        name: "知识恶魔", max_hp: 379, start_status: &[], loop_from: 1, machine: None,
        moves: &[
            EnemyMove { name: "知识诅咒", intent: "Debuff", ops: &[EOp::PlayerStatus { st: St::Weak, amt: 2 }, EOp::PlayerStatus { st: St::Frail, amt: 2 }] },
            EnemyMove { name: "拍击", intent: "Attack", ops: &[EOp::Attack { base: 17, hits: 1 }] },
            EnemyMove { name: "知识淹没", intent: "Attack", ops: &[EOp::Attack { base: 8, hits: 3 }] },
            EnemyMove { name: "深思", intent: "Attack", ops: &[EOp::Attack { base: 11, hits: 1 }, EOp::SelfStatus { st: St::Strength, amt: 2 }] },
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
    //   3. tough 变体：观测到的 HP 是 wiki 括号里那个（209/199），而伤害基础值
    //      却是**不带括号**那一列。两个括号不是同一个变体轴，**没分辨出来**，
    //      所以这里只写实测到的那一组，不去猜另一组。
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
    // `start_status` 留空：包围/背后攻击/蟹之怒三个 status 内核都没有，
    // **按本仓库的规矩，没建模的东西不写进表里冒充建模过**。
    EnemyDef {
        name: "碾碎爪", max_hp: 209, start_status: &[], loop_from: 0, machine: None,
        moves: &[
            EnemyMove { name: "摧折", intent: "Attack", ops: &[EOp::Attack { base: 12, hits: 1 }] },
            EnemyMove { name: "增幅打击", intent: "Attack", ops: &[EOp::Attack { base: 4, hits: 1 }] },
            EnemyMove { name: "虫刺", intent: "Attack", ops: &[EOp::Attack { base: 6, hits: 2 }, EOp::PlayerStatus { st: St::Weak, amt: 2 }, EOp::PlayerStatus { st: St::Frail, amt: 2 }] },
            EnemyMove { name: "适应", intent: "Buff", ops: &[EOp::SelfStatus { st: St::Strength, amt: 2 }] },
            EnemyMove { name: "戒备打击", intent: "Attack", ops: &[EOp::Attack { base: 12, hits: 1 }, EOp::Block(18)] },
        ],
    },
    EnemyDef {
        name: "火箭", max_hp: 199, start_status: &[], loop_from: 0, machine: None,
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
    // 蜂群 3 点 × 7 段（进阶 8 段）；长矛 18（进阶 20）。
    //
    // **开局自带人体蜂房 1 层**（`AfterAddedToRoom`）。那个 status 内核
    // **没有建模行为**，只映射了名字和层数 —— 欠什么、以及它对打法的影响，
    // 写在 `St::PersonalHive` 的注释里。**读这只怪的求解结果时要记得那一段**：
    // 内核算不出多段攻击的真实代价。
    //
    // 信息素喷吐（`SpitMove`）在内核里被简化成「+2 力量」：
    // [源码] 是 `if (hive < 3) { hive += 1; 力量 +1 } else { 力量 +2 }`，
    // 而内核没有人体蜂房这个量，那个分支表达不了。**取了力量 +2 那一支**，
    // 因为它是不高估玩家的那边（力量涨得更快）。实测第一次喷吐走的是
    // hive < 3 那一支（力量 +1），所以**内核这里会高估敌人伤害**，
    // 这是刻意的方向，不是没发现。
    EnemyDef {
        name: "蜂群术士", max_hp: 145, start_status: &[(St::PersonalHive, 1)], loop_from: 0,
        machine: Some(&M_ENTOMANCER),
        moves: &[
            EnemyMove { name: "蜂群", intent: "Attack", ops: &[EOp::Attack { base: 3, hits: 7 }] },
            EnemyMove { name: "长矛", intent: "Attack", ops: &[EOp::Attack { base: 18, hits: 1 }] },
            EnemyMove { name: "信息素喷吐", intent: "Buff",
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
    // 而污染让我挨的每一次攻击 +1。两个 status 内核都**只映射不建模**，
    // 理由（尤其是"建了会重复计数"）写在 `St::Tainted` 的注释里。
    // **读这只怪的求解结果时要记得那一段**：求解器会高估技能牌。
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
    // 内核建了"给我 4 层沙坑"和"塞 6 张狂乱逃离"，**没建"归零即死"** ——
    // 理由和后果写在 `St::Sandpit`。读这只怪的求解结果时必须带着那一段：
    // **求解器眼里这场仗只有血量，而实际决定胜负的是回合数。**
    //
    // 一处已知近似：[源码] 那 6 张狂乱逃离是 **3 张进抽牌堆、3 张进弃牌堆**
    //（`i < 3 ? PileType.Draw : PileType.Discard`，且位置随机），
    // 而 `EOp::AddCardToDiscard` 只能进弃牌堆。**全塞弃牌堆**是保守的那边：
    // 进抽牌堆的那 3 张会更早污染我的抽牌，全进弃牌堆等于让它们晚一点出现。
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
                       EOp::AddCardToDiscard { card: card::FRANTIC_ESCAPE, count: 6 }] },
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
        name: "永世沙漏", max_hp: 512, start_status: &[(St::Artifact, 3)], loop_from: 0, machine: None,
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
    // **接续（死后复活 25 血）没建模，内核因此低估这一场** —— 见 `St::Reattach`。
    // [实测] 2026-08-27：我靠"三节要死就同一回合死"绕过了它 ——
    // 只要场上没有别的节活着，`DoReattach` 里那个 `AreAllOtherSegmentsDead()`
    // 就不会让它回来。**这条战术是内核给不了的，因为内核根本没有这个机制。**
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
    // A1 血量三阶段：
    //   Form 1 (单头): 111 HP (ToughEnemies 档 111，基础 100)
    //   Form 2 (双头): 212 HP (ToughEnemies 档 212，基础 200)
    //   Form 3 (三头): 313 HP (ToughEnemies 档 313，基础 300)
    //   总计 636 HP。
    //
    // 伤害与效果（A1 档）：
    //   Phase 1: 啃咬 20、头槌猛击 14 + 易伤 1（两者交替）
    //   Phase 2: 连环爪击 10×N（基础 3 段，每次出招段数 +1，持续自循环）
    //   Phase 3: 撕裂 10×3 -> 猛扑 45 -> 灼热咆哮（塞 3 灼伤 + 2 力量）-> 撕裂
    //
    // Powers:
    //   开局自带激怒 2（贯穿三阶段，每打 1 张技能 +2 力量）、适生力 1（死后复活进下一阶段）。
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
    // 69 [源码] 瀑布巨兽 `WaterfallGiant`（第 1 幕 Boss）
    //
    // A0/A2：血量 240（ToughEnemies 为 250）。
    // 出招：
    // 0: 加压：蒸汽喷发 +15
    // 1: 重踏：15 点伤害 + 玩家虚弱 1 + 蒸汽喷发 +3
    // 2: 撞击：10 点伤害 + 蒸汽喷发 +3
    // 3: 虹吸：自身回血 10 + 蒸汽喷发 +3
    // 4: 高压枪：20 点伤害（每用一次基础+5）+ 蒸汽喷发 +3
    // 5: 升压：13 点伤害 + 蒸汽喷发 +3
    // 循环：0 -> 1 -> 2 -> 3 -> 4 -> 5 -> 1 ...
    EnemyDef {
        name: "瀑布巨兽", max_hp: 240, start_status: &[], loop_from: 0,
        machine: Some(&M_WATERFALL_GIANT),
        moves: &[
            EnemyMove { name: "加压", intent: "Buff", ops: &[EOp::SelfStatus { st: St::SteamEruption, amt: 15 }] },
            EnemyMove { name: "重踏", intent: "Attack", ops: &[EOp::Attack { base: 15, hits: 1 }, EOp::PlayerStatus { st: St::Weak, amt: 1 }, EOp::SelfStatus { st: St::SteamEruption, amt: 3 }] },
            EnemyMove { name: "撞击", intent: "Attack", ops: &[EOp::Attack { base: 10, hits: 1 }, EOp::SelfStatus { st: St::SteamEruption, amt: 3 }] },
            EnemyMove { name: "虹吸", intent: "Heal", ops: &[EOp::SelfStatus { st: St::SteamEruption, amt: 3 }] },
            EnemyMove { name: "高压枪", intent: "Attack", ops: &[EOp::Attack { base: 20, hits: 1 }, EOp::SelfStatus { st: St::SteamEruption, amt: 3 }] },
            EnemyMove { name: "升压", intent: "Attack", ops: &[EOp::Attack { base: 13, hits: 1 }, EOp::SelfStatus { st: St::SteamEruption, amt: 3 }] },
        ],
    },
];

/// 实验体三个形态的最大生命（A1 非进阶档；`ToughEnemies` 是 111/212/313）。
/// **复活规则和 [`remaining_hp_including_revives`] 共用这三个数**，别在别处再抄。
pub const TEST_SUBJECT_FORM_HP: [i32; 3] = [100, 200, 300];

/// 区分形态 1 和形态 2 的门槛（形态 1 的最大生命 ≤ 这个数）。
/// 取在 100 和 200 中间，进阶档的 111/212 也照样分得开。
pub const TEST_SUBJECT_FORM1_MAX: i32 = 150;

/// 「这只敌人还剩多少血才算**真的**打完」—— **含还没出场的形态**。
///
/// 目前只有实验体的适生力（`St::Adaptable`）会让这个数大于当前血量。
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
    let hp = e.hp.max(0);
    if e.get(St::Adaptable) <= 0 {
        return hp;
    }
    if e.max_hp <= TEST_SUBJECT_FORM1_MAX {
        hp + TEST_SUBJECT_FORM_HP[1] + TEST_SUBJECT_FORM_HP[2]
    } else {
        hp + TEST_SUBJECT_FORM_HP[2]
    }
}

#[inline]
pub fn enemy_def(id: u16) -> &'static EnemyDef {
    &ENEMIES[id as usize]
}
