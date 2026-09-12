//! **L3 阶段 3：整幕链式评估** —— `(牌组, 遗物, 血量, 一条路线) -> 走完这一幕的死亡率`。
//!
//! 阶段 2 把**一场**仗打完 N 次（[`crate::synth::eval`]）。这一层把一整幕
//! 串起来打 N 次：遭遇按 [源码] 的分布抽、**血量跨场累计**、死在第几间也记下来。
//!
//! ```text
//! 牌组 ─┐                        ┌── 房间 1：抽一场遭遇 ─► eval 打一场 ─► 剩多少血 ─┐
//! 遗物 ─┼─► 每个样本抽一条遭遇序列 ┤   房间 2：…                            ◄────────┘
//! 血量 ─┤   （**牌组进不来**）     └── 房间 k：死了就停，没死就把血带进下一场
//! 路线 ─┘                                                │
//!                     死亡率 · 死在第几间 · 终点血量分布 ◄┘
//! ```
//!
//! # 为什么是这一层而不是单场
//!
//! 路线图给 L3 定的判决是**「走完这一幕的死亡概率」**，理由在父目录
//! `CLAUDE.md`：单场 ΔHP 在死亡处被截断，而**引擎牌的钱要跨一整幕才收得回来**
//! —— 添柴/黑暗之拥那套在单场看必然亏。一整幕串起来才问得出这件事。
//!
//! # 路线**不建**，由调用方给（[`ActPlan::rooms`]）
//!
//! 「走哪条路、走几间、在哪歇」是局外决策，路线图的第一刀就把它划在外面。
//! 这一层只吃一个**已经定下来的房间序列**，然后回答"按这条路走，这副牌组
//! 会怎么样"。房间类型只有四种（[`Room`]），别的一律不在这一层：
//! 商店/事件/宝箱对**战斗链**是空操作，把它们写进来只会让人以为建了。
//!
//! # 抽序列：照 [源码] `ActModel.GenerateRooms` 的**分布**，不是它的随机流
//!
//! | [源码] | 这里 |
//! |---|---|
//! | 前 `NumberOfWeakEncounters` 场从弱怪池的 `GrabBag` 抽 | 同 |
//! | 其余抽到 `BaseNumberOfRooms` 场，从普通池抽 | 同 |
//! | 精英预抽 15 场 | 同（[`ELITE_DRAWS`]）|
//! | `AddWithoutRepeatingTags`：不许和**上一场**共享 tag，也不许是同一场 | 同 |
//! | 袋子抽空了才补 | 同 |
//! | Boss 在 `AllBossEncounters` 里均匀取一个 | 同 |
//! | A10 第二个 Boss：在**其余** Boss 里均匀取一个，**只有最后一幕** | [`ActPlan::double_boss`] |
//!
//! **`GrabBag` 的权重全是 1**（`grabBag.Add(e, 1.0)`），所以它等价于
//! 「在满足谓词的那些里等概率抽一个、抽完拿走」—— 游戏那个 `do/while`
//! 拒绝采样收敛到的正是这个分布。**随机流本来就不和游戏一致**（不变量 4），
//! 这里要的是分布对。
//!
//! **一条抄错就会静默偏**：弱怪和普通怪进的是**同一个输出列表**
//! （[源码] 两个循环都 `AddWithoutRepeatingTags(_rooms.normalEncounters, …)`），
//! 所以普通池抽的第一场要和**最后一场弱怪**比 tag。分成两个列表的话，
//! 「连着两场史莱姆」这件事会在交界处漏掉。
//!
//! # 覆盖率不是 100%，开不出来的**跳过并计数**
//!
//! 内核今天开不出这一幕的全部遭遇（第 1 幕 Underdocks 只有一半），
//! 而抽序列**故意不过滤**（见 [`Table::pool`]）—— 过滤掉就等于
//! "这一幕只有内核认识的那几场"，那是个静默的谎。
//!
//! 抽到开不出的那一场：**这一间不打，计一笔 [`ActSample::unsimulated`]**，
//! 血量原样带下去。方向因此是**乐观**的（少挨了几场打），
//! 报告里必须和结论并排印 —— [`ActEval::coverage`] 和 `unsimulated` 就是那两个数。
//!
//! # 截断：整条链作废
//!
//! 一场仗撞上回合上限就**没有分出胜负**，而它照样带着一个血量 ——
//! 把那个血量带进下一场，后面每一场都建在一个假前提上。所以链在那里停住，
//! 整个样本既不进死亡数也不进血量分布（和 [`crate::synth::eval`] 对单场的
//! 处置是同一条），由 `bin/act_eval` 守那道硬门。
//!
//! # CRN：这一层比单场**多共享一维**
//!
//! 阶段 2 量到「死亡率是噪声最大的那一维」，并把方差缩减留给了这一层。
//! 这里多共享的那一维正是路线图点名的那个：**「这一幕抽到哪几场遭遇」**。
//!
//! | 共享 | 怎么做到的 |
//! |---|---|
//! | 这一条链**抽到哪几场遭遇** | [`seq_seed`] 只吃 `(base, i)`，牌组进不来 |
//! | 第 k 场的**敌人血量** | [`fight_seed`] 只吃 `(base, i, k)`，`synth::hp_stream` 只吃它 |
//! | 三条战斗内随机流的**起点** | 同上 |
//!
//! **共享到此为止**：牌组一换，洗牌流和敌人流消耗的次数当场不同
//! （见 `eval` 模块头）。而且链还有一条单场没有的分岔 —— **候选 A 死在第 3 间、
//! B 活着走完**，那之后两条链看到的是同一批遭遇但不同的血量。
//! 这不是噪声，那正是要量的东西。

