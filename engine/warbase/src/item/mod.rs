//! Items & inventory (WC3 alignment, ADR-0016): defs live in the map bundle;
//! use-effects ride the existing SkillCast channel (an item is a levelless
//! skill — effect systems stay untouched); carried passives inject StatMods
//! entries (ADR-0011 semantics, source = the item stack entity).
//!
//! Scheduling: pickups resolve in Cleanup; usage flows through CastSkill.
//! SEAM0b pins the types.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const INVENTORY_SLOTS: usize = 6;

/// Map-bundle item table (SEAM0d; optional, old maps default empty).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ItemsDef {
    #[serde(default)]
    pub items: HashMap<String, ItemDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemDef {
    pub id: String,
    pub name: String,
    pub kind: ItemKind,
    /// Charges for Charged items (consumed per use; None = unlimited uses
    /// for Consumable one-shots, passive for Passive).
    pub charges: Option<u8>,
    /// Shared use cooldown (seconds).
    #[serde(default)]
    pub cooldown: f32,
    /// Effect = an id into SkillDefs (INV2: use emits SkillCast with this id;
    /// the def carries targeting Point/Unit/None exactly like skills).
    pub effect: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemKind {
    /// One-shot consumable (potion archetype).
    Consumable,
    /// Limited charges (tome/scroll archetype).
    Charged,
    /// Carried passive (stat stick archetype).
    Passive,
}

#[derive(Resource, Debug, Default, Clone)]
pub struct ItemRegistry {
    pub defs: HashMap<String, ItemDef>,
}

/// A ground item entity (pickable via Order::Pickup; INV1 spawns/renders).
#[derive(Component, Debug)]
pub struct GroundItem {
    pub def: String,
}

/// Fixed 6-slot inventory on a hero (None = empty; WC3 layout).
#[derive(Component, Debug, Default)]
pub struct Inventory {
    pub slots: [Option<ItemStack>; INVENTORY_SLOTS],
}

/// A stack occupying one inventory slot.
#[derive(Debug, Clone)]
pub struct ItemStack {
    pub def: String,
    pub charges: u8,
}

/// Readable rejection reasons for pickup/use flows (UI-facing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemReject {
    InventoryFull,
    UnknownItem,
    NoCharges,
}

/// Bundle dispatch stub (SEAM0d): INV1 fills this in.
pub fn apply_items(world: &mut World, bundle: &crate::bundle::MapBundle) {
    let _ = (world, bundle);
    // TODO(INV1): insert ItemRegistry from ItemsDef.
}

/// Domain systems (registered once by WarbasePlugin; INV increments add
/// systems HERE, never in lib.rs).
pub struct ItemPlugin;

impl Plugin for ItemPlugin {
    fn build(&self, _app: &mut App) {
        // TODO(INV1): pickup resolution in Cleanup.
    }
}

pub mod prelude {
    pub use super::{GroundItem, Inventory, ItemDef, ItemKind, ItemStack, ItemPlugin, ItemReject, ItemRegistry, ItemsDef, INVENTORY_SLOTS};
}
