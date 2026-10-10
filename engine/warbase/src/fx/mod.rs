//! Generic battle FX (WX4, warden FX1/FX2 lifted): health bars, hit flash,
//! blast rings, death shells — the visual primitives every battle game
//! shares. Data sources are base components (Hp/MaxHp) and the `FxBlast`
//! message; bar styling is game-supplied via `HealthBarStyle` (data-driven).
//! Game-identity visuals — status tints, unit badges, heavy skill VFX —
//! stay game-side by design (they consume DamageDealt/skill events).

use bevy::prelude::*;

use crate::unit::{Hp, MaxHp};

// --- Health bars (warden FX1 generalized) ----------------------------------------

/// Marker on a unit's health bar (a child entity of the unit).
#[derive(Component, Debug)]
pub struct HealthBar {
    pub target: Entity,
    /// Full-hp width in world units (from the style or the default).
    pub width: f32,
}

/// Optional styling a game inserts on a unit BEFORE first damage: bar width
/// and color (e.g. wider + golden for elites/bosses). Absent => defaults.
#[derive(Component, Debug, Clone, Copy)]
pub struct HealthBarStyle {
    pub width: f32,
    pub color: Color,
}

/// Default bar width / color / height offset.
pub const BAR_WIDTH: f32 = 0.9;
pub const BAR_COLOR: Color = Color::srgb(0.3, 0.9, 0.35);
pub const BAR_HEIGHT: f32 = 0.9;

/// Maintain health bars: a bar appears above a unit only once it is damaged
/// (no full-health spam), width == hp/max_hp x style width, hidden again at
/// full hp. Despawns bars whose target died.
pub fn refresh_health_bars(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    units: Query<(Entity, &Hp, &MaxHp, Option<&HealthBarStyle>)>,
    mut bars: Query<(Entity, &HealthBar, &mut Transform, &mut Visibility)>,
) {
    // Ensure every damaged unit has a bar.
    for (uid, hp, max, style) in &units {
        if hp.0 >= max.0 || hp.0 <= 0.0 {
            continue;
        }
        if bars.iter().any(|(_, b, _, _)| b.target == uid) {
            continue;
        }
        let (width, color) = match style {
            Some(s) => (s.width, s.color),
            None => (BAR_WIDTH, BAR_COLOR),
        };
        commands.entity(uid).with_children(|p| {
            p.spawn((
                HealthBar { target: uid, width },
                Mesh3d(meshes.add(Cuboid::new(1.0, 0.07, 0.05))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: color,
                    unlit: true,
                    ..default()
                })),
                Visibility::Hidden,
                Transform::from_xyz(0.0, BAR_HEIGHT, 0.0),
            ));
        });
    }
    // Sync visibility + width; drop bars whose target is gone.
    for (bid, bar, mut tf, mut vis) in &mut bars {
        let Ok((_, hp, max, _)) = units.get(bar.target) else {
            commands.entity(bid).despawn();
            continue;
        };
        let damaged = hp.0 < max.0 && hp.0 > 0.0;
        *vis = if damaged { Visibility::Visible } else { Visibility::Hidden };
        if damaged {
            tf.scale.x = (hp.0 / max.0).max(0.01) * bar.width;
        }
    }
}

// --- Hit flash (warden FX2) -------------------------------------------------------

/// A white flash on a damaged unit; expires back to the original color.
#[derive(Component, Debug)]
pub struct HitFlash {
    pub timer: f32,
    pub original: Option<Color>,
}

/// The hit flash window (shared by every injector).
pub const HIT_FLASH_SECS: f32 = 0.15;

/// Tick hit flashes: capture the original color once, flash white, restore.
pub fn tick_hit_flash(
    time: Res<Time>,
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(Entity, &mut HitFlash, &MeshMaterial3d<StandardMaterial>)>,
) {
    for (eid, mut flash, mat) in &mut q {
        flash.timer -= time.delta_secs();
        if let Some(mut m) = materials.get_mut(&mat.0) {
            if flash.original.is_none() {
                flash.original = Some(m.base_color);
                m.base_color = Color::srgb(1.0, 1.0, 0.95);
            }
            if flash.timer <= 0.0 {
                if let Some(orig) = flash.original {
                    m.base_color = orig;
                }
                commands.entity(eid).remove::<HitFlash>();
            }
        }
    }
}

