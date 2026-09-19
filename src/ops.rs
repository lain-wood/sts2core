//! Card effects as *data*. Adding a card is a table entry, not a code change.

use crate::state::St;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tgt {
    Me,
    Enemy,
    AllEnemies,
}

/// How a damage value scales off game state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scale {
    None,
    /// 灰烬打击: +n per card in the exhaust pile.
    PerExhaust(i32),
    /// 欺凌: +n per Vulnerable stack on the target.
    PerTargetVuln(i32),
    /// 完美打击「你每有一张名字中含有"打击"的牌，伤害+n」。
    /// 数的是**整个牌组**：抽/弃/手/消耗堆全算（玩家确认）。
    PerStrikeCard(i32),
    /// 全身撞击「造成你当前格挡值的伤害」：基础值 = 我当前的格挡。
    /// **格挡不消耗**（玩家确认），所以它只是被读一下，仍然照常挡伤害。
    /// 读出来之后照常过力量/易伤/缓慢 —— 和别的攻击牌没有区别。
    PlayerBlock,
}

/// 「如果…则…」的条件。
///
/// 这几个读的都是 `State` 里**早就存在**的每回合计数器 —— `state.rs` 里那段
/// 注释写得很清楚：这些字段存在，就是为了让条件牌变成普通牌，而不是像旧的
/// Python 模拟器那样直接拒绝评分。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cond {
    /// 邪眼 / 被遗忘的仪式：本回合消耗过卡牌
    ExhaustedThisTurn,
    /// 怨恨：本回合失去过生命值
    LostHpThisTurn,
    /// 急躁：手牌中没有攻击牌（这张牌自己已经离手了）
    NoAttackInHand,
    /// 狂宴：紧接着的上一次攻击**打死了**目标。
    /// 游戏里是 `attackCommand.Results.Any(r => r.WasTargetKilled)`（[源码]），
    /// 即牌自己内联判断，**不是**一个 `OnKill` 触发器 —— 所以走 `Cond` 不走 `Hook`。
    LastAttackKilled,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    Damage { base: i32, hits: i32, scale: Scale },
    DamageAll { base: i32, hits: i32, scale: Scale },
    /// 扯碎「造成 N 点伤害。在本场战斗中，你每失去过一次生命值，
    /// 这张牌就额外造成一次伤害。」**变的是段数，不是每段的数值** ——
    /// 所以它不能写成 `Scale`（那一族改的是基础伤害）。
    ///
    /// [源码] `TearAsunder`：段数 = `CalculationBase(0) + CalculationExtra(1) × (1 + M)`
    /// = **1 + M**，`M = State::hp_loss_hits`。
    /// 游戏把算好的段数直接渲染进卡面（`（命中3次）`），
    /// 所以"卡面写着造成3次"是**那一刻的快照**，不是这张牌的定义 ——
    /// 照它写死 `hits: 3` 是这张牌第一版的错法。
    DamagePerHpLossHit { base: i32 },
    /// 拆卸: repeat the preceding damage if the target is Vulnerable.
    DamageIfVuln { base: i32, scale: Scale },
    Block { base: i32 },
    Status { tgt: Tgt, st: St, amt: i32 },
    GainEnergy(i32),
    Draw(i32),
    /// Direct HP loss — bypasses block (烙印, 祭品, 腐化's self-damage).
    LoseHp(i32),
    /// 恶魔之焰: exhaust the whole hand, then deal `per` damage per card exhausted.
    ExhaustHandDamage { per: i32 },
    /// 烙印: open a choice of N cards in hand to exhaust.
    ExhaustChoose(i32),
    /// 主宰: gain 1 Strength per Vulnerable stack currently on the target.
    StrengthPerTargetVuln,
    /// 无情猛攻: next attack card costs 0.
    FreeNextAttack,
    /// 时候未到: 回血，封顶在 max_hp。
    Heal(i32),
    /// 拳斗「获得等量于所造成伤害的格挡」。
    ///
    /// "所造成伤害"取的是**过完乘区、扣格挡之前**的那个数（玩家确认：
    /// 7 点打在带易伤的目标上放大成 10，拿到的是 10 点格挡，不是 7）。
    /// 读的是 `State::last_damage`，所以必须紧跟在一个伤害 op 后面。
    BlockEqualToLastDamage,
    /// 万向斩「对所有其他敌人造成等量的伤害」。
    ///
    /// "等量"是**对主目标实际打出的数字**（玩家确认），其他敌人**不再各自
    /// 过一遍乘区**，直接吃这个平值（仍然会被各自的格挡吸收）。
    DamageOthersEqualToLast,
    /// 熔融之拳「将该敌人身上的易伤层数翻倍」。
    DoubleTargetVuln,
    /// 我方受到伤害，**走格挡**（灼伤/腐朽/瓦解那类"你受到 N 点伤害"）。
    /// 和 `LoseHp` 的区别就在这里：`LoseHp` 直接扣血、能唤醒撕裂，
    /// 这个先被格挡吃。玩家确认走格挡。
    TakeDamage(i32),
    /// 条件分支。`then` 是嵌套的 op 序列，条件不成立就整段跳过。
    Conditional { cond: Cond, then: &'static [Op] },
    /// 薪火之源：**能量上限** +n（不是当回合给 n 点能量）。
    ///
    /// 卡面写的是"在回合开始时，获得1能量"，但实测（2026-08-15 第1幕 Boss）
    /// 打出的瞬间显示就变成 `1/4` —— 上限当场从 3 变 4，当前能量只扣了牌费。
    /// 按卡面建成 `TurnStart` 钩子会**重复计数**：`sync` 从观测拿到
    /// `max_energy=4` 之后钩子再加 1，回合开始变成 5。对拍当场报出来了。
    GainMaxEnergy(i32),
    /// 暴走「将这张牌在本场战斗中的伤害增加 n」——写回 `CardInst.bonus`，
    /// 所以是**这一张牌实例**变强，不是这个牌名变强。
    /// 本次结算已经用的是旧值，从下一次打出开始生效。
    GrowThisCard(i32),
    /// 从弃牌堆检索 N 张牌（发掘、头槌）
    FetchFromDiscard(i32),
    /// 将 N 张手牌放到抽牌堆顶（战吼）
    PutOnTopOfDraw(i32),
    /// 升级手牌（武装）
    UpgradeInHand(i32),
    /// 飞剑回旋镖：`hits` 段伤害，**每一段各自随机挑一个活着的敌人**
    /// （[源码] `TargetingRandomOpponents`）。和 `DamageAll` 不一样：
    /// 那个是每个敌人都挨，这个是每一下随机落一个。
    DamageRandom { base: i32, hits: i32, scale: Scale },
    /// 狂宴：**永久**增加最大生命并同时回同样多的血（[源码] `GainMaxHp`）。
    GainMaxHp(i32),
    /// 跃跃欲试：按**手牌里攻击牌的张数**给能量，每张 `per` 点。
    /// 这张牌自己是技能牌，且打出时已经离手，不用担心数到自己。
    EnergyPerAttackInHand { per: i32 },
    /// 武装+：升级手牌里**所有**可升级的牌。基础版是 `UpgradeInHand(1)`（带选择）。
    UpgradeAllInHand,
    /// 头槌：从**弃牌堆**选 N 张放到**抽牌堆顶**。
    /// 和 `FetchFromDiscard` 的区别只在去处（那个进手牌），但去处不同
    /// 价值完全不同，不能共用。
    DiscardToTopOfDraw(i32),
    /// 愤怒：把**这张牌自己的一份复制**放进弃牌堆。
    /// 复制的是**当前这一张实例**（含升级态/暴走的 bonus），不是牌名。
    AddCopyOfThisToDiscard,
    /// 添柴「消耗所有手牌。每消耗一张牌，将1张**随机牌**加入你的手牌。」
    /// 升级版加入的是**已升级**的牌。
    ///
    /// **先消耗、再生成**（玩家判定 + [源码] 的顺序一致）：张数先快照，
    /// 全部消耗完了才开始生成，所以生成出来的牌不会被同一张添柴吃掉。
    /// 和恶魔之焰「不烧新抽的牌」是同一类顺序判定。
    ///
    /// 注意它是**生成**不是抽牌 —— 抽牌是黑暗之拥那条规则。
    ExhaustHandGenerate { upgraded: bool },
    /// 「将一张随机的某类型牌加入你的手牌，那张牌本回合免费」。
    ///
    /// 消费者三个：地狱之刃（攻击）、攻击药水（攻击）、技能药水（技能），
    /// 外加欧洛巴斯之酸一次要三张（攻击 + 技能 + 能力各一）。
    ///
    /// > **两瓶药水这里是刻意的保守近似。** [源码] `AttackPotion` /
    /// > `SkillPotion` 生成的是 **3 张让你挑 1 张**（`FromChooseACardScreen`），
    /// > 而"从生成的候选里选"需要一种内核还没有的 `Pending`。
    /// > 建成"随机给 1 张"等于**不给玩家挑选的收益**，方向是**低估自己** ——
    /// > 和痛殴取卡面基础值那条选的是同一边。欧洛巴斯之酸没有这个问题：
    /// > 它本来就是三张全给、不用选。
    GenerateFree(Kind),
    /// **击晕目标**（吹哨）。顶掉它下一次行动。
    ///
    /// [源码] `CreatureCmd.Stun` -> `Creature.StunInternal`，三条语义见
    /// [`crate::state::St::Stunned`]。**必须放在伤害之后**：源码的 `OnPlay`
    /// 就是先 `DamageCmd.Attack` 再 `CreatureCmd.Stun`，而 `StunInternal`
    /// 对已经死掉的目标是空操作 —— 所以斩杀的那一下白给一个击晕。
    StunEnemy,
    /// 狂乱逃离：把**这一张实例**的费用永久 +n（[源码] `EnergyCost.AddThisCombat`）。
    /// 和 `GrowThisCard` 是同一个形状，只是长的是费用不是伤害。
    GrowThisCardCost(i32),
    /// 余烬「造成N点伤害。随机消耗1张牌。」
    /// [源码] 消耗的是**抽牌堆顶**的 N 张（不是手牌），空了先洗。
    /// 卡面写"随机"是因为抽牌堆对玩家不可见，实现上就是取顶。
    ExhaustFromDrawTop(i32),
    /// 坚毅（基础版）「随机消耗 N 张**手牌**」。升级版是你自己选，走
    /// 现成的 `ExhaustChoose`。
    ExhaustRandomFromHand(i32),
    /// 痛殴「消耗你手牌中随机一张攻击牌，并将它的伤害添加给这张牌」。
    /// **永久加在这一张实例上**（写 `CardInst.bonus`），和暴走同一个机制。
    ExhaustRandomAttackAddDamage,
    /// 劫掠「抽牌直到你抽到一张非攻击牌」。
    /// [源码] 是 do-while：**至少抽一张**，然后只要抽到的是攻击牌就继续。
    DrawUntilNonAttack,
    /// 原始力量「将手牌中的所有攻击牌变化为巨石」。
    TransformAttacksInHand { into: u16, upgraded: bool },
    /// 旋风斩「对所有敌人造成 N 点伤害 **X** 次」。X = `State::last_x`。
    DamageAllXTimes { base: i32 },
    /// 倾泻「打出你抽牌堆顶部的 **X** 张牌」（升级 X+1）。
    AutoPlayFromDrawTopX { plus: i32, force_exhaust: bool },
    /// 破灭「打出抽牌堆顶部的牌并将其消耗。」
    /// [源码] `AutoPlayFromDrawPile(count, Top, forceExhaust: true)`。
    AutoPlayFromDrawTop { count: i32, force_exhaust: bool },
    /// 异蛇之油：把手牌里每张牌的费用**随机成 0..max-1**（[源码]
    /// `Rng.CombatEnergyCosts.NextInt(4)`）。
    ///
    /// 三条来自 [源码] 的过滤，一条都不能省：
    /// * **X 费牌不改**（`!c.EnergyCost.CostsX`）
    /// * 当前费用 < 0 的不改（`GetWithModifiers(None) >= 0`）
    /// * 只动**手牌**，不动抽牌堆/弃牌堆
    ///
    /// **抽出来的具体费用是内核 RNG 的一个样本，不是预测**（不变量 4：
    /// 内核的随机流故意和游戏不一致）。对拍时费用是软 diff，而且下一帧
    /// `sync` 会拿观测到的真实费用盖回去 —— "观测到的费用是权威"那条规矩。
    /// 求解器读到的是**分布正确的一次采样**，比"当作没变"诚实。
    RandomizeHandCosts { max: i32 },
    /// 手牌 + 弃牌堆全部并进抽牌堆，整体洗一次（瓶装潜能）。
    /// 语义和 [源码] 的两条命令一致，见 `State::shuffle_all_into_draw`。
    ShuffleAllIntoDraw,
    /// 失去 n 点**最大**生命。
    ///
    /// **和 `GainMaxHp(-n)` 不是一回事**，所以单开一条：
    /// [源码] `CreatureCmd.LoseMaxHp` 在「当前血 > 新的上限」时走一次
    /// `Damage(..., Unblockable | Unpowered | Move)` 把差额扣掉 ——
    /// **那是伤害**，会触发百年积木这类"掉血就发作"的东西。
    /// `GainMaxHp` 那条是直接改数字，两者差的正好是这个触发。
    LoseMaxHp(i32),
    /// 重振精神：消耗手牌里**所有非攻击牌**，每消耗一张获得 `per` 点格挡。
    /// 格挡走 `card_block`（[源码] `GainBlock(..., cardPlay)` 带牌来源），
    /// 所以吃脆弱、也吃臂甲的首次翻倍。
    ExhaustNonAttacksForBlock { per: i32 },
}

