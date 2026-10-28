//! Round-4 LDEBT acceptance battery (plan v4 §Phase 1 line 3): the seven
//! uplited seams L1-L3 + L5-L8, one file so each hole's contract reads as
//! a unit. Games keep their own behavioral batteries (legion behavior.rs);
//! here the SEAM is the subject.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use warbase::prelude::*;
use warbase::testing::test_app;

// -- shared helpers ---------------------------------------------------------

/// A unit-carrying world entity to use as a killer in synthetic
/// `UnitDied` messages (apply_econ only reads its `Owner`).
fn owned_unit(world: &mut World, owner: Option<PlayerId>) -> Entity {
    let mut e = world.spawn((UnitInstance { def: "victim".into(), level: 1 }, Hp(10.0)));
    if let Some(p) = owner {
        e.insert(Owner(p));
    }
    e.id()
}

fn died(killer: Option<DamageOrigin>, gold: u32) -> UnitDied {
    UnitDied { entity: Entity::PLACEHOLDER, def: "victim".into(), gold, wood: 0, xp: 0, killer }
}

/// A bare app with 2v2 wallets: P0+P2 team 0 vs P1+P3 team 1, local = P0,
/// every wallet seeded to 100g (mirror left at zero so the first update
/// projects the local view — matching the projection branch under test).
fn mp2_app() -> App {
    let mut app = test_app();
    let world = app.world_mut();
    world.insert_resource(MatchDiplomacy {
        players: vec![
            (PlayerId(0), TeamId(0)),
            (PlayerId(1), TeamId(1)),
            (PlayerId(2), TeamId(0)),
            (PlayerId(3), TeamId(1)),
        ],
        hostile_pairs: vec![(TeamId(0), TeamId(1))],
    });
    let wallet = Wallet { gold: 100, ..Default::default() };
    world.insert_resource(Wallets(vec![wallet; 4]));
    app
}

// -- L2: per-player economy ---------------------------------------------------

/// L2-i (zero drift): an UNOWNED killer credits the local wallet and the
/// legacy `Resources` view together — the exact pre-L2 global semantics
/// single-player maps (warden/loop-td/rts-lab) rely on.
#[test]
fn unowned_kill_bounty_lands_in_local_wallet_and_view() {
    let mut app = test_app();
    let killer = owned_unit(app.world_mut(), None);
    app.world_mut().write_message(died(Some(DamageOrigin::Attack(killer)), 7));
    app.update();
    let w = &app.world().resource::<Wallets>().0[0];
    assert_eq!((w.gold, w.wood), (7, 0), "local wallet credited");
    assert_eq!(app.world().resource::<Resources>().gold, 7, "legacy view identical");
}

/// L2-ii (attribution): P0's unit kills -> P0's wallet grows, P1's does
/// not, and the local view (P0) grows with it.
#[test]
fn owned_kill_credits_killer_wallet_only() {
    let mut app = mp2_app();
    app.update(); // project: view <- wallets[local] (100g)

    let killer = owned_unit(app.world_mut(), Some(PlayerId(0)));
    app.world_mut().write_message(died(Some(DamageOrigin::Attack(killer)), 12));
    app.update();

    let wallets = &app.world().resource::<Wallets>().0;
    assert_eq!(wallets[0].gold, 112, "P0 wallet takes the bounty");
    assert_eq!(wallets[1].gold, 100, "P1 wallet untouched");
    assert_eq!(app.world().resource::<Resources>().gold, 112, "local view follows P0");
}