use crate::rollout::{can_rollout, par_map, rollout_outcome_with, Policy};
use crate::state::{next_below, next_u64};
use crate::synth::encounters::{Act, Encounter, Pool, Table, Unresolved};
use crate::synth::eval::{Caveat, Dist, DEFAULT_SAMPLES, EVAL_MAX_TURNS};
use crate::synth::{build, FightSpec, Gap};
use crate::ops::Kind;
use crate::content::card;

/// [源码] `ActModel.GenerateRooms` 里精英遭遇预抽的场数（`for (num3 = 0; num3 < 15; …)`）。
///
/// 它**不是**"这一幕有几间精英房"（那是地图的事，`MapPointTypeCounts.NumOfElites`
/// 基础 5、A1 的蜂拥精英抬到 8）—— 预抽多是为了够用，走几间由路线定。
pub const ELITE_DRAWS: usize = 15;

/// 一条路线上的一个房间。**只有这四种**，理由见模块头。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Room {
    /// 普通战斗（拉 `normalEncounters` 的下一场）
    Monster,
    /// 精英（拉 `eliteEncounters` 的下一场）
    Elite,
    /// Boss。A10 的第二个 Boss 是**第二间** Boss 房，不是同一间里两只
    /// （[源码] `StandardActMap`：`SecondBossMapPoint` 挂在 Boss 之上一行）
    Boss,
    /// 休息处（睡觉那一项）。**打铁不在这一层** —— 那是改牌组，
    /// 而这一层拿到的牌组是调用方给的一份定值
    Rest,
}

impl Room {
    /// 一个字母一个房间：`M` / `E` / `B` / `R`。大小写都认，其余一律 `None`。
    pub fn parse(c: char) -> Option<Room> {
        match c.to_ascii_uppercase() {
            'M' => Some(Room::Monster),
            'E' => Some(Room::Elite),
            'B' => Some(Room::Boss),
            'R' => Some(Room::Rest),
            _ => None,
        }
    }

    pub fn letter(&self) -> char {
        match self {
            Room::Monster => 'M',
            Room::Elite => 'E',
            Room::Boss => 'B',
            Room::Rest => 'R',
        }
    }

    pub fn is_fight(&self) -> bool {
        !matches!(self, Room::Rest)
    }
}

/// 一整串房间（`"MMRMEB"`）。认不出的字符**整条拒绝**并说明是哪一个 ——
/// 悄悄跳过一个字符就是悄悄改掉一条路线。
pub fn parse_rooms(s: &str) -> Result<Vec<Room>, String> {
    let mut out = Vec::new();
    for c in s.chars() {
        if c.is_whitespace() || c == '-' || c == ',' {
            continue;
        }
        match Room::parse(c) {
            Some(r) => out.push(r),
            None => return Err(format!("房间字母 `{c}` 不认识（只认 M/E/B/R）")),
        }
    }
    if out.is_empty() {
        return Err("一个房间都没有".to_string());
    }
    Ok(out)
}

