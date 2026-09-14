//! SkillsPlugin: commander skills, phase 1 (SK1 framework + SK2/3/4 effects;
//! requirements §17). Cooldowns tick in Simulate (Playing only → frozen on
//! pause); casting/target-picking runs in Input; buttons refresh in Observe.

use bevy::prelude::*;

use crate::resources::{SkillState, SkillTargeting, WarFrenzy};
use crate::sets::GameSet;
use crate::states::GameState;
use crate::systems;

pub struct SkillsPlugin;

impl Plugin for SkillsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SkillState>()
            .init_resource::<SkillTargeting>()
            .init_resource::<WarFrenzy>()
            .add_systems(
                Update,
                (
                    // Target picking owns the world click while active: it
                    // runs before select_object (no re-select behind the
                    // cast) and before the button handler (the click that
                    // pressed a skill button cannot confirm itself).
                    systems::skills::update_skill_targeting
                        .in_set(GameSet::Input)
                        .before(systems::command::select_object)
                        .before(systems::skills::handle_skill_buttons),
                    // Z/X/C inject the same Interaction a click produces.
                    systems::skills::skill_hotkeys
                        .in_set(GameSet::Input)
                        .before(systems::skills::handle_skill_buttons),
                    systems::skills::handle_skill_buttons.in_set(GameSet::Input),
                )
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                (
                    systems::skills::tick_skill_cooldowns,
                    // SK2: volley shots release in Simulate right after the
                    // cooldown tick (cast lands in Input the same frame, so
                    // the first meteor drops immediately on the cast frame).
                    systems::skills::release_meteor_shots.after(systems::skills::tick_skill_cooldowns),
                    // SK4: frenzy countdown.
                    systems::skills::tick_war_frenzy,
                )
                    .in_set(GameSet::Simulate)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                systems::skills::refresh_skill_buttons.in_set(GameSet::Observe),
            );
    }
}
