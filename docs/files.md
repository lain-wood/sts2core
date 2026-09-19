# 代码地图

每个文件干什么、有哪些坑。**这是唯一维护的一份**，README 里只有一张目录级的地图。

### `src/`

| 文件 | 内容 |
|---|---|
| `state.rs` | POD 状态、状态位 `St`、牌区、RNG、抽牌/洗牌、抽牌堆的**已知前缀** `n_draw_known`（+ 收口点 `pop_draw_top`）、卡实例 `CardInst`（8 字节：id/flags/bonus/cost_delta/**ench**/**ench_amt**）。`MAX_ENEMIES 5` / `N_STATUS 164` / `MAX_POTIONS 10` |
| `damage.rs` | **唯一**伤害管线 + 格挡 / 人工制品 |
| `asc.rs` | **生成产物，别手改**（`tools/dump_ascension.py`）。进阶数值两档：`ASC_HP`（血量区间）+ `ASC_OPS`（招式数值）+ **唯一的收口点 `adjust`**。见 [design-l1.md](design-l1.md) 的「L1：进阶」 |
| `ops.rs` | `Op` / `EOp` / `CardDef` / `EnemyDef` 定义 + `POTIONS`（表头列着故意不建的那些各自欠什么机制）|
| `content.rs` | `CARDS` / `ENEMIES` / `POWERS` / `RELICS` / **`ENCHANTS`**（附魔，含 `enchant_by_id` 和三个取值函数）/ `GEN_POOL` 生成池 / `TURN_SCOPED` / `RULE_MODIFIERS` / `HAND_END` / **`EXHAUST_END_AUTOPLAY`** / `KNOWN_UNMODELLED` / `X_COST_CARDS` / `UNPLAYABLE_CARDS`（判据是 [源码] `CardKeyword.Unplayable`，不是"有没有 ops"）|
| `step.rs` | `step` / `legal_actions` / `begin_combat` / `end_turn_with_incoming` / `fire` / `exhaust_card` / `gain_block` |
| `json.rs` | 最小 JSON 读取器（为了保持零依赖）|
| `replay.rs` | 对拍：观测同步 + 逐字段 diff。`Replayer::sync` 也被 L2 的验收复用 |
| `solver.rs` | **L2 单回合求解器**：`Threat` / `Line` / `solve_turn` / 目标函数 / 药水定价 / 局面指纹 `key`（Zobrist 风格）|
| `plan.rs` | **跨回合 planner**（限深 expectimax）：机会节点 `chance_children`（超几何枚举 / CRN 采样）· 确定性窗口 `window_is_certain` · 候选生成（`cand_score` / `k_at` + `root_is_narrow`）· 跨候选 memo（`plan::key` + 无锁 `Tt`）· `Leaf` 枚举 · `power_reserve`。**`Plan::apply` / `Plan::describe` 也在这里** —— `--alt` / `--set` / `--plan-set` 三个验收台共用一份键解析 |
| `rollout.rs` | 跨回合推演：`Policy`（`Fast` / `Solver` / `Plan`）+ `predicted_threat` + `close_pending` + `Outcome` + **`par_map`**（并行分块，`sample_outcomes` 和 `synth::eval` 共用 —— "结果和串行逐字相同"这条性质只该有一处守着）|
| `synth.rs` | **L3 阶段 1：合成战斗构造器**。`FightSpec` / `DeckCard` / `EnemySpec` / `Gap` / `build`。血量从 `asc::hp_range` 掷（**独立随机流**，不碰 `State::rng`）。`card_from_name` 是「游戏里的牌 -> 内核的牌」那一步，和 `replay::sync` 的 `push` 必须给出同一个 `CardInst` |
| `synth/eval.rs` | **L3 阶段 2：单场评估**。`EvalCfg` / `FightEval` / `Dist` / `Caveat` / `Refusal` / `evaluate` / `paired_delta`。**每个样本各 `build` 一次**（敌人血量是这场仗真实存在的一条方差）· 截断的样本不进任何分布 · 配对里死亡和血量**拆开**。见 [design-l3.md](design-l3.md) 的「L3：单场评估」 |
| `synth/from_obs.rs` | 观测第 0 帧 -> `FightSpec`：`extract` / `build_start` / `corrected_hp` / `DECK_REWRITERS`。**`synth_audit` 和 `fight_eval` 共用这一份** |
| `synth/act.rs` | **L3 阶段 3：整幕链式评估**。`Room` / `ActPlan` / `ActCfg` / `ActEval` / `evaluate_act` / `paired_act_delta`（+ 换路线那一对走 `paired_act_delta_across_routes`）/ `draw_sequence` / `rest_heal`。`ActPlan::pin_boss` 钉住局外已知的那只 Boss（钉错幕整个拒绝作答）。**路线由调用方给**（这一层不建路线）· 抽序列照 [源码] `GenerateRooms` 的分布 · **开不出来的那一场跳过并计数** · `crn_salt` 是把共享关掉的归因旋钮。见 [design-l3.md](design-l3.md) 的「L3：整幕链式评估」 |
| `synth/encounters.rs` | 遭遇表加载器：`data/encounters.json` + `data/enemy_ids.json` -> `Table`。`resolve`（遭遇 key -> 敌人，构成含随机的**整条拒绝**）· `identify`（场上这批敌人是哪一场）· `coverage`（这一幕开得出几场）|

