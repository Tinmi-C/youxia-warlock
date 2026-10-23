//! Items & inventory (WC3 alignment, ADR-0016 §8): defs live in the map
//! bundle; use-effects ride the existing SkillCast channel (an item is a
//! levelless skill — effect systems dispatch by id and stay untouched);
//! carried passives inject StatMods entries (ADR-0011 semantics).
//!
//! Scheduling (D2): command consumption and cooldown/grace timers tick in
//! `WarbaseSet::Tick`; the proximity pickup scan and loot drops run in
//! `WarbaseSet::Cleanup` (drops ordered `before(combat::cleanup_dead)` so
//! the corpse's Transform is still readable).
//!
//! SEAM notes (domain increments may not edit lib.rs, D3):
//! - `ItemEvents` (rejections) and `ItemCommands` / `ShopCommands`
//!   (requests) are polled Vec RESOURCES, not messages: registering a new
//!   message type needs `add_message` in lib.rs, which is frozen for domain
//!   increments. Games push commands before their update and drain events
//!   after it (`std::mem::take`). The pickup seam is observation-only on
//!   purpose: MOV4's `Order::Pickup` execution lives in the movement
//!   module; here a carrier within `body_radius + PICKUP_MARGIN` of a
//!   ground item auto-stows it, so the two compose without coupling.
//! - Passive carry uses the HOLDER entity as the ModEntry source, so two
//!   carried passives with the same StatKind overwrite each other (upsert
//!   is keyed on (source, kind)) and a hero's level-growth DamageMult
//!   entry (`skill::distribute_xp`, same source) collides with a
//!   DamageMult sword. Known semantic flaw, pinned by the INV task: the
//!   fix is per-stack source entities ("ItemStack entity-ization"),
//!   deferred to Phase 4 RL validation instead of guessed here.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::buff::{ModEntry, StatKind, StatMods};
use crate::combat::{Dead, UnitDied};
use crate::econ::{EconChanged, Population, Resources};
use crate::skill::{SkillCast, SkillDefs};
use crate::unit::{UnitInstance, UnitRegistry};

pub const INVENTORY_SLOTS: usize = 6;
/// Pickup reach = carrier body_radius + this margin (walk-over pickup).
pub const PICKUP_MARGIN: f32 = 0.2;
/// Loot drops spawn this far from the corpse (random direction, seeded).
pub const LOOT_SCATTER_RADIUS: f32 = 0.3;
/// A dropped item stays unpickable for this long so the dropper does not
/// instantly re-stow it at their own feet (no movement coupling here).
pub const DROP_GRACE: f32 = 1.0;
/// Sell refund: cost_gold divided by this (WC3-style half-price sellback).
pub const SELL_REFUND_DIVISOR: u32 = 2;

/// Map-bundle item section (SEAM0d; optional, old maps default empty).
/// `drops` is the INV3 loot table: dying UnitId -> (ItemId, chance) rolls.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ItemsDef {
    #[serde(default)]
    pub items: HashMap<String, ItemDef>,
    #[serde(default)]
    pub drops: HashMap<String, Vec<(String, f32)>>,
}

/// One carried-passive stat injected into the holder's StatMods while the
/// stack occupies a slot (kind + amount ship per map, like aura defs).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PassiveStat {
    pub kind: StatKind,
    pub amount: f32,
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
    /// Shop price in gold (INV3); doubles as the sell-refund base.
    #[serde(default)]
    pub cost_gold: u32,
    /// Carried passive (Passive kind): injected into the holder's StatMods.
    #[serde(default)]
    pub passive: Option<PassiveStat>,
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

impl ItemRegistry {
    pub fn get(&self, id: &str) -> Option<&ItemDef> {
        self.defs.get(id)
    }
}

/// Assembled loot table (INV3): dying UnitId -> (ItemId, chance) rolls.
#[derive(Resource, Debug, Default, Clone)]
pub struct DropTable(pub HashMap<String, Vec<(String, f32)>>);

/// A ground item entity. `grace` is the seconds until pickable: 0 for
/// loot (pick up on walk-over), [`DROP_GRACE`] for player drops so the
/// dropper does not instantly re-stow the item at their own feet.
#[derive(Component, Debug)]
pub struct GroundItem {
    pub def: String,
    pub grace: f32,
}