/// 「回合结束时这张牌还在手牌里」才发生的事。
///
/// 灼伤/腐朽/羞耻/感染都是这个形状，它们**不是能力牌**（效果挂在卡上不是挂在
/// 人身上），所以不能塞进 `POWERS`。单开一张表，避免给 `CardDef` 加两个
/// 只有五张牌用得上的字段。
pub struct HandEndDef {
    pub card: u16,
    pub ops: &'static [Op],
    /// 虚无：回合结束时若还在手牌中则消耗掉自己（晕眩/笨拙）
    pub void: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Attack,
    Skill,
    Power,
    Status,
    Curse,
    /// [源码] `CardType.Quest`（藏宝图那一类）。效果全在局外，战斗里只是一张
    /// 占位牌。单独立一个变体而不是塞进 `Status`，是因为 `upgradable()` 的
    /// 判据本来就写着「Status / Curse / Quest」三类，含糊在类型里迟早会被读错。
    Quest,
}

pub struct CardDef {
    pub name: &'static str,
    pub cost: i32,
    pub kind: Kind,
    pub targeted: bool,
    pub exhausts: bool,
    /// 踩踏: effective cost drops by 1 per attack already played this turn.
    pub cost_minus_attacks: bool,
    pub ops: &'static [Op],
    /// Upgraded variant; falls back to `ops` when empty.
    pub ops_upg: &'static [Op],
    pub cost_upg: i32,
}

// ---------------------------------------------------------------------------
// 触发器：能力牌作为数据
// ---------------------------------------------------------------------------
//
// 在这之前，`恶魔形态` 硬编码在 `start_player_turn` 里、`激怒` 硬编码在
// `play_card` 里 —— 每加一张能力牌就要再加一个 `if`，直接违反不变量 3
// 「加一张牌应该是加一行表」。106 张已发现的牌里有 21 张是触发式的，
// 照那个路子走会把 `step.rs` 变成一堆特判。
//
// 建模选择：**能力牌就是一个带层数的 status**（游戏本来也是这么显示的），
// 层数按本仓库既有约定 = 每次触发的效果值。规则本身放进 `content.rs`
// 的 `POWERS` 表，`step` 只负责在固定的几个点上 fire。

/// 触发点。只列**已经有卡在用**的，加钩子要连着加它的消费者，
/// 否则就是在给不存在的需求写代码。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hook {
    /// **任何一边**的回合开始的**最早一档**：我的回合是清格挡、能量回满、`TurnStart` 之前；
    /// 敌人回合是清敌人格挡、`EnemyTurnStart` 之前。两边各点一次火。
    ///
    /// [源码] `CombatManager.StartTurn` 里 `Hook.BeforeSideTurnStart(state, CurrentSide, …)`
    /// 排在 `AfterTurnStart`（清格挡）/ `SetupPlayerTurn`（能量、抽牌）/ `AfterSideTurnStart` 所有这些之前，
    /// 而且**不分哪一边**。
    ///
    /// 2026-09-19 为**硬化外壳**（鬼祟珊瑚群）加的，它是唯一的消费者：每一边的回合开始把
    /// 「这个回合还能掉多少血」回满。**为什么不挂在 `TurnStart` / `EnemyTurnStart` 上**：
    /// `TurnStart` 上有水银沙漏、滚石这些回合开始就打敌人的规则，压在同一个钩子上，
    /// 那几点伤害算进哪一个回合的额度就只剩 `POWERS` 的表内顺序 —— 奥利哈钢那条注释明令禁止的那种。
    /// [源码] 那几件是 `AfterSideTurnStart` / `AfterPlayerTurnStart`，本来就排在清零之后。
    SideTurnStart,
    /// 我的回合开始（能量已回满、格挡已清零、抽牌**之前**）
    TurnStart,
    /// 我的回合开始、**手牌已经发到手上之后**（`step::open_hand` 末尾）。
    ///
    /// 和 `TurnStart` 差的就是抽牌那一步，而这一步**分得开两批遗物**：
    /// * 抽牌**之前**：宝石面具（[源码] `BeforeHandDraw`，它要往抽牌堆里挑牌）
    /// * 抽牌**之后**：风箱 / 骨茶（[源码] `AfterPlayerTurnStart`，它们升级**手牌**）
    ///
    /// 挂在 `TurnStart` 上的话，风箱会去升级一手**还没发下来的空牌**。
    /// 2026-09-09 为这两件加的，加的时候就有两个消费者。
    HandDrawn,
    /// **获得了格挡**，且实际数值 > 0。谁在用：势不可当。
    ///
    /// 游戏侧是 `AfterBlockGained(creature, amount, ...)`，里面第一句就是
    /// `if (!(amount <= 0m) && creature == Owner)` —— 所以 **0 点格挡不触发**，
    /// 这一条是源码明写的，不是推的。
    ///
    /// 内核里所有给玩家加格挡的路径都必须走 `step::gain_block` 收口，
    /// 和 `CardExhausted` 走 `step::exhaust_card` 是同一个套路：
    /// 漏掉一条路径就是一个静默失效的触发器。
    GainBlock,
    /// 我的回合结束（弃手牌之前，敌人行动之前）
    TurnEnd,
    /// 我的回合结束的**最后一步**：弃完手牌之后、敌人行动之前。
    ///
    /// [源码] `EndPlayerTurnPhaseTwoInternal` 先 `FlushPlayerHand`，再 `Hook.AfterTurnEnd` ——
    /// 那里面先跑一遍 `AfterSideTurnEnd`、再跑一遍 `AfterSideTurnEndLate`。
    /// 和 `TurnEnd` 分开，是因为那个在弃牌**之前**，灼伤那类手牌发作也排在它后面。
    /// 2026-09-14 为瓦解（知识恶魔的诅咒）加的，它是唯一的消费者。
    TurnEndLate,
    /// **敌人**回合结束（敌人全部行动完之后）。
    ///
    /// [源码] 对应 `AfterSideTurnEnd(CombatSide.Enemy)`。
    /// 现有的 `TurnEnd` 是**我的**回合结束，两者不是一回事 ——
    /// 这个钩子是 2026-08-22 为高压（电击机器人每个敌人回合 +2 力量）加的，
    /// **有且只有它一个消费者**，符合"不加没有消费者的空钩子"。
    ///
    /// 注意它和 `St::Sandpit` 想要的**不是**同一个钩子：
    /// 沙坑要的是敌人回合**开始**里更靠后的那一档（`AfterSideTurnStartLate`）。
    EnemyTurnEnd,
    /// **敌人**回合结束的**早一档**：敌人全部行动完之后、`EnemyTurnEnd` 之前。
    ///
    /// [源码] 对应 `BeforeSideTurnEndEarly(CombatSide.Enemy)`，而 `EnemyTurnEnd` 是
    /// `AfterSideTurnEnd` —— `CombatManager.EndEnemyTurnInternal` 先 `Hook.BeforeTurnEnd`
    /// 再 `Hook.AfterTurnEnd`。
    ///
    /// 2026-09-14 为熟睡甲虫拆出来的，消费者是**敌人持有的覆甲给格挡**那一条：
    /// 覆甲在早一档给格挡，熟睡在晚一档减层、醒来时把覆甲整个移除。压在同一个钩子上，
    /// 先后就取决于 `POWERS` 的表内顺序（奥利哈钢那条注释明令禁止），排反了的后果是
    /// 醒来那一回合**少一堵墙**。和 `TurnEndVeryEarly` / `TurnEnd` 是同一个做法。
    EnemyTurnEndEarly,
    /// **敌人**回合结束的**最早一档**：比 `EnemyTurnEndEarly` 还早。
    ///
    /// [源码] `Hook.BeforeSideTurnEnd` 里三档依次跑
    /// `BeforeSideTurnEndVeryEarly` -> `BeforeSideTurnEndEarly` -> `BeforeSideTurnEnd`。
    ///
    /// 2026-09-17 为**沉睡**（乐加维林族母）加的，它是唯一的消费者：最后一个睡眠回合
    /// 要在覆甲给格挡**之前**把覆甲摘掉（[源码] `AsleepPower.BeforeSideTurnEndVeryEarly`，
    /// 而覆甲给格挡是 `BeforeSideTurnEndEarly`）。压在同一个钩子上就只剩表内顺序，
    /// 而那是奥利哈钢那条注释明令禁止的；排反了的后果是族母**白拿一堵 10 点的墙**。
    ///
    /// 和玩家侧的 `TurnEndVeryEarly` 是对称的两档，两边各有一个消费者。
    EnemyTurnEndVeryEarly,
    /// **敌人**回合开始（`begin_enemy_turn` 清完格挡之后，第一只出手之前）。
    ///
    /// [源码] 对应 `AfterSideTurnStart(CombatSide.Enemy)`。
    /// 2026-08-30 为**敌人持有的覆甲**加的，唯一的消费者就是它那条掉层规则。
    ///
    /// 为什么不塞进 `EnemyTurnEnd` 里和给格挡写成一条：`Amt::Stacks` 是
    /// 触发那一刻的快照，同一条规则里掉完层再读还是旧值；而拆成表里前后
    /// 两条又要依赖 `POWERS` 的表内顺序，那是奥利哈钢那条注释明令禁止的。
    /// 源码本来就把这两件事放在敌人回合的**两端**，照抄就同时躲开了两个坑。
    EnemyTurnStart,
    /// 玩家打出一张技能牌 —— 激怒。**在牌结算之前**触发，保持原有行为
    PlayerSkill,
    /// 有一张牌被消耗（不分来源：卡面自带消耗、恶魔之焰清手牌、烙印选牌）
    CardExhausted,
    /// 玩家在自己回合内失去生命（御血术/烙印/放血这类，不含敌人打的伤害）。
    /// **会被别的钩子套着触发**：绯红披风回合开始掉血 -> 撕裂。
    PlayerLoseHp,
    /// 玩家给敌人上了易伤。**每个吃到易伤的敌人各触发一次** ——
    /// 闪电霹雳打全体时触发 N 次，不是 1 次。
    ApplyVuln,
    /// 玩家打出一张攻击牌，**在牌结算之后**（和 `PlayerSkill` 相反，
    /// 那个是结算之前，为了保住激怒原有的已验证行为）。
    /// 放在之后是保守选择：全身撞击不会吃到自己这一下狂怒给的格挡。
    PlayerAttack,
    /// 玩家打出了**任意一张**牌，**在牌结算之后**。谁在用：凋萎存在。
    ///
    /// [源码] `PowerModel.AfterCardPlayed(choiceContext, cardPlay)`，
    /// `WitheringPresencePower` 里第一句就是
    /// `if (cardPlay.Card.Owner == Target.Player)` —— 所以**只数我打出的牌**。
    ///
    /// 和 `PlayerAttack` / `PlayerSkill` 的关系：那两个是**按类型过滤**的同一个
    /// 时点，这个不过滤。三者不合并，因为激怒必须留在"结算之前"
    /// （已验证的行为），而这一个和狂怒一样在结算之后。
    ///
    /// **每张牌只触发一次**：连环拳让一张攻击牌把 `ops` 跑两遍，但那是
    /// 「同一张牌多打出一次」而不是「多打出一张牌」，`cards_played` 也只 +1。
    /// 自动打出的牌（破灭/惊逃/倾泻）**算**，它们在游戏里同样产生 `CardPlay`。
    CardPlayed,
    /// **这只敌人挨了我一下攻击**，`ctx` = 挨打的那只，**只有它自己触发**。
    /// 谁在用：荆棘（多刺蟾蜍 / 蝌蚪）。
    ///
    /// 和 `EnemyDamaged` 差三条，所以不能合并（每一条都是 [源码] 定的）：
    ///
    /// | | `EnemyAttacked`（本钩子）| `EnemyDamaged` |
    /// |---|---|---|
    /// | 时点 | `absorb` **之前** | `absorb` **之后** |
    /// | 打死了还触发吗 | **触发**（伤害还没落地）| 不触发 |
    /// | 什么伤害算 | **只有攻击**（`powered`）| 任何伤害 |
    ///
    /// [源码] `ThornsPower.BeforeDamageReceived` 的门是
    /// `props.IsPoweredAttack()`，而 `Hook.BeforeDamageReceived` 在
    /// `CreatureCmd` 里排在 `DamageBlockInternal` **之前**
    /// —— 所以**格挡挡住也照样反弹**。
    EnemyAttacked,
    /// **敌人**挨了我一下（伤害已经结算完）。`ctx` = 挨打的那个敌人下标，
    /// 而且**只有它自己会触发** —— 和 `Attacked` 不一样，那个的持有者是玩家、
    /// ctx 是攻击者。谁在用：蜷身。
    ///
    /// 触发点在 `hit_enemy_with` 的最后，所以**伤害先落地再触发** ——
    /// 实测就是这个顺序（蜷身那 14 点格挡没能吃掉触发它的那 7 点伤害）。
    EnemyDamaged,
    /// **一个敌人死了**，`ctx` = 死的那只。谁在用：地精之角。
    ///
    /// 和 `EnemyDamaged` 是互斥的两条边：`hit_enemy_with` 打完之后，
    /// 活着就发 `EnemyDamaged`、死了就发这个。所以"打死"不会同时触发两者。
    ///
    /// **它不像 `EnemyDamaged` 那样只对 ctx 那只触发** ——
    /// 持有者是玩家（遗物），ctx 只是"谁死了"。
    EnemyDied,
    /// **别的敌人死了**（蟹之怒）。和 `EnemyDied` 是一对，语义正好相反：
    ///
    /// | 钩子 | 谁触发 |
    /// |---|---|
    /// | `EnemyDied` | **死掉的那一只自己**（寄生物召唤它自己那 4 只扭动虫）|
    /// | `AllyDied`  | **活着的其他敌人**（蟹之怒：同伴死了，我 +6 力量 +99 格挡）|
    ///
    /// 分成两个钩子而不是一个带条件的钩子，理由和 `Attacked`/`EnemyDamaged`
    /// 那一对一样：**"ctx 指谁"是每个钩子自己的语义**，混在一起迟早写反。
    AllyDied,
    /// 玩家挨了敌人一次攻击。**多段攻击每一段各触发一次**（玩家确认），
    /// 且不管这一下有没有被格挡完全吃掉。
    /// `run_trigger` 的 `ctx` 是攻击者的敌人下标。
    Attacked,
    /// **持有者（敌人）这一下攻击打穿了玩家的格挡**。`ctx` = 攻击者，**只有它自己触发**。
    /// 谁在用：纸伤难愈（咬人卷轴，掉最大生命）· 剧痛刺击（实验体阶段 2，塞伤口）。
    ///
    /// 和 `Attacked` 出自同一个触发点（`take_attack_hit`），拆开是因为三条都不一样，
    /// 每条都是 [源码] 定的：
    ///
    /// | | `Attacked` | `AttackUnblocked`（本钩子）|
    /// |---|---|---|
    /// | 持有者 | 玩家（火焰屏障）| **打人的那只敌人** |
    /// | 被格挡完全吃掉还算吗 | 算 | **不算**（`UnblockedDamage > 0`）|
    /// | 谁触发 | 玩家侧 | 只有 `ctx` 那只 |
    ///
    /// **多段攻击每一段各判一次**。纸伤难愈是 `AfterDamageGiven`（本来就逐段）；
    /// 剧痛刺击是 `AfterAttack` 里数「打穿了几段」再乘 —— 两种写法给出同一个总数，
    /// 而伤口进弃牌堆，攻击中途没有任何东西读它。
    ///
    /// 两条敌人回合的路径（`enemy_turn` / `end_turn_with_incoming`）都走
    /// `take_attack_hit`，所以注入式威胁下它也照样发作。
    AttackUnblocked,
    /// 玩家**真的掉了血**（一次攻击里被格挡吃掉之后仍有伤害落到 HP 上）。
    ///
    /// 和 `Attacked` 分得很开：那个是"挨了一下"（火焰屏障要反伤，
    /// 哪怕这一下被完全挡住），这个是"血真的少了"。百年积木要的是后者
    /// （[源码] `CentennialPuzzle.AfterDamageReceived` 判的是
    /// `result.UnblockedDamage > 0`）。
    ///
    /// **三条掉血路径各点一次火**，收口在 `step::after_player_hp_lost`：
    /// 挨打、卡牌"失去生命"、灼伤那类过格挡的伤害。
    ///
    /// 「卡牌失去生命也算」是 2026-08-27 第 1 幕 Boss 实测纠正的：
    /// 那一版只在挨打那条路上点火，放血打出去时游戏手牌 6 / 内核 3。
    /// [源码] 里放血走的是 `CreatureCmd.Damage(..., Unblockable, ...)` ——
    /// **是伤害，只是不可格挡**。
    PlayerDamaged,
    /// 我的回合结束，**在 `TurnEnd` 之前**（[源码] `BeforeSideTurnEndVeryEarly`）。
    /// 谁在用：奥利哈钢的第一段（快照"这时候有没有格挡"）。
    ///
    /// 分成两段不是洁癖：`TurnEnd` 里**覆甲也在给格挡**，压成一个钩子的话，
    /// 奥利哈钢要么永远不触发（覆甲先给了格挡），要么触发时机取决于
    /// `POWERS` 表里的先后 —— 那是**隐式**的，改一行表的顺序就能悄悄改行为。
    /// 两个钩子把顺序写进类型里。
    TurnEndVeryEarly,
    /// **战斗胜利结算的第一段**（[源码] `AfterCombatVictoryEarly`）。谁在用：带骨肉。
    ///
    /// 和下面那个分成两个钩子不是洁癖：带骨肉判「血量 ≤ 50%」，
    /// 而燃烧之血会把血量抬高 6 点。合成一个钩子就得靠表里的顺序来定先后，
    /// 那是**隐式**的；两个钩子把顺序写在类型里。
    CombatVictoryEarly,
    /// 战斗胜利结算的第二段（[源码] `AfterCombatVictory`）。谁在用：燃烧之血。
    ///
    /// 两个钩子都**只在玩家活着的时候**触发（源码两处都是
    /// `if (!Owner.Creature.IsDead)`），且**只触发一次** —— `check_over`
    /// 会被反复调用，靠"这次是不是刚翻成 over"守着。
    CombatVictory,
}

