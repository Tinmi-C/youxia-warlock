//! Seeded match RNG (TS0): all gameplay randomness must go through this
//! single resource so headless harness runs are reproducible. Games and the
//! harness set the seed explicitly; the default seed is random.

use bevy::prelude::*;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

#[derive(Resource, Debug)]
pub struct MatchRng {
    rng: StdRng,
    seed: u64,
}

impl MatchRng {
    pub fn seeded(seed: u64) -> Self {
        Self { rng: StdRng::seed_from_u64(seed), seed }
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn next_f32(&mut self) -> f32 {
        self.rng.random()
    }

    pub fn next_range(&mut self, lo: f32, hi: f32) -> f32 {
        self.rng.random_range(lo..hi)
    }

    pub fn next_u32(&mut self) -> u32 {
        self.rng.random()
    }

    /// Uniform pick from a slice (empty slice returns None).
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            Some(&items[self.rng.random_range(0..items.len())])
        }
    }
}

impl Default for MatchRng {
    fn default() -> Self {
        // Entropy only for interactive runs; tests and the harness always
        // seed explicitly.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        Self::seeded(nanos)
    }
}
