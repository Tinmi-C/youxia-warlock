//! BLDG acceptance (round-2 plan §3): the worker-driven construction loop —
//! charge-on-order timing, frame-exact progress, multi-worker linear
//! acceleration, cancel refunds / stop-freeze semantics, the town tier
//! chain T1 -> T3 with unlock grants, and zero drift without build content.
//!
//! ① single worker, 6s site: frame 359 under construction, frame 360 done
//! ② two workers with builders_accelerate: 3s (180 frames), ONE charge
//! ②b builders_accelerate=false: extra workers do not stack
//! ③ short gold/wood: 0 charge, 0 site, order dropped
//! ④ CancelBuild: 100% refund, site gone; worker Stop: frozen, no refund
//! ⑤ town T1 -> T2 charges & unlocks town_unlocks[1]; T2 -> T3; T3 capped;
//!    broke / mid-upgrade rejections
//! ⑥ no build content: stray orders rejected, nothing attaches, no noise
//! ⑦ a distant worker walks first — the site is frozen until arrival

use bevy::prelude::*;
use std::collections::HashMap;
use warbase::prelude::*;
use warbase::testing::{count_over_frames, send, test_app};

// -- fixtures ---------------------------------------------------------------

fn unit_def(id: &str) -> UnitDef {
    UnitDef {
        leak: 1,
        id: id.into(),
        attrs: BaseAttributes { hp: 10.0, move_speed: 0.0, body_radius: 0.3 },
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
        // HERO-line field (post-BLDG-branch merge): no attributes here.
        hero_stats: None,
        skills: vec![],
        upgrade_to: None,
        heal: None,
        mana: None,
        skill_levels: vec![],
    }
}

/// App with the BLDG config installed: farm (6s, 30g/10w, accelerating),
/// tower (6s, no acceleration), hall as town hall; town upgrades
/// T1->T2 (100g/50w, 1s) and T2->T3 (150g/75w, 1s); town_unlocks grants
/// "keep" at T2 and "castle" at T3. Wallet 400g/200w.
fn bldg_app() -> App {
    let mut app = test_app();
    let mut defs = HashMap::new();
    defs.insert("farm".to_string(), unit_def("farm"));
    defs.insert("tower".to_string(), unit_def("tower"));
    defs.insert("hall".to_string(), unit_def("hall"));
    app.insert_resource(UnitRegistry { defs });
    app.insert_resource(Resources { gold: 400, wood: 200 });
    app.insert_resource(BuildRtsDef {
        orders: HashMap::from([
            (
                "farm".to_string(),
                BuildOrderDef {
                    time: 6.0,
                    cost_gold: 30,
                    cost_wood: 10,
                    builders_accelerate: true,
                },
            ),
            (
                "tower".to_string(),
                BuildOrderDef {
                    time: 6.0,
                    cost_gold: 30,
                    cost_wood: 10,
                    builders_accelerate: false,
                },
            ),
        ]),
        town_halls: vec!["hall".to_string()],
        town_upgrades: vec![
            TownUpgradeDef { cost_gold: 100, cost_wood: 50, time: 1.0 },
            TownUpgradeDef { cost_gold: 150, cost_wood: 75, time: 1.0 },
        ],
        town_unlocks: vec![
            vec![],
            vec!["keep".to_string()],
            vec!["castle".to_string()],
        ],
    });
    // Warm-up update: absorbs the zero-delta first frame so every `send`
    // below is a real 1/60 frame (the prd_domain fixture convention).
    app.update();
    app
}

/// A free worker (speed 5, brake inside the 1.5 build range).
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

fn hall_at(app: &mut App, pos: Vec3) -> Entity {
    app.world_mut()
        .spawn((
            UnitInstance { def: "hall".into(), level: 1 },
            Transform::from_translation(pos),
        ))
        .id()
}

fn gold(app: &App) -> u32 {
    app.world().resource::<Resources>().gold
}

fn wood(app: &App) -> u32 {
    app.world().resource::<Resources>().wood
}

