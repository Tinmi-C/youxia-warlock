//! Day/night cycle (MAP, ADR-0018 Phase-1, round-2 plan §3): the `DayClock`
//! resource + cycle parameters (day/night lengths, night vision factor).
//! Lengths default to 0.0 = cycle disabled (a map without the section never
//! leaves Day); the vision factor defaults to the neutral 1.0 (same identity
//! default as AttackArmorTable multipliers — not a gameplay number).
//!
//! Vision seam (deliberate choice, smallest breakage surface): the daynight
//! domain maintains [`NightVisionFactor`] (the factor while Night, 1.0
//! while Day, 1.0 whenever the cycle is disabled) and `vision::
//! update_vision_grid` multiplies every VisionSource radius by it at grid
//! update time — a ~5-line read in vision, no mutation of VisionSource
//! components. The alternative (daynight scaling VisionSource.radius on
//! phase flips) would need per-source original-radius bookkeeping AND an
//! Added<VisionSource> pass for sources spawned mid-Night (the acceptance
//! pins exactly that case), strictly more moving parts.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Which half of the cycle the match is in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DayPhase {
    #[default]
    Day,
    Night,
}

/// The match clock (resource): current phase plus seconds elapsed within
/// it. Maintained by [`tick_daynight`].
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct DayClock {
    pub phase: DayPhase,
    pub t: f32,
}

impl Default for DayClock {
    fn default() -> Self {
        Self { phase: DayPhase::Day, t: 0.0 }
    }
}

/// Cycle parameters (resource): the bundle's numbers, installed by
/// [`apply_daynight`]. Default = disabled + neutral factor, agreeing with
/// [`DayNightDef::default`] field for field. Either length <= 0 disables
/// the cycle entirely (a degenerate half-length would oscillate the clock
/// every frame; "never leaves Day" is the only sane disabled semantic).
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct DayNightParams {
    pub day_secs: f32,
    pub night_secs: f32,
    pub night_vision_factor: f32,
}

impl Default for DayNightParams {
    fn default() -> Self {
        Self { day_secs: 0.0, night_secs: 0.0, night_vision_factor: 1.0 }
    }
}

/// Effective vision-radius multiplier right now: `night_vision_factor`
/// while Night, 1.0 while Day or with the cycle disabled. Maintained by
/// [`tick_daynight`] on phase flips; the ONLY consumer is
/// `vision::update_vision_grid` (see the module docs for the seam choice).
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct NightVisionFactor(pub f32);

impl Default for NightVisionFactor {
    fn default() -> Self {
        Self(1.0)
    }
}

/// Emitted once per phase transition (Day -> Night and Night -> Day).
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct DayPhaseChanged {
    pub from: DayPhase,
    pub to: DayPhase,
}

/// Map-bundle day/night section. Zero lengths (the default) disable the
/// cycle entirely — old maps load unchanged and sit in Day forever.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayNightDef {
    #[serde(default)]
    pub day_secs: f32,
    #[serde(default)]
    pub night_secs: f32,
    /// Vision radius multiplier while Night (neutral 1.0 default).
    #[serde(default = "neutral_vision")]
    pub night_vision_factor: f32,
}

fn neutral_vision() -> f32 {
    1.0
}

/// Struct-level serde default (a whole missing section) agrees with the
/// field-level defaults above: cycle disabled, vision factor neutral.
impl Default for DayNightDef {
    fn default() -> Self {
        Self { day_secs: 0.0, night_secs: 0.0, night_vision_factor: neutral_vision() }
    }
}

/// Tick: advance the clock by the frame delta; on a completed half-cycle
/// flip the phase, reset `t`, publish [`DayPhaseChanged`], and refresh
/// [`NightVisionFactor`]. Disabled (either length <= 0): no advance, no
/// flips, factor stays 1.0 — the zero-drift contract for maps without the
/// section.
///
/// Frame exactness: the flip test uses a half-frame tolerance (`t + dt/2 >=
/// length`), so exact frame counts survive f32 accumulation at fixed steps
/// — the same convention as combat's damage-point timing.
pub fn tick_daynight(
    time: Res<Time>,
    params: Res<DayNightParams>,
    mut clock: ResMut<DayClock>,
    mut factor: ResMut<NightVisionFactor>,
    mut changed: MessageWriter<DayPhaseChanged>,
) {
    if !(params.day_secs > 0.0 && params.night_secs > 0.0) {
        return; // cycle disabled (either length <= 0 / NaN): sit in Day forever
    }
    let dt = time.delta_secs();
    clock.t += dt;
    let length = match clock.phase {
        DayPhase::Day => params.day_secs,
        DayPhase::Night => params.night_secs,
    };
    if clock.t + dt * 0.5 < length {
        return;
    }
    let from = clock.phase;
    clock.phase = match from {
        DayPhase::Day => DayPhase::Night,
        DayPhase::Night => DayPhase::Day,
    };
    clock.t = 0.0;
    *factor = match clock.phase {
        DayPhase::Night => NightVisionFactor(params.night_vision_factor),
        DayPhase::Day => NightVisionFactor(1.0),
    };
    changed.write(DayPhaseChanged { from, to: clock.phase });
}

/// Bundle dispatch (SEAM0d symmetry): resets the clock and installs the
/// cycle parameters for the assembled match (re-assembly = fresh cycle).
pub fn apply_daynight(world: &mut World, bundle: &crate::bundle::MapBundle) {
    world.insert_resource(DayClock::default());
    world.insert_resource(DayNightParams {
        day_secs: bundle.daynight.day_secs,
        night_secs: bundle.daynight.night_secs,
        night_vision_factor: bundle.daynight.night_vision_factor,
    });
    // Matches the Day start above regardless of the authored factor.
    world.insert_resource(NightVisionFactor(1.0));
}

/// Domain plugin (registered once by WarbasePlugin; the ticker landed with
/// MAP). `DayPhaseChanged` is registered by lib.rs (the increment's one
/// authorized line there).
pub struct DayNightPlugin;

impl Plugin for DayNightPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DayClock>()
            .init_resource::<DayNightParams>()
            .init_resource::<NightVisionFactor>()
            .add_systems(Update, tick_daynight.in_set(crate::WarbaseSet::Tick));
    }
}

pub mod prelude {
    pub use super::{
        apply_daynight, DayClock, DayNightDef, DayNightParams, DayNightPlugin, DayPhase,
        DayPhaseChanged, NightVisionFactor,
    };
}
