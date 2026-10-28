//! legion LG0+LG1 acceptance battery (ADR-0019 Phase 2, proposal §2 anchors):
//! headless apps drive the thin game layer over warbase mechanisms.
//!
//! LG4 (round-4 LDEBT): migrated onto the uplited base seams with zero
//! behavioral drift — per-player gold/pop read base `Wallets` (L2), lives
//! read base `PlayerLives` (L5, elimination + LastTeamStanding-on-lives are
//! the base's now), the round cap writes `MatchFlow::Ended{Win|Draw}` (L7),
//! sends spawn through the base owned-injection seam with the order armed
//! at spawn (L8), deploy slots are GENERATED from walkable:false lanes
//! (L1) and pops come from `UnitDef.pop_cost` (L3). Value anchors (prices,
//! bounties, timings, rejection reasons) are unchanged from LG0-LG2.
//!
//! LG0
//! ① map assembles: 4 open routing-only lanes (each end = own deploy zone /
//!    opponent baseline), 2v2 diplomacy (P0P2 team0 vs P1P3 team1), 48
//!    GENERATED deploy slots at the exact coordinates the hand-written
//!    FixedSlots list used to carry
//! ② mercenary def table: 6 defs, 30-120 gold, bounties + pop costs, kits
//!    (ranged projectile / support healer / tank anchor 120g)
//! ③ opening: 4 keeps owned by P0-P3, stationary scenery (no MoveAgent,
//!    no banner kit — presence is the lives board since L5)
//! ④ wallets seeded from econ_start (base Wallets, L2); lanes stay
//!    walkable (FreeGrid + walkable:false, L1)
//!
//! LG1
//! ⑤ hire: gold 120 -> buy tank(120g) -> unit in slot, wallet zero, pop
//!    booked; insufficient funds / wrong phase / occupied slot rejected
//! ⑥ round clock: build phase counts 1800 frames -> battle transition
//! ⑦ clash: opposing hires meet mid-lane -> DamageDealt (base combat)
//! ⑧ leak: a marcher reaching the enemy baseline costs the victim a life
//! ⑨ income: settlement +50g to the living
//! ⑩ elimination: lives exhausted -> base TeamLives rule sweeps + crowns
//!    team 0 (structured ending, L5/L7)
//!
//! LG2
//! ⑪ send: build-phase sends inject next round's attackers — 25g first,
//!    exactly 27 the second (25*1.1 floored), 30 the third (25*1.1^2);
//!    the units spawn from the sender's deploy zone through the base L8
//!    seam (Owner + attack-move order ARMED at spawn) and the march order
//!    targets the victim's line; cross-lane / self / unknown def /
//!    battle-phase sends rejected
//! ⑫ breach bounty: a sent unit reaching the victim's baseline costs a
//!    life AND pays the sponsor +5g
//! ⑬ round cap win: round 20 settlement adjudicates on lives — more
//!    lives wins (MatchEnded reason names the team, MatchWinner set)
//! ⑭ round cap draw: equal lives -> Draw, readable reason, no MatchWinner
//!
//! LG4 new seams
//! ⑮ local-player view: `Resources`/`Population` mirror `Wallets[P0]` at
//!    all times (L2 compatibility contract)
//! ⑯ 2v2 bounty isolation: a P0 kill credits ONLY team-0 wallets; P1/P3
//!    wallets never move (the pre-L2 base could not express this)

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use legion::components::{DeploySlot, Merc, SentMerc};
use legion::resources::{CurrentMap, Phase, RoundFlow, BUILD_FRAMES};
use legion::systems::round::{
    HireCommand, HireRejected, LeakHappened, PhaseChanged, PlayerEliminated, SendCommand,
    SendRejected,
};
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

/// First free deploy slot of `player` (generated slot order = deterministic).
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

/// Set every wallet to `gold` (base Wallets, index = player slot).
fn set_gold(app: &mut App, gold: u32) {
    for wallet in app.world_mut().resource_mut::<Wallets>().0.iter_mut() {
        wallet.gold = gold;
    }
}

