# 对拍 trace 格式 v1

一条 trace = 一场战斗里 **观测帧 / 动作 / 观测帧** 的交替序列。
录制器 (`tools/record_trace.py`) 写，`cargo run --bin verify` 读。

真实游戏是唯一权威。trace 是它的证词，内核是被告。

录制用**固定的一条命令** `record_trace.py --plan`，动作写进
`traces/_plan.txt`（命令字符串一变就要重新过权限确认）。语法和落盘语义见
`sts2core/CLAUDE.md` 的「录制：永远只用这一条命令」。

---

## 从 mod 源码里读出来的四条硬约束

这些不是设计偏好，是 `STS2MCP/McpMod.StateBuilder.cs` 已经决定的事实，
格式必须绕着它们走。

### 1. 敌人按 `combat_id` 索引，不按下标，**也不按 `entity_id`**

`BuildBattleState` 只序列化 `creature.IsAlive` 的敌人
（[McpMod.StateBuilder.cs:1087](../../STS2MCP/McpMod.StateBuilder.cs)）。
敌人一死就从数组里消失，后面所有下标平移。

**比下标平移更坑的是 `entity_id` 会重新编号。** `entity_id` 由 `BuildEnemyState`
里的 `entityCounts` 现场生成，而那张计数表每次序列化都重建，只统计**存活**的敌人。
实测（2026-08-15 第1幕第7层）：两只小啃兽 `NIBBIT_0` / `NIBBIT_1`，打死 `NIBBIT_0`
之后，活着的那只 `entity_id` 从 `NIBBIT_1` **变成了 `NIBBIT_0`**，而它的
`combat_id` 始终是 2。拿 `entity_id` 当键会直接张冠李戴。

顺带一个实战陷阱：出牌接口的 `target` 参数收的正是 `entity_id`，
所以**每次出牌前都必须重读状态**，不能沿用上一帧记下的 id。

**格式对策**：`enemies` 是一个以 `combat_id` 为键的 map，不是数组。
内核侧的 `enemy_def[i]` 槽位由 `replay` 维护一张 `combat_id -> slot` 表来对齐，
死掉的敌人保留槽位（内核的 `Entity.hp = 0`），不参与位置比较。

### 2. 抽牌堆顺序 ——「丢了」这条 2026-08-29 部分解除了

**原本**：`draw_pile` 输出前按 **稀有度 + 卡 ID 排序**
（[McpMod.StateBuilder.cs:1133](../../STS2MCP/McpMod.StateBuilder.cs)），
真实抽牌顺序不可恢复。

**现在**：给 STS2MCP 打了一个**本地补丁**（不是上游），
在同一处多输出一个 `draw_pile_order` —— **就是 `combatState.DrawPile.Cards`
本身，没排序，下标 0 是牌堆顶**。原来的 `draw_pile` 一个字节没动，
markdown 渲染也没动，所以补丁只有 17 行、`git pull` 时好合。

> **真实顺序一直就在那儿，是那次排序把它扔了。**
> 和 `max_potion_slots`、`draw_pile` 内容本身是同一个模式：
> **别以为拿不到，先去看 raw。**

方向由 [源码] 定死，不是猜的：
`CardPileCmd.Draw` 取 `drawPile.Cards.FirstOrDefault()`，
`CardPile.MoveToTopInternal` 是 `_cards.Insert(0, card)`。
**内核的 `State::draw` 反过来，顶在末尾**（`pop_draw_top` 从末尾取），
所以 `sync` 填进去要**反向**；`sync_puts_the_real_draw_order_top_last` 钉着这条
（写反了不会报错，只会让 planner 一路信一个反着的前缀）。

拿到它之后 `sync` 走一条新路：整堆逐字照抄、一个随机数都不掷、
`n_draw_known` 拉满。跨回合 planner 的机会节点在前缀耗尽之前
全部塌缩成确定节点。

**没有这个字段时（老 trace、没打补丁的 mod）行为一个字没变**：
内容当多重集、顺序自己洗一个样本、`n_draw_known = 0`。
两种 trace 会长期共存（老语料重录不了），
所以**代码里不许假设 `n_draw_known` 在 `sync` 之后是 0**。

