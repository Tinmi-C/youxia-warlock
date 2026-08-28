//! VfxPlugin: card 9 (golden shockwave) upgraded by card 23 NovaJuice — the
//! Nova burst is now a LAYERED explosion on every [`NovaFired`]:
//!   ring    : the original radial shockwave, retuned (speed/size variance,
//!             gold -> orange -> ember color fade, shrinking particles)
//!   sparks  : a few bright chunks thrown upward, falling under gravity
//!   ground  : a fast flat flash ring hugging the floor (pure "pop" accent)
//!   flash   : a 0.1 s white core flash (the hit "accent")
//! plus a 0.15 s camera shake (deterministic jitter, ease-out squared; the
//! camera is static, so the home position is captured lazily and restored —
//! camera.rs itself stays untouched).
//! All bevy_hanabi GPU particles; mounted only on the real app (`build_app`)
//! so headless tests carry no renderer dependency. API usage verified against
//! the bevy_hanabi-0.19 sources (firework example: uniform ranges, rand
//! vectors, AccelModifier gravity; one-shot SpawnerSettings + reset()).

use bevy::prelude::*;
// Explicit imports: bevy's prelude also exports a `Gradient`, so the hanabi
// prelude glob would be ambiguous. Verified against bevy_hanabi-0.19 sources.
use bevy_hanabi::{
    AccelModifier, Attribute, ColorOverLifetimeModifier, EffectAsset, EffectSpawner, ExprWriter,
    Gradient, HanabiPlugin, LinearDragModifier, MotionIntegration, ParticleEffect,
    SetAttributeModifier, SetPositionCircleModifier, ShapeDimension, SizeOverLifetimeModifier,
    SpawnerSettings, VectorType,
};

use crate::systems::nova::NovaFired;

/// Camera shake total duration (card 23 acceptance: "felt, not dizzy").
const SHAKE_SECS: f32 = 0.15;
/// Peak shake offset in world units, scaled by trauma^2 (ease-out).
const SHAKE_AMPLITUDE: f32 = 0.12;

pub struct VfxPlugin;

impl Plugin for VfxPlugin {
    fn build(&self, app: &mut App) {
        // HanabiPlugin registers Assets<EffectAsset> + the particle render
        // extraction; without it the asset store does not exist and our
        // Startup spawner panics (found on first real-machine run).
        app.init_resource::<CameraShake>()
            .add_plugins(HanabiPlugin)
            .add_systems(Startup, spawn_nova_effects)
            .add_systems(
                Update,
                (replay_on_nova, arm_camera_shake, apply_camera_shake).chain(),
            );
    }
}

/// Trauma-based camera shake (card 23). `home` captures the static camera's
/// rest position on first use so every burst restores it exactly.
#[derive(Resource, Default)]
struct CameraShake {
    trauma: f32,
    home: Option<Vec3>,
}

// --- effect assets (card 23: one function per layer) ------------------------

fn base_once(count: f32) -> SpawnerSettings {
    // One-shot burst, manually triggered per nova (with_emit_on_start(false)).
    SpawnerSettings::once(count.into()).with_emit_on_start(false)
}

fn init_circle(writer: &ExprWriter, radius: f32) -> SetPositionCircleModifier {
    SetPositionCircleModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        axis: writer.lit(Vec3::Y).expr(),
        radius: writer.lit(radius).expr(),
        dimension: ShapeDimension::Surface,
    }
}

/// The main radial shockwave (card 9 origin, card 23 retune): random speed
/// spread, shrinking particles, three-stage color fade.
fn nova_ring_asset(effects: &mut Assets<EffectAsset>) -> Handle<EffectAsset> {
    let writer = ExprWriter::new();

    let age = writer.lit(0.).expr();
    let init_age = SetAttributeModifier::new(Attribute::AGE, age);
    let lifetime = writer.lit(0.55).expr();
    let init_lifetime = SetAttributeModifier::new(Attribute::LIFETIME, lifetime);
    let init_pos = init_circle(&writer, 0.2);

    // Velocity: radially outward in XZ, speed randomized per particle.
    let dir = writer.attr(Attribute::POSITION) * writer.lit(Vec3::new(1., 0., 1.));
    let speed = writer.lit(4.5).uniform(writer.lit(6.5));
    let init_vel =
        SetAttributeModifier::new(Attribute::VELOCITY, (dir.normalized() * speed).expr());

    let drag = LinearDragModifier::new(writer.lit(3.5).expr());

    let mut color = Gradient::new();
    color.add_key(0.0, Vec4::new(1.0, 0.9, 0.45, 1.0));
    color.add_key(0.45, Vec4::new(1.0, 0.5, 0.18, 0.85));
    color.add_key(1.0, Vec4::new(0.75, 0.2, 0.08, 0.0));

    let mut size = Gradient::new();
    size.add_key(0.0, Vec3::ONE * 0.10);
    size.add_key(1.0, Vec3::ONE * 0.03);

    effects.add(
        EffectAsset::new(1024, base_once(160.0), writer.finish())
            .with_name("nova_ring")
            .init(init_pos)
            .init(init_vel)
            .init(init_age)
            .init(init_lifetime)
            .update(drag)
            .render(ColorOverLifetimeModifier::new(color))
            .render(SizeOverLifetimeModifier {
                gradient: size,
                screen_space_size: false,
            }),
    )
}