fn site_count(app: &mut App) -> usize {
    let mut q = app.world_mut().query::<&UnderConstruction>();
    q.iter(app.world()).count()
}

/// The single live site as (entity, builders, remaining, total).
fn site_state(app: &mut App) -> Option<(Entity, u32, f32, f32)> {
    let mut q = app.world_mut().query::<(Entity, &UnderConstruction)>();
    q.iter(app.world())
        .next()
        .map(|(e, s)| (e, s.builders, s.remaining, s.total))
}

fn units_with_def(app: &mut App, def: &str) -> Vec<Entity> {
    let mut q = app.world_mut().query::<(Entity, &UnitInstance)>();
    q.iter(app.world()).filter(|(_, i)| i.def == def).map(|(e, _)| e).collect()
}

fn build_order(what: &str, at: Vec3, subjects: Vec<Entity>) -> UnitCommand {
    UnitCommand { order: Order::Build { what: what.into(), at }, subjects }
}

// -- ①: single worker, 6s site, frame-exact ----------------------------------

#[test]
fn single_worker_six_second_site_completes_on_frame_360() {
    let mut app = bldg_app();
    let at = Vec3::new(10.0, 0.0, 0.0);
    let w = worker_at(&mut app, Vec3::new(9.0, 0.0, 0.0)); // 1.0 away: in range now
    send(&mut app, build_order("farm", at, vec![w])); // frame 1: charge + site

    assert_eq!((gold(&app), wood(&app)), (370, 190), "charged on the order frame");
    let (_, builders, remaining, total) = site_state(&mut app).expect("site from the order frame");
    assert_eq!((builders, total), (1, 6.0), "one hammerer, full duration left");
    assert!(remaining < 6.0, "progress ticked on frame 1 (remaining {remaining})");

    for _ in 0..358 {
        app.update(); // frames 2..=359
    }
    let (_, builders, remaining, _) = site_state(&mut app).expect("frame 359: still under construction");
    assert_eq!(builders, 1);
    assert!(remaining > 0.0, "frame 359 incomplete (remaining {remaining})");

    app.update(); // frame 360
    assert_eq!(site_count(&mut app), 0, "frame 360: site completed");
    let farms = units_with_def(&mut app, "farm");
    assert_eq!(farms.len(), 1, "the building is a real unit now");
    assert_eq!(app.world().get::<Transform>(farms[0]).unwrap().translation, at);
    assert_eq!((gold(&app), wood(&app)), (370, 190), "no further charges");
    assert!(app.world().get::<MoveAgent>(w).unwrap().order.is_none(), "order spent");
    assert!(app.world().get::<BuildAssign>(w).is_none(), "link removed");
}

// -- ②: two workers accelerate linearly, one charge ---------------------------

#[test]
fn two_workers_with_accelerate_finish_in_half_time() {
    let mut app = bldg_app();
    let at = Vec3::new(10.0, 0.0, 0.0);
    let w1 = worker_at(&mut app, Vec3::new(9.0, 0.0, 0.0));
    let w2 = worker_at(&mut app, Vec3::new(10.8, 0.0, 0.0));
    send(&mut app, build_order("farm", at, vec![w1, w2])); // frame 1

    assert_eq!((gold(&app), wood(&app)), (370, 190), "ONE charge for the shared site");
    let (_, builders, _, total) = site_state(&mut app).expect("site exists");
    assert_eq!((builders, total), (2, 6.0), "both workers hammer immediately");

    for _ in 0..178 {
        app.update(); // frames 2..=179
    }
    assert!(site_state(&mut app).is_some(), "frame 179: 6s / 2 builders is not done yet");

    app.update(); // frame 180: 6s at 2x rate = 3s = 180 frames
    assert_eq!(site_count(&mut app), 0, "frame 180: completed");
    assert_eq!(units_with_def(&mut app, "farm").len(), 1);
    assert!(app.world().get::<MoveAgent>(w1).unwrap().order.is_none());
    assert!(app.world().get::<MoveAgent>(w2).unwrap().order.is_none());
}

// -- ②b: builders_accelerate=false ignores extra workers ----------------------

