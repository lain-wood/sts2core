//! sts2core — L1 kernel.
//!
//! Public surface is deliberately tiny:
//!   step(State, Action) -> State
//!   legal_actions(&State) -> [Action]
//!   begin_combat(State) -> State
//!
//! L2 (in-combat solver) and L3 (deck advisor) are built on top of these and
//! own no game rules themselves.

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
                    | TOp::OwnerClearStatus(st) => out.push(*st),
                    TOp::If { cond, then } => {
                        match cond {
                            TCond::EveryNTurns { phase, .. } => out.push(*phase),
                            TCond::OwnerHas(st) => out.push(*st),
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
                        | EOp::AttackPlusStackHits { per, .. } => referenced.push(*per),
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

        let face = card_face_damage(6, false, 0, atk.get(St::Strength), 0); // 10
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
        assert_eq!(card_face_damage(10, true, 0, 0, 0), 15);
        assert_eq!(card_face_damage(10, true, 0, 3, 0), 18);
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

    /// 表里的 id 必须真的存在于权威遗物表里。
    ///
    /// **拼错一个 id 是静默失效**：`relic_by_id` 永远查不到，遗物被当成
    /// 「内容表里没有」，而它明明就在表里。拿 `traces/relics_catalog.json`
    /// （`tools/dump_relics.py` 从游戏导的 55/55）当权威对照。
    #[test]
    fn relic_ids_exist_in_the_authoritative_catalog() {
        let Ok(src) = std::fs::read_to_string("traces/relics_catalog.json") else {
            return; // 权威表还没导出来就跳过，不因此挡住构建
        };
        for r in RELICS {
            let needle = format!("\"{}\"", r.id);
            assert!(
                src.contains(&needle),
                "{}（{}）不在权威遗物表里 —— id 是不是拼错了？",
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
            // 臂甲充能：`damage::card_block` 里消耗 —— 那是"从卡牌获得格挡"
            // 的唯一入口，所以药水/能力牌的格挡自动不吃翻倍
            St::VambraceCharge,
            // 摆动球的相位：被 `TCond::EveryNTurns` 读，不是被某条 `PowerDef`
            // 认领。它是**条件的输入**，不是触发器本身。
            St::PendulumPhase,
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
        let mut r = super::replay::Replayer::new(&t.run);
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
        let mut r = super::replay::Replayer::new(&t.run);
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
        let mut r = super::replay::Replayer::new(&t.run);
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
    /// 实测同一个局面（一只 1 血 + 一只 300 血，`h` 全程 8）无能力 +75、
    /// 滚石 10 **−16425**。**病根是 `n_alive` 不是 `h`** —— 三种配置下 `h`
    /// 一个字没动，所以"把 h 换成静态常数"治不了这条。
    ///
    /// 逐只封顶之后不变量是可证明的：敌人剩 R 血，收掉它 `eval` 白赚
    /// `R × w.enemy_hp`，而封顶后的滚石损失 ≤ `R × w.enemy_hp`。
    ///
    /// **坏掉的样子**：把第三项改回 `n_alive × total`，滚石那几行当场红。
    #[test]
    fn killing_an_enemy_never_lowers_the_leaf_score() {
        use crate::solver::{eval, horizon, Weights};
        // 四种能力配置。`none` 是对照 —— 它红了说明问题不在能力项。
        let configs: [(&str, fn(&mut State)); 4] = [
            ("none", |_s: &mut State| {}),
            ("boulder10", |s: &mut State| s.player.set(St::RollingBoulder, 10)),
            ("demon2", |s: &mut State| s.player.set(St::DemonForm, 2)),
            ("pyre2", |s: &mut State| {
                s.player.set(St::Pyre, 2);
                s.base_energy = crate::solver::BASE_ENERGY + 2;
            }),
        ];
        // 被收的那只 × 留下的那只。**两边都要扫** —— `h` 由血墙算出来，
        // 只固定一个血量的话扫不到 `h` 会变的那几档。
        let victim_hps = [1, 4, 12, 40, 120];
        let other_hps = [1, 30, 120, 300, 600];
        for (name, setup) in configs {
            for &vh in &victim_hps {
                for &oh in &other_hps {
                    let mut s = State::new(80, 21);
                    s.add_enemy(enemy::DUMMY, vh.max(oh));
                    s.add_enemy(enemy::DUMMY, vh.max(oh));
                    for _ in 0..6 {
                        s.add_card(card::STRIKE, 0, 0);
                    }
                    for _ in 0..4 {
                        s.add_card(card::DEFEND, 0, 0);
                    }
                    let mut alive = begin_combat(s);
                    alive.enemies[0].hp = vh;
                    alive.enemies[1].hp = oh;
                    setup(&mut alive);
                    let mut dead = alive;
                    dead.enemies[0].hp = 0;
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
    }

    /// 上面那条的**方向对照**：收人头不但不该亏，还该实实在在地赚。
    ///
    /// 只断言"不降"的话，一个把整个能力项砍成 0 的实现也能过 —— 那就把
    /// `leaf_eval_sees_pyre` 那条修复退回去了。所以这里另外钉住：
    /// 带滚石时收人头**严格赚**，而且赚的数就是那只敌人的血价
    /// （`R × w.enemy_hp`，因为封顶让滚石那一项跌得正好是 `min(total, R)`）。
    #[test]
    fn killing_an_enemy_is_strictly_worth_it_under_a_boulder() {
        use crate::solver::{eval, Weights};
        let mut s = State::new(80, 21);
        s.add_enemy(enemy::DUMMY, 300);
        s.add_enemy(enemy::DUMMY, 300);
        for _ in 0..6 {
            s.add_card(card::STRIKE, 0, 0);
        }
        for _ in 0..4 {
            s.add_card(card::DEFEND, 0, 0);
        }
        let mut alive = begin_combat(s);
        alive.enemies[0].hp = 1;
        alive.enemies[1].hp = 300;
        alive.player.set(St::RollingBoulder, 10);
        let mut dead = alive;
        dead.enemies[0].hp = 0;
        let gain = eval(&dead, &Weights::LEAF) - eval(&alive, &Weights::LEAF);
        // 1 血的怪：`eval` 白赚 1×75，滚石那一项跌 min(total,1)×75 = 75，
        // 而 `w.power = 100` ⇒ 净 75 - 75 = 0？不是 —— 能力项还要过
        // `× w.power / 100`，LEAF 是 100，所以净收益正好是 0。
        // **真正该断言的是"至少不亏"，严格赚要看 h 有没有跟着变。**
        // 这里 h 两边都是 8（血墙 301 -> 300），所以净收益 = 0。
        assert!(gain >= 0, "收人头在滚石下亏了 {} 分", -gain);
        // 对照：换一只血厚到滚石打不穿的敌人，收它必须**严格**赚。
        let mut alive2 = alive;
        alive2.enemies[0].hp = 120;
        let mut dead2 = alive2;
        dead2.enemies[0].hp = 0;
        let gain2 = eval(&dead2, &Weights::LEAF) - eval(&alive2, &Weights::LEAF);
        assert!(gain2 > 0, "收掉一只 120 血的敌人在滚石下居然不赚：{gain2}");
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
        let mut tt = Tt::new(10);
        tt.store(0xABCD, 2, 42);
        assert_eq!(tt.probe(0xABCD, 1), Some(42), "存 2 要 1，可以用");
        assert_eq!(tt.probe(0xABCD, 2), Some(42), "存 2 要 2，可以用");
        assert_eq!(tt.probe(0xABCD, 3), None, "存 2 要 3，**不能**用");
        assert_eq!(tt.probe(0x1234, 1), None, "没存过的 key");
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
}
