//! `step(state, action) -> state`: the pure transition function.
//!
//! `State` is `Copy`, so this genuinely takes ownership of a value and returns a
//! new one. Search code can keep states on the stack and never allocate.

use crate::content::{card, card_ops, enemy_def, playable, HAND_END, POWERS, TURN_SCOPED};
use crate::damage::*;
use crate::ops::*;
use crate::state::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    PlayCard { hand: u8, target: u8 },
    UsePotion { slot: u8, target: u8 },
    EndTurn,
    Choose { hand: u8 },
}

pub const MAX_ACTIONS: usize = 128;

/// Effective energy cost, accounting for 踩踏's discount and 无情猛攻's free attack.
///
/// 层次照 [源码] `CardEnergyCost.GetWithModifiers`：**本地**改费（这一张实例自己的：免费 / 狂乱逃离的增量 /
/// 踩踏的减费，都不夹 0）-> **全局**钩子（缠结 +N）-> `Late` 钩子（无情猛攻置 0）-> 最后夹 0 一次。
/// 缠结为 0 时逐字等于加它之前的样子（免费那一支原来直接 return 0，现在是 `max(0, 0 − 踩踏减的)` = 0）。
pub fn effective_cost(s: &State, hand_ix: usize) -> i32 {
    let inst = s.cards[s.hand[hand_ix] as usize];
    let d = card(inst.id);
    let base = if inst.upgraded() { d.cost_upg } else { d.cost };
    // 这一张实例本回合免费（技能药水生成的牌）。挂在实例上，不是牌名上。
    // [源码] `SetToFreeThisTurn` 是一条**本地**的「置 0」，全局钩子照样加在它上面 ——
    // 所以缠结下药水 / 地狱之刃给的免费攻击牌要 1 费，不是 0。
    let free = inst.flags & F_FREE_THIS_TURN != 0;
    // X 费：**花光当前所有能量**（[源码] `CapturedXValue = 当前能量`）。
    // 放在免费之后：免费的 X 费牌 X = 0，和游戏一致（捕获的是"要花的能量"）。
    if crate::content::costs_x(inst.id) {
        return if free { 0 } else { s.energy.max(0) };
    }
    // 无情猛攻是 `TryModifyEnergyCostInCombatLate`：在缠结**之后**置 0，所以它压过缠结。
    if matches!(d.kind, Kind::Attack) && s.free_attack > 0 {
        return 0;
    }
    // 这一张实例的费用增量（狂乱逃离）。放在免费/X 费之后、减费之前：
    // 免费压倒一切，而减费是在"这张牌现在要多少钱"的基础上再减。
    let mut c = if free { 0 } else { base + inst.cost_delta as i32 };
    if d.cost_minus_attacks {
        c -= s.attacks_played;
    }
    // 缠结加在**夹 0 之前**：踩踏减到 −1 再 +1 是 0，不是 1（本地改费不夹 0，见 [源码] `LocalCostModifier.Modify`）。
    c += tangled_cost_addend(&s.player, inst.id);
    if c < 0 {
        c = 0;
    }
    c
}

/// 缠结给这张牌加多少费（[源码] `TangledPower.TryModifyEnergyCostInCombat`：带「缠身」的牌 +`Amount`，
/// 而挂上那一刻全部攻击牌都带上了）。X 费牌 0 —— `GetWithModifiers` 对 `CostsX` 提前 return，钩子根本不跑。
///
/// **两个客户共用这一份**：`effective_cost`（内核自己算费）和 `replay::sync`（mod 报的费用是
/// `GetAmountToSpend`，**已经含缠结**，要先扣掉再判「这张实例被永久改过费」—— 不扣的话
/// 每张攻击牌记成 `cost_delta = +1`，内核再加一遍缠结，双算）。
#[inline]
pub fn tangled_cost_addend(player: &Entity, id: u16) -> i32 {
    if matches!(card(id).kind, Kind::Attack) && !crate::content::costs_x(id) {
        player.get(St::Tangled)
    } else {
        0
    }
}

/// 这一刻**还能不能出牌**（轰鸣，[源码] `RingingPower.ShouldPlay`）。
///
/// **必须是一个函数、两个客户共用**：`legal_actions`（求解器先问再走）和
/// `rollout::fast_play_turn`（直接走）。抄两份的那一版当场被 rollout 的
/// P3(a) 抓住 —— 6 次"走了 legal_actions 不认的动作"。那条判据存在的
/// 全部理由就是这个，别把它变成两份实现。
///
/// **只挡牌，不挡药水** —— `ShouldPlay` 的入参是 `CardModel`。
#[inline]
pub fn cards_locked(s: &State) -> bool {
    (s.player.get(St::Ringing) > 0 && s.cards_played > 0)
        // 懒惰（知识恶魔的诅咒，[源码] `SlothPower.ShouldPlay => cardsPlayedThisTurn < Amount`）：
        // 本回合打满 N 张就锁死。和轰鸣同一类（"这一回合最多出几张"），收在同一个判据里。
        || (s.player.get(St::Sloth) > 0 && s.cards_played >= s.player.get(St::Sloth))
}

/// 选牌候选去重：**同一张牌的两个副本完全可互换**，展开两次是白搜。
///
/// 判据是整个 `CardInst` 逐字节相同（id + flags + bonus + cost_delta）。
/// 光比 `id` 会把「升级过的打击」和没升级的当成一张；
/// `bonus`（暴走那种本场变强）和 `cost_delta`（狂乱逃离每打一次自己 +1 费）
/// 同样是这一张实例的身份，不能忽略。
///
/// **为什么剪掉不丢解**：把候选 i 换成等价的 j，之后任何一条线都有一条
/// 一一对应的线（只是牌区里的下标换了个位置），终局逐字段等价。
///
/// **为什么置换表接不住这件事**：两个副本是 `s.cards` 里两个不同的下标，
/// 选完之后牌区的字节排布不同 ⇒ 状态不"逐字节相同" ⇒ 合并不了。
/// 所以只能在展开这一层剪。
///
/// 2026-08-30 加的。头槌那类「从弃牌堆挑一张」在牌堆变厚之后
/// 一个节点能岔出十几条，而其中大半是同一张打击/防御。
#[inline]
fn choice_is_duplicate(s: &State, pile: &[u8], i: usize, excl: u8) -> bool {
    let a = s.cards[pile[i] as usize];
    let mut j = 0;
    while j < i {
        // 被排除的那张**也不参与去重比较**：它自己不是候选，拿它去挡掉
        // 后面一张一模一样的牌就是真丢解（弃牌堆里有第二张头槌的时候）。
        if pile[j] != excl && s.cards[pile[j] as usize] == a {
            return true;
        }
        j += 1;
    }
    false
}

pub fn legal_actions(s: &State) -> ([Action; MAX_ACTIONS], usize) {
    let mut out = [Action::EndTurn; MAX_ACTIONS];
    let mut n = 0usize;
    if s.combat_over {
        return (out, 0);
    }

    match s.pending {
        Pending::None => {}
        Pending::ExhaustFromHand { .. } | Pending::PutToDrawPile { .. } => {
            for i in 0..s.n_hand as usize {
                if choice_is_duplicate(s, &s.hand, i, u8::MAX) {
                    continue;
                }
                if n < MAX_ACTIONS {
                    out[n] = Action::Choose { hand: i as u8 };
                    n += 1;
                }
            }
            return (out, n);
        }
        // 升级选牌的候选按 `IsUpgradable` 筛（[源码] `CardSelectCmd`）。
        // 兜底：万一一张都筛不出来（同步进来的局面可能带 `Pending`），
        // 退回"全都可选"而不是返回空集 —— 空集会让搜索死在这里。
        Pending::UpgradeInHand { .. } => {
            for i in 0..s.n_hand as usize {
                let c = s.hand[i] as usize;
                if choice_is_duplicate(s, &s.hand, i, u8::MAX) {
                    continue;
                }
                if crate::content::upgradable(s.cards[c].id)
                    && s.cards[c].flags & F_UPGRADED == 0
                    && n < MAX_ACTIONS
                {
                    out[n] = Action::Choose { hand: i as u8 };
                    n += 1;
                }
            }
            if n == 0 {
                for i in 0..s.n_hand as usize {
                    if choice_is_duplicate(s, &s.hand, i, u8::MAX) {
                        continue;
                    }
                    if n < MAX_ACTIONS {
                        out[n] = Action::Choose { hand: i as u8 };
                        n += 1;
                    }
                }
            }
            return (out, n);
        }
        // **`hand` 这个字段名在这一支里指的是弃牌堆下标**（候选集在哪个牌堆
        // 由 `Pending` 决定）。名字是历史遗留 —— 2026-08-30 因为它，
        // `solve --live` 印错过一次卡名（按手牌查名字，印出弃牌堆里没有的牌）。
        Pending::FetchFromDiscard { .. } | Pending::DiscardToDrawTop { .. } => {
            // 头槌**排除它自己**（见 `Pending::DiscardToDrawTop` 的注释）。
            // 那张牌是在设完 `Pending` **之后**才进弃牌堆的，所以到这一步
            // 它已经躺在候选里了 —— 不排除就等于允许"头槌把自己捞回来再打一次"，
            // 而游戏不接受这条线。2026-08-30 实战时求解器真的建议过它。
            let excl = match s.pending {
                Pending::DiscardToDrawTop { exclude, .. } => exclude,
                _ => u8::MAX,
            };
            for i in 0..s.n_disc as usize {
                if s.disc[i] == excl {
                    continue;
                }
                if choice_is_duplicate(s, &s.disc, i, excl) {
                    continue;
                }
                if n < MAX_ACTIONS {
                    out[n] = Action::Choose { hand: i as u8 };
                    n += 1;
                }
            }
            return (out, n);
        }
    }

    // 轰鸣（[源码] `RingingPower.ShouldPlay`）：被附魔的牌，只要本回合
    // **已经出过牌**就打不出。它 afflict 的是所有牌（含中途进场的），
    // 所以等价于「这一回合最多出 1 张」。**这是规则修饰，不是数值修饰** ——
    // 漏掉它内核会给出游戏根本不接受的线，`legal_actions` 和游戏就不一致了。
    for i in 0..(if cards_locked(s) { 0 } else { s.n_hand as usize }) {
        let inst = s.cards[s.hand[i] as usize];
        if !playable(inst.id) {
            continue;
        }
        if effective_cost(s, i) > s.energy {
            continue;
        }
        // 手牌里**完全相同**的牌只保留第一张。
        if (0..i).any(|j| s.cards[s.hand[j] as usize] == inst) {
            continue;
        }
        let d = card(inst.id);
        if d.targeted {
            for e in 0..s.n_enemies as usize {
                if s.enemies[e].alive() && n < MAX_ACTIONS {
                    out[n] = Action::PlayCard { hand: i as u8, target: e as u8 };
                    n += 1;
                }
            }
        } else if n < MAX_ACTIONS {
            out[n] = Action::PlayCard { hand: i as u8, target: 0 };
            n += 1;
        }
    }

    // 药水动作。
    //
    // **按真实槽位数扫，不是按容量。** `MAX_POTIONS` 是数组容量（10，
    // 留给炼金宝匣那种 +4 的遗物），而这里是 `legal_actions` 的热路径 ——
    // 每个搜索节点都跑一次，按容量扫等于白扫 7 个空槽。
    // （实测这一句本身在 bench 里几乎看不出来 —— bench 的 `State::new`
    // 槽位就是 3，两种写法扫的次数一样。改它是为了**带腰带的真实局面**，
    // 那时容量 10 而槽位 5。别把这条当成性能优化的证据。）
    for p_idx in 0..(s.potion_slots as usize).min(MAX_POTIONS) {
        let pot_id = s.potions[p_idx];
        if pot_id == potion::NONE || pot_id == potion::UNKNOWN {
            continue;
        }
        // [源码] `PotionUsage.Automatic` 的喝不了（瓶中精灵）。
        // 不挡的话搜索会多出一条"喝掉救命药水什么也不发生"的分支。
        if crate::ops::potion_is_automatic(pot_id) {
            continue;
        }
        let pdef = potion_def(pot_id);
        if pdef.targeted {
            for e in 0..s.n_enemies as usize {
                if s.enemies[e].alive() && n < MAX_ACTIONS {
                    out[n] = Action::UsePotion { slot: p_idx as u8, target: e as u8 };
                    n += 1;
                }
            }
        } else if n < MAX_ACTIONS {
            out[n] = Action::UsePotion { slot: p_idx as u8, target: 0 };
            n += 1;
        }
    }

    if n < MAX_ACTIONS {
        out[n] = Action::EndTurn;
        n += 1;
    }
    (out, n)
}

#[inline]
fn scaled_base(s: &State, base: i32, scale: Scale, tgt: usize) -> i32 {
    match scale {
        Scale::None => base,
        Scale::PerExhaust(n) => base + n * s.n_exh as i32,
        Scale::PerTargetVuln(n) => base + n * s.enemies[tgt].get(St::Vulnerable),
        Scale::PlayerBlock => base + s.player.block,
        // 整个牌组，四个牌区都算（玩家确认）。`s.cards` 本来就是全部牌实例，
        // 不分区，所以直接数它就是"整个牌组"。
        Scale::PerStrikeCard(n) => {
            let c = (0..s.n_cards as usize)
                .filter(|&i| crate::content::is_strike(s.cards[i].id))
                .count() as i32;
            base + n * c
        }
    }
}

fn cond_holds(s: &State, c: Cond) -> bool {
    match c {
        Cond::ExhaustedThisTurn => s.exhausted_this_turn > 0,
        Cond::LostHpThisTurn => s.hp_lost_this_turn > 0,
        Cond::NoAttackInHand => (0..s.n_hand as usize)
            .all(|i| !matches!(card(s.cards[s.hand[i] as usize].id).kind, Kind::Attack)),
        Cond::LastAttackKilled => s.last_kill,
    }
}

/// **给玩家/敌人加格挡的唯一入口。** 势不可当挂在这里。
///
/// 和 `exhaust_card` 同一个道理：内核里有四条加格挡的路径
/// （卡牌 `Op::Block`、拳斗、能力牌 `TOp::OwnerBlock`、敌人 `EOp::Block`），
/// 任何新路径都必须走它，否则触发器会**静默失效** —— 那种 bug 只有在
/// 实战里带着这张牌打一场才看得出来。
///
/// `amount <= 0` 不触发，这条是 [源码] 明写的（`AfterBlockGained` 第一句）。
fn gain_block(s: &mut State, who: Owner, amount: i32, depth: u8) {
    // [源码] `AfterBlockGained` 第一句是 `if (!(amount <= 0m) && ...)`，
    // 逐字对应成这一条守卫。写成两条（先 `== 0` 早返回、再 `> 0` 判触发）
    // 是有害的：那样删掉后一条也不会有测试变红，守卫就成了摆设。
    if amount <= 0 {
        return;
    }
    owner_mut(s, who).block += amount;
    // **谁获得的格挡要传下去**：[源码] 势不可当只认自己那一份
    // （`JuggernautPower.AfterBlockGained` 里的 `creature == base.Owner`）。
    // 收口在 `fire_ctx` 的 `only_ctx` / `player_fires` 两条，见那里。
    let ctx = match who {
        Owner::Player => usize::MAX,
        Owner::Enemy(i) => i,
    };
    fire_ctx(s, Hook::GainBlock, depth + 1, ctx);
}

/// 随机挑一个**活着**的敌人。用 `rng.enemy` 流（和洗牌流分开，
/// 这样出牌顺序不会因为触发了一次反伤就把手牌洗乱）。
/// 全死光了返回 `None`。
fn random_living_enemy(s: &mut State) -> Option<usize> {
    let mut n = 0;
    for e in 0..s.n_enemies as usize {
        if s.enemies[e].alive() {
            n += 1;
        }
    }
    if n == 0 {
        return None;
    }
    let mut k = (crate::state::next_u64(&mut s.rng.enemy) % n as u64) as usize;
    for e in 0..s.n_enemies as usize {
        if s.enemies[e].alive() {
            if k == 0 {
                return Some(e);
            }
            k -= 1;
        }
    }
    None
}

/// 一张牌的**卡面基础伤害**（升级态算进去，含实例 bonus）。痛殴要读它。
/// 找的是 ops 里第一个伤害类 op；没有伤害 op 的牌返回 0。
fn base_damage_of(inst: CardInst) -> i32 {
    let ops = card_ops(inst.id, inst.upgraded());
    for op in ops {
        match *op {
            Op::Damage { base, .. }
            | Op::DamageAll { base, .. }
            | Op::DamagePerHpLossHit { base } => return base + inst.bonus as i32,
            _ => {}
        }
    }
    0
}

/// 把一只敌人放进场上。**两个召唤入口共用它**（敌人出招 `EOp::Summon`、
/// 触发器 `TOp::SummonN`），免得两份代码长歪。
///
/// **有空位就新开，满了才复用死掉的槽位。** 顺序不能反：
/// 寄生物是在**宿主自己的死亡触发里**召唤的，先复用的话第一只 Wriggler
/// 会顶进宿主那一格，尸体凭空消失。测试 `killing_the_parasite_...` 守着这条。
///
/// **复用时必须把出招历史一起清掉** ——
/// `enemy_hist` / `enemy_used` 是按槽位存的，不清就等于让新召出来的敌人
/// 继承前一个占用者的历史，`CannotRepeat` / `UseOnlyOnce` 会莫名其妙地失灵。
/// （这一条在抽 helper 之前就是错的，只是当时没有敌人在复用的槽位上带机器。）
/// 返回新敌人落在哪个槽位。`None` = 场上塞不下了（不 panic，见不变量 5）。
/// 调用方拿它去做"召唤完还要改这一只"的事（库存：把层数设成我的层数 −1）。
fn summon_one(s: &mut State, def: u16, hp: i32) -> Option<usize> {
    let slot = ((s.n_enemies as usize) < MAX_ENEMIES)
        .then(|| {
            let i = s.n_enemies as usize;
            s.n_enemies += 1;
            i
        })
        .or_else(|| (0..s.n_enemies as usize).find(|&i| !s.enemies[i].alive()));
    if let Some(i) = slot {
        s.enemies[i] = Entity::new(hp);
        s.enemy_def[i] = def;
        s.enemy_move[i] = 0;
        s.enemy_hist[i] = [u8::MAX; crate::state::ENEMY_HIST];
        s.enemy_used[i] = 0;
        for (st, amt) in enemy_def(def).start_status {
            s.enemies[i].add(*st, *amt);
        }
    }
    slot
}

/// 打敌人一下。**`powered` 决定过不过乘区**（[源码] `IsPoweredAttack`）：
/// 卡牌攻击是 `true`，药水 / 遗物 / 能力牌打出的伤害是 `false`。
/// 两条路共用这一个函数，所以格挡吸收、蜷身、死亡触发不可能长歪。
///
/// # `hook_depth`：这一下打出来的钩子在第几层
///
/// **这一笔伤害是玩家自己打的（0），还是某个触发器打的（那个触发器的层数+1）。**
/// 打牌那条路一律传 0；`run_ops` 那条（触发器造成的伤害）传 `depth + 1`。
///
/// 不带它的话 `MAX_HOOK_DEPTH` 在**整条伤害路径上失效** ——
/// 这四个钩子原来一律写死 0，于是任何「触发器打人 -> 挨打的触发器再打人」
/// 的环都数不到上限，**表现是爆栈，不是一条红**。
/// [实测] 2026-09-12：势不可当（获得格挡就打一下）+ 蜷身（挨打就获得格挡）
/// 在整幕链的一条链上把工作线程的栈打爆了；那个环每绕一圈 `depth` 都被重置成 0。
fn hit_enemy_with(s: &mut State, tgt: usize, face: i32, powered: bool, hook_depth: u8) {
    if !s.enemies[tgt].alive() {
        s.last_damage = 0;
        s.last_kill = false;
        s.last_hit_attack = false;
        s.last_hit_unblocked = false;
        return;
    }
    let d = if powered {
        apply_modifiers(face, &s.player, &s.enemies[tgt])
    } else {
        crate::damage::apply_modifiers_unpowered(face, &s.enemies[tgt])
    };
    // 记的是过完乘区、**扣格挡之前**的数字，拳斗/万向斩读它
    s.last_damage = d;
    // 荆棘那一类「挨我一下攻击就反弹」。**放在 `absorb` 之前**，
    // 而且**只有攻击**才算 —— 两条都是 [源码] 定的，见 `Hook::EnemyAttacked`：
    // `ThornsPower.BeforeDamageReceived` 的门是 `props.IsPoweredAttack()`，
    // 而这个钩子在 `DamageBlockInternal` 之前。
    //
    // 顺序的两个可观测后果：**格挡挡住也照样反弹**、**打死它也照样反弹**。
    // 写在 `absorb` 之后就都没了，而且不会报错。
    if powered {
        fire_ctx(s, Hook::EnemyAttacked, hook_depth, tgt);
    }
    let prev_block = s.enemies[tgt].block;
    let through = absorb(&mut s.enemies[tgt], d);
    // 这一下的两个性质，给 `EnemyDamaged` 上的规则读（`TCond::LastHitWasAttack` /
    // `LastHitUnblocked`）。和 `last_kill` 同一类，所以在点火**之前**写好：
    // 人体蜂房只认攻击、不管打没打穿；熟睡只认打穿、不管是不是攻击。
    s.last_hit_attack = powered;
    s.last_hit_unblocked = through > 0;
    // 狂宴读这个。放在 `absorb` 之后、触发钩子之前 —— 钩子里可能再打死别人，
    // 那不该算成"这一下打死的"。
    s.last_kill = !s.enemies[tgt].alive();
    // 蜷身那一类「被命中时…」。**伤害先落地再触发** —— 实测就是这个顺序
    // （蜷身给的 14 点格挡没能吃掉触发它的那 7 点）。
    // 打死了就不触发：给尸体加格挡没有意义，也免得多一层无谓的钩子递归。
    if s.enemies[tgt].alive() {
        // 钻地（[源码] `BurrowedPower.AfterBlockBroken`）：攻击破盾时被打进眩晕
        if prev_block > 0 && s.enemies[tgt].block == 0 && s.enemies[tgt].get(St::Burrowed) > 0 {
            s.enemies[tgt].set(St::Burrowed, 0);
            s.enemy_move[tgt] = 3;
            s.enemies[tgt].set(St::MoveForcedThisTurn, 1);
            s.enemy_hist[tgt] = [u8::MAX; crate::state::ENEMY_HIST];
        }
        // 胆小（[源码] `SkittishPower`）：受到未被格挡的卡牌攻击时，本回合第一次获得 6 点格挡
        if powered
            && through > 0
            && s.enemies[tgt].get(St::Skittish) > 0
            && s.enemies[tgt].get(St::SkittishTriggered) == 0
        {
            let blk = s.enemies[tgt].get(St::Skittish);
            s.enemies[tgt].set(St::SkittishTriggered, 1);
            s.enemies[tgt].block += blk;
        }
        fire_ctx(s, Hook::EnemyDamaged, hook_depth, tgt);
    } else {
        // 死了 ⇒ 走另一条边。两个持有者各取所需：
        // 玩家的地精之角（给能量+抽牌）、**死的那一只**自己的寄生物（召唤）。
        // 必须传 ctx，否则场上别的带寄生物的敌人也会跟着召唤。
        fire_ctx(s, Hook::EnemyDied, hook_depth, tgt);
        // 同一件事的另一半：**活着的其他敌人**的反应（蟹之怒）。
        fire_ctx(s, Hook::AllyDied, hook_depth, tgt);
    }
}

