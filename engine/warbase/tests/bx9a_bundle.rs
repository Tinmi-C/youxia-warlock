//! BX9a acceptance: RON loading, rejection cases (bad syntax, dangling
//! references, version mismatch), and assemble wiring a runnable match.

use bevy::prelude::*;
use warbase::bundle::{assemble, load_bundle, validate, MapBundle, MAP_FORMAT_VERSION};
use warbase::prelude::*;
use warbase::testing::test_app;

/// A minimal valid bundle used as the mutation base for rejection cases.
fn sample_bundle() -> MapBundle {
    let mut units = std::collections::HashMap::new();
    units.insert(
        "grunt".to_string(),
        UnitDef {
            id: "grunt".into(),
            attrs: BaseAttributes { hp: 50.0, move_speed: 1.0, body_radius: 0.3 },
            attack: None,
            armor_type: ArmorType::Light,
            cost_gold: 0,
            cost_wood: 0,
            bounty_gold: 3,
            bounty_wood: 0,
            grant_xp: 1,
            traits: vec![],
        },
    );
    MapBundle {
        identity: warbase::bundle::MapIdentity { id: "sample".into(), version: MAP_FORMAT_VERSION },
        world: warbase::bundle::WorldDef {
            paths: vec![PathDef::closed_loop(vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]])],
            build: warbase::bundle::BuildDomainDef::FreeGrid { cell: 1.0, cols: 16, rows: 16 },
            blocked: vec![],
        },
        units,
        waves: WaveTable {
            waves: vec![WaveEntry {
                spawns: vec![SpawnSpec { unit: "grunt".into(), count: 2, interval: 0.5, path: 0, delay: 0.0, hp_scale: 1.0 }],
                boss_timeout: None,
                preview: "2 grunts".into(),
            }],
            advance: Advance::Auto { intermission: 1.0 },
            early_call_bonus: Some(10),
        },
        rules: MatchRules {
            leak: LeakPolicy::Stockpile { cap: 10 },
            build_window: BuildWindow::Always,
            lap_buff: Some(LapBuffPolicy { speed_pct: 0.05, hp_pct: 0.03 }),
            win: WinCondition::SurviveWaves { waves: 1 },
            sell_refund_pct: 0.7,
        },
        econ_start: EconStart { gold: 120, wood: 5, pop_cap: 8 },
        econ_rules: EconRules::default(),
        attack_armor: AttackArmorTable::default(),
    }
}

fn write_ron(app: &mut App, bundle: &MapBundle) -> std::path::PathBuf {
    let path = warbase::testing::temp_path("map");
    std::fs::write(&path, ron::ser::to_string_pretty(bundle, Default::default()).unwrap()).unwrap();
    let _ = app;
    path
}

/// UT-BUN-04: broken syntax is a readable parse error.
#[test]
fn broken_ron_is_rejected_as_parse_error() {
    let mut app = test_app();
    let path = warbase::testing::temp_path("map");
    std::fs::write(&path, "this is (not ron").unwrap();
    match load_bundle(&path) {
        Err(warbase::bundle::MapError::Parse(_)) => {}
        other => panic!("expected parse error, got {other:?}"),
    }
}

/// UT-BUN-05: dangling unit reference in a wave names the wave/spawn.
#[test]
fn dangling_unit_reference_is_rejected() {
    let mut bundle = sample_bundle();
    bundle.waves.waves[0].spawns[0].unit = "ghost".into();
    let err = validate(&bundle).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("unknown unit 'ghost'"), "{msg}");
    assert!(msg.contains("wave 0 spawn 0"), "{msg}");
}

/// UT-BUN-05: out-of-range path index is rejected.
#[test]
fn out_of_range_path_index_is_rejected() {
    let mut bundle = sample_bundle();
    bundle.waves.waves[0].spawns[0].path = 7;
    let err = validate(&bundle).unwrap_err();
    assert!(format!("{err}").contains("path index 7"), "{}", err);
}

/// UT-BUN-03: version mismatch is refused loudly (no silent migration).
#[test]
fn version_mismatch_is_refused() {
    let mut bundle = sample_bundle();
    bundle.identity.version = 999;
    let err = validate(&bundle).unwrap_err();
    assert!(format!("{err}").contains("999"), "{}", err);
}

/// Round trip: a valid bundle saves, loads, validates, assembles — and the
/// assembled match actually runs (director spawns tagged units from the
/// file's wave table).
#[test]
fn bundle_round_trips_and_assembles_a_running_match() {
    let mut app = test_app();
    let bundle = sample_bundle();
    let path = write_ron(&mut app, &bundle);
    let loaded = load_bundle(&path).expect("valid bundle loads");
    validate(&loaded).expect("valid bundle validates");

    assemble(&mut app.world_mut(), &loaded);
    assert_eq!(app.world().resource::<Resources>().gold, 120);
    assert_eq!(app.world().resource::<Population>().cap, 8);

    // Auto advance with 1 s intermission (60 frames) + second spawn at +0.5 s
    // (interval 0.5) => both grunts on the loop well before frame 100.
    for _ in 0..100 {
        app.update();
    }
    let tagged = app.world_mut().query::<&WaveTag>().iter(app.world()).count();
    assert_eq!(tagged, 2, "spawned from the file's wave table");
}
