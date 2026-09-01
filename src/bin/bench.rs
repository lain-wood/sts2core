//! Throughput check. The whole reason for a native kernel is that L2/L3 need
//! billions of steps per decision; this measures whether we actually get them.
//!
//! No dependencies on purpose (std threads, not rayon) so it builds offline.

use std::time::Instant;

use sts2core::content::{card, enemy};
use sts2core::state::{next_below, State};
use sts2core::step::{begin_combat, legal_actions, step, Action};

/// A 26-card deck roughly matching the real Act 3 deck, vs a 300 HP enemy.
fn make_combat(seed: u64) -> State {
    let mut s = State::new(95, seed);
    s.add_enemy(enemy::DUMMY, 300);
    for _ in 0..4 {
        s.add_card(card::STRIKE, 0, 0);
    }
    for _ in 0..4 {
        s.add_card(card::DEFEND, 0, 0);
    }
    s.add_card(card::BASH, 0, 0);
    s.add_card(card::DISMANTLE, sts2core::F_UPGRADED | sts2core::F_CORRUPT, 0);
    s.add_card(card::ASH_STRIKE, 0, 0);
    s.add_card(card::ASH_STRIKE, sts2core::F_UPGRADED, 0);
    s.add_card(card::DEMON_FLAME, sts2core::F_UPGRADED, 2);
    s.add_card(card::LIGHTNING, 0, 0);
    s.add_card(card::LIGHTNING, sts2core::F_UPGRADED, 0);
    s.add_card(card::BULLY, 0, 0);
    s.add_card(card::BRAND, 0, 0);
    s.add_card(card::DOMINATE, 0, 0);
    s.add_card(card::STAMPEDE, 0, 0);
    s.add_card(card::DEMON_FORM, sts2core::F_UPGRADED, 0);
    s.add_card(card::RUTHLESS, 0, 0);
    for _ in 0..7 {
        s.add_card(card::STRIKE, 0, 0);
    }
    begin_combat(s)
}

/// One rollout under a uniform-random legal policy. Returns steps taken.
fn rollout(seed: u64) -> u64 {
    let mut s = make_combat(seed);
    let mut rng = seed ^ 0xDEAD_BEEF_CAFE_F00D;
    let mut steps = 0u64;
    while !s.combat_over && s.turn < 40 {
        let (acts, n) = legal_actions(&s);
        if n == 0 {
            break;
        }
        // bias away from ending the turn immediately so hands actually get played
        let pick = if n > 1 { next_below(&mut rng, n - 1) } else { 0 };
        s = step(s, acts[pick]);
        steps += 1;
    }
    let _ = std::hint::black_box(s.player.hp);
    steps
}

fn main() {
    println!("size_of::<State>() = {} bytes", std::mem::size_of::<State>());
    println!("size_of::<Action>() = {} bytes", std::mem::size_of::<Action>());

    // warmup
    let mut warm = 0u64;
    for i in 0..2_000 {
        warm += rollout(i);
    }
    std::hint::black_box(warm);

    // ---- single thread ----
    let iters = 200_000u64;
    let t0 = Instant::now();
    let mut steps = 0u64;
    for i in 0..iters {
        steps += rollout(i);
    }
    let el = t0.elapsed();
    let sps = steps as f64 / el.as_secs_f64();
    println!(
        "\n1 thread : {} rollouts, {} steps in {:.3}s",
        iters,
        steps,
        el.as_secs_f64()
    );
    println!(
        "           {:.2} M steps/s   ({:.0} ns/step, {:.1} us/rollout)",
        sps / 1e6,
        1e9 / sps,
        el.as_secs_f64() * 1e6 / iters as f64
    );

    // ---- all cores ----
    let nthreads = std::thread::available_parallelism().map(|v| v.get()).unwrap_or(8);
    let per = iters;
    let t1 = Instant::now();
    let total: u64 = std::thread::scope(|sc| {
        let hs: Vec<_> = (0..nthreads)
            .map(|t| {
                sc.spawn(move || {
                    let mut st = 0u64;
                    let base = (t as u64) * 1_000_000;
                    for i in 0..per {
                        st += rollout(base + i);
                    }
                    st
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).sum()
    });
    let el1 = t1.elapsed();
    let sps1 = total as f64 / el1.as_secs_f64();
    println!(
        "\n{} threads: {} steps in {:.3}s",
        nthreads,
        total,
        el1.as_secs_f64()
    );
    println!("           {:.1} M steps/s aggregate", sps1 / 1e6);

    // What that buys for one card-reward decision:
    // 4 candidates x 12 encounters x 500 paired seeds x ~3e5 steps
    let need = 4.0 * 12.0 * 500.0 * 3e5;
    println!(
        "\none card-reward decision (~{:.1e} steps): {:.1} s",
        need,
        need / sps1
    );

    // ---- 指纹开销：planner 每个节点都要算一次 ----
    //
    // **这个数决定要不要做增量 Zobrist。** 一个搜索节点 ≈ 一次 `step`；
    // 指纹如果和 step 同量级，那 planner 的一半时间花在哈希上，值得动；
    // 如果只有零头，增量哈希就是拿复杂度换不到的东西
    //（增量意味着每一处状态改动都要跟着更新，而 `State` 是 POD、`step` 是纯函数，
    //  那条路会把不变量 1 和 5 一起搅浑）。
    {
        use sts2core::solver::key;
        let mut s = sts2core::step::begin_combat({
            let mut s = State::new(80, 1);
            s.add_enemy(sts2core::content::enemy::NIBBIT, 44);
            for _ in 0..24 {
                s.add_card(sts2core::content::card::STRIKE, 0, 0);
            }
            s
        });
        s.n_hand = 7;
        let n = 2_000_000u64;
        // **归因**：把局面拆开量，好知道那 140 ns 花在哪。
        // 结论决定了 planner 要不要做增量哈希 —— 如果时间在"状态槽位"上，
        // 增量也救不了（每步都会动状态）；如果在牌堆上，倒是能靠"牌堆没变就
        // 不重算"省下来。
        let bench1 = |tag: &str, s: &State| {
            let mut s = *s;
            let t = Instant::now();
            let mut acc = 0u64;
            for i in 0..n {
                s.player.hp = 60 + (i % 20) as i32; // 免得被优化掉
                acc ^= key(&s);
            }
            println!(
                "  {tag:<26} {:.0} ns/次  [acc {}]",
                t.elapsed().as_secs_f64() * 1e9 / n as f64,
                acc & 1
            );
        };
        println!("
key() 指纹开销（手牌 {} / 抽牌堆 {} / 敌人 {}）:", s.n_hand, s.n_draw, s.n_enemies);
        bench1("完整局面", &s);
        let mut no_draw = s;
        no_draw.n_draw = 0;
        bench1("抽牌堆清空", &no_draw);
        let mut no_enemy = s;
        no_enemy.n_enemies = 0;
        bench1("敌人清空", &no_enemy);
        let mut bare = s;
        bare.n_draw = 0;
        bare.n_enemies = 0;
        bare.n_hand = 0;
        bench1("只剩玩家状态槽", &bare);
    }
}
