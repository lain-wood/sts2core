//! **L3 阶段 4：构筑顾问的实战入口** —— 一份 JSON 进去，一份人读的报告出来。
//!
//! ```text
//! 游戏 ──HTTP──► tools/advise_core.py ──JSON/stdin──► 本程序 ──► 报告 ──► MCP 工具的返回值
//!                （脏活：读状态、认牌名、挑候选）      （L3：evaluate_act / evaluate + 配对比较）
//! ```
//!
//! 和 `bin/solve --live` 走的是同一条路（独立可执行文件 + JSON/stdio），理由
//! 在 `../CLAUDE.md` 的「构建与环境」：这台机器是 GNU 工具链，PyO3 过不去，
//! 而边界调用本来就少。
//!
//! # 它和前四个台子的分工
//!
//! `synth_audit` / `fight_eval` / `act_eval` 是**验收台**：测试集是实录，
//! 判据是红不红。这个不是台子，它是**生产入口** —— 测试集是"现在这一局"，
//! 没有红绿，只有一份带着缺口清单的报告。所以它**不进那九条验收**。
//!
//! # 一条命令答四个问题，靠的是"候选"这一个概念
//!
//! 拿牌 / 移除 / 升级 / 走不走精英，在 L3 眼里是同一件事：
//! **一副基准牌组 + 若干个改动，在同一批随机上配对比**。所以请求里只有
//! [`Cand`] 一种东西（加几张 · 去几张 · 升几张 · 换条路线），
//! 四个 MCP 工具都由 Python 侧翻译成它。
//!
//! # 三条它**故意**不做
//!
//! | 不做 | 为什么 |
//! |---|---|
//! | 不认牌名以外的任何游戏文本 | 牌名 -> 内核的牌只有 `synth::card_from_name` 一处实现（和 `replay::sync` 共用），在这里再写一份就是两份 |
//! | 不建路线 | 路线图第一刀就划在外面（`synth::act` 模块头）。`rooms` 由调用方给，**回显在报告最上面** |
//! | 不挑候选 | "牌奖励屏上有哪三张"是局外信息，Python 侧读得到，内核读不到 |
//!
//! # 报告的规矩：**死亡率在前，血量在后，缺口和结论并排**
//!
//! 这三条是路线图给 L3 定的，不是排版偏好：
//! 单场 ΔHP 在死亡处被截断（旧 advisor 的老毛病）· 覆盖率不是 100% 而
//! 「这次评估漏掉了几场」正比于结论的乐观程度 · 构造器自己报的缺口不往上传，
//! 报出来的就是一个自信的数。

use std::io::Read;
use std::process::ExitCode;

use sts2core::json::{parse, Json};
use sts2core::ops::Kind;
use sts2core::synth::act::{
    evaluate_act, paired_act_delta, paired_act_delta_across_routes, parse_rooms, ActCfg, ActDelta,
    ActEval, ActPlan, Room,
};
use sts2core::synth::encounters::Table;
use sts2core::synth::eval::{
    evaluate, paired_delta, Dist, EvalCfg, FightEval, PairedDelta, EVAL_MAX_TURNS,
};
use sts2core::synth::{card_from_name, DeckCard, EnemySpec, FightSpec, Gap, RelicSpec};

/// 默认路线。**和 `bin/act_eval::DEFAULT_ROOMS` 是同一个 `[判断]`**
/// （8 杂兵 + 1 精英 + 3 休息 + Boss），调用方几乎总是该给一条真的。
const DEFAULT_ROOMS: &str = "MMRMMMREMMRB";

/// 默认采样数。比两个验收台的 64 大一档：**死亡率是这套东西里噪声最大的那一维**
/// （`synth::eval` 模块头量过），而实战问的正是它，而且一条链只要几毫秒。
const DEFAULT_CHAINS: usize = 256;

// ---------------------------------------------------------------------------
// 请求
// ---------------------------------------------------------------------------

/// 牌组里的一张牌 + 它的名字（内核只认 id，报告要认名字）。
struct Named {
    card: DeckCard,
    name: String,
}

/// 一个候选改动。**四个 MCP 工具都翻译成它**，见模块头。
struct Cand {
    label: String,
    add: Vec<Named>,
    remove: Vec<usize>,
    upgrade: Vec<usize>,
    rooms: Option<Vec<Room>>,
    /// 它自己知道的毛病（下标越界 / 已经升过了 / 内核不认识这张牌）。
    /// **非空就不评分** —— 给一个"改了等于没改"的差值比不给更糟
    problems: Vec<String>,
}

/// 问哪一问。**三个问法共用同一份请求**（牌组/遗物/血量那一大半是一样的）。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Question {
    /// 走完这一幕会不会死（L3 的判决量）
    Act,
    /// 这一场仗打完会怎样（从**开局**打，不是从当前局面续）
    Fight,
    /// **一次推演都不跑**：内核看这副牌组/这些遗物/这些药水，认出了什么、缺什么
    Deck,
}

