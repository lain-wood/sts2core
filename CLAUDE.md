# sts2core：《杀戮尖塔 2》模拟内核

零依赖的 Rust 内核：L1 规则 + L2 求解器 + L3 构筑顾问，外加 `sts2-advisor` MCP server
（`advisor/`）和所有 Python 工具的解释器（`.venv/`）。
实战驱动流程在工作台的 [`../CLAUDE.md`](../CLAUDE.md)，**先读那份**。

## 文档地图：动哪块读哪份

| 文档 | 管什么 | 什么时候读 |
|---|---|---|
| **本文件** | 分层、不变量、改内核的流程、验收命令、环境 | 动代码之前，**从头读到尾** |
| [`docs/design-l1.md`](docs/design-l1.md) | 伤害管线 · 触发器（26 个钩子）· 三个类别 · 敌人侧 · 进阶 | 动 `damage.rs` / `step.rs` / `content.rs` / `asc.rs` |
| [`docs/design-l2.md`](docs/design-l2.md) | 单回合求解器 · 跨回合搜索（rollout / planner） | 动 `solver.rs` / `rollout.rs` / `plan.rs` |
| [`docs/design-l3.md`](docs/design-l3.md) | 构造器 · 单场评估 · 整幕链 · MCP 接线 | 动 `synth*` / `bin/advise.rs` / `advisor/` |
| [`docs/acceptance.md`](docs/acceptance.md) | 每条验收跑什么、判什么、独占哪个检验面 · **当前基线** | 跑验收、读验收输出 |
| [`docs/verified-rules.md`](docs/verified-rules.md) | **现在认定为真的规则**：伤害管线、玩家给的判定、验证器抓到过的真错误 | 改一个数字、加一条规则**之前** |
| [`docs/verification-log.md`](docs/verification-log.md) | **证据**：哪天做了什么、当时读数多少、先做错了什么。**只有带日期的观测** | 查一条规则是被哪一帧钉死的（拿日期回来搜） |
| [`docs/content.md`](docs/content.md) | 内容清单（计数的唯一出处）+ 缺的牌 / 遗物各自卡在哪 | 补内容之前 |
| [`docs/files.md`](docs/files.md) | 代码地图：每个文件干什么、有哪些坑 | 找东西 |
| [`docs/roadmap.md`](docs/roadmap.md) | 做完了什么 / 还没做 / **故意**没做 | 决定下一步 |
| [`docs/trace-format.md`](docs/trace-format.md) | trace 格式 + 从 mod 源码读出来的硬约束 | 动录制器或验证器 |
| [`docs/relic-hook-taxonomy.md`](docs/relic-hook-taxonomy.md) | 遗物按钩子面分类 | 建模遗物之前 |
| [`docs/driving.md`](docs/driving.md) · [`docs/enemies.md`](docs/enemies.md) | 实战：工具怎么读 · 逐只敌人的打法和求解器盲点 | 打游戏的时候 |
| [`docs/sts2mcp-patches.md`](docs/sts2mcp-patches.md) | 上游 mod 的四个本地补丁（正本在 `patches/`） | 升级 STS2MCP |

**三条文档纪律**（都是吃过亏立的）：

* **本文件和 `design-*` 只写现在的样子和为什么。** 「哪天做了什么、读数多少」一律进
  verification-log。
* **verification-log 不许写规则。** 任何「X 是 Y」的陈述句都属于 verified-rules：
  规则摘要和证据混在一个文件里存了半个月，摘要里的「缩小 ×2/3」早在 08-21 就被
  同一个文件里的另一条推翻了，而没有任何东西因此变红。
* **会变的数不手抄。** 计数跑 `tools/count_content.py --md`，覆盖率跑
  `tools/dump_encounters.py --md`，读数看 `acceptance.md` 的「当前基线」(整块覆盖，不叠层)。

---

## 为什么有这个内核

它替换掉的旧 Python 模拟器（`sts2sim/`，2026-09-12 删除）状态是对象图、效果是硬编码：
只认 35 张牌、预算一到就退化成贪心、敌人模型表达不了复活 / 召唤 / 无实体。
2026-08-14 那局死在第 3 幕 Boss 实验体，`inspect_fight` 报 `traits=[none]`，
而它有三阶段、隔回合无实体、激怒。搜索再快也没用，搜的是一个不存在的游戏。

> **一个自信地算错的模拟器，比没有模拟器更危险。**
> 这份代码的大部分复杂度花在「怎么知道自己错了」上，不是在搜索速度上。

---

