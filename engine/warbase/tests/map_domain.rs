//! MAP acceptance (round-2 plan §3): neutral guard camps + the day/night
//! cycle. Numbering matches the acceptance list:
//! ① an intruder inside the camp radius flips the whole camp to
//!   AttackTarget within 2 frames (order + CurrentTarget)
//! ② pursuit past radius x 1.5 leashes: Move back to the post within 2
//!   frames of crossing, HoldPosition again on arrival (AgentOrder asserts)
//! ③ a wiped camp respawns EXACTLY N guards at their posts after
//!   respawn_secs, never more
//! ④ a chance-1.0 camp drop lands a GroundItem within 0.3 of the corpse
//!   (and chance 0.0 lands nothing)
//! ⑤ day 300 frames / night 300 frames: one DayPhaseChanged at frame 300,
//!   the next exactly 300 frames later; a source spawned during Night sees
//!   its vision radius x factor (grid-visible-range assertion)
//! ⑥ default sections (no camps, 0/0/1.0 day/night) = zero drift
//! plus a serde pin: `guards` defaults empty, `drops` carries chances.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use std::collections::HashMap;
use warbase::bundle::{assemble, validate, BuildDomainDef, MapBundle, MapIdentity, WorldDef};
use warbase::prelude::*;
use warbase::testing::test_app;

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
        skills: vec![],
        skill_levels: vec![],
        upgrade_to: None,
        heal: None,
        mana: None,
        hero_stats: None,
        pop_cost: 1,
    }
}

/// The camp guard: fast (12/s) melee brain (range 3.5, one-point pokes on a
/// 10s interval so test dummies survive the fight).
fn guard_def() -> UnitDef {
    let mut d = unit_def("gnoll");
    d.attrs.move_speed = 12.0;
    d.attack = Some(AttackDef {
        damage: 1.0,
        interval: 10.0,
        range: 3.5,
        attack_type: AttackType::Normal,
        targets: TargetDomain::Ground,
        projectile: None,
        windup_secs: 0.0,
        backswing_secs: 0.0,
    });
    d
}

fn camp_def(guards: &[&str], radius: f32, respawn_secs: f32, drops: Vec<(String, f32)>) -> CampDef {
    CampDef {
        id: "c1".into(),
        pos: [0.0, 0.0],
        radius,
        respawn_secs,
        drops,
        guards: guards.iter().map(|s| (*s).to_string()).collect(),
    }
}

/// Assembled map app: a guard def in the registry, the given camps + day/night
/// sections, seeded RNG, one warm-up update (absorbs the zero-delta first
/// frame — the bldg/prd fixture convention).
fn map_app(camps: CampsDef, daynight: DayNightDef) -> App {
    let mut app = test_app();
    app.insert_resource(MatchRng::seeded(42));
    let bundle = MapBundle {
        identity: MapIdentity { id: "map_domain".into(), version: warbase::bundle::MAP_FORMAT_VERSION },
        world: WorldDef {
            paths: vec![],
            build: BuildDomainDef::FreeGrid { cell: 1.0, cols: 16, rows: 16 },
            blocked: vec![],
        },
        units: HashMap::from([("gnoll".to_string(), guard_def())]),
        waves: Default::default(),
        rules: MatchRules::default(),
        econ_start: EconStart { gold: 100, wood: 0, pop_cap: 10 },
        econ_rules: Default::default(),
        attack_armor: Default::default(),
        nav: Default::default(),
        vision: Default::default(),
        items: Default::default(),
        production: Default::default(),
        teams: Default::default(),
        starting_forces: vec![],
        harvest: Default::default(),
        camps,
        daynight,
        terrain: Default::default(),
    };
    validate(&bundle).unwrap();
    assemble(app.world_mut(), &bundle);
    app.update(); // warm-up
    app
}

fn one_camp(guards: &[&str], radius: f32, respawn_secs: f32, drops: Vec<(String, f32)>) -> CampsDef {
    CampsDef { camps: vec![camp_def(guards, radius, respawn_secs, drops)] }
}

