//! Gameplay core: map load + worker-population booking + the thin RTS layer
//! (unit kit attachment, train input). Thin-game rule: mechanisms are
//! warbase's; this crate wires them to the map file and composes them into
//! the acceptance slice.
//!
//! RL4 (C->A second pass): the starting layout (starting_forces section),
//! the harvest loop (harvest section + the base `Order::Harvest`/`ReturnCargo`
//! cycle) and free-agent auras (base `AuraDef::affects`) are all BASE now —
//! the former game-side incubators are deleted. What remains game-side:
//! the peasant auto-harvest order at spawn, hero slot pre-learning, shop
//! stock, and population booking for the opening workers.

use bevy::prelude::*;
use warbase::bundle::{assemble, load_bundle};
use warbase::prelude::*;

use crate::resources::CurrentMap;

/// Default map: the crate's own maps/arena.ron. Override with `--map <path>`
/// or env `RTS_LAB_MAP`.
pub fn default_map_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("RTS_LAB_MAP") {
        return p.into();
    }
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--map" {
            if let Some(p) = args.next() {
                return p.into();
            }
        }
    }
    std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/maps/arena.ron"))
}

// ---------------------------------------------------------------------------
// Skill defs (RL2): SkillDefs is a base RESOURCE with no bundle section —
// games author skill defs in Rust (SPL1's pinned stance). Registered at
// Startup BEFORE the starting layout spawns (spawn_unit reads SkillDefs to
// build the hero's SkillCaster slots).
// ---------------------------------------------------------------------------

pub const HOLY_LIGHT: &str = "holy_light";
pub const DEVOTION: &str = "devotion";
/// Item use-effect id (INV2: an item is a levelless skill fired as SkillCast).
pub const POTION_HEAL: &str = "potion_heal";
/// holy_light heal amount (GDD placeholder).
pub const HOLY_LIGHT_HEAL: f32 = 60.0;
/// healing_potion heal amount (GDD placeholder).
pub const POTION_HEAL_AMOUNT: f32 = 80.0;

/// Fill the base `SkillDefs` resource: holy_light (active, unit-targeted,
/// 40 mana / 6s cd), devotion (passive marker — its EFFECT is the def aura,
/// consumed by the base generalized `apply_follower_auras` since SEAM2b),
/// potion_heal (the item use-effect, instant).
pub fn register_skills(world: &mut World) {
    let mut defs = SkillDefs::default();
    defs.defs.insert(
        HOLY_LIGHT.into(),
        SkillDef {
            id: HOLY_LIGHT.into(),
            kind: SkillKind::Active,
            targeting: SkillTargeting::Unit,
            max_level: 1,
            cooldown: 6.0,
            mana_cost: 40.0,
            cast_time: 0.0,
            channel: None,
            area_radius: 0.0, summon: None,
            badge: Some('Q'),
            hotkey: None,
        },
    );
    defs.defs.insert(
        DEVOTION.into(),
        SkillDef {
            id: DEVOTION.into(),
            kind: SkillKind::Passive,
            targeting: SkillTargeting::None,
            max_level: 1,
            cooldown: 0.0,
            mana_cost: 0.0,
            cast_time: 0.0,
            channel: None,
            area_radius: 0.0, summon: None,
            badge: None,
            hotkey: None,
        },
    );
    defs.defs.insert(
        POTION_HEAL.into(),
        SkillDef {
            id: POTION_HEAL.into(),
            kind: SkillKind::Active,
            targeting: SkillTargeting::None,
            max_level: 1,
            cooldown: 0.0,
            mana_cost: 0.0,
            cast_time: 0.0,
            channel: None,
            area_radius: 0.0, summon: None,
            badge: None,
            hotkey: None,
        },
    );
    world.insert_resource(defs);
}

