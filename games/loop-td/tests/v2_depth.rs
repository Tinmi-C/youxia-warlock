//! LT2 (round-2 main course): the six RPG-depth mechanisms on top of the
//! round-1 base + LT-ADOPT seams — equipment on hero towers, the artifact
//! quest aura, challenge waves, treasure digs, tower bonds, and the
//! multiplayer leak pass. Spec: docs/topics/game-design/loop-td-v2-rpg-proposal.md §2.
//! Everything here drives the REAL game composition (GamePlugin on the base
//! test app), same stance as tests/adopt_domain.rs.

use bevy::ecs::message::Messages;
use bevy::prelude::*;
use loop_td::plugins::game::GamePlugin;
use loop_td::systems::gameplay::try_place;
use warbase::bundle::MapBundle;
use warbase::prelude::*;
use warbase::testing::test_app;

const LOOP_MAP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/maps/loop.ron");

/// The real game headless (same recipe as adopt_domain): base test app +
/// GamePlugin; Startup loads the shipped map (grid, cards, item table).
fn game_app() -> App {
    let mut app = test_app();
    app.add_plugins(GamePlugin);
    app.insert_resource(MatchRng::seeded(42));
    app.update();
    app.update();
    app
}

/// Replace the raw keyboard resource (headless has no InputPlugin).
fn press_keys(app: &mut App, codes: &[KeyCode]) {
    let mut keys = ButtonInput::<KeyCode>::default();
    for code in codes {
        keys.press(*code);
    }
    app.insert_resource(keys);
}

/// First free grid cell (row-major).
fn first_free_cell(grid: &BuildGrid) -> Cell {
    (0..grid.cols)
        .flat_map(|x| (0..grid.rows).map(move |y| Cell(x, y)))
        .find(|c| grid.check(*c).is_ok())
        .expect("a free cell")
}

/// Place the hero tower (funded; hunter costs 150 > 120 start gold) and let
/// one update run so the attach kits (Selectable / Inventory / EquipSlots)
/// land on the fresh spawn.
fn place_hero(app: &mut App) -> Entity {
    app.world_mut().resource_mut::<Resources>().gold += 200;
    let cell = first_free_cell(app.world().resource::<BuildGrid>());
    let hero = try_place(&mut app.world_mut(), "hunter", cell).expect("place hunter");
    app.update(); // attach_td_kit + attach_equip_slots (Added<UnitInstance>)
    hero
}

// ---------------------------------------------------------------------------
// LT2-1: equipment on hero towers — 3 equip slots, affixes ride StatMods with
// a dedicated marker source per slot (dodges the INV same-source flaw).
// ---------------------------------------------------------------------------

#[test]
fn equipment_injects_replaces_and_falls_back() {
    use loop_td::systems::equip::{equip_item, unequip_slot, EquipSlots};

    let mut app = game_app();
    let hero = place_hero(&mut app);
    assert!(
        app.world().get::<EquipSlots>(hero).is_some(),
        "hero towers carry equip slots"
    );

    // 攻+12% blade -> damage_mult 1.12.
    equip_item(&mut app.world_mut(), hero, 0, "blade12").expect("equip blade12");
    app.update(); // aggregate_stats folds the entry
    let eff = app.world().get::<EffectiveStats>(hero).unwrap();
    assert!((eff.damage_mult - 1.12).abs() < 1e-5, "got {}", eff.damage_mult);

    // Exactly one DamageMult entry from the slot source (no stacking dupes).
    let mods = app.world().get::<StatMods>(hero).unwrap();
    let dmg_entries = mods
        .entries
        .iter()
        .filter(|e| e.kind == StatKind::DamageMult)
        .count();
    assert_eq!(dmg_entries, 1, "one entry per equipped item");

    // Unequip -> falls back to 1.0.
    unequip_slot(&mut app.world_mut(), hero, 0).expect("unequip");
    app.update();
    let eff = app.world().get::<EffectiveStats>(hero).unwrap();
    assert!((eff.damage_mult - 1.0).abs() < 1e-5, "got {}", eff.damage_mult);

    // Different item into the SAME slot -> new value, old gone (撤旧上新).
    equip_item(&mut app.world_mut(), hero, 0, "blade08").expect("equip blade08");
    app.update();
    let eff = app.world().get::<EffectiveStats>(hero).unwrap();
    assert!((eff.damage_mult - 1.08).abs() < 1e-5, "got {}", eff.damage_mult);

    // Same-slot replacement while occupied: blade12 over blade08 -> 1.12.
    equip_item(&mut app.world_mut(), hero, 0, "blade12").expect("replace in place");
    app.update();
    let eff = app.world().get::<EffectiveStats>(hero).unwrap();
    assert!((eff.damage_mult - 1.12).abs() < 1e-5, "got {}", eff.damage_mult);
    // The replaced blade08 went back into the hero's bag (Inventory).
    let inv = app.world().get::<Inventory>(hero).unwrap();
    let bag: Vec<&str> = inv
        .slots
        .iter()
        .filter_map(|s| s.as_ref().map(|st| st.def.as_str()))
        .collect();
    assert!(bag.contains(&"blade08"), "replaced item stowed back: {bag:?}");
}

