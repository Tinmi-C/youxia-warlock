//! BX1 acceptance: attack/armor table lookups + def-driven spawning.

use bevy::prelude::*;
use warbase::prelude::*;
use warbase::testing::test_app;

fn table_with_unique_values() -> AttackArmorTable {
    let mut t = AttackArmorTable::default();
    for (ai, a) in AttackType::ALL.iter().enumerate() {
        for (ri, r) in ArmorType::ALL.iter().enumerate() {
            // 1.05 + tiny offsets: every combo distinct and != 1.0.
            t.mult[*a as usize][*r as usize] = 1.05 + ai as f32 * 0.01 + ri as f32 * 0.001;
            let _ = (ai, ri);
        }
    }
    t
}

/// UT-AA-01: every (attack, armor) combo returns exactly the shipped value.
#[test]
fn attack_armor_table_returns_shipped_values_for_all_combos() {
    let t = table_with_unique_values();
    for a in AttackType::ALL {
        for r in ArmorType::ALL {
            let expected = 1.05 + (a as usize as f32) * 0.01 + (r as usize as f32) * 0.001;
            let got = t.multiplier(a, r);
            assert!((got - expected).abs() < 1e-6, "{a:?} x {r:?}: {got} != {expected}");
        }
    }
}

/// UT-AA-02: default table (unknown/unfilled combos) falls back to 1.0.
#[test]
fn default_table_multiplier_is_one() {
    let t = AttackArmorTable::default();
    assert_eq!(t.multiplier(AttackType::Magic, ArmorType::Divine), 1.0);
}

fn grunt_registry() -> UnitRegistry {
    let mut defs = std::collections::HashMap::new();
    defs.insert(
        "grunt".into(),
        UnitDef {
            id: "grunt".into(),
            attrs: BaseAttributes { hp: 120.0, move_speed: 2.0, body_radius: 0.4 },
            attack: None,
            armor_type: ArmorType::Heavy,
            cost_gold: 0,
            cost_wood: 0,
            bounty_gold: 5,
            bounty_wood: 1,
            grant_xp: 3,
            traits: vec![UnitTrait::Fast],
        },
    );
    UnitRegistry { defs }
}

/// UT-UNIT-01: spawn assembles the component set from the def.
#[test]
fn spawn_unit_builds_components_from_def() {
    let mut app = test_app();
    let reg = grunt_registry();
    let Some(e) = spawn_unit(&mut app.world_mut(), &reg, "grunt", Vec3::new(1.0, 0.0, 2.0), Allegiance::Attacker, 1.0) else {
        panic!("known def must spawn");
    };
    let w = app.world();
    assert_eq!(w.get::<Hp>(e).unwrap().0, 120.0);
    assert_eq!(w.get::<MaxHp>(e).unwrap().0, 120.0);
    assert_eq!(w.get::<Armor>(e).unwrap().0, ArmorType::Heavy);
    assert_eq!(w.get::<UnitInstance>(e).unwrap().def, "grunt");
    assert_eq!(w.get::<MoveSpeed>(e).unwrap().0, 2.0);
    assert_eq!(*w.get::<Allegiance>(e).unwrap(), Allegiance::Attacker);
    assert!(w.get::<MoveSpeed>(e).is_some());
}

/// UT-UNIT-02 (adapted): traits map to markers at spawn (Fast => still walks
/// at def speed; Air => AirUnit marker that Ground attacks must skip).
#[test]
fn air_trait_spawns_air_marker() {
    let mut app = test_app();
    let mut reg = grunt_registry();
    reg.defs.get_mut("grunt").unwrap().traits = vec![UnitTrait::Air];
    let e = spawn_unit(&mut app.world_mut(), &reg, "grunt", Vec3::ZERO, Allegiance::Attacker, 1.0).unwrap();
    assert!(app.world().get::<AirUnit>(e).is_some());
}

#[test]
fn spawn_unknown_def_returns_none() {
    let mut app = test_app();
    assert!(spawn_unit(&mut app.world_mut(), &UnitRegistry::default(), "nope", Vec3::ZERO, Allegiance::Attacker, 1.0).is_none());
}
