//! BX5 acceptance: polyline math (L0), constant-speed advancement, loop
//! wrap + LapCompleted, open-chain Arrived.

use bevy::prelude::*;
use warbase::prelude::*;
use warbase::testing::{count_over_frames, test_app};

/// UT-PATH-01: length matches hand-computed geometry.
#[test]
fn polyline_length_matches_hand_computed() {
    let open = PathDef::open(vec![[0.0, 0.0], [3.0, 0.0], [3.0, 4.0]]);
    assert!((open.length() - 7.0).abs() < 1e-5);

    let square = PathDef::closed_loop(vec![[0.0, 0.0], [3.0, 0.0], [3.0, 3.0], [0.0, 3.0]]);
    assert!((square.length() - 12.0).abs() < 1e-5, "closed adds the wrap segment");
}

/// UT-PATH-02: closed sampling is continuous across the wrap point.
#[test]
fn closed_loop_sampling_wraps_continuously() {
    let square = PathDef::closed_loop(vec![[0.0, 0.0], [3.0, 0.0], [3.0, 3.0], [0.0, 3.0]]);
    assert_eq!(square.sample(0.0), Vec2::new(0.0, 0.0));
    assert_eq!(square.sample(3.0), Vec2::new(3.0, 0.0));
    let just_before_wrap = square.sample(11.999);
    // End of the closing segment (0,3)->(0,0): back near the start point.
    assert!(just_before_wrap.x.abs() < 0.01 && just_before_wrap.y < 0.1);
    let wrapped = square.sample(12.5); // 0.5 past the wrap
    assert_eq!(wrapped, Vec2::new(0.5, 0.0));

    let open = PathDef::open(vec![[0.0, 0.0], [2.0, 0.0]]);
    assert_eq!(open.sample(99.0), Vec2::new(2.0, 0.0), "open clamps at the end");
}

/// UT-PATH-03: N frames at speed s move exactly s * dt * N (frame-rate
/// independent), and the derived Transform follows the sample.
#[test]
fn follower_advances_at_constant_speed() {
    let mut app = test_app();
    let path = app.world_mut().spawn(PathDef::open(vec![[0.0, 0.0], [10.0, 0.0]])).id();
    let unit = app
        .world_mut()
        .spawn((
            PathFollower { path, progress: 0.0, laps: 0, finished: false },
            MoveSpeed(2.0),
            Transform::default(),
        ))
        .id();
    for _ in 0..120 {
        app.update(); // ~2 s
    }
    // Assert against the *actually* elapsed time (first-frame clock quirks
    // must not break this — same lesson as the template's PlayerMove test).
    let elapsed = app.world().resource::<bevy::prelude::Time>().elapsed_secs();
    let expected = 2.0 * elapsed;
    let f = app.world().get::<PathFollower>(unit).unwrap();
    assert!((f.progress - expected).abs() < 1e-3, "progress {} vs {expected}", f.progress);
    assert!((app.world().get::<Transform>(unit).unwrap().translation.x - expected).abs() < 1e-3);
}

/// UT-PATH-04: a full lap on a closed loop counts once and emits exactly one
/// LapCompleted (per lap).
#[test]
fn closed_loop_counts_laps_and_emits_lap_completed() {
    let mut app = test_app();
    let path = app.world_mut().spawn(PathDef::closed_loop(vec![[0.0, 0.0], [6.0, 0.0], [6.0, 6.0], [0.0, 6.0]])).id();
    let unit = app
        .world_mut()
        .spawn((
            PathFollower { path, progress: 0.0, laps: 0, finished: false },
            MoveSpeed(24.0), // perimeter 24 => one lap per second
            Transform::default(),
        ))
        .id();
    let laps = count_over_frames::<LapCompleted>(&mut app, 130); // ~2.16 s => 2 laps
    assert_eq!(laps, 2);
    assert_eq!(app.world().get::<PathFollower>(unit).unwrap().laps, 2);
}

/// UT-PATH-05: open chains finish once — one Arrived, no spam, finished flag.
#[test]
fn open_chain_arrives_once() {
    let mut app = test_app();
    let path = app.world_mut().spawn(PathDef::open(vec![[0.0, 0.0], [1.0, 0.0]])).id();
    let unit = app
        .world_mut()
        .spawn((
            PathFollower { path, progress: 0.0, laps: 0, finished: false },
            MoveSpeed(6.0), // 1 unit in 1/6 s
            Transform::default(),
        ))
        .id();
    let arrived = count_over_frames::<Arrived>(&mut app, 60);
    assert_eq!(arrived, 1, "Arrived exactly once");
    assert!(app.world().get::<PathFollower>(unit).unwrap().finished);
}
