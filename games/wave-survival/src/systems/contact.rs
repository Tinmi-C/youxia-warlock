//! CombatContact: monster bites the player + death. Capability card 5 (docs/capability-cards.md).
//! Distance-based contact (m2 convention) rather than rapier contact events, because
//!   kinematic-vs-kinematic bodies generate no contact events (see card 4 note).

use bevy::prelude::*;

use crate::components::{Hp, Monster, Player, Visual};
use crate::states::GameState;
use crate::systems::pickup::spawn_pickup;

/// Contact tuning (GDD / m2 ContactSystem).
pub const CONTACT_DIST: f32 = 0.40;
pub const CONTACT_DAMAGE: f32 = 15.0;
pub const INVULN_TIME: f32 = 0.9;

/// A monster touching the player bites: hp down, invulnerability frames, white flash.
/// At most one bite per frame (m2: "一帧最多挨一口").
pub fn contact_damage(
    time: Res<Time>,
    mut player: Query<(&Transform, &mut Hp, &mut Visual), With<Player>>,
    monsters: Query<&Transform, With<Monster>>,
) {
    let dt = time.delta_secs();
    let Some((player_tf, mut hp, mut visual)) = player.iter_mut().next() else {
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
            hp.hp -= CONTACT_DAMAGE;
            hp.invuln = INVULN_TIME;
            visual.flash = 1.0;
            info!("[contact] player bitten, hp {:.0}", hp.hp);
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