/// Live camp guards, entity-index ascending (D6 order).
fn camp_guards(world: &mut World) -> Vec<Entity> {
    let mut q = world.query_filtered::<Entity, With<CampGuard>>();
    let mut ids: Vec<Entity> = q.iter(world).collect();
    ids.sort_by_key(|e| e.index_u32());
    ids
}

fn order_of(world: &World, e: Entity) -> Option<Order> {
    world.get::<MoveAgent>(e).unwrap().order.clone()
}

fn pos_of(world: &World, e: Entity) -> Vec3 {
    world.get::<Transform>(e).unwrap().translation
}

/// A hostile intruder dummy (huge hp so guard pokes never kill it).
fn intruder_at(app: &mut App, x: f32, z: f32) -> Entity {
    app.world_mut()
        .spawn((Transform::from_xyz(x, 0.0, z), Hp(1_000_000.0), Allegiance::Defender))
        .id()
}

// -- ① intruder flips the whole camp to AttackTarget -------------------------

#[test]
fn intruder_in_radius_flips_whole_camp_to_attack_target() {
    let mut app = map_app(
        one_camp(&["gnoll", "gnoll"], 5.0, 60.0, vec![]),
        DayNightDef::default(),
    );
    let guards = camp_guards(app.world_mut());
    assert_eq!(guards.len(), 2, "both authored guards spawned");
    for g in &guards {
        assert_eq!(order_of(app.world(), *g), Some(Order::HoldPosition), "spawned holding");
    }

    let enemy = intruder_at(&mut app, 3.0, 0.0);
    app.update(); // camp_ai (Tick) -> UnitCommand -> tick_orders same frame; pick_targets (Intent)

    // Acceptance ①: within 2 frames every guard holds AttackTarget and the
    // CurrentTarget is the intruder.
    for g in &guards {
        assert_eq!(
            order_of(app.world(), *g),
            Some(Order::AttackTarget { target: enemy }),
            "guard {g:?} pursues the intruder"
        );
        assert_eq!(
            app.world().get::<CurrentTarget>(*g).unwrap().0,
            Some(enemy),
            "guard {g:?} holds CurrentTarget"
        );
    }
    app.update(); // no flapping: still attacking
    for g in &guards {
        assert!(matches!(order_of(app.world(), *g), Some(Order::AttackTarget { .. })));
    }
}

// -- ② leash: past radius x 1.5 walk home, arrive -> Hold --------------------

