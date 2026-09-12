//! **合成战斗构造器的验收台** —— 拿实录的**第 0 帧**当测试集。
//!
//! ```text
//! cargo run --release --bin synth_audit -- traces/act*.json
//! cargo run --release --bin synth_audit -- traces/act*.json --all   # 连对上的也逐条列
//! ```
//!
//! # 它问的是一件既有验收全都答不了的事
//!
//! 七条既有验收全部从 `Replayer::sync` 出发 —— 局面是**从观测同步来的**。
//! L3 手上没有观测：它问的是「这副牌组走完这一幕会不会死」，那些仗还没打。
//! 所以 L3 的第一块地基是 `synth::build`：凭牌组/遗物/血量/遭遇**搭出**一场仗。
//!
//! 判据只有一条，而且现成：**同一场仗，构造器搭出来的局面，
//! 应该和 `sync` 从第 0 帧同步出来的局面逐字段相同。**
//! 第 0 帧是战斗刚开始那一刻（`round == 1`、弃牌堆和消耗堆都空），
//! 而 `sync` 那条路已经被 70 条实录逐帧钉过 —— 拿它当参照是**免费**的。
//!
//! ```text
//!            ┌── replay::sync(第0帧观测) ──► State ─┐
//! 实录第 0 帧 ┤                                      ├─► 逐字段比
//!            └── 抽出 spec ─► synth::build ──► State ┘
//! ```
//!
//! # 四栏读数，各自独占一类失败
//!
//! | 读数 | 独占什么 | 坏掉的样子 |
//! |---|---|---|
//! | **逐字段差** | 构造器（和它读的那几张表）缺什么 | 遗物给的 status 没挂上、开局手牌少一张、药水槽算错 |
//! | **遭遇识别** | `data/encounters.json` 认不认得这场仗 | L3 的整幕链会在这一幕漏掉几场遭遇，而它自己不知道 |
//! | **血量区间** | `asc::hp_range` 的区间对不对 | 实测血量落在区间外 = 两个来源打架；没有区间 = 那只敌人只能用定值 |
//! | **开局第一手** | `step::initial_move` 挑得对不对 | **`verify --predict-enemy` 结构上照不到**：它是拿观测到的第一个意图去*对齐*指针，开局那一手从来没被验过 |
//!
//! # 三样**结构上**不可比，不判红（列在这里，免得下一个人以为漏了）
//!
//! | 不比什么 | 为什么 |
//! |---|---|
//! | 手牌/抽牌堆里**具体是哪几张** | 洗牌是内核自己掷的（不变量 4：随机流故意和游戏不一致）。比的是**多重集**和张数 |
//! | `n_draw_known` | `sync` 整堆照抄 mod 报的真实牌序（补丁），构造器洗完是 0 —— 这是设计，不是缺陷 |
//! | `enemy_def` / `enemy_move` | `sync` 一律安 `enemy::UNKNOWN`（trace-format 约束 4）。开局第一手另立一栏比**意图签名** |
//!
//! # 跳过的那些
//!
//! * 抽牌堆只报了张数的老 trace —— 牌组凑不齐，**不猜**
//! * 第 0 帧不是开局的（`round != 1` / 弃牌堆非空）—— 那不是"一场仗的起点"

use std::collections::BTreeMap;
use std::process::ExitCode;

use sts2core::asc;
use sts2core::content::{enemy_def, relic_by_id};
use sts2core::replay::{
    move_signature, observed_signature, parse_trace, Obs, Replayer,
};
use sts2core::state::{St, State, MAX_POTIONS, N_STATUS};
use sts2core::step::{allowed_initial, current_move_ix};
use sts2core::synth::encounters::Table;
use sts2core::synth::from_obs::{build_start, extract, is_fight_start, probe_boss_room, Extracted};
use sts2core::synth::{card_ident, Gap};

/// 一处逐字段差。`game` 是 `sync` 出来的（观测那一侧），`kernel` 是构造器。
struct Diff {
    field: String,
    game: String,
    kernel: String,
}

fn d(field: impl Into<String>, game: impl ToString, kernel: impl ToString) -> Diff {
    Diff { field: field.into(), game: game.to_string(), kernel: kernel.to_string() }
}

