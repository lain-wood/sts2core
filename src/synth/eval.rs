//! **L3 阶段 2：单场评估** —— `(牌组, 遗物, 血量, 遭遇) -> (死亡率, 血量分布, 截断率)`。
//!
//! 阶段 1 让内核**搭得出**一场还没打的仗（[`crate::synth::build`]）。
//! 这一层把那场仗**打完 N 次**，报出来的不是一个分数，是一份分布。
//!
//! ```text
//! FightSpec ─► 每个样本各 build 一次（换种子）─► rollout 到底 ─► Outcome
//!                                                                 │
//!                        死亡率 · 终点血量分布 · 战损分布 · 截断率 ◄┘
//! ```
//!
//! # 判什么：**死亡率在前，血量在后**
//!
//! 这是父目录 `CLAUDE.md` 里那条老账在 L3 侧的收口：旧 advisor 的 ΔHP
//! **在死亡处被截断**，于是一场两边都会死的仗里每个候选的 ΔHP 都塌向 0 ——
//! 排名恰好在最要紧的时候失声。所以这里：
//!
//! * 死亡是**一个单独的计数**，不是"掉了很多血"的极端值；
//! * 血量分布的分母**只有打完并活下来的样本**（[`FightEval::finished_alive`]），
//!   它答的是"活下来的话大概剩多少"，**不许拿它去代表这场仗的好坏**；
//! * 两个数一起读才有意义，所以 [`FightEval::headline`] 把它们印在一行里。
//!
//! # 截断是**硬判据**，不是一栏诊断
//!
//! 撞上回合上限的推演**战斗没有分出胜负**，而 [`Outcome::final_hp`] 照样是
//! 当时的血量 —— 那是一个凭空的存活数字（`bin/rollout` 的 P4 量的正是这个）。
//! 所以：
//!
//! * 截断的样本**既不进死亡数、也不进血量分布**，单独计数；
//! * `bin/fight_eval` 在截断 > 0 时**退非零码**。这是本仓库唯一一个
//!   由 L3 台子把守的硬门 —— 别用抬高上限去关掉它：
//!   [`Outcome::enemy_hp_left`] 就是用来分辨「差一口气」和「僵住了」的，
//!   而僵住通常意味着**策略**选错了目标函数（用单回合的 `survive_first`
//!   当跨回合策略实测 52/8320 条只挡不打，见 `rollout::Policy::Solver`）。
//!
//! # 配对随机数（CRN）：候选之间必须共用同一批样本
//!
//! L3 问的从来不是"这副牌组多好"，而是"**加这张牌**比不加好多少"。
//! 那个差要在同一批随机上量，不然噪声比效应大。这里的做法是
//! [`sample_seed`]：样本 `i` 的种子只由 `(spec.seed, i)` 决定，
//! **和牌组无关** —— 于是两个候选在样本 `i` 上：
//!
//! | 共用 | 为什么共用得了 |
//! |---|---|
//! | 敌人血量的掷点 | `synth::hp_stream` 只吃 `spec.seed`，牌组进不去；**这一条是逐字相同的** |
//! | 三条随机流的**起点** | `Rng::new` 从同一个种子派生，起点相同 |
//!
//! 守着这条的是 `two_decks_see_the_same_enemy_hp_on_the_same_sample`。
//!
//! > **共享的是起点，不是序列 —— 别把它当成方差归零。** 牌组一换，
//! > 洗牌流和敌人流**消耗的次数**当场就不一样（仗长了一个回合，敌人就多掷一次），
//! > 两条流从那一刻起各走各的。所以配对之后**逐场**仍然会有噪声，
//! > 真正逐字共享的只有「这一场敌人有多少血」——
//! > 而那恰好是单场评估里最大的一块方差（`asc::hp_range` 的区间宽到 ±10%）。
//! > 判据因此要看**一批仗上的方向**，不是某一场的正负。
//!
//! # 三条**已经知道会算偏**的，全部随结果印出来
//!
//! 路线图那五条里，属于单场评估这一层的是这三条。它们不是免责声明，
//! 是**方向已知**的偏差，读结论时要带着：
//!
//! | 偏 | 方向 | 出处 |
//! |---|---|---|
//! | 引擎牌（每回合给格挡的 / 会衰减的 / 触发式的）在跨回合搜索里评不到 | **低估**带引擎的牌组 | `CLAUDE.md` 的「跨回合搜索」 |
//! | 推演**一瓶药水都不喝**（`Policy::Solver` 的 `allowed_potions = 0`）| **低估**（身上有药水时）| `rollout::Policy` |
//! | 构造器自己报的缺口（不认识的牌 / 遗物 / 没有血量区间…）| 逐条不同，[`Gap`] 里写着 | `synth::Gap` |
//!
//! 前两条由 [`FightEval::caveats`] **从这副牌组自己算出来**（有几张能力牌、
//! 身上几瓶药水），不是一段写死的免责文本 —— 没有能力牌的牌组不该看见
//! 那一行。

