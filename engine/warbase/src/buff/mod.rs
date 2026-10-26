//! Buff/aura layer: source-tagged stat mods with optional expiry, folded
//! per frame into EffectiveStats; auras from map defs apply per frame by
//! full rebuild (in-range sources replace their entries; non-aura entries
//! like level/lap growth and timed buffs are preserved). The base ships
//! aggregation mechanics only — all numbers live in the map bundle.
//!
//! AURA GENERALIZATION (hole-7 upcast, SEAM2b): sources and targets are ANY
//! combat unit (`UnitInstance` carrier) — the old `PathFollower` query
//! binding was TD heritage that froze free-agent RTS units out of auras
//! entirely. Which side an aura touches is declared by the def
//! (`AuraDef::affects`), not by which query shape the carrier happens to
//! fit: `Enemies` auras ride `apply_auras` (hostile relation, the tower
//! debuff archetype), `Allies` auras ride `apply_follower_auras` (allied
//! relation, the devotion/shaman archetype — carrier included). Old maps
//! without the field deserialize as `Enemies`, identical to the old tower
//! behavior; the two systems strip only the entries their own relation
//! owns, so both may hit the same target without erasing each other.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::unit::{Allegiance, Hostility, UnitInstance};

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

/// Which side an aura touches (hole-7): `Enemies` = hostile units in range
/// (tower debuff archetype — the pre-generalization tower behavior, and the
/// serde default so old RON maps load unchanged); `Allies` = allied units
/// in range, carrier included (devotion/shaman archetype — the
/// pre-generalization follower-carrier behavior, now reachable by free
/// agents too).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuraAffects {
    #[default]
    Enemies,
    Allies,
}

/// Aura definition shipped in the map bundle (function towers).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuraDef {
    pub radius: f32,
    pub kind: StatKind,
    pub amount: f32,
    /// Who the aura touches (hole-7); defaults to Enemies (legacy tower
    /// semantics — old maps without the field behave exactly as before).
    #[serde(default)]
    pub affects: AuraAffects,
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

/// Allied auras (`AuraAffects::Allies`): carriers buff their ALLIES in
/// range, carrier included (devotion/shaman archetype — "kill the shaman
/// first"). Same per-frame rebuild semantics as hostile auras: entries
/// owned by allied-relation carriers are recomputed from scratch every
/// frame, so a sold/despawned carrier leaves nothing behind.
pub fn apply_follower_auras(
    hostility: Res<Hostility>,
    sources: Query<(Entity, &Transform, &Allegiance, &Auras), With<UnitInstance>>,
    mut targets: Query<(Entity, &Transform, &Allegiance, &mut StatMods), With<UnitInstance>>,
) {
    // Carrier snapshot (Entity -> side for strip ownership; defs filtered to
    // this system's tag). D6 note: HashMap is point-lookup only here.
    let carriers: Vec<(Entity, Vec3, Allegiance, Vec<&AuraDef>)> = sources
        .iter()
        .map(|(e, tf, side, auras)| {
            let defs = auras
                .0
                .iter()
                .filter(|a| a.affects == AuraAffects::Allies)
                .collect();
            (e, tf.translation, *side, defs)
        })
        .collect();
    let sides: std::collections::HashMap<Entity, Allegiance> =
        carriers.iter().map(|&(e, _, side, _)| (e, side)).collect();
    let mut grants: std::collections::HashMap<Entity, Vec<ModEntry>> =
        std::collections::HashMap::new();
    for (tgt_e, tgt_pos, tgt_side, mut mods) in &mut targets {
        // Selective strip (hole-7): entries from allied-relation carriers
        // belong to this system and are rebuilt below; hostile-aura entries
        // survive for apply_auras' own rebuild; entries whose source is no
        // longer a live carrier (sold tower / dead shaman) are stale and
        // dropped — carrier cleanup is the rebuild's job now that the two
        // systems no longer wipe each other.
        mods.entries.retain(|e| {
            if !e.from_aura {
                return true;
            }
            match sides.get(&e.source) {
                None => false, // carrier gone: stale aura entry
                Some(&src_side) => !hostility.allied(src_side, *tgt_side),
            }
        });
        for (src_e, src_pos, src_side, auras) in &carriers {
            if !hostility.allied(*src_side, *tgt_side) {
                continue; // allied auras never touch hostiles
            }
            let d = src_pos.distance(tgt_pos.translation);
            for aura in auras {
                if d <= aura.radius {
                    grants.entry(tgt_e).or_default().push(ModEntry {
                        source: *src_e,
                        kind: aura.kind,
                        amount: aura.amount,
                        expires_at: None,
                        from_aura: true,
                    });
                }
            }
        }
    }
    for (tgt, list) in grants {
        if let Ok((_, _, _, mut mods)) = targets.get_mut(tgt) {
            for entry in list {
                mods.upsert(entry);
            }
        }
    }
}

/// Hostile auras (`AuraAffects::Enemies`): carriers debuff HOSTILE units in
/// range (the tower-function archetype). Per-frame full rebuild of the
/// entries this system owns; entries from non-aura sources and from
/// allied-relation carriers are preserved.
pub fn apply_auras(
    hostility: Res<Hostility>,
    sources: Query<(Entity, &Transform, &Allegiance, &Auras), With<UnitInstance>>,
    mut targets: Query<(Entity, &Transform, &Allegiance, &mut StatMods), With<UnitInstance>>,
) {
    // See apply_follower_auras for the snapshot/strip rationale.
    let carriers: Vec<(Entity, Vec3, Allegiance, Vec<&AuraDef>)> = sources
        .iter()
        .map(|(e, tf, side, auras)| {
            let defs = auras
                .0
                .iter()
                .filter(|a| a.affects == AuraAffects::Enemies)
                .collect();
            (e, tf.translation, *side, defs)
        })
        .collect();
    let sides: std::collections::HashMap<Entity, Allegiance> =
        carriers.iter().map(|&(e, _, side, _)| (e, side)).collect();
    let mut grants: std::collections::HashMap<Entity, Vec<ModEntry>> =
        std::collections::HashMap::new();
    for (tgt_e, tgt_pos, tgt_side, mut mods) in &mut targets {
        // Selective strip (hole-7) — see apply_follower_auras: hostile-
        // relation entries are rebuilt below, allied entries survive for the
        // follower rebuild, stale sources (carriers gone) drop.
        mods.entries.retain(|e| {
            if !e.from_aura {
                return true;
            }
            match sides.get(&e.source) {
                None => false, // carrier gone: stale aura entry
                Some(&src_side) => !hostility.hostile(src_side, *tgt_side),
            }
        });
        for (src_e, src_pos, src_side, auras) in &carriers {
            if !hostility.hostile(*src_side, *tgt_side) {
                continue; // hostile auras never touch allies
            }
            let d = src_pos.distance(tgt_pos.translation);
            for aura in auras {
                if d <= aura.radius {
                    grants.entry(tgt_e).or_default().push(ModEntry {
                        source: *src_e,
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
