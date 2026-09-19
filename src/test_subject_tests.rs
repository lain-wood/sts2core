//! Regression witnesses for the 2026-09-15 TestSubject recording.
use crate::{content::*, replay, state::*, step::*, *};

fn fight(cards: &[(u16, u8)], relic: &str, hp: i32, max_hp: i32) -> State {
    let mut s = State::new(max_hp, 19);
    s.player.hp = hp;
    s.add_enemy(enemy::DUMMY, 500);
    for &(id, flags) in cards { s.add_card(id, flags, 0); }
    grant_relics(&mut s, &[(relic_by_id(relic).unwrap(), None)]);
    begin_combat(s)
}

fn play(s: State, id: u16) -> State {
    let hand = (0..s.n_hand).find(|&i| s.cards[s.hand[i as usize] as usize].id == id).unwrap();
    step(s, Action::PlayCard { hand, target: 0 })
}

fn incoming(s: State, damage: i32) -> State {
    let mut inc = [(0, 0); MAX_ENEMIES];
    inc[0] = (damage, 1);
    end_turn_with_incoming(s, &inc)
}

fn recorded() -> replay::Trace {
    replay::parse_trace(include_str!("../traces/act3_f48_boss_test_subject_2026-09-15.json")).unwrap()
}

#[test]
fn strike_dummy_adds_before_multipliers_and_only_to_tagged_attacks() {
    let mut s = fight(&[(card::STRIKE, F_CORRUPT)], "STRIKE_DUMMY", 80, 80);
    s.player.set(St::Strength, 2);
    s.player.set(St::Weak, 1);
    s.enemies[0].set(St::Vulnerable, 1);
    let s = play(s, card::STRIKE);
    // floor((6*1.5 + 3 + 2)*.75*1.5) = 15.
    assert_eq!(s.enemies[0].hp, 485);
    let s = fight(&[(card::BASH, 0)], "STRIKE_DUMMY", 80, 80);
    assert_eq!(play(s, card::BASH).enemies[0].hp, 492);
    let mut s = fight(&[], "STRIKE_DUMMY", 80, 80);
    s.potions[0] = potion::FIRE;
    assert_eq!(step(s, Action::UsePotion { slot: 0, target: 0 }).enemies[0].hp, 480);
}

#[test]
fn red_skull_crosses_threshold_in_both_directions_without_stacking() {
    let s = fight(&[(card::BLOODLETTING, 0)], "RED_SKULL", 41, 80);
    assert_eq!(s.player.get(St::Strength), 0);
    let mut s = play(s, card::BLOODLETTING);
    assert_eq!(s.player.get(St::Strength), 3);
    s = incoming(s, 1);
    assert_eq!(s.player.get(St::Strength), 3, "further damage must not award strength again");
    s.player.set(St::Regen, 20);
    s = incoming(s, 0);
    assert!(s.player.hp > 40);
    assert_eq!(s.player.get(St::Strength), 0, "healing across half removes exactly three");
    assert_eq!(fight(&[], "RED_SKULL", 40, 81).player.get(St::Strength), 3);
    assert_eq!(fight(&[], "RED_SKULL", 41, 81).player.get(St::Strength), 0);
}

#[test]
fn red_skull_from_enemy_damage_immediately_changes_next_attack() {
    let s = fight(&[(card::STRIKE, 0)], "RED_SKULL", 41, 80);
    let s = incoming(s, 1);
    assert_eq!(s.player.hp, 40);
    assert_eq!(s.player.get(St::Strength), 3);
    assert_eq!(play(s, card::STRIKE).enemies[0].hp, 491);
}

#[test]
fn test_subject_recording_has_no_hard_single_step_mismatch_or_unknown_potion() {
    let report = replay::verify(&recorded());
    for r in report.results {
        assert!(!matches!(r.verdict, replay::Verdict::Mismatch | replay::Verdict::UnknownContent),
            "frame {}: {:?} {:?}", r.i, r.verdict, r.diffs);
    }
}

#[test]
fn test_subject_recording_identifies_phase_and_growing_hits_on_every_live_frame() {
    let t = recorded();
    let mut r = replay::Replayer::new(&t.run);
    for frame in &t.frames {
        let mut synced = r.sync(&frame.obs);
        let rows = r.identify_enemies(&mut synced.state, &frame.obs);
        for row in rows {
            assert!(row.usable(), "{} {:?}", frame.obs.round, row);
            let e = &synced.state.enemies[row.slot];
            if e.get(St::PainfulStabs) > 0 {
                assert_eq!(row.move_ix, Some(3));
                let observed = &frame.obs.enemies[0];
                assert_eq!(e.get(St::ClawGrowth) + 3, observed.attacks[0].1);
                let threat = rollout::predicted_threat(&synced.state);
                assert_eq!(threat.incoming[row.slot].1, observed.attacks[0].1);
            }
            if e.get(St::Nemesis) > 0 { assert!((4..=6).contains(&row.move_ix.unwrap())); }
        }
    }
}

#[test]
fn test_subject_next_intent_is_predicted_and_wrong_hit_count_is_rejected() {
    let t = recorded();
    let report = replay::verify_enemy_ai(&t);
    assert_eq!(report.rows[0].exact, report.rows[0].predicted, "{:?}", report.rows);
    let mut bad = t.clone();
    for f in &mut bad.frames {
        if f.obs.round == 3 {
            for e in &mut f.obs.enemies {
                e.attacks[0].1 += 2;
                for (kind, label) in &mut e.raw_intents {
                    if kind == "Attack" { *label = "10×6".into(); }
                }
            }
        }
    }
    let report = replay::verify_enemy_ai(&bad);
    assert!(report.rows[0].exact < report.rows[0].predicted,
        "a corrupted observed count must not become its own expected value");
}

#[test]
fn test_subject_empty_revive_window_keeps_the_next_form_for_planning() {
    let t = recorded();
    let mut r = replay::Replayer::new(&t.run);
    let mut checked = 0;
    for f in &t.frames {
        let mut synced = r.sync(&f.obs);
        if !f.obs.pending && f.obs.enemies.is_empty() && (f.obs.round == 1 || f.obs.round == 4) {
            r.identify_enemies(&mut synced.state, &f.obs);
            let s = synced.state;
            assert_eq!(s.enemy_def[0], enemy::TEST_SUBJECT_BOSS);
            assert_eq!(s.enemies[0].hp, 0);
            let s = step(s, Action::EndTurn);
            let next_hp = if f.obs.round == 1 { 200 } else { 300 };
            assert_eq!(s.enemies[0].hp, next_hp);
            assert_eq!(s.enemy_move[0], if next_hp == 200 { 3 } else { 4 });
            checked += 1;
        }
    }
    assert!(checked >= 2);
}
