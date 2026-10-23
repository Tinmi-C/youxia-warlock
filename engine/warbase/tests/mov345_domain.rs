//! MOV3/4/5 domain acceptance (ADR-0016 alignment plan §5): soft-collision
//! separation, the WC3 order state machine (tick_orders + acquire_engagement,
//! combat untouched), formation scatter. Numbering continues mov_domain.rs
//! (UT-MOV-01..06).

use bevy::prelude::*;
use warbase::prelude::*;
use warbase::testing::test_app;

/// A short-range instant-hit kit (fire_attacks-friendly).
fn kit(range: f32) -> AttackKit {
    AttackKit {
        def: AttackDef {
            damage: 5.0,
            interval: 0.5,
            range,
            attack_type: AttackType::Normal,
            targets: TargetDomain::Both,
            projectile: None,
        },
        priority: TargetingPriority::Nearest,
    }
}

/// Full commanded-unit bundle: MoveAgent + order state + combat kit
/// (no Hp — commanded units never appear in targeting enemy queries).
fn spawn_commanded(
    world: &mut World,
    pos: Vec3,
    speed: f32,
    radius: f32,
    range: f32,
    side: Allegiance,
) -> Entity {
    world
        .spawn((
            MoveAgent {
                speed,
                arrival_radius: 0.05,
                waypoints: Vec::new(),
                velocity: Vec3::ZERO,
                body_radius: radius,
                order: None,
            },
            AgentOrderState::Idle,
            Transform::from_translation(pos),
            side,
            kit(range),
            AttackCooldown(0.0),
            CurrentTarget::default(),
        ))
        .id()
}

/// A plain hostile body (no MoveAgent — not a separation participant).
fn spawn_enemy(world: &mut World, pos: Vec3) -> Entity {
    world
        .spawn((Transform::from_translation(pos), Hp(100.0), Allegiance::Attacker))
        .id()
}

/// Write one UnitCommand and advance a single frame.
fn send_order(app: &mut App, order: Order, subjects: Vec<Entity>) {
    app.world_mut().write_message(UnitCommand { order, subjects });
    app.update();
}

fn pos_of(app: &App, e: Entity) -> Vec3 {
    app.world().get::<Transform>(e).unwrap().translation
}

/// UT-MOV-07 (acceptance ①, MOV3): 10 radius-0.3 agents spawned at the
/// same point spread within 300 frames to pairwise >= 0.9*(0.3+0.3) = 0.54
/// while the pile's centroid stays within 1.0 of the spawn point.
#[test]
fn separation_spreads_coincident_pile() {
    let mut app = test_app();
    let mut units = Vec::new();
    for _ in 0..10 {
        units.push(spawn_commanded(
            app.world_mut(),
            Vec3::new(5.0, 0.0, 5.0),
            1.0,
            0.3,
            3.0,
            Allegiance::Defender,
        ));
    }
    for _ in 0..300 {
        app.update();
    }
    let positions: Vec<Vec3> = units.iter().map(|&e| pos_of(&app, e)).collect();
    for i in 0..10 {
        for j in (i + 1)..10 {
            let d = positions[i].distance(positions[j]);
            assert!(d >= 0.54, "pair ({i},{j}) only {d} apart after 300 frames");
        }
    }
    let centroid = positions.iter().fold(Vec3::ZERO, |acc, &p| acc + p) / 10.0;
    let drift = centroid.distance(Vec3::new(5.0, 0.0, 5.0));
    assert!(drift <= 1.0, "centroid drifted {drift}");
    // Separation is pure position correction — order state untouched.
    for &e in &units {
        assert_eq!(*app.world().get::<AgentOrderState>(e).unwrap(), AgentOrderState::Idle);
    }
}

