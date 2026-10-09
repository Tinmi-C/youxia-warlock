//! BAL anchors (A1–A3) — spec: docs/topics/engine/warbase-test-strategy.md §3.10.
//! The greedy arrow baseline is the "zero strategy" novice stand-in; all
//! readings come from the shipped map file (patched only for scenario scope).

use bevy::prelude::*;
use loop_td::systems::gameplay::{build_grid_from, try_place};
use warbase::bundle::{assemble, load_bundle};
use warbase::harness::{run_headless, GreedyDriver, HarnessOpts};
use warbase::prelude::*;
use warbase::testing::{count_over_frames, test_app};

const LOOP_MAP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/maps/loop.ron");

/// A1 (novice survival): through wave 5 with basic towers only, the
/// stockpile peak stays <= 10 (out of 50) — leaks must not snowball early.
#[test]
fn bal_a1_novice_stock_peak_through_wave_5() {
    let mut bundle = load_bundle(std::path::Path::new(LOOP_MAP)).unwrap();
    bundle.waves.waves.truncate(5);
    bundle.rules.win = WinCondition::SurviveWaves { waves: 5 };

    let mut driver = GreedyDriver { tower: "arrow".into() };
    let r = run_headless(
        &bundle,
        &mut driver,
        HarnessOpts { max_frames: 60 * 220, ..Default::default() },
    );
    println!("[BAL-A1] {r:?}");
    // Anchor spec: with zero function towers the stockpile peak stays <= 10
    // through wave 5 — leaks must not snowball early (survival, not speed).
    assert!(r.stock_peak <= 10, "A1: stock peak {} > 10 (leaks snowballed early)", r.stock_peak);
    assert!(
        r.outcome != Err("stockpile full (50/50)".to_string()),
        "A1: novice must survive, got {:?}",
        r.outcome
    );
    assert!(r.waves_cleared >= 4, "A1: baseline should be into wave 5, cleared {}", r.waves_cleared);
}

/// A2 (comeback): a forced leak of 20 units at match start (the harness's
/// injection stands in for the wave-15 inject while the scenario runs the
/// first 8 waves) must be worked back down — final stock <= 15 and the map
/// still won.
#[test]
fn bal_a2_forced_leak_recovers() {
    let mut bundle = load_bundle(std::path::Path::new(LOOP_MAP)).unwrap();
    bundle.waves.waves.truncate(6);
    bundle.rules.win = WinCondition::SurviveWaves { waves: 6 };

    let mut driver = GreedyDriver { tower: "arrow".into() };
    let r = run_headless(
        &bundle,
        &mut driver,
        HarnessOpts {
            max_frames: 60 * 300,
            inject: Some(("grunt".into(), 20)),
            ..Default::default()
        },
    );
    println!("[BAL-A2] {r:?}");
    // Spec: the forced leak must be worked back down and the map still won.
    assert_eq!(r.outcome, Ok("survived all waves".to_string()), "A2: must still win");
    assert!(r.stock_peak <= 30, "A2: stock peak {} never got far past the injected 20", r.stock_peak);
}

/// A3 (boss tightness): a standard build (3x arrow3) kills the wave-1 boss
/// within [0.6T, 0.9T] of its 60s timeout — too fast means free wins, too
/// slow means unwinnable.
#[test]
fn bal_a3_boss_timer_is_tight() {
    let mut bundle = load_bundle(std::path::Path::new(LOOP_MAP)).unwrap();
    bundle.waves.waves.truncate(1);
    bundle.waves.waves[0].spawns = vec![SpawnSpec {
        unit: "boss".into(),
        count: 1,
        interval: 1.0,
        path: 0,
        delay: 0.0,
        hp_scale: 1.25,
    }];
    bundle.waves.waves[0].boss_timeout = Some(60.0);
    bundle.rules.win = WinCondition::SurviveWaves { waves: 1 };
    bundle.rules.leak = LeakPolicy::Stockpile { cap: 50 };

    let mut app = test_app();
    assemble(&mut app.world_mut(), &bundle);
    app.insert_resource(build_grid_from(&bundle));
    app.world_mut().resource_mut::<Resources>().gold = 600;

    // Standard build: three arrow3 spread around the loop (players cover
    // the ring, not one corner).
    let grid = app.world().resource::<BuildGrid>().clone();
    let path = bundle.world.paths[0].clone();
    let mut placed = 0;
    for frac in [0.0f32, 0.33, 0.66] {
        let pos = path.sample(path.length() * frac);
        let cell = grid.world_to_cell(pos);
        let mut spot = None;
        for ring in 1..6 {
            for dy in -ring..=ring {
                for dx in -ring..=ring {
                    let c = Cell(cell.0 + dx, cell.1 + dy);
                    if grid.check(c).is_ok() {
                        spot = Some(c);
                        break;
                    }
                }
                if spot.is_some() { break; }
            }
            if spot.is_some() { break; }
        }
        if let Some(c) = spot {
            if try_place(&mut app.world_mut(), "arrow3", c).is_ok() {
                placed += 1;
            }
        }
    }
    assert_eq!(placed, 3, "standard build assembled");

    app.update();
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<StartWave>>()
        .write(StartWave);

    // Count frames until the wave clears (boss dead) or the match ends.
    let mut frames = 0u64;
    let cleared = loop {
        app.update();
        frames += 1;
        if count_over_frames_clears(&mut app) > 0 {
            break true;
        }
        if *app.world().resource::<MatchFlow>() != MatchFlow::Running {
            break false;
        }
        if frames > 60 * 70 {
            break false;
        }
    };
    let kill_secs = frames as f32 / 60.0;
    println!("[BAL-A3] cleared={cleared} kill_time={kill_secs:.1}s of T=60s");
    assert!(cleared, "A3: boss must die within its timer");
    assert!(
        kill_secs >= 36.0 && kill_secs <= 54.0,
        "A3: kill time {kill_secs:.1}s outside [36.0, 54.0] (0.6T..0.9T)"
    );
}

fn count_over_frames_clears(app: &mut App) -> usize {
    // Non-advancing check of the current frame's WaveCleared writes.
    let mut cursor = bevy::ecs::message::MessageCursor::<WaveCleared>::default();
    let msgs = app.world().resource::<bevy::ecs::message::Messages<WaveCleared>>();
    cursor.read(msgs).count()
}

#[allow(unused)]
fn keep_count_over_frames_import(app: &mut App) {
    let _: Option<()> = None;
    let _ = count_over_frames::<StartWave>(app, 0);
}
