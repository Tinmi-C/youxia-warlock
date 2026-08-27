//! Behavior consistency regression tests: pin down "changed A, B did not break".
//! Pattern: build a headless app (no renderer/window), drive it manually, assert.
//! Acceptance sentences from capability cards live here as executable tests.

use std::time::Duration;

use bevy::{
    ecs::message::Messages, prelude::*, state::app::StatesPlugin, time::TimeUpdateStrategy,
};
use bevy_rapier3d::prelude::{Collider, NoUserData, RapierPhysicsPlugin, RigidBody, Velocity};

use wave_survival::components::{
    Attack, Chasing, Heading, Hp, Monster, MonsterKind, NovaAttack, Pickup, Player, Visual,
    WalkCycle,
};
use wave_survival::plugins::presentation::MAX_TURN_RATE_DEG;
use wave_survival::resources::{Balance, Wave};
use wave_survival::systems::heading::derive_heading;
use wave_survival::systems::nova::{NovaFired, NOVA_COOLDOWN, NOVA_DAMAGE, NOVA_RADIUS};
use wave_survival::systems::wave::{
    kinds_for_wave, runner_count, tank_count, wave_count, wave_hp, wave_speed,
};
use wave_survival::systems::{combat, contact};
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

// --- HeroPresentation helpers (card 12) ---

fn player_walk_playing(app: &mut App) -> bool {
    let mut q = app.world_mut().query::<&WalkCycle>();
    let walk = q.single(app.world()).expect("player carries WalkCycle");
    walk.playing
}

fn press_key(app: &mut App, key: KeyCode) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
}

fn release_key(app: &mut App, key: KeyCode) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(key);
}

/// Capability card 12 — acceptance sentence 2: the WalkCycle data chain tracks
/// real movement headlessly: true while WASD is held, false after release.
#[test]
fn walk_cycle_plays_only_while_moving() {
    let mut app = test_app();
    run_frames(&mut app, 2); // startup + first tracker seeding
    press_key(&mut app, KeyCode::KeyW);
    run_frames(&mut app, 3);
    assert!(
        player_walk_playing(&mut app),
        "WalkCycle.playing should be true while the player moves"
    );
    release_key(&mut app, KeyCode::KeyW);
    run_frames(&mut app, 3);
    assert!(
        !player_walk_playing(&mut app),
        "WalkCycle.playing should fall back to false once idle"
    );
}

/// Capability card 12 review decision: outside Playing nothing may keep the
/// walk flag up (the model must never moonwalk through pause/GameOver).
#[test]
fn walk_flag_clears_while_paused_even_with_keys_held() {
    let mut app = test_app();
    run_frames(&mut app, 2);
    press_key(&mut app, KeyCode::KeyW);
    run_frames(&mut app, 3);
    assert!(
        player_walk_playing(&mut app),
        "precondition: moving before pause"
    );
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Paused);
    run_frames(&mut app, 2); // keys stay held; clear_walk_on_pause owns the flag now
    assert!(
        !player_walk_playing(&mut app),
        "walk flag must clear while Paused even though W stays pressed"
    );
}

// --- MonsterFacing (card 15) ---

fn monster_heading(app: &mut App) -> Vec2 {
    let mut q = app
        .world_mut()
        .query_filtered::<(&Heading, &Transform), With<Monster>>();
    let (h, _) = q.single(app.world()).expect("one heading monster");
    h.dir
}

fn angle_between(a: Vec2, b: Vec2) -> f32 {
    let (la, lb) = (a.length(), b.length());
    if la <= f32::EPSILON || lb <= f32::EPSILON {
        return f32::INFINITY;
    }
    // signed angle via dot/cross, robust for the sub-2° assertions here
    let cos = a.dot(b) / (la * lb);
    let sin = a.perp_dot(b) / (la * lb);
    sin.atan2(cos).abs().to_degrees()
}

/// Minimal harness for the heading observer alone: no GamePlugin chain (physics
/// etc. would perturb exact displacement math), just this system + a stub.
fn heading_stub_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
    app.add_systems(Update, derive_heading);
    // no Player entity: first-sight seed falls back to +Z (Vec2::Y)
    app.world_mut()
        .spawn((Monster, Transform::from_xyz(-3.0, 0.5, 0.0)));
    app
}

#[test]
fn heading_tracks_displacement_then_freezes_when_still() {
    let mut app = heading_stub_app();
    run_frames(&mut app, 1); // seeding pass

    // March +X one small step per frame (speed >> MIN_SPEED).
    for i in 1..=30 {
        let x = -3.0 + 0.05 * i as f32;
        let mut q = app
            .world_mut()
            .query_filtered::<&mut Transform, With<Monster>>();
        for mut tf in q.iter_mut(app.world_mut()) {
            tf.translation.x = x;
        }
        app.update();
    }
    let dir = monster_heading(&mut app);
    assert!(
        angle_between(dir, Vec2::X) < 2.0,
        "after marching +X, heading should point within 2° of world +X, got {dir:?}"
    );

    // Stand still: heading must hold bit-for-bit.
    run_frames(&mut app, 5);
    assert_eq!(
        monster_heading(&mut app),
        dir,
        "stationary frames must not perturb the frozen heading"
    );

    // About-face on the DATA side is instant (smoothing lives presentation-side).
    let mut q = app
        .world_mut()
        .query_filtered::<&mut Transform, With<Monster>>();
    for mut tf in q.iter_mut(app.world_mut()) {
        tf.translation.x -= 0.05;
    }
    app.update();
    let flipped = monster_heading(&mut app);
    assert!(
        angle_between(flipped, -Vec2::X) < 2.0,
        "data-side heading flips to the new displacement immediately, got {flipped:?}"
    );
}

