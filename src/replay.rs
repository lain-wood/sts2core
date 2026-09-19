//! 对拍验证：把真实对局的 trace 喂进内核，逐字段 diff。
//!
//! 这一层**不实现任何游戏规则**，它只做三件事：
//!   1. 把观测同步成一个 [`State`]
//!   2. 调 [`step`] 走一步
//!   3. 拿结果和下一帧的观测比
//!
//! 设计上的三个关键选择，理由见 `docs/trace-format.md`：
//!
//! * **每帧独立**。diff 完立刻用观测重新同步，所以一处真错不会在后面派生出
//!   几十个假错。每一帧都是一次独立的"一步预测"检验。
//! * **不预测敌人行动**。敌人一律用 [`enemy::UNKNOWN`]（什么都不做），
//!   因为敌人 AI 本来就在「故意没做」清单里。因此 `end_turn` 帧只做**部分检查**。
//! * **不预测抽牌**。抽牌堆顺序在 mod 的 JSON 里已经被排序破坏，内核 RNG
//!   永远对不齐。抽牌堆用占位牌填充，牌一旦被抽过，手牌身份比较就自动降级成张数比较。

use crate::content::{card, enemy};
use crate::ops::{CardDef, EOp, Kind};
use crate::json::Json;
use crate::state::*;
use crate::step::{step, Action};
use std::collections::{BTreeMap, BTreeSet};

// --------------------------------------------------------------------------
// trace 数据结构
// --------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct CardObs {
    pub slot: usize,
    pub id: String,
    pub name: String,
    pub cost: Option<i32>,
    pub kind: String,
    pub upgraded: bool,
    pub description: String,
    /// 附魔的 id（`NIMBLE` 那种），没有就是空串。mod 2026-09-01 才开始报。
    pub enchant_id: String,
    /// 附魔的 `Amount`。怎么用取决于是哪一种附魔，见 `content::ENCHANTS`。
    pub enchant_amount: i32,
}

#[derive(Clone, Debug, Default)]
pub struct EnemyObs {
    pub combat_id: String,
    pub entity_id: String,
    pub name: String,
    pub hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub status: BTreeMap<String, i32>,
    /// 从 `Attack` 意图标签解析出的 `(每次伤害, 次数)`。
    ///
    /// 标签是游戏显示给玩家的来袭伤害，**已经含了敌人自己的力量**
    /// （实测：立柱构造体基础 7，力量 2 时显示 9，力量 4 时显示 11）。
    /// 所以它被当作最终面板值，不再过攻击方乘区。
    pub attacks: Vec<(i32, i32)>,
    /// 存在 `Attack` 意图但标签解析不出数字 —— 该帧不能用来对拍敌人回合。
    pub intent_unparsed: bool,
    /// 原样保留的 `(类型, 标签)`。意图标签的格式是靠数据认识的，不是靠猜的：
    /// [`Report::seen_intents`] 把见过的全列出来，多段攻击到底写成 `3x4` 还是
    /// 别的样子，看一眼就知道。
    pub raw_intents: Vec<(String, String)>,
}

/// 身上的一件遗物。**只带身份**：内核按 `id` 去 `content::RELICS` 查，
/// 描述是渲染文本（带计数器/状态），不可信也用不着。
#[derive(Clone, Debug, Default)]
pub struct RelicObs {
    /// 遗物面板上的计数器（游戏的 `DisplayAmount`）。**有些遗物的状态跨战斗
    /// 保留**，这是唯一能看到那个值的地方。见 `RelicDef::counter_to`。
    pub counter: Option<i32>,
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default)]
pub struct PotionObs {
    pub slot: usize,
    /// 游戏的内部 id（`BLOCK_POTION`），映射走 [`map_potion`]
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default)]
pub struct Obs {
    pub state_type: String,
    pub round: i32,
    pub is_play_phase: bool,
    pub energy: i32,
    pub max_energy: i32,
    pub hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub status: BTreeMap<String, i32>,
    pub enemies: Vec<EnemyObs>,
    pub hand: Vec<CardObs>,
    pub draw_count: i32,
    /// 抽牌堆的**内容**（牌名，升级态带 `+`）。**顺序不可信** ——
    /// mod 输出前按稀有度+id 排过（约束 2）。空 = 这条 trace 录在
    /// 加这个字段之前，那时只有 `draw_count`。
    pub draw: Vec<String>,
    /// 抽牌堆的**真实牌序**，**下标 0 是牌堆顶**。
    ///
    /// 来自 mod 的 `draw_pile_order`（2026-08-29 本地给 STS2MCP 打的补丁加的，
    /// 不是上游字段）。上面那个 `draw` 是 mod 按稀有度+id 重排过的，
    /// 真实顺序在那一步被扔掉；这个字段是同一堆牌，没排序。
    ///
    /// **空 = 拿不到**（老 trace，或者跑的是没打补丁的 mod）——
    /// 那时退回原来的行为：内容当多重集，顺序自己洗一个样本。
    ///
    /// 方向由 [源码] 定死：`CardPileCmd.Draw` 取 `drawPile.Cards.FirstOrDefault()`，
    /// `CardPile.MoveToTopInternal` 是 `_cards.Insert(0, card)`。
    /// **内核的 `State::draw` 反过来，顶在末尾**（`pop_draw_top` 从末尾取），
    /// 所以填进去要反向。
    pub draw_order: Vec<String>,
    /// 和 `draw_order` **逐位置配对**的附魔 `(id, amount)`，没附魔是 `("", 0)`。
    ///
    /// 来自 mod 的 `draw_pile_order_enchantments`（2026-09-03 本地补丁，
    /// 和 `draw_pile_order` 同一批）。**空 = 拿不到**（老 trace / 没打这版补丁
    /// 的 mod），那时抽到的牌一律按没附魔算 —— 那正是这个字段要补的洞：
    /// 一张带灵巧的耸肩无视在手里给 10 点格挡，段内才抽出来的却按卡表算 8。
    ///
    /// 挂在 `draw_order` 上而不是 `draw` 上是**必须的**：后者被 mod 按稀有度+id
    /// 重排过（约束 2），下标不再指向同一张实体牌。
    pub draw_order_enchant: Vec<(String, i32)>,
    pub discard: Vec<String>,
    /// 和 `discard` 逐位置配对的附魔，语义同 `draw_order_enchant`。
    /// 弃牌堆没被重排，所以这里直接跟着 `discard` 走。
    pub discard_enchant: Vec<(String, i32)>,
    pub exhaust: Vec<String>,
    /// 和 `exhaust` 逐位置配对的附魔，语义同 `draw_order_enchant`。
    pub exhaust_enchant: Vec<(String, i32)>,
    pub pending: bool,
    /// 身上的药水。**必须带槽位号**：喝掉中间一格之后观测数组会收缩
    /// （实测 `act1_f14`：喝掉 slot 2 后数组从 3 项变 2 项），
    /// 拿数组下标当槽位号迟早张冠李戴 —— 和敌人 `combat_id` 是同一个坑。
    pub potions: Vec<PotionObs>,
    /// 这一局有几个药水槽位。游戏在 `player.max_potion_slots` 里直接报，
    /// **所以不要从遗物反推** —— 高进阶会把初始值从 3 改成 2，而进阶等级
    /// 不在战斗观测里，反推一定会错。
    ///
    /// `None` = 这条 trace 是加这个字段之前录的。回退见 `sync`。
    pub max_potion_slots: Option<usize>,
    /// 身上的遗物。**遗物建模是增量的**：内核只认 `RELICS` 表里那几个，
    /// 其余靠这里的名字被 `--live` 点名报出来 —— 内核必须能说出
    /// 「我不认识这个遗物」，否则连"我可能算错了"都讲不出来。
    pub relics: Vec<RelicObs>,
}

#[derive(Clone, Debug)]
pub enum Act {
    Play { slot: usize, card_id: String, card_name: String, target: Option<String> },
    EndTurn,
    SelectCard { slot: usize },
    Confirm,
    /// 喝药水。录制器一直就把 `slot`/`potion_id`/`target` 都写进 trace 了，
    /// 只是 v1 的解析器只取了名字 —— 内核那时还没有药水模型。现在全都要。
    UsePotion { name: String, slot: usize, target: Option<String> },
    /// **被动录制器反推不出来的那一步**（trace 里 `kind: "unknown"`）。
    ///
    /// 它和 `action` 字段整个缺席（`Option::None`）**不是一回事**，
    /// 这个区分是 2026-08-25 为标定样本加的：
    ///
    /// | | 含义 | 后面的帧还能用吗 |
    /// |---|---|---|
    /// | `None` | trace 到头了，最后一帧没有动作 | 没有后面了 |
    /// | `Some(Unknown)` | 这一步反推不出来（玩家手快，一次轮询盖住了几个动作）| **能** —— 观测是真的，只有这一步的"做了什么"是空的 |
    ///
    /// 两者以前都落成 `None`，于是 `turn_segments` 在被动语料的第 0 帧就停了，
    /// 整条 trace 一条样本都导不出来。**能重放它的地方一律拒绝**（`advance`
    /// 返回 false、对拍跳过），但**只需要观测的地方照用**。
    Unknown,
}

