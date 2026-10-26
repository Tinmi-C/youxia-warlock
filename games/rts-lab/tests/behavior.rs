//! rts-lab RL1-RL2 acceptance battery (ADR-0016 §11, GDD「数字化验收标准」):
//! headless apps drive the thin game layer over warbase mechanisms.
//!
//! RL1
//! ① map/teams/production assemble + starting layout kits
//! ② train: pay -> 300 frames -> +1 peasant / gold spent / pop +1
//! ③ harvest loop: Carrying -> deposit, two trips = +20 gold
//! ④ selection: left-click pick / right-click Move order / Aabb box select
//! ⑤ LastTeamStanding: grunts eliminated -> team 0 wins
//! (⑥ zero-engine-diff is a git-level gate, evidenced in the report — not a
//! runtime assertion.)
//!
//! RL2
//! ⑦ A-key targeting: CommandHotkey arms PendingTargetOrder; the game-side
//!    click transition issues AttackMove on the y=0 plane; a peasant under
//!    that order engages an enemy on its path (DamageDealt appears)
//! ⑧ barracks trains footman (melee kit) and archer (projectile kit)
//! ⑨ holy_light: hero mana 100 -40 -> SkillCast -> HealRequest -> ally Hp+;
//!    insufficient mana is refused; aim policy picks the most wounded ally
//! ⑩ research: attack_upgrade done -> footman DamageMult 1.15 (old + new)
//! ⑪ items: kill grunt -> GroundItem -> walk-over pickup -> 1-key use -> Hp+
//!    (plus shop buy -> gold spent -> stack stowed)

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

// -- ⑦ RL2: A-key attack-move targeting + engagement -------------------------

/// RL2-⑦a: the pure game-side leg of the base PendingTargetOrder machine:
/// A armed + click -> AttackMove ON the gameplay plane; P is a two-click
/// machine (first parks a, second fires Patrol{a, b}); ground Vec2 is (x, z).
#[test]
fn pending_click_transition_pure_decisions() {
    use rts_lab::systems::input_bridge::pending_click_transition;
    let ground = Vec2::new(2.0, 5.0);

    // A armed -> fires immediately at (x, 0, z), machine back to Idle.
    assert_eq!(
        pending_click_transition(PendingTargetOrder::AttackMove, ground),
        (
            PendingTargetOrder::Idle,
            Some(Order::AttackMove { to: Vec3::new(2.0, 0.0, 5.0) })
        )
    );
    // P first click parks point a (nothing fires yet).
    let (next, order) = pending_click_transition(PendingTargetOrder::PatrolFirst, ground);
    assert_eq!(next, PendingTargetOrder::PatrolSecond { a: Vec3::new(2.0, 0.0, 5.0) });
    assert_eq!(order, None);
    // P second click fires Patrol{a, b} on the plane.
    let a = Vec3::new(2.0, 0.0, 5.0);
    assert_eq!(
        pending_click_transition(PendingTargetOrder::PatrolSecond { a }, Vec2::new(7.0, -1.0)),
        (
            PendingTargetOrder::Idle,
            Some(Order::Patrol { a, b: Vec3::new(7.0, 0.0, -1.0), toward_b: true })
        )
    );
    // Idle never targets.
    assert_eq!(
        pending_click_transition(PendingTargetOrder::Idle, ground),
        (PendingTargetOrder::Idle, None)
    );
}