#[test]
fn no_accelerate_site_ignores_extra_workers() {
    let mut app = bldg_app();
    let at = Vec3::new(10.0, 0.0, 0.0);
    let w1 = worker_at(&mut app, Vec3::new(9.0, 0.0, 0.0));
    let w2 = worker_at(&mut app, Vec3::new(10.8, 0.0, 0.0));
    send(&mut app, build_order("tower", at, vec![w1, w2])); // frame 1

    let (_, builders, _, _) = site_state(&mut app).expect("site exists");
    assert_eq!(builders, 2, "both stand at the site");
    for _ in 0..178 {
        app.update(); // frames 2..=179
    }
    assert!(
        site_state(&mut app).is_some_and(|(_, _, remaining, _)| remaining > 0.0),
        "frame 179: undead-style site does not stack (would be done at 2x)"
    );

    // Single rate: done exactly on frame 360 like the lone-worker case.
    for _ in 0..180 {
        app.update(); // frames 180..=359
    }
    assert!(site_state(&mut app).is_some(), "frame 359: still building");
    app.update(); // frame 360
    assert_eq!(site_count(&mut app), 0, "frame 360: single-rate completion");
    assert_eq!(units_with_def(&mut app, "tower").len(), 1);
}

// -- ③: short funds reject with zero charge and zero site ---------------------

#[test]
fn short_funds_reject_with_zero_charge_and_zero_site() {
    let mut app = bldg_app();
    let at = Vec3::new(10.0, 0.0, 0.0);

    // Gold short (farm needs 30).
    app.insert_resource(Resources { gold: 29, wood: 100 });
    let w1 = worker_at(&mut app, Vec3::new(9.0, 0.0, 0.0));
    send(&mut app, build_order("farm", at, vec![w1]));
    assert_eq!((gold(&app), wood(&app)), (29, 100), "0 charge on gold-short reject");
    assert_eq!(site_count(&mut app), 0, "0 site");
    assert!(app.world().get::<MoveAgent>(w1).unwrap().order.is_none(), "order dropped");

    // Wood short (farm needs 10).
    app.insert_resource(Resources { gold: 400, wood: 9 });
    let w2 = worker_at(&mut app, Vec3::new(9.0, 0.0, 0.0));
    send(&mut app, build_order("farm", at, vec![w2]));
    assert_eq!((gold(&app), wood(&app)), (400, 9), "0 charge on wood-short reject");
    assert_eq!(site_count(&mut app), 0, "0 site");
    assert!(app.world().get::<MoveAgent>(w2).unwrap().order.is_none(), "order dropped");

    assert_eq!(count_over_frames::<EconChanged>(&mut app, 30), 0, "no econ snapshots");
}

// -- ④: explicit cancel refunds; worker stop freezes --------------------------

#[test]
fn cancel_build_refunds_in_full_and_removes_site() {
    let mut app = bldg_app();
    let at = Vec3::new(10.0, 0.0, 0.0);
    let w = worker_at(&mut app, Vec3::new(9.0, 0.0, 0.0));
    send(&mut app, build_order("farm", at, vec![w])); // frame 1: charged 370/190
    let (site, _, _, _) = site_state(&mut app).expect("site exists");

    send(&mut app, CancelBuild { site }); // frame 2
    assert_eq!((gold(&app), wood(&app)), (400, 200), "100% refund");
    assert_eq!(site_count(&mut app), 0, "site removed");
    assert!(app.world().get::<UnderConstruction>(site).is_none());
    assert!(app.world().get::<MoveAgent>(w).unwrap().order.is_none(), "builder freed");
    assert!(app.world().get::<BuildAssign>(w).is_none(), "link removed");

    for _ in 0..400 {
        app.update(); // well past any 6s window
    }
    assert_eq!(units_with_def(&mut app, "farm").len(), 0, "cancelled site never completes");
}

