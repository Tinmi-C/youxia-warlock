//! Hero domain (SEAM2d seam, HERO increment, round-2 plan §3): the WC3
//! three primary attributes with spawn-time derived-stat conversion, altar
//! revival, and kill-xp range sharing. Level/xp growth already lives in
//! `skill::XpHolder`; this adds STR/AGI/INT as an ADDITIVE layer on top of
//! the `UnitDef` base fields (never a replacement — `MaxHp += str *
//! hp_per_str`, and so on), the conversion numbers shipped per map through
//! the `HeroStatsDef` resource, the revive/share policy through `HeroRules`.
//! The base never guesses numbers (ADR-0012): every default is the identity
//! (0.0 / off), so maps without hero data behave exactly as before.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::buff::{ModEntry, StatKind, StatMods};
use crate::combat::Dead;
use crate::unit::{Hp, Mana, ManaRegen, MaxHp, MaxMana, XpHolder};

/// The three WC3 hero attributes (term pinned by the round-2 plan §3).
/// Authored per unit def (`UnitDef::hero_stats`, serde-default None =
/// plain unit, zero drift) and applied against the `HeroStatsDef` table at
/// spawn (static in round 2 — per-level attribute growth is a documented
/// seam: re-run [`apply_attribute_bonuses`] when a future increment adds
/// level-up attribute points).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeroStats {
    pub str: u8,
    pub agi: u8,
    pub int: u8,
}

/// Conversion table: how much derived stat one point of each attribute is
/// worth. Authored per map (resource; the base ships the identity defaults
/// — 0.0 converts nothing until a map says otherwise).
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct HeroStatsDef {
    /// Max HP (and spawn HP) per point of STR.
    pub hp_per_str: f32,
    /// Attack-speed bonus per point of AGI, folded into the StatMods
    /// pipeline as a NEGATIVE `AttackIntervalMult` amount (interval x
    /// (1 - sum): 0.02 = each AGI point makes attacks 2% faster).
    pub attack_speed_per_agi: f32,
    /// Damage-reduction per point of AGI, folded as a NEGATIVE
    /// `DamageTakenMult` amount. Naming note: WC3 armor is a numeric
    /// multiplier there, but warbase armor is the enum attack x armor
    /// MATRIX with no numeric armor value — the AGI "armor" equivalent
    /// under a matrix system is exactly "damage taken -x% per point", so
    /// the field keeps its WC3-facing name while meaning taken-mult.
    pub armor_per_agi: f32,
    /// Mana pool (MaxMana and spawn Mana) per point of INT.
    pub mana_per_int: f32,
    /// Mana regen per second per point of INT.
    pub mana_regen_per_int: f32,
}

/// Per-map hero policy (revive + xp share; HERO, round-2 plan §3). Games
/// insert this from their map data — same channel as every other `*Def`
/// resource. Defaults are ALL OFF (zero drift: without a policy, dead
/// heroes despawn through the legacy `combat::cleanup_dead` path and kill
/// xp feeds only the killer, exactly the pre-HERO behavior).
#[derive(Resource, Debug, Clone, Copy)]
pub struct HeroRules {
    /// Altar revival window in seconds. 0 (default) = policy off.
    pub revive_secs: f32,
    /// Gold paid at revival start (deducted from `econ::Resources`).
    /// Unaffordable revives proceed for free (`cost_paid = false`): the
    /// base ships the mechanism, games gate affordability externally if
    /// they want WC3's "wait at the altar" instead.
    pub revive_cost_gold: u32,
    /// Altar ground position `(x, z)`; None = revive in place.
    pub revive_altar: Option<[f32; 2]>,
    /// Kill-xp share radius around the killer (allied `XpHolder` heroes
    /// inside split the bounty evenly, killer included when eligible).
    /// 0 (default) = killer takes all — the exact pre-HERO credit path.
    pub share_radius: f32,
}

impl Default for HeroRules {
    fn default() -> Self {
        Self { revive_secs: 0.0, revive_cost_gold: 0, revive_altar: None, share_radius: 0.0 }
    }
}

/// A dead hero waiting out the altar revive timer (HERO, plan §3). While
/// present the entity KEEPS its `Dead` marker — dead semantics are exactly
/// the invisibility/invulnerability we want (every targeting, attack and
/// heal query already excludes `Dead`) — but is exempt from
/// `combat::cleanup_dead` (the one revive-driven change there). Timer
/// expiry (see `tick_revive`) removes both markers and resurrects the hero
/// at half Hp / half Mana at the altar (or in place).
#[derive(Component, Debug)]
pub struct Reviving {
    /// Seconds until resurrection; the death frame is frame zero and the
    /// hero returns exactly `revive_secs / dt` frames later.
    pub remaining: f32,
    /// Whether the gold cost was actually paid (bookkeeping for UIs).
    pub cost_paid: bool,
}

