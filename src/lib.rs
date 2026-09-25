//! sts2core — L1 kernel.
//!
//! Public surface is deliberately tiny:
//!   step(State, Action) -> State
//!   legal_actions(&State) -> [Action]
//!   begin_combat(State) -> State
//!
//! L2 (in-combat solver) and L3 (deck advisor) are built on top of these and
//! own no game rules themselves.

pub mod asc;
pub mod content;
pub mod damage;
pub mod json;
pub mod ops;
pub mod plan;
pub mod replay;
pub mod rollout;
pub mod solver;
pub mod state;
pub mod step;
/// L3 的地基：合成战斗构造器（`(牌组, 遗物, 血量, 遭遇, 种子) -> State`）
pub mod synth;

#[cfg(test)]
mod test_subject_tests;

pub use state::{CardInst, Entity, Pending, Rng, St, State, F_CORRUPT, F_UPGRADED};
pub use step::{
    begin_combat, end_turn_with_incoming, end_turn_with_live_incoming, legal_actions, step, Action,
    Incoming,
};

#[cfg(test)]
mod tests {
    use super::content::*;
    use super::damage::*;
    use super::rollout;
    use super::state::*;
    use super::step::*;

    fn base_state() -> State {
        let mut s = State::new(80, 12345);
        s.add_enemy(enemy::DUMMY, 100);
        s
    }

    /// 回归测试：敌人挡下的格挡必须活过我的整个回合。
    ///
    /// `start_player_turn` 曾经把敌人格挡一起清零，等于让所有 Defend 意图失效。
    /// 对 L2 求解器来说这是致命的 —— 它会以为自己的攻击不会被挡下。
    /// 实测（2026-08-15 第1幕第7层）：小啃兽 `Attack:6, Defend:` 之后，
    /// 到我的回合开始时它仍带着 5 点格挡。
    #[test]
    fn enemy_block_survives_into_my_turn() {
        let mut s = State::new(80, 7);
        s.add_enemy(enemy::NIBBIT, 44);
        for _ in 0..10 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        // 小啃兽的第 1 手才是「啃咬并戒备」（6 点伤害 + 5 格挡）；
        // 第 0 手是纯撞击。出招表按 wiki 补全之后顺序变了，这里跟着指过去。
        s.enemy_move[0] = 1;
        let s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[0].block, 5, "敌人的格挡不该在我的回合开始时被清掉");

        // 打击 6 点：先被 5 点格挡吃掉，只有 1 点进 HP
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.enemies[0].block, 0);
        assert_eq!(s.enemies[0].hp, 43);
    }

    /// 手牌顺序由洗牌决定，测试不该依赖它 —— 按卡 id 找位置。
    fn hand_ix_of(s: &State, id: u16) -> u8 {
        (0..s.n_hand)
            .find(|&i| s.cards[s.hand[i as usize] as usize].id == id)
            .expect("手牌里没有这张牌")
    }

    /// 缓慢按**牌**累加，且力量加在缓慢**之前**。
    ///
    /// 这一整段是 2026-08-15 第1幕第13层旧日雕像那一回合的逐张实录：
    ///   预备打击(打 7，SLOW 0→10) 防御(→20) 防御(→30) 防御(→40) 打击(打 11，→50)
    ///
    /// 最后那个 11 是三选一的判据，换任何一条规则都会给出别的数字：
    ///   (6+2)×1.4 = 11.2 → 11   ← 实测
    ///   (6×1.4=8)+2      = 10   力量若加在缓慢之后
    ///   (6+2)×1.5        = 12   缓慢的计数若把当前这张牌也算进去
    /// 顺带钉住：防御这种**技能牌也计数**（否则打击只有 ×1.1）。
    #[test]
    fn slow_ramps_per_card_and_strength_applies_before_it() {
        let mut s = State::new(80, 99);
        s.add_enemy(enemy::BYGONE_EFFIGY, 127);
        s.add_card(card::SETUP_STRIKE, 0, 0);
        s.add_card(card::DEFEND, 0, 0);
        s.add_card(card::DEFEND, 0, 0);
        s.add_card(card::DEFEND, 0, 0);
        s.add_card(card::STRIKE, 0, 0);
        let mut s = begin_combat(s);
        s.energy = 5; // 实战里这一格能量是能量药水给的
        assert_eq!(s.enemies[0].get(St::Slow), 0, "缓慢回合开始时是 0，不是「没有」");

        let i = hand_ix_of(&s, card::SETUP_STRIKE);
        s = step(s, Action::PlayCard { hand: i, target: 0 });
        assert_eq!(s.enemies[0].hp, 120, "缓慢为 0 时预备打击打 7");
        assert_eq!(s.player.get(St::Strength), 2);
        assert_eq!(s.enemies[0].get(St::Slow), 10);

        for expect in [20, 30, 40] {
            let i = hand_ix_of(&s, card::DEFEND);
            s = step(s, Action::PlayCard { hand: i, target: 0 });
            assert_eq!(s.enemies[0].get(St::Slow), expect, "技能牌也给缓慢计数");
        }
        assert_eq!(s.enemies[0].hp, 120, "防御不该造成伤害");

        let i = hand_ix_of(&s, card::STRIKE);
        s = step(s, Action::PlayCard { hand: i, target: 0 });
        assert_eq!(s.enemies[0].hp, 109, "(6+2)×1.4 = 11.2 → 11");
        assert_eq!(s.enemies[0].get(St::Slow), 50);
    }

    /// 「在本回合内获得 2 点力量」到回合结束必须收回去。
    ///
    /// 游戏是真的加力量再挂一个标记 power（实测同时出现 `STRENGTH_POWER=2`
    /// 和 `SETUP_STRIKE_POWER=2`），内核照抄这个建模，所以这里两个都要归零。
    #[test]
    fn setup_strike_strength_expires_at_end_of_turn() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::DUMMY, 100);
        for _ in 0..6 {
            s.add_card(card::SETUP_STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.player.get(St::Strength), 2);
        assert_eq!(s.player.get(St::TempStrength), 2);

        s = step(s, Action::EndTurn);
        assert_eq!(s.player.get(St::Strength), 0, "本回合限定的力量必须在回合结束时收回");
        assert_eq!(s.player.get(St::TempStrength), 0);
    }

    // ---------------- 触发器（能力牌作为数据）----------------

    /// **狂乱逃离每打出一次，这一张实例的费用永久 +1。**
    ///
    /// [源码] `FranticEscape.OnPlay` 末尾的 `base.EnergyCost.AddThisCombat(1)` ——
    /// `base` 是**这一张卡实例**，所以 6 张各自第一次都只要 1 费。
    /// 实战正是靠这条连买了三个回合；我一开始按"打一次全部涨价"估过，
    /// 那会得出"买不起时间"的错结论。
    ///
    /// **这条规则没有实录钉着**：那一场我每次打的都是没打过的新副本，
    /// 从没打出过涨价后的那一张，所以对拍语料分不开"涨价"和"不涨价"。
    /// 在拿到那样一帧之前，守它的只有这个测试。
    #[test]
    fn frantic_escape_makes_only_its_own_copy_cost_more() {
        let mut s = hand_of(1, 100, &[card::FRANTIC_ESCAPE, card::FRANTIC_ESCAPE], 3);
        let a = card::FRANTIC_ESCAPE;
        assert_eq!(effective_cost(&s, 0), 1, "第一张：卡面 1 费");
        assert_eq!(effective_cost(&s, 1), 1, "另一张也是 1 费");

        // 打掉第 0 张
        s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.energy, 2, "扣的是**旧**费用 1，涨价从下一次开始");

        // 手上剩的那张**没有**跟着涨价 —— 涨的是实例不是牌名
        assert_eq!(effective_cost(&s, 0), 1, "另一张实例不受影响");
        assert_eq!(super::content::card(a).cost, 1, "内容表里的基础费用没被改");

        // 把打出去那张捞回手牌，它现在要 2 费
        let played = s.disc[0];
        s.hand[s.n_hand as usize] = played;
        s.n_hand += 1;
        let ix = s.n_hand as usize - 1;
        assert_eq!(s.cards[played as usize].cost_delta, 1, "增量记在这一张实例上");
        assert_eq!(effective_cost(&s, ix), 2, "打过一次的那张要 2 费");
    }

    /// **抽牌只会抽到抽牌堆里的牌。**
    ///
    /// 这条看着是废话，但它是"求解器为什么会建议一张我牌组里没有的牌"
    /// 这个问题的**一半答案**，而 2026-08-22 玩家真的问了这个问题。
    /// 另一半是**生成**（添柴 / 地狱之刃 / 惊逃）—— 那条路**确实**会造出
    /// 牌组里根本没有的牌，因为卡面写的就是"将 1 张随机牌加入你的手牌"，
    /// 随机牌来自**角色牌池**（`GEN_POOL`）而不是你的牌组。
    ///
    /// 两条路必须分开看，因为可信度完全不同：
    ///
    /// | | 内容 | 顺序 | 求解器的线有多可信 |
    /// |---|---|---|---|
    /// | 抽牌 | **观测量**（`player.draw_pile`）| 不可知，内核重洗 | 牌是真的，只是不一定这时候到手 |
    /// | 生成 | **内核凭空造的**，池子还只是真实池子的子集 | —— | 那张牌**不存在**，别照着打 |
    ///
    /// 所以 `solve --live` 现在把生成的牌单独标出来，不和抽牌混在一句警告里。
    #[test]
    fn drawing_only_ever_yields_cards_that_were_in_the_draw_pile() {
        use std::collections::BTreeSet;
        // 抽牌堆里只放两种牌，任何"凭空多出来的牌"都会一眼看出来
        let mut s = hand_of(3, 100, &[card::POMMEL_STRIKE], 1);
        let mut in_draw: BTreeSet<u16> = BTreeSet::new();
        for id in [card::DEFEND, card::BLOOD_WALL, card::DEFEND, card::BLOOD_WALL] {
            s.add_card(id, 0, 0);
            s.draw[s.n_draw as usize] = s.n_cards - 1;
            s.n_draw += 1;
            in_draw.insert(id);
        }
        let n_before = s.n_cards;

        // 剑柄打击（抽 2 张）
        let a = step(s, Action::PlayCard { hand: 0, target: 0 });

        for i in 0..a.n_hand as usize {
            let ix = a.hand[i] as usize;
            assert!(
                ix < n_before as usize,
                "抽牌不该造出新的卡实例：槽 {i} 的下标 {ix} 越过了出牌前的 {n_before}"
            );
            assert!(
                in_draw.contains(&a.cards[ix].id),
                "抽到了抽牌堆里没有的牌：{}",
                super::content::card(a.cards[ix].id).name
            );
        }
    }

    /// **敌人名字是连接键，所以合成假人不许占用真名字。**
    ///
    /// `enemy_id()` 靠中文名把观测连到内容表。一个只为测试存在的假人如果
    /// 叫了真敌人的名字，真遇到那只怪时内核会**静默地**把它当成假人 ——
    /// 不报错，只是安安静静地按错的血量和错的招算。
    ///
    /// 2026-08-22 就是这么被咬的：id 1 那个 22 血单招的「难以杀灭 9 测试用
    /// 敌人」名字就叫「外骨骼虫」，第2幕第20层真遇到三只外骨骼虫时，
    /// `--predict-enemy` 报「对不齐」、`solve --live` 报「没认出」。
    /// 同一次还查出 id 2 叫「实验体#C8」—— **那是第 3 幕 Boss 的名字**。
    ///
    /// 规矩：合成条目一律用 `<...>` 包起来，游戏里不会有这种名字。
    /// 顺带守住名字唯一 —— 重名同样让 `enemy_id` 取到第一个，一样是静默错。
    #[test]
    fn enemy_names_are_unique_and_no_synthetic_squats_on_a_real_name() {
        use super::content::{enemy, ENEMIES};
        let synthetic = [enemy::DUMMY, enemy::EXOSKELETON, enemy::TEST_SUBJECT, enemy::UNKNOWN];
        for id in synthetic {
            let n = ENEMIES[id as usize].name;
            assert!(
                n.starts_with('<') && n.ends_with('>'),
                "合成/占位敌人 {id} 叫「{n}」—— 必须用 <...> 包起来，否则会和真敌人重名"
            );
        }
        for (i, a) in ENEMIES.iter().enumerate() {
            for (j, b) in ENEMIES.iter().enumerate().skip(i + 1) {
                assert_ne!(a.name, b.name, "敌人重名：{i} 和 {j} 都叫「{}」", a.name);
            }
        }
    }

    /// 状态/诅咒/任务牌的**可打出性必须逐张登记**，不许再靠"有没有 ops"推。
    ///
    /// 那条启发式活了很久，因为当时表里所有零效果的状态牌恰好都真的不可打出
    /// —— 又一次"和所有已知数据一致不等于对"。孢子心灵是分得开两种解的那个
    /// 样本：Curse、零效果、却真能花 1 费打出来消耗掉（[源码] 只带 `Exhaust`
    /// 不带 `Unplayable`，第2幕第19层实录帧9 逐帧证实）。
    ///
    /// 这个测试的价值在**计数**那一行：新加一张状态/诅咒/任务牌而不在这里
    /// 表态，它就会红。不加的话，漏登记的表现是 `legal_actions` 悄悄多出
    /// 或少掉一个动作，没有任何东西会报。
    #[test]
    fn every_status_curse_quest_card_declares_whether_it_can_be_played() {
        use super::content::{playable, CARDS};
        use super::ops::Kind;
        // 判据一律是 [源码] `CardKeyword.Unplayable`（31 张带这个关键字）。
        let expected: &[(&str, bool)] = &[
            ("伤口", false),
            ("<未知牌>", false), // 占位：内核不认识的牌保守当作不可打出
            ("黏液", true),      // Slimed 不带 Unplayable，1 费真能打
            ("灼伤", false),
            ("感染", false),
            ("腐朽", false),
            ("羞耻", false),
            ("瓦解", false),
            ("晕眩", false),
            ("笨拙", false),
            ("孢子心灵", true), // SporeMind：只带 Exhaust，零效果但可打出
            ("藏宝图", false),  // SpoilsMap：带 Unplayable
            // FranticEscape：[源码] 没有 Unplayable 关键字，1 费真的能打出来
            //（实战靠它买了三个回合）。它还是唯一会改自己费用的牌，
            // 见 `CardInst::cost_delta`。
            ("狂乱逃离", true),
            // Wither：[源码] 带 `CardKeyword.Unplayable`。第 3 幕 Boss 永世沙漏塞的。
            ("凋萎", false),
            // ---- 2026-08-30 ----
            // Toxic：**可以打出**。它只带 `Exhaust`、不带 `Unplayable`，
            // 所以花 1 费打出去就是把它消耗掉 —— 和孢子心灵同一个形状，
            // 也正是这个测试当初被逼出来的那一课。
            ("毒素", true),
            // Greed：[源码] 带 `Eternal | Unplayable`
            ("贪婪", false),
            // Debt：[源码] 带 `Unplayable`
            ("债务", false),
            // 呼唤（灵魂异鱼塞的）：[源码] `Beckon()` 是 `base(1, Status, …)`
            // 且**不带 `Unplayable`** —— 花 1 费打出去什么都不发生，
            // 但那正是它的用处：从手里挪走就不用在回合末挨那 6 点。
            ("呼唤", true),
        ];
        let actual: Vec<(&str, bool)> = CARDS
            .iter()
            .enumerate()
            .filter(|(_, d)| matches!(d.kind, Kind::Status | Kind::Curse | Kind::Quest))
            .map(|(i, d)| (d.name, playable(i as u16)))
            .collect();
        assert_eq!(
            actual.len(),
            expected.len(),
            "新增了状态/诅咒/任务牌却没在这个测试里登记可打出性：{actual:?}"
        );
        for (want, got) in expected.iter().zip(actual.iter()) {
            assert_eq!(want, got, "可打出性登记对不上：期望 {want:?}，实际 {got:?}");
        }
    }

    /// 触发器表的完整性。这是"加一张能力牌 = 加一行表"这句话的守门人：
    /// 能力牌挂上去的 status **必须**在 `POWERS` 里有人认领，否则那张牌是个
    /// 哑弹 —— 打出去有 status、永远不触发，而且没有任何测试会失败。
    ///
    /// 允许**同一个 status 有多条规则**，只要钩子不同（覆甲：回合结束给格挡 +
    /// 回合开始掉 1 层）。禁止的是同一个 `(st, hook)` 出现两次。
    #[test]
    fn every_power_card_has_a_rule_and_no_rule_is_claimed_twice() {
        use super::ops::*;
        for (i, a) in POWERS.iter().enumerate() {
            for b in &POWERS[i + 1..] {
                assert!(
                    !(a.st == b.st && a.hook == b.hook),
                    "{:?} 在 {:?} 上有两条规则",
                    a.st,
                    a.hook
                );
            }
        }
        // 这些 status 本身就有意义，不需要触发器（燃烧给的就是普通力量）
        const PLAIN: [St; 8] = [
            St::Strength,
            St::Dexterity,
            St::Vulnerable,
            St::Weak,
            St::Frail,
            St::Artifact,
            St::Intangible,
            St::DamageCap,
        ];
        for def in CARDS {
            if !matches!(def.kind, Kind::Power) {
                continue;
            }
            for ops in [def.ops, def.ops_upg] {
                for op in ops {
                    if let Op::Status { st, .. } = op {
                        assert!(
                            PLAIN.contains(st)
                                || POWERS.iter().any(|p| p.st == *st)
                                || RULE_MODIFIERS.contains(st)
                                || MARKER_STATUSES.contains(st),
                            "能力牌「{}」挂的 {:?} 四个类别都不属于（普通 status /\
                             POWERS 触发规则 / RULE_MODIFIERS 规则修饰 / \
                             MARKER_STATUSES 纯标记）—— 是张哑弹",
                            def.name,
                            st
                        );
                    }
                }
            }
        }
    }

    /// 上一条只问「**能力牌**挂的 status 有没有规则」，
    /// 于是**遗物和敌人挂的 status 从这个口子漏出去了**。
    ///
    /// 2026-08-29 漏出去一颗：`St::Thorns`。铜质鳞片（3 层）和多刺蟾蜍（5 层）
    /// 都在挂它，而内核里没有任何规则读它 —— `grep St::Thorns` 只有枚举声明。
    /// 它是追第 2 幕 Boss 的一帧时**偶然**发现的，没有任何测试会发现它。
    ///
    /// 所以这条把口径从「能力牌」放宽到「**任何被挂上的 status**」：
    /// `RelicDef` 的两栏、`EnemyDef::start_status`、以及敌人招式里的
    /// `EOp::SelfStatus` / `EOp::Status`。四个类别之外的一律算哑弹。
    #[test]
    fn no_status_handed_out_by_a_relic_or_an_enemy_is_a_dud() {
        use super::ops::*;
        const PLAIN: [St; 8] = [
            St::Strength,
            St::Dexterity,
            St::Vulnerable,
            St::Weak,
            St::Frail,
            St::Artifact,
            St::Intangible,
            St::DamageCap,
        ];
        // 一个 status 算「有人认领」有四条路。第二条要**递归走进规则体** ——
        // 光看 `p.st` 会漏掉只被**引用**而没有自己规则的那些：
        // 摆动球的相位 `PendulumPhase` 就只出现在 `TCond::EveryNTurns { phase }` 里。
        fn refs_in_ops(ops: &[TOp], out: &mut Vec<St>) {
            for op in ops {
                match op {
                    TOp::OwnerStatus { st, .. }
                    | TOp::PlayerStatus { st, .. }
                    | TOp::AllEnemiesStatus { st, .. }
                    | TOp::OwnerClearStatus(st)
                    | TOp::PlayerClearStatus(st) => out.push(*st),
                    TOp::If { cond, then } => {
                        match cond {
                            TCond::EveryNTurns { phase, .. } => out.push(*phase),
                            TCond::OwnerHas(st) => out.push(*st),
                            // 双截棍 / 铁棒的跨战斗计数器只被这个条件读
                            TCond::CounterMultipleOf { st, .. } => out.push(*st),
                            _ => {}
                        }
                        refs_in_ops(then, out);
                    }
                    _ => {}
                }
            }
        }
        let mut referenced: Vec<St> = Vec::new();
        for p in POWERS {
            referenced.push(p.st);
            refs_in_ops(p.ops, &mut referenced);
        }
        // **敌人招式里按层数缩放的那两个 op，它们的 `per` 就是消费点。**
        // 不扫这一层的话，「连环爪击的段数计数器」这种只被自己那一手读的
        // status 会被判成哑弹 —— 而它有真消费点，只是不在 `POWERS` 里。
        for e in ENEMIES {
            for m in e.moves {
                for op in m.ops {
                    match op {
                        EOp::SelfStatusPerStack { per, .. }
                        | EOp::AttackPlusStackHits { per, .. }
                        | EOp::AttackPlusSelfStatus { per, .. } => referenced.push(*per),
                        _ => {}
                    }
                }
            }
        }
        let claimed = |st: &St| {
            PLAIN.contains(st)
                || referenced.contains(st)
                || RULE_MODIFIERS.contains(st)
                || PIPELINE_STATUSES.contains(st)
                || MARKER_STATUSES.contains(st)
                || KNOWN_UNMODELLED.contains(st)
        };

        for r in RELICS {
            for (st, _) in r.start_status.iter().chain(r.private_status.iter()) {
                assert!(claimed(st), "遗物「{}」挂的 {:?} 没有任何消费点 —— 哑弹", r.name, st);
            }
            if let Some(st) = r.counter_to {
                assert!(claimed(&st), "遗物「{}」的计数器 {:?} 没有任何消费点", r.name, st);
            }
        }

        for e in ENEMIES {
            for (st, _) in e.start_status {
                assert!(claimed(st), "敌人「{}」开局挂的 {:?} 没有任何消费点 —— 哑弹", e.name, st);
            }
            for m in e.moves {
                for op in m.ops {
                    let st = match op {
                        EOp::SelfStatus { st, .. } => st,
                        EOp::PlayerStatus { st, .. } => st,
                        _ => continue,
                    };
                    assert!(
                        claimed(st),
                        "敌人「{}」的招式「{}」挂的 {:?} 没有任何消费点 —— 哑弹",
                        e.name,
                        m.name,
                        st
                    );
                }
            }
        }
    }


    /// 撕裂：失去生命触发，且**在同一张牌的伤害之前**到手。
    ///
    /// 御血术是 `LoseHp(2)` 然后 `Damage(15)`。撕裂给的 1 点力量必须赶得上
    /// 这一发，打出 16 而不是 15 —— 这条同时钉住了 op 的结算顺序。
    #[test]
    fn rupture_fires_in_time_for_the_same_card_damage() {
        let mut s = State::new(80, 5);
        s.add_enemy(enemy::DUMMY, 100);
        for _ in 0..6 {
            s.add_card(card::HEMOKINESIS, 0, 0);
        }
        let mut s = begin_combat(s);
        s.player.set(St::Rupture, 1);
        s.energy = 3;
        let hp0 = s.enemies[0].hp;
        s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.player.get(St::Strength), 1, "失去生命应触发撕裂");
        assert_eq!(s.player.hp, 78);
        assert_eq!(hp0 - s.enemies[0].hp, 16, "15 基础 + 1 力量，力量要赶在伤害之前");
    }

    /// 无惧疼痛：**每一次**消耗都触发，包括恶魔之焰把自己消耗掉的那次。
    ///
    /// 这条守的是 `exhaust_card` 这个收口 —— 内核里有三条进消耗堆的路径
    /// （卡面自带消耗、恶魔之焰清手牌、烙印选牌），漏掉任何一条都会少算格挡。
    #[test]
    fn feel_no_pain_fires_on_every_exhaust_path() {
        let mut s = base_state();
        let flame = s.add_card(card::DEMON_FLAME, 0, 0);
        for _ in 0..4 {
            s.add_card(card::STRIKE, 0, 0);
        }
        s = begin_combat(s);
        s.n_hand = 5;
        s.hand[0] = flame;
        for k in 1..5u8 {
            s.hand[k as usize] = k;
        }
        s.energy = 3;
        s.player.set(St::FeelNoPain, 3);
        let hp0 = s.enemies[0].hp;
        s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(hp0 - s.enemies[0].hp, 28, "4 张手牌 × 7");
        assert_eq!(s.player.block, 15, "4 张手牌 + 恶魔之焰自己 = 5 次消耗 × 3");
    }

    /// 黑暗之拥 + 恶魔之焰：消耗触发抽牌，但**抽进来的牌不会被同一张恶魔之焰烧掉**。
    ///
    /// 这条是玩家指出来的实际行为，我一开始写成了 `while s.n_hand > 0` 的循环，
    /// 那会把新抽的牌接着烧掉、伤害也跟着虚高。正确做法是**先给手牌拍快照**。
    #[test]
    fn fiend_fire_does_not_burn_cards_drawn_by_dark_embrace() {
        let mut s = base_state();
        let flame = s.add_card(card::DEMON_FLAME, 0, 0);
        for _ in 0..9 {
            s.add_card(card::STRIKE, 0, 0);
        }
        s = begin_combat(s);
        s.n_hand = 5;
        s.hand[0] = flame;
        for k in 1..5u8 {
            s.hand[k as usize] = k;
        }
        s.energy = 3;
        s.player.set(St::DarkEmbrace, 1);
        s.player.set(St::FeelNoPain, 1);
        let hp0 = s.enemies[0].hp;
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });

        assert_eq!(hp0 - s.enemies[0].hp, 28, "按快照的 4 张算，不是越烧越多");
        // 4 张手牌 + 恶魔之焰自己 = 5 次消耗
        assert_eq!(s.n_exh, 5, "只该消耗快照里的 4 张 + 自己");
        assert_eq!(s.player.block, 5, "无惧疼痛 1 × 5 次消耗");
        assert_eq!(s.n_hand, 5, "黑暗之拥抽的 5 张全都留在手里");
    }

    /// 绯红披风 -> 撕裂：**钩子必须能套钩子**。这是放血流的核心组合，
    /// 不接上等于把一整套构筑算废。
    ///
    /// 我最初为了躲死循环写成"钩子不套钩子"，是错的；正确做法是照常触发，
    /// 用 `MAX_HOOK_DEPTH` 兜底。
    #[test]
    fn crimson_mantle_wakes_up_rupture() {
        let mut s = State::new(80, 23);
        s.add_enemy(enemy::DUMMY, 100);
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.player.set(St::CrimsonMantle, 8);
        s.player.set(St::Rupture, 1);

        s = step(s, Action::EndTurn);
        assert_eq!(s.player.get(St::Strength), 1, "回合开始自伤 1 点应唤醒撕裂");
        s = step(s, Action::EndTurn);
        assert_eq!(s.player.get(St::Strength), 2, "每回合各触发一次");
    }

    /// 凶恶：**每个吃到易伤的敌人各触发一次**，不是整张牌一次。
    /// 闪电霹雳打 3 个敌人 => 抽 3 张。
    #[test]
    fn vicious_fires_once_per_enemy_that_got_vulnerable() {
        let mut s = State::new(80, 29);
        for _ in 0..3 {
            s.add_enemy(enemy::DUMMY, 100);
        }
        let lightning = s.add_card(card::LIGHTNING, 0, 0);
        for _ in 0..12 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = lightning;
        s.energy = 3;
        s.player.set(St::Vicious, 1);

        s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.n_hand, 3, "3 个敌人吃到易伤 => 抽 3 张");
        for e in 0..3 {
            assert_eq!(s.enemies[e].get(St::Vulnerable), 1);
        }
    }

    /// 人工制品挡掉的易伤**不算"给出去了"**，凶恶不该为它抽牌。
    #[test]
    fn vicious_does_not_fire_when_artifact_eats_the_vulnerable() {
        let mut s = State::new(80, 31);
        s.add_enemy(enemy::DUMMY, 100);
        let bash = s.add_card(card::BASH, 0, 0);
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = bash;
        s.energy = 3;
        s.player.set(St::Vicious, 1);
        s.enemies[0].set(St::Artifact, 1);

        s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.enemies[0].get(St::Vulnerable), 0, "被人工制品吃掉");
        assert_eq!(s.n_hand, 0, "没真的给出易伤，凶恶不触发");
    }

    /// 滚石：回合开始打全体，然后**把自己的层数改大**。
    /// `TOp::GrowSelf` 是唯一一个会自我修改的触发效果。
    #[test]
    fn rolling_boulder_grows_itself_each_turn() {
        let mut s = State::new(80, 11);
        s.add_enemy(enemy::DUMMY, 100);
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.player.set(St::RollingBoulder, 5);

        let hp0 = s.enemies[0].hp;
        s = step(s, Action::EndTurn);
        assert_eq!(hp0 - s.enemies[0].hp, 5, "第一次 5 点");
        assert_eq!(s.player.get(St::RollingBoulder), 10, "打完把自己 +5");

        let hp1 = s.enemies[0].hp;
        s = step(s, Action::EndTurn);
        assert_eq!(hp1 - s.enemies[0].hp, 10, "第二次 10 点");
        assert_eq!(s.player.get(St::RollingBoulder), 15);
    }

    /// 绯红披风：一条规则里混用 `Amt::Fixed`（掉 1 点血）和 `Amt::Stacks`
    /// （8 点格挡）—— 这就是 `Amt` 存在的理由。
    ///
    /// 顺带钉住：**能力牌给的格挡不吃脆弱**。卡面原文是"从**卡牌**中获得的
    /// 格挡值减少25%"，能力触发的格挡不是从卡牌来的。**这条未实测。**
    /// 撕裂的联动见 [`crimson_mantle_wakes_up_rupture`]。
    #[test]
    fn crimson_mantle_mixes_fixed_and_stack_amounts() {
        let mut s = State::new(80, 13);
        s.add_enemy(enemy::DUMMY, 100); // 每回合打 12
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.player.set(St::CrimsonMantle, 8);
        s.player.set(St::Frail, 3);

        s = step(s, Action::EndTurn);
        assert_eq!(s.player.block, 8, "能力牌的格挡不该被脆弱打折（若打折是 6）");
        assert_eq!(s.player.hp, 80 - 12 - 1, "敌人 12 点 + 回合开始自伤 1 点");
    }

    /// 薪火之源改的是**能量上限**，不是"回合开始给 1 点"。
    ///
    /// 卡面写的是后者，我一开始也是按后者建的 `TurnStart` 钩子 —— 实测
    /// （2026-08-15 第1幕 Boss）打出的瞬间显示就从 `3/3` 变 `1/4`，
    /// 上限当场变了。按钩子建会和 `sync` 拿到的 `max_energy` **重复计数**，
    /// 对拍报出 `能量 游戏=4 内核=5`。
    ///
    /// 顺带守住另一条同一帧抓到的：**能力牌打出后离场，不进弃牌堆**。
    #[test]
    fn pyre_raises_max_energy_and_leaves_play() {
        let mut s = State::new(80, 17);
        s.add_enemy(enemy::DUMMY, 100);
        let pyre = s.add_card(card::PYRE, 0, 0);
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = pyre;
        s.energy = 3;
        let base = s.base_energy;

        s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.base_energy, base + 1, "上限当场 +1");
        assert_eq!(s.energy, 1, "当前能量只扣牌费，不当场回能");
        assert_eq!(s.n_disc, 0, "能力牌打出后离场，不进弃牌堆");
        assert_eq!(s.n_exh, 0, "也不能图省事丢进消耗堆，那会误触发无惧疼痛/黑暗之拥");

        s = step(s, Action::EndTurn);
        assert_eq!(s.energy, base + 1, "下回合按新上限回满，**不再**额外加一次");
    }

    // ---------------- 新叶子 Op ----------------
    //
    // 这几条判定卡面文本读不出来，是玩家凭游戏经验给的答案（2026-08-15）。
    // 测试把它们钉住，免得以后被"合理推断"改回去。

    /// 拳斗：格挡等于**过完乘区**的伤害，不是卡面基础值。
    ///
    /// 玩家确认：7 点打在带易伤的目标上放大成 10，拿到的是 **10** 点格挡。
    #[test]
    fn pugilism_blocks_for_the_amplified_damage_not_the_face_value() {
        let mut s = State::new(80, 41);
        s.add_enemy(enemy::DUMMY, 100);
        let fist = s.add_card(card::PUGILISM, 0, 0);
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = fist;
        s.energy = 3;
        s.enemies[0].set(St::Vulnerable, 1);

        let hp0 = s.enemies[0].hp;
        s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(hp0 - s.enemies[0].hp, 10, "7 × 1.5 = 10.5 -> 10");
        assert_eq!(s.player.block, 10, "格挡跟着放大后的伤害走，不是 7");
    }

    /// 全身撞击：伤害 = 当前格挡，**且打完格挡还在**（玩家确认）。
    #[test]
    fn body_slam_reads_block_without_consuming_it() {
        let mut s = State::new(80, 43);
        s.add_enemy(enemy::DUMMY, 100);
        let slam = s.add_card(card::BODY_SLAM, 0, 0);
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = slam;
        s.energy = 3;
        s.player.block = 14;

        let hp0 = s.enemies[0].hp;
        s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(hp0 - s.enemies[0].hp, 14, "伤害 = 当前格挡");
        assert_eq!(s.player.block, 14, "格挡只是被读了一下，不消耗");
    }

    /// 万向斩：其他敌人吃的是**对主目标实际打出的数字**，不再各自过乘区。
    ///
    /// 主目标带易伤（8 -> 12），另外两个不带 —— 三个都掉 12。
    /// 若是"各自过自己的乘区"，另外两个只会掉 8。
    #[test]
    fn omni_slash_copies_the_number_dealt_to_the_main_target() {
        let mut s = State::new(80, 47);
        for _ in 0..3 {
            s.add_enemy(enemy::DUMMY, 100);
        }
        let omni = s.add_card(card::OMNI_SLASH, 0, 0);
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = omni;
        s.energy = 3;
        s.enemies[0].set(St::Vulnerable, 1);

        s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(100 - s.enemies[0].hp, 12, "主目标 8 × 1.5 = 12");
        assert_eq!(100 - s.enemies[1].hp, 12, "其他敌人吃同一个数字，不是 8");
        assert_eq!(100 - s.enemies[2].hp, 12);
    }

    /// 覆甲：**回合结束**给格挡（所以真的挡得住敌人这一击），
    /// **回合开始**掉 1 层。关键词原文如此 —— 和初代的行为不一样，别照记忆写。
    #[test]
    fn plated_armor_blocks_at_turn_end_and_decays_at_turn_start() {
        let mut s = State::new(80, 53);
        s.add_enemy(enemy::DUMMY, 100); // 每回合打 12
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.player.set(St::PlatedArmor, 4);

        s = step(s, Action::EndTurn);
        // 回合结束给 4 点格挡 -> 敌人 12 点被挡掉 4 -> 掉 8 血
        assert_eq!(s.player.hp, 72, "覆甲的格挡在敌人出手前就该到位");
        assert_eq!(s.player.get(St::PlatedArmor), 3, "回合开始掉 1 层");

        s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, 72 - (12 - 3), "这回合只剩 3 点格挡");
        assert_eq!(s.player.get(St::PlatedArmor), 2);
    }

    /// 黑暗镣铐：「本回合失去 N 点力量」用 `TempStrength` 存负数，
    /// 回合结束时和预备打击走**同一条**收回路径，只是方向相反。
    #[test]
    fn dark_shackles_restores_enemy_strength_at_end_of_turn() {
        let mut s = State::new(80, 59);
        s.add_enemy(enemy::DUMMY, 100);
        let shackles = s.add_card(card::DARK_SHACKLES, 0, 0);
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = shackles;
        s.energy = 3;
        s.enemies[0].set(St::Strength, 12);

        s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.enemies[0].get(St::Strength), 3, "12 - 9");
        // 训练假人基础 12 点，力量 3 => 15
        let hp0 = s.player.hp;
        s = step(s, Action::EndTurn);
        assert_eq!(hp0 - s.player.hp, 15, "敌人这一击吃的是被削后的力量");
        assert_eq!(s.enemies[0].get(St::Strength), 12, "回合结束把 9 点还回去");
    }

    /// 黏液能打出来（1 费抽 1 张后消耗），伤口不能 —— 两者都是状态牌。
    #[test]
    fn slimed_is_playable_but_wound_is_not() {
        assert!(playable(card::SLIMED), "黏液是能打出来的状态牌");
        assert!(!playable(card::WOUND), "伤口卡面写着「不能被打出」");
    }

    /// 火焰屏障：**多段攻击每一段各反伤一次**（玩家确认），且不看那一下
    /// 有没有被格挡吃掉 —— 它自带 12 点格挡，要求"真掉血才反伤"等于自我抵消。
    ///
    /// 用追踪手的 `1×8` 做样本：8 段 × 4 点 = 32 点反伤。
    /// HP 给到 100 是**故意的** —— 真实的 24 血追踪手会在第 6 段被反伤打死，
    /// 那测的就是「死了还打不打」而不是反伤本身
    /// （那条单独测：`dead_enemy_stops_mid_multi_hit_attack`）。
    #[test]
    fn flame_barrier_thorns_every_hit_of_a_multi_hit_attack() {
        let mut s = State::new(80, 61);
        s.add_enemy(enemy::RAIDER_TRACKER, 100);
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        // 追踪手的第 2 手才是连射 1×8，先空过一回合
        s.enemy_move[0] = 1;
        s.player.set(St::FlameBarrier, 4);
        s.player.block = 100; // 全挡下来，验证"被挡住也反伤"

        let hp0 = s.enemies[0].hp;
        s = step(s, Action::EndTurn);
        assert_eq!(hp0 - s.enemies[0].hp, 32, "8 段各反伤 4 点");
    }

    /// 攻击者在**多段攻击中途**被反伤打死，剩下的段数不该继续打。
    ///
    /// 这条是上面那个测试逼出来的真 bug：24 血的追踪手打 `1×8`，
    /// 反伤在第 6 段就把它打死了，可它照样把 8 段全打完。
    #[test]
    fn dead_enemy_stops_mid_multi_hit_attack() {
        let mut s = State::new(80, 97);
        s.add_enemy(enemy::RAIDER_TRACKER, 24);
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.enemy_move[0] = 1; // 连射 1×8
        s.player.set(St::FlameBarrier, 4);

        let hp0 = s.player.hp;
        s = step(s, Action::EndTurn);
        assert!(!s.enemies[0].alive(), "24 血被 6 段反伤 ×4 打死");
        assert_eq!(hp0 - s.player.hp, 6, "它只来得及打 6 下，不是 8 下");
    }

    /// 狂怒：本回合内每打出一张攻击牌获得格挡，且**回合一过就失效**。
    #[test]
    fn frenzy_blocks_per_attack_and_expires_next_turn() {
        let mut s = State::new(80, 67);
        s.add_enemy(enemy::DUMMY, 100);
        for _ in 0..10 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        s.player.set(St::Frenzy, 3);
        s.energy = 3;

        s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.player.block, 3);
        s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.player.block, 6, "每张攻击牌各给一次");

        s = step(s, Action::EndTurn);
        assert_eq!(s.player.get(St::Frenzy), 0, "「这个回合」到下回合开始就清了");
    }

    /// 手牌垃圾牌：回合结束时还在手上才发作，伤害**走格挡**（玩家确认）。
    /// 虚无（晕眩）则是把自己消耗掉。
    #[test]
    fn burn_hits_through_block_and_dazed_voids_itself() {
        let mut s = State::new(80, 71);
        s.add_enemy(enemy::DUMMY, 100); // 每回合打 12
        let burn = s.add_card(card::BURN, 0, 0);
        let dazed = s.add_card(card::DAZED, 0, 0);
        for _ in 0..8 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.n_hand = 2;
        s.hand[0] = burn;
        s.hand[1] = dazed;
        s.player.block = 10;

        s = step(s, Action::EndTurn);
        // 灼伤 2 点被 10 点格挡吃掉 -> 剩 8 点格挡 -> 敌人 12 点打进 4 点
        assert_eq!(s.player.hp, 76, "灼伤走格挡，不是直接扣血");
        assert_eq!(s.n_exh, 1, "晕眩把自己消耗掉了");
    }

    /// 怨恨：`hp_lost_this_turn` 这个字段当初就是为它这类牌准备的。
    /// 没失血 = 打 1 下，失过血 = 打 2 下。
    #[test]
    fn resentment_reads_the_hp_lost_counter() {
        fn play(bleed_first: bool) -> i32 {
            let mut s = State::new(80, 73);
            s.add_enemy(enemy::DUMMY, 100);
            let res = s.add_card(card::RESENTMENT, 0, 0);
            let hemo = s.add_card(card::HEMOKINESIS, 0, 0);
            for _ in 0..8 {
                s.add_card(card::DEFEND, 0, 0);
            }
            let mut s = begin_combat(s);
            s.n_hand = 2;
            s.hand[0] = res;
            s.hand[1] = hemo;
            s.energy = 3;
            if bleed_first {
                // 御血术失去 2 点生命
                s = step(s, Action::PlayCard { hand: 1, target: 0 });
            }
            let hp0 = s.enemies[0].hp;
            let ix = hand_ix_of(&s, card::RESENTMENT);
            let s = step(s, Action::PlayCard { hand: ix, target: 0 });
            hp0 - s.enemies[0].hp
        }
        assert_eq!(play(false), 5, "本回合没失过血 => 只打 1 下");
        assert_eq!(play(true), 10, "失过血 => 攻击 2 次");
    }

    /// 壁垒是**规则修饰**：格挡不在回合开始时清零。
    /// 触发器表达不了"不要做某件事"，所以它走 `RULE_MODIFIERS` + 一个窄 if。
    #[test]
    fn barricade_keeps_block_across_turns() {
        let mut s = State::new(80, 79);
        s.add_enemy(enemy::DUMMY, 100);
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.player.set(St::Barricade, 1);
        s.player.block = 30;

        s = step(s, Action::EndTurn);
        // 敌人打 12，从 30 点格挡里扣，剩 18 —— 而且回合开始没清零
        assert_eq!(s.player.block, 18, "壁垒下格挡跨回合保留");
        assert_eq!(s.player.hp, 80, "一点血没掉");
    }

    /// 孤注一掷：50 点格挡不是白给的，挨一下没被挡住的攻击就死。
    /// **漏掉这个死亡条件会让内核以为它是张白送格挡的牌** —— 正是
    /// "模拟器过于乐观"那个失败模式。
    #[test]
    fn all_or_nothing_kills_you_on_unblocked_damage() {
        let mut s = State::new(80, 83);
        s.add_enemy(enemy::DUMMY, 100); // 每回合打 12
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.player.set(St::AllOrNothing, 1);

        // 格挡够厚 -> 平安无事
        s.player.block = 50;
        s = step(s, Action::EndTurn);
        assert!(!s.player_dead, "全挡下来就没事");

        // 格挡不够 -> 立刻死
        s.player.block = 1;
        let s = step(s, Action::EndTurn);
        assert!(s.player_dead, "漏进来一点攻击伤害就该死");
    }

    /// 暴走改的是**这一张牌实例**，不是同名的所有牌。
    #[test]
    fn rampage_grows_only_its_own_instance() {
        let mut s = State::new(80, 89);
        s.add_enemy(enemy::DUMMY, 100);
        let a = s.add_card(card::RAMPAGE, 0, 0);
        let b = s.add_card(card::RAMPAGE, 0, 0);
        for _ in 0..8 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.n_hand = 2;
        s.hand[0] = a;
        s.hand[1] = b;
        s.energy = 3;

        let hp0 = s.enemies[0].hp;
        s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(hp0 - s.enemies[0].hp, 9, "第一张打 9");
        let hp1 = s.enemies[0].hp;
        s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(hp1 - s.enemies[0].hp, 9, "另一张仍然是 9，没被带着一起变强");
        assert_eq!(s.cards[a as usize].bonus, 5, "变强的是打出去过的那一张");
        assert_eq!(s.cards[b as usize].bonus, 5);
    }

    /// 脆弱 (`FRAIL_POWER`) 让**卡牌给的格挡** ×0.75 向下取整。
    ///
    /// 实测（2026-08-15 第1幕第12层）：追踪手挂上脆弱 2 之后，手牌里防御的
    /// 卡面描述从「获得5点格挡」变成「获得3点格挡」，打出去也确实是 3。
    /// 5×0.75 = 3.75 → 3。
    #[test]
    fn frail_scales_card_block_by_three_quarters() {
        let mut me = Entity::new(80);
        assert_eq!(card_block(5, &mut me), 5);
        me.set(St::Frail, 2);
        assert_eq!(card_block(5, &mut me), 3, "防御 5 在脆弱下应给 3");
    }

    /// `legal_actions` 对**完全相同**的手牌去重：打哪一张结果一样。
    ///
    /// 这是给 L2 用的：不去重的话三张打击 × 三个敌人会生成 9 个动作，
    /// 而实际只有 3 个不同的决策，分支因子白白翻三倍。
    /// 升级过的那张**不能**和普通的混为一谈 —— `CardInst` 带着升级位。
    #[test]
    fn legal_actions_dedups_identical_cards_in_hand() {
        let mut s = State::new(80, 113);
        for _ in 0..3 {
            s.add_enemy(enemy::DUMMY, 100);
        }
        for _ in 0..3 {
            s.add_card(card::STRIKE, 0, 0);
        }
        s.add_card(card::STRIKE, F_UPGRADED, 0);
        s.add_card(card::DEFEND, 0, 0);
        let mut s = begin_combat(s);
        s.energy = 9;

        let (_, n) = legal_actions(&s);
        // 打击(3 张相同 -> 1) × 3 个敌人 = 3
        // 打击+(1 张，升级位不同，不去重) × 3 个敌人 = 3
        // 防御(不指向目标) = 1
        // 结束回合 = 1
        assert_eq!(n, 8, "三张相同的打击应当只留一张的分支");
    }

    // ---------------- 召唤 / 爪牙 / 敌人塞牌 ----------------

    /// 雾菇召唤利齿之眼：新敌人当回合**不行动**，下回合才出手。
    #[test]
    fn summon_adds_an_enemy_that_waits_a_turn() {
        let mut s = State::new(80, 101);
        s.add_enemy(enemy::FOGMOG, 74);
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        assert_eq!(s.n_enemies, 1);

        let cards_before = s.n_cards;
        s = step(s, Action::EndTurn); // 雾菇第一手：召唤
        assert_eq!(s.n_enemies, 2, "召出来一个");
        assert_eq!(s.enemies[1].hp, 6);
        assert_eq!(s.enemies[1].get(St::Minion), 1, "爪牙标记来自它自己的 start_status");
        assert_eq!(
            s.n_cards, cards_before,
            "召出来的那回合它不该行动（塞牌是它的第一手）"
        );
    }

    /// **非爪牙全死光，战斗就结束**，剩下的爪牙跟着消失。
    ///
    /// 两次实测：雾菇一死利齿之眼消失；第1幕 Boss 神官一死两个信徒消失。
    /// 不实现这条，L2 会把那场 Boss 估成 307 血而不是实际要打的 190。
    #[test]
    fn killing_the_master_ends_the_fight_even_with_minions_alive() {
        let mut s = State::new(80, 103);
        s.add_enemy(enemy::KIN_PRIEST, 15);
        s.add_enemy(enemy::KIN_FOLLOWER, 59);
        s.add_enemy(enemy::KIN_FOLLOWER, 58);
        for _ in 0..10 {
            s.add_card(card::HEMOKINESIS, 0, 0);
        }
        let mut s = begin_combat(s);
        s.energy = 3;

        // 御血术**基础版** 15 点（不是升级版的 20），正好打死 15 血的神官
        s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert!(!s.enemies[0].alive(), "神官死了");
        assert!(s.enemies[1].alive() && s.enemies[2].alive(), "信徒还站着");
        assert!(s.combat_over, "非爪牙全灭 => 战斗结束，不用清爪牙");
        assert!(!s.player_dead);
    }

    /// 一场只有爪牙的仗不该开局就判定结束（`no_master_left` 的兜底分支）。
    #[test]
    fn a_fight_of_only_minions_still_has_to_be_fought() {
        let mut s = State::new(80, 107);
        s.add_enemy(enemy::KIN_FOLLOWER, 59);
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let s = begin_combat(s);
        assert!(!s.combat_over, "场上从没有过非爪牙时，退回普通规则");
    }

    /// 史莱姆的 `StatusCard`：往**弃牌堆**塞黏液，不是手牌。
    #[test]
    fn slime_spits_slimed_into_the_discard_pile() {
        let mut s = State::new(80, 109);
        s.add_enemy(enemy::TWIG_SLIME_M, 28);
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        // 树枝史莱姆（中）的第 0 手就是吐黏液（wiki：Starts with Sticky Shot）
        let disc_before = s.n_disc;
        let hand_before = s.n_hand;

        s = step(s, Action::EndTurn);
        // 结束回合会把手牌弃掉再抽 5 张，所以只比"多出来的那一张"
        assert_eq!(
            s.n_disc,
            disc_before + hand_before + 1,
            "弃牌堆比「原有 + 弃掉的手牌」多一张"
        );
        let slimed = (0..s.n_disc as usize)
            .filter(|&i| s.cards[s.disc[i] as usize].id == card::SLIMED)
            .count();
        assert_eq!(slimed, 1, "多出来的那张是黏液");
    }

    /// 乘区累乘之后**只取整一次**，不是每个乘区各取一次。
    ///
    /// 这条推翻了我先前写死在文档里的"每一步都向下取整"。证据是
    /// 2026-08-15 第1幕 Boss 的一帧：我带虚弱、神官带易伤，
    /// 怨恨（基础 5，攻击两次）打出 **10** 而不是 8。这也是验证器建成以来
    /// 报出的**第一个真 MISMATCH**。
    #[test]
    fn multipliers_floor_once_at_the_end() {
        let mut atk = Entity::new(80);
        atk.set(St::Weak, 1);
        let mut def = Entity::new(100);
        def.set(St::Vulnerable, 1);

        // 怨恨基础 5：floor(5 × 0.75 × 1.5) = floor(5.625) = 5
        // 逐步取整会给 floor(floor(5×0.75)×1.5) = floor(3×1.5) = 4
        assert_eq!(apply_modifiers(5, &atk, &def), 5, "只有小基础值分得开两种算法");
        // 御血术+ 基础 20：两种算法都给 22 —— 留在这里说明为什么这条
        // 拖到现在才被发现，之前的样本全都分不开
        assert_eq!(apply_modifiers(20, &atk, &def), 22);
    }

    /// 缩小 (`SHRINK_POWER`) 让攻击方伤害 ×2/3，向下取整。
    ///
    /// 四个实测同时满足这一个倍率（2026-08-15 第1幕第2层，缩小甲虫）：
    /// 打击 6→4、痛击 8→5、打击带易伤 6→6、无 debuff 痛击 8→8。
    /// 注意**只有倍率被验证，乘区位置没有** —— 见 `damage.rs` 里的说明。
    #[test]
    fn shrink_scales_attacker_damage_by_two_thirds() {
        let mut me = Entity::new(80);
        let foe = Entity::new(100);
        assert_eq!(apply_modifiers(6, &me, &foe), 6);

        me.set(St::Shrink, -1);
        assert_eq!(apply_modifiers(6, &me, &foe), 4, "打击 6 在缩小下应打出 4");
        assert_eq!(apply_modifiers(8, &me, &foe), 5, "痛击 8 在缩小下应打出 5（向下取整）");

        let mut vuln_foe = Entity::new(100);
        vuln_foe.set(St::Vulnerable, 1);
        assert_eq!(apply_modifiers(6, &me, &vuln_foe), 6, "缩小 + 易伤应打出 6");
    }

    /// A state is a plain value: copying it must be a memcpy, and mutating the
    /// copy must not touch the original. This is what makes search cheap.
    #[test]
    fn state_is_a_value() {
        let s = base_state();
        let mut t = s;
        t.player.hp = 1;
        assert_eq!(s.player.hp, 80);
        assert_eq!(t.player.hp, 1);
        // keep an eye on the size; it should stay cache-friendly
        assert!(
            std::mem::size_of::<State>() < 4096,
            "State grew to {} bytes",
            std::mem::size_of::<State>()
        );
    }

    #[test]
    fn step_is_pure() {
        let mut s = base_state();
        s.add_card(card::STRIKE, 0, 0);
        s = begin_combat(s);
        let before = s;
        let after = step(s, Action::PlayCard { hand: 0, target: 0 });
        // the input value is untouched
        assert_eq!(before.enemies[0].hp, 100);
        assert!(after.enemies[0].hp < 100);
    }

    /// The multiplier order this run got wrong in my head. Two identical 打击
    /// in one turn against a Slow+Vulnerable target must hit for 16 then 18.
    #[test]
    fn multipliers_are_multiplicative_not_additive() {
        let mut atk = Entity::new(50);
        atk.set(St::Strength, 4);
        let mut def = Entity::new(200);
        def.set(St::Vulnerable, 1);

        let face = card_face_damage(6, (1, 1), 0, atk.get(St::Strength), 0); // 10
        assert_eq!(face, 10);

        def.set(St::Slow, 10);
        assert_eq!(apply_modifiers(face, &atk, &def), 16);
        def.set(St::Slow, 20);
        assert_eq!(apply_modifiers(face, &atk, &def), 18);
        // additive would have given 16 and 17
    }

    #[test]
    fn corrupt_scales_base_before_strength() {
        // 拆卸+ with 腐化: base 10 -> 15, then +strength
        assert_eq!(card_face_damage(10, (3, 2), 0, 0, 0), 15);
        assert_eq!(card_face_damage(10, (3, 2), 0, 3, 0), 18);
    }

    #[test]
    fn damage_cap_applies() {
        let atk = Entity::new(50);
        let mut def = Entity::new(50);
        def.set(St::DamageCap, 9);
        assert_eq!(apply_modifiers(40, &atk, &def), 9);
        assert_eq!(apply_modifiers(5, &atk, &def), 5);
    }

    #[test]
    fn intangible_floors_everything_to_one() {
        let atk = Entity::new(50);
        let mut def = Entity::new(50);
        def.set(St::Intangible, 1);
        assert_eq!(apply_modifiers(300, &atk, &def), 1);
    }

    #[test]
    fn artifact_eats_one_debuff_application_not_one_stack() {
        let mut e = Entity::new(50);
        e.set(St::Artifact, 1);
        // a 2-stack Vulnerable application is fully absorbed by 1 Artifact
        assert!(artifact_absorbs(&mut e, true));
        assert_eq!(e.get(St::Artifact), 0);
        assert!(!artifact_absorbs(&mut e, true));
    }

    /// 激怒: Skills feed the boss Strength, Powers do not.
    #[test]
    fn rage_triggers_on_skills_but_not_powers() {
        let mut s = State::new(80, 1);
        s.add_enemy(enemy::TEST_SUBJECT, 100);
        s.add_card(card::DEFEND, 0, 0);
        s.add_card(card::DEMON_FORM, 0, 0);
        s = begin_combat(s);
        s.energy = 9;
        assert_eq!(s.enemies[0].get(St::Strength), 0);

        // find and play the Skill
        let skill = (0..s.n_hand as usize)
            .find(|&i| s.cards[s.hand[i] as usize].id == card::DEFEND)
            .unwrap();
        s = step(s, Action::PlayCard { hand: skill as u8, target: 0 });
        assert_eq!(s.enemies[0].get(St::Strength), 2, "Skill must feed 激怒");

        let power = (0..s.n_hand as usize)
            .find(|&i| s.cards[s.hand[i] as usize].id == card::DEMON_FORM)
            .unwrap();
        s = step(s, Action::PlayCard { hand: power as u8, target: 0 });
        assert_eq!(s.enemies[0].get(St::Strength), 2, "Power must NOT feed 激怒");
    }

    /// 灰烬打击 scales off the exhaust pile — the old sim could not express this.
    #[test]
    fn ash_strike_scales_with_exhaust_pile() {
        let mut s = base_state();
        for _ in 0..6 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let ash = s.add_card(card::ASH_STRIKE, 0, 0);
        s = begin_combat(s);
        // force a known exhaust pile and a known hand
        s.n_exh = 5;
        s.n_hand = 1;
        s.hand[0] = ash;
        s.energy = 3;
        let hp0 = s.enemies[0].hp;
        s = step(s, Action::PlayCard { hand: 0, target: 0 });
        // 6 + 3*5 = 21
        assert_eq!(hp0 - s.enemies[0].hp, 21);
    }

    /// 恶魔之焰 exhausts the rest of the hand and scales off how many.
    #[test]
    fn demon_flame_consumes_hand() {
        let mut s = base_state();
        let flame = s.add_card(card::DEMON_FLAME, 0, 0);
        for _ in 0..4 {
            s.add_card(card::STRIKE, 0, 0);
        }
        s = begin_combat(s);
        s.n_hand = 5;
        s.hand[0] = flame;
        for k in 1..5u8 {
            s.hand[k as usize] = k;
        }
        s.energy = 3;
        let hp0 = s.enemies[0].hp;
        s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.n_hand, 0, "hand must be emptied");
        assert_eq!(hp0 - s.enemies[0].hp, 28, "4 cards x 7");
        assert_eq!(s.n_exh, 5, "4 exhausted + the card itself");
    }

    /// 踩踏 gets cheaper per attack played this turn.
    #[test]
    fn stampede_cost_drops_per_attack() {
        let mut s = base_state();
        s.add_card(card::STAMPEDE, 0, 0);
        s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = 0;
        assert_eq!(effective_cost(&s, 0), 3);
        s.attacks_played = 2;
        assert_eq!(effective_cost(&s, 0), 1);
        s.attacks_played = 5;
        assert_eq!(effective_cost(&s, 0), 0);
    }

    /// 拆卸 hits twice only when the target is Vulnerable.
    #[test]
    fn dismantle_doubles_on_vulnerable() {
        let mut s = base_state();
        let d = s.add_card(card::DISMANTLE, 0, 0);
        s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = d;
        s.energy = 3;
        let mut t = s;
        let hp0 = t.enemies[0].hp;
        t = step(t, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(hp0 - t.enemies[0].hp, 8, "single hit without 易伤");

        let mut u = s;
        u.enemies[0].set(St::Vulnerable, 2);
        let hp1 = u.enemies[0].hp;
        u = step(u, Action::PlayCard { hand: 0, target: 0 });
        // 8 * 1.5 = 12, twice
        assert_eq!(hp1 - u.enemies[0].hp, 24, "double hit under 易伤");
    }

    /// 烙印 opens a sub-choice; while pending, only Choose is legal.
    #[test]
    fn pending_choice_gates_actions() {
        let mut s = base_state();
        let brand = s.add_card(card::BRAND, 0, 0);
        for _ in 0..3 {
            s.add_card(card::STRIKE, 0, 0);
        }
        s = begin_combat(s);
        s.n_hand = 4;
        s.hand[0] = brand;
        for k in 1..4u8 {
            s.hand[k as usize] = k;
        }
        s.energy = 3;
        s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert!(matches!(s.pending, Pending::ExhaustFromHand { .. }));

        let (acts, n) = legal_actions(&s);
        assert!(n > 0);
        assert!(
            acts[..n].iter().all(|a| matches!(a, Action::Choose { .. })),
            "only Choose is legal while a choice is pending"
        );

        let exh0 = s.n_exh;
        s = step(s, Action::Choose { hand: 0 });
        assert_eq!(s.pending, Pending::None);
        assert_eq!(s.n_exh, exh0 + 1);
        assert_eq!(s.player.get(St::Strength), 1, "烙印 grants Strength");
    }

    /// Same seed + same actions => byte-identical state. Required for paired
    /// comparison and for replay validation.
    #[test]
    fn deterministic_given_seed() {
        let build = || {
            let mut s = State::new(80, 999);
            s.add_enemy(enemy::DUMMY, 100);
            for _ in 0..5 {
                s.add_card(card::STRIKE, 0, 0);
            }
            for _ in 0..5 {
                s.add_card(card::DEFEND, 0, 0);
            }
            begin_combat(s)
        };
        let mut a = build();
        let mut b = build();
        for _ in 0..6 {
            a = step(a, Action::EndTurn);
            b = step(b, Action::EndTurn);
        }
        assert_eq!(a, b);
    }

    // -----------------------------------------------------------------------
    // L1：注入式敌人回合（`end_turn_with_incoming`）
    //
    // 它是 L2 的入口，所以先把它自己钉住：注入的伤害必须走**和敌人真出手
    // 完全相同**的那条路（格挡吸收、火焰屏障、孤注一掷、死人不出手）。
    // 走岔了的话，求解器算出来的"挨完打还剩多少血"全是错的。
    // -----------------------------------------------------------------------

    /// 注入的伤害先被格挡吃，溢出才进 HP —— 和 `enemy_turn` 同一条 `absorb`。
    #[test]
    fn injected_incoming_goes_through_block_first() {
        let mut s = base_state();
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.player.block = 7;
        let mut inc = NO_INCOMING;
        inc[0] = (10, 1);
        let after = end_turn_with_incoming(s, &inc);
        assert_eq!(after.player.hp, 80 - 3, "10 点打进 7 点格挡 => 只掉 3 血");
    }

    /// **死掉的敌人不出手。** 这条正是单回合求解的核心价值 ——
    /// 这回合把谁打死了，它那份伤害就不会落下来。
    #[test]
    fn a_dead_enemy_deals_no_injected_damage() {
        let mut s = State::new(80, 5);
        s.add_enemy(enemy::DUMMY, 100);
        s.add_enemy(enemy::DUMMY, 100);
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.enemies[0].hp = 0;
        let mut inc = NO_INCOMING;
        inc[0] = (20, 1);
        inc[1] = (5, 1);
        let after = end_turn_with_incoming(s, &inc);
        assert_eq!(after.player.hp, 75, "死掉的那只不该打出它的 20 点");
    }

    /// 火焰屏障对**注入的**攻击一样每段反伤一次。
    /// 两条路径共用 `take_attack_hit` 就是为了这个。
    #[test]
    fn flame_barrier_answers_injected_hits_too() {
        let mut s = base_state();
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.player.set(St::FlameBarrier, 4);
        let hp0 = s.enemies[0].hp;
        let mut inc = NO_INCOMING;
        inc[0] = (3, 3);
        let after = end_turn_with_incoming(s, &inc);
        assert_eq!(hp0 - after.enemies[0].hp, 12, "3 段各反伤 4 点");
    }

    // -----------------------------------------------------------------------
    // L2：单回合求解器
    //
    // 每一条测的都是**顺序**，因为顺序正是这一层唯一的产出。测的机制全部
    // 已经和真实游戏对拍验证过（踩踏减费、先易伤后拆卸、灰烬打击随消耗堆增长），
    // 所以这些断言的权威性来自 L1，不来自我对策略的判断。
    // -----------------------------------------------------------------------

    use super::solver::{
        advise_potions, explain, score, score_line, solve_turn, solve_turn_budget, Line,
        PotionPolicy, PotionVerdict, Threat,
    };

    /// 手牌里放指定的几张牌，能量给定，其余牌区清空 —— 让搜索空间可控。
    fn hand_of(seed: u64, enemy_hp: i32, cards: &[u16], energy: i32) -> State {
        let mut s = State::new(80, seed);
        s.add_enemy(enemy::DUMMY, enemy_hp);
        let ix: Vec<u8> = cards.iter().map(|&c| s.add_card(c, 0, 0)).collect();
        let mut s = begin_combat(s);
        s.n_hand = ix.len() as u8;
        for (i, c) in ix.iter().enumerate() {
            s.hand[i] = *c;
        }
        s.n_draw = 0;
        s.n_disc = 0;
        s.energy = energy;
        s
    }

    /// 踩踏「你在本回合中每打出过一张攻击牌，其耗能减少1」。
    ///
    /// 先打踩踏：3 费，一回合就这一张。
    /// 先打两张打击：1+1，踩踏降到 1 费，三张全打出去。
    /// 减费**已在实战确认**（第1幕第8层），所以这条线是真的。
    #[test]
    fn solver_finds_the_stomp_discount_order() {
        let s = hand_of(1, 100, &[card::STAMPEDE, card::STRIKE, card::STRIKE], 3);
        let line = solve_turn(&s, &Threat::NONE, score::survive_first);
        let names = explain(&s, line.acts());
        assert_eq!(names.len(), 3, "三张都该打出去，实际: {names:?}");
        assert_eq!(names[2], "踩踏", "踩踏必须放在两张攻击牌之后: {names:?}");
    }

    /// 先上易伤，拆卸才打第二下。
    ///
    /// 痛击→拆卸: 8 + (8×1.5)×2 = 8+24 = 32
    /// 拆卸→痛击: 8 + 8 = 16（拆卸打出去时目标还没有易伤）
    #[test]
    fn solver_applies_vulnerable_before_the_double_hit() {
        let s = hand_of(2, 100, &[card::DISMANTLE, card::BASH], 3);
        let line = solve_turn(&s, &Threat::NONE, score::survive_first);
        assert_eq!(explain(&s, line.acts()), vec!["痛击", "拆卸"]);
        let end = super::solver::replay_line(&s, line.acts()).unwrap();
        assert_eq!(100 - end.enemies[0].hp, 32);
    }

    /// 灰烬打击随消耗堆增长，所以自带消耗的牌要先打。
    ///
    /// 烙印（0 费，失 1 血，消耗手里一张牌，+1 力量）→ 灰烬打击：
    /// 消耗堆 +1 => 基础 6+3=9，再吃 +1 力量 = 10。
    /// 反过来先打灰烬打击只有 6。
    #[test]
    fn solver_exhausts_before_ash_strike() {
        let s = hand_of(3, 100, &[card::ASH_STRIKE, card::BRAND, card::DEFEND], 3);
        let line = solve_turn(&s, &Threat::NONE, score::survive_first);
        let names = explain(&s, line.acts());
        assert_eq!(names[0], "烙印", "先消耗再打灰烬打击: {names:?}");
        assert!(names.iter().any(|n| n == "灰烬打击"), "{names:?}");
        // 子选择（`Pending`）也在搜索空间里：烙印开的选牌界面必须被走完，
        // 否则后面一张牌都打不出去。
        assert!(names.iter().any(|n| n.starts_with("选牌:")), "{names:?}");
    }

    /// 同一个局面，**只换威胁**，最优线就从进攻变成防守。
    ///
    /// 这正是"`Threat` 是输入"这条设计的意义：换提供者（观测意图 /
    /// `EnemyDef` 预测）不用改求解器一行。
    #[test]
    fn the_threat_is_an_input_and_it_changes_the_answer() {
        let build = || {
            let mut s = hand_of(4, 100, &[card::STRIKE, card::DEFEND, card::DEFEND], 3);
            s.player.hp = 10;
            s
        };

        let calm = solve_turn(&build(), &Threat::NONE, score::survive_first);
        assert_eq!(explain(&build(), calm.acts()), vec!["打击"], "没有威胁时格挡一文不值");

        let mut t = Threat::new();
        t.set(0, 12, 1);
        let danger = solve_turn(&build(), &t, score::survive_first);
        let names = explain(&build(), danger.acts());
        assert_eq!(names.iter().filter(|n| *n == "防御").count(), 2, "{names:?}");
        let end = super::solver::replay_line(&build(), danger.acts()).unwrap();
        let after = end_turn_with_incoming(end, &t.incoming);
        assert!(!after.player_dead, "10 血挨 12 点，两张防御正好活下来");
    }

    /// 杀掉打得最疼的那只，等于这回合少挨一份伤害。
    /// 求解器必须自己发现这一点 —— 它是从 `end_turn_with_incoming` 里长出来的，
    /// 不是我写死的启发式。
    #[test]
    fn solver_kills_the_attacker_that_hurts_most() {
        let mut s = State::new(80, 6);
        s.add_enemy(enemy::DUMMY, 6); // 一下就能打死
        s.add_enemy(enemy::DUMMY, 100);
        let strike = s.add_card(card::STRIKE, 0, 0);
        for _ in 0..8 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = strike;
        s.n_draw = 0;
        s.energy = 1;

        let mut t = Threat::new();
        t.set(0, 20, 1); // 脆但打得疼
        t.set(1, 1, 1);
        let line = solve_turn(&s, &t, score::survive_first);
        assert_eq!(line.acts(), &[Action::PlayCard { hand: 0, target: 0 }]);
    }

    /// **求解器不许说谎**：它报的分数必须能被重放出来。
    ///
    /// 这条是结构性的 —— 分数是搜索里算的，线是搜索里记的，两者对不上
    /// 就意味着记录和评估走岔了，而那种 bug 从输出上完全看不出来。
    #[test]
    fn the_reported_line_replays_to_the_reported_score() {
        let s = hand_of(
            7,
            60,
            &[card::BASH, card::DISMANTLE, card::STRIKE, card::DEFEND, card::STAMPEDE],
            3,
        );
        let mut t = Threat::new();
        t.set(0, 9, 2);
        let sol = solve_turn_budget(&s, &t, score::survive_first, super::solver::DEFAULT_BUDGET);
        assert!(sol.complete, "这个规模应该能搜完");
        let again = score_line(&s, &t, score::survive_first, sol.line.acts());
        assert_eq!(again, Some(sol.line.score));
    }

    /// 搜完的时候，没有任何一条人手打的线能赢过它。
    /// （验收用的正是这个性质：求解器打不赢实录里的人，就说明有 bug。）
    #[test]
    fn a_complete_search_is_not_beaten_by_any_hand_written_line() {
        let s = hand_of(8, 60, &[card::BASH, card::DISMANTLE, card::STRIKE], 3);
        let mut t = Threat::new();
        t.set(0, 7, 1);
        let sol = solve_turn_budget(&s, &t, score::survive_first, super::solver::DEFAULT_BUDGET);
        assert!(sol.complete);

        // 手写几条合法的线，全部不该超过它
        let candidates: Vec<Vec<Action>> = vec![
            vec![],
            vec![Action::PlayCard { hand: 1, target: 0 }],
            vec![
                Action::PlayCard { hand: 1, target: 0 },
                Action::PlayCard { hand: 0, target: 0 },
            ],
            vec![
                Action::PlayCard { hand: 0, target: 0 },
                Action::PlayCard { hand: 0, target: 0 },
            ],
        ];
        for c in candidates {
            if let Some(v) = score_line(&s, &t, score::survive_first, &c) {
                assert!(v <= sol.line.score, "手写线 {c:?} 得分 {v} 超过了求解器 {}", sol.line.score);
            }
        }
    }

    /// 预算用光要**报出来**，不能悄悄返回一个看着很自信的分数。
    /// 旧 Python 模拟器满表的 `approx=yes` 就是这个失败模式。
    #[test]
    fn budget_exhaustion_is_reported_not_hidden() {
        let s = hand_of(
            9,
            200,
            &[
                card::STRIKE,
                card::DEFEND,
                card::BASH,
                card::DISMANTLE,
                card::ASH_STRIKE,
                card::IRON_WAVE,
            ],
            9,
        );
        let sol = solve_turn_budget(&s, &Threat::NONE, score::survive_first, 20);
        assert!(!sol.complete, "20 个节点不可能搜完这手牌");
        // 就算搜不完，返回的线也必须是合法的
        assert!(super::solver::replay_line(&s, sol.line.acts()).is_some());
    }

    /// 基线 = 什么都不打。`gain()` 是相对它的净收益 ——
    /// 这个仓库一贯的读法是比较，不看绝对分。
    #[test]
    fn the_baseline_is_doing_nothing() {
        let s = hand_of(10, 100, &[card::STRIKE], 3);
        let sol = solve_turn_budget(&s, &Threat::NONE, score::survive_first, 10_000);
        assert_eq!(sol.baseline, score_line(&s, &Threat::NONE, score::survive_first, &[]).unwrap());
        assert!(sol.gain() > 0, "打一张打击总比不打强");
    }

    /// 空手牌：返回空线，不 panic，分数就是基线。
    #[test]
    fn an_empty_hand_solves_to_an_empty_line() {
        let mut s = hand_of(11, 100, &[card::STRIKE], 3);
        s.n_hand = 0;
        let sol = solve_turn_budget(&s, &Threat::NONE, score::survive_first, 10_000);
        assert!(sol.line.is_empty());
        assert_eq!(sol.line.score, sol.baseline);
    }

    /// 换序到达同一局面的分支要被合并掉，否则分支因子是阶乘级的。
    ///
    /// 四张互不相同、没有减费/易伤/抽牌耦合的牌：不合并的话前缀数
    /// = Σ P(4,k) = 65，节点数 129（65 次叶子评估 + 64 次 step）。
    ///
    /// 合并到不了理论下限 48（= 2^4 个子集）：`State::last_damage` 也是状态的
    /// 一部分（拳斗和万向斩读它），所以"最后打出的是哪张攻击牌"不同的两个
    /// 顺序**确实是两个局面**，指纹分开是对的，不是漏合并。
    #[test]
    fn transpositions_are_merged() {
        let s = hand_of(
            12,
            200,
            &[card::STRIKE, card::DEFEND, card::IRON_WAVE, card::TWIN_STRIKE],
            4,
        );
        let sol = solve_turn_budget(&s, &Threat::NONE, score::survive_first, 200_000);
        assert!(sol.complete);
        assert!(sol.nodes <= 70, "节点数 {} 说明换序基本没被合并（不合并是 129）", sol.nodes);
    }

    /// **抽牌会让换序合并失效**，这是搜索规模上最该知道的一条。
    ///
    /// 抽一张牌就把抽牌堆/弃牌堆搅动一次，于是本来等价的两个顺序到达的局面
    /// 不再逐字节相同，指纹自然分开。不是 bug —— 抽到什么本来就依赖顺序 ——
    /// 但它意味着**带抽牌的手牌，搜索规模是阶乘级的**，预算更容易见底。
    #[test]
    fn drawing_defeats_transposition_merging() {
        let s = hand_of(
            13,
            200,
            &[card::STRIKE, card::DEFEND, card::IRON_WAVE, card::POMMEL_STRIKE],
            4,
        );
        let sol = solve_turn_budget(&s, &Threat::NONE, score::survive_first, 200_000);
        assert!(sol.complete);
        assert!(sol.nodes > 100, "有抽牌时不该合并掉换序，实际 {}", sol.nodes);
    }

    /// `Line` 是 POD，可以按值塞进搜索栈 —— 和 `State` 一样的要求。
    #[test]
    fn a_line_is_a_value() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<Line>();
        assert_copy::<Threat>();
    }

    #[test]
    fn combat_ends_when_enemy_dies() {
        let mut s = State::new(80, 7);
        s.add_enemy(enemy::EXOSKELETON, 22);
        for _ in 0..6 {
            s.add_card(card::STRIKE, 0, 0);
        }
        s = begin_combat(s);
        s.energy = 99;
        let mut guard = 0;
        while !s.combat_over && guard < 50 {
            let (acts, n) = legal_actions(&s);
            if n == 0 {
                break;
            }
            s = step(s, acts[0]);
            guard += 1;
        }
        assert!(s.combat_over);
        assert!(!s.player_dead);
    }

    // -----------------------------------------------------------------------
    // 药水策略：机会成本在搜索**外面**
    //
    // 药水不要能量，所以只要边际收益 > 0 求解器就会喝。实测过：满血 80/80、
    // 第 1 回合、一只打 8 点的敌人，它会把三瓶一次喝光。定价必须在外面做。
    // -----------------------------------------------------------------------

    fn with_potions(seed: u64, hp: i32, enemy_hp: i32, pots: &[u8]) -> State {
        let mut s = hand_of(seed, enemy_hp, &[card::STRIKE, card::DEFEND], 3);
        s.player.hp = hp;
        for (i, p) in pots.iter().enumerate() {
            s.potions[i] = *p;
        }
        s
    }

    /// 只省几点血的药水要**留着**。这是玩家给的家规：省 10 点以上才值得喝。
    #[test]
    fn a_potion_that_saves_a_little_is_held() {
        let s = with_potions(1, 80, 100, &[potion::BLOCK]);
        let mut t = Threat::new();
        t.set(0, 8, 1);
        let (dry, adv) = advise_potions(
            &s,
            &t,
            score::survive_first,
            10_000,
            &PotionPolicy::HOUSE_RULE,
        );
        assert!(!dry.line.is_empty(), "不喝药水也该有一条线");
        assert_eq!(adv.len(), 1);
        assert!(adv[0].hp_saved < 10, "这瓶只省了 {} 血", adv[0].hp_saved);
        assert_eq!(adv[0].verdict, PotionVerdict::Hold);
    }

    /// 不喝就会死的时候，门槛**不起作用**。
    /// 这条不需要特判：死亡在目标函数里就压倒一切，定价层只是照实报出来。
    #[test]
    fn a_potion_that_saves_your_life_ignores_the_threshold() {
        let s = with_potions(2, 12, 100, &[potion::BLOCK]);
        let mut t = Threat::new();
        t.set(0, 22, 1);
        let (_, adv) = advise_potions(
            &s,
            &t,
            score::survive_first,
            10_000,
            &PotionPolicy::HOUSE_RULE,
        );
        assert_eq!(adv[0].verdict, PotionVerdict::SaveMyLife);
        assert!(adv[0].hp_saved < 10, "省的血还不到门槛，但仍然该喝");
    }

    /// 跨回合药水**拒绝评分**，不是给个低分。
    /// 力量药水在单回合视角下只值 1 点出头的血，而一场五回合的仗它可能值
    /// 20 点伤害 —— 给一个自信的低分比不给分更危险。
    #[test]
    fn cross_turn_potions_are_refused_not_scored() {
        let s = with_potions(3, 80, 100, &[potion::STRENGTH, potion::DEXTERITY, potion::REGEN]);
        let mut t = Threat::new();
        t.set(0, 8, 1);
        let (_, adv) = advise_potions(
            &s,
            &t,
            score::survive_first,
            10_000,
            &PotionPolicy::HOUSE_RULE,
        );
        assert_eq!(adv.len(), 3);
        for a in &adv {
            assert_eq!(a.verdict, PotionVerdict::CrossTurn, "{} 不该被定价", a.name);
        }
    }

    /// 本幕最后一场（Boss）门槛归零：**别攥着药水死**。
    /// L2 不可能自己知道这件事，所以它是输入。
    #[test]
    fn the_last_fight_drops_the_reserve_to_zero() {
        let s = with_potions(4, 80, 100, &[potion::BLOCK]);
        let mut t = Threat::new();
        t.set(0, 8, 1);
        let boss = PotionPolicy { reserve_hp: 10, last_fight: true };
        let (_, adv) = advise_potions(&s, &t, score::survive_first, 10_000, &boss);
        assert_eq!(adv[0].verdict, PotionVerdict::Drink, "最后一场，省 1 点血也该喝");
    }

    /// 定价用的是「不喝药水的最优线」做基准，所以**搜索里真的没碰药水**。
    #[test]
    fn the_dry_line_never_touches_a_potion() {
        let s = with_potions(5, 40, 100, &[potion::BLOCK, potion::FIRE]);
        let mut t = Threat::new();
        t.set(0, 15, 1);
        let (dry, _) = advise_potions(
            &s,
            &t,
            score::survive_first,
            10_000,
            &PotionPolicy::HOUSE_RULE,
        );
        assert!(
            !dry.line.acts().iter().any(|a| matches!(a, Action::UsePotion { .. })),
            "基准线里不该出现药水"
        );
    }

    // -----------------------------------------------------------------------
    // 药水：三条「刻意的选择」必须有测试钉着
    //
    // 这三条的实现都是对的，但在此之前**一个测试都没有** —— 谁把
    // `use_potion` 重构一下（比如让它复用 `resolve_ops` 而不加 `Source`），
    // 三条会同时静默变错，而原有 5 个药水测试一个都不会红。
    // -----------------------------------------------------------------------

    /// **药水给的格挡不吃脆弱。** 这是实测过的（`act1_f14` 帧4：带脆弱 2
    /// 喝格挡药水仍得满 12），也是 `Source::Potion` 存在的头号理由。
    /// 同一帧里防御只给 3（5×3/4），两者放在一个测试里对比才看得出区别。
    #[test]
    fn potion_block_ignores_frail_but_card_block_does_not() {
        let mut s = hand_of(1, 100, &[card::DEFEND], 3);
        s.player.set(St::Frail, 2);
        s.potions[0] = potion::BLOCK;

        let a = step(s, Action::UsePotion { slot: 0, target: 0 });
        assert_eq!(a.player.block, 12, "药水不是牌，不吃脆弱");

        let b = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(b.player.block, 3, "卡牌的格挡照常吃脆弱：5×3/4=3");
    }

    /// **火焰药水攻防两侧的乘区一个都不吃，但仍然吃难以杀灭。**
    ///
    /// > 这个测试原来叫 `fire_potion_ignores_strength_but_still_respects_vulnerable`，
    /// > 断言的是"照常吃防御方的易伤：20×1.5=30"，注释里写着**（推断，未实测）**。
    /// > **那条推断是错的。** 2026-08-22 第2幕第27层精英实测：蜂群术士带易伤 2，
    /// > 火焰药水打出 **20** 而不是 30，对拍当场报 `游戏=19 内核=9`。
    /// > [源码] 坐实：易伤/虚弱/缓慢/缩小/巨像**五个乘区的第一句都是**
    /// > `if (!props.IsPoweredAttack()) return 1m;`，而药水是 `ValueProp.Unpowered`。
    /// >
    /// > 教训还是老那条：**标着"推断"的东西迟早会错，而且是安静地错**。
    /// > 一个测试钉住一条推断，只会让那条推断更难被推翻。
    #[test]
    fn fire_potion_takes_no_multipliers_but_still_respects_the_damage_cap() {
        let mut s = hand_of(2, 100, &[], 3);
        s.player.set(St::Strength, 5);
        s.potions[0] = potion::FIRE;
        let a = step(s, Action::UsePotion { slot: 0, target: 0 });
        assert_eq!(100 - a.enemies[0].hp, 20, "药水伤害不吃我的力量");

        let mut s2 = s;
        s2.enemies[0].set(St::Vulnerable, 1);
        let b = step(s2, Action::UsePotion { slot: 0, target: 0 });
        assert_eq!(100 - b.enemies[0].hp, 20, "[实测] 药水伤害**不吃**防御方的易伤");

        // 反过来：难以杀灭/无实体走的是 `ModifyDamageCap` / `ModifyHpLostAfterOsty`，
        // 源码里**没有** `IsPoweredAttack` 那条 gate，所以药水照吃。
        // **这一半还没有实测样本**（打精英时对面没有伤害上限），只有 [源码]。
        let mut s3 = s;
        s3.enemies[0].set(St::DamageCap, 9);
        let c = step(s3, Action::UsePotion { slot: 0, target: 0 });
        assert_eq!(100 - c.enemies[0].hp, 9, "[源码] 难以杀灭对药水一样有效");
    }

    /// **易伤药水不触发凶恶**（玩家判定 2026-08-16）。
    /// 卡面对这条是欠定的，问过玩家才定下来；对照组是闪电霹雳，
    /// 同样是"给易伤"，但它是牌，**要**触发。
    #[test]
    fn vulnerable_potion_does_not_wake_vicious_but_a_card_does() {
        let mut s = hand_of(3, 100, &[card::LIGHTNING], 3);
        for _ in 0..5 {
            s.add_card(card::DEFEND, 0, 0);
            s.draw[s.n_draw as usize] = s.n_cards - 1;
            s.n_draw += 1;
        }
        s.player.set(St::Vicious, 1);
        s.potions[0] = potion::VULNERABLE;

        let a = step(s, Action::UsePotion { slot: 0, target: 0 });
        assert_eq!(a.enemies[0].get(St::Vulnerable), 3, "易伤照常上");
        assert_eq!(a.n_hand, s.n_hand, "但凶恶不该为它抽牌");

        let b = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(b.n_hand, s.n_hand - 1 + 1, "闪电霹雳是牌：打出去一张、凶恶抽一张");
    }

    /// **药水不算牌**（实测）：不推进缓慢的累加，也不给踩踏减费。
    /// 这两条共用「药水不碰 `cards_played`/`attacks_played`」这一个前提。
    #[test]
    fn a_potion_is_not_a_card_for_slow_or_stomp() {
        let mut s = hand_of(4, 100, &[card::STAMPEDE], 3);
        s.enemies[0].set(St::SlowSource, 10);
        s.potions[0] = potion::BLOCK;
        s.potions[1] = potion::STRENGTH;

        let a = step(s, Action::UsePotion { slot: 0, target: 0 });
        let a = step(a, Action::UsePotion { slot: 1, target: 0 });
        assert_eq!(a.enemies[0].get(St::Slow), 0, "喝药水不推进缓慢");
        assert_eq!(a.attacks_played, 0, "药水不是攻击牌");
        assert_eq!(effective_cost(&a, 0), 3, "踩踏没有因为喝了两瓶药水而减费");
    }

    // -----------------------------------------------------------------------
    // 实战路径的每回合计数器（`replay::sync_latest`）
    //
    // 观测里没有这些计数器，单帧推不出来。对拍那边靠 `Replayer::counters`
    // 跨帧累加；实战这条线以前是单帧的，于是踩踏按满费算、怨恨按只打一次算。
    // -----------------------------------------------------------------------

    /// 踩踏减费：走完历史之后 `attacks_played` 必须是真实值。
    /// `synthetic_stomp_discount.json` 就是为这条造的（连打两张打击再打踩踏）。
    #[test]
    fn sync_latest_carries_the_attack_counter_for_stomp() {
        let src = std::fs::read_to_string("traces/synthetic_stomp_discount.json").unwrap();
        let t = crate::replay::parse_trace(&src).unwrap();
        // 停在「打出踩踏」那一帧之前 —— 要验的是它**打出去之前**看到的费用
        let k = t
            .frames
            .iter()
            .position(|f| {
                matches!(&f.action, Some(crate::replay::Act::Play { card_name, .. })
                    if card_name == "踩踏")
            })
            .expect("这条 trace 里应该有踩踏");
        let prefix = crate::replay::Trace {
            ascension: 0,
            version: t.version,
            run: t.run.clone(),
            frames: t.frames[..=k].to_vec(),
        };
        let (_, sy, used) = crate::replay::sync_latest(&prefix).unwrap();
        assert!(used >= 2, "该走过至少两帧历史，实际 {used}");
        assert_eq!(sy.state.attacks_played, 2, "本回合已打出两张攻击牌");
        // 踩踏 3 费，打过 2 张攻击牌 => 实际 1 费
        let stomp = (0..sy.state.n_hand as usize)
            .find(|&h| sy.names[sy.state.hand[h] as usize] == "踩踏")
            .expect("手牌里应该有踩踏");
        assert_eq!(effective_cost(&sy.state, stomp), 1);
    }

    /// 怨恨双击：绯红披风在**回合开始**扣的 1 点血属于新回合，
    /// 必须活着传到这一帧 —— 这正是实战里少算一次攻击的那个 bug。
    #[test]
    fn sync_latest_keeps_hp_loss_from_turn_start_powers() {
        let src = std::fs::read_to_string("traces/act2_f31_louse.json").unwrap();
        let t = crate::replay::parse_trace(&src).unwrap();
        // 找到「打出怨恨」那一帧，在它之前停下
        let k = t
            .frames
            .iter()
            .position(|f| {
                matches!(&f.action, Some(crate::replay::Act::Play { card_name, .. })
                    if card_name == "怨恨")
            })
            .expect("这条 trace 里应该有怨恨");
        let prefix = crate::replay::Trace {
            ascension: 0,
            version: t.version,
            run: t.run.clone(),
            frames: t.frames[..=k].to_vec(),
        };
        let (_, sy, _) = crate::replay::sync_latest(&prefix).unwrap();
        assert!(
            sy.state.hp_lost_this_turn > 0,
            "绯红披风回合开始扣的血必须带过来，否则怨恨只打一次"
        );
    }

    /// 只有一帧时**老老实实报 0 帧历史**，不假装知道。
    #[test]
    fn sync_latest_reports_when_there_is_no_history() {
        let src = std::fs::read_to_string("traces/act2_f31_louse.json").unwrap();
        let t = crate::replay::parse_trace(&src).unwrap();
        let one = crate::replay::Trace {
            ascension: 0,
            version: t.version,
            run: t.run.clone(),
            frames: vec![t.frames[0].clone()],
        };
        let (_, sy, used) = crate::replay::sync_latest(&one).unwrap();
        assert_eq!(used, 0);
        assert_eq!(sy.state.attacks_played, 0);
        assert_eq!(sy.state.hp_lost_this_turn, 0);
    }

    // -----------------------------------------------------------------------
    // 遗物表的完整性
    //
    // 和 `every_power_card_has_a_rule_and_no_rule_is_claimed_twice` 同一个用意：
    // 表里出现**哑条目**（挂了东西却没人认领、或者 id 拼错永远匹配不上）
    // 不加测试没人会发现，而它会静默地让内核少算一件遗物。
    // -----------------------------------------------------------------------

    /// id 不能重复 —— `relic_by_id` 取第一条，重复等于后一条永远不生效。
    #[test]
    fn relic_ids_are_unique() {
        let mut ids: Vec<&str> = RELICS.iter().map(|r| r.id).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n, "RELICS 里有重复 id");
    }

    /// 说自己没建模的，必须写清楚卡在哪 —— 否则下一个人无从判断该不该信它。
    /// 反过来，挂了 `start_status` 却标 `modelled: false` 是自相矛盾。
    #[test]
    fn every_unmodelled_relic_says_why() {
        for r in RELICS {
            if !r.modelled {
                assert!(!r.note.is_empty(), "{} 说没建模，却没写原因", r.name);
            }
            if !r.start_status.is_empty() {
                assert!(
                    r.modelled,
                    "{} 挂了开局 status 却标成没建模 —— 二者只能选一个",
                    r.name
                );
            }
        }
    }

    /// 表里的 id 必须真的是游戏用的那个 id。
    ///
    /// **拼错一个 id 是静默失效**：`relic_by_id` 永远查不到，遗物被当成
    /// 「内容表里没有」，而它明明就在表里。
    ///
    /// 权威有两个，**都要认**：
    ///
    /// * `traces/relics_catalog.json`（`dump_relics.py` 从游戏导的）——
    ///   但它是**快照**，只含"这个存档已经发现的"，会落后于实录。
    /// * **实录本身**：`relics[].id` 是游戏当场报出来的，比任何 dump 都新。
    ///
    /// 2026-09-06 建佩尔之血时这条测试当场红了：它不在 09-01 那份快照里，
    /// 而它明明躺在两条第 3 幕实录的 `relics[]` 里 —— **快照旧了，不是 id 错了**。
    /// 两处都找不到才是真的拼错。
    ///
    /// **第三个权威（2026-09-25）**：`data/relic_ids.json`，[源码] 档 ——
    /// 游戏自己算 id 的那个函数（`StringHelper.Slugify(类名)`）照抄出来的全表
    /// （`tools/dump_relic_ids.py`，它自检过游戏导出的每一个 id 都算得出来）。
    /// 为**预先补**的遗物加的：那一批 12 件这个存档一件都没捡到过，前两个权威都没有。
    #[test]
    fn relic_ids_exist_in_the_authoritative_catalog() {
        let Ok(src) = std::fs::read_to_string("traces/relics_catalog.json") else {
            return; // 权威表还没导出来就跳过，不因此挡住构建
        };
        let from_source = std::fs::read_to_string("data/relic_ids.json").unwrap_or_default();
        // 实录那一侧是**懒扫**：快照命中就不去读那几十兆 JSON。
        fn seen_in_a_trace(needle: &str) -> bool {
            let Ok(rd) = std::fs::read_dir("traces") else { return false };
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().map_or(false, |x| x == "json") {
                    if std::fs::read_to_string(&p).map_or(false, |s| s.contains(needle)) {
                        return true;
                    }
                }
            }
            false
        }
        for r in RELICS {
            let needle = format!("\"{}\"", r.id);
            assert!(
                src.contains(&needle) || from_source.contains(&needle) || seen_in_a_trace(&needle),
                "{}（{}）**三个权威都找不到**（导出的遗物表 + 源码 id 表 + 全部实录）
                 —— id 是不是拼错了？",
                r.name,
                r.id
            );
        }
    }

    // -----------------------------------------------------------------------
    // 遗物第 4 期：战斗内触发式
    // -----------------------------------------------------------------------

    fn no_incoming() -> crate::Incoming {
        [(0, 0); crate::state::MAX_ENEMIES]
    }

    /// **这一期最要紧的一条。** 奥利哈钢和覆甲都挂在回合结束，
    /// 而覆甲会给格挡 —— 压成一个钩子的话，奥利哈钢要么永远不触发，
    /// 要么触发与否取决于 `POWERS` 表里两行的先后（**隐式**，改表顺序就变）。
    ///
    /// 两段式的意思是：判定用**覆甲给格挡之前**的格挡值。
    ///
    /// 注意量的是**挨打之后的血**，不是 `player.block` —— 格挡在我的下一个
    /// 回合开始时就清零了，回合结束拿到的那点格挡只在敌人这一手里存在。
    #[test]
    fn orichalcum_snapshots_before_plating_armor_gives_block() {
        let inc = |n: i32| {
            let mut i = no_incoming();
            i[0] = (n, 1);
            i
        };
        // 基准：什么都没有，挨 6 点就掉 6 血
        let s = end_turn_with_incoming(hand_of(1, 100, &[], 3), &inc(6));
        assert_eq!(s.player.hp, 74);

        // 只有奥利哈钢：回合结束没格挡 -> +6，正好吃掉这 6 点
        let mut s = hand_of(1, 100, &[], 3);
        s.player.set(St::Orichalcum, 6);
        let s = end_turn_with_incoming(s, &inc(6));
        assert_eq!(s.player.hp, 80, "奥利哈钢那 6 点格挡该把这一下吃掉");

        // 奥利哈钢 + 覆甲 4：判定发生在覆甲给格挡**之前**，所以照样触发，
        // 一共 6 + 4 = 10 点格挡
        let mut s = hand_of(1, 100, &[], 3);
        s.player.set(St::Orichalcum, 6);
        s.player.set(St::PlatedArmor, 4);
        let s = end_turn_with_incoming(s, &inc(10));
        assert_eq!(s.player.hp, 80, "覆甲不该把奥利哈钢挤掉：6+4 该挡满 10");
    }

    /// 已经有格挡就不触发；而且**下一回合还能再触发**（武装标记要清干净）。
    #[test]
    fn orichalcum_skips_when_block_is_already_there_and_rearms_next_turn() {
        let inc = |n: i32| {
            let mut i = no_incoming();
            i[0] = (n, 1);
            i
        };
        let mut s = hand_of(1, 100, &[], 3);
        s.player.set(St::Orichalcum, 6);
        s.player.block = 3; // 回合结束时手上已经有格挡
        let s = end_turn_with_incoming(s, &inc(6));
        assert_eq!(s.player.hp, 77, "只有那 3 点格挡，剩下 3 点进血");
        assert_eq!(s.player.get(St::OrichalcumArmed), 0, "没触发就不该留下武装标记");

        // 下一回合格挡已清零，该正常触发
        let s = end_turn_with_incoming(s, &inc(6));
        assert_eq!(s.player.hp, 77, "这一回合奥利哈钢该挡满");
        assert_eq!(s.player.get(St::OrichalcumArmed), 0, "触发之后标记必须清掉");
    }

    /// 荆棘（我方）：挨一下攻击就反弹层数点给攻击者，**格挡挡住也照样反弹**。
    ///
    /// [源码] `Hook.BeforeDamageReceived` 在 `CreatureCmd` 里排在
    /// `DamageBlockInternal` **之前**，所以反弹和这一下有没有被挡住无关。
    /// 这条最容易写错成"真掉血才反弹"（那是百年积木的语义，不是荆棘的）。
    #[test]
    fn player_thorns_reflects_even_when_the_hit_is_fully_blocked() {
        let inc = |n: i32| {
            let mut i = no_incoming();
            i[0] = (n, 1);
            i
        };
        // 挡不住：反弹 3
        let mut s = hand_of(1, 100, &[], 3);
        s.player.set(St::Thorns, 3);
        let after = end_turn_with_incoming(s, &inc(6));
        assert_eq!(after.player.hp, 74, "6 点照常进血");
        assert_eq!(after.enemies[0].hp, 97, "反弹 3 点");

        // 全挡住：**照样**反弹 3
        let mut s = hand_of(1, 100, &[], 3);
        s.player.set(St::Thorns, 3);
        s.player.block = 20;
        let after = end_turn_with_incoming(s, &inc(6));
        assert_eq!(after.player.hp, 80, "格挡吃满");
        assert_eq!(after.enemies[0].hp, 97, "挡住了也要反弹 —— 钩子在扣格挡之前");
    }

    /// 荆棘按**每一段**触发，不是每张牌一次。多段攻击 5×3 要反弹三次。
    #[test]
    fn player_thorns_fires_once_per_hit_not_once_per_attack() {
        let mut s = hand_of(1, 100, &[], 3);
        s.player.set(St::Thorns, 3);
        let mut inc = no_incoming();
        inc[0] = (5, 3); // 5 点打三下
        let after = end_turn_with_incoming(s, &inc);
        assert_eq!(after.player.hp, 65, "15 点进血");
        assert_eq!(after.enemies[0].hp, 91, "3 段各反弹 3 = 9");
    }

    /// 荆棘（敌方）：我打它，它反弹给我。**打死它也照样反弹** ——
    /// [源码] 的钩子在伤害落地之前，所以致命一击也吃得到这一下。
    ///
    /// 这条是内核最容易和 `Hook::EnemyDamaged` 混掉的地方：那个钩子
    /// 「打死了就不触发」，而荆棘必须触发。两者的区别写在 `Hook::EnemyAttacked`。
    #[test]
    fn enemy_thorns_reflects_on_the_killing_blow_too() {
        // 蟾蜍带 5 层荆棘（`start_status`），血压到 3 点，一张打击（6）打死它
        let mut s = State::new(80, 7);
        s.add_enemy(enemy::SPINY_TOAD, 3);
        let c = s.add_card(card::STRIKE, 0, 0);
        let mut s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = c;
        s.n_draw = 0;
        s.energy = 3;
        s.enemies[0].set(St::Thorns, 5);

        let after = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert!(!after.enemies[0].alive(), "6 点打死 3 血的蟾蜍");
        assert_eq!(after.player.hp, 75, "致命一击照样吃 5 点反弹");
    }

    /// 敌方荆棘**只认攻击**：药水/遗物/能力牌那类 `Unpowered` 伤害不触发。
    /// [源码] 门是 `props.IsPoweredAttack()`，而 `IsPoweredAttack = Move && !Unpowered`。
    #[test]
    fn enemy_thorns_ignores_unpowered_damage() {
        let mut s = State::new(80, 7);
        s.add_enemy(enemy::SPINY_TOAD, 100);
        let mut s = begin_combat(s);
        s.n_hand = 0;
        s.n_draw = 0;
        s.energy = 3;
        s.enemies[0].set(St::Thorns, 5);
        s.potions[0] = crate::state::potion::FIRE;

        let after = step(s, Action::UsePotion { slot: 0, target: 0 });
        assert_eq!(after.enemies[0].hp, 80, "火焰药水 20 点照常打");
        assert_eq!(after.player.hp, 80, "药水不是攻击 —— 荆棘不该反弹");
    }

    /// **敌人身上的荆棘不会因为「我」挨打而发作。**
    ///
    /// `Hook::Attacked` 的语义是"我挨了一下攻击"（唯一触发点是玩家专用的
    /// `take_attack_hit`），所以它只该在玩家侧触发。不拦住的话，场上有蟾蜍时
    /// 我每挨一下，蟾蜍都会跟着反弹一次，而且反弹给打我的那只。
    #[test]
    fn enemy_thorns_does_not_fire_when_the_player_is_attacked() {
        let mut s = State::new(80, 7);
        s.add_enemy(enemy::DUMMY, 100); // 打我的那只
        s.add_enemy(enemy::SPINY_TOAD, 100); // 带 5 层荆棘的旁观者
        let mut s = begin_combat(s);
        s.n_hand = 0;
        s.n_draw = 0;
        s.enemies[1].set(St::Thorns, 5);

        let mut inc = no_incoming();
        inc[0] = (6, 1);
        let after = end_turn_with_incoming(s, &inc);
        assert_eq!(after.player.hp, 74, "只挨了 6 点");
        assert_eq!(after.enemies[0].hp, 100, "蟾蜍的荆棘不该打在沙包身上");
        assert_eq!(after.enemies[1].hp, 100, "蟾蜍自己也没挨打");
    }

    // ------------------------------------------------------------------
    // 2026-09-13 第 3 幕补敌人（批 1 + 批 2）：新机制各自的守卫。
    // 这六只一条实录都没有 —— **这些单测是它们今天唯一的检验面**。
    // ------------------------------------------------------------------

    /// 场上只有一只指定敌人、手里和牌堆全空（出招指针交给调用方）。
    fn lone(def: u16, hp: i32) -> State {
        let mut s = State::new(80, 13);
        s.add_enemy(def, hp);
        let mut s = begin_combat(s);
        s.n_hand = 0;
        s.n_draw = 0;
        s.n_disc = 0;
        s
    }

    /// 造一张牌**直接进手牌**（`add_card` 会顺手塞进抽牌堆，挪出来）。
    fn give(s: &mut State, id: u16) -> u8 {
        let c = s.add_card(id, 0, 0);
        s.n_draw -= 1;
        s.to_hand(c);
        c
    }

    fn cards_named(s: &State, id: u16) -> usize {
        (0..s.n_cards as usize).filter(|&i| s.cards[i].id == id).count()
    }

    /// 火焰喷射塞 4 张灼伤进**手牌**，**手满了溢出进弃牌堆，不是丢掉**。
    ///
    /// [源码] `CardPileCmd.Add`：`isFullHandAdd` 时 `targetPile = Discard`。
    /// 用均衡把 8 张手牌留过回合边界，敌人出手时手里正好还剩 2 格。
    #[test]
    fn mecha_knight_flamethrower_overflows_into_discard_when_the_hand_is_full() {
        let mut s = lone(enemy::MECHA_KNIGHT, 300);
        for _ in 0..8 {
            give(&mut s, card::DEFEND);
        }
        s.player.set(St::Entrench, 1);
        s.enemy_move[0] = 1; // 喷火器（定环，第 1 手）
        let s = step(s, Action::EndTurn);
        assert_eq!(cards_named(&s, card::BURN), 4, "4 张一张不少 —— 满了是溢出，不是不造");
        let in_hand = (0..s.n_hand as usize).filter(|&h| s.cards[s.hand[h] as usize].id == card::BURN).count();
        assert_eq!(in_hand, 2, "手里只剩 2 格，另外 2 张去了弃牌堆");
    }

    /// 遗忘之物：瘴气**先加格挡、后偷敏捷**，恐惧吃的是偷完之后的敏捷。
    ///
    /// [源码] `MiasmaMove` 三句的顺序 + `DreadDamage => 13 + 自己的敏捷` +
    /// `DexterityPower` 对怪物招式的格挡（`ValueProp.Move`）同样生效。
    /// 两圈下来：格挡 8 -> 10，恐惧 15 -> 17，我的敏捷 −2 -> −4。
    #[test]
    fn the_forgotten_block_and_dread_grow_with_the_dexterity_it_steals() {
        let mut s = lone(enemy::THE_FORGOTTEN, 106);
        let mut seen = vec![];
        for _ in 0..4 {
            s = step(s, Action::EndTurn);
            seen.push((s.enemies[0].block, s.enemies[0].get(St::Dexterity), s.player.get(St::Dexterity), s.player.hp));
        }
        assert_eq!(
            seen,
            vec![(8, 2, -2, 80), (0, 2, -2, 65), (10, 4, -4, 65), (0, 4, -4, 48)],
            "逐回合（它的格挡, 它的敏捷, 我的敏捷, 我的血）"
        );
    }

    /// 偷走的属性**只在小偷自己死时**还回来。
    ///
    /// [源码] `PossessStrengthPower.AfterDeath` 判 `creature == Owner`。
    /// 同场两只：先杀遗忘之物只还敏捷，力量要等失落之物死了才还。
    #[test]
    fn possess_returns_what_was_stolen_only_when_the_thief_itself_dies() {
        let mut s = State::new(80, 13);
        s.add_enemy(enemy::THE_LOST, 93);
        s.add_enemy(enemy::THE_FORGOTTEN, 106);
        let mut s = begin_combat(s);
        s.n_hand = 0;
        s.n_draw = 0;
        s = step(s, Action::EndTurn);
        assert_eq!((s.player.get(St::Strength), s.player.get(St::Dexterity)), (-2, -2), "各偷 2");

        // 力量 −2 的打击打 4 点：压到 1 血、清掉格挡，一刀一只
        s.enemies[1].hp = 1;
        s.enemies[1].block = 0;
        give(&mut s, card::STRIKE);
        s.energy = 3;
        let ix = hand_ix_of(&s, card::STRIKE);
        let mut s = step(s, Action::PlayCard { hand: ix, target: 1 });
        assert!(!s.enemies[1].alive());
        assert_eq!(s.player.get(St::Dexterity), 0, "遗忘之物死了 ⇒ 敏捷还回来");
        assert_eq!(s.player.get(St::Strength), -2, "力量是失落之物偷的，不该跟着还");

        s.enemies[0].hp = 1;
        s.enemies[0].block = 0;
        give(&mut s, card::STRIKE);
        let ix = hand_ix_of(&s, card::STRIKE);
        let s = step(s, Action::PlayCard { hand: ix, target: 0 });
        assert!(!s.enemies[0].alive());
        assert_eq!(s.player.get(St::Strength), 0, "失落之物死了 ⇒ 力量还回来");
    }

    /// 我的人工制品挡得住「偷力量」那一下，**而它照样给自己 +2**。
    ///
    /// [源码] `PowerModel.GetTypeForAmount`：`Counter` 且 `AllowNegative` 的 power
    /// 给负数就是 debuff。2026-09-13 之前敌人招式那条路只看 status 种类，
    /// 负力量直接穿过人工制品。
    #[test]
    fn player_artifact_blocks_the_strength_steal_but_the_thief_still_gains() {
        let mut s = lone(enemy::THE_LOST, 93);
        s.player.set(St::Artifact, 1);
        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.get(St::Strength), 0, "−2 被人工制品吃掉");
        assert_eq!(s.player.get(St::Artifact), 0, "吃掉一次掉一层");
        assert_eq!(s.enemies[0].get(St::Strength), 2, "它自己那 +2 和我挡没挡住无关");
    }

    /// 流电：打出**能力牌**挨 6 点、先吃格挡；技能牌不挨。
    #[test]
    fn galvanic_hurts_the_player_for_power_cards_only_and_goes_through_block() {
        let mut s = lone(enemy::GLOBE_HEAD, 148);
        give(&mut s, card::SHRUG_IT_OFF);
        give(&mut s, card::DEMON_FORM);
        s.energy = 10;
        let ix = hand_ix_of(&s, card::SHRUG_IT_OFF);
        let s = step(s, Action::PlayCard { hand: ix, target: 0 });
        assert_eq!((s.player.hp, s.player.block), (80, 8), "技能牌不触发流电");
        let ix = hand_ix_of(&s, card::DEMON_FORM);
        let s = step(s, Action::PlayCard { hand: ix, target: 0 });
        assert_eq!((s.player.hp, s.player.block), (80, 2), "能力牌挨 6 点，先吃格挡");
    }

    /// 纸伤难愈：**每打穿一段** −2 最大生命；被格挡完全吃掉的那一段不算；
    /// **只认它自己打穿的**。注入式威胁（L2 的结算路径）和真敌人回合走同一个
    /// `take_attack_hit`，两条都验。
    #[test]
    fn paper_cuts_costs_max_hp_per_unblocked_hit_of_its_own_on_both_turn_paths() {
        let mut s = lone(enemy::SCROLL_OF_BITING, 33);
        s.player.block = 5;
        let mut inc = no_incoming();
        inc[0] = (5, 2);
        let after = end_turn_with_incoming(s, &inc);
        assert_eq!(after.player.max_hp, 78, "第一段被 5 格挡吃光、不算；第二段打穿 −2");
        assert_eq!(after.player.hp, 75);

        let mut s = lone(enemy::SCROLL_OF_BITING, 33);
        s.enemy_move[0] = 1; // 咀嚼 5×2
        let after = step(s, Action::EndTurn);
        assert_eq!(after.player.max_hp, 76, "真敌人回合：两段都打穿 −4");
        assert_eq!(after.player.hp, 70);

        let mut s = State::new(80, 13);
        s.add_enemy(enemy::DUMMY, 100);
        s.add_enemy(enemy::SCROLL_OF_BITING, 33);
        let s = begin_combat(s);
        let mut inc = no_incoming();
        inc[0] = (6, 1);
        let after = end_turn_with_incoming(s, &inc);
        assert_eq!(after.player.max_hp, 80, "打穿我的是沙包，旁边卷轴的纸伤难愈不该发作");
    }

    /// 剧痛刺击：它每打穿一段塞 1 张伤口；被格挡吃光的那段不塞。
    /// 伤口可能在下回合开局就被洗回来抽进手里，所以数的是全场实例。
    #[test]
    fn painful_stabs_adds_one_wound_per_unblocked_hit() {
        let mut s = lone(enemy::TEST_SUBJECT_BOSS, 100);
        s.enemies[0].set(St::PainfulStabs, 1);
        s.player.block = 15;
        let mut inc = no_incoming();
        inc[0] = (10, 3);
        let after = end_turn_with_incoming(s, &inc);
        assert_eq!(cards_named(&after, card::WOUND), 2, "15 格挡：第一段吃光、第二段穿 5、第三段穿 10 ⇒ 2 张");
        assert_eq!(after.player.hp, 65);
    }

    /// 咬人卷轴按槽位错开起手（大啃 / 咀嚼 / 更多牙齿，第四卷写死更多牙齿），
    /// 以及咀嚼之后那个随机分支的 `CanRepeatXTimes(2)`。
    /// 固定 `num = 0` 那条近似的代价写在 `M_SCROLL_OF_BITING` 上。
    #[test]
    fn scrolls_of_biting_start_staggered_by_slot_and_chew_at_most_twice() {
        let mut s = State::new(80, 13);
        for _ in 0..4 {
            s.add_enemy(enemy::SCROLL_OF_BITING, 33);
        }
        let mut s = begin_combat(s);
        let def = enemy_def(enemy::SCROLL_OF_BITING);
        let starts: Vec<usize> = (0..4).map(|e| current_move_ix(def, &s, e)).collect();
        assert_eq!(starts, vec![0, 1, 2, 2]);

        s.enemy_move[0] = 1;
        assert_eq!(allowed_next(&s, 0), 0b011, "咀嚼之后：大啃，或者再咀嚼一次");
        s.enemy_hist[0][0] = 1;
        assert_eq!(allowed_next(&s, 0), 0b001, "已经连着咀嚼两次 ⇒ 只能大啃");
    }

    /// 整幕链要把最大生命带进下一场 —— 结局里先得有这个数。
    /// 5 张打击、3 点能量打不死 33 血的卷轴，第 1 回合的大啃 14 没有格挡可挡，必然打穿。
    #[test]
    fn rollout_outcome_reports_the_max_hp_the_fight_ended_with() {
        let mut s = State::new(80, 13);
        s.add_enemy(enemy::SCROLL_OF_BITING, 33);
        for _ in 0..5 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let o = crate::rollout::rollout_outcome(begin_combat(s), 40);
        assert!(!o.truncated, "{o:?}");
        assert!(o.final_max_hp <= 78, "至少被打穿一次 −2：{o:?}");
    }

    // ------------------------------------------------------------------
    // 2026-09-14 第 3 幕骑士团（批 3）：恶咒 / 抑制 / 回合末两段的顺序。
    // ------------------------------------------------------------------

    /// 回合末**先消耗虚无、后发作**（[源码] `CombatManager.DoTurnEnd`）。
    ///
    /// 晕眩被消耗 ⇒ 无惧疼痛给 3 格挡 ⇒ 灼伤那 2 点被挡住。
    /// 2026-09-14 之前是先发作后消耗，这一回合会掉 2 血。
    #[test]
    fn ethereal_exhausts_happen_before_turn_end_in_hand_effects() {
        let mut s = lone(enemy::DUMMY, 100);
        give(&mut s, card::DAZED);
        give(&mut s, card::BURN);
        s.player.set(St::FeelNoPain, 3);
        let after = end_turn_with_incoming(s, &no_incoming());
        assert_eq!(after.player.hp, 80, "无惧疼痛的格挡先到，灼伤被挡住");
        assert_eq!(after.n_exh, 1, "只有晕眩被消耗");
    }

    /// 恶咒：手里**每一张**没有回合末发作效果的牌都虚无 —— 连带保留的也消耗；
    /// 灼伤不算虚无（`HasTurnEndInHandEffect` 那一支先截走），照样烫人、照样进弃牌堆。
    #[test]
    fn hex_exhausts_every_card_left_in_hand_except_turn_end_effect_cards() {
        let mut s = lone(enemy::DUMMY, 100);
        s.player.set(St::Hex, 2);
        give(&mut s, card::STRIKE);
        let kept = give(&mut s, card::DEFEND);
        s.cards[kept as usize].flags |= F_RETAIN;
        give(&mut s, card::BURN);
        let after = end_turn_with_incoming(s, &no_incoming());
        let mut exh: Vec<u16> = (0..after.n_exh as usize).map(|i| after.cards[after.exh[i] as usize].id).collect();
        exh.sort();
        assert_eq!(exh, vec![card::STRIKE, card::DEFEND], "打击和带保留的防御都被消耗");
        assert_eq!(after.player.hp, 78, "灼伤没被恶咒消耗，照样烫 2 点");
    }

    /// 恶咒**只在幽灵骑士自己死时**解除；施咒者标记开局就挂好（合成路径）。
    #[test]
    fn hex_lifts_only_when_the_spectral_knight_itself_dies() {
        let mut s = State::new(80, 13);
        s.add_enemy(enemy::FLAIL_KNIGHT, 101);
        s.add_enemy(enemy::SPECTRAL_KNIGHT, 93);
        let mut s = begin_combat(s);
        s.n_hand = 0;
        s.n_draw = 0;
        assert_eq!(s.enemies[1].get(St::HexCaster), 1, "开局就挂好施咒者标记");
        assert_eq!(s.enemies[0].get(St::HexCaster), 0);
        s.player.set(St::Hex, 2);

        s.enemies[0].hp = 1;
        give(&mut s, card::STRIKE);
        s.energy = 3;
        let ix = hand_ix_of(&s, card::STRIKE);
        let mut s = step(s, Action::PlayCard { hand: ix, target: 0 });
        assert!(!s.enemies[0].alive());
        assert_eq!(s.player.get(St::Hex), 2, "死的是连枷骑士 ⇒ 不解咒");

        s.enemies[1].hp = 1;
        give(&mut s, card::STRIKE);
        let ix = hand_ix_of(&s, card::STRIKE);
        let s = step(s, Action::PlayCard { hand: ix, target: 1 });
        assert_eq!(s.player.get(St::Hex), 0, "幽灵骑士死了 ⇒ 解咒");
    }

    /// 连枷骑士 + 魔法骑士、弃牌堆一张升过级的打击和一张没升过的，魔法骑士下一手是抑制。
    fn magi_about_to_dampen(artifact: i32) -> (State, u8, u8) {
        let mut s = State::new(80, 13);
        s.add_enemy(enemy::FLAIL_KNIGHT, 101);
        s.add_enemy(enemy::MAGI_KNIGHT, 82);
        let mut s = begin_combat(s);
        s.n_hand = 0;
        s.n_draw = 0;
        s.n_disc = 0;
        let up = s.add_card(card::STRIKE, F_UPGRADED, 0);
        s.n_draw -= 1;
        s.to_discard(up);
        let plain = s.add_card(card::STRIKE, 0, 0);
        s.n_draw -= 1;
        s.to_discard(plain);
        s.enemy_move[1] = 1; // 抑制
        s.player.set(St::Artifact, artifact);
        (s, up, plain)
    }

    /// 抑制：本场已升级的牌降级并记下是谁；**魔法骑士自己死了**才升回去。
    #[test]
    fn dampen_downgrades_upgraded_cards_until_the_magi_knight_dies() {
        let (s, up, plain) = magi_about_to_dampen(0);
        let mut s = step(s, Action::EndTurn);
        assert_eq!(s.player.get(St::Dampen), 1);
        assert!(!s.cards[up as usize].upgraded(), "升过级的那张被降级");
        assert_ne!(s.cards[up as usize].flags & F_DAMPENED, 0, "并且记下了");
        assert_eq!(s.cards[plain as usize].flags & F_DAMPENED, 0, "没升过级的不记");

        s.enemies[1].hp = 1;
        s.enemies[1].block = 0;
        give(&mut s, card::STRIKE);
        s.energy = 3;
        let ix = hand_ix_of(&s, card::STRIKE);
        let s = step(s, Action::PlayCard { hand: ix, target: 1 });
        assert!(!s.enemies[1].alive());
        assert!(s.cards[up as usize].upgraded(), "魔法骑士死了 ⇒ 升回去");
        assert_eq!(s.cards[up as usize].flags & F_DAMPENED, 0);
        assert!(!s.cards[plain as usize].upgraded(), "本来就没升过的不会被白送一次升级");
        assert_eq!(s.player.get(St::Dampen), 0);
    }

    /// 人工制品挡掉抑制 ⇒ `AfterApplied` 不发作，一张牌都不降。
    #[test]
    fn artifact_blocks_dampen_and_nothing_is_downgraded() {
        let (s, up, _) = magi_about_to_dampen(1);
        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.get(St::Dampen), 0);
        assert_eq!(s.player.get(St::Artifact), 0, "吃掉一次掉一层");
        assert!(s.cards[up as usize].upgraded(), "没挂上就不降级");
    }

    /// 三只骑士的开局第一手和几个分支（[源码] 状态机）。
    #[test]
    fn knights_open_and_branch_like_the_source() {
        let mut s = State::new(80, 13);
        s.add_enemy(enemy::FLAIL_KNIGHT, 101);
        s.add_enemy(enemy::SPECTRAL_KNIGHT, 93);
        s.add_enemy(enemy::MAGI_KNIGHT, 82);
        let mut s = begin_combat(s);
        let starts: Vec<usize> = (0..3).map(|e| current_move_ix(enemy_def(s.enemy_def[e]), &s, e)).collect();
        assert_eq!(starts, vec![0, 0, 0], "连枷起手撞击 · 幽灵起手恶咒 · 魔法起手强力护盾");
        assert_eq!(allowed_next(&s, 0), 0b111, "连枷骑士撞完进三选一");
        assert_eq!(allowed_next(&s, 1), 0b010, "恶咒之后固定灵魂斩击");
        assert_eq!(allowed_next(&s, 2), 0b010, "强力护盾之后是抑制");
        s.enemy_hist[0][0] = 0;
        assert_eq!(allowed_next(&s, 0), 0b110, "已经连着撞了两次 ⇒ 不能再撞");
    }


    /// 选牌候选去重：弃牌堆里 4 张一样的打击只该岔出 1 条线。
    ///
    /// 头槌那类「从弃牌堆挑一张放到牌堆顶」在牌堆变厚之后一个节点能岔十几条，
    /// 而大半是同一张打击/防御。置换表接不住这件事 ——
    /// 两个副本是 `s.cards` 里不同的下标，选完之后状态不逐字节相同。
    #[test]
    fn card_choices_collapse_identical_copies() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::DUMMY, 100);
        s.n_draw = 0;
        // 弃牌堆：打击 ×4、防御 ×2、升级过的打击 ×1
        for _ in 0..4 {
            let c = s.add_card(card::STRIKE, 0, 0);
            s.n_draw = 0;
            s.to_discard(c);
        }
        for _ in 0..2 {
            let c = s.add_card(card::DEFEND, 0, 0);
            s.n_draw = 0;
            s.to_discard(c);
        }
        let up = s.add_card(card::STRIKE, crate::state::F_UPGRADED, 0);
        s.n_draw = 0;
        s.to_discard(up);
        assert_eq!(s.n_disc, 7);

        s.pending = Pending::DiscardToDrawTop { remaining: 1, exclude: u8::MAX };
        let (_, n) = legal_actions(&s);
        assert_eq!(n, 3, "7 张塌成 3 类：打击 / 防御 / 打击+");
    }

    /// **升级过的和没升级的不是同一张。** 光比 `id` 会把它们合掉，
    /// 那是真丢解 —— 把升级过的放到牌堆顶和把没升级的放上去不是一回事。
    #[test]
    fn card_choices_do_not_merge_across_upgrade_or_bonus() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::DUMMY, 100);
        let a = s.add_card(card::STRIKE, 0, 0);
        let b = s.add_card(card::STRIKE, crate::state::F_UPGRADED, 0);
        let c = s.add_card(card::STRIKE, 0, 5); // 本场 +5 伤害（暴走那类）
        s.n_draw = 0;
        s.n_hand = 3;
        s.hand[0] = a;
        s.hand[1] = b;
        s.hand[2] = c;
        s.pending = Pending::ExhaustFromHand { remaining: 1 };
        let (_, n) = legal_actions(&s);
        assert_eq!(n, 3, "id 相同但 flags/bonus 不同 ⇒ 三张各算一类");
    }

    /// 仪式：**施加它的那一个回合末不结算**，之后每个敌人回合末 +Amount 力量，
    /// 而且层数不减 ⇒ 攻击力永久爬升。
    ///
    /// 复刻虔诚雕刻师的真实时间线（禁忌咒语 9 层 + 狂暴 12）：
    /// 第 1 回合施加、第 2 回合打 12、第 3 回合 21、第 4 回合 30。
    /// **第 2 回合那个 12 就是 `WasJustAppliedByEnemy` 那条**（[源码]）——
    /// 少了它会变成 21，一整回合的伤害差。
    #[test]
    fn ritual_skips_the_turn_it_was_applied_then_ramps_every_turn() {
        let mut s = State::new(200, 11);
        s.add_enemy(enemy::DEVOTED_SCULPTOR, 162);
        let s = begin_combat(s);

        // **必须走 `step(EndTurn)`（内核自己掷招），不能用注入** ——
        // 注入路径直接给伤害、不让敌人打出自己的招，禁忌咒语根本不会发生，
        // 仪式也就挂不上。写这条测试时当场踩到了。
        //
        // 回合 1：禁忌咒语（Buff，0 伤害），回合末**不**结算
        let s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[0].get(St::Ritual), 9, "9 层仪式挂上了");
        assert_eq!(s.enemies[0].get(St::Strength), 0, "施加当回合末不转化");

        // 回合 2：狂暴，此时力量还是 0 ⇒ 打 12。回合末才 +9
        let s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[0].get(St::Strength), 9, "第二个回合末开始转化");
        // 回合 3 末：再 +9
        let s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[0].get(St::Strength), 18, "层数不减，每回合都 +9");
        assert_eq!(s.enemies[0].get(St::Ritual), 9, "仪式本身不消耗");
    }

    /// 攻击力真的按 12 -> 21 -> 30 爬。
    /// 用 `enemy_turn`（内核自己掷招）而不是注入，才看得到伤害本身。
    #[test]
    fn devoted_sculptor_damage_ramps_by_nine_each_turn() {
        let mut s = State::new(400, 11);
        s.add_enemy(enemy::DEVOTED_SCULPTOR, 162);
        let s = begin_combat(s);
        let mut hp = s.player.hp;
        let mut got = Vec::new();
        let mut s = s;
        for _ in 0..4 {
            s = step(s, Action::EndTurn);
            got.push(hp - s.player.hp);
            hp = s.player.hp;
        }
        assert_eq!(got, vec![0, 12, 21, 30], "禁忌咒语(0) -> 狂暴 12 -> 21 -> 30");
    }

    /// 击晕：被击晕的敌人**这一手什么都不做**，两种威胁口径下都要生效。
    ///
    /// 注入路径（`end_turn_with_incoming`）那一条最容易漏：注入的伤害来自
    /// 同步那一刻的观测意图，而击晕是我这回合打出去之后才发生的。
    /// 不在 `injected_enemy_turn` 里拦一道，求解器就会以为吹哨白花了 3 费。
    #[test]
    fn stun_skips_the_enemy_action_in_both_threat_modes() {
        let inc = |n: i32| {
            let mut i = no_incoming();
            i[0] = (n, 1);
            i
        };
        // 冻住的意图（live=false）
        let mut s = hand_of(1, 100, &[], 3);
        s.enemies[0].set(St::Stunned, 1);
        let a = end_turn_with_incoming(s, &inc(20));
        assert_eq!(a.player.hp, 80, "被击晕 ⇒ 这一手不打");
        assert_eq!(a.enemies[0].get(St::Stunned), 0, "一次性，用完就清");

        // 面板基础值现算（live=true）
        let mut s = hand_of(1, 100, &[], 3);
        s.enemies[0].set(St::Stunned, 1);
        let a = crate::step::end_turn_with_live_incoming(s, &inc(20));
        assert_eq!(a.player.hp, 80, "现算口径同样要认");
    }

    /// 吹哨：先 33 点伤害，**再**击晕。
    #[test]
    fn whistle_hits_then_stuns() {
        let s = hand_of(1, 100, &[card::WHISTLE], 3);
        let a = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(a.enemies[0].hp, 67, "33 点");
        assert_eq!(a.enemies[0].get(St::Stunned), 1, "挂上击晕");
        assert_eq!(a.n_exh, 1, "消耗");
    }

    /// **斩杀的那一下，击晕是白给的。**
    ///
    /// [源码] `StunInternal` 第一句就是 `if (CombatState != null && !IsDead)` ——
    /// 目标已经死了就什么都不做。把顺序写反（先击晕再伤害）会凭空多出
    /// 一个"打死了还留着击晕"的收益，而那个收益不存在。
    #[test]
    fn whistle_stun_is_wasted_on_a_killing_blow() {
        let s = hand_of(1, 20, &[card::WHISTLE], 3);
        let a = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert!(!a.enemies[0].alive(), "20 血被 33 点打死");
        assert_eq!(a.enemies[0].get(St::Stunned), 0, "死人身上不该留下击晕标记");
    }

    /// 醒来之后**回到上一次真正打出的那一手**，被顶掉的那一手是**丢了**。
    ///
    /// [源码] `FollowUpStateId = stateLog.Last().Id` —— 读的是已经打出过的那一手。
    /// 这条卡面读不出来，也是最容易写成"击晕只是推迟一回合"的地方。
    ///
    /// 用定环的高塔炮手验：出招表是 倾泻火力(0) -> 倾泻火力2(1) -> 装填(2)。
    /// 打完第 0 手之后指针指向 1；这时击晕 ⇒ 跳过一手，醒来再打第 0 手。
    #[test]
    fn stun_rewinds_to_the_last_move_actually_performed() {
        let mut s = State::new(80, 5);
        s.add_enemy(enemy::TURRET_OPERATOR, 100);
        let s = begin_combat(s);
        // 第一个敌人回合：打出第 0 手，指针推到 1
        let s = end_turn_with_incoming(s, &no_incoming());
        assert_eq!(s.enemy_move[0], 1, "打完第 0 手，下一手是第 1 手");

        // 击晕：这一手跳过，指针退回"上一次打出的"= 0
        let mut s = s;
        s.enemies[0].set(St::Stunned, 1);
        let s = end_turn_with_incoming(s, &no_incoming());
        assert_eq!(s.enemy_move[0], 0, "醒来之后重打第 0 手 —— 第 1 手被顶掉了，不是推迟");

        // 再过一个回合：真的又打了一次第 0 手
        let s = end_turn_with_incoming(s, &no_incoming());
        assert_eq!(s.enemy_move[0], 1);
    }

    /// 还没出过手就被击晕：没有"上一手"可回，**保持指针不动**。
    ///
    /// 源码那边这时 `stateLog` 里是初始状态，语料里一个样本都没有 ——
    /// 按规矩**不猜**，取"至少不会凭空多跳一手"的那边。
    /// 有样本了再来改这条。
    #[test]
    fn stun_before_any_move_keeps_the_pointer() {
        let mut s = State::new(80, 5);
        s.add_enemy(enemy::TURRET_OPERATOR, 100);
        let mut s = begin_combat(s);
        let before = s.enemy_move[0];
        s.enemies[0].set(St::Stunned, 1);
        let s = end_turn_with_incoming(s, &no_incoming());
        assert_eq!(s.enemy_move[0], before, "没有上一手可回，指针不动");
    }

    /// 坚定不移：**每回合前 N 次**从卡牌获得的格挡翻倍，不是"永远只有第一次"。
    ///
    /// 卡面写的是"每回合第一次"，[源码] `UnmovablePower.ModifyBlockMultiplicative`
    /// 数的是本回合此前获得过几次（`num >= Amount` 才停）——
    /// 所以**两张就是前两次都翻倍**。这条只有源码看得出来。
    #[test]
    fn unmovable_doubles_the_first_n_card_blocks_each_turn_not_just_the_first() {
        // 一层：第一张翻倍，第二张不翻。
        // **不用手工设 `UnmovableCharge`** —— 它是"本回合已经拿过几次卡牌格挡"
        // 的计数器，回合开头本来就是 0。
        let mut s = hand_of(1, 100, &[card::DEFEND, card::DEFEND], 3);
        s.player.set(St::Unmovable, 1);
        let s1 = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s1.player.block, 10, "防御 5 翻倍 = 10");
        let s2 = step(s1, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s2.player.block, 15, "第二张只给 5，余额已经用完");

        // 两层：**前两张都翻倍**
        let mut s = hand_of(1, 100, &[card::DEFEND, card::DEFEND, card::DEFEND], 3);
        s.player.set(St::Unmovable, 2);
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.player.block, 20, "两张各 10");
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.player.block, 25, "第三张恢复原价");
    }

    /// 坚定不移**回合中途才拿到也立刻生效**，计数器每回合清零。
    ///
    /// 前半条是 2026-08-30 第 3 幕第 43 层实录**帧 18 的真错**：
    /// 那一回合我先打了坚定不移+ 再打挑衅+，游戏给 12 点格挡
    /// （8 × 2 翻倍 × 0.75 脆弱），内核只给 6 —— 因为内核当时把它做成
    /// 「我的回合开始把余额上膛」，回合中途拿到的那一回合余额恒为 0。
    /// [源码] 数的是本回合此前拿过几次卡牌格挡，是个**往上数**的计数器。
    ///
    /// **这一帧以前一直被跳过**：青蛙骑士不在内容表里 ⇒ 整帧判「内容缺失」。
    /// 补完那只敌人才露出来 —— 内容覆盖率 ≠ 行为覆盖率的又一例。
    #[test]
    fn unmovable_works_when_gained_mid_turn_and_the_counter_resets_each_turn() {
        // 回合中途才拿到：这一回合的第一张格挡牌就该翻倍
        let mut s = hand_of(1, 100, &[card::DEFEND], 3);
        s.player.set(St::Unmovable, 1);
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.player.block, 10, "回合中途拿到坚定不移，当回合就该翻倍");
        assert_eq!(s.player.get(St::UnmovableCharge), 1, "计数器往上数");

        // 计数器在我的回合开始无条件清零（它在 `TURN_SCOPED` 里）。
        // 不清的话，上一回合数到的次数会把下一回合的翻倍吃掉。
        let mut s = hand_of(1, 100, &[], 3);
        s.player.set(St::Unmovable, 1);
        s.player.set(St::UnmovableCharge, 5);
        let s = end_turn_with_incoming(s, &no_incoming());
        assert_eq!(s.player.get(St::UnmovableCharge), 0, "每回合清零");
    }

    /// 坚定不移只吃**从卡牌**来的格挡：药水和能力牌给的都不翻倍。
    /// 门是 [源码] 的 `props.IsCardOrMonsterMove()`，内核里对应
    /// `damage::card_block` 这个唯一入口 —— 和臂甲一样是自动成立的。
    #[test]
    fn unmovable_ignores_potion_and_power_block() {
        let mut s = hand_of(1, 100, &[], 3);
        s.player.set(St::Unmovable, 1);
        s.potions[0] = crate::state::potion::BLOCK;
        let after = step(s, Action::UsePotion { slot: 0, target: 0 });
        assert_eq!(after.player.block, 12, "格挡药水给 12，不翻倍");
        assert_eq!(after.player.get(St::UnmovableCharge), 0, "药水格挡也不该被计数");
    }

    /// 盾墙：**我的**回合开始时给场上所有高塔炮手加格挡。
    ///
    /// [源码] `RampartPower.AfterSideTurnStart` 筛的是
    /// `c.Monster is TurretOperator` —— **写死的怪物种类，不是"所有队友"**。
    /// 所以别的队友一点都拿不到，而杀掉活体盾就等于拆掉炮手的格挡引擎。
    #[test]
    fn rampart_only_shields_turret_operators() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::LIVING_SHIELD, 55);
        s.add_enemy(enemy::TURRET_OPERATOR, 41);
        s.add_enemy(enemy::DUMMY, 100); // 另一种队友，不该拿到格挡
        // **别手工挂盾墙**：`begin_combat` 会从 `EnemyDef::start_status` 自动挂 25，
        // 再 set 一次就变成 50 —— 写这条测试时当场踩到了。
        let s = begin_combat(s);
        // 打完一个回合，回到我的回合开始
        let s = end_turn_with_incoming(s, &no_incoming());
        assert_eq!(s.enemies[1].block, 25, "高塔炮手拿到 25");
        assert_eq!(s.enemies[2].block, 0, "别的队友一点都没有");
    }

    /// 盾墙的引擎在**活体盾**身上：它一死就不再给格挡了。
    /// 这一条决定第 3 幕那一场先杀谁 —— 2026-08-30 实战就是照这个打的。
    #[test]
    fn rampart_stops_when_its_owner_dies() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::LIVING_SHIELD, 55);
        s.add_enemy(enemy::TURRET_OPERATOR, 41);
        let mut s = begin_combat(s);
        s.enemies[1].block = 0; // 把开局那一次给的清掉，只看死后还给不给
        s.enemies[0].hp = 0; // 活体盾死了
        let s = end_turn_with_incoming(s, &no_incoming());
        assert_eq!(s.enemies[1].block, 0, "引擎没了，炮手不再回盾");
    }

    /// 啄击升级加的是**段数**（2×3 -> 2×4），不是伤害。
    /// 段数不是装饰：荆棘/火焰屏障按段触发，缓慢也按段累加。
    #[test]
    fn peck_upgrade_adds_a_hit_not_damage() {
        let s = hand_of(1, 100, &[card::PECK], 3);
        let a = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(a.enemies[0].hp, 94, "2×3 = 6");

        let mut s = hand_of(1, 100, &[], 3);
        let c = s.add_card(card::PECK, crate::state::F_UPGRADED, 0);
        s.n_draw = 0;
        s.n_hand = 1;
        s.hand[0] = c;
        let a = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(a.enemies[0].hp, 92, "升级后 2×4 = 8");
    }

    /// 毒素**能打出来**（1 费消耗掉自己），留在手上则回合末吃 5 点、**过格挡**。
    /// 可打出这一条和孢子心灵同源：只带 `Exhaust`、不带 `Unplayable`。
    #[test]
    fn toxic_can_be_played_to_exhaust_itself_and_hurts_through_block_if_kept() {
        // 打出去：消耗掉，不掉血
        let s = hand_of(1, 100, &[card::TOXIC], 3);
        let a = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(a.n_hand, 0);
        assert_eq!(a.n_exh, 1, "进消耗堆");
        assert_eq!(a.player.hp, 80, "打出去不掉血");

        // 留在手上：回合末 5 点，**先被格挡吃**
        let mut s = hand_of(1, 100, &[card::TOXIC], 3);
        s.player.block = 3;
        let a = end_turn_with_incoming(s, &no_incoming());
        assert_eq!(a.player.hp, 78, "5 点里 3 点被格挡吃掉，进血 2");
    }

    /// 箭雨的"保留手牌"和均衡是**同一个** power（[源码] 两张都
    /// `PowerCmd.Apply<RetainHandPower>`）—— 卡面措辞不同，源码才看得出来。
    #[test]
    fn salvo_retains_the_hand_with_the_same_power_equilibrium_uses() {
        let s = hand_of(1, 100, &[card::SALVO, card::STRIKE, card::DEFEND], 3);
        let a = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(a.enemies[0].hp, 88, "12 点");
        assert_eq!(a.player.get(St::Entrench), 1, "挂的就是均衡那个 status");
        // **用 `end_turn_before_draw`**：`end_turn_with_incoming` 会一路跑进抽牌，
        // 而抽牌会把手牌张数补回去 —— 保留和不保留最后都是 3 张，分不开。
        // 停在抽牌之前才看得到"手牌没被弃掉"这件事本身。
        let b = crate::step::end_turn_before_draw(a);
        assert_eq!(b.n_hand, 2, "手上那两张保留了，没进弃牌堆");
        assert_eq!(b.n_disc, 1, "弃牌堆里只有打出去的箭雨");
    }

    /// 精致折扇：[源码] 是 `% 3 == 0`，**第 6 张也给** —— 卡面读不出来这一条。
    #[test]
    fn ornamental_fan_fires_on_every_third_attack_not_just_the_third() {
        let cards = [card::STRIKE; 6];
        let mut s = hand_of(5, 500, &cards, 99);
        s.player.set(St::OrnamentalFan, 4);
        let mut got = Vec::new();
        for _ in 0..6 {
            s = step(s, Action::PlayCard { hand: 0, target: 0 });
            got.push(s.player.block);
        }
        assert_eq!(got, vec![0, 0, 4, 4, 4, 8], "第 3 张和第 6 张各给一次");
    }

    /// 灯笼：[源码] 是 `TurnNumber <= 1`。第 2 回合不该再给。
    #[test]
    fn lantern_only_pays_on_the_first_turn() {
        let mut s = hand_of(1, 100, &[], 3);
        s.player.set(St::Lantern, 1);
        s.turn = 1;
        let e1 = s.energy;
        let s = end_turn_with_incoming(s, &no_incoming());
        assert_eq!(s.turn, 2);
        assert_eq!(s.energy, e1, "第 2 回合不该拿到灯笼的能量");
    }

    /// 历石：只在第 7 回合结束打，别的回合一点不打。
    #[test]
    fn stone_calendar_fires_only_on_turn_seven() {
        let mut s = hand_of(1, 200, &[], 3);
        s.player.set(St::StoneCalendar, 52);
        s.turn = 6;
        let s = end_turn_with_incoming(s, &no_incoming());
        assert_eq!(s.enemies[0].hp, 200, "第 6 回合结束不该打");
        let s = end_turn_with_incoming(s, &no_incoming());
        assert_eq!(s.enemies[0].hp, 148, "第 7 回合结束打 52");
    }

    /// 摆动球的**相位跨战斗保留**（[源码] `TurnsSeen` 带 `SavedProperty`）。
    /// 相位 0 时在第 3 回合抽；相位 1 时提前到第 2 回合。
    ///
    /// 假设"每场都从 0 开始"就会系统性错一个回合 —— 那是自信地算错。
    #[test]
    fn pendulum_phase_comes_from_the_relic_counter_not_from_zero() {
        let draws_on = |phase: i32| -> Vec<i32> {
            let mut s = State::new(80, 9);
            s.add_enemy(enemy::DUMMY, 100);
            for _ in 0..12 {
                s.add_card(card::STRIKE, 0, 0);
            }
            let mut s = begin_combat(s);
            s.player.set(St::Pendulum, 1);
            s.player.set(St::PendulumPhase, phase);
            let mut fired = Vec::new();
            for _ in 0..4 {
                let before = s.n_hand;
                let t = s.turn;
                s = end_turn_with_incoming(s, &no_incoming());
                // 回合开始抽 5 张是基础抽牌，摆动球多抽 1 张
                if s.n_hand as i32 > before as i32 {
                    let _ = t;
                }
                fired.push(s.n_hand as i32);
            }
            fired
        };
        let p0 = draws_on(0);
        let p1 = draws_on(1);
        assert_ne!(p0, p1, "相位不同，抽牌的回合就该不同");
    }

    // -----------------------------------------------------------------------
    // 敌人出招机器（随机权重 / 条件分支 / 允许集合）
    // -----------------------------------------------------------------------

    fn with_enemy(def: u16, hp: i32) -> State {
        let mut s = State::new(80, 3);
        s.add_enemy(def, hp);
        begin_combat(s)
    }

    fn def_id_of(name: &str) -> u16 {
        (0..ENEMIES.len() as u16).find(|&i| crate::content::enemy_def(i).name == name).unwrap()
    }

    // -----------------------------------------------------------------------
    // 对拍层：两条"观测里没有、但卡面文本里有"的量
    // -----------------------------------------------------------------------

    /// 痛殴攒下来的加值从卡面反推。**渲染值含攻击方乘区、不含防御方乘区**
    /// （实测两帧钉的），所以反推的办法是拿同一条管线**正着算一遍试过去**。
    #[test]
    fn thrash_bonus_is_read_back_from_the_card_text() {
        let bonus = |desc: &str, me: &Entity| crate::replay::observed_thrash_bonus(desc, true, me, false);
        let mut me = Entity::new(80);
        // 帧3 实测：渲染 7、力量 1、还没吃过牌 ⇒ 加值 0
        me.set(St::Strength, 1);
        assert_eq!(bonus("造成7点伤害两次。 消耗你的手牌中……", &me), Some(0));
        // 吃过一张 16 点的巨石之后
        assert_eq!(bonus("造成23点伤害两次。", &me), Some(16));
        // **帧29 实测**：力量 1 + 虚弱 1，渲染 13 ⇒ ⌊(6+B+1)×3/4⌋ = 13 ⇒ B = 11。
        // 那一帧真吃掉的正是剑柄打击+（渲染 11 点），两边对得上。
        // 2026-09-06 之前这里返回 `None`（"除不回去"），红了一帧。
        me.set(St::Weak, 1);
        assert_eq!(bonus("造成13点伤害两次。", &me), Some(11));
        // **欠定时取最小解**：⌊v×3/4⌋ = 15 的 v 有 20 和 21 两个 ⇒ 取 13 不取 14
        assert_eq!(bonus("造成15点伤害两次。", &me), Some(13));
        // 读不出数字仍然拒绝作答
        assert_eq!(bonus("没有数字", &me), None);
        // 渲染值比任何加值都算不出来（这句话不是我以为的那句）⇒ 拒绝作答
        me.set(St::Weak, 0);
        assert_eq!(bonus("造成2点伤害两次。", &me), None);
    }

    /// 钢笔尖到 9 时，**手牌里的攻击牌渲染的是翻倍后的数** ——
    /// 不认这件事，反推出来的加值会正好多出一个卡面基础值那么多。
    ///
    /// [实测] 同一场帧31/32：同样 6 点格挡的全身撞击+，计数 8 渲染 5、计数 9 渲染 10。
    #[test]
    fn thrash_bonus_accounts_for_the_pen_nib_preview() {
        let mut me = Entity::new(80);
        me.set(St::Strength, 1);
        // 没翻倍：渲染 7 ⇒ 加值 0
        assert_eq!(crate::replay::observed_thrash_bonus("造成7点伤害两次。", true, &me, false), Some(0));
        // 翻倍：同一张牌渲染的是 14，加值仍然是 0
        assert_eq!(crate::replay::observed_thrash_bonus("造成14点伤害两次。", true, &me, true), Some(0));
        // 不认翻倍的话，14 会被读成"加值 7"
        assert_eq!(crate::replay::observed_thrash_bonus("造成14点伤害两次。", true, &me, false), Some(7));
    }

    /// **身份被就地改写过的牌，名字要跟着走。**
    ///
    /// `Op::TransformAttacksInHand`（原始力量）是唯一改 `CardInst.id` 的 op，
    /// 而对拍的名字表是同步那一刻的快照 —— 不认这件事就会报一条**假红**
    /// （游戏 3 张巨石+，内核报的还是原来那三张攻击牌）。
    #[test]
    fn a_card_rewritten_in_place_reports_its_new_name() {
        // 第二张要挑一张**内容表今天真没有的**牌（不然 `lookup_card` 找得到，
        // 就会被当成"身份被改写过"）。彼岸咆哮 2026-09-06 建掉了，换成许愿。
        let names = vec!["完美打击".to_string(), "许愿".to_string()];
        // 0 号被原始力量+ 改写成巨石+；1 号是内容表没有的牌，没被改写
        let cards = [
            CardInst {
                id: card::BOULDER,
                flags: F_UPGRADED,
                bonus: 0,
                cost_delta: 0,
                ench: 0,
                ench_amt: 0,
            },
            CardInst { id: card::UNKNOWN, flags: 0, bonus: 0, cost_delta: 0, ench: 0, ench_amt: 0 },
        ];
        assert_eq!(crate::replay::current_card_name(&names, &cards, 0), "巨石+");
        assert_eq!(
            crate::replay::current_card_name(&names, &cards, 1),
            "许愿",
            "没被改写的未知牌必须留着快照名字 —— 那正是快照存在的理由"
        );
    }

    // -----------------------------------------------------------------------
    // 幻象：杀不掉的爪牙（胧光怪的寄生惧魔 / 雾菇的利齿之眼）
    // -----------------------------------------------------------------------

    /// 胧光怪 + 它召出来的寄生惧魔，跳过召唤那一手直接摆好。
    fn obscura_pair() -> State {
        let mut s = State::new(80, 11);
        s.add_enemy(enemy::THE_OBSCURA, 123);
        s.add_enemy(enemy::PARAFRIGHT, 21);
        for _ in 0..10 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        // 本体跳过「幻象」那一手（这几个测试量的是幻象本身，不是召唤）
        s.enemy_move[0] = 1;
        s
    }

    /// **幻象砍死了不算完**：尸体留在场上，它自己回合开始时回满血。
    ///
    /// [源码] `IllusionPower`：`ShouldCreatureBeRemovedFromCombatAfterDeath => false`
    /// + `AfterDeath` -> `SetMoveImmediate(REVIVE_MOVE)` -> `Heal(MaxHp - CurrentHp)`。
    /// [实测] `act1_f15_ninth`：利齿之眼帧5 被打死、帧6/7 观测里消失、帧8 6/6 回来，
    /// 而雾菇那两回合的意图**不是** Summon —— 没有第二次召唤。
    #[test]
    fn an_illusion_minion_revives_at_full_hp_on_its_own_turn() {
        let mut s = obscura_pair();
        // 用真的一刀砍死它 —— `Hook::EnemyDied` 只在伤害落地那条路上发
        s.enemies[1].hp = 1;
        let ix = hand_ix_of(&s, card::STRIKE);
        let s = step(s, Action::PlayCard { hand: ix, target: 1 });
        assert!(!s.enemies[1].alive(), "砍死那一帧它还是死的");
        assert!(!s.combat_over, "本体还活着，战斗不该结束");
        assert_eq!(s.enemies[1].get(St::Illusion), 1, "幻象标记要留着，不然复活就没了");

        let s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[1].hp, 21, "它自己回合开始时回满");
        // 复苏那一手不打人：玩家只该挨本体那一下
        assert!(s.player.hp > 0);
    }

    /// **死亡剥离的方向和适生力正好相反**：buff 留着，我上的 debuff 被剥掉。
    /// [源码] `ShouldPowerBeRemovedOnDeath` = `Type == Debuff && !(power is ITemporaryPower)`。
    #[test]
    fn illusion_death_keeps_its_buffs_but_strips_my_debuffs() {
        let mut s = obscura_pair();
        s.enemies[1].set(St::Strength, 3);
        s.enemies[1].set(St::Vulnerable, 2);
        s.enemies[1].set(St::Weak, 2);
        s.enemies[1].hp = 1;
        let ix = hand_ix_of(&s, card::STRIKE);
        let s = step(s, Action::PlayCard { hand: ix, target: 1 });
        assert_eq!(s.enemies[1].get(St::Strength), 3, "力量是 buff，源码明说留着");
        assert_eq!(s.enemies[1].get(St::Vulnerable), 0, "非临时 debuff 被剥掉");
        assert_eq!(s.enemies[1].get(St::Weak), 0);
    }

    /// **本体一死，幻象跟着消失** —— 它带爪牙标记，走 `step::no_master_left`。
    /// 实录最后一帧正是这样收的场：幻象剩 1 血活着，本体被打死，战斗结束。
    #[test]
    fn killing_the_master_ends_the_fight_even_with_the_illusion_alive() {
        let mut s = obscura_pair();
        s.enemies[1].hp = 1;
        s.enemies[0].hp = 1;
        let ix = hand_ix_of(&s, card::STRIKE);
        let s = step(s, Action::PlayCard { hand: ix, target: 0 });
        assert!(!s.enemies[0].alive());
        assert!(s.combat_over && !s.player_dead, "非爪牙全死光 ⇒ 战斗结束");
    }

    /// 哀嚎给**自己这一侧每一只**加 3 力量，**含它自己**。
    /// [源码] `GetTeammatesOf(c) => GetCreaturesOnSide(c.Side)`，含自己。
    #[test]
    fn the_obscura_wail_buffs_itself_and_the_illusion() {
        let mut s = obscura_pair();
        s.enemy_move[0] = 2; // 哀嚎
        s.enemy_move[1] = 1; // 幻象照常猛撞
        let s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[0].get(St::Strength), 3, "本体自己也吃这 3 点");
        assert_eq!(s.enemies[1].get(St::Strength), 3, "幻象也吃");
    }

    /// 出招图：**幻象只出现一次**（没有任何一条边指回它），
    /// 之后三手等权随机且都不能连出两次。
    #[test]
    fn the_obscura_summons_once_and_then_never_repeats_a_move() {
        // **血量给够** —— 幻象每回合 16 点，80 血活不过 6 个回合，
        // 而 `step` 在 `combat_over` 之后原样返回：局面冻住之后再比"有没有重复"
        // 比的是同一帧和它自己。第一版就是这么假红的。
        let mut s = State::new(2000, 3);
        s.add_enemy(def_id_of("胧光怪"), 123);
        let mut s = begin_combat(s);
        let mut seen = Vec::new();
        for _ in 0..12 {
            assert!(!s.combat_over, "这个测试不该打完");
            let allowed = crate::step::allowed_next(&s, 0);
            assert_eq!(allowed & 1, 0, "「幻象」一旦出过就不该再进允许集合");
            let did = s.enemy_move[0];
            seen.push(did);
            s = step(s, Action::EndTurn);
            assert_ne!(s.enemy_move[0], did, "三条边都是 CannotRepeat");
        }
        assert_eq!(seen[0], 0, "起手必须是幻象");
        assert!(seen[1..].iter().all(|&m| (1..=3).contains(&m)), "之后只在 1..3 里转");
        // 三条边等权 ⇒ 12 手里三种都该出现过（不是概率断言：`NotTwice` 之下
        // 连续 12 手只出两种要求每一次都躲开第三种，实际序列是确定的）
        for m in 1..=3u8 {
            assert!(seen[1..].contains(&m), "招 {m} 一次都没出现，等权分支不该这样");
        }
    }

    // -----------------------------------------------------------------------
    // 接续：残杀千足虫的一节砍死了不算完，死后第二个敌人回合 25 血回来
    // -----------------------------------------------------------------------

    /// 三节千足虫，起手指针摆成三种不同的招（和 [源码] 三节错开起手一致）。
    fn decimillipede_trio() -> State {
        let seg = def_id_of("残杀千足虫");
        let mut s = State::new(200, 13);
        for hp in [42, 40, 44] {
            s.add_enemy(seg, hp);
        }
        for _ in 0..10 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        for e in 0..3 {
            s.enemy_move[e] = e as u8;
        }
        s
    }

    /// **三节起手按槽位错开，但单看一节三手都可能**（`ECond::SlotRep`，2026-09-15）。
    ///
    /// [源码] `DecimillipedeElite`：Front / Middle / Back = `num / num+1 / num+2`（`num = Rng.NextInt(3)`）；
    /// `DecimillipedeSegment` 的 `StarterMoveIdx % 3` -> 扭动 / 壮硕 / 缠绕。三段断言各钉一种写法错：
    /// * 实录的开局是内核起手的轮换 —— 等权 `Rand` 起手（`initial_move` 逐只取最低位，三节全是扭动、整场同相）
    ///   和反方向的错开（`num / num+2 / num+1`）都在这里红；槽位对哪一节、轮换朝哪边，两条实录都分得开
    /// * 起手 `[0, 1, 2]` —— 内核选的是 `num = 0` 那一支（另外两支同样是轮换，这一条只钉住选择）
    /// * 允许集合三手全开 —— `SlotIs` 起手会把 `num = 0` 当成事实，`synth_audit` 在 `num != 0` 的实录上报集合外
    #[test]
    fn decimillipede_segments_open_staggered_by_slot_but_each_first_move_stays_open() {
        let Some(t) = act_table() else { return };
        let seg = def_id_of("残杀千足虫");
        let enemies = t.resolve("DecimillipedeElite").unwrap();
        assert!(enemies.len() == 3 && enemies.iter().all(|e| e.def == seg), "三节共用一个 EnemyDef");
        let deck = synth_deck(10);
        let s = crate::synth::build(&crate::synth::FightSpec::new(&deck, 80, &enemies, 1)).state;
        let def = enemy_def(seg);
        let starts: Vec<usize> = (0..3).map(|e| current_move_ix(def, &s, e)).collect();
        // [实测] 第 0 帧 Front / Middle / Back 的意图 ——
        // act2_f28：`Attack:6,Buff` / `Attack:8,Debuff` / `Attack:5×2`（num = 1）
        // act2_f30：`Attack:5×2` / `Attack:6,Buff` / `Attack:8,Debuff`（num = 0）
        for (trace, seen) in [("act2_f28_decimillipede", [1, 2, 0]), ("act2_f30_elite_decimillipede", [0, 1, 2])] {
            assert!(
                (0..3).any(|k| (0..3).all(|e| seen[e] == (starts[e] + k) % 3)),
                "{trace} 的开局 {seen:?} 不是内核起手 {starts:?} 的轮换"
            );
        }
        assert_eq!(starts, vec![0, 1, 2], "合成路径（遭遇表的出场顺序）：扭动 / 壮硕 / 缠绕");
        for e in 0..3 {
            assert_eq!(
                crate::step::allowed_initial(&s, e),
                0b111,
                "槽位 {e}：num 是遭遇级随机数，单看一节三手都可能"
            );
        }
    }

    /// [源码] `ReattachPower` + `DecimillipedeSegment`：死后第一个敌人回合 `DEAD_MOVE`、
    /// 第二个 `REATTACH_MOVE` 回 `Amount`（25）血，**不是回满**。
    /// [实测] `act2_f28_decimillipede`：节 1 第 3 回合砍死、第 4 回合观测里没有、第 5 回合 25/44；
    /// 死前缠绕标签 `Attack:10`（带力量 2），回来之后缠绕 `Attack:8` ⇒ 力量被死亡剥离了。
    #[test]
    fn a_decimillipede_segment_reattaches_with_25_hp_on_the_second_enemy_turn() {
        let mut s = decimillipede_trio();
        s.enemies[0].set(St::Strength, 2);
        s.enemies[0].set(St::Vulnerable, 2);
        // 1 血挨一刀打击 ⇒ 超杀到负数：回血必须从 0 往上回，不然回来是 20 不是 25
        s.enemies[0].hp = 1;
        let ix = hand_ix_of(&s, card::STRIKE);
        let s = step(s, Action::PlayCard { hand: ix, target: 0 });
        assert!(!s.enemies[0].alive(), "砍死那一帧它是死的、打不到");
        assert!(!s.combat_over, "别的节还活着，战斗不该结束");
        assert_eq!(s.enemies[0].get(St::Reattach), 25, "接续自己不被死亡剥离");
        assert_eq!(s.enemies[0].get(St::Strength), 0, "力量被剥掉");
        assert_eq!(s.enemies[0].get(St::Vulnerable), 0, "我上的易伤也被剥掉");

        let s = step(s, Action::EndTurn);
        assert!(!s.enemies[0].alive(), "死后第一个敌人回合是 DEAD_MOVE，还没回来");
        assert!(!s.combat_over);

        let s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[0].hp, 25, "第二个敌人回合回 25，不是回满 42");
        assert_eq!(s.enemies[0].max_hp, 42, "最大生命不变");
        assert_eq!(s.enemies[0].get(St::ReattachDue), 0);
        assert!(s.enemy_move[0] <= 2, "重接那一手出完之后回到三手里随机");
        assert_eq!(
            s.enemies[0].get(St::MoveForcedThisTurn),
            0,
            "敌人回合开始时换招不该留下'意图过期'标记 —— 留着的话下一回合注入威胁会跳过它"
        );
    }

    /// **窗口是两个我方回合**：砍死一节之后、它回来之前把别的节全砍掉，它就回不来 —— 战斗结束。
    /// [源码] `ShouldOwnerDeathTriggerFatal => AreAllOtherSegmentsDead()`；内核里就是
    /// `no_master_left`（非爪牙全死光），所以接续**不进** `any_enemy_present`。
    #[test]
    fn killing_the_other_segments_inside_the_reattach_window_ends_the_fight() {
        let mut s = decimillipede_trio();
        s.enemies[0].hp = 1;
        let ix = hand_ix_of(&s, card::STRIKE);
        let s = step(s, Action::PlayCard { hand: ix, target: 0 });
        let mut s = step(s, Action::EndTurn);
        assert!(!s.enemies[0].alive() && !s.combat_over, "下一个我方回合，尸体还在等");
        s.enemies[1].hp = 1;
        s.enemies[2].hp = 1;
        let ix = hand_ix_of(&s, card::STRIKE);
        let s = step(s, Action::PlayCard { hand: ix, target: 1 });
        assert!(!s.combat_over, "还剩一节活着");
        let ix = hand_ix_of(&s, card::STRIKE);
        let s = step(s, Action::PlayCard { hand: ix, target: 2 });
        assert!(s.combat_over && !s.player_dead, "最后一节死的时候尸体还欠一次接续 —— 照样结束");
    }

    // -----------------------------------------------------------------------
    // 知识恶魔：三次二选一的诅咒，选法是策略参数（`State::curse_policy`）
    // -----------------------------------------------------------------------

    fn knowledge_demon_vs(policy: u8) -> State {
        let mut s = State::new(3000, 21);
        s.add_enemy(enemy::KNOWLEDGE_DEMON, 379);
        for _ in 0..12 {
            s.add_card(card::STRIKE, 0, 0);
        }
        s.curse_policy = policy;
        begin_combat(s)
    }

    /// [源码] `KnowledgeDemon.GenerateMoveStateMachine`：诅咒 -> 抽打 -> 知识过载 -> 思考 ->
    /// （诅咒不到 3 次 ? 诅咒 : 抽打）。所以第 1/5/9 回合诅咒，第 13 回合起只剩三手循环。
    /// 默认选法 `0b010`：心灵腐化 / 瓦解 7 / 虚脱（[玩家判定] 第二次一般选瓦解）。
    #[test]
    fn knowledge_demon_curses_three_times_then_cycles_without_the_curse() {
        let mut s = knowledge_demon_vs(crate::content::DEFAULT_CURSE_POLICY);
        let mut seen = Vec::new();
        for _ in 0..16 {
            assert!(!s.combat_over, "这个测试不该打完");
            seen.push(s.enemy_move[0]);
            s = step(s, Action::EndTurn);
        }
        assert_eq!(seen, vec![0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 1, 2, 3, 1]);
        assert_eq!(s.player.get(St::MindRot), 1, "第 1 次：心灵腐化");
        assert_eq!(s.player.get(St::Disintegration), 7, "第 2 次：瓦解 7");
        assert_eq!(s.player.get(St::WasteAway), 1, "第 3 次：虚脱");
        assert_eq!(s.player.get(St::Sloth), 0);
        assert_eq!(s.base_energy, 2, "虚脱：最大能量 −1，永久");
        assert_eq!(s.energy, 2);
    }

    /// `curse_policy` 的第 k 位 = 第 k 次选瓦解。瓦解是 `Counter`，三次都选就叠成 6+7+8。
    #[test]
    fn curse_policy_bits_pick_the_side_of_each_curse() {
        let mut s = knowledge_demon_vs(0b111);
        for _ in 0..10 {
            s = step(s, Action::EndTurn);
        }
        assert_eq!(s.player.get(St::Disintegration), 6 + 7 + 8);
        assert_eq!(s.player.get(St::MindRot) + s.player.get(St::Sloth) + s.player.get(St::WasteAway), 0);
        assert_eq!(s.base_energy, 3, "没选虚脱，能量不动");

        let mut s = knowledge_demon_vs(0b000);
        for _ in 0..10 {
            s = step(s, Action::EndTurn);
        }
        assert_eq!(s.player.get(St::Disintegration), 0);
        assert_eq!(
            (s.player.get(St::MindRot), s.player.get(St::Sloth), s.player.get(St::WasteAway)),
            (1, 3, 1)
        );
    }

    /// 思考：11 伤害 + **回 30 血** + 自身力量 2（[源码] `PonderMove`；wiki 那条没有回血）。
    #[test]
    fn knowledge_demon_ponder_heals_30_and_gains_strength() {
        let mut s = knowledge_demon_vs(0);
        s.enemy_move[0] = 3;
        s.enemies[0].hp = 100;
        let s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[0].hp, 130);
        assert_eq!(s.enemies[0].get(St::Strength), 2);
    }

    /// 思考的意图签名是 攻击 + 回血 + 强化 三个 —— `identify_enemies` 靠它把实况对齐到这一手。
    #[test]
    fn knowledge_demon_ponder_signature_is_attack_heal_buff() {
        let demon = crate::state::Entity::new(379);
        let me = crate::state::Entity::new(80);
        let sig = crate::replay::move_signature(enemy::KNOWLEDGE_DEMON, 3, &demon, &me, 0);
        let want: Vec<(String, String)> = [("Attack", "11"), ("Heal", ""), ("Buff", "")]
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        assert_eq!(sig, want);
    }

    // ---- 蜂群术士（2026-09-14 批 3）----

    /// 人体蜂房（[源码] `PersonalHivePower.AfterDamageReceived`）：**每一段攻击**塞层数那么多张晕眩
    /// 进抽牌堆，**不看打没打穿**；药水的伤害（`Unpowered`）一张都不塞。
    /// [实测] 2026-08-22 `act2_f27_elite_entomancer`（蜂房 1 层）：五张攻击各 +1、火焰药水 +0。
    #[test]
    fn personal_hive_dazes_every_attack_hit_but_not_potions() {
        let mk = |hive: i32| {
            let mut s = State::new(80, 5);
            s.add_enemy(enemy::ENTOMANCER, 145);
            for _ in 0..10 {
                s.add_card(card::STRIKE, 0, 0);
            }
            let mut s = begin_combat(s);
            s.enemies[0].set(St::PersonalHive, hive);
            s.energy = 10;
            s
        };
        let dazed_in_draw = |s: &State| {
            (0..s.n_draw as usize).filter(|&i| s.cards[s.draw[i] as usize].id == card::DAZED).count()
        };

        let mut s = mk(2);
        give(&mut s, card::TWIN_STRIKE);
        let ix = hand_ix_of(&s, card::TWIN_STRIKE);
        let s = step(s, Action::PlayCard { hand: ix, target: 0 });
        assert_eq!(s.enemies[0].hp, 135);
        assert_eq!(dazed_in_draw(&s), 4, "双重打击两段 × 蜂房 2 层");

        let mut s = mk(1);
        s.enemies[0].block = 50;
        let ix = hand_ix_of(&s, card::STRIKE);
        let s = step(s, Action::PlayCard { hand: ix, target: 0 });
        assert_eq!(s.enemies[0].hp, 145, "这一下全被格挡吃掉");
        assert_eq!(dazed_in_draw(&s), 1, "挡住了也塞");

        let mut s = mk(3);
        s.potions[0] = crate::state::potion::FIRE;
        let s = step(s, Action::UsePotion { slot: 0, target: 0 });
        assert_eq!(s.enemies[0].hp, 125);
        assert_eq!(dazed_in_draw(&s), 0, "火焰药水不是攻击");
    }

    /// 喷射信息素按蜂房层数分两支（[源码] `SpitMove`）：< 3 时蜂房 +1、力量 +1；≥ 3 时只加 2 力量。
    /// 内核拆成下标 2 / 3 两手，由矛击！之后那条条件边挑（`M_ENTOMANCER`），喷射完回蜜——蜂——！。
    #[test]
    fn entomancer_spit_branches_on_hive_stacks() {
        let def = crate::content::enemy_def(enemy::ENTOMANCER);
        let spear_then_spit = |hive: i32| {
            let mut s = with_enemy(enemy::ENTOMANCER, 145);
            s.enemies[0].set(St::PersonalHive, hive);
            s.enemy_move[0] = 1;
            let s = step(s, Action::EndTurn);
            let spit = crate::step::current_move_ix(def, &s, 0);
            let s = step(s, Action::EndTurn);
            (
                spit,
                s.enemies[0].get(St::PersonalHive),
                s.enemies[0].get(St::Strength),
                crate::step::current_move_ix(def, &s, 0),
            )
        };
        assert_eq!(spear_then_spit(1), (2, 2, 1, 0), "蜂房 1：+1 蜂房 +1 力量");
        assert_eq!(spear_then_spit(2), (2, 3, 1, 0), "蜂房 2：还是那一支，涨到 3");
        assert_eq!(spear_then_spit(3), (3, 3, 2, 0), "蜂房 3：只加 2 力量");
    }

    /// 两支喷射信息素的意图签名逐字相同，实况对齐靠 `move_reachable_now` 排掉当前层数下走不到的那一支。
    #[test]
    fn move_reachable_now_rules_out_the_spit_branch_the_hive_forbids() {
        use crate::step::move_reachable_now as reach;
        let mut s = with_enemy(enemy::ENTOMANCER, 145);
        let me = Entity::new(80);
        assert_eq!(
            crate::replay::move_signature(enemy::ENTOMANCER, 2, &s.enemies[0], &me, 0),
            crate::replay::move_signature(enemy::ENTOMANCER, 3, &s.enemies[0], &me, 0),
            "这正是需要筛子的原因"
        );
        s.enemies[0].set(St::PersonalHive, 1);
        assert!(reach(&s, 0, enemy::ENTOMANCER, 2) && !reach(&s, 0, enemy::ENTOMANCER, 3));
        s.enemies[0].set(St::PersonalHive, 3);
        assert!(!reach(&s, 0, enemy::ENTOMANCER, 2) && reach(&s, 0, enemy::ENTOMANCER, 3));
        assert!(reach(&s, 0, enemy::ENTOMANCER, 0), "有 Go 边进来的永远走得到");
    }

    // ---- 异螨 / 熟睡甲虫（2026-09-14 批 4，**全部 [源码]，没有实录**）----

    /// 异螨起手按站位（[源码] 初始态是读 `SlotName` 的 `ConditionalBranchState`）：first 浓毒、
    /// second 吸吮；之后 浓毒 -> 啃咬 -> 吸吮 定环。浓毒往**手牌**塞 2 张毒素。
    #[test]
    fn mytes_open_by_slot_and_toxic_goes_into_the_hand() {
        let myte = def_id_of("异螨");
        let def = crate::content::enemy_def(myte);
        let name = |s: &State, e: usize| def.moves[crate::step::current_move_ix(def, s, e)].name;
        let mut s = State::new(80, 5);
        s.add_enemy(myte, 64);
        s.add_enemy(myte, 64);
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let s = begin_combat(s);
        assert_eq!((name(&s, 0), name(&s, 1)), ("浓毒", "吸吮"), "first 先浓毒、second 先吸吮");

        let s = step(s, Action::EndTurn);
        let toxic_in_hand =
            (0..s.n_hand as usize).filter(|&i| s.cards[s.hand[i] as usize].id == card::TOXIC).count();
        assert_eq!(toxic_in_hand, 2, "浓毒塞手牌，新回合发牌之后还在手里");
        assert_eq!(s.player.hp, 80 - 4, "吸吮 4");
        assert_eq!(s.enemies[1].get(St::Strength), 2, "吸吮给自己 +2 力量");
        assert_eq!((name(&s, 0), name(&s, 1)), ("啃咬", "浓毒"));
    }

    /// 瀑布巨兽（第 1 幕 Boss，2026-09-15 批 1）：虹吸回血 10（封顶最大生命）、高压枪每打一次 +5。
    /// [实测] `act1_f17_waterfall_giant`：第 4 -> 5 回合 174 -> 184；高压枪第 5 回合 20、第 10 回合 25。
    #[test]
    fn waterfall_giant_siphon_heals_and_its_pressure_gun_grows_by_five() {
        let giant = crate::content::enemy::WATERFALL_GIANT;
        let def = crate::content::enemy_def(giant);
        let name = |s: &State| def.moves[crate::step::current_move_ix(def, s, 0)].name;
        let mut s = State::new(500, 3);
        s.add_enemy(giant, 240);
        let mut s = begin_combat(s);
        let mut lost = Vec::new();
        for turn in 1..=10 {
            let was = name(&s);
            if was == "虹吸" {
                s.enemies[0].hp = if turn == 4 { 174 } else { 235 };
            }
            let hp0 = s.player.hp;
            s = step(s, Action::EndTurn);
            lost.push((was, hp0 - s.player.hp));
            if was == "虹吸" {
                assert_eq!(s.enemies[0].hp, if turn == 4 { 184 } else { 240 }, "回血 10，封顶最大生命");
            }
        }
        assert_eq!(
            lost,
            vec![
                ("加压", 0),
                ("重踏", 15),
                ("撞击", 10),
                ("虹吸", 0),
                ("高压枪", 20),
                ("升压", 13),
                ("重踏", 15),
                ("撞击", 10),
                ("虹吸", 0),
                ("高压枪", 25),
            ]
        );
        assert_eq!(s.enemies[0].get(St::PressureGunGrowth), 10, "打过两次，再下一次是 30");
        assert_eq!(s.enemies[0].get(St::SteamEruption), 42, "15 + 9 × 3，和实录死之前那一帧一致");
    }

    /// 瀑布巨兽被砍死**不算赢**：锁成 999999999 血、别的 status 随死亡摘掉、蒸汽喷发留着；
    /// 下一个敌人回合「即将爆发」不打人、层数记成爆炸伤害；再下一个「爆炸」打那么多（它身上的虚弱照样压低），
    /// 然后自杀，战斗这才结束。锁血期间叶评估数 0 血（它会自己炸死）。
    #[test]
    fn waterfall_giant_killed_is_about_to_blow_then_explodes_for_its_steam() {
        use crate::content::{remaining_hp_including_revives, ABOUT_TO_BLOW_HP};
        let giant = crate::content::enemy::WATERFALL_GIANT;
        let def = crate::content::enemy_def(giant);
        let name = |s: &State| def.moves[crate::step::current_move_ix(def, s, 0)].name;
        let mut s = State::new(500, 3);
        s.add_enemy(giant, 240);
        for _ in 0..10 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        for _ in 0..3 {
            s = step(s, Action::EndTurn); // 加压 / 重踏 / 撞击
        }
        assert_eq!(s.enemies[0].get(St::SteamEruption), 21);
        s.enemies[0].hp = 3;
        s.enemies[0].set(St::Weak, 2);
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert!(!s.combat_over, "砍死它不算赢");
        assert_eq!((s.enemies[0].hp, s.enemies[0].max_hp), (ABOUT_TO_BLOW_HP, ABOUT_TO_BLOW_HP));
        assert_eq!(s.enemies[0].get(St::Weak), 0, "别的 status 随死亡摘掉");
        assert_eq!(s.enemies[0].get(St::SteamEruption), 21, "蒸汽喷发自己留着");
        assert_eq!(name(&s), "即将爆发");
        assert_eq!(remaining_hp_including_revives(&s.enemies[0]), 0, "那十亿血不是要打的血");

        let hp0 = s.player.hp;
        let mut s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0, "即将爆发不打人");
        assert_eq!(s.enemies[0].get(St::SteamEruption), 0);
        assert_eq!(s.enemies[0].get(St::EruptionDamage), 21);
        assert_eq!(name(&s), "爆炸");
        assert!(!s.combat_over);

        s.enemies[0].set(St::Weak, 2);
        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 15, "爆炸是攻击：21 × 虚弱 0.75 = 15");
        assert!(!s.enemies[0].alive(), "炸完自杀");
        assert!(s.combat_over && !s.player_dead, "这时候才结束");
    }

    /// 它在**自己出招途中**被反伤打死：死亡规则把它强制到「即将爆发」，这一手打完**指针不推进**
    /// （[源码] `AboutToBlowState.MustPerformOnceBeforeTransitioning`）—— 否则「即将爆发」被跳过、下一手直接爆炸。
    /// 这一手剩下的 op 照样落地（重踏的 +3 蒸汽喷发）。注入式敌人回合（L2 叶子走的那条）同一条规矩。
    #[test]
    fn waterfall_giant_dying_to_thorns_mid_attack_still_winds_up_before_exploding() {
        use crate::content::ABOUT_TO_BLOW_HP;
        let giant = crate::content::enemy::WATERFALL_GIANT;
        let def = crate::content::enemy_def(giant);
        let name = |s: &State| def.moves[crate::step::current_move_ix(def, s, 0)].name;
        let mk = || {
            let mut s = State::new(500, 3);
            s.add_enemy(giant, 240);
            let mut s = step(begin_combat(s), Action::EndTurn); // 加压 ⇒ 蒸汽喷发 15，下一手重踏
            s.enemies[0].hp = 2;
            s.player.set(St::Thorns, 5);
            s
        };

        let s = step(mk(), Action::EndTurn);
        assert_eq!(s.player.hp, 500 - 15, "重踏照样打到");
        assert_eq!(s.enemies[0].max_hp, ABOUT_TO_BLOW_HP, "被荆棘反弹打死 ⇒ 锁血");
        assert_eq!(s.enemies[0].get(St::SteamEruption), 18, "重踏剩下的 +3 照样落地");
        assert_eq!(name(&s), "即将爆发", "指针停住，不跟着重踏推进");
        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, 500 - 15, "即将爆发不打人");
        assert_eq!(name(&s), "爆炸");

        let mut inc = crate::step::NO_INCOMING;
        inc[0] = (15, 1);
        let s = crate::step::end_turn_with_incoming(mk(), &inc);
        assert_eq!(name(&s), "即将爆发", "注入路径同一条规矩");
        let hp0 = s.player.hp;
        let s = crate::step::end_turn_with_incoming(s, &inc);
        assert_eq!(s.player.hp, hp0, "强制改招标记留着 ⇒ 下一个注入回合跳过那个过期的 15");
        assert_eq!(name(&s), "爆炸");
    }

    /// 瀑布巨兽的两个私有计数器从**这一手**的意图标签还原（`replay::infer_private_attack_counter`）。
    /// [实测] `act1_f17_waterfall_giant`：帧 45 高压枪亮 25（涨过 5）· 帧 57 爆炸亮 42。
    /// 不还原的话帧 45 逐字对不上任何一手，按类型退回会对到同样是 `Attack + Buff` 的撞击上。
    #[test]
    fn waterfall_giant_private_counters_are_recovered_from_the_intent_label() {
        let Ok(src) = std::fs::read_to_string("traces/act1_f17_waterfall_giant.json") else { return };
        let t = super::replay::parse_trace(&src).unwrap();
        let giant = crate::content::enemy::WATERFALL_GIANT;
        let def = crate::content::enemy_def(giant);
        for (frame, want_move, st, v) in
            [(45, "高压枪", St::PressureGunGrowth, 5), (57, "爆炸", St::EruptionDamage, 42)]
        {
            let obs = &t.frames[frame].obs;
            let mut r = super::replay::Replayer::for_trace(&t);
            let mut sy = r.sync(obs);
            let ident = r.identify_enemies(&mut sy.state, obs);
            let id = ident.iter().find(|i| i.def == Some(giant)).expect("认得出瀑布巨兽");
            assert_eq!(id.move_ix.map(|m| def.moves[m].name), Some(want_move), "帧 {frame}");
            assert_eq!(sy.state.enemies[id.slot].get(st), v, "帧 {frame}");
        }
    }

    /// 熟睡甲虫没人碰：打鼾 3 个敌人回合，第 3 个回合末熟睡归零、覆甲移除，**第 4 个敌人回合出击**。
    /// 覆甲在同一个回合末**先**给过格挡（[源码] `BeforeSideTurnEndEarly` 早于 `AfterSideTurnEnd`）。
    ///
    /// 机器那条条件边的阈值是 2 不是 1（时点换算，见 `M_SLUMBERING_BEETLE`）：
    /// 写成 1 的话这条会在「第 4 个敌人回合出击」上红 —— 它会多睡一回合。
    #[test]
    fn slumbering_beetle_left_alone_sleeps_three_enemy_turns_then_rolls_out() {
        let beetle = def_id_of("熟睡甲虫");
        let def = crate::content::enemy_def(beetle);
        let name = |s: &State| def.moves[crate::step::current_move_ix(def, s, 0)].name;
        let mut s = with_enemy(beetle, 86);
        assert_eq!(s.enemies[0].block, 15, "敌人的覆甲开局给一次格挡");
        let hp0 = s.player.hp;
        for turn in 1..=3 {
            assert_eq!(name(&s), "打鼾", "第 {turn} 个敌人回合");
            s = step(s, Action::EndTurn);
        }
        assert_eq!(s.player.hp, hp0, "睡着不打人");
        assert_eq!(s.enemies[0].get(St::Slumber), 0);
        assert_eq!(s.enemies[0].get(St::PlatedArmor), 0, "醒来移除覆甲");
        assert_eq!(s.enemies[0].block, 13, "覆甲 15 -> 14 -> 13，醒来那个回合末的格挡已经给过了");
        assert_eq!(name(&s), "出击", "回合末醒的 ⇒ 下一手直接出击");
        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 16);
        assert_eq!(s.enemies[0].get(St::Strength), 2);

        // 对拍那条路拿我方回合开头的层数判下一手：2 层 ⇒ 回合末剩 1、还睡；1 层 ⇒ 回合末醒
        let mut s = with_enemy(beetle, 86);
        s.enemies[0].set(St::Slumber, 2);
        assert_eq!(crate::step::allowed_next(&s, 0), 1 << 0);
        s.enemies[0].set(St::Slumber, 1);
        assert_eq!(crate::step::allowed_next(&s, 0), 1 << 1);
    }

    /// 熟睡被**打穿**才减（[源码] `UnblockedDamage != 0`，**不分是不是攻击**）：格挡吃掉的不算、药水打穿算。
    /// 打穿到 0 是击晕换招：下一个敌人回合「醒来」（移除覆甲、不打人、回合末不再给格挡），再下一个起出击。
    #[test]
    fn slumbering_beetle_woken_by_damage_is_stunned_one_turn_then_rolls_out() {
        let beetle = def_id_of("熟睡甲虫");
        let def = crate::content::enemy_def(beetle);
        let name = |s: &State| def.moves[crate::step::current_move_ix(def, s, 0)].name;
        let mk = || {
            let mut s = State::new(80, 5);
            s.add_enemy(beetle, 86);
            for _ in 0..10 {
                s.add_card(card::STRIKE, 0, 0);
            }
            let mut s = begin_combat(s);
            s.energy = 10;
            s
        };

        let s = mk();
        let s = step(s, Action::PlayCard { hand: hand_ix_of(&s, card::STRIKE), target: 0 });
        assert_eq!(s.enemies[0].block, 9, "打击 6 全被 15 格挡吃掉");
        assert_eq!(s.enemies[0].get(St::Slumber), 3, "被格挡吃掉的不算");

        let mut s = mk();
        s.enemies[0].block = 0;
        s.potions[0] = crate::state::potion::FIRE;
        let s = step(s, Action::UsePotion { slot: 0, target: 0 });
        assert_eq!(s.enemies[0].get(St::Slumber), 2, "药水打穿也算");
        assert_eq!(name(&s), "打鼾", "没减到 0 不换招");

        let mut s = mk();
        s.enemies[0].block = 0;
        for _ in 0..3 {
            s = step(s, Action::PlayCard { hand: hand_ix_of(&s, card::STRIKE), target: 0 });
        }
        assert_eq!(s.enemies[0].get(St::Slumber), 0);
        assert_eq!(name(&s), "醒来", "打穿到 0 ⇒ 击晕换招");
        let hp0 = s.player.hp;
        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0, "醒来那一手不打人");
        assert_eq!(s.enemies[0].get(St::PlatedArmor), 0);
        assert_eq!(s.enemies[0].block, 0, "覆甲没了，这个回合末不给格挡");
        assert_eq!(name(&s), "出击");
        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 16);
    }

    /// 乐加维林族母没人碰：沉睡 3 个敌人回合，**第 3 个回合末拿不到覆甲那堵墙** ——
    /// 沉睡在 `EnemyTurnEndVeryEarly` 就把覆甲摘了，而覆甲给格挡在 `EnemyTurnEndEarly`。
    ///
    /// 这一条是这只 Boss 和熟睡甲虫的分界：甲虫的覆甲是**醒来那一手**清的，
    /// 那一回合的格挡已经给过了（13 点）；族母这里是 **0**。
    /// 两个钩子压成一个的话这里会红。
    #[test]
    fn lagavulin_left_alone_sleeps_three_turns_and_loses_the_last_wall() {
        let matriarch = def_id_of("乐加维林族母");
        let def = crate::content::enemy_def(matriarch);
        let name = |s: &State| def.moves[crate::step::current_move_ix(def, s, 0)].name;
        let mut s = with_enemy(matriarch, 222);
        assert_eq!(s.enemies[0].block, 12, "敌人的覆甲开局给一次格挡");
        let hp0 = s.player.hp;

        for (turn, want_block, want_asleep) in [(1, 12, 2), (2, 11, 1), (3, 0, 0)] {
            assert_eq!(name(&s), "沉睡", "第 {turn} 个敌人回合");
            s = step(s, Action::EndTurn);
            assert_eq!(s.enemies[0].block, want_block, "第 {turn} 个敌人回合末的格挡");
            assert_eq!(s.enemies[0].get(St::Asleep), want_asleep, "第 {turn} 个敌人回合末的沉睡");
        }
        assert_eq!(s.player.hp, hp0, "睡着不打人");
        assert_eq!(s.enemies[0].get(St::PlatedArmor), 0, "最后一个睡眠回合把覆甲摘了");
        assert_eq!(name(&s), "斩击", "自然醒 ⇒ 下一手直接斩击，没有击晕");
        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 19);

        // 对拍那条路拿我方回合开头的层数判下一手：2 层 ⇒ 回合末剩 1、还睡；1 层 ⇒ 回合末醒
        let mut s = with_enemy(matriarch, 222);
        s.enemies[0].set(St::Asleep, 2);
        assert_eq!(crate::step::allowed_next(&s, 0), 1 << 0);
        s.enemies[0].set(St::Asleep, 1);
        assert_eq!(crate::step::allowed_next(&s, 0), 1 << 1);
    }

    /// 沉睡被**打穿**就整条没了（[源码] `Remove(this)`，不是减 1 层）：覆甲当场移除、击晕一回合。
    /// 被格挡吃掉的不算 —— 开局那 12 点覆甲格挡正是这只 Boss 的第一道门槛。
    #[test]
    fn lagavulin_woken_by_damage_loses_plating_at_once_and_is_stunned() {
        let matriarch = def_id_of("乐加维林族母");
        let def = crate::content::enemy_def(matriarch);
        let name = |s: &State| def.moves[crate::step::current_move_ix(def, s, 0)].name;
        let mk = || {
            let mut s = State::new(80, 5);
            s.add_enemy(matriarch, 222);
            for _ in 0..10 {
                s.add_card(card::STRIKE, 0, 0);
            }
            let mut s = begin_combat(s);
            s.energy = 10;
            s
        };

        let s = mk();
        let s = step(s, Action::PlayCard { hand: hand_ix_of(&s, card::STRIKE), target: 0 });
        assert_eq!(s.enemies[0].block, 6, "打击 6 全被 12 格挡吃掉");
        assert_eq!(s.enemies[0].get(St::Asleep), 3, "被格挡吃掉的不算");
        assert_eq!(s.enemies[0].get(St::PlatedArmor), 12);

        let mut s = mk();
        s.enemies[0].block = 0;
        let s = step(s, Action::PlayCard { hand: hand_ix_of(&s, card::STRIKE), target: 0 });
        assert_eq!(s.enemies[0].get(St::Asleep), 0, "打穿一下整条沉睡就没了");
        assert_eq!(s.enemies[0].get(St::PlatedArmor), 0, "覆甲当场移除，我方回合里就没了");
        assert_eq!(name(&s), "醒来", "击晕换招");
        let hp0 = s.player.hp;
        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0, "醒来那一手不打人");
        assert_eq!(s.enemies[0].block, 0, "覆甲没了，这个回合末不给格挡");
        assert_eq!(name(&s), "斩击");
        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 19);
    }

    /// 醒来之后的四手定环：斩击 -> 开膛破肚 -> 斩击2（带格挡）-> 灵魂汲取 -> 斩击…
    /// 灵魂汲取给我 −2 力量 −2 敏捷、给它自己 +2 力量，所以下一圈的斩击是 21。
    #[test]
    fn lagavulin_awake_cycle_is_four_moves_and_soul_siphon_swings_two_strength() {
        let matriarch = def_id_of("乐加维林族母");
        let def = crate::content::enemy_def(matriarch);
        let name = |s: &State| def.moves[crate::step::current_move_ix(def, s, 0)].name;
        let mut s = with_enemy(matriarch, 222);
        // 直接从醒着的状态起步：沉睡摘掉、指针放到斩击
        s.enemies[0].set(St::Asleep, 0);
        s.enemies[0].set(St::PlatedArmor, 0);
        s.enemies[0].block = 0;
        s.enemy_move[0] = 1;

        let hp0 = s.player.hp;
        assert_eq!(name(&s), "斩击");
        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 19);
        assert_eq!(name(&s), "开膛破肚");
        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 19 - 18, "9×2");
        assert_eq!(name(&s), "斩击2");
        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 19 - 18 - 12);
        assert_eq!(s.enemies[0].block, 12, "斩击2 自带 12 格挡");
        assert_eq!(name(&s), "灵魂汲取");
        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 19 - 18 - 12, "灵魂汲取不打人");
        assert_eq!(s.player.get(St::Strength), -2);
        assert_eq!(s.player.get(St::Dexterity), -2);
        assert_eq!(s.enemies[0].get(St::Strength), 2);
        assert_eq!(name(&s), "斩击", "四手定环，转回斩击");
        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 19 - 18 - 12 - 21, "19 + 它自己的 2 点力量");
    }

    /// 滑溜封的是**掉血**不是伤害：格挡照常被打满，只有漏过格挡的那一截压成 1。
    ///
    /// 这一条是它和无实体的**唯一**区别，而两者差着一整场仗的节奏：
    /// 无实体（`ModifyDamageCap => 1`）下 26 点只扣 1 点格挡，滑溜下格挡当场清零。
    /// 建在 `apply_modifiers` 里的话这里会红。
    #[test]
    fn slippery_caps_hp_loss_not_damage_so_block_still_takes_the_full_hit() {
        let mut e = Entity::new(100);
        e.set(St::Slippery, 8);
        e.block = 8;
        let through = crate::damage::absorb(&mut e, 26);
        assert_eq!(through, 1, "漏过格挡的 18 点压成 1");
        assert_eq!(e.block, 0, "格挡照常被 26 点打满 —— 这一句无实体给不出");
        assert_eq!(e.hp, 99);

        // 完全被格挡吃掉 ⇒ 一点血都不掉，也不算一次"打穿"
        let mut e = Entity::new(100);
        e.set(St::Slippery, 8);
        e.block = 30;
        assert_eq!(crate::damage::absorb(&mut e, 26), 0);
        assert_eq!(e.hp, 100);

        // 没有滑溜就照常掉
        let mut e = Entity::new(100);
        e.block = 8;
        assert_eq!(crate::damage::absorb(&mut e, 26), 18);
        assert_eq!(e.hp, 82);
    }

    /// 滑溜**每打穿一次减 1 层**（不分是不是攻击），所以**段数是货币**：
    /// 三段的旋风斩在墨影幻灵身上比一段的重击拆得快，尽管后者面板伤害高得多。
    /// 层数清完之后才开始正常掉血。
    #[test]
    fn slippery_is_paid_in_hits_not_in_damage() {
        let vantom = def_id_of("墨影幻灵");
        let mk = |card_id: u16, n: usize| {
            let mut s = State::new(80, 5);
            s.add_enemy(vantom, 173);
            for _ in 0..n {
                s.add_card(card_id, 0, 0);
            }
            let mut s = begin_combat(s);
            s.energy = 30;
            s
        };

        // 三张打击 = 三次打穿 ⇒ 掉 3 血、滑溜 8 -> 5
        let mut s = mk(card::STRIKE, 6);
        for _ in 0..3 {
            s = step(s, Action::PlayCard { hand: hand_ix_of(&s, card::STRIKE), target: 0 });
        }
        assert_eq!(s.enemies[0].get(St::Slippery), 5, "每次打穿减 1");
        assert_eq!(s.enemies[0].hp, 170, "每次只掉 1 血，和打了多少无关");

        // 打穿 8 次之后滑溜没了，第 9 下按真实伤害算。
        // 一手只有 5 张，所以跨一个回合打（墨迹 7 点，和这一条无关）。
        let mut s = mk(card::STRIKE, 15);
        for _ in 0..5 {
            s = step(s, Action::PlayCard { hand: hand_ix_of(&s, card::STRIKE), target: 0 });
        }
        s = step(s, Action::EndTurn);
        s.energy = 30;
        for _ in 0..3 {
            s = step(s, Action::PlayCard { hand: hand_ix_of(&s, card::STRIKE), target: 0 });
        }
        assert_eq!(s.enemies[0].get(St::Slippery), 0, "8 次打穿把 8 层清光");
        assert_eq!(s.enemies[0].hp, 173 - 8);
        let s = step(s, Action::PlayCard { hand: hand_ix_of(&s, card::STRIKE), target: 0 });
        assert_eq!(s.enemies[0].hp, 173 - 8 - 6, "滑溜清完，打击照常打 6");
    }

    /// 墨影幻灵四手定环：墨迹 7 -> 墨水长枪 6×2 -> 肢解 26 + 3 张伤口进**弃牌堆** -> 准备（力量 +2）。
    #[test]
    fn vantom_cycle_is_four_moves_and_dismember_adds_three_wounds_to_the_discard() {
        let vantom = def_id_of("墨影幻灵");
        let def = crate::content::enemy_def(vantom);
        let name = |s: &State| def.moves[crate::step::current_move_ix(def, s, 0)].name;
        let mut s = State::new(120, 3);
        s.add_enemy(vantom, 173);
        for _ in 0..10 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        let hp0 = s.player.hp;

        assert_eq!(name(&s), "墨迹");
        s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 7);
        assert_eq!(name(&s), "墨水长枪");
        s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 7 - 12, "6×2");
        assert_eq!(name(&s), "肢解");
        let wounds_before = (0..s.n_cards as usize).filter(|&i| s.cards[i].id == card::WOUND).count();
        s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 7 - 12 - 26);
        let wounds = (0..s.n_cards as usize).filter(|&i| s.cards[i].id == card::WOUND).count();
        assert_eq!(wounds, wounds_before + 3, "3 张伤口");
        assert_eq!(name(&s), "准备");
        s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[0].get(St::Strength), 2, "准备不打人，给自己 +2 力量");
        assert_eq!(name(&s), "墨迹", "转回第一手");
        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 7 - 12 - 26 - 9, "7 + 力量 2");
    }

    /// 三只墨宝**中间那只起手旋风**，另外两只起手刺击（[源码] `InkletsNormal` 只给中间那只
    /// `MiddleInklet = true`，遭遇本身一个随机数都没掷 ⇒ 起手是**确定**的，用 `SlotIs`）。
    /// 之后刺击和「锐利凝视 | 旋风」交替。
    #[test]
    fn the_middle_inklet_opens_with_whirlwind_and_the_others_jab() {
        let inklet = def_id_of("墨宝");
        let def = crate::content::enemy_def(inklet);
        let mut s = State::new(80, 3);
        for _ in 0..3 {
            s.add_enemy(inklet, 14);
        }
        let s = begin_combat(s);
        let name = |s: &State, e: usize| def.moves[crate::step::current_move_ix(def, s, e)].name;
        assert_eq!(name(&s, 0), "刺击");
        assert_eq!(name(&s, 1), "旋风", "中间那只");
        assert_eq!(name(&s, 2), "刺击");
        // 开局允许集合也是单元素的 —— 起手确定，不是"哪一手都可能"
        for e in 0..3 {
            let set = crate::step::allowed_initial(&s, e);
            assert_eq!(set.count_ones(), 1, "槽 {e} 的开局是确定的");
        }
        // 刺击之后是二选一，旋风 / 锐利凝视之后回刺击
        let mut s2 = s;
        s2.enemy_move[0] = 1;
        assert_eq!(crate::step::allowed_next(&s2, 0), 1 << 0, "旋风 -> 刺击");
        s2.enemy_move[0] = 2;
        assert_eq!(crate::step::allowed_next(&s2, 0), 1 << 0, "锐利凝视 -> 刺击");
        s2.enemy_move[0] = 0;
        assert_eq!(crate::step::allowed_next(&s2, 0), (1 << 1) | (1 << 2), "刺击 -> 二选一");
    }

    // ---- 2026-09-19 第 1 幕批 4（鬼祟珊瑚群）/ 批 5（暗港杂兵）。全部 [源码]，没有实录。

    /// 硬化外壳封的是**一个回合累计**的掉血（和难以杀灭的「每一下封顶」不是一回事），
    /// 而且和滑溜一样在**扣完格挡之后**才封 —— 格挡照常被打满。
    /// 门是**上限**不是余额：余额扣到 0 正是外壳最硬的时候，拿余额当门的话额度一用完外壳就「消失」了。
    #[test]
    fn hardened_shell_caps_hp_loss_per_turn_after_block_not_per_hit() {
        let shell = |bal: i32| {
            let mut e = Entity::new(75);
            e.set(St::HardenedShellCap, 20);
            e.set(St::HardenedShell, bal);
            e
        };
        let mut e = shell(20);
        e.block = 5;
        assert_eq!(crate::damage::absorb(&mut e, 15), 10, "5 点格挡先吃，漏过的 10 全掉");
        assert_eq!((e.block, e.hp, e.get(St::HardenedShell)), (0, 65, 10));
        assert_eq!(crate::damage::absorb(&mut e, 15), 10, "余额只剩 10 —— 累计封顶，不是每一下 20");
        assert_eq!((e.hp, e.get(St::HardenedShell)), (55, 0));
        assert_eq!(crate::damage::absorb(&mut e, 30), 0, "余额 0：再打多少都不掉");
        assert_eq!(e.hp, 55);

        let mut e = shell(0);
        e.block = 12;
        assert_eq!(crate::damage::absorb(&mut e, 30), 0);
        assert_eq!(e.block, 0, "余额 0 也照样把格挡打光 —— 无实体 / 难以杀灭给不出这一句");

        // 没有上限标记 = 没有外壳：余额那一格是 0 也照常掉血
        let mut e = Entity::new(75);
        assert_eq!(crate::damage::absorb(&mut e, 30), 30);
    }

    /// 硬化外壳**双方各自的回合开始**都回满（[源码] `BeforeSideTurnStart`，不分哪一边），
    /// 而且回满在**最早一档**（`Hook::SideTurnStart`）—— 早于水银沙漏那种回合开始就打敌人的规则。
    ///
    /// 三件事：我这个回合打满 20 之后再打是白打 · 敌人回合里荆棘反弹给它的伤害吃的是
    /// 敌人回合那一份额度 · 下一个我方回合开头水银沙漏的 3 点算进**这个回合**的额度（余额 17）。
    /// 把回满挂到 `TurnStart` 上的话（表里排在水银沙漏后面），后两句都会错。
    #[test]
    fn hardened_shell_refills_at_each_side_turn_start_before_anything_hits_it() {
        let colony = def_id_of("鬼祟珊瑚群");
        let mut s = State::new(80, 7);
        s.add_enemy(colony, 75);
        for _ in 0..10 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        assert_eq!(s.enemies[0].get(St::HardenedShell), 20, "开局余额 = 上限");
        assert_eq!(s.enemies[0].get(St::HardenedShellCap), 20, "上限是按名字挂的私有标记");
        s.energy = 10;
        for _ in 0..4 {
            s = step(s, Action::PlayCard { hand: hand_ix_of(&s, card::STRIKE), target: 0 });
        }
        assert_eq!(s.enemies[0].hp, 55, "4 × 6 = 24，只掉得下 20");
        assert_eq!(s.enemies[0].get(St::HardenedShell), 0);
        let s5 = step(s, Action::PlayCard { hand: hand_ix_of(&s, card::STRIKE), target: 0 });
        assert_eq!(s5.enemies[0].hp, 55, "额度用完，第 5 张白打");

        let mut s = s;
        s.player.set(St::Thorns, 3);
        s.player.set(St::MercuryHourglass, 3);
        let s = step(s, Action::EndTurn);
        assert_eq!(
            s.enemies[0].hp,
            55 - 3 - 3,
            "猛冲打我、荆棘反弹 3（敌人回合，额度刚回满）+ 水银沙漏 3（我的回合，额度又回满）"
        );
        assert_eq!(s.enemies[0].get(St::HardenedShell), 17, "我的回合开头先回满，水银沙漏那 3 点算进这个回合");
    }

    /// 鬼祟珊瑚群四手定环：猛冲 14 -> 猛冲 14 -> 惯性 9 + 力量 2 -> 穿刺戳击 7×2 -> 猛冲 …
    #[test]
    fn skulking_colony_cycle_is_zoom_zoom_inertia_piercing_stabs() {
        let colony = def_id_of("鬼祟珊瑚群");
        let def = crate::content::enemy_def(colony);
        let name = |s: &State| def.moves[crate::step::current_move_ix(def, s, 0)].name;
        let mut s = State::new(200, 3);
        s.add_enemy(colony, 75);
        for _ in 0..10 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        let hp0 = s.player.hp;
        assert_eq!(name(&s), "猛冲");
        s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 14);
        assert_eq!(name(&s), "猛冲2");
        s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 28);
        assert_eq!(name(&s), "惯性");
        s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 37);
        assert_eq!(s.enemies[0].get(St::Strength), 2, "惯性打完 +2 力量");
        assert_eq!(name(&s), "穿刺戳击");
        s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 37 - 18, "(7 + 2) × 2");
        assert_eq!(name(&s), "猛冲", "四手定环");
    }

    /// 幽灵船：起手纠缠（我虚弱 3 + **5 张晕眩进弃牌堆**，不打人），之后扫击 13 / 践踏 4×3 交替。
    #[test]
    fn haunted_ship_haunts_once_then_alternates_swipe_and_stomp() {
        let ship = def_id_of("幽灵船");
        let def = crate::content::enemy_def(ship);
        let name = |s: &State| def.moves[crate::step::current_move_ix(def, s, 0)].name;
        let mut s = State::new(120, 3);
        s.add_enemy(ship, 63);
        for _ in 0..10 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        let hp0 = s.player.hp;
        assert_eq!(name(&s), "纠缠");
        s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0, "纠缠不打人");
        assert!(s.player.get(St::Weak) > 0, "给我挂了虚弱");
        assert_eq!(cards_named(&s, card::DAZED), 5, "5 张晕眩");
        assert_eq!(name(&s), "扫击");
        s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 13);
        assert_eq!(name(&s), "践踏");
        s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 13 - 12, "4×3");
        assert_eq!(name(&s), "扫击", "纠缠只出一次，之后扫击 / 践踏交替");
        assert_eq!(cards_named(&s, card::DAZED), 5, "晕眩没再加");
    }

    /// 蟾蜍蝌蚪按站位起手（前面那只带刺、后面那只旋转 —— 遭遇写死的，开局允许集合单元素），
    /// 环是 旋转 -> 带刺 -> 吐刺。**带刺之后打它吃 2 点反弹；吐刺先把刺收回去再打人。**
    #[test]
    fn toadpoles_open_by_slot_and_retract_their_spikes_before_spitting() {
        let tp = def_id_of("蟾蜍蝌蚪");
        let def = crate::content::enemy_def(tp);
        let name = |s: &State, e: usize| def.moves[crate::step::current_move_ix(def, s, e)].name;
        let mut s = State::new(80, 3);
        s.add_enemy(tp, 23);
        s.add_enemy(tp, 23);
        for _ in 0..10 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let s = begin_combat(s);
        assert_eq!(name(&s, 0), "带刺", "前面那只");
        assert_eq!(name(&s, 1), "旋转", "后面那只");
        for e in 0..2 {
            assert_eq!(crate::step::allowed_initial(&s, e).count_ones(), 1, "槽 {e} 的开局是确定的");
        }
        let hp0 = s.player.hp;
        let mut s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 7, "只有后面那只打人（旋转 7）");
        assert_eq!(s.enemies[0].get(St::Thorns), 2, "前面那只长了 2 层刺");

        s.energy = 5;
        let hp1 = s.player.hp;
        s = step(s, Action::PlayCard { hand: hand_ix_of(&s, card::STRIKE), target: 0 });
        assert_eq!(s.player.hp, hp1 - 2, "打长了刺的那只吃 2 点反弹");
        assert_eq!(name(&s, 0), "吐刺");
        assert_eq!(name(&s, 1), "带刺");
        let hp2 = s.player.hp;
        let s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[0].get(St::Thorns), 0, "吐刺先把刺收回去");
        assert_eq!(s.enemies[1].get(St::Thorns), 2, "后面那只这一手才长刺");
        assert_eq!(s.player.hp, hp2 - 9, "吐刺 3×3，带刺不打人");
    }

    /// 化石追踪者的吮吸：它的攻击**每打穿一段** +3 力量，被格挡吃光的那段不算；
    /// **给的力量进不了同一手的下一段**（[源码] 在 `AfterAttack` 里一次给）。
    /// 注入路径（L2 的结算）和真敌人回合走同一个 `take_attack_hit`，两条都验。
    /// 顺带：起手缠上，之后三选一、同一手最多连出两次。
    #[test]
    fn fossil_stalker_sucks_strength_per_unblocked_hit_after_the_attack() {
        let fs = def_id_of("化石追踪者");
        let def = crate::content::enemy_def(fs);
        let s = lone(fs, 52);
        assert_eq!(def.moves[current_move_ix(def, &s, 0)].name, "缠上", "起手缠上");
        assert_eq!(s.enemies[0].get(St::Suck), 3);

        // 甩动 3×2：第一段被 4 点格挡吃光（剩 1），第二段穿 2
        let mut s = lone(fs, 52);
        s.enemy_move[0] = 2;
        s.player.block = 4;
        let after = step(s, Action::EndTurn);
        assert_eq!(after.player.hp, 78);
        assert_eq!(after.enemies[0].get(St::Strength), 3, "只有第二段打穿");

        let mut s = lone(fs, 52);
        s.enemy_move[0] = 2;
        let after = step(s, Action::EndTurn);
        assert_eq!(after.player.hp, 80 - 6, "3 + 3 —— 第二段没吃到第一段给的力量");
        assert_eq!(after.enemies[0].get(St::Strength), 6, "两段都穿 +6");

        let s = lone(fs, 52);
        let mut inc = no_incoming();
        inc[0] = (12, 1);
        let after = end_turn_with_incoming(s, &inc);
        assert_eq!(after.enemies[0].get(St::Strength), 3, "注入路径同样发作");

        let mut s = lone(fs, 52);
        s.enemy_move[0] = 2;
        assert_eq!(allowed_next(&s, 0), 0b111, "三选一");
        s.enemy_hist[0][0] = 2;
        assert_eq!(allowed_next(&s, 0), 0b011, "已经连着甩了两次 ⇒ 不能再甩");
    }

    /// 地精佣兵：三手定环（拿来 7×2 -> 双重猛击 6×2 + 我虚弱 2 -> 嘿嘿 8 + 自身力量 2）；
    /// **打死它不算赢**，冒出卑鄙地精 + 胖地精（[源码] `SurprisePower`）。
    /// 两只都先醒来一手；卑鄙地精之后一直冲撞 9，胖地精之后一直「逃跑」（内核原地不动）。
    #[test]
    fn killing_the_gremlin_merc_summons_two_gremlins_and_combat_goes_on() {
        let merc = def_id_of("地精佣兵");
        let def = crate::content::enemy_def(merc);
        let name = |s: &State| def.moves[crate::step::current_move_ix(def, s, 0)].name;
        let mut s = State::new(120, 3);
        s.add_enemy(merc, 48);
        for _ in 0..10 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        assert_eq!((s.enemies[0].get(St::Surprise), s.enemies[0].get(St::Thievery)), (1, 20));
        let hp0 = s.player.hp;
        assert_eq!(name(&s), "拿来");
        s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 14);
        assert_eq!(name(&s), "双重猛击");
        s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 26);
        assert!(s.player.get(St::Weak) > 0);
        assert_eq!(name(&s), "嘿嘿");
        s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 34);
        assert_eq!(s.enemies[0].get(St::Strength), 2);
        assert_eq!(name(&s), "拿来", "三手定环");

        s.enemies[0].hp = 1;
        s.energy = 5;
        let s = step(s, Action::PlayCard { hand: hand_ix_of(&s, card::STRIKE), target: 0 });
        assert!(!s.enemies[0].alive());
        assert!(!s.combat_over, "打死地精佣兵**不该**判成胜利");
        let alive: Vec<(&str, i32)> = (0..s.n_enemies as usize)
            .filter(|&i| s.enemies[i].alive())
            .map(|i| (crate::content::enemy_def(s.enemy_def[i]).name, s.enemies[i].hp))
            .collect();
        assert_eq!(alive, vec![("卑鄙地精", 12), ("胖地精", 15)], "先卑鄙后胖；名字是连接键");
        let hp1 = s.player.hp;
        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp1, "召唤出来的第一个敌人回合两只都只是醒来");
        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp1 - 9, "卑鄙地精冲撞 9，胖地精逃跑不打人");
        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp1 - 18, "之后一直冲撞");
    }

    /// 这一批几手的**意图签名**要和 [源码] 的意图逐个对上 —— 对拍 / 实况对齐靠它认招。
    /// 三个容易写错的：纠缠是 `Debuff` + `StatusCard:5`（塞牌是副作用、另起一个意图）·
    /// 吐刺那 −2 荆棘不该冒出一个 `Buff` · 惯性是 攻击 + 强化。
    #[test]
    fn act1_underdocks_batch_intent_signatures_match_the_source_intents() {
        let sig = |name: &str, mv: usize| {
            let id = def_id_of(name);
            crate::replay::move_signature(id, mv, &Entity::new(50), &Entity::new(80), 0)
        };
        let v = |xs: &[(&str, &str)]| xs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>();
        assert_eq!(sig("幽灵船", 0), v(&[("Debuff", ""), ("StatusCard", "5")]));
        assert_eq!(sig("幽灵船", 2), v(&[("Attack", "4×3")]));
        assert_eq!(sig("蟾蜍蝌蚪", 0), v(&[("Attack", "3×3")]));
        assert_eq!(sig("蟾蜍蝌蚪", 2), v(&[("Buff", "")]));
        assert_eq!(sig("化石追踪者", 0), v(&[("Attack", "9"), ("Debuff", "")]));
        assert_eq!(sig("鬼祟珊瑚群", 2), v(&[("Attack", "9"), ("Buff", "")]));
        assert_eq!(sig("鬼祟珊瑚群", 3), v(&[("Attack", "7×2")]));
        assert_eq!(sig("地精佣兵", 1), v(&[("Attack", "6×2"), ("Debuff", "")]));
        assert_eq!(sig("地精佣兵", 2), v(&[("Attack", "8"), ("Buff", "")]));
        assert_eq!(sig("卑鄙地精", 0), v(&[("Stun", "")]));
        assert_eq!(sig("胖地精", 1), v(&[("Escape", "")]));
    }

    /// 缠结的费用落在**哪一层**（[源码] `CardEnergyCost.GetWithModifiers`：本地改费 -> 全局钩子 -> `Late` -> 夹 0）。
    /// 每一条都挑的是**两种建法给不同数**的样本：
    ///
    /// * 踩踏打过 4 张攻击：`3 − 4 + 1 = 0`（缠结在夹 0 **之前**）；夹完再加是 1
    /// * 药水 / 地狱之刃给的「本回合免费」是本地的置 0，缠结照样加 ⇒ **1**；「免费就直接 return 0」是 0
    /// * 无情猛攻是 `Late`，压过缠结 ⇒ 0
    /// * X 费不吃它（`CostsX` 提前 return）· 技能牌不吃它
    #[test]
    fn tangled_adds_to_attack_costs_after_local_modifiers_and_before_the_late_free_attack() {
        let mut s = lone(enemy::DUMMY, 100);
        give(&mut s, card::STRIKE);
        give(&mut s, card::DEFEND);
        give(&mut s, card::STAMPEDE);
        give(&mut s, card::WHIRLWIND);
        let free = give(&mut s, card::STRIKE);
        s.cards[free as usize].flags |= F_FREE_THIS_TURN;
        s.energy = 3;
        let cost = |s: &State| (0..5).map(|i| effective_cost(s, i)).collect::<Vec<_>>();
        assert_eq!(cost(&s), vec![1, 1, 3, 3, 0], "没有缠结：打击 / 防御 / 踩踏 / 旋风斩（X = 能量）/ 免费打击");

        s.player.set(St::Tangled, 1);
        assert_eq!(cost(&s), vec![2, 1, 4, 3, 1], "缠结 1：攻击 +1，技能和 X 费不动，**免费的攻击也要 1**");
        s.attacks_played = 3;
        assert_eq!(effective_cost(&s, 2), 1, "踩踏 3 − 3 + 1");
        s.attacks_played = 4;
        assert_eq!(effective_cost(&s, 2), 0, "踩踏 3 − 4 + 1 = 0 —— 缠结加在夹 0 之前，不是 max(0, −1) + 1");
        s.free_attack = 1;
        assert_eq!(cost(&s)[0], 0, "无情猛攻是 Late 钩子，压过缠结");
        assert_eq!(cost(&s)[3], 3, "X 费照旧是能量");
    }

    /// 藤蔓蹒跚者三手定环，**起点是挥击**：挥击 6×2 -> 紧绕藤蔓 8 + 缠结 1 -> 大啃 16 -> 挥击 …
    /// 缠结是**敌人回合里**挂上的，撑过我的下一个回合（那一回合攻击牌 +1 费、能量照 2 扣），
    /// **我的回合结束**摘掉（敌人回合结束不摘）；人工制品挡得住（`PowerType.Debuff`）。
    #[test]
    fn vine_shambler_tangles_my_attacks_for_exactly_one_player_turn() {
        let vs = def_id_of("藤蔓蹒跚者");
        let def = crate::content::enemy_def(vs);
        let name = |s: &State| def.moves[crate::step::current_move_ix(def, s, 0)].name;
        let mut s = State::new(120, 3);
        s.add_enemy(vs, 61);
        for _ in 0..10 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        let hp0 = s.player.hp;
        assert_eq!(name(&s), "挥击", "起点是挥击，不是列表里排第一的紧绕藤蔓");
        s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 12, "6×2");
        assert_eq!(s.player.get(St::Tangled), 0);
        assert_eq!(name(&s), "紧绕藤蔓");
        s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 20);
        assert_eq!(s.player.get(St::Tangled), 1, "打完给我挂缠结，撑到我的回合");
        let h = hand_ix_of(&s, card::STRIKE) as usize;
        assert_eq!(effective_cost(&s, h), 2);
        s.energy = 3;
        s = step(s, Action::PlayCard { hand: h as u8, target: 0 });
        assert_eq!(s.energy, 1, "照 2 费扣");
        assert_eq!(name(&s), "大啃");
        s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 36, "大啃 16");
        assert_eq!(s.player.get(St::Tangled), 0, "我的回合结束摘掉");
        assert_eq!(effective_cost(&s, hand_ix_of(&s, card::STRIKE) as usize), 1, "下一回合恢复原价");
        assert_eq!(name(&s), "挥击", "三手定环");

        // 人工制品挡掉这一笔
        let mut s = lone(vs, 61);
        s.enemy_move[0] = 1;
        s.player.set(St::Artifact, 1);
        let s = step(s, Action::EndTurn);
        assert_eq!((s.player.get(St::Tangled), s.player.get(St::Artifact)), (0, 0), "人工制品吃掉缠结");

        // 注入路径（L2 叶子）照设计不跑非攻击 op；缠结只在真的敌人回合里挂上
        let mut s = lone(vs, 61);
        s.player.set(St::Tangled, 1);
        let s = end_turn_with_incoming(s, &no_incoming());
        assert_eq!(s.player.get(St::Tangled), 0, "两条回合结束路径都摘（共用 `end_turn_impl`）");
    }

    /// 劫掠者暴徒：殴打 7 -> 怒吼（+3 力量，不打人）-> 殴打 10 -> …；劫掠者刺客：一直致命射击 10。
    #[test]
    fn ruby_raider_brute_roars_every_other_turn_and_the_assassin_always_shoots() {
        let brute = def_id_of("劫掠者暴徒");
        let assassin = def_id_of("劫掠者刺客");
        let mut s = State::new(200, 3);
        s.add_enemy(brute, 31);
        s.add_enemy(assassin, 20);
        for _ in 0..10 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        let hp0 = s.player.hp;
        s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 7 - 10);
        s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 17 - 10, "怒吼不打人");
        assert_eq!(s.enemies[0].get(St::Strength), 3);
        s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, hp0 - 27 - 10 - 10, "殴打 7 + 3");
        s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[0].get(St::Strength), 6, "每两个回合 +3");
    }

    /// 批 6 这几手的意图签名。**紧绕藤蔓的副意图是 `CardDebuff` 不是 `Debuff`**（[源码] `CardDebuffIntent`）——
    /// 落进通用的 `PlayerStatus` 臂会签成 `Debuff`，整只对不齐。
    #[test]
    fn act1_overgrowth_batch_intent_signatures_match_the_source_intents() {
        let sig = |name: &str, mv: usize| {
            let id = def_id_of(name);
            crate::replay::move_signature(id, mv, &Entity::new(50), &Entity::new(80), 0)
        };
        let v = |xs: &[(&str, &str)]| xs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>();
        assert_eq!(sig("藤蔓蹒跚者", 0), v(&[("Attack", "6×2")]));
        assert_eq!(sig("藤蔓蹒跚者", 1), v(&[("Attack", "8"), ("CardDebuff", "")]));
        assert_eq!(sig("藤蔓蹒跚者", 2), v(&[("Attack", "16")]));
        assert_eq!(sig("劫掠者刺客", 0), v(&[("Attack", "10")]));
        assert_eq!(sig("劫掠者暴徒", 0), v(&[("Attack", "7")]));
        assert_eq!(sig("劫掠者暴徒", 1), v(&[("Buff", "")]));
    }

    /// 完整路径（`step(EndTurn)`）的敌人回合打完，「本回合被强制改招」要清掉。
    ///
    /// 不清的话，之后每个回合求解器叶子上的注入威胁（`end_turn_with_live_incoming`）都把这只敌人跳过 ——
    /// 被打醒的熟睡甲虫在推演的求解器眼里从此「不打人」。2026-09-14 之前只有注入路径清它。
    #[test]
    fn a_forced_move_flag_does_not_outlive_the_enemy_turn_on_the_full_path() {
        let beetle = def_id_of("熟睡甲虫");
        let mut s = State::new(80, 5);
        s.add_enemy(beetle, 86);
        for _ in 0..10 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        s.energy = 10;
        s.enemies[0].block = 0;
        for _ in 0..3 {
            s = step(s, Action::PlayCard { hand: hand_ix_of(&s, card::STRIKE), target: 0 });
        }
        assert_eq!(s.enemies[0].get(St::MoveForcedThisTurn), 1, "打醒 = 击晕换招，标记挂上");
        let s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[0].get(St::MoveForcedThisTurn), 0, "醒来那个敌人回合打完就用完了");

        let mut inc = [(0, 0); crate::state::MAX_ENEMIES];
        inc[0] = (16, 1);
        let hp = s.player.hp;
        let s = crate::step::end_turn_with_live_incoming(s, &inc);
        assert_eq!(s.player.hp, hp - 16, "下一手出击要真的打进来");
    }

    /// **拿实录对拍晕眩的张数**：`act2_f27_elite_entomancer` 每一帧出牌 / 喝药水之后，
    /// 内核四个牌区里的晕眩总数和抽牌堆张数，都要等于下一帧观测到的。
    ///
    /// **`verify` 看不见这件事** —— 一步对拍比手牌 / 弃牌堆 / 消耗堆 / 药水槽，**不比抽牌堆**，
    /// 而人体蜂房塞的晕眩全进抽牌堆。建它之前三种对拍模式在这条实录上一直全绿。
    /// 这份实录早于牌序补丁，所以只比张数、不比位置（剑柄打击抽上来的是不是晕眩，落在总数里）。
    #[test]
    fn personal_hive_dazed_counts_match_the_entomancer_trace() {
        use crate::replay::{parse_trace, Act, Replayer};
        let src = std::fs::read_to_string("traces/act2_f27_elite_entomancer.json").unwrap();
        let t = parse_trace(&src).unwrap();
        let mut r = Replayer::for_trace(&t);
        let obs_dazed = |o: &crate::replay::Obs| {
            let named = |n: &str| n == "晕眩";
            o.hand.iter().filter(|c| named(&c.name)).count()
                + o.draw.iter().filter(|n| named(n)).count()
                + o.discard.iter().filter(|n| named(n)).count()
                + o.exhaust.iter().filter(|n| named(n)).count()
        };
        let ker_dazed = |s: &State| {
            let zone = |z: &[u8], n: u8| {
                z[..n as usize].iter().filter(|&&c| s.cards[c as usize].id == card::DAZED).count()
            };
            zone(&s.hand, s.n_hand) + zone(&s.draw, s.n_draw) + zone(&s.disc, s.n_disc) + zone(&s.exh, s.n_exh)
        };
        let mut checked = 0;
        for i in 0..t.frames.len() - 1 {
            let (f, next) = (&t.frames[i], &t.frames[i + 1].obs);
            if next.enemies.is_empty() {
                break; // 战斗结束那一帧牌区被清空，没得比
            }
            let sy = r.sync(&f.obs);
            let st = match &f.action {
                Some(Act::Play { card_name, .. }) => {
                    let h = (0..sy.state.n_hand as usize)
                        .find(|&h| sy.names.get(sy.state.hand[h] as usize) == Some(card_name))
                        .expect("手牌里找不到这张");
                    step(sy.state, Action::PlayCard { hand: h as u8, target: 0 })
                }
                Some(Act::UsePotion { slot, .. }) => {
                    step(sy.state, Action::UsePotion { slot: *slot as u8, target: 0 })
                }
                _ => continue,
            };
            assert_eq!(ker_dazed(&st), obs_dazed(next), "帧{i} {:?} 之后晕眩总数", f.action);
            assert_eq!(st.n_draw as usize, next.draw.len(), "帧{i} 之后抽牌堆张数");
            checked += 1;
        }
        assert!(checked >= 9, "这条实录里出牌 + 喝药水至少 9 次，只比了 {checked} 次");
    }

    /// [源码] `DisintegrationPower.AfterSideTurnEndLate` + [玩家判定] 2026-09-14：
    /// **回合末剩的格挡先吃瓦解**，吃剩的才去挡敌人那一手；每个回合只发作一次。
    #[test]
    fn disintegration_eats_my_leftover_block_before_the_enemy_attacks() {
        let mk = || {
            let mut s = State::new(80, 5);
            s.add_enemy(enemy::DUMMY, 100); // 沙包：每回合打 12
            for _ in 0..10 {
                s.add_card(card::STRIKE, 0, 0);
            }
            let mut s = begin_combat(s);
            s.player.set(St::Disintegration, 6);
            s
        };
        let mut s = mk();
        s.player.block = 10;
        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, 80 - (6 + 12 - 10), "格挡 10：瓦解吃 6，剩 4 挡沙包的 12");

        let s = step(mk(), Action::EndTurn);
        assert_eq!(s.player.hp, 80 - 6 - 12, "没有格挡：瓦解 6 + 沙包 12，只发作一次");
    }

    /// 懒惰 3：本回合打满 3 张之后，`legal_actions` 一张都不给打，`step` 也原样返回。
    #[test]
    fn sloth_stops_the_fourth_card_in_a_turn() {
        let mut s = State::new(80, 5);
        s.add_enemy(enemy::DUMMY, 100);
        for _ in 0..10 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        s.player.set(St::Sloth, 3);
        s.energy = 10;
        for _ in 0..3 {
            let ix = hand_ix_of(&s, card::STRIKE);
            s = step(s, Action::PlayCard { hand: ix, target: 0 });
        }
        assert_eq!(s.cards_played, 3);
        let (acts, n) = crate::step::legal_actions(&s);
        assert!(
            !acts[..n].iter().any(|a| matches!(a, Action::PlayCard { .. })),
            "打满 3 张之后一张都不给打"
        );
        let ix = hand_ix_of(&s, card::STRIKE);
        let t = step(s, Action::PlayCard { hand: ix, target: 0 });
        assert_eq!(t.cards_played, 3, "step 也要拒绝（不变量 5：非法动作原样返回）");
        assert_eq!(t.n_hand, s.n_hand);
    }

    /// 心灵腐化 1：回合开始那一手少抽 1 张。
    #[test]
    fn mind_rot_draws_one_fewer_card_at_turn_start() {
        let mut s = State::new(80, 5);
        s.add_enemy(enemy::DUMMY, 100);
        for _ in 0..15 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        s.player.set(St::MindRot, 1);
        let s = step(s, Action::EndTurn);
        assert_eq!(s.n_hand, 4);
    }

    /// 诅咒出过几次**从我身上的 status 反推**：已经出过的是一段前缀，
    /// 三组的另一边各不相同、瓦解 6/7/8 的子集和互不相同 ⇒ 前缀长度唯一。
    #[test]
    fn curses_taken_reads_the_prefix_back_from_my_statuses() {
        use crate::content::{curses_taken, KNOWLEDGE_CURSES as K};
        let mk = |d: i32, m: i32, sl: i32, w: i32| {
            let mut e = crate::state::Entity::new(80);
            e.set(St::Disintegration, d);
            e.set(St::MindRot, m);
            e.set(St::Sloth, sl);
            e.set(St::WasteAway, w);
            e
        };
        assert_eq!(curses_taken(&mk(0, 0, 0, 0), &K), 0);
        assert_eq!(curses_taken(&mk(6, 0, 0, 0), &K), 1);
        assert_eq!(curses_taken(&mk(0, 1, 0, 0), &K), 1);
        assert_eq!(curses_taken(&mk(7, 1, 0, 0), &K), 2);
        assert_eq!(curses_taken(&mk(13, 0, 0, 0), &K), 2);
        assert_eq!(curses_taken(&mk(6, 0, 3, 0), &K), 2);
        assert_eq!(curses_taken(&mk(21, 0, 0, 0), &K), 3);
        assert_eq!(curses_taken(&mk(14, 0, 3, 0), &K), 3);
        assert_eq!(curses_taken(&mk(15, 1, 0, 0), &K), 3);
        assert_eq!(curses_taken(&mk(0, 1, 3, 1), &K), 3);
    }

    // -----------------------------------------------------------------------
    // 沙坑：即死倒计时（第 2 幕 Boss 无厌沙虫）
    // -----------------------------------------------------------------------

    /// 只有沙虫和一手打不动它的牌 —— 用来量倒计时，不量伤害。
    fn worm_only(sandpit: i32) -> State {
        let mut s = State::new(80, 9);
        s.add_enemy(enemy::THE_INSATIABLE, 321);
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        // 跳过液化地面那一手：这几个测试量的是倒计时本身，不是它怎么挂上的
        s.enemy_move[0] = 1;
        s.enemies[0].set(St::Sandpit, sandpit);
        s
    }

    /// 沙坑每个**敌人回合开始**减 1 —— 减到 0 那一刻**直接死**，和血量无关。
    ///
    /// [源码] `SandpitPower.AfterSideTurnStartLate(Enemy)` -> `Decrement`；
    /// 归零 ⇒ `AfterRemoved` ⇒ `CreatureCmd.Kill(玩家, force: true)`。
    #[test]
    fn sandpit_counts_down_once_per_enemy_turn_and_kills_at_zero() {
        let mut s = worm_only(3);
        let mut seen = vec![];
        for _ in 0..3 {
            s = step(s, Action::EndTurn);
            seen.push(s.enemies[0].get(St::Sandpit));
            if s.player_dead {
                break;
            }
        }
        assert_eq!(seen, vec![2, 1, 0], "每个敌人回合正好减 1 层");
        assert!(s.player_dead, "沙坑归零就该被吞掉");
        assert!(s.combat_over);
    }

    /// **`force: true` 挡掉瓶中精灵。** 这是它和"血量掉到 0"那条死法的分界：
    /// 后者走 `check_over` -> `try_fairy`，这条不走。
    #[test]
    fn sandpit_death_ignores_the_fairy() {
        let mut s = worm_only(1);
        s.potion_slots = 1;
        s.potions[0] = crate::state::potion::FAIRY;
        let s = step(s, Action::EndTurn);
        assert!(s.player_dead, "沙坑是强制死亡，瓶中精灵救不回来");
        assert_eq!(s.potions[0], crate::state::potion::FAIRY, "而且不该白白消耗掉那一瓶");
    }

    /// 狂乱逃离买回一个回合，**并且这一张实例自己涨 1 费**。
    #[test]
    fn frantic_escape_buys_a_turn_and_gets_more_expensive() {
        let mut s = worm_only(1);
        let cix = s.add_card(card::FRANTIC_ESCAPE, 0, 0);
        s.to_hand(cix);
        let ix = hand_ix_of(&s, card::FRANTIC_ESCAPE);
        let s = step(s, Action::PlayCard { hand: ix, target: 0 });
        assert_eq!(s.enemies[0].get(St::Sandpit), 2, "打出去给沙坑 +1");
        assert_eq!(s.cards[cix as usize].cost_delta, 1, "这一张实例永久 +1 费");
        let s = step(s, Action::EndTurn);
        assert!(!s.player_dead, "买到的这个回合必须真的活下来");
        assert_eq!(s.enemies[0].get(St::Sandpit), 1);
    }

    /// **L2 走的注入路径也要发 `EnemyTurnStart`。**
    ///
    /// 这条是整件事的要害：求解器只经由 `end_turn_with_incoming` 结算回合，
    /// 那条路上不发这个钩子的话，它**永远看不见自己会被吞掉** ——
    /// 2026-09-05 那次 AI 驾驶就是这么死的。
    #[test]
    fn the_injected_enemy_turn_also_ticks_the_sandpit() {
        let s = worm_only(1);
        let inc = crate::solver::Threat::default().incoming;
        let after = crate::step::end_turn_with_incoming(s, &inc);
        assert!(after.player_dead, "注入路径（0 点来袭）也必须触发沙坑的即死");
    }

    /// 即死倒计时那一项**对所有没有沙坑的局面恒为 0**。
    ///
    /// 这条是 `Weights::clock` 敢进**所有**目标函数（而 `power` 只敢进 `LEAF`）
    /// 的全部依据：风险面是空的。它扫的是整张 `ENEMIES` 表 ——
    /// 哪天有第二只怪挂得出沙坑，这个测试会先红。
    #[test]
    fn the_clock_term_is_inert_on_every_fight_without_a_sandpit() {
        let mut with_sandpit = vec![];
        for id in 0..ENEMIES.len() as u16 {
            let def = crate::content::enemy_def(id);
            let hands_it_out = def.start_status.iter().any(|(st, _)| *st == St::Sandpit)
                || def.moves.iter().any(|m| {
                    m.ops.iter().any(|o| {
                        matches!(o, crate::ops::EOp::SelfStatus { st: St::Sandpit, .. })
                    })
                });
            if hands_it_out {
                with_sandpit.push(def.name);
                continue;
            }
            let mut s = State::new(80, 5);
            s.add_enemy(id, 100);
            for _ in 0..8 {
                s.add_card(card::STRIKE, 0, 0);
            }
            let s = begin_combat(s);
            assert_eq!(
                crate::solver::eval(&s, &crate::solver::Weights::LEAF),
                crate::solver::eval(
                    &s,
                    &crate::solver::Weights { clock: 0, ..crate::solver::Weights::LEAF }
                ),
                "{} 身上没有沙坑，倒计时那一项必须一分钱都不动",
                def.name
            );
        }
        assert_eq!(with_sandpit, vec!["无厌沙虫"], "挂得出沙坑的敌人变了，重读 `Weights::clock` 的风险面");
    }

    /// **打掉敌人的血永远不会让倒计时那一项变差。**
    ///
    /// 和滚石那一项踩过的是同一个坑（叶评估里"收人头"变成负收益）：
    /// 缺口 = `horizon − 层数`，而 `horizon` 对血墙单调不减 ⇒ 缺口单调不减 ⇒
    /// 血墙变小这一项只会变好。这里用真的出牌走一遍，守的是"实现和推理一致"。
    #[test]
    fn the_clock_term_never_punishes_progress() {
        let mut s = worm_only(2);
        for _ in 0..5 {
            let cix = s.add_card(card::STRIKE, 0, 0);
            s.to_hand(cix);
        }
        let w = crate::solver::Weights::LEAF;
        let mut prev = crate::solver::eval(&s, &w) - crate::solver::eval(&s, &Weights0::of(&w));
        for _ in 0..3 {
            let ix = hand_ix_of(&s, card::STRIKE);
            s = step(s, Action::PlayCard { hand: ix, target: 0 });
            let now = crate::solver::eval(&s, &w) - crate::solver::eval(&s, &Weights0::of(&w));
            assert!(now >= prev, "打掉敌人的血把倒计时那一项从 {prev} 压到了 {now}");
            prev = now;
        }
    }

    /// 只是个取 `clock = 0` 的小工具，免得上面那条测试里到处写结构体字面量。
    struct Weights0;
    impl Weights0 {
        fn of(w: &crate::solver::Weights) -> crate::solver::Weights {
            crate::solver::Weights { clock: 0, ..*w }
        }
    }

    /// 单回合求解器**自己**就该在沙坑 = 1 的回合打出狂乱逃离：
    /// 结束回合 -> 敌人回合开始 -> 减到 0 -> 死，这一整条都在它的视野里。
    #[test]
    fn the_single_turn_solver_plays_frantic_escape_when_the_clock_is_at_one() {
        let mut s = worm_only(1);
        let cix = s.add_card(card::FRANTIC_ESCAPE, 0, 0);
        s.to_hand(cix);
        let threat = crate::solver::Threat::default();
        let line = crate::solver::solve_turn(&s, &threat, crate::solver::score::survive_first);
        let played: Vec<u16> = line
            .acts()
            .iter()
            .filter_map(|a| match a {
                Action::PlayCard { hand, .. } => Some(s.cards[s.hand[*hand as usize] as usize].id),
                _ => None,
            })
            .collect();
        assert!(
            played.contains(&card::FRANTIC_ESCAPE),
            "沙坑=1 时不打狂乱逃离就是死，求解器必须看得见：{played:?}"
        );
    }

    /// 没挂机器的敌人**行为一个字节都不能变** —— 30 只还在用固定循环，
    /// 而那 85/95 的基线就建在它们身上。
    #[test]
    fn cycle_enemies_still_walk_the_plain_loop() {
        let id = def_id_of("旧日雕像"); // loop_from = 2
        let def = crate::content::enemy_def(id);
        assert!(def.machine.is_none());
        let mut s = with_enemy(id, 100);
        // 允许集合永远是单元素，而且就是 move_index 给的那一手
        for _ in 0..6 {
            let want = crate::ops::move_index(def, s.enemy_move[0].wrapping_add(1) as u32);
            let allowed = crate::step::allowed_next(&s, 0);
            assert_eq!(allowed.count_ones(), 1, "固定循环的允许集合必须是单元素");
            assert_eq!(allowed.trailing_zeros() as usize, want);
            s = end_turn_with_incoming(s, &[(0, 0); crate::state::MAX_ENEMIES]);
        }
    }

    /// 树叶史莱姆（小）：两手互相不能连出 ⇒ 允许集合永远是**另一手**，单元素。
    #[test]
    fn cannot_repeat_shrinks_the_allowed_set_to_one() {
        let id = def_id_of("树叶史莱姆（小）");
        let mut s = with_enemy(id, 13);
        // 开局还没出过手，两手都可能 —— 这问的是 `allowed_initial`，
        // 和「这一手之后能接什么」是两件事
        assert_eq!(crate::step::allowed_initial(&s, 0).count_ones(), 2);
        let before = s.enemy_move[0];
        s = end_turn_with_incoming(s, &[(0, 0); crate::state::MAX_ENEMIES]);
        // 出完一手之后，**下一手已经选好了**，而且不可能还是刚出的那一手
        assert_ne!(s.enemy_move[0], before, "不能连出同一手");
        // 而现在这一手打完之后，同样只剩另一个选择
        let allowed = crate::step::allowed_next(&s, 0);
        assert_eq!(allowed.count_ones(), 1, "刚出过的那一手不能连出，只剩一个选择");
        assert_ne!(allowed.trailing_zeros() as u8, s.enemy_move[0]);
    }

    /// 树枝史莱姆（中）**开局固定吐黏液** —— 这一条固定循环表达不了：
    /// 源码的 initialState 是 STICKY_SHOT，不是那个随机分支。
    #[test]
    fn twig_slime_opens_with_a_fixed_move() {
        let id = def_id_of("树枝史莱姆（中）");
        let s = with_enemy(id, 28);
        let def = crate::content::enemy_def(id);
        assert_eq!(def.moves[crate::step::current_move_ix(def, &s, 0)].name, "吐黏液");
    }

    /// 冷却：飞蝇菌子的易伤孢子 cooldown = 3，出过之后连着三手都不能再出。
    ///
    /// **那个 3 是 cooldown 不是权重** —— 重载是
    /// `AddBranch(state, int cooldown, MoveRepeatType)`。我第一次读成权重了。
    #[test]
    fn cooldown_keeps_a_move_out_of_the_set() {
        let id = def_id_of("飞蝇菌子");
        let mut s = with_enemy(id, 47);
        // 手动把「刚出过易伤孢子(2)」写进历史
        s.enemy_move[0] = 2;
        s.enemy_hist[0] = [2, u8::MAX, u8::MAX, u8::MAX];
        let allowed = crate::step::allowed_next(&s, 0);
        assert_eq!(allowed & (1 << 2), 0, "cooldown 内不该再出易伤孢子");
        assert!(allowed.count_ones() >= 1);
    }

    /// 实验体的「适生力」：**三条命，而且死亡会把身上的 status 剥光**。
    ///
    /// 四条一起守：
    ///
    /// 1. **打死带适生力的那一具不算赢**，回满血换下一个形态（100 → 200 → 300）。
    ///    阶段判据是**最大生命**（观测量），不是内核私有计数器。
    /// 2. **死亡剥离**：[源码] `ShouldPowerBeRemovedAfterOwnerDeath()` 默认 true，
    ///    只有适生力/剧痛刺击重写成 false。于是**激怒和攒下的力量全部清零** ——
    ///    这正是玩家给的判定「只有第一条命打技能牌才涨力量」，源码在这里对上了。
    ///    我给它挂的易伤/虚弱同样一起没。
    /// 3. **第三条命没有适生力** ⇒ 打死就真的结束，不会有第四条。
    /// 4. **复苏之后按形态分岔**：形态 2 去连环爪击，形态 3 去撕裂。
    #[test]
    fn test_subject_has_three_lives_and_death_strips_its_powers() {
        fn move_name(s: &State, e: usize) -> &'static str {
            let d = crate::content::enemy_def(s.enemy_def[e]);
            d.moves[crate::step::current_move_ix(d, s, e)].name
        }
        fn kill(mut s: State) -> State {
            s.enemies[0].hp = 1;
            s.enemies[0].block = 0;
            let i = hand_ix_of(&s, card::STRIKE);
            step(s, Action::PlayCard { hand: i, target: 0 })
        }

        let mut s = State::new(400, 3);
        s.add_enemy(enemy::TEST_SUBJECT_BOSS, 100);
        for _ in 0..16 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        assert_eq!(s.enemies[0].max_hp, 100, "A1 是非进阶档 100，不是 ToughEnemies 的 111");
        assert_eq!(s.enemies[0].get(St::Adaptable), 1);
        assert_eq!(s.enemies[0].get(St::Rage), 2, "第一条命自带激怒 2");
        assert_eq!(move_name(&s, 0), "啃咬", "原装那只从啃咬起手");

        // 先给它喂点力量和易伤，验证复活会不会一起剥掉
        s.enemies[0].add(St::Strength, 8);
        s.enemies[0].add(St::Vulnerable, 3);

        // 第一条命 -> 第二条
        let s = kill(s);
        assert!(!s.combat_over, "带适生力的那一具死了不算赢");
        // **砍死那一帧它还是死的** —— 只是把下一个形态定好了。
        // [实测] 2026-08-30：那一帧观测里 `enemies` 是空的，而我还在出牌阶段。
        assert!(!s.enemies[0].alive(), "这一整个我方回合它都还是死的");
        assert_eq!(s.enemies[0].max_hp, 200, "但下一个形态已经定好");
        assert_eq!(s.enemies[0].get(St::Rage), 0, "**激怒被死亡剥离** —— 只有第一条命会涨力量");
        assert_eq!(s.enemies[0].get(St::Strength), 0, "攒的力量一起没");
        assert_eq!(s.enemies[0].get(St::Vulnerable), 0, "我挂的易伤也一起没");
        assert_eq!(s.enemies[0].get(St::Adaptable), 1, "适生力自己留着（源码重写成 false）");
        assert_eq!(s.enemies[0].get(St::PainfulStabs), 1, "形态 2 拿到剧痛刺击");
        assert_eq!(move_name(&s, 0), "复苏", "下一手被强制成复苏");

        // **死着的时候打不到它。** 这条就是这次修的要害：合成一步的话
        // 求解器会以为砍完还能接着输出，于是把收人头的牌排在回合中间，
        // 而实际那之后整回合的能量都没处花（2026-08-30 实战白扔了约 6 点能量）。
        let i = hand_ix_of(&s, card::STRIKE);
        let probe = step(s, Action::PlayCard { hand: i, target: 0 });
        assert!(!probe.enemies[0].alive(), "打不到死着的它");
        assert_eq!(probe.enemies[0].hp, s.enemies[0].hp, "血量一点没动");

        // **它自己的回合开始才回满**，然后走复苏那一手
        let s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[0].hp, 200, "自己回合开始才回满");
        assert_eq!(move_name(&s, 0), "连环爪击", "形态 2 的分岔");

        // 第二条命 -> 第三条
        let s = kill(s);
        assert!(!s.combat_over);
        assert_eq!(s.enemies[0].max_hp, 300, "换形态 3");
        assert_eq!(s.enemies[0].get(St::Nemesis), 1, "换上复仇宿敌");
        // **适生力这时候还在** —— 它正是"还留在场上"的凭据。
        // 在死亡当帧就摘掉的话，第三条命根本不会出场（写这条时就是这么错了一次）。
        assert_eq!(s.enemies[0].get(St::Adaptable), 1, "死亡当帧还不摘，摘了就不出场了");

        // 复苏之后该去阶段 3 的撕裂
        let s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[0].hp, 300, "第二次复苏同样是自己回合开始才回满");
        assert_eq!(s.enemies[0].get(St::Adaptable), 0, "回血之后才摘 ⇒ 第三条命是最后一条");
        assert_eq!(s.enemies[0].get(St::PainfulStabs), 0, "剧痛刺击一起摘掉");
        assert_eq!(move_name(&s, 0), "撕裂", "形态 3 的分岔");
        // 复仇宿敌：敌人回合末交替开关无实体，上面那一次 EndTurn 已经翻了一下
        assert_eq!(s.enemies[0].get(St::Intangible), 1, "第一个敌人回合末挂上无实体");
        let s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[0].get(St::Intangible), 0, "下一个回合末又摘掉 —— 隔回合交替");

        // 第三条命死了就真的结束
        let s = kill(s);
        assert!(s.combat_over, "没有适生力的那一具死了，战斗必须结束");
    }

    /// 巨斧机器人的「库存」：**死了换上一个库存少一个的自己，而且越死越强**。
    ///
    /// 三条一起守，因为它们互相依赖，单独测哪一条都漏得掉：
    ///
    /// 1. **重生次数有限。** `SummonCarryingSelfMinusOne` 用 `set` 覆盖掉
    ///    `start_status` 挂上的满库存 2；写成 `add` 或者直接用 `SummonN`，
    ///    每一具都满库存 ⇒ 内核以为这场仗**永远打不完**，
    ///    而 `cargo test` 不会红、对拍也不会红（语料里我三刀就打完了）。
    /// 2. **原装那只从锤击上勾拳起手，重生的从启动起手。**
    ///    [源码] `initialState = _stockOverrideAmount.HasValue ? 启动 : 锤击上勾拳`。
    ///    内核靠「启动放 0 号 + `summon_one` 把 `enemy_move` 置 0」表达，
    ///    把启动挪个位置就会静默地让重生的那具先去打人。
    /// 3. **启动给的力量 = 6 − 3×库存**（[源码] `3 * (2 - StockAmount)`）：
    ///    第二具 +3、第三具 +6。写死成某一个值，另外一具就系统性错，
    ///    而第三具那一头是**乐观**的（内核低估最难的那一具）。
    ///
    /// [实测] 2026-08-30 第 3 幕第 45 层：三具 74 / 71 / 77 血，
    /// 意图依次 `Attack:12,Debuff:` → `Defend:,Buff:`(力量3) → `Defend:,Buff:`(力量6)。
    #[test]
    fn axebot_respawns_twice_and_boots_up_stronger_each_time() {
        fn alive(s: &State) -> usize {
            (0..s.n_enemies as usize).find(|&i| s.enemies[i].alive()).expect("场上该有活的")
        }
        fn move_name(s: &State, e: usize) -> &'static str {
            let d = crate::content::enemy_def(s.enemy_def[e]);
            d.moves[crate::step::current_move_ix(d, s, e)].name
        }

        let mut s = State::new(300, 3);
        s.add_enemy(enemy::AXEBOT, 74);
        for _ in 0..16 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        assert_eq!(s.enemies[0].get(St::Stock), 2, "开局自带库存 2");
        assert_eq!(move_name(&s, 0), "锤击上勾拳", "原装那只不从启动起手");

        // 第一具 -> 第二具
        let e = alive(&s);
        s.enemies[e].hp = 1;
        let i = hand_ix_of(&s, card::STRIKE);
        let mut s = step(s, Action::PlayCard { hand: i, target: e as u8 });
        assert!(!s.combat_over, "带库存的那一具死了不算赢");
        let b2 = alive(&s);
        assert_eq!(s.enemies[b2].hp, 74, "换上来的是满血的一具");
        assert_eq!(s.enemies[b2].get(St::Stock), 1, "库存 -1，不是又回到 2");
        assert_eq!(move_name(&s, b2), "启动", "重生的从启动起手");

        // 让第二具真的启动一次：力量 = 6 - 3x1 = 3
        let s = step(s, Action::EndTurn);
        let b2 = alive(&s);
        assert_eq!(s.enemies[b2].get(St::Strength), 3, "第二具启动给 +3");
        assert_eq!(s.enemies[b2].block, 10, "启动同时给 10 格挡");

        // 第二具 -> 第三具
        // **格挡要一起清掉** —— 启动刚给了 10 点，一记打击 6 点根本打不死它。
        // 第一版测试就栽在这儿：它没死，"第三具" 拿到的还是第二具。
        let mut s = s;
        s.enemies[b2].hp = 1;
        s.enemies[b2].block = 0;
        let i = hand_ix_of(&s, card::STRIKE);
        let mut s = step(s, Action::PlayCard { hand: i, target: b2 as u8 });
        assert!(!s.combat_over);
        let b3 = alive(&s);
        assert_eq!(s.enemies[b3].get(St::Stock), 0, "库存见底 ⇒ status 直接不在了");
        assert_eq!(move_name(&s, b3), "启动");

        // 第三具启动：力量 = 6 - 0 = 6
        let s = step(s, Action::EndTurn);
        let b3 = alive(&s);
        assert_eq!(s.enemies[b3].get(St::Strength), 6, "第三具启动给 +6，越死越强");

        // 第三具死了就真的结束了 —— 没有第四具
        let mut s = s;
        s.enemies[b3].hp = 1;
        s.enemies[b3].block = 0;
        let i = hand_ix_of(&s, card::STRIKE);
        let s = step(s, Action::PlayCard { hand: i, target: b3 as u8 });
        assert!(s.combat_over, "库存 0 的那一具死了，战斗必须结束");
        assert!(
            (0..s.n_enemies as usize).all(|i| !s.enemies[i].alive()),
            "不该再冒出第四具"
        );
    }

    /// 青蛙骑士的半血分支：**血量条件和"只冲一次"必须同时成立**。
    ///
    /// [源码] `conditionalBranchState.AddState(BEETLE_CHARGE,
    ///          () => !HasBeetleCharged && CurrentHp < MaxHp / 2)`。
    /// 拆成两支（"或"）会让它冲完一次之后每轮都可能再冲 —— 那是**高估**
    /// 这只怪的伤害，方向反了也一样是错的。`ECond::All` 就是为这条加的。
    ///
    /// 阈值是 C# 整数除法：191 / 2 = 95，所以 95 不冲、94 冲。
    #[test]
    fn frog_knight_charges_only_below_half_and_only_once() {
        let id = def_id_of("青蛙骑士");
        let mut s = with_enemy(id, 191);
        // 站在「为了女王」那一手上（下标 2），分支就挂在它后面
        s.enemy_move[0] = 2;
        s.enemies[0].hp = 95;
        assert_eq!(crate::step::allowed_next(&s, 0), 1 << 0, "血量 >= 95 时回舌鞭，不冲锋");
        s.enemies[0].hp = 94;
        assert_eq!(crate::step::allowed_next(&s, 0), 1 << 3, "血量 < 95 且没冲过 ⇒ 甲虫冲锋");
        // `HasBeetleCharged` 读的就是 `enemy_used` 这个位掩码
        s.enemy_used[0] |= 1 << 3;
        s.enemies[0].hp = 20;
        assert_eq!(crate::step::allowed_next(&s, 0), 1 << 0, "一整场只冲一次，血再低也不冲");
    }

    /// 灵魂枢纽 [源码] `SoulNexus`：
    /// 灵魂灼烧(29) 起手，之后在 {灵魂灼烧(29), 大漩涡(6x4), 汲取生命(18+易伤2+虚弱2)}
    /// 三手里等权随机，且不能连出同一手（CannotRepeat）。
    #[test]
    fn soul_nexus_opens_with_soul_burn_and_transitions_to_other_moves() {
        let id = def_id_of("灵魂枢纽");
        let def = crate::content::enemy_def(id);
        assert!(def.machine.is_some());
        assert_eq!(def.moves.len(), 3);
        assert_eq!(def.moves[0].name, "灵魂灼烧");
        assert_eq!(def.moves[1].name, "大漩涡");
        assert_eq!(def.moves[2].name, "汲取生命");

        let mut s = with_enemy(id, 234);
        // 开局初始态必须是灵魂灼烧 (下标 0)
        assert_eq!(crate::step::current_move_ix(def, &s, 0), 0);
        // 出完第 0 手后，下一手不能是 0（Repeat::NotTwice），允许集合为 {1, 2}
        let allowed = crate::step::allowed_next(&s, 0);
        assert_eq!(allowed, (1 << 1) | (1 << 2), "刚出过灵魂灼烧(0)，接下来只能在大漩涡(1)和汲取生命(2)中选");

        // 模拟打出汲取生命(2)，验证它给玩家施加 2 层易伤与虚弱（在进入我方回合时衰减 1 层，剩余 1 层）
        s.enemy_move[0] = 2;
        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.get(St::Vulnerable), 1, "汲取生命施加 2 层易伤，进入我方回合衰减 1 层后余 1 层");
        assert_eq!(s.player.get(St::Weak), 1, "汲取生命施加 2 层虚弱，进入我方回合衰减 1 层后余 1 层");

        // 刚打完汲取生命(2)，选出的下一手必然不是 2
        assert_ne!(s.enemy_move[0], 2, "刚出过汲取生命(2)，下一手不能连出 2");
        // 下一手出完之后的允许集合禁掉那一手自己，剩 2 个候选
        let allowed2 = crate::step::allowed_next(&s, 0);
        assert_eq!(allowed2 & (1 << s.enemy_move[0]), 0, "出完当前手后不能连出自身");
        assert_eq!(allowed2.count_ones(), 2, "三手等权随机每次禁掉刚出的那一手");
    }

    /// **`hp_loss_hits` 数的是"挨穿过几次"，不是"掉了多少血"。**
    ///
    /// [源码] 的过滤器是 `DamageReceivedEntry` 且 `Result.UnblockedDamage > 0`：
    /// 被完全格挡的一次都不算，而**来源一概不问** —— 敌人打的、荆棘反弹的、
    /// 放血那种自伤（[源码] `Bloodletting` 走的也是 `CreatureCmd.Damage`）全算。
    /// 而且它**整场累加**，不跟着回合清零（`hp_lost_this_turn` 才是每回合的）。
    #[test]
    fn hp_loss_hits_counts_unblocked_instances_not_hp() {
        let mut s = base_state();
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        assert_eq!(s.hp_loss_hits, 0);

        // 一次挨 20 点：算 1 次，不是 20 次
        crate::step::take_attack_hit(&mut s, 0, 20);
        assert_eq!(s.hp_loss_hits, 1);
        assert_eq!(s.hp_lost_this_turn, 20, "血量那一栏数的还是血");

        // 被完全挡住：一次都不算
        s.player.block = 50;
        crate::step::take_attack_hit(&mut s, 0, 20);
        assert_eq!(s.hp_loss_hits, 1, "全被格挡吃掉 ⇒ UnblockedDamage 是 0");

        // 挡不满：算 1 次
        s.player.block = 5;
        crate::step::take_attack_hit(&mut s, 0, 20);
        assert_eq!(s.hp_loss_hits, 2);

        // 跨回合不清零
        let before = s.hp_loss_hits;
        let s = step(s, Action::EndTurn);
        assert_eq!(s.hp_lost_this_turn, 0, "这一栏每回合清零");
        assert!(s.hp_loss_hits >= before, "这一栏不清零（敌人这一手可能又加了）");
    }

    /// 扯碎「在本场战斗中，你每失去过一次生命值，这张牌就额外造成一次伤害」。
    ///
    /// [源码] `TearAsunder`：`WithHitCount(Calculate(target))`，
    /// `Calculate = 0 + 1 × (1 + M)` = **1 + M**。
    ///
    /// 两件事一起守：
    /// 1. 段数跟着 `hp_loss_hits` 走 —— **卡面上那个「造成3次」是快照不是定义**，
    ///    第一版就是照它写死了 `hits: 3`；
    /// 2. 段数在**第一段落地之前**就定死（源码里是构造命令时求值的）。
    ///    敌人带荆棘时每一段都会反弹到我身上、把计数器顶上去，
    ///    循环里重读的话这张牌会自己越滚越长。
    #[test]
    fn tear_asunder_hit_count_is_one_plus_unblocked_hits_and_frozen_at_play_time() {
        fn play_tear(hits_before: u8, thorns: i32) -> i32 {
            let mut s = State::new(80, 3);
            s.add_enemy(enemy::DUMMY, 4000);
            for _ in 0..6 {
                s.add_card(card::TEAR_ASUNDER, 0, 0);
            }
            let mut s = begin_combat(s);
            s.hp_loss_hits = hits_before;
            s.enemies[0].set(St::Thorns, thorns);
            s.energy = 9;
            let hp_before = s.enemies[0].hp;
            let i = hand_ix_of(&s, card::TEAR_ASUNDER);
            let after = step(s, Action::PlayCard { hand: i, target: 0 });
            hp_before - after.enemies[0].hp
        }

        // 基础伤害 5，没挨过打 ⇒ 1 段
        assert_eq!(play_tear(0, 0), 5);
        // 挨穿过 2 次 ⇒ 3 段
        assert_eq!(play_tear(2, 0), 15);
        assert_eq!(play_tear(7, 0), 40, "act1_f17 卡面「命中8次」那一档");

        // 敌人带荆棘：每一段反弹 1 点到我身上，`hp_loss_hits` 一路涨，
        // 但段数必须还是 3 —— 涨到 4、5 段就说明段数被写在循环里重读了。
        assert_eq!(play_tear(2, 1), 15, "段数在打出的那一刻就定死了");
    }

    /// 升级只加伤害，不改费用（[源码] `OnUpgrade` 只有 `UpgradeValueBy(2)`）。
    #[test]
    fn tear_asunder_upgrade_only_touches_damage() {
        let d = card(card::TEAR_ASUNDER);
        assert_eq!(d.cost, 2);
        assert_eq!(d.cost_upg, 2, "费用不变 —— 观测到的 扯碎+ 也是 2 费");
        assert!(!d.exhausts);
        assert!(d.targeted);
        assert_eq!(d.ops, &[crate::ops::Op::DamagePerHpLossHit { base: 5 }]);
        assert_eq!(d.ops_upg, &[crate::ops::Op::DamagePerHpLossHit { base: 7 }]);
    }

    /// **复活窗口里无目标牌照样打得出去。**
    ///
    /// [实测] act3_f48 帧11/12：实验体被砍掉一条命、观测里 `enemies: []`、
    /// 出牌阶段还开着，玩家打了坚定不移+ 和 时候未到，游戏**都接受了**。
    /// 打不出去的只有攻击牌 —— 它们没有目标。
    ///
    /// 这条守的是 `step` 和 `legal_actions` **对同一个局面给同一个答案**。
    /// 原来 `step` 的 `PlayCard` 在 `first_alive()` 为空时无条件原样返回，
    /// 于是无目标牌被 `legal_actions` 允许、被 `step` 拒绝 —— 而 `step` 拒绝的
    /// 表现形式是"状态没变"，**不报错**。药水那一支从来没这个洞。
    #[test]
    fn untargeted_cards_still_play_while_the_boss_is_waiting_to_revive() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::TEST_SUBJECT_BOSS, 100);
        for _ in 0..5 {
            s.add_card(card::DEFEND, 0, 0);
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        // 砍掉这一条命：血 0、适生力还在（[源码] 摘除发生在它自己的复苏那一手）
        s.enemies[0].hp = 0;
        assert!(!s.enemies[0].alive());
        assert!(s.any_enemy_present(), "带适生力 = 还在场上");
        s.energy = 3;

        let (acts, n) = legal_actions(&s);
        let d = hand_ix_of(&s, card::DEFEND);
        assert!(
            acts[..n].iter().any(|a| matches!(a, Action::PlayCard { hand, .. } if *hand == d)),
            "legal_actions 允许无目标牌"
        );

        let block_before = s.player.block;
        let after = step(s, Action::PlayCard { hand: d, target: 0 });
        assert_ne!(after, s, "step 也必须接受它，否则两个入口对不上");
        assert!(after.player.block > block_before, "防御该真的给出格挡");
        assert_eq!(after.energy, 2);

        // 反面：攻击牌没有目标，两边都该拒
        let t = hand_ix_of(&s, card::STRIKE);
        assert!(
            !acts[..n].iter().any(|a| matches!(a, Action::PlayCard { hand, .. } if *hand == t)),
            "有目标牌在没有活敌人时不该出现在候选里"
        );
        assert_eq!(step(s, Action::PlayCard { hand: t, target: 0 }), s, "打不出去");
    }

    /// 实验体 [源码] `TestSubject`（第 3 幕 Boss）：
    /// 验证开局激怒 2 + 适生力 1，以及阶段 1 啃咬(1) <-> 头槌猛击(2)、
    /// 阶段 2 连环爪击(3)、阶段 3 撕裂(4) -> 猛扑(5) -> 灼热咆哮(6) 的状态转移。
    #[test]
    fn test_subject_forms_and_moves_definition() {
        let id = def_id_of("实验体");
        let def = crate::content::enemy_def(id);
        assert!(def.machine.is_some());
        assert_eq!(def.moves.len(), 7);
        assert_eq!(def.moves[0].name, "复苏");
        assert_eq!(def.moves[1].name, "啃咬");
        assert_eq!(def.moves[2].name, "头槌猛击");
        assert_eq!(def.moves[3].name, "连环爪击");
        assert_eq!(def.moves[4].name, "撕裂");
        assert_eq!(def.moves[5].name, "猛扑");
        assert_eq!(def.moves[6].name, "灼热咆哮");

        let mut s = State::new(80, 3);
        s.add_enemy(id, 111);
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        assert_eq!(s.enemies[0].get(St::Rage), 2, "开局自带激怒 2");
        assert_eq!(s.enemies[0].get(St::Adaptable), 1, "开局自带适生力 1");
        // 起手是啃咬 (1)
        assert_eq!(crate::step::current_move_ix(def, &s, 0), 1);
        assert_eq!(crate::step::allowed_next(&s, 0), 1 << 2, "啃咬(1)后固定接头槌猛击(2)");

        // 验证打出技能牌会触发激怒（+2 力量）
        s.energy = 3;
        let str_before = s.enemies[0].get(St::Strength);
        s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.enemies[0].get(St::Strength), str_before + 2, "打出技能牌使实验体获得 2 点力量");

        // 验证 Phase 1 循环：啃咬(1) -> 头槌猛击(2) -> 啃咬(1)
        s.enemy_move[0] = 2;
        assert_eq!(crate::step::allowed_next(&s, 0), 1 << 1, "头槌猛击(2)后固定回啃咬(1)");

        // 验证 Phase 2：连环爪击(3) 自循环
        s.enemy_move[0] = 3;
        assert_eq!(crate::step::allowed_next(&s, 0), 1 << 3, "连环爪击(3)自循环");

        // 验证 Phase 3：撕裂(4) -> 猛扑(5) -> 灼热咆哮(6) -> 撕裂(4)
        s.enemy_move[0] = 4;
        assert_eq!(crate::step::allowed_next(&s, 0), 1 << 5, "撕裂(4)后接猛扑(5)");
        s.enemy_move[0] = 5;
        assert_eq!(crate::step::allowed_next(&s, 0), 1 << 6, "猛扑(5)后接灼热咆哮(6)");
        s.enemy_move[0] = 6;
        assert_eq!(crate::step::allowed_next(&s, 0), 1 << 4, "灼热咆哮(6)后接撕裂(4)");

        // 模拟执行灼热咆哮(6)：向弃牌堆加入 3 张灼伤，自身 +2 力量
        let disc_before = s.n_disc;
        let str_before_growl = s.enemies[0].get(St::Strength);
        s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[0].get(St::Strength), str_before_growl + 2, "灼热咆哮给自己 +2 力量");
        let burns = (0..s.n_disc as usize).filter(|&i| s.cards[s.disc[i] as usize].id == card::BURN).count();
        assert!(burns >= 3, "弃牌堆中应有加入的 3 张灼伤，实际 {burns}");
    }

    /// **敌人持有的覆甲每回合把墙重新砌起来**，层数每回合掉 1（第 1 回合不掉）。
    ///
    /// 这个测试是补一颗**哑弹**：改之前内核只有玩家那两条规则，而 `fire` 对
    /// `TurnEnd` 两边都发 —— 敌人在**我的**回合末拿到的格挡，紧接着就被
    /// `begin_enemy_turn` 清零了。于是 191 血的青蛙骑士每回合白送 15 点墙，
    /// 内核一点都看不见，**而且没有任何测试会失败**。
    ///
    /// 期望值是 2026-08-30 第 3 幕第 43 层的实录逐帧读数：
    /// 我的第 1/2/3/4 回合，它的格挡和覆甲层数都是 15 / 15 / 14 / 13。
    /// 格挡和层数**同值**这一点要紧：它说明回合末给的是已经掉过层的那个数。
    #[test]
    fn enemy_plating_rebuilds_the_wall_every_turn_and_decays_by_one() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::FROG_KNIGHT, 191);
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        let mut seen = Vec::new();
        for _ in 0..4 {
            seen.push((s.enemies[0].block, s.enemies[0].get(St::PlatedArmor)));
            s = step(s, Action::EndTurn);
        }
        assert_eq!(seen, vec![(15, 15), (15, 15), (14, 14), (13, 13)], "实录：15/15/14/13");
    }

    /// 反向守卫：**玩家**持有的覆甲一个字节都没变。
    ///
    /// 加 `TCond::OwnerIsPlayer` 那道门的风险正是把玩家这一半也关掉。
    /// 舌鞭 13 点，覆甲 4 在我的回合末给 4 点格挡 ⇒ 只该掉 9 点。
    /// 关掉了就是掉 13；敌人那条规则漏了 `OwnerIsEnemy` 的门、玩家在
    /// 敌人回合末又拿一次的话就是掉 5 —— 三种情况这一个数字全分得开。
    #[test]
    fn player_plating_is_unchanged_by_the_enemy_side_rules() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::FROG_KNIGHT, 191);
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.player.add(St::PlatedArmor, 4);
        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.hp, 71, "玩家覆甲仍然在我的回合末给 4 点格挡");
        assert_eq!(s.player.get(St::PlatedArmor), 3, "我的回合开始掉 1 层");
    }

    /// 武装+「获得5点格挡。升级你手牌中的所有牌。」
    ///
    /// 2026-08-22 第14层实录抓到：游戏把手里两张防御都升成了防御+，
    /// 内核一张都没升。这个测试是那一帧的最小复现。
    #[test]
    fn armaments_upgraded_upgrades_every_card_in_hand() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::NIBBIT, 44);
        let arm = s.add_card(card::ARMAMENTS, F_UPGRADED, 0);
        let d1 = s.add_card(card::DEFEND, 0, 0);
        let d2 = s.add_card(card::DEFEND, 0, 0);
        let mut s = begin_combat(s);
        // 手动摆手牌，绕开洗牌
        s.n_hand = 0;
        s.n_draw = 0;
        for c in [arm, d1, d2] {
            s.hand[s.n_hand as usize] = c;
            s.n_hand += 1;
        }
        let ix = hand_ix_of(&s, card::ARMAMENTS);
        let s = step(s, Action::PlayCard { hand: ix, target: 0 });
        assert_eq!(s.player.block, 5, "武装+ 升级前后都是 5 点格挡");
        for c in [d1, d2] {
            assert!(
                s.cards[c as usize].flags & F_UPGRADED != 0,
                "武装+ 该把手里每一张牌都升级"
            );
        }
    }

    /// 瓶中精灵：**将要死时**丢掉这瓶，回到最大生命值的 30%，而且**不算死**。
    ///
    /// [实测] 2026-08-22 第1幕Boss 帧46：上限 80、7 血挨 21 点 → 正好 24。
    /// 在补这条之前，那一帧是语料里唯一一个真 MISMATCH（游戏 24 / 内核 0）。
    #[test]
    fn fairy_in_a_bottle_prevents_death_and_heals_to_30_percent() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::NIBBIT, 44);
        for _ in 0..5 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        s.potion_slots = 1;
        s.potions[0] = crate::replay::map_potion("FAIRY_IN_A_BOTTLE");
        s.player.hp = 7;
        // 挨一下必死的伤害
        let s = end_turn_with_incoming(s, &[(21, 1), (0, 0), (0, 0), (0, 0), (0, 0)]);
        assert!(!s.player_dead, "瓶中精灵该挡下这次死亡");
        assert!(!s.combat_over, "挡下了就不该判战斗结束");
        assert_eq!(s.player.hp, 24, "回到 80 的 30%");
        assert_eq!(s.potions[0], crate::state::potion::NONE, "这瓶该被消耗掉");
    }

    /// 瓶中精灵**喝不了**（[源码] `PotionUsage.Automatic`）。
    /// 不挡的话搜索会多出一条"喝掉救命药水什么也不发生"的分支。
    #[test]
    fn fairy_in_a_bottle_is_never_a_legal_action() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::NIBBIT, 44);
        for _ in 0..5 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        s.potion_slots = 2;
        s.potions[0] = crate::replay::map_potion("FAIRY_IN_A_BOTTLE");
        s.potions[1] = crate::replay::map_potion("BLOCK_POTION");
        let (acts, n) = legal_actions(&s);
        let pots: Vec<u8> = acts[..n]
            .iter()
            .filter_map(|a| match a {
                Action::UsePotion { slot, .. } => Some(*slot),
                _ => None,
            })
            .collect();
        assert!(!pots.contains(&0), "瓶中精灵不该出现在合法动作里");
        assert!(pots.contains(&1), "格挡药水该照常可喝");
    }

    /// 只消耗一瓶：身上两瓶精灵时，挡下一次死亡只用掉一瓶。
    #[test]
    fn only_one_fairy_is_spent_per_death() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::NIBBIT, 44);
        for _ in 0..5 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        s.potion_slots = 2;
        s.potions[0] = crate::replay::map_potion("FAIRY_IN_A_BOTTLE");
        s.potions[1] = crate::replay::map_potion("FAIRY_IN_A_BOTTLE");
        s.player.hp = 5;
        let s = end_turn_with_incoming(s, &[(30, 1), (0, 0), (0, 0), (0, 0), (0, 0)]);
        assert_eq!(s.player.hp, 24);
        assert_eq!(s.potions[0], crate::state::potion::NONE);
        assert_eq!(s.potions[1], crate::replay::map_potion("FAIRY_IN_A_BOTTLE"), "第二瓶该留着");
    }

    /// 仪式兽的耕地闸门：血量掉到 **≤150** 时清空力量 + 打进眩晕。
    ///
    /// 这是这场 Boss 战的全部战术核心 —— 第一阶段每回合 +2 力量一路涨，
    /// 而闸门会把涨上去的全部清零。[源码] `PlowPower.AfterDamageReceived`。
    #[test]
    fn plow_gate_strips_strength_and_stuns_the_beast_at_150() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::CEREMONIAL_BEAST, 252);
        for _ in 0..5 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        // 手动摆成"践地打完、耕地涨了几回合"的样子
        s.enemies[0].set(St::Plow, 150);
        s.enemies[0].set(St::Strength, 8);

        // 还没到闸门：打一下，力量原样
        s.enemies[0].hp = 160;
        let ix = hand_ix_of(&s, card::STRIKE);
        let s1 = step(s, Action::PlayCard { hand: ix, target: 0 });
        assert_eq!(s1.enemies[0].get(St::Strength), 8, "154 > 150，闸门不该开");
        assert_eq!(s1.enemies[0].get(St::Plow), 150);

        // 跨过闸门：力量清零、当前手变成眩晕、耕地自己消失
        let mut s2 = s1;
        s2.enemies[0].hp = 152;
        let ix = hand_ix_of(&s2, card::STRIKE);
        let s2 = step(s2, Action::PlayCard { hand: ix, target: 0 });
        assert!(s2.enemies[0].hp <= 150, "这一下该打到 150 以下");
        assert_eq!(s2.enemies[0].get(St::Strength), 0, "闸门开了要清空力量");
        assert_eq!(s2.enemies[0].get(St::Plow), 0, "耕地自己移除");
        let d = crate::content::enemy_def(s2.enemy_def[0]);
        assert_eq!(d.moves[crate::step::current_move_ix(d, &s2, 0)].name, "眩晕",
                   "被打进眩晕");
    }

    /// 轰鸣：这一回合**最多只能出 1 张牌**，但药水不受限制。
    ///
    /// [源码] `RingingPower.ShouldPlay` 判的是"被附魔的牌 + 本回合已出过牌"，
    /// 而它 afflict 所有牌，所以等价于每回合 1 张。
    /// **药水不走 `ShouldPlay`** —— 早先我把它写成提前 return，把药水也挡了。
    #[test]
    fn ringing_allows_only_one_card_per_turn_but_not_potions() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::CEREMONIAL_BEAST, 252);
        for _ in 0..5 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        s.player.set(St::Ringing, 1);
        s.potion_slots = 1;
        s.potions[0] = crate::replay::map_potion("BLOCK_POTION");

        // 第一张照常可以打
        let (_, n0) = legal_actions(&s);
        assert!(n0 > 1, "轰鸣不该挡住这一回合的第一张牌");
        let ix = hand_ix_of(&s, card::STRIKE);
        let s = step(s, Action::PlayCard { hand: ix, target: 0 });

        // 之后就只剩结束回合和药水
        let (acts, n) = legal_actions(&s);
        let plays = acts[..n].iter().filter(|a| matches!(a, Action::PlayCard { .. })).count();
        let potions = acts[..n].iter().filter(|a| matches!(a, Action::UsePotion { .. })).count();
        assert_eq!(plays, 0, "轰鸣下出过一张之后不该还能出牌");
        assert_eq!(potions, 1, "药水不受轰鸣限制");
    }

    /// 轰鸣在**我的回合结束**时消失，不是回合开始。
    /// 放进 `TURN_SCOPED` 会让它在生效前就被清掉。
    #[test]
    fn ringing_is_cleared_at_my_turn_end_not_turn_start() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::CEREMONIAL_BEAST, 252);
        for _ in 0..5 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        s.player.set(St::Ringing, 1);
        let s = end_turn_with_incoming(s, &[(0, 0); crate::state::MAX_ENEMIES]);
        assert_eq!(s.player.get(St::Ringing), 0, "我的回合结束时该消失");
    }

    /// 异蛙寄生虫：**打死本体不算赢**，会冒出 4 只扭动虫。
    ///
    /// 这是这场精英唯一致命的一条：内核要是把宿主之死判成胜利，
    /// 求解器就会给出一条"这回合能打完"的假线，而实际上第二阶段
    /// 还有 4×19 血在等着。[源码] `InfestedPower.ShouldStopCombatFromEnding`。
    #[test]
    fn killing_the_parasite_spawns_four_wrigglers_and_combat_goes_on() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::PHROG_PARASITE, 64);
        for _ in 0..5 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        assert_eq!(s.enemies[0].get(St::Infested), 4, "开局自带寄生物 4 层");
        // 直接把本体打死
        s.enemies[0].hp = 1;
        let ix = hand_ix_of(&s, card::STRIKE);
        let s = step(s, Action::PlayCard { hand: ix, target: 0 });
        assert!(!s.enemies[0].alive(), "本体该死了");
        assert!(!s.combat_over, "打死本体**不该**判成胜利");
        let alive = (0..s.n_enemies as usize).filter(|&i| s.enemies[i].alive()).count();
        assert_eq!(alive, 4, "该冒出 4 只扭动虫，实际 {alive} 只");
        for i in 0..s.n_enemies as usize {
            if s.enemies[i].alive() {
                let d = crate::content::enemy_def(s.enemy_def[i]);
                assert_eq!(d.name, "扭动虫", "名字必须是游戏里显示的那个 —— 它是连接键");
                assert_eq!(d.moves[crate::step::current_move_ix(d, &s, i)].name, "眩晕",
                           "召唤出来第一手是眩晕（StartStunned）");
                assert_eq!(s.enemies[i].get(St::Minion), 0,
                           "Wriggler 不是爪牙 —— 是爪牙的话宿主一死战斗就结束了");
            }
        }
    }

    /// 缠绕：**我的回合结束**时受到等于层数的伤害，而且那点伤害**走格挡**。
    ///
    /// 走不走格挡是这条规则唯一的判断点：[源码] 用 `CreatureCmd.Damage(..,
    /// Unpowered, ..)`，和灼伤逐字同构，而灼伤走格挡是玩家判过的。
    /// 写成 `OwnerLoseHp` 就会绕过格挡 —— 这个测试就是那个实现的处刑帧。
    #[test]
    fn constrict_damages_through_block_at_my_turn_end() {
        let id = def_id_of("蛇行扼杀者");

        // 有格挡：挡下来，不掉血
        let mut s = with_enemy(id, 55);
        s.player.set(St::Constrict, 3);
        s.player.block = 5;
        let s1 = end_turn_with_incoming(s, &[(0, 0); crate::state::MAX_ENEMIES]);
        assert_eq!(s1.player.hp, 80, "3 点被 5 点格挡吃掉，不该掉血");

        // 没格挡：掉 3 点
        let mut s = with_enemy(id, 55);
        s.player.set(St::Constrict, 3);
        s.player.block = 0;
        let s2 = end_turn_with_incoming(s, &[(0, 0); crate::state::MAX_ENEMIES]);
        assert_eq!(s2.player.hp, 77, "没格挡就实扣 3 点");
    }

    /// 缠绕是**计数型，不衰减** —— [源码] `PowerStackType.Counter`。
    /// 易伤/虚弱/脆弱那三个才每回合掉一层。
    #[test]
    fn constrict_does_not_decay() {
        let id = def_id_of("蛇行扼杀者");
        let mut s = with_enemy(id, 55);
        s.player.set(St::Constrict, 3);
        for _ in 0..3 {
            s = end_turn_with_incoming(s, &[(0, 0); crate::state::MAX_ENEMIES]);
        }
        assert_eq!(s.player.get(St::Constrict), 3, "缠绕不该衰减");
    }

    /// 蛇行扼杀者：缠绕 → {重击|鞭击} → 缠绕 → … 严格交替。
    #[test]
    fn strangler_alternates_constrict_and_an_attack() {
        let id = def_id_of("蛇行扼杀者");
        let def = crate::content::enemy_def(id);
        let mut s = with_enemy(id, 999);
        assert_eq!(def.moves[crate::step::current_move_ix(def, &s, 0)].name, "缠绕",
                   "起始态是缠绕");
        // 缠绕之后：两条攻击手都可能，缠绕自己不可能
        let a = crate::step::allowed_next(&s, 0);
        assert_eq!(a, 0b110, "缠绕之后只能接重击或鞭击");
        s = end_turn_with_incoming(s, &[(0, 0); crate::state::MAX_ENEMIES]);
        // 打完攻击手之后：只能回缠绕
        let ix = crate::step::current_move_ix(def, &s, 0);
        assert!(ix == 1 || ix == 2, "第二手该是攻击");
        assert_eq!(crate::step::allowed_next(&s, 0), 0b001, "攻击之后固定回缠绕");
    }

    // ---- 组装师那一场（2026-08-22 第3幕第45层实录）----

    /// 高压：[源码] `HighVoltagePower.AfterSideTurnEnd` —— **敌人回合结束**
    /// 给自己加等于层数的力量，`Counter` 型不衰减，所以一路涨。
    ///
    /// 这条是新钩子 `Hook::EnemyTurnEnd` 的**唯一**消费者。
    ///
    /// **必须走 `step(EndTurn)` 而不是 `end_turn_with_incoming`**：后者是
    /// 注入路径（敌人这一手打多少由调用方给定），它**不执行敌人的 ops**，
    /// 钩子自然也不发作。四条新测试第一次全红就是踩的这个。
    #[test]
    fn high_voltage_grows_its_own_strength_every_enemy_turn() {
        let id = def_id_of("电击机器人");
        let mut s = with_enemy(id, 999);
        assert_eq!(s.enemies[0].get(St::HighVoltage), 2, "开局自带 2 层");
        assert_eq!(s.enemies[0].get(St::Strength), 0, "还没结算过敌人回合");

        s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[0].get(St::Strength), 2, "第一个敌人回合结束 +2");
        s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[0].get(St::Strength), 4, "不衰减，接着涨");
        assert_eq!(s.enemies[0].get(St::HighVoltage), 2, "层数自己不变");
    }

    /// 高压是 **side 级**钩子，不是每只敌人各触发一次全场。
    /// 两只电击机器人时，各自只该 +2，不是各 +4。
    /// （把 `fire` 放进 `enemy_turn` 的循环里就会红。）
    #[test]
    fn high_voltage_fires_once_per_side_turn_not_once_per_enemy() {
        let id = def_id_of("电击机器人");
        let mut s = State::new(80, 3);
        s.add_enemy(id, 999);
        s.add_enemy(id, 999);
        let s = begin_combat(s);
        let s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[0].get(St::Strength), 2);
        assert_eq!(s.enemies[1].get(St::Strength), 2, "两只各 +2，不是各 +4");
    }

    /// 噪音机器人：**两张眩晕去两个不同的牌堆**。
    /// [源码] 一张 `PileType.Discard`、一张 `PileType.Draw` + `CardPilePosition.Random`。
    ///
    /// 两张都塞弃牌堆是错的：进抽牌堆的那张**这一局就可能抽到**。
    ///
    /// **抽牌堆必须够大**：牌堆空的时候我下个回合开始抽 5 张会把弃牌堆整个
    /// 洗回抽牌堆，两张眩晕就都跑到手上，这条判据当场失效（第一版就是这么红的）。
    /// 所以垫 12 张防御，抽 5 张之后弃牌堆不会被动过。
    #[test]
    fn noisebot_puts_one_dazed_in_discard_and_one_in_the_draw_pile() {
        let mut s = State::new(80, 3);
        s.add_enemy(def_id_of("噪音机器人"), 999);
        for _ in 0..12 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.n_hand = 0;
        let s = step(s, Action::EndTurn);

        let dazed_in = |pile: &[u8], n: u8, st: &State| {
            pile[..n as usize].iter().filter(|&&c| st.cards[c as usize].id == card::DAZED).count()
        };
        assert_eq!(dazed_in(&s.disc, s.n_disc, &s), 1, "弃牌堆恰好一张眩晕");
        assert_eq!(
            dazed_in(&s.draw, s.n_draw, &s) + dazed_in(&s.hand, s.n_hand, &s),
            1,
            "另一张进的是抽牌堆（可能已经被抽进手里，两处合起来算）"
        );
    }

    /// 护卫机器人：**给非爪牙加格挡，不是给自己**。
    /// [源码] `Guardbot.GuardMove` 遍历的是场上的组装师。
    /// 写成 `EOp::Block` 的话它会给自己 15 点，那是个静默的错。
    #[test]
    fn guardbot_blocks_the_fabricator_and_not_itself() {
        let mut s = State::new(80, 3);
        s.add_enemy(def_id_of("组装师"), 150);
        s.add_enemy(def_id_of("护卫机器人"), 20);
        let s = begin_combat(s);
        assert_eq!(s.enemies[1].get(St::Minion), 1, "护卫是爪牙");
        let s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[0].block, 15, "组装师拿到 15 格挡");
        assert_eq!(s.enemies[1].block, 0, "护卫自己一点都没有");
    }

    /// 组装师的分支：`CanFabricate = 同一边活着的（含自己）< 4`。
    /// [实测] 第3幕第45层第3回合场上 4 只 ⇒ 出「分解」(11 点)。
    ///
    /// 造得动的时候允许集合是 {制造, 制造打击} 两支（那一步是随机的，
    /// 按不变量 4 本来就预测不了）；造不动的时候塌成 {分解} 一支。
    #[test]
    fn fabricator_stops_fabricating_once_four_are_alive_counting_itself() {
        let id = def_id_of("组装师");
        // 只有它自己 —— 1 < 4，造得动
        let s = with_enemy(id, 150);
        assert_eq!(crate::step::allowed_next(&s, 0), 0b011, "造得动：制造 或 制造打击");

        // 补到 4 只（含自己）—— 造不动了
        let mut s = State::new(80, 3);
        s.add_enemy(id, 150);
        for _ in 0..3 {
            s.add_enemy(def_id_of("电击机器人"), 23);
        }
        let s = begin_combat(s);
        assert_eq!(crate::step::allowed_next(&s, 0), 0b100, "满员：只剩分解");

        // 打死一只回到 3 只，又造得动
        let mut s = s;
        s.enemies[3].hp = 0;
        assert_eq!(crate::step::allowed_next(&s, 0), 0b011, "死一只就又造得动了");
    }

    /// 闪光贾克斯果：**只有一手，自己接自己**，每回合给自己力量 2，
    /// 所以它的攻击一路涨。
    #[test]
    fn jaxfruit_ramps_its_own_strength_every_turn() {
        let id = def_id_of("闪光贾克斯果");
        // **必须走真实敌人回合**：`end_turn_with_incoming` 是注入路径，
        // 只按给定数字扣血，不跑敌人自己的 `EOp` —— 力量永远不会涨。
        // 这一点我第一版写错了，测试当场红。
        let mut s = with_enemy(id, 31);
        let hp0 = s.player.hp;
        s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[0].get(St::Strength), 2, "第一手之后力量 2");
        assert_eq!(hp0 - s.player.hp, 3, "第一手打 3 点（力量是打完才加的）");
        s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[0].get(St::Strength), 4, "第二手之后力量 4");
        assert_eq!(hp0 - s.player.hp, 3 + 5, "第二手打 3+2=5 点");
    }

    /// 蛮兽**开局固定爪击** —— [源码] 的 initialState 是 `moveState3`。
    #[test]
    fn mawler_opens_with_claw() {
        let id = def_id_of("蛮兽");
        let s = with_enemy(id, 72);
        let def = crate::content::enemy_def(id);
        assert_eq!(def.moves[crate::step::current_move_ix(def, &s, 0)].name, "爪击");
        assert_eq!(def.max_hp, 72, "A0 是 72，76 是 ToughEnemies 那一档");
    }

    /// `UseOnlyOnce`：蛮兽**一整场只咆哮一次**。
    ///
    /// 这条是 [`Repeat::Once`] 的存在理由。用 `enemy_hist` 实现的话，
    /// 咆哮滑出那 4 手的窗口之后就会被重新放进允许集合 ——
    /// 所以这里要跑够 12 手，比历史深度长得多。
    #[test]
    fn mawler_roars_only_once_per_combat() {
        let id = def_id_of("蛮兽");
        let def = crate::content::enemy_def(id);
        let roar = def.moves.iter().position(|m| m.name == "咆哮").unwrap() as u32;
        let mut s = with_enemy(id, 999);
        let mut roars = 0;
        let mut seen_roar = false;
        for turn in 0..12 {
            if crate::step::current_move_ix(def, &s, 0) as u32 == roar {
                roars += 1;
                seen_roar = true;
            }
            if seen_roar {
                assert_eq!(
                    crate::step::allowed_next(&s, 0) & (1 << roar),
                    0,
                    "第 {turn} 手：咆哮出过之后不该再进允许集合"
                );
            }
            s = end_turn_with_incoming(s, &[(0, 0); crate::state::MAX_ENEMIES]);
        }
        assert!(roars <= 1, "一整场只该咆哮一次，实际 {roars} 次");
    }

    /// `Repeat::Once` 读的是「**曾经**」不是「最近几手」。
    ///
    /// 直接把历史清空、只留 `enemy_used` 那一位 —— 拿 `enemy_hist` 实现的
    /// 版本在这里会把咆哮放回集合里。**这就是那个实现的处刑帧。**
    #[test]
    fn use_only_once_reads_ever_not_recent_history() {
        let id = def_id_of("蛮兽");
        let def = crate::content::enemy_def(id);
        let roar = def.moves.iter().position(|m| m.name == "咆哮").unwrap() as u32;
        let mut s = with_enemy(id, 999);
        s.enemy_move[0] = 0; // 当前手：撕裂
        s.enemy_hist[0] = [u8::MAX; crate::state::ENEMY_HIST]; // 历史全空
        s.enemy_used[0] = 1 << roar; // 但这一场出过咆哮
        assert_eq!(
            crate::step::allowed_next(&s, 0) & (1 << roar),
            0,
            "历史窗口里没有，但这一场出过 —— 仍然不许再出"
        );
    }

    /// 条件分支：盛碗虫（石）**被完全挡住才失衡**，失衡了下一手才是晕眩。
    /// 这条同时是 `St::Imbalanced` 语义的回归测试 —— 它原来写着"语义完全未知"。
    #[test]
    fn bowlbug_goes_dizzy_only_after_a_fully_blocked_hit() {
        let id = def_id_of("盛碗虫（石）");

        // 没挡住：不失衡，下一手还是头槌
        let mut s = with_enemy(id, 45);
        s.player.block = 0;
        let s1 = end_turn_with_incoming(s, &[(15, 1), (0, 0), (0, 0), (0, 0), (0, 0)]);
        assert_eq!(s1.enemies[0].get(St::OffBalance), 0);
        let d1 = crate::content::enemy_def(id);
        assert_eq!(crate::step::current_move_ix(d1, &s1, 0), 0, "没失衡就接着头槌");

        // 挡满：失衡，下一手是晕眩
        s = with_enemy(id, 45);
        s.player.block = 99;
        let s2 = end_turn_with_incoming(s, &[(15, 1), (0, 0), (0, 0), (0, 0), (0, 0)]);
        assert_eq!(s2.enemies[0].get(St::OffBalance), 1, "完全挡住就失衡");
        let d2 = crate::content::enemy_def(id);
        assert_eq!(crate::step::current_move_ix(d2, &s2, 0), 1, "失衡之后下一手是晕眩");
    }

    /// 允许集合**永远非空**，而且只包含合法下标。
    /// 空集会让对拍那边无从判断，越界会 panic —— 两个都不许发生。
    #[test]
    fn the_allowed_set_is_never_empty_and_always_in_range() {
        for id in 0..ENEMIES.len() as u16 {
            let def = crate::content::enemy_def(id);
            if def.moves.is_empty() {
                continue;
            }
            let mut s = with_enemy(id, def.max_hp.max(1));
            for _ in 0..8 {
                let a = crate::step::allowed_next(&s, 0);
                assert!(a != 0, "{} 的允许集合空了", def.name);
                assert!(
                    a < (1u32 << def.moves.len()),
                    "{} 的允许集合里有越界下标 {a:#b}",
                    def.name
                );
                s = end_turn_with_incoming(s, &[(0, 0); crate::state::MAX_ENEMIES]);
            }
        }
    }

    // -----------------------------------------------------------------------
    // 战斗胜利结算（燃烧之血 / 带骨肉）
    // -----------------------------------------------------------------------

    /// 造一个"再打一下就赢"的局面：敌人 1 血，手里一张打击。
    fn about_to_win(hp: i32, relics: &[(St, i32)]) -> State {
        let mut s = hand_of(7, 1, &[card::STRIKE], 3);
        s.player.hp = hp;
        for (st, amt) in relics {
            s.player.set(*st, *amt);
        }
        s
    }

    #[test]
    fn burning_blood_heals_six_on_victory() {
        let s = about_to_win(50, &[(St::BurningBlood, 6)]);
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert!(s.combat_over && !s.player_dead);
        assert_eq!(s.player.hp, 56);
    }

    /// **本测试是这一整轮的核心。** 带骨肉的 50% 阈值拿的是**战斗结束那一刻**
    /// 的血量，**不含燃烧之血回的那 6 点**。
    ///
    /// 数字直接取自实录 `act2_f31`：80 血上限、结束时 36 血、终帧 54。
    /// 36 ≤ 40 → 带骨肉 +12 → 48，燃烧之血 +6 → 54。
    /// 若反过来先回 6：36+6=42 > 40，带骨肉不触发，只会到 42。
    /// **这个反例是真跑出来过的**：把 `check_over` 里两个 fire 调换之后，
    /// 对拍当场报 `游戏=54 内核=42`。所以这条不是读源码读出来的，是被实录判过的。
    #[test]
    fn meat_on_the_bone_threshold_ignores_burning_bloods_heal() {
        let s = about_to_win(36, &[(St::BurningBlood, 6), (St::MeatOnTheBone, 12)]);
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.player.hp, 54, "36 -> 带骨肉 +12 -> 48 -> 燃烧之血 +6 -> 54");
    }

    /// 阈值**向下取整**（[源码] `(int)(MaxHp * 50/100)`），且是"≤"不是"<"。
    #[test]
    fn meat_on_the_bone_threshold_is_inclusive_and_floored() {
        let just_at = about_to_win(40, &[(St::MeatOnTheBone, 12)]);
        assert_eq!(step(just_at, Action::PlayCard { hand: 0, target: 0 }).player.hp, 52);
        let just_over = about_to_win(41, &[(St::MeatOnTheBone, 12)]);
        assert_eq!(
            step(just_over, Action::PlayCard { hand: 0, target: 0 }).player.hp,
            41,
            "41/80 高于阈值 40，一点都不该回"
        );
    }

    /// 回血封顶在 max_hp。实录里有三场就是被封顶的（f6/f12 只 +4）。
    #[test]
    fn victory_heal_is_capped_at_max_hp() {
        let s = about_to_win(77, &[(St::BurningBlood, 6)]);
        assert_eq!(step(s, Action::PlayCard { hand: 0, target: 0 }).player.hp, 80);
    }

    /// **只结算一次。** `check_over` 会被反复调用（每次伤害落地、每个动作之后），
    /// 漏了那个 `just_won` 守卫就会每调一次回一次血。
    #[test]
    fn victory_relics_settle_exactly_once() {
        let s = about_to_win(50, &[(St::BurningBlood, 6)]);
        let won = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(won.player.hp, 56);
        // 战斗已经结束，再走几个动作不该再回血
        let again = step(won, Action::EndTurn);
        assert_eq!(again.player.hp, 56, "结束之后不许再结算一次");
    }

    /// 玩家死了不触发（[源码] 两处都是 `if (!Owner.Creature.IsDead)`）。
    #[test]
    fn victory_relics_do_not_fire_when_the_player_dies() {
        let mut s = about_to_win(50, &[(St::BurningBlood, 6)]);
        s.player.hp = 1;
        s.enemies[0].hp = 100; // 打不死它，改成让自己死在敌人这一手上
        let mut inc: crate::Incoming = [(0, 0); crate::state::MAX_ENEMIES];
        inc[0] = (99, 1);
        let s = end_turn_with_incoming(s, &inc);
        assert!(s.player_dead);
        assert_eq!(s.player.hp, 0, "死了就不该被燃烧之血救回来");
    }

    /// `private_status` 里的 status **不许**出现在 `replay::ALL_ST`。
    ///
    /// `ALL_ST` 是对拍逐字段比较的清单。私有 status 游戏根本不报，进了清单
    /// 就会**每帧**报一个「游戏 0 / 内核 1」的假 MISMATCH，把真错误淹掉。
    #[test]
    fn relic_private_status_is_never_compared_against_the_game() {
        for r in RELICS {
            for (st, _) in r.private_status {
                assert!(
                    !crate::replay::ALL_ST.contains(st),
                    "{}（{}）的私有 status {:?} 出现在 ALL_ST 里 —— 对拍会每帧报假 MISMATCH",
                    r.name,
                    r.id,
                    st
                );
            }
        }
    }

    /// 反过来：`start_status` 里的必须是**观测能映射到**的量。
    ///
    /// 这一栏的语义是"游戏也会报，所以对拍路径不用管它"。要是塞了一个
    /// 观测里根本没有的 status，那它既不会被同步、又不会被 carry，
    /// **等于什么都没做** —— 一个静默失效的遗物，比没建模更糟。
    #[test]
    fn relic_start_status_is_observable() {
        for r in RELICS {
            for (st, _) in r.start_status {
                assert!(
                    crate::replay::ALL_ST.contains(st),
                    "{}（{}）的 start_status {:?} 观测里没有 —— 它该放 private_status",
                    r.name,
                    r.id,
                    st
                );
            }
        }
    }

    /// 遗物挂上的 status 必须**有人认领**：`POWERS` 里有规则，或者在下面这张
    /// 白名单里（`step`/`damage` 里写死的窄消费点）。
    ///
    /// 和 `every_power_card_has_a_rule_and_no_rule_is_claimed_twice` 同一个道理：
    /// 挂了 status 却没人读，就是一件**看着建模了、实际没效果**的遗物。
    #[test]
    fn every_relic_status_is_claimed_by_someone() {
        // 写死消费点的（不走 POWERS）。加一条就要在这里登记，理由写清楚。
        const CONSUMED_IN_CODE: &[St] = &[
            // Strike tag damage, and the dynamic HP threshold strength hook.
            St::StrikeDummy,
            St::RedSkull,
            // 臂甲充能：`damage::card_block` 里消耗 —— 那是"从卡牌获得格挡"
            // 的唯一入口，所以药水/能力牌的格挡自动不吃翻倍
            St::VambraceCharge,
            // 摆动球的相位：被 `TCond::EveryNTurns` 读，不是被某条 `PowerDef`
            // 认领。它是**条件的输入**，不是触发器本身。
            St::PendulumPhase,
            // 钢笔尖的三个：在场标记和计数器在 `step::resolve_played_card`
            // 出牌结算**之前**那一段读（数第 10 张攻击），翻倍标记在
            // `damage::apply_modifiers` 里当乘区读。
            // 三个是一条链，缺任何一环都不会报错、只会静默失效。
            St::PenNib,
            St::PenNibCount,
            St::PenNibArmed,
            // 损毁头盔：`step::modify_status_amount_received` 读它
            // （本场第一次加力量 ×2）。和钢笔尖同一族 —— 改的是"正在算的那个数"，
            // 所以消费点在施加 status 那条路上，不是触发器。
            St::RuinedHelmet,
            // 天鹅绒颈圈：`step::play_cap_reached`（和懒惰同一处）。
            St::VelvetChoker,
            // 双截棍 / 铁棒的跨战斗计数器：和摆动球的相位一样是**条件的输入**
            // （`TCond::CounterMultipleOf`），加 1 的那一步在各自本体的规则里。
            St::NunchakuCount,
            St::IronClubCount,
        ];
        // `counter_to` 那一栏也要查 —— 它同样是"遗物往状态里塞了个东西"，
        // 塞了没人读一样是哑弹。摆动球的相位就在这一栏里。
        for r in RELICS {
            for st in r
                .start_status
                .iter()
                .chain(r.private_status.iter())
                .map(|(st, _)| st)
                .chain(r.counter_to.iter())
            {
                let claimed = POWERS.iter().any(|p| p.st == *st)
                    || CONSUMED_IN_CODE.contains(st)
                    || crate::replay::ALL_ST.contains(st);
                assert!(
                    claimed,
                    "{}（{}）挂了 {:?}，但没人读它 —— 哑弹",
                    r.name, r.id, st
                );
            }
        }
    }

    /// 药水槽位数是**观测量**，不是常数，也不该从遗物反推。
    ///
    /// 三件遗物会改它（药水腰带 +2 / 炼金宝匣 +4 / 药瓶皮套 +1），
    /// **而且高进阶把初始值从 3 改成 2** —— 进阶等级不在战斗观测里，
    /// 所以任何反推都会错。游戏在 `player.max_potion_slots` 里直接报。
    #[test]
    fn potion_slot_count_is_read_from_the_observation() {
        let t = crate::replay::parse_trace(&one_frame_with_potions(
            r#""max_potion_slots": 5,"#,
            r#"{"slot": 0, "id": "BLOCK_POTION"}, {"slot": 4, "id": "STRENGTH_POTION"}"#,
        ))
        .unwrap();
        let (_, sy, _) = crate::replay::sync_latest(&t).unwrap();
        assert_eq!(sy.state.potion_slots, 5, "槽位数该照观测走");
        assert_ne!(sy.state.potions[4], crate::state::potion::NONE, "第 5 格那瓶不能丢");
        assert!(sy.unmapped.is_empty(), "5 个槽位在容量之内，不该有告警");
    }

    /// 老 trace 没有这个字段时，退回"见过的最大槽位号 + 1"，**不会比真相小**。
    #[test]
    fn potion_slot_count_falls_back_without_underestimating() {
        let t = crate::replay::parse_trace(&one_frame_with_potions(
            "",
            r#"{"slot": 3, "id": "BLOCK_POTION"}"#,
        ))
        .unwrap();
        let (_, sy, _) = crate::replay::sync_latest(&t).unwrap();
        assert_eq!(sy.state.potion_slots, 4, "见过 slot 3 就至少有 4 个槽位");
    }

    /// 容量被顶破时**必须报出来**，不许静默丢。
    ///
    /// 静默丢的后果是求解器在一副更少药水的手上求最优，而输出里完全看不出来
    /// —— `MAX_POTIONS` 写死 3 的那一版就是这样，玩家带着药水腰带（5 格），
    /// 第 4、5 格的药水内核根本看不见。
    #[test]
    fn potion_beyond_capacity_is_reported_not_silently_dropped() {
        let over = crate::state::MAX_POTIONS + 1;
        let t = crate::replay::parse_trace(&one_frame_with_potions(
            "",
            &format!(r#"{{"slot": {over}, "id": "BLOCK_POTION"}}"#),
        ))
        .unwrap();
        let (_, sy, _) = crate::replay::sync_latest(&t).unwrap();
        assert!(
            sy.unmapped.iter().any(|u| u.contains("超出容量")),
            "顶破容量必须进 unmapped，实际：{:?}",
            sy.unmapped
        );
    }

    // -----------------------------------------------------------------------
    // 抽牌堆重建（rollout 的第二个地基）
    // -----------------------------------------------------------------------

    /// 有 `draw` 内容时，抽牌堆填的是**真牌**，不是占位牌。
    ///
    /// 占位牌 `playable() == false`，所以 rollout 里的"我"从下一个回合起
    /// 几乎什么都打不出来 —— **进攻被系统性低估、续航被高估**。
    /// 这正是 rollout 一直不可用的第二个原因。
    #[test]
    fn draw_pile_is_filled_with_real_cards_when_the_trace_has_them() {
        let t = crate::replay::parse_trace(&one_frame_with_draw(
            r#"{"name": "打击", "cost": "1"}, {"name": "防御+", "cost": "1"}, {"name": "踩踏", "cost": "3"}"#,
        ))
        .unwrap();
        let (_, sy, _) = crate::replay::sync_latest(&t).unwrap();
        assert_eq!(sy.state.n_draw, 3);
        let ids: Vec<u16> =
            (0..3).map(|i| sy.state.cards[sy.state.draw[i] as usize].id).collect();
        assert!(!ids.contains(&card::UNKNOWN), "不该还是占位牌：{ids:?}");
        // **升级态从名字后缀取**（牌堆里没有 is_upgraded，约束 3）
        let upg = (0..3)
            .filter(|&i| sy.state.cards[sy.state.draw[i] as usize].upgraded())
            .count();
        assert_eq!(upg, 1, "防御+ 必须是升级态，否则 rollout 整堆矮一截");
    }

    /// 没有 `draw` 的老 trace **照旧用占位牌**，而且不假装知道。
    #[test]
    fn old_traces_without_draw_still_use_placeholders() {
        let t = crate::replay::parse_trace(&one_frame_with_potions("", "")).unwrap();
        let (_, sy, _) = crate::replay::sync_latest(&t).unwrap();
        for i in 0..sy.state.n_draw as usize {
            assert_eq!(sy.state.cards[sy.state.draw[i] as usize].id, card::UNKNOWN);
        }
    }

    /// **顺序被重新洗过**，不照抄观测给的那个。
    ///
    /// 观测里的顺序是 mod 按稀有度+id 排的（约束 2），直接拿来用会让
    /// 「每次都先抽到同一张」变成系统性偏差 —— 那比不知道顺序更糟，
    /// 因为它是一个**看起来有信息的错顺序**。
    #[test]
    fn draw_order_is_reshuffled_not_taken_from_the_observation() {
        // 八张**互不相同**的牌，这样"顺序变没变"是可判定的。
        // 用同名牌做这个测试是不行的：那时只有一张牌的位置能观察，
        // 洗出恒等排列的概率是 1/n，测试会偶发地红。
        let names = ["打击", "防御", "踩踏", "拆卸", "御血术", "闪电霹雳", "剑柄打击", "预备打击"];
        let cards: Vec<String> =
            names.iter().map(|n| format!(r#"{{"name": "{n}", "cost": "1"}}"#)).collect();
        let t = crate::replay::parse_trace(&one_frame_with_draw(&cards.join(", "))).unwrap();
        let (_, sy, _) = crate::replay::sync_latest(&t).unwrap();
        assert_eq!(sy.state.n_draw as usize, names.len());

        let got: Vec<u16> = (0..names.len())
            .map(|i| sy.state.cards[sy.state.draw[i] as usize].id)
            .collect();
        let want_multiset = {
            let mut v: Vec<u16> =
                names.iter().map(|n| crate::replay::lookup_card(n).unwrap()).collect();
            v.sort();
            v
        };
        let mut got_sorted = got.clone();
        got_sorted.sort();
        assert_eq!(got_sorted, want_multiset, "洗牌不能改变内容");

        let as_observed: Vec<u16> =
            names.iter().map(|n| crate::replay::lookup_card(n).unwrap()).collect();
        assert_ne!(got, as_observed, "顺序没被洗过 —— 照抄观测顺序是系统性偏差");
    }

    fn one_frame_with_draw(draw: &str) -> String {
        format!(
            r#"{{"version": 1, "frames": [{{"obs": {{
                 "state_type": "monster", "round": 1, "is_play_phase": true,
                 "energy": 3, "max_energy": 3,
                 "player": {{"hp": 70, "max_hp": 80, "block": 0, "status": {{}}}},
                 "enemies": [], "hand": [], "draw_count": 0,
                 "draw": [{draw}],
                 "discard": [], "exhaust": [], "potions": [], "relics": []
               }}}}]}}"#
        )
    }

    /// 造一条最小的单帧 trace，只为了测药水那几条。
    fn one_frame_with_potions(slots_field: &str, potions: &str) -> String {
        format!(
            r#"{{"version": 1, "frames": [{{"obs": {{
                 "state_type": "combat", "round": 1, "is_play_phase": true,
                 "energy": 3, "max_energy": 3,
                 "player": {{"hp": 70, "max_hp": 80, "block": 0, "status": {{}}, {slots_field}
                   "potions_placeholder": 0}},
                 "enemies": [], "hand": [], "draw_count": 0,
                 "discard": [], "exhaust": [],
                 "potions": [{potions}], "relics": []
               }}}}]}}"#
        )
    }

    // -----------------------------------------------------------------------
    // 蜷身（敌人身上"会改变我这一手打多少"的状态，第一个）
    // -----------------------------------------------------------------------

    /// **伤害先落地，格挡后到。** 这是实测钉死的顺序
    /// （第2幕31层：`CURL_UP_POWER=14`，闪电霹雳+ 打了 7 点 134→127，
    /// 之后格挡才变 14）—— 反过来的话那 7 点会被吃掉，内核会高估敌人血量。
    #[test]
    fn curl_up_gives_block_after_the_hit_lands_not_before() {
        let mut s = hand_of(1, 100, &[card::STRIKE], 3);
        s.enemies[0].set(St::CurlUp, 14);
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.enemies[0].hp, 94, "打击那 6 点必须先落地");
        assert_eq!(s.enemies[0].block, 14, "然后才拿到 14 点格挡");
        assert_eq!(s.enemies[0].get(St::CurlUp), 0, "一次性：用完就没了");
    }

    /// 「**第一次**被命中」—— 第二下不再给格挡，只是被已有的格挡吃掉。
    #[test]
    fn curl_up_only_triggers_once() {
        let mut s = hand_of(2, 100, &[card::STRIKE, card::STRIKE], 3);
        s.enemies[0].set(St::CurlUp, 14);
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.enemies[0].hp, 94, "第二下 6 点被格挡吃掉，血不再掉");
        assert_eq!(s.enemies[0].block, 8, "14 - 6 = 8，而不是又加了 14");
    }

    /// **只有挨打的那一只触发。** 这条和 `Attacked` 正好相反（那个的持有者是
    /// 玩家、ctx 是攻击者），所以 `fire_ctx` 里要按钩子区分，不能一概而论。
    /// 写错的话打一个敌人会让全场敌人一起加格挡 —— 而且只在多怪场才看得出来。
    #[test]
    fn curl_up_fires_only_on_the_enemy_that_was_hit() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::DUMMY, 100);
        s.add_enemy(enemy::DUMMY, 100);
        let strike = s.add_card(card::STRIKE, 0, 0);
        for _ in 0..6 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = strike;
        s.energy = 3;
        s.enemies[0].set(St::CurlUp, 14);
        s.enemies[1].set(St::CurlUp, 14);

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.enemies[0].block, 14, "挨打的那只该拿格挡");
        assert_eq!(s.enemies[1].block, 0, "旁观的那只不该跟着拿");
        assert_eq!(s.enemies[1].get(St::CurlUp), 14, "旁观者的蜷身也不该被消耗");
    }

    // -----------------------------------------------------------------------
    // 臂甲：本场第一次**从卡牌**获得的格挡翻倍
    // -----------------------------------------------------------------------

    /// 只翻倍第一次，之后恢复原值。
    /// [实测] act2_f31 帧1：防御+（基础 8）给出 16。
    #[test]
    fn vambrace_doubles_only_the_first_card_block() {
        let mut s = hand_of(1, 100, &[card::DEFEND, card::DEFEND], 3);
        s.player.set(St::VambraceCharge, 1);
        let a = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(a.player.block, 10, "防御 5 翻倍 = 10");
        assert_eq!(a.player.get(St::VambraceCharge), 0, "充能用掉了");
        let b = step(a, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(b.player.block, 15, "第二张恢复原值：10 + 5");
    }

    /// 「从**卡牌**中获得的格挡」—— 药水和能力牌给的都不算。
    /// 这两条是**自动成立**的（药水走 `Source::Potion`、能力牌走
    /// `TOp::OwnerBlock`，都绕开 `card_block`），但正因为是自动的，
    /// 更需要测试钉住 —— 将来谁把入口改统一了，这里会立刻红。
    #[test]
    fn vambrace_ignores_potion_and_power_block() {
        let mut s = hand_of(2, 100, &[card::DEFEND], 3);
        s.player.set(St::VambraceCharge, 1);
        s.potions[0] = potion::BLOCK;

        let a = step(s, Action::UsePotion { slot: 0, target: 0 });
        assert_eq!(a.player.block, 12, "药水格挡不翻倍");
        assert_eq!(a.player.get(St::VambraceCharge), 1, "更不该把充能花掉");

        // 能力牌给的格挡（绯红披风回合开始 8 点）同样不吃翻倍
        let mut c = a;
        c.player.set(St::CrimsonMantle, 8);
        let c = step(c, Action::EndTurn);
        assert_eq!(c.player.get(St::VambraceCharge), 1, "能力牌的格挡也不该花掉充能");
    }

    /// 拳斗「获得等量于所造成伤害的格挡」也是**从卡牌获得**，一样吃翻倍。
    #[test]
    fn vambrace_applies_to_pugilism() {
        let mut s = hand_of(3, 100, &[card::PUGILISM], 3);
        s.player.set(St::VambraceCharge, 1);
        let a = step(s, Action::PlayCard { hand: 0, target: 0 });
        let dealt = 100 - a.enemies[0].hp;
        assert!(dealt > 0);
        assert_eq!(a.player.block, dealt * 2, "打出 {dealt} 点 => 翻倍成 {} 点格挡", dealt * 2);
    }

    #[test]
    fn test_use_block_potion_gives_12_block() {
        let mut s = State::new(80, 1);
        s.potions[0] = potion::BLOCK;
        let ns = step(s, Action::UsePotion { slot: 0, target: 0 });
        assert_eq!(ns.player.block, 12);
        assert_eq!(ns.potions[0], potion::NONE);
    }

    #[test]
    fn test_use_strength_potion_scales_attacks() {
        let mut s = hand_of(1, 80, &[card::STRIKE], 3);
        s.potions[0] = potion::STRENGTH;
        let ns1 = step(s, Action::UsePotion { slot: 0, target: 0 });
        assert_eq!(ns1.player.get(St::Strength), 2);
        let ns2 = step(ns1, Action::PlayCard { hand: 0, target: 0 });
        // 基础打击 6 点 + 2 力量 = 8 点伤害，80 - 8 = 72 HP
        assert_eq!(ns2.enemies[0].hp, 72);
    }

    #[test]
    fn test_use_fire_potion_deals_20_damage_and_kills() {
        let mut s = hand_of(2, 20, &[], 3);
        s.potions[0] = potion::FIRE;
        let ns = step(s, Action::UsePotion { slot: 0, target: 0 });
        assert_eq!(ns.enemies[0].hp, 0);
        assert!(ns.combat_over, "火焰药水打死唯一敌人应结束战斗");
    }

    #[test]
    fn test_use_energy_potion_adds_2_energy() {
        let mut s = State::new(80, 1);
        s.energy = 1;
        s.potions[0] = potion::ENERGY;
        let ns = step(s, Action::UsePotion { slot: 0, target: 0 });
        assert_eq!(ns.energy, 3);
        assert_eq!(ns.potions[0], potion::NONE);
    }

    #[test]
    fn test_solver_uses_potion_to_save_life() {
        let mut s = hand_of(3, 100, &[], 0); // 0 能量，无法出牌
        s.player.hp = 8;
        s.potions[0] = potion::BLOCK; // 12 点格挡药水
        let mut t = Threat::new();
        t.set(0, 15, 1); // 来袭 15 点伤害，不出药水必死 (8 HP 挨 15 伤害)
        let line = solve_turn(&s, &t, score::survive_first);
        assert_eq!(line.acts(), &[Action::UsePotion { slot: 0, target: 0 }]);
        let end = super::solver::replay_line(&s, line.acts()).unwrap();
        let after = end_turn_with_incoming(end, &t.incoming);
        assert!(!after.player_dead, "喝了格挡药水后挡下 12 点伤害，存活");
        assert_eq!(after.player.hp, 5); // 8 - (15 - 12) = 5
    }

    /// 手牌上限 10 张 —— [实测] `act2_f33_boss_crusher_retry` 帧13。
    ///
    /// 复现的是那一帧的形状：手牌 9 张，打出 0 费的战斗专注（离手后 8 张）
    /// 抽 3 张。游戏进来 2 张停在 10，**抽牌堆 6 -> 4**：第 3 张压根没被抽走。
    /// 这里同时钉住两件事 —— 上限是 10、以及**被拒绝的那次抽牌不消耗抽牌堆**。
    #[test]
    fn hand_caps_at_ten_and_refused_draws_stay_in_the_draw_pile() {
        let mut s = State::new(80, 1);
        // 没有活着的敌人时 `PlayCard` 直接返回原状态，出牌根本不会发生
        s.add_enemy(enemy::DUMMY, 100);
        // `State::new` 自带起手牌组，先清空（`add_card` 是往抽牌堆塞）
        s.n_cards = 0;
        s.n_draw = 0;
        // 9 张手牌，其中一张是战斗专注
        let bt = s.add_card(card::BATTLE_TRANCE, 0, 0);
        let mut hand = vec![bt];
        for _ in 0..8 {
            hand.push(s.add_card(card::STRIKE, 0, 0));
        }
        s.n_draw = 0;
        for c in hand {
            s.to_hand(c);
        }
        // 抽牌堆 6 张
        let draw: Vec<u8> = (0..6).map(|_| s.add_card(card::DEFEND, 0, 0)).collect();
        s.n_draw = 0;
        for c in draw {
            s.to_draw_top(c);
        }
        assert_eq!(s.n_hand, 9);
        assert_eq!(s.n_draw, 6);

        s.energy = 3;
        let ns = step(s, Action::PlayCard { hand: 0, target: 0 });

        assert_eq!(ns.n_hand, 10, "手牌卡在 10，不是 8+3=11");
        assert_eq!(ns.n_draw, 4, "只抽走 2 张；被拒绝的那一张**留在抽牌堆**");
        assert_eq!(ns.n_disc, 1, "战斗专注自己进弃牌堆");
    }

    /// 手牌已经满 10 张时抽牌是**彻底的空操作**：不动抽牌堆、不动弃牌堆、
    /// 也不触发洗牌。
    ///
    /// 后半条是 L2 会踩到的：如果满手时抽牌仍然推动洗牌流，
    /// "先打一张再抽"和"直接抽"会走出两条不同的随机序列，
    /// 换序合并就不成立了。
    #[test]
    fn drawing_with_a_full_hand_is_a_complete_no_op() {
        let mut s = State::new(80, 1);
        s.n_cards = 0;
        s.n_draw = 0;
        let hand: Vec<u8> = (0..10).map(|_| s.add_card(card::STRIKE, 0, 0)).collect();
        s.n_draw = 0;
        for c in hand {
            s.to_hand(c);
        }
        let draw: Vec<u8> = (0..5).map(|_| s.add_card(card::DEFEND, 0, 0)).collect();
        s.n_draw = 0;
        for c in draw {
            s.to_draw_top(c);
        }
        let before = s;
        s.draw_n(4);
        assert_eq!(s.n_hand, 10);
        assert_eq!(s.n_draw, 5, "抽牌堆一张没少");
        assert_eq!(s.rng.shuffle, before.rng.shuffle, "被拒绝的抽牌不许推动洗牌流");
    }

    #[test]
    fn test_pending_fetch_from_discard_and_upgrade() {
        let mut s = State::new(80, 1);
        let c0 = s.add_card(card::STRIKE, 0, 0);
        let c1 = s.add_card(card::DEFEND, 0, 0);
        s.n_draw = 0;
        s.to_discard(c0);
        s.to_hand(c1);
        assert_eq!(s.n_disc, 1);
        assert_eq!(s.n_hand, 1);

        // 测试从弃牌堆拿回卡牌
        s.pending = Pending::FetchFromDiscard { remaining: 1 };
        let ns = step(s, Action::Choose { hand: 0 });
        assert_eq!(ns.n_disc, 0);
        assert_eq!(ns.n_hand, 2);
        assert_eq!(ns.pending, Pending::None);

        // 测试手牌升级
        let mut s2 = ns;
        s2.pending = Pending::UpgradeInHand { remaining: 1 };
        let s3 = step(s2, Action::Choose { hand: 1 });
        assert!(s3.cards[s3.hand[1] as usize].upgraded());
        assert_eq!(s3.pending, Pending::None);
    }

    #[test]
    fn test_mcts_rollout_deterministic_with_seed() {
        let s = hand_of(10, 80, &[card::STRIKE, card::DEFEND], 3);
        let hp1 = rollout::rollout_combat(&s, 20, 12345);
        let hp2 = rollout::rollout_combat(&s, 20, 12345);
        assert_eq!(hp1, hp2, "相同种子下 MCTS 模拟结果必须严格确定");
    }

    #[test]
    fn test_mcts_prioritizes_demon_form_in_long_combat() {
        // 构建一个敌人血量厚 (80 HP)、本回合来袭 8 点伤害的局面
        let mut s = State::new(80, 42);
        s.add_enemy(enemy::EXOSKELETON, 80);
        let df = s.add_card(card::DEMON_FORM, 0, 0);
        for _ in 0..6 {
            s.add_card(card::STRIKE, 0, 0);
            s.add_card(card::DEFEND, 0, 0);
        }
        s = begin_combat(s);
        s.n_hand = 2;
        s.hand[0] = df; // 恶魔形态 (3 费)
        s.hand[1] = s.cards.iter().position(|c| c.id == card::DEFEND).unwrap() as u8;
        s.energy = 3;

        let mut t = Threat::new();
        t.set(0, 8, 1); // 本回合来袭 8 点伤害

        // 单回合目标函数（survive_first）因为恶魔形态当回合无格挡产出且占用全部3费，会选择打防御而不是恶魔形态
        let single_turn_line = solve_turn(&s, &t, score::survive_first);
        assert!(!explain(&s, single_turn_line.acts()).contains(&"恶魔形态".to_string()));

        // MCTS 全场期望战损目标函数（score::mcts_rollout）在长局模拟中识别出力量滚雪球价值，果断打出恶魔形态！
        let mcts_line = solve_turn(&s, &t, score::mcts_rollout);
        assert!(
            explain(&s, mcts_line.acts()).contains(&"恶魔形态".to_string()),
            "MCTS 应识别出恶魔形态在长局中的巨大终局收益: {:?}",
            explain(&s, mcts_line.acts())
        );
    }

    // =======================================================================
    // 第一批铁甲补牌（2026-08-19）。数值来源 [源码]，一张都没实战验过 ——
    // 下面测的是**行为**（触发条件、目标选择、结算顺序），不是数值本身。
    // 数值要等第一次实战打出来由对拍判决。
    // =======================================================================

    /// 势不可当「每当你获得格挡时，对随机敌人造成 N 点伤害」。
    ///
    /// **0 点格挡不触发** —— [源码] `AfterBlockGained` 第一句就是
    /// `if (!(amount <= 0m) && ...)`。这条最容易写漏：不判 0 的话，
    /// 一个给 0 格挡的场面会凭空多出伤害。
    #[test]
    fn juggernaut_fires_on_block_but_not_on_zero_block() {
        let mut s = hand_of(1, 100, &[card::DEFEND], 3);
        s.player.set(St::Juggernaut, 5);
        let s2 = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s2.enemies[0].hp, 95, "防御给 5 格挡 => 势不可当打 5");

        // **真正获得了 0 点格挡**的情况：拳斗「造成7点伤害，获得等量格挡」，
        // 力量 -7 把伤害压到 0，于是格挡也是 0 —— `gain_block` 被调到了，
        // 但拿到的是 0，不该触发。这一条才卡得住那个守卫；
        // 用一张纯攻击牌是卡不住的（那根本走不到 `gain_block`）。
        let mut s3 = hand_of(1, 100, &[card::PUGILISM], 3);
        s3.player.set(St::Juggernaut, 5);
        s3.player.set(St::Strength, -7);
        let s4 = step(s3, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s4.player.block, 0, "前提：这一手确实只拿到 0 点格挡");
        assert_eq!(s4.enemies[0].hp, 100, "获得 0 格挡不触发势不可当");
    }

    /// 势不可当**不吃力量** —— [源码] 走 `ValueProp.Unpowered`。
    /// 这一条不是"没证据就选保守"，是源码明写的，所以值得单独钉住。
    #[test]
    fn juggernaut_damage_ignores_strength() {
        let mut s = hand_of(1, 100, &[card::DEFEND], 3);
        s.player.set(St::Juggernaut, 5);
        s.player.set(St::Strength, 10);
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.enemies[0].hp, 95, "力量 10 不该加到势不可当的 5 点上");
    }

    /// **能力牌给的格挡一样触发势不可当。**
    ///
    /// 内核里有四条加格挡的路径，全部收口在 `step::gain_block`。
    /// 这条测的是「能力牌那条路径也走了收口」—— 漏掉任何一条，
    /// 触发器就静默失效，而那种 bug 只有实战带着这张牌打一场才看得出来。
    /// 和 `CardExhausted` 靠 `exhaust_card` 收口是同一个道理。
    #[test]
    fn juggernaut_also_fires_on_block_from_a_power() {
        let mut s = hand_of(1, 100, &[card::STRIKE], 3);
        s.player.set(St::Juggernaut, 5);
        s.player.set(St::PlatedArmor, 4); // 回合结束时给 4 格挡
        let s = step(s, Action::EndTurn);
        assert!(
            s.enemies[0].hp <= 95,
            "覆甲在回合结束给的格挡也该触发势不可当，实得 hp={}",
            s.enemies[0].hp
        );
    }

    /// 巨像「攻击你的**带易伤的**敌人伤害减半」。
    ///
    /// 条件挂在**攻击者**身上（[源码] `dealer.HasPower<VulnerablePower>()`），
    /// 不是挂在我身上。读反了会变成"我给敌人上易伤反而自己少挨打"，
    /// 那是个方向完全相反的错误。
    #[test]
    fn colossus_halves_damage_only_from_a_vulnerable_attacker() {
        let base = {
            let mut s = State::new(80, 1);
            s.add_enemy(enemy::DUMMY, 100);
            let s = begin_combat(s);
            apply_modifiers(20, &s.enemies[0], &s.player)
        };
        assert_eq!(base, 20, "基准：没有任何乘区");

        let mut me = Entity::new(80);
        me.set(St::Colossus, 1);
        let mut attacker = Entity::new(100);
        assert_eq!(
            apply_modifiers(20, &attacker, &me),
            20,
            "攻击者不带易伤时，巨像不生效"
        );
        attacker.set(St::Vulnerable, 2);
        assert_eq!(
            apply_modifiers(20, &attacker, &me),
            10,
            "攻击者带易伤 => 我受到的伤害减半"
        );
    }

    /// 狂宴「若此牌杀死了敌人，永久提升 3 点最大生命」。
    /// 没杀死就什么都不发生 —— 条件牌最容易写成无条件生效。
    #[test]
    fn feed_gains_max_hp_only_on_a_kill() {
        // 敌人 5 血，狂宴打 10 => 杀死
        let s = hand_of(1, 5, &[card::FEED], 3);
        let before = s.player.max_hp;
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert!(!s.enemies[0].alive(), "5 血该被 10 点打死");
        assert_eq!(s.player.max_hp, before + 3, "杀死了 => 最大生命 +3");
        assert_eq!(s.player.hp, 80 + 3, "同时回同样多的血（[源码] GainMaxHp）");

        // 敌人 100 血，打不死
        let s2 = hand_of(1, 100, &[card::FEED], 3);
        let before2 = s2.player.max_hp;
        let s2 = step(s2, Action::PlayCard { hand: 0, target: 0 });
        assert!(s2.enemies[0].alive());
        assert_eq!(s2.player.max_hp, before2, "没杀死 => 最大生命不变");
    }

    /// 跃跃欲试「手牌中每有一张攻击牌，获得 1 点能量」。
    /// 数的是**打出之后**的手牌（这张牌自己是技能牌且已经离手）。
    #[test]
    fn expect_a_fight_counts_attacks_left_in_hand() {
        let s = hand_of(
            1,
            100,
            &[card::EXPECT_A_FIGHT, card::STRIKE, card::STRIKE, card::DEFEND],
            3,
        );
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        // 3 费 - 2 费(牌费) + 2(两张打击) = 3
        assert_eq!(s.energy, 3, "手里剩两张攻击牌 => +2 能量");
    }

    /// 跃跃欲试的**第二句**：「你在本回合内不能再获得能量。」
    ///
    /// [源码] `NoEnergyGainPower.ModifyEnergyGain => 0m`，而卡面两条 op 的**顺序
    /// 就是规则** —— 先拿能量再上禁令，反过来写这张牌自己就拿不到能量了。
    ///
    /// 不建这一句，方向是**乐观**：求解器会以为可以先跃跃欲试拿 2 点、
    /// 再被遗忘的仪式拿 4 点，而游戏给 0。
    #[test]
    fn expect_a_fight_locks_out_every_later_energy_gain_this_turn() {
        let s = hand_of(
            1,
            100,
            &[card::EXPECT_A_FIGHT, card::STRIKE, card::STRIKE, card::BRIGHTEST_FLAME],
            3,
        );
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.energy, 3, "自己那一笔照给：3 - 2 + 2");
        assert!(s.player.get(St::NoEnergyGain) > 0, "禁令挂上了");
        // 至亮之焰「获得2点能量。抽2张牌。失去1点最大生命。」—— 能量那一笔被锁掉，
        // **别的效果照常**（禁令只改能量，不是"这张牌打不出"）。
        let before_hp = s.player.max_hp;
        let i = (0..s.n_hand as usize)
            .find(|&i| s.cards[s.hand[i] as usize].id == card::BRIGHTEST_FLAME)
            .expect("至亮之焰在手里") as u8;
        let s = step(s, Action::PlayCard { hand: i, target: 0 });
        assert_eq!(s.energy, 3, "0 费的牌，那 2 点能量一点都没进来");
        assert_eq!(s.player.max_hp, before_hp - 1, "牌的其余效果照常结算");
    }

    /// 钢笔尖：**每打出的第 10 张攻击牌造成双倍伤害**（[源码] `PenNib`）。
    ///
    /// 计数器跨战斗保留，所以这里直接把它摆到 8 —— 实战里 `sync` 从遗物面板灌。
    /// 数在**结算之前**，所以翻倍的是第 10 张自己。
    #[test]
    fn pen_nib_doubles_the_tenth_attack_and_only_that_one() {
        let mut s = hand_of(7, 200, &[card::STRIKE, card::STRIKE, card::STRIKE], 9);
        s.player.set(St::PenNib, 1);
        s.player.set(St::PenNibCount, 8);
        let hp0 = s.enemies[0].hp;

        // 第 9 张：照常
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        let plain = hp0 - s.enemies[0].hp;
        assert_eq!(s.player.get(St::PenNibCount), 9);
        assert_eq!(s.player.get(St::PenNibArmed), 0, "打完就摘，别留到下一张");

        // 第 10 张：×2，计数器归零
        let hp1 = s.enemies[0].hp;
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(hp1 - s.enemies[0].hp, plain * 2, "第 10 张翻倍");
        assert_eq!(s.player.get(St::PenNibCount), 0, "数到 10 归零");

        // 第 11 张：又是照常
        let hp2 = s.enemies[0].hp;
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(hp2 - s.enemies[0].hp, plain, "第 11 张不翻倍");
        assert_eq!(s.player.get(St::PenNibCount), 1);
    }

    /// 钢笔尖只数**攻击牌**，而且没这件遗物时一个计数器都不该动。
    #[test]
    fn pen_nib_counts_only_attacks_and_only_when_you_have_it() {
        let s = hand_of(7, 200, &[card::DEFEND, card::STRIKE], 9);
        let mut s2 = s;
        s2.player.set(St::PenNib, 1);
        s2.player.set(St::PenNibCount, 3);
        let s2 = step(s2, Action::PlayCard { hand: 0, target: 0 }); // 防御
        assert_eq!(s2.player.get(St::PenNibCount), 3, "技能牌不计数");
        let s2 = step(s2, Action::PlayCard { hand: 0, target: 0 }); // 打击
        assert_eq!(s2.player.get(St::PenNibCount), 4);
        // 没遗物：计数器一直是 0，也没人翻倍
        let s = step(s, Action::PlayCard { hand: 1, target: 0 });
        assert_eq!(s.player.get(St::PenNibCount), 0);
    }

    // -----------------------------------------------------------------------
    // 附魔（`content::ENCHANTS`）
    // -----------------------------------------------------------------------

    /// 附魔挂在**卡实例**上：同名的两张牌可以只有一张带。
    /// 灵巧加格挡、锋利加伤害，**两族互不串**（一张既打伤害又给格挡的牌
    /// 只带灵巧时不该凭空多伤害）。
    #[test]
    fn enchantments_are_per_instance_and_do_not_cross_hooks() {
        let nimble = crate::content::enchant_by_id("NIMBLE").unwrap().0;
        let sharp = crate::content::enchant_by_id("SHARP").unwrap().0;

        let mut s = hand_of(3, 200, &[card::DEFEND, card::DEFEND, card::STRIKE], 9);
        // 0 号防御带灵巧2，1 号不带
        let c0 = s.hand[0];
        s.cards[c0 as usize].ench = nimble;
        s.cards[c0 as usize].ench_amt = 2;
        // 打击带锋利3
        let c2 = s.hand[2];
        s.cards[c2 as usize].ench = sharp;
        s.cards[c2 as usize].ench_amt = 3;

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        let with_nimble = s.player.block;
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        let plain = s.player.block - with_nimble;
        assert_eq!(with_nimble - plain, 2, "灵巧 +2 只跟着那一张实例");

        let hp = s.enemies[0].hp;
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(hp - s.enemies[0].hp, 6 + 3, "打击 6 + 锋利 3");
    }

    /// 灵巧的加值加在**卡面基础格挡**上，然后才过敏捷/脆弱
    /// （[源码] `EnchantBlockAdditive(originalBlock)`，"runs BEFORE all other
    /// block modification hooks"）。
    ///
    /// **样本要挑分得开的**：基础 5 + 灵巧 2 带脆弱时两种顺序都给 5
    /// （⌊7×0.75⌋ = 5，⌊5×0.75⌋+2 = 5），什么都没验到。
    /// 防御+（基础 8）分得开：⌊10×0.75⌋ = **7**，而先脆弱再加是 6+2 = **8**。
    #[test]
    fn nimble_is_added_before_frail_not_after() {
        let nimble = crate::content::enchant_by_id("NIMBLE").unwrap().0;
        let mut s = hand_of(3, 200, &[card::DEFEND], 9);
        let c = s.hand[0];
        s.cards[c as usize].flags |= F_UPGRADED;
        s.cards[c as usize].ench = nimble;
        s.cards[c as usize].ench_amt = 2;
        s.player.set(St::Frail, 1);
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.player.block, 7, "(8 + 2) × 3/4 = 7.5 -> 7");
    }

    /// 「王室认证」（王室印章给的）：`OnEnchant` 加**固有 + 保留**。
    ///
    /// 保留 = 回合结束不弃掉这一张。[实测] 2026-09-06
    /// `act3_f46_elite_soul_nexus` 两个回合边界：带它的均衡+ 留在手上，
    /// 同一手的添柴+ 被弃掉。
    #[test]
    fn royally_approved_retains_its_card_across_the_turn_boundary() {
        let ra = crate::content::enchant_by_id("ROYALLY_APPROVED").unwrap().0;
        let mut s = hand_of(5, 200, &[card::STRIKE, card::DEFEND], 3);
        let keep = s.hand[0];
        s.cards[keep as usize].ench = ra;
        s.cards[keep as usize].ench_amt = 1;
        s.cards[keep as usize].flags |= F_RETAIN;
        // **停在抽牌之前**：抽牌堆是空的，`open_hand` 会把弃牌堆洗回来再抽，
        // 那样刚弃掉的那张又回到手上，这条测试就什么都没验到。
        let s = crate::step::end_turn_before_draw(s);
        assert!(
            (0..s.n_hand as usize).any(|i| s.hand[i] == keep),
            "带保留的那一张留在手上"
        );
        assert_eq!(s.n_hand, 1, "只留下它一张");
        assert_eq!(s.n_disc, 1, "同一手的另一张照常进弃牌堆");
    }

    /// 固有：开局洗完之后必定在起手。内核的抽牌堆顶在**末尾**，所以是挪到末尾。
    #[test]
    fn innate_cards_are_moved_to_the_top_of_the_draw_pile() {
        let mut s = State::new(80, 99);
        s.add_enemy(enemy::DUMMY, 100);
        for _ in 0..12 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let innate = s.add_card(card::DEFEND, F_INNATE, 0);
        let s = begin_combat(s);
        assert!(
            (0..s.n_hand as usize).any(|i| s.hand[i] == innate),
            "13 张牌里抽 5 张，固有那张必定在起手"
        );
    }

    /// 彼岸咆哮：**进了消耗堆之后**每个回合从那里再自己打一次
    /// （[源码] `AfterAutoPostPlayPhaseEntered` 判 `Pile.Type == Exhaust`）。
    ///
    /// **它自己不消耗** —— 打出去进弃牌堆。2026-09-09 之前这里写着"自带消耗"，
    /// 依据是权威卡表的关键字词表，而那一栏里的"消耗"来自描述里的
    /// 「消耗**牌堆**」四个字。[实测] `act1_f7_sewer_clam` 帧1→2 判死了这条。
    /// 所以这个测试得**先把它送进消耗堆**（这里用烙印那类做不到，
    /// 直接摆一个消耗堆状态）。
    ///
    /// 时点由 [源码] `CombatManager` 定死：在回合末钩子和弃手牌之前。
    #[test]
    fn howl_from_beyond_replays_itself_from_the_exhaust_pile_every_turn() {
        let s = hand_of(11, 300, &[card::HOWL_FROM_BEYOND], 3);
        let hp0 = s.enemies[0].hp;
        let mut s = step(s, Action::PlayCard { hand: 0, target: 0 });
        let once = hp0 - s.enemies[0].hp;
        assert_eq!(once, 16, "对所有敌人 16 点");
        assert_eq!(s.n_exh, 0, "**它自己不消耗**，打出去进弃牌堆");
        assert_eq!(s.n_disc, 1);
        // 手动把它挪进消耗堆（游戏里靠恶魔之焰/烙印那类），再验重放那一半
        let c = s.take_from_disc(0);
        s.exh[s.n_exh as usize] = c;
        s.n_exh += 1;
        let s = s;

        let hp1 = s.enemies[0].hp;
        let s = end_turn_with_incoming(s, &NO_INCOMING);
        assert_eq!(hp1 - s.enemies[0].hp, 16, "回合末从消耗堆里打了一次");

        // **打完就离开消耗堆** —— [源码] `CardModel.GetResultPileTypeForCardPlay()`
        // 只看这张牌自己带不带 `Exhaust` 关键字，**不管它是从哪个堆打出来的**，
        // 所以非消耗牌一律进弃牌堆。于是这是**一次性**的，不是永动机：
        // 要再来一次得把它重新消耗掉（恶魔之焰 / 烙印 / 痛殴）。
        assert_eq!(s.n_exh, 0, "打完进弃牌堆，不留在消耗堆");
        let hp2 = s.enemies[0].hp;
        let s = end_turn_with_incoming(s, &NO_INCOMING);
        assert_eq!(hp2 - s.enemies[0].hp, 0, "已经不在消耗堆里了，不再发作");
    }

    /// 附魔表的完整性：id 不重名、`modelled: false` 必须写清楚卡在哪。
    ///
    /// 后半条是这张表的全部价值 —— 一个没建全、又没说明的附魔，
    /// 和"建好了"长得一模一样。
    #[test]
    fn every_enchantment_is_either_modelled_or_says_what_is_missing() {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        for e in crate::content::ENCHANTS {
            assert!(seen.insert(e.id), "附魔 id 重名：{}", e.id);
            assert!(
                e.id.chars().all(|c| c.is_ascii_uppercase() || c == '_'),
                "附魔 id 要和游戏一致（大写下划线）：{}",
                e.id
            );
            if !e.modelled {
                assert!(!e.note.is_empty(), "{} 没建全，但没说卡在哪", e.id);
            }
        }
        // `CardInst::ench` 存的是下标+1，`u8` 装得下
        assert!(crate::content::ENCHANTS.len() < 255);
    }

    /// 佩尔之血：**每回合起手多抽 1 张，无条件**
    /// （[源码] `PaelsBlood.ModifyHandDraw => count + 1`）。
    ///
    /// 建成"回合开始多抽"而不是"把 5 改成 6"：`Hook::TurnStart` 在 `open_hand`
    /// 之前跑，先抽 1 再抽 5 和一次抽 6 从牌堆顶取到的是同一批牌。
    #[test]
    fn paels_blood_draws_one_extra_card_every_turn() {
        let mut s = State::new(80, 4);
        s.add_enemy(enemy::DUMMY, 100);
        for _ in 0..20 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let plain = begin_combat(s).n_hand;
        assert_eq!(plain, 5, "没这件遗物就是 5 张");

        s.player.set(St::PaelsBlood, 1);
        let s = begin_combat(s);
        assert_eq!(s.n_hand, 6, "第 1 回合就生效");
        let s = end_turn_with_incoming(s, &NO_INCOMING);
        assert_eq!(s.n_hand, 6, "**每个**回合都生效，不是只有第 1 回合");
    }

    /// 遗物面板上那个计数器有两种，**灌进内核的方式不同**：
    /// 回合相位要减掉这一场已经走过的回合数，别的计数器原样灌。
    ///
    /// 判据走 `content::is_turn_phase`（谁在 `EveryNTurns` 里当 `phase`）。
    /// 写死成名单的话，下一个带计数器的遗物进来时会被静默地按错的那一档处理。
    #[test]
    fn only_turn_phase_counters_get_the_round_offset() {
        // 摆动球：[源码] 每回合 `(x+1) % 3`，观测到的是加过 round 次之后的值
        assert!(crate::content::is_turn_phase(St::PendulumPhase));
        // 钢笔尖：数的是打出过几张攻击牌，和回合数无关
        assert!(!crate::content::is_turn_phase(St::PenNibCount));
    }

    /// **从战斗中途接进来时，只有"一场用一次"的私有量该当成已经用掉。**
    ///
    /// 实战驱动（`solve --live`）永远是中途调用，所以这条决定了那时内核
    /// 看不看得见身上的遗物。判据走 `content::spent_once_per_combat`
    /// —— 数据（规则里有没有 `ClearSelf`），不是名单。
    #[test]
    fn mid_fight_sync_keeps_constant_relic_markers_but_not_once_per_combat_charges() {
        use crate::content::spent_once_per_combat;
        // 臂甲的充能：`damage::card_block` 里花掉，没有 PowerDef 认领 ⇒ 写死在函数里
        assert!(spent_once_per_combat(St::VambraceCharge));
        // 百年积木：[源码] 的 `UsedThisCombat`，规则里就是 `TOp::ClearSelf`
        assert!(spent_once_per_combat(St::CentennialPuzzle));
        // "我身上有这件遗物"的常数标记：中途接进来照样该恢复
        assert!(!spent_once_per_combat(St::BurningBlood));
        assert!(!spent_once_per_combat(St::PenNib));
        assert!(!spent_once_per_combat(St::ParryingShield));
        // 回合闸门的那几件靠 `TCond` 拦，不靠"用掉"—— 恢复了也不会再发作
        assert!(!spent_once_per_combat(St::Anchor));
        assert!(!spent_once_per_combat(St::Lantern));
    }

    /// 钢笔尖那个 ×2 是**乘区**，和虚弱一起累乘、最后只取整一次。
    ///
    /// [实测] `act3_f46_elite_soul_nexus` 帧32：格挡 6 + 力量 1 = 7，带虚弱，
    /// 游戏打 10 = ⌊7 × 3/4 × 2⌋。
    #[test]
    fn pen_nib_multiplies_inside_the_chain_not_after_it() {
        let mut atk = Entity::new(80);
        atk.set(St::Weak, 1);
        let def = Entity::new(100);
        assert_eq!(apply_modifiers(7, &atk, &def), 5, "只有虚弱：⌊7×0.75⌋");
        atk.set(St::PenNibArmed, 1);
        assert_eq!(apply_modifiers(7, &atk, &def), 10, "⌊7×0.75×2⌋ = 10");
    }

    /// 飞剑回旋镖「造成 3 点伤害，随机 3 次」。
    ///
    /// 每一段**各自**随机挑目标（[源码] `TargetingRandomOpponents`），
    /// 所以多怪场里伤害会分散 —— 和 `DamageAll`（每个敌人都挨满）
    /// 完全不是一回事，写混了会让它在多怪场强出一倍。
    #[test]
    fn sword_boomerang_spreads_its_hits_at_random() {
        let mut s = State::new(80, 7);
        s.add_enemy(enemy::DUMMY, 100);
        s.add_enemy(enemy::DUMMY, 100);
        let c = s.add_card(card::SWORD_BOOMERANG, 0, 0);
        let mut s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = c;
        s.energy = 3;
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });

        let lost0 = 100 - s.enemies[0].hp;
        let lost1 = 100 - s.enemies[1].hp;
        assert_eq!(lost0 + lost1, 9, "总共 3 段 × 3 点，一点不多一点不少");
        assert!(
            lost0 != 9 || lost1 != 9,
            "不该是每个敌人都挨满 9 —— 那是 DamageAll 的语义"
        );
    }


    // =======================================================================
    // 第二批铁甲补牌（2026-08-19）。数值来自实时游戏卡表，语义来自 [源码]。
    // 每条测的都是「写反了也能编译」的那个点。
    // =======================================================================

    /// 战斗专注「抽3张牌。你在本回合内不能再抽任何牌。」
    ///
    /// **先抽再上 NoDraw。** 顺序写反这张牌就一张都抽不到，而且照样编译通过。
    #[test]
    fn battle_trance_draws_first_then_locks_drawing() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::DUMMY, 100);
        let bt = s.add_card(card::BATTLE_TRANCE, 0, 0);
        for _ in 0..8 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = bt;
        s.energy = 3;
        let before_draw = s.n_draw;

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.n_hand, 3, "该抽到 3 张");
        assert_eq!(s.n_draw, before_draw - 3, "抽牌堆少 3 张");
        assert!(s.player.get(St::NoDraw) > 0, "之后挂上 NoDraw");

        // 再抽就抽不动了
        let mut s2 = s;
        let h = s2.n_hand;
        s2.draw_n(3);
        assert_eq!(s2.n_hand, h, "NoDraw 之后一张都不该再抽到");
    }

    /// 重振精神「消耗手牌中所有非攻击牌，每张获得5点格挡。」
    ///
    /// 攻击牌**留在手上**。写成"消耗所有手牌"会多消耗一堆牌还多给格挡。
    #[test]
    fn second_wind_exhausts_only_non_attacks() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::DUMMY, 100);
        let sw = s.add_card(card::SECOND_WIND, 0, 0);
        let d1 = s.add_card(card::DEFEND, 0, 0);
        let d2 = s.add_card(card::DEFEND, 0, 0);
        let st1 = s.add_card(card::STRIKE, 0, 0);
        let mut s = begin_combat(s);
        s.n_hand = 4;
        s.hand[0] = sw;
        s.hand[1] = d1;
        s.hand[2] = d2;
        s.hand[3] = st1;
        s.n_draw = 0;
        s.n_disc = 0;
        s.energy = 3;

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.n_hand, 1, "只剩那张打击");
        assert_eq!(
            card(s.cards[s.hand[0] as usize].id).name,
            "打击",
            "留下的必须是攻击牌"
        );
        assert_eq!(s.player.block, 10, "两张防御被消耗 => 2 × 5 = 10 点格挡");
    }

    /// 愤怒「将一张**此牌的复制品**加入你的弃牌堆」。
    ///
    /// 复制的是**这一张实例**（[源码] `CreateClone()`），所以升级过的愤怒
    /// 复制出来也是升级的。写成"按牌名新建一张"就会退化成未升级版本。
    #[test]
    fn anger_clones_the_upgraded_instance_not_just_the_name() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::DUMMY, 100);
        let a = s.add_card(card::ANGER, F_UPGRADED, 0);
        let mut s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = a;
        s.n_draw = 0;
        s.n_disc = 0;
        s.energy = 3;

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.enemies[0].hp, 92, "升级版愤怒打 8 点");
        // 打出的那张进弃牌堆，复制品也进弃牌堆
        assert_eq!(s.n_disc, 2, "自己 + 复制品，弃牌堆两张");
        for i in 0..s.n_disc as usize {
            let inst = s.cards[s.disc[i] as usize];
            assert_eq!(card(inst.id).name, "愤怒");
            assert!(
                inst.flags & F_UPGRADED != 0,
                "复制品也该是升级态（复制的是实例不是牌名）"
            );
        }
    }

    /// 武装+「获得5点格挡。升级你手牌中的**所有**牌。」
    /// 格挡升级前后都是 5 —— 容易想当然写成"升级也涨格挡"。
    #[test]
    fn armaments_upgraded_upgrades_the_whole_hand_and_keeps_block_at_5() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::DUMMY, 100);
        let arm = s.add_card(card::ARMAMENTS, F_UPGRADED, 0);
        let a = s.add_card(card::STRIKE, 0, 0);
        let b = s.add_card(card::DEFEND, 0, 0);
        let mut s = begin_combat(s);
        s.n_hand = 3;
        s.hand[0] = arm;
        s.hand[1] = a;
        s.hand[2] = b;
        s.n_draw = 0;
        s.n_disc = 0;
        s.energy = 3;

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.player.block, 5, "武装+ 的格挡仍然是 5，不是 8");
        for i in 0..s.n_hand as usize {
            assert!(
                s.cards[s.hand[i] as usize].flags & F_UPGRADED != 0,
                "手上每一张都该被升级"
            );
        }
    }

    /// 头槌「将你弃牌堆中的一张牌放到**抽牌堆顶部**」。
    ///
    /// 顶部 = `draw` 数组末尾（`draw_one` 从 `n_draw - 1` 取）。
    /// 放到另一头就变成"沉到牌堆底"，效果完全相反且照样编译。
    ///
    /// **弃牌堆放两张**：一张的时候游戏根本不弹界面（见
    /// `headbutt_with_a_single_candidate_resolves_without_a_choice`），
    /// 那条路径测不到这里要守的"选哪张 -> 放哪头"。
    #[test]
    fn headbutt_puts_the_chosen_discard_on_top_of_the_draw_pile() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::DUMMY, 100);
        let hb = s.add_card(card::HEADBUTT, 0, 0);
        let filler = s.add_card(card::DEFEND, 0, 0);
        let want = s.add_card(card::BASH, 0, 0);
        let other = s.add_card(card::DEFEND, 0, 0);
        let mut s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = hb;
        s.n_draw = 1;
        s.draw[0] = filler;
        s.n_disc = 2;
        s.disc[0] = want;
        s.disc[1] = other;
        s.energy = 3;

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.enemies[0].hp, 91, "头槌打 9 点");
        assert!(
            matches!(s.pending, Pending::DiscardToDrawTop { .. }),
            "两张候选该进入'从弃牌堆选一张'的子选择"
        );
        let s = step(s, Action::Choose { hand: 0 });
        assert_eq!(s.pending, Pending::None);
        assert_eq!(s.n_draw, 2);

        // 下一张抽到的必须是它
        let mut s2 = s;
        s2.n_hand = 0;
        s2.draw_n(1);
        assert_eq!(
            card(s2.cards[s2.hand[0] as usize].id).name,
            "痛击",
            "放到顶部就该是下一张抽到的"
        );
    }

    /// **头槌捞不到它自己。**
    ///
    /// 效果结算（设 `Pending`）在牌进弃牌堆**之前**，玩家做选择在**之后** ——
    /// 不排除的话头槌会把自己放回抽牌堆顶、下次抽到再打一次。
    /// 2026-08-30 实战时 `solve --live` 真的建议过这条线（头槌+ 打两次），
    /// 而游戏根本不接受：那一帧我先打凌虐+ 再打头槌+，
    /// 候选里**有凌虐+、没有头槌+** —— 排除的是"它自己"，
    /// 不是"本回合打过的牌"。
    ///
    /// **四种对拍模式全看不见这条**（`verify` 跳过选牌帧，`--per-turn` 也不判），
    /// 所以它只能靠单测守。
    #[test]
    fn headbutt_cannot_fetch_itself() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::DUMMY, 100);
        let hb = s.add_card(card::HEADBUTT, 0, 0);
        let a = s.add_card(card::BASH, 0, 0);
        let b = s.add_card(card::STRIKE, 0, 0);
        let mut s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = hb;
        s.n_draw = 0;
        s.n_disc = 2;
        s.disc[0] = a;
        s.disc[1] = b;
        s.energy = 3;

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        // 头槌自己这时已经躺在弃牌堆里了 —— 正是它会被误选的原因
        assert_eq!(s.n_disc, 3, "头槌打完进了弃牌堆");
        assert!(s.disc[..3].contains(&hb));

        let (acts, n) = legal_actions(&s);
        assert_eq!(n, 2, "三张里只有两张是候选");
        for a in &acts[..n] {
            let Action::Choose { hand: i } = *a else { panic!("这一层只该有 Choose") };
            assert_ne!(s.disc[i as usize], hb, "头槌不该出现在自己的候选里");
        }
    }

    /// 只有一张候选时游戏**不弹选牌界面**，直接放上去。
    ///
    /// [实测 1 帧] 2026-08-30 第 3 幕第 45 层回合 1：弃牌堆只有一张防御，
    /// 打出头槌+ 之后没有 `card_select`，抽牌堆 24 -> 25、弃牌堆仍是 1。
    ///
    /// **结果等价**（一个候选的选择本来就是强制的），建它是为了让
    /// `solve --live` 不再给出一个游戏里根本不存在的选牌步骤。
    #[test]
    fn headbutt_with_a_single_candidate_resolves_without_a_choice() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::DUMMY, 100);
        let hb = s.add_card(card::HEADBUTT, 0, 0);
        let only = s.add_card(card::BASH, 0, 0);
        let mut s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = hb;
        s.n_draw = 0;
        s.n_disc = 1;
        s.disc[0] = only;
        s.energy = 3;

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.pending, Pending::None, "一张候选不该留下子选择");
        assert_eq!(s.n_draw, 1, "那一张直接上了抽牌堆");
        assert_eq!(s.draw[0], only);
        assert_eq!(s.n_disc, 1, "弃牌堆里换成了头槌自己");
        assert_eq!(s.disc[0], hb);
    }

    /// 燃烧契约「消耗1张牌。抽2张牌。」张数对得上就行 ——
    /// "先抽后选"那条已知偏差写在 `content.rs` 的注释和「故意没做」清单里。
    #[test]
    fn burning_pact_exhausts_one_and_draws_two() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::DUMMY, 100);
        let bp = s.add_card(card::BURNING_PACT, 0, 0);
        let victim = s.add_card(card::DEFEND, 0, 0);
        for _ in 0..5 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        s.n_hand = 2;
        s.hand[0] = bp;
        s.hand[1] = victim;
        s.energy = 3;
        let before_draw = s.n_draw;

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.n_draw, before_draw - 2, "抽了 2 张");
        assert!(matches!(s.pending, Pending::ExhaustFromHand { .. }));
        let s = step(s, Action::Choose { hand: 0 });
        assert_eq!(s.n_exh, 1, "消耗掉 1 张");
    }


    // =======================================================================
    // 牌生成（添柴 / 地狱之刃）。
    //
    // **这里刻意不对随机序列做任何断言。** 游戏用的是
    // `System.Random(seed + hash(流名))` 加一个存进存档的 `Counter`，
    // 而 `System.Random` 的算法在 .NET 版本间换过实现 —— 复刻不了，
    // 也不需要复刻（对拍路径每帧用观测覆盖手牌）。
    // 所以测的是**行为和池子**，不是"第几张会生成什么"。
    // =======================================================================

    /// 添柴「消耗所有手牌。每消耗一张牌，将1张随机牌加入你的手牌。」
    ///
    /// **先消耗、再生成**（玩家判定）。写成边消耗边生成会无限循环 ——
    /// 生成出来的牌又被同一张添柴吃掉，再生成…
    #[test]
    fn stoke_exhausts_the_whole_hand_then_generates_the_same_count() {
        let mut s = State::new(80, 11);
        s.add_enemy(enemy::DUMMY, 100);
        let stoke = s.add_card(card::STOKE, 0, 0);
        let a = s.add_card(card::STRIKE, 0, 0);
        let b = s.add_card(card::DEFEND, 0, 0);
        let c = s.add_card(card::DEFEND, 0, 0);
        let mut s = begin_combat(s);
        s.n_hand = 4;
        s.hand[0] = stoke;
        s.hand[1] = a;
        s.hand[2] = b;
        s.hand[3] = c;
        s.n_draw = 0;
        s.n_disc = 0;
        s.energy = 3;

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        // 打出添柴之后手上剩 3 张 -> 消耗 3 张 -> 生成 3 张
        assert_eq!(s.n_hand, 3, "消耗几张就生成几张");
        // **添柴自己不消耗。** 这一行原来断言的是 4（"添柴自己也消耗"），
        // 是错的：[源码] `Stoke` 没有 `Exhaust` 关键字，`OnPlay` 消耗的是
        // **手牌堆**，而这时添柴已经离开手牌，所以烧不到自己。
        // 2026-08-22 第2幕第20层第一次真打出添柴+，对拍当场报出
        // 「消耗堆 游戏=5 内核=6 / 弃牌堆 游戏含添柴+ 内核没有」。
        assert_eq!(s.n_exh, 3, "只消耗手上那 3 张，添柴自己不消耗");
        assert_eq!(s.n_disc, 1, "添柴自己进弃牌堆");

        // 生成出来的必须都在生成池里
        for i in 0..s.n_hand as usize {
            let id = s.cards[s.hand[i] as usize].id;
            assert!(
                GEN_POOL.contains(&id),
                "生成的牌必须来自生成池，实得 {}",
                card(id).name
            );
        }
    }

    /// 生成池**不含 Basic 牌，也不含狂宴**。
    /// [源码] `FilterForCombat`：`CanBeGeneratedInCombat && Rarity != Basic/Ancient/Event`。
    /// 狂宴自己 `CanBeGeneratedInCombat => false` —— 否则可以无限刷最大生命。
    #[test]
    fn generation_pool_excludes_basics_and_feed() {
        for &bad in &[card::STRIKE, card::DEFEND, card::BASH, card::FEED] {
            assert!(
                !GEN_POOL.contains(&bad),
                "{} 不该在生成池里",
                card(bad).name
            );
        }
        assert!(GEN_POOL.len() > 30, "池子不该是空的");
        // 不许有重复（[源码] 那一步是 `.Distinct()`）
        let mut seen = GEN_POOL.to_vec();
        seen.sort_unstable();
        let n = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), n, "生成池里有重复项");
    }

    /// 升级版添柴生成的是**已升级**的牌。
    #[test]
    fn stoke_upgraded_generates_upgraded_cards() {
        let mut s = State::new(80, 5);
        s.add_enemy(enemy::DUMMY, 100);
        let stoke = s.add_card(card::STOKE, F_UPGRADED, 0);
        let a = s.add_card(card::STRIKE, 0, 0);
        let mut s = begin_combat(s);
        s.n_hand = 2;
        s.hand[0] = stoke;
        s.hand[1] = a;
        s.n_draw = 0;
        s.n_disc = 0;
        s.energy = 3;

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.n_hand, 1);
        assert!(
            s.cards[s.hand[0] as usize].flags & F_UPGRADED != 0,
            "升级版添柴生成的牌该是升级态"
        );
    }

    /// **换个种子结果会变** —— 证明它真的在随机，而不是每次都吐同一张。
    /// 这条只断言"有随机性"，不断言具体是哪一张。
    #[test]
    fn stoke_actually_varies_with_the_seed() {
        let names = |seed: u64| {
            let mut s = State::new(80, seed);
            s.add_enemy(enemy::DUMMY, 100);
            let stoke = s.add_card(card::STOKE, 0, 0);
            for _ in 0..4 {
                s.add_card(card::STRIKE, 0, 0);
            }
            let mut s = begin_combat(s);
            s.n_hand = 5;
            s.hand[0] = stoke;
            for i in 1..5 {
                s.hand[i] = (i as u8) + 0; // 上面 add_card 的顺序即索引
            }
            s.n_draw = 0;
            s.n_disc = 0;
            s.energy = 3;
            let s = step(s, Action::PlayCard { hand: 0, target: 0 });
            (0..s.n_hand as usize)
                .map(|i| card(s.cards[s.hand[i] as usize].id).name)
                .collect::<Vec<_>>()
        };
        let mut differ = false;
        for seed in 1..12u64 {
            if names(seed) != names(seed + 100) {
                differ = true;
                break;
            }
        }
        assert!(differ, "换种子该换结果 —— 否则根本没在随机");
    }

    /// 地狱之刃：生成的必须是**攻击牌**，而且**本回合免费**。
    #[test]
    fn infernal_blade_generates_a_free_attack() {
        let mut s = State::new(80, 9);
        s.add_enemy(enemy::DUMMY, 100);
        let ib = s.add_card(card::INFERNAL_BLADE, 0, 0);
        let mut s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = ib;
        s.n_draw = 0;
        s.n_disc = 0;
        s.energy = 3;

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.n_hand, 1, "生成 1 张进手牌");
        let inst = s.cards[s.hand[0] as usize];
        assert!(
            matches!(card(inst.id).kind, crate::ops::Kind::Attack),
            "必须是攻击牌，实得 {}",
            card(inst.id).name
        );
        assert!(
            inst.flags & F_FREE_THIS_TURN != 0,
            "那张牌本回合应免费"
        );
        assert_eq!(effective_cost(&s, 0), 0, "免费 = 实际算出来 0 费");
    }


    // =======================================================================
    // 「牌打牌」那一批。玩家判定：auto_play **不扣能量、但计入**。
    // =======================================================================

    /// 破灭「打出抽牌堆顶部的牌并将其消耗。」
    ///
    /// 三件事一起测：真的打出了（伤害落地）、**消耗**而不是进弃牌堆、
    /// 以及**不扣能量**（自动打出的那张不花钱）。
    #[test]
    fn havoc_plays_the_top_of_draw_and_exhausts_it_without_paying() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::DUMMY, 100);
        let hv = s.add_card(card::HAVOC, 0, 0);
        let top = s.add_card(card::STRIKE, 0, 0);
        let mut s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = hv;
        s.n_draw = 1;
        s.draw[0] = top;
        s.n_disc = 0;
        s.energy = 3;

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.enemies[0].hp, 94, "抽牌堆顶那张打击该真的打出来");
        assert_eq!(s.energy, 2, "只扣破灭自己的 1 费，自动打出的那张不扣能量");
        assert_eq!(s.n_exh, 1, "被打出的那张该进消耗堆");
        assert_eq!(
            card(s.cards[s.exh[0] as usize].id).name,
            "打击",
            "进消耗堆的是被翻出来那张"
        );
        assert_eq!(s.n_draw, 0, "抽牌堆少一张");
    }

    /// **auto_play 计入** —— 玩家判定。踩踏的减费读 `attacks_played`，
    /// 所以"自动打出的攻击牌算不算数"是看得见的。
    #[test]
    fn auto_played_attacks_count_toward_attacks_played() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::DUMMY, 100);
        let hv = s.add_card(card::HAVOC, 0, 0);
        let top = s.add_card(card::STRIKE, 0, 0);
        let mut s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = hv;
        s.n_draw = 1;
        s.draw[0] = top;
        s.n_disc = 0;
        s.energy = 3;

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.attacks_played, 1, "自动打出的攻击牌要计入");
        assert_eq!(s.cards_played, 2, "破灭自己 + 被打出的那张");
    }

    /// 连环拳「你打出的下 1 张攻击牌会被额外打出一次」。
    ///
    /// **每张攻击牌消耗一层**，不是整个回合都翻倍 —— 第二张打击只打一次。
    /// 而且技能牌不消耗层数。
    #[test]
    fn one_two_punch_doubles_only_the_next_attack() {
        let mut s = hand_of(1, 200, &[card::STRIKE, card::DEFEND, card::STRIKE], 9);
        s.player.set(St::OneTwoPunch, 1);

        // 先打一张技能牌：不该消耗层数
        let s = step(s, Action::PlayCard { hand: 1, target: 0 });
        assert_eq!(s.player.get(St::OneTwoPunch), 1, "技能牌不消耗层数");

        // 第一张打击：打两次 = 12
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.enemies[0].hp, 188, "6 × 2 = 12");
        assert_eq!(s.player.get(St::OneTwoPunch), 0, "消耗掉一层");

        // 第二张打击：只打一次 = 6
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.enemies[0].hp, 182, "层数用完了，只打 6");
    }

    /// 杂耍「将你在每回合打出的**第三张**攻击牌的复制品加入你的手牌」。
    ///
    /// [源码] 是 `== 3` 的相等判断，不是每 3 张 —— 所以第 4、5、6 张都不触发。
    #[test]
    fn juggling_clones_exactly_the_third_attack_of_the_turn() {
        // **打满 6 张**才分得开 `== 3` 和 `% 3`：只打 4 张的话两种写法
        // 结果完全相同（1,2,3,4 里只有 3 命中，两边都一样）。第 6 张才是
        // 那个能把假设分开的样本 —— 仓库自己那条「和所有已知数据一致不等于对」。
        let mut s = hand_of(
            1,
            500,
            &[
                card::STRIKE,
                card::STRIKE,
                card::STRIKE,
                card::STRIKE,
                card::STRIKE,
                card::STRIKE,
            ],
            20,
        );
        s.player.set(St::Juggling, 1);

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.n_hand, 5, "第 1 张：不复制");
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.n_hand, 4, "第 2 张：不复制");
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.n_hand, 4, "第 3 张：复制 1 份（剩 3 张 + 复制品）");
        assert_eq!(s.attacks_played, 3);
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.n_hand, 3, "第 4 张：不复制");
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.n_hand, 2, "第 5 张：不复制");
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.attacks_played, 6);
        assert_eq!(
            s.n_hand, 1,
            "第 6 张：**不复制** —— 是 `== 3` 不是 `% 3`。             写成 % 3 的话这里会多一张复制品"
        );
    }

    /// 惊逃「在你的回合结束时，随机打出你手牌中的 1 张攻击牌攻击随机敌人」。
    /// 打出的伤害在**敌人行动之前**落地（钩子挂在 `TurnEnd`）。
    #[test]
    fn stampede_auto_plays_an_attack_at_end_of_turn() {
        let mut s = hand_of(1, 100, &[card::STRIKE, card::DEFEND], 3);
        s.player.set(St::Stampede, 1);
        let s = step(s, Action::EndTurn);
        assert!(
            s.enemies[0].hp <= 94,
            "回合结束该自动打出那张打击，实得 hp={}",
            s.enemies[0].hp
        );
    }

    /// 惊逃**只挑攻击牌**。手上全是技能牌时什么都不该发生。
    #[test]
    fn stampede_picks_only_attacks() {
        let mut s = hand_of(1, 100, &[card::DEFEND, card::DEFEND], 3);
        s.player.set(St::Stampede, 1);
        let s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[0].hp, 100, "手上没有攻击牌就不该打出任何东西");
    }

    /// 好勇斗狠「回合开始，将你弃牌堆的一张随机攻击牌放入你的手牌并将其升级」。
    #[test]
    fn aggression_fetches_and_upgrades_an_attack_from_discard() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::DUMMY, 100);
        let d1 = s.add_card(card::STRIKE, 0, 0);
        let d2 = s.add_card(card::DEFEND, 0, 0);
        for _ in 0..5 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.player.set(St::Aggression, 1);
        s.n_hand = 0;
        s.n_disc = 2;
        s.disc[0] = d1;
        s.disc[1] = d2;

        // 走公开路径进入下一个我方回合（钩子挂在 TurnStart）
        let s = step(s, Action::EndTurn);
        // 弃牌堆里只有一张攻击牌，必须是它被捞上来
        let mut found = false;
        for i in 0..s.n_hand as usize {
            let inst = s.cards[s.hand[i] as usize];
            if inst.id == card::STRIKE && inst.flags & F_UPGRADED != 0 {
                found = true;
            }
        }
        assert!(found, "该从弃牌堆捞出那张打击并升级它");
    }


    // =======================================================================
    // 最后 7 张。X 费两张的 X = **打出时花光的能量**
    // （[源码] `CapturedXValue = 当前能量`），不是玩家另给的参数。
    // =======================================================================

    /// 旋风斩「对所有敌人造成 5 点伤害 **X** 次」。
    /// X = 打出瞬间的能量，且**全部花光**。
    #[test]
    fn whirlwind_hits_all_enemies_x_times_and_drains_energy() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::DUMMY, 100);
        s.add_enemy(enemy::DUMMY, 100);
        let w = s.add_card(card::WHIRLWIND, 0, 0);
        let mut s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = w;
        s.n_draw = 0;
        s.n_disc = 0;
        s.energy = 3;

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.energy, 0, "X 费花光所有能量");
        assert_eq!(s.enemies[0].hp, 85, "5 × 3 次");
        assert_eq!(s.enemies[1].hp, 85, "对所有敌人");
    }

    // ---- 活力 / 赤牛 / 石化蟾蜍（2026-08-22 进表，全部 [源码]） ----

    /// **这条是活力最容易写错的一半，也是唯一分得开两种读法的样本。**
    ///
    /// [源码] `AttackCommand.Execute`：`Hook.BeforeAttack` 在多段 `for` 循环
    /// **之前**（536 行）、`Hook.AfterAttack` 在**之后**（656 行），而
    /// `VigorPower.ModifyDamageAdditive` 是每次命中各调一次的。
    /// 所以整条命令的每一段都吃满，打完才一次性清零。
    ///
    /// 双重打击 5×2 带 8 活力：
    /// ```text
    /// 每段都吃   (5+8)×2 = 26   ✔
    /// 只吃一次   5×2 + 8 = 18   ✘
    /// ```
    /// 单段牌上两种读法**完全相同**（都是 base+8），所以只有多段牌能判 ——
    /// 和缩小那个 ×0.7 / ×2/3 是同一个形状的坑。
    #[test]
    fn vigor_applies_to_every_hit_of_a_multi_hit_card_then_is_spent_once() {
        let mut s = hand_of(1, 100, &[card::TWIN_STRIKE, card::TWIN_STRIKE], 3);
        s.player.set(St::Vigor, 8);

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(100 - s.enemies[0].hp, 26, "(5+8)×2，不是 5×2+8");
        assert_eq!(s.player.get(St::Vigor), 0, "整条攻击命令打完，活力一次性清零");

        // 第二张双重打击已经没有活力可吃了
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(100 - s.enemies[0].hp, 26 + 10, "第二张只有 5×2");
    }

    /// 活力和力量是同一档加法项，两者叠加后**再**过乘区（不是各自过）。
    /// 力量 2 + 活力 8 打一张 5×2 的双重打击给带易伤的目标：
    /// `floor((5+2+8) × 1.5) = 22`，两段共 44。
    #[test]
    fn vigor_stacks_additively_with_strength_before_the_multipliers() {
        let mut s = hand_of(1, 200, &[card::TWIN_STRIKE], 3);
        s.player.set(St::Vigor, 8);
        s.player.set(St::Strength, 2);
        s.enemies[0].set(St::Vulnerable, 2);

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(200 - s.enemies[0].hp, 44, "floor((5+2+8)×1.5) = 22，两段");
    }

    /// **活力只吃有源攻击。** [源码] `VigorPower.ModifyDamageAdditive` 第一句
    /// 就是 `if (!props.IsPoweredAttack()) return 0m;` —— 和那五个乘区同一条 gate。
    ///
    /// 药水形状的石头是 `ValueProp.Unpowered`，所以带着 8 活力喝它仍然只有 15，
    /// **而且活力不会被它消耗掉**（那条命令压根没被 `BeforeAttack` latch 住）。
    #[test]
    fn vigor_does_not_touch_potion_damage_and_is_not_spent_by_it() {
        let mut s = hand_of(1, 100, &[], 3);
        s.player.set(St::Vigor, 8);
        s.potions[0] = potion::POTION_SHAPED_ROCK;

        let s = step(s, Action::UsePotion { slot: 0, target: 0 });
        assert_eq!(100 - s.enemies[0].hp, 15, "药水是 Unpowered，不吃活力");
        assert_eq!(s.player.get(St::Vigor), 8, "也不该被药水消耗掉");
    }

    /// 药水形状的石头连易伤都不吃（`Unpowered` 走
    /// `damage::apply_modifiers_unpowered`），和火焰药水那一帧同一条规则。
    #[test]
    fn potion_shaped_rock_is_unpowered_so_vulnerable_does_not_amplify_it() {
        let mut s = hand_of(1, 100, &[], 3);
        s.enemies[0].set(St::Vulnerable, 3);
        s.potions[0] = potion::POTION_SHAPED_ROCK;

        let s = step(s, Action::UsePotion { slot: 0, target: 0 });
        assert_eq!(100 - s.enemies[0].hp, 15, "不是 floor(15×1.5)=22");
    }

    /// 赤牛：[源码] `Akabeko.AfterSideTurnStart`，条件 `TurnNumber <= 1`。
    /// **只在第一回合给一次** —— 和灯笼同一个 `<= 1`。
    ///
    /// 判据必须是"第二回合没有**再加** 8"，不能写成"第二回合活力是 0" ——
    /// 那条第一次就红了，而红的是断言不是内核：活力是 `PowerStackType.Counter`
    /// 且 `VigorPower` 里**没有任何回合末衰减**，所以没花掉的活力**会跨回合留着**。
    /// 这是个实打实的战术信息（第一回合不攻击，那 8 点原样留到下回合），
    /// 记在 `St::Vigor` 的注释里。
    #[test]
    fn akabeko_grants_vigor_once_on_the_first_turn_and_never_again() {
        let mut s = State::new(80, 1);
        s.add_enemy(enemy::DUMMY, 100);
        let a = s.add_card(card::STRIKE, 0, 0);
        s.player.set(St::Akabeko, 8);
        let mut s = begin_combat(s);
        assert_eq!(s.player.get(St::Vigor), 8, "第一回合开始 +8 活力");

        // 花掉它，这样第二回合的读数只可能来自"赤牛又给了一次"
        s.n_hand = 1;
        s.hand[0] = a;
        s.energy = 3;
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.player.get(St::Vigor), 0, "打一张攻击牌花光");

        let s = step(s, Action::EndTurn);
        assert!(s.turn >= 2, "确实进了第二回合");
        assert_eq!(s.player.get(St::Vigor), 0, "第二回合不再给（TurnNumber <= 1）");
    }

    /// **没花掉的活力跨回合留着。** [源码] `VigorPower` 是 `Counter` 型，
    /// 而且整个类里没有任何 `AfterTurnEnd` / 衰减 —— 只有 `AfterAttack` 会清它。
    /// 所以第一回合只防御的话，那 8 点原样带到第二回合。
    #[test]
    fn unspent_vigor_carries_across_turns() {
        let mut s = State::new(80, 1);
        s.add_enemy(enemy::DUMMY, 100);
        s.player.set(St::Akabeko, 8);
        let s = begin_combat(s);
        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.get(St::Vigor), 8, "一张攻击牌都没打，8 点原样留着");
    }

    /// 0 能量时打 X 费牌：X = 0，合法但什么都不做。
    /// （游戏里确实可以这么打 —— 拒绝它会让 `legal_actions` 少一个真实动作。）
    #[test]
    fn x_cost_card_with_zero_energy_is_legal_and_does_nothing() {
        let s = hand_of(1, 100, &[card::WHIRLWIND], 0);
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.enemies[0].hp, 100, "X = 0 就是不打伤害");
        assert_eq!(s.n_hand, 0, "牌确实打出去了");
    }

    /// 倾泻「打出你抽牌堆顶部的 X 张牌」，升级 X+1。
    /// [源码] `forceExhaust: false` —— 和破灭不一样，打完进弃牌堆不是消耗堆。
    #[test]
    fn cascade_plays_x_cards_from_draw_and_does_not_exhaust_them() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::DUMMY, 100);
        let c = s.add_card(card::CASCADE, 0, 0);
        let a = s.add_card(card::STRIKE, 0, 0);
        let b = s.add_card(card::STRIKE, 0, 0);
        let mut s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = c;
        s.n_draw = 2;
        s.draw[0] = a;
        s.draw[1] = b;
        s.n_disc = 0;
        s.energy = 2;

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.enemies[0].hp, 88, "X=2，两张打击各 6 点");
        assert_eq!(s.n_exh, 0, "倾泻**不**强制消耗（和破灭的区别）");
        assert!(s.n_disc >= 2, "打出的牌进弃牌堆");
    }

    /// 余烬：消耗**抽牌堆顶**那张，不是手牌。
    #[test]
    fn cinder_exhausts_a_random_card_from_hand() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::DUMMY, 100);
        let cd = s.add_card(card::CINDER, 0, 0);
        let keep = s.add_card(card::DEFEND, 0, 0);
        let top = s.add_card(card::STRIKE, 0, 0);
        let mut s = begin_combat(s);
        s.n_hand = 2;
        s.hand[0] = cd;
        s.hand[1] = keep;
        s.n_draw = 1;
        s.draw[0] = top;
        s.n_disc = 0;
        s.energy = 3;

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.enemies[0].hp, 82, "18 点伤害（游戏卡表，不是源码的 17）");
        assert_eq!(s.n_exh, 1);
        assert_eq!(
            card(s.cards[s.exh[0] as usize].id).name,
            "防御",
            "消耗的是手牌里的防御，不是抽牌堆的打击"
        );
        assert_eq!(s.n_hand, 0, "手牌里的防御被消耗掉了");
        assert_eq!(s.n_draw, 1, "抽牌堆里的打击完好无损");
    }

    /// 坚毅：基础版**随机**消耗、升级版**你自己选**。
    /// 卡面两版都只写"消耗1张牌"，这条只有 [源码] 分得开。
    #[test]
    fn true_grit_is_random_at_base_and_a_choice_when_upgraded() {
        // 基础版：直接消耗掉，不进子选择
        let s = hand_of(1, 100, &[card::TRUE_GRIT, card::DEFEND, card::STRIKE], 3);
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.player.block, 7);
        assert_eq!(s.pending, Pending::None, "基础版是随机，不该开选牌界面");
        assert_eq!(s.n_exh, 1, "随机消耗了一张");

        // 升级版：开子选择
        let mut s2 = State::new(80, 1);
        s2.add_enemy(enemy::DUMMY, 100);
        let tg = s2.add_card(card::TRUE_GRIT, F_UPGRADED, 0);
        let d = s2.add_card(card::DEFEND, 0, 0);
        let mut s2 = begin_combat(s2);
        s2.n_hand = 2;
        s2.hand[0] = tg;
        s2.hand[1] = d;
        s2.n_draw = 0;
        s2.n_disc = 0;
        s2.energy = 3;
        let s2 = step(s2, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s2.player.block, 9, "升级版给 9 点格挡");
        assert!(
            matches!(s2.pending, Pending::ExhaustFromHand { .. }),
            "升级版该让玩家自己选"
        );
    }

    /// 痛殴「消耗手牌中随机一张攻击牌，并将它的伤害**添加给这张牌**」。
    ///
    /// 加是**永久**的，写在 `CardInst.bonus` 上 —— 所以是这一张实例变强。
    /// 而且本次结算用的是**旧值**（和暴走一致）。
    #[test]
    fn thrash_permanently_absorbs_the_exhausted_attacks_damage() {
        let mut s = State::new(80, 1);
        s.add_enemy(enemy::DUMMY, 200);
        let th = s.add_card(card::THRASH, 0, 0);
        let victim = s.add_card(card::BOULDER, 0, 0); // 巨石 16 点
        let mut s = begin_combat(s);
        s.n_hand = 2;
        s.hand[0] = th;
        s.hand[1] = victim;
        s.n_draw = 0;
        s.n_disc = 0;
        s.energy = 3;

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.enemies[0].hp, 192, "本次仍按 4×2 结算（用旧值）");
        assert_eq!(s.n_exh, 1, "消耗掉那张巨石");
        // 痛殴那张实例的 bonus 该 +16
        let th_inst = s.cards[th as usize];
        assert_eq!(th_inst.bonus, 16, "巨石的 16 点被永久加到痛殴这一张实例上");
    }

    /// 劫掠「抽牌直到你抽到一张非攻击牌」。
    /// [源码] 是 do-while：**至少抽一张**，抽到非攻击牌就停。
    #[test]
    fn pillage_draws_until_it_hits_a_non_attack() {
        let mut s = State::new(80, 1);
        s.add_enemy(enemy::DUMMY, 100);
        let p = s.add_card(card::PILLAGE, 0, 0);
        // 抽牌堆（顶在末尾）：打击 打击 防御 —— 该抽到 打击,打击,防御 共 3 张
        let d = s.add_card(card::DEFEND, 0, 0);
        let a1 = s.add_card(card::STRIKE, 0, 0);
        let a2 = s.add_card(card::STRIKE, 0, 0);
        let mut s = begin_combat(s);
        s.n_hand = 1;
        s.hand[0] = p;
        s.n_draw = 3;
        s.draw[0] = d;
        s.draw[1] = a1;
        s.draw[2] = a2;
        s.n_disc = 0;
        s.energy = 3;

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.enemies[0].hp, 94, "6 点伤害");
        assert_eq!(s.n_hand, 3, "两张打击 + 停在那张防御上");
        assert_eq!(
            card(s.cards[s.hand[2] as usize].id).name,
            "防御",
            "最后抽到的该是那张非攻击牌"
        );
    }

    /// 劫掠是 **do-while**：**至少抽一张**，哪怕手上最后一张已经是非攻击牌。
    ///
    /// 这条才分得开 `do-while` 和 `while`。上一条测试分不开 —— 那里打出劫掠后
    /// 手牌是空的，两种写法都会去抽。又一次「要主动去找能分开假设的样本」。
    #[test]
    fn pillage_always_draws_at_least_one_card() {
        let mut s = State::new(80, 1);
        s.add_enemy(enemy::DUMMY, 100);
        let p = s.add_card(card::PILLAGE, 0, 0);
        let inhand = s.add_card(card::DEFEND, 0, 0); // 手上留一张**非攻击**牌
        let top = s.add_card(card::DEFEND, 0, 0);
        let mut s = begin_combat(s);
        s.n_hand = 2;
        s.hand[0] = p;
        s.hand[1] = inhand;
        s.n_draw = 1;
        s.draw[0] = top;
        s.n_disc = 0;
        s.energy = 3;

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(
            s.n_hand, 2,
            "手上那张防御 + 必须抽到的那一张。写成 while 的话这里是 1"
        );
        assert_eq!(s.n_draw, 0, "抽牌堆该被抽空");
    }

    /// 原始力量「将手牌中的所有攻击牌变化为巨石」。
    ///
    /// **变形不是"消耗再生成"**：牌实例还是那一张。写成消耗+生成会触发
    /// 无惧疼痛/黑暗之拥 —— 这条测试就是钉住这个区别。
    #[test]
    fn primal_force_transforms_in_place_without_exhausting() {
        let mut s = State::new(80, 1);
        s.add_enemy(enemy::DUMMY, 100);
        let pf = s.add_card(card::PRIMAL_FORCE, 0, 0);
        let a1 = s.add_card(card::STRIKE, 0, 0);
        let a2 = s.add_card(card::BASH, 0, 0);
        let keep = s.add_card(card::DEFEND, 0, 0);
        let mut s = begin_combat(s);
        s.n_hand = 4;
        s.hand[0] = pf;
        s.hand[1] = a1;
        s.hand[2] = a2;
        s.hand[3] = keep;
        s.n_draw = 0;
        s.n_disc = 0;
        s.energy = 3;
        s.player.set(St::FeelNoPain, 5); // 若走了"消耗"路径会白拿格挡

        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.n_hand, 3, "手牌张数不变（除了打出去的原始力量）");
        assert_eq!(s.n_exh, 0, "变形不该消耗任何东西");
        assert_eq!(
            s.player.block, 0,
            "无惧疼痛不该被触发 —— 变形不是消耗"
        );
        let mut rocks = 0;
        for i in 0..s.n_hand as usize {
            if s.cards[s.hand[i] as usize].id == card::BOULDER {
                rocks += 1;
            }
        }
        assert_eq!(rocks, 2, "两张攻击牌都该变成巨石");
    }

    #[test]
    fn test_mcts_rollout_canonical_shuffle_deterministic() {
        let src = match std::fs::read_to_string("traces/act1_f4_second.json") {
            Ok(s) => s,
            Err(_) => return,
        };
        let t = super::replay::parse_trace(&src).unwrap();
        let mut r = super::replay::Replayer::for_trace(&t);
        let mut sy = r.sync(&t.frames[0].obs);
        r.identify_enemies(&mut sy.state, &t.frames[0].obs);

        let mut threat = super::solver::Threat::new();
        threat.set(0, 4, 1);
        threat.set(2, 3, 1);

        let sol = super::solver::solve_turn_budget(&sy.state, &threat, super::solver::score::mcts_rollout, 200_000);
        let human_acts = vec![
            Action::PlayCard { hand: 4, target: 0 },
            Action::PlayCard { hand: 2, target: 0 },
            Action::PlayCard { hand: 2, target: 0 },
        ];
        let h_score = super::solver::score_line(&sy.state, &threat, super::solver::score::mcts_rollout, &human_acts).unwrap();
        assert!(sol.line.score >= h_score, "求解器搜索得分 ({}) 不得低于实战线得分 ({})", sol.line.score, h_score);
    }

    /// 撞上回合上限的推演**没打完**，不能报成"活着结束"。
    ///
    /// `rollout_single` 返回的是一个 `i32`，读不出这件事 —— 它会把一场
    /// 没打赢的仗的当前血量当成结局交出去。`bin/rollout` 的 P4 靠
    /// `Outcome::truncated` 判这条，所以这个字段必须真的立起来。
    #[test]
    fn rollout_reports_truncation_instead_of_faking_survival() {
        let mut s = State::new(80, 4242);
        // 一只打不动、也打不死的敌人：血够厚，几回合的伤害远不够
        s.n_enemies = 1;
        s.enemy_def[0] = super::content::enemy::NIBBIT;
        s.enemies[0] = crate::state::Entity::new(9999);
        for _ in 0..5 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let s = begin_combat(s);

        let o = super::rollout::rollout_outcome(s, 3);
        assert!(o.truncated, "3 个回合打不完这场仗，必须报截断");
        assert!(!o.won && !o.died, "既没赢也没死");
        assert_eq!(o.turns, 3, "走满了上限");
        // 而旧接口把同一个局面报成了一个看着很正常的存活血量
        assert!(
            super::rollout::rollout_single(s, 3) > 0,
            "这正是 P4 要抓的东西：截断的推演在 `rollout_single` 眼里和活着结束长得一样"
        );
    }

    /// 虚弱药水必须报**拒绝**，不能报「省 0 血」。
    ///
    /// `Threat` 来自观测意图标签，`injected_enemy_turn` 拿那个常数直接打、
    /// 不过任何乘区 —— 所以给敌人上虚弱在这一层是空操作，定价必然得 0。
    /// 那个 0 是"这层看不见"，不是"没用"，两者天差地别。
    /// 2026-08-21 实战抓到：蛮兽来袭 14 点、手上虚弱药水，报「省 0 血 · 留着」。
    #[test]
    fn weak_potion_is_refused_not_priced_at_zero() {
        use crate::solver::{advise_potions, PotionPolicy, PotionVerdict, Threat};
        let mut s = State::new(80, 5);
        s.n_enemies = 1;
        s.enemy_def[0] = crate::content::enemy::UNKNOWN;
        s.enemies[0] = crate::state::Entity::new(72);
        s.potions[0] = crate::state::potion::WEAK;
        s.potions[1] = crate::state::potion::BLOCK;
        s.potion_slots = 2;
        for _ in 0..5 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let s = begin_combat(s);

        let mut threat = Threat::new();
        threat.set(0, 14, 1);
        let (_, advice) = advise_potions(
            &s,
            &threat,
            crate::solver::score::survive_first,
            10_000,
            &PotionPolicy::HOUSE_RULE,
        );

        let weak = advice
            .iter()
            .find(|a| a.name == "虚弱药水")
            .expect("虚弱药水该出现在建议里");
        assert_eq!(
            weak.verdict,
            PotionVerdict::ThreatIsFixed,
            "虚弱药水必须拒绝评分，报 Hold/省0 就是个自信的错答案"
        );
        // 对照：格挡药水在同一个局面下**照常定价**，说明拒绝是针对性的、
        // 不是把整张表都关掉了
        let block = advice
            .iter()
            .find(|a| a.name == "格挡药水")
            .expect("格挡药水该出现在建议里");
        assert!(
            matches!(block.verdict, PotionVerdict::Drink | PotionVerdict::Hold),
            "格挡药水该被正常定价，实际 {:?}",
            block.verdict
        );
        assert!(block.hp_saved > 0, "12 点格挡对着 14 点来袭该省下血");
    }

    /// 屈伸药剂：**这一回合** +5 力量，回合末干干净净地还回去。
    ///
    /// 写错过一次：只挂了 `St::TempStrength`（那只是记账标记），结果这回合
    /// 0 力量、回合末还倒扣 5。真正的力量得另写一条 `St::Strength`。
    #[test]
    fn flex_potion_strength_lasts_exactly_one_turn() {
        let mut s = State::new(80, 11);
        s.n_enemies = 1;
        s.enemy_def[0] = crate::content::enemy::UNKNOWN;
        s.enemies[0] = crate::state::Entity::new(200);
        s.potions[0] = crate::state::potion::FLEX;
        s.potion_slots = 1;
        for _ in 0..5 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        s.energy = 3;

        let before = s.enemies[0].hp;
        let s = step(s, Action::UsePotion { slot: 0, target: 0 });
        assert_eq!(s.player.get(St::Strength), 5, "喝完当场就该有 5 点力量");

        // 这一回合打出去的打击吃得到那 5 点
        let h = (0..s.n_hand as usize)
            .find(|&i| s.cards[s.hand[i] as usize].id == card::STRIKE)
            .expect("手里该有打击");
        let s = step(s, Action::PlayCard { hand: h as u8, target: 0 });
        assert_eq!(before - s.enemies[0].hp, 11, "打击 6 + 力量 5");

        let s = step(s, Action::EndTurn);
        assert_eq!(s.player.get(St::Strength), 0, "回合末还回去，不许跨回合");
        assert_eq!(s.player.get(St::TempStrength), 0, "记账标记也清干净");
    }

    /// 爆炸安瓿打**所有**敌人，铁心给的是覆甲（跨回合每回合末出格挡）。
    ///
    /// 这两瓶是 2026-08-21 增量补的，`Op` 一个都没新加 —— 这个测试就是在钉
    /// "复用现成 `Op` 真的复用对了"。
    #[test]
    fn new_potions_reuse_existing_ops_correctly() {
        let mut s = State::new(80, 13);
        s.n_enemies = 2;
        for e in 0..2 {
            s.enemy_def[e] = crate::content::enemy::UNKNOWN;
            s.enemies[e] = crate::state::Entity::new(50);
        }
        s.potions[0] = crate::state::potion::EXPLOSIVE;
        s.potions[1] = crate::state::potion::HEART_OF_IRON;
        s.potion_slots = 2;
        let s = begin_combat(s);

        let s = step(s, Action::UsePotion { slot: 0, target: 0 });
        assert_eq!(s.enemies[0].hp, 40, "爆炸安瓿打第一只 10");
        assert_eq!(s.enemies[1].hp, 40, "**也打第二只** —— AllEnemies，不是单体");

        let s = step(s, Action::UsePotion { slot: 1, target: 0 });
        assert_eq!(s.player.get(St::PlatedArmor), 7, "铁心 = 覆甲 7");
        let s = step(s, Action::EndTurn);
        assert!(
            s.player.get(St::PlatedArmor) > 0,
            "覆甲跨回合留着（回合开始掉 1 层），所以它是跨回合药水"
        );
    }

    /// `fast_play_turn` 和 `fast_play_turn_rec` 必须是同一段代码。
    ///
    /// 记录版是给 `bin/rollout` 的 P3 用的：它要拿"推演里真正打出去的牌"去和
    /// 穷尽搜索比。两个函数一旦长歪，P3 验的就是一个不存在的策略。
    #[test]
    fn fast_play_turn_and_its_recording_twin_do_the_same_thing() {
        let src = match std::fs::read_to_string("traces/act1_f4_second.json") {
            Ok(s) => s,
            Err(_) => return,
        };
        let t = super::replay::parse_trace(&src).unwrap();
        let mut r = super::replay::Replayer::for_trace(&t);
        let mut sy = r.sync(&t.frames[0].obs);
        r.identify_enemies(&mut sy.state, &t.frames[0].obs);

        let mut a = sy.state;
        super::rollout::fast_play_turn(&mut a);

        let mut b = sy.state;
        let mut rec = Some(Vec::new());
        super::rollout::fast_play_turn_rec(&mut b, &mut rec);
        let acts = rec.unwrap();

        assert_eq!(a, b, "记录与否不该改变局面");
        assert!(!acts.is_empty(), "这个局面本来就该打出点什么");
        // 记下来的那串动作重放一遍，必须回到同一个局面
        let replayed = super::solver::replay_line(&sy.state, &acts).expect("记录下来的线必须能重放");
        assert_eq!(replayed, a, "记录下来的动作串和真正走过的路必须一致");
    }
    // ---------------------------------------------------------------
    // 永世沙漏那一套：凋萎存在 / 假升级 / 剧烈增强
    // （2026-08-25 补，见 docs/verification-log.md 的「第 3 幕 Boss 那一项红」）
    // ---------------------------------------------------------------

    /// 凋萎存在：**每打出 6 张牌**往手牌塞 1 张凋萎，计数器归位 6。
    ///
    /// [源码] `WitheringPresencePower.AfterCardPlayed`：先 `CardsLeft--`，
    /// 再判 `<= 0`。所以**第 6 张牌自己也算数**，塞牌发生在它打完之后。
    #[test]
    fn withering_presence_adds_a_wither_every_sixth_card() {
        let mut s = hand_of(1, 300, &[card::DEFEND; 6], 99);
        s.player.set(St::WitheringPresence, 6);

        for i in 1..=5 {
            s = step(s, Action::PlayCard { hand: 0, target: 0 });
            assert_eq!(s.player.get(St::WitheringPresence), 6 - i, "打了 {i} 张牌之后的计数器");
            assert!(
                !(0..s.n_hand as usize).any(|h| s.cards[s.hand[h] as usize].id == card::WITHER),
                "第 {i} 张牌还不该塞凋萎"
            );
        }

        s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.player.get(St::WitheringPresence), 6, "第 6 张之后计数器该归位 6");
        assert_eq!(
            (0..s.n_hand as usize)
                .filter(|&h| s.cards[s.hand[h] as usize].id == card::WITHER)
                .count(),
            1,
            "第 6 张之后手牌里该正好多一张凋萎"
        );
    }

    /// 凋萎的回合末伤害 = **3 + 这一张实例的 `bonus`**。
    ///
    /// 层数是观测量（游戏把它写进牌名 `凋萎+1`），所以这里直接摆 `bonus`。
    /// 2026-08-25 之前 `Op::TakeDamage` 不看 `bonus`，长仗里系统性少扣血。
    #[test]
    fn wither_turn_end_damage_scales_with_its_fake_upgrade_level() {
        let hp_after = |bonus: i16| {
            let mut s = hand_of(3, 300, &[card::WITHER], 3);
            s.cards[s.hand[0] as usize].bonus = bonus;
            // 敌人这一手打多少不影响两条线的**差值**，两边同一个敌人同一手
            let s = step(s, Action::EndTurn);
            s.player.hp
        };
        // 凋萎（3 点） vs 凋萎+1（6 点）：差正好一层的 3 点
        assert_eq!(hp_after(0) - hp_after(3), 3, "每一层假升级该多扣 3 点");
        // 凋萎+2 再多 3 点
        assert_eq!(hp_after(3) - hp_after(6), 3, "第二层同理");
    }

    /// `凋萎+1` 这种「假升级」牌名要认得出来，而且**别和普通升级牌搞混**。
    #[test]
    fn fake_upgrade_suffix_parses_apart_from_ordinary_upgrades() {
        use super::replay::{lookup_card, split_fake_upgrade};
        assert_eq!(split_fake_upgrade("凋萎"), ("凋萎", 0));
        assert_eq!(split_fake_upgrade("凋萎+1"), ("凋萎", 1));
        assert_eq!(split_fake_upgrade("凋萎+2"), ("凋萎", 2));
        // 普通升级牌的名字是**光秃秃一个 `+`**，不该被当成 0 层假升级以外的东西
        assert_eq!(split_fake_upgrade("拆卸+"), ("拆卸+", 0));
        // 三种写法都要落到同一张牌上
        assert_eq!(lookup_card("凋萎"), Some(card::WITHER));
        assert_eq!(lookup_card("凋萎+1"), Some(card::WITHER));
        assert_eq!(lookup_card("凋萎+2"), Some(card::WITHER));
    }

    /// 剧烈增强：一招过后，场上**每一张**凋萎（含它自己刚塞的那张）都升一层。
    ///
    /// **这个测试特意从「场上一张凋萎都没有」开始** —— 那是唯一能把内核的
    /// 两种写法分开的局面。[源码] `IncreasingIntensityMove` 先升级已有的、
    /// 再 `WitherUpgradeCount++`、最后生成一张**已经匹配到新层数**的凋萎，
    /// 所以哪怕一张都没有，新生成的那张也是 `凋萎+1`。
    ///
    /// 内核没有那个计数器（不是观测量），靠 `spawn_card` 从场上同名牌抄，
    /// 于是必须**先生成、再连新的一起升级**。反过来写在这个局面下给 0 层，
    /// 而**实录分不开这两种写法**（两次剧烈增强时场上都已经有凋萎了）——
    /// 判据只有源码，守它的只有这个测试。
    #[test]
    fn increasing_intensity_levels_up_even_the_wither_it_just_spawned() {
        let fresh = || {
            let mut s = State::new(80, 11);
            s.add_enemy(enemy::AEONGLASS, 512);
            for _ in 0..5 {
                s.add_card(card::DEFEND, 0, 0);
            }
            s
        };
        let levels = |s: State| -> Vec<i16> {
            let mut s = begin_combat(s);
            // 0 退潮 / 1 眼部激光 / 2 剧烈增强
            s.enemy_move[0] = 2;
            let s = step(s, Action::EndTurn);
            (0..s.n_cards as usize)
                .filter(|&i| s.cards[i].id == card::WITHER)
                .map(|i| s.cards[i].bonus)
                .collect()
        };

        // (a) 场上一张凋萎都没有 —— 分得开两种写法的那个局面
        let a = levels(fresh());
        assert_eq!(a, vec![3], "凭空生成的第一张凋萎就该是 +1 层（6 点），实际 {a:?}");

        // (b) 场上已经有一张 0 层的 —— 两种写法在这里给出相同的数
        let mut s = fresh();
        s.add_card(card::WITHER, 0, 0);
        let b = levels(s);
        assert_eq!(b.len(), 2, "老的那张 + 新塞进弃牌堆的那张");
        assert!(b.iter().all(|&x| x == 3), "两张都该是 +1 层，实际 {b:?}");
    }

    /// 异蛇之油：抽 7 张，**然后**把手牌的费用随机成 0..3。
    ///
    /// 具体抽到几费是内核 RNG 的一个样本（不变量 4），所以这里只判两件
    /// 可判的事：张数，和「每一张都落在 0..=3 里」。
    #[test]
    fn snecko_oil_draws_seven_then_randomizes_every_hand_cost() {
        let mut s = State::new(80, 23);
        s.add_enemy(enemy::DUMMY, 300);
        // 抽牌堆里塞满 3 费的怒吼，随机之后不可能还是 3 费以上
        for _ in 0..12 {
            s.add_card(card::BLOODLETTING, 0, 0);
        }
        let mut s = begin_combat(s);
        s.potions[0] = crate::state::potion::SNECKO_OIL;
        let before = s.n_hand as usize;
        let draw_before = s.n_draw as usize;
        let s = step(s, Action::UsePotion { slot: 0, target: 0 });
        // 开局 5 张 + 抽 7 = 12，**但手牌上限是 10**（`MAX_HAND`）。
        // 这条断言 2026-08-29 从 `before + 7` 改成上限 —— 原来那个数是内核
        // 当时**没有手牌上限**的产物，不是游戏行为。上限本身由
        // `act2_f33_boss_crusher_retry` 帧13 钉死（战斗专注抽 3 只进来 2）。
        assert_eq!(s.n_hand as usize, MAX_HAND, "抽到手牌满 10 张就停");
        // 满手之后的那几次抽牌是空操作：抽牌堆只少了实际进手的那几张
        assert_eq!(
            draw_before - s.n_draw as usize,
            MAX_HAND - before,
            "被上限挡下的那几张要留在抽牌堆里，不许凭空消失"
        );
        for h in 0..s.n_hand as usize {
            let c = effective_cost(&s, h);
            assert!((0..=3).contains(&c), "第 {h} 张的费用 {c} 落在 0..=3 之外");
        }
    }

    /// 能力药水：生成一张**能力牌**，且**本回合免费**。
    /// （保守近似：真实是 3 张挑 1，内核只给 1 张，见 `Op::GenerateFree`）
    #[test]
    fn power_potion_generates_one_free_power_card() {
        let mut s = State::new(80, 29);
        s.add_enemy(enemy::DUMMY, 300);
        for _ in 0..5 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.potions[0] = crate::state::potion::POWER;
        let before = s.n_hand;
        let s = step(s, Action::UsePotion { slot: 0, target: 0 });
        assert_eq!(s.n_hand, before + 1, "该多一张牌");
        let ix = s.hand[s.n_hand as usize - 1] as usize;
        assert!(
            matches!(crate::content::card(s.cards[ix].id).kind, crate::ops::Kind::Power),
            "生成的该是能力牌，实际 {}",
            crate::content::card(s.cards[ix].id).name
        );
        assert_eq!(effective_cost(&s, s.n_hand as usize - 1), 0, "该本回合免费");
    }

    /// 招架盾：回合结束时**格挡 ≥ 10** 才打那 6 点，9 点格挡不算。
    ///
    /// 门槛这条要单独测，因为 [源码] 判的是 `!(Block < 10)` ——
    /// 边界值 10 是**触发**的那一侧，写成 `> 10` 会静默少打一整场的伤害。
    #[test]
    fn parrying_shield_fires_at_ten_block_not_at_nine() {
        for (block, expect_hit) in [(9, false), (10, true)] {
            let mut s = State::new(80, 29);
            s.add_enemy(enemy::DUMMY, 300);
            for _ in 0..5 {
                s.add_card(card::DEFEND, 0, 0);
            }
            let mut s = begin_combat(s);
            s.player.set(St::ParryingShield, 6);
            s.player.block = block;
            let hp_before = s.enemies[0].hp;
            // 只跑到"我的回合结束"这一段：`end_turn_with_incoming` 不让敌人还手，
            // 敌人掉的血就只可能是招架盾打的
            let after = crate::end_turn_with_incoming(s, &[(0, 0); crate::state::MAX_ENEMIES]);
            let dealt = hp_before - after.enemies[0].hp;
            assert_eq!(
                dealt,
                if expect_hit { 6 } else { 0 },
                "{block} 点格挡时招架盾打了 {dealt} 点"
            );
        }
    }

    /// 尖叫酒壶：回合结束**手牌为空**才打那 20 点，手里还有牌就一点不打。
    ///
    /// **反例那一半才是这条规则的全部内容。** [源码] 的钩子是
    /// `BeforeSideTurnEnd`，跑在**弃手牌之前**；挪到弃牌之后手牌恒空，
    /// 这件遗物会变成"每回合白给 20 点"，而且**四种对拍模式都不会红**
    /// —— 那是敌人血量上一个稳定的偏差，观测里看不出是谁打的。
    ///
    /// 顺带钉两件事：打的是**所有**敌人（[源码] `HittableEnemies`），
    /// 以及力量不进这一条（`ValueProp.Unpowered`）。
    #[test]
    fn screaming_flagon_fires_only_on_an_empty_hand_and_hits_everyone() {
        for (n_hand, expect) in [(3u8, 0), (0u8, 20)] {
            let mut s = State::new(80, 29);
            s.add_enemy(enemy::DUMMY, 300);
            s.add_enemy(enemy::DUMMY, 300);
            for _ in 0..5 {
                s.add_card(card::DEFEND, 0, 0);
            }
            let mut s = begin_combat(s);
            s.player.set(St::ScreamingFlagon, 20);
            // 力量 5 —— 进了这一条就说明走错了乘区（该走 unpowered）
            s.player.set(St::Strength, 5);
            s.n_hand = n_hand;
            let before = [s.enemies[0].hp, s.enemies[1].hp];
            // 敌人不还手，掉的血就只可能是酒壶打的
            let after = crate::end_turn_with_incoming(s, &[(0, 0); crate::state::MAX_ENEMIES]);
            for e in 0..2 {
                assert_eq!(
                    before[e] - after.enemies[e].hp,
                    expect,
                    "手牌 {n_hand} 张时敌人 {e} 掉了 {} 点",
                    before[e] - after.enemies[e].hp
                );
            }
        }
    }

    /// 百年积木：本场**第一次真掉血**抽 3，第二次不再抽。
    ///
    /// 「真掉血」是判据：一下被格挡完全吃掉不该触发
    /// （[源码] 判的是 `result.UnblockedDamage > 0`）。三种情况一起测，
    /// 因为把它写成"挨打就抽"和写成"掉血就抽"在**没格挡**的样本上一模一样。
    #[test]
    fn centennial_puzzle_draws_three_on_first_unblocked_hit_only() {
        let mut s = State::new(80, 29);
        s.add_enemy(enemy::DUMMY, 300);
        for _ in 0..20 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        s.player.set(St::CentennialPuzzle, 3);

        // 回合结束会**弃掉整只手牌**再抽 5，所以这里比的是绝对值不是增量：
        // 正常一手 5 张，百年积木发作那一回合是 5 + 3。
        let incoming = |d: i32| {
            let mut t = [(0, 0); crate::state::MAX_ENEMIES];
            t[0] = (d, 1);
            t
        };

        // (a) 被完全挡住 -> 不抽
        s.player.block = 20;
        let mut s = crate::end_turn_with_incoming(s, &incoming(5));
        assert_eq!(s.n_hand, 5, "被挡住不该额外抽牌");
        assert_eq!(s.player.get(St::CentennialPuzzle), 3, "没触发就不该被清掉");

        // (b) 真掉血 -> 抽 3
        s.player.block = 0;
        let mut s = crate::end_turn_with_incoming(s, &incoming(5));
        assert_eq!(s.n_hand, 8, "第一次真掉血该多抽 3 张");
        assert_eq!(s.player.get(St::CentennialPuzzle), 0, "触发过就该清掉");

        // (c) 再掉一次血 -> 不再抽
        s.player.block = 0;
        let s = crate::end_turn_with_incoming(s, &incoming(5));
        assert_eq!(s.n_hand, 5, "一场只该发作一次");
    }

    /// 第二批遗物里**只在某一回合发作**的那几件：锚（只第1回合）、
    /// 烛台（[源码] 是 `== 2`，不是 `>= 2`）、佩尔之肉（`>= 3` 之后**每回合**）。
    ///
    /// 三件一起测，因为它们错的方式是同一种：**把边界条件抄成另一个**。
    /// 烛台写成 `>=` 会一路白给能量，佩尔之肉写成 `==` 会只给一回合 ——
    /// 两种都不会报错，只会让求解器算在一个不存在的资源上。
    ///
    /// **status 必须在 `begin_combat` 之前挂**：第 1 回合的 `TurnStart`
    /// 就在 `begin_combat` 里点火，事后再挂就赶不上了。
    #[test]
    fn second_relic_batch_turn_gated_relics_fire_on_the_right_turns() {
        let run = |st: St, stacks: i32, turns: usize| -> Vec<(i32, i32)> {
            let mut s = State::new(80, 29);
            s.add_enemy(enemy::DUMMY, 300);
            for _ in 0..20 {
                s.add_card(card::DEFEND, 0, 0);
            }
            s.player.set(st, stacks);
            let mut s = begin_combat(s);
            let mut out = vec![(s.energy, s.player.block)];
            for _ in 1..turns {
                s = crate::end_turn_with_incoming(s, &[(0, 0); crate::state::MAX_ENEMIES]);
                out.push((s.energy, s.player.block));
            }
            out
        };
        // 锚：只有第 1 回合那 10 点格挡
        let a = run(St::Anchor, 10, 3);
        assert_eq!(a[0].1, 10, "第1回合该有 10 点格挡");
        assert_eq!(a[1].1, 0, "第2回合不该再给");
        // 烛台：**只有**第 2 回合 +2
        let c = run(St::Candelabra, 2, 4);
        assert_eq!(c[0].0, 3, "第1回合就是基础 3 点能量");
        assert_eq!(c[1].0, 5, "第2回合 +2");
        assert_eq!(c[2].0, 3, "第3回合不该再给 —— 源码是 == 2");
        // 佩尔之肉：第 3 回合起**每回合** +1
        let f = run(St::PaelsFlesh, 1, 5);
        assert_eq!(f[0].0, 3, "第1回合不给");
        assert_eq!(f[1].0, 3, "第2回合不给");
        assert_eq!(f[2].0, 4, "第3回合 +1");
        assert_eq!(f[3].0, 4, "第4回合还要给 —— 源码是 >= 3 不是 == 3");
    }

    /// 弹珠袋 / 红面具：第 1 回合给**全体**敌人挂 1 层，而且只给活着的。
    ///
    /// 「全体」这一条只有多怪场分得开 —— 单怪场里写成"给第一只"也是绿的。
    #[test]
    fn second_relic_batch_bag_of_marbles_hits_every_living_enemy() {
        let mut s = State::new(80, 29);
        s.add_enemy(enemy::DUMMY, 30);
        s.add_enemy(enemy::DUMMY, 30);
        s.add_enemy(enemy::DUMMY, 30);
        for _ in 0..10 {
            s.add_card(card::DEFEND, 0, 0);
        }
        s.player.set(St::BagOfMarbles, 1);
        s.player.set(St::RedMask, 1);
        // 第三只开打前就是死的：不该被挂
        s.enemies[2].hp = 0;
        let s = begin_combat(s);
        for e in 0..2 {
            assert_eq!(s.enemies[e].get(St::Vulnerable), 1, "第 {e} 只该有易伤");
            assert_eq!(s.enemies[e].get(St::Weak), 1, "第 {e} 只该有虚弱");
        }
        assert_eq!(s.enemies[2].get(St::Vulnerable), 0, "死了的不该被挂");
    }

    /// 苦无：同一回合**每第 3 张**攻击牌 +1 敏捷（第 3/6 张都给，不是只有第 3 张）。
    /// 和精致折扇同一个条件，抄错成"只有第 3 张"不会报错、只会少给。
    #[test]
    fn second_relic_batch_kunai_fires_every_third_attack() {
        let cards = [card::STRIKE; 6];
        let mut s = hand_of(7, 500, &cards, 99);
        s.player.set(St::Kunai, 1);
        let mut dex = vec![];
        for _ in 0..6 {
            s = step(s, Action::PlayCard { hand: 0, target: 0 });
            dex.push(s.player.get(St::Dexterity));
        }
        assert_eq!(dex, vec![0, 0, 1, 1, 1, 2], "第3张和第6张各给 1 点敏捷，实际 {dex:?}");
    }

    /// 开信刀：同回合每 **3 张技能牌** 对全体 5 点，而且**只在技能牌上数**。
    ///
    /// 这条测试是从一个当天自己造出来的 bug 里长出来的：第一版把它挂在
    /// `Hook::CardPlayed` 上（每张牌都触发），条件只看"技能数是不是 3 的倍数" ——
    /// 于是打满 3 张技能之后**后面每打一张牌都再发作一次**。
    /// 实录当帧就红了（内核多杀一只、多回 6 血）。
    ///
    /// 所以这里**必须测第 4 张牌打的是攻击**：只测"3 张技能发作一次"是绿的。
    #[test]
    fn letter_opener_fires_on_every_third_skill_only() {
        let cards = [card::DEFEND, card::DEFEND, card::DEFEND, card::STRIKE, card::DEFEND];
        let mut s = hand_of(11, 500, &cards, 99);
        s.player.set(St::LetterOpener, 5);
        let mut hp = vec![];
        for _ in 0..5 {
            s = step(s, Action::PlayCard { hand: 0, target: 0 });
            hp.push(s.enemies[0].hp);
        }
        // 前两张技能不发作；第 3 张 -5；**第 4 张是攻击**：只吃打击自己的 6 点；
        // 第 5 张技能（技能数 4，不是 3 的倍数）不发作。
        assert_eq!(hp, vec![500, 500, 495, 489, 489], "实际 {hp:?}");
    }

    /// `content::powers_for` 必须**逐条等于**「整表按钩子过滤、保持表内顺序」——
    /// `fire_ctx` 从整表线性扫换成按组扫，全部等价性就押在这一条上。
    /// 顺序换了不会报错，只会让同一钩子上两条规则的先后悄悄变掉（奥利哈钢那条注释说的那种）。
    #[test]
    fn powers_by_hook_is_the_table_filtered_in_order() {
        let mut hooks: Vec<crate::ops::Hook> = Vec::new();
        for p in POWERS {
            if !hooks.contains(&p.hook) {
                hooks.push(p.hook);
            }
        }
        let mut total = 0;
        for h in hooks {
            let want: Vec<*const crate::ops::PowerDef> = POWERS.iter().filter(|p| p.hook == h).map(|p| p as *const _).collect();
            let got: Vec<*const crate::ops::PowerDef> = powers_for(h).iter().map(|&p| p as *const _).collect();
            assert_eq!(got, want, "{h:?} 那一组和整表过滤不一致");
            total += got.len();
        }
        assert_eq!(total, POWERS.len(), "每一条规则恰好在一组里");
    }

    // -----------------------------------------------------------------------
    // 2026-09-25 预先补的一批遗物。全部照 [源码]，还没有一件在实录里出现过。
    // -----------------------------------------------------------------------

    fn relic_state(relics: &[&str], cards: &[u16], seed: u64) -> State {
        let mut s = State::new(80, seed);
        s.add_enemy(enemy::DUMMY, 500);
        for &c in cards {
            s.add_card(c, 0, 0);
        }
        let defs: Vec<_> = relics.iter().map(|id| (relic_by_id(id).expect(id), None)).collect();
        grant_relics(&mut s, &defs);
        begin_combat(s)
    }

    /// 天鹅绒颈圈：第 7 张一张都不给打（`legal_actions` 和 `step` 同一个答案），下回合重数。
    #[test]
    fn velvet_choker_stops_the_seventh_card_and_resets_next_turn() {
        let mut s = hand_of(3, 500, &[card::STRIKE; 8], 99);
        s.player.set(St::VelvetChoker, 6);
        for _ in 0..6 {
            s = step(s, Action::PlayCard { hand: 0, target: 0 });
        }
        assert_eq!(s.cards_played, 6);
        let (acts, n) = legal_actions(&s);
        assert!(!acts[..n].iter().any(|a| matches!(a, Action::PlayCard { .. })), "第 7 张不给打");
        assert_eq!(step(s, Action::PlayCard { hand: 0, target: 0 }), s, "step 也原样返回（不变量 5）");
        let s = end_turn_with_incoming(s, &no_incoming());
        let (acts, n) = legal_actions(&s);
        assert!(acts[..n].iter().any(|a| matches!(a, Action::PlayCard { .. })), "下个回合重新数");
    }

    /// 出牌数打满之后，**自动打出**的牌也被拦：[源码] `CardCmd.AutoPlay` 里 `ShouldPlay`
    /// 返回 false 就 `MoveToResultPileWithoutPlaying`，和不可打出的牌同一句。
    #[test]
    fn play_cap_also_blocks_autoplay_from_exhaust() {
        let run = |cap: i32| {
            let mut s = hand_of(3, 500, &[card::HOWL_FROM_BEYOND], 3);
            let cix = s.take_from_hand(0);
            s.to_exhaust(cix);
            s.player.set(St::VelvetChoker, cap);
            s.cards_played = 6;
            end_turn_with_incoming(s, &no_incoming()).enemies[0].hp
        };
        assert_eq!(run(7), 484, "没打满：彼岸咆哮回合末从消耗堆里打出 16");
        assert_eq!(run(6), 500, "打满了：没打出去");
    }

    /// 单帧同步时 `cards_played` 游戏不报，但颈圈 / 头冠的面板计数器就是它。
    #[test]
    fn cards_played_is_read_from_the_choker_or_diadem_counter() {
        use crate::replay::{observed_cards_played, RelicObs};
        let relic = |id: &str, counter: Option<i32>| RelicObs { counter, id: id.into(), name: String::new() };
        assert_eq!(observed_cards_played(&[relic("VELVET_CHOKER", Some(4))]), Some(4));
        assert_eq!(observed_cards_played(&[relic("PEN_NIB", Some(7)), relic("DIAMOND_DIADEM", Some(2))]), Some(2));
        assert_eq!(observed_cards_played(&[relic("PEN_NIB", Some(7))]), None, "别的遗物的计数器不算");
    }

    /// 小提琴：开局 5+2；回合中的抽牌全部被拦；`ModifyHandDraw` 那一族（佩尔之血）不拦；
    /// 摆动球是真抽牌（`AfterPlayerTurnStart`），拦。
    #[test]
    fn fiddle_draws_seven_then_locks_draws_for_the_rest_of_my_turn() {
        let s = relic_state(&["FIDDLE"], &[card::POMMEL_STRIKE; 20], 5);
        assert_eq!(s.n_hand, 7, "开局 5 + 2");
        let t = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(t.n_hand, 6, "剑柄打击的抽 1 被拦：打出一张、没抽回来");
        assert_eq!(t.n_draw, s.n_draw, "被拦的抽牌不动抽牌堆");

        let s = relic_state(&["FIDDLE", "PAELS_BLOOD"], &[card::STRIKE; 20], 5);
        assert_eq!(s.n_hand, 8, "佩尔之血改的是发牌张数，不拦");

        let mut s = relic_state(&["FIDDLE"], &[card::STRIKE; 20], 5);
        s.player.set(St::Pendulum, 1);
        s.player.set(St::PendulumPhase, 1); // 下一回合（第 2 回合）就发作
        let s = end_turn_with_incoming(s, &no_incoming());
        assert_eq!(s.n_hand, 7, "摆动球那 1 张是回合中的真抽牌，被拦");
    }

    /// 小提琴的锁**只在我的回合里**（[源码] `Side != CurrentSide` 就放行）：
    /// 敌人回合挨打触发的百年积木照抽 3 张，下回合开局 3 + 7 = 10 张（顶到手牌上限）。
    /// 锁要是不看是谁的回合，那 3 张会被拦掉，开局只有 7 张。
    #[test]
    fn fiddle_does_not_lock_draws_during_the_enemy_turn() {
        let s = relic_state(&["FIDDLE", "CENTENNIAL_PUZZLE"], &[card::STRIKE; 30], 5);
        assert!(!s.enemy_side);
        let mut inc = no_incoming();
        inc[0] = (5, 1);
        let t = end_turn_with_incoming(s, &inc);
        assert_eq!(t.player.hp, 75, "敌人那 5 点打穿了");
        assert_eq!(t.n_hand, 10, "敌人回合里百年积木抽的 3 张没被拦");
        assert!(!t.enemy_side, "回到我的回合，锁又生效");
    }

    /// 战斗专注的「不能再抽牌」**到我的回合结束为止**（[源码] `NoDrawPower.AfterSideTurnEnd`）：
    /// 敌人回合里挨打触发的百年积木照抽 3 张。原来的内核拦到下一个回合开始，这 3 张被吞掉。
    /// 同一回合里的抽牌照旧被拦（这一半是 2026-08 实测过的老行为）。
    #[test]
    fn battle_trance_stops_draws_only_until_my_turn_ends() {
        let mut cards = vec![card::BATTLE_TRANCE, card::POMMEL_STRIKE];
        cards.extend([card::STRIKE; 20]);
        let mut s = relic_state(&["CENTENNIAL_PUZZLE"], &cards, 4);
        // 两张牌都要在手上：在抽牌堆里的就和手牌对调（两边都不重复）
        for (slot, id) in [(0usize, card::BATTLE_TRANCE), (1, card::POMMEL_STRIKE)] {
            let cix = (0..s.n_cards).find(|&i| s.cards[i as usize].id == id).unwrap();
            if let Some(p) = (0..s.n_draw as usize).find(|&p| s.draw[p] == cix) {
                s.draw[p] = s.hand[slot];
                s.hand[slot] = cix;
            }
        }
        s.energy = 9;
        let s = step(s, Action::PlayCard { hand: hand_ix_of(&s, card::BATTLE_TRANCE), target: 0 });
        assert!(s.player.get(St::NoDraw) > 0);
        let before = s.n_hand;
        let s = step(s, Action::PlayCard { hand: hand_ix_of(&s, card::POMMEL_STRIKE), target: 0 });
        assert_eq!(s.n_hand, before - 1, "同一回合：剑柄打击的抽 1 被拦");
        let mut inc = no_incoming();
        inc[0] = (5, 1);
        let t = end_turn_with_incoming(s, &inc);
        assert_eq!(t.player.hp, 75, "敌人那 5 点打穿了");
        assert_eq!(t.n_hand, 3 + 5, "敌人回合里百年积木的 3 张没被拦，加上开局 5 张");
        assert_eq!(t.player.get(St::NoDraw), 0, "到我的下一个回合它已经不在了（和观测一致）");
    }

    /// 钻石头冠：本回合出牌 ≤ 2 ⇒ 敌人这一手 ×0.5；3 张就不给。
    /// **两条敌人回合的路都要减半**：冻住的意图标签（对拍 / `Threat::set`）和现算（`Threat::set_live`）。
    #[test]
    fn diamond_diadem_halves_the_next_enemy_turn_only_after_a_short_turn() {
        let hit = |played: usize, live: bool| {
            let mut s = hand_of(3, 500, &[card::DEFEND; 4], 99);
            s.player.set(St::DiamondDiadem, 1);
            s.player.set(St::Dexterity, -5); // 防御给 0 格挡，只数出牌张数
            for _ in 0..played {
                s = step(s, Action::PlayCard { hand: 0, target: 0 });
            }
            let mut inc = no_incoming();
            inc[0] = (11, 1);
            let t = if live { end_turn_with_live_incoming(s, &inc) } else { end_turn_with_incoming(s, &inc) };
            assert_eq!(t.player.get(St::DiamondDiademActive), 0, "敌人回合末摘掉");
            80 - t.player.hp
        };
        assert_eq!(hit(2, false), 5, "冻住的标签 11 -> ⌊11/2⌋");
        assert_eq!(hit(2, true), 5, "现算：11 × 1/2 只取整一次");
        assert_eq!(hit(0, false), 5, "一张不出也算 ≤ 2");
        assert_eq!(hit(3, false), 11, "出了 3 张：不减半");
        assert_eq!(hit(3, true), 11);
    }

    /// 冻住的标签上补减半：无实体下标签已经是 1，不补（游戏是先 ×0.5 再压成 1）。
    #[test]
    fn frozen_label_halving_respects_intangible() {
        let mut me = Entity::new(80);
        assert_eq!(frozen_label_after_turn_end(11, &me), 11, "没挂减半不动");
        me.set(St::DiamondDiademActive, 1);
        assert_eq!(frozen_label_after_turn_end(11, &me), 5);
        assert_eq!(frozen_label_after_turn_end(1, &me), 0, "1 × 0.5 = 0.5 -> 0");
        me.set(St::Intangible, 1);
        assert_eq!(frozen_label_after_turn_end(1, &me), 1, "无实体：标签就是答案");
    }

    /// 波纹水盆：本回合没打过攻击 ⇒ 回合末 4 格挡（在敌人出手之前）；打过一张就没有。
    #[test]
    fn ripple_basin_blocks_only_on_a_turn_without_attacks() {
        let taken = |first: u16| {
            let mut s = hand_of(3, 500, &[first], 99);
            s.player.set(St::RippleBasin, 4);
            s.player.set(St::Dexterity, -5);
            let s = step(s, Action::PlayCard { hand: 0, target: 0 });
            let mut inc = no_incoming();
            inc[0] = (6, 1);
            80 - end_turn_with_incoming(s, &inc).player.hp
        };
        assert_eq!(taken(card::DEFEND), 2, "只打了技能：4 格挡吃掉 6 里的 4");
        assert_eq!(taken(card::STRIKE), 6, "打过攻击：没有格挡");
    }

    /// 风的女儿：每张攻击 +1 格挡，`Unpowered` ⇒ 不吃敏捷；技能不给。
    #[test]
    fn daughter_of_the_wind_gives_one_block_per_attack_ignoring_dexterity() {
        let mut s = hand_of(3, 500, &[card::STRIKE, card::STRIKE], 99);
        s.player.set(St::DaughterOfTheWind, 1);
        s.player.set(St::Dexterity, 3);
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.player.block, 1);
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.player.block, 2);
    }

    /// 手里剑：和苦无同一个条件（第 3/6 张），给的是力量。
    #[test]
    fn shuriken_fires_every_third_attack() {
        let mut s = hand_of(7, 500, &[card::STRIKE; 6], 99);
        s.player.set(St::Shuriken, 1);
        let mut st = vec![];
        for _ in 0..6 {
            s = step(s, Action::PlayCard { hand: 0, target: 0 });
            st.push(s.player.get(St::Strength));
        }
        assert_eq!(st, vec![0, 0, 1, 1, 1, 2], "实际 {st:?}");
    }

    /// 锁镰：同回合第 3 张攻击，随机一个敌人 6 点（`Unpowered`，不吃力量）。
    #[test]
    fn kusarigama_hits_a_random_enemy_on_every_third_attack_without_strength() {
        let mut s = hand_of(7, 500, &[card::STRIKE; 3], 99);
        s.player.set(St::Kusarigama, 6);
        s.player.set(St::Strength, 2);
        let mut hp = vec![];
        for _ in 0..3 {
            s = step(s, Action::PlayCard { hand: 0, target: 0 });
            hp.push(s.enemies[0].hp);
        }
        // 每张打击 6+2 = 8；第 3 张之后锁镰 6（不加力量）
        assert_eq!(hp, vec![492, 484, 470], "实际 {hp:?}");
    }

    /// 双截棍：计数器跨战斗，从面板灌；第 10 张攻击 +1 能量。
    /// 面板在触发后的那一秒显示 10（`IsActivating`），灌进来 10 也不能凭空多给。
    #[test]
    fn nunchaku_counts_across_combats_and_tolerates_the_activating_display() {
        let energy_after = |counter: i32, attacks: usize| {
            let mut s = hand_of(3, 500, &[card::STRIKE; 4], 50);
            s.player.set(St::Nunchaku, 1);
            s.player.set(St::NunchakuCount, counter);
            let e0 = s.energy;
            for _ in 0..attacks {
                s = step(s, Action::PlayCard { hand: 0, target: 0 });
            }
            s.energy - (e0 - attacks as i32)
        };
        assert_eq!(energy_after(8, 1), 0, "第 9 张：不给");
        assert_eq!(energy_after(8, 2), 1, "第 10 张：+1");
        assert_eq!(energy_after(10, 1), 0, "面板显示 10 = 刚触发过，下一张是第 1 张");
        assert_eq!(energy_after(0, 4), 0);
    }

    /// 铁棒：任意牌，第 4 张抽 1（计数器同双截棍）。
    #[test]
    fn iron_club_draws_on_every_fourth_card_of_any_kind() {
        let s = relic_state(&["IRON_CLUB"], &[card::DEFEND; 20], 5);
        let mut s = s;
        s.energy = 99;
        s.player.set(St::IronClubCount, 2);
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.n_hand, 4, "第 3 张：不抽");
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.n_hand, 4, "第 4 张：打出一张、抽回一张");
    }

    /// 棋子：打能力牌抽 1；带着小提琴时被拦（它是真抽牌）。
    #[test]
    fn game_piece_draws_on_power_and_is_locked_by_fiddle() {
        let mut cards = vec![card::DEMON_FORM];
        cards.extend([card::STRIKE; 20]);
        let play_power = |relics: &[&str]| {
            let mut s = relic_state(relics, &cards, 9);
            s.energy = 99;
            // 恶魔形态不一定在起手里：在抽牌堆里的话和手牌第一格对调（两边都不重复）
            let dix = (0..s.n_cards).find(|&i| s.cards[i as usize].id == card::DEMON_FORM).unwrap();
            if let Some(p) = (0..s.n_draw as usize).find(|&p| s.draw[p] == dix) {
                s.draw[p] = s.hand[0];
                s.hand[0] = dix;
            }
            let h = hand_ix_of(&s, card::DEMON_FORM);
            s.n_hand as i32 - step(s, Action::PlayCard { hand: h, target: 0 }).n_hand as i32
        };
        assert_eq!(play_power(&["GAME_PIECE"]), 0, "打出一张、抽回一张");
        assert_eq!(play_power(&["GAME_PIECE", "FIDDLE"]), 1, "小提琴拦掉了那 1 张");
    }

    /// 两件赝品复用正品的 status：同时带着时相加（和锚 + 锚？？？同一个先例）。
    #[test]
    fn fake_strike_dummy_and_fake_orichalcum_stack_with_the_real_ones() {
        let s = relic_state(&["STRIKE_DUMMY", "FAKE_STRIKE_DUMMY"], &[card::STRIKE; 10], 3);
        assert_eq!(s.player.get(St::StrikeDummy), 4);
        let t = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.enemies[0].hp - t.enemies[0].hp, 6 + 4, "打击 6 + 木偶 3 + 赝品 1");

        let s = relic_state(&["FAKE_ORICHALCUM"], &[card::STRIKE; 10], 3);
        let mut inc = no_incoming();
        inc[0] = (3, 1);
        assert_eq!(end_turn_with_incoming(s, &inc).player.hp, 80, "赝品单独带：3 格挡挡住 3 点");
        let s = relic_state(&["ORICHALCUM", "FAKE_ORICHALCUM"], &[card::STRIKE; 10], 3);
        inc[0] = (10, 1);
        assert_eq!(end_turn_with_incoming(s, &inc).player.hp, 79, "两件一起：6 + 3 = 9 格挡");
    }

    /// 我方**虚弱**要作用在 AoE 牌上（突破+ 13 点打全体）。
    ///
    /// 2026-08-27 第 2 幕精英那一帧对拍红了：游戏 14、内核 19。
    /// 13 × 3/4（我虚弱1）× 3/2（目标易伤2）= 14.6 -> 14，游戏是对的。
    #[test]
    fn weak_applies_to_aoe_cards() {
        let mut s = State::new(80, 33);
        s.add_enemy(enemy::DUMMY, 100);
        s.add_card(card::BREAKTHROUGH, crate::state::F_UPGRADED, 0);
        for _ in 0..6 { s.add_card(card::DEFEND, 0, 0); }
        let mut s = begin_combat(s);
        s.player.set(St::Weak, 1);
        s.enemies[0].set(St::Vulnerable, 2);
        let b = (0..s.n_cards).find(|&i| s.cards[i as usize].id == card::BREAKTHROUGH).unwrap();
        s.hand[0] = b;
        s.energy = 9;
        let hp0 = s.enemies[0].hp;
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(hp0 - s.enemies[0].hp, 14, "13 × 3/4 × 3/2 = 14");
    }

    /// 朝向：**背后打过来的那一击 ×1.5**，而打出有目标的牌会把朝向转过去。
    ///
    /// [实测] 2026-08-27 第 2 幕 Boss 帧25 -> 帧28：同一只、同一手招式，
    /// 我把牌打到它身上之后意图标签 `21 -> 14`。21 = 14 × 1.5。
    /// 这条测试复刻那一对：先背对它挨 21，转身之后挨 14。
    #[test]
    fn facing_flips_the_back_attack_multiplier() {
        let mut s = State::new(80, 41);
        s.add_enemy(enemy::DUMMY, 100);
        s.add_card(card::STRIKE, 0, 0);
        for _ in 0..6 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.player.set(St::Surrounded, 1);
        s.enemies[0].set(St::BackAttackLeft, 1);
        // 我面朝右 ⇒ 左边那只在背后
        s.player.set(St::FacingRight, 1);
        let mut inc = [(0, 0); crate::state::MAX_ENEMIES];
        inc[0] = (14, 1); // 面板基础值 14
        let behind = crate::end_turn_with_live_incoming(s, &inc);
        assert_eq!(s.player.hp - behind.player.hp, 21, "背后挨打该是 14×1.5=21");

        // 打一张有目标的牌转身，再挨同一手
        let st = s.hand[..s.n_hand as usize]
            .iter()
            .position(|&c| s.cards[c as usize].id == card::STRIKE);
        let mut s2 = s;
        if st.is_none() {
            s2.hand[0] = (0..s2.n_cards).find(|&i| s2.cards[i as usize].id == card::STRIKE).unwrap();
        }
        let h = s2.hand[..s2.n_hand as usize]
            .iter()
            .position(|&c| s2.cards[c as usize].id == card::STRIKE)
            .expect("手上该有打击") as u8;
        let s2 = step(s2, Action::PlayCard { hand: h, target: 0 });
        assert_eq!(s2.player.get(St::FacingRight), 0, "打完有目标的牌该面朝左");
        let front = crate::end_turn_with_live_incoming(s2, &inc);
        assert_eq!(s2.player.hp - front.player.hp, 14, "正面挨打就是 14");
    }

    /// 现算口径：**我这一手改了敌人打多少，叶子要算得进去**。
    ///
    /// 凌虐（−10 力量）是最干净的例子。冻住的口径下它是空操作 ——
    /// 2026-08-27 第 2 幕 Boss 那一场求解器因此报「必死」，而活线是存在的。
    #[test]
    fn live_threat_sees_mangle_cutting_the_incoming_hit() {
        let mut s = State::new(80, 43);
        s.add_enemy(enemy::DUMMY, 300);
        s.add_card(card::TORTURE, 0, 0);
        for _ in 0..6 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.enemies[0].set(St::Strength, 12);
        s.energy = 9;
        let m = (0..s.n_cards).find(|&i| s.cards[i as usize].id == card::TORTURE).unwrap();
        s.hand[0] = m;

        let mut inc = [(0, 0); crate::state::MAX_ENEMIES];
        inc[0] = (10, 1); // 面板 10，加上 12 力量 = 22

        // 不打凌虐：22 点
        let plain = crate::end_turn_with_live_incoming(s, &inc);
        assert_eq!(s.player.hp - plain.player.hp, 22, "10 + 力量12");

        // 打凌虐：力量 12 -> 2，这一击变 12
        let after = step(s, Action::PlayCard { hand: 0, target: 0 });
        let hp_before = after.player.hp;
        let mangled = crate::end_turn_with_live_incoming(after, &inc);
        assert_eq!(hp_before - mangled.player.hp, 12, "凌虐削掉 10 点力量");
    }

    /// 蟹之怒：**盟友死亡时**，活着的持有者 +6 力量 +99 格挡。
    ///
    /// [实测] 2026-08-27 第 2 幕 Boss：碾碎爪死的那一帧，火箭 `力量 2 -> 8`、
    /// `格挡 0 -> 99`。**死掉的那只不该给自己发**（它已经不在场上）。
    #[test]
    fn crab_rage_fires_on_ally_death_only_for_survivors() {
        let mut s = State::new(80, 47);
        s.add_enemy(enemy::DUMMY, 6);
        s.add_enemy(enemy::DUMMY, 200);
        s.add_card(card::STRIKE, 0, 0);
        for _ in 0..6 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.enemies[0].set(St::CrabRage, 1);
        s.enemies[1].set(St::CrabRage, 1);
        s.enemies[1].set(St::Strength, 2);
        s.energy = 9;
        let st = (0..s.n_cards).find(|&i| s.cards[i as usize].id == card::STRIKE).unwrap();
        s.hand[0] = st;
        let s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert!(!s.enemies[0].alive(), "6 血的那只该被 6 点打击打死");
        assert_eq!(s.enemies[1].get(St::Strength), 8, "活着的那只 +6 力量");
        assert_eq!(s.enemies[1].block, 99, "并且拿到 99 格挡");
    }

    /// 凌虐「敌人在本回合失去10点力量」：**只管这一回合**，敌人的回合结束就还回去。
    ///
    /// [源码] `ManglePower : TemporaryStrengthPower`，
    /// `AfterSideTurnEnd` 里 `Remove(this)` + `Apply<StrengthPower>(-Sign * Amount)`。
    /// 也就是和黑暗镣铐**同一套机器**（`TempStrength` + `strip_temp_strength`）。
    ///
    /// 这条测试是 2026-08-27 补的，起因是我在 verification-log 里写了一条
    /// **「内核把它当成永久，偏乐观」**——那是**错的**：我读了 `state.rs` 上
    /// 「`TempStrength` 只做玩家侧」那句旧注释就下了结论，没去看 `step.rs`
    /// 里 `strip_temp_strength` 是对玩家**和每一只敌人**都跑的。
    /// 教训和内核里那些坑同构：**注释也是"假设"这一档，代码才是事实。**
    #[test]
    fn mangle_strength_loss_is_returned_at_end_of_enemy_turn() {
        let mut s = State::new(80, 31);
        s.add_enemy(enemy::DUMMY, 300);
        s.add_card(card::TORTURE, 0, 0);
        for _ in 0..8 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        // 敌人先有 12 点力量，好把"扣了"和"还了"分得开
        s.enemies[0].set(St::Strength, 12);
        let m = (0..s.n_cards).find(|&i| s.cards[i as usize].id == card::TORTURE)
            .expect("牌组里该有凌虐");
        if !s.hand[..s.n_hand as usize].contains(&m) {
            s.hand[0] = m;
        }
        let h = s.hand[..s.n_hand as usize].iter().position(|&c| c == m).expect("手上该有") as u8;
        s.energy = 9;
        let s = step(s, Action::PlayCard { hand: h, target: 0 });
        assert_eq!(s.enemies[0].get(St::Strength), 2, "打出当回合该是 12 − 10");

        // 敌人打完这一手、回合交替之后：还回去
        let after = crate::end_turn_with_incoming(s, &[(0, 0); crate::state::MAX_ENEMIES]);
        assert_eq!(after.enemies[0].get(St::Strength), 12, "敌人回合结束该还回 10 点");
        assert_eq!(after.enemies[0].get(St::TempStrength), 0, "临时量该清干净");
    }

    /// 至亮之焰：+2 能量、抽 2、失去 1 点**最大**生命，
    /// 而那 1 点最大生命**是伤害** —— 所以它会触发百年积木。
    ///
    /// [源码] `CreatureCmd.LoseMaxHp`：当前血高于新上限时走一次
    /// `Damage(..., Unblockable | Unpowered | Move)`。这条不是推的：
    /// 2026-08-27 第 2 幕第 19 层实测，手牌 5 -> 9（-1 打出、+2 抽、+3 积木），
    /// 而当天那版内核没触发，晚了一张牌才发作，是个真 MISMATCH。
    #[test]
    fn brightest_flame_loses_max_hp_as_damage_and_wakes_centennial_puzzle() {
        let mut s = State::new(80, 29);
        s.add_enemy(enemy::DUMMY, 300);
        s.add_card(card::BRIGHTEST_FLAME, 0, 0);
        for _ in 0..15 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        s.player.set(St::CentennialPuzzle, 3);
        let bf = (0..s.n_cards).find(|&i| s.cards[i as usize].id == card::BRIGHTEST_FLAME)
            .expect("牌组里该有至亮之焰");
        if !s.hand[..s.n_hand as usize].contains(&bf) {
            s.hand[0] = bf;
        }
        let h = s.hand[..s.n_hand as usize].iter().position(|&c| c == bf).expect("手上该有") as u8;
        let before = s.n_hand;
        let e0 = s.energy;
        let s = step(s, Action::PlayCard { hand: h, target: 0 });
        assert_eq!(s.energy, e0 + 2, "该 +2 能量（0 费牌，不扣）");
        assert_eq!(s.player.max_hp, 79, "最大生命 -1");
        assert_eq!(s.player.hp, 79, "当前血跟着降到新上限");
        assert_eq!(s.n_hand, before - 1 + 2 + 3, "抽 2 + 百年积木的 3");
        assert_eq!(s.player.get(St::CentennialPuzzle), 0, "积木该被这 1 点最大生命唤醒");
    }

    /// 百年积木：**卡牌的"失去生命"也算掉血**，照样抽 3。
    ///
    /// 这条是 2026-08-27 第 1 幕 Boss 实测纠正的：当天那版内核只在
    /// `take_attack_hit`（挨敌人打）那条路上点火，打出放血时游戏手牌 6 / 内核 3，
    /// 差的正好是这 3 张。[源码] `Bloodletting.OnPlay` 走的是
    /// `CreatureCmd.Damage(..., Unblockable | Unpowered | Move, this)` ——
    /// **它是伤害，只是不可格挡**，所以 `AfterDamageReceived` 照样触发。
    ///
    /// 测的是那次修复的**收口**（`step::after_player_hp_lost`）：
    /// 一场只发作一次这条已经被另一个测试守着，这里只钉"这条路也算"。
    #[test]
    fn centennial_puzzle_also_fires_on_card_hp_loss() {
        let mut s = State::new(80, 29);
        s.add_enemy(enemy::DUMMY, 300);
        s.add_card(card::BLOODLETTING, 0, 0);
        for _ in 0..15 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        s.player.set(St::CentennialPuzzle, 3);
        // 把放血挪到手上（起手抽的是哪几张不重要，这里要的是"打出它"）
        let bl = (0..s.n_cards).find(|&i| s.cards[i as usize].id == card::BLOODLETTING)
            .expect("牌组里该有放血");
        if !s.hand[..s.n_hand as usize].contains(&bl) {
            s.hand[0] = bl;
        }
        let h = s.hand[..s.n_hand as usize].iter().position(|&c| c == bl).expect("手上该有放血") as u8;
        let before = s.n_hand;
        let s = step(s, Action::PlayCard { hand: h, target: 0 });
        // 打出去 -1 张，百年积木 +3 张
        assert_eq!(s.n_hand, before - 1 + 3, "放血掉的血该触发百年积木抽 3 张");
        assert_eq!(s.player.get(St::CentennialPuzzle), 0, "触发过就该清掉");
    }

    /// 瓶装潜能：**手牌和弃牌堆一起并进抽牌堆**，洗完再抽 5。
    ///
    /// 判据不是"抽到了什么"（顺序是 RNG 的一个样本），而是三条不变量：
    /// 牌一张都没丢、弃牌堆被清空、抽完之后手上正好 5 张。
    /// 「弃牌堆也进去」这一条是这瓶药水唯一容易写错的地方 ——
    /// 卡面只说"你的所有牌"，是 [源码] 的 `CardPileCmd.Shuffle` 把它钉死的。
    #[test]
    fn bottled_potential_folds_hand_and_discard_into_draw_then_draws_five() {
        let mut s = State::new(80, 29);
        s.add_enemy(enemy::DUMMY, 300);
        for _ in 0..12 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        // 弃牌堆里先放两张，好把「弃牌堆也进去」和「只有手牌进去」分开
        for _ in 0..2 {
            let c = s.pop_draw_top().expect("抽牌堆该还有牌");
            s.to_discard(c);
        }
        let total = s.n_hand as i32 + s.n_draw as i32 + s.n_disc as i32;
        assert!(s.n_disc >= 2 && s.n_hand > 0, "前提没摆好");
        s.potions[0] = crate::state::potion::BOTTLED_POTENTIAL;
        let s = step(s, Action::UsePotion { slot: 0, target: 0 });
        assert_eq!(s.n_hand, 5, "抽完该正好 5 张");
        assert_eq!(s.n_disc, 0, "弃牌堆该被并进抽牌堆");
        assert_eq!(
            s.n_hand as i32 + s.n_draw as i32 + s.n_disc as i32,
            total,
            "牌的总数不该变（消耗堆不参与）"
        );
        assert_eq!(s.n_draw_known, 0, "洗过之后没有一张是确定的");
    }

    /// **求解器策略必须把子选择闭掉 —— 包括它自己这一回合开出来的那个。**
    ///
    /// 这条是 2026-08-25 换策略时踩的坑，而且**第一版测试没测到点子上**：
    /// 那一版只在回合开头摆一个开着的子选择，进门那次 `close_pending` 就闭掉了，
    /// 把出口那次删掉照样绿。真正的失效模式是**搜索选出来的线以一个开着的
    /// 子选择结尾**（烙印那类牌立了标记，而"就停在这儿"恰好是最高分）。
    ///
    /// `Pending != None` 时 `legal_actions` 只生成 `Choose`，`EndTurn` 是非法
    /// 动作，而 `step` 对非法动作**原样返回**。策略留着一个开着的子选择出去，
    /// `rollout_outcome_with` 的主循环就会**空转到回合上限**：每圈什么都不做、
    /// `turns` 照加，最后报一个「没打完」。
    ///
    /// 当时的症状是 P4 从 0/8320 变成 34/8320，而且**把上限从 40 抬到 300，
    /// 那些数字一个字都不变** —— 一场真的打不完的仗会随上限变化，空转不会。
    #[test]
    fn solver_policy_never_leaves_a_pending_open() {
        use super::rollout::{play_turn_rec, Policy};

        // (a) 回合开头就带着一个开着的子选择（上一回合留下来的）
        let mut s = hand_of(5, 200, &[card::DEFEND, card::STRIKE, card::STRIKE], 3);
        s.pending = Pending::ExhaustFromHand { remaining: 1 };
        let mut st = s;
        play_turn_rec(&mut st, Policy::default(), &mut None);
        assert_eq!(st.pending, Pending::None, "进门那次没闭上");
        assert_ne!(step(st, Action::EndTurn), st, "`EndTurn` 必须真的推动局面");

        // (b) **子选择是策略自己这一回合开出来的** —— 这才是真正的失效模式。
        // 逐个试那几张会立 `Pending` 的牌，每一张都必须以"闭着"结束。
        for &c in &[card::BRAND, card::ARMAMENTS, card::HEADBUTT, card::TRUE_GRIT] {
            for seed in 1..=6u64 {
                let s = hand_of(seed, 300, &[c, card::STRIKE, card::DEFEND, card::DEFEND], 3);
                let mut st = s;
                play_turn_rec(&mut st, Policy::default(), &mut None);
                assert_eq!(
                    st.pending,
                    Pending::None,
                    "打完 {} 之后子选择还开着（seed {seed}）—— 推演会在这里空转到回合上限",
                    crate::content::card(c).name
                );
                assert_ne!(
                    step(st, Action::EndTurn),
                    st,
                    "`EndTurn` 走不动（{} / seed {seed}）",
                    crate::content::card(c).name
                );
            }
        }
    }

    /// 空转和"真的打不完"分得开：**抬高回合上限，真打不完的仗数字会变，空转不会。**
    ///
    /// 这里把上限从 8 抬到 40，一个打得完的局面必须在两种上限下都不被截断。
    #[test]
    fn a_finishable_fight_is_not_truncated_at_any_cap() {
        use super::rollout::{rollout_outcome_with, Policy};
        let s = hand_of(7, 20, &[card::STRIKE, card::STRIKE, card::DEFEND], 3);
        for cap in [8usize, 40, 120] {
            let o = rollout_outcome_with(s, cap, Policy::default());
            assert!(!o.truncated, "上限 {cap} 时被截断了：{o:?}");
            assert!(o.won, "这个局面该打得赢：{o:?}");
        }
    }

    /// **战斗已经结束之后，`close_pending` 必须立刻返回。**
    ///
    /// 这条是 2026-08-25 换策略时挂死的根因，而且**前两版测试都没测到点子上**：
    /// 一版在回合开头摆子选择（进门那次就闭掉了），一版猜是牌堆满了
    /// （`n_draw == MAX_CARDS` 和 `n_disc > 0` 其实**同时成立不了** ——
    /// 牌总共就 `MAX_CARDS` 张）。真正的原因是
    /// **`step` 在 `combat_over` 之后对任何动作都原样返回**：
    /// 打出最后一张牌的同时立了个标记，出口那次 `close_pending` 就永远转下去。
    ///
    /// 实测那一帧：`DiscardToDrawTop { remaining: 1 }`，
    /// `n_hand=4 n_disc=7 n_draw=3` —— 局面完全正常，就是仗已经打完了。
    ///
    /// **它失效的样子是"测试跑不完"而不是"测试报错"**，因为那是个死循环。
    #[test]
    fn close_pending_returns_once_the_fight_is_over() {
        let mut s = hand_of(3, 200, &[card::STRIKE], 3);
        let c = s.add_card(card::DEFEND, 0, 0);
        s.to_discard(c);
        s.pending = Pending::DiscardToDrawTop { remaining: 1, exclude: u8::MAX };
        s.combat_over = true;

        // 挂死的话这里根本回不来。回得来就只剩一件事要判：它没有硬造局面。
        let before = s;
        super::rollout::close_pending(&mut s, &mut None);
        assert_eq!(s, before, "仗打完了就该原样返回，不该再动局面");
    }

    /// **两张只有费用不同的牌，指纹必须不同。**
    ///
    /// 2026-08-25 把指纹换成 Zobrist 风格时**顺带修的一个真错**：
    /// 老的 `card_key` 只揉 id / flags / bonus，**漏了 `cost_delta`**。
    /// 于是"手上这张狂乱逃离已经涨到 2 费"和"它还是 1 费"哈希相同，
    /// 搜索会把两个局面合并 —— 一个能打得起、一个打不起，合并掉就是剪错枝。
    ///
    /// 这个洞在 `cost_delta` 出现之前不存在（那时它恒为 0），
    /// 是狂乱逃离和异蛇之油进内核之后才变得可达的。
    #[test]
    fn cost_delta_is_part_of_the_state_fingerprint() {
        let a = hand_of(1, 100, &[card::STRIKE, card::DEFEND], 3);
        let mut b = a;
        b.cards[b.hand[0] as usize].cost_delta = 1;
        assert_ne!(
            super::solver::key(&a),
            super::solver::key(&b),
            "只有费用不同的两个局面被当成了同一个"
        );
    }

    /// 多重集是**可交换**的，但**不许抵消**。
    ///
    /// 用异或做多重集组合会让相同的两张牌互相消掉（一手两张打击和零张打击
    /// 哈希相同），多重数就丢了。这里两条一起钉：换手牌顺序指纹不变、
    /// 多一张同名牌指纹要变。
    #[test]
    fn hand_is_a_multiset_that_does_not_cancel() {
        let a = hand_of(2, 100, &[card::STRIKE, card::DEFEND, card::BASH], 3);
        let mut swapped = a;
        swapped.hand.swap(0, 2);
        assert_eq!(super::solver::key(&a), super::solver::key(&swapped), "手牌顺序不该影响指纹");

        // **两手牌张数相同、各自是一对同名牌** —— 异或组合下两边都抵消成 0，
        // 而 `n_hand` 也一样，于是指纹会撞。加法组合分得开。
        // （第一版这里拿"两张打击 vs 空手牌"比，`n_hand` 那一项自己就把它们
        //   分开了，把异或那个突变漏了过去 —— 测试测的不是它要测的东西。）
        let pair_a = hand_of(2, 100, &[card::STRIKE, card::STRIKE], 3);
        let pair_b = hand_of(2, 100, &[card::DEFEND, card::DEFEND], 3);
        assert_ne!(
            super::solver::key(&pair_a),
            super::solver::key(&pair_b),
            "两张同名牌互相抵消了：多重集的组合方式不能用异或"
        );
    }

    /// 抽牌堆是**有序**的，顺序换了就是另一个局面。
    ///
    /// 手牌按多重集、抽牌堆按顺序，这个区分从 FNV 那一版就是对的：
    /// 回合内抽牌是按顺序发的，把抽牌堆也当多重集会让"下一张抽到什么"消失。
    #[test]
    fn draw_pile_order_is_part_of_the_fingerprint() {
        let mut a = hand_of(3, 100, &[card::STRIKE], 3);
        let s1 = a.add_card(card::DEFEND, 0, 0);
        let s2 = a.add_card(card::BASH, 0, 0);
        a.n_draw = 2;
        a.draw[0] = s1;
        a.draw[1] = s2;
        let mut b = a;
        b.draw.swap(0, 1);
        assert_ne!(super::solver::key(&a), super::solver::key(&b), "抽牌堆顺序必须进指纹");

        // 而 `draw_multiset_key`（机会节点的 CRN 种子）恰恰**不该**看顺序
        assert_eq!(
            super::solver::draw_multiset_key(&a),
            super::solver::draw_multiset_key(&b),
            "CRN 种子要的是「还没抽的是哪些牌」，不是它们排成什么顺序"
        );
    }

    // ---------------------------------------------------------------
    // 抽牌堆的「已知前缀」`n_draw_known`（2026-08-25，planner 的 S0）
    //
    // **这几个测试盯的是同一个方向的错：把不确定的牌算成确定的。**
    // 反方向（少算几张确定的）只是丢信息，planner 会保守一点；
    // 而多算一张，机会节点就会拿一个假前提去枚举 —— 不报错，只是搜错。
    // 所以每条断言都写成「顶上那几张必须真的是我放上去的那几张」。
    // ---------------------------------------------------------------

    /// 顶上的 `n_draw_known` 张，必须**确实**是被放上去的那几张。
    fn known_top_is_sane(s: &State, known: &[u8]) {
        let k = s.n_draw_known as usize;
        assert!(k <= s.n_draw as usize, "已知区比抽牌堆还长：{k} > {}", s.n_draw);
        for i in 0..k {
            let c = s.draw[s.n_draw as usize - 1 - i];
            assert!(
                known.contains(&c),
                "顶上第 {i} 张（牌 {c}）不在「放上去过」的集合里 —— 把不确定的算成了确定的"
            );
        }
    }

    /// 放到顶上的牌是确定的，而且**先放的后抽**（顶 = 数组末尾）。
    #[test]
    fn known_prefix_tracks_cards_put_on_top() {
        let mut s = hand_of(1, 100, &[card::STRIKE], 3);
        s.n_draw = 0;
        let a = s.add_card(card::DEFEND, 0, 0);
        let b = s.add_card(card::BASH, 0, 0);
        // `add_card` 是往抽牌堆里塞，不算"放到顶上"
        s.n_draw = 0;
        s.n_draw_known = 0;

        s.to_draw_top(a);
        s.to_draw_top(b);
        assert_eq!(s.n_draw_known, 2);
        known_top_is_sane(&s, &[a, b]);

        // 抽一张：拿到的是最后放上去的那张，已知区 -1
        s.draw_n(1);
        assert_eq!(s.hand[s.n_hand as usize - 1], b, "抽牌堆顶 = 最后放上去的那张");
        assert_eq!(s.n_draw_known, 1);
        s.draw_n(1);
        assert_eq!(s.hand[s.n_hand as usize - 1], a);
        assert_eq!(s.n_draw_known, 0, "抽光了就没有确定的了");
    }

    /// **洗牌把已知区清零。**
    ///
    /// 这一条今天走正常流程**碰不到**：已知的牌都在顶上，抽牌先把它们抽走，
    /// 所以抽牌堆空到要洗牌时 `n_draw_known` 早就是 0 了。
    /// 但它是 `reshuffle_discard_into_draw` 的契约的一部分（洗完顺序全变），
    /// 所以这里**直接考那个函数**，不绕正常流程 —— 第一版就是绕着考的，
    /// 把「删掉那行清零」的突变放了过去。
    #[test]
    fn reshuffling_clears_the_known_prefix() {
        let mut s = hand_of(2, 100, &[card::STRIKE], 3);
        s.n_draw = 0;
        for _ in 0..3 {
            let c = s.add_card(card::STRIKE, 0, 0);
            s.n_draw -= 1;
            s.to_discard(c);
        }
        // 人为摆出「抽牌堆空了，但还记着有确定的牌」这个局面
        s.n_draw = 0;
        s.n_draw_known = 2;
        s.reshuffle_discard_into_draw();
        assert!(s.n_draw > 0, "弃牌堆该被洗回来了");
        assert_eq!(s.n_draw_known, 0, "洗完之后不该有确定的牌");
    }

    /// **`n_draw_known` 永远不超过 `n_draw`**，而且推演跑一整场也不会破。
    ///
    /// 这条是上面那几条的兜底：那几条各盯一个入口，这条盯的是"所有入口加起来
    /// 有没有漏"。跑一场真的推演，每个回合边界都查一次。
    #[test]
    fn known_prefix_never_exceeds_the_draw_pile() {
        use super::rollout::{play_turn_rec, Policy};
        let mut s = State::new(80, 17);
        s.add_enemy(enemy::NIBBIT, 44);
        for _ in 0..12 {
            s.add_card(card::STRIKE, 0, 0);
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        for _ in 0..12 {
            if s.combat_over {
                break;
            }
            play_turn_rec(&mut s, Policy::default(), &mut None);
            assert!(
                s.n_draw_known <= s.n_draw,
                "已知区 {} > 抽牌堆 {}",
                s.n_draw_known,
                s.n_draw
            );
            s = step(s, Action::EndTurn);
            assert!(s.n_draw_known <= s.n_draw);
        }
    }

    /// 往**随机位置**插一张（噪音机器人的第二张眩晕）会把已知区截短。
    ///
    /// 插在已知区下面不受影响，插进已知区里面就只剩插入点以上那几张 ——
    /// 无论哪种，顶上那几张都必须仍然是真的确定的。
    #[test]
    fn inserting_a_random_card_shrinks_the_known_prefix() {
        for seed in 1..=40u64 {
            let mut s = hand_of(seed, 100, &[card::STRIKE], 3);
            s.n_draw = 0;
            let a = s.add_card(card::DEFEND, 0, 0);
            let b = s.add_card(card::BASH, 0, 0);
            let c = s.add_card(card::DEFEND, 0, 0);
            s.n_draw = 0;
            s.to_draw_top(a);
            s.to_draw_top(b);
            s.to_draw_top(c);
            assert_eq!(s.n_draw_known, 3);

            let junk = s.add_card(card::WOUND, 0, 0);
            s.n_draw -= 1; // 抵消 `add_card` 那次推进
            s.to_draw_random(junk);
            known_top_is_sane(&s, &[a, b, c]);
        }
    }

    /// `begin_combat` 洗完牌，一张确定的都不该剩。
    #[test]
    fn begin_combat_clears_the_known_prefix() {
        let mut s = State::new(80, 9);
        s.add_enemy(enemy::DUMMY, 100);
        for _ in 0..10 {
            s.add_card(card::STRIKE, 0, 0);
        }
        s.n_draw_known = 5; // 硬摆一个，看它清不清
        let s = begin_combat(s);
        assert_eq!(s.n_draw_known, 0, "开局洗牌之后不该有确定的牌");
    }

    /// **`sync` 之后必然是 0。**
    ///
    /// 观测里的抽牌堆是 mod 排过序的（trace-format 的约束 2），
    /// 顺序本来就丢了，一张确定的都没有。这一条要是漏了，
    /// planner 会在每个同步点凭空捡到一把"确定的牌"。
    #[test]
    fn sync_reports_no_known_prefix() {
        let src = match std::fs::read_to_string("traces/act1_f4_second.json") {
            Ok(s) => s,
            Err(_) => return,
        };
        let t = super::replay::parse_trace(&src).unwrap();
        let mut r = super::replay::Replayer::for_trace(&t);
        for f in t.frames.iter().take(4) {
            let sy = r.sync(&f.obs);
            assert_eq!(sy.state.n_draw_known, 0, "同步出来的局面不该带已知前缀");
        }
    }

    // ---------------------------------------------------------------
    // 跨回合 planner 的地基（`plan.rs`，2026-08-25 的 S0）
    //
    // 每条测试先想"它坏掉长什么样"，再写能看见那个样子的断言 ——
    // 这一轮已经栽过三次（凋萎的顺序、`close_pending` 的失效模式、
    // 多重集用异或），三次都是第一版测了别的东西。
    // ---------------------------------------------------------------

    /// 回合边界拆成两半之后，**拼回去必须逐字节等于原来那一步**。
    ///
    /// 它坏掉的样子：`end_turn_before_draw` 漏做了 `check_over` 或者把某段
    /// 结算做了两遍，于是 planner 搜的是一个和 `step(EndTurn)` 不一样的游戏
    /// —— 而 `step(EndTurn)` 才是被 36 条实录对拍验过的那条路。
    #[test]
    fn end_turn_split_equals_the_atomic_one() {
        for seed in 1..=25u64 {
            let mut s = State::new(80, seed);
            s.add_enemy(enemy::NIBBIT, 44);
            for _ in 0..8 {
                s.add_card(card::STRIKE, 0, 0);
                s.add_card(card::DEFEND, 0, 0);
            }
            let mut s = begin_combat(s);
            // 打两张牌再结束，让局面别太平凡
            if s.n_hand > 0 {
                s = step(s, Action::PlayCard { hand: 0, target: 0 });
            }
            let atomic = step(s, Action::EndTurn);
            let mut split = crate::step::end_turn_before_draw(s);
            crate::step::open_hand(&mut split);
            assert_eq!(atomic, split, "seed {seed}：拆开再拼回去和原子的那一步不一样");
        }
    }

    /// **深层选线的目标函数默认必须是叶评估。**
    ///
    /// expectimax 的规矩：决策节点取"后继子树价值"的最大值，而最后一层的后继
    /// 就是叶子。2026-09-04 之前这里是 `cfg.score`（`damage_first`），
    /// 于是最后一层的 argmax 取的是**另一个函数**的最大值。
    ///
    /// 这条测试只钉默认值 —— "换回去会怎样"是 `bin/rollout --alt` 的活，
    /// "换过来对不对"是 `bin/plan_audit` 的活。**这里只保证没人把默认悄悄改回去。**
    #[test]
    fn deep_score_defaults_to_the_leaf_objective() {
        use crate::plan::Plan;
        use crate::solver::score;
        let cfg = Plan::default();
        // 函数指针先显式转一次再比地址：直接拿函数项转 usize 比的是零大小类型
        let want: fn(&crate::state::State) -> i32 = score::leaf;
        assert_eq!(
            cfg.deep_score as usize, want as usize,
            "深层选线的目标函数该是 score::leaf（和 cfg.leaf 同口径），见 Plan::deep_score"
        );
        assert_ne!(
            cfg.deep_score as usize, cfg.score as usize,
            "deep_score 和 score 相等就等于退回 2026-09-04 之前 —— \
             要 A/B 请用 `--alt \"deep-score=damage\"`，别改默认值"
        );
    }

    /// **`deep_score` 一个字都不许影响 D=1。**
    ///
    /// `plan_line_with_threat` 在 `depth <= 1` 时提前 return、走的是 `cfg.score`，
    /// 所以 `plan_depth_one_is_exactly_solve_turn` 那条门不受影响 ——
    /// 但那条测试用的是**默认配置**，改错了它不一定红。这一条专门盯着：
    /// 把 `deep_score` 换成一个明显不同的函数，D=1 的线必须逐字不变。
    #[test]
    fn deep_score_does_not_touch_depth_one() {
        use crate::plan::{plan_line, Plan};
        use crate::solver::{explain, score};
        for seed in 1..=15u64 {
            let s = hand_of(seed, 120, &[card::STRIKE, card::DEFEND, card::BASH], 3);
            let base = Plan::default();
            let mut alt = base;
            alt.deep_score = score::hp_only;
            assert_eq!(
                explain(&s, plan_line(&s, &base).acts()),
                explain(&s, plan_line(&s, &alt).acts()),
                "seed {seed}：D=1 的线被 deep_score 改掉了 —— \
                 说明有人在 depth<=1 那条路上读了它"
            );
        }
    }

    /// **窗口目标函数的默认值不许悄悄改回去**，而且它和叶评估**只该差 `win`**。
    ///
    /// 第二条断言是这一整条改动的全部内容：`Weights::WINDOW` 要是还差了别的项，
    /// "窗口内换尺子"就不再是"拿掉一个地平线人造物"，而是一次没标定过的换权重。
    #[test]
    fn the_window_objective_is_the_leaf_objective_minus_the_win_constant() {
        use crate::plan::Plan;
        use crate::solver::{score, Weights};
        let cfg = Plan::default();
        let want: fn(&crate::state::State) -> i32 = score::window;
        assert_eq!(
            cfg.window_score as usize, want as usize,
            "窗口内的目标函数该是 score::window（最终血量口径），见 Plan::window_score"
        );
        assert_eq!(Weights::WINDOW.win, 0, "WINDOW 的 win 不是 0，那 1000 血的台阶还在");
        let (w, l) = (&Weights::WINDOW, &Weights::LEAF);
        assert_eq!(
            (w.hp, w.enemy_hp, w.death, w.enemy_vuln, w.enemy_weak, w.strength, w.power),
            (l.hp, l.enemy_hp, l.death, l.enemy_vuln, l.enemy_weak, l.strength, l.power),
            "WINDOW 和 LEAF 除了 win 之外还差了别的项 —— 那就不是「拿掉地平线人造物」了"
        );
    }

    /// **两把尺子只在"赢下来的终局"上不同。**
    ///
    /// 这条把上面那个权重断言翻译成行为：仗还在打 ⇒ 逐字相等；赢了 ⇒ 差恰好一个
    /// `Weights::LEAF.win`。`plan_audit` 报告里"不含 win 那一列是口径中立的"
    /// 这句话，全部依据就是这一条。
    #[test]
    fn the_two_rulers_differ_only_on_a_won_terminal() {
        use crate::solver::{score, Weights};
        for seed in 1..=15u64 {
            let s = hand_of(seed, 120, &[card::STRIKE, card::DEFEND, card::BASH], 3);
            assert!(!s.combat_over, "场景立不住：仗还没打完才对");
            assert_eq!(
                score::window(&s),
                score::leaf(&s),
                "seed {seed}：仗还在打，两把尺子却给了不同的分"
            );
            // 同一个局面，把敌人打没
            let mut won = s;
            won.enemies[0].hp = 0;
            won.combat_over = true;
            assert_eq!(
                score::leaf(&won) - score::window(&won),
                Weights::LEAF.win,
                "seed {seed}：赢下来那一刻两把尺子的差不是一个 win"
            );
        }
    }

    /// **`window_score` 一个字都不许影响 D=1。**
    ///
    /// 和 `deep_score_does_not_touch_depth_one` 同一类：`plan_line_with_threat`
    /// 在 `depth <= 1` 时提前 return、走 `cfg.score`，所以换掉窗口目标函数
    /// 不该动实战驱动那条路径（**驾驶用的就是 D=1**）。
    #[test]
    fn window_score_does_not_touch_depth_one() {
        use crate::plan::{plan_line, Plan};
        use crate::solver::{explain, score};
        for seed in 1..=15u64 {
            let s = hand_of(seed, 120, &[card::STRIKE, card::DEFEND, card::BASH], 3);
            let base = Plan::default();
            let mut alt = base;
            alt.window_score = score::hp_only;
            assert_eq!(
                explain(&s, plan_line(&s, &base).acts()),
                explain(&s, plan_line(&s, &alt).acts()),
                "seed {seed}：D=1 的线被 window_score 改掉了"
            );
        }
    }

    /// **尺子是每次搜索一把，判据是窄根**（[`crate::plan::Plan::at_root`]）。
    ///
    /// 三件事一起钉：窄根上深层选线和叶评估**一起**换掉（只换一半就是筛子和尺子
    /// 分家）· 非窄根上一个字不动 · `window-score=leaf` 把整件事变回恒等。
    ///
    /// **坏掉的样子**：逐叶子挑尺子。那时同一次搜索里"确定赢的那条线"用
    /// `win = 0` 的尺子、"采样分支里侥幸赢的那条线"用 `win = 100000` 的尺子，
    /// 事实输给运气 —— 这条测试拦不住那种写法，但 `at_root` 的文档解释了为什么
    /// 挑法必须收口在这一个函数里，而这条测试钉着它就是唯一的入口。
    #[test]
    fn the_window_ruler_is_picked_once_per_search_and_only_on_a_narrow_root() {
        use crate::plan::{root_is_narrow, Leaf, Plan};
        use crate::solver::score;
        let cfg = Plan::default();

        // 窄根：整堆牌序已知（`draw_pile_order` 补丁之后 `sync` 出来的样子）
        let mut narrow = short_draw_pile_scene(8, 4);
        narrow.n_draw_known = narrow.n_draw;
        assert!(root_is_narrow(&narrow), "场景立不住：这该是个窄根");
        let eff = cfg.at_root(&narrow);
        assert_eq!(
            eff.deep_score as usize, cfg.window_score as usize,
            "窄根上深层选线没换成窗口目标函数"
        );
        match eff.leaf {
            Leaf::Eval(f) => assert_eq!(
                f as usize, cfg.window_score as usize,
                "窄根上叶评估没跟着换 —— 筛子和尺子分家了"
            ),
            other => panic!("默认配置的叶评估该是 Leaf::Eval，收到 {other:?}"),
        }

        // 非窄根：牌序未知，一个字都不该动
        let wide = short_draw_pile_scene(8, 4);
        assert!(!root_is_narrow(&wide), "场景立不住：这不该是窄根");
        let same = cfg.at_root(&wide);
        assert_eq!(same.deep_score as usize, cfg.deep_score as usize, "非窄根上尺子被换了");
        match (same.leaf, cfg.leaf) {
            (Leaf::Eval(a), Leaf::Eval(b)) => {
                assert_eq!(a as usize, b as usize, "非窄根上叶评估被换了")
            }
            _ => panic!("默认配置的叶评估该是 Leaf::Eval"),
        }

        // `window-score=leaf` ⇒ 整件事是恒等映射（A/B 的退回通道）
        let mut off = cfg;
        off.window_score = score::leaf;
        let rev = off.at_root(&narrow);
        assert_eq!(
            rev.deep_score as usize, off.deep_score as usize,
            "window-score=leaf 该逐字退回，深层选线却变了"
        );
        match (rev.leaf, off.leaf) {
            (Leaf::Eval(a), Leaf::Eval(b)) => {
                assert_eq!(a as usize, b as usize, "window-score=leaf 该逐字退回，叶评估却变了")
            }
            _ => panic!("默认配置的叶评估该是 Leaf::Eval"),
        }
    }

    /// **D=1 的 planner 必须逐字等于 `solve_turn`。**
    ///
    /// 这是整个重构最强的回归锚点：搜索框架长歪了、威胁提供者接错了、
    /// 目标函数传错了，这一条都会红。
    #[test]
    fn plan_depth_one_is_exactly_solve_turn() {
        use crate::plan::{plan_line, Plan};
        use crate::rollout::predicted_threat;
        use crate::solver::{explain, solve_turn};
        let cfg = Plan::default();
        for seed in 1..=15u64 {
            let s = hand_of(seed, 120, &[card::STRIKE, card::DEFEND, card::BASH], 3);
            let want = solve_turn(&s, &predicted_threat(&s), cfg.score);
            let got = plan_line(&s, &cfg);
            assert_eq!(
                explain(&s, want.acts()),
                explain(&s, got.acts()),
                "seed {seed}：D=1 和 solve_turn 给出了不同的线"
            );
            assert_eq!(want.score, got.score, "seed {seed}：分数也该一样");
        }
    }

    /// planner **不许喝药水**（搜索里药水是免费的，会被一口气喝光）。
    ///
    /// 它坏掉的样子：把 `allowed_potions` 从 0 改成 `ALL_POTIONS`，
    /// 于是推演第 1 回合就清仓。上面那条 D=1 的测试**看不见**这个
    /// （那些局面身上没药水），所以要单独摆一个有药水的局面。
    #[test]
    fn planner_never_drinks_potions() {
        use crate::plan::{plan_line, Plan};
        use crate::rollout::predicted_threat;
        use crate::solver::{score, solve_turn};
        let mut s = hand_of(3, 200, &[card::STRIKE, card::DEFEND], 3);
        s.potions[0] = crate::state::potion::BLOCK;
        s.potions[1] = crate::state::potion::FIRE;
        s.potion_slots = 3;

        let line = plan_line(&s, &Plan::default());
        assert!(
            !line.acts().iter().any(|a| matches!(a, Action::UsePotion { .. })),
            "planner 的线里出现了药水"
        );
        // 对照：同一个局面，允许喝药水的 `solve_turn` 是会喝的 ——
        // 证明这个局面本来就有得喝，上面那条断言不是因为没药水才过的
        let open = solve_turn(&s, &predicted_threat(&s), score::damage_first);
        assert!(
            open.acts().iter().any(|a| matches!(a, Action::UsePotion { .. })),
            "对照组没喝药水，这个局面挑得不好，换一个"
        );
    }

    /// 一条线里打没打出能力牌。**和求解器用的是同一张数据表**（`CardDef.kind`），
    /// 不按牌名判 —— 换一张别的能力牌进手牌，这个判据不用改。
    fn line_plays_power(s: &State, line: &crate::solver::Line) -> bool {
        let mut st = *s;
        for &a in line.acts() {
            if let Action::PlayCard { hand, .. } = a {
                let i = hand as usize;
                if i < st.n_hand as usize
                    && card(st.cards[st.hand[i] as usize].id).kind == crate::ops::Kind::Power
                {
                    return true;
                }
            }
            st = step(st, a);
        }
        false
    }

    /// 手牌里有能力牌、单回合分数又必然垫底的局面。
    ///
    /// 薪火之源 2 费、当回合 0 伤害 0 格挡；3 费的预算里还有 3 张打击 2 张防御，
    /// 于是"打薪火之源"的每一条线都被一堆普通线严格压住。
    fn pyre_hand() -> (State, crate::solver::Threat) {
        let s = hand_of(
            9,
            200,
            &[card::PYRE, card::STRIKE, card::STRIKE, card::STRIKE, card::DEFEND, card::DEFEND],
            3,
        );
        // 威胁显式给，不走 `predicted_threat` —— 格挡值不值钱得由这个数说了算，
        // 让它跟着某只敌人的出招表漂移的话，这个测试就变成在测那张表了。
        let mut threat = crate::solver::Threat::new();
        threat.set(0, 9, 1);
        (s, threat)
    }

    /// **能力线保底必须是纯增量**：主表那几条一条不少、顺序一个字不动。
    ///
    /// 这是整个改动的可归因性所在 —— 保底要是会挤掉普通候选，P5 差出来的
    /// 就是"多看了能力线"和"少看了普通线"的合计，归不了因。
    #[test]
    fn power_reserve_only_adds_candidates_never_reorders() {
        use crate::solver::{score, solve_turn_topk_split};
        let (s, threat) = pyre_hand();
        let (base, base_rescued, _) =
            solve_turn_topk_split(&s, &threat, score::damage_first, 200_000, 0, 6, 0, 0);
        assert!(base_rescued.is_empty(), "reserve=0 不该捞回任何线");
        let (main, _, _) =
            solve_turn_topk_split(&s, &threat, score::damage_first, 200_000, 0, 6, 2, 0);
        assert_eq!(base, main, "开了保底之后主表被动过了 —— 保底必须是纯增量");
    }

    /// **保底真的捞回了一条本来会被剪掉的能力线。**
    ///
    /// 它坏掉的样子就是这个改动之前的样子：打出薪火之源的线在根上按单回合
    /// 分数排到 K 条之外，深层搜索和叶评估**根本见不到它** ——
    /// 那时候光去修叶评估是够不着的，修好的叶子评不到一条不存在的候选线。
    #[test]
    fn power_reserve_rescues_a_line_the_root_pruning_dropped() {
        use crate::solver::{score, solve_turn_topk_split};
        let (s, threat) = pyre_hand();

        // 对照组：不开保底，候选里**一条能力线都没有**。
        // 这一条要是红了，说明这个局面挑得不好（能力线自己就挤进去了），
        // 那下面那条断言即使绿了也证明不了保底有用 —— 换个局面，别改断言。
        let (base, _, _) =
            solve_turn_topk_split(&s, &threat, score::damage_first, 200_000, 0, 6, 0, 0);
        assert!(
            !base.iter().any(|l| line_plays_power(&s, l)),
            "对照组失效：不开保底它自己就进了候选，这个局面测不出保底的作用，换一个"
        );

        let (_, rescued, _) =
            solve_turn_topk_split(&s, &threat, score::damage_first, 200_000, 0, 6, 2, 0);
        assert!(!rescued.is_empty(), "保底一条都没捞回来");
        assert!(
            rescued.iter().all(|l| line_plays_power(&s, l)),
            "保底段里混进了不打能力牌的线 —— 那个名额是专款专用的"
        );
    }

    /// 手牌里有高伤害攻击牌、但在单回合防守评分下会被防守线挤掉的局面。
    fn heavy_attack_hand() -> (State, crate::solver::Threat) {
        let s = hand_of(
            15,
            200,
            &[
                card::STRIKE,
                card::STRIKE,
                card::STRIKE,
                card::DEFEND,
                card::DEFEND,
                card::DEFEND,
            ],
            3,
        );
        let mut threat = crate::solver::Threat::new();
        threat.set(0, 15, 1);
        (s, threat)
    }

    /// **伤害线保底必须是纯增量**：主表那几条一条不少、顺序一个字不动。
    #[test]
    fn damage_reserve_only_adds_candidates_never_reorders() {
        use crate::solver::{score, solve_turn_topk_split};
        let (s, threat) = heavy_attack_hand();
        let (base, _, _) =
            solve_turn_topk_split(&s, &threat, score::survive_first, 200_000, 0, 2, 0, 0);
        let (main, _, _) =
            solve_turn_topk_split(&s, &threat, score::survive_first, 200_000, 0, 2, 0, 2);
        assert_eq!(base, main, "开了伤害保底之后主表被动过了 —— 伤害保底必须是纯增量");
    }

    /// **伤害保底真的捞回了被单回合 survive 剪掉的最大伤害线。**（解决 Top-K 权重倒挂）
    #[test]
    fn damage_reserve_rescues_high_damage_line_pruned_by_survive() {
        use crate::solver::{explain, score, solve_turn_topk_split};
        let (s, threat) = heavy_attack_hand();

        // 对照组：不开伤害保底，k=2 且使用 survive_first 时，候选集里全是被防守牌主导的线
        let (base, _, _) =
            solve_turn_topk_split(&s, &threat, score::survive_first, 200_000, 0, 2, 0, 0);
        let has_pure_strike = base.iter().any(|l| {
            let names = explain(&s, l.acts());
            names.len() == 3 && names.iter().all(|n| n == "打击")
        });
        assert!(!has_pure_strike, "对照组失效：主表自己就包含了 3 打击线");

        // 实验组：开伤害保底
        let (_, _, rescued_dmg) =
            solve_turn_topk_split(&s, &threat, score::survive_first, 200_000, 0, 2, 0, 2);
        assert!(!rescued_dmg.is_empty(), "伤害保底一条都没捞回来");
        let rescued_has_pure_strike = rescued_dmg.iter().any(|l| {
            let names = explain(&s, l.acts());
            names.len() == 3 && names.iter().all(|n| n == "打击")
        });
        assert!(rescued_has_pure_strike, "伤害保底捞回来的应该包含 3 打击线");
    }

    /// **跨回合搜索中，伤害保底解决单回合与深搜的权重倒挂**
    ///
    /// 根回合单回合 survive 倾向纯防守，将打出 3 张打击的高伤害线裁掉。
    /// 有了 `damage_reserve`，伤害保底成功将高伤害线捞入候选池并完成深层估值。
    #[test]
    fn multi_turn_plan_includes_rescued_damage_line() {
        use crate::plan::{plan_candidates, Plan};
        use crate::solver::{explain, score, Threat};

        let s = hand_of(
            15,
            200,
            &[
                card::STRIKE,
                card::STRIKE,
                card::STRIKE,
                card::DEFEND,
                card::DEFEND,
                card::DEFEND,
            ],
            3,
        );
        let mut threat = Threat::new();
        threat.set(0, 15, 1);

        let mut cfg = Plan::default();
        cfg.depth = 2;
        cfg.k = 2;
        cfg.power_reserve = 0;
        cfg.damage_reserve = 0;
        cfg.score = score::survive_first; // 根剪枝用苛刻的防守评分

        // 1. 不开伤害保底：候选池里没有 3 打击线
        let (cands_no_res, _) = plan_candidates(&s, &cfg, &threat);
        let has_3_strikes_no_res = cands_no_res.iter().any(|(l, _)| {
            let names = explain(&s, l.acts());
            names.len() == 3 && names.iter().all(|n| n == "打击")
        });
        assert!(!has_3_strikes_no_res, "不开伤害保底时不应该有 3 打击线");

        // 2. 开启伤害保底：3 打击线被成功捞回并获得深层估值
        cfg.damage_reserve = 2;
        let (cands_with_res, n_main) = plan_candidates(&s, &cfg, &threat);
        assert_eq!(n_main, 2, "主表应该依然只有 2 条线");
        let has_3_strikes_with_res = cands_with_res.iter().any(|(l, val)| {
            let names = explain(&s, l.acts());
            names.len() == 3 && names.iter().all(|n| n == "打击") && *val > i32::MIN / 2
        });
        assert!(has_3_strikes_with_res, "开启伤害保底后 3 打击线应该成功进入深搜评估池");
    }

    /// 没有能力状态、也没有额外能量时，能力项必须是 **0**。
    ///
    /// 它守的是那条早退：`power_horizon_value` 要扫牌堆（`optimistic_damage`
    /// 要排序、`damage_per_energy` 要过全牌堆），比一个搜索节点贵得多。
    /// 早退坏掉不会报错，只会让每个叶子都付这笔钱。
    #[test]
    fn power_term_is_zero_without_any_power() {
        use crate::solver::{power_horizon_value, Weights};
        let s = hand_of(4, 100, &[card::STRIKE, card::DEFEND, card::STRIKE], 3);
        assert_eq!(s.base_energy, crate::solver::BASE_ENERGY, "这个局面不该自带额外能量");
        assert_eq!(power_horizon_value(&s, &Weights::LEAF), 0);
    }

    /// **薪火之源的印记层数 == 它给出去的最大能量。**
    ///
    /// [`solver::pyre_energy`] 拿印记做归因，靠的就是这条等式。
    /// 它坏掉的样子很安静：改内容表时把 `Op::Status{Pyre, n}` 和
    /// `Op::GainMaxEnergy(n)` 的两个 `n` 改得不一样，能力项就会静默算错，
    /// 而**没有任何对拍模式看得见**（那是叶评估，不是规则）。
    #[test]
    fn pyre_marker_tracks_the_energy_it_granted() {
        for upg in [false, true] {
            let mut s = hand_of(11, 300, &[card::PYRE, card::STRIKE], 3);
            if upg {
                let ix = s.hand[0] as usize;
                s.cards[ix].flags |= crate::state::F_UPGRADED;
            }
            let before = s.base_energy;
            let after = step(s, Action::PlayCard { hand: 0, target: 0 });
            assert_eq!(
                after.player.get(St::Pyre),
                after.base_energy - before,
                "薪火之源（升级={upg}）的印记层数和它给的最大能量对不上"
            );
        }
    }

    /// **额外最大能量只算归因得到薪火之源的那部分。**
    ///
    /// 这条守的是 2026-09-01 修掉的一个真 bug：`power_horizon_value` 原来拿
    /// `base_energy - BASE_ENERGY` 当"薪火之源给的能量"，而 `Replayer::sync`
    /// 是 `s.base_energy = obs.max_energy` —— **直接从观测灌**。于是身上一件
    /// 内核没建模的 +1 最大能量遗物，就会被当成打过一张薪火之源。
    ///
    /// 而这一项乘着 `horizon`：敌人血越少 `h` 越小 ⇒ 这笔凭空的加分随着我打死
    /// 敌人而缩水 ⇒ **它奖励留着敌人不杀**。和 `eval` 那个用当前敌人血量当进度
    /// 的老 bug 是同一个失效模式。
    ///
    /// 坏掉的样子：把 `pyre_energy` 换回 `s.base_energy - BASE_ENERGY`，
    /// 下面两条断言当场红。
    #[test]
    fn extra_energy_is_only_counted_when_it_can_be_attributed_to_pyre() {
        use crate::solver::{power_horizon_value, pyre_energy, Weights};
        // 一件内核没建模的 +1 最大能量遗物：sync 会把 base_energy 灌成 4，
        // 而身上一层薪火之源印记都没有。
        let mut relic = hand_of(4, 300, &[card::STRIKE, card::DEFEND, card::STRIKE], 3);
        relic.base_energy = crate::solver::BASE_ENERGY + 1;
        assert_eq!(relic.player.get(St::Pyre), 0, "这个局面不该有薪火之源印记");
        assert_eq!(pyre_energy(&relic), 0, "遗物给的能量不该被归因到薪火之源");
        assert_eq!(
            power_horizon_value(&relic, &Weights::LEAF),
            0,
            "内核看不见的遗物能量被当成能力牌计价了"
        );

        // **而且它不该改变"打死敌人是赚的"这件事。**
        // bug 版本里 h 随敌人血量缩水，杀敌反而扣分。
        let mut low = relic;
        let mut high = relic;
        for e in 0..low.n_enemies as usize {
            low.enemies[e].hp = 40;
            high.enemies[e].hp = 300;
        }
        let gain = crate::solver::score::leaf(&low) - crate::solver::score::leaf(&high);
        assert!(gain > 0, "打掉敌人 260 点血在叶评估里居然不是赚的：{gain}");

        // 对照：真的打出薪火之源时，这一项必须**照常**给分。
        let pyre = step(
            hand_of(11, 300, &[card::PYRE, card::STRIKE, card::STRIKE], 3),
            Action::PlayCard { hand: 0, target: 0 },
        );
        assert!(pyre_energy(&pyre) > 0, "真的打出薪火之源反而不计价了");
        assert!(power_horizon_value(&pyre, &Weights::LEAF) > 0);
    }

    /// **planner 的并列判据必须量在「敌人已经打完」的局面上**，和
    /// `solver::consider_stopping_here` 同一个口径。
    ///
    /// 2026-09-01 之前 planner 自己写了一遍，取的是敌人**还没动**的局面。
    /// 两个函数体逐字相同，作用的局面差一整个敌人回合 —— 有回合边界伤害
    /// （荆棘 / 火焰屏障 / 滚石 / 水银沙漏 / 历石）的场上就分岔。
    ///
    /// **老注释还把守卫认错了**：它说 `plan_depth_one_is_exactly_solve_turn`
    /// 守着，而 `plan_line_with_threat` 在 `depth <= 1` 时提前 return，
    /// 那条测试根本走不到那段代码。所以要有这一条。
    #[test]
    fn plan_tiebreak_is_measured_after_the_enemy_turn() {
        use crate::plan::tiebreak_left;
        use crate::rollout::predicted_threat;
        use crate::solver::{solve_turn, Threat};
        // 荆棘：敌人打我一下就掉血 ⇒ 敌人回合前后的"敌人剩多少血"不一样。
        let mut s = hand_of(7, 120, &[card::STRIKE, card::DEFEND], 3);
        s.player.set(St::Thorns, 5);
        let threat: Threat = predicted_threat(&s);
        let line = solve_turn(&s, &threat, crate::solver::score::damage_first);

        let after = crate::solver::replay_line(&s, line.acts()).expect("线该能重放");
        let pre = enemy_hp_left_for_test(&after); // 敌人还没动
        let post = enemy_hp_left_for_test(&threat.end_turn(after)); // 敌人打完
        assert_ne!(
            pre, post,
            "这个局面本来就分不开两种口径，测试立不住 —— 换一个带回合边界伤害的局面"
        );
        assert_eq!(
            tiebreak_left(&s, &line, &threat),
            post,
            "planner 的并列判据量在了敌人回合**之前**，和 solver 不是一个口径"
        );
    }

    /// `solver::enemy_hp_left` 的复制品，只给上面那条测试用
    /// （本体是 `pub(crate)`，但测试要的是"独立算一遍"而不是调用它）。
    fn enemy_hp_left_for_test(s: &State) -> i32 {
        (0..s.n_enemies as usize)
            .filter(|&e| s.enemies[e].alive())
            .map(|e| crate::content::remaining_hp_including_revives(&s.enemies[e]))
            .sum()
    }

    /// **打出薪火之源之后，叶评估必须给出更高的分。**
    ///
    /// 这就是方案 A 要修的那件事本身。它坏掉的样子正是 2026-08-31 之前的样子：
    /// `eval` 数的五样东西里没一样看得见能力状态，挂着薪火之源和没挂
    /// **叶分数逐字相同**，于是跨回合搜索里打能力牌的线在叶子上拿不到任何优势。
    #[test]
    fn leaf_eval_sees_pyre() {
        use crate::solver::score;
        let s = hand_of(11, 300, &[card::PYRE, card::STRIKE, card::STRIKE, card::STRIKE], 3);
        let after = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert!(after.base_energy > s.base_energy, "薪火之源该把最大能量抬上去");
        assert!(
            score::leaf(&after) > score::leaf(&s),
            "叶评估看不见薪火之源：打出前 {} / 打出后 {}",
            score::leaf(&s),
            score::leaf(&after)
        );
        // **对照：单回合那三个目标函数必须一个字不变** —— 它们是驾驶路径，
        // 给它们加跨回合的项会直接改掉实战建议，还会污染 `bin/solve`
        // 那条"实战线赢不过穷尽搜索"的自检。
        for (name, f) in [
            ("survive", score::survive_first as fn(&crate::State) -> i32),
            ("damage", score::damage_first),
            ("hp_only", score::hp_only),
        ] {
            assert_eq!(
                f(&after) - f(&s),
                {
                    let mut a = after;
                    let mut b = s;
                    a.base_energy = 0;
                    b.base_energy = 0;
                    f(&a) - f(&b)
                },
                "{name} 受到了 base_energy 的影响 —— 单回合目标函数不该看跨回合的账"
            );
        }
    }

    /// 给下面两条守卫用的场景：两只敌人，血量各自指定，外加一个能力配置。
    fn two_enemy_scene(vh: i32, oh: i32, setup: fn(&mut State)) -> State {
        let mut s = State::new(80, 21);
        s.add_enemy(enemy::DUMMY, 600);
        s.add_enemy(enemy::DUMMY, 600);
        for _ in 0..6 {
            s.add_card(card::STRIKE, 0, 0);
        }
        for _ in 0..4 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.enemies[0].hp = vh;
        s.enemies[1].hp = oh;
        setup(&mut s);
        s
    }

    /// 四种能力配置。`none` 是对照 —— 它红了说明问题不在能力项。
    const POWER_CONFIGS: [(&str, fn(&mut State)); 4] = [
        ("none", |_s: &mut State| {}),
        ("boulder10", |s: &mut State| s.player.set(St::RollingBoulder, 10)),
        ("demon2", |s: &mut State| s.player.set(St::DemonForm, 2)),
        ("pyre2", |s: &mut State| {
            s.player.set(St::Pyre, 2);
            s.base_energy = crate::solver::BASE_ENERGY + 2;
        }),
    ];

    /// **收掉一只敌人永远不该让叶分数变低。**
    ///
    /// 这是 `power_horizon_value` 第三项（滚石）2026-09-02 修掉的那个 bug 的
    /// 不变量形式。旧写法按**还活着几只**乘：
    ///
    /// ```text
    /// n_alive × (boulder·h + 5·h(h−1)/2) × w.enemy_hp
    /// ```
    ///
    /// 一只 1 血的怪只值 `1×75 = 75` 分的"还活着"惩罚，却扛着一整份未来滚石
    /// 收益 —— 而那份收益根本兑现不了，它只剩 1 血可挨。于是**收人头是亏的**：
    /// 实测同一个局面（一只 1 血 + 一只 300 血，`h` 全程 8 没动）无能力 **+75**、
    /// 滚石 10 **−16425**。**主犯是 `n_alive` 而不是 `h`** —— 那三行 `h` 一个字
    /// 没动，所以"把 h 换成静态常数"治不了这条。
    ///
    /// # 三项各自靠什么守住斜率
    ///
    /// 判据是 `power_horizon_value` 文档里那条「斜率契约」：
    /// `∂P/∂(敌人血) ≤ w.enemy_hp`。三项各走两种合法形态之一 ——
    /// 薪火之源和滚石**用敌人血当单位再封顶**（滚石是逐只），
    /// 恶魔形态**把地平线夹在斜率界以内**，而地平线本身换成了定点，
    /// 免得 `ceil` 在边界上给出无穷大的斜率。
    ///
    /// > 2026-09-02 有过一版中间状态：只修了滚石，这张表对 demon2/pyre2
    /// > 在「`h` 变了」的格子上明确豁免（当时最多亏 −525 / −825）。
    /// > **豁免已经删掉了**，现在整表 100 格无条件成立。
    #[test]
    fn killing_an_enemy_never_lowers_the_leaf_score() {
        use crate::solver::{eval, horizon, Weights};
        // 被收的那只 × 留下的那只。**两边都要扫** —— `h` 由血墙算出来，
        // 只固定一个血量的话扫不到 `h` 会变的那几档。
        let mut checked = 0;
        for (name, setup) in POWER_CONFIGS {
            for &vh in &[1, 4, 12, 40, 120] {
                for &oh in &[1, 30, 120, 300, 600] {
                    let alive = two_enemy_scene(vh, oh, setup);
                    let mut dead = alive;
                    dead.enemies[0].hp = 0;
                    checked += 1;
                    let a = eval(&alive, &Weights::LEAF);
                    let d = eval(&dead, &Weights::LEAF);
                    assert!(
                        d >= a,
                        "{name}: 收掉一只 {vh} 血的敌人（场上另有 {oh} 血）让叶分数掉了 \
                         {} 分 —— 活着 {a} / 收掉 {d}（h: {} -> {}）",
                        a - d,
                        horizon(&alive),
                        horizon(&dead),
                    );
                }
            }
        }
        assert_eq!(checked, 100, "表没扫全，只检了 {checked} 格");
    }

    /// **收掉一只「死时召唤」的宿主，「还要啃多少血」不能变多。**
    ///
    /// 三只宿主各打一次（走真的 `step`：打击砍死、召唤在 `EnemyDied` 里发生）：
    /// 砍之前这一只算「剩的几点 + 死时要召出来的」，砍之后召出来的那几只各自按满血算、
    /// 尸体不再算 —— 前后之差正好是砍掉的那几点。
    ///
    /// 2026-09-19 之前 `remaining_hp_including_revives` 不数召唤，砍死宿主那一下这个和**跳涨**
    /// （地精佣兵 5 -> 27、寄生物 5 -> 76、巨斧机器人 5 -> 74），求解器因此不肯收宿主：
    /// 实录 `act1_f12_elite_phrog`（玩家 −1 血）内核重打 89% 死。
    #[test]
    fn killing_a_death_summoning_host_never_raises_the_hp_left_to_chew() {
        let left = |s: &State| -> i32 {
            (0..s.n_enemies as usize)
                .filter(|&e| s.enemies[e].alive())
                .map(|e| crate::content::remaining_hp_including_revives(&s.enemies[e]))
                .sum()
        };
        for (name, hp_full) in [("地精佣兵", 48), ("异蛙寄生虫", 64), ("巨斧机器人", 74)] {
            let mut s = lone(def_id_of(name), hp_full);
            give(&mut s, card::STRIKE);
            s.enemies[0].hp = 5;
            let before = left(&s);
            assert!(before > 5, "{name}：死时要召唤的血该算进去（{before}）");
            let after = step(s, Action::PlayCard { hand: hand_ix_of(&s, card::STRIKE), target: 0 });
            assert!(!after.enemies[0].alive(), "{name}：宿主该死了");
            assert!(!after.combat_over, "{name}：召唤出来了，仗没打完");
            assert_eq!(left(&after), before - 5, "{name}：砍掉的正好是宿主剩的 5 点，一点都不该多出来");
        }
    }

    /// **打掉敌人的血，叶分数永远不该变低** —— 沿着**整条**掉血路径，
    /// 不只是"收人头"那一下。
    ///
    /// 上一条是固定的 100 格表；这一条是随机猎杀，专门去找能分开假设的样本
    /// （本仓库的老规矩：「和所有已知数据一致」不等于对）。它同时覆盖两件事：
    ///
    /// * **连续那一半**（41 -> 40 血）—— 守的是斜率界。`ceil` 的地平线在这里
    ///   会给出无穷大的斜率，而恶魔形态没有敌人血单位可以封顶。
    /// * **离散那一半**（1 -> 0 血，敌人真的死掉）—— 守的是封顶。
    ///
    /// 参数空间比那张表宽得多：1~4 只敌人 × 血量 1~500 × 牌组 1~9 张打击
    /// （改 `optimistic_damage`）× 力量 0~14 × 三种能力**独立掷**（所以会叠加）。
    ///
    /// # 为什么允许 `n_active` 分的额度
    ///
    /// **那几分是整数截断，不是模型缺陷。** 能力项的三项各做一次整除，每项最多
    /// 多跌 1 分（1 分 = 1/75 点血）；同时挂着几项就是几分。
    ///
    /// [实测] 2026-09-02，1260 万步扫描（三种能力独立掷）：
    /// 违反 8832 步 = 0.070%，**最坏 −1 分 = 0.013 点血**。
    ///
    /// > 一度用过一层 1% 的比例余量（`w.enemy_hp` 从 75 压到 74）把它抹平，
    /// > 同一份扫描下确实是 0 违反。**但那层余量本身平均要花掉 100 分**
    /// > （能力项均值 10017 分），**代价是它防的东西的 100 倍**；而且量纲不对 ——
    /// > 截断误差是**常数**（每项 1 分），余量是**比例**，项值小到 ~120 分时
    /// > 余量薄到刚好等于误差本身，**最不管用的地方恰好是最该管用的地方**。
    /// > 所以改成在这里写明一个**有界**的额度，而不是拿一个更大的失真去盖住它。
    ///
    /// 结构性的那三类倒挂在这个额度里**根本装不下**：`n_alive` 是 −16425 分、
    /// `ceil` 阶跃是 −525 ~ −1125 分、多项叠加超预算是 −6 分，都比 `n_active` 大。
    ///
    /// **坏掉的样子（三种，各验过一次）**：地平线退回 `ceil` / 第一项去掉
    /// 按敌人血封顶 / 第二项去掉斜率界，任意一条都会让它当场红。
    #[test]
    fn leaf_score_never_drops_as_i_damage_an_enemy() {
        use crate::solver::{eval, Weights};
        let mut seed = 0xC0FFEE_u64;
        let mut next = |n: u64| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            (seed >> 33) % n
        };
        for _ in 0..20000 {
            let n_en = 1 + next(4) as usize;
            let mut s = State::new(80, next(1000));
            for _ in 0..n_en {
                s.add_enemy(enemy::DUMMY, 600);
            }
            for _ in 0..(1 + next(9)) {
                s.add_card(card::STRIKE, 0, 0);
            }
            for _ in 0..next(6) {
                s.add_card(card::DEFEND, 0, 0);
            }
            let mut base = begin_combat(s);
            for e in 0..n_en {
                base.enemies[e].hp = 1 + next(500) as i32;
            }
            base.player.set(St::Strength, next(15) as i32);
            // **三种能力独立掷，所以会同时挂上两三个。**
            // 这一条必须扫到：三项各自的斜率都 ≤ w.enemy_hp，可是
            // `eval` 的敌人项只赚一次 —— 叠起来会不会超预算，只能量。
            let cfg = next(8);
            if cfg & 1 != 0 {
                base.player.set(St::RollingBoulder, 5 + next(30) as i32);
            }
            if cfg & 2 != 0 {
                base.player.set(St::DemonForm, 1 + next(5) as i32);
            }
            if cfg & 4 != 0 {
                let e = 1 + next(3) as i32;
                base.player.set(St::Pyre, e);
                base.base_energy = crate::solver::BASE_ENERGY + e;
            }
            // 挑一只敌人，把它的血一路打下去（含打到 0）
            let victim = next(n_en as u64) as usize;
            let hp0 = base.enemies[victim].hp;
            let mut prev = eval(&base, &Weights::LEAF);
            let mut cur = hp0;
            while cur > 0 {
                // 步长也随机 —— 固定步长会系统性地跳过某些 `ceil` 边界
                cur = (cur - 1 - next(7) as i32).max(0);
                let mut hit = base;
                hit.enemies[victim].hp = cur;
                let now = eval(&hit, &Weights::LEAF);
                // **允许的只有整数截断那几分，见测试文档。**
                // 能力项每项做一次整除，每项最多多跌 1 分；同时挂着几项就是几分。
                // 一分是 1/75 点血 —— 结构性的那三类倒挂（`n_alive` / `ceil` 阶跃 /
                // 多项叠加超预算）在这个额度里**根本装不下**，它们的量级是几百到上万分。
                let slack = (base.player.get(St::RollingBoulder) > 0) as i32
                    + (base.player.get(St::DemonForm) > 0) as i32
                    + (crate::solver::pyre_energy(&base) > 0) as i32;
                assert!(
                    now >= prev - slack,
                    "cfg={cfg} 敌人数={n_en}：把 {victim} 号从上一档打到 {cur} 血，\
                     叶分数掉了 {} 分（{prev} -> {now}），超过截断额度 {slack} 分",
                    prev - now,
                );
                prev = now;
            }
        }
    }

    /// **滚石那一项逐只敌人算地平线，所以收人头无条件划算。**
    ///
    /// 上面那条对 `h` 会变的格子网开一面（前两项还欠着），滚石这一项**不欠** ——
    /// 它是这次真正修好的那一项，所以在整张表上无条件成立。
    ///
    /// 证明：`h_e` 和 `R_e` 都只看**这只**敌人自己，收掉 v 之后活下来那几只的
    /// 项逐字不变；v 自己那项跌 `min(total(h_e), R_v)·w ≤ R_v·w`，
    /// 而 `eval` 的敌人项白赚 `R_v·w` ⇒ 净收益恒 ≥ 0。
    ///
    /// **坏掉的样子（两种，各验过一次）**：
    /// * 第三项改回 `n_alive × total` ⇒ 大面积红（`h` 不变的格子也红）；
    /// * 只留封顶、把 `h_e` 换回整场的 `h` ⇒ `h` 会变的格子红。
    #[test]
    fn boulder_horizon_is_measured_per_enemy() {
        use crate::solver::{eval, horizon, Weights};
        let boulder = POWER_CONFIGS[1].1;
        for &vh in &[1, 4, 12, 40, 120] {
            for &oh in &[1, 30, 120, 300, 600] {
                let alive = two_enemy_scene(vh, oh, boulder);
                let mut dead = alive;
                dead.enemies[0].hp = 0;
                let a = eval(&alive, &Weights::LEAF);
                let d = eval(&dead, &Weights::LEAF);
                assert!(
                    d >= a,
                    "滚石: 收掉一只 {vh} 血的敌人（场上另有 {oh} 血）让叶分数掉了 {} 分 \
                     —— 活着 {a} / 收掉 {d}（h: {} -> {}）",
                    a - d,
                    horizon(&alive),
                    horizon(&dead),
                );
            }
        }
        // **方向对照**：只断言"不降"的话，把整个能力项砍成 0 也能过 ——
        // 那就把 `leaf_eval_sees_pyre` 那条修复退回去了。
        // 一只血厚到滚石打不穿的敌人，收掉它必须**严格**赚。
        let alive = two_enemy_scene(120, 300, boulder);
        let mut dead = alive;
        dead.enemies[0].hp = 0;
        assert!(
            eval(&dead, &Weights::LEAF) > eval(&alive, &Weights::LEAF),
            "收掉一只 120 血的敌人在滚石下居然不赚"
        );
    }

    /// 第 3 幕 Boss 实验体在某个形态、某个血量上的局面。
    ///
    /// `max_hp` 就是形态：100 / 200 / 300（`TEST_SUBJECT_FORM_HP`）。
    /// 形态 3 上适生力已经掉光了 —— 这正是 `remaining_hp_including_revives`
    /// 读的那个条件。
    fn test_subject_at(form_max_hp: i32, hp: i32, strength: i32) -> State {
        let mut s = State::new(80, 21);
        s.add_enemy(enemy::TEST_SUBJECT_BOSS, form_max_hp);
        for _ in 0..6 {
            s.add_card(card::STRIKE, 0, 0);
        }
        for _ in 0..4 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut s = begin_combat(s);
        s.enemies[0].max_hp = form_max_hp;
        s.enemies[0].hp = hp;
        if form_max_hp >= crate::content::TEST_SUBJECT_FORM_HP[2] {
            s.enemies[0].set(St::Adaptable, 0); // 最后一个形态，不再复活
        }
        // 抬高 `optimistic_damage`，好让地平线落在闸（8）**下面** ——
        // 两边都顶到 8 的话这条测试是空的。
        s.player.set(St::Strength, strength);
        s
    }

    /// **多形态 Boss 上，`horizon` 不因为形态切换而阶跃。**
    ///
    /// `horizon` 答的是「这场仗还要打几个我的回合」，而后面的形态是**必然
    /// 要来的**。2026-09-02 之前它读 `enemy_wall`（只算当前形态），于是
    /// 形态 1 打到剩 1 血时 `h = 1`、复活成形态 2 满血那一帧 `h = 7` ——
    /// 所有正比于 `h` 的能力项在那一帧翻七倍，决策跟着震荡。
    ///
    /// **坏掉的样子**：`horizon` 改回 `enemy_wall`，下面第二段断言当场红。
    /// 第一段（`assert!(steps_with_current_form)`）是**反vacuous 检查** ——
    /// 它保证这个局面真的分得开两种口径，不然这条测试立不住。
    #[test]
    fn horizon_does_not_step_when_a_boss_changes_form() {
        use crate::solver::{enemy_wall, horizon, optimistic_damage, HORIZON_CAP};
        // 2026-09-02 之前的 `horizon` **逐字抄在这里** —— 要的是"独立算一遍
        // 旧口径"，不是调用现在的实现。
        let old_horizon = |s: &State| {
            let dpt = optimistic_damage(s).max(1);
            ((enemy_wall(s) + dpt - 1) / dpt).clamp(1, HORIZON_CAP)
        };
        const STR: i32 = 20;
        // 一整场：形态 1 从满血打到 1 血，复活成形态 2，再打到形态 3。
        let path = [
            (crate::content::TEST_SUBJECT_FORM_HP[0], 100),
            (crate::content::TEST_SUBJECT_FORM_HP[0], 40),
            (crate::content::TEST_SUBJECT_FORM_HP[0], 1),
            (crate::content::TEST_SUBJECT_FORM_HP[1], 200),
            (crate::content::TEST_SUBJECT_FORM_HP[1], 40),
            (crate::content::TEST_SUBJECT_FORM_HP[1], 1),
            (crate::content::TEST_SUBJECT_FORM_HP[2], 300),
            (crate::content::TEST_SUBJECT_FORM_HP[2], 40),
            (crate::content::TEST_SUBJECT_FORM_HP[2], 1),
        ];
        let states: Vec<State> = path.iter().map(|&(m, h)| test_subject_at(m, h, STR)).collect();

        // 反 vacuous：旧口径（只算当前形态）在这条路上**必须**阶跃，
        // 否则这个局面分不开两种口径，测试白写。
        let old: Vec<i32> = states.iter().map(old_horizon).collect();
        assert!(
            old.windows(2).any(|w| w[1] > w[0]),
            "旧口径在这条路上没有阶跃，这个局面分不开两种口径：{old:?}"
        );

        // 新口径：地平线沿着"我打掉血"这条路**单调不增**。
        let new: Vec<i32> = states.iter().map(horizon).collect();
        for (i, w) in new.windows(2).enumerate() {
            assert!(
                w[1] <= w[0],
                "形态 {:?} -> {:?} 那一步地平线从 {} 跳到 {}：整条路 {new:?}",
                path[i],
                path[i + 1],
                w[0],
                w[1],
            );
        }
        // 而且不能是"全顶到闸上"那种平凡的不阶跃。
        assert!(
            new.iter().any(|&h| h < crate::solver::HORIZON_CAP),
            "整条路的地平线都顶在闸上（{new:?}），这条测试是空的"
        );
    }

    /// **斩杀延伸仍然按「当前形态」判，不跟着 `horizon` 换口径。**
    ///
    /// 两处口径是**故意不同**的：延伸划算的唯一理由是"多搜一层能把估值变成
    /// 事实"，而过一层之后成为事实的是「这一形态死没死」。改成整场（含后面
    /// 的形态）之后，多形态 Boss 上斩杀延伸**永远不会触发** ——
    /// 那正好把实测唯一划算的那条延伸关掉了。
    ///
    /// **坏掉的样子**：`want_extension` 改用 `enemy_wall_all_forms`，这条当场红。
    #[test]
    fn lethal_extension_still_looks_at_the_current_form_only() {
        use crate::plan::{want_extension, Ext, Plan};
        // 形态 1 剩 1 血：这一形态一击就死，但整场还剩 501 血。
        let s = test_subject_at(crate::content::TEST_SUBJECT_FORM_HP[0], 1, 20);
        let leaf = crate::step::end_turn_before_draw(s);
        let mut cfg = Plan::default();
        cfg.ext_spike = false;
        cfg.ext_boundary = false;
        assert!(
            matches!(want_extension(&leaf, &cfg), Some(Ext::Lethal)),
            "形态 1 只剩 1 血，斩杀延伸居然没上膛 —— 多半是跟着 horizon 换成了整场口径"
        );
    }

    /// 地平线必须**随敌人血量变长、随我的输出变短**，并且被闸夹住。
    ///
    /// 常数地平线正是要避免的失败模式：快打完的仗里高估能力牌、
    /// 长 Boss 战里继续低估。
    #[test]
    fn horizon_tracks_the_fight_length() {
        use crate::solver::{horizon, HORIZON_CAP};
        let small = hand_of(5, 10, &[card::STRIKE, card::STRIKE], 3);
        let huge = hand_of(5, 600, &[card::STRIKE, card::STRIKE], 3);
        assert!(
            horizon(&small) < horizon(&huge),
            "血更厚的敌人该给出更长的地平线：{} vs {}",
            horizon(&small),
            horizon(&huge)
        );
        assert!(horizon(&huge) <= HORIZON_CAP, "地平线没有被闸夹住");
        assert!(horizon(&small) >= 1, "地平线不该到 0");
    }

    /// **推演叶评估看得见静态评估结构性看不见的东西。**（方案 C）
    ///
    /// 这一条同时钉住两件事，而它们正是 A 和 C 的分界：
    /// * 绯红披风在 `power_horizon_value` 里是**明确拒绝计价**的 ——
    ///   它的价值是 `min(格挡, 来袭)`，而 `fn(&State) -> i32` 拿不到来袭。
    ///   所以静态叶评估对"挂着 10 层绯红披风"和"没挂"**必须给出同一个分**。
    /// * 推演里那 10 点格挡每回合真的产生、真的去挡敌人那一手，
    ///   所以推演叶评估**必须**分得开这两个局面。
    ///
    /// 它坏掉的样子：第一条断言红 = 有人给绯红披风加了静态计价（那就要连着
    /// 解决"拿不到来袭"那个问题）；第二条红 = 推演叶子没真的跑起来。
    #[test]
    fn rollout_leaf_sees_what_static_eval_refuses() {
        use crate::plan::Leaf;
        use crate::solver::score;
        let mut base = State::new(80, 21);
        base.add_enemy(enemy::NIBBIT, 44);
        for _ in 0..8 {
            base.add_card(card::STRIKE, 0, 0);
        }
        for _ in 0..4 {
            base.add_card(card::DEFEND, 0, 0);
        }
        let base = begin_combat(base);
        let mut with = base;
        with.player.set(St::CrimsonMantle, 10);

        assert_eq!(
            score::leaf(&base),
            score::leaf(&with),
            "静态叶评估分开了这两个局面 —— 绯红披风本该是拒绝计价的那一类"
        );

        let roll = Leaf::Rollout { samples: 16, turns: 12, tail: score::leaf };
        let a = roll.value(&base, 0xC0FFEE, 1);
        let b = roll.value(&with, 0xC0FFEE, 1);
        assert!(b > a, "推演叶评估也没看见绯红披风：没挂 {a} / 挂着 {b}");
    }

    /// 推演叶子必须是**确定**的：同一个局面 + 同一个种子 + 同一个深度 ⇒ 同一个数。
    ///
    /// 这是 CRN 的前提。种子里一旦混进局面自身的东西（比如牌堆多重集），
    /// 兄弟候选就各抽各的样本，**最该配对的那一层反而没配对** ——
    /// 2026-08-27 机会节点的播种 bug 就是这么来的。
    #[test]
    fn rollout_leaf_is_deterministic_given_seed_and_depth() {
        use crate::plan::Leaf;
        use crate::solver::score;
        let mut s = State::new(70, 9);
        s.add_enemy(enemy::NIBBIT, 44);
        // **牌组必须是混的。** 一副 10 张全打击的牌洗成什么样，抽到的手牌
        // 都一模一样 —— 那种局面下"换种子不变"是对的，测不出种子有没有进播种。
        // （第一版就是这么写的，白红了一次。）
        for _ in 0..6 {
            s.add_card(card::STRIKE, 0, 0);
        }
        for _ in 0..6 {
            s.add_card(card::DEFEND, 0, 0);
        }
        for _ in 0..3 {
            s.add_card(card::BASH, 0, 0);
        }
        let s = begin_combat(s);
        let roll = Leaf::Rollout { samples: 4, turns: 6, tail: score::leaf };
        assert_eq!(roll.value(&s, 77, 2), roll.value(&s, 77, 2), "同种子同深度给出了两个数");
        // **种子必须真的进到播种里**：换一个全局种子，样本就该换一批。
        // 这一条防的是"种子参数被接了但没用"——那种错不报错，
        // 只是让所有叶子共用一条随机流。
        let a = roll.value(&s, 77, 2);
        let b = roll.value(&s, 5_000_011, 2);
        let c = roll.value(&s, 900_007, 2);
        assert!(
            a != b || a != c,
            "换了两个全局种子，推演叶子给出的还是同一个数 —— 种子多半没进播种"
        );
    }

    /// `rollout_to_state` 是从 `rollout_outcome_with` 里拆出来的，
    /// 拆的时候**行为必须一个字节没变**。
    #[test]
    fn rollout_to_state_agrees_with_outcome() {
        use crate::rollout::{rollout_outcome_with, rollout_to_state, Policy};
        for seed in 1..=6u64 {
            let mut s = State::new(75, seed);
            s.add_enemy(enemy::NIBBIT, 44);
            for _ in 0..10 {
                s.add_card(card::STRIKE, 0, 0);
            }
            let s = begin_combat(s);
            let o = rollout_outcome_with(s, 12, Policy::Fast);
            let (end, turns) = rollout_to_state(s, 12, Policy::Fast);
            assert_eq!(o.turns, turns, "seed {seed}：回合数对不上");
            assert_eq!(o.final_hp, end.player.hp, "seed {seed}：终局血量对不上");
            assert_eq!(o.won, end.combat_over && !end.player_dead, "seed {seed}：胜负对不上");
        }
    }

    /// 造一个「抽牌堆凑不齐一手、必须洗弃牌堆」的回合起点。
    ///
    /// 手牌清空、抽牌堆 `n_draw` 张、弃牌堆 `n_disc` 张，牌**必须是混的** ——
    /// 一副全打击的牌洗成什么样抽出来都一样，那种局面测不出种子有没有进播种。
    fn short_draw_pile_scene(n_draw: usize, n_disc: usize) -> State {
        let kinds = [card::STRIKE, card::DEFEND, card::BASH];
        let mut s = State::new(80, 21);
        s.add_enemy(enemy::DUMMY, 100);
        let ix: Vec<u8> =
            (0..n_draw + n_disc).map(|i| s.add_card(kinds[i % kinds.len()], 0, 0)).collect();
        let mut s = begin_combat(s);
        s.n_hand = 0;
        s.n_draw = n_draw as u8;
        for i in 0..n_draw {
            s.draw[i] = ix[i];
        }
        s.n_draw_known = 0;
        s.n_disc = n_disc as u8;
        for i in 0..n_disc {
            s.disc[i] = ix[n_draw + i];
        }
        crate::step::end_turn_before_draw(s)
    }

    /// 一个孩子的手牌指纹（多重集，排过序）。
    fn hand_ids(s: &State) -> Vec<u16> {
        let mut v: Vec<u16> = (0..s.n_hand as usize).map(|i| s.cards[s.hand[i] as usize].id).collect();
        v.sort_unstable();
        v
    }

    /// **抽牌堆凑不齐一手时，机会节点必须真的采样，而且要读 `Plan::seed`。**
    ///
    /// 2026-09-02 之前 `chance_children` 开头是
    /// `if r == 0 || n < WANT { ... return vec![Draw { p: 1.0 }] }` ——
    /// **把两种完全不同的情况合并了**：
    /// * `r == 0`（前 5 张全是已知前缀）确实确定，`p = 1.0` 是对的；
    /// * `n < 5` **必须洗弃牌堆才凑得齐一手**，整个弃牌堆洗回来是那一刻的
    ///   主导不确定性，却既不枚举也不采样。
    ///
    /// 最要命的一层：那条路径**不读 `cfg.seed`**（洗牌用状态自带的
    /// `rng.shuffle`），于是 `tools/plan_seed_sweep.py` 把这些回合一律记成
    /// "换种子不变" —— **可重复性指标恰好在不确定性最大的那四分之一回合上是瞎的**。
    /// [实测] 60 条实录的 230 个 `end_turn` 帧里 57 个 `draw_count < 5` = 25%。
    ///
    /// **坏掉的样子**：把两条分支合回一个 `if`，(a)(b) 两条断言当场红。
    #[test]
    fn a_short_draw_pile_is_sampled_and_reads_the_seed() {
        use crate::plan::{chance_children, Plan};
        let s = short_draw_pile_scene(3, 6);
        assert!(s.n_draw < 5 && s.n_disc > 0, "场景立不住：{}/{}", s.n_draw, s.n_disc);

        // (a) 必须多于一个孩子，而且权重和仍然是 1
        let cfg = Plan::default();
        let kids = chance_children(&s, &cfg, 1);
        assert!(kids.len() > 1, "抽牌堆只剩 {} 张、弃牌堆 {} 张，机会节点却只给了 1 个孩子", s.n_draw, s.n_disc);
        let total: f64 = kids.iter().map(|k| k.p).sum();
        assert!((total - 1.0).abs() < 1e-9, "权重和是 {total}，不是 1");
        for k in &kids {
            assert_eq!(k.state.n_hand, 5, "有个孩子没抽满 5 张");
        }
        // 采样是有意义的：w 个样本不该全是同一手牌
        let distinct: std::collections::BTreeSet<Vec<u16>> =
            kids.iter().map(|k| hand_ids(&k.state)).collect();
        assert!(distinct.len() > 1, "{} 个样本抽出来是同一手牌 —— 洗牌没被播种", kids.len());

        // (b) 换 `Plan::seed` 结果必须变
        let mut other = Plan::default();
        other.seed = 0xA5A5_1234;
        let kids2 = chance_children(&s, &other, 1);
        let a: Vec<Vec<u16>> = kids.iter().map(|k| hand_ids(&k.state)).collect();
        let b: Vec<Vec<u16>> = kids2.iter().map(|k| hand_ids(&k.state)).collect();
        assert_ne!(a, b, "换了 Plan::seed，采到的还是同一批手牌 —— 这条路径没读种子");

        // CRN 的前提照旧：同一个局面 + 同一个种子 + 同一层 ⇒ 同一批样本
        let again: Vec<Vec<u16>> = chance_children(&s, &cfg, 1).iter().map(|k| hand_ids(&k.state)).collect();
        assert_eq!(a, again, "同种子同深度采到了两批不同的样本");
    }

    /// 反过来的两条：**该确定的时候仍然只给一个 `p = 1.0` 的孩子。**
    ///
    /// 拆分支很容易顺手把这两种也拖进采样，那就是花 `width` 倍的钱买 w 份
    /// 一模一样的孩子。
    #[test]
    fn a_determinate_draw_still_collapses_to_one_child() {
        use crate::plan::{chance_children, Plan};
        let cfg = Plan::default();

        // 1. 抽牌堆不够、但**弃牌堆是空的** —— 剩下几张全抽走，没别的可能
        let dry = short_draw_pile_scene(3, 0);
        assert_eq!(dry.n_disc, 0);
        let kids = chance_children(&dry, &cfg, 1);
        assert_eq!(kids.len(), 1, "弃牌堆空的时候不该采样");
        assert!((kids[0].p - 1.0).abs() < 1e-9);

        // 2. 已知前缀 ≥ 5 张 —— 前 5 张身份明确，抽到什么是定死的
        let mut known = short_draw_pile_scene(8, 4);
        known.n_draw_known = known.n_draw;
        let kids = chance_children(&known, &cfg, 1);
        assert_eq!(kids.len(), 1, "已知前缀盖满 5 张时不该采样");
        assert!((kids[0].p - 1.0).abs() < 1e-9);
    }

    /// **机会节点不许偷看真实的下一手。**
    ///
    /// 2026-09-02 发现的、比"不读种子"更重的一层：`chance_children` 拿到的局面
    /// 正是 `end_turn_before_draw` 的输出，也就是 `step(EndTurn)` **马上就要
    /// `open_hand` 的那个局面**（敌人那一手已经打完了）。旧分支在这个局面上
    /// 直接 `open_hand` 并把它当成唯一的孩子 —— 那**逐字节就是真实的下一手**。
    ///
    /// 于是在 25% 的回合边界上，planner 是**带着答案在搜**。
    /// 这解释了修好之后 P5 均值从 +4.2 掉到 +3.8：**掉的是作弊分**。
    /// （敌人流和洗牌流是分开的 —— 不变量 4 —— 所以敌人那一手不会扰动抽牌。）
    #[test]
    fn the_chance_node_does_not_peek_at_the_real_next_hand() {
        use crate::plan::{chance_children, Plan};
        // 和 `short_draw_pile_scene` 同一副牌，但从**回合边界之前**出发，
        // 这样才走得到 `step(EndTurn)` 那条真路径。
        let kinds = [card::STRIKE, card::DEFEND, card::BASH];
        let mut pre = State::new(80, 21);
        pre.add_enemy(enemy::DUMMY, 100);
        let ix: Vec<u8> = (0..9).map(|i| pre.add_card(kinds[i % 3], 0, 0)).collect();
        let mut pre = begin_combat(pre);
        pre.n_hand = 0;
        pre.n_draw = 3;
        for i in 0..3 {
            pre.draw[i] = ix[i];
        }
        pre.n_draw_known = 0;
        pre.n_disc = 6;
        for i in 0..6 {
            pre.disc[i] = ix[3 + i];
        }
        let truth = hand_ids(&step(pre, Action::EndTurn));
        let boundary = crate::step::end_turn_before_draw(pre);
        let kids = chance_children(&boundary, &Plan::default(), 1);
        assert!(
            kids.iter().any(|k| hand_ids(&k.state) != truth),
            "机会节点采出来的每一手都等于真实的下一手 —— planner 在偷看"
        );
    }

    /// 机会节点：**枚举出来的概率必须加起来等于 1**。
    #[test]
    fn chance_weights_sum_to_one() {
        use crate::plan::{chance_children, Plan};
        let cfg = Plan::default();
        for seed in 1..=8u64 {
            let mut s = State::new(80, seed);
            s.add_enemy(enemy::DUMMY, 200);
            for _ in 0..5 {
                s.add_card(card::STRIKE, 0, 0);
            }
            for _ in 0..4 {
                s.add_card(card::DEFEND, 0, 0);
            }
            s.add_card(card::BASH, 0, 0);
            let s = begin_combat(s);
            let s = crate::step::end_turn_before_draw(s);
            if s.n_draw < 5 {
                continue;
            }
            let kids = chance_children(&s, &cfg, 1);
            let total: f64 = kids.iter().map(|k| k.p).sum();
            assert!((total - 1.0).abs() < 1e-9, "seed {seed}：概率和是 {total}，不是 1");
        }
    }

    /// 机会节点的两条硬性质，**带不带已知前缀都要成立**：
    /// 1. 每个孩子都抽满 5 张，概率加起来是 1；
    /// 2. 顶上那几张确定的牌每个孩子都抽到；
    /// 3. **枚举说抽到什么，就真的抽到什么** —— 拿超几何的期望对一遍。
    ///
    /// 第 3 条是这里最要紧的：它同时看得见两个经典错误 ——
    /// 把选中的牌挪到了牌堆**底**（`draw_one` 取的是末尾），
    /// 以及**把已知前缀也算进要随机挑的那 r 张里**（那样 r 多了两张，
    /// 抽出来的手牌和标签对不上）。
    /// 第一版只测了"确定的牌被抽到"，把后一个错放了过去 —— 因为那些牌
    /// 本来就在顶上，怎么算都会被抽到，错的是**别的牌的分布**。
    fn check_chance_node(s: &State, tag: &str) {
        use crate::plan::{chance_children, Plan};
        let n = s.n_draw as usize;
        let known = (s.n_draw_known as usize).min(n);
        assert!(n >= 5, "{tag}：这个局面该有至少 5 张可抽，实际 {n}");

        // 期望张数：确定的那几张里有几张打击是定死的，
        // 剩下 r 张按超几何从未知区里出
        let r = 5 - known.min(5);
        let known_strikes = (n - known..n)
            .filter(|&i| s.cards[s.draw[i] as usize].id == card::STRIKE)
            .count();
        let unk_strikes =
            (0..n - known).filter(|&i| s.cards[s.draw[i] as usize].id == card::STRIKE).count();
        let unk_n = n - known;
        let want = known_strikes as f64
            + if unk_n > 0 { r as f64 * unk_strikes as f64 / unk_n as f64 } else { 0.0 };

        let kids = chance_children(s, &Plan::default(), 1);
        let total: f64 = kids.iter().map(|k| k.p).sum();
        assert!((total - 1.0).abs() < 1e-9, "{tag}：概率和是 {total}");

        let mut got = 0.0f64;
        for k in &kids {
            assert_eq!(k.state.n_hand, 5, "{tag}：有个孩子没抽满 5 张");
            let hand: Vec<u8> = (0..k.state.n_hand as usize).map(|i| k.state.hand[i]).collect();
            for i in n - known..n {
                assert!(hand.contains(&s.draw[i]), "{tag}：确定的牌没被抽到");
            }
            let c = hand
                .iter()
                .filter(|&&ix| k.state.cards[ix as usize].id == card::STRIKE)
                .count();
            got += k.p * c as f64;
        }
        assert!(
            (got - want).abs() < 1e-9,
            "{tag}：手牌里打击的期望张数 {got}，超几何说该是 {want}"
        );
    }

    /// 机会节点：**没有已知前缀**的常规局面。
    #[test]
    fn chance_node_matches_the_hypergeometric_without_a_known_prefix() {
        let mut s = State::new(80, 6);
        s.add_enemy(enemy::DUMMY, 200);
        for _ in 0..8 {
            s.add_card(card::STRIKE, 0, 0);
        }
        for _ in 0..6 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let s = begin_combat(s);
        let s = crate::step::end_turn_before_draw(s);
        assert_eq!(s.n_draw_known, 0);
        check_chance_node(&s, "无已知前缀");
    }

    // -----------------------------------------------------------------
    // 确定性窗口（2026-09-05，阶段 2）
    // -----------------------------------------------------------------

    /// **`chance_is_certain` 说确定，`chance_children` 就必须只给一个孩子。**
    ///
    /// 这一条守的是"判据只有一处定义"：确定性窗口靠 `chance_is_certain` 决定
    /// 借不借这一层，而真正展开的是 `chance_children`。两边一旦分岔，
    /// 窗口就会在一个**会分岔**的节点上借层 —— 那正是这个判据要防的事，
    /// 而且**不报错，只是搜错**。
    #[test]
    fn chance_is_certain_agrees_with_chance_children() {
        use crate::plan::{chance_children, chance_is_certain, Plan};
        let cfg = Plan::default();
        // 五种局面各来一份：抽干净、抽干净但有弃牌堆、已知前缀盖满、
        // 已知前缀不够、整堆都不知道
        let mut scenes: Vec<(State, &str)> = vec![
            (short_draw_pile_scene(3, 0), "抽牌堆 3 张、弃牌堆空"),
            (short_draw_pile_scene(3, 6), "抽牌堆 3 张、弃牌堆 6 张"),
            (short_draw_pile_scene(8, 4), "抽牌堆 8 张、没有已知前缀"),
        ];
        let mut known = short_draw_pile_scene(8, 4);
        known.n_draw_known = known.n_draw;
        scenes.push((known, "整堆已知"));
        let mut half = short_draw_pile_scene(8, 4);
        half.n_draw_known = 2;
        scenes.push((half, "只知道顶上 2 张"));

        for (s, tag) in &scenes {
            let kids = chance_children(s, &cfg, 1);
            let single = kids.len() == 1 && (kids[0].p - 1.0).abs() < 1e-9;
            if chance_is_certain(s) {
                assert!(single, "{tag}：判据说确定，`chance_children` 却给了 {} 个孩子", kids.len());
            }
        }
    }

    /// **判据是"双重确定"，不是"单孩子"。**
    ///
    /// 同一个整堆已知的牌堆，只换场上那只敌人：
    /// 假人（`machine: None`，固定循环）⇒ 下一手唯一 ⇒ 窗口开；
    /// 蛮兽（`MAWLER_RAND`，从爪击出发允许 {撕咬, 咆哮} 两手）⇒ 掷骰 ⇒ 窗口关。
    ///
    /// **坏掉的样子**：把判据写成"孩子只有一个"。那时第二条断言当场红 ——
    /// 抽牌确实定死了，可 `advance_move` 会掷一次骰，我们跟着的只是一条抽样
    /// 轨迹，把它当事实往下深搜是在放大一个样本。
    #[test]
    fn the_window_needs_both_halves_not_just_one_child() {
        use crate::plan::{chance_is_certain, window_is_certain};

        let mut fixed = short_draw_pile_scene(8, 4);
        fixed.n_draw_known = fixed.n_draw;
        assert!(chance_is_certain(&fixed), "场景立不住：抽牌那一半就不确定");
        assert!(window_is_certain(&fixed), "假人是固定循环，下一手该是唯一的");

        // 同一个牌堆，把敌人换成带随机分支的蛮兽
        let mut rand_foe = fixed;
        rand_foe.n_enemies = 0; // 让 `add_enemy` 覆盖 0 号槽
        rand_foe.add_enemy(enemy::MAWLER, 100);
        rand_foe.enemy_hist[0] = [u8::MAX; crate::state::ENEMY_HIST];
        rand_foe.enemy_move[0] = crate::step::initial_move(&rand_foe, 0);
        assert!(
            crate::step::allowed_next(&rand_foe, 0).count_ones() > 1,
            "场景立不住：蛮兽这一手之后本来就该有两个可能"
        );
        assert!(chance_is_certain(&rand_foe), "抽牌那一半没变，还该是确定的");
        assert!(
            !window_is_certain(&rand_foe),
            "敌人下一手会掷骰，窗口却认为这一层是确定的 —— 判据退化成了「单孩子」"
        );
    }

    /// **窗口的默认值不许悄悄改回去**，而且 `window = 0` 要真的关得掉。
    #[test]
    fn window_defaults_to_the_cap_and_zero_switches_it_off() {
        use crate::plan::{Plan, HARD_DEPTH_CAP, MAX_EXTEND, WINDOW_CAP};
        let cfg = Plan::default();
        assert_eq!(cfg.window, WINDOW_CAP, "确定性窗口的默认值被改了，见 Plan::window");
        // 三笔预算加起来必须够不着硬闸，否则窗口会被截断而没人知道
        assert!(
            (cfg.depth + MAX_EXTEND + WINDOW_CAP) as usize <= HARD_DEPTH_CAP,
            "depth {} + 延伸 {MAX_EXTEND} + 窗口 {WINDOW_CAP} 顶到了硬闸 {HARD_DEPTH_CAP}",
            cfg.depth
        );
    }

    /// **确定层不扣计划深度**，而且三笔账都真的被扣了。
    ///
    /// 造一个整堆已知、敌人是固定循环的局面（那一段是有限确定性博弈），
    /// 从 `left = 0` 出发：
    ///
    /// * `window = 0` ⇒ 就地评估（老行为），一层都不借；
    /// * `window > 0` ⇒ 真的往下搜，`window_fired > 0`，而且**借的层数不超过预算**；
    /// * 无论借多少，`max_depth` 都在 [`HARD_DEPTH_CAP`] 之内。
    ///
    /// 第三条是这里唯一能抓住"忘了扣 `free`"的：那种错不报错、不改端点的值，
    /// 只让搜索越钻越深 —— 和 `extension_budget_is_actually_checked` 守的是
    /// 同一类事故。
    #[test]
    fn the_window_deepens_without_spending_planned_depth() {
        use crate::plan::{node_value_ext, window_is_certain, Plan, PlanStat, HARD_DEPTH_CAP};
        // 厚血假人 + 1 能量：仗一时半会打不完，窗口才有得借
        let mut s = State::new(80, 44);
        s.add_enemy(enemy::DUMMY, 300);
        for _ in 0..20 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        s.base_energy = 1;
        s.energy = 1;
        let mut leaf = crate::step::end_turn_before_draw(s);
        // 整堆已知 = `draw_pile_order` 补丁之后 `sync` 出来的样子
        leaf.n_draw_known = leaf.n_draw;
        assert!(window_is_certain(&leaf), "场景立不住：这一层本来就该是确定的");

        let mut off = Plan::default();
        off.depth = 2;
        off.window = 0;
        let mut st_off = PlanStat::default();
        let v_off = node_value_ext(&leaf, &off, 0, 1, 0, &mut st_off);
        assert_eq!(v_off, off.leaf.value(&leaf, off.seed, 1), "关掉窗口就该就地评估");
        assert_eq!(st_off.window_fired, 0, "窗口关着还借了层");

        let mut on = off;
        on.window = 3;
        let mut st_on = PlanStat::default();
        let v_on = node_value_ext(&leaf, &on, 0, 1, 0, &mut st_on);
        assert!(st_on.window_fired > 0, "确定的层一层都没借");
        assert!(
            st_on.window_fired <= on.window as u32,
            "借了 {} 层，而预算只有 {} —— `free` 没被扣",
            st_on.window_fired,
            on.window
        );
        assert_ne!(v_on, v_off, "借了层却和就地评估同分，说明根本没往下搜");
        assert!(
            (st_on.max_depth as usize) <= HARD_DEPTH_CAP,
            "递归到了第 {} 层，硬闸在 {HARD_DEPTH_CAP}",
            st_on.max_depth
        );
        // 借来的层不该动 `ext` 那笔账 —— 上面传的 `ext` 就是 0
        assert_eq!(st_on.ext_fired, 0, "窗口把延伸预算也花掉了，两笔账串了");
    }

    /// 机会节点：**顶上摆了两张确定的牌**。
    ///
    /// 这一条是上面那条的另一半：确定的那两张不参与随机，
    /// 剩下 3 张才按超几何从未知区里出。把已知前缀也算进随机区（`known = 0`）
    /// 在上面那条里是无害的，在这里就对不上了。
    #[test]
    fn chance_node_matches_the_hypergeometric_with_a_known_prefix() {
        let mut s = State::new(80, 6);
        s.add_enemy(enemy::DUMMY, 200);
        for _ in 0..8 {
            s.add_card(card::STRIKE, 0, 0);
        }
        for _ in 0..6 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let s = begin_combat(s);
        let mut s = crate::step::end_turn_before_draw(s);
        // 从抽牌堆里挑两张摆到顶上（一张打击一张防御，好让分布真的变）
        let a = s.add_card(card::STRIKE, 0, 0);
        let b = s.add_card(card::DEFEND, 0, 0);
        s.n_draw -= 2;
        s.to_draw_top(a);
        s.to_draw_top(b);
        assert_eq!(s.n_draw_known, 2);
        check_chance_node(&s, "带已知前缀");
    }

    /// 置换表：**只有存的深度 ≥ 要的深度才能复用**。
    ///
    /// 它坏掉的样子：拿一个搜得更浅的值去顶替深搜，深度被偷偷砍掉且不报错。
    #[test]
    fn tt_only_reuses_entries_deep_enough() {
        use crate::plan::Tt;
        let tt = Tt::new(10);
        tt.store(0xABCD, 2, 42);
        assert_eq!(tt.probe(0xABCD, 1), Some(42), "存 2 要 1，可以用");
        assert_eq!(tt.probe(0xABCD, 2), Some(42), "存 2 要 2，可以用");
        assert_eq!(tt.probe(0xABCD, 3), None, "存 2 要 3，**不能**用");
        assert_eq!(tt.probe(0x1234, 1), None, "没存过的 key");
        assert_eq!(tt.probes(), 4, "四次 probe 都该记上账");
        assert_eq!(tt.hits(), 2, "命中两次");
        // 负数的值要能原样取回来 —— 载荷是打包进一个 u64 的，
        // 符号位掉了的话所有"要死了"的叶子会变成天文数字的好分数
        tt.store(0x77, 0, i32::MIN / 2);
        assert_eq!(tt.probe(0x77, 0), Some(i32::MIN / 2), "负分打包/解包不能变号");
    }

    /// 置换表**并发写不会互相撕裂**：多个线程同时往同一张表写，
    /// 读回来的每一格要么是某一次写的原样，要么 miss —— 不能出现
    /// "这一半来自 A、那一半来自 B"的缝合值。
    ///
    /// 它守的是 lockless hashing 那个异或校验。写反了/漏了校验位，
    /// 症状是**偶尔**读到一个别人写的值，而搜索照样跑完、不报错。
    #[test]
    fn tt_survives_concurrent_writers() {
        use crate::plan::Tt;
        let tt = Tt::new(12);
        // 值和 key 绑死：`value == (key * 3) as i32`。读到不满足这条的就是缝合的。
        std::thread::scope(|sc| {
            for t in 0..4u64 {
                let tt = &tt;
                sc.spawn(move || {
                    for i in 0..20_000u64 {
                        let k = (i * 4 + t) | 1;
                        tt.store(k, 1, k.wrapping_mul(3) as i32);
                        if let Some(v) = tt.probe(k, 1) {
                            assert_eq!(v, k.wrapping_mul(3) as i32, "key {k} 读到了缝合值");
                        }
                    }
                });
            }
        });
        assert!(tt.probes() > 0, "一次都没探过，这条测试什么都没守住");
    }

    /// 一个**带抽牌堆**的战斗局面。`hand_of` 把牌全发到手上（`n_draw = 0`），
    /// 而候选生成那几条读数问的正是抽牌堆 —— 所以它们要另一个场景。
    fn combat_with_draw_pile(seed: u64) -> State {
        let mut s = State::new(80, seed);
        s.add_enemy(enemy::DUMMY, 300);
        for i in 0..20 {
            s.add_card(if i % 2 == 0 { card::STRIKE } else { card::DEFEND }, 0, 0);
        }
        begin_combat(s)
    }

    /// **`plan::key` 必须分开 `solver::key` 合并掉的那几样。**
    ///
    /// 这是阶段 3c 唯一能钉住的东西：拿 `solver::key` 去做跨回合 memo
    /// 是一个**静默错误** —— 两个只差"这场用过哪几手"的局面被并进同一格，
    /// 而四种对拍模式全看不见。每一项都先断言 `solver::key` 真的合并了它
    /// （否则这条测试是在守一件本来就不会发生的事），再断言 `plan::key` 分开了。
    #[test]
    fn plan_key_separates_what_solver_key_merges() {
        // 要一副**真的抽牌堆**（`hand_of` 把牌全发到手上，`n_draw` 是 0，
        // 那样 `n_draw_known` 那一条就无从改起）
        let base = combat_with_draw_pile(7);
        assert!(base.n_draw >= 5, "场景立不住：抽牌堆还不够一手");
        assert_eq!(crate::plan::key(&base), crate::plan::key(&base), "同一个局面该同一个 key");

        let cases: [(&str, fn(&mut State)); 6] = [
            ("n_draw_known", |s| s.n_draw_known = s.n_draw.min(3)),
            ("enemy_hist", |s| s.enemy_hist[0][0] = 2),
            ("enemy_used", |s| s.enemy_used[0] = 0b1010),
            ("rng.shuffle", |s| s.rng.shuffle ^= 0xDEAD_BEEF),
            ("rng.enemy", |s| s.rng.enemy ^= 0xDEAD_BEEF),
            ("rng.gen", |s| s.rng.gen ^= 0xDEAD_BEEF),
        ];
        for (name, mutate) in cases {
            let mut other = base;
            mutate(&mut other);
            assert_ne!(base, other, "{name}：局面根本没改动，这一条什么都没守住");
            assert_eq!(
                crate::solver::key(&base),
                crate::solver::key(&other),
                "{name}：`solver::key` 居然分得开它 —— 那这条测试守的前提不成立了，\
                 回去看 `plan::key` 的文档表还对不对"
            );
            assert_ne!(
                crate::plan::key(&base),
                crate::plan::key(&other),
                "{name}：`plan::key` 把两个不同的局面并成了同一格"
            );
        }
    }

    /// **窄根才开大 K。** `Plan::k_at` 是这三处（planner / 诊断 / 审计台）
    /// 唯一的定义，它判错的样子是"审计台量的候选集不是 planner 用的那个"。
    #[test]
    fn k_opens_up_only_on_a_narrow_root() {
        use crate::plan::{root_is_narrow, Plan};
        let cfg = Plan::default();
        let mut s = combat_with_draw_pile(11);
        // 整堆已知 = `draw_pile_order` 补丁之后 `sync` 出来的样子
        s.n_draw_known = s.n_draw;
        assert!(s.n_draw >= 5, "场景立不住：抽牌堆还不够一手");
        assert!(root_is_narrow(&s), "整堆已知却不算窄根");
        assert_eq!(cfg.k_at(&s), cfg.k_certain, "窄根该用 k_certain");

        // 只已知半手：根这一手随便抽两张就把"整堆已知"破了
        let mut half = s;
        half.n_draw_known = 3;
        assert!(!root_is_narrow(&half), "只已知 3 张不该算窄根");
        assert_eq!(cfg.k_at(&half), cfg.k, "宽根该退回 k");

        // 关掉之后一律用 k，和阶段 3b 之前逐字相同
        let mut off = cfg;
        off.k_certain = 0;
        assert_eq!(off.k_at(&s), off.k, "k_certain=0 该退回 k");
    }

    /// **`--set` / `--alt` 的键解析和一行说明住在库里，三个验收台共用一份。**
    ///
    /// 它坏掉的样子：写错一个键名而它默默不生效，得到的是"两条 arm 逐字相同"
    /// 这种看着很正常的读数。所以不认识的键必须**报错**。
    #[test]
    fn plan_overrides_round_trip() {
        use crate::plan::{Plan, K_CERTAIN, TT_BITS, WINDOW_CAP};
        let mut c = Plan::default();
        assert_eq!(c.k_certain, K_CERTAIN);
        assert_eq!(c.tt_bits, TT_BITS);
        // **默认三处同口径**：候选排序 / 深层选线 / 叶评估。
        // 阶段 3a 的全部内容就是把第一处从 `score` 挪到这一档上，
        // 而它是个**独立字段**（A/B 要单独扳得动它），所以默认值会不会
        // 悄悄跟 `deep_score` 走散，只有这一条守得住。
        assert_eq!(
            c.cand_score as usize, c.deep_score as usize,
            "默认配置里候选排序和深层选线该是同一个目标函数"
        );
        assert!(
            matches!(c.leaf, crate::plan::Leaf::Eval(f) if f as usize == c.deep_score as usize),
            "默认叶评估也该是同一个"
        );
        c.apply("cand-score=damage,k-certain=off,tt=off").expect("这三个键该认得");
        assert_eq!(c.k_certain, 0);
        assert_eq!(c.tt_bits, 0);
        // **先显式转成 `fn(...)` 再比地址** —— 直接拿函数项转 usize 转的是
        // 零大小类型的地址，编译器会警告，比出来的东西也没有意义。
        let addr = |f: fn(&State) -> i32| f as usize;
        assert_eq!(addr(c.cand_score), addr(crate::solver::score::damage_first));
        c.apply("k-certain=on,tt=on,window=on").expect("on 该认得");
        assert_eq!(c.k_certain, K_CERTAIN);
        assert_eq!(c.tt_bits, TT_BITS);
        assert_eq!(c.window, WINDOW_CAP);
        assert!(c.apply("cand-scroe=damage").is_err(), "键名写错了必须报错，不能默默忽略");
        assert!(c.apply("cand-score=nonsense").is_err(), "值不认识也要报错");
        // 说明里三个目标函数都要看得见 —— 它们各自不同正是这两轮改动的内容
        let d = c.describe();
        for want in ["线目标", "候选目标", "深层目标", "置换表", "窄根"] {
            assert!(d.contains(want), "describe 里少了 {want}：{d}");
        }
    }

    /// 斩杀延伸的乐观界要数**整个循环**的牌，不只手牌。
    ///
    /// 它坏掉的样子：只看手牌 —— 于是"下回合抽到那张究极打击就能斩杀"这件事
    /// 永远触发不了延伸，而那正是延伸最该管的局面。
    #[test]
    fn optimistic_damage_counts_the_whole_cycle() {
        let mut s = hand_of(2, 300, &[card::DEFEND], 3);
        s.n_draw = 0;
        s.n_disc = 0;
        // 一张大牌**压在弃牌堆**里（抽空了会洗回来，所以够得着）
        let big = s.add_card(card::ULTIMATE_STRIKE, 0, 0);
        s.n_draw -= 1;
        s.to_discard(big);
        let with = crate::solver::optimistic_damage(&s);

        let mut s2 = s;
        s2.n_disc = 0;
        let without = crate::solver::optimistic_damage(&s2);
        assert!(with > without, "弃牌堆里那张大牌该算进乐观界：{with} vs {without}");
    }

    /// 乐观界要吃力量加成 —— 不吃的话残局里"加了力量就能斩杀"看不出来。
    #[test]
    fn optimistic_damage_includes_strength() {
        let s = hand_of(3, 300, &[card::STRIKE, card::STRIKE], 3);
        let base = crate::solver::optimistic_damage(&s);
        let mut buff = s;
        buff.player.add(St::Strength, 5);
        assert!(
            crate::solver::optimistic_damage(&buff) > base,
            "力量该抬高乐观界"
        );
    }

    /// **延伸的预算要真的被查。** 两个方向都要看得见：
    /// 预算为 0 时就地评估、预算够时真的多搜了一层。
    ///
    /// 它坏掉的样子是最危险的那种：预算没被扣，每层都觉得自己该延伸 ——
    /// 搜索不报错，只是越跑越久，最后表现成"planner 卡住了"。
    #[test]
    fn extension_budget_is_actually_checked() {
        use crate::plan::{node_value_ext, want_extension, Plan};
        let mut cfg = Plan::default();
        cfg.depth = 2;
        // 找一个真的会触发延伸的叶局面
        let mut s = State::new(80, 21);
        s.add_enemy(enemy::DUMMY, 8); // 血薄 ⇒ 乐观界够得着 ⇒ 斩杀延伸
        for _ in 0..10 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let s = begin_combat(s);
        let leaf = crate::step::end_turn_before_draw(s);
        assert!(want_extension(&leaf, &cfg).is_some(), "这个局面本来就该触发延伸");

        let mut st0 = crate::plan::PlanStat::default();
        let no_budget = node_value_ext(&leaf, &cfg, 0, 1, 0, &mut st0);
        assert_eq!(no_budget, cfg.leaf.value(&leaf, cfg.seed, 1), "预算 0 就该就地评估");
        assert_eq!(st0.ext_fired, 0, "预算 0 还触发了延伸");

        let mut st2 = crate::plan::PlanStat::default();
        let with_budget = node_value_ext(&leaf, &cfg, 0, 1, 2, &mut st2);
        assert_ne!(with_budget, no_budget, "预算够就该真的多搜一层，值该不一样");
        assert!(st2.ext_fired > 0, "该记下延伸触发过");

        // **预算真的被扣了没有。** 这一条是这组测试里唯一能抓住
        // "忘了 `ext -= 1`"的：那种错不报错、不改上面那两个端点的值，
        // 只让搜索越钻越深。
        //
        // 要看得见它，局面得满足两条：**延伸一直触发** 且 **仗打不完**。
        // 前面那个 8 血假人不行 —— 敌人两下就死了，递归自己停了，
        // 少扣预算也看不出来（第一版就是这么放过去的）。
        // 这里改用洗牌边界（每个循环都会重新触发）+ 一个厚血敌人。
        // 要的是**斩杀触发一直成立、但仗一时半会打不完**：
        // 乐观界按"抽到最好的 5 张、能量管够"算 ⇒ 5 张打击 = 30 点，够得着
        // 28 血的敌人；而实际每回合只有 1 点能量、只打得出 1 张 ⇒ 要打好几回合。
        // 边界延伸做不到这一点（抽一手牌堆就掉出 5..10 那个区间，触发即停），
        // 第二版就是这么被放过去的。
        let mut cfg2 = Plan::default();
        cfg2.ext_lethal = true;
        cfg2.ext_boundary = false;
        let mut s2 = State::new(80, 33);
        s2.add_enemy(enemy::DUMMY, 28);
        for _ in 0..14 {
            s2.add_card(card::STRIKE, 0, 0);
        }
        let mut s2 = begin_combat(s2);
        s2.base_energy = 1;
        s2.energy = 1;
        let leaf2 = crate::step::end_turn_before_draw(s2);
        assert!(
            crate::plan::want_extension(&leaf2, &cfg2).is_some(),
            "这个局面该一直触发斩杀延伸"
        );
        let mut deep = crate::plan::PlanStat::default();
        node_value_ext(&leaf2, &cfg2, 0, 1, crate::plan::MAX_EXTEND, &mut deep);
        let cap = 1 + crate::plan::MAX_EXTEND as usize;
        assert!(
            (deep.max_depth as usize) <= cap,
            "递归到了第 {} 层，而最多借 {} 层 ⇒ 上限 {cap}（硬闸在 {}）",
            deep.max_depth,
            crate::plan::MAX_EXTEND,
            crate::plan::HARD_DEPTH_CAP
        );
    }

    /// 关掉三个开关，延伸就一次都不该触发。
    #[test]
    fn extensions_can_be_switched_off() {
        use crate::plan::{want_extension, Plan};
        let mut cfg = Plan::default();
        cfg.ext_lethal = false;
        cfg.ext_spike = false;
        cfg.ext_boundary = false;
        let mut s = State::new(80, 21);
        s.add_enemy(enemy::DUMMY, 8);
        for _ in 0..10 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let s = begin_combat(s);
        let leaf = crate::step::end_turn_before_draw(s);
        assert!(want_extension(&leaf, &cfg).is_none(), "全关了还触发");
    }

    /// 战鼓：自身被消耗时获得 2 能量（升级 3 能量）。
    #[test]
    fn drum_of_battle_grants_energy_on_exhaust() {
        let mut s = State::new(80, 77);
        s.add_enemy(enemy::DUMMY, 100);
        let drum = s.add_card(card::DRUM_OF_BATTLE, 0, 0);
        let drum_upg = s.add_card(card::DRUM_OF_BATTLE, F_UPGRADED, 0);
        for _ in 0..8 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        s.energy = 0;
        exhaust_card(&mut s, drum);
        assert_eq!(s.energy, 2, "战鼓消耗给 2 能量");

        exhaust_card(&mut s, drum_upg);
        assert_eq!(s.energy, 5, "战鼓+ 消耗给 3 能量");
    }

    /// 地道虫：钻地保留格挡，且破盾时被打进眩晕并清除钻地。
    #[test]
    fn tunneler_burrow_retains_block_and_break_stuns() {
        let mut s = State::new(80, 99);
        s.add_enemy(enemy::TUNNELER, 87);
        for _ in 0..10 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        s.enemies[0].block = 32;
        s.enemies[0].set(St::Burrowed, 1);
        s.enemy_move[0] = 2; // Move 2: 地底突袭

        // 回合结束，敌人回合开始时格挡不清零
        s = step(s, Action::EndTurn);
        assert_eq!(s.enemies[0].block, 32, "钻地期间敌人回合开始格挡不清零");

        // 我方打 6 点，格挡 32 -> 26，未破盾
        s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.enemies[0].block, 26);
        assert_eq!(s.enemies[0].get(St::Burrowed), 1);
        assert_eq!(s.enemy_move[0], 2);

        // 人工把格挡设为 5，再打 6 点击破格挡
        s.enemies[0].block = 5;
        s = step(s, Action::PlayCard { hand: 0, target: 0 });
        assert_eq!(s.enemies[0].block, 0);
        assert_eq!(s.enemies[0].get(St::Burrowed), 0, "破盾清除钻地");
        assert_eq!(s.enemy_move[0], 3, "破盾被打进眩晕（move 3）");
        assert_eq!(s.enemies[0].get(St::MoveForcedThisTurn), 1);
    }

    // -----------------------------------------------------------------
    // 进阶（`src/asc.rs`，生成产物）
    //
    // 四条守卫，各守一件**表和内核之间会悄悄错位**的事。生成器按
    // (种类, 低进阶值) 去认内核的 op，所以内核那边改一个数、插一条敌人，
    // 表就指错地方了 —— 而指错**不会报错**，只会在高进阶下算错。
    // -----------------------------------------------------------------

    /// A8 以下 `adjust` 必须逐字返回原 op —— 这是"既有对拍逐字节不变"的全部依据。
    #[test]
    fn ascension_below_8_is_a_no_op() {
        for r in crate::asc::ASC_OPS {
            let op = crate::content::ENEMIES[r.def as usize].moves[r.mv as usize].ops
                [r.op as usize];
            for a in 0..8u8 {
                assert_eq!(
                    format!("{:?}", crate::asc::adjust(r.def, r.mv as usize, r.op as usize, a, op)),
                    format!("{op:?}"),
                    "A{a} 下 {} 的「{}」被改了，而 A8 才是第一档",
                    r.name,
                    r.mv_name
                );
            }
        }
        for r in crate::asc::ASC_HP {
            for a in 0..8u8 {
                assert_eq!(crate::asc::hp_range(r.def, a), Some(r.low), "{} A{a}", r.name);
            }
        }
    }

    /// 表里的下标还指着它自己写的那只敌人 / 那一手。
    ///
    /// 有人往 `ENEMIES` 中间插一条，所有下标就整体平移了，而**平移不会报错**。
    #[test]
    fn asc_table_indices_still_point_at_the_named_enemy() {
        for r in crate::asc::ASC_HP {
            let d = &crate::content::ENEMIES[r.def as usize];
            assert_eq!(d.name, r.name, "ASC_HP 的下标 {} 指错了敌人", r.def);
        }
        for r in crate::asc::ASC_OPS {
            let d = &crate::content::ENEMIES[r.def as usize];
            assert_eq!(d.name, r.name, "ASC_OPS 的下标 {} 指错了敌人", r.def);
            let m = &d.moves[r.mv as usize];
            assert_eq!(m.name, r.mv_name, "{} 的第 {} 手指错了", r.name, r.mv);
            assert!(
                (r.op as usize) < m.ops.len(),
                "{} 的「{}」只有 {} 个 op，表里指到 {}",
                r.name,
                r.mv_name,
                m.ops.len(),
                r.op
            );
        }
    }

    /// **连接键还成立**：表里的 `low` 必须逐字等于内核那个 op 今天写着的数。
    ///
    /// 生成器就是靠这个值认出"这一行说的是哪个 op"的。内核改了数而没重新
    /// 生成表，这条会红 —— 否则高进阶下面那一档会被换成一个**不相干的值**。
    #[test]
    fn asc_ops_low_value_still_matches_the_kernel_op() {
        use crate::ops::EOp;
        for r in crate::asc::ASC_OPS {
            let op =
                crate::content::ENEMIES[r.def as usize].moves[r.mv as usize].ops[r.op as usize];
            let got = match (op, r.hits) {
                (EOp::Attack { hits, .. }, true) => hits,
                (EOp::Attack { base, .. }, false) => base,
                (EOp::AttackPlusStackHits { hits, .. }, true) => hits,
                (EOp::AttackPlusStackHits { base, .. }, false) => base,
                (EOp::AttackPlusSelfStatus { hits, .. }, true) => hits,
                (EOp::AttackPlusSelfStatus { base, .. }, false) => base,
                (EOp::Block(n), _) => n,
                (EOp::SelfStatus { amt, .. }, _) => amt,
                (EOp::PlayerStatus { amt, .. }, _) => amt,
                (EOp::AddCardToDiscard { count, .. }, _) => count,
                (EOp::Heal(n), _) => n,
                _ => panic!(
                    "{} 的「{}」第 {} 个 op 是 {:?}，`asc::patch` 认不出来 —— 该重新生成表",
                    r.name, r.mv_name, r.op, op
                ),
            };
            assert_eq!(
                got, r.low,
                "{} 的「{}」({}) 内核是 {got}，表里的低进阶值是 {} —— \
                 内核改过数而表没重新生成，跑 tools/dump_ascension.py",
                r.name, r.mv_name, r.prop, r.low
            );
        }
    }

    /// 内核那个**点值**血量必须落在 [源码] 的低进阶区间里。
    ///
    /// 两个来源独立：内核的数是 A1/A2 实测钉的，区间是反编译读的。
    /// 对不上说明其中一个错了，而两个都会让 L3 开出一场不存在的仗。
    #[test]
    fn asc_hp_low_range_contains_the_kernel_point_value() {
        for r in crate::asc::ASC_HP {
            let d = &crate::content::ENEMIES[r.def as usize];
            assert!(
                r.low.0 <= d.max_hp && d.max_hp <= r.low.1,
                "{} 内核血量 {}，[源码] 低进阶区间 {:?}",
                r.name,
                d.max_hp,
                r.low
            );
        }
    }

    /// **正面证据**：A9 之后小啃兽的撞击真的从 12 打到 13。
    ///
    /// 上面三条守的都是"别错位""别提前生效"，**没有一条证明它真的生效** ——
    /// 一张全是 no-op 的空表也能让那三条全绿。
    #[test]
    fn ascension_9_actually_raises_the_damage_that_lands() {
        let nibbit = crate::content::ENEMIES.iter().position(|d| d.name == "小啃兽").unwrap();
        let hit_at = |asc: u8| {
            let mut s = State::new(80, 1);
            s.ascension = asc;
            s.add_enemy(nibbit as u16, 45);
            let mut s = crate::step::begin_combat(s);
            s.enemy_move[0] = 0; // 撞击
            s = step(s, Action::EndTurn);
            80 - s.player.hp
        };
        // A7 走低进阶那一档，A9 走高进阶那一档。ButtDamage 12 -> 13。
        assert_eq!(hit_at(7), 12, "A7 该是低进阶值");
        assert_eq!(hit_at(8), 12, "血量那一档是 A8，伤害那一档是 A9 —— 别混");
        assert_eq!(hit_at(9), 13, "A9 `DeadlyEnemies` 之后撞击该打 13");
        assert_eq!(hit_at(10), 13);
        // 血量区间是另一档：A8 起 42–46 变 44–48
        assert_eq!(crate::asc::hp_range(nibbit as u16, 7), Some((42, 46)));
        assert_eq!(crate::asc::hp_range(nibbit as u16, 8), Some((44, 48)));
    }

    // ======================================================================
    // L3 阶段 1：合成战斗构造器（`src/synth.rs`）
    //
    // 验收台是 `bin/synth_audit`（拿实录第 0 帧当测试集）。下面这几条守的是
    // **审计台照不到的那半**：它只跑得了语料里出现过的组合，而这些是规矩。
    // ======================================================================

    /// `St::ALL` 的下标必须就是 `St::ix`。
    ///
    /// 它只用来起名字（诊断输出），**比较本身走 `0..N_STATUS` 的下标** ——
    /// 所以漏一条的代价只是印成 `status[87]`。这条守的是**顺序**：
    /// 顺序错了名字就会张冠李戴，那比印下标更糟。
    #[test]
    fn st_all_is_indexed_by_ix() {
        for (i, st) in St::ALL.iter().enumerate() {
            assert_eq!(st.ix(), i, "St::ALL 第 {i} 项是 {st:?}，它的 ix 是 {}", st.ix());
        }
        assert!(St::ALL.len() <= crate::state::N_STATUS);
    }

    /// `SYNTH_ONLY_GAPS` 里的 id 必须真在 `RELICS` 里、而且 `modelled: true`。
    ///
    /// 打错一个字的后果是这条**永远不会被报出来** —— 而这张表存在的全部意义
    /// 就是把"合成路径上还欠什么"报出来。`modelled: false` 的那些走
    /// `Gap::UnmodelledRelic`，重复挂在这里只会让同一件事报两遍。
    #[test]
    fn synth_only_gaps_name_real_relics() {
        for (id, why) in crate::content::SYNTH_ONLY_GAPS {
            let def = crate::content::relic_by_id(id)
                .unwrap_or_else(|| panic!("SYNTH_ONLY_GAPS 里的 {id} 不在 RELICS 表里"));
            assert!(
                def.modelled,
                "{id} 已经是 modelled: false 了，Gap::UnmodelledRelic 会报它 —— 挂在合成缺口表里等于报两遍"
            );
            assert!(!why.is_empty(), "{id} 得写清楚它在开局那一刻少做了什么");
        }
    }

    /// `ENEMY_START_PLAYER_STATUS` 里的名字必须解析得到真敌人。
    ///
    /// 它按**名字**索引（不是下标），所以表增删不会漂 —— 但拼错就是静默失效。
    #[test]
    fn enemy_start_player_status_names_resolve() {
        for (name, st, amt) in crate::content::ENEMY_START_PLAYER_STATUS {
            assert!(
                crate::replay::enemy_id(name).is_some(),
                "ENEMY_START_PLAYER_STATUS 里的「{name}」查不到敌人"
            );
            assert!(*amt != 0, "「{name}」给的 {st:?} 是 0 层，等于没写");
        }
    }

    /// `ENEMY_PRIVATE_MARKERS` 的名字解析得到真敌人，**而且每个标记都有规则读它** ——
    /// 否则标记挂上去什么都不做（两条路径都按名字挂，拼错就是静默失效）。
    #[test]
    fn enemy_private_markers_resolve_and_are_read_by_a_rule() {
        for (name, st, v) in crate::content::ENEMY_PRIVATE_MARKERS {
            assert!(crate::replay::enemy_id(name).is_some(), "ENEMY_PRIVATE_MARKERS 里的「{name}」查不到敌人");
            assert!(*v > 0, "「{name}」的标记 {st:?} 是 {v} 层 —— `fire` 不发 0 层的规则，等于没挂");
            assert!(
                crate::content::POWERS.iter().any(|p| p.st == *st),
                "「{name}」的标记 {st:?} 没有任何规则读它"
            );
        }
    }

    fn synth_deck(n: usize) -> Vec<crate::synth::DeckCard> {
        (0..n).map(|_| crate::synth::DeckCard::new(card::STRIKE, false)).collect()
    }

    /// 遗物的 `start_status` 要落到玩家身上，**而且第 1 回合的覆甲不掉层**。
    ///
    /// 后半句是 2026-09-09 由 `bin/synth_audit` 报出来的真 bug：
    /// [源码] `PlatingPower.AfterSideTurnStart` 对玩家那一支要求
    /// `TurnNumber != 1`，内核只给敌人那一支加了这个门。
    /// **对拍路径结构上看不见它** —— 那边每帧从观测重灌 `PLATING_POWER`，
    /// 而第 1 回合的 `TurnStart` 只有 `begin_combat` 跑得到。
    #[test]
    fn synth_hangs_relic_start_status_on_the_player() {
        let deck = synth_deck(10);
        let relics = [
            crate::synth::RelicSpec::new("VAJRA"),  // 力量 +1
            crate::synth::RelicSpec::new("GORGET"), // 覆甲 4
        ];
        let enemies = [crate::synth::EnemySpec::with_hp(enemy::DUMMY, 100)];
        let spec = crate::synth::FightSpec {
            relics: &relics,
            ..crate::synth::FightSpec::new(&deck, 80, &enemies, 7)
        };
        let built = crate::synth::build(&spec);
        assert_eq!(built.state.player.get(St::Strength), 1, "金刚杵的力量没挂上");
        assert_eq!(
            built.state.player.get(St::PlatedArmor),
            4,
            "护喉甲给 4 层覆甲，而第 1 回合不掉层（[源码] TurnNumber != 1）"
        );
        assert_eq!(built.state.turn, 1);
        assert_eq!(built.state.n_hand, 5, "开局发 5 张");
    }

    /// 人工制品要吃掉**触发器发出去的** debuff，不只是卡牌发出去的。
    ///
    /// [源码] `ArtifactPower.TryModifyPowerAmountReceived` 挂在接收方身上、
    /// 不问来源。[实测] 2026-09-09 `act3_f48_boss_aeonglass_2026-09-06` 第 0 帧：
    /// 永世沙漏是「人工制品 2 · 虚弱 0」（红面具那 1 层被吃掉了）。
    #[test]
    fn artifact_absorbs_a_debuff_handed_out_by_a_relic() {
        let deck = synth_deck(10);
        let relics = [crate::synth::RelicSpec::new("RED_MASK")];
        let aeon =
            crate::content::ENEMIES.iter().position(|d| d.name == "永世沙漏").unwrap() as u16;
        let enemies = [crate::synth::EnemySpec::with_hp(aeon, 512)];
        let spec = crate::synth::FightSpec {
            relics: &relics,
            ..crate::synth::FightSpec::new(&deck, 80, &enemies, 3)
        };
        let built = crate::synth::build(&spec);
        assert_eq!(built.state.enemies[0].get(St::Weak), 0, "虚弱该被人工制品吃掉");
        assert_eq!(built.state.enemies[0].get(St::Artifact), 2, "吃掉一次要掉一层");
    }

    /// 敌人开战时给**玩家**挂的 status（火箭的包围）。
    ///
    /// `EnemyDef::start_status` 只装挂自己身上的，这一类走
    /// `content::ENEMY_START_PLAYER_STATUS`，消费点在 `begin_combat`。
    #[test]
    fn an_enemy_can_hand_the_player_a_status_at_combat_start() {
        let deck = synth_deck(10);
        let rocket = crate::content::ENEMIES.iter().position(|d| d.name == "火箭").unwrap() as u16;
        let enemies = [crate::synth::EnemySpec::with_hp(rocket, 199)];
        let built = crate::synth::build(&crate::synth::FightSpec::new(&deck, 80, &enemies, 1));
        assert_eq!(built.state.player.get(St::Surrounded), 1, "火箭开局给玩家挂包围");
        assert_eq!(built.state.enemies[0].get(St::BackAttackRight), 1);
        assert_eq!(built.state.enemies[0].get(St::CrabRage), 1);
    }

    /// 血量掷点：**落在区间里**，而且**不动战斗内的三条随机流**。
    ///
    /// 后半句是 CRN 的前提（不变量 4 的同一条道理）：掷血量发生在开仗之前，
    /// 让它扰动 `rng.shuffle` 就等于"换一只敌人的血"会改掉整局的抽牌顺序。
    #[test]
    fn synth_rolls_enemy_hp_inside_the_range_without_touching_the_combat_rngs() {
        let nibbit =
            crate::content::ENEMIES.iter().position(|d| d.name == "小啃兽").unwrap() as u16;
        let (lo, hi) = crate::asc::hp_range(nibbit, 0).expect("小啃兽该有血量区间");
        let deck = synth_deck(10);
        let mut seen_low = false;
        let mut seen_high = false;
        for seed in 0..64u64 {
            let enemies = [crate::synth::EnemySpec::rolled(nibbit)];
            let built =
                crate::synth::build(&crate::synth::FightSpec::new(&deck, 80, &enemies, seed));
            let hp = built.state.enemies[0].max_hp;
            assert!(hp >= lo && hp <= hi, "掷出来的 {hp} 不在区间 [{lo},{hi}] 里");
            seen_low |= hp == lo;
            seen_high |= hp == hi;
            // 同一个种子、只把血量从"掷"换成"给定"，抽牌顺序必须逐字节相同
            let fixed = [crate::synth::EnemySpec::with_hp(nibbit, hp)];
            let same = crate::synth::build(&crate::synth::FightSpec::new(&deck, 80, &fixed, seed));
            assert_eq!(
                built.state.hand, same.state.hand,
                "掷血量扰动了洗牌流 —— CRN 配对比较会当场失效"
            );
            assert_eq!(built.state.rng, same.state.rng);
        }
        assert!(seen_low && seen_high, "64 个种子该把区间两端都掷到");
    }

    /// 同种子 + 同 spec ⇒ 逐字节相同（不变量 4 在 L3 这一侧的形状）。
    #[test]
    fn synth_is_deterministic_given_the_seed() {
        let deck = synth_deck(12);
        let enemies = [crate::synth::EnemySpec::rolled(enemy::NIBBIT)];
        let a = crate::synth::build(&crate::synth::FightSpec::new(&deck, 80, &enemies, 99));
        let b = crate::synth::build(&crate::synth::FightSpec::new(&deck, 80, &enemies, 99));
        assert_eq!(a.state, b.state);
    }

    /// 构造器**知道自己缺了什么**：认不出的遗物、没建模的遗物、
    /// 只在合成路径上欠的遗物、没给的跨战斗计数器、内核不认识的牌，各报各的。
    #[test]
    fn synth_reports_every_gap_it_cannot_fill() {
        use crate::synth::Gap;
        let deck = vec![
            crate::synth::DeckCard::new(card::STRIKE, false),
            crate::synth::DeckCard::new(card::UNKNOWN, false),
        ];
        let relics = [
            crate::synth::RelicSpec::new("NO_SUCH_RELIC"),
            crate::synth::RelicSpec::new("REPTILE_TRINKET"), // modelled: false
            crate::synth::RelicSpec::new("PETRIFIED_TOAD"),    // 只在合成路径上欠
            crate::synth::RelicSpec::new("PENDULUM"),          // 有计数器，没给值
        ];
        let enemies = [crate::synth::EnemySpec::rolled(enemy::DUMMY)]; // 假人没有血量区间
        let spec = crate::synth::FightSpec {
            relics: &relics,
            ..crate::synth::FightSpec::new(&deck, 80, &enemies, 5)
        };
        let built = crate::synth::build(&spec);
        let has = |f: &dyn Fn(&Gap) -> bool| built.gaps.iter().any(|g| f(g));
        assert!(has(&|g| matches!(g, Gap::UnknownCard { ix: 1 })), "{:?}", built.gaps);
        assert!(has(&|g| matches!(g, Gap::UnknownRelic(id) if id == "NO_SUCH_RELIC")));
        assert!(has(&|g| matches!(g, Gap::UnmodelledRelic(id) if id == "REPTILE_TRINKET")));
        assert!(has(&|g| matches!(g, Gap::SynthUnmodelledRelic { id, .. } if id == "PETRIFIED_TOAD")));
        assert!(has(&|g| matches!(g, Gap::RelicCounterMissing(id) if id == "PENDULUM")));
        assert!(has(&|g| matches!(g, Gap::NoHpRange { .. })));
        assert!(!built.clean());
    }

    /// 牌组进去多少张，开局手牌 + 抽牌堆就该是多少张（多重集不变）。
    #[test]
    fn the_deck_goes_in_whole() {
        let deck: Vec<_> = (0..9)
            .map(|i| {
                crate::synth::DeckCard::new(
                    if i % 3 == 0 { card::DEFEND } else { card::STRIKE },
                    i % 2 == 0,
                )
            })
            .collect();
        let enemies = [crate::synth::EnemySpec::with_hp(enemy::DUMMY, 50)];
        let built = crate::synth::build(&crate::synth::FightSpec::new(&deck, 80, &enemies, 42));
        let s = &built.state;
        assert_eq!(s.n_cards as usize, deck.len());
        assert_eq!((s.n_hand + s.n_draw) as usize, deck.len(), "牌只该在手牌和抽牌堆里");
        assert_eq!(s.n_disc, 0);
        assert_eq!(s.n_exh, 0);
        let mut want: Vec<_> = deck.iter().map(|c| (c.id, c.flags)).collect();
        let mut got: Vec<_> =
            (0..s.n_cards as usize).map(|i| (s.cards[i].id, s.cards[i].flags)).collect();
        want.sort();
        got.sort();
        assert_eq!(want, got, "多重集变了 —— 洗牌不该改牌的身份");
    }

    /// 两件遗物给**同一个**私有 status 时，`sync` 那条路要相加，不是后一条盖前一条。
    ///
    /// [实测] 2026-09-09 `act3_f46_soul_nexus` 第 0 帧玩家格挡 **14** = 锚 10 + 假锚 4。
    /// 合成那条路一直是 `add`（`step::grant_relic`），`bin/synth_audit`
    /// 就是拿这个把两条路的分歧比出来的。
    #[test]
    fn two_relics_granting_the_same_status_stack_in_sync() {
        let mk = |id: &str| crate::replay::RelicObs {
            id: id.to_string(),
            name: id.to_string(),
            counter: None,
        };
        let obs = crate::replay::Obs {
            round: 1,
            hp: 59,
            max_hp: 75,
            relics: vec![mk("ANCHOR"), mk("FAKE_ANCHOR")],
            ..Default::default()
        };
        let mut r = crate::replay::Replayer::new("test");
        let synced = r.sync(&obs);
        assert_eq!(
            synced.state.player.get(St::Anchor),
            14,
            "锚 10 + 假锚 4 = 14；后一条盖掉前一条的话这里会是 4 或 10"
        );
    }


    // ---- 2026-09-09 第三批遗物（合成审计台照出来的那六件 + 损毁头盔）----

    /// 斗篷扣：回合结束时**每张手牌**给 1 点格挡。
    ///
    /// [源码] `CloakClasp.BeforeSideTurnEnd` = `(int)(cards.Count × Block)`，
    /// 时点在**弃手牌之前** —— 数的是"我没打出去的那几张"。
    #[test]
    fn cloak_clasp_blocks_per_card_left_in_hand() {
        let build = |cards_in_hand: usize| -> i32 {
            let mut s = State::new(80, 3);
            s.add_enemy(enemy::DUMMY, 100);
            for _ in 0..10 {
                s.add_card(card::STRIKE, 0, 0);
            }
            let mut s = begin_combat(s);
            s.player.set(St::CloakClasp, 1);
            // 打掉几张，手上就少几张
            for _ in 0..(5 - cards_in_hand) {
                s = step(s, Action::PlayCard { hand: 0, target: 0 });
            }
            let mut inc = NO_INCOMING;
            inc[0] = (30, 1); // 敌人打 30，格挡挡下几点看得出来
            let after = end_turn_with_incoming(s, &inc);
            80 - after.player.hp
        };
        // 手上 5 张 -> 5 点格挡 -> 挨 25；手上 2 张 -> 2 点 -> 挨 28
        assert_eq!(build(5), 25, "5 张手牌该给 5 点格挡");
        assert_eq!(build(2), 28, "2 张手牌该给 2 点格挡");
    }

    /// 号角靴钉：**只在第 2 回合**开始给 14 点格挡（清完格挡之后）。
    #[test]
    fn horn_cleat_fires_only_on_turn_two() {
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::DUMMY, 100);
        for _ in 0..20 {
            s.add_card(card::STRIKE, 0, 0);
        }
        let mut s = begin_combat(s);
        s.player.set(St::HornCleat, 14);
        assert_eq!(s.player.block, 0, "第 1 回合不给");
        let s = end_turn_with_incoming(s, &NO_INCOMING);
        assert_eq!(s.turn, 2);
        assert_eq!(s.player.block, 14, "第 2 回合开始给 14 点");
        let s = end_turn_with_incoming(s, &NO_INCOMING);
        assert_eq!(s.turn, 3);
        assert_eq!(s.player.block, 0, "只给一次");
    }

    /// 损毁头盔：本场**第一次**获得力量时翻倍，之后不再触发。
    ///
    /// [实测] 2026-09-09 `act3_f48_boss_aeonglass_2026-09-06` 第 0 帧：
    /// 身上金刚杵（开局 +1 力量）+ 损毁头盔，观测到的力量是 **2**。
    #[test]
    fn ruined_helmet_doubles_the_first_strength_gain_only() {
        // 开局那条路（`grant_relics` 的第二趟）
        let deck: Vec<_> =
            (0..10).map(|_| crate::synth::DeckCard::new(card::STRIKE, false)).collect();
        let enemies = [crate::synth::EnemySpec::with_hp(enemy::DUMMY, 100)];
        // **故意把金刚杵写在损毁头盔前面**：两趟的意义就在于顺序无关
        let relics = [
            crate::synth::RelicSpec::new("VAJRA"),
            crate::synth::RelicSpec::new("RUINED_HELMET"),
        ];
        let spec = crate::synth::FightSpec {
            relics: &relics,
            ..crate::synth::FightSpec::new(&deck, 80, &enemies, 11)
        };
        let built = crate::synth::build(&spec);
        assert_eq!(built.state.player.get(St::Strength), 2, "金刚杵 1 点该被翻成 2");
        assert_eq!(built.state.player.get(St::RuinedHelmet), 0, "用掉就清零");

        // 卡牌那条路（`apply_status`），而且只翻第一次
        let mut s = State::new(80, 3);
        s.add_enemy(enemy::DUMMY, 100);
        for _ in 0..6 {
            s.add_card(card::COMBUST, 0, 0); // 燃烧：+2 力量
        }
        let mut s = begin_combat(s);
        s.player.set(St::RuinedHelmet, 1);
        let before = s.player.get(St::Strength);
        let ix = hand_ix_of(&s, card::COMBUST);
        let s = step(s, Action::PlayCard { hand: ix, target: 0 });
        assert_eq!(s.player.get(St::Strength), before + 4, "第一次 +2 该翻成 +4");
        let ix = hand_ix_of(&s, card::COMBUST);
        let s = step(s, Action::PlayCard { hand: ix, target: 0 });
        assert_eq!(s.player.get(St::Strength), before + 6, "第二次照常 +2");
    }

    /// 古茶具：**只有刚休息过**才给那 +2 能量，而且只给第 1 回合。
    ///
    /// 它欠的「上一个房间是不是休息处」不在 L1 里，但**在 L3 手上** ——
    /// 所以走 `FightSpec::after_rest` 这个输入，而不是内核去猜。
    /// 假货给 1 点（[源码] 真品 `EnergyVar(2)`、假货 1）。
    #[test]
    fn the_tea_set_only_pays_out_after_a_rest() {
        let deck: Vec<_> =
            (0..10).map(|_| crate::synth::DeckCard::new(card::STRIKE, false)).collect();
        let enemies = [crate::synth::EnemySpec::with_hp(enemy::DUMMY, 100)];
        let energy_at_start = |id: &str, after_rest: bool| -> i32 {
            let relics = [crate::synth::RelicSpec::new(id)];
            let spec = crate::synth::FightSpec {
                relics: &relics,
                after_rest,
                ..crate::synth::FightSpec::new(&deck, 80, &enemies, 4)
            };
            crate::synth::build(&spec).state.energy
        };
        assert_eq!(energy_at_start("VENERABLE_TEA_SET", false), 3, "没休息过就是 3 点");
        assert_eq!(energy_at_start("VENERABLE_TEA_SET", true), 5, "刚休息过 +2");
        assert_eq!(energy_at_start("FAKE_VENERABLE_TEA_SET", true), 4, "假货只给 1 点");
        // 第 2 回合不再给（标记用掉就清）
        let relics = [crate::synth::RelicSpec::new("VENERABLE_TEA_SET")];
        let spec = crate::synth::FightSpec {
            relics: &relics,
            after_rest: true,
            ..crate::synth::FightSpec::new(&deck, 80, &enemies, 4)
        };
        let s = crate::synth::build(&spec).state;
        let s = end_turn_with_incoming(s, &NO_INCOMING);
        assert_eq!(s.energy, 3, "只兑现一次");
    }

    /// 新进表的六件里，三件是纯局外的 —— 它们**不该**挂任何战斗层状态。
    ///
    /// 这条守的是"别为了看起来建好了而乱挂 status"：局外遗物的产物
    /// （牌组、最大生命、金币）本来就在 L3 的输入里。
    #[test]
    fn the_three_outside_combat_relics_hang_nothing() {
        for id in ["LEAFY_POULTICE", "FROZEN_EGG", "SIGNET_RING"] {
            let def = crate::content::relic_by_id(id).unwrap();
            assert!(def.start_status.is_empty() && def.private_status.is_empty());
            assert!(def.counter_to.is_none());
            assert!(def.modelled, "{id} 是局外遗物，战斗层不欠它什么");
            assert!(
                crate::content::synth_gap(id).is_none(),
                "{id} 的产物已经在 L3 的输入里（牌组/最大生命/金币），不是合成缺口"
            );
        }
    }


    // ---- 2026-09-09 批 3：开局手牌那一类（风箱 / 骨茶 / 宝石面具 / 碎石者 / 小血瓶）----

    fn synth_fight(relics: &[&str], deck: &[crate::synth::DeckCard], seed: u64) -> State {
        let specs: Vec<_> = relics.iter().map(|id| crate::synth::RelicSpec::new(id)).collect();
        let enemies = [crate::synth::EnemySpec::with_hp(enemy::DUMMY, 100)];
        let spec = crate::synth::FightSpec {
            relics: &specs,
            ..crate::synth::FightSpec::new(deck, 80, &enemies, seed)
        };
        crate::synth::build(&spec).state
    }

    /// 风箱：开局把**手牌**升级 —— 而且必须在**抽牌之后**。
    ///
    /// [源码] `Bellows.AfterPlayerTurnStart` + `TurnNumber <= 1`。
    /// 挂在 `TurnStart` 上的话它会去升级一手还没发下来的空牌，
    /// 所以钩子是 `HandDrawn`。这条测试就是那个时序的判据。
    #[test]
    fn bellows_upgrades_the_opening_hand_after_the_draw() {
        let deck: Vec<_> =
            (0..12).map(|_| crate::synth::DeckCard::new(card::STRIKE, false)).collect();
        let s = synth_fight(&["BELLOWS"], &deck, 7);
        assert_eq!(s.n_hand, 5);
        for i in 0..s.n_hand as usize {
            assert!(s.cards[s.hand[i] as usize].upgraded(), "手上这 5 张该全是 +");
        }
        // 抽牌堆里的**不**升级 —— 它只管手牌
        let un = (0..s.n_draw as usize).filter(|&i| !s.cards[s.draw[i] as usize].upgraded()).count();
        assert_eq!(un, s.n_draw as usize, "抽牌堆里那 7 张一张都不该升");
        // 没有风箱就一张都不升
        let plain = synth_fight(&[], &deck, 7);
        assert!(
            (0..plain.n_hand as usize).all(|i| !plain.cards[plain.hand[i] as usize].upgraded()),
            "没风箱不该有 +"
        );
    }

    /// 碎石者：开局把**抽牌堆**里随机 2 张可升级的升掉（不动手牌）。
    #[test]
    fn stone_cracker_upgrades_two_in_the_draw_pile() {
        let deck: Vec<_> =
            (0..12).map(|_| crate::synth::DeckCard::new(card::STRIKE, false)).collect();
        let s = synth_fight(&["STONE_CRACKER"], &deck, 5);
        let up = (0..s.n_cards as usize).filter(|&i| s.cards[i].upgraded()).count();
        assert_eq!(up, 2, "[源码] `Take(2)`");
        // 升的是抽牌堆里的：**规则在抽牌之前跑**，所以升过的那两张可能被抽上来。
        // 判据只看总数 —— 位置由洗牌决定，不该钉。
    }

    /// 宝石面具：开局从抽牌堆挑一张**能力牌**进手牌，并让它本回合免费。
    ///
    /// [源码] `JeweledMask.BeforeHandDraw` —— **抽牌之前**，所以手牌是 5+1。
    /// 卡面文本写的是"本场战斗"，而源码那个方法叫 `SetToFreeThisTurn`：照源码。
    #[test]
    fn jeweled_mask_pulls_a_free_power_before_the_draw() {
        let mut deck: Vec<_> =
            (0..12).map(|_| crate::synth::DeckCard::new(card::STRIKE, false)).collect();
        deck.push(crate::synth::DeckCard::new(card::PYRE, false)); // 唯一一张能力牌
        let s = synth_fight(&["JEWELED_MASK"], &deck, 3);
        assert_eq!(s.n_hand, 6, "抽 5 张之前先塞进来一张 ⇒ 6 张");
        let ix = (0..s.n_hand as usize)
            .find(|&i| s.cards[s.hand[i] as usize].id == card::PYRE)
            .expect("那张能力牌该在手上");
        assert!(
            s.cards[s.hand[ix] as usize].flags & crate::state::F_FREE_THIS_TURN != 0,
            "它该被标成本回合免费"
        );
        // 牌堆里一张能力牌都没有时什么都不做
        let plain: Vec<_> =
            (0..12).map(|_| crate::synth::DeckCard::new(card::STRIKE, false)).collect();
        assert_eq!(synth_fight(&["JEWELED_MASK"], &plain, 3).n_hand, 5);
    }

    /// 小血瓶：开局回 2 血（假货 1），**只回第 1 回合那一次**，而且顶到上限为止。
    #[test]
    fn blood_vial_heals_at_the_start_of_combat_only() {
        let deck: Vec<_> =
            (0..12).map(|_| crate::synth::DeckCard::new(card::STRIKE, false)).collect();
        let heal = |id: &str, hp: i32| -> i32 {
            let specs = [crate::synth::RelicSpec::new(id)];
            let enemies = [crate::synth::EnemySpec::with_hp(enemy::DUMMY, 100)];
            let spec = crate::synth::FightSpec {
                relics: &specs,
                max_hp: 80,
                ..crate::synth::FightSpec::new(&deck, hp, &enemies, 2)
            };
            crate::synth::build(&spec).state.player.hp
        };
        assert_eq!(heal("BLOOD_VIAL", 60), 62);
        assert_eq!(heal("FAKE_BLOOD_VIAL", 60), 61, "假货只回 1 点");
        assert_eq!(heal("BLOOD_VIAL", 79), 80, "顶到上限为止");
        // 第 2 回合不再回
        let specs = [crate::synth::RelicSpec::new("BLOOD_VIAL")];
        let enemies = [crate::synth::EnemySpec::with_hp(enemy::DUMMY, 100)];
        let spec = crate::synth::FightSpec {
            relics: &specs,
            max_hp: 80,
            ..crate::synth::FightSpec::new(&deck, 60, &enemies, 2)
        };
        let s = crate::synth::build(&spec).state;
        let s = end_turn_with_incoming(s, &NO_INCOMING);
        assert_eq!(s.player.hp, 62, "只回一次");
    }

    /// 缩放仪：**只有 Boss 房**才回那 25 血。
    #[test]
    fn pantograph_only_heals_in_a_boss_room() {
        let deck: Vec<_> =
            (0..12).map(|_| crate::synth::DeckCard::new(card::STRIKE, false)).collect();
        let hp_after = |boss: bool| -> i32 {
            let specs = [crate::synth::RelicSpec::new("PANTOGRAPH")];
            let enemies = [crate::synth::EnemySpec::with_hp(enemy::DUMMY, 100)];
            let spec = crate::synth::FightSpec {
                relics: &specs,
                max_hp: 80,
                boss_room: boss,
                ..crate::synth::FightSpec::new(&deck, 40, &enemies, 2)
            };
            crate::synth::build(&spec).state.player.hp
        };
        assert_eq!(hp_after(false), 40, "普通房不给");
        assert_eq!(hp_after(true), 65, "Boss 房 +25");
    }

    /// 骨茶：**局外计数器 > 0** 才武装，而且和风箱共用同一个 status。
    #[test]
    fn bone_tea_arms_only_when_the_caller_says_it_has_combats_left() {
        let deck: Vec<_> =
            (0..12).map(|_| crate::synth::DeckCard::new(card::STRIKE, false)).collect();
        let upgraded_in_hand = |counter: Option<i32>| -> usize {
            let specs = [crate::synth::RelicSpec { id: "BONE_TEA", counter }];
            let enemies = [crate::synth::EnemySpec::with_hp(enemy::DUMMY, 100)];
            let spec = crate::synth::FightSpec {
                relics: &specs,
                ..crate::synth::FightSpec::new(&deck, 80, &enemies, 9)
            };
            let s = crate::synth::build(&spec).state;
            (0..s.n_hand as usize).filter(|&i| s.cards[s.hand[i] as usize].upgraded()).count()
        };
        assert_eq!(upgraded_in_hand(Some(2)), 5, "还剩 2 场 ⇒ 升级整手");
        assert_eq!(upgraded_in_hand(Some(0)), 0, "用完了就不给");
        assert_eq!(upgraded_in_hand(None), 0, "拿不到就当没武装（低估自己）");
    }

    /// 拿走抽牌堆中间一张时，**已知前缀要跟着截**。
    ///
    /// `n_draw_known` 数的是末尾那几张，而 `take_from_draw` 把它上面那一段
    /// 整体前移了 —— 不截的话 planner 会以为自己知道一批**已经换了位置**的牌。
    #[test]
    fn taking_a_card_out_of_the_draw_pile_truncates_the_known_prefix() {
        let mut s = State::new(80, 1);
        for _ in 0..6 {
            s.add_card(card::STRIKE, 0, 0);
        }
        // 只知道顶上两张（下标 4、5）
        s.n_draw_known = 2;
        let n = s.n_draw;
        // 从**已知区下面**（下标 2）拿走：其余每一张的身份都没变 ⇒ 已知数不动
        s.take_from_draw(2);
        assert_eq!(s.n_draw, n - 1);
        assert_eq!(s.n_draw_known, 2, "抽走已知区下面的牌不该让已知区缩水");
        // 从**已知区里**（末尾）拿走：少了一张已知的 ⇒ −1
        let mut s2 = State::new(80, 1);
        for _ in 0..6 {
            s2.add_card(card::STRIKE, 0, 0);
        }
        s2.n_draw_known = 2;
        s2.take_from_draw(5);
        assert_eq!(s2.n_draw_known, 1);
    }


    // ======================================================================
    // L3 阶段 2：单场评估（`src/synth/eval.rs`）
    //
    // 验收台是 `bin/fight_eval`（拿实录第 0 帧当测试集，判据是**截断率 0**）。
    // 下面这几条守的是台子照不到的那半：它只跑得了语料里出现过的组合，
    // 而这些是规矩 —— 尤其是「截断的样本不许进分布」和 CRN 那一条。
    // ======================================================================

    fn eval_deck(n: usize) -> Vec<crate::synth::DeckCard> {
        (0..n)
            .map(|i| crate::synth::DeckCard::new(
                if i % 2 == 0 { card::STRIKE } else { card::DEFEND },
                false,
            ))
            .collect()
    }

    /// **认不出敌人就拒绝作答**，而不是给一个自信的 0% 死亡率。
    ///
    /// 推演靠 `EnemyDef::moves` 推敌人每一手，`enemy::UNKNOWN` 唯一的一手是
    /// `EOp::Nothing` —— 那是**一场敌人不出手的仗**（12 血打 200 血一滴不掉，
    /// 见 `rollout` 模块头）。这条要是松了，L3 会对每一场它看不懂的仗报「稳赢」。
    #[test]
    fn eval_refuses_when_it_cannot_predict_the_enemy() {
        use crate::synth::eval::{evaluate, EvalCfg, Refusal};
        let deck = eval_deck(10);
        let enemies = [crate::synth::EnemySpec::with_hp(enemy::UNKNOWN, 50)];
        let spec = crate::synth::FightSpec::new(&deck, 80, &enemies, 7);
        let ev = evaluate(&spec, &EvalCfg { samples: 8, ..EvalCfg::default() });
        assert_eq!(ev.refused, Some(Refusal::CannotRollout));
        assert_eq!(ev.n, 0, "拒绝作答时一个数都不许报");
        assert_eq!(ev.hp_end.n, 0);
        assert!(ev.samples.is_empty());
    }

    /// **CRN**：两副不同的牌组，样本 `i` 上看到的敌人血量必须一样。
    ///
    /// L3 问的是"加这张牌好多少"，那个差要在同一批随机上量。
    /// `synth::hp_stream` 只吃 `spec.seed`，而 `eval::sample_seed` 只吃
    /// `(seed, i)` —— 牌组进不来。这条断了的话，"多一张牌"会顺手换掉敌人的血，
    /// 两个候选比的就不是同一场仗了。
    #[test]
    fn two_decks_see_the_same_enemy_hp_on_the_same_sample() {
        use crate::synth::eval::sample_seed;
        let a = eval_deck(10);
        let b = eval_deck(14); // 多四张牌
        let enemies = [crate::synth::EnemySpec::rolled(enemy::NIBBIT)];
        let mut seen = Vec::new();
        for i in 0..8 {
            let sa = crate::synth::FightSpec::new(&a, 80, &enemies, sample_seed(999, i));
            let sb = crate::synth::FightSpec::new(&b, 80, &enemies, sample_seed(999, i));
            let (ba, bb) = (crate::synth::build(&sa), crate::synth::build(&sb));
            assert_eq!(
                ba.state.enemies[0].max_hp, bb.state.enemies[0].max_hp,
                "样本 {i}：换了牌组敌人血量就变了 —— CRN 断了"
            );
            seen.push(ba.state.enemies[0].max_hp);
        }
        seen.dedup();
        assert!(seen.len() > 1, "八个样本掷出同一个血量 —— 血量那条方差没被采到");
    }

    /// 同一个 spec 跑两遍，**逐样本逐字段相同**。
    ///
    /// 并行是按下标写回原位的（`rollout::par_map`），顺序一变分位数就不可复现。
    #[test]
    fn eval_is_deterministic_given_the_seed() {
        use crate::synth::eval::{evaluate, EvalCfg};
        let deck = eval_deck(12);
        let enemies = [crate::synth::EnemySpec::rolled(enemy::NIBBIT)];
        let spec = crate::synth::FightSpec::new(&deck, 80, &enemies, 4242);
        let cfg = EvalCfg { samples: 16, ..EvalCfg::default() };
        let a = evaluate(&spec, &cfg);
        let b = evaluate(&spec, &cfg);
        assert_eq!(a.samples, b.samples);
        assert_eq!(a.deaths, b.deaths);
        assert_eq!(a.hp_end, b.hp_end);
    }

    /// **截断的样本既不进血量分布、也不进死亡数。**
    ///
    /// 撞上回合上限的推演战斗没有分出胜负，而它照样带着一个血量 ——
    /// 把那个数读成"活着结束"正是 `bin/rollout` 的 P4 在量的事。
    /// 这里把上限压到 1 个回合，制造一批必然截断的样本。
    #[test]
    fn truncated_samples_stay_out_of_the_distributions() {
        use crate::synth::eval::{evaluate, EvalCfg};
        let deck = eval_deck(12);
        // 假人 600 血：一个回合绝对打不完
        let enemies = [crate::synth::EnemySpec::with_hp(enemy::DUMMY, 600)];
        let spec = crate::synth::FightSpec::new(&deck, 80, &enemies, 11);
        let ev = evaluate(&spec, &EvalCfg { samples: 8, max_turns: 1, ..EvalCfg::default() });
        assert_eq!(ev.truncated, 8, "一个回合打不完 600 血");
        assert_eq!(ev.deaths, 0);
        assert_eq!(ev.hp_end.n, 0, "截断的样本不许进血量分布");
        assert_eq!(ev.loss.n, 0);
        assert_eq!(ev.turns.n, 0);
        assert!(ev.truncation_rate() > 0.99);
    }

    /// 配对比较**必须是同一批样本**，对不上就返回 `None` 而不是硬算。
    #[test]
    fn paired_delta_refuses_two_different_batches() {
        use crate::synth::eval::{evaluate, paired_delta, EvalCfg};
        let deck = eval_deck(12);
        let enemies = [crate::synth::EnemySpec::with_hp(enemy::NIBBIT, 46)];
        let spec = crate::synth::FightSpec::new(&deck, 80, &enemies, 5);
        let a = evaluate(&spec, &EvalCfg { samples: 8, ..EvalCfg::default() });
        let b = evaluate(&spec, &EvalCfg { samples: 12, ..EvalCfg::default() });
        assert!(paired_delta(&a, &b).is_none(), "样本数不同就不是同一批随机");
        // 种子基不同也不许配对 —— 那时算出来的"差"是两份噪声相减
        let other_seed =
            evaluate(&crate::synth::FightSpec::new(&deck, 80, &enemies, 6), &EvalCfg { samples: 8, ..EvalCfg::default() });
        assert!(paired_delta(&a, &other_seed).is_none(), "种子基不同就不是同一批随机");
        // 同一批：自己和自己比，逐样本差必须全是 0
        let c = evaluate(&spec, &EvalCfg { samples: 8, ..EvalCfg::default() });
        let d = paired_delta(&a, &c).expect("同一个 spec 同一个样本数");
        assert_eq!(d.pairs, 8);
        assert_eq!(d.hp.min, 0);
        assert_eq!(d.hp.max, 0);
        assert_eq!(d.deaths_delta, 0);
    }

    /// **升级过的牌组不许更差** —— 配对之后逐样本比。
    ///
    /// 这是阶段 3 那条单调性自检的单场版本。它**不是**统计检验：
    /// 打击 5 -> 打击+ 8 是严格更强的一张牌，同一批随机下不该出现
    /// "整体更差"。方向反了通常意味着评估口径写反了（比如把战损当成了收益）。
    #[test]
    fn upgrading_every_strike_never_reads_worse() {
        use crate::synth::eval::{evaluate, paired_delta, EvalCfg};
        let base = eval_deck(12);
        let upg: Vec<_> = base
            .iter()
            .map(|c| crate::synth::DeckCard::new(c.id, c.id == card::STRIKE))
            .collect();
        let enemies = [crate::synth::EnemySpec::with_hp(enemy::NIBBIT, 46)];
        let cfg = EvalCfg { samples: 32, ..EvalCfg::default() };
        let a = evaluate(&crate::synth::FightSpec::new(&base, 60, &enemies, 8), &cfg);
        let b = evaluate(&crate::synth::FightSpec::new(&upg, 60, &enemies, 8), &cfg);
        let d = paired_delta(&a, &b).expect("同一个种子基、同一个样本数");
        assert!(d.deaths_delta <= 0, "升级之后死得更多：{}", d.deaths_delta);
        assert!(
            d.hp.mean >= 0.0,
            "升级之后平均终点血量更低（{:.2}）—— 更强 {} / 更弱 {}",
            d.hp.mean,
            d.b_better,
            d.a_better
        );
    }

    /// **已知偏差是从这副牌组自己算出来的**，不是一段写死的免责文本。
    #[test]
    fn caveats_are_derived_from_the_deck_not_hardcoded() {
        use crate::synth::eval::{evaluate, Caveat, EvalCfg};
        let cfg = EvalCfg { samples: 4, ..EvalCfg::default() };
        let enemies = [crate::synth::EnemySpec::with_hp(enemy::NIBBIT, 46)];
        let plain = eval_deck(10);
        let ev = evaluate(&crate::synth::FightSpec::new(&plain, 80, &enemies, 1), &cfg);
        assert!(
            !ev.caveats().iter().any(|c| matches!(c, Caveat::EngineCards(_))),
            "一张能力牌都没有，不该报引擎牌那一条"
        );
        assert!(!ev.caveats().iter().any(|c| matches!(c, Caveat::PotionsUnused(_))));
        // 掺一张能力牌进去，那一条就该出现
        let mut with_power = plain.clone();
        with_power.push(crate::synth::DeckCard::new(card::DEMON_FORM, false));
        let potions = [crate::state::potion::FIRE, 0, 0];
        let spec = crate::synth::FightSpec {
            potions: &potions,
            ..crate::synth::FightSpec::new(&with_power, 80, &enemies, 1)
        };
        let ev = evaluate(&spec, &cfg);
        assert!(ev.caveats().iter().any(|c| matches!(c, Caveat::EngineCards(1))));
        assert!(ev.caveats().iter().any(|c| matches!(c, Caveat::PotionsUnused(1))));
        // **A8 以上**那条只在高进阶出现：整张进阶表是 [源码] 档，
        // 而目标是 A10 —— 这一栏必须在读数旁边，不能等到出结果才想起来。
        assert!(!ev.caveats().iter().any(|c| matches!(c, Caveat::SourceOnlyAscension(_))));
        let a10 = crate::synth::FightSpec {
            ascension: 10,
            ..crate::synth::FightSpec::new(&plain, 80, &enemies, 1)
        };
        let ev = evaluate(&a10, &cfg);
        assert!(ev.caveats().iter().any(|c| matches!(c, Caveat::SourceOnlyAscension(10))));
    }

    // ----------------------------------------------------------------------
    // L1：势不可当的归属，和「触发器打出来的伤害也要数层数」
    //
    // 两条都是 2026-09-12 由 L3 阶段 3 的整幕链照出来的 —— 它是第一个
    // **长时间连打几百场**的台子，而这两条错都要绕很多圈才看得见。
    // ----------------------------------------------------------------------

    /// **势不可当只认自己获得的格挡。**
    ///
    /// [源码] `JuggernautPower.AfterBlockGained` 的判据是
    /// `!(amount <= 0m) && creature == base.Owner`。第二个条件内核原来没有，
    /// 于是**敌人蜷身获得格挡会触发我的势不可当** —— 白送一次伤害，
    /// 而且那一下又把蜷身打醒，两条规则合起来是个环。
    #[test]
    fn juggernaut_only_fires_on_its_owners_block() {
        let deck = [
            crate::synth::DeckCard::new(card::STRIKE, false),
            crate::synth::DeckCard::new(card::DEFEND, false),
        ];
        let enemies = [crate::synth::EnemySpec::with_hp(enemy::DUMMY, 100)];
        let spec = crate::synth::FightSpec {
            start_status: &[(St::Juggernaut, 5)],
            ..crate::synth::FightSpec::new(&deck, 80, &enemies, 7)
        };
        // --- 反面：敌人自己获得格挡，势不可当**不许**发作 ---
        let mut s = crate::synth::build(&spec).state;
        s.enemies[0].add(St::CurlUp, 14);
        let hp0 = s.enemies[0].hp;
        // 打一张打击：敌人挨打 -> 蜷身给**它自己**14 格挡。
        let strike = (0..s.n_hand as usize)
            .find(|&i| s.cards[s.hand[i] as usize].id == card::STRIKE)
            .expect("手上有打击");
        s = crate::step(s, crate::step::Action::PlayCard { hand: strike as u8, target: 0 });
        assert_eq!(s.enemies[0].block, 14, "蜷身该给它自己 14 点格挡");
        assert_eq!(
            s.enemies[0].hp,
            hp0 - 6,
            "只该掉打击那一下（6）—— 掉更多就是势不可当跟着敌人的格挡发作了"
        );
        assert_eq!(s.enemies[0].get(St::CurlUp), 0, "蜷身用完就该清掉");

        // --- 正面：我自己获得格挡，它照常发作 ---
        let mut s = crate::synth::build(&spec).state;
        let hp0 = s.enemies[0].hp;
        let defend = (0..s.n_hand as usize)
            .find(|&i| s.cards[s.hand[i] as usize].id == card::DEFEND)
            .expect("手上有防御");
        s = crate::step(s, crate::step::Action::PlayCard { hand: defend as u8, target: 0 });
        assert_eq!(s.enemies[0].hp, hp0 - 5, "我获得格挡 ⇒ 势不可当打 5 点");
    }

    /// **触发器打出来的那一下，它的钩子要算在上一层里。**
    ///
    /// `hit_enemy_with` 原来把 `EnemyAttacked/EnemyDamaged/EnemyDied/AllyDied`
    /// 一律按 `depth = 0` 点火，于是 `MAX_HOOK_DEPTH` 在**整条伤害路径上失效**：
    /// 任何「触发器打人 -> 挨打的那只再触发」的环都数不到上限，
    /// 表现是**爆栈**而不是一条红。
    ///
    /// 这里从 `MAX_HOOK_DEPTH` 那一层点火：势不可当自己还能发作（判据是
    /// `depth > MAX`），但它打出来的那一下已经在 `MAX + 1` 层上 ——
    /// 蜷身因此**不该**再被唤醒。修之前这一格恒等于 14。
    #[test]
    fn a_trigger_caused_hit_carries_the_hook_depth() {
        let deck = [crate::synth::DeckCard::new(card::STRIKE, false)];
        let enemies = [crate::synth::EnemySpec::with_hp(enemy::DUMMY, 100)];
        let spec = crate::synth::FightSpec {
            start_status: &[(St::Juggernaut, 5)],
            ..crate::synth::FightSpec::new(&deck, 80, &enemies, 3)
        };
        let mut s = crate::synth::build(&spec).state;
        s.enemies[0].add(St::CurlUp, 14);
        let hp0 = s.enemies[0].hp;
        crate::step::fire(&mut s, crate::ops::Hook::GainBlock, crate::step::MAX_HOOK_DEPTH);
        assert_eq!(s.enemies[0].hp, hp0 - 5, "势不可当在 MAX 层上仍该发作");
        assert_eq!(
            s.enemies[0].block, 0,
            "它打出来的那一下已经在 MAX+1 层 —— 蜷身不该再被唤醒"
        );
        assert_eq!(s.enemies[0].get(St::CurlUp), 14, "没被唤醒就不该掉层");

        // 同一件事从第 0 层点火：蜷身**该**醒，这一半证明上面那条不是
        // 「钩子根本没接上」。
        let mut s = crate::synth::build(&spec).state;
        s.enemies[0].add(St::CurlUp, 14);
        crate::step::fire(&mut s, crate::ops::Hook::GainBlock, 0);
        assert_eq!(s.enemies[0].block, 14, "第 0 层点火时蜷身该醒");
    }

    // ----------------------------------------------------------------------
    // L3 阶段 3：整幕链式评估（`src/synth/act.rs`）
    // ----------------------------------------------------------------------

    fn act_table() -> Option<crate::synth::encounters::Table> {
        crate::synth::encounters::Table::load("data").ok()
    }

    /// 盛碗虫两场的构成是**分布**（`encounters_overrides.json` 的 `distributions`，[源码] 逐支枚举）：
    /// 严格的 `resolve` 照旧拒绝；`resolve_sampled` 按权重抽一支；覆盖率要每一支都开得出；
    /// 观测到的构成落在某一支上就认得死。2026-09-14 起 Hive 20/20。
    #[test]
    fn bowlbug_encounters_are_a_distribution_sampled_by_weight() {
        let Some(t) = act_table() else { return };
        use crate::content::enemy::{BOWLBUG_EGG as EGG, BOWLBUG_NECTAR as NECTAR, BOWLBUG_ROCK as ROCK, BOWLBUG_SILK as SILK};
        use crate::synth::encounters::Unresolved;
        assert!(
            matches!(t.resolve("BowlbugsNormal"), Err(Unresolved::Inexact { .. })),
            "严格的 resolve 答「这一场是哪几只」，分布没有唯一答案"
        );
        let comp = |key: &str, pick: u64| {
            let mut d: Vec<u16> = t.resolve_sampled(key, pick).unwrap().iter().map(|e| e.def).collect();
            d.sort_unstable();
            d
        };
        let sorted = |mut v: Vec<u16>| {
            v.sort_unstable();
            v
        };
        let normal: Vec<Vec<u16>> = (0..3).map(|p| comp("BowlbugsNormal", p)).collect();
        assert_eq!(
            normal,
            vec![sorted(vec![ROCK, EGG, SILK]), sorted(vec![ROCK, EGG, NECTAR]), sorted(vec![ROCK, SILK, NECTAR])],
            "石 + 不放回抽两只工蜂，三支等权"
        );
        assert_eq!(comp("BowlbugsNormal", 3), normal[0], "pick 对总权重取模");
        assert_eq!(
            (0..2).map(|p| comp("BowlbugsWeak", p)).collect::<Vec<_>>(),
            vec![sorted(vec![ROCK, EGG]), sorted(vec![ROCK, NECTAR])]
        );
        assert_eq!(comp("MytesNormal", 7), comp("MytesNormal", 0), "构成确定的遭遇不读 pick");

        let hive = t.act_by_name("Hive").unwrap();
        assert_eq!(t.coverage(hive), (20, 20));
        let (exact, _) = t.identify(&[NECTAR, ROCK]);
        assert!(exact.contains(&"BowlbugsWeak".to_string()), "落在分布的某一支上 ⇒ 认得死");
    }

    /// 第 1 幕批 0（2026-09-15）：怪都在表里、只是构成随机的六场进表。
    ///
    /// * 三场**多重集确定**（随机的只有起手相位）走 `encounters` override —— 严格的 `resolve` 就认
    /// * 三场是**分布**，和盛碗虫同一条路：`resolve_sampled` 按权重抽，`pick` 对总权重取模
    /// * 蛇行扼杀者的小史莱姆是**放回**抽 ⇒ 有两只同名的那一支（和 `SlimesWeak` 的不放回正好是一对）
    #[test]
    fn act1_random_encounters_resolve_or_sample_by_source_weights() {
        let Some(t) = act_table() else { return };
        use crate::content::enemy::{
            CORPSE_SLUG as SLUG, FLYCONID, LEAF_SLIME_M as LM, LEAF_SLIME_S as LS,
            SLITHERING_STRANGLER as SS, SNAPPING_JAXFRUIT as JAX, TWIG_SLIME_M as TM,
            TWIG_SLIME_S as TS, TWO_TAILED_RAT as RAT,
        };
        use crate::synth::encounters::Unresolved;
        let sorted = |mut v: Vec<u16>| {
            v.sort_unstable();
            v
        };
        let exact = |key: &str| sorted(t.resolve(key).unwrap().iter().map(|e| e.def).collect());
        assert_eq!(exact("CorpseSlugsNormal"), vec![SLUG; 3]);
        assert_eq!(exact("CorpseSlugsWeak"), vec![SLUG; 2]);
        assert_eq!(exact("TwoTailedRatsNormal"), vec![RAT; 3]);

        let comp = |key: &str, pick: u64| {
            sorted(t.resolve_sampled(key, pick).unwrap().iter().map(|e| e.def).collect())
        };
        assert!(
            matches!(t.resolve("SlitheringStranglerNormal"), Err(Unresolved::Inexact { .. })),
            "分布没有唯一答案，严格的 resolve 照旧拒绝"
        );
        assert_eq!(
            (0..2).map(|p| comp("FlyconidNormal", p)).collect::<Vec<_>>(),
            vec![sorted(vec![FLYCONID, LM]), sorted(vec![FLYCONID, TM])]
        );
        assert_eq!(
            (0..2).map(|p| comp("SlimesWeak", p)).collect::<Vec<_>>(),
            vec![sorted(vec![LS, TS, LM]), sorted(vec![LS, TS, TM])],
            "两只小的不放回抽完 ⇒ 恒定一树叶一树枝"
        );
        // 12 份：贾克斯果 4 · 树叶（中）2 · 树枝（中）2 · 两树叶（小）1 · 一树叶一树枝（小）2 · 两树枝（小）1
        let mut counts: std::collections::BTreeMap<Vec<u16>, u32> = Default::default();
        for p in 0..12 {
            *counts.entry(comp("SlitheringStranglerNormal", p)).or_default() += 1;
        }
        let want: std::collections::BTreeMap<Vec<u16>, u32> = [
            (sorted(vec![JAX, SS]), 4),
            (sorted(vec![LM, SS]), 2),
            (sorted(vec![TM, SS]), 2),
            (sorted(vec![LS, LS, SS]), 1),
            (sorted(vec![LS, TS, SS]), 2),
            (sorted(vec![TS, TS, SS]), 1),
        ]
        .into_iter()
        .collect();
        assert_eq!(counts, want);

        for key in [
            "CorpseSlugsNormal",
            "CorpseSlugsWeak",
            "TwoTailedRatsNormal",
            "FlyconidNormal",
            "SlimesWeak",
            "SlitheringStranglerNormal",
        ] {
            assert!(t.can_open(key), "{key} 应该开得出");
        }
        let (hits, _) = t.identify(&[SS, TS, TS]);
        assert!(
            hits.contains(&"SlitheringStranglerNormal".to_string()),
            "放回抽出来的两只同名那一支也认得死"
        );
    }

    /// `RubyRaidersNormal`：五种劫掠者不放回抽 3 ⇒ **10 支等权**，每支三只各不相同（批 6 进表）。
    /// 实录 `act1_f12_seventh`（弩手 + 斧手 + 追踪手）要从「构成含随机」变成落在某一支上的唯一命中；
    /// 密林两场都开得出 ⇒ 这一幕 22/22。
    #[test]
    fn ruby_raiders_are_ten_equal_branches_of_three_distinct_raiders() {
        let Some(t) = act_table() else { return };
        use crate::content::enemy::{
            RAIDER_ASSASSIN, RAIDER_AXE, RAIDER_BRUTE, RAIDER_CROSSBOW, RAIDER_TRACKER, VINE_SHAMBLER,
        };
        let five = [RAIDER_AXE, RAIDER_ASSASSIN, RAIDER_BRUTE, RAIDER_CROSSBOW, RAIDER_TRACKER];
        let comp = |pick: u64| {
            let mut v: Vec<u16> = t.resolve_sampled("RubyRaidersNormal", pick).unwrap().iter().map(|e| e.def).collect();
            v.sort_unstable();
            v
        };
        let mut counts: std::collections::BTreeMap<Vec<u16>, u32> = Default::default();
        for p in 0..10 {
            *counts.entry(comp(p)).or_default() += 1;
        }
        assert_eq!(counts.len(), 10, "C(5,3) = 10 支");
        assert!(counts.values().all(|&n| n == 1), "等权：每支在一轮 10 次里恰好一次");
        for k in counts.keys() {
            assert_eq!(k.len(), 3);
            assert!(k.windows(2).all(|w| w[0] != w[1]), "每种上限 1：{k:?} 里不该有重复");
            assert!(k.iter().all(|d| five.contains(d)));
        }
        assert!(t.can_open("RubyRaidersNormal"));
        let hit = |defs: &[u16]| t.identify(defs).0.contains(&"RubyRaidersNormal".to_string());
        assert!(hit(&[RAIDER_CROSSBOW, RAIDER_AXE, RAIDER_TRACKER]), "`act1_f12_seventh` 那一组");
        assert!(hit(&[RAIDER_BRUTE, RAIDER_ASSASSIN, RAIDER_TRACKER]));
        assert!(!hit(&[RAIDER_AXE, RAIDER_AXE, RAIDER_TRACKER]), "两只同名不在任何一支里");

        let exact = t.resolve("VineShamblerNormal").unwrap();
        assert_eq!(exact.iter().map(|e| e.def).collect::<Vec<_>>(), vec![VINE_SHAMBLER]);
    }

    /// **休息处回多少血是 [源码] 定的，不是拍的。**
    ///
    /// `HealRestSiteOption.GetBaseHealAmount = MaxHp * 0.3m`，落地那一步
    /// `Creature.SetCurrentHpInternal` 是 `(int)Math.Min(hp + amount, MaxHp)`
    /// —— **向下取整**。73 血上限回 21 而不是 22。
    #[test]
    fn rest_heals_thirty_percent_rounded_down() {
        use crate::synth::act::rest_heal;
        assert_eq!(rest_heal(73), 21);
        assert_eq!(rest_heal(80), 24);
        assert_eq!(rest_heal(0), 0);
        assert_eq!(rest_heal(9), 2, "2.7 -> 2，向下取整");
    }

    /// 路线字符串**认不出的字母整条拒绝**，不悄悄跳过。
    #[test]
    fn parse_rooms_refuses_a_letter_it_does_not_know() {
        use crate::synth::act::{parse_rooms, Room};
        assert_eq!(parse_rooms("MMRB").unwrap(), vec![Room::Monster, Room::Monster, Room::Rest, Room::Boss]);
        assert_eq!(parse_rooms("m e b r").unwrap().len(), 4, "大小写和空白都认");
        assert!(parse_rooms("MMX").is_err(), "X 不是房间 —— 整条拒绝");
        assert!(parse_rooms("   ").is_err());
    }

    /// **两副牌组在同一个样本上抽到同一批遭遇**（CRN 多共享的那一维）。
    ///
    /// 判据取的是**可观测的后果**：哪几间开不出来只由遭遇序列决定，
    /// 所以两副牌组逐样本的 `unsimulated` 必须**逐个相等**。
    /// 序列一旦跟着牌组走，这一列当场就会分岔。
    #[test]
    fn two_decks_see_the_same_encounter_sequence() {
        let Some(t) = act_table() else { return };
        use crate::synth::act::{evaluate_act, parse_rooms, ActCfg, ActPlan};
        let rooms = parse_rooms("MMEB").unwrap();
        let plan = ActPlan::new("Underdocks", &rooms);
        let cfg = ActCfg { samples: 12, ..ActCfg::default() };
        let a = eval_deck(10);
        let b = eval_deck(16); // 多六张牌
        let enemies: [crate::synth::EnemySpec; 0] = [];
        let sa = crate::synth::FightSpec::new(&a, 80, &enemies, 4242);
        let sb = crate::synth::FightSpec::new(&b, 80, &enemies, 4242);
        let (ea, eb) = (evaluate_act(&sa, &plan, &t, &cfg), evaluate_act(&sb, &plan, &t, &cfg));
        assert_eq!(ea.n, eb.n);
        for i in 0..ea.n {
            assert_eq!(
                ea.samples[i].unsimulated, eb.samples[i].unsimulated,
                "样本 {i}：换了牌组抽到的遭遇就变了 —— 整幕链这一维的 CRN 断了"
            );
        }
    }

    /// **开不出来的那一场是"跳过并计数"，不是"不存在"。**
    ///
    /// 漏掉的场次要同时出现在两处：逐样本的 `unsimulated` 和报告里的
    /// `skipped` 原因清单 —— 少一处，读结论的人就看不见这个数是**乐观**的。
    ///
    /// 原来拿第 1 幕 Underdocks 当「必然漏几场」的那一幕，2026-09-19 批 4 / 批 5 之后它 20/20 了。
    /// 换成**第 3 幕钉住女王**（今天唯一开不出来的 Boss，缺火炬头聚合体），路线 `MB`：
    /// 每条链都必然走到它、必然漏它，不再靠抽序列碰运气。前面那一间弱怪（Glory 的弱怪池全开得出）
    /// 不能省 —— 一场都开不出来的路线是**拒绝作答**（`NothingSimulable`），不是「漏了一场」。
    /// 血给到 300 是为了让大部分链走得到女王（死在弱怪那一间的就不算漏）；
    /// 起手牌组打第 3 幕弱怪，300 血也有链死 —— 实测 8 条里死 2 条，所以只要求「漏了」> 0。
    /// 女王建掉那天这条要再换 —— 到时候看 `dump_encounters.py` 的覆盖率报告挑还缺的那一场。
    #[test]
    fn an_encounter_the_kernel_cannot_open_is_counted_not_hidden() {
        let Some(t) = act_table() else { return };
        use crate::synth::act::{evaluate_act, parse_rooms, ActCfg, ActPlan};
        let rooms = parse_rooms("MB").unwrap();
        let mut plan = ActPlan::new("Glory", &rooms);
        plan.pin_boss = Some("QueenBoss");
        let cfg = ActCfg { samples: 8, ..ActCfg::default() };
        let deck = eval_deck(10);
        let enemies: [crate::synth::EnemySpec; 0] = [];
        let ev = evaluate_act(
            &crate::synth::FightSpec::new(&deck, 300, &enemies, 99),
            &plan,
            &t,
            &cfg,
        );
        assert!(ev.refused.is_none(), "{:?}", ev.refused);
        assert!(ev.deaths < 8, "一条链都没走到女王，这条测试就量不到「漏」");
        assert!(ev.unsimulated.mean > 0.0, "这一幕开得出 {:?}，走到女王的链不可能一场都不漏", ev.coverage);
        assert!(!ev.skipped.is_empty(), "漏了却没有理由清单 = 静默地漏");
        assert!(ev.coverage.0 < ev.coverage.1, "覆盖率要和结论一起报");
        assert!(ev.missing_fights().is_some(), "漏了就要报那条偏差");
    }

    /// 配对比较的三个前提：**同种子基、同样本数、同一条路线**。
    ///
    /// 路线那一条是整幕独有的 —— 换了路线就不是同一个问题了，
    /// 那时候的差值什么都不是。
    #[test]
    fn paired_act_delta_refuses_anything_but_the_same_setup() {
        let Some(t) = act_table() else { return };
        use crate::synth::act::{evaluate_act, paired_act_delta, parse_rooms, ActCfg, ActPlan};
        let rooms = parse_rooms("MMB").unwrap();
        let other = parse_rooms("MMRB").unwrap();
        let cfg = ActCfg { samples: 8, ..ActCfg::default() };
        let deck = eval_deck(10);
        let enemies: [crate::synth::EnemySpec; 0] = [];
        let spec = crate::synth::FightSpec::new(&deck, 80, &enemies, 5);
        let plan = ActPlan::new("Underdocks", &rooms);
        let a = evaluate_act(&spec, &plan, &t, &cfg);
        // 自己和自己：逐样本差必须全是 0
        let b = evaluate_act(&spec, &plan, &t, &cfg);
        let d = paired_act_delta(&a, &b).expect("同一个 spec、同一条路线");
        assert_eq!(d.hp.min, 0);
        assert_eq!(d.hp.max, 0);
        assert_eq!(d.deaths_delta, 0);
        // 换种子基 / 换样本数 / 换路线，三条都不许配对
        let other_seed = evaluate_act(
            &crate::synth::FightSpec { seed: 6, ..spec },
            &plan,
            &t,
            &cfg,
        );
        assert!(paired_act_delta(&a, &other_seed).is_none(), "种子基不同");
        let fewer = evaluate_act(&spec, &plan, &t, &ActCfg { samples: 4, ..cfg });
        assert!(paired_act_delta(&a, &fewer).is_none(), "样本数不同");
        let other_route = evaluate_act(
            &spec,
            &ActPlan { rooms: &other, ..plan },
            &t,
            &cfg,
        );
        assert!(paired_act_delta(&a, &other_route).is_none(), "路线不同");
    }

    /// **升级过的牌组走完一幕不许更差** —— 阶段 2 那条单调性自检的整幕版本。
    ///
    /// 它不是统计检验：打击/防御 5 -> 8 是严格更强的牌，同一批遭遇下
    /// 不该出现"整体更危险"。方向反了通常意味着链的血量接错了地方
    /// （比如把战损当成了收益，或者休息处回血算进了错的一侧）。
    #[test]
    fn upgrading_the_basics_never_reads_worse_across_an_act() {
        let Some(t) = act_table() else { return };
        use crate::synth::act::{
            evaluate_act, paired_act_delta, parse_rooms, upgrade_basics, ActCfg, ActPlan,
        };
        let rooms = parse_rooms("MMRMB").unwrap();
        let plan = ActPlan::new("Underdocks", &rooms);
        let cfg = ActCfg { samples: 24, ..ActCfg::default() };
        let base = eval_deck(12);
        let upg = upgrade_basics(&base);
        let enemies: [crate::synth::EnemySpec; 0] = [];
        let spec = crate::synth::FightSpec::new(&base, 60, &enemies, 8);
        let a = evaluate_act(&spec, &plan, &t, &cfg);
        let b = evaluate_act(&crate::synth::FightSpec { deck: &upg, ..spec }, &plan, &t, &cfg);
        let d = paired_act_delta(&a, &b).expect("同一个种子基、同一条路线");
        assert!(d.deaths_delta <= 0, "升级之后整幕死得更多：{}", d.deaths_delta);
        assert!(
            d.hp.mean >= 0.0,
            "升级之后走完一幕的血量更低（{:.2}）—— 更好 {} / 更差 {}",
            d.hp.mean,
            d.b_better,
            d.a_better
        );
    }

    /// **休息处那一间只做一件事：回 30% 上限、封顶。**
    ///
    /// 用一条只有休息处的"路线"把它单独量出来 —— 一场仗都不打，
    /// 终点血量就该正好是 `min(上限, 进来时 + rest_heal)`。
    #[test]
    fn a_rest_room_heals_and_caps_at_max_hp() {
        let Some(t) = act_table() else { return };
        use crate::synth::act::{evaluate_act, parse_rooms, rest_heal, ActCfg, ActPlan};
        let rooms = parse_rooms("RR").unwrap();
        let plan = ActPlan::new("Underdocks", &rooms);
        let cfg = ActCfg { samples: 4, ..ActCfg::default() };
        let deck = eval_deck(10);
        let enemies: [crate::synth::EnemySpec; 0] = [];
        let spec = crate::synth::FightSpec {
            max_hp: 80,
            ..crate::synth::FightSpec::new(&deck, 30, &enemies, 1)
        };
        let ev = evaluate_act(&spec, &plan, &t, &cfg);
        assert_eq!(ev.deaths, 0);
        assert_eq!(ev.hp_end.min, (30 + 2 * rest_heal(80)).min(80), "两觉睡满");
        let full = crate::synth::FightSpec { hp: 78, ..spec };
        let ev = evaluate_act(&full, &plan, &t, &cfg);
        assert_eq!(ev.hp_end.max, 80, "封顶在上限");
    }

    /// **钉死的 Boss 真的被钉住了，而且不扰动这条链的其余部分。**
    ///
    /// 两半缺一不可：钉了要用那一只（不然报告里那句"钉死为 X"是假的），
    /// 而杂兵/精英那两串必须**逐字不变** —— 抽 Boss 原来要掷一次骰子，
    /// 跳过它却让后面的抽牌漂掉的话，"钉 Boss"就顺手换了整幕的遭遇，
    /// 而两个候选之间的 CRN 正是靠这一维。
    #[test]
    fn pinning_the_boss_uses_it_and_leaves_the_rest_of_the_sequence_alone() {
        let Some(t) = act_table() else { return };
        use crate::synth::act::{draw_sequence, parse_rooms, ActPlan};
        let rooms = parse_rooms("MEB").unwrap();
        let act = t.act_by_name("Underdocks").expect("表里有这一幕");
        let free = ActPlan::new("Underdocks", &rooms);
        let pinned = ActPlan { pin_boss: Some("WaterfallGiantBoss"), ..free };
        for seed in [1u64, 2, 3, 99, 12345] {
            let a = draw_sequence(&t, act, &free, seed);
            let b = draw_sequence(&t, act, &pinned, seed);
            assert_eq!(b.boss.as_deref(), Some("WaterfallGiantBoss"), "钉了却没用");
            assert_eq!(a.normals, b.normals, "种子 {seed}：钉 Boss 把杂兵那一串抽漂了");
            assert_eq!(a.elites, b.elites, "种子 {seed}：钉 Boss 把精英那一串抽漂了");
        }
    }

    /// **钉错幕的 Boss 整个拒绝作答**，不退回去掷一个。
    ///
    /// 悄悄掷一个的话，报出来的数会像是"按你说的那只 Boss 算的" ——
    /// 这正是本仓库最忌讳的"自信地算错"。
    #[test]
    fn pinning_a_boss_from_another_act_refuses_instead_of_rolling_one() {
        let Some(t) = act_table() else { return };
        use crate::synth::act::{evaluate_act, parse_rooms, ActCfg, ActPlan, ActRefusal};
        let rooms = parse_rooms("B").unwrap();
        let cfg = ActCfg { samples: 4, ..ActCfg::default() };
        let deck = eval_deck(10);
        let enemies: [crate::synth::EnemySpec; 0] = [];
        let spec = crate::synth::FightSpec::new(&deck, 80, &enemies, 7);
        // 无厌沙虫是第 2 幕（Hive）的 Boss
        let plan = ActPlan {
            pin_boss: Some("TheInsatiableBoss"),
            ..ActPlan::new("Underdocks", &rooms)
        };
        let ev = evaluate_act(&spec, &plan, &t, &cfg);
        match ev.refused {
            Some(ActRefusal::NoSuchBoss { .. }) => {}
            other => panic!("钉错幕的 Boss 没被拒：{other:?}"),
        }
    }

    /// **换路线的那一对走另一个入口。**
    ///
    /// `paired_act_delta` 挡住路线不同的一对（比牌组时路线必须固定），
    /// 而"走不走精英"这个决策**只能**由两条不同的路线表达 ——
    /// 所以另开一个口子，并在那边的文档里写清 CRN 共享到哪为止。
    #[test]
    fn comparing_two_routes_needs_the_other_entry_point() {
        let Some(t) = act_table() else { return };
        use crate::synth::act::{
            evaluate_act, paired_act_delta, paired_act_delta_across_routes, parse_rooms, ActCfg,
            ActPlan,
        };
        let with_elite = parse_rooms("EMB").unwrap();
        let without = parse_rooms("MMB").unwrap();
        let cfg = ActCfg { samples: 8, ..ActCfg::default() };
        let deck = eval_deck(10);
        let enemies: [crate::synth::EnemySpec; 0] = [];
        let spec = crate::synth::FightSpec::new(&deck, 80, &enemies, 11);
        let a = evaluate_act(&spec, &ActPlan::new("Underdocks", &with_elite), &t, &cfg);
        let b = evaluate_act(&spec, &ActPlan::new("Underdocks", &without), &t, &cfg);
        assert!(paired_act_delta(&a, &b).is_none(), "路线不同，这个入口必须拒绝");
        let d = paired_act_delta_across_routes(&a, &b).expect("换路线的那个入口要收");
        assert!(d.pairs > 0);
        // 种子基不同那一条**两个入口都要挡**
        let other_seed = evaluate_act(
            &crate::synth::FightSpec { seed: 12, ..spec },
            &ActPlan::new("Underdocks", &without),
            &t,
            &cfg,
        );
        assert!(
            paired_act_delta_across_routes(&a, &other_seed).is_none(),
            "种子基不同就不是配对比较了"
        );
    }

    // -----------------------------------------------------------------------
    // 2026-09-20 修 L2 审计（09-19）里的四条：指纹漏字段 / 附魔分组 / 截断 / 开着的选牌
    //
    // 每一条都先断言「场景立得住」（反例的前提真的成立），再断言修好的行为 ——
    // 否则撤掉修复也不会红，守卫就成了摆设。
    // -----------------------------------------------------------------------

    /// 手牌 / 抽牌堆 / 弃牌堆各放指定的牌，全是 `enemy::DUMMY`，抽牌堆顺序未知。
    /// `draw` 的最后一张在数组末尾 = 牌堆顶。
    fn audit_scene(
        seed: u64,
        enemies: &[i32],
        hand: &[u16],
        draw: &[u16],
        disc: &[u16],
        energy: i32,
    ) -> State {
        let mut s = State::new(80, seed);
        for &hp in enemies {
            s.add_enemy(enemy::DUMMY, hp);
        }
        let h: Vec<u8> = hand.iter().map(|&c| s.add_card(c, 0, 0)).collect();
        let d: Vec<u8> = draw.iter().map(|&c| s.add_card(c, 0, 0)).collect();
        let x: Vec<u8> = disc.iter().map(|&c| s.add_card(c, 0, 0)).collect();
        let mut s = begin_combat(s);
        s.n_hand = h.len() as u8;
        s.hand[..h.len()].copy_from_slice(&h);
        s.n_draw = d.len() as u8;
        s.draw[..d.len()].copy_from_slice(&d);
        s.n_draw_known = 0;
        s.n_disc = x.len() as u8;
        s.disc[..x.len()].copy_from_slice(&x);
        s.n_exh = 0;
        s.energy = energy;
        s
    }

    /// 按顺序打出 `(牌, 目标)`，每一张都必须合法。
    fn audit_play_all(s: State, plays: &[(u16, u8)]) -> State {
        plays.iter().fold(s, |st, &(id, target)| {
            let i = (0..st.n_hand as usize)
                .find(|&i| st.cards[st.hand[i] as usize].id == id)
                .expect("手里没有这张牌");
            let ns = step(st, Action::PlayCard { hand: i as u8, target });
            assert_ne!(ns, st, "非法动作");
            ns
        })
    }

    /// **不去重**的穷举，和求解器同一套终点（开着选牌的局面不算终点）。只回最优分。
    fn audit_brute(s: &State, th: &Threat, sc: fn(&State) -> i32, depth: usize, best: &mut i32) {
        if s.pending == Pending::None || s.combat_over {
            *best = (*best).max(sc(&th.end_turn(*s)));
        }
        if s.combat_over || depth >= 30 {
            return;
        }
        let (acts, n) = legal_actions(s);
        for &a in &acts[..n] {
            if matches!(a, Action::EndTurn | Action::UsePotion { .. }) {
                continue;
            }
            let ns = step(*s, a);
            if ns != *s {
                audit_brute(&ns, th, sc, depth + 1, best);
            }
        }
    }

    /// 荆棘 2 / 4 的两只敌人 + 一只没荆棘的，手上防御 + 三张打击 + 扯碎。
    fn thorns_scene() -> State {
        let mut s = audit_scene(
            2,
            &[200, 200, 200],
            &[card::DEFEND, card::STRIKE, card::STRIKE, card::STRIKE, card::TEAR_ASUNDER],
            &[],
            &[],
            6,
        );
        s.enemies[0].set(St::Thorns, 2);
        s.enemies[1].set(St::Thorns, 4);
        s
    }

    /// **掉血一样、挨穿次数不一样的两个局面，指纹必须分开。**
    ///
    /// 荆棘反弹和格挡谁先谁后：先挡住两下 2 点、第三下 4 点打穿 3 ⇒ 挨穿 1 次；
    /// 先挡住 4 点、剩下的 1 点格挡只吃掉一下 2 点的一半 ⇒ 挨穿 2 次。
    /// 两边都是 77 血、0 格挡、`hp_lost_this_turn` 3，而扯碎的段数 = 1 + 挨穿次数。
    /// 2026-09-20 之前两个指纹逐字相同，后访问到的那个被当成重复剪掉。
    #[test]
    fn dedup_keeps_states_that_differ_only_in_hp_loss_hits() {
        let s = thorns_scene();
        let a = audit_play_all(
            s,
            &[(card::DEFEND, 0), (card::STRIKE, 0), (card::STRIKE, 0), (card::STRIKE, 1)],
        );
        let b = audit_play_all(
            s,
            &[(card::DEFEND, 0), (card::STRIKE, 1), (card::STRIKE, 0), (card::STRIKE, 0)],
        );
        // 前提：规则看得见的只差这一栏，而扯碎读的正是它
        assert_eq!(
            (a.player.hp, a.player.block, a.hp_lost_this_turn),
            (b.player.hp, b.player.block, b.hp_lost_this_turn),
            "场景立不住：两条顺序掉的血或剩的格挡不一样"
        );
        assert_eq!((a.hp_loss_hits, b.hp_loss_hits), (1, 2), "场景立不住：挨穿次数没分开");
        let hit = |st: State| st.enemies[2].hp - audit_play_all(st, &[(card::TEAR_ASUNDER, 2)]).enemies[2].hp;
        assert_eq!((hit(a), hit(b)), (10, 15), "场景立不住：扯碎的段数没跟着挨穿次数走");

        assert_ne!(crate::solver::key(&a), crate::solver::key(&b), "单回合指纹把两个未来不同的局面并成了一个");
        assert_ne!(crate::plan::key(&a), crate::plan::key(&b), "跨回合指纹把两个未来不同的局面并成了一个");
    }

    /// 同一个毛病的**后果**：去重剪掉了最优线。
    ///
    /// 网格扫描里差得最多的那一例（2026-09-19，`leaf` 口径）：荆棘 4 / 6、力量 1、扯碎+。
    /// 修之前求解器给 68 血 / 敌伤 53，不去重的穷举是 **71 血 / 敌伤 53** —— 白丢 3 血。
    /// 判据不写死那个数，写成「求解器 = 不去重的穷举」。
    #[test]
    fn dedup_does_not_prune_the_best_line_behind_an_hp_loss_hits_collision() {
        let mut s = audit_scene(
            11,
            &[200, 200, 200],
            &[card::DEFEND, card::STRIKE, card::STRIKE, card::STRIKE, card::TEAR_ASUNDER],
            &[],
            &[],
            10,
        );
        s.enemies[0].set(St::Thorns, 4);
        s.enemies[1].set(St::Thorns, 6);
        s.player.set(St::Strength, 1);
        let t = s.hand[4] as usize;
        s.cards[t].flags |= F_UPGRADED;

        let sol = solve_turn_budget(&s, &Threat::NONE, score::leaf, 200_000);
        assert!(sol.complete, "场景立不住：搜索没搜完，比不了");
        let mut best = i32::MIN;
        audit_brute(&s, &Threat::NONE, score::leaf, 0, &mut best);
        let end = crate::solver::replay_line(&s, sol.line.acts()).unwrap();
        assert_eq!(
            sol.line.score,
            best,
            "搜完了却比不去重的穷举差 {} 分（求解器那条打完剩 {} 血）：{:?}",
            best - sol.line.score,
            end.player.hp,
            explain(&s, sol.line.acts())
        );
        assert_eq!(end.player.hp, 71, "穷举最优是 71 血（修之前求解器给 68）");
    }

    /// **机会节点要把带附魔的那一张和普通的分开算。**
    ///
    /// 6 张打击、1 张带锋利、抽 5：锋利那张进手的概率是 5/6，**和它在数组里的位置无关**。
    /// 2026-09-20 之前 `plan::ident` 不含附魔，6 张并成一组、只给一个 `p = 1.0` 的孩子，
    /// 锋利那张在 `draw[0]` 时 100%、在 `draw[5]` 时 0%。
    #[test]
    fn chance_node_tells_enchanted_copies_apart() {
        use crate::plan::{chance_children, Plan};
        let sharp = crate::content::enchant_by_id("SHARP").unwrap().0;
        for pos in [0usize, 5] {
            let mut s = audit_scene(3, &[200], &[], &[card::STRIKE; 6], &[], 3);
            let ix = s.draw[pos];
            s.cards[ix as usize].ench = sharp;
            s.cards[ix as usize].ench_amt = 3;
            let kids = chance_children(&s, &Plan::default(), 1);
            let total: f64 = kids.iter().map(|k| k.p).sum();
            assert!((total - 1.0).abs() < 1e-9, "draw[{pos}]：概率和是 {total}");
            let p: f64 = kids
                .iter()
                .filter(|k| k.state.hand[..k.state.n_hand as usize].contains(&ix))
                .map(|k| k.p)
                .sum();
            assert!(
                (p - 5.0 / 6.0).abs() < 1e-9,
                "锋利那张在 draw[{pos}]：进手概率 {p}，应为 5/6（{} 个孩子）",
                kids.len()
            );
        }
    }

    /// 机会节点的孩子里，`ix` 那张进手的总概率。
    fn p_in_hand(kids: &[crate::plan::Draw], ix: u8) -> f64 {
        kids.iter()
            .filter(|k| k.state.hand[..k.state.n_hand as usize].contains(&ix))
            .map(|k| k.p)
            .sum()
    }

    /// 7 张各不相同的牌（一张一个身份，好逐张数进手概率）。
    const SEVEN_DISTINCT: [u16; 7] =
        [card::STRIKE, card::DEFEND, card::BASH, card::DISMANTLE, card::ASH_STRIKE, card::DEMON_FLAME, card::LIGHTNING];

    /// **机会节点抽几张，问 L1。** 心灵腐化那一手只发 4 张：6 张不同的牌逐张进手 2/3。
    ///
    /// 2026-09-25 之前机会节点写死 5：按 5 张挑一个多重集放到顶上，`open_hand` 只拿走 4 张，
    /// 挑出来的第一张永远留在牌堆里 —— 逐张 0 / 2/3 / 5/6 ×4，**概率和照样是 1**。
    /// 牌堆正着放、倒着放各跑一遍：错的那一版偏向哪张取决于数组顺序。
    #[test]
    fn chance_node_draws_as_many_as_open_hand_does_under_mind_rot() {
        use crate::plan::{chance_children, Plan};
        for rev in [false, true] {
            let mut ids = SEVEN_DISTINCT[..6].to_vec();
            if rev {
                ids.reverse();
            }
            let mut s = audit_scene(3, &[200], &[], &ids, &[], 3);
            s.player.set(St::MindRot, 1);
            assert_eq!(hand_draw_count(&s), 4, "场景立不住：心灵腐化 1 层该发 4 张");
            let kids = chance_children(&s, &Plan::default(), 1);
            let total: f64 = kids.iter().map(|k| k.p).sum();
            assert!((total - 1.0).abs() < 1e-9, "概率和是 {total}");
            assert!(kids.iter().all(|k| k.state.n_hand == 4), "有孩子不是 4 张手牌");
            for &ix in &s.draw[..6] {
                let p = p_in_hand(&kids, ix);
                assert!(
                    (p - 2.0 / 3.0).abs() < 1e-9,
                    "{}（倒序={rev}）进手概率 {p}，应为 2/3",
                    card(s.cards[ix as usize].id).name
                );
            }
        }
    }

    /// **手牌上限也进张数**：手上留着 6 张，这一手最多再发 4 张。
    #[test]
    fn chance_node_respects_the_hand_limit() {
        use crate::plan::{chance_children, Plan};
        let s = audit_scene(3, &[200], &[card::DEFEND; 6], &SEVEN_DISTINCT[..6], &[], 3);
        assert_eq!(hand_draw_count(&s), 4, "场景立不住：手上 6 张该只发 4 张");
        let kids = chance_children(&s, &Plan::default(), 1);
        for &ix in &s.draw[..6] {
            let p = p_in_hand(&kids, ix);
            assert!((p - 2.0 / 3.0).abs() < 1e-9, "进手概率 {p}，应为 2/3");
        }
    }

    /// **佩尔之血那 +1 要进机会节点**，不能在它之前就从牌序数组顶上抽走。
    ///
    /// 走真实路径（`end_turn_before_draw`，`TurnStart` 在里面点火）：7 张不同的牌、
    /// 这一手发 6 张 ⇒ 逐张进手 6/7。2026-09-25 之前那一张在 `TurnStart` 上先抽了，
    /// 机会节点只枚举剩下 6 张里的 5 张：先抽的那张 100%、其余 5/6，换种子也不变。
    #[test]
    fn paels_blood_extra_card_is_part_of_the_chance_node() {
        use crate::plan::{chance_children, Plan};
        let mut s = audit_scene(3, &[200], &[], &SEVEN_DISTINCT, &[], 3);
        s.player.set(St::PaelsBlood, 1);
        let pre = end_turn_before_draw(s);
        assert!(!pre.combat_over && !pre.player_dead, "场景立不住：假人回合就打完了");
        assert_eq!(pre.n_hand, 0, "佩尔之血在发牌之前就抽了牌");
        assert_eq!((pre.n_draw, pre.n_draw_known), (7, 0), "场景立不住：牌堆不是 7 张全未知");
        assert_eq!(hand_draw_count(&pre), 6, "佩尔之血该让这一手发 6 张");
        let kids = chance_children(&pre, &Plan::default(), 1);
        assert!(kids.iter().all(|k| k.state.n_hand == 6), "有孩子不是 6 张手牌");
        for &ix in &pre.draw[..7] {
            let p = p_in_hand(&kids, ix);
            assert!((p - 6.0 / 7.0).abs() < 1e-9, "进手概率 {p}，应为 6/7");
        }
    }

    /// **`open_hand` 发的就是 `hand_draw_count` 张**，两边是同一个数 ——
    /// 机会节点靠的就是这一条。加张数（佩尔之血 / 小提琴）只记账不先抽，发完清零。
    ///
    /// 外加手满时的一条 [源码] 细节：`CardPileCmd.Draw` 在手满时**直接返回、不洗牌**
    /// （`num == 0` 在 `ShuffleIfNecessary` 之前）。
    #[test]
    fn open_hand_deals_exactly_hand_draw_count() {
        for (mind_rot, pael, fiddle, kept) in
            [(0, 0, 0, 0), (1, 0, 0, 0), (0, 1, 0, 0), (1, 1, 0, 0), (0, 0, 2, 0), (1, 1, 2, 0), (0, 1, 0, 6), (0, 0, 2, 8)]
        {
            let mut s = audit_scene(3, &[200], &[], &[card::STRIKE; 30], &[], 3);
            s.player.set(St::MindRot, mind_rot);
            s.player.set(St::PaelsBlood, pael);
            s.player.set(St::Fiddle, fiddle);
            let mut pre = end_turn_before_draw(s);
            // 回合末留在手上的牌（保留那一类）：直接塞进手牌。`add_card` 顺手把它放进了抽牌堆顶，挪过来
            for _ in 0..kept {
                let ix = pre.add_card(card::DEFEND, 0, 0);
                pre.n_draw -= 1;
                pre.hand[pre.n_hand as usize] = ix;
                pre.n_hand += 1;
            }
            let tag = format!("心灵腐化 {mind_rot} · 佩尔之血 {pael} · 小提琴 {fiddle} · 留手 {kept}");
            let want = hand_draw_count(&pre);
            let expect = ((5 + pael + fiddle - mind_rot).max(0) as usize).min(MAX_HAND - pre.n_hand as usize);
            assert_eq!(want, expect, "{tag}");
            let mut t = pre;
            open_hand(&mut t);
            assert_eq!(t.n_hand as usize - pre.n_hand as usize, want, "{tag}：`open_hand` 发的张数和 `hand_draw_count` 不一样");
            assert_eq!(t.player.get(St::HandDrawBonus), 0, "{tag}：加张数发完没清零");
        }

        // 手满、抽牌堆空、弃牌堆有牌：一张不抽，**也不洗**
        let mut full = audit_scene(3, &[200], &[card::DEFEND; MAX_HAND], &[], &[card::STRIKE; 5], 3);
        assert_eq!(hand_draw_count(&full), 0);
        let before = full;
        open_hand(&mut full);
        assert_eq!((full.n_hand, full.n_disc, full.n_draw), (before.n_hand, before.n_disc, 0), "手满了还洗了牌");
        assert_eq!(full.rng.shuffle, before.rng.shuffle, "手满了还推动了洗牌流");

        // 停在发牌之前、没接着 `open_hand` 就又结束一个回合（`Leaf::Rollout` 碰上留手牌时
        // 就是这样跳过发牌的）：上回合记的加张数不许叠到这一回合
        let mut s = audit_scene(3, &[200], &[], &[card::STRIKE; 30], &[], 3);
        s.player.set(St::PaelsBlood, 1);
        let twice = end_turn_before_draw(end_turn_before_draw(s));
        assert_eq!(twice.player.get(St::HandDrawBonus), 1, "没发牌的那一回合的加张数叠进了下一回合");
    }

    /// **线被 `MAX_LINE` 截断而局面还能往下走，就不许报穷尽。**
    ///
    /// 手上 5 张、抽牌堆 30 张亮剑（0 费、5 伤、抽 1）：一条过牌链能一直打下去。
    /// 2026-09-20 之前搜到第 24 步直接返回、`complete = true`，而线末再合法打一张
    /// 分数还能 +150。
    #[test]
    fn a_line_cut_at_max_line_is_not_reported_as_exhaustive() {
        let s = audit_scene(6, &[1000], &[card::FLASH_OF_STEEL; 5], &[card::FLASH_OF_STEEL; 30], &[], 3);
        let sol = solve_turn_budget(&s, &Threat::NONE, score::survive_first, 200_000);
        assert_eq!(sol.line.n as usize, crate::solver::MAX_LINE, "场景立不住：线没顶到上限");
        let end = crate::solver::replay_line(&s, sol.line.acts()).unwrap();
        let (acts, n) = legal_actions(&end);
        assert!(
            acts[..n].iter().any(|a| matches!(a, Action::PlayCard { .. })),
            "场景立不住：线末已经没牌可打"
        );
        assert!(!sol.complete, "线被截断、局面还能走，却报了穷尽");

        // 反过来：真搜完了照样报穷尽，别把这一栏变成恒 false
        let short = hand_of(1, 100, &[card::STRIKE, card::DEFEND], 3);
        assert!(solve_turn_budget(&short, &Threat::NONE, score::survive_first, 200_000).complete);
    }

    /// **开着子选择的局面不是合法终点。** 线里一定把选牌做完，候选表同口径。
    ///
    /// 2026-09-20 之前两种样子都出现过：
    /// 1. 线中途开出选牌（头槌，弃牌堆 2 张）：返回的线就是 `[头槌]`，选牌没做 ——
    ///    做不做在这一回合的分数里是平的，而并列时短线赢；
    /// 2. 根上就开着选牌、能量 0：返回 **0 个动作**、`complete = true`，
    ///    而 `step(EndTurn)` 在那个局面上原地不动。
    #[test]
    fn an_open_choice_is_not_a_legal_terminal() {
        let s = audit_scene(8, &[100], &[card::HEADBUTT], &[], &[card::STRIKE, card::DEFEND], 1);
        let sol = solve_turn_budget(&s, &Threat::NONE, score::survive_first, 200_000);
        let end = crate::solver::replay_line(&s, sol.line.acts()).unwrap();
        assert!(
            sol.line.acts().iter().any(|a| matches!(a, Action::PlayCard { .. })),
            "场景立不住：头槌都没打"
        );
        assert_eq!(end.pending, Pending::None, "线停在选牌之前：{:?}", explain(&s, sol.line.acts()));
        assert_ne!(step(end, Action::EndTurn), end, "线末结束不了回合");
        for c in crate::solver::solve_turn_topk(&s, &Threat::NONE, score::leaf, 200_000, 0, 6, 2, 2) {
            let e = crate::solver::replay_line(&s, c.acts()).unwrap();
            assert_eq!(e.pending, Pending::None, "候选线停在选牌之前：{:?}", explain(&s, c.acts()));
        }

        let mut r = audit_scene(7, &[100], &[card::STRIKE, card::DEFEND, card::BASH], &[], &[], 0);
        r.pending = Pending::ExhaustFromHand { remaining: 1 };
        assert_eq!(step(r, Action::EndTurn), r, "场景立不住：根上的结束回合居然合法");
        let sol = solve_turn_budget(&r, &Threat::NONE, score::survive_first, 200_000);
        assert!(sol.complete);
        let end = crate::solver::replay_line(&r, sol.line.acts()).unwrap();
        assert_eq!(end.pending, Pending::None, "根上开着的选牌没做就收手了");
        assert_ne!(step(end, Action::EndTurn), end, "线末结束不了回合");
    }

    /// 反过来的边角：**选牌一个都走不动**（手牌满了还要从弃牌堆拿牌）。
    ///
    /// # 这一格 2026-09-22 之前是个真的死局
    ///
    /// `legal_actions` 在 `Pending` 分支里提前 return（只给 `Choose`、不给 `EndTurn`），
    /// 而每个 `Choose` 都被 `step` 那条 `n_hand < MAX_HAND` 的守卫挡住、原地不动，
    /// `step(EndTurn)` 又因为 `pending != None` 原样返回 ——
    /// **没有任何一个动作改得了状态，回合也结束不了，这一局再也走不下去。**
    /// （3000 场随机战斗扫出一场：`FetchFromDiscard{1}` + 手牌 10 + 弃牌堆 6。）
    ///
    /// 当时这个测试断言的是**下游的止血**（求解器退回老口径、不报穷尽）。
    /// 止血留着，但病因修在了 L1：[`step::close_pending_if_stuck`] 让内核吐出来的
    /// `State` 永远不带一个推不动的 `Pending`。所以这里改成钉**那条不变量**。
    #[test]
    fn a_choice_that_cannot_close_is_never_a_dead_end() {
        let mut s = audit_scene(12, &[100], &[card::DEFEND; 10], &[], &[card::STRIKE, card::BASH], 0);
        s.pending = Pending::FetchFromDiscard { remaining: 1 };
        // 场景立得住：这个 `Pending` 真的推不动（手牌满 10 张）
        assert!(!crate::step::pending_is_satisfiable(&s), "场景立不住：这个选牌还推得动");
        let (acts, n) = legal_actions(&s);
        assert!(n > 0, "场景立不住：一个候选都没有");

        // **每一个合法动作都必须真的推动局面**，而且推完之后选牌是关掉的
        for &a in &acts[..n] {
            let ns = step(s, a);
            assert_ne!(ns, s, "死局：{:?} 原地不动", a);
            assert_eq!(ns.pending, Pending::None, "{:?} 之后选牌还开着", a);
        }
        // 出口收口在 `step` 上，所以走一步就能结束回合
        let out = step(s, acts[0]);
        assert_ne!(step(out, Action::EndTurn), out, "走一步之后仍然结束不了回合");

        // 求解器因此能正常作答：有线、有分数、而且是**穷尽**的
        let sol = solve_turn_budget(&s, &Threat::NONE, score::survive_first, 200_000);
        assert!(sol.line.score > i32::MIN, "没给分数");
        assert!(sol.complete, "这个局面小得很，该搜得穷尽");
        let end = crate::solver::replay_line(&s, sol.line.acts()).unwrap();
        assert_eq!(end.pending, Pending::None, "线末选牌还开着");
        assert_ne!(step(end, Action::EndTurn), end, "线末结束不了回合");
    }

    /// 止血本身还留着：**一个合法终点都没找到时不许报穷尽**。
    ///
    /// 病因修掉之后这条路只剩一个来源 —— **预算在闭环之前就用光**。
    /// 用它来钉，比用一个已经不存在的死局来钉诚实。
    #[test]
    fn finding_no_legal_terminal_is_never_reported_as_exhaustive() {
        let mut s = audit_scene(12, &[100], &[card::DEFEND; 10], &[], &[card::STRIKE, card::BASH], 0);
        s.pending = Pending::FetchFromDiscard { remaining: 1 };
        // 预算 0：根上开着选牌 ⇒ 不评"到此为止"，而第一个孩子就被预算拦住
        let sol = solve_turn_budget(&s, &Threat::NONE, score::survive_first, 0);
        assert!(sol.line.is_empty());
        assert!(sol.line.score > i32::MIN, "没给分数");
        assert!(!sol.complete, "一个合法终点都没走到，不许报穷尽");
    }

    /// **planner 不许把开着的选牌带进下一回合。**
    ///
    /// 涅奥之怒（从弃牌堆拿 2 张进手）打完能量就光了，拿不拿在这一回合的分数里是平的。
    /// 2026-09-20 之前候选里有一条停在选牌之前的线，`end_turn_before_draw` 把 `Pending`
    /// 原样带进下一回合，下一回合拿**新的**手牌和弃牌堆做一次幻影选牌 ——
    /// 那条线的深层估值比做完选牌的高 650 分，而两者真实的未来一模一样。
    #[test]
    fn the_planner_never_hands_an_open_choice_to_the_next_turn() {
        use crate::plan::{plan_candidates, plan_report, Plan};
        let s = audit_scene(9, &[300], &[card::NEOW_WRATH], &[card::DEFEND; 10], &[card::POMMEL_STRIKE; 3], 1);
        let th = rollout::predicted_threat(&s);
        let cfg = Plan { depth: 2, ..Plan::default() };
        let (cands, _) = plan_candidates(&s, &cfg, &th);
        assert!(
            cands.iter().any(|(l, _)| l.acts().iter().any(|a| matches!(a, Action::Choose { .. }))),
            "场景立不住：候选里一条选牌都没有"
        );
        for (l, _) in &cands {
            let e = crate::solver::replay_line(&s, l.acts()).unwrap();
            assert_eq!(e.pending, Pending::None, "候选线停在选牌之前：{:?}", explain(&s, l.acts()));
        }
        let rep = plan_report(&s, &cfg, &th);
        let e = crate::solver::replay_line(&s, rep.line.acts()).unwrap();
        assert_eq!(e.pending, Pending::None, "planner 选的线停在选牌之前");
        assert_eq!(crate::step::end_turn_before_draw(e).pending, Pending::None);
    }

    /// **往抽牌堆顶放牌不算抽牌。** `Line::drew` 管的是"抽到什么是不是猜的"。
    ///
    /// 判据原来是 `n_draw` 变没变，头槌的选牌让它 +1 也算。线里做完选牌之后
    /// （开着的选牌不再算终点），`--live` 就在一张牌都没抽的线上印
    /// 「抽穿了已知牌序（中途洗过牌）」。反过来那一半也要守住：真抽了牌照样要标。
    #[test]
    fn putting_a_card_on_the_draw_pile_is_not_drawing() {
        let s = audit_scene(8, &[100], &[card::HEADBUTT], &[], &[card::STRIKE, card::DEFEND], 1);
        let sol = solve_turn_budget(&s, &Threat::NONE, score::survive_first, 200_000);
        assert!(
            sol.line.acts().iter().any(|a| matches!(a, Action::Choose { .. })),
            "场景立不住：线里没有那次放牌"
        );
        assert!(!sol.line.drew, "只往牌堆顶放了一张，却被记成抽过牌");

        let s = audit_scene(8, &[100], &[card::FLASH_OF_STEEL], &[card::STRIKE, card::DEFEND], &[], 1);
        let sol = solve_turn_budget(&s, &Threat::NONE, score::survive_first, 200_000);
        assert!(sol.line.drew, "亮剑抽了一张，却没被标出来");
    }

    /// `explain` 对**弃牌堆**选牌要去弃牌堆查名字（头槌 / 涅奥之怒）。
    ///
    /// `bin/solve` 的逐步清单 2026-08-30 修过，`explain` 一直按手牌查 ——
    /// 修 `Pending` 那一条之后线里的选牌多了，D=2 那一行会成批印出「选牌:<越界>」。
    #[test]
    fn explain_names_discard_choices_from_the_discard_pile() {
        let s = audit_scene(8, &[100], &[card::HEADBUTT], &[], &[card::STRIKE, card::DEFEND], 1);
        let names = explain(&s, &[Action::PlayCard { hand: 0, target: 0 }, Action::Choose { hand: 1 }]);
        assert_eq!(names, vec!["头槌".to_string(), "选牌:防御".to_string()]);
    }

    // -----------------------------------------------------------------------
    // 2026-09-22 代码审查修的三条。同一条规矩：先断言「场景立得住」，再断言行为。
    // -----------------------------------------------------------------------

    /// **注入式敌人回合和完整敌人回合，回合末必须逐字相同。**
    ///
    /// L2 的每一个叶子都走注入路径（`Threat::end_turn`），而 `injected_enemy_turn`
    /// 2026-09-22 之前只 fire 了 `EnemyTurnStart`，三档 `EnemyTurnEnd*` 一个都没发。
    /// 后果清一色是**乐观**（叶子上的敌人比真实的弱）：仪式/高压/领地意识不涨力量、
    /// 覆甲不给格挡、沉睡/熟睡不掉层、宿敌不切无实体。
    ///
    /// 招式取**纯攻击**（`ops` 只有一条 `EOp::Attack`），这样差出来的东西只可能
    /// 来自钩子，不可能来自招式自己的 op —— 少了这一条，测试会把
    /// 「小啃兽啃咬并戒备自带 5 格挡」误当成钩子的功劳。
    #[test]
    fn the_injected_enemy_turn_fires_the_same_turn_end_hooks_as_the_real_one() {
        use crate::ops::EOp;
        // 找一手纯攻击招
        let (def_ix, mv) = (0..ENEMIES.len())
            .find_map(|d| {
                let def = enemy_def(d as u16);
                (0..def.moves.len())
                    .find(|&m| matches!(def.moves[m].ops, [EOp::Attack { .. }]))
                    .map(|m| (d as u16, m))
            })
            .expect("内容表里一手纯攻击招都没有");

        let probe = |st: St, amt: i32| -> (State, State) {
            let mut s = State::new(80, 99);
            s.add_enemy(def_ix, 200);
            for _ in 0..8 {
                s.add_card(card::DEFEND, 0, 0);
            }
            let mut s = begin_combat(s);
            s.enemy_move[0] = mv as u8;
            s.enemies[0].set(st, amt);
            let threat = rollout::predicted_threat(&s);
            (threat.end_turn(s), step(s, Action::EndTurn))
        };

        // 覆甲：回合末给自己格挡（`Hook::EnemyTurnEndEarly`）
        let (inj, full) = probe(St::PlatedArmor, 7);
        assert_eq!(full.enemies[0].block, 7, "场景立不住：完整回合也没给格挡");
        assert_eq!(inj.enemies[0].block, full.enemies[0].block, "注入回合漏了敌人的覆甲");

        // 高压：回合末给自己加力量（`Hook::EnemyTurnEnd`）
        let (inj, full) = probe(St::HighVoltage, 2);
        assert_eq!(full.enemies[0].get(St::Strength), 2, "场景立不住：完整回合也没涨力量");
        assert_eq!(
            inj.enemies[0].get(St::Strength),
            full.enemies[0].get(St::Strength),
            "注入回合漏了敌人的高压"
        );

        // 沉睡：回合末掉 1 层（`Hook::EnemyTurnEnd`）—— 它决定族母什么时候醒
        let (inj, full) = probe(St::Asleep, 2);
        assert_eq!(full.enemies[0].get(St::Asleep), 1, "场景立不住：完整回合也没掉层");
        assert_eq!(
            inj.enemies[0].get(St::Asleep),
            full.enemies[0].get(St::Asleep),
            "注入回合漏了沉睡掉层"
        );
    }

    /// **喝不了的药水，`step` 也必须拒绝。**
    ///
    /// `legal_actions` 一直查 `potion_is_automatic`（瓶中精灵），`use_potion`
    /// 2026-09-22 之前没查 —— 两个入口对同一个局面给出两个答案（不变量 5）。
    ///
    /// 而且这条不一致不是无害的：`use_potion` 会先把槽位清空，
    /// 而瓶中精灵的 ops 是空的（效果在 `try_fairy`）⇒ **保命药水白白扔掉、
    /// 还返回一个"变了"的状态**，求解器那条 `ns == *s` 的守卫因此也接不住它。
    #[test]
    fn step_refuses_to_drink_an_automatic_potion() {
        let mut s = hand_of(3, 60, &[card::STRIKE, card::DEFEND], 3);
        s.potions[0] = potion::FAIRY;
        s.potion_slots = 3;

        // 场景立得住：`legal_actions` 确实不给这个动作
        let (acts, n) = legal_actions(&s);
        assert!(
            !acts[..n].iter().any(|a| matches!(a, Action::UsePotion { slot: 0, .. })),
            "场景立不住：legal_actions 居然给出了喝瓶中精灵"
        );

        assert_eq!(step(s, Action::UsePotion { slot: 0, target: 0 }), s, "step 喝掉了喝不了的药水");
        assert_eq!(s.potions[0], potion::FAIRY, "瓶中精灵被清掉了");

        // 它照常在"将要死"的那一刻发作 —— 拒绝喝不等于把它建废了
        let mut dying = s;
        dying.player.hp = 1;
        let dead = crate::step::end_turn_with_incoming(dying, &{
            let mut inc = crate::step::NO_INCOMING;
            inc[0] = (50, 1);
            inc
        });
        assert!(!dead.player_dead, "瓶中精灵没救人");
        assert_eq!(dead.potions[0], potion::NONE, "救人之后瓶子该没了");
    }
}
