//! Headless balance readout tool (team 「设计目标锚点框架」methodology).
//!
//! This is NOT a pass/fail test of balance yet — it is the *observation
//! channel*: given a tower loadout, it simulates a real headless run and
//! reports per-wave clear time, leaks, and gold gain so that a human (or the
//! AI) can read the numbers against the §14 target ranges / playtest feedback
//! and converge the starting values. Run it with:
//!
//! `cargo test --test balance -- --nocapture`
//!
//! Assertions here only guard the *harness* (a wave resolves, metrics are
//! sane); balance anchor assertions are added once the design owner sets the
//! target ranges.

use std::time::Duration;

use bevy::{prelude::*, state::app::StatesPlugin, time::TimeUpdateStrategy};

use warden::components::{Tower, TowerKind, TowerSlot};
use warden::plugins::{
    acquisition::AcquisitionPlugin, economy::EconomyPlugin, enemies::EnemiesPlugin,
    game::GamePlugin, map::MapPlugin, meta::MetaPlugin, towers::TowersPlugin, ui::UiPlugin,
    waves::WavesPlugin,
};
use warden::resources::{BaseHp, Economy, Hand, MetaSavePath, RunRng, TowerDefs, WaveChoice, WavePhase, WaveState};
use warden::states::GameState;

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin))
        .init_state::<GameState>()
        .add_plugins((
            GamePlugin,
            MapPlugin,
            TowersPlugin,
            EnemiesPlugin,
            WavesPlugin,
            EconomyPlugin,
            AcquisitionPlugin,
            MetaPlugin,
            UiPlugin,
        ))
        .insert_resource(MetaSavePath(
            std::env::temp_dir().join("warden_meta_balance_test.txt"),
        ))
        .insert_resource(RunRng::seeded(1234))
        // Headless: no winit / asset plugins, so create the resources the
        // systems under test need (input, mesh/material asset stores).
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<bevy::input::mouse::MouseButton>>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
    app
}

/// One cleared wave's readout.
#[derive(Clone, Copy)]
struct WaveSample {
    index: u32,
    /// Game seconds from wave start to resolve (killed + leaked).
    clear_secs: f32,
    /// Base HP lost (enemies that leaked) this wave.
    leaks: u32,
    /// Gold gained during the wave (kill gold + wave reward).
    gold_gain: u32,
    /// False if the wave did not resolve within the update budget.
    resolved: bool,
}

/// Slot world positions (from the map subsystem).
fn slot_positions(app: &mut App) -> Vec<Vec3> {
    let world = app.world_mut();
    let mut q = world.query_filtered::<&Transform, With<TowerSlot>>();
    q.iter(world).map(|t| t.translation).collect()
}

/// Place `tower_type` base towers on the first `count` slots, paying their cost.
/// Returns how many were placed (stops if gold runs out).
fn place_loadout(app: &mut App, tower_type: usize, count: usize) -> usize {
    let defs = app.world().resource::<TowerDefs>().list.clone();
    let def = &defs[tower_type];
    let slots = slot_positions(app);
    let mut placed = 0;
    for pos in slots.iter().take(count) {
        let gold = app.world().resource::<Economy>().gold;
        if gold < def.cost {
            break;
        }
        *app.world_mut().resource_mut::<Economy>() = Economy { gold: gold - def.cost };
        app.world_mut().spawn((
            Tower {
                tower_index: tower_type,
                damage: def.damage,
                attack_speed: def.attack_speed,
                range: def.range,
                attack_type: def.attack_type,
                cooldown: 0.0,
                target: None,
                kind: TowerKind::Base(tower_type),
                aoe_radius: 0.0,
                slow_factor: 0.0,
                slow_duration: 0.0,
            },
            Transform::from_xyz(pos.x, 0.6, pos.z),
        ));
        placed += 1;
    }
    placed
}

/// Run `waves_remaining` waves start-to-finish and record a sample per wave.
fn run_readout(app: &mut App, waves: u32, max_updates_per_wave: u32) -> Vec<WaveSample> {
    let mut samples = Vec::new();
    for _ in 0..waves {
        // Unblock the next wave (skip the three-choose-one: baseline readout).
        app.world_mut().resource_mut::<WaveChoice>().pending = false;

        let start = app.world().resource::<Time>().elapsed_secs();
        let start_hp = app.world().resource::<BaseHp>().hp;
        let start_gold = app.world().resource::<Economy>().gold;
        let start_wave = app.world().resource::<WaveState>().current;

        // Press Space to start the wave, then advance until it resolves.
        app.world_mut()
            .resource_mut::<bevy::input::ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        let mut updates = 0u32;
        let mut resolved = false;
        while updates < max_updates_per_wave {
            app.update();
            updates += 1;
            let wave = app.world().resource::<WaveState>();
            if wave.current != start_wave && wave.phase == WavePhase::Intermission {
                resolved = true;
                break;
            }
        }
        let end = app.world().resource::<Time>().elapsed_secs();
        let hp = app.world().resource::<BaseHp>().hp;
        let gold = app.world().resource::<Economy>().gold;
        samples.push(WaveSample {
            index: start_wave,
            clear_secs: if resolved { end - start } else { 0.0 },
            leaks: start_hp.saturating_sub(hp),
            gold_gain: gold.saturating_sub(start_gold),
            resolved,
        });
    }
    samples
}

/// Balance readout smoke: with a fixed loadout, read a few waves' numbers.
/// This guards the harness (waves resolve, metrics sane), not balance values.
#[test]
fn balance_readout_smoke() {
    let mut app = test_app();
    app.update(); // startup: map/slots/hand/meta

    // Grant a generous budget so this smoke reliably clears the first waves
    // (the readout is about measuring the harness, not proving a build).
    *app.world_mut().resource_mut::<Economy>() = Economy { gold: 260 };
    let placed_archer = place_loadout(&mut app, 0, 2); // archer x2
    let placed_mage = place_loadout(&mut app, 2, 1); // mage x1
    let placed_cannon = place_loadout(&mut app, 3, 1); // cannon x1
    assert!(placed_archer + placed_mage + placed_cannon >= 3, "loadout must be affordable");

    let samples = run_readout(&mut app, 3, 4000);
    assert_eq!(samples.len(), 3, "three wave samples");

    println!("--- balance readout (loadout: {placed_archer} archer + {placed_mage} mage + {placed_cannon} cannon) ---");
    println!("wave | clear(s) | leaks | gold+");
    for s in &samples {
        println!(
            "  {}  |   {:8.2}   |  {:2}   |  {}",
            s.index, s.clear_secs, s.leaks, s.gold_gain
        );
        assert!(s.resolved, "wave {} must resolve, got a stalled run", s.index);
        assert!(s.clear_secs > 0.0, "wave {} clear time must be positive", s.index);
    }
    let base = app.world().resource::<BaseHp>();
    let gold = app.world().resource::<Economy>();
    println!(
        "aggregate: base_hp={}/{} gold={} waves_cleared={}",
        base.hp, base.max_hp, gold.gold, samples.len()
    );
    // The harness must have actually advanced the run.
    let wave = app.world().resource::<WaveState>();
    assert!(wave.current >= 3, "expected at least 3 waves consumed, got {}", wave.current);
}