/// Fixed 6-slot inventory on a hero (None = empty; WC3 layout).
#[derive(Component, Debug, Default)]
pub struct Inventory {
    pub slots: [Option<ItemStack>; INVENTORY_SLOTS],
}

impl Inventory {
    /// Index of the first empty slot (pickup stow target).
    pub fn first_free_slot(&self) -> Option<usize> {
        self.slots.iter().position(|s| s.is_none())
    }
}

/// A stack occupying one inventory slot.
#[derive(Debug, Clone)]
pub struct ItemStack {
    pub def: String,
    pub charges: u8,
    /// Seconds until the next use (def.cooldown set at each use; Tick
    /// decrements; use is blocked while > 0).
    pub cooldown: f32,
}

impl ItemStack {
    /// Fresh stack from a def. Charge defaults by kind: Consumable and
    /// Charged fall back to a single charge when the def omits one;
    /// Passives carry nothing to spend.
    pub fn from_def(def: &ItemDef) -> Self {
        let charges = match def.kind {
            ItemKind::Consumable | ItemKind::Charged => def.charges.unwrap_or(1),
            ItemKind::Passive => def.charges.unwrap_or(0),
        };
        Self { def: def.id.clone(), charges, cooldown: 0.0 }
    }
}

/// Readable rejection reasons for pickup/use/shop flows (UI-facing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemReject {
    InventoryFull,
    UnknownItem,
    NoCharges,
}

/// Rejection events, appended by the item systems. A polled Vec resource
/// instead of a message because message registration lives in the frozen
/// lib.rs (D3); games drain it (e.g. `std::mem::take`) in their UI pass.
/// Appends are per-frame per-attempt: a carrier standing on an item it
/// cannot stow adds one entry per frame until it drains or moves away.
#[derive(Resource, Debug, Default)]
pub struct ItemEvents(pub Vec<ItemReject>);

/// Player-facing item requests, same resource-event seam as ItemEvents:
/// games push before their update; the Tick consumer drains in arrival
/// order (the message-arrival-order semantics of D4 without lib.rs).
#[derive(Debug, Clone)]
pub enum ItemCommand {
    /// Use the stack in `slot` (charges + cooldown gated; fires SkillCast).
    Use { user: Entity, slot: usize },
    /// Drop the stack in `slot` back onto the ground at the holder's feet.
    Drop { holder: Entity, slot: usize },
}

#[derive(Resource, Debug, Default)]
pub struct ItemCommands(pub Vec<ItemCommand>);

/// Shop stock list (INV3): item ids this shop sells at their ItemDef
/// `cost_gold`; selling refunds cost_gold / SELL_REFUND_DIVISOR.
#[derive(Component, Debug)]
pub struct Shop {
    pub stock: Vec<String>,
}

/// Player-facing shop requests (resource-event seam as above).
#[derive(Debug, Clone)]
pub enum ShopCommand {
    /// Buy `item` from `shop` into the buyer's first free slot (charged on
    /// success only).
    Buy { buyer: Entity, shop: Entity, item: String },
    /// Sell the stack in the seller's `slot` for half its cost_gold.
    Sell { seller: Entity, slot: usize },
}

#[derive(Resource, Debug, Default)]
pub struct ShopCommands(pub Vec<ShopCommand>);

// ---------------------------------------------------------------------------
// Helpers shared by the pickup/use/shop flows
// ---------------------------------------------------------------------------

fn passive_entry(holder: Entity, p: &PassiveStat) -> ModEntry {
    ModEntry {
        source: holder,
        kind: p.kind,
        amount: p.amount,
        expires_at: None,
        from_aura: false,
    }
}

/// Grant (upsert) the carried-passive entry; inserts a bare StatMods when
/// the holder spawned without one (spawn_unit always adds one).
fn grant_passive(
    stats: &mut Query<&mut StatMods>,
    commands: &mut Commands,
    holder: Entity,
    def: &ItemDef,
) {
    let Some(p) = &def.passive else { return };
    match stats.get_mut(holder) {
        Ok(mut mods) => mods.upsert(passive_entry(holder, p)),
        Err(_) => {
            commands
                .entity(holder)
                .insert(StatMods { entries: vec![passive_entry(holder, p)] });
        }
    }
}

