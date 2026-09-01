//! Game-order stages for the `Update` schedule (card 29 review, §7).
//!
//! The gameplay domain was previously one long `.chain()` inside
//! `GamePlugin`. Splitting it into named stages gives capability cards a
//! stable "mount point" (a card declares `挂载: GameSet::X` / `依赖消息: [...]`),
//! so a new system can be slotted in without hand-editing the main tuple.
//!
//! Ordering contract:
//! - The *stages* are chained via `configure_sets(Update, (...).chain())`.
//! - The *systems inside* each stage are chained via `(..).chain().in_set(..)`,
//!   which keeps the original relative order intact (the previous code relied
//!   on e.g. `derive_heading` running before `update_walk_cycle`).
//!
//! Stages own the temporal contract; plugins still own the domain boundary.

use bevy::prelude::*;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameSet {
    /// Write position/velocity: move_player, enemy_chase.
    Movement,
    /// Read position -> write Hp/flash: player_attack, nova_slash, contact_damage.
    Combat,
    /// Clear dead bodies (must run after all damage is settled): death_despawn.
    Despawn,
    /// Spawned entities / waves (depend on post-Despawn enemy count):
    /// pickup_drop, wave_system.
    Spawn,
    /// Observation / presentation drive (last, reads this frame's results):
    /// decay_flash, derive_heading, update_walk_cycle.
    Observe,
}