/// L2-iii (remote attribution): P1's unit kills -> only P1's wallet grows;
/// the local view (P0) does NOT move (this is the double-accounting the
/// pre-L2 base could not express).
#[test]
fn remote_owner_kill_credits_only_that_wallet() {
    let mut app = mp2_app();
    app.update();

    let killer = owned_unit(app.world_mut(), Some(PlayerId(1)));
    app.world_mut().write_message(died(Some(DamageOrigin::Attack(killer)), 9));
    app.update();

    let wallets = &app.world().resource::<Wallets>().0;
    assert_eq!(wallets[1].gold, 109, "P1 wallet credited");
    assert_eq!(wallets[0].gold, 100, "P0 wallet untouched");
    assert_eq!(
        app.world().resource::<Resources>().gold,
        100,
        "the local view mirrors P0 only"
    );
}

/// L2-iv (view identity): wallet writes project into the view, direct view
/// writes absorb into the wallet — the invariant is `Resources ==
/// Wallets[local]` after either kind of write.
#[test]
fn local_view_and_wallet_stay_identical_both_directions() {
    let mut app = mp2_app();
    app.update();
    assert_eq!(app.world().resource::<Resources>().gold, 100);

    // MP-style write: the wallet is the writer -> the view projects.
    app.world_mut().resource_mut::<Wallets>().of_mut(PlayerId(0)).gold = 42;
    app.update();
    assert_eq!(app.world().resource::<Resources>().gold, 42, "view follows wallet");

    // Legacy-style write: the view is the writer (cheat panel / old game
    // code) -> the wallet absorbs.
    app.world_mut().resource_mut::<Resources>().gold = 77;
    app.update();
    assert_eq!(
        app.world().resource::<Wallets>().0[0].gold,
        77,
        "wallet absorbs the view write"
    );

    // Idle frame: converged, nothing moves (the zero-write branch).
    app.update();
    assert_eq!(app.world().resource::<Resources>().gold, 77);
    assert_eq!(app.world().resource::<Wallets>().0[0].gold, 77);
}

// -- L7: structured match endings ---------------------------------------------

/// L7-i: `MatchFlow::Ended` carries the structured outcome — a Draw is
/// first-class data (the pre-L7 legion had to ride the Won channel with a
/// reason-string convention), and the reason stays the readable contract.
/// Game-side write path (round-cap adjudication) exercised directly.
#[test]
fn ended_flow_carries_structured_win_and_draw() {
    // Draw: no winner crowns.
    let mut app = test_app();
    let mut ended = MessageCursor::<MatchEnded>::default();
    *app.world_mut().resource_mut::<MatchFlow>() = MatchFlow::Ended {
        reason: "round cap 20: draw on lives (t0=20, t1=20)".into(),
        outcome: MatchOutcome::Draw,
    };
    app.world_mut().write_message(MatchEnded {
        outcome: Ok("round cap 20: draw on lives (t0=20, t1=20)".into()),
    });
    app.update();
    let flow = app.world().resource::<MatchFlow>().clone();
    assert!(matches!(
        flow,
        MatchFlow::Ended { outcome: MatchOutcome::Draw, ref reason } if reason.contains("draw")
    ));
    assert!(
        app.world().get_resource::<MatchWinner>().is_none(),
        "a draw crowns nobody"
    );
    let msgs = app.world().resource::<Messages<MatchEnded>>();
    assert!(ended.read(msgs).any(|m| matches!(&m.outcome, Ok(r) if r.contains("draw"))));

    // Win: the outcome carries the team as data.
    let flow = MatchFlow::Ended {
        reason: "last team standing on lives: team 0 wins".into(),
        outcome: MatchOutcome::Win(TeamId(0)),
    };
    assert!(matches!(flow, MatchFlow::Ended { outcome: MatchOutcome::Win(TeamId(0)), .. }));
}

// -- L5: team-lives win condition ---------------------------------------------

