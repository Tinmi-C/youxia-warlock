//! Behavior consistency regression tests: pin down "changed A, B did not break".
//! Pattern: build a headless app (no renderer/window), drive it manually, assert.
//! Acceptance sentences from capability cards live here as executable tests.

use std::time::Duration;

use bevy::{
    prelude::*,
    state::app::StatesPlugin,
    time::TimeUpdateStrategy,
};
use bevy_rapier3d::prelude::{Collider, NoUserData, RapierPhysicsPlugin, RigidBody, Velocity};

use wave_survival::components::{Chasing, Hp, Monster, Player, Visual};
use wave_survival::resources::Wave;
use wave_survival::systems::wave::{wave_count, wave_hp, wave_speed};
use wave_survival::{plugins::game::GamePlugin, states::GameState};

/// Headless app: MinimalPlugins (no renderer/window) + game logic + fixed timestep.
fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        StatesPlugin,
        TransformPlugin,
        RapierPhysicsPlugin::<NoUserData>::default(),
    )) // init_state needs StateTransition schedule
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

// --- WaveSystem tests (capability card 3) ---

fn monster_count(app: &mut App) -> usize {
    let mut q = app.world_mut().query_filtered::<(), With<Monster>>();
    q.iter(app.world()).count()
}

fn current_wave(app: &App) -> u32 {
    app.world().resource::<Wave>().n
}

/// WaveSystem — acceptance: mixed-growth formulas (count 2+n / speed 1.1+0.08n / hp 30*(1+0.4n)).
#[test]
fn wave_formulas() {
    assert_eq!(wave_count(1), 3);
    assert_eq!(wave_count(5), 7);
    assert!((wave_speed(1) - 1.18).abs() < 1e-5);
    assert!((wave_speed(10) - 1.9).abs() < 1e-5);
    assert!((wave_hp(1) - 42.0).abs() < 1e-4);
    assert!((wave_hp(5) - 90.0).abs() < 1e-4);
}

/// WaveSystem — acceptance: first wave is delayed by WAVE_BREAK, then spawns 2+1 = 3.
#[test]
fn first_wave_spawns_after_break() {
    let mut app = test_app();
    run_frames(&mut app, 60); // 1s: well before the 3s break ends
    assert_eq!(current_wave(&app), 0, "no wave before the initial break");
    assert_eq!(monster_count(&mut app), 0);

    run_frames(&mut app, 200); // total ~4.3s: break elapsed + spawn landed
    assert_eq!(current_wave(&app), 1);
    assert_eq!(monster_count(&mut app), 3, "wave 1 = 2+1 = 3 monsters");
}

/// WaveSystem — acceptance: 3s rest between waves (no respawn while resting).
#[test]
fn wave_rests_between_waves() {
    let mut app = test_app();
    run_frames(&mut app, 240); // wave 1 spawned
    assert_eq!(current_wave(&app), 1);

    // Clear the field (CombatContact despawn is a later card; simulate it here).
    let ids: Vec<Entity> = {
        let mut q = app.world_mut().query_filtered::<Entity, With<Monster>>();
        q.iter(app.world()).collect()
    };
    for id in ids {
        app.world_mut().despawn(id);
    }
    assert_eq!(monster_count(&mut app), 0);

    run_frames(&mut app, 60); // ~1s into the 3s rest
    assert_eq!(current_wave(&app), 1, "no wave 2 during the 3s rest");
    assert_eq!(monster_count(&mut app), 0);

    run_frames(&mut app, 200); // rest elapsed
    assert_eq!(current_wave(&app), 2);
    assert_eq!(monster_count(&mut app), 4, "wave 2 = 2+2 = 4 monsters");
}

/// WaveSystem — acceptance: monsters carry per-wave Hp and chase speed.
#[test]
fn wave_monsters_carry_per_wave_stats() {
    let mut app = test_app();
    run_frames(&mut app, 240); // wave 1 spawned (hp 42, speed 1.18)

    let mut q = app.world_mut().query::<(&Hp, &Chasing)>();
    let mut seen = 0;
    for (hp, chasing) in q.iter(app.world()) {
        assert!((hp.hp - 42.0).abs() < 1e-4, "wave 1 hp should be 42");
        assert!((chasing.speed - 1.18).abs() < 1e-5, "wave 1 speed should be 1.18");
        seen += 1;
    }
    assert_eq!(seen, 3, "3 wave-1 monsters");
}

// --- EnemyChase tests (capability card 4) ---

/// Spawn a chasing monster at (x, 0.5, 0) with the given chase speed.
fn spawn_chaser(app: &mut App, x: f32, speed: f32) -> Entity {
    app.world_mut()
        .spawn((
            Monster,
            Chasing { speed },
            Hp { hp: 42.0 },
            Visual { flash: 0.0 },
            RigidBody::KinematicVelocityBased,
            Collider::ball(0.3),
            Velocity::zero(),
            Transform::from_xyz(x, 0.5, 0.0),
        ))
        .id()
}

/// EnemyChase — acceptance: velocity points at the player on the XZ plane at Chasing.speed.
#[test]
fn enemy_chase_sets_velocity_toward_player() {
    let mut app = test_app();
    run_frames(&mut app, 1); // Startup spawns the player at the origin
    let e = spawn_chaser(&mut app, 2.0, 1.18);

    app.update(); // enemy_chase runs, writes velocity

    let vel = app.world().entity(e).get::<Velocity>().unwrap();
    // Player at origin, monster at +X -> velocity points -X.
    assert!(
        vel.linear.x < -1.0,
        "should move toward the player (-X), x={}",
        vel.linear.x
    );
    assert!(vel.linear.y.abs() < 1e-6, "no vertical motion, y={}", vel.linear.y);
    assert!(vel.linear.z.abs() < 1e-6, "no Z motion, z={}", vel.linear.z);
    assert!(
        (vel.linear.length() - 1.18).abs() < 1e-3,
        "speed should be 1.18, got {}",
        vel.linear.length()
    );
}

/// EnemyChase — acceptance: the monster actually moves toward the player over time.
#[test]
fn enemy_chase_moves_monster_toward_player() {
    let mut app = test_app();
    run_frames(&mut app, 1);
    let e = spawn_chaser(&mut app, 2.0, 1.18);

    let start = app.world().entity(e).get::<Transform>().unwrap().translation;
    run_frames(&mut app, 60); // 1s
    let tf = app.world().entity(e).get::<Transform>().unwrap().translation;

    let moved = (start - tf).length();
    assert!(moved > 0.5, "monster should move toward the player in 1s, moved {moved}");
    assert!(
        (moved - 1.18).abs() < 0.5,
        "should move ~1.18 units in 1s (speed 1.18), moved {moved}"
    );
}
