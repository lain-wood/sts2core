//! **L2 跨回合 planner 的地基**（限深 expectimax 的 S0）。
//!
//! 这个文件今天**还不做搜索**，它只把搜索需要的三样东西建好，并各自钉上判据：
//!
//! | 部件 | 是什么 | 判据 |
//! |---|---|---|
//! | [`chance_children`] | 机会节点：这一手会抽到什么 | 权重和为 1 · 已知前缀必被抽到 · 枚举和采样的均值一致 |
//! | [`Tt`] | 置换表 | 只在 `存的深度 ≥ 要的深度` 时复用 |
//! | [`plan_line`] | 搜索入口（D=1 就是单回合） | **D=1 必须逐字等于 `solve_turn`** |
//!
//! # 为什么先把机会节点单独做出来
//!
//! 跨回合搜索比单回合多出来的东西**只有两样**：机会节点（抽牌/敌人分支）和
//! 叶评估。叶评估的成色由 P6 标定管（`bin/calib`），机会节点的成色由这里的
//! 三条判据管。两样分开建、分开验，出问题才归得了因 —— 和 `bin/rollout`
//! 把四条判据拆开是同一个道理。
//!
//! # 深度不按"覆盖一轮洗牌"定，按分支坍缩定
//!
//! 一个周期里抽牌的分支宽度是塌下去的：21 张牌每回合抽 5，
//! 不同手牌多重集 16473 -> ~4368 -> ~462 -> 6 -> 1。
//! **周期末尾的手牌是被迫的，最后一两层深度几乎免费。**
//!
//! 但**"多重集比组合数少"这条随幕数迅速失效**（实测：第 1 幕 13 张牌 7 种名字
//! 只有 110 个多重集，第 3 幕 21 张牌 20 种名字有 16473 个，≈ C(21,5)）。
//! 所以 [`Plan::exact_threshold`] 那条"够小就精确枚举"在成熟牌组里
//! **只有周期的最后两三个回合用得上**，前面必须采样。
//!
//! # 药水不进搜索
//!
//! 和 `rollout::Policy::Solver` 同一条理由：药水不要能量，边际收益 > 0 就喝，
//! 一场推演会在第 1 回合把整包喝光。定价在搜索外面（`solver::advise_potions`）。

use crate::rollout::{predicted_threat, Policy};
use crate::solver::{
    enemy_wall, optimistic_damage, replay_line, score, solve_turn_potions, solve_turn_topk,
    solve_turn_topk_split, Line, Threat,
};
use crate::step::end_turn_before_draw;
use crate::content::enemy_def;
use crate::ops::EOp;
use crate::state::{next_below, State, St};


/// 叶子怎么估值。
///
/// # 为什么要做成枚举，而不是继续用一个 `fn(&State) -> i32`
///
/// 推演型叶评估需要**采样数、截断长度、兜底函数、以及一个种子**，
/// 裸函数指针一个都带不出去。而种子这件事不是细节：
/// **兄弟叶子必须共用同一批种子（CRN）**，否则每个候选各抽各的运气，
/// 排序里混进的是采样噪声不是决策差异 —— 2026-08-27 机会节点的播种
/// bug 就是这么来的，那次修完单次可重复性好了 28%。
#[derive(Clone, Copy, Debug)]
pub enum Leaf {
    /// 静态评估。今天的默认，权重是标定出来的（[`Weights::LEAF`](crate::solver::Weights::LEAF)）。
    Eval(fn(&State) -> i32),
    /// **截断推演 + 静态评估兜底**（方案 C）。
    ///
    /// 从叶局面抽一手牌、按 [`Policy::Fast`] 打 `turns` 个回合，
    /// 打完了就用真实结局，没打完就拿截断处的局面过一遍 `tail`。
    ///
    /// # 它买到的是静态评估**结构性**拿不到的东西
    ///
    /// 方案 A 的 `power_horizon_value` 明确拒绝计价三类能力，理由都是
    /// "在叶子上不 determinate"：每回合给格挡的（价值 = `min(格挡, 来袭)`，
    /// 而 `fn(&State)->i32` 拿不到来袭）、会衰减的、触发式的（每回合触发几次
    /// 取决于抽到什么）。**推演里这三类全都自己发生了** ——
    /// 绯红披风真的每回合给 8 点格挡并真的去挡那一手，
    /// 撕裂真的被自伤唤醒，黑暗之拥真的因为消耗而抽牌。
    /// 不需要给它们各写一条换算，也就不需要再标一个常数。
    ///
    /// # 代价：它是**平方级**的
    ///
    /// 每个叶子跑 `samples` 条推演，每条推演走 `turns` 个回合。
    /// `Policy::Fast` 是手写启发式（几十步），所以一个叶子从约 100 ns
    /// 涨到几十 µs。`turns` 和 `samples` 都要按实测定，别凭"更深更准"往大了设 ——
    /// `exact_threshold` 那条教训（凭直觉设 1024，验收慢 3 倍而读数一个字没变）
    /// 在这里同样适用。
    Rollout {
        /// 每个叶子采几条推演。**兄弟之间共用同一批种子**。
        samples: u16,
        /// 最多推几个回合。到了就截断，交给 `tail`。
        turns: u16,
        /// 截断处的兜底评估。**不能拿截断时的血量当结果** ——
        /// 那场仗没分出胜负，报出去就是 [`Outcome::truncated`](crate::rollout::Outcome)
        /// 那条注释点名的"自信地算错"。
        tail: fn(&State) -> i32,
    },
}