/// A 2v2 app adjudicating on `TeamLives` (rules/diplomacy/lives board
/// inserted directly — rules_tick reads resources, it does not care who
/// assembled them) with one owned unit per player.
fn team_lives_app() -> App {
    let mut app = test_app();
    let world = app.world_mut();
    world.insert_resource(MatchDiplomacy {
        players: vec![
            (PlayerId(0), TeamId(0)),
            (PlayerId(1), TeamId(1)),
            (PlayerId(2), TeamId(0)),
            (PlayerId(3), TeamId(1)),
        ],
        hostile_pairs: vec![(TeamId(0), TeamId(1))],
    });
    world.insert_resource(MatchRules {
        leak: LeakPolicy::Lives { lives: 20 },
        build_window: BuildWindow::Always,
        lap_buff: None,
        win: WinCondition::TeamLives { per_team: 20 },
        sell_refund_pct: 0.7,
    });
    world.insert_resource(PlayerLives(vec![20, 20, 20, 20]));
    for p in 0..4 {
        let e = owned_unit(world, Some(PlayerId(p)));
        world.entity_mut(e).insert(Transform::from_xyz(p as f32, 0.0, 0.0));
    }
    // One update before handing over (the legion_app convention): Bevy
    // 0.19 rejects entity commands on the FIRST update for entities
    // despawned before it (first-frame schedule init) — touch entities
    // only after the app has run once.
    app.update();
    *app.world_mut().resource_mut::<MatchFlow>() = MatchFlow::Running;
    app
}

/// L5-i: presence is the LIVES board, not the field — an empty arena with
/// every team alive never crowns (the exact mis-fire the 0-damage banner
/// kits used to paper over), and eliminating a whole team via the board
/// sweeps their units and ends the match with the structured winner.
#[test]
fn team_lives_adjudicates_on_the_board_not_the_field() {
    let mut app = team_lives_app();

    // Empty the field entirely (simulating a build phase before anyone
    // hired): the match must keep running.
    {
        // NOTE: despawn the UNITS only — `query::<Entity>()` would sweep
        // MinimalPlugins' own internal entities too and Bevy's first-frame
        // bookkeeping then errors on their stale ids (pitfall, KB'd).
        let world = app.world_mut();
        let mut q = world.query_filtered::<Entity, With<UnitInstance>>();
        let units: Vec<Entity> = q.iter(world).collect();
        for e in units {
            world.despawn(e);
        }
    }
    app.update();
    assert_eq!(
        *app.world().resource::<MatchFlow>(),
        MatchFlow::Running,
        "empty field, all teams alive: no crown"
    );

    // Team 1 eliminated through the board: their units are swept and the
    // structured ending fires with MatchWinner.
    let mut app = team_lives_app();
    {
        let world = app.world_mut();
        world.resource_mut::<PlayerLives>().0 = vec![20, 0, 20, 0];
    }
    let mut ended = MessageCursor::<MatchEnded>::default();
    app.update();
    app.update(); // deferred sweep commands apply, adjudication lands
    let flow = app.world().resource::<MatchFlow>().clone();
    assert!(matches!(
        flow,
        MatchFlow::Ended { ref reason, outcome: MatchOutcome::Win(TeamId(0)) }
            if reason.contains("team 0")
    ));
    assert_eq!(app.world().resource::<MatchWinner>().0, TeamId(0));
    let msgs = app.world().resource::<Messages<MatchEnded>>();
    assert!(ended.read(msgs).any(|m| matches!(&m.outcome, Ok(r) if r.contains("team 0"))));

    // The sweep: P1/P3 units despawned, team-0 units untouched.
    let world = app.world_mut();
    let mut q = world.query_filtered::<&Owner, With<UnitInstance>>();
    let owners: Vec<u8> = q.iter(world).map(|o| o.0 .0).collect();
    assert_eq!(owners, vec![0, 2], "eliminated players' units swept");

    // A single-team-1 survivor (P3 alive) keeps the match running.
    let mut app = team_lives_app();
    app.world_mut().resource_mut::<PlayerLives>().0 = vec![20, 0, 20, 0];
    // P3 revived to 1 via board write before any update:
    app.world_mut().resource_mut::<PlayerLives>().0[3] = 1;
    app.update();
    app.update();
    assert_eq!(
        *app.world().resource::<MatchFlow>(),
        MatchFlow::Running,
        "team 1 still stands (P3 has lives)"
    );
}