#[test]
fn equipment_range_affix_and_guards() {
    use loop_td::systems::equip::{equip_item, unequip_slot, EquipSlots, EQUIP_SLOTS};

    let mut app = game_app();
    let hero = place_hero(&mut app);

    // Hunter base range 5.0 (maps/loop.ron); scope adds +1.5 flat.
    equip_item(&mut app.world_mut(), hero, 1, "scope").expect("equip scope");
    app.update();
    let kit = app.world().get::<AttackKit>(hero).unwrap();
    assert!((kit.def.range - 6.5).abs() < 1e-5, "got {}", kit.def.range);
    // Interval affix rides StatMods (quiver -10% attack interval).
    equip_item(&mut app.world_mut(), hero, 2, "quiver").expect("equip quiver");
    app.update();
    let eff = app.world().get::<EffectiveStats>(hero).unwrap();
    assert!((eff.interval_mult - 0.90).abs() < 1e-5, "got {}", eff.interval_mult);

    unequip_slot(&mut app.world_mut(), hero, 1).expect("unequip scope");
    app.update();
    let kit = app.world().get::<AttackKit>(hero).unwrap();
    assert!((kit.def.range - 5.0).abs() < 1e-5, "range restored");

    // Guards: slot index, unknown item, non-equipment rejects, plain towers.
    assert!(equip_item(&mut app.world_mut(), hero, EQUIP_SLOTS, "scope").is_err());
    assert!(equip_item(&mut app.world_mut(), hero, 0, "no-such-item").is_err());
    let cell = first_free_cell(app.world().resource::<BuildGrid>());
    let arrow = try_place(&mut app.world_mut(), "arrow", cell).unwrap();
    app.update();
    assert!(app.world().get::<EquipSlots>(arrow).is_none(), "plain towers carry none");
    assert!(equip_item(&mut app.world_mut(), arrow, 0, "blade12").is_err());
}

// ---------------------------------------------------------------------------
// LT2-2: the artifact quest — clear N=10 waves + kill M=2 bosses, then a
// team-unique claim grants every tower on the field an aura entry whose
// source is the artifact entity (claim gate + grant asserted).
// ---------------------------------------------------------------------------

fn wave_cleared(app: &mut App, wave: usize) {
    app.world_mut()
        .resource_mut::<Messages<WaveCleared>>()
        .write(WaveCleared(wave));
    app.update(); // artifact progress system counts it
}

fn boss_killed(app: &mut App) {
    app.world_mut()
        .resource_mut::<Messages<UnitDied>>()
        .write(UnitDied {
            entity: Entity::PLACEHOLDER,
            def: "boss".into(),
            gold: 0,
            wood: 0,
            xp: 0,
            killer: Some(DamageOrigin::System),
        });
    app.update();
}

