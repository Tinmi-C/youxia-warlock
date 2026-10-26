//! warbase — Warcraft-style battle runtime base (ADR-0012 / ADR-0013).
//!
//! Layering: mechanisms & pipelines live here; content & numbers live in each
//! game's RON map bundle (`games/<game>/maps/*.ron`); game-specific rules live
//! in thin game plugins bound by hook ids. Hard rules (anti-corruption):
//! dependencies point one way (games -> warbase -> bevy), no game concepts,
//! no art assets, no concrete numbers in this crate.
//!
//! Design docs: docs/topics/engine/warbase-{architecture,implementation-design,test-strategy}.md

pub use crate::combat::DamageOrigin;

pub mod prelude {
    pub use crate::build::{try_place, try_sell, try_upgrade, Buildable, BuildGrid, BuildMode, BuildReject, Cell};
    pub use crate::bundle::build_grid;
    pub use crate::buff::{
        aggregate_stats, apply_auras, AuraAffects, AuraDef, Auras, EffectiveStats, ModEntry,
        StatKind, StatMods,
    };
    pub use crate::combat::{
        cleanup_dead, fire_attacks, healer_tick, move_projectiles, pick_targets, resolve_damage,
        resolve_heals, tick_cooldowns, AttackCooldown, AttackKit, AttackLock, AttackWindup,
        CurrentTarget, DamageDealt, DamageOrigin, DamageRequest, Dead, HealRequest, Healer,
        Projectile, TargetingPriority, UnitDied,
    };
    pub use crate::econ::{
        apply_econ, EconChanged, EconCommand, EconRejected, EconRules, EconStart, Population,
        PopPurchases, Resources,
    };
    pub use crate::path::{advance_followers, Arrived, LapCompleted, MoveSpeed, PathDef, PathFollower};
    pub use crate::rng::MatchRng;
    pub use crate::rule::{
        building_open, rules_tick, BuildWindow, LapBuffPolicy, LeakPolicy, LivesLeft, MatchEnded,
        MatchFlow, MatchRules, StockCount, WinCondition,
    };
    pub use crate::skill::{
        area_targets, distribute_xp, process_casts, run_autocast, spend_skill_point,
        tick_skill_cooldowns, ApplyStatus, CastSkill, Casting, Channeling, InterruptCast,
        Silenced, SkillCast, SkillCaster, SkillDef, SkillDefs, SkillKind, SkillSlot,
        SkillTargeting, StatusKind, Stunned, SummonSpec, Summoned,
    };
    pub use crate::unit::{
        spawn_unit, spawn_unit_deferred, sync_allegiance, AirUnit, Allegiance, Armor, ArmorType,
        AttackArmorTable, AttackDef, AttackType, BaseAttributes, HealDef, Hp, Hostility, Mana,
        ManaDef, ManaRegen, MatchDiplomacy, MaxHp, MaxMana, Owner, PlayerId, ProjDef, TargetDomain,
        TeamId, TeamsDef, UnitDef, UnitId, UnitInstance, UnitPlugin, UnitRegistry, UnitTrait,
        XpHolder,
    };
    pub use crate::item::prelude::*;
    pub use crate::movement::prelude::*;
    pub use crate::production::prelude::*;
    pub use crate::replay::prelude::*;
    pub use crate::selection::prelude::*;
    pub use crate::sfx::prelude::*;
    pub use crate::vision::prelude::*;
    // SEAM2d domain skeletons (eight new domains, ADR-0018 Phase 1).
    pub use crate::build_rts::prelude::*;
    pub use crate::camera_rts::prelude::*;
    pub use crate::camp::prelude::*;
    pub use crate::daynight::prelude::*;
    pub use crate::harvest::prelude::*;
    pub use crate::hero::prelude::*;
    pub use crate::save::prelude::*;
    pub use crate::spectate::prelude::*;
    pub use crate::wave::{
        run_director, Advance, BossTimedOut, DirectorPhase, Paths, SpawnSpec, StartWave,
        WaveCleared, WaveDirector, WaveEntry, WavePreview, WaveStarted, WaveTable, WaveTag,
    };
    pub use crate::WarbasePlugin;
    pub use crate::WarbaseSet;
}

mod combat;
mod item;
mod movement;
mod path;
mod production;
mod replay;
mod rng;
pub mod testing;
mod unit;
mod vision;

mod buff;
pub mod build;
pub mod bundle;
// SEAM2d domain skeletons (ADR-0018 Phase-1 seams; `build_rts` is named so
// the frozen TD `build` module stays untouched).
mod build_rts;
mod camera_rts;
mod camp;
mod daynight;
mod econ;
pub mod harness;
mod harvest;
mod hero;
mod rule;
mod save;
mod selection;
mod sfx;
mod skill;
mod spectate;
mod wave;

