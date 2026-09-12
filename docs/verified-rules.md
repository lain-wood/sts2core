# 实测规则手册 —— 现在认定为真的那些

**这份文件只写"现在是什么"，一条流水账都不记。** 每条规则被哪一帧钉死、
当时读数多少、我先做错了什么，全在 [`verification-log.md`](verification-log.md)。
设计和接口在 [`../CLAUDE.md`](../CLAUDE.md)。

三条读法（和整个项目同一条）：

* **真实游戏是唯一权威。** wiki 和卡面文本都只是假设，冲突时以实测为准。
* **「和所有已知数据一致」不等于对。** 只说明现有样本分不开它和别的解。
  要主动去找**能分开假设**的样本 —— 下面「乘区取整」那条就是这么翻的车。
* **来源档次要跟着数字走**：`[实测]` 真实游戏判过 · `[源码]` 反编译 ·
  `[游戏文本]` 状态说明/卡面 · `[玩家]` 用户凭游戏经验给的判定。
  档次不同不代表可信度高低 —— `[玩家]` 在触发类交互上**高于**卡面文本。

**改这里的任何一个数字之前，先在 verification-log 里找到钉死它的那一帧。**

---

## 1. 伤害管线

唯一实现在 `damage.rs`，所有造成伤害的东西都走它。乘区顺序：

| # | 乘区 | 值 | 档次 |
|---|---|---|---|
| 1 | 卡面基础值（+升级） | — | |
| 2 | **附魔的伤害乘区** / 腐化 Corrupt | ×1.5（腐化）· ×2（直觉），**加在基础值上、力量之前** | `[源码]` |
| 3 | + **附魔的伤害加值**（锋利=`Amount`）· + 力量 · + 活力 | 加法同档 | `[源码]` |
| 3.5 | 攻击方 缩小 Shrink | **×0.7** | `[游戏文本]`+`[源码]`，**位置未钉死** |
| 4 | 攻击方 虚弱 Weak | ×0.75 | `[实测]` |
| 4.5 | **钢笔尖**（这一次出牌是第 10 张攻击） | **×2** | `[源码]`+`[实测]` |
| 5 | 防御方 易伤 Vulnerable | ×1.5 | `[实测]` |
| 6 | 防御方 缓慢 Slow | ×(1 + 0.1 × 本回合此牌**之前**打出的牌数) | `[实测]` |
| 7 | 难以杀灭 DamageCap | min(d, cap) | `[实测]` |
| 8 | 无实体 Intangible | d = 1 | `[实测]` |
| 9 | 格挡吸收，余数进 HP | 逐次吸收、溢出 | `[实测]` |

第 2 步那一格 2026-09-06 从"腐化"推广成"附魔的伤害乘区"：
[源码] `EnchantmentModel.EnchantDamageMultiplicative` 的文档写着
"runs BEFORE all other damage modification hooks"，而力量走的是普通的
`ModifyDamageAdditive` —— 和腐化那一版本来就在同一个位置，只是从一个 `bool`
推广成一对整数（`damage::card_face_damage` 的 `mul` 参数）。

### 1.0 钢笔尖：第 10 张攻击 ×2，**是个乘区**

`[源码]` `PenNib.ModifyDamageMultiplicative => 2m`，带 `IsPoweredAttack()` 门
（所以药水/遗物伤害不吃）。计数在 `BeforeCardPlayed`、`AttacksPlayed % 10`，
数到 0 的那一次给**这一张牌**挂上 `AttackToDouble` ⇒ **翻倍的是第 10 张自己**。
计数器带 `[SavedProperty]`，**跨战斗保留**。

`[实测]` 2026-09-06 `act3_f46_elite_soul_nexus`：遗物面板计数 8 -> 9 -> **0**，
第 10 张（全身撞击+）游戏打 10 而内核当时打 5。
那一帧格挡 6 + 力量 1 = 7，带虚弱 ⇒ `⌊7 × 3/4 × 2⌋ = 10`。

> **这一帧分不开"乘区"和"打完再翻倍"**（`⌊7×0.75⌋×2` 也是 10）。
> 照 [源码] 写成乘区。要分开需要一个 `base × 0.75` 小数部分 ≥ 0.5 的样本。

**它进的是 `damage.rs` 的乘区链，不是什么新表。** 遗物把状态交给玩家实体
（三个私有 status），`damage.rs` 照旧只认 status ——
「`ModifyXxx` 那族遗物没地方放」这条旧结论因此不成立了。

### 1.1 步骤 3.5–6 累乘，**只在最后取整一次**

`[实测]` 2026-08-15 第 1 幕 Boss，也是验证器建成以来第一个真 MISMATCH。
带虚弱、神官带易伤，怨恨（基础 5，攻击两次）实打 **10** 而不是 8：

```text
逐步取整   floor(5×0.75)=3 -> floor(3×1.5)=4    两下共 8   ✘
累乘一次   floor(5 × 0.75 × 1.5) = floor(5.625) = 5   共 10  ✔
```

在此之前**所有**验过的读数两种算法结果完全相同 —— 只有基础值小到让中间结果
出现小数的牌才分得开。测试 `multipliers_floor_once_at_the_end`。

### 1.2 乘区是**乘法**，不是加法

`[实测]` 同一回合两张相同的打击打出 16 和 18（易伤+缓慢）：
`10 × 1.5 × 1.1 = 16.5 → 16`，`10 × 1.5 × 1.2 = 18`。加法会给出 16 和 17。
测试 `multipliers_are_multiplicative_not_additive`。

### 1.3 缩小是 ×0.7，不是 ×2/3

`[游戏文本]` 「缩小甲虫存活时，你的攻击伤害**减少30%**」·
`[源码]` `ShrinkPower.ModifyDamageMultiplicative` → `(100 - 30) / 100`。

> **2026-08-21 从 ×2/3 改过来的。** 原来的注释写着「实测四个观测同时满足」——
> 那句话从来没成立过：那四个观测两种算法给出的数**完全相同**。
> **要分开需要缩小状态下打一张基础值 ≥ 10 的攻击牌**，那一局没拿到。
> 下次在缩小状态下拿到大牌，第一时间打一张。

它在管线里的**位置**（3.5，力量之后、虚弱之前）**没有任何数据钉死**。

### 1.4 敌人意图标签是**最终伤害值**

`[实测]` 攻击方和防御方的乘区**都已经算进去了**。`replay.rs::inject_enemy_attacks`
直接把标签数字扣进玩家、不再过任何乘区，依据就是这个。四个独立证据：

