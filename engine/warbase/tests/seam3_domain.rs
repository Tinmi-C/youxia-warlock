//! SEAM3 regression (ADR-0019, round-3 plan §Phase 0): pins the eight
//! debt-uplift contracts + the spectate/replay vocabulary. Each test names
//! its debt item in the doc comment; zero drift for everything else is
//! pinned by the pre-existing suites (baseline 530).

use bevy::ecs::message::Messages;
use bevy::prelude::*;
use warbase::prelude::*;
use warbase::testing::test_app;

// ---------------------------------------------------------------------------
// Shared fixtures
// ---------------------------------------------------------------------------

/// Tiny unit def helper for hand-built bundles and direct spawns.
fn unit_def(id: &str, hp: f32, dmg: f32, range: f32) -> UnitDef {
    UnitDef {
        hero_stats: None,
        leak: 1,
        id: id.into(),
        attrs: BaseAttributes { hp, move_speed: 1.0, body_radius: 0.3 },
        attack: if dmg > 0.0 {
            Some(AttackDef {
                damage: dmg,
                interval: 0.3,
                range,
                attack_type: AttackType::Magic,
                targets: TargetDomain::Both,
                projectile: None,
                windup_secs: 0.0,
                backswing_secs: 0.0,
            })
        } else {
            None
        },
        armor_type: ArmorType::Heavy,
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
        skill_levels: vec![],
    }
}

// ---------------------------------------------------------------------------
// Debt 1 — StatKind::RangeMult: the full chain (StatMods entry -> folded
// EffectiveStats.range_mult -> attack_range -> pick_targets reach).
// ---------------------------------------------------------------------------

/// A range mod folds into EffectiveStats and multiplies the def range.
#[test]
fn range_mod_folds_and_multiplies_attack_range() {
    let def = unit_def("tower", 100.0, 10.0, 4.0);
    let attack = def.attack.clone().unwrap();

    // Neutral (no stats yet) = the legacy def range, byte for byte.
    assert!((attack_range(&attack, None) - 4.0).abs() < 1e-6);
    assert!((attack_range(&attack, Some(&EffectiveStats::neutral())) - 4.0).abs() < 1e-6);

    let mut eff = EffectiveStats::neutral();
    eff.range_mult = 1.3; // +30% (the folded sum of RangeMult deltas)
    assert!((attack_range(&attack, Some(&eff)) - 5.2).abs() < 1e-6);

    // Clamped at zero: a -200% stack cannot invent negative range.
    eff.range_mult = -2.0;
    assert!(attack_range(&attack, Some(&eff)) >= 0.0);
    assert!((attack_range(&attack, Some(&eff)) - 0.0).abs() < 1e-6);
}

/// StatMods with RangeMult entries aggregates into range_mult (sum of same-
/// kind deltas from distinct sources — the affix-stacking contract).
#[test]
fn range_mods_aggregate_from_distinct_sources() {
    let mut app = test_app();
    let src_a = app.world_mut().spawn_empty().id();
    let src_b = app.world_mut().spawn_empty().id();
    let tower = app
        .world_mut()
        .spawn((
            UnitInstance { def: "t".into(), level: 1 },
            Allegiance::Defender,
            AttackKit {
                def: unit_def("t", 100.0, 10.0, 4.0).attack.unwrap(),
                priority: TargetingPriority::First,
            },
            StatMods {
                entries: vec![
                    ModEntry {
                        source: src_a,
                        kind: StatKind::RangeMult,
                        amount: 0.3,
                        expires_at: None,
                        from_aura: false,
                    },
                    ModEntry {
                        source: src_b,
                        kind: StatKind::RangeMult,
                        amount: -0.1,
                        expires_at: None,
                        from_aura: false,
                    },
                ],
            },
            Transform::default(),
        ))
        .id();
    app.update(); // aggregate_stats (Tick)
    let eff = app.world().get::<EffectiveStats>(tower).unwrap();
    assert!((eff.range_mult - 1.2).abs() < 1e-5, "got {}", eff.range_mult);
}

