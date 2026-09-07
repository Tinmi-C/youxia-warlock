//! Headless balance readout tool (team 「设计目标锚点框架」methodology).
//!
//! This is NOT a pass/fail test of balance yet — it is the *observation
//! channel*: given a sane auto-build policy, it simulates a real headless
//! run and reports per-wave clear time / leaks / gold gain, plus the ending
//! base HP, gold, and win/lose. The AI (or human) reads these numbers against
//! the §14/§15 target ranges and converges the starting values. Run with:
//!
//! `cargo test --test balance -- --nocapture`
//!
//! Assertions here only guard the *harness*; balance-range assertions are
//! added once the design owner pins the anchor targets (BAL2).

use std::time::Duration;

use bevy::prelude::*;
use bevy::{state::app::StatesPlugin, time::TimeUpdateStrategy};

use warden::components::{Tower, TowerKind, TowerSlot};
use warden::plugins::{
    acquisition::AcquisitionPlugin, economy::EconomyPlugin, enemies::EnemiesPlugin,
    game::GamePlugin, map::MapPlugin, meta::MetaPlugin, towers::TowersPlugin, ui::UiPlugin,
    waves::WavesPlugin,
};
use warden::resources::{
    BaseHp, Economy, MetaSavePath, RunRng, TowerDefs, WaveChoice, WavePhase, WaveState,
};
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
}

/// Slot world positions (from the map subsystem).
fn slot_positions(app: &mut App) -> Vec<Vec3> {
    let world = app.world_mut();
    let mut q = world.query_filtered::<&Transform, With<TowerSlot>>();
    q.iter(world).map(|t| t.translation).collect()
}

/// Place a base tower at a slot, paying its cost. Marks the slot used by bumping
/// `occupied`. Returns true if the tower was placed.
fn place_one(app: &mut App, tower_type: usize, occupied: &mut usize) -> bool {
    let defs = app.world().resource::<TowerDefs>().list.clone();
    let def = &defs[tower_type];
    let slots = slot_positions(app);
    if *occupied >= slots.len() {
        return false;
    }
    let gold = app.world().resource::<Economy>().gold;
    if gold < def.cost {
        return false;
    }
    *app.world_mut().resource_mut::<Economy>() = Economy { gold: gold - def.cost };
    let pos = slots[*occupied];
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
    *occupied += 1;
    true
}

/// A sane greedy auto-builder: spend gold at intermissions on a mixed loadout
/// (archer/mage/cannon/shield cycling) as long as slots and gold allow.
fn invest_build(app: &mut App, occupied: &mut usize) {
    let defs = app.world().resource::<TowerDefs>().list.clone();
    let order = [0usize, 2, 3, 1]; // archer, mage, cannon, shield
    let mut i = 0;
    loop {
        let cheapest = defs.iter().map(|d| d.cost).min().unwrap();
        let gold = app.world().resource::<Economy>().gold;
        if gold < cheapest || *occupied >= slot_positions(app).len() {
            break;
        }
        let t = order[i % order.len()];
        i += 1;
        if !place_one(app, t, occupied) {
            continue; // try the next type
        }
    }
}

/// Resolve a pending three-choose-one by pressing 1 (picks option 0), then a
/// single update so the choice system applies it.
fn auto_choose(app: &mut App) {
    if app.world().resource::<WaveChoice>().pending {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Digit1);
        app.update();
    }
}

/// Run `remaining` waves (each: choose -> invest -> start -> resolve) from the
/// current intermission. Returns samples plus whether the run is still alive.
fn run_waves(app: &mut App, remaining: u32, occupied: &mut usize, cap: u32) -> (Vec<WaveSample>, bool) {
    let mut samples = Vec::new();
    for _ in 0..remaining {
        if *app.world().resource::<State<GameState>>() != GameState::Playing {
            return (samples, false);
        }
        auto_choose(app);
        invest_build(app, occupied);

        let start = app.world().resource::<Time>().elapsed_secs();
        let start_hp = app.world().resource::<BaseHp>().hp;
        let start_gold = app.world().resource::<Economy>().gold;
        let start_wave = app.world().resource::<WaveState>().current;

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        let mut updates = 0u32;
        let mut resolved = false;
        while updates < cap {
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
        });
        if !resolved {
            return (samples, false);
        }
    }
    (samples, true)
}

/// Balance readout smoke (harness guard): with a comfortable budget, run a few
/// waves and read their numbers. Guards the harness, not balance values.
#[test]
fn balance_readout_smoke() {
    let mut app = test_app();
    app.update();
    *app.world_mut().resource_mut::<Economy>() = Economy { gold: 260 };
    let mut occupied = 0usize;
    invest_build(&mut app, &mut occupied);
    assert!(occupied >= 3, "loadout must be affordable to exercise the readout");

    let (samples, alive) = run_waves(&mut app, 3, &mut occupied, 4000);
    assert!(alive, "smoke run should stay alive");
    assert_eq!(samples.len(), 3, "three wave samples");

    println!("--- balance readout smoke (occupied {occupied} towers) ---");
    println!("wave | clear(s) | leaks | gold+");
    for s in &samples {
        println!(
            "  {}  |   {:8.2}   |  {:2}   |  {}",
            s.index, s.clear_secs, s.leaks, s.gold_gain
        );
        assert!(s.clear_secs > 0.0, "wave {} clear time must be positive", s.index);
    }
    let wave = app.world().resource::<WaveState>();
    assert!(wave.current >= 3, "expected at least 3 waves consumed, got {}", wave.current);
}

/// Difficulty curve readout: full run under the sane auto-build policy, from the
/// realistic 100 starting gold. Reports per-wave leaks and the win/lose result.
#[test]
fn full_run_difficulty_curve() {
    let mut app = test_app();
    app.update();
    let mut occupied = 0usize;
    invest_build(&mut app, &mut occupied); // spend the 100 starting gold

    let (samples, _alive) = run_waves(&mut app, 10, &mut occupied, 6000);
    let base = app.world().resource::<BaseHp>();
    let gold = app.world().resource::<Economy>();
    let state = *app.world().resource::<State<GameState>>().get();
    println!("--- full run difficulty curve (auto-build, start 100g) ---");
    println!("wave | clear(s) | leaks | gold+ | towers={occupied}");
    for s in &samples {
        println!(
            "  {}  |   {:8.2}   |  {:2}   |  {}",
            s.index, s.clear_secs, s.leaks, s.gold_gain
        );
    }
    println!(
        "result: state={state:?} waves_cleared={} base_hp={}/{} gold={}",
        samples.len(),
        base.hp,
        base.max_hp,
        gold.gold
    );
    // Harness sanity only: at least one wave must have been consumed and a
    // sample produced. Difficulty is the *report*, not an assertion here — the
    // balance-range assertions live in a future BAL2 anchor test.
    let wave = app.world().resource::<WaveState>();
    assert!(
        !samples.is_empty(),
        "harness must produce at least one wave sample, got {}",
        samples.len()
    );
    println!("(wave.current={} samples.len={})", wave.current, samples.len());
}