**仍然没解决的一半**：抽牌堆抽空之后要把弃牌堆洗回来，
那次洗牌的结果内核仍然不可知（游戏的 `Rng.Shuffle` 状态没暴露）。
所以确定的只有**当前这一堆**，不是整场战斗。

### 3. 牌堆里的牌没有 `id` 和 `is_upgraded` ——「附魔」这一半 2026-09-03 补上了

`BuildPileCardList` 只给 `name` / `cost` / `star_cost` / `description`
（[McpMod.StateBuilder.cs:1303](../../STS2MCP/McpMod.StateBuilder.cs)）；
只有**手牌**走 `BuildCardInfo`，带完整的 `id` / `is_upgraded` / `type` / `rarity`。

**格式对策**：trace 里牌堆的牌记 `name` + `cost`，但**对拍只比 `name` 多重集**
（内核侧没有"显示费用"这个概念，比 cost 得先从 `CardDef` 反推，得不偿失）。
手牌是强身份，走完整比较。这够抓住"该进消耗堆的进了弃牌堆"这类错误，
抓不住升级态错误。够用。

**但「弱身份」不该顺手把附魔一起丢掉。** 附魔是**逐实例**的，名字和升级态
都表达不了它：一张带灵巧的耸肩无视给 10 点格挡而卡表说 8。手牌 2026-09-01
就带上了 `enchantment`，牌堆那边没有 —— 于是**同一张牌在手里是 10、
在牌堆里是 8**，段内才抽出来的那一张按 8 算。
`act1_f14_phantasmal_gardeners` 的整回合对拍红了一帧（游戏 15 / 内核 13），
差的正好是那 2 点。

2026-09-03 的本地补丁（第三个）加了两处，**都只增字段**：

| 键 | 是什么 |
|---|---|
| `draw_pile_order_enchantments` | 和 `draw_pile_order` **逐位置配对**的附魔数组，同长度、同下标，没附魔是 `null` |
| `discard_pile[].enchantment` / `exhaust_pile[].enchantment` | `BuildPileCardList` 每张牌多一个字段（这两堆没被重排，位置本来就对得上） |

**为什么抽牌堆的那份要挂在 `draw_pile_order` 上而不是 `draw_pile` 上**：
后者被按稀有度+id 重排过（约束 2），下标已经不指向同一张实体牌了。
按名字回配也不行 —— 牌组里有三张打击而只有一张带附魔时那是**欠定**的。

trace 侧对应 `draw_order_enchant` / `discard`、`exhaust` 里的 `enchantment`。
**老 trace / 没打这版补丁的 mod 一律按"没附魔"处理**，方向是低估，
和 `draw_order` 缺失时的回退是同一个处置。

### 4. 敌人意图是文本标签，不是数字

`intents[].label` 是渲染后的字符串，没有结构化的伤害值
（[McpMod.StateBuilder.cs:1370](../../STS2MCP/McpMod.StateBuilder.cs)）。

**格式对策**：**不预测敌人行动**。`end_turn` 帧上，敌人打了多少由
`obs_after - obs_before` 反推，内核侧直接注入这个结果。
这符合 `sts2core/CLAUDE.md` 里"敌人 AI 只是循环出招"本来就在故意没做清单里。
原始 intent 文本照抄进 trace，留给后面灌内容用。

见过的标签格式：纯数字 `"5"` / `"23"`，多段 `"1×8"`（乘号是 U+00D7，
`parse_intent_label` 同时认 `x`/`X`/`×`），Defend/Buff/Debuff/Sleep 一律空串。

标签是**含攻击方力量的最终值**，不再过攻击方乘区 —— 两个独立证据：
立柱构造体基础 7，力量 2 时标签 9、力量 4 时标签 11；旧日雕像基础 13，
力量 10 时标签 23 且实打 23。
**防御方乘区是否也算进标签里仍未验证**（需要一局能给自己挂易伤的仗）。

---

## 顶层结构

```json
{
  "version": 1,
  "recorded_at": "2026-08-14T21:03:11+08:00",
  "run": { "act": 1, "floor": 3, "ascension": 0, "character": "铁甲战士", "room_type": "monster" },
  "settle": { "poll_interval_ms": 40, "stable_polls": 3, "timeout_ms": 4000 },
  "frames": [ /* Frame, ... */ ],
  "notes": "自由文本"
}
```