/// 药水 / 遗物 / 能力牌的伤害：**只过难以杀灭和无实体**，见
/// [`crate::damage::apply_modifiers_unpowered`]。
#[inline]
fn hit_enemy_unpowered(s: &mut State, tgt: usize, face: i32, hook_depth: u8) {
    hit_enemy_with(s, tgt, face, false, hook_depth);
}

/// 给谁上 status。返回**实际有几个敌人吃到了**（被人工制品挡掉的不算），
/// 因为凶恶是按"给出去几次易伤"触发的 —— 闪电霹雳给全体易伤时
/// **每个敌人各触发一次**，不是整张牌只触发一次。
fn apply_status(s: &mut State, tgt: Tgt, st: St, amt: i32, target: usize, src: Source) {
    // 减力量（黑暗镣铐/凌虐）/ 减敏捷也是 debuff，人工制品挡得住。
    // [源码] `GetTypeForAmount` 定的，见 `damage::is_debuff_amount`。
    let debuff = crate::damage::is_debuff_amount(st, amt);
    let amt = modify_status_amount_received(s, matches!(tgt, Tgt::Me), st, amt);
    let mut landed_on_enemies = 0;
    match tgt {
        Tgt::Me => {
            if !artifact_absorbs(&mut s.player, debuff) {
                s.player.add(st, amt);
            }
        }
        Tgt::Enemy => {
            if s.enemies[target].alive() && !artifact_absorbs(&mut s.enemies[target], debuff) {
                s.enemies[target].add(st, amt);
                landed_on_enemies += 1;
            }
        }
        Tgt::AllEnemies => {
            for e in 0..s.n_enemies as usize {
                if s.enemies[e].alive() && !artifact_absorbs(&mut s.enemies[e], debuff) {
                    s.enemies[e].add(st, amt);
                    landed_on_enemies += 1;
                }
            }
        }
    }
    // 只有**我**给出去的易伤才算（敌人给我上易伤走的是 `EOp::PlayerStatus`，
    // 不经过这里），正好对上卡面的"每当**你**给予易伤时"。
    //
    // **药水给的易伤不算**（玩家判定 2026-08-16）：喝易伤药水不触发凶恶。
    // 卡面对这条是欠定的，所以按玩家的答案来，不按我的推断。
    if st == St::Vulnerable && src == Source::Card {
        for _ in 0..landed_on_enemies {
            fire(s, Hook::ApplyVuln, 0);
        }
    }
}

// ---------------------------------------------------------------------------
// 触发器
// ---------------------------------------------------------------------------

/// 触发器的宿主。恶魔形态挂在玩家身上，激怒挂在敌人身上，一套词汇覆盖两边。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Owner {
    Player,
    Enemy(usize),
}

#[inline]
fn owner_ref(s: &State, w: Owner) -> &Entity {
    match w {
        Owner::Player => &s.player,
        Owner::Enemy(i) => &s.enemies[i],
    }
}

#[inline]
fn owner_mut(s: &mut State, w: Owner) -> &mut Entity {
    match w {
        Owner::Player => &mut s.player,
        Owner::Enemy(i) => &mut s.enemies[i],
    }
}

/// 触发嵌套的深度上限。
///
/// 钩子**必须**能套钩子：绯红披风回合开始掉 1 点血要唤醒撕裂，
/// 这是放血流的核心组合，不接上等于把一整套构筑算废。
/// 但互相触发的组合天然有死循环风险，所以给一个硬上限 ——
/// 目前见过的最深是 2（绯红披风 -> 撕裂），4 留足了余量。
pub(crate) const MAX_HOOK_DEPTH: u8 = 4;

/// 在 `hook` 这个时点，把所有挂着对应 power 的实体都跑一遍。
///
/// 表在 `content::POWERS`，这里**不认识任何具体的 status** —— 这正是
/// 「加一张能力牌 = 加一行表」的兑现处。
/// 没有上下文的钩子（绝大多数）。
pub(crate) fn fire(s: &mut State, hook: Hook, depth: u8) {
    fire_ctx(s, hook, depth, usize::MAX);
}

/// Restore a vanished reviving enemy by applying its real death rules in an
/// isolated state. Player death rewards must not be repeated during sync.
pub(crate) fn reviving_enemy_snapshot(def: u16, max_hp: i32, adaptable: i32, asc: u8) -> Entity {
    let mut scratch = State::new(1, 1);
    scratch.ascension = asc;
    scratch.add_enemy(def, max_hp);
    scratch.enemies[0].hp = 0;
    scratch.enemies[0].set(St::Adaptable, adaptable);
    fire_ctx(&mut scratch, Hook::EnemyDied, 0, 0);
    scratch.enemies[0]
}

/// `ctx` 是"跟这次触发有关的那个敌人"的下标：`Attacked` 用它表示攻击者，
/// 让火焰屏障知道该反伤谁。其他钩子传 `usize::MAX`。
fn fire_ctx(s: &mut State, hook: Hook, depth: u8, ctx: usize) {
    if depth > MAX_HOOK_DEPTH {
        return;
    }
    for def in POWERS {
        if def.hook != hook {
            continue;
        }
        // `EnemyDamaged` 的持有者**必须是挨打的那一只**，别的敌人不该跟着触发
        // —— 这和 `Attacked` 正好相反：那个的持有者是玩家、`ctx` 是攻击者。
        // 一个钩子的 `ctx` 到底指谁，是每个钩子自己的语义，不能一概而论。
        // 敌人侧**只有 ctx 那一只**触发的钩子。`EnemyDied` 也在其中：
        // 死的是谁，就只有谁的能力发作（寄生物只召唤自己那 4 只）。
        //
        // **`GainBlock` 的 `ctx` 是"谁获得了格挡"**（玩家时是 `usize::MAX`）。
        // [源码] `JuggernautPower.AfterBlockGained` 的第二个条件就是
        // `creature == base.Owner` —— 势不可当只对**自己**获得的格挡发作。
        // 不带这条的话，敌人蜷身获得格挡会触发**我的**势不可当，
        // 而那一下又把蜷身打醒：两条规则各自没错，合起来是个无限环。
        // [实测] 2026-09-12 整幕链的一条链上真的绕进去了（栈爆在工作线程里）。
        // `AttackUnblocked` 的 `ctx` 是攻击者，持有者就是它本身（纸伤难愈只认自己打穿的）。
        let only_ctx = matches!(
            hook,
            Hook::EnemyDamaged | Hook::EnemyAttacked | Hook::EnemyDied | Hook::GainBlock | Hook::AttackUnblocked
        );
        // `AllyDied` 正好相反：**除了 ctx 之外**的活敌人才触发
        let except_ctx = hook == Hook::AllyDied;
        // 玩家侧照常触发的钩子。`EnemyDamaged` 是唯一的例外
        // —— `EnemyDied` 玩家是要触发的（地精之角）。
        // `GainBlock` 那一条是"获得格挡的不是我就不发作"，见上面。
        let player_fires =
            !matches!(hook, Hook::EnemyDamaged | Hook::EnemyAttacked | Hook::AllyDied | Hook::AttackUnblocked)
            && !(hook == Hook::GainBlock && ctx != usize::MAX);
        // **`Attacked` 只在玩家侧触发。** 它的语义就是"我挨了一下攻击"
        //（唯一的触发点是 `take_attack_hit`，那是玩家专用的），
        // 敌人身上同名的 status 跟着发作是没有意义的。
        //
        // 以前这条无所谓：`Attacked` 只有火焰屏障一个消费者，而没有敌人带火焰屏障。
        // 加了荆棘之后就有意义了 —— 多刺蟾蜍带 5 层荆棘，不拦住的话
        // 「我挨了碾碎爪一下」会让蟾蜍也反弹一次，而且反弹给碾碎爪。
        let enemies_fire = hook != Hook::Attacked;
        // **允许死人触发**。只有 `EnemyDied` 需要：触发的前提就是它死了，
        // 按活人过滤等于这个钩子永远不发作。这一条踩过：寄生物的召唤
        // 一次都没执行，因为宿主在触发那一刻已经是死的。
        // **允许死人触发**。`EnemyDied` 的理由见上；`EnemyTurnStart` 是给
        // 实验体的复苏用的 —— 它被砍死之后血量是 0、要等到自己的回合开始
        // 才回满，那条规则必须在"还是死的"时候跑得起来。
        let allow_dead = matches!(hook, Hook::EnemyDied | Hook::EnemyTurnStart);
        if player_fires {
            let n = s.player.get(def.st);
            if n > 0 {
                run_trigger(s, def, Owner::Player, n, depth, ctx);
            }
        }
        for e in 0..s.n_enemies as usize {
            if !enemies_fire {
                break;
            }
            if !allow_dead && !s.enemies[e].alive() {
                continue;
            }
            if only_ctx && e != ctx {
                continue;
            }
            if except_ctx && e == ctx {
                continue;
            }
            let n = s.enemies[e].get(def.st);
            if n > 0 {
                run_trigger(s, def, Owner::Enemy(e), n, depth, ctx);
            }
        }
    }
}

/// 触发器里的条件求值。**只读**，一律看持有者一侧的局面。
///
/// `stacks` 是**触发那一刻**这个 power 的层数，给 `OwnerHpAtMostStacks` 用
///（耕地的 150 存在层数里，不是常数）。
///
/// `self_st` 是这条规则自己那个 status。**它和 `stacks` 不是一回事**：
/// `stacks` 是快照，`self_st` 读的是**当前值** —— 同一段 op 序列里前面的
/// `GrowSelf` 已经改过它了。凋萎存在要的正是后者（[源码] 先 `CardsLeft--`
/// 再判 `<= 0`）。
fn tcond_holds(s: &State, who: Owner, c: TCond, stacks: i32, self_st: St) -> bool {
    match c {
        TCond::OwnerHasNoBlock => owner_ref(s, who).block == 0,
        TCond::OwnerBlockAtLeast(n) => owner_ref(s, who).block >= n,
        // [源码] 灯笼是 `TurnNumber <= 1`，不是 `== 1`。照抄。
        TCond::TurnAtMost(n) => s.turn <= n,
        TCond::TurnAtLeast(n) => s.turn >= n,
        TCond::TurnMultipleOf(n) => n > 0 && s.turn % n == 0,
        TCond::TurnIs(n) => s.turn == n,
        // [源码] `TurnsSeen = (TurnsSeen + 1) % n`，触发条件是回到 0。
        // 相位来自观测，见 `St::PendulumPhase`。
        TCond::EveryNTurns { n, phase } => {
            n > 0 && (owner_ref(s, who).get(phase) + s.turn).rem_euclid(n) == 0
        }
        // [源码] `AttacksPlayedThisTurn % 3 == 0` —— 第 3/6/9… 张都算，
        // 不是只有第 3 张。这一条卡面读不出来。
        TCond::EveryNthAttackThisTurn(n) => {
            n > 0 && s.attacks_played > 0 && s.attacks_played % n == 0
        }
        // 开信刀：和精致折扇同构，只是数技能牌。
        TCond::EveryNthSkillThisTurn(n) => {
            n > 0 && s.skills_played > 0 && s.skills_played % n == 0
        }
        TCond::OwnerHas(st) => owner_ref(s, who).get(st) > 0,
        // [源码] `PlowPower`：`target.CurrentHp <= Amount`。
        // `stacks` 就是这个 power 自己的层数（仪式兽是 150）。
        TCond::OwnerHpAtMostStacks => owner_ref(s, who).hp <= stacks,
        // **读当前值，不是 `stacks` 那个快照**，理由见函数头。
        TCond::SelfStacksAtMost(n) => owner_ref(s, who).get(self_st) <= n,
        TCond::SelfStacksAtLeast(n) => owner_ref(s, who).get(self_st) >= n,
        TCond::OwnerMaxHpAtMost(n) => owner_ref(s, who).max_hp <= n,
        TCond::OwnerMaxHpAtLeast(n) => owner_ref(s, who).max_hp >= n,
        TCond::OwnerIsDead => !owner_ref(s, who).alive(),
        TCond::OwnerIsPlayer => matches!(who, Owner::Player),
        TCond::OwnerIsEnemy => matches!(who, Owner::Enemy(_)),
        // 只对玩家有意义（敌人没有手牌这个概念），而这个 status 只有
        // 尖叫酒壶发得出来、只发给玩家。
        TCond::HandEmpty => s.n_hand == 0,
        // 刚打出的那张。`last_played_card` 在 `resolve_played_card` 开头写好，
        // `CardPlayed` 在同一个函数末尾点火 —— 读到的就是触发这次的那一张。
        TCond::LastPlayedKindIs(k) => {
            (s.last_played_card as usize) < s.n_cards as usize
                && card(s.cards[s.last_played_card as usize].id).kind == k
        }
        // 刚落地的那一下。两个字段由 `hit_enemy_with` 在点 `EnemyDamaged` 之前写好。
        TCond::LastHitWasAttack => s.last_hit_attack,
        TCond::LastHitUnblocked => s.last_hit_unblocked,
    }
}

fn run_trigger(s: &mut State, def: &PowerDef, who: Owner, stacks: i32, depth: u8, ctx: usize) {
    run_ops(s, def.ops, def.st, who, stacks, depth, ctx);
}

