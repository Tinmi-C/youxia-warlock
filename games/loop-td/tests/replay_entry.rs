//! RPL2 acceptance (alignment plan §10): the loop-td record/replay entry
//! points. Everything here drives the PRODUCTION code paths
//! (`replay_io::arm_recording` / `write_input_log` / `replay_file` and the
//! `ReplayIoPlugin` match-end writer) — main.rs only parses flags around
//! them.
//!
//! Composition note: the recording app below mirrors `warbase::replay`'s
//! own app exactly (test_app + seed + assemble + grid + RecordingLog, the
//! bevy 0.19 resource-entity symmetry the replay module documents), which
//! is why the reconciliation is exact. The plugin's writer system is
//! covered separately (`record_plugin_writes_run_at_match_end`) without a
//! reconciliation claim — see the replay_io module header for the phase-1
//! real-app boundary.

use bevy::math::Vec2;
use loop_td::replay_io;
use warbase::bundle::{assemble, build_grid, load_bundle, MapBundle};
use warbase::prelude::*;
use warbase::testing::test_app;

const LOOP_MAP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/maps/loop.ron");

fn grunt(id: &str) -> UnitDef {
    UnitDef {
        leak: 1,
        id: id.into(),
        attrs: BaseAttributes { hp: 40.0, move_speed: 4.0, body_radius: 0.3 },
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
    skill_levels: vec![],
    }
}

