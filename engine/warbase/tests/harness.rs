//! BX10 acceptance: reproducibility, injection, dual-bundle readings, and
//! the greedy baseline actually winning a winnable scenario.

use warbase::bundle::MapBundle;
use warbase::harness::{run_headless, DriverAction, DriverStrategy, GreedyDriver, HarnessOpts};
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
