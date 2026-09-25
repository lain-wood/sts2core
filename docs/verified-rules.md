# 实测规则手册 —— 现在认定为真的那些

**这份文件只写"现在是什么"，一条流水账都不记。** 每条规则被哪一帧钉死、
当时读数多少、我先做错了什么，全在 [`verification-log.md`](verification-log.md)。
设计和接口在 [`../CLAUDE.md`](../CLAUDE.md) 和 `design-l1/l2/l3.md`。

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

### 1.8 打击木偶与红头骨

* **打击木偶**：`[源码] StrikeDummy.ModifyDamageAdditive` 只认 `CardTag.Strike`，
  有源攻击每段 +3；在卡面基础值乘区之后、虚弱/易伤乘区之前。
  内核 `content::STRIKE_CARDS` 是已支持卡牌的标签表，也供完美打击计数。
  `[实测] act3_f48_boss_test_subject_2026-09-15` 的帧9/11/18/30 钉住此前的伤害缺口。
* **红头骨**：`[源码] RedSkull` 在血量 ≤ 最大血量的一半时施加力量3，回血越过阈值时移除3；
  后续继续掉血不能重复施加。走当前生命变化入口，卡牌失血、敌人攻击、回血都适用。
  `[实测]` 同一场跌到49/98时力量3→6。同步只恢复私有生效标记，力量本身照抄观测。
* **芒果 / 宾邦**：`[源码] Mango.AfterObtained` 增加最大生命14；
  `BingBong.AfterCardChangedPiles` 复制新加入主牌组的牌。这些是局外效果，
  牌组/血量快照已包含结果，战斗层不重复施加。
* **痊愈药水**：`[源码+实测] CureAll` 先获得1能量，再抽2张；目标是玩家自身。

守卫见 `src/test_subject_tests.rs`：包含乘区顺序、非打击牌/药水不吃加成、半血整数边界、
敌人攻击触发、继续受伤不重复增加、回血撤销，以及完整实录单步验证。

### 1.9 实验体的意图与复活窗口同步

* `[实测]` 第二阶段连环爪击是10×3→10×4→10×5；`ClawGrowth` 为私有量，
  实况同步从已经亮出的段数恢复。`[源码]` 该计数在攻击后增加，不显示额外Buff意图。
* `[源码+实测]` 第二阶段的剧痛刺击和第三阶段的复仇宿敌区分两手同为10×3的招式。
  单回合威胁、rollout 和 D=2 的威胁入口均读取递增段数。
* 实录敌人预测从**上一手**的计数与操作推进；最大生命变化且上一形态带适生力，
  表明期间经历了复苏，应从复苏后继推进。篡改当前观测的段数会使测试失败。
* `[实测]` 敌人在复活窗口完全消失。同步保留最后已知形态，在隔离状态中执行同一份死亡规则，
  恢复下一个形态与私有标记；不重发玩家击杀奖励。只有规划路径恢复敌人定义，注入对拍仍用UNKNOWN。

这里的实录是A1/A3；A8以上复活血量的进阶映射仍是已知缺口，不能用这些测试证明A10正确。

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

**这一手发几张只有一处定义：`step::hand_draw_count`**（2026-09-25），`open_hand` 发的就是它，
planner 的机会节点也读它。照 `[源码]` `CombatManager.SetupPlayerTurn` 的顺序：

1. `Hook.ModifyHandDraw(5)`：佩尔之血 / 准备背包 / 花粉核心**加**，心灵腐化**减**、到 0 封底。
   内核里这几件挂在 `Hook::TurnStart` 上**只记账**（`TOp::OwnerHandDraw` 加进 `St::HandDrawBonus`），
   不先抽；
2. `ModifyHandDrawLate`：小提琴 +2。内核并进第 1 步那笔加张数，只在「5 + 加张数 − 心灵腐化 < 0」时
   和源码不同（心灵腐化今天只有 1 层）；
3. `CardPileCmd.Draw`：**手牌上限封顶**，手满了一张不抽、**也不洗牌**（`num == 0` 直接返回，
   在 `ShuffleIfNecessary` 之前）。

摆动球**不在这里面**：它是 `AfterPlayerTurnStart` 里的真抽牌（小提琴拦它），内核仍挂在 `TurnStart` 上。

> 2026-09-25 之前这几件是在 `TurnStart` 上**直接抽 N 张**，理由是"先抽 N 再抽 5 和一次抽 5+N
> 从牌堆顶取到的是同一批牌"。**对 L1 自己确实一样**，逐帧对拍看不出区别 —— 但那 N 张抽在 planner 的
> 机会节点**之前**，取的是内核自己那次洗牌的牌序：机会节点枚举不到、换种子也不变。
> 而机会节点那边写死按 5 张枚举，心灵腐化 / 手牌上限下也是错的。见 verification-log 同日条目。

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

内核侧的形状和收口点见 [`design-l1.md`](design-l1.md) 的「L1：进阶」，逐条数值在 `data/ascension.json`。

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

## 2.12 第 3 幕六只敌人带进来的规则（2026-09-13，**全部 `[源码]`，未实测**）

史莱姆狂战士 · 机甲骑士 · 电球头 · 咬人卷轴 · 失落之物 · 遗忘之物。
**名字（怪物和招式）取自游戏本地化表**（`SlayTheSpire2.pck` 里的 `*.name` /
`*.moves.*.title`），不是推的；图鉴里隐藏的招式没有译名，照抄源码 id。

| 规则 | 源码出处 | 守卫 |
|---|---|---|
| **流电**：我每打出一张**能力牌**（没有别的 affliction 的），挨 `Amount` 点 `Unpowered` 伤害，走格挡、不是攻击 | `GalvanicPower.AfterCardPlayed` | `galvanic_hurts_the_player_for_power_cards_only_and_goes_through_block` |
| **纸伤难愈**：它的攻击**每打穿一段**，我失去 `Amount` 点最大生命；被格挡吃光的那段不算；只认它自己打穿的 | `PaperCutsPower.AfterDamageGiven` | `paper_cuts_costs_max_hp_per_unblocked_hit_of_its_own_on_both_turn_paths` |
| **剧痛刺击**：同一个「打穿」判据，每段塞 `Amount` 张伤口进弃牌堆（源码是整条攻击数段数再乘，总数相同） | `PainfulStabsPower.AfterAttack` | `painful_stabs_adds_one_wound_per_unblocked_hit` |
| **抢夺力量/速度**：偷走的属性**只在小偷自己死时**还回来 | `PossessStrengthPower.AfterDeath`（`creature == Owner`） | `possess_returns_what_was_stolen_only_when_the_thief_itself_dies` |
| **负数的力量/敏捷是 debuff**，人工制品挡得住（小偷自己那 +2 照加） | `PowerModel.GetTypeForAmount`：`Counter && AllowNegative && amount < 0` ⇒ `Debuff` | `player_artifact_blocks_the_strength_steal_but_the_thief_still_gains` |
| **敌人招式的格挡吃它自己的敏捷** | `DexterityPower.ModifyBlockAdditive` 的门是 `IsPoweredCardOrMonsterMoveBlock` | `the_forgotten_block_and_dread_grow_with_the_dexterity_it_steals` |
| **往手牌塞的牌，手满了溢出进弃牌堆**（不是丢掉）—— 凋萎存在那条路原来是"满了不造"，一起改对 | `CardPileCmd.Add`：`isFullHandAdd` ⇒ `targetPile = Discard` | `mecha_knight_flamethrower_overflows_into_discard_when_the_hand_is_full` |

两条**有意的近似**，方向写清楚：

* **退还量读小偷自己的力量/敏捷**，不是源码那张私有字典（观测里没有、`sync` 带不过来）。
  两者只在「我的人工制品挡掉了偷窃」（多还，乐观）和「我永久削了它的力量」（少还，悲观）时分岔。
* **咬人卷轴的起手相位固定 `num = 0`**（源码是遭遇级随机数）。第 1 回合总是先大啃后咀嚼，
  纸伤难愈按打穿段数算 ⇒ **偏悲观**；`synth_audit` 的开局第一手在真实录像上约 2/3 会报集合外。
  （同形状的千足虫 2026-09-15 改用 `ECond::SlotRep`，允许集合不塌，见 §2.14。卷轴没跟着换：一条实录都没有，
  集合外那条代价在它身上还是假设。）

---

## 2.13 骑士团带进来的规则（2026-09-14，**全部 `[源码]`，未实测**）

连枷骑士 · 幽灵骑士 · 魔法骑士（`KnightsElite` 三只同场）。名字同样取自游戏本地化表。

| 规则 | 源码出处 | 守卫 |
|---|---|---|
| **回合末先消耗虚无的牌，再让"留在手上就发作"的牌发作**；带发作效果的牌**不算虚无**；两段都在弃手牌/保留之前 | `CombatManager.DoTurnEnd`：`if HasTurnEndInHandEffect … else if Ethereal`，先 `Exhaust` 列表后 `OnTurnEndInHandWrapper`；`FlushPlayerHand` 在第二阶段 | `ethereal_exhausts_happen_before_turn_end_in_hand_effects` |
| **恶咒**：身上有它时手里每一张（没有别的 affliction 的）牌都虚无 —— 带保留的也照样消耗，灼伤那类发作牌不受影响 | `HexPower.TryModifyKeywordsInCombat` + `Hexed` | `hex_exhausts_every_card_left_in_hand_except_turn_end_effect_cards` |
| **恶咒只在施咒者自己死时解除** | `HexPower.AfterDeath`：`creature == Applier` | `hex_lifts_only_when_the_spectral_knight_itself_dies` |
| **抑制**：挂上那一刻把本场所有已升级的牌降级；施咒者死光才升回去（期间被再升过的不重复升） | `DampenPower.AfterApplied` / `AfterDeath` / `AfterRemoved`（`CardCmd.Upgrade` 跳过不可升级的） | `dampen_downgrades_upgraded_cards_until_the_magi_knight_dies` |
| **恶咒、抑制都是 debuff**，人工制品挡掉就什么都不发生（抑制的降级挂在 `AfterApplied` 上） | 两者 `PowerType.Debuff` | `artifact_blocks_dampen_and_nothing_is_downgraded` |

