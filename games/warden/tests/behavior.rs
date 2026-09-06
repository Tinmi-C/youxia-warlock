//! Behavior consistency regression tests: pin down "changed A, B did not break".
//! Pattern: build a headless app (no renderer/window), drive it manually, assert.
//! Acceptance sentences from capability cards live here as executable tests.

use std::time::Duration;

use bevy::{
    prelude::*,
    state::app::StatesPlugin,
    time::TimeUpdateStrategy,
};

use warden::{
    components::{AttackType, Enemy, FusionKind, Healer, PlacementCursor, Tower, TowerKind},
    plugins::{
        economy::EconomyPlugin, enemies::EnemiesPlugin, game::GamePlugin, map::MapPlugin,
        towers::TowersPlugin, ui::UiPlugin, waves::WavesPlugin,
    },
    resources::{
        BaseHp, Boosts, ChoiceKind, ChoiceOption, Economy, WaveChoice, WavePhase, WaveState,
    },
    states::GameState,
};

/// Headless app: all gameplay plugins (no renderer/window) + fixed timestep.
fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin)) // init_state needs StateTransition schedule
        .init_state::<GameState>()
        .add_plugins((
            GamePlugin,
            MapPlugin,
            TowersPlugin,
            EnemiesPlugin,
            WavesPlugin,
            EconomyPlugin,
            UiPlugin,
        ))
        // Headless: no winit / asset plugins, so create the resources the
        // systems under test need (input, mesh/material asset stores).
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<bevy::input::mouse::MouseButton>>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        // Fixed timestep makes tests reproducible (1/60 s per update).
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
    app
}

/// Cursor distance from its spawn (0, 0.3, 0). Movement is constrained to the
/// XZ ground plane, so measuring the XZ length of the translation is correct.
fn cursor_displacement(app: &mut App) -> f32 {
    let mut q = app.world_mut().query::<(&PlacementCursor, &Transform)>();
    let (_, tf) = q.single(app.world()).expect("one placement cursor exists");
    tf.translation.xz().length()
}

fn enemy_hp(app: &mut App) -> f32 {
    let mut q = app.world_mut().query::<&Enemy>();
    q.single(app.world()).map(|e| e.hp).expect("one enemy exists")
}

fn count_towers(app: &mut App) -> usize {
    let mut q = app.world_mut().query::<&Tower>();
    q.iter(app.world()).count()
}

/// Capability card CursorMove — acceptance: distance == speed × elapsed time
/// (frame-rate independent; asserted against the *actually* elapsed time, so
/// first-frame clock quirks can't break it).
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
    let dist = cursor_displacement(&mut app);
    let expected = 8.0 * elapsed; // PlacementCursor.speed × actually elapsed seconds
    assert!(
        (dist - expected).abs() < 0.05,
        "expected ≈{expected:.3} units in {elapsed:.3}s, got {dist}"
    );
}

/// Capability card CursorMove — acceptance: diagonal speed is NOT speed * sqrt(2).
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
    let elapsed = app.world().resource::<Time>().elapsed_secs();
    let dist = cursor_displacement(&mut app);
    let expected = 8.0 * elapsed; // speed × actually elapsed seconds
    assert!(
        (dist - expected).abs() < 0.05,
        "diagonal got {dist}, should be ≈{expected} (normalized, elapsed={elapsed}), not {}*sqrt(2)",
        8.0
    );
}

/// Capability card GameState — acceptance: paused state freezes movement.
#[test]
fn paused_state_stops_cursor() {
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
    let dist = cursor_displacement(&mut app);
    assert!(dist < 0.01, "cursor moved {dist} units while paused");
}

/// Capability card TO2 — acceptance: valid placement deducts the tower cost and
/// occupies a slot; the nearest empty slot to the cursor is used.
#[test]
fn place_tower_deducts_gold_and_occupies_slot() {
    let mut app = test_app();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyE);
    app.update();
    let gold = app.world().resource::<Economy>().gold;
    assert_eq!(gold, 50, "placing an archer (cost 50) should leave 50 gold");
    assert_eq!(count_towers(&mut app), 1, "one tower should be placed");
}

/// Capability card TO3/TO4 — acceptance: a tower within range fires at the
/// nearest enemy, dealing its damage (cooldown resets to 1/attack_speed).
#[test]
fn tower_fires_at_enemy_in_range() {
    let mut app = test_app();
    app.world_mut().spawn((
        Tower {
            tower_index: 0,
            damage: 10.0,
            attack_speed: 1.0,
            range: 20.0,
            attack_type: AttackType::Physical,
            cooldown: 0.0,
            target: None,
            kind: TowerKind::Base(0),
            aoe_radius: 0.0,
            slow_factor: 0.0,
            slow_duration: 0.0,
        },
        Transform::from_xyz(0.0, 0.5, 0.0),
    ));
    app.world_mut().spawn((
        Enemy {
            hp: 50.0,
            max_hp: 50.0,
            speed: 0.0,
            leak: 0,
            kill_gold: 0,
            physical_armor: false,
            next_wp: 0,
        },
        Transform::from_xyz(2.0, 0.5, 0.0),
    ));
    app.update();
    let hp = enemy_hp(&mut app);
    assert!((hp - 40.0).abs() < 0.01, "one hit should reduce hp to 40, got {hp}");
}