impl Leaf {
    /// 估一个叶局面。`depth` 只用来播种 —— 见 [`Leaf`] 头上关于 CRN 那段。
    ///
    /// **种子只由 `cfg.seed` 和 `depth` 决定，不含局面**。含局面的话兄弟叶子
    /// （牌堆本来就不同）会各抽各的样本，最该配对的那一层反而没配对。
    pub fn value(&self, s: &State, seed: u64, depth: usize) -> i32 {
        match *self {
            Leaf::Eval(f) => f(s),
            Leaf::Rollout { samples, turns, tail } => {
                // 打完了就没什么可推的
                if s.combat_over || s.player_dead {
                    return tail(s);
                }
                let n = samples.max(1);
                let base = seed ^ ((depth as u64) << 40) ^ 0x1EAF_0C05_1EAF_0C05;
                let mut total: i64 = 0;
                for i in 0..n {
                    let mut sim = *s;
                    // **叶子是未抽牌的**，所以要先抽一手 —— 而且必须真的洗一次
                    // 未知区：`draw_one` 从数组末尾取牌、**不洗牌**，只改
                    // `rng.shuffle` 的话每个样本抽到的是同一手牌。
                    // 这个坑 2026-08-25 在标定器上踩过一次，症状是噪声地板
                    // 低到 0.03 血。已知前缀（头槌放上去的那张）不参与随机。
                    let mut sd = base.wrapping_add((i as u64).wrapping_mul(GOLDEN));
                    // **绝大多数叶子是未抽牌的**（`end_turn_before_draw` 之后），
                    // 但 `node_value_ext` 有一个兜底分支传进来的是机会节点的孩子，
                    // 那个**手牌已经抽好了**。对它再 `open_hand` 会凭空多抽一手。
                    // 所以这里按手牌空不空分岔，别假设调用方永远给未抽牌的局面。
                    if sim.n_hand == 0 {
                        shuffle_unknown(&mut sim, &mut sd);
                        sim.rng.shuffle = sd;
                        crate::step::open_hand(&mut sim);
                    } else {
                        sim.rng.shuffle = sd;
                    }
                    // **两条随机流各自换种子**：只换 shuffle 的话所有样本共用
                    // 同一条敌人随机流，等于对敌人的随机分支一次都没采样 ——
                    // `rollout_combat` 就有这个毛病，`sample_outcomes` 的注释
                    // 点名过。
                    sim.rng.enemy = base.wrapping_add(
                        ((i as u64) ^ 0xA5A5_A5A5).wrapping_mul(GOLDEN),
                    );
                    let (end, _) =
                        crate::rollout::rollout_to_state(sim, turns as usize, Policy::Fast);
                    // 打完了（赢或死）就是事实，没打完就交给兜底。
                    // 两条路都走同一个 `tail` —— 它认得 `combat_over` 和
                    // `player_dead`，胜负的分在 `Weights` 里本来就压倒一切。
                    total += tail(&end) as i64;
                }
                (total / n as i64) as i32
            }
        }
    }
}

const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;

/// 一次跨回合搜索的参数。
#[derive(Clone, Copy, Debug)]
pub struct Plan {
    /// 搜几个**回合层**。1 = 只搜这一回合，退化成 `solve_turn`。
    pub depth: u8,
    /// 每层机会节点采几个样本（下标 = 层）。超出长度的层用最后一个值。
    pub widths: [u16; 4],
    /// 不同手牌多重集**不超过这个数**就精确枚举（带超几何权重），否则采样。
    ///
    /// **它必须和 `widths` 挂钩，不能凭"精确更好"往大了设。** 枚举一个孩子的
    /// 代价 ≈ 一次 `solve_turn`，所以阈值设成 1024、而采样宽度只有 96 时，
    /// 精确那条路比采样贵 10 倍 —— 换来的是"这一层没有采样误差"，
    /// 而下一层照样在采样。
    ///
    /// **2026-08-25 实测**：阈值 1024 -> 整套 rollout 验收 55 秒；压到 128 -> **19.5 秒**，
    /// 而 P5 的读数**一个字没变**（均值 +1.9，2049/473/5798）。
    /// 也就是说那 3 倍的钱买的是零。
    ///
    /// 判据只有一条：**精确只在它比采样还便宜的时候才划算**。
    pub exact_threshold: usize,
    /// 每层的单回合搜索预算（下标 = 层）。根回合该给足，深层要便宜。
    pub budgets: [u32; 4],
    /// 目标函数。**跨回合别用 `survive_first`** —— 它单回合看挡下 1 点永远比
    /// 打出 1 点值钱 3.3 倍，迭代下去就是一个只挡不打、打不完仗的模拟玩家
    /// （2026-08-25 实测，见 `rollout::Policy::Solver::score`）。
    pub score: fn(&State) -> i32,
    /// 全局种子。CRN 用它和局面指纹一起决定采到哪几手牌。
    pub seed: u64,
    /// 根回合留几条候选线深搜（shortlist 的 K）。
    ///
    /// **它是决定这东西跑不跑得动的那个参数**：每多一条候选，就多
    /// `widths[1]` 次深层单回合搜索。根回合有几十条线，逐条深搜搜不动。
    pub k: usize,
    /// 根候选里给**打出过能力牌的线**额外留几个名额（在 `k` 之外，纯增量）。
    ///
    /// **没有它，能力牌在根上就被剪掉了**：`k` 条候选是按**单回合分数**剪的，
    /// 而能力牌当回合的产出定义上就是 0 伤害 0 格挡，必然垫底。
    /// 于是"打出薪火之源"这条线连深层搜索都进不去 —— 叶评估里加多少项都白搭，
    /// 因为那条线根本不在候选集里。判据和代价见 [`solve_turn_topk`] 的
    /// 「能力线保底」。
    ///
    /// **0 = 关掉，行为和 2026-08-31 之前逐字节相同**，A/B 用它。
    pub power_reserve: usize,
    /// 根候选里给**打掉敌人血量最多（高伤害/抢节奏）的线**额外留几个名额（在 `k` 和 `power_reserve` 之外，纯增量）。
    ///
    /// **解决 Top-K 剪枝与叶评估权重倒挂的另一半**：
    /// 根候选使用单回合分数（如 `survive_first`）时，吃少量擦伤打巨额伤害的攻击线会被防守线挤出 Top-K，
    /// 而深搜叶评估（`Weights::LEAF`）对伤害的估值比单回合高得多（75 vs 30）。
    /// 开启伤害保底后，最大伤害线必定进入候选集供深搜与叶评估决策。
    pub damage_reserve: usize,
    /// 叶评估，见 [`Leaf`]。叶子是**「敌人打完、回合开始的结算做完、还没抽牌」**
    /// 的局面。
    ///
    /// 默认 [`Leaf::Eval`] + [`score::leaf`](crate::solver::score::leaf)，
    /// **权重是标定出来的**（`bin/calib --siblings`，见 `Weights::LEAF`）。
    /// [`Leaf::Rollout`] 是方案 C：把叶子换成截断推演，
    /// 静态评估**结构性**拿不到的那几类（每回合给格挡的 / 触发式的）
    /// 在推演里自己发生。
    ///
    /// **它和 `score`（线的目标函数）是两个参数，不该绑在一起** ——
    /// 标定量出来 `damage_first` 在叶子上排序更差（后悔 2.16 vs 1.96），
    /// 而它当策略目标又是必须的（`survive_first` 当策略会只挡不打）。
    ///
    /// **叶子里没有牌组画像，而且标定说不该有**：兄弟叶子共用同一副牌组
    /// （同一个根，只差这回合打掉/消耗了哪几张），任何牌组统计量在组内
    /// 都近似常数，进不了组内排序。详见 verification-log 的 P6 那一节。
    pub leaf: Leaf,
    /// 斩杀延伸：乐观界显示下回合可能打穿敌人血墙时，多搜一层落实。
    pub ext_lethal: bool,
    /// 尖峰延伸：敌人下一手的允许集合里有一记 ≥ `spike_pct`% 当前血量的大伤害时，
    /// 多搜一层，让"留格挡接这一下"进搜索。
    pub ext_spike: bool,
    /// 尖峰的阈值，**按当前血量的百分比**。20 点伤害对 80 血和对 25 血
    /// 完全是两回事，绝对值阈值在残血时会全程触发。
    pub spike_pct: u8,
    /// 洗牌边界对齐：再抽一手就要洗牌时，多搜一层让叶子落在边界上。
    pub ext_boundary: bool,
    /// 延伸那一层机会节点的宽度。
    ///
    /// **窄了会把延伸自己拖垮**：延伸换来的是"多看一个回合"，但如果那一层
    /// 只采 4 个样本、而正常层采 12 个，多出来的估计噪声可能把收益吃掉。
    /// 这一条 2026-08-25 专门量过，读数记在 verification-log。
    pub ext_width: u16,
}

