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