/// 触发效果的数值从哪来。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Amt {
    /// 用这个 power 自己的层数（绝大多数）
    Stacks,
    /// 写死（绯红披风的「失去 1 点生命」和它的格挡值不是同一个数）
    Fixed(i32),
    /// **本场当前是第几个回合**（抱抱先生：伤害 = `TurnNumber`）。
    ///
    /// 为什么不用「层数 1 + 每次 `GrowSelf(1)`」那条现成的路（滚石就是那么写的）：
    /// 那样相位靠**跨帧携带**，从战斗中途 `sync` 进来就会偏；而回合数是
    /// `obs.round` 直接同步进来的观测量，永远对。
    /// 同样的坑摆动球踩过一次（`TurnsSeen` 跨战斗保留，假设从 0 开始会系统性错一个回合）。
    TurnNumber,
    /// 本回合**已经打出的牌数**（`State::cards_played`）。
    /// 娇弱回合末要还回去的正是这个数。
    CardsPlayed,
    /// **手上现在有几张牌** × 这个 power 的层数（斗篷扣：每张 1 点格挡）。
    ///
    /// [源码] `CloakClasp.BeforeSideTurnEnd` 是 `(int)(cards.Count * Block)`，
    /// 所以层数是**每张给几点**、不是总数 —— 和 `Stacks` 那一族的语义一致。
    HandCardsTimesStacks,
    /// **持有者身上另一个 status 的当前层数**，夹到 ≥ 0（抢夺力量 / 抢夺速度的退还量）。
    ///
    /// [源码] `PossessStrengthPower` 私下记着一张「从谁身上偷了多少」的字典，
    /// 自己死时逐条还回去。那张字典**观测里没有**（面板上 `POSSESS_STRENGTH_POWER`
    /// 恒为 1），而 `sync` 每帧从观测重建、私有计数器带不过来 ——
    /// 所以退还量读的是**它自己的力量/敏捷**：那一手偷 2、自己加 2，两边逐次相等。
    ///
    /// 两者**只在两种情况下分岔**，方向相反：
    /// * 我的人工制品挡掉了那 −2（[源码] `GetTypeForAmount` 把负数的力量/敏捷算 debuff）
    ///   —— 没偷到、它照样 +2 ⇒ 这里**多还**（乐观）
    /// * 我永久削掉了它的力量 ⇒ 这里**少还**（悲观）
    ///
    /// 夹到 0 是因为「退还」不该变成再偷一次。
    OwnerStacksOf(St),
}

/// 触发器里的条件。**每一条都有一件真遗物在用**，没有为将来预留的。
///
/// 数值取自 [源码]，不是卡面 —— 三处对不上：
/// * 灯笼源码是 `TurnNumber <= 1`（不是 `== 1`）
/// * 摆动球源码是 `(TurnsSeen + 1) % 3 == 0`，而 `TurnsSeen` 带
///   `[SavedProperty]`，**跨战斗保留** —— 所以相位不是每场都从 0 开始
/// * 精致折扇是 `AttacksPlayedThisTurn % 3 == 0`，即第 3/6/9 张，不是只有第 3 张
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TCond {
    /// 我方当前**没有格挡**（奥利哈钢第一段的快照条件）
    OwnerHasNoBlock,
    /// 拥有者的格挡 ≥ n。招架盾（[源码] `ParryingShield` 判的是
    /// `!(Block < 10)`，也就是 ≥ 10，边界值算触发）。
    OwnerBlockAtLeast(i32),
    /// 本场第 n 个回合，或更早（灯笼 `<= 1`）
    TurnAtMost(i32),
    /// 回合数 ≥ n。[源码] 里长这样：`if (TurnNumber < 3) return;`（佩尔之肉）。
    TurnAtLeast(i32),
    /// 回合数是 n 的倍数（第 n / 2n / 3n… 回合）。
    ///
    /// **和 `EveryNTurns` 的区别是有没有相位**：摆动球的计数器跨战斗保留，
    /// 所以它必须带一个从观测灌进来的相位；开心小花/花粉核心的计数器
    /// 每场从 0 数（[源码] 的 `TurnsSeen` 不带 `[SavedProperty]`），
    /// 用回合数直接取模就够了 —— 不必为了统一而硬塞一个恒为 0 的相位。
    TurnMultipleOf(i32),
    /// 本场正好第 n 个回合（历石 = 7）
    TurnIs(i32),
    /// 每 n 个回合触发一次，相位取 `phase` 那个 status 的层数（摆动球）。
    ///
    /// **相位必须来自观测**（遗物面板上那个计数器），不能假设从 0 开始 ——
    /// 上一场打了几个回合会把它带过来。`RelicDef::counter_to` 负责灌。
    EveryNTurns { n: i32, phase: St },
    /// 本回合打出的攻击牌张数是 n 的倍数（精致折扇 n=3）
    EveryNthAttackThisTurn(i32),
    /// 同回合第 n / 2n / 3n… 张**技能牌**（开信刀）。
    /// 和上面那条逐字同构，数的量不同。
    EveryNthSkillThisTurn(i32),
    /// 某个 status 大于 0（奥利哈钢第二段：第一段有没有把它武装起来）
    OwnerHas(St),
    /// 持有者的**当前血量 ≤ 这个 power 自己的层数**（耕地：150）。
    /// [源码] `PlowPower.AfterDamageReceived` 判的就是 `CurrentHp <= Amount`。
    OwnerHpAtMostStacks,
    /// 这个 power **自己的层数 ≤ n**（凋萎存在：减到 0 就发作）。
    ///
    /// 判的是**同一段 op 序列里前面几条改过之后**的层数，不是触发时的快照 ——
    /// [源码] 里 `CardsLeft--` 和 `if (CardsLeft <= 0)` 是紧挨着的两句。
    SelfStacksAtMost(i32),
    /// 这个 status 自己的层数 ≥ n。逃脱大师的「减到 1 就不再减」用它。
    SelfStacksAtLeast(i32),
    /// 持有者已经死了（血量 ≤ 0）。**只有还留在场上的死人用得着** ——
    /// 实验体被砍死之后要等到自己的回合才复苏，那条规则靠它认出
    /// 「现在该回血了」。
    OwnerIsDead,
    /// 持有者的**最大**生命 ≤ n / ≥ n。
    ///
    /// 实验体用它当**阶段指示器**：三个形态的最大生命是 100 / 200 / 300，
    /// 而最大生命是**观测量**（每帧同步进来），所以拿它判阶段不需要任何
    /// 内核私有的计数器 —— 复活一次它自己就变了。
    /// [源码] `RespawnMove` 里那个 `switch (Respawns)` 就是这个意思。
    OwnerMaxHpAtMost(i32),
    OwnerMaxHpAtLeast(i32),
    /// 持有者是玩家 / 是敌人。
    ///
    /// **为什么需要它**：钩子的归属和 status 的归属是两件事。
    /// [源码] 里有两种真实存在的组合 ——
    /// 盾墙是**敌人持有、挂在玩家回合开始**（`side == CombatSide.Player`），
    /// 而覆甲**同一个 status 两边都能持有、时序却不一样**
    /// （玩家在我的回合末给格挡，敌人在它自己的回合末给）。
    /// 所以 `fire_ctx` 那层的「这个钩子给谁发」一刀切不了，
    /// 门只能开在规则自己身上。
    ///
    /// 消费者：覆甲那四条（两条玩家、两条敌人）。
    OwnerIsPlayer,
    OwnerIsEnemy,
    /// 持有者的**手牌是空的**（尖叫酒壶）。
    ///
    /// [源码] `ScreamingFlagon.BeforeSideTurnEnd` 判的是
    /// `PileType.Hand.GetPile(Owner).IsEmpty`，而那个钩子跑在**弃手牌之前**。
    /// **时点是这条规则的全部内容**：挪到弃手牌之后它就恒真，
    /// 这件遗物会变成"每回合白给 20 点"。
    HandEmpty,
    /// **刚打出的那张牌**是这一类（流电：只认能力牌）。
    ///
    /// 读 `State::last_played_card` —— 它在 `resolve_played_card` 开头写好，
    /// 而 `Hook::CardPlayed` 在同一个函数末尾点火，读到的就是触发它的那一张。
    ///
    /// [源码] `GalvanicPower` 判的是 `cardPlay.Card.Affliction is Galvanized`，
    /// 而 `BeforeCombatStart` + `AfterCardEnteredCombat` 给**每一张没有别的 affliction
    /// 的能力牌**挂上它。内核没有 affliction 这个概念（今天也没有任何别的来源会给
    /// 能力牌挂 affliction），所以等价于"能力牌全都算"。
    LastPlayedKindIs(Kind),
    /// **刚落地的那一下伤害是攻击**（[源码] `props.IsPoweredAttack()`）。人体蜂房。
    ///
    /// 读 `State::last_hit_attack`，`hit_enemy_with` 在点 `EnemyDamaged` 之前写好。
    /// 卡牌攻击的每一段都算；药水 / 遗物 / 荆棘 / 能力牌的伤害（`Unpowered`）不算。
    LastHitWasAttack,
    /// **刚落地的那一下打穿了格挡**（[源码] `DamageResult.UnblockedDamage > 0`）。熟睡。
    ///
    /// 读 `State::last_hit_unblocked`，同上。**不分是不是攻击** —— 熟睡的门里没有 `IsPoweredAttack`。
    LastHitUnblocked,
}

