//! rts-lab RL1 acceptance battery (ADR-0016 §11, GDD「数字化验收标准」):
//! headless apps drive the thin game layer over warbase mechanisms.
//!
//! ① map/teams/production assemble + starting layout kits
//! ② train: pay -> 300 frames -> +1 peasant / gold spent / pop +1
//! ③ harvest loop: Carrying -> deposit, two trips = +20 gold
//! ④ selection: left-click pick / right-click Move order / Aabb box select
//! ⑤ LastTeamStanding: grunts eliminated -> team 0 wins
//! (⑥ zero-engine-diff is a git-level gate, evidenced in the report — not a
//! runtime assertion.)

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use rts_lab::components::{Carrying, GoldMine, HarvestRoute, MainHall};
use rts_lab::resources::CurrentMap;
use warbase::prelude::*;
use warbase::testing::{send, test_app};

// -- helpers ---------------------------------------------------------------

/// Headless lab: base + game core, one update (Startup assembles the map and
/// the starting layout; observers attach kit + production queue).
fn lab_app() -> App {
    let mut app = test_app();
    app.add_plugins(rts_lab::plugins::game::GamePlugin);
    app.update();
    app
}

/// All live instances of a def id.
fn units_of(app: &mut App, def: &str) -> Vec<Entity> {
    let mut q = app.world_mut().query::<(Entity, &UnitInstance)>();
    q.iter(app.world()).filter(|(_, i)| i.def == def).map(|(e, _)| e).collect()
}

fn pos_of(app: &App, e: Entity) -> Vec3 {
    app.world().get::<Transform>(e).unwrap().translation
}

/// Freeze agents in place (clear order + waypoints), keep everything else.
fn stop_units(app: &mut App, entities: &[Entity]) {
    for &e in entities {
        let Ok(mut em) = app.world_mut().get_entity_mut(e) else { continue };
        if let Some(mut agent) = em.get_mut::<MoveAgent>() {
            agent.order = None;
            agent.waypoints.clear();
        }
    }
}

/// Freeze the peasants AND take them off the harvest loop (static dolls for
/// pick/move assertions; no income side effects).
fn calm_peasants(app: &mut App) -> Vec<Entity> {
    let peasants = units_of(app, "peasant");
    for e in &peasants {
        let Ok(mut em) = app.world_mut().get_entity_mut(*e) else { continue };
        em.remove::<HarvestRoute>();
        if let Some(mut agent) = em.get_mut::<MoveAgent>() {
            agent.order = None;
            agent.waypoints.clear();
        }
    }
    peasants
}

fn set_gold(app: &mut App, gold: u32) {
    app.world_mut().resource_mut::<Resources>().gold = gold;
}

// -- ① assembly ------------------------------------------------------------

/// RL1-①: the shipped arena.ron assembles — teams/diplomacy, production
/// tables, nav walls, and the starting layout with the full free-agent kit.
#[test]
fn arena_map_assembles_with_teams_production_and_kit() {
    let mut app = lab_app();

    let bundle = app.world().resource::<CurrentMap>().0.clone();
    assert_eq!(bundle.identity.id, "arena");
    assert_eq!(bundle.teams.players, vec![(0, 0), (1, 1)]);
    assert_eq!(bundle.teams.hostile_pairs, vec![(0, 1)]);
    assert_eq!(
        app.world().resource::<MatchDiplomacy>().players,
        vec![(PlayerId(0), TeamId(0)), (PlayerId(1), TeamId(1))]
    );
    assert!(app
        .world()
        .resource::<Hostility>()
        .hostile(Allegiance::Defender, Allegiance::Attacker));

    let trains = &app.world().resource::<TrainTable>().0;
    assert_eq!(trains.get("main_hall"), Some(&vec!["peasant".to_string()]));
    assert_eq!(app.world().resource::<TrainTimes>().0.get("peasant"), Some(&5.0));

    // Perimeter path doubles as the arena wall: border cell blocked, center free.
    let nav = app.world().resource::<NavGrid>();
    assert!(nav.has_grid() && nav.cols == 24 && nav.rows == 24);
    assert!(nav.blocked.contains(&(0, 0)) && nav.blocked.contains(&(23, 23)));
    assert!(!nav.blocked.contains(&(12, 12)), "map center must stay walkable");

    // Starting layout: 1 hall / 1 mine / 4 peasants / 3 grunts, all selectable.
    assert_eq!(units_of(&mut app, "main_hall").len(), 1);
    assert_eq!(units_of(&mut app, "gold_mine").len(), 1);
    let peasants = units_of(&mut app, "peasant");
    let grunts = units_of(&mut app, "grunt");
    assert_eq!(peasants.len(), 4);
    assert_eq!(grunts.len(), 3);
    for &e in peasants.iter().chain(grunts.iter()) {
        assert!(app.world().get::<Selectable>(e).is_some(), "def-spawned units are selectable");
    }

    // Peasant kit: free agent + harvest route bound to the single hall/mine.
    let hall = units_of(&mut app, "main_hall")[0];
    let mine = units_of(&mut app, "gold_mine")[0];
    assert!(app.world().get::<ProductionQueue>(hall).is_some(), "queue attached");
    for &p in &peasants {
        assert!(app.world().get::<MoveAgent>(p).is_some(), "peasant moves");
        let route = app.world().get::<HarvestRoute>(p).expect("peasant on a harvest route");
        assert_eq!((route.mine, route.hall), (mine, hall));
        assert!(app.world().get::<GoldMine>(mine).is_some());
        assert!(app.world().get::<MainHall>(hall).is_some());
    }
    // Grunts: armed guards that stay put (no MoveAgent).
    for &g in &grunts {
        assert!(app.world().get::<AttackKit>(g).is_some());
        assert!(app.world().get::<MoveAgent>(g).is_none(), "grunt is a stationary guard");
    }
    // Population booked for the initial workers.
    assert_eq!(app.world().resource::<Population>().used, 4);
}