/// Card 15 acceptance math anchor: MAX_TURN_RATE_DEG must convert any worst-case
/// about-face (~180°) into less than the 0.6 s visual convergence deadline.
#[test]
fn max_turn_rate_covers_about_face_deadline() {
    let about_face_seconds = 180.0 / MAX_TURN_RATE_DEG;
    assert!(
        about_face_seconds > 0.0 && about_face_seconds <= 0.6,
        "about-face via wrapper smoothing must converge within 0.6 s, got {about_face_seconds:.3}s"
    );
}

// --- MonsterPresentation (card 13) ---

/// Capability card 13 — scheme C body scales are pinned so a silent drift
/// cannot desynchronize visual size language from gameplay stats.
#[test]
fn variant_visual_scales_match_scheme_c() {
    assert_eq!(MonsterKind::Grunt.visual_scale(), 1.0);
    assert_eq!(MonsterKind::Runner.visual_scale(), 0.85);
    assert_eq!(MonsterKind::Tank.visual_scale(), 1.25);
}

/// Card 13 data contract: wave-spawned monsters carry `WalkCycle { playing }`
/// so the presentation layer can animate them; the logic side stays the single
/// source of truth for when walking stops.
#[test]
fn wave_monsters_spawn_with_walk_flag_up() {
    let mut app = test_app();
    run_frames(&mut app, 220); // wave 1 arrives naturally
    let total = monster_count(&mut app);
    assert!(total > 0, "wave 1 should have spawned by now");
    let flagged = {
        let mut q = app
            .world_mut()
            .query_filtered::<&WalkCycle, With<Monster>>();
        q.iter(app.world()).filter(|w| w.playing).count()
    };
    assert_eq!(
        flagged, total,
        "every spawned monster must carry WalkCycle.playing = true"
    );
}

// --- HitFlashFeedback (card 14) ---

fn player_flash(app: &mut App) -> f32 {
    let mut q = app.world_mut().query_filtered::<&Visual, With<Player>>();
    q.single(app.world()).expect("player carries Visual").flash
}