* 攻击方力量：立柱构造体（基础 7，力量 2 时标签 9、力量 4 时标签 11）；
  旧日雕像（基础 13，力量 10 时标签 23，实打 23）
* **防御方易伤**（2026-08-15 第 14 层，飞蝇菌子）：同一手攻击，我无易伤时标签
  `Attack:8`，它给我上易伤后同一手变成 `Attack:12` = 8×1.5；另一手标签
  `Attack:16`，我垫 10 点格挡，HP 27→21 ⇒ 总伤害正好 16（含易伤应是 24）。

这条从第一天悬到这里，因为之前的牌组**没有任何途径给自己挂易伤**。

### 1.5 格挡侧

| 规则 | 值 | 测试 |
|---|---|---|
| 脆弱 Frail | **卡牌**格挡 ×3/4 向下取整（防御 5 → 3） | `frail_scales_card_block_by_three_quarters` |
| 药水格挡 | **不吃脆弱**（脆弱 2 下格挡药水给满 12） | `potion_block_ignores_frail_but_card_block_does_not` |
| 多段攻击 | 格挡逐次吸收；**缓慢按「牌」不按「命中」** | `dismantle_doubles_on_vulnerable` |
| 力量 vs 缓慢 | **力量加在缓慢之前**。判据一次 11：`(6+2)×1.4 = 11.2 → 11` | `slow_ramps_per_card_and_strength_applies_before_it` |
| 缓慢计数 | **任何牌都计数**（防御也算）、**不含当前这张**、**药水不算牌**、回合开始归零 | `a_potion_is_not_a_card_for_slow_or_stomp` |
| 人工制品 | 吃**一次施加**，不是一层 debuff | `[实测]` |

> **观测层面的坑**：**手牌的 `description` 是带乘区实时渲染的**（脆弱下直接写
> 「获得3点格挡」），而牌堆里同一张牌仍写 5。
> **任何时候都不要去 parse 牌堆描述里的数字。**

### 1.6 手牌那句渲染文本反过来是个**观测量**，反推的办法是「正着算一遍」

内核有两个量观测里根本没有、又会跨帧累积：痛殴攒下的伤害加值（`CardInst::bonus`）
和扯碎的段数（`hp_loss_hits`）。`sync` 每帧从观测重建手牌，不反推就归零。
游戏把**算好的值**渲染进了手牌描述（`造成13点伤害两次`），那句话就是唯一的来源。

**渲染值 = 过完攻击方乘区、没过防御方乘区。** `[实测]` 同一场两帧钉死：

| 帧 | 渲染 | 我的 status | 敌人 status | 实打/次 |
|---|---|---|---|---|
| 3  | 7  | 力量1 | 易伤2 | 10 = ⌊7×1.5⌋ ⇒ **易伤没进渲染值** |
| 29 | 13 | 力量1 虚弱1 | — | 13 ⇒ **虚弱进了**（否则实打该是 ⌊13×0.75⌋ = 9）|

反推**不要做除法**：加值是个小非负整数，拿同一条伤害管线（攻击方 = 玩家本人 +
钢笔尖的预览标记，防御方 = 全 0 的中性实体）**正着**算一遍、逐个试过去就行。
`floor` 的原像最多两个整数，**欠定时取最小的那个**（方向是低估）。

`[实测]` 帧29 那一张真吃掉的是剑柄打击+（渲染 11 点），
而 `⌊(6+B+1)×3/4⌋ = 13` 解出来的 B 正好是 **11**，两边独立对上。

> 2026-09-06 之前这里是减法（`加值 = 渲染值 − 卡面 − 力量 − 活力`），
> **只在攻击方一个乘区都没有时成立**，带虚弱就整帧放弃。
> 正着算的另一半价值：**它永远和 `damage.rs` 同口径** ——
> 再进来一个攻击方乘区（钢笔尖就是），这里一个字都不用改。

### 1.7 钢笔尖的**预览**也在那句话里

`[实测]` 同一场帧31/32：同样 6 点格挡的全身撞击+，遗物计数 8 时渲染「造成5点伤害」、
计数 9 时渲染「造成10点伤害」。所以手牌里攻击牌的渲染值在计数到 9 时**就是翻倍后的数**
（[源码] `ModifyDamageMultiplicative` 里那条 `Pile.Type != Play && AttacksPlayed == 9`
正是给预览用的）。反推加值时不认这件事，会正好多读出一个卡面基础值。

---

## 2. 玩家判定（卡面读不出来的那些）

**卡面文本对触发类交互是欠定的。** 遇到「每当…时」/ 多目标 / 牌堆顺序这类判定，
**先问用户，不要挑一组自洽的解写进去** —— 他一次纠正过我三个凭卡面推的错结论。

| # | 判定 | 我原本会写错的 | 钉在哪 |
|---|---|---|---|
| 1 | **绯红披风 + 撕裂**：自伤**会**唤醒撕裂（放血流核心） | 为躲死循环写成"钩子不套钩子" | `crimson_mantle_wakes_up_rupture` |
| 2 | **闪电霹雳 + 凶恶**：**每个吃到易伤的敌人各触发一次** | 读不出来，干脆没做这张牌 | `vicious_fires_once_per_enemy_that_got_vulnerable` |
| 3 | **恶魔之焰 + 黑暗之拥**：**不会**烧掉新抽的牌 | 把正确的快照写法改成了循环 | `fiend_fire_does_not_burn_cards_drawn_by_dark_embrace` |
| 4 | **拳斗**：格挡 = **过完乘区**的伤害（易伤下 7→10 给 10） | 卡面基础值 7 | `pugilism_blocks_for_the_amplified_damage_not_the_face_value` |
| 5 | **全身撞击**：格挡**不消耗**，只是被读一下 | 转化成伤害后清零 | `body_slam_reads_block_without_consuming_it` |
| 6 | **火焰屏障**：**多段攻击每一段各反伤一次** | 整次攻击算一次 | `flame_barrier_thorns_every_hit_of_a_multi_hit_attack` |
| 7 | **灼伤 / 腐朽 / 瓦解**：那点伤害**走格挡** | 照 `LoseHp` 直接扣血 | `burn_hits_through_block_and_dazed_voids_itself` |
| 8 | **势不可当**（`[源码]` 定死）：每次获得格挡且**数值 > 0** 触发一次，打**随机**一个敌人，伤害 `Unpowered`（**不吃力量**） | 拍脑袋选一个 | `juggernaut_fires_on_block_but_not_on_zero_block`<br>`juggernaut_damage_ignores_strength`<br>`juggernaut_also_fires_on_block_from_a_power` |
| 9 | **自动打出的牌**（破灭那类）：**不扣能量，但计入**「本回合打出的第 N 张」 | 两条各写一遍 | `havoc_plays_the_top_of_draw_and_exhausts_it_without_paying`<br>`auto_played_attacks_count_toward_attacks_played` |
| 10 | **复活的敌人**：死亡剥离**所有**没重写 `ShouldPowerBeRemovedAfterOwnerDeath` 的 power —— 激怒、攒的 10 点力量、**以及我挂上去的易伤/虚弱全清**。只有第一条命打技能牌才涨力量 | 只清它自己的增益 | `test_subject_has_three_lives_and_death_strips_its_powers` |
| 11 | **踩踏**的减费实战确认（打过一张攻击牌后显示 2 费，也确实只扣 2 点能量） | — | `solver_finds_the_stomp_discount_order` |
| 12 | **药水给的格挡不吃脆弱** | 走 `card_block` | `potion_block_ignores_frail_but_card_block_does_not` |