#[test]
fn artifact_quest_gates_claim_and_grants_team_aura() {
    use loop_td::systems::artifact::{
        apply_artifact_aura, claim_artifact, ARTIFACT_AURA_AMOUNT, ARTIFACT_BOSSES_REQUIRED,
        ARTIFACT_WAVES_REQUIRED,
    };

    let mut app = game_app();
    // Two towers on the field ("全塔" = every Defender unit).
    let cell_a = first_free_cell(app.world().resource::<BuildGrid>());
    let arrow = try_place(&mut app.world_mut(), "arrow", cell_a).unwrap();
    let hero = place_hero(&mut app);
    app.update();

    // 9 waves + 1 boss: not enough — the claim refuses with both gaps named.
    for w in 0..9 {
        wave_cleared(&mut app, w);
    }
    boss_killed(&mut app);
    let err = claim_artifact(&mut app.world_mut()).unwrap_err();
    assert!(
        err.contains("waves") && err.contains("boss"),
        "readable gate reason, got '{err}'"
    );

    // 10 waves + 2 bosses: eligible. Claim before that still refuses.
    wave_cleared(&mut app, 9);
    let err = claim_artifact(&mut app.world_mut()).unwrap_err();
    assert!(err.contains("boss"), "boss gap still gates, got '{err}'");
    boss_killed(&mut app);
    assert_eq!(ARTIFACT_WAVES_REQUIRED, 10);
    assert_eq!(ARTIFACT_BOSSES_REQUIRED, 2);
    claim_artifact(&mut app.world_mut()).expect("claim after quest complete");

    // Every Defender tower now carries the aura entry (source = artifact
    // entity) folded into EffectiveStats.
    apply_artifact_aura(&mut app.world_mut());
    app.update();
    app.update();
    let quest = app.world().resource::<loop_td::systems::artifact::ArtifactQuest>();
    assert!(quest.claimed, "claim booked");
    let mut checked = 0;
    {
        let mut q = app
            .world_mut()
            .query_filtered::<&EffectiveStats, (With<UnitInstance>, With<Allegiance>)>();
        let world = app.world();
        for eff in q.iter(world) {
            checked += 1;
            assert!(
                (eff.damage_mult - (1.0 + ARTIFACT_AURA_AMOUNT)).abs() < 1e-5,
                "tower at {} vs {}",
                eff.damage_mult,
                1.0 + ARTIFACT_AURA_AMOUNT
            );
        }
    }
    assert!(checked >= 2, "both pre-claim towers covered ({checked})");
    for tower in [arrow, hero] {
        let eff = app.world().get::<EffectiveStats>(tower).unwrap();
        assert!(
            (eff.damage_mult - (1.0 + ARTIFACT_AURA_AMOUNT)).abs() < 1e-5,
            "tower {:?} at {}",
            tower,
            eff.damage_mult
        );
    }

    // A tower placed AFTER the claim joins the aura on the next pass.
    let cell_b = first_free_cell(app.world().resource::<BuildGrid>());
    let late = try_place(&mut app.world_mut(), "arrow", cell_b).unwrap();
    app.update();
    apply_artifact_aura(&mut app.world_mut());
    app.update();
    app.update();
    let eff = app.world().get::<EffectiveStats>(late).unwrap();
    assert!((eff.damage_mult - (1.0 + ARTIFACT_AURA_AMOUNT)).abs() < 1e-5, "late tower");

    // Team-unique: a second claim refuses.
    assert!(claim_artifact(&mut app.world_mut()).is_err(), "artifact is unique");
}

// ---------------------------------------------------------------------------
// LT2-3: challenge waves — every 10th wave milestone offers one optional
// challenge during the intermission (key C): the next wave's hp_scale is
// doubled and its reward tripled; clearing it restores the table and drops
// exactly one guaranteed equipment GroundItem.
// ---------------------------------------------------------------------------

