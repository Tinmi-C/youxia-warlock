//! RPL1 acceptance (alignment plan §10): record a scripted headless match,
//! replay it bit-exact, and prove the hash reconciliation is live (seed
//! sensitivity, RON roundtrip). Bundle shape mirrors tests/harness.rs.

use bevy::app::App;
use bevy::math::Vec2;
use warbase::bundle::{assemble, build_grid, MapBundle};
use warbase::prelude::*;

fn grunt() -> UnitDef {
    UnitDef {
        leak: 1,
        id: "grunt".into(),
        attrs: BaseAttributes { hp: 40.0, move_speed: 12.0, body_radius: 0.3 },
        attack: None,
        armor_type: ArmorType::Heavy,
        cost_gold: 0,
        cost_wood: 0,
        bounty_gold: 0,
        bounty_wood: 0,
        grant_xp: 0,
        traits: vec![],
        auras: vec![],
        hero: false,
        skills: vec![],
        upgrade_to: None,
        heal: None,
        mana: None,
    }
}

/// 2-wave manual-advance scenario: wave 0 spawns nothing (clears instantly
/// once started), wave 1 is the real creep wave circling a small closed loop
/// under lap buffs so the state fingerprint keeps moving after the spawns.
/// `reward_gold: 0` and no early-call bonus are deliberate: engine-internal
/// econ writes are re-derived on replay and must stay out of the recorded
/// stream (see the replay module docs).
fn scenario_bundle() -> MapBundle {
    let mut units = std::collections::HashMap::new();
    units.insert("grunt".into(), grunt());
    let mut waves = WaveTable::default();
    waves.waves.push(WaveEntry {
        spawns: vec![],
        boss_timeout: None,
        preview: "calm before".into(),
        reward_gold: 0,
    });
    waves.waves.push(WaveEntry {
        spawns: vec![SpawnSpec {
            unit: "grunt".into(),
            count: 2,
            interval: 0.2,
            path: 0,
            delay: 0.0,
            hp_scale: 1.0,
        }],
        boss_timeout: None,
        preview: "2 grunts".into(),
        reward_gold: 0,
    });
    waves.advance = Advance::Manual;
    MapBundle {
        identity: warbase::bundle::MapIdentity { id: "rpl1".into(), version: 1 },
        world: warbase::bundle::WorldDef {
            paths: vec![PathDef::closed_loop(vec![
                [-2.0, -1.0],
                [2.0, -1.0],
                [2.0, 1.0],
                [-2.0, 1.0],
            ])],
            build: warbase::bundle::BuildDomainDef::FreeGrid { cell: 1.0, cols: 16, rows: 12 },
            blocked: vec![],
        },
        units,
        waves,
        rules: MatchRules {
            leak: LeakPolicy::Stockpile { cap: 50 },
            build_window: BuildWindow::Always,
            lap_buff: Some(LapBuffPolicy { speed_pct: 0.05, hp_pct: 0.1 }),
            win: WinCondition::SurviveWaves { waves: 2 },
            sell_refund_pct: 0.7,
        },
        econ_start: EconStart { gold: 200, wood: 0, pop_cap: 8 },
        econ_rules: EconRules::default(),
        attack_armor: AttackArmorTable::default(),
        nav: Default::default(),
        vision: Default::default(),
        items: Default::default(),
        production: Default::default(),
        teams: Default::default(),
        starting_forces: Default::default(),
    }
}

/// Player inputs for `frame`, written between updates — the same timing real
/// input has. Exercises all three recorded vocabulary kinds, including two
/// inputs on one frame (arrival order must survive).
fn script_inputs(app: &mut App, frame: u32) {
    let world = app.world_mut();
    match frame {
        10 => {
            world.write_message(PickAttempt {
                world: Vec2::ZERO,
                button: PickButton::Left,
                click_count: 1,
                ctrl: false,
                shift: false,
            });
            world.write_message(PickAttempt {
                world: Vec2::new(1.5, -0.5),
                button: PickButton::Right,
                click_count: 2,
                ctrl: true,
                shift: true,
            });
        }
        20 => {
            world.write_message(EconCommand::Grant { gold: 50, wood: 5 });
        }
        35 => {
            world.write_message(EconCommand::BuyPopulation);
        }
        40 => {
            world.write_message(StartWave);
        }
        45 => {
            world.write_message(EconCommand::ConvertWood { wood: 3 });
        }
        50 => {
            world.write_message(StartWave);
        }
        _ => {}
    }
}

/// Run the scripted match for `frames` updates with recording on; returns
/// the finished `RecordingLog`. Same init recipe as `replay`/`run_headless`.
fn record_scripted(bundle: &MapBundle, seed: u64, frames: u32) -> RecordingLog {
    let mut app = warbase::testing::test_app();
    app.insert_resource(MatchRng::seeded(seed));
    assemble(&mut app.world_mut(), bundle);
    app.insert_resource(build_grid(&bundle.world));
    let mut rec = RecordingLog::default();
    rec.log.version = INPUT_LOG_VERSION;
    rec.log.seed = seed;
    rec.log.map_hash = 1; // opaque bundle fingerprint (RPL2 hashes the file)
    app.insert_resource(rec);
    for frame in 1..=frames {
        script_inputs(&mut app, frame);
        app.update();
    }
    app.world_mut().remove_resource::<RecordingLog>().expect("recording was active")
}

