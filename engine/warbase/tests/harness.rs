//! BX10 acceptance: reproducibility, injection, dual-bundle readings, and
//! the greedy baseline actually winning a winnable scenario.
//! WX5a: the full driver wordlist (upgrade/call-wave/cast/spend-point) and
//! the per-wave / per-origin report readings.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::math::Vec3;
use warbase::bundle::{assemble, build_grid, MapBundle};
use warbase::harness::{
    apply_driver_action, run_headless, DriverAction, DriverStrategy, GreedyDriver, HarnessOpts,
};
use warbase::prelude::*;

fn unit(id: &str, hp: f32, armor: ArmorType, dmg: f32, cost: u32) -> UnitDef {
    UnitDef {
        id: id.into(),
        attrs: BaseAttributes { hp, move_speed: 1.0, body_radius: 0.3 },
        attack: if dmg > 0.0 {
            Some(AttackDef {
                damage: dmg,
                interval: 0.3,
                range: 12.0,
                attack_type: AttackType::Magic,
                targets: TargetDomain::Both,
                projectile: Some(ProjDef { speed: 60.0 }),
            })
        } else {
            None
        },
        armor_type: armor,
        cost_gold: cost,
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
    }
}

fn bundle(leak: LeakPolicy, open: bool, waves: usize) -> MapBundle {
    let mut units = std::collections::HashMap::new();
    units.insert("grunt".into(), unit("grunt", 40.0, ArmorType::Heavy, 0.0, 0));
    units.insert("tower".into(), unit("tower", 80.0, ArmorType::Fortified, 20.0, 50));
    let path = if open {
        PathDef::open(vec![[-6.0, 0.0], [6.0, 0.0]])
    } else {
        PathDef::closed_loop(vec![[-6.0, -4.0], [6.0, -4.0], [6.0, 4.0], [-6.0, 4.0]])
    };
    MapBundle {
        identity: warbase::bundle::MapIdentity { id: "harness".into(), version: 1 },
        world: warbase::bundle::WorldDef {
            paths: vec![path],
            build: warbase::bundle::BuildDomainDef::FreeGrid { cell: 1.0, cols: 16, rows: 12 },
            blocked: vec![],
        },
        units,
        waves: WaveTable {
            waves: (0..waves)
                .map(|_| WaveEntry {
                    spawns: vec![SpawnSpec {
                        unit: "grunt".into(),
                        count: 2,
                        interval: 0.2,
                        path: 0,
                        delay: 0.0,
                        hp_scale: 1.0,
                    }],
                    boss_timeout: None,
                    preview: "2 grunts".into(),
                    reward_gold: 0,
                })
                .collect(),
            advance: Advance::Auto { intermission: 0.5 },
            early_call_bonus: None,
        },
        rules: MatchRules {
            leak,
            build_window: BuildWindow::Always,
            lap_buff: None,
            win: WinCondition::SurviveWaves { waves: waves as u32 },
            sell_refund_pct: 0.7,
        },
        econ_start: EconStart { gold: 200, wood: 0, pop_cap: 8 },
        econ_rules: EconRules::default(),
        attack_armor: AttackArmorTable::default(),
    }
}

/// UT-HAR-01: same seed => identical report (bit-for-bit floats included).
#[test]
fn same_seed_same_report() {
    let b = bundle(LeakPolicy::Stockpile { cap: 50 }, false, 1);
    let mut d1 = GreedyDriver { tower: "tower".into() };
    let mut d2 = GreedyDriver { tower: "tower".into() };
    let r1 = run_headless(&b, &mut d1, HarnessOpts { max_frames: 60 * 30, ..Default::default() });
    let r2 = run_headless(&b, &mut d2, HarnessOpts { max_frames: 60 * 30, ..Default::default() });
    assert_eq!(r1, r2);
    assert!(r1.outcome.is_ok(), "greedy tower clears the scenario: {:?}", r1.outcome);
    assert_eq!(r1.waves_cleared, 1);
    assert!(r1.damage_by_def.contains_key("tower"));
}