### `src/bin/`

| | 干什么 |
|---|---|
| `verify.rs` | 对拍验证器（三种模式）|
| `solve.rs` | L2 验收（求解线 vs 实战线）+ `--live` 实战建议 + `upgrade_threat_live` |
| `rollout.rs` | 跨回合验收 P1–P5。`--policy` 选被审的策略，**要跑两遍**。`--alt "k=v,…"` 是**同深度 A/B** 通道（对照策略从主策略配置出发，只改点名的键）—— 没有它，P5 只答得了"D=2 比 D=1 好多少"。`--set "k=v,…"` 改**主策略**（键相同），单独跑一遍量耗时用它 —— 一个 `--alt` 进程里两条 arm 的耗时是混在一起的 |
| `plan_audit.rs` | **planner 的审计台**：候选覆盖 / 深层内层最优性 / 到边界后悔。三条都在**确定性起点**上量，裁判是"真敌人回合 + `score::leaf`"而不是 `threat.end_turn`。五个口径逐条评同一批线，其中 **A2 = planner 今天真的在做的事**（含确定性窗口），A→A2 就是阶段 2 那一栏。它还会**真跑一遍 `plan::plan_report`**，报候选集条数（和审计台自己搭的那份对不上就是台子错了）和置换表命中率。`--set "k=v,…"` 换被测配置（键同 `--alt`），`--inner` 那一条很慢 |
| `bestline.rs` | **整场战斗的上界**：跨回合束搜索。`--policies` 在同一批种子上并排跑单回合策略和 planner（**配对比较**），`--sweep` 扫 `Weights::clock` 的刻度，`--explain N` 把一整场逐回合印出来。**它不是策略** —— 它看得见这条随机轨迹的未来，报的是"至少能打成这样" |
| `calib.rs` | 标定样本导出器：每个回合起点导「叶局面 → 真实结局」 |
| `synth_audit.rs` | **合成战斗构造器的验收台**：拿实录的**第 0 帧**当测试集，构造器搭同一场仗，和 `Replayer::sync` 逐字段比。四栏读数各占一类失败（逐字段差 / 遭遇识别 / 血量区间 / **开局第一手**）。**不判红** —— 它报的是内容覆盖，不是正确性。三样结构上不可比的（手牌身份 / `n_draw_known` / `enemy_def`）写在模块头 |
| `fight_eval.rs` | **L3 阶段 2 的验收台**：拿实录第 0 帧当测试集，构造器搭同一场仗，**打完 64 次**报分布。**唯一的硬判据是截断率 0**（判红）。`--upgrade-check` 是单调性自检（基础牌全升，配对比）。它**故意不做回溯检验** —— 那是阶段 3 的判据，而 trace 末帧不保证是战斗结束、实测那场又是玩家打的，两个差混在一个数里什么都判不了 |
| `act_eval.rs` | **L3 阶段 3 的验收台**，两栏：**整幕链**（拿实录的真牌组走完那一幕 64 次，硬判据仍是截断率 0）和**回溯检验**（真牌组 + 真遭遇 + 真战损，看预测的战损分布包不包得住实测值，另报分位数排名直方图）。后一栏**不判红** —— 它量的是「玩家 + 我们这套策略」两个差的合量，判得了**偏的方向**，判不了"内核对不对"。`--rooms` 换路线（默认那条是 `[判断]`，印在报告最上面）· `--upgrade-check` 整幕单调性 · `--crn-off` 归因臂 |
| `advise.rs` | **L3 阶段 4：构筑顾问的生产入口**（**不是验收台，不进那九条**）。一份 JSON 进、一份人读的报告出；三个问法 `act` / `fight` / `deck` 共用一份请求，四个决策都翻译成「候选」。**主信号是 0 时另开一栏**（两边都死的链谁走得更深 / 两边都死的仗谁把敌人打得更残）。见 [design-l3.md](design-l3.md) 的「L3：MCP 接线」 |
| `bench.rs` | 吞吐基准（无外部依赖）。**这台机器读数波动很大，别拿它当基线** |

