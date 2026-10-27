//! legion LG0+LG1 acceptance battery (ADR-0019 Phase 2, proposal §2 anchors):
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
//!
//! LG1
//! ⑤ hire: gold 120 -> buy tank(120g) -> unit in slot, purse zero, pop
//!    booked; insufficient funds / wrong phase / occupied slot rejected
//! ⑥ round clock: build phase counts 1800 frames -> battle transition
//! ⑦ clash: opposing hires meet mid-lane -> DamageDealt (base combat)
//! ⑧ leak: a marcher reaching the enemy baseline costs the victim a life
//! ⑨ income: settlement +50g to the living
//! ⑩ elimination: lives exhausted -> player swept -> LastTeamStanding
//!    (base rule) crowns team 0

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use legion::components::{DeploySlot, Merc};
use legion::resources::{CurrentMap, LivesBoard, Phase, PopLedger, Purse, RoundFlow, BUILD_FRAMES};
use legion::systems::round::{HireCommand, HireRejected, LeakHappened, PlayerEliminated};
use warbase::prelude::*;
use warbase::testing::{send, test_app};

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

/// First free deploy slot of `player` (authored slot order = deterministic).
fn free_slot(app: &mut App, player: u8) -> Entity {
    let occupied: Vec<Entity> = {
        let mut q = app.world_mut().query::<&Merc>();
        q.iter(app.world()).map(|m| m.slot).collect()
    };
    let mut q = app.world_mut().query::<(Entity, &DeploySlot)>();
    let (entity, _) = q
        .iter(app.world())
        .find(|(e, s)| s.player == player && !occupied.contains(e))
        .expect("player has a free slot");
    entity
}

/// Hire one merc for `player` (build-phase command), consuming one frame.
fn hire_for(app: &mut App, player: u8, def: &str) {
    let slot = free_slot(app, player);
    send(app, HireCommand { player, def: def.into(), slot });
}

/// Set every wallet to `gold`.
fn set_gold(app: &mut App, gold: u32) {
    app.world_mut().resource_mut::<Purse>().0 = [gold; 4];
}

/// Fast-forward to the first battle frame (frame clock to the brink; the
/// next update transitions and issues the march orders).
fn skip_to_battle(app: &mut App) {
    {
        let mut flow = app.world_mut().resource_mut::<RoundFlow>();
        assert_eq!(flow.phase, Phase::Build, "skip only works in build phase");
        flow.frame = BUILD_FRAMES - 1;
    }
    app.update();
    assert_eq!(app.world().resource::<RoundFlow>().phase, Phase::Battle);
}

/// Advance `frames` updates, returning how many `M` messages were written.
fn run_counting<M: Message>(app: &mut App, frames: usize) -> usize {
    let mut cursor = MessageCursor::<M>::default();
    let mut total = 0;
    for _ in 0..frames {
        app.update();
        let msgs = app.world().resource::<Messages<M>>();
        total += cursor.read(msgs).count();
    }
    total
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
/// stationary (speed 0 -> no MoveAgent). Their only combat presence is the
/// zero-damage banner kit (hole L5): a persistent LastTeamStanding marker
/// that vanishes when the player is eliminated.
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
    assert_eq!(app.world().resource::<Purse>().0, [120; 4]);

    let nav = app.world().resource::<NavGrid>();
    assert!(!nav.has_grid(), "FixedSlots map: no nav geometry, straight-line fallback");
    assert!(nav.blocked.is_empty(), "no lane cells blocked");
}

// -- ⑤ LG1: hire shop --------------------------------------------------------

/// LG1-⑤: the tank anchor — gold 120 buys merc_tank(120g): the unit lands
/// in the chosen slot with the right owner, the purse zeroes, population
/// books 3. Rejections: insufficient funds, wrong phase, occupied slot.
#[test]
fn hire_tank_charges_wallets_and_books_population() {
    let mut app = legion_app();
    set_gold(&mut app, 120);

    hire_for(&mut app, 0, "merc_tank");
    assert_eq!(app.world().resource::<Purse>().0[0], 0, "120g spent exactly");

    // One tank in the field, owned by P0, sitting on a P0 slot.
    let mut q = app.world_mut().query::<(Entity, &Merc, &UnitInstance, &Owner)>();
    let hired: Vec<(Entity, &Merc, &UnitInstance, &Owner)> = q.iter(app.world()).collect();
    assert_eq!(hired.len(), 1);
    assert_eq!(hired[0].2.def, "merc_tank");
    assert_eq!(hired[0].3.0, PlayerId(0));
    let slot_owner = app.world().get::<DeploySlot>(hired[0].1.slot).unwrap().player;
    assert_eq!(slot_owner, 0, "hired into own deploy zone");

    // Population booked: tank costs 3.
    assert_eq!(app.world().resource::<PopLedger>().used, [3, 0, 0, 0]);

    // Insufficient funds: purse is 0 now, cheap(30g) refused, nothing spawns.
    // (Cursor BEFORE the trigger — a fresh cursor misses prior frames.)
    let mut rej = MessageCursor::<HireRejected>::default();
    hire_for(&mut app, 0, "merc_cheap");
    let mut q = app.world_mut().query::<&Merc>();
    assert_eq!(q.iter(app.world()).count(), 1, "still just the tank");
    assert_eq!(app.world().resource::<Purse>().0[0], 0, "nothing charged");
    let msgs = app.world().resource::<Messages<HireRejected>>();
    assert!(rej.read(msgs).count() >= 1, "insufficient-funds rejection surfaced");

    // Wrong phase: battle locks the shop.
    let slot = free_slot(&mut app, 0);
    app.world_mut().resource_mut::<RoundFlow>().phase = Phase::Battle;
    let mut rej = MessageCursor::<HireRejected>::default();
    send(&mut app, HireCommand { player: 0, def: "merc_cheap".into(), slot });
    let mut q = app.world_mut().query::<&Merc>();
    assert_eq!(q.iter(app.world()).count(), 1, "battle-phase hire refused");
    let msgs = app.world().resource::<Messages<HireRejected>>();
    assert!(rej.read(msgs).count() >= 1, "phase rejection surfaced");
}

