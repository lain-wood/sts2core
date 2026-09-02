//! Flat, copyable game state. No heap, no pointers into the state.
//!
//! Everything is fixed-size arrays + integers so that cloning a state is a
//! `memcpy` and the whole thing stays resident in L1/L2 cache during search.

pub const MAX_CARDS: usize = 128;
/// **手牌上限 10 张**。[实测] `act2_f33_boss_crusher_retry` 帧13：手牌 9 张时
/// 打出 0 费的战斗专注（离手后剩 8）抽 3 张，游戏只进来 2 张就停在 10，
/// 而抽牌堆 6 -> 4 —— **第 3 张没被抽走，留在抽牌堆里**，不是抽出来再丢掉。
/// 全语料 1017 帧里手牌从没超过 10，触到 10 有 5 次。
///
/// 这条以前压根不存在：`draw_one` 拿 `MAX_CARDS`（128，那是数组容量）
/// 当手牌上限用，于是内核会抽出第 11 张。**这不是"多抽一张"这么轻**——
/// 手牌满时抽牌是空操作，意味着"先打一张腾出位置再抽"和"直接抽"不等价，
/// L2 的出牌顺序会因此变。
pub const MAX_HAND: usize = 10;
pub const MAX_ENEMIES: usize = 5;
/// 每只敌人记多少手历史。见 [`State::enemy_hist`]。
pub const ENEMY_HIST: usize = 4;
/// status 槽位数。**能力牌就是 status**（见 `ops.rs` 的 `PowerDef`），所以每加
/// 一张能力牌就占一个槽 —— 这个数会跟着内容表一起长。16 个在加了 6 张能力牌
/// 之后就满了，直接给到 32；代价是每个 Entity 多 64 字节，`state_is_a_value`
/// 那条 4 KB 上限守着它。
///
/// **64 → 80（2026-08-22）**：第 2 幕两只精英一次带来 5 个新 status
/// （人体蜂房 / 孵化 / 污染 / 活力火花 / 黑暗镣铐），加上两件新遗物的私有量，
/// 64 个槽当场溢出（）。
/// 扩容的代价照上一次（48 → 64）的做法**实测记在 CLAUDE.md 的「当前状态」里**，
/// 别凭感觉估 —— 上次实测是 −6%，不是零。
/// **96 → 112（2026-08-30）**：加坚定不移带来两个（`Unmovable` 本体 +
/// `UnmovableCharge` 充能），95 个变体顶到 96 的天花板了。
/// 一次多给 16 个槽，免得每加一张能力牌都要动这里。
/// 代价照规矩实测记在 CLAUDE.md 的「当前状态」里（`StatusVal = i16`，
/// 每多一个槽是 2 字节 × 6 个 Entity = 12 字节）。
/// **112 → 128（2026-09-02）**：加地道虫钻地等状态扩容到 128。
pub const N_STATUS: usize = 128;