/// Capability card EN2 — acceptance: an enemy that reaches the base deducts its
/// leak amount from base HP and despawns.
#[test]
fn leaked_enemy_deducts_base_hp() {
    let mut app = test_app();
    app.world_mut().spawn((
        Enemy {
            hp: 10.0,
            max_hp: 10.0,
            speed: 0.0,
            leak: 1,
            kill_gold: 0,
            physical_armor: false,
            next_wp: 4, // == waypoints.len(), so it counts as reached the base
        },
        Transform::from_xyz(12.0, 0.4, -9.0),
    ));
    app.update();
    assert_eq!(
        app.world().resource::<BaseHp>().hp,
        9,
        "an ordinary leaked enemy should drop base hp 10 -> 9"
    );
}

/// Capability card WA1/WA2 — acceptance: starting a wave flips to Combat and
/// spawns enemies; once the wave resolves (queue empty, none active) the reward
/// is added and the phase returns to Intermission.
#[test]
fn wave_resolve_awards_reward_and_returns_intermission() {
    let mut app = test_app();
    {
        let mut ws = app.world_mut().resource_mut::<WaveState>();
        ws.current = 0;
        ws.phase = WavePhase::Combat;
        ws.spawn_queue.clear();
        ws.active = 0;
    }
    let before = app.world().resource::<Economy>().gold; // 100
    app.update();
    assert_eq!(
        app.world().resource::<Economy>().gold,
        before + 20,
        "clearing wave 1 should award 20 gold"
    );
    let ws = app.world().resource::<WaveState>();
    assert_eq!(ws.phase, WavePhase::Intermission, "phase resets to build window");
    assert_eq!(ws.current, 1, "wave index advances");
}

/// Kind of a field tower when exactly one exists.
fn fused_kind(app: &mut App) -> Option<TowerKind> {
    let mut q = app.world_mut().query::<&Tower>();
    let mut iter = q.iter(app.world());
    let kind = iter.next().map(|t| t.kind)?;
    if iter.next().is_some() {
        None
    } else {
        Some(kind)
    }
}

/// HP of the single non-healer enemy (filters the healer out).
fn non_healer_hp(app: &mut App) -> f32 {
    let mut q = app.world_mut().query_filtered::<&Enemy, Without<Healer>>();
    q.single(app.world()).expect("one non-healer enemy").hp
}

/// Capability card EN5 — acceptance: a living healer restores nearby damaged
/// enemies; once the healer is gone the healing stops.
#[test]
fn healer_restores_nearby_enemies_and_stops_when_gone() {
    let mut app = test_app();
    let healer = app
        .world_mut()
        .spawn((
            Enemy {
                hp: 45.0,
                max_hp: 45.0,
                speed: 0.0,
                leak: 0,
                kill_gold: 0,
                physical_armor: false,
                next_wp: 0,
            },
            Healer {
                radius: 6.0,
                period: 1.0,
                heal_per_tick: 3.0,
                tick_timer: 1.0,
            },
            Transform::from_xyz(0.0, 0.4, 0.0),
        ))
        .id();
    app.world_mut().spawn((
        Enemy {
            hp: 10.0,
            max_hp: 30.0,
            speed: 0.0,
            leak: 0,
            kill_gold: 0,
            physical_armor: false,
            next_wp: 0,
        },
        Transform::from_xyz(2.0, 0.4, 0.0),
    ));
    // ~1.17s: exactly one heal tick (period 1s) reaches the damaged ally.
    for _ in 0..70 {
        app.update();
    }
    let healed = non_healer_hp(&mut app);
    assert!(
        (healed - 13.0).abs() < 0.01,
        "one heal tick (+3) should bring hp 10 -> 13, got {healed}"
    );
    // Remove the healer: healing must stop.
    assert!(app.world_mut().despawn(healer), "healer despawned");
    for _ in 0..70 {
        app.update();
    }
    let after = non_healer_hp(&mut app);
    assert!(
        (after - healed).abs() < 1e-6,
        "healing must stop without the healer ({healed} -> {after})"
    );
}

