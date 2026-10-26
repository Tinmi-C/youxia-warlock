//! SEAM2 acceptance (ADR-0018 / round-2 plan §2): the seam increment pins
//! the cross-domain contracts Phase 1's seven parallel lines build against.
//! Numbering matches the acceptance list: ① SkillCast.target round-trip
//! (hole-6); ② aura generalization with TD invariance (hole-7);
//! ③ ModalKeyClaim digit yield (hole-8); ④ UnitDef.skill_levels compat
//! states (hole-9); ⑤ CommandEnvelope vocabulary serde round-trip
//! (hole-10); ⑥ command messages registered + consumed (SEAM2c);
//! ⑦ selection收口: Pickup/Follow/M-key/double-click (SEAM2c);
//! ⑧ subgroup command filtering (SEAM2c); ⑨ eight new domain skeletons +
//! MapBundle sections default-load (SEAM2d).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::input::ButtonInput;
use bevy::prelude::*;
use warbase::prelude::*;
use warbase::testing::{test_app, STEP};

// -- shared fixtures ---------------------------------------------------------

fn def(id: &str) -> UnitDef {
    UnitDef {
        id: id.to_string(),
        attrs: BaseAttributes { hp: 100.0, move_speed: 3.0, body_radius: 0.3 },
        attack: None,
        armor_type: ArmorType::Medium,
        cost_gold: 0,
        cost_wood: 0,
        bounty_gold: 0,
        bounty_wood: 0,
        grant_xp: 0,
        traits: vec![],
        auras: vec![],
        hero: false,
        skills: vec![],
        skill_levels: vec![],
        upgrade_to: None,
        heal: None,
        mana: None,
        leak: 1,
    }
}

fn skill(id: &str, kind: SkillKind) -> SkillDef {
    SkillDef {
        id: id.to_string(),
        kind,
        targeting: SkillTargeting::None,
        max_level: 3,
        cooldown: 1.0,
        mana_cost: 0.0,
        cast_time: 0.0,
        channel: None,
        badge: None,
        hotkey: None,
    }
}

/// Drain SkillCast messages written since the last call.
fn drain_casts(app: &mut App, cursor: &mut MessageCursor<SkillCast>) -> Vec<SkillCast> {
    let msgs = app.world().resource::<Messages<SkillCast>>();
    cursor.read(msgs).cloned().collect()
}

// -- ① hole-6: SkillCast.target round-trip -----------------------------------

/// Instant path: CastSkill.target lands on SkillCast.target untouched.
#[test]
fn skillcast_target_passthrough_instant() {
    let mut app = test_app();
    let zap = skill("zap", SkillKind::Active);
    app.world_mut().resource_mut::<SkillDefs>().defs.insert("zap".into(), zap);
    let mut d = def("mage");
    d.skills = vec!["zap".into()];
    d.skill_levels = vec![1];
    let reg = UnitRegistry { defs: [("mage".into(), d)].into_iter().collect() };
    let caster = spawn_unit(
        app.world_mut(),
        &reg,
        "mage",
        Vec3::ZERO,
        Allegiance::Defender,
        1.0,
    )
    .unwrap();
    let victim = app.world_mut().spawn_empty().id();

    app.world_mut()
        .write_message(CastSkill { caster, slot: 0, target: Some(victim) });
    let mut cursor = MessageCursor::<SkillCast>::default();
    app.update();
    let casts = drain_casts(&mut app, &mut cursor);
    assert_eq!(casts.len(), 1, "instant cast fires on the request frame");
    assert_eq!(casts[0].target, Some(victim), "hole-6: target passes through");
    assert_eq!(casts[0].caster, caster);
    assert_eq!(casts[0].skill, "zap");
    assert_eq!(casts[0].level, 1);
    assert_eq!(casts[0].point, None);
}

/// Timed path: CastSkill.target parks on `Casting` and reaches the
/// completion-frame SkillCast.
#[test]
fn skillcast_target_passthrough_timed() {
    let mut app = test_app();
    let mut beam = skill("beam", SkillKind::Active);
    beam.cast_time = 2.0 * STEP as f32; // fires on the second update
    app.world_mut().resource_mut::<SkillDefs>().defs.insert("beam".into(), beam);
    let mut d = def("priest");
    d.skills = vec!["beam".into()];
    d.skill_levels = vec![1];
    let reg = UnitRegistry { defs: [("priest".into(), d)].into_iter().collect() };
    let caster = spawn_unit(
        app.world_mut(),
        &reg,
        "priest",
        Vec3::ZERO,
        Allegiance::Defender,
        1.0,
    )
    .unwrap();
    let victim = app.world_mut().spawn_empty().id();

    app.world_mut()
        .write_message(CastSkill { caster, slot: 0, target: Some(victim) });
    let mut cursor = MessageCursor::<SkillCast>::default();
    app.update(); // frame 1: Casting attached, no cast yet
    assert!(drain_casts(&mut app, &mut cursor).is_empty(), "still casting");
    assert_eq!(
        app.world().get::<Casting>(caster).map(|c| c.target),
        Some(Some(victim)),
        "Casting parks the aimed target"
    );
    // Completion: bounded wait (the first update may carry a zero delta,
    // so the 2-frame cast can land on the third update).
    let mut casts = Vec::new();
    for _ in 0..5 {
        app.update();
        casts = drain_casts(&mut app, &mut cursor);
        if !casts.is_empty() {
            break;
        }
    }
    assert_eq!(casts.len(), 1, "timed cast fires at completion");
    assert_eq!(casts[0].target, Some(victim), "hole-6: target survives the wait");
}

