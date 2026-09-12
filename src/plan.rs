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
    /// 静态那一半是哪个函数（[`Leaf::Rollout`] 给的是它截断处的兜底 `tail`）。
    ///
    /// **`bin/plan_audit` 靠它把 A2 那一列对齐到 planner 真的在用的尺子上。**
    /// 审计台自己搭的口径和被测对象用的口径分家，量出来的就不是被测对象了 ——
    /// 和 [`Plan::k_at`] 那句“只有这一处定义”是同一条规矩。
    pub fn score_fn(&self) -> fn(&State) -> i32 {
        match *self {
            Leaf::Eval(f) => f,
            Leaf::Rollout { tail, .. } => tail,
        }
    }
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

/// 一手抽几张。**只有这一处定义** —— `chance_children` / `sample_children` /
/// `chance_is_certain` 三个地方各写一个 `5`，改起来必然漏一个。
/// （`step::open_hand` 那个 5 是另一件事：那是 L1 的规则，这里是 L2 对它的预期。）
const WANT: usize = 5;

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
    ///
    /// **2026-09-05 起它只管一件事**：`depth <= 1` 那条提前 return
    /// （那一条是 `plan_depth_one_is_exactly_solve_turn` 的门，也是候选集
    /// 建不出来时的兜底）。根回合的候选排序归 [`Plan::cand_score`]，
    /// 深层每个回合选哪条线归 [`Plan::deep_score`]。
    pub score: fn(&State) -> i32,
    /// **根回合候选集的排序目标函数** —— [`solve_turn_topk`] 拿它剪枝。
    ///
    /// # 剪枝和评估同口径（阶段 3a）
    ///
    /// 候选集是按**单回合分数**剪的，剪完之后给每条候选打分的却是深层搜索
    /// （[`Plan::deep_score`] 选线、[`Plan::leaf`] 评估，这两者 2026-09-04
    /// 已经对齐成同一个 `score::leaf`）。2026-09-05 之前这里用的是 `cfg.score`
    /// （`damage_first`，hp 40 : 敌血 60），于是**筛子和尺子是两把** ——
    /// 被筛掉的线再也没机会被那把尺子量到。[`Plan::power_reserve`] 和
    /// [`Plan::damage_reserve`] 两条保底正是在给这个错打补丁：
    /// 能力线（当回合产出 0）和高伤线（吃擦伤换血量）各自被另一把尺子判低了。
    ///
    /// 判据是 `bin/plan_audit` 的读数 1「候选覆盖」：到边界最优的那条根线
    /// 在不在候选集里。**这一栏对目标函数和叶评估免疫** ——
    /// 不在集合里的线，评得再准也评不到。
    ///
    /// `cand_score = damage` 就是 2026-09-05 之前的行为，A/B 用它
    /// （`bin/rollout --alt "cand-score=damage"` / `bin/solve --cand-score damage`）。
    pub cand_score: fn(&State) -> i32,
    /// **深层（`depth >= 1`）每个回合选线用的目标函数。**
    ///
    /// # 为什么它和 [`Plan::score`] 必须分开
    ///
    /// expectimax 的规矩是：一个决策节点取"后继子树价值"的最大值。
    /// 最后一层的后继就是叶子，所以那一层的选线本该按**叶评估**取 argmax。
    /// 2026-09-04 之前这里用的是 `cfg.score`（默认 `damage_first`，hp 40 : 敌血 60），
    /// 而给它打分的是 `cfg.leaf`（`Weights::LEAF`，hp 100 : 敌血 75）——
    /// **选线和评分是两个函数**，最后一层的 argmax 取的是另一个函数的最大值。
    ///
    /// [实测] 同一候选全集下只换这一处，根建议在 **10/45** 个确定性起点上翻转；
    /// 83 个起点的 2476 次深层搜索里有 **417 次**因此换线。
    ///
    /// # 判据不可能写成"argmax 等于 argmax"
    ///
    /// 内层 `solve_turn` 取 argmax 的局面是 `threat.end_turn(·)`（**只挨攻击伤害**、
    /// 手牌已抽好），而父节点估值的局面是 `end_turn_before_draw(·)`（**真敌人回合**、
    /// 未抽牌）—— 不是同一个局面（`Threat` 装不下敌人加格挡 / 自增益 / 给我上
    /// debuff / 召唤 / 塞牌，内容表 204 手招里 103 手含这类 op）。
    /// 判据只能是实测式的：**内层选出的线，在"真敌人回合 + 叶评估"口径下
    /// 必须是这一回合全部不同终局里的最优**。
    /// [实测] 换成 `score::leaf` 之后，第 1 幕语料 11584 个深层孩子里残差 **0**；
    /// 第 2/3 幕 12822 个里残差 **335**（中位 1063 分 ≈ 10.6 血）—— 剩下那部分
    /// 就是上面那条 `Threat` 丢信息，不是这个参数能修的。
    ///
    /// **改它必须同时看 P4 截断率。** `score::leaf` 是 100 : 75，比 `damage_first`
    /// 的 40 : 60 更偏防守；而 `survive_first`（100 : 30）当跨回合策略时
    /// P4 当场 52/8320（推演一条都没死、就是不赢 —— 只挡不打）。
    /// **P4 不为 0 就整条退回。**
    ///
    /// `deep_score = score` 就是 2026-09-04 之前的行为，A/B 用它
    /// （`bin/rollout --alt "deep-score=damage"`）。
    pub deep_score: fn(&State) -> i32,
    /// **确定性窗口内**的目标函数（选线 + 叶评估两处）。
    ///
    /// # 它只和 [`Plan::deep_score`] 差一个常数，而那个常数值 1000 血
    ///
    /// [`Weights::WINDOW`](crate::solver::Weights::WINDOW) = [`Weights::LEAF`](crate::solver::Weights::LEAF)
    /// 但 `win = 0`。两把尺子**只在“赢下来的终局”上不同** —— 其余局面逐字相等。
    /// 而 `win = 100_000` 等于 1000 点血：窗口里只要一条线在边界之前把仗打完，
    /// 它就以 1000 血的优势压过所有没打完的线，**不管两边各剩多少血**。
    /// “仗有没有在窗口内结束”取决于窗口有多长，是个**地平线人造物**。
    ///
    /// # 为什么只在**窄根**上换
    ///
    /// 窄根（[`root_is_narrow`]）上确定性窗口真的会开，而且那条性质自己会往下传，
    /// 一路传到洗牌边界 —— 那一整段一个骰子都不掷，所以“到边界还剩多少血”
    /// 是**事实**；仗真的在那一段里打完时 `hp × 100` 就是这场仗的最终血量，
    /// 正是 P5 端到端量的那个数。根不窄时整次搜索基本靠采样，
    /// 拿掉 1000 血那道台阶会把一批候选变成**近似平局** ——
    /// 而近似平局在采样估值下就是单次可重复性塌掉的样子
    /// （`Leaf::Rollout` 那次 87% -> 71%）。**确定性才免疫那个失败模式。**
    ///
    /// # 尺子是**每次搜索一把**，不是逐叶子挑
    ///
    /// 挑法写在 [`Plan::at_root`]，那里也记着第一版“逐叶子挑”为什么是错的：
    /// 两把尺子只在“赢下来的终局”上不同，混着用会让**确定赢的那条线**
    /// 输给**采样分支里侥幸赢的那条线**，差 1000 血，偏置方向正好是反的。
    ///
    /// `window-score=leaf` **逐字节退回 2026-09-05 之前**，A/B 用它
    /// （`bin/rollout --alt "window-score=leaf"` / `bin/plan_audit --set …`）。
    pub window_score: fn(&State) -> i32,
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
    /// **确定性窗口**：这一层一个骰子都不掷时，**免费**再搜一层，最多几层。
    ///
    /// # 判据是「双重确定」，不是「单孩子」
    ///
    /// [`window_is_certain`]：机会节点确定（[`chance_is_certain`]）**且**每只
    /// 活敌人的下一手唯一。只看单孩子的话，场上带随机分支的敌人会被误判成确定，
    /// 那时往下深搜是在放大 `pick_next` 掷出来的一条轨迹。
    ///
    /// # 为什么它不占 `depth`（这是这个字段的全部内容）
    ///
    /// `depth` 花的钱是**分支宽度**：一层不确定的机会节点要 `widths[d]` 个孩子、
    /// 每个孩子一次 `solve_turn`，深度是**指数**的。确定层只有一个孩子，
    /// 多搜一层的代价是**一次** `solve_turn`，是**线性**的。
    /// 把两者记在同一笔账上，等于让最贵的层和最便宜的层花同一笔钱 ——
    /// 而牌序整堆已知时（`draw_pile_order` 补丁之后 `sync` 出来的局面都是），
    /// 洗牌之前那一整段本来就是有限确定性博弈，那几层是白送的。
    ///
    /// 所以确定层**不扣 `left`**，扣这一笔独立的预算。计划深度留给真的会分岔的层。
    ///
    /// # 三笔账互不相干
    ///
    /// `left` = 计划好的（会分岔的）深度 · `ext` = 「话说到一半」时借的层
    /// （[`want_extension`]）· `window` = 确定层。三笔各扣各的，
    /// [`HARD_DEPTH_CAP`] 那道闸原样保留兜底。
    ///
    /// **0 = 关掉，行为和 2026-09-05 之前逐字节相同**（实测：`--set "window=off"`
    /// 跑出来的 P1/P3/P4 和 9-04 的基线逐个数相同），A/B 用它：
    /// `bin/rollout --alt "window=off"` / `bin/solve --window 0`。
    pub window: u8,
    /// **窄根上把候选集的 K 开到这个数**（[`root_is_narrow`]）。0 = 关掉，
    /// 一律用 [`Plan::k`]。
    ///
    /// # 为什么可以只在窄根上开大
    ///
    /// K 花的钱是**每条候选一次深层求值**，而深层求值的代价由机会节点的
    /// 宽度定：牌序整堆已知时 [`chance_children`] 返回单个 `p = 1.0` 的孩子，
    /// 一条候选往下就是**每层一次 `solve_turn`**（线性）；不确定时是
    /// `widths[d]` 个孩子（指数）。**同一个 K 在两种根上贵得完全不是一个量级**，
    /// 所以它该是自适应的，不是一个全局常数。
    ///
    /// 敌人的随机分支**不进这个判据**：`node_value_window` 只按机会节点的
    /// 孩子铺开，敌人那一手是 `pick_next` 掷出来的一条轨迹，不产生分支。
    /// 那一半是[窗口能不能借层](Plan::window)的判据，不是代价的判据 ——
    /// 两个问题，两条判据，别混。
    ///
    /// # 64 不是拍的
    ///
    /// `bin/plan_audit` 量到根回合平均 **1059** 个不同终局，而窗口长度分布是
    /// `[1,22,15,4,3,0,0,0]`（最长 4 层）。64 条候选 × 4 层 × 一次 `solve_turn`
    /// 是几百次单回合搜索，秒级；全枚举（K = 100000）在同一批起点上也跑得动，
    /// 是这个旋钮的上界。判据是读数 1「候选覆盖」，代价看 `--set` 单独跑的耗时。
    pub k_certain: usize,
    /// 置换表的对数大小（跨候选 memo）。**0 = 不建表**，行为和 2026-09-05
    /// 之前逐字节相同。
    ///
    /// 表是**每次 [`plan_report`] 新建一张**、几条候选求值线程共享
    /// （见 [`Tt`] 的无锁实现）。不跨调用保留是因为 `plan_line` 是个自由函数，
    /// 而实战驱动每回合是一个新进程 —— 真要跨回合留表，那是驱动方的事。
    ///
    /// **命中率是它唯一的证据**：[`PlanReport::tt_hits`] / `tt_probes`。
    /// key 写错的样子就是命中率掉到 0，而那**不会让任何东西变红**。
    pub tt_bits: u8,
}