/// Status / power slots. Indexed into `Entity::status`.
///
/// Note these are *values*, not always stack counts:
///   - `DamageCap` stores the cap itself (难以杀灭 9 => 9)
///   - `Slow` stores accumulated bonus percent this turn (缓慢, +10 per card)
///   - `Rage` stores strength gained per player Skill (激怒 2 => 2)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum St {
    Strength = 0,
    Dexterity,
    Vulnerable,
    Weak,
    Frail,
    Artifact,
    Intangible,
    DemonForm,
    Rage,
    Slow,
    DamageCap,
    /// 缩小 (`SHRINK_POWER`)：攻击方的伤害 ×2/3。
    ///
    /// 存的是观测到的显示值（实测是 `-1`）。**每层的缩放比例未验证** ——
    /// 至今只见过 1 层，所以按"有/无"处理，和易伤同样的语义。
    Shrink,
    /// 「这个敌人带缓慢」——每打出一张牌给它的 [`St::Slow`] 加多少个百分点。
    ///
    /// 需要和 `Slow` 分开存，是因为 `Slow` 每回合归零，光看 `Slow == 0`
    /// 分不出「没有缓慢」和「有缓慢但这回合还没出过牌」。实测（2026-08-15
    /// 第1幕第13层，旧日雕像）回合开始就是 `SLOW_POWER=0`。
    /// 游戏不报这个量，它是内核从敌人身上有没有缓慢这件事推出来的。
    SlowSource,
    /// 只在本回合有效的力量（预备打击的 `SETUP_STRIKE_POWER`）。
    ///
    /// 实测（2026-08-15 第1幕第13层）：打出预备打击后同时出现
    /// `STRENGTH_POWER=2` 和 `SETUP_STRIKE_POWER=2` —— 游戏就是加真力量，
    /// 再挂一个「回合结束时扣回去」的标记。这里照抄这个建模。
    TempStrength,

    // ---- 以下是能力牌。它们的效果**不写在这里**，写在 content.rs 的 `POWERS`
    // 表里；层数按本仓库既有约定 = **每次触发的效果值**（激怒 2 = 每次 +2 力量）。
    /// 薪火之源：回合开始获得 N 能量
    Pyre,
    /// 无惧疼痛：每消耗一张牌获得 N 格挡
    FeelNoPain,
    /// 黑暗之拥：每消耗一张牌抽 N 张
    DarkEmbrace,
    /// 撕裂：我的回合内每次失去生命，获得 N 力量
    Rupture,
    /// 绯红披风：回合开始失去 1 生命并获得 N 格挡
    CrimsonMantle,
    /// 滚石：回合开始对全体造成 N 伤害，然后 N += 5（层数会自己长）
    RollingBoulder,
    /// 凶恶：每给一个敌人上易伤就抽 N 张
    Vicious,
    /// 再生。关键词原文：「再生会在你的回合结束时回复相应生命。
    /// 每回合再生的数值会减少1。」目前只有再生药水能给（药水没建模），
    /// 但实录里出现过，映射它是为了别让那些帧被降级。
    Regen,
    /// 覆甲。关键词原文（权威卡表）：**「在你的回合结束时获得格挡。
    /// 覆甲会在你的回合开始时减少1层。」** —— 和初代不一样（初代是回合开始
    /// 给格挡、挨了未被格挡的伤害才掉层），别照初代记忆写。
    /// 在 `POWERS` 里是**两条**规则：TurnEnd 给格挡，TurnStart 掉 1 层。
    PlatedArmor,
    /// 狂怒：本回合内每打出一张攻击牌获得 N 格挡。在 `TURN_SCOPED` 里
    Frenzy,
    /// 火焰屏障：本回合内每挨一次攻击就反伤攻击者 N 点。在 `TURN_SCOPED` 里
    FlameBarrier,
    /// 势不可当：**每次获得格挡**（实际数值 > 0）就对**随机一个**敌人造成
    /// N 点伤害。伤害走 `ValueProp.Unpowered`，即**不吃力量**（[源码]）。
    Juggernaut,
    /// 巨像：**攻击者身上带易伤时**，我受到的伤害 ×1/2；敌人回合结束掉 1 层。
    /// 注意条件挂在**攻击者**身上，不是我身上 —— 是「打我的那个自己带易伤」。
    Colossus,
    // ---- 以下三个是**规则修饰**，不是触发器：它们表达"不要做某件事"，
    // `POWERS` 表达不了。消费点在 `step.rs` 里三个窄 `if`，集中注释在那儿。
    /// 壁垒：格挡不再在回合开始时清零
    Barricade,
    /// 均衡：本回合结束时不弃手牌。在 `TURN_SCOPED` 里
    Entrench,
    /// 惊逃：回合结束时随机打出手牌里 N 张攻击牌（攻击随机敌人）。
    Stampede,
    /// 连环拳：接下来 N 张攻击牌各**额外打出一次**。每张攻击牌消耗一层。
    /// 在 `TURN_SCOPED` 里（[源码] `AfterTurnEnd` 清掉）。
    OneTwoPunch,
    /// 杂耍：每回合打出的**第 3 张**攻击牌，复制 N 份进手牌。
    Juggling,
    /// 好勇斗狠：回合开始从弃牌堆随机取 N 张攻击牌进手牌**并升级**。
    Aggression,
    /// 战斗专注留下的「本回合内不能再抽牌」。**规则修饰**（表达"不要做某件事"），
    /// 消费点在 `State::draw_one` 的第一行。在 `TURN_SCOPED` 里。
    NoDraw,
    /// 孤注一掷：本场战斗中一旦受到未被格挡的**攻击**伤害就立刻死亡
    AllOrNothing,

    // ---- 以下内核**完全不实现**，只是为了让它们从「未知 status」变成
    // 「已知但不建模」。区别很实际：未映射的 status 会把整帧降级成
    // UNKNOWN_CONTENT，于是同一帧里的**真错误**跟着一起被藏起来 ——
    // 第1幕 Boss 那一帧就是这么把两个真 bug 盖住的。
    /// 爪牙（`MINION_POWER`）。只是个标签：影响「斩杀」能不能触发，
    /// 以及**召唤者一死爪牙跟着死**（第15层雾菇实测）。内核不建模这两条。
    Minion,
    /// 幻象（`ILLUSION_POWER`）。**语义未确定**：第15层实测利齿之眼被打死后
    /// 下一回合满血复活，怀疑是它，但没能和「雾菇重新召唤」区分开。
    /// 内核不建模复活 —— 挂在这里只为对齐观测，**不要以为它有效果**。
    Illusion,
    /// 臂甲的充能。**内核自己造的量，游戏不报** —— 和 [`St::SlowSource`] 同类，
    /// 所以它**故意不在** `replay::ALL_ST` 里（拿它去 diff 只会得到一列假不一致）。
    ///
    /// 语义：`1` = 本场还没用过，`0` = 用掉了。`card_block` 里消耗。
    /// 遗物「每场战斗中，你第一次从**卡牌**中获得的格挡值翻倍」——
    /// 所以药水格挡（走 `Source::Potion`，不过 `card_block`）和能力牌给的格挡
    /// （`TOp::OwnerBlock`）都不吃它，这两条是自动成立的。
    VambraceCharge,
    /// 蜷身（`CURL_UP_POWER`）。**敌人**持有：第一次被命中时获得 N 点格挡，
    /// 然后这个 status 消失。层数 = 格挡值（观测到 14 就是 14 点格挡）。
    ///
    /// 实测（2026-08-17 第2幕31层 虱虫之祖）：
    /// ```text
    /// 帧0  134血 blk0  CURL_UP_POWER=14
    /// 帧1  127血 blk14 （状态没了）      ← 闪电霹雳+ 打了 7 点
    /// ```
    /// 三件事一次钉死：**伤害先落地、格挡后到**（否则那 7 点会被吃掉）、
    /// 层数就是格挡值、**一次性**（打完就从状态表里消失）。
    CurlUp,
    /// 失衡（`IMBALANCED_POWER`）。第2幕第19层盛碗虫（石）开局自带。
    ///
    /// **语义 2026-08-20 查明**（[源码] `ImbalancedPower.AfterDamageGiven`）：
    /// 持有者造成的伤害**被完全挡住**时，盛碗虫（石）进入失衡
    /// （[`St::OffBalance`]），其它怪则直接被击晕。
    ///
    /// 也就是说 —— **把它这一手完全挡下来，它下一手就废了**。
    /// 这是个实打实的战术信息，原来这里写着"语义完全未知"。
    Imbalanced,
    /// 扑翼 (`FLUTTER_POWER`)。偷窃草蜢等第2幕敌人自带
    Flutter,
    /// 逃跑大师 (`ESCAPE_ARTIST_POWER`)。
    EscapeArtist,
    /// 偷窃 (`SWIPE_POWER`)。
    Swipe,
    /// 荆棘（`THORNS_POWER`）。铜质鳞片给玩家 3 层，多刺蟾蜍/蝌蚪自带。
    ///
    /// **2026-08-29 建了规则**（`POWERS` 里两条，见 `content.rs`）。
    /// 在那之前它就挂在这一段"已知但不建模"里 —— 挂着、观测对得上、
    /// 但**没有任何规则读它**，所以内核少算了所有反弹伤害。
    /// 方向是低估自己的输出，不会让内核高估存活，但 L2 会少算斩杀。
    Thorns,
    /// 盛碗虫（石）**已经失衡**。和 [`St::Imbalanced`] 不是一回事：
    /// 那个是"它带失衡这个特性"，这个是"它这次真的被完全挡住了"。
    ///
    /// [源码] `ImbalancedPower.AfterDamageGiven`：`result.WasFullyBlocked`
    /// 时置位。置位之后它下一手走晕眩（`ECond::OffBalance`），
    /// 晕眩那一手 [源码] `DizzyMove` 里再清掉。
    ///
    /// **这条同时解掉了 `Imbalanced` 那句「语义完全未知」** —— 见它的注释。
    OffBalance,

    // ---- 战斗结束类遗物。**内核私有**（游戏不把遗物报成 status），
    // 所以和 `VambraceCharge` 一样故意不在 `replay::ALL_ST` 里。
    // 层数 = 回血量，规则在 `content::POWERS`。
    /// 燃烧之血：战斗胜利时回 N 血。铁甲战士自带，**每局开局就有**。
    BurningBlood,
    /// 精致折扇：本回合每打出第 3/6/9… 张攻击牌，获得 N 点格挡。
    OrnamentalFan,
    /// 水银沙漏：回合开始对全体造成 N 点伤害（**不吃力量**，[源码] `Unpowered`）。
    MercuryHourglass,
    /// 灯笼：第一回合获得 N 点能量。
    Lantern,
    /// 摆动球：每 3 个回合抽 N 张。相位见 [`St::PendulumPhase`]。
    Pendulum,
    /// 摆动球的**相位**。源码里那个 `TurnsSeen` 带 `[SavedProperty]`，
    /// **跨战斗保留** —— 上一场打了几个回合会带过来，所以不能假设从 0 开始。
    /// 层数从观测的遗物计数器灌（`RelicDef::counter_to`）。
    PendulumPhase,
    /// 历石：第 7 回合结束时对全体造成 N 点伤害。
    StoneCalendar,
    /// 奥利哈钢：回合结束时若没有格挡，获得 N 点格挡。
    Orichalcum,
    /// 奥利哈钢**第一段的快照结果**：回合结束的最早时点没有格挡 ⇒ 置 1。
    /// 第二段读它。分两段的理由见 `Hook::TurnEndVeryEarly`。
    OrichalcumArmed,
    /// 带骨肉：战斗胜利时若血量 ≤ 最大值的 50%，回 N 血。
    ///
    /// **在燃烧之血之前结算**（[源码] `AfterCombatVictoryEarly` 早于
    /// `AfterCombatVictory`），所以那 50% 是拿**战斗结束那一刻**的血量判的，
    /// 不含燃烧之血回的那 6 点。实录 `act2_f31` 把这个顺序钉死了：
    /// 36/80 血结束 → 54，正好 +18；若在 +6 之后判阈值，42 > 40 就只该 +6。
    MeatOnTheBone,
    /// 耕地（[源码] `PlowPower`，仪式兽践地那一手给自己挂 150 层）。
    ///
    /// **它是个血量闸门，不是护盾**：`AfterDamageReceived` 里判的是
    /// `target.CurrentHp <= Amount` —— 血量掉到 150 或以下、且这一下有
    /// 未被格挡的伤害，就**清空它的全部力量 + 眩晕一回合 + 移除自己**，
    /// 之后进第二阶段。层数 150 是那个阈值，不是可扣的量。
    ///
    /// 这条决定了这场仗怎么打：第一阶段耕地每回合 +2 力量一路涨，
    /// 而打到 150 会把涨上去的全部清零。
    Plow,
    /// 轰鸣（[源码] `RingingPower` + `Ringing` 附魔，仪式兽兽吼给我挂）。
    ///
    /// 真实机制是**牌级附魔**：给我牌组里每一张牌挂 `Ringing`，
    /// 而 `ShouldPlay` 判「这张牌被附魔了 **且** 本回合已经出过牌 ⇒ 打不出」。
    /// 因为它afflict的是**所有**牌（含中途进场的），效果等价于
    /// **这一回合最多只能出 1 张牌**，所以内核把它降成一个玩家身上的 status，
    /// 消费点在 `legal_actions`。
    ///
    /// **牌级附魔这个系统内核没有**，这是一处刻意的降维。等出现第二个
    /// afflict 类机制（而且语义和这个不同）时再考虑建真的附魔表。
    ///
    /// 在**我的回合结束**时移除（[源码] `AfterSideTurnEnd`），
    /// 所以它不在 `TURN_SCOPED` 里 —— 那个是回合**开始**清。
    Ringing,
    /// 寄生物（[源码] `InfestedPower`，异蛙寄生虫开局自带 4 层）。
    ///
    /// **宿主死亡时召唤 4 只 Wriggler**，层数就是召唤只数。
    /// 源码还有一条 `ShouldStopCombatFromEnding() => true` —— 内核不需要
    /// 单独实现它：召唤发生在 `hit_enemy_with` 里、`check_over` 之前，
    /// 那时场上已经有活的敌人了，胜利判定自然不会成立。
    Infested,
    /// 地精之角：敌人死亡时 +1 能量、抽 1 张牌（[源码] `GremlinHorn`）。
    ///
    /// **遗物私有量**：游戏不把遗物报成 status，所以它走 `private_status`
    /// 逐帧 carry，并且**不进** `replay::ALL_ST`。
    GremlinHorn,
    /// 缠绕（[源码] `ConstrictPower`，蛇行扼杀者施加）。
    ///
    /// **计数型，不衰减**（[源码] `PowerStackType.Counter`），
    /// 在**我的回合结束**时对我造成等于层数的伤害
    /// （`AfterSideTurnEnd` + `participants.Contains(Owner)`）。
    ///
    /// 那点伤害**走格挡**：[源码] 用的是 `CreatureCmd.Damage(..., Unpowered, ...)`,
    /// 和灼伤逐字同构，而灼伤走不走格挡是玩家判过的（走）。
    /// 见 docs/verification-log.md 的「玩家给的判定」。
    ///
    /// **一处[源码]里有、内核没建**：`AfterDeath` 会在**施加者**死掉时
    /// 移除这个 debuff。内核不记 status 是谁挂的，所以没做 —— 后果是
    /// 打死扼杀者之后内核仍然每回合扣血，**高估**了伤害（保守方向）。
    Constrict,
    /// 抱抱先生（[源码] `MrStruggles.AfterPlayerTurnStart`）：
    /// 我的回合开始时，对**所有可命中的敌人**造成等于**当前回合数**的伤害。
    ///
    /// 伤害是 `ValueProp.Unpowered` —— **不吃力量，也不算"攻击"**。
    /// 后者今天就有后果：蜂群术士的人体蜂房只对 `IsPoweredAttack()` 发作，
    /// 所以抱抱先生这一下**不会**给我塞晕眩（第2幕第27层实测：145→144，
    /// 抽牌堆没变）。
    ///
    /// **遗物私有量**，走 `private_status` 逐帧 carry，不进 `ALL_ST`。
    MrStruggles,
    /// 人体蜂房（[源码] `PersonalHivePower`，蜂群术士开局自带 1 层）。
    ///
    /// **已知但内核没建模。** 语义：被一次 `IsPoweredAttack()` 的伤害命中时，
    /// 往我的**抽牌堆随机位置**塞 `Amount` 张晕眩。判据是
    /// `props.HasFlag(Move) && !props.HasFlag(Unpowered)`，所以
    /// **卡牌攻击的每一段命中各触发一次**，而遗物/药水伤害不触发
    /// （第2幕第27层实测：火焰药水 20 点没塞晕眩，抱抱先生也没有）。
    /// 层数由它的信息素喷吐从 1 涨到最多 3。
    ///
    /// **原来记的"没建的原因"已经不成立了**（2026-08-25 复核）：那句话说
    /// 「内核分不出 powered」，而 2026-08-22 那轮已经把这一维穿进了伤害管线
    /// （`hit_enemy_with` 的 `powered` 参数 + `damage::apply_modifiers_unpowered`）。
    /// **今天真正还欠的是两样**：把 `powered` 传进 `Hook::EnemyDamaged`
    /// （`fire_ctx` 现在不带它），以及一个「往抽牌堆随机位置塞 N 张」的 op。
    /// 映射它只是为了别让整帧降级成 UNKNOWN
    /// （和 `Minion` / `Illusion` 同一条理由）。
    ///
    /// **战术后果记在这里，因为求解器看不见**：对带这个 status 的敌人，
    /// 多段攻击的真实代价远高于面板伤害（焚烧+ 五段 = 五张晕眩），
    /// 该用少段大伤害和药水。
    PersonalHive,
    /// 污染（[源码] `TaintedPower`，感染棱柱的活力火花挂在我每张技能牌上）。
    ///
    /// **已知但内核没建模行为。** 语义：我身上每层污染，让我挨的
    /// **每一次** `IsPoweredAttack()` 伤害 **+1**（`ModifyDamageAdditive`），
    /// 在**敌人回合结束时整个移除**（不是每回合掉一层）。
    ///
    /// **为什么不能照着建**：默认对拍路径上 `injected_enemy_turn` 直接拿
    /// **观测到的意图标签**当最终伤害，而标签**已经含了污染**
    /// （第2幕第31层实测：打出一张技能后标签当场从 `Attack:15` 变 `Attack:17`）。
    /// 再在内核里加一次就是**重复计数** —— 和古茶具那 2 点能量、
    /// 薪火之源那 1 点能量是同一个坑。
    ///
    /// **它真正咬人的地方在 L2**：`solve_turn` 的 `Threat` 是**同步那一刻**
    /// 冻住的标签，而线里每多打一张技能牌，实际来袭就比它多
    /// `层数 × 命中段数`。所以求解器在这类战斗里**系统性高估技能牌**。
    /// 实战验过一次：它按"防御给 5 点格挡"选防御，而扣掉污染只值净 3，
    /// 换算成它自己的权重，御血术+ 反而更高分。
    /// 要修得让 `Threat` 带上"同步那一刻的污染层数"，那是 L2 的接口改动。
    Tainted,
    /// 活力火花（[源码] `VitalSparkPower`，感染棱柱开局自带 2 层）。
    /// **已知但内核没建模。** 它给我牌组里**每一张技能牌**挂 `Tainted` 附魔
    /// （含中途进场的），层数 = 它自己的层数；脉动那一手还会给自己再 +2。
    /// 牌级附魔这个系统内核没有（和轰鸣同一处降维），只映射名字和层数。
    VitalSpark,
    /// 黑暗镣铐（[源码] `DarkShackles`）：**本回合**让一名敌人失去 N 点力量。
    /// 游戏用两个 status 表示（`DARK_SHACKLES_POWER` 记账 + `STRENGTH_POWER` 变负），
    /// 回合末还回去。
    ///
    /// **这一栏（游戏那个记账 status）只映射不建模**，因为内核用的是另一套记法：
    /// 力量当场变负 + `TempStrength` 记住欠多少，回合末由
    /// `step::strip_temp_strength` 还回去 —— 那个函数**对玩家和每一只敌人
    /// 都跑**（`end_turn` 里两个循环），凌虐和黑暗镣铐走的是同一条路。
    ///
    /// > 这里原来写着「内核的 `TempStrength` 只做玩家侧，敌人侧还没有」。
    /// > **那句话是错的**，而且 2026-08-27 我照它在 verification-log 里报了一条
    /// > 不存在的 bug（"凌虐被当成永久减力量"）。两个测试一直钉着这条路：
    /// > `dark_shackles_restores_enemy_strength_at_end_of_turn` 和
    /// > `mangle_strength_loss_is_returned_at_end_of_enemy_turn`。
    /// > **注释是"假设"这一档，代码才是事实** —— 这条教训值得留在原地。
    ///
    /// [实测] 第2幕第31层：给 −9 力量之后，`5×3` 的意图变成 `0×3` ——
    /// 也就是说**加法修正先求和再夹 0**（`5 − 9 + 2(污染) = −2 → 0`）。
    DarkShackles,
    /// 沙坑（[源码] `SandpitPower`，第 2 幕 Boss 无厌沙虫开局给我挂 4 层）。
    ///
    /// **已知但内核没建模。** 语义是一条**即死倒计时**：
    /// 每个**敌人回合开始**减 1（`AfterSideTurnStartLate(Enemy)`），
    /// 减到 0 时 `AfterRemoved` 走 `CreatureCmd.Kill(玩家, force: true)` —— 直接死，
    /// 和血量无关。它塞给我的 6 张狂乱逃离打出去给它 +1。
    ///
    /// **没建的原因**：内核没有"敌人回合开始"这个钩子（现有的 `TurnStart` 是
    /// **我的**回合开始），而为它加一个只有一个消费者的钩子、还要在 `check_over`
    /// 之外再开一条死亡路径，是个真正的接口改动，不是一行表。
    ///
    /// **战术后果记在这里，因为求解器完全看不见这条时间线**：
    /// 打这只 Boss 时"还剩几个回合"由它决定，而不是由血量决定；
    /// 狂乱逃离虽然是张状态牌，**打出去买一个回合几乎总是划算的**
    /// （1 费换 3 费的行动量），实战就是靠这条赢的。
    Sandpit,
    /// 舵盘（[源码] `CaptainsWheel`）：第 3 个回合开始时获得 18 点格挡。
    /// 层数就是格挡值。**遗物私有量**，不进 `ALL_ST`。
    CaptainsWheel,
    /// 孵化（[源码] `HatchPower`，结实的卵自带）。**已知但内核没建模。**
    ///
    /// 计数型，每个回合末掉 1 层；掉到 0 时那只卵**原地变成幼虫**
    /// （换名字、血量重掷成 19-22 并回满）。内核没有"敌人原地变形"这种 op，
    /// 建它要先加 `EOp::TransformSelf`。映射同样只为不让整帧降级。
    Hatch,
    /// 仪式（[源码] `RitualPower`，虔诚雕刻师等持有）。
    /// 回合结束时转化为等量力量。
    Ritual,
    /// 护壁（[源码] `RampartPower`，活体盾开局自带 25 层）。
    /// 玩家回合开始时给场上所有高塔炮手等量格挡。
    Rampart,
    /// 库存（[源码] `StockPower`，巨斧机器人开局自带 2 层）。
    ///
    /// **被击杀时，在同一格召唤一只全新的自己，层数 −1** ——
    /// 所以一场要打三具身体（实测 74 / 71 / 77 血，源码区间 70-78）。
    /// `ShouldStopCombatFromEnding()` 返回 true，打死带库存的那一具**不算赢**。
    ///
    /// 层数还**决定它启动那一手给自己多少力量**：
    /// [源码] `BootUpStrGain * (2 - StockAmount)` = `3 × (2 − 层数)`，
    /// 于是第一具 +0、第二具 +3、第三具 +6 —— **越死越强**。
    /// 内核用 `EOp::SelfStatusPerStack` 表达这个 `6 − 3×层数`。
    ///
    /// 规则在 `POWERS` 的 `Hook::EnemyDied`，走
    /// `TOp::SummonCarryingSelfMinusOne`。层数减到 0 时那一具身上就没有这个
    /// status 了，`fire` 对 0 层的 status 本来就不触发 ⇒
    /// **"第三具死了战斗结束"是自动成立的**，不需要额外判断。
    Stock,
    /// 翱翔/飞行（[源码] `SoarPower`，猫头鹰法官等持有）。
    /// 受到有源攻击伤害时减少 50%（×0.5）。
    Soar,
    /// 活力（[源码] `VigorPower`）。**下一张攻击牌的伤害加值**，加法项，
    /// 和力量同一档（`ModifyDamageAdditive`）。赤牛开局给 8 层。
    ///
    /// 三条从源码读出来的、卡面读不出来的规矩：
    ///
    /// 1. **只吃有源攻击**（`if (!props.IsPoweredAttack()) return 0m;`），
    ///    所以药水 / 遗物伤害不吃它 —— 和那五个乘区同一条 gate。
    /// 2. **多段牌的每一段都吃满。** `AttackCommand.Execute` 里
    ///    `Hook.BeforeAttack` 在第 536 行、多段 `for` 循环在 538、
    ///    `Hook.AfterAttack` 在 656 —— 钩子在循环**外面**，
    ///    所以活力挂在整条 `AttackCommand` 上，循环里每一段
    ///    `ModifyDamageAdditive` 都返回满额。
    ///    8 活力配双重打击（5x2）是 (5+8)x2 = 26，**不是** 5x2+8 = 18。
    ///    读成后者会系统性低估赤牛，而且只在多段牌上看得出来。
    /// 3. **整条 `AttackCommand` 打完才一次性清零**（`AfterAttack` 里
    ///    `ModifyAmount(-amountWhenAttackStarted)`），不是每段扣一层。
    ///    同一张牌若发第二条攻击命令，第二条**吃不到**（`commandToModify`
    ///    已经被占住，`cardSource` 对不上就返回 0）。
    ///
    /// 4. **没花掉的活力跨回合留着。** `PowerStackType.Counter`，而且整个
    ///    `VigorPower` 里**没有任何回合末衰减** —— 只有 `AfterAttack` 会清它。
    ///    所以第一回合只防御的话，赤牛那 8 点原样带到第二回合。
    ///    （这条是写测试时红出来的：当时断言写的"第二回合活力该是 0"，
    ///    红的是断言不是内核。）
    ///
    /// 游戏报 `VIGOR_POWER`，所以它**进** `replay::ALL_ST` 参与 diff。
    Vigor,
    /// 赤牛（[源码] `Akabeko`）：第一回合开始时获得 N 点活力。
    ///
    /// 判据是 `TurnNumber <= 1`（**不是 `== 1`**），和灯笼逐字同构 ——
    /// 抄的是同一个坑。层数 = 给多少活力（8）。
    /// **遗物私有量**（游戏不把遗物报成 status），不进 `ALL_ST`。
    Akabeko,
    /// 高压（[源码] `HighVoltagePower`，电击机器人 `AfterAddedToRoom` 自带 2 层）。
    ///
    /// `AfterSideTurnEnd` + `participants.Contains(Owner)` ⇒ **敌人回合结束时，
    /// 给自己加等于层数的力量**。`PowerStackType.Counter`，不衰减，所以它会
    /// 一路涨：出场那回合结束 +2，下回合结束再 +2……
    ///
    /// [实测] 第3幕第45层：出场后第一次看到它带 `STRENGTH_POWER=2`，
    /// 意图标签 `Attack:16` = 基础 14 + 力量 2，对上。
    ///
    /// 规则在 `POWERS` 的 `Hook::EnemyTurnEnd`（那个钩子就是为它加的）。
    /// 游戏报 `HIGH_VOLTAGE_POWER`，所以它**进** `replay::ALL_ST`。
    HighVoltage,
    /// 领地意识（[源码] `TerritorialPower`，精英 多尼斯异鸟 `AfterAddedToRoom` 自带 1 层）。
    ///
    /// **和高压逐字同构**：`AfterSideTurnEnd` + `participants.Contains(Owner)`
    /// ⇒ 敌人回合结束时给自己加等于层数的力量，`Counter` 型不衰减。
    /// 两条规则同构不是巧合 —— 源码里就是同一套写法，所以这里也共用
    /// `Hook::EnemyTurnEnd`，一行表。
    ///
    /// [实测] 2026-08-27 第1幕第9层：出场带 `TERRITORIAL_POWER=1`，
    /// 第 1 手 `Attack:17`（力量 0），它的回合结束后力量 1，
    /// 第 2 手 `Attack:4×3`（基础 3×3 + 力量 1），再结束后力量 2，
    /// 第 3 手 `Attack:19`（基础 17 + 力量 2）。三帧互相印证。
    ///
    /// 游戏报 `TERRITORIAL_POWER`，所以它**进** `replay::ALL_ST`。
    Territorial,
    /// 招架盾（[源码] `ParryingShield.AfterSideTurnEnd`）。**遗物私有量** ——
    /// 游戏不把遗物报成 status，所以不进 `replay::ALL_ST`，靠 `relic_carry` 逐帧带。
    /// 层数 = 打多少（6）；门槛 10 点格挡写在规则的 `TCond` 里，不是层数。
    ParryingShield,
    // ---- 2026-08-27 第二批遗物。**全部是内核私有量**（游戏不把遗物报成 status），
    //      所以一律不进 `replay::ALL_ST`，靠 `relic_carry` 逐帧带着走。
    //      层数一律 = 效果值，条件写在规则的 `TCond` 里。
    /// 锚（[源码] `Anchor.BeforeCombatStart` -> `GainBlock(10, Unpowered)`）。
    Anchor,
    /// 弹珠袋（[源码] `BagOfMarbles`，`TurnNumber <= 1` 给全体易伤 1）。
    BagOfMarbles,
    /// 红面具（[源码] `RedMask`，`TurnNumber <= 1` 给全体虚弱 1）。
    RedMask,
    /// 准备背包（[源码] `BagOfPreparation`，第 1 回合多抽 2）。
    BagOfPreparation,
    /// 烛台（[源码] `Candelabra`，`TurnNumber == 2` 时 +2 能量）。
    Candelabra,
    /// 开心小花（[源码] `HappyFlower`，每 3 回合 +1 能量）。
    HappyFlower,
    /// 花粉核心（[源码] `PollinousCore`，每 4 回合多抽 2）。
    PollinousCore,
    /// 佩尔之肉（[源码] `PaelsFlesh`，`TurnNumber >= 3` 起每回合 +1 能量）。
    PaelsFlesh,
    /// 苦无（[源码] `Kunai`，同回合每 3 张攻击牌 +1 敏捷）。
    Kunai,
    /// 开信刀（[源码] `LetterOpener`）。**遗物私有量**，层数 = 伤害（5）。
    /// 「同回合每 3 张技能牌」那一半在规则的 `TCond::EveryNthSkillThisTurn` 里。
    LetterOpener,
    /// 遭到包围（[源码] `SurroundedPower`，第 2 幕 Boss 开局给我挂）。
    ///
    /// **被从后方攻击时受到的伤害 +50%**，而"后方"由**我的朝向**决定：
    /// 敌人分左右两侧（`BackAttackLeft` / `BackAttackRight`），
    /// 我面向哪一侧，另一侧打过来就是背后。
    /// **打出有目标的牌或药水会把朝向转到那个目标那一侧**（游戏原文）。
    ///
    /// [实测] 2026-08-27 第 2 幕 Boss 帧25 -> 帧28：**同一只、同一手招式**，
    /// 我把有目标的牌打到它身上之后，意图标签 `21 -> 14`。21 = 14 × 1.5。
    /// 另有两组同向的：火箭 `27` = 18×1.5、`30` = 20×1.5（我当时面向碾碎爪）。
    Surrounded,
    /// 敌人在**左侧**（[源码] `BackAttackLeftPower`）。纯站位标记，自己没有效果，
    /// 只和 `Surrounded` + `FacingRight` 一起决定这一击算不算"从后方"。
    BackAttackLeft,
    /// 敌人在**右侧**（[源码] `BackAttackRightPower`）。同上。
    BackAttackRight,
    /// **我面朝右侧**（内核私有：游戏不把朝向报成 status）。
    ///
    /// 0 = 面朝左。它不是观测量，所以 `Replayer` 每帧从**观测到的意图标签**
    /// 反推一次（`infer_facing`）—— 比"自己记着"稳，因为读档/中途接入都还原得回来。
    FacingRight,
    /// 蟹之怒（[源码] `CrabRagePower`，第 2 幕 Boss 两只都带）。
    /// **有盟友死亡时，持有者获得 6 点力量和 99 点格挡。**
    ///
    /// [实测] 2026-08-27：碾碎爪死的那一帧，火箭 `力量 2 -> 8`、`格挡 0 -> 99`；
    /// **下一帧它自己回合开始，99 清零** —— 也就是那 99 只吃我"杀掉它同伴之后
    /// 到它出手之前"这一段的伤害。实战据此判断：该杀就杀，别为了躲 99 拖回合。
    CrabRage,
    /// 接续（[源码] `ReattachPower`，残杀千足虫每一节出场自带 25）。
    ///
    /// **已知但内核没建模行为，方向是乐观的**：一节死掉之后不会被移出战斗，
    /// 隔一手走 `REATTACH_MOVE`，**只要还有别的节活着就回满 25 血**
    /// （`DoReattach` 里 `if (!AreAllOtherSegmentsDead())`）。
    /// 内核里敌人死了就是死了，所以它会**低估这一场**。
    ///
    /// 要建得先有"尸体留在场上 + 死亡后仍然走出招表"这套东西 ——
    /// 和雾菇的复活是同一个洞，一起记在 roadmap 的「故意没做」里。
    /// 这一栏只做标记，**不进 `replay::ALL_ST`**（内核产生不出这个量）。
    Reattach,
    /// 光耀（[源码] `RadiancePower`，明耀酊剂给的）。
    ///
    /// `AfterEnergyReset` -> `GainEnergy(1)` 然后 `Decrement` ——
    /// 也就是**回合开始能量回满之后 +1，并掉一层**，层数 = 还能给几回合。
    /// 内核的 `Hook::TurnStart` 正好是"能量回满后、抽牌前"，位置对得上。
    ///
    /// 游戏报 `RADIANCE_POWER`，行为已建模 ⇒ **进 `replay::ALL_ST`**。
    Radiance,
    /// 娇弱（[源码] `TenderPower`，猎人杀手给我挂的）。
    ///
    /// **本回合每打出一张牌 −1 力量 −1 敏捷，回合结束时按"这回合打了几张"
    /// 一次性还回来。** 也就是说它惩罚的是"一回合里打很多张"，而不是永久削弱 ——
    /// 卡面读不出这一半，是源码里 `AfterSideTurnEnd` 那一句还回去的。
    ///
    /// **内核这一栏是「有没有挂着」的标记，不是层数**：游戏报的
    /// `DisplayAmount` 是本回合已出牌数（我打了 5 张就报 5），
    /// 而内核自己有 `cards_played`，还的时候用 `Amt::CardsPlayed`。
    /// 两个量不同源 ⇒ **它不进 `replay::ALL_ST`**，否则每帧都是假红。
    Tender,
    /// 百年积木（[源码] `CentennialPuzzle.AfterDamageReceived`）。同上，私有量。
    /// 层数 = 抽几张（3）。**一场只发作一次**，靠规则里的 `TOp::ClearSelf` 实现 ——
    /// 清零之后这条规则再也匹配不上，正好等于源码里那个 `UsedThisCombat`。
    CentennialPuzzle,
    /// 凋萎存在（[源码] `WitheringPresencePower`，第 3 幕 Boss 永世沙漏
    /// `AfterAddedToRoom` 给**玩家**挂 6 层）。
    ///
    /// **2026-08-25 建了。** 语义：`AfterCardPlayed` 让计数器 `CardsLeft` −1，
    /// 减到 0 时往我**手牌**塞 1 张凋萎，然后计数器重置回 6。
    /// 层数（游戏显示的那个数）就是 `CardsLeft`，**也是观测量**，逐帧同步得到。
    ///
    /// 规则在 `content::POWERS` 里，靠 2026-08-25 新加的 `Hook::CardPlayed`
    /// ——「打出任意一张牌」这个钩子在此之前不存在，只有按类型过滤的
    /// `PlayerAttack` / `PlayerSkill` 两个。
    ///
    /// **它的代价**：每打 6 张牌 = 手上多一张凋萎 = 回合末至少 3 点血。
    /// 没建之前内核在这只 Boss 面前系统性地**乐观**。
    WitheringPresence,
    /// 坚定不移（[源码] `UnmovablePower`）。**层数 = 每回合可以翻倍几次**。
    ///
    /// 卡面写的是「翻倍你每回合第一次从卡牌中获得的格挡」，源码不是"第一次"：
    /// `ModifyBlockMultiplicative` 数的是**本回合此前已经获得过几次**
    /// （`BlockGainedEntry.HappenedThisTurn` 且 `IsCardOrMonsterMove`），
    /// `num >= Amount` 才停。所以两张就是**前两次**都翻倍。
    ///
    /// 门是 `props.IsCardOrMonsterMove()` —— 正好对应内核的
    /// [`crate::damage::card_block`]，和臂甲同一个入口，
    /// 于是"药水/能力牌给的格挡不吃翻倍"这条自动成立。
    /// 假商人版开心小花（[源码] `FakeHappyFlower`）。**周期不同：每 5 回合**，
    /// 真品是每 3 回合。周期写死在 `POWERS` 的 `TurnMultipleOf` 里，
    /// 所以两者不能共用一个 status —— 复用真品的会让内核每 3 回合就多给 1 能量。
    /// **被击晕**：这只敌人的下一次行动被顶掉（[源码] `Creature.StunInternal`）。
    ///
    /// 三条语义全是源码给的，卡面一条都读不出来：
    /// 1. **打死了就不生效** —— `if (CombatState != null && !IsDead)`。
    ///    吹哨是"先打 33 点再击晕"，斩杀的那一下白给一个击晕。
    /// 2. **本来要出的那一手被顶掉，不是推迟** —— `SetMoveImmediate` 换掉
    ///    当前的 `MoveState`，那一手就没了。
    /// 3. **醒来之后回到"上一次真正打出的那一手"** ——
    ///    `FollowUpStateId = stateLog.Last().Id`，读的是**已经打出过**的那一手，
    ///    不是被顶掉的那一手。所以击晕会让它把上一手**再打一遍**。
    ///
    /// 内核里是个一次性标记：敌人回合开头看到它就跳过这一手、清掉自己，
    /// 并把出招指针按第 3 条退回去。见 `step::take_stun_if_stunned`。
    /// 仪式**已经上膛**：下一次敌人回合结束才真的转化成力量。
    ///
    /// [源码] `RitualPower` 里那个 `_wasJustAppliedByEnemy` ——
    /// 施加它的**那一个**回合末**跳过不结算**，之后每个回合末都 +Amount 力量。
    /// 内核里把它反过来表达成"上膛"：回合末先看上没上膛（上了才给力量），
    /// 然后无条件上膛。第一次回合末必然没上膛 ⇒ 自动跳过，和源码等价。
    ///
    /// 这一条**决定虔诚雕刻师第 2 回合打 12 还是 21**，差一整回合的伤害。
    RitualArmed,
    Stunned,
    FakeHappyFlower,
    Unmovable,
    /// 坚定不移**这回合还剩几次翻倍**。**内核自己造的量，游戏不报**
    /// —— 和 [`St::VambraceCharge`] / [`St::SlowSource`] 同类，
    /// 所以它**故意不在** `replay::ALL_ST` 里。
    ///
    /// 我的回合开始时被重置成 `Unmovable` 的层数（`POWERS` 里那条规则），
    /// 在 `card_block` 里一次次用掉。
    /// 和臂甲的区别只有一个：那个一场一次，这个**每回合回满**。
    UnmovableCharge,
    /// 适生力（[源码] `AdaptablePower`，实验体阶段 1 & 2 自带 1 层）。
    /// 死亡时阻止战斗结束，触发复苏并进入下一阶段。
    Adaptable,
    /// 剧痛刺击（[源码] `PainfulStabsPower`，实验体阶段 2 自带 1 层）。
    /// 未被格挡的攻击命中时，给玩家弃牌堆塞入等量伤口。
    PainfulStabs,
    /// 天敌/复仇宿敌（[源码] `NemesisPower`，实验体阶段 3 自带 1 层）。
    /// 敌方回合结束时交替获得/移除 1 层无实体。
    Nemesis,
    /// 连环爪击**已经多出来几段**（[源码] `TestSubject.ExtraMultiClawCount`）。
    ///
    /// 实验体阶段 2 那一手是 `10 × (3 + ExtraMultiClawCount)`，
    /// 而 `ExtraMultiClawCount++` 在**攻击之后** —— 所以第一次 3 段、
    /// 然后 4、5、6…… 这是阶段 2 的时钟，不建就是**乐观**的。
    ///
    /// **内核私有量，游戏不报**（它只在意图标签里体现成段数），
    /// 所以它**故意不在** `replay::ALL_ST` 里 —— 和 `UnmovableCharge` 同类。
    ClawGrowth,
    /// 尖叫（骇鳗自带）。**层数是血量阈值，不是层数**（`PowerStackType.Counter`）。
    ///
    /// [源码] `ShriekPower.AfterDamageReceived`：挨到**未被格挡**的伤害且
    /// `CurrentHp <= Amount` 时，把自己击晕并跳到 `TerrorState`，然后移除自身。
    /// 和巨像的 `St::Plow` 是同一个形状（血量闸门 -> 换招），规则在 `POWERS` 里。
    Shriek,
    /// 侵蚀/烟雾（[源码] `SmoggyPower`，活雾给我挂的）。
    Smoggy,
    /// 胆小（[源码] `SkittishPower`，花园幽灵鳗自带 6 层）。
    /// 每回合第一次受到未被格挡的卡牌攻击伤害时，获得等量格挡。
    Skittish,
    /// 胆小本回合是否已经触发过格挡（内核私有量，在 `TURN_SCOPED` 里）。
    SkittishTriggered,
    /// 蒸汽喷发（[源码] `SteamEruptionPower`，瀑布巨兽的压力计数器）。
    SteamEruption,
    /// 钻地（[源码] `BurrowedPower`，地道虫持有）。
    /// 钻地期间敌人回合开始格挡不清零，受到攻击且格挡被击破时被打进眩晕并移除自身。
    Burrowed,
    /// **我这一回合改过这只敌人的出招指针**（`TOp::OwnerForceMove` 置位）。
    ///
    /// 消费点只有一处：`step::injected_enemy_turn`。注入的伤害来自
    /// **同步那一刻观测到的意图**，一旦我把它的招换掉，那个标签就过期了 ——
    /// 实验体被砍死之后换成「复苏」（不打人）、仪式兽过血量闸门被换成眩晕，
    /// 两处都是这样。不拦的话求解器会以为它照打不误。
    ///
    /// 内核私有量，游戏不报 ⇒ **故意不在** `replay::ALL_ST` 里。
    /// 用完当帧就清（敌人整边行动完之后）。
    MoveForcedThisTurn,
}

