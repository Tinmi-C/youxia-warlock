//! LT2-1 equipment on hero towers (v2 proposal §2 #1): 3 dedicated equip
//! slots on hero defs; each equipment piece is an `ItemDef` whose affix
//! (攻+X% / 间隔-X%) rides a `StatMods` entry injected into the tower with a
//! MARKER ENTITY per (tower, slot) as the entry source — the INV domain's
//! known flaw (passives use the holder as source, so same-kind entries
//! overwrite each other) is dodged by construction: every slot owns its own
//! source, same-slot replacement strips exactly that source's entries.
//!
//! ENGINE HOLE (recorded for the C-channel): `StatKind` has no range entry,
//! so the 射程+X affix cannot ride StatMods. The game-side `RANGE_AFFIXES`
//! table below carries it instead and `refresh_range` adjusts the tower's
//! `AttackKit.def.range` (base range from the registry + equipped bonuses),
//! restored on unequip. ItemDef-ridable numbers live in maps/equipment.ron
//! (merged into the in-memory bundle at load time — the engine seam test
//! pins the shipped loop.ron items-section-free; see maps/equipment.ron).

use bevy::prelude::*;
use warbase::prelude::*;

/// Equip slots per hero tower (proposal §2: 3).
pub const EQUIP_SLOTS: usize = 3;

/// Flat range affixes the engine stat vocabulary cannot express
/// (item id -> +range). Game-side config until StatKind grows a range kind.
pub const RANGE_AFFIXES: &[(&str, f32)] = &[("scope", 1.5)];

/// One occupied equip slot. `source` is the dedicated StatMods source entity
/// for this (tower, slot) pair — stable for the slot's lifetime.
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

/// Flat range bonus of an item id (0.0 when none) — the game-side table the
/// engine stat vocabulary cannot express. Public since LT2-UI reads it for
/// the 射程+X affix summaries.
pub fn range_affix(item: &str) -> f32 {
    RANGE_AFFIXES.iter().find(|(id, _)| *id == item).map(|(_, r)| *r).unwrap_or(0.0)
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

/// Recompute the tower's attack range: registry base + sum of equipped range
/// affixes (call after ANY equip-slot change).
fn refresh_range(world: &mut World, tower: Entity) {
    let Some(rack) = world.get::<EquipSlots>(tower).cloned() else { return };
    let base = world
        .get::<UnitInstance>(tower)
        .and_then(|i| world.resource::<UnitRegistry>().get(&i.def).cloned())
        .and_then(|d| d.attack.map(|a| a.range));
    let Some(base) = base else { return };
    let bonus: f32 = rack.slots.iter().filter_map(|s| s.as_ref()).map(|it| range_affix(&it.def)).sum();
    if let Some(mut kit) = world.get_mut::<AttackKit>(tower) {
        kit.def.range = base + bonus;
    }
}

/// Strip every StatMods entry owned by `source` on `tower` (all kinds — the
/// slot's source is exclusive to the slot).
fn strip_slot_entries(world: &mut World, tower: Entity, source: Entity) {
    if let Some(mut mods) = world.get_mut::<StatMods>(tower) {
        mods.entries.retain(|e| e.source != source);
    }
}

/// Stow an item stack into the tower's bag (Inventory) when there is room.
fn stow_into_bag(world: &mut World, tower: Entity, item: &str) -> Result<(), String> {
    let def = world
        .resource::<ItemRegistry>()
        .get(item)
        .cloned()
        .ok_or_else(|| format!("unknown item '{item}'"))?;
    let Some(mut inv) = world.get_mut::<Inventory>(tower) else {
        return Ok(()); // no bag (never happens for hero towers) — item is lost
    };
    match inv.first_free_slot() {
        Some(slot) => {
            inv.slots[slot] = Some(ItemStack::from_def(&def));
            Ok(())
        }
        None => Err("bag full".into()),
    }
}

/// Equip `item_id` into `slot` on a hero tower, replacing whatever the slot
/// held (the replaced piece goes back into the bag; a full bag refuses the
/// swap BEFORE anything changes). Stat affixes upsert through the slot's
/// dedicated source; range affixes recompute `AttackKit.def.range`.
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
    refresh_range(world, tower);
    info!("[loop-td] equip '{}' on tower {:?} slot {slot}", def.id, tower);
    Ok(())
}

/// Remove the piece in `slot`: strips its StatMods entries, recomputes the
/// range, and stows it back into the bag (a full bag refuses). Returns the
/// removed item id.
pub fn unequip_slot(world: &mut World, tower: Entity, slot: usize) -> Result<String, String> {
    let Some(item) = world.get::<EquipSlots>(tower).and_then(|r| r.slots[slot].clone()) else {
        return Err(format!("slot {slot} is empty"));
    };
    stow_into_bag(world, tower, &item.def)?;
    strip_slot_entries(world, tower, item.source);
    if let Some(mut rack) = world.get_mut::<EquipSlots>(tower) {
        rack.slots[slot] = None;
    }
    refresh_range(world, tower);
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
    let cell = world.resource::<crate::systems::gameplay::BuildCursor>().0;
    let Some(hero) = world.resource::<BuildGrid>().occupant(cell) else { return };
    if world.get::<EquipSlots>(hero).is_none() {
        return;
    }
    // First equipment-class stack in the bag.
    let candidate: Option<(usize, ItemStack)> = world.get::<Inventory>(hero).and_then(|inv| {
        inv.slots.iter().enumerate().find_map(|(i, s)| {
            let stack = s.as_ref()?;
            let is_equipment = world
                .resource::<ItemRegistry>()
                .get(&stack.def)
                .is_some_and(|d| d.kind == ItemKind::Passive);
            is_equipment.then(|| (i, stack.clone()))
        })
    });
    let Some((bag_slot, stack)) = candidate else {
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
    if let Some(mut inv) = world.get_mut::<Inventory>(hero) {
        inv.slots[bag_slot] = None;
    }
    if let Err(e) = equip_item(world, hero, free, &stack.def) {
        info!("[loop-td] equip failed: {e}");
        stow_into_bag(world, hero, &stack.def).ok(); // put it back
    }
}