/// 敌人血量：观测到的 `max_hp` 落不落在 `asc::hp_range` 的区间里。
enum HpVerdict {
    In,
    Out { got: i32, lo: i32, hi: i32 },
    NoRange,
    UnknownEnemy,
}

/// 开局第一手：游戏摆出来的那个意图，在不在内核的**开局允许集合**里。
///
/// **判据是集合不是逐字相等**，和 `verify --predict-enemy` 同一条规矩：
/// 好几只敌人的开局本来就是随机分支（[源码] `RandomBranchState` 当 initialState，
/// 或者一个 `_screamFirst` 那样的每只实例随机的标志），内核的随机流
/// 故意和游戏不一致（不变量 4），"开局是哪一手"本来就预测不了。
///
/// 集合越大越容易命中，所以**集合的平均大小和命中率必须一起读** ——
/// 报告里两个数并排印。
enum MoveVerdict {
    /// 内核默认挑的那一手就是游戏出的那一手（集合大小 1 时这才有含金量）
    Same,
    /// 不是默认那一手，但**在允许集合里**（开局随机分支的另一个代表元）
    InSet,
    /// 集合里有一手类型对得上、数字不对
    KindOnly { want: String, got: String },
    /// **落在集合外** —— 这才是真的缺口
    Outside { want: String, got: String },
    UnknownEnemy,
    NoIntent,
}

/// 遭遇识别的结局。
enum EncVerdict {
    Exact(String),
    Ambiguous(Vec<String>),
    Loose(Vec<String>),
    None,
    UnknownEnemy,
    NoTable,
}

struct Row {
    file: String,
    skip: Option<String>,
    /// 审计台**故意没喂给构造器**的遗物（见 `synth::from_obs::DECK_REWRITERS`）
    withheld: Vec<String>,
    diffs: Vec<Diff>,
    gaps: Vec<Gap>,
    enc: EncVerdict,
    hp: Vec<(String, HpVerdict)>,
    moves: Vec<(String, MoveVerdict)>,
    /// 每只敌人的**开局允许集合**有多大。命中率和它必须一起读
    move_set_sizes: Vec<usize>,
}

fn status_name(ix: usize) -> String {
    St::name_of_ix(ix).unwrap_or_else(|| format!("status[{ix}]"))
}

