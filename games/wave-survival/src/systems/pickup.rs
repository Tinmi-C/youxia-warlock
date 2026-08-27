//! PickupDrop: monster kills drop a golden pickup; the player walks near to heal.
//! Capability card 6 (docs/capability-cards.md). Numbers from m2 (heal 10, arm 0.6s, dist 0.45).

use bevy::prelude::*;

use crate::components::{Hp, Pickup, Player};

pub const PICKUP_HEAL: f32 = 10.0;
pub const PICKUP_ARM: f32 = 0.6;
pub const PICKUP_DIST: f32 = 0.45;

/// Spawn a golden pickup at the given XZ position (used when a monster dies).
pub fn spawn_pickup(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    at: Vec3,
) {
    commands.spawn((
        Pickup { heal: PICKUP_HEAL, arm: PICKUP_ARM },
        Mesh3d(meshes.add(Cuboid::new(0.25, 0.25, 0.25))),
        MeshMaterial3d(materials.add(Color::srgb(0.95, 0.8, 0.25))),
        Transform::from_xyz(at.x, 0.25, at.z),
    ));
}

/// After `arm`, a pickup within `PICKUP_DIST` of the player heals (capped at max) and despawns.
pub fn pickup_drop(
    time: Res<Time>,
    mut commands: Commands,
    mut pickups: Query<(Entity, &mut Pickup, &Transform)>,
    mut player: Query<(&mut Hp, &Transform), With<Player>>,
) {
    let dt = time.delta_secs();
    let Ok((mut hp, player_tf)) = player.single_mut() else {
        return; // no player: nothing to heal
    };
    let p = player_tf.translation;

    for (e, mut pk, tf) in &mut pickups {
        pk.arm = (pk.arm - dt).max(0.0);
        if pk.arm <= 0.0 {
            let d = Vec2::new(tf.translation.x - p.x, tf.translation.z - p.z).length();
            if d <= PICKUP_DIST {
                hp.hp = (hp.hp + pk.heal).min(hp.max);
                commands.entity(e).despawn();
                info!("[pickup] healed to {:.0}", hp.hp);
            }
        }
    }
}