/// 触发时能做的事。刻意做得很小 —— 每多一条都要有一张真牌在等着它。
///
/// `Owner` 指**持有这个 power 的实体**：恶魔形态的 owner 是玩家，
/// 激怒的 owner 是敌人。这样一套词汇同时覆盖两边。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TOp {
    OwnerStatus { st: St, amt: Amt },
    /// 给**玩家**挂 status，而持有者是**敌人**（活力火花：我每打一张技能牌，
    /// 它就给我 N 层污染）。
    ///
    /// 和 `OwnerStatus` 的区别就是给谁 —— 激怒是敌人给**自己**加力量，
    /// 这条是敌人给**我**加 debuff，同一个 `Hook::PlayerSkill` 上的两种形状。
    PlayerStatus { st: St, amt: Amt },
    /// 给**场上所有活着的敌人**挂 status（弹珠袋的易伤、红面具的虚弱）。
    /// [源码] 两件都是 `PowerCmd.Apply<..>(choiceContext, combatState.HittableEnemies, ..)`。
    AllEnemiesStatus { st: St, amt: Amt },
    /// **不过脆弱**：卡面原文是「从**卡牌**中获得的格挡值减少25%」，
    /// 能力牌给的格挡不是从卡牌来的。这条未实测。
    OwnerBlock(Amt),
    OwnerLoseHp(Amt),
    /// **走格挡**地受到伤害（缠绕）。和 `OwnerLoseHp` 的区别就是格挡：
    /// 那个直接扣血、会唤醒撕裂；这个先被格挡吸收，和敌人打过来的伤害同路，
    /// 所以**不唤醒撕裂**。和 `Op::TakeDamage` 是同一条语义。
    OwnerTakeDamage(Amt),
    /// 回血，封顶在 max_hp（再生）
    OwnerHeal(Amt),
    OwnerEnergy(Amt),
    OwnerDraw(Amt),
    /// 只对玩家持有的 power 有意义（滚石）
    DamageAllEnemies(Amt),
    /// 触发后把自己的层数加 n（滚石：伤害每回合 +5）
    GrowSelf(i32),
    /// 把持有者身上**另一个** status 清零（耕地：清光力量）。
    /// 和 `ClearSelf` 的区别就是清谁。
    OwnerClearStatus(St),
    /// 把持有者身上**另一个** status **置成** n（不是加 n）。
    ///
    /// 和 `OwnerClearStatus` 是同一件事的一般化（那个就是置 0），
    /// 和 `SetSelf` 的区别是改谁。
    ///
    /// 谁在用：坚定不移 —— 我的回合开始时把 `UnmovableCharge` 重置成层数。
    /// **必须是"置"不是"加"**：没用完的余额不该攒到下一回合，
    /// 写成 `OwnerStatus`（加法）的话，十个回合不吃格挡就能攒出十次翻倍。
    OwnerSetStatus { st: St, amt: Amt },
    /// 给场上**所有 `def` 这一种**敌人加 n 点格挡。
    ///
    /// 谁在用：盾墙（活体盾开局自带 25 层）。
    /// **敌人种类是写死在源码里的，不是"所有队友"**：
    /// [源码] `RampartPower.AfterSideTurnStart` 里是
    /// `CombatState.Enemies.Where(c => c.Monster is TurretOperator)` ——
    /// 场上换一种队友就一点格挡都不给。所以这里带 `def` 参数，
    /// 而不是做成一个"给全体队友加格挡"的通用 op。
    ///
    /// 格挡是 `ValueProp.Unpowered`，不过任何乘区。
    BlockEnemiesOfDef { def: u16, amt: Amt },
    /// 强制持有者下一手出第 n 招（耕地把仪式兽打进眩晕）。
    /// 只对敌人持有者有意义；玩家持有时是空操作。
    OwnerForceMove(u8),
    /// 触发后**把自己清零** —— 一次性的 status 用它收口（蜷身）。
    /// 和 `GrowSelf(-n)` 的区别：那个要知道层数是多少，这个不用。
    ClearSelf,
    /// 打触发它的那个敌人（火焰屏障的反伤）。只在带 `ctx` 的钩子里有意义。
    DamageContextEnemy(Amt),
    /// 打**这一击的攻击方**（荆棘）。两侧对称，所以是一个 op 不是两个：
    ///
    /// * 持有者是玩家（`Hook::Attacked`）⇒ 打 `ctx`，也就是打我的那只敌人
    /// * 持有者是敌人（`Hook::EnemyAttacked`）⇒ 打我
    ///
    /// **伤害是 `Unpowered`**（[源码] `ValueProp.Unpowered | SkipHurtAnim`）：
    /// 不吃力量、不过攻防乘区，只过难以杀灭和无实体。
    ///
    /// 打到我身上时**走格挡**（游戏侧就是一次普通的 `CreatureCmd.Damage`），
    /// 但**不走 `take_attack_hit`** —— 那条路会再触发一次 `Hook::Attacked`
    /// 和孤注一掷，而反弹伤害不是攻击（`Unpowered` 进不了
    /// `IsPoweredAttack()` 那道门），两边都不该发作。
    /// 掉血照样记进 `after_player_hp_lost`（百年积木看的是"真掉了血"）。
    DamageAttacker(Amt),
    /// 召唤 `count` 只敌人（寄生物：宿主死时冒出 4 只 Wriggler）。
    ///
    /// 和 `EOp::Summon` 是同一件事的两个入口：那个是敌人**出招**召唤，
    /// 这个是**触发器**召唤。两者共用 `step::summon_one`，不许各写一份。
    SummonN { def: u16, hp: i32, count: i32 },
    /// 召唤一只 `def`，并把**这条规则自己的层数减 1** 设给它的同名 status。
    ///
    /// [源码] `StockPower.AfterDeath`：`axebot.StockAmount = base.Amount - 1`，
    /// 也就是"我死了，换上一个库存少一个的我"。
    ///
    /// **为什么不能用 `SummonN`**：那个走被召唤方 `EnemyDef` 的 `start_status`，
    /// 巨斧机器人的 start_status 是「库存 2」—— 用它召唤等于每一具都满库存，
    /// 内核会以为这场仗**永远打不完**。
    ///
    /// **减到 0 就是"不挂"**（`set` 成 0 等于没有这个 status），
    /// 而 `fire` 对 0 层的 status 本来就不触发 ⇒ 最后一具死了战斗自然结束，
    /// 不需要额外判断。这和游戏侧 `AfterAddedToRoom` 里
    /// `if (StockAmount > 0)` 才挂 power 是同一件事。
    SummonCarryingSelfMinusOne { def: u16, hp: i32 },
    /// 把持有者的最大生命设成 n，**不回血**。
    ///
    /// 和 [`TOp::OwnerHealToFull`] 是**刻意拆开的两步**，因为游戏里它们
    /// 发生在不同的时点：实验体被砍死那一帧只定下一个形态，
    /// 血量留在 0 —— 它在我这个回合剩下的时间里仍然是死的、**打不到**，
    /// 观测里连这只敌人都不出现（[实测] 2026-08-30 那一帧 `enemies: []`）。
    /// 回满是它自己回合开始（`RespawnMove`）才做的事。
    ///
    /// **合成一步是错的，而且方向乐观**：那样求解器会以为"砍完还能接着打"，
    /// 于是把收人头的牌排在回合中间 —— 而实际那之后整回合的能量都没处花。
    /// 2026-08-30 实战就是这么白扔了约 6 点能量。
    OwnerSetMaxHp(i32),
    /// 把持有者的血量回满到当前最大生命（实验体的复苏那一手）。
    /// [源码] `TestSubject.Revive`：`SetMaxHp(hp)` 然后 `Heal(hp)`。
    OwnerHealToFull,
    /// 清光持有者身上**除了名单里那几个之外**的所有 status。
    ///
    /// 这是[源码]的**死亡剥离**：`Creature.cs` 删掉所有
    /// `ShouldPowerBeRemovedAfterOwnerDeath()` 返回 true 的 power，
    /// 而那个虚方法**默认就是 true**，只有 `AdaptablePower` /
    /// `PainfulStabsPower` / `MinionPower` / `ReattachPower` 那几个重写成 false。
    ///
    /// **写成"保留名单"而不是"清除名单"是有意的**：死亡剥离连**我给它挂的
    /// 易伤/虚弱**一起清掉，用清除名单迟早会漏。
    /// 实验体身上最要紧的两条后果：
    /// **激怒被剥掉**（所以只有第一条命打技能牌才涨力量，玩家给的判定，
    /// 源码在这里对上了）、**攒的力量也被剥掉**（第一阶段喂进去的全归零）。
    ClearOwnerStatusesExcept(&'static [St]),
    /// 把持有者身上某个 status 在 0 和 1 之间**翻转**。
    ///
    /// [源码] `NemesisPower.AfterSideTurnEnd` 就是一个
    /// `_shouldApplyIntangible = !_shouldApplyIntangible` 的开关：
    /// 翻到 true 就挂 1 层无实体，翻到 false 就把它摘掉。
    /// 内核直接翻转**无实体本身**，不用再造一个私有标记。
    ///
    /// **不能用两条互斥的 `TOp::If` 代替**：`ops` 是顺序执行的，
    /// 前一条改完标记之后，后一条读到的就是新值，两条会一起发作。
    OwnerToggleStatus(St),
    /// 惊逃：从手牌里**随机挑 N 张可打出的攻击牌自动打出**（攻击随机敌人）。
    AutoPlayRandomAttacksFromHand(Amt),
    /// 杂耍：本回合打出的**第 `nth` 张**攻击牌，复制 `amt` 份进手牌。
    /// 条件写进 op 而不是另造一张条件表 —— `POWERS` 本来就不带条件，
    /// 而这是目前唯一一条需要"第几张"的规则。
    CloneLastAttackToHandAt { nth: i32, amt: Amt },
    /// 好勇斗狠：从**弃牌堆**随机取 N 张攻击牌进手牌，**并升级**它们。
    FetchRandomAttacksFromDiscardUpgraded(Amt),
    /// 带骨肉：**当前血量 ≤ max_hp × pct%** 时才回血，否则什么都不做。
    ///
    /// 阈值取整照 [源码]：`(int)(MaxHp * pct / 100)`，**向下取整**
    /// （80 血 → 40）。条件写进 op 而不是另造一张条件表 ——
    /// 和 `CloneLastAttackToHandAt` 同一个理由：`POWERS` 本来就不带条件，
    /// 而这是目前第二条、也是唯一一条需要"血量阈值"的规则。
    OwnerHealIfHpAtMost { pct: i32, amt: Amt },
    /// 条件包装。条件不成立就整段跳过。
    ///
    /// 为什么不给 `PowerDef` 加一个 `cond` 字段：**一条规则可以有多个条件段**，
    /// 而且条件是"这一段做不做"而不是"这条规则算不算数"。把它做成 `TOp`
    /// 和 `Op::Conditional` 是同一个形状，也和 `CloneLastAttackToHandAt`
    /// 把条件写进 op 的既有做法一致。
    If { cond: TCond, then: &'static [TOp] },
    /// 打**随机一个**活着的敌人（势不可当）。用 `rng.enemy` 流。
    /// 和滚石/火焰屏障一样**不加力量**（[源码] 里是 `ValueProp.Unpowered`，
    /// 这次不是推断，是源码明写的）。
    DamageRandomEnemy(Amt),
    /// 往玩家**手牌**里塞 `count` 张牌（凋萎存在：每打 6 张牌塞 1 张凋萎）。
    ///
    /// 和 `EOp::AddCardToDiscard` / `AddCardToDraw` 是同一件事的第三个入口，
    /// 三者共用 `step::spawn_card` —— 那里管着一条**不能各写一份**的规则：
    /// 新生成的牌要不要继承场上同名牌的层数（[源码]
    /// `Aeonglass.AfterCardGeneratedForCombat`）。
    AddCardToHand { card: u16, count: i32 },
    /// 把**手上现在这几张**全部升级（风箱 / 骨茶）。
    /// 已经升级过的不动；假升级（`凋萎+N`）那一栏不碰。
    UpgradeHand,
    /// 从**抽牌堆**随机挑 n 张可升级的牌升级（碎石者，n=2）。
    /// [源码] `StoneCracker.AfterRoomEntered`：`Where(IsUpgradable).StableShuffle().Take(n)`。
    UpgradeRandomInDraw(i32),
    /// 从抽牌堆随机挑一张**能力牌**放进手牌，并给它挂上「本回合免费」
    /// （宝石面具）。抽牌堆里没有能力牌就什么都不做。
    ///
    /// [源码] `JeweledMask.BeforeHandDraw`：`SetToFreeThisTurn()` 之后
    /// `CardPileCmd.Add(card, Hand)`。**卡面文本写的是"本场战斗"，
    /// 而源码那个方法名是 `ThisTurn`** —— 照源码，两者只在"留到下回合"时分得开。
    MoveRandomPowerFromDrawToHandFree,
    /// 把自己的层数**置成** n（不是加 n）。凋萎存在的计数器归位用它。
    ///
    /// 和 `GrowSelf` 的区别：那个要知道现在是多少，这个不用。
    /// [源码] `CardsLeft.BaseValue = 6m` 就是一次赋值，照抄。
    SetSelf(i32),
    /// **直接杀死玩家**，和血量无关（沙坑倒计时归零 ⇒ 被吞下）。
    ///
    /// [源码] `SandpitPower.AfterRemoved` 走的是
    /// `CreatureCmd.Kill(allAffectedCreature, force: true)`，而 `force: true`
    /// 的语义源码里明写着 —— **"blocking death prevention by effects like
    /// Fairy in a Bottle"**。所以这条路**不给瓶中精灵机会**，和"血量掉到 0"
    /// 那条死法不是同一条：后者走 `check_over` -> `try_fairy`。
    ///
    /// 这是内核里第二条通往 `player_dead` 的路。第一条在 `check_over` 里，
    /// 判据是 `hp <= 0`；这一条判据是**回合数**。两条不能合并 ——
    /// 合并就得给 `check_over` 编一个假的"血量 0"，那样瓶中精灵会把它救回来。
    KillPlayer,
    /// 敌人持有的能力**打我**（流电：我每打出一张能力牌挨 6 点）。
    ///
    /// [源码] `CreatureCmd.Damage(.., ValueProp.Unpowered | Move, ..)` ——
    /// **走格挡、过难以杀灭/无实体、不是攻击**：不触发 `Attacked`（火焰屏障）、
    /// 不唤醒孤注一掷。和敌人身上的荆棘（`TOp::DamageAttacker`）是同一条路，
    /// 收口在 `step::damage_player_unpowered`。
    DamagePlayer(Amt),
    /// **玩家**失去最大生命（纸伤难愈：每次被打穿 −2）。
    ///
    /// 和卡牌的 `Op::LoseMaxHp` 共用 `step::player_lose_max_hp`：
    /// [源码] `CreatureCmd.LoseMaxHp` 在「当前血 > 新上限」时把差额当伤害扣掉，
    /// 那一下会唤醒百年积木 —— 两条路各写一份迟早长歪。
    PlayerLoseMaxHp(Amt),
    /// 往玩家**弃牌堆**塞 `count` 张牌（剧痛刺击：每段打穿塞 1 张伤口）。
    ///
    /// `count` 是编译期字段、不吃 `Amt`，和 `SummonN` 同一个理由：
    /// 今天唯一的消费者层数恒为 1（[源码] `TestSubject` 那一句 `Apply<PainfulStabsPower>(1)`）。
    /// 层数变了要回来改这一行。
    AddCardToDiscard { card: u16, count: i32 },
    /// 把**玩家**身上某个 status 清零（恶咒 / 抑制的施咒者死了）。
    ///
    /// 和 `OwnerClearStatus` 的区别就是清谁：规则挂在施咒者（敌人）身上，要摘的在我身上。
    PlayerClearStatus(St),
    /// 抑制解除：带 `F_DAMPENED` 的牌升回去（[源码] `DampenPower.AfterRemoved`）。
    RestoreDampenedCards,
    /// 往玩家**抽牌堆随机位置**塞 N 张牌（人体蜂房：挨一段攻击塞层数那么多张晕眩）。
    ///
    /// 和 `EOp::AddCardToDraw` 同一条路（`step::spawn_card` + `State::to_draw_random`，
    /// 插进已知前缀里面时前缀跟着缩）。和 `TOp::AddCardToDiscard` 不同，**张数吃 `Amt`**：
    /// 蜂房的层数会被喷射信息素从 1 涨到 3，写死就只对第一段。
    AddCardToDraw { card: u16, amt: Amt },
}

/// 一条「什么 status 在什么时候做什么」的规则。
///
/// **同一个 status 可以有多条规则**，只要钩子不同 —— 覆甲就是这样：
/// 「回合结束时获得格挡」+「回合开始时减少 1 层」是两条。
/// 不允许的是同一个 `(st, hook)` 出现两次，那是复制粘贴出来的错。
pub struct PowerDef {
    pub st: St,
    pub hook: Hook,
    pub ops: &'static [TOp],
}

/// Enemy move effects.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EOp {
    Attack { base: i32, hits: i32 },
    Block(i32),
    SelfStatus { st: St, amt: i32 },
    /// 给自己加 `amt × (自己 `per` 那个 status 的层数)`。
    ///
    /// 唯一的消费者是巨斧机器人的启动：[源码]
    /// `PowerCmd.Apply<StrengthPower>(..., BootUpStrGain * (2 - StockAmount))`
    /// = `3 × (2 − 库存)`，内核拆成 `SelfStatus{力量,+6}` +
    /// `SelfStatusPerStack{力量, −3, 库存}`。
    ///
    /// **不建它就只能在三具身体里挑一个力量值写死**，而那个数正是
    /// 「越死越强」这条机制本身：第一具 +0、第二具 +3、第三具 +6。
    /// 挑哪个都会在另外两具上系统性错，且第三具那一头是**乐观**的。
    SelfStatusPerStack { st: St, amt: i32, per: St },
    /// 多段攻击，**段数 = `hits` + 自己 `per` 那个 status 的层数**。
    ///
    /// [源码] 实验体的连环爪击：`WithHitCount(BaseMultiClawCount + ExtraMultiClawCount)`，
    /// 而 `ExtraMultiClawCount++` 在攻击**之后** —— 所以这一手要写成
    /// 「先 `AttackPlusStackHits`，再 `SelfStatus{per, +1}`」，顺序不能反。
    /// 3 → 4 → 5 → 6…… 这是阶段 2 的时钟；写死 3 段是**乐观**的。
    AttackPlusStackHits { base: i32, hits: i32, per: St },
    /// 攻击，**每段基础伤害 = `base` + 自己 `per` 那个 status 的层数**（遗忘之物的恐惧）。
    ///
    /// [源码] `TheForgotten.DreadDamage => 13 + Creature.GetPowerAmount<DexterityPower>()`，
    /// 意图是 `SingleAttackIntent(() => DreadDamage)` —— 观测标签里已经含它。
    /// 每次瘴气偷走我 2 点敏捷加给自己，于是恐惧 13 -> 15 -> 17…
    ///
    /// **和 `AttackPlusStackHits` 必须分开**：那个加段数，这个加每段的伤害。
    AttackPlusSelfStatus { base: i32, hits: i32, per: St },
    /// 把自己身上某个 status **清零**（不是减一层）。
    /// 盛碗虫（石）的晕眩用它清掉失衡标记（[源码] `IsOffBalance = false`）。
    /// 和 `SelfStatus { amt: -1 }` 的区别：那个在已经是 0 的时候会变成 -1，
    /// 之后再置位就成了 0 —— 一个只在特定顺序下发作的静默错误。
    /// 给**它自己这一侧所有活着的敌人**加 status，**包含它自己**。
    ///
    /// [源码] 胧光怪的哀嚎：
    /// `PowerCmd.Apply<StrengthPower>(.., GetTeammatesOf(base.Creature), 3m, ..)`，
    /// 而 `GetTeammatesOf(c) => GetCreaturesOnSide(c.Side)` —— **含自己**
    /// （同一个函数已经被组装师那条 `ECond::AlliesAliveAtLeast` 钉过一次：
    /// 第3幕第45层那一帧同时钉死了阈值和"含不含自己"）。
    ///
    /// **和 `SelfStatus` 必须分开**：场上只有它一只时两者结果相同，
    /// 带着幻象时差一份力量 —— 而"带着幻象"正是这只怪的常态。
    /// 写成 `SelfStatus` 是**乐观**的（少算幻象那 16 点撞击的加成）。
    TeamStatus { st: St, amt: i32 },
    ClearSelfStatus(St),
    /// **自己死掉**（[源码] `CreatureCmd.Kill(base.Creature)`）：血量归 0，照常点 `EnemyDied` / `AllyDied`。
    ///
    /// 消费者：瀑布巨兽「爆炸」打完之后自杀。那时它身上已经没有蒸汽喷发（「即将爆发」移除了），
    /// 死亡规则不会再把它拉回来，战斗照常结束。
    /// 气态炸弹的「自爆」[源码] 同样是打完就 `Kill` —— 内核那一手还没接上它，见 roadmap。
    KillSelf,
    /// 往我的**抽牌堆**塞 N 张牌（噪音机器人的第二张眩晕）。
    ///
    /// **和 `AddCardToDiscard` 必须分开**：[源码] `Noisebot.NoiseMove` 是
    /// 两张分别去两个牌堆 ——
    /// `AddGeneratedCardToCombat(card, PileType.Discard)` +
    /// `AddGeneratedCardToCombat(card2, PileType.Draw, CardPilePosition.Random)`。
    /// 两张都塞弃牌堆是错的：进抽牌堆的那张**这一局就可能抽到**，
    /// 进弃牌堆的要等洗牌，对当前回合的影响完全不同。
    ///
        /// 插入位置是随机的，内核按自己的 RNG 放（不变量 4：随机序列故意不一致）。
    AddCardToDraw { card: u16, count: i32 },
    /// 往我的**手牌**塞 N 张牌（机甲骑士的火焰喷射：4 张灼伤）。
    ///
    /// **手牌满 10 张时溢出进弃牌堆，不是丢掉** —— [源码] `CardPileCmd.Add` 里
    /// `isFullHandAdd` 时 `targetPile = Discard`。收口在 `step::add_generated_to_hand`，
    /// 和凋萎存在的 `TOp::AddCardToHand` 共用。
    AddCardToHand { card: u16, count: i32 },
    /// 把**本场所有**指定牌名的实例，伤害各 +`by`（永世沙漏的剧烈增强
    /// 把我持有的每一张凋萎 `FakeUpgrade()` 一次，各 +3）。
    ///
    /// [源码] `IncreasingIntensityMove` 遍历的是
    /// `PlayerCombatState.AllCards` —— **所有牌区**，不只手牌。
    /// 内核对应地扫 `s.cards`（本场全部实例），加在 `CardInst::bonus` 上。
    ///
    /// 新生成的凋萎不用这条 op 补：`step::spawn_card` 会让它和场上同名牌
    /// 层数一致（[源码] `AfterCardGeneratedForCombat` -> `MatchWitherToUpgradeCount`）。
    UpgradeAllCopies { card: u16, by: i32 },
    /// 给场上**每一只非爪牙**敌人 N 点格挡（Guardbot 的护卫）。
    ///
    /// [源码] `Guardbot.GuardMove` 遍历的是 `Enemies.Where(c => c.Monster is Fabricator)`
    /// —— 给每只**组装师** 15 点格挡，**不是给自己**。内核没有"按怪物类型筛选"，
    /// 但在组装师的遭遇里"非爪牙"和"组装师"是同一批（爪牙全是它造的），
    /// 而内核已经靠这个区分做 `no_master_left`。所以这是**换了个等价谓词**，
    /// 不是随手近似 —— 前提写在这儿，遇到反例（非组装师遭遇里出现 Guardbot）要改。
    ///
    /// 格挡是 `ValueProp.Unpowered`。
    BlockNonMinions(i32),
    PlayerStatus { st: St, amt: i32 },
    /// 召唤。新敌人用它自己 `EnemyDef` 的 `start_status`（爪牙标记就在那儿）。
    ///
    /// 实测（2026-08-15 第15层）：雾菇第一手叫出 6 血的利齿之眼。
    /// 召出来的敌人**当回合不行动** —— `enemy_turn` 的循环范围在进入时就定了。
    Summon { def: u16, hp: i32 },
    /// 往我的**弃牌堆**塞 N 张牌（史莱姆那手 `StatusCard`）。
    ///
    /// 实测（2026-08-15 第14层）：树枝史莱姆（中）的 `StatusCard:1` 让牌总数
    /// 14 → 15，多出来的那张**出现在弃牌堆**，是黏液。
    AddCardToDiscard { card: u16, count: i32 },
    /// 从我的**抽牌堆 + 弃牌堆**里拿走 n 张（偷窃草蜢的「偷盗」）。
    ///
    /// [源码] `ThievingHopper.ThieveryMove`：从 `PileType.Draw, PileType.Discard`
    /// 里按优先级挑一张，`CardPileCmd.RemoveFromCombat`。
    ///
    /// **不是消耗**：它走的是"移出战斗"，`CardExhausted` 那一串（无惧疼痛 /
    /// 黑暗之拥）**不该触发**。所以内核这条不许复用 `exhaust_card`。
    StealCard(i32),
    /// 把本场**所有**已升级的牌降级，并记下是哪几张（`F_DAMPENED`）—— 魔法骑士的抑制。
    ///
    /// [源码] `DampenPower.AfterApplied`：`AllCards.Where(c => c.IsUpgraded)` 逐张
    /// `CardCmd.Downgrade`。它挂在"抑制挂上了"之后，所以写成紧跟在
    /// `PlayerStatus{抑制}` 后面的一条 op，**并且只有前一条 `PlayerStatus` 真的落地才发作**
    /// —— 我带人工制品时抑制被吃掉，`AfterApplied` 根本不会被调用。
    DowngradeUpgradedCards,
    /// 敌人给自己回血，封顶在最大生命（知识恶魔的思考：[源码] `CreatureCmd.Heal(Creature, 30 × 玩家数)`）。
    Heal(i32),
    /// **知识的诅咒**：弹一个不能跳过的二选一，第 k 次用 `sets[k]`。
    ///
    /// k 从我身上的诅咒 status 反推（`content::curses_taken`），**选哪边照 `State::curse_policy`
    /// 的第 k 位**：1 = 瓦解、0 = 另一边。为什么是策略参数而不是 `Pending`，见 `State::curse_policy`。
    CurseOfKnowledge(&'static [CurseSet]),
    Nothing,
}

/// 知识的诅咒里的一组二选一（[源码] `KnowledgeDemon._curseOfKnowledgeSets` 的一项 +
/// `_disintegrationDamageValues` 的对应档）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CurseSet {
    /// 选瓦解那一边：挂几层瓦解
    pub disintegration: i32,
    /// 另一边：挂哪个 status、几层
    pub other: St,
    pub other_amt: i32,
    /// 另一边附带的**最大能量变化**（虚脱 −1）。[源码] 是 `ModifyMaxEnergy`，内核照薪火之源
    /// 的做法在选中那一刻落到 `State::base_energy` 上，status 只当印记。
    pub other_max_energy: i32,
}

