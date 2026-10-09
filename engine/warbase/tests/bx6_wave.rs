//! BX6 acceptance: Auto/Manual advance, preview timing, early-call bonus,
//! boss timers, cleared detection.

use bevy::prelude::*;
use warbase::prelude::*;
use warbase::testing::{count_over_frames, test_app};

fn grunt_registry() -> UnitRegistry {
    let mut defs = std::collections::HashMap::new();
    defs.insert(
        "grunt".into(),
        UnitDef {
            id: "grunt".into(),
            attrs: BaseAttributes { hp: 10.0, move_speed: 0.0, body_radius: 0.3 },
            attack: None,
            armor_type: ArmorType::Light,
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
        },
    );
    UnitRegistry { defs }
}

fn armed_app(table: WaveTable) -> App {
    let mut app = test_app();
    app.insert_resource(grunt_registry())
        .insert_resource(table)
        .insert_resource(EconRules::default());
    let path = app.world_mut().spawn(PathDef::closed_loop(vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]])).id();
    app.insert_resource(Paths(vec![path]));
    app
}

fn one_grunt_wave() -> WaveEntry {
    WaveEntry {
        spawns: vec![SpawnSpec { unit: "grunt".into(), count: 1, interval: 1.0, path: 0, delay: 0.0, hp_scale: 1.0 }],
        boss_timeout: None,
        preview: "grunt pack".into(),
        reward_gold: 0,
    }
}

/// UT-WAV-03 + UT-WAV-01: preview fires at intermission entry, wave starts
/// when the countdown ends.
#[test]
fn auto_advance_previews_then_starts() {
    let mut app = armed_app(WaveTable {
        waves: vec![one_grunt_wave()],
        advance: Advance::Auto { intermission: 0.5 },
        early_call_bonus: None,
    });
    let previews = count_over_frames::<WavePreview>(&mut app, 1);
    assert_eq!(previews, 1, "preview on the first intermission frame");

    let started = count_over_frames::<WaveStarted>(&mut app, 40); // 0.5 s = 30 frames
    assert_eq!(started, 1, "auto-start after intermission");
    assert_eq!(app.world_mut().query::<&WaveTag>().iter(app.world()).count(), 1);
}

/// UT-WAV-02: early call starts now and pays the bonus.
#[test]
fn early_call_starts_wave_and_grants_bonus() {
    let mut app = armed_app(WaveTable {
        waves: vec![one_grunt_wave()],
        advance: Advance::Auto { intermission: 10.0 },
        early_call_bonus: Some(20),
    });
    for _ in 0..3 {
        app.update();
    }
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<StartWave>>()
        .write(StartWave);
    let started = count_over_frames::<WaveStarted>(&mut app, 2);
    assert_eq!(started, 1, "early call starts immediately");
    assert_eq!(app.world().resource::<Resources>().gold, 20);
}

/// UT-WAV-05: Manual never auto-starts; StartWave triggers.
#[test]
fn manual_advance_waits_for_start() {
    let mut app = armed_app(WaveTable {
        waves: vec![one_grunt_wave()],
        advance: Advance::Manual,
        early_call_bonus: None,
    });
    let started = count_over_frames::<WaveStarted>(&mut app, 120);
    assert_eq!(started, 0, "manual waits");

    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<StartWave>>()
        .write(StartWave);
    let started = count_over_frames::<WaveStarted>(&mut app, 2);
    assert_eq!(started, 1);
}

/// UT-WAV-04: boss timeout fires once for the running wave.
#[test]
fn boss_timeout_fires_once() {
    let mut entry = one_grunt_wave();
    entry.boss_timeout = Some(0.5);
    let mut app = armed_app(WaveTable {
        waves: vec![entry],
        advance: Advance::Manual,
        early_call_bonus: None,
    });
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<StartWave>>()
        .write(StartWave);
    let timeouts = count_over_frames::<BossTimedOut>(&mut app, 60); // ~1 s
    assert_eq!(timeouts, 1);
}

/// Cleared: kill the tagged unit -> WaveCleared, then Done (single wave).
#[test]
fn killing_all_wave_units_emits_cleared_and_finishes() {
    let mut app = armed_app(WaveTable {
        waves: vec![one_grunt_wave()],
        advance: Advance::Manual,
        early_call_bonus: None,
    });
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<StartWave>>()
        .write(StartWave);
    app.update();
    app.update();

    // Kill the grunt through the damage pipeline (single writer).
    let victim = app
        .world_mut()
        .query_filtered::<Entity, With<WaveTag>>()
        .single(app.world())
        .unwrap();
    let src = app.world_mut().spawn_empty().id();
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<DamageRequest>>()
        .write(DamageRequest { target: victim, source: src, amount: 999.0, attack_type: AttackType::Normal, crit: false });

    let cleared = count_over_frames::<WaveCleared>(&mut app, 10);
    assert_eq!(cleared, 1);
    assert_eq!(app.world().resource::<WaveDirector>().phase, DirectorPhase::Done);
}