/// RL2-⑦b: full chain — select a peasant, A arms the base PendingTargetOrder
/// (CommandHotkey path), the click transition yields AttackMove, the UnitCommand
/// walks the peasant, and an enemy planted on its route gets ENGAGED
/// (DamageDealt appears; the base acquire_engagement seam).
#[test]
fn attack_move_order_engages_enemy_on_route() {
    let mut app = lab_app();
    calm_peasants(&mut app);
    let peasants = units_of(&mut app, "peasant");
    let p = peasants[0];
    let mut grunts = units_of(&mut app, "grunt");
    let grunt = grunts.pop().unwrap();
    // Park the grunt mid-field on the peasant's route (peasant at (-3.5, 1.5)).
    *app.world_mut().get_mut::<Transform>(grunt).unwrap() = Transform::from_xyz(0.0, 0.0, 1.5);
    let grunt_max = app.world().get::<MaxHp>(grunt).unwrap().0;

    // 1) Select + arm (base state machine; empty selection would not arm).
    send(&mut app, SelectCommand::Set(vec![p]));
    send(&mut app, CommandHotkey::AttackMove);
    assert_eq!(
        *app.world().resource::<PendingTargetOrder>(),
        PendingTargetOrder::AttackMove,
        "A arms the targeting cursor"
    );

    // 2) The game-side click leg (what the real pointer bridge runs on left
    //    click while armed): apply the transition to the resource (armed ->
    //    Idle) and broadcast AttackMove at the XZ point.
    let (next, order) =
        rts_lab::systems::input_bridge::pending_click_transition(PendingTargetOrder::AttackMove, Vec2::new(4.0, 1.5));
    assert_eq!(next, PendingTargetOrder::Idle);
    app.world_mut()
        .resource_mut::<PendingTargetOrder>()
        .set_if_neq(next);
    send(&mut app, UnitCommand { order: order.unwrap(), subjects: vec![p] });
    assert_eq!(*app.world().resource::<PendingTargetOrder>(), PendingTargetOrder::Idle);

    // 3) The peasant walks the route; the grunt at (0, 1.5) is inside its
    //    weapon range on the way -> engagement -> DamageDealt.
    let mut dealt = MessageCursor::<DamageDealt>::default();
    let mut frames = 0;
    loop {
        app.update();
        frames += 1;
        let msgs = app.world().resource::<Messages<DamageDealt>>();
        if dealt.read(msgs).any(|d| d.target == grunt) {
            break;
        }
        assert!(frames < 600, "no engagement within {frames} frames");
    }
    let hp = app.world().get::<Hp>(grunt).unwrap().0;
    assert!(hp < grunt_max, "grunt damaged (hp {hp}/{grunt_max})");
}

// -- ⑧ RL2: barracks trains footman + archer ----------------------------------

/// RL2-⑧: the barracks queue pays and graduates a footman (melee kit: no
/// projectile, Ground targets) and an archer (projectile kit: Some(speed),
/// Ground targets), both selectable + free-moving with vision-free kit.
#[test]
fn barracks_trains_footman_and_archer_with_kits() {
    let mut app = lab_app();
    calm_peasants(&mut app);
    set_gold(&mut app, 300);

    let barracks = units_of(&mut app, "barracks")[0];
    let trains = app.world().get::<ProductionQueue>(barracks).unwrap().trains.clone();
    assert_eq!(trains, vec!["footman".to_string(), "archer".to_string()]);

    send(&mut app, TrainCommand { builder: barracks, unit: "footman".into() });
    send(&mut app, TrainCommand { builder: barracks, unit: "archer".into() });
    assert_eq!(app.world().resource::<Resources>().gold, 300 - 60 - 70, "paid on enqueue");

    for _ in 0..800 {
        app.update(); // footman 6s + archer 7s serialized in one queue = 780 frames
    }
    let footmen = units_of(&mut app, "footman");
    let archers = units_of(&mut app, "archer");
    assert_eq!(footmen.len(), 1, "footman graduated");
    assert_eq!(archers.len(), 1, "archer graduated");
    assert_eq!(app.world().resource::<Population>().used, 6, "two more pop booked (4+2)");

    let fk = app.world().get::<AttackKit>(footmen[0]).unwrap();
    assert_eq!(fk.def.damage, 12.0);
    assert!(fk.def.projectile.is_none(), "footman is melee");
    assert_eq!(fk.def.targets, TargetDomain::Ground);
    let ak = app.world().get::<AttackKit>(archers[0]).unwrap();
    assert!(ak.def.projectile.as_ref().is_some_and(|p| p.speed == 16.0), "archer fires projectiles");
    assert_eq!(ak.def.targets, TargetDomain::Ground);
    assert!(app.world().get::<MoveAgent>(footmen[0]).is_some());
    assert!(app.world().get::<Selectable>(archers[0]).is_some());
}