/// UT-MOV-08 (acceptance ②, MOV3): two units hand-overlapped deeper than
/// 50% (center distance 0.1 < 0.5*(0.3+0.3)) are NOT pushed — bit-identical
/// positions one frame later (anti-jitter rule).
#[test]
fn deep_overlap_is_not_pushed() {
    let mut app = test_app();
    let a = spawn_commanded(app.world_mut(), Vec3::ZERO, 1.0, 0.3, 3.0, Allegiance::Defender);
    let b = spawn_commanded(
        app.world_mut(),
        Vec3::new(0.1, 0.0, 0.0),
        1.0,
        0.3,
        3.0,
        Allegiance::Defender,
    );
    let (pa, pb) = (pos_of(&app, a), pos_of(&app, b));
    app.update();
    assert_eq!(pos_of(&app, a), pa, "deeply-overlapped unit a was pushed");
    assert_eq!(pos_of(&app, b), pb, "deeply-overlapped unit b was pushed");
}

/// UT-MOV-09 (acceptance ③, MOV4): an attack-move unit engages a hostile
/// that appears mid-route (CurrentTarget set + Engaging + displacement stops
/// within 2 frames), and when the hostile dies it resumes the ORIGINAL route
/// within 2 frames and still reaches the ordered destination.
#[test]
fn attack_move_engages_and_resumes_route() {
    let mut app = test_app();
    let unit = spawn_commanded(app.world_mut(), Vec3::ZERO, 1.0, 0.3, 3.5, Allegiance::Defender);
    send_order(&mut app, Order::AttackMove { to: Vec3::new(10.0, 0.0, 0.0) }, vec![unit]);

    // Walk to the mid-route mark.
    let mut frames = 0;
    while pos_of(&app, unit).x < 2.0 {
        app.update();
        frames += 1;
        assert!(frames < 300, "never reached the mid-route mark");
    }
    // Hostile by the path, inside weapon range (dist ~2.6 < 3.5).
    let enemy = spawn_enemy(app.world_mut(), Vec3::new(4.5, 0.0, 0.8));
    app.update(); // frame 1: acquire (Intent) -> CurrentTarget + Engaging
    assert_eq!(app.world().get::<CurrentTarget>(unit).unwrap().0, Some(enemy));
    assert_eq!(
        *app.world().get::<AgentOrderState>(unit).unwrap(),
        AgentOrderState::Engaging
    );
    let frozen = pos_of(&app, unit);
    let route = app.world().get::<MoveAgent>(unit).unwrap().waypoints.clone();
    app.update(); // frame 2: engaged units hold position
    assert_eq!(pos_of(&app, unit), frozen, "engaged unit kept walking");

    // Kill the hostile: motion must resume within 2 frames on the SAME route.
    app.world_mut().entity_mut(enemy).insert(Dead);
    app.update();
    app.update();
    assert!(pos_of(&app, unit).x > frozen.x, "route did not resume within 2 frames");
    assert_eq!(app.world().get::<MoveAgent>(unit).unwrap().waypoints, route);
    assert_eq!(
        *app.world().get::<AgentOrderState>(unit).unwrap(),
        AgentOrderState::Moving
    );

    // ... and the unit still arrives at the ordered point (order spent).
    for _ in 0..1500 {
        app.update();
    }
    let pos = pos_of(&app, unit);
    assert!(pos.distance(Vec3::new(10.0, 0.0, 0.0)) <= 0.06, "stopped at {pos:?}");
    assert_eq!(app.world().get::<MoveAgent>(unit).unwrap().order, None);
}

/// UT-MOV-10 (acceptance ④, MOV4): HoldPosition — a hostile beyond weapon
/// range never becomes CurrentTarget and is never chased; inside range the
/// unit acquires (and fires via combat) but STILL never moves or relabels.
#[test]
fn hold_position_acquires_in_range_but_never_chases() {
    let mut app = test_app();
    let unit = spawn_commanded(app.world_mut(), Vec3::ZERO, 1.0, 0.3, 3.0, Allegiance::Defender);
    send_order(&mut app, Order::HoldPosition, vec![unit]);
    let start = pos_of(&app, unit);

    let far = spawn_enemy(app.world_mut(), Vec3::new(5.0, 0.0, 0.0)); // 5 > range 3
    for _ in 0..10 {
        app.update();
        assert_eq!(app.world().get::<CurrentTarget>(unit).unwrap().0, None);
        assert_eq!(
            *app.world().get::<AgentOrderState>(unit).unwrap(),
            AgentOrderState::Idle
        );
    }
    assert_eq!(pos_of(&app, unit), start, "hold unit crept while out of range");

    app.world_mut().entity_mut(far).despawn();
    let near = spawn_enemy(app.world_mut(), Vec3::new(2.0, 0.0, 0.0)); // 2 <= 3
    app.update();
    assert_eq!(app.world().get::<CurrentTarget>(unit).unwrap().0, Some(near));
    assert_eq!(
        *app.world().get::<AgentOrderState>(unit).unwrap(),
        AgentOrderState::Idle
    );
    assert_eq!(pos_of(&app, unit), start, "hold unit moved toward an in-range hostile");
}

