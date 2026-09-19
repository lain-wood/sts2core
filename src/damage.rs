//! The single damage pipeline. Everything that deals damage goes through here.
//!
//! Multiplier order was reverse-engineered from observed play (Act 1-3 logs):
//!
//!   1. card base (+upgrade)
//!   2. 腐化 (Corrupt) x1.5   -- on the card's base, BEFORE strength
//!   3. + 锋利 flat bonus, + Strength, + 活力 (Vigor)
//!   3.5 attacker 缩小 (Shrink)   x0.7   -- [游戏文本+源码]，位置未被数据钉死
//!   4. attacker 虚弱 (Weak)      x0.75
//!   5. defender 易伤 (Vulnerable) x1.5
//!   6. defender 缓慢 (Slow)      x(1 + 0.1 * cards played before this one)
//!   7. 难以杀灭 (DamageCap)  min(d, cap)
//!   8. 无实体 (Intangible)   d = 1
//!   9. block absorbs, remainder hits HP —— 硬化外壳（本回合累计余额）和滑溜（压成 1）
//!      封的都是**这一截**，格挡照常被打满，见 `absorb`
//!
//! **步骤 3.5-6 的乘区累乘，只在最后取整一次**（不是每步各取一次）。
//! 见 `apply_modifiers` 的注释：怨恨那一帧证伪了逐步取整。
//!
//! NOTE: this is an empirical model. `tools/replay_check` (phase 2) diffs it
//! against the real game and is the authority; do not "fix" numbers here by
//! intuition without a replay that disagrees.

use crate::state::{Entity, St};

/// [源码] StrikeDummy: additive bonus before Weak/Vulnerable, for tagged attacks.
pub fn tagged_attack_bonus(attacker: &Entity, strike: bool) -> i32 {
    if strike { attacker.get(St::StrikeDummy) } else { 0 }
}

/// Steps 1-3: the number the card *displays* in hand. Target-independent.
///
/// `vigor` 是活力（[源码] `VigorPower.ModifyDamageAdditive`）。它和力量同一档
/// 加法项，但**消耗规则完全不同** —— 力量常驻，活力打完一条 `AttackCommand`
/// 就整个清零。清零那一步不在这里，在 `step::resolve_ops`：本函数是纯的，
/// 只负责"这一段该加多少"。
///
/// 调用方必须保证只在**有源攻击**上传非 0 的 `vigor`
/// （`if (!props.IsPoweredAttack()) return 0m;`）—— 药水/遗物伤害传 0。
/// `mul` 是**作用在基础值上、在力量之前**的那一档乘区：老的腐化 flag
/// （`F_CORRUPT`）和附魔的 `EnchantDamageMultiplicative`（腐化 3/2、直觉 2/1）
/// 走的是同一个口子。传 `(1, 1)` = 不乘。
///
/// 位置为什么在力量之前：[源码] `EnchantmentModel.EnchantDamageMultiplicative`
/// 的文档写着 "This hook runs BEFORE all other damage modification hooks"，
/// 而力量走的是普通的 `ModifyDamageAdditive`。腐化那一版本来就是这么建的
/// （`docs/design-l1.md` 伤害管线的第 2 步），这里只是把它从一个 `bool` 推广成一对整数。
#[inline]
pub fn card_face_damage(base: i32, mul: (i32, i32), bonus: i32, strength: i32, vigor: i32) -> i32 {
    let mut d = base;
    if mul != (1, 1) {
        d = d * mul.0 / mul.1;
    }
    d + bonus + strength + vigor
}