/// `self_st` = 这条规则自己那个 status。`GrowSelf` / `ClearSelf` 要用它，
/// 而条件段（`TOp::If`）里嵌套的 op **必须拿到同一个** —— 所以显式传，
/// 不是从 `def` 里现取。
fn run_ops(
    s: &mut State,
    ops: &'static [TOp],
    self_st: St,
    who: Owner,
    stacks: i32,
    depth: u8,
    ctx: usize,
) {
    for op in ops {
        let val = |a: Amt| match a {
            Amt::Stacks => stacks,
            Amt::Fixed(n) => n,
            Amt::TurnNumber => s.turn,
            Amt::CardsPlayed => s.cards_played,
            Amt::HandCardsTimesStacks => s.n_hand as i32 * stacks,
            Amt::OwnerStacksOf(st) => owner_ref(s, who).get(st).max(0),
        };
        match *op {
            TOp::If { cond, then } => {
                if tcond_holds(s, who, cond, stacks, self_st) {
                    run_ops(s, then, self_st, who, stacks, depth, ctx);
                }
            }
            TOp::OwnerStatus { st, amt } => {
                let v = val(amt);
                owner_mut(s, who).add(st, v);
            }
            // 不走 card_block：脆弱的卡面原文是「从**卡牌**中获得的格挡值减少25%」，
            // 能力牌给的格挡不是从卡牌来的。未实测。
            TOp::OwnerBlock(amt) => {
                let v = val(amt);
                gain_block(s, who, v, depth);
            }
            // 走完整的失血路径，**会继续触发 `PlayerLoseHp`** ——
            // 绯红披风 -> 撕裂 是放血流的核心组合。深度由 MAX_HOOK_DEPTH 兜底。
            TOp::OwnerLoseHp(amt) => {
                let v = val(amt);
                match who {
                    Owner::Player => player_lose_hp_at(s, v, depth + 1),
                    Owner::Enemy(i) => s.enemies[i].hp -= v,
                }
            }
            TOp::SummonN { def, hp, count } => {
                for _ in 0..count {
                    summon_one(s, def, hp);
                }
            }
            // 库存：换上一个"库存少一个的我"。**必须用 `set` 不是 `add`** ——
            // `summon_one` 已经按被召唤方的 `start_status` 挂了满库存（2），
            // 这里要把它**覆盖**成我的层数 −1。写成加法就是每一具越来越多。
            // **只定最大生命，不回血。** 实验体被砍死那一帧只是"定好下一个形态"，
            // 血量留在 0 —— 它在我这个回合的剩余时间里仍然是死的、打不到，
            // 观测里也看不见（[实测] 2026-08-30 那一帧 `enemies: []`）。
            // 回满是它自己回合开始时的事，见 `TOp::OwnerHealToFull`。
            TOp::OwnerSetMaxHp(n) => {
                let o = owner_mut(s, who);
                o.max_hp = n;
                o.block = 0;
            }
            TOp::OwnerHealToFull => {
                let o = owner_mut(s, who);
                o.hp = o.max_hp;
            }
            // 死亡剥离：除了名单里那几个，其余 status 全清。
            //
            // [源码] `Creature.cs:670` 删掉所有
            // `ShouldPowerBeRemovedAfterOwnerDeath()` 为 true 的 power，
            // 而那个虚方法默认就是 true —— 所以**保留名单**才是忠实的写法。
            // 连我给它挂的易伤/虚弱一起清掉，这是有意的。
            TOp::ClearOwnerStatusesExcept(keep) => {
                let mut saved = [0i32; 8];
                let n = keep.len().min(saved.len());
                for k in 0..n {
                    saved[k] = owner_ref(s, who).get(keep[k]);
                }
                let o = owner_mut(s, who);
                for i in 0..crate::state::N_STATUS {
                    o.status[i] = 0;
                }
                for k in 0..n {
                    o.set(keep[k], saved[k]);
                }
            }
            TOp::OwnerToggleStatus(st) => {
                let o = owner_mut(s, who);
                let v = if o.get(st) > 0 { 0 } else { 1 };
                o.set(st, v);
            }
            TOp::SummonCarryingSelfMinusOne { def, hp } => {
                let left = owner_ref(s, who).get(self_st) - 1;
                if let Some(i) = summon_one(s, def, hp) {
                    s.enemies[i].set(self_st, left.max(0));
                }
            }
            // 走格挡 —— 和 `Op::TakeDamage` 同一条语义，别改成 `OwnerLoseHp`。
            TOp::OwnerTakeDamage(amt) => {
                let v = val(amt);
                match who {
                    Owner::Player => {
                        let through = absorb(&mut s.player, v);
                        s.hp_lost_this_turn += through;
                        after_player_hp_lost(s, through, depth + 1);
                    }
                    Owner::Enemy(i) => {
                        absorb(&mut s.enemies[i], v);
                    }
                }
            }
            // **从 0 往上回，不从负数往上回。** `absorb` 允许血量变成负数（超杀），
            // 而 [源码] 生物的血量夹在 0。以前没区别（活人的血量不会是负的），
            // 接续给尸体回血之后有了：超杀 8 点砍死的那一节，接续回来要是 25 不是 17。
            TOp::OwnerHeal(amt) => {
                let v = val(amt);
                let e = owner_mut(s, who);
                e.hp = (e.hp.max(0) + v).min(e.max_hp);
                if matches!(who, Owner::Player) { refresh_hp_threshold_strength(s); }
            }
            // 带骨肉。阈值向下取整，和 [源码] 的 `(int)(MaxHp * pct/100)` 一致。
            TOp::OwnerHealIfHpAtMost { pct, amt } => {
                let v = val(amt);
                let e = owner_mut(s, who);
                if e.hp <= e.max_hp * pct / 100 {
                    e.hp = (e.hp + v).min(e.max_hp);
                }
                if matches!(who, Owner::Player) { refresh_hp_threshold_strength(s); }
            }
            TOp::OwnerEnergy(amt) => {
                if matches!(who, Owner::Player) {
                    gain_energy(s, val(amt));
                }
            }
            TOp::OwnerDraw(amt) => {
                if matches!(who, Owner::Player) {
                    s.draw_n(val(amt));
                }
            }
            // 滚石。**这里不加力量**：它不是攻击牌，卡面也没说吃力量，
            // 两边都没有证据，就选不会高估玩家的那个（上一局正是死于
            // 模拟器太乐观）。防御方的易伤/缓慢仍然照常吃，因为那是
            // `apply_modifiers` 对所有伤害一视同仁的部分。
            TOp::DamageAllEnemies(amt) => {
                let face = val(amt);
                for e in 0..s.n_enemies as usize {
                    hit_enemy_unpowered(s, e, face, depth + 1);
                }
            }
            // 敌人持有的能力给**玩家**挂 status（活力火花 -> 污染）
            TOp::PlayerStatus { st, amt } => {
                let v = val(amt);
                s.player.add(st, v);
            }
            TOp::GrowSelf(n) => {
                owner_mut(s, who).add(self_st, n);
            }
            // [源码] `CardsLeft.BaseValue = 6m` —— 一次赋值，不是加。
            TOp::SetSelf(n) => {
                owner_mut(s, who).set(self_st, n);
            }
            // 凋萎存在：往**手牌**塞牌。走 `spawn_card` 收口 ——
            // 新生成的凋萎要继承场上同名牌的层数，那条规则只能有一份。
            // 手牌满了溢出进弃牌堆，见 `add_generated_to_hand`。
            TOp::AddCardToHand { card, count } => {
                for _ in 0..count {
                    if !add_generated_to_hand(s, card) {
                        break;
                    }
                }
            }
            // 剧痛刺击：伤口进弃牌堆。和敌人招式的 `EOp::AddCardToDiscard` 同一条路。
            TOp::AddCardToDiscard { card, count } => {
                for _ in 0..count {
                    let Some(ix) = spawn_card(s, card) else { break };
                    s.to_discard(ix);
                }
            }
            // 人体蜂房：晕眩进抽牌堆随机位置。和敌人招式的 `EOp::AddCardToDraw` 同一条路。
            TOp::AddCardToDraw { card, amt } => {
                let n = val(amt);
                for _ in 0..n {
                    let Some(ix) = spawn_card(s, card) else { break };
                    s.to_draw_random(ix);
                }
            }
            // 纸伤难愈。和卡牌的 `Op::LoseMaxHp` 共用一份实现。
            TOp::PlayerLoseMaxHp(amt) => {
                let v = val(amt);
                player_lose_max_hp(s, v, depth + 1);
            }
            // 流电。`Unpowered`、走格挡、不是攻击。
            TOp::DamagePlayer(amt) => {
                let face = val(amt);
                damage_player_unpowered(s, face, depth + 1);
            }
            // 恶咒 / 抑制的施咒者死了：把我身上那个 debuff 摘掉。
            TOp::PlayerClearStatus(st) => s.player.set(st, 0),
            // 抑制解除：`F_DAMPENED` 的牌升回去。[源码] `DampenPower.AfterRemoved` 按记下的
            // 层数逐级 `CardCmd.Upgrade`，而那个函数对**已经是升级态**的牌（抑制期间又被
            // 武装升了一次的）直接跳过 —— 内核的升级只有一级，置位就是同一件事。
            TOp::RestoreDampenedCards => {
                for i in 0..s.n_cards as usize {
                    let c = &mut s.cards[i];
                    if c.flags & crate::state::F_DAMPENED != 0 {
                        c.flags = (c.flags & !crate::state::F_DAMPENED) | F_UPGRADED;
                    }
                }
            }
            // 一次性 status 打完就清零（蜷身）。**必须放在最后一条 op**，
            // 否则同一条规则里后面的 op 拿到的层数就是 0 了。
            TOp::ClearSelf => {
                owner_mut(s, who).set(self_st, 0);
            }
            // 沙坑归零：**强制死亡**，不给瓶中精灵机会（[源码] `force: true`）。
            // 血量一起清零只是为了让所有"看血量"的下游（叶评估、报告）读到
            // 一致的局面；真正判死的是 `player_dead`，而 `check_over` 开头
            // 那道守卫保证它不会被后来的 `try_fairy` 翻案。
            TOp::KillPlayer => {
                s.player.hp = 0;
                s.player_dead = true;
                s.combat_over = true;
            }
            TOp::OwnerClearStatus(st) => {
                owner_mut(s, who).set(st, 0);
            }
            // **置**，不是加。理由见 `TOp::OwnerSetStatus` 的文档。
            TOp::OwnerSetStatus { st, amt } => {
                let v = val(amt);
                owner_mut(s, who).set(st, v);
            }
            // 盾墙。敌人种类写死在源码里（只给高塔炮手），所以带 def 参数。
            // 格挡直接加，不过乘区（[源码] `ValueProp.Unpowered`）。
            TOp::BlockEnemiesOfDef { def, amt } => {
                let v = val(amt);
                if v > 0 {
                    for e in 0..s.n_enemies as usize {
                        if s.enemies[e].alive() && s.enemy_def[e] == def {
                            s.enemies[e].block += v;
                        }
                    }
                }
            }
            // 强制改招。玩家持有时没有意义，静默跳过（不变量 5：不 panic）。
            TOp::OwnerForceMove(ix) => {
                if let Owner::Enemy(e) = who {
                    s.enemy_move[e] = ix;
                    // 记一笔：注入路径要靠它知道"观测到的那个意图已经过期了"
                    s.enemies[e].set(St::MoveForcedThisTurn, 1);
                    // 历史也要跟着断：被强制打断的那一手没有真的打出去，
                    // 让它继续参与 `CannotRepeat` 的判定会歪。
                    s.enemy_hist[e] = [u8::MAX; crate::state::ENEMY_HIST];
                }
            }
            // 火焰屏障的反伤。和滚石一样不加力量（不是攻击牌），
            // **也不吃防御方的易伤/缓慢** —— 能力牌打出的伤害是
            // [源码] `ValueProp.Unpowered`，走 `hit_enemy_unpowered`。
            TOp::DamageContextEnemy(amt) => {
                if ctx < s.n_enemies as usize {
                    let face = val(amt);
                    hit_enemy_unpowered(s, ctx, face, depth + 1);
                }
            }
            // 荆棘。两侧对称，见 `TOp::DamageAttacker` 的文档。
            TOp::DamageAttacker(amt) => {
                let face = val(amt);
                match who {
                    // 我身上的荆棘 ⇒ 打回给触发这一击的那只敌人
                    Owner::Player => {
                        if ctx < s.n_enemies as usize {
                            hit_enemy_unpowered(s, ctx, face, depth + 1);
                        }
                    }
                    // 敌人身上的荆棘 ⇒ 打回给我。**走格挡但不走
                    // `take_attack_hit`**：反弹伤害是 `Unpowered`，
                    // 既不该再触发一次 `Attacked`，也不该唤醒孤注一掷。
                    // 和流电共用 `damage_player_unpowered`（2026-09-13 起它也过无实体，
                    // 照 `TOp::DamageAttacker` 自己文档里写的那条）。
                    Owner::Enemy(_) => damage_player_unpowered(s, face, depth + 1),
                }
            }
            // 惊逃：回合结束随机打出手牌里 N 张**可打出的攻击牌**。
            // [源码] `StampedePower.BeforeTurnEnd`：每次循环都**重新**从手牌里
            // 挑（因为上一张打出后手牌变了），过滤 `Unplayable`。
            TOp::AutoPlayRandomAttacksFromHand(amt) => {
                if matches!(who, Owner::Player) {
                    for _ in 0..val(amt) {
                        let mut cand = [0usize; MAX_CARDS];
                        let mut n = 0usize;
                        for i in 0..s.n_hand as usize {
                            let id = s.cards[s.hand[i] as usize].id;
                            if matches!(card(id).kind, Kind::Attack) && playable(id) {
                                cand[n] = i;
                                n += 1;
                            }
                        }
                        if n == 0 {
                            break;
                        }
                        let k = crate::state::next_below(&mut s.rng.enemy, n);
                        let c = s.take_from_hand(cand[k]);
                        auto_play_card(s, c, false, depth);
                    }
                }
            }
            // 杂耍：本回合第 `nth` 张攻击牌打出时，复制它 `amt` 份进手牌。
            // [源码] `JugglingPower.AfterCardPlayed` 里是 `== 3` 的**相等**判断
            // （不是 `% 3`），而且计数每回合清零 —— 所以一回合只触发一次。
            TOp::CloneLastAttackToHandAt { nth, amt } => {
                if matches!(who, Owner::Player) && s.attacks_played == nth {
                    let src_ix = s.last_played_card as usize;
                    let inst = s.cards[src_ix];
                    for _ in 0..val(amt) {
                        if (s.n_cards as usize) >= MAX_CARDS
                            || (s.n_hand as usize) >= MAX_CARDS
                        {
                            break;
                        }
                        let ix = s.n_cards;
                        s.cards[ix as usize] = inst;
                        s.n_cards += 1;
                        s.hand[s.n_hand as usize] = ix;
                        s.n_hand += 1;
                    }
                }
            }
            // 好勇斗狠：回合开始从弃牌堆随机取 N 张攻击牌进手牌**并升级**。
            TOp::FetchRandomAttacksFromDiscardUpgraded(amt) => {
                if matches!(who, Owner::Player) {
                    for _ in 0..val(amt) {
                        let mut cand = [0usize; MAX_CARDS];
                        let mut n = 0usize;
                        for i in 0..s.n_disc as usize {
                            let id = s.cards[s.disc[i] as usize].id;
                            if matches!(card(id).kind, Kind::Attack) {
                                cand[n] = i;
                                n += 1;
                            }
                        }
                        if n == 0 || (s.n_hand as usize) >= MAX_HAND {
                            break;
                        }
                        let k = crate::state::next_below(&mut s.rng.enemy, n);
                        let c = s.take_from_disc(cand[k]);
                        s.cards[c as usize].flags |= F_UPGRADED;
                        s.to_hand(c);
                    }
                }
            }
            // 势不可当。**不加力量**：[源码] 里走的是 `ValueProp.Unpowered`。
            // 这和滚石/火焰屏障当初"没证据就选不高估玩家"的推断不同，
            // 这一条是源码明写的。
            // 给全体活着的敌人挂 status。**死了的不算** —— [源码] 是
            // `combatState.HittableEnemies`，而不是所有槽位。
            // 升级手上现在这几张（风箱 / 骨茶）。已经升级的不动。
            TOp::UpgradeHand => {
                if matches!(who, Owner::Player) {
                    for i in 0..s.n_hand as usize {
                        s.cards[s.hand[i] as usize].flags |= F_UPGRADED;
                    }
                }
            }
            // 从抽牌堆随机挑 n 张**还没升级的**升级（碎石者）。
            // [源码] 的筛子是 `IsUpgradable` —— 内核这边同义于"没带 `F_UPGRADED`"。
            TOp::UpgradeRandomInDraw(n) => {
                if matches!(who, Owner::Player) {
                    for _ in 0..n {
                        let mut cand = [0usize; MAX_CARDS];
                        let mut n = 0usize;
                        for i in 0..s.n_draw as usize {
                            if s.cards[s.draw[i] as usize].flags & F_UPGRADED == 0 {
                                cand[n] = i;
                                n += 1;
                            }
                        }
                        if n == 0 {
                            break;
                        }
                        let k = crate::state::next_below(&mut s.rng.gen, n);
                        s.cards[s.draw[cand[k]] as usize].flags |= F_UPGRADED;
                    }
                }
            }
            // 宝石面具：抽牌堆里随机一张**能力牌**进手牌，本回合免费。
            TOp::MoveRandomPowerFromDrawToHandFree => {
                if matches!(who, Owner::Player) && (s.n_hand as usize) < MAX_HAND {
                    let mut cand = [0usize; MAX_CARDS];
                    let mut n = 0usize;
                    for i in 0..s.n_draw as usize {
                        let id = s.cards[s.draw[i] as usize].id;
                        if matches!(card(id).kind, Kind::Power) {
                            cand[n] = i;
                            n += 1;
                        }
                    }
                    if n > 0 {
                        let k = crate::state::next_below(&mut s.rng.gen, n);
                        let pos = cand[k];
                        let cix = s.draw[pos];
                        // 从抽牌堆中间抽走一张：**已知前缀要跟着截**，
                        // 收口点是 `State::take_from_draw`（和头槌那条路同一个）。
                        s.take_from_draw(pos);
                        s.cards[cix as usize].flags |= F_FREE_THIS_TURN;
                        s.to_hand(cix);
                    }
                }
            }
            TOp::AllEnemiesStatus { st, amt } => {
                let v = val(amt);
                // **过人工制品**，判据和 `apply_status`（卡牌那条路）逐字相同。
                // [源码] `ArtifactPower.TryModifyPowerAmountReceived` 挂在
                // **接收方**身上、不问来源 —— 触发器发出去的 debuff 一样被吃掉。
                // 漏了这一条在对拍路径上看不见（status 每帧从观测重灌），
                // [实测] 2026-09-09 `bin/synth_audit`：`act3_f48_boss_aeonglass_2026-09-06`
                // 第 0 帧永世沙漏是 `人工制品 2 · 虚弱 0`（红面具那 1 层被吃掉了），
                // 内核给的是 `人工制品 3 · 虚弱 1`。
                let debuff = crate::damage::is_debuff_amount(st, v);
                for e in 0..s.n_enemies as usize {
                    if s.enemies[e].alive() && !artifact_absorbs(&mut s.enemies[e], debuff) {
                        s.enemies[e].add(st, v);
                    }
                }
            }
            TOp::DamageRandomEnemy(amt) => {
                let face = val(amt);
                if let Some(e) = random_living_enemy(s) {
                    hit_enemy_unpowered(s, e, face, depth + 1);
                }
            }
        }
    }
}

/// 回合结束时手牌的两段结算，**顺序照 [源码] `CombatManager.DoTurnEnd`**：
///
/// 1. 先把**虚无**的牌消耗掉 —— 晕眩/笨拙自带的，以及身上有**恶咒**时手里所有别的牌
/// 2. 再让"留在手上就发作"的牌发作（灼伤/腐朽/羞耻/感染/瓦解/凋萎/呼唤/毒素）
///
/// 分组按**进来那一刻的快照**做（源码是先遍历一遍分成两个列表）：
/// **带回合末发作效果的牌不算虚无**（源码是 `if HasTurnEndInHandEffect … else if Ethereal`），
/// 所以恶咒不会把灼伤消耗掉，灼伤照样烫人。消耗时黑暗之拥抽上来的牌不在快照里、不发作。
///
/// **顺序有后果**：虚无消耗触发的无惧疼痛格挡，要赶在灼伤那几点之前落地。
/// 2026-09-14 之前这里是"先发作、后消耗"—— 晕眩 + 灼伤 + 无惧疼痛时少挡一次。
///
/// 两段都在弃手牌 / 保留**之前**（源码第二阶段才 `FlushPlayerHand`），
/// 所以带保留（`F_RETAIN`）或均衡留下的牌照样被恶咒消耗。
fn resolve_hand_end(s: &mut State) {
    let hexed = s.player.get(St::Hex) > 0;
    let mut to_void = [0u8; MAX_CARDS];
    let mut n_void = 0usize;
    let mut to_fire = [0u8; MAX_CARDS];
    let mut n_fire = 0usize;

    for i in 0..s.n_hand as usize {
        let cix = s.hand[i];
        let id = s.cards[cix as usize].id;
        match HAND_END.iter().find(|d| d.card == id) {
            Some(def) if !def.ops.is_empty() => {
                to_fire[n_fire] = cix;
                n_fire += 1;
            }
            Some(def) if def.void || hexed => {
                to_void[n_void] = cix;
                n_void += 1;
            }
            None if hexed => {
                to_void[n_void] = cix;
                n_void += 1;
            }
            _ => {}
        }
    }

    for &cix in &to_void[..n_void] {
        // 手牌里找到它现在的位置再拿走 —— 前一张的消耗触发可能已经动过手牌
        if let Some(i) = (0..s.n_hand as usize).find(|&i| s.hand[i] == cix) {
            let c = s.take_from_hand(i);
            exhaust_card(s, c);
        }
    }

    for &cix in &to_fire[..n_fire] {
        if !(0..s.n_hand as usize).any(|i| s.hand[i] == cix) {
            continue;
        }
        let inst = s.cards[cix as usize];
        let Some(def) = HAND_END.iter().find(|d| d.card == inst.id) else { continue };
        resolve_ops(s, def.ops, inst, cix as usize, 0, Source::Card);
    }
}

/// **凭空造一张牌**。所有"往牌堆里塞一张本来不存在的牌"的路径都走这里
/// （敌人塞弃牌堆/抽牌堆、凋萎存在塞手牌），和 `exhaust_card` 是同一个套路：
/// 收口点只有一个，规则就不可能长歪。
///
/// 它管着一条**不能各写一份**的规则：**新生成的牌继承场上同名牌的层数**。
/// [源码] `Aeonglass.AfterCardGeneratedForCombat` 对每一张新生成的凋萎调
/// `MatchWitherToUpgradeCount`，把它拉到 Boss 当前的 `WitherUpgradeCount`；
/// 而 `IncreasingIntensityMove` 又会把场上每一张凋萎一起升级。
/// 两条合起来的结果是：**场上所有凋萎的层数恒等**。
///
/// 所以内核不另存一个 `WitherUpgradeCount` 计数器，直接从场上同名牌抄 ——
/// 两者恒等，而这样**不用携带一个观测里没有的量**（`sync` 每帧从观测重建，
/// 携带不过来的计数器是这个仓库踩过的坑）。场上一张都没有时抄到 0，
/// 那正是第一张凋萎的正确层数。
///
/// 返回 `None` 表示牌位满了（`MAX_CARDS`）—— 调用方该停下，不是静默继续。
fn spawn_card(s: &mut State, id: u16) -> Option<u8> {
    if (s.n_cards as usize) >= MAX_CARDS {
        return None;
    }
    let bonus = (0..s.n_cards as usize)
        .filter(|&i| s.cards[i].id == id)
        .map(|i| s.cards[i].bonus)
        .max()
        .unwrap_or(0);
    let ix = s.n_cards;
    s.cards[ix as usize] = CardInst { id, flags: 0, bonus, cost_delta: 0, ench: 0, ench_amt: 0 };
    s.n_cards += 1;
    Some(ix)
}

/// **凭空造一张牌塞进手牌，手牌满了进弃牌堆。** 返回 `false` = 牌位满了，调用方该停。
///
/// [源码] `CardPileCmd.Add`：`isFullHandAdd = targetPile.Type == Hand && Count >= MaxCardsInHand`
/// 时 `targetPile = Discard` —— **溢出不是丢掉**。机甲骑士一次塞 4 张灼伤，
/// 手牌 8 张时进手 2 张、进弃牌堆 2 张，洗牌之后照样烫人。
///
/// 两个调用者：敌人招式 `EOp::AddCardToHand`（火焰喷射）和触发器
/// `TOp::AddCardToHand`（凋萎存在）。后者 2026-09-13 之前是"满了就不造"，
/// 和源码不一致 —— 它走的也是 `AddToCombatAndPreview(.., PileType.Hand, ..)`。
fn add_generated_to_hand(s: &mut State, id: u16) -> bool {
    let Some(ix) = spawn_card(s, id) else { return false };
    if (s.n_hand as usize) < crate::state::MAX_HAND {
        s.to_hand(ix);
    } else {
        s.to_discard(ix);
    }
    true
}

/// 玩家失去最大生命。
///
/// [源码] `CreatureCmd.LoseMaxHp`：先算新上限，**当前血高于新上限时把差额当伤害扣掉**
/// （`Unblockable`），再把上限设过去。走伤害那条路是关键 —— 百年积木就是这么被
/// 至亮之焰触发的（2026-08-27 第 2 幕第 19 层实测，见 verification-log）。
///
/// 卡牌（`Op::LoseMaxHp`）和敌人的能力（`TOp::PlayerLoseMaxHp`，纸伤难愈）共用这一份。
fn player_lose_max_hp(s: &mut State, n: i32, depth: u8) {
    if n <= 0 {
        return;
    }
    let new_max = (s.player.max_hp - n).max(1);
    if s.player.hp > new_max {
        let d = s.player.hp - new_max;
        s.player.hp = new_max;
        s.hp_lost_this_turn += d;
        after_player_hp_lost(s, d, depth);
    }
    s.player.max_hp = new_max;
    refresh_hp_threshold_strength(s);
}

/// 敌人那一侧的**非攻击**伤害打到我身上（敌人的荆棘反弹 / 流电）。
///
/// `Unpowered`：不吃力量、不过攻防乘区，**只过难以杀灭和无实体**，然后走格挡。
/// **不走 `take_attack_hit`** —— 那条路会再触发一次 `Attacked`（火焰屏障）、
/// 唤醒孤注一掷、点纸伤难愈，而这类伤害不是攻击，三样都不该发作。
/// 掉血照样记进 `after_player_hp_lost`（百年积木看的是"真掉了血"）。
fn damage_player_unpowered(s: &mut State, face: i32, depth: u8) {
    let d = crate::damage::apply_modifiers_unpowered(face, &s.player);
    let through = absorb(&mut s.player, d);
    s.hp_lost_this_turn += through;
    after_player_hp_lost(s, through, depth);
}

/// 给玩家加能量。**所有回合内的加能量路径都必须走这里** ——
/// [源码] 侧那几笔全都过 `PlayerCmd.GainEnergy`，而 `NoEnergyGainPower`
/// 挂在 `ModifyEnergyGain` 上，也就是挂在那个收口点上。
///
/// **回合开始的能量回满不走这里**：那是 `s.energy = s.base_energy` 一条赋值，
/// [源码] 侧也不是 `GainEnergy`。这个区别是有意义的 ——
/// 跃跃欲试的禁令只管本回合，下回合照常回满。
pub(crate) fn gain_energy(s: &mut State, n: i32) {
    if s.player.get(St::NoEnergyGain) > 0 {
        return;
    }
    s.energy += n;
}

/// 消耗一张牌。**所有**进消耗堆的路径都必须走这里，否则
/// 无惧疼痛/黑暗之拥 会漏触发。
pub(crate) fn exhaust_card(s: &mut State, c: u8) {
    s.to_exhaust(c);
    // 战鼓（[源码] `DrumOfBattle.AfterCardExhausted`）：自身被消耗时获得 2 能量（升级 3 能量）
    if s.cards[c as usize].id == crate::content::card::DRUM_OF_BATTLE {
        gain_energy(s, if s.cards[c as usize].upgraded() { 3 } else { 2 });
    }
    fire(s, Hook::CardExhausted, 0);
}

fn player_lose_hp_at(s: &mut State, n: i32, depth: u8) {
    s.player.hp -= n;
    refresh_hp_threshold_strength(s);
    s.hp_lost_this_turn += n;
    fire(s, Hook::PlayerLoseHp, depth);
    after_player_hp_lost(s, n, depth);
}

/// [源码] RedSkull compares decimal HP against 50% of maximum HP.
pub fn hp_threshold_active(player: &Entity) -> bool {
    player.hp * 2 <= player.max_hp
}

/// Called at HP changes, before later damage/status hooks. The latch prevents
/// repeated awards; removing the bonus also happens when healing above half.
fn refresh_hp_threshold_strength(s: &mut State) {
    let amount = s.player.get(St::RedSkull);
    if amount <= 0 { return; }
    let active = hp_threshold_active(&s.player);
    if active == (s.player.get(St::RedSkullActive) > 0) { return; }
    s.player.set(St::RedSkullActive, i32::from(active));
    apply_status(s, Tgt::Me, St::Strength, if active { amount } else { -amount }, 0, Source::Potion);
}