impl Default for Plan {
    fn default() -> Plan {
        Plan {
            depth: 1,
            widths: [96, 12, 4, 4],
            exact_threshold: 128,
            budgets: [200_000, 2_000, 500, 500],
            score: score::damage_first,
            // 候选排序、深层选线、叶评估**三处同口径**，见 `Plan::cand_score`。
            cand_score: score::leaf,
            deep_score: score::leaf,
            // 窗口内换成最终血量口径（`win = 0`），见 `Plan::window_score`。
            window_score: score::window,
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
            window: WINDOW_CAP,
            k_certain: K_CERTAIN,
            tt_bits: TT_BITS,
        }
    }
}

impl Plan {
    /// 第 `d` 层机会节点采几个样本。**确定层用不上它**（只有一个孩子）。
    pub fn width(&self, d: usize) -> usize {
        self.widths[d.min(self.widths.len() - 1)] as usize
    }
    /// 这个根留几条候选：窄根用 [`Plan::k_certain`]，其余用 [`Plan::k`]。
    ///
    /// **只有这一处定义** —— `plan_report` / `plan_candidates` / `bin/plan_audit`
    /// 三个地方各写一遍的话，审计台量的就不是 planner 真的在用的那个候选集了。
    pub fn k_at(&self, s: &State) -> usize {
        if self.k_certain > self.k && root_is_narrow(s) {
            self.k_certain
        } else {
            self.k
        }
    }
    /// 这一叶子该用哪把尺子：在[确定性窗口](Plan::window_score)里就用窗口那把。
    ///
    /// **只换 [`Leaf::Eval`] 那一半。** [`Leaf::Rollout`] 原样不动 —— 它从叶子出发
    /// 拿 [`Policy::Fast`] 推好几个回合，**那几个回合早就跑出窗口了**，
    /// 它拿回来的不是事实而是一批采样。把窗口那把尺子接到一个采样估值上，
    /// 正好是 [`Plan::window_score`] 里点名要防的那个失败模式。
    pub(crate) fn leaf_at(&self, in_window: bool) -> Leaf {
        match self.leaf {
            Leaf::Eval(_) if in_window => Leaf::Eval(self.window_score),
            other => other,
        }
    }
    /// **这个根该用哪把尺子。** 窄根（[`root_is_narrow`]）⇒ 深层选线和叶评估
    /// 一起换成 [`Plan::window_score`]；其余原样返回。
    ///
    /// # 为什么是「每次搜索一把」，不是「逐叶子挑」
    ///
    /// 第一版写的是逐叶子挑：从根一路确定到这个叶子才用窗口那把尺子。
    /// **那样根上的候选就没法互相比了** —— 两把尺子只在「赢下来的终局」上不同，
    /// 于是会出现这么一幕：候选 A 这一回合就把人打死（确定，用窗口尺子，`win = 0`），
    /// 候选 B 没打死、但它窗口外的某个**采样**分支赢了（用叶尺子，`win = 100000`）。
    /// A 的**事实**输给 B 的**运气**，而且差 1000 血。
    /// **偏置的方向正好是反的**：越确定的那条线越吃亏。
    ///
    /// 所以尺子必须在入口处一次定死。判据取 [`root_is_narrow`]（`n_draw_known == n_draw`）：
    /// 它正是「确定性窗口真的会开」的那种根，而且这条性质**自己会往下传** ——
    /// 打牌只把牌挪去弃牌堆/消耗堆，抽走的从已知前缀里出，一路传到洗牌边界
    /// （和 [`Plan::k_certain`] 用的是同一条判据、同一个理由）。
    ///
    /// **敌人的随机分支不进这个判据**，和 `k_certain` 那条一样：它问的不是
    /// "这一层是不是事实"（那是 [`window_is_certain`] 的活，管的是搜多深），
    /// 而是"这次搜索整体处在确定性的那个区间里吗"。窄根上敌人掷骰只会让
    /// **窗口**提前关掉、退回采样，尺子仍然是一次搜索一把 —— 一致性不受影响。
    ///
    /// [`Plan::cand_score`] **故意不换**：换了 `plan_audit` 的读数 1（候选覆盖）
    /// 就不再对目标函数免疫，而那一栏的全部价值就在于它免疫。
    pub fn at_root(&self, s: &State) -> Plan {
        if !root_is_narrow(s) {
            return *self;
        }
        Plan { deep_score: self.window_score, leaf: self.leaf_at(true), ..*self }
    }
    /// 第 `d` 层单回合搜索的节点预算。
    ///
    /// **按 `depth` 索引，确定性窗口不改这条**：窗口层落在 `depth >= 2`，
    /// 拿的就是 500。
    ///
    /// **"那是不是该给多一点"已经量过了，答案是不用**（2026-09-06，原阶段 3.5）：
    /// 把深层预算 ×16 是"自适应预算"的**上界**（自适应只让同一个效果更便宜），
    /// 端到端 **+0.1 血**（P5 2256 对，104/66），可重复性 228/334 -> 231/334。
    /// 而同一天的 `plan_audit --inner` 说 **6.3% 的深层选线确实是被这个预算搞错的**
    /// （读数 2 的 A→B，2078/32831）—— **机制那头更严重、端到端不动**。
    /// 要改这个数之前先读 `docs/verification-log.md` 的 09-06「阶段 3.5」那一节。
    pub fn budget(&self, d: usize) -> u32 {
        self.budgets[d.min(self.budgets.len() - 1)]
    }