impl Act {
    pub fn label(&self) -> String {
        match self {
            Act::Play { slot, card_name, target, .. } => match target {
                Some(t) => format!("出牌[{slot}] {card_name} -> {t}"),
                None => format!("出牌[{slot}] {card_name}"),
            },
            Act::EndTurn => "结束回合".to_string(),
            Act::SelectCard { slot } => format!("选牌[{slot}]"),
            Act::Confirm => "确认选择".to_string(),
            Act::UsePotion { name, .. } => format!("喝药水 {name}"),
            Act::Unknown => "<反推不出来的一步>".to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Settle {
    pub polls: i32,
    pub ms: i32,
    pub unstable: bool,
    pub intermediate_differs: bool,
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub obs: Obs,
    pub action: Option<Act>,
    pub settle: Option<Settle>,
}

#[derive(Clone, Debug)]
pub struct Trace {
    pub version: i64,
    pub run: String,
    /// 这一局的进阶等级（`run.ascension`，缺就是 0）。
    ///
    /// `run` 那个字符串里也有它，但那是给人看的。**内核要的是数**：
    /// `Replayer::for_trace` 把它灌进 `State::ascension`，
    /// A8/A9 的敌人数值才生效（见 `asc`）。
    pub ascension: u8,
    pub frames: Vec<Frame>,
}

// --------------------------------------------------------------------------
// 解析
// --------------------------------------------------------------------------

fn as_i32(j: Option<&Json>) -> i32 {
    j.and_then(|v| v.as_i64()).unwrap_or(0) as i32
}

fn status_map(j: Option<&Json>) -> BTreeMap<String, i32> {
    let mut out = BTreeMap::new();
    if let Some(Json::Obj(m)) = j {
        for (k, v) in m {
            if let Some(n) = v.as_i64() {
                out.insert(k.clone(), n as i32);
            }
        }
    }
    out
}

/// 一个 `{"id","name","amount"}` 附魔对象 -> `(id, amount)`；`null`/缺失 -> `("", 0)`。
fn parse_enchant(j: Option<&Json>) -> (String, i32) {
    match j {
        Some(e) => (
            e.str("id").unwrap_or_default().to_string(),
            e.i64("amount").unwrap_or(0) as i32,
        ),
        None => (String::new(), 0),
    }
}

fn parse_card(j: &Json) -> CardObs {
    CardObs {
        slot: j.i64("slot").unwrap_or(0) as usize,
        id: j.str("id").unwrap_or_default().to_string(),
        name: j.str("name").unwrap_or_default().to_string(),
        cost: j.i64("cost").map(|c| c as i32),
        kind: j.str("type").unwrap_or_default().to_string(),
        upgraded: j.get("upgraded").and_then(|v| v.as_bool()).unwrap_or(false),
        description: j.str("description").unwrap_or_default().to_string(),
        enchant_id: j
            .get("enchantment")
            .and_then(|e| e.str("id"))
            .unwrap_or_default()
            .to_string(),
        enchant_amount: j
            .get("enchantment")
            .and_then(|e| e.i64("amount"))
            .unwrap_or(0) as i32,
    }
}

/// 扯碎的段数：**从卡面文本反推**「本场挨穿过几次」（`State::hp_loss_hits`）。
///
/// 那个计数器观测里没有，但游戏把**算好的段数**渲染进了这张牌的描述：
/// `造成5点伤害。 在本场战斗中，…… （命中3次）`。段数 = 1 + 次数（[源码]
/// `TearAsunder`），所以次数 = 括号里那个数 − 1。
///
/// 这是「**观测到的数是权威**」那条老规矩的又一例（费用、附魔层数都是这么办的）：
/// 内核自己那份计数只从同步的那一刻开始累加，而这张牌吃的是**整场**的历史。
///
/// **只认扯碎**：括号里的数是这张牌独有的（它唯一的 `CalculatedVar` 就是段数），
/// 换张牌同样的括号可能是别的意思。取的是最后一对括号里第一串数字，
/// 全角半角都认；**读不出来就返回 `None`**，不猜。
fn observed_tear_hits(obs: &Obs) -> Option<u8> {
    let c = obs
        .hand
        .iter()
        .find(|c| c.id == "TEAR_ASUNDER" || lookup_card(&c.name) == Some(card::TEAR_ASUNDER))?;
    // **切片要落在字符边界上**：`（` 是 3 个字节，`i + 1` 会切进它中间直接 panic。
    // 从括号本身切起就行 —— 下面那个 `skip_while` 顺手把它跳掉。
    let open = c.description.rfind(['（', '('])?;
    let digits: String = c.description[open..]
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    let hits: i32 = digits.parse().ok()?;
    // 段数至少是 1；再小说明这句话不是我以为的那句，宁可当读不出来。
    if hits < 1 { None } else { Some((hits - 1).min(u8::MAX as i32) as u8) }
}

/// 痛殴这一张实例**攒下来的伤害加值**：从卡面文本反推。
///
/// `Op::ExhaustRandomAttackAddDamage` 把吃掉那张攻击牌的伤害**永久**写进
/// `CardInst.bonus`，而 `bonus` 观测里没有 —— `sync` 每帧从观测重建手牌，
/// 于是攒了一整场的痛殴每帧都被重置回卡表基础值。
///
/// 但游戏把**算好的伤害**渲染进了描述：`造成13点伤害两次`。
/// 和 [`observed_tear_hits`] 是同一条老规矩（观测到的数是权威），
/// 也共享同一处局限：**只有手牌看得见**，牌不在手上时加值是 0，方向是**低估**。
///
/// # 渲染的那个数是「过了攻击方乘区、没过防御方乘区」的值
///
/// **这一条是实测钉死的，不是推的** —— 第3幕第46层同一场两帧：
///
/// | 帧 | 渲染 | 我的 status | 敌人 status | 实打/次 |
/// |---|---|---|---|---|
/// | 3  | 7  | 力量1 | 易伤2 | 10 = ⌊7×1.5⌋ |
/// | 29 | 13 | 力量1 虚弱1 | — | 13 |
///
/// 帧3 说明**易伤（防御方）没进渲染值**（否则会渲染 10）；
/// 帧29 说明**虚弱（攻击方）进了**（`(6+B+1)×0.75 = 13` ⇒ B = 11，
/// 而 `6+B+1 = 13` 会给出 B = 6，那样实打就该是 ⌊13×0.75⌋ = 9 而不是 13）。
///
/// # 怎么反推：**正着算一遍，试到相等为止**
///
/// 2026-09-06 之前这里是 `加值 = 渲染值 − 卡面 − 力量 − 活力`，**只在攻击方
/// 一个乘区都没有时成立**，带虚弱就整帧放弃（`act3_f46` 帧29 那条红）。
/// 除以 0.75 再取整确实不可逆 —— **但不需要除**：加值是个小非负整数，
/// 拿同一条伤害管线**正着**算一遍，逐个试过去就行。
///
/// 好处不只是多修一帧：这样反推**永远和 `damage.rs` 同口径**。
/// 将来再进来一个攻击方乘区（纸蛙那类），这里一个字都不用改；
/// 而减法那版会静默地错。
///
/// `preview_double` 是钢笔尖：计数器到 9 时**手牌里的攻击牌渲染的就是翻倍后
/// 的数**（[实测] 同一场帧31/32 的全身撞击+，同样 6 点格挡，渲染 5 -> 10）。
/// 它进 `apply_modifiers` 的方式和真打出去时一模一样（挂 `PenNibArmed`）。
///
/// **欠定时取最小的那个解**（`floor` 的原像最多两个整数）。方向是低估，
/// 和这个函数其余每一处回退一致 —— 但它**不是"留空"**：0 是个更差的猜测，
/// 而这里的候选集是穷举出来的、边界清楚。
///
/// **只认痛殴**：描述里第一个数字对别的牌可能是别的意思。读不出来返回 `None`，不猜。
pub fn observed_thrash_bonus(
    desc: &str,
    upgraded: bool,
    player: &Entity,
    preview_double: bool,
) -> Option<i16> {
    let shown: i32 = desc
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()?;
    let def = card(card::THRASH);
    let base = if upgraded { def.ops_upg } else { def.ops }.iter().find_map(|op| match op {
        crate::ops::Op::Damage { base, .. } => Some(*base),
        _ => None,
    })?;
    // 攻击方 = 玩家本人（外加钢笔尖那个预览标记），防御方 = 一个**中性实体**。
    // 中性那一半是关键：渲染值**不含防御方乘区**（帧3 实测，见上面那张表），
    // 而全 0 的 `Entity` 在 `apply_modifiers` 里每一条防御方分支都是空操作。
    let mut atk = *player;
    atk.set(St::PenNibArmed, if preview_double { 1 } else { 0 });
    let neutral = Entity::new(0);
    let strength = player.get(St::Strength);
    let vigor = player.get(St::Vigor);
    // 加值是「吃掉的那些攻击牌的伤害之和」，一场仗攒不到 200。
    // 找**第一个**算得出这个渲染值的加值：管线对加值单调不减，
    // 所以第一个就是最小解。
    for b in 0..=200 {
        let face = crate::damage::card_face_damage(base, (1, 1), b, strength, vigor);
        if crate::damage::apply_modifiers(face, &atk, &neutral) == shown {
            return Some(b as i16);
        }
    }
    None
}

/// 观测里的附魔 -> `CardInst` 那两个字段 `(下标+1, Amount)`。
///
/// 表里没有这个 id 就返回 `(0, 0)`（当没附魔算，方向是**低估**，
/// 和 `draw_order` 缺失时的回退同一个处置），并由调用方点名。
/// 规则本身一条都不在这里 —— 全在 `content::ENCHANTS` 那张表里。
pub fn lookup_enchant(id: &str, amount: i32) -> (u8, i32) {
    if id.is_empty() {
        return (0, 0);
    }
    match crate::content::enchant_by_id(&id.to_ascii_uppercase()) {
        Some((ix, _)) => (ix, amount),
        None => (0, 0),
    }
}

/// 这个附魔内核**建全了没有**。没建全 = 这张牌会被算错。
///
/// 「表里没有」和「表里有但 `modelled: false`」对内核是同一件事，
/// 所以两者都返回 false，一起进 `Report::unknown_enchantments` 点名。
pub fn enchant_fully_modelled(id: &str) -> bool {
    crate::content::enchant_by_id(&id.to_ascii_uppercase())
        .map_or(false, |(_, def)| def.modelled)
}

/// 意图标签 -> `(每次伤害, 次数)`。见过的形式：`"11"`、`"6"`，以及多段的 `"3x4"`。
/// 多段必须保留次数而不是求和：格挡是**逐次**吸收的，3×4 打空 5 点格挡的结果
/// 和一次 12 点完全不同。
fn parse_intent_label(label: &str) -> Option<(i32, i32)> {
    let s = label.trim();
    if s.is_empty() {
        return None;
    }
    for sep in ['x', 'X', '×'] {
        if let Some((a, b)) = s.split_once(sep) {
            let base = a.trim().parse::<i32>().ok()?;
            let hits = b.trim().parse::<i32>().ok()?;
            return Some((base, hits));
        }
    }
    s.parse::<i32>().ok().map(|d| (d, 1))
}

/// Recover a private hit counter from the CURRENT observed intent. This is
/// state synchronization, never a prediction of that same intent.
fn sync_attack_counters(e: &EnemyObs, ent: &mut Entity) {
    let Some(def) = enemy_id(&e.name) else { return };
    for (mv, m) in crate::content::enemy_def(def).moves.iter().enumerate() {
        if !crate::content::move_matches_form(def, mv, ent) { continue; }
        for op in m.ops {
            if let crate::ops::EOp::AttackPlusStackHits { hits, per, .. } = *op {
                if let Some(&(_, observed_hits)) = e.attacks.first() {
                    ent.set(per, (observed_hits - hits).max(0));
                }
            }
        }
    }
}

/// 私有计数器反推的上限。瀑布巨兽的爆炸伤害 = 15 + 每手 3，拖到 40 个回合也才 135；高压枪每次 +5。
const PRIVATE_COUNTER_CAP: i32 = 400;

/// 敌人**私有的**伤害计数器（`EOp::AttackPlusSelfStatus` 的 `per`，游戏不报、不在 [`ALL_ST`] 里的那种）
/// 从**这一手**的意图标签反推：拿同一条签名正着算一遍，逐个值试过去（和 [`observed_thrash_bonus`]
/// 同一招，所以永远和 `damage.rs` 同口径 —— 我身上的易伤、它身上的虚弱都自动算进去）。
///
/// 消费者是瀑布巨兽的两个：高压枪涨过多少（`PressureGunGrowth`）、爆炸记下多少（`EruptionDamage`）。
/// 不反推的话，战斗中途同步进来它们一律是 0 —— `solve --live` 那条「现算必须和标签逐字相同」的
/// 自检当场失败、整份退回冻住的标签，跨回合那几层按 0 预测高压枪和爆炸；
/// 对齐出招指针时高压枪 25 还会按类型退回、对到同样是 `Attack + Buff` 的撞击上。
///
/// **这是同步不是预测**：读的是它这一手已经亮出来的标签，只拿来还原状态。
/// 观测里有的 status（遗忘之物的敏捷）一概不碰。对不上返回 `None`。
fn infer_private_attack_counter(
    def_id: u16,
    mv: usize,
    ent: &Entity,
    player: &Entity,
    asc: u8,
    want: &[(String, String)],
) -> Option<(St, i32)> {
    let m = crate::content::enemy_def(def_id).moves.get(mv)?;
    let per = m.ops.iter().find_map(|op| match *op {
        EOp::AttackPlusSelfStatus { per, .. } if !ALL_ST.contains(&per) => Some(per),
        _ => None,
    })?;
    let mut probe = *ent;
    (0..=PRIVATE_COUNTER_CAP).find_map(|v| {
        probe.set(per, v);
        (move_signature(def_id, mv, &probe, player, asc) == want).then_some((per, v))
    })
}

fn parse_obs(j: &Json) -> Obs {
    let player = j.get("player");
    let mut enemies = Vec::new();
    if let Some(Json::Obj(m)) = j.get("enemies") {
        for (cid, e) in m {
            let mut attacks = Vec::new();
            let mut intent_unparsed = false;
            let mut raw_intents = Vec::new();
            for it in e.arr("intents").unwrap_or(&[]) {
                let ty = it.str("type").unwrap_or("?").to_string();
                let label = it.str("label").unwrap_or("").to_string();
                raw_intents.push((ty.clone(), label.clone()));
                if ty != "Attack" && ty != "DeathBlow" {
                    continue;
                }
                match parse_intent_label(&label) {
                    Some(a) => attacks.push(a),
                    None => intent_unparsed = true,
                }
            }
            enemies.push(EnemyObs {
                combat_id: cid.clone(),
                entity_id: e.str("entity_id").unwrap_or_default().to_string(),
                name: e.str("name").unwrap_or_default().to_string(),
                hp: as_i32(e.get("hp")),
                max_hp: as_i32(e.get("max_hp")),
                block: as_i32(e.get("block")),
                status: status_map(e.get("status")),
                attacks,
                intent_unparsed,
                raw_intents,
            });
        }
    }
    let pile = |key: &str| -> Vec<String> {
        j.arr(key)
            .unwrap_or(&[])
            .iter()
            .map(|c| c.str("name").unwrap_or_default().to_string())
            .collect()
    };
    // 牌堆的附魔，和上面的牌名逐位置配对。整堆一个都没有就返回空 vec ——
    // 「一张都没附魔」和「这条 trace 根本没这个字段」在下游是同一个处置，
    // 不值得为区分它俩多带一个 Option。
    let pile_enchant = |key: &str| -> Vec<(String, i32)> {
        let v: Vec<(String, i32)> = j
            .arr(key)
            .unwrap_or(&[])
            .iter()
            .map(|c| parse_enchant(c.get("enchantment")))
            .collect();
        if v.iter().all(|(id, _)| id.is_empty()) { Vec::new() } else { v }
    };
    let potions = player
        .and_then(|p| p.arr("potions"))
        .or_else(|| j.arr("potions"))
        .unwrap_or(&[])
        .iter()
        .enumerate()
        // `slot` 缺失时退回**数组下标**。mod 现在是给 slot 的（实录里都有），
        // 但缺了就静默丢掉整瓶药水太狠了 —— 那会让求解器以为你两手空空，
        // 而这件事在输出里看不出来。
        .map(|(i, pot)| PotionObs {
            slot: pot.get("slot").and_then(|v| v.as_i64()).filter(|v| *v >= 0).unwrap_or(i as i64)
                as usize,
            id: pot.str("id").unwrap_or_default().to_string(),
            name: pot.str("name").unwrap_or_default().to_string(),
        })
        .collect();

    let relics = player
        .and_then(|p| p.arr("relics"))
        .or_else(|| j.arr("relics"))
        .unwrap_or(&[])
        .iter()
        .map(|r| RelicObs {
            counter: r.get("counter").and_then(|v| v.as_i64()).map(|v| v as i32),
            id: r.str("id").unwrap_or_default().to_string(),
            name: r.str("name").unwrap_or_default().to_string(),
        })
        .collect();

    Obs {
        state_type: j.str("state_type").unwrap_or_default().to_string(),
        round: as_i32(j.get("round")),
        is_play_phase: j.get("is_play_phase").and_then(|v| v.as_bool()).unwrap_or(false),
        energy: as_i32(j.get("energy")),
        max_energy: as_i32(j.get("max_energy")),
        hp: as_i32(player.and_then(|p| p.get("hp"))),
        max_hp: as_i32(player.and_then(|p| p.get("max_hp"))),
        block: as_i32(player.and_then(|p| p.get("block"))),
        status: status_map(player.and_then(|p| p.get("status"))),
        enemies,
        hand: j.arr("hand").unwrap_or(&[]).iter().map(parse_card).collect(),
        draw_count: as_i32(j.get("draw_count")),
        draw: pile("draw"),
        draw_order: j
            .arr("draw_order")
            .unwrap_or(&[])
            .iter()
            .filter_map(|v| v.as_str())
            .map(|v| v.to_string())
            .collect(),
        draw_order_enchant: {
            let v: Vec<(String, i32)> = j
                .arr("draw_order_enchant")
                .unwrap_or(&[])
                .iter()
                .map(|e| parse_enchant(Some(e)))
                .collect();
            if v.iter().all(|(id, _)| id.is_empty()) { Vec::new() } else { v }
        },
        discard: pile("discard"),
        discard_enchant: pile_enchant("discard"),
        exhaust: pile("exhaust"),
        exhaust_enchant: pile_enchant("exhaust"),
        pending: j.get("pending").is_some(),
        potions,
        max_potion_slots: player
            .and_then(|p| p.get("max_potion_slots"))
            .or_else(|| j.get("max_potion_slots"))
            .and_then(|v| v.as_i64())
            .filter(|v| *v > 0)
            .map(|v| v as usize),
        relics,
    }
}

fn parse_action(j: &Json) -> Option<Act> {
    match j.str("kind")? {
        "play_card" => Some(Act::Play {
            slot: j.i64("slot").unwrap_or(0) as usize,
            card_id: j.str("card_id").unwrap_or_default().to_string(),
            card_name: j.str("card_name").unwrap_or_default().to_string(),
            target: j.str("target").map(|s| s.to_string()),
        }),
        "end_turn" => Some(Act::EndTurn),
        "select_card" => Some(Act::SelectCard { slot: j.i64("slot").unwrap_or(0) as usize }),
        "confirm_selection" => Some(Act::Confirm),
        // 被动录制器写的占位。见 `Act::Unknown` 的注释。
        "unknown" => Some(Act::Unknown),
        "use_potion" => Some(Act::UsePotion {
            name: j.str("potion_name").unwrap_or("<未知药水>").to_string(),
            slot: as_i32(j.get("slot")).max(0) as usize,
            target: j.str("target").map(|s| s.to_string()),
        }),
        _ => None,
    }
}

pub fn parse_trace(src: &str) -> Result<Trace, String> {
    let j = crate::json::parse(src).map_err(|e| e.to_string())?;
    let version = j.i64("version").unwrap_or(0);
    if version != 1 {
        return Err(format!("不支持的 trace 版本 {version}（本程序只认 1）"));
    }
    // **数值那一份要单独取。** `run` 那个字符串是给人看的，
    // 内核读的是 `Trace::ascension`（进 `State::ascension`，A8/A9 才生效）。
    let ascension =
        j.get("run").and_then(|r| r.i64("ascension")).unwrap_or(0).clamp(0, 10) as u8;
    let run = match j.get("run") {
        Some(r) => format!(
            "第{}幕 第{}层 A{} {}",
            r.i64("act").unwrap_or(0),
            r.i64("floor").unwrap_or(0),
            r.i64("ascension").unwrap_or(0),
            r.str("character").unwrap_or("?")
        ),
        None => "?".to_string(),
    };
    let mut frames = Vec::new();
    for f in j.arr("frames").ok_or("trace 里没有 frames 数组")? {
        frames.push(Frame {
            obs: parse_obs(f.get("obs").ok_or("帧里没有 obs")?),
            action: f.get("action").and_then(parse_action),
            settle: f.get("settle").map(|s| Settle {
                polls: as_i32(s.get("polls")),
                ms: as_i32(s.get("ms")),
                unstable: s.get("unstable").and_then(|v| v.as_bool()).unwrap_or(false),
                intermediate_differs: s
                    .get("intermediate_differs")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            }),
        });
    }
    Ok(Trace { version, run, ascension, frames })
}

// --------------------------------------------------------------------------
// 内容映射
// --------------------------------------------------------------------------

/// 游戏的 power id -> 内核的状态位。
///
/// 实录确认（2026-08-15，第1幕第2层）：mod 输出的 `power.Id.Entry` 形如
/// `VULNERABLE_POWER` / `SHRINK_POWER`，全大写并带 `_POWER` 后缀，
/// 所以先归一化再查表。对不上的 id 不会被静默忽略：
/// [`Report::unmapped_status`] 会把它们全列出来，照着补表即可。
///
/// **映射表不全时，`MATCH` 本身不可信** —— 同一场实录里，内核既漏了
/// `SHRINK_POWER` 的减伤又漏了 `VULNERABLE_POWER` 的加成，两个错误
/// 恰好抵消，一帧假的 `MATCH` 就这么出来了。补表是第一优先级。
pub fn map_status(id: &str) -> Option<St> {
    let k = id.to_ascii_lowercase();
    let k = k.strip_suffix("_power").unwrap_or(&k);
    Some(match k {
        "strength" | "力量" => St::Strength,
        "dexterity" | "敏捷" => St::Dexterity,
        "vulnerable" | "易伤" => St::Vulnerable,
        "weak" | "weakness" | "虚弱" => St::Weak,
        "frail" | "脆弱" => St::Frail,
        "artifact" | "人工制品" => St::Artifact,
        "intangible" | "无实体" => St::Intangible,
        "demon_form" | "demonform" | "恶魔形态" => St::DemonForm,
        "enrage" => St::Rage,
        "slow" | "缓慢" => St::Slow,
        "damage_cap" | "hard_to_kill" | "难以杀灭" => St::DamageCap,
        "shrink" | "缩小" => St::Shrink,
        "curl_up" | "蜷身" => St::CurlUp,
        "constrict" | "constricted" | "缠绕" => St::Constrict,
        "infested" | "寄生物" => St::Infested,
        "plow" | "耕地" => St::Plow,
        // 库存（[源码] `StockPower`）。游戏报 `STOCK_POWER`，
        // 2026-08-30 第 3 幕第 45 层 24 帧里这个字段一直没被检查过。
        "stock" | "库存" => St::Stock,
        "ringing" | "轰鸣" => St::Ringing,
        // 凌虐（[源码] `ManglePower : TemporaryStrengthPower`）和预备打击共用
        // 内核的 `TempStrength`，但**观测里的符号相反** —— 见 `status_value`。
        "setup_strike" | "mangle" | "凌虐" => St::TempStrength,
        // 触发式能力牌。游戏侧的 id 是照 `traces/cards_catalog.json` 的卡 id 推的，
        // **还没有一帧真实观测确认过**；第一次真拿到这些牌时，若 id 对不上，
        // `unmapped_status` 会把真名报出来，照着补即可。
        "pyre" | "薪火之源" => St::Pyre,
        "feel_no_pain" | "无惧疼痛" => St::FeelNoPain,
        "dark_embrace" | "黑暗之拥" => St::DarkEmbrace,
        "rupture" | "撕裂" => St::Rupture,
        "crimson_mantle" | "绯红披风" => St::CrimsonMantle,
        "rolling_boulder" | "滚石" => St::RollingBoulder,
        "vicious" | "凶恶" => St::Vicious,
        // 覆甲。**游戏报的是 `PLATING_POWER`** —— `plated_armor` 是当初照
        // `cards_catalog.json` 的卡 id 推的，从没被观测确认过。
        // 2026-08-30 第一次真遇到带覆甲的敌人（青蛙骑士）才对上：
        // `verify` 当场报「没映射的 status：PLATING_POWER×27」，
        // 那 27 帧里这个字段**一个都没被检查过**。
        // 两个 key 都留着：真名在前，推的那个当别名。
        "plating" | "plated_armor" | "覆甲" => St::PlatedArmor,
        // 贪食（噬尸蛞蝓）。**层数是观测量**（游戏面板报 `RAVENOUS_POWER`），
        // 所以它也进 `ALL_ST` —— 规则那一半（同伴死 -> 加力量 + 击晕）在 POWERS。
        "ravenous" | "贪食" => St::Ravenous,
        "rage" | "frenzy" | "狂怒" => St::Frenzy,
        "flame_barrier" | "火焰屏障" => St::FlameBarrier,
        "juggernaut" | "势不可当" => St::Juggernaut,
        "stampede" | "惊逃" => St::Stampede,
        "one_two_punch" | "连环拳" => St::OneTwoPunch,
        "juggling" | "杂耍" => St::Juggling,
        "aggression" | "好勇斗狠" => St::Aggression,
        "colossus" | "巨像" => St::Colossus,
        "barricade" | "壁垒" => St::Barricade,
        // 均衡。**游戏报的是 `RETAIN_HAND_POWER`** —— `entrench` 是当初照卡 id
        // 推的，从没被观测确认过。和覆甲那次是**逐字同一个失效模式**：
        // 2026-09-06 第3幕第42层第一次真打出均衡+，`verify` 当场报
        // 「我方.均衡 游戏=0 内核=1」，而同一帧的报告里
        // 「没映射的 status: RETAIN_HAND_POWER×3」就在下面两行。
        // 真名在前，推的那个当别名 —— 处置和覆甲一致。
        "retain_hand" | "entrench" | "均衡" => St::Entrench,
        // 跃跃欲试自己给自己挂的「本回合不能再获得能量」。
        // 和均衡是同一批：`act3_f46_elite_soul_nexus` 报「没映射」才发现的。
        "no_energy_gain" => St::NoEnergyGain,
        "all_or_nothing" | "孤注一掷" => St::AllOrNothing,
        // 已知但内核不建模。映射它们是为了**别让整帧降级成 UNKNOWN**，
        // 那会连同一帧里的真错误一起藏掉。见 `St::Minion` / `St::Illusion`。
        "minion" | "爪牙" => St::Minion,
        // 坚定不移（[源码] `UnmovablePower`）。层数 = 每回合翻倍几次
        "unmovable" | "坚定不移" => St::Unmovable,
        "illusion" | "幻象" => St::Illusion,
        "regen" | "再生" => St::Regen,
        "imbalanced" | "失衡" => St::Imbalanced,
        "personal_hive" | "人体蜂房" => St::PersonalHive,
        "slumber" | "熟睡" => St::Slumber,
        // 沉睡（乐加维林族母，2026-09-17）。**和熟睡不是一个 power**，见 `St::Asleep`。
        "asleep" | "沉睡" => St::Asleep,
        // 滑溜（墨影幻灵 / 墨宝，2026-09-17）。**和无实体不是一个** —— 它封的是掉血不是伤害。
        "slippery" | "滑溜" => St::Slippery,
        "sandpit" | "沙坑" => St::Sandpit,
        "tainted" | "污染" => St::Tainted,
        "vital_spark" | "活力火花" => St::VitalSpark,
        "dark_shackles" | "黑暗镣铐" => St::DarkShackles,
        "hatch" | "孵化" => St::Hatch,
        "flutter" | "扑翼" => St::Flutter,
        "escape_artist" | "逃跑大师" => St::EscapeArtist,
        // 偷窃草蜢偷牌的那个。本地化名是「顺走」—— 这里原来收的是「偷窃」，
        // 那是地精佣兵 `THIEVERY_POWER` 的名字（2026-09-19 对 pck 对出来的）。观测走的是 id，没踩到过。
        "swipe" | "顺走" => St::Swipe,
        // 2026-09-19 第 1 幕批 4 / 批 5。id 照 [源码] 类名推（`HardenedShellPower` -> `HARDENED_SHELL_POWER`），
        // 中文取自本地化表 `*_POWER.title`。**还没有一帧真实观测确认过**。
        // 硬化外壳：mod 报的是 `DisplayAmount`（本回合余额），内核这一栏存的就是余额，原样搬。
        "hardened_shell" | "硬化外壳" => St::HardenedShell,
        "suck" | "吮吸" => St::Suck,
        "surprise" | "意外" => St::Surprise,
        "thievery" | "偷窃" => St::Thievery,
        "heist" | "盗窃" => St::Heist,
        // 2026-09-19 第 1 幕批 6。`TangledPower` -> `TANGLED_POWER`，中文取自 `TANGLED_POWER.title`（「缠绕」是另一个：`CONSTRICT`）。
        // **还没有一帧真实观测确认过**。
        "tangled" | "缠结" => St::Tangled,
        "thorns" | "荆棘" => St::Thorns,
        "ritual" | "仪式" => St::Ritual,
        // 尖叫：游戏报 `SHRIEK_POWER`，层数是**血量阈值**（70 / 进阶 75）
        "shriek" | "尖叫" => St::Shriek,
        // 活力：游戏报 `VIGOR_POWER`。**内核建模了行为**（不是只映射名字）——
        // 规则在 `St::Vigor` + `step::spend_vigor`，来源是赤牛。
        "vigor" | "活力" => St::Vigor,
        // 高压：游戏报 `HIGH_VOLTAGE_POWER`。**行为已建模**（Hook::EnemyTurnEnd）。
        "high_voltage" | "高压" => St::HighVoltage,
        "territorial" | "领地意识" => St::Territorial,
        "tender" | "娇弱" => St::Tender,
        "radiance" | "光耀" => St::Radiance,
        "reattach" | "接续" => St::Reattach,
        // 知识恶魔的四个诅咒（2026-09-14）。中文名取自游戏本地化表 `*_POWER.title`
        "disintegration" | "瓦解" => St::Disintegration,
        "mind_rot" | "心灵腐化" => St::MindRot,
        "sloth" | "懒惰" => St::Sloth,
        "waste_away" | "虚脱" => St::WasteAway,
        "surrounded" | "遭到包围" => St::Surrounded,
        "back_attack_left" => St::BackAttackLeft,
        "back_attack_right" => St::BackAttackRight,
        "crab_rage" | "蟹之怒" => St::CrabRage,
        // 战斗专注的「本回合不能再抽牌」。行为**早就建了**（`State::draw_one`
        // 最前面那一句），缺的只是名字映射 —— 不映射会让整帧的 status 报「没检查」。
        "no_draw" | "不能抽牌" => St::NoDraw,
        // 凋萎存在：游戏报 `WITHERING_PRESENCE_POWER`，层数 = 还差几张牌。
        // **行为没建模**（内核没有"打出任意一张牌"的钩子），但层数是观测量。
        "withering_presence" | "凋萎存在" => St::WitheringPresence,
        "rampart" | "护壁" | "城垛" => St::Rampart,
        "soar" | "翱翔" | "飞行" => St::Soar,
        "adaptable" | "适生力" | "适应力" => St::Adaptable,
        "painful_stabs" | "剧痛刺击" => St::PainfulStabs,
        "nemesis" | "复仇宿敌" | "天敌" => St::Nemesis,
        "smoggy" | "smog" | "侵蚀" | "烟雾" => St::Smoggy,
        "skittish" | "胆小" => St::Skittish,
        "steam_eruption" | "蒸汽喷发" => St::SteamEruption,
        "burrowed" | "钻地" => St::Burrowed,
        // 2026-09-13 第 3 幕补敌人。id 取自 [源码] 类名（`GalvanicPower` -> `GALVANIC_POWER`），
        // 中文取自游戏本地化表的 `*.title`。**还没有一帧真实观测确认过**。
        "galvanic" | "流电" => St::Galvanic,
        "paper_cuts" | "纸伤难愈" => St::PaperCuts,
        "possess_strength" | "抢夺力量" => St::PossessStrength,
        "possess_speed" | "抢夺速度" => St::PossessSpeed,
        // 2026-09-14 骑士团。同样是照 [源码] 类名推的 id，**还没有观测确认过**。
        "hex" | "恶咒" => St::Hex,
        "dampen" | "抑制" => St::Dampen,
        _ => return None,
    })
}

/// 观测里那个数 -> 内核这一栏**该存的值**。
///
/// 绝大多数 status 是原样搬，两个例外，而且**两个都不是数值精度问题，
/// 是"面板那个数和内核那个数根本不是同一个量"**：
///
/// * **娇弱**：`DisplayAmount => CardsPlayedThisTurn`，回合开始报 0 而 status 还挂着。
///   内核这栏是「挂没挂着」的标记 ⇒ 出现就置 ≥1（要还多少走 `Amt::CardsPlayed`）。
/// * **凌虐**：游戏报 `MANGLE_POWER: 10` 表示"临时**减** 10"（`Sign = -1`），
///   而内核的 `TempStrength` 存的是**加到力量上的增量**（−10），
///   `strip_temp_strength` 回合末做 `add(Strength, -tmp)` 把它还回去。
///   [实测] 三条实录里全是 `MANGLE_POWER: 10` 配 `STRENGTH_POWER: -10`。
///   **照搬正数会让回合末反向加 10 点力量** —— 那种错只在敌人还活着的长仗里才看得出来。
///
/// 收口成一个函数，是因为同步点有四处（玩家 / 敌人 / 认敌人 / 每回合重建），
/// 手写的地方一定会漏（`relic_carry` 就漏过）。
pub(crate) fn status_value(id: &str, st: St, amt: i32) -> i32 {
    let k = id.to_ascii_lowercase();
    let k = k.strip_suffix("_power").unwrap_or(&k);
    match st {
        St::Tender => amt.max(1),
        St::TempStrength if k == "mangle" || k == "凌虐" => -amt,
        _ => amt,
    }
}

/// 观测里的哪个 key 表示「这个敌人带缓慢」。
///
/// 缓慢的层数每回合归零，所以键**存在**（哪怕值是 0）才是「它有缓慢」的证据，
/// 值本身只是本回合已经累计的百分比。`sync` 靠这个把 [`St::SlowSource`] 补上，
/// 否则内核不知道该给谁累加 —— 敌人一律用 `enemy::UNKNOWN`，查不到 def。
fn observed_has_slow(status: &BTreeMap<String, i32>) -> bool {
    status.keys().any(|k| map_status(k) == Some(St::Slow))
}

/// 战斗已经结束的观测（奖励结算界面等）。
///
/// 战斗结束那一帧不能做全字段对拍：`hand`/`discard` 会被清空，`hp` 还会被
/// 战斗结束类遗物改写（实录里燃烧之血回了 6 点，内核没有遗物概念）。
/// 这些都不是内核算错了。
/// 战斗结束帧的**窄对拍：只比血量**。
///
/// `combat_over_obs` 那条注释说的是实话 —— 牌区被清空、身份信息没了，
/// 全字段对拍在这一帧做不了。但它当时的结论（整帧跳过）多跳了一样东西：
/// **血量还在**，而战斗结束类遗物只在这一帧显形。跳过它，
/// 燃烧之血/带骨肉/天选芝士 这一整类就永远没有证据面。
///
/// 三种结局，分开报：
/// * 血量对上 → `Match`（并把数字写进 notes，好让人自己核）
/// * 对不上、且遗物内核全认识 → `Mismatch`，**这是要修的信号**
/// * 对不上、但身上有不认识的遗物 → `UnknownContent`，不算内核的账
///
/// **没录 `relics` 的老 trace 直接跳过**：内核不知道身上有什么，
/// 自然算不出该回多少血。这时报"通过"是假的，报"不一致"是冤枉。
fn combat_end_hp_verdict(after: &State, obs: &Obs, next: &Obs, res: &mut FrameResult) {
    if obs.relics.is_empty() {
        res.verdict = Verdict::Skipped;
        res.notes.push(
            "战斗在这一步结束。这条 trace 没录 relics 字段，结束类遗物无从判断".to_string(),
        );
        return;
    }
    if after.player.hp == next.hp {
        res.verdict = Verdict::Match;
        res.notes.push(format!(
            "战斗结束帧：牌区不可比，只比血量 —— {} 血 ✓（结束类遗物已结算）",
            next.hp
        ));
        return;
    }
    let unknown: Vec<&str> = obs
        .relics
        .iter()
        .filter(|r| crate::content::relic_by_id(&r.id).is_none())
        .map(|r| r.name.as_str())
        .collect();
    if !unknown.is_empty() {
        res.diffs.push(Diff::hard("我方.血量(战斗结束)", next.hp, after.player.hp));
        res.verdict = Verdict::UnknownContent;
        res.notes.push(format!(
            "身上有内核不认识的遗物：{} —— 差的这几点血可能是它们回的",
            unknown.join("、")
        ));
    } else {
        res.diffs.push(Diff::soft("我方.血量(战斗结束结算)", next.hp, after.player.hp));
        res.verdict = Verdict::Match;
        res.notes.push(format!(
            "战斗结束帧：终局结算，血量 游戏={} 内核={}（结束类遗物已结算）",
            next.hp, after.player.hp
        ));
    }
}

pub fn combat_over_obs(o: &Obs) -> bool {
    !matches!(o.state_type.as_str(), "monster" | "elite" | "boss" | "hand_select")
}

/// 参与 diff 的 status。
///
/// **两个故意不在里面的**，理由相同 —— 游戏不报这两个量，它们是内核自己
/// 推出来/造出来的，拿去比只会得到一列恒定的假不一致：
/// * [`St::SlowSource`]：谁带缓慢是从敌人身上推的
/// * [`St::VambraceCharge`]：臂甲这场用没用过，观测里根本没有这个字段
pub(crate) const ALL_ST: [St; 81] = [
    St::Strength,
    St::Dexterity,
    St::Vulnerable,
    St::Weak,
    St::Frail,
    St::Artifact,
    St::Intangible,
    St::DemonForm,
    St::Rage,
    St::Slow,
    St::DamageCap,
    St::Shrink,
    St::Pyre,
    St::FeelNoPain,
    St::DarkEmbrace,
    St::Rupture,
    St::CrimsonMantle,
    St::RollingBoulder,
    St::Vicious,
    St::PlatedArmor,
    St::Frenzy,
    St::FlameBarrier,
    St::Barricade,
    St::Entrench,
    St::NoEnergyGain,
    St::AllOrNothing,
    St::Minion,
    St::Illusion,
    St::Regen,
    St::Imbalanced,
    St::Flutter,
    St::EscapeArtist,
    St::Swipe,
    St::Thorns,
    St::CurlUp,
    // 游戏报 `CONSTRICT_POWER`，所以它**进** ALL_ST 参与 diff
    St::Constrict,
    // 游戏报 `INFESTED_POWER`，进 ALL_ST
    St::Infested,
    // 游戏都报（`PLOW_POWER` / `RINGING_POWER`），进 ALL_ST
    St::Plow,
    St::Ringing,
    // 游戏报 `PERSONAL_HIVE_POWER` / `HATCH_POWER`，进 ALL_ST。**层数是观测量**，进 diff 才能守住
    // "层数怎么涨"（人体蜂房会被喷射信息素从 1 加到 3，孵化每回合末掉 1）。
    // 人体蜂房的行为 2026-09-14 建了（挨一段攻击塞层数那么多张晕眩），孵化的还没建。
    St::PersonalHive,
    St::Hatch,
    // 游戏报 `SLUMBER_POWER`（熟睡甲虫，2026-09-14），进 ALL_ST：减层是观测量，进 diff 才守得住。
    St::Slumber,
    // 游戏报 `ASLEEP_POWER`（乐加维林族母，2026-09-17），同上。
    St::Asleep,
    // 游戏报 `SLIPPERY_POWER`（墨影幻灵 / 墨宝，2026-09-17）：减层是观测量，进 diff 才守得住。
    St::Slippery,
    // 游戏报 `TAINTED_POWER` / `VITAL_SPARK_POWER` / `DARK_SHACKLES_POWER`，进 ALL_ST。
    // 行为都没建模，但**层数是观测量**，进 diff 才守得住"层数怎么涨"那一半
    // （每打一张技能牌污染 +2、脉动给活力火花 +2）。
    St::Tainted,
    St::VitalSpark,
    // 游戏报 `SANDPIT_POWER`，进 ALL_ST。行为（即死倒计时）没建模，
    // 但**层数是观测量**，进 diff 才守得住「狂乱逃离让它 +1」那一半。
    St::Sandpit,
    St::Ritual,
    St::Shriek,
    St::Rampart,
    St::Soar,
    // 游戏报 `VIGOR_POWER`，进 ALL_ST。**行为已建模**，所以这一列既守层数
    // 也守"打完一张攻击牌它该归零"——后者正是最容易写错的那一半。
    //
    // **赤牛的 `St::Akabeko` 故意不进**：那是遗物私有量（游戏只报活力这个结果，
    // 不报"赤牛配置了 8"），和 `VambraceCharge` / `Lantern` 同一条理由。
    St::Vigor,
    // 游戏报 `HIGH_VOLTAGE_POWER`，进 ALL_ST。行为已建模，所以这一列同时守着
    // "每个敌人回合它该给自己 +2 力量"（力量本身也在 ALL_ST 里，两列互相印证）。
    St::HighVoltage,
    // 领地意识：和高压同构（敌人回合结束 +1 力量），行为已建模 ⇒ 进 ALL_ST。
    // 力量也在 ALL_ST 里，两列互相印证：漏掉这条规则会让力量那一列当场变红。
    St::Territorial,
    // 光耀：回合开始 +1 能量并掉一层。层数是观测量，行为已建模 ⇒ 进 ALL_ST。
    St::Radiance,

    // **黑暗镣铐故意不进 ALL_ST**：那张牌内核不认识（`card::UNKNOWN`），
    // 所以内核永远产生不出这个 status，放进 diff 只会得到一列必然的假红。
    // 名字仍然映射着，为的是别让整帧降级成 UNKNOWN。
    //
    // **凋萎存在 2026-08-25 进来了。** 它 2026-08-22 被挡在外面，理由是
    // 「内核没有『打出任意一张牌』的钩子，产生不出这个递减，放进去是一列
    // 必然的假红」。那个理由现在不成立了：`Hook::CardPlayed` 建好了，
    // 规则在 `content::POWERS`。
    //
    // 它进来之后守的正是那一半：**每打一张牌该减 1、减到 0 该塞牌并归位 6**。
    // 当年那组读数（游戏 5/3/2/1，内核 6/4/3/2）现在就是判据。
    St::WitheringPresence,
    // 游戏报 `RAVENOUS_POWER`（噬尸蛞蝓开局自带），层数是观测量
    St::Ravenous,
    // **`UnmovableCharge` 故意不在这里** —— 那是内核私有的"本回合还剩几次"，
    // 游戏只报 `UNMOVABLE_POWER` 这个层数。拿私有量去 diff 只会得到一列假不一致，
    // 和 `VambraceCharge` / `SlowSource` 是同一条理由。
    St::Unmovable,
    St::Adaptable,
    // 接续（2026-09-14 进来）：游戏报 `REATTACH_POWER`，行为建了 ⇒ 进 diff。
    // **倒计时 `ReattachDue` 故意不进**：内核私有，尸体整只不在观测里。
    St::Reattach,
    // 知识恶魔的四个诅咒（2026-09-14）：游戏报成 power，行为建了 ⇒ 进 diff。
    St::Disintegration,
    St::MindRot,
    St::Sloth,
    St::WasteAway,
    St::PainfulStabs,
    St::Nemesis,
    St::Smoggy,
    St::Skittish,
    St::SteamEruption,
    St::Burrowed,
    // 第 3 幕补的四个（2026-09-13）：都是游戏报的 power，行为已建模 ⇒ 进 diff。
    St::Galvanic,
    St::PaperCuts,
    St::PossessStrength,
    St::PossessSpeed,
    // 骑士团（2026-09-14）挂在我身上的两个。**施咒者标记 `HexCaster` / `DampenCaster`
    // 故意不进**：那是内核私有的身份量，游戏不报。
    St::Hex,
    St::Dampen,
    // 第 1 幕批 4 / 批 5（2026-09-19）。
    // 硬化外壳的**余额**进 diff —— 游戏报的就是余额（`DisplayAmount`），这一列守的正是
    // 「这一下该吃掉多少额度、回合开始有没有回满」。**上限 `HardenedShellCap` 故意不进**：游戏不报。
    // 吮吸 / 意外 / 偷窃是开局写死、之后不变的层数，进 diff 守「挂没挂上」。
    // **盗窃故意不进**：层数是偷到的金币数，内核给不出（召唤出来的胖地精身上不挂）。
    St::HardenedShell,
    St::Suck,
    St::Surprise,
    St::Thievery,
    // 第 1 幕批 6（2026-09-19）：缠结挂在我身上，进 diff 守「挂没挂上、我的回合末摘没摘」。
    St::Tangled,
];

pub fn st_name(s: St) -> &'static str {
    match s {
        St::StrikeDummy => "打击木偶",
        St::RedSkull => "红头骨",
        St::RedSkullActive => "红头骨·已生效",
        St::PressureGunGrowth => "瀑布巨兽·高压枪已涨",
        St::EruptionDamage => "瀑布巨兽·爆炸伤害",
        // 钢笔尖的三个私有量（游戏显示在遗物上、不报成 status），名字只为 diff 可读
        St::PenNib => "钢笔尖",
        St::PenNibCount => "钢笔尖·攻击计数",
        St::PenNibArmed => "钢笔尖·这一张翻倍",
        St::ScreamingFlagon => "尖叫酒壶",
        St::CloakClasp => "斗篷扣",
        St::HornCleat => "号角靴钉",
        St::RuinedHelmet => "损毁头盔",
        St::TeaSet => "古茶具·武装",
        St::Ravenous => "贪食",
        St::UpgradeOpeningHand => "开局升级手牌",
        St::JeweledMask => "宝石面具",
        St::StoneCracker => "碎石者",
        St::BloodVial => "小血瓶",
        St::Pantograph => "缩放仪",
        St::Burrowed => "钻地",
        St::Galvanic => "流电",
        St::PaperCuts => "纸伤难愈",
        St::PossessStrength => "抢夺力量",
        St::PossessSpeed => "抢夺速度",
        St::Hex => "恶咒",
        St::Dampen => "抑制",
        St::HexCaster => "幽灵骑士·施咒者",
        St::DampenCaster => "魔法骑士·施咒者",
        St::ReattachDue => "接续·复活倒计时",
        St::Disintegration => "瓦解",
        St::MindRot => "心灵腐化",
        St::Sloth => "懒惰",
        St::WasteAway => "虚脱",
        St::SteamEruption => "蒸汽喷发",
        St::Skittish => "胆小",
        St::SkittishTriggered => "胆小·已触发",
        St::Adaptable => "适生力",
        St::PainfulStabs => "剧痛刺击",
        St::Nemesis => "复仇宿敌",
        St::ClawGrowth => "连环爪击·已增长段数",
        St::MoveForcedThisTurn => "本回合被强制改招",
        St::Vigor => "活力",
        St::HighVoltage => "高压",
        St::Territorial => "领地意识",
        // 这两件是**遗物私有量**（游戏不报），名字只用于 diff 的可读性
        St::ParryingShield => "招架盾",
        St::CentennialPuzzle => "百年积木",
        // 2026-08-27 第二批遗物，同样是私有量，名字只为 diff 可读
        St::Anchor => "锚",
        St::BagOfMarbles => "弹珠袋",
        St::RedMask => "红面具",
        St::BagOfPreparation => "准备背包",
        St::Candelabra => "烛台",
        St::HappyFlower => "开心小花",
        St::PollinousCore => "花粉核心",
        St::PaelsFlesh => "佩尔之肉",
        St::PaelsBlood => "佩尔之血",
        St::Kunai => "苦无",
        St::Tender => "娇弱",
        St::Radiance => "光耀",
        St::Reattach => "接续",
        St::Surrounded => "遭到包围",
        St::BackAttackLeft => "左侧",
        St::BackAttackRight => "右侧",
        St::FacingRight => "面朝右",
        St::CrabRage => "蟹之怒",
        St::LetterOpener => "开信刀",
        St::WitheringPresence => "凋萎存在",
        St::RitualArmed => "仪式·已上膛",
        St::Stunned => "被击晕",
        St::FakeHappyFlower => "开心小花？？？",
        St::Unmovable => "坚定不移",
        // 内核私有量（游戏不报），名字只为 diff 好读
        St::UnmovableCharge => "坚定不移·本回合余额",
        St::CurlUp => "蜷身",
        St::Constrict => "缠绕",
        St::Infested => "寄生物",
        St::Plow => "耕地",
        St::Ringing => "轰鸣",
        // 游戏报 `PERSONAL_HIVE_POWER` / `HATCH_POWER`，进 ALL_ST。
        // 人体蜂房的行为建了（规则在 `POWERS`）；孵化没建，欠什么写在 `St` 的注释里。
        St::PersonalHive => "人体蜂房",
        St::Slumber => "熟睡",
        St::Asleep => "沉睡",
        St::Slippery => "滑溜",
        St::Sandpit => "沙坑",
        St::Tainted => "污染",
        St::VitalSpark => "活力火花",
        St::DarkShackles => "黑暗镣铐",
        St::Hatch => "孵化",
        St::Ritual => "仪式",
        St::Shriek => "尖叫",
        St::Rampart => "护壁",
        St::Stock => "库存",
        St::Soar => "翱翔",
        // 遗物私有量，**故意不在 ALL_ST 里**
        St::Akabeko => "赤牛",
        St::MrStruggles => "抱抱先生",
        St::CaptainsWheel => "舵盘",
        St::GremlinHorn => "地精之角",
        St::VambraceCharge => "臂甲充能",
        // 遗物私有量。**故意不在 ALL_ST 里** —— 游戏不把遗物报成 status，
        // 拿它们去 diff 只会得到一列假不一致。名字留着是给 diff 输出用的。
        St::BurningBlood => "燃烧之血",
        St::MeatOnTheBone => "带骨肉",
        // 内核自己造的量（游戏报的是 IMBALANCED_POWER 那个特性，
        // 不报"这一下有没有被完全挡住"），同样不进 ALL_ST
        St::OffBalance => "失衡·已触发",
        // 以下都是遗物私有量，同样**故意不在 ALL_ST 里**
        St::OrnamentalFan => "精致折扇",
        St::MercuryHourglass => "水银沙漏",
        St::Lantern => "灯笼",
        St::Pendulum => "摆动球",
        St::PendulumPhase => "摆动球·相位",
        St::StoneCalendar => "历石",
        St::Orichalcum => "奥利哈钢",
        St::OrichalcumArmed => "奥利哈钢·已武装",
        St::Strength => "力量",
        St::Dexterity => "敏捷",
        St::Vulnerable => "易伤",
        St::Weak => "虚弱",
        St::Frail => "脆弱",
        St::Artifact => "人工制品",
        St::Intangible => "无实体",
        St::DemonForm => "恶魔形态",
        St::Rage => "激怒",
        St::Slow => "缓慢",
        St::DamageCap => "难以杀灭",
        St::Shrink => "缩小",
        St::SlowSource => "缓慢来源",
        St::TempStrength => "临时力量",
        St::Pyre => "薪火之源",
        St::FeelNoPain => "无惧疼痛",
        St::DarkEmbrace => "黑暗之拥",
        St::Rupture => "撕裂",
        St::CrimsonMantle => "绯红披风",
        St::RollingBoulder => "滚石",
        St::Vicious => "凶恶",
        St::PlatedArmor => "覆甲",
        St::Frenzy => "狂怒",
        St::FlameBarrier => "火焰屏障",
        St::Juggernaut => "势不可当",
        St::Colossus => "巨像",
        St::NoDraw => "本回合不能抽牌",
        St::Stampede => "惊逃",
        St::OneTwoPunch => "连环拳",
        St::Juggling => "杂耍",
        St::Aggression => "好勇斗狠",
        St::Barricade => "壁垒",
        St::Entrench => "均衡",
        St::NoEnergyGain => "本回合不再获得能量",
        St::AllOrNothing => "孤注一掷",
        St::Minion => "爪牙",
        St::Illusion => "幻象",
        St::Regen => "再生",
        St::Imbalanced => "失衡",
        St::Flutter => "扑翼",
        St::EscapeArtist => "逃跑大师",
        // 本地化表 `SWIPE_POWER.title` 是「顺走」。原来写的「偷窃」是地精佣兵 `THIEVERY_POWER` 的名字
        // （2026-09-19 从 pck 里取批 5 的名字时对出来的）。
        St::Swipe => "顺走",
        St::Thorns => "荆棘",
        St::Smoggy => "侵蚀",
        // 2026-09-19 第 1 幕批 4 / 批 5。名字取自本地化表 `*_POWER.title`
        St::HardenedShell => "硬化外壳",
        St::HardenedShellCap => "硬化外壳·上限",
        St::Suck => "吮吸",
        St::Surprise => "意外",
        St::Thievery => "偷窃",
        St::Heist => "盗窃",
        St::Tangled => "缠结",
    }
}

/// 给 diff 输出用的可读名。`potion_def` 对 UNKNOWN 会回落到 0 号（"无"），
/// 那会把「不认识的药水」和「空槽」显示成同一个东西，这里分开。
pub fn potion_name(id: u8) -> &'static str {
    match id {
        potion::NONE => "空",
        potion::UNKNOWN => "<内核不认识>",
        _ => crate::ops::potion_def(id).name,
    }
}

/// **新增一瓶时记得同时看 `ops::AUTOMATIC_POTIONS`** —— 自动药水
/// 进了表也不该出现在 `legal_actions` 里。
pub fn map_potion(id: &str) -> u8 {
    let s = id.trim().to_uppercase();
    if s.contains("BLOCK_POTION") || s.contains("格挡") {
        potion::BLOCK
    } else if s.contains("STRENGTH_POTION") || s.contains("力量") {
        potion::STRENGTH
    } else if s.contains("DEXTERITY_POTION") || s.contains("敏捷") {
        potion::DEXTERITY
    } else if s.contains("ENERGY_POTION") || s.contains("能量") {
        potion::ENERGY
    } else if s.contains("FIRE_POTION") || s.contains("火焰") || s.contains("火油") {
        potion::FIRE
    } else if s.contains("FEAR_POTION") || s.contains("VULNERABLE") || s.contains("易伤") || s.contains("恐惧") {
        potion::VULNERABLE
    } else if s.contains("WEAK_POTION") || s.contains("WEAK") || s.contains("虚弱") {
        potion::WEAK
    } else if s.contains("SWIFT_POTION") || s.contains("迅捷") {
        potion::SWIFT
    } else if s.contains("REGEN_POTION") || s.contains("再生") {
        potion::REGEN
    } else if s.contains("EXPLOSIVE_AMPOULE") || s.contains("爆炸") {
        potion::EXPLOSIVE
    } else if s.contains("FAIRY_IN_A_BOTTLE") || s.contains("瓶中精灵") {
        potion::FAIRY
    // 「瓶装潜能」和「瓶中精灵」都以"瓶"开头 ⇒ 两条都用完整词，别用单字匹配
    } else if s.contains("BOTTLED_POTENTIAL") || s.contains("瓶装潜能") {
        potion::BOTTLED_POTENTIAL
    } else if s.contains("CURE_ALL") || s.contains("痊愈药水") {
        potion::CURE_ALL
    } else if s.contains("RADIANT_TINCTURE") || s.contains("明耀酊剂") {
        potion::RADIANT_TINCTURE
    // 注意次序：`ATTACK_POTION` 必须排在含 "ATTACK" 的宽匹配之前，
    // 而 `OROBIC_ACID` 和"酸"都不含别的瓶子的关键字，放哪都行。
    } else if s.contains("ATTACK_POTION") || s.contains("攻击药水") {
        potion::ATTACK
    } else if s.contains("SKILL_POTION") || s.contains("技能药水") {
        potion::SKILL
    // 能力药水。**"能力"和上面那个"能量"只差一个字**，靠的是两条都用完整词
    // 匹配（`能量` / `能力`）而不是单字，所以不会互相吃掉。
    } else if s.contains("POWER_POTION") || s.contains("能力药水") {
        potion::POWER
    } else if s.contains("OROBIC_ACID") || s.contains("欧洛巴斯") {
        potion::OROBIC_ACID
    } else if s.contains("HEART_OF_IRON") || s.contains("铁心") {
        potion::HEART_OF_IRON
    } else if s.contains("FYSH_OIL") || s.contains("鱼油") {
        potion::FYSH_OIL
    } else if s.contains("FLEX_POTION") || s.contains("屈伸") {
        potion::FLEX
    } else if s.contains("SNECKO_OIL") || s.contains("异蛇") {
        potion::SNECKO_OIL
    // 石化蟾蜍塞的那瓶。名字里不含任何别的瓶子的关键字，位置无所谓。
    } else if s.contains("POTION_SHAPED_ROCK") || s.contains("药水形状") {
        potion::POTION_SHAPED_ROCK
    } else if s.is_empty() {
        potion::NONE
    } else {
        potion::UNKNOWN
    }
}

/// 把一条 trace 切成**回合段**：`[开头帧, 该回合最后一个动作帧)`。
///
/// 段的开头就是"回合开始、手牌已经抽好"的那一帧 —— 跨回合的东西
/// （`bin/rollout` 的四条判据、`bin/calib` 的标定样本）都从这里起跳，
/// 所以**这份切法只能有一处**。
pub fn turn_segments(t: &Trace) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let n = t.frames.len();
    let mut i = 0usize;
    while i + 1 < n {
        if t.frames[i].action.is_none() {
            break;
        }
        let mut j = i;
        while j < n && !matches!(t.frames[j].action, Some(Act::EndTurn) | None) {
            j += 1;
        }
        out.push((i, j));
        i = j + 1;
    }
    out
}