内核用 `TOp::ClearOwnerStatusesExcept(&[Adaptable, PainfulStabs])` 表达第 10 条 ——
**写成"保留名单"而不是"清除名单"是有意的**，用清除名单迟早会漏。

### 2.1 这三条**只钉在 op 实现上，没有专门测试**

抽这份手册时查出来的。它们各自只有 `step.rs` 里一处实现和 `content.rs` 里
一行注释；**照突变验证的规矩，改坏了不会有测试变红**。

| 判定 | 实现 | 我原本会写错的 |
|---|---|---|
| **万向斩**："等量" = 对**主目标实际打出的数字**，其他敌人**不再过自己的乘区** | `Op::DamageOthersEqualToLast`（`step.rs:1565`） | 各自过各自的乘区 |
| **完美打击**：数**整个牌组**（抽/弃/手/消耗堆全算），**它自己也含"打击"、会数到自己** | `Scale::PerStrikeCard`（`step.rs:251`） | 只数手牌 |
| **熔融之拳**：翻倍先后**无所谓** —— 易伤是二值乘区，层数不改倍率（我问了个伪问题） | `Op::DoubleTargetVuln`（`step.rs:1573`） | 以为顺序有影响 |

---

## 2.2 沙坑：内核里**唯一**一条不看血量的死法

`[源码]` `SandpitPower` + `TheInsatiable`（第 2 幕 Boss 无厌沙虫）·
`[实测]` 2026-09-05 两份实录逐帧对上。

| 事实 | 来源 |
|---|---|
| 液化地面（开场那一手）给**它自己**挂 4 层沙坑，并塞 6 张狂乱逃离：**3 张进抽牌堆、3 张进弃牌堆**，位置都随机 | `[源码]` `LiquifyMove`：`i < 3 ? PileType.Draw : PileType.Discard` |
| 沙坑在**敌人整边回合开始**减 1（`AfterSideTurnStartLate(Enemy)`）。挂上它的那个敌人回合**自己不减** | `[源码]` + `[实测]` |
| 减到 0 ⇒ power 被移除 ⇒ `AfterRemoved` 走 `CreatureCmd.Kill(玩家, force: true)` | `[源码]` |
| **`force: true` 挡掉瓶中精灵** —— 源码注释明写 "blocking death prevention by effects like Fairy in a Bottle" | `[源码]` `CreatureCmd.Kill` |
| 狂乱逃离打出去给沙坑 **+1**，并把**这一张实例**的费用永久 +1（`EnergyCost.AddThisCombat`）。6 张各自第一次都只要 1 费 | `[源码]` `FranticEscape.OnPlay` |

**读法：玩家回合开始时看到沙坑 = 1，就意味着这一回合不打狂乱逃离就会死** ——
结束回合 ⇒ 敌人回合开始 ⇒ 减到 0 ⇒ 被吞。

`[实测]` 逐帧（`act2_f33_boss_crusher_2026-09-05.json`，A2）：
r2=4 · r3=3 · r4=2（打了一张 ->3）· r5=2（再打一张 ->3）· r6=2 · r7=1，
第 8 个敌人回合归零暴毙。同一场的另一份（玩家驾驶通关，
`passive_act2_f33_boss_insatiable.json`）在 r2/r5/r6/r8/r9 各打一张，
**正好是活到第 10 回合所需的最小张数 5 张**。

这条规则改的不只是一个数：**打这只 Boss 时"还剩几个回合"由倒计时决定，
不由血量决定。** 内核 2026-09-05 之前只建了层数没建后果，
于是求解器把狂乱逃离评成"0 伤害 0 格挡的废牌"，全程不打 —— 那一局 AI 驾驶
第 8 回合被吞。测试：`sandpit_counts_down_once_per_enemy_turn_and_kills_at_zero` ·
`sandpit_death_ignores_the_fairy` · `frantic_escape_buys_a_turn_and_gets_more_expensive` ·
`the_injected_enemy_turn_also_ticks_the_sandpit`。

### 出招表（`[源码]` `TheInsatiable.GenerateMoveStateMachine`，A2 数值）

```text
液化地面(挂沙坑4+6张状态牌) -> 鞭挞 8×2 -> 猛扑撕咬 28 -> 垂涎(自身力量+2) -> 鞭挞2 8×2 -> 鞭挞 -> …
```

液化地面**只出现一次**（没有任何一手指回它），之后是四手定环。
进阶阈值：血量 321（`ToughEnemies` 341）· 鞭挞 8（`DeadlyEnemies` 9）·
撕咬 28（31）· 垂涎 +2（+3）。

---

## 2.3 幻象：杀不掉的爪牙（寄生惧魔 / 利齿之眼）

`[源码]` `IllusionPower` · `[实测]` 2026-09-05 / 2026-09-06 两份实录。

两只怪**逐字同构**：`Parafright`（胧光怪召的，21 血，猛撞 16）和
`EyeWithTeeth`（雾菇召的，6 血，扰乱塞 3 张晕眩）——
`AfterAddedToRoom` 都是同一句 `PowerCmd.Apply<IllusionPower>(.., 1m, ..)`，
出招表都是**单手自循环**。