/// UT-HAR-02: injection spawns extra stock immediately.
#[test]
fn inject_stock_loses_immediately() {
    let b = bundle(LeakPolicy::Stockpile { cap: 2 }, false, 9);
    let mut driver = NoopDriver;
    let r = run_headless(
        &b,
        &mut driver,
        HarnessOpts {
            max_frames: 30,
            inject: Some(("grunt".into(), 2)),
            ..Default::default()
        },
    );
    assert_eq!(r.outcome, Err("stockpile full (2/2)".into()));
    assert_eq!(r.stock_peak, 2);
}

/// UT-HAR-03: the same driver + same base produces different readings on
/// different bundles (loop vs open-chain).
#[test]
fn dual_bundle_readings_differ() {
    let loop_b = bundle(LeakPolicy::Stockpile { cap: 50 }, false, 1);
    let open_b = bundle(LeakPolicy::Lives { lives: 10 }, true, 1);
    let mut d1 = GreedyDriver { tower: "tower".into() };
    let mut d2 = GreedyDriver { tower: "tower".into() };
    let r1 = run_headless(&loop_b, &mut d1, HarnessOpts { max_frames: 60 * 30, ..Default::default() });
    let r2 = run_headless(&open_b, &mut d2, HarnessOpts { max_frames: 60 * 30, ..Default::default() });
    assert!(r1.outcome.is_ok(), "loop scenario won: {:?}", r1.outcome);
    assert!(r2.outcome.is_ok(), "open scenario won: {:?}", r2.outcome);
    assert_ne!(r1.frames, r2.frames, "different maps, different timelines");
}

struct NoopDriver;
impl DriverStrategy for NoopDriver {
    fn decide(&mut self, _view: &warbase::harness::HarnessView) -> Vec<DriverAction> {
        Vec::new()
    }
}

// --- WX5a: driver wordlist + report readings ---------------------------------------

/// A bundle exercising the full wordlist: a tower -> tower2 upgrade chain,
/// a hero carrying one skill, Manual wave advance.
fn rich_bundle() -> MapBundle {
    let mut units = std::collections::HashMap::new();
    units.insert("grunt".into(), unit("grunt", 40.0, ArmorType::Heavy, 0.0, 0));
    units.insert("tower".into(), {
        let mut t = unit("tower", 80.0, ArmorType::Fortified, 20.0, 50);
        t.upgrade_to = Some("tower2".into());
        t
    });
    units.insert("tower2".into(), unit("tower2", 120.0, ArmorType::Fortified, 30.0, 80));
    let mut hero = unit("hero", 100.0, ArmorType::Fortified, 5.0, 90);
    hero.hero = true;
    hero.skills = vec!["snipe".into()];
    units.insert("hero".into(), hero);
    let mut waves = WaveTable::default();
    waves.waves.push(WaveEntry {
        spawns: vec![SpawnSpec {
            unit: "grunt".into(),
            count: 2,
            interval: 0.2,
            path: 0,
            delay: 0.0,
            hp_scale: 1.0,
        }],
        boss_timeout: None,
        preview: "2 grunts".into(),
        reward_gold: 0,
    });
    waves.advance = Advance::Manual;
    MapBundle {
        identity: warbase::bundle::MapIdentity { id: "rich".into(), version: 1 },
        world: warbase::bundle::WorldDef {
            paths: vec![PathDef::closed_loop(vec![[-6.0, -4.0], [6.0, -4.0], [6.0, 4.0], [-6.0, 4.0]])],
            build: warbase::bundle::BuildDomainDef::FreeGrid { cell: 1.0, cols: 16, rows: 12 },
            blocked: vec![],
        },
        units,
        waves,
        rules: MatchRules {
            leak: LeakPolicy::Stockpile { cap: 50 },
            build_window: BuildWindow::Always,
            lap_buff: None,
            win: WinCondition::SurviveWaves { waves: 1 },
            sell_refund_pct: 0.7,
        },
        econ_start: EconStart { gold: 300, wood: 0, pop_cap: 8 },
        econ_rules: EconRules::default(),
        attack_armor: AttackArmorTable::default(),
    }
}

