//! BX3-lite acceptance: skill cooldowns/cast validation/autocast, hero xp
//! leveling (+DamageMult growth), aura slow/damage-taken through the whole
//! pipeline (StatMods -> EffectiveStats -> combat/path readers).

use bevy::prelude::*;
use warbase::prelude::*;
use warbase::testing::{count_over_frames, test_app};

fn def(id: &str, hp: f32) -> UnitDef {
    UnitDef {
        id: id.into(),
        attrs: BaseAttributes { hp, move_speed: 1.0, body_radius: 0.3 },
        attack: None,
        armor_type: ArmorType::Light,
        cost_gold: 0, cost_wood: 0,
        bounty_gold: 0, bounty_wood: 0, grant_xp: 12,
        traits: vec![],
        auras: vec![],
        hero: false,
        skills: vec![],
        upgrade_to: None,
        heal: None,
    }
}

/// UT-SKL-01/02: cooldown ticks; an unlearned or cooling skill refuses to
/// cast; a valid cast emits SkillCast exactly once and restarts cooldown.
#[test]
fn cast_validation_and_cooldown() {
    let mut app = test_app();
    app.insert_resource({
        let mut defs = SkillDefs::default();
        defs.defs.insert(
            "nova".into(),
            SkillDef {
                id: "nova".into(),
                kind: SkillKind::Active,
                targeting: SkillTargeting::None,
                max_level: 3,
                cooldown: 2.0,
            },
        );
        defs
    });
    let caster = app
        .world_mut()
        .spawn(SkillCaster {
            slots: vec![SkillSlot { skill: "nova".into(), level: 1, cooldown: 0.0, autocast_on: false }],
        })
        .id();

    // Unlearned slot (level 0) refuses.
    app.world_mut().get_mut::<SkillCaster>(caster).unwrap().slots[0].level = 0;
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<CastSkill>>()
        .write(CastSkill { caster, slot: 0 });
    app.update();
    assert_eq!(count_over_frames::<SkillCast>(&mut app, 1), 0);

    // Learned + ready casts once, then cooldown blocks the immediate re-cast.
    app.world_mut().get_mut::<SkillCaster>(caster).unwrap().slots[0].level = 1;
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<CastSkill>>()
        .write(CastSkill { caster, slot: 0 });
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<CastSkill>>()
        .write(CastSkill { caster, slot: 0 });
    let casts = count_over_frames::<SkillCast>(&mut app, 2);
    assert_eq!(casts, 1, "second cast blocked by cooldown");
}

/// Autocast: a ready autocast slot casts itself without any request.
#[test]
fn autocast_fires_when_ready() {
    let mut app = test_app();
    app.insert_resource({
        let mut defs = SkillDefs::default();
        defs.defs.insert(
            "harden".into(),
            SkillDef { id: "harden".into(), kind: SkillKind::Autocast, targeting: SkillTargeting::None, max_level: 1, cooldown: 5.0 },
        );
        defs
    });
    app.world_mut().spawn(SkillCaster {
        slots: vec![SkillSlot { skill: "harden".into(), level: 1, cooldown: 0.0, autocast_on: true }],
    });
    let casts = count_over_frames::<SkillCast>(&mut app, 3);
    assert_eq!(casts, 1, "fires once, then cooldown");
}

/// HR-lite: killer with XpHolder levels up, banks a point, and its
/// permanent DamageMult growth entry shows in EffectiveStats.
#[test]
fn killer_xp_levels_and_grows() {
    let mut app = test_app();
    let mut victim = def("victim", 10.0);
    victim.grant_xp = 12;
    let mut hero = def("hero", 100.0);
    hero.hero = true;
    hero.attack = Some(AttackDef {
        damage: 99.0, interval: 10.0, range: 5.0,
        attack_type: AttackType::Normal, targets: TargetDomain::Both, projectile: None,
    });
    let reg = UnitRegistry { defs: [("victim".to_string(), victim), ("hero".to_string(), hero)].into_iter().collect() };
    app.insert_resource(UnitRegistry { defs: reg.defs.clone() });

    let v = spawn_unit(&mut app.world_mut(), &reg, "victim", Vec3::ZERO, Allegiance::Attacker, 1.0).unwrap();
    let h = spawn_unit(&mut app.world_mut(), &reg, "hero", Vec3::ZERO, Allegiance::Defender, 1.0).unwrap();
    assert!(app.world().get::<XpHolder>(h).is_some());

    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<DamageRequest>>()
        .write(DamageRequest { target: v, source: h, amount: 99.0, attack_type: AttackType::Normal, crit: false });
    for _ in 0..4 {
        app.update(); // death -> UnitDied -> distribute_xp
    }
    let xp = app.world().get::<XpHolder>(h).unwrap();
    assert_eq!(xp.level, 2, "12 xp >= 10 needed");
    assert_eq!(xp.points, 1);
    let eff = app.world().get::<EffectiveStats>(h).expect("stats aggregated");
    assert!((eff.damage_mult - 1.05).abs() < 1e-5, "{}", eff.damage_mult);
}