/// Scenario bundle for the challenge mechanism: 12 tiny waves (reward 10,
/// bounty-free grunts), equipment in the item table, fast intermission.
fn challenge_bundle() -> MapBundle {
    let unit = |id: &str, hp: f32, dmg: f32| UnitDef {
        hero_stats: None,
        leak: 1,
        id: id.into(),
        attrs: BaseAttributes { hp, move_speed: 1.0, body_radius: 0.3 },
        attack: if dmg > 0.0 {
            Some(AttackDef {
                damage: dmg, interval: 0.3, range: 12.0, attack_type: AttackType::Magic,
                targets: TargetDomain::Both, projectile: Some(ProjDef { speed: 60.0 }),
                windup_secs: 0.0, backswing_secs: 0.0,
            })
        } else { None },
        armor_type: ArmorType::Heavy,
        cost_gold: 50, cost_wood: 0,
        bounty_gold: 0, bounty_wood: 0, grant_xp: 0,
        traits: vec![], auras: vec![], hero: false, skills: vec![],
        upgrade_to: None, heal: None, mana: None, skill_levels: vec![],
    };
    let mut units = std::collections::HashMap::new();
    units.insert("grunt".into(), unit("grunt", 40.0, 0.0));
    units.insert("tower".into(), unit("tower", 80.0, 20.0));
    let mut items = std::collections::HashMap::new();
    items.insert(
        "blade08".into(),
        ItemDef {
            id: "blade08".into(), name: "制式利刃".into(), kind: ItemKind::Passive,
            charges: None, cooldown: 0.0, effect: None, cost_gold: 60,
            passive: Some(PassiveStat { kind: StatKind::DamageMult, amount: 0.08 }),
        },
    );
    MapBundle {
        identity: warbase::bundle::MapIdentity { id: "lt2-challenge".into(), version: 1 },
        world: warbase::bundle::WorldDef {
            paths: vec![PathDef::closed_loop(vec![[-6.0, -4.0], [6.0, -4.0], [6.0, 4.0], [-6.0, 4.0]])],
            build: warbase::bundle::BuildDomainDef::FreeGrid { cell: 1.0, cols: 16, rows: 12 },
            blocked: vec![],
        },
        units,
        waves: WaveTable {
            waves: (0..12)
                .map(|i| WaveEntry {
                    spawns: vec![SpawnSpec {
                        unit: "grunt".into(), count: 2, interval: 0.1, path: 0,
                        delay: 0.0, hp_scale: 1.0 + i as f32 * 0.1,
                    }],
                    boss_timeout: None,
                    preview: format!("wave {i}"),
                    reward_gold: 10,
                })
                .collect(),
            advance: Advance::Auto { intermission: 0.1 },
            early_call_bonus: None,
        },
        rules: MatchRules {
            leak: LeakPolicy::Stockpile { cap: 50 },
            build_window: BuildWindow::Always,
            lap_buff: None,
            win: WinCondition::SurviveWaves { waves: 12 },
            sell_refund_pct: 0.7,
        },
        econ_start: warbase::prelude::EconStart { gold: 200, wood: 0, pop_cap: 8 },
        econ_rules: EconRules::default(),
        attack_armor: AttackArmorTable::default(),
        nav: Default::default(),
        vision: Default::default(),
        items: warbase::prelude::ItemsDef { items, drops: Default::default() },
        production: Default::default(),
        teams: Default::default(),
        starting_forces: Default::default(),
        harvest: Default::default(),
        camps: Default::default(),
        daynight: Default::default(),
        terrain: Default::default(),
    }
}