pub struct EnemyMove {
    pub name: &'static str,
    /// 这一手在**游戏观测里显示的意图类型**，逐字照抄 mod 输出的字符串：
    /// `Attack` / `Defend` / `Buff` / `Debuff` / `DebuffStrong` /
    /// `StatusCard` / `Summon` / `Sleep`。
    ///
    /// 为什么要单独存而不从 `ops` 推：`EOp::Nothing` 推不出任何类型，
    /// 于是"沉睡"和"有一手但我没建模"长得一模一样 ——
    /// `verify --predict-enemy` 当场被这个坑得对不齐了三只敌人。
    ///
    /// **不要照抄 wiki 的 intent 列**，那一列不可靠：它把
    /// `Big Swing`（12 点伤害）标成 Utility、`Expel Blast`（5x2 伤害）标成 Buff。
    pub intent: &'static str,
    pub ops: &'static [EOp],
}

/// 敌人打完一手之后去哪。对应 [源码] `MonsterState` 的三种子类。
///
/// **为什么不把它做成"权重表 + 概率"**：内核的随机流故意和游戏不一致
/// （不变量 4），所以"下一手是哪个"根本预测不了。能预测的是
/// **下一手在哪个集合里** —— `allowed_moves` 给的就是这个集合，
/// `verify --predict-enemy` 也改成判**成员资格**而不是逐字相等。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Next {
    /// 固定接第 n 手（[源码] `MoveState.FollowUpState` 直接指向另一个 MoveState）
    Go(u8),
    /// 加权随机分支（[源码] `RandomBranchState`）
    Rand(&'static [Branch]),
    /// 条件分支（[源码] `ConditionalBranchState`）。
    /// **按表里的顺序取第一个成立的** —— 源码是遍历 `States` 累加权重，
    /// 条件成立算 1、不成立算 0，等价于"第一个成立的"。
    Cond(&'static [(ECond, u8)]),
}

/// 随机分支的一条边。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Branch {
    /// 去第几手
    pub to: u8,
    /// 相对权重。[源码] 是 `Func<float>`，实际用到的只有等权和雾菇的
    /// 0.4 : 0.6 —— 这里取整数比例（2 : 3），省掉浮点也省掉可比较性的麻烦。
    pub weight: u16,
    pub repeat: Repeat,
    /// 冷却：**最近 `cooldown` 手里出现过就不能再出**（权重归零）。
    ///
    /// 这一条我一开始读错过：飞蝇菌子的 `AddBranch(move, 3, CannotRepeat)`
    /// 里那个 3 是 **cooldown 不是权重** —— 重载是
    /// `AddBranch(state, int cooldown, MoveRepeatType)`，权重默认 1。
    pub cooldown: u8,
}