/// Exclusive startup: load + validate + assemble + build grid.
pub fn load_map(world: &mut World) {
    let path = default_map_path();
    let bundle = match load_bundle(&path) {
        Ok(b) => b,
        Err(e) => panic!("[rts-lab] failed to load map {}: {e}", path.display()),
    };
    assemble(world, &bundle);
    let grid = warbase::bundle::build_grid(&bundle.world);
    world.insert_resource(grid);
    world.insert_resource(CurrentMap(bundle));
    info!("[rts-lab] map loaded: {}", path.display());
}

// ---------------------------------------------------------------------------
// Starting layout (RL4: the layout itself is map content — starting_forces)
// ---------------------------------------------------------------------------

/// Def ids the game-side wiring references (must exist in maps/arena.ron;
/// the placements themselves live in its starting_forces section, hole-3
/// upcast — gold_mine/grunt have no game-side wiring anymore).
pub const HALL_DEF: &str = "main_hall";
pub const BARRACKS_DEF: &str = "barracks";
pub const PEASANT_DEF: &str = "peasant";
pub const FOOTMAN_DEF: &str = "footman";
pub const ARCHER_DEF: &str = "archer";
pub const PALADIN_DEF: &str = "paladin";
pub const SHOP_DEF: &str = "shop";

/// NOTE (hole #2b residue): which building is a shop and its stock list is
/// still game-side (the bundle has no shop-stock section); attach_rts_kit
/// attaches it per def. Stock: the potion and the passive sword from
/// maps/arena.ron items section.
const SHOP_STOCK: [&str; 2] = ["healing_potion", "blade_of_valor"];

/// Exclusive startup (after load_map): book `Population.used` for the
/// opening workers. The starting layout itself spawns inside `assemble`
/// (starting_forces, hole-3) which deliberately does NOT book population —
/// pop accounting is a game economy decision and rts-lab books its own
/// (training books its own too; the hero and the shop stay unbooked
/// placeholders — pop economics is not this slice's aim).
pub fn book_worker_population(world: &mut World) {
    let workers = world
        .query::<&UnitInstance>()
        .iter(world)
        .filter(|i| i.def == PEASANT_DEF)
        .count() as u8;
    world.resource_mut::<Population>().used = workers;
    info!("[rts-lab] starting layout: hall, barracks, shop, mine, paladin, {workers} peasants, 3 grunts");
}

// ---------------------------------------------------------------------------
// RTS kit attachment (every spawn path funnels here)
// ---------------------------------------------------------------------------

/// Distance-ascending nearest Mine with the entity-index tie-break (the
/// base's own `nearest` rule — D6: query iteration order never decides).
fn nearest_mine(mines: &[(Entity, Vec3)], from: Vec3) -> Option<Entity> {
    mines
        .iter()
        .map(|&(e, pos)| (from.distance_squared(pos), e.index_u32(), e))
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|(_, _, e)| e)
}

/// Attach the free-agent kit to freshly spawned units (Added<UnitInstance>
/// observer — covers both the starting_forces layout and PRD graduates):
/// - `Selectable` on every instance (buildings included, WC3 habit);
/// - `MoveAgent` + `AgentOrderState` when the def moves (peasant 3.0);
///   move_speed 0 defs (hall/mine/grunt) stay put — grunt 站桩守卫 comes free.
/// - peasants enter the BASE harvest loop from frame one: the agent spawns
///   with `Order::Harvest` at the nearest `Mine` (RL4: replaces the former
///   game-side `HarvestRoute` marker — the base `harvest_cycle` owns every
///   leg now; no mine on the map => plain idle worker).
/// - `Inventory` on heroes and peasants (RL2: walk-over pickup — the base
///   `pickup_items` observation seam serves every Inventory carrier; spawn
///   attaches it to NOBODY, so the thin wire lives here).
/// - `VisionSource` (RL3) on the seeing defs — radii are hard-coded GDD
///   placeholders: hall 8.0, paladin 7.0, footman/archer 5.5 (peasants see
///   nothing; the fog grid only lights from these sources).
/// - hero slots pre-learned at rank 1 (the RL2 acceptance stance: Q works
///   from minute zero; level points remain the intended path) and shop
///   stock attached to shop defs (hole #2b residue, map-content numbers).
pub fn vision_radius(def: &str) -> Option<f32> {
    match def {
        HALL_DEF => Some(8.0),
        PALADIN_DEF => Some(7.0),
        FOOTMAN_DEF | ARCHER_DEF => Some(5.5),
        _ => None,
    }
}