#[test]
fn challenge_wave_doubles_hp_triples_reward_and_drops_equipment() {
    use loop_td::systems::challenge::{
        arm_challenge, challenge_progress, ChallengeSpawn, CHALLENGE_HP_MULT,
        CHALLENGE_MILESTONE, CHALLENGE_REWARD_MULT,
    };

    let bundle = challenge_bundle();
    let mut app = test_app();
    app.insert_resource(MatchRng::seeded(3));
    warbase::bundle::assemble(&mut app.world_mut(), &bundle);
    app.insert_resource(loop_td::systems::gameplay::build_grid_from(&bundle));
    let base_hp_scale = bundle.waves.waves[10].spawns[0].hp_scale;
    let base_reward = bundle.waves.waves[10].reward_gold;

    // Guards: no offer outside a 10-wave milestone, none while spawning,
    // none once this milestone's challenge was already taken.
    app.world_mut().resource_mut::<WaveDirector>().next_wave = 9;
    app.world_mut().resource_mut::<WaveDirector>().phase = DirectorPhase::Intermission;
    assert!(arm_challenge(&mut app.world_mut()).is_err(), "not a milestone");

    let mut dir = app.world_mut().resource_mut::<WaveDirector>();
    dir.next_wave = CHALLENGE_MILESTONE;
    dir.phase = DirectorPhase::Spawning;
    drop(dir);
    assert!(arm_challenge(&mut app.world_mut()).is_err(), "no offer while a wave runs");

    app.world_mut().resource_mut::<WaveDirector>().phase = DirectorPhase::Intermission;
    let w = arm_challenge(&mut app.world_mut()).expect("arm at the milestone");
    assert_eq!(w, CHALLENGE_MILESTONE);

    // Table mutation: hp x2, reward x3 — asserted against the shipped values.
    let entry = &app.world().resource::<WaveTable>().waves[w];
    assert!((entry.spawns[0].hp_scale - base_hp_scale * CHALLENGE_HP_MULT).abs() < 1e-6);
    assert_eq!(entry.reward_gold, base_reward * CHALLENGE_REWARD_MULT);

    // The challenge wave actually spawns doubled-hp units tagged as challenges.
    app.world_mut()
        .resource_mut::<Messages<StartWave>>()
        .write(StartWave);
    let mut saw_challenge_unit = false;
    for _ in 0..30 {
        app.update();
        challenge_progress(&mut app.world_mut());
        let mut q = app
            .world_mut()
            .query_filtered::<&MaxHp, (With<WaveTag>, With<ChallengeSpawn>)>();
        if let Some(max) = q.iter(app.world()).next() {
            assert!((max.0 - 40.0 * base_hp_scale * CHALLENGE_HP_MULT).abs() < 1e-4, "hp {}", max.0);
            saw_challenge_unit = true;
            break;
        }
    }
    assert!(saw_challenge_unit, "challenge unit spawned and tagged");

    // Clear the wave by hand; the reward grant is EXACTLY 3x (bounty-free
    // creeps) and exactly one equipment GroundItem lands.
    let gold_before = app.world().resource::<Resources>().gold;
    for _ in 0..600 {
        // Kill every live attacker of the wave.
        let victims: Vec<Entity> = {
            let mut q = app
                .world_mut()
                .query_filtered::<(Entity, &Allegiance), With<PathFollower>>();
            let world = app.world_mut();
            q.iter(world)
                .filter(|&(_, side)| *side == Allegiance::Attacker)
                .map(|(e, _)| e)
                .collect()
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
        app.update();
        challenge_progress(&mut app.world_mut());
        let restored = &app.world().resource::<WaveTable>().waves[w];
        if (restored.spawns[0].hp_scale - base_hp_scale).abs() < 1e-9 {
            break; // restore ran => wave cleared
        }
    }
    // The director writes the reward as an EconCommand on the clear frame;
    // apply_econ settles it on the NEXT frame's Tick.
    for _ in 0..3 {
        app.update();
    }
    let gold_after = app.world().resource::<Resources>().gold;
    assert_eq!(gold_after - gold_before, base_reward * CHALLENGE_REWARD_MULT, "exactly 3x reward");
    let entry = &app.world().resource::<WaveTable>().waves[w];
    assert!((entry.spawns[0].hp_scale - base_hp_scale).abs() < 1e-9, "table restored");
    assert_eq!(entry.reward_gold, base_reward, "reward restored");
    let ground: Vec<&str> = {
        let mut q = app.world_mut().query_filtered::<&GroundItem, ()>();
        let world = app.world();
        q.iter(world).map(|g| g.def.as_str()).collect()
    };
    assert_eq!(ground.len(), 1, "exactly one guaranteed drop: {ground:?}");
    assert!(matches!(ground[0], "blade08"), "drop is equipment");

    // The milestone's offer is spent (same milestone re-visited refuses).
    {
        let mut dir = app.world_mut().resource_mut::<WaveDirector>();
        dir.next_wave = CHALLENGE_MILESTONE;
        dir.phase = DirectorPhase::Intermission;
    }
    assert!(arm_challenge(&mut app.world_mut()).is_err(), "one challenge per milestone");
}

// ---------------------------------------------------------------------------
// LT2-4: treasure digs — 50 gold during an intermission rolls the seeded
// MatchRng: 50% nothing / 35% +80 gold / 15% a random equipment piece.
// Same seed => identical outcome sequence; the gold expectation over a big
// sample sits inside the proposal's 31.5±5 window.
// ---------------------------------------------------------------------------

#[test]
fn digs_are_seeded_with_expected_gold_value() {
    use loop_td::systems::dig::{dig, DigOutcome, DIG_COST, DIG_GOLD_AMOUNT};

    let mut app = game_app();
    app.world_mut().resource_mut::<WaveDirector>().phase = DirectorPhase::Intermission;
    app.world_mut().resource_mut::<Resources>().gold = 1_000_000;

    // Guards: no dig outside the intermission, none without the 50 gold.
    app.world_mut().resource_mut::<WaveDirector>().phase = DirectorPhase::Spawning;
    assert!(dig(&mut app.world_mut()).is_err(), "digs are intermission-only");
    app.world_mut().resource_mut::<WaveDirector>().phase = DirectorPhase::Intermission;
    app.world_mut().resource_mut::<Resources>().gold = DIG_COST - 1;
    assert!(dig(&mut app.world_mut()).is_err(), "no gold, no dig");

    // Seeded determinism: the same seed replays the same 20-roll sequence.
    app.insert_resource(MatchRng::seeded(1234));
    app.world_mut().resource_mut::<Resources>().gold = 1_000_000;
    let seq_a: Vec<DigOutcome> = (0..20).map(|_| dig(&mut app.world_mut()).unwrap()).collect();
    app.insert_resource(MatchRng::seeded(1234));
    let seq_b: Vec<DigOutcome> = (0..20).map(|_| dig(&mut app.world_mut()).unwrap()).collect();
    assert!(!seq_a.is_empty() && seq_a != vec![DigOutcome::Nothing; 20], "sequence has variety");
    assert_eq!(seq_a, seq_b, "same seed => same sequence");

    // One gold outcome observed with exact accounting (50 spent, 80 back).
    let idx = seq_a.iter().position(|o| matches!(o, DigOutcome::Gold(_)));
    if let Some(idx) = idx {
        app.insert_resource(MatchRng::seeded(1234));
        let before = app.world().resource::<Resources>().gold;
        let mut saw_gold = false;
        for _ in 0..=idx {
            if let DigOutcome::Gold(won) = dig(&mut app.world_mut()).unwrap() {
                assert_eq!(won, DIG_GOLD_AMOUNT, "gold dig pays exactly 80");
                saw_gold = true;
            }
        }
        assert!(saw_gold, "replay reaches the same gold roll");
        let now = app.world().resource::<Resources>().gold;
        assert_eq!(now, before - DIG_COST * (idx as u32 + 1) + DIG_GOLD_AMOUNT);
    }

    // Big sample: gold expectation inside 31.5±5 (proposal EV window).
    app.insert_resource(MatchRng::seeded(7));
    app.world_mut().resource_mut::<Resources>().gold = 1_000_000;
    let mut gold_total = 0u32;
    for _ in 0..500 {
        if let DigOutcome::Gold(won) = dig(&mut app.world_mut()).unwrap() {
            gold_total += won;
        }
    }
    let mean = gold_total as f32 / 500.0;
    assert!(
        (mean - 31.5).abs() <= 5.0,
        "dig EV {mean} outside 31.5±5 (gold_total {gold_total})"
    );
    let stats = app.world().resource::<loop_td::systems::dig::DigStats>();
    let expected_rolls = 20 + 20 + idx.map(|i| i as u32 + 1).unwrap_or(0) + 500;
    assert_eq!(stats.rolls, expected_rolls, "every dig is booked");
    assert!(stats.equipment_found > 0, "equipment branch exercised");
}

#[test]
fn dig_equipment_lands_in_a_hero_bag_or_on_the_ground() {
    use loop_td::systems::dig::dig;

    // With a hero on the field, equipment outcomes stow into its bag.
    let mut app = game_app();
    app.world_mut().resource_mut::<WaveDirector>().phase = DirectorPhase::Intermission;
    let hero = place_hero(&mut app);
    app.world_mut().resource_mut::<Resources>().gold = 1_000_000;
    let mut stowed = false;
    for seed in 1..50u64 {
        app.insert_resource(MatchRng::seeded(seed));
        if let loop_td::systems::dig::DigOutcome::Equipment(id) = dig(&mut app.world_mut()).unwrap()
        {
            let inv = app.world().get::<Inventory>(hero).unwrap();
            let bag: Vec<&str> = inv
                .slots
                .iter()
                .filter_map(|s| s.as_ref().map(|st| st.def.as_str()))
                .collect();
            assert!(bag.contains(&id.as_str()), "equipment '{id}' in the hero bag: {bag:?}");
            stowed = true;
            break;
        }
    }
    assert!(stowed, "some seed digs equipment within 50 tries");

    // Without any hero, the piece drops as a GroundItem (observable loot).
    let mut bare = game_app();
    bare.world_mut().resource_mut::<WaveDirector>().phase = DirectorPhase::Intermission;
    bare.world_mut().resource_mut::<Resources>().gold = 1_000_000;
    let mut dropped = false;
    for seed in 1..50u64 {
        bare.insert_resource(MatchRng::seeded(seed));
        if let loop_td::systems::dig::DigOutcome::Equipment(ref id) =
            dig(&mut bare.world_mut()).unwrap()
        {
            let ground: Vec<&str> = {
                let mut q = bare.world_mut().query_filtered::<&GroundItem, ()>();
                let world = bare.world();
                q.iter(world).map(|g| g.def.as_str()).collect()
            };
            assert_eq!(ground, vec![id.as_str()], "dropped where the hero can see it");
            dropped = true;
            break;
        }
    }
    assert!(dropped, "some seed drops equipment within 50 tries");
}

// ---------------------------------------------------------------------------
// LT2-5: tower bonds — the game-side maps/bonds.ron table maps tower-type
// counts on the field to a StatMods grant on EVERY tower (marker source per
// bond); selling below the threshold withdraws it.
// ---------------------------------------------------------------------------

#[test]
fn bonds_link_tower_combos_to_all_tower_stats() {
    use loop_td::systems::bonds::apply_bonds;

    let mut app = game_app();
    // 2 arrows + 1 ice satisfies "arrow_ice" (arrow x2 + ice x1).
    app.world_mut().resource_mut::<Resources>().gold += 200; // 2x50 + 60 ice
    let grid = app.world().resource::<BuildGrid>().clone();
    let mut cells = (0..grid.cols)
        .flat_map(|x| (0..grid.rows).map(move |y| Cell(x, y)))
        .filter(|c| grid.check(*c).is_ok());
    let arrow_cells: Vec<Cell> = cells.by_ref().take(2).collect();
    let ice_cell = cells.next().unwrap();
    for &c in &arrow_cells {
        try_place(&mut app.world_mut(), "arrow", c).expect("place arrow");
    }
    let ice = try_place(&mut app.world_mut(), "ice", ice_cell).expect("place ice");
    let arrows: Vec<Entity> = {
        let mut q = app
            .world_mut()
            .query_filtered::<(Entity, &Allegiance), With<UnitInstance>>();
        let world = app.world_mut();
        q.iter(world)
            .filter(|&(_, side)| *side == Allegiance::Defender)
            .map(|(e, _)| e)
            .collect()
    };
    assert_eq!(arrows.len(), 3, "2 arrows + 1 ice on the field");

    // Bond satisfied -> EVERY tower (ice included) at +10% damage.
    apply_bonds(&mut app.world_mut());
    app.update();
    app.update();
    for tower in &arrows {
        let eff = app.world().get::<EffectiveStats>(*tower).unwrap();
        assert!((eff.damage_mult - 1.10).abs() < 1e-5, "tower at {}", eff.damage_mult);
    }

    // The bond entry rides a marker source distinct from other writers:
    // equipping on a hero coexists rather than overwriting.
    // (covered by composition; here: sell one arrow -> threshold broken.)
    try_sell(&mut app.world_mut(), arrow_cells[0]).expect("sell one arrow");
    apply_bonds(&mut app.world_mut());
    app.update();
    app.update();
    let survivors: Vec<Entity> = arrows
        .iter()
        .copied()
        .filter(|e| app.world().get::<UnitInstance>(*e).is_some())
        .collect();
    assert_eq!(survivors.len(), 2, "ice + one arrow remain");
    for tower in &survivors {
        let eff = app.world().get::<EffectiveStats>(*tower).unwrap();
        assert!((eff.damage_mult - 1.0).abs() < 1e-5, "bond withdrawn: {}", eff.damage_mult);
    }
    // No stale entries from the withdrawn bond remain anywhere.
    let mods = app.world().get::<StatMods>(ice).unwrap();
    assert!(
        mods.entries.iter().all(|e| e.amount != 0.10),
        "ice carries no stale bond entry: {:?}",
        mods.entries
    );
}