/// [源码] `MoveRepeatType`。`CanRepeatForever` 之外的三种都要看历史。
///
/// `UseOnlyOnce` 一度没建（"不加没有消费者的空钩子"），2026-08-22 蛮兽进表
/// 时补上 —— 它的咆哮就是 `UseOnlyOnce`。**它读的不是 `enemy_hist`**：
/// 定长历史表达不了"这一整场曾经出过"，见 [`Repeat::Once`]。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Repeat {
    Forever,
    /// 上一手就是它 ⇒ 不能再出（`CannotRepeat`）
    NotTwice,
    /// 最近连续 n 手都是它 ⇒ 不能再出（`CanRepeatXTimes(n)`）
    AtMost(u8),
    /// **这一整场只出一次**（[源码] `UseOnlyOnce`）。
    ///
    /// 和上面两种不是一个数据源：定长的 `enemy_hist`（深度 4）表达不了
    /// "曾经"，所以它读 [`crate::state::State::enemy_used`] 那个位掩码。
    /// 消费者：蛮兽的咆哮（易伤 3，一场只咆哮一次）。
    Once,
}

/// 条件分支的条件。**判不出来的一律用 [`ECond::Unknown`]** ——
/// 那会让允许集合退化成"所有分支"，仍然比"猜一个"强：
/// 集合大一点只是弱，猜错是**自信地错**。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ECond {
    /// 场上只有它一只（小啃兽 `IsAlone`，[源码] 里由**遭遇**在开局设定：
    /// `NibbitsWeak` 设 `IsAlone = true`）。内核按"开局只有一只非爪牙"近似。
    Alone,
    /// 它是最前面那只（小啃兽 `IsFront`，同样由遭遇设定）。
    /// 内核用**槽位号最小的活着的敌人**近似。
    Front,
    NotFront,
    /// 盛碗虫（石）的失衡。语义是 [源码] `ImbalancedPower`：
    /// **它的攻击被完全挡住** ⇒ 下一手晕眩。
    OffBalance,
    NotOffBalance,
    /// 它自己的血量 ≤ n（仪式兽的耕地闸门 150）。
    ///
    /// **为什么闸门要在机器里再表达一次**：真正的规则是 `PlowPower` 这个
    /// 触发器（`Hook::EnemyDamaged`），而 `verify --predict-enemy` 那条路径
    /// **不重放我的出牌**，触发器在那里根本不会发作 —— 于是内核会一直预测
    /// "耕地"，实测却是眩晕，报「观测不在允许集合里」。
    ///
    /// 在预测层面这个转移本来就是血量条件的，所以这不是打补丁：
    /// 「血量 ≤ 150 ⇒ 下一手是眩晕」是真命题。闸门开过之后耕地不再出现，
    /// 第二阶段那几手的后继都是 `Go`，不受这条影响。
    HpAtMost(i32),
    /// 它站在第 n 格（外骨骼虫的开场手按站位分：first/second/third/fourth
    /// 各出一手不同的招，[源码] `Creature.SlotName`）。
    ///
    /// 内核没有"站位名"，用**槽位号**近似 —— 而槽位号就是观测里的出场顺序，
    /// 和 `ECond::Front` 用的是同一个近似。2026-08-22 第2幕第20层实测三只：
    /// 槽0 疾走 / 槽1 大颚 / 槽2 激怒，和源码的 first/second/third 逐个对上。
    /// **第四格没见过**（那一格源码走随机分支），所以那条边仍然是未验证的。
    SlotIs(u8),
    /// **起手的代表元提示**：这一支在槽位 n 上是内核挑的那一个，但**不算确定成立**。
    ///
    /// 给「遭遇掷一个随机数、几只同类按槽位错开起手」那种开局用（残杀千足虫：
    /// [源码] `DecimillipedeElite` 给三节 `num / num+1 / num+2`，`num = Rng.NextInt(3)`）。
    /// 那种开局有两个问题，答案不一样，这一个条件分开回答：
    /// * **单看一只，第一手可能是什么**（`step::allowed_initial`，`synth_audit` 的开局第一手）：
    ///   哪一手都可能 ⇒ 求值一律「判不了」，允许集合是所有分支
    /// * **合成一场仗时挑哪一个**（`step::initial_move`）：几只必须错开 ⇒ 按槽位挑，
    ///   等于把遭遇的随机数固定成 0
    ///
    /// 和 [`ECond::SlotIs`] 的差别就在第一问：`SlotIs` 会把「随机数固定成 0」当成事实报出去，
    /// 真实录像上随机数不是 0 的那些场次，每一只都报集合外。
    /// **只在 `Machine::start` 里有意义** —— 放进 `after` 的话 `pick_next` 取最低位，提示被静默忽略。
    SlotRep(u8),
    /// 场上**活着的敌人（含它自己）**至少 n 只。
    ///
    /// [源码] 组装师的 `CanFabricate` 是
    /// `GetTeammatesOf(Creature).Count(c => c.IsAlive) < 4`，
    /// 而 `GetTeammatesOf(c) => GetCreaturesOnSide(c.Side)` —— **包含自己**。
    /// 所以"造不了了"等价于 `AlliesAliveAtLeast(4)`。
    ///
    /// [实测] 第3幕第45层第3回合：场上 4 只（组装师 + 2 噪音 + 1 电击），
    /// 意图是 `Attack:11`（分解，那一手只有造不了的时候才出）——
    /// 如果 `GetTeammatesOf` 不含自己，那时是 3 只、该继续造，和观测矛盾。
    /// 这一帧同时钉死了阈值和"含不含自己"。
    AlliesAliveAtLeast(u8),
    /// 它自己的**最大**生命 ≤ n。实验体用它当阶段指示器
    /// （三个形态 100 / 200 / 300，复苏一次它自己就变了），
    /// 见 `TCond::OwnerMaxHpAtMost` 那条更长的说明。
    MaxHpAtMost(i32),
    /// **这一整场还没出过第 n 手**（[源码] 里那种 `private bool _hasXxxed`）。
    ///
    /// 读的是 [`crate::state::State::enemy_used`] 那个位掩码 ——
    /// 和 [`Repeat::Once`] 同一个数据源，不是新开一份状态。
    ///
    /// 消费者：青蛙骑士的 `HasBeetleCharged`（甲虫冲锋一场只冲一次）。
    MoveUnused(u8),
    /// 逻辑与。三值：有一条**确定不成立**就是不成立；全部确定成立才成立；
    /// 否则（有判不了的、又没有确定不成立的）返回"判不了"。
    ///
    /// 加它是因为 [源码] 的条件本来就是复合的
    ///（青蛙骑士：`!HasBeetleCharged && CurrentHp < MaxHp / 2`），
    /// 而 `Next::Cond` 一支只放得下一个 `ECond`。
    /// **拆成两支是错的** —— 那表达的是"或"。
    All(&'static [ECond]),
    /// 知识的诅咒**已经落下过**不到 n 次 / 至少 n 次（知识恶魔思考之后的分支）。
    ///
    /// [源码] 读的是私有的 `_curseOfKnowledgeCounter`。观测里没有它，内核也不另存一份：
    /// 从我身上的诅咒 status 反推（`content::curses_taken`），于是同步进来的局面自己带着答案。
    CursesTakenBelow(&'static [CurseSet], u8),
    CursesTakenAtLeast(&'static [CurseSet], u8),
    /// 它自己身上某个 status 的层数 ≥ n / < n（蜂群术士的喷射信息素、熟睡甲虫的打鼾）。
    ///
    /// **判的是内核推进指针那一刻的层数**（`step::advance_move`，出完招立刻判），而 [源码] 的
    /// `RollMove` 在**我方回合开始**才掷（`CombatManager.StartTurn` -> `PrepareForNextTurn`）。
    /// 两者之间隔着敌人的回合末钩子 —— 那几个钩子会改这个 status 时，阈值要按「掷的那一刻」
    /// 换算，写在消费者自己的机器上（熟睡甲虫就是这样，见 `M_SLUMBERING_BEETLE`）。
    SelfStatusAtLeast(St, i32),
    SelfStatusBelow(St, i32),
    /// 内核判不了 —— 允许集合退化成所有分支
    Unknown,
}

/// 一台出招机器。没有它就是 `loop_from` 那套固定循环。
pub struct Machine {
    /// 开局第一手怎么定（[源码] `MonsterMoveStateMachine` 的 initialState）
    pub start: Next,
    /// 第 i 手打完之后去哪。下标和 `EnemyDef::moves` 对齐。
    pub after: &'static [Next],
}

pub struct EnemyDef {
    pub name: &'static str,
    pub max_hp: i32,
    /// Statuses applied at combat start (难以杀灭, 激怒, ...).
    pub start_status: &'static [(St, i32)],
    /// 按顺序出招。走到头之后回到 [`EnemyDef::loop_from`] 而不是 0。
    pub moves: &'static [EnemyMove],
    /// 出招走到末尾后**回到第几手**。
    ///
    /// 真实出招表大量是"开头几手固定，之后只循环后半段"：
    /// 旧日雕像 `沉睡 → 苏醒 → 挥砍 → 挥砍 → …`（`loop_from = 2`），
    /// 缩小甲虫 `缩小 → 啃咬 → 踩踏 → 啃咬 → 踩踏`（`loop_from = 1`）。
    /// 一律 `% len` 会把开场手当成循环的一部分，那是错的。
    pub loop_from: u8,
    /// 出招机器。`None` = 用上面那套固定循环。
    ///
    /// **两者不是等价的表达方式，是两种可信度**：固定循环是从实录反推的，
    /// 机器是从 [源码] 的状态机照抄的。30 只还在用循环，是因为源码确认了
    /// 它们本来就是确定性的（121 只里 82 只纯确定性）—— 那种情况下循环表
    /// 就是对的结构，没必要动。
    ///
    /// 有机器的时候，`enemy_move[e]` 存的是**当前手的下标**；
    /// 没有的时候存的是**出招次数**。这两个含义不同，
    /// 一律走 `current_move_ix` 取，别直接读那个字段。
    pub machine: Option<&'static Machine>,
}

/// 第 `counter` 次出招该用哪一手。见 [`EnemyDef::loop_from`]。
#[inline]
pub fn move_index(def: &EnemyDef, counter: u32) -> usize {
    let n = def.moves.len();
    if n == 0 {
        return 0;
    }
    let i = counter as usize;
    if i < n {
        return i;
    }
    let loop_len = n - def.loop_from as usize;
    if loop_len == 0 {
        return n - 1;
    }
    def.loop_from as usize + (i - def.loop_from as usize) % loop_len
}

// ---------------------------------------------------------------------------
// 药水系统
// ---------------------------------------------------------------------------

pub struct PotionDef {
    pub name: &'static str,
    pub targeted: bool,
    pub ops: &'static [Op],
}

// ---------------------------------------------------------------------------
// 遗物
// ---------------------------------------------------------------------------
//
// **遗物就是开局挂上的 status。** 不发明新范式：能力牌已经是"带层数的 status +
// `POWERS` 里一条规则"，遗物只是"开局自带、不占牌"的那个版本。于是
// 「加一个遗物 = 加一行表」，`fire()` 照样不认识任何具体遗物。
//
// 分四类，各回各家（2026-08-17 按这一局身上的 6 个真实遗物归纳）：
//
// | 类 | 例子 | 落到哪 |
// |---|---|---|
// | 触发式 | 燃烧之血、奥利哈钢 | `POWERS`（本表只负责把 status 挂上）|
// | 规则修饰 | 臂甲（首次卡牌格挡翻倍）| `step.rs` 的窄 `if` |
// | 结构性 | 药水腰带（+2 药水栏位）| `State` 容量，不是 status |
// | 局外 | 白银熔炉、佩尔之翼 | **不进 L1** —— 不属于战斗层 |

/// 附魔的 `Amount` 怎么变成一个数值。**只有这三种形状**，别再加第四种没有
/// 真实附魔在用的 —— 和「不许有空钩子」同一条规矩。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EnchVal {
    /// 这个钩子这个附魔不改（`EnchantXxxAdditive` 的默认实现就是返回 0）
    Zero,
    /// `=> Amount`（灵巧、锋利、旺盛）
    Amount,
    /// `=> Amount - 1`（黏糊糊：第一次打出时 `Amount` 是 1 ⇒ 加 0）
    AmountMinus1,
    /// 数值写死在附魔自己的 `DynamicVar` 上、和 `Amount` 无关
    /// （墨迹 +1 伤害、特兹卡塔拉的余烬 +3 伤害）
    Fixed(i32),
}

impl EnchVal {
    #[inline]
    pub fn eval(self, amount: i32) -> i32 {
        match self {
            EnchVal::Zero => 0,
            EnchVal::Amount => amount,
            EnchVal::AmountMinus1 => amount - 1,
            EnchVal::Fixed(v) => v,
        }
    }
}

/// 一种**附魔**（[源码] `EnchantmentModel`）。一张牌至多带一个，挂在**卡实例**上
/// （`CardInst::ench` / `ench_amt`），所以牌组里三张打击可以只有一张带。
///
/// # 这张表的形状 = `EnchantmentModel` 的虚方法表
///
/// 源码里附魔**只有六个口子**能改游戏：`EnchantBlockAdditive` /
/// `EnchantBlockMultiplicative` / `EnchantDamageAdditive` /
/// `EnchantDamageMultiplicative` / `EnchantPlayCount`，外加 `OnEnchant`（改关键字
/// 和费用）和 `OnPlay`（打出时多做一件事）。
///
/// **这张表只装内核今天真的消费的那几个**（格挡加、伤害加、伤害乘、关键字）。
/// 其余的口子**没有字段** —— 加一个没人读的字段和加一个空钩子是同一种错：
/// 它看起来像建好了。装不下的附魔一律 `modelled: false`，
/// `replay` 会把它们点名进 `Report::unknown_enchantments`。
///
/// # `modelled: false` 不等于"没效果"
///
/// 它等于"内核会**算错**这张牌，而且知道自己在算错"。`--live` 把它们列出来，
/// 对拍不会静默放过。
pub struct EnchantDef {
    /// 游戏 id（大写下划线），和观测里 `enchantment.id` 逐字对齐
    pub id: &'static str,
    pub name: &'static str,
    /// `EnchantBlockAdditive`：加在**卡面基础格挡**上，在敏捷/脆弱/臂甲之前
    pub block_add: EnchVal,
    /// `EnchantDamageAdditive`：和锋利同一档，加在基础值上、在力量之前
    pub damage_add: EnchVal,
    /// `EnchantDamageMultiplicative`：`(分子, 分母)`，`(1, 1)` = 不乘。
    /// **作用在基础值上、在力量之前**（源码里它"先于所有其它伤害钩子"跑）
    pub damage_mul: (i32, i32),
    /// `OnEnchant` 给这张牌永久加的关键字（`F_INNATE` / `F_RETAIN`）
    pub keywords: u8,
    /// 内核是否**完整**建了这一个（差一个口子就是 false）
    pub modelled: bool,
    /// 没建的那部分卡在哪。建全了的写依据
    pub note: &'static str,
}

/// 一个遗物。`start_status` 是它在**战斗开始时**给玩家挂的 status，
/// 规则本身写在 `content::POWERS` 里（和能力牌同一套机器）。
///
/// `start_status` 为空**不等于**没效果，可能是：
/// * 局外遗物（白银熔炉）—— 本来就不该进战斗层，`modelled` 为 true
/// * 还没建模 —— `modelled` 为 false，`--live` 会把它点名报出来
///
/// 这个区分很实际：内核**必须能说出"我不认识这个遗物"**，
/// 否则它连"我可能算错了"都讲不出来 —— 那正是遗物今天的处境。
pub struct RelicDef {
    /// 游戏内部 id，如 `BURNING_BLOOD`
    pub id: &'static str,
    pub name: &'static str,
    /// 战斗开始时挂上的、**游戏也会报**的 status（金刚杵的力量、护喉甲的覆甲）。
    ///
    /// **对拍路径上不许用它，也不需要用它** —— 观测里本来就有这些量。
    /// 早先这一栏和 `private_status` 是同一个字段，一起进 `relic_carry`
    /// 每帧 `set` 回去；那样加一件金刚杵就会**把力量钉死在 1**，
    /// 药水/撕裂/内脏撕裂加的力量全部被覆盖掉。
    ///
    /// 它的用处在**内核自己开一场仗**的时候（L3 的推演、合成 trace），
    /// 那时没有观测可抄。`relic_start_status_is_observable` 守着"这一栏里的
    /// status 必须是观测能映射到的"。
    pub start_status: &'static [(St, i32)],
    /// 内核**私有**的记账，游戏根本不报（臂甲的 `VambraceCharge`）。
    ///
    /// 和上面那栏的处置**正好相反**：必须由 `relic_carry` 逐帧带着走
    /// （游戏不报 ⇒ 每帧重新初始化的话，内核会以为臂甲每帧都还能翻倍），
    /// 而且**不许出现在 `replay::ALL_ST`** —— 进去了对拍会每帧报一个
    /// 游戏里根本不存在的 status。两个测试各守一半。
    pub private_status: &'static [(St, i32)],
    /// 这件遗物**面板上那个计数器**该灌进哪个 status。
    ///
    /// 有些遗物的状态**跨战斗保留**（摆动球的 `TurnsSeen` 带 `[SavedProperty]`），
    /// 战斗开始时它不是 0，而是上一场留下的值。假设它从 0 开始就会**系统性
    /// 错相位** —— 那是"自信地算错"，比不建模更糟。
    ///
    /// 好在游戏把它显示在遗物上（`ShowCounter` / `DisplayAmount`），
    /// 录制器一直就把 `counter` 写进 trace 了。这里只是把它接上。
    pub counter_to: Option<St>,
    /// 战斗层的行为**是否已经建模到位**（局外遗物也算 true —— 它们不欠战斗层什么）
    pub modelled: bool,
    /// 为什么没建模 / 建模依据。空串表示不需要说明
    pub note: &'static str,
}

/// 一段 `Op` 是**从哪儿发出来的**。
///
/// 药水复用 `Op` 词汇表（加一瓶 = 加一行表），但**药水不是牌**，有三条规则
/// 因此不同。把区别收进这一个枚举，好过让药水另开一个 `Op` 解释器 ——
/// 那样 `Op::Block` 会在两处有两个含义，而没有任何测试守着它们不长歪。
///
/// | 差异 | 依据 |
/// |---|---|
/// | 格挡不过 `card_block`（不吃脆弱） | **实测**：`act1_f14` 帧4，带脆弱2 喝格挡药水仍得满 12 |
/// | 伤害不吃力量/腐化/锋利 | 推断（初代如此），**未实测** |
/// | 给易伤**不触发** `ApplyVuln`（凶恶不抽牌） | **玩家判定**（2026-08-16）：卡面推不出来，问过 |
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Card,
    Potion,
}

