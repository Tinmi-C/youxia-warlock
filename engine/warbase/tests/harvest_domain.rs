//! HARV acceptance (round-2 plan §3): the worker loop — load, haul, bank,
//! walk back; depletion refusal; the three-tier upkeep ladder; auto
//! command chaining through `UnitCommand`; zero drift for maps without a
//! harvest section.
//!
//! ① one full trip banks exactly per_trip (+10, no upkeep)
//! ② a 250-capacity mine at 10/trip depletes after 25 trips; the 26th
//!    visit refuses to load (Carrying stays 0)
//! ③ the WC3-style three-tier ladder at 80 population pays 40% (10 -> 4)
//! ④ cargo loaded before a mid-trip depletion still delivers in full
//! ⑤ the haul/return legs chain through public UnitCommand messages
//! ⑥ no harvest section => the domain is a silent no-op

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use std::collections::HashMap;
use warbase::bundle::{assemble, BuildDomainDef, MapBundle, MapIdentity, WorldDef, MAP_FORMAT_VERSION};
use warbase::prelude::*;
use warbase::testing::{count_over_frames, send, test_app};

// -- fixtures ---------------------------------------------------------------

/// App with the harvest config installed (per_trip 10, authored dropoffs,
/// optional upkeep ladder) and a zero-balance economy.
fn harvest_app(dropoffs: &[&str], upkeep: Vec<(u8, f32)>) -> App {
    let mut app = test_app();
    app.insert_resource(HarvestDef {
        dropoffs: dropoffs.iter().map(|s| s.to_string()).collect(),
        upkeep_tiers: upkeep,
        ..Default::default()
    });
    app
}

/// A free worker near the action. arrival_radius 0.5 < GATHER_RANGE so the
/// locomotion brake still lands inside the gather/deliver trigger zone.
fn worker_at(app: &mut App, pos: Vec3) -> Entity {
    app.world_mut()
        .spawn((
            MoveAgent {
                speed: 5.0,
                arrival_radius: 0.5,
                waypoints: vec![],
                velocity: Vec3::ZERO,
                body_radius: 0.3,
                order: None,
            },
            AgentOrderState::Idle,
            Transform::from_translation(pos),
        ))
        .id()
}

fn mine_at(app: &mut App, id: &str, pos: Vec2, capacity: u32) -> Entity {
    app.world_mut()
        .spawn((
            Mine { deposit: id.into(), remaining: capacity },
            Transform::from_translation(pos.extend(0.0)),
        ))
        .id()
}

/// A cargo-accepting building ("hall" is in the authored dropoff list).
fn hall_at(app: &mut App, pos: Vec2) -> Entity {
    app.world_mut()
        .spawn((
            UnitInstance { def: "hall".into(), level: 1 },
            Transform::from_translation(pos.extend(0.0)),
        ))
        .id()
}

fn gold(app: &App) -> u32 {
    app.world().resource::<Resources>().gold
}

fn carrying(app: &App, worker: Entity) -> u32 {
    app.world().get::<Carrying>(worker).map(|c| c.0).unwrap_or(0)
}

/// Orders chained by the cycle since the last drain.
fn drain_commands(app: &mut App, cursor: &mut MessageCursor<UnitCommand>) -> Vec<Order> {
    let msgs = app.world().resource::<Messages<UnitCommand>>();
    cursor.read(msgs).map(|c| c.order.clone()).collect()
}

/// The classic three-tier ladder as map content (floor -> payout factor):
/// pop 0-50 full, 51-79 at 70%, 80+ at 40%.
fn wc3_ladder() -> Vec<(u8, f32)> {
    vec![(0, 1.0), (51, 0.7), (80, 0.4)]
}