struct Req {
    question: Question,
    act: String,
    rooms: Vec<Room>,
    /// 这条路线**是怎么来的**（"调用方给的" / "默认那条 `[判断]`"）。
    /// 调用方可以用 `rooms_note` 说得更细 —— 它比内核更知道这条路线是怎么定的
    /// （从地图上数的、还是照默认那条截的）
    rooms_src: String,
    double_boss: bool,
    pin_boss: Option<String>,
    pin_second_boss: Option<String>,
    /// `fight` 模式：打哪一场（遭遇 key），和 `enemies` 二选一
    encounter: Option<String>,
    enemy_names: Vec<String>,
    boss_room: bool,
    after_rest: bool,
    hp: i32,
    max_hp: i32,
    ascension: u8,
    base_energy: i32,
    potion_slots: u8,
    potions: Vec<u8>,
    /// 请求里那几个字符串原文 —— 内核不认得的那瓶要**报出它的原文**，
    /// 不然"认不得"和"空槽"在报告里长得一样
    potion_names: Vec<String>,
    relics: Vec<(String, Option<i32>)>,
    deck: Vec<Named>,
    cands: Vec<Cand>,
    samples: usize,
    max_turns: usize,
    seed: u64,
    /// 报告里印几行候选（0 = 全印）。**它只管印，不管算** ——
    /// 每条候选都评过了，少印的那几条会在末尾报一句"还有 N 条更差的"
    top: usize,
    note: Option<String>,
}

fn as_u64(j: Option<&Json>) -> Option<u64> {
    j.and_then(Json::as_i64).map(|v| v as u64)
}

/// 一张牌：`{"name": "打击+", "upgraded": true, "enchant_id": "...", "enchant_amount": 2}`
/// 或者光一个字符串。
///
/// `upgraded` 不给就看名字末尾那个 `+` —— **和 `synth::from_obs` 对牌堆里的牌
/// 同一条回退**（观测里牌堆的牌没有 `is_upgraded` 字段，约束 3）。
fn parse_card(j: &Json) -> Option<Named> {
    if let Some(s) = j.as_str() {
        return Some(Named { card: card_from_name(s, s.ends_with('+'), "", 0), name: s.to_string() });
    }
    let name = j.str("name")?.to_string();
    let up = j.get("upgraded").and_then(Json::as_bool).unwrap_or_else(|| name.ends_with('+'));
    let eid = j.str("enchant_id").unwrap_or("");
    let amt = j.i64("enchant_amount").unwrap_or(0) as i32;
    Some(Named { card: card_from_name(&name, up, eid, amt), name })
}

fn parse_cards(j: Option<&Json>) -> Vec<Named> {
    j.and_then(Json::as_arr).map(|a| a.iter().filter_map(parse_card).collect()).unwrap_or_default()
}

fn parse_ix(j: Option<&Json>, n: usize, what: &str, problems: &mut Vec<String>) -> Vec<usize> {
    let mut out = Vec::new();
    for v in j.and_then(Json::as_arr).unwrap_or(&[]) {
        match v.as_i64() {
            Some(i) if i >= 0 && (i as usize) < n => out.push(i as usize),
            Some(i) => problems.push(format!("{what}：牌组里没有第 {i} 张（牌组 {n} 张）")),
            None => problems.push(format!("{what}：下标要是个数，给的是 {v:?}")),
        }
    }
    out
}