use crate::content::card;
use crate::ops::Kind;
use crate::rollout::{can_rollout, par_map, rollout_outcome_with, Outcome, Policy};
use crate::state::{next_u64, State};
use crate::synth::{build, FightSpec, Gap};

/// 回合上限。**它是判据的一部分，不是一个安全阀** —— 见模块头「截断是硬判据」。
///
/// 取 `bin/rollout` 验收那一档（40）而不是生产那一档（`PRODUCTION_MAX_TURNS` 30）：
/// 两个数的次序是有意的，验收口径必须比生产口径宽，宽出来的那一段就是
/// 「生产会不会撞上」的余量。L3 是验收侧，用宽的那个。
pub const EVAL_MAX_TURNS: usize = 40;

/// 默认采样数。和 `bin/rollout` 的 `--samples` 同一个数，理由也同一条：
/// 推演里有**两条**随机流（洗牌 + 敌人分支），再加上 L3 这一层的敌人血量掷点，
/// **单次结果没有意义**。
pub const DEFAULT_SAMPLES: usize = 64;

/// 一次单场评估的配置。
#[derive(Clone, Copy, Debug)]
pub struct EvalCfg {
    pub samples: usize,
    pub max_turns: usize,
    /// 谁在驾驶。**默认 [`Policy::default`]（求解器策略 + `damage_first`）** ——
    /// 别为了"看死亡率"把它换成 `survive_first`：那正是 P4 变红的原因
    /// （单回合目标函数当跨回合策略 ⇒ 只挡不打 ⇒ 打不完 ⇒ 截断）。
    pub policy: Policy,
}

impl Default for EvalCfg {
    fn default() -> EvalCfg {
        EvalCfg {
            samples: DEFAULT_SAMPLES,
            max_turns: EVAL_MAX_TURNS,
            policy: Policy::default(),
        }
    }
}

/// 拒绝作答的理由。**给一个自信的数比拒绝更危险**，所以这不是一个错误码，
/// 它是结果的一部分。
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Refusal {
    /// 场上有敌人内核不认识（或者表里没有招）。推演出来的会是
    /// **一场敌人不出手的仗** —— 12 血打 200 血一滴不掉，见 `rollout` 模块头。
    CannotRollout,
    /// 一只敌人都没有：这不是一场仗
    NoEnemies,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::CannotRollout => {
                write!(f, "有敌人内核不认识（或没有出招表）—— 推演会是一场它不出手的仗")
            }
            Refusal::NoEnemies => write!(f, "场上一只敌人都没有"),
        }
    }
}

/// 一组整数的分布。**报分位数不报方差**：这些量的尾巴是不对称的
/// （血量有 0 这个下界、战损有"一击必杀"这种长尾），方差读不出那件事。
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Dist {
    pub n: usize,
    pub min: i32,
    pub p10: i32,
    pub p50: i32,
    pub p90: i32,
    pub max: i32,
    pub mean: f64,
}

impl Dist {
    /// `v` 会被就地排序（分位数是排出来的）。
    pub fn of(v: &mut [i32]) -> Dist {
        if v.is_empty() {
            return Dist::default();
        }
        v.sort_unstable();
        let at = |p: f64| v[(((v.len() - 1) as f64) * p).round() as usize];
        Dist {
            n: v.len(),
            min: v[0],
            p10: at(0.10),
            p50: at(0.50),
            p90: at(0.90),
            max: v[v.len() - 1],
            mean: v.iter().map(|&x| x as f64).sum::<f64>() / v.len() as f64,
        }
    }
}

impl std::fmt::Display for Dist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.n == 0 {
            return write!(f, "—");
        }
        write!(f, "p10/p50/p90 = {}/{}/{}", self.p10, self.p50, self.p90)
    }
}