impl Default for Plan {
    fn default() -> Plan {
        Plan {
            depth: 1,
            widths: [96, 12, 4, 4],
            exact_threshold: 128,
            budgets: [200_000, 2_000, 500, 500],
            score: score::damage_first,
            seed: 0x5EED_0825,
            k: 6,
            power_reserve: 2,
            damage_reserve: 2,
            leaf: Leaf::Eval(score::leaf),
            ext_lethal: true,
            // **默认关掉，实测不划算** —— 见下面「延伸只有在能把估值变成事实
            // 的时候才划算」那段。留着开关是为了以后叶评估换了还能重测。
            ext_spike: false,
            spike_pct: 35,
            ext_boundary: false,
            ext_width: EXT_WIDTH as u16,
        }
    }
}

impl Plan {
    fn width(&self, d: usize) -> usize {
        self.widths[d.min(self.widths.len() - 1)] as usize
    }
    fn budget(&self, d: usize) -> u32 {
        self.budgets[d.min(self.budgets.len() - 1)]
    }
}

// ---------------------------------------------------------------------------
// 置换表
// ---------------------------------------------------------------------------

/// 置换表的一格。
#[derive(Clone, Copy, Default)]
pub struct TtEntry {
    pub key: u64,
    /// **这个值是搜到多深算出来的。** 只有 `stored >= needed` 才能复用 ——
    /// 拿一个搜得更浅的值去顶替深搜，等于偷偷把深度砍了，而且不报错。
    pub depth: u8,
    /// 第几次搜索写的。跨回合保留，靠它淘汰旧值。
    pub age: u16,
    pub value: i32,
}

/// 定长置换表。**不用 `HashMap`**：这一层要能被 `State` 一样的纪律约束
/// （固定容量、无分配），而且直接寻址比哈希表快。
///
/// 冲突就直接覆盖（always-replace），但**优先覆盖 age 更旧的那个** ——
/// 深搜的值比浅搜的值贵，同一轮里不该被随手挤掉。
pub struct Tt {
    slots: Vec<TtEntry>,
    mask: usize,
    age: u16,
    pub hits: u64,
    pub probes: u64,
}

impl Tt {
    /// `bits` = 表的对数大小（20 -> 1M 格 -> 约 16 MB）。
    pub fn new(bits: u32) -> Tt {
        let n = 1usize << bits;
        Tt { slots: vec![TtEntry::default(); n], mask: n - 1, age: 1, hits: 0, probes: 0 }
    }

    /// 新一轮搜索。**不清表** —— 跨回合保留正是置换表的价值所在。
    pub fn bump_age(&mut self) {
        self.age = self.age.wrapping_add(1);
    }

    pub fn probe(&mut self, key: u64, need_depth: u8) -> Option<i32> {
        self.probes += 1;
        let e = self.slots[(key as usize) & self.mask];
        if e.key == key && e.depth >= need_depth {
            self.hits += 1;
            return Some(e.value);
        }
        None
    }

    pub fn store(&mut self, key: u64, depth: u8, value: i32) {
        let ix = (key as usize) & self.mask;
        let old = self.slots[ix];
        // 同一轮里，深的压浅的；跨轮无条件覆盖
        if old.age == self.age && old.depth > depth {
            return;
        }
        self.slots[ix] = TtEntry { key, depth, age: self.age, value };
    }
}

// ---------------------------------------------------------------------------
// 机会节点：这一手会抽到什么
// ---------------------------------------------------------------------------

/// **把抽牌堆的未知区洗一次**，已知前缀原样不动。
///
/// 机会节点的采样分支和 P6 的标定共用这一份 —— 两边各写一遍，迟早有一边
/// 忘了"已知前缀不参与随机"，而那种错**不报错，只是搜错**。
///
/// 2026-08-25 踩过一次相关的坑：标定那边只改了 `rng.shuffle` 就去 `open_hand`，
/// 而 `draw_one` 是从数组末尾取牌、**不洗牌**，于是 64 个样本抽到的是同一手牌，
/// 量出来的"叶子的值"其实是"某一条固定抽牌序列下的结局"。
/// 症状是标签的噪声地板低到 0.03 血 —— 一个好得不合理的数就是这么用的。
pub fn shuffle_unknown(s: &mut State, seed: &mut u64) {
    let n = s.n_draw as usize;
    let known = (s.n_draw_known as usize).min(n);
    let m = n - known;
    for i in (1..m).rev() {
        let j = next_below(seed, i + 1);
        s.draw.swap(i, j);
    }
}