fn parse_req(src: &str) -> Result<Req, String> {
    let j = parse(src).map_err(|e| format!("请求不是合法 JSON：{e}"))?;
    let deck = parse_cards(j.get("deck"));
    let question = match j.str("question").unwrap_or("act") {
        "act" => Question::Act,
        "fight" => Question::Fight,
        "deck" => Question::Deck,
        other => return Err(format!("`question` 只认 act / fight / deck，给的是 `{other}`")),
    };
    let (rooms_str, rooms_src) = match j.str("rooms") {
        Some(r) if !r.trim().is_empty() => (r.to_string(), "调用方给的"),
        _ => (DEFAULT_ROOMS.to_string(), "**默认那条 `[判断]`**"),
    };
    let rooms_src = j.str("rooms_note").unwrap_or(rooms_src).to_string();
    let rooms = parse_rooms(&rooms_str).map_err(|e| format!("`rooms` 解析不了：{e}"))?;

    let mut cands = Vec::new();
    for (i, c) in j.get("candidates").and_then(Json::as_arr).unwrap_or(&[]).iter().enumerate() {
        let mut problems = Vec::new();
        let label = c.str("label").unwrap_or("").to_string();
        let label = if label.is_empty() { format!("候选 {}", i + 1) } else { label };
        let remove = parse_ix(c.get("remove"), deck.len(), "移除", &mut problems);
        let upgrade = parse_ix(c.get("upgrade"), deck.len(), "升级", &mut problems);
        for &ix in &upgrade {
            // **已经升过的牌再升一次是空操作** —— 那会给出一个"差 0"的自信答案。
            if deck[ix].card.flags & sts2core::state::F_UPGRADED != 0 {
                problems.push(format!("第 {ix} 张「{}」已经是升级版了", deck[ix].name));
            }
            if deck[ix].card.id == sts2core::content::card::UNKNOWN {
                problems.push(format!("第 {ix} 张「{}」内核不认识，升级它评不出任何差别", deck[ix].name));
            }
        }
        let rooms = match c.str("rooms") {
            Some(r) if !r.trim().is_empty() => match parse_rooms(r) {
                Ok(v) => Some(v),
                Err(e) => {
                    problems.push(format!("这条候选的 `rooms` 解析不了：{e}"));
                    None
                }
            },
            _ => None,
        };
        let add = parse_cards(c.get("add"));
        // **已经有毛病就不再补这一条** —— 下标越界的候选当然"什么都没改"，
        // 两条一起报会让人以为是两个毛病。
        if problems.is_empty()
            && add.is_empty()
            && remove.is_empty()
            && upgrade.is_empty()
            && rooms.is_none()
        {
            problems.push("这条候选什么都没改".to_string());
        }
        cands.push(Cand { label, add, remove, upgrade, rooms, problems });
    }

    let mut relics = Vec::new();
    for r in j.get("relics").and_then(Json::as_arr).unwrap_or(&[]) {
        if let Some(s) = r.as_str() {
            relics.push((s.to_string(), None));
        } else if let Some(id) = r.str("id") {
            relics.push((id.to_string(), r.i64("counter").map(|v| v as i32)));
        }
    }

    let mut potions = vec![0u8; sts2core::state::MAX_POTIONS];
    let mut potion_names = vec![String::new(); sts2core::state::MAX_POTIONS];
    for (i, p) in j.get("potions").and_then(Json::as_arr).unwrap_or(&[]).iter().enumerate() {
        if i >= potions.len() {
            break;
        }
        // 空槽在请求里是 `null` 或 `""` —— 两个都落到 `potion::NONE`
        let key = p.as_str().unwrap_or("");
        if !key.is_empty() {
            potions[i] = sts2core::replay::map_potion(key);
            potion_names[i] = key.to_string();
        }
    }

    let max_hp = j.i64("max_hp").unwrap_or(0) as i32;
    let hp = j.i64("hp").unwrap_or(max_hp as i64) as i32;
    if hp <= 0 || max_hp <= 0 {
        return Err("`hp` / `max_hp` 缺了或者不是正数 —— 没有血量就没有这一问".to_string());
    }
    if deck.is_empty() {
        return Err("`deck` 是空的 —— 这一层评的就是牌组".to_string());
    }
    Ok(Req {
        question,
        act: j.str("act").unwrap_or("").to_string(),
        rooms,
        rooms_src,
        double_boss: j.get("double_boss").and_then(Json::as_bool).unwrap_or(false),
        pin_boss: j.str("boss").map(str::to_string),
        pin_second_boss: j.str("second_boss").map(str::to_string),
        encounter: j.str("encounter").map(str::to_string),
        enemy_names: j
            .get("enemies")
            .and_then(Json::as_arr)
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
        boss_room: j.get("boss_room").and_then(Json::as_bool).unwrap_or(false),
        after_rest: j.get("after_rest").and_then(Json::as_bool).unwrap_or(false),
        hp,
        max_hp,
        ascension: j.i64("ascension").unwrap_or(0).clamp(0, 10) as u8,
        base_energy: j.i64("base_energy").unwrap_or(sts2core::solver::BASE_ENERGY as i64) as i32,
        potion_slots: j.i64("potion_slots").unwrap_or(3).clamp(1, 10) as u8,
        potions,
        potion_names,
        relics,
        deck,
        cands,
        samples: j.i64("samples").map(|v| v.max(1) as usize).unwrap_or(DEFAULT_CHAINS),
        max_turns: j.i64("max_turns").map(|v| v.max(1) as usize).unwrap_or(EVAL_MAX_TURNS),
        seed: as_u64(j.get("seed")).unwrap_or(0xA10D_0000_5EED),
        top: j.i64("top").map(|v| v.max(0) as usize).unwrap_or(0),
        note: j.str("note").map(str::to_string),
    })
}

/// 候选改动落到一副真牌组上。**顺序是 去 -> 升 -> 加**（去和升都按基准牌组的
/// 下标，所以要先取完再重排）。
fn apply(base: &[Named], c: &Cand) -> Vec<DeckCard> {
    let mut out = Vec::with_capacity(base.len() + c.add.len());
    for (ix, n) in base.iter().enumerate() {
        if c.remove.contains(&ix) {
            continue;
        }
        let mut card = n.card;
        if c.upgrade.contains(&ix) {
            card.flags |= sts2core::state::F_UPGRADED;
        }
        out.push(card);
    }
    out.extend(c.add.iter().map(|n| n.card));
    out
}

// ---------------------------------------------------------------------------
// 报告
// ---------------------------------------------------------------------------

fn pct(x: f64) -> String {
    format!("{:.0}%", x * 100.0)
}