/// Card 14 — acceptance 2 + 1 combined, driven through the REAL system chain:
/// a same-frame hit ends the update at exactly max(1 - rate*dt, 0), and the
/// value keeps decaying to an exact-zero clamp without ever going negative.
#[test]
fn flash_decays_predictably_and_clamps_at_zero() {
    let mut app = test_app();
    run_frames(&mut app, 2);
    // a monster standing on the player guarantees a same-frame bite
    spawn_monster(&mut app, 0.0, 0.0);

    run_frames(&mut app, 1);
    let expected = (1.0 - contact::FLASH_DECAY_RATE / 60.0).max(0.0);
    let got = player_flash(&mut app);
    assert!(
        (got - expected).abs() < 1e-6,
        "hit frame must end at max(1 - rate*dt, 0): got {got}, expected {expected}"
    );

    // ~0.75 s more than covers the 0.25 s fade window
    run_frames(&mut app, 45);
    let gone = player_flash(&mut app);
    assert!(
        gone == 0.0,
        "flash must clamp exactly to zero after fading, got {gone}"
    );
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
            Hp::full(100.0),
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

    assert!(
        (monster_hp(&app, near) - 66.0).abs() < 0.5,
        "d=0.5 should deal 34"
    );
    assert!(
        (monster_hp(&app, mid) - 83.0).abs() < 0.5,
        "d=1.2 should deal ~17"
    );
    assert!(
        (monster_hp(&app, far) - 100.0).abs() < 0.5,
        "d=1.6 should deal 0"
    );
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

    assert!(
        (monster_hp(&app, at_full) - 66.0).abs() < 1.0,
        "d=0.9 should deal 34"
    );
    assert!(
        (monster_hp(&app, at_far) - 100.0).abs() < 1.0,
        "d=1.5 should deal 0"
    );
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

    assert!(
        (monster_hp(&app, a) - 66.0).abs() < 0.5,
        "a (d=0.5) should take 34"
    );
    assert!(
        (monster_hp(&app, b) - 83.0).abs() < 0.5,
        "b (d=1.2) should take ~17"
    );
    // card 14 (reviewed): the chained decay_flash runs the same frame, so the
    // end-of-update value is exactly max(1 - rate*dt, 0), not the raw 1.0.
    let hit_frame = (1.0 - contact::FLASH_DECAY_RATE / 60.0).max(0.0);
    assert!(
        (app.world().entity(a).get::<Visual>().unwrap().flash - hit_frame).abs() < 1e-6,
        "a should flash on hit (decayed to {hit_frame} by frame end)"
    );
    assert!(
        (app.world().entity(b).get::<Visual>().unwrap().flash - hit_frame).abs() < 1e-6,
        "b should flash on hit (decayed to {hit_frame} by frame end)"
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
        assert!(
            (chasing.speed - 1.18).abs() < 1e-5,
            "wave 1 speed should be 1.18"
        );
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
            Hp::full(42.0),
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
    assert!(
        vel.linear.y.abs() < 1e-6,
        "no vertical motion, y={}",
        vel.linear.y
    );
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

    let start = app
        .world()
        .entity(e)
        .get::<Transform>()
        .unwrap()
        .translation;
    run_frames(&mut app, 60); // 1s
    let tf = app
        .world()
        .entity(e)
        .get::<Transform>()
        .unwrap()
        .translation;

    let moved = (start - tf).length();
    assert!(
        moved > 0.5,
        "monster should move toward the player in 1s, moved {moved}"
    );
    assert!(
        (moved - 1.18).abs() < 0.5,
        "should move ~1.18 units in 1s (speed 1.18), moved {moved}"
    );
}

// --- CombatContact tests (capability card 5) ---

fn player_hp(app: &mut App) -> f32 {
    let mut q = app.world_mut().query_filtered::<&Hp, With<Player>>();
    q.single(app.world()).unwrap().hp
}

/// CombatContact — acceptance: a monster within CONTACT_DIST bites (hp 100→85, invuln, flash).
#[test]
fn contact_bites_player() {
    let mut app = test_app();
    run_frames(&mut app, 1); // player spawned
    spawn_monster(&mut app, 0.3, 0.0); // within CONTACT_DIST 0.4

    app.update(); // contact_damage runs

    assert!((player_hp(&mut app) - 85.0).abs() < 0.01, "100 - 15 = 85");
    let (invuln, flash) = {
        let mut q = app
            .world_mut()
            .query_filtered::<(&Hp, &Visual), With<Player>>();
        let (hp, vis) = q.single(app.world()).unwrap();
        (hp.invuln, vis.flash)
    };
    assert!(invuln > 0.8, "invuln should be ~0.9, got {invuln}");
    // card 14 (reviewed): same-frame decay — see flash_decays_predictably test
    let hit_frame = (1.0 - contact::FLASH_DECAY_RATE / 60.0).max(0.0);
    assert!(
        (flash - hit_frame).abs() < 1e-6,
        "player should flash on hit (decayed to {hit_frame} by frame end)"
    );
}

/// CombatContact — acceptance: invulnerability frames prevent a second bite.
#[test]
fn contact_invuln_prevents_rebite() {
    let mut app = test_app();
    run_frames(&mut app, 1);
    spawn_monster(&mut app, 0.3, 0.0);

    app.update(); // first bite
    assert!((player_hp(&mut app) - 85.0).abs() < 0.01);

    run_frames(&mut app, 30); // 0.5s (< 0.9s invuln)
    assert!(
        (player_hp(&mut app) - 85.0).abs() < 0.01,
        "no second bite within invulnerability frames"
    );
}

/// CombatContact — acceptance: at most one bite per frame even with several monsters.
#[test]
fn contact_one_bite_per_frame() {
    let mut app = test_app();
    run_frames(&mut app, 1);
    spawn_monster(&mut app, 0.3, 0.0);
    spawn_monster(&mut app, -0.3, 0.0);

    app.update();
    assert!(
        (player_hp(&mut app) - 85.0).abs() < 0.01,
        "one bite (15), not two (30)"
    );
}

/// CombatContact — acceptance: a monster at hp <= 0 is despawned.
#[test]
fn monster_dies_and_despawns() {
    let mut app = test_app();
    run_frames(&mut app, 1);
    let e = spawn_monster(&mut app, 0.5, 0.0);
    app.world_mut().entity_mut(e).get_mut::<Hp>().unwrap().hp = 1.0; // nearly dead

    press_space(&mut app);
    app.update(); // slash (34) -> hp <= 0 -> death_despawn despawns

    assert!(
        app.world().get_entity(e).is_err(),
        "monster should be despawned"
    );
}

/// CombatContact — acceptance: player hp <= 0 flips the game to GameOver.
#[test]
fn player_death_sets_game_over() {
    let mut app = test_app();
    run_frames(&mut app, 1);

    // Kill the player directly.
    let player = {
        let mut q = app.world_mut().query_filtered::<Entity, With<Player>>();
        q.single(app.world()).unwrap()
    };
    app.world_mut()
        .entity_mut(player)
        .get_mut::<Hp>()
        .unwrap()
        .hp = -1.0;

    app.update(); // death_despawn sets NextState = GameOver
    app.update(); // StateTransition applies it

    let state = app.world().resource::<State<GameState>>().get();
    assert_eq!(*state, GameState::GameOver);
}

// --- PickupDrop tests (capability card 6) ---

fn spawn_pickup_at(app: &mut App, x: f32, z: f32, arm: f32, heal: f32) -> Entity {
    app.world_mut()
        .spawn((Pickup { heal, arm }, Transform::from_xyz(x, 0.25, z)))
        .id()
}

fn pickup_count(app: &mut App) -> usize {
    let mut q = app.world_mut().query::<&Pickup>();
    q.iter(app.world()).count()
}

/// PickupDrop — acceptance: a ready pickup within range heals the player (capped at max).
#[test]
fn pickup_heals_player_when_close() {
    let mut app = test_app();
    run_frames(&mut app, 1);

    // Damage the player first so healing is observable.
    let player = {
        let mut q = app.world_mut().query_filtered::<Entity, With<Player>>();
        q.single(app.world()).unwrap()
    };
    app.world_mut()
        .entity_mut(player)
        .get_mut::<Hp>()
        .unwrap()
        .hp = 50.0;

    spawn_pickup_at(&mut app, 0.0, 0.0, 0.0, 10.0); // ready, on the player
    app.update(); // pickup_drop heals

    assert!(
        (player_hp(&mut app) - 60.0).abs() < 0.01,
        "50 + 10 = 60, got {}",
        player_hp(&mut app)
    );
    assert_eq!(pickup_count(&mut app), 0, "pickup consumed on heal");
}

/// PickupDrop — acceptance: killing a monster drops a pickup at its position.
#[test]
fn monster_kill_drops_pickup() {
    let mut app = test_app();
    run_frames(&mut app, 1);
    let before = pickup_count(&mut app);
    let e = spawn_monster(&mut app, 1.0, 0.0);
    app.world_mut().entity_mut(e).get_mut::<Hp>().unwrap().hp = -1.0; // dead

    app.update(); // death_despawn drops a pickup + despawns the monster

    assert_eq!(
        pickup_count(&mut app),
        before + 1,
        "a pickup should be dropped"
    );
    assert!(
        app.world().get_entity(e).is_err(),
        "monster should be despawned"
    );
}

// --- GameLoop test (capability card 8) ---

/// Full vertical-slice loop: spawn → wave 1 → player dies → GameOver → R restart → Playing.
#[test]
fn game_loop_full_cycle() {
    let mut app = test_app();
    run_frames(&mut app, 220); // wave 1 spawns (~3s)
    assert_eq!(current_wave(&app), 1);
    assert_eq!(monster_count(&mut app), 3);
    assert_eq!(
        *app.world().resource::<State<GameState>>().get(),
        GameState::Playing
    );

    // Kill the player -> GameOver.
    let player = {
        let mut q = app.world_mut().query_filtered::<Entity, With<Player>>();
        q.single(app.world()).unwrap()
    };
    app.world_mut()
        .entity_mut(player)
        .get_mut::<Hp>()
        .unwrap()
        .hp = -1.0;
    app.update();
    app.update();
    assert_eq!(
        *app.world().resource::<State<GameState>>().get(),
        GameState::GameOver
    );

    // Press R -> restart -> back to Playing, wave reset, player full, monsters cleared.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyR);
    app.update();
    app.update();
    assert_eq!(
        *app.world().resource::<State<GameState>>().get(),
        GameState::Playing
    );
    assert_eq!(current_wave(&app), 0, "wave reset to 0 on restart");
    assert!(
        (player_hp(&mut app) - 100.0).abs() < 0.01,
        "player reset to full hp"
    );
    assert_eq!(monster_count(&mut app), 0, "monsters cleared on restart");
}

// --- 阶段一验收：垂直切片端到端（真实系统链完整一局） ---

/// Phase-1 acceptance: the whole slice driven through the REAL system chain —
/// wave spawns real chasing monsters, they close in and bite, held Space slays
/// them (pickups drop), field clears, a stronger wave 2 arrives, the player
/// dies, and R restarts. Mirrors docs/GDD.md acceptance: 能完整玩一局。
#[test]
fn phase1_acceptance_full_vertical_slice() {
    let mut app = test_app();

    // 1. 开场喘息结束，第 1 波到场（3 只）。
    run_frames(&mut app, 220);
    assert_eq!(current_wave(&app), 1, "wave 1 spawned");
    assert_eq!(monster_count(&mut app), 3);

    // 2. 站桩让怪贴脸（出生环半径 3、速度 1.18，约 2.6s 抵达玩家）。
    run_frames(&mut app, 130);
    assert!(player_hp(&mut app) > 0.0, "player survives the approach");

    // 3. 按住 Space 连砍至清场（每刀对范围内所有怪各 34 伤害，冷却 0.45s）。
    press_space(&mut app);
    let mut guard = 0;
    while monster_count(&mut app) > 0 && guard < 240 {
        app.update();
        guard += 1;
    }
    release_space(&mut app);
    assert!(guard < 240, "slaying should finish promptly");
    assert_eq!(monster_count(&mut app), 0, "all wave-1 monsters slain");
    assert!(
        pickup_count(&mut app) >= 1,
        "slain monsters drop golden pickups"
    );
    assert!(player_hp(&mut app) > 0.0, "player survives the melee");

    // 4. 波间喘息后第 2 波到来，更强（数量 4 / 血 54 / 速 1.26）。
    run_frames(&mut app, 200);
    assert_eq!(current_wave(&app), 2, "wave 2 after the break");
    assert_eq!(monster_count(&mut app), 4, "wave 2 = 2+2 = 4 monsters");
    let mut q = app.world_mut().query::<(&Hp, &Chasing)>();
    for (hp, chasing) in q.iter(app.world()) {
        assert!((hp.hp - 54.0).abs() < 1e-4, "wave 2 hp 30*(1+0.4*2)=54");
        assert!(
            (chasing.speed - 1.26).abs() < 1e-5,
            "wave 2 speed 1.1+0.08*2=1.26"
        );
    }

    // 5. 玩家死亡 → GameOver。
    let player = {
        let mut q = app.world_mut().query_filtered::<Entity, With<Player>>();
        q.single(app.world()).unwrap()
    };
    app.world_mut()
        .entity_mut(player)
        .get_mut::<Hp>()
        .unwrap()
        .hp = -1.0;
    app.update();
    app.update();
    assert_eq!(
        *app.world().resource::<State<GameState>>().get(),
        GameState::GameOver,
        "death flips to GameOver"
    );

    // 6. R 重开 → Playing、波次归零、满血、清场。
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyR);
    app.update();
    app.update();
    assert_eq!(
        *app.world().resource::<State<GameState>>().get(),
        GameState::Playing,
        "restart returns to Playing"
    );
    assert_eq!(current_wave(&app), 0, "wave reset to 0");
    assert!(
        (player_hp(&mut app) - 100.0).abs() < 0.01,
        "player reset to full hp"
    );
    assert_eq!(monster_count(&mut app), 0, "field cleared on restart");
}

