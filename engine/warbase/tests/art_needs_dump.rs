//! ADR-0015 acceptance: `dump_units_json` projection consumed by art-needs.
//!
//! The projection is a contract with Python tooling: field names, kind
//! classification, upgrade chains and wave reference counts must stay stable.

use std::collections::HashMap;

use warbase::bundle::{dump_units_json, MapBundle, MAP_FORMAT_VERSION};
use warbase::prelude::*;
use warbase::testing::test_app;

fn unit(id: &str, cost: u32, bounty: u32, upgrade_to: Option<&str>) -> UnitDef {
    UnitDef {
        id: id.into(),
        attrs: BaseAttributes {
            hp: 50.0,
            move_speed: if cost > 0 { 0.0 } else { 1.0 },
            body_radius: 0.3,
        },
        attack: None,
        armor_type: ArmorType::Light,
        cost_gold: cost,
        cost_wood: 0,
        bounty_gold: bounty,
        bounty_wood: 0,
        grant_xp: 1,
        traits: vec![],
        auras: vec![],
        hero: false,
        skills: vec![],
        upgrade_to: upgrade_to.map(Into::into),
        heal: None,
    }
}

fn bundle_with_towers() -> MapBundle {
    let mut units: HashMap<String, UnitDef> = HashMap::new();
    units.insert("arrow".into(), unit("arrow", 50, 0, Some("arrow2")));
    units.insert("arrow2".into(), unit("arrow2", 90, 0, None));
    units.insert("grunt".into(), unit("grunt", 0, 3, None));
    MapBundle {
        identity: warbase::bundle::MapIdentity { id: "sample".into(), version: MAP_FORMAT_VERSION },
        world: warbase::bundle::WorldDef {
            paths: vec![PathDef::closed_loop(vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]])],
            build: warbase::bundle::BuildDomainDef::FreeGrid { cell: 1.0, cols: 16, rows: 16 },
            blocked: vec![],
        },
        units,
        waves: WaveTable {
            waves: vec![
                WaveEntry {
                    spawns: vec![SpawnSpec {
                        unit: "grunt".into(), count: 6, interval: 0.5,
                        path: 0, delay: 0.0, hp_scale: 1.0,
                    }],
                    boss_timeout: None,
                    preview: "6 grunts".into(),
                    reward_gold: 25,
                },
                WaveEntry {
                    spawns: vec![SpawnSpec {
                        unit: "grunt".into(), count: 4, interval: 0.5,
                        path: 0, delay: 0.0, hp_scale: 2.0,
                    }],
                    boss_timeout: None,
                    preview: "4 grunts".into(),
                    reward_gold: 30,
                },
            ],
            advance: Advance::Auto { intermission: 1.0 },
            early_call_bonus: Some(10),
        },
        rules: MatchRules {
            leak: LeakPolicy::Stockpile { cap: 10 },
            build_window: BuildWindow::Always,
            lap_buff: Some(LapBuffPolicy { speed_pct: 0.05, hp_pct: 0.03 }),
            win: WinCondition::SurviveWaves { waves: 2 },
            sell_refund_pct: 0.7,
        },
        econ_start: EconStart { gold: 120, wood: 5, pop_cap: 8 },
        econ_rules: EconRules::default(),
        attack_armor: AttackArmorTable::default(),
    }
}

#[test]
fn projection_fields_and_kinds() {
    let _app = test_app(); // keep parity with sibling tests' setup needs
    let json: serde_json::Value =
        serde_json::from_str(&dump_units_json(&bundle_with_towers())).unwrap();

    assert_eq!(json["map"]["id"], "sample");
    let units = json["units"].as_array().unwrap();
    assert_eq!(units.len(), 3);
    // Deterministic: sorted by id.
    let ids: Vec<&str> = units.iter().map(|u| u["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["arrow", "arrow2", "grunt"]);

    let arrow = &units[0];
    assert_eq!(arrow["kind"], "tower"); // cost_gold > 0
    assert_eq!(arrow["upgrade_to"], "arrow2");
    assert_eq!(arrow["wave_refs"], 0); // towers are never spawned by waves
    let grunt = &units[2];
    assert_eq!(grunt["kind"], "creep"); // bounty_gold > 0
    assert_eq!(grunt["upgrade_to"], serde_json::Value::Null);
    assert_eq!(grunt["wave_refs"], 10); // 6 + 4 bodies across waves
}

#[test]
fn projection_round_trips_through_ron_loader() {
    // Same path the bin takes: RON text -> load_bundle -> dump_units_json.
    let dir = std::env::temp_dir().join("warbase-dump-test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("probe.ron");
    std::fs::write(&path, ron::to_string(&bundle_with_towers()).unwrap()).unwrap();

    let loaded = warbase::bundle::load_bundle(&path).unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&dump_units_json(&loaded)).unwrap();
    assert_eq!(json["units"].as_array().unwrap().len(), 3);
}