一条**有意的缺口**，方向写清楚：

* **抑制降过哪几张，只有合成路径知道**（`CardInst` 的 `F_DAMPENED`）。对拍 / 实战路径从观测灌牌名，
  降过级的牌看起来就是没升级的牌 —— `solve --live` 那一回合里砍死魔法骑士，内核看不见升回来的牌
  （**低估自己**）；将来录到这一场时，`verify` 在魔法骑士死掉那一帧会报手牌身份不一致。
  补法是拿观测里的 `deck`（第 4 个补丁的主牌组）按牌名配对推回来，同名多张时欠定。

「谁是施咒者」观测里没有（恶咒/抑制挂在**我**身上，`Applier` 不报）。内核在骑士身上挂私有标记，
合成路径（`begin_combat`）和对拍路径（`sync`）都按**名字**从 `content::ENEMY_PRIVATE_MARKERS` 挂 ——
和 `sync` 从观测补 `SlowSource` 同一个道理：这是身份，不是预测。

---

## 2.14 残杀千足虫的接续（2026-09-14，`[源码]` **+ 实录两次复活**）

| 规则 | 源码出处 | 证据 | 守卫 |
|---|---|---|---|
| **三节起手两两不同，Front / Middle / Back 是同一方向的轮换**（`num / num+1 / num+2`，`num` 是遭遇级随机数 ⇒ 单看一节三手都可能）（2026-09-15） | `DecimillipedeElite.GenerateMonsters` + `DecimillipedeSegment` 的 `StarterMoveIdx % 3` -> 扭动 / 壮硕 / 缠绕 | 第 0 帧 `act2_f28` 壮硕/缠绕/扭动（`num = 1`）· `act2_f30` 扭动/壮硕/缠绕（`num = 0`）；反方向两条都对不上 | `decimillipede_segments_open_staggered_by_slot_but_each_first_move_stays_open` |
| 一节被砍死**不移出战斗、打不到**；别的节全死了战斗才结束 | `ReattachPower.ShouldCreatureBeRemovedFromCombatAfterDeath` / `ShouldAllowHitting` / `ShouldOwnerDeathTriggerFatal` | `act2_f28_decimillipede` | `killing_the_other_segments_inside_the_reattach_window_ends_the_fight` |
| **死后第二个敌人回合**回 `Amount` = **25** 血（不是回满）；超杀不扣回血量 | `SetMoveImmediate(DeadState)` -> `DEAD_MOVE` -> `REATTACH_MOVE` -> `Heal(Amount)` | 第 3 回合砍死 -> 第 5 回合 25/44；第 5 回合砍死 -> 第 7 回合 25/40 | `a_decimillipede_segment_reattaches_with_25_hp_on_the_second_enemy_turn` |
| 死亡剥离**连力量一起清**，只留接续本身 | `ShouldPowerBeRemovedAfterOwnerDeath` 默认 true | 死前缠绕 `Attack:10`，回来 `Attack:8` | 同上 |
| 重接之后等权随机三选一，再回到三手循环 | `RandomBranchState`（`CannotRepeat`，上一手是重接 ⇒ 三条都放行） | 复活后第一手分别是缠绕、壮硕，之后照循环 | — |
| 对拍路径：尸体整只不在观测里，倒计时从「最后一次被看见是第几回合」推 | — | 节 1 第 4 回合那几帧 `enemies` 里没有它 | `sync_keeps_a_vanished_segment_as_a_corpse_owed_a_reattach` |

**战术推论：窗口是两个我方回合** —— 砍死一节的那一回合 + 下一回合结束之前把别的节全砍掉，它就回不来。

有意的近似和缺口，方向写清楚：

* **回血发生在敌人回合开始**，不在它自己那一手（和适生力 / 幻象同一个简化，死人不出手）。
* **被荆棘 / 火焰屏障在敌人回合里反伤打死的那一节**，源码要再晚一回合回来；对拍路径分不开，早回来一回合（悲观）。
* **叶评估不数尸体欠的 25**（乐观）：计入会让求解器不愿收掉低血的节，要自己的 A/B。
* **合成路径上三节的起手固定 `num = 0`**（扭动 / 壮硕 / 缠绕；2026-09-15 之前是三节全从扭动起、整场同相）。
  写法是 `ECond::SlotRep`：`initial_move` 按槽位挑，`allowed_initial` 仍然三手全开 ——
  **没用咬人卷轴的 `SlotIs`**，那会让 `synth_audit` 在 `act2_f28`（`num = 1`）上三节全报集合外。
  代价是出手顺序和「哪一节先攒力量」跟槽位绑死；换 `num` 量过，见 verification-log 09-15。
* **最大血量没调成偶数且互不相同**（[源码] `AfterAddedToRoom`，要遭遇级参数）。

---

## 2.15 知识恶魔（2026-09-14，**`[源码]` + 玩家判定，未实测**）

名字取自游戏本地化表：知识恶魔 · 知识的诅咒 / 抽打 / 知识过载 / 思考；
四个诅咒 `DISINTEGRATION_POWER` 瓦解 · `MIND_ROT_POWER` 心灵腐化 · `SLOTH_POWER` 懒惰 · `WASTE_AWAY_POWER` 虚脱。

| 规则 | 源码出处 | 守卫 |
|---|---|---|
| 出招：诅咒 -> 抽打 -> 知识过载 -> 思考 -> **诅咒不到 3 次回诅咒，否则回抽打** | `GenerateMoveStateMachine`（`_curseOfKnowledgeCounter < 3`） | `knowledge_demon_curses_three_times_then_cycles_without_the_curse` |
| 思考：11 伤害 + **回 30 血**（× 玩家数）+ 力量 2 | `PonderMove` | `knowledge_demon_ponder_heals_30_and_gains_strength` |
| 思考的意图是 攻击 + 回血 + 强化 | `new MoveState(..., SingleAttackIntent, HealIntent, BuffIntent)` | `knowledge_demon_ponder_signature_is_attack_heal_buff` |
| 知识的诅咒**不打人**，二选一**不能跳过**；三组 瓦解 6 / 心灵腐化 1 · 瓦解 7 / 懒惰 3 · 瓦解 8 / 虚脱 1，都不吃进阶 | `CurseOfKnowledge` · `_curseOfKnowledgeSets` · `_disintegrationDamageValues` · `FromChooseACardScreen(canSkip = false)` | `curse_policy_bits_pick_the_side_of_each_curse` |
| 瓦解：**我的回合末最后一步**（弃完手牌之后、敌人出手之前）受 `Amount` 点 `Unpowered` 伤害，**走格挡**；`Counter`，会叠 | `DisintegrationPower.AfterSideTurnEndLate`；`Hook.AfterTurnEnd` 在 `FlushPlayerHand` 之后，先 `AfterSideTurnEnd` 后 `…Late` | `disintegration_eats_my_leftover_block_before_the_enemy_attacks` |
| 心灵腐化：回合开始那一手少抽 `Amount` 张（`max(0, …)`） | `MindRotPower.ModifyHandDraw` | `mind_rot_draws_one_fewer_card_at_turn_start` |
| 懒惰：本回合打出满 `Amount` 张之后不能再打（自动打出也拦） | `SlothPower.ShouldPlay` / `BeforeCardPlayed` | `sloth_stops_the_fourth_card_in_a_turn` |
| 虚脱：最大能量 −`Amount` | `WasteAwayPower.ModifyMaxEnergy`（和薪火之源同一个口子） | 同第一条 |
| 诅咒出过几次**从我身上的 status 反推**（前缀长度唯一） | —（内核自己的做法，源码是私有计数器） | `curses_taken_reads_the_prefix_back_from_my_statuses` |

**玩家判定**（2026-09-14）：「崩解那一下，回合末剩的格挡真的挡得住，并且是先结算」——
内核读成「**在敌人出手之前**、这回合的格挡还在的时候结算」，和源码时点一致。
「实战第一个都可以，第二一般选瓦解，第三个看情况」—— 进了 `content::DEFAULT_CURSE_POLICY = 0b010`
（只有第 2 位是判定，第 1、3 位是 `[判断]`）。

**选法是策略参数，不是 `Pending`**（`State::curse_policy`）：选牌屏开在敌人回合中间，而 `step(EndTurn)` 是原子的；
这个选择的价值全在后面几个回合，单回合求解器定不了价。`advise` 的单场问法把 8 种选法配对比一遍。

有意的近似和缺口：