// --- NovaSlash tests (capability card 9; the hanabi visual item is accepted by
// --- running the game — headless tests cover the logic acceptance sentences). ---

fn press_shift(app: &mut App) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ShiftLeft);
}

fn release_shift(app: &mut App) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(KeyCode::ShiftLeft);
}

/// Fire once on a fresh app: press Shift for exactly one frame. The blast writes
/// NovaFired into the message buffer of that same update.
fn fire_nova_once(app: &mut App) {
    press_shift(app);
    app.update();
    release_shift(app);
}

/// Messages buffered in the current update (len() semantics suffice here because
/// every assert reads immediately after the firing frame or measures a delta).
fn nova_messages_now(app: &App) -> usize {
    app.world().resource::<Messages<NovaFired>>().len()
}

/// NovaSlash — acceptance: cooldown throttles (2s apart → still just 1 blast;
/// ≥5s later → fires again). Every check measures a message-count DELTA around
/// its own blast so the assertion holds regardless of buffer-flush semantics.
#[test]
fn nova_respects_cooldown() {
    let mut app = test_app();
    run_frames(&mut app, 2);

    let b0 = nova_messages_now(&app);
    fire_nova_once(&mut app);
    assert_eq!(nova_messages_now(&app) - b0, 1, "first Shift fires");

    // ~2s later (< 5s CD): no second blast.
    run_frames(&mut app, 120);
    let b1 = nova_messages_now(&app);
    fire_nova_once(&mut app);
    assert_eq!(nova_messages_now(&app) - b1, 0, "still cooling down at 2s");

    // ~4 more seconds (total > 5s since the first blast): ready again.
    run_frames(&mut app, 260);
    let b2 = nova_messages_now(&app);
    fire_nova_once(&mut app);
    assert_eq!(nova_messages_now(&app) - b2, 1, "ready again after 5s");
}

