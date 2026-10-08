//! G-LP behavior regression (thin-game acceptance): everything here drives
//! the game through warbase mechanisms + the thin loop-td composition layer
//! (maps/*.ron, try_place/try_sell, build_grid_from). No pipeline code
//! exists in this crate — that's the point under test.

use bevy::prelude::*;
use loop_td::systems::gameplay::{build_grid_from, try_place, try_sell};
use warbase::bundle::{assemble, load_bundle, validate, MapBundle};
use warbase::prelude::*;
use warbase::testing::{count_over_frames, test_app};

const LOOP_MAP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/maps/loop.ron");
const MAZE_MAP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/maps/maze-smoke.ron");

/// Headless armed app from a bundle: base + assemble + game grid.
fn armed(bundle: &MapBundle) -> App {
    let mut app = test_app();
    assemble(&mut app.world_mut(), bundle);
    let grid = build_grid_from(bundle);
    app.insert_resource(grid);
    app
}

/// A tiny authored bundle for targeted scenarios (cap/win/boss/armor).
fn scenario_bundle(
    leak: LeakPolicy,
    win_waves: u32,
    boss_timeout: Option<f32>,
) -> MapBundle {
    let unit = |id: &str, hp: f32, armor: ArmorType, dmg: f32| UnitDef {
        id: id.into(),
        attrs: BaseAttributes { hp, move_speed: 1.0, body_radius: 0.3 },
        attack: if dmg > 0.0 {
            Some(AttackDef {
                damage: dmg, interval: 0.3, range: 12.0, attack_type: AttackType::Magic,
                targets: TargetDomain::Both, projectile: Some(ProjDef { speed: 60.0 }),
            })
        } else { None },
        armor_type: armor,
        cost_gold: 50, cost_wood: 0,
        bounty_gold: 0, bounty_wood: 0, grant_xp: 0,
        traits: vec![],
        auras: vec![],
        hero: false,
        skills: vec![],
    };
    let mut units = std::collections::HashMap::new();
    units.insert("grunt".into(), unit("grunt", 40.0, ArmorType::Heavy, 0.0));
    units.insert("tower".into(), unit("tower", 80.0, ArmorType::Fortified, 20.0));
    MapBundle {
        identity: warbase::bundle::MapIdentity { id: "scenario".into(), version: 1 },
        world: warbase::bundle::WorldDef {
            paths: vec![PathDef::closed_loop(vec![[-6.0, -4.0], [6.0, -4.0], [6.0, 4.0], [-6.0, 4.0]])],
            build: warbase::bundle::BuildDomainDef::FreeGrid { cell: 1.0, cols: 16, rows: 12 },
            blocked: vec![],
        },
        units,
        waves: WaveTable {
            waves: vec![WaveEntry {
                spawns: vec![SpawnSpec { unit: "grunt".into(), count: 2, interval: 0.2, path: 0, delay: 0.0, hp_scale: 1.0 }],
                boss_timeout,
                preview: "scenario".into(),
            }],
            advance: Advance::Auto { intermission: 0.5 },
            early_call_bonus: None,
        },
        rules: MatchRules {
            leak,
            build_window: BuildWindow::Always,
            lap_buff: None,
            win: WinCondition::SurviveWaves { waves: win_waves },
            sell_refund_pct: 0.7,
        },
        econ_start: EconStart { gold: 200, wood: 0, pop_cap: 8 },
        econ_rules: EconRules::default(),
        attack_armor: AttackArmorTable::default(),
    }
}

/// G-LP-00a: the shipped map file loads, validates and runs wave 1.
#[test]
fn shipped_loop_map_loads_and_runs() {
    let bundle = load_bundle(std::path::Path::new(LOOP_MAP)).expect("maps/loop.ron loads");
    validate(&bundle).expect("maps/loop.ron validates");
    assert_eq!(bundle.waves.waves.len(), 30);
    assert_eq!(bundle.identity.id, "loop");

    let mut app = armed(&bundle);
    // Fast-forward the first intermission (12 s => 720 frames) via early call.
    app.update();
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<StartWave>>()
        .write(StartWave);
    let started = count_over_frames::<WaveStarted>(&mut app, 5);
    assert_eq!(started, 1);
    let tagged = app.world_mut().query::<&WaveTag>().iter(app.world()).count();
    assert!(tagged >= 1, "grunts on the loop from the file's wave table");
}