### `tools/`（全部只用标准库，不依赖 MCP）

| | 干什么 |
|---|---|
| `solve_now.py` | **实战入口**：读局面 → `record_trace.normalize()` → 单帧 `traces/_live.json` → `solve --live --plan`。**固定命令，无参数** |
| `advise_core.py` | **L3 的实战入口**：读实况 → 拼请求 → `advise.exe`。它独占两件内核办不到的事：**认这是哪一幕**（第 1 幕两个同序号，只有地图屏的 Boss id 分得开，缓存在 `traces/_advise_ctx.json`）和**数还剩几间房**。`advisor/server.py` 的八个 MCP 工具全调它 |
| `fixtures/` | 两份**手搓的实况 JSON**（不是实录）：`advise_core.py --selftest` 的测试集。一份带地图块（认幕 + 钉 Boss 走它），一份故意没有（验"认不出是哪一幕就拒绝作答"）|
| `record_trace.py` | **驱动式**录制器（它执行动作，所以动作是已知的）。`--plan` 见下 |
| `watch_trace.py` | **被动**录制器（玩家自己点，我在旁边看）。配置走 `traces/_watch.txt` |
| `reinfer_trace.py` | 反推规则改进后，拿存下来的观测把 `inferred` 的动作**重算一遍**，不用重打一场。真实动作一个都不碰 |
| `backfill_draw.py` | 从保留的原始帧把抽牌堆**内容**回填进老 trace。只增 `draw` 一个键，按指纹配对 |
| `plan_seed_sweep.py` | 量 D=2 建议的**可重复性**：同一个局面只换 `Plan::seed`，看它换不换线 |
| `calib_report.py` / `calib_power.py` | 读 `bin/calib` 的 CSV 做标定 |
| `dump_catalog.py` / `dump_relics.py` / `dump_potions.py` / `dump_bestiary.py` | 导权威表。**药水那个和另外三个不同构**：wiki 端点给不了药水，文本得从 `player.potions[]` / `rewards.items[]` 捡，只增不减 |
| `count_content.py` | **内容清单唯一的计数口径**。`--md` 直接吐上面那张表的 markdown，整块替换。**任何文档里的计数都从这里来，不要手写** |
| `enemy_report.py` | 从 trace 聚合敌人观测 → `traces/enemies_observed.json` |
| `relic_hooks.py` | 从反编译源码读每件遗物的**钩子面**（行为，不是静态字段）|
| `dump_ascension.py` | 从反编译源码导**进阶数值**（A8 敌人耐久 / A9 敌人输出）到 `data/ascension.json` + 生成 `src/asc.rs`。**按 (种类, 低进阶值) 认内核的 op**，认不准就整条跳过并报出来 |
| `dump_encounters.py` | **L3 的数据地基**：从反编译源码导「幕 → 遭遇 → 怪物」到 `data/encounters.json`，从实录导英文类名 ↔ 内核敌人的连接键到 `data/enemy_ids.json`，并印**覆盖率**（这一幕内核今天开得出几场仗）。`--md` 吐覆盖率表。**解析器敢于放弃** —— 构成含随机的遭遇标 `exact: false`，由 `data/encounters_overrides.json` 手填 |