// -- ② training ------------------------------------------------------------

/// RL1-②: from gold 100, one TrainCommand pays immediately; exactly 300
/// frames (5s at the fixed 1/60 step) later the graduate is on the field,
/// gold is spent, population +1.
#[test]
fn train_command_pays_and_spawns_after_300_frames() {
    let mut app = lab_app();
    calm_peasants(&mut app); // no harvest income: gold assertions stay exact
    set_gold(&mut app, 100);

    let hall = units_of(&mut app, "main_hall")[0];
    send(&mut app, TrainCommand { builder: hall, unit: "peasant".into() });
    assert_eq!(app.world().resource::<Resources>().gold, 100 - 50, "pays on enqueue");
    assert_eq!(app.world().get::<ProductionQueue>(hall).unwrap().entries.len(), 1);

    for _ in 0..305 {
        app.update();
    }
    assert_eq!(units_of(&mut app, "peasant").len(), 5, "graduate on the field");
    assert_eq!(app.world().resource::<Resources>().gold, 50, "gold spent, no refund");
    assert_eq!(app.world().resource::<Population>().used, 5, "population +1");
    assert_eq!(app.world().get::<ProductionQueue>(hall).unwrap().entries.len(), 0);
    // The graduate is a full peasant: selectable, moving, on the harvest loop.
    // (Query order is arbitrary — identify it by the highest entity index;
    // this test despawns nothing, so ids stay spawn-ordered.)
    let mut peasants = units_of(&mut app, "peasant");
    peasants.sort_by_key(|e| e.index_u32());
    let p = *peasants.last().unwrap();
    assert!(app.world().get::<MoveAgent>(p).is_some());
    assert!(app.world().get::<HarvestRoute>(p).is_some());
}

// -- ③ harvest loop ---------------------------------------------------------

/// RL1-③: a lone peasant parked beside the mine loads Carrying(10), walks
/// home, deposits +10, and loops — two full trips add exactly +20 gold.
#[test]
fn harvest_loop_two_trips_deposit_twenty() {
    let mut app = lab_app();
    let peasants = units_of(&mut app, "peasant");
    for e in &peasants[1..] {
        app.world_mut().entity_mut(*e).despawn();
    }
    let worker = peasants[0];
    stop_units(&mut app, &[worker]);
    set_gold(&mut app, 0);
    // Park the worker within HARVEST_RADIUS of the mine (mine sits at (5, 0)).
    *app.world_mut().get_mut::<Transform>(worker).unwrap() =
        Transform::from_xyz(4.0, 0.0, 0.0);

    app.update(); // idle at the mine -> load + walk home
    assert_eq!(
        app.world().get::<Carrying>(worker).map(|c| c.0),
        Some(10),
        "loaded at the mine"
    );

    // Leg time: 10 world units @ speed 3.0 = 200 frames; two round trips
    // finish well inside 1200. Loop exits the moment gold hits 20.
    let mut frames = 0;
    while app.world().resource::<Resources>().gold < 20 && frames < 1200 {
        app.update();
        frames += 1;
    }
    assert_eq!(
        app.world().resource::<Resources>().gold,
        20,
        "exactly two deposits (+10 each) after {frames} frames"
    );
    assert!(
        frames > 400,
        "two full mine<->hall round trips must take real travel time ({frames})"
    );
}

// -- ④ selection / smart command --------------------------------------------
//
// NOTE (hole #4, see input_bridge.rs): the base pick chain's world-geometry
// assumes the z=0 plane while units/nav play on y=0, so headless acceptance
// drives the base's PLANE-FREE command seam (SelectCommand / UnitCommand)
// directly — the base's own headless discipline (plan §4: "选择逻辑只认
// SelectCommand，headless 直发命令即测"). Real-mouse translation lives in the
// game-side bridge; its decision core is the pure `right_click_order`.