    /// 把一串 `"k=v,k=v"` **覆盖**到一个已经建好的配置上。
    ///
    /// # 为什么是"覆盖"而不是"重建"
    ///
    /// `bin/rollout` 的 P5 是配对比较（CRN）：两条 arm 看到的是同一手牌、
    /// 同一条敌人随机流，差出来的只该是**点名改掉的那几个键**。
    /// 从 `Plan::default()` 重建的话，主策略上所有 `--ext` / `--leaf` /
    /// `--power-reserve` 都会被悄悄还原，于是差出来的是"这些键的合计"，
    /// 而标题写的是别的。
    ///
    /// **不认识的键报错，不静默忽略**：写错一个键名而它默默不生效，
    /// 得到的是"两条 arm 逐字相同"这种看着很正常的读数。
    ///
    /// **住在库里而不是某个 `bin/` 里**，因为三个验收台都要用同一份：
    /// `bin/rollout --alt/--set`、`bin/plan_audit --set`、`bin/solve`。
    /// 各写一遍的话，"同一个 `--set` 字符串"在三条验收里指的就不是同一个配置。
    pub fn apply(&mut self, spec: &str) -> Result<(), String> {
        let pick = |v: &str| -> Result<fn(&State) -> i32, String> {
            Ok(match v {
                "survive" => score::survive_first,
                "damage" => score::damage_first,
                "hp" => score::hp_only,
                "leaf" => score::leaf,
                "window" => score::window,
                _ => {
                    return Err(format!(
                        "目标函数只认 survive/damage/hp/leaf/window，收到 {v:?}"
                    ))
                }
            })
        };
        for item in spec.split(',') {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            let (k, v) = item.split_once('=').ok_or_else(|| format!("{item:?} 不是 key=value"))?;
            let num = |t: &str| -> Result<u64, String> {
                v.parse::<u64>().map_err(|_| format!("{t} 要一个数字，收到 {v:?}"))
            };
            let four = |t: &str| -> Result<[u32; 4], String> {
                let parts: Vec<&str> = v.split(':').collect();
                if parts.len() != 4 {
                    return Err(format!("{t} 要 4 个用 : 分隔的数，收到 {v:?}"));
                }
                let mut out = [0u32; 4];
                for (i, p) in parts.iter().enumerate() {
                    out[i] = p.parse().map_err(|_| format!("{p:?} 不是数字"))?;
                }
                Ok(out)
            };
            match k.trim() {
                "depth" => self.depth = num("depth")?.max(1) as u8,
                "score" => self.score = pick(v)?,
                // 阶段 3a：候选排序的目标函数。`damage` 退回 2026-09-05 之前。
                "cand-score" => self.cand_score = pick(v)?,
                "deep-score" => self.deep_score = pick(v)?,
                // 阶段 4：窗口内的目标函数。`leaf` 退回 2026-09-05 之前。
                "window-score" => self.window_score = pick(v)?,
                "k" => self.k = num("k")? as usize,
                // 阶段 3b：窄根上的 K。`off`/`0` 退回"一律用 k"。
                "k-certain" => {
                    self.k_certain = match v {
                        "on" => K_CERTAIN,
                        "off" => 0,
                        "all" => usize::MAX / 2,
                        _ => num("k-certain")? as usize,
                    }
                }
                // 阶段 3c：置换表。`off`/`0` = 不建表。
                "tt" => {
                    self.tt_bits = match v {
                        "on" => TT_BITS,
                        "off" => 0,
                        _ => num("tt")? as u8,
                    }
                }
                "power-reserve" => self.power_reserve = num("power-reserve")? as usize,
                "damage-reserve" => self.damage_reserve = num("damage-reserve")? as usize,
                "exact-threshold" => self.exact_threshold = num("exact-threshold")? as usize,
                "ext-width" => self.ext_width = num("ext-width")? as u16,
                // 确定性窗口。`off`/`0` = 2026-09-05 之前的行为，A/B 用它。
                "window" => {
                    self.window = match v {
                        "on" => WINDOW_CAP,
                        "off" => 0,
                        _ => num("window")? as u8,
                    }
                }
                "seed" => self.seed = num("seed")?,
                "budgets" => self.budgets = four("budgets")?,
                "widths" => {
                    let b = four("widths")?;
                    self.widths = [b[0] as u16, b[1] as u16, b[2] as u16, b[3] as u16];
                }
                // 逗号被外层用掉了，这里用 `+` 分隔
                "ext" => {
                    let on = |x: &str| v.split('+').any(|t| t.trim() == x);
                    self.ext_lethal = on("lethal");
                    self.ext_spike = on("spike");
                    self.ext_boundary = on("boundary");
                }
                "leaf" => {
                    let mut p = v.split(':');
                    match p.next() {
                        Some("eval") => self.leaf = Leaf::Eval(score::leaf),
                        Some("rollout") => {
                            let samples = p.next().and_then(|x| x.parse().ok()).unwrap_or(8u16);
                            let turns = p.next().and_then(|x| x.parse().ok()).unwrap_or(12u16);
                            self.leaf = Leaf::Rollout { samples, turns, tail: score::leaf };
                        }
                        _ => return Err(format!("leaf 只认 eval 或 rollout[:n:t]，收到 {v:?}")),
                    }
                }
                other => {
                    return Err(format!(
                        "不认识的键 {other:?}。可用：depth score cand-score deep-score k \
                         k-certain tt power-reserve damage-reserve exact-threshold ext-width \
                         window window-score seed budgets widths ext leaf"
                    ))
                }
            }
        }
        Ok(())
    }

