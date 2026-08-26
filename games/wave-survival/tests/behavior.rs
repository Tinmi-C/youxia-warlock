//! Behavior consistency regression tests: pin down "changed A, B did not break".
//! Pattern: build a headless app (no renderer/window), drive it manually, assert.
//! Acceptance sentences from capability cards live here as executable tests.

use std::time::Duration;

use bevy::{
    prelude::*,
    state::app::StatesPlugin,
    time::TimeUpdateStrategy,
};

use wave_survival::components::{Hp, Monster, Player, Visual};
use wave_survival::{plugins::game::GamePlugin, states::GameState};

/// Headless app: MinimalPlugins (no renderer/window) + game logic + fixed timestep.
fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin)) // init_state needs StateTransition schedule
        .init_state::<GameState>()
        .add_plugins(GamePlugin)
        // Headless: no winit / asset plugins, so create the resources the
        // systems under test need (input, mesh/material asset stores).
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        // Fixed timestep makes tests reproducible (1/60 s per update).
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
    app
}

fn player_distance(app: &mut App) -> f32 {
    let mut q = app.world_mut().query::<(&Player, &Transform)>();
    let (_, tf) = q.single(app.world()).expect("one player exists");
    tf.translation.distance(Vec3::new(0.0, 0.5, 0.0)) // spawn point
}

/// Capability card PlayerMove — acceptance: distance == speed × elapsed time
/// (frame-rate independent; asserted against the *actually* elapsed time, not an
/// assumed frame count, so first-frame clock quirks can't break it).
#[test]
fn straight_move_distance_equals_speed_times_time() {
    let mut app = test_app();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyW);
    for _ in 0..60 {
        app.update();
    }
    let elapsed = app.world().resource::<Time>().elapsed_secs();
    let dist = player_distance(&mut app);
    let expected = 5.0 * elapsed; // Player.speed × actually elapsed seconds
    assert!(
        (dist - expected).abs() < 0.05,
        "expected ≈{expected:.3} units in {elapsed:.3}s, got {dist}"
    );
}

/// Capability card PlayerMove — acceptance: diagonal speed is NOT speed * sqrt(2).
#[test]
fn diagonal_move_is_not_faster() {
    let mut app = test_app();
    let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    keys.press(KeyCode::KeyW);
    keys.press(KeyCode::KeyD);
    drop(keys);
    for _ in 0..60 {
        app.update();
    }
    let dist = player_distance(&mut app);
    let expected = 5.0;
    assert!(
        (dist - expected).abs() < 0.1,
        "diagonal got {dist}, should be ≈{expected} (normalized), not {expected}*sqrt(2)"
    );
}

/// Capability card GameState — acceptance: paused state freezes movement.
#[test]
fn paused_state_stops_movement() {
    let mut app = test_app();
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Paused);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyW);
    for _ in 0..60 {
        app.update();
    }
    let dist = player_distance(&mut app);
    assert!(dist < 0.01, "player moved {dist} units while paused");
}

// --- PlayerAttack helpers (capability card 2) ---

fn run_frames(app: &mut App, n: usize) {
    for _ in 0..n {
        app.update();
    }
}

fn press_space(app: &mut App) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Space);
}

fn release_space(app: &mut App) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(KeyCode::Space);
}

/// Spawn a stub monster at (x, 0.5, z). The player spawns at (0, 0.5, 0), so
/// `x` is the horizontal distance to the player.
fn spawn_monster(app: &mut App, x: f32, z: f32) -> Entity {
    app.world_mut()
        .spawn((
            Monster,
            Hp { hp: 100.0 },
            Visual { flash: 0.0 },
            Transform::from_xyz(x, 0.5, z),
        ))
        .id()
}

fn monster_hp(app: &App, e: Entity) -> f32 {
    app.world().entity(e).get::<Hp>().unwrap().hp
}

/// PlayerAttack — acceptance: cooldown throttles (0.3s apart → 1 hit; 0.5s → 2 hits).
#[test]
fn attack_respects_cooldown() {
    let mut app = test_app();
    run_frames(&mut app, 1); // run Startup (spawns the player)
    let e = spawn_monster(&mut app, 0.5, 0.0);

    press_space(&mut app);
    app.update();
    release_space(&mut app);
    assert!(
        (monster_hp(&app, e) - 66.0).abs() < 0.01,
        "first slash should deal 34, got hp {}",
        monster_hp(&app, e)
    );

    run_frames(&mut app, 18); // +0.3s (cooldown 0.45 not elapsed)
    press_space(&mut app);
    app.update();
    release_space(&mut app);
    assert!(
        (monster_hp(&app, e) - 66.0).abs() < 0.01,
        "0.3s later the slash must be blocked by cooldown"
    );

    run_frames(&mut app, 30); // +0.5s (cooldown elapsed)
    press_space(&mut app);
    app.update();
    release_space(&mut app);
    assert!(
        (monster_hp(&app, e) - 32.0).abs() < 0.01,
        "0.5s later the slash should deal 34 again"
    );
}

/// PlayerAttack — acceptance: damage by distance (0.5 → 34; 1.2 → ~17; 1.6 → 0).
#[test]
fn slash_damage_by_distance() {
    let mut app = test_app();
    run_frames(&mut app, 1);
    let near = spawn_monster(&mut app, 0.5, 0.0);
    let mid = spawn_monster(&mut app, 1.2, 0.0);
    let far = spawn_monster(&mut app, 1.6, 0.0);

    press_space(&mut app);
    app.update();

    assert!((monster_hp(&app, near) - 66.0).abs() < 0.5, "d=0.5 should deal 34");
    assert!((monster_hp(&app, mid) - 83.0).abs() < 0.5, "d=1.2 should deal ~17");
    assert!((monster_hp(&app, far) - 100.0).abs() < 0.5, "d=1.6 should deal 0");
}

/// PlayerAttack — acceptance: falloff boundaries (0.9 → 34; 1.5 → 0).
#[test]
fn slash_falloff_boundaries() {
    let mut app = test_app();
    run_frames(&mut app, 1);
    let at_full = spawn_monster(&mut app, 0.9, 0.0);
    let at_far = spawn_monster(&mut app, 1.5, 0.0);

    press_space(&mut app);
    app.update();

    assert!((monster_hp(&app, at_full) - 66.0).abs() < 1.0, "d=0.9 should deal 34");
    assert!((monster_hp(&app, at_far) - 100.0).abs() < 1.0, "d=1.5 should deal 0");
}

/// PlayerAttack — acceptance: multiple targets each take distance-correct damage and flash.
#[test]
fn slash_hits_multiple_targets_and_flashes() {
    let mut app = test_app();
    run_frames(&mut app, 1);
    let a = spawn_monster(&mut app, 0.5, 0.0);
    let b = spawn_monster(&mut app, 1.2, 0.0);

    press_space(&mut app);
    app.update();

    assert!((monster_hp(&app, a) - 66.0).abs() < 0.5, "a (d=0.5) should take 34");
    assert!((monster_hp(&app, b) - 83.0).abs() < 0.5, "b (d=1.2) should take ~17");
    assert!(
        app.world().entity(a).get::<Visual>().unwrap().flash > 0.99,
        "a should flash on hit"
    );
    assert!(
        app.world().entity(b).get::<Visual>().unwrap().flash > 0.99,
        "b should flash on hit"
    );
}