| 事实 | 来源 |
|---|---|
| `IllusionPower` 顺带挂上 `MinionPower` ⇒ **主人一死它跟着消失** | `[源码]` `AfterApplied` · `[实测]` 胧光怪那场最后一帧：幻象剩 1 血活着，本体被打死，战斗结束 |
| 死了**不移出战斗**，尸体留在场上 | `[源码]` `ShouldCreatureBeRemovedFromCombatAfterDeath => false` |
| 下一手变成「复苏」（`HealIntent`），**回满血**，然后接回原来那一手 | `[源码]` `AfterDeath` -> `SetMoveImmediate(REVIVE_MOVE)`；`FollowUpStateId` 记着原来那一手 |
| **复活次数无上限** —— 没有适生力那种形态计数 | `[源码]` |
| 复活期间**打不到、也收不了 status** | `[源码]` `ShouldAllowHitting(owner) => !IsReviving` |
| 死亡剥离：**buff 全留着（含它自己那个），只剥非临时的 debuff** | `[源码]` `ShouldPowerBeRemovedOnDeath` = `Type == Debuff && !(power is ITemporaryPower)`，注释原文 "Illusions keep their buffs after dying" |

> **这条剥离的方向和适生力正好相反，抄错了会静默**：适生力是"默认全剥、
> 只留名单里那两个"，幻象是"默认全留、只剥非临时 debuff"。
> 内核用 `ClearOwnerStatusesExcept(&[幻象, 爪牙, 力量, 临时力量])` 表达 ——
> 前两个是它自己的标记，力量是哀嚎给的（源码明说 buff 留着），
> 临时力量是黑暗镣铐那种记账量（源码特意放过临时 debuff，好让回合末还得回去）。

### 「是复活还是重新召唤」这个欠定是怎么结掉的

`St::Illusion` 在 `KNOWN_UNMODELLED` 里挂着「欠定」很久，理由写的是
「分不出是雾菇重新召唤还是它自己的 `ILLUSION_POWER`」。
**结掉它不需要新数据，只需要把已有的两半读一遍**：

* `[源码]` `IllusionPower.AfterDeath` 写得毫不含糊
* `[实测]` `act1_f15_ninth` 同一场：帧5 闪电霹雳+ 打死利齿之眼（6 血），
  帧6/7 观测里**整只消失**，帧8 它 6/6 带着两个 power 回来 ——
  而**雾菇那两个回合的意图是 `Attack:8, Buff:` 和 `Attack:15`，都不是 Summon**。
  没有第二次召唤，所以只能是它自己复活的。

**教训：一个"欠定"要写清楚欠的是哪一份证据。** 这条欠的是"去读一遍源码"，
不是"再打一场"，而表里没写这件事，于是它按"不可再生"那一档躺着。

### 胧光怪 `TheObscura`

血量 123（进阶 `ToughEnemies` 129）。出招图：
`起点 幻象 -> RAND{ 穿刺凝视 10 | 哀嚎 | 硬化打击 6伤+6挡 }`，三条**等权**、
都是 `CannotRepeat`，**幻象只出现一次**（没有一条边指回它）。
`[wiki]` 的 pattern 一字不差地说了同一件事。

**哀嚎给的是"全队 3 力量，含它自己"** —— `[源码]`
`Apply<StrengthPower>(.., GetTeammatesOf(base.Creature), 3m, ..)`，
而 `GetTeammatesOf(c) => GetCreaturesOnSide(c.Side)` 含自己
（同一个函数早被组装师那条 `AlliesAliveAtLeast` 钉过一次）。
写成"只给自己"是**乐观**的：少算幻象那 16 点撞击的加成。

`[实测]` 2026-09-05 第2幕第30层：123/123 · 第 1 回合 `Summon:` ·
第 2 回合 `Attack:6, Defend:`（副意图由 `EOp::Block` 生成）。
**哀嚎和穿刺凝视那一场没出过**，那两条今天还只有 `[源码]` 和 `[wiki]`。

---

## 2.4 附魔：一张牌至多一个，钩子面只有六个

`[源码]` `CardModel.Enchantment` 是单个引用，`EnchantmentModel.CanEnchant` 里
`card.Enchantment != null` 直接拒绝 ⇒ **至多一个**。
能改游戏的口子只有六个：`EnchantBlockAdditive` / `EnchantBlockMultiplicative` /
`EnchantDamageAdditive` / `EnchantDamageMultiplicative` / `EnchantPlayCount`，
外加 `OnEnchant`（改关键字和费用）和 `OnPlay`。

内核建全的五种（`content::ENCHANTS`，22 种全部进表）：

| 附魔 | 规则 | 档次 |
|---|---|---|
| 灵巧 `NIMBLE` | 格挡 **+`Amount`**，加在**卡面基础格挡上**（敏捷/脆弱/臂甲之前） | `[源码]`+`[实测]` |
| 锋利 `SHARP` | 伤害 **+`Amount`**，和力量同一档 | `[源码]` |
| 直觉 `INSTINCT` | 伤害 **×2**，作用在基础值上、力量之前 | `[源码]` |
| 王室认证 `ROYALLY_APPROVED` | `OnEnchant` 加**固有 + 保留** | `[源码]`+`[实测]` |
| 沉稳 `STEADY` | `OnEnchant` 加**保留** | `[源码]` |

`[实测]` 灵巧那条：带灵巧2 的耸肩无视给 **10** 而卡表 8（2026-09-01 起语料里 76 次）。
顺序那半也钉住了：防御+（基础 8）+ 灵巧2、带脆弱 ⇒ `⌊10×3/4⌋ = 7`
（先脆弱再加会得到 6+2 = 8，两种写法在基础 5 上给出同一个数、分不开）。

`[实测]` 王室认证那条：2026-09-06 `act3_f46_elite_soul_nexus` **两个回合边界**，
带它的均衡+ 留在手上、同一手的添柴+ 被弃掉。
**保留 ≠ 均衡的 `St::Entrench`**：那个保整手牌、只保一个回合，是牌的效果。

其余 17 种**每一条都在 `note` 里点名卡在哪一个还没有的机制**，
并且照样进 `Report::unknown_enchantments` 被点名 —— 「表里没有」和
「表里有但没建全」对内核是同一件事：**这张牌会被算错，而内核知道自己在算错。**

## 2.5 彼岸咆哮：在消耗堆里每回合自己再打一次

`[源码]` `HowlFromBeyond.AfterAutoPostPlayPhaseEntered` 判 `Pile.Type == Exhaust`
就 `CardCmd.AutoPlay`。打完它自带消耗、回到消耗堆 ⇒ **每个回合都发作，永远**。

时点由 `[源码]` `CombatManager.EndPlayerTurnPhaseOneInternal` 定死：
`AutoPostPlay` 阶段跑在 `Hook.BeforeTurnEnd`（内核的 `Hook::TurnEnd`）**之前**，
也在弃手牌之前。