// -- ⑨ RL2: holy_light (mana, aim, effect chain) -------------------------------

/// RL2-⑨a: aim policy — most wounded ally in the selection wins; nobody
/// wounded (or hero alone) falls back to the caster itself.
#[test]
fn holy_light_aim_picks_most_wounded_ally() {
    let mut app = lab_app();
    calm_peasants(&mut app);
    let hero = units_of(&mut app, "paladin")[0];
    let peasants = units_of(&mut app, "peasant");
    let (hurt, fine) = (peasants[0], peasants[1]);
    app.world_mut().get_mut::<Hp>(hurt).unwrap().0 = 12.0; // most wounded
    app.world_mut().get_mut::<Hp>(fine).unwrap().0 = 44.0; // barely wounded

    // Hero + two wounded allies selected -> the 12-hp one.
    send(&mut app, SelectCommand::Set(vec![hero, fine, hurt]));
    assert_eq!(rts_lab::systems::gameplay::aim_holy_light(app.world(), hero), hurt);
    // Hero alone, full hp -> self.
    send(&mut app, SelectCommand::Set(vec![hero]));
    assert_eq!(rts_lab::systems::gameplay::aim_holy_light(app.world(), hero), hero);
}

/// RL2-⑨b: full cast chain — cast_holy_light (the Q-key path) aims, parks the
/// aim, writes CastSkill; the base process_casts spends 40 mana and fires
/// SkillCast; the game effect dispatch writes HealRequest; resolve_heals
/// lifts the ally's hp. Insufficient mana is refused silently (no SkillCast,
/// no heal, mana untouched).
#[test]
fn holy_light_cast_spends_mana_and_heals_wounded_ally() {
    let mut app = lab_app();
    calm_peasants(&mut app);
    let hero = units_of(&mut app, "paladin")[0];
    let ally = units_of(&mut app, "peasant")[0];
    app.world_mut().get_mut::<Hp>(ally).unwrap().0 = 10.0;

    // Not enough mana: set to 20, cast -> refused.
    app.world_mut().get_mut::<Mana>(hero).unwrap().0 = 20.0;
    send(&mut app, SelectCommand::Set(vec![hero, ally]));
    rts_lab::systems::gameplay::cast_holy_light(app.world_mut());
    // cast_holy_light writes the message; let the pipeline run one frame.
    let mut casts = MessageCursor::<SkillCast>::default();
    {
        app.update();
        let msgs = app.world().resource::<Messages<SkillCast>>();
        assert!(casts.read(msgs).next().is_none(), "mana-starved cast fires nothing");
    }
    let mana = app.world().get::<Mana>(hero).unwrap().0;
    assert!(
        (20.0..21.0).contains(&mana),
        "no 40-mana spend (mana {mana}, only regen drift)"
    );
    assert_eq!(app.world().get::<Hp>(ally).unwrap().0, 10.0, "no heal");

    // Full mana: cast -> -40 mana, ally +60 (capped at max 45).
    app.world_mut().get_mut::<Mana>(hero).unwrap().0 = 100.0;
    send(&mut app, SelectCommand::Set(vec![hero, ally]));
    rts_lab::systems::gameplay::cast_holy_light(app.world_mut());
    app.update(); // CastSkill -> process_casts -> SkillCast -> heal -> resolve
    assert_eq!(app.world().get::<Mana>(hero).unwrap().0, 60.0, "40 mana spent at cast start");
    assert_eq!(app.world().get::<Hp>(ally).unwrap().0, 45.0, "healed to max");
    assert!(app.world().get::<rts_lab::systems::gameplay::HolyLightAim>(hero).is_none(),
        "aim component spent");
    // Cooldown started (6s) — an immediate second cast is refused.
    app.world_mut().get_mut::<Hp>(ally).unwrap().0 = 10.0;
    rts_lab::systems::gameplay::cast_holy_light(app.world_mut());
    app.update();
    let mana = app.world().get::<Mana>(hero).unwrap().0;
    assert!(
        (60.0..61.0).contains(&mana),
        "cooldown blocks: no further spend (mana {mana}, only regen drift)"
    );
    assert_eq!(app.world().get::<Hp>(ally).unwrap().0, 10.0, "no second heal");
}

