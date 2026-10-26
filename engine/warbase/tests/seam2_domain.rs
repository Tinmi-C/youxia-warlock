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
        area_radius: 0.0, summon: None,
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

// -- ⑥ SEAM2c: command messages registered + consumed ------------------------

/// All four flipped command messages can be sent without panic and drive
/// their consumption chains end-to-end: rally attaches a RallyPoint,
/// research enqueues + charges, an item use fires its SkillCast effect, a
/// shop buy charges gold and stows the stack.
#[test]
fn command_messages_send_and_consume() {
    let mut app = test_app();

    // Rally: Set on a fresh builder inserts the component (insert path —
    // commands are deferred, so check after the update).
    let builder = app.world_mut().spawn_empty().id();
    app.world_mut().write_message(RallyCommands(RallyCommand::Set {
        builder,
        order: Order::Move { to: Vec3::new(1.0, 0.0, 2.0) },
    }));
    app.update();
    assert_eq!(
        app.world().get::<RallyPoint>(builder).and_then(|r| r.0.clone()),
        Some(Order::Move { to: Vec3::new(1.0, 0.0, 2.0) }),
        "rally Set consumed"
    );

    // Research: a table entry starts and charges on the same update.
    app.world_mut().insert_resource(ResearchTable(
        [(
            "atk-up".to_string(),
            ResearchDef {
                id: "atk-up".into(),
                cost_gold: 50,
                time: 10.0,
                grants: vec![],
            },
        )]
        .into_iter()
        .collect(),
    ));
    app.world_mut().resource_mut::<Resources>().gold = 100;
    app.world_mut()
        .write_message(ResearchCommands(ResearchCommand::Start { id: "atk-up".into() }));
    app.update();
    assert_eq!(app.world().resource::<ResearchState>().entries.len(), 1);
    assert_eq!(app.world().resource::<Resources>().gold, 50, "research charged");

    // Item use: a charged potion with a registered effect fires SkillCast.
    let potion = ItemDef {
        id: "potion".into(),
        name: "Potion".into(),
        kind: ItemKind::Consumable,
        charges: Some(1),
        cooldown: 0.0,
        effect: Some("potion_heal".into()),
        cost_gold: 30,
        passive: None,
    };
    app.world_mut()
        .insert_resource(ItemRegistry { defs: [("potion".into(), potion)].into_iter().collect() });
    app.world_mut()
        .resource_mut::<SkillDefs>()
        .defs
        .insert("potion_heal".into(), skill("potion_heal", SkillKind::Active));
    let hero = app
        .world_mut()
        .spawn((
            Inventory::default(),
            StatMods::default(),
            Transform::default(),
        ))
        .id();
    app.world_mut().get_mut::<Inventory>(hero).unwrap().slots[0] =
        Some(ItemStack { def: "potion".into(), charges: 1, cooldown: 0.0 });
    let mut casts = MessageCursor::<SkillCast>::default();
    app.world_mut()
        .write_message(ItemCommands(ItemCommand::Use { user: hero, slot: 0 }));
    app.update();
    let fired: Vec<SkillCast> = {
        let msgs = app.world().resource::<Messages<SkillCast>>();
        casts.read(msgs).cloned().collect()
    };
    assert_eq!(fired.len(), 1, "item use fires its effect cast");
    assert_eq!(fired[0].skill, "potion_heal");
    assert!(app.world().get::<Inventory>(hero).unwrap().slots[0].is_none(), "spent");

    // Shop buy: stock + funds + free slot -> charge + stow.
    let shop = app
        .world_mut()
        .spawn((Shop { stock: vec!["potion".into()] }, Transform::default()))
        .id();
    let buyer = app
        .world_mut()
        .spawn((Inventory::default(), StatMods::default(), Transform::default()))
        .id();
    app.world_mut().resource_mut::<Resources>().gold = 100;
    app.world_mut().write_message(ShopCommands(ShopCommand::Buy {
        buyer,
        shop,
        item: "potion".into(),
    }));
    app.update();
    assert_eq!(app.world().resource::<Resources>().gold, 70, "buy charged 30");
    assert_eq!(
        app.world().get::<Inventory>(buyer).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .def,
        "potion"
    );
}

// -- ⑦ SEAM2c: selection收口 ---------------------------------------------------

fn selectable_unit(world: &mut World, def: &str, pos: Vec2, side: Allegiance) -> Entity {
    world
        .spawn((
            Selectable,
            side,
            Transform::from_translation(Vec3::new(pos.x, 0.0, pos.y)),
            UnitInstance { def: def.into(), level: 1 },
        ))
        .id()
}

