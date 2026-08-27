//! NovaSlash: the Shift AoE blast. Capability card 9 (docs/capability-cards.md).
//! Interface: Shift (in) + player Transform/NovaAttack + monster Hp/Transform
//!   (out: Hp.hp, Visual.flash, NovaAttack.cooldown, NovaFired message).
//! Behavior: the nova ticks its own cooldown; pressing Shift when ready deals a
//!   flat NOVA_DAMAGE to every monster within NOVA_RADIUS (no falloff — an AoE
//!   is full damage inside the circle), fires exactly one `NovaFired`, and rearms.
//! VFX separation: this module is pure gameplay logic (headless-testable); the
//!   hanabi shockwave lives in plugins::vfx, which consumes `NovaFired`.
//!
//! Note (Bevy 0.19): buffered events were renamed to messages — `#[derive(Message)]`,
//! `MessageWriter`, `add_message`. The observer-style `Event` trait is a different thing.

use bevy::prelude::*;

use crate::components::{Hp, Monster, NovaAttack, Player, Visual};
use crate::resources::Balance;

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

/// The Shift nova: tick cooldown, then blast every monster in radius once ready.
pub fn nova_slash(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    balance: Res<Balance>,
    mut nova_fired: MessageWriter<NovaFired>,
    mut player: Query<(&mut NovaAttack, &Transform), With<Player>>,
    mut monsters: Query<(&Transform, &mut Hp, &mut Visual), With<Monster>>,
) {
    // Tick the nova's own cooldown (independent of the melee slash).
    let dt = time.delta_secs();
    let Ok((mut nova, tf)) = player.single_mut() else {
        return;
    };
    nova.cooldown = (nova.cooldown - dt).max(0.0);
    if !(keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight)) {
        return;
    }
    if nova.cooldown > 0.0 {
        return;
    }

    nova.cooldown = balance.nova_cooldown;
    let origin = tf.translation;
    let radius_sq = balance.nova_radius * balance.nova_radius;
    let mut hits = 0;
    for (mtf, mut hp, mut visual) in &mut monsters {
        let dx = mtf.translation.x - origin.x;
        let dz = mtf.translation.z - origin.z;
        if dx * dx + dz * dz <= radius_sq {
            hp.hp -= balance.nova_damage; // full damage inside the circle, no falloff
            visual.flash = 1.0;
            hits += 1;
        }
    }

    nova_fired.write(NovaFired { at: origin });
    info!("[nova] blast at ({:.1}, {:.1}), hit {hits} monsters", origin.x, origin.z);
}