* **人工制品挡掉一次诅咒**之后内核反推不出那一次，下一次会重复同一组（源码计数器照样加一）。
* **懒惰只拦 `legal_actions` 和 `step` 的出牌**；破灭 / 倾泻 / 惊逃那类自动打出没拦（源码拦）—— 组合罕见，方向是高估自己。
* **推演不看局面换选法**：8 种里每一种都是从开局定死的。
* 旧的 `瓦解` **卡牌**（`content::CARDS`，状态牌 + `HAND_END` 6 点）是之前照卡面猜的：[源码] 那张牌只在选牌屏上出现，
  `OnChosen` 挂 power，**从来不进手牌**。它不影响任何路径，没删。

---

## 2.16 蜂群术士（2026-09-14，`[源码]` **+ 实录一场**）

名字取自游戏本地化表：蜂群术士 · 蜜——蜂——！/ 矛击！/ 喷射信息素；`PERSONAL_HIVE_POWER` 人体蜂房。

| 规则 | 源码出处 | 证据 | 守卫 |
|---|---|---|---|
| 人体蜂房：**每一段攻击**命中塞 `Amount` 张晕眩进**抽牌堆随机位置**；**挡住也塞**；药水 / 遗物 / 荆棘 / 能力牌不塞 | `PersonalHivePower.AfterDamageReceived`（`IsPoweredAttack`，没有 `UnblockedDamage` 门） | `act2_f27_elite_entomancer`：五张攻击各 +1、两瓶药水 +0 | `personal_hive_dazes_every_attack_hit_but_not_potions` · `personal_hive_dazed_counts_match_the_entomancer_trace` |
| 出招：蜜——蜂——！-> 矛击！-> 喷射信息素 -> …，起点蜜——蜂——！ | `GenerateMoveStateMachine`（`initialState = moveState2`） | `--predict-enemy` 2/2 | — |
| 喷射信息素：蜂房 < 3 时蜂房 +1、力量 +1；否则只加力量 2（两个数不吃进阶） | `SpitMove` | **无实录**（那一场没活到喷射） | `entomancer_spit_branches_on_hive_stacks` |

内核的做法和它的前提：

* **喷射拆成两手**（下标 2 / 3），分支是矛击！之后的条件边（`ECond::SelfStatusBelow` / `SelfStatusAtLeast`）。
  等价的前提是「内核判的那一刻（矛击！出完）到游戏判的那一刻（喷射执行）之间，没有别的东西改蜂房」—— 今天成立。
* **两手的意图签名逐字相同**，实况对齐靠 `step::move_reachable_now` 排掉当前层数走不到的那一支
  （`move_reachable_now_rules_out_the_spit_branch_the_hive_forbids`）。

**`verify` 看不见晕眩**：一步对拍不比抽牌堆。上表那条实录对拍是单独的测试，不在九条验收里。

有意的近似和缺口：

* **打死它的那一下不塞**（规则挂在 `EnemyDamaged`，死了不发）。源码塞，但仗已经打完了。
* **牌位上限 `MAX_CARDS` = 128**：满了就不再塞（`spawn_card` 返回 `None`）。三层蜂房配多段牌的长仗碰得到，方向是乐观。
* **叶评估不给晕眩定价**（代价在后面几个回合）：求解器仍然会高估多段牌 —— 只是推演里现在真的会被晕眩卡手。

### 2.16.1 敌人的覆甲在**早一档**给格挡

[源码] `PlatingPower.BeforeSideTurnEndEarly` 给格挡，早于 `AfterSideTurnEnd` 那一档（熟睡减层 / 醒来移除覆甲，见 §2.17）。
内核拆出 `Hook::EnemyTurnEndEarly`，敌人覆甲给格挡那条挂上去 —— 和 `TurnEndVeryEarly` / `TurnEnd` 同一个理由：顺序写进钩子，不靠 `POWERS` 的表内先后。
对既有内容惰性：拆完之后 Glory / Underdocks（青蛙骑士 / 下水道蚌所在）的整幕链逐行不变。

---

## 2.17 异螨 / 熟睡甲虫（2026-09-14，**全部 `[源码]`，未实测**）

名字取自游戏本地化表：`MYTE` 异螨 · 浓毒 / 啃咬 / 吸吮；`SLUMBERING_BEETLE` 熟睡甲虫 · 打鼾 / 出击 / 醒来；`SLUMBER_POWER` 熟睡。

| 规则 | 源码出处 | 守卫 |
|---|---|---|
| 异螨定环 浓毒 -> 啃咬 13 -> 吸吮 4 + 力量 2；**起手按站位**：first 浓毒、second 吸吮 | `Myte.GenerateMoveStateMachine`（初始态是读 `SlotName` 的条件分支） | `mytes_open_by_slot_and_toxic_goes_into_the_hand` |
| 浓毒往**手牌**塞 2 张毒素（写死，不吃进阶；手满溢出进弃牌堆） | `ToxicMove`：`AddToCombatAndPreview<Toxic>(.., PileType.Hand, 2)` | 同上 |
| 熟睡甲虫开局覆甲 15 + 熟睡 3；醒了之后**永远出击** 16 + 力量 2 | `AfterAddedToRoom` · `RolloutMove`（`FollowUpState` 指向自己） | `slumbering_beetle_left_alone_sleeps_three_enemy_turns_then_rolls_out` |
| 熟睡**自己回合末** −1；归零当场醒：移除覆甲（那个回合末的格挡已经给过），下一个敌人回合出击 | `SlumberPower.AfterSideTurnEnd` -> `WakeUpMove` | 同上 |
| 熟睡**被打穿** −1（`UnblockedDamage != 0`，**不分是不是攻击**；被格挡吃掉的不算）；归零是**击晕换招**：下一个敌人回合「醒来」（移除覆甲、不打人），之后出击 | `SlumberPower.AfterDamageReceived` -> `CreatureCmd.Stun(WakeUpMove, "ROLL_OUT_MOVE")` | `slumbering_beetle_woken_by_damage_is_stunned_one_turn_then_rolls_out` |

**打法推论**（和用户给的一致）：这一场先清两只盛碗虫、别碰甲虫 —— 打穿它一下就少睡一个回合。

### 2.17.1 游戏在**我方回合开始**才掷下一手，内核在出完招那一刻就推进

[源码] `CombatManager.StartTurn`（玩家那一边）里 `enemy.PrepareForNextTurn` -> `MonsterModel.RollMove` ->
`MonsterMoveStateMachine.FindNextMoveState`；而内核的 `step::advance_move` 在 `enemy_turn` 里每只敌人出完招立刻推进。
**两者之间隔着敌人回合末的钩子**和我方回合开始的 `BeforeSideTurnStart`。

今天全表只有熟睡甲虫的条件读的是那几个钩子会改的量（熟睡在 `AfterSideTurnEnd` −1）：
[源码] 条件是 `HasPower<SlumberPower>`，内核写成 **`熟睡 ≥ 2`**（`M_SLUMBERING_BEETLE`）。
「掷的那一刻还有熟睡」⇔「推进那一刻熟睡 ≥ 2」；`--predict-enemy` 拿我方回合开头的观测判下一手，那一刻也还没减 ——
同一个阈值两条路都对。写成 ≥ 1，回合末醒来之后会**多睡一回合**。

**以后再加「条件读 status」的敌人，先查那个 status 会不会在敌人回合末 / 我方回合开始被改。**

### 2.17.2 完整路径上「本回合被强制改招」原来不清

`St::MoveForcedThisTurn` 的文档一直写着「敌人整边行动完之后清」，但只有注入路径清它。完整路径（`step(EndTurn)`）上，
我方回合里被强制换过招的敌人（耕地 / 尖叫 / 钻地 / 被打醒的熟睡甲虫）从此带着它 ⇒ 之后每个回合求解器叶子上的注入威胁
都把那只敌人这一手跳过（乐观）。2026-09-14 在 `enemy_turn` 末尾补上
（`a_forced_move_flag_does_not_outlive_the_enemy_turn_on_the_full_path`）。
千足虫接续那条规则里紧跟着清一次的写法（§2.14）**仍然需要**：它挂在敌人回合开始，注入路径在同一个敌人回合里就会读到。

有意的近似和缺口：

* **站位用槽位号近似**（`ECond::SlotIs`，和外骨骼虫同一个）：`MytesNormal` 两格按出场顺序就是 0 / 1。
* **熟睡甲虫的 A8 覆甲 18 没进 `asc`**：生成器认不出开局 status 里的数（青蛙骑士同一个欠账），高进阶低估一截墙。
* 「醒来」的意图字符串 `Stun` 没实测过（盛碗虫（石）的晕眩同一条）。
* 注入路径不执行「醒来」的 op（它只结算伤害），单回合叶子上覆甲还挂着 —— 叶子不看敌人的覆甲层数，没有影响。

---

## 2.18 盛碗虫两场的构成是**分布**（2026-09-14，`[源码]`）

roadmap 上这一条挂着「要用户拍板：这算不算挑一组自洽的解」，用户 09-14 的批 4 方案里点了它（理由：源码里的分布完全确定）。

| 遭遇 | [源码] | 进表的样子 |
|---|---|---|
| `BowlbugsNormal` | 石站 first；工蜂从 `_workerValidCounts`（卵 / 丝 / 蜜各上限 1）`Rng.NextItem` 抽两次，抽过的不再进候选 | 三支等权：石+卵+丝 · 石+卵+蜜 · 石+丝+蜜 |
| `BowlbugsWeak` | 石站 odd；另一只 `Rng.NextItem(Bugs)`，`Bugs = { 卵, 蜜 }` | 两支等权：石+卵 · 石+蜜 |