/// 一串字符在**等宽终端里**占几格。中日韩的字占两格，而 `{:<26}` 数的是字符数
/// —— 报告里的候选名几乎全是中文，不算这个整张表就是歪的。
///
/// 判据取 Unicode 的 East Asian Wide/Fullwidth 那几段（够这份报告用，
/// 不是一份完整的实现 —— 完整的那份要一张表，而这里只排一列名字）。
fn width(s: &str) -> usize {
    s.chars()
        .map(|c| {
            let u = c as u32;
            let wide = (0x1100..=0x115F).contains(&u)
                || (0x2E80..=0xA4CF).contains(&u)
                || (0xAC00..=0xD7A3).contains(&u)
                || (0xF900..=0xFAFF).contains(&u)
                || (0xFE30..=0xFE6F).contains(&u)
                || (0xFF00..=0xFF60).contains(&u)
                || (0xFFE0..=0xFFE6).contains(&u);
            if wide {
                2
            } else {
                1
            }
        })
        .sum()
}

/// 左对齐补到 `w` 格宽（按 [`width`] 算）。
fn pad(s: &str, w: usize) -> String {
    let mut out = s.to_string();
    out.push_str(&" ".repeat(w.saturating_sub(width(s))));
    out
}

/// 一条候选的结论行。`ActDelta` 和 `PairedDelta` 字段逐个同义（两层读同一套词），
/// 所以这里只抄成一份中间结构，报告不分两套。
struct Row {
    label: String,
    /// B（候选）死的次数减 A（基准）死的次数，**负 = 候选更安全**
    deaths_delta: i32,
    only_base_died: usize,
    only_cand_died: usize,
    hp: Dist,
    note: Option<String>,
    /// 主信号是 0 时才印的次要读数，见 [`Fallback`]
    fallback: Option<(Fallback, bool)>,
}

impl Row {
    fn from_act(label: &str, d: &ActDelta, note: Option<String>, fb: Fallback) -> Row {
        Row {
            label: label.to_string(),
            deaths_delta: d.deaths_delta,
            only_base_died: d.only_a_died,
            only_cand_died: d.only_b_died,
            hp: d.hp,
            note,
            fallback: Some((fb, true)),
        }
    }
    fn from_fight(label: &str, d: &PairedDelta, note: Option<String>, fb: Fallback) -> Row {
        Row {
            label: label.to_string(),
            deaths_delta: d.deaths_delta,
            only_base_died: d.only_a_died,
            only_cand_died: d.only_b_died,
            hp: d.hp,
            note,
            fallback: Some((fb, false)),
        }
    }
}

/// **两边都死的那些样本上的次要读数：谁离赢更近。**
///
/// 主信号（死亡率差）在**基准必死**的牌组上恒等于 0 —— 那正是旧 advisor
/// 「ΔHP 在死亡处被截断」那条老毛病换了个地方犯：排名又一次在最要紧的时候失声。
/// 这里的出路不是把两个量混成一个分数（那是老毛病本身），而是**另开一栏**：
///
/// * 整幕：候选**死得更靠后**（多走了几间）的对数
/// * 单场：候选结束时**敌人剩的血更少**（差一口气 vs 打不动）
///
/// **它只在主信号是 0 的时候印** —— 有死亡率差的时候看它是舍本逐末。
struct Fallback {
    /// 候选更好（死得更深 / 敌人剩血更少）的对数
    better: usize,
    worse: usize,
    /// 单场：敌人剩血差的均值（B − A，**负 = 候选离赢更近**）
    hp_left: f64,
    n: usize,
}

impl Fallback {
    fn line(&self, act: bool) -> String {
        if self.n == 0 {
            return String::new();
        }
        if act {
            format!("两边都死的 {} 条链里：候选走得更深 {} / 更浅 {}", self.n, self.better, self.worse)
        } else {
            format!(
                "两边都死的 {} 次里：敌人剩血 {:+.0}（负 = 离赢更近），更近 {} / 更远 {}",
                self.n, self.hp_left, self.better, self.worse
            )
        }
    }
}

/// 整幕：两边都死的链上比**死在第几间**。
fn act_fallback(a: &ActEval, b: &ActEval) -> Fallback {
    let (mut better, mut worse, mut n) = (0, 0, 0);
    for i in 0..a.n.min(b.n) {
        let (x, y) = (&a.samples[i], &b.samples[i]);
        if x.truncated_at.is_some() || y.truncated_at.is_some() {
            continue;
        }
        let (Some(xa), Some(yb)) = (x.died_at, y.died_at) else { continue };
        n += 1;
        match yb.cmp(&xa) {
            std::cmp::Ordering::Greater => better += 1,
            std::cmp::Ordering::Less => worse += 1,
            std::cmp::Ordering::Equal => {}
        }
    }
    Fallback { better, worse, hp_left: 0.0, n }
}

/// 单场：两边都死的样本上比**敌人还剩多少血**。
fn fight_fallback(a: &FightEval, b: &FightEval) -> Fallback {
    let (mut better, mut worse, mut n, mut sum) = (0, 0, 0usize, 0i64);
    for i in 0..a.n.min(b.n) {
        let (x, y) = (&a.samples[i], &b.samples[i]);
        if x.truncated || y.truncated || !x.died || !y.died {
            continue;
        }
        n += 1;
        sum += (y.enemy_hp_left - x.enemy_hp_left) as i64;
        match y.enemy_hp_left.cmp(&x.enemy_hp_left) {
            std::cmp::Ordering::Less => better += 1,
            std::cmp::Ordering::Greater => worse += 1,
            std::cmp::Ordering::Equal => {}
        }
    }
    Fallback { better, worse, hp_left: if n == 0 { 0.0 } else { sum as f64 / n as f64 }, n }
}

