//! Spectator mode (SEAM2d skeleton, SEAM3c engine): watch a recorded match
//! re-simulate on a REAL (rendered) app. The `SpectateSession` resource
//! drives the replayed input stream frame by frame through the same
//! `replay::apply_envelope` re-injection the headless driver uses; hash
//! checkpoints reconcile as DIAGNOSTICS (a rendered app legitimately
//! carries viewer-side entities — end screens, panels — that the recording
//! never fingerprinted, so a divergence is reported, never fatal).
//!
//! Driver timing: `spectate_drive` runs in PreUpdate BEFORE bevy's input
//! systems, so a `Key` envelope (written as a `KeyboardInput` message)
//! becomes a `ButtonInput::just_pressed` in the SAME numbered frame the
//! recording consumed it — Update-phase game input sees identical state.
//!
//! Composition note (resource-entity symmetry, the replay pitfall): a
//! spectate app registers `SpectateSession` where the recording app
//! registered `RecordingLog` — one bookkeeping entity either way. Sessions
//! that need EXACT hash parity (automated reconciliation) mirror the
//! recording composition; the shipped `--spectate` viewer treats the
//! checkpoint chain as advisory.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::replay::{apply_envelope, state_hash, CommandEnvelope, MatchInputLog, HASH_INTERVAL};

/// Spectator mode parameters (resource; disabled by default — every
/// existing app is a player app, zero drift). SEAM3c: `enabled` flips on in
/// spectate sessions and `speed` carries the playback rate (the game feeds
/// it to `Time<Virtual>`; 1.0 = real time).
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpectateDef {
    #[serde(default)]
    pub enabled: bool,
    /// Spectated player slot label (observers may lock onto one player's
    /// view; None-style empty string = free camera).
    #[serde(default)]
    pub follow: String,
    /// Playback speed multiplier (SEAM3c; 1x / 2x).
    #[serde(default = "one")]
    pub speed: f32,
}

fn one() -> f32 {
    1.0
}

impl Default for SpectateDef {
    fn default() -> Self {
        Self { enabled: false, follow: String::new(), speed: 1.0 }
    }
}

/// One live spectate session: insert with the parsed log to drive the app.
#[derive(Resource, Debug)]
pub struct SpectateSession {
    log: MatchInputLog,
    next_input: usize,
    /// Completed updates seen by the driver (frame N = PreUpdate of the
    /// N-th Update; entry `(N, envs)` applies before it).
    frame: u32,
    hi: usize,
    /// Checkpoint divergences so far: (frame, recorded, replayed).
    pub divergences: Vec<(u32, u64, u64)>,
    /// Set once every recorded input AND hash frame has been replayed.
    pub finished: bool,
}

impl SpectateSession {
    pub fn new(log: MatchInputLog) -> Self {
        Self {
            log,
            next_input: 0,
            frame: 0,
            hi: 0,
            divergences: Vec::new(),
            finished: false,
        }
    }

    /// The last frame the log can teach us about (inputs + hashes).
    pub fn last_frame(&self) -> u32 {
        self.log
            .frames
            .last()
            .map(|f| f.0)
            .unwrap_or(0)
            .max(self.log.hashes.last().map(|f| f.0).unwrap_or(0))
    }

    /// The recorded checkpoint chain (diagnostics / reconciliation tests).
    pub fn recorded_hashes(&self) -> &[(u32, u64)] {
        &self.log.hashes
    }
}

/// PreUpdate driver (SEAM3c): applies the recording's frame inputs, then
/// reconciles the previous frame's hash checkpoint (the recording's entry
/// `(F, hash)` = world settled AFTER update F; at PreUpdate of F+1 the
/// world holds exactly that state). Divergences are collected, not fatal
/// (see the module docs). No `SpectateSession` = one lookup, zero drift.
pub fn spectate_drive(world: &mut World) {
    let Some(mut session) = world.remove_resource::<SpectateSession>() else {
        return;
    };
    session.frame += 1;
    let frame = session.frame;

    // Hash checkpoint for the PREVIOUS frame's settled state (frame 0 =
    // the assembled initial state, reconciled at PreUpdate of frame 1).
    let prev = frame - 1;
    if prev % HASH_INTERVAL == 0 {
        let hash = state_hash(world);
        while session.hi < session.log.hashes.len() && session.log.hashes[session.hi].0 < prev {
            session.hi += 1;
        }
        if let Some(&(f, expect)) = session.log.hashes.get(session.hi) {
            if f == prev && hash != expect {
                session.divergences.push((prev, expect, hash));
            }
        }
    }

    // Apply this frame's recorded inputs (between-frames visibility, the
    // same contract the headless `replay()` honors).
    if !session.finished {
        while session.next_input < session.log.frames.len()
            && session.log.frames[session.next_input].0 < frame
        {
            session.next_input += 1; // skip frames the log skipped
        }
        if session.next_input < session.log.frames.len()
            && session.log.frames[session.next_input].0 == frame
        {
            let envs: Vec<CommandEnvelope> =
                session.log.frames[session.next_input].1.clone();
            session.next_input += 1;
            for env in &envs {
                if let Err(e) = apply_envelope(world, env) {
                    warn!("[spectate] frame {frame}: {e}");
                }
            }
        }
    }
    if session.next_input >= session.log.frames.len() && frame >= session.last_frame() {
        session.finished = true;
    }
    world.insert_resource(session);
}

/// Domain plugin (SEAM2d skeleton; SEAM3c adds the live driver).
pub struct SpectatePlugin;

impl Plugin for SpectatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpectateDef>()
            // Before bevy's input systems: a `Key` envelope written NOW is
            // turned into ButtonInput by THIS frame's keyboard_input_system
            // (PreUpdate), so Update-phase consumers see it in frame N —
            // the exact frame the recording consumed the press. On apps
            // without bevy InputPlugin the ordering is vacuous (empty set).
            .add_systems(
                PreUpdate,
                spectate_drive.before(bevy::input::InputSystems),
            );
    }
}

pub mod prelude {
    pub use super::{spectate_drive, SpectateDef, SpectatePlugin, SpectateSession};
}
