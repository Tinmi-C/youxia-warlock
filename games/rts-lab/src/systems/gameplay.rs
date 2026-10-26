//! Gameplay core: map load + starting layout + the thin RTS layer
//! (unit kit attachment, train input, harvest loop). Thin-game rule:
//! mechanisms are warbase's; this crate wires them to the map file and
//! composes them into the acceptance slice.

use bevy::prelude::*;
use warbase::bundle::{assemble, load_bundle};
use warbase::prelude::*;

use crate::components::{Carrying, GoldMine, HarvestRoute, MainHall};
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
/// see free_agent_auras), potion_heal (the item use-effect, instant).
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
// Starting layout
// ---------------------------------------------------------------------------

/// Def ids the layout spawns (must exist in maps/arena.ron).
pub const HALL_DEF: &str = "main_hall";
pub const BARRACKS_DEF: &str = "barracks";
pub const PEASANT_DEF: &str = "peasant";
pub const FOOTMAN_DEF: &str = "footman";
pub const ARCHER_DEF: &str = "archer";
pub const PALADIN_DEF: &str = "paladin";
pub const SHOP_DEF: &str = "shop";
pub const MINE_DEF: &str = "gold_mine";
pub const GRUNT_DEF: &str = "grunt";

/// NOTE (hole #2, channel C): MapBundle cannot author starting unit
/// placements — an RTS开局兵力 is map content (ADR-0013) but the bundle has
/// no starting-units section (TD maps spawn via waves / build placement).
/// Until the base grows such a section, the layout lives HERE as game-side
/// data; every number is placeholder (GDD).
const HALL_POS: (f32, f32) = (-5.0, 0.0);
const BARRACKS_POS: (f32, f32) = (-5.0, 4.5);
const MINE_POS: (f32, f32) = (5.0, 0.0);
const SHOP_POS: (f32, f32) = (0.0, 0.0);
const PALADIN_POS: (f32, f32) = (-4.5, -2.8); // clear of the RL1 box-select test rect
const PEASANT_SPOTS: [(f32, f32); 4] = [(-3.5, 1.5), (-3.5, -1.5), (-2.5, 3.0), (-2.5, -3.0)];
const GRUNT_SPOTS: [(f32, f32); 3] = [(9.5, -6.0), (11.0, -7.5), (8.0, -8.5)];

/// NOTE (hole #2b, channel C): the bundle has no shop placement section
/// either (which building is a shop and its stock list is map content);
/// game-side `Shop` component + stock lives HERE. Stock: the potion and the
/// passive sword from maps/arena.ron items section.
const SHOP_STOCK: [&str; 2] = ["healing_potion", "blade_of_valor"];

