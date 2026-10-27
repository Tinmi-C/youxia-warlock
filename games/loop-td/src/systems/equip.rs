//! LT2-1 equipment on hero towers (v2 proposal §2 #1): 3 dedicated equip
//! slots on hero defs; each equipment piece is an `ItemDef` whose affix
//! (攻+X% / 间隔-X% / 射程+X%) rides a `StatMods` entry injected into the
//! tower with a MARKER ENTITY per (tower, slot) as the entry source — the
//! INV domain's pre-SEAM3b flaw (passives used the holder as source, so
//! same-kind entries overwrote each other) is dodged by construction:
//! every slot owns its own source, same-slot replacement strips exactly
//! that source's entries.
//!
//! SEAM3b: the range affix (观测镜 射程+X%) moved into the base vocabulary
//! (`StatKind::RangeMult` folds into `EffectiveStats.range_mult` and
//! `combat::attack_range` applies it at targeting time) — the game-side
//! `RANGE_AFFIXES` table and the `AttackKit.def.range` rewrite are retired;
//! item numbers now live purely in the map's item table.

use bevy::prelude::*;
use warbase::prelude::*;

/// Equip slots per hero tower (proposal §2: 3).
pub const EQUIP_SLOTS: usize = 3;

/// One occupied equip slot. `source` is the dedicated StatMods source entity
/// for this (tower, slot) pair — stable for the slot's lifetime.
///
/// SEAM3b note: the base Inventory bag is now slot-entity-backed (per-stack
/// ModEntry sources), but the EQUIP rack keeps its own marker scheme: the
/// rack is a distinct 3-slot surface with swap-back-to-bag semantics, and
/// its per-slot marker is already a dedicated source — re-parenting it onto
/// the bag stack entity would churn the swap path for no behavioral gain
/// (smallest blast radius, pinned here deliberately).
#[derive(Debug, Clone)]
pub struct EquippedItem {
    pub def: String,
    pub source: Entity,
}

/// 3-slot equipment rack on hero towers (plain towers carry none).
#[derive(Component, Debug, Default, Clone)]
pub struct EquipSlots {
    pub slots: [Option<EquippedItem>; EQUIP_SLOTS],
}

/// Update: fresh hero spawns (placement, upgrades, grants, restarts) get the
/// rack — same Added<UnitInstance> pattern as adopt::attach_td_kit.
pub fn attach_equip_slots(
    added: Query<(Entity, &UnitInstance, &Allegiance), Added<UnitInstance>>,
    registry: Res<UnitRegistry>,
    mut commands: Commands,
) {
    for (entity, instance, side) in &added {
        if *side != Allegiance::Defender {
            continue;
        }
        let Some(def) = registry.get(&instance.def) else { continue };
        if def.hero {
            commands.entity(entity).insert(EquipSlots::default());
        }
    }
}

/// All equipment-class item ids in the registry, sorted (deterministic
/// picks for random-equipment rewards — challenge drops, treasure digs).
pub fn equipment_ids(world: &mut World) -> Vec<String> {
    let mut ids: Vec<String> = world
        .resource::<ItemRegistry>()
        .defs
        .values()
        .filter(|d| d.kind == ItemKind::Passive)
        .map(|d| d.id.clone())
        .collect();
    ids.sort();
    ids
}

/// Strip every StatMods entry owned by `source` on `tower` (all kinds — the
/// slot's source is exclusive to the slot).
fn strip_slot_entries(world: &mut World, tower: Entity, source: Entity) {
    if let Some(mut mods) = world.get_mut::<StatMods>(tower) {
        mods.entries.retain(|e| e.source != source);
    }
}

/// Stow an item stack into the tower's bag (Inventory) when there is room.
/// SEAM3b: bags are slot-ENTITY backed — stowing spawns the stack entity
/// (`StackHolder` links it for the base's orphan sweep).
fn stow_into_bag(world: &mut World, tower: Entity, item: &str) -> Result<(), String> {
    let def = world
        .resource::<ItemRegistry>()
        .get(item)
        .cloned()
        .ok_or_else(|| format!("unknown item '{item}'"))?;
    if world.get::<Inventory>(tower).is_none() {
        return Ok(()); // no bag (never happens for hero towers) — item is lost
    }
    let Some(slot) = world
        .get::<Inventory>(tower)
        .and_then(|inv| inv.first_free_slot())
    else {
        return Err("bag full".into());
    };
    let stack = world
        .spawn((ItemStack::from_def(&def), StackHolder(tower)))
        .id();
    if let Some(mut inv) = world.get_mut::<Inventory>(tower) {
        inv.slots[slot] = Some(stack);
    }
    Ok(())
}

