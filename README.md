# sts2core — 《杀戮尖塔 2》高性能模拟内核与跨回合 AI 求解器

零依赖、极致性能的 **Slay the Spire 2** 纯函数 Rust 规则模拟内核与局内 Expectimax 求解器。

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.80%2B-orange.svg)](https://www.rust-lang.org/)

---

## 🌟 核心特性

- **纯函数内核（L1）**：
  - 核心状态 `State` 满足 `POD + Copy`（3104 字节），状态快照与回滚开销仅为一条 `memcpy`（0 纳秒）；
  - 统一、严格对拍的伤害与乘区管线（`damage.rs`）；
  - 数据驱动：卡牌、敌人、能力状态全表驱动（覆盖 97/106 卡牌、86 件遗物与敌人出招状态机）。
- **极速求解器（L2）**：
  - **单回合搜索（`solve_turn`）**：毫秒级穷举最优出牌序列与目标，支持存活优先 / 竞速等多目标函数；
  - **跨回合 Planner（`plan_line`）**：D=2 限深 Expectimax 跨回合推演，配合已知抽牌前缀、机会节点塌缩与 S3 斩杀延伸；
  - **高吞吐推演**：单核推演吞吐高达 **4.87 M steps/s**。
- **实战验证（Ground Truth 对拍）**：
  - 内置 53 场真实游戏实录战斗 Trace（含 A1 第 2 幕 Boss 碾碎爪 408 血击破）；
  - 经过 **一步预测、整回合预测、敌人 AI 对拍** 等全量回归测试（**0 MISMATCH**）。

---

## 🚀 快速上手

### 1. 编译
```bash
cargo build --release
```

### 2. 运行回归验证套件
```bash
# 验证所有实战 Trace 的每一步规则一致性
cargo run --release --bin verify -- traces/act*.json traces/synthetic_*.json

# 跨回合推演验收
cargo run --release --bin rollout -- traces/act2_f33_boss_crusher_final.json
```

### 3. 运行性能基准测试
```bash
cargo run --release --bin bench
```

---

## 📂 项目结构

```
sts2core/
├── src/
│   ├── state.rs      # POD 状态定义与 3 条分流随机数（splitmix64）
│   ├── step.rs       # 纯函数转移 step(state, action) -> state
│   ├── damage.rs     # 统一伤害与乘区计算管线
│   ├── content.rs    # 数据驱动内容表（CardDef / EnemyDef / PowerDef）
│   ├── solver.rs     # L2 单回合搜索与 Top-K 候选剪枝
│   ├── plan.rs       # L2 跨回合 Expectimax 求解器
│   └── rollout.rs    # 跨回合推演系统（Policy::Fast / Solver / Plan）
├── src/bin/
│   ├── solve.rs      # 实战局内出牌建议
│   ├── verify.rs     # 逐帧与整回合对拍验证器
│   └── rollout.rs    # 跨回合 Monte-Carlo 验收
├── docs/             # 详尽的规则对拍证据链与审计报告
└── traces/           # 53 条不可再生的真实对拍语料库
```

---

## 📜 开源协议

本项目采用 [MIT License](LICENSE) 开源协议。
