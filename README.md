# sts2core

《杀戮尖塔 2》的**纯函数规则内核（L1）+ 局内求解器（L2）+ 构筑顾问（L3）**，零依赖 Rust。
L3 通过一个 MCP server（`advisor/`）接到实战驱动流程里。

适配游戏 **v0.107.1**。所有实录语料都是在这个版本上打出来的。

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
![game](https://img.shields.io/badge/STS2-v0.107.1-8a2be2)
![deps](https://img.shields.io/badge/dependencies-0-brightgreen)

---

## 这是什么

```
L1  step(state, action) -> state     纯函数内核，唯一持有游戏规则
L2  局内求解器                        调用 L1，搜「这回合/这场仗怎么打」
L3  构筑顾问                          调用 L2，搜「这副牌组走完这一幕会不会死」
```

`State` 是 **POD + `Copy`**（3880 字节，4 KB 封顶），快照和回滚就是一条 `memcpy`。
卡牌 / 敌人 / 能力状态**全是数据表**，加一张牌应该是加一行表而不是加一个 `if`。

规则不是照着 wiki 抄的，是**被真实对局逐帧判过的**：语料由一套 MCP 驱动的录制器
从实际游戏里录下来（每一帧带观测 + 动作），验证器把内核重放的结果和观测逐字段对。
`docs/verification-log.md` 里每一条规则都写着它被哪一帧钉死。

> **一个自信地算错的模拟器，比没有模拟器更危险。** 这份代码的大部分复杂度
> 花在「怎么知道自己错了」上，不是在搜索速度上。

L3 回答「**这副牌组走完这一幕会不会死**」：同一副牌组、同一条路线、同一批随机，
基准和每个候选（拿这张牌 / 删那张 / 升级哪张 / 走不走精英）各走 256 条整幕链，
配对比较死亡率。整幕的遭遇按游戏源码里的分布抽。

---

## 现在真实的样子

**读数只维护一份**：[`docs/acceptance.md`](docs/acceptance.md) 的「当前基线」，
跑一遍就能复现，红的也列在那里。这个仓库栽过"抄早了一版数字"，所以这里不再抄一份。

粗线条（2026-09-25）：79 条实录语料；第 1、2 幕的遭遇内核全开得出，第 3 幕差女王；
`cargo test` 和 L2 / L3 的硬门全绿；两条已知红是附魔补丁之前录的老 trace，重录不了。

2026-09-25 修完一轮 L2 审计，五条都是先用反例实测成立再修的：去重指纹漏了扯碎读的计数、
机会节点不区分附魔、抽牌张数写死 5 张、线被截断仍报「穷尽」、没做完的选牌被当成终点。
后两条直接关系到 `solve --live` 那一列 `complete` 能不能信。经过和读数变化见
[`docs/verification-log.md`](docs/verification-log.md) 的「2026-09-20 L2 审计」和「2026-09-25 L2 审计第 5 条」。

内容计数只有一个家，[`docs/content.md`](docs/content.md)，由 `tools/count_content.py --md` 重数。

---

## 已知的洞

**这一节存在的理由**：全绿只说明语料照得到的地方没有分歧。

* **L3 的绝对水平不可信，只有成对的差值可信。** 回溯检验量到推演每场比玩家多掉约
  6.6 血；推演一瓶药水都不喝；引擎牌（每回合给格挡的 / 触发式的）在跨回合搜索里评不到，
  带引擎的构筑因此被低估。报告末尾会把这几条从牌组自己算出来。
* **覆盖率不是 100%**：抽到内核开不出的那一场就跳过并计数，死亡率因此偏乐观。
* **新补的敌人大多是 `[源码]` 档**，一场实录都没有。A8 以上的进阶数值整张表是 `[源码]` 档。
* **缺的牌 / 遗物各自卡在一个还没有的机制上**，不是忘了填。逐条见 `docs/content.md`。
* **`sync` 不还原 `Pending`**：游戏开着选牌界面时 `solve --live` 拒绝作答。止血，不是修好。
* **抽牌只有当前这一堆是确定的**：抽牌堆抽空之后那次洗牌不可知（游戏没暴露 RNG 状态）。
* **跨回合 planner 的机会节点只枚举开局发牌**：回合开始的真抽牌（摆动球）和敌人回合里的抽牌
  （百年积木）发生在它之前，牌序未知时取的是内核自己那次洗牌 —— 一个样本，不是分布。
* **跨回合 planner（D=2）不当驾驶员**：它平均更好（+4.2 血/场），但同一个局面换个
  随机种子约 29% 会改主意。实战一个回合只发生一次，要的是单次可重复性。

---

## 快速上手

```bash
cargo build --release
cargo test --release
```

全套验收（**glob 要靠 shell 展开**，这些是 bash 命令；PowerShell 不给原生 exe 展开 glob，
会报 `os error 123`）：

```bash
cargo run --release --bin verify  -- traces/act*.json traces/synthetic_*.json
cargo run --release --bin verify  -- traces/act*.json traces/synthetic_*.json --per-turn
cargo run --release --bin verify  -- traces/act*.json traces/synthetic_*.json --predict-enemy
cargo run --release --bin solve   -- traces/act*.json
cargo run --release --bin rollout -- traces/act*.json
cargo run --release --bin rollout -- traces/act*.json --policy fast
cargo run --release --bin synth_audit -- traces/act*.json traces/synthetic_*.json
cargo run --release --bin fight_eval  -- traces/act*.json traces/synthetic_*.json
cargo run --release --bin act_eval    -- traces/act*.json
```

每一条都有**独占的检验面**，这是这套验收的全部价值，逐条见 [`docs/acceptance.md`](docs/acceptance.md)。
CI（`.github/workflows/ci.yml`）每次推 `main`、每个 PR 都跑一遍。

MCP server（需要 [uv](https://docs.astral.sh/uv/)）：

```bash
uv run --directory <本仓库> python -m advisor.server
```

它读游戏状态走 [STS2MCP](https://github.com/Gennadiyev/STS2MCP) mod 的 HTTP 接口；
那个 mod 需要打本仓库 `patches/` 里的补丁，见 [`docs/sts2mcp-patches.md`](docs/sts2mcp-patches.md)。

---

## 项目结构

```
src/        state · step · damage · ops · content · asc     L1 规则
            solver（单回合）· plan（跨回合）· rollout       L2 求解
            synth · synth/{eval,act,encounters,from_obs}    L3 构筑顾问
            replay（对拍）· json（零依赖读取器）
src/bin/    验收台 verify · solve · rollout · plan_audit · synth_audit · fight_eval · act_eval · bestline
            生产入口 advise · 工具 bench · calib
advisor/    sts2-advisor MCP server（唯一依赖 mcp 包的地方）
tools/      录制器 · 实战入口 · 权威表导出 · 计数（只用标准库）
data/       读源码得来的权威表（遭遇分布 · 敌人连接键 · 进阶数值 · 遗物 id）
traces/     act*.json 实录语料（**不可再生**）· synthetic_* · passive_* · 问游戏得来的权威表
patches/    上游 STS2MCP 的本地补丁
docs/       见下
```

逐文件说明在 [`docs/files.md`](docs/files.md)。

## 文档

| 文件 | 管什么 |
|---|---|
| [`CLAUDE.md`](CLAUDE.md) | 分层、不变量、改内核的流程、验收命令：**动代码之前从头读** |
| [`docs/design-l1.md`](docs/design-l1.md) · [`l2`](docs/design-l2.md) · [`l3`](docs/design-l3.md) | 每一层为什么是这个形状 |
| [`docs/acceptance.md`](docs/acceptance.md) | 验收：每条判什么、独占什么、**当前基线** |
| [`docs/verified-rules.md`](docs/verified-rules.md) | **现在认定为真的规则**：伤害管线、玩家给的判定、验证器抓到过的真错误 |
| [`docs/verification-log.md`](docs/verification-log.md) | **证据**：哪一天做了什么、当时读数多少。**只有带日期的观测，没有规则** |
| [`docs/content.md`](docs/content.md) | 内容清单 + 缺的那些卡在哪 |
| [`docs/roadmap.md`](docs/roadmap.md) | 做完了什么 / 还没做 / **故意**没做 |
| [`docs/driving.md`](docs/driving.md) · [`docs/enemies.md`](docs/enemies.md) | 实战：工具输出怎么读 · 逐只敌人的打法和求解器盲点 |
| [`docs/trace-format.md`](docs/trace-format.md) | trace 格式 + 从 mod 源码读出来的硬约束 |
| [`docs/relic-hook-taxonomy.md`](docs/relic-hook-taxonomy.md) | 遗物按钩子面分类：每件卡在哪个机制上 |
| [`docs/sts2mcp-patches.md`](docs/sts2mcp-patches.md) | 上游 mod 的四个本地补丁 |

## 开源协议

[MIT](LICENSE)
