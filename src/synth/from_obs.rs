//! **观测 -> [`FightSpec`]**：把一帧「战斗刚开局」的观测抽成合成构造器的输入。
//!
//! 这一步 2026-09-09 生在 `bin/synth_audit` 里（它是当时唯一的消费者）。
//! 阶段 2 的评估台需要**逐字同一份** —— 两边各写一遍的话，两个台子读出来的
//! 就不再是同一场仗，而"哪个台子抽错了"没有任何东西会报。
//!
//! # 它只做"翻译"，一条规则都不自己写
//!
//! 牌名 -> `CardInst` 走 [`crate::synth::card_from_name`]（那一步和
//! `replay::sync` 的 `push` 共用同一批函数）；敌人名 -> def 走
//! `replay::enemy_id`；药水走 `replay::map_potion`。
//! **认不出来的一律走那几个函数自己的回退**（未知牌 / 未知敌人），
//! 方向都是低估这副牌组。
//!
//! # 两处「第 0 帧已经不是开局那一刻」的修正，两边都要
//!
//! | 修正 | 为什么 | 谁踩过 |
//! |---|---|---|
//! | [`DECK_REWRITERS`] 整件扣下 | 风箱/骨茶/碎石者开局就把牌升了，而牌组是从**第 0 帧**抽的 —— 照喂进去构造器会**再升一次** | `synth_audit` 2026-09-09 |
//! | 开局回血倒推（[`build_start`]） | `FightSpec::hp` 的语义是「**进这场仗之前**的血量」，而第 0 帧的观测里小血瓶那 2 点已经回过了 | 同上 |
//!
//! 两条都不是台子的洁癖：**阶段 3 的整幕链喂进去的血量正是"上一场打完剩多少"**，
//! 那个数天然是"回血之前"的，和这里倒推出来的是同一个口径。

use crate::content::{is_turn_phase, relic_by_id};
use crate::replay::{Obs, Trace};
use crate::state::MAX_POTIONS;
use crate::synth::encounters::Table;
use crate::synth::{build, card_from_name, Built, DeckCard, EnemySpec, FightSpec, RelicSpec};

/// **第 0 帧的牌组已经被这几件改过**，所以抽 spec 时**整件扣下**。
///
/// 风箱/骨茶开局升级手牌、碎石者开局升级抽牌堆里两张 —— 而牌组是从
/// **第 0 帧**抽出来的，那时它们已经升过了。照喂进去构造器会在一副
/// 已经升过级的牌上**再升一次**，多出来的那几张 `+` 是台子自己造的假差。
///
/// **哪几张被升过是不可反推的**（观测只说"这张是 `+`"，不说谁升的），
/// 所以只能整件扣下并**报出来**（[`Extracted::withheld`]）。
/// 它们的规则由单测钉。
///
/// > 阶段 3 的整幕链**不受这条影响**：那边牌组来自局外账本（还没被改过的
/// > 那一份），遗物照常喂进去，由构造器自己去升。这条只对"从第 0 帧倒推"
/// > 这种用法成立。
pub const DECK_REWRITERS: &[&str] = &["BELLOWS", "BONE_TEA", "STONE_CRACKER"];

/// 从第 0 帧抽出来的一场仗。字符串**自己持有**（[`RelicSpec`] 是借的）。
pub struct Extracted {
    pub deck: Vec<DeckCard>,
    /// 牌名，和 `deck` 逐位置配对 —— 诊断输出用（内核只认 id）
    pub deck_names: Vec<String>,
    /// 喂给构造器的那几件（已经扣掉 [`DECK_REWRITERS`]）
    pub relics: Vec<(String, Option<i32>)>,
    /// 扣下的那几件，**要报出来**：它们的效果这一场里一件都不生效
    pub withheld: Vec<String>,
    pub enemies: Vec<EnemySpec>,
    pub enemy_names: Vec<String>,
    pub enemy_defs: Vec<Option<u16>>,
    pub potions: Vec<u8>,
    pub potion_slots: u8,
    pub hp: i32,
    pub max_hp: i32,
    pub ascension: u8,
    pub after_rest: bool,
    pub base_energy: i32,
    pub boss_room: bool,
}