/// UT-MOV-11 (acceptance ⑤, MOV4): AttackTarget — a fleeing (slower) target
/// is closed on monotonically; once inside weapon range the chaser stops
/// (Engaging) while combat handles the firing.
#[test]
fn attack_target_chases_fleeing_target_and_stops_in_range() {
    let mut app = test_app();
    let unit = spawn_commanded(app.world_mut(), Vec3::ZERO, 1.0, 0.3, 0.8, Allegiance::Defender);
    let target = spawn_enemy(app.world_mut(), Vec3::new(3.0, 0.0, 0.0));
    send_order(&mut app, Order::AttackTarget { target }, vec![unit]);

    // Target flees at 0.6/s (unit speed 1.0/s): the gap closes every frame
    // while the unit is Chasing.
    let mut prev = pos_of(&app, unit).distance(pos_of(&app, target));
    let mut engaged = false;
    for frame in 0..600 {
        app.world_mut()
            .get_mut::<Transform>(target)
            .unwrap()
            .translation
            .x += 0.6 / 60.0;
        app.update();
        if *app.world().get::<AgentOrderState>(unit).unwrap() == AgentOrderState::Engaging {
            engaged = true;
            break;
        }
        let d = pos_of(&app, unit).distance(pos_of(&app, target));
        assert!(d < prev, "gap grew at frame {frame}: {d} >= {prev}");
        prev = d;
    }
    assert!(engaged, "chaser never closed to weapon range");
    let final_gap = pos_of(&app, unit).distance(pos_of(&app, target));
    assert!(final_gap <= 0.8, "engaged at {final_gap}, beyond weapon range");
    assert_eq!(app.world().get::<CurrentTarget>(unit).unwrap().0, Some(target));

    // In range (target now stationary): the chaser stops dead.
    let stopped = pos_of(&app, unit);
    app.update();
    app.update();
    assert_eq!(pos_of(&app, unit), stopped, "engaged chaser kept moving");
    assert_eq!(
        *app.world().get::<AgentOrderState>(unit).unwrap(),
        AgentOrderState::Engaging
    );
}

/// UT-MOV-12 (acceptance ⑥, MOV4): Patrol — the unit reaches BOTH endpoints
/// (arrival radius) and the toward_b flag flips at each end (a<->b forever).
#[test]
fn patrol_pingpongs_between_endpoints() {
    let mut app = test_app();
    let unit = spawn_commanded(app.world_mut(), Vec3::ZERO, 1.0, 0.3, 3.0, Allegiance::Defender);
    let (a, b) = (Vec3::ZERO, Vec3::new(3.0, 0.0, 0.0));
    send_order(&mut app, Order::Patrol { a, b, toward_b: true }, vec![unit]);

    let flipped_off = |app: &App| -> bool {
        match &app.world().get::<MoveAgent>(unit).unwrap().order {
            Some(Order::Patrol { toward_b, .. }) => !*toward_b,
            _ => false,
        }
    };
    // Leg 1: to b, flips toward_b -> false.
    let mut frames = 0;
    while !flipped_off(&app) {
        app.update();
        frames += 1;
        assert!(frames < 600, "never reached endpoint b");
    }
    assert!(pos_of(&app, unit).distance(b) <= 0.1, "b not reached before the flip");

    // Leg 2: back to a, flips toward_b -> true again.
    let mut back = false;
    while frames < 1200 {
        app.update();
        frames += 1;
        if let Some(Order::Patrol { toward_b, .. }) = &app.world().get::<MoveAgent>(unit).unwrap().order {
            if *toward_b {
                back = true;
                break;
            }
        }
    }
    assert!(back, "never returned to endpoint a");
    assert!(pos_of(&app, unit).distance(a) <= 0.1, "a not reached before the flip");
}