/// UT-RPL-01 (AC1): a 300-frame scripted match records all three envelope
/// kinds (same-frame inputs grouped in arrival order) and a strictly
/// increasing hash checkpoint chain that starts at the initial state.
#[test]
fn records_scripted_match() {
    let bundle = scenario_bundle();
    let rec = record_scripted(&bundle, 1234, 300);

    assert!(!rec.log.frames.is_empty(), "inputs were recorded");
    for w in rec.log.frames.windows(2) {
        assert!(w[0].0 < w[1].0, "input frames strictly increasing");
    }
    let flat: Vec<&CommandEnvelope> = rec.log.frames.iter().flat_map(|(_, e)| e.iter()).collect();
    assert!(flat.iter().any(|e| matches!(e, CommandEnvelope::Pick { .. })), "pick recorded");
    assert!(flat.iter().any(|e| matches!(e, CommandEnvelope::WaveStart)), "wave start recorded");
    assert!(
        flat.iter().any(|e| matches!(e, CommandEnvelope::Ext { kind, .. } if kind == ECON_KIND)),
        "econ recorded"
    );
    // Frame 10 carries both picks, arrival order preserved.
    let frame10 = rec.log.frames.iter().find(|(f, _)| *f == 10).expect("frame 10 recorded");
    assert_eq!(
        frame10.1.as_slice(),
        &[
            CommandEnvelope::Pick {
                world: [0.0, 0.0],
                button: PickButton::Left,
                clicks: 1,
                ctrl: false,
                shift: false,
            },
            CommandEnvelope::Pick {
                world: [1.5, -0.5],
                button: PickButton::Right,
                clicks: 2,
                ctrl: true,
                shift: true,
            },
        ][..]
    );
    // Checkpoints (0, 30, ..., 270) over 300 updates, frame 0 = initial state.
    assert_eq!(rec.log.hashes.len(), 10);
    assert_eq!(rec.log.hashes[0].0, 0);
    assert_eq!(rec.log.hashes.last().unwrap().0, 270);
    for w in rec.log.hashes.windows(2) {
        assert!(w[0].0 < w[1].0, "hash frames strictly increasing");
    }
}

/// UT-RPL-02 (AC2): the same log replayed twice yields the identical hash
/// chain — replay itself is deterministic.
#[test]
fn replay_twice_is_identical() {
    let bundle = scenario_bundle();
    let rec = record_scripted(&bundle, 99, 300);
    let first = replay(&bundle, &rec.log).expect("replay reconciles");
    let second = replay(&bundle, &rec.log).expect("replay reconciles again");
    assert!(!first.is_empty());
    assert_eq!(first, second);
}

/// UT-RPL-03 (AC3, the reconciliation core): the replayed match's per-frame
/// state hash equals the recorded one at every shared checkpoint, and the
/// chain actually evolved (econ changes, spawns, lap buffs) — not a vacuous
/// pass over a frozen world.
#[test]
fn replay_matches_recording_hashes() {
    let bundle = scenario_bundle();
    let rec = record_scripted(&bundle, 1234, 300);
    let actual = replay(&bundle, &rec.log).expect("replay reconciles");
    assert_eq!(actual, rec.log.hashes, "same-frame state hashes must be equal");
    let distinct: std::collections::HashSet<u64> = actual.iter().map(|(_, h)| *h).collect();
    assert!(distinct.len() >= 4, "hash chain moved during the match: {actual:?}");
}

/// UT-RPL-04 (AC4, sensitivity): a tampered seed must fail reconciliation —
/// the base is currently RNG-free (only game-side code consumes MatchRng),
/// so the fingerprint carries the seed resource; the mismatch surfaces at
/// the very first checkpoint (frame 0).
#[test]
fn tampered_seed_fails_reconciliation() {
    let bundle = scenario_bundle();
    let rec = record_scripted(&bundle, 1234, 300);
    let mut tampered = rec.log.clone();
    tampered.seed ^= 1;
    let err = replay(&bundle, &tampered).expect_err("a mis-seeded replay must not reconcile");
    assert!(err.contains("frame 0"), "error names the first divergent frame: {err}");
}

/// UT-RPL-05 (AC5): a RON roundtrip of the log changes nothing — the replay
/// of the deserialized log produces the identical hash chain.
#[test]
fn ron_roundtrip_preserves_replay() {
    let bundle = scenario_bundle();
    let rec = record_scripted(&bundle, 1234, 300);
    let text = ron::to_string(&rec.log).expect("log serializes");
    let back: MatchInputLog = ron::from_str(&text).expect("log parses");
    let direct = replay(&bundle, &rec.log).expect("direct replay reconciles");
    let roundtrip = replay(&bundle, &back).expect("roundtrip replay reconciles");
    assert_eq!(direct, roundtrip);
}