/// Steps 4-8: apply attacker/defender modifiers to a face value.
///
/// **所有乘区累乘成一个有理数，最后只取整一次。**
///
/// 这里原来是逐个乘区各自向下取整，`damage.rs` 和 `CLAUDE.md` 都写着
/// "每一步都向下取整，这很关键" —— **那句话是错的**。
/// 2026-08-15 第1幕 Boss 那一帧把它证伪了：我带虚弱、神官带易伤，
/// 怨恨（基础 5，攻击两次）实际打出 10：
///
/// ```text
/// 逐步取整   floor(5×0.75)=3  -> floor(3×1.5)=4   两下共 8   ✘
/// 累乘一次   floor(5 × 0.75 × 1.5) = floor(5.625) = 5  共 10  ✔
/// ```
///
/// 在此之前所有验过的读数（16/18 那组、拆卸+ 双击 16、御血术+ 36、
/// 缩小 5、缓慢 11）两种算法给出的结果**完全相同** —— 只有基础值小到
/// 让中间结果产生小数的牌才分得开。怨恨的 5 恰好是那个数。
#[inline]
pub fn apply_modifiers(face: i32, attacker: &Entity, defender: &Entity) -> i32 {
    // 污染（[源码] `TaintedPower.ModifyDamageAdditive`）：防御方每层污染，
    // 让**这一次命中**多吃 1 点。加法项，和力量同一档，所以在乘区之前、
    // **也在夹 0 之前** —— [实测] 第2幕第31层：基础 5、力量 −9、污染 2，
    // 意图标签是 `0×3`，即 `5 − 9 + 2 = −2` 先求和再夹 0。
    // 夹完再加会得到 2×3，和观测对不上。
    //
    // **只有预测路径会走到这里。** 默认对拍和 L2 的 `Threat` 走
    // `injected_enemy_turn`，那条路拿观测到的意图标签**直接**扣血、不过本函数，
    // 而标签本来就含污染（实测：打出一张技能后标签当场 15 -> 17）。
    // 所以这一行不会造成重复计数 —— 这正是两条路径分开的价值，
    // 参见 `St::Tainted` 里那段"建了会重复计数"的分析。
    let mut d = face + defender.get(St::Tainted);
    if d < 0 {
        d = 0;
    }
    // 乘区累乘：分子分母各自累积，除法只做一次。
    // i64 是为了不溢出：面板值 × 各乘区分子最坏也就几十万，离 i64 远得很。
    let mut num: i64 = 1;
    let mut den: i64 = 1;
    // 缩小 ×0.7 —— **2026-08-21 从 ×2/3 改过来的，理由见下**。
    //
    // 原来写 ×2/3，注释说"四个观测同时满足"，那句话是真的**但不构成证据**：
    //   打击 6→4      6×0.7=4.2→4   6×2/3=4.0→4    一样
    //   痛击 8→5      8×0.7=5.6→5   8×2/3=5.33→5   一样
    //   打击带易伤 6→6  6×0.7×1.5=6.3→6  6×2/3×1.5=6.0→6  一样
    //   无 debuff 8→8  不带缩小                      不适用
    // **四个样本在两种倍率下取整后完全相同**，所以它们从来没有钉死 ×2/3 ——
    // 那是当时从两个自洽解里挑的一个。这正是 `../CLAUDE.md` 第 2、4 条
    //（"欠定就留空"/"和所有已知数据一致不等于对"）说的那个坑，
    // 而且和乘区取整那个 bug 是同一个形状：藏得住，因为样本分不开。
    //
    // 改的依据是**两个独立来源一致**：
    //   [游戏] 缩小的状态说明原文：「缩小甲虫存活时，你的攻击伤害减少30%」
    //   [源码] `ShrinkPower.ModifyDamageMultiplicative` -> `(100 - 30) / 100`
    //
    // **仍然没有一个实测样本能把 0.7 和 2/3 分开**，别把这行当成实测。
    // 要分开需要缩小状态下打一张**基础值 ≥ 10** 的攻击牌：
    //   base 10  →  0.7 得 7，2/3 得 6      ← 差 1，对拍当场会判
    //   base 13  →  0.7 得 9，2/3 得 8
    // 现有牌组最大基础值是 9（剑柄打击/突破），9×0.7=6.3 和 9×2/3=6.0 都落到 6，
    // 所以这一局取不到判决样本。**下次在缩小状态下拿到大牌，第一时间打一张。**
    //
    // 位置（排在易伤之前、作用在基础值+力量上）**依然没有被数据钉死**，
    // 和倍率是两件事，别一起当成已验证。
    if attacker.get(St::Shrink) != 0 {
        num *= 7;
        den *= 10;
    }
    if attacker.get(St::Weak) > 0 {
        num *= 3;
        den *= 4;
    }
    // 钢笔尖：**这一次出牌是第 10 张攻击** ⇒ ×2。
    //
    // [源码] `PenNib.ModifyDamageMultiplicative => 2m`，带 `IsPoweredAttack()` 的门
    // （所以它和上下这几条一样只在本函数里，药水/遗物那条路不吃）。
    // 标记怎么上、什么时候摘，见 `St::PenNibArmed`。
    //
    // **它是个乘区，不是"打完再翻倍"** —— 和别的乘区一起累乘、最后只取整一次。
    // [实测] 2026-09-06 `act3_f46_elite_soul_nexus` 帧32：格挡 6 + 力量 1 = 7，
    // 带虚弱 ⇒ 7 × 3/4 × 2 = 10.5 -> **10**，游戏正是 10。
    // （这一帧两种写法都给 10：⌊7×0.75⌋×2 也是 10。**分不开，照源码写。**
    //   要分开需要一个 `base×0.75` 的小数部分 ≥ 0.5 的样本。）
    if attacker.get(St::PenNibArmed) > 0 {
        num *= 2;
    }
    if defender.get(St::Vulnerable) > 0 {
        num *= 3;
        den *= 2;
    }
    // 巨像：**攻击者**带易伤时，防御方受到的伤害减半。
    // [源码] `ColossusPower.ModifyDamageMultiplicative`：条件是
    // `dealer.HasPower<VulnerablePower>()`，注意判的是**攻击者**身上的易伤，
    // 不是防御方的 —— 读反了会变成"我给敌人上易伤反而自己少挨打"。
    //
    // 位置：游戏把所有 `ModifyDamageMultiplicative` 放在同一个循环里连乘、
    // 中间不取整，所以它和易伤/缓慢谁先谁后**不影响结果**（乘法可交换，
    // 而本函数也只在最后取整一次）。放这里纯粹是就近。
    if defender.get(St::Colossus) > 0 && attacker.get(St::Vulnerable) > 0 {
        den *= 2;
    }
    // 翱翔/飞行：[源码] `SoarPower.ModifyDamageMultiplicative` -> 受到有源攻击伤害减少 50%
    if defender.get(St::Soar) > 0 {
        den *= 2;
    }
    // 遭到包围 + 后方攻击：[源码] `SurroundedPower` 判的是"攻击者在不在我背后"，
    // 而"背后"由我的朝向决定。内核把这三样都做成 status，所以这里能就地判：
    // 防御方带 `Surrounded`，且**攻击者所在的一侧和我面朝的一侧相反** ⇒ ×1.5。
    //
    // [实测] 2026-08-27 第 2 幕 Boss 帧25 -> 帧28：同一只、同一手，
    // 我转身之后标签 21 -> 14。**这是一对只差朝向的样本**，别的都没动。
    //
    // 位置和别的乘区一样在累乘段里（游戏把所有 `ModifyDamageMultiplicative`
    // 放同一个循环里连乘、最后才取整），所以和易伤/虚弱谁先谁后不影响结果。
    if defender.get(St::Surrounded) > 0 {
        let facing_right = defender.get(St::FacingRight) > 0;
        let from_behind = if facing_right {
            attacker.get(St::BackAttackLeft) > 0
        } else {
            attacker.get(St::BackAttackRight) > 0
        };
        if from_behind {
            num *= 3;
            den *= 2;
        }
    }
    // 扑翼：[源码] `FlutterPower.ModifyDamageMultiplicative` —— `DamageDecrease = 50`，
    // 返回 `50/100`，而且和上面几条一样带 `IsPoweredAttack()` 的门。
    // 结构上和翱翔逐字同构，所以就放在它旁边。
    //
    // [实测] 2026-08-27 第2幕第20层：突破+（面板 13）打带易伤2、扑翼5 的偷窃草蜢，
    // 游戏掉 9 血。13 × 3/2 × 1/2 = 9.75 -> 取整 9 ——
    // **这一帧同时印证了"乘区累乘、最后只取整一次"**：
    // 逐步取整会得到 (13×1.5=19) ×0.5 = 9 也是 9，但换成 21 的那一手就分得开了。
    if defender.get(St::Flutter) > 0 {
        den *= 2;
    }
    let slow = defender.get(St::Slow);
    if slow > 0 {
        num *= (100 + slow) as i64;
        den *= 100;
    }
    // 唯一一次取整。d 非负，整数除法即向下取整。
    d = ((d as i64) * num / den) as i32;

    let cap = defender.get(St::DamageCap);
    if cap > 0 && d > cap {
        d = cap;
    }
    if defender.get(St::Intangible) > 0 {
        d = 1;
    }
    d
}

