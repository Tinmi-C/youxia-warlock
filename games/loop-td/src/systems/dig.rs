//! LT2-4 treasure digs (v2 proposal §2 #4): during an intermission the
//! player spends 50 gold on one dig (key V). A single `MatchRng` roll maps
//! to 50% nothing / 35% +80 gold / 15% a random equipment piece (seeded
//! pick from the sorted equipment list). Equipment outcomes stow into the
//! first hero tower's bag with a free slot (Entity-index order, D6) or, with
//! no hero able to carry it, land as a `GroundItem` at the ring start so
//! the loot stays observable.
//!
//! EV note (pinned by test): gold outcomes alone have EV 0.35 x 80 = 28,
//! inside the proposal's 31.5±5 statistical window.

use bevy::prelude::*;
use warbase::prelude::*;

/// Gold spent per dig.
pub const DIG_COST: u32 = 50;
/// Gold paid by a gold outcome.
pub const DIG_GOLD_AMOUNT: u32 = 80;
/// Roll thresholds: < 0.50 nothing, < 0.85 gold, else equipment.
pub const DIG_EMPTY_UNTIL: f32 = 0.50;
pub const DIG_GOLD_UNTIL: f32 = 0.85;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DigOutcome {
    Nothing,
    Gold(u32),
    Equipment(String),
}

#[derive(Resource, Debug, Default)]
pub struct DigStats {
    pub rolls: u32,
    pub gold_won: u32,
    pub equipment_found: u32,
    pub last: Option<DigOutcome>,
}

/// Roll one dig: intermission-only, 50 gold, seeded outcomes.
pub fn dig(world: &mut World) -> Result<DigOutcome, String> {
    if *world.resource::<MatchFlow>() != MatchFlow::Running {
        return Err("match over".into());
    }
    if !matches!(
        world.resource::<WaveDirector>().phase,
        DirectorPhase::Intermission
    ) {
        return Err("digs only during the intermission".into());
    }
    if world.resource::<Resources>().gold < DIG_COST {
        return Err(format!("need {DIG_COST} gold to dig"));
    }
    world.resource_mut::<Resources>().gold -= DIG_COST;

    let roll = world.resource_mut::<MatchRng>().next_f32();
    let outcome = if roll < DIG_EMPTY_UNTIL {
        DigOutcome::Nothing
    } else if roll < DIG_GOLD_UNTIL {
        DigOutcome::Gold(DIG_GOLD_AMOUNT)
    } else {
        let items = crate::systems::equip::equipment_ids(world);
        let pick = world.resource_mut::<MatchRng>().pick(&items).cloned();
        match pick {
            Some(id) => DigOutcome::Equipment(id),
            None => DigOutcome::Nothing, // no equipment in the item table
        }
    };
    match &outcome {
        DigOutcome::Nothing => info!("[loop-td] dig: nothing"),
        DigOutcome::Gold(g) => {
            world.resource_mut::<Resources>().gold += *g;
            info!("[loop-td] dig: +{g} gold");
        }
        DigOutcome::Equipment(id) => {
            grant_equipment(world, id);
            info!("[loop-td] dig: equipment '{id}'");
        }
    }
    let mut stats = world.get_resource_or_insert_with(DigStats::default);
    stats.rolls += 1;
    match &outcome {
        DigOutcome::Gold(g) => stats.gold_won += *g,
        DigOutcome::Equipment(_) => stats.equipment_found += 1,
        DigOutcome::Nothing => {}
    }
    stats.last = Some(outcome.clone());
    Ok(outcome)
}

/// Stow an equipment id into the first hero tower with a free bag slot
/// (Entity-index order); otherwise drop it as a GroundItem at the ring start.
/// SEAM3b: bags are slot-entity backed — stowing spawns the stack entity.
fn grant_equipment(world: &mut World, item: &str) {
    let mut heroes: Vec<Entity> = Vec::new();
    {
        let mut q = world.query_filtered::<Entity, With<XpHolder>>();
        heroes.extend(q.iter(&*world));
    }
    heroes.sort_by_key(|e| e.index());
    for hero in heroes {
        let free_slot = world
            .get::<Inventory>(hero)
            .and_then(|inv| inv.first_free_slot());
        if let Some(slot) = free_slot {
            let def = world
                .resource::<ItemRegistry>()
                .get(item)
                .cloned()
                .expect("equipment ids come from the registry");
            let stack = world
                .spawn((ItemStack::from_def(&def), StackHolder(hero)))
                .id();
            if let Some(mut inv) = world.get_mut::<Inventory>(hero) {
                inv.slots[slot] = Some(stack);
            }
            return;
        }
    }
    // No hero can carry it: visible ground drop at the first path's start.
    let pos = world
        .resource::<Paths>()
        .0
        .first()
        .and_then(|&p| world.get::<PathDef>(p).map(|d| d.sample(0.0)))
        .unwrap_or_default();
    world.spawn((
        GroundItem { def: item.to_string(), grace: 0.0 },
        Transform::from_translation(Vec3::new(pos.x, 0.0, pos.y)),
    ));
}

/// Run-restart hook: per-run dig statistics reset with the match.
pub fn reset(world: &mut World) {
    world.remove_resource::<DigStats>();
}