// -- ⑩ RL2: research ------------------------------------------------------------

/// RL2-⑩: StartResearch pays 100, after 8s (480 frames) the grant lands in
/// TypeModifiers and folds into EVERY footman's EffectiveStats (old hand and
/// a fresh graduate alike): damage_mult 1.15.
#[test]
fn research_attack_upgrade_buffs_old_and_new_footmen() {
    let mut app = lab_app();
    calm_peasants(&mut app);
    set_gold(&mut app, 300);

    // An "old" footman on the field before the research completes.
    let registry = app.world().resource::<UnitRegistry>().clone();
    let old = warbase::prelude::spawn_unit(
        app.world_mut(),
        &registry,
        "footman",
        Vec3::new(-1.0, 0.0, 4.0),
        Allegiance::Defender,
        1.0,
    )
    .unwrap();
    app.update(); // attach_rts_kit day-one kit

    app.world_mut()
        .write_message(ResearchCommands(ResearchCommand::Start {
            id: "attack_upgrade".into(),
        }));
    app.update(); // Tick consumer drains: validate + pay + enqueue
    assert_eq!(app.world().resource::<Resources>().gold, 200, "paid on start");
    assert_eq!(app.world().resource::<ResearchState>().entries.len(), 1);

    for _ in 0..490 {
        app.update(); // 8s = 480 frames + margin
    }
    assert!(app.world().resource::<ResearchState>().entries.is_empty(), "research done");
    let table = app.world().resource::<TypeModifiers>().0.clone();
    assert_eq!(
        table.get(&("footman".to_string(), StatKind::DamageMult)),
        Some(&0.15),
        "grant landed in the type-modifier table"
    );
    assert_eq!(
        table.get(&("archer".to_string(), StatKind::DamageMult)),
        Some(&0.15),
        "archer granted too"
    );
    let old_eff = app.world().get::<EffectiveStats>(old).unwrap();
    assert!((old_eff.damage_mult - 1.15).abs() < 1e-6, "old footman buffed");

    // A fresh graduate inherits the completed research on its first ticks.
    let new = warbase::prelude::spawn_unit(
        app.world_mut(),
        &registry,
        "footman",
        Vec3::new(-1.0, 0.0, 6.0),
        Allegiance::Defender,
        1.0,
    )
    .unwrap();
    for _ in 0..3 {
        app.update();
    }
    let new_eff = app.world().get::<EffectiveStats>(new).unwrap();
    assert!((new_eff.damage_mult - 1.15).abs() < 1e-6, "new footman buffed");
}

// -- ⑪ RL2: items (drop -> pickup -> use; shop buy) -----------------------------

