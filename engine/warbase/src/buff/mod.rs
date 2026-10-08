//! Buff/aura layer: source-tagged stat mods with optional expiry, folded
//! per frame into EffectiveStats; auras from map defs apply per frame by
//! full rebuild (in-range sources replace their entries; non-aura entries
//! like level/lap growth and timed buffs are preserved). The base ships
//! aggregation mechanics only — all numbers live in the map bundle.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::path::PathFollower;
use crate::unit::Allegiance;

/// Which stat a mod touches. Amounts are multiplier deltas (0.3 = +30%,
/// -0.25 = -25%); same-kind deltas from different sources sum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StatKind {
    MoveSpeedMult,
    DamageMult,
    DamageTakenMult,
    /// >0 = slower attacks (interval x (1+sum)).
    AttackIntervalMult,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModEntry {
    /// Stable key: entries from the same (source, kind) replace, not stack.
    pub source: Entity,
    pub kind: StatKind,
    pub amount: f32,
    pub expires_at: Option<f32>,
    /// Aura-applied entries are rebuilt from scratch every frame, so a
    /// sold/despawned carrier leaves no stale slow behind.
    pub from_aura: bool,
}

/// Per-entity mod list. Writers: auras (per-frame rebuild), timed buffs,
/// permanent growth (levels / lap buffs).
#[derive(Component, Debug, Default)]
pub struct StatMods {
    pub entries: Vec<ModEntry>,
}

impl StatMods {
    pub fn upsert(&mut self, entry: ModEntry) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.source == entry.source && e.kind == entry.kind) {
            *e = entry;
        } else {
            self.entries.push(entry);
        }
    }
}

/// Aggregated per frame; readers multiply their base value by these
/// (default = all 1.0).
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct EffectiveStats {
    pub move_mult: f32,
    pub damage_mult: f32,
    pub damage_taken_mult: f32,
    pub interval_mult: f32,
}

impl EffectiveStats {
    pub fn neutral() -> Self {
        Self { move_mult: 1.0, damage_mult: 1.0, damage_taken_mult: 1.0, interval_mult: 1.0 }
    }
}

/// Aura definition shipped in the map bundle (function towers).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuraDef {
    pub radius: f32,
    pub kind: StatKind,
    pub amount: f32,
}

/// Present when the unit's def carries auras (spawn inserts it).
#[derive(Component, Debug)]
pub struct Auras(pub Vec<AuraDef>);

/// Tick: fold mods into EffectiveStats; expired timed entries are skipped
/// (lazily — writers may also sweep them).
pub fn aggregate_stats(
    time: Res<Time>,
    mut q: Query<(&StatMods, &mut EffectiveStats)>,
    mut bare: Query<(Entity, &StatMods), Without<EffectiveStats>>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs();
    for (mods, mut eff) in &mut q {
        *eff = EffectiveStats::neutral();
        fold(mods, &mut eff, now);
    }
    for (e, mods) in &mut bare {
        let mut eff = EffectiveStats::neutral();
        fold(mods, &mut eff, now);
        commands.entity(e).insert(eff);
    }
}

fn fold(mods: &StatMods, eff: &mut EffectiveStats, now: f32) {
    for entry in &mods.entries {
        if entry.expires_at.is_some_and(|t| t <= now) {
            continue;
        }
        match entry.kind {
            StatKind::MoveSpeedMult => eff.move_mult += entry.amount,
            StatKind::DamageMult => eff.damage_mult += entry.amount,
            StatKind::DamageTakenMult => eff.damage_taken_mult += entry.amount,
            StatKind::AttackIntervalMult => eff.interval_mult += entry.amount,
        }
    }
}

/// Auras: sources = aura carriers without PathFollower (towers), targets =
/// enemy creeps on paths. Per-frame full rebuild of the target's
/// aura-sourced entries; entries from non-aura sources are preserved.
pub fn apply_auras(
    sources: Query<(Entity, &Transform, &Allegiance, &Auras), Without<PathFollower>>,
    mut targets: Query<(Entity, &Transform, &Allegiance, &mut StatMods), With<PathFollower>>,
) {
    // Rebuild semantics: aura entries are recomputed from scratch every
    // frame (so a sold/despawned carrier leaves nothing behind). Non-aura
    // entries (level growth, timed buffs) are preserved.
    let mut grants: std::collections::HashMap<Entity, Vec<ModEntry>> =
        std::collections::HashMap::new();
    for (tgt_e, tgt_pos, tgt_side, mut mods) in &mut targets {
        mods.entries.retain(|e| !e.from_aura);
        for (src_e, src_pos, src_side, auras) in &sources {
            if *src_side == *tgt_side {
                continue;
            }
            let d = src_pos.translation.distance(tgt_pos.translation);
            for aura in &auras.0 {
                if d <= aura.radius {
                    grants.entry(tgt_e).or_default().push(ModEntry {
                        source: src_e,
                        kind: aura.kind,
                        amount: aura.amount,
                        expires_at: None,
                        from_aura: true,
                    });
                }
            }
        }
    }
    // Second mutable pass: apply each target's grants.
    for (tgt, list) in grants {
        if let Ok((_, _, _, mut mods)) = targets.get_mut(tgt) {
            for entry in list {
                mods.upsert(entry);
            }
        }
    }
}