/// Standard layout: mine at (10, 0), hall at the origin, worker 1.0 from
/// the mine, already harvesting.
fn one_worker_mining(app: &mut App, mine_capacity: u32) -> Entity {
    let mine = mine_at(app, "gold", Vec2::new(10.0, 0.0), mine_capacity);
    hall_at(app, Vec2::ZERO);
    let w = worker_at(app, Vec3::new(9.0, 0.0, 0.0));
    send(app, UnitCommand { order: Order::Harvest { target: mine }, subjects: vec![w] });
    assert_eq!(carrying(app, w), 10, "gather triggers within GATHER_RANGE on the order frame");
    w
}

// -- ① + ⑤: one full trip banks exactly per_trip, legs auto-chain ----------

#[test]
fn full_trip_banks_exactly_per_trip_and_rearms() {
    let mut app = harvest_app(&["hall"], vec![]);
    let mine = mine_at(&mut app, "gold", Vec2::new(10.0, 0.0), 250);
    let hall = hall_at(&mut app, Vec2::ZERO);
    let w = worker_at(&mut app, Vec3::new(9.0, 0.0, 0.0));
    let mut cmd_cursor = MessageCursor::<UnitCommand>::default();
    let mut econ_cursor = MessageCursor::<EconChanged>::default();
    send(&mut app, UnitCommand { order: Order::Harvest { target: mine }, subjects: vec![w] });

    assert_eq!(app.world().get::<Mine>(mine).unwrap().remaining, 240, "one trip cut");
    assert_eq!(carrying(&app, w), 10, "loaded per_trip = 10");

    // Walk the haul leg; the cycle banks exactly +10 (no upkeep ladder).
    let mut chained: Vec<Order> = Vec::new();
    let mut snaps: Vec<EconChanged> = Vec::new();
    let mut frames = 0usize;
    while gold(&app) == 0 {
        app.update();
        chained.extend(drain_commands(&mut app, &mut cmd_cursor));
        let msgs = app.world().resource::<Messages<EconChanged>>();
        snaps.extend(econ_cursor.read(msgs).copied());
        frames += 1;
        assert!(frames < 600, "haul leg must finish");
    }
    assert_eq!(gold(&app), 10, "exactly +10 banked");
    assert_eq!(carrying(&app, w), 0, "cargo banked");
    assert_eq!(app.world().get::<Mine>(mine).unwrap().remaining, 240, "no second cut mid-leg");

    // ⑤ the haul leg chained through the public command stream...
    assert!(
        chained.iter().any(|o| matches!(o, Order::ReturnCargo { target } if *target == hall)),
        "auto ReturnCargo to the nearest hall: {chained:?}"
    );
    // ...and the delivery chained the return leg back to the mine.
    assert!(
        chained.iter().any(|o| matches!(o, Order::Harvest { target } if *target == mine)),
        "auto return Harvest re-issued: {chained:?}"
    );
    // Two frames later the re-armed order is live on the agent.
    app.update();
    app.update();
    assert_eq!(
        app.world().get::<MoveAgent>(w).unwrap().order,
        Some(Order::Harvest { target: mine })
    );

    // Delivery published an EconChanged snapshot carrying the banked gold.
    assert!(snaps.iter().any(|s| s.gold == 10), "econ snapshot after delivery: {snaps:?}");
}

// -- ②: depletion after capacity/per_trip trips, then refusal ---------------