**这不是挑代表值**：全部分支和权重都列出来，整幕链每个样本按权重抽一支（`Table::resolve_sampled`，
`pick` 由 `(种子基, 样本, 房间)` 派生、和血量掷点不共用一个数）。严格的 `Table::resolve` 照旧拒绝这两场 ——
单场问法（`advise` 的 `question: fight`）要调用方直接给场上敌人的名字。

数据落在 `data/encounters_overrides.json` 的 `distributions`（手写），`dump_encounters.py` 并进 `encounters.json` 的 `variants`，
过两道自检：每只怪都是真实的怪物类；解析器看得见的候选全都出现在某一支里。

三处口径跟着改（守卫 `bowlbug_encounters_are_a_distribution_sampled_by_weight`）：

* **覆盖率**要**每一支**都开得出才算这一场开得出（`Table::can_open`，Python 那边同口径）
* **认遭遇**：观测到的构成落在某一支上 ⇒ 算「唯一命中」；**不退回宽口径** —— 不在任何一支里说明那张分布表错了，报「表里没有」更响
* **站位**不进分布：这几只盛碗虫都没有看站位的招式

### 2.18.1 第 1 幕六场跟进（2026-09-15，第 1 幕批 0，`[源码]`）

用户 09-15 点的（「照盛碗虫的先例进表」）。两类，**别混**：

| 遭遇 | [源码] | 进表的样子 |
|---|---|---|
| `CorpseSlugsNormal` / `CorpseSlugsWeak` | 3 / 2 只噬尸蛞蝓恒定；`EnsureCorpseSlugsStartWithDifferentMoves` 只错开 `StarterMoveIdx` | **`encounters` override**（多重集确定，严格的 `resolve` 就认） |
| `TwoTailedRatsNormal` | 3 只双尾鼠恒定（third / fourth / fifth）；随机的只有 `StarterMoveIndex` 错开 | 同上 |
| `FlyconidNormal` | 飞蝇菌子 + `NextItem(_mediumSlimes)` | 分布：两支等权（+ 树叶（中）/ + 树枝（中）） |
| `SlimesWeak` | 小史莱姆 `ToList` 抽一只、`Remove`、再抽 ⇒ **一树叶一树枝恒定**；中史莱姆二选一 | 分布：两支等权 |
| `SlitheringStranglerNormal` | 先三选一 `SecondaryEnemyType`；中史莱姆那支二选一；小史莱姆那支对静态数组**放回**抽两次 | 分布，12 份：贾克斯果 4 · 树叶（中）2 · 树枝（中）2 · 两树叶（小）1 · 一树叶一树枝（小）2 · 两树枝（小）1 |

**两处小史莱姆的抽法正好是一对反例**：`SlimesWeak` 抽完从列表里删掉（不放回，恒定一对），
`SlitheringStranglerNormal` 直接对静态数组 `NextItem` 两次（放回，抽得出两只同名）。照一个写另一个就错。
起手相位仍然是出招机器的事（蛞蝓 / 双尾鼠按集合建），不进遭遇表。

`RubyRaidersNormal`（5 选 3、各上限 1 ⇒ 10 支等权）2026-09-19 刺客 / 暴徒进表时一起填了，见 §2.25。

守卫 `act1_random_encounters_resolve_or_sample_by_source_weights`。

---

## 2.19 瀑布巨兽：打死它不算赢（2026-09-15，第 1 幕批 1，`[源码]` **+ 实录一场**）

招式名取自游戏本地化表：`WATERFALL_GIANT.moves.ABOUT_TO_BLOW` 即将爆发 · `EXPLODE` 爆炸。
证据是 `act1_f17_waterfall_giant`（A2，70 帧，三段全在里面）。

| 规则 | 源码出处 | 实录 | 守卫 |
|---|---|---|---|
| 虹吸回血 10（A8 15），封顶最大生命 | `SiphonMove`：`Heal(SiphonHeal × 玩家数)` | 第 4 -> 5 回合 174 -> 184 | `waterfall_giant_siphon_heals_and_its_pressure_gun_grows_by_five` |
| 高压枪从 20（A9 23）起，**每打一次 +5**，+5 在攻击之后 | `AfterAddedToRoom` 设初值 · `PressureGunMove` 末尾 `+= PressureGunIncrease` | 第 5 回合 20 · 第 10 回合 25 | 同上 |
| **被击杀 ⇒ 锁成 999999999 血**：别的 power 随死亡摘掉、蒸汽喷发留着，下一手强制「即将爆发」 | `SteamEruptionPower.AfterDeath` -> `TriggerAboutToBlowState`（`SetMaxAndCurrentHp` + `SetMoveImmediate(.., forceTransition: true)`）· `ShouldPowerBeRemovedAfterOwnerDeath => false` | 死后那一帧 999999999 / 999999999、蒸汽喷发 42 | `waterfall_giant_killed_is_about_to_blow_then_explodes_for_its_steam` |
| 「即将爆发」不打人：蒸汽喷发层数记成爆炸伤害，移除蒸汽喷发 | `AboutToBlowMove` | 第 11 回合意图 `Stun`；第 12 回合蒸汽喷发没了 | 同上 |
| 「爆炸」是**攻击**（它身上的虚弱、我的格挡照算），打完 `Kill` 自己 ⇒ 战斗结束 | `ExplodeMove`：`DamageCmd.Attack(SteamEruptionDamage).FromMonster(this)` + `CreatureCmd.Kill` | 第 12 回合 `DeathBlow:42`（= 15 + 9 × 3），之后战斗结束 | 同上 |
| **自己出招途中**被反伤打死：这一手剩下的 op 照样落地，但指针**不推进**（先「即将爆发」再爆炸） | `AboutToBlowState.MustPerformOnceBeforeTransitioning = true` | — | `waterfall_giant_dying_to_thorns_mid_attack_still_winds_up_before_exploding` |

内核侧的四个做法：

* **锁血阶段的判据就是最大生命 = 999999999**（`content::ABOUT_TO_BLOW_HP` / `about_to_blow`），不另开标记 ——
  它是观测量，从战斗中途同步进来不会错相，和实验体拿最大生命判形态同一个做法。
  锁血期间 `content::hp_left_this_form` / `remaining_hp_including_revives` 数 **0**：它会自己炸死，
  当成血量的话叶评估会以为砍死它亏了十亿血（求解器不肯收人头），`rollout` 的 `enemy_hp_left` 也走这个函数
  （不然 L3「两边都死的仗谁把敌人打得更残」会被这一个数淹掉）。
  `solver::enemy_wall`（只给斩杀延伸用）**故意照读原始血量**：锁血的它打不死，按 0 读会误触斩杀。
* **和适生力不一样，血当场就回来**：从死的那一刻起它就是活的（打得到、上得了 debuff），
  「还在场上」那三处口径一处没碰。
* **「出招途中被改了招就不推进」是通用规则**，两条敌人回合路径都有（`step::enemy_turn` / `injected_enemy_turn`
  比出招前后的指针）。[源码] 依据是内核建了的**每一个**强制改招目标都带 `MustPerformOnceBeforeTransitioning`：
  `CreatureCmd.Stun` 造的 `STUNNED` 状态写死带它（`Creature.StunInternal`；尖叫 / 耕地 / 钻地 / 熟睡 / 失衡 / 贪食都走这条），
  直接 `SetMoveImmediate` 的几个各自也带（幻象复活 · 千足虫 `DeadState` · 实验体 `RESPAWN_MOVE` · 瀑布巨兽 `ABOUT_TO_BLOW_MOVE`）；
  带它的状态没打过就不会被 `RollMove` 转走（`MoveState.CanTransitionAway`）。内核在出完招的那一刻就推进，所以要自己拦。
  **例外只有一个，内核还没建**：女王的 `SetMoveImmediate(EnragedState)` 不带这个标记 —— 建女王时别套这条。
  注入路径上 `MoveForcedThisTurn` 留着：下一个注入回合读到它、跳过那个过期的伤害再推进 —— 正好是这类招都不打人。
  **它顺带修了实验体**：在它自己出招时被火焰屏障 / 荆棘打死，以前指针从「复苏」推进过去，复活那个回合就直接出手；
  现在先复苏。地道虫出招时被反伤打破钻地格挡同理（没有实录也没有单测）。读数和归因见 verification-log 09-15 批 1。
* **两个私有计数器**（`St::PressureGunGrowth` / `St::EruptionDamage`，游戏不报、不进 `ALL_ST`）
  由 `replay::infer_private_attack_counter` 从**这一手**的意图标签反推（正着算一遍签名、逐个值试，和痛殴加值同一招），
  在 `identify_enemies` 对齐指针时做 —— 不做的话高压枪 25 逐字对不上，按类型退回会对到同样是 `Attack + Buff` 的撞击上。
  守卫 `waterfall_giant_private_counters_are_recovered_from_the_intent_label`。

有意的缺口：

* **注入路径不执行非攻击 op**（设计如此），所以 L2 叶子上「即将爆发」没有把层数搬进爆炸伤害；跨回合那几层走完整路径，不受影响。
* **`verify --predict-enemy` 在它死后那两帧照旧对不上**：那条诊断路径不重放我方出牌、看不见「死了」（和耕地闸门同一个局限）；
  它也不带着私有量往后走，所以第二次高压枪那一帧的 +5 在那条路径上推不出来。

---

## 2.20 乐加维林族母的沉睡（2026-09-17，第 1 幕批 2，**全部 `[源码]`，未实测**）