/// G-LP-01: placing a tower spends gold, occupies the cell, and the tower
/// fights through the base pipeline (DamageDealt observed).
#[test]
fn placing_a_tower_spends_and_fires() {
    let bundle = scenario_bundle(LeakPolicy::Stockpile { cap: 10 }, 1, None);
    let mut app = armed(&bundle);

    let grid = app.world().resource::<BuildGrid>().clone();
    let spot = (0..grid.cols)
        .flat_map(|x| (0..grid.rows).map(move |y| Cell(x, y)))
        .find(|c| grid.check(*c).is_ok())
        .expect("free cell");
    let tower = try_place(&mut app.world_mut(), "tower", spot).expect("place ok");
    assert_eq!(app.world().resource::<Resources>().gold, 150, "200 - 50");
    assert_eq!(app.world().resource::<Population>().used, 1);
    assert!(app.world().get::<loop_td::systems::gameplay::Buildable>(tower).is_some());

    // Fast-forward to wave + combat: grunts spawn at (-6,-4), speed 1.
    let dealt = count_over_frames::<DamageDealt>(&mut app, 400); // ~6.6 s
    assert!(dealt >= 2, "tower fought through the base pipeline ({dealt} hits)");
}

/// G-LP-02/03: leaked units stay on the loop, laps accumulate, and hitting
/// the stockpile cap loses the match with a readable reason.
#[test]
fn stockpile_pressure_loses_with_reason() {
    let mut bundle = scenario_bundle(LeakPolicy::Stockpile { cap: 2 }, 9, None);
    // Two grunts, no towers: they circle forever.
    bundle.units.get_mut("tower").unwrap().attack = None;
    let mut app = armed(&bundle);
    for _ in 0..400 {
        app.update(); // wave spawns, nobody dies, stock hits cap 2
    }
    assert_eq!(
        *app.world().resource::<MatchFlow>(),
        MatchFlow::Lost("stockpile full (2/2)".into())
    );
}

/// G-LP-04: killing everything on the final wave wins.
#[test]
fn clearing_all_waves_wins() {
    let bundle = scenario_bundle(LeakPolicy::Stockpile { cap: 50 }, 1, None);
    let mut app = armed(&bundle);
    let grid = app.world().resource::<BuildGrid>().clone();
    let spot = (0..grid.cols)
        .flat_map(|x| (0..grid.rows).map(move |y| Cell(x, y)))
        .find(|c| grid.check(*c).is_ok())
        .unwrap();
    try_place(&mut app.world_mut(), "tower", spot).unwrap();
    let won = count_over_frames::<MatchEnded>(&mut app, 900); // ~15 s
    assert!(won >= 1, "match ended");
    assert_eq!(*app.world().resource::<MatchFlow>(), MatchFlow::Won("survived all waves".into()));
}

/// G-LP-05: an unkilled boss wave times out and loses.
#[test]
fn boss_timeout_loses() {
    let mut bundle = scenario_bundle(LeakPolicy::Stockpile { cap: 99 }, 9, Some(0.5));
    bundle.units.get_mut("tower").unwrap().attack = None;
    let mut app = armed(&bundle);
    let timeouts = count_over_frames::<BossTimedOut>(&mut app, 120);
    assert_eq!(timeouts, 1);
    assert_eq!(*app.world().resource::<MatchFlow>(), MatchFlow::Lost("boss timeout".into()));
}