#[test]
fn depleted_mine_refuses_the_26th_trip() {
    let mut app = harvest_app(&["hall"], vec![]);
    let mine = mine_at(&mut app, "gold", Vec2::new(10.0, 0.0), 250);
    hall_at(&mut app, Vec2::ZERO);
    let w = worker_at(&mut app, Vec3::new(9.0, 0.0, 0.0));
    send(&mut app, UnitCommand { order: Order::Harvest { target: mine }, subjects: vec![w] });

    // 25 trips x 10 empty the 250-capacity mine.
    let mut frames = 0usize;
    while gold(&app) < 250 {
        app.update();
        frames += 1;
        assert!(frames < 12_000, "25 trips must complete (gold {}, frames {frames})", gold(&app));
    }
    assert_eq!(app.world().get::<Mine>(mine).unwrap().remaining, 0, "mine dry");
    assert_eq!(gold(&app), 250, "25 full-price trips banked 250");

    // The loop re-armed onto the depleted mine; let the worker walk back
    // and stand there refusing to load.
    for _ in 0..400 {
        app.update();
    }
    let agent_pos = app.world().get::<Transform>(w).unwrap().translation;
    let mine_pos = app.world().get::<Transform>(mine).unwrap().translation;
    assert_eq!(
        app.world().get::<MoveAgent>(w).unwrap().order,
        Some(Order::Harvest { target: mine })
    );
    assert!(
        agent_pos.distance(mine_pos) <= GATHER_RANGE + 0.1,
        "worker stands at the dry mine (dist {})",
        agent_pos.distance(mine_pos)
    );
    assert_eq!(carrying(&app, w), 0, "26th trip refuses: Carrying stays 0");
    assert_eq!(gold(&app), 250, "nothing more banks from a dry mine");
}

// -- ③: three-tier upkeep, 80 population pays 40% ----------------------------

#[test]
fn three_tier_upkeep_80_pop_banks_four() {
    let tiers = wc3_ladder();
    // Pure ladder checks (f32 tolerance per spec).
    assert!((upkeep_factor(&tiers, 30) - 1.0).abs() < 1e-4, "low pop: full payout");
    assert!((upkeep_factor(&tiers, 50) - 1.0).abs() < 1e-4, "tier boundary 50: still full");
    assert!((upkeep_factor(&tiers, 60) - 0.7).abs() < 1e-4, "mid tier");
    assert!((upkeep_factor(&tiers, 80) - 0.4).abs() < 1e-4, "80 used sits in the 0.4 tier");
    assert!((upkeep_factor(&[], 99) - 1.0).abs() < 1e-4, "empty ladder = no upkeep");

    // End-to-end: 80 population banks exactly round(10 * 0.4) = 4.
    let mut app = harvest_app(&["hall"], tiers);
    app.insert_resource(Population { used: 80, cap: 100 });
    let w = one_worker_mining(&mut app, 250);
    let mut frames = 0usize;
    while gold(&app) == 0 {
        app.update();
        frames += 1;
        assert!(frames < 600, "trip must complete");
    }
    assert_eq!(gold(&app), 4, "10 cargo x 0.4 = exactly 4 under 80 pop");
    assert_eq!(carrying(&app, w), 0);
}

// -- ④: cargo loaded before a mid-trip depletion delivers in full -----------

#[test]
fn cargo_survives_mid_trip_depletion() {
    let mut app = harvest_app(&["hall"], vec![]);
    let mine = mine_at(&mut app, "gold", Vec2::new(10.0, 0.0), 10); // one trip empties it
    hall_at(&mut app, Vec2::ZERO);
    let w = worker_at(&mut app, Vec3::new(9.0, 0.0, 0.0));
    send(&mut app, UnitCommand { order: Order::Harvest { target: mine }, subjects: vec![w] });

    assert_eq!(app.world().get::<Mine>(mine).unwrap().remaining, 0, "dry after the load");
    assert_eq!(carrying(&app, w), 10, "cargo was loaded before the depletion");

    let mut frames = 0usize;
    while gold(&app) == 0 {
        app.update();
        frames += 1;
        assert!(frames < 600, "delivery must complete");
    }
    assert_eq!(gold(&app), 10, "the carried 10 banks in full despite the dry mine");
}

// -- ⑥: zero drift without a harvest section --------------------------------

