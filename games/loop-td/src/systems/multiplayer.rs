//! LT2-6 multiplayer leak pass (v2 proposal §2 #6): the base rule module
//! reserves `LeakPolicy::PassToNeighbor` as the water-TD multiplayer seam
//! and deliberately does nothing with it (engine keeps mechanism-free). This
//! game-side system ACTIVATES the seam for loop maps: on a closed loop a
//! completed lap IS the leak event (a creep that circled a full ring past
//! that player's defenses), so every `LapCompleted` under the policy
//! transfers the creep to the NEIGHBOR ring (path index + 1, wrapping):
//! same def, same current/max hp, same wave tag (the wave can still clear),
//! spawned at the neighbor's start point — the leaking ring's stock drops
//! to zero for that unit because the entity moves, not duplicates.
//!
//! Dual-ring scenario bundles (tests/v2_depth.rs) carry the two-player
//! diplomacy (P0/P1 on team 0 vs environment team 1); the shipped loop.ron
//! stays single-player Stockpile and this system is a no-op there.

use bevy::prelude::*;
use warbase::prelude::*;

/// Marker on a creep that arrived via a leak pass (test/UI observation).
#[derive(Component, Debug)]
pub struct TransferredLeak;

#[derive(Resource, Default)]
struct LapCursor(bevy::ecs::message::MessageCursor<LapCompleted>);

/// Update: under `LeakPolicy::PassToNeighbor`, every lap completion moves
/// the creeping unit to the next path index (wrap). Any other policy (the
/// shipped Stockpile/Lives maps) leaves this a cheap no-op.
pub fn pass_leaks_to_neighbor(world: &mut World) {
    world.get_resource_or_insert_with(LapCursor::default);
    let active = matches!(
        world.resource::<MatchRules>().leak,
        LeakPolicy::PassToNeighbor
    );
    let paths = world.resource::<Paths>().0.clone();
    let laps: Vec<(Entity, u32)> = world.resource_scope(|w, mut c: Mut<LapCursor>| {
        let msgs = w.resource::<bevy::ecs::message::Messages<LapCompleted>>();
        c.0.read(msgs).map(|l| (l.unit, l.lap)).collect()
    });
    if !active || paths.len() < 2 || laps.is_empty() {
        return; // cursor stays drained either way
    }

    for (unit, lap) in laps {
        let (Some(follow), Some(inst)) =
            (world.get::<PathFollower>(unit), world.get::<UnitInstance>(unit))
        else {
            continue;
        };
        let Some(idx) = paths.iter().position(|&p| p == follow.path) else { continue };
        let neighbor = paths[(idx + 1) % paths.len()];
        let def = inst.def.clone();
        let hp = world.get::<Hp>(unit).map(|h| h.0);
        let max = world.get::<MaxHp>(unit).map(|m| m.0);
        let wave_tag = world.get::<WaveTag>(unit).map(|t| t.index());

        // Move the entity: despawn on the leaking ring, twin on the neighbor.
        if let Ok(mut em) = world.get_entity_mut(unit) {
            em.despawn();
        } else {
            continue;
        }
        let start = world
            .get::<PathDef>(neighbor)
            .map(|p| p.sample(0.0))
            .unwrap_or_default();
        let registry = world.resource::<UnitRegistry>().clone();
        let Some(twin) = spawn_unit(
            world,
            &registry,
            &def,
            Vec3::new(start.x, 0.0, start.y),
            Allegiance::Attacker,
            1.0,
        ) else {
            continue;
        };
        if let Some(hp) = hp {
            if let Some(mut h) = world.get_mut::<Hp>(twin) {
                h.0 = hp;
            }
        }
        if let Some(max) = max {
            if let Some(mut m) = world.get_mut::<MaxHp>(twin) {
                m.0 = max;
            }
        }
        let Ok(mut em) = world.get_entity_mut(twin) else { continue };
        em.insert(PathFollower { path: neighbor, progress: 0.0, laps: 0, finished: false })
            .insert(TransferredLeak);
        if let Some(w) = wave_tag {
            em.insert(WaveTag::Normal(w));
        }
        info!(
            "[loop-td] leak pass: '{def}' ring {idx} -> ring {} (lap {lap})",
            (idx + 1) % paths.len()
        );
    }
}
