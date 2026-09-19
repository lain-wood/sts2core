# L1 设计：规则内核

内核长什么样、为什么这样。**规则本身**（伤害管线的每一个数、玩家给的判定）以
[verified-rules.md](verified-rules.md) 为准；分层、五个入口和不变量在
[`../CLAUDE.md`](../../CLAUDE.md)。动 `damage.rs` / `step.rs` / `content.rs` 之前读这份。

## L1：伤害管线（`damage.rs`）

全内核最高杠杆的一处。乘区顺序是从实战日志反推、再被对拍钉死的。

> **先看这一条**：下面 2–6 那五个乘区（腐化 + 缩小/虚弱/易伤/缓慢）**只对"攻击"
> 生效**。[源码] 它们各自第一句都是 `if (!props.IsPoweredAttack()) return 1m;`，
> 而 `IsPoweredAttack = Move && !Unpowered`。所以**药水 / 遗物 / 能力牌打出的伤害
> 一概不过它们**，走 `damage::apply_modifiers_unpowered`。
> **7 和 8（难以杀灭 / 无实体）没有这条 gate，两条路都要过。**

```
1. 卡面基础值（含升级、含附魔加值）
2. 腐化 ×1.5        ← 作用在基础值上，在力量【之前】
3. + 锋利加值 + 力量 + 活力
   ---- 以下乘区【累乘，最后只取整一次】 ----
3.5 攻击方缩小 ×0.7
4.  攻击方虚弱 ×0.75
5.  防御方易伤 ×1.5
6.  防御方缓慢 ×(1 + 0.1 × 本回合此牌之前打出的牌数)
   ---- 取整 ----
7. 难以杀灭  min(d, cap)
8. 无实体    d = 1
9. 扣格挡，余下进 HP —— **硬化外壳和滑溜在这一步**：格挡照常被打满，
   余下的那一截先按硬化外壳的**本回合余额**封顶、再被滑溜压成 1
```

> **第 8 步和第 9 步的那个 1 不是同一个 1。** 无实体封的是**伤害**
> （[源码] 还有一条 `ModifyDamageCap`），所以 26 点只扣得掉 1 点格挡；
> 滑溜（墨影幻灵 / 墨宝）**只有** `ModifyHpLostAfterOsty`，封的是**掉血** ——
> 格挡该清零还是清零。消费点因此在 `damage::absorb` 而不是 `apply_modifiers`，
> 逐条见 [verified-rules 的 2.21](verified-rules.md)。
>
> **硬化外壳（鬼祟珊瑚群，2026-09-19）也在第 9 步，但它和难以杀灭（第 7 步）不是一回事**：
> 难以杀灭每一下封顶、在扣格挡之前；硬化外壳是**一个回合累计**最多掉 20、在扣格挡之后，
> 余额在**双方各自回合开始的最早一档**回满（`Hook::SideTurnStart`）。
> 内核存的是**余额**（观测报的就是余额），上限是按名字挂的私有标记，见 [verified-rules 的 2.22](verified-rules.md)。