/// Equip `item_id` into `slot` on a hero tower, replacing whatever the slot
/// held (the replaced piece goes back into the bag; a full bag refuses the
/// swap BEFORE anything changes). Every affix — damage, interval, range —
/// upserts through the slot's dedicated source as one StatMods entry.
pub fn equip_item(world: &mut World, tower: Entity, slot: usize, item_id: &str) -> Result<(), String> {
    if slot >= EQUIP_SLOTS {
        return Err(format!("slot {slot} out of range (0..{EQUIP_SLOTS})"));
    }
    let def = world
        .resource::<ItemRegistry>()
        .get(item_id)
        .cloned()
        .ok_or_else(|| format!("unknown item '{item_id}'"))?;
    if def.kind != ItemKind::Passive {
        return Err(format!("'{item_id}' is not equipment (kind {:?})", def.kind));
    }
    if world.get::<EquipSlots>(tower).is_none() {
        return Err("tower has no equip slots".into());
    }

    // Same-slot replacement: stow the old piece first (fail before mutating).
    let existing = world.get::<EquipSlots>(tower).and_then(|r| r.slots[slot].clone());
    if let Some(old) = &existing {
        stow_into_bag(world, tower, &old.def)?;
    }

    // Slot source: reuse the existing marker, or spawn one for a fresh slot.
    let source = match &existing {
        Some(old) => old.source,
        None => world.spawn_empty().id(),
    };
    if let Some(old) = &existing {
        strip_slot_entries(world, tower, old.source);
    }
    if let Some(passive) = &def.passive {
        let Ok(mut holder) = world.get_entity_mut(tower) else { return Err("tower gone".into()) };
        let mut mods = holder.entry::<StatMods>().or_default();
        mods.into_mut().upsert(ModEntry {
            source,
            kind: passive.kind,
            amount: passive.amount,
            expires_at: None,
            from_aura: false,
        });
    }
    let Some(mut rack) = world.get_mut::<EquipSlots>(tower) else { return Err("tower gone".into()) };
    rack.slots[slot] = Some(EquippedItem { def: def.id.clone(), source });
    info!("[loop-td] equip '{}' on tower {:?} slot {slot}", def.id, tower);
    Ok(())
}

/// Remove the piece in `slot`: strips its StatMods entries and stows it back
/// into the bag (a full bag refuses). Returns the removed item id. Range
/// affixes need no separate restore — they rode the same StatMods entry.
pub fn unequip_slot(world: &mut World, tower: Entity, slot: usize) -> Result<String, String> {
    let Some(item) = world.get::<EquipSlots>(tower).and_then(|r| r.slots[slot].clone()) else {
        return Err(format!("slot {slot} is empty"));
    };
    stow_into_bag(world, tower, &item.def)?;
    strip_slot_entries(world, tower, item.source);
    if let Some(mut rack) = world.get_mut::<EquipSlots>(tower) {
        rack.slots[slot] = None;
    }
    Ok(item.def)
}

/// Minimal input entry (full panel is the LT2-UI line): E over the build
/// cursor equips the first equipment-class item in the hero's bag into the
/// first free equip slot.
pub fn equip_input(world: &mut World) {
    if !world.contains_resource::<ButtonInput<KeyCode>>() {
        return;
    }
    if *world.resource::<MatchFlow>() != MatchFlow::Running {
        return; // E at the end screen means "endless restart" (end_screen_input)
    }
    let keys = world.resource::<ButtonInput<KeyCode>>().clone();
    if !keys.just_pressed(KeyCode::KeyE) {
        return;
    }
    // SEAM3c (LT2 key envelope): record the raw E press when recording is
    // armed — game-side vocabulary the base recorder does not know (same
    // precedent as the game-side econ filtering). The envelope re-injects
    // as a KeyboardInput message, so a replay/spectate run re-fires this
    // exact path in the same frame.
    if let Some(mut rec) = world.get_resource_mut::<RecordingLog>() {
        append_envelope(
            &mut rec,
            CommandEnvelope::Key { code: "KeyE".into() },
        );
    }
    let cell = world.resource::<crate::systems::gameplay::BuildCursor>().0;
    let Some(hero) = world.resource::<BuildGrid>().occupant(cell) else { return };
    if world.get::<EquipSlots>(hero).is_none() {
        return;
    }
    // First equipment-class stack in the bag (slot entities, SEAM3b).
    let candidate: Option<(usize, String)> = world.get::<Inventory>(hero).and_then(|inv| {
        inv.slots.iter().enumerate().find_map(|(i, s)| {
            let &stack_e = s.as_ref()?;
            let def_id = world.get::<ItemStack>(stack_e)?.def.clone();
            let is_equipment = world
                .resource::<ItemRegistry>()
                .get(&def_id)
                .is_some_and(|d| d.kind == ItemKind::Passive);
            is_equipment.then(|| (i, def_id))
        })
    });
    let Some((bag_slot, item_id)) = candidate else {
        info!("[loop-td] equip: no equipment in the bag");
        return;
    };
    let Some(free) = world
        .get::<EquipSlots>(hero)
        .and_then(|r| r.slots.iter().position(|s| s.is_none()))
    else {
        info!("[loop-td] equip: all slots occupied");
        return;
    };
    // The piece moves bag -> slot: clear the bag stack BEFORE equip_item's
    // replacement stow (it only stows on replacement, so a plain clear works).
    let bag_stack = world
        .get_mut::<Inventory>(hero)
        .and_then(|mut inv| inv.slots[bag_slot].take());
    if let Some(stack_e) = bag_stack {
        let _ = world.get_entity_mut(stack_e).map(|mut e| e.despawn());
    }
    if let Err(e) = equip_item(world, hero, free, &item_id) {
        info!("[loop-td] equip failed: {e}");
        stow_into_bag(world, hero, &item_id).ok(); // put it back
    }
}