名字取自游戏本地化表（`LAGAVULIN_MATRIARCH.*` / `ASLEEP_POWER.title`）：
沉睡 · 斩击 · 开膛破肚 · 灵魂汲取 · 醒来。图鉴里没有 `SLASH2` 的译名，
内核照无厌沙虫「鞭挞2」的老办法叫**斩击2**。

血量 222（A8 233，`Min == Max`）。开局 **覆甲 12 + 沉睡 3**，两个数都不吃进阶。

| 规则 | 源码出处 | 守卫 |
|---|---|---|
| 睡着时什么都不做；沉睡还在就接着睡（`SLEEP_BRANCH`） | `GenerateMoveStateMachine`：`ConditionalBranchState(HasPower<AsleepPower>)` | `lagavulin_left_alone_sleeps_three_turns_and_loses_the_last_wall` |
| **被打穿一下整条沉睡就没了**：覆甲当场移除 + 击晕换招，之后接斩击 | `AsleepPower.AfterDamageReceived`（`UnblockedDamage != 0` ⇒ `Remove<PlatingPower>` + `Stun(WakeUpMove, "SLASH_MOVE")` + `Remove(this)`） | `lagavulin_woken_by_damage_loses_plating_at_once_and_is_stunned` |
| **最后一个睡眠回合拿不到覆甲那堵墙**：沉睡 ≤ 1 时在覆甲给格挡**之前**把覆甲摘掉 | `AsleepPower.BeforeSideTurnEndVeryEarly`，而覆甲给格挡是 `BeforeSideTurnEndEarly`（§2.16.1） | 同上第一条（第 3 个敌人回合末格挡 **0**） |
| 自己回合末 −1，归零自然醒；**醒来那一手不碰覆甲** | `AfterSideTurnEnd` -> `Decrement` -> `WakeUpMove`，而 `WakeUpMove` 只播动画 | 同上 |
| 醒后四手定环：斩击 19（A9 21）-> 开膛破肚 9×2（A9 10×2）-> 斩击2 12（A9 14）+ 格挡 12（A8 14）-> 灵魂汲取 | 五个 `MoveState` 的 `FollowUpState` | `lagavulin_awake_cycle_is_four_moves_and_soul_siphon_swings_two_strength` |
| 灵魂汲取：我 −2 力量 −2 敏捷、它 +2 力量（都不吃进阶），不打人 | `SoulSiphonMove` | 同上 |

**和熟睡甲虫的熟睡（§2.17）逐条不同，别照着改**：那个被打穿是 −1 层、覆甲是**醒来那一手**清的
（所以醒来那一回合的 13 点格挡照给）；这个被打穿是**整条清零**、覆甲由沉睡自己摘（所以那一回合 **0** 格挡）。
两条压在同一个钩子上就只剩 `POWERS` 的表内顺序 —— 反向突变量过：把它挪到 `EnemyTurnEndEarly`，
守卫当场红在「第 3 个敌人回合末的格挡 左 10 / 右 0」。

**`Hook::EnemyTurnEndVeryEarly` 是为这一条加的**（[源码] `Hook.BeforeSideTurnEnd` 里
VeryEarly -> Early -> Before 三档依次跑），沉睡是它唯一的消费者 —— 和玩家侧的
`TurnEndVeryEarly`（奥利哈钢）是对称的两档。

**条件边的阈值是 `沉睡 ≥ 2`**，和熟睡甲虫**同一条时点换算**（§2.17.1）：游戏在我方回合开始才掷下一手，
那时敌人回合末的 −1 已经发生；内核在出完沉睡那一刻就推进指针。写成 ≥ 1 它会多睡一回合（乐观）。

> **打法**：开局那 12 点覆甲每个敌人回合末补满（覆甲自己每回合 −1：12/11/10…），
> 打不穿就醒不了。**打穿是划算的** —— 省掉那一堵墙，还白赚一个「醒来」不打人的回合；
> 而让它睡满，第 3 个回合末它本来就拿不到墙，下一手直接 19 点砍过来。

## 2.21 滑溜：封的是掉血，不是伤害（2026-09-17，第 1 幕批 3，**全部 `[源码]`，未实测**）

墨影幻灵（`VANTOM`，密林 Boss，173 血 / A8 183，开局**滑溜 8** / A8 9）和
墨宝（`INKLET`，`InkletsNormal` 三只，11–17 血 / A8 12–18，开局**滑溜 1**）。
名字取自本地化表；`SLIPPERY_POWER.title` = 滑溜，卡面原话
「这个生物下一次要失去生命值时，只会失去 1 点生命。」

**这一条是这一批的全部重点：滑溜 ≠ 无实体。**

| | 滑溜 `SlipperyPower` | 无实体 `IntangiblePower` |
|---|---|---|
| `ModifyHpLostAfterOsty` | 有（掉血封顶 1） | 有 |
| `ModifyDamageCap` | **没有** | 有（伤害本身封顶 1） |
| 26 点打在 8 点格挡上 | 格挡**清零**，掉 1 血 | 伤害先压成 1，格挡只掉 1 |

而 `CreatureCmd.Damage` 的顺序是 `DamageBlockInternal`（扣格挡）-> `Hook.ModifyHpLost`（封顶）
-> `LoseHpInternal`，所以内核的消费点在 **`damage::absorb`**，不在 `apply_modifiers`。
建错地方会让它的格挡永远掉不下去 —— 而那是这一整场仗的节奏。
守卫 `slippery_caps_hp_loss_not_damage_so_block_still_takes_the_full_hit`；
反向突变（改成无实体那样在扣格挡之前封顶）当场红。

`DamageResult.UnblockedDamage` 是**封顶之后**的那个数（`LoseHpInternal(unblockedDamage)`），
`wasFullyBlocked` 判的也是它 —— 所以「打穿了没有」这条门不受封顶顺序影响。

| 规则 | 源码出处 | 守卫 |
|---|---|---|
| 每**打穿一次** −1 层（`UnblockedDamage >= 1`，不分是不是攻击），归零即移除 | `SlipperyPower.AfterDamageReceived` | `slippery_is_paid_in_hits_not_in_damage` |
| 墨影幻灵四手定环：墨迹 7（A9 8）-> 墨水长枪 6×2（A9 7×2）-> 肢解 26（A9 30）+ 3 张伤口进**弃牌堆** -> 准备（自身力量 +2） | `Vantom.GenerateMoveStateMachine` · `DismemberMove` 的 `AddToCombatAndPreview<Wound>(.., PileType.Discard, 3)` | `vantom_cycle_is_four_moves_and_dismember_adds_three_wounds_to_the_discard` |
| 墨宝：刺击 3（A9 4）-> 随机（锐利凝视 10 / A9 11 \| 旋风 2×3 / A9 3×3）-> 刺击 -> … | `Inklet.GenerateMoveStateMachine` | `the_middle_inklet_opens_with_whirlwind_and_the_others_jab` |
| **中间那只起手旋风**，另外两只起手刺击 —— 遭遇本身一个随机数都没掷 | `InkletsNormal.GenerateMonsters` 只给中间那只 `MiddleInklet = true` | 同上 |

> **起手用 `ECond::SlotIs` 不是 `SlotRep`。** 两者的差别是「遭遇掷没掷骰子」：
> 千足虫是 [源码] `Rng.NextInt(3)` 决定三节从哪一手错开（所以是**代表元提示**，§2.14），
> 而这里三只的起手是写死的 —— 开局允许集合因此是**单元素**，守卫里有一条钉着这件事。
>
> 源码里那个 `INIT_RAND` 分支**是死代码**（既没进 `list`、也不是 initialState），照抄它是错的。

**打法：段数是货币。** 8 层滑溜是一堵**要 8 次命中**的墙，一段 26 点和一段 2 点付的价钱一样 ——
多段牌（旋风斩 / 双重打击 / 墨水长枪那类）在这只 Boss 身上的实际效率比面板高得多。
三只墨宝各 1 层，等于每只都要多挨一次命中才开始掉血。

**已知偏差，方向是乐观**：`solver::enemy_wall` / `optimistic_damage` / `horizon`
这几个启发式按**血量**算，看不见「前 8 次打穿只掉 8 点血」。真的推演（`step`）是对的，
偏的只有叶评估和地平线 —— 求解器会高估自己啃穿这堵墙的速度。

---

## 2.22 硬化外壳：一个回合累计封顶，双方各算一份（2026-09-19，第 1 幕批 4，**全部 `[源码]`，未实测**）

鬼祟珊瑚群（`SKULKING_COLONY`，暗港精英，75 血 / A8 80，`Min == Max`）开局**硬化外壳 20**
（`AfterAddedToRoom` 写死，不吃进阶）。名字取自本地化表：猛冲 · 惯性 · 穿刺戳击，
`HARDENED_SHELL_POWER.title` = 硬化外壳。`ZOOM_MOVE_2` 本地化表里没有，照老办法叫**猛冲2**。

