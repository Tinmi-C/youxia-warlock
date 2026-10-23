//! MOV domain acceptance (ADR-0016 alignment plan §5, MOV1+MOV2): agent
//! kinematics, A* determinism (pinned tie-breaks), frame-budgeted path
//! queue, dirty re-path on new blockers, MoveAgent/PathFollower mutual
//! exclusion.

use bevy::prelude::*;
use warbase::prelude::*;
use warbase::testing::test_app;

/// 10x10 grid, cell 1.0, origin (0,0), `blocked` seeded both as authored and
/// live (dirty_recompute rebuilds the live set from authored + BuildGrid).
fn nav_10x10(blocked: &[(i32, i32)]) -> NavGrid {
    let mut nav = NavGrid::default();
    nav.cell = 1.0;
    nav.cols = 10;
    nav.rows = 10;
    for &c in blocked {
        nav.authored.insert(c);
        nav.blocked.insert(c);
    }
    nav
}

/// Mechanical patch (MOV3/4/5): MoveAgent grew `body_radius` + `order`.
fn spawn_agent(world: &mut World, pos: Vec3, speed: f32, arrival_radius: f32) -> Entity {
    world
        .spawn((
            MoveAgent {
                speed,
                arrival_radius,
                waypoints: Vec::new(),
                velocity: Vec3::ZERO,
                body_radius: DEFAULT_BODY_RADIUS,
                order: None,
            },
            AgentOrderState::Idle,
            Transform::from_translation(pos),
        ))
        .id()
}

/// Sum of grid-step costs of a raw path (cardinal 1, diagonal sqrt(2)).
fn path_cost(path: &[(i32, i32)]) -> f32 {
    path.windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            if a.0 != b.0 && a.1 != b.1 {
                std::f32::consts::SQRT_2
            } else {
                1.0
            }
        })
        .sum()
}

/// Every step is unit-length and no diagonal cuts a blocked corner.
fn assert_no_corner_cut(blocked: &[(i32, i32)], path: &[(i32, i32)]) {
    for w in path.windows(2) {
        let (a, b) = (w[0], w[1]);
        assert!(
            (a.0 - b.0).abs() <= 1 && (a.1 - b.1).abs() <= 1,
            "step {a:?}->{b:?} is not unit-length"
        );
        if a.0 != b.0 && a.1 != b.1 {
            for o in [(a.0, b.1), (b.0, a.1)] {
                assert!(
                    !blocked.contains(&o),
                    "corner-cut diagonal {a:?}->{b:?} (orthogonal {o:?} blocked)"
                );
            }
        }
    }
}

/// Does the polyline pos -> waypoints pass through the given cell square?
fn route_crosses(pos: Vec3, waypoints: &[Vec3], cell: (i32, i32)) -> bool {
    let inside = |p: Vec3| {
        p.x >= cell.0 as f32 && p.x < cell.0 as f32 + 1.0 && p.z >= cell.1 as f32 && p.z < cell.1 as f32 + 1.0
    };
    let mut prev = pos;
    for w in waypoints {
        let delta = *w - prev;
        let dist = delta.length();
        let steps = ((dist / 0.25).ceil() as usize).max(1);
        for i in 0..=steps {
            if inside(prev + delta * (i as f32 / steps as f32)) {
                return true;
            }
        }
        prev = *w;
    }
    false
}

/// UT-MOV-01 (acceptance ①): 10x10 L-wall (0,0)->(9,9): optimal raw path
/// (cost 10 + 4*sqrt(2), hand-derived below) with the pinned step count,
/// and a pinned pulled waypoint list.
#[test]
fn astar_l_wall_optimal_and_pinned() {
    // L: vertical bar col 4 rows 0..=7 + horizontal arm row 7 cols 5..=8.
    let mut wall = Vec::new();
    for r in 0..=7 {
        wall.push((4, r));
    }
    for c in 5..=8 {
        wall.push((c, 7));
    }
    let nav = nav_10x10(&wall);

    let path = astar(&nav, (0, 0), (9, 9)).expect("goal is reachable");
    // Hand-verified optimum: 4 diagonals to (3,3), up col 3 to (3,8), a
    // corner-safe diagonal to (4,9) (orthogonals (3,9)/(4,8) free), east to
    // the goal. Pinned sequence (determinism lock).
    assert_eq!(
        path,
        vec![
            (0, 0), (1, 1), (2, 2), (3, 3), (3, 4), (3, 5), (3, 6), (3, 7), (3, 8), (4, 9),
            (5, 9), (6, 9), (7, 9), (8, 9), (9, 9),
        ]
    );
    // Optimal cost, hand-derived: the col-4 wall (rows 0..=7) forces the
    // crossing at row 8 or 9; any optimal route = 10 cardinals + 4
    // diagonals = 10 + 4*sqrt(2) (~15.657). Cheaper means wall leak or
    // corner cut; dearer means suboptimal search.
    let expected = 10.0 + 4.0 * std::f32::consts::SQRT_2;
    let cost = path_cost(&path);
    assert!((cost - expected).abs() < 1e-3, "cost {cost} vs hand-derived {expected}");
    assert_no_corner_cut(&wall, &path);
    // 14 moves => 15 cells (pinned).
    assert_eq!(path.len(), 15, "raw cell count");
    // Pulled waypoints: 2 legs around the wall corner — LOS (0,0)->(4,9)
    // crosses col 4 only at rows 8/9 (verified by hand), (4,9)->(9,9) runs
    // free along row 9. Pinned.
    let pulled = smooth_path(&nav, &path);
    assert_eq!(pulled.len(), 3, "pulled waypoint count");
    assert_eq!(pulled, vec![(0, 0), (4, 9), (9, 9)]);
}

