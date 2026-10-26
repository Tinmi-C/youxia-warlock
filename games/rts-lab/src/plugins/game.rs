//! GamePlugin: headless-safe core — map load + starting layout, RTS kit
//! attachment, command-card inputs (train/research/skill/item/shop), the
//! game-side pointer bridge (channel-C patch for the base's pick-plane seam,
//! see input_bridge.rs), skill effect dispatch, and the free-agent aura
//! incubator. Rendering/UI asset deps stay in VisualPlugin.

use bevy::prelude::*;
use warbase::prelude::aggregate_stats;
use warbase::WarbaseSet;

use crate::systems::{gameplay, input_bridge};

/// Headless-safe core (the pointer bridge no-ops without a window/input).
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            // Skills first (spawn reads SkillDefs), layout spawns AFTER the
            // map assembles (registry/resources ready); the base pointer
            // layer is silenced via its own swap seam.
            (
                gameplay::register_skills,
                gameplay::load_map,
                gameplay::spawn_starting_units,
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
                gameplay::harvest_tick,
                // D2-style placement: raw input translation at the very front.
                input_bridge::rts_pointer_bridge.before(WarbaseSet::Tick),
                // Devotion for free agents (hole #7 incubator): entries land
                // before the same frame's EffectiveStats fold.
                gameplay::free_agent_auras.before(aggregate_stats).in_set(WarbaseSet::Tick),
                // SkillCast -> HealRequest: after the cast pipeline fires the
                // event (Act), before the single hp-up writer (Resolve).
                gameplay::skill_effects
                    .after(WarbaseSet::Act)
                    .before(WarbaseSet::Resolve),
            ),
        );
    }
}