| 规则 | 源码出处 | 守卫 |
|---|---|---|
| 每次掉血封成 `min(漏过格挡的, Amount − 本回合已掉)`；**在扣完格挡之后**（格挡照常被打满） | `ModifyHpLostBeforeOstyLate` · `CreatureCmd.Damage` 里 `DamageBlockInternal` 在 `ModifyHpLost` 之前 | `hardened_shell_caps_hp_loss_per_turn_after_block_not_per_hit` |
| 「已掉」加的是 `result.UnblockedDamage`（**封过之后**的实际掉血） | `AfterDamageReceived`（`WasFullyBlocked` 那次不加） | 同上 |
| **任何一边**的回合开始清零，而且在**最早一档**（早于清格挡、能量回满、`AfterSideTurnStart`） | `BeforeSideTurnStart`（不看 `side`）· `CombatManager.StartTurn` 的顺序 | `hardened_shell_refills_at_each_side_turn_start_before_anything_hits_it` |
| mod 报的是 `DisplayAmount` = `max(0, Amount − 已掉)`，**余额** | `HardenedShellPower.DisplayAmount` · mod `BuildPowers` 的 `["amount"] = power.DisplayAmount` | `sync_reads_the_hardened_shell_balance_and_puts_the_cap_back_by_name` |
| 四手定环：猛冲 14（A9 16）-> 猛冲2 14 -> 惯性 9（A9 11）+ 力量 2（A9 4）-> 穿刺戳击 7×2（A9 8×2） | `GenerateMoveStateMachine`（四个 `FollowUpState` 一个环） | `skulking_colony_cycle_is_zoom_zoom_inertia_piercing_stabs` |

**和难以杀灭（`DamageCap`）不是一回事**：那个是每一下封顶（`ModifyDamageCap`，扣格挡之前），
这个是一个回合累计、扣完格挡之后。和滑溜（§2.21）同一格，排在它前面（`BeforeOstyLate` 早于 `AfterOsty`）。

内核的四个做法：

* **存余额，不存 `Amount`**（`St::HardenedShell`）：观测里那个数就是余额，`sync` 从中途接进来一格不用换算，
  `verify` 逐字段比的也正是「这一下该吃掉多少」。上限 20 观测里没有，是内核私有的 `St::HardenedShellCap`，
  两条路径都按名字从 `content::ENEMY_PRIVATE_MARKERS` 挂（那张表为它从「层数恒为 1」放宽成带层数的一栏）。
* **`damage::absorb` 的门是上限不是余额** —— 余额扣到 0 正是外壳最硬的时候。反向突变量过：门换成余额，三条守卫当场红。
* **回满挂在新钩子 `Hook::SideTurnStart` 上**（两边的回合开始各点一次火，最早一档）。挂在 `TurnStart` 上的话
  水银沙漏 / 滚石那几点算进哪个回合的额度就只剩 `POWERS` 的表内顺序；反向突变（挪到 `TurnStart`）当场红在
  「荆棘 3 + 水银沙漏 3 一点都没打进去：左 55 / 右 49」。
* **扣完格挡之后才封** —— 反向突变（改成扣格挡之前封）只有「余额 0 也照样把格挡打光」那一句红，
  前面两下的数两种建法一样。**那一句存在的全部理由就是这个**（和滑溜那条是同一个教训）。

> **打法**：75 血至少 4 个我方回合（20+20+20+15），一个回合的第 21 点起全是白打 —— **打满 20 就转去挡**。
> 敌人回合里荆棘 / 火焰屏障反弹给它的伤害吃的是**敌人回合那一份**额度，不占我下一回合的 20。

**已知偏差（乐观）**：和滑溜同一条 —— `solver::horizon` / `optimistic_damage` 按血量算，看不见每回合 20 的天花板，
地平线偏短（能力牌的钱偏低）。真的推演（`step`）是对的；单回合求解器也看得见（打满 20 之后再出攻击牌叶子上不掉血）。

---

## 2.23 暗港杂兵（2026-09-19，第 1 幕批 5，**全部 `[源码]`，未实测**）

四场：`HauntedShipNormal` · `ToadpolesWeak` · `FossilStalkerNormal` · `GremlinMercNormal`（七只怪，名字取自本地化表）。

| 规则 | 源码出处 | 守卫 |
|---|---|---|
| 幽灵船：起手纠缠（我虚弱 3 + 5 张晕眩进**弃牌堆**，都不吃进阶），之后扫击 13（A9 14）/ 践踏 4×3（A9 5×3）交替 | `HAUNT -> SWIPE -> STOMP -> SWIPE` · `AddToCombatAndPreview<Dazed>(.., Discard, 5)` | `haunted_ship_haunts_once_then_alternates_swipe_and_stomp` |
| 蟾蜍蝌蚪：前面那只起手带刺、后面那只起手旋转（遭遇写死 `IsFront`，**一个随机数都没掷** ⇒ `SlotIs`）；环 旋转 7 -> 带刺（荆棘 +2）-> 吐刺（**先**荆棘 −2 再 3×3） | `ToadpolesWeak.GenerateMonsters` · `Toadpole.GenerateMoveStateMachine` | `toadpoles_open_by_slot_and_retract_their_spikes_before_spitting` |
| 化石追踪者：开局吮吸 3；起手缠上 12（A9 14），之后每一手三选一等权（冲撞 9 + 我脆弱 1 / 缠上 / 甩动 3×2），**同一手最多连出两次** | `AddBranch(state, 2)` = `(state, maxRepeats)` ⇒ `CanRepeatXTimes(2)`；`StateLog` 只记招式（分支态 `ShouldAppearInLogs => false`） | `fossil_stalker_sucks_strength_per_unblocked_hit_after_the_attack` |
| **吮吸**：它的一次攻击里**打穿了几段**就 +`Amount` × 段数 力量；被格挡吃光的段不算 | `SuckPower.AfterAttack`（`UnblockedDamage > 0` 逐段数） | 同上 |
| 地精佣兵：三手定环 拿来 7×2 -> 双重猛击 6×2 + 我虚弱 2 -> 嘿嘿 8 + 自身力量 2；**伤害挂的是 `ToughEnemies`（A8）** 不是 `DeadlyEnemies` | `GimmeDamage` 等三个都是 `GetValueIfAscension(ToughEnemies, …)` | `killing_the_gremlin_merc_summons_two_gremlins_and_combat_goes_on` |
| **意外**：地精佣兵死时召唤卑鄙地精、再召唤胖地精；**打死它不算赢** | `SurprisePower.AfterDeath` · `ShouldStopCombatFromEnding => true` | 同上 |
| 卑鄙地精：醒来（不打人）-> 一直冲撞 9（A9 10）；胖地精：醒来 -> 一直逃跑 | `SPAWNED_MOVE` 两只都是初始态；`FollowUpState` 指向自己 | 同上 |
| 这一批几手的意图签名（纠缠 = `Debuff` + `StatusCard:5` · 吐刺的 −2 荆棘**不**冒 `Buff` · 惯性 / 嘿嘿 = 攻击 + `Buff`） | 各 `MoveState` 的意图列表 | `act1_underdocks_batch_intent_signatures_match_the_source_intents` |

**吮吸逐段给和 [源码] 的「打完一次给」等价**，前提是：敌人一手攻击的面板值（`base + 力量 + 活力`）
在 `EOp::Attack` 开头**只算一次**。反向突变（挪进段循环里、每段重算）当场红在
「3 + 3：左 71 / 右 74」—— **而且全库 464 条里只有这一条红**：「一手攻击的面板值只算一次」在此之前没有任何测试守着。

有意的近似，方向写清楚：

* **胖地精的逃跑近似成原地不动**（`EOp::Nothing`，和偷窃草蜢同一个处置）。[源码] 是 `CreatureCmd.Escape`（离场）；
  内核没有「离场」，用 `KillSelf` 又会点燃地精之角那类死亡触发。**悲观**：多花 15 点伤害收掉它才算打完，不费血
  （卑鄙地精还活着时例外 —— 多拖的每个回合多挨一下 9）。
* **召唤血量是 `[判断]`**：卑鄙地精 10–14、胖地精 13–17（A8 各 +1），`SummonN` 只收一个数，取中位 12 / 15 ——
  和寄生物的扭动虫取 19（17–21）同一个做法；A8 以上各低估 1。
* **偷窃 / 盗窃只动金币**（`PlayerCmd.LoseGold` / 死时 `GoldReward`），战斗层没有金币 ⇒ 进 `MARKER_STATUSES`，只为和观测对齐。
  盗窃的层数是偷到的金币数、内核给不出，不进 `ALL_ST`，召唤出来的胖地精身上也不挂。
* **蝌蚪的荆棘不夹 0**：[源码] `ThornsPower` 不许负数、归零移除；内核的 `EOp::SelfStatus` 照加。
  环里吐刺永远接在带刺后面（2 -> 0），碰不到。

**顺带对出来的一处错名**：`replay::map_status` 原来把中文「偷窃」映射到 `SWIPE_POWER`（偷窃草蜢偷牌）——
本地化表里 `SWIPE_POWER.title` 是「**顺走**」，「偷窃」是 `THIEVERY_POWER`。观测走的是英文 id，
这个错名从没被踩到过；现在 `swipe | 顺走` / `thievery | 偷窃` 各归各。

---

## 2.24 死时召唤的宿主：「还要啃多少血」要把召出来的算上（2026-09-19，**L2 叶评估**）

**这一条不是新机制，是一个早就在的 L2 错**：`content::remaining_hp_including_revives`（`eval` 里敌人血那一项、
`horizon`、并列判据都读它）原来只认实验体的复活，**不认死时召唤**。砍死宿主那一下，这一项从
「宿主剩的几点」**跳涨**到「召出来的那几只的满血」—— 于是求解器不肯收宿主，宁可站着挨打。
和 2026-08-30 实验体那一次（「砍掉最后 4 点让这一项从 4 跳到 200」）是同一个陷阱的另一种形状。

