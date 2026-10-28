//! warden.ron authoring seam (WX6-6f): `maps/warden.ron` is generated from
//! the code tables — the SAME builders the app used before the RON landed —
//! so the file is schema-valid and value-identical by construction.
//! Regenerate after tuning any table:
//!   cargo test -p warden --test map_bundle -- --ignored --nocapture

use bevy::prelude::*;
use warbase::bundle::{build_grid, load_bundle, validate, MapBundle, MAP_FORMAT_VERSION};
use warden::resources::{
    EnemyDefs, FusionDefs, TowerDefs, WaveSchedule, SELL_REFUND_RATIO,
};
use warden::systems::damage::warden_armor_table;
use warden::systems::enemy::warden_registry;
use warden::systems::map::{slot_grid, SLOT_POSITIONS};
use warden::systems::wave::base_wave_table;

fn warden_bundle() -> MapBundle {
    let enemies = EnemyDefs::palette();
    let towers = TowerDefs::palette();
    let fusions = FusionDefs::palette();
    let schedule = WaveSchedule::schedule();
    MapBundle {
        identity: warbase::bundle::MapIdentity {
            id: "warden".into(),
            version: MAP_FORMAT_VERSION,
        },
        world: warbase::bundle::WorldDef {
            paths: vec![warbase::prelude::PathDef::open(vec![
                [-13.0, 9.0],
                [-4.0, 9.0],
                [-4.0, -9.0],
                [12.0, -9.0],
            ])],
            build: warbase::bundle::BuildDomainDef::FixedSlots {
                slots: SLOT_POSITIONS.iter().map(|(x, z)| [*x, *z]).collect(),
            },
            blocked: vec![],
        },
        units: warden_registry(&enemies, &towers, &fusions).defs,
        waves: base_wave_table(&schedule, &enemies),
        rules: warbase::prelude::MatchRules {
            leak: warbase::prelude::LeakPolicy::Lives { lives: 10 },
            build_window: warbase::prelude::BuildWindow::PrepOnly,
            lap_buff: None,
            win: warbase::prelude::WinCondition::SurviveWaves {
                waves: schedule.waves.len() as u32,
            },
            sell_refund_pct: SELL_REFUND_RATIO,
        },
        econ_start: warbase::prelude::EconStart { gold: 100, wood: 0, pop_cap: 8 },
        econ_rules: warbase::prelude::EconRules::default(),
        attack_armor: warden_armor_table(),
        nav: Default::default(),
        vision: Default::default(),
        items: Default::default(),
        production: Default::default(),
        teams: Default::default(),
        starting_forces: Default::default(),
    harvest: Default::default(),
    camps: Default::default(),
    daynight: Default::default(),
    terrain: Default::default(),
    }
}

/// Regenerate maps/warden.ron from the code tables (see module doc).
#[test]
#[ignore = "authoring seam: writes the repo's map file on purpose"]
fn regenerate_warden_ron() {
    let bundle = warden_bundle();
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("maps/warden.ron");
    std::fs::write(&path, ron::ser::to_string_pretty(&bundle, Default::default()).unwrap())
        .unwrap();
    println!("wrote {}", path.display());
}

/// The committed file loads, validates, and matches the code tables exactly
/// (units/waves/rules/econ/attack-armor + the slot grid) — the drift guard
/// for the RON being the live map.
#[test]
fn committed_ron_matches_code_tables() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("maps/warden.ron");
    let loaded = load_bundle(&path).expect("warden.ron loads");
    validate(&loaded).expect("warden.ron validates");
    let code = warden_bundle();

    let key = |m: &std::collections::HashMap<String, warbase::prelude::UnitDef>| {
        let mut v: Vec<_> = m.values().map(|d| d.id.clone()).collect();
        v.sort();
        v
    };
    assert_eq!(key(&loaded.units), key(&code.units), "same unit ids");
    for (id, def) in &code.units {
        let got = &loaded.units[id];
        assert_eq!(got.armor_type, def.armor_type, "{id} armor");
        assert_eq!(got.cost_gold, def.cost_gold, "{id} cost");
        assert_eq!(got.bounty_gold, def.bounty_gold, "{id} bounty");
        assert_eq!(got.leak, def.leak, "{id} leak");
        assert_eq!(got.heal.is_some(), def.heal.is_some(), "{id} heal presence");
        assert_eq!(got.attrs.hp, def.attrs.hp, "{id} hp");
        assert_eq!(got.attrs.move_speed, def.attrs.move_speed, "{id} speed");
        // Roster v2 drift guard: the full attack block (damage / interval /
        // range / attack type / windup / backswing) and the hero layer
        // (hero flag, three-attribute stats, signature skill slot) ship in
        // the RON verbatim — the file is the numbers' single authority.
        match (&got.attack, &def.attack) {
            (Some(a), Some(b)) => {
                assert_eq!(a.damage, b.damage, "{id} attack damage");
                assert_eq!(a.interval, b.interval, "{id} attack interval");
                assert_eq!(a.range, b.range, "{id} attack range");
                assert_eq!(a.attack_type, b.attack_type, "{id} attack type");
                assert_eq!(a.windup_secs, b.windup_secs, "{id} windup");
                assert_eq!(a.backswing_secs, b.backswing_secs, "{id} backswing");
            }
            (None, None) => {}
            _ => panic!("{id} attack presence mismatch"),
        }
        assert_eq!(got.hero, def.hero, "{id} hero");
        assert_eq!(got.hero_stats, def.hero_stats, "{id} hero stats");
        assert_eq!(got.skills, def.skills, "{id} skills");
        assert_eq!(got.skill_levels, def.skill_levels, "{id} skill levels");
    }
    assert_eq!(loaded.waves.waves.len(), code.waves.waves.len());
    for (i, (lw, cw)) in loaded.waves.waves.iter().zip(code.waves.waves.iter()).enumerate() {
        assert_eq!(lw.reward_gold, cw.reward_gold, "wave {i} reward");
        assert_eq!(lw.spawns.len(), cw.spawns.len(), "wave {i} spec count");
        for (j, (ls, cs)) in lw.spawns.iter().zip(cw.spawns.iter()).enumerate() {
            assert_eq!(ls.unit, cs.unit, "wave {i} spec {j} unit");
            assert_eq!(ls.count, cs.count, "wave {i} spec {j} count");
        }
    }
    assert_eq!(
        loaded.attack_armor.mult[warbase::prelude::AttackType::Normal as usize]
            [warbase::prelude::ArmorType::Fortified as usize],
        0.5,
        "physical x shield 0.5"
    );
    assert_eq!(loaded.econ_start.gold, 100);
    // The slot grid derived from the file matches the code grid.
    let lg = build_grid(&loaded.world);
    let cg = slot_grid();
    assert_eq!(lg.slots, cg.slots, "slot positions");
}
