//! Presentation domain — heading facing (card 15 / 18) and locomotion feel
//! (card 24 / 25).

use bevy::prelude::*;

use crate::components::{Heading, WalkCycle};
use crate::plugins::anim;
use crate::states::GameState;

use super::anim_runtime::AnimLink;
use super::model::MODEL_Y_OFFSET;
use super::PresentationSet;

// --- constants -------------------------------------------------------------

/// Constant yaw turn speed for heading convergence (card 15 acceptance math:
/// a full about-face takes 180/540 = 0.33 s, inside the 0.6 s deadline).
pub const MAX_TURN_RATE_DEG: f32 = 540.0;

/// Card 24 lean/bob tuning.
const LEAN_MAX_DEG: f32 = 10.0;
const LEAN_REF_SPEED: f32 = 4.0;
const LEAN_RESPONSE: f32 = 6.0;
const BOB_AMP: f32 = 0.045;
const BOB_RESPONSE: f32 = 14.0;
const WALK_CYCLE_SECS: f32 = 1.375;
const RUN_CYCLE_SECS: f32 = 0.8;

// --- component -------------------------------------------------------------

/// Card 24 locomotion-feel state, presentation-local (lives on the wrapper):
/// smoothed lean angle, smoothed bob height, and the bob oscillator phase.
/// Card 26 feedback: also the measured ground speed (root displacement / dt,
/// written by locomotion_feel, read by sync ONE FRAME LATER).
#[derive(Component, Default)]
pub(crate) struct FeelState {
    pub(crate) lean: f32,
    pub(crate) bob: f32,
    pub(crate) phase: f32,
    pub(crate) speed: Option<f32>,
    pub(crate) prev: Option<Vec3>,
}

// --- plugin ----------------------------------------------------------------

/// Facing + locomotion feel domain plugin.
pub struct FeelPlugin;

impl Plugin for FeelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, face_towards_heading.in_set(PresentationSet::Facing))
            .add_systems(Update, locomotion_feel.in_set(PresentationSet::Feel));
    }
}

// --- card 15: monster facing ----------------------------------------------

/// Turn any heading owner's model wrapper (monsters AND the player, card 18)
/// towards its logic-side `Heading` at a constant rate along the shortest arc.
/// Wrapper-local only — roots never rotate, physics never sees this.
fn face_towards_heading(
    time: Res<Time>,
    headings: Query<(&Heading, &Children)>,
    links: Query<&AnimLink>,
    mut wrappers: Query<&mut Transform>,
) {
    let dt = time.delta_secs();
    let max_step = MAX_TURN_RATE_DEG.to_radians() * dt;

    for (heading, owner_children) in &headings {
        // the wrapper child carrying the model + AnimLink
        let mut wrapper = None;
        for kid in owner_children.iter() {
            if links.contains(kid) {
                wrapper = Some(kid);
            }
        }
        let Some(wrapper) = wrapper else { continue };
        let Ok(mut tf) = wrappers.get_mut(wrapper) else {
            continue;
        };

        // model faces +Z at yaw 0 ⇒ forward(yaw) = (sin, cos) must equal
        // (dir.x, dir.y[=world +Z]) ⇒ yaw = atan2(dir.x, dir.z)
        let target_yaw = f32::atan2(heading.dir.x, heading.dir.y);
        let current_yaw = 2.0 * f32::atan2(tf.rotation.y, tf.rotation.w);
        let diff = wrap_pi(target_yaw - current_yaw);
        let step = diff.clamp(-max_step, max_step);
        if step != 0.0 {
            tf.rotation = Quat::from_rotation_y(wrap_pi(current_yaw + step));
        }
    }
}

/// Wrap an angle into (-π, π].
fn wrap_pi(a: f32) -> f32 {
    const TWO_PI: f32 = std::f32::consts::TAU;
    (a + std::f32::consts::PI).rem_euclid(TWO_PI) - std::f32::consts::PI
}

// --- card 24: locomotion feel ----------------------------------------------

/// Two garnish layers over locomotion, per wrapper (hero + monsters alike):
/// 1. lean — the body pitches forward into its movement, ramping with ground
///    speed up to LEAN_MAX_DEG at LEAN_REF_SPEED;
/// 2. bob — the body bounces at two footfalls per walk cycle via |sin|.
/// Runs AFTER face_towards_heading (which writes a pure absolute yaw each
/// frame), so composing `Ry(yaw) * Rx(pitch)` keeps that yaw extraction exact.
/// Reads WalkCycle + the measured FeelState speed (card 26: displacement
/// measurement replaced the static Player.speed/Chasing.speed reads).
fn locomotion_feel(
    time: Res<Time>,
    game: Res<State<GameState>>,
    roots: Query<(&Transform, &WalkCycle, &Children)>,
    links: Query<&AnimLink>,
    // Without<WalkCycle>: wrappers never carry it (only roots do), which makes
    // the two Transform accesses provably disjoint (Bevy B0001)
    mut wrappers: Query<(&mut Transform, &mut FeelState), Without<WalkCycle>>,
) {
    let in_game = *game.get() == GameState::Playing;
    let dt = time.delta_secs();
    for (root_tf, walk, owner_children) in &roots {
        let Some(wrapper) = owner_children.iter().find(|kid| links.contains(*kid)) else {
            continue;
        };
        let Ok((mut tf, mut state)) = wrappers.get_mut(wrapper) else {
            continue;
        };
        // Measure ACTUAL ground speed from root displacement (card 26
        // feedback): static speed components go blind to sprinting and any
        // future speed source; displacement cannot lie. Sync reads this value
        // one frame later (chain order: feel runs after sync).
        let measured = match (state.prev, dt > 0.0) {
            (Some(prev), true) => {
                ((root_tf.translation - prev).length() / dt).min(20.0) // R-restart teleport guard
            }
            _ => 0.0,
        };
        state.prev = Some(root_tf.translation);
        state.speed = Some(measured);
        // bob cadence follows the active state clip (card 25: run cycles fast)
        let running = links
            .get(wrapper)
            .ok()
            .and_then(|link| link.current)
            .is_some_and(|c| c == anim::AnimStateId::Run);
        let cycle_secs = if running {
            RUN_CYCLE_SECS
        } else {
            WALK_CYCLE_SECS
        };

        let moving = in_game && walk.playing && measured > 0.0;
        let authored = if running {
            anim::RUN_CLIP_AUTHORED_SPEED
        } else {
            anim::WALK_CLIP_AUTHORED_SPEED
        };
        let walk_rate = (measured / authored).clamp(0.5, 4.0);

        let lean_target = if moving {
            LEAN_MAX_DEG * (measured / LEAN_REF_SPEED).clamp(0.0, 1.0)
        } else {
            0.0
        };
        state.lean += (lean_target - state.lean) * (LEAN_RESPONSE * dt).min(1.0);
        let yaw = 2.0 * f32::atan2(tf.rotation.y, tf.rotation.w);
        tf.rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-state.lean.to_radians());

        if moving {
            // two footfalls per cycle, rate-scaled to match the clip playback
            state.phase += dt * std::f32::consts::TAU * (2.0 / cycle_secs) * walk_rate;
        }
        let bob_target = if moving {
            state.phase.sin().abs() * BOB_AMP
        } else {
            0.0
        };
        state.bob += (bob_target - state.bob) * (BOB_RESPONSE * dt).min(1.0);
        tf.translation.y = MODEL_Y_OFFSET + state.bob;
    }
}