/// Bright chunks thrown upward, arcing back down under gravity (firework
/// example idiom: rand direction biased up, uniform speed, AccelModifier).
fn nova_sparks_asset(effects: &mut Assets<EffectAsset>) -> Handle<EffectAsset> {
    let writer = ExprWriter::new();

    let age = writer.lit(0.).expr();
    let init_age = SetAttributeModifier::new(Attribute::AGE, age);
    let lifetime = writer.lit(0.5).uniform(writer.lit(0.8)).expr();
    let init_lifetime = SetAttributeModifier::new(Attribute::LIFETIME, lifetime);
    let init_pos = init_circle(&writer, 0.12);

    // Mostly-up random direction with random speed.
    let dir = writer.rand(VectorType::VEC3F) * writer.lit(Vec3::new(1.0, 1.8, 1.0))
        + writer.lit(Vec3::Y * 1.4);
    let speed = writer.lit(2.2).uniform(writer.lit(3.6));
    let init_vel =
        SetAttributeModifier::new(Attribute::VELOCITY, (dir.normalized() * speed).expr());

    // Gravity pulls the sparks back down.
    let accel = AccelModifier::new(writer.lit(Vec3::Y * -9.0).expr());

    let mut color = Gradient::new();
    color.add_key(0.0, Vec4::new(1.0, 0.85, 0.3, 1.0));
    color.add_key(1.0, Vec4::new(1.0, 0.45, 0.1, 0.0));

    effects.add(
        EffectAsset::new(512, base_once(30.0), writer.finish())
            .with_name("nova_sparks")
            .init(init_pos)
            .init(init_vel)
            .init(init_age)
            .init(init_lifetime)
            .update(accel)
            .render(ColorOverLifetimeModifier::new(color))
            .render(SizeOverLifetimeModifier {
                gradient: Gradient::constant(Vec3::ONE * 0.05),
                screen_space_size: false,
            }),
    )
}

/// A fast flat flash ring hugging the floor: expands hard, decelerates hard,
/// gone in a blink (the "pop" accent that sells the ground impact).
fn nova_ground_ring_asset(effects: &mut Assets<EffectAsset>) -> Handle<EffectAsset> {
    let writer = ExprWriter::new();

    let age = writer.lit(0.).expr();
    let init_age = SetAttributeModifier::new(Attribute::AGE, age);
    let lifetime = writer.lit(0.28).expr();
    let init_lifetime = SetAttributeModifier::new(Attribute::LIFETIME, lifetime);
    let init_pos = init_circle(&writer, 0.25);

    let dir = writer.attr(Attribute::POSITION) * writer.lit(Vec3::new(1., 0., 1.));
    let speed = writer.lit(6.0).uniform(writer.lit(7.5));
    let init_vel =
        SetAttributeModifier::new(Attribute::VELOCITY, (dir.normalized() * speed).expr());

    // Hard drag: the ring slams outward then almost stops.
    let drag = LinearDragModifier::new(writer.lit(9.0).expr());

    let mut color = Gradient::new();
    color.add_key(0.0, Vec4::new(1.0, 0.95, 0.7, 1.0));
    color.add_key(1.0, Vec4::new(1.0, 0.7, 0.2, 0.0));

    effects.add(
        EffectAsset::new(512, base_once(90.0), writer.finish())
            .with_name("nova_ground_ring")
            .init(init_pos)
            .init(init_vel)
            .init(init_age)
            .init(init_lifetime)
            .update(drag)
            .render(ColorOverLifetimeModifier::new(color))
            .render(SizeOverLifetimeModifier {
                gradient: Gradient::constant(Vec3::ONE * 0.045),
                screen_space_size: false,
            }),
    )
}