/// **「这个比较没有信号」要写出来，不能让读者从一行 0 里自己猜。**
///
/// 旧 advisor 那一栏叫 `signal=no`，父目录的 `CLAUDE.md` 专门教过怎么读它：
/// 一场两边都死（或者两边都毫发无伤）的仗里每个候选都是 0，
/// **那个 0 是"看不见"不是"没用"** —— 和药水拒绝定价是同一条规矩。
fn no_signal(r: &Row) -> String {
    let quiet = r.deaths_delta == 0
        && r.only_base_died == 0
        && r.only_cand_died == 0
        && (r.hp.n == 0 || (r.hp.min == 0 && r.hp.max == 0));
    if !quiet {
        return String::new();
    }
    match &r.fallback {
        Some((fb, act)) if fb.n > 0 => format!("死亡/血量都没差 —— {}", fb.line(*act)),
        _ => "**没有信号**：每个样本两边结局相同".to_string(),
    }
}

/// **排序：死亡在前，血量在后。** 和 `paired_*_delta` 拆开死亡与血量是同一条 ——
/// 一场两边都会死的仗里血量差塌向 0，那时候排名会恰好在最要紧的地方失声。
fn rank(rows: &mut [Row]) {
    rows.sort_by(|a, b| {
        a.deaths_delta
            .cmp(&b.deaths_delta)
            .then(b.hp.mean.partial_cmp(&a.hp.mean).unwrap_or(std::cmp::Ordering::Equal))
    });
}

fn print_rows(rows: &[Row], n_samples: usize, top: usize) {
    if rows.is_empty() {
        return;
    }
    println!(
        "\n候选（**负 = 比基准好**；死亡那一列在前，血量只在两边都活下来的样本上算）："
    );
    println!(
        "  {} {:>11}  {:>13}  {:>16}  {}",
        pad("候选", 30),
        "Δ死亡",
        "救回/反送命",
        "血量差(均值/n)",
        "备注"
    );
    let shown = if top == 0 { rows.len() } else { top.min(rows.len()) };
    for r in &rows[..shown] {
        let dr = r.deaths_delta as f64 / n_samples as f64;
        let hp = if r.hp.n == 0 {
            "—".to_string()
        } else {
            format!("{:+.1} / {}", r.hp.mean, r.hp.n)
        };
        println!(
            "  {} {:>4} ({:>4})  {:>6} / {:<4}  {:>16}  {}",
            pad(&r.label, 30),
            format!("{:+}", r.deaths_delta),
            pct(dr),
            r.only_base_died,
            r.only_cand_died,
            hp,
            r.note.clone().unwrap_or_else(|| no_signal(r)),
        );
    }
    if shown < rows.len() {
        println!("  （还有 {} 条更差的没印 —— 它们都评过了，`top` 只管印几行）", rows.len() - shown);
    }
}

/// 报告尾巴：**已知偏差 + 缺口**，两样都必须和结论并排印。
fn print_caveats(caveats: &[sts2core::synth::eval::Caveat], gaps: &[Gap], missing: Option<f64>) {
    for c in caveats {
        println!("  ~ {c}");
    }
    if let Some(m) = missing {
        println!(
            "  ~ 平均每条链**漏掉 {m:.1} 场**（抽到了但内核开不出来）—— 那几场没挨打，\n    **上面每个死亡率因此都是乐观的**"
        );
    }
    for g in gaps {
        println!("  · 缺口 {g}");
    }
}

fn deck_summary(deck: &[Named]) {
    let unknown: Vec<&str> = deck
        .iter()
        .filter(|n| n.card.id == sts2core::content::card::UNKNOWN)
        .map(|n| n.name.as_str())
        .collect();
    let powers =
        deck.iter().filter(|n| matches!(sts2core::content::card(n.card.id).kind, Kind::Power)).count();
    println!(
        "牌组 {} 张（能力牌 {powers} 张{}）",
        deck.len(),
        if unknown.is_empty() {
            String::new()
        } else {
            format!("，**内核不认识 {} 张：{}**", unknown.len(), unknown.join(" / "))
        }
    );
}

// ---------------------------------------------------------------------------
// 两种问法
// ---------------------------------------------------------------------------

