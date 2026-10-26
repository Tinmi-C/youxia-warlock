//! Day/night cycle skeleton (SEAM2d, ADR-0018 Phase-1 MAP seam): the
//! DayClock resource + cycle parameters (day/night lengths, night vision
//! factor). The ticking clock and vision-radius scaling land with the MAP
//! increment; the types are pinned here. Lengths default to 0.0 = cycle
//! disabled (a map without the section never leaves Day); the vision
//! factor defaults to the neutral 1.0 (same identity default as
//! AttackArmorTable multipliers — not a gameplay number).

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
/// it. Maintained by the MAP increment's ticker.
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

/// Bundle dispatch (SEAM0d symmetry): resets the clock for the assembled
/// match (re-assembly = fresh cycle); the ticker lands with MAP.
pub fn apply_daynight(world: &mut World, bundle: &crate::bundle::MapBundle) {
    let _ = bundle.daynight.night_vision_factor;
    world.insert_resource(DayClock::default());
}

/// Domain plugin (SEAM2d skeleton; systems land with MAP).
pub struct DayNightPlugin;

impl Plugin for DayNightPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DayClock>();
    }
}

pub mod prelude {
    pub use super::{apply_daynight, DayClock, DayNightDef, DayNightPlugin, DayPhase};
}
