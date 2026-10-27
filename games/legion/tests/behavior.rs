//! legion LG0 acceptance battery (ADR-0019 Phase 2, proposal §2 anchors):
//! headless apps drive the thin game layer over warbase mechanisms.
//!
//! LG0
//! ① map assembles: 4 open lanes (each end = own deploy zone / opponent
//!    baseline), 2v2 diplomacy (P0P2 team0 vs P1P3 team1), 48 deploy slots
//! ② mercenary def table: 6 defs, 30-120 gold, bounties, kits (ranged
//!    projectile / support healer / tank anchor 120g)
//! ③ opening: 4 keeps owned by P0-P3, stationary scenery (no MoveAgent)
//! ④ wallets seeded from econ_start; lanes stay walkable (FixedSlots keeps
//!    the nav grid geometry-free)

use bevy::prelude::*;
use legion::resources::CurrentMap;
use warbase::prelude::*;
use warbase::testing::test_app;

// -- helpers ---------------------------------------------------------------

/// Headless legion: base + game core, one update (Startup assembles the map
/// and the keeps; the observer attaches kit).
fn legion_app() -> App {
    let mut app = test_app();
    app.add_plugins(legion::plugins::game::GamePlugin);
    app.update();
    app
}

/// All live instances of a def id.
fn units_of(app: &mut App, def: &str) -> Vec<Entity> {
    let mut q = app.world_mut().query::<(Entity, &UnitInstance)>();
    q.iter(app.world()).filter(|(_, i)| i.def == def).map(|(e, _)| e).collect()
}

// -- ① assembly: lanes / diplomacy / slots ----------------------------------

/// LG0-①: the shipped arena.ron assembles — 4 open lanes with the authored
/// endpoints, the 2v2 diplomacy, and the 12-per-lane deploy grid.
#[test]
fn arena_map_assembles_four_lanes_two_teams() {
    let app = legion_app();

    let bundle = app.world().resource::<CurrentMap>().0.clone();
    assert_eq!(bundle.identity.id, "arena");
    // 4 open lanes: start = own deploy end, end = opponent baseline.
    assert_eq!(bundle.world.paths.len(), 4);
    for path in &bundle.world.paths {
        assert!(!path.closed, "legion lanes are open chains");
        assert_eq!(path.points.len(), 2, "straight lanes");
    }
    assert_eq!(bundle.world.paths[0].points[0], [-13.0, -4.0]); // P0 deploy
    assert_eq!(bundle.world.paths[0].points[1], [13.0, -4.0]); // P1 baseline
    assert_eq!(bundle.world.paths[1].points[1], [-13.0, -4.0]); // P0 baseline
    assert_eq!(bundle.world.paths[3].points[1], [-13.0, 4.0]); // P2 baseline
    // Pairs share a corridor so the two march columns clash head-on.
    assert_eq!(bundle.world.paths[0].points[1], bundle.world.paths[1].points[0]);

    // 2v2 diplomacy: P0P2 team 0 vs P1P3 team 1, single hostile pair.
    assert_eq!(
        app.world().resource::<MatchDiplomacy>().players,
        vec![
            (PlayerId(0), TeamId(0)),
            (PlayerId(1), TeamId(1)),
            (PlayerId(2), TeamId(0)),
            (PlayerId(3), TeamId(1)),
        ]
    );
    let hostility = app.world().resource::<Hostility>();
    assert!(hostility.teams_hostile(TeamId(0), TeamId(1)));
    assert!(!hostility.teams_hostile(TeamId(0), TeamId(0)));
    // Derived side view: team 1 players fold to Attacker, team 0 to Defender.
    assert!(hostility.hostile(Allegiance::Defender, Allegiance::Attacker));

    // Deploy grid: 48 FixedSlots (12 per lane).
    match &bundle.world.build {
        warbase::bundle::BuildDomainDef::FixedSlots { slots } => {
            assert_eq!(slots.len(), 48, "4 lanes x 12 deploy cells");
        }
        other => panic!("expected FixedSlots, got {other:?}"),
    }
    let grid = app.world().resource::<warbase::build::BuildGrid>();
    assert_eq!(grid.slots.len(), 48, "build grid carries the slot centers");
}