/// Fast-loss scenario for the plugin test: 3 grunts circle a tiny closed
/// loop and the stockpile (cap 2) fills within a second.
fn fast_loss_bundle() -> MapBundle {
    let mut units = std::collections::HashMap::new();
    units.insert("grunt".into(), grunt("grunt"));
    let mut waves = WaveTable::default();
    waves.waves.push(WaveEntry {
        spawns: vec![SpawnSpec {
            unit: "grunt".into(),
            count: 3,
            interval: 0.2,
            path: 0,
            delay: 0.0,
            hp_scale: 1.0,
        }],
        boss_timeout: None,
        preview: "3 grunts".into(),
        reward_gold: 5,
    });
    waves.advance = Advance::Auto { intermission: 1.0 };
    MapBundle {
        identity: warbase::bundle::MapIdentity { id: "rpl2-fast".into(), version: 1 },
        world: warbase::bundle::WorldDef {
            paths: vec![PathDef::closed_loop(vec![
                [-1.0, -1.0],
                [1.0, -1.0],
                [1.0, 1.0],
                [-1.0, 1.0],
            ])],
            build: warbase::bundle::BuildDomainDef::FreeGrid { cell: 1.0, cols: 8, rows: 8 },
            blocked: vec![],
        },
        units,
        waves,
        rules: MatchRules {
            leak: LeakPolicy::Stockpile { cap: 2 },
            build_window: BuildWindow::Always,
            lap_buff: None,
            // 1 = the table's only wave (validation: win must not exceed
            // the wave table); the stockpile loss fires first anyway.
            win: WinCondition::SurviveWaves { waves: 1 },
            sell_refund_pct: 0.7,
        },
        econ_start: EconStart { gold: 100, wood: 0, pop_cap: 4 },
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

/// UT-RPL2-01 (acceptance ③+④): a headless driven loop.ron match, recorded
/// through the production arm/write functions, lands as `runs-style`
/// `<ts>.jsonl` that parses back to a `MatchInputLog`; the `--replay` entry
/// reconciles it — the replay's state-hash chain equals the recorded one at
/// every checkpoint. Also pins the econ filter: engine-internal wave
/// rewards / early-call bonuses are recorded by the base recorder but
/// dropped at write time (replay re-derives them).
#[test]
fn record_then_replay_reconciles() {
    let map_path = std::path::PathBuf::from(LOOP_MAP);
    let bundle = load_bundle(&map_path).expect("loop.ron loads");
    let seed = 20261022u64;
    let mut app = test_app();
    app.insert_resource(MatchRng::seeded(seed));
    assemble(&mut app.world_mut(), &bundle);
    app.insert_resource(build_grid(&bundle.world));
    replay_io::arm_recording(app.world_mut(), &map_path).expect("recording armed");

    // Scripted player inputs, written between updates (real-input timing):
    // two picks plus an early wave start (the early call grants +25 — an
    // ENGINE econ write, present raw, filtered on write, re-derived on
    // replay). 600 frames ≈ 10s: the early wave spawns its grunts so the
    // fingerprint chain moves (spawned Hp pairs), without reaching the end.
    for frame in 1..=600u32 {
        let world = app.world_mut();
        match frame {
            30 => {
                world.write_message(PickAttempt {
                    world: Vec2::new(0.0, 0.0),
                    button: PickButton::Left,
                    click_count: 1,
                    ctrl: false,
                    shift: false,
                });
            }
            61 => {
                world.write_message(StartWave);
            }
            90 => {
                world.write_message(PickAttempt {
                    world: Vec2::new(2.5, -1.5),
                    button: PickButton::Right,
                    click_count: 2,
                    ctrl: true,
                    shift: false,
                });
            }
            _ => {}
        }
        app.update();
    }
    let rec = app
        .world_mut()
        .remove_resource::<RecordingLog>()
        .expect("recording was active");
    assert!(!rec.log.frames.is_empty(), "inputs were recorded");
    let raw_engine_econ = rec.log.frames.iter().any(|(_, envs)| {
        envs.iter()
            .any(|e| matches!(e, CommandEnvelope::Ext { kind, .. } if kind == ECON_KIND))
    });
    assert!(
        raw_engine_econ,
        "engine-internal econ (early call bonus / wave rewards) rides the raw stream"
    );

    let dir = std::env::temp_dir().join(format!("loop_td_rpl2_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = replay_io::write_input_log(&dir, &rec.log).expect("run file written");
    assert_eq!(
        path.extension().and_then(|e| e.to_str()),
        Some("jsonl"),
        "runs use the <timestamp>.jsonl naming"
    );

    let text = std::fs::read_to_string(&path).expect("file readable");
    let back: MatchInputLog =
        ron::from_str(&text).expect("jsonl payload parses back as a RON MatchInputLog");
    assert_eq!(back.version, INPUT_LOG_VERSION);
    assert_eq!(back.seed, seed);
    assert_eq!(back.hashes, rec.log.hashes, "hash chain copied verbatim");
    assert!(
        back.frames.iter().all(|(_, envs)| {
            !envs.iter()
                .any(|e| matches!(e, CommandEnvelope::Ext { kind, .. } if kind == ECON_KIND))
        }),
        "engine econ is filtered at write time (replay re-derives it)"
    );
    assert!(
        back.frames
            .iter()
            .any(|(_, envs)| envs.iter().any(|e| matches!(e, CommandEnvelope::WaveStart))),
        "the player wave start survives the filter"
    );

    // The --replay entry: map fingerprint check + base replay + hash
    // reconciliation.
    let chain = replay_io::replay_file(&path, &map_path).expect("replay reconciles");
    assert_eq!(chain, rec.log.hashes, "every checkpoint matches the recording");
    let distinct: std::collections::HashSet<u64> = chain.iter().map(|(_, h)| *h).collect();
    assert!(distinct.len() >= 2, "chain moved during the match: {chain:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// UT-RPL2-02 (sensitivity): the entry refuses a tampered seed at the very
/// first checkpoint and a wrong map at the fingerprint check — "recording
/// authenticity" means both are verified.
#[test]
fn replay_rejects_tampered_seed_and_wrong_map() {
    let map_path = std::path::PathBuf::from(LOOP_MAP);
    let bundle = load_bundle(&map_path).expect("loop.ron loads");
    let mut app = test_app();
    app.insert_resource(MatchRng::seeded(4242));
    assemble(&mut app.world_mut(), &bundle);
    app.insert_resource(build_grid(&bundle.world));
    replay_io::arm_recording(app.world_mut(), &map_path).expect("recording armed");
    for frame in 1..=120u32 {
        if frame == 10 {
            app.world_mut().write_message(StartWave);
        }
        app.update();
    }
    let rec = app
        .world_mut()
        .remove_resource::<RecordingLog>()
        .expect("recording was active");
    let dir = std::env::temp_dir().join(format!("loop_td_rpl2b_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let good = replay_io::write_input_log(&dir, &rec.log).expect("written");
    assert!(replay_io::replay_file(&good, &map_path).is_ok(), "untouched log reconciles");

    // Tampered seed: first checkpoint (frame 0) must fail.
    let mut tampered = rec.log.clone();
    tampered.seed ^= 1;
    let tampered_path = dir.join("tampered.jsonl");
    std::fs::write(&tampered_path, ron::to_string(&tampered).expect("serializes")).unwrap();
    let err = replay_io::replay_file(&tampered_path, &map_path)
        .expect_err("a mis-seeded replay must not reconcile");
    assert!(err.contains("frame 0"), "error names the first divergent frame: {err}");

    // Wrong map: the fingerprint gate refuses before any simulation.
    let wrong_map =
        std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/maps/maze-smoke.ron"));
    let err = replay_io::replay_file(&good, &wrong_map)
        .expect_err("a mismatched map must be refused");
    assert!(err.contains("fingerprint"), "error names the map check: {err}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// UT-RPL2-03 (the --record wiring): the plugin arms at Startup and writes
/// the finished run into runs/ when the match ends — win or lose. Uses a
/// fast-loss scenario; asserts the file's existence/parse only (the plugin
/// adds systems to the recording app, so hash reconciliation is claimed by
/// UT-RPL2-01's plugin-free composition — see the module header).
#[test]
fn record_plugin_writes_run_at_match_end() {
    let bundle = fast_loss_bundle();
    // The plugin fingerprints the map FILE; park the scenario's RON in a
    // temp file so the log's map_hash is self-consistent.
    let map_file = std::env::temp_dir().join(format!(
        "loop_td_rpl2_map_{}.ron",
        std::process::id()
    ));
    std::fs::write(&map_file, ron::to_string(&bundle).expect("bundle serializes")).unwrap();

    let mut app = test_app();
    app.insert_resource(MatchRng::seeded(7));
    app.add_plugins(loop_td::replay_io::ReplayIoPlugin { map_path: Some(map_file.clone()) });
    assemble(&mut app.world_mut(), &bundle);
    app.insert_resource(build_grid(&bundle.world));
    app.update(); // Startup: arm recording

    let mut ended_at = None;
    for frame in 1..=600u32 {
        app.update();
        if *app.world().resource::<MatchFlow>() != MatchFlow::Running {
            ended_at = Some(frame);
            break;
        }
    }
    let frame = ended_at.expect("the fast-loss scenario ends the match");
    assert!(frame < 600, "stockpile fills quickly (cap 2 vs 3 grunts)");

    // The run landed in runs/ (gitignored), fingerprints THIS run's map
    // file, and parses back. (Older run files from previous invocations may
    // share the seed; the map hash is the discriminator.)
    let expected_hash = replay_io::map_file_hash(&map_file).expect("map hashes");
    let runs = std::path::Path::new(loop_td::replay_io::RUNS_DIR);
    let mut found: Option<std::path::PathBuf> = None;
    for entry in std::fs::read_dir(runs).expect("runs dir exists after match end") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Ok(log) = ron::from_str::<MatchInputLog>(&text) else { continue };
        if log.seed == 7 && log.map_hash == expected_hash {
            found = Some(path);
            break;
        }
    }
    let path = found.expect("the match-end writer dropped runs/<timestamp>.jsonl");
    let text = std::fs::read_to_string(&path).unwrap();
    let log: MatchInputLog = ron::from_str(&text).unwrap();
    assert_eq!(log.version, INPUT_LOG_VERSION);
    assert!(!log.hashes.is_empty(), "state fingerprints were recorded");

    // Bonus reconciliation: if schedule systems do not allocate world
    // entities in bevy 0.19, this plugin-recorded run also replays clean
    // through the --replay entry. Do not gate on it (phase-1 claim is the
    // plugin-free composition); just observe.
    match replay_io::replay_file(&path, &map_file) {
        Ok(chain) => println!("[probe] plugin-recorded run reconciles: {} checkpoints", chain.len()),
        Err(e) => println!("[probe] plugin-recorded run does not reconcile (expected if systems allocate entities): {e}"),
    }
}
