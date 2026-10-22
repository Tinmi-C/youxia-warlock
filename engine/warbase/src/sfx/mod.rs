//! Sfx semantic channel (WC3 alignment, ADR-0016): the base emits semantic
//! audio requests at gameplay seams; games attach listeners that map kinds
//! to assets. No audio code and no assets live here (art is out of scope).
//!
//! Discipline: each player command produces exactly one Ack or Error; the
//! same kind is throttled (0.1s) to prevent frame-rate-driven spam.

use bevy::prelude::*;

/// Semantic sound kinds (grow per domain; names are the contract with the
/// game-side listener).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SfxKind {
    ClickAck,
    Select,
    OrderAck,
    Error,
    Cast,
    Hit,
    Death,
    WaveStart,
    WaveCleared,
    BuildComplete,
    ResearchComplete,
    LevelUp,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct SfxRequest {
    pub kind: SfxKind,
}

pub mod prelude {
    pub use super::{SfxKind, SfxRequest};
}
