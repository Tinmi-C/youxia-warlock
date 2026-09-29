//! Stat modification pipeline (SK6, ADR-0011 layer 3): source-tagged
//! `StatMods` on an entity are folded with its base Tower stats and the
//! global Boosts/Frenzy multipliers into `EffectiveStats` each frame, right
//! before any consumer (`tower_fire`) runs. Auras and timed buffs are just
//! entries with a removable identity — add on enter, remove on leave/expire.
//!
//! Fold order per stat: `(base + Σ add) × Π mult`, where the global Boosts
//! multiplier (and Frenzy, for attack speed) is simply the first `mult`.

use bevy::prelude::*;

use crate::components::{EffectiveStats, ModSource, StatMod, StatMods, Tower};
use crate::resources::{Boosts, SkillBalance, StatKind, WarFrenzy};

/// No skill grants crit below this — every tower rolls with these unless a
/// StatMod raises them (SK9 鹰眼).
pub const BASE_CRIT_CHANCE: f32 = 0.0;
pub const BASE_CRIT_MULTIPLIER: f32 = 2.0;

/// Insert or replace the modifier `source` contributes to one stat. The
/// identity is (source, kind): a second write refreshes, never stacks —
/// auras re-assert every frame, timed buffs refresh their duration.
pub fn set_stat_mod(mods: &mut StatMods, m: StatMod) {
    if let Some(slot) = mods
        .0
        .iter_mut()
        .find(|e| e.source == m.source && e.kind == m.kind)
    {
        *slot = m;
    } else {
        mods.0.push(m);
    }
}

/// Remove one source's contribution to one stat (aura leave).
pub fn clear_stat_mod(mods: &mut StatMods, source: ModSource, kind: StatKind) {
    mods.0.retain(|e| !(e.source == source && e.kind == kind));
}

/// Fold `(base + Σ add) × Π mult` for one stat, starting the product at
/// `global_mult` (Boosts / Frenzy live there).
fn fold(mods: Option<&StatMods>, kind: StatKind, base: f32, global_mult: f32) -> f32 {
    let (mut add, mut mult) = (0.0, global_mult);
    if let Some(mods) = mods {
        for m in &mods.0 {
            if m.kind == kind {
                add += m.add;
                mult *= m.mult;
            }
        }
    }
    (base + add) * mult
}

/// Tick timed modifiers down and drop expired ones (SK6). Permanent entries
/// (None) are untouched; auras remove themselves via `clear_stat_mod`.
pub fn tick_stat_mods(time: Res<Time>, mut mods: Query<&mut StatMods>) {
    let dt = time.delta_secs();
    for mut mods in &mut mods {
        mods.0.retain_mut(|m| match m.remaining {
            Some(r) => {
                let r = (r - dt).max(0.0);
                m.remaining = Some(r);
                r > 0.0
            }
            None => true,
        });
    }
}

/// Aggregate every tower's effective stats (SK6): base fields + Boosts
/// (per `tower_index`, the AC4 semantics fused towers share) + Frenzy
/// (attack speed while the burst lasts) + the tower's own StatMods.
/// `tower_fire` (and later consumers) read only `EffectiveStats`.
pub fn aggregate_stats(
    boost: Res<Boosts>,
    frenzy: Res<WarFrenzy>,
    balance: Res<SkillBalance>,
    mut commands: Commands,
    // No Transform/conflict with fire: read Tower, write EffectiveStats.
    mut towers: Query<(Entity, &Tower, Option<&StatMods>, Option<&mut EffectiveStats>)>,
) {
    let frenzy_mult = if frenzy.remaining > 0.0 {
        balance.frenzy_speed_mult
    } else {
        1.0
    };
    for (eid, tower, mods, estats) in &mut towers {
        let idx = tower.tower_index;
        let dmg_boost = boost.damage_mult.get(idx).copied().unwrap_or(1.0);
        let speed_boost = boost.attack_speed_mult.get(idx).copied().unwrap_or(1.0);
        let range_boost = boost.range_mult.get(idx).copied().unwrap_or(1.0);
        let stats = EffectiveStats {
            damage: fold(mods, StatKind::Damage, tower.damage, dmg_boost),
            attack_speed: fold(
                mods,
                StatKind::AttackSpeed,
                tower.attack_speed,
                speed_boost * frenzy_mult,
            ),
            range: fold(mods, StatKind::Range, tower.range, range_boost),
            crit_chance: fold(mods, StatKind::CritChance, BASE_CRIT_CHANCE, 1.0),
            crit_multiplier: fold(mods, StatKind::CritMultiplier, BASE_CRIT_MULTIPLIER, 1.0),
        };
        if let Some(mut e) = estats {
            *e = stats;
        } else {
            commands.entity(eid).insert(stats);
        }
    }
}