/// 观测到的卡名 -> 内容表 id。升级牌的标题可能带 `+`，先剥掉。
/// 游戏把「假升级」的层数**写进牌名**：[源码] `Wither.Title` 在基础名后面追加
/// `+{FakeUpgradeLevel}`（`凋萎+1` / `凋萎+2`）。
///
/// 和普通升级牌区分得开：那些的名字是**光秃秃一个 `+`**（`拆卸+`），
/// 后面不带数字。所以「`+` 后面跟数字」这个形状只有假升级会出现。
///
/// 返回 (基础名, 假升级层数)。不是这个形状就原样返回、层数 0。
///
/// **这一条 2026-08-25 之前不存在，后果是静默的**：`凋萎+1` 查不到牌，
/// 落成 `card::UNKNOWN`，于是它的回合末 3 点伤害整个消失 ——
/// 第 3 幕 Boss 那条实录里 82 张凋萎有 82 张是这个形状。
pub fn split_fake_upgrade(name: &str) -> (&str, i32) {
    let n = name.trim();
    let Some(p) = n.rfind('+') else { return (n, 0) };
    let tail = &n[p + 1..];
    if tail.is_empty() || !tail.bytes().all(|b| b.is_ascii_digit()) {
        return (n, 0);
    }
    match tail.parse::<i32>() {
        Ok(lv) => (&n[..p], lv),
        Err(_) => (n, 0),
    }
}

/// 一层假升级值多少。[源码] `Wither.FakeUpgrade` -> `UpgradeValueBy(3m)`。
/// **今天只有凋萎一张牌用假升级**，所以这个常数就写在这里，
/// 将来出现第二张再谈要不要进表。
pub const FAKE_UPGRADE_STEP: i16 = 3;

pub fn lookup_card(name: &str) -> Option<u16> {
    let (name, _) = split_fake_upgrade(name);
    let base = name.trim_end_matches('+').trim();
    let base = match base {
        "撕成碎片" | "tear_asunder" => "扯碎",
        b => b,
    };
    crate::content::CARDS
        .iter()
        .position(|d| d.name == base)
        .map(|i| i as u16)
        .filter(|&i| i != card::UNKNOWN)
}

fn lookup_enemy(name: &str) -> bool {
    enemy_id(name).is_some()
}

/// 敌人的**名字**（游戏报的中文名）-> `content::ENEMIES` 的下标。
///
/// **名字解析只有这一处实现**：直配 -> 截 `#` 编号后缀 -> 下面那张别名表。
/// `tools/dump_encounters.py` 读的也是这张表（它明说了"只读不复制"），
/// `synth::encounters` 走的是这个函数 —— 三处口径因此不可能长歪。
pub fn enemy_id(name: &str) -> Option<u16> {
    let n = name.trim();
    // 优先直接匹配中文名
    if let Some(pos) = crate::content::ENEMIES.iter().position(|d| d.name == n) {
        return Some(pos as u16);
    }
    // **带编号后缀的敌人**：游戏报的是「实验体 #C10」，而编号**每一局都不一样**
    // （2026-08-14 那局是 #C8，2026-08-30 这局是 #C10）。别名表天生覆盖不了它，
    // 只能按前缀截。
    //
    // 这一条是「名字就是连接键」那个老教训的第二种形态：
    // 上一次是名字**拼错**（永世沙漏），这一次是名字**带变量**。两种都让
    // `--live` 报「1 只里认出 0 只 / 跨回合推演不可用」，也就是**建了等于没建**。
    //
    // 只截 `#` 及其之后，且要求 `#` 前面有内容 —— 不去动别的名字。
    if let Some(p) = n.find('#') {
        let head = n[..p].trim_end();
        if !head.is_empty() {
            if let Some(pos) = crate::content::ENEMIES.iter().position(|d| d.name == head) {
                return Some(pos as u16);
            }
        }
    }
    // 别名与 ID 映射
    let lower = n.to_ascii_lowercase();
    let alias = match lower.as_str() {
        "thieving_hopper" | "thieving_hopper_0" | "thieving-hopper" | "偷盗跳虫" | "偷窃草蜢" => "偷窃草蜢",
        "bowlbug_rock" | "bowlbug-rock" | "盛碗虫（石）" => "盛碗虫（石）",
        "bowlbug_egg" | "bowlbug-egg" | "盛碗虫（卵）" => "盛碗虫（卵）",
        "bowlbug_nectar" | "bowlbug-nectar" | "盛碗虫（花蜜）" => "盛碗虫（花蜜）",
        "bowlbug_silk" | "bowlbug-silk" | "盛碗虫（蚕丝）" => "盛碗虫（蚕丝）",
        "chomper" | "大啃兽" => "大啃兽",
        "louse_progenitor" | "louse-progenitor" | "始祖虱虫" => "始祖虱虫",
        // 类名 key 给 `tools/dump_encounters.py` 用（没有实录的敌人只能靠它连上）。
        // 知识恶魔 / 异螨 / 熟睡甲虫都是 2026-09-14 照 [源码] 建的，中文名取自本地化表、直配。
        // 异螨这个下标原来是一条错的 [wiki]「螨虫」，那个旧名**故意不收** —— 游戏里没有叫它的怪。
        "knowledge_demon" => "知识恶魔",
        "myte" => "异螨",
        "slumbering_beetle" => "熟睡甲虫",
        "ovicopter" | "直升虫" => "直升虫",
        "spiny_toad" | "spiny_toad_0" | "spiny-toad" | "spiny-toad-0" | "棘蟾" | "棘刺蟾蜍" | "多刺蟾蜍" => "棘蟾",
        "queen" | "蜂后" => "蜂后",
        "doormaker" | "造门者" => "造门者",
        "soul_nexus" | "soul-nexus" | "灵魂枢纽" => "灵魂枢纽",
        "test_subject" | "test_subject_0" | "test-subject" | "实验体" => "实验体",
        "tunneler" | "tunneler_0" | "tunneler-0" | "地道虫" => "地道虫",
        // 2026-09-13 第 3 幕补的六只。中文名直配（取自游戏本地化表），这里只放**类名 key**，
        // 给 `tools/dump_encounters.py` 从 [源码] 遭遇表连过来用（它按 key 小写查这张表）。
        "slimed_berserker" => "史莱姆狂战士",
        "mecha_knight" => "机甲骑士",
        "globe_head" => "电球头",
        "scroll_of_biting" => "咬人卷轴",
        "the_lost" => "失落之物",
        "the_forgotten" => "遗忘之物",
        "flail_knight" => "连枷骑士",
        "spectral_knight" => "幽灵骑士",
        "magi_knight" => "魔法骑士",
        // 2026-09-17 第 1 幕批 2。中文名直配（本地化表 `LAGAVULIN_MATRIARCH.name`），
        // 这里只放**类名 key**，给 `tools/dump_encounters.py` 从 [源码] 遭遇表连过来用。
        "lagavulin_matriarch" => "乐加维林族母",
        // 2026-09-17 第 1 幕批 3，同上。
        "vantom" => "墨影幻灵",
        "inklet" => "墨宝",
        // 2026-09-19 第 1 幕批 4 / 批 5（暗港精英 + 暗港杂兵），同上。
        "skulking_colony" => "鬼祟珊瑚群",
        "haunted_ship" => "幽灵船",
        "toadpole" => "蟾蜍蝌蚪",
        "fossil_stalker" => "化石追踪者",
        "gremlin_merc" => "地精佣兵",
        "sneaky_gremlin" => "卑鄙地精",
        "fat_gremlin" => "胖地精",
        // 2026-09-19 第 1 幕批 6（密林杂兵），同上。另外三种劫掠者早就有实录连接键（`act1_f12_seventh`）。
        "vine_shambler" => "藤蔓蹒跚者",
        "assassin_ruby_raider" => "劫掠者刺客",
        "brute_ruby_raider" => "劫掠者暴徒",
        _ => return None,
    };
    crate::content::ENEMIES.iter().position(|d| d.name == alias).map(|i| i as u16)
}

// --------------------------------------------------------------------------
// 报告
// --------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    /// 检查的字段全一致。
    Match,
    /// 只做了部分检查（`end_turn`：敌人行动不由内核预测）。
    Partial,
    /// 内容都认识，但状态对不上。**唯一要修的信号。**
    Mismatch,
    /// 牌/敌人不在内容表里，跳过不比。不是失败。
    UnknownContent,
    /// v1 还不支持的动作（选牌界面）。
    Skipped,
}

#[derive(Clone, Debug)]
pub struct Diff {
    pub field: String,
    pub game: String,
    pub kernel: String,
    /// 硬 diff 判定为 MISMATCH；软 diff 只报告，不算失败。
    pub hard: bool,
}