    /// 一行说明。**主策略和对照策略共用这一份** —— 各印各的话，A/B 的两行
    /// 会用不同的字段描述同一个东西，读的人就得自己猜"没印出来的那些一不一样"。
    ///
    /// **三个目标函数都要印出来**：`cand_score`（候选排序，阶段 3a）、
    /// `deep_score`（深层选线，2026-09-04）、叶评估 —— 它们各自不同这件事
    /// 正是那两轮改动的全部内容。
    pub fn describe(&self) -> String {
        // **函数指针要先显式转成 `fn(...)` 再比地址** —— 直接拿函数项转 usize
        // 是零大小类型的地址，编译器会警告，而且比出来的东西没有意义。
        let addr = |f: fn(&State) -> i32| f as usize;
        let sc = |f: fn(&State) -> i32| -> &'static str {
            let known: [(fn(&State) -> i32, &'static str); 5] = [
                (score::survive_first, "survive"),
                (score::damage_first, "damage"),
                (score::hp_only, "hp"),
                (score::leaf, "leaf"),
                (score::window, "window"),
            ];
            known.iter().find(|(g, _)| addr(*g) == addr(f)).map(|(_, n)| *n).unwrap_or("?")
        };
        format!(
            "plan（限深 expectimax，D={}，确定性窗口 {}，K={}+{}能力+{}伤害（窄根 {}），\
             置换表 {}，宽度 {:?}，预算 {:?}，线目标 {}，候选目标 {}，深层目标 {}，窗口目标 {}，叶 {}）",
            self.depth,
            if self.window == 0 { "关".to_string() } else { format!("{} 层", self.window) },
            self.k,
            self.power_reserve,
            self.damage_reserve,
            if self.k_certain > self.k { format!("K={}", self.k_certain) } else { "关".into() },
            if self.tt_bits == 0 { "关".to_string() } else { format!("2^{} 格", self.tt_bits) },
            &self.widths[..(self.depth as usize).min(self.widths.len())],
            &self.budgets[..(self.depth as usize).min(self.budgets.len())],
            sc(self.score),
            sc(self.cand_score),
            sc(self.deep_score),
            sc(self.window_score),
            match self.leaf {
                Leaf::Eval(_) => "静态".to_string(),
                Leaf::Rollout { samples, turns, .. } =>
                    format!("推演 {samples}x{turns}回合"),
            }
        )
    }
}

// ---------------------------------------------------------------------------
// 置换表
// ---------------------------------------------------------------------------

/// 置换表的一格：**两个 `AtomicU64`，异或校验配对**。
///
/// `k` 里存的是 `key ^ data`，`d` 里存 `data`。读回来异或一下等于 `key` 才算数 ——
/// 撕裂的读（一半是新写的、一半还是旧的）异或不回去，自动降级成 miss。
/// 象棋引擎那套 lockless hashing，代价是每格多存 8 字节。
#[derive(Default)]
struct TtSlot {
    k: std::sync::atomic::AtomicU64,
    d: std::sync::atomic::AtomicU64,
}

/// 一格的载荷打包成一个 u64：`value`（低 32）· `depth`（32..40）· `age`（40..56）。
fn pack(depth: u8, age: u16, value: i32) -> u64 {
    (value as u32 as u64) | ((depth as u64) << 32) | ((age as u64) << 40)
}
fn unpack(d: u64) -> (u8, u16, i32) {
    ((d >> 32) as u8, (d >> 40) as u16, d as u32 as i32)
}

/// 定长置换表。**不用 `HashMap`**：这一层要能被 `State` 一样的纪律约束
/// （固定容量、无分配），而且直接寻址比哈希表快。
///
/// 冲突就直接覆盖（always-replace），但**优先覆盖 age 更旧的那个** ——
/// 深搜的值比浅搜的值贵，同一轮里不该被随手挤掉。
///
/// # 为什么是原子的，不是 `Mutex<Tt>`
///
/// 根候选是**并行**求值的（[`eval_candidates`] 按线程数分块），而
/// 「跨候选 memo」的全部价值就在于那几个线程看的是同一张表。加锁的话每次
/// probe 都要过一次锁，而 probe 的正常代价只有一次访存。
///
/// 替换那一步是「读旧的、判一下、写新的」，**并发下这中间可以插进别人的写**：
/// 最坏结果是一条本该留下的深值被浅值顶掉。那是**性能**上的损失，不是正确性 ——
/// 每一格自己是自洽的（异或校验），而 `probe` 还要再判一次 `depth >= need`。
pub struct Tt {
    slots: Vec<TtSlot>,
    mask: usize,
    age: std::sync::atomic::AtomicU32,
    hits: std::sync::atomic::AtomicU64,
    probes: std::sync::atomic::AtomicU64,
}