/// Marker: `hero_stats` present on a def spawned through the DEFERRED path
/// (`spawn_unit_deferred` — wave creeps, summons, production graduates),
/// which has no World access to read the `HeroStatsDef` table mid-system.
/// The hero Tick (`apply_pending_stats`) converts on the first frame after
/// materialization; World-variant spawns convert at spawn (pinned by the
/// round-2 plan: "spawn 时算定").
#[derive(Component, Debug)]
pub struct HeroStatsRaw;

/// Apply the three-attribute conversion: base stats come from the UnitDef
/// fields, attributes are an additive layer on top (STR -> Hp/MaxHp, INT
/// -> Mana/MaxMana/ManaRegen — conjuring an empty pool first only when INT
/// actually converts to something; AGI rides the StatMods pipeline with
/// `source` = the hero itself and `from_aura = false`, so auras/timed
/// buffs compose instead of overwrite). Idempotent-safe by construction:
/// AGI entries upsert on (self, kind); the additive HP/mana bumps are
/// meant to run exactly once per spawn (and again on future growth, with
/// the delta passed by that increment).
pub fn apply_attribute_bonuses(world: &mut World, entity: Entity) {
    let Some(stats) = world.get::<HeroStats>(entity).copied() else { return };
    // Missing table (an app spawning hero defs without the hero plugin)
    // converts nothing — the identity default, never a panic.
    let table = world.get_resource::<HeroStatsDef>().copied().unwrap_or_default();

    // STR -> HP pool.
    let hp_bonus = stats.str as f32 * table.hp_per_str;
    if hp_bonus != 0.0 {
        if let Some(mut hp) = world.get_mut::<Hp>(entity) {
            hp.0 += hp_bonus;
        }
        if let Some(mut max) = world.get_mut::<MaxHp>(entity) {
            max.0 += hp_bonus;
        }
    }

    // INT -> mana pool + regen.
    let mana_bonus = stats.int as f32 * table.mana_per_int;
    let regen_bonus = stats.int as f32 * table.mana_regen_per_int;
    if mana_bonus != 0.0 || regen_bonus != 0.0 {
        if world.get::<MaxMana>(entity).is_none() {
            world.entity_mut(entity).insert((Mana(0.0), MaxMana(0.0), ManaRegen(0.0)));
        }
        if let Some(mut m) = world.get_mut::<Mana>(entity) {
            m.0 += mana_bonus;
        }
        if let Some(mut m) = world.get_mut::<MaxMana>(entity) {
            m.0 += mana_bonus;
        }
        if let Some(mut r) = world.get_mut::<ManaRegen>(entity) {
            r.0 += regen_bonus;
        }
    }

    // AGI -> attack speed + damage reduction (StatMods; the aggregation
    // pipeline is already wired — no per-frame hero math).
    let speed = -(stats.agi as f32 * table.attack_speed_per_agi);
    let reduction = -(stats.agi as f32 * table.armor_per_agi);
    if speed != 0.0 || reduction != 0.0 {
        if world.get::<StatMods>(entity).is_none() {
            world.entity_mut(entity).insert(StatMods::default());
        }
        if let Some(mut mods) = world.get_mut::<StatMods>(entity) {
            if speed != 0.0 {
                mods.upsert(ModEntry {
                    source: entity,
                    kind: StatKind::AttackIntervalMult,
                    amount: speed,
                    expires_at: None,
                    from_aura: false,
                });
            }
            if reduction != 0.0 {
                mods.upsert(ModEntry {
                    source: entity,
                    kind: StatKind::DamageTakenMult,
                    amount: reduction,
                    expires_at: None,
                    from_aura: false,
                });
            }
        }
    }

    // Debt cleared (World-path spawns never carried it; defensive remove).
    world.entity_mut(entity).remove::<HeroStatsRaw>();
}

/// Tick: convert deferred-spawned heroes marked [`HeroStatsRaw`] (see the
/// marker's doc). Ordered before the buff aggregation chain so the AGI
/// entries fold into `EffectiveStats` on the same frame.
pub fn apply_pending_stats(world: &mut World) {
    let pending: Vec<Entity> = world
        .query_filtered::<Entity, With<HeroStatsRaw>>()
        .iter(world)
        .collect();
    for e in pending {
        apply_attribute_bonuses(world, e);
    }
}