#[test]
fn pursuit_past_leash_returns_to_post_and_reholds() {
    let mut app = map_app(
        one_camp(&["gnoll"], 4.0, 60.0, vec![]), // leash = 4.0 * 1.5 = 6.0
        DayNightDef::default(),
    );
    let guards = camp_guards(app.world_mut());
    assert_eq!(guards.len(), 1);
    let guard = guards[0];
    let post = app.world().get::<CampGuard>(guard).unwrap().post;
    assert_eq!(post, Vec3::ZERO, "lone guard's post is the camp center");

    // The enemy sits OUTSIDE the camp radius (not a fresh intruder) but is
    // alive: an AttackTarget issued on it drives a real chase (test ① pinned
    // the camp's own aggro path; this pins the leash in isolation). At 11.0
    // the guard would only stop to engage at 11.0 - 3.5 = 7.5 from the
    // center — past the 6.0 leash, so the leash must fire mid-chase.
    let enemy = intruder_at(&mut app, 11.0, 0.0);
    app.world_mut().write_message(UnitCommand {
        order: Order::AttackTarget { target: enemy },
        subjects: vec![guard],
    });
    app.update();
    assert!(matches!(order_of(app.world(), guard), Some(Order::AttackTarget { .. })));
    assert_eq!(
        app.world().get::<AgentOrderState>(guard).unwrap(),
        &AgentOrderState::Chasing,
        "movement chases under AttackTarget"
    );

    // Run until the leash flips the order to Move-home; the flip must land
    // while the guard is still within one chase step of the leash line.
    let mut move_frame = None;
    for f in 0..200 {
        app.update();
        if order_of(app.world(), guard) == Some(Order::Move { to: post }) {
            move_frame = Some(f);
            let d = pos_of(app.world(), guard).length();
            assert!(
                d <= 4.0 * LEASH_MULT + 12.0 / 60.0 + 0.05,
                "leash fired near the line, dist {d:.3}"
            );
            break;
        }
        assert!(
            matches!(order_of(app.world(), guard), Some(Order::AttackTarget { .. })),
            "still chasing until the leash"
        );
    }
    assert!(move_frame.is_some(), "leash eventually fires (crossing at ~30 frames, speed 12)");

    // Walk home (from ~6.0 at speed 12 ~ 30 frames): arrival spends the Move
    // order (one transient order-less frame) and the next camp tick restores
    // Hold.
    let mut hold_frame = None;
    for f in 0..120 {
        app.update();
        if order_of(app.world(), guard) == Some(Order::HoldPosition) {
            hold_frame = Some(f);
            break;
        }
        let o = order_of(app.world(), guard);
        assert!(
            o == Some(Order::Move { to: post }) || o.is_none(),
            "returning under Move until arrival (transient order-less frame): {o:?}"
        );
    }
    assert!(hold_frame.is_some(), "guard re-holds at the post");
    assert!(
        pos_of(app.world(), guard).distance(post) <= 0.7,
        "back at the post"
    );
    // The distant enemy never re-aggros a holding guard (outside the radius).
    app.update();
    app.update();
    assert_eq!(order_of(app.world(), guard), Some(Order::HoldPosition));
}

// -- ③ wipe -> respawn exactly N at the posts --------------------------------

#[test]
fn wiped_camp_respawns_exactly_n_guards_at_posts() {
    let mut app = map_app(
        one_camp(&["gnoll", "gnoll", "gnoll"], 5.0, 1.0, vec![]), // respawn = 1s = 60 frames
        DayNightDef::default(),
    );
    let guards = camp_guards(app.world_mut());
    assert_eq!(guards.len(), 3);
    let mut posts: Vec<Vec3> = guards.iter().map(|&g| pos_of(app.world(), g)).collect();
    posts.sort_by(|a, b| (a.x, a.z).partial_cmp(&(b.x, b.z)).unwrap());

    // Wipe: all three dead in one frame.
    for g in &guards {
        app.world_mut().entity_mut(*g).insert(Dead);
    }
    app.update(); // wipe booked, countdown starts
    for _ in 0..40 {
        app.update();
        assert_eq!(camp_guards(app.world_mut()).len(), 0, "no respawn before the timer");
    }
    // Timer = 1s: the camp respawns by ~frame 65 (wipe frame + 60 + slop).
    let mut respawned = None;
    for f in 0..30 {
        app.update();
        let n = camp_guards(app.world_mut()).len();
        if n == 3 {
            respawned = Some(40 + f);
            break;
        }
        assert_eq!(n, 0, "respawn is all-at-once, not drip-fed");
    }
    let at = respawned.expect("guards respawn after respawn_secs");
    assert!((58..=66).contains(&at), "respawn lands one respawn_secs after the wipe: {at}");

    // Exactly N, at the original posts, holding again.
    let mut new_posts: Vec<Vec3> =
        camp_guards(app.world_mut()).iter().map(|&g| pos_of(app.world(), g)).collect();
    new_posts.sort_by(|a, b| (a.x, a.z).partial_cmp(&(b.x, b.z)).unwrap());
    assert_eq!(new_posts.len(), 3);
    for (a, b) in new_posts.iter().zip(posts.iter()) {
        assert!(a.distance(*b) < 1e-4, "respawned at the original post: {a} vs {b}");
    }
    for g in camp_guards(app.world_mut()) {
        assert_eq!(order_of(app.world(), g), Some(Order::HoldPosition), "respawned holding");
    }
    // And never more than N: no double respawn.
    for _ in 0..120 {
        app.update();
    }
    assert_eq!(camp_guards(app.world_mut()).len(), 3, "no second respawn while guards live");
}

