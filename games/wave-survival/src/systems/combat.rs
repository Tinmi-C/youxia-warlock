//! Combat: PlayerAttack (melee slash). Capability card 2 (docs/capability-cards.md).
//! Interface: Space (in) + player Attack/Transform + monster Hp/Transform
//!   (out: Hp.hp, Visual.flash, Attack.cooldown).
//! Behavior: cooldown ticks down each frame; Space + cooldown ready slashes once;
//!   damage falls off linearly from full (<= 0.9) to zero (1.5); hit monsters flash.

use bevy::prelude::*;

use crate::components::{Attack, Hp, Monster, Player, Visual};

/// Slash tuning (GDD values, inherited from m2 CombatSystem).
pub const SLASH_DAMAGE: f32 = 34.0;
pub const SLASH_FULL_RADIUS: f32 = 0.9;
pub const SLASH_FAR_RADIUS: f32 = 1.5;
pub const SLASH_COOLDOWN: f32 = 0.45;

/// Damage as a function of horizontal distance `d` from the player.
/// `d <= FULL_RADIUS` → full; `FULL..=FAR` → linear falloff to 0; `> FAR` → 0.
pub fn damage_for(d: f32) -> f32 {
    if d <= SLASH_FULL_RADIUS {
        SLASH_DAMAGE
    } else if d <= SLASH_FAR_RADIUS {
        SLASH_DAMAGE * (SLASH_FAR_RADIUS - d) / (SLASH_FAR_RADIUS - SLASH_FULL_RADIUS)
    } else {
        0.0
    }
}

/// Melee slash: tick cooldown, then slash once when Space is held and ready.
pub fn player_attack(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut player: Query<(&mut Attack, &Transform), With<Player>>,
    mut monsters: Query<(&Transform, &mut Hp, &mut Visual), With<Monster>>,
) {
    let dt = time.delta_secs();

    // Tick cooldown and decide whether a slash fires this frame.
    let mut slash_origin = Vec3::ZERO;
    let mut slashing = false;
    if let Some((mut attack, tf)) = player.iter_mut().next() {
        attack.cooldown = (attack.cooldown - dt).max(0.0);
        if keys.pressed(KeyCode::Space) && attack.cooldown <= 0.0 {
            attack.cooldown = SLASH_COOLDOWN;
            slash_origin = tf.translation;
            slashing = true;
        }
    }
    if !slashing {
        return;
    }

    for (tf, mut hp, mut visual) in &mut monsters {
        let d = Vec2::new(
            tf.translation.x - slash_origin.x,
            tf.translation.z - slash_origin.z,
        )
        .length();
        let dmg = damage_for(d);
        if dmg > 0.0 {
            hp.hp -= dmg;
            visual.flash = 1.0;
            info!(
                "[combat] slash hit monster at d={d:.2}, dealt {dmg:.1}, hp now {:.1}",
                hp.hp
            );
        }
    }
}

/// Demo stub monsters so the running game has something to hit. Not part of
/// GamePlugin (tests spawn their own at exact distances); placeholder until
/// WaveSystem / EnemyChase (cards 3-4) replace them.
pub fn spawn_stub_monsters(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // One in the full-damage zone (0.5), one in the falloff zone (1.2).
    for x in [0.5_f32, 1.2] {
        commands.spawn((
            Monster,
            Hp { hp: 100.0 },
            Visual { flash: 0.0 },
            Mesh3d(meshes.add(Cuboid::new(0.6, 0.6, 0.6))),
            MeshMaterial3d(materials.add(Color::srgb(0.75, 0.2, 0.2))),
            Transform::from_xyz(x, 0.5, 0.0),
        ));
    }
    info!("[combat] spawned 2 stub monsters for melee testing");
}