/// UT-MOV-02 (acceptance ②): diagonals never cut a blocked corner — the
/// detour around it is taken even though cutting would be shorter.
#[test]
fn astar_refuses_corner_cutting() {
    let blocked = [(2, 1)];
    let nav = nav_10x10(&blocked);
    let path = astar(&nav, (0, 0), (2, 2)).expect("goal is reachable");
    // Pinned: the corner-safe optimum around (2,1) — diagonal to (1,1)
    // (orthogonals free), north, then east; never the (1,1)->(2,2) cut.
    assert_eq!(path, vec![(0, 0), (1, 1), (1, 2), (2, 2)]);
    assert_no_corner_cut(&blocked, &path);
    // Cutting through the corner would cost 2*sqrt(2) (~2.83); the honest
    // detour = 2 cardinals + 1 diagonal = 2 + sqrt(2) (~3.41).
    let expected = 2.0 + std::f32::consts::SQRT_2;
    let cost = path_cost(&path);
    assert!((cost - expected).abs() < 1e-3, "cost {cost} vs hand-derived {expected}");
    assert_eq!(path.len(), 4, "3 moves minimum");
}

/// UT-MOV-03 (acceptance ③): 100 same-frame requests — at most
/// `budget_per_frame` (8) A* calls that frame; all complete within
/// ceil(100/8) = 13 frames. (Plan §5's "within 8 frames" is arithmetically
/// unsatisfiable at 8 calls/frame for 100 requests — 13 pinned here and
/// flagged in the increment report.)
#[test]
fn path_queue_throttles_to_budget() {
    let mut app = test_app();
    app.insert_resource(BuildGrid::free_grid(1.0, 10, 10));
    // L-wall so every request runs real A* (no trivial same-cell answers).
    let mut wall = Vec::new();
    for r in 0..=7 {
        wall.push((4, r));
    }
    for c in 5..=8 {
        wall.push((c, 7));
    }
    app.insert_resource(nav_10x10(&wall));

    let mut units = Vec::new();
    for i in 0..100 {
        let (fx, fy) = ((i % 10) as i32, (i / 10) as i32);
        let e = spawn_agent(app.world_mut(), Vec3::new(fx as f32 + 0.5, 0.0, fy as f32 + 0.5), 1.0, 0.05);
        let to = Vec3::new((9 - fx) as f32 + 0.5, 0.0, (9 - fy) as f32 + 0.5);
        request_path(app.world_mut(), e, to);
        units.push(e);
    }
    app.update();
    let calls = app.world().resource::<AstarCount>().calls;
    assert!(calls <= 8, "{calls} A* calls in one frame (budget 8)");
    assert_eq!(calls, 8, "full budget is used while work is pending");
    for _ in 0..12 {
        app.update();
    }
    assert!(app.world().resource::<PathRequestQueue>().0.is_empty(), "queue must be drained");
    for e in &units {
        let agent = app.world().get::<MoveAgent>(*e).unwrap();
        assert!(!agent.waypoints.is_empty(), "unit {e:?} is still routeless after 13 frames");
        assert_eq!(*app.world().get::<AgentOrderState>(*e).unwrap(), AgentOrderState::Moving);
    }
}