impl St {
    #[inline(always)]
    pub const fn ix(self) -> usize {
        self as usize
    }
}

/// A single card instance. Per-instance modifiers (upgrade, enchants) live here
/// rather than in the card database, because two copies of the same card can
/// carry different enchants (this run had 腐化 on one 拆卸+ only).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CardInst {
    pub id: u16,
    pub flags: u16,
    /// Flat damage bonus from 锋利 (Sharp) style enchants.
    pub bonus: i16,
    /// **这一张实例的费用增量。** 内容表给的是牌名的基础费用，而游戏里有牌
    /// 会**永久改这一张的费用**（狂乱逃离每打出一次自己 +1 费，
    /// [源码] `FranticEscape.OnPlay` 里的 `EnergyCost.AddThisCombat(1)`）。
    ///
    /// 它和 `F_FREE_THIS_TURN` 是同一条规矩的两半：**观测到的费用是权威**。
    /// 观测报 0 而表里要钱 ⇒ 免费标记；观测报得**更高**而内核解释不了 ⇒ 记在这里。
    ///
    /// 不加这个字段的后果不是"少算一点"，是**内核会给出游戏不接受的线** ——
    /// 2026-08-22 无情猛攻那个 bug 就是这个形状（内核以为牌比实际便宜）。
    pub cost_delta: i8,
    /// **这一张实例的格挡加值**，来自附魔（[源码] `EnchantmentModel.EnchantBlockAdditive`，
    /// 「灵巧」`Nimble` 就是 `=> Amount`）。
    ///
    /// 为什么要独立一个字段、不复用 `bonus`：`bonus` 是**伤害**加值（锋利那一族），
    /// 而一张既打伤害又给格挡的牌（铁斩波）可以只带灵巧 —— 合成一个字段会让它
    /// 凭空多 2 点伤害。两族附魔的 `CanEnchant` 判据本来就不同
    /// （灵巧要 `card.GainsBlock`，锋利要能打伤害）。
    ///
    /// 取 `i8` 是为了塞进 `CardInst` 原有的填充字节里 —— **`State` 一个字节没涨**。
    /// 附魔的 `Amount` 都是个位数，±127 绰绰有余。
    ///
    /// 2026-09-01 加：在那之前一张带灵巧的耸肩无视在内核里是 8 点格挡、
    /// 游戏里是 10 点，对拍当场红（骇鳗那一场帧 0）。
    pub block_bonus: i8,
}