/// **不吃乘区的那一档伤害**（[源码] `ValueProp.Unpowered`：
/// "Damage from relics, potions, and powers"）。
///
/// 判据在 [源码] `ValuePropExtensions.IsPoweredAttack`：
/// `props.HasFlag(Move) && !props.HasFlag(Unpowered)`。
/// 易伤 / 虚弱 / 缓慢 / 缩小 / 巨像 **五个乘区各自的第一句就是这条判断**，
/// 所以药水、遗物、能力牌打出的伤害一概不过它们。
///
/// 但 **难以杀灭和无实体照吃** —— 它们走的是 `ModifyDamageCap` /
/// `ModifyHpLostAfterOsty`，源码里只判 `target == Owner`，**没有**那条 gate。
/// 这个区别不是细节：外骨骼虫的上限 9 对火焰药水一样有效。
///
/// > **2026-08-22 实战抓到的真错。** 在此之前所有伤害都走 `apply_modifiers`，
/// > 注释里写着"照常吃防御方的易伤/缓慢 —— 推断，未实测"。
/// > 第2幕第27层精英那一帧判了它：蜂群术士带易伤 2，火焰药水卡面 20，
/// > **游戏打 20，内核打 30**。补进蜂群术士之后这一帧才从"内容缺失"
/// > 变成真的被算 —— 又一次「补内容会让以前被掩盖的行为缺口现形」。
#[inline]
pub fn apply_modifiers_unpowered(face: i32, defender: &Entity) -> i32 {
    let mut d = if face < 0 { 0 } else { face };
    let cap = defender.get(St::DamageCap);
    if cap > 0 && d > cap {
        d = cap;
    }
    if defender.get(St::Intangible) > 0 {
        d = 1;
    }
    d
}

