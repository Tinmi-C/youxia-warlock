//! Tower acquisition (获取来源，requirements §8): AC1 opening hand, AC2 shop
//! refresh, AC3 kill drops. All randomness flows through the `RunRng` resource
//! so tests can seed it deterministically.

use bevy::prelude::*;
use rand::RngExt; // rand 0.10: random_range / random_bool live on RngExt

use crate::resources::{Hand, RunRng, ShopOffers, WavePhase, WaveState};

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

/// AC2 core: fill `shop` with 3 random base-tower types; while the player does
/// not own all four, force at least one offer to be a type they do not own.
/// Bumps the version so the UI knows to rebuild.
pub fn refresh_shop_offers(shop: &mut ShopOffers, hand: &Hand, rng: &mut RunRng) {
    let mut offers: Vec<usize> = (0..3).map(|_| rng.0.random_range(0..4)).collect();
    let owns_all = (0..4).all(|t| hand.owned_towers.contains(&t));
    if !owns_all && !offers.iter().any(|t| !hand.owned_towers.contains(t)) {
        if let Some(unowned) = (0..4).find(|t| !hand.owned_towers.contains(t)) {
            offers[0] = unowned;
        }
    }
    shop.offers = offers;
    shop.version += 1;
    info!("[acquisition] shop refreshed {:?}", shop.offers);
}

/// AC2: initial offer at Startup, right after the opening hand is dealt.
pub fn setup_shop_offers(
    mut shop: ResMut<ShopOffers>,
    hand: Res<Hand>,
    mut rng: ResMut<RunRng>,
) {
    refresh_shop_offers(&mut shop, &hand, &mut rng);
}

/// AC2 hook: refresh the offer whenever a wave resolves back to Intermission
/// (the slow "经营" window where the shop is open). Runs in Observe so it sees
/// the phase change settled by wave_resolve (Cleanup).
pub fn refresh_shop_on_intermission(
    wave: Res<WaveState>,
    mut shop: ResMut<ShopOffers>,
    hand: Res<Hand>,
    mut rng: ResMut<RunRng>,
) {
    if wave.phase != WavePhase::Intermission || !wave.is_changed() {
        return;
    }
    refresh_shop_offers(&mut shop, &hand, &mut rng);
}