#[test]
fn worker_stop_freezes_site_without_refund() {
    let mut app = bldg_app();
    let at = Vec3::new(10.0, 0.0, 0.0);
    let w = worker_at(&mut app, Vec3::new(9.0, 0.0, 0.0));
    send(&mut app, build_order("farm", at, vec![w])); // frame 1: charged, one tick
    send(&mut app, UnitCommand { order: Order::Stop, subjects: vec![w] }); // frame 2

    assert_eq!((gold(&app), wood(&app)), (370, 190), "Stop does NOT refund");
    let (site, builders, remaining, _) = site_state(&mut app).expect("site stays");
    assert_eq!(builders, 0, "no hammerer");
    let frozen = remaining;

    for _ in 0..400 {
        app.update();
    }
    let (_, builders, remaining, _) = site_state(&mut app).expect("site still frozen in place");
    assert_eq!(builders, 0);
    assert_eq!(remaining, frozen, "remaining frozen bit-exact");
    assert_eq!((gold(&app), wood(&app)), (370, 190), "charge kept");
    assert_eq!(units_with_def(&mut app, "farm").len(), 0);
    let _ = site;
}

// -- ⑤: town tier chain -------------------------------------------------------

#[test]
fn town_t1_to_t2_charges_and_unlocks_builds() {
    let mut app = bldg_app();
    let hall = hall_at(&mut app, Vec3::ZERO);
    app.update(); // attach_town_tiers
    assert_eq!(app.world().get::<TownTier>(hall).unwrap().0, 1, "fresh hall is T1");

    send(&mut app, UpgradeTown { hall }); // frame 1: charge 100g/50w + 1s timer
    assert_eq!((gold(&app), wood(&app)), (300, 150), "upgrade charged");
    assert!(app.world().get::<TownUpgrading>(hall).is_some(), "timer running");

    for _ in 0..58 {
        app.update(); // frames 2..=59
    }
    assert!(
        app.world().get::<TownUpgrading>(hall).is_some(),
        "frame 59: 1s upgrade still running"
    );
    app.update(); // frame 60
    assert!(app.world().get::<TownUpgrading>(hall).is_none(), "frame 60: done");
    assert_eq!(app.world().get::<TownTier>(hall).unwrap().0, 2, "tier bumped");
    let unlocks = app.world().resource::<Unlocks>().clone();
    assert!(is_build_unlocked(&unlocks, "keep"), "town_unlocks[1] granted into Unlocks");
    assert!(!is_build_unlocked(&unlocks, "castle"), "town_unlocks[2] held back");
    assert_eq!((gold(&app), wood(&app)), (300, 150));

    // T2 -> T3 completes the chain and grants the T3 set.
    send(&mut app, UpgradeTown { hall });
    assert_eq!((gold(&app), wood(&app)), (150, 75), "second tier charged");
    for _ in 0..60 {
        app.update();
    }
    assert_eq!(app.world().get::<TownTier>(hall).unwrap().0, 3, "T3 reached");
    let unlocks = app.world().resource::<Unlocks>().clone();
    assert!(is_build_unlocked(&unlocks, "castle"), "town_unlocks[2] granted at T3");

    // T3 is the cap: further upgrades are rejected without a charge.
    send(&mut app, UpgradeTown { hall });
    assert_eq!((gold(&app), wood(&app)), (150, 75), "capped tier: no charge");
    assert!(app.world().get::<TownUpgrading>(hall).is_none());
    assert_eq!(app.world().get::<TownTier>(hall).unwrap().0, 3);
}

#[test]
fn town_upgrade_rejections() {
    // Broke hall: no charge, no timer, tier stays.
    let mut app = bldg_app();
    app.insert_resource(Resources { gold: 50, wood: 200 });
    let hall = hall_at(&mut app, Vec3::ZERO);
    app.update();
    send(&mut app, UpgradeTown { hall });
    assert_eq!(gold(&app), 50, "no charge on insufficient funds");
    assert!(app.world().get::<TownUpgrading>(hall).is_none());
    assert_eq!(app.world().get::<TownTier>(hall).unwrap().0, 1);

    // Mid-upgrade: a second command is ignored (no double charge).
    let mut app = bldg_app();
    let hall = hall_at(&mut app, Vec3::ZERO);
    app.update();
    send(&mut app, UpgradeTown { hall }); // starts the 1s timer
    send(&mut app, UpgradeTown { hall }); // ignored
    assert_eq!((gold(&app), wood(&app)), (300, 150), "charged exactly once");
    assert!(app.world().get::<TownUpgrading>(hall).is_some());
    app.update();
    assert!(app.world().get::<TownUpgrading>(hall).is_some(), "timer not restarted");

    // A non-hall entity is skipped silently.
    let rando = worker_at(&mut app, Vec3::ZERO);
    send(&mut app, UpgradeTown { hall: rando });
    assert_eq!(gold(&app), 300, "no charge for a non-hall");
    assert!(app.world().get::<TownUpgrading>(rando).is_none());
}