pub const F_UPGRADED: u16 = 1 << 0;
pub const F_INNATE: u16 = 1 << 1;
pub const F_RETAIN: u16 = 1 << 2;
/// 腐化: this card deals +50% damage.
pub const F_CORRUPT: u16 = 1 << 3;
/// **这一张实例本回合免费**（[源码] `CardModel.SetToFreeThisTurn()`）。
///
/// 技能药水就是这么工作的：它随机生成 3 张技能牌让你挑 1 张，挑中的那张
/// `SetToFreeThisTurn()` 之后进手牌。所以「免费」挂在**卡实例**上，
/// 不是"本回合所有技能牌都免费"—— 差别很大，写成后者会凭空多出一堆免费牌。
///
/// 内核没有牌生成模型，所以这个 flag **不由内核自己设**，而是
/// `replay::sync` 按观测到的费用设（游戏是唯一权威，它说这张 0 费就是 0 费）。
pub const F_FREE_THIS_TURN: u16 = 1 << 4;

impl CardInst {
    pub const EMPTY: CardInst = CardInst { id: 0, flags: 0, bonus: 0, cost_delta: 0, block_bonus: 0 };

    #[inline(always)]
    pub fn upgraded(&self) -> bool {
        self.flags & F_UPGRADED != 0
    }
    #[inline(always)]
    pub fn corrupt(&self) -> bool {
        self.flags & F_CORRUPT != 0
    }
}