/// 药水表。**来源档次照本仓库的规矩逐条标注**（`[实测]`/`[游戏]`/`[源码]`/`[推断]`）。
///
/// # 2026-08-21：`[推断]` 全部清空了
///
/// 以前这里有 6 瓶标着 `[推断]`，数值照初代记忆填 —— 而这个仓库已经被初代
/// 记忆坑过一次（痛击基础值 + 5 张牌的 `ops_upg`）。现在有两个真来源：
///
/// * **`traces/potions_catalog.json`**（`tools/dump_potions.py` 导的）——
///   id 来自 compendium 的 `potion_lab`，**文本来自游戏自己**
///   （`player.potions[].description` / `rewards.items[].potion_description`）。
///   **wiki 端点给不了药水**：它的 scope 写死是
///   `active_profile_discovered_cards_and_relics`。
/// * **`decompiled/.../Potions/*.cs`** —— `[源码]` 那一档。
///
/// **两个来源在重合的 7 瓶上逐字一致（7/7）**：格挡 12 / 能量 2 / 再生 5 /
/// 迅捷抽 3 / 复制 1 / 赌徒特酿 / 技能药水。这个 7/7 就是"其余的敢照源码填"
/// 的依据 —— 但它**不是实测**：反编译器不是零误差，读代码也会读错
/// （本仓库有过按 grep `OnPlay` 分档、5 张能力牌全判错的案例）。
///
/// **对了一个真错**：虚弱药水源码是 `WeakPower 3`，表里原来写的是 2 ——
/// 那正是照初代记忆填的那一档，也正是这次翻表要找的东西。
///
/// # 已发现 40 瓶（2026-08-27 重导），这里建了 22 瓶，**剩下的是故意不建的**
///
/// 「增量建模」不是偷懒，是本仓库的教训（灌了 54 张牌之后，未验证的内容变成
/// 已验证的三倍）。判据只有一条：**机制内核已经有的才进表**。
/// 缺的那些各自欠什么，列在这里，免得下一个人再查一遍源码
/// （**表里只列已经查过源码的那 12 瓶**；`potions_catalog.json` 里另有 21 瓶
/// 连游戏原文都还没捡到，那些一个字都没查）：
///
/// | 药水 | [源码] 干什么 | 欠什么 |
/// |---|---|---|
/// | 无色药水 | 生成 3 张随机牌 -> 选 1 -> 本回合免费 | 「从生成的候选里选」这种 `Pending`。`F_FREE_THIS_TURN` 已经有了。**攻击/技能/能力三瓶已按保守近似建了**（随机给 1 张，不给挑选收益），见 `Op::GenerateFree` |
/// | 赌徒特酿 | 丢弃任意张，再抽同样多 | 张数不定的 `Pending` |
/// | 复制药水 | `DuplicationPower` 1：下一张牌多打一次 | 「改这张牌打几次」这类修饰器 |
/// | 稳定血清 | `RetainHandPower` 2：保留手牌 | 保留(retain)：flag 位有了，`step` 还没用 |
/// | 蒸馏混沌 | 自动打出抽牌堆顶 3 张 | 自动出牌 |
/// | 幸运补药 | `BufferPower` 1：挡掉下一次失去生命 | 「抵消一次伤害」的修饰器 |
/// | 光耀酊剂 | +1 能量，且 `RadiancePower` 3 每回合再 +1 | 一个新 `PowerDef`（能建，但这一轮不建，见增量规矩）|
/// | 清晰 | 抽 1，且 `ClarityPower` 3 每回合多抽 1 | 「改每回合抽几张」的修饰器 |
/// | 巨化药水 | `GigantificationPower` 1：改下一次攻击 | 数值修饰器（和遗物审计里那族同一个洞）|
/// | 铁匠祝福 | 升级手上所有能升的牌 | 「升级整只手牌」的 `Op`（`Pending::UpgradeInHand` 是选着升）|
/// | 强化药剂 | 格挡翻倍（`Block * 2`）| 「读当前格挡再加」的 `Op` |
/// | 恶臭药水 | 12 点全体 + 100 金，**局外也能喝** | 金币是局外的，语义混着，不碰 |
///
/// 内容表里没有的药水一律落到 `potion::UNKNOWN`：`legal_actions` 不生成它、
/// 求解器也不假装它有效果。**拒绝比瞎猜好。**
///
/// 想把一瓶从 `[源码]` 升成 `[实测]`：实战里喝一次，录进 trace，`verify` 会自动比。
/// 喝药水那一帧现在**是参与对拍的**（2026-08-16 接上的）。
pub const POTIONS: &[PotionDef] = &[
    // 0: 占位。`potion::NONE` = 空槽
    PotionDef { name: "无", targeted: false, ops: &[] },
    // 1: [实测] 格挡药水 —— `act1_f14` 帧4：**带脆弱 2 时仍然给满 12**
    //    （同局面下防御只给 3）。这是 `Source::Potion` 不走 `card_block` 的依据。
    PotionDef { name: "格挡药水", targeted: false, ops: &[Op::Block { base: 12 }] },
    // 2: [源码] 力量药水 +2（`StrengthPotion` = `PowerVar<StrengthPower>(2)`）。未实测
    PotionDef { name: "力量药水", targeted: false, ops: &[Op::Status { tgt: Tgt::Me, st: St::Strength, amt: 2 }] },
    // 3: [源码] 敏捷药水 +2（`DexterityPotion` = `PowerVar<DexterityPower>(2)`）。未实测
    PotionDef { name: "敏捷药水", targeted: false, ops: &[Op::Status { tgt: Tgt::Me, st: St::Dexterity, amt: 2 }] },
    // 4: [实测] 能量药水 +2 —— `act1_f13` 帧0：能量 3/3 -> 5/3
    PotionDef { name: "能量药水", targeted: false, ops: &[Op::GainEnergy(2)] },
    // 5: [源码] 火焰药水 20 点（`FirePotion` = `DamageVar(20, Unpowered)`）。**未实测**。
    //    `Unpowered` 这个 prop 正好印证了第二条判定：它**不吃力量**
    //    （走 `Source::Potion`，不过 `card_face_damage`）—— 但两条都还没被实战比过。
    PotionDef { name: "火焰药水", targeted: true, ops: &[Op::Damage { base: 20, hits: 1, scale: Scale::None }] },
    // 6: [源码] 易伤药水 3 层（`VulnerablePotion` = `PowerVar<VulnerablePower>(3)`）。未实测。
    //    另有一条**玩家判定**（2026-08-16）：它**不触发凶恶**，见 `Source`
    PotionDef { name: "易伤药水", targeted: true, ops: &[Op::Status { tgt: Tgt::Enemy, st: St::Vulnerable, amt: 3 }] },
    // 7: [游戏+源码] 虚弱药水 **3** 层。源码 `PowerVar<WeakPower>(3)`，
    //    **游戏原文当场证实**：2026-08-21 新开一局，第 1 层奖励屏上写着
    //    「虚弱药水 - 给予3层虚弱。」（`traces/potions_catalog.json`）。
    //    **原来写的是 2，错的** —— 那是照初代记忆填的。
    //    这就是"照初代记忆填"为什么危险：它不会报错，只会一直算少 1 层。
    PotionDef { name: "虚弱药水", targeted: true, ops: &[Op::Status { tgt: Tgt::Enemy, st: St::Weak, amt: 3 }] },
    // 8: [游戏+源码] 迅捷药水 抽 3 张。游戏原文「抽3张牌。」，源码 `CardsVar(3)`
    PotionDef { name: "迅捷药水", targeted: false, ops: &[Op::Draw(3)] },
    // 9: [实测] 再生药水 5 层 —— `act1_f17` 帧16：喝完 `REGEN_POWER=5`
    PotionDef { name: "再生药水", targeted: false, ops: &[Op::Status { tgt: Tgt::Me, st: St::Regen, amt: 5 }] },
    // ---- 2026-08-21 增量补的四瓶：**机制内核已经有的，一个新 Op 都没加** ----
    // 10: [游戏+源码] 爆炸安瓿 对**所有**敌人 10 点。源码 `TargetType.AllEnemies`
    //     + `DamageVar(10, Unpowered)`，游戏原文「对所有敌人造成10点伤害。」
    //     （2026-08-21 第 3 层奖励屏实拿到手）
    PotionDef { name: "爆炸安瓿", targeted: false, ops: &[Op::DamageAll { base: 10, hits: 1, scale: Scale::None }] },
    // 11: [源码] 铁心 覆甲 7（`HeartOfIron` = `PowerVar<PlatingPower>(7)`）。未实测。
    //     `PlatingPower` 就是覆甲，内核已经有（`St::PlatedArmor` + `POWERS` 里那两条）
    PotionDef { name: "铁心", targeted: false, ops: &[Op::Status { tgt: Tgt::Me, st: St::PlatedArmor, amt: 7 }] },
    // 12: [源码] 鱼油 力量 1 + 敏捷 1（`FyshOil`，两个 `PowerVar` 各 1）。未实测
    PotionDef {
        name: "鱼油",
        targeted: false,
        ops: &[
            Op::Status { tgt: Tgt::Me, st: St::Strength, amt: 1 },
            Op::Status { tgt: Tgt::Me, st: St::Dexterity, amt: 1 },
        ],
    },
    // 13: [源码] 屈伸药剂 **本回合**力量 5。未实测。
    //     源码挂的是 `FlexPotionPower : TemporaryStrengthPower` —— **不是普通力量**，
    //     所以走 `St::TempStrength`（和预备打击同一条路，回合结束 `strip_temp_strength`
    //     减回去）。看成普通力量会让它跨回合，那是个凭空多出来的收益。
    //     **也正因为它只管这一回合，它不是跨回合药水**，L2 该给它定价而不是拒绝评分。
    //     **两条 op，不是一条**：`St::TempStrength` 只是记账用的标记，
    //     真正的力量是另一条 `St::Strength`（`strip_temp_strength` 回合末按标记减回去）。
    //     只写标记那一条的话，这瓶药水这回合给 0 力量、回合末还倒扣 5 —— 写错过一次。
    PotionDef {
        name: "屈伸药剂",
        targeted: false,
        ops: &[
            Op::Status { tgt: Tgt::Me, st: St::Strength, amt: 5 },
            Op::Status { tgt: Tgt::Me, st: St::TempStrength, amt: 5 },
        ],
    },
    // 14: [源码] 瓶中精灵。**ops 为空是有意的** —— 它 `PotionUsage.Automatic`，
    //     根本走不到"喝下去"这条路。效果在 `step::try_fairy`（将要死时发作）。
    PotionDef { name: "瓶中精灵", targeted: false, ops: &[] },
    // 15: [源码] 攻击药水 `AttackPotion` —— 生成 3 张**不重复的攻击牌**让你挑 1 张，
    //     挑中的那张本回合免费。
    //     **保守近似**：内核建成"随机给 1 张攻击牌、本回合免费"，因为
    //     "从生成的候选里选"需要一种内核还没有的 `Pending`。
    //     近似的方向是**不给挑选收益 ⇒ 低估自己**，理由见 `Op::GenerateFree`。
    PotionDef { name: "攻击药水", targeted: false, ops: &[Op::GenerateFree(Kind::Attack)] },
    // 16: [源码] 技能药水 `SkillPotion` —— 同上，类型换成技能。
    //     它就是 L2 验收里那个"跳过 1"的来源。**近似之后它不再被跳过**，
    //     但那意味着"按一个低估的版本参与比较"，不等于"验过了"。
    PotionDef { name: "技能药水", targeted: false, ops: &[Op::GenerateFree(Kind::Skill)] },
    // 17: [源码+实测] 欧洛巴斯之酸 `OrobicAcid` —— 攻击/技能/能力**各生成 1 张，
    //     全部本回合免费，没有选择**。
    //     **三瓶里唯一不欠任何机制的**，因为它不用挑。
    //     [实测] 第2幕第33层：给了 熔融之拳 / 破灭 / 恶魔形态，三张都显示 0 费。
    PotionDef {
        name: "欧洛巴斯之酸",
        targeted: false,
        ops: &[
            Op::GenerateFree(Kind::Attack),
            Op::GenerateFree(Kind::Skill),
            Op::GenerateFree(Kind::Power),
        ],
    },
    // 18: [源码] 异蛇之油 `SneckoOil` —— 抽 7 张（`CardsVar(7)`），
    //     然后把**手牌里**每张牌的费用随机成 0..3（`Rng.CombatEnergyCosts.NextInt(4)`）。
    //     **两条 op 的顺序是源码的顺序**：先抽再随机，所以抽上来的那 7 张也吃随机。
    //     写反了的话新抽的牌保持原价 —— 一个只在这瓶药水上发作的静默错误。
    //     具体随机成几费是内核 RNG 的一个样本（不变量 4），见 `Op::RandomizeHandCosts`。
    PotionDef {
        name: "异蛇之油",
        targeted: false,
        ops: &[Op::Draw(7), Op::RandomizeHandCosts { max: 4 }],
    },
    // 19: [源码] 药水形状的石头 `PotionShapedRock` —— 石化蟾蜍每场战斗开局塞的那瓶。
    //     `TargetType.AnyEnemy` + `DamageVar(15m, ValueProp.Unpowered)`，
    //     `OnUse` 就一句 `CreatureCmd.Damage(target, 15, ...)`。
    //     **Unpowered 是关键**：走 `Source::Potion` 那条路 ⇒ 不吃力量/腐化/锋利/活力，
    //     也不吃易伤/虚弱/缓慢/缩小/巨像，只吃难以杀灭和无实体。
    //     和爆炸安瓿同构，只是单体、15 点。**一个新机制都不欠。**
    PotionDef {
        name: "药水形状的石头",
        targeted: true,
        ops: &[Op::Damage { base: 15, hits: 1, scale: Scale::None }],
    },
    // 20: [源码] 能力药水 `PowerPotion` —— 生成 3 张**不重复的能力牌**让你挑 1 张，
    //     挑中的那张本回合免费。和攻击/技能药水**逐字同构**，
    //     所以走同一条保守近似（随机给 1 张、不给挑选收益 ⇒ 低估自己）。
    //     理由在 `Op::GenerateFree` 的注释里，这里不重复。
    //
    //     它是 2026-08-23 第 3 幕 Boss 那条实录里唯一一瓶
    //     **表里连名字都没有**的药水（帧 11 报 `UNKNOWN`）。
    PotionDef { name: "能力药水", targeted: false, ops: &[Op::GenerateFree(Kind::Power)] },
    // 21: [游戏+源码] 瓶装潜能。游戏原文「将你的所有牌洗入你的抽牌堆。抽5张牌。」
    //     （2026-08-27 从 `player.potions[]` 捡到，`traces/potions_catalog.json`）。
    //     [源码] `BottledPotential.OnUse`：`CardPileCmd.Add(手牌 -> Draw)` ->
    //     `CardPileCmd.Shuffle` -> `Draw(5)`，而那个 `Shuffle` **连弃牌堆一起洗**。
    //     所以"你的所有牌"是字面意思，见 `State::shuffle_all_into_draw`。
    //     **它的目标是 `AnyPlayer`（单人局里就是自己），不是敌人** ⇒ targeted: false。
    PotionDef {
        name: "瓶装潜能",
        targeted: false,
        ops: &[Op::ShuffleAllIntoDraw, Op::Draw(5)],
    },
    // 22: [源码] 明耀酊剂 `RadiantTincture`：`GainEnergy(1)` + `RadiancePower 3`。
    //     那 3 层的规则在 `content::POWERS`（回合开始 +1 能量并掉一层）。
    //     **2026-08-27 从"故意不建"那张表里拿出来建了** —— 当时欠的就是那个
    //     `PowerDef`，而娇弱/领地意识这一批已经把同型的钩子铺好了。
    //     **接在表尾**：这些下标就是 `state::potion` 里的常数。
    PotionDef {
        name: "明耀酊剂",
        targeted: false,
        ops: &[Op::GainEnergy(1), Op::Status { tgt: Tgt::Me, st: St::Radiance, amt: 3 }],
    },
    // 23 [源码+实测] CureAll: energy first, then draw two.
    PotionDef { name: "痊愈药水", targeted: false, ops: &[Op::GainEnergy(1), Op::Draw(2)] },
];

/// **喝不了的药水**（[源码] `PotionUsage.Automatic`）。
///
/// 做成一张可枚举的小表而不是给 `PotionDef` 加字段 —— 和 `RULE_MODIFIERS`
/// 同一个理由：目前只有一条，而它需要被三个地方问到
///（`legal_actions` 不许生成动作、L2 的定价不许给它标价、`--live` 要区别显示）。
/// 多起来的话该给它们一张带行为的表。
pub static AUTOMATIC_POTIONS: &[u8] = &[crate::state::potion::FAIRY];

#[inline]
pub fn potion_is_automatic(id: u8) -> bool {
    AUTOMATIC_POTIONS.contains(&id)
}

pub fn potion_def(id: u8) -> &'static PotionDef {
    if (id as usize) < POTIONS.len() {
        &POTIONS[id as usize]
    } else {
        &POTIONS[0]
    }
}
