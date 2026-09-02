# sts2core

《杀戮尖塔 2》的**纯函数规则内核（L1）+ 局内求解器（L2）**，零依赖 Rust。

适配游戏 **v0.107**。所有实录语料都是在这个版本上打出来的。

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
![game](https://img.shields.io/badge/STS2-v0.107-8a2be2)
![deps](https://img.shields.io/badge/dependencies-0-brightgreen)

---

## 这是什么

```
L1  step(state, action) -> state     纯函数内核，唯一持有游戏规则
L2  局内求解器                        调用 L1，搜「这回合/这场仗怎么打」
L3  构筑顾问                          调用 L2，搜「拿哪张牌/移除/升级/路线」  ← 还没写
```

`State` 是 **POD + `Copy`**（3296 字节），快照和回滚就是一条 `memcpy`。
卡牌 / 敌人 / 能力状态**全是数据表**，加一张牌应该是加一行表而不是加一个 `if`。

规则不是照着 wiki 抄的，是**被真实对局逐帧判过的**：语料由一套 MCP 驱动的录制器
从实际游戏里录下来（每一帧带观测 + 动作），验证器把内核重放的结果和观测逐字段对。
`docs/verification-log.md` 里每一条规则都写着它被哪一帧钉死。

> **一个自信地算错的模拟器，比没有模拟器更危险。** 这份代码的大部分复杂度
> 花在「怎么知道自己错了」上，不是在搜索速度上。

---

## 现在真实的样子

**下面这些是当天跑出来的，不是宣传数字。红的也列在这里。**

### 对拍（2026-09-02，61 条实录 + 2 条合成 + 1 条被动）

| 项 | 读数 | 说明 |
|---|---|---|
| `cargo test --release` | **301 通过** | |
| 一步预测 | **5 帧不一致** | 4 帧在第 3 幕 Boss 的两次复活窗口；1 帧在一条**录在附魔补丁之前**的老 trace（观测缺字段，修不好，只能重打一场录） |
| 整回合预测 | **2 帧不一致** | |
| 敌人出招预测 | 259/316 = **81%** 完全一致 | 允许集合平均 **1.10** 手/次（集合越大这个百分比越不值钱）；0 只对不齐 / 0 只未知；**6 例落在允许集合外**（4 例是击晕那个已知洞，2 例还没归因）|
| L2 自洽性 | 219 回合，**0 例实战线赢过穷尽搜索** | 8 个回合没搜完（预算 200000），那几行是启发式排名不是解 |
| 跨回合 rollout（solver 策略） | P1 154/159 · P3(a)(b) 各 0 · P4 0/16064 | 硬判据全过 |
| 跨回合 rollout（fast 策略） | **✗ P4 红** | 生产口径（30 回合上限）有 **1/16064** 条推演没打完就被截断。基线曾是 0/15872 —— 新加的地道虫「钻地期间格挡不清零 + 32 格挡」是头号嫌疑，**还没查** |
| 跨回合 planner（D=2） | P5 配对 16064 对，均值 **+3.7 血**，更好 6301 / 更差 971 / 打平 8792 | 硬判据全过 |
| 同局面换随机种子，D=2 换不换线 | **224/286 = 78% 不变** | 也就是 **22% 的回合它换个种子就改主意**（单回合求解器是 0，回合内穷尽、确定）|

吞吐（`cargo run --release --bin bench`）：单线程 **3.61 M steps/s**、32 线程 55.3 M。
**这台机器上 bench 波动很大，别拿它当基线**，也别拿它和别的机器比。

### 内容表

| 表 | 内核 | 权威表（从游戏 `/api/v1/wiki` 导出）|
|---|---|---|
| `CARDS` | **114** 项 | 128 张 —— 覆盖 **111/128**，另有 1 个占位 + 2 张权威表还没收录的（战鼓 / 涅奥之怒）|
| `ENEMIES` | **71** 项（含合成假人和占位）| wiki 115 只 |
| `POWERS` | **70** 条触发规则 | 20 个钩子每个都有真牌在用，没有预留的空钩子 |
| `RELICS` | **96** 件 | 97 件。**"进表"不等于"建模"** —— 战斗层真正建模的约 38/61（2026-09-01 的分类快照，见 `docs/relic-audit.md`）|
| `POTIONS` | **23** 个槽 | 已发现 40 瓶，其中 21 瓶拿到了游戏原文 |

> 权威表是「**这个存档发现了多少**」的快照，会随着打得多而涨。所以"还欠多少"的
> **分母会变大**，别把它读成退步 —— 分子（建了几件）和分母（发现了几件）是两件事。

---

## 已知的洞

**这一节存在的理由**：全绿只说明语料照得到的地方没有分歧。

* **还缺 17 张牌**，每张各自卡在一个还没有的机制上（「从生成的候选里挑一张」的
  `Pending`、抽牌数修饰器、重放关键字、固有……）。**不是忘了填。**
* **23 件遗物没建**，5 件卡在同一个洞（附魔系统）。另有一整类
  `ModifyXxx`（"改一个正在算的数值"，8 件）在现有三张表里**没有位置** ——
  硬加就得往 `damage.rs` 塞 `if 有没有某遗物`，那是设计上明确禁止的方向。
* **10 条 `KNOWN_UNMODELLED`**，是欠账清单不是白名单。其中若干条方向是**乐观**的
  （内核低估那一场），各自点了名卡在哪。
* **雾菇的复活是欠定的** —— 分不出是它重新召唤还是敌人自己的能力，按规矩留空。
* **`sync` 不还原 `Pending`**：游戏开着选牌界面时内核会以为可以随便出牌。
  `solve --live` 现在遇到 `pending` 直接拒绝作答 —— 那是止血不是修好。
* **抽牌只有当前这一堆是确定的**：抽牌堆抽空之后那次洗牌不可知（游戏没暴露 RNG 状态）。
* **跨回合 planner（D=2）不当驾驶员**：它平均更好（+3.7 血），但**同一个局面换个
  随机种子有 22% 会改主意**。实战一个回合只发生一次，要的是单次可重复性。
  它在实战里只作为**分歧样本**并排报出来。
* **L3 还不存在。** 构筑决策目前仍由一个旧的 Python 顾问承担，它只认 35 张牌、
  看不见 archetype、敌人模型里没有复活/召唤/多阶段。

---

## 快速上手

```bash
cargo build --release
cargo test --release
```

对拍验收（**glob 要靠 shell 展开，验证器自己不认通配符** —— 这些是 bash 命令；
PowerShell 不给原生 exe 展开 glob，会报 `os error 123`）：

```bash
cargo run --release --bin verify  -- traces/act*.json traces/synthetic_*.json
cargo run --release --bin verify  -- traces/act*.json traces/synthetic_*.json --per-turn
cargo run --release --bin verify  -- traces/act*.json traces/synthetic_*.json --predict-enemy
cargo run --release --bin solve   -- traces/act*.json
cargo run --release --bin rollout -- traces/act*.json
cargo run --release --bin rollout -- traces/act*.json --policy fast
```

动了跨回合 planner 才要跑的第七条（约 2 分半，上面六条加起来只要几秒）：

```bash
cargo run --release --bin rollout -- traces/act*.json --policy plan --plan-depth 2 --depth-sweep 1
```

> **别写成 `traces/*.json`** —— 那个目录里除了语料还放着权威数据表，
> 验证器会在它们身上报「不支持的 trace 版本 0」然后停下。

每一项都有**独占的检验面**，这是这套验收的全部价值：单测覆盖对拍照不到的单点规则；
`--per-turn` 覆盖"观测里没有、又会跨动作累积"的东西；`--predict-enemy` 只判
**下一手是什么**、不判这一手打多少；`bin/rollout` 独占**敌人在预测路径上打多少**
（前面四种模式对这件事全部沉默 —— 实测过：把敌人攻击改成 −3，只有这条会红）。

---

## 接下来做什么

按优先级，细节在 [`docs/roadmap.md`](docs/roadmap.md)：

1. **L3 构筑顾问** —— 当前最大的空缺，也是"卡牌构筑还在做"指的那件事。
   目标是能评估「**牌组 vs 一幕的遭遇分布**」，而不是今天这种「固定牌组 + 一张
   边际改动」。后者结构性看不见 archetype：消耗流的组合件逐个量都是弱的，
   跨一整幕它才是引擎。
2. **查 `rollout --policy fast` 的 P4 红** —— 判据现成：抬高回合上限，
   数字变了就是"仗真打不完"，不变就是挂死。
3. **补内容** —— 17 张牌 / 23 件遗物 / 17 瓶药水，各自卡在一个具体机制上；
   外加给 `ModifyXxx` 那一族遗物设计一张新表。
4. **叶评估斜率契约的收尾** —— 能力项现在有一层 1% 的安全余量吸收取整误差，
   但那个常数还欠一段推导；另有一版"精确"实现（有理地平线 + 按活跃项分摊斜率预算）
   已经量过（100 格表 + 40000 例随机猎杀全 0 倒挂），选哪个还没定。
5. **把权威数据表从 `traces/` 挪进 `data/`** —— 今天靠 glob 绕开，是个真疙瘩。
6. （可选）**价值网络引导搜索**。现在 GPU 闲置是对的：这个负载分支密集、
   状态极小，GPU 占用率会很差。

---

## 项目结构

```
src/
  state.rs     POD 状态、状态位、牌区、三条分流 RNG（splitmix64）、抽牌堆已知前缀
  step.rs      step / legal_actions / begin_combat / 触发器分发 / 消耗收口
  damage.rs    唯一的伤害管线（乘区顺序只在这一处定义）
  ops.rs       Op / EOp / CardDef / EnemyDef 定义 + 药水表
  content.rs   CARDS / ENEMIES / POWERS / RELICS / 生成池 / 各类状态分类表
  solver.rs    L2 单回合求解器：Threat / solve_turn / 目标函数 / 药水定价 / 局面指纹
  plan.rs      L2 跨回合 planner（限深 expectimax）：机会节点 / 置换表 / 叶评估
  rollout.rs   跨回合推演：Policy(Fast/Solver/Plan) + 威胁预测 + 终局统计
  replay.rs    对拍：观测同步 + 逐字段 diff
  json.rs      最小 JSON 读取器（为了保持零依赖）
  bin/         verify（对拍，三种模式）· solve（L2 验收 + 实战建议）
               rollout（跨回合验收）· calib（标定导出）· bench
tools/         录制器、权威表导出、标定报告、种子扫描（只用 Python 标准库）
traces/        act*.json 实录语料（**不可再生**）· synthetic_* 合成 · passive_* 被动录制
               外加几张权威数据表（该挪走，见上）
docs/          verification-log.md（证据链）· roadmap.md · trace-format.md · relic-audit.md
```

## 文档

| 文件 | 管什么 |
|---|---|
| [`CLAUDE.md`](CLAUDE.md) | 设计、不变量、接口、验收怎么跑 —— **动代码之前从头读** |
| [`docs/verification-log.md`](docs/verification-log.md) | **证据**：每条规则被哪一帧钉死、验证器抓到过哪些真错 |
| [`docs/roadmap.md`](docs/roadmap.md) | 做完了什么 / 还没做 / **故意**没做 |
| [`docs/trace-format.md`](docs/trace-format.md) | trace 格式 + 从 mod 源码读出来的硬约束 |
| [`docs/relic-audit.md`](docs/relic-audit.md) | 遗物按钩子面分类（计数是快照，分类不过期）|

## 开源协议

[MIT](LICENSE)