/// NovaSlash — acceptance: full damage inside the radius (d=0.4 and d=1.55 →
/// −60 each), nothing beyond it (d=1.65 → unchanged); no falloff inside.
#[test]
fn nova_full_damage_inside_radius_only() {
    let mut app = test_app();
    run_frames(&mut app, 2);
    let near = spawn_monster(&mut app, 0.4, 0.0);
    let mid = spawn_monster(&mut app, 1.55, 0.0);
    let out = spawn_monster(&mut app, -1.65, 0.0);

    let b = nova_messages_now(&app);
    fire_nova_once(&mut app);
    assert_eq!(nova_messages_now(&app) - b, 1, "blast fired");

    assert!(
        (monster_hp(&app, near) - 40.0).abs() < 1e-4,
        "d=0.4: -60 flat"
    );
    assert!(
        (monster_hp(&app, mid) - 40.0).abs() < 1e-4,
        "d=1.55: -60 flat"
    );
    assert!(
        (monster_hp(&app, out) - 100.0).abs() < 1e-4,
        "d=1.65: untouched"
    );
}

/// NovaSlash — acceptance: multiple targets inside the circle all take −60 in
/// the same blast, and each hit monster flashes.
#[test]
fn nova_hits_multiple_targets_and_flashes() {
    let mut app = test_app();
    run_frames(&mut app, 2);
    let a = spawn_monster(&mut app, 0.3, 0.0);
    let b = spawn_monster(&mut app, -0.8, 0.5);
    let c = spawn_monster(&mut app, 0.6, -1.2);

    let b0 = nova_messages_now(&app);
    fire_nova_once(&mut app);
    assert_eq!(nova_messages_now(&app) - b0, 1, "exactly one blast");

    assert!((monster_hp(&app, a) - 40.0).abs() < 1e-4);
    assert!((monster_hp(&app, b) - 40.0).abs() < 1e-4);
    assert!((monster_hp(&app, c) - 40.0).abs() < 1e-4);
    for e in [a, b, c] {
        let flash = app.world().entity(e).get::<Visual>().unwrap().flash;
        // card 14 (reviewed): same-frame decay band, see the dedicated test
        let hit_frame = (1.0 - contact::FLASH_DECAY_RATE / 60.0).max(0.0);
        assert!(
            (flash - hit_frame).abs() < 1e-6,
            "hit monster flashes (decayed to {hit_frame} by frame end), got {flash}"
        );
    }
}

