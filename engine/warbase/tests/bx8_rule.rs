//! BX8 acceptance: leak policies (Stockpile / Lives), lap buffs, boss
//! timeout loss, survive-waves win, build window.

use bevy::prelude::*;
use warbase::prelude::*;
use warbase::testing::{count_over_frames, test_app};

fn grunt_registry() -> UnitRegistry {
    let mut defs = std::collections::HashMap::new();
    defs.insert(
        "grunt".into(),
        UnitDef {
            id: "grunt".into(),
            attrs: BaseAttributes { hp: 100.0, move_speed: 1.0, body_radius: 0.3 },
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

fn spawn_attacker(app: &mut App) -> Entity {
    let reg = grunt_registry();
    let e = spawn_unit(&mut app.world_mut(), &reg, "grunt", Vec3::ZERO, Allegiance::Attacker, 1.0).unwrap();
    app.world_mut()
        .entity_mut(e)
        .insert(PathFollower { path: Entity::PLACEHOLDER, progress: 0.0, laps: 0, finished: false });
    e
}

fn ended_outcome(app: &App) -> Option<Result<String, String>> {
    let mut cursor = bevy::ecs::message::MessageCursor::<MatchEnded>::default();
    let msgs = app.world().resource::<bevy::ecs::message::Messages<MatchEnded>>();
    cursor.read(msgs).last().map(|m| m.outcome.clone())
}

/// UT-RUL-01: reaching the stockpile cap loses with a readable reason.
#[test]
fn stockpile_cap_loses() {
    let mut app = test_app();
    app.insert_resource(MatchRules {
        leak: LeakPolicy::Stockpile { cap: 2 },
        ..Default::default()
    });
    spawn_attacker(&mut app);
    spawn_attacker(&mut app);
    app.update();
    assert_eq!(
        ended_outcome(&app),
        Some(Err("stockpile full (2/2)".into()))
    );
    assert_eq!(*app.world().resource::<MatchFlow>(), MatchFlow::Lost("stockpile full (2/2)".into()));
}

/// UT-RUL-02: a real pipeline kill drops the stock count and the match
/// keeps running below the cap.
#[test]
fn stockpile_recounts_after_real_kill() {
    let mut app = test_app();
    app.insert_resource(MatchRules {
        leak: LeakPolicy::Stockpile { cap: 2 },
        ..Default::default()
    })
    .insert_resource(grunt_registry());
    let a = spawn_attacker(&mut app);
    app.update();
    assert_eq!(app.world().resource::<StockCount>().0, 1);

    // Kill through the damage pipeline (single writer) — despawn lands in
    // Cleanup before rules_tick recounts.
    let src = app.world_mut().spawn_empty().id();
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<DamageRequest>>()
        .write(DamageRequest { target: a, source: src, amount: 999.0, attack_type: AttackType::Normal, crit: false });
    app.update();
    assert_eq!(app.world().resource::<StockCount>().0, 0, "despawned attacker not counted");
    assert_eq!(*app.world().resource::<MatchFlow>(), MatchFlow::Running);
}

/// UT-RUL-03 (Lives): each Arrived costs a life; zero lives loses.
#[test]
fn lives_decrement_and_lose() {
    let mut app = test_app();
    app.insert_resource(MatchRules {
        leak: LeakPolicy::Lives { lives: 2 },
        ..Default::default()
    });
    app.update(); // rules_tick lazily inserts LivesLeft
    assert_eq!(app.world().resource::<LivesLeft>().0, 2);

    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<Arrived>>()
        .write(Arrived { unit: Entity::PLACEHOLDER });
    app.update();
    assert_eq!(app.world().resource::<LivesLeft>().0, 1);

    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<Arrived>>()
        .write(Arrived { unit: Entity::PLACEHOLDER });
    app.update();
    assert_eq!(ended_outcome(&app), Some(Err("no lives left".into())));
}

/// UT-RUL-04: lap buffs are permanent and multiplicative.
#[test]
fn lap_buff_applies_permanently() {
    let mut app = test_app();
    app.insert_resource(MatchRules {
        lap_buff: Some(LapBuffPolicy { speed_pct: 0.05, hp_pct: 0.03 }),
        ..Default::default()
    });
    let e = spawn_attacker(&mut app);
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<LapCompleted>>()
        .write(LapCompleted { unit: e, lap: 1 });
    app.update();
    let speed = app.world().get::<MoveSpeed>(e).unwrap().0;
    let hp = app.world().get::<Hp>(e).unwrap().0;
    let max = app.world().get::<MaxHp>(e).unwrap().0;
    assert!((speed - 1.05).abs() < 1e-5, "speed {speed}");
    assert!((max - 103.0).abs() < 1e-4, "max {max}");
    assert!((hp - 103.0).abs() < 1e-4, "hp gains the same amount");
}

/// UT-RUL-05: boss timeout loses the match.
#[test]
fn boss_timeout_loses() {
    let mut app = test_app();
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<BossTimedOut>>()
        .write(BossTimedOut(9));
    app.update();
    assert_eq!(ended_outcome(&app), Some(Err("boss timeout".into())));
}

/// UT-RUL-06: SurviveWaves wins on clearing the final wave.
#[test]
fn surviving_all_waves_wins() {
    let mut app = test_app();
    app.insert_resource(MatchRules {
        win: WinCondition::SurviveWaves { waves: 3 },
        ..Default::default()
    });
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<WaveCleared>>()
        .write(WaveCleared(2));
    app.update();
    assert_eq!(ended_outcome(&app), Some(Ok("survived all waves".into())));
    assert_eq!(*app.world().resource::<MatchFlow>(), MatchFlow::Won("survived all waves".into()));
}

/// Build window: PrepOnly locks during Spawning, open otherwise.
#[test]
fn build_window_prep_only_locks_during_spawning() {
    let rules = MatchRules { build_window: BuildWindow::PrepOnly, ..Default::default() };
    let mut director = WaveDirector::default();
    director.phase = DirectorPhase::Spawning;
    assert!(!building_open(&rules, &director));
    director.phase = DirectorPhase::Intermission;
    assert!(building_open(&rules, &director));
    director.phase = DirectorPhase::Idle;
    assert!(building_open(&rules, &director));
}