/// 一场仗被打完 N 次之后的样子。
pub struct FightEval {
    /// 采了几个样本
    pub n: usize,
    /// 死了几次
    pub deaths: usize,
    /// **撞上回合上限**几次。这些样本的血量不是战斗结果
    pub truncated: usize,
    /// 进这场仗时的血量（`FightSpec::hp`，**开局回血之前**）
    pub start_hp: i32,
    /// 打完并活下来的那些样本的**终点血量**
    pub hp_end: Dist,
    /// 同一批样本的**战损** = `start_hp − final_hp`。
    /// **可以是负的** —— 胜利回血（燃烧之血 / 带骨肉）和开局回血都算在里面，
    /// 而阶段 3 的整幕链正是拿这个数往下接。
    pub loss: Dist,
    /// 同一批样本打了几个回合
    pub turns: Dist,
    /// 构造器自己报的缺口（去重）。**不带这份清单的结论是个自信的数**
    pub gaps: Vec<Gap>,
    /// 不为 `None` 时上面每一栏都是空的
    pub refused: Option<Refusal>,
    /// 逐样本的原始结局。**配对比较（CRN）要用它** ——
    /// 两个候选的样本 `i` 是同一批随机，差值要逐样本算而不是拿分布相减
    pub samples: Vec<Outcome>,
    /// **样本种子的基**（`spec.seed`）。配对比较要求两边逐字相同 ——
    /// 不同的基就是不同的两批随机，那时候的差值是噪声不是效应
    pub seed: u64,
    /// 这副牌组里有几张能力牌 / 身上几瓶药水 / 打的是第几进阶，
    /// [`FightEval::caveats`] 用
    deck_powers: usize,
    potions: usize,
    ascension: u8,
}

impl FightEval {
    pub fn death_rate(&self) -> f64 {
        if self.n == 0 {
            0.0
        } else {
            self.deaths as f64 / self.n as f64
        }
    }

    pub fn truncation_rate(&self) -> f64 {
        if self.n == 0 {
            0.0
        } else {
            self.truncated as f64 / self.n as f64
        }
    }

    /// 打完了、而且活着的样本数 —— 血量分布的分母。
    pub fn finished_alive(&self) -> usize {
        self.hp_end.n
    }

    /// **一行读数，死亡率在最前面。**
    pub fn headline(&self) -> String {
        if let Some(r) = &self.refused {
            return format!("拒绝作答：{r}");
        }
        format!(
            "死 {}/{} ({:.0}%) · 截断 {} · 终点血量 {} · 战损 p50 {} · 回合 p50 {}",
            self.deaths,
            self.n,
            self.death_rate() * 100.0,
            self.truncated,
            self.hp_end,
            if self.loss.n == 0 { "—".to_string() } else { self.loss.p50.to_string() },
            if self.turns.n == 0 { "—".to_string() } else { self.turns.p50.to_string() },
        )
    }

    /// **方向已知的偏差**，从这副牌组自己算出来（见模块头那张表）。
    ///
    /// 没有能力牌就不报能力牌那一条 —— 一段写死的免责文本会被读者跳过，
    /// 一行"这副牌组里有 4 张能力牌，策略对它们系统性低估"不会。
    pub fn caveats(&self) -> Vec<Caveat> {
        let mut v = Vec::new();
        if self.deck_powers > 0 {
            v.push(Caveat::EngineCards(self.deck_powers));
        }
        if self.potions > 0 {
            v.push(Caveat::PotionsUnused(self.potions));
        }
        if !self.gaps.is_empty() {
            v.push(Caveat::Gaps(self.gaps.len()));
        }
        if self.ascension >= 8 {
            v.push(Caveat::SourceOnlyAscension(self.ascension));
        }
        v
    }
}

/// 一条**方向已知**的偏差。它不是免责声明 —— 每条都带着数（几张牌 / 几瓶药）
/// 和方向，读结论时要把它加回去。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Caveat {
    /// 牌组里有 N 张能力牌：跨回合搜索对引擎牌拒绝计价 ⇒ **低估**
    EngineCards(usize),
    /// 身上 N 瓶药水，推演一瓶都不喝 ⇒ **低估**
    PotionsUnused(usize),
    /// 构造器报了 N 条缺口：那几件东西这一场不生效，方向逐条不同
    Gaps(usize),
    /// **A8 以上**：敌人的耐久/输出走 `asc::adjust`，而那张表整张是 `[源码]` 档
    /// —— 全部实录是 A1/A2，**A8 以上一次都没被观测碰过**。方向不定
    SourceOnlyAscension(u8),
}