impl Extracted {
    /// 借出遗物（已经扣掉 [`DECK_REWRITERS`]）。
    pub fn relic_specs(&self) -> Vec<RelicSpec<'_>> {
        self.relics.iter().map(|(id, c)| RelicSpec { id: id.as_str(), counter: *c }).collect()
    }

    /// 拼一份 [`FightSpec`]。`relics` 由调用方持有（生命周期）。
    ///
    /// **`start_status` 一条都不传**：遗物解释不了的 status 要原样出现在
    /// 审计台的逐字段差里，而不是被一个万能参数填平。
    pub fn fight_spec<'a>(&'a self, relics: &'a [RelicSpec<'a>], seed: u64) -> FightSpec<'a> {
        FightSpec {
            deck: &self.deck,
            relics,
            hp: self.hp,
            max_hp: self.max_hp,
            start_status: &[],
            after_rest: self.after_rest,
            boss_room: self.boss_room,
            base_energy: self.base_energy,
            enemies: &self.enemies,
            potions: &self.potions,
            potion_slots: self.potion_slots,
            ascension: self.ascension,
            // 实录里选过的诅咒是观测量，但开局第 0 帧还没选 —— 验收台一律按默认选法打
            curse_policy: crate::content::DEFAULT_CURSE_POLICY,
            seed,
        }
    }
}

/// 这一帧是不是「一场仗的起点」。不是就别拿它当测试集。
pub fn is_fight_start(o: &Obs) -> Result<(), String> {
    if o.round != 1 {
        return Err(format!("第 0 帧是第 {} 回合，不是开局", o.round));
    }
    if !o.discard.is_empty() || !o.exhaust.is_empty() {
        return Err(format!(
            "第 0 帧弃牌堆 {} 张 / 消耗堆 {} 张，不是开局",
            o.discard.len(),
            o.exhaust.len()
        ));
    }
    if o.draw_order.is_empty() && o.draw.is_empty() && o.draw_count > 0 {
        return Err("抽牌堆只报了张数（老 trace），牌组凑不齐".to_string());
    }
    Ok(())
}

/// 「这一场是不是 Boss 房」—— 从遭遇表认（`encounters.json` 的 `room_type`）。
///
/// 缩放仪那 25 点开局回血只在 Boss 房给。**认不出遭遇就是 `false`**，
/// 方向是低估自己。
pub fn probe_boss_room(table: Option<&Table>, o: &Obs) -> bool {
    let Some(t) = table else { return false };
    let defs: Vec<Option<u16>> = o.enemies.iter().map(|e| crate::replay::enemy_id(&e.name)).collect();
    if !defs.iter().all(|d| d.is_some()) {
        return false;
    }
    let defs: Vec<u16> = defs.into_iter().flatten().collect();
    let (exact, _) = t.identify(&defs);
    exact.len() == 1 && t.is_boss(&exact[0])
}