/// 这一幕的路线。**路线是输入不是结论**，见模块头。
#[derive(Clone, Copy, Debug)]
pub struct ActPlan<'a> {
    /// 遭遇表里的幕名（`"Underdocks"` / `"Hive"` …）
    pub act: &'a str,
    pub rooms: &'a [Room],
    /// **A10 且这是最后一幕**才为真（[源码] `RunManager`：
    /// `i == State.Acts.Count - 1 && HasLevel(DoubleBoss)`）。
    /// 为真时第二间 Boss 房拉的是另一只 Boss。
    pub double_boss: bool,
    /// **这一幕的 Boss 已经定下来了**（遭遇 key，`"SoulFyshBoss"`）。
    ///
    /// 实战里这是个**已知量**：地图屏一进去就报着这一幕的 Boss 是谁
    /// （mod 的 `map.boss.id`，[源码] `ActModel.BossEncounter`）。
    /// 给了就不掷 —— 掷一个均匀的 Boss 等于把一条已知信息换成方差，
    /// 而 Boss 那一场是整幕死亡率里最大的一块。
    ///
    /// **这是阶段 1 那句「欠一个局外信息有时候等于欠一个参数」的第四条**
    /// （前三条是 `after_rest` / `RelicSpec::counter` / `boss_room`）。
    /// 给的 key 不在这一幕的 Boss 池里就**整个拒绝作答**
    /// （[`ActRefusal::NoSuchBoss`]）：那时候悄悄退回去掷一个，
    /// 报出来的数会像是"按你说的那只 Boss 算的"。
    pub pin_boss: Option<&'a str>,
    /// A10 第二个 Boss 的同一件事。`double_boss` 为假时它没有消费点。
    pub pin_second_boss: Option<&'a str>,
}

impl<'a> ActPlan<'a> {
    /// 最小构造：一幕 + 一条路线。Boss 掷、不是双 Boss。
    pub fn new(act: &'a str, rooms: &'a [Room]) -> ActPlan<'a> {
        ActPlan { act, rooms, double_boss: false, pin_boss: None, pin_second_boss: None }
    }
}

/// 一幕抽出来的遭遇序列（[源码] `RoomSet` 的三份清单）。
#[derive(Clone, Debug, Default)]
pub struct Sequence {
    /// 普通房按顺序拉的那些（前几场是弱怪）
    pub normals: Vec<String>,
    pub elites: Vec<String>,
    pub boss: Option<String>,
    /// A10 的第二个 Boss
    pub second_boss: Option<String>,
}

impl Sequence {
    /// [源码] `RoomSet.NextNormalEncounter`：**按访问次数取模**，
    /// 走的间数超过预抽的场数就从头再来。
    fn pull(list: &[String], visited: usize) -> Option<&str> {
        if list.is_empty() {
            None
        } else {
            Some(list[visited % list.len()].as_str())
        }
    }
}

/// 一次抽袋（[源码] `GrabBag` + `AddWithoutRepeatingTags`）。
///
/// 权重全是 1，所以「按权重抽 + 拒绝采样」等价于「在满足谓词的那些里等概率抽」。
/// **谓词一个都不满足时退回不带谓词的那一次抽**（[源码] 那两行就是这么写的），
/// 不是"抽不出来"。
fn grab<'a>(bag: &mut Vec<&'a Encounter>, last: Option<&Encounter>, rng: &mut u64) -> Option<&'a Encounter> {
    if bag.is_empty() {
        return None;
    }
    let ok: Vec<usize> = (0..bag.len())
        .filter(|&i| match last {
            None => true,
            Some(l) => bag[i].key != l.key && !shares_tag(bag[i], l),
        })
        .collect();
    let ix = if ok.is_empty() { next_below(rng, bag.len()) } else { ok[next_below(rng, ok.len())] };
    Some(bag.remove(ix))
}

fn shares_tag(a: &Encounter, b: &Encounter) -> bool {
    a.tags.iter().any(|t| b.tags.contains(t))
}

/// 从一个池子里抽 `n` 场，抽空了补袋 —— [源码] 那两个 `for` 循环。
///
/// `out` 是**共用的输出列表**：弱怪那一轮和普通那一轮写进同一个 `out`，
/// 所以普通池的第一场会和最后一场弱怪比 tag（见模块头）。
///
/// **"上一场是什么"要去整张表里查，不是在本池里查** —— 交界处那一场
/// 在**另一个**池子里（弱怪池），在本池里找必然找不到，那条 tag 约束
/// 就会在唯一需要它的地方失效。
fn draw_into(t: &Table, out: &mut Vec<String>, pool: &[&Encounter], n: usize, rng: &mut u64) {
    if pool.is_empty() {
        return;
    }
    let mut bag: Vec<&Encounter> = Vec::new();
    let mut last: Option<&Encounter> = out.last().and_then(|k| t.encounters.get(k));
    for _ in 0..n {
        if bag.is_empty() {
            bag = pool.to_vec();
        }
        match grab(&mut bag, last, rng) {
            Some(e) => {
                out.push(e.key.clone());
                last = Some(e);
            }
            None => break,
        }
    }
}

