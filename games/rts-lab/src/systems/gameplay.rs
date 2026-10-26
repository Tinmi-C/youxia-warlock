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
pub const PEASANT_DEF: &str = "peasant";
pub const MINE_DEF: &str = "gold_mine";
pub const GRUNT_DEF: &str = "grunt";

/// NOTE (hole #2, channel C): MapBundle cannot author starting unit
/// placements — an RTS开局兵力 is map content (ADR-0013) but the bundle has
/// no starting-units section (TD maps spawn via waves / build placement).
/// Until the base grows such a section, the layout lives HERE as game-side
/// data; every number is placeholder (GDD).
const HALL_POS: (f32, f32) = (-5.0, 0.0);
const MINE_POS: (f32, f32) = (5.0, 0.0);
const PEASANT_SPOTS: [(f32, f32); 4] = [(-3.5, 1.5), (-3.5, -1.5), (-2.5, 3.0), (-2.5, -3.0)];
const GRUNT_SPOTS: [(f32, f32); 3] = [(9.5, -6.0), (11.0, -7.5), (8.0, -8.5)];

/// Exclusive startup (after load_map): spawn P0's base + workers and P1's
/// guards. Kit attachment (Selectable/MoveAgent/HarvestRoute) is NOT done
/// here — [`attach_rts_kit`] does it uniformly on the spawn frame for every
/// spawn path (initial layout AND trained graduates). Population for the
/// initial workers is booked here (training books its own).
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
    info!("[rts-lab] starting layout: 1 hall, 1 mine, {workers} peasants, 3 grunts");
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
        ec.insert(Selectable);
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
    }
}

// ---------------------------------------------------------------------------
// Train input (minimal command card: the T key)
// ---------------------------------------------------------------------------

/// T key -> TrainCommand{builder: THE main hall, unit: peasant}. The base
/// production domain validates + pays + queues + spawns (PRD1). Headless
/// (no ButtonInput) is a no-op — tests write TrainCommand directly.
pub fn train_input(world: &mut World) {
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>().cloned() else {
        return;
    };
    if !keys.just_pressed(KeyCode::KeyT) {
        return;
    }
    let mut halls = world.query_filtered::<Entity, (With<MainHall>, With<ProductionQueue>)>();
    let Some(hall) = halls.iter(world).next() else {
        info!("[rts-lab] T: no main hall with a production queue");
        return;
    };
    world.write_message(TrainCommand { builder: hall, unit: PEASANT_DEF.into() });
    info!("[rts-lab] T: train peasant queued at the main hall");
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
