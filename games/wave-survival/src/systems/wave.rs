//! WaveSystem: the core wave-survival loop. Capability card 3 (docs/capability-cards.md).
//! State is derived, not stored ("the world is the truth", m2 convention):
//!   enemies > 0            -> combat (do nothing)
//!   enemies == 0, timer>0  -> rest between waves (count down)
//!   enemies == 0, timer<=0 -> spawn next wave (n += 1, then re-arm timer)
//! Formulas (mixed growth, inherited from m2): count = 2+n, speed = 1.1+0.08n, hp = 30*(1+0.4n).

use bevy::prelude::*;
use bevy_rapier3d::prelude::{Collider, RigidBody, Velocity};

use crate::components::{Chasing, Hp, Monster, Visual};
use crate::resources::{Wave, WAVE_BREAK};

/// Spawn ring radius (world units, XZ plane). Placeholder until an arena exists.
pub const SPAWN_RADIUS: f32 = 3.0;

/// Monster count for wave `n` (n is 1-based). Wave 1 = 3, wave 5 = 7.
pub fn wave_count(n: u32) -> u32 {
    2 + n
}

/// Chase speed for wave `n`. Wave 1 = 1.18, wave 10 = 1.9.
pub fn wave_speed(n: u32) -> f32 {
    1.1 + 0.08 * n as f32
}

/// Monster HP for wave `n`. Wave 1 = 42, wave 5 = 90.
pub fn wave_hp(n: u32) -> f32 {
    30.0 * (1.0 + 0.4 * n as f32)
}

/// Spawn one wave-`n` monster at `at`. Kinematic rigid body (card 4: chase via velocity).
fn spawn_wave_monster(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    at: Vec3,
    n: u32,
) {
    commands.spawn((
        Monster,
        Hp { hp: wave_hp(n) },
        Chasing { speed: wave_speed(n) },
        Visual { flash: 0.0 },
        RigidBody::KinematicVelocityBased,
        Collider::ball(0.3),
        Velocity::zero(),
        Mesh3d(meshes.add(Cuboid::new(0.6, 0.6, 0.6))),
        MeshMaterial3d(materials.add(Color::srgb(0.75, 0.2, 0.2))),
        Transform::from_translation(at),
    ));
}

/// Core wave loop: count monsters, rest between waves, spawn the next wave.
pub fn wave_system(
    time: Res<Time>,
    mut wave: ResMut<Wave>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    monsters: Query<(), With<Monster>>,
) {
    let dt = time.delta_secs();

    if monsters.iter().count() > 0 {
        return; // combat
    }
    if wave.timer > 0.0 {
        wave.timer = (wave.timer - dt).max(0.0);
        return; // rest between waves
    }

    // Spawn the next wave.
    wave.n += 1;
    let n = wave.n;
    info!(
        "[wave] wave {n} incoming: {} enemies, speed {:.2}, hp {:.0}",
        wave_count(n),
        wave_speed(n),
        wave_hp(n)
    );
    let count = wave_count(n);
    for i in 0..count {
        let angle = i as f32 / count as f32 * std::f32::consts::TAU;
        let at = Vec3::new(
            SPAWN_RADIUS * angle.cos(),
            0.5,
            SPAWN_RADIUS * angle.sin(),
        );
        spawn_wave_monster(&mut commands, &mut meshes, &mut materials, at, n);
    }
    wave.timer = WAVE_BREAK; // re-arm the rest that follows this wave (GDD: 3s between every wave)
}