/// RL1-④a: selecting a peasant settles SelectionState, the marker, and the
/// HUD panel projection.
#[test]
fn select_command_selects_peasant_and_fills_panel() {
    let mut app = lab_app();
    calm_peasants(&mut app);
    let p = units_of(&mut app, "peasant")[0];

    send(&mut app, SelectCommand::Set(vec![p]));
    assert_eq!(app.world().resource::<SelectionState>().entities, vec![p]);
    assert!(app.world().get::<Selected>(p).is_some());
    assert_eq!(app.world().resource::<SelectionPanelData>().names, vec!["peasant"]);
}

/// RL1-④b: a Move order for the selection walks the peasant across the
/// arena and spends itself on arrival.
#[test]
fn move_order_walks_peasant_to_target() {
    let mut app = lab_app();
    calm_peasants(&mut app);
    let p = units_of(&mut app, "peasant")[0];
    let start = pos_of(&app, p);

    let target = Vec3::new(2.0, 0.0, 5.0);
    send(
        &mut app,
        UnitCommand { order: Order::Move { to: target }, subjects: vec![p] },
    );

    for _ in 0..30 {
        app.update();
    }
    assert!(
        pos_of(&app, p).distance(target) < start.distance(target),
        "displacing toward the target"
    );
    for _ in 0..200 {
        app.update(); // 6.5 units @ 3.0 u/s ~= 130 frames; margin included
    }
    let dist = pos_of(&app, p).distance(target);
    assert!(dist <= 0.15, "arrived (distance {dist})");
    assert!(
        matches!(app.world().get::<AgentOrderState>(p), Some(state) if *state == AgentOrderState::Idle),
        "order spent after arrival"
    );
}

/// RL1-④b': the game-side bridge's smart-order decision core: hostile hits
/// attack, friendly hits issue nothing, open ground moves ON the gameplay
/// plane (x, 0, z) — the exact spot the base chain gets wrong.
#[test]
fn right_click_order_pure_decisions() {
    use rts_lab::systems::input_bridge::right_click_order;
    let grunt = Entity::from_bits(123);
    let mate = Entity::from_bits(456);
    let ground = Vec2::new(2.0, 5.0);

    assert_eq!(
        right_click_order(Some(grunt), true, ground),
        Some(Order::AttackTarget { target: grunt }),
        "hostile first-hit -> attack"
    );
    assert_eq!(right_click_order(Some(mate), false, ground), None, "friendly hit -> nothing");
    assert_eq!(
        right_click_order(None, false, ground),
        Some(Order::Move { to: Vec3::new(2.0, 0.0, 5.0) }),
        "open ground -> move on the y=0 plane"
    );
}

/// RL1-④c: an Aabb box select covering exactly two peasants selects exactly
/// those two (disc hit test, hall disc deliberately outside the rect).
#[test]
fn aabb_box_selects_exactly_two() {
    let mut app = lab_app();
    calm_peasants(&mut app);

    // Covers (-3.5, +/-1.5) both; hall (-5, 0, r 1.2) and (-2.5, +/-3.0)
    // stay outside (closest hall approach 1.3 > 1.2 + eps).
    send(
        &mut app,
        SelectCommand::Aabb { min: Vec2::new(-3.7, -2.2), max: Vec2::new(-2.9, 2.2) },
    );
    let state = app.world().resource::<SelectionState>();
    assert_eq!(state.entities.len(), 2, "exactly the two boxed peasants");
    for &e in &state.entities {
        assert_eq!(app.world().get::<UnitInstance>(e).unwrap().def, "peasant");
        assert!(app.world().get::<Selected>(e).is_some());
    }
}

// -- ⑤ last team standing ----------------------------------------------------

/// RL1-⑤: eliminate the three grunts with direct DamageRequests (test-side
/// kills) -> the base MP2 rule declares MatchEnded Ok + MatchWinner team 0
/// (peasants' weak militia kits keep team 0 standing).
#[test]
fn last_team_standing_wins_for_team_zero() {
    let mut app = lab_app();
    calm_peasants(&mut app);

    let grunts = units_of(&mut app, "grunt");
    assert_eq!(grunts.len(), 3);
    for g in grunts {
        app.world_mut().write_message(DamageRequest {
            target: g,
            source: DamageOrigin::System,
            amount: 999.0,
            attack_type: AttackType::Normal,
            crit: false,
        });
    }
    let mut ended = MessageCursor::<MatchEnded>::default();
    app.update(); // Resolve kills -> Cleanup: despawn + rules_tick verdict

    let msgs = app.world().resource::<Messages<MatchEnded>>();
    let outcomes: Vec<_> = ended.read(msgs).collect();
    assert_eq!(outcomes.len(), 1, "exactly one match-end message");
    assert!(outcomes[0].outcome.is_ok());
    assert!(matches!(
        app.world().resource::<MatchFlow>(),
        MatchFlow::Won(_)
    ));
    assert_eq!(app.world().resource::<MatchWinner>().0, TeamId(0));
    assert_eq!(units_of(&mut app, "grunt").len(), 0, "guards despawned");
}
