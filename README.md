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
L3  构筑顾问                          调用 L2，搜「拿哪张牌/移除/升级/路线」  ← 地基建了，还没接线
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

### 对拍（2026-09-03 晚，64 条实录 + 2 条合成 + 1 条被动）

> **这张表旧了。** 语料后来涨到 78 条实录，读数也跟着变过好几轮 ——
> **当前那一份在 [`CLAUDE.md`](CLAUDE.md) 的「今天的读数」**（那里只有一份，
> 跑一遍就能复现）。这里留着是因为它记着几条已经归因完的红，
> 而**这个仓库栽过"抄早了一版数字"**，所以不在两个地方各维护一份。

| 项 | 读数 | 说明 |
|---|---|---|
| `cargo test --release` | **309 通过** | |
| 一步预测 | **1 帧不一致** | 一条**录在附魔补丁之前**的老 trace（观测缺字段，修不好，只能重打一场录）。第 3 幕 Boss 复活窗口那 4 帧 9-03 修掉了 —— 一半是同步层的 `revive_owed`，另一半是 `step` 和 `legal_actions` 对同一个局面给出不同答案的真 bug |
| 整回合预测 | **2 帧不一致** | **两帧是同一件事**：一张带「灵巧」的耸肩无视（+2 格挡），内核按卡表算 8 而游戏给 10。一条 trace 早于附魔字段；另一条那张牌是**回合中途被抽出来的**，而牌堆的附魔要到 9-03 的第三个 mod 补丁才有。**两条语料都重录不了** |
| 敌人出招预测 | 290/349 = **83%** 完全一致 | 允许集合平均 **1.09** 手/次（集合越大这个百分比越不值钱）；**0 只对不齐/未知**；**8 例落在允许集合外**（多数是击晕那个已知洞，其余还没归因）|
| L2 自洽性 | 231 回合，**0 例实战线赢过穷尽搜索** | 7 个回合没搜完（预算 200000），那几行是启发式排名不是解 |
| 跨回合 rollout（solver 策略） | P1 163/168 · P3(a)(b) 各 0 · P4 0/17088 | 硬判据全过 |
| 跨回合 rollout（fast 策略） | P1 163/168 · P3(a)(b) 各 0 · P4 0/17088 | 硬判据全过。9-03 早些时候那条 **1/16192** 的 P4 红**已归因**：是第 1 幕瀑布巨兽（240 血）在故意做弱的 `fast` 策略下的长尾 —— 中位 17 回合、尾巴到 41~48，抬高上限数字会变（32 回合归 0）⇒ **不是挂死**。**这次采样上是 0，但 CI 基线不往下调** —— 长尾的 0 不是"修好了" |
| 跨回合 planner（D=2） | **D=2 vs D=1** 配对 17088 对，均值 **+4.9 血**，更好 7955 / 更差 868 / 打平 8265。同深度 A/B（P5，阶段 3 vs 阶段 2）是均值 **+1.1 血** | 硬判据全过 |
| 同局面换随机种子，D=2 换不换线 | **221/312 = 71% 不变** | 也就是 **29% 的回合它换个种子就改主意**（单回合求解器是 0，回合内穷尽、确定）。历史：232 → 230（9-04 `--window 0` 基线）→ 220（阶段 2）→ 221（阶段 3）|

吞吐（`cargo run --release --bin bench`）：单线程 **3.61 M steps/s**、32 线程 55.3 M。
**这台机器上 bench 波动很大，别拿它当基线**，也别拿它和别的机器比。

### 内容表

**计数不在这里** —— 只有一个家，在 [`CLAUDE.md`](CLAUDE.md) 的「内容清单」，
由 `tools/count_content.py` 重数。这份文件复制一份的话必然漂
（2026-09-05 就抓到过：两份表在两天内漂开了）。

```bash
"D:\game mod\sts2sim\.venv\Scripts\python.exe" tools/count_content.py
```

粗线条：牌和敌人覆盖了实战打得到的绝大部分，遗物**进表**接近满、**建模**还欠一批，
药水建了一半多。**「进表」不等于「建模」**，两个数差得不小。

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
* **跨回合 planner（D=2）不当驾驶员**：它平均更好（+4.9 血），但**同一个局面换个
  随机种子有 29% 会改主意**（221/312 不变）。实战一个回合只发生一次，
  要的是单次可重复性。它在实战里只作为**分歧样本**并排报出来。
