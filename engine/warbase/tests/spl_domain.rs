//! SPL domain acceptance (ADR-0016 §6): mana pools/regen/cost validation
//! and cast-time/channel/interrupt semantics. Everything runs on the fixed
//! 1/60 stepping from `warbase::testing`, so every frame count below is
//! exact (30 frames = 0.5s, 60 frames = 1.0s).

use bevy::ecs::message::Messages;
use bevy::prelude::*;
use warbase::prelude::*;
use warbase::testing::{count_over_frames, test_app, STEP};

/// Minimal mana-less unit def (legacy shape: no mana bar at all).
fn def(id: &str, hp: f32) -> UnitDef {
    UnitDef {
        leak: 1,
        id: id.into(),
        attrs: BaseAttributes { hp, move_speed: 0.0, body_radius: 0.3 },
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
        heal: None,
        mana: None,
    }
}

fn skill_def(id: &str, cooldown: f32, mana_cost: f32) -> SkillDef {
    SkillDef {
        id: id.into(),
        kind: SkillKind::Active,
        targeting: SkillTargeting::None,
        max_level: 3,
        cooldown,
        badge: None,
        hotkey: None,
        mana_cost,
    }
}

fn with_skill_defs(app: &mut App, defs: Vec<SkillDef>) {
    let mut res = SkillDefs::default();
    for d in defs {
        res.defs.insert(d.id.clone(), d);
    }
    app.insert_resource(res);
}

fn spawn_with_slot(
    app: &mut App,
    unit_def: UnitDef,
    skills: Vec<SkillDef>,
    slot_skill: &str,
) -> Entity {
    let reg = UnitRegistry {
        defs: [(unit_def.id.clone(), unit_def)].into_iter().collect(),
    };
    app.insert_resource(UnitRegistry { defs: reg.defs.clone() });
    with_skill_defs(app, skills);
    let e = spawn_unit(
        &mut app.world_mut(),
        &reg,
        &reg.defs.keys().next().unwrap(),
        Vec3::ZERO,
        Allegiance::Defender,
        1.0,
    )
    .unwrap();
    app.world_mut().entity_mut(e).insert(SkillCaster {
        slots: vec![SkillSlot {
            skill: slot_skill.into(),
            level: 1,
            cooldown: 0.0,
            autocast_on: false,
        }],
    });
    e
}

fn cast(app: &mut App, caster: Entity, slot: usize) {
    app.world_mut()
        .resource_mut::<Messages<CastSkill>>()
        .write(CastSkill { caster, slot });
}

fn mana_of(app: &App, e: Entity) -> f32 {
    app.world().get::<Mana>(e).unwrap().0
}

fn cooldown_of(app: &App, e: Entity) -> f32 {
    app.world().get::<SkillCaster>(e).unwrap().slots[0].cooldown
}

/// SPL1-1: a def with `mana` spawns Mana/MaxMana/ManaRegen at max; an
/// instant cast with mana_cost 30 deducts exactly 30 at cast start
/// (100 -> 70) and fires one SkillCast.
#[test]
fn mana_pool_spawns_and_instant_cast_deducts() {
    let mut app = test_app();
    let mut u = def("mage", 100.0);
    u.mana = Some(ManaDef { max: 100.0, regen: 0.0 });
    let e = spawn_with_slot(
        &mut app,
        u,
        vec![skill_def("bolt", 2.0, 30.0)],
        "bolt",
    );

    assert!((mana_of(&app, e) - 100.0).abs() < 1e-5, "spawns at max");
    assert!((app.world().get::<MaxMana>(e).unwrap().0 - 100.0).abs() < 1e-5);
    assert!((app.world().get::<ManaRegen>(e).unwrap().0 - 0.0).abs() < 1e-5);

    cast(&mut app, e, 0);
    assert_eq!(count_over_frames::<SkillCast>(&mut app, 2), 1);
    assert!(
        (mana_of(&app, e) - 70.0).abs() < 1e-5,
        "mana deducted at cast start: {}",
        mana_of(&app, e)
    );
}

/// SPL1-2: mana 20 < cost 30 -> the cast is refused with no side effects:
/// no SkillCast, cooldown NOT started, mana untouched.
#[test]
fn insufficient_mana_refuses_cast_without_side_effects() {
    let mut app = test_app();
    let mut u = def("mage", 100.0);
    u.mana = Some(ManaDef { max: 20.0, regen: 0.0 });
    let e = spawn_with_slot(&mut app, u, vec![skill_def("bolt", 2.0, 30.0)], "bolt");

    cast(&mut app, e, 0);
    assert_eq!(count_over_frames::<SkillCast>(&mut app, 3), 0, "out of mana: skipped");
    assert_eq!(cooldown_of(&app, e), 0.0, "refused cast starts no cooldown");
    assert!((mana_of(&app, e) - 20.0).abs() < 1e-5, "mana untouched");
}

/// SPL1-3: regen advances at regen*dt per frame in the Tick slot and clamps
/// at MaxMana.
#[test]
fn mana_regen_ticks_and_clamps_at_max() {
    let mut app = test_app();
    let mut u = def("mage", 100.0);
    u.mana = Some(ManaDef { max: 100.0, regen: 2.0 });
    let reg = UnitRegistry { defs: [("mage".to_string(), u)].into_iter().collect() };
    app.insert_resource(UnitRegistry { defs: reg.defs.clone() });
    let e = spawn_unit(&mut app.world_mut(), &reg, "mage", Vec3::ZERO, Allegiance::Defender, 1.0)
        .unwrap();

    // Warm up one frame first: the harness clock has a first-frame dt=0
    // quirk (ts0_determinism), so regen is only exact from frame 2 on.
    app.update();
    // 0.5s at regen 2.0/s from 40.0 -> 41.0 exactly (30 frames).
    app.world_mut().get_mut::<Mana>(e).unwrap().0 = 40.0;
    for _ in 0..30 {
        app.update();
    }
    assert!(
        (mana_of(&app, e) - 41.0).abs() < 1e-4,
        "30 frames * 2.0/s: {}",
        mana_of(&app, e)
    );

    // Clamp: already at max, regen adds nothing.
    app.world_mut().get_mut::<Mana>(e).unwrap().0 = 100.0;
    for _ in 0..30 {
        app.update();
    }
    assert!((mana_of(&app, e) - 100.0).abs() < 1e-4, "clamped at max");
}

/// SPL1-4 (zero drift): `mana: None` units (every warden tower today) get
/// no mana components, and a zero-cost skill behaves exactly as before —
/// instant SkillCast, cooldown started at cast. The workspace-wide
/// regression run is the zero-drift proof at scale; this pins the seam.
#[test]
fn manaless_unit_zero_cost_skill_unchanged() {
    let mut app = test_app();
    let e = spawn_with_slot(&mut app, def("tower", 100.0), vec![skill_def("zap", 2.0, 0.0)], "zap");

    assert!(app.world().get::<Mana>(e).is_none(), "no mana bar");
    assert!(app.world().get::<MaxMana>(e).is_none());
    assert!(app.world().get::<ManaRegen>(e).is_none());

    cast(&mut app, e, 0);
    assert_eq!(count_over_frames::<SkillCast>(&mut app, 2), 1, "free cast still fires");
    // Cast on frame 1 sets cooldown 2.0; frame 2's Tick decrements once.
    let expected = 2.0 - STEP as f32;
    assert!(
        (cooldown_of(&app, e) - expected).abs() < 1e-5,
        "cooldown as before: {}",
        cooldown_of(&app, e)
    );
}