/// Step 9. Returns damage that actually reached HP (used for on-HP-loss hooks).
///
/// **滑溜在这里，不在 `apply_modifiers` 里**（[源码] `SlipperyPower` 只实现
/// `ModifyHpLostAfterOsty`，而无实体**另外**还有 `ModifyDamageCap`）。
/// `CreatureCmd.Damage` 的顺序是 `DamageBlockInternal`（扣格挡）->
/// `Hook.ModifyHpLost`（封顶）-> `LoseHpInternal`，所以：
///
/// * 格挡**照常被打满** —— 26 点打在 8 点格挡上，格挡照样清零
/// * 漏过格挡的那一截压成 **1**，`UnblockedDamage` 也是压完之后的那个 1
///   （源码的 `wasFullyBlocked` 判的就是压完之后的值）
///
/// 建在伤害那一格的话它的格挡永远掉不下去，而那是**这一整场仗的节奏**。
///
/// **硬化外壳也在这里**，而且排在滑溜**前面** —— 两者都是「扣完格挡之后封掉血」，
/// 顺序照 [源码] `Hook.ModifyHpLost` 的四档：`BeforeOsty` -> `BeforeOstyLate`（硬化外壳）
/// -> `AfterOsty`（滑溜）-> `AfterOstyLate`。今天没有一只敌人两个都带，顺序只是照抄。
///
/// 硬化外壳封的是**这个回合累计**的掉血：`min(漏过格挡的, 余额)`，然后余额减掉**实际掉的**
/// （[源码] `AfterDamageReceived` 加的是 `result.UnblockedDamage`，封过之后的数）。
/// 门是上限 [`St::HardenedShellCap`] 而不是余额 —— 余额扣到 0 正是它最要紧的时候，
/// 拿余额当门的话，额度一用完外壳就「消失」了。
#[inline]
pub fn absorb(defender: &mut Entity, dmg: i32) -> i32 {
    if dmg <= 0 {
        return 0;
    }
    let blocked = dmg.min(defender.block);
    defender.block -= blocked;
    let mut through = dmg - blocked;
    let shell = defender.get(St::HardenedShellCap) > 0;
    if shell {
        through = through.min(defender.get(St::HardenedShell).max(0));
    }
    if through > 1 && defender.get(St::Slippery) > 0 {
        through = 1;
    }
    defender.hp -= through;
    if shell {
        defender.add(St::HardenedShell, -through);
    }
    through
}

