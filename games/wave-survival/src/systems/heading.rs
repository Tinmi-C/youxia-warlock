//! Card 15 MonsterFacing (data half): observe monster displacement and expose
//! it as an XZ unit vector (`Heading`). Observation precedent: card 12's
//! `update_walk_cycle` watches `move_player`'s result instead of touching it —
//! `enemy_chase` keeps its scalar-speed interface untouched. The presentation
//! plugin turns `Heading` into wrapper yaw; physics stays out entirely
//! (monsters are rotation-locked dynamics, root transforms never rotate).
//! Card 18 PlayerFacing: the player mirrors the same displacement rules (with
//! no chase target to seed from — its spawn-time heading just persists).

use bevy::prelude::*;

use crate::components::{Heading, Monster, Player, PrevTranslation};

/// Movement slower than this counts as standing still (same epsilon as the
/// walk-cycle threshold); heading holds its last value while stationary.
const MIN_SPEED: f32 = 0.02;

pub fn derive_heading(
    time: Res<Time>,
    mut commands: Commands,
    mut player: Query<
        (
            Entity,
            &Transform,
            Option<&mut PrevTranslation>,
            Option<&mut Heading>,
        ),
        (With<Player>, Without<Monster>),
    >,
    mut monsters: Query<
        (
            Entity,
            &Transform,
            Option<&mut PrevTranslation>,
            Option<&mut Heading>,
        ),
        (With<Monster>, Without<Player>),
    >,
) {
    let dt = time.delta_secs();
    // Seed reference for first sight: face towards the player when known.
    let player_at = player.iter().next().map(|(_, tf, _, _)| tf.translation);

    // --- card 18: player heading (mirror of the monster rules, no seed target).
    // The player anchor is OWNED by card 12's update_walk_cycle, which runs
    // after us: we READ it (it still holds last frame's post-move position, so
    // our delta is this frame's move) but never write it — writing here would
    // zero the walk-cycle's delta and kill the walk animation. Outside Playing
    // clear_walk_on_pause re-anchors it, so resuming cannot cause a fake flip.
    if let Ok((_entity, tf, prev, heading)) = player.single_mut() {
        let here = tf.translation;
        if let (Some(prev), Some(mut heading)) = (prev, heading) {
            let delta = Vec2::new(here.x - prev.v.x, here.z - prev.v.z);
            if delta.length() / dt.max(f32::EPSILON) > MIN_SPEED {
                heading.dir = delta.normalize();
            }
            // slow frame → hold dir (no write-back: the anchor is not ours)
        }
        // no seeding: update_walk_cycle owns player anchor setup
    }

    for (entity, tf, prev, heading) in &mut monsters {
        let here = tf.translation;
        let Some(mut prev) = prev else {
            // First frame in world: seed at the current spot facing the player,
            // so freshly spawned ring monsters look inward instead of all +Z.
            commands.entity(entity).insert((
                PrevTranslation { v: here },
                Heading {
                    dir: unit_towards(here, player_at),
                },
            ));
            continue;
        };

        let delta = Vec2::new(here.x - prev.v.x, here.z - prev.v.z);
        if delta.length() / dt.max(f32::EPSILON) > MIN_SPEED {
            let dir = delta.normalize();
            match heading {
                Some(mut h) => h.dir = dir,
                None => {
                    commands.entity(entity).insert(Heading { dir });
                }
            }
        }
        // Hold dir on slow frames (write-back of the position always happens).
        prev.v = here;
    }
}

/// Unit XZ direction from `at` to `player`, falling back to +Z-facing north
/// when positions coincide or no player exists (degenerate seeds must not NaN).
fn unit_towards(at: Vec3, player_at: Option<Vec3>) -> Vec2 {
    player_at
        .map(|p| Vec2::new(p.x - at.x, p.z - at.z))
        .filter(|d| d.length_squared() > f32::EPSILON)
        .map(Vec2::normalize)
        .unwrap_or(Vec2::Y)
}