/// RL2-⑪a: kill a grunt -> the map loot table drops a healing_potion at the
/// corpse (chance 1.0) -> the hero walks over it -> stowed in slot 0 ->
/// 1-key use fires the potion_heal SkillCast -> hp restored.
#[test]
fn grunt_drops_potion_hero_picks_up_and_uses_it() {
    let mut app = lab_app();
    calm_peasants(&mut app);
    let hero = units_of(&mut app, "paladin")[0];
    let grunts = units_of(&mut app, "grunt");
    let victim = grunts[0];

    // Kill via the unified damage pipeline (System damage; the last-team
    // standing rule keeps running — two grunts remain).
    app.world_mut().write_message(DamageRequest {
        target: victim,
        source: DamageOrigin::System,
        amount: 999.0,
        attack_type: AttackType::Normal,
        crit: false,
    });
    app.update(); // death -> drop_loot (Cleanup, before corpse cleanup)

    let mut grounds = app.world_mut().query::<(Entity, &GroundItem)>();
    let dropped: Vec<(Entity, String)> =
        grounds.iter(app.world()).map(|(e, g)| (e, g.def.clone())).collect();
    assert_eq!(dropped.len(), 1, "exactly one ground item dropped");
    assert_eq!(dropped[0].1, "healing_potion", "the map loot table rolled");
    let ground_entity = dropped[0].0;

    // Hero walks over the drop: teleport onto it, pickup stows it.
    let drop_pos = app.world().get::<Transform>(ground_entity).unwrap().translation;
    *app.world_mut().get_mut::<Transform>(hero).unwrap() = Transform::from_translation(drop_pos);
    app.update(); // pickup_items (Cleanup)
    let inv = app.world().get::<Inventory>(hero).expect("hero carries an inventory");
    let stack = inv.slots[0].as_ref().expect("potion stowed in slot 0");
    assert_eq!(stack.def, "healing_potion");
    assert_eq!(stack.charges, 1);
    assert_eq!(grounds.iter(app.world()).count(), 0, "ground item gone");

    // Use it: hp 100 -> +80.
    app.world_mut().get_mut::<Hp>(hero).unwrap().0 = 100.0;
    app.world_mut()
        .write_message(ItemCommands(ItemCommand::Use { user: hero, slot: 0 }));
    app.update(); // Use -> SkillCast(potion_heal) -> HealRequest -> resolve_heals
    assert_eq!(app.world().get::<Hp>(hero).unwrap().0, 180.0, "potion healed 80");
    let inv = app.world().get::<Inventory>(hero).unwrap();
    assert!(inv.slots[0].is_none(), "consumable spent its slot");
}

/// RL2-⑪b: the shop channel — the game-side B key range-gates the buy
/// (refused while far); a direct ShopCommand near the shop charges gold and
/// stows the stack (the base consumer does NOT range-check — the gate is the
/// game-side thin wire under test).
#[test]
fn shop_buy_charges_gold_and_stows_potion() {
    let mut app = lab_app();
    calm_peasants(&mut app);
    set_gold(&mut app, 200);
    let hero = units_of(&mut app, "paladin")[0];
    let shop = {
        let mut q = app.world_mut().query_filtered::<Entity, With<Shop>>();
        q.iter(app.world()).next().unwrap()
    };
    let shop_pos = app.world().get::<Transform>(shop).unwrap().translation;

    // Select the hero; B while far from the shop -> game-side gate refuses
    // (headless: inject the pressed key and call the exclusive input fn).
    send(&mut app, SelectCommand::Set(vec![hero]));
    let mut far = ButtonInput::<KeyCode>::default();
    far.press(KeyCode::KeyB);
    app.world_mut().insert_resource(far);
    rts_lab::systems::gameplay::buy_input(app.world_mut());
    // SEAM2c: command queues are messages now — count what the game layer
    // wrote this frame through a fresh cursor (far buy = nothing written).
    let mut far_cursor = bevy::ecs::message::MessageCursor::<ShopCommands>::default();
    app.update();
    {
        let msgs = app.world().resource::<bevy::ecs::message::Messages<ShopCommands>>();
        assert_eq!(far_cursor.read(msgs).count(), 0, "far buy refused game-side");
    }

    // B in range -> Buy pushed -> base consumer charges + stows. The pressed
    // key is reset BEFORE update() so the scheduled input system does not
    // fire a second buy (MinimalPlugins has no input plugin to clear it).
    *app.world_mut().get_mut::<Transform>(hero).unwrap() =
        Transform::from_translation(shop_pos + Vec3::new(1.0, 0.0, 0.0));
    let mut near = ButtonInput::<KeyCode>::default();
    near.press(KeyCode::KeyB);
    app.world_mut().insert_resource(near);
    rts_lab::systems::gameplay::buy_input(app.world_mut());
    app.world_mut().insert_resource(ButtonInput::<KeyCode>::default());
    let mut near_cursor = bevy::ecs::message::MessageCursor::<ShopCommands>::default();
    app.update();
    {
        let msgs = app.world().resource::<bevy::ecs::message::Messages<ShopCommands>>();
        assert_eq!(near_cursor.read(msgs).count(), 1, "near buy written once");
    }
    assert_eq!(app.world().resource::<Resources>().gold, 170, "potion charged 30");
    let inv = app.world().get::<Inventory>(hero).unwrap();
    assert_eq!(inv.slots[0].as_ref().unwrap().def, "healing_potion");
}