/// Revoke the carried-passive entry. Source is the holder entity (see the
/// module docs: same-kind entries from other same-source writers — hero
/// level growth, a second same-kind sword — are removed with it).
fn revoke_passive(stats: &mut Query<&mut StatMods>, holder: Entity, def: &ItemDef) {
    let Some(p) = &def.passive else { return };
    if let Ok(mut mods) = stats.get_mut(holder) {
        mods.entries.retain(|e| !(e.source == holder && e.kind == p.kind));
    }
}

/// Stow a fresh stack from `def` into the first free slot; None when full.
fn try_stow(inv: &mut Inventory, def: &ItemDef) -> Option<usize> {
    let slot = inv.first_free_slot()?;
    inv.slots[slot] = Some(ItemStack::from_def(def));
    Some(slot)
}

/// `EconChanged` snapshot (same shape econ::apply_econ publishes), so shop
/// gold flows refresh UI panels exactly like tower builds and trains.
fn econ_changed(res: &Resources, pop: &Population) -> EconChanged {
    EconChanged { gold: res.gold, wood: res.wood, pop_used: pop.used, pop_cap: pop.cap }
}

// ---------------------------------------------------------------------------
// Bundle dispatch (SEAM0d): INV1 fills the registry + loot table.
// ---------------------------------------------------------------------------