/// 玩家**真的掉了血**之后的触发点（今天只有百年积木）。
///
/// **三条路各调一次**，因为内核里玩家掉血本来就有三个入口：
///   `take_attack_hit`（挨敌人打，过格挡）· `player_lose_hp_at`（卡牌"失去生命"）
///   · `Op::TakeDamage` / `TOp::OwnerTakeDamage`（灼伤那类，过格挡）
///
/// 为什么"卡牌失去生命"也算：[源码] `Bloodletting.OnPlay` 走的是
/// `CreatureCmd.Damage(..., Unblockable | Unpowered | Move, this)` ——
/// **它是伤害，只是不可格挡**，所以 `AfterDamageReceived` 照样触发。
/// 这条不是推的：2026-08-27 第 1 幕 Boss 帧2 打出放血，游戏手牌 6 / 内核 3，
/// 差的正好是百年积木抽的 3 张。**当天那版内核只在挨打那条路上点火，是错的。**
#[inline]
fn after_player_hp_lost(s: &mut State, through: i32, depth: u8) {
    if through > 0 {
        refresh_hp_threshold_strength(s);
        // **数的是次数，不是血量**（扯碎的段数读它）。收在这里正是因为上面
        // 那段注释列的三条路都汇到这个函数 —— [源码] 那边的判据
        // （`DamageReceivedEntry` 且 `UnblockedDamage > 0`）同样不分来源。
        s.hp_loss_hits = s.hp_loss_hits.saturating_add(1);
        fire(s, Hook::PlayerDamaged, depth);
    }
}

fn player_lose_hp(s: &mut State, n: i32) {
    player_lose_hp_at(s, n, 0);
}

/// 一条攻击命令结算完，把活力整个清掉（[源码] `VigorPower.AfterAttack` 里
/// `ModifyAmount(-amountWhenAttackStarted)`，**不是每段扣一层**）。
///
/// 本地那个 `vigor` 变量同时也要归零，否则同一张牌里后面的伤害 op
/// 还会接着用旧值 —— 而源码里第二条命令是明确吃不到的。
#[inline]
fn spend_vigor(s: &mut State, vigor: &mut i32) {
    if *vigor > 0 {
        s.player.set(St::Vigor, 0);
        *vigor = 0;
    }
}

/// `cix` 是这张牌在 `s.cards` 里的下标 —— 暴走那种「把这张牌本场的伤害改大」
/// 要写回卡实例（`CardInst.bonus`），光有 `inst` 的副本改不动。
fn resolve_ops(s: &mut State, ops: &[Op], inst: CardInst, cix: usize, target: usize, src: Source) {
    // 基础值上的乘区：老的腐化 flag 和附魔的 `EnchantDamageMultiplicative`
    // （腐化 ×1.5 / 直觉 ×2）是同一个口子，在这里合成一对整数交给
    // `card_face_damage`。两个都带时连乘 —— 游戏侧那些 `Modify*Multiplicative`
    // 本来就是同一个循环里连乘的。
    let ench_mul = crate::content::ench_damage_mul(&inst);
    let base_mul = if inst.corrupt() {
        (ench_mul.0 * 3, ench_mul.1 * 2)
    } else {
        ench_mul
    };
    // 伤害加值：暴走/痛殴攒在实例上的那一份（`CardInst::bonus`）
    // **加上**附魔给的（锋利 = Amount）。两者同一档，都在力量之前。
    let bonus = inst.bonus as i32 + crate::content::ench_damage_add(&inst)
        + tagged_attack_bonus(&s.player, crate::content::is_strike(inst.id));
    // 面板伤害：**药水不吃力量/腐化/锋利**，卡牌吃。
    // **防御方那一侧也分来源**（[源码] `IsPoweredAttack`）：药水是
    // `ValueProp.Unpowered`，不吃易伤/虚弱/缓慢/缩小，只吃难以杀灭和无实体。
    // 这里原来写着"两边一视同仁 …… 推断，未实测"—— 2026-08-22 第2幕第27层
    // 那一帧把它证伪了（火焰药水打带易伤 2 的目标：游戏 20，内核 30）。
    let powered = matches!(src, Source::Card);
    // 活力（[源码] `VigorPower`）。三条规矩全在 `St::Vigor` 的注释里，这里只说
    // 内核怎么落地它们：
    //
    // * **只有有源攻击吃得到** ⇒ `powered` 为假时直接取 0，药水那条路不碰它。
    // * **一条 `AttackCommand` 的每一段都吃满** ⇒ `face` 在 `for hits` 循环
    //   **外面**算一次，循环里每段都用同一个含活力的面板值。这不是省事，
    //   是源码里钩子就在多段循环外面（`AttackCommand.cs` 536/538/656）。
    // * **打完整条命令一次性清零** ⇒ 每个伤害 op 结算完调 `spend_vigor`。
    //
    // 「一条 `AttackCommand` ≈ 一个伤害 op」这个对应关系：现有内容表里每张
    // 攻击牌的 `OnPlay` 都只发一条 `DamageCmd.Attack(...)`（多段走
    // `WithHitCount`，仍是同一条命令），所以这里是**精确**的，不是近似。
    // 将来若进来一张一次发两条攻击命令的牌，第二条本来就该吃不到活力 ——
    // 现在的写法（第一个 op 之后清零）自动就是对的。
    let mut vigor = if powered { s.player.get(St::Vigor) } else { 0 };
    let face_of = |s: &State, b: i32, vig: i32| match src {
        Source::Card => card_face_damage(b, base_mul, bonus, s.player.get(St::Strength), vig),
        Source::Potion => b,
    };
    for op in ops {
        match *op {
            Op::Damage { base, hits, scale } => {
                let b = scaled_base(s, base, scale, target);
                let face = face_of(s, b, vigor);
                for _ in 0..hits {
                    hit_enemy_with(s, target, face, powered, 0);
                }
                spend_vigor(s, &mut vigor);
            }
            // 扯碎。**段数在第一段落地之前就定死**（[源码] `WithHitCount(...)`
            // 是构造 `AttackCommand` 时求值的），所以循环里不能重读 ——
            // 敌人带荆棘时每一段都会反弹伤害到我身上、把 `hp_loss_hits` 顶上去，
            // 写在循环里就是一条会自己越滚越长的攻击。
            Op::DamagePerHpLossHit { base } => {
                let hits = 1 + s.hp_loss_hits as i32;
                let face = face_of(s, base, vigor);
                for _ in 0..hits {
                    hit_enemy_with(s, target, face, powered, 0);
                }
                spend_vigor(s, &mut vigor);
            }
            // 击晕。**对死人是空操作**（[源码] `if (... && !IsDead)`）——
            // 吹哨先打 33 点，斩杀的那一下这个击晕就白给了。
            Op::StunEnemy => {
                if target < s.n_enemies as usize && s.enemies[target].alive() {
                    s.enemies[target].set(St::Stunned, 1);
                }
            }
            Op::DamageIfVuln { base, scale } => {
                if s.enemies[target].get(St::Vulnerable) > 0 {
                    let b = scaled_base(s, base, scale, target);
                    let face = face_of(s, b, vigor);
                    hit_enemy_with(s, target, face, powered, 0);
                    spend_vigor(s, &mut vigor);
                }
            }
            // AOE 是**一条** `AttackCommand`（`_singleTarget` 为 null），
            // 所以活力对每个目标、每一段都生效，全打完才清零 —— 清零在循环外面。
            Op::DamageAll { base, hits, scale } => {
                for e in 0..s.n_enemies as usize {
                    if !s.enemies[e].alive() {
                        continue;
                    }
                    let b = scaled_base(s, base, scale, e);
                    let face = face_of(s, b, vigor);
                    for _ in 0..hits {
                        hit_enemy_with(s, e, face, powered, 0);
                    }
                }
                spend_vigor(s, &mut vigor);
            }
            // 药水给的格挡**不过 `card_block`**：脆弱的卡面原文是「从**卡牌**中
            // 获得的格挡值减少25%」，药水不是牌。
            // **实测**（`act1_f14` 帧4）：带脆弱 2 时格挡药水仍然给满 12，
            // 而同一局面下防御只给 3（5×3/4）。敏捷那一侧未实测，一并按不吃处理。
            Op::Block { base } => {
                let b = match src {
                    // **附魔的格挡加值加在卡面基础值上**，再过敏捷/脆弱/臂甲
                    // （[源码] `EnchantBlockAdditive(originalBlock)`，作用在原始格挡上）。
                    // 药水没有附魔，所以只有卡牌这一支加。
                    Source::Card => {
                        card_block(base + crate::content::ench_block_add(&inst), &mut s.player)
                    }
                    Source::Potion => base,
                };
                gain_block(s, Owner::Player, b, 0);
            }
            Op::Status { tgt, st, amt } => apply_status(s, tgt, st, amt, target, src),
            Op::GainEnergy(n) => gain_energy(s, n),
            // 飞剑回旋镖：每一段各自随机挑一个活着的敌人。
            // 基础值只算**一次**（力量/腐化/暴走都在 `face_of` 里），
            // 每一段的防御方乘区在 `hit_enemy_with` 里各自过 —— 这和多段攻击
            // 「缓慢按牌不按命中」那条实测结论是一致的。
            Op::DamageRandom { base, hits, scale } => {
                let b = scaled_base(s, base, scale, target);
                let face = face_of(s, b, vigor);
                for _ in 0..hits {
                    match random_living_enemy(s) {
                        Some(e) => hit_enemy_with(s, e, face, powered, 0),
                        None => break,
                    }
                }
                spend_vigor(s, &mut vigor);
            }
            // 狂宴：**永久**加最大生命，并同时回同样多的血。
            // [源码] `CreatureCmd.GainMaxHp` 两件事一起做。
            Op::GainMaxHp(n) => {
                s.player.max_hp += n;
                s.player.hp += n;
                refresh_hp_threshold_strength(s);
            }
            // 规则在 `player_lose_max_hp`（纸伤难愈那条触发器也走它）。
            Op::LoseMaxHp(n) => player_lose_max_hp(s, n, 0),
            // 跃跃欲试：手牌里每有一张攻击牌给 `per` 点能量。
            // 打出时这张牌自己已经离手（和急躁那条 `NoAttackInHand` 同样的前提）。
            Op::EnergyPerAttackInHand { per } => {
                let n = (0..s.n_hand as usize)
                    .filter(|&i| {
                        matches!(card(s.cards[s.hand[i] as usize].id).kind, Kind::Attack)
                    })
                    .count() as i32;
                gain_energy(s, per * n);
            }
            Op::Draw(n) => s.draw_n(n),
            // 手牌并进抽牌堆、连弃牌堆一起洗。抽牌是**下一条 `Op::Draw`** 干的事，
            // 不在这里顺手抽 —— 一条 `Op` 只做一件事，瓶装潜能那两条就是两条。
            Op::ShuffleAllIntoDraw => s.shuffle_all_into_draw(),
            Op::LoseHp(n) => player_lose_hp(s, n),
            // **先给手牌拍个快照，再逐张消耗**。这一步不能写成
            // `while s.n_hand > 0`：黑暗之拥会在消耗触发里补抽，写成循环的话
            // 新抽的牌会被同一张恶魔之焰接着烧掉 —— 实测不是这样，
            // 新抽的牌留在手里。伤害也按快照的张数算。
            Op::ExhaustHandDamage { per } => {
                let count = s.n_hand as usize;
                let mut snapshot = [0u8; MAX_CARDS];
                snapshot[..count].copy_from_slice(&s.hand[..count]);
                s.n_hand = 0;
                for &c in &snapshot[..count] {
                    exhaust_card(s, c);
                }
                // **每消耗一张牌各算一次命中**，不是把 `per * count` 当成一下。
                // [源码] `FiendFire.OnPlay`：`DamageCmd.Attack(7).WithHitCount(cardCount)`。
                //
                // 内核原来写的是一次 `per * count`，**只有在有乘区或伤害上限时才分得开**
                // —— 2026-08-22 第2幕 Boss 那一帧判了它：消耗 3 张、目标带易伤，
                // 游戏 `floor(7×1.5)×3 = 30`，内核 `floor(21×1.5) = 31`。差 1。
                // 遇到难以杀灭差得更多（逐段封顶 vs 一次封顶）。
                // 恶魔之焰同样是**一条** `WithHitCount(cardCount)` 的攻击命令，
                // 所以活力对每一段都生效，全部打完才清零。
                let face =
                    card_face_damage(per, base_mul, bonus, s.player.get(St::Strength), vigor);
                for _ in 0..count {
                    hit_enemy_with(s, target, face, powered, 0);
                }
                spend_vigor(s, &mut vigor);
            }
            // 武装+：升级手上所有能升级的。就地改 `CardInst` 的 flag。
            // [源码] `Armaments.OnPlay`：`.Where(c => c.IsUpgradable)` ——
            // **只升可升级的牌**。诅咒/状态/任务牌不在其中。
            Op::UpgradeAllInHand => {
                for i in 0..s.n_hand as usize {
                    let c = s.hand[i] as usize;
                    if crate::content::upgradable(s.cards[c].id) {
                        s.cards[c].flags |= F_UPGRADED;
                    }
                }
            }
            // 头槌：弃牌堆 -> 抽牌堆顶。空弃牌堆时什么都不做（不是错误）。
            //
            // **这一刻这张牌还没进弃牌堆**（`resolve_played_card` 里
            // `s.to_discard(cix)` 在 `resolve_ops` 之后），所以此处的 `n_disc`
            // 正好等于游戏给出的真实候选数 —— 下面两条都建在这个事实上。
            Op::DiscardToTopOfDraw(n) => {
                if s.n_disc > 0 {
                    // 只有一张候选时游戏**不弹选牌界面**，直接放上去。
                    // [实测 1 帧] 2026-08-30 第 3 幕第 45 层回合 1：弃牌堆只有
                    // 一张防御，`play 头槌+` 之后没有 `card_select`，
                    // 抽牌堆 24 -> 25、弃牌堆仍是 1。
                    //
                    // 这条**结果等价**（一个候选的选择本来就是强制的），
                    // 建它只是为了让 `solve --live` 不再给出一个游戏里
                    // 根本不存在的选牌步骤。
                    if s.n_disc == 1 && (s.n_draw as usize) < MAX_CARDS {
                        let c = s.take_from_disc(0);
                        s.to_draw_top(c);
                    } else {
                        // 两张以上才交给玩家选，并记下要排除的那张（它自己）。
                        // 药水之类没有"自己"可排除，用 `u8::MAX`。
                        let exclude = if src == Source::Card { cix as u8 } else { u8::MAX };
                        s.pending = Pending::DiscardToDrawTop { remaining: n as u8, exclude };
                    }
                }
            }
            // 愤怒：复制这一张实例进弃牌堆。
            // **复制的是实例不是牌名** —— 升级过的愤怒复制出来也是升级的，
            // 这一点卡面文本读不出来，是 [源码] `CreateClone()` 定的。
            Op::AddCopyOfThisToDiscard => {
                if (s.n_cards as usize) < MAX_CARDS && (s.n_disc as usize) < MAX_CARDS {
                    let inst = s.cards[cix];
                    let ix = s.n_cards;
                    s.cards[ix as usize] = inst;
                    s.n_cards += 1;
                    s.to_discard(ix);
                }
            }
            // 重振精神：消耗手里所有**非攻击**牌，每张给 `per` 点格挡。
            //
            // 快照写法和恶魔之焰同一个理由：消耗会触发黑暗之拥补抽，
            // 边遍历边改手牌必然下标漂移。这里还多一层 —— 新抽进来的牌
            // **不该**被同一张重振精神接着消耗。
            //
            // 每一张各自调一次 `gain_block`，所以势不可当会触发 N 次；
            // 格挡走 `card_block`，所以臂甲只翻倍第一次。两条都和 [源码]
            // 里「循环体内逐次 GainBlock」的写法一致。
            // 添柴「消耗所有手牌。每消耗一张牌，将1张随机牌加入你的手牌。」
            //
            // **顺序是先消耗、再生成**（玩家判定，[源码] 的顺序也一致）：
            // 张数先快照，全部消耗完才开始生成，所以新生成的牌不会被同一张
            // 添柴接着吃掉。和恶魔之焰「不烧新抽的牌」是同一类判定 ——
            // 写成边消耗边生成会无限循环。
            //
            // 消耗走 `exhaust_card` 收口，所以无惧疼痛/黑暗之拥照常触发。
            // **黑暗之拥补抽进来的牌也不该被吃** —— 快照写法天然保证了这一点。
            Op::ExhaustHandGenerate { upgraded } => {
                let count = s.n_hand as usize;
                let mut snapshot = [0u8; MAX_CARDS];
                snapshot[..count].copy_from_slice(&s.hand[..count]);
                s.n_hand = 0;
                for &c in &snapshot[..count] {
                    exhaust_card(s, c);
                }
                for _ in 0..count {
                    if (s.n_cards as usize) >= MAX_CARDS || (s.n_hand as usize) >= MAX_HAND {
                        break;
                    }
                    let Some(id) = crate::content::random_generated_card(&mut s.rng.gen, false)
                    else {
                        break;
                    };
                    let flags = if upgraded { F_UPGRADED } else { 0 };
                    let ix = s.n_cards;
                    s.cards[ix as usize] = CardInst { id, flags, bonus: 0, cost_delta: 0, ench: 0, ench_amt: 0 };
                    s.n_cards += 1;
                    s.hand[s.n_hand as usize] = ix;
                    s.n_hand += 1;
                }
            }
            // 地狱之刃：随机一张**攻击牌**进手牌，**本回合免费**。
            // 免费走的是上一轮给技能药水加的 `F_FREE_THIS_TURN`
            // ——同一个机制（[源码] 里两边都是 `SetToFreeThisTurn()`）。
            // 余烬：消耗**抽牌堆顶**的 N 张（[源码] 每次都先 `ShuffleIfNecessary`）。
            Op::ExhaustFromDrawTop(n) => {
                for _ in 0..n {
                    if s.n_draw == 0 {
                        s.reshuffle_discard_into_draw();
                    }
                    if s.n_draw == 0 {
                        break;
                    }
                    let Some(c) = s.pop_draw_top() else { break };
                    exhaust_card(s, c);
                }
            }
            // 坚毅（基础版）：**随机**消耗 N 张手牌。
            // 升级版是玩家自己选，走 `ExhaustChoose` —— 两者不是同一个 op，
            // 因为"随机"和"你挑"在估值上差很远（挑的时候会挑垃圾牌）。
            Op::ExhaustRandomFromHand(n) => {
                for _ in 0..n {
                    if s.n_hand == 0 {
                        break;
                    }
                    let k = crate::state::next_below(&mut s.rng.gen, s.n_hand as usize);
                    let c = s.take_from_hand(k);
                    exhaust_card(s, c);
                }
            }
            // 痛殴：消耗手里**随机一张攻击牌**，把它的伤害**永久**加到这张牌上。
            //
            // 加的是那张牌的**卡面基础伤害**（升级态算进去）。[源码] 加的是
            // 过完 `ModifyDamage` 的值 —— 那会把当时的力量/易伤也算进去，
            // 意味着同一张痛殴在不同局面下涨得不一样。这里取**基础值**，
            // 是一个**已知的简化**：不高估玩家，也不让牌的强度依赖打出的时机。
            // 真实差异要等实战对拍报出来。
            Op::ExhaustRandomAttackAddDamage => {
                let mut cand = [0usize; MAX_CARDS];
                let mut n = 0usize;
                for i in 0..s.n_hand as usize {
                    let id = s.cards[s.hand[i] as usize].id;
                    if matches!(card(id).kind, Kind::Attack) {
                        cand[n] = i;
                        n += 1;
                    }
                }
                if n > 0 {
                    let k = crate::state::next_below(&mut s.rng.gen, n);
                    let hi = cand[k];
                    let victim = s.hand[hi];
                    let vinst = s.cards[victim as usize];
                    let dmg = base_damage_of(vinst);
                    let c = s.take_from_hand(hi);
                    exhaust_card(s, c);
                    // 永久写回**这一张实例**（和暴走同一个机制）
                    let b = s.cards[cix].bonus as i32 + dmg;
                    s.cards[cix].bonus = b.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                }
            }
            // 劫掠「抽牌直到你抽到一张非攻击牌」。
            // [源码] 是 **do-while**：至少抽一张，之后只要抽到的是攻击牌就继续，
            // 手牌到 10 张也停。写成 while 的话手上没牌时会一张都不抽。
            Op::DrawUntilNonAttack => {
                for _ in 0..MAX_CARDS {
                    let before = s.n_hand;
                    s.draw_n(1);
                    if s.n_hand == before {
                        break; // 抽不动了（牌堆空 / 手牌满 / NoDraw）
                    }
                    let id = s.cards[s.hand[s.n_hand as usize - 1] as usize].id;
                    if !matches!(card(id).kind, Kind::Attack) {
                        break;
                    }
                    if s.n_hand as usize >= 10 {
                        break;
                    }
                }
            }
            // 原始力量：手牌里所有攻击牌**变形**成另一张牌。
            // 变形是就地改 `CardInst.id`（牌实例还是那一张，身份换了），
            // 不是"消耗掉再生成" —— 后者会触发无惧疼痛/黑暗之拥，那是错的。
            Op::TransformAttacksInHand { into, upgraded } => {
                for i in 0..s.n_hand as usize {
                    let c = s.hand[i] as usize;
                    if matches!(card(s.cards[c].id).kind, Kind::Attack) {
                        s.cards[c].id = into;
                        s.cards[c].bonus = 0;
                        if upgraded {
                            s.cards[c].flags |= F_UPGRADED;
                        } else {
                            s.cards[c].flags &= !F_UPGRADED;
                        }
                    }
                }
            }
            // 旋风斩「对所有敌人造成 N 点伤害 X 次」。X = 打出时花光的能量。
            Op::DamageAllXTimes { base } => {
                let x = s.last_x;
                for e in 0..s.n_enemies as usize {
                    if !s.enemies[e].alive() {
                        continue;
                    }
                    let face = face_of(s, base, vigor);
                    for _ in 0..x {
                        hit_enemy_with(s, e, face, powered, 0);
                    }
                }
                spend_vigor(s, &mut vigor);
            }
            // 倾泻「打出你抽牌堆顶部的 X 张牌」（升级 X+1）。
            // 和破灭共用 `auto_play_card`，只是张数是 X 且**不强制消耗**。
            Op::AutoPlayFromDrawTopX { plus, force_exhaust } => {
                let count = s.last_x + plus;
                let mut picked = [0u8; MAX_CARDS];
                let mut n = 0usize;
                for _ in 0..count {
                    if s.n_draw == 0 {
                        s.reshuffle_discard_into_draw();
                    }
                    if s.n_draw == 0 {
                        break;
                    }
                    let Some(c) = s.pop_draw_top() else { break };
                    picked[n] = c;
                    n += 1;
                }
                for &c in &picked[..n] {
                    auto_play_card(s, c, force_exhaust, 0);
                }
            }
            // 破灭「打出抽牌堆顶部的牌并将其消耗。」
            //
            // [源码] `AutoPlayFromDrawPile` 是**两阶段**的：先把 N 张全部搬进
            // "正在打出"堆，再逐张打。张数 > 1 时这个区别看得见 —— 第一张
            // 打出的效果不会改变第二张是谁。这里照抄那个结构。
            // 抽牌堆顶 = `draw` 数组末尾（和 `draw_one` 一致）。
            Op::AutoPlayFromDrawTop { count, force_exhaust } => {
                let mut picked = [0u8; MAX_CARDS];
                let mut n = 0usize;
                for _ in 0..count {
                    if s.n_draw == 0 {
                        s.reshuffle_discard_into_draw();
                    }
                    if s.n_draw == 0 {
                        break;
                    }
                    let Some(c) = s.pop_draw_top() else { break };
                    picked[n] = c;
                    n += 1;
                }
                for &c in &picked[..n] {
                    auto_play_card(s, c, force_exhaust, 0);
                }
            }
            Op::GenerateFree(kind) => {
                if (s.n_cards as usize) < MAX_CARDS && (s.n_hand as usize) < MAX_HAND {
                    if let Some(id) =
                        crate::content::random_generated_card_of(&mut s.rng.gen, kind)
                    {
                        let ix = s.n_cards;
                        s.cards[ix as usize] =
                            CardInst { id, flags: F_FREE_THIS_TURN, bonus: 0, cost_delta: 0, ench: 0, ench_amt: 0 };
                        s.n_cards += 1;
                        s.hand[s.n_hand as usize] = ix;
                        s.n_hand += 1;
                    }
                }
            }
            // 这一张实例的费用永久 +n（狂乱逃离）。和 `GrowThisCard` 一样，
            // **本次结算已经按旧费用扣过钱了**，从下一次打出开始生效。
            Op::GrowThisCardCost(n) => {
                if cix < MAX_CARDS {
                    let v = s.cards[cix].cost_delta as i32 + n;
                    s.cards[cix].cost_delta = v.clamp(-100, 100) as i8;
                }
            }
            Op::ExhaustNonAttacksForBlock { per } => {
                let mut snapshot = [0u8; MAX_CARDS];
                let mut n_snap = 0usize;
                let mut keep = [0u8; MAX_CARDS];
                let mut n_keep = 0usize;
                for i in 0..s.n_hand as usize {
                    let c = s.hand[i];
                    if matches!(card(s.cards[c as usize].id).kind, Kind::Attack) {
                        keep[n_keep] = c;
                        n_keep += 1;
                    } else {
                        snapshot[n_snap] = c;
                        n_snap += 1;
                    }
                }
                s.n_hand = n_keep as u8;
                s.hand[..n_keep].copy_from_slice(&keep[..n_keep]);
                for &c in &snapshot[..n_snap] {
                    exhaust_card(s, c);
                    let b = card_block(per, &mut s.player);
                    gain_block(s, Owner::Player, b, 0);
                }
            }
            Op::ExhaustChoose(n) => {
                if s.n_hand > 0 {
                    s.pending = Pending::ExhaustFromHand { remaining: n as u8 };
                }
            }
            Op::StrengthPerTargetVuln => {
                let v = s.enemies[target].get(St::Vulnerable);
                s.player.add(St::Strength, v);
            }
            Op::FreeNextAttack => s.free_attack += 1,
            Op::Heal(n) => {
                s.player.hp = (s.player.hp + n).min(s.player.max_hp);
                refresh_hp_threshold_strength(s);
            }
            // 拳斗。紧跟在伤害 op 后面读 `last_damage`。
            // 和别处的格挡一样走 `card_block`（这是从**卡牌**获得的格挡，吃脆弱）。
            Op::BlockEqualToLastDamage => {
                // 拳斗也是"从卡牌获得的格挡"，所以它一样吃臂甲的翻倍
                let last = s.last_damage;
                let b = card_block(last, &mut s.player);
                gain_block(s, Owner::Player, b, 0);
            }
            // 万向斩。其他敌人吃的是主目标那个**已经算好的平值**，
            // 不再各自过一遍乘区；格挡照常吸收。
            Op::DamageOthersEqualToLast => {
                let d = s.last_damage;
                for e in 0..s.n_enemies as usize {
                    if e != target && s.enemies[e].alive() {
                        absorb(&mut s.enemies[e], d);
                    }
                }
            }
            Op::DoubleTargetVuln => {
                let v = s.enemies[target].get(St::Vulnerable);
                if v > 0 {
                    s.enemies[target].set(St::Vulnerable, v * 2);
                }
            }
            // 走格挡（玩家确认）。所以它**不唤醒撕裂** —— 和敌人打的伤害
            // 走同一条路，撕裂管的是"失去生命"那条路（御血术/烙印）。
            // **加上这一张实例的 `bonus`**。表里那个数是基础值：永世沙漏的
            // 剧烈增强把每一张凋萎 `FakeUpgrade()`（各 +3），层数记在
            // `CardInst::bonus` 上（游戏把它写进牌名 `凋萎+1`，`sync` 解析）。
            // 灼伤/感染/腐朽/瓦解那几张的 bonus 恒为 0，行为不变。
            Op::TakeDamage(n) => {
                let through = absorb(&mut s.player, n + inst.bonus as i32);
                s.hp_lost_this_turn += through;
                after_player_hp_lost(s, through, 0);
            }
            // 异蛇之油。[源码] `SneckoOil.OnUse`：抽完之后遍历**手牌**，
            // 跳过 X 费牌和当前费用 < 0 的，其余 `SetThisTurnOrUntilPlayed(NextInt(4))`。
            //
            // 内核把结果写进 `CardInst::cost_delta`（让**有效费用**等于抽到的数），
            // 和狂乱逃离共用那一个字段。**代价说清楚**：`cost_delta` 是永久的，
            // 而源码那条是"本回合或直到打出"—— 内核少了一次回合末归位。
            // 对拍上看不见（`sync` 每帧拿观测费用盖回去，那是"观测费用是权威"），
            // 跨回合推演里会**多留一个回合的折扣**。和 `F_FREE_THIS_TURN`
            // 至今也没有回合末清理是同一个洞，两者该一起补。
            Op::RandomizeHandCosts { max } => {
                if max > 0 {
                    for i in 0..s.n_hand as usize {
                        let ix = s.hand[i] as usize;
                        let id = s.cards[ix].id;
                        if crate::content::costs_x(id) {
                            continue;
                        }
                        let d = card(id);
                        let base =
                            if s.cards[ix].upgraded() { d.cost_upg } else { d.cost };
                        if base < 0 {
                            continue;
                        }
                        // 用 `gen` 流（生成类随机都走它），不和抽牌的 `shuffle`
                        // 混在一起 —— 混了的话喝不喝这瓶药水会改变后面的抽牌顺序。
                        let want = crate::state::next_below(&mut s.rng.gen, max as usize) as i32;
                        s.cards[ix].cost_delta = (want - base).clamp(-100, 100) as i8;
                    }
                }
            }
            Op::Conditional { cond, then } => {
                if cond_holds(s, cond) {
                    resolve_ops(s, then, inst, cix, target, src);
                }
            }
            Op::GainMaxEnergy(n) => s.base_energy += n,
            // 「把**这张牌**本场的伤害改大」——药水没有卡实例可写回，直接跳过。
            // 不加这个守卫的话，药水侧传进来的 `cix` 会去改 0 号牌（打击）。
            Op::GrowThisCard(n) => {
                if src == Source::Card {
                    let b = s.cards[cix].bonus as i32 + n;
                    s.cards[cix].bonus = b.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                }
            }
            Op::FetchFromDiscard(n) => {
                if s.n_disc > 0 {
                    s.pending = Pending::FetchFromDiscard { remaining: n as u8 };
                }
            }
            Op::PutOnTopOfDraw(n) => {
                if s.n_hand > 0 {
                    s.pending = Pending::PutToDrawPile { remaining: n as u8 };
                }
            }
            // 基础版武装是**选一张**升。候选同样按 `IsUpgradable` 筛
            // （[源码] `CardSelectCmd.FromHandForUpgrade`），一张可升的都没有
            // 就根本不开选择界面 —— 否则会挂一个谁都满足不了的 `Pending`。
            Op::UpgradeInHand(n) => {
                let any = (0..s.n_hand as usize).any(|i| {
                    let c = s.hand[i] as usize;
                    crate::content::upgradable(s.cards[c].id)
                        && s.cards[c].flags & F_UPGRADED == 0
                });
                if any {
                    s.pending = Pending::UpgradeInHand { remaining: n as u8 };
                }
            }
        }
    }
}