/// status 用 `i16` 存，对外的读写仍然是 `i32`。
///
/// 理由是纯粹的体积：`State` 的复制是热路径的主要开销（119 ns/step 里绝大部分
/// 是一次 memcpy），槽位从 16 扩到 32 时用 `i32` 存会让 `State` 从 1816 涨到
/// 2200 字节，实测掉 11% 吞吐。换成 `i16` 之后 32 个槽位和原来 16 个 `i32`
/// 一样大，扩容不要钱。
///
/// 取值范围够用：见过的最大值是缓慢的百分比（一回合几十）和滚石自增的伤害，
/// 离 32767 远得很。`add` 做了饱和，不会静默回绕。
type StatusVal = i16;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Entity {
    pub hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub status: [StatusVal; N_STATUS],
}

impl Entity {
    pub const fn new(hp: i32) -> Entity {
        Entity { hp, max_hp: hp, block: 0, status: [0; N_STATUS] }
    }
    #[inline(always)]
    pub fn get(&self, s: St) -> i32 {
        self.status[s.ix()] as i32
    }
    #[inline(always)]
    pub fn set(&mut self, s: St, v: i32) {
        self.status[s.ix()] = v.clamp(StatusVal::MIN as i32, StatusVal::MAX as i32) as StatusVal;
    }
    #[inline(always)]
    pub fn add(&mut self, s: St, v: i32) {
        let n = self.status[s.ix()] as i32 + v;
        self.set(s, n);
    }
    #[inline(always)]
    pub fn alive(&self) -> bool {
        self.hp > 0
    }
}

