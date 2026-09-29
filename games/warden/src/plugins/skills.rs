//! SkillsPlugin: commander skills, phase 1 (SK1 framework + SK2/3/4 effects;
//! requirements §17) + the generic per-entity skill framework (phase 2,
//! ADR-0011 / card SK5). Cooldowns tick in Simulate (Playing only → frozen
//! on pause); casting/target-picking runs in Input; buttons refresh in
//! Observe.

use bevy::prelude::*;

use crate::resources::{SkillAiming, SkillBalance, SkillDefs, SkillState, SkillTargeting, WarFrenzy};
use crate::sets::GameSet;
use crate::states::GameState;
use crate::systems;

pub struct SkillsPlugin;

impl Plugin for SkillsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SkillState>()
            .init_resource::<SkillTargeting>()
            .init_resource::<WarFrenzy>()
            // Generic framework (SK5, ADR-0011).
            .init_resource::<SkillDefs>()
            .init_resource::<SkillBalance>()
            .init_resource::<SkillAiming>()
            .add_message::<systems::skills::CastSkill>()
            .add_message::<systems::skills::SkillCast>()
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
                    // Same ordering contract for the SK5 framework path.
                    systems::skills::update_aim_picking
                        .in_set(GameSet::Input)
                        .before(systems::command::select_object)
                        .before(systems::skills::handle_cast_requests),
                    // Z/X/C inject the same Interaction a click produces.
                    systems::skills::skill_hotkeys
                        .in_set(GameSet::Input)
                        .before(systems::skills::handle_skill_buttons),
                    systems::skills::handle_skill_buttons.in_set(GameSet::Input),
                    // SK5: validate cast requests (message-driven; writers
                    // are the UI/autocast systems of SK8-SK10 and tests).
                    systems::skills::handle_cast_requests.in_set(GameSet::Input),
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
                    // SK5: per-entity slot cooldowns, same law.
                    systems::skills::tick_caster_cooldowns,
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