/// 爪牙（`St::Minion`）不算数：**非爪牙全死光，战斗就结束**，
/// 场上还剩着的爪牙跟着一起消失。
///
/// 两次独立实测（2026-08-15）：
/// * 第15层，雾菇一死，它召出来的 6 血利齿之眼当场消失，战斗立刻结束
/// * 第17层 Boss，同族神官一死，两个 34/31 血的同族信徒当场消失
///
/// 不实现这条的话 L2 会以为必须把所有敌人打完 —— 那场 Boss 就会被估成
/// 307 血而不是实际需要打的 190。
///
/// **只在场上曾经有过非爪牙时才适用**：万一遇到一场从头到尾只有爪牙的仗，
/// 退回普通规则，免得开局就判定结束。
fn no_master_left(s: &State) -> bool {
    let mut saw_master = false;
    for e in 0..s.n_enemies as usize {
        if s.enemies[e].get(St::Minion) == 0 {
            saw_master = true;
            // **「还在场上」不是「活着」。** 带适生力的实验体被砍死之后血量是 0，
            // 但它要等到自己的回合才复苏 —— 用 `alive()` 判的话，
            // 砍掉第一条命就直接判"主人全死光"、战斗当场结束。
            // 和 `State::any_enemy_present` 是同一条口径，两处必须一致。
            if s.enemies[e].alive() || s.enemies[e].get(St::Adaptable) > 0 {
                return false;
            }
        }
    }
    saw_master
}

/// 瓶中精灵（[源码] `FairyInABottle`）：**将要死的那一刻**丢掉这瓶，回到
/// 最大生命值的 30%。
///
/// 源码是两段：`ShouldDie` 对持有者返回 `false`（阻止死亡），
/// `AfterPreventingDeath` 再走 `OnUse` = `Heal(max(MaxHp * 0.3, 1))`。
/// **血量先被夹到 0 再回血**，所以结果是 30% 而不是"当前血 + 30%"。
///
/// [实测] 2026-08-22 第1幕Boss 帧46：上限 80、7 血挨 21 点，结果正好 **24**。
/// 那一帧在补这条之前是语料里唯一一个真 MISMATCH（游戏 24 / 内核 0）。
///
/// 身上有多瓶时只消耗一瓶 —— 找到第一瓶就返回。
fn try_fairy(s: &mut State) -> bool {
    let n = (s.potion_slots as usize).min(crate::state::MAX_POTIONS);
    for i in 0..n {
        if s.potions[i] == potion::FAIRY {
            s.potions[i] = potion::NONE;
            s.player.hp = (s.player.max_hp * 3 / 10).max(1);
            refresh_hp_threshold_strength(s);
            return true;
        }
    }
    false
}

fn check_over(s: &mut State) {
    // **已经死了就不再翻案。** 强制死亡（沙坑）不走血量那条路，而 `check_over`
    // 会被反复调用 —— 少了这道守卫，下一次调用会看见 `hp <= 0` 然后把瓶中精灵
    // 递上去，把一次 `force: true` 的死亡救回来。
    if s.player_dead {
        s.combat_over = true;
        return;
    }
    if s.player.hp <= 0 {
        // 先给瓶中精灵一次机会。它成功了就**不算死**，战斗继续。
        if try_fairy(s) {
            return;
        }
        s.player.hp = 0;
        s.player_dead = true;
        s.combat_over = true;
    } else if !s.any_enemy_present() || no_master_left(s) {
        // **只在刚翻成"结束"的那一次结算胜利遗物。** `check_over` 会被反复
        // 调用（每次伤害落地、每个动作之后），漏了这个守卫就会每调一次回一次血。
        let just_won = !s.combat_over;
        s.combat_over = true;
        if just_won {
            // 顺序是 [源码] 定的、也被实录判过：带骨肉(Early) 在燃烧之血之前。
            // 反过来的话 50% 阈值会拿加过 6 点的血量去判，act2_f31 那场
            // （36/80 结束）就会从 +18 变成 +6。
            fire(s, Hook::CombatVictoryEarly, 0);
            fire(s, Hook::CombatVictory, 0);
        }
    }
}

fn play_card(mut s: State, hand_ix: usize, target: usize) -> State {
    let cost = effective_cost(&s, hand_ix);
    let cix = s.hand[hand_ix];
    let inst = s.cards[cix as usize];
    let d = card(inst.id);

    if matches!(d.kind, Kind::Attack) && s.free_attack > 0 {
        s.free_attack -= 1;
    }
    // X 费牌把「这次花了多少」记下来给 ops 用。非 X 费牌保持 0，
    // 免得某张牌误读到上一张 X 费牌留下的值。
    s.last_x = if crate::content::costs_x(inst.id) { cost } else { 0 };
    s.energy -= cost;

    // Remove from hand *before* resolving, so 恶魔之焰 exhausts the other cards
    // and not itself.
    s.take_from_hand(hand_ix);

    resolve_played_card(&mut s, cix, target, false, 0);
    s
}

/// **一张牌离开手牌之后**的全部结算。`play_card` 和 `auto_play_card` 共用。
///
/// 抽出来共用不是为了少写几行：能量之外的每一件事（激怒、消耗/弃牌路由、
/// `cards_played` / `attacks_played` 计数、狂怒、缓慢累加、结束判定）
/// 自动playAuto 和手动出牌就**不可能长歪**。这和 `end_turn_with_incoming`
/// 与 `enemy_turn` 共用 `take_attack_hit` 是同一个套路。
///
/// * `force_exhaust`：破灭那类「打出并消耗」，覆盖卡面自带的消耗位。
/// * `depth`：`auto_play` 会递归（破灭翻出破灭），由 `MAX_HOOK_DEPTH` 兜底。
/// 把朝向转到 `tgt` 那一侧（游戏原文：「使用有目标的卡牌或药水来改变你的朝向」）。
///
/// **只有带 `Surrounded` 的战斗里这件事才有后果**，但内核一律记着 ——
/// 记账便宜，而"只在需要时才记"会让读档/中途接入的那一帧无从还原。
///
/// 站位是敌人身上的 status（`BackAttackLeft` / `BackAttackRight`），
/// 所以这里只是把那一侧抄到玩家的 `FacingRight` 上。
/// 敌人两侧都没标（普通战斗）时**什么都不做** —— 别把朝向清成一个假值。
pub(crate) fn face_toward(s: &mut State, tgt: usize) {
    if tgt >= s.n_enemies as usize {
        return;
    }
    if s.enemies[tgt].get(St::BackAttackRight) > 0 {
        s.player.set(St::FacingRight, 1);
    } else if s.enemies[tgt].get(St::BackAttackLeft) > 0 {
        s.player.set(St::FacingRight, 0);
    }
}

fn resolve_played_card(s: &mut State, cix: u8, target: usize, force_exhaust: bool, depth: u8) {
    let inst = s.cards[cix as usize];
    let d = card(inst.id);

    // 杂耍要知道"刚打出的是哪一张"。钩子本身带不了这个信息（`fire_ctx` 的 ctx
    // 是敌人下标），所以放进 State，和 `last_damage` 同一类：只在紧跟其后的
    // 触发里有意义。
    s.last_played_card = cix;

    // **有目标的牌会把朝向转过去**，而且要在结算之前 —— 这一手打出去之后
    // 敌人那一击算不算"从背后"，看的是转完之后的朝向。
    if d.targeted {
        face_toward(s, target);
    }

    // 激怒（敌人）在这里触发。能力牌不触发它 —— 靠的是 `Kind::Power != Kind::Skill`
    // 这个天然区分，不是特判；规则本身在 `content::POWERS` 里。
    //
    // **`skills_played` 必须在点火之前加**：开信刀（每 3 张技能牌）也挂在这个钩子上，
    // 它读的就是这个计数。加在后面的话，第 3 张技能打出时计数还是 2，永远不发作。
    //
    // 计数放在这里、而攻击那个 `attacks_played` 放在结算之后，是因为**两个钩子的
    // 时点本来就不一样**（`PlayerSkill` 在牌结算前、`PlayerAttack` 在结算后），
    // 各自的消费者都按自己那一侧验证过。不要为了"对称"把其中一个挪走。
    if matches!(d.kind, Kind::Skill) {
        s.skills_played += 1;
        fire(s, Hook::PlayerSkill, depth);
    }

    // 钢笔尖，第一半：**结算之前**数这一张（[源码] `PenNib.BeforeCardPlayed`）。
    //
    // 计数器 `mod 10`，归零的那一次就是"第 10 张"，给这一次出牌挂上翻倍标记
    // （源码里的 `AttackToDouble`），`damage::apply_modifiers` 读它。
    //
    // **必须在结算之前**：数在后面的话这张牌自己吃不到翻倍，
    // 而游戏正是让第 10 张自己翻倍的。
    //
    // 进来先把标记清掉、出去再还原，是为了**套娃出牌**（破灭翻出一张牌、
    // 彼岸咆哮回合末自己打出来）：内层那张不该沾上外层的翻倍
    // —— 源码里那是一个具体的 `CardModel` 引用，认牌不认时间窗。
    let outer_armed = s.player.get(St::PenNibArmed);
    s.player.set(St::PenNibArmed, 0);
    if matches!(d.kind, Kind::Attack) && s.player.get(St::PenNib) > 0 {
        let n = (s.player.get(St::PenNibCount) + 1) % 10;
        s.player.set(St::PenNibCount, n);
        if n == 0 {
            s.player.set(St::PenNibArmed, 1);
        }
    }

    let ops = card_ops(inst.id, inst.upgraded());
    resolve_ops(s, ops, inst, cix as usize, target, Source::Card);

    // 连环拳「你打出的下 N 张攻击牌会被额外打出一次」。
    // [源码] `OneTwoPunchPower.ModifyCardPlayCount` 给攻击牌 +1 次，
    // 然后 `AfterModifyingCardPlayCount` 立刻 `Decrement` —— 所以是
    // **每张攻击牌消耗一层**，不是整个回合都翻倍。
    //
    // 只重跑 `ops`，不重跑消耗/弃牌路由和计数 —— 游戏是"同一张牌多打出一次"，
    // 不是"多打出一张牌"。
    if matches!(d.kind, Kind::Attack) && s.player.get(St::OneTwoPunch) > 0 && depth < MAX_HOOK_DEPTH
    {
        s.player.add(St::OneTwoPunch, -1);
        resolve_ops(s, ops, inst, cix as usize, target, Source::Card);
    }

    // 钢笔尖，第二半：这张牌打完了就摘掉标记（[源码] `AfterCardPlayed` 把
    // `AttackToDouble` 置空），把外层那一次的标记还回去。
    //
    // **连环拳那次重打在标记里面**：游戏侧是"同一张牌多打出一次"，
    // `AttackToDouble` 一直指着它 ⇒ 两次都翻倍。和 `cards_played` 只 +1 同口径。
    s.player.set(St::PenNibArmed, outer_armed);

    if force_exhaust || d.exhausts {
        exhaust_card(s, cix);
    } else if matches!(d.kind, Kind::Power) {
        // 能力牌打出后**离场**：既不进弃牌堆也不进消耗堆。
        // 实测（2026-08-15 第1幕 Boss）：打出薪火之源后弃牌堆仍然是空的，
        // 而内核当时把它塞进了弃牌堆。牌实例留在 `s.cards` 里但不属于任何牌区
        // —— 这正是"移出游戏"的意思。
        //
        // 不能图省事丢进消耗堆：那会错误地触发 `CardExhausted`
        //（无惧疼痛白拿格挡、黑暗之拥白抽牌）。
    } else {
        s.to_discard(cix);
    }

    s.cards_played += 1;
    // `skills_played` **不在这里加** —— 它在上面 `Hook::PlayerSkill` 点火之前就加过了，
    // 理由见那里。
    if matches!(d.kind, Kind::Attack) {
        s.attacks_played += 1;
        // 狂怒。**在牌结算之后**触发，所以全身撞击吃不到自己这一下给的格挡
        fire(s, Hook::PlayerAttack, depth);
    }

    // 「打出了任意一张牌」。[源码] `AfterCardPlayed`，谁在用：凋萎存在。
    // **放在 `PlayerAttack` 之后**：攻击牌两个钩子都要过，先按类型的那个
    // （已验证的行为不动），再过这个通用的。
    // **每张牌只触发一次** —— 连环拳让 `ops` 跑两遍，但那是"同一张牌多打出
    // 一次"，`cards_played` 同样只 +1，两处口径必须一致。
    fire(s, Hook::CardPlayed, depth);

    // 缓慢：带这个 power 的敌人，**每打出一张牌**（不只是攻击牌）受到的攻击
    // 伤害就再 +10%。累加发生在牌结算**之后**，所以这张牌自己不吃自己的加成。
    //
    // 实测（2026-08-15 第1幕第13层，旧日雕像）一回合内逐张读数：
    //   预备打击(伤害7, SLOW 0->10) 防御(->20) 防御(->30) 防御(->40)
    //   打击 (6+2力量)×1.4 = 11.2 -> 11，同时 SLOW ->50
    // 三件事一次钉死：防御这种技能牌也计数；计数**不含自己**（否则打击是 12）；
    // 力量加在缓慢**之前**（否则打击是 10）。喝药水不计数（SLOW 保持 0）。
    for e in 0..s.n_enemies as usize {
        if s.enemies[e].alive() {
            let per = s.enemies[e].get(St::SlowSource);
            if per > 0 {
                s.enemies[e].add(St::Slow, per);
            }
        }
    }

    check_over(s);
}

