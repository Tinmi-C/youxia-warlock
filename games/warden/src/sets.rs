//! Game-order stages for the `Update` schedule.
//!
//! Splitting gameplay into named stages gives capability cards a stable mount
//! point ("一个功能挂在 GameSet::X"), so new systems slot in without hand-editing
//! a growing chain. Only the *stages* are ordered here (via `configure_sets`);
//! the *systems inside* a stage keep their own relative order via `.chain()`.
//!
//! Stages own the temporal contract; plugins still own the domain boundary.

use bevy::prelude::*;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameSet {
    /// Read input, write intent (cursor / selection movement).
    Input,
    /// Placement actions (place / sell / upgrade a tower) — future card.
    Placement,
    /// Simulation: enemies move, towers fire, projectiles travel — future cards.
    Simulate,
    /// Spawning (waves spawn enemies) depends on post-cleanup counts — future cards.
    Spawn,
    /// Remove dead entities after all damage is settled — future cards.
    Cleanup,
    /// Observation / presentation drive (last, reads this frame's results).
    Observe,
}