现在由 `content::death_summon_hp` 补上：宿主身上每一条 `Hook::EnemyDied` 召唤规则，
`SummonN` 数 `hp × count`（寄生物 4 × 19 · 意外 12 + 15），`SummonCarryingSelfMinusOne` 数 `hp × 库存层数`
（巨斧机器人：库存 k ⇒ 后面还有 k 具）。**数字从 `POWERS` 读，不另抄一份**；**死了的不算**（召唤已经发生过了）。

| 证据 | 补之前 | 补之后 |
|---|---|---|
| 起手牌组打地精佣兵（`advise` 单场问法，A10，256 次） | 10 个回合 · 战损 p50 **76** | 4 个回合 · 战损 p50 **30** |
| `fight_eval` 重打实录 `act1_f12_elite_phrog`（**玩家实际 −1 血**） | 死 **57/64**（89%）· 11 个回合 | 死 **0/64** · 5 个回合 · 战损 p50 18 |
| `fight_eval` 重打 `act1_f15_new_elite`（也是异蛙寄生虫，玩家 13 血） | 死 37/64（58%） | 死 **0/64** |
| `rollout`（求解器策略）从 `act1_f12_elite_phrog` 第 1 回合推 | 死 59/64 | 死 **0/64**，回合中位 5（实战剩 6） |

| `solve` 验收台 `act3_f45_axebot` 回合 5（巨斧机器人，**库存**那一种召唤） | 求解线「只喝一瓶虚弱药水」，一张牌不打 | 剑柄打击+ -> 欺凌 -> 凌虐+ -> 虚弱药水 -> 头槌+ |

守卫 `killing_a_death_summoning_host_never_raises_the_hp_left_to_chew`（三只宿主各砍一次，走真的 `step`）；
反向突变（去掉召唤那一项）当场红。巨斧机器人那一项在整幕 / 单场的**死亡数**上量不出来
（单独去掉它，整幕 3810 -> 3809、单场一条不动 —— 推演里本来就打得过），**但单回合线上看得见**（上表最后一行）。

---

## 2.25 密林杂兵：藤蔓蹒跚者的缠结 · 劫掠者刺客 / 暴徒（2026-09-19，第 1 幕批 6，**全部 `[源码]`，未实测**）

两场：`VineShamblerNormal`（藤蔓蹒跚者单怪）· `RubyRaidersNormal`（五种劫掠者抽三）。名字取自本地化表的**简中**那一份
（`BRUTE_RUBY_RAIDER.moves.ROAR.title` 在包里有「咆哮」「怒吼」两条，前者是日文那份；简中是**怒吼**）。
`TANGLED_POWER.title` = 缠结（「缠绕」是 `CONSTRICT`，别混）。

| 规则 | 源码出处 | 守卫 |
|---|---|---|
| 藤蔓蹒跚者 61 血（A8 64），三手定环，**起点是挥击**：挥击 6×2（A9 7×2）-> 紧绕藤蔓 8（A9 9）+ 缠结 1 -> 大啃 16（A9 18） | `GenerateMoveStateMachine` 的 `initialState = moveState2` | `vine_shambler_tangles_my_attacks_for_exactly_one_player_turn` |
| 缠结：**我方攻击牌 +`Amount` 费**；X 费不吃；技能不吃 | `TangledPower.TryModifyEnergyCostInCombat`（只认带「缠身」affliction 的牌，挂上那一刻全部攻击牌都打标、之后进场的也打标）· `CardEnergyCost.GetWithModifiers` 对 `CostsX` 提前 return | `tangled_adds_to_attack_costs_after_local_modifiers_and_before_the_late_free_attack` |
| **加在哪一层**：本地改费（免费置 0 / 狂乱逃离 / 踩踏减费，都不夹 0）-> **缠结** -> 无情猛攻（`Late`，置 0）-> 夹 0 一次 | `GetWithModifiers`：`_localModifiers` -> `Hook.ModifyEnergyCostInCombat`（先普通、后 `Late`）-> `Math.Max(0, …)` · `LocalCostModifier.Modify` 不夹 0 | 同上 |
| 敌人回合里挂上，撑过我的下一个回合，**我的回合结束**摘掉 | `AfterSideTurnEnd`：`participants.Contains(Owner)` —— 敌人回合结束时 participants 是敌人 | `vine_shambler_tangles…` |
| 是 debuff，人工制品挡得住 | `PowerType.Debuff` | 同上 |
| 紧绕藤蔓的意图是 `Attack` + **`CardDebuff`**（不是 `Debuff`） | `SingleAttackIntent` + `CardDebuffIntent` | `act1_overgrowth_batch_intent_signatures_match_the_source_intents` |
| **mod 报的手牌费用已经含缠结**（`GetAmountToSpend`），`sync` 先扣再判「这张实例被本地改过费」 | mod `GetCostDisplay` -> `card.EnergyCost.GetAmountToSpend()` | `sync_does_not_double_count_tangled_in_the_observed_hand_cost` |
| 劫掠者刺客 18–23 血（A8 19–24），一直致命射击 10（A9 11） | `KillshotMove`，`FollowUpState` 指向自己 | `ruby_raider_brute_roars_every_other_turn_and_the_assassin_always_shoots` |
| 劫掠者暴徒 30–33 血（A8 31–34），殴打 7（A9 8）/ 怒吼（自身力量 +3，常量）交替，起手殴打 | `BEAT -> ROAR -> BEAT`，`_roarStrength = 3` | 同上 |
| `RubyRaidersNormal`：五种各上限 1、不放回抽 3 ⇒ **10 支等权** | `_raiderValidCounts` + `for (i < 3) Rng.NextItem(还没抽满的)` | `ruby_raiders_are_ten_equal_branches_of_three_distinct_raiders` |

**三处和「照卡面写」会写错的地方，各自有一个分得开的样本钉着**（反向突变都当场红）：

* **夹 0 之前加**：踩踏打过 4 张攻击，`3 − 4 + 1 = 0`；「夹完再加」给 1
* **免费的攻击牌在缠结下要 1 费**：[源码] `SetToFreeThisTurn` 是一条**本地**的「置 0」，全局钩子照样加在上面。
  内核原来对 `F_FREE_THIS_TURN` 直接 return 0 —— 缠结为 0 时两种写法逐字相同（`max(0, 0 − 踩踏减的)` = 0），所以改它不动任何旧读数
* **`sync` 不扣就双算**：缠结下打击显示 2，照旧规则记成 `cost_delta = +1`，`effective_cost` 再加缠结 ⇒ 3。
  同一个函数（`step::tangled_cost_addend`）给 `effective_cost` 和 `sync` 两个客户用；`verify` 那条「费用(内容表)」软差异也过它

内核的两个做法：

* **降成玩家 status，不建 affliction**：「全部攻击牌都带标」在内核里等价于「是攻击牌」，和轰鸣（`St::Ringing`）同一个处置。
  唯一分不开的是**已经带着别的 affliction 的攻击牌**（`AfterCardEnteredCombat` 只给 `Affliction == null` 的打标）——
  内核今天没有任何 affliction，碰不到；建女王的锁链（Bound）时要回来看这一条
* **摘掉挂 `Hook::TurnEndLate`**：[源码] 那一句在 `FlushPlayerHand` 之后，`TurnEndLate` 是内核离它最近的一档。
  两档之间没有任何东西读攻击牌的费用，这个选择改不了一个数

**已知偏差**：L2 叶子的注入路径照设计不跑非攻击 op（§2.19 那条「有意的缺口」），所以单回合求解器看不见
「这一手紧绕藤蔓会让我下一回合攻击贵 1」—— 那是下一回合的事，单回合本来就看不见。**缠结已经挂上的那一回合**它是看得见的
（观测里有 `TANGLED_POWER`，`sync` 灌进来，`effective_cost` 照算）。

> **打法**：缠结值的不是血是**回合** —— `advise` 单场问法（A10，256 次）把它摘掉对比：
> 起手牌组 终点血量 p50 不动、p10 39 -> 30、回合 p50 3 -> 4；13 张带四张额外攻击牌的牌组战损 p50 23 -> **31**、回合 2 -> 3。
> 紧绕藤蔓之后那一回合攻击贵 1，拖出来的那一回合正好吃大啃 16。**紧绕藤蔓那一回合前先把血量压低**，或者那一回合转去挡。

## 2.26 预先补的 12 件遗物（2026-09-25，**全部 `[源码]`，未实测**）

**没有一件在实录里出现过**，所以 79 条语料一个字节都没动 —— 下面每一条都只被单元测试守着，
第一次带着它们打仗时要重点看对拍。