* **老语料拿不到牌堆里的附魔**：2026-09-03 之前录的 trace，一张带附魔的牌只要
  还在牌堆里，内核就按卡表的数值算（方向是**低估**）。补丁之后录的没有这个洞，
  但**老语料重录不了** —— 上面「整回合预测」那 2 帧就是它。
* **L3 只建了地基。** 合成战斗构造器（`src/synth.rs`：牌组/遗物/血量/遭遇 -> 局面）
  2026-09-09 落地并被 81 条语料的第 0 帧逐字段验过，但**单场评估 / 整幕链 / MCP 接线
  三个阶段都还没做**。构筑决策目前仍由一个旧的 Python 顾问承担，它只认 35 张牌、
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

动了跨回合 planner 才要跑的第七、八条（各约 7 分钟，上面六条加起来只要几秒）：

```bash
cargo run --release --bin rollout    -- traces/act*.json --policy plan --plan-depth 2 --depth-sweep 1
cargo run --release --bin plan_audit -- traces/act*.json
```

`plan_audit` 是 planner 的审计台，独占三件既有验收全都照不到的事，
**候选覆盖**尤其重要：不在候选集里的线，叶评估和目标函数评得再准也评不到。
改 planner 之前先把被测配置扳回改之前（`--set "…"`）跑一遍当闸门。

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
2. **决定要不要抬 `PRODUCTION_MAX_TURNS`（30）。** P4 那条红已经归因完了
   （瀑布巨兽的长尾，不是挂死，见上表），剩下的是个取舍：抬到 48 能让 P4 归零、
   开销约 +0.007%，但会削弱 P4 对"真挂死"的灵敏度；不抬就得让 CI 一直挂着
   一条基线为 1 的已知红。
3. **补内容** —— 17 张牌 / 23 件遗物 / 17 瓶药水，各自卡在一个具体机制上；
   外加给 `ModifyXxx` 那一族遗物设计一张新表。
4. **补上瀑布巨兽「虹吸」那一手的回血** —— `ENEMIES` 里注释写着"自身回血 10"，
   而 `ops` 里只有 `SelfStatus{SteamEruption,3}`，**回血没实现**。方向是乐观的
   （内核低估这一场）。
5. **把权威数据表从 `traces/` 挪进 `data/`** —— 今天靠 glob 绕开，是个真疙瘩。
6. （可选）**价值网络引导搜索**。现在 GPU 闲置是对的：这个负载分支密集、
   状态极小，GPU 占用率会很差。

---

## 项目结构

```
src/     state · step · damage · ops · content      L1 规则
         solver（单回合）· plan（跨回合）· rollout   L2 求解
         replay（对拍）· json（零依赖读取器）
src/bin/ verify · solve · rollout · plan_audit · calib · bench
tools/   录制器 · 权威表导出 · 计数 · 标定 · 种子扫描（只用标准库）
traces/  act*.json 实录语料（**不可再生**）· synthetic_* · passive_*
         外加几张权威数据表（该挪走，见 docs/roadmap.md 的小疙瘩）
docs/    verified-rules（规则）· verification-log（证据）· roadmap · trace-format · relic-hook-taxonomy
```

**每个文件具体干什么、有哪些坑**，在 [`CLAUDE.md`](CLAUDE.md) 的「文件」一节 ——
那份是唯一维护的清单，这里只是张地图。

## 文档

| 文件 | 管什么 |
|---|---|
| [`CLAUDE.md`](CLAUDE.md) | 设计、不变量、接口、验收怎么跑 —— **动代码之前从头读** |
| [`docs/verified-rules.md`](docs/verified-rules.md) | **现在认定为真的规则**：伤害管线、玩家给的判定、验证器抓到过的真错误 |
| [`docs/verification-log.md`](docs/verification-log.md) | **证据**：哪一天做了什么、当时读数多少。**只有带日期的观测，没有规则** |
| [`docs/roadmap.md`](docs/roadmap.md) | 做完了什么 / 还没做 / **故意**没做 |
| [`docs/trace-format.md`](docs/trace-format.md) | trace 格式 + 从 mod 源码读出来的硬约束 |
| [`docs/relic-hook-taxonomy.md`](docs/relic-hook-taxonomy.md) | 遗物按钩子面分类：每件卡在哪个机制上。**不报进度** |

## 开源协议

[MIT](LICENSE)