impl Caveat {
    /// 汇总用的类别名（数不进来 —— 每场的数都不一样）
    pub fn kind(&self) -> &'static str {
        match self {
            Caveat::EngineCards(_) => "带能力牌（引擎牌评不到，**低估**）",
            Caveat::PotionsUnused(_) => "身上带药水（推演不喝，**低估**）",
            Caveat::Gaps(_) => "构造器有缺口（那几件东西不生效）",
            Caveat::SourceOnlyAscension(_) => "A8 以上（进阶数值是 [源码] 档，没被观测碰过）",
        }
    }
}

impl std::fmt::Display for Caveat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Caveat::EngineCards(n) => write!(f, "牌组里 {n} 张能力牌：跨回合搜索对**引擎牌**（每回合给格挡的 / 会衰减的 / 触发式的）拒绝计价，这副牌组的真实成色**比这里报的好**"),
            Caveat::PotionsUnused(n) => write!(f, "身上 {n} 瓶药水：推演一瓶都不喝（`Policy::Solver` 的 `allowed_potions = 0`），方向是**低估**"),
            Caveat::Gaps(n) => write!(f, "构造器报了 {n} 条缺口 —— 那几件东西在这场仗里不生效，逐条见下"),
            Caveat::SourceOnlyAscension(a) => write!(f, "A{a}：敌人的耐久/输出走 `asc::adjust`，而那张表整张是 [源码] 档（全部实录是 A1/A2，**A8 以上一次都没被观测碰过**）—— 方向不定"),
        }
    }
}