/// The 0.1 s white core flash — the hit accent. Nearly stationary particles
/// with a big-to-small size fade; no velocity modifier needed (defaults zero).
fn nova_center_flash_asset(effects: &mut Assets<EffectAsset>) -> Handle<EffectAsset> {
    let writer = ExprWriter::new();

    let age = writer.lit(0.).expr();
    let init_age = SetAttributeModifier::new(Attribute::AGE, age);
    let lifetime = writer.lit(0.1).expr();
    let init_lifetime = SetAttributeModifier::new(Attribute::LIFETIME, lifetime);
    let init_pos = init_circle(&writer, 0.06);

    let mut color = Gradient::new();
    color.add_key(0.0, Vec4::new(1.0, 1.0, 1.0, 0.95));
    color.add_key(1.0, Vec4::new(1.0, 0.95, 0.8, 0.0));

    let mut size = Gradient::new();
    size.add_key(0.0, Vec3::ONE * 0.40);
    size.add_key(1.0, Vec3::ONE * 0.15);

    effects.add(
        EffectAsset::new(512, base_once(24.0), writer.finish())
            .with_name("nova_center_flash")
            // stationary by design: no VELOCITY attribute, so opt out of motion
            // integration entirely (silences hanabi's missing-VELOCITY warning)
            .with_motion_integration(MotionIntegration::None)
            .init(init_pos)
            .init(init_age)
            .init(init_lifetime)
            .render(ColorOverLifetimeModifier::new(color))
            .render(SizeOverLifetimeModifier {
                gradient: size,
                screen_space_size: false,
            }),
    )
}

/// Layer heights above the floor; the burst point itself comes from the event.
fn layer_height(name: &Name) -> f32 {
    match name.as_str() {
        "nova_sparks" => 0.30,
        "nova_center_flash" => 0.45,
        "nova_ground_ring" => 0.06,
        _ => 0.15, // nova_ring
    }
}

fn spawn_nova_effects(mut commands: Commands, mut effects: ResMut<Assets<EffectAsset>>) {
    for asset in [
        nova_ring_asset(&mut effects),
        nova_sparks_asset(&mut effects),
        nova_ground_ring_asset(&mut effects),
        nova_center_flash_asset(&mut effects),
    ] {
        commands.spawn((ParticleEffect::new(asset), Name::new("nova-layer")));
    }
    info!("[vfx] nova layered burst registered (ring/sparks/ground/flash)");
}

fn replay_on_nova(
    mut messages: MessageReader<NovaFired>,
    mut effects: Query<(&mut EffectSpawner, &mut Transform, &Name), With<ParticleEffect>>,
) {
    for fired in messages.read() {
        for (mut spawner, mut tf, name) in &mut effects {
            tf.translation = Vec3::new(fired.at.x, layer_height(name), fired.at.z);
            spawner.reset(); // emit one burst per layer at the (moved) emitter
        }
        info!("[vfx] nova burst at ({:.1}, {:.1})", fired.at.x, fired.at.z);
    }
}

// --- camera shake (card 23) -------------------------------------------------

fn arm_camera_shake(mut messages: MessageReader<NovaFired>, mut shake: ResMut<CameraShake>) {
    for _ in messages.read() {
        shake.trauma = 1.0;
    }
}

/// Deterministic jitter (sin/cos at mismatched frequencies — no RNG keeps the
/// motion reproducible); squared trauma gives an ease-out so the shake dies
/// smoothly instead of cutting off. The static camera's rest position is
/// captured once and every burst restores it exactly when trauma reaches 0.
fn apply_camera_shake(
    time: Res<Time>,
    mut shake: ResMut<CameraShake>,
    mut cameras: Query<&mut Transform, With<Camera3d>>,
) {
    if shake.trauma <= 0.0 {
        return;
    }
    let Ok(mut tf) = cameras.single_mut() else {
        return;
    };
    let home = *shake.home.get_or_insert(tf.translation);
    shake.trauma = (shake.trauma - time.delta_secs() / SHAKE_SECS).max(0.0);
    let t = shake.trauma * shake.trauma;
    let phase = time.elapsed_secs();
    let jitter = Vec3::new((phase * 67.0).sin(), 0.0, (phase * 53.0).cos()) * (SHAKE_AMPLITUDE * t);
    tf.translation = home + jitter;
}