fn right_click(app: &mut App, at: Vec2) -> Vec<UnitCommand> {
    let mut attempts = MessageCursor::<PickAttempt>::default();
    let _ = &mut attempts;
    app.world_mut().write_message(PickAttempt {
        world: at,
        button: PickButton::Right,
        click_count: 1,
        ctrl: false,
        shift: false,
    });
    let mut cursor = MessageCursor::<UnitCommand>::default();
    app.update();
    let msgs = app.world().resource::<Messages<UnitCommand>>();
    cursor.read(msgs).cloned().collect()
}

/// Right-click a (Selectable) GroundItem -> Order::Pickup.
#[test]
fn right_click_ground_item_pickup() {
    let mut app = test_app();
    let worker = selectable_unit(app.world_mut(), "peasant", Vec2::new(0.0, 2.0), Allegiance::Defender);
    let item = app
        .world_mut()
        .spawn((
            Selectable,
            GroundItem { def: "potion".into(), grace: 0.0 },
            Transform::default(),
        ))
        .id();
    app.world_mut()
        .write_message(SelectCommand::Set(vec![worker]));
    app.update(); // selection settles

    let commands = right_click(&mut app, Vec2::ZERO);
    assert_eq!(commands.len(), 1);
    assert!(
        matches!(commands[0].order, Order::Pickup { item: e } if e == item),
        "hole-2/INV收口: {:?}",
        commands[0].order
    );
}

/// Right-click a friendly Selectable -> Order::Follow (was a silent no-op).
#[test]
fn right_click_friendly_follows() {
    let mut app = test_app();
    let worker = selectable_unit(app.world_mut(), "peasant", Vec2::new(0.0, 2.0), Allegiance::Defender);
    let hero = selectable_unit(app.world_mut(), "paladin", Vec2::ZERO, Allegiance::Defender);
    app.world_mut()
        .write_message(SelectCommand::Set(vec![worker]));
    app.update();

    let commands = right_click(&mut app, Vec2::ZERO);
    assert_eq!(commands.len(), 1);
    assert!(
        matches!(commands[0].order, Order::Follow { target: t } if t == hero),
        "SEAM2c: {:?}",
        commands[0].order
    );
}

/// M-key two-state machine: M arms Move targeting; the next left click
/// issues a plain Move to that point and disarms; while armed, a left
/// click never touches the selection set.
#[test]
fn m_key_arms_explicit_move() {
    let mut app = test_app();
    let unit = selectable_unit(app.world_mut(), "footman", Vec2::new(0.0, 5.0), Allegiance::Defender);
    app.world_mut()
        .write_message(SelectCommand::Set(vec![unit]));
    app.update();

    app.world_mut().write_message(CommandHotkey::Move);
    app.update();
    assert_eq!(
        app.world().resource::<PendingTargetOrder>().clone(),
        PendingTargetOrder::Move,
        "M arms the cursor"
    );

    // Left click targets the order (not a select).
    app.world_mut().write_message(PickAttempt {
        world: Vec2::new(3.0, 4.0),
        button: PickButton::Left,
        click_count: 1,
        ctrl: false,
        shift: false,
    });
    let mut cursor = MessageCursor::<UnitCommand>::default();
    app.update();
    let commands: Vec<UnitCommand> = {
        let msgs = app.world().resource::<Messages<UnitCommand>>();
        cursor.read(msgs).cloned().collect()
    };
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].order, Order::Move { to: Vec3::new(3.0, 0.0, 4.0) });
    assert_eq!(
        app.world().resource::<PendingTargetOrder>().clone(),
        PendingTargetOrder::Idle,
        "cursor disarmed after issuing"
    );
    assert_eq!(
        app.world().resource::<SelectionState>().entities,
        vec![unit],
        "targeting click never touches the selection"
    );
}

/// Double-click selects every on-screen body of the clicked def id: the
/// camera-visible AABB (`SelectionScreenRect`) bounds "on screen" — the
/// default infinite rect selects world-wide; a narrow rect clips.
#[test]
fn double_click_selects_same_type_on_screen() {
    let mut app = test_app();
    let near = selectable_unit(app.world_mut(), "footman", Vec2::new(0.0, 0.0), Allegiance::Defender);
    let far = selectable_unit(app.world_mut(), "footman", Vec2::new(50.0, 50.0), Allegiance::Defender);
    let other = selectable_unit(app.world_mut(), "archer", Vec2::new(1.0, 0.0), Allegiance::Defender);

    // Infinite default rect: both footmen, no archer.
    app.world_mut().write_message(PickAttempt {
        world: Vec2::ZERO,
        button: PickButton::Left,
        click_count: 2,
        ctrl: false,
        shift: false,
    });
    app.update(); // point select emits Set ...
    app.update(); // ... consumed next frame
    let mut selected = app.world().resource::<SelectionState>().entities.clone();
    selected.sort_by_key(|e| e.index());
    let mut expected = vec![near, far];
    expected.sort_by_key(|e| e.index());
    assert_eq!(selected, expected, "default rect: same-type world-wide");

    // Narrow rect around the origin: only the near footman qualifies
    // ("on screen" per the camera-visible AABB).
    app.world_mut().insert_resource(SelectionScreenRect {
        min: Vec2::new(-5.0, -5.0),
        max: Vec2::new(5.0, 5.0),
    });
    app.world_mut().write_message(PickAttempt {
        world: Vec2::ZERO,
        button: PickButton::Left,
        click_count: 2,
        ctrl: false,
        shift: false,
    });
    app.update();
    app.update();
    assert_eq!(
        app.world().resource::<SelectionState>().entities,
        vec![near],
        "narrow rect clips same-type to the visible AABB"
    );
    let _ = other;
}