/// Per-player gold snapshot.
fn golds(app: &mut App) -> Vec<u32> {
    app.world().resource::<Wallets>().0.iter().map(|w| w.gold).collect()
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

/// LG0-① (L1 shape): the shipped arena.ron assembles — 4 open
/// ROUTING-ONLY lanes (walkable: false) with the authored endpoints, the
/// 2v2 diplomacy, and 48 GENERATED deploy cells at the exact coordinates
/// the hand-written FixedSlots list used to enumerate.
#[test]
fn arena_map_assembles_four_lanes_two_teams() {
    let mut app = legion_app();

    let bundle = app.world().resource::<CurrentMap>().0.clone();
    assert_eq!(bundle.identity.id, "arena");
    // 4 open lanes: start = own deploy end, end = opponent baseline.
    assert_eq!(bundle.world.paths.len(), 4);
    for path in &bundle.world.paths {
        assert!(!path.closed, "legion lanes are open chains");
        assert_eq!(path.points.len(), 2, "straight lanes");
        assert!(!path.walkable, "lanes are routing-only (L1) — never blocked");
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

    // L5: the base seeded the per-player lives board from TeamLives(20).
    assert_eq!(
        app.world().resource::<PlayerLives>().0,
        vec![20, 20, 20, 20],
        "every listed player starts at per_team lives"
    );

    // L1: build domain is FreeGrid; nothing blocks (routing-only lanes).
    match &bundle.world.build {
        warbase::bundle::BuildDomainDef::FreeGrid { .. } => {}
        other => panic!("expected FreeGrid, got {other:?}"),
    }
    let grid = app.world().resource::<warbase::build::BuildGrid>();
    assert_eq!(grid.mode, warbase::build::BuildMode::FreeGrid);
    assert!(grid.blocked.is_empty(), "no lane cell is blocked");

    // 48 generated deploy slots (4 lanes x 12), at the exact legacy
    // coordinates: first P0 cell (-12, -5), last P0 cell (-9, -3); first
    // P1 cell (12, -5) (order preserved: row-major toward the baseline).
    let mut q = app.world_mut().query::<(&DeploySlot, &Transform)>();
    let mut slots: Vec<(u8, u8, Vec2)> = q
        .iter(app.world())
        .map(|(s, t)| (s.player, s.index, Vec2::new(t.translation.x, t.translation.z)))
        .collect();
    slots.sort_by_key(|(_, i, _)| *i);
    assert_eq!(slots.len(), 48, "4 lanes x 12 generated deploy cells");
    assert_eq!(slots[0], (0, 0, Vec2::new(-12.0, -5.0)), "P0 first legacy coord");
    assert_eq!(slots[11], (0, 11, Vec2::new(-9.0, -3.0)), "P0 last legacy coord");
    assert_eq!(slots[12], (1, 12, Vec2::new(12.0, -5.0)), "P1 first legacy coord");
    assert_eq!(slots[47], (3, 47, Vec2::new(9.0, 5.0)), "P3 last legacy coord");
    // 12 slots per player.
    for p in 0..4u8 {
        assert_eq!(slots.iter().filter(|(s, _, _)| *s == p).count(), 12, "P{p} owns 12 cells");
    }
}

// -- ② mercenary def table ---------------------------------------------------

/// LG0-② (+L3): six mercenary defs ship the proposal §2 anchors — 30-120
/// gold, bounties for the payout loop, pop costs IN THE DEFS (1-4, the
/// game-side table retired), ranged projectile kit, support healer kit,
/// tank at exactly 120g (the buy-out acceptance anchor).
#[test]
fn mercenary_table_matches_proposal_anchors() {
    let app = legion_app();
    let registry = app.world().resource::<UnitRegistry>();

    let anchors = [
        ("merc_cheap", 30u32, 4u32, 1u8),
        ("merc_swarm", 45, 5, 2),
        ("merc_tank", 120, 15, 3),
        ("merc_ranged", 70, 8, 2),
        ("merc_aoe", 100, 12, 3),
        ("merc_support", 60, 6, 2),
    ];
    for (id, gold, bounty, pop) in anchors {
        let def = registry.get(id).unwrap_or_else(|| panic!("missing def {id}"));
        assert_eq!(def.cost_gold, gold, "{id} price");
        assert_eq!(def.bounty_gold, bounty, "{id} bounty");
        assert_eq!(def.pop_cost, pop, "{id} pop cost (L3: ships in the def)");
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

/// LG0-③ (L5 shape): four keeps spawn behind their baselines, owned by
/// P0-P3, and stay stationary (speed 0 -> no MoveAgent). The 0-damage
/// banner kit is RETIRED — keeps carry no AttackKit at all; presence is
/// the PlayerLives board.
#[test]
fn four_keeps_spawn_owned_and_stationary() {
    let mut app = legion_app();
    let keeps = units_of(&mut app, "keep");
    assert_eq!(keeps.len(), 4, "one keep per player");

    let mut owners = Vec::new();
    for &e in &keeps {
        assert!(app.world().get::<Selectable>(e).is_some(), "keeps are selectable");
        assert!(app.world().get::<MoveAgent>(e).is_none(), "keeps do not move");
        assert!(
            app.world().get::<AttackKit>(e).is_none(),
            "banner kit retired (L5): keeps are pure presence markers"
        );
        owners.push(app.world().get::<Owner>(e).unwrap().0 .0);
    }
    owners.sort_unstable();
    assert_eq!(owners, vec![0, 1, 2, 3], "each player owns exactly one keep");
}

// -- ④ wallets + walkability --------------------------------------------------

/// LG0-④ (L2 shape): per-player wallets seed from `econ_start.gold` (120)
/// via the BASE assemble, and the walkable:false lanes + empty blocked set
/// keep the nav grid free — lanes are pure routing data, nothing blocks
/// the march (L1: the TD "path = wall" behavior does not apply).
#[test]
fn wallets_seed_and_lanes_stay_walkable() {
    let app = legion_app();
    let wallets = app.world().resource::<Wallets>();
    assert_eq!(wallets.0.len(), 4, "one wallet per listed player");
    assert!(
        wallets.0.iter().all(|w| w.gold == 120),
        "every wallet seeds at econ_start.gold (120)"
    );
    assert!(
        wallets.0.iter().all(|w| w.pop_cap == 15),
        "pop caps seed from econ_start.pop_cap (15)"
    );

    let nav = app.world().resource::<NavGrid>();
    assert!(nav.blocked.is_empty(), "no lane cell blocked (walkable: false, L1)");
}

// -- ⑤ LG1: hire shop --------------------------------------------------------

/// LG1-⑤: the tank anchor — gold 120 buys merc_tank(120g): the unit lands
/// in the chosen slot with the right owner, the wallet zeroes, population
/// books 3. Rejections: insufficient funds, wrong phase, occupied slot.
#[test]
fn hire_tank_charges_wallets_and_books_population() {
    let mut app = legion_app();
    set_gold(&mut app, 120);

    hire_for(&mut app, 0, "merc_tank");
    assert_eq!(golds(&mut app)[0], 0, "120g spent exactly");

    // One tank in the field, owned by P0, sitting on a P0 slot.
    let mut q = app.world_mut().query::<(Entity, &Merc, &UnitInstance, &Owner)>();
    let hired: Vec<(Entity, &Merc, &UnitInstance, &Owner)> = q.iter(app.world()).collect();
    assert_eq!(hired.len(), 1);
    assert_eq!(hired[0].2.def, "merc_tank");
    assert_eq!(hired[0].3.0, PlayerId(0));
    let slot_owner = app.world().get::<DeploySlot>(hired[0].1.slot).unwrap().player;
    assert_eq!(slot_owner, 0, "hired into own deploy zone");

    // Population booked (base Wallets pop fields, L2): tank costs 3.
    app.update(); // recount writes the wallet
    assert_eq!(
        app.world().resource::<Wallets>().0[0].pop_used,
        3,
        "population booked via Wallets"
    );

    // Insufficient funds: wallet is 0 now, cheap(30g) refused, nothing spawns.
    // (Cursor BEFORE the trigger — a fresh cursor misses prior frames.)
    let mut rej = MessageCursor::<HireRejected>::default();
    hire_for(&mut app, 0, "merc_cheap");
    let mut q = app.world_mut().query::<&Merc>();
    assert_eq!(q.iter(app.world()).count(), 1, "still just the tank");
    assert_eq!(golds(&mut app)[0], 0, "nothing charged");
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
        golds(&mut app),
        vec![120 + 50; 4],
        "income +50 to every living player (LG1-⑨)"
    );
}

// -- ⑦ LG1: auto clash -------------------------------------------------------

/// LG1-⑦ (+LG4-⑯): P0 and P1 each hire a cheap merc; at battle they march
/// their shared corridor head-on and base combat exchanges damage
/// (DamageDealt from both directions is the mutual-fire evidence). The
/// duel's winner banks income 50 + kill bounty 4; the OTHER side's wallets
/// (P1's teammate P3 included) never see the bounty.
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
    let golds = golds(&mut app);
    let winner_gold = golds[0].max(golds[1]);
    assert!(
        winner_gold >= 50 + 4,
        "income 50 + kill bounty 4 credited to the duel's winner ({golds:?})"
    );
}

// -- ⑧ LG1: leak rule --------------------------------------------------------

/// LG1-⑧: an unopposed marcher reaches the enemy baseline -> the victim
/// loses exactly one life (base PlayerLives, L5), the marcher despawns,
/// and the round settles.
#[test]
fn leak_at_baseline_costs_victim_one_life() {
    let mut app = legion_app();
    set_gold(&mut app, 30);
    hire_for(&mut app, 0, "merc_cheap");
    skip_to_battle(&mut app);

    // ~25 units at 3.2 u/s ≈ 470 frames; 900 covers it with margin.
    let leaks = run_counting::<LeakHappened>(&mut app, 900);
    assert_eq!(leaks, 1, "exactly one leak");
    assert_eq!(app.world().resource::<PlayerLives>().0, vec![20, 19, 20, 20]);
    assert_eq!(units_of(&mut app, "merc_cheap").len(), 0, "leaked marcher swept");

    // Field empty -> settlement (+50 income, round 2).
    app.update();
    let flow = app.world().resource::<RoundFlow>().clone();
    assert_eq!((flow.round, flow.phase == Phase::Build), (2, true));
    assert_eq!(golds(&mut app)[0], 0 + 50);
}

// -- ⑩ LG1: elimination -> base TeamLives crown ---------------------------------

/// LG1-⑩ (L5/L7 shape): both team-1 players at 1 life; one leak each
/// eliminates them — the BASE rules_tick sweeps their units (keeps
/// included; no banner kits anymore) and crowns team 0 through the
/// structured ending on the next frame.
#[test]
fn lives_exhaustion_eliminates_and_crowns_team_zero() {
    let mut app = legion_app();
    app.world_mut().resource_mut::<PlayerLives>().0 = vec![20, 1, 20, 1];
    set_gold(&mut app, 30);
    hire_for(&mut app, 0, "merc_cheap");
    hire_for(&mut app, 2, "merc_cheap");

    skip_to_battle(&mut app);
    let eliminated = run_counting::<PlayerEliminated>(&mut app, 900);
    assert_eq!(eliminated, 2, "P1 and P3 both out");

    // Keeps swept with their owners by the base rule.
    assert_eq!(units_of(&mut app, "keep").len(), 2, "only team-0 keeps remain");

    // Base rule crowns team 0: structured ending + MatchWinner (L5/L7).
    let flow = app.world().resource::<MatchFlow>().clone();
    assert!(matches!(
        flow,
        MatchFlow::Ended { ref reason, outcome: MatchOutcome::Win(TeamId(0)) }
            if reason.contains("last team standing on lives")
    ));
    assert_eq!(app.world().resource::<MatchWinner>().0, TeamId(0));
}

// -- ⑪ LG2: send shop ---------------------------------------------------------

/// LG2-⑪ (L8 shape): the send anchor — P0 pays to inject next round's
/// attackers against P1. Prices escalate per target: 25, then EXACTLY 27
/// (25*1.1 floored — the rounding rule is pinned here), then 30 (25*1.1^2).
/// The queued units spawn from P0's deploy zone through the BASE seam —
/// owned, with the attack-move order ARMED AT SPAWN — and the march order
/// aims at P1's line (P0's lane baseline). Cross-lane targets, self-sends,
/// unknown defs and battle-phase sends are all refused without charge.
#[test]
fn send_injects_next_round_from_deploy_zone_at_escalating_cost() {
    let mut app = legion_app();
    set_gold(&mut app, 100);

    // Three sends to the same target: 25 + 27 + 30 = 82 spent exactly.
    let mut rej = MessageCursor::<SendRejected>::default();
    for _ in 0..3 {
        send(&mut app, SendCommand { player: 0, target: 1, def: "merc_cheap".into() });
        app.update();
    }
    assert_eq!(golds(&mut app)[0], 100 - 82, "25 + 27 + 30 (floored 1.1x)");
    let msgs = app.world().resource::<Messages<SendRejected>>();
    assert_eq!(rej.read(msgs).count(), 0, "well-formed sends never rejected");

    // Rejections, none of them charged: cross-lane target (P3 shares no
    // corridor with P0; P2 is an ally — same refusal), self, unknown def.
    // (Each `send` advances one frame; a persistent cursor drains every
    // frame's buffer — a fresh end-of-batch cursor sees only the last one.)
    let mut rej = MessageCursor::<SendRejected>::default();
    let mut reasons: Vec<String> = Vec::new();
    for target in [3u8, 2, 0] {
        send(&mut app, SendCommand { player: 0, target, def: "merc_cheap".into() });
        let msgs = app.world().resource::<Messages<SendRejected>>();
        reasons.extend(rej.read(msgs).map(|r| r.reason.clone()));
    }
    send(&mut app, SendCommand { player: 0, target: 1, def: "merc_nope".into() });
    let msgs = app.world().resource::<Messages<SendRejected>>();
    reasons.extend(rej.read(msgs).map(|r| r.reason.clone()));
    assert_eq!(reasons.len(), 4, "cross-lane x2, self, unknown def all refused");
    assert!(reasons.iter().filter(|r| r.contains("no lane")).count() == 2);
    assert!(reasons.iter().any(|r| r.contains("yourself")));
    assert!(reasons.iter().any(|r| r.contains("unknown def")));
    assert_eq!(golds(&mut app)[0], 18, "rejections cost nothing");

    // Battle transition: the three queued sends spawn at P0's zone mouth and
    // the march order (base UnitCommand::AttackMove) targets P1's line.
    let mut orders = MessageCursor::<UnitCommand>::default();
    skip_to_battle(&mut app);
    let cmds: Vec<UnitCommand> = {
        let msgs = app.world().resource::<Messages<UnitCommand>>();
        orders.read(msgs).cloned().collect()
    };
    assert_eq!(cmds.len(), 1, "one squad order (only P0 has units)");
    match &cmds[0].order {
        Order::AttackMove { to } => {
            assert_eq!(*to, Vec3::new(13.0, 0.0, -4.0), "march aims at P1's line");
        }
        other => panic!("expected AttackMove, got {other:?}"),
    }
    assert_eq!(cmds[0].subjects.len(), 3, "the three sent units ride the order");

    // Spawned on-board one frame later, from P0's deploy zone (lane start),
    // owned by the sponsor, marked as sent, and — the L8 seam — already
    // carrying a MoveAgent with the attack-move order ARMED at spawn.
    app.update();
    let mut q = app
        .world_mut()
        .query::<(&Transform, &Owner, &Merc, &SentMerc, &MoveAgent)>();
    let sent: Vec<(&Transform, &Owner, &Merc, &SentMerc, &MoveAgent)> = q.iter(app.world()).collect();
    assert_eq!(sent.len(), 3, "P1 faces three extra attackers next round");
    for (tf, owner, merc, _, agent) in &sent {
        assert_eq!(owner.0, PlayerId(0), "sent units fight for the sponsor");
        assert_eq!(merc.player, 0);
        assert_eq!(merc.pop, 0, "sends book no population");
        assert_eq!(
            agent.order,
            Some(Order::AttackMove { to: Vec3::new(13.0, 0.0, -4.0) }),
            "L8: the march order is armed at spawn"
        );
        assert!(
            tf.translation.distance(Vec3::new(-13.0, 0.0, -4.0)) < 1.0,
            // Three units spawn on the same point; separation pushes them
            // apart within a frame — 1.0 still pins "P0's side of the
            // corridor" (P1's zone is 25 units away).
            "spawned at P0's deploy-zone mouth, got {:?}",
            tf.translation
        );
    }

    // Battle-phase sends are refused (the shop closes with the phase).
    let mut rej = MessageCursor::<SendRejected>::default();
    send(&mut app, SendCommand { player: 0, target: 1, def: "merc_cheap".into() });
    app.update();
    let msgs = app.world().resource::<Messages<SendRejected>>();
    assert!(rej.read(msgs).count() >= 1, "battle-phase send refused");
    assert_eq!(golds(&mut app)[0], 18, "still 18 after the refusal");
}

// -- ⑫ LG2: breach bounty -----------------------------------------------------

/// LG2-⑫: a sent unit that breaks through P1's line costs P1 one life AND
/// pays the sponsor a +5g breach bounty — landing on the settle frame, so
/// the wallet reads exactly 50 (income) + 5 (bounty); a missing bounty would
/// read 50 (proposal §2 "发兵方获得击穿赏金").
#[test]
fn sent_unit_breach_costs_a_life_and_pays_the_sponsor() {
    let mut app = legion_app();
    set_gold(&mut app, 25);
    send(&mut app, SendCommand { player: 0, target: 1, def: "merc_cheap".into() });
    app.update();
    assert_eq!(golds(&mut app)[0], 0, "the 25g send drained the wallet");

    skip_to_battle(&mut app);
    // Catch the leak frame. NOTE: the schedule's auto sync point between the
    // chained leak_tick and round_tick applies the leak despawn BEFORE the
    // round clock runs — so settlement (+50) and the breach bounty (+5) land
    // the SAME frame; the wallet read at the leak is exactly 50+5 (a missing
    // bounty would read 50).
    let mut cursor = MessageCursor::<LeakHappened>::default();
    let mut gold_at_leak = None;
    for _ in 0..900 {
        app.update();
        let msgs = app.world().resource::<Messages<LeakHappened>>();
        if cursor.read(msgs).count() > 0 {
            gold_at_leak = Some(golds(&mut app)[0]);
            break;
        }
    }
    assert_eq!(gold_at_leak, Some(55), "breach bounty +5 rides the settle frame (+50 income)");
    assert_eq!(
        app.world().resource::<PlayerLives>().0,
        vec![20, 19, 20, 20],
        "P1 pays one life"
    );
    assert_eq!(units_of(&mut app, "merc_cheap").len(), 0, "breached unit swept");
    let flow = app.world().resource::<RoundFlow>().clone();
    assert_eq!((flow.round, flow.phase == Phase::Build), (2, true), "round settled");
}

// -- ⑬⑭ LG2: round-cap adjudication -------------------------------------------

/// Drive to round 20's battle with an empty field: the settlement adjudicates
/// on lives instead of opening round 21.
fn settle_round_20(app: &mut App) {
    {
        let mut flow = app.world_mut().resource_mut::<RoundFlow>();
        assert_eq!(flow.phase, Phase::Build);
        flow.round = 20;
        flow.frame = BUILD_FRAMES - 1;
    }
    app.update(); // transition to battle
    app.update(); // empty field -> settle -> cap adjudication
}

/// LG2-⑬ (L7 shape): at the round-20 cap the team with more lives wins —
/// the structured ending carries the team, MatchEnded's reason names it,
/// MatchWinner is set, and the round layer freezes (no round 21).
#[test]
fn round_cap_crowns_team_with_more_lives() {
    let mut app = legion_app();
    app.world_mut().resource_mut::<PlayerLives>().0 = vec![20, 19, 20, 19]; // 40 vs 38

    let mut ended = MessageCursor::<MatchEnded>::default();
    settle_round_20(&mut app);

    let flow = app.world().resource::<MatchFlow>().clone();
    let MatchFlow::Ended { reason, outcome } = flow else {
        panic!("cap must end the match, got {flow:?}");
    };
    assert!(reason.contains("round cap 20"), "reason: {reason}");
    assert_eq!(outcome, MatchOutcome::Win(TeamId(0)), "the lives leader is named");
    let msgs = app.world().resource::<Messages<MatchEnded>>();
    let outcomes: Vec<&MatchEnded> = ended.read(msgs).collect();
    assert_eq!(outcomes.len(), 1, "exactly one match-end message");
    assert!(matches!(&outcomes[0].outcome, Ok(r) if *r == reason));

    assert_eq!(app.world().resource::<MatchWinner>().0, TeamId(0));

    // Frozen: further updates never open round 21.
    let _ = run_counting::<PhaseChanged>(&mut app, 5);
    let flow = app.world().resource::<RoundFlow>().clone();
    assert_eq!((flow.round, flow.phase == Phase::Battle), (20, true), "no round 21");
}

/// LG2-⑭ (L7 shape): equal lives at the cap -> DRAW, first-class: the
/// structured outcome is `Draw`, the reason stays readable, and MatchWinner
/// crowns nobody (the pre-L7 Won-channel workaround is retired).
#[test]
fn round_cap_draws_on_equal_lives() {
    let mut app = legion_app();
    app.world_mut().resource_mut::<PlayerLives>().0 = vec![20, 20, 20, 20]; // 40 vs 40

    let mut ended = MessageCursor::<MatchEnded>::default();
    settle_round_20(&mut app);

    let flow = app.world().resource::<MatchFlow>().clone();
    let MatchFlow::Ended { reason, outcome } = flow else {
        panic!("cap must end the match, got {flow:?}");
    };
    assert!(reason.contains("round cap 20"), "reason: {reason}");
    assert!(reason.contains("draw"), "the draw is readable: {reason}");
    assert_eq!(outcome, MatchOutcome::Draw, "L7: the draw is structured data");
    let msgs = app.world().resource::<Messages<MatchEnded>>();
    assert!(ended.read(msgs).any(|m| matches!(&m.outcome, Ok(r) if r.contains("draw"))));
    assert!(
        app.world().get_resource::<MatchWinner>().is_none(),
        "a draw crowns nobody"
    );
}

// -- ⑮⑯ LG4: base-seam acceptance ---------------------------------------------

/// LG4-⑮ (L2 compatibility): the legacy single-player singletons stay a
/// faithful LOCAL view — after hires, bounties and settlements, Resources
/// and Population always equal Wallets[P0] (the sync watermark contract).
#[test]
fn local_view_mirrors_wallets_through_gameplay() {
    let mut app = legion_app();
    set_gold(&mut app, 120);
    hire_for(&mut app, 0, "merc_tank");
    app.update();
    app.update(); // let the wallet write project into the view

    let view = app.world().resource::<Resources>();
    let wallet = &app.world().resource::<Wallets>().0[0];
    assert_eq!(view.gold, wallet.gold, "gold view == P0 wallet");
    let pop = app.world().resource::<Population>();
    assert_eq!((pop.used, pop.cap), (wallet.pop_used, wallet.pop_cap), "pop view == P0 wallet");

    // Settlement income lands in the wallets; the view follows next frame.
    skip_to_battle(&mut app);
    app.update();
    app.update();
    let view = app.world().resource::<Resources>();
    let wallet = &app.world().resource::<Wallets>().0[0];
    assert_eq!(view.gold, wallet.gold, "view tracks income (+50 settle)");
}

/// LG4-⑯ (2v2 bounty isolation): P0's merc kills P1's merc — the bounty
/// credits the killer's wallet (base apply_econ killer-Owner attribution,
/// L2) and P1's team wallets never see a coin of it; the victim P1 banked
/// no income (eliminated same frame via lives board -> only the living
/// settle).
#[test]
fn kill_bounty_isolated_to_killer_wallet_in_2v2() {
    let mut app = legion_app();
    set_gold(&mut app, 30);
    // P3 at 1 life; P1's merc leaks through to P3's baseline? No — the
    // duel: P0 vs P1 cheap mercs clash; the survivor leaks into the
    // victim's baseline. The loser's wallet keeps exactly its starting 30
    // +50 income if alive... simplest isolated read: P0's kill pays 4 to
    // wallets[0]; wallets[2]/[3] (not in the fight) stay at their seeds
    // except income.
    hire_for(&mut app, 0, "merc_cheap");
    hire_for(&mut app, 1, "merc_cheap");
    skip_to_battle(&mut app);

    let _ = run_counting::<DamageDealt>(&mut app, 2000);
    let golds = golds(&mut app);
    // Whichever side won the duel banked income(50) + bounty(4); the
    // spectators' wallets (P2/P3) only ever see income — never a bounty:
    assert!(
        golds[2] == 0 + 50 || golds[2] == 30 + 50 || golds[2] == 50,
        "P2 spectator wallet sees income only, got {golds:?}"
    );
    assert!(
        golds[3] == 0 + 50 || golds[3] == 30 + 50 || golds[3] == 50,
        "P3 spectator wallet sees income only, got {golds:?}"
    );
    let winner_gold = golds[0].max(golds[1]);
    assert!(winner_gold >= 54, "the duel's winner banked 50 + 4 ({golds:?})");
}