/// Exclusive startup (after load_map): spawn P0's base + workers and P1's
/// guards. Kit attachment (Selectable/MoveAgent/HarvestRoute) is NOT done
/// here — [`attach_rts_kit`] does it uniformly on the spawn frame for every
/// spawn path (initial layout AND trained graduates). Population for the
/// initial workers is booked here (training books its own; the hero and the
/// shop are unbooked placeholders — pop economics is not this slice's aim).
pub fn spawn_starting_units(world: &mut World) {
    let registry = world.resource::<UnitRegistry>().clone();

    let hall = spawn_unit(
        world,
        &registry,
        HALL_DEF,
        Vec3::new(HALL_POS.0, 0.0, HALL_POS.1),
        Allegiance::Defender,
        1.0,
    )
    .expect("main_hall def exists in arena.ron");
    world.entity_mut(hall).insert((MainHall, Owner(PlayerId(0))));

    let barracks = spawn_unit(
        world,
        &registry,
        BARRACKS_DEF,
        Vec3::new(BARRACKS_POS.0, 0.0, BARRACKS_POS.1),
        Allegiance::Defender,
        1.0,
    )
    .expect("barracks def exists in arena.ron");
    world.entity_mut(barracks).insert(Owner(PlayerId(0)));

    let mine = spawn_unit(
        world,
        &registry,
        MINE_DEF,
        Vec3::new(MINE_POS.0, 0.0, MINE_POS.1),
        Allegiance::Defender, // neutral building on the home side; no kit -> non-combatant
        1.0,
    )
    .expect("gold_mine def exists in arena.ron");
    world.entity_mut(mine).insert(GoldMine);

    // Neutral shop at the arena center (stock = game-side, hole #2b above).
    let shop = spawn_unit(
        world,
        &registry,
        SHOP_DEF,
        Vec3::new(SHOP_POS.0, 0.0, SHOP_POS.1),
        Allegiance::Defender,
        1.0,
    )
    .expect("shop def exists in arena.ron");
    world.entity_mut(shop).insert(Shop { stock: SHOP_STOCK.map(String::from).to_vec() });

    // The hero (RL2): XpHolder + SkillCaster + Mana come from the def via
    // spawn_unit. Base spawn leaves Active/Passive slots UNLEARNED (level 0 —
    // level points are the intended path); this acceptance slice pre-learns
    // every slot at rank 1 so Q works from minute zero (hard-coded aim, GDD).
    let paladin = spawn_unit(
        world,
        &registry,
        PALADIN_DEF,
        Vec3::new(PALADIN_POS.0, 0.0, PALADIN_POS.1),
        Allegiance::Defender,
        1.0,
    )
    .expect("paladin def exists in arena.ron");
    world.entity_mut(paladin).insert(Owner(PlayerId(0)));
    if let Some(mut caster) = world.get_mut::<SkillCaster>(paladin) {
        for slot in &mut caster.slots {
            slot.level = 1;
        }
    }

    let mut workers = 0u8;
    for (x, z) in PEASANT_SPOTS {
        let e = spawn_unit(
            world,
            &registry,
            PEASANT_DEF,
            Vec3::new(x, 0.0, z),
            Allegiance::Defender,
            1.0,
        )
        .expect("peasant def exists in arena.ron");
        world.entity_mut(e).insert(Owner(PlayerId(0)));
        workers += 1;
    }
    for (x, z) in GRUNT_SPOTS {
        let e = spawn_unit(
            world,
            &registry,
            GRUNT_DEF,
            Vec3::new(x, 0.0, z),
            Allegiance::Attacker,
            1.0,
        )
        .expect("grunt def exists in arena.ron");
        world.entity_mut(e).insert(Owner(PlayerId(1)));
    }
    world.resource_mut::<Population>().used = workers;
    info!("[rts-lab] starting layout: hall, barracks, shop, mine, paladin, {workers} peasants, 3 grunts");
}

// ---------------------------------------------------------------------------
// RTS kit attachment (every spawn path funnels here)
// ---------------------------------------------------------------------------

/// Attach the free-agent kit to freshly spawned units (Added<UnitInstance>
/// observer — covers both the starting layout and PRD graduates):
/// - `Selectable` on every instance (buildings included, WC3 habit);
/// - `MoveAgent` + `AgentOrderState` when the def moves (peasant 3.0);
///   move_speed 0 defs (hall/mine/grunt) stay put — grunt 站桩守卫 comes free.
/// - `HarvestRoute` on peasants when exactly one hall and one mine exist.
/// - `Inventory` on heroes and peasants (RL2: walk-over pickup — the base
///   `pickup_items` observation seam serves every Inventory carrier; spawn
///   attaches it to NOBODY, so the thin wire lives here).
/// - `VisionSource` (RL3) on the seeing defs — radii are hard-coded GDD
///   placeholders: hall 8.0, paladin 7.0, footman/archer 5.5 (peasants see
///   nothing; the fog grid only lights from these sources).
pub fn vision_radius(def: &str) -> Option<f32> {
    match def {
        HALL_DEF => Some(8.0),
        PALADIN_DEF => Some(7.0),
        FOOTMAN_DEF | ARCHER_DEF => Some(5.5),
        _ => None,
    }
}