/// 机会节点的一个孩子：一个**已经抽好手牌**的局面，和它的概率。
pub struct Draw {
    pub state: State,
    pub p: f64,
}

/// 一张牌在"能不能抽到"这件事上的身份。
///
/// 和 `solver::key` 里的 `card_ident` 同一个口径（id / 升级 / 腐化 / 加值 /
/// 改过的费用），**因为"抽到哪一张"就是按这个口径区分的**：
/// 两张同名但一张涨过费的牌，抽到哪张不一样。
fn ident(s: &State, ix: u8) -> u64 {
    let c = s.cards[ix as usize];
    (c.id as u64) << 40
        | (c.flags as u64) << 24
        | (c.bonus as u16 as u64) << 8
        | (c.cost_delta as u8 as u64)
}

fn binom(n: usize, k: usize) -> f64 {
    if k > n {
        return 0.0;
    }
    let k = k.min(n - k);
    let mut r = 1.0f64;
    for i in 0..k {
        r = r * ((n - i) as f64) / ((i + 1) as f64);
    }
    r
}

/// 把一段牌按身份分组，返回 (身份, 这段里的下标们)。
fn group(s: &State, region: &[u8]) -> Vec<(u64, Vec<u8>)> {
    let mut g: Vec<(u64, Vec<u8>)> = Vec::new();
    for &ix in region {
        let id = ident(s, ix);
        match g.iter_mut().find(|(k, _)| *k == id) {
            Some((_, v)) => v.push(ix),
            None => g.push((id, vec![ix])),
        }
    }
    g
}

/// 从分组里枚举所有"取 r 张"的**不同多重集**，附超几何权重。
///
/// 返回 (每组各取几张, 概率)。概率 = ∏ C(组大小, 取几张) / C(总数, r)。
fn enumerate_takes(g: &[(u64, Vec<u8>)], r: usize) -> Vec<(Vec<usize>, f64)> {
    let n: usize = g.iter().map(|(_, v)| v.len()).sum();
    let denom = binom(n, r);
    let mut out = Vec::new();
    let mut cur = vec![0usize; g.len()];
    fn rec(
        g: &[(u64, Vec<u8>)],
        i: usize,
        left: usize,
        cur: &mut Vec<usize>,
        out: &mut Vec<(Vec<usize>, f64)>,
    ) {
        if left == 0 {
            out.push((cur.clone(), 0.0));
            return;
        }
        if i == g.len() {
            return;
        }
        // 剩下的组加起来够不够 —— 剪掉根本凑不满 r 张的分支
        let rest: usize = g[i..].iter().map(|(_, v)| v.len()).sum();
        if rest < left {
            return;
        }
        for take in 0..=g[i].1.len().min(left) {
            cur[i] = take;
            rec(g, i + 1, left - take, cur, out);
        }
        cur[i] = 0;
    }
    rec(g, 0, r, &mut cur, &mut out);
    for (takes, p) in out.iter_mut() {
        let mut w = 1.0f64;
        for (j, t) in takes.iter().enumerate() {
            w *= binom(g[j].1.len(), *t);
        }
        *p = if denom > 0.0 { w / denom } else { 0.0 };
    }
    out
}

/// 不同多重集有多少个（不真的枚举，只数）。给"要不要精确枚举"做判据用。
pub fn distinct_hands(s: &State, r: usize) -> usize {
    let n = s.n_draw as usize;
    let known = (s.n_draw_known as usize).min(n);
    let region: Vec<u8> = s.draw[..n - known].to_vec();
    let g = group(s, &region);
    let sizes: Vec<usize> = g.iter().map(|(_, v)| v.len()).collect();
    fn cnt(sizes: &[usize], i: usize, left: usize) -> usize {
        if left == 0 {
            return 1;
        }
        if i == sizes.len() {
            return 0;
        }
        let rest: usize = sizes[i..].iter().sum();
        if rest < left {
            return 0;
        }
        (0..=sizes[i].min(left)).map(|t| cnt(sizes, i + 1, left - t)).sum()
    }
    cnt(&sizes, 0, r.min(n - known))
}