impl Diff {
    fn hard(field: impl Into<String>, game: impl ToString, kernel: impl ToString) -> Diff {
        Diff {
            field: field.into(),
            game: game.to_string(),
            kernel: kernel.to_string(),
            hard: true,
        }
    }
    fn soft(field: impl Into<String>, game: impl ToString, kernel: impl ToString) -> Diff {
        Diff {
            field: field.into(),
            game: game.to_string(),
            kernel: kernel.to_string(),
            hard: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FrameResult {
    pub i: usize,
    pub action: String,
    pub verdict: Verdict,
    pub diffs: Vec<Diff>,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct CardSighting {
    pub id: String,
    pub name: String,
    pub cost: Option<i32>,
    pub kind: String,
    pub description: String,
    pub count: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub run: String,
    pub results: Vec<FrameResult>,
    /// 待导入的牌：第 3 步「灌内容」的直接输入。
    pub missing_cards: BTreeMap<String, CardSighting>,
    pub missing_enemies: BTreeMap<String, i32>,
    /// 用过但内核不认识的药水 -> 次数。
    pub missing_potions: BTreeMap<String, u32>,
    /// 见过的意图 `类型 -> {标签}`。用来搞清标签的真实格式（尤其是多段攻击），
    /// 以及"玩家带上易伤之后同一个招式的标签会不会变"。
    pub seen_intents: BTreeMap<String, std::collections::BTreeSet<String>>,
    /// 内核没有对应概念的 status id -> 出现次数。补 `map_status` 用。
    pub unmapped_status: BTreeMap<String, u32>,
    /// **见过但没建全的附魔 id**（表里没有、或者有但 `modelled: false`）。
    /// 补 `content::ENCHANTS` 用。
    ///
    /// 它不是装饰：一张带没建模附魔的牌，内核会照卡表的基础值算，
    /// 而游戏用的是改过的值 —— 差多少不知道，方向也不知道。
    /// 所以这里点名，不静默。
    pub unknown_enchantments: std::collections::BTreeSet<String>,
    pub settle_unstable: u32,
    pub settle_intermediate: u32,
    pub settle_max_ms: i32,
}

impl Report {
    pub fn count(&self, v: Verdict) -> usize {
        self.results.iter().filter(|r| r.verdict == v).count()
    }
    pub fn ok(&self) -> bool {
        self.count(Verdict::Mismatch) == 0
    }
}

// --------------------------------------------------------------------------
// 同步：观测 -> State
// --------------------------------------------------------------------------

/// 同步出来的状态，外加每张牌实例对应的观测卡名。
///
/// 名字必须单独带着走：牌堆里的牌可能是内容表还没有的牌，内核里它们全是
/// `<未知牌>`，直接用内容表的名字去比会把一堆不同的未知牌混成一个。
pub struct Synced {
    pub state: State,
    pub names: Vec<String>,
    /// 本帧同步时遇到的、内核没有对应概念的 status id。
    ///
    /// 非空就意味着这一帧的结果**可能**取决于内核看不见的东西，所以不一致
    /// 应该判为「内容缺失」而不是「内核算错」。实录第一场就是这样：
    /// `SHRINK_POWER` 让每张牌少打 2 点，内核完全看不到它。
    pub unmapped: Vec<String>,
}

const DRAW_PLACEHOLDER: &str = "<抽牌堆未知>";

/// 回合内累计的计数器。**观测里根本没有这些字段**，所以每帧同步都会把它们
/// 清零，由 `Replayer` 自己按帧累加、跨回合重置。
///
/// 不补这个的话，凡是读这些计数的牌都会报**假 MISMATCH**——踩踏是最典型的：
/// 游戏按"本回合打过 N 张攻击牌"减费收 1 点能量，内核从 0 开始算收 3 点，
/// 能量对不上。那看起来和真 bug 一模一样，最浪费时间。
///
/// 注意这**不是**整回合预测：每帧仍然从观测重新同步，只有观测里没有的这几个
/// 量靠自己带。整回合预测是另一回事（连着跑完一整回合再比）。
#[derive(Clone, Copy, Default)]
struct TurnCounters {
    cards_played: i32,
    attacks_played: i32,
    skills_played: i32,
    exhausted_this_turn: i32,
    hp_lost_this_turn: i32,
    free_attack: i32,
    /// 坚定不移的「本回合已经从卡牌拿过几次格挡」。
    ///
    /// **它是个每回合计数器，不是遗物私有量** —— 来源是一张牌（坚定不移），
    /// 所以进不了 `relic_carry`（那一栏只装遗物的 `private_status`）。
    /// 不带过来的话每帧 `sync` 都把它清零，于是**每一张格挡牌都当成
    /// "本回合第一次"**，整场的格挡全部算高。
    /// [实测] 2026-08-30 第 3 幕第 45 层帧 11：挑衅+12 之后的防御+，
    /// 游戏 18（12+6）、内核 24（12+12）。
    unmovable_charge: i32,
    /// 胆小（花园幽灵鳗）的「本回合是否已经获得过格挡」。
    /// 游戏不报这个私有标志，如果不跨帧带过去，同一回合的后续攻击打上去
    /// 会被当成未触发而再次获得格挡。
    skittish_triggered: [i32; MAX_ENEMIES],
}

impl Replayer {
    /// 每帧跑完之后，把**观测里没有的量**存下来带到下一帧：
    /// 每回合计数器 + 遗物给的 status。
    ///
    /// 收成一个函数是因为这两样必须一起存 —— 早先只存计数器，
    /// 遗物那半没人管，臂甲就会每帧复活一次。
    fn save_carry(&mut self, s: &State) {
        self.counters = TurnCounters::save_from(s);
        // 整场累计，**不跟着回合清零** —— 和 `relic_carry` 同一档作用域。
        self.hp_loss_hits = s.hp_loss_hits;
        for (st, v) in self.relic_carry.iter_mut() {
            *v = s.player.get(*st);
        }
        self.pending = s.pending;
    }
}

impl TurnCounters {
    fn load_into(self, s: &mut State) {
        s.cards_played = self.cards_played;
        s.attacks_played = self.attacks_played;
        s.skills_played = self.skills_played;
        s.exhausted_this_turn = self.exhausted_this_turn;
        s.hp_lost_this_turn = self.hp_lost_this_turn;
        s.free_attack = self.free_attack;
        s.player.set(St::UnmovableCharge, self.unmovable_charge);
        for i in 0..MAX_ENEMIES {
            s.enemies[i].set(St::SkittishTriggered, self.skittish_triggered[i]);
        }
    }
    fn save_from(s: &State) -> TurnCounters {
        let mut skittish = [0; MAX_ENEMIES];
        for i in 0..MAX_ENEMIES {
            skittish[i] = s.enemies[i].get(St::SkittishTriggered);
        }
        TurnCounters {
            cards_played: s.cards_played,
            attacks_played: s.attacks_played,
            skills_played: s.skills_played,
            exhausted_this_turn: s.exhausted_this_turn,
            hp_lost_this_turn: s.hp_lost_this_turn,
            free_attack: s.free_attack,
            unmovable_charge: s.player.get(St::UnmovableCharge),
            skittish_triggered: skittish,
        }
    }
}

/// 往 `relic_carry` 里加一条：**同一个 status 已经在里面就相加**。
///
/// 原来这里是无脑 `push`，而下面那个循环是逐条 `set` —— 于是**两件遗物给同一个
/// status 时后一条把前一条盖掉**。真的会发生：锚（10）+ 假锚（4）同时在身上。
/// [实测] 2026-09-09 `act3_f46_soul_nexus` 第 0 帧玩家格挡是 **14**，
/// 正是 10+4；`bin/synth_audit` 就是拿这个把两条路的分歧比出来的
/// （合成路径走 `step::grant_relic`，那边一直是 `add`）。
fn push_carry(carry: &mut Vec<(St, i32)>, st: St, v: i32) {
    if let Some(slot) = carry.iter_mut().find(|(s, _)| *s == st) {
        slot.1 += v;
    } else {
        carry.push((st, v));
    }
}

/// 一个槽位**上一次被观测到**时的样子，见 [`Replayer::seen`]。
#[derive(Clone, Default)]
struct SeenEnemy {
    round: i32,
    max_hp: i32,
    name: String,
}

/// 把观测同步成内核状态的那一半对拍器。
///
/// L2 的验收（`bin/solve.rs`）复用它，理由和 `verify` 一样：**观测怎么变成
/// 状态只该有一处实现**。两处各写一遍的话，求解器和对拍器会在不同的局面上
/// 跑，比出来的差说不清是策略差还是同步差。
pub struct Replayer {
    /// combat_id -> 内核敌人槽位。敌人死后从观测里消失，但槽位保留，
    /// 否则后面的下标会整体平移（见 trace-format.md 约束 1）。
    slots: BTreeMap<String, usize>,
    /// 见 [`TurnCounters`]。`sync` 会自动把它灌进同步出来的状态。
    counters: TurnCounters,
    /// 上一帧的回合数，用来发现回合切换并重置计数器。
    last_round: i32,
    /// 遗物给的 status 的**当前值**。观测里没有这些量（游戏不报"臂甲这场
    /// 用过没有"），所以内核自己带着：第一帧从遗物表初始化，之后每帧跟着走。
    ///
    /// 和 [`TurnCounters`] 同一个道理，区别只是作用域 —— 那个是一个回合，
    /// 这个是**一整场战斗**。
    relic_carry: Vec<(St, i32)>,
    relic_init: bool,
    /// 本场战斗里我挨穿过几次（`State::hp_loss_hits`）。观测里没有这个量，
    /// 所以和 `relic_carry` 一样由内核自己带着 —— 区别是它**还有一个观测来源**
    /// （扯碎的卡面，见 `observed_tear_hits`），而观测赢过携带值。
    ///
    /// 携带值本身只是**下界**：它只数得到内核自己模拟过的那些帧。
    hp_loss_hits: u8,
    /// 每个槽位**上一次被观测到时**身上的适生力层数。
    ///
    /// 观测层根本表达不了「死着等复活」：实验体被砍掉一条命之后整个我方回合
    /// 都不在 `enemies` 里（[实测] 那几帧 `enemies: []`），和"已经没了"长得
    /// 一模一样。`sync` 每帧从观测重建，于是适生力一丢内核当场判战斗结束 ——
    /// 后面的出牌全被 `legal_actions` 拒掉、`EndTurn` 变成空操作（能量不回满、
    /// 格挡不清零），还会白结算一次胜利遗物。
    ///
    /// 所以这一栏和 `relic_carry` 同一个道理：**观测里没有、内核自己带着**。
    /// 判据只有一条 —— 消失前身上有适生力就是"欠一次复活"，
    /// 没有就是真死了（[源码] 第三形态不带适生力，砍掉就结束）。
    /// 复活之后它自己会带着适生力回到观测里，这一栏跟着覆盖。
    revive_owed: [i32; MAX_ENEMIES],
    /// 同上，但记的是**幻象**（`St::Illusion`，寄生惧魔 / 利齿之眼）。
    ///
    /// **和适生力分开存，因为放回去的是不同的 status** —— 合成一栏就得再记
    /// "当时是哪一种"，那还是两个数。
    ///
    /// [实测] `act1_f15_ninth` 帧5 打死利齿之眼，帧6/7 观测里**整只消失**，
    /// 帧8 它 6/6 回来 —— 和实验体那几帧是同一个观测形状。
    illusion_owed: [i32; MAX_ENEMIES],
    /// 同上，记的是**接续**（`St::Reattach`，残杀千足虫）。
    ///
    /// 和那两栏的区别是**光记层数不够，还要知道它是什么时候消失的**：接续不在下一个
    /// 敌人回合，而在第二个（[源码] 死后先走 `DEAD_MOVE` 再走 `REATTACH_MOVE`），
    /// 所以放回 `St::ReattachDue` 时要知道已经过了几个敌人回合 —— 读 [`Replayer::seen`]。
    /// [实测] `act2_f28_decimillipede`：第 3 回合砍死、第 4 回合观测里没有、第 5 回合 25 血回来。
    reattach_owed: [i32; MAX_ENEMIES],
    /// 每个槽位**上一次被观测到**时的回合数 / 最大生命 / 名字。今天只给接续的尸体用：
    /// 回合数推倒计时；最大生命是回血的上限（尸体的实体是清零重建的，不放回去就只能回到 0）；
    /// 名字让 `identify_enemies` 把尸体认回 `EnemyDef`。
    seen: [SeenEnemy; MAX_ENEMIES],
    /// 跨帧携带的选牌状态（Op 产生的 Pending）。
    pending: Pending,
    /// 这一局的进阶等级，每帧灌进 `State::ascension`。
    ///
    /// **观测里没有这个量** —— 它在 trace 的 `run` 里，是一局的常数，
    /// 所以和 `relic_carry` 一样由 `Replayer` 带着。
    ascension: u8,
    report: Report,
}

impl Replayer {
    pub fn new(run: &str) -> Replayer {
        let mut report = Report::default();
        report.run = run.to_string();
        Replayer {
            slots: BTreeMap::new(),
            counters: TurnCounters::default(),
            // -1：第一帧的回合数一定和它不同，于是计数器从干净状态开始
            last_round: -1,
            relic_carry: Vec::new(),
            relic_init: false,
            hp_loss_hits: 0,
            revive_owed: [0; MAX_ENEMIES],
            illusion_owed: [0; MAX_ENEMIES],
            reattach_owed: [0; MAX_ENEMIES],
            seen: Default::default(),
            pending: Pending::None,
            ascension: 0,
            report,
        }
    }

    /// 从一条 trace 建 —— **比 [`Replayer::new`] 多带一个进阶等级**。
    ///
    /// 凡是手上有 `Trace` 的调用方都该走这一个。走 `new(&t.run)` 会拿到
    /// `ascension = 0`，于是 A8/A9 的敌人数值**静默地**不生效
    /// （今天全部语料都是 A1/A2，看不出差别 —— 正因如此才容易漏）。
    pub fn for_trace(t: &Trace) -> Replayer {
        let mut r = Replayer::new(&t.run);
        r.ascension = t.ascension;
        r
    }

    /// 某个 `combat_id` 已经分到的槽位（**不**分配新的）。
    /// 威胁表是按槽位索引的，所以从观测建 `Threat` 要走这里。
    pub fn slot_of_existing(&self, combat_id: &str) -> Option<usize> {
        self.slots.get(combat_id).copied()
    }

    fn slot_of(&mut self, combat_id: &str) -> Option<usize> {
        if let Some(&s) = self.slots.get(combat_id) {
            return Some(s);
        }
        let n = self.slots.len();
        if n >= MAX_ENEMIES {
            return None;
        }
        self.slots.insert(combat_id.to_string(), n);
        Some(n)
    }

    pub fn sync(&mut self, obs: &Obs) -> Synced {
        let mut unmapped: Vec<String> = Vec::new();
        let mut s = State::new(obs.hp, 0);
        // 一局的常数，观测里没有，`Replayer` 带着（见 `Replayer::for_trace`）
        s.ascension = self.ascension;
        s.player.max_hp = obs.max_hp;
        s.player.block = obs.block;
        for (id, amt) in &obs.status {
            match map_status(id) {
                // **值要过一次 `status_value`**：面板上那个数和内核这一栏
                // 不一定是同一个量（娇弱、凌虐两个例外，逐条理由在那个函数上）。
                Some(st) => s.player.set(st, status_value(id, st, *amt)),
                // 有消费者、但不是一个 `St` 的那几个：**别报成"没检查"**。
                // 报表那句「这些字段没被检查」对它们是假话，而假的报表会让人
                // 去补一条本来就不该补的映射。
                None if STATUS_HANDLED_OUTSIDE_MAP.contains(&id.as_str()) => {}
                None => {
                    *self.report.unmapped_status.entry(id.clone()).or_insert(0) += 1;
                    unmapped.push(id.clone());
                }
            }
        }
        // **`free_attack` 是那批"每回合计数器"里唯一一个观测得到的**
        // （无情猛攻挂的 `FREE_ATTACK_POWER`），所以按仓库老规矩以观测为准。
        // 这里就要算出来 —— 下面那个手牌循环要用它判「这张牌显示 0 费是不是
        // 内核自己算得出来的」，而 `counters.load_into` 要到本函数末尾才跑。
        //
        // 不读观测有一个真实的洞：`--live` 只有单帧、没有历史，携带值恒为 0，
        // 于是打完无情猛攻之后求解器既以为下一张攻击还要花钱（低估自己），
        // 又会把手里每张攻击牌都盖上「本回合免费」（高估自己）。
        let free_attack = observed_free_attack(&obs.status).unwrap_or(self.counters.free_attack);
        s.energy = obs.energy;
        s.base_energy = obs.max_energy;
        s.turn = obs.round;
        // 槽位数是**观测量**。缺了（老 trace）就退回"见过的最大槽位号 + 1"，
        // 至少不会比真相小；再兜底一个 3。**不从遗物反推**，理由见 `Obs`。
        s.potion_slots = obs
            .max_potion_slots
            .unwrap_or_else(|| obs.potions.iter().map(|p| p.slot + 1).max().unwrap_or(3).max(3))
            .min(MAX_POTIONS) as u8;
        for p in obs.potions.iter() {
            if p.slot < MAX_POTIONS {
                // id 优先，缺了退回中文名 —— `map_potion` 两种都认
                let key = if p.id.is_empty() { &p.name } else { &p.id };
                s.potions[p.slot] = map_potion(key);
            } else {
                // 容量被顶破了。**必须报出来**：静默跳过等于求解器在一副
                // 更少药水的手上求最优，而输出里完全看不出来。
                // 顶破的原因只会是新遗物（药瓶皮套就是新加的），
                // 修法是调大 `MAX_POTIONS`，不是接受这一瓶丢掉。
                unmapped.push(format!(
                    "药水槽位 {} 超出容量 {}（{}）—— 调大 MAX_POTIONS",
                    p.slot,
                    MAX_POTIONS,
                    if p.name.is_empty() { &p.id } else { &p.name }
                ));
            }
        }

        // 敌人：一律用 UNKNOWN def（内核不预测敌人行动），但真实 HP/状态照搬
        let mut n_slots = 0usize;
        for e in &obs.enemies {
            let Some(slot) = self.slot_of(&e.combat_id) else { continue };
            s.enemies[slot] = Entity { hp: e.hp, max_hp: e.max_hp, block: e.block, status: [0; N_STATUS] };
            for (id, amt) in &e.status {
                match map_status(id) {
                    Some(st) => s.enemies[slot].set(st, status_value(id, st, *amt)),
                    None => {
                        *self.report.unmapped_status.entry(id.clone()).or_insert(0) += 1;
                        unmapped.push(id.clone());
                    }
                }
            }
            // 缓慢的累加规则要内核自己跑，但「谁带缓慢」是观测信息（和 max_hp
            // 一样属于身份，不属于预测）。敌人一律用 UNKNOWN def，查不到
            // `start_status`，所以在这里从观测补上。
            if observed_has_slow(&e.status) {
                s.enemies[slot].set(St::SlowSource, 10);
            }
            // 「它是不是施咒者」同样是身份：幽灵骑士 / 魔法骑士身上挂私有标记，
            // 解咒规则才找得到该在谁死的时候发作。名字走 `enemy_id`（截 `#`、别名），
            // 和合成路径的 `begin_combat` 读同一张表（`content::ENEMY_PRIVATE_MARKERS`）。
            // 鬼祟珊瑚群的硬化外壳**上限**也走这里：观测只报余额，上限（20）是这只怪的身份。
            if let Some(id) = enemy_id(&e.name) {
                for (st, v) in crate::content::enemy_private_markers(crate::content::enemy_def(id).name) {
                    s.enemies[slot].set(st, v);
                }
            }
            s.enemy_def[slot] = enemy::UNKNOWN;
            sync_attack_counters(e, &mut s.enemies[slot]);
            n_slots = n_slots.max(slot + 1);
            // 看得见它的时候记下适生力，看不见的时候才知道它是"欠一次复活"
            // 还是真死了。见 `revive_owed`。
            self.revive_owed[slot] = s.enemies[slot].get(St::Adaptable);
            self.illusion_owed[slot] = s.enemies[slot].get(St::Illusion);
            self.reattach_owed[slot] = s.enemies[slot].get(St::Reattach);
            self.seen[slot] = SeenEnemy { round: obs.round, max_hp: e.max_hp, name: e.name.clone() };
            if !lookup_enemy(&e.name) {
                self.report.missing_enemies.insert(e.name.clone(), e.max_hp);
            }
        }
        // 已经死掉、从观测里消失的敌人：保留槽位，hp=0
        for (_, &slot) in self.slots.iter() {
            n_slots = n_slots.max(slot + 1);
        }
        for slot in 0..n_slots {
            if !obs.enemies.iter().any(|e| self.slots.get(&e.combat_id) == Some(&slot)) {
                s.enemies[slot] = Entity { hp: 0, max_hp: 0, block: 0, status: [0; N_STATUS] };
                s.enemy_def[slot] = enemy::UNKNOWN;
                // **「还在场上」不等于「活着」**（`State::any_enemy_present`）。
                // 血留 0：它确实打不到、也不该被当成攻击目标；只把适生力放回去，
                // 战斗因此不结束。回满血是它自己回合的事（`TOp::OwnerHealToFull`），
                // 而下一帧的观测本来就把复活后的血量报出来了。
                if self.revive_owed[slot] > 0 {
                    s.enemies[slot].set(St::Adaptable, self.revive_owed[slot]);
                    if let Some(def) = enemy_id(&self.seen[slot].name) {
                        s.enemies[slot] = crate::step::reviving_enemy_snapshot(
                            def, self.seen[slot].max_hp, self.revive_owed[slot], s.ascension);
                        // Leave enemy_def UNKNOWN on the injection/replay path.
                        s.enemy_move[slot] = 0;
                    }
                }
                // 幻象走的是同一条路，但**不进 `any_enemy_present`** ——
                // 它是爪牙，主人死了它就该跟着消失（`step::no_master_left`）。
                // 放回这个标记只是为了让它自己回合开始那条回满血的规则还在。
                if self.illusion_owed[slot] > 0 {
                    s.enemies[slot].set(St::Illusion, self.illusion_owed[slot]);
                    s.enemies[slot].set(St::Minion, 1);
                }
                // 接续（残杀千足虫）：同样不进 `any_enemy_present` —— 别的节全死了战斗就该结束。
                // 放回的是层数（回血量）+ 倒计时 + 最大生命（回血的上限，实体刚被清零重建过）。
                //
                // **倒计时从"它最后一次被看见是第几回合"推**：同一回合里 = 2（还没过敌人回合），
                // 下一回合 = 1。[实测] `act2_f28_decimillipede` 第 3 回合砍死、第 5 回合回来。
                // 分不开的一种：被荆棘/火焰屏障**在敌人回合里**反伤打死的那一节，
                // 源码里要再晚一个回合回来，这里会早一回合 —— 方向是悲观，而且极少见。
                if self.reattach_owed[slot] > 0 {
                    let seen = &self.seen[slot];
                    s.enemies[slot].max_hp = seen.max_hp;
                    s.enemies[slot].set(St::Reattach, self.reattach_owed[slot]);
                    s.enemies[slot].set(St::ReattachDue, (2 - (obs.round - seen.round)).max(1));
                }
            }
        }
        s.n_enemies = n_slots as u8;

        // 牌区
        let mut names: Vec<String> = Vec::new();
        // `ench` 收的是 `(附魔下标+1, Amount)`，`(0, 0)` = 没附魔。
        // **附魔会顺手改 flags**（王室认证给固有+保留），所以它必须在这里
        // 落地而不是在调用点 —— 四个牌区共用这一个入口。
        let push = |s: &mut State,
                    names: &mut Vec<String>,
                    id: u16,
                    upg: bool,
                    name: &str,
                    ench: (u8, i32)|
         -> u8 {
            let ix = s.n_cards;
            // 假升级（`凋萎+1`）进 `bonus`。**观测到的牌名是权威的另一半** ——
            // 和费用那条同一个道理：层数是游戏当场报出来的，内核不用自己数
            // Boss 用过几次剧烈增强。
            let bonus = split_fake_upgrade(name).1 as i16 * FAKE_UPGRADE_STEP;
            let mut flags = if upg { F_UPGRADED } else { 0 };
            if let Some(def) = crate::content::ENCHANTS.get(ench.0.wrapping_sub(1) as usize) {
                if ench.0 != 0 {
                    flags |= def.keywords;
                }
            }
            s.cards[ix as usize] = CardInst {
                id,
                flags,
                bonus,
                cost_delta: 0,
                ench: ench.0,
                ench_amt: ench.1.clamp(-127, 127) as i8,
            };
            s.n_cards += 1;
            names.push(name.to_string());
            ix
        };

        // 钢笔尖的**预览**：计数器到 9 时手牌里攻击牌渲染的是翻倍后的数。
        // 只有痛殴的加值反推读卡面文本，所以这里只为它算一次。
        //
        // **不能从 `s.player` 读** —— 遗物那一段（`relic_carry`）在本函数**末尾**
        // 才跑，这会儿 `PenNibCount` 还是 0。观测里那个计数器是权威，直接读它。
        let pen_nib_preview_double = obs.relics.iter().any(|r| {
            r.id == "PEN_NIB" && r.counter.map_or(false, |c| c.rem_euclid(10) == 9)
        });
        for c in &obs.hand {
            if s.n_cards as usize >= MAX_CARDS {
                break;
            }
            let id = lookup_card(&c.name).unwrap_or(card::UNKNOWN);
            // 附魔：查 `content::ENCHANTS`。表里没有、或者有但 `modelled: false`，
            // 都进 `unknown_enchantments` 点名 —— **这两件事对内核是同一件**：
            // 这张牌会被算错，而内核知道自己在算错。
            let eb = lookup_enchant(&c.enchant_id, c.enchant_amount);
            if !c.enchant_id.is_empty() && !enchant_fully_modelled(&c.enchant_id) {
                self.report.unknown_enchantments.insert(c.enchant_id.clone());
            }
            let ix = push(&mut s, &mut names, id, c.upgraded, &c.name, eb);
            // **观测到的费用是权威。** 这张实例显示 0 费、而内容表说它要钱、
            // 又不是内核自己算得出的减费（见 `kernel_can_explain_zero_cost`）
            // —— 那就是内核看不见的某种「本回合免费」，目前已知的来源是技能药水
            // （[源码] `SetToFreeThisTurn()`，挂在**实例**上不是牌名上）。
            //
            // 这里不去猜是谁给的，只如实照抄游戏说的费用。内核没有牌生成模型，
            // 猜来源只会猜错；而费用本身是观测量，直接信它就行。
            //
            // `free_attack` 取 `self.counters`：它是**跨帧带过来的量**，
            // 观测里没有，而 `counters.load_into` 要到本函数末尾才跑。
            if let Some(cost) = c.cost {
                let def = card(id);
                let want = if c.upgraded { def.cost_upg } else { def.cost };
                // **mod 报的是 `GetAmountToSpend`，已经含全局钩子**（缠结 +N）。下面两条判的都是
                // 「这张**实例**被本地改过费」，所以先把全局那一截扣掉 —— 不扣的话缠结下每张攻击牌
                // 记成 `cost_delta = +1`，`effective_cost` 再加一遍缠结，双算。
                // `s.player` 这时已经从观测灌好了（本函数开头）。免费攻击牌在缠结下显示 1，扣完是 0，照旧盖免费。
                let cost = cost - crate::step::tangled_cost_addend(&s.player, id);
                if cost == 0 {
                    if want > 0 && !kernel_can_explain_zero_cost(def, free_attack) {
                        s.cards[ix as usize].flags |= F_FREE_THIS_TURN;
                    }
                } else if cost > want {
                    // **观测到的费用比内容表高**，内核解释不了 ⇒ 这一张实例被
                    // 永久改过费用（狂乱逃离每打一次自己 +1）。记进 `cost_delta`。
                    //
                    // 这是"观测到的费用是权威"那条规矩的**另一半**。只写 0 费那一半
                    // 是不够的：`sync` 每帧从观测重建手牌，内核自己 `GrowThisCardCost`
                    // 记下的增量当帧就被冲掉了，于是 2 费的狂乱逃离又被当成 1 费 ——
                    // 内核会给出游戏不接受的线，和无情猛攻那个 bug 同一个形状。
                    s.cards[ix as usize].cost_delta = (cost - want).clamp(-100, 100) as i8;
                }
            }
            // 痛殴攒下来的伤害加值只有卡面文本知道，见 `observed_thrash_bonus`。
            if id == card::THRASH {
                if let Some(b) = observed_thrash_bonus(
                    &c.description,
                    c.upgraded,
                    &s.player,
                    pen_nib_preview_double,
                ) {
                    s.cards[ix as usize].bonus = b;
                }
            }
            s.hand[s.n_hand as usize] = ix;
            s.n_hand += 1;
        }
        // 升级态从**名字后缀**取。牌堆里的牌没有 `is_upgraded`（约束 3），
        // 但升级牌的名字本身就带 `+`（`lookup_card` 也是靠 trim 掉它找的基础牌）。
        //
        // 原来这里一律传 `false`，于是弃牌堆里的 `拆卸+` 被当成 `拆卸`。
        // 一步对拍看不出来（那边只比名字多重集），但**从弃牌堆取牌的牌
        //（头槌 / 好勇斗狠）会取回一张没升级的**，rollout 更是整堆都矮一截。
        // 牌堆里那一张的附魔加值。**牌堆的附魔和手牌的是同一件事** ——
        // 一张带灵巧的耸肩无视不会因为躺在牌堆里就变回 8 点格挡。
        // 拿不到（老 trace / 老 mod）就是 0，那时它被系统性低估。
        //
        // 不认识的附魔照旧当没附魔算（`lookup_enchant` 返回 `(0, 0)`），
        // 但**牌堆这边不进 `unknown_enchantments`**：那一栏是给"该补哪条映射"
        // 看的，手牌那边每帧都会报同一个 id，重复计数没有信息。
        let pile_bonus = |ench: &[(String, i32)], i: usize| -> (u8, i32) {
            ench.get(i).map_or((0, 0), |(id, amt)| lookup_enchant(id, *amt))
        };
        for (i, name) in obs.discard.iter().enumerate() {
            if s.n_cards as usize >= MAX_CARDS {
                break;
            }
            let id = lookup_card(name).unwrap_or(card::UNKNOWN);
            let eb = pile_bonus(&obs.discard_enchant, i);
            let ix = push(&mut s, &mut names, id, name.ends_with('+'), name, eb);
            s.disc[s.n_disc as usize] = ix;
            s.n_disc += 1;
        }
        for (i, name) in obs.exhaust.iter().enumerate() {
            if s.n_cards as usize >= MAX_CARDS {
                break;
            }
            let id = lookup_card(name).unwrap_or(card::UNKNOWN);
            let eb = pile_bonus(&obs.exhaust_enchant, i);
            let ix = push(&mut s, &mut names, id, name.ends_with('+'), name, eb);
            s.exh[s.n_exh as usize] = ix;
            s.n_exh += 1;
        }
        // 抽牌堆。**内容可知、顺序不可知**（约束 2：mod 输出前排过序）。
        //
        // 有内容就填真牌再洗一次 —— 那正是标准的 determinization：
        // 观测确定了牌的**多重集**，顺序是从所有可能顺序里采的一个样本。
        // 照观测给的顺序直接用是**错**的，那是按稀有度排的，
        // 会让"每次都先抽到防御+"变成系统性偏差。
        //
        // 没内容（老 trace）就退回占位牌 —— 占位牌 `playable() == false`，
        // rollout 里的"我"于是几乎不出手，**进攻被系统性低估**。
        // 这正是 rollout 一直不可用的第二个原因。
        if !obs.draw_order.is_empty() {
            // **真实牌序拿得到**（mod 的 `draw_pile_order`，本地补丁）。
            // 这条路一个随机数都不掷：整堆逐字照抄，`n_draw_known` 拉满。
            //
            // **反向填**：观测下标 0 是牌堆顶，而内核 `draw[]` 顶在末尾
            //（`pop_draw_top` 从末尾取）。写正了会让"下一张抽什么"整个反过来，
            // 而且不会报错 —— 只会让 planner 一直信一个反着的前缀。
            // `sync_puts_the_real_draw_order_top_last` 钉着这条。
            //
            // 附魔跟着**原下标**走（`draw_order_enchant` 是和 `draw_order`
            // 逐位置配对的），所以反向遍历时要把下标换算回去，
            // 不能拿 `rev()` 的计数当下标。
            let n_order = obs.draw_order.len();
            for (k, name) in obs.draw_order.iter().rev().enumerate() {
                if s.n_cards as usize >= MAX_CARDS {
                    break;
                }
                let id = lookup_card(name).unwrap_or(card::UNKNOWN);
                let eb = pile_bonus(&obs.draw_order_enchant, n_order - 1 - k);
                let ix = push(&mut s, &mut names, id, name.ends_with('+'), name, eb);
                s.draw[s.n_draw as usize] = ix;
                s.n_draw += 1;
            }
            s.n_draw_known = s.n_draw;
        } else if obs.draw.is_empty() {
            for _ in 0..obs.draw_count {
                if s.n_cards as usize >= MAX_CARDS {
                    break;
                }
                let ix = push(&mut s, &mut names, card::UNKNOWN, false, DRAW_PLACEHOLDER, (0, 0));
                s.draw[s.n_draw as usize] = ix;
                s.n_draw += 1;
            }
        } else {
            for name in &obs.draw {
                if s.n_cards as usize >= MAX_CARDS {
                    break;
                }
                let id = lookup_card(name).unwrap_or(card::UNKNOWN);
                let ix = push(&mut s, &mut names, id, name.ends_with('+'), name, (0, 0));
                s.draw[s.n_draw as usize] = ix;
                s.n_draw += 1;
            }
            let n = s.n_draw as usize;
            for i in (1..n).rev() {
                let j = crate::state::next_below(&mut s.rng.shuffle, i + 1);
                s.draw.swap(i, j);
            }
        }

        // 遗物给的**私有** status，只取 `private_status` 那一栏。
        //
        // **`start_status` 那一栏绝不能进这里。** 那栏装的是游戏也会报的量
        // （力量/覆甲/荆棘），观测里本来就有；放进 carry 就会在下面那个
        // `set` 里把观测覆盖掉 —— 加一件金刚杵就能把力量永远钉在 1，
        // 药水和撕裂加的力量全部丢失。这两栏的处置正好相反，见 `RelicDef`。
        //
        // 只在**第一次**同步时初始化，之后跟着 `relic_carry` 走 ——
        // 因为「臂甲这场用过没有」游戏根本不报，每帧重新初始化的话，
        // 内核会以为每一帧都还能翻倍，整场战斗的格挡全部算高。
        //
        // **从战斗中途接进来时（`round > 1`）只跳过一场用一次的那些。**
        //
        // 原来这里是整块跳过，理由写着"前面很可能已经打过格挡牌了，宁可当作
        // 已经用掉"。**那个理由只管得住臂甲那一类**，而它顺手把「我身上有这件
        // 遗物」这种常数标记也一起扔了 —— 于是 `solve --live` 在第 2 回合之后
        // **一件遗物都看不见**（燃烧之血、锚、红面具、招架盾、钢笔尖……全没有），
        // 而实战驱动恰恰全是中途调用。2026-09-06 拆开：
        //
        // * 一场只用一次的（`content::spent_once_per_combat`，判据是它自己的
        //   规则会不会 `ClearSelf`）—— 中途接入时**当成已经用掉**，照旧低估自己
        // * 其余私有量 —— 常数，任何时候恢复都对
        // * `counter_to` —— **观测量**（游戏把它显示在遗物上），任何时候都对
        //
        // 对拍语料一个字节都没变：trace 的第一帧本来就是 `round == 1`。
        if !self.relic_init {
            self.relic_init = true;
            let mid_fight = obs.round > 1;
            for r in &obs.relics {
                if let Some(def) = crate::content::relic_by_id(&r.id) {
                    for (st, amt) in def.private_status {
                        if mid_fight && crate::content::spent_once_per_combat(*st) {
                            continue;
                        }
                        push_carry(&mut self.relic_carry, *st, *amt);
                    }
                    // 跨战斗保留的计数器：从观测灌，**不假设从 0 开始**。
                    // 观测里没有（老 trace / 游戏没给）就退回 0 并且不声张。
                    //
                    // **回合相位要把这一场已经走过的回合减回去。**
                    // [源码] 摆动球是 `AfterPlayerTurnStart` 里
                    // `TurnsSeen = (TurnsSeen + 1) % 3`，所以观测到的那个数
                    // 是**加过 `round` 次之后**的；而 `TCond::EveryNTurns`
                    // 算的是 `phase + turn`，要的是战斗开始那一刻的相位。
                    // 不减的话整条相位**早一个回合**：
                    // [实测] 2026-09-06 `act3_f46_elite_soul_nexus` 观测到的计数器
                    // 逐回合是 0,1,2,0,1,2,0（第 1/4/7 回合抽牌），
                    // 而内核抽在第 3/6 回合 —— 三处「回合开始手牌张数」的软差异
                    // 里有两处是这么来的。**佩尔之血补上之后它才露出来**：
                    // 在那之前内核每回合少一张，两个错互相盖住了。
                    //
                    // 钢笔尖那种和回合数无关的计数器**原样灌**，判据走
                    // `content::is_turn_phase`（数据，不是名单）。
                    if let Some(st) = def.counter_to {
                        let raw = r.counter.unwrap_or(0);
                        let v = if crate::content::is_turn_phase(st) { raw - obs.round } else { raw };
                        push_carry(&mut self.relic_carry, st, v);
                    }
                }
            }
        }
        for (st, v) in &self.relic_carry {
            s.player.set(*st, *v);
        }
        // Observed Strength already includes RedSkull. Restore its latch only.
        s.player.set(St::RedSkullActive, i32::from(
            s.player.get(St::RedSkull) > 0 && crate::step::hp_threshold_active(&s.player)));
        // **开局赠予在第 0 帧之前就已经发生过了**（金刚杵的力量、护喉甲的覆甲…），
        // 观测里那份力量就是它发生过的证据。**修饰器要跟着记成用过一次** ——
        // 不消耗的话内核会以为这一场还能再翻一次倍（损毁头盔），那是**高估自己**。
        //
        // 走 `step` 那一个实现，判据不另抄一份；**返回值扔掉** ——
        // 力量本身是观测量，上面已经从观测灌过了，这里只为了让修饰器掉一层。
        // 幂等：第二次跑的时候标记已经是 0，什么都不会发生。
        // [实测] 2026-09-09 `act3_f48_boss_aeonglass_2026-09-06` 第 0 帧
        // 力量 2 = 金刚杵 1 × 损毁头盔 —— 头盔在那一刻就已经用掉了。
        for r in &obs.relics {
            if let Some(def) = crate::content::relic_by_id(&r.id) {
                for (st, amt) in def.start_status {
                    let _ = crate::step::modify_status_amount_received(&mut s, true, *st, *amt);
                }
            }
        }

        // 观测里没有的每回合计数器：回合一换就清零，否则沿用上一帧跑完的值。
        // 放在最后，免得被前面任何一句覆盖掉。
        if obs.round != self.last_round {
            self.counters = TurnCounters::default();
            self.last_round = obs.round;
        }
        self.counters.load_into(&mut s);
        // 见上面 `free_attack` 那一段：观测赢过携带值。
        if let Some(n) = observed_free_attack(&obs.status) {
            s.free_attack = n;
            self.counters.free_attack = n;
        }
        // 扯碎的段数同理：卡面上那个「命中 N 次」是游戏算好的，比内核自己
        // 从同步那一刻起数的靠谱。读不出来（牌不在手上 / 换了语言）就用携带值，
        // 那是个**下界** —— 方向是低估，和本仓库其它拿不到数时的处置一致。
        s.hp_loss_hits = observed_tear_hits(obs).unwrap_or(self.hp_loss_hits);
        self.hp_loss_hits = s.hp_loss_hits;

        // 跨帧携带的 Pending：观测开着选牌界面（obs.pending）且内核之前走进了 Pending 时沿用；
        // 观测已经关闭选牌界面时清零。
        if obs.pending && self.pending != Pending::None {
            s.pending = self.pending;
        } else if !obs.pending {
            self.pending = Pending::None;
        }

        unmapped.sort();
        unmapped.dedup();
        Synced { state: s, names, unmapped }
    }
}

/// 一只敌人被"认出来"的结果。
/// 反推**我面朝哪一侧**（游戏不报朝向，但报意图标签，而标签含朝向那个 ×1.5）。
///
/// 做法：两个假设各跑一遍对齐，数一数有几只**逐字对上**，多的那个胜出；
/// 打平就保持原样（多半是没有包围、朝向根本不影响任何标签）。
///
/// 为什么不"自己记着"：读档、中途接入、以及我这边任何一次漏记都会让朝向长歪，
/// 而它错了之后**所有**减伤判断跟着错。从观测反推是自愈的 —— 每帧重新推一次。
///
/// 不带 `Surrounded` 时直接返回：那种战斗里朝向不进任何乘区，推它没有意义。
fn infer_facing(r: &Replayer, s: &mut State, obs: &Obs) {
    if s.player.get(St::Surrounded) <= 0 {
        return;
    }
    let mut best = (-1i32, s.player.get(St::FacingRight));
    for hypo in [0, 1] {
        let mut player = Entity::new(s.player.max_hp.max(1));
        player.set(St::Surrounded, 1);
        player.set(St::FacingRight, hypo);
        let mut exact = 0i32;
        for e in &obs.enemies {
            if r.slot_of_existing(&e.combat_id).is_none() {
                continue;
            }
            let Some(def_id) = enemy_id(&e.name) else { continue };
            let n_moves = crate::content::enemy_def(def_id).moves.len();
            let mut ent =
                Entity { hp: e.hp, max_hp: e.max_hp, block: e.block, status: [0; N_STATUS] };
            for (id, amt) in &e.status {
                if let Some(st) = map_status(id) {
                    ent.set(st, status_value(id, st, *amt));
                }
            }
            let want = observed_signature(e);
            sync_attack_counters(e, &mut ent);
            if (0..n_moves).any(|m| move_signature(def_id, m, &ent, &player, s.ascension) == want)
            {
                exact += 1;
            }
        }
        if exact > best.0 {
            best = (exact, hypo);
        }
    }
    s.player.set(St::FacingRight, best.1);
}

#[derive(Clone, Debug)]
pub struct Identified {
    pub slot: usize,
    pub name: String,
    /// 内容表里的 `EnemyDef` 下标。`None` = 表里没这只敌人
    pub def: Option<u16>,
    /// 出招指针有没有靠观测到的意图对齐上。没对齐就**不能**拿它推演下一手
    pub aligned: bool,
    /// 对齐到的**招式下标**（`EnemyDef::moves` 的下标）。
    ///
    /// 有了它，实战那条路就能拿到这一手的**面板基础值** ——
    /// `Threat` 才装得进"可现算"的口径（见 `solver::Threat::set_live`）。
    pub move_ix: Option<usize>,
}

impl Identified {
    /// 能不能拿它做跨回合推演
    pub fn usable(&self) -> bool {
        self.def.is_some() && self.aligned
    }
}

impl Replayer {
    /// 把同步出来的状态里的敌人**认出来**：按名字查 `EnemyDef`，
    /// 再用观测到的意图把出招指针对齐（和 `verify_enemy_ai` 同一套对齐逻辑）。
    ///
    /// **对拍绝不能这么做。** `sync` 一律安 `enemy::UNKNOWN` 是 trace-format
    /// 约束 4 定下来的：内核不预测敌人，敌人打了多少由观测注入。让内核自己
    /// 出招会把对拍变成"内核和内核比"。
    ///
    /// 但**跨回合推演必须知道对面是谁**：不认识时 `UNKNOWN` 的唯一一手是
    /// `EOp::Nothing`，推演出来的是一场敌人永远不出手的仗 —— 那比不推演危险得多
    /// （实测过：12 血打 200 血的敌人，rollout 认为最终 HP = 12）。
    ///
    /// 所以认敌人这件事**只给求解器用**，而且认不出来要**说出来**，
    /// 由调用方决定拒绝还是降级。
    pub fn identify_enemies(&self, s: &mut State, obs: &Obs) -> Vec<Identified> {
        // **先把朝向反推出来。** 游戏不把朝向报成 status，但它**报意图标签**，
        // 而标签里含着朝向那个 ×1.5 —— 所以两个假设各对齐一遍，
        // 哪个假设下"逐字对上"的敌人多，就是哪个。见 `infer_facing`。
        infer_facing(self, s, obs);
        let mut out = Vec::new();
        // **玩家实体要带上朝向**：`move_signature` 走的是完整的 `apply_modifiers`，
        // 而朝向乘区是从防御方（我）身上读的。空实体会让带包围的那场全部对不齐。
        let mut player = Entity::new(s.player.max_hp.max(1));
        player.set(St::Surrounded, s.player.get(St::Surrounded));
        player.set(St::FacingRight, s.player.get(St::FacingRight));
        for e in &obs.enemies {
            let Some(&slot) = self.slots.get(&e.combat_id) else { continue };
            let mut row =
                Identified { slot, name: e.name.clone(), def: None, aligned: false, move_ix: None };
            let Some(def_id) = enemy_id(&e.name) else {
                out.push(row);
                continue;
            };
            row.def = Some(def_id);
            let n_moves = crate::content::enemy_def(def_id).moves.len();
            if n_moves == 0 {
                out.push(row);
                continue;
            }
            // 用观测里的 status 造一个和当下一致的敌人实体，好把力量算进意图标签
            let mut ent =
                Entity { hp: e.hp, max_hp: e.max_hp, block: e.block, status: [0; N_STATUS] };
            for (id, amt) in &e.status {
                if let Some(st) = map_status(id) {
                    ent.set(st, status_value(id, st, *amt));
                }
            }
            let want = observed_signature(e);
            // 先要求逐字相等，退而求其次只对意图类型（玩家带易伤时数字会差）。
            //
            // **逐字相等的里面，先挑当前局面下机器走得到的那一手。** 两手签名可以逐字相同
            // （蜂群术士的喷射信息素按蜂房层数拆成两手，都是 `Buff`），而 `find` 取第一个 ——
            // 蜂房 ≥ 3 时就会对到「蜂房 +1、力量 +1」那一支，之后的推演每次喷射少 1 点力量。
            // 判据见 `step::move_reachable_now`：只排掉「进它的条件边在当前局面下确定不成立」的。
            // 私有计数器（瀑布巨兽高压枪涨过多少 / 爆炸记下多少）先从**这一手**的标签还原，再比签名。
            // 不先还原的话高压枪 25 逐字对不上任何一手，按类型退回会对到同样是 `Attack + Buff` 的撞击上。
            sync_attack_counters(e, &mut ent);
            s.enemies[slot].set(St::ClawGrowth, ent.get(St::ClawGrowth));
            let with_private = |m: usize| {
                let mut e2 = ent;
                if let Some((st, v)) =
                    infer_private_attack_counter(def_id, m, &ent, &player, s.ascension, &want)
                {
                    e2.set(st, v);
                }
                e2
            };
            let exact = |m: usize| {
                let e2 = with_private(m);
                crate::content::move_matches_form(def_id, m, &e2)
                    && move_signature(def_id, m, &e2, &player, s.ascension) == want
            };
            let m = (0..n_moves)
                .find(|&m| exact(m) && crate::step::move_reachable_now(s, slot, def_id, m))
                .or_else(|| (0..n_moves).find(|&m| exact(m)))
                .or_else(|| {
                    (0..n_moves).find(|&m| {
                        crate::content::move_matches_form(def_id, m, &ent)
                            && same_kinds(&move_signature(def_id, m, &ent, &player, s.ascension), &want)
                    })
                });
            if let Some(m) = m {
                s.enemy_def[slot] = def_id;
                if let Some((st, v)) =
                    infer_private_attack_counter(def_id, m, &ent, &player, s.ascension, &want)
                {
                    s.enemies[slot].set(st, v);
                }
                // `enemy_move` 是"下一次出第几手"的计数器，观测到的意图就是
                // **这一手**，所以指针直接指向它
                s.enemy_move[slot] = m as u8;
                row.aligned = true;
                row.move_ix = Some(m);
            }
            out.push(row);
        }
        // **欠一次接续的尸体不在观测里**，上面那个循环碰不到它。按它消失前的名字把
        // `EnemyDef` 认回来 —— 不然推演里它复活之后是一只永远不出手的 UNKNOWN。
        // 出招指针不用对齐：复活那一刻规则自己把它强制到「重接」（`St::ReattachDue`）。
        // 不进 `out`：这张表报的是「观测里的敌人认出了几只」。
        for slot in 0..s.n_enemies as usize {
            if s.enemies[slot].alive() || (s.enemies[slot].get(St::ReattachDue) <= 0
                && s.enemies[slot].get(St::Adaptable) <= 0) {
                continue;
            }
            if let Some(def_id) = enemy_id(&self.seen[slot].name) {
                s.enemy_def[slot] = def_id;
            }
        }
        out
    }
}

/// 把一条**正在进行中**的 trace 跑到最后一帧，返回那一帧同步出来的状态。
///
/// 存在的唯一理由是**观测里没有的每回合计数器**：
/// `attacks_played`（踩踏减费）/ `hp_lost_this_turn`（怨恨双击）/
/// `exhausted_this_turn`（邪眼、被遗忘的仪式）/ `free_attack`（无情猛攻）。
/// 单看一帧观测推不出它们，只能**从这一回合的开头一路走过来**。
///
/// 实战里踩过：`--live` 单帧求解时 `hp_lost_this_turn` 恒为 0，而绯红披风
/// 在回合开始扣的那 1 点血正好让怨恨攻击两次 —— 内核少算一次
/// （2026-08-17 第2幕31层，两个回合各差 7 点和 10 点）。
///
/// **不 diff、不判决**，只维护状态：这跟 [`verify`] 是两个用途，那边要逐帧比，
/// 这边只要"走到现在"。计数器怎么跨帧带的规则和 `verify` 必须一致，
/// `sync_latest_carries_the_same_counters_as_verify` 那条测试守着这一点。
///
/// 返回 `(replayer, 最后一帧的同步状态, 真正用上的历史帧数)`。
/// 历史帧数为 0 表示只有一帧、没有历史可带 —— 调用方应当把这件事说出来。
pub fn sync_latest(t: &Trace) -> Option<(Replayer, Synced, usize)> {
    let last = t.frames.len().checked_sub(1)?;
    let mut r = Replayer::for_trace(t);
    let mut used = 0usize;
    for i in 0..last {
        if r.advance(&t.frames[i]) {
            used += 1;
        }
    }
    let sy = r.sync(&t.frames[last].obs);
    Some((r, sy, used))
}

impl Replayer {
    /// 把一帧的动作跑一遍，**只维护跨帧要带的量**（每回合计数器 + 遗物 status），
    /// 不 diff、不判决。返回这一帧算不算"走过了"。
    ///
    /// 抽出来是因为有三个地方要走这条路：`sync_latest`（实战）、
    /// `verify_per_turn`（整回合对拍段与段之间），还有将来的跨回合 rollout。
    /// 各写一遍的话，「计数器怎么带」就有三份实现，迟早长歪 ——
    /// 臂甲已经因为漏了一处（`verify_per_turn`）而在段首复活过一次。
    pub fn advance(&mut self, f: &Frame) -> bool {
        let Some(act) = &f.action else { return false };
        if combat_over_obs(&f.obs) {
            return false;
        }
        let sy = self.sync(&f.obs);
        match act {
            // 反推不出来的一步**重放不了**。返回 false 和 2026-08-25 之前
            // （那时它落成 `action = None`）逐字相同，行为没变。
            Act::Unknown => false,
            Act::Play { card_name, target, .. } => {
                let Some(h) = (0..sy.state.n_hand as usize).find(|&h| {
                    sy.names.get(sy.state.hand[h] as usize).map(|s| s.as_str())
                        == Some(card_name.as_str())
                }) else {
                    return false;
                };
                let tgt = target
                    .as_ref()
                    .and_then(|t| f.obs.enemies.iter().find(|e| &e.entity_id == t))
                    .and_then(|e| self.slots.get(&e.combat_id).copied())
                    .unwrap_or(0);
                let st = step(sy.state, Action::PlayCard { hand: h as u8, target: tgt as u8 });
                if st == sy.state {
                    return false;
                }
                self.save_carry(&st);
                true
            }
            Act::UsePotion { slot, target, .. } => {
                let tgt = target
                    .as_ref()
                    .and_then(|t| f.obs.enemies.iter().find(|e| &e.entity_id == t))
                    .and_then(|e| self.slots.get(&e.combat_id).copied())
                    .unwrap_or(0);
                let st =
                    step(sy.state, Action::UsePotion { slot: *slot as u8, target: tgt as u8 });
                if st == sy.state {
                    return false;
                }
                self.save_carry(&st);
                true
            }
            Act::EndTurn => {
                let st = sy.state;
                let end = match observed_incoming(&f.obs, &self.slots) {
                    Some(inc) => crate::step::end_turn_with_incoming(st, &inc),
                    None => step(st, Action::EndTurn),
                };
                self.save_carry(&end);
                // `end_turn` 已经跑进下一个回合，这批计数器**属于新回合**，
                // 要认领而不是让 `sync` 清零。
                self.last_round = end.turn;
                true
            }
            Act::SelectCard { slot } => {
                let st = sy.state;
                if st.pending == Pending::None {
                    return false;
                }
                let next_st = step(st, Action::Choose { hand: *slot as u8 });
                if next_st == st {
                    return false;
                }
                self.save_carry(&next_st);
                true
            }
            Act::Confirm => {
                let mut st = sy.state;
                if st.pending != Pending::None {
                    st.pending = Pending::None;
                    self.save_carry(&st);
                }
                true
            }
        }
    }
}

fn multiset(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

/// 无情猛攻挂的「下一张攻击牌 0 费」在观测里的名字。
///
/// 它是 `TurnCounters` 那批量里**唯一一个观测得到的** —— 其余四个
/// （打出张数 / 攻击张数 / 本回合消耗数 / 本回合失血）游戏都不报，
/// 只能跨帧携带。能观测就以观测为准，这是仓库第一条规矩。
pub const FREE_ATTACK_STATUS: &str = "FREE_ATTACK_POWER";

/// 观测里有、`map_status` 里**故意没有**、但另有消费者的 status。
///
/// 目前只有一个：无情猛攻的 `FREE_ATTACK_POWER` 存在 `State::free_attack`
/// 这个整数字段上，不是一个 `St`（做成 `St` 就等于同一件事存两份）。
/// 它由 `sync` 读进来、由 `diff_play` 逐帧验。
pub const STATUS_HANDLED_OUTSIDE_MAP: &[&str] = &[FREE_ATTACK_STATUS];

pub fn observed_free_attack(status: &BTreeMap<String, i32>) -> Option<i32> {
    status.get(FREE_ATTACK_STATUS).copied()
}

/// 观测到的某个 status 的层数（id 走 `map_status`，英文 id 和中文名都认；没有就是 0）。
/// 和 `diff_statuses` 同一个口径。
pub fn observed_player_status(status: &BTreeMap<String, i32>, st: St) -> i32 {
    status.iter().filter(|(id, _)| map_status(id) == Some(st)).map(|(_, v)| *v).sum()
}

/// 观测里这张牌显示 0 费时，这个 0 **内核自己算得出来吗**。
///
/// 两个调用点问的是同一件事，所以它是一个函数（这个仓库已经在"同一件事写了
/// 两份"上栽过三次：`cards_locked` 的谓词、`current_card_name` 的名字、
/// 还有这一条）：
///
/// * `sync`：算不出来才给这张**实例**盖 `F_FREE_THIS_TURN`
///   —— 那代表"内核看不见的某种本回合免费"，已知来源是技能药水
///   （[源码] `SetToFreeThisTurn()`，挂在实例上不是牌名上）
/// * 卡面费用软 diff：算得出来就不报，否则全是噪音
///
/// 内核算得出来的减费有两种：**踩踏**（费用随本回合打过的攻击牌下降）
/// 和**无情猛攻**（下一张攻击牌 0 费）。
///
/// > **2026-08-22 实战抓到的真错**：这个判断原来只在软 diff 那一处写全了，
/// > `sync` 那一处漏了无情猛攻。而游戏在 `FREE_ATTACK_POWER` 生效时把手里
/// > **每一张**攻击牌都渲染成 0 费 —— 于是 `sync` 给它们**全部**盖上免费标记，
/// > 内核以为这回合能白打任意多张攻击牌。第2幕第20层求解器据此给出
/// > 「闪电霹雳+ 然后 御血术+」，而游戏只认第一张。
/// > 注释里当时写着"又不是踩踏/无情猛攻那种"，**代码里无情猛攻那一半从没写过**。
pub fn kernel_can_explain_zero_cost(def: &CardDef, free_attack: i32) -> bool {
    def.cost_minus_attacks || (matches!(def.kind, Kind::Attack) && free_attack > 0)
}

/// 牌区里每张牌**当前**该显示成什么名字。
///
/// `names` 是**同步那一刻**的名字快照，而卡实例的升级位是会被规则原地改的
/// （武装 / 武装+）。直接用快照就等于假设"牌名一整帧不变" ——
/// 2026-08-22 第14层实录踩到了：游戏把手里两张防御升成防御+，内核也升了，
/// 但 diff 拿内核的**旧名字**去比，报出一个假 MISMATCH，
/// 而内核那条规则本身有测试守着、是对的。
///
/// 所以这里按**当前**的升级位重新拼名字：去掉快照名末尾的 `+`，
/// 再按 `F_UPGRADED` 补回去。`+` 后缀就是 `sync` 那边读升级位的依据
///（`push(.., name.ends_with('+'), ..)`），两边用同一条约定。
/// **一张牌当前该叫什么。** 三个地方要问它，所以它是一个函数：
/// 牌区 diff（[`zone_names`]）、L2 验收里按名字找手牌位置、
/// 整回合模式里同样的查找。
///
/// `names` 是**同步那一刻**的快照，而升级位会被规则原地改（武装 / 武装+）。
/// 拿快照当真名有两个后果，两个都真踩过：diff 报假 MISMATCH（已修），
/// 以及**实战线重放不出来**（回合被静默跳过，验收在悄悄丢覆盖）。
///
/// `+` 后缀就是 `sync` 那边读升级位的依据，两边用同一条约定。
pub fn current_card_name(names: &[String], cards: &[CardInst], ix: usize) -> String {
    // **快照只覆盖同步那一刻存在的牌。** 内核自己生成的牌（添柴 / 地狱之刃 /
    // 破灭那类）是 `step` 在这一帧里新 push 进 `s.cards` 的，下标必然越过
    // `names` 的末尾 —— 2026-08-22 第2幕第20层第一次打出添柴+，验证器当场
    // panic「index out of bounds: the len is 19 but the index is 19」。
    //
    // 这类牌的身份**只能**来自内容表：观测里没有它们（生成发生在这一帧内），
    // 而内核是自己造的，知道 id。所以越界不是要兜底的异常，是另一个合法来源。
    // **身份被就地改写过的牌，快照就不再是它了。**
    //
    // `Op::TransformAttacksInHand`（原始力量）是内核里唯一一个**改 `CardInst.id`**
    // 的 op —— 扯碎/暴走/痛殴改的都是 `bonus`，所以它们碰不到这里。
    // 判据是「快照那个名字当初同步成了哪个 id」和「现在是哪个 id」：
    // 不一样就说明这一帧被改写过，得改用内容表的名字。
    //
    // 这一条**必须收在名字层，不能靠 `names` 跟着改**：`names` 是按 `s.cards`
    // 下标的快照，`step` 不该知道验证器的存在（L1 不认识对拍）。
    //
    // 2026-09-06 第3幕第46层第一次打出原始力量+ 抓到的：游戏 3 张巨石+，
    // 内核报的还是 `完美打击+,彼岸咆哮+,灰烬打击+` —— **假红**，
    // 内核的 `id` 其实已经是巨石了。
    let rewritten = names
        .get(ix)
        .map(|n| lookup_card(n.trim_end_matches('+')).unwrap_or(card::UNKNOWN) != cards[ix].id)
        .unwrap_or(false);
    let base = match names.get(ix).filter(|_| !rewritten) {
        // 同步进来的牌：名字快照里已经带着假升级后缀了（`凋萎+1`），
        // 而 `trim_end_matches('+')` 只剥光秃秃的那个 `+`，动不到 `+1`。
        Some(n) => n.trim_end_matches('+').to_string(),
        // 内核自己生成的牌：只能从内容表拿名字 —— **外加把假升级层数补回去**。
        // 游戏把层数写进标题（[源码] `Wither.Title`），不补的话内核刚塞进手牌的
        // 那张凋萎会显示成 `凋萎`，和游戏的 `凋萎+1` 对不上，
        // 报出来是一条"生成的牌不一样"的软差异 —— 而实际上 `bonus` 是对的，
        // 差的只是这里没渲染。层数从 `bonus` 反推，和 `sync` 那边是同一个常数。
        None => {
            let d = crate::content::card(cards[ix].id);
            let lv = cards[ix].bonus / FAKE_UPGRADE_STEP;
            if cards[ix].id == card::WITHER && lv > 0 {
                format!("{}+{}", d.name, lv)
            } else {
                d.name.to_string()
            }
        }
    };
    if cards[ix].flags & crate::state::F_UPGRADED != 0 {
        format!("{base}+")
    } else {
        base
    }
}

fn zone_names(zone: &[u8], n: u8, names: &[String], cards: &[CardInst]) -> Vec<String> {
    (0..n as usize).map(|i| current_card_name(names, cards, zone[i] as usize)).collect()
}

// --------------------------------------------------------------------------
// diff
// --------------------------------------------------------------------------

/// `enemy_may_add`：这一帧敌人有机会施加新状态（`end_turn`）。
///
/// 此时"内核比观测少"和"内核比观测多"是两件完全不同的事：
///   * 内核少了 ⇒ 敌人加了内核预测不到的 debuff，属于敌人 AI，不是算错（软）
///   * 内核多了 ⇒ 该衰减的没衰减，是真 bug（硬）
///
/// 用绝对值比较，因为 `缩小` 存的是负的显示值（-1）。
fn diff_statuses(
    what: &str,
    e: &Entity,
    obs: &BTreeMap<String, i32>,
    enemy_may_add: bool,
    out: &mut Vec<Diff>,
) {
    for st in ALL_ST {
        let game: i32 = obs
            .iter()
            .filter(|(id, _)| map_status(id) == Some(st))
            .map(|(_, v)| *v)
            .sum();
        let kernel = e.get(st);
        if game == kernel {
            continue;
        }
        let field = format!("{what}.{}", st_name(st));
        if enemy_may_add && kernel.abs() < game.abs() {
            out.push(Diff::soft(format!("{field}(敌方施加，内核不预测)"), game, kernel));
        } else {
            out.push(Diff::hard(field, game, kernel));
        }
    }
}

fn diff_play(
    sy: &Synced,
    next: &Obs,
    slots: &BTreeMap<String, usize>,
    drew: bool,
    random_exhaust: bool,
) -> Vec<Diff> {
    let s = &sy.state;
    let mut d = Vec::new();

    if next.hp != s.player.hp {
        d.push(Diff::hard("我方.HP", next.hp, s.player.hp));
    }
    if next.block != s.player.block {
        d.push(Diff::hard("我方.格挡", next.block, s.player.block));
    }
    diff_statuses("我方", &s.player, &next.status, false, &mut d);

    if next.energy != s.energy {
        d.push(Diff::hard("能量", next.energy, s.energy));
    }
    // 无情猛攻的「下一张攻击牌 0 费」。它不是一个 `St`（内核存在
    // `State::free_attack` 这个字段上），所以 `diff_statuses` 检不到它 ——
    // 在此之前它只是在报表里以「没映射的 status」露一面，**没有任何东西验它**。
    // 而这一帧比的正是内核有没有把它正确地给出去/消耗掉。
    if let Some(game_fa) = observed_free_attack(&next.status) {
        if game_fa != s.free_attack {
            d.push(Diff::hard("我方.下张攻击免费", game_fa, s.free_attack));
        }
    } else if s.free_attack != 0 {
        d.push(Diff::hard("我方.下张攻击免费", 0, s.free_attack));
    }

    // ---- 召唤：这一帧新冒出来的敌人 ----
    //
    // `slots` 是**这一帧开头**建的映射，里面没有召唤物 —— 它们带全新的
    // `combat_id`。而内核那边是自己 `summon_one` 出来的新槽位。两边都是
    // "多出来的"，按**出场顺序**配对；游戏不报槽位号，这是唯一能拿到的对应关系。
    //
    // 配对之后血量判**软**：召唤物的血是游戏现掷的（扭动虫实测 17/19/20/21），
    // 内核表里只能填一个数，这条结构性不可对拍，和抽牌堆顺序是同一类。
    // **但"召唤了几只"是硬的** —— 那才是这条规则真正要验的东西，
    // 名字对不上同样是硬的（召错了种类）。
    let mut slot_for: BTreeMap<String, usize> = slots.clone();
    let mut summoned: BTreeSet<usize> = BTreeSet::new();
    {
        let claimed: BTreeSet<usize> =
            next.enemies.iter().filter_map(|e| slots.get(&e.combat_id).copied()).collect();
        let newcomers: Vec<usize> = (0..next.enemies.len())
            .filter(|&i| !slots.contains_key(&next.enemies[i].combat_id))
            .collect();
        let fresh: Vec<usize> = (0..s.n_enemies as usize)
            .filter(|i| !claimed.contains(i) && s.enemies[*i].alive())
            .collect();
        if newcomers.len() != fresh.len() {
            d.push(Diff::hard("本帧新出现的敌人数", newcomers.len(), fresh.len()));
        }
        for (k, &oi) in newcomers.iter().enumerate() {
            if let Some(&slot) = fresh.get(k) {
                slot_for.insert(next.enemies[oi].combat_id.clone(), slot);
                summoned.insert(slot);
            }
        }
    }

    for e in &next.enemies {
        let Some(&slot) = slot_for.get(&e.combat_id) else {
            d.push(Diff::hard(format!("敌[{}]", e.name), "存在", "内核里没有这个槽位"));
            continue;
        };
        let ke = &s.enemies[slot];
        let fresh = summoned.contains(&slot);
        if fresh {
            let kname = crate::content::enemy_def(s.enemy_def[slot]).name;
            if kname != e.name {
                d.push(Diff::hard(format!("召唤物[槽{slot}]"), e.name.clone(), kname.to_string()));
            }
        }
        if e.hp != ke.hp {
            let f = format!("敌[{}].HP", e.name);
            if fresh {
                d.push(Diff::soft(format!("{f}(召唤物血量游戏现掷，内核不可知)"), e.hp, ke.hp));
            } else {
                d.push(Diff::hard(f, e.hp, ke.hp));
            }
        }
        if e.block != ke.block {
            d.push(Diff::hard(format!("敌[{}].格挡", e.name), e.block, ke.block));
        }
        diff_statuses(&format!("敌[{}]", e.name), ke, &e.status, false, &mut d);
    }
    // 观测里消失 = 已死。内核还活着就是漏杀或多打。
    for slot in 0..s.n_enemies as usize {
        let still_there = next.enemies.iter().any(|e| slot_for.get(&e.combat_id) == Some(&slot));
        if !still_there && s.enemies[slot].hp > 0 {
            d.push(Diff::hard(format!("敌槽{slot}"), "已死亡(从观测中消失)", s.enemies[slot].hp));
        }
    }

    // 手牌：抽过牌就只能比张数（抽到什么不可预测，约束 2）
    let kernel_hand = zone_names(&s.hand, s.n_hand, &sy.names, &s.cards);
    if next.hand.len() != kernel_hand.len() {
        d.push(Diff::hard("手牌张数", next.hand.len(), kernel_hand.len()));
    } else if random_exhaust {
        let game = multiset(next.hand.iter().map(|c| c.name.clone()).collect());
        let ker = multiset(kernel_hand);
        if game != ker {
            d.push(Diff::soft(
                "手牌(随机消耗手牌,内核随机流故意不同)",
                game.join(","),
                ker.join(","),
            ));
        }
    } else if !drew {
        // **内核这一帧自己生成的牌**（添柴 / 地狱之刃 / 惊逃那类）：它们是
        // `step` 新 push 进 `s.cards` 的，下标必然越过同步那一刻的名字快照。
        //
        // 它们的**身份结构性不可对拍**：内核的随机流故意和游戏不一致
        // （不变量 4），`GEN_POOL` 又只是真实生成池的一个子集。
        // 照召唤物那条先例切开 —— **几张、升没升级判硬，是哪几张判软**。
        // 全判硬的话这一帧永远红；全判软的话「添柴+ 该生成升级牌」就没人看着了。
        let generated: Vec<usize> = (0..s.n_hand as usize)
            .filter(|&i| s.hand[i] as usize >= sy.names.len())
            .collect();
        if generated.is_empty() {
            let game = multiset(next.hand.iter().map(|c| c.name.clone()).collect());
            let ker = multiset(kernel_hand);
            if game != ker {
                d.push(Diff::hard("手牌", game.join(","), ker.join(",")));
            }
        } else {
            // 先把「本来就在手里的那些」逐张从游戏手牌里划掉，剩下的就是
            // 游戏生成的那几张（游戏不告诉我们哪几张是生成的，只能这么反推）。
            let mut game_rest: Vec<&CardObs> = next.hand.iter().collect();
            let mut unmatched: Vec<String> = Vec::new();
            for i in 0..s.n_hand as usize {
                let ix = s.hand[i] as usize;
                if ix >= sy.names.len() {
                    continue;
                }
                let name = current_card_name(&sy.names, &s.cards, ix);
                match game_rest.iter().position(|c| c.name == name) {
                    Some(p) => {
                        game_rest.remove(p);
                    }
                    None => unmatched.push(name),
                }
            }
            if !unmatched.is_empty() {
                d.push(Diff::hard(
                    "手牌(非生成的那部分)",
                    "游戏手牌里有",
                    format!("内核有而游戏没有：{}", unmatched.join(",")),
                ));
            }
            let ker_gen: Vec<String> = generated
                .iter()
                .map(|&i| current_card_name(&sy.names, &s.cards, s.hand[i] as usize))
                .collect();
            // 硬：生成了几张（= 消耗了几张），以及其中几张是升级牌
            if game_rest.len() != ker_gen.len() {
                d.push(Diff::hard("生成的牌张数", game_rest.len(), ker_gen.len()));
            }
            let game_upg = game_rest.iter().filter(|c| c.upgraded).count();
            let ker_upg = generated
                .iter()
                .filter(|&&i| {
                    s.cards[s.hand[i] as usize].flags & crate::state::F_UPGRADED != 0
                })
                .count();
            if game_upg != ker_upg {
                d.push(Diff::hard("生成的牌里已升级的张数", game_upg, ker_upg));
            }
            // 软：是哪几张。内核的随机序列**故意**和游戏不一致，见不变量 4
            let game_gen = multiset(game_rest.iter().map(|c| c.name.clone()).collect());
            let ker_gen_ms = multiset(ker_gen);
            if game_gen != ker_gen_ms {
                d.push(Diff::soft(
                    "生成的牌(内核随机流故意不同,不可对拍)",
                    game_gen.join(","),
                    ker_gen_ms.join(","),
                ));
            }
        }
    }

    let game_disc = multiset(next.discard.clone());
    let ker_disc = multiset(zone_names(&s.disc, s.n_disc, &sy.names, &s.cards));
    if game_disc != ker_disc {
        d.push(Diff::hard("弃牌堆", game_disc.join(","), ker_disc.join(",")));
    }
    let game_exh = multiset(next.exhaust.clone());
    let ker_exh = multiset(zone_names(&s.exh, s.n_exh, &sy.names, &s.cards));
    if next.exhaust.len() != s.n_exh as usize {
        d.push(Diff::hard("消耗堆张数", next.exhaust.len(), s.n_exh));
    } else if game_exh != ker_exh {
        if random_exhaust {
            d.push(Diff::soft(
                "消耗堆(随机消耗手牌,内核随机流故意不同)",
                game_exh.join(","),
                ker_exh.join(","),
            ));
        } else {
            d.push(Diff::hard("消耗堆", game_exh.join(","), ker_exh.join(",")));
        }
    }

    // 药水槽。逐格比而不是比多重集 —— 「喝掉了正确的那一瓶」和
    // 「喝掉了另一瓶同名药水」在多重集下看不出区别，而槽位号正是
    // `Action::UsePotion` 的参数，错了就是真错。
    for slot in 0..MAX_POTIONS {
        let game = next
            .potions
            .iter()
            .find(|p| p.slot == slot)
            .map(|p| map_potion(if p.id.is_empty() { &p.name } else { &p.id }))
            .unwrap_or(potion::NONE);
        let ker = s.potions[slot];
        if game != ker {
            d.push(Diff::hard(
                format!("药水槽{slot}"),
                potion_name(game),
                potion_name(ker),
            ));
        }
    }

    d
}

/// 把观测到的敌人攻击**注入**内核的伤害管线。
///
/// 内核不预测敌人出什么（敌人 AI 在「故意没做」清单上），但意图标签把
/// 来袭伤害直接写在观测里了，所以"打多少"是观测、"打完剩多少血"才是规则。
/// 注入的是前者，检验的是后者 —— 这样 `end_turn` 帧终于能验到**防守侧**
/// 的管线：格挡逐次吸收、溢出进 HP。这半条管线此前一帧都没被验过。
///
/// 走的是 [`absorb`] 而不是自己写减法，规则仍然只有 `damage.rs` 一处。
///
/// **一个未验证的假设**：意图标签被当作最终来袭值，不再过任何乘区。
/// 敌人自身的力量确实已经含在里面（实测），但玩家身上的易伤/无实体
/// 是否也已经算进去，至今没有样本能判定 —— 六条 trace 里玩家从没带过
/// 这些状态。真遇到时这里会报 MISMATCH，那就是答案。
/// 把观测到的意图标签打包成 [`Incoming`]，交给 `end_turn_with_incoming` 去结算。
///
/// **为什么不再自己打**（2026-08-29 修）：老版本是先 `take_attack_hit` 把伤害
/// 打完、再 `step(EndTurn)`。顺序反了 —— 真实的回合结束是
/// 「我的回合结束钩子 -> 敌人行动」，而回合结束钩子里**有给格挡的**
///（奥利哈钢没格挡时 +6、覆甲每回合 +N）。伤害先落地，那些格挡就永远挡不到
/// 这一手，内核会**系统性高估自己掉多少血**。
///
/// 藏了这么久是因为语料里几乎每次结束回合手上都还有格挡，奥利哈钢根本没上膛。
/// `act2_f33_boss_crusher_final` 帧38 是全语料**唯一**一次 block=0 结束回合 ——
/// 一次就把它抓出来了（游戏 31 / 内核 25，差的正是那 6 点）。
///
/// 返回 `None` = **这一帧不能注入**：意图标签解析不出数字，或者某只敌人有
/// 一条以上的攻击意图（`Incoming` 每只敌人只装一条，硬塞会**悄悄少打一段**）。
/// 后一种情况全语料 1344 个敌人-帧样本里出现 0 次，但这里不赌它永远是 0。
fn observed_incoming(prev: &Obs, slots: &BTreeMap<String, usize>) -> Option<crate::step::Incoming> {
    if prev.enemies.iter().any(|e| e.intent_unparsed) {
        return None;
    }
    let mut inc = [(0, 0); MAX_ENEMIES];
    for e in &prev.enemies {
        if e.attacks.len() > 1 {
            return None;
        }
        let Some(&slot) = slots.get(&e.combat_id) else { continue };
        if let Some(&(base, hits)) = e.attacks.first() {
            inc[slot] = (base, hits);
        }
    }
    Some(inc)
}

/// `end_turn` 帧的检查。注入敌人攻击之后，回合交替的记账全部可比。
///
/// 唯一仍然不比的是**敌人的格挡**：Defend 意图的标签是空的，加多少格挡
/// 无从预测。敌人 HP 也不比（敌人可能自伤/回血，同样是 AI 范畴）。
/// 注入式敌人回合跑完之后，内核和游戏的**存活名单**是否一致。
///
/// 为什么要有这条判断：回合结束的钩子里有**随机选目标**的效果
///（招架盾：格挡 ≥10 时对随机一只敌人打伤害）。内核的随机流故意不和游戏一致
///（不变量 4），所以它可能打死游戏里活下来的那只 —— 那只本该出手的敌人
///这一手就凭空消失了，我方 HP 自然对不上。
///
/// 这**不是规则错**，是随机分支分岔，所以那一帧的 HP 只能当软信号。
/// 名单一致时 HP 照旧是硬检验 —— 真的算错格挡仍然会红。
///
/// 2026-08-29 `act2_f33_boss_crusher_final` 帧43 就是这个：
/// 游戏的招架盾打了碾碎爪（90->84，活着并打出 9 点），
/// 内核打了 6 血的火箭（当场死，那 7 点再也没打出来）。
fn roster_diverged(s: &State, next: &Obs, slots: &BTreeMap<String, usize>) -> Option<String> {
    for e in &next.enemies {
        let Some(&slot) = slots.get(&e.combat_id) else { continue };
        if slot >= s.n_enemies as usize {
            continue;
        }
        // 游戏里它还活着（下一帧还报得出来且有血），内核里已经死了
        if e.hp > 0 && !s.enemies[slot].alive() {
            return Some(e.name.clone());
        }
        // 敌人锁血/转阶段自爆蓄力（如瀑布巨兽锁血 999999999 / 进眩晕未出招）
        if e.hp > 10000 || e.raw_intents.iter().any(|(k, _)| k == "Stun") {
            return Some(e.name.clone());
        }
    }
    None
}

fn diff_end_turn(sy: &Synced, prev: &Obs, next: &Obs, injected: bool, roster_ok: bool) -> Vec<Diff> {
    let s = &sy.state;
    let mut d = Vec::new();

    if injected && !roster_ok {
        // 存活名单分岔（多半是回合结束的随机选目标打死了不该死的那只）——
        // 这一帧的 HP 不再是格挡吸收的干净检验，降级成软信号。
        if next.hp != s.player.hp {
            d.push(Diff::soft("我方.HP(存活名单分岔，随机分支)", next.hp, s.player.hp));
        }
    } else if injected {
        // 注入之后这是真检验：格挡吸收 + 溢出进 HP 算对了没有
        if next.hp != s.player.hp {
            d.push(Diff::hard("我方.HP(敌方回合后)", next.hp, s.player.hp));
        }
    } else if next.hp != s.player.hp {
        d.push(Diff::soft("我方.HP(意图无法解析，未注入)", next.hp, s.player.hp));
    }

    if next.energy != s.energy {
        d.push(Diff::hard("能量(回合开始应回满)", next.energy, s.energy));
    }
    if next.block != s.player.block {
        d.push(Diff::hard("我方.格挡(回合开始应清零)", next.block, s.player.block));
    }
    diff_statuses("我方", &s.player, &next.status, true, &mut d);

    if next.round != prev.round + 1 && next.round != prev.round {
        d.push(Diff::soft("回合数", next.round, prev.round + 1));
    }
    // 抽牌张数是真规则（基础 5 张），但遗物/能力会改，所以只当软信号
    if next.hand.len() != s.n_hand as usize {
        d.push(Diff::soft("回合开始手牌张数", next.hand.len(), s.n_hand));
    }
    d
}

/// 从前后观测反推敌方本回合打了多少。**只在格挡被打穿时是准确值。**
///
/// 回合开始时格挡清零，所以"没用掉的格挡"和"被吸收的伤害"在观测里无法区分：
///   - 掉血了 ⇒ 格挡一定被打光，伤害 = 掉血 + 回合前的格挡，准确
///   - 没掉血 ⇒ 伤害被格挡全吃下，只知道它不超过回合前的格挡，是个上界
///
/// 这个数字将来要喂 bestiary，所以宁可标成区间也不要报一个假的确切值 ——
/// 实测第二场就撞上了：5 点格挡吃掉 3 点攻击，早先的写法报成了 5 点。
fn incoming_damage_note(prev: &Obs, next: &Obs) -> String {
    let hp_lost = prev.hp - next.hp;
    if hp_lost > 0 {
        format!("敌方本回合造成 {} 点（观测反推，未参与对拍）", hp_lost + prev.block)
    } else if prev.block > 0 {
        format!("敌方本回合造成 ≤{} 点（全被格挡吸收，无法确知，未参与对拍）", prev.block)
    } else {
        "敌方本回合造成 0 点（未参与对拍）".to_string()
    }
}

// --------------------------------------------------------------------------
// 主循环
// --------------------------------------------------------------------------

// --------------------------------------------------------------------------
// 敌人 AI 对拍
// --------------------------------------------------------------------------

/// 一手招式在观测里**长什么样**：`(意图类型, 标签)` 的列表。
///
/// 拿它和 trace 里的 `raw_intents` 直接比。攻击的标签是**最终伤害值**
/// （已实测：攻击方力量和防御方易伤都算进去了），所以这里必须走完整的
/// `apply_modifiers`，不能只报基础值。
pub fn move_signature(
    def_id: u16,
    mv: usize,
    enemy: &Entity,
    player: &Entity,
    asc: u8,
) -> Vec<(String, String)> {
    let def = crate::content::enemy_def(def_id);
    if def.moves.is_empty() {
        return Vec::new();
    }
    let m = &def.moves[mv];
    // 意图**类型**取自 `EnemyMove::intent`（照抄游戏字符串），不从 ops 推 ——
    // `EOp::Nothing` 推不出类型，会让"沉睡"和"没建模的一手"长得一样。
    // 标签只从 ops 里算：攻击是最终伤害值，塞牌是张数，其余为空。
    let mut label = String::new();
    let mut extra = Vec::new();
    // **塞牌的张数要把两条路加起来。** [源码] `TheInsatiable.LiquifyMove` 是
    // 一个 `for (i < 6)` 循环，前 3 张进抽牌堆、后 3 张进弃牌堆，而游戏的
    // `StatusIntent` 报的是**总张数 6**。内核把它建成了两条 op
    // （`AddCardToDraw` + `AddCardToDiscard`），签名只数其中一条的话
    // 标签就是 3，和观测差一半 —— `bin/synth_audit` 的开局第一手那一栏
    // 2026-09-09 把它报成「类型对、数字不对」。
    let status_cards: i32 = m
        .ops
        .iter()
        .enumerate()
        .map(|(oi, op)| match crate::asc::adjust(def_id, mv, oi, asc, *op) {
            EOp::AddCardToDiscard { count, .. }
            | EOp::AddCardToDraw { count, .. }
            | EOp::AddCardToHand { count, .. } => count,
            _ => 0,
        })
        .sum();
    // 进阶收口，见 `asc::adjust` —— 游戏在 A8/A9 显示的意图标签本来就是缩放过的，
    // 这里不缩放的话高进阶的每一手都会判成「不在允许集合里」。
    for (oi, op) in m.ops.iter().enumerate() {
        let op = crate::asc::adjust(def_id, mv, oi, asc, *op);
        match op {
            EOp::Attack { base, hits } => {
                let face = base + enemy.get(St::Strength);
                let d = crate::damage::apply_modifiers(face, enemy, player);
                label = if hits > 1 { format!("{d}×{hits}") } else { format!("{d}") };
            }
            // 恐惧（遗忘之物）：[源码] 意图是 `SingleAttackIntent(() => DreadDamage)`，
            // 那个 lambda 里已经加了它自己的敏捷 —— 标签里含它。
            EOp::AttackPlusSelfStatus { base, hits, per } => {
                let face = base + enemy.get(per) + enemy.get(St::Strength);
                let d = crate::damage::apply_modifiers(face, enemy, player);
                label = if hits > 1 { format!("{d}×{hits}") } else { format!("{d}") };
            }
            EOp::AttackPlusStackHits { base, hits, per } => {
                let hits = hits + enemy.get(per);
                let d = crate::damage::apply_modifiers(base + enemy.get(St::Strength), enemy, player);
                label = if hits > 1 { format!("{d}×{hits}") } else { format!("{d}") };
            }
            // 塞牌是**主意图**时（史莱姆吐黏液、异蛙寄生虫感染）写进主标签；
            // 是**副作用**时（扭动虫的扭动 `Buff:, StatusCard:1`）另起一个意图。
            // 2026-08-22 实战抓到：少了下面那条，扭动虫的签名只有一个 `Buff:1`，
            // 观测判成「不在允许集合里」—— 那是本轮唯一一次真红的指标。
            // 两条塞牌 op 共用**同一个总数**（见上面 `status_cards`），
            // 所以这里只认"有没有塞牌"，数字统一取那个和。
            EOp::AddCardToDiscard { .. } | EOp::AddCardToDraw { .. } | EOp::AddCardToHand { .. }
                if m.intent == "StatusCard" =>
            {
                label = format!("{status_cards}")
            }
            EOp::AddCardToDiscard { .. } | EOp::AddCardToDraw { .. } | EOp::AddCardToHand { .. } => {
                extra.push(("StatusCard".to_string(), format!("{status_cards}")))
            }
            // 一手里带了副作用时，游戏会**额外显示一个意图**。实测三种：
            //   劫掠者斧手  `Attack:5, Defend:`   攻击 + 加格挡
            //   同族神官    `Attack:8, Debuff:`   攻击 + 给我上 debuff
            //   立柱构造体  `Attack:9, Buff:`     攻击 + 给自己加力量
            // 漏掉这些会让整只敌人「对不齐」—— 神官和飞蝇菌子就是这么挂的。
            EOp::Block(_) if m.intent != "Defend" => {
                extra.push(("Defend".to_string(), String::new()))
            }
            EOp::PlayerStatus { st: St::Smoggy, .. } if m.intent != "CardDebuff" => {
                extra.push(("CardDebuff".to_string(), String::new()))
            }
            // 缠结（藤蔓蹒跚者的紧绕藤蔓）：[源码] `SingleAttackIntent + CardDebuffIntent`，
            // 不是 `DebuffIntent` —— 它改的是**牌**的费用。落进下面那条通用臂就会签成 `Debuff`。
            EOp::PlayerStatus { st: St::Tangled, .. } if m.intent != "CardDebuff" => {
                extra.push(("CardDebuff".to_string(), String::new()))
            }
            EOp::PlayerStatus { .. } if m.intent != "Debuff" && m.intent != "DebuffStrong" => {
                // 这两条也要过 `asc::adjust` —— 判据读的是 `amt >= 2`，
                // 而 A9 会抬 debuff 的量，不缩放就会在高进阶上判错意图类型。
                let adj = |i: usize| crate::asc::adjust(def_id, mv, i, asc, m.ops[i]);
                let has_strong = (0..m.ops.len()).any(
                    |i| matches!(adj(i), EOp::PlayerStatus { st: St::Vulnerable, amt } if amt >= 2),
                ) && (0..m.ops.len())
                    .any(|i| matches!(adj(i), EOp::PlayerStatus { st: St::Weak, .. }));
                let debuff_name = if has_strong {
                    "DebuffStrong"
                } else {
                    "Debuff"
                };
                extra.push((debuff_name.to_string(), String::new()))
            }
            EOp::Summon { .. } if m.intent != "Summon" => {
                extra.push(("Summon".to_string(), String::new()))
            }
            // 回血：游戏额外显示一个 `Heal:`（知识恶魔的思考是 攻击 + 回血 + 强化 三个意图）
            EOp::Heal(_) if m.intent != "Heal" => extra.push(("Heal".to_string(), String::new())),
            EOp::SelfStatus { st: St::ClawGrowth, .. } => {} // private counter, not a Buff intent
            EOp::SelfStatus { amt, .. } if amt > 0 && m.intent != "Buff" => {
                extra.push(("Buff".to_string(), String::new()))
            }
            // 给全队加 buff 和给自己加 buff 在意图面上是同一件事。
            // **今天这条臂打不到**（唯一的消费者胧光怪的哀嚎本来就是 `Buff`），
            // 留着是因为 `_ => {}` 那个兜底是静默的：漏一个 EOp 的后果是
            // 整只敌人「对不齐」，而这一臂和上面那条逐字同构，没有新判断。
            EOp::TeamStatus { amt, .. } if amt > 0 && m.intent != "Buff" => {
                extra.push(("Buff".to_string(), String::new()))
            }
            // 偷牌：游戏额外显示一个 `CardDebuff:`（偷窃草蜢的「偷盗」是
            // `SingleAttackIntent + CardDebuffIntent` 两个意图）。
            EOp::StealCard(_) if m.intent != "CardDebuff" => {
                extra.push(("CardDebuff".to_string(), String::new()))
            }
            _ => {}
        }
    }
    let mut out = vec![(m.intent.to_string(), label)];
    // **同一个类型只会显示一个意图。** 实测：碾碎爪的「虫刺」一手同时上虚弱 2 和
    // 脆弱 2（回合4 玩家身上两个都有），而游戏只显示**一个** `Debuff:`。
    // 判据不止那一帧：14 条实录里 314 组意图，**从没出现过同类型重复**
    // （最常见的组合是 `Attack,Debuff` 35 次、`Attack,Defend` 23 次）。
    // 不去重的话，一手带两个 `PlayerStatus` 就会生成 `Attack,Debuff,Debuff`，
    // 长度和观测对不上，整只敌人判「对不齐」。
    for e in extra {
        if !out.iter().any(|(t, _)| *t == e.0 || (t.starts_with("Debuff") && e.0.starts_with("Debuff"))) {
            out.push(e);
        }
    }
    out
}

/// 观测到的意图，规整成和 [`move_signature`] 同样的形状。
///
/// 和上面那个一起 `pub`：`bin/synth_audit` 要问「构造器开局挑的那一手，
/// 和游戏开局真的出的那一手一样吗」，而**那件事既有验收一条都答不了** ——
/// `verify --predict-enemy` 是先拿观测到的第一个意图去**对齐**指针，
/// 再验后面几手，开局那一手本身从来没被验过。
pub fn observed_signature(e: &EnemyObs) -> Vec<(String, String)> {
    e.raw_intents.iter().map(|(t, l)| (t.clone(), l.trim().to_string())).collect()
}

/// 只比意图**类型**，不比数字。用来在"招式对上了但伤害算错"和
/// "根本预测错了是哪一手"之间区分开 —— 这两件事要修的地方完全不同。
fn same_kinds(a: &[(String, String)], b: &[(String, String)]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.0 == y.0 || (x.0.starts_with("Debuff") && y.0.starts_with("Debuff")))
}

#[derive(Clone, Debug, Default)]
pub struct EnemyAiRow {
    pub name: String,
    /// 内容表里根本没有这个敌人
    pub unknown: bool,
    /// 对齐失败：**没有任何一手**能对上第一帧观测到的意图
    pub no_alignment: bool,
    pub predicted: usize,
    /// 类型和数字都对上
    pub exact: usize,
    /// 意图类型对上了，但数字不对（伤害算错，或者力量没跟上）
    pub kind_only: usize,
    /// 第一处分歧，人可读
    pub first_divergence: Option<String>,
    /// 每次预测时**允许集合的大小**之和。
    ///
    /// **必须和 `exact` 一起看。** 指标从"逐字相等"改成"在允许集合里"之后，
    /// 集合越大越容易命中 —— 一个把集合开到全表的实现能拿 100%，
    /// 而它什么都没预测。这个数就是用来揭穿那种情况的：
    /// 平均集合 1.0 = 完全确定，越大越弱。
    pub set_size_sum: usize,
}

#[derive(Clone, Debug, Default)]
pub struct EnemyAiReport {
    pub run: String,
    pub rows: Vec<EnemyAiRow>,
}

/// **敌人 AI 对拍**：不注入观测，改用 `EnemyDef` 预测下一手，然后和观测比。
///
/// 这是 `content.rs` 里那些出招表**唯一的判决机制**。在它出现之前，
/// 每条 `EnemyMove` 的循环都是我从三五个回合猜的，或者从 wiki 抄的，
/// 两者都没有证据。没有这个模式，L2 的跨回合搜索就是在搜一个不存在的游戏。
///
/// 做法：
/// 1. 拿这只敌人**第一次**出现时的意图，在它的招式表里找一手对得上的 —— 这是对齐。
///    对不上就说明表里根本没有这一手，直接判 `no_alignment`。
/// 2. 之后每个回合，按内核自己的规则推进指针（`+1` 循环），
///    把预测的意图和观测到的比。
///
/// **它验的是"下一手是什么"，不是"这一手打多少"** —— 后者由默认模式
/// 的注入检验（那条已经全绿）。两者要分开看。
pub fn verify_enemy_ai(t: &Trace) -> EnemyAiReport {
    let mut rep = EnemyAiReport { run: t.run.clone(), ..Default::default() };

    // combat_id -> 这只敌人的出招序列。
    //
    // **每回合一帧，外加"回合内意图类型变了"的那一帧。**
    //
    // 只按回合去重是不够的：有些机制会在**我的回合中途**把敌人改成另一手，
    // 而下一回合的观测已经是改完之后的下一步了。仪式兽的耕地闸门就是这样 ——
    // 我打到 150 那一下把它从「耕地」打进「眩晕」，回合末它执行眩晕、
    // 接着走到「兽吼」。只取回合首帧的话序列里 **少了眩晕这一手**，
    // 于是内核被喂了一个断了一步的序列，预测必然对不上。
    // 2026-08-22 那是语料里唯一一例「观测不在允许集合里」。
    //
    // 判据取**类型签名**而不是整个签名：易伤会让攻击标签的数字变
    //（`Attack:8` -> `Attack:12`），那不是换招，记进去只会重复计数。
    let mut seq: BTreeMap<String, Vec<EnemyObs>> = BTreeMap::new();
    let mut seen_round: BTreeMap<String, i32> = BTreeMap::new();
    let mut last_kinds: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for f in &t.frames {
        if combat_over_obs(&f.obs) {
            continue;
        }
        for e in &f.obs.enemies {
            let last = seen_round.get(&e.combat_id).copied().unwrap_or(-1);
            let kinds: Vec<String> =
                observed_signature(e).into_iter().map(|(k, _)| k).collect();
            let new_round = f.obs.round != last;
            let switched = last_kinds.get(&e.combat_id).is_some_and(|k| *k != kinds);
            if new_round || switched {
                seen_round.insert(e.combat_id.clone(), f.obs.round);
                last_kinds.insert(e.combat_id.clone(), kinds);
                seq.entry(e.combat_id.clone()).or_default().push(e.clone());
            }
        }
    }

    for (_cid, obs_list) in seq {
        let Some(first) = obs_list.first() else { continue };
        let mut row = EnemyAiRow { name: first.name.clone(), ..Default::default() };
        let Some(def_id) = enemy_id(&first.name) else {
            row.unknown = true;
            rep.rows.push(row);
            continue;
        };
        let n_moves = crate::content::enemy_def(def_id).moves.len();
        if n_moves == 0 {
            row.unknown = true;
            rep.rows.push(row);
            continue;
        }

        // 用观测里的 status 造一个和当时一致的敌人实体，好把力量算进标签
        let entity_of = |e: &EnemyObs| {
            let mut ent = Entity { hp: e.hp, max_hp: e.max_hp, block: e.block, status: [0; N_STATUS] };
            for (id, amt) in &e.status {
                if let Some(st) = map_status(id) {
                    ent.set(st, *amt);
                }
            }
            sync_attack_counters(e, &mut ent);
            ent
        };
        // 玩家侧只需要能影响标签的部分（易伤）。这里退化成一个空玩家 ——
        // 玩家的 status 在 `EnemyObs` 里拿不到，所以带玩家易伤的那些回合
        // 数字会对不上，会落进 `kind_only`。这是已知的局限，不是内核错。
        let player = Entity::new(80);

        // 对齐：第一次见到它时的意图对应哪一手
        let want = observed_signature(first);
        let start = (0..n_moves)
            .find(|&m| move_signature(def_id, m, &entity_of(first), &player, t.ascension) == want)
            .or_else(|| {
                (0..n_moves)
                    .find(|&m| {
                        same_kinds(
                            &move_signature(def_id, m, &entity_of(first), &player, t.ascension),
                            &want,
                        )
                    })
            });
        let Some(start) = start else {
            row.no_alignment = true;
            rep.rows.push(row);
            continue;
        };

        // 逐帧走出招机器。两处和旧版不同，都是刻意的：
        //
        // 1. **判的是「观测到的这一手在不在允许集合里」**，不是逐字相等。
        //    内核的随机流故意和游戏不一致（不变量 4），"下一手是哪个"
        //    本来就预测不了；能验的只有集合。集合大小记在 `set_size_sum`，
        //    **必须一起看** —— 把集合开到全表也能拿 100%。
        // 2. **用观测到的那一手推进指针**，不是按固定循环往下数。
        //    和默认模式"注入观测"是同一个哲学：每一步都被真实游戏拉回正轨，
        //    验的是"这一步合不合法"，而不是"连猜 20 步能不能对"。
        let mut cur = start;
        let mut hist = [u8::MAX; crate::state::ENEMY_HIST];
        let push = |h: &mut [u8; crate::state::ENEMY_HIST], v: u8| {
            for i in (1..crate::state::ENEMY_HIST).rev() {
                h[i] = h[i - 1];
            }
            h[0] = v;
        };
        for (k, o) in obs_list.iter().enumerate() {
            let seen = observed_signature(o);
            if k == 0 {
                // 第一帧是拿来对齐的，不算预测 —— 把它算进去等于自己给自己送分。
                //（旧版把它算了，所以旧的分母 95 里有一部分是白送的。）
                // `hist` **不含当前手** —— `allowed_next` 内部会自己前置它
                //（`step::eff_hist`）。在这里也 push 一次就数了两遍。
                cur = start;
                continue;
            }
            let mut scratch = State::new(80, 1);
            scratch.n_enemies = 1;
            scratch.enemy_def[0] = def_id;
            scratch.enemies[0] = entity_of(o);
            scratch.enemy_move[0] = cur as u8;
            let previous = entity_of(&obs_list[k - 1]);
            // A changed maximum HP after Adaptable is observable evidence that
            // a revive occurred between these snapshots. Advance its revive move,
            // not the attack that was interrupted by the player killing that form.
            let revived = previous.get(St::Adaptable) > 0 && o.max_hp > previous.max_hp;
            if revived {
                scratch.enemy_move[0] = 0;
            }
            // Predict growth from the PREVIOUS move's operations. Never use the
            // current observed hit count as the expected value.
            let mut growth = if revived { 0 } else { previous.get(St::ClawGrowth) };
            if !revived {
                for op in crate::content::enemy_def(def_id).moves[cur].ops {
                    if let crate::ops::EOp::SelfStatus { st: St::ClawGrowth, amt } = *op {
                        growth += amt;
                    }
                }
            }
            scratch.enemies[0].set(St::ClawGrowth, growth);
            // `hist[0]` 必须**已经是刚打完的那一手** —— `branch_open` 的
            // `NotTwice` 判的就是它。差一位的话"不能连出"会判到上上手去。
            scratch.enemy_hist[0] = hist;
            let allowed = crate::step::allowed_next(&scratch, 0);
            row.predicted += 1;
            row.set_size_sum += allowed.count_ones() as usize;

            // **在允许集合里逐个比签名**，不要反过来"先把观测认成某一手再看它在不在集合里"。
            // 后者会被签名歧义坑：两手的签名可能相同，`find` 取到的不一定是
            // 集合里的那一个，于是明明对了却报"不在集合里"。这个坑真踩过。
            let mut exact_hit = None;
            let mut kind_hit = None;
            for m in 0..n_moves {
                if allowed & (1 << m) == 0 {
                    continue;
                }
                let pred = move_signature(def_id, m, &scratch.enemies[0], &player, t.ascension);
                if pred == seen {
                    exact_hit = Some(m);
                    break;
                }
                if kind_hit.is_none() && same_kinds(&pred, &seen) {
                    kind_hit = Some((m, pred));
                }
            }
            let next_ix = match (exact_hit, &kind_hit) {
                (Some(m), _) => {
                    row.exact += 1;
                    Some(m)
                }
                (None, Some((m, pred))) => {
                    row.kind_only += 1;
                    if row.first_divergence.is_none() {
                        row.first_divergence = Some(format!(
                            "回合{}: 招在集合里但数字不对 预测={:?} 观测={:?}",
                            o_round(&obs_list, k),
                            pred,
                            seen
                        ));
                    }
                    Some(*m)
                }
                (None, None) => {
                    if row.first_divergence.is_none() {
                        let names: Vec<&str> = (0..n_moves)
                            .filter(|m| allowed & (1 << m) != 0)
                            .map(|m| crate::content::enemy_def(def_id).moves[m].name)
                            .collect();
                        row.first_divergence = Some(format!(
                            "回合{}: 观测到的这一手**不在允许集合里** 允许={:?} 观测={:?}",
                            o_round(&obs_list, k),
                            names,
                            seen
                        ));
                    }
                    // 认不出来就整表找一手最像的，好让指针能接着走
                    (0..n_moves).find(|&m| {
                        same_kinds(
                            &move_signature(def_id, m, &entity_of(o), &player, t.ascension),
                            &seen,
                        )
                    })
                }
            };
            if let Some(ix) = next_ix {
                push(&mut hist, cur as u8);
                cur = ix;
            }
        }
        rep.rows.push(row);
    }
    rep
}

/// 第 k 个观测对应的回合号（1 基，只为报告好读）
fn o_round(_list: &[EnemyObs], k: usize) -> usize {
    k + 1
}

/// **整回合预测**：一段连续出牌只在开头同步一次，连着跑完，只比末态。
///
/// 和默认的一步预测的区别就一条：中间**不重新同步**。所以它检验的是
/// 「内核连续跑 k 步会不会飘」，而一步预测每帧都被观测拉回正轨，永远发现不了
/// 累积性的错。今天精英那个 5 连击回合（预备打击→防御×3→打击）就是现成样本。
///
/// 三条限制，都是结构性的：
///
/// 1. **抽牌一旦发生就降级**。抽牌堆顺序在 mod 的 JSON 里已经丢了
///    （trace-format 约束 2），内核抽到的牌和游戏不可能一样，
///    手牌/牌堆的比较全部作废，只剩 HP/格挡/能量/敌人还能比。
/// 2. **按牌名找手牌位置**，不用观测里的 `slot`。不重新同步之后，内核的手牌
///    顺序和游戏的可能已经错开，拿观测的下标去索引内核手牌会张冠李戴。
/// 3. **只报长度 ≥2 的段**。长度 1 的段和一步预测完全等价，报出来是噪音。
pub fn verify_per_turn(t: &Trace) -> Report {
    // 走 `Replayer::new`，别手写构造 —— 每加一个跨帧带的字段就要改一处，
    // 手写的地方一定会漏（`relic_carry` 就漏过）。
    let mut r = Replayer::for_trace(t);

    let n = t.frames.len();
    let mut i = 0usize;
    // 已经"走过"的帧。段与段之间那些帧（结束回合、单张出牌、喝药水）也必须
    // 走一遍，否则**观测里没有的那些量**带不过来 —— 臂甲的充能会在下一段
    // 段首复活、`hp_lost_this_turn` 会清零导致怨恨少打一次。
    // 这两个都是实测撞出来的，不是假想。
    let mut cursor = 0usize;
    while i + 1 < n {
        while cursor < i {
            r.advance(&t.frames[cursor]);
            cursor += 1;
        }
        // 一段 = 连续的出牌动作。遇到结束回合/选牌/药水/终局就断开。
        let mut j = i;
        while j + 1 < n && matches!(t.frames[j].action, Some(Act::Play { .. })) {
            j += 1;
        }
        let len = j - i;
        if len < 2 {
            i = if len == 0 { i + 1 } else { j };
            continue;
        }

        let start = &t.frames[i];
        let target_obs = &t.frames[j].obs;
        // 段的终点落在战斗结束帧就没法比：牌区被清空、结束类遗物还改了血
        //（燃烧之血 +6）。一步预测那边靠 `combat_over_obs` 挡着，这里同理。
        if combat_over_obs(target_obs) {
            r.report.results.push(FrameResult {
                i,
                action: format!("整回合 帧{i}..{j}（{len} 个动作）"),
                verdict: Verdict::Skipped,
                diffs: Vec::new(),
                notes: vec!["这一段打完战斗就结束了，末态已不是战斗状态，不做对拍".to_string()],
            });
            i = j;
            continue;
        }
        let mut res = FrameResult {
            i,
            action: format!("整回合 帧{i}..{j}（{len} 个动作，中途不重新同步）"),
            verdict: Verdict::Match,
            diffs: Vec::new(),
            notes: Vec::new(),
        };

        let sy = r.sync(&start.obs);
        let mut st = sy.state;
        let names = sy.names.clone();
        let mut drew = false;
        let mut random_exhaust = false;
        let mut bail: Option<String> = None;

        for k in i..j {
            let f = &t.frames[k];
            let Some(Act::Play { card_name, target, .. }) = &f.action else { break };
            if lookup_card(card_name).is_none() {
                bail = Some(format!("{card_name} 不在内容表里"));
                break;
            }
            // 按牌名找位置，不用观测的 slot（见上面第 2 条）。
            // **名字要按当前升级位算** —— 见 `current_card_name`。
            let Some(h) = (0..st.n_hand as usize)
                .find(|&h| current_card_name(&names, &st.cards, st.hand[h] as usize) == *card_name)
            else {
                bail = Some(format!("内核手牌里已经没有「{card_name}」了，段内状态已经分叉"));
                break;
            };
            let tgt = target
                .as_ref()
                .and_then(|t| f.obs.enemies.iter().find(|e| &e.entity_id == t))
                .and_then(|e| r.slots.get(&e.combat_id).copied())
                .unwrap_or(0);

            let before = st;
            let c = &before.cards[before.hand[h] as usize];
            let ops = crate::content::card_ops(c.id, c.flags & crate::state::F_UPGRADED != 0);
            if ops.iter().any(|op| matches!(op, crate::ops::Op::ExhaustRandomFromHand(_) | crate::ops::Op::ExhaustRandomAttackAddDamage)) {
                random_exhaust = true;
            }
            st = step(st, Action::PlayCard { hand: h as u8, target: tgt as u8 });
            if st == before {
                bail = Some(format!("内核拒绝了「{card_name}」，但游戏接受了"));
                break;
            }
            if st.n_draw != before.n_draw {
                drew = true;
            }
        }

        if let Some(why) = bail {
            res.verdict = Verdict::UnknownContent;
            res.notes.push(why);
        } else if target_obs.pending || st.pending != Pending::None {
            res.verdict = Verdict::Skipped;
            res.notes.push(
                "段的末态停在选牌界面（pending），两边状态不在同一个时点，跳过整段对拍"
                    .to_string(),
            );
        } else {
            if drew {
                res.notes
                    .push("段内抽过牌，手牌/牌堆比较作废，只比 HP/格挡/能量/敌人".to_string());
            }
            if random_exhaust {
                res.notes.push(
                    "段内随机消耗了手牌，手牌与消耗堆身份降级为只比张数（内核随机流故意不同）"
                        .to_string(),
                );
            }
            let after = Synced { state: st, names, unmapped: sy.unmapped };
            res.diffs = diff_play(&after, target_obs, &r.slots, drew, random_exhaust);
            res.verdict = if !res.diffs.iter().any(|d| d.hard) {
                Verdict::Match
            } else if !after.unmapped.is_empty() {
                res.notes
                    .push(format!("本段存在内核不认识的 status：{}", after.unmapped.join(" ")));
                Verdict::UnknownContent
            } else {
                Verdict::Mismatch
            };
        }
        r.report.results.push(res);
        i = j;
    }
    r.report
}

/// 两帧观测**在判得着的字段上**逐字相同吗。
///
/// 只用来识别"重复记录的 end_turn"那一类伪影，所以取的是几个一动就说明
/// 真的过了一回合的量：回合数、我的血/格挡/能量、每只敌人的血和格挡。
/// **故意不比 status 和手牌** —— 那些在同一回合内也可能因为动画帧而抖动。
fn same_observation(a: &Obs, b: &Obs) -> bool {
    if a.round != b.round
        || a.hp != b.hp
        || a.block != b.block
        || a.energy != b.energy
        || a.enemies.len() != b.enemies.len()
    {
        return false;
    }
    a.enemies
        .iter()
        .zip(b.enemies.iter())
        .all(|(x, y)| x.combat_id == y.combat_id && x.hp == y.hp && x.block == y.block)
}

pub fn verify(t: &Trace) -> Report {
    // 走 `Replayer::new`，别手写构造 —— 每加一个跨帧带的字段就要改一处，
    // 手写的地方一定会漏（`relic_carry` 就漏过）。
    let mut r = Replayer::for_trace(t);

    for f in &t.frames {
        for e in &f.obs.enemies {
            for (ty, label) in &e.raw_intents {
                r.report.seen_intents.entry(ty.clone()).or_default().insert(label.clone());
            }
        }
    }

    for i in 0..t.frames.len().saturating_sub(1) {
        if let Some(res) = verify_step(&mut r, t, i) {
            r.report.results.push(res);
        }
    }

    r.report
}

/// 只验证 trace 的最新一步（倒数第 2 帧 -> 最后一帧）。
/// 前置帧只跑 advance 维护跨帧携带的计数器与遗物状态。
pub fn verify_last(t: &Trace) -> Option<FrameResult> {
    if t.frames.len() < 2 {
        return None;
    }
    let mut r = Replayer::for_trace(t);
    for f in &t.frames {
        for e in &f.obs.enemies {
            for (ty, label) in &e.raw_intents {
                r.report.seen_intents.entry(ty.clone()).or_default().insert(label.clone());
            }
        }
    }
    let last_idx = t.frames.len() - 2;
    for k in 0..last_idx {
        r.advance(&t.frames[k]);
    }
    verify_step(&mut r, t, last_idx)
}

/// 验证 trace 中的单步（第 i 帧动作 -> 第 i+1 帧观测）。
/// 若第 i 帧没有动作，返回 None。
pub fn verify_step(r: &mut Replayer, t: &Trace, i: usize) -> Option<FrameResult> {
    let f = &t.frames[i];
    let next = &t.frames[i + 1].obs;
    let act = f.action.as_ref()?;

        if let Some(sv) = f.settle {
            if sv.unstable {
                r.report.settle_unstable += 1;
            }
            if sv.intermediate_differs {
                r.report.settle_intermediate += 1;
            }
            r.report.settle_max_ms = r.report.settle_max_ms.max(sv.ms);
        }

        let mut res =
            FrameResult { i, action: act.label(), verdict: Verdict::Match, diffs: Vec::new(), notes: Vec::new() };

        // **重复记录的 end_turn**：上一帧也是 end_turn，而两帧的观测**逐字相同**。
        //
        // 那是录制器重试留下的伪影（同一次结束回合被记了两遍），不是游戏里
        // 真的连着结束了两回合 —— 判据就是"观测一个字都没动"：真结束过一次的话
        // 血量/格挡/能量/回合数至少要动一个。
        //
        // 内核会照常把这一帧当成"再结束一回合"，于是凭空多挨一顿 ——
        // 2026-08-27 在 `act2_f33_boss_crusher.json` 帧18 露出来过
        // （游戏 41 血、内核 21）。**它以前是被"未映射 status"顺带跳过的**，
        // 我把那批 status 接上之后才现形 —— 一个被别的问题挡住的问题。
        //
        // 显式跳过并报出来，而不是放宽判据：这一帧本来就没有可判的东西。
        if matches!(act, Act::EndTurn) {
            // 判据看的是**下一帧**：我这一帧结束了回合，可是下一帧的观测一个字没动、
            // 而且录制器又记了一次 `end_turn` —— 说明**这一次没落地**，是重试。
            let next_is_end = matches!(t.frames[i + 1].action, Some(Act::EndTurn));
            if next_is_end && same_observation(&f.obs, next) {
                res.verdict = Verdict::Skipped;
                res.notes.push(
                    "重复记录的 end_turn（和上一帧观测逐字相同）—— 录制器重试留下的伪影，                     这一帧不判"
                        .to_string(),
                );
                return Some(res);
            }
        }

        match act {
            // 被动录制器反推不出来的一步。**以前它落成 `action = None`、被
            // 静默 `continue` 掉**；现在显式报成跳过，理由写出来。
            // 判决不变（这一帧本来就没参与），变的只是它不再是隐形的。
            Act::Unknown => {
                res.verdict = Verdict::Skipped;
                res.notes.push(
                    "动作反推不出来（被动录制），这一帧不判 —— 但它后面那几帧照常判，                     所以真正的风险是「漏掉的那一步改了局面」而不是这一帧本身"
                        .to_string(),
                );
            }
            Act::SelectCard { .. } | Act::Confirm => {
                res.verdict = Verdict::Skipped;
                res.notes.push("v1 还不对拍选牌界面".to_string());
            }
            Act::UsePotion { name, slot, target } => {
                let sy = r.sync(&f.obs);
                let kid = if *slot < MAX_POTIONS {
                    sy.state.potions[*slot]
                } else {
                    potion::UNKNOWN
                };
                if kid == potion::NONE || kid == potion::UNKNOWN {
                    // 内容表里没有这瓶 —— 是覆盖率问题，不是算错。
                    r.report
                        .missing_potions
                        .entry(name.clone())
                        .and_modify(|n| *n += 1)
                        .or_insert(1);
                    res.verdict = Verdict::UnknownContent;
                    res.notes.push(format!("{name}（槽{slot}）：内容表里没有这瓶药水"));
                } else {
                    let tgt = target
                        .as_ref()
                        .and_then(|t| f.obs.enemies.iter().find(|e| &e.entity_id == t))
                        .and_then(|e| r.slots.get(&e.combat_id).copied())
                        .unwrap_or(0);
                    let before = sy.state;
                    let st = step(
                        before,
                        Action::UsePotion { slot: *slot as u8, target: tgt as u8 },
                    );
                    if st == before {
                        res.verdict = Verdict::Mismatch;
                        res.diffs.push(Diff::hard(
                            format!("喝药水[{slot}] {name}"),
                            "游戏喝下去了",
                            "内核拒绝了这个动作",
                        ));
                    } else if combat_over_obs(next) {
                        // **喝药水也可能是最后一击。** 出牌那条分支早就有这个
                        // 判断，药水这条漏了 —— 于是"药水斩杀"的最后一帧会拿
                        // 战斗结束后被清空的牌区去和内核比，报出一整列假不一致
                        //（2026-08-22 第2幕第31层精英就是火焰药水收的尾）。
                        // 两条路径问的是同一件事，判断也该是同一个。
                        r.save_carry(&st);
                        combat_end_hp_verdict(&st, &f.obs, next, &mut res);
                    } else {
                        let drew = st.n_draw != before.n_draw;
                        r.save_carry(&st);
                        let after =
                            Synced { state: st, names: sy.names, unmapped: sy.unmapped };
                        res.diffs = diff_play(&after, next, &r.slots, drew, false);
                        res.verdict = if !res.diffs.iter().any(|d| d.hard) {
                            Verdict::Match
                        } else if !after.unmapped.is_empty() {
                            res.notes.push(format!(
                                "本帧存在内核不认识的 status：{}",
                                after.unmapped.join(" ")
                            ));
                            Verdict::UnknownContent
                        } else {
                            Verdict::Mismatch
                        };
                        if drew {
                            res.notes
                                .push("这瓶药水抽了牌，手牌比较降级成张数".to_string());
                        }
                    }
                }
            }
            Act::EndTurn => {
                let sy = r.sync(&f.obs);
                let incoming = observed_incoming(&f.obs, &r.slots);
                let can_inject = incoming.is_some();
                let total: i32 =
                    f.obs.enemies.iter().flat_map(|e| &e.attacks).map(|(b, h)| b * h).sum();

                let st = sy.state;
                // `end_turn` 会一路跑进 `start_player_turn`，所以 `end_state`
                // 已经是**下一个回合**了。它身上的每回合计数器必须跟着带过去：
                //
                // 绯红披风在回合开始时扣 1 点血 —— 那笔 `hp_lost_this_turn`
                // 属于**新回合**，而怨恨「本回合失去过生命值则攻击两次」读的正是它。
                // 早先这里只存不认领，下一帧 `sync` 一看回合数变了就整个清零，
                // 于是怨恨少打一次。实战抓到的（2026-08-17 第2幕31层，
                // 帧12/帧17 各差 7 点和 10 点）—— 合成样本里从来没同时出现过
                // 「回合开始掉血的能力」和「读掉血计数的牌」。
                //
                // 所以把 `last_round` 也推到新回合：这不是"跳过重置"，是**认领**
                // 这批计数器已经属于新回合了。
                // **注入走 `end_turn_with_incoming`**，它把这一手放在
                // 回合结束钩子**之后**（正确的位置）。不能注入时退回
                // `step(EndTurn)`，那条路由内核自己的敌人 AI 出手。
                let end_state = match &incoming {
                    Some(inc) => crate::step::end_turn_with_incoming(st, inc),
                    None => step(st, Action::EndTurn),
                };
                r.save_carry(&end_state);
                r.last_round = end_state.turn;
                if combat_over_obs(next) {
                    let mut final_state = end_state;
                    if !final_state.combat_over {
                        final_state.combat_over = true;
                        crate::step::fire(&mut final_state, crate::ops::Hook::CombatVictoryEarly, 0);
                        crate::step::fire(&mut final_state, crate::ops::Hook::CombatVictory, 0);
                    }
                    r.save_carry(&final_state);
                    combat_end_hp_verdict(&final_state, &f.obs, next, &mut res);
                } else {
                    let after = Synced {
                        state: end_state,
                        names: sy.names,
                        unmapped: sy.unmapped,
                    };
                    let diverged = if can_inject {
                        roster_diverged(&after.state, next, &r.slots)
                    } else {
                        None
                    };
                    res.diffs = diff_end_turn(&after, &f.obs, next, can_inject, diverged.is_none());
                    res.verdict = if res.diffs.iter().any(|d| d.hard) {
                        if !after.unmapped.is_empty() {
                            res.notes.push(format!(
                                "本帧存在内核不认识的 status：{}",
                                after.unmapped.join(" ")
                            ));
                            Verdict::UnknownContent
                        } else {
                            Verdict::Mismatch
                        }
                    } else if can_inject {
                        Verdict::Match
                    } else {
                        Verdict::Partial
                    };
                    if let Some(who) = &diverged {
                        res.notes.push(format!(
                            "存活名单分岔：{who} 在游戏里活着、在内核里死了。回合结束有随机选目标的效果（招架盾/势不可当），而内核的随机流故意不和游戏一致（不变量 4）—— 那只该出手的敌人这一手没打出来，所以这一帧的 HP 只当软信号"
                        ));
                    }
                    if can_inject {
                        res.notes.push(format!("注入敌方意图 {total} 点，检验格挡吸收与溢出"));
                    } else {
                        res.notes.push("有意图标签解析不出数字，未注入".to_string());
                        res.notes.push(incoming_damage_note(&f.obs, next));
                    }
                }
            }
            Act::Play { slot, card_id, card_name, target } => {
                let Some(c) = f.obs.hand.iter().find(|c| c.slot == *slot) else {
                    res.verdict = Verdict::Skipped;
                    res.notes.push(format!("trace 损坏：手牌里没有第 {slot} 位"));
                    return Some(res);
                };
                if !card_id.is_empty() && &c.id != card_id {
                    res.verdict = Verdict::Skipped;
                    res.notes.push(format!(
                        "trace 损坏：第 {slot} 位记的是 {card_id}，实际是 {}",
                        c.id
                    ));
                    return Some(res);
                }

                match lookup_card(&c.name) {
                    None => {
                        let e = r.report.missing_cards.entry(c.name.clone()).or_insert_with(|| {
                            CardSighting {
                                id: c.id.clone(),
                                name: c.name.clone(),
                                cost: c.cost,
                                kind: c.kind.clone(),
                                description: c.description.clone(),
                                count: 0,
                            }
                        });
                        e.count += 1;
                        res.verdict = Verdict::UnknownContent;
                        res.notes.push(format!("{card_name} 不在内容表里"));
                    }
                    Some(cid) => {
                        // 卡面费用对不上，本身就是内容表的错，先报出来。
                        //
                        // 但**减费牌不查**：踩踏的显示费用本来就会随本回合打过的
                        // 攻击牌下降（实录 f13 里看到 3→2→1→0），无情猛攻之后的
                        // 那张攻击牌也是 0 费。拿它们去比内容表里的基础费用，
                        // 报出来的全是噪音。
                        let def = card(cid);
                        let want = if c.upgraded { def.cost_upg } else { def.cost };
                        let discountable =
                            kernel_can_explain_zero_cost(def, r.counters.free_attack);
                        // 缠结是全局改费，不是内容表的错：比之前扣掉（和 `sync` 同一个函数）。
                        let tangled = observed_player_status(&f.obs.status, St::Tangled);
                        let mut who = Entity::new(1);
                        who.set(St::Tangled, tangled);
                        let global = crate::step::tangled_cost_addend(&who, cid);
                        if let Some(cost) = c.cost.map(|c| c - global) {
                            if cost != want && !discountable {
                                res.diffs.push(Diff::soft(
                                    format!("{card_name}.费用(内容表)"),
                                    cost,
                                    want,
                                ));
                            }
                        }

                        let sy = r.sync(&f.obs);
                        let tgt = target
                            .as_ref()
                            .and_then(|t| f.obs.enemies.iter().find(|e| &e.entity_id == t))
                            .and_then(|e| r.slots.get(&e.combat_id).copied())
                            .unwrap_or(0);

                        let before = sy.state;
                        let after_state =
                            step(before, Action::PlayCard { hand: *slot as u8, target: tgt as u8 });
                        // 把内核跑出来的每回合计数带到下一帧（观测里没有这些量）
                        r.save_carry(&after_state);

                        if after_state == before {
                            res.verdict = Verdict::Mismatch;
                            res.notes.push(
                                "内核拒绝了这个动作（判为非法），但游戏接受了".to_string(),
                            );
                        } else if next.pending || after_state.pending != Pending::None {
                            // **游戏还开着选牌界面，两边不在同一个时点。**
                            //
                            // [源码] `Brand.OnPlay` 是
                            // `失血 -> await CardSelectCmd.FromHand(...) -> 消耗 -> 力量 +N`：
                            // 选择**没确认之前**，后面那半根本还没结算，牌也还没进弃牌堆。
                            // 而内核的 `Pending` 是**急切**的：`Op::ExhaustChoose` 只是
                            // 立一个标记，同一张牌剩下的 op 当场就跑完了。
                            //
                            // 于是这一帧内核比游戏"走得更远"，逐字段比必然假红
                            // —— 2026-08-22 第2幕 Boss 第一次真打出烙印+ 时报的就是
                            // 「我方.力量 游戏=0 内核=2」和「弃牌堆多一张烙印+」，
                            // 而**确认之后两边完全一致**（帧38 对上了）。
                            //
                            // 真修是让 `step` 能在选择处**挂起并续跑**（`State` 里存
                            // "打到第几个 op"），那是引擎级改动，不是一行表。
                            // 在那之前这一帧报**跳过并说明原因**，而不是报内核算错
                            // —— 和 `solve --live` 遇到 pending 直接拒绝作答是同一条止血。
                            res.verdict = Verdict::Skipped;
                            res.notes.push(
                                "两边的选牌状态对不上：内核的 `Pending` 是**急切**的（`Op::ExhaustChoose` 只立标记，同一张牌剩下的 op 当场跑完），而游戏要等确认。两个方向都会假红：游戏还开着界面时内核走得更远；候选只剩一张时游戏**自动确认**而内核还挂着。确认之后的那一帧照常逐字段判。"
                                    .to_string(),
                            );
                        } else if combat_over_obs(next) {
                            // 战斗结束帧原来是**整帧跳过**的，于是战斗结束类遗物
                            //（燃烧之血/带骨肉）"建了也验不了"。其实牌区不可比
                            // 不代表血量不可比 —— 而血量正是那一类遗物**唯一**
                            // 显形的地方。改成只比这一条。
                            combat_end_hp_verdict(&after_state, &f.obs, next, &mut res);
                        } else {
                            let drew = after_state.n_draw != before.n_draw;
                            if drew {
                                res.notes
                                    .push("这张牌抽了牌，手牌身份降级为只比张数".to_string());
                            }
                            let random_exhaust = if let Some(Act::Play { slot, .. }) = f.action {
                                if (slot as usize) < before.n_hand as usize {
                                    let c = &before.cards[before.hand[slot as usize] as usize];
                                    let ops = crate::content::card_ops(c.id, c.flags & crate::state::F_UPGRADED != 0);
                                    ops.iter().any(|op| matches!(op, crate::ops::Op::ExhaustRandomFromHand(_) | crate::ops::Op::ExhaustRandomAttackAddDamage))
                                } else {
                                    false
                                }
                            } else {
                                false
                            };
                            if random_exhaust {
                                res.notes.push(
                                    "这张牌随机消耗了手牌，手牌与消耗堆身份降级为只比张数（内核随机流故意不同）"
                                        .to_string(),
                                );
                            }
                            let after = Synced {
                                state: after_state,
                                names: sy.names,
                                unmapped: sy.unmapped,
                            };
                            // 追加而不是覆盖 —— 之前这里是 `=`，把上面收集的
                            // 卡面费用软差异整个丢掉了，成功路径上永远看不见它
                            res.diffs.extend(diff_play(&after, next, &r.slots, drew, random_exhaust));
                            res.verdict = if !res.diffs.iter().any(|d| d.hard) {
                                Verdict::Match
                            } else if !after.unmapped.is_empty() {
                                // 内核看不见的状态在场，不能把差异算在伤害管线头上
                                res.notes.push(format!(
                                    "本帧存在内核不认识的 status：{}",
                                    after.unmapped.join(" ")
                                ));
                                Verdict::UnknownContent
                            } else {
                                Verdict::Mismatch
                            };
                        }
                    }
                }
            }
        }
    Some(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **复活窗口：观测里没有它，不等于战斗结束。**
    ///
    /// 实验体被砍掉一条命之后整个我方回合都不在 `enemies` 里（[实测]
    /// act3_f48 帧11~13 是 `enemies: []`），和"已经打完了"在观测层长得一样。
    /// `sync` 每帧从观测重建，所以只能靠 `revive_owed` 记住"消失前它带着适生力"。
    ///
    /// 丢了这一条不会报错，会**静悄悄地**判战斗结束：后面的出牌全被判非法、
    /// `EndTurn` 变空操作（能量不回满、格挡不清零）、还白结算一次胜利遗物。
    #[test]
    fn sync_keeps_a_vanished_adaptable_enemy_on_the_field() {
        fn frame(enemies: &str) -> String {
            format!(
                r#"{{
  "version": 1,
  "run": {{ "act": 3, "floor": 48, "ascension": 1, "character": "铁甲战士" }},
  "frames": [
    {{ "i": 0,
      "obs": {{
        "state_type": "boss", "round": 2, "is_play_phase": true,
        "energy": 3, "max_energy": 3,
        "player": {{ "hp": 60, "max_hp": 80, "block": 0, "status": {{}} }},
        "enemies": {enemies},
        "hand": [], "draw_count": 0, "draw": [], "discard": [], "exhaust": [],
        "pending": null
      }},
      "action": null,
      "settle": {{ "polls": 1, "ms": 0, "unstable": false, "intermediate_differs": false }} }}
  ]
}}"#
            )
        }

        let alive = frame(
            r#"{ "1": { "entity_id": "test_subject_0", "name": "实验体 #C10",
                        "hp": 4, "max_hp": 100, "block": 0,
                        "status": { "ADAPTABLE_POWER": 1 }, "intents": [] } }"#,
        );
        let gone = frame("{}");

        let mut r = Replayer::new("");
        let s = r.sync(&parse_trace(&alive).unwrap().frames[0].obs).state;
        assert!(s.any_enemy_present(), "看得见的时候当然在场");

        // 下一帧它从观测里消失 —— 但这是"死着等复活"，不是打完了
        let s = r.sync(&parse_trace(&gone).unwrap().frames[0].obs).state;
        assert!(!s.enemies[0].alive(), "血量确实是 0，它这一手打不到人");
        assert!(s.any_enemy_present(), "带着适生力消失 = 欠一次复活，战斗不结束");

        // 对照：第三形态不带适生力，砍掉就是真结束
        let last_form = frame(
            r#"{ "1": { "entity_id": "test_subject_0", "name": "实验体 #C10",
                        "hp": 5, "max_hp": 300, "block": 0,
                        "status": { "NEMESIS_POWER": 1 }, "intents": [] } }"#,
        );
        let mut r2 = Replayer::new("");
        r2.sync(&parse_trace(&last_form).unwrap().frames[0].obs);
        let s = r2.sync(&parse_trace(&gone).unwrap().frames[0].obs).state;
        assert!(!s.any_enemy_present(), "没有适生力就是真死了，别把战斗吊着");
    }

    /// **接续的尸体：不在观测里，但欠一次复活 —— 而且要推回"还差几个敌人回合"。**
    ///
    /// [实测] `act2_f28_decimillipede` 节 1：第 3 回合砍死、第 4 回合观测里没有、
    /// 第 5 回合 25/44 回来。三样缺一不可：层数（回多少）· 倒计时（什么时候回）·
    /// 最大生命（回血的上限 —— 尸体的实体是清零重建的，不放回去就只能回到 0）。
    #[test]
    fn sync_keeps_a_vanished_segment_as_a_corpse_owed_a_reattach() {
        fn frame(round: i32, enemies: &str) -> String {
            format!(
                r#"{{
  "version": 1,
  "run": {{ "act": 2, "floor": 28, "ascension": 2, "character": "铁甲战士" }},
  "frames": [
    {{ "i": 0,
      "obs": {{
        "state_type": "elite", "round": {round}, "is_play_phase": true,
        "energy": 3, "max_energy": 3,
        "player": {{ "hp": 60, "max_hp": 80, "block": 0, "status": {{}} }},
        "enemies": {enemies},
        "hand": [], "draw_count": 0, "draw": [], "discard": [], "exhaust": [],
        "pending": null
      }},
      "action": null,
      "settle": {{ "polls": 1, "ms": 0, "unstable": false, "intermediate_differs": false }} }}
  ]
}}"#
            )
        }
        fn seg(id: &str, hp: i32, max_hp: i32) -> String {
            format!(
                r#""{id}": {{ "entity_id": "decimillipede_segment_{id}", "name": "残杀千足虫",
                        "hp": {hp}, "max_hp": {max_hp}, "block": 0,
                        "status": {{ "REATTACH_POWER": 25 }}, "intents": [] }}"#
            )
        }
        let tr = |round: i32, segs: &[String]| {
            parse_trace(&frame(round, &format!("{{ {} }}", segs.join(", ")))).unwrap()
        };

        let mut r = Replayer::new("");
        r.sync(&tr(3, &[seg("1", 12, 44), seg("2", 40, 40), seg("3", 46, 46)]).frames[0].obs);

        // 同一回合里节 1 从观测里消失：死了，但欠一次接续，倒计时 2（还没过敌人回合）
        let s = r.sync(&tr(3, &[seg("2", 40, 40), seg("3", 46, 46)]).frames[0].obs).state;
        assert!(!s.enemies[0].alive(), "尸体打不到");
        assert_eq!(s.enemies[0].get(St::Reattach), 25);
        assert_eq!(s.enemies[0].get(St::ReattachDue), 2, "砍死那一回合：还要过两个敌人回合");
        assert_eq!(s.enemies[0].max_hp, 44, "回血上限要放回去");
        assert!(s.any_enemy_present());

        // 下一回合仍然不在：倒计时 1。走一个敌人回合，它该 25 血回来 ——
        // 对拍路径上尸体是 UNKNOWN def，回血那条规则不能依赖认出它是谁
        let s = r.sync(&tr(4, &[seg("2", 40, 40), seg("3", 46, 46)]).frames[0].obs).state;
        assert_eq!(s.enemies[0].get(St::ReattachDue), 1, "过了一个敌人回合");
        let s = crate::step::end_turn_with_incoming(s, &crate::step::NO_INCOMING);
        assert_eq!(s.enemies[0].hp, 25, "第二个敌人回合开始时 25 血回来");

        // 对照：三节都不在了 —— 尸体照样挂着接续，但战斗该结束（接续不进 any_enemy_present）
        let s = r.sync(&tr(4, &[]).frames[0].obs).state;
        assert!(!s.any_enemy_present(), "别的节全死了就是真结束，别把战斗吊着");
    }

    /// **硬化外壳从战斗中途接进来。** mod 报的是 `DisplayAmount`（这个回合的**余额**），
    /// 原样搬进 `St::HardenedShell`；上限（`Amount` = 20）观测里没有，按名字从
    /// `content::ENEMY_PRIVATE_MARKERS` 挂回来 —— 挂不上的话，下一个回合开始它永远回不满，
    /// 而余额是 0 的那一帧外壳会「消失」（`absorb` 的门是上限）。
    #[test]
    fn sync_reads_the_hardened_shell_balance_and_puts_the_cap_back_by_name() {
        fn frame(balance: i32) -> String {
            format!(
                r#"{{
  "version": 1,
  "run": {{ "act": 1, "floor": 9, "ascension": 2, "character": "铁甲战士" }},
  "frames": [
    {{ "i": 0,
      "obs": {{
        "state_type": "elite", "round": 2, "is_play_phase": true,
        "energy": 3, "max_energy": 3,
        "player": {{ "hp": 60, "max_hp": 80, "block": 0, "status": {{}} }},
        "enemies": {{ "1": {{ "entity_id": "skulking_colony_0", "name": "鬼祟珊瑚群",
                            "hp": 60, "max_hp": 75, "block": 0,
                            "status": {{ "HARDENED_SHELL_POWER": {balance} }}, "intents": [] }} }},
        "hand": [], "draw_count": 0, "draw": [], "discard": [], "exhaust": [],
        "pending": null
      }},
      "action": null,
      "settle": {{ "polls": 1, "ms": 0, "unstable": false, "intermediate_differs": false }} }}
  ]
}}"#
            )
        }
        let mut r = Replayer::new("");
        let s = r.sync(&parse_trace(&frame(5)).unwrap().frames[0].obs).state;
        assert_eq!(s.enemies[0].get(St::HardenedShell), 5, "余额原样搬");
        assert_eq!(s.enemies[0].get(St::HardenedShellCap), 20, "上限按名字挂回来");
        let mut e = s.enemies[0];
        assert_eq!(crate::damage::absorb(&mut e, 12), 5, "这个回合只剩 5");
        let s = crate::step::end_turn_with_incoming(s, &crate::step::NO_INCOMING);
        assert_eq!(s.enemies[0].get(St::HardenedShell), 20, "回合开始回满到上限");

        let mut r = Replayer::new("");
        let s = r.sync(&parse_trace(&frame(0)).unwrap().frames[0].obs).state;
        let mut e = s.enemies[0];
        assert_eq!(crate::damage::absorb(&mut e, 12), 0, "余额 0 的那一帧外壳还在");
    }

    /// **缠结下 mod 报的手牌费用已经含 +1**（`GetAmountToSpend` 过了全局钩子），`sync` 要先扣掉再判
    /// 「这张实例被本地改过费」。不扣的话每张攻击牌记成 `cost_delta = +1`，`effective_cost` 再加一遍缠结 ——
    /// 打击变 3 费，求解器给出游戏里根本打得出、却被它当成打不出的线（和无情猛攻那个 bug 同一个形状）。
    /// 第四张是缠结下的**免费**打击：显示 1，扣完 0 ⇒ 照旧盖「本回合免费」，内核算回 1。
    #[test]
    fn sync_does_not_double_count_tangled_in_the_observed_hand_cost() {
        let hand = r#"[
          { "slot": 0, "id": "STRIKE_IRONCLAD", "name": "打击", "cost": 2, "type": "Attack", "upgraded": false, "can_play": true, "description": "" },
          { "slot": 1, "id": "DEFEND_IRONCLAD", "name": "防御", "cost": 1, "type": "Skill", "upgraded": false, "can_play": true, "description": "" },
          { "slot": 2, "id": "BASH", "name": "痛击", "cost": 3, "type": "Attack", "upgraded": false, "can_play": true, "description": "" },
          { "slot": 3, "id": "STRIKE_IRONCLAD", "name": "打击", "cost": 1, "type": "Attack", "upgraded": false, "can_play": true, "description": "" }
        ]"#;
        let frame = |status: &str| {
            format!(
                r#"{{
  "version": 1,
  "run": {{ "act": 1, "floor": 5, "ascension": 2, "character": "铁甲战士" }},
  "frames": [
    {{ "i": 0,
      "obs": {{
        "state_type": "monster", "round": 3, "is_play_phase": true,
        "energy": 3, "max_energy": 3,
        "player": {{ "hp": 60, "max_hp": 80, "block": 0, "status": {{ {status} }} }},
        "enemies": {{ "1": {{ "entity_id": "vine_shambler_0", "name": "藤蔓蹒跚者",
                            "hp": 50, "max_hp": 61, "block": 0, "status": {{}}, "intents": [] }} }},
        "hand": {hand},
        "draw_count": 0, "draw": [], "discard": [], "exhaust": [], "pending": null
      }},
      "action": null,
      "settle": {{ "polls": 1, "ms": 0, "unstable": false, "intermediate_differs": false }} }}
  ]
}}"#
            )
        };
        let mut r = Replayer::new("");
        let s = r.sync(&parse_trace(&frame(r#""TANGLED_POWER": 1"#)).unwrap().frames[0].obs).state;
        assert_eq!(s.player.get(St::Tangled), 1);
        let costs: Vec<i32> = (0..4).map(|i| crate::step::effective_cost(&s, i)).collect();
        assert_eq!(costs, vec![2, 1, 3, 1], "和游戏显示的逐张相同，没有双算");
        let deltas: Vec<i8> = (0..4).map(|i| s.cards[s.hand[i] as usize].cost_delta).collect();
        assert_eq!(deltas, vec![0, 0, 0, 0], "缠结是全局改费，不是这张实例被改过");
        assert!(s.cards[s.hand[3] as usize].flags & F_FREE_THIS_TURN != 0, "显示 1 = 免费 + 缠结");
        assert!(s.cards[s.hand[0] as usize].flags & F_FREE_THIS_TURN == 0);

        // 对照：同一手牌、没有缠结 ⇒ 打击 2 费 / 痛击 3 费就是这张实例真被改过（狂乱逃离那种）
        let mut r2 = Replayer::new("");
        let s = r2.sync(&parse_trace(&frame("")).unwrap().frames[0].obs).state;
        let deltas: Vec<i8> = (0..4).map(|i| s.cards[s.hand[i] as usize].cost_delta).collect();
        assert_eq!(deltas, vec![1, 0, 1, 0]);
    }

    /// **扯碎的段数从卡面反推。**
    ///
    /// `hp_loss_hits`（本场挨穿过几次）观测里没有，内核自己那份只从同步那一刻
    /// 起累加 —— 而这张牌吃的是整场历史。游戏把算好的段数渲染进了卡面
    /// （`（命中3次）`），段数 = 1 + 次数，所以次数 = 3 − 1 = 2。
    ///
    /// [实测] `act2_f28_decimillipede` 帧31 就是这句话，而在那之前只有一个
    /// 回合边界掉过血（55 → 44）—— 那一手是多段攻击，**两段打穿了 8 点格挡**。
    /// 逐帧数"掉血的回合"会得到 1，那是错的：数的是**伤害次数**。
    #[test]
    fn sync_reads_tear_asunder_hit_count_off_the_card_text() {
        fn frame(hand: &str) -> String {
            format!(
                r#"{{
  "version": 1,
  "run": {{ "act": 2, "floor": 28, "ascension": 2, "character": "铁甲战士" }},
  "frames": [
    {{ "i": 0,
      "obs": {{
        "state_type": "monster", "round": 5, "is_play_phase": true,
        "energy": 3, "max_energy": 3,
        "player": {{ "hp": 44, "max_hp": 80, "block": 0, "status": {{}} }},
        "enemies": {{ "7": {{ "entity_id": "worm_0", "name": "<合成:沙包>",
                            "hp": 90, "max_hp": 90, "block": 0, "status": {{}}, "intents": [] }} }},
        "hand": {hand},
        "draw_count": 0, "draw": [], "discard": [], "exhaust": [], "pending": null
      }},
      "action": null,
      "settle": {{ "polls": 1, "ms": 0, "unstable": false, "intermediate_differs": false }} }}
  ]
}}"#
            )
        }

        let tear = r#"[{ "slot": 0, "id": "TEAR_ASUNDER", "name": "扯碎+", "cost": 2,
                         "type": "Attack", "upgraded": true, "can_play": true,
                         "description": "造成7点伤害。 在本场战斗中，你每失去过一次生命值，这张牌就额外造成一次伤害。 （命中3次）" }]"#;
        let t = parse_trace(&frame(tear)).expect("trace 解析失败");
        let mut r = Replayer::new("");
        let s = r.sync(&t.frames[0].obs).state;
        assert_eq!(s.hp_loss_hits, 2, "段数 3 ⇒ 挨穿过 2 次");

        // 牌不在手上：读不出来就用携带值（这里是 0），**不猜**
        let t = parse_trace(&frame("[]")).expect("trace 解析失败");
        let mut r2 = Replayer::new("");
        assert_eq!(r2.sync(&t.frames[0].obs).state.hp_loss_hits, 0);

        // 括号里没有数字（换了语言 / 换了措辞）：同样按读不出来处理
        let odd = r#"[{ "slot": 0, "id": "TEAR_ASUNDER", "name": "扯碎", "cost": 2,
                        "type": "Attack", "upgraded": false, "can_play": true,
                        "description": "Deal 5 damage." }]"#;
        let t = parse_trace(&frame(odd)).expect("trace 解析失败");
        let mut r3 = Replayer::new("");
        assert_eq!(r3.sync(&t.frames[0].obs).state.hp_loss_hits, 0);
    }

    /// **牌堆里的附魔和手牌里的是同一件事。**
    ///
    /// 带灵巧的耸肩无视在手上给 10 点格挡；它躺在抽牌堆里的时候观测不带附魔，
    /// 于是段内才抽出来的那一张按卡表算 8 —— `act1_f14_phantasmal_gardeners`
    /// 的整回合对拍就是这么红的（游戏 15 / 内核 13，差的正好是灵巧那 2 点）。
    ///
    /// 顺带钉住**下标换算**：`draw_order_enchant` 和 `draw_order` 逐位置配对，
    /// 而内核 `draw[]` 是反着填的。拿 `rev()` 的计数当下标不会报错，
    /// 只会把附魔安到另一张牌头上 —— 所以这里故意把附魔放在中间那一张。
    #[test]
    fn sync_carries_enchantments_on_cards_still_in_the_draw_pile() {
        let src = r#"{
  "version": 1,
  "run": { "act": 1, "floor": 14, "ascension": 2, "character": "铁甲战士" },
  "frames": [
    { "i": 0,
      "obs": {
        "state_type": "monster", "round": 1, "is_play_phase": true,
        "energy": 3, "max_energy": 3,
        "player": { "hp": 57, "max_hp": 80, "block": 0, "status": {} },
        "enemies": { "7": { "entity_id": "worm_0", "name": "<合成:沙包>",
                            "hp": 22, "max_hp": 22, "block": 0, "status": {}, "intents": [] } },
        "hand": [],
        "draw_count": 3,
        "draw": [ { "name": "防御" }, { "name": "打击" }, { "name": "耸肩无视" } ],
        "draw_order": [ "打击", "耸肩无视", "防御" ],
        "draw_order_enchant": [ null, { "id": "NIMBLE", "name": "灵巧", "amount": 2 }, null ],
        "discard": [], "exhaust": [], "pending": null
      },
      "action": null,
      "settle": { "polls": 1, "ms": 0, "unstable": false, "intermediate_differs": false } }
  ]
}"#;
        let t = parse_trace(src).expect("trace 解析失败");
        let mut r = Replayer::new("");
        let mut s = r.sync(&t.frames[0].obs).state;

        let first = s.pop_draw_top().expect("抽牌堆非空");
        assert_eq!(crate::content::card(s.cards[first as usize].id).name, "打击");
        assert_eq!(s.cards[first as usize].ench, 0, "附魔不该串到邻座");

        let shrug = s.pop_draw_top().expect("还有牌");
        assert_eq!(crate::content::card(s.cards[shrug as usize].id).name, "耸肩无视");
        assert_eq!(
            crate::content::ench_block_add(&s.cards[shrug as usize]),
            2,
            "灵巧 +2 跟着这一张实例"
        );

        // 端到端：真打出去就该是 10 点，不是卡表的 8
        s.to_hand(shrug);
        s.energy = 3;
        let i = (s.n_hand - 1) as u8;
        let after = crate::step::step(s, crate::Action::PlayCard { hand: i, target: 0 });
        assert_eq!(after.player.block, 10, "8 + 灵巧 2");
    }

    /// 没有 `draw_order_enchant`（老 trace / 没打这版补丁的 mod）时照旧按 0 算 ——
    /// 这个洞是**已知的、方向是低估**，不许因为字段缺失就去猜一个值。
    #[test]
    fn sync_without_pile_enchantments_falls_back_to_no_bonus() {
        let src = r#"{
  "version": 1,
  "run": { "act": 1, "floor": 14, "ascension": 2, "character": "铁甲战士" },
  "frames": [
    { "i": 0,
      "obs": {
        "state_type": "monster", "round": 1, "is_play_phase": true,
        "energy": 3, "max_energy": 3,
        "player": { "hp": 57, "max_hp": 80, "block": 0, "status": {} },
        "enemies": { "7": { "entity_id": "worm_0", "name": "<合成:沙包>",
                            "hp": 22, "max_hp": 22, "block": 0, "status": {}, "intents": [] } },
        "hand": [], "draw_count": 1,
        "draw": [ { "name": "耸肩无视" } ],
        "draw_order": [ "耸肩无视" ],
        "discard": [], "exhaust": [], "pending": null
      },
      "action": null,
      "settle": { "polls": 1, "ms": 0, "unstable": false, "intermediate_differs": false } }
  ]
}"#;
        let t = parse_trace(src).expect("trace 解析失败");
        let mut r = Replayer::new("");
        let mut s = r.sync(&t.frames[0].obs).state;
        let ix = s.pop_draw_top().expect("抽牌堆非空");
        assert_eq!(s.cards[ix as usize].ench, 0);
    }

    /// **真实牌序的方向**：观测下标 0 是牌堆顶，内核 `draw[]` 顶在末尾。
    ///
    /// 这条测试存在的唯一理由是那个方向写反了**不会报错** —— 内核照样跑，
    /// 只是每次"下一张抽什么"都反着，而 planner 会一路信下去。
    /// 方向由 [源码] 定死：`CardPileCmd.Draw` 取 `Cards.FirstOrDefault()`，
    /// `MoveToTopInternal` 是 `Insert(0, card)`。
    #[test]
    fn sync_puts_the_real_draw_order_top_last() {
        let src = r#"{
  "version": 1,
  "run": { "act": 1, "floor": 1, "ascension": 0, "character": "铁甲战士" },
  "frames": [
    { "i": 0,
      "obs": {
        "state_type": "monster", "round": 1, "is_play_phase": true,
        "energy": 3, "max_energy": 3,
        "player": { "hp": 70, "max_hp": 70, "block": 0, "status": {} },
        "enemies": { "7": { "entity_id": "worm_0", "name": "<合成:沙包>",
                            "hp": 22, "max_hp": 22, "block": 0, "status": {}, "intents": [] } },
        "hand": [],
        "draw_count": 3,
        "draw": [ { "name": "防御" }, { "name": "打击" }, { "name": "痛击" } ],
        "draw_order": [ "痛击", "打击", "防御" ],
        "discard": [], "exhaust": [], "pending": null
      },
      "action": null,
      "settle": { "polls": 1, "ms": 0, "unstable": false, "intermediate_differs": false } }
  ]
}"#;
        let t = parse_trace(src).expect("trace 解析失败");
        let mut r = Replayer::new("");
        let sy = r.sync(&t.frames[0].obs);
        let s = sy.state;

        assert_eq!(s.n_draw, 3);
        assert_eq!(s.n_draw_known, 3, "真实牌序拿得到时整堆都是确定的");

        // `draw_order[0]` = 痛击 = 牌堆顶 ⇒ 第一张抽到的必须是痛击
        let mut s = s;
        let top = s.pop_draw_top().expect("抽牌堆非空");
        assert_eq!(
            crate::content::card(s.cards[top as usize].id).name,
            "痛击",
            "observed draw_order[0] 是牌堆顶，第一张抽的就该是它"
        );
        let second = s.pop_draw_top().expect("还有牌");
        assert_eq!(crate::content::card(s.cards[second as usize].id).name, "打击");
        assert_eq!(s.n_draw_known, 1, "取走两张，已知前缀跟着减");
    }

    /// 没有 `draw_order`（老 trace / 没打补丁的 mod）时退回原来的行为：
    /// 内容当多重集、顺序自己洗，`n_draw_known` 是 0。
    #[test]
    fn sync_without_draw_order_keeps_the_old_unknown_order_behaviour() {
        let src = r#"{
  "version": 1,
  "run": { "act": 1, "floor": 1, "ascension": 0, "character": "铁甲战士" },
  "frames": [
    { "i": 0,
      "obs": {
        "state_type": "monster", "round": 1, "is_play_phase": true,
        "energy": 3, "max_energy": 3,
        "player": { "hp": 70, "max_hp": 70, "block": 0, "status": {} },
        "enemies": { "7": { "entity_id": "worm_0", "name": "<合成:沙包>",
                            "hp": 22, "max_hp": 22, "block": 0, "status": {}, "intents": [] } },
        "hand": [],
        "draw_count": 3,
        "draw": [ { "name": "防御" }, { "name": "打击" }, { "name": "痛击" } ],
        "discard": [], "exhaust": [], "pending": null
      },
      "action": null,
      "settle": { "polls": 1, "ms": 0, "unstable": false, "intermediate_differs": false } }
  ]
}"#;
        let t = parse_trace(src).expect("trace 解析失败");
        let mut r = Replayer::new("");
        let s = r.sync(&t.frames[0].obs).state;
        assert_eq!(s.n_draw, 3);
        assert_eq!(s.n_draw_known, 0, "顺序不可知时一张确定的都没有");
    }

    /// 一条最小 trace：满血打一张打击（6 伤害），敌人 22 HP -> 16 HP。
    fn trace_json(enemy_hp_after: i32) -> String {
        format!(
            r#"{{
  "version": 1,
  "run": {{ "act": 1, "floor": 1, "ascension": 0, "character": "铁甲战士" }},
  "frames": [
    {{ "i": 0,
       "obs": {{
         "state_type": "monster", "round": 1, "is_play_phase": true,
         "energy": 3, "max_energy": 3,
         "player": {{ "hp": 70, "max_hp": 70, "block": 0, "status": {{}} }},
         "enemies": {{ "7": {{ "entity_id": "worm_0", "name": "<合成:沙包>",
                             "hp": 22, "max_hp": 22, "block": 0, "status": {{}}, "intents": [] }} }},
         "hand": [ {{ "slot": 0, "id": "strike", "name": "打击", "cost": "1",
                     "type": "Attack", "upgraded": false, "description": "造成6点伤害。" }} ],
         "draw_count": 4, "discard": [], "exhaust": [], "pending": null
       }},
       "action": {{ "kind": "play_card", "slot": 0, "card_id": "strike",
                   "card_name": "打击", "target": "worm_0" }},
       "settle": {{ "polls": 3, "ms": 120, "unstable": false, "intermediate_differs": false }} }},
    {{ "i": 1,
       "obs": {{
         "state_type": "monster", "round": 1, "is_play_phase": true,
         "energy": 2, "max_energy": 3,
         "player": {{ "hp": 70, "max_hp": 70, "block": 0, "status": {{}} }},
         "enemies": {{ "7": {{ "entity_id": "worm_0", "name": "<合成:沙包>",
                             "hp": {enemy_hp_after}, "max_hp": 22, "block": 0, "status": {{}}, "intents": [] }} }},
         "hand": [],
         "draw_count": 4, "discard": [ {{ "name": "打击", "cost": "1" }} ],
         "exhaust": [], "pending": null
       }},
       "action": null }}
  ]
}}"#
        )
    }

    #[test]
    fn a_correct_trace_matches() {
        let t = parse_trace(&trace_json(16)).expect("trace 应当能解析");
        let r = verify(&t);
        assert!(r.ok(), "不该有 MISMATCH: {:?}", r.results);
        assert_eq!(r.count(Verdict::Match), 1);
    }

    /// 对拍器最重要的性质：**它真的会失败**。
    /// 一个永远绿的验证器比没有验证器更糟 —— 这正是重写旧 sim 的理由。
    #[test]
    fn injected_error_is_caught() {
        let t = parse_trace(&trace_json(15)).unwrap();
        let r = verify(&t);
        assert!(!r.ok(), "把敌人 HP 改错 1 点，必须报出来");
        let d = &r.results[0].diffs[0];
        assert!(d.field.contains("HP"), "报的应该是 HP 字段，实际是 {}", d.field);
        assert_eq!((d.game.as_str(), d.kernel.as_str()), ("15", "16"));
    }

    #[test]
    fn unknown_card_is_not_a_failure() {
        let src = trace_json(16).replace("打击", "还没实现的牌");
        let t = parse_trace(&src).unwrap();
        let r = verify(&t);
        assert!(r.ok(), "不认识的牌不该算失败");
        assert_eq!(r.count(Verdict::UnknownContent), 1);
        assert!(r.missing_cards.contains_key("还没实现的牌"));
    }
}