/// NovaSlash — acceptance: melee slash and nova throttle independently — using
/// Space does not consume the nova, and both cooldowns tick separately.
#[test]
fn nova_independent_of_melee_cooldown() {
    let mut app = test_app();
    run_frames(&mut app, 2);
    let m = spawn_monster(&mut app, 0.5, 0.0); // inside both circles

    // One melee slash: −34, Attack.cooldown rearmed.
    press_space(&mut app);
    app.update();
    release_space(&mut app);
    assert!((monster_hp(&app, m) - 66.0).abs() < 1e-4, "melee dealt 34");

    // Immediately after: Shift is NOT blocked by the melee cooldown (−60 more).
    let b = nova_messages_now(&app);
    press_shift(&mut app);
    app.update();
    release_shift(&mut app);
    assert!(
        (monster_hp(&app, m) - 6.0).abs() < 1e-4,
        "nova fired right after melee: total damage 94"
    );
    assert_eq!(nova_messages_now(&app) - b, 1, "exactly one blast message");

    let player = {
        let mut q = app.world_mut().query_filtered::<Entity, With<Player>>();
        q.single(app.world()).unwrap()
    };
    let attack = app.world().entity(player).get::<Attack>().unwrap().cooldown;
    let nova = app
        .world()
        .entity(player)
        .get::<NovaAttack>()
        .unwrap()
        .cooldown;
    // Melee was rearmed one frame earlier than nova, hence the tick difference.
    assert!(
        (attack - 0.45).abs() < 0.05 && attack < 0.45,
        "melee CD ≈0.45 s (one frame ticked), got {attack}"
    );
    assert!(
        (nova - 5.0).abs() < 1e-4,
        "nova CD rearmed to exactly 5 s, got {nova}"
    );
}

// --- EnemyVariants tests (capability card 10) ---

/// EnemyVariants — acceptance: composition formulas (runner from wave 3,
/// tank from wave 5, both capped).
#[test]
fn variant_count_formulas() {
    assert_eq!(runner_count(2), 0);
    assert_eq!(runner_count(3), 1);
    assert_eq!(runner_count(4), 1);
    assert_eq!(runner_count(5), 2);
    assert_eq!(runner_count(7), 3);
    assert_eq!(runner_count(9), 3); // capped
    assert_eq!(tank_count(4), 0);
    assert_eq!(tank_count(5), 1);
    assert_eq!(tank_count(9), 1);
    assert_eq!(tank_count(10), 2);
    assert_eq!(tank_count(15), 2); // capped
}

/// EnemyVariants — acceptance: grunt + runner + tank == wave_count(n) for
/// every n in 1..=15, all counts non-negative.
#[test]
fn variant_composition_conserves_total() {
    for n in 1..=15u32 {
        let kinds = kinds_for_wave(n);
        assert_eq!(
            kinds.len() as u32,
            wave_count(n),
            "n={n}: composition must conserve the total count"
        );
        let runners = kinds.iter().filter(|k| **k == MonsterKind::Runner).count();
        let tanks = kinds.iter().filter(|k| **k == MonsterKind::Tank).count();
        assert_eq!(runners as u32, runner_count(n));
        assert_eq!(tanks as u32, tank_count(n));
    }
}

/// EnemyVariants — acceptance: a forced wave-3 spawn carries exactly 1 Runner
/// (speed ≈1.34×1.6, hp ≈66×0.5) among standard grunts; per-kind stats hold.
#[test]
fn wave3_spawns_runner_with_kind_stats() {
    let mut app = test_app();
    run_frames(&mut app, 220); // wave 1 arrives naturally
    assert_eq!(current_wave(&app), 1);

    // Force wave 3 immediately: empty field + timer expired.
    let ids: Vec<Entity> = {
        let mut q = app.world_mut().query_filtered::<Entity, With<Monster>>();
        q.iter(app.world()).collect()
    };
    for id in ids {
        app.world_mut().despawn(id);
    }
    *app.world_mut().resource_mut::<Wave>() = Wave { n: 2, timer: -1.0 };
    run_frames(&mut app, 3);

    assert_eq!(current_wave(&app), 3, "forced spawn advanced to wave 3");
    assert_eq!(monster_count(&mut app), 5, "wave 3 = 2+3 = 5 monsters");

    // Group spawned monsters by kind and check stats against baseline × mul.
    let mut grunts: Vec<(f32, f32)> = Vec::new();
    let mut runners: Vec<(f32, f32)> = Vec::new();
    let mut tanks: Vec<(f32, f32)> = Vec::new();
    {
        let mut q = app.world_mut().query::<(&MonsterKind, &Hp, &Chasing)>();
        for (kind, hp, chasing) in q.iter(app.world()) {
            match kind {
                MonsterKind::Grunt => grunts.push((hp.hp, chasing.speed)),
                MonsterKind::Runner => runners.push((hp.hp, chasing.speed)),
                MonsterKind::Tank => tanks.push((hp.hp, chasing.speed)),
            }
        }
    }

    let base_hp = wave_hp(3); // 66
    let base_speed = wave_speed(3); // 1.34
    assert_eq!(runners.len(), 1, "wave 3 has exactly 1 runner");
    assert!(tanks.is_empty(), "no tanks before wave 5");
    assert_eq!(grunts.len(), 4, "grunt remainder conserves the total");

    for &(hp, speed) in &grunts {
        assert!((hp - base_hp).abs() < 1e-3, "grunt hp {hp} vs {base_hp}");
        assert!(
            (speed - base_speed).abs() < 1e-3,
            "grunt speed {speed} vs {base_speed}"
        );
    }
    for &(hp, speed) in &runners {
        assert!((hp - base_hp * 0.5).abs() < 1e-3, "runner hp {hp}");
        assert!(
            (speed - base_speed * 1.6).abs() < 1e-3,
            "runner speed {speed}"
        );
    }
}