// -- ② hole-7: aura generalization -------------------------------------------

/// TD invariance, value-for-value: an Enemies aura tower slows a track
/// creep by exactly the authored amount, and moving the carrier out of
/// range restores the stat — the pre-generalization behavior.
#[test]
fn tower_aura_on_track_creep_is_unchanged() {
    let mut app = test_app();
    let path = app
        .world_mut()
        .spawn(warbase::prelude::PathDef::open(vec![[0.0, 0.0], [10.0, 0.0]]))
        .id();
    let creep = app
        .world_mut()
        .spawn((
            PathFollower { path, progress: 0.0, laps: 0, finished: false },
            MoveSpeed(1.0),
            Transform::default(),
            StatMods::default(),
            Allegiance::Attacker,
            UnitInstance { def: "creep".into(), level: 1 },
        ))
        .id();
    let tower = app
        .world_mut()
        .spawn((
            Transform::from_xyz(0.0, 0.0, 0.0),
            Allegiance::Defender,
            Auras(vec![AuraDef {
                radius: 4.0,
                kind: StatKind::MoveSpeedMult,
                amount: -0.5,
                affects: AuraAffects::Enemies,
            }]),
            UnitInstance { def: "slow_tower".into(), level: 1 },
        ))
        .id();

    app.update();
    app.update();
    let eff = *app.world().get::<EffectiveStats>(creep).unwrap();
    assert!((eff.move_mult - 0.5).abs() < 1e-5, "slowed: {}", eff.move_mult);

    app.world_mut()
        .entity_mut(tower)
        .insert(Transform::from_xyz(100.0, 0.0, 0.0));
    app.update();
    app.update();
    let eff = *app.world().get::<EffectiveStats>(creep).unwrap();
    assert!((eff.move_mult - 1.0).abs() < 1e-5, "restored: {}", eff.move_mult);
}

/// Positive generalization: an Allies aura reaches FREE units (no
/// PathFollower anywhere) — carrier included — and skips hostiles; an
/// Enemies aura reaches free hostile units too.
#[test]
fn free_unit_auras_apply_after_generalization() {
    let mut app = test_app();
    let paladin = app
        .world_mut()
        .spawn((
            Transform::from_xyz(0.0, 0.0, 0.0),
            Allegiance::Defender,
            StatMods::default(),
            Auras(vec![AuraDef {
                radius: 4.0,
                kind: StatKind::DamageTakenMult,
                amount: -0.15,
                affects: AuraAffects::Allies,
            }]),
            UnitInstance { def: "paladin".into(), level: 1 },
        ))
        .id();
    let footman = app
        .world_mut()
        .spawn((
            Transform::from_xyz(1.0, 0.0, 0.0),
            Allegiance::Defender,
            StatMods::default(),
            UnitInstance { def: "footman".into(), level: 1 },
        ))
        .id();
    let grunt = app
        .world_mut()
        .spawn((
            Transform::from_xyz(2.0, 0.0, 0.0),
            Allegiance::Attacker,
            StatMods::default(),
            UnitInstance { def: "grunt".into(), level: 1 },
        ))
        .id();

    app.update();
    app.update();
    let fm = *app.world().get::<EffectiveStats>(footman).unwrap();
    assert!(
        (fm.damage_taken_mult - 0.85).abs() < 1e-5,
        "hole-7: free ally inside devotion: {}",
        fm.damage_taken_mult
    );
    let pal = *app.world().get::<EffectiveStats>(paladin).unwrap();
    assert!(
        (pal.damage_taken_mult - 0.85).abs() < 1e-5,
        "carrier buffs itself: {}",
        pal.damage_taken_mult
    );
    let gr = *app.world().get::<EffectiveStats>(grunt).unwrap();
    assert!(
        (gr.damage_taken_mult - 1.0).abs() < 1e-5,
        "allies aura never touches hostiles: {}",
        gr.damage_taken_mult
    );

    // Enemies side of the generalization: a hostile-debuff aura carrier
    // reaches a FREE hostile unit (pre-SEAM2b this needed PathFollower).
    app.world_mut().spawn((
        Transform::from_xyz(2.0, 0.0, 0.5),
        Allegiance::Defender,
        Auras(vec![AuraDef {
            radius: 4.0,
            kind: StatKind::DamageTakenMult,
            amount: 0.2,
            affects: AuraAffects::Enemies,
        }]),
        UnitInstance { def: "curse_tower".into(), level: 1 },
    ));
    app.update();
    app.update();
    let gr = *app.world().get::<EffectiveStats>(grunt).unwrap();
    assert!(
        (gr.damage_taken_mult - 1.2).abs() < 1e-5,
        "enemies aura reaches free hostiles: {}",
        gr.damage_taken_mult
    );
}