pub fn apply_items(world: &mut World, bundle: &crate::bundle::MapBundle) {
    // insert (not resource_mut): assemble runs on worlds that do NOT add
    // WarbasePlugin (warden's legacy stack loads bundles through it while
    // initializing only the resources it consumes), and the registry is
    // assemble-authoritative anyway — same pattern as UnitRegistry.
    world.insert_resource(ItemRegistry { defs: bundle.items.items.clone() });
    world.insert_resource(DropTable(bundle.items.drops.clone()));
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Cleanup: observation-mode pickup. Every live carrier (Inventory, not
/// Dead) auto-stows ground items within `body_radius + PICKUP_MARGIN`
/// into its first free slot; a ground item that no in-reach carrier can stow
/// emits exactly one `ItemReject::InventoryFull` and stays on the ground.
/// Both sides iterate sorted by Entity index (D6 determinism).
pub fn pickup_items(
    registry: Res<ItemRegistry>,
    unit_defs: Res<UnitRegistry>,
    mut holders: Query<
        (Entity, &Transform, &mut Inventory, Option<&UnitInstance>),
        Without<Dead>,
    >,
    mut stats: Query<&mut StatMods>,
    grounds: Query<(Entity, &Transform, &GroundItem)>,
    mut events: ResMut<ItemEvents>,
    mut commands: Commands,
) {
    if grounds.is_empty() {
        return; // zero drift: item-less maps never pass this gate
    }
    let mut carriers: Vec<(Entity, Vec3, f32)> = holders
        .iter()
        .map(|(e, pos, _, instance)| {
            let radius = instance
                .and_then(|i| unit_defs.get(&i.def))
                .map_or(0.0, |d| d.attrs.body_radius);
            (e, pos.translation, radius)
        })
        .collect();
    carriers.sort_by_key(|&(e, _, _)| e.index());

    let mut drops: Vec<(Entity, Vec3, String, f32)> = grounds
        .iter()
        .map(|(e, pos, item)| (e, pos.translation, item.def.clone(), item.grace))
        .collect();
    drops.sort_by_key(|&(e, _, _, _)| e.index());

    for (ground_e, ground_pos, def_id, grace) in drops {
        if grace > 0.0 {
            continue; // freshly dropped at someone's feet (DROP_GRACE)
        }
        let Some(def) = registry.get(&def_id) else {
            events.0.push(ItemReject::UnknownItem);
            continue; // unknown defs stay on the ground, visible to debug
        };
        let mut reached = false;
        let mut stowed = false;
        for &(carrier, carrier_pos, radius) in &carriers {
            if carrier_pos.distance(ground_pos) > radius + PICKUP_MARGIN {
                continue;
            }
            reached = true;
            let Ok((_, _, mut inv, _)) = holders.get_mut(carrier) else {
                continue;
            };
            if try_stow(&mut inv, def).is_some() {
                grant_passive(&mut stats, &mut commands, carrier, def);
                commands.entity(ground_e).despawn();
                stowed = true;
                break;
            }
        }
        if reached && !stowed {
            events.0.push(ItemReject::InventoryFull);
        }
    }
}

/// Tick: per-stack use cooldowns and ground-item grace windows tick down
/// (clamped at zero, same contract as skill slot cooldowns).
pub fn tick_item_timers(time: Res<Time>, mut invs: Query<&mut Inventory>, mut grounds: Query<&mut GroundItem>) {
    let dt = time.delta_secs();
    for mut inv in &mut invs {
        for slot in &mut inv.slots {
            if let Some(stack) = slot.as_mut() {
                stack.cooldown = (stack.cooldown - dt).max(0.0);
            }
        }
    }
    for mut item in &mut grounds {
        item.grace = (item.grace - dt).max(0.0);
    }
}

/// Tick: drain `ItemCommands` (arrival order). Use validates charges and
/// the stack cooldown, spends a charge, and — when the def's effect id is
/// registered — writes a SkillCast directly onto the existing message
/// stream (the item-as-levelless-skill seam; effect systems are read-only
/// consumers and stay untouched). An exhausted stack leaves its slot and
/// its passive entry goes with it. Drop returns the stack to the ground
/// under the holder with [`DROP_GRACE`] of unpickable time and revokes
/// the passive entry. Cooldown-gated uses skip silently (no reject
/// vocabulary for it — same stance as refused casts in the skill module).
pub fn consume_item_commands(
    mut commands_in: ResMut<ItemCommands>,
    registry: Res<ItemRegistry>,
    skill_defs: Res<SkillDefs>,
    mut holders: Query<&mut Inventory>,
    transforms: Query<&Transform>,
    mut stats: Query<&mut StatMods>,
    mut events: ResMut<ItemEvents>,
    mut casts: MessageWriter<SkillCast>,
    mut commands: Commands,
) {
    for cmd in std::mem::take(&mut commands_in.0) {
        match cmd {
            ItemCommand::Use { user, slot } => {
                let Ok(mut inv) = holders.get_mut(user) else { continue };
                let Some(stack) = inv.slots.get_mut(slot).and_then(|s| s.as_mut()) else {
                    continue;
                };
                if stack.charges == 0 {
                    events.0.push(ItemReject::NoCharges);
                    continue;
                }
                if stack.cooldown > 0.0 {
                    continue;
                }
                let Some(def) = registry.get(&stack.def).cloned() else {
                    events.0.push(ItemReject::UnknownItem);
                    continue;
                };
                if def.kind == ItemKind::Passive {
                    continue; // carried, not used
                }
                stack.charges -= 1;
                stack.cooldown = def.cooldown;
                let exhausted = stack.charges == 0;
                if exhausted {
                    inv.slots[slot] = None;
                    revoke_passive(&mut stats, user, &def);
                }
                if let Some(effect) = &def.effect {
                    if skill_defs.get(effect).is_some() {
                        casts.write(SkillCast {
                            caster: user,
                            skill: effect.clone(),
                            level: 1,
                            point: None,
                        });
                    }
                }
            }
            ItemCommand::Drop { holder, slot } => {
                let Ok(mut inv) = holders.get_mut(holder) else { continue };
                let Some(stack) = inv.slots.get(slot).cloned().flatten() else { continue };
                let Some(def) = registry.get(&stack.def).cloned() else {
                    events.0.push(ItemReject::UnknownItem);
                    continue;
                };
                inv.slots[slot] = None;
                revoke_passive(&mut stats, holder, &def);
                let pos = transforms.get(holder).map(|t| t.translation).unwrap_or_default();
                commands.spawn((
                    GroundItem { def: stack.def, grace: DROP_GRACE },
                    Transform::from_translation(pos),
                ));
            }
        }
    }
}

/// Tick: drain `ShopCommands`. Buy validates stock, def, funds, and a free
/// slot — charging only on success (an `InventoryFull` or empty-pocket buy
/// emits the reject / stays silent and charges nothing). Sell removes the
/// stack, revokes its passive, and refunds `cost_gold / 2` (pinned WC3
/// sellback). Gold mutations publish an `EconChanged` snapshot like every
/// other direct Resources writer in the base.
pub fn consume_shop_commands(
    mut commands_in: ResMut<ShopCommands>,
    registry: Res<ItemRegistry>,
    mut res: ResMut<Resources>,
    pop: Res<Population>,
    shops: Query<&Shop>,
    mut holders: Query<&mut Inventory>,
    mut stats: Query<&mut StatMods>,
    mut events: ResMut<ItemEvents>,
    mut changed: MessageWriter<EconChanged>,
    mut commands: Commands,
) {
    for cmd in std::mem::take(&mut commands_in.0) {
        match cmd {
            ShopCommand::Buy { buyer, shop, item } => {
                let Ok(shop) = shops.get(shop) else { continue };
                if !shop.stock.iter().any(|s| s == &item) {
                    continue;
                }
                let Some(def) = registry.get(&item) else {
                    events.0.push(ItemReject::UnknownItem);
                    continue;
                };
                if res.gold < def.cost_gold {
                    continue; // empty pockets: silent, no charge, no item
                }
                let Ok(mut inv) = holders.get_mut(buyer) else { continue };
                match try_stow(&mut inv, def) {
                    Some(_) => {
                        res.gold -= def.cost_gold;
                        grant_passive(&mut stats, &mut commands, buyer, def);
                        changed.write(econ_changed(&res, &pop));
                    }
                    None => events.0.push(ItemReject::InventoryFull),
                }
            }
            ShopCommand::Sell { seller, slot } => {
                let Ok(mut inv) = holders.get_mut(seller) else { continue };
                let Some(stack) = inv.slots.get(slot).cloned().flatten() else { continue };
                let Some(def) = registry.get(&stack.def).cloned() else {
                    events.0.push(ItemReject::UnknownItem);
                    continue;
                };
                inv.slots[slot] = None;
                revoke_passive(&mut stats, seller, &def);
                res.gold += def.cost_gold / SELL_REFUND_DIVISOR;
                changed.write(econ_changed(&res, &pop));
            }
        }
    }
}

/// Cleanup (before `combat::cleanup_dead` so the corpse still has a
/// Transform): roll each dying def's loot table through `MatchRng` (D6
/// determinism: message order x Vec order x seeded rolls) and scatter
/// winners on the `LOOT_SCATTER_RADIUS` circle around the corpse.
pub fn drop_loot(
    mut deaths: MessageReader<UnitDied>,
    tables: Res<DropTable>,
    mut rng: ResMut<crate::rng::MatchRng>,
    transforms: Query<&Transform>,
    mut commands: Commands,
) {
    if tables.0.is_empty() {
        let _ = deaths.read().count(); // keep the cursor fresh anyway
        return;
    }
    for death in deaths.read() {
        let Some(rolls) = tables.0.get(&death.def) else { continue };
        let Some(pos) = transforms.get(death.entity).ok().map(|t| t.translation) else {
            continue;
        };
        for (item, chance) in rolls {
            if rng.next_f32() < *chance {
                let angle = rng.next_f32() * std::f32::consts::TAU;
                let offset = Vec3::new(angle.cos(), 0.0, angle.sin()) * LOOT_SCATTER_RADIUS;
                commands.spawn((
                    GroundItem { def: item.clone(), grace: 0.0 },
                    Transform::from_translation(pos + offset),
                ));
            }
        }
    }
}

/// Domain systems (registered once by WarbasePlugin; INV increments add
/// systems HERE, never in lib.rs).
pub struct ItemPlugin;

impl Plugin for ItemPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ItemEvents>()
            .init_resource::<ItemCommands>()
            .init_resource::<ShopCommands>()
            .init_resource::<DropTable>()
            .add_systems(
                Update,
                (tick_item_timers, consume_item_commands, consume_shop_commands)
                    .chain()
                    // After the econ command stream settles (same-frame
                    // bounty income visible to shop validations), matching
                    // the production module's ordering stance.
                    .after(crate::econ::apply_econ)
                    .in_set(crate::WarbaseSet::Tick),
            )
            .add_systems(
                Update,
                drop_loot
                    .before(crate::combat::cleanup_dead)
                    .in_set(crate::WarbaseSet::Cleanup),
            )
            .add_systems(Update, pickup_items.in_set(crate::WarbaseSet::Cleanup));
    }
}

pub mod prelude {
    pub use super::{
        DropTable, GroundItem, Inventory, ItemCommand, ItemCommands, ItemDef, ItemEvents, ItemKind,
        ItemPlugin, ItemReject, ItemStack, ItemRegistry, ItemsDef, PassiveStat, Shop, ShopCommand,
        ShopCommands, DROP_GRACE, INVENTORY_SLOTS, LOOT_SCATTER_RADIUS, PICKUP_MARGIN,
        SELL_REFUND_DIVISOR,
    };
}
