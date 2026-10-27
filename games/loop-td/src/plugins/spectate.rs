//! SEAM3c spectate mode (--spectate <file>): watch a recorded match
//! re-simulate on the REAL game app (DefaultPlugins + full loop-td stack).
//! The base `SpectateSession` driver re-injects the recorded input stream
//! frame by frame (Card/Key/WaveStart/Pick envelopes through the same
//! `replay::apply_envelope` the headless verifier uses); this plugin only
//! arms the session and owns the viewer ergonomics:
//!
//! - Startup (after `gameplay::load_map`): parse the log, warn on map
//!   fingerprint drift (loop.ron moved on since the recording — advisory:
//!   a changed map desyncs visuals, the driver keeps replaying), re-seed
//!   `MatchRng` with the recorded seed, insert `SpectateSession` +
//!   `SpectateDef { enabled: true }`.
//! - Update: keys 1 / 2 set playback speed (feeds `Time<Virtual>`);
//!   when the session finishes, print the divergence report and exit 0
//!   after a short grace so the last frames render.
//!
//! Viewer etiquette: the re-simulated match is deterministic — real user
//! input (clicks, most keys) desyncs the visuals. The speed keys are the
//! only intentional viewer input.

use bevy::prelude::*;
use warbase::prelude::{
    MatchInputLog, MatchRng, SpectateDef, SpectateSession,
};

/// Frames rendered after the log runs dry before the viewer exits.
const EXIT_GRACE_FRAMES: u32 = 120;

/// The `--spectate <file>` plugin.
pub struct SpectateIoPlugin {
    pub log_path: std::path::PathBuf,
    /// Starting playback speed (1.0 = real time; 2.0 = double).
    pub speed: f32,
}

/// Parse a recorded log from disk (shared with main.rs error reporting).
pub fn load_log(path: &std::path::Path) -> Result<MatchInputLog, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read replay {}: {e}", path.display()))?;
    ron::from_str(&text)
        .map_err(|e| format!("cannot parse {} as a MatchInputLog (RON payload): {e}", path.display()))
}

/// Viewer config (plugin data as a resource so the arm system is a plain
/// fn that can be ordered against `load_map`).
#[derive(Resource)]
struct SpectateConfig {
    log_path: std::path::PathBuf,
    speed: f32,
}

#[derive(Resource, Default)]
struct ExitCountdown(Option<u32>);

impl Plugin for SpectateIoPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SpectateConfig { log_path: self.log_path.clone(), speed: self.speed })
            .insert_resource(ExitCountdown(None))
            .add_systems(
                Startup,
                arm_spectate_session.after(crate::systems::gameplay::load_map),
            )
            .add_systems(Update, (spectate_speed_keys, spectate_exit));
    }
}

/// Startup: arm the session (see the module docs).
fn arm_spectate_session(world: &mut World) {
    let (log_path, start_speed) = {
        let cfg = world.remove_resource::<SpectateConfig>().expect("inserted at build");
        (cfg.log_path, cfg.speed)
    };
    arm(world, &log_path, start_speed);
}

/// Startup arm logic (shared by the plugin closure and tests).
fn arm(world: &mut World, log_path: &std::path::Path, start_speed: f32) {
    // The recorded seed makes the re-simulation consume the same MatchRng
    // stream (digs, drops, grants).
    let log = match load_log(log_path) {
        Ok(log) => log,
        Err(e) => {
            eprintln!("[loop-td] spectate: {e}");
            std::process::exit(1);
        }
    };
    match crate::replay_io::map_file_hash(&crate::systems::gameplay::default_map_path()) {
        Ok(hash) if hash != log.map_hash => {
            eprintln!(
                "[loop-td] spectate: map fingerprint drift ({:#x} != recorded {:#x}) — visuals may desync; pass the recording's map via --map",
                hash, log.map_hash
            );
        }
        Err(e) => eprintln!("[loop-td] spectate: {e}"),
        _ => {}
    }
    world.insert_resource(MatchRng::seeded(log.seed));
    world.insert_resource(SpectateSession::new(log));
    let mut def = SpectateDef::default();
    def.enabled = true;
    def.speed = start_speed;
    world.insert_resource(def);
    world.resource_mut::<Time<Virtual>>().set_relative_speed(start_speed);
    info!("[loop-td] spectate armed");
}

/// Keys 1 / 2 (and 3 for 3x): playback speed through `Time<Virtual>`.
fn spectate_speed_keys(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut time: ResMut<Time<Virtual>>,
    mut def: Option<ResMut<SpectateDef>>,
) {
    let Some(keys) = keys else { return };
    for (code, speed) in [
        (KeyCode::Digit1, 1.0),
        (KeyCode::Digit2, 2.0),
        (KeyCode::Digit3, 3.0),
    ] {
        if keys.just_pressed(code) {
            time.set_relative_speed(speed);
            if let Some(def) = def.as_deref_mut() {
                def.speed = speed;
            }
            info!("[loop-td] spectate speed x{speed}");
        }
    }
}

/// Print the verdict and exit once the log is fully replayed.
fn spectate_exit(world: &mut World) {
    let (finished, report) = {
        let Some(session) = world.get_resource::<SpectateSession>() else { return };
        if !session.finished {
            return;
        }
        (
            true,
            (
                session.recorded_hashes().len(),
                session.divergences.clone(),
            ),
        )
    };
    let _ = finished;
    let mut countdown = world.get_resource_or_insert_with(ExitCountdown::default);
    let left = countdown.0.get_or_insert(EXIT_GRACE_FRAMES);
    *left -= 1;
    if *left > 0 {
        return;
    }
    let (hashes, divergences) = report;
    println!(
        "[loop-td] spectate finished: {} hash checkpoints replayed, {} divergences",
        hashes,
        divergences.len()
    );
    for (frame, recorded, replayed) in divergences.iter().take(5) {
        println!("  frame {frame}: recorded {recorded:#x}, replayed {replayed:#x}");
    }
    std::process::exit(0);
}
