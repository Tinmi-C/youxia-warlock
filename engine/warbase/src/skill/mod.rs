//! Skill framework (BX3-lite, ADR-0011 essence): per-entity SkillCaster,
//! three-way kinds (Passive/Active/Autocast), cooldowns, cast validation,
//! and SkillCast events for game-side effect dispatch (by id, no trait
//! objects, no scripting). Numbers ship per map; base ships mechanics.
//! Also hosts hero xp distribution (killer-fed XpHolder leveling).

use bevy::prelude::*;
use std::collections::HashMap;
use bevy::ecs::world::Mut;

use crate::buff::{ModEntry, StatKind, StatMods};
use crate::combat::UnitDied;
use crate::unit::{Mana, ManaRegen, MaxMana, XpHolder};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkillKind {
    Passive,
    Active,
    Autocast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkillTargeting {
    None,
    Point,
    Unit,
}

#[derive(Debug, Clone)]
pub struct SkillDef {
    pub id: String,
    pub kind: SkillKind,
    pub targeting: SkillTargeting,
    pub max_level: u8,
    pub cooldown: f32,
    /// Mana spent at cast start (SPL1). Default semantics = 0 (free cast);
    /// games authoring SkillDefs in Rust write `mana_cost: 0.0` for legacy
    /// behavior (D8: zero drift for warden's cooldown-driven free casts).
    pub mana_cost: f32,
    /// UI badge char (e.g. 'Z' for the hotkey label). Game-agnostic.
    pub badge: Option<char>,
    /// Global hotkey if the skill has one (commander Z/X/C style).
    pub hotkey: Option<KeyCode>,
}

#[derive(Resource, Debug, Default)]
pub struct SkillDefs {
    pub defs: HashMap<String, SkillDef>,
}

impl SkillDefs {
    pub fn get(&self, id: &str) -> Option<&SkillDef> {
        self.defs.get(id)
    }

    /// Total registered skills.
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

#[derive(Component, Debug)]
pub struct SkillCaster {
    pub slots: Vec<SkillSlot>,
}

#[derive(Debug, Clone)]
pub struct SkillSlot {
    pub skill: String,
    pub level: u8,
    pub cooldown: f32,
    pub autocast_on: bool,
}

/// Request (UI / autocast) -> validated here.
#[derive(Message, Debug)]
pub struct CastSkill {
    pub caster: Entity,
    pub slot: usize,
}

/// Interrupt an in-progress cast (stun/silence archetype effects; SPL2's
/// `Casting` component consumes this). Interrupted casts refund no mana and
/// start cooldown from the interrupt tick — pinned by the alignment plan.
#[derive(Message, Debug, Clone, Copy)]
pub struct InterruptCast {
    /// Read by SPL2's casting consumer (silence-all vs single-target).
    #[allow(dead_code)]
    pub caster: Entity,
}

/// Emitted after validation; game-side effect systems dispatch by skill id.
/// `point` is Some only when a game-side aiming layer confirmed a
/// Point-target cast (the base validates and fires immediately; games that
/// defer to a pick write the confirmed ground point here).
#[derive(Message, Debug, Clone)]
pub struct SkillCast {
    pub caster: Entity,
    pub skill: String,
    pub level: u8,
    pub point: Option<Vec3>,
}

pub fn tick_skill_cooldowns(
    time: Res<Time>,
    mut q: Query<&mut SkillCaster>,
    mut mana_q: Query<(&mut Mana, &MaxMana, &ManaRegen)>,
) {
    let dt = time.delta_secs();
    for mut caster in &mut q {
        for slot in &mut caster.slots {
            // Clamp at zero: cooldown is "seconds until ready" — never
            // negative (exact-0 assertions and UI reads stay stable).
            slot.cooldown = (slot.cooldown - dt).max(0.0);
        }
    }
    // SPL1 mana regen (Tick slot; the lib.rs system table is SEAM property
    // per D3, so the regen loop folds into the registered Tick system).
    for (mut mana, max, regen) in &mut mana_q {
        mana.0 = (mana.0 + regen.0 * dt).min(max.0);
    }
}

/// Intent: autocast slots that are ready and learned cast themselves.
pub fn run_autocast(casters: Query<(Entity, &SkillCaster)>, mut requests: MessageWriter<CastSkill>) {
    for (e, caster) in &casters {
        for (i, slot) in caster.slots.iter().enumerate() {
            if slot.autocast_on && slot.level > 0 && slot.cooldown <= 0.0 {
                requests.write(CastSkill { caster: e, slot: i });
            }
        }
    }
}

/// Act: validate requests (caster exists -> slot learned -> off cooldown ->
/// mana >= cost), spend mana at cast start, start cooldown, and emit
/// SkillCast for effect dispatch. Refused casts leave no trace (skip; the
/// base ships no reject channel by design).
pub fn process_casts(
    mut requests: MessageReader<CastSkill>,
    mut casts: MessageWriter<SkillCast>,
    defs: Res<SkillDefs>,
    mut casters: Query<(&mut SkillCaster, Option<&mut Mana>)>,
) {
    for req in requests.read() {
        let Ok((mut caster, mut mana)) = casters.get_mut(req.caster) else { continue };
        let Some(slot) = caster.slots.get_mut(req.slot) else { continue };
        if slot.level == 0 || slot.cooldown > 0.0 {
            continue;
        }
        let Some(def) = defs.get(&slot.skill) else { continue };
        // SPL1 mana gate: costs > 0 need a pool with enough mana; the spend
        // happens here, at cast start (interrupts refund nothing).
        if def.mana_cost > 0.0 {
            let Some(mana) = mana.as_deref_mut() else { continue };
            if mana.0 < def.mana_cost {
                continue;
            }
            mana.0 -= def.mana_cost;
        }
        slot.cooldown = def.cooldown;
        casts.write(SkillCast {
            caster: req.caster,
            skill: slot.skill.clone(),
            level: slot.level,
            point: None,
        });
    }
}

/// HR2: spend one banked point on a skill slot. Errors are readable for UI.
pub fn spend_skill_point(world: &mut World, entity: Entity, slot: usize) -> Result<(), String> {
    // Scoped reads first (sequential borrows only).
    let (points, slot_info) = {
        let Ok(holder) = world.get_entity_mut(entity) else {
            return Err("no such entity".into());
        };
        let points = holder.get::<XpHolder>().map(|x| x.points).ok_or("not a hero")?;
        let info = holder
            .get::<SkillCaster>()
            .and_then(|c| c.slots.get(slot).map(|s| (s.skill.clone(), s.level)))
            .ok_or("no such slot")?;
        (points, info)
    };
    if points == 0 {
        return Err("no skill points".into());
    }
    let (skill, level) = slot_info;
    let max_level = world
        .resource::<SkillDefs>()
        .get(&skill)
        .map(|d| d.max_level)
        .unwrap_or(1);
    if level >= max_level {
        return Err("max level".into());
    }
    let Ok(mut holder) = world.get_entity_mut(entity) else {
        return Err("no such entity".into());
    };
    holder.get_mut::<XpHolder>().unwrap().points -= 1;
    holder.get_mut::<SkillCaster>().unwrap().slots[slot].level += 1;
    Ok(())
}

/// Persistent cursor for xp events — a fresh cursor every frame would
/// re-read history and double-count (the double-buffer pitfall).
#[derive(Resource, Default)]
pub struct XpEventCursor(pub bevy::ecs::message::MessageCursor<UnitDied>);

/// Cleanup (exclusive): kill xp feeds the killer's XpHolder; each level
/// grants a point and refreshes a permanent DamageMult growth entry
/// (+5% per level above 1) — the hero-tower growth seam.
pub fn distribute_xp(world: &mut World) {
    let events: Vec<UnitDied> = {
        world.resource_scope(|world, mut cursor: Mut<XpEventCursor>| {
            let msgs = world.resource::<bevy::ecs::message::Messages<UnitDied>>();
            cursor.0.read(msgs).cloned().collect()
        })
    };
    for death in events {
        let Some(crate::combat::DamageOrigin::Attack(killer)) = death.killer else {
            continue; // skill/system kills don't feed tower xp in v1
        };
        let Ok(mut holder) = world.get_entity_mut(killer) else { continue };
        let Some(mut xp) = holder.get_mut::<XpHolder>() else { continue };
        xp.xp += death.xp;
        while xp.xp >= xp.next_level_xp {
            xp.xp -= xp.next_level_xp;
            xp.level += 1;
            xp.points += 1;
            xp.next_level_xp = (xp.next_level_xp as f32 * 2.0) as u32;
        }
        let level = xp.level;
        let entry = holder.entry::<StatMods>().or_default();
        entry.into_mut().upsert(ModEntry {
            source: killer,
            kind: StatKind::DamageMult,
            amount: 0.05 * (level - 1) as f32,
            expires_at: None,
            from_aura: false,
        });
    }
}
