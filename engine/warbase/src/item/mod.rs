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
//! SEAM notes:
//! - `ItemEvents` (rejections) is still a polled Vec RESOURCE on purpose
//!   (game UI drains it with `std::mem::take`); the request side flipped to
//!   MESSAGES in SEAM2c (`ItemCommands`/`ShopCommands`, one command per
//!   message, registered in lib.rs). The pickup seam stays
//!   observation-only: MOV4's `Order::Pickup` execution lives in the
//!   movement module; here a carrier within `body_radius + PICKUP_MARGIN`
//!   of a ground item auto-stows it, so the two compose without coupling.
//! - ITEM-STACK ENTITY MODEL (SEAM3b, the INV source-flaw fix): every stack
//!   occupying a bag slot is its OWN entity (`ItemStack` component + a
//!   `StackHolder` back-link); the slot holds the entity id, and carried
//!   passives source their ModEntry from the STACK entity, never the
//!   holder. Two same-kind swords now stack (distinct sources) instead of
//!   overwriting each other, and a hero's level-growth DamageMult entry
//!   (source = the hero, `skill::distribute_xp`) no longer collides with a
//!   carried sword. `cleanup_stacks` sweeps stacks whose holder is gone
//!   (death, restart) so entities never outlive their bag.

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

/// Fixed 6-slot inventory on a hero (None = empty; WC3 layout). SEAM3b:
/// each occupied slot holds the entity id of its `ItemStack` — stacks are
/// entities so their ModEntry sources are unique per stack.
#[derive(Component, Debug, Default)]
pub struct Inventory {
    pub slots: [Option<Entity>; INVENTORY_SLOTS],
}

impl Inventory {
    /// Index of the first empty slot (pickup stow target).
    pub fn first_free_slot(&self) -> Option<usize> {
        self.slots.iter().position(|s| s.is_none())
    }
}

/// One stack occupying one inventory slot (SEAM3b: a Component on its own
/// entity; `def`/`charges`/`cooldown` semantics unchanged).
#[derive(Component, Debug, Clone)]
pub struct ItemStack {
    pub def: String,
    pub charges: u8,
    /// Seconds until the next use (def.cooldown set at each use; Tick
    /// decrements; use is blocked while > 0).
    pub cooldown: f32,
}

/// Back-link from a stack entity to the holder whose bag it occupies (the
/// orphan-sweep key; kept as a component so `cleanup_stacks` never touches
/// the Inventory itself).
#[derive(Component, Debug)]
pub struct StackHolder(pub Entity);

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

/// Player-facing item requests (SEAM2c message flip): one message per
/// command, arrival order preserved by the message stream. The Tick
/// consumer drains them in arrival order (D4 semantics unchanged).
#[derive(Debug, Clone)]
pub enum ItemCommand {
    /// Use the stack in `slot` (charges + cooldown gated; fires SkillCast).
    Use { user: Entity, slot: usize },
    /// Drop the stack in `slot` back onto the ground at the holder's feet.
    Drop { holder: Entity, slot: usize },
}

/// One queued item command as a message (SEAM2c: the polled Vec resource
/// became a Message now that lib.rs registration is unlocked; the plural
/// name is kept so consumer call sites read the same).
#[derive(Message, Debug, Clone)]
pub struct ItemCommands(pub ItemCommand);

/// Shop stock list (INV3): item ids this shop sells at their ItemDef
/// `cost_gold`; selling refunds cost_gold / SELL_REFUND_DIVISOR.
#[derive(Component, Debug)]
pub struct Shop {
    pub stock: Vec<String>,
}

/// Player-facing shop requests (SEAM2c message flip, same shape as
/// ItemCommands).
#[derive(Debug, Clone)]
pub enum ShopCommand {
    /// Buy `item` from `shop` into the buyer's first free slot (charged on
    /// success only).
    Buy { buyer: Entity, shop: Entity, item: String },
    /// Sell the stack in the seller's `slot` for half its cost_gold.
    Sell { seller: Entity, slot: usize },
}