/// 逐字段比。**不可比的三样列在模块头**，这里只写"比了什么"。
///
/// `skip_st` 是**被扣下的那几件遗物**（`from_obs::DECK_REWRITERS`）挂的 status：
/// 台子自己没把遗物喂给构造器，那它们的标记两边对不上是**台子造成的**，
/// 不是构造器缺东西。扣了什么每条语料都印着，不是静默跳过。
fn compare(g: &State, k: &State, ex: &Extracted, skip_st: &[usize], out: &mut Vec<Diff>) {
    // ---- 玩家 ----
    if g.player.hp != k.player.hp {
        out.push(d("玩家 hp", g.player.hp, k.player.hp));
    }
    if g.player.max_hp != k.player.max_hp {
        out.push(d("玩家 max_hp", g.player.max_hp, k.player.max_hp));
    }
    if g.player.block != k.player.block {
        out.push(d("玩家 block", g.player.block, k.player.block));
    }
    for i in 0..N_STATUS {
        if skip_st.contains(&i) {
            continue;
        }
        let (a, b) = (g.player.status[i], k.player.status[i]);
        if a != b {
            out.push(d(format!("玩家 {}", status_name(i)), a, b));
        }
    }
    // ---- 资源 ----
    if g.energy != k.energy {
        out.push(d("energy", g.energy, k.energy));
    }
    if g.base_energy != k.base_energy {
        out.push(d("base_energy", g.base_energy, k.base_energy));
    }
    if g.turn != k.turn {
        out.push(d("turn", g.turn, k.turn));
    }
    if g.ascension != k.ascension {
        out.push(d("ascension", g.ascension, k.ascension));
    }
    if g.potion_slots != k.potion_slots {
        out.push(d("potion_slots", g.potion_slots, k.potion_slots));
    }
    for i in 0..MAX_POTIONS {
        if g.potions[i] != k.potions[i] {
            out.push(d(
                format!("药水槽 {i}"),
                sts2core::replay::potion_name(g.potions[i]),
                sts2core::replay::potion_name(k.potions[i]),
            ));
        }
    }
    // ---- 牌区 ----
    if g.n_cards != k.n_cards {
        out.push(d("n_cards", g.n_cards, k.n_cards));
    }
    if g.n_hand != k.n_hand {
        out.push(d("开局手牌张数", g.n_hand, k.n_hand));
    }
    if g.n_draw != k.n_draw {
        out.push(d("抽牌堆张数", g.n_draw, k.n_draw));
    }
    if g.n_disc != k.n_disc {
        out.push(d("弃牌堆张数", g.n_disc, k.n_disc));
    }
    if g.n_exh != k.n_exh {
        out.push(d("消耗堆张数", g.n_exh, k.n_exh));
    }
    // 牌的**多重集**（洗牌之后下标不可比，见模块头）
    let bag = |s: &State| {
        let mut m: BTreeMap<(u16, u8, i16, u8, i8), i32> = BTreeMap::new();
        for i in 0..s.n_cards as usize {
            *m.entry(card_ident(&s.cards[i])).or_insert(0) += 1;
        }
        m
    };
    let (bg, bk) = (bag(g), bag(k));
    if bg != bk {
        let mut keys: Vec<_> = bg.keys().chain(bk.keys()).cloned().collect();
        keys.sort();
        keys.dedup();
        for key in keys {
            let (a, b) = (bg.get(&key).copied().unwrap_or(0), bk.get(&key).copied().unwrap_or(0));
            if a != b {
                let name = ex
                    .deck_names
                    .iter()
                    .find(|n| sts2core::replay::lookup_card(n) == Some(key.0))
                    .cloned()
                    .unwrap_or_else(|| sts2core::content::card(key.0).name.to_string());
                out.push(d(
                    format!("牌「{name}」flags={} bonus={} ench={}/{}", key.1, key.2, key.3, key.4),
                    a,
                    b,
                ));
            }
        }
    }
    // ---- 敌人 ----
    if g.n_enemies != k.n_enemies {
        out.push(d("敌人数", g.n_enemies, k.n_enemies));
    }
    for e in 0..(g.n_enemies.min(k.n_enemies)) as usize {
        let name = ex.enemy_names.get(e).cloned().unwrap_or_else(|| format!("槽{e}"));
        if g.enemies[e].hp != k.enemies[e].hp {
            out.push(d(format!("敌人「{name}」hp"), g.enemies[e].hp, k.enemies[e].hp));
        }
        if g.enemies[e].max_hp != k.enemies[e].max_hp {
            out.push(d(format!("敌人「{name}」max_hp"), g.enemies[e].max_hp, k.enemies[e].max_hp));
        }
        if g.enemies[e].block != k.enemies[e].block {
            out.push(d(format!("敌人「{name}」block"), g.enemies[e].block, k.enemies[e].block));
        }
        for i in 0..N_STATUS {
            let (a, b) = (g.enemies[e].status[i], k.enemies[e].status[i]);
            if a != b {
                out.push(d(format!("敌人「{name}」{}", status_name(i)), a, b));
            }
        }
    }
    // ---- 杂项标志 ----
    if g.player_dead != k.player_dead {
        out.push(d("player_dead", g.player_dead, k.player_dead));
    }
    if g.combat_over != k.combat_over {
        out.push(d("combat_over", g.combat_over, k.combat_over));
    }
    if g.hp_loss_hits != k.hp_loss_hits {
        out.push(d("hp_loss_hits", g.hp_loss_hits, k.hp_loss_hits));
    }
    if g.free_attack != k.free_attack {
        out.push(d("free_attack", g.free_attack, k.free_attack));
    }
}

fn check_hp(ex: &Extracted, o: &Obs) -> Vec<(String, HpVerdict)> {
    let mut out = Vec::new();
    for (i, e) in o.enemies.iter().enumerate() {
        let name = ex.enemy_names[i].clone();
        let v = match ex.enemy_defs[i] {
            None => HpVerdict::UnknownEnemy,
            Some(def) => match asc::hp_range(def, ex.ascension) {
                None => HpVerdict::NoRange,
                Some((lo, hi)) => {
                    if e.max_hp >= lo && e.max_hp <= hi {
                        HpVerdict::In
                    } else {
                        HpVerdict::Out { got: e.max_hp, lo, hi }
                    }
                }
            },
        };
        out.push((name, v));
    }
    out
}

