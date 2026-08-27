//! WaveSystem: the core wave-survival loop. Capability card 3 (docs/capability-cards.md)
//! + card 10 EnemyVariants (composition & per-kind stats).
//! State is derived, not stored ("the world is the truth", m2 convention):
//!   enemies > 0            -> combat (do nothing)
//!   enemies == 0, timer>0  -> rest between waves (count down)
//!   enemies == 0, timer<=0 -> spawn next wave (n += 1, then re-arm timer)
//! Formulas (mixed growth, inherited from m2): count = 2+n, speed = 1.1+0.08n, hp = 30*(1+0.4n).
//! Composition (card 10): runners from wave 3, tanks from wave 5; the grunt
//! remainder conserves the total. Each variant's stats = baseline x its
//! multipliers (components::MonsterKind).

use bevy::prelude::*;
use bevy_rapier3d::prelude::{Collider, GravityScale, LockedAxes, RigidBody, Velocity};

use crate::components::{Chasing, Hp, Monster, MonsterKind, Visual, WalkCycle};
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

/// Runner slots in wave `n` (card 10): none before wave 3, then floor((n-1)/2),
/// capped at 3. w3=1, w4=1, w5=2, w6=2, w7+=3.
pub fn runner_count(n: u32) -> u32 {
    if n < 3 {
        0
    } else {
        ((n - 1) / 2).min(3)
    }
}

/// Tank slots in wave `n` (card 10): none before wave 5, then floor(n/5),
/// capped at 2. w5..w9=1, w10+=2.
pub fn tank_count(n: u32) -> u32 {
    if n < 5 {
        0
    } else {
        (n / 5).min(2)
    }
}

/// Deterministic wave composition: `wave_count(n)` kinds in ring order —
/// grunts first, then runners, then tanks. Sum always equals `wave_count(n)`.
pub fn kinds_for_wave(n: u32) -> Vec<MonsterKind> {
    let total = wave_count(n) as usize;
    let grunt = total - runner_count(n) as usize - tank_count(n) as usize;
    let mut kinds = Vec::with_capacity(total);
    kinds.extend(std::iter::repeat_n(MonsterKind::Grunt, grunt));
    kinds.extend(std::iter::repeat_n(
        MonsterKind::Runner,
        runner_count(n) as usize,
    ));
    kinds.extend(std::iter::repeat_n(
        MonsterKind::Tank,
        tank_count(n) as usize,
    ));
    kinds
}

/// Spawn one wave-`n` monster of `kind` at `at`: baseline stats x kind multipliers.
fn spawn_wave_monster(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    at: Vec3,
    n: u32,
    kind: MonsterKind,
) {
    commands.spawn((
        Monster,
        kind,
        Hp::full(wave_hp(n) * kind.hp_mul()),
        Chasing {
            speed: wave_speed(n) * kind.speed_mul(),
        },
        Visual { flash: 0.0 },
        // card 13: monsters always chase while Playing, so they spawn walking;
        // clear_walk_on_pause owns the flag outside Playing.
        WalkCycle { playing: true },
        // Visual-pass fix #4: kinematic-vs-kinematic never generates contact
        // response (rapier design, see card 4 note) — monsters phased through
        // the player and each other. Dynamic bodies with zero gravity and
        // locked rotation keep the planar chase intact while letting crowds
        // shove instead of overlapping.
        RigidBody::Dynamic,
        GravityScale(0.0),
        LockedAxes::ROTATION_LOCKED,
        Collider::ball(kind.cube_size() / 2.0),
        Velocity::zero(),
        Mesh3d(meshes.add(Cuboid::new(
            kind.cube_size(),
            kind.cube_size(),
            kind.cube_size(),
        ))),
        MeshMaterial3d(materials.add(kind.color())),
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
    let kinds = kinds_for_wave(n);
    info!(
        "[wave] wave {n} incoming: {} enemies ({} grunts / {} runners / {} tanks), \
         speed {:.2}, hp {:.0}",
        kinds.len(),
        kinds.iter().filter(|k| **k == MonsterKind::Grunt).count(),
        kinds.iter().filter(|k| **k == MonsterKind::Runner).count(),
        kinds.iter().filter(|k| **k == MonsterKind::Tank).count(),
        wave_speed(n),
        wave_hp(n)
    );
    let count = kinds.len() as u32;
    for (i, kind) in kinds.into_iter().enumerate() {
        let i = i as f32;
        let angle = i / count as f32 * std::f32::consts::TAU;
        let at = Vec3::new(SPAWN_RADIUS * angle.cos(), 0.5, SPAWN_RADIUS * angle.sin());
        spawn_wave_monster(&mut commands, &mut meshes, &mut materials, at, n, kind);
    }
    wave.timer = WAVE_BREAK; // re-arm the rest that follows this wave (GDD: 3s between every wave)
}