/// **在结算过程中再打出一张牌**（破灭翻抽牌堆顶、惊逃回合结束自动出牌）。
///
/// 玩家判定（2026-08-19）：**不扣能量，但计入**（`cards_played` /
/// `attacks_played` 都照加）。「计入」是共用 `resolve_played_card` 天然得到的，
/// 不是这里另写的一条。
///
/// [源码] `CardCmd.AutoPlay`：不可打出的牌**不结算、直接进弃牌堆**；
/// 指向性牌没给目标时**随机挑一个敌人**（`Rng.CombatTargets`，
/// 正是内核这条 `rng.enemy` 流）。
///
/// `depth` 由 `MAX_HOOK_DEPTH` 兜底 —— 破灭翻出破灭是真会发生的。
fn auto_play_card(s: &mut State, cix: u8, force_exhaust: bool, depth: u8) {
    if depth > MAX_HOOK_DEPTH || s.combat_over {
        return;
    }
    let inst = s.cards[cix as usize];
    let d = card(inst.id);
    // 不可打出的牌（黏液/伤口那类）：不结算，直接进弃牌堆。
    if !playable(inst.id) {
        s.to_discard(cix);
        return;
    }
    let target = if d.targeted {
        match random_living_enemy(s) {
            Some(e) => e,
            None => return,
        }
    } else {
        0
    };
    resolve_played_card(s, cix, target, force_exhaust, depth + 1);
}

#[inline]
fn strip_temp_strength(e: &mut Entity) {
    let tmp = e.get(St::TempStrength);
    if tmp != 0 {
        e.add(St::Strength, -tmp);
        e.set(St::TempStrength, 0);
    }
}

fn decay(e: &mut Entity) {
    for st in [St::Vulnerable, St::Weak, St::Frail] {
        let v = e.get(st);
        if v > 0 {
            e.set(st, v - 1);
        }
    }
}

/// 挨敌人 `attacker` 的**一段**攻击，`d` 是**过完所有乘区之后**的最终数字。
///
/// 敌人回合的每一击都走这里，不管这个数字是内核自己按 `EnemyDef` 算的
/// （[`enemy_turn`]）还是外部给定的（[`end_turn_with_incoming`]）——
/// 后者是 L2 求解器的入口，让它拿观测到的意图标签当威胁，而**不用自己实现
/// 格挡吸收、孤注一掷、火焰屏障这些规则**。写成一个函数就是为了保证
/// 两条路径永远不会长歪。
pub(crate) fn take_attack_hit(s: &mut State, attacker: usize, d: i32) {
    let through = absorb(&mut s.player, d);
    s.hp_lost_this_turn += through;
    // 失衡（[源码] `ImbalancedPower.AfterDamageGiven`）：带失衡的敌人，
    // 这一下**被完全挡住**就进入失衡，下一手改走晕眩。
    //
    // 收在这里而不是在 `enemy_turn` 里，理由和别的收口一样：注入式敌人回合
    //（`end_turn_with_incoming`）也走这条路，写在外面就只有一半路径生效。
    if d > 0 && through == 0 && attacker < s.n_enemies as usize
        && s.enemies[attacker].get(St::Imbalanced) > 0
    {
        s.enemies[attacker].set(St::OffBalance, 1);
    }
    // 孤注一掷：「如果你在本场战斗中受到未被格挡的攻击伤害，
    // 则立刻死亡。」只认**攻击**伤害，所以只在这里判，
    // 不在灼伤那类 `Op::TakeDamage` 上判。
    // 漏掉它会让内核以为这是张白送 50 格挡的牌 ——
    // 正是"模拟器过于乐观"那个失败模式。
    if through > 0 && s.player.get(St::AllOrNothing) > 0 {
        s.player.hp = 0;
    }
    // 百年积木：**真掉了血**才算。和下面的火焰屏障正好是一对反例 ——
    // 一个看"血少没少"，一个看"挨没挨打"，所以是两个钩子而不是一个带条件的钩子。
    after_player_hp_lost(s, through, 0);
    // 纸伤难愈 / 剧痛刺击：**攻击者自己的**能力，这一段真的打穿了才发作。
    // 收在这里的理由和失衡一样：注入式敌人回合也走这条路。
    if through > 0 && attacker < s.n_enemies as usize {
        fire_ctx(s, Hook::AttackUnblocked, 0, attacker);
    }
    // 火焰屏障。**每一段各触发一次**（玩家确认），
    // 且不看这一下有没有被格挡吃掉 —— 它自带 12 点格挡，
    // 要求"真掉血才反伤"等于两个效果互相抵消。
    fire_ctx(s, Hook::Attacked, 0, attacker);
}

/// 敌人回合开始：格挡在**拥有者**的回合开始时清空（持有钻地 BurrowedPower 时不清空）。
///
/// 两条敌人回合的路径共用它，见 [`take_attack_hit`]。
fn begin_enemy_turn(s: &mut State) {
    // 敌人这一边的回合开始，**最早一档**（[源码] `BeforeSideTurnStart`，在清格挡之前）。
    // 两条敌人回合路径都走这里，所以注入式威胁下硬化外壳也照样回满。见 `Hook::SideTurnStart`。
    fire(s, Hook::SideTurnStart, 0);
    for e in 0..s.n_enemies as usize {
        if s.enemies[e].get(St::Burrowed) == 0 {
            s.enemies[e].block = 0;
        }
    }
}

/// 这只敌人**当前**该出第几手。
///
/// 有机器时 `enemy_move[e]` 存的是**手的下标**，没有时存的是**出招次数** ——
/// 两个含义不同，所以任何地方都不要直接读那个字段，走这里。
#[inline]
pub fn current_move_ix(def: &EnemyDef, s: &State, e: usize) -> usize {
    let n = def.moves.len().max(1);
    if def.machine.is_some() {
        (s.enemy_move[e] as usize) % n
    } else {
        move_index(def, s.enemy_move[e] as u32)
    }
}

/// 把刚出完的这一手记进历史（新的在 `[0]`）。
fn push_hist(s: &mut State, e: usize, ix: u8) {
    for i in (1..ENEMY_HIST).rev() {
        s.enemy_hist[e][i] = s.enemy_hist[e][i - 1];
    }
    s.enemy_hist[e][0] = ix;
}

/// 条件求值。判不出来的返回 `None` —— 调用方会把这一支当成"可能成立"，
/// 于是允许集合变大。**大一点只是弱，猜错是自信地错。**
fn eval_cond(s: &State, e: usize, used: u32, c: ECond) -> Option<bool> {
    match c {
        ECond::Unknown => None,
        // `used` 已经含当前这一手（`eff_used`），和 `Repeat::Once` 同一份数据。
        ECond::MoveUnused(ix) => Some(used & bit(ix) == 0),
        // 三值与：一条确定不成立就够了；有判不了的就整体判不了。
        ECond::All(cs) => {
            let mut unknown = false;
            for c in cs {
                match eval_cond(s, e, used, *c) {
                    Some(false) => return Some(false),
                    Some(true) => {}
                    None => unknown = true,
                }
            }
            if unknown { None } else { Some(true) }
        }
        // 含它自己 —— [源码] `GetTeammatesOf(c) => GetCreaturesOnSide(c.Side)`。
        // 这里不过滤爪牙：组装师数的是**同一边所有活着的**，爪牙也算。
        ECond::AlliesAliveAtLeast(n) => Some(
            (0..s.n_enemies as usize).filter(|&i| s.enemies[i].alive()).count() >= n as usize,
        ),
        ECond::Alone => Some(
            (0..s.n_enemies as usize)
                .filter(|&i| s.enemies[i].alive() && s.enemies[i].get(St::Minion) == 0)
                .count()
                <= 1,
        ),
        // 「最前面那只」内核用**槽位号最小的活着的敌人**近似。游戏里这是
        // 遭遇在开局钉死的属性（[源码] `NibbitsNormal` 给第一只设
        // `IsFront = true`），而观测里的槽位顺序就是出场顺序。
        // **三只以上没验过。**
        ECond::Front | ECond::NotFront => {
            let front = (0..s.n_enemies as usize).find(|&i| s.enemies[i].alive());
            let is = front == Some(e);
            Some(if matches!(c, ECond::Front) { is } else { !is })
        }
        ECond::HpAtMost(n) => Some(s.enemies[e].hp <= n),
        ECond::MaxHpAtMost(n) => Some(s.enemies[e].max_hp <= n),
        ECond::SlotIs(n) => Some(e == n as usize),
        // 代表元提示不是事实：集合照「哪一支都可能」算，挑哪一个归 `initial_move`。
        ECond::SlotRep(_) => None,
        ECond::OffBalance => Some(s.enemies[e].get(St::OffBalance) > 0),
        ECond::NotOffBalance => Some(s.enemies[e].get(St::OffBalance) == 0),
        ECond::CursesTakenBelow(sets, n) => {
            Some(crate::content::curses_taken(&s.player, sets) < n as usize)
        }
        ECond::CursesTakenAtLeast(sets, n) => {
            Some(crate::content::curses_taken(&s.player, sets) >= n as usize)
        }
        // 读的是**推进指针那一刻**的层数，和游戏掷招的时点差着敌人回合末，见 `ECond` 的文档。
        ECond::SelfStatusAtLeast(st, n) => Some(s.enemies[e].get(st) >= n),
        ECond::SelfStatusBelow(st, n) => Some(s.enemies[e].get(st) < n),
    }
}

/// **判定用的历史**：`enemy_hist` 里存的是「当前这一手**之前**」的，
/// 而 `NotTwice` / `cooldown` 要连**当前这一手**一起算 ——
/// 因为问的是"当前这一手打完之后能接什么"，那时当前手已经进历史了。
///
/// 这一位曾经差错过：`enemy_hist` 里直接带上当前手，结果
/// `advance_move` 里又 push 一次，同一手被数了两遍，
/// 「不能连出」变成了「隔一手不能出」。两个测试当场抓到。
fn eff_hist(s: &State, e: usize, cur: u8) -> [u8; ENEMY_HIST] {
    let mut h = [u8::MAX; ENEMY_HIST];
    h[0] = cur;
    for i in 1..ENEMY_HIST {
        h[i] = s.enemy_hist[e][i - 1];
    }
    h
}

/// 这一条随机分支现在还能不能走（[源码] `RandomBranchState.GetStateWeight`）。
///
/// `used` 是「这一整场出过哪几手」的位掩码，**必须已经含当前这一手** ——
/// 和 `h` 同一个道理（见 [`eff_hist`]）。只有 [`Repeat::Once`] 读它。
fn branch_open(h: &[u8; ENEMY_HIST], used: u32, b: &Branch) -> bool {
    let cd = (b.cooldown as usize).min(ENEMY_HIST);
    if cd > 0 && h.iter().take(cd).any(|&x| x == b.to) {
        return false;
    }
    match b.repeat {
        Repeat::Forever => true,
        Repeat::NotTwice => h[0] != b.to,
        Repeat::AtMost(n) => {
            let n = (n as usize).min(ENEMY_HIST);
            n == 0 || !h.iter().take(n).all(|&x| x == b.to)
        }
        Repeat::Once => used & bit(b.to) == 0,
    }
}

/// 出招下标 -> 位掩码的那一位。下标理论上能超过 31（表里最多 5 手），
/// 越界就退回第 31 位而不是 panic（不变量 5）。
#[inline]
fn bit(ix: u8) -> u32 {
    1u32 << (ix as u32).min(31)
}

/// 「这一整场出过哪几手」，**含当前这一手**。和 [`eff_hist`] 成对使用。
#[inline]
fn eff_used(s: &State, e: usize, cur: u8) -> u32 {
    s.enemy_used[e] | bit(cur)
}

/// **下一手的允许集合**，位掩码（第 i 位 = 第 i 手可能出）。
///
/// 这是这套东西的主接口。内核的随机流故意和游戏不一致（不变量 4），
/// 所以"下一手是哪一个"预测不了；能给的、也是唯一诚实的东西就是这个集合。
/// `verify --predict-enemy` 判的是**成员资格**，不是逐字相等。
pub fn allowed_next(s: &State, e: usize) -> u32 {
    let def = enemy_def(s.enemy_def[e]);
    let n = def.moves.len().max(1);
    match def.machine {
        // 固定循环：集合是单元素，和改造之前的行为完全一样
        None => 1u32 << (move_index(def, s.enemy_move[e].wrapping_add(1) as u32) % n),
        Some(m) => {
            let cur = current_move_ix(def, s, e);
            let h = eff_hist(s, e, cur as u8);
            let u = eff_used(s, e, cur as u8);
            resolve_set(s, e, &h, u, m.after.get(cur).copied().unwrap_or(Next::Go(0)))
        }
    }
}

fn resolve_set(s: &State, e: usize, h: &[u8; ENEMY_HIST], used: u32, n: Next) -> u32 {
    match n {
        Next::Go(i) => 1u32 << i,
        Next::Rand(bs) => {
            let mut mask = 0u32;
            for b in bs {
                if branch_open(h, used, b) {
                    mask |= 1 << b.to;
                }
            }
            // 全关了说明数据写错了（源码那边直接抛异常）。内核不 panic
            //（不变量 5），退回"全都可能" —— 弱，但不会自信地错。
            if mask == 0 {
                bs.iter().fold(0, |a, b| a | 1 << b.to)
            } else {
                mask
            }
        }
        Next::Cond(cs) => {
            let mut mask = 0u32;
            for (c, to) in cs {
                match eval_cond(s, e, used, *c) {
                    // 第一个**确定成立**的就是答案，后面的不看
                    Some(true) => return 1 << to,
                    Some(false) => {}
                    // 判不出来 ⇒ 这一支也可能成立，收进集合继续看
                    None => mask |= 1 << to,
                }
            }
            if mask == 0 {
                cs.iter().fold(0, |a, (_, t)| a | 1 << t)
            } else {
                mask
            }
        }
    }
}

/// 按权重采样下一手。**只给 rollout 用**；对拍那条路走 [`allowed_next`]，
/// 不采样。用 `rng.enemy` 流（不变量 4 的三条分流之一）。
fn pick_next(s: &mut State, e: usize) -> u8 {
    let def = enemy_def(s.enemy_def[e]);
    let Some(m) = def.machine else {
        return s.enemy_move[e].wrapping_add(1);
    };
    let cur = current_move_ix(def, s, e);
    let h = eff_hist(s, e, cur as u8);
    let u = eff_used(s, e, cur as u8);
    let n = m.after.get(cur).copied().unwrap_or(Next::Go(0));
    match n {
        Next::Go(i) => i,
        // 条件分支不掷骰：取集合最低位。大于 1 位只会发生在"内核判不出条件"
        // 的时候，那时**任选一个都是猜** —— 取最低位至少是确定性的，
        // 不会让同一个局面每次 rollout 出不同结果。
        Next::Cond(_) => resolve_set(s, e, &h, u, n).trailing_zeros() as u8,
        Next::Rand(bs) => {
            let mut total = 0u32;
            for b in bs {
                if branch_open(&h, u, b) {
                    total += b.weight as u32;
                }
            }
            if total == 0 {
                return bs.first().map(|b| b.to).unwrap_or(0);
            }
            let mut r = crate::state::next_below(&mut s.rng.enemy, total as usize) as u32;
            for b in bs {
                if !branch_open(&h, u, b) {
                    continue;
                }
                let w = b.weight as u32;
                if r < w {
                    return b.to;
                }
                r -= w;
            }
            bs[0].to
        }
    }
}

/// **开局第一手**由机器的 `start` 决定，不是写死的 0。
///
/// 这一条不是锦上添花：树枝史莱姆（中）源码的 initialState 就是「吐黏液」
/// 而不是那个随机分支；飞蝇菌子的开局池也比之后的小（易伤孢子不在里面）。
/// 不接这一条，固定循环表和机器在第一手上就是一样的，那这套东西白做了一半。
///
/// **代表元逐只挑、而约束可以是整场的**：几只同类由遭遇的一个随机数错开起手时
/// （残杀千足虫三节两两不同），逐只取最低位会让它们全从同一手起、整场同相。
/// 那种开局写成 `ECond::SlotRep`：集合不变，这里按槽位挑（2026-09-15）。
pub fn initial_move(s: &State, e: usize) -> u8 {
    let def = enemy_def(s.enemy_def[e]);
    let Some(m) = def.machine else { return 0 };
    let empty = [u8::MAX; ENEMY_HIST];
    match m.start {
        Next::Go(i) => i,
        // 开局的随机分支：内核**不掷骰**，因为对拍那条路要的是集合不是样本。
        // rollout 想采样的话走 `pick_next`，那时已经有当前手了。
        // 这里取集合里的一个确定性代表元：有槽位提示且它在集合里就取它，否则取最低位。
        // 「在集合里」这一条让确定成立的分支优先 —— 那时集合只剩那一支。
        n => {
            let set = resolve_set(s, e, &empty, 0, n);
            if let Next::Cond(cs) = n {
                let hinted = cs.iter().find(|(c, to)| {
                    matches!(c, ECond::SlotRep(k) if *k as usize == e) && set & (1u32 << to) != 0
                });
                if let Some(&(_, to)) = hinted {
                    return to;
                }
            }
            set.trailing_zeros() as u8
        }
    }
}

/// **这一手在当前局面下，出招机器走不走得到。** 给实况对齐挑签名歧义用
/// （`Replayer::identify_enemies`）：两手签名逐字相同时，排掉「进它的边全是条件边、
/// 而那些条件在当前局面下都确定不成立」的那一手。
///
/// 今天唯一的歧义是蜂群术士的喷射信息素（按蜂房层数拆成两手，都是 `Buff`）。
///
/// **只会说「走不到」，不会说「一定是它」**：
/// * 没机器（定环）、有 `Go` / `Rand` 边进它、或者**没有任何边进它**（只能被强制打进去的
///   击晕 / 醒来那一类）⇒ 一律 `true`
/// * 条件边判不出来（`ECond::Unknown`）⇒ 算走得到
///
/// 对齐那一刻没有出招记录，所以 `Repeat` / `cooldown` 不看、`used` 当 0。
pub fn move_reachable_now(s: &State, e: usize, def_id: u16, m: usize) -> bool {
    let def = enemy_def(def_id);
    let Some(mc) = def.machine else { return true };
    let empty = [u8::MAX; ENEMY_HIST];
    let mut saw_cond = false;
    for n in std::iter::once(mc.start).chain(mc.after.iter().copied()) {
        match n {
            Next::Go(i) if i as usize == m => return true,
            Next::Rand(bs) if bs.iter().any(|b| b.to as usize == m) => return true,
            Next::Cond(cs) if cs.iter().any(|(_, to)| *to as usize == m) => {
                saw_cond = true;
                if resolve_set(s, e, &empty, 0, n) & (1u32 << m) != 0 {
                    return true;
                }
            }
            _ => {}
        }
    }
    !saw_cond
}

/// **开局允许集合**。和 [`allowed_next`] 分开是因为问的是两件事：
/// 那个问"这一手之后能接什么"，这个问"第一手可能是什么"。
pub fn allowed_initial(s: &State, e: usize) -> u32 {
    let def = enemy_def(s.enemy_def[e]);
    let n = def.moves.len().max(1);
    match def.machine {
        None => 1,
        Some(m) => resolve_set(s, e, &[u8::MAX; ENEMY_HIST], 0, m.start) & ((1u32 << n) - 1),
    }
}

/// 出招指针推进。**两条敌人回合的路径都走它** —— `enemy_turn` 和
/// `end_turn_with_incoming`。漏掉一条，机器就只在一半的路径上生效。
/// 敌人回合开头：这只被击晕了吗？被击晕就**跳过这一手**并把出招指针退回去。
///
/// 返回 `true` = 这一手跳过（调用方要 `continue`，而且**不要**调 `advance_move`）。
///
/// [源码] `Creature.StunInternal` 的 `FollowUpStateId = stateLog.Last().Id` ——
/// 读的是**已经打出过**的那一手，不是被顶掉的那一手。所以醒来之后它把
/// **上一手再打一遍**，而本来要出的那一手是**丢了**。
///
/// 两种出招表示要分开退（`current_move_ix` 对它们的解释不一样）：
/// * 有机器：`enemy_move` 存的是**下标** ⇒ 退回 `enemy_hist[0]`（上一次打出的那一手）
/// * 没机器（定环）：`enemy_move` 存的是**次数** ⇒ 退 1（上一次打出的就是 cur-1）
///
/// **还没出过手就被击晕**时没有"上一手"可回（源码那边 `stateLog` 里是初始状态）。
/// 这种情况保持指针不动 —— 语料里一个样本都没有，**不猜**，
/// 而"保持不动"至少不会凭空跳过一手。这条标在 `stun_before_any_move_keeps_the_pointer`。
fn take_stun_if_stunned(s: &mut State, e: usize) -> bool {
    if s.enemies[e].get(St::Stunned) == 0 {
        return false;
    }
    s.enemies[e].set(St::Stunned, 0);
    let def = enemy_def(s.enemy_def[e]);
    if def.machine.is_some() {
        // 有历史才退；没有就保持不动（见上面那段）
        if s.enemy_used[e] != 0 {
            s.enemy_move[e] = s.enemy_hist[e][0];
        }
    } else if s.enemy_move[e] > 0 {
        s.enemy_move[e] -= 1;
    }
    true
}

fn advance_move(s: &mut State, e: usize) {
    let def = enemy_def(s.enemy_def[e]);
    let cur = current_move_ix(def, s, e) as u8;
    if def.machine.is_some() {
        // **先挑再记**：`pick_next` 内部自己把当前手前置进历史（`eff_hist`），
        // 这里先 push 会把它数两遍。
        let nxt = pick_next(s, e);
        push_hist(s, e, cur);
        s.enemy_used[e] |= bit(cur);
        s.enemy_move[e] = nxt;
    } else {
        s.enemy_move[e] = s.enemy_move[e].wrapping_add(1);
    }
}

/// 知识的诅咒落地：第 k 次（k 从我身上的诅咒 status 反推）按 `curse_policy` 第 k 位挑一边。
///
/// **不是 `Pending`**：[源码] 这个选牌屏开在敌人回合中间，而 `step(EndTurn)` 是原子的 ——
/// 让它停在半路等玩家选，planner 的回合边界拆分和 rollout 的闭环都得改。
/// 这个选择的价值全在后面几个回合，单回合求解器本来也定不了价，所以做成**策略参数**：
/// L3 把 8 种选法配对比，实战里由玩家选。
///
/// 四个诅咒都是 debuff，人工制品挡得住；挡掉之后内核反推不出这一次（见 `content::curses_taken`）。
fn apply_curse_choice(s: &mut State, sets: &[crate::ops::CurseSet]) {
    let k = crate::content::curses_taken(&s.player, sets);
    let Some(set) = sets.get(k) else { return };
    if artifact_absorbs(&mut s.player, true) {
        return;
    }
    if s.curse_policy & (1u8 << k) != 0 {
        s.player.add(St::Disintegration, set.disintegration);
    } else {
        s.player.add(set.other, set.other_amt);
        s.base_energy += set.other_max_energy;
    }
}