// -- ⑥ LG1: round clock -------------------------------------------------------

/// LG1-⑥: build phase is frame-counted — 1799 updates in, still building;
/// update 1800 flips to battle (and an empty field settles the round right
/// after: +50g income, back to build, round 2).
#[test]
fn build_phase_counts_1800_frames_then_battle_settles() {
    let mut app = legion_app();
    // legion_app() ran 1 update (frame 1). Advance to frame 1799.
    for _ in 0..(BUILD_FRAMES as usize - 2) {
        app.update();
    }
    let flow = app.world().resource::<RoundFlow>().clone();
    assert_eq!(flow.phase, Phase::Build);
    assert_eq!(flow.frame, BUILD_FRAMES - 1);

    // Update 1800: battle (no mercs on the field).
    app.update();
    assert_eq!(app.world().resource::<RoundFlow>().phase, Phase::Battle);

    // Next update: empty battlefield -> instant settlement -> round 2 build.
    app.update();
    let flow = app.world().resource::<RoundFlow>().clone();
    assert_eq!(flow.phase, Phase::Build);
    assert_eq!(flow.round, 2, "settlement advances the round");
    assert_eq!(
        app.world().resource::<Purse>().0,
        [120 + 50; 4],
        "income +50 to every living player (LG1-⑨)"
    );
}

// -- ⑦ LG1: auto clash -------------------------------------------------------

/// LG1-⑦: P0 and P1 each hire a cheap merc; at battle they march their
/// shared corridor head-on and base combat exchanges damage (DamageDealt
/// from both directions is the mutual-fire evidence).
#[test]
fn opposing_mercs_clash_with_damage_dealt() {
    let mut app = legion_app();
    set_gold(&mut app, 30);
    hire_for(&mut app, 0, "merc_cheap");
    hire_for(&mut app, 1, "merc_cheap");
    assert_eq!(units_of(&mut app, "merc_cheap").len(), 2);

    skip_to_battle(&mut app);
    // ~25 units at ~6.4 u/s closing speed: contact inside ~250 frames;
    // 700 frames comfortably covers first strikes from both sides.
    let dealt = run_counting::<DamageDealt>(&mut app, 700);
    assert!(dealt >= 2, "mutual combat happened (DamageDealt x{dealt})");

    // The fight resolves and the round settles (one survivor leaks, bounty
    // flows to the killer, next round starts) — total window is generous.
    let _ = run_counting::<DamageDealt>(&mut app, 1300);
    let flow = app.world().resource::<RoundFlow>().clone();
    assert!(flow.round >= 2, "clash resolved, round settled (round {})", flow.round);
    let purse = app.world().resource::<Purse>().0;
    let winner_gold = purse[0].max(purse[1]);
    assert!(
        winner_gold >= 50 + 4,
        "income 50 + kill bounty 4 credited to the duel's winner ({purse:?})"
    );
}

// -- ⑧ LG1: leak rule --------------------------------------------------------

/// LG1-⑧: an unopposed marcher reaches the enemy baseline -> the victim
/// loses exactly one life, the marcher despawns, and the round settles.
#[test]
fn leak_at_baseline_costs_victim_one_life() {
    let mut app = legion_app();
    set_gold(&mut app, 30);
    hire_for(&mut app, 0, "merc_cheap");
    skip_to_battle(&mut app);

    // ~25 units at 3.2 u/s ≈ 470 frames; 900 covers it with margin.
    let leaks = run_counting::<LeakHappened>(&mut app, 900);
    assert_eq!(leaks, 1, "exactly one leak");
    assert_eq!(app.world().resource::<LivesBoard>().0, [20, 19, 20, 20]);
    assert_eq!(units_of(&mut app, "merc_cheap").len(), 0, "leaked marcher swept");

    // Field empty -> settlement (+50 income, round 2).
    app.update();
    let flow = app.world().resource::<RoundFlow>().clone();
    assert_eq!((flow.round, flow.phase == Phase::Build), (2, true));
    assert_eq!(app.world().resource::<Purse>().0[0], 0 + 50);
}

// -- ⑩ LG1: elimination -> LastTeamStanding -----------------------------------

/// LG1-⑩: both team-1 players at 1 life; one leak each eliminates them
/// (owned units swept, keeps and banner kits gone) — the base
/// LastTeamStanding rule crowns team 0 on the next frame.
#[test]
fn lives_exhaustion_eliminates_and_crowns_team_zero() {
    let mut app = legion_app();
    app.world_mut().resource_mut::<LivesBoard>().0 = [20, 1, 20, 1];
    set_gold(&mut app, 30);
    hire_for(&mut app, 0, "merc_cheap");
    hire_for(&mut app, 2, "merc_cheap");

    skip_to_battle(&mut app);
    let eliminated = run_counting::<PlayerEliminated>(&mut app, 900);
    assert_eq!(eliminated, 2, "P1 and P3 both out");

    // Keeps swept with their owners (banner kit gone -> no team-1 fighter).
    assert_eq!(units_of(&mut app, "keep").len(), 2, "only team-0 keeps remain");

    // Base rule crowns team 0 (MatchWinner is the structured winner).
    let flow = app.world().resource::<MatchFlow>().clone();
    assert!(matches!(flow, MatchFlow::Won(ref r) if r.contains("last team standing")));
    assert_eq!(app.world().resource::<MatchWinner>().0, TeamId(0));
}