## 分层

```
L1  step(state, action) -> state     纯函数内核，唯一持有游戏规则
L2  局内求解器                        调用 L1，搜"这回合/这场仗怎么打"
L3  构筑顾问                          调用 L2，搜"拿哪张牌/移除/升级/路线"
```

**L2 和 L3 不许自己实现任何游戏规则**，只能经由 L1 的**五个**入口：

```rust
pub fn step(s: State, a: Action) -> State;
pub fn legal_actions(s: &State) -> ([Action; MAX_ACTIONS /* 128 */], usize);
pub fn begin_combat(s: State) -> State;
pub fn end_turn_with_incoming(s: State, inc: &Incoming) -> State;
pub fn grant_relic(s: &mut State, def: &RelicDef, counter: Option<i32>);  // L3 开仗用
```

第四个是给 L2 的：结束回合，但**敌人这一手打多少由调用方给定**。没有它，L2 就得自己写
"格挡怎么吃这几下伤害"，那就是在 L2 里复制规则。它和 `enemy_turn` 共用
`take_attack_hit`，所以格挡逐次吸收、孤注一掷、火焰屏障、`hp_lost_this_turn` 记账
全是同一条路，**不可能长歪**。

第五个是给 L3 的：把一件遗物的战斗层状态交给玩家实体。**「哪件遗物给哪几个 status」
是游戏规则，所以它在 L1**。它必须在 `begin_combat` **之前**调用：好几件遗物的效果挂在
第 1 回合的 `TurnStart` 上（赤牛的活力、锚的格挡）。

`advisor/server.py` 是 L3 接口的**消费者**，所以和接口住在同一个仓库：八个 MCP 工具名
映射到 `tools/advise_core.py` -> `bin/advise`，**一条游戏规则都没有**。

---

## 不变量

改代码前先确认没有破坏这些。它们是性能和正确性的地基：

1. **`State` 保持 POD + `Copy`。** 不许出现 `Vec` / `HashMap` / `Box` / 引用，
   clone 必须是一条 memcpy。当前 **3880 字节**，`state_is_a_value` 守着 4 KB 上限。
   `N_STATUS` = 176，用了 171 格：**再扩 16 格就顶破 4 KB**（每格 12 字节，还剩约 216 字节），
   下一次要加之前先清掉只映射不建模的那几格。**每次扩容都要拿 `bin/bench` 连跑三次量代价**，
   读数记进 verification-log。
   `CardInst` 是 8 字节 × 128 张 = 1024 字节，占三分之一；`flags` 是 `u8`，
   **还剩 2 个位**。`solver.rs` 里一条 `size_of::<CardInst>() == 8` 的编译期断言守着
   （2026-09-06 直接加字段那一次，`State` 当场涨、单线程掉约 7%）。
2. **所有伤害走 `damage.rs`。** 不许在别处零散写 `hp -= x`。乘区顺序只在那一处定义。
3. **卡牌 / 敌人 / 能力是数据不是代码**（`content.rs` 的 `CardDef`/`EnemyDef`/`PowerDef`）。
   加一张牌应该是加一行表，不是加一个 `if`。加不进去就说明 `Op` 枚举缺原语：
   **扩 `Op`，不要在 `step.rs` 里特判卡名。**
4. **RNG 在状态里**（`state.rs` 的 `Rng`，splitmix64，**shuffle / enemy / gen 三条分流**）。
   分流不是洁癖：合流的话「多打一张添柴」会改掉后面所有抽牌。
   **内核的随机序列故意不和游戏一致，也做不到**，理由见 verification-log 的「随机数」一节。
   同种子 + 同动作序列 ⇒ 状态逐字节相同（`deterministic_given_seed` 守着），
   这是配对随机数比较（CRN）和回放验证的前提。
5. **`step` 是全函数**：非法动作返回原状态，不 panic、不报错。
6. **子选择用 `Pending` 建模**，不要让 `step` 变成可重入的。
   `Pending != None` 时 `legal_actions` 只返回 `Action::Choose`。

---

## 改内核的流程

1. **读那一层的 `design-*.md`。** 要改一个数字或加一条规则，再读 verified-rules
   （那条规则被哪一帧钉死、玩家给过什么判定）。