#[test]
fn no_harvest_section_zero_drift() {
    let mut app = test_app(); // plugin defaults: no deposits, no upkeep
    assert_eq!(app.world().resource::<HarvestDef>().per_trip, 10, "neutral default");
    let w = worker_at(&mut app, Vec3::ZERO);
    send(&mut app, UnitCommand { order: Order::Move { to: Vec3::new(5.0, 0.0, 0.0) }, subjects: vec![w] });
    app.update();
    app.update(); // flush the Move command out of the double buffer

    assert_eq!(count_over_frames::<UnitCommand>(&mut app, 300), 0, "no auto commands");
    assert_eq!(count_over_frames::<EconChanged>(&mut app, 300), 0, "no econ snapshots");
    assert_eq!(gold(&app), 0);
    assert!(app.world().get::<Carrying>(w).is_none(), "no cargo ever inserted");
    let mut q = app.world_mut().query::<&Mine>();
    assert_eq!(q.iter(app.world()).count(), 0, "no mine entities");
}

// -- apply: assemble lands the config and spawns mine entities ---------------

fn unit_def(id: &str) -> UnitDef {
    UnitDef {
        hero_stats: None,
        pop_cost: 1,
        leak: 1,
        id: id.into(),
        attrs: BaseAttributes { hp: 10.0, move_speed: 1.0, body_radius: 0.3 },
        attack: None,
        armor_type: ArmorType::Light,
        cost_gold: 0,
        cost_wood: 0,
        bounty_gold: 0,
        bounty_wood: 0,
        grant_xp: 0,
        traits: vec![],
        auras: vec![],
        hero: false,
        skills: vec![],
        upgrade_to: None,
        heal: None,
        mana: None,
        skill_levels: vec![],
    }
}

fn bundle_with(harvest: HarvestDef) -> MapBundle {
    MapBundle {
        identity: MapIdentity { id: "harv".into(), version: MAP_FORMAT_VERSION },
        world: WorldDef {
            paths: vec![],
            build: BuildDomainDef::FreeGrid { cell: 1.0, cols: 8, rows: 8 },
            blocked: vec![],
        },
        units: HashMap::from([("grunt".into(), unit_def("grunt"))]),
        waves: Default::default(),
        rules: MatchRules::default(),
        econ_start: EconStart { gold: 0, wood: 0, pop_cap: 10 },
        attack_armor: Default::default(),
        harvest,
        camps: Default::default(),
        daynight: Default::default(),
        terrain: Default::default(),
        econ_rules: Default::default(),
        nav: Default::default(),
        vision: Default::default(),
        items: Default::default(),
        production: Default::default(),
        teams: Default::default(),
        starting_forces: vec![],
    }
}

#[test]
fn assemble_lands_config_and_mine_entities() {
    // Empty section: zero mine entities, neutral resource (zero drift).
    let mut app = test_app();
    assemble(&mut app.world_mut(), &bundle_with(HarvestDef::default()));
    let mut q = app.world_mut().query::<&Mine>();
    assert_eq!(q.iter(app.world()).count(), 0, "no deposits, no entities");

    // Authored section: entities + the live config resource.
    let mut app = test_app();
    let section = HarvestDef {
        deposits: vec![MineDef { id: "gold".into(), pos: [3.0, 4.0], capacity: 1500 }],
        per_trip: 8,
        dropoffs: vec!["hall".into()],
        upkeep_tiers: vec![(51, 0.7)],
    };
    assemble(&mut app.world_mut(), &bundle_with(section));
    let cfg = app.world().resource::<HarvestDef>();
    assert_eq!(cfg.per_trip, 8);
    assert_eq!(cfg.dropoffs, vec!["hall".to_string()]);
    assert_eq!(cfg.upkeep_tiers, vec![(51, 0.7)]);
    let mut q = app.world_mut().query::<(&Mine, &Transform)>();
    let mut found = q.iter(app.world());
    let (mine, tf) = found.next().unwrap();
    assert!(found.next().is_none(), "exactly one mine entity");
    assert_eq!((mine.deposit.as_str(), mine.remaining), ("gold", 1500));
    assert_eq!(tf.translation, Vec3::new(3.0, 0.0, 4.0), "hole-1 XZ convention");
}