/// 药水槽位的**容量上限**，不是玩家有几个槽位。后者是 [`State::potion_slots`]，
/// 是个**观测量**（游戏在 `player.max_potion_slots` 里直接报）。
///
/// 两者必须分开，因为槽位数**这一局之内就会变**，而且不止一个来源：
///
/// | 来源 | 增减 |
/// |---|---|
/// | 初始 | 3 |
/// | 高进阶（[源码] `AscensionLevel::TightBelt`）| **−1**（初始变 2）|
/// | 药水腰带 | +2 |
/// | 炼金宝匣 | +4 |
/// | 药瓶皮套 | +1 |
///
/// 所以已知上界 = 3 + 2 + 4 + 1 = **10**。取 10 而不是"够用就行"的 5：
/// 溢出的后果是**静默丢掉整瓶药水**（`sync` 里那条 `slot < MAX_POTIONS`
/// 会把它跳过），求解器于是在一副更少药水的手上求最优，而这件事在输出里
/// 看不出来。宁可多几个字节。
///
/// **上界会被新遗物顶破。** 药瓶皮套就是 2026-06 那一版新加的 —— 所以
/// `sync` 里那条守卫不能只是跳过，必须**报出来**（见 `Synced::unmapped`）。
pub const MAX_POTIONS: usize = 10;

pub mod potion {
    pub const NONE: u8 = 0;
    pub const BLOCK: u8 = 1;
    pub const STRENGTH: u8 = 2;
    pub const DEXTERITY: u8 = 3;
    pub const ENERGY: u8 = 4;
    pub const FIRE: u8 = 5;
    pub const VULNERABLE: u8 = 6;
    pub const WEAK: u8 = 7;
    pub const SWIFT: u8 = 8;
    pub const REGEN: u8 = 9;
    // 2026-08-21 增量补的四瓶。**只补机制内核已经有的那些** ——
    // 已发现的 28 瓶里另外 15 瓶要新机制，一瓶都没建，名单和理由在
    // `ops.rs::POTIONS` 的表头。
    pub const EXPLOSIVE: u8 = 10;
    pub const HEART_OF_IRON: u8 = 11;
    pub const FYSH_OIL: u8 = 12;
    pub const FLEX: u8 = 13;
    /// 瓶中精灵。**它和上面所有瓶子不是一类**：`PotionUsage.Automatic`，
    /// 喝不了，只在"将要死"的那一刻自己发作。规则在 `step::try_fairy`。
    pub const FAIRY: u8 = 14;
    // 2026-08-22 这一局遇到的三瓶。攻击/技能药水是**保守近似**
    //（真实的是 3 张挑 1，内核只给 1 张），欧洛巴斯之酸是忠实建模
    //（它本来就不用挑）。逐条理由在 `ops.rs::POTIONS` 里。
    pub const ATTACK: u8 = 15;
    pub const SKILL: u8 = 16;
    pub const OROBIC_ACID: u8 = 17;
    pub const SNECKO_OIL: u8 = 18;
    /// 药水形状的石头（[源码] `PotionShapedRock`）。石化蟾蜍每场战斗开局塞一瓶。
    /// `Rarity.Token` / `CombatOnly` / `AnyEnemy` / `DamageVar(15, Unpowered)`。
    /// **Unpowered** ⇒ 走 `Source::Potion` 那条路，不吃力量也不吃易伤/虚弱/缓慢。
    pub const POTION_SHAPED_ROCK: u8 = 19;
    /// 能力药水（[源码] `PowerPotion`）。和攻击/技能药水同构，同一条保守近似。
    /// **接在末尾而不是插在 15/16 旁边** —— 这些常数是 `POTIONS` 的下标，
    /// 中间插一个会把后面每一瓶都平移一位，而槽位号会进 trace。
    pub const POWER: u8 = 20;
    /// 瓶装潜能（[源码] `BottledPotential`）：手牌并进抽牌堆 -> 整堆（含弃牌堆）洗 -> 抽 5。
    /// 规则在 `State::shuffle_all_into_draw`，同样接在末尾（这些常数是 `POTIONS` 的下标）。
    pub const BOTTLED_POTENTIAL: u8 = 21;
    /// 明耀酊剂（[源码] `RadiantTincture`）：+1 能量，并挂 3 层光耀。
    /// **接在末尾**，理由同上面能力药水那条：这些常数是 `POTIONS` 的下标。
    pub const RADIANT_TINCTURE: u8 = 22;
    pub const UNKNOWN: u8 = 255;
}

/// A pending sub-choice opened by a card (e.g. 烙印 "choose a card to exhaust").
/// While `kind != None`, the only legal actions are `Action::Choose`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pending {
    None,
    /// Choose N cards in hand to exhaust (烙印).
    ExhaustFromHand { remaining: u8 },
    /// Choose N cards in discard pile to put to hand (发掘/头槌).
    FetchFromDiscard { remaining: u8 },
    /// 头槌：从弃牌堆选一张放到**抽牌堆顶**。和 `FetchFromDiscard` 的候选集
    /// 一样（都是弃牌堆），去处不同。
    ///
    /// `exclude` 是**头槌自己**在 `cards` 里的下标，`u8::MAX` 表示没有要排除的。
    ///
    /// **为什么需要它**：效果结算（设 `Pending`）发生在牌进弃牌堆**之前**，
    /// 而玩家真正做选择发生在**之后** —— 于是不排除的话，头槌会把自己捞回
    /// 抽牌堆顶再打一次。[实测] 2026-08-30 第 3 幕第 45 层两帧：
    /// 弃牌堆只有一张防御时游戏**连界面都没弹**、直接把防御送上牌堆顶；
    /// 另一帧我先打凌虐+ 再打头槌+，候选里**有凌虐+、没有头槌+** ——
    /// 所以规则是「排除它自己」，不是「排除本回合打过的牌」。
    DiscardToDrawTop { remaining: u8, exclude: u8 },
    /// Choose N cards in hand to put to top of draw pile (战吼).
    PutToDrawPile { remaining: u8 },
    /// Choose N cards in hand to upgrade (武装).
    UpgradeInHand { remaining: u8 },
}