/// WX5a AC1: the extended wordlist executes against a real assembled world —
/// place/upgrade walks the chain, CallWave writes StartWave, Cast issues the
/// request, SpendPoint consumes a banked point, unknown skills fail cleanly.
#[test]
fn driver_wordlist_executes() {
    let b = rich_bundle();
    let mut app = warbase::testing::test_app();
    assemble(&mut app.world_mut(), &b);
    app.insert_resource(build_grid(&b.world));
    // Register the hero's skill so the slot spawns learned (spawn reads
    // SkillDefs; any kind makes it level 1).
    {
        let mut defs = SkillDefs::default();
        defs.defs.insert(
            "snipe".into(),
            SkillDef {
                id: "snipe".into(),
                kind: SkillKind::Active,
                targeting: SkillTargeting::None,
                max_level: 3,
                cooldown: 5.0,
                badge: None,
                hotkey: None,
            },
        );
        app.insert_resource(defs);
    }
    let world = app.world_mut();
    // First buildable cell (the loop path's bounding box blocks the middle).
    let cell = {
        let grid = world.resource::<BuildGrid>();
        let mut found = None;
        'cells: for y in 0..grid.rows {
            for x in 0..grid.cols {
                let c = Cell(x, y);
                if grid.check(c).is_ok() {
                    found = Some(c);
                    break 'cells;
                }
            }
        }
        found.expect("a free cell")
    };
    // Place then upgrade: the occupant walks tower -> tower2.
    assert!(apply_driver_action(world, &DriverAction::Place { def: "tower".into(), cell }));
    assert!(apply_driver_action(world, &DriverAction::Upgrade { cell }), "chain upgrade");
    let occupant = world.resource::<BuildGrid>().occupant(cell).expect("occupied");
    assert_eq!(world.get::<UnitInstance>(occupant).unwrap().def, "tower2");
    assert!(
        !apply_driver_action(world, &DriverAction::Upgrade { cell }),
        "top of the chain rejects"
    );

    // Hero: bank points, spend one on snipe (slot 1 -> 2).
    let registry = world.resource::<UnitRegistry>().clone();
    let hero =
        spawn_unit(world, &registry, "hero", Vec3::new(3.0, 0.0, 3.0), Allegiance::Defender, 1.0)
            .expect("hero spawns");
    world.get_mut::<XpHolder>(hero).unwrap().points = 3;
    assert!(apply_driver_action(world, &DriverAction::SpendPoint { skill: "snipe".into() }));
    {
        let (caster, xp) = (
            world.get::<SkillCaster>(hero).unwrap(),
            world.get::<XpHolder>(hero).unwrap(),
        );
        assert_eq!(caster.slots[0].level, 2, "the point raised the slot");
        assert_eq!(xp.points, 2, "one point consumed");
    }

    // Cast issues the request; unknown skills fail cleanly.
    assert!(apply_driver_action(world, &DriverAction::Cast { skill: "snipe".into() }));
    assert!(!apply_driver_action(world, &DriverAction::Cast { skill: "nope".into() }));
    {
        let mut cursor = MessageCursor::<StartWave>::default();
        assert_eq!(cursor.read(world.resource::<Messages<StartWave>>()).count(), 0, "no wave yet");
    }
    assert!(apply_driver_action(world, &DriverAction::CallWave));
    let mut cursor = MessageCursor::<StartWave>::default();
    assert_eq!(cursor.read(world.resource::<Messages<StartWave>>()).count(), 1, "wave called");
}

/// WX5a AC2: the report carries per-wave clear times, per-wave leaks, and
/// damage split by origin channel (attack/skill/system).
#[test]
fn report_carries_wave_and_origin_readings() {
    let b = bundle(LeakPolicy::Stockpile { cap: 50 }, false, 2);
    let mut driver = GreedyDriver { tower: "tower".into() };
    let r = run_headless(
        &b,
        &mut driver,
        HarnessOpts { max_frames: 60 * 60, ..Default::default() },
    );
    assert!(r.outcome.is_ok(), "greedy clears: {:?}", r.outcome);
    assert_eq!(r.waves_cleared, 2);
    assert_eq!(r.wave_times.len(), 2, "one clear-time sample per wave");
    assert!(r.wave_times.iter().all(|t| *t > 0.0), "clear times positive ({:?})", r.wave_times);
    assert_eq!(r.leaks_per_wave.len(), 2);
    assert!(
        *r.damage_by_origin.get("attack").unwrap_or(&0.0) > 0.0,
        "tower damage lands in the attack column"
    );
    assert_eq!(
        r.damage_by_origin.get("skill").copied().unwrap_or(0.0),
        0.0,
        "no skill damage in this scenario"
    );
}