**只建第一句（3 费 AOE 16）会把一张引擎牌看成一张烂牌** ——
这正是"内容缺失会以别的样子露头"的一例：它缺席时 `act3_f46` 帧35 报的是
「原始力量+ 没把它变成巨石+」，看起来像规则错了。

---

## 2.6 每回合抽几张：基础 5 + 两件遗物，逐帧对得上

`[源码]` `CombatManager` 里那一句是 `Hook.ModifyHandDraw(state, player, 5m, ...)`
—— **基础 5 张，而且 `ModifyHandDraw` 只作用在回合开始那一次抽牌上**，
卡牌驱动的抽牌（耸肩无视那种）不过它。

| 来源 | 规则 | 档次 |
|---|---|---|
| 基础 | 5 张 | `[源码]` |
| 佩尔之血 `PAELS_BLOOD` | **每回合 +1，无条件** | `[源码]` `ModifyHandDraw => count + 1` |
| 准备背包 | 第 1 回合 +2 | `[源码]` |
| 花粉核心 | 每 4 回合 +2 | `[源码]` |
| 摆动球 | **每 3 回合 +1**，相位跨战斗保留 | `[源码]`+`[实测]` |
| 保留（`F_RETAIN`） | 上一回合没弃掉的留在手上 | `[源码]`+`[实测]` |
| 上限 | 手牌 10 张封顶 | `[实测]` |

**内核把这几件都建成"回合开始多抽 N 张"而不是"把 5 改成 5+N"** ——
`Hook::TurnStart` 在 `open_hand` 之前跑，先抽 N 再抽 5 和一次抽 5+N
从牌堆顶取到的是同一批牌。

### 摆动球的相位：观测到的计数器**已经加过这一场的回合数了**

`[源码]` `Pendulum.AfterPlayerTurnStart`：`TurnsSeen = (TurnsSeen + 1) % 3`，
**先加再判零**，而且 `TurnsSeen` 带 `[SavedProperty]`（跨战斗）。
所以第 R 个回合面板上那个数是 `(战斗开始那一刻的相位 + R) mod 3`，
**不是**相位本身。`TCond::EveryNTurns` 算的是 `phase + turn`，
要的是战斗开始那一刻的值 ⇒ 灌进去要减掉 `obs.round`。

`[实测]` 2026-09-06 `act3_f46_elite_soul_nexus`：面板计数器逐回合
`0,1,2,0,1,2,0`，游戏在第 **1/4/7** 回合多抽一张；不减 `round` 的内核抽在第
**3/6** 回合。

> **这条错误藏了很久，因为它和另一个错互相抵消**：佩尔之血没建（每回合少 1 张）
> + 摆动球相位早一个回合（该抽的回合不抽、不该抽的回合抽）——
> 合起来看是「每回合都少一点」的噪声。**补上佩尔之血之后，某一回合内核反而
> 比游戏多发一张**，相位错才露出来。
>
> `[实测]` 三条一起建全之后，**全语料 64 处「回合开始手牌张数」软差异归零**。
> 那 64 处以前被当成"洗牌/保留的 ±1 噪声"—— **系统性错误可以长得像噪声。**

---

## 2.7 进阶：十档里只有两档进战斗层

`[源码]` `AscensionLevel` 是个枚举，**序号就是进阶数**
（`AscensionManager.HasLevel(l) => _level >= (int)l`，`maxAscensionAllowed = 10`）。

`[源码]` `AscensionHelper.GetValueIfAscension(level, ascensionValue, fallbackValue)`
—— **没到那一档返回第二个参数**，所以写在前面的那个数是**高进阶值**。
读这一族调用时把两个参数读反，整张表会系统性偏低。

| 档 | 名字 | 改什么 | 进不进 L1 |
|---|---|---|---|
| 1 | `SwarmingElites` | 地图精英数 ×1.6 | 否 |
| 2 | `WearyTraveler` | **远古事件**回满血 ×0.8（不是篝火）| 否 |
| 3 | `Poverty` | 战斗金币奖励 ×0.75 | 否 |
| 4 | `TightBelt` | 药水槽 −1 | 否（内核照抄观测的 `max_potion_slots`）|
| 5 | `AscendersBane` | 开局往牌组塞一张升华诅咒 | 否（走观测到的牌组）|
| 6 | `Inflation` | 商店移除费 75→100，每次涨价 25→50 | 否 |
| 7 | `Scarcity` | 卡牌稀有度与升级概率变差 | 否 |
| **8** | **`ToughEnemies`** | **敌人耐久**：血量（含多形态/孵化）· 格挡 · 覆甲 · 几个 status 量 | **是** |
| **9** | **`DeadlyEnemies`** | **敌人输出**：伤害 · 段数 · 力量成长 · 部分格挡 · debuff 量 | **是** |
| 10 | `DoubleBoss` | **只给最后一幕**加第二个 Boss（`RunManager`：`i == Acts.Count - 1`）| 否 |

**怪物血量在 `[源码]` 里是个区间**（`MinInitialHp` / `MaxInitialHp`），两档各一对；
内核 `EnemyDef::max_hp` 是区间里的一个点（A1/A2 实测钉的）。
`asc_hp_low_range_contains_the_kernel_point_value` 守着"那个点落在低进阶区间里"。

内核侧的形状和收口点见 `../CLAUDE.md` 的「L1：进阶」，逐条数值在 `data/ascension.json`。

## 2.8 开局那一刻：六条**只有合成路径看得见**的规则

2026-09-09 建 L3 阶段 1（合成战斗构造器 `src/synth.rs`）时，验收台
`bin/synth_audit` 一次报出来的。**它们全都是真规则，而且全都在对拍路径上
结构性地看不见** —— 原因只有一条，值得先写清楚：

> **`sync` 每帧从观测重灌 status，而第 1 回合的 `Hook::TurnStart`
> 只有 `begin_combat` 跑得到。** 对拍语料的第一帧就是"回合 1 已经开始"，
> 内核从来没有机会自己跑那一刻 —— 于是"开局那一刻发生什么"这一整类规则
> 在七条既有验收里**一次都没有被检验过**。
>
> 合成路径没有观测可抄，它必须自己跑出那一刻。所以这一批是
> **新检验面照出来的旧错误**，不是新引入的。