/// Block gained from a card, after 敏捷 (Dexterity) and 脆弱 (Frail).
///
/// 收 `&mut` 是为了**臂甲**：它要在这里把自己的充能用掉。放在这条管线里
/// 而不是调用点，是因为「从卡牌获得的格挡」只有这一个入口 ——
/// 药水走 `Source::Potion` 绕开它、能力牌走 `TOp::OwnerBlock` 也绕开它，
/// 于是「臂甲只翻倍卡牌格挡」这条自动成立，不用在三个地方各写一遍。
#[inline]
pub fn card_block(base: i32, owner: &mut Entity) -> i32 {
    let mut b = base + owner.get(St::Dexterity);
    if b < 0 {
        b = 0;
    }
    if owner.get(St::Frail) > 0 {
        b = b * 3 / 4;
    }
    // 臂甲：本场第一次从卡牌获得的格挡翻倍，用掉即失效。
    //
    // [实测] 2026-08-17 第2幕31层 帧1：防御+（基础 8）给出 **16**。
    // [未实测] **和脆弱的先后顺序**：那一帧我没带脆弱，分不开。
    //   base 5 带脆弱1：先脆弱再翻倍 = floor(5×3/4)×2 = 6
    //                   先翻倍再脆弱 = floor(10×3/4) = 7
    //   选了给 6 的那个 —— 两条都说得通时取**不高估玩家**的那边。
    //   将来带着脆弱打出第一张格挡牌，对拍会当场报出来。
    if b > 0 && owner.get(St::VambraceCharge) > 0 {
        owner.add(St::VambraceCharge, -1);
        b *= 2;
    }
    // 坚定不移：**每回合前 N 次**从卡牌获得的格挡翻倍（N = 层数）。
    //
    // [源码] `UnmovablePower.ModifyBlockMultiplicative` 判的是
    //
    //     int num = History.Entries.OfType<BlockGainedEntry>()
    //         .Count(e => e.HappenedThisTurn(...) && e.Actor == target
    //                     && e.Props.IsCardOrMonsterMove() && e.CardPlay != cardPlay);
    //     if (num >= base.Amount) return 1m;  else return 2m;
    //
    // —— 它数的是**本回合此前从卡牌拿过几次格挡**，是个**往上数的计数器**，
    // 不是"回合开始上膛的余额"。
    //
    // **原来那版是错的**（2026-08-30 第 3 幕第 43 层实录帧 18 抓到：
    // 挑衅+ 游戏给 12、内核给 6）：余额由 `POWERS` 的 `TurnStart` 规则重置，
    // 于是**回合中途才打出坚定不移**时余额恒为 0，那一整回合一次都不翻倍。
    // 这一帧以前一直是「内容缺失」被跳过（青蛙骑士不在表里），
    // 补完敌人才露出来 —— **内容覆盖率 ≠ 行为覆盖率**的又一例。
    //
    // 计数器 `UnmovableCharge` 要**无条件**地数、**无条件**地在我的回合开始清零
    //（它在 `TURN_SCOPED` 里）。不能只在身上有坚定不移时才维护：
    // 上一回合攒下的数会跨到"这一回合中途才拿到坚定不移"的那个回合里去。
    //
    // 门是 `props.IsCardOrMonsterMove()`，正好是这个函数 —— 所以药水格挡
    // （走 `Source::Potion`）和能力牌格挡（`TOp::OwnerBlock`）既不翻倍也不计数，
    // 和臂甲一样是自动成立的。
    //
    // **两件一起带时是 ×4**：游戏侧所有 `ModifyBlockMultiplicative` 连乘，
    // 这里两个 `if` 顺序执行等价。**未实测**。
    //
    // **一处已知的简化**：源码那个 `e.CardPlay != cardPlay` 把**同一次出牌**
    // 产生的格挡排除在计数之外（一张牌给两段格挡时两段都翻倍），
    // 内核按调用次数数，第二段会被第一段挡住。目前没有这样的牌，
    // 有了就会在对拍里当场露出来。
    if b > 0 {
        if owner.get(St::UnmovableCharge) < owner.get(St::Unmovable) {
            b *= 2;
        }
        owner.add(St::UnmovableCharge, 1);
    }
    b
}

