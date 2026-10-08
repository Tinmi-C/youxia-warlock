//! TS0 acceptance: deterministic infra — seeded RNG reproducibility, fixed
//! timestep, per-frame message counting, temp-file isolation.

use bevy::prelude::*;
use warbase::testing::{count_over_frames, temp_path, test_app};
use warbase::prelude::MatchRng;

#[derive(Message)]
struct ProbeMsg;

fn emit_probe(mut w: MessageWriter<ProbeMsg>) {
    w.write(ProbeMsg);
}

#[test]
fn seeded_rng_same_seed_same_sequence() {
    let mut a = MatchRng::seeded(42);
    let mut b = MatchRng::seeded(42);
    for _ in 0..16 {
        assert_eq!(a.next_f32(), b.next_f32());
        assert_eq!(a.next_u32(), b.next_u32());
    }

    let mut c = MatchRng::seeded(7);
    assert_ne!(a.next_u32(), c.next_u32());
}

#[test]
fn test_app_runs_fixed_timestep() {
    let mut app = test_app();
    for _ in 0..60 {
        app.update();
    }
    let e1 = app.world().resource::<Time>().elapsed_secs();
    for _ in 0..60 {
        app.update();
    }
    let e2 = app.world().resource::<Time>().elapsed_secs();
    // Difference cancels the first-frame dt=0 quirk: exactly 60 steps of 1/60.
    assert!((e2 - e1 - 1.0).abs() < 1e-3, "60 frames = {} s", e2 - e1);
    assert!(e1 > 0.0);
}

#[test]
fn count_over_frames_counts_every_frame_not_just_last() {
    let mut app = test_app();
    app.add_message::<ProbeMsg>();
    app.add_systems(Update, emit_probe);
    // Double-buffer pitfall: a single end-of-run drain would see only 1.
    assert_eq!(count_over_frames::<ProbeMsg>(&mut app, 10), 10);
}

#[test]
fn temp_path_is_unique_and_clears_stale_files() {
    let a = temp_path("meta");
    let b = temp_path("meta");
    assert_ne!(a, b);
    std::fs::write(&a, b"stale").unwrap();
    assert!(a.exists());
    let again = temp_path("meta");
    assert_ne!(again, a, "unique per call");
    let _ = std::fs::remove_file(&a);
}
