//! BX2 acceptance: damage pipeline (single Hp writer, armor multipliers,
//! kill chain, attribution), targeting priorities, projectiles, air filtering.

use bevy::prelude::*;
use warbase::prelude::*;
use warbase::testing::{count_over_frames, test_app};

fn def(id: &str, hp: f32, armor: ArmorType) -> UnitDef {
    UnitDef {
        id: id.into(),
        attrs: BaseAttributes { hp, move_speed: 0.0, body_radius: 0.3 },
        attack: None,
        armor_type: armor,
        bounty_gold: 7,
        bounty_wood: 0,
        grant_xp: 2,
        traits: vec![],
    }
}

fn registry(defs: Vec<UnitDef>) -> UnitRegistry {
    UnitRegistry { defs: defs.into_iter().map(|d| (d.id.clone(), d)).collect() }
}

/// UT-CMB-01: a DamageRequest reduces Hp by exactly `amount` (1.0 table).
#[test]
fn damage_request_reduces_hp() {
    let mut app = test_app();
    let reg = registry(vec![def("grunt", 100.0, ArmorType::Medium)]);
    let e = spawn_unit(&mut app.world_mut(), &reg, "grunt", Vec3::ZERO, Allegiance::Attacker).unwrap();
    let source = app.world_mut().spawn_empty().id();
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<DamageRequest>>()
        .write(DamageRequest { target: e, source, amount: 30.0, attack_type: AttackType::Normal, crit: false });
    app.update();
    assert_eq!(app.world().get::<Hp>(e).unwrap().0, 70.0);
}

/// UT-CMB-02: the armor multiplier from the shipped table is applied.
#[test]
fn armor_multiplier_applies() {
    let mut app = test_app();
    let mut table = AttackArmorTable::default();
    table.mult[AttackType::Magic as usize][ArmorType::Heavy as usize] = 0.5;
    app.insert_resource(table);
    let reg = registry(vec![def("ogre", 100.0, ArmorType::Heavy)]);
    let e = spawn_unit(&mut app.world_mut(), &reg, "ogre", Vec3::ZERO, Allegiance::Attacker).unwrap();
    let source = app.world_mut().spawn_empty().id();
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<DamageRequest>>()
        .write(DamageRequest { target: e, source, amount: 40.0, attack_type: AttackType::Magic, crit: false });
    app.update();
    assert_eq!(app.world().get::<Hp>(e).unwrap().0, 80.0);
}

/// UT-CMB-03: overkill kill emits DamageDealt{killed} + UnitDied with
/// bounty/xp, and the entity despawns in Cleanup.
#[test]
fn kill_chain_emits_died_and_despawns() {
    let mut app = test_app();
    let reg = registry(vec![def("grunt", 10.0, ArmorType::Light)]);
    app.insert_resource(UnitRegistry { defs: reg.defs.clone() });
    let e = spawn_unit(&mut app.world_mut(), &reg, "grunt", Vec3::ZERO, Allegiance::Attacker).unwrap();
    let source = app.world_mut().spawn_empty().id();
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<DamageRequest>>()
        .write(DamageRequest { target: e, source, amount: 99.0, attack_type: AttackType::Normal, crit: false });
    app.update();

    let mut cursor = bevy::ecs::message::MessageCursor::<UnitDied>::default();
    let msgs = app.world().resource::<bevy::ecs::message::Messages<UnitDied>>();
    let deaths: Vec<_> = cursor.read(msgs).collect();
    assert_eq!(deaths.len(), 1);
    assert_eq!(deaths[0].gold, 7);
    assert_eq!(deaths[0].xp, 2);

    let mut dealt_cursor = bevy::ecs::message::MessageCursor::<DamageDealt>::default();
    let dealt_msgs = app.world().resource::<bevy::ecs::message::Messages<DamageDealt>>();
    let dealt: Vec<_> = dealt_cursor.read(dealt_msgs).collect();
    assert_eq!(dealt.len(), 1);
    assert!(dealt[0].killed);
    assert_eq!(dealt[0].amount, 99.0);
    assert_eq!(dealt[0].source, source, "attribution rides the pipeline");

    app.update(); // Cleanup despawns
    assert!(app.world().get_entity(e).is_err());
}