/// 观测 -> spec。
///
/// **牌组 = 手牌 + 抽牌堆**：第 0 帧弃牌堆和消耗堆都是空的，所以这两堆合起来
/// 就是这场仗开局的整副牌。（mod 2026-09-08 起直接报 `deck`，但录制器的
/// `normalize()` 是白名单式的，语料里一条都没有 —— 见 docs/sts2mcp-patches.md。）
pub fn extract(t: &Trace, o: &Obs, boss_room: bool) -> Extracted {
    let mut deck = Vec::new();
    let mut deck_names = Vec::new();
    for c in &o.hand {
        deck.push(card_from_name(&c.name, c.upgraded, &c.enchant_id, c.enchant_amount));
        deck_names.push(c.name.clone());
    }
    if !o.draw_order.is_empty() {
        for (i, name) in o.draw_order.iter().enumerate() {
            let (eid, amt) =
                o.draw_order_enchant.get(i).map(|(a, b)| (a.as_str(), *b)).unwrap_or(("", 0));
            deck.push(card_from_name(name, name.ends_with('+'), eid, amt));
            deck_names.push(name.clone());
        }
    } else {
        for name in &o.draw {
            deck.push(card_from_name(name, name.ends_with('+'), "", 0));
            deck_names.push(name.clone());
        }
    }

    // 遗物的计数器：**照抄 `sync` 的口径** —— 回合相位要把这一场已经走过的
    // 回合减回去（[源码] 摆动球是 `AfterPlayerTurnStart` 里加的），
    // 其余原样。两边不一致的话审计台自己就会把它比出来。
    let mut relics = Vec::new();
    let mut withheld = Vec::new();
    for r in &o.relics {
        if DECK_REWRITERS.contains(&r.id.as_str()) {
            withheld.push(r.id.clone());
            continue;
        }
        let counter = r.counter.map(|raw| match relic_by_id(&r.id).and_then(|def| def.counter_to) {
            Some(st) if is_turn_phase(st) => raw - o.round,
            _ => raw,
        });
        relics.push((r.id.clone(), counter));
    }

    let mut enemies = Vec::new();
    let mut enemy_names = Vec::new();
    let mut enemy_defs = Vec::new();
    for e in &o.enemies {
        let def = crate::replay::enemy_id(&e.name);
        enemy_names.push(e.name.clone());
        enemy_defs.push(def);
        // 血量用观测到的 **max_hp**：区间对不对是另一栏的事，掺进逐字段比
        // 只会让每一只敌人都红。**取 max 而不是 hp** —— 开局那一刻血量就该是
        // 满的，而第 0 帧的 `hp` 可能已经掉过（抱抱先生回合开始对全体 1 点）。
        // 两者的差因此是一条**有信息的**读数：它正是"开局那几个钩子打了多少"。
        enemies.push(EnemySpec::with_hp(def.unwrap_or(crate::content::enemy::UNKNOWN), e.max_hp));
    }

    let mut potions = vec![0u8; MAX_POTIONS];
    for p in &o.potions {
        if p.slot < MAX_POTIONS {
            let key = if p.id.is_empty() { &p.name } else { &p.id };
            potions[p.slot] = crate::replay::map_potion(key);
        }
    }
    let potion_slots = o
        .max_potion_slots
        .unwrap_or_else(|| o.potions.iter().map(|p| p.slot + 1).max().unwrap_or(3).max(3))
        .min(MAX_POTIONS) as u8;

    // **「上一场是不是休息处」只能从观测反推**：trace 里没有房间历史
    // （`run.room_type` 是**这一间**的类型，不是上一间）。唯一的证据是
    // 第 0 帧的能量比上限高 —— 那正是古茶具兑现过的样子。
    //
    // **所以这一栏验不了触发条件，只验数值和归属**：反推说"武装了"，
    // 而内核按 `REST_ARMED` 给 +2 / 假货 +1 —— 给错了数或者给错了遗物照样红。
    // 真要验触发条件，得让 mod 报遗物的 `RelicStatus.Active`（第 5 个补丁）。
    let after_rest = o.energy > o.max_energy;

    Extracted {
        deck,
        deck_names,
        relics,
        withheld,
        enemies,
        enemy_names,
        enemy_defs,
        potions,
        potion_slots,
        hp: o.hp,
        max_hp: o.max_hp,
        ascension: t.ascension,
        after_rest,
        // 能量上限是**观测量**（`player.max_energy`）。L3 那边它来自局外账本，
        // 这里照抄 —— 内核没有"这一局的能量上限"这个概念，`solver::BASE_ENERGY`
        // 只是个默认值。
        base_energy: o.max_energy,
        boss_room,
    }
}

/// 构造这场仗，**并把开局回血倒推掉**。
///
/// `FightSpec::hp` 的语义是「**进这场仗之前**的血量」，而第 0 帧的观测是
/// 「开局那几个钩子跑完之后」的血量 —— 小血瓶的 2 点、缩放仪的 25 点
/// 已经在里面了。照观测直接喂进去，构造器会**再回一次**。
///
/// 做法：先跑一遍量出它回了多少，再把那个数从输入里减掉。
/// **这一步让"回几点"这件事在本函数上变得不可证伪**（减多少就加回多少），
/// 所以那个数值由 [源码] 和单测钉；台子验的是**其余每一格**不因此漂掉。
/// 血量顶到上限时 `healed` 自然是 0，不需要特判。
pub fn build_start(ex: &Extracted, seed: u64) -> Built {
    let relics = ex.relic_specs();
    let spec = ex.fight_spec(&relics, seed);
    let probe = build(&spec);
    let healed = probe.state.player.hp - ex.hp;
    if healed <= 0 {
        return probe;
    }
    build(&FightSpec { hp: ex.hp - healed, ..spec })
}

/// 倒推之后的「进这场仗之前的血量」—— [`build_start`] 用的就是它。
///
/// 单独露出来，是因为**阶段 2 的评估台要的是 spec 不是局面**：它自己会
/// 逐样本换种子重搭（`synth::eval::evaluate`），拿不到 `build_start` 搭的那一个。
///
/// 回几点血**不依赖种子**（小血瓶 2 / 缩放仪 25 都是定值），所以拿哪个种子
/// 探都一样 —— 但仍然照调用方给的那个探，免得下一件"按随机回血"的遗物
/// 进表时这里静默地对不上。
pub fn corrected_hp(ex: &Extracted, seed: u64) -> i32 {
    let relics = ex.relic_specs();
    let probe = build(&ex.fight_spec(&relics, seed));
    let healed = probe.state.player.hp - ex.hp;
    ex.hp - healed.max(0)
}