// -- ② mercenary def table ---------------------------------------------------

/// LG0-②: six mercenary defs ship the proposal §2 anchors — 30-120 gold,
/// bounties for the payout loop, ranged projectile kit, support healer kit,
/// tank at exactly 120g (the buy-out acceptance anchor).
#[test]
fn mercenary_table_matches_proposal_anchors() {
    let app = legion_app();
    let registry = app.world().resource::<UnitRegistry>();

    let anchors = [
        ("merc_cheap", 30u32, 4u32),
        ("merc_swarm", 45, 5),
        ("merc_tank", 120, 15),
        ("merc_ranged", 70, 8),
        ("merc_aoe", 100, 12),
        ("merc_support", 60, 6),
    ];
    for (id, gold, bounty) in anchors {
        let def = registry.get(id).unwrap_or_else(|| panic!("missing def {id}"));
        assert_eq!(def.cost_gold, gold, "{id} price");
        assert_eq!(def.bounty_gold, bounty, "{id} bounty");
        assert!(
            def.cost_gold >= 30 && def.cost_gold <= 120,
            "{id} price inside the proposal's 30-120 band"
        );
        assert!(def.attack.is_some(), "{id} is a fighter (support has a chip attack)");
        assert!(def.attrs.move_speed > 0.0, "{id} marches");
    }

    // Kit spot checks: ranged carries a projectile; support heals; tank is
    // the slow heavy body; all six target ground.
    let ranged = registry.get("merc_ranged").unwrap();
    assert!(ranged.attack.as_ref().unwrap().projectile.is_some(), "ranged shoots");
    let support = registry.get("merc_support").unwrap();
    assert!(support.heal.is_some(), "support heals");
    let tank = registry.get("merc_tank").unwrap();
    assert_eq!(tank.armor_type, ArmorType::Heavy);
    assert!(tank.attrs.hp >= 300.0, "tank is the meat shield");

    // Attack-armor counters ship non-trivial: Piercing vs Light 1.3.
    let table = app.world().resource::<AttackArmorTable>();
    assert_eq!(table.multiplier(AttackType::Piercing, ArmorType::Light), 1.3);
}

// -- ③ opening layout --------------------------------------------------------

/// LG0-③: four keeps spawn behind their baselines, owned by P0-P3, and stay
/// stationary (speed 0 -> no MoveAgent): they are scenery anchors, not
/// LastTeamStanding fighters — elimination flows through the lives board.
#[test]
fn four_keeps_spawn_owned_and_stationary() {
    let mut app = legion_app();
    let keeps = units_of(&mut app, "keep");
    assert_eq!(keeps.len(), 4, "one keep per player");

    let mut owners = Vec::new();
    for &e in &keeps {
        assert!(app.world().get::<Selectable>(e).is_some(), "keeps are selectable");
        assert!(app.world().get::<MoveAgent>(e).is_none(), "keeps do not move");
        owners.push(app.world().get::<Owner>(e).unwrap().0 .0);
    }
    owners.sort_unstable();
    assert_eq!(owners, vec![0, 1, 2, 3], "each player owns exactly one keep");
}

// -- ④ wallets + walkability --------------------------------------------------

/// LG0-④: per-player wallets seed from `econ_start.gold` (120), and the
/// FixedSlots map keeps the nav grid geometry-free — lanes are pure routing
/// data, nothing blocks the march (the TD "path = wall" behavior does not
/// apply; see docs/kpi.md hole L1).
#[test]
fn purses_seed_and_lanes_stay_walkable() {
    let app = legion_app();
    assert_eq!(app.world().resource::<legion::resources::Purse>().0, [120; 4]);

    let nav = app.world().resource::<NavGrid>();
    assert!(!nav.has_grid(), "FixedSlots map: no nav geometry, straight-line fallback");
    assert!(nav.blocked.is_empty(), "no lane cells blocked");
}