/// 样本 `i` 的种子。**只吃 `(base, i)`，牌组进不来** —— 这是 CRN 的全部内容，
/// 见模块头那张表。
///
/// 走一步 splitmix64 而不是 `base + i`：相邻样本的种子只差低几位的话，
/// `Rng::new` 那三条 XOR 派生流的开头会相关，而"两条随机流各自换种子"
/// 本来就是为了把分布量宽。
pub fn sample_seed(base: u64, i: usize) -> u64 {
    let mut z = base.wrapping_add((i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    next_u64(&mut z)
}

/// 把这场仗打完 `cfg.samples` 次。
///
/// **每个样本各 `build` 一次**（不是搭一次然后换种子推演）：敌人血量是
/// [`crate::asc::hp_range`] 里掷出来的，它是这场仗**真实存在**的一条方差 ——
/// 40 血的小怪和 48 血的小怪不是同一场仗。搭一次就把这条方差冻掉了。
pub fn evaluate(spec: &FightSpec, cfg: &EvalCfg) -> FightEval {
    let n = cfg.samples.max(1);
    // 探一次：缺口和"认不认得敌人"都只由 spec 决定，和样本种子无关。
    let probe = build(&FightSpec { seed: sample_seed(spec.seed, 0), ..*spec });
    let deck_powers = spec
        .deck
        .iter()
        .filter(|c| matches!(card(c.id).kind, Kind::Power))
        .count();
    let potions = spec.potions.iter().filter(|&&p| p != 0).count();
    let mut out = FightEval {
        n: 0,
        deaths: 0,
        truncated: 0,
        start_hp: spec.hp,
        hp_end: Dist::default(),
        loss: Dist::default(),
        turns: Dist::default(),
        gaps: dedup_gaps(probe.gaps),
        refused: None,
        samples: Vec::new(),
        seed: spec.seed,
        deck_powers,
        potions,
        ascension: spec.ascension,
    };
    if probe.state.n_enemies == 0 {
        out.refused = Some(Refusal::NoEnemies);
        return out;
    }
    if !can_rollout(&probe.state) {
        out.refused = Some(Refusal::CannotRollout);
        return out;
    }

    let (max_turns, policy, base) = (cfg.max_turns, cfg.policy, spec.seed);
    let samples = par_map(n, |i| {
        let built = build(&FightSpec { seed: sample_seed(base, i), ..*spec });
        rollout_outcome_with(built.state, max_turns, policy)
    });

    out.n = n;
    out.deaths = samples.iter().filter(|o| o.died).count();
    out.truncated = samples.iter().filter(|o| o.truncated).count();
    // **分布的分母只有"打完了并且活着"的样本**，见模块头。
    let mut hp: Vec<i32> =
        samples.iter().filter(|o| !o.truncated && !o.died).map(|o| o.final_hp).collect();
    let mut loss: Vec<i32> = hp.iter().map(|h| spec.hp - h).collect();
    let mut turns: Vec<i32> =
        samples.iter().filter(|o| !o.truncated && !o.died).map(|o| o.turns as i32).collect();
    out.hp_end = Dist::of(&mut hp);
    out.loss = Dist::of(&mut loss);
    out.turns = Dist::of(&mut turns);
    out.samples = samples;
    out
}

/// 同一条缺口在一副牌组里可能报很多遍（每张不认识的牌各一条），
/// 报告里只要**种类**。
fn dedup_gaps(mut gaps: Vec<Gap>) -> Vec<Gap> {
    let mut seen: Vec<String> = Vec::new();
    gaps.retain(|g| {
        let k = g.to_string();
        if seen.contains(&k) {
            false
        } else {
            seen.push(k);
            true
        }
    });
    gaps
}

/// 两个候选的**配对差值**：逐样本相减，再看差值的分布。
///
/// **不是拿两份 [`Dist`] 相减** —— 那等于把 CRN 扔了。配对之后
/// "同一批随机下 B 比 A 好几点血"这件事才量得准，而 L3 要的正是那个差。
///
/// `a` / `b` 必须是**同一个 `spec.seed` 和同一个 `cfg.samples`** 跑出来的，
/// 否则样本 `i` 不是同一批随机 —— 对不上时返回 `None` 而不是硬算。
/// **两个条件都查**：只查样本数的话，两批不同种子的样本会安静地配成一对，
/// 而那时算出来的"差"是两份噪声相减。
pub fn paired_delta(a: &FightEval, b: &FightEval) -> Option<PairedDelta> {
    if a.refused.is_some() || b.refused.is_some() || a.n != b.n || a.n == 0 || a.seed != b.seed {
        return None;
    }
    let mut d: Vec<i32> = Vec::with_capacity(a.n);
    let (mut b_better, mut a_better, mut tie) = (0, 0, 0);
    let (mut only_a_died, mut only_b_died) = (0, 0);
    let mut pairs = 0;
    for i in 0..a.n {
        let (x, y) = (&a.samples[i], &b.samples[i]);
        // 截断的样本血量不是战斗结果 —— 这一对整个跳过（见模块头）
        if x.truncated || y.truncated {
            continue;
        }
        pairs += 1;
        // **死亡和血量分开数，不许混。** 死掉那一侧的 `final_hp` 是 0 或负数，
        // 把它平均进"血量差"里，就是拿一个被截断的量当连续量用 ——
        // 那正是旧 advisor 的毛病（`CLAUDE.md`「怎么读它的数」第二条）。
        // 不一致的那几对（一边死一边活）单独数，**它才是"更安全"的直接证据**。
        match (x.died, y.died) {
            (true, true) => continue,
            (true, false) => {
                only_a_died += 1;
                continue;
            }
            (false, true) => {
                only_b_died += 1;
                continue;
            }
            (false, false) => {}
        }
        let v = y.final_hp - x.final_hp;
        match v.cmp(&0) {
            std::cmp::Ordering::Greater => b_better += 1,
            std::cmp::Ordering::Less => a_better += 1,
            std::cmp::Ordering::Equal => tie += 1,
        }
        d.push(v);
    }
    Some(PairedDelta {
        pairs,
        deaths_delta: b.deaths as i32 - a.deaths as i32,
        only_a_died,
        only_b_died,
        hp: Dist::of(&mut d),
        b_better,
        a_better,
        tie,
    })
}

/// [`paired_delta`] 的结果。**死亡在前**，血量在后 —— 而且两者**不混在一个数里**。
pub struct PairedDelta {
    /// 配上对的样本数（截断的那几对已经扣掉）
    pub pairs: usize,
    /// B 死的次数减 A 死的次数（**负 = B 更安全**）
    pub deaths_delta: i32,
    /// **只有 A 死**的对数 —— B 在这一对上救回了一条命
    pub only_a_died: usize,
    /// **只有 B 死**的对数
    pub only_b_died: usize,
    /// **两边都活下来**的那些对的终点血量差（B − A）。
    /// 分母是 `hp.n`，不是 `pairs` —— 死掉的对一条都不在里面
    pub hp: Dist,
    pub b_better: usize,
    pub a_better: usize,
    pub tie: usize,
}

/// 局面的一句话描述，诊断输出用（"打谁 · 多少血"）。
pub fn describe_fight(s: &State) -> String {
    let mut parts = Vec::new();
    for e in 0..s.n_enemies as usize {
        parts.push(format!(
            "{} {}血",
            crate::content::enemy_def(s.enemy_def[e]).name,
            s.enemies[e].max_hp
        ));
    }
    parts.join(" + ")
}
