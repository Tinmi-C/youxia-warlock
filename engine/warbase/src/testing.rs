//! Deterministic test infrastructure (TS0), shared by warbase's own tests
//! and consumer games. Covers the four KB-pitfall classes: wall-clock reads,
//! unseeded randomness, double-buffered message counting, stale temp files.

use bevy::app::App;
use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::MinimalPlugins;
use std::path::PathBuf;
use std::time::Duration;

pub const STEP: f64 = 1.0 / 60.0;

/// Headless app: MinimalPlugins + WarbasePlugin + fixed 1/60 timestep.
/// The caller registers its own game plugins/messages on top.
pub fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(crate::WarbasePlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(STEP)));
    app
}

/// Advance `frames` updates and count every `M` message written, draining
/// per frame (Messages are double-buffered; a single end-of-run read loses
/// everything but the last frame).
pub fn count_over_frames<M: Message>(app: &mut App, frames: usize) -> usize {
    let mut cursor = MessageCursor::<M>::default();
    let mut total = 0;
    for _ in 0..frames {
        app.update();
        let msgs = app.world().resource::<Messages<M>>();
        total += cursor.read(msgs).count();
    }
    total
}

/// Unique temp path per call; a stale file from a previous run is deleted
/// first (meta-save pollution pitfall from warden UI5).
pub fn temp_path(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    let p = std::env::temp_dir().join(format!("warbase_{}_{}_{}.tmp", tag, std::process::id(), n));
    let _ = std::fs::remove_file(&p);
    p
}