/// Cleanup: catch freshly-dead heroes BEFORE `combat::cleanup_dead`
/// despawns them. A "hero" at runtime is an `XpHolder` carrier (spawned
/// iff `UnitDef::hero` — the plan's "hero: true 或有 XpHolder+HeroStats"
/// collapses to the XpHolder marker; warden's hero towers stay exempt from
/// revival simply by leaving `revive_secs` at its 0 default). Exclusive
/// world system: the `Reviving` insert is structural and must be visible
/// to `cleanup_dead`'s query THIS frame.
pub fn begin_revive(world: &mut World) {
    let rules = *world.resource::<HeroRules>();
    if rules.revive_secs <= 0.0 {
        return; // policy off: legacy despawn path, zero drift
    }
    let fresh: Vec<Entity> = world
        .query_filtered::<Entity, (With<XpHolder>, With<Dead>, Without<Reviving>)>()
        .iter(world)
        .collect();
    if fresh.is_empty() {
        return;
    }
    // Pay the revival cost up front (scoped borrow; unaffordable — or a
    // missing econ resource — revives proceed for free, see
    // HeroRules::revive_cost_gold).
    let mut paid: Vec<(Entity, bool)> = Vec::with_capacity(fresh.len());
    {
        let mut resources = world.get_resource_mut::<crate::econ::Resources>();
        for e in &fresh {
            let mut cost_paid = false;
            if let Some(res) = resources.as_deref_mut() {
                if rules.revive_cost_gold > 0 && res.gold >= rules.revive_cost_gold {
                    res.gold -= rules.revive_cost_gold;
                    cost_paid = true;
                }
            }
            paid.push((*e, cost_paid));
        }
    }
    for (e, cost_paid) in paid {
        world.entity_mut(e).insert(Reviving { remaining: rules.revive_secs, cost_paid });
    }
}

/// Cleanup: count revive timers down and resurrect on expiry — remove
/// `Dead` + `Reviving`, Hp = 50% of MaxHp, Mana = 50% of MaxMana (when a
/// pool exists), teleport to the altar (None = in place). Pinned AFTER
/// `distribute_xp` so a hero on its exact revive frame cannot leech
/// shared xp through this frame's kills (the `Dead` marker is still
/// present while xp is credited).
///
/// Single-writer note: `resolve_damage` is documented as the only Hp
/// writer; resurrection is a death-boundary transition (a respawn, not a
/// heal — `resolve_heals` refuses dead targets by design), so this writes
/// Hp directly beside that rule, mirroring how `resolve_heals` itself
/// lives next to the pipeline.
pub fn tick_revive(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs();
    let half_frame = dt * 0.5;
    let altar = world.resource::<HeroRules>().revive_altar;
    let mut due: Vec<Entity> = Vec::new();
    {
        let mut timers = world.query::<(Entity, &mut Reviving)>();
        for (e, mut rev) in timers.iter_mut(world) {
            rev.remaining -= dt;
            if rev.remaining <= half_frame {
                due.push(e);
            }
        }
    }
    for e in due {
        let Some(max_hp) = world.get::<MaxHp>(e) else { continue };
        let max_hp = max_hp.0;
        let max_mana = world.get::<MaxMana>(e).map(|m| m.0);
        let Ok(mut ent) = world.get_entity_mut(e) else { continue };
        if let Some(mut hp) = ent.get_mut::<Hp>() {
            hp.0 = max_hp * 0.5;
        }
        if let Some(max) = max_mana {
            if let Some(mut mana) = ent.get_mut::<Mana>() {
                mana.0 = max * 0.5;
            }
        }
        if let Some([x, z]) = altar {
            if let Some(mut tf) = ent.get_mut::<Transform>() {
                tf.translation.x = x;
                tf.translation.z = z;
            }
        }
        ent.remove::<(Dead, Reviving)>();
    }
}

/// Domain plugin (SEAM2d; systems landed with the HERO increment,
/// ADR-0018 Phase-1).
pub struct HeroPlugin;

impl Plugin for HeroPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeroStatsDef>()
            .init_resource::<HeroRules>()
            .add_systems(
                Update,
                apply_pending_stats
                    .in_set(crate::WarbaseSet::Tick)
                    .before(crate::buff::apply_auras),
            )
            .add_systems(
                Update,
                begin_revive
                    .in_set(crate::WarbaseSet::Cleanup)
                    .before(crate::combat::cleanup_dead),
            )
            .add_systems(
                Update,
                tick_revive
                    .in_set(crate::WarbaseSet::Cleanup)
                    .after(crate::combat::cleanup_dead)
                    .after(crate::skill::distribute_xp),
            );
    }
}

pub mod prelude {
    pub use super::{
        apply_attribute_bonuses, apply_pending_stats, begin_revive, HeroPlugin, HeroRules,
        HeroStats, HeroStatsDef, HeroStatsRaw, Reviving, tick_revive,
    };
}