/// UT-BUF-01: an aura slows creeps inside its radius (path speed reads the
/// aggregated move_mult); leaving the radius restores speed next frame.
#[test]
fn aura_slows_inside_radius_only() {
    let mut app = test_app();
    let path = app.world_mut().spawn(PathDef::open(vec![[0.0, 0.0], [10.0, 0.0]])).id();
    let creep = app
        .world_mut()
        .spawn((
            PathFollower { path, progress: 0.0, laps: 0, finished: false },
            MoveSpeed(1.0),
            Transform::default(),
            StatMods::default(),
            Allegiance::Attacker,
        ))
        .id();
    let slow_tower = app
        .world_mut()
        .spawn((
            Transform::from_xyz(0.0, 0.0, 0.0),
            Allegiance::Defender,
            Auras(vec![AuraDef { radius: 4.0, kind: StatKind::MoveSpeedMult, amount: -0.5 }]),
        ))
        .id();

    app.update();
    app.update();
    let eff = app.world().get::<EffectiveStats>(creep).unwrap();
    assert!((eff.move_mult - 0.5).abs() < 1e-5, "slowed in radius: {}", eff.move_mult);

    // Move the aura carrier out of range: entry must disappear.
    app.world_mut().entity_mut(slow_tower).insert(Transform::from_xyz(100.0, 0.0, 0.0));
    app.update();
    app.update();
    let eff = app.world().get::<EffectiveStats>(creep).unwrap();
    assert!((eff.move_mult - 1.0).abs() < 1e-5, "restored out of radius: {}", eff.move_mult);
}

/// UT-BUF-03 + pipeline: two damage-taken auras stack (sum of deltas) and
/// resolve_damage multiplies by the aggregated factor.
#[test]
fn damage_taken_auras_stack_into_resolve() {
    let mut app = test_app();
    let grunt = def("grunt", 100.0, );
    let reg = UnitRegistry { defs: [("grunt".to_string(), grunt)].into_iter().collect() };
    let e = spawn_unit(&mut app.world_mut(), &reg, "grunt", Vec3::ZERO, Allegiance::Attacker, 1.0).unwrap();
    app.world_mut().entity_mut(e).insert(PathFollower { path: Entity::PLACEHOLDER, progress: 0.0, laps: 0, finished: false });
    app.insert_resource(UnitRegistry { defs: reg.defs.clone() });

    for amount in [0.2f32, 0.3] {
        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.0, 0.0),
            Allegiance::Defender,
            Auras(vec![AuraDef { radius: 4.0, kind: StatKind::DamageTakenMult, amount }]),
        ));
    }
    app.update();
    app.update();

    let src = app.world_mut().spawn_empty().id();
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<DamageRequest>>()
        .write(DamageRequest { target: e, source: src, amount: 10.0, attack_type: AttackType::Normal, crit: false });
    app.update();

    let eff = app.world().get::<EffectiveStats>(e).unwrap();
    assert!((eff.damage_taken_mult - 1.5).abs() < 1e-5, "0.2+0.3 deltas: {}", eff.damage_taken_mult);
    // 10 dmg x 1.5 taken = 15 dealt.
    let mut cursor = bevy::ecs::message::MessageCursor::<DamageDealt>::default();
    let msgs = app.world().resource::<bevy::ecs::message::Messages<DamageDealt>>();
    let dealt: Vec<f32> = cursor.read(msgs).map(|d| d.amount).collect();
    assert!((dealt[0] - 15.0).abs() < 1e-4, "{dealt:?}");
}