/// Selective strip: hostile and allied auras on the SAME target no longer
/// erase each other across the two rebuild systems (the latent pre-SEAM2b
/// cross-strip: whichever system ran second wiped the other's entries).
#[test]
fn hostile_and_allied_auras_coexist_on_one_target() {
    let mut app = test_app();
    let path = app
        .world_mut()
        .spawn(warbase::prelude::PathDef::open(vec![[0.0, 0.0], [10.0, 0.0]]))
        .id();
    let creep = app
        .world_mut()
        .spawn((
            PathFollower { path, progress: 0.0, laps: 0, finished: false },
            Transform::default(),
            StatMods::default(),
            Allegiance::Attacker,
            UnitInstance { def: "creep".into(), level: 1 },
        ))
        .id();
    // Allied shaman (creep-side) + hostile slow tower both in range.
    app.world_mut().spawn((
        Transform::from_xyz(0.5, 0.0, 0.0),
        Allegiance::Attacker,
        Auras(vec![AuraDef {
            radius: 4.0,
            kind: StatKind::DamageTakenMult,
            amount: -0.25,
            affects: AuraAffects::Allies,
        }]),
        UnitInstance { def: "shaman".into(), level: 1 },
    ));
    app.world_mut().spawn((
        Transform::from_xyz(0.0, 0.0, 0.5),
        Allegiance::Defender,
        Auras(vec![AuraDef {
            radius: 4.0,
            kind: StatKind::MoveSpeedMult,
            amount: -0.5,
            affects: AuraAffects::Enemies,
        }]),
        UnitInstance { def: "slow_tower".into(), level: 1 },
    ));

    app.update();
    app.update();
    let eff = *app.world().get::<EffectiveStats>(creep).unwrap();
    assert!(
        (eff.move_mult - 0.5).abs() < 1e-5,
        "hostile aura entry survives the allied rebuild: {}",
        eff.move_mult
    );
    assert!(
        (eff.damage_taken_mult - 0.75).abs() < 1e-5,
        "allied aura entry survives the hostile rebuild: {}",
        eff.damage_taken_mult
    );
}

// -- ③ hole-8: ModalKeyClaim digit yield -------------------------------------

fn press_key(app: &mut App, code: KeyCode) {
    let mut keys = ButtonInput::<KeyCode>::default();
    keys.press(code);
    app.insert_resource(keys);
}

/// While a modal holds the claim, digit presses produce NO GroupCommand;
/// once released, the same press recalls group 1 as before.
#[test]
fn modal_key_claim_yields_digits() {
    let mut app = test_app();

    // Claimed: digit 1 belongs to the modal.
    app.world_mut()
        .insert_resource(ModalKeyClaim(Some("item_hotbar")));
    press_key(&mut app, KeyCode::Digit1);
    let mut cursor = MessageCursor::<GroupCommand>::default();
    app.update();
    let written: Vec<GroupCommand> = {
        let msgs = app.world().resource::<Messages<GroupCommand>>();
        cursor.read(msgs).copied().collect()
    };
    assert!(
        written.is_empty(),
        "hole-8: claimed digits never reach groups: {written:?}"
    );

    // Released: the identical press recalls group 1 (legacy behavior).
    app.world_mut().insert_resource(ModalKeyClaim(None));
    press_key(&mut app, KeyCode::Digit1);
    let mut cursor = MessageCursor::<GroupCommand>::default();
    app.update();
    let msgs = app.world().resource::<Messages<GroupCommand>>();
    let written: Vec<GroupCommand> = cursor.read(msgs).copied().collect();
    assert_eq!(written.len(), 1);
    assert!(matches!(written[0], GroupCommand::Recall(1)), "{written:?}");
}