// -- L8: owned-unit injection seam ---------------------------------------------

/// A registry with one marching def + 2v2 diplomacy.
fn injectable_app() -> App {
    let mut app = test_app();
    let world = app.world_mut();
    world.insert_resource(UnitRegistry {
        defs: [(
            "grunt".to_string(),
            UnitDef {
                id: "grunt".into(),
                attrs: BaseAttributes { hp: 50.0, move_speed: 3.0, body_radius: 0.3 },
                attack: None,
                armor_type: ArmorType::Light,
                cost_gold: 0,
                cost_wood: 0,
                bounty_gold: 0,
                bounty_wood: 0,
                grant_xp: 0,
                traits: Vec::new(),
                auras: Vec::new(),
                hero: false,
                skills: Vec::new(),
                skill_levels: Vec::new(),
                upgrade_to: None,
                heal: None,
                leak: 1,
                pop_cost: 1,
                mana: None,
                hero_stats: None,
            },
        )]
        .into_iter()
        .collect(),
    });
    world.insert_resource(MatchDiplomacy {
        players: vec![
            (PlayerId(0), TeamId(0)),
            (PlayerId(1), TeamId(1)),
            (PlayerId(2), TeamId(0)),
            (PlayerId(3), TeamId(1)),
        ],
        hostile_pairs: vec![(TeamId(0), TeamId(1))],
    });
    app
}

/// L8-i: a SpawnRequest for P1 (team 1, hostile to team 0) spawns an OWNED
/// unit — `Owner(P1)` + derived `Attacker` allegiance — marching via a
/// `MoveAgent` whose attack-move order is ARMED AT SPAWN (the same-frame
/// seam; no observer, no next-frame command needed).
#[test]
fn owned_injection_spawns_with_owner_and_armed_attack_move() {
    let mut app = injectable_app();
    let to = Vec3::new(13.0, 0.0, -4.0);
    app.world_mut().write_message(SpawnRequest {
        def: "grunt".into(),
        owner: PlayerId(1),
        pos: Vec3::new(-13.0, 0.0, -4.0),
        hp_scale: 1.0,
        march: MarchMode::AttackMove { to },
    });
    app.update();
    app.update(); // deferred spawn materialized

    let world = app.world_mut();
    let mut q = world.query_filtered::<
        (&Owner, &Allegiance, &MoveAgent, &AgentOrderState),
        With<UnitInstance>,
    >();
    let injected: Vec<_> = q.iter(world).collect();
    assert_eq!(injected.len(), 1, "exactly one injected unit");
    let (owner, side, agent, state) = injected[0];
    assert_eq!(owner.0, PlayerId(1), "the spawn carries its owner");
    assert_eq!(*side, Allegiance::Attacker, "team-1 owner derives Attacker");
    assert_eq!(agent.order, Some(Order::AttackMove { to }), "order armed at spawn");
    assert_eq!(agent.waypoints, vec![to], "straight-line seed waypoint");
    assert_eq!(*state, AgentOrderState::Moving, "marching, not idle");
    assert_eq!(agent.speed, 3.0, "speed from the def");
}

/// L8-ii: the FollowPath mode attaches a PathFollower on the requested
/// path entity (the TD track shape), still owned.
#[test]
fn owned_injection_follows_the_requested_path() {
    let mut app = injectable_app();
    let path_entity = {
        let world = app.world_mut();
        let e = world.spawn(PathDef::open(vec![[-10.0, 0.0], [10.0, 0.0]])).id();
        world.insert_resource(Paths(vec![e]));
        e
    };
    app.world_mut().write_message(SpawnRequest {
        def: "grunt".into(),
        owner: PlayerId(0),
        pos: Vec3::new(-10.0, 0.0, 0.0),
        hp_scale: 1.0,
        march: MarchMode::FollowPath { path: 0 },
    });
    app.update();
    app.update();

    let world = app.world_mut();
    let mut q = world.query_filtered::<(&Owner, &PathFollower), With<UnitInstance>>();
    let injected: Vec<_> = q.iter(world).collect();
    assert_eq!(injected.len(), 1);
    assert_eq!(injected[0].0 .0, PlayerId(0));
    assert_eq!(injected[0].1.path, path_entity, "follower rides the requested path");
}