// --- Blast rings (warden FX2) -----------------------------------------------------

/// Observability message for area detonations: one per resolved impact with
/// a blast radius (games consume/emit their own equivalents and forward).
#[derive(Message, Debug, Clone, Copy)]
pub struct FxBlast {
    pub position: Vec3,
    pub radius: f32,
}

pub const BLAST_RING_SECS: f32 = 0.5;

/// Marker on an expanding AOE blast ring.
#[derive(Component, Debug)]
pub struct BlastRing {
    pub timer: f32,
    pub max_radius: f32,
}

/// On every blast message, spawn an expanding ring at the point.
pub fn spawn_blast_rings(
    mut blasts: MessageReader<FxBlast>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for blast in blasts.read() {
        if blast.radius <= 0.0 {
            continue;
        }
        commands.spawn((
            BlastRing {
                timer: BLAST_RING_SECS,
                max_radius: blast.radius,
            },
            Mesh3d(meshes.add(Circle::new(1.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(1.0, 0.7, 0.25, 0.6),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            })),
            Visibility::Visible,
            Transform::from_xyz(blast.position.x, 0.1, blast.position.z)
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        ));
    }
}

/// Expand + fade blast rings, despawn after BLAST_RING_SECS.
pub fn tick_blast_rings(
    time: Res<Time>,
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(Entity, &mut BlastRing, &mut Transform, &MeshMaterial3d<StandardMaterial>)>,
) {
    for (eid, mut ring, mut tf, mat) in &mut q {
        ring.timer -= time.delta_secs();
        let t = 1.0 - (ring.timer / BLAST_RING_SECS).clamp(0.0, 1.0); // 0->1
        let r = ring.max_radius * (0.2 + 0.8 * t);
        tf.scale = Vec3::splat(r);
        if let Some(mut m) = materials.get_mut(&mat.0) {
            m.base_color.set_alpha(0.6 * (1.0 - t));
        }
        if ring.timer <= 0.0 {
            commands.entity(eid).despawn();
        }
    }
}

// --- Death shells (warden FX2) ----------------------------------------------------

pub const DEATH_SHELL_SECS: f32 = 0.4;

/// Marker on a death shell (pure visual; the logic despawn already happened).
#[derive(Component, Debug)]
pub struct DeathShell {
    pub timer: f32,
}

/// Spawn a dissolving shell for a dying unit. Called right before the
/// despawn — rewards and timing stay in game logic; this only adds the
/// visual.
pub fn spawn_death_shell(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    tf: &Transform,
    color: Color,
) {
    commands.spawn((
        DeathShell {
            timer: DEATH_SHELL_SECS,
        },
        Mesh3d(meshes.add(Cuboid::new(0.7, 0.7, 0.7))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: color,
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Visibility::Visible,
        Transform::from_translation(tf.translation),
    ));
}

/// Shrink + fade death shells, despawn after DEATH_SHELL_SECS.
pub fn tick_death_shells(
    time: Res<Time>,
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(Entity, &mut DeathShell, &mut Transform, &MeshMaterial3d<StandardMaterial>)>,
) {
    for (eid, mut shell, mut tf, mat) in &mut q {
        shell.timer -= time.delta_secs();
        let t = (shell.timer / DEATH_SHELL_SECS).clamp(0.0, 1.0); // 1->0
        tf.scale = Vec3::splat(t.max(0.05));
        if let Some(mut m) = materials.get_mut(&mat.0) {
            m.base_color.set_alpha(t);
        }
        if shell.timer <= 0.0 {
            commands.entity(eid).despawn();
        }
    }
}

pub struct FxPlugin;

impl Plugin for FxPlugin {
    fn build(&self, app: &mut App) {
        // Headless apps (tests/harness) carry no RenderPlugin, but the FX
        // systems take the asset stores as params — init them (idempotent;
        // real renders already provide their own).
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_message::<FxBlast>()
            .add_systems(
                Update,
                (
                    refresh_health_bars,
                    tick_hit_flash,
                    spawn_blast_rings,
                    tick_blast_rings,
                    tick_death_shells,
                ),
            );
    }
}