fn run_act(req: &Req, table: &Table) -> ExitCode {
    let relics: Vec<RelicSpec> =
        req.relics.iter().map(|(id, c)| RelicSpec { id: id.as_str(), counter: *c }).collect();
    let base_deck: Vec<DeckCard> = req.deck.iter().map(|n| n.card).collect();
    let spec = FightSpec {
        deck: &base_deck,
        relics: &relics,
        hp: req.hp,
        max_hp: req.max_hp,
        start_status: &[],
        after_rest: req.after_rest,
        boss_room: false,
        enemies: &[],
        potions: &req.potions,
        potion_slots: req.potion_slots,
        base_energy: req.base_energy,
        ascension: req.ascension,
        seed: req.seed,
    };
    let plan = ActPlan {
        act: &req.act,
        rooms: &req.rooms,
        double_boss: req.double_boss,
        pin_boss: req.pin_boss.as_deref(),
        pin_second_boss: req.pin_second_boss.as_deref(),
    };
    let cfg = ActCfg { samples: req.samples, max_turns: req.max_turns, ..ActCfg::default() };

    println!(
        "幕 {} · 路线 `{}`（{} 间：杂兵 {} · 精英 {} · Boss {} · 休息 {}）—— {}",
        req.act,
        req.rooms.iter().map(Room::letter).collect::<String>(),
        req.rooms.len(),
        req.rooms.iter().filter(|r| **r == Room::Monster).count(),
        req.rooms.iter().filter(|r| **r == Room::Elite).count(),
        req.rooms.iter().filter(|r| **r == Room::Boss).count(),
        req.rooms.iter().filter(|r| **r == Room::Rest).count(),
        req.rooms_src,
    );
    match &req.pin_boss {
        Some(b) => println!("Boss **钉死为 {b}**（地图上写着的那只）"),
        None => println!("Boss **没钉** —— 从这一幕的 Boss 池里均匀掷，方差比实战大"),
    }
    deck_summary(&req.deck);
    println!(
        "血量 {}/{} · A{} · 遗物 {} 件 · 每条链走 {} 次 · 种子基 {:#x}",
        req.hp,
        req.max_hp,
        req.ascension,
        req.relics.len(),
        req.samples,
        req.seed
    );

    let t0 = std::time::Instant::now();
    let base = evaluate_act(&spec, &plan, table, &cfg);
    if let Some(r) = &base.refused {
        println!("\n[拒绝作答] {r}");
        return ExitCode::from(1);
    }
    println!("\n[基准] 不动牌组：{}", base.headline());
    if base.deaths > 0 {
        let by: Vec<String> = base
            .deaths_by_room
            .iter()
            .enumerate()
            .filter(|(_, n)| **n > 0)
            .map(|(k, n)| format!("#{}{}={}", k + 1, base.rooms[k].letter(), n))
            .collect();
        println!("       死在：{}", by.join(" "));
    }
    if base.truncated > 0 {
        println!(
            "       **截断 {}/{} 条链** —— 那几条没分出胜负，既不算死也不算活（已经从上面每个分布里扣掉）",
            base.truncated, base.n
        );
    }

    let mut rows = Vec::new();
    let mut refused = Vec::new();
    for c in &req.cands {
        if !c.problems.is_empty() {
            refused.push(format!("{}：{}", c.label, c.problems.join("；")));
            continue;
        }
        let deck = apply(&req.deck, c);
        let rooms = c.rooms.as_deref().unwrap_or(&req.rooms);
        let cplan = ActPlan { rooms, ..plan };
        let ev = evaluate_act(&FightSpec { deck: &deck, ..spec }, &cplan, table, &cfg);
        if let Some(r) = &ev.refused {
            refused.push(format!("{}：{r}", c.label));
            continue;
        }
        // **换路线的那一对走另一个入口**，它比"同路线换牌组"噪声大，
        // 理由在 `paired_act_delta_across_routes` 的文档里。
        let (d, note) = if c.rooms.is_some() {
            (
                paired_act_delta_across_routes(&base, &ev),
                Some(format!(
                    "换路线 `{}`（逐场血量那一维不共享，噪声更大）",
                    rooms.iter().map(Room::letter).collect::<String>()
                )),
            )
        } else {
            (paired_act_delta(&base, &ev), None)
        };
        match d {
            Some(d) => rows.push(Row::from_act(&c.label, &d, note, act_fallback(&base, &ev))),
            None => refused.push(format!("{}：配对前提不成立（种子基/样本数对不上）", c.label)),
        }
    }
    rank(&mut rows);
    print_rows(&rows, req.samples, req.top);
    for r in &refused {
        println!("  [不评分] {r}");
    }

    println!("\n-- 这次评估自己知道的毛病（**和上面每个数一起读**）：");
    println!(
        "  ~ 这一幕内核开得出 {}/{} 场遭遇",
        base.coverage.0, base.coverage.1
    );
    print_caveats(&base.caveats(), &base.gaps, base.missing_fights());
    for s in &base.skipped {
        println!("  · 没打成 {s}");
    }
    println!(
        "  ~ **报的是「这副牌组在我们这套策略手里」的成色**：引擎牌（每回合给格挡的 /\n    会衰减的 / 触发式的）在跨回合搜索里评不到 ⇒ 带引擎的牌组被**低估**；\n    回溯检验量到我们这套策略**每场比玩家多掉 6.9 血**，一幕几场就是几倍。"
    );
    println!("\n({:.1} 秒)", t0.elapsed().as_secs_f64());
    ExitCode::SUCCESS
}