// -- ⑫ RL3: fog of war ----------------------------------------------------------

/// RL3-⑫: with the vision section live (cell 1.0) and VisionSource carriers
/// (hall 8.0 / paladin 7.0), the far grunts sit outside team-0 vision and
/// carry `InFog`; walking one into the hall's circle removes the mark the
/// next Cleanup. Own units are never visually hidden (the dim filter is
/// enemy-only, visuals.rs) and combat NEVER reads InFog (D5): the marked
/// grunt still fights (DamageDealt flows) while fogged.
#[test]
fn fog_marks_far_enemies_and_clears_when_seen() {
    let mut app = lab_app();
    calm_peasants(&mut app);
    let grunts = units_of(&mut app, "grunt");
    let far = grunts[0];

    app.update(); // vision grid (Tick) + mark_in_fog (Cleanup)
    assert!(
        app.world().get::<InFog>(far).is_some(),
        "grunt outside all vision circles carries InFog"
    );
    for g in &grunts[1..] {
        assert!(app.world().get::<InFog>(*g).is_some(), "all far grunts fogged");
    }

    // Walk one into the hall's circle (hall (-5,0) r 8.0; (-2,0) is 3.0 away).
    *app.world_mut().get_mut::<Transform>(far).unwrap() = Transform::from_xyz(-2.0, 0.0, 0.0);
    app.update();
    assert!(app.world().get::<InFog>(far).is_none(), "seen grunt loses the mark");
    assert!(app.world().get::<InFog>(grunts[1]).is_some(), "far grunts stay fogged");

    // D5 discipline: fog never gates combat — the (now-visible) grunt still
    // engages a peasant in range; a FOGGED grunt too. Plant a peasant next to
    // a fogged grunt and watch DamageDealt appear.
    let peasants = units_of(&mut app, "peasant");
    let bait = peasants[0];
    let fogged = grunts[1];
    let fogged_pos = app.world().get::<Transform>(fogged).unwrap().translation;
    *app.world_mut().get_mut::<Transform>(bait).unwrap() =
        Transform::from_translation(fogged_pos + Vec3::new(1.0, 0.0, 0.0));
    let mut dealt = MessageCursor::<DamageDealt>::default();
    let mut seen = false;
    for _ in 0..130 {
        app.update(); // grunt interval 1.0s = 60 frames
        let msgs = app.world().resource::<Messages<DamageDealt>>();
        if dealt.read(msgs).any(|d| d.target == bait) {
            seen = true;
            break;
        }
    }
    assert!(seen, "fogged grunt still fires (combat ignores InFog)");
}

// -- ⑬ RL3: minimap data model ---------------------------------------------------