// -- L1: routing-only paths -----------------------------------------------------

/// L1-i: `walkable: false` paths never land in the build grid's blocked
/// set (nor, through apply_nav's single source, in the nav grid) — they
/// stay routable while the corridor stays buildable/marchable. Walkable
/// paths (serde default true) keep blocking exactly as before.
#[test]
fn routing_only_paths_do_not_block_the_build_grid() {
    use warbase::bundle::{build_grid, BuildDomainDef, WorldDef};
    let world = WorldDef {
        paths: vec![
            // Routing-only lane across the middle (would block a full row
            // of the 4x2 grid if counted).
            PathDef { points: vec![[-10.0, 0.0], [10.0, 0.0]], closed: false, junctions: Vec::new(), walkable: false },
            // TD creep track at the top edge: terrain, as always.
            PathDef { points: vec![[-2.0, 0.9], [2.0, 0.9]], closed: false, junctions: Vec::new(), walkable: true },
        ],
        build: BuildDomainDef::FreeGrid { cell: 1.0, cols: 4, rows: 2 },
        blocked: Vec::new(),
    };
    let grid = build_grid(&world);
    assert_eq!(grid.mode, BuildMode::FreeGrid);
    // The walkable track blocks its cells (zero-drift legacy behavior).
    assert!(
        grid.blocked.contains(&grid.world_to_cell(Vec2::new(0.0, 0.9))),
        "walkable path still blocks"
    );
    // The routing-only lane does NOT block anywhere along its span.
    for x in [-9.0f32, -5.0, 0.0, 5.0, 9.0] {
        let probe = grid.world_to_cell(Vec2::new(x, 0.0));
        assert!(!grid.blocked.contains(&probe), "routing-only lane cell {probe:?} stays open");
    }
    // A walkable-path-less world centers the grid on the origin (the
    // pathless rule; routing-only maps behave like skirmish arenas).
    let world = WorldDef {
        paths: vec![PathDef {
            points: vec![[-13.0, -4.0], [13.0, 4.0]],
            closed: false,
            junctions: Vec::new(),
            walkable: false,
        }],
        build: BuildDomainDef::FreeGrid { cell: 1.0, cols: 4, rows: 3 },
        blocked: Vec::new(),
    };
    let grid = build_grid(&world);
    assert_eq!(grid.origin, Vec2::new(-2.0, -1.5), "origin-centered FreeGrid");
    assert!(grid.blocked.is_empty(), "no terrain from routing-only paths");
}

// -- L3: def-level population cost ----------------------------------------------

/// L3-i: `pop_cost` ships per def (legion's 1-4 band) and defaults to 1
/// for defs that omit it — old RON loads unchanged, every unit is one pop
/// slot unless the map says otherwise.
#[test]
fn pop_cost_ships_per_def_and_defaults_to_one() {
    let with_field: UnitDef = ron::from_str(
        r#"(id: "merc_tank", attrs: (hp: 420.0, move_speed: 2.4, body_radius: 0.5), armor_type: Heavy, pop_cost: 3)"#,
    )
    .expect("parse");
    assert_eq!(with_field.pop_cost, 3);

    let without: UnitDef = ron::from_str(
        r#"(id: "merc_cheap", attrs: (hp: 90.0, move_speed: 3.2, body_radius: 0.3), armor_type: Light)"#,
    )
    .expect("parse");
    assert_eq!(without.pop_cost, 1, "serde default: one body, one slot");
}
