//! Tower acquisition (获取来源，requirements §8): AC1 opening hand, AC2 shop
//! refresh, AC3 kill drops. All randomness flows through the `RunRng` resource
//! so tests can seed it deterministically.

use bevy::prelude::*;
use rand::RngExt; // rand 0.10: random_range / random_bool live on RngExt

use crate::resources::{Hand, RunRng};

/// AC1: deal the opening hand — 2 distinct random base towers, guaranteeing
/// >=1 output tower (archer 0 / cannon 3) so a run can't start unkillable.
/// Runs once at Startup, replacing the empty placeholder hand.
pub fn deal_opening_hand(mut hand: ResMut<Hand>, mut rng: ResMut<RunRng>) {
    if !hand.owned_towers.is_empty() {
        return; // already dealt (idempotent guard)
    }
    let mut pool = [0usize, 1, 2, 3];
    // Partial Fisher-Yates: shuffle, then take two distinct types.
    for i in (1..pool.len()).rev() {
        let j = rng.0.random_range(0..=i);
        pool.swap(i, j);
    }
    let mut picked = pool[..2].to_vec();
    if !(picked.contains(&0) || picked.contains(&3)) {
        // Guarantee: the second card becomes a random output tower.
        picked[1] = if rng.0.random_bool(0.5) { 0 } else { 3 };
    }
    hand.owned_towers = picked;
    info!("[acquisition] opening hand {:?}", hand.owned_towers);
}
