//! Replay (WC3 alignment, ADR-0016): record the player INPUT stream and
//! replay it against the deterministic base (MatchRng + fixed timestep +
//! D6 discipline are the foundations this harvests).
//!
//! Input-level, not command-level: the envelope never carries Entity ids
//! (not serializable, not stable across runs) — replays re-inject inputs and
//! let the logic layers derive commands, WC3-style. RPL1 lands the recorder
//! system (Update, before everything) and the replay driver; RPL2 the
//! real-machine entry points. SEAM0b pins the container shape.

use serde::{Deserialize, Serialize};

/// Version of the input-log format itself (bump on envelope changes).
pub const INPUT_LOG_VERSION: u32 = 1;

/// One recorded player input (frame index lives in the log entry).
/// RPL1 may refine variants, but they stay serde-only, Entity-free.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CommandEnvelope {
    Pick {
        world: [f32; 2],
        button: crate::selection::PickButton,
        clicks: u8,
        ctrl: bool,
        shift: bool,
    },
    Key {
        code: String,
    },
    WaveStart,
    /// Escape hatch for domain-specific inputs until RPL1 finalizes the
    /// vocabulary (kind + RON/JSON payload string).
    Ext { kind: String, payload: String },
}

/// The recorded log: map fingerprint, seed, and per-frame inputs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MatchInputLog {
    pub version: u32,
    pub map_hash: u64,
    pub seed: u64,
    /// (frame, inputs applied at that frame).
    pub frames: Vec<(u32, Vec<CommandEnvelope>)>,
    /// State fingerprint every `hash_interval` frames (replay reconciliation).
    pub hashes: Vec<(u32, u64)>,
}

/// Domain systems (registered once by WarbasePlugin; RPL1 adds the recorder
/// HERE, never in lib.rs). Recording is pure logic — feature-free.
pub struct ReplayPlugin;

impl bevy::prelude::Plugin for ReplayPlugin {
    fn build(&self, _app: &mut bevy::prelude::App) {
        // TODO(RPL1): input recorder first in Update (before everything).
    }
}

pub mod prelude {
    pub use super::{CommandEnvelope, MatchInputLog, ReplayPlugin, INPUT_LOG_VERSION};
}