`settle` 记录录制时用的稳定判据，因为**时序是这套东西最可能的失败点**：
如果 mod 在动画途中就返回状态，对拍会满屏假阳性。见下面「时序证据」。

## Frame

```json
{
  "i": 0,
  "obs": { /* Obs */ },
  "action": { /* Action */ } | null,
  "settle": { "polls": 3, "ms": 128, "unstable": false, "intermediate_differs": true }
}
```

* `obs` —— 执行 `action` **之前**的观测。
* `action` —— 在这一帧上执行的动作；最后一帧为 `null`（终局观测）。
* `settle` —— 执行完 `action` 后等待稳定的证据，见下。

第 `i` 帧的 `action` 作用后应当得到第 `i+1` 帧的 `obs`。这就是被验证的命题。

## Obs

只保留内核有对应概念的字段，外加身份信息。原始 JSON 由录制器另存
`--raw` 旁路文件，不进 trace（否则 trace 会大到没法读）。

```json
{
  "round": 1,
  "side": "player",
  "is_play_phase": true,
  "energy": 3,
  "max_energy": 3,
  "player": {
    "hp": 70, "max_hp": 70, "block": 0,
    "status": { "vulnerable": 2, "strength": 1 }
  },
  "enemies": {
    "17": {
      "entity_id": "jaw_worm_0", "name": "颚虫",
      "hp": 42, "max_hp": 42, "block": 0,
      "status": { "ritual": 3 },
      "intents": [ { "type": "Attack", "label": "11", "description": "..." } ]
    }
  },
  "hand": [
    { "slot": 0, "id": "strike", "name": "打击", "cost": "1",
      "type": "Attack", "rarity": "Basic", "upgraded": false,
      "can_play": true, "target_type": "SingleEnemy",
      "description": "造成6点伤害。" }
  ],
  "relics": [ { "id": "VAMBRACE", "name": "臂甲", "counter": null } ],
  "draw_count": 5,
  "discard": [ { "name": "防御", "cost": "1" } ],
  "exhaust": [],
  "pending": null
}
```

`relics` **只记身份，不记描述**：描述是渲染文本（带计数器/状态），而内核认的是
`id`。遗物建模是增量的 —— 内容表 `content::RELICS` 里只有"内核对它有话可说"的
那几件，其余靠这里的名字被 `solve --live` 点名报出来。**沉默地漏掉一件遗物是
最坏的情况**：臂甲让防御+ 从 8 点格挡变成 16 点，而在补这个字段之前，
内核连它存在都不知道。

`status` 是 `power_id -> amount` 的 map，直接来自 `BuildPowersState` 的
`id` / `amount`（`DisplayAmount`）。**不做名字翻译**——翻译放在 `replay.rs` 的
映射表里，trace 保持原样，这样映射表改了不用重录。

`pending` 非空表示游戏开着一个选牌界面：
```json
{ "kind": "card_select", "prompt": "选择一张牌消耗", "can_confirm": false,
  "selected": [2], "candidates": [ /* 同 hand 的卡片对象 */ ] }
```

## Action

```json
{ "kind": "play_card",  "slot": 0, "card_id": "strike", "card_name": "打击", "target": "jaw_worm_0" }
{ "kind": "end_turn" }
{ "kind": "select_card", "slot": 2 }
{ "kind": "confirm_selection" }
```

`play_card` 冗余记了 `card_id` / `card_name`：`slot` 会随手牌变化漂移，
出问题时靠名字才能一眼看出录的是哪张牌。`replay` 校验二者一致，不一致直接判
trace 损坏而不是内核错。

## 时序证据（`settle`）

录制器发出动作后，按 `poll_interval_ms` 轮询状态，直到**连续 `stable_polls`
次观测完全相同**才认为结算完毕。记录：

| 字段 | 含义 |
|---|---|
| `polls` | 一共轮询了几次 |
| `ms` | 从发出动作到稳定的毫秒数 |
| `unstable` | 到 `timeout_ms` 仍未稳定 —— 这一帧不可信 |
| `intermediate_differs` | 第一次轮询的结果和最终稳定态**不同** |