/// 抽这一幕的遭遇序列。**只吃种子、表和路线，牌组进不来** —— 那是 CRN 的全部内容。
///
/// `plan` 进来只为了两样**局外已知**的东西：`double_boss` 和两个
/// [`ActPlan::pin_boss`]。路线本身（走哪几间）在这一步没有用 ——
/// 序列是先抽好的一份清单，走几间由 [`one_chain`] 去拉。
pub fn draw_sequence(t: &Table, act: &Act, plan: &ActPlan, seed: u64) -> Sequence {
    let mut rng = seed;
    let mut out = Sequence::default();

    let weak = t.pool(act, Pool::Weak);
    let regular = t.pool(act, Pool::Regular);
    draw_into(t, &mut out.normals, &weak, act.weak_encounters, &mut rng);
    // [源码] 第二个循环从 `NumberOfWeakEncounters` 数到 `GetNumberOfRooms()`，
    // 也就是**剩下的那几场**。弱怪池空了的话第一轮少抽了几场，
    // 这里照 [源码] 的循环边界走（不去补），差额会体现在序列变短上。
    let rest = act.base_rooms.saturating_sub(act.weak_encounters);
    draw_into(t, &mut out.normals, &regular, rest, &mut rng);

    let elite = t.pool(act, Pool::Elite);
    draw_into(t, &mut out.elites, &elite, ELITE_DRAWS, &mut rng);

    // Boss：[源码] `rng.NextItem(AllBossEncounters)` —— 均匀一个。
    //
    // **`ApplyDiscoveryOrderModifications` 没建**：它会在"这只 Boss 还没见过"时
    // 强制选它，而"见没见过"是存档进度，战斗观测里一个字都没有。
    // 欠一个拿不到的输入 ⇒ 留空（这里就是均匀抽），不挑一组自洽的解。
    //
    // **给了 `pin_boss` 就不掷**：那是局外已知量（地图屏上写着），
    // 见 [`ActPlan::pin_boss`]。key 合不合法在 [`evaluate_act`] 里查过了，
    // 这里只管用 —— 每条链各查一遍是把同一件事做 N 遍。
    let bosses = t.pool(act, Pool::Boss);
    if !bosses.is_empty() {
        let b = match plan.pin_boss {
            Some(k) => k.to_string(),
            None => bosses[next_below(&mut rng, bosses.len())].key.clone(),
        };
        if plan.double_boss {
            out.second_boss = match plan.pin_second_boss {
                Some(k) => Some(k.to_string()),
                // [源码] `NextItem(AllBossEncounters.Where(e => e.Id != BossEncounter.Id))`
                None => {
                    let others: Vec<&&Encounter> = bosses.iter().filter(|e| e.key != b).collect();
                    (!others.is_empty())
                        .then(|| others[next_below(&mut rng, others.len())].key.clone())
                }
            };
        }
        out.boss = Some(b);
    }
    out
}

/// 休息处睡一觉回多少血。
///
/// [源码] `HealRestSiteOption.GetBaseHealAmount = MaxHp * 0.3m`，落地那一步是
/// `Creature.SetCurrentHpInternal` 的 `(int)Math.Min(hp + amount, MaxHp)`
/// —— **向下取整、封顶在上限**。73 血上限回 21（不是 22）。
///
/// **`Hook.ModifyRestSiteHealAmount` 没建**（有遗物会改这个数）。
/// 方向是低估自己 —— 和构造器那几处回退一致。
pub fn rest_heal(max_hp: i32) -> i32 {
    (max_hp * 3 / 10).max(0)
}

/// 一条链跑完之后的样子。
#[derive(Clone, Debug)]
pub struct ActSample {
    /// 死在第几个房间（`rooms` 的下标）。`None` = 走完了
    pub died_at: Option<usize>,
    /// 哪一间的推演撞上了回合上限。**这条链从那里作废**
    pub truncated_at: Option<usize>,
    /// 走完（或死掉）时的血量。截断的样本这个数**不是结果**
    pub final_hp: i32,
    /// 真打了几场
    pub fights: usize,
    /// 抽到了但**开不出来**的战斗房间数（内核缺这一场）。
    /// **它是乐观的方向**：少打了这几场
    pub unsimulated: usize,
    /// 睡了几觉（诊断用：回了多少血是路线给的，不是牌组挣的）
    pub rests: usize,
    /// 全程打了几个回合
    pub turns: u32,
}

/// 拒绝作答的理由。**给一个自信的数比拒绝更危险。**
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ActRefusal {
    /// 遭遇表里没有这一幕
    NoSuchAct(String),
    /// 这条路线上一场仗都开不出来（这一幕内核一场都不认识）
    NothingSimulable,
    /// 调用方钉的那只 Boss 不在这一幕的 Boss 池里。
    /// **退回去掷一个是错的** —— 报出来的数会像是"按你说的那只算的"
    NoSuchBoss { act: String, key: String },
}