/// UT-MOV-04 (acceptance ④): a blocker dropped mid-route (BuildGrid change)
/// re-paths the affected agent well inside 60 frames, and the unit still
/// arrives at its destination.
#[test]
fn mid_route_blocker_triggers_repath_and_arrival() {
    let mut app = test_app();
    app.insert_resource(BuildGrid::free_grid(1.0, 10, 10));
    app.insert_resource(nav_10x10(&[]));

    let unit = spawn_agent(app.world_mut(), Vec3::new(0.5, 0.0, 0.5), 1.0, 0.05);
    let dest = Vec3::new(9.5, 0.0, 0.5);
    request_path(app.world_mut(), unit, dest);
    app.update(); // straight route assigned (frame-1 dirty rebuild is a no-op)

    // Walk until past cell (3,0), then drop a wall at (4,0) through the
    // build grid (exactly what try_place does to the resource).
    let mut frames = 0;
    while app.world().get::<Transform>(unit).unwrap().translation.x < 3.6 {
        app.update();
        frames += 1;
        assert!(frames < 240, "unit never reached the mid-route mark");
    }
    app.world_mut().resource_mut::<BuildGrid>().blocked.insert(Cell(4, 0));

    // Within 60 frames: a fresh route avoiding (4,0), or already arrived.
    let mut settled = None;
    for f in 0..60 {
        app.update();
        let agent = app.world().get::<MoveAgent>(unit).unwrap();
        let pos = app.world().get::<Transform>(unit).unwrap().translation;
        let arrived = pos.distance(dest) <= 0.1;
        let holding = !agent.waypoints.is_empty() && !route_crosses(pos, &agent.waypoints, (4, 0));
        if arrived || holding {
            settled = Some((f, arrived));
            break;
        }
    }
    let Some((f, _)) = settled else {
        panic!("no re-path within 60 frames");
    };
    // Dirty detection (1 frame) + budgeted processing (1 frame): the re-path
    // itself lands on the 2nd frame after the change.
    assert!(f <= 2, "re-path took {f} frames after the blocker change");

    // The re-routed unit still arrives exactly.
    for _ in 0..1500 {
        app.update();
    }
    let pos = app.world().get::<Transform>(unit).unwrap().translation;
    assert!(pos.distance(dest) < 0.1, "did not arrive, stopped at {pos:?}");
    assert_eq!(*app.world().get::<AgentOrderState>(unit).unwrap(), AgentOrderState::Idle);
}

/// UT-MOV-05 (acceptance ⑤): kinematics — speed 1.0 for 60 frames moves
/// exactly 1.0 (frame-rate independent), and the arrival radius brakes.
#[test]
fn kinematics_constant_speed_and_arrival_brake() {
    // (a) constant speed with the waypoint far away. One warm-up update
    // first: the first ManualDuration frame carries dt=0 (same first-frame
    // clock quirk bx5_path documents), then 60 real frames.
    let mut app = test_app();
    let unit = spawn_agent(app.world_mut(), Vec3::ZERO, 1.0, 0.05);
    app.world_mut().get_mut::<MoveAgent>(unit).unwrap().waypoints = vec![Vec3::new(5.0, 0.0, 0.0)];
    app.update(); // warm-up (dt == 0)
    for _ in 0..60 {
        app.update();
    }
    let x = app.world().get::<Transform>(unit).unwrap().translation.x;
    assert!((x - 1.0).abs() < 1e-4, "displacement after 60 frames: {x} != 1.0");
    let agent = app.world().get::<MoveAgent>(unit).unwrap();
    assert!((agent.velocity.length() - 1.0).abs() < 1e-4, "velocity magnitude != speed");
    assert_eq!(*app.world().get::<AgentOrderState>(unit).unwrap(), AgentOrderState::Moving);

    // (b) arrival brake: radius 0.3 around (1,0,0) stops at ~0.7 and stays.
    let mut app = test_app();
    let unit = spawn_agent(app.world_mut(), Vec3::ZERO, 1.0, 0.3);
    app.world_mut().get_mut::<MoveAgent>(unit).unwrap().waypoints = vec![Vec3::new(1.0, 0.0, 0.0)];
    for _ in 0..120 {
        app.update();
        if *app.world().get::<AgentOrderState>(unit).unwrap() == AgentOrderState::Idle {
            break;
        }
    }
    let x = app.world().get::<Transform>(unit).unwrap().translation.x;
    assert!((x - 0.7).abs() < 1e-3, "braked at {x}, expected ~0.7");
    let agent = app.world().get::<MoveAgent>(unit).unwrap();
    assert!(agent.waypoints.is_empty(), "waypoints cleared on arrival");
    assert_eq!(agent.velocity, Vec3::ZERO);
    assert_eq!(*app.world().get::<AgentOrderState>(unit).unwrap(), AgentOrderState::Idle);
    for _ in 0..30 {
        app.update();
    }
    let x2 = app.world().get::<Transform>(unit).unwrap().translation.x;
    assert!((x2 - x).abs() < 1e-6, "crept after braking: {x2} vs {x}");
}

/// UT-MOV-06 (acceptance ⑥): MoveAgent + PathFollower on one entity is a
/// contract violation — debug builds halt (release builds skip the entity).
#[test]
#[should_panic(expected = "mutually exclusive")]
fn move_agent_with_path_follower_rejected() {
    let mut app = test_app();
    let path = app.world_mut().spawn(PathDef::open(vec![[0.0, 0.0], [1.0, 0.0]])).id();
    app.world_mut().spawn((
        MoveAgent {
            speed: 1.0,
            arrival_radius: 0.05,
            waypoints: vec![Vec3::new(2.0, 0.0, 0.0)],
            velocity: Vec3::ZERO,
            body_radius: DEFAULT_BODY_RADIUS,
            order: None,
        },
        AgentOrderState::Moving,
        Transform::default(),
        PathFollower { path, progress: 0.0, laps: 0, finished: false },
        MoveSpeed(1.0),
    ));
    app.update();
}
