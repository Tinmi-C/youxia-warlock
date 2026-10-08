//! warbase — Warcraft-style battle runtime base (ADR-0012 / ADR-0013).
//!
//! Layering: mechanisms & pipelines live here; content & numbers live in each
//! game's RON map bundle (`games/<game>/maps/*.ron`); game-specific rules live
//! in thin game plugins bound by hook ids. Hard rules (anti-corruption):
//! dependencies point one way (games -> warbase -> bevy), no game concepts,
//! no art assets, no concrete numbers in this crate.
//!
//! Design docs: docs/topics/engine/warbase-{architecture,implementation-design,test-strategy}.md

pub mod prelude {
    pub use crate::WarbasePlugin;
    pub use crate::WarbaseSet;
}

mod buff;
mod build;
mod bundle;
mod combat;
mod econ;
mod harness;
mod path;
mod rule;
mod skill;
mod unit;
mod wave;

#[cfg(feature = "fx")]
pub mod fx;
#[cfg(feature = "ui")]
pub mod ui;

use bevy::prelude::*;

/// Scheduling contract between the base and its games (impl design §2).
/// Games insert systems around these sets via `before`/`after`.
/// `Resolve` holds the single writer of `Hp` (unified damage pipeline).
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum WarbaseSet {
    /// Cooldowns/timers tick, buffs expire, econ commands are processed.
    Tick,
    /// Wave director spawns; path followers advance.
    Director,
    /// Targeting, skill triggers, autocast decisions.
    Intent,
    /// Firing, projectile flight, cast effects (emit DamageRequest only).
    Act,
    /// resolve_damage: the only system allowed to write `Hp`.
    Resolve,
    /// Deaths, rewards, leak counting, win/lose checks, events out.
    Cleanup,
}

/// Umbrella plugin; games add it before their own `GamePlugin`.
pub struct WarbasePlugin;

impl Plugin for WarbasePlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            (
                WarbaseSet::Tick,
                WarbaseSet::Director,
                WarbaseSet::Intent,
                WarbaseSet::Act,
                WarbaseSet::Resolve,
                WarbaseSet::Cleanup,
            )
                .chain(),
        );
        #[cfg(feature = "fx")]
        app.add_plugins(fx::FxPlugin);
        #[cfg(feature = "ui")]
        app.add_plugins(ui::UiPlugin);
    }
}