fn enemy_turn(s: &mut State) {
    begin_enemy_turn(s);
    // 敌人整边开始（[源码] `AfterSideTurnStart(CombatSide.Enemy)`）。
    // 唯一的消费者是敌人持有的覆甲：在这里掉 1 层，回合末再按掉完的层数给格挡。
    // 和 `EnemyTurnEnd` 一样放在循环**外面** —— 它是 side 级钩子，
    // 放进循环会变成每只敌人各触发一次全场。
    fire(s, Hook::EnemyTurnStart, 0);
    for e in 0..s.n_enemies as usize {
        if !s.enemies[e].alive() {
            continue;
        }
        // 被击晕 ⇒ 这一手什么都不做，**而且不 advance_move**（指针已经退过了）
        if take_stun_if_stunned(s, e) {
            continue;
        }
        let def = enemy_def(s.enemy_def[e]);
        let mi = current_move_ix(def, s, e);
        let mv = &def.moves[mi];
        // 出招之前的指针。出到一半被改了招（反伤打死瀑布巨兽）就不推进，见循环末尾。
        let ptr = s.enemy_move[e];
        // **进阶收口点之一。** A8 抬血量/格挡、A9 抬伤害/段数，`asc < 8` 时
        // `adjust` 逐字返回原 op（一次表都不查），所以 A1/A2 的语料逐字节不变。
        // 读 `EnemyMove::ops` 的地方一共八处，全部要走这一层 ——
        // 漏一处的表现是那条路径**静默地**按低进阶算。
        //
        // `landed`：这一手里上一条 `PlayerStatus` 有没有真的落地（没被人工制品吃掉）。
        // 唯一的读者是 `EOp::DowngradeUpgradedCards`（抑制的 `AfterApplied`）。
        let mut landed = false;
        for (oi, op) in mv.ops.iter().enumerate() {
            let op = crate::asc::adjust(s.enemy_def[e], mi, oi, s.ascension, *op);
            match op {
                EOp::Attack { base, hits } => {
                    // **敌人也吃自己的活力。** 骇鳗的乱舞打完给自己 6 活力，
                    // 下一手撞击就是 16+6=22 —— 不读它，内核会系统性**低估**
                    // 这只怪的伤害（方向是乐观的，最危险的那一边）。
                    // 规矩和玩家侧逐字相同（`spend_vigor` 那一套）：
                    // **多段的每一段都吃满加值，整条攻击命令打完清零**。
                    let face =
                        base + s.enemies[e].get(St::Strength) + s.enemies[e].get(St::Vigor);
                    for _ in 0..hits {
                        // 攻击者可能在多段攻击**中途**死掉（火焰屏障的反伤），
                        // 死了就不该把剩下的段数打完。这个 bug 是
                        // `flame_barrier_thorns_every_hit_of_a_multi_hit_attack`
                        // 逼出来的 —— 反伤在第 6 段杀死追踪手，它却还打了 2 段。
                        if !s.enemies[e].alive() {
                            break;
                        }
                        let d = apply_modifiers(face, &s.enemies[e], &s.player);
                        take_attack_hit(s, e, d);
                    }
                    s.enemies[e].set(St::Vigor, 0);
                }
                // 敌人也走收口。势不可当只挂在玩家身上，但收口本身要完整 ——
                // 「有一条路径没走收口」正是这类触发器静默失效的唯一原因。
                // 敌人招式的格挡吃它自己的敏捷（遗忘之物），见 `damage::monster_block`。
                EOp::Block(n) => {
                    let b = crate::damage::monster_block(n, &s.enemies[e]);
                    gain_block(s, Owner::Enemy(e), b, 0)
                }
                EOp::SelfStatus { st, amt } => s.enemies[e].add(st, amt),
                // 巨斧机器人的启动：力量 = 6 − 3×库存（[源码] `3 * (2 - StockAmount)`）
                EOp::SelfStatusPerStack { st, amt, per } => {
                    let k = s.enemies[e].get(per);
                    s.enemies[e].add(st, amt * k);
                }
                // 连环爪击：段数 = hits + 自己那个计数器的层数。
                // **和上面 `EOp::Attack` 走同一条 `take_attack_hit`**，
                // 所以多段中途被反伤打死、格挡逐次吸收这些全都不会长歪。
                EOp::AttackPlusStackHits { base, hits, per } => {
                    let face =
                        base + s.enemies[e].get(St::Strength) + s.enemies[e].get(St::Vigor);
                    let n = hits + s.enemies[e].get(per);
                    for _ in 0..n {
                        if !s.enemies[e].alive() {
                            break;
                        }
                        let d = apply_modifiers(face, &s.enemies[e], &s.player);
                        take_attack_hit(s, e, d);
                    }
                    s.enemies[e].set(St::Vigor, 0);
                }
                // 恐惧：每段基础伤害吃自己那个 status 的层数（遗忘之物的敏捷）。
                // 其余逐字同 `EOp::Attack`（活力、多段中途被反伤打死就停）。
                EOp::AttackPlusSelfStatus { base, hits, per } => {
                    let face = base
                        + s.enemies[e].get(per)
                        + s.enemies[e].get(St::Strength)
                        + s.enemies[e].get(St::Vigor);
                    for _ in 0..hits {
                        if !s.enemies[e].alive() {
                            break;
                        }
                        let d = apply_modifiers(face, &s.enemies[e], &s.player);
                        take_attack_hit(s, e, d);
                    }
                    s.enemies[e].set(St::Vigor, 0);
                }
                // 减力量/减敏捷也吃人工制品（失落之物/遗忘之物偷属性），见 `is_debuff_amount`。
                EOp::PlayerStatus { st, amt } => {
                    landed = !artifact_absorbs(&mut s.player, crate::damage::is_debuff_amount(st, amt));
                    if landed {
                        s.player.add(st, amt);
                    }
                }
                // 思考的回血，封顶在最大生命。
                EOp::Heal(n) => {
                    let en = &mut s.enemies[e];
                    en.hp = (en.hp.max(0) + n).min(en.max_hp);
                }
                // 知识的诅咒：选哪边照 `curse_policy`，见 `apply_curse_choice`。
                EOp::CurseOfKnowledge(sets) => apply_curse_choice(s, sets),
                // 抑制：挂上的那一刻把本场所有已升级的牌降级、记下是谁（`F_DAMPENED`）。
                // **只有上一条 `PlayerStatus` 真的落地才发作**，理由见 `EOp::DowngradeUpgradedCards`。
                EOp::DowngradeUpgradedCards => {
                    if landed {
                        for i in 0..s.n_cards as usize {
                            let c = &mut s.cards[i];
                            if c.flags & F_UPGRADED != 0 {
                                c.flags = (c.flags & !F_UPGRADED) | crate::state::F_DAMPENED;
                            }
                        }
                    }
                }
                // 召唤。优先复用死掉的槽位，否则往后加 —— 不这么做的话
                // 反复召唤会把 MAX_ENEMIES 撑爆，然后静默地不再召唤。
                EOp::Summon { def, hp } => {
                    summon_one(s, def, hp);
                }
                // 偷牌：抽牌堆 + 弃牌堆里随机拿走 n 张，**不进消耗堆**（见 `EOp::StealCard`）。
            // 用 `rng.gen` 流：敌人偷哪一张不该影响我的抽牌序列（不变量 4 的分流）。
            EOp::StealCard(n) => {
                for _ in 0..n {
                    let total = s.n_draw as usize + s.n_disc as usize;
                    if total == 0 {
                        break;
                    }
                    let k = crate::state::next_below(&mut s.rng.gen, total);
                    if k < s.n_draw as usize {
                        // 抽牌堆里那一张：**已知前缀**只保到被拿走的那一张之上
                        let above = s.n_draw as usize - 1 - k;
                        s.draw.copy_within(k + 1..s.n_draw as usize, k);
                        s.n_draw -= 1;
                        if (s.n_draw_known as usize) > above {
                            s.n_draw_known = above as u8;
                        }
                    } else {
                        let k = k - s.n_draw as usize;
                        s.disc.copy_within(k + 1..s.n_disc as usize, k);
                        s.n_disc -= 1;
                    }
                }
            }
            EOp::AddCardToDiscard { card, count } => {
                    for _ in 0..count {
                        let Some(ix) = spawn_card(s, card) else { break };
                        s.to_discard(ix);
                    }
                }
                // 和 `AddCardToDiscard` 分开：进抽牌堆的那张**这一局就可能抽到**。
                // [源码] `Noisebot.NoiseMove` 两张分别去 Discard 和 Draw(Random)。
                EOp::AddCardToDraw { card, count } => {
                    for _ in 0..count {
                        let Some(ix) = spawn_card(s, card) else { break };
                        s.to_draw_random(ix);
                    }
                }
                // 火焰喷射：塞手牌，满了溢出进弃牌堆（见 `add_generated_to_hand`）。
                EOp::AddCardToHand { card, count } => {
                    for _ in 0..count {
                        if !add_generated_to_hand(s, card) {
                            break;
                        }
                    }
                }
                // 剧烈增强：把**本场所有**凋萎各 +3 伤害（[源码] `FakeUpgrade`
                // 逐张调 `UpgradeValueBy(3m)`）。扫的是 `s.cards` 也就是本场
                // 全部实例，对应 [源码] 的 `PlayerCombatState.AllCards`
                // —— **不只手牌**，弃牌堆和抽牌堆里的也要涨。
                EOp::UpgradeAllCopies { card, by } => {
                    for i in 0..s.n_cards as usize {
                        if s.cards[i].id == card {
                            s.cards[i].bonus = s.cards[i].bonus.saturating_add(by as i16);
                        }
                    }
                }
                // Guardbot：给场上每一只**非爪牙**敌人加格挡（不是给自己）。
                // 走 `gain_block` 收口 —— 和其它加格挡的路径一样，
                // 漏掉收口就是一个静默失效的触发器。
                EOp::BlockNonMinions(n) => {
                    for t in 0..s.n_enemies as usize {
                        if s.enemies[t].alive() && s.enemies[t].get(St::Minion) == 0 {
                            gain_block(s, Owner::Enemy(t), n, 0);
                        }
                    }
                }
                // 哀嚎：给自己这一侧每一只**活着的**敌人加层数，**含它自己**。
                // 活着的判据和 `BlockNonMinions` 同一条 —— 正在复苏的幻象
                // 血量是 0，游戏侧 `ShouldAllowHitting` 也不让它这时候收 power。
                EOp::TeamStatus { st, amt } => {
                    for t in 0..s.n_enemies as usize {
                        if s.enemies[t].alive() {
                            s.enemies[t].add(st, amt);
                        }
                    }
                }
                EOp::ClearSelfStatus(st) => s.enemies[e].set(st, 0),
                // 自杀（瀑布巨兽的爆炸）。和被打死一样点两条边，见 `hit_enemy_with`。
                EOp::KillSelf => {
                    s.enemies[e].hp = 0;
                    fire_ctx(s, Hook::EnemyDied, 0, e);
                    fire_ctx(s, Hook::AllyDied, 0, e);
                }
                EOp::Nothing => {}
            }
        }
        // **这一手出到一半被改了招 ⇒ 指针不推进。** 来路是它自己出招时被反伤打死、死亡规则把它
        // 强制到下一手（瀑布巨兽 -> 即将爆发）。[源码] 那一手带 `MustPerformOnceBeforeTransitioning`：
        // 推进的话「即将爆发」被跳过、直接爆炸。我方回合里改的招不走这条 —— 那时指针在出招**之前**
        // 就改好了，`ptr` 读到的已经是新的。
        if s.enemy_move[e] == ptr {
            advance_move(s, e);
        }
        if s.player.hp <= 0 {
            break;
        }
    }
    // 敌人整边行动完之后（[源码] `AfterSideTurnEnd(CombatSide.Enemy)`）。
    // 唯一的消费者是高压：电击机器人每个敌人回合给自己 +2 力量。
    //
    // 放在循环**外面**是照抄源码的 side 级钩子 —— 放进循环里会变成
    // "每只敌人行动完各触发一次全场"，多只电击机器人时会成倍加力量。
    if s.player.hp > 0 {
        // 三档依次发（[源码] `Hook.BeforeSideTurnEnd` 里 VeryEarly -> Early，
        // 之后才是 `AfterSideTurnEnd`）：
        //   最早一档：**沉睡**在最后一个睡眠回合把覆甲摘掉 —— 族母因此拿不到那一回合的墙
        //   早一档  ：敌人的**覆甲**给格挡
        //   晚一档  ：**熟睡**减层、醒来移除覆甲
        // 顺序写在钩子里，不靠表内先后。
        fire(s, Hook::EnemyTurnEndVeryEarly, 0);
        fire(s, Hook::EnemyTurnEndEarly, 0);
        fire(s, Hook::EnemyTurnEnd, 0);
    }
    // 「本回合被强制改招」到这里用完了（`St::MoveForcedThisTurn` 的文档：敌人整边行动完之后清）。
    // 注入路径在读到它的那一刻清；**这条完整路径 2026-09-14 之前一处都不清** —— 我方回合里被强制
    // 换过招的敌人（耕地 / 尖叫 / 钻地 / 被打醒的熟睡甲虫）从此带着它，之后每个回合求解器叶子上的
    // 注入威胁都把那只敌人这一手跳过（乐观）。建熟睡甲虫时发现的。
    for e in 0..s.n_enemies as usize {
        s.enemies[e].set(St::MoveForcedThisTurn, 0);
    }
}

// ---------------------------------------------------------------------------
// 规则修饰（rule modifiers）
// ---------------------------------------------------------------------------
//
// 和触发器**不是一回事**：触发器只能"在某个时点多做一件事"，表达不了
// "不要做某件事"。壁垒（格挡不清零）、均衡（不弃手牌）、孤注一掷（挨了
// 未格挡的伤害就死）都属于后者，只能在规则本身那几行上开口子。
//
// 目前就这三张，各自一个窄 `if`，全部集中注释在这里。**如果这类牌多起来，
// 该给它们一张像 `POWERS` 那样的表**，而不是继续往 `step.rs` 里塞 `if`。

/// 我的回合开始，**但不抽牌**。
///
/// 拆出来是给跨回合 planner 用的：机会节点（这一手抽到什么）必须插在
/// 「敌人打完、回合开始结算做完」和「抽牌」之间，而 `step(EndTurn)` 把这
/// 一整段做成了一个原子动作。
///
/// **拆的方式是"切开"不是"抄一份"**：`start_player_turn` 现在就是
/// 这个函数加一句 `open_hand`，两条路径不可能长歪。
/// 和当初为 L2 加 `end_turn_with_incoming` 是同一个先例。
fn start_player_turn_before_draw(s: &mut State) {
    // 我这一边的回合开始，**最早一档**（[源码] `BeforeSideTurnStart`）：
    // 清格挡、能量回满、`TurnStart`（水银沙漏 / 滚石打敌人）全在它后面。见 `Hook::SideTurnStart`。
    fire(s, Hook::SideTurnStart, 0);
    s.turn += 1;
    // 壁垒：「格挡不再在你的回合开始时消失。」
    if s.player.get(St::Barricade) == 0 {
        s.player.block = 0;
    }
    s.energy = s.base_energy;
    s.cards_played = 0;
    s.attacks_played = 0;
    s.skills_played = 0;
    s.exhausted_this_turn = 0;
    s.hp_lost_this_turn = 0;
    s.free_attack = 0;
    // 缓慢 accumulates within a turn only
    //
    // 敌人的格挡**不在这里清**：格挡在其拥有者的回合开始时才消失，敌人挡下的
    // 格挡要一直留到我这一整个回合，否则敌人的 Defend 意图等于没有效果。
    // 实测（2026-08-15 第1幕第7层）：小啃兽上回合 `Attack:6, Defend:`，
    // 到我的回合开始时它仍带着 5 点格挡。清零放在 `enemy_turn` 开头。
    for e in 0..s.n_enemies as usize {
        s.enemies[e].set(St::Slow, 0);
    }
    // 「这个回合」限定的能力（狂怒/火焰屏障）到这里失效。
    //
    // 清在**我的下一个回合开始**而不是上个回合结束：火焰屏障要挡的正是
    // 敌人回合那几下，回合结束就清等于这张牌完全没用。
    for st in TURN_SCOPED {
        s.player.set(*st, 0);
        for e in 0..s.n_enemies as usize {
            s.enemies[e].set(*st, 0);
        }
    }
    // 回合开始的能力（恶魔形态/薪火之源/绯红披风/滚石）。
    // 位置要紧：**能量已经回满之后**（薪火之源是加在满能量上的），
    // **抽牌之前**（绯红披风的格挡在抽牌前就该到位）。
    fire(s, Hook::TurnStart, 0);
}

/// 抽开局手牌（5 张）。**跨回合 planner 的机会节点就插在它前面。**
///
/// 那个 5 是基础张数。今天唯一**减**它的是心灵腐化（知识恶魔的诅咒）；
/// 加它的佩尔之血挂在 `TurnStart` 上先抽。清晰/稳定血清那类药水没建，
/// 理由在 `ops.rs::POTIONS` 表头 —— 将来有了，改这一处。
pub fn open_hand(s: &mut State) {
    // 心灵腐化（知识恶魔的诅咒，[源码] `MindRotPower.ModifyHandDraw => max(0, count − Amount)`）。
    // 佩尔之血那 +1 挂在 `TurnStart` 上先抽了，这里只减基础那 5 张 ——
    // 合起来和源码的 `max(0, 5 + 1 − Amount)` 相同（`Amount` 只来自一次诅咒，1 层）。
    s.draw_n((5 - s.player.get(St::MindRot)).max(0) as _);
    // 手牌发下来之后才发作的那几件（风箱/骨茶升级手牌）。**必须在这里，
    // 不能挂 `TurnStart`** —— 那个钩子在抽牌之前，升级的是一手空牌。
    fire(s, Hook::HandDrawn, 0);
    // 绯红披风/滚石 可能在这里就把人打死或把敌人打死
    check_over(s);
}

fn start_player_turn(s: &mut State) {
    start_player_turn_before_draw(s);
    open_hand(s);
}

/// 敌人这一整个回合会打进来的伤害：每个槽位 `(每次伤害, 次数)`，
/// **已经是过完所有乘区的最终值**。
///
/// 这正是观测到的意图标签的语义（实测：攻击方力量和防御方易伤都算在里面了，
/// 见 `docs/trace-format.md` 约束 4），所以实战驱动时可以逐字照抄。
pub type Incoming = [(i32, i32); MAX_ENEMIES];

pub const NO_INCOMING: Incoming = [(0, 0); MAX_ENEMIES];

/// 敌人回合的**注入版**：打多少由调用方给定，内核只负责结算。
///
/// 和 [`enemy_turn`] 共用 [`take_attack_hit`]，所以格挡逐次吸收、孤注一掷、
/// 火焰屏障、`hp_lost_this_turn` 记账全都是同一条路。
///
/// **死掉的敌人不出手** —— 这条正是单回合求解的核心价值：这回合把谁打死了，
/// 它那份伤害就不会落下来。
fn injected_enemy_turn(s: &mut State, inc: &Incoming, live: bool) {
    begin_enemy_turn(s);
    // **注入路径也要发这个钩子。** 它管的是"敌人整边开始时发生的事"，
    // 和"这一手打多少"是两件事 —— 注入的只有后者。
    //
    // 漏掉它的代价是**结构性**的：沙坑的即死倒计时挂在这里，而 L2 走的
    // 正是这条注入路径 ⇒ 求解器**永远看不见自己会被吞掉**。
    // 2026-09-05 第 2 幕 Boss 那次 AI 驾驶就死在这上面：第 8 回合沙坑归零，
    // 而求解器全程把狂乱逃离评成"0 伤害 0 格挡的废牌"。
    //
    // 顺带把敌人持有的覆甲掉层和实验体的复苏也接上了 —— 两条本来就该在这里。
    fire(s, Hook::EnemyTurnStart, 0);
    if s.combat_over {
        return;
    }
    for e in 0..s.n_enemies as usize {
        if !s.enemies[e].alive() {
            continue;
        }
        // **注入路径也要认击晕。** 注入的伤害来自"同步那一刻观测到的意图"，
        // 而击晕是我这一回合打出去之后才发生的 —— 不在这里拦一道，
        // 求解器就会以为吹哨白打了 3 费（两种威胁口径都受影响）。
        // 这和凌虐减力量、给敌人上虚弱是同一类：**这一回合的来袭跟着我打的牌变**。
        if take_stun_if_stunned(s, e) {
            continue;
        }
        // **我这一回合改过它的招 ⇒ 注入的那个标签过期了。**
        // 注入值来自同步那一刻的意图；实验体被砍死之后换成「复苏」、
        // 仪式兽过闸门被换成眩晕 —— 两处换上去的都是不打人的招，
        // 照着旧标签打就是凭空多挨一手。
        //
        // 和上面那条击晕是同一类（"这一回合的来袭跟着我打的牌变"），
        // 只是击晕能从状态看出来，改指针只能靠这个标记。
        // **这里只跳过伤害，指针照常推进** —— 它确实出了那一手（复苏/眩晕）。
        if s.enemies[e].get(St::MoveForcedThisTurn) > 0 {
            s.enemies[e].set(St::MoveForcedThisTurn, 0);
            advance_move(s, e);
            continue;
        }
        let (base, hits) = inc[e];
        let ptr = s.enemy_move[e];
        for _ in 0..hits {
            // 攻击者可能在多段攻击中途被反伤打死，和 `enemy_turn` 同一条判断
            if !s.enemies[e].alive() || s.player.hp <= 0 {
                break;
            }
            // `live` = 传进来的是**面板基础值**，要在结算这一刻过一遍伤害管线；
            // 否则就是**最终值**（观测到的意图标签），照打。两条路的区别见
            // `end_turn_with_live_incoming` 的文档。
            let d = if live {
                let face = base + s.enemies[e].get(St::Strength);
                crate::damage::apply_modifiers(face, &s.enemies[e], &s.player)
            } else {
                base
            };
            take_attack_hit(s, e, d);
        }
        // 出招指针照常推进：跨回合 rollout 换成 `EnemyDef` 预测威胁时，
        // 下一回合要接着这个指针往下猜。
        // **挨打途中被改了招就不推进**（反伤打死瀑布巨兽），理由同 `enemy_turn`。那个
        // `MoveForcedThisTurn` 留着：下一个注入回合读到它，跳过伤害再推进 —— 正好是「即将爆发」不打人。
        if s.enemy_move[e] == ptr {
            advance_move(s, e);
        }
        if s.player.hp <= 0 {
            break;
        }
    }
}