/// EnemyVariants — acceptance: a forced wave-5 spawn carries exactly 1 Tank
/// (speed ≈1.50×0.6, hp ≈90×3) alongside its runners; kinds only change data.
#[test]
fn wave5_spawns_tank_with_kind_stats() {
    let mut app = test_app();
    run_frames(&mut app, 2);
    // Straight to wave 5 from an empty fresh field (tests drive the system directly).
    *app.world_mut().resource_mut::<Wave>() = Wave { n: 4, timer: -1.0 };
    run_frames(&mut app, 3);

    assert_eq!(current_wave(&app), 5);
    assert_eq!(monster_count(&mut app), 7, "wave 5 = 2+5 = 7 monsters");

    let mut grunts = 0;
    let mut runners = 0;
    let mut tanks = 0;
    {
        let mut q = app.world_mut().query::<(&MonsterKind, &Hp, &Chasing)>();
        for (kind, hp, chasing) in q.iter(app.world()) {
            match kind {
                MonsterKind::Grunt => {
                    grunts += 1;
                    assert!((hp.hp - 90.0).abs() < 1e-3);
                    assert!((chasing.speed - 1.50).abs() < 1e-3);
                }
                MonsterKind::Runner => {
                    runners += 1;
                    assert!((hp.hp - 45.0).abs() < 1e-3, "runner hp = 90*0.5");
                    assert!(
                        (chasing.speed - 2.40).abs() < 1e-3,
                        "runner speed = 1.50*1.6"
                    );
                }
                MonsterKind::Tank => {
                    tanks += 1;
                    assert!((hp.hp - 270.0).abs() < 1e-3, "tank hp = 90*3");
                    assert!((chasing.speed - 0.90).abs() < 1e-3, "tank speed = 1.50*0.6");
                }
            }
        }
    }
    assert_eq!((grunts, runners, tanks), (4, 2, 1), "wave 5 composition");
}

// --- EguiTunePanel / Balance tests (capability card 11; the F1 panel itself is
// --- a visual item accepted by running the game — headless covers Balance). ---

/// Balance — acceptance: defaults equal the GDD constants (pure value migration).
#[test]
fn balance_defaults_equal_gdd_consts() {
    let b = Balance::default();
    assert_eq!(b.slash_damage, combat::SLASH_DAMAGE);
    assert_eq!(b.slash_cooldown, combat::SLASH_COOLDOWN);
    assert_eq!(b.nova_radius, NOVA_RADIUS);
    assert_eq!(b.nova_damage, NOVA_DAMAGE);
    assert_eq!(b.nova_cooldown, NOVA_COOLDOWN);
    assert_eq!(b.contact_damage, contact::CONTACT_DAMAGE);
}

/// Balance — acceptance: retuning slash_damage changes the very next swing.
#[test]
fn balance_slash_damage_applies_live() {
    let mut app = test_app();
    run_frames(&mut app, 2);
    let m = spawn_monster(&mut app, 0.5, 0.0); // full-damage band

    app.world_mut().resource_mut::<Balance>().slash_damage = 60.0;
    press_space(&mut app);
    app.update();
    release_space(&mut app);

    assert!(
        (monster_hp(&app, m) - 40.0).abs() < 1e-4,
        "retuned slash must deal 60, hp left {}",
        monster_hp(&app, m)
    );
}

/// Balance — acceptance: retuning contact_damage changes the very next bite.
#[test]
fn balance_contact_damage_applies_live() {
    let mut app = test_app();
    run_frames(&mut app, 1);
    spawn_monster(&mut app, 0.2, 0.0); // well inside CONTACT_DIST

    app.world_mut().resource_mut::<Balance>().contact_damage = 30.0;
    run_frames(&mut app, 2);

    assert!(
        (player_hp(&mut app) - 70.0).abs() < 1e-4,
        "retuned bite must deal 30, hp now {}",
        player_hp(&mut app)
    );
}

// --- UiFormalization (card 16) ---

use wave_survival::components::{UiCooldownFill, UiNovaFill, UiPauseOverlay, UiWavePips};

fn pip_count(app: &mut App) -> usize {
    let world = app.world_mut();
    let mut container = world.query_filtered::<Entity, With<UiWavePips>>();
    let mut children = world.query::<&Children>();
    let e = container
        .single(world)
        .expect("pip container entity exists");
    // Children is removed once the last pip despawns, hence the Option-or-zero.
    children.get(world, e).map(|c| c.len()).unwrap_or(0)
}

