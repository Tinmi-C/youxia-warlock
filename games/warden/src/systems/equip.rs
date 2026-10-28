//! Roster v2 hero-tower equipment (the LT2 `EquipSlots` pattern ported to
//! warden's stat pipeline, roster-v2.md §4): fused towers are HERO towers and
//! carry a 3-slot rack; each slot holds at most one item whose affix injects
//! a permanent `StatMod` with a per-slot identity — same-slot equips refresh,
//! different slots stack, unequip withdraws. Plain base towers carry none.
//!
//! v1 scope: the data model + World-level API (debug/test seam). Acquisition
//! flows (drops / shop) are a follow-up card; nothing equips by default, so
//! every anchor binds the unequipped baseline (zero drift).

use bevy::prelude::*;

use crate::components::{EquipItem, StatMod, StatMods, TowerEquip, TowerKind, Tower};
use crate::resources::StatKind;
use crate::systems::stats::set_stat_mod;

/// One equippable item: a display name and the permanent affix it injects.
pub struct EquipDef {
    pub id: &'static str,
    pub label: &'static str,
    pub label_en: &'static str,
    pub kind: StatKind,
    /// Multiplicative affix (the v1 table is mult-only; `mult` folds into
    /// the pipeline's product arm).
    pub mult: f32,
}

/// The v1 item table (numbers live in code, ADR-0008; tune via §14).
pub const EQUIP_ITEMS: [EquipDef; 3] = [
    EquipDef {
        id: "whetstone",
        label: "磨刀石",
        label_en: "Whetstone",
        kind: StatKind::Damage,
        mult: 1.15,
    },
    EquipDef {
        id: "swift_string",
        label: "迅捷之弦",
        label_en: "Swift String",
        kind: StatKind::AttackSpeed,
        mult: 1.10,
    },
    EquipDef {
        id: "eagle_lens",
        label: "鹰眼镜",
        label_en: "Eagle Lens",
        kind: StatKind::Range,
        mult: 1.15,
    },
];

/// The per-slot removable identity an equipped item's affix carries —
/// `ModSource::Other("equip:<slot>")` (the LT2 marker-entity role in
/// warden's `ModSource` vocabulary: same source+kind refreshes, never
/// stacks; slots are independent identities).
fn slot_source(slot: usize) -> crate::components::ModSource {
    crate::components::ModSource::Other(match slot {
        0 => "equip:0",
        1 => "equip:1",
        _ => "equip:2",
    })
}

/// Equip `item` (an `EQUIP_ITEMS` index) into `slot` on a hero tower. A
/// previously equipped item in that slot is withdrawn first (its affix
/// removed) — LT2 same-slot replace semantics. Returns Err with a readable
/// reason when the target is not a hero tower, the slot/item index is out
/// of range, or the tower is gone.
pub fn equip_tower_item(
    world: &mut World,
    tower: Entity,
    slot: usize,
    item: usize,
) -> Result<(), String> {
    if item >= EQUIP_ITEMS.len() {
        return Err(format!("unknown item index {item}"));
    }
    if slot >= 3 {
        return Err(format!("slot {slot} out of range (0..3)"));
    }
    let kind = world.get::<Tower>(tower).map(|t| t.kind);
    if !matches!(kind, Some(TowerKind::Fused(_))) {
        return Err("only hero (fused) towers carry equipment slots".into());
    }
    if world.get::<TowerEquip>(tower).is_none() {
        world.entity_mut(tower).insert(TowerEquip::default());
    }
    // Withdraw any previous occupant of the slot (affix first, then slot).
    if let Some(rack) = world.get::<TowerEquip>(tower) {
        if rack.slots[slot].is_some() {
            withdraw(world, tower, slot)?;
        }
    }
    let def = &EQUIP_ITEMS[item];
    {
        let mut mods = world
            .get_mut::<StatMods>(tower)
            .expect("hero towers carry a StatMods bag");
        set_stat_mod(
            &mut mods,
            StatMod {
                source: slot_source(slot),
                kind: def.kind,
                add: 0.0,
                mult: def.mult,
                remaining: None,
            },
        );
    }
    world.entity_mut(tower).get_mut::<TowerEquip>().unwrap().slots[slot] =
        Some(EquipItem { item });
    info!("[equip] tower {tower:?} slot {slot} <- {}", def.id);
    Ok(())
}

/// Withdraw whatever sits in `slot` (its affix is removed; the slot
/// empties). Withdrawing an empty slot is a no-op.
pub fn unequip_tower_slot(world: &mut World, tower: Entity, slot: usize) -> Result<(), String> {
    if world.get::<TowerEquip>(tower).is_none() {
        return Err("tower carries no equipment rack".into());
    }
    withdraw(world, tower, slot)
}

fn withdraw(world: &mut World, tower: Entity, slot: usize) -> Result<(), String> {
    let occupied = world
        .get::<TowerEquip>(tower)
        .is_some_and(|r| r.slots[slot].is_some());
    if !occupied {
        return Ok(());
    }
    let item = world
        .get::<TowerEquip>(tower)
        .and_then(|r| r.slots[slot])
        .map(|e| e.item);
    if let Some(idx) = item {
        let kind = EQUIP_ITEMS[idx].kind;
        if let Some(mut mods) = world.get_mut::<StatMods>(tower) {
            crate::systems::stats::clear_stat_mod(&mut mods, slot_source(slot), kind);
        }
    }
    world.entity_mut(tower).get_mut::<TowerEquip>().unwrap().slots[slot] = None;
    Ok(())
}

/// Debug-visibility helper: the rack of a tower, if any (tests/panels).
pub fn rack_of(world: &World, tower: Entity) -> Option<TowerEquip> {
    world.get::<TowerEquip>(tower).copied()
}

/// Tower still exists guard for UI panels (reads `Tower` so a despawned
/// entity reports false without panicking).
pub fn tower_alive(world: &World, tower: Entity) -> bool {
    world.get::<Tower>(tower).is_some()
}
