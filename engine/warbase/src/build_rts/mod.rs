//! RTS construction skeleton (SEAM2d, ADR-0018 Phase-1 BLDG seam): the
//! ORDER vocabulary for "build a building" (the existing `build` domain is
//! the TD placement flow — slots/grid/costs; this module is the worker-
//! driven construction loop: site selection, progress, multi-worker
//! acceleration, town tiers). Named `build_rts` to keep the frozen `build`
//! module untouched. Execution lands with the BLDG increment.

use bevy::prelude::*;

/// What one `Order::Build { what, at }` has to satisfy (per-building
/// authored numbers; the registry key is the building's UnitId).
#[derive(Debug, Clone, Default)]
pub struct BuildOrderDef {
    /// Seconds one builder needs alone (BLDG: N builders accelerate
    /// linearly when `builders_accelerate`).
    pub time: f32,
    pub cost_gold: u32,
    pub cost_wood: u32,
    /// Whether extra assigned workers speed the site up (WC3 humans) or
    /// not (undead-style single-builder sites).
    pub builders_accelerate: bool,
}

/// Town-hall tier marker (T1 -> T2 -> T3 upgrade chain; the tier's unlock
/// set rides the production domain's `Unlocks`). A fresh hall spawns with
/// `TownTier(1)`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownTier(pub u8);

/// Domain plugin (SEAM2d skeleton; systems land with BLDG).
pub struct BuildRtsPlugin;

impl Plugin for BuildRtsPlugin {
    fn build(&self, _app: &mut App) {}
}

pub mod prelude {
    pub use super::{BuildOrderDef, BuildRtsPlugin, TownTier};
}