/// Deterministic split-stream RNG. Seed is part of the state, so a state fully
/// determines its own future — that is what makes paired/common-random-number
/// comparisons and reproducible replays possible.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rng {
    pub shuffle: u64,
    pub enemy: u64,
    /// **牌生成**（添柴/地狱之刃）。和洗牌流分开，理由和 `enemy` 分流一样：
    /// 生成一张牌不该扰动抽牌顺序。合流的话「多打一张添柴」会把这一局后面
    /// 所有抽牌全部改掉，求解器两条线的对比就失去意义了。
    pub gen: u64,
}

impl Rng {
    pub const fn new(seed: u64) -> Rng {
        Rng {
            shuffle: seed ^ 0x9E37_79B9_7F4A_7C15,
            enemy: seed ^ 0xBF58_476D_1CE4_E5B9,
            gen: seed ^ 0x94D0_49BB_1331_11EB,
        }
    }
}

#[inline(always)]
pub fn next_u64(s: &mut u64) -> u64 {
    // splitmix64
    *s = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *s;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[inline(always)]
pub fn next_below(s: &mut u64, n: usize) -> usize {
    if n <= 1 {
        return 0;
    }
    (next_u64(s) % (n as u64)) as usize
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct State {
    pub player: Entity,
    pub energy: i32,
    pub base_energy: i32,

    pub cards: [CardInst; MAX_CARDS],
    pub n_cards: u8,

    pub draw: [u8; MAX_CARDS],
    pub n_draw: u8,
    /// 抽牌堆**顶部有几张是确定的**（`draw` 数组末尾那几张）。
    ///
    /// # 它是给跨回合 planner 用的，L1 自己一个字都不读
    ///
    /// 抽牌堆整体是"顺序不可知"的（约束 2：mod 排过序，观测拿不到真实顺序），
    /// 但**有几条路径会把牌明确放到顶上**：头槌的「弃牌堆 -> 抽牌堆顶」、
    /// `Op::PutOnTopOfDraw`。这几张的身份是**确定**的，
    /// 把它们和底下那堆一起当随机是白白丢信息 —— 机会节点该只对
    /// 「顶上这几张之外」的部分枚举/采样。
    ///
    /// **维护规则只有四条**，全部收口在 `State` 的方法里：
    /// * 放到顶上（[`State::to_draw_top`]）+1
    /// * 从顶上拿走（[`State::pop_draw_top`]，**所有取牌堆顶的路径都走它**）−1
    /// * 洗牌（[`State::reshuffle_discard_into_draw`] / `begin_combat`）归 0
    /// * 往随机位置插一张（[`State::to_draw_random`]）：插进已知区就把已知区
    ///   截到插入点以上
    ///
    /// **`sync` 之后是 0 还是满，取决于 mod 报不报真实牌序**（2026-08-29）：
    /// * 老口径：`draw_pile` 被 mod 按稀有度+id 排过（约束 2），一张确定的都没有 ⇒ 0
    /// * 新口径：`draw_pile_order` 报了真实牌序（本地给 mod 打的补丁），
    ///   整堆都是确定的 ⇒ `n_draw`。机会节点在前缀耗尽之前全部塌缩成确定节点。
    ///
    /// 这两种 trace 会长期共存（老语料重录不了），所以**别假设它是 0**。
    pub n_draw_known: u8,
    pub hand: [u8; MAX_CARDS],
    pub n_hand: u8,
    pub disc: [u8; MAX_CARDS],
    pub n_disc: u8,
    pub exh: [u8; MAX_CARDS],
    pub n_exh: u8,

    pub enemies: [Entity; MAX_ENEMIES],
    pub enemy_def: [u16; MAX_ENEMIES],
    pub enemy_move: [u8; MAX_ENEMIES],
    pub n_enemies: u8,

    // ---- per-turn event counters ----
    // These exist so that "conditional" cards are ordinary cards. The old Python
    // sim refused to score 踩踏 / 被遗忘的仪式 / 怨恨 precisely because it had
    // nowhere to put these.
    /// 上一次攻击**过完乘区、扣格挡之前**打出的数字。
    ///
    /// 拳斗「获得等量于所造成伤害的格挡」和万向斩「对所有其他敌人造成等量的
    /// 伤害」都要读它。存的是 `apply_modifiers` 的输出，不是实际掉的血 ——
    /// 玩家确认过：7 点打在带易伤的目标上给 10 点格挡。
    pub last_damage: i32,
    /// 紧接着的上一次攻击有没有**打死**目标（狂宴读它）。
    /// 和 `last_damage` 同一类：由 `hit_enemy_with` 写，只在紧跟其后的 op 里有意义。
    pub last_kill: bool,
    /// 刚打出的那张牌在 `cards` 里的下标（杂耍要复制它）。
    /// 和 `last_damage` 同一类：只在紧跟其后的触发里有意义。
    pub last_played_card: u8,
    /// X 费牌打出时**捕获**的 X 值。
    ///
    /// [源码] `CardCmd`：`card.EnergyCost.CapturedXValue = playerCombatState.Energy`
    /// —— X 就是「打出这张牌的瞬间你还剩多少能量」，然后全部花光。
    /// 所以 X 费**不需要给 `Action` 加参数**：它不是玩家的选择，是局面决定的。
    pub last_x: i32,
    pub cards_played: i32,
    pub attacks_played: i32,
    /// 本回合打出的**技能牌**张数。开信刀（每 3 张技能牌 -> 全体 5 点）要用。
    ///
    /// **不能用 `cards_played - attacks_played` 顶替**：能力牌也计入 cards_played，
    /// 那样一张薪火之源就会被算成技能牌，开信刀提前发作。
    pub skills_played: i32,
    pub exhausted_this_turn: i32,
    pub hp_lost_this_turn: i32,
    /// 无情猛攻: next attack card costs 0.
    pub free_attack: i32,

    pub turn: i32,
    pub potions: [u8; MAX_POTIONS],
    /// 这一局**实际有几个**药水槽位。观测量，默认 3，见 [`MAX_POTIONS`]。
    ///
    /// 它不影响 `legal_actions`（空槽是 `potion::NONE`，本来就打不出来），
    /// 影响的是**"满仓"这个判断**：L2 的药水定价按"这一幕还会不会再掉药水"
    /// 决定要不要清仓，而 3/3 和 3/5 是两个完全不同的局面。
    pub potion_slots: u8,
    /// 每只敌人**最近几手的下标**，新的在 `[0]`。给出招机器的
    /// `CannotRepeat` / `CanRepeatXTimes` / `cooldown` 用。
    ///
    /// 深度 4 是够用而不是随便取的：现有数据里最长的约束是飞蝇菌子的
    /// `cooldown = 3`。`u8::MAX` 表示"还没出过手"。
    pub enemy_hist: [[u8; ENEMY_HIST]; MAX_ENEMIES],
    /// 每只敌人**这一整场出过哪几手**的位掩码（第 i 位 = 第 i 手出过）。
    ///
    /// 和 `enemy_hist` 分开存，因为问的是两件事：那个问"最近几手"
    /// （`CannotRepeat` / `cooldown`），这个问"曾经"（`UseOnlyOnce`）。
    /// 定长历史答不了"曾经" —— 蛮兽咆哮完 5 手之后它就滑出窗口了。
    pub enemy_used: [u32; MAX_ENEMIES],
    pub rng: Rng,
    pub pending: Pending,
    pub player_dead: bool,
    pub combat_over: bool,
}

impl State {
    pub fn new(player_hp: i32, seed: u64) -> State {
        State {
            player: Entity::new(player_hp),
            energy: 3,
            base_energy: 3,
            cards: [CardInst::EMPTY; MAX_CARDS],
            n_cards: 0,
            draw: [0; MAX_CARDS],
            n_draw: 0,
            n_draw_known: 0,
            hand: [0; MAX_CARDS],
            n_hand: 0,
            disc: [0; MAX_CARDS],
            n_disc: 0,
            exh: [0; MAX_CARDS],
            n_exh: 0,
            enemies: [Entity::new(0); MAX_ENEMIES],
            enemy_def: [0; MAX_ENEMIES],
            enemy_move: [0; MAX_ENEMIES],
            n_enemies: 0,
            last_damage: 0,
            last_kill: false,
            last_played_card: 0,
            last_x: 0,
            cards_played: 0,
            attacks_played: 0,
            skills_played: 0,
            exhausted_this_turn: 0,
            hp_lost_this_turn: 0,
            free_attack: 0,
            turn: 1,
            potions: [0; MAX_POTIONS],
            potion_slots: 3,
            enemy_hist: [[u8::MAX; ENEMY_HIST]; MAX_ENEMIES],
            enemy_used: [0; MAX_ENEMIES],
            rng: Rng::new(seed),
            pending: Pending::None,
            player_dead: false,
            combat_over: false,
        }
    }

    pub fn add_card(&mut self, id: u16, flags: u16, bonus: i16) -> u8 {
        let ix = self.n_cards;
        self.cards[ix as usize] = CardInst { id, flags, bonus, cost_delta: 0, block_bonus: 0 };
        self.n_cards += 1;
        self.draw[self.n_draw as usize] = ix;
        self.n_draw += 1;
        ix
    }

    pub fn add_enemy(&mut self, def: u16, hp: i32) -> usize {
        let i = self.n_enemies as usize;
        self.enemies[i] = Entity::new(hp);
        self.enemy_def[i] = def;
        self.enemy_move[i] = 0;
        self.enemy_used[i] = 0;
        self.n_enemies += 1;
        i
    }

    /// **「还在场上」不等于「活着」。**
    ///
    /// 带适生力的实验体被砍死之后，血量是 0、打不到、观测里也看不见它，
    /// 但战斗**不结束** —— 它要等到自己的回合才复苏成下一个形态
    /// （[源码] `AdaptablePower.ShouldStopCombatFromEnding() => true`
    /// 和 `ShouldCreatureBeRemovedFromCombatAfterDeath() => false`）。
    ///
    /// 判定胜利只能用这个函数，不能用 `alive()` 逐个数 ——
    /// 用后者的话砍掉第一条命就直接判赢了。
    #[inline]
    pub fn any_enemy_present(&self) -> bool {
        (0..self.n_enemies as usize)
            .any(|i| self.enemies[i].alive() || self.enemies[i].get(St::Adaptable) > 0)
    }

    #[inline]
    pub fn any_enemy_alive(&self) -> bool {
        (0..self.n_enemies as usize).any(|i| self.enemies[i].alive())
    }

    /// First living enemy, used as the implicit target for convenience.
    #[inline]
    pub fn first_alive(&self) -> Option<usize> {
        (0..self.n_enemies as usize).find(|&i| self.enemies[i].alive())
    }

    /// 弃牌堆洗回抽牌堆。**抽牌和破灭那类「翻抽牌堆顶」共用这一份** ——
    /// 洗牌逻辑只该有一处，两处各写一遍迟早会长歪。
    /// 弃牌堆空了就什么都不做（不是错误）。
    pub fn reshuffle_discard_into_draw(&mut self) {
        if self.n_disc == 0 {
            return;
        }
        for i in 0..self.n_disc as usize {
            self.draw[i] = self.disc[i];
        }
        self.n_draw = self.n_disc;
        self.n_disc = 0;
        // 洗完之后一张确定的都没有了
        self.n_draw_known = 0;
        self.regularize_and_shuffle_draw();
    }

    /// 抽牌堆就地洗一次。**所有洗牌都走这一份** —— 规范化排序 + Fisher-Yates，
    /// 少一处就会让"相同的牌堆多重集 + 相同种子"洗出不同的结果，
    /// 而配对随机数比较（CRN）和回放验证正是靠这条性质站住的。
    ///
    /// 调用方负责 `n_draw_known`：洗完当然一张确定的都没有了，但把它写在这里
    /// 会让"只想换个顺序"的调用方无从表达。
    fn regularize_and_shuffle_draw(&mut self) {
        // 先按卡牌指纹规范化排序，保证相同的弃牌多重集在相同 RNG 下产生绝对一致的洗牌结果（CRN）
        let n = self.n_draw as usize;
        let cards = &self.cards;
        self.draw[..n].sort_unstable_by_key(|&ix| {
            let c = cards[ix as usize];
            ((c.id as u64) << 32) | ((c.flags as u64) << 16) | (c.bonus as u16 as u64)
        });
        // Fisher-Yates using the shuffle stream
        for i in (1..n).rev() {
            let j = next_below(&mut self.rng.shuffle, i + 1);
            self.draw.swap(i, j);
        }
    }

    /// 把**手牌 + 弃牌堆**并进抽牌堆，整体洗一次（瓶装潜能）。
    ///
    /// [源码] `BottledPotential.OnUse` 是两条命令：先 `CardPileCmd.Add(手牌, Draw)`，
    /// 再 `CardPileCmd.Shuffle`。而那个 `Shuffle` **把弃牌堆和抽牌堆一起洗**
    /// （`CardPileCmd.cs`：取弃牌堆的 list、`AddRange(抽牌堆)`、`StableShuffle`，
    /// 洗完整体塞回抽牌堆）。所以卡面那句"你的所有牌"是字面意思 ——
    /// 手牌、弃牌堆、抽牌堆合成一堆。**消耗堆不动**（那些牌已经离场）。
    pub fn shuffle_all_into_draw(&mut self) {
        for i in 0..self.n_hand as usize {
            let c = self.hand[i];
            if (self.n_draw as usize) < MAX_CARDS {
                self.draw[self.n_draw as usize] = c;
                self.n_draw += 1;
            }
        }
        self.n_hand = 0;
        for i in 0..self.n_disc as usize {
            let c = self.disc[i];
            if (self.n_draw as usize) < MAX_CARDS {
                self.draw[self.n_draw as usize] = c;
                self.n_draw += 1;
            }
        }
        self.n_disc = 0;
        self.n_draw_known = 0;
        self.regularize_and_shuffle_draw();
    }

    pub fn draw_one(&mut self) {
        // 战斗专注「你在本回合内不能再抽任何牌」。放在**最前面**：连洗牌都不该发生，
        // 否则一次被禁止的抽牌仍然会推动 shuffle 流，让同种子的复现对不上。
        if self.player.get(St::NoDraw) > 0 {
            return;
        }
        if self.n_draw == 0 {
            self.reshuffle_discard_into_draw();
        }
        if self.n_draw == 0 || self.n_hand as usize >= MAX_HAND {
            return;
        }
        let Some(c) = self.pop_draw_top() else { return };
        self.hand[self.n_hand as usize] = c;
        self.n_hand += 1;
    }

    pub fn draw_n(&mut self, n: i32) {
        for _ in 0..n {
            self.draw_one();
        }
    }

    /// Remove hand slot `i`, returning the card index.
    pub fn take_from_hand(&mut self, i: usize) -> u8 {
        let c = self.hand[i];
        for k in i..(self.n_hand as usize - 1) {
            self.hand[k] = self.hand[k + 1];
        }
        self.n_hand -= 1;
        c
    }

    pub fn to_discard(&mut self, c: u8) {
        self.disc[self.n_disc as usize] = c;
        self.n_disc += 1;
    }

    pub fn to_exhaust(&mut self, c: u8) {
        self.exh[self.n_exh as usize] = c;
        self.n_exh += 1;
        self.exhausted_this_turn += 1;
    }

    pub fn take_from_disc(&mut self, i: usize) -> u8 {
        let c = self.disc[i];
        for k in i..(self.n_disc as usize - 1) {
            self.disc[k] = self.disc[k + 1];
        }
        self.n_disc -= 1;
        c
    }

    /// **手牌满 10 张时这一张就进不来了**（`MAX_HAND`）。
    ///
    /// **调用方注意**：这里只是"没放进手牌"，它不负责把牌送去别的地方。
    /// 所有调用点都必须在**把牌从原来的牌堆拿走之前**先检查 `n_hand`，
    /// 否则那张牌会凭空消失。目前四个调用点都是这么写的。
    ///
    /// **欠定**：手牌满时游戏把这张牌送去哪（弃牌堆？原地不动？）语料里
    /// 一个样本都没有 —— 唯一钉死的是**抽牌**那条（留在抽牌堆）。
    pub fn to_hand(&mut self, c: u8) {
        if (self.n_hand as usize) < MAX_HAND {
            self.hand[self.n_hand as usize] = c;
            self.n_hand += 1;
        }
    }

    /// 放到抽牌堆**顶**。放上去的这一张身份是确定的，`n_draw_known` +1。
    pub fn to_draw_top(&mut self, c: u8) {
        if (self.n_draw as usize) < MAX_CARDS {
            self.draw[self.n_draw as usize] = c;
            self.n_draw += 1;
            self.n_draw_known += 1;
        }
    }

    /// 从抽牌堆**顶**拿走一张。
    ///
    /// **所有"取牌堆顶"的路径都必须走这里** —— 抽牌、余烬（消耗牌堆顶）、
    /// 破灭/倾泻（打出牌堆顶）。和 `exhaust_card`、`spawn_card` 是同一个套路：
    /// 收口的理由是 `n_draw_known`，少更新一处，planner 就会把一张其实已经
    /// 不确定的牌当成确定的 —— 而那种错**不会报错，只会让搜索信一个假前提**。
    ///
    /// 它**不管洗牌**：什么时候该把弃牌堆洗回来是调用方的语义
    ///（抽牌要洗、余烬也要洗，但两者的时机判断写在各自那边）。
    pub fn pop_draw_top(&mut self) -> Option<u8> {
        if self.n_draw == 0 {
            return None;
        }
        self.n_draw -= 1;
        if self.n_draw_known > 0 {
            self.n_draw_known -= 1;
        }
        Some(self.draw[self.n_draw as usize])
    }

    /// 往抽牌堆的**随机位置**插一张（[源码] `CardPilePosition.Random`，
    /// 噪音机器人的第二张眩晕走这条）。
    ///
    /// 和 [`State::to_draw_top`] 的区别是真实的：塞到顶上等于"下一张必抽到"，
    /// 塞到随机位置只是"这一局某个时候会抽到"。噪音机器人要的是后者。
    ///
    /// 用 `rng.gen` 流（不变量 4 的三条分流之一）—— **不是 `shuffle` 流**：
    /// 混进去会让"敌人多塞一张牌"改掉我后面所有的抽牌顺序，
    /// 那正是分流要防的那件事。
    pub fn to_draw_random(&mut self, c: u8) {
        let n = self.n_draw as usize;
        if n >= MAX_CARDS {
            return;
        }
        let at = next_below(&mut self.rng.gen, n + 1);
        self.draw.copy_within(at..n, at + 1);
        self.draw[at] = c;
        self.n_draw += 1;
        // 插进了已知区里面 ⇒ 已知区只剩插入点**以上**那几张。
        // 插在已知区下面则不受影响。
        let above = (n - at) as u8;
        if self.n_draw_known > above {
            self.n_draw_known = above;
        }
    }
}