/// UT-MOV-13 (acceptance ⑦, MOV5): a 5-unit group Move from one point
/// scatters into deterministic hex-ring slots — the i-th smallest entity
/// index gets slot i (slot 0 = the order point, ring 1 hexagon at spacing
/// 2*max radius), and every pair of first waypoints is >= 0.54 apart. A
/// single-subject Move goes straight to the order point.
#[test]
fn group_move_scatters_into_deterministic_slots() {
    let mut app = test_app();
    let mut units = Vec::new();
    for _ in 0..5 {
        units.push(spawn_commanded(app.world_mut(), Vec3::ZERO, 1.0, 0.3, 3.0, Allegiance::Defender));
    }
    units.sort_by_key(|&e| e.index_u32());
    send_order(&mut app, Order::Move { to: Vec3::new(10.0, 0.0, 0.0) }, units.clone());

    // Expected slots, same formula as the implementation: slot i = the
    // order point for i == 0, else ring-1 slot j = i - 1 (spacing 2*0.3).
    let spacing = 0.6_f32;
    let expected: Vec<Vec3> = (0..5)
        .map(|i| {
            if i == 0 {
                Vec3::new(10.0, 0.0, 0.0)
            } else {
                let angle = ((i - 1) as f32 / 6.0) * std::f32::consts::TAU;
                Vec3::new(10.0 + spacing * angle.cos(), 0.0, spacing * angle.sin())
            }
        })
        .collect();
    let slots: Vec<Vec3> = units
        .iter()
        .map(|&e| app.world().get::<MoveAgent>(e).unwrap().waypoints[0])
        .collect();
    for (j, wp) in slots.iter().enumerate() {
        assert!(
            (wp - expected[j]).length() < 1e-5,
            "slot {j}: {wp:?} != {:?}",
            expected[j]
        );
    }
    for i in 0..5 {
        for j in (i + 1)..5 {
            let d = slots[i].distance(slots[j]);
            assert!(d >= 0.54, "slots ({i},{j}) only {d} apart");
        }
    }

    // Single subject: straight to the order point (slot 0 IS the point).
    let mut app2 = test_app();
    let solo = spawn_commanded(app2.world_mut(), Vec3::ZERO, 1.0, 0.3, 3.0, Allegiance::Defender);
    send_order(&mut app2, Order::Move { to: Vec3::new(4.0, 0.0, 0.0) }, vec![solo]);
    assert_eq!(
        app2.world().get::<MoveAgent>(solo).unwrap().waypoints,
        vec![Vec3::new(4.0, 0.0, 0.0)]
    );
}

/// UT-MOV-14 (MOV4, Stop semantics): Stop clears the route and the order;
/// the unit freezes in place.
#[test]
fn stop_clears_route_and_order() {
    let mut app = test_app();
    let unit = spawn_commanded(app.world_mut(), Vec3::ZERO, 1.0, 0.3, 3.0, Allegiance::Defender);
    app.update(); // warm-up: the first ManualDuration frame carries dt = 0
    send_order(&mut app, Order::Move { to: Vec3::new(5.0, 0.0, 0.0) }, vec![unit]);
    assert!(!app.world().get::<MoveAgent>(unit).unwrap().waypoints.is_empty());
    app.update();
    assert!(pos_of(&app, unit).x > 0.0, "unit never started moving");
    send_order(&mut app, Order::Stop, vec![unit]);
    assert!(app.world().get::<MoveAgent>(unit).unwrap().waypoints.is_empty());
    assert_eq!(app.world().get::<MoveAgent>(unit).unwrap().order, None);
    assert_eq!(
        *app.world().get::<AgentOrderState>(unit).unwrap(),
        AgentOrderState::Idle
    );
    let frozen = pos_of(&app, unit);
    for _ in 0..30 {
        app.update();
    }
    assert_eq!(pos_of(&app, unit), frozen, "stopped unit crept afterwards");
}