// -- ⑧ SEAM2c: subgroup command filtering --------------------------------------

/// Three units, two subgroups: with the Tab cursor on the footman bucket,
/// a right-click order's subjects contain ONLY the footmen (WC3: orders go
/// to the active subgroup). A single-subgroup selection commands everyone.
#[test]
fn subgroup_cursor_filters_command_subjects() {
    let mut app = test_app();
    let f1 = selectable_unit(app.world_mut(), "footman", Vec2::new(0.0, 3.0), Allegiance::Defender);
    let f2 = selectable_unit(app.world_mut(), "footman", Vec2::new(1.0, 3.0), Allegiance::Defender);
    let archer = selectable_unit(app.world_mut(), "archer", Vec2::new(2.0, 3.0), Allegiance::Defender);
    // Sorted subgroup keys: ["archer", "footman"] — cursor 1 = footmen.
    app.world_mut()
        .write_message(SelectCommand::Set(vec![f1, f2, archer]));
    app.update();
    app.world_mut().resource_mut::<SelectionState>().subgroup_cursor = 1;

    let commands = right_click(&mut app, Vec2::new(10.0, 10.0));
    assert_eq!(commands.len(), 1);
    let mut subjects = commands[0].subjects.clone();
    subjects.sort_by_key(|e| e.index());
    let mut expected = vec![f1, f2];
    expected.sort_by_key(|e| e.index());
    assert_eq!(subjects, expected, "cursor on footman bucket: only footmen move");

    // Single subgroup: everyone commands (cursor irrelevant).
    app.world_mut()
        .write_message(SelectCommand::Set(vec![f1, f2]));
    app.update();
    let commands = right_click(&mut app, Vec2::new(-10.0, -10.0));
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].subjects.len(), 2, "single subgroup: all subjects");
}

// -- ⑨ SEAM2d: eight domain skeletons + MapBundle sections --------------------

use warbase::bundle::{assemble, validate, BuildDomainDef, MapIdentity, WorldDef};

/// Every pinned skeleton type is constructible through the prelude, and
/// the Order stubs exist in the movement vocabulary (stored-only until
/// their owning domains land).
#[test]
fn new_domain_types_construct() {
    // harvest (HARV added per_trip/dropoffs/upkeep_tiers; `..Default::default()`
    // keeps this seam assertion construction-valid without pinning the new fields)
    let h = HarvestDef {
        deposits: vec![MineDef { id: "gold".into(), pos: [1.0, 2.0], capacity: 250 }],
        ..Default::default()
    };
    assert_eq!(h.deposits.len(), 1);
    let mine = Mine { deposit: "gold".into(), remaining: 250 };
    assert_eq!(mine.remaining, 250);
    // build_rts
    let b = BuildOrderDef {
        time: 6.0,
        cost_gold: 100,
        cost_wood: 0,
        builders_accelerate: true,
    };
    assert!(b.builders_accelerate);
    let tier = TownTier(1);
    assert_eq!(tier.0, 1);
    // hero
    let stats = HeroStats { str: 10, agi: 7, int: 5 };
    assert_eq!((stats.str, stats.agi, stats.int), (10, 7, 5));
    let _conv = HeroStatsDef::default();
    // daynight
    let clock = DayClock { phase: DayPhase::Night, t: 3.0 };
    assert_eq!(clock.phase, DayPhase::Night);
    assert_eq!(DayNightDef::default().night_vision_factor, 1.0, "neutral default");
    // camera_rts
    assert_eq!(EdgeScroll::default().speed_px_per_sec, 0.0, "disabled default");
    let ping = MinimapPing { pos: Vec2::new(1.0, 2.0), team: TeamId(0) };
    assert_eq!(ping.pos, Vec2::new(1.0, 2.0));
    // camp
    let camps = CampsDef {
        camps: vec![CampDef {
            id: "troll_hole".into(),
            pos: [4.0, 4.0],
            radius: 5.0,
            respawn_secs: 60.0,
            drops: vec!["t1_scroll".into()],
        }],
    };
    assert_eq!(camps.camps.len(), 1);
    // save / spectate
    let slot = SaveSlot("arena_001".into());
    assert_eq!(slot.0, "arena_001");
    assert!(!SpectateDef::default().enabled);
    // terrain placeholder
    assert!(TerrainDef::default().high_cells.is_empty());
    // Order stub variants exist and park (no execution) in tick_orders.
    let stubs = vec![
        Order::Harvest { target: Entity::PLACEHOLDER },
        Order::ReturnCargo { target: Entity::PLACEHOLDER },
        Order::Build { what: "farm".into(), at: Vec3::ZERO },
    ];
    assert_eq!(stubs.len(), 3);
}