fn sig_text(s: &[(String, String)]) -> String {
    s.iter()
        .map(|(t, l)| if l.is_empty() { t.clone() } else { format!("{t}:{l}") })
        .collect::<Vec<_>>()
        .join(",")
}

fn check_moves(ex: &Extracted, o: &Obs, k: &State, sizes: &mut Vec<usize>) -> Vec<(String, MoveVerdict)> {
    let mut out = Vec::new();
    for (i, e) in o.enemies.iter().enumerate() {
        let name = ex.enemy_names[i].clone();
        let Some(def_id) = ex.enemy_defs[i] else {
            out.push((name, MoveVerdict::UnknownEnemy));
            continue;
        };
        let want = observed_signature(e);
        if want.is_empty() {
            out.push((name, MoveVerdict::NoIntent));
            continue;
        }
        let def = enemy_def(def_id);
        let picked = current_move_ix(def, k, i);
        // **开局允许集合**（`step::allowed_initial`）：内核默认挑的是集合里的
        // 一个代表元，而集合里其它几手同样合法。
        let allowed = allowed_initial(k, i);
        let n_moves = def.moves.len().max(1);
        let sig = |m: usize| move_signature(def_id, m, &k.enemies[i], &k.player, ex.ascension);
        let in_set = |pred: &dyn Fn(&[(String, String)]) -> bool| -> Option<usize> {
            (0..n_moves).find(|&m| allowed & (1 << m) != 0 && pred(&sig(m)))
        };
        let exact = in_set(&|g: &[(String, String)]| g == want.as_slice());
        let v = match exact {
            Some(m) if m == picked => MoveVerdict::Same,
            Some(_) => MoveVerdict::InSet,
            None => {
                let kind = in_set(&|g: &[(String, String)]| {
                    g.len() == want.len()
                        && g.iter().zip(&want).all(|(a, b)| {
                            a.0 == b.0 || (a.0.starts_with("Debuff") && b.0.starts_with("Debuff"))
                        })
                });
                match kind {
                    Some(m) => MoveVerdict::KindOnly {
                        want: sig_text(&want),
                        got: sig_text(&sig(m)),
                    },
                    None => MoveVerdict::Outside {
                        want: sig_text(&want),
                        got: sig_text(&sig(picked)),
                    },
                }
            }
        };
        sizes.push(allowed.count_ones() as usize);
        out.push((name, v));
    }
    out
}

fn identify(table: Option<&Table>, ex: &Extracted) -> EncVerdict {
    let Some(t) = table else { return EncVerdict::NoTable };
    if ex.enemy_defs.iter().any(|d| d.is_none()) {
        return EncVerdict::UnknownEnemy;
    }
    let defs: Vec<u16> = ex.enemy_defs.iter().filter_map(|d| *d).collect();
    let (exact, loose) = t.identify(&defs);
    match (exact.len(), loose.len()) {
        (1, _) => EncVerdict::Exact(exact[0].clone()),
        (0, 0) => EncVerdict::None,
        (0, _) => EncVerdict::Loose(loose),
        _ => EncVerdict::Ambiguous(exact),
    }
}