/// **机会节点**：`s` 是「回合开始、还没抽牌」的局面
/// （`step::end_turn_before_draw` 给出来的那种），返回这一手可能抽到的各种手牌。
///
/// # 三件事按顺序
///
/// 1. **已知前缀是确定的。** 抽牌堆顶那 `n_draw_known` 张身份明确
///    （头槌放上去的、`PutOnTopOfDraw` 放上去的），它们**必被抽到**，
///    不参与随机。把它们当随机是白白丢信息。
/// 2. 剩下要抽的 `r` 张从**未知区**里出。不同多重集不超过
///    [`Plan::exact_threshold`] 就精确枚举（超几何权重），否则采样。
/// 3. **采样种子只由「还没抽的是哪些牌」和深度决定**（CRN）：
///    兄弟动作比较用同一组手牌，方差抵消；同一个节点重访拿到同一批样本，
///    置换表里存的采样估计才自洽。
///
/// 抽牌堆不够 `want` 张时，剩下的从弃牌堆洗回来。抽牌堆里那几张是**全都要
/// 抽走**的（不构成选择），真正的随机在弃牌堆那次洗牌上 —— 所以这种情况
/// **走采样分支**（`cfg.width(depth)` 个样本，每个 `p = 1/w`）。
///
/// > 2026-09-02 之前它和「已知前缀就是全部」合在一个 `if` 里，一起返回单个
/// > `p = 1.0` 的孩子，而且**不读 `cfg.seed`**。那是错的，理由和实测读数
/// > 写在 [`chance_children`] 里那个分支上。
pub fn chance_children(s: &State, cfg: &Plan, depth: usize) -> Vec<Draw> {
    const WANT: usize = 5;
    let n = s.n_draw as usize;
    let known = (s.n_draw_known as usize).min(n).min(WANT);
    let r = WANT - known;

    // **1. 前 5 张全是已知前缀 ⇒ 真的确定，`p = 1.0` 就是对的。**
    // （`r == 0` 蕴含 `known == 5`，也就蕴含 `n >= 5`，和下面那条不重叠。）
    if r == 0 {
        let mut st = *s;
        crate::step::open_hand(&mut st);
        return vec![Draw { state: st, p: 1.0 }];
    }

    // **2. 抽牌堆凑不齐一手 ⇒ 要洗弃牌堆。**
    //
    // 这一条 2026-09-02 之前和上面那条合在一个 `if` 里、一起返回单个
    // `p = 1.0` 的孩子。**两种情况完全不同**：上面那条确实确定，这一条里
    // 「整个弃牌堆洗回来洗成什么样」是那一刻的**主导**不确定性，
    // 却既不枚举也不采样。
    //
    // 最要命的是那条路径**不读 `cfg.seed`**（洗牌用的是状态自带的
    // `rng.shuffle`）—— 于是 `tools/plan_seed_sweep.py` 把这些回合一律
    // 记成"换种子不变"，**那个可重复性指标恰好在不确定性最大的回合上是瞎的**。
    // [实测] 2026-09-02：`n_draw=3` + 弃牌堆 6 张，孩子数 1、`p=1.00`、
    // 手牌里有 2 张是洗回来的，换 5 个 `Plan::seed` 结果逐字相同。
    // 发生频率：60 条实录的 230 个 `end_turn` 帧里 **57 个** `draw_count < 5`
    // （其中 54 个弃牌堆非空、真要洗）= **25% 的回合边界**。
    //
    // 弃牌堆是空的时候它**仍然是确定的**（把抽牌堆剩下的全抽走，没别的可能），
    // 那种情况照旧给一个 `p = 1.0` 的孩子 —— 采 12 个一模一样的样本
    // 只是白花 12 倍的钱。
    if n < WANT {
        if s.n_disc == 0 {
            let mut st = *s;
            crate::step::open_hand(&mut st);
            return vec![Draw { state: st, p: 1.0 }];
        }
        return sample_children(s, cfg, depth);
    }

    let region: Vec<u8> = s.draw[..n - (s.n_draw_known as usize).min(n)].to_vec();
    let g = group(s, &region);
    let n_distinct = distinct_hands(s, r);

    if n_distinct <= cfg.exact_threshold {
        let takes = enumerate_takes(&g, r);
        let mut out = Vec::with_capacity(takes.len());
        for (t, p) in takes {
            if p <= 0.0 {
                continue;
            }
            // 把选中的那几张挪到未知区的**顶部**（= 已知前缀的正下方），
            // 剩下的排在它们下面。抽牌从数组末尾取，所以这样抽出来的正好是它们。
            let mut chosen: Vec<u8> = Vec::with_capacity(r);
            let mut rest: Vec<u8> = Vec::new();
            for (j, (_, ixs)) in g.iter().enumerate() {
                for (m, &ix) in ixs.iter().enumerate() {
                    if m < t[j] {
                        chosen.push(ix);
                    } else {
                        rest.push(ix);
                    }
                }
            }
            let mut st = *s;
            let base = region.len();
            for (i, &ix) in rest.iter().enumerate() {
                st.draw[i] = ix;
            }
            for (i, &ix) in chosen.iter().enumerate() {
                st.draw[rest.len() + i] = ix;
            }
            debug_assert_eq!(rest.len() + chosen.len(), base);
            crate::step::open_hand(&mut st);
            out.push(Draw { state: st, p });
        }
        return out;
    }

    sample_children(s, cfg, depth)
}

/// 机会节点的**采样**分支。两个入口共用（未知区太大枚举不动 / 抽牌堆凑不齐
/// 一手要洗弃牌堆），**播种规则因此只有一处定义**。
///
/// **CRN：种子只由深度决定，故意不含这个节点自己的牌堆。**
///
/// 原来这里是 `draw_multiset_key(s) ^ cfg.seed ^ depth`，注释也写着 CRN ——
/// 但那样**恰恰破坏了要配对的那一层**：根回合的几条候选线打的牌不同，
/// 留下的牌堆就不同，于是每条候选各抽各的样本，兄弟之间的比较是**独立**的，
/// 排序里混进了整整一份采样噪声。
///
/// [实测] 2026-08-27 第 2 幕精英那一帧（`traces/act2_f24_elite_prism.json` 帧17）：
/// 「凌虐」根分最高（−600），深层估值 103512，输给冠军线 **182 分（0.18%）**；
/// **换 8 个种子，8 个都选凌虐** —— 也就是说那次落选完全是采样噪声。
///
/// 改成只由深度播种之后，同一层的所有节点共用一条随机流：牌堆不同 ⇒ 抽到的牌
/// 当然不同，但"运气"是同一份，差值里那部分方差被抵消掉。
/// 这就是配对随机数（common random numbers）本来的样子。
///
/// **置换表仍然稳**：种子不再依赖状态，同一个状态在同一层永远采到同一批样本
/// （比原来更强的性质，原来还要求牌堆多重集相同）。
fn sample_children(s: &State, cfg: &Plan, depth: usize) -> Vec<Draw> {
    const WANT: usize = 5;
    let w = cfg.width(depth).max(1);
    let mut seed = cfg.seed ^ ((depth as u64) << 56);
    // **抽牌堆凑不齐一手时，随机性在弃牌堆那次洗牌里，不在抽牌堆里。**
    // `shuffle_unknown` 只动抽牌堆的未知区；那次洗牌发生在
    // `open_hand -> draw_one -> reshuffle_discard_into_draw` 里，用的是**状态
    // 自带**的 `rng.shuffle`。不把它也从样本流里播一次，w 个样本会逐字相同。
    //
    // **只在真要洗牌时动它。** 抽牌堆够抽的时候这一手已经被 `shuffle_unknown`
    // 定死了，`rng.shuffle` 只影响**更深层**的抽牌 —— 顺手改掉它就等于
    // 悄悄改了原有那条采样路径的行为（和这次要修的东西无关）。
    let needs_reshuffle = (s.n_draw as usize) < WANT;
    let mut out = Vec::with_capacity(w);
    let p = 1.0 / w as f64;
    for _ in 0..w {
        let mut st = *s;
        shuffle_unknown(&mut st, &mut seed);
        if needs_reshuffle {
            st.rng.shuffle = crate::state::next_u64(&mut seed);
        }
        crate::step::open_hand(&mut st);
        out.push(Draw { state: st, p });
    }
    out
}

// ---------------------------------------------------------------------------
// 搜索入口
// ---------------------------------------------------------------------------