// -- ④ guard death rolls the camp drop at the corpse -------------------------

#[test]
fn guard_death_rolls_camp_drop_at_the_corpse() {
    let mut app = map_app(
        one_camp(&["gnoll"], 5.0, 60.0, vec![("tome".into(), 1.0)]),
        DayNightDef::default(),
    );
    let guard = camp_guards(app.world_mut())[0];
    let corpse = pos_of(app.world(), guard);
    // Kill through the damage pipeline so UnitDied fires with a live corpse.
    app.world_mut().write_message(DamageRequest {
        target: guard,
        source: DamageOrigin::System,
        amount: 999.0,
        attack_type: AttackType::Normal,
        crit: false,
    });
    app.update();

    let items: Vec<(String, Vec3)> = {
        let mut q = app.world_mut().query::<(&GroundItem, &Transform)>();
        q.iter(app.world())
            .map(|(item, tf)| (item.def.clone(), tf.translation))
            .collect()
    };
    assert_eq!(items.len(), 1, "chance 1.0 drops exactly the winner: {items:?}");
    assert_eq!(items[0].0, "tome");
    assert!(
        items[0].1.distance(corpse) <= LOOT_SCATTER_RADIUS + 1e-4,
        "GroundItem within 0.3 of the death point: {:?} vs {corpse:?}",
        items[0].1
    );
}

#[test]
fn guard_death_zero_chance_drops_nothing() {
    let mut app = map_app(
        one_camp(&["gnoll"], 5.0, 60.0, vec![("never".into(), 0.0)]),
        DayNightDef::default(),
    );
    let guard = camp_guards(app.world_mut())[0];
    app.world_mut().write_message(DamageRequest {
        target: guard,
        source: DamageOrigin::System,
        amount: 999.0,
        attack_type: AttackType::Normal,
        crit: false,
    });
    app.update();
    app.update();
    let n = {
        let mut q = app.world_mut().query_filtered::<&GroundItem, ()>();
        q.iter(app.world()).count()
    };
    assert_eq!(n, 0, "chance 0.0 never drops");
}

// -- ⑤ day/night: one message per boundary, vision factor --------------------

#[test]
fn day_night_flips_emit_one_message_per_boundary() {
    let mut app = map_app(
        CampsDef::default(),
        DayNightDef { day_secs: 5.0, night_secs: 5.0, night_vision_factor: 0.5 }, // 300 frames each
    );
    let mut cursor = MessageCursor::<DayPhaseChanged>::default();
    let mut first = None;
    let mut second = None;
    let mut total = 0usize;
    for f in 1..=650 {
        app.update();
        total += {
            let msgs = app.world().resource::<Messages<DayPhaseChanged>>();
            cursor.read(msgs).count()
        };
        if total >= 1 && first.is_none() {
            first = Some(f);
            assert_eq!(app.world().resource::<DayClock>().phase, DayPhase::Night);
            assert_eq!(app.world().resource::<NightVisionFactor>().0, 0.5);
        }
        if total >= 2 && second.is_none() {
            second = Some(f);
            assert_eq!(app.world().resource::<DayClock>().phase, DayPhase::Day);
            assert_eq!(app.world().resource::<NightVisionFactor>().0, 1.0);
        }
    }
    assert_eq!(total, 2, "exactly two flips inside 650 frames of a 300/300 cycle");
    // Acceptance: the first flip lands on frame 300 (exactly one message by
    // then), the second exactly 300 frames later. Windows absorb at most one
    // float-slop frame; the gap is pinned exact.
    let f1 = first.expect("day -> night fired");
    let f2 = second.expect("night -> day fired");
    assert!((299..=301).contains(&f1), "first flip at frame ~300: {f1}");
    assert_eq!(f2 - f1, 300, "second flip exactly one night later");
}