/// Capability card TO5 — acceptance: fusing two archers yields one Marksman and
/// costs a 20% fee (of 2×archer cost), freeing one field-slot worth of towers.
#[test]
fn fusing_two_archers_creates_marksman_and_costs_fee() {
    let mut app = test_app();
    app.world_mut().resource_mut::<Economy>().gold = 200;
    for pos in [Vec3::new(-11.5, 0.6, 11.0), Vec3::new(-7.5, 0.6, 11.0)] {
        app.world_mut().spawn((
            Tower {
                tower_index: 0,
                damage: 10.0,
                attack_speed: 1.0,
                range: 9.0,
                attack_type: AttackType::Physical,
                cooldown: 0.0,
                target: None,
                kind: TowerKind::Base(0),
                aoe_radius: 0.0,
                slow_factor: 0.0,
                slow_duration: 0.0,
            },
            Transform::from_xyz(pos.x, pos.y, pos.z),
        ));
    }
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyF);
    app.update();
    assert_eq!(
        app.world().resource::<Economy>().gold,
        180,
        "fee = 20% of (50+50)=20, so 200-20=180"
    );
    assert_eq!(count_towers(&mut app), 1, "two archers fuse into one tower");
    assert_eq!(
        fused_kind(&mut app),
        Some(TowerKind::Fused(FusionKind::Marksman))
    );
}

/// Capability card WA1/WA2/AC4 — acceptance: clearing a non-final wave returns
/// to Intermission AND deals a three-choose-one (3 options) before the next wave.
#[test]
fn wave_resolve_surfaces_three_options() {
    let mut app = test_app();
    {
        let mut ws = app.world_mut().resource_mut::<WaveState>();
        ws.current = 0;
        ws.phase = WavePhase::Combat;
        ws.spawn_queue.clear();
        ws.active = 0;
    }
    app.update();
    let choice = app.world().resource::<WaveChoice>();
    assert!(choice.pending, "a three-choose-one should be pending after a wave");
    assert_eq!(choice.options.len(), 3, "exactly three options");
}

/// Capability card AC4 — acceptance: picking a stat-boost option applies +20%
/// damage to that base tower type and clears the pending choice.
#[test]
fn choosing_stat_boost_applies_damage_mult() {
    let mut app = test_app();
    {
        let mut choice = app.world_mut().resource_mut::<WaveChoice>();
        choice.options = vec![ChoiceOption {
            label: "x",
            kind: ChoiceKind::StatBoost { tower_type: 2 },
        }];
        choice.pending = true;
    }
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Digit1);
    app.update();
    let boosts = app.world().resource::<Boosts>();
    assert!((boosts.damage_mult[2] - 1.2).abs() < 1e-6, "mage damage should be +20%");
    assert!(!app.world().resource::<WaveChoice>().pending, "choice consumed");
}

/// Capability card EN4 — acceptance: physical damage is halved against an
/// armored (shield) enemy: 10 physical deals 5 (hp 90 -> 85).
#[test]
fn physical_damage_halved_vs_armored_enemy() {
    let mut app = test_app();
    app.world_mut().spawn((
        Tower {
            tower_index: 0,
            damage: 10.0,
            attack_speed: 1.0,
            range: 20.0,
            attack_type: AttackType::Physical,
            cooldown: 0.0,
            target: None,
            kind: TowerKind::Base(0),
            aoe_radius: 0.0,
            slow_factor: 0.0,
            slow_duration: 0.0,
        },
        Transform::from_xyz(0.0, 0.5, 0.0),
    ));
    app.world_mut().spawn((
        Enemy {
            hp: 90.0,
            max_hp: 90.0,
            speed: 0.0,
            leak: 0,
            kill_gold: 0,
            physical_armor: true,
            next_wp: 0,
        },
        Transform::from_xyz(2.0, 0.5, 0.0),
    ));
    app.update();
    let hp = enemy_hp(&mut app);
    assert!(
        (hp - 85.0).abs() < 0.01,
        "10 physical vs armor should deal 5 (hp 90 -> 85), got {hp}"
    );
}

/// Capability card EN4 — acceptance: magic damage ignores physical armor:
/// 15 magic deals 15 (hp 90 -> 75).
#[test]
fn magic_damage_ignores_armor() {
    let mut app = test_app();
    app.world_mut().spawn((
        Tower {
            tower_index: 2,
            damage: 15.0,
            attack_speed: 1.0,
            range: 20.0,
            attack_type: AttackType::Magic,
            cooldown: 0.0,
            target: None,
            kind: TowerKind::Base(2),
            aoe_radius: 0.0,
            slow_factor: 0.0,
            slow_duration: 0.0,
        },
        Transform::from_xyz(0.0, 0.5, 0.0),
    ));
    app.world_mut().spawn((
        Enemy {
            hp: 90.0,
            max_hp: 90.0,
            speed: 0.0,
            leak: 0,
            kill_gold: 0,
            physical_armor: true,
            next_wp: 0,
        },
        Transform::from_xyz(2.0, 0.5, 0.0),
    ));
    app.update();
    let hp = enemy_hp(&mut app);
    assert!(
        (hp - 75.0).abs() < 0.01,
        "15 magic should bypass armor entirely (hp 90 -> 75), got {hp}"
    );
}