> **不要凭直觉改这张表里的数字。** 「每一步都向下取整」这条曾经写在这里，是错的，
> 被第 1 幕 Boss 的一帧推翻。全部证据在
> [verified-rules 的 1.1](verified-rules.md#11-步骤-356-累乘只在最后取整一次)。

**活力有一条会改变出牌顺序的规矩：多段牌的每一段都吃满加值。**
[源码] `AttackCommand.cs` 的 `Hook.BeforeAttack`/`AfterAttack` 都在多段 `for`
循环**外面**，所以活力挂在整条 `AttackCommand` 上：8 活力配双重打击（5x2）是
**(5+8)x2 = 26**，不是 5x2+8。单段牌上两种读法给出相同的数，只有多段牌分得开。
另一半：活力是 `Counter` 型、**没有回合末衰减**（没花掉的跨回合留着），
但**一张攻击牌打完整个清零**。

### 段数也可以是**算出来的**

伤害管线管的是「每一段打多少」，**段数是另一件事**。今天只有一张牌的段数不是常数：

扯碎（[源码] `TearAsunder`）`WithHitCount(Calculate(target))`，
`Calculate = CalculationBase(0) + CalculationExtra(1) × (1 + M)` = **1 + M**，
`M = State::hp_loss_hits`（本场挨穿过几次）。所以它走 `Op::DamagePerHpLossHit`
而不是 `Scale` —— `Scale` 那一族改的是**基础伤害**，改不了段数。

三条容易写错的：

* **段数在打出的那一刻就定死**（源码里是构造 `AttackCommand` 时求值）。
  写在循环里重读的话，敌人带荆棘时每一段反弹都会把计数器顶上去，
  这张牌会自己越滚越长。
* **`hp_loss_hits` 数的是次数不是血量**，判据是 [源码] 的
  `DamageReceivedEntry` 且 `UnblockedDamage > 0` —— 被完全格挡的不算，
  而来源一概不问（敌人打的、荆棘反弹的、放血那种自伤全算）。
  收口在 `step::after_player_hp_lost`。
* **卡面上那个「（命中3次）」是快照，不是定义。** 游戏把算好的段数渲染进了
  描述文本，照它写死 `hits: 3` 是这张牌第一版的错法。反过来这也是内核唯一的
  观测来源：`sync` 从那句话反推 `hp_loss_hits`（`replay::observed_tear_hits`），
  牌不在手上时那一栏是 0，**方向是低估**。

---

### 附魔

一张牌可以带一个附魔（[源码] `CardModel.Enchantment`，至多一个），
观测里从 2026-09-01 起有这个字段（父目录的第二个补丁），
**牌堆里的牌从 2026-09-03 起也有**（第三个补丁）。

那半年多的空档是有代价的：**同一张牌在手里给 10、在抽牌堆里给 8**，
段内被抽出来的那一张就按 8 算 —— `act1_f14_phantasmal_gardeners` 的整回合
对拍就红在这里。老 trace 拿不到这个字段时**一律按没附魔算**（方向是低估），
和 `draw_order` 缺失时的回退是同一个处置。

内核侧是 `CardInst.ench`（`content::ENCHANTS` 的下标+1）+ `ench_amt`（`Amount`），
**`State` 一个字节没涨** —— `flags` 从 `u16` 窄到 `u8`（当时只有 5 个位）腾出来的地方
正好装下这两个字节，`CardInst` 仍然是 8 字节。`solver::card_ident` 里有一条
`size_of::<CardInst>() == 8` 的编译期断言守着这件事（它的位宽是精确的，不是哈希）。

2026-09-06 之前这里是一个 `block_bonus: i8` —— **装得下的只有五个钩子里的一个**，
其余（伤害加/乘、关键字）一律当成"没有效果"，而那些的**方向并不统一**：
锋利是低估、腐化是低估、**王室认证是把一张该保留的牌弃掉**。

### 这张表的形状 = `EnchantmentModel` 的虚方法表

[源码] 附魔只有六个口子能改游戏：`EnchantBlockAdditive` / `EnchantBlockMultiplicative` /
`EnchantDamageAdditive` / `EnchantDamageMultiplicative` / `EnchantPlayCount`，
外加 `OnEnchant`（改关键字和费用）和 `OnPlay`（打出时多做一件事）。

`content::ENCHANTS` **只装内核今天真的消费的那四个**：格挡加 · 伤害加 · 伤害乘 ·
关键字。**其余的口子没有字段** —— 加一个没人读的字段和加一个空钩子是同一种错，
它看起来像建好了。三个消费点各自收口：`Op::Block`（`ench_block_add`）·
`resolve_ops` 的 `bonus` 和 `base_mul`（`ench_damage_add` / `ench_damage_mul`，
后者和老的 `F_CORRUPT` 是同一个口子）· `replay::push` 把关键字落进 `CardInst::flags`。

**22 种全部进表**，`modelled` 那一列才是进度（现数见 [content.md](content.md)）。
没建全的会被**两处印出来**：`verify` 的「没建全的附魔」和 `solve --live` 的
「附魔」那一行（连同它缺哪一个机制）。
**这两处 2026-09-06 才补** —— 在那之前 `Report::unknown_enchantments`
只往结构体里写、没有任何地方印，和「没映射的 status」是同一类静默洞。
建全的：灵巧（格挡+`Amount`）· 锋利（伤害+`Amount`）· 直觉（伤害×2）·
**王室认证（固有+保留）** · 沉稳（保留）。
没建全的每一条都写着**卡在哪一个还没有的机制**，
`replay` 把它们点名进 `Report::unknown_enchantments`（表里没有、和有但没建全，
对内核是同一件事）。

**id 是从类名推的**（`RoyallyApproved` -> `ROYALLY_APPROVED`），
观测里见过的两个都对上了，其余 20 个对不上会被点名，不会静默算错。

### 保留和固有：附魔的第四个口子有真消费点了

`F_RETAIN` / `F_INNATE` 两个 flag 位一直存在、**一直没人读**（路线图里挂着
"flag 位有了，`step` 还没用"）。王室认证给的正是这两个，于是它们各自接上了：

* **保留** —— `step::end_turn_impl` 弃手牌那个循环跳过带 `F_RETAIN` 的牌。
  [实测] 2026-09-06 `act3_f46_elite_soul_nexus` 两个回合边界：带王室认证的均衡+
  留在手上，同一手的添柴+ 被弃掉。**和均衡的 `St::Entrench` 不是一回事**：
  那个保整手牌、只保一个回合。
* **固有** —— `step::begin_combat` 洗完之后把它们挪到牌堆顶（内核的顶在**末尾**）。
  多张固有牌之间的相对顺序**没有依据**，保持原有先后，不另外洗一次。

---

## L1：触发器（`Hook` / `PowerDef`）

**能力牌就是带层数的 status**，层数 = 每次触发的效果值（激怒 2 = 每次 +2 力量）。
规则写在 `content::POWERS`，`step::fire(s, hook)` 在固定几个点上把它们跑一遍 ——
`fire()` **不认识任何具体 status**。

`every_power_card_has_a_rule_and_no_rule_is_claimed_twice` 守着这条：挂了 status
却没人认领的能力牌是张哑弹，不加这个测试没人会发现。姊妹测试
`no_status_handed_out_by_a_relic_or_an_enemy_is_a_dud` 把口径推广到遗物和敌人 ——
它已经真的抓到过哑弹（荆棘、敌人持有的覆甲）。

### 26 个钩子，每个都有真牌在用

**没有为将来预留的空钩子**，这是一条规矩：加钩子必须连着它的消费者一起加。

| Hook | 触发点 | 消费者（`content::POWERS` 里的 `st`，现数走 `count_content.py`） |
|---|---|---|
| `SideTurnStart` | **任何一边**的回合开始的**最早一档**（我的回合：清格挡 / 能量回满 / `TurnStart` 之前；敌人回合：`begin_enemy_turn` 最前面，两条敌人回合路径都走）。[源码] `BeforeSideTurnStart`，不分哪一边 | **硬化外壳**(鬼祟珊瑚群：余额回满到上限)。2026-09-19 为它加的 —— 挂在 `TurnStart` 上的话，水银沙漏 / 滚石那几点算进哪个回合的额度就只剩表内顺序 |
| `TurnStart` | **我的**回合开始，能量回满后、抽牌前 | 28 条。恶魔形态 绯红披风 滚石 盾墙 覆甲(玩家掉层，**第 2 回合起**) 巨像 好勇斗狠 抱抱先生 水银沙漏 灯笼 锚 弹珠袋 红面具 准备背包 烛台 开心小花 花粉核心 佩尔之肉 光耀 赤牛 摆动球 舵盘 **号角靴钉 宝石面具 碎石者 小血瓶 缩放仪 古茶具** |
| `HandDrawn` | **我的**回合开始、**手牌已经发下来之后**（`open_hand` 末尾） | **风箱 / 骨茶**（开局升级手牌）。和 `TurnStart` 差的就是抽牌那一步 —— 挂在那个钩子上的话它会去升级一手还没发下来的空牌。[源码] 那两件是 `AfterPlayerTurnStart`，而宝石面具是 `BeforeHandDraw`，**这一步分得开这两批** |
| `TurnEnd` | **我的**回合结束，弃手牌前、敌人行动前 | 招架盾 **尖叫酒壶** **斗篷扣**(每张手牌 1 格挡) 覆甲(玩家给格挡) 奥利哈钢 再生 惊逃 缠绕 娇弱 历石 轰鸣 |
| `TurnEndVeryEarly` | 我的回合结束，**在 `TurnEnd` 之前** | 奥利哈钢(快照) |
| `TurnEndLate` | 我的回合结束的**最后一步**：弃完手牌之后、敌人行动之前（[源码] `AfterSideTurnEndLate`） | **瓦解**(知识恶魔的诅咒：这回合剩的格挡先吃它) **缠结**(藤蔓蹒跚者：我的回合结束摘掉；[源码] 是早半档的 `AfterSideTurnEnd`，两档之间没有东西读攻击牌费用) |
| `EnemyTurnStart` | **敌人**整边开始，清完格挡、第一只出手之前 | 覆甲(敌人掉层) 适生力 **幻象(回满血)** **沙坑(即死倒计时)** **接续倒计时**(千足虫：数完回 25 血) |
| `EnemyTurnEndVeryEarly` | **敌人**整边行动完之后的**最早一档**，`EnemyTurnEndEarly` 之前（[源码] `BeforeSideTurnEndVeryEarly`） | **沉睡**(乐加维林族母：最后一个睡眠回合在覆甲给格挡**之前**把覆甲摘掉 ⇒ 那一回合没有墙) |
| `EnemyTurnEndEarly` | **敌人**整边行动完之后、`EnemyTurnEnd` **之前**（[源码] `BeforeSideTurnEndEarly`） | 覆甲(敌人给格挡)。夹在沉睡（早一档）和熟睡（晚一档）中间，顺序写在钩子里不靠表内先后 |
| `EnemyTurnEnd` | **敌人**整边行动完之后 | 高压 仪式 领地意识 逃跑大师 天敌 **熟睡**(熟睡甲虫：减层，归零醒来移除覆甲) **沉睡**(族母：减层；覆甲上一档已经摘了) |
| `PlayerSkill` | 打出技能牌，**牌结算之前** | 激怒（敌人持有）开信刀 活力火花 |
| `PlayerAttack` | 打出攻击牌，**牌结算之后** | 狂怒 精致折扇 苦无 杂耍 |
| `CardPlayed` | 打出任意牌 | 凋萎存在 娇弱 **流电**(电球头：打出能力牌挨 6 点，`TCond::LastPlayedKindIs`) |
| `CardExhausted` | 所有进消耗堆的路径 | 无惧疼痛 黑暗之拥 |
| `GainBlock` | **获得格挡且数值 > 0**，四条加格挡的路径全收口在 `step::gain_block`。**`ctx` = 谁获得了格挡**（玩家是 `usize::MAX`）| 势不可当（[源码] 只认自己那一份）|
| `ApplyVuln` | 我给敌人上易伤，**每个吃到的敌人各一次** | 凶恶 |
| `PlayerLoseHp` | 我方回合内因卡牌失去生命 | 撕裂 |
| `PlayerDamaged` | 我挨伤害 | 百年积木 |
| `Attacked` | 挨敌人一次攻击，**多段各一次**（`ctx` = 攻击者） | 火焰屏障 荆棘 |
| `AttackUnblocked` | 敌人这一段攻击**打穿了格挡**（`take_attack_hit`，两条敌人回合路径都走）。**持有者是攻击者本身**，只有 `ctx` 那只触发 | **纸伤难愈**(咬人卷轴：−最大生命) **剧痛刺击**(实验体阶段 2：塞伤口) **吮吸**(化石追踪者：每段打穿 +3 力量；[源码] 是打完整条攻击再给，等价的前提是敌人一手的面板值只算一次) |
| `EnemyAttacked` | 敌人发起攻击 | 荆棘 |
| `EnemyDamaged` | 敌人挨我一下，**伤害落地之后**（`ctx` = 挨打的那只，**且只有它触发**）。这一下是不是攻击 / 有没有打穿，由 `hit_enemy_with` 写进 `State::last_hit_attack` / `last_hit_unblocked`，规则用 `TCond::LastHitWasAttack` / `LastHitUnblocked` 读 | 蜷身 扑翼 尖叫 耕地 **人体蜂房**(蜂群术士：每段攻击塞层数张晕眩，挡住也塞) **熟睡**(熟睡甲虫：被打穿 −1，归零击晕换招) **沉睡**(族母：被打穿整条清零 + 摘覆甲 + 击晕) **滑溜**(墨影幻灵 / 墨宝：被打穿 −1) |
| `EnemyDied` | 一只敌人死 | 寄生物 **意外**(地精佣兵：召唤卑鄙地精 + 胖地精) 适生力 **蒸汽喷发**(瀑布巨兽：锁成 999999999 血、强制「即将爆发」；爆炸那一手 `EOp::KillSelf` 再死一次才是真死) **幻象** **接续**(千足虫：死亡剥离 + 置复活倒计时) 库存 地精之角 **抢夺力量/速度**(小偷死了才还，退还量读它自己的力量/敏捷 `Amt::OwnerStacksOf`) **恶咒/抑制的施咒者**(幽灵骑士死了解咒、魔法骑士死了把牌升回去；施咒者标记游戏不报，两条路径都按名字从 `content::ENEMY_PRIVATE_MARKERS` 挂) |
| `AllyDied` | 同伴死 | 蟹之怒 **贪食**(噬尸蛞蝓：+力量 且被自己的进食击晕一回合) |
| `CombatVictoryEarly` | 敌人清空、玩家活着，**第一段** | 带骨肉 |
| `CombatVictory` | 同上，**第二段** | 燃烧之血 |

> **薪火之源不在这张表里。** `St::Pyre` 只是显示印记（`MARKER_STATUSES`），
> 真实效果是 `Op::GainMaxEnergy`（`s.base_energy += n`，**永久**，不衰减不带条件）。
> 能力的价值因此有两种结构：**永久资源**和**每回合触发** —— 叶评估的公式得分开写。

### 三条容易写错的

* **`Attacked` 和 `EnemyDamaged` 的 `ctx` 不是一回事。** 前者持有者是玩家、ctx
  是攻击者（火焰屏障要反伤谁）；后者持有者就是 ctx 那只敌人本身。所以 `fire_ctx`
  里对 `EnemyDamaged` 有一条窄判断：**只对挨打的那只触发**。写错了会让打一个敌人
  导致全场一起加格挡，而且**只在多怪场才看得出来** ——
  `curl_up_fires_only_on_the_enemy_that_was_hit` 守着。
* **`PlayerSkill` 在结算之前、`PlayerAttack` 在结算之后**，这个不对称是刻意的：
  前者保住激怒已经验证过的行为，后者是保守选择（全身撞击吃不到自己那一下狂怒
  给的格挡）。
* **钩子归属（给玩家发还是给敌人发）一刀切不了。** `fire` 对绝大多数钩子两边都发，
  而真实规则里两种组合都存在：**盾墙是敌人持有、挂在玩家回合开始**；
  **覆甲同一个 status 两边都能持有、时序却完全不同**（玩家在我的回合末给格挡，
  敌人在它自己的回合末给）。所以门开在**规则自己**身上 ——
  `TCond::OwnerIsPlayer` / `OwnerIsEnemy`，**别去改 `fire_ctx` 的分发**。

**钩子可以套钩子**，由 `MAX_HOOK_DEPTH = 4` 兜底。绯红披风回合开始掉 1 点血会唤醒
撕裂 —— 这是放血流的核心组合，不接上等于把一整套构筑算废。
（最初为了躲死循环写成"钩子不套钩子"，是错的，玩家指出来的。）

> **那个兜底要真的盖到伤害路径上。** `hit_enemy_with` 原来把
> `EnemyAttacked` / `EnemyDamaged` / `EnemyDied` / `AllyDied` 一律按 `depth = 0`
> 点火，于是「触发器打人 -> 挨打的那只再触发 -> 再打人」的环**每绕一圈层数就被
> 重置**，`MAX_HOOK_DEPTH` 形同虚设 —— 表现是**爆栈**，不是一条红。
> 2026-09-12 修成：打牌那条路传 0（逐字同旧），`run_ops` 那条传 `depth + 1`。
> 同一天的另一半是势不可当的归属（见 `GainBlock` 那一行）。
> **两条都是 `bin/act_eval` 照出来的，前八条验收全绿。**

代价：`fire()` 吃掉约 12% 的单线程吞吐。**要优化的话第一步是按 hook 给 `POWERS`
分组**（现在是整表线性扫），别先动 `State` 的布局。

---

## L1：三个类别，别混淆

"卡牌带来的行为"分三类，各有各的表。加牌前先想清楚属于哪一类：

| 类别 | 表 | 能表达什么 |
|---|---|---|
| 触发器 | `POWERS` | 「在某个时点**多做**一件事」 |
| 规则修饰 | `RULE_MODIFIERS` | 「**不要做**某件事」/ 直接改判定 |
| 手牌发作 | `HAND_END` | 「回合结束时这张牌**还在手上**才发生」 |
| 消耗堆发作 | `EXHAUST_END_AUTOPLAY` | 「回合结束时这张牌**在消耗堆里**就自己打出来」 |
| 附魔 | `ENCHANTS` | 「这**一张实例**的格挡/伤害/关键字被改过」 |

`EXHAUST_END_AUTOPLAY` 是 `HAND_END` 的姊妹表，今天各只有一张牌在用
（彼岸咆哮 / 凋萎）。时点由 [源码] `CombatManager.EndPlayerTurnPhaseOneInternal`
定死：`AutoPostPlay` 阶段跑在 `Hook.BeforeTurnEnd` 和弃手牌**之前**。

`RULE_MODIFIERS` 这张表**不带行为**，消费点是 `step.rs` / `damage.rs` 里几个写死的
窄 `if`（壁垒不清格挡、均衡不弃手牌、孤注一掷立刻死、臂甲和坚定不移的充能、失衡、
**恶咒让回合末手里的牌全部虚无**）。
它存在的意义是让这个类别**可枚举**，好让完整性测试区分"故意写死的规则修饰"和
"忘了写规则的哑弹"。**这类牌要是多起来，该给它们一张真正带行为的表，
而不是继续往 `step.rs` 塞 `if`。**

`TURN_SCOPED` 里的 status 在**我的下一个回合开始时**清零，不是回合结束时 ——
火焰屏障要挡的正是敌人回合那几下。

---

## L1：敌人侧

* **召唤** `EOp::Summon`：新敌人用它自己 `EnemyDef` 的 `start_status`
  （爪牙标记就在那儿），且**当回合不行动**。
* **爪牙随主死** `step::no_master_left`：**非爪牙全死光，战斗就结束**，场上剩的
  爪牙跟着消失。两次独立实测（雾菇→利齿之眼、同族神官→两个信徒）。不实现这条，
  L2 会把第 1 幕 Boss 估成 307 血而不是实际要打的 190。
  兜底：场上从没出现过非爪牙时退回普通规则，免得一场纯爪牙的仗开局就判结束。
* **敌人塞状态牌** `EOp::AddCardToDiscard`：史莱姆的 `StatusCard:N` 往我的
  **弃牌堆**塞 N 张黏液（不是手牌）。张数取自意图标签，**只有树枝史莱姆（中）
  那一手实测过**，其余是推的。
* **同名的多个 `EnemyDef` 会互相盖住** —— 名字到 def 是 `position(|d| d.name == n)`，
  第一个匹配就返回。巨斧机器人的「三具身体共用一个 def」因此只能由**层数驱动**
  差异（`TOp::SummonCarryingSelfMinusOne` + `EOp::SelfStatusPerStack`）。

`CardExhausted` 靠 `step::exhaust_card` 收口 —— 内核里有三条进消耗堆的路径
（卡面自带消耗、恶魔之焰清手牌、烙印选牌），**任何新路径都必须走它**。

### 「还在场上」不等于「活着」，一共三处口径

带适生力的实验体被砍死之后血是 0、打不到、**观测里也看不见它**，
但战斗不结束（[源码] `AdaptablePower.ShouldStopCombatFromEnding() => true`）。
三处必须一起读，缺一处就是静默的错：

| 处 | 干什么 | 漏了会怎样 |
|---|---|---|
| `State::any_enemy_present` / `step::no_master_left` | 判胜利用 `alive() \|\| Adaptable > 0` | 砍掉第一条命当场判赢 |
| `Replayer::revive_owed` | 记住"消失前它带着适生力" | `sync` 每帧从观测重建，适生力一丢就退化成上一行 |
| `step` 的 `PlayCard`：**没有活敌人时无目标牌照打** | 和 `legal_actions` 保持一致 | 那个窗口里**每一张牌**都被拒，而"拒绝"的表现是状态没变，不报错 |

最后那条是 2026-09-03 才发现的真 bug，被前两条挡在后面很久 ——
详见 verification-log 的「复活窗口那 4 帧」。

### 出招表和 AI 预测

`tools/dump_bestiary.py` 从 sts2.wiki 扒了 115 个敌人的出招表到
`traces/enemies_wiki.json`。**wiki 不是权威，游戏才是** —— 脚本强制自检，把 wiki 的
HP 和实录里量到的逐个对，这才是它可以被当作*假设来源*的理由。

出招表是**状态机**（`EnemyDef::machine`）：反编译出的 121 只里 82 只本来就是纯
确定性的，那些继续用循环；结构性需要机器的逐条照抄了 [源码]。

判决机制是 `verify --predict-enemy`，判的是**「观测到的这一手在不在允许集合里」**，
不是逐字相等 —— 内核的随机流故意和游戏不一致（不变量 4），"下一手是哪一个"本来
就预测不了。它验的是**「下一手是什么」，不是「这一手打多少」**；后者由默认对拍
模式检验。两者要分开看，修的地方完全不同。

---

## L1：进阶（`src/asc.rs`，**生成产物**）

**十档各自改什么、以及为什么只有两档进 L1，是规则，写在
[verified-rules 的 2.7](verified-rules.md)。** 这里只写内核长什么样。

`State::ascension`（0–10）。战斗层只有两档读得到它，**其余八档一条都不该进 L1**：

| 档 | 改什么 | 谁读 |
|---|---|---|
| **A8 `ToughEnemies`** | 敌人**耐久**：血量（含多形态/孵化）· 格挡 · 覆甲 · 几个 status 量 | `asc::hp_range` / `asc::adjust` |
| **A9 `DeadlyEnemies`** | 敌人**输出**：伤害 · **段数** · 力量成长 · 部分格挡 · debuff 量 | `asc::adjust` |

其余的全在局外：A1 精英数 ×1.6 · A2 **远古事件**回血 ×0.8（不是篝火）· A3 金币 ×0.75 ·
A4 药水槽 −1（内核照抄观测的 `max_potion_slots`，不从进阶推）· A5 开局一张升华诅咒 ·
A6 商店价 · A7 卡牌概率 · **A10 第二个 Boss 只给最后一幕**。
逐条出处在 `data/ascension.json` 的 `outside_combat`。

### 一个收口点，六处过它、三处有意不过

```rust
asc::adjust(def, mv, op_ix, asc, op) -> EOp   // asc < 8 时逐字返回原 op，一次表都不查
asc::hp_range(def, asc) -> Option<(i32, i32)> // 血量区间两档
```

读 `EnemyMove::ops` 的地方一共九处。**六处必须过 `adjust`**：`step::enemy_turn`（执行）·
`rollout::fast_play_turn` 和 `rollout::predicted_threat`（两个威胁预测）·
`plan` 的最坏来袭 · `replay::move_signature`（意图标签 —— 游戏在 A8/A9 显示的
标签本来就是缩放过的）· `bin/solve::upgrade_threat_live`。
**三处有意不过**：`bin/plan_audit::move_has_non_attack_ops` 和 `lib.rs` 的两条
完整性测试 —— 它们只看 op 的**种类**，而进阶从不换 `EOp` 的变体。
**漏一处的表现是那条路径静默地按低进阶算。**

`ascension` 从 trace 的 `run.ascension` 进来（`Trace::ascension` 是数、`run` 那个
字符串是给人看的），`Replayer::for_trace` 带进 `State`。**手上有 `Trace` 就用它** ——
走 `Replayer::new(&t.run)` 会拿到 0，而今天全部语料都是 A1/A2，看不出差别。

### 这张表是**认**出来的，不是抄出来的

[源码] 那边是属性名（`ButtDamage`），内核这边是 `ops` 里的一个数，中间没有词典。
生成器按 **(种类, 低进阶值)** 去认 —— 低进阶值就是内核今天写着的那个数
（A1/A2 实测钉的）。**候选个数和 [源码] 的用点数对不上就整条跳过并报出来**，
欠的那些在 `data/ascension.json` 的 `unmatched` 里逐条写着卡在哪。

> **正因为是"认"的，守卫测试才是这套东西的地基。**
> `asc_ops_low_value_still_matches_the_kernel_op` 第一次跑就抓到生成器把 op 下标
> 数成了"在它认得的那几种 op 里排第几"（活雾「膨胀」夹着一个 `EOp::Summon`）——
> **下标平移不会报错**，只会在高进阶下改掉另一个 op。
> 五条守卫见 verification-log 的 09-08 下午那条，其中
> `ascension_9_actually_raises_the_damage_that_lands` 是**正面证据** ——
> 前四条对一张全是 no-op 的空表也会全绿。

**A8 以上一次都没被观测碰过**（全部实录是 A1/A2），所以整张表是 `[源码]` 档，
不是 `[实测]` 档。对应的安全性质就是那条 `asc < 8` 提前 return。