/// 搜这一回合怎么打。**`depth == 1` 时它就是 `solver::solve_turn`。**
///
/// 今天只有 D=1 这一条路是通的 —— 更深的层要等叶评估（P6 标定在
/// `bin/calib`）和延伸规则做完。**在那之前它不该被当成"跨回合求解器"用。**
///
/// 威胁由 [`rollout::predicted_threat`](crate::rollout::predicted_threat) 提供：
/// 跨回合没有观测，只能拿 `EnemyDef` 推，而 `Threat` 的契约是**最终值**，
/// 所以那边要过一遍伤害管线。
pub fn plan_line(s: &State, cfg: &Plan) -> Line {
    let threat = predicted_threat(s);
    plan_line_with_threat(s, cfg, &threat)
}

/// 同 [`plan_line`]，但**根回合的威胁由调用方给定**。
///
/// 实战驱动时根回合的威胁是**观测到的意图标签**（已实测是最终伤害值，
/// 见 `docs/trace-format.md` 约束 4），比 `EnemyDef` 预测准。更深的层没有观测，
/// 仍然由 [`predicted_threat`] 提供；回合边界照旧走 `step(EndTurn)`，
/// 和 [`crate::rollout`] 的约定一个字都没改 —— 换掉的只有**根回合**那一份威胁。
///
/// 这个区分是有代价的：根回合的威胁准了，但根回合**敌人真正打出的那一手**
/// 仍然由 `EnemyDef` 掷（`line_value` 走 `end_turn_before_draw`），
/// 所以深层看到的局面可能和游戏里真的发生的那一手不一致。
/// 单回合线的排序不受影响（它只用根威胁），受影响的是 D≥2 的那部分估值。
pub fn plan_line_with_threat(s: &State, cfg: &Plan, threat: &Threat) -> Line {
    if cfg.depth <= 1 {
        return solve_turn_potions(s, threat, cfg.score, cfg.budget(0), 0).line;
    }
    let cands = solve_turn_topk(
        s,
        threat,
        cfg.score,
        cfg.budget(0),
        0,
        cfg.k,
        cfg.power_reserve,
        cfg.damage_reserve,
    );
    if cands.is_empty() {
        return solve_turn_potions(s, threat, cfg.score, cfg.budget(0), 0).line;
    }
    // 每条候选独立，`State` 是 POD + `Copy`，直接并行
    let vals: Vec<i32> = std::thread::scope(|sc| {
        let hs: Vec<_> = cands
            .iter()
            .map(|c| {
                let c = *c;
                sc.spawn(move || line_value(s, &c, cfg))
            })
            .collect();
        hs.into_iter().map(|h| h.join().expect("候选线求值不该 panic")).collect()
    });
    // 并列判据和 `solver::consider_stopping_here` **一个口径**：
    // 先比打掉的敌人血，再比线的长短。口径由 [`tiebreak_left`] 收口 ——
    // 这里曾经自己拼过一遍，而且**拼错了**（见那个函数的文档）。
    let left: Vec<i32> = cands.iter().map(|c| tiebreak_left(s, c, threat)).collect();
    let mut best = 0usize;
    for i in 1..cands.len() {
        let better = vals[i] > vals[best]
            || (vals[i] == vals[best]
                && (left[i] < left[best]
                    || (left[i] == left[best] && cands[i].n < cands[best].n)));
        if better {
            best = i;
        }
    }
    let mut out = cands[best];
    out.score = vals[best];
    out
}

/// **诊断用**：把根回合的候选线连同它们的深层估值一起吐出来。
///
/// 它回答的是一个很具体的问题：**这条我认为该打的线，到底是没进候选、
/// 还是进了候选但被打了低分？** 两者的修法完全不同 ——
/// 前者要动候选生成（`solve_turn_topk` 用的是根威胁，看不见"我这一手会改敌人打多少"），
/// 后者要动叶评估或权重。
///
/// 返回 `(线, 深层估值)` 的表，外加**主表有几条** —— 下标 >= 这个数的
/// 就是[能力线保底](Plan::power_reserve)与[伤害线保底](Plan::damage_reserve)捞回来的那几条。
///
/// 这个边界是诊断的一半：一条线排在保底段里，说明它靠自己的单回合分数
/// 根本进不了候选集，那是候选生成的问题，不是叶评估的问题。
pub fn plan_candidates(s: &State, cfg: &Plan, threat: &Threat) -> (Vec<(Line, i32)>, usize) {
    let (main, rescued_p, rescued_d) = solve_turn_topk_split(
        s,
        threat,
        cfg.score,
        cfg.budget(0),
        0,
        cfg.k,
        cfg.power_reserve,
        cfg.damage_reserve,
    );
    let n_main = main.len();
    let all: Vec<Line> = main.into_iter().chain(rescued_p).chain(rescued_d).collect();
    (all.iter().map(|c| (*c, line_value(s, c, cfg))).collect(), n_main)
}

/// 并列判据的第二关键字：**这条线打完、而且敌人也打完之后**，敌人还剩多少血。
///
/// # 这个函数存在的理由，就是它修掉的那个 bug
///
/// 2026-09-01 之前 planner 在这一步自己写了一遍，取的是
/// `replay_line(s, line)` —— **我这条线打完、敌人还没动**的局面。
/// 而 `solver::consider_stopping_here` 取的是 `threat.end_turn(s)` ——
/// **敌人打完、我下回合开始的结算也做完**的局面。
///
/// 两个函数体逐字相同，作用的局面却差一整个敌人回合。有回合边界伤害的场上
/// 就会分岔（2026-09-01 实测：荆棘 5 差 5 点、滚石 10 差 10 点），
/// 于是 D=1 和 D≥2 在**并列局面**上会给出不同的线 —— 正是当时那条注释
/// 声称不该发生的事。
///
/// **那条注释还把守卫认错了**：它说 `plan_depth_one_is_exactly_solve_turn`
/// 守着这件事，而 `plan_line_with_threat` 在 `depth <= 1` 时**提前 return**，
/// 那条测试根本走不到这段代码。改错了不会有任何东西变红。
/// 今天守这条的是 `plan_tiebreak_is_measured_after_the_enemy_turn`。
///
/// 口径统一到 solver 那一边（敌人打完之后），因为叶评估本来就定义在那个时点
/// （模块头「打分发生在挨完打之后」）。
pub(crate) fn tiebreak_left(s: &State, line: &Line, threat: &Threat) -> i32 {
    replay_line(s, line.acts())
        .map(|a| crate::solver::enemy_hp_left(&threat.end_turn(a)))
        // 线重放不出来：给一个不会被选中的值，别静默当成"打掉了很多血"
        .unwrap_or(i32::MAX)
}