/// 结束回合，敌人这一手的伤害由**调用方**给定（见 [`Incoming`]）。
///
/// 存在的理由：L2 求解器不许自己实现游戏规则，但它必须知道"这条线打完之后
/// 挨完打还剩多少血"。把威胁做成输入、把结算留在 L1，两边都不用妥协 ——
/// 换威胁的提供者（观测意图 / `EnemyDef` 预测）不影响这个函数。
pub fn end_turn_with_incoming(s: State, inc: &Incoming) -> State {
    if s.combat_over {
        return s;
    }
    end_turn_impl(s, Some((inc, false)), true)
}

/// 同 [`end_turn_with_incoming`]，但传进来的是**面板基础值**，每一击的伤害在
/// **结算那一刻**现算（`base + 当前力量`，过当前所有乘区）。
///
/// # 为什么要有这条路
///
/// `Threat` 原来只能装"观测到的意图标签"，而那是**同步那一刻就冻住的常数**。
/// 后果是：凡是"我这一手会改敌人这一击打多少"的东西，L2 一概看不见 ——
/// 给敌人上虚弱、凌虐减 10 力量、巨像减半、我自己吃污染、转身改朝向，
/// 全都是空操作。2026-08-27 第 2 幕 Boss 那一场，求解器因此报「这样打仍然会死」，
/// 而实际存在一条把 36 点来袭压到 24 的活线（虚弱药水 + 转身 + 敏捷加成）。
///
/// 现算之后这些**自动**都算进去了 —— 因为它们本来就都在 `apply_modifiers` 里。
///
/// # 两条路都要留着
///
/// * **对拍必须用冻住的那条**：`verify` 的契约是"敌人打多少由观测注入"，
///   现算等于让内核自己预测敌人，那是"内核和内核比"。
/// * **敌人认不出来时也只能用冻住的那条**：不知道它这一手的面板基础值，
///   就没有东西可以重算。`Threat::set` 和 `Threat::set_live` 分别对应两种情况。
pub fn end_turn_with_live_incoming(s: State, base: &Incoming) -> State {
    if s.combat_over {
        return s;
    }
    end_turn_impl(s, Some((base, true)), true)
}

fn end_turn(s: State) -> State {
    end_turn_impl(s, None, true)
}

/// 结束回合，但**停在抽牌之前**。跨回合 planner 用它给机会节点让出位置：
/// 拿到的局面是「敌人打完、回合开始的结算都做完、手牌还是空的」。
///
/// 抽牌自己调 [`open_hand`]。**`end_turn_before_draw` + `open_hand`
/// 必须逐字节等于 `step(EndTurn)`**，`end_turn_split_equals_the_atomic_one`
/// 钉着这条 —— 拆出来的东西一旦和原路径分岔，planner 就在另一个游戏里搜。
pub fn end_turn_before_draw(s: State) -> State {
    end_turn_impl(s, None, false)
}

/// 回合结束时，消耗堆里那些「在消耗堆里就把自己打出来」的牌（彼岸咆哮）。
///
/// **先快照再打**：打出去之后它自带消耗，当场又回到消耗堆 ——
/// 边扫边改的话同一张牌这一个回合会被打出无数次（不是变慢，是挂死）。
fn autoplay_from_exhaust(s: &mut State) {
    let mut queue = [0u8; MAX_CARDS];
    let mut n = 0usize;
    for i in 0..s.n_exh as usize {
        if crate::content::EXHAUST_END_AUTOPLAY.contains(&s.cards[s.exh[i] as usize].id) {
            queue[n] = s.exh[i];
            n += 1;
        }
    }
    for &cix in &queue[..n] {
        if s.combat_over {
            return;
        }
        // 现在的位置要重找 —— 前一张打出去已经动过消耗堆了。
        let Some(i) = (0..s.n_exh as usize).find(|&i| s.exh[i] == cix) else { continue };
        s.take_from_exh(i);
        auto_play_card(s, cix, false, 0);
    }
}

fn end_turn_impl(mut s: State, inc: Option<(&Incoming, bool)>, open: bool) -> State {
    // 回合末的**自动打出**阶段。[源码] `CombatManager.EndPlayerTurnPhaseOneInternal`
    // 把 `AutoPostPlay` 排在 `Hook.BeforeTurnEnd` 和弃手牌**之前**，所以它在最前面。
    autoplay_from_exhaust(&mut s);
    check_over(&mut s);
    if s.combat_over {
        return s;
    }
    // **两段式回合结束。** 第一段只做快照（奥利哈钢记下"这时候还没有格挡"），
    // 第二段才结算 —— 因为覆甲在第二段给格挡，压成一段的话奥利哈钢永远不触发。
    // 顺序写在钩子的类型里，不靠 `POWERS` 表里的先后。
    fire(&mut s, Hook::TurnEndVeryEarly, 0);
    fire(&mut s, Hook::TurnEnd, 0);
    // 手牌里那些留着就发作的牌（灼伤/腐朽/羞耻/感染/瓦解/虚无）
    resolve_hand_end(&mut s);
    check_over(&mut s);
    if s.combat_over {
        return s;
    }
    // 均衡：「在本回合保留你的手牌。」它是 TURN_SCOPED，所以只保这一回合 ——
    // 下个回合开始时清掉，再下次结束回合就正常弃牌了。
    if s.player.get(St::Entrench) == 0 {
        // **保留（`F_RETAIN`）的牌留在手上。** 今天唯一的来源是附魔
        // （王室认证 / 沉稳），见 `content::ENCHANTS`。
        //
        // [实测] 2026-09-06 `act3_f46_elite_soul_nexus` 两个回合边界：
        // 带王室认证的均衡+ 留下、同一手的添柴+ 被弃掉。
        //
        // 逐个走而不是 `while n_hand > 0`：留下的那几张要**保持在手上**，
        // 所以下标只在留下时才往前走。
        let mut i = 0;
        while i < s.n_hand as usize {
            if s.cards[s.hand[i] as usize].flags & F_RETAIN != 0 {
                i += 1;
                continue;
            }
            let c = s.take_from_hand(i);
            s.to_discard(c);
        }
    }
    // 回合末**最后一步**（[源码] `AfterSideTurnEndLate`，第二阶段弃完手牌之后）：瓦解。
    // 在敌人出手之前，所以这一回合剩下的格挡先吃它。
    fire(&mut s, Hook::TurnEndLate, 0);
    check_over(&mut s);
    if s.combat_over {
        return s;
    }
    match inc {
        Some((i, live)) => injected_enemy_turn(&mut s, i, live),
        None => enemy_turn(&mut s),
    }
    check_over(&mut s);
    if s.combat_over {
        return s;
    }
    // 「本回合内」的力量增减到此为止 —— 两个方向共用这一条路：
    // 预备打击给玩家 +2（TempStrength = +2，这里减回去），
    // 黑暗镣铐给敌人 -9（TempStrength = -9，这里加回来）。
    //
    // **必须在 `enemy_turn` 之后**：黑暗镣铐的"本回合失去 9 点力量"要覆盖
    // 敌人这一击，放在敌人出手之前等于这张牌完全没用。
    // （第一版就放错了位置，`dark_shackles_restores_enemy_strength_at_end_of_turn`
    // 当场报出敌人打了 24 而不是 15。）
    strip_temp_strength(&mut s.player);
    for e in 0..s.n_enemies as usize {
        strip_temp_strength(&mut s.enemies[e]);
    }
    decay(&mut s.player);
    for e in 0..s.n_enemies as usize {
        decay(&mut s.enemies[e]);
    }
    start_player_turn_before_draw(&mut s);
    if open {
        open_hand(&mut s);
    }
    check_over(&mut s);
    s
}

/// 喝一瓶药水。**药水不是牌**：不计入 `cards_played`/`attacks_played`
/// （所以踩踏不会因此减费、缓慢不会因此累加 —— 两条都是实测过的），
/// 也不触发 `PlayerSkill`（激怒不会因为你喝药水而涨力量）。
///
/// 效果本身走**和卡牌同一个** `resolve_ops`，差异全部收在 [`Source`] 里。
/// 早先这里另写了一个 `Op` 解释器，那样 `Op::Block` 会在两处有两个含义，
/// 而没有任何东西守着它们不长歪。
fn use_potion(mut s: State, slot: usize, target: usize) -> State {
    if slot >= MAX_POTIONS {
        return s;
    }
    let pot_id = s.potions[slot];
    // 内容表不认识的药水**不伪造行为**：原样返回。`legal_actions` 也不会生成它，
    // 于是求解器既不会喝它、也不会假装它有效果。
    if pot_id == potion::NONE || pot_id == potion::UNKNOWN {
        return s;
    }
    s.potions[slot] = potion::NONE;
    let ops = potion_def(pot_id).ops;
    // `cix` 传 0 是安全的：`Op::GrowThisCard` 在 `Source::Potion` 下被守卫跳过。
    resolve_ops(&mut s, ops, CardInst::EMPTY, 0, target, Source::Potion);
    check_over(&mut s);
    s
}

/// The public transition. Illegal actions return the state unchanged.
pub fn step(mut s: State, a: Action) -> State {
    if s.combat_over {
        return s;
    }
    match a {
        Action::Choose { hand } => {
            let i = hand as usize;
            match s.pending {
                Pending::None => {}
                Pending::ExhaustFromHand { remaining } => {
                    if i < s.n_hand as usize {
                        let c = s.take_from_hand(i);
                        exhaust_card(&mut s, c);
                        if remaining <= 1 || s.n_hand == 0 {
                            s.pending = Pending::None;
                        } else {
                            s.pending = Pending::ExhaustFromHand { remaining: remaining - 1 };
                        }
                    }
                }
                // 头槌：弃牌堆 -> **抽牌堆顶**。候选集和 `FetchFromDiscard`
                // 一样是弃牌堆，只有去处不同。
                Pending::DiscardToDrawTop { remaining, exclude } => {
                    if i < s.n_disc as usize && (s.n_draw as usize) < MAX_CARDS {
                        let c = s.take_from_disc(i);
                        // 抽牌堆顶 = `draw` 数组的末尾：`draw_one` 取的是
                        // `n_draw - 1`。放错一头就变成"沉到牌堆底"，
                        // 那是效果完全相反的 bug。
                        // **走 `to_draw_top` 而不是手写**：那边还要维护
                        // `n_draw_known`（这一张是确定的）。
                        s.to_draw_top(c);
                        if remaining <= 1 || s.n_disc == 0 {
                            s.pending = Pending::None;
                        } else {
                            // 排除的那张还是同一张（多选的每一步都不许选它自己）
                            s.pending =
                                Pending::DiscardToDrawTop { remaining: remaining - 1, exclude };
                        }
                    }
                }
                Pending::FetchFromDiscard { remaining } => {
                    if i < s.n_disc as usize && (s.n_hand as usize) < MAX_HAND {
                        let c = s.take_from_disc(i);
                        s.to_hand(c);
                        if remaining <= 1 || s.n_disc == 0 {
                            s.pending = Pending::None;
                        } else {
                            s.pending = Pending::FetchFromDiscard { remaining: remaining - 1 };
                        }
                    }
                }
                Pending::PutToDrawPile { remaining } => {
                    if i < s.n_hand as usize {
                        let c = s.take_from_hand(i);
                        s.to_draw_top(c);
                        if remaining <= 1 || s.n_hand == 0 {
                            s.pending = Pending::None;
                        } else {
                            s.pending = Pending::PutToDrawPile { remaining: remaining - 1 };
                        }
                    }
                }
                Pending::UpgradeInHand { remaining } => {
                    if i < s.n_hand as usize {
                        let cix = s.hand[i] as usize;
                        // 选中不可升级的牌：不生效，但仍然推进 —— 不留死局。
                        if crate::content::upgradable(s.cards[cix].id) {
                            s.cards[cix].flags |= F_UPGRADED;
                        }
                        if remaining <= 1 {
                            s.pending = Pending::None;
                        } else {
                            s.pending = Pending::UpgradeInHand { remaining: remaining - 1 };
                        }
                    }
                }
            }
            s
        }
        Action::UsePotion { slot, target } => {
            if s.pending != Pending::None {
                return s;
            }
            let sl = slot as usize;
            let t = target as usize;
            if t >= s.n_enemies as usize || !s.enemies[t].alive() {
                match s.first_alive() {
                    Some(a) => use_potion(s, sl, a),
                    None => use_potion(s, sl, 0),
                }
            } else {
                // 有目标的药水同样会转朝向（游戏原文把牌和药水并列）
                if crate::ops::potion_def(s.potions[sl.min(crate::state::MAX_POTIONS - 1)]).targeted {
                    face_toward(&mut s, t);
                }
                use_potion(s, sl, t)
            }
        }
        Action::PlayCard { hand, target } => {
            if s.pending != Pending::None {
                return s;
            }
            // 懒惰：`legal_actions` 不给打，`step` 也要原样返回（不变量 5）。
            // 轰鸣故意没收进来：它一直只在 `legal_actions` 那一侧，改它要单独过一遍对拍。
            if s.player.get(St::Sloth) > 0 && s.cards_played >= s.player.get(St::Sloth) {
                return s;
            }
            let i = hand as usize;
            if i >= s.n_hand as usize {
                return s;
            }
            if !playable(s.cards[s.hand[i] as usize].id) {
                return s;
            }
            if effective_cost(&s, i) > s.energy {
                return s;
            }
            let t = target as usize;
            if t < s.n_enemies as usize && s.enemies[t].alive() {
                play_card(s, i, t)
            } else if let Some(a) = s.first_alive() {
                // 目标给错了（同步进来的局面、或者那只已经被这一线打死了）
                // 就落到第一只活着的身上。
                play_card(s, i, a)
            } else if !card(s.cards[s.hand[i] as usize].id).targeted {
                // **一个活着的敌人都没有，但战斗还没结束**：实验体被砍掉一条命
                // 之后整个我方回合它都不在场（`State::any_enemy_present`），
                // 而出牌阶段还开着、能量也还在。[实测] 那个窗口里**无目标牌
                // 照样打得出去**（act3_f48 帧11/12 打了坚定不移+ 和 时候未到），
                // 打不出去的只有攻击牌 —— 它们没有目标。
                //
                // 原来这一支无条件返回原状态，于是**每一张牌都被拒**：
                // `legal_actions` 允许（它对无目标牌本来就不看敌人），`step` 拒绝，
                // 两个入口对同一个局面给出不同的答案。药水那一支从来没有这个洞
                // （`None => use_potion(s, sl, 0)`），是这里漏了。
                play_card(s, i, 0)
            } else {
                s
            }
        }
        Action::EndTurn => {
            if s.pending != Pending::None {
                return s;
            }
            end_turn(s)
        }
    }
}

/// `ModifyXxx` 那一族里**改"这次施加多少层"**的那些。今天只有损毁头盔一件。
///
/// [源码] `RuinedHelmet.TryModifyPowerAmountReceived`：只认 `StrengthPower`、
/// 只认加在**自己**身上的、只认 `amount > 0`，用掉之后 `UsedThisCombat = true`。
///
/// **两个调用点必须共用它**：卡牌/药水那条路（[`apply_status`]）和遗物开局赠予
/// 那条路（[`grant_relics`] 的第二趟）。金刚杵的 1 点力量在游戏里走的也是
/// `PowerCmd.Apply`，一样会被头盔翻倍 —— [实测] 2026-09-09
/// `act3_f48_boss_aeonglass_2026-09-06` 第 0 帧玩家力量是 **2**（金刚杵 1 × 2）。
///
/// 它读的是 **status**（`RULE_MODIFIERS` 里登记着），不是 `if 有没有某遗物`。
pub(crate) fn modify_status_amount_received(s: &mut State, to_player: bool, st: St, amt: i32) -> i32 {
    if to_player && st == St::Strength && amt > 0 && s.player.get(St::RuinedHelmet) > 0 {
        s.player.set(St::RuinedHelmet, 0);
        return amt * 2;
    }
    amt
}

/// 把一件遗物的**战斗层状态**交给玩家实体。**必须在 [`begin_combat`] 之前调用。**
///
/// # 为什么这条规则在 L1 而不在 L3
///
/// 遗物在这个内核里没有自己的实体，它的全部战斗层效果都是**挂在玩家身上的
/// status**（`RelicDef::start_status` / `private_status` / `counter_to`），规则由
/// `POWERS` 和 `damage.rs` 认那些 status 来跑 —— 一个 `if 有没有某遗物` 都没有。
/// 「哪件遗物给哪几个 status」因此是游戏规则的一部分，按不变量它就该在 L1，
/// 让 L3 自己抄一遍等于把规则复制到上层。
///
/// # 三栏的处置各不相同（照 `RelicDef` 那三栏的注释）
///
/// * `start_status` —— 游戏也会报的量（金刚杵的力量、护喉甲的覆甲）。
///   **对拍那条路不走这里**（观测里本来就有），这个函数是给「内核自己开一场仗」用的
/// * `private_status` —— 游戏不报的内核私有记账（臂甲的充能）
/// * `counter_to` —— 面板上那个**跨战斗保留**的计数器。`None` 一律当 0，
///   而 0 对摆动球那种相位量**是个静默的错**：调用方拿得到就传，
///   拿不到该由调用方点名报出来（`synth::Gap::RelicCounterMissing`）
///
/// 用 `add` 不用 `set`：两件遗物给同一个 status 时该叠加。
/// （`replay::sync` 那条路是逐帧 `set` 回去的 —— 那边每帧重建，语义不同。）
///
/// # 两趟，顺序不能换
///
/// 1. **私有量和计数器**（臂甲的充能、钢笔尖的计数、**损毁头盔**）
/// 2. **游戏也会报的那些**（金刚杵的力量、护喉甲的覆甲…），过一遍
///    [`modify_status_amount_received`]
///
/// 因为第 1 趟里有**修饰器**：损毁头盔要在力量加上来之前就位。游戏那边
/// 不存在这个问题（遗物在进房间之前全都已经在身上，修饰器钩子是被查询的），
/// 而内核这边是**顺序执行**的 —— 一趟走完就会依赖遗物在观测里的排列顺序，
/// 那是个静默的错：同一批遗物换个拾取顺序，力量就从 2 变成 1。
pub fn grant_relics(s: &mut State, relics: &[(&RelicDef, Option<i32>)]) {
    for (def, counter) in relics {
        for (st, amt) in def.private_status {
            s.player.add(*st, *amt);
        }
        if let Some(st) = def.counter_to {
            s.player.set(st, counter.unwrap_or(0));
        }
    }
    for (def, _) in relics {
        for (st, amt) in def.start_status {
            let v = modify_status_amount_received(s, true, *st, *amt);
            s.player.add(*st, v);
        }
    }
}

/// Set up a combat: shuffle the draw pile and open the first hand.
pub fn begin_combat(mut s: State) -> State {
    refresh_hp_threshold_strength(&mut s);
    let n = s.n_draw as usize;
    for i in (1..n).rev() {
        let j = next_below(&mut s.rng.shuffle, i + 1);
        s.draw.swap(i, j);
    }
    // 洗完了，顶上一张确定的都没有
    s.n_draw_known = 0;
    // **固有（`F_INNATE`）的牌挪到牌堆顶**，于是起手必定摸到。
    // 内核的抽牌堆顶在**末尾**（`pop_draw_top` 从末尾取），所以是往后挪。
    //
    // 今天唯一的来源是「王室认证」附魔。多张固有牌之间的相对顺序**没有依据**
    // （源码里是洗完之后逐张 `MoveToTop`，取决于遍历顺序）——
    // 保持它们在牌堆里的原有先后，不另外洗一次。
    {
        let n = s.n_draw as usize;
        let mut tmp = [0u8; MAX_CARDS];
        let mut k = 0;
        for pass_innate in [false, true] {
            for r in 0..n {
                let is_innate = s.cards[s.draw[r] as usize].flags & F_INNATE != 0;
                if is_innate == pass_innate {
                    tmp[k] = s.draw[r];
                    k += 1;
                }
            }
        }
        s.draw[0..n].copy_from_slice(&tmp[0..n]);
    }
    for e in 0..s.n_enemies as usize {
        let def = enemy_def(s.enemy_def[e]);
        for (st, amt) in def.start_status {
            s.enemies[e].add(*st, *amt);
        }
        // 有的敌人开战时给**玩家**挂东西（火箭的包围）。`EnemyDef::start_status`
        // 只装挂自己身上的，那一类走 `content::ENEMY_START_PLAYER_STATUS`。
        for (st, amt) in crate::content::enemy_start_player_status(def.name) {
            s.player.add(st, amt);
        }
        // 游戏不报、规则要读的身份标记（骑士团的施咒者）。对拍路径在 `replay::sync` 里按
        // 观测到的名字挂同一张表，见 `content::ENEMY_PRIVATE_MARKERS`。
        for (st, v) in crate::content::enemy_private_markers(def.name) {
            s.enemies[e].set(st, v);
        }
    }
    // **开局朝右。** [源码] `SurroundedPower._facing` 是个没有初始化式的
    // `Direction` 字段，而 `Direction.Right` 是枚举的 0 —— 所以挂上包围的
    // 那一刻玩家朝右，**从左边打来的**（`BackAttackLeft`）才吃 ×1.5。
    // 内核的 `St::FacingRight` 默认 0（朝左），正好反了。
    //
    // [实测] 2026-09-09 `act2_f33_boss_crusher` 第 0 帧的意图标签：
    // 碾碎爪（左）**18 = 12×1.5**、火箭（右）**3**（面板值，没乘）。
    // 对拍那条路不受影响 —— 那边朝向是从意图标签反推的
    // （`replay::infer_facing`），它每帧重推、自愈。
    if s.player.get(St::Surrounded) > 0 {
        s.player.set(St::FacingRight, 1);
    }
    // 开局第一手交给机器（没机器的还是 0，行为不变）
    for e in 0..s.n_enemies as usize {
        s.enemy_move[e] = initial_move(&s, e);
    }
    s.turn = 0;
    start_player_turn(&mut s);
    s
}
