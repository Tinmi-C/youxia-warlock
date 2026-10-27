//! GamePlugin: headless-safe core — map load + worker-pop booking, RTS kit
//! attachment (including the peasant auto-harvest order), command-card
//! inputs (train/research/skill/item/shop), the game-side pointer bridge
//! (channel-C patch for the base's pick-plane seam, see input_bridge.rs),
//! and skill effect dispatch. Rendering/UI asset deps stay in VisualPlugin.
//! (RL4: the harvest loop and auras are base-domain consumers now — no
//! game-side systems for them remain.)

use bevy::prelude::*;
use warbase::WarbaseSet;

use crate::systems::{gameplay, input_bridge};

/// Headless-safe core (the pointer bridge no-ops without a window/input).
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            // Skills first (spawn reads SkillDefs); the map assembles next
            // (starting_forces spawn inside assemble — registry/resources
            // ready), then the game books worker population; the base
            // pointer layer is silenced via its own swap seam.
            (
                gameplay::register_skills,
                gameplay::load_map,
                gameplay::book_worker_population,
                input_bridge::silence_base_picker,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                gameplay::attach_rts_kit,
                // Command-card inputs (headless no-ops without ButtonInput).
                (
                    gameplay::train_input,
                    gameplay::research_input,
                    gameplay::skill_input,
                    gameplay::item_input,
                    gameplay::buy_input,
                ),
                // D2-style placement: raw input translation at the very front.
                input_bridge::rts_pointer_bridge.before(WarbaseSet::Tick),
                // SkillCast -> HealRequest: after the cast pipeline fires the
                // event (Act), before the single hp-up writer (Resolve).
                gameplay::skill_effects
                    .after(WarbaseSet::Act)
                    .before(WarbaseSet::Resolve),
                // SFX channel acceptance (RL3): the minimal listener proving
                // the base SfxRequest stream reaches game side (no assets —
                // log only; a real audio backend maps SfxKind -> asset here).
                gameplay::sfx_probe.after(WarbaseSet::Cleanup),
            ),
        );
    }
}