2. **卡面文本对触发类交互是欠定的。** 遇到「每当…时」/ 多目标 / 牌堆顺序这类判定，
   **先问用户，不要挑一组自洽的解写进去**：他一次纠正过三个凭卡面推的错结论。
   已有的判定表在 [verified-rules 第 2 节](docs/verified-rules.md#2-玩家判定卡面读不出来的那些)。
3. **真实游戏是唯一权威，欠定就留空。** 每条内容标来源档次：`[实测]` / `[源码]` /
   `[wiki]` / `[推断]`。wiki 和卡面都只是假设（旧日雕像的伤害 wiki 说 15，实测 13）。
4. **"和所有已知数据一致"不等于对。** 乘区取整那个 bug 藏了很久，因为之前所有样本
   两种算法结果都一样。要主动去找**能分开假设**的样本；改完做一次**反向突变**
   （只撤这一条、重跑），确认真有东西守着它。
5. **跑全套验收**（下一节），别信文档里的绿灯：录完 trace 当场跑。
6. **落账**：规则进 verified-rules，证据和读数进 verification-log 的当天条目，
   `acceptance.md` 的「当前基线」整块换新，计数重跑 `count_content.py --md`。

---

## 验收

改完内核**每次都跑全套**（bash；PowerShell 不给原生 exe 展开 glob，会报 `os error 123`）：

```bash
cargo test --release
cargo run --release --bin verify  -- traces/act*.json traces/synthetic_*.json
cargo run --release --bin verify  -- traces/act*.json traces/synthetic_*.json --per-turn
cargo run --release --bin verify  -- traces/act*.json traces/synthetic_*.json --predict-enemy
cargo run --release --bin solve   -- traces/act*.json
cargo run --release --bin rollout -- traces/act*.json
cargo run --release --bin rollout -- traces/act*.json --policy fast
cargo run --release --bin synth_audit -- traces/act*.json traces/synthetic_*.json
cargo run --release --bin fight_eval  -- traces/act*.json traces/synthetic_*.json
cargo run --release --bin act_eval    -- traces/act*.json
.venv/Scripts/python.exe tools/advise_core.py --selftest
```

| 判据 | 哪几条 |
|---|---|
| **硬门**（红了就是回归） | `cargo test` · `solve` 的「0 例实战线赢过穷尽搜索」· `rollout` 的 P3(a)(b) = 0 和 P4 · `fight_eval` / `act_eval` 的**截断率 0** · advisor 自检 |
| **已知红，只守不许变多** | `verify` 一步 1 帧 · `--per-turn` 2 帧（同一件事：附魔补丁之前的老 trace，重录不了）· `rollout fast` 截断基线 1 |
| **不判红，但要读** | `--predict-enemy`（看集合外 / 未知，不看百分比）· `synth_audit`（内容覆盖）· 回溯检验（偏的方向） |

动了 planner 还要跑另外三条（`--depth-sweep` / `--alt` / `plan_audit` 闸门）。
每条独占什么检验面、怎么读那些数、当前基线：[`docs/acceptance.md`](docs/acceptance.md)。
**别写成 `traces/*.json`**：那个目录里还放着权威数据表，验证器会在它们身上停下。

---

## 数据的档次

| 档次 | 是什么 | 能不能重做 |
|---|---|---|
| **实录语料** | `traces/act*.json`，真实战斗的证词；`passive_*.json` 是玩家驾驶、动作反推的，低一档，不进默认 glob | **不可再生**，只能重新打一场 |
| **权威表（问游戏）** | `traces/*_catalog.json`、`enemies_wiki.json`，从游戏的 `GET /api/v1/wiki` 或 sts2.wiki 导出 | dump 工具重新生成。它是「**这个存档发现了多少**」的快照，会随着打得多而涨 |
| **权威表（读源码）** | `data/`：遭遇分布 · 英文类名 ↔ 内核敌人 · 进阶数值（连同生成的 `src/asc.rs`）；`*_overrides.json` 是手写的 | dump 工具重新生成，**但输入是 `decompiled/`**（gitignore 掉的），游戏一更新就要重新反编译再重跑，所以产物照样跟踪 |
| **生成物** | `target/`、`__pycache__/`、`traces/_live.json`、`--emit-missing` 的输出 | 随手可删 |

`data/enemies_observed_legacy.json` 是 2026-08-14 那一局边打边记的**实测敌人血量**
（语料 08-16 才开始，35 只里 22 只语料里没有），**不可再生**。

---

## 实战前必须知道的洞

1. **`sync` 不还原 `Pending`。** 游戏开着选牌界面时，`solve --live` **直接拒绝作答**。
   这是止血不是修好：真修要把选牌界面的候选和剩余张数一起同步进来。
2. **抽牌只有当前这一堆是确定的。** `draw_pile_order` 补丁之后 `sync` 整堆照抄；
   抽牌堆抽空之后那次洗牌的结果不可知（游戏的 `Rng.Shuffle` 状态没暴露）。
   老 trace 没这个字段，照旧自己洗一个样本。`Line::drew` 把这件事标出来。
3. **预算用光（或者线长顶到 `MAX_LINE` 24 步）就退化成启发式排名**，`Solved::complete` 报出来。手牌一大，
   "穷尽搜索"这个前提就开始失效。**读 `--live` 时看 `complete` 那一列。**
4. **从战斗中途接入时**（实战驱动永远是中途），「一场只用得掉一次」的遗物一律当成已经用掉
   （`content::spent_once_per_combat`）：前面发生过什么不可知，宁可低估自己。

`--live` 会把手里它不认识的牌、身上它看不见的遗物（连同卡在哪）都点名列出来。
**那几行不是装饰。**

---

## 录制

两个录制器，各一条固定命令（命令字符串一变就要重新过一次权限确认）：

```bash
# 驱动式：我出牌，动作写进 traces/_plan.txt
& "D:\game mod\sts2core\.venv\Scripts\python.exe" "D:\game mod\sts2core\tools\record_trace.py" --plan
# 被动式：玩家出牌，动作是反推的，配置走 traces/_watch.txt
& "D:\game mod\sts2core\.venv\Scripts\python.exe" "D:\game mod\sts2core\tools\watch_trace.py"
```

计划文件语法、判成败的口径、为什么录制时不能混用 `combat_play_card`：
[`docs/driving.md`](docs/driving.md) 的「录制」。trace 格式：[`docs/trace-format.md`](docs/trace-format.md)。

---

## 目录

| 目录 | 是什么 |
|---|---|
| `src/` | L1（`state` `step` `damage` `ops` `content` `asc`）· L2（`solver` `plan` `rollout`）· L3（`synth` + `synth/`）· 对拍（`replay` `json`） |
| `src/bin/` | 验收台（`verify` `solve` `rollout` `plan_audit` `synth_audit` `fight_eval` `act_eval` `bestline`）+ 生产入口 `advise` + `bench` `calib` |
| `tools/` | 录制器 · 实战入口 `solve_now.py` / `advise_core.py` · dump 工具 · 计数。**只用标准库**，不依赖 MCP |
| `advisor/` | `sts2-advisor` MCP server，**唯一依赖 `mcp` 包的地方** |
| `data/` | 读源码得来的权威表（见「数据的档次」） |
| `traces/` | 实录语料 + 问游戏得来的权威表 + 两个录制器的配置文件 |
| `patches/` | 上游 STS2MCP 的本地补丁正本 |
| `decompiled/` | `sts2.dll` 反编译出的 `.cs`，`[源码]` 档的来源。**gitignore，可再生** |

逐文件说明：[`docs/files.md`](docs/files.md)。

---

## 构建与环境

```bash
cd "D:\game mod\sts2core"
cargo test --release --lib
cargo run --release --bin bench
```

工具链：rustc 1.97.1，**stable-x86_64-pc-windows-gnu**（这台机器没有 MSVC，GNU 工具链
自带链接器）。cargo 在 `C:\Users\admin\.cargo\bin`。因为选了 GNU，Python 侧**走独立可执行
文件 + JSON/stdio IPC**，不走 PyO3：边界调用很少，IPC 开销可以忽略，还绕开了 CPython 的
MSVC ABI 问题。

Python 环境由 `uv` 按 `pyproject.toml` 建（`.venv/`，gitignore 掉）：

```bash
uv run --directory "D:\game mod\sts2core" python -m advisor.server   # MCP 那边就是这么起的
"D:\game mod\sts2core\.venv\Scripts\python.exe" tools/advise_core.py --selftest
```

这个环境的 shell 坑：

* 默认 shell 是 **Windows PowerShell 5.1**：**没有 `&&`**，用 `;` 或 `; if ($?) { ... }`
* **PowerShell 不给原生 exe 展开 glob**：验收命令用 bash 跑，或者写成
  `$f = (Get-ChildItem traces\act*.json).FullName; .\target\release\verify.exe $f`
* **不要对原生 exe 用 `2>&1`**：5.1 会把 stderr 每行包成 ErrorRecord，
  即使 cargo 返回 0 也会冒出 `NativeCommandError`
* 路径含空格（`D:\game mod`），记得加引号
* PATH 里的 `python` 是 Microsoft Store 的假壳，**真的解释器在 `.venv/`**
