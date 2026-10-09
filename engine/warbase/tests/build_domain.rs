//! Build domain acceptance: the five rejection classes, occupancy +
//! freeing, ghost/commit same-source validation, dual modes.

use bevy::prelude::*;
use warbase::build::{BuildGrid, BuildMode, BuildReject, Cell};

fn grid() -> BuildGrid {
    let mut g = BuildGrid::free_grid(1.0, 4, 4);
    g.blocked.insert(Cell(2, 2));
    g
}

/// UT-BLD-01: each rejection class fires for its own cause.
#[test]
fn placement_rejections_are_distinct() {
    let mut app = App::new();
    let mut g = grid();
    let e = app.world_mut().spawn_empty().id();

    assert_eq!(g.check(Cell(-1, 0)), Err(BuildReject::OutOfBounds));
    assert_eq!(g.check(Cell(4, 0)), Err(BuildReject::OutOfBounds));
    assert_eq!(g.check(Cell(2, 2)), Err(BuildReject::Blocked));

    g.place(Cell(0, 0), e);
    assert_eq!(g.check(Cell(0, 0)), Err(BuildReject::Occupied));
    assert!(g.check(Cell(1, 1)).is_ok());
}

/// UT-BLD-02: placing occupies, freeing releases (sell path pairs with this).
#[test]
fn place_then_free_round_trips() {
    let mut app = App::new();
    let mut g = grid();
    let e = app.world_mut().spawn_empty().id();
    g.place(Cell(1, 1), e);
    assert_eq!(g.used(), 1);
    assert_eq!(g.free(Cell(1, 1)), Some(e));
    assert_eq!(g.used(), 0);
    assert!(g.check(Cell(1, 1)).is_ok());
}

/// UT-BLD-03: ghost preview and commit share one verdict function by
/// construction — same input, same result, twice in a row.
#[test]
fn ghost_and_commit_share_validation() {
    let mut app = App::new();
    let mut g = grid();
    let e = app.world_mut().spawn_empty().id();
    let cell = g.world_to_cell(Vec2::new(1.4, 0.2)); // -> Cell(1, 0)
    assert_eq!(cell, Cell(1, 0));
    assert!(g.check(cell).is_ok()); // ghost says yes
    assert!(g.check(cell).is_ok()); // commit asks the same thing
    g.place(cell, e);
    assert_eq!(g.check(cell), Err(BuildReject::Occupied)); // ghost now says no
}

/// UT-BLD-04: FixedSlots mode — slots hand out in order, then reject.
#[test]
fn fixed_slots_hand_out_in_order_then_reject() {
    let mut app = App::new();
    let mut g = BuildGrid::fixed_slots(vec![Vec2::new(1.0, 0.0), Vec2::new(2.0, 0.0)]);
    assert_eq!(g.mode, BuildMode::FixedSlots);

    let (c0, p0) = g.first_free_slot().unwrap();
    assert_eq!(p0, Vec2::new(1.0, 0.0));
    g.place(c0, app.world_mut().spawn_empty().id());

    let (c1, p1) = g.first_free_slot().unwrap();
    assert_eq!(p1, Vec2::new(2.0, 0.0));
    g.place(c1, app.world_mut().spawn_empty().id());

    assert_eq!(g.first_free_slot(), Err(BuildReject::NoFreeSlot));
    // Freeing the first slot makes it available again.
    g.free(c0);
    assert!(g.first_free_slot().is_ok());
}

/// TO3: upgrade swaps the tier in place — pays the target def's cost,
/// keeps cell + population, changes the def; max tier rejects.
#[test]
fn upgrade_swaps_tier_in_place() {
    use warbase::prelude::*;
    let mut app = warbase::testing::test_app();
    let mut defs = std::collections::HashMap::new();
    let tower_def = |id: &str, cost: u32, up: Option<String>| UnitDef {
        id: id.into(),
        attrs: BaseAttributes { hp: 80.0, move_speed: 0.0, body_radius: 0.4 },
        attack: Some(AttackDef {
            damage: 12.0, interval: 0.8, range: 5.0,
            attack_type: AttackType::Piercing, targets: TargetDomain::Both,
            projectile: Some(ProjDef { speed: 20.0 }),
        }),
        armor_type: ArmorType::Fortified,
        cost_gold: cost, cost_wood: 0,
        bounty_gold: 0, bounty_wood: 0, grant_xp: 0,
        traits: vec![], auras: vec![], hero: false, skills: vec![],
        upgrade_to: up,
    };
    defs.insert("arrow".into(), tower_def("arrow", 50, Some("arrow2".into())));
    defs.insert("arrow2".into(), tower_def("arrow2", 90, None));
    let registry = UnitRegistry { defs };
    app.insert_resource(UnitRegistry { defs: registry.defs.clone() });
    app.insert_resource(BuildGrid::free_grid(1.0, 8, 8));
    app.world_mut().resource_mut::<warbase::prelude::Resources>().gold = 200;
    app.world_mut().resource_mut::<warbase::prelude::Population>().cap = 8;

    let cell = Cell(2, 2);
    let t1 = try_place(&mut app.world_mut(), "arrow", cell).unwrap();
    assert_eq!(app.world().resource::<warbase::prelude::Resources>().gold, 150);
    assert_eq!(app.world().resource::<warbase::prelude::Population>().used, 1);

    let t2 = try_upgrade(&mut app.world_mut(), cell).unwrap();
    assert_ne!(t1, t2, "new entity replaces the old");
    assert_eq!(app.world().get::<UnitInstance>(t2).unwrap().def, "arrow2");
    assert_eq!(app.world().resource::<warbase::prelude::Resources>().gold, 150 - 90);
    assert_eq!(app.world().resource::<warbase::prelude::Population>().used, 1, "1:1 swap");
    assert_eq!(app.world().resource::<BuildGrid>().occupant(cell), Some(t2));

    // Max tier rejects with a readable reason.
    let err = try_upgrade(&mut app.world_mut(), cell).unwrap_err();
    assert_eq!(err, "max tier");
    assert_eq!(app.world().resource::<warbase::prelude::Resources>().gold, 60, "no spend on reject");
}