impl std::fmt::Display for ActRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActRefusal::NoSuchAct(a) => write!(f, "遭遇表里没有「{a}」这一幕"),
            ActRefusal::NothingSimulable => {
                write!(f, "这条路线上一场仗都开不出来 —— 这不是 0 分，是没有分")
            }
            ActRefusal::NoSuchBoss { act, key } => {
                write!(f, "「{key}」不是「{act}」这一幕的 Boss —— 钉错了 Boss 的数比没有数更糟")
            }
        }
    }
}

/// 一条链上某一间**没打成**的原因，逐条报。
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Skipped {
    /// 遭遇表解不开这一场（构成含随机 / 内核没建这只怪）
    Unresolved(String),
    /// 解得开，但内核不认得它的出招表 —— 推演会是一场它不出手的仗
    CannotRollout(String),
    /// 这一类房间在这一幕一场都没抽出来（池子空了）
    EmptyPool(Room),
}

impl std::fmt::Display for Skipped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Skipped::Unresolved(w) => write!(f, "{w}"),
            Skipped::CannotRollout(k) => write!(f, "{k} 的敌人内核不认得出招表"),
            Skipped::EmptyPool(r) => write!(f, "{:?} 这一类在这一幕的池子是空的", r),
        }
    }
}

/// 一次整幕评估的配置。三个字段和 [`crate::synth::eval::EvalCfg`] 同名同义。
#[derive(Clone, Copy, Debug)]
pub struct ActCfg {
    pub samples: usize,
    pub max_turns: usize,
    pub policy: Policy,
    /// **故意破坏 CRN 的那个旋钮**（默认 0 = 不破坏）。
    ///
    /// 非 0 时它进 [`seq_seed`] 和 [`fight_seed`] 的基，于是这一次评估
    /// 抽到的遭遇序列和逐场敌人血量**和另一次完全无关**。
    ///
    /// 它存在的唯一理由是**归因**：整幕链比单场多共享了一维
    /// （「这一幕抽到哪几场遭遇」），而"整幕的判据更干净"同时还有另一个来源
    /// —— 一条链里有好几场仗，信号本来就更多。
    /// **两个来源混在一个数里，那个数说不清是哪一件事买来的**，
    /// 所以留一个能把共享关掉的开关，A/B 一次就分得开。
    pub crn_salt: u64,
}

impl Default for ActCfg {
    fn default() -> ActCfg {
        ActCfg {
            samples: DEFAULT_SAMPLES,
            max_turns: EVAL_MAX_TURNS,
            policy: Policy::default(),
            crn_salt: 0,
        }
    }
}

/// 一幕被走完 N 次之后的样子。**死亡率在前，血量在后** —— 和单场同一条规矩。
pub struct ActEval {
    pub n: usize,
    pub deaths: usize,
    /// 有一场撞上回合上限的链数。**这些链既不算死也不算活**
    pub truncated: usize,
    pub start_hp: i32,
    /// 走完了、而且活着的那些链的**终点血量**
    pub hp_end: Dist,
    /// 同一批链的**全程战损** = `start_hp − final_hp`（含休息处回的血，
    /// 所以**可以是负的**）
    pub loss: Dist,
    /// 每条链真打了几场
    pub fights: Dist,
    /// 每条链**没打成**几场（内核开不出来的）
    pub unsimulated: Dist,
    /// 死在第几个房间：`deaths_by_room[i]` = 第 i 间死掉的链数。
    /// **死亡率是一个数，死在哪一间是一条曲线** —— 后者才说得出
    /// 「是被 Boss 打死的还是一路被磨死的」
    pub deaths_by_room: Vec<usize>,
    /// 这一幕内核开得出几场 / 一共几场（[`Table::coverage`]）。
    /// **必须和结论并排印**，见模块头
    pub coverage: (usize, usize),
    /// 逐条「这一间没打成」的原因，去重
    pub skipped: Vec<Skipped>,
    /// 构造器报的缺口，去重
    pub gaps: Vec<Gap>,
    pub refused: Option<ActRefusal>,
    pub samples: Vec<ActSample>,
    pub seed: u64,
    /// 这一幕的路线（回显用）
    pub rooms: Vec<Room>,
    deck_powers: usize,
    potions: usize,
    ascension: u8,
}

impl ActEval {
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

    /// 走完了并且活着的链数 —— 血量分布的分母。
    pub fn finished_alive(&self) -> usize {
        self.hp_end.n
    }

    /// **一行读数**：死亡率在最前面，覆盖率紧跟着 ——
    /// 后者不在旁边的话，前者就是个自信的数。
    pub fn headline(&self) -> String {
        if let Some(r) = &self.refused {
            return format!("拒绝作答：{r}");
        }
        format!(
            "死 {}/{} ({:.0}%) · 截断 {} · 终点血量 {} · 打了 {:.1} 场 · 漏掉 {:.1} 场（这一幕开得出 {}/{}）",
            self.deaths,
            self.n,
            self.death_rate() * 100.0,
            self.truncated,
            self.hp_end,
            self.fights.mean,
            self.unsimulated.mean,
            self.coverage.0,
            self.coverage.1,
        )
    }