/// The full chain end-to-end: an out-of-reach creep becomes targetable once
/// a +RangeMult entry lands on the tower (pick_targets is the read side).
#[test]
fn range_mod_extends_pick_targets_reach() {
    let mut app = test_app();
    let mut e = app.world_mut();
    let tower = e
        .spawn((
            UnitInstance { def: "t".into(), level: 1 },
            Allegiance::Defender,
            AttackKit {
                def: unit_def("t", 100.0, 10.0, 4.0).attack.unwrap(),
                priority: TargetingPriority::First,
            },
            Transform::default(),
            CurrentTarget::default(),
        ))
        .id();
    // Creep at distance 4.8 — outside the base 4.0 range.
    let creep = e
        .spawn((
            UnitInstance { def: "c".into(), level: 1 },
            Allegiance::Attacker,
            Hp(50.0),
            MaxHp(50.0),
            Transform::from_xyz(4.8, 0.0, 0.0),
        ))
        .id();
    drop(e);
    app.update();
    assert!(
        app.world().get::<CurrentTarget>(tower).unwrap().0.is_none(),
        "base range 4.0 cannot reach 4.8"
    );

    // Equip-style +30% range: 4.0 x 1.3 = 5.2 > 4.8 — now targetable.
    let affix_source = app.world_mut().spawn_empty().id();
    app.world_mut().entity_mut(tower).insert(StatMods {
        entries: vec![ModEntry {
            source: affix_source,
            kind: StatKind::RangeMult,
            amount: 0.3,
            expires_at: None,
            from_aura: false,
        }],
    });
    app.update(); // aggregate folds, pick_targets re-picks
    assert_eq!(
        app.world().get::<CurrentTarget>(tower).unwrap().0,
        Some(creep),
        "range mod extends targeting reach"
    );
}

// ---------------------------------------------------------------------------
// Debt 2 — WaveTag variant typing: serde compatibility + challenge tagging.
// ---------------------------------------------------------------------------

/// Serde wire compatibility: a plain number is Normal (old RON data), Normal
/// serializes BACK as the number, Challenge round-trips explicitly.
#[test]
fn wave_tag_serde_maps_legacy_numbers() {
    // Legacy numeric form -> Normal.
    let tag: WaveTag = ron::from_str("5").unwrap();
    assert_eq!(tag, WaveTag::Normal(5));
    assert_eq!(tag.index(), 5);
    assert!(!tag.is_challenge());

    // Normal serializes back as the plain number (old consumers see no drift).
    let text = ron::to_string(&WaveTag::Normal(5)).unwrap();
    assert_eq!(text.trim(), "5");

    // Explicit variant forms round-trip.
    for (src, expect) in [("Normal(3)", WaveTag::Normal(3)), ("Challenge(4)", WaveTag::Challenge(4))] {
        let tag: WaveTag = ron::from_str(src).unwrap();
        assert_eq!(tag, expect);
        let back = ron::to_string(&tag).unwrap();
        let re: WaveTag = ron::from_str(&back).unwrap();
        assert_eq!(re, expect, "{src} round-trips through {back}");
    }
    assert_eq!(WaveTag::Challenge(4).index(), 4);
    assert!(WaveTag::Challenge(4).is_challenge());

    // The RON map file path (serde structs carrying a wave tag) parses the
    // legacy integer too — old saves/fixtures keep loading.
    #[derive(serde::Deserialize, serde::Serialize, PartialEq, Debug)]
    struct Saved {
        tag: WaveTag,
    }
    let s: Saved = ron::from_str("(tag: 9)").unwrap();
    assert_eq!(s, Saved { tag: WaveTag::Normal(9) });
}