### `advisor/`：唯一依赖 `mcp` 包的地方

| | 干什么 |
|---|---|
| `server.py` | `sts2-advisor` MCP server。八个工具名映射到 `tools/advise_core.py` -> `bin/advise` 的三个问法，**一条游戏规则都没有**。2026-09-12 从 `sts2sim/` 搬进来 —— 它是 L3 接口的消费者，接口一变它就得跟着变，放在另一个目录时没有任何东西把这两件事绑在一起 |
| `../pyproject.toml` | 只为 `uv run` 存在（`mcp` 这一个依赖）。`packages = ["advisor", "tools"]` 让 `from tools import advise_core` 成立 —— **普通 import，不是 `sys.path` 注入** |
| `../uv.lock` · `../.venv/` | 这个仓库自己的 Python 环境。**`.venv` 是所有 Python 工具的解释器**（PATH 上那个 `python` 是 Store 假壳）|

> **依赖的分界线**：`mcp` 只出现在本目录。**`tools/` 那十几个脚本一律只用标准库**
> —— 那条规矩保证录制器和实战入口不依赖 MCP，游戏跑着、MCP server 也跑着的时候
> 它们能独立工作，不跟它抢连接。

### `data/`：不是 trace 的那些表

| 文件 | 是什么 | 谁生成 |
|---|---|---|
| `encounters.json` | [源码] 幕 → 遭遇 → 怪物 + 房间构成 + 怪物血量区间 | `dump_encounters.py` |
| `enemy_ids.json` | [实录] 英文 `MonsterModel` 类名 ↔ 内核敌人 | 同上 |
| `encounters_overrides.json` | **手写**：解析器放弃掉的那几场，每条写着为什么确定 | 人 |
| `ascension.json` | [源码] 进阶数值全表，含 `unmatched` 那份欠账 | `dump_ascension.py` |
| `ascension_overrides.json` | **手写**：生成器认不出的那些填这里（今天是空的） | 人 |
| `enemies_observed_legacy.json` | **[实测] 敌人最大血量**，2026-08-14 那一局边打边记的（旧 advisor 的 `bestiary.json`，2026-09-12 随它退役搬进来）。**35 只里 22 只对拍语料里没有** —— 语料 08-16 才开始，那一局打到第 3 幕却一帧没录。用处：对 `asc::hp_range` 那张 [源码] 档的表、补敌人内容时有个实测起点。四条注意事项（进阶不明 / 幕和种类是旧口径 / 名字带局内后缀 / 伤害是意图标签）写在文件头 | 人（迁移）|

**生成的那三份连同 `src/asc.rs` 一起跟踪，尽管可重跑** ——
它们的输入是 `decompiled/`，而那个目录是 `.gitignore` 掉的：
不跟踪产物的话，clone 下来的仓库重跑不出这几份表，而 `src/asc.rs` 还要参与编译。

**这个目录是 roadmap 那条「把权威表挪进 `data/`」的第一块** ——
既有的 `traces/*_catalog.json` 还没搬，搬它要动 `src/lib.rs` 的路径断言和四个 dump 工具。

### `traces/` 和 `decompiled/`

`act*.json` / `synthetic_*.json` 是对拍语料；`passive_*.json` 不进默认 glob；
`*_catalog.json` / `enemies_*.json` 是**权威数据表、不是 trace**（混在一个目录里，
见 roadmap 的「小疙瘩」）；`_plan.txt` / `_watch.txt` 是两个录制器的配置文件；
`_live.json` 是生成物。

`decompiled/` 是**生成物**：`sts2.dll` 反编译出的 3425 个 `.cs`，
就是全文里 `[源码]` 这一档的来源。可再生，随手可删。