/// RL3-⑬: MinimapModel blips == exactly the units standing in visible cells
/// (self-consistent recomputation via point_visible), the paladin flags
/// is_hero, and the dark mine/grunts leave no blip.
#[test]
fn minimap_blips_match_visible_units() {
    let mut app = lab_app();
    calm_peasants(&mut app);
    for _ in 0..3 {
        app.update(); // vision grid + InFog marking + minimap refresh settle
    }

    let model = app.world().resource::<MinimapModel>().clone();
    assert!(!model.map_bounds.is_empty(), "map bounds from the vision grid geometry");

    // Expected set: every live unit whose cell is visible.
    let grid = app.world().resource::<VisionGrid>().clone();
    let mut q = app.world_mut().query::<(&UnitInstance, &Transform)>();
    let mut expected = 0usize;
    let mut hero_blips = 0;
    let mut peasant_blips = 0;
    for (inst, tf) in q.iter(app.world()) {
        let pos = Vec2::new(tf.translation.x, tf.translation.z);
        if point_visible(&grid, pos) {
            expected += 1;
            match inst.def.as_str() {
                "paladin" => hero_blips += 1,
                "peasant" => peasant_blips += 1,
                _ => {}
            }
        }
    }
    assert!(expected >= 7, "hall, barracks, shop, paladin, 4 peasants visible ({expected})");
    assert_eq!(model.blips.len(), expected, "blips == visible units, nothing more");
    assert_eq!(
        model.blips.iter().filter(|b| b.is_hero).count(),
        hero_blips,
        "paladin is the hero blip"
    );
    // Peasant blips are Defender side; fogged enemies are absent.
    assert!(model
        .blips
        .iter()
        .filter(|b| b.side == Allegiance::Defender)
        .count() >= peasant_blips);
    // Sanity: the mine at (5,0) is outside every circle -> no blip there.
    assert!(!model.blips.iter().any(|b| (b.pos - Vec2::new(5.0, 0.0)).length() < 0.5));
}

// -- ⑭ RL3: record -> file -> replay ----------------------------------------------

/// RL3-⑭: a recorded lab match (100 frames of the autonomous harvest
/// economy, seed 42) lands on disk and reconciles through the lab replay
/// driver: every HASH_INTERVAL checkpoint (incl. frame 0 via the probe)
/// matches; a tampered checkpoint is caught at the first mismatch.
#[test]
fn record_writes_file_and_replay_reconciles() {
    use rts_lab::systems::replay_io as io;
    use warbase::prelude::RecordingLog;

    let map = rts_lab::systems::gameplay::default_map_path();
    let mut app = io::recording_lab(42, &map).expect("arm");
    for _ in 0..100 {
        app.update();
    }
    let log = app
        .world_mut()
        .remove_resource::<RecordingLog>()
        .expect("recording armed at startup")
        .log;
    assert_eq!(log.version, warbase::prelude::INPUT_LOG_VERSION);
    assert_eq!(log.map_hash, io::map_file_hash(&map).unwrap());
    assert_eq!(
        log.hashes.iter().map(|(f, _)| *f).collect::<Vec<_>>(),
        vec![0, 30, 60, 90],
        "checkpoint cadence over 100 frames"
    );
    assert!(log.frames.is_empty(), "rts-lab command surface records no envelopes (hole #10)");

    // Round trip through disk: write (runs convention) -> replay_file.
    let dir = std::env::temp_dir();
    let path = io::write_input_log(&dir, &log).expect("write log");
    let chain = io::replay_file(&path, &map).expect("replay reconciles");
    assert_eq!(
        chain.iter().map(|(f, _)| *f).collect::<Vec<_>>(),
        vec![0, 30, 60, 90],
        "frame-0 probe + every cadence checkpoint reconciled"
    );
    let _ = std::fs::remove_file(&path);

    // Tamper: flip the frame-30 checkpoint -> the driver must catch it.
    let bundle = warbase::bundle::load_bundle(&map).unwrap();
    let mut bad = log.clone();
    bad.hashes[1].1 ^= 0xdead_beef;
    let err = io::replay_lab(&bundle, &bad).expect_err("tampered checkpoint detected");
    assert!(err.contains("frame 30"), "error names the divergent frame: {err}");
}