#[allow(clippy::type_complexity)]
pub fn attach_rts_kit(
    added: Query<(Entity, &Transform, &UnitInstance), Added<UnitInstance>>,
    registry: Res<UnitRegistry>,
    mines: Query<(Entity, &Transform), With<Mine>>,
    mut casters: Query<&mut SkillCaster>,
    mut commands: Commands,
) {
    let mine_spots: Vec<(Entity, Vec3)> = mines
        .iter()
        .map(|(e, tf)| (e, tf.translation))
        .collect();
    for (entity, tf, instance) in &added {
        let Some(def) = registry.get(&instance.def) else { continue };
        let mut ec = commands.entity(entity);
        // Visual-side hierarchy correctness: base spawn gives units a bare
        // Transform; mesh children (visualize_units) carry InheritedVisibility
        // whose propagation wants the parent to have it too (bevy B0004).
        // Visibility requires the inherited family — insert once, here, for
        // every spawn path. Inert headless (no render plugin reads it).
        ec.insert((Selectable, Visibility::default()));
        if def.attrs.move_speed > 0.0 {
            let order = if instance.def == PEASANT_DEF {
                nearest_mine(&mine_spots, tf.translation).map(|mine| Order::Harvest { target: mine })
            } else {
                None
            };
            ec.insert((
                MoveAgent {
                    speed: def.attrs.move_speed,
                    arrival_radius: 0.1,
                    waypoints: Vec::new(),
                    velocity: Vec3::ZERO,
                    body_radius: def.attrs.body_radius,
                    order,
                },
                AgentOrderState::Idle,
            ));
        }
        if def.hero || instance.def == PEASANT_DEF {
            ec.insert(Inventory::default());
        }
        if let Some(radius) = vision_radius(&instance.def) {
            ec.insert(VisionSource { radius });
        }
        if instance.def == SHOP_DEF {
            ec.insert(Shop { stock: SHOP_STOCK.map(String::from).to_vec() });
        }
        if def.hero {
            if let Ok(mut caster) = casters.get_mut(entity) {
                for slot in &mut caster.slots {
                    slot.level = 1;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Command-card inputs (minimal keyboard command card; headless no-ops)
// ---------------------------------------------------------------------------

/// Find the first selected entity matching a filter (selection order is
/// entity-index sorted, so this is deterministic).
fn first_selected(world: &World, mut f: impl FnMut(Entity) -> bool) -> Option<Entity> {
    world.resource::<SelectionState>().entities.iter().copied().find(|&e| f(e))
}

/// First selected building of def `def` that actually owns a production
/// queue (the train target for T/F routing).
fn producer_of(world: &World, def: &str) -> Option<Entity> {
    first_selected(world, |e| {
        world.get::<UnitInstance>(e).is_some_and(|i| i.def == def)
            && world.get::<ProductionQueue>(e).is_some()
    })
}

/// T / F keys -> TrainCommand on the SELECTED producer (RL2 routing):
/// - selection contains a barracks: T trains footman, F trains archer;
/// - else a main hall: T trains peasant (RL1 behavior), F nothing.
/// Headless (no ButtonInput) is a no-op — tests write TrainCommand directly.
pub fn train_input(world: &mut World) {
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>().cloned() else {
        return;
    };
    let (t, f) = (keys.just_pressed(KeyCode::KeyT), keys.just_pressed(KeyCode::KeyF));
    if !t && !f {
        return;
    }
    let (builder, unit) = match (producer_of(world, BARRACKS_DEF), producer_of(world, HALL_DEF)) {
        (Some(barracks), _) if t => (barracks, FOOTMAN_DEF),
        (Some(barracks), _) => (barracks, ARCHER_DEF), // F at the barracks
        (_, Some(hall)) if t => (hall, PEASANT_DEF),   // T at the hall (RL1)
        _ => return,
    };
    world.write_message(TrainCommand { builder, unit: unit.into() });
    info!("[rts-lab] train {unit} queued");
}

/// R key -> StartResearch("attack_upgrade") on the player research column
/// (base ResearchCommands message — SEAM2c flip; the consumer validates
/// gold/cap). Headless no-op.
pub fn research_input(world: &mut World) {
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>().cloned() else {
        return;
    };
    if !keys.just_pressed(KeyCode::KeyR) {
        return;
    }
    world.write_message(ResearchCommands(ResearchCommand::Start {
        id: "attack_upgrade".into(),
    }));
    info!("[rts-lab] research attack_upgrade started");
}

// ---------------------------------------------------------------------------
// Holy light: aim (Q key) + effect dispatch (SkillCast -> HealRequest)
// ---------------------------------------------------------------------------

/// Game component carrying the aimed holy_light target from the Q press to
/// the effect frame. NOTE (hole #6, channel C): the base `SkillCast` event
/// has NO unit-target field (only `point`), so a `SkillTargeting::Unit`
/// skill cannot convey its target through the event stream; `Casting` has a
/// reserved `target` field but the instant path never fills it. Until the
/// base grows a target field, the game side passes it via this side channel.
#[derive(Component, Debug)]
pub struct HolyLightAim(pub Entity);

/// Hard-coded aim policy (task spec): the most wounded ally (lowest hp)
/// inside the CURRENT SELECTION that is missing hp; nobody wounded or hero
/// selected alone -> the caster itself (self-heal). Alive-only.
pub fn aim_holy_light(world: &World, caster: Entity) -> Entity {
    let selection = &world.resource::<SelectionState>().entities;
    let wounded = || {
        selection
            .iter()
            .copied()
            .filter(|&e| {
                let (Some(hp), Some(max), Some(side), Some(cside)) = (
                    world.get::<Hp>(e),
                    world.get::<MaxHp>(e),
                    world.get::<Allegiance>(e),
                    world.get::<Allegiance>(caster),
                ) else {
                    return false;
                };
                hp.0 > 0.0 && hp.0 < max.0 && side == cside
            })
            .min_by(|a, b| {
                let ha = world.get::<Hp>(*a).map(|h| h.0).unwrap_or(f32::MAX);
                let hb = world.get::<Hp>(*b).map(|h| h.0).unwrap_or(f32::MAX);
                ha.total_cmp(&hb).then_with(|| a.index().cmp(&b.index()))
            })
    };
    wounded().unwrap_or(caster)
}

/// Q key: aim holy_light (policy above), park the target on the caster, and
/// request the cast (`CastSkill` -> base `process_casts` validates mana /
/// cooldown / level and fires `SkillCast`). Headless no-op — tests call
/// [`cast_holy_light`] directly.
pub fn skill_input(world: &mut World) {
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>().cloned() else {
        return;
    };
    if !keys.just_pressed(KeyCode::KeyQ) {
        return;
    }
    cast_holy_light(world);
}

/// The full game-side cast path (shared by the Q key and tests): select the
/// hero, aim, park the aim, write CastSkill on the holy_light slot.
pub fn cast_holy_light(world: &mut World) {
    let Some(hero) = first_selected(world, |e| {
        world.get::<SkillCaster>(e).is_some_and(|c| c.slots.iter().any(|s| s.skill == HOLY_LIGHT))
    }) else {
        info!("[rts-lab] Q: no holy_light caster selected");
        return;
    };
    let slot = world
        .get::<SkillCaster>(hero)
        .unwrap()
        .slots
        .iter()
        .position(|s| s.skill == HOLY_LIGHT)
        .unwrap();
    let target = aim_holy_light(world, hero);
    world.entity_mut(hero).insert(HolyLightAim(target));
    world.write_message(CastSkill { caster: hero, slot, target: None });
    info!("[rts-lab] Q: holy_light cast requested by {hero:?} at {target:?}");
}

/// Effect dispatch (after Act, before Resolve): SkillCast by skill id ->
/// - holy_light: HealRequest at the parked aim (HolyLightAim), then the
///   aim component is spent (one cast = one aim);
/// - potion_heal: HealRequest on the user (item use-effect, INV2).
/// The base `resolve_heals` (Resolve) is the single "hp goes up" writer.
pub fn skill_effects(
    mut casts: MessageReader<SkillCast>,
    mut heals: MessageWriter<HealRequest>,
    aims: Query<&HolyLightAim>,
    mut commands: Commands,
) {
    for cast in casts.read() {
        match cast.skill.as_str() {
            HOLY_LIGHT => {
                let Some(&HolyLightAim(target)) = aims.get(cast.caster).ok() else {
                    continue; // no aim parked: refuse (should not happen)
                };
                heals.write(HealRequest {
                    target,
                    source: cast.caster,
                    amount: HOLY_LIGHT_HEAL,
                });
                commands.entity(cast.caster).remove::<HolyLightAim>();
            }
            POTION_HEAL => {
                heals.write(HealRequest {
                    target: cast.caster,
                    source: cast.caster,
                    amount: POTION_HEAL_AMOUNT,
                });
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Items: use (1) + buy (B) inputs
// ---------------------------------------------------------------------------

/// 1 key -> ItemCommands Use{hero, slot 0} for the selected hero (task spec:
/// hardcoded slot 0 = the potion). NOTE: the base keyboard bridge maps
/// digits to control groups on the same press (unswappable — no modality
/// seam); with no group bound the recall clears the selection AFTER the use
/// fired. Recorded as channel-C friction (see the RL2 report).
pub fn item_input(world: &mut World) {
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>().cloned() else {
        return;
    };
    if !keys.just_pressed(KeyCode::Digit1) {
        return;
    }
    let Some(hero) = first_selected(world, |e| world.get::<Inventory>(e).is_some()) else {
        return;
    };
    world.write_message(ItemCommands(ItemCommand::Use { user: hero, slot: 0 }));
    info!("[rts-lab] 1: use item slot 0 on {hero:?}");
}

/// B key -> ShopCommand Buy{hero, THE shop, healing_potion} when the
/// selected hero stands within BUY_RANGE of the shop (the base shop consumer
/// does NOT range-check — the thin wire here does; hard-coded potion, task
/// spec). Headless no-op.
pub const BUY_RANGE: f32 = 3.0;
pub fn buy_input(world: &mut World) {
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>().cloned() else {
        return;
    };
    if !keys.just_pressed(KeyCode::KeyB) {
        return;
    }
    let Some(hero) = first_selected(world, |e| world.get::<Inventory>(e).is_some()) else {
        return;
    };
    let mut shops = world.query_filtered::<(Entity, &Transform), With<Shop>>();
    let Some((shop, shop_pos)) = shops.iter(world).next() else {
        return;
    };
    let hero_pos = world.get::<Transform>(hero).map(|t| t.translation);
    if hero_pos.is_none_or(|p| p.distance(shop_pos.translation) > BUY_RANGE) {
        info!("[rts-lab] B: shop out of range");
        return;
    }
    world.write_message(ShopCommands(ShopCommand::Buy {
        buyer: hero,
        shop,
        item: "healing_potion".into(),
    }));
    info!("[rts-lab] B: healing_potion bought");
}

/// SFX channel acceptance (RL3): minimal listener over the base `SfxRequest`
/// stream (hit/death/research-complete/match-end...). No audio assets in the
/// greybox — the println-equivalent log line proves the channel is wired; a
/// real game maps `SfxKind` -> asset/spatialization here.
pub fn sfx_probe(mut sfx: MessageReader<SfxRequest>) {
    for request in sfx.read() {
        info!("[sfx] {:?}", request.kind);
    }
}