pub fn attach_rts_kit(
    added: Query<(Entity, &UnitInstance), Added<UnitInstance>>,
    registry: Res<UnitRegistry>,
    halls: Query<Entity, With<MainHall>>,
    mines: Query<Entity, With<GoldMine>>,
    mut commands: Commands,
) {
    let route_target = match (halls.single().ok(), mines.single().ok()) {
        (Some(hall), Some(mine)) => Some((hall, mine)),
        _ => None, // layout incomplete (or a marker despawned): peasants just idle
    };
    for (entity, instance) in &added {
        let Some(def) = registry.get(&instance.def) else { continue };
        let mut ec = commands.entity(entity);
        // Visual-side hierarchy correctness: base spawn gives units a bare
        // Transform; mesh children (visualize_units) carry InheritedVisibility
        // whose propagation wants the parent to have it too (bevy B0004).
        // Visibility requires the inherited family — insert once, here, for
        // every spawn path. Inert headless (no render plugin reads it).
        ec.insert((Selectable, Visibility::default()));
        if def.attrs.move_speed > 0.0 {
            ec.insert((
                MoveAgent {
                    speed: def.attrs.move_speed,
                    arrival_radius: 0.1,
                    waypoints: Vec::new(),
                    velocity: Vec3::ZERO,
                    body_radius: def.attrs.body_radius,
                    order: None,
                },
                AgentOrderState::Idle,
            ));
        }
        if instance.def == PEASANT_DEF {
            if let Some((hall, mine)) = route_target {
                ec.insert(HarvestRoute { mine, hall });
            }
        }
        if def.hero || instance.def == PEASANT_DEF {
            ec.insert(Inventory::default());
        }
        if let Some(radius) = vision_radius(&instance.def) {
            ec.insert(VisionSource { radius });
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
/// (base ResearchCommands resource queue — PRD2's frozen-lib.rs seam; the
/// consumer validates gold/cap). Headless no-op.
pub fn research_input(world: &mut World) {
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>().cloned() else {
        return;
    };
    if !keys.just_pressed(KeyCode::KeyR) {
        return;
    }
    world
        .resource_mut::<ResearchCommands>()
        .0
        .push_back(ResearchCommand::Start { id: "attack_upgrade".into() });
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
    world.write_message(CastSkill { caster: hero, slot });
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
    world
        .resource_mut::<ItemCommands>()
        .0
        .push(ItemCommand::Use { user: hero, slot: 0 });
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
    world
        .resource_mut::<ShopCommands>()
        .0
        .push(ShopCommand::Buy {
            buyer: hero,
            shop,
            item: "healing_potion".into(),
        });
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

// ---------------------------------------------------------------------------
// Devotion aura for free agents (channel C patch, hole #7)
// ---------------------------------------------------------------------------
/// NOTE (hole #7, channel C): the base aura consumers (`buff::apply_auras` /
/// `apply_follower_auras`) both filter targets to `PathFollower` carriers
/// (TD creeps). A free-agent RTS has no PathFollower, so the paladin's
/// devotion aura (def.auras) would reach nobody. Game-side incubation: this
/// system mirrors apply_follower_auras' per-frame rebuild semantics for
/// ALLIED free agents (Without<PathFollower>), source included (WC3
/// devotion buffs the paladin too). Ordered `.before(aggregate_stats)` so
/// the same frame's EffectiveStats fold sees the entries.
pub fn free_agent_auras(
    hostility: Res<Hostility>,
    sources: Query<(Entity, &Transform, &Allegiance, &Auras), Without<PathFollower>>,
    mut targets: Query<
        (Entity, &Transform, &Allegiance, &mut StatMods),
        (Without<PathFollower>, Without<Dead>),
    >,
) {
    if sources.is_empty() {
        return;
    }
    let mut grants: std::collections::HashMap<Entity, Vec<ModEntry>> =
        std::collections::HashMap::new();
    for (tgt_e, tgt_pos, tgt_side, mut mods) in &mut targets {
        mods.entries.retain(|e| !e.from_aura);
        for (src_e, src_pos, src_side, auras) in &sources {
            if !hostility.allied(*src_side, *tgt_side) {
                continue; // free-agent auras buff allies (devotion semantics)
            }
            let d = src_pos.translation.distance(tgt_pos.translation);
            for aura in &auras.0 {
                if d <= aura.radius {
                    grants.entry(tgt_e).or_default().push(ModEntry {
                        source: src_e,
                        kind: aura.kind,
                        amount: aura.amount,
                        expires_at: None,
                        from_aura: true,
                    });
                }
            }
        }
    }
    for (tgt, list) in grants {
        if let Ok((_, _, _, mut mods)) = targets.get_mut(tgt) {
            for entry in list {
                mods.upsert(entry);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Harvest loop (game-side mechanism, C-channel incubation)
// ---------------------------------------------------------------------------

/// 装载半径：农民距矿/城中心 ≤ 此值即装/卸（GDD 数值）。
pub const HARVEST_RADIUS: f32 = 1.5;
/// 单趟搬运金量。
pub const CARRY_AMOUNT: u32 = 10;
/// 集结环半径（城/矿各自的停驻分槽），都落在 HARVEST_RADIUS 内。
const HALL_SLOT_RING: f32 = 1.35;
const MINE_SLOT_RING: f32 = 1.2;
/// 「目标即此点」的判定容差。
const SAME_GOAL_EPSILON_SQ: f32 = 0.25;

/// 确定性停驻分槽：同一建筑的多个农民按实体 index 散到 8 向环上，
/// 避免全部挤向精确同一点（见下方 harvest_tick 的推挤平衡注记）。
fn slot_around(center: Vec3, worker: Entity, ring: f32) -> Vec3 {
    let step = worker.index_u32() % 8;
    let angle = (step as f32 / 8.0) * std::f32::consts::TAU;
    Vec3::new(center.x + ring * angle.cos(), 0.0, center.z + ring * angle.sin())
}

fn targets_same(a: Vec3, b: Vec3) -> bool {
    a.distance_squared(b) <= SAME_GOAL_EPSILON_SQ
}

/// The RL1 harvest loop (game-side system; the base has no economy-gathering
/// domain yet — this is the incubator). Per peasant with a HarvestRoute:
/// - empty hands + within HARVEST_RADIUS of the mine (and its order, if any,
///   targets the mine — or none) -> Carrying(10) + Move to its hall slot;
/// - Carrying + within HARVEST_RADIUS of the hall (ditto) -> gold += 10
///   (direct write + EconChanged snapshot, the PRD/build econ mode) + Move
///   to its mine slot;
/// - fully idle elsewhere -> Move to the current goal (covers trained
///   spawns beside the hall and interrupted workers).
/// Player orders always win: an order targeting anywhere else (or any
/// non-Move order) is left untouched; the loop resumes once it settles.
///
/// INTEGRATION NOTE (soft-collision equilibrium): with every worker issued
/// to the SAME exact point, MOV3's anti-jitter rule (penetration > 50% is
/// not pushed) settles the pile into a stand-off ~0.4 short of the target —
/// forever short of `arrival_radius`. Two mitigations, both game-side:
/// per-worker deterministic slots (MOV5's scatter only fires for GROUP
/// commands, not for independent single-subject commands) + proximity-based
/// leg transitions (load/deposit at HARVEST_RADIUS, not at exact arrival).
pub fn harvest_tick(world: &mut World) {
    let routes: Vec<(Entity, Entity, Entity)> = {
        let mut q = world.query::<(Entity, &HarvestRoute)>();
        q.iter(world).map(|(e, r)| (e, r.mine, r.hall)).collect()
    };
    for (peasant, mine, hall) in routes {
        // Current order snapshot: harvest legs are Move orders to the mine
        // or hall (exact center or own slot); anything else is the player's.
        let (goal, idle) = match world.get::<MoveAgent>(peasant) {
            Some(agent) => {
                let goal = agent.order.as_ref().and_then(|o| match o {
                    Order::Move { to } => Some(*to),
                    _ => None,
                });
                (goal, agent.order.is_none() && agent.waypoints.is_empty())
            }
            None => continue, // no agent, no loop
        };
        let Some((pos, mine_pos, hall_pos)) = (|| {
            Some((
                world.get::<Transform>(peasant)?.translation,
                world.get::<Transform>(mine)?.translation,
                world.get::<Transform>(hall)?.translation,
            ))
        })() else {
            continue; // mine or hall despawned mid-match
        };
        let mine_slot = slot_around(mine_pos, peasant, MINE_SLOT_RING);
        let hall_slot = slot_around(hall_pos, peasant, HALL_SLOT_RING);
        let goal_is_mine =
            goal.is_none_or(|to| targets_same(to, mine_pos) || targets_same(to, mine_slot));
        let goal_is_hall =
            goal.is_none_or(|to| targets_same(to, hall_pos) || targets_same(to, hall_slot));
        let carrying = world.get::<Carrying>(peasant).is_some();

        if !carrying && pos.distance(mine_pos) <= HARVEST_RADIUS && goal_is_mine {
            // Load at the mine, walk home (to this worker's hall slot).
            world.entity_mut(peasant).insert(Carrying(CARRY_AMOUNT));
            world.write_message(UnitCommand {
                order: Order::Move { to: hall_slot },
                subjects: vec![peasant],
            });
        } else if carrying && pos.distance(hall_pos) <= HARVEST_RADIUS && goal_is_hall {
            // Deposit at the hall (PRD/build econ mode: direct write + snapshot),
            // then walk back (to this worker's mine slot).
            world.entity_mut(peasant).remove::<Carrying>();
            let (gold, wood) = {
                let mut res = world.resource_mut::<Resources>();
                res.gold = res.gold.saturating_add(CARRY_AMOUNT);
                (res.gold, res.wood)
            };
            let (used, cap) = {
                let pop = world.resource::<Population>();
                (pop.used, pop.cap)
            };
            world.write_message(EconChanged { gold, wood, pop_used: used, pop_cap: cap });
            world.write_message(UnitCommand {
                order: Order::Move { to: mine_slot },
                subjects: vec![peasant],
            });
        } else if idle {
            // Idle away from the endpoints: head to the current goal.
            let goal = if carrying { hall_slot } else { mine_slot };
            world.write_message(UnitCommand {
                order: Order::Move { to: goal },
                subjects: vec![peasant],
            });
        }
    }
}