fn audit(path: &str, table: Option<&Table>) -> Row {
    let mut row = Row {
        file: path.rsplit(['/', '\\']).next().unwrap_or(path).to_string(),
        skip: None,
        withheld: Vec::new(),
        diffs: Vec::new(),
        gaps: Vec::new(),
        enc: EncVerdict::NoTable,
        hp: Vec::new(),
        moves: Vec::new(),
        move_set_sizes: Vec::new(),
    };
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            row.skip = Some(format!("读不了: {e}"));
            return row;
        }
    };
    let t = match parse_trace(&src) {
        Ok(t) => t,
        Err(e) => {
            row.skip = Some(e);
            return row;
        }
    };
    let Some(f0) = t.frames.first() else {
        row.skip = Some("没有帧".to_string());
        return row;
    };
    if let Err(e) = is_fight_start(&f0.obs) {
        row.skip = Some(e);
        return row;
    }

    // **「这一场是不是 Boss」从遭遇表来**（`encounters.json` 的 `room_type`）——
    // 缩放仪那 25 点开局回血只在 Boss 房给。认不出遭遇就是 `false`，方向是低估。
    // 先认一遍遭遇，因为 spec 要用它。
    let ex = extract(&t, &f0.obs, probe_boss_room(table, &f0.obs));
    row.withheld = ex.withheld.clone();
    // 种子只决定洗牌，而手牌身份本来就不可比（见模块头）。固定一个数，
    // 好让这台子的输出可重复。**开局回血由 `build_start` 倒推**（见那个函数）。
    let built = build_start(&ex, 0xA10D_0000_5EED);
    let mut rep = Replayer::for_trace(&t);
    let mut synced = rep.sync(&f0.obs);
    // **朝向要让参照那一侧也认出来。** `sync` 本身不反推朝向（游戏不把它报成
    // status），那是 `identify_enemies` 的事 —— 而实战驱动那条路每次都调它。
    // 不调的话参照永远是"朝左"（`State` 的默认 0），而构造器按 [源码] 的
    // `SurroundedPower._facing = Direction.Right` 开局朝右，于是带包围的那场
    // 会报一处假差。调了之后这一格反而变成**真检验**：`infer_facing` 是从
    // 观测到的意图标签反推的，两边对上就说明构造器的默认朝向和游戏一致。
    //
    // 它顺带把 `enemy_def` 认出来 —— 那一栏本来就不比（见模块头）。
    rep.identify_enemies(&mut synced.state, &f0.obs);

    // 被扣下的那几件遗物挂的 status —— 两边必然对不上，而那是台子的选择。
    let skip_st: Vec<usize> = row
        .withheld
        .iter()
        .filter_map(|id| relic_by_id(id))
        .flat_map(|def| {
            def.private_status
                .iter()
                .map(|(st, _)| st.ix())
                .chain(sts2core::content::conditional_start(def.id).map(|(_, st, _)| st.ix()))
                .collect::<Vec<_>>()
        })
        .collect();
    compare(&synced.state, &built.state, &ex, &skip_st, &mut row.diffs);
    row.gaps = built.gaps;
    row.enc = identify(table, &ex);
    row.hp = check_hp(&ex, &f0.obs);
    row.moves = check_moves(&ex, &f0.obs, &built.state, &mut row.move_set_sizes);
    row
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("用法: synth_audit <trace.json>... [--all] [--data <目录>]");
        eprintln!("  --all    连逐字段对上的那些也列出来");
        eprintln!("  --data   遭遇表所在目录（默认 data）");
        return ExitCode::from(2);
    }
    let mut paths = Vec::new();
    let mut show_all = false;
    let mut data_dir = "data".to_string();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--all" => show_all = true,
            "--data" => match it.next() {
                Some(p) => data_dir = p.clone(),
                None => {
                    eprintln!("--data 后面要跟一个目录");
                    return ExitCode::from(2);
                }
            },
            _ => paths.push(a.clone()),
        }
    }

    let table = match Table::load(&data_dir) {
        Ok(t) => Some(t),
        Err(e) => {
            println!("[!] 遭遇表读不了（{e}）—— 遭遇识别那一栏整栏跳过");
            None
        }
    };

    let rows: Vec<Row> = paths.iter().map(|p| audit(p, table.as_ref())).collect();

    // ---- 逐条 ----
    for r in &rows {
        if let Some(s) = &r.skip {
            println!("[跳过] {:<44} {s}", r.file);
            continue;
        }
        let tag = if r.diffs.is_empty() { "对上" } else { "有差" };
        let enc = match &r.enc {
            EncVerdict::Exact(k) => format!("遭遇 {k}"),
            EncVerdict::Ambiguous(ks) => format!("遭遇 {} 条都对得上", ks.len()),
            EncVerdict::Loose(ks) => format!("遭遇 {}（构成含随机）", ks[0]),
            EncVerdict::None => "遭遇 表里没有".to_string(),
            EncVerdict::UnknownEnemy => "遭遇 有敌人内核不认识".to_string(),
            EncVerdict::NoTable => String::new(),
        };
        if r.diffs.is_empty() && !show_all {
            println!("[{tag}] {:<44} {enc}", r.file);
        } else {
            println!("[{tag}] {:<44} {enc}", r.file);
            for x in &r.diffs {
                println!("       x {:<34} 游戏={:<14} 构造器={}", x.field, x.game, x.kernel);
            }
        }
        if !r.withheld.is_empty() {
            println!(
                "       · 没喂给构造器（第 0 帧的牌组已经被它改过）：{}",
                r.withheld.join(" ")
            );
        }
        if show_all {
            for g in &r.gaps {
                println!("       · 缺口 {g}");
            }
            for (n, v) in &r.hp {
                if let HpVerdict::Out { got, lo, hi } = v {
                    println!("       ~ 血量 {n} 实测 {got} 不在区间 [{lo},{hi}] 里");
                }
            }
            for (n, v) in &r.moves {
                match v {
                    MoveVerdict::Outside { want, got } => {
                        println!("       ~ 开局第一手 {n} 落在允许集合外 游戏={want} 内核默认={got}")
                    }
                    MoveVerdict::KindOnly { want, got } => {
                        println!("       ~ 开局第一手 {n} 类型对、数字不对 游戏={want} 内核={got}")
                    }
                    _ => {}
                }
            }
        }
    }

    // ---- 汇总 ----
    let done: Vec<&Row> = rows.iter().filter(|r| r.skip.is_none()).collect();
    let clean = done.iter().filter(|r| r.diffs.is_empty()).count();
    println!();
    println!(
        "== {} 条语料：逐字段一致 {} · 有差 {} · 跳过 {}",
        rows.len(),
        clean,
        done.len() - clean,
        rows.len() - done.len()
    );

    // 按字段汇总：**数的是"几条语料在这一格上有差"**，不是差了几次 ——
    // 一件遗物在 20 条语料里各错一次是**一个**缺口，不是 20 个。
    let mut by_field: BTreeMap<String, usize> = BTreeMap::new();
    for r in &done {
        let mut seen: Vec<&str> = Vec::new();
        for x in &r.diffs {
            if !seen.contains(&x.field.as_str()) {
                seen.push(&x.field);
                *by_field.entry(x.field.clone()).or_insert(0) += 1;
            }
        }
    }
    if !by_field.is_empty() {
        println!("-- 逐字段差（分母是语料条数 {}）：", done.len());
        let mut v: Vec<_> = by_field.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        for (f, n) in v.iter().take(30) {
            println!("   {n:>3}  {f}");
        }
        if v.len() > 30 {
            println!("   …… 另有 {} 个字段", v.len() - 30);
        }
    }

    let withheld_total = done.iter().filter(|r| !r.withheld.is_empty()).count();
    if withheld_total > 0 {
        let mut ids: Vec<String> = done.iter().flat_map(|r| r.withheld.clone()).collect();
        ids.sort();
        ids.dedup();
        println!(
            "-- {withheld_total} 条语料带着改牌组的遗物（{}），**没喂给构造器** —— 见 from_obs::DECK_REWRITERS",
            ids.join(" ")
        );
    }

    let mut gap_count: BTreeMap<String, usize> = BTreeMap::new();
    for r in &done {
        let mut seen: Vec<String> = Vec::new();
        for g in &r.gaps {
            let s = g.to_string();
            if !seen.contains(&s) {
                seen.push(s.clone());
                *gap_count.entry(s).or_insert(0) += 1;
            }
        }
    }
    if !gap_count.is_empty() {
        println!("-- 构造器自己报的缺口：");
        let mut v: Vec<_> = gap_count.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        for (g, n) in v.iter().take(20) {
            println!("   {n:>3}  {g}");
        }
        if v.len() > 20 {
            println!("   …… 另有 {} 条", v.len() - 20);
        }
    }

    let (mut e_exact, mut e_amb, mut e_loose, mut e_none, mut e_unk) = (0, 0, 0, 0, 0);
    for r in &done {
        match &r.enc {
            EncVerdict::Exact(_) => e_exact += 1,
            EncVerdict::Ambiguous(_) => e_amb += 1,
            EncVerdict::Loose(_) => e_loose += 1,
            EncVerdict::None => e_none += 1,
            EncVerdict::UnknownEnemy => e_unk += 1,
            EncVerdict::NoTable => {}
        }
    }
    println!(
        "-- 遭遇识别：唯一 {e_exact} · 多条都对得上 {e_amb} · 构成含随机 {e_loose} · 表里没有 {e_none} · 有敌人不认识 {e_unk}"
    );
    if e_none > 0 {
        for r in &done {
            if matches!(r.enc, EncVerdict::None) {
                println!("   表里没有：{}（{}）", r.file, r.enemy_names_text());
            }
        }
    }

    let (mut h_in, mut h_out, mut h_no, mut h_unk) = (0, 0, 0, 0);
    let mut out_rows: Vec<String> = Vec::new();
    let mut no_range: BTreeMap<String, usize> = BTreeMap::new();
    for r in &done {
        for (n, v) in &r.hp {
            match v {
                HpVerdict::In => h_in += 1,
                HpVerdict::Out { got, lo, hi } => {
                    h_out += 1;
                    out_rows.push(format!("{n} 实测 {got}，区间 [{lo},{hi}]（{}）", r.file));
                }
                HpVerdict::NoRange => {
                    h_no += 1;
                    *no_range.entry(n.clone()).or_insert(0) += 1;
                }
                HpVerdict::UnknownEnemy => h_unk += 1,
            }
        }
    }
    println!(
        "-- 血量区间（逐只敌人）：落在区间里 {h_in} · 区间外 {h_out} · 没有区间 {h_no} · 敌人不认识 {h_unk}"
    );
    out_rows.sort();
    out_rows.dedup();
    for s in &out_rows {
        println!("   区间外：{s}");
    }
    if !no_range.is_empty() {
        let mut v: Vec<_> = no_range.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        println!(
            "   没有区间的敌人：{}",
            v.iter().map(|(n, c)| format!("{n}×{c}")).collect::<Vec<_>>().join(" ")
        );
    }

    let (mut m_same, mut m_inset, mut m_kind, mut m_out, mut m_unk, mut m_no) = (0, 0, 0, 0, 0, 0);
    let mut diff_rows: Vec<String> = Vec::new();
    let mut set_sizes: Vec<usize> = Vec::new();
    for r in &done {
        set_sizes.extend(r.move_set_sizes.iter().copied());
        for (n, v) in &r.moves {
            match v {
                MoveVerdict::Same => m_same += 1,
                MoveVerdict::InSet => m_inset += 1,
                MoveVerdict::KindOnly { want, got } => {
                    m_kind += 1;
                    diff_rows.push(format!("{n} 类型对、数字不对：游戏={want} 内核={got}"));
                }
                MoveVerdict::Outside { want, got } => {
                    m_out += 1;
                    diff_rows.push(format!("{n} **落在允许集合外** 游戏={want} 内核默认={got}"));
                }
                MoveVerdict::UnknownEnemy => m_unk += 1,
                MoveVerdict::NoIntent => m_no += 1,
            }
        }
    }
    let avg = if set_sizes.is_empty() {
        0.0
    } else {
        set_sizes.iter().sum::<usize>() as f64 / set_sizes.len() as f64
    };
    println!(
        "-- 开局第一手（逐只敌人）：默认那一手就中 {m_same} · 在允许集合里 {m_inset} · 类型对 {m_kind} · **集合外 {m_out}** · 敌人不认识 {m_unk} · 没有意图 {m_no}"
    );
    println!(
        "   允许集合平均 {avg:.2} 手/只（1.00 = 开局完全确定）—— **集合越大命中越不值钱**，判据是「集合外」那个数"
    );
    diff_rows.sort();
    diff_rows.dedup();
    for s in diff_rows.iter().take(20) {
        println!("   {s}");
    }

    if let Some(t) = &table {
        println!("-- 遭遇表覆盖率（内核这一侧算的）：");
        for a in &t.acts {
            let (ok, all) = t.coverage(a);
            println!("   第 {} 幕 {:<12} {ok}/{all} 场开得出", a.index + 1, a.name);
        }
    }

    // **这个台子不判红。** 它报的是"构造器缺什么"，而缺什么是内容覆盖问题，
    // 和 `verify` 的 MISMATCH 不是一档。退出码留给"跑不起来"。
    ExitCode::SUCCESS
}

impl Row {
    fn enemy_names_text(&self) -> String {
        self.hp.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>().join("+")
    }
}