/// The yield is digit-only: with a claim held, the Tab hotkey still
/// translates (modals do not own the whole keyboard).
#[test]
fn modal_key_claim_keeps_other_keys_live() {
    let mut app = test_app();
    app.world_mut()
        .insert_resource(ModalKeyClaim(Some("item_hotbar")));
    press_key(&mut app, KeyCode::Tab);
    let mut cursor = MessageCursor::<CycleSubgroups>::default();
    app.update();
    let msgs = app.world().resource::<Messages<CycleSubgroups>>();
    assert_eq!(cursor.read(msgs).count(), 1, "Tab stays live under a claim");
}

// -- ④ hole-9: UnitDef.skill_levels compat states ----------------------------

fn spawn_with_levels(app: &mut App, levels: Vec<u8>) -> Vec<(String, u8, bool)> {
    for (id, kind) in
        [("act", SkillKind::Active), ("auto", SkillKind::Autocast), ("pas", SkillKind::Passive)]
    {
        let d = skill(id, kind);
        app.world_mut()
            .resource_mut::<SkillDefs>()
            .defs
            .insert(id.to_string(), d);
    }
    let mut d = def("caster");
    d.skills = vec!["act".into(), "auto".into(), "pas".into()];
    d.skill_levels = levels;
    let reg = UnitRegistry { defs: [("caster".into(), d)].into_iter().collect() };
    let e = spawn_unit(app.world_mut(), &reg, "caster", Vec3::ZERO, Allegiance::Defender, 1.0)
        .unwrap();
    app.world()
        .get::<SkillCaster>(e)
        .unwrap()
        .slots
        .iter()
        .map(|s| (s.skill.clone(), s.level, s.autocast_on))
        .collect::<Vec<(String, u8, bool)>>()
}

/// Equal length covers slot-for-slot.
#[test]
fn skill_levels_equal_length_overrides() {
    let mut app = test_app();
    let slots = spawn_with_levels(&mut app, vec![2, 3, 1]);
    let levels: Vec<u8> = slots.iter().map(|s| s.1).collect();
    assert_eq!(levels, vec![2, 3, 1], "authored levels win over defaults");
    assert!(slots[1].2, "autocast slot still starts ON");
}

/// Shorter vecs pad with 0; Autocast resolves 0 -> 1 (legacy semantics).
#[test]
fn skill_levels_short_vec_pads_and_autocast_bumps() {
    let mut app = test_app();
    let slots = spawn_with_levels(&mut app, vec![2]);
    let levels: Vec<u8> = slots.iter().map(|s| s.1).collect();
    assert_eq!(levels, vec![2, 1, 0], "pad 0, autocast 0 -> 1, passive 0");
}

/// Empty vec (the serde default) reproduces the pre-SEAM2b spawn exactly:
/// Active/Passive 0, Autocast 1.
#[test]
fn skill_levels_empty_matches_legacy_spawn() {
    let mut app = test_app();
    let slots = spawn_with_levels(&mut app, vec![]);
    let levels: Vec<u8> = slots.iter().map(|s| s.1).collect();
    assert_eq!(levels, vec![0, 1, 0], "bit-identical legacy behavior");
}

// -- ⑤ hole-10: CommandEnvelope vocabulary serde round-trip -------------------

/// Every pinned variant round-trips through RON with PartialEq intact —
/// the vocabulary is Entity-free and stable for the RPL recorder.
#[test]
fn command_envelope_new_variants_roundtrip() {
    let variants = vec![
        CommandEnvelope::Select { op: 1 },
        CommandEnvelope::Order { kind: "move".into(), to: [3.5, -2.0] },
        CommandEnvelope::Train { unit: "footman".into() },
        CommandEnvelope::Research { id: "attack_upgrade".into() },
        CommandEnvelope::UseItem { slot: 0 },
    ];
    for e in variants {
        let text = ron::to_string(&e).unwrap();
        let back: CommandEnvelope = ron::from_str(&text).unwrap();
        assert_eq!(e, back, "round-trip: {text}");
    }
    // The full log (with the new vocabulary in frames) round-trips too.
    let log = MatchInputLog {
        version: INPUT_LOG_VERSION,
        map_hash: 7,
        seed: 42,
        frames: vec![(
            5,
            vec![
                CommandEnvelope::Order { kind: "harvest".into(), to: [1.0, 2.0] },
                CommandEnvelope::Train { unit: "peasant".into() },
            ],
        )],
        hashes: vec![(5, 99)],
    };
    let text = ron::to_string(&log).unwrap();
    let back: MatchInputLog = ron::from_str(&text).unwrap();
    assert_eq!(log.frames, back.frames, "frames round-trip (MatchInputLog has no PartialEq; element-wise)");
    assert_eq!(log.hashes, back.hashes);
}