fn run_fight(req: &Req, table: Option<&Table>) -> ExitCode {
    // 敌人两个来源：遭遇 key（查表，**构成含随机的整条拒绝**）或者直接给名字。
    let (enemies, what) = match (&req.encounter, table) {
        (Some(key), Some(t)) => match t.resolve(key) {
            Ok(e) => (e, key.clone()),
            Err(why) => {
                println!("[拒绝作答] 这一场开不出来：{why}");
                return ExitCode::from(1);
            }
        },
        (Some(_), None) => {
            println!("[拒绝作答] 给了遭遇 key 但遭遇表读不了");
            return ExitCode::from(2);
        }
        (None, _) => {
            let mut v = Vec::new();
            let mut unknown = Vec::new();
            for n in &req.enemy_names {
                match sts2core::replay::enemy_id(n) {
                    Some(def) => v.push(EnemySpec::rolled(def)),
                    None => unknown.push(n.clone()),
                }
            }
            if !unknown.is_empty() {
                println!(
                    "[拒绝作答] 内核不认识这几只敌人：{} —— 推演会是一场它们不出手的仗",
                    unknown.join(" / ")
                );
                return ExitCode::from(1);
            }
            if v.is_empty() {
                println!("[拒绝作答] 一只敌人都没给（`encounter` 或 `enemies` 二选一）");
                return ExitCode::from(2);
            }
            (v, req.enemy_names.join(" + "))
        }
    };

    let relics: Vec<RelicSpec> =
        req.relics.iter().map(|(id, c)| RelicSpec { id: id.as_str(), counter: *c }).collect();
    let base_deck: Vec<DeckCard> = req.deck.iter().map(|n| n.card).collect();
    let boss_room = req.boss_room
        || req.encounter.as_deref().map_or(false, |k| table.map_or(false, |t| t.is_boss(k)));
    let spec = FightSpec {
        deck: &base_deck,
        relics: &relics,
        hp: req.hp,
        max_hp: req.max_hp,
        start_status: &[],
        after_rest: req.after_rest,
        boss_room,
        enemies: &enemies,
        potions: &req.potions,
        potion_slots: req.potion_slots,
        base_energy: req.base_energy,
        ascension: req.ascension,
        seed: req.seed,
    };
    let cfg = EvalCfg { samples: req.samples, max_turns: req.max_turns, ..EvalCfg::default() };

    println!("这一场：{what}");
    deck_summary(&req.deck);
    println!(
        "血量 {}/{} · A{} · 遗物 {} 件 · 打 {} 次 · 种子基 {:#x}",
        req.hp,
        req.max_hp,
        req.ascension,
        req.relics.len(),
        req.samples,
        req.seed
    );
    println!(
        "**这是从开局重打这一场**（不是从当前局面续）—— 局内逐张出牌问 `tools/solve_now.py`。"
    );

    let t0 = std::time::Instant::now();
    let base: FightEval = evaluate(&spec, &cfg);
    if let Some(r) = &base.refused {
        println!("\n[拒绝作答] {r}");
        return ExitCode::from(1);
    }
    println!("\n[基准] 不动牌组：{}", base.headline());
    if base.truncated > 0 {
        println!(
            "       **截断 {}/{} 次** —— 那几次没分出胜负（已经从上面每个分布里扣掉）",
            base.truncated, base.n
        );
    }

    let mut rows = Vec::new();
    let mut refused = Vec::new();
    for c in &req.cands {
        if !c.problems.is_empty() {
            refused.push(format!("{}：{}", c.label, c.problems.join("；")));
            continue;
        }
        if c.rooms.is_some() {
            refused.push(format!("{}：单场问法里没有路线这回事", c.label));
            continue;
        }
        let deck = apply(&req.deck, c);
        let ev = evaluate(&FightSpec { deck: &deck, ..spec }, &cfg);
        if let Some(r) = &ev.refused {
            refused.push(format!("{}：{r}", c.label));
            continue;
        }
        match paired_delta(&base, &ev) {
            Some(d) => rows.push(Row::from_fight(&c.label, &d, None, fight_fallback(&base, &ev))),
            None => refused.push(format!("{}：配对前提不成立（种子基/样本数对不上）", c.label)),
        }
    }
    rank(&mut rows);
    print_rows(&rows, req.samples, req.top);
    for r in &refused {
        println!("  [不评分] {r}");
    }

    println!("\n-- 这次评估自己知道的毛病（**和上面每个数一起读**）：");
    print_caveats(&base.caveats(), &base.gaps, None);
    println!(
        "  ~ **单场看不见引擎牌的钱** —— 添柴/黑暗之拥那套要跨一整幕才收得回来。\n    要问「这副牌组走得完这一幕吗」，用 `question: act`。"
    );
    println!("\n({:.1} 秒)", t0.elapsed().as_secs_f64());
    ExitCode::SUCCESS
}

