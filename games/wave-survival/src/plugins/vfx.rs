//! VfxPlugin: the visual half of card 9 — a golden expanding shockwave ring on
//! every [`NovaFired`]. bevy_hanabi GPU particles; mounted only on the real app
//! (`build_app`) so headless tests carry no renderer dependency.
//! API usage follows the verified bevy_hanabi-0.19 examples (spawn_on_command /
//! firework): one-shot SpawnerSettings re-triggered via `EffectSpawner::reset()`.

use bevy::prelude::*;
// Explicit imports: bevy's prelude also exports a `Gradient`, so the hanabi
// prelude glob would be ambiguous. Verified against bevy_hanabi-0.19 sources.
use bevy_hanabi::{
    Attribute, ColorOverLifetimeModifier, EffectAsset, EffectSpawner, ExprWriter, Gradient,
    HanabiPlugin, LinearDragModifier, ParticleEffect, SetAttributeModifier,
    SetPositionCircleModifier, ShapeDimension, SizeOverLifetimeModifier, SpawnerSettings,
};

use crate::systems::nova::NovaFired;

pub struct VfxPlugin;

impl Plugin for VfxPlugin {
    fn build(&self, app: &mut App) {
        // HanabiPlugin registers Assets<EffectAsset> + the particle render
        // extraction; without it the asset store does not exist and our
        // Startup spawner panics (found on first real-machine run).
        app.add_plugins(HanabiPlugin)
            .add_systems(Startup, spawn_nova_shockwave_effect)
            .add_systems(Update, replay_shockwave_on_nova);
    }
}

/// Build the shockwave asset: a ring burst expanding toward `NOVA_RADIUS`.
fn nova_shockwave_asset(effects: &mut Assets<EffectAsset>) -> Handle<EffectAsset> {
    // One-shot burst, manually triggered per nova (with_emit_on_start(false)).
    let spawner = SpawnerSettings::once(140.0.into()).with_emit_on_start(false);

    let writer = ExprWriter::new();

    let age = writer.lit(0.).expr();
    let init_age = SetAttributeModifier::new(Attribute::AGE, age);
    let lifetime = writer.lit(0.7).expr();
    let init_lifetime = SetAttributeModifier::new(Attribute::LIFETIME, lifetime);

    // Particles start on a small circle around the player (XZ plane).
    let init_pos = SetPositionCircleModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        axis: writer.lit(Vec3::Y).expr(),
        radius: writer.lit(0.2).expr(),
        dimension: ShapeDimension::Surface,
    };

    // Velocity: radially outward in the XZ plane. POSITION was set by init_pos,
    // so flattening it to y=0 and normalizing gives each particle's outward dir.
    let dir = writer.attr(Attribute::POSITION) * writer.lit(Vec3::new(1., 0., 1.));
    let speed = writer.lit(5.0);
    let init_vel = SetAttributeModifier::new(Attribute::VELOCITY, (dir.normalized() * speed).expr());

    // Drag decelerates the ring as it expands (placeholder tuning).
    let drag = writer.lit(3.5).expr();
    let update_drag = LinearDragModifier::new(drag);

    // Golden, fading out over the ring's lifetime.
    let mut gradient = Gradient::new();
    gradient.add_key(0.0, Vec4::new(1.0, 0.85, 0.25, 1.0));
    gradient.add_key(1.0, Vec4::new(1.0, 0.85, 0.25, 0.0));

    effects.add(
        EffectAsset::new(4096, spawner, writer.finish())
            .with_name("nova_shockwave")
            .init(init_pos)
            .init(init_vel)
            .init(init_age)
            .init(init_lifetime)
            .update(update_drag)
            .render(ColorOverLifetimeModifier::new(gradient))
            .render(SizeOverLifetimeModifier {
                gradient: Gradient::constant(Vec3::ONE * 0.07),
                screen_space_size: false,
            }),
    )
}

fn spawn_nova_shockwave_effect(
    mut commands: Commands,
    mut effects: ResMut<Assets<EffectAsset>>,
) {
    let handle = nova_shockwave_asset(&mut effects);
    commands.spawn((ParticleEffect::new(handle), Name::new("nova-shockwave")));
    info!("[vfx] nova shockwave effect registered");
}

fn replay_shockwave_on_nova(
    mut messages: MessageReader<NovaFired>,
    mut effect: Query<(&mut EffectSpawner, &mut Transform), With<ParticleEffect>>,
) {
    for fired in messages.read() {
        if let Ok((mut spawner, mut tf)) = effect.single_mut() {
            tf.translation = Vec3::new(fired.at.x, 0.15, fired.at.z);
            spawner.reset(); // emit one burst at the (moved) emitter
            info!("[vfx] shockwave burst at ({:.1}, {:.1})", fired.at.x, fired.at.z);
        }
    }
}