/// WaveEntry.challenge drives the director's spawn tag: challenge waves get
/// WaveTag::Challenge, plain waves Normal; cleared detection counts both.
#[test]
fn challenge_entry_flags_spawn_tag_kind() {
    let mut units = std::collections::HashMap::new();
    units.insert("grunt".into(), unit_def("grunt", 40.0, 0.0, 0.0));
    let bundle = warbase::bundle::MapBundle {
        identity: warbase::bundle::MapIdentity { id: "seam3-chal".into(), version: 1 },
        world: warbase::bundle::WorldDef {
            paths: vec![PathDef::closed_loop(vec![[-6.0, -4.0], [6.0, -4.0], [6.0, 4.0], [-6.0, 4.0]])],
            build: warbase::bundle::BuildDomainDef::FreeGrid { cell: 1.0, cols: 16, rows: 12 },
            blocked: vec![],
        },
        units,
        waves: WaveTable {
            waves: vec![
                WaveEntry {
                    spawns: vec![SpawnSpec {
                        unit: "grunt".into(),
                        count: 1,
                        interval: 0.1,
                        path: 0,
                        delay: 0.0,
                        hp_scale: 1.0,
                    }],
                    boss_timeout: None,
                    preview: "plain".into(),
                    reward_gold: 0,
                    challenge: false,
                },
                WaveEntry {
                    spawns: vec![SpawnSpec {
                        unit: "grunt".into(),
                        count: 1,
                        interval: 0.1,
                        path: 0,
                        delay: 0.0,
                        hp_scale: 1.0,
                    }],
                    boss_timeout: None,
                    preview: "挑战!".into(),
                    reward_gold: 0,
                    challenge: true,
                },
            ],
            advance: Advance::Manual,
            early_call_bonus: None,
        },
        rules: MatchRules {
            leak: LeakPolicy::Stockpile { cap: 50 },
            build_window: BuildWindow::Always,
            lap_buff: None,
            win: WinCondition::Endless,
            sell_refund_pct: 0.7,
        },
        econ_start: EconStart { gold: 100, wood: 0, pop_cap: 8 },
        econ_rules: EconRules::default(),
        attack_armor: AttackArmorTable::default(),
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
    };
    let mut app = test_app();
    warbase::bundle::assemble(&mut app.world_mut(), &bundle);

    // Wave 0 (plain): Normal tag.
    app.world_mut()
        .resource_mut::<Messages<StartWave>>()
        .write(StartWave);
    app.update();
    let tags: Vec<WaveTag> = {
        let mut q = app.world_mut().query::<&WaveTag>();
        q.iter(app.world()).copied().collect()
    };
    assert!(tags.contains(&WaveTag::Normal(0)), "plain wave tags Normal: {tags:?}");

    // Kill the wave so the director advances.
    let victims: Vec<Entity> = {
        let mut q = app
            .world_mut()
            .query_filtered::<Entity, With<PathFollower>>();
        q.iter(app.world()).collect()
    };
    for v in victims {
        app.world_mut()
            .resource_mut::<Messages<DamageRequest>>()
            .write(DamageRequest {
                target: v,
                source: DamageOrigin::System,
                amount: 999_999.0,
                attack_type: AttackType::Magic,
                crit: false,
            });
    }
    for _ in 0..5 {
        app.update();
    }

    // Wave 1 (challenge flag): Challenge tag.
    app.world_mut()
        .resource_mut::<Messages<StartWave>>()
        .write(StartWave);
    app.update();
    let tags: Vec<WaveTag> = {
        let mut q = app.world_mut().query::<&WaveTag>();
        q.iter(app.world()).copied().collect()
    };
    assert!(tags.contains(&WaveTag::Challenge(1)), "flagged wave tags Challenge: {tags:?}");
}

// ---------------------------------------------------------------------------
// Debt 4 — CommandAction::Custom registry seam: game-defined slots dispatch
// through the SAME card pipeline (hotkey + click -> CommandCardAction).
// ---------------------------------------------------------------------------

/// A Custom slot on the active card dispatches via the card hotkey path and
/// arrives as `CommandAction::Custom(id)` — the game-side dispatch arm.
#[test]
fn custom_action_dispatches_through_the_card() {
    use warbase::ui::{
        CommandCardDef, CommandCards, CommandSlotDef, Hotkey, Selection, SelectionMode,
        BUILD_CARD,
    };
    let mut app = test_app();
    let mut cards = CommandCards::default();
    cards.cards.insert(
        BUILD_CARD.to_string(),
        CommandCardDef {
            slots: vec![CommandSlotDef {
                label: "dig".into(),
                hotkey: Hotkey::Key(KeyCode::KeyV),
                tooltip: "dig once".into(),
                action: warbase::ui::CommandAction::Custom("dig".into()),
                enabled: true,
                badge: Some('V'),
            }],
        },
    );
    app.insert_resource(cards)
        .insert_resource(Selection { mode: SelectionMode::Build })
        .init_resource::<ButtonInput<KeyCode>>();
    // Press V: the base command_input bridge fires the action message.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyV);
    app.update();
    let mut cursor = bevy::ecs::message::MessageCursor::<
        warbase::ui::CommandCardAction,
    >::default();
    let actions: Vec<_> = {
        let msgs = app
            .world()
            .resource::<Messages<warbase::ui::CommandCardAction>>();
        cursor.read(msgs).cloned().collect()
    };
    assert_eq!(actions.len(), 1, "hotkey dispatch fires once");
    assert_eq!(
        actions[0].action,
        warbase::ui::CommandAction::Custom("dig".into())
    );
    assert_eq!(actions[0].selection, SelectionMode::Build);

    // Disabled custom slots stay inert (WX3 semantics cover customs too).
    let mut cards = app.world_mut().resource_mut::<CommandCards>();
    cards.cards.get_mut(BUILD_CARD).unwrap().slots[0].enabled = false;
    drop(cards);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .clear();
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyV);
    app.update();
    let fired: usize = {
        let msgs = app
            .world()
            .resource::<Messages<warbase::ui::CommandCardAction>>();
        cursor.read(msgs).count()
    };
    assert_eq!(fired, 0, "a disabled custom slot never dispatches");

    // CommandAction is serde (SEAM3c envelope payload prerequisite).
    let text = ron::to_string(&warbase::ui::CommandAction::Custom("dig".into())).unwrap();
    let back: warbase::ui::CommandAction = ron::from_str(&text).unwrap();
    assert_eq!(back, warbase::ui::CommandAction::Custom("dig".into()));
}
