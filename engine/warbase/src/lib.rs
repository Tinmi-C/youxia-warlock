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
    pub use crate::combat::{
        cleanup_dead, fire_attacks, move_projectiles, pick_targets, resolve_damage,
        tick_cooldowns, AttackCooldown, AttackKit, CurrentTarget, DamageDealt, DamageRequest,
        Dead, Projectile, TargetingPriority, UnitDied,
    };
    pub use crate::path::{advance_followers, Arrived, LapCompleted, MoveSpeed, PathDef, PathFollower};
    pub use crate::rng::MatchRng;
    pub use crate::unit::{
        spawn_unit, AirUnit, Allegiance, Armor, ArmorType, AttackArmorTable, AttackDef,
        AttackType, BaseAttributes, Hp, MaxHp, ProjDef, TargetDomain, UnitDef, UnitId,
        UnitInstance, UnitRegistry, UnitTrait, XpHolder,
    };
    pub use crate::WarbasePlugin;
    pub use crate::WarbaseSet;
}

mod combat;
mod path;
mod rng;
pub mod testing;
mod unit;

mod buff;
mod build;
mod bundle;
mod econ;
mod harness;
mod rule;
mod skill;
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

        app.add_message::<combat::DamageRequest>()
            .add_message::<combat::DamageDealt>()
            .add_message::<combat::UnitDied>()
            .add_message::<path::LapCompleted>()
            .add_message::<path::Arrived>()
            .init_resource::<unit::UnitRegistry>()
            .init_resource::<unit::AttackArmorTable>()
            .init_resource::<rng::MatchRng>();

        app.add_systems(Update, combat::tick_cooldowns.in_set(WarbaseSet::Tick));
        app.add_systems(Update, path::advance_followers.in_set(WarbaseSet::Director));
        app.add_systems(Update, combat::pick_targets.in_set(WarbaseSet::Intent));
        app.add_systems(
            Update,
            (combat::fire_attacks, combat::move_projectiles)
                .chain()
                .in_set(WarbaseSet::Act),
        );
        app.add_systems(Update, combat::resolve_damage.in_set(WarbaseSet::Resolve));
        app.add_systems(Update, combat::cleanup_dead.in_set(WarbaseSet::Cleanup));

        #[cfg(feature = "fx")]
        app.add_plugins(fx::FxPlugin);
        #[cfg(feature = "ui")]
        app.add_plugins(ui::UiPlugin);
    }
}
