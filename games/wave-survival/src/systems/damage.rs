//! Unified damage settlement pipeline (structure-review discussion).
//!
//! Producers (attacks/skills) do NOT touch `Hp` directly — each emits a
//! [`DamageRequest`] for one damage event. The single [`apply_damage`] system
//! reads every request for the frame and applies them all in one pass.
//!
//! Why (scaling with many skills):
//! - `Hp` is written by exactly one system per frame -> no `&mut Hp` contention
//!   (which would serialize many skill systems) and no double-settlement
//!   (each request is applied once; the "apply" path is a single place).
//! - Producers that want to damage a target only *read* positions (shared `&`
//!   access, parallelizable) and emit a request; they never `&mut` the target.
//!
//! Scheduling: producers write in `GameSet::Combat`; this runs in
//! `GameSet::Resolve` (after Combat, before Despawn), so all damage for a frame
//! lands before death (`hp.hp <= 0`) is decided.

use bevy::prelude::*;

use crate::components::{Hp, Visual};

/// Where a damage request came from (logs / kill attribution). Grows with each
/// new source: Slash (card 2/29), later Nova / Contact / named skills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DamageSource {
    Slash,
}

/// One damage event = one request, applied exactly once by [`apply_damage`].
#[derive(Message)]
pub struct DamageRequest {
    pub target: Entity,
    pub amount: f32,
    pub source: DamageSource,
}

/// Single settlement pass: drain all requests for this frame, subtract from
/// `Hp`, set the hit-flash. Skips targets that no longer match (despawned or
/// not damageable). Runs in `GameSet::Resolve` after every Combat producer.
pub fn apply_damage(
    mut reader: MessageReader<DamageRequest>,
    mut q: Query<(&mut Hp, &mut Visual)>,
) {
    for req in reader.read() {
        let Ok((mut hp, mut visual)) = q.get_mut(req.target) else {
            continue; // target despawned / no longer damageable
        };
        hp.hp -= req.amount;
        visual.flash = 1.0;
        info!(
            "[dmg] {:?} hits {:?} for {:.1}, hp now {:.1}",
            req.source, req.target, req.amount, hp.hp
        );
    }
}