#[cfg(feature = "fx")]
pub mod fx;
#[cfg(feature = "ui")]
pub mod ui;
#[cfg(feature = "ui")]
pub mod input;

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
            .add_message::<econ::EconCommand>()
            .add_message::<econ::EconChanged>()
            .add_message::<econ::EconRejected>()
            .add_message::<wave::WavePreview>()
            .add_message::<wave::WaveStarted>()
            .add_message::<wave::WaveCleared>()
            .add_message::<wave::BossTimedOut>()
            .add_message::<wave::StartWave>()
            .add_message::<rule::MatchEnded>()
            .add_message::<skill::CastSkill>()
            .add_message::<skill::SkillCast>()
            .add_message::<skill::InterruptCast>()
            // SKVOC (round-2 plan §3): status-effect request message. The
            // ONLY message-table line this increment adds.
            .add_message::<skill::ApplyStatus>()
            .add_message::<combat::HealRequest>()
            .add_message::<selection::PickAttempt>()
            .add_message::<selection::PickResolved>()
            .add_message::<selection::SelectCommand>()
            .add_message::<selection::GroupCommand>()
            .add_message::<selection::CycleSubgroups>()
            .add_message::<movement::UnitCommand>()
            .add_message::<production::TrainCommand>()
            .add_message::<production::CancelTrain>()
            // SEAM2c message flip: the four command queues that lived as
            // polled resources while lib.rs was frozen are Messages now.
            .add_message::<item::ItemCommands>()
            .add_message::<item::ShopCommands>()
            .add_message::<production::ResearchCommands>()
            .add_message::<production::RallyCommands>()
            .add_message::<sfx::SfxRequest>()
            .init_resource::<rule::MatchRules>()
            .init_resource::<rule::MatchFlow>()
            .init_resource::<rule::StockCount>()
            .init_resource::<skill::SkillDefs>()
            .init_resource::<skill::XpEventCursor>()
            .init_resource::<unit::UnitRegistry>()
            .init_resource::<unit::AttackArmorTable>()
            .init_resource::<unit::Hostility>()
            .init_resource::<unit::MatchDiplomacy>()
            .init_resource::<rng::MatchRng>()
            .init_resource::<econ::Resources>()
            .init_resource::<econ::Population>()
            .init_resource::<econ::PopPurchases>()
            .init_resource::<econ::EconRules>()
            .init_resource::<wave::WaveTable>()
            .init_resource::<wave::Paths>()
            .init_resource::<wave::WaveDirector>()
            .init_resource::<selection::SelectionState>()
            .init_resource::<selection::ControlGroups>()
            .init_resource::<selection::SelectionPanelData>()
            .init_resource::<item::ItemRegistry>()
            .init_resource::<movement::NavGrid>();

        app.add_systems(
            Update,
            (
                combat::tick_cooldowns,
                skill::tick_skill_cooldowns,
                econ::apply_econ,
                buff::apply_auras,
                buff::apply_follower_auras,
                buff::aggregate_stats,
            )
                .chain()
                .in_set(WarbaseSet::Tick),
        );
        app.add_systems(
            Update,
            (wave::run_director, path::advance_followers)
                .chain()
                .in_set(WarbaseSet::Director),
        );
        app.add_systems(
            Update,
            (skill::run_autocast, combat::pick_targets)
                .chain()
                .in_set(WarbaseSet::Intent),
        );
        app.add_systems(
            Update,
            (skill::process_casts, combat::fire_attacks, combat::move_projectiles)
                .chain()
                .in_set(WarbaseSet::Act),
        );
        app.add_systems(
            Update,
            (combat::resolve_damage, combat::resolve_heals)
                .chain()
                .in_set(WarbaseSet::Resolve),
        );
        app.add_systems(Update, combat::healer_tick.in_set(WarbaseSet::Tick));
        app.add_systems(
            Update,
            (combat::cleanup_dead, rule::rules_tick, skill::distribute_xp)
                .chain()
                .in_set(WarbaseSet::Cleanup),
        );

        #[cfg(feature = "fx")]
        app.add_plugins(fx::FxPlugin);
        #[cfg(feature = "ui")]
        app.add_plugins(ui::UiPlugin);
        #[cfg(feature = "ui")]
        app.add_plugins(input::InputPlugin);

        // Domain plugins (ADR-0016 alignment plan): domain systems register
        // inside their own plugin; lib.rs wires them exactly once. Domain
        // increments never edit this file. SEAM2d adds the eight skeleton
        // domains once so Phase-1 parallel lines never touch lib.rs again.
        app.add_plugins(selection::SelectionPlugin)
            .add_plugins(movement::MovementPlugin)
            .add_plugins(vision::VisionPlugin)
            .add_plugins(item::ItemPlugin)
            .add_plugins(production::ProductionPlugin)
            .add_plugins(replay::ReplayPlugin)
            .add_plugins(sfx::SfxPlugin)
            .add_plugins(unit::UnitPlugin)
            .add_plugins(harvest::HarvestPlugin)
            .add_plugins(build_rts::BuildRtsPlugin)
            .add_plugins(hero::HeroPlugin)
            .add_plugins(daynight::DayNightPlugin)
            .add_plugins(camera_rts::CameraRtsPlugin)
            .add_plugins(camp::CampPlugin)
            .add_plugins(save::SavePlugin)
            .add_plugins(spectate::SpectatePlugin);
    }
}