/// MinimapPing round-trips through the registered message stream (send
/// does not panic; the CAM increment consumes it later).
#[test]
fn minimap_ping_message_sendable() {
    let mut app = test_app();
    let mut cursor = MessageCursor::<MinimapPing>::default();
    app.world_mut()
        .write_message(MinimapPing { pos: Vec2::ONE, team: TeamId(1) });
    app.update();
    let msgs = app
        .world()
        .resource::<Messages<MinimapPing>>();
    assert_eq!(cursor.read(msgs).count(), 1, "ping registered and delivered");
}

/// Old maps load unchanged: a shipped pre-SEAM2d RON (no harvest/camps/
/// daynight/terrain section anywhere) validates, assembles, and leaves
/// every new resource at its neutral default.
#[test]
fn old_map_loads_without_new_sections() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../games/loop-td/maps/maze.ron");
    let bundle = warbase::bundle::load_bundle(&path).expect("shipped pre-SEAM2d map loads");
    validate(&bundle).unwrap();
    assert!(bundle.harvest.deposits.is_empty());
    assert!(bundle.camps.camps.is_empty());
    assert_eq!(bundle.daynight.day_secs, 0.0, "cycle disabled by default");
    assert_eq!(bundle.daynight.night_vision_factor, 1.0);
    assert!(bundle.terrain.high_cells.is_empty());
    let mut app = test_app();
    assemble(&mut app.world_mut(), &bundle);
    assert_eq!(app.world().resource::<DayClock>().phase, DayPhase::Day);
    assert!(!app.world().resource::<SpectateDef>().enabled);
}

/// Maps WITH the new sections round-trip and assemble (schema superset;
/// version NOT bumped).
#[test]
fn new_sections_parse_and_assemble() {
    let bundle = warbase::bundle::MapBundle {
        identity: MapIdentity { id: "seam2d".into(), version: warbase::bundle::MAP_FORMAT_VERSION },
        world: WorldDef {
            paths: vec![],
            build: BuildDomainDef::FreeGrid { cell: 1.0, cols: 8, rows: 8 },
            blocked: vec![],
        },
        units: registry(&[def("grunt")]).defs,
        waves: Default::default(),
        rules: MatchRules::default(),
        econ_start: EconStart { gold: 100, wood: 0, pop_cap: 10 },
        attack_armor: Default::default(),
        harvest: HarvestDef {
            deposits: vec![MineDef { id: "gold".into(), pos: [2.0, 2.0], capacity: 1500 }],
            ..Default::default()
        },
        camps: CampsDef {
            camps: vec![CampDef {
                id: "gnoll_camp".into(),
                pos: [5.0, 5.0],
                radius: 4.0,
                respawn_secs: 60.0,
                drops: vec![],
            }],
        },
        daynight: DayNightDef { day_secs: 480.0, night_secs: 240.0, night_vision_factor: 0.6 },
        terrain: TerrainDef { high_cells: vec![[1, 1], [2, 1]] },
        econ_rules: Default::default(),
        nav: Default::default(),
        vision: Default::default(),
        items: Default::default(),
        production: Default::default(),
        teams: Default::default(),
        starting_forces: vec![],
    };
    validate(&bundle).unwrap();
    // Serde round-trip of the section-bearing bundle.
    let text = ron::to_string(&bundle).unwrap();
    let back: warbase::bundle::MapBundle = ron::from_str(&text).unwrap();
    assert_eq!(back.harvest.deposits[0].capacity, 1500);
    assert_eq!(back.camps.camps[0].radius, 4.0);
    assert_eq!(back.daynight.night_vision_factor, 0.6);
    assert_eq!(back.terrain.high_cells, vec![[1, 1], [2, 1]]);
    let mut app = test_app();
    assemble(&mut app.world_mut(), &back);
    assert!(app.world().contains_resource::<DayClock>());
}

fn registry(defs: &[UnitDef]) -> UnitRegistry {
    UnitRegistry { defs: defs.iter().cloned().map(|d| (d.id.clone(), d)).collect() }
}
