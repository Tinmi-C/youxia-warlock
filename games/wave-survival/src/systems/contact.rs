//! CombatContact: monster bites the player + death. Capability card 5 (docs/capability-cards.md).
//! Distance-based contact (m2 convention) rather than rapier contact events, because
//!   kinematic-vs-kinematic bodies generate no contact events (see card 4 note).

use bevy::prelude::*;

use crate::components::{Hp, Monster, Player, Visual};
use crate::resources::Balance;
use crate::states::GameState;
use crate::systems::damage::{DamageRequest, DamageSource};
use crate::systems::pickup::spawn_pickup;

/// Contact tuning (GDD / m2 ContactSystem). INVULN_TIME stays fixed by design;
/// CONTACT_DAMAGE is the Balance default (card 11).
pub const CONTACT_DIST: f32 = 0.40;
pub const CONTACT_DAMAGE: f32 = 15.0;
pub const INVULN_TIME: f32 = 0.9;

/// A monster touching the player bites: hp down, invulnerability frames, white flash.
/// At most one bite per frame (m2: "一帧最多挨一口").
pub fn contact_damage(
    time: Res<Time>,
    balance: Res<Balance>,
    mut wrt: MessageWriter<DamageRequest>,
    mut player: Query<(Entity, &Transform, &mut Hp), With<Player>>,
    monsters: Query<&Transform, With<Monster>>,
) {
    let dt = time.delta_secs();
    let Some((player_entity, player_tf, mut hp)) = player.iter_mut().next() else {
        return; // no player (dead/despawned): nothing to bite
    };

    hp.invuln = (hp.invuln - dt).max(0.0);
    if hp.invuln > 0.0 {
        return; // still invulnerable
    }

    let p = player_tf.translation;
    for mtf in &monsters {
        let d = Vec2::new(mtf.translation.x - p.x, mtf.translation.z - p.z).length();
        if d <= CONTACT_DIST {
            // Decide the bite here (invuln gate); the actual Hp/flash is applied
            // by the single apply_damage in GameSet::Resolve. Log the projected
            // post-bite hp (damage is applied later in the same frame).
            hp.invuln = INVULN_TIME;
            wrt.write(DamageRequest {
                target: player_entity,
                amount: balance.contact_damage,
                source: DamageSource::Contact,
            });
            info!("[contact] player bitten, hp {:.0}", hp.hp - balance.contact_damage);
            break; // one bite per frame
        }
    }
}

/// hp <= 0: monsters despawn (dropping a pickup, card 6); the player flips to GameOver.
pub fn death_despawn(
    mut commands: Commands,
    monsters: Query<(Entity, &Hp, &Transform), With<Monster>>,
    player: Query<&Hp, With<Player>>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (e, hp, tf) in &monsters {
        if hp.hp <= 0.0 {
            spawn_pickup(&mut commands, &mut meshes, &mut materials, tf.translation);
            commands.entity(e).despawn();
        }
    }
    if let Ok(hp) = player.single() {
        if hp.hp <= 0.0 && *state.get() == GameState::Playing {
            next.set(GameState::GameOver);
            info!("GAME OVER — survived to wave N");
        }
    }
}

// --- HitFlashFeedback (card 14) ---

/// Card 14: flash fully decays in ~0.25 s (1.0 / rate). Subjective feel value;
/// promoting it into the Balance panel is a later, separate decision.
pub const FLASH_DECAY_RATE: f32 = 4.0;

/// Card 14 (logic half): decay every Visual.flash toward zero and never below.
/// Runs at the tail of the Playing chain (after the writers), so a same-frame
/// hit ends up at `max(1 - rate*dt, 0)` — deterministic per acceptance #2.
/// The presentation plugin mirrors this value onto material emissive; headless
/// worlds only ever see the number, which keeps every old test untouched.
pub fn decay_flash(time: Res<Time>, mut q: Query<&mut Visual>) {
    let dt = time.delta_secs();
    for mut visual in &mut q {
        if visual.flash > 0.0 {
            visual.flash = (visual.flash - FLASH_DECAY_RATE * dt).max(0.0);
        }
    }
}
