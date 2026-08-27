//! Card 15 MonsterFacing (data half): observe monster displacement and expose
//! it as an XZ unit vector (`Heading`). Observation precedent: card 12's
//! `update_walk_cycle` watches `move_player`'s result instead of touching it —
//! `enemy_chase` keeps its scalar-speed interface untouched. The presentation
//! plugin turns `Heading` into wrapper yaw; physics stays out entirely
//! (monsters are rotation-locked dynamics, root transforms never rotate).

use bevy::prelude::*;

use crate::components::{Heading, Monster, Player, PrevTranslation};

/// Movement slower than this counts as standing still (same epsilon as the
/// walk-cycle threshold); heading holds its last value while stationary.
const MIN_SPEED: f32 = 0.02;

pub fn derive_heading(
    time: Res<Time>,
    mut commands: Commands,
    player: Query<&Transform, With<Player>>,
    mut monsters: Query<
        (
            Entity,
            &Transform,
            Option<&mut PrevTranslation>,
            Option<&mut Heading>,
        ),
        With<Monster>,
    >,
) {
    let dt = time.delta_secs();
    // Seed reference for first sight: face towards the player when known.
    let player_at = player.single().ok().map(|tf| tf.translation);

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
