//! FOG1 acceptance: vision grid geometry from the bundle, exact circular
//! boundaries, explored persistence, source union, and the fingerprint skip.
//!
//! Bundle geometry trick (documented, not shared helpers): the sample map's
//! path bbox center pushes the FreeGrid origin to (-0.5, -0.5), so every
//! cell (i, j) CENTER sits on the integer world point (i, j). That makes the
//! pinned boundary values exact: source at the origin, r = 5.0 -> cell (3,4)
//! center is at distance sqrt(9+16) = 5.0 (inclusive) and cell (4,4) at
//! sqrt(32) ~ 5.657 (excluded).

use bevy::prelude::*;
use warbase::bundle::{
    assemble, BuildDomainDef, MapBundle, MapIdentity, WorldDef, MAP_FORMAT_VERSION,
};
use warbase::prelude::*;
use warbase::testing::test_app;

/// Minimal valid bundle (bx9a pattern, rebuilt in-file — no cross-file
/// private helpers). Paths square 0..15 so the 16x16/1.0 FreeGrid centers on
/// (-0.5, -0.5) => integer cell centers (see module doc).
fn sample_bundle() -> MapBundle {
    let mut units = std::collections::HashMap::new();
    units.insert(
        "grunt".to_string(),
        UnitDef {
            hero_stats: None,
            pop_cost: 1,
            leak: 1,
            id: "grunt".into(),
            attrs: BaseAttributes { hp: 50.0, move_speed: 1.0, body_radius: 0.3 },
            attack: None,
            armor_type: ArmorType::Light,
            cost_gold: 0,
            cost_wood: 0,
            bounty_gold: 3,
            bounty_wood: 0,
            grant_xp: 1,
            traits: vec![],
            auras: vec![],
            hero: false,
            skills: vec![],
            upgrade_to: None,
            heal: None,
            mana: None,
        skill_levels: vec![],
        },
    );
    MapBundle {
        identity: MapIdentity { id: "fog-sample".into(), version: MAP_FORMAT_VERSION },
        world: WorldDef {
            paths: vec![PathDef::closed_loop(vec![
                [0.0, 0.0],
                [15.0, 0.0],
                [15.0, 15.0],
                [0.0, 15.0],
            ])],
            build: BuildDomainDef::FreeGrid { cell: 1.0, cols: 16, rows: 16 },
            blocked: vec![],
        },
        units,
        waves: WaveTable {
            waves: vec![WaveEntry {
                spawns: vec![SpawnSpec {
                    unit: "grunt".into(),
                    count: 1,
                    interval: 1.0,
                    path: 0,
                    delay: 600.0, // keep creeps out of these short tests
                    hp_scale: 1.0,
                }],
                boss_timeout: None,
                preview: "1 grunt".into(),
                reward_gold: 0,
                challenge: false,
                }],
            advance: Advance::Auto { intermission: 600.0 },
            early_call_bonus: None,
        },
        rules: MatchRules {
            leak: LeakPolicy::Stockpile { cap: 10 },
            win: WinCondition::SurviveWaves { waves: 1 },
            ..Default::default()
        },
        econ_start: EconStart { gold: 120, wood: 5, pop_cap: 8 },
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
    }
}

/// Spawn a vision source at world (x, z) on the logic plane.
fn spawn_source(app: &mut App, x: f32, z: f32, radius: f32) -> Entity {
    app.world_mut()
        .spawn((VisionSource { radius }, Transform::from_translation(Vec3::new(x, 0.0, z))))
        .id()
}

/// UT-FOG-01: apply_vision sizes the grid from the build extent re-tiled at
/// VisionDef.cell; equal cells reproduce the build layout exactly.
#[test]
fn assemble_sizes_vision_grid_from_build_extent() {
    let mut app = test_app();
    assemble(&mut app.world_mut(), &sample_bundle());
    let grid = app.world().resource::<VisionGrid>();
    assert_eq!(grid.cell, 1.0);
    assert_eq!((grid.cols, grid.rows), (16, 16));
    assert_eq!(grid.origin, Vec2::new(-0.5, -0.5));
    assert!(grid.visible.is_empty());
    assert!(grid.explored.is_empty());

    // A finer fog cell covers the same world extent: 16 build cells of 1.0
    // re-tile to 32 fog cells of 0.5.
    let mut finer = sample_bundle();
    finer.vision.cell = 0.5;
    let mut app2 = test_app();
    assemble(&mut app2.world_mut(), &finer);
    let grid = app2.world().resource::<VisionGrid>();
    assert_eq!(grid.cell, 0.5);
    assert_eq!((grid.cols, grid.rows), (32, 32));
}