// -- ⑥: zero drift without build content --------------------------------------

#[test]
fn no_build_content_zero_drift() {
    let mut app = test_app(); // plugin defaults: empty BuildRtsDef
    assert_eq!(app.world().resource::<BuildRtsDef>().orders.len(), 0);
    assert_eq!(app.world().resource::<BuildRtsDef>().town_halls.len(), 0);

    // A stray Build order against the empty table is dropped silently.
    let w = worker_at(&mut app, Vec3::ZERO);
    send(&mut app, build_order("farm", Vec3::new(3.0, 0.0, 0.0), vec![w]));
    assert!(app.world().get::<MoveAgent>(w).unwrap().order.is_none(), "rejected: no def");
    assert_eq!(site_count(&mut app), 0);
    assert_eq!((gold(&app), wood(&app)), (0, 0), "wallet untouched");

    // No hall tiers attach; stray cancel/upgrade commands are silent.
    let hall = hall_at(&mut app, Vec3::ZERO);
    app.update();
    assert!(app.world().get::<TownTier>(hall).is_none(), "no hall list, no tier");
    app.world_mut().write_message(CancelBuild { site: hall });
    app.world_mut().write_message(UpgradeTown { hall });
    app.update();
    assert!(app.world().get::<UnitInstance>(hall).is_some(), "non-site untouched");
    assert!(app.world().get::<TownTier>(hall).is_none());

    // No econ snapshots, no auto commands, nothing spawns.
    assert_eq!(count_over_frames::<EconChanged>(&mut app, 60), 0);
    assert_eq!(count_over_frames::<UnitCommand>(&mut app, 60), 0);
    assert_eq!(site_count(&mut app), 0);
    assert_eq!((gold(&app), wood(&app)), (0, 0));
}

// -- ⑦: a distant worker walks first — frozen until arrival -------------------

#[test]
fn distant_worker_walks_then_builds() {
    let mut app = bldg_app();
    let at = Vec3::new(8.0, 0.0, 0.0);
    let w = worker_at(&mut app, Vec3::ZERO); // 8.0 away, speed 5
    send(&mut app, build_order("farm", at, vec![w])); // frame 1

    let (_, builders, remaining, total) = site_state(&mut app).expect("site from the order frame");
    assert_eq!((builders, remaining, total), (0, 6.0, 6.0), "no hammerer: frozen, untouched");

    for _ in 0..40 {
        app.update(); // still mid-walk (needs ~78 frames to cross 1.5)
    }
    let (_, builders, remaining, _) = site_state(&mut app).expect("site persists");
    assert_eq!((builders, remaining), (0, 6.0), "frozen until a builder arrives");
    assert!(
        app.world().get::<Transform>(w).unwrap().translation.x > 3.0,
        "the worker is actually walking"
    );

    let mut frames = 0usize;
    while units_with_def(&mut app, "farm").is_empty() {
        app.update();
        frames += 1;
        assert!(frames < 600, "walk (8/5s) + build (6s) must fit in 600 frames");
    }
    let farms = units_with_def(&mut app, "farm");
    assert_eq!(farms.len(), 1);
    assert_eq!(app.world().get::<Transform>(farms[0]).unwrap().translation, at);
    assert_eq!(site_count(&mut app), 0);
    assert!(app.world().get::<MoveAgent>(w).unwrap().order.is_none());
}