    /// **方向已知的偏差**，从这副牌组和这次评估自己算出来。
    ///
    /// 单场那四条原样带过来（引擎牌 / 药水 / 缺口 / A8 以上），
    /// 外加整幕独有的一条：**漏掉的那几场**。
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

    /// 整幕独有的那一条偏差：**平均每条链漏掉几场**。
    /// 有漏才报 —— 一句永远都在的免责文本会被读者跳过。
    pub fn missing_fights(&self) -> Option<f64> {
        if self.unsimulated.n > 0 && self.unsimulated.mean > 0.0 {
            Some(self.unsimulated.mean)
        } else {
            None
        }
    }
}

/// 样本 `i` 抽序列用的种子。**只吃 `(base, i)`** —— 牌组进不来，
/// 于是两个候选在样本 `i` 上抽到的是**同一批遭遇**。
pub fn seq_seed(base: u64, i: usize) -> u64 {
    let mut z = base ^ 0x5EED_0000_AC70_0000;
    z = z.wrapping_add((i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    next_u64(&mut z)
}

/// 样本 `i` 的第 `k` 间房那场仗的种子。**只吃 `(base, i, k)`** ——
/// `synth::hp_stream` 只吃它，所以两个候选在同一格看到的敌人血量逐字相同。
pub fn fight_seed(base: u64, i: usize, k: usize) -> u64 {
    let mut z = base;
    z = z.wrapping_add((i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = z.wrapping_add((k as u64).wrapping_mul(0xD1B5_4A32_D192_ED03));
    next_u64(&mut z)
}

/// 走完这一幕 `cfg.samples` 次。
///
/// `spec` 里的 `enemies` / `hp` / `after_rest` / `boss_room` / `seed`
/// **会被逐场覆盖** —— 它们是链自己算出来的。其余（牌组、遗物、上限血量、
/// 药水、能量、进阶）整幕不变：**局外一律不建**，牌组在一幕之内是一份定值。
pub fn evaluate_act(spec: &FightSpec, plan: &ActPlan, t: &Table, cfg: &ActCfg) -> ActEval {
    let n = cfg.samples.max(1);
    let deck_powers =
        spec.deck.iter().filter(|c| matches!(crate::content::card(c.id).kind, Kind::Power)).count();
    let potions = spec.potions.iter().filter(|&&p| p != 0).count();
    let mut out = ActEval {
        n: 0,
        deaths: 0,
        truncated: 0,
        start_hp: spec.hp,
        hp_end: Dist::default(),
        loss: Dist::default(),
        fights: Dist::default(),
        unsimulated: Dist::default(),
        deaths_by_room: vec![0; plan.rooms.len()],
        coverage: (0, 0),
        skipped: Vec::new(),
        gaps: Vec::new(),
        refused: None,
        samples: Vec::new(),
        seed: spec.seed,
        rooms: plan.rooms.to_vec(),
        deck_powers,
        potions,
        ascension: spec.ascension,
    };
    let Some(act) = t.act_by_name(plan.act) else {
        out.refused = Some(ActRefusal::NoSuchAct(plan.act.to_string()));
        return out;
    };
    out.coverage = t.coverage(act);
    // **钉的那只 Boss 必须真的是这一幕的。** 查一次（不是每条链各查一遍），
    // 不合法就整个拒绝作答 —— 见 [`ActPlan::pin_boss`]。
    for key in [plan.pin_boss, plan.pin_second_boss].into_iter().flatten() {
        if !t.pool(act, Pool::Boss).iter().any(|e| e.key == key) {
            out.refused =
                Some(ActRefusal::NoSuchBoss { act: act.name.clone(), key: key.to_string() });
            return out;
        }
    }

    // `crn_salt` 默认 0 ⇒ 基就是 `spec.seed`，两个候选逐字相同（CRN）。
    let base = spec.seed ^ cfg.crn_salt;
    let runs = par_map(n, |i| one_chain(spec, plan, act, t, cfg, base, i));

    out.n = n;
    out.deaths = runs.iter().filter(|r| r.0.died_at.is_some()).count();
    out.truncated = runs.iter().filter(|r| r.0.truncated_at.is_some()).count();
    for (s, _, _) in &runs {
        if let Some(k) = s.died_at {
            if k < out.deaths_by_room.len() {
                out.deaths_by_room[k] += 1;
            }
        }
    }
    // **分布的分母只有"走完了并且活着"的链** —— 和单场同一条规矩。
    let done = |s: &ActSample| s.truncated_at.is_none() && s.died_at.is_none();
    let mut hp: Vec<i32> = runs.iter().filter(|r| done(&r.0)).map(|r| r.0.final_hp).collect();
    let mut loss: Vec<i32> = hp.iter().map(|h| spec.hp - h).collect();
    // 打了几场 / 漏了几场**所有没截断的链都算**（死掉的那几条也是真读数：
    // 它打到第几场才死，正是"死在哪一间"那条曲线的另一面）
    let mut fights: Vec<i32> =
        runs.iter().filter(|r| r.0.truncated_at.is_none()).map(|r| r.0.fights as i32).collect();
    let mut miss: Vec<i32> =
        runs.iter().filter(|r| r.0.truncated_at.is_none()).map(|r| r.0.unsimulated as i32).collect();
    out.hp_end = Dist::of(&mut hp);
    out.loss = Dist::of(&mut loss);
    out.fights = Dist::of(&mut fights);
    out.unsimulated = Dist::of(&mut miss);
    for (_, gaps, skipped) in &runs {
        for g in gaps {
            if !out.gaps.contains(g) {
                out.gaps.push(g.clone());
            }
        }
        for s in skipped {
            if !out.skipped.contains(s) {
                out.skipped.push(s.clone());
            }
        }
    }
    out.samples = runs.into_iter().map(|r| r.0).collect();
    if out.fights.n > 0 && out.fights.max == 0 {
        out.refused = Some(ActRefusal::NothingSimulable);
    }
    out
}

/// 走一条链。返回 (结局, 这条链上报的缺口, 这条链上没打成的那几间)。
fn one_chain(
    spec: &FightSpec,
    plan: &ActPlan,
    act: &Act,
    t: &Table,
    cfg: &ActCfg,
    base: u64,
    i: usize,
) -> (ActSample, Vec<Gap>, Vec<Skipped>) {
    let seq = draw_sequence(t, act, plan, seq_seed(base, i));
    let mut gaps: Vec<Gap> = Vec::new();
    let mut skipped: Vec<Skipped> = Vec::new();
    let mut s = ActSample {
        died_at: None,
        truncated_at: None,
        final_hp: spec.hp,
        fights: 0,
        unsimulated: 0,
        rests: 0,
        turns: 0,
    };
    // **进这一间之前的血量**（`FightSpec::hp` 的语义），开局回血由构造器自己加
    let mut hp = spec.hp;
    let (mut n_normal, mut n_elite, mut n_boss) = (0usize, 0usize, 0usize);
    // 上一间是不是休息处 —— 古茶具靠它武装（`content::CONDITIONAL_START`）。
    // **这就是阶段 1 留下的那个"欠一个参数"**：整幕链自己知道在模拟哪个房间。
    let mut after_rest = spec.after_rest;

    for (k, room) in plan.rooms.iter().enumerate() {
        if *room == Room::Rest {
            s.rests += 1;
            hp = (hp + rest_heal(spec.max_hp)).min(spec.max_hp);
            after_rest = true;
            continue;
        }
        let key = match room {
            Room::Monster => Sequence::pull(&seq.normals, n_normal),
            Room::Elite => Sequence::pull(&seq.elites, n_elite),
            Room::Boss => {
                if n_boss > 0 {
                    seq.second_boss.as_deref().or(seq.boss.as_deref())
                } else {
                    seq.boss.as_deref()
                }
            }
            Room::Rest => unreachable!("上面已经 continue 了"),
        };
        match room {
            Room::Monster => n_normal += 1,
            Room::Elite => n_elite += 1,
            Room::Boss => n_boss += 1,
            Room::Rest => {}
        }
        let Some(key) = key else {
            s.unsimulated += 1;
            push_unique(&mut skipped, Skipped::EmptyPool(*room));
            continue;
        };
        let enemies = match t.resolve(key) {
            Ok(e) => e,
            Err(why) => {
                s.unsimulated += 1;
                push_unique(&mut skipped, Skipped::Unresolved(describe(&why)));
                continue;
            }
        };
        let fight = FightSpec {
            hp,
            enemies: &enemies,
            after_rest,
            boss_room: t.is_boss(key),
            seed: fight_seed(base, i, k),
            ..*spec
        };
        if std::env::var("ACT_TRACE").is_ok() {
            eprintln!("[dbg] sample {i} room {k} {key} hp {hp}");
        }
        let built = build(&fight);
        for g in built.gaps {
            push_unique(&mut gaps, g);
        }
        if !can_rollout(&built.state) {
            // 敌人认得出名字、却没有出招表 —— 推演会是一场它不出手的仗
            // （`rollout` 模块头那条 12 血打 200 血）。**不打，计一笔。**
            s.unsimulated += 1;
            push_unique(&mut skipped, Skipped::CannotRollout(key.to_string()));
            continue;
        }
        let o = rollout_outcome_with(built.state, cfg.max_turns, cfg.policy);
        s.fights += 1;
        s.turns += o.turns;
        after_rest = false;
        if o.truncated {
            // 这一场没分出胜负 ⇒ 后面每一场都会建在一个假前提上。**链停在这里。**
            s.truncated_at = Some(k);
            s.final_hp = o.final_hp;
            return (s, gaps, skipped);
        }
        hp = o.final_hp;
        if o.died {
            s.died_at = Some(k);
            s.final_hp = hp;
            return (s, gaps, skipped);
        }
    }
    s.final_hp = hp;
    (s, gaps, skipped)
}

fn push_unique<T: PartialEq>(v: &mut Vec<T>, x: T) {
    if !v.contains(&x) {
        v.push(x);
    }
}

fn describe(u: &Unresolved) -> String {
    u.to_string()
}

/// 两条链的**配对差值**。和单场的 `paired_delta` 是同一条规矩：
/// **死亡和血量分开数，不许混**。
///
/// `a` / `b` 必须同种子基、同样本数、**同一条路线** —— 路线一换就不是
/// 同一个问题了，那时的差值什么都不是。
pub fn paired_act_delta(a: &ActEval, b: &ActEval) -> Option<ActDelta> {
    if a.rooms != b.rooms {
        return None;
    }
    paired_act_delta_across_routes(a, b)
}

/// 同一副牌组、**两条不同路线**的配对差值（"走精英那条 vs 绕开那条"）。
///
/// [`paired_act_delta`] 挡住路线不同的那一对，理由是"路线一换就不是同一个问题了"
/// —— 那条挡对绝大多数调用是对的（比牌组的时候路线必须固定）。
/// **但"走哪条路"本身就是一个决策**，而它恰好只能由两条不同的 `rooms` 表达。
///
/// **这一对上 CRN 共享到哪为止，要知道**：
///
/// | 共享 | 不共享 |
/// |---|---|
/// | 这一幕**抽到哪几场遭遇**（[`seq_seed`] 只吃 `(base, i)`）| 第 k 间的敌人血量 —— [`fight_seed`] 吃 `k`，而**同一场仗在两条路线上的 k 不同** |
///
/// 所以它比"同路线换牌组"那一对**噪声大**：共享的那一维还在（两条路线面对的是
/// 同一批遭遇，这正是要的），但逐场血量那一维在路线错位的那一刻就分岔了。
/// 读它要看**一批起点上的方向**，别拿单次的正负当结论。
pub fn paired_act_delta_across_routes(a: &ActEval, b: &ActEval) -> Option<ActDelta> {
    if a.refused.is_some() || b.refused.is_some() || a.n != b.n || a.n == 0 || a.seed != b.seed {
        return None;
    }
    let mut d: Vec<i32> = Vec::new();
    let (mut b_better, mut a_better, mut tie) = (0, 0, 0);
    let (mut only_a_died, mut only_b_died) = (0, 0);
    let mut pairs = 0;
    for i in 0..a.n {
        let (x, y) = (&a.samples[i], &b.samples[i]);
        if x.truncated_at.is_some() || y.truncated_at.is_some() {
            continue;
        }
        pairs += 1;
        match (x.died_at.is_some(), y.died_at.is_some()) {
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
    Some(ActDelta {
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

/// [`paired_act_delta`] 的结果。字段和单场的 `PairedDelta` 逐个同义 ——
/// 两层读同一套数，不另立一套词。
pub struct ActDelta {
    pub pairs: usize,
    /// B 死的次数减 A 死的次数（**负 = B 更安全**）
    pub deaths_delta: i32,
    /// **只有 A 死**的链数 —— B 在这条链上救回了一整幕
    pub only_a_died: usize,
    pub only_b_died: usize,
    /// **两边都走完**的那些链的终点血量差（B − A）
    pub hp: Dist,
    pub b_better: usize,
    pub a_better: usize,
    pub tie: usize,
}

/// 把基础打击/防御全升一遍 —— **单调性自检**的那副牌组。
///
/// 和 `bin/fight_eval` 的 `--upgrade-check` 是同一副牌：只动这两张，
/// 因为它们的升级**无条件更好**（5 -> 8），而"每张牌都升"里混着
/// 改费用、改关键字的那些，方向不干净。
pub fn upgrade_basics(deck: &[crate::synth::DeckCard]) -> Vec<crate::synth::DeckCard> {
    deck.iter()
        .map(|c| {
            if c.id == card::STRIKE || c.id == card::DEFEND {
                crate::synth::DeckCard { flags: c.flags | crate::state::F_UPGRADED, ..*c }
            } else {
                *c
            }
        })
        .collect()
}