/// 一条根候选线值多少：打完它，让敌人真的打这一手，然后往下递归。
fn line_value(s: &State, line: &Line, cfg: &Plan) -> i32 {
    let Some(after) = replay_line(s, line.acts()) else {
        // 线重放不出来（不该发生）：给一个不会被选中的分数，别静默当成好线
        return i32::MIN / 2;
    };
    if after.combat_over {
        // 仗在根回合就打完了，没有「下一个回合」可推 —— 这一支和叶子的
        // 采样无关，深度记 0。
        return cfg.leaf.value(&after, cfg.seed, 0);
    }
    let s1 = end_turn_before_draw(after);
    node_value(&s1, cfg, cfg.depth - 1, 1)
}

// ---------------------------------------------------------------------------
// 三类延伸
// ---------------------------------------------------------------------------
//
// 象棋引擎那套 quiescence / singular extension 的对应物：**叶子落在一个
// "话说到一半"的地方时，别评估，多搜一层把话说完。**
//
// 三条触发条件**全部读内核里已有的确定量**，一条启发式猜测都没有：
//   斩杀 —— 牌堆里拿得到的最大伤害（`Op::Damage` 的面板值 + 力量）
//   尖峰 —— `step::allowed_next` 给的下一手允许集合
//   边界 —— 运行时的 `n_draw`（**不能预计算**：消耗会永久移除牌，
//           第 3 幕 Boss 还每 6 张牌往手牌塞一张凋萎，"周期"根本不稳定）
//
// # 一条让人放心的性质：**延伸判错只花时间，不会算错**
//
// 延伸做的事是「把一个估值换成一次真的搜索」。所以触发条件宽了，代价是慢；
// 窄了，代价是少捞一点。**两个方向都不会让结果变错** —— 这也是为什么
// 斩杀那个"乐观界"不必是严格上界（生成牌/连打那类可以超出 5 张的情况，
// 它会低估），宽松一点只是少延伸几次。
//
// # 实测出来的一条判据：**延伸只有在能把"估值"变成"事实"时才划算**
//
// 2026-08-25 三条各自消融（8320 对配对样本，D=2 vs D=1 的血量优势）：
//
// ```
// 都不开        +2.4 血（更好 2240 / 更差 407）  20.5 秒
// 只斩杀        +2.8 血（更好 2601 / 更差 250）  34.3 秒   ← 留着
// 只尖峰        +2.3 血（更好 2155 / 更差 523）  35.5 秒   ← 关掉
// 只边界        +2.4 血（和不开一模一样）        47.4 秒   ← 关掉
// ```
//
// **斩杀延伸把一件事从"估"变成了"判"**：多搜那一层之后敌人到底死没死是
// `combat_over` 说了算，估值里那一大块不确定当场塌缩成事实。
//
// **尖峰和边界什么都没变成事实**：多看一个回合，末端还是同一个叶评估在估，
// 只是把它的误差往后推了一层 —— 花了钱，还多叠一层偏差（尖峰的「更差」
// 从 407 涨到 523 就是这个）。
//
// 顺带证伪了一个听着合理的猜测：**不是窄采样拖累了尖峰**。
// 把延伸宽度从 4 调到 12（和正常层一样），读数一个字没变（+2.3，2151/515）。
//
// 边界那条还有一层更根本的原因：**它只有在叶评估认牌堆时才可能划算**，
// 而 P6 标定的结论正是叶评估不该认牌组（兄弟叶子共用同一副牌）。
// 也就是说边界对齐是 S2 的下游，S2 撤了它就没了立足点。

/// 这个叶子还能延伸几层。**硬上限，防的是"每层都觉得自己该延伸"。**
///
/// 2 层是掂量出来的起点：斩杀 + 边界最多各一次，再多就该调 `depth` 而不是
/// 靠延伸堆深度。
pub const MAX_EXTEND: u8 = 2;

/// **硬深度闸。** 正常情况下永远碰不到（`depth + MAX_EXTEND` 就到顶了），
/// 它防的是"延伸预算写漏一处"那种错：少扣一次 `ext`，每一层都会觉得自己
/// 还能再借，搜索**不报错、只是永远跑不完**。
///
/// 有了这道闸，同样的错会降级成"深度被截断" —— 而 `PlanStat::max_depth`
/// 会当场把它照出来，测试一条断言就能钉死。
/// **一个只会变慢、不会变错的 bug 是最难查的**，所以这里宁可多一道闸。
pub const HARD_DEPTH_CAP: usize = 12;

/// 延伸时机会节点的宽度。**窄采样** —— 延伸是为了把一件具体的事看清楚
///（能不能斩杀 / 这一记尖峰打不打得住），不是为了把分布铺开。
pub const EXT_WIDTH: usize = 4;


/// 敌人**下一手**在允许集合里的最大伤害（过一遍伤害管线，所以是最终值）。
///
/// 读的是 `step::allowed_next` —— 内核里现成的、判 `--predict-enemy` 用的
/// 那个集合，不是另猜一个。集合里有几手就取最狠的那手：尖峰延伸要防的
/// 正是"最坏情况打不打得住"。
fn next_spike(s: &State) -> i32 {
    let mut worst = 0;
    for e in 0..s.n_enemies as usize {
        if !s.enemies[e].alive() {
            continue;
        }
        let def = enemy_def(s.enemy_def[e]);
        let allowed = crate::step::allowed_next(s, e);
        let mut best = 0;
        for m in 0..def.moves.len().min(32) {
            if allowed & (1u32 << m) == 0 {
                continue;
            }
            let mut d = 0;
            for op in def.moves[m].ops {
                if let EOp::Attack { base, hits } = *op {
                    let face = base + s.enemies[e].get(St::Strength);
                    d += crate::damage::apply_modifiers(face, &s.enemies[e], &s.player) * hits;
                }
            }
            best = best.max(d);
        }
        worst += best;
    }
    worst
}