| # | 规则 | 档次 |
|---|---|---|
| 1 | **玩家身上的覆甲，第 1 回合不掉层** | `[源码]`+`[实测]` |
| 2 | **人工制品照样吃掉触发器发出去的 debuff**（红面具的虚弱） | `[源码]`+`[实测]` |
| 3 | **两件遗物给同一个 status 时相加**（锚 10 + 假锚 4 = 14） | `[实测]` |
| 4 | 第 2 幕 Boss 开局挂三个 status，**其中包围挂在玩家身上** | `[源码]`+`[实测]` |
| 5 | **带包围时开局朝右** | `[源码]`+`[实测]` |
| 6 | 永世沙漏的凋萎存在**挂在它自己身上**，目标才是玩家 | `[源码]`+`[实测]` |

### 2.8.1 覆甲：玩家侧第 1 回合不掉层

`[源码]` `PlatingPower.AfterSideTurnStart` 一个条件管两边：

```csharp
participants.Contains(Owner)
  && (Owner.Player == null || Owner.Player.PlayerCombatState.TurnNumber != 1)
  && (Owner.Side != CombatSide.Enemy || combatState.RoundNumber != 1)
```

敌人那一半 2026-08-30 就照抄进内核了（`TCond::TurnAtLeast(2)`），
**玩家这一半漏了整整十天**。

`[实测]` 2026-09-09：14 条带护喉甲（开局 4 层覆甲）的语料，第 0 帧游戏一律报
`PLATING_POWER: 4`，而内核算出来是 3。`act2_f28_decimillipede` 逐回合是
`4 / 9 / 8 / 7 / 6 / 5 / 4 / 3` —— 第 2 回合那个 9 是「4 + 岩石铠甲 6，减 1」，
**掉层是从第 2 回合开始的**。

### 2.8.2 人工制品不问 debuff 是谁发的

`[源码]` `ArtifactPower.TryModifyPowerAmountReceived` 挂在**接收方**身上，
只判三件事：目标是不是自己 · 这个 power 算不算 `PowerType.Debuff` · 它可见不可见。
**来源一个字都不问** —— 卡牌、药水、遗物、触发器发出去的一样被吃掉，
然后 `AfterModifyingPowerAmountReceived` 给自己掉一层。

内核原来只在 `step::apply_status`（卡牌/药水那条路）过人工制品，
`TOp::AllEnemiesStatus`（触发器那条路）直接 `set` 上去。

`[实测]` 2026-09-09 `act3_f48_boss_aeonglass_2026-09-06` 第 0 帧：
永世沙漏是 **`人工制品 2 · 虚弱 0`** —— 它开局自带人工制品 3，红面具那 1 层虚弱
被吃掉、人工制品掉到 2。内核当时给的是 `人工制品 3 · 虚弱 1`。

### 2.8.3 两件遗物给同一个 status 要相加

`[实测]` 2026-09-09 `act3_f46_soul_nexus` 第 0 帧：身上同时有**锚**（开局 10 点格挡）
和**假锚**（4 点），玩家格挡是 **14**。

内核有两条路各自处理遗物给的 status，**它们原来不一致**：
`step::grant_relic`（合成路径）是 `add`，而 `replay::sync` 的 `relic_carry`
是逐条 `push` 再逐条 `set` —— 后一条把前一条**盖掉**，只剩 10。
审计台把两条路摆在一起才把它比出来（`push_carry` 修的就是这个）。

### 2.8.4 第 2 幕 Boss：三个开局 status，包围挂在**玩家**身上

`[源码]` `Crusher.AfterAddedToRoom` -> `BackAttackLeftPower(1)` + `CrabRagePower(1)`
（都给自己）；`Rocket.AfterAddedToRoom` -> `BackAttackRightPower(1)` + `CrabRagePower(1)`
给自己，**外加 `SurroundedPower(1)` 给 `GetOpponentsOf(self)`（玩家）**。

`EnemyDef::start_status` 只装得下挂在自己身上的那些，给对面挂的那条走
`content::ENEMY_START_PLAYER_STATUS`（消费点是 `step::begin_combat`）。

> 内核这三格原来是空的，注释写着"这三个 status 内核都没有"。
> **那句话过期了** —— 三个后来都建了（包围进伤害管线、站位是它的输入、
> 蟹之怒有 `AllyDied` 规则），只有那张表没跟着改。

### 2.8.5 带包围时**开局朝右**

`[源码]` `SurroundedPower` 的朝向是个没有初始化式的
`private Direction _facing;`，而 `Direction.Right` 是枚举的 **0** ⇒ 默认朝右。
朝右时**从左边打来的**（`BackAttackLeft`）才吃 ×1.5。

`[实测]` 2026-09-09 `act2_f33_boss_crusher` 第 0 帧的意图标签：
碾碎爪（左）**18 = 12×1.5**、火箭（右）**3**（面板值，没乘）。

内核的 `St::FacingRight` 默认 0（朝左）**正好反了**。对拍路径逃过一劫是因为
朝向在那边是**反推**的（`replay::infer_facing` 拿两个假设各对齐一遍，
哪个"逐字对上"的敌人多算哪个）—— 它每帧重推、自愈，所以从来没暴露默认值。

### 2.8.6 凋萎存在挂在 Boss 自己身上

`[源码]` `Aeonglass.AfterAddedToRoom`：

```csharp
foreach (Creature item in GetOpponentsOf(base.Creature)) {
    witheringPresencePower.Target = item;          // 凋萎塞进【玩家】手牌
    await PowerCmd.Apply(ctx, witheringPresencePower, base.Creature, 6m,
                         base.Creature, null);      // 但 power 挂在【Boss】身上
}
```

`[实测]` 两条永世沙漏语料的第 0 帧都把 `WITHERING_PRESENCE_POWER: 6`
记在**敌人**那一栏，玩家身上没有。

内核的规则本来就是对的（`Hook::CardPlayed` 两边都发、`AddCardToHand` 加进我的手牌），
错的只有一句注释和 `EnemyDef::start_status` 里缺的那一格。

---

## 2.9 卡面的「关键字词表」不是这张牌的关键字

`traces/cards_catalog.json` 里每张牌带一个 `keywords` 数组。**那是描述文本里
出现过的名词的词条表**（给玩家 hover 用的），不是这张牌自己的关键字。

`[实测]` 2026-09-09 `act1_f7_sewer_clam` 帧1→2：**彼岸咆哮打出去进的是弃牌堆**，
消耗堆一张没动。而它的 `keywords` 里明明有「消耗」——那两个字来自描述里的
「如果这张牌在你的**消耗**牌堆中」。

