//! Presentation (皮相) domain — split from one god-file into domain plugins so
//! each presentation concern can grow independently (model skinning / animation
//! runtime / weapon / feel / flash) instead of stacking capability cards into a
//! single 1150-line file. Cards 12/13/14/15/18/19/21/22/24/25/30/33.
//!
//! Entry is [`PresentationPlugin`]. It is added only in `build_app()` (the
//! card-12 "back/presentation separable" rule) — headless test worlds skip it.
//!
//! Public surface preserved for headless tests (tests/behavior.rs):
//!   `find_bone_in_subtree`, `weapon_scale_fixup`, `MAX_TURN_RATE_DEG`,
//!   `HERO_CLIP_*` — re-exported below.

mod anim_runtime;
mod feel;
mod flash;
mod model;
mod weapon;

use bevy::prelude::*;

pub use feel::MAX_TURN_RATE_DEG;
pub use model::{HERO_CLIP_ATTACK, HERO_CLIP_HIT, HERO_CLIP_IDLE, HERO_CLIP_RUN, HERO_CLIP_WALK};
pub use weapon::{find_bone_in_subtree, weapon_scale_fixup};

/// Cross-plugin ordering for the presentation systems, mirroring the original
/// single `.chain()` inside `PresentationPlugin`:
///   skin_new_monsters -> bind_monster_models -> apply_flash_visuals ->
///   drive_anim_states -> face_towards_heading -> locomotion_feel ->
///   attach_weapon -> weapon_scale_fixup.
/// Each sub-plugin slots its systems into one of these stages, so the ordering
/// contract survives the split (the gameplay domain does the same with
/// `crate::sets::GameSet`).
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PresentationSet {
    Model,
    Flash,
    Anim,
    Facing,
    Feel,
    Weapon,
    WeaponFix,
}

/// Assemble the presentation sub-plugins and pin their temporal ordering.
pub struct PresentationPlugin;

impl Plugin for PresentationPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            (
                PresentationSet::Model,
                PresentationSet::Flash,
                PresentationSet::Anim,
                PresentationSet::Facing,
                PresentationSet::Feel,
                PresentationSet::Weapon,
                PresentationSet::WeaponFix,
            )
                .chain(),
        )
        .add_plugins((
            model::ModelPlugin,
            flash::FlashPlugin,
            anim_runtime::AnimRuntimePlugin,
            feel::FeelPlugin,
            weapon::WeaponPlugin,
        ));
    }
}