/// 该不该在这个叶子上再多搜一层，以及为什么。
///
/// **返回 `None` 就是"就地评估"。** 三条各自读一个确定量，见模块里那段注释。
pub(crate) fn want_extension(s: &State, cfg: &Plan) -> Option<Ext> {
    // 1. 斩杀：乐观界够得着敌人的血墙 ⇒ 别在"连招中间"评估，
    //    多搜一层把"到底杀不杀得掉"落实。
    //
    //    **这里用 `enemy_wall`（只算当前形态），不是 `enemy_wall_all_forms`。**
    //    延伸划算的判据是一句话：**只有在能把"估值"变成"事实"的时候才划算** ——
    //    多搜那一层过后，「这一形态死没死」由 `combat_over` / 复活规则说了算，
    //    估值里最大的一块不确定当场塌缩。而「整场（含后面几个形态）打不打得完」
    //    多搜一层根本落实不了，拿它当条件只会让延伸在多形态 Boss 上永不触发。
    //    `horizon` 要的是另一个口径，两者**故意不同**，见
    //    [`solver::enemy_wall`](crate::solver::enemy_wall) 上的那张表。
    if cfg.ext_lethal && optimistic_damage(s) >= enemy_wall(s) {
        return Some(Ext::Lethal);
    }
    // 2. 尖峰：下一手的允许集合里有一记大的 ⇒ 让"留格挡/留药水接这一下"
    //    进搜索，而不是靠叶评估去估。
    //    阈值按**当前血量的比例**，不是绝对值：20 点对 80 血和对 25 血
    //    完全是两回事。
    if cfg.ext_spike {
        let spike = next_spike(s);
        if spike > 0 && spike * 100 >= s.player.hp * cfg.spike_pct as i32 {
            return Some(Ext::Spike);
        }
    }
    // 3. 洗牌边界：**按运行时的牌堆算**。这一副牌的"周期"不稳定 ——
    //    消耗永久移除牌，第 3 幕 Boss 每 6 张牌还往手牌塞一张凋萎，
    //    所以不能预计算周期长度，只能看现在抽牌堆里还剩几张。
    //
    //    再抽一手就要洗牌（`n_draw` 在 5..10）⇒ 多搜一层，让叶子正好落在
    //    边界上：那里抽牌堆刚被重置成"整副牌减去消耗掉的"，
    //    评估函数不用去推理一个抽了一半的牌堆。
    if cfg.ext_boundary {
        let n = s.n_draw as i32;
        if (5..10).contains(&n) {
            return Some(Ext::Boundary);
        }
    }
    None
}

/// 一次搜索的**账**。不参与决策，只用来看清楚搜索实际长什么样。
///
/// `max_depth` 是这里最要紧的一个：**它是延伸预算唯一看得见的证据**。
/// 预算写漏一处（比如忘了 `ext -= 1`），搜索不会报错，只会越钻越深 ——
/// 表现出来是"planner 卡住了"，而不是一条红。有了这个数，
/// 一个断言就能把它钉死。
#[derive(Clone, Copy, Default, Debug)]
pub struct PlanStat {
    /// 递归到过的最深层（根回合算 0）
    pub max_depth: u8,
    /// 延伸触发了几次
    pub ext_fired: u32,
}

/// 延伸的三种理由。只用来统计和解释，不影响算法。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ext {
    Lethal,
    Spike,
    Boundary,
}

/// **叶子在这里**：`s` 是「敌人打完、回合开始结算做完、还没抽牌」的局面。
///
/// `left` 还剩几个回合层。`left == 0` 就地评估，否则展开机会节点、
/// 在每个孩子上解一个回合、再往下。
fn node_value(s: &State, cfg: &Plan, left: u8, depth: usize) -> i32 {
    let mut st = PlanStat::default();
    node_value_ext(s, cfg, left, depth, MAX_EXTEND, &mut st)
}

/// `ext` 是**还能延伸几层**的预算。它和 `left` 是两笔账：
/// `left` 是计划好的深度，`ext` 是"话说到一半"时额外借的层数。
/// 分开记的理由是不让延伸把计划深度也一起吃掉 —— 借来的层只能借一次。
pub(crate) fn node_value_ext(
    s: &State,
    cfg: &Plan,
    left: u8,
    depth: usize,
    ext: u8,
    stat: &mut PlanStat,
) -> i32 {
    stat.max_depth = stat.max_depth.max(depth.min(255) as u8);
    if s.combat_over || s.player_dead || depth >= HARD_DEPTH_CAP {
        return cfg.leaf.value(s, cfg.seed, depth);
    }
    // 计划的深度走完了：看看这个叶子是不是落在"话说到一半"的地方
    let mut left = left;
    let mut ext = ext;
    let mut narrow = false;
    if left == 0 {
        if ext == 0 {
            return cfg.leaf.value(s, cfg.seed, depth);
        }
        match want_extension(s, cfg) {
            None => return cfg.leaf.value(s, cfg.seed, depth),
            Some(_) => {
                // 借一层，用窄采样：延伸是为了把一件具体的事看清楚，
                // 不是把分布铺开
                left = 1;
                ext -= 1;
                stat.ext_fired += 1;
                narrow = true;
            }
        }
    }
    let kids = if narrow {
        let mut c = *cfg;
        c.widths = [cfg.ext_width; 4];
        chance_children(s, &c, depth)
    } else {
        chance_children(s, cfg, depth)
    };
    if kids.is_empty() {
        return cfg.leaf.value(s, cfg.seed, depth);
    }
    let mut acc = 0.0f64;
    for k in &kids {
        let threat = predicted_threat(&k.state);
        let line = solve_turn_potions(&k.state, &threat, cfg.score, cfg.budget(depth), 0).line;
        let v = match replay_line(&k.state, line.acts()) {
            Some(after) if !after.combat_over => {
                let s1 = end_turn_before_draw(after);
                node_value_ext(&s1, cfg, left - 1, depth + 1, ext, stat)
            }
            Some(after) => cfg.leaf.value(&after, cfg.seed, depth + 1),
            None => cfg.leaf.value(&k.state, cfg.seed, depth + 1),
        };
        acc += k.p * v as f64;
    }
    acc.round() as i32
}