/// **`question: "deck"`：一次推演都不跑。**
///
/// 它答的是「内核**看见**了什么」—— 认不认得这些牌、这些遗物在合成路径上缺什么、
/// 药水认不认得。旧 advisor 的 `inspect_fight` / `snapshot_deck` 是为了同一件事
/// 存在的：**信任任何建议之前，先看它实际推断出了什么**。
///
/// 这里**不掷任何随机数**，所以它随手可跑（毫秒级），
/// 而且它报的东西一个都不依赖采样。
fn run_deck(req: &Req) -> ExitCode {
    let relics: Vec<RelicSpec> =
        req.relics.iter().map(|(id, c)| RelicSpec { id: id.as_str(), counter: *c }).collect();
    let base_deck: Vec<DeckCard> = req.deck.iter().map(|n| n.card).collect();
    // 敌人给空：`build` 照样把牌组、遗物、药水全过一遍，**缺口一条不少**
    // —— 它报的缺口只由 spec 决定（`eval::evaluate` 的探针也是这么用的）。
    let spec = FightSpec {
        deck: &base_deck,
        relics: &relics,
        hp: req.hp,
        max_hp: req.max_hp,
        start_status: &[],
        after_rest: req.after_rest,
        boss_room: req.boss_room,
        enemies: &[],
        potions: &req.potions,
        potion_slots: req.potion_slots,
        base_energy: req.base_energy,
        ascension: req.ascension,
        seed: req.seed,
    };
    let built = sts2core::synth::build(&spec);

    deck_summary(&req.deck);
    // 逐张按张数列出来：读者要认的是"这副牌组长什么样"，不是 24 行
    let mut counts: Vec<(String, usize)> = Vec::new();
    for n in &req.deck {
        match counts.iter_mut().find(|(k, _)| *k == n.name) {
            Some((_, c)) => *c += 1,
            None => counts.push((n.name.clone(), 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    println!(
        "  {}",
        counts
            .iter()
            .map(|(k, c)| if *c > 1 { format!("{k} ×{c}") } else { k.clone() })
            .collect::<Vec<_>>()
            .join(" · ")
    );

    println!("遗物 {} 件：{}", req.relics.len(), req.relics.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>().join(" · "));
    println!(
        "药水 {} 槽：{}",
        req.potion_slots,
        (0..req.potion_slots as usize)
            .map(|i| match req.potions.get(i).copied().unwrap_or(0) {
                sts2core::state::potion::NONE => "空槽".to_string(),
                // `potion_def` 对 `UNKNOWN` 退回 0 号（名字是「无」）——
                // 照它印会让"认不得"和"空槽"长得一样，所以这里报原文
                sts2core::state::potion::UNKNOWN =>
                    format!("**内核不认得**「{}」", req.potion_names[i]),
                p => sts2core::ops::potion_def(p).name.to_string(),
            })
            .collect::<Vec<_>>()
            .join(" · ")
    );
    println!("血量 {}/{} · A{}", req.hp, req.max_hp, req.ascension);

    if built.gaps.is_empty() {
        println!("
构造器**一条缺口都没报** —— 这副牌组和这几件遗物，内核开局那一刻全都做得到。");
    } else {
        println!("
构造器自己报的缺口（**「不认识」和「没效果」是两件事**）：");
        let mut seen: Vec<String> = Vec::new();
        for g in &built.gaps {
            let k = g.to_string();
            if !seen.contains(&k) {
                println!("  · {k}");
                seen.push(k);
            }
        }
    }
    println!(
        "
要问「这副牌组走得完这一幕吗」用 `question: act`，「这一场打得过吗」用 `question: fight`。"
    );
    ExitCode::SUCCESS
}

fn usage() {
    eprintln!("用法: advise <请求.json> | advise -   （`-` = 从 stdin 读）");
    eprintln!("  请求的形状见本文件的模块头；Python 侧的构造器是 tools/advise_core.py");
    eprintln!("  [--data 目录]  遭遇表在哪（默认 data）");
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        usage();
        return ExitCode::from(2);
    }
    let mut path: Option<String> = None;
    let mut data_dir = "data".to_string();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--data" => match it.next() {
                Some(v) => data_dir = v.clone(),
                None => {
                    eprintln!("--data 后面要跟一个目录");
                    return ExitCode::from(2);
                }
            },
            _ => path = Some(a.clone()),
        }
    }
    let Some(path) = path else {
        usage();
        return ExitCode::from(2);
    };
    let src = if path == "-" {
        let mut s = String::new();
        if let Err(e) = std::io::stdin().read_to_string(&mut s) {
            eprintln!("stdin 读不了：{e}");
            return ExitCode::from(2);
        }
        s
    } else {
        match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("读不了 {path}：{e}");
                return ExitCode::from(2);
            }
        }
    };
    let req = match parse_req(&src) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[请求不合法] {e}");
            return ExitCode::from(2);
        }
    };
    // **遭遇表读不了**：整幕链整条建在它上面 ⇒ 停下来；单场问法只有"按 key 认那一场"
    // 用得到它，给了名字就照跑。
    let table = match Table::load(&data_dir) {
        Ok(t) => Some(t),
        Err(e) => {
            eprintln!("[!] 遭遇表读不了（{e}）");
            None
        }
    };
    if let Some(n) = &req.note {
        println!("{n}");
    }
    match req.question {
        Question::Deck => return run_deck(&req),
        Question::Fight => return run_fight(&req, table.as_ref()),
        Question::Act => {}
    }
    let Some(table) = table else {
        eprintln!("整幕链没有遭遇表什么都做不了");
        return ExitCode::from(2);
    };
    if req.act.is_empty() {
        eprintln!("[请求不合法] `act` 是空的 —— 整幕链要知道是哪一幕（幕名，不是序号）");
        return ExitCode::from(2);
    }
    run_act(&req, &table)
}