| 遗物 | 规则 | 源码出处 | 守卫 |
|---|---|---|---|
| 天鹅绒颈圈 | 本回合打满 6 张就**不能再出牌**，**自动打出的也拦**（进结果堆、不结算）；+1 能量是 `ModifyMaxEnergy`，观测的 `max_energy` 已含 | `VelvetChoker.ShouldPlay(card, _)` · `CardCmd.AutoPlay`：`ShouldPlay` 为 false 和 `Unplayable` 同一句 `MoveToResultPileWithoutPlaying` | `velvet_choker_stops_the_seventh_card_and_resets_next_turn` · `play_cap_also_blocks_autoplay_from_exhaust` |
| 懒惰（顺带） | 同上：**自动打出的也拦**（它的 `ShouldPlay` 也不看 `AutoPlayType`）。内核原来只拦手动出牌 | `SlothPower.ShouldPlay(card, _)` | 同上（两件共用 `step::play_cap_reached`） |
| 小提琴 | 开局发牌 +2；**我的回合里**除开局发牌外一张都抽不到；**敌人回合里的抽牌不拦** | `Fiddle.ModifyHandDrawLate` · `ShouldDraw(player, fromHandDraw)`：`fromHandDraw` 放行、`Side != CurrentSide` 放行 · 全游戏只有 `CombatManager` 发开局手牌那一处传 `fromHandDraw: true` | `fiddle_draws_seven_then_locks_draws_for_the_rest_of_my_turn` · `fiddle_does_not_lock_draws_during_the_enemy_turn` |
| 「发牌」和「抽牌」 | 佩尔之血 / 准备背包 / 花粉核心改的是**发牌张数**（小提琴不拦）；摆动球是 `AfterPlayerTurnStart` 里的**真抽牌**（小提琴拦） | 前三件 `ModifyHandDraw` · `Pendulum.AfterPlayerTurnStart` → `CardPileCmd.Draw` | 同上（带佩尔之血开局 8 张、带摆动球那一张被拦） |
| 钻石头冠 | 我的回合末本回合出牌 ≤ 2 ⇒ 挂上一个 power：**受到的有源攻击伤害 ×0.5**（乘区，和别的乘区一起只取整一次），**敌人回合末**摘掉 | `DiamondDiadem.BeforeSideTurnEnd`（`<= CardThreshold`，2）· `DiamondDiademPower.ModifyDamageMultiplicative` 带 `IsPoweredAttack` · `AfterSideTurnEnd(Enemy)` 移除 | `diamond_diadem_halves_the_next_enemy_turn_only_after_a_short_turn` |
| 钻石头冠 × 冻住的意图标签 | 标签是**我出牌时**算的，那时 power 还没挂 ⇒ 注入路径要在标签上补 ×0.5。**在取整过的标签上再减半是精确的**：`⌊⌊x⌋/2⌋ = ⌊x/2⌋`。无实体 / 难以杀灭在身上时不补（它们在乘区之后，标签已经被改过） | 同上 | 同上（冻住 / 现算两条路各测一遍）· `frozen_label_halving_respects_intangible` |
| 波纹水盆 | 我的回合末本回合**没打过攻击牌** ⇒ 4 格挡（`Unpowered`），在敌人出手之前 | `RippleBasin.BeforeSideTurnEnd` 翻 `CardPlaysFinished` | `ripple_basin_blocks_only_on_a_turn_without_attacks` |
| 风的女儿 | 每打出一张攻击 1 格挡，`Unpowered` ⇒ **不吃敏捷** | `AfterCardPlayed` · `BlockVar(1, Unpowered)`、`GainBlock(.., null)` | `daughter_of_the_wind_gives_one_block_per_attack_ignoring_dexterity` |
| 手里剑 | 同回合第 3/6/9… 张攻击 +1 力量（和苦无同一个条件） | `AttacksPlayedThisTurn % 3 == 0` | `shuriken_fires_every_third_attack` |
| 锁镰 | 同上的条件，随机一个敌人 6 点，`Unpowered` ⇒ **不吃力量** | `Rng.CombatTargets.NextItem(HittableEnemies)` · `DamageVar(6, Unpowered)` | `kusarigama_hits_a_random_enemy_on_every_third_attack_without_strength` |
| 双截棍 / 铁棒 | 第 10 张攻击 +1 能量 / 第 4 张牌（任意）抽 1；**计数器跨战斗**，从面板灌 | `AttacksPlayed` / `CardsPlayed` 带 `[SavedProperty]` · `% n == 0` | `nunchaku_counts_across_combats_and_tolerates_the_activating_display` · `iron_club_draws_on_every_fourth_card_of_any_kind` |
| 面板计数器触发后那一秒显示 n | 同步恰好读到 n 时要当 0：**判倍数，不判 ≥ n** | `DisplayAmount` 在 `IsActivating` 时返回 `Cards.IntValue`（钢笔尖同一个写法） | 同上（灌 10 再打一张：不给） |
| 棋子 | 打出能力牌抽 1（真抽牌 ⇒ 小提琴拦） | `AfterCardPlayed`，`CardType.Power` | `game_piece_draws_on_power_and_is_locked_by_fiddle` |
| 打击木偶？？？ / 奥利哈钢？？？ | 和正品**逐字同一个类**，只有数值不同（1 / 3）；和正品同时带着时相加（奥利哈钢两件各判各的，结果等于 6+3） | 两对 `.cs` 逐行 diff 只差类名、稀有度、`MerchantCost` 和那一个数 | `fake_strike_dummy_and_fake_orichalcum_stack_with_the_real_ones` |
| 颈圈 / 头冠的面板计数器 | **就是本回合已出牌数** ⇒ 单帧同步时拿它覆盖不可知的 `cards_played` | 两件都自己数 `_cardsPlayedThisTurn` 并 `DisplayAmount` 返回它 | `cards_played_is_read_from_the_choker_or_diadem_counter` |

**已知没建的**：

* 颈圈等几件 `ModifyMaxEnergy` 的 +1 能量在 **L3 战斗外**拿不到（advise 在战斗外读不到能量上限，按 3 算并报一句）——
  这是所有加能量上限遗物共有的洞，不是这一件的
* 单帧同步（没有录制中的 trace）时 `attacks_played` 不可知、按 0 算 ⇒ 波纹水盆偏乐观、手里剑 / 锁镰的第 3 张可能数错。
  和苦无、踩踏同一个老洞；颈圈 / 头冠那两个计数器只补得上 `cards_played`
* ~~战斗专注在敌人回合里多拦抽牌~~ **同一天对齐了**，见下一条

### 2.26.1 战斗专注的「不能再抽牌」到**我的回合结束**为止（2026-09-25，`[源码]`，未实测）

| 规则 | 源码出处 | 守卫 |
|---|---|---|
| 同一回合里的抽牌照旧被拦；**敌人回合里的抽牌不拦**（挨打的百年积木、荆棘打死敌人的地精之角）；开局发牌不拦 | `NoDrawPower.ShouldDraw`：`fromHandDraw` 放行 · `AfterSideTurnEnd` 且 participants 含我 ⇒ 移除 | `battle_trance_stops_draws_only_until_my_turn_ends` |

内核没挪它的清零时点（仍在 `TURN_SCOPED`、我的下一个回合开始清），而是在锁上加了 `!State::enemy_side`：
**每一个决策点上两者的层数逐字相同**（游戏那边已经移除，内核这边已经清零），差的只有敌人回合里那几次抽牌。
对拍语料一帧都没碰到它；变的只有推演（fight_eval 死亡 487 -> 486，act_eval 3842 -> 3841），
是带战斗专注的牌组在敌人回合里多摸到了百年积木那几张。

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
  `REATTACH_POWER` · `PERSONAL_HIVE_POWER` ·
  `SWIPE_POWER` · `HATCH_POWER`。观测天天碰到，内核天天算错，
  而验收全绿 —— **那张表存在的意义就是把这件事变成可枚举的，而不是变成没有。**
  （`SANDPIT_POWER` 2026-09-05、`ILLUSION_POWER` 2026-09-06、
  `PAINFUL_STABS_POWER` 2026-09-13 从这张表里建掉了，见 §2.2 / §2.3 / §2.12。）
* **§2.12 / §2.13 那两批第 3 幕敌人一帧实录都没有**：数值、出招表、新机制全是 `[源码]`，
  守着它们的只有单测。第一次实战遇到时录下来 —— **骑士团那场尤其要录**，
  恶咒/抑制的观测 id（`HEX_POWER` / `DAMPEN_POWER`）是照类名推的，也没被观测确认过。
* **§2.20–§2.23 第 1 幕四批（族母 / 幻灵 + 墨宝 / 珊瑚群 / 暗港杂兵）一帧实录都没有**。
  观测 id（`HARDENED_SHELL_POWER` / `SUCK_POWER` / `SURPRISE_POWER` / `THIEVERY_POWER` / `HEIST_POWER`）是照类名推的；
  **硬化外壳那一栏报的是余额**是从 mod 源码（`DisplayAmount`）读出来的，没有一帧观测确认过 ——
  **鬼祟珊瑚群那场一定要录**：它是整批里唯一一个「观测量的含义」本身就是推断的。
* **§2.25 第 1 幕批 6（藤蔓蹒跚者 / 劫掠者刺客 / 暴徒）一帧实录都没有**。`TANGLED_POWER` 是照类名推的；
  「mod 报的手牌费用含缠结」是从 mod 源码（`GetAmountToSpend`）读出来的 —— **藤蔓蹒跚者那场值得录一次**，
  紧绕藤蔓之后那一帧的手牌费用就是那条 `sync` 扣法的第一个真样本。
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
  所以真正守着它的是四条单测而不是对拍，见 [`design-l1.md`](design-l1.md) 的「L1：进阶」。

> **「尚未被碰过」这类清单本身是最容易烂掉的一节** —— 它记的是"还没发生的事"，
> 而事情一直在发生。2026-09-01 重扫时，原文列的四条（虚弱/敏捷/难以杀灭/无实体
> 没被碰过、敌人 AI 不预测、抽牌洗牌不可对拍）**四条全部已经不成立**。
> **改内核之前先重扫一遍语料，别照抄这一段。**