/// 人工制品 (Artifact) absorbs one *application* of a debuff, not one stack.
/// Returns true if the debuff was blocked.
#[inline]
pub fn artifact_absorbs(target: &mut Entity, is_debuff: bool) -> bool {
    if is_debuff && target.get(St::Artifact) > 0 {
        target.add(St::Artifact, -1);
        true
    } else {
        false
    }
}

#[inline]
pub fn is_debuff(s: St) -> bool {
    // 恶咒 / 抑制（骑士团）在 [源码] 里是 `PowerType.Debuff` —— 人工制品挡得住。
    // 缠结（藤蔓蹒跚者）同样是 `PowerType.Debuff`。
    // 这个函数今天只喂人工制品的判定（回合末掉层走 `step::decay` 自己那张表）。
    matches!(s, St::Vulnerable | St::Weak | St::Frail | St::Slow | St::Hex | St::Dampen | St::Tangled)
}

/// **施加这一笔**算不算 debuff（人工制品挡不挡）—— 看的是数值，不只是种类。
///
/// [源码] `ArtifactPower` 判的是 `canonicalPower.GetTypeForAmount(amount)`，而
/// `PowerModel.GetTypeForAmount` 对 `Counter` 型且 `AllowNegative` 的 power
/// （力量 / 敏捷）**给负数时返回 Debuff**。所以「减力量 / 减敏捷」挡得住，
/// 「加力量」挡不住。
///
/// 所有施加 status 的路径（卡牌、触发器、敌人招式）都走这一个判据 ——
/// 2026-09-13 之前敌人招式那条只看种类，于是失落之物偷力量时人工制品不起作用。
#[inline]
pub fn is_debuff_amount(s: St, amt: i32) -> bool {
    is_debuff(s) || (matches!(s, St::Strength | St::Dexterity) && amt < 0)
}

/// **敌人招式**给的格挡（`EOp::Block`），过它自己的敏捷。
///
/// [源码] `DexterityPower.ModifyBlockAdditive` 的门是
/// `props.IsPoweredCardOrMonsterMoveBlock()` —— 敌人招式的格挡（`ValueProp.Move`）
/// 一样吃敏捷。今天唯一带敏捷的敌人是遗忘之物（瘴气先加格挡、后偷敏捷，
/// 于是 8 -> 10 -> 12…）。
///
/// **脆弱没有放进来**：内核里没有任何东西给敌人上脆弱，照抄一个没有消费者的
/// 乘区等于加一个空钩子。有了再加。
#[inline]
pub fn monster_block(base: i32, owner: &Entity) -> i32 {
    (base + owner.get(St::Dexterity)).max(0)
}
