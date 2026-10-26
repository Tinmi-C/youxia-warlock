//! Hero skeleton (SEAM2d, ADR-0018 Phase-1 HERO seam): the WC3 three
//! primary attributes and their conversion table. Level/xp growth already
//! lives in `skill::XpHolder`; this adds STR/AGI/INT + derived-stat
//! conversion (numbers ship per map through the resource, never here).
//! Altar revival and xp sharing land with the HERO increment.

use bevy::prelude::*;

/// The three WC3 hero attributes (term pinned by the round-2 plan §3).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HeroStats {
    pub str: u8,
    pub agi: u8,
    pub int: u8,
}

/// Conversion table: how much derived stat one point of each attribute is
/// worth (HP per STR, attack-speed + armor per AGI, mana pool + regen per
/// INT). Authored per map; the defaults are the identity (0.0 = no
/// conversion until a bundle says otherwise — the base never guesses
/// numbers, ADR-0012).
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct HeroStatsDef {
    pub hp_per_str: f32,
    pub attack_speed_per_agi: f32,
    pub armor_per_agi: f32,
    pub mana_per_int: f32,
    pub mana_regen_per_int: f32,
}

/// Domain plugin (SEAM2d skeleton; systems land with HERO).
pub struct HeroPlugin;

impl Plugin for HeroPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeroStatsDef>();
    }
}

pub mod prelude {
    pub use super::{HeroPlugin, HeroStats, HeroStatsDef};
}
