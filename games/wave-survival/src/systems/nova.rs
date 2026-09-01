//! NovaSlash: the Q AoE blast. Capability card 9 (docs/capability-cards.md);
//! card 26 moved the key Shift -> Q (Shift now carries the sprint).
//! Interface: Q (in) + player Transform/NovaAttack + monster Hp/Transform
//!   (out: Hp.hp, Visual.flash, NovaAttack.cooldown, NovaFired message).
//! Behavior: the nova ticks its own cooldown; pressing Q when ready deals a
//!   flat NOVA_DAMAGE to every monster within NOVA_RADIUS (no falloff — an AoE
//!   is full damage inside the circle), fires exactly one `NovaFired`, and rearms.
//! VFX separation: this module is pure gameplay logic (headless-testable); the
//!   hanabi shockwave lives in plugins::vfx, which consumes `NovaFired`.
//!
//! Note (Bevy 0.19): buffered events were renamed to messages — `#[derive(Message)]`,
//! `MessageWriter`, `add_message`. The observer-style `Event` trait is a different thing.

use bevy::prelude::*;

use crate::components::{Monster, NovaAttack, Player};
use crate::resources::Balance;
use crate::systems::damage::{DamageRequest, DamageSource};

/// Nova defaults (GDD number table: 半径 1.6 / 伤害 60 / CD 5s); all three are
/// Balance-tunable at run time (card 11).
pub const NOVA_RADIUS: f32 = 1.6;
pub const NOVA_DAMAGE: f32 = 60.0;
pub const NOVA_COOLDOWN: f32 = 5.0;

/// Fired exactly once per nova blast at the player's position (VFX hook, card 9).
#[derive(Message)]
pub struct NovaFired {
    pub at: Vec3,
}

/// The nova blast: tick cooldown, then fire every monster in radius once ready.
/// Card 26: the key moved Shift -> Q so Shift can carry the sprint; E stays
/// reserved for the planned dash skill.
pub fn nova_slash(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    balance: Res<Balance>,
    mut nova_fired: MessageWriter<NovaFired>,
    mut wrt: MessageWriter<DamageRequest>,
    mut player: Query<(&mut NovaAttack, &Transform), With<Player>>,
    monsters: Query<(Entity, &Transform), With<Monster>>,
) {
    // Tick the nova's own cooldown (independent of the melee slash).
    let dt = time.delta_secs();
    let Ok((mut nova, tf)) = player.single_mut() else {
        return;
    };
    nova.cooldown = (nova.cooldown - dt).max(0.0);
    if !keys.pressed(KeyCode::KeyQ) {
        return;
    }
    if nova.cooldown > 0.0 {
        return;
    }

    nova.cooldown = balance.nova_cooldown;
    let origin = tf.translation;
    let radius_sq = balance.nova_radius * balance.nova_radius;
    let mut hits = 0;
    for (entity, mtf) in monsters.iter() {
        let dx = mtf.translation.x - origin.x;
        let dz = mtf.translation.z - origin.z;
        if dx * dx + dz * dz <= radius_sq {
            // Full damage inside the circle, no falloff — emit a request, let
            // the single apply_damage in GameSet::Resolve settle it.
            wrt.write(DamageRequest {
                target: entity,
                amount: balance.nova_damage,
                source: DamageSource::Nova,
            });
            hits += 1;
        }
    }

    nova_fired.write(NovaFired { at: origin });
    info!(
        "[nova] blast at ({:.1}, {:.1}), hit {hits} monsters",
        origin.x, origin.z
    );
}
