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
use crate::unit::XpHolder;

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
}

#[derive(Resource, Debug, Default)]
pub struct SkillDefs {
    pub defs: HashMap<String, SkillDef>,
}

impl SkillDefs {
    pub fn get(&self, id: &str) -> Option<&SkillDef> {
        self.defs.get(id)
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

/// Emitted after validation; game-side effect systems dispatch by skill id.
#[derive(Message, Debug, Clone)]
pub struct SkillCast {
    pub caster: Entity,
    pub skill: String,
    pub level: u8,
}

pub fn tick_skill_cooldowns(time: Res<Time>, mut q: Query<&mut SkillCaster>) {
    let dt = time.delta_secs();
    for mut caster in &mut q {
        for slot in &mut caster.slots {
            slot.cooldown -= dt;
        }
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

/// Act: validate requests (learned + off cooldown), start cooldown, emit
/// SkillCast for effect dispatch.
pub fn process_casts(
    mut requests: MessageReader<CastSkill>,
    mut casts: MessageWriter<SkillCast>,
    defs: Res<SkillDefs>,
    mut casters: Query<&mut SkillCaster>,
) {
    for req in requests.read() {
        let Ok(mut caster) = casters.get_mut(req.caster) else { continue };
        let Some(slot) = caster.slots.get_mut(req.slot) else { continue };
        if slot.level == 0 || slot.cooldown > 0.0 {
            continue;
        }
        let Some(def) = defs.get(&slot.skill) else { continue };
        slot.cooldown = def.cooldown;
        casts.write(SkillCast { caster: req.caster, skill: slot.skill.clone(), level: slot.level });
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
        let mut entry = holder.entry::<StatMods>().or_default();
        entry.into_mut().upsert(ModEntry {
            source: killer,
            kind: StatKind::DamageMult,
            amount: 0.05 * (level - 1) as f32,
            expires_at: None,
            from_aura: false,
        });
    }
}