/// UT-FOG-02: single source r=5.0 at the origin — the boundary cell is in,
/// one step further out is not (pinned values, see module doc).
#[test]
fn single_source_boundary_is_inclusive() {
    let mut app = test_app();
    assemble(&mut app.world_mut(), &sample_bundle());
    spawn_source(&mut app, 0.0, 0.0, 5.0);
    app.update();
    let grid = app.world().resource::<VisionGrid>();
    assert!(
        grid.visible.contains(&(3, 4)),
        "cell (3,4) center at d=5.0 <= 5.0 must be visible"
    );
    assert!(
        !grid.visible.contains(&(4, 4)),
        "cell (4,4) center at d=sqrt(32) ~ 5.657 > 5.0 must not be visible"
    );
    assert!(grid.explored.contains(&(3, 4)), "visible implies explored");
}

/// UT-FOG-03: explored is memory — moving the source away clears visible
/// for the old area but never un-explodes it.
#[test]
fn explored_persists_after_source_moves_away() {
    let mut app = test_app();
    assemble(&mut app.world_mut(), &sample_bundle());
    let src = spawn_source(&mut app, 0.0, 0.0, 5.0);
    app.update();
    {
        let grid = app.world().resource::<VisionGrid>();
        assert!(grid.visible.contains(&(3, 4)));
    }

    // Move far away (well past the old radius), then tick.
    *app.world_mut()
        .entity_mut(src)
        .get_mut::<Transform>()
        .unwrap() = Transform::from_translation(Vec3::new(12.0, 0.0, 12.0));
    app.update();

    let grid = app.world().resource::<VisionGrid>();
    assert!(
        !grid.visible.contains(&(3, 4)),
        "old area no longer visible (d = sqrt(145) ~ 12.04 > 5)"
    );
    assert!(grid.explored.contains(&(3, 4)), "explored only grows");
    assert!(grid.visible.contains(&(12, 12)), "new area is lit");
}

/// UT-FOG-04: two sources light the union of their circles, overlapping in
/// the middle.
#[test]
fn two_sources_union_their_circles() {
    let mut app = test_app();
    assemble(&mut app.world_mut(), &sample_bundle());
    spawn_source(&mut app, 0.0, 0.0, 5.0);
    spawn_source(&mut app, 10.0, 0.0, 5.0);
    app.update();
    let grid = app.world().resource::<VisionGrid>();
    assert!(grid.visible.contains(&(3, 4)), "covered by source A only");
    assert!(grid.visible.contains(&(7, 4)), "covered by source B only");
    assert!(grid.visible.contains(&(5, 0)), "covered by both (overlap)");
    assert!(
        !grid.visible.contains(&(5, 5)),
        "d = sqrt(50) ~ 7.07 > 5 from both -> dark"
    );
}

/// UT-FOG-05: the source-set fingerprint skip — an unchanged source set
/// (same count, same cells, same radii) does not recompute; crossing a cell
/// boundary does.
#[test]
fn unchanged_source_set_skips_recompute() {
    let mut app = test_app();
    assemble(&mut app.world_mut(), &sample_bundle());
    let src = spawn_source(&mut app, 0.0, 0.0, 5.0);
    app.update();
    assert_eq!(app.world().resource::<VisionStats>().recomputes, 1, "first tick computes");

    // Two more ticks, source untouched: no recompute.
    app.update();
    app.update();
    assert_eq!(
        app.world().resource::<VisionStats>().recomputes,
        1,
        "unchanged fingerprint skips the full recompute"
    );

    // A sub-cell wobble stays inside the same cell: still skipped (the
    // fingerprint is per-cell by design — cell-granular dirtiness).
    *app.world_mut()
        .entity_mut(src)
        .get_mut::<Transform>()
        .unwrap() = Transform::from_translation(Vec3::new(0.1, 0.0, 0.1));
    app.update();
    assert_eq!(
        app.world().resource::<VisionStats>().recomputes,
        1,
        "same-cell movement does not change the fingerprint"
    );

    // Crossing into another cell recomputes and relights around it.
    *app.world_mut()
        .entity_mut(src)
        .get_mut::<Transform>()
        .unwrap() = Transform::from_translation(Vec3::new(8.0, 0.0, 0.0));
    app.update();
    assert_eq!(app.world().resource::<VisionStats>().recomputes, 2, "cell change recomputes");
    let grid = app.world().resource::<VisionGrid>();
    assert!(grid.visible.contains(&(8, 0)));
    assert!(!grid.visible.contains(&(3, 4)), "moved-away area is dark again");
}

/// UT-FOG-06 (zero drift for warden-style maps): a FixedSlots map has no
/// build extent — the grid exists, is empty, and ticking it is a no-op.
#[test]
fn fixed_slots_map_yields_empty_but_present_grid() {
    let mut bundle = sample_bundle();
    bundle.world.build =
        BuildDomainDef::FixedSlots { slots: vec![[0.0, 0.0], [4.0, 0.0], [8.0, 0.0]] };
    let mut app = test_app();
    assemble(&mut app.world_mut(), &bundle);
    spawn_source(&mut app, 4.0, 0.0, 5.0);
    for _ in 0..3 {
        app.update();
    }
    let grid = app.world().resource::<VisionGrid>();
    assert_eq!((grid.cols, grid.rows), (0, 0));
    assert!(grid.visible.is_empty());
    assert!(grid.explored.is_empty());
}