`intermediate_differs=true` 就是"存在动画中途状态"的直接证据。
**如果全场都是 `false`，说明 POST 返回即结算完毕，settle 逻辑可以简化。
这是开局第一场战斗要回答的两个未知量之一。**
另一个是手牌 `id` 能不能稳定对上 `content.rs` 的键——由 `verify` 的
`UNKNOWN_CONTENT` 统计直接回答。

## 对拍时哪些字段被检查

| 字段 | 检查 | 理由 |
|---|---|---|
| `player.hp` / `block` | **严格** | 伤害管线的主要输出 |
| `player.status[*]` | **严格**（仅已映射的 status） | 力量/易伤/虚弱等乘区输入 |
| `enemies[*].hp` / `block` | **严格** | 伤害管线的主要输出 |
| `enemies[*].status[*]` | **严格**（仅已映射的） | 同上 |
| `energy` | **严格** | 费用规则（踩踏减费、无情猛攻） |
| `hand` | 多重集，强身份 | 打出的牌离手、抽到的牌进手 |
| `discard` / `exhaust` | 多重集，弱身份（name+cost） | 消耗 vs 弃牌走向 |
| `draw_count` | 只比张数 | 内容/张数可比；顺序见约束 2 |
| `draw_order` | **不比** | 输入，不是被验对象：它就是牌序的权威 |
| `round` | 严格 | 回合推进 |
| 未映射的 status | **忽略并计数** | 内核没这个概念，报出来只是噪音；计数用于排下一步该实现啥 |

## 三态结果

| 结果 | 含义 | 是不是失败 |
|---|---|---|
| `MATCH` | 检查的字段全一致 | 否 |
| `MISMATCH` | 内容都认识，但状态对不上 | **是。这是唯一要修的信号** |
| `UNKNOWN_CONTENT` | 牌/敌人/status 不在 `content.rs` | 否，进「待导入」清单 |

`UNKNOWN_CONTENT` 帧被跳过（不 diff、不中断），但它的原始卡面文本会被
`verify --emit-missing` 导出，正好是第 3 步灌内容的输入。
**一条 trace 会随着内容表变全而自动变得更有价值 —— 所以早录不亏。**

## 误差不级联：每帧独立

`replay` 在 diff 完一帧之后，**用观测值重新同步内核状态**再走下一帧。
每一帧因此都是一次独立的"一步预测"检验。

不这么做的话，一处真实错误会在后面几十帧里派生出几十个假错误，
第一处不一致之后的报告全是噪音。

### 例外：观测里没有的每回合计数器必须自己带

重新同步只能恢复**观测里有的东西**。这几个量 mod 根本不输出：

| 字段 | 谁读它 |
|---|---|
| `attacks_played` | 踩踏（每打出一张攻击牌减 1 费） |
| `free_attack` | 无情猛攻（下一张攻击牌 0 费） |
| `hp_lost_this_turn` | 怨恨（本回合失过血就攻击两次） |
| `exhausted_this_turn` | 邪眼 / 被遗忘的仪式 |
| `cards_played` | （缓慢不靠它——缓慢存在敌人的 status 里，是观测量） |

如果每帧同步都把它们清零，这些牌会报出**假 MISMATCH**：踩踏最典型，
游戏按减费收 1 点能量，内核按 3 费算，直接判"内核拒绝了这个动作"。
那看起来和真 bug 一模一样。

**对策**：`Replayer::counters` 自己按帧累加、跨回合（`obs.round` 变化时）重置，
`sync` 在最后把它灌回状态。这**不是**整回合预测 —— 每帧仍然从观测重新同步，
只有观测里没有的这几个量靠自己带。

`traces/synthetic_stomp_discount.json` 是这条的回归测试：连打两张打击再打踩踏，
去掉这段代码它立刻报 MISMATCH。

### 另一种模式：`--per-turn`

一段连续出牌只在开头同步一次，连着跑完，只比末态 —— 检验的是
「内核连续跑 k 步会不会飘」。限制和成色见 `sts2core/CLAUDE.md` 的
「两种对拍模式」，**同一份 trace 两种模式都能跑，不用重录**。