impl Tt {
    /// `bits` = 表的对数大小（16 -> 65536 格 -> 1 MB）。
    pub fn new(bits: u32) -> Tt {
        let n = 1usize << bits;
        let mut slots = Vec::with_capacity(n);
        slots.resize_with(n, TtSlot::default);
        Tt {
            slots,
            mask: n - 1,
            age: std::sync::atomic::AtomicU32::new(1),
            hits: std::sync::atomic::AtomicU64::new(0),
            probes: std::sync::atomic::AtomicU64::new(0),
        }
    }

    fn age(&self) -> u16 {
        self.age.load(std::sync::atomic::Ordering::Relaxed) as u16
    }

    /// 新一轮搜索。**不清表** —— 跨轮保留正是置换表的价值所在。
    pub fn bump_age(&self) {
        self.age.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn hits(&self) -> u64 {
        self.hits.load(std::sync::atomic::Ordering::Relaxed)
    }
    pub fn probes(&self) -> u64 {
        self.probes.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn probe(&self, key: u64, need_depth: u8) -> Option<i32> {
        use std::sync::atomic::Ordering::Relaxed;
        self.probes.fetch_add(1, Relaxed);
        let slot = &self.slots[(key as usize) & self.mask];
        let k = slot.k.load(Relaxed);
        let d = slot.d.load(Relaxed);
        if k ^ d != key {
            return None;
        }
        let (depth, _age, value) = unpack(d);
        if depth >= need_depth {
            self.hits.fetch_add(1, Relaxed);
            return Some(value);
        }
        None
    }

    pub fn store(&self, key: u64, depth: u8, value: i32) {
        use std::sync::atomic::Ordering::Relaxed;
        let slot = &self.slots[(key as usize) & self.mask];
        let old_d = slot.d.load(Relaxed);
        let age = self.age();
        // 同一轮里，深的压浅的；跨轮无条件覆盖。
        // 空格的 `d` 是 0 ⇒ 解出来 `age = 0`，而 `age` 从 1 起 —— 空格必然被写。
        let (od, oa, _) = unpack(old_d);
        if oa == age && od > depth {
            return;
        }
        let d = pack(depth, age, value);
        // **先写校验位再写载荷**：反过来的话，一个读者可能看到"新载荷 + 旧校验"
        // 而它们恰好异或成某个别的 key。两个顺序都可能被读到撕裂的一半，
        // 但异或校验只在**配对**时才通过，所以两种顺序都安全 —— 写死一个是为了
        // 别让人以为这里有余地。
        slot.k.store(key ^ d, Relaxed);
        slot.d.store(d, Relaxed);
    }
}

/// **跨回合 memo 的局面指纹。** `solver::key` 加上它省掉的那几样。
///
/// # 为什么不能直接用 `solver::key`
///
/// `solver::key` 是给**单回合**搜索去重用的，它省掉的正是回合内不会变、
/// 跨回合会变的东西：
///
/// | 省掉的 | 谁会读它 |
/// |---|---|
/// | `n_draw_known` | [`chance_children`]：抽牌堆顶那几张确不确定 |
/// | `enemy_hist` / `enemy_used` | 出招机器：25 条 `NotTwice` / 3 条 `Once` / 3 条 `AtMost` / 3 条冷却，另有 `Next::Cond` 直接读 `used` |
/// | `rng.shuffle` / `enemy` / `gen` | 洗牌 · `Next::Rand` 掷骰 · 生成牌 |
/// | 消耗堆的**内容**（只记了 `n_exh`）| 从消耗堆取牌的机制 |
/// | `turn` / `player_dead` | 回合数条件；死了没有 |
///
/// 回合内这些一个都不变，所以 `solver::key` 是对的，而且**不该为跨回合多花钱**
/// （它每个搜索节点都要算一次，实测 137 ns 那一版占了节点开销的四成）。
/// 拿它去做跨回合 memo 则是一个**静默错误**：两个只差"这场用过哪几手"的局面
/// 会被并进同一格，而它们的未来完全不同 —— 而这套验收的四种对拍模式
/// 全都看不见这种错。
///
/// 随机流三条都在里面。省掉它们等于宣称"这两个局面的骰子结果一样"，那是欠定的；
/// 而**确定性窗口里它们一个都不掷**（`pick_next` 只在 `Next::Rand` 上读
/// `rng.enemy`），所以真该命中的地方一次也没多花。
pub fn key(s: &State) -> u64 {
    let mut h = crate::solver::key(s);
    let mut mix = |x: u64| h = crate::solver::z(h ^ x);
    mix(s.n_draw_known as u64 ^ 0xD4A0_0000_0000_0001);
    for e in 0..s.n_enemies as usize {
        for (i, m) in s.enemy_hist[e].iter().enumerate() {
            mix(((e as u64) << 40) ^ ((i as u64) << 32) ^ (*m as u64) ^ 0xE151_0000_0000_0002);
        }
        mix(((e as u64) << 40) ^ (s.enemy_used[e] as u64) ^ 0x0DED_0000_0000_0003);
    }
    mix(s.rng.shuffle ^ 0x5F0F_0000_0000_0004);
    mix(s.rng.enemy ^ 0xE0E0_0000_0000_0005);
    mix(s.rng.gen ^ 0x9E11_0000_0000_0006);
    // 消耗堆按**多重集**（顺序无关，和 `solver::key` 对手牌/弃牌堆同一个套路）
    let mut bag: u64 = 0;
    for i in 0..s.n_exh as usize {
        bag = bag.wrapping_add(crate::solver::z(
            crate::solver::card_ident(s, s.exh[i]) ^ 0xE314_A057_E314_A057,
        ));
    }
    mix(bag);
    mix(s.turn as u64 ^ ((s.player_dead as u64) << 32) ^ 0x7A11_0000_0000_0007);
    h
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

/// 这个机会节点**真的确定**吗 —— [`chance_children`] 会不会返回单个 `p = 1.0`
/// 的孩子。两种情况：
///
/// 1. **前 5 张全是已知前缀**（`n_draw_known` 盖满一手）：身份全明确，
///    头槌那类放到顶上的牌本来就不该当随机。
/// 2. **抽牌堆凑不齐一手、而弃牌堆是空的**：剩下几张全抽走，没别的可能。
///
/// # 它比「孩子只有一个」窄一点，这是**故意的**
///
/// 枚举分支在退化情况下也会给出单个 `p = 1.0` 的孩子（未知区里所有牌
/// 身份相同，比如整堆都是打击）。那种"确定"只确定到 [`ident`] 那个口径为止，
/// 而 `ident` 今天**不含附魔的 `ench` / `ench_amt`** —— 带灵巧的防御和普通防御
/// 会被并成一组（关键字那半跟着 `flags` 进来了，数值那半没有）。
/// 拿一个已知欠定的身份口径去支撑"这一层是事实"，方向是乐观的，
/// 所以这里**不认**它，代价只是少借几层深度。
pub fn chance_is_certain(s: &State) -> bool {
    let n = s.n_draw as usize;
    let known = (s.n_draw_known as usize).min(n).min(WANT);
    known == WANT || (n < WANT && s.n_disc == 0)
}

/// 每只**活着的**敌人下一手都唯一吗。
///
/// 读的是 `step::allowed_next` —— 内核里现成的、`verify --predict-enemy` 判
/// 成员资格用的那个集合，不另猜一个。它答的正是「这一层结束时 `advance_move`
/// 会把 `enemy_move` 推到哪」：`advance_move` 在敌人打完这一手之后调 `pick_next`，
/// 而 `pick_next` 和 `allowed_next` 读同一份 `branch_open`。
/// **集合是单元素 ⇒ 这一层一个骰子都不掷。**
///
/// 两条口径要写明：
///
/// * **只看活着的**，和 `enemy_turn` 那个循环一致：`!alive()` 的敌人这一手
///   不行动、也不 `advance_move`，不构成分支。（带适生力的敌人 0 血还在场上、
///   战斗也不结束，见「还在场上不等于活着」那三处口径。）
/// * **`Next::Cond` 的单元素集合仍然算确定**：`pick_next` 对条件分支不掷骰
///   （取集合最低位）。条件是在这一层**开始**时求值的，玩家打完这一回合之后
///   真正选中的那一手可能是另一个 —— 但那仍然是状态的确定函数，
///   **没有第二条轨迹**。判不出条件时 `resolve_set` 给的是多位掩码，
///   那时这里返回 `false`，方向是保守的。
fn enemy_next_is_certain(s: &State) -> bool {
    (0..s.n_enemies as usize)
        .filter(|&e| s.enemies[e].alive())
        .all(|e| crate::step::allowed_next(s, e).count_ones() == 1)
}

/// 这一层**一个骰子都不掷**吗：机会节点确定 **且** 每只活敌人的下一手唯一。
///
/// 这是确定性窗口的判据，见 [`Plan::window`]。**两半缺一不可** ——
/// 只看单孩子的话，场上带随机分支的敌人会被误判成确定：那时我们跟着的
/// 只是 `pick_next` 掷出来的一条轨迹，把它当事实往下深搜是在放大一个样本。
///
/// **这批局面不是假想的**：`bin/plan_audit` 会报「牌序整堆已知、但场上有
/// 带随机分支的敌人」的起点数（`dropped_random`）—— 抽牌那一半定死了、
/// 敌人那一半要掷骰，正是只看单孩子会漏判的那一类。
pub fn window_is_certain(s: &State) -> bool {
    chance_is_certain(s) && enemy_next_is_certain(s)
}

/// 这个**根**往下的机会节点是不是一路单孩子 —— [`Plan::k_at`] 靠它决定
/// 候选集开多大。
///
/// 判据只有抽牌那一半，而且要的是**整堆已知**（`n_draw_known == n_draw`），
/// 不是 [`chance_is_certain`] 的"够盖住一手就行"：
///
/// * **整堆已知才递推得下去。** 打完这一回合，牌只会从抽牌堆流向弃牌堆/消耗堆
///   （抽走的从已知前缀里出），所以"整堆已知"这条性质自己会往下传，
///   一路传到洗牌边界。只已知 5 张的话，根这一手随便抽两张就破了。
/// * **敌人的随机分支不进这个判据。** `node_value_window` 只按机会节点的孩子
///   铺开，敌人那一手是 `pick_next` 掷出来的**一条**轨迹，一个分支都不产生。
///   它影响的是[窗口能不能借层](Plan::window)（那里判据是双重确定），
///   不影响这里问的"一条候选往下要花多少钱"。**两个问题，两条判据。**
///
/// 它比 `bin/plan_audit` 筛确定性起点那两条（整堆已知 + 机器里没有 `Next::Rand`）
/// **宽**，宽出来的正好是"抽牌定死了、敌人要掷骰"那一类 —— 审计台自己量到
/// 14 个。那批局面里 K 开大照样便宜，只是"最优"没有定义、进不了那三个读数。
pub fn root_is_narrow(s: &State) -> bool {
    (s.n_draw as usize) >= WANT && s.n_draw_known == s.n_draw
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
    let n = s.n_draw as usize;
    let known = (s.n_draw_known as usize).min(n).min(WANT);
    let r = WANT - known;

    // **1. 真的确定的那两种情况 ⇒ 单个 `p = 1.0` 的孩子。**
    // 判据收口在 [`chance_is_certain`]（确定性窗口也读同一份，见 [`Plan::window`]）——
    // "这个节点确不确定"两处各写一遍，迟早有一边漏掉一种情况，
    // 而那种错**不报错，只是搜错**。
    if chance_is_certain(s) {
        let mut st = *s;
        crate::step::open_hand(&mut st);
        return vec![Draw { state: st, p: 1.0 }];
    }

    // **2. 抽牌堆凑不齐一手、而弃牌堆非空 ⇒ 要洗弃牌堆。**
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
    // 那种情况上面那条 `chance_is_certain` 已经接走了 —— 采 12 个一模一样的
    // 样本只是白花 12 倍的钱。
    if n < WANT {
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
    plan_report(s, cfg, threat).line
}

/// 一次跨回合搜索的**账**。[`plan_report`] 带出来，不参与任何决策。
///
/// 它存在的理由和 [`PlanStat`] 一样：候选生成那三个旋钮
/// （[`Plan::cand_score`] / [`Plan::k_certain`] / [`Plan::tt_bits`]）
/// **坏掉的样子都不会报错** —— K 没开大只是少几条候选，置换表的 key 写错
/// 只是命中率掉到 0。两者都要有一个看得见的数，否则"开了"和"没开"长得一样。
#[derive(Clone, Copy, Debug)]
pub struct PlanReport {
    pub line: Line,
    pub stat: PlanStat,
    /// 候选集实际有几条（含两条保底捞回来的）
    pub n_cands: usize,
    /// 这一次用的 K，见 [`Plan::k_at`]
    pub k_used: usize,
    /// 这个根算不算**窄根**，见 [`root_is_narrow`]
    pub narrow: bool,
    pub tt_probes: u64,
    pub tt_hits: u64,
}

/// 同 [`plan_line_with_threat`]，但把这次搜索的账一起带出来。
pub fn plan_report(s: &State, cfg: &Plan, threat: &Threat) -> PlanReport {
    let narrow = root_is_narrow(s);
    let k_used = cfg.k_at(s);
    // **阶段 4：窗口内的目标函数**。窄根上深层选线和叶评估一起换成
    // [`Plan::window_score`]（最终血量口径，`win = 0`）。一次搜索一把尺子，
    // 理由见 [`Plan::at_root`]。`cfg.score`（D=1 那条提前 return）和
    // `cfg.cand_score`（候选生成）都不动。
    let windowed = cfg.at_root(s);
    let cfg = &windowed;
    let bare = |line: Line| PlanReport {
        line,
        stat: PlanStat::default(),
        n_cands: 0,
        k_used,
        narrow,
        tt_probes: 0,
        tt_hits: 0,
    };
    if cfg.depth <= 1 {
        return bare(solve_turn_potions(s, threat, cfg.score, cfg.budget(0), 0).line);
    }
    // **候选排序用 `cand_score`，不是 `cfg.score`** —— 筛子和尺子同口径，
    // 理由和实测读数见 [`Plan::cand_score`]。
    let cands = solve_turn_topk(
        s,
        threat,
        cfg.cand_score,
        cfg.budget(0),
        0,
        k_used,
        cfg.power_reserve,
        cfg.damage_reserve,
    );
    if cands.is_empty() {
        return bare(solve_turn_potions(s, threat, cfg.score, cfg.budget(0), 0).line);
    }
    let tt = (cfg.tt_bits > 0).then(|| Tt::new(cfg.tt_bits as u32));
    let (vals, stat) = eval_candidates(s, &cands, cfg, tt.as_ref());
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
    PlanReport {
        line: out,
        stat,
        n_cands: cands.len(),
        k_used,
        narrow,
        tt_probes: tt.as_ref().map(|t| t.probes()).unwrap_or(0),
        tt_hits: tt.as_ref().map(|t| t.hits()).unwrap_or(0),
    }
}

/// 逐条候选求深层估值。**按线程数分块，不是一条候选一个线程。**
///
/// 一条一个线程在 K=6 时无所谓，[`Plan::k_certain`] 把 K 开到 64 之后就是
/// 64 个线程抢几个核 —— 而窄根下每条候选只花几次 `solve_turn`，
/// 线程创建比求值本身还贵。分块之后线程数是**机器的**，不是 K 的。
///
/// 置换表按 `&Tt` 共享（内部原子，见 [`Tt`]）：跨候选 memo 的全部价值
/// 就在于这几个线程看的是同一张表。
fn eval_candidates(
    s: &State,
    cands: &[Line],
    cfg: &Plan,
    tt: Option<&Tt>,
) -> (Vec<i32>, PlanStat) {
    if cands.is_empty() {
        return (Vec::new(), PlanStat::default());
    }
    let nthreads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(cands.len())
        .max(1);
    let chunk = cands.len().div_ceil(nthreads);
    std::thread::scope(|sc| {
        let hs: Vec<_> = cands
            .chunks(chunk)
            .map(|part| {
                sc.spawn(move || {
                    let mut st = PlanStat::default();
                    let v: Vec<i32> =
                        part.iter().map(|c| line_value(s, c, cfg, tt, &mut st)).collect();
                    (v, st)
                })
            })
            .collect();
        let mut vals = Vec::with_capacity(cands.len());
        let mut stat = PlanStat::default();
        for h in hs {
            let (v, st) = h.join().expect("候选线求值不该 panic");
            vals.extend(v);
            stat.merge(&st);
        }
        (vals, stat)
    })
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
    // 尺子也和 `plan_report` 同一把（阶段 4，见 [`Plan::at_root`])——
    // 诊断报的分数必须就是 planner 排序用的那个分数。
    let windowed = cfg.at_root(s);
    let cfg = &windowed;
    // **和 `plan_report` 逐字同一套参数**（`cand_score` + `k_at`）——
    // 两边不一致的话，这个诊断报的就不是 planner 真的在看的那个候选集，
    // 而它存在的全部理由就是回答"那条线在不在候选集里"。
    let (main, rescued_p, rescued_d) = solve_turn_topk_split(
        s,
        threat,
        cfg.cand_score,
        cfg.budget(0),
        0,
        cfg.k_at(s),
        cfg.power_reserve,
        cfg.damage_reserve,
    );
    let n_main = main.len();
    let all: Vec<Line> = main.into_iter().chain(rescued_p).chain(rescued_d).collect();
    let tt = (cfg.tt_bits > 0).then(|| Tt::new(cfg.tt_bits as u32));
    let (vals, _) = eval_candidates(s, &all, cfg, tt.as_ref());
    (all.into_iter().zip(vals).collect(), n_main)
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
fn line_value(
    s: &State,
    line: &Line,
    cfg: &Plan,
    tt: Option<&Tt>,
    stat: &mut PlanStat,
) -> i32 {
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
    node_value_window(&s1, cfg, cfg.depth - 1, 1, MAX_EXTEND, cfg.window, tt, stat)
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

/// **确定性窗口最多免费借几层**（[`Plan::window`] 的默认值）。
///
/// 6 不是猜的：它是 `bin/plan_audit` 那把尺子（C 列「到边界」）用的同一个数，
/// 两边取同一个值，A2 和 C 之间差出来的就**只有预算**，不含"谁搜得更深"。
/// 实测的窗口长度分布是 `[1,22,15,4,3,0,0,0]`（下标 = 层数，45 个确定性起点）——
/// **最长 4 层**，所以 6 是个够不着的兜底，不是一个会咬人的旋钮。
///
/// 深度总账：`depth + MAX_EXTEND + WINDOW_CAP` = 2+2+6 = 10 < [`HARD_DEPTH_CAP`]。
pub const WINDOW_CAP: u8 = 6;

/// **窄根上候选集的 K**（[`Plan::k_certain`] 的默认值）。
///
/// 6（[`Plan::k`]）是按"每条候选乘 `widths[1]` = 12 次深层搜索"定的价；
/// 窄根上那个乘数是 1，所以同一个 K 在这里便宜一个数量级。
/// 64 的选法和代价见 [`Plan::k_certain`]。
pub const K_CERTAIN: usize = 64;

/// **置换表的默认大小**（[`Plan::tt_bits`]），16 -> 65536 格 -> 1 MB。
///
/// 表是每次搜索新建的，所以这个数同时是"每回合白花多少次 memset"。
/// 往大了设不会更准，只会更慢 —— 一次搜索存进去的条目数是**几百**量级
/// （几十条候选 × 几层），65536 格已经空得几乎不冲突。
pub const TT_BITS: u8 = 16;


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
            for (oi, op) in def.moves[m].ops.iter().enumerate() {
                // 进阶收口，见 `asc::adjust`
                if let EOp::Attack { base, hits } =
                    crate::asc::adjust(s.enemy_def[e], m, oi, s.ascension, *op)
                {
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
    /// 确定性窗口白送了几层（[`Plan::window`]）。
    ///
    /// 和 `ext_fired` 同一个用处：**免费的那笔预算唯一看得见的证据**。
    /// 它为 0 而 `max_depth` 却在涨，说明有人在别的地方漏扣了。
    pub window_fired: u32,
}

impl PlanStat {
    /// 把另一条线程的账并进来。**深度取最大、次数相加** ——
    /// 候选是分块并行求值的（[`eval_candidates`]），每块各记各的。
    pub fn merge(&mut self, o: &PlanStat) {
        self.max_depth = self.max_depth.max(o.max_depth);
        self.ext_fired += o.ext_fired;
        self.window_fired += o.window_fired;
    }
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
/// `ext` 是**还能延伸几层**的预算。它和 `left` 是两笔账：
/// `left` 是计划好的深度，`ext` 是"话说到一半"时额外借的层数。
/// 分开记的理由是不让延伸把计划深度也一起吃掉 —— 借来的层只能借一次。
///
/// **第三笔账是确定性窗口**（[`Plan::window`]），从 `cfg` 里起头，
/// 见 [`node_value_window`]。
///
/// **不带置换表**（`tt = None`）—— 这是测试用的入口，而 memo 的正确性
/// 和搜索的正确性是两件事，测试那一条不该被 memo 挡在外面。
#[cfg(test)]
pub(crate) fn node_value_ext(
    s: &State,
    cfg: &Plan,
    left: u8,
    depth: usize,
    ext: u8,
    stat: &mut PlanStat,
) -> i32 {
    node_value_window(s, cfg, left, depth, ext, cfg.window, None, stat)
}

/// 同 [`node_value_ext`]，但把**确定性窗口**那笔预算也显式传进来。
///
/// 三笔账的规矩，一行一条：
///
/// * 这一层**一个骰子都不掷**（[`window_is_certain`]）且 `free > 0`
///   ⇒ 扣 `free`，**`left` 一分不动**。计划深度是留给会分岔的层的。
/// * 否则 `left == 0` ⇒ 老规矩：能延伸就借一层（扣 `ext`），否则就地评估。
/// * `depth` 三笔一起数，[`HARD_DEPTH_CAP`] 兜底。
///
/// **尺子（`deep_score` / `leaf`）不在这里挑。** 一次搜索只有一把，由
/// [`Plan::at_root`] 在入口处定好 —— 逐层挑会让根上的候选互相之间没法比，
/// 理由写在那个函数里。
#[allow(clippy::too_many_arguments)]
fn node_value_window(
    s: &State,
    cfg: &Plan,
    left: u8,
    depth: usize,
    ext: u8,
    free: u8,
    tt: Option<&Tt>,
    stat: &mut PlanStat,
) -> i32 {
    stat.max_depth = stat.max_depth.max(depth.min(255) as u8);
    if s.combat_over || s.player_dead || depth >= HARD_DEPTH_CAP {
        return cfg.leaf.value(s, cfg.seed, depth);
    }
    // **memo 的 key 里必须带上这四笔账。**
    //
    // 一个局面的值不是它自己的函数：`depth` 定了这一层的预算
    // （[`Plan::budget`]）和叶子的播种（CRN，见 [`Leaf`]），`left`/`ext`/`free`
    // 定了还能往下走多远。少揉一个进去，就是拿"搜得浅的那次"去顶替深搜 ——
    // 正是 [`TtEntry`] 那条"存的深度 ≥ 要的深度"在防的事，只不过那条一维的
    // 深度在这里是四维的。四笔全进 key ⇒ 复用是**精确复用**，
    // `probe` 里那条 `depth >= need` 就只剩兜底。
    let tk = tt.map(|_| {
        crate::solver::z(
            key(s)
                ^ ((depth as u64) << 48)
                ^ ((left as u64) << 32)
                ^ ((ext as u64) << 16)
                ^ (free as u64),
        )
    });
    if let (Some(t), Some(k)) = (tt, tk) {
        if let Some(v) = t.probe(k, left) {
            return v;
        }
    }
    let orig_left = left;
    let mut left = left;
    let mut ext = ext;
    let mut free = free;
    let mut narrow = false;
    // **确定性窗口**：这一层没有任何随机 ⇒ 它不是"深度"，是白送的。
    // 判在最前面，`left == 0` 也照借 —— 窗口正是靠这一条越过计划深度的。
    let certain = free > 0 && window_is_certain(s);
    if certain {
        free -= 1;
        stat.window_fired += 1;
    } else if left == 0 {
        // 计划的深度走完了：看看这个叶子是不是落在"话说到一半"的地方
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
    debug_assert!(!certain || kids.len() == 1, "确定层却给出了 {} 个孩子", kids.len());
    // **确定层不扣计划深度**，见 [`Plan::window`]。`certain == false` 时
    // 上面那段保证了 `left >= 1`（要么本来就 >= 1，要么延伸把它顶成了 1）。
    let next_left = if certain { left } else { left - 1 };
    let mut acc = 0.0f64;
    for k in &kids {
        let threat = predicted_threat(&k.state);
        // **深层选线用 `deep_score`，不是 `cfg.score`** —— 这一层的后继就是叶子
        // （或者更深一层的期望），所以 argmax 该按叶评估取。理由和实测读数见
        // [`Plan::deep_score`]。**窄根上它已经被 [`Plan::at_root`] 换成了
        // [`Plan::window_score`]**（阶段 4），这里读到的就是换好的那一个。
        let line =
            solve_turn_potions(&k.state, &threat, cfg.deep_score, cfg.budget(depth), 0).line;
        let v = match replay_line(&k.state, line.acts()) {
            Some(after) if !after.combat_over => {
                let s1 = end_turn_before_draw(after);
                node_value_window(&s1, cfg, next_left, depth + 1, ext, free, tt, stat)
            }
            Some(after) => cfg.leaf.value(&after, cfg.seed, depth + 1),
            None => cfg.leaf.value(&k.state, cfg.seed, depth + 1),
        };
        acc += k.p * v as f64;
    }
    let out = acc.round() as i32;
    if let (Some(t), Some(k)) = (tt, tk) {
        // 存的"深度"就是探的那一个：四笔账已经在 key 里，这里只是让
        // [`Tt::store`] 的替换规则（同一轮深的压浅的）有个可比的数。
        t.store(k, orig_left, out);
    }
    out
}