#[test]
fn night_shrinks_vision_radius_for_new_spawns() {
    let mut app = map_app(
        CampsDef::default(),
        DayNightDef { day_secs: 5.0, night_secs: 5.0, night_vision_factor: 0.5 },
    );
    // Fast-forward into Night (flip at frame 300).
    for _ in 0..305 {
        app.update();
    }
    assert_eq!(app.world().resource::<DayClock>().phase, DayPhase::Night);

    // NEW VisionSource spawned during Night: radius 4.0 must act as 2.0.
    app.world_mut()
        .spawn((VisionSource { radius: 4.0 }, Transform::from_translation(Vec3::ZERO)));
    app.update(); // grid recomputed with the factor
    {
        let grid = app.world().resource::<VisionGrid>().clone();
        // Cell centers at (3.5, 0.5) / (1.5, 0.5): distances 3.54 / 1.58.
        assert!(
            !point_visible(&grid, Vec2::new(3.5, 0.5)),
            "night: 3.54 > 4.0 x 0.5 is dark"
        );
        assert!(
            point_visible(&grid, Vec2::new(1.5, 0.5)),
            "night: near cells stay lit"
        );
    }

    // Back to Day (frame 600): the full authored radius returns.
    for _ in 0..300 {
        app.update();
    }
    assert_eq!(app.world().resource::<DayClock>().phase, DayPhase::Day);
    let grid = app.world().resource::<VisionGrid>().clone();
    assert!(
        point_visible(&grid, Vec2::new(3.5, 0.5)),
        "day: full radius 4.0 relights the far cell"
    );
}

// -- ⑥ default sections = zero drift -----------------------------------------

#[test]
fn default_sections_zero_drift() {
    let mut app = map_app(CampsDef::default(), DayNightDef::default());
    let mut cursor = MessageCursor::<DayPhaseChanged>::default();
    for _ in 0..200 {
        app.update();
    }
    let clock = *app.world().resource::<DayClock>();
    assert_eq!(clock.phase, DayPhase::Day, "0/0 cycle never leaves Day");
    assert_eq!(clock.t, 0.0, "disabled clock never accumulates");
    assert_eq!(app.world().resource::<NightVisionFactor>().0, 1.0);
    {
        let msgs = app.world().resource::<Messages<DayPhaseChanged>>();
        assert_eq!(cursor.read(msgs).count(), 0, "no phase messages");
    }
    assert_eq!(camp_guards(app.world_mut()).len(), 0, "no camps, no guards");
    let mut q = app.world_mut().query_filtered::<Entity, With<Camp>>();
    assert_eq!(q.iter(app.world()).count(), 0, "no Camp anchors");
}

// -- serde pin: guards default empty, drops carry chances ---------------------

#[test]
fn camp_section_serde_defaults_guards_empty() {
    // Authored without guards: an inert anchor (empty vec default).
    let text = r#"
        (camps: [
            (id: "hole", pos: (1.0, 2.0), radius: 3.0, respawn_secs: 60.0,
             drops: [("tome", 0.5)]),
        ])
    "#;
    let camps: CampsDef = ron::from_str(text).unwrap();
    assert!(camps.camps[0].guards.is_empty(), "guards serde-default empty");
    assert_eq!(camps.camps[0].drops, vec![("tome".to_string(), 0.5)]);

    // Full section round-trips through RON.
    let full = CampsDef {
        camps: vec![CampDef {
            id: "troll_hole".into(),
            pos: [4.0, 4.0],
            radius: 5.0,
            respawn_secs: 60.0,
            drops: vec![("t1_scroll".into(), 0.25)],
            guards: vec!["troll".into(), "troll".into()],
        }],
    };
    let back: CampsDef = ron::from_str(&ron::to_string(&full).unwrap()).unwrap();
    assert_eq!(back.camps[0].guards, vec!["troll".to_string(), "troll".to_string()]);
    assert_eq!(back.camps[0].drops[0].1, 0.25);
}

// (helpers live above; the file ends here)