`[源码]` `HowlFromBeyond` 根本没有 `CanonicalKeywords`；对比 `Dazed` 那种真带
关键字的，写的是 `CanonicalKeywords => { Ethereal, Unplayable }`。

> **判据只有一个：[源码] 的 `CanonicalKeywords`。** 卡表里另外 12 张描述提到
> "消耗"的牌恰好都写对了（`exhausts: false`），只有这一张翻了车 ——
> **十三分之一的命中率不是"基本没问题"，是"这条路子本来就不该走"**。

### 2.9.1 彼岸咆哮：从消耗堆自动打出**只有一次**

`[源码]` `HowlFromBeyond.AfterAutoPostPlayPhaseEntered`：牌在消耗堆里就
`CardCmd.AutoPlay(this)`。而 `AutoPlay` 走的是普通的 `OnPlayWrapper`，
结果堆由 `CardModel.GetResultPileTypeForCardPlay()` 定 ——
**只看这张牌自己带不带 `Exhaust`，不管它是从哪个堆打出来的**：

```csharp
if (IsDupe || Type == CardType.Power)          return PileType.None;
if (ExhaustOnNextPlay || Keywords.Contains(CardKeyword.Exhaust)) return PileType.Exhaust;
return PileType.Discard;
```

所以：它被别的牌消耗掉（恶魔之焰 / 烙印 / 痛殴）之后，**回合末从消耗堆打出一次，
然后进弃牌堆**。要再来一次得再消耗一次。

> **2026-09-06 建它的时候写的是「消耗之后每个回合自己再打一次，永远」** ——
> 那是把 `exhausts: true` 那个错一路推下去的结果：自动打出之后它又回到消耗堆，
> 于是看起来像个永动机。**一个错的关键字能凭空造出一张引擎牌。**

---

## 2.10 开局那一刻，第二批（2026-09-09 六批修完之后）