/// UT-CMB-05: First targets the highest (lap, progress); Last the lowest.
#[test]
fn targeting_first_and_last_rank_by_path_progress() {
    let mut app = test_app();
    let reg = registry(vec![def("grunt", 100.0, ArmorType::Light)]);
    let mut world = app.world_mut();
    let front = spawn_unit(&mut world, &reg, "grunt", Vec3::ZERO, Allegiance::Attacker).unwrap();
    let back = spawn_unit(&mut world, &reg, "grunt", Vec3::ZERO, Allegiance::Attacker).unwrap();
    world.entity_mut(front).insert(PathFollower { path: Entity::PLACEHOLDER, progress: 8.0, laps: 1, finished: false });
    world.entity_mut(back).insert(PathFollower { path: Entity::PLACEHOLDER, progress: 2.0, laps: 0, finished: false });

    let mut tower_def = def("tower", 100.0, ArmorType::Fortified);
    tower_def.attack = Some(AttackDef {
        damage: 1.0, interval: 1.0, range: 100.0,
        attack_type: AttackType::Normal, targets: TargetDomain::Both, projectile: None,
    });
    let reg2 = registry(vec![tower_def.clone()]);
    let mut tower = spawn_unit(&mut app.world_mut(), &reg2, "tower", Vec3::ZERO, Allegiance::Defender).unwrap();
    app.update();
    assert_eq!(app.world().get::<CurrentTarget>(tower).unwrap().0, Some(front));

    app.world_mut().entity_mut(tower).insert(AttackKit { def: tower_def.attack.clone().unwrap(), priority: TargetingPriority::Last });
    app.update();
    assert_eq!(app.world().get::<CurrentTarget>(tower).unwrap().0, Some(back));
    tower = tower; // keep the binding used
}

/// UT-CMB-07: Ground-only attacks never lock Air units.
#[test]
fn ground_only_attack_skips_air_units() {
    let mut app = test_app();
    let mut air = def("bat", 50.0, ArmorType::Light);
    air.traits = vec![UnitTrait::Air];
    let reg = registry(vec![air]);
    let bat = spawn_unit(&mut app.world_mut(), &reg, "bat", Vec3::ZERO, Allegiance::Attacker).unwrap();

    let mut tower_def = def("tower", 100.0, ArmorType::Fortified);
    tower_def.attack = Some(AttackDef {
        damage: 10.0, interval: 1.0, range: 100.0,
        attack_type: AttackType::Piercing, targets: TargetDomain::Ground, projectile: None,
    });
    let reg2 = registry(vec![tower_def]);
    let tower = spawn_unit(&mut app.world_mut(), &reg2, "tower", Vec3::ZERO, Allegiance::Defender).unwrap();
    app.update();
    assert_eq!(app.world().get::<CurrentTarget>(tower).unwrap().0, None, "air unit must be skipped");

    // Sanity: Both-domain does lock it.
    app.world_mut().entity_mut(tower).get_mut::<AttackKit>().unwrap().def.targets = TargetDomain::Both;
    app.update();
    assert_eq!(app.world().get::<CurrentTarget>(tower).unwrap().0, Some(bat));
}

/// UT-CMB-06: homing projectile lands its DamageRequest; a target that
/// vanishes mid-flight makes the projectile fizzle (no damage).
#[test]
fn projectile_hits_and_fizzles_on_lost_target() {
    let mut app = test_app();
    let grunt = def("grunt", 100.0, ArmorType::Light);
    let mut tower_def = def("tower", 100.0, ArmorType::Fortified);
    tower_def.attack = Some(AttackDef {
        damage: 25.0, interval: 10.0, range: 100.0,
        attack_type: AttackType::Piercing, targets: TargetDomain::Both,
        projectile: Some(ProjDef { speed: 60.0 }),
    });
    let reg = registry(vec![grunt, tower_def]);
    let e = spawn_unit(&mut app.world_mut(), &reg, "grunt", Vec3::new(5.0, 0.0, 0.0), Allegiance::Attacker).unwrap();
    let tower = spawn_unit(&mut app.world_mut(), &reg, "tower", Vec3::ZERO, Allegiance::Defender).unwrap();

    // Projectile speed 60 => 1 unit per frame; distance 5 => impact on frame 5.
    for _ in 0..7 {
        app.update();
    }
    let hp = app.world().get::<Hp>(e).unwrap().0;
    assert!((hp - 75.0).abs() < 1e-3, "hp {hp}");

    // Fizzle: fire again at a fresh victim, then despawn it mid-flight.
    let victim = spawn_unit(&mut app.world_mut(), &reg, "grunt", Vec3::new(5.0, 0.0, 0.0), Allegiance::Attacker).unwrap();
    app.world_mut().entity_mut(tower).get_mut::<AttackCooldown>().unwrap().0 = 0.0; // bypass interval for the test
    app.update(); // Intent picks victim, Act fires a fresh projectile
    app.world_mut().entity_mut(victim).despawn();
    for _ in 0..3 {
        app.update();
    }
    let hp_after = app.world().get::<Hp>(e).unwrap().0;
    assert!((hp_after - 75.0).abs() < 1e-3, "no further damage after target lost (hp {hp_after})");
}