/// G-LP-11: the shipped attack x armor matrix rides the pipeline
/// (Magic vs Heavy = 0.5 in maps/loop.ron).
#[test]
fn shipped_armor_matrix_applies() {
    let loop_map = load_bundle(std::path::Path::new(LOOP_MAP)).unwrap();
    let m = loop_map.attack_armor.multiplier(AttackType::Magic, ArmorType::Heavy);
    assert!((m - 0.5).abs() < 1e-6, "matrix value {m}");

    let mut bundle = scenario_bundle(LeakPolicy::Stockpile { cap: 50 }, 9, None);
    // tower Magic 20 dmg vs grunt Heavy => 10 per hit.
    let mut app = armed(&bundle);
    app.insert_resource(loop_map.attack_armor.clone());
    let grid = app.world().resource::<BuildGrid>().clone();
    let spot = (0..grid.cols)
        .flat_map(|x| (0..grid.rows).map(move |y| Cell(x, y)))
        .find(|c| grid.check(*c).is_ok())
        .unwrap();
    try_place(&mut app.world_mut(), "tower", spot).unwrap();

    let mut cursor = bevy::ecs::message::MessageCursor::<DamageDealt>::default();
    let mut first_amount = None;
    for _ in 0..300 {
        app.update();
        let msgs = app.world().resource::<bevy::ecs::message::Messages<DamageDealt>>();
        for d in cursor.read(msgs) {
            if first_amount.is_none() {
                first_amount = Some(d.amount);
            }
        }
        if first_amount.is_some() {
            break;
        }
    }
    assert_eq!(first_amount, Some(10.0), "20 dmg x 0.5 Magic-vs-Heavy");
}

/// G-LP-07: selling refunds the bundle pct, frees cell + population.
#[test]
fn selling_refunds_and_frees() {
    let bundle = scenario_bundle(LeakPolicy::Stockpile { cap: 50 }, 9, None);
    let mut app = armed(&bundle);
    let grid = app.world().resource::<BuildGrid>().clone();
    let spot = (0..grid.cols)
        .flat_map(|x| (0..grid.rows).map(move |y| Cell(x, y)))
        .find(|c| grid.check(*c).is_ok())
        .unwrap();
    try_place(&mut app.world_mut(), "tower", spot).unwrap();
    let refund = try_sell(&mut app.world_mut(), spot).expect("sell ok");
    assert_eq!(refund, 35, "70% of 50");
    assert_eq!(app.world().resource::<Resources>().gold, 200 - 50 + 35);
    assert_eq!(app.world().resource::<Population>().used, 0);
    assert!(app.world().resource::<BuildGrid>().check(spot).is_ok(), "cell freed");
}

/// SMK-01: the maze smoke map — same base, different bundle = different game
/// (open path + Lives + Manual). Towers clear all 5 waves.
#[test]
fn maze_smoke_map_swaps_the_game() {
    let bundle = load_bundle(std::path::Path::new(MAZE_MAP)).expect("maze-smoke loads");
    validate(&bundle).unwrap();
    assert!(matches!(bundle.rules.leak, LeakPolicy::Lives { .. }));
    assert!(matches!(bundle.waves.advance, Advance::Manual));

    let mut app = armed(&bundle);
    let grid = app.world().resource::<BuildGrid>().clone();
    let spot = (0..grid.cols)
        .flat_map(|x| (0..grid.rows).map(move |y| Cell(x, y)))
        .find(|c| grid.check(*c).is_ok())
        .unwrap();
    try_place(&mut app.world_mut(), "guard", spot).unwrap();

    // Manual: start each wave as the previous clears.
    let mut ended = 0;
    for _ in 0..5 {
        app.world_mut()
            .resource_mut::<bevy::ecs::message::Messages<StartWave>>()
            .write(StartWave);
        for _ in 0..400 {
            app.update();
            if matches!(app.world().resource::<MatchFlow>(), MatchFlow::Won(_)) {
                ended += 1;
                break;
            }
        }
    }
    assert_eq!(
        *app.world().resource::<MatchFlow>(),
        MatchFlow::Won("survived all waves".into()),
        "maze bundle cleared through the same base"
    );
    assert_eq!(ended, 1);
}