和 [2.8](#28-开局那一刻六条只有合成路径看得见的规则) 同一个检验面
（`bin/synth_audit`），同一条理由：**对拍路径每帧从观测重灌，第 1 回合的
`TurnStart` 只有 `begin_combat` 跑得到**。

| 规则 | 档次 |
|---|---|
| **损毁头盔**：本场第一次获得力量时层数 ×2（只认给自己的、只认正数），用掉当场清 | `[源码]`+`[实测]` |
| **古茶具**：上一个房间是休息处才给那 +2 能量（假货 +1），第 1 回合一次 | `[源码]` |
| **风箱 / 骨茶**：开局把**手牌**升级 —— 时点在**抽牌之后** | `[源码]` |
| **宝石面具**：开局从抽牌堆挑一张**能力牌**进手牌 —— 时点在**抽牌之前** | `[源码]` |
| **碎石者**：开局把抽牌堆里随机 2 张可升级的牌升级 | `[源码]` |
| **小血瓶 / 假血瓶**：开局回 2 / 1 血 | `[源码]` |
| **缩放仪**：**Boss 房**开局回 25 血 | `[源码]` |
| **斗篷扣**：我的回合结束时，**每张手牌**给 1 点格挡（弃手牌之前数） | `[源码]` |
| **号角靴钉**：**第 2 回合**开始给 14 点格挡（清完格挡之后） | `[源码]` |
| **呼唤**（灵魂异鱼塞的状态牌）：回合末留在手上失去 6 点生命，**不可格挡** | `[源码]`+`[实测]` |

### 2.10.1 「抽牌之前」和「抽牌之后」是两个不同的时点

风箱要升级的是**手牌**（`AfterPlayerTurnStart`），宝石面具要从**抽牌堆**里挑牌
（`BeforeHandDraw`）—— 内核原来只有一个 `Hook::TurnStart`（在抽牌之前），
把风箱挂上去它会去升级一手还没发下来的空牌。所以 2026-09-09 加了
`Hook::HandDrawn`（`open_hand` 末尾），**加的时候就有两个消费者**。

### 2.10.2 局外状态：内核不该猜，**调用方给**

古茶具欠的是「上一个房间是不是休息处」、骨茶欠的是「还剩几场」、
缩放仪欠的是「这一场是不是 Boss」。这三样**战斗观测里一个都没有**，
而**整幕链自己知道**（它在模拟哪个房间）。

所以它们不进 `RelicDef::private_status`（那一栏是无条件挂的），
走 `content::CONDITIONAL_START` + `synth::FightSpec` 的三个输入。
**对拍路径一条都不挂** —— 那边能量/手牌/血量本来就是观测量，挂了就是重复计数。

> **这是「L1 缺的信息在 L3 手上」的第一批。** 在这之前这三件的结论是
> 「欠一个局外信息，不在 L1 里」，于是 `modelled: false` 挂了很久 ——
> 而真正缺的不是机制，是**一个参数**。

---

## 2.11 触发器互相点火：归属和层数（2026-09-12）

两条都是**整幕链**（`bin/act_eval`，L3 阶段 3）照出来的。它是第一个
**连着打几百场**的台子，而这两条错各自要绕好几圈才看得见 ——
前十条验收在它们身上**全绿**。

### 2.11.1 势不可当只对**自己**获得的格挡发作

[源码] `JuggernautPower.AfterBlockGained`：

```csharp
if (!(amount <= 0m) && creature == base.Owner) { ... }
```

**第二个条件内核原来没有。** `fire(Hook::GainBlock)` 对两侧都发，于是
**敌人蜷身获得格挡会触发我的势不可当**：白送一次伤害，方向是**高估玩家**。

内核侧的收口是 `fire_ctx` 的 `ctx`：`gain_block` 把「谁获得了格挡」传下去
（玩家是 `usize::MAX`），`GainBlock` 因此进 `only_ctx` 那一档，
玩家侧则在 `ctx != usize::MAX` 时不发作。
**门开在分发上而不是规则里**，理由和 `EnemyDamaged` 那条一样：
「一个钩子的 `ctx` 到底指谁」是每个钩子自己的语义。
（守卫：`juggernaut_only_fires_on_its_owners_block`，正反两面都钉。）

### 2.11.2 触发器打出来的伤害，它的钩子算在上一层

`hit_enemy_with` 原来把 `EnemyAttacked` / `EnemyDamaged` / `EnemyDied` /
`AllyDied` 四个钩子**一律按 `depth = 0`** 点火。于是 `MAX_HOOK_DEPTH`
**在整条伤害路径上是个摆设**：任何「触发器打人 → 挨打的那只再触发 →
再打人」的环，每绕一圈层数都被重置成 0。

**表现不是一条红，是爆栈**（工作线程直接死掉）。
[实测] 2026-09-12：势不可当（获得格挡就打一下）配上会给自己加格挡的敌人，
在整幕链的一条链上绕进去了；`RUST_MIN_STACK` 开到 64 MB 照样爆 ——
那正是"无限"和"很深"的判据。

修法是把层数传下去：`hit_enemy_with(.., hook_depth)`，
**打牌那条路传 0**（和原来逐字相同），`run_ops` 那条（触发器造成的伤害）
传 `depth + 1`。（守卫：`a_trigger_caused_hit_carries_the_hook_depth`，
从 `MAX_HOOK_DEPTH` 那一层点火，蜷身**不该**再被唤醒；再从第 0 层点一次
证明钩子本身是通的。）

> **两条错是一对**：2.11.1 造出那个环，2.11.2 让它没有底。
> 只修前一条的话，下一个互相点火的组合会再把它打爆一次。

---

## 3. 验证器抓到过的真错误

**验证器的价值在它抓到的错误，不在那些 MATCH。** 这张表是「哪类 bug 会静默」的索引：

| 抓到的 | 类型 |
|---|---|
| `start_player_turn` 错清敌人格挡 | 回合边界记账 |
| 痛击基础值被我凭欠定数据改错 | **拿欠定数据当判决** |
| `STAMPEDE` 名实不符 | 内容表张冠李戴 |
| 主宰 `ops_upg` 缺失 | 升级分支漏建 |
| **能力牌被错误地丢进弃牌堆** | 牌区流转 |
| **薪火之源按卡面建模导致能量重复计数** | **照卡面建模** |
| **乘区逐步取整**（第一个真 MISMATCH） | **"和已知数据一致"不等于对** |
| 敌人在多段攻击中途被反伤打死后还会把剩下的段数打完 | 结算顺序 |
| `step` 和 `legal_actions` 对同一个局面给出不同答案 | **两条路径长歪** |
| 验证器**自己**：`res.diffs = diff_play(...)` 覆盖式赋值，把之前收集的软差异整个丢掉 | 成功路径上永远看不见 |

两条从这里长出来的做法，已经变成规矩：

* **共用一份实现，两条路径就不可能长歪。** `play_card` / `auto_play_card` 共用
  `resolve_played_card`；`end_turn_with_incoming` / `enemy_turn` 共用 `take_attack_hit`。
* **「和所有已知数据一致」不等于对，这条也适用于测试设计。** 杂耍的 `== 3` 改成
  `% 3` 突变没变红 —— 因为测试只打了 4 张攻击牌，1/2/3/4 里两种写法命中完全相同。
  要打到**第 6 张**才分得开。

---

## 4. 已知没被钉住的

**内容表的覆盖率 ≠ 行为的覆盖率。** 看 `missing.json` 为 0 的时候记得这句话
（它由 `verify --emit-missing` 现场生成，不落库）。

* **缩小的管线位置**（§1.3）—— 值有两个来源，位置一个都没有。
* **附魔里除了灵巧和王室认证，一种都没被实战碰过。** 22 种进了表、5 种建全了，
  而语料里只见过这两种。锋利/直觉/沉稳是 `[源码]` 灌的，**第一次打出时对拍才判它**。
  没建全的 17 种照样会被 `unknown_enchantments` 点名 —— 不会静默算错。
* **钢笔尖那个 ×2 是"乘区"还是"打完再翻倍"，现有样本分不开**（§1.0）。
  照 `[源码]` 写成乘区。要分开需要一个 `base × 0.75` 小数部分 ≥ 0.5 的样本。
* **固有（`F_INNATE`）一次都没被观测碰过**：`sync` 每帧照抄观测的手牌，
  所以它只在内核自己开的仗（rollout / planner / 合成语料）里起作用。
* **彼岸咆哮的消耗堆重放同样没被语料碰过**：它在实录里只作为添柴生成的牌
  出现过一瞬间，随即被原始力量+ 变成了巨石+。规则是 `[源码]` + 单测。
* **`[源码]` 灌进去的内容**：表里很多牌和敌人的数值来自权威卡表 / 反编译，
  **第一次实战打出时对拍才会判它**。「没实现」和「没验过」是两件事，
  **后者的数字大得多**。（比例要重数才能写，别沿用任何旧数字。）
* **语料碰到了、内核却没建的敌人机制**（在 `KNOWN_UNMODELLED` 里，**对拍不报红**）：
  `REATTACH_POWER` · `PAINFUL_STABS_POWER` · `PERSONAL_HIVE_POWER` ·
  `SWIPE_POWER` · `HATCH_POWER`。观测天天碰到，内核天天算错，
  而验收全绿 —— **那张表存在的意义就是把这件事变成可枚举的，而不是变成没有。**
  （`SANDPIT_POWER` 2026-09-05、`ILLUSION_POWER` 2026-09-06 从这张表里建掉了，
  见 §2.2 / §2.3。）
* ~~雾菇的复活~~ **2026-09-06 结掉了**，见 §2.3 —— 它是幻象自己的复活，
  而结掉它靠的是已有的源码和已有的那一场实录，没有新数据。
* **抽牌堆抽空之后那次洗牌**：游戏的 `Rng.Shuffle` 状态没暴露，不可知。
  （`draw_pile_order` 补丁之后，**当前这一堆的顺序是确定的**。）
* **没映射的 status**（`replay.rs::map_status` 缺映射 ⇒ 那个字段从不被比较）：
  当前语料里只剩 `SPEED_POTION_POWER`。
* **进阶 A8 以上一次都没被观测碰过**（§2.7）：全部实录是 A1/A2，
  所以 `src/asc.rs` 整张表是 `[源码]` 档。对拍照不到它 —— **`asc::adjust` 在
  `asc < 8` 时逐字返回原 op**，语料跑到的永远是那条提前 return。
  它同时也是一张**认**出来的表（按「种类 + 低进阶值」去对内核的 op），
  所以真正守着它的是四条单测而不是对拍，见 `../CLAUDE.md` 的「L1：进阶」。

> **「尚未被碰过」这类清单本身是最容易烂掉的一节** —— 它记的是"还没发生的事"，
> 而事情一直在发生。2026-09-01 重扫时，原文列的四条（虚弱/敏捷/难以杀灭/无实体
> 没被碰过、敌人 AI 不预测、抽牌洗牌不可对拍）**四条全部已经不成立**。
> **改内核之前先重扫一遍语料，别照抄这一段。**