fn fill_pct(app: &mut App, marker: &'static str) -> f32 {
    let world = app.world_mut();
    let mut cd = world.query_filtered::<&Node, With<UiCooldownFill>>();
    let mut nova = world.query_filtered::<&Node, With<UiNovaFill>>();
    let node = match marker {
        "slash" => cd.single(world).expect("slash fill"),
        _ => nova.single(world).expect("nova fill"),
    };
    match node.width {
        Val::Percent(p) => p,
        other => panic!("fill width should be Percent, got {other:?}"),
    }
}

/// Card 16 — acceptance: pips mirror the alive-monster count, including the
/// clear-to-zero moment right after a wipe (before the next wave timer lands).
#[test]
fn wave_pips_track_alive_monsters() {
    let mut app = test_app();
    run_frames(&mut app, 2);
    *app.world_mut().resource_mut::<Wave>() = Wave { n: 4, timer: -1.0 };
    run_frames(&mut app, 3);
    assert_eq!(monster_count(&mut app), 7, "wave 5 forced");

    run_frames(&mut app, 2); // sync pass + spawn-settle pass
    assert_eq!(pip_count(&mut app), 7, "pips == alive monsters");

    // Remove two monsters directly (death flow is covered by combat tests).
    let victims: Vec<Entity> = {
        let mut q = app.world_mut().query::<(Entity, &Monster)>();
        q.iter(app.world()).take(2).map(|(e, _)| e).collect()
    };
    for v in &victims {
        app.world_mut().despawn(*v);
    }
    run_frames(&mut app, 2);
    assert_eq!(pip_count(&mut app), 5, "pips drop with each kill");

    // Wipe the rest: pips hit zero on the next sync (next wave stays pending
    // behind its timer, so the zero state is observable).
    let rest: Vec<Entity> = {
        let mut q = app.world_mut().query::<(Entity, &Monster)>();
        q.iter(app.world()).map(|(e, _)| e).collect()
    };
    for v in &rest {
        app.world_mut().despawn(*v);
    }
    run_frames(&mut app, 2);
    assert_eq!(pip_count(&mut app), 0, "cleared wave shows zero pips");
}

/// Card 16 — acceptance: Nova bar mirrors (1 − cooldown/NOVA_COOLDOWN) exactly
/// like the slash bar; guards against a copy-paste reading the wrong cooldown.
/// Cooldowns decay every frame, so we seed distinct ratios, let them run, then
/// assert each bar against the LIVE value's formula (wrong-source bars diverge).
#[test]
fn cooldown_fills_track_nova_and_slash_ratios() {
    let mut app = test_app();
    run_frames(&mut app, 2);

    {
        let mut q = app
            .world_mut()
            .query_filtered::<&mut Attack, With<Player>>();
        let mut attack = q.single_mut(app.world_mut()).expect("player attack");
        attack.cooldown = combat::SLASH_COOLDOWN * 0.25; // ~75% ready seed
    }
    {
        let mut q = app
            .world_mut()
            .query_filtered::<&mut NovaAttack, With<Player>>();
        let mut nova = q.single_mut(app.world_mut()).expect("player nova");
        nova.cooldown = NOVA_COOLDOWN * 0.2; // ~80% ready seed
    }
    run_frames(&mut app, 2);

    // Freeze the clock: cooldowns stop decaying, so the fill (written by
    // ui_update, order-ambiguous vs the combat chain) and the live value we
    // read afterwards refer to the exact same tick — no lag tolerance needed.
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    run_frames(&mut app, 2);

    // live values at assertion time (frozen since the clock stopped)
    let (slash_cd, nova_cd) = {
        let world = app.world_mut();
        let mut qa = world.query_filtered::<&Attack, With<Player>>();
        let mut qn = world.query_filtered::<&NovaAttack, With<Player>>();
        let a = qa.single(world).expect("player attack");
        let n = qn.single(world).expect("player nova");
        (a.cooldown, n.cooldown)
    };
    let slash_expect = ((1.0 - slash_cd / combat::SLASH_COOLDOWN).clamp(0.0, 1.0)) * 100.0;
    let nova_expect = ((1.0 - nova_cd / NOVA_COOLDOWN).clamp(0.0, 1.0)) * 100.0;

    assert!(
        (fill_pct(&mut app, "slash") - slash_expect).abs() < 1.0,
        "slash fill {0:.2} should track its live cooldown ratio {slash_expect:.2}",
        fill_pct(&mut app, "slash")
    );
    assert!(
        (fill_pct(&mut app, "nova") - nova_expect).abs() < 1.0,
        "nova fill {0:.2} should track its live cooldown ratio {nova_expect:.2}",
        fill_pct(&mut app, "nova")
    );
}

/// Card 16 — acceptance: the pause overlay is visible exactly while Paused.
#[test]
fn pause_overlay_visibility_follows_state() {
    let mut app = test_app();
    run_frames(&mut app, 2);

    let visible = |app: &mut App| -> bool {
        let mut q = app
            .world_mut()
            .query_filtered::<&Visibility, With<UiPauseOverlay>>();
        *q.single(app.world()).expect("pause overlay") == Visibility::Visible
    };
    assert!(!visible(&mut app), "starts hidden");

    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Paused);
    run_frames(&mut app, 1);
    assert!(visible(&mut app), "visible while Paused");

    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Playing);
    run_frames(&mut app, 1);
    assert!(!visible(&mut app), "hidden again once Playing");
}