/// One queued shop command as a message (SEAM2c).
#[derive(Message, Debug, Clone)]
pub struct ShopCommands(pub ShopCommand);

// ---------------------------------------------------------------------------
// Helpers shared by the pickup/use/shop flows
// ---------------------------------------------------------------------------

/// Carried-passive ModEntry, SOURCED FROM THE STACK ENTITY (SEAM3b: unique
/// per stack — same-kind passives stack across items and never collide with
/// the holder's own entries).
fn passive_entry(stack: Entity, p: &PassiveStat) -> ModEntry {
    ModEntry {
        source: stack,
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
    stack: Entity,
    def: &ItemDef,
) {
    let Some(p) = &def.passive else { return };
    match stats.get_mut(holder) {
        Ok(mut mods) => mods.upsert(passive_entry(stack, p)),
        Err(_) => {
            commands
                .entity(holder)
                .insert(StatMods { entries: vec![passive_entry(stack, p)] });
        }
    }
}

/// Revoke the stack's carried-passive entry: strip EVERY entry that stack
/// sourced (all kinds — the stack is gone, its entries go with it).
fn revoke_passive(stats: &mut Query<&mut StatMods>, holder: Entity, stack: Entity) {
    if let Ok(mut mods) = stats.get_mut(holder) {
        mods.entries.retain(|e| e.source != stack);
    }
}

/// Stow a fresh stack from `def` into the first free slot. The stack is
/// SPAWNED as its own entity (Commands returns the reserved id immediately,
/// so the slot records it in the same system); None when the bag is full.
fn try_stow(
    commands: &mut Commands,
    holder: Entity,
    inv: &mut Inventory,
    def: &ItemDef,
) -> Option<(usize, Entity)> {
    let slot = inv.first_free_slot()?;
    let stack = commands
        .spawn((ItemStack::from_def(def), StackHolder(holder)))
        .id();
    inv.slots[slot] = Some(stack);
    Some((slot, stack))
}

/// Remove the stack occupying `slot` from a WORLD-driven context (tests,
/// exclusive systems): clear the slot, strip its StatMods entries, despawn
/// the entity. None when the holder or slot is already gone. System
/// contexts inline the same three steps (slot clear / strip / deferred
/// despawn) — see `consume_item_commands`.
pub fn remove_stack(world: &mut World, holder: Entity, slot: usize) -> Option<ItemStack> {
    let stack_e = world.get::<Inventory>(holder)?.slots.get(slot).copied().flatten()?;
    let stack = world.get::<ItemStack>(stack_e).cloned()?;
    if let Some(mut inv) = world.get_mut::<Inventory>(holder) {
        inv.slots[slot] = None;
    }
    if let Some(mut mods) = world.get_mut::<StatMods>(holder) {
        mods.entries.retain(|e| e.source != stack_e);
    }
    let _ = world.get_entity_mut(stack_e).map(|e| e.despawn());
    Some(stack)
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
            if let Some((_, stack_e)) = try_stow(&mut commands, carrier, &mut inv, def) {
                grant_passive(&mut stats, &mut commands, carrier, stack_e, def);
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
/// (clamped at zero, same contract as skill slot cooldowns). Stacks are
/// entities now (SEAM3b) — the query walks them directly.
pub fn tick_item_timers(
    time: Res<Time>,
    mut stacks: Query<&mut ItemStack>,
    mut grounds: Query<&mut GroundItem>,
) {
    let dt = time.delta_secs();
    for mut stack in &mut stacks {
        stack.cooldown = (stack.cooldown - dt).max(0.0);
    }
    for mut item in &mut grounds {
        item.grace = (item.grace - dt).max(0.0);
    }
}

/// Tick: drain `ItemCommands` (arrival order). Use validates charges and
/// the stack cooldown, spends a charge, and — when the def's effect id is
/// registered — writes a SkillCast directly onto the existing message
/// stream (the item-as-levelless-skill seam; effect systems are read-only
/// consumers and stay untouched). An exhausted stack leaves its slot, its
/// entity despawns, and its passive entries go with it. Drop returns the
/// stack to the ground under the holder with [`DROP_GRACE`] of unpickable
/// time and revokes the passive entry. Cooldown-gated uses skip silently
/// (no reject vocabulary for it — same stance as refused casts in the skill
/// module).
#[allow(clippy::type_complexity)]
pub fn consume_item_commands(
    mut commands_in: MessageReader<ItemCommands>,
    registry: Res<ItemRegistry>,
    skill_defs: Res<SkillDefs>,
    mut holders: Query<&mut Inventory>,
    stacks: Query<&ItemStack>,
    transforms: Query<&Transform>,
    mut stats: Query<&mut StatMods>,
    mut events: ResMut<ItemEvents>,
    mut casts: MessageWriter<SkillCast>,
    mut commands: Commands,
) {
    for wrapped in commands_in.read() {
        let cmd = wrapped.0.clone();
        match cmd {
            ItemCommand::Use { user, slot } => {
                // Snapshot the stack entity + def id first (single borrow).
                let (stack_e, def_id, charges, cooldown) = {
                    let Ok(inv) = holders.get(user) else { continue };
                    let Some(&stack_e) = inv.slots.get(slot).and_then(|s| s.as_ref()) else {
                        continue;
                    };
                    let Ok(stack) = stacks.get(stack_e) else { continue };
                    (stack_e, stack.def.clone(), stack.charges, stack.cooldown)
                };
                if charges == 0 {
                    events.0.push(ItemReject::NoCharges);
                    continue;
                }
                if cooldown > 0.0 {
                    continue;
                }
                let Some(def) = registry.get(&def_id).cloned() else {
                    events.0.push(ItemReject::UnknownItem);
                    continue;
                };
                if def.kind == ItemKind::Passive {
                    continue; // carried, not used
                }
                // Spend through the stack ENTITY (slot clear + entry strip
                // are immediate; the entity write/despawn is deferred and
                // flushes before any next-frame reader).
                let exhausted = charges == 1;
                if exhausted {
                    if let Ok(mut inv) = holders.get_mut(user) {
                        inv.slots[slot] = None;
                    }
                    revoke_passive(&mut stats, user, stack_e);
                    commands.entity(stack_e).despawn();
                } else {
                    commands.entity(stack_e).insert(ItemStack {
                        def: def_id.clone(),
                        charges: charges - 1,
                        cooldown: def.cooldown,
                    });
                }
                if let Some(effect) = &def.effect {
                    if skill_defs.get(effect).is_some() {
                        casts.write(SkillCast {
                            caster: user,
                            skill: effect.clone(),
                            level: 1,
                            target: None,
                            point: None,
                        });
                    }
                }
            }
            ItemCommand::Drop { holder, slot } => {
                let (stack_e, def_id) = {
                    let Ok(inv) = holders.get(holder) else { continue };
                    let Some(&stack_e) = inv.slots.get(slot).and_then(|s| s.as_ref()) else {
                        continue;
                    };
                    let def_id = stacks.get(stack_e).ok().map(|s| s.def.clone());
                    (stack_e, def_id)
                };
                let Some(def_id) = def_id else { continue };
                let Some(def) = registry.get(&def_id).cloned() else {
                    events.0.push(ItemReject::UnknownItem);
                    continue;
                };
                if let Ok(mut inv) = holders.get_mut(holder) {
                    inv.slots[slot] = None;
                }
                revoke_passive(&mut stats, holder, stack_e);
                commands.entity(stack_e).despawn();
                let pos = transforms.get(holder).map(|t| t.translation).unwrap_or_default();
                commands.spawn((
                    GroundItem { def: def.id.clone(), grace: DROP_GRACE },
                    Transform::from_translation(pos),
                ));
            }
        }
    }
}

/// Tick: drain `ShopCommands`. Buy validates stock, def, funds, and a free
/// slot — charging only on success (an `InventoryFull` or empty-pocket buy
/// emits the reject / stays silent and charges nothing). Sell removes the
/// stack entity (slot + passive entries + despawn) and refunds
/// `cost_gold / 2` (pinned WC3 sellback). Gold mutations publish an
/// `EconChanged` snapshot like every other direct Resources writer in the
/// base.
#[allow(clippy::type_complexity)]
pub fn consume_shop_commands(
    mut commands_in: MessageReader<ShopCommands>,
    registry: Res<ItemRegistry>,
    mut res: ResMut<Resources>,
    pop: Res<Population>,
    shops: Query<&Shop>,
    mut holders: Query<&mut Inventory>,
    stacks: Query<&ItemStack>,
    mut stats: Query<&mut StatMods>,
    mut events: ResMut<ItemEvents>,
    mut changed: MessageWriter<EconChanged>,
    mut commands: Commands,
) {
    for wrapped in commands_in.read() {
        let cmd = wrapped.0.clone();
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
                match try_stow(&mut commands, buyer, &mut inv, def) {
                    Some((_, stack_e)) => {
                        res.gold -= def.cost_gold;
                        grant_passive(&mut stats, &mut commands, buyer, stack_e, def);
                        changed.write(econ_changed(&res, &pop));
                    }
                    None => events.0.push(ItemReject::InventoryFull),
                }
            }
            ShopCommand::Sell { seller, slot } => {
                let (stack_e, def_id) = {
                    let Ok(inv) = holders.get(seller) else { continue };
                    let Some(&stack_e) = inv.slots.get(slot).and_then(|s| s.as_ref()) else {
                        continue;
                    };
                    (stack_e, stacks.get(stack_e).ok().map(|s| s.def.clone()))
                };
                let Some(def_id) = def_id else { continue };
                let Some(def) = registry.get(&def_id).cloned() else {
                    events.0.push(ItemReject::UnknownItem);
                    continue;
                };
                if let Ok(mut inv) = holders.get_mut(seller) {
                    inv.slots[slot] = None;
                }
                revoke_passive(&mut stats, seller, stack_e);
                commands.entity(stack_e).despawn();
                res.gold += def.cost_gold / SELL_REFUND_DIVISOR;
                changed.write(econ_changed(&res, &pop));
            }
        }
    }
}

/// Cleanup: sweep stack entities whose holder is gone (holder died to
/// `cleanup_dead`, or a game-side restart despawned it) — stacks never
/// outlive their bag (SEAM3b entity model's bookkeeping end).
pub fn cleanup_stacks(
    mut commands: Commands,
    stacks: Query<(Entity, &StackHolder)>,
    holders: Query<Entity, With<Inventory>>,
) {
    for (e, holder) in &stacks {
        if holders.get(holder.0).is_err() {
            commands.entity(e).despawn();
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
            .add_systems(Update, pickup_items.in_set(crate::WarbaseSet::Cleanup))
            // SEAM3b: stack entities never outlive their bag (holder death
            // / restart). After cleanup_dead so same-frame deaths sweep now.
            .add_systems(
                Update,
                cleanup_stacks
                    .after(crate::combat::cleanup_dead)
                    .in_set(crate::WarbaseSet::Cleanup),
            );
    }
}

pub mod prelude {
    pub use super::{
        cleanup_stacks, remove_stack, DropTable, GroundItem, Inventory, ItemCommand,
        ItemCommands, ItemDef, ItemEvents, ItemKind, ItemPlugin, ItemReject, ItemStack,
        ItemRegistry, ItemsDef, PassiveStat, Shop, ShopCommand, ShopCommands, StackHolder,
        DROP_GRACE, INVENTORY_SLOTS, LOOT_SCATTER_RADIUS, PICKUP_MARGIN, SELL_REFUND_DIVISOR,
    };
}