/// Nearest free cell to `around` (ring order) — tests place towers right
/// next to the action instead of the grid's first-free corner.
fn free_cell_near(grid: &BuildGrid, around: Cell) -> Option<Cell> {
    for ring in 1i32..=8 {
        let found = (-ring..=ring)
            .flat_map(|dx| (-ring..=ring).map(move |dy| (dx, dy)))
            .filter(|(dx, dy)| i32::abs(*dx) == ring || i32::abs(*dy) == ring)
            .map(|(dx, dy)| Cell(around.0 + dx, around.1 + dy))
            .find(|c| grid.check(*c).is_ok());
        if found.is_some() {
            return found;
        }
    }
    None
}

/// G-TO4: the shipped function towers ride the aura layer — ice slows
/// creeps inside its radius, and the slow disappears when the tower is sold.
#[test]
fn function_tower_auras_work_through_the_bundle() {
    let bundle = load_bundle(std::path::Path::new(LOOP_MAP)).unwrap();
    let mut app = armed(&bundle);

    app.update();
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<StartWave>>()
        .write(StartWave);
    for _ in 0..10 {
        app.update();
    }
    let creep = app
        .world_mut()
        .query_filtered::<Entity, With<PathFollower>>()
        .iter(app.world())
        .next()
        .expect("creep on the loop");

    let creep_pos = app.world().get::<Transform>(creep).unwrap().translation;
    let grid = app.world().resource::<BuildGrid>().clone();
    let cell = grid.world_to_cell(Vec2::new(creep_pos.x, creep_pos.z));
    let spot = free_cell_near(&grid, cell).expect("free cell near the creep");
    try_place(&mut app.world_mut(), "ice", spot).expect("place ice");

    for _ in 0..3 {
        app.update();
    }
    let slowed = app.world().get::<EffectiveStats>(creep).unwrap().move_mult;
    assert!(slowed < 1.0, "creep slowed by ice aura ({slowed})");

    try_sell(&mut app.world_mut(), spot).unwrap();
    for _ in 0..3 {
        app.update();
    }
    let restored = app.world().get::<EffectiveStats>(creep).unwrap().move_mult;
    assert!((restored - 1.0).abs() < 1e-5, "slow gone after sell ({restored})");
}

/// HR-lite: the hero tower eats kill xp (grunt grant_xp=3 from the shipped
/// map) and its damage growth folds into EffectiveStats. Deaths are fed
/// deterministically (the fight path is covered by the placement test).
#[test]
fn hero_tower_levels_through_kills() {
    let bundle = load_bundle(std::path::Path::new(LOOP_MAP)).unwrap();
    let mut app = armed(&bundle);
    let grid = app.world().resource::<BuildGrid>().clone();
    // Park the hunter next to the spawn corner so it eats every wave's xp.
    let spawn_cell = grid.world_to_cell(Vec2::new(-10.0, -6.0));
    let spot = free_cell_near(&grid, spawn_cell).expect("free cell near spawn");
    app.world_mut().resource_mut::<Resources>().gold += 200; // hero costs 150, map starts at 120
    let hero = try_place(&mut app.world_mut(), "hunter", spot).expect("place hunter");
    assert!(app.world().get::<XpHolder>(hero).is_some(), "hero spawns with XpHolder");

    // 4 grunt kills (3 xp each = 12 >= 10 needed for level 2).
    for _ in 0..4 {
        app.world_mut()
            .resource_mut::<bevy::ecs::message::Messages<UnitDied>>()
            .write(UnitDied { entity: Entity::PLACEHOLDER, def: "grunt".into(), gold: 0, wood: 0, xp: 3, killer: Some(hero) });
        app.update();
    }
    let xp = app.world().get::<XpHolder>(hero).unwrap();
    assert_eq!(xp.level, 2, "12 xp >= 10 needed");
    assert_eq!(xp.points, 1);
    app.update(); // growth entry lands in Cleanup; aggregation folds next Tick
    let eff = app.world().get::<EffectiveStats>(hero).unwrap();
    assert!((eff.damage_mult - 1.05).abs() < 1e-5, "growth folded: {}", eff.damage_mult);
}
