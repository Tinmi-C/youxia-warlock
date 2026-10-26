//! Neutral-camp domain (MAP, ADR-0018 Phase-1, round-2 plan §3): WC3-style
//! creep camps from the map bundle's `camps` section. Everything runs on the
//! base command mechanisms — guards are ordinary free agents whose orders
//! change through `UnitCommand` messages; the movement domain itself is
//! never touched.
//!
//! Three-state guard AI (`camp_ai`, Tick before movement's `tick_orders` so
//! order flips are consumed in the same frame):
//! - Holding: the guard stands its post under `Order::HoldPosition`. A
//!   hostile inside the camp radius flips the WHOLE camp to
//!   `Order::AttackTarget` (chase) — target = nearest intruder to the camp
//!   center, entity-index tie-break (D6).
//! - Fighting: movement's own AttackTarget state machine chases/engages. The
//!   camp re-targets when the order is spent (target died) and pulls the
//!   guard home once it pursues farther than `radius * LEASH_MULT` from the
//!   center (the leash) or the fight ends with no intruder left.
//! - Returning: `Order::Move` back to the guard's post; arrival (order
//!   spent) restores `Order::HoldPosition`. A fresh intruder re-aggros.
//!
//! Wipe & respawn: when every guard of a camp is gone, `respawn_secs`
//! counts down once, then the full guard set respawns at their posts.
//!
//! Drops (`camp_drops`, Cleanup before `combat::cleanup_dead`): every guard
//! death rolls the camp's `(ItemId, chance)` table and scatters winners on
//! the corpse — the exact loot semantics of `item::drop_loot` (seeded
//! MatchRng, [`LOOT_SCATTER_RADIUS`] circle), kept as a separate roll so
//! camp tables and the per-def `ItemsDef.drops` tables compose.
//!
//! Zero drift: maps without camps spawn nothing; both systems early-return
//! on an empty camp query.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::combat::{Dead, UnitDied};
use crate::item::{GroundItem, LOOT_SCATTER_RADIUS};
use crate::movement::{AgentOrderState, MoveAgent, Order, UnitCommand, DEFAULT_BODY_RADIUS};
use crate::rng::MatchRng;
use crate::unit::{spawn_unit, spawn_unit_deferred, Allegiance, Hostility, Hp, UnitRegistry};

/// Pursuit leash = camp radius x this factor (WC3 "chase past the guard
/// zone, then give up"); pinned by the round-2 plan ("leash = radius x 1.5").
pub const LEASH_MULT: f32 = 1.5;

/// Guard walk-to-post brake radius. A mechanism default on the same scale as
/// movement's `DEFAULT_BODY_RADIUS` (two body radii — matches the guard-post
/// spacing below), not a gameplay number.
pub const GUARD_ARRIVAL_RADIUS: f32 = 2.0 * DEFAULT_BODY_RADIUS;

/// Map-bundle camps section.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CampsDef {
    #[serde(default)]
    pub camps: Vec<CampDef>,
}

/// One authored neutral camp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampDef {
    pub id: String,
    /// Camp anchor, ground (x, z).
    pub pos: [f32; 2],
    /// Guard radius: hostiles inside it trigger pursuit; pursuit past
    /// `radius * LEASH_MULT` leashes back to the post.
    #[serde(default)]
    pub radius: f32,
    /// Seconds after a wiped camp before it respawns.
    #[serde(default)]
    pub respawn_secs: f32,
    /// Loot rolls on guard death: `(ItemId, chance)` — the same roll shape
    /// and scatter semantics as `item::ItemsDef.drops` tables.
    #[serde(default)]
    pub drops: Vec<(String, f32)>,
    /// Guard def ids (UnitId list, index = post slot); empty = an inert
    /// anchor with no guards. Unknown ids skip silently on direct
    /// construction (unreachable through `load_bundle` only for validated
    /// bundles — `validate` does not check camp guards; documented seam).
    #[serde(default)]
    pub guards: Vec<String>,
}

/// Per-guard AI mode (the camp's three-state machine; "movement" is always
/// the base order pipeline, this is only the camp's bookkeeping on top).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardMode {
    /// At its post under HoldPosition.
    Holding,
    /// Pursuing an intruder under AttackTarget.
    Fighting,
    /// Walking back to its post under Move (leash / fight ended).
    Returning,
}

/// Marker + camp AI state on a live guard.
#[derive(Component, Debug)]
pub struct CampGuard {
    pub camp: Entity,
    pub mode: GuardMode,
    /// The guard's post: walk-back target and respawn position.
    pub post: Vec3,
}

/// Runtime state of a live camp anchor (spawned by [`apply_camps`]).
#[derive(Component, Debug)]
pub struct Camp {
    pub def: String,
    /// Camp center on the ground plane.
    pub center: Vec3,
    pub radius: f32,
    pub respawn_secs: f32,
    /// Guard def ids, index-aligned with `posts`.
    pub guards: Vec<String>,
    /// Loot rolls on guard death (`(ItemId, chance)`).
    pub drops: Vec<(String, f32)>,
    /// Guard posts, index-aligned with `guards`.
    pub posts: Vec<Vec3>,
    /// Respawn countdown while wiped (`None` = guards alive).
    pub respawn_in: Option<f32>,
}

/// Deterministic guard-post layout around the camp center: slot 0 IS the
/// center, ring k >= 1 adds 6k posts on a hexagon of radius `k * spacing`
/// (`spacing` = 2 body radii, so adjacent posts never overlap) — the same
/// pinned shape as movement's formation slots, no RNG (D6).
fn guard_posts(center: Vec3, count: usize) -> Vec<Vec3> {
    if count == 0 {
        return Vec::new();
    }
    let spacing = 2.0 * DEFAULT_BODY_RADIUS;
    let mut posts = Vec::with_capacity(count);
    posts.push(center);
    let mut ring: usize = 1;
    while posts.len() < count {
        let n = 6 * ring;
        for j in 0..n {
            if posts.len() >= count {
                break;
            }
            let angle = (j as f32 / n as f32) * std::f32::consts::TAU;
            posts.push(Vec3::new(
                center.x + ring as f32 * spacing * angle.cos(),
                0.0,
                center.z + ring as f32 * spacing * angle.sin(),
            ));
        }
        ring += 1;
    }
    posts
}

/// The guard kit every camp guard carries on top of its def components:
/// camp membership + free-agent locomotion parked on HoldPosition. Returned
/// as a tuple bundle so both the World and Commands spawn paths insert the
/// identical component set.
fn guard_kit(camp: Entity, post: Vec3, speed: f32, body_radius: f32) -> (CampGuard, MoveAgent, AgentOrderState) {
    (
        CampGuard { camp, mode: GuardMode::Holding, post },
        MoveAgent {
            speed,
            arrival_radius: GUARD_ARRIVAL_RADIUS,
            waypoints: vec![],
            velocity: Vec3::ZERO,
            body_radius,
            order: Some(Order::HoldPosition),
        },
        AgentOrderState::Idle,
    )
}

/// Spawn one guard at its post (assemble-time, World access; full component
/// set incl. skill slots via `spawn_unit`).
fn spawn_guard(
    world: &mut World,
    registry: &UnitRegistry,
    camp: Entity,
    unit: &str,
    post: Vec3,
) -> Option<Entity> {
    let def = registry.get(unit)?.clone();
    let e = spawn_unit(world, registry, unit, post, Allegiance::Attacker, 1.0)?;
    world.entity_mut(e).insert(guard_kit(camp, post, def.attrs.move_speed, def.attrs.body_radius));
    Some(e)
}

/// Commands-based variant for the respawn path (system context). Same shape
/// as the wave director's spawns, inheriting its documented limitation:
/// skill slots stay exclusive to the World variant (guards are auto-attack
/// brutes; a skills-on-respawn need would revisit via a SkillDefs read).
fn spawn_guard_deferred(
    commands: &mut Commands,
    registry: &UnitRegistry,
    camp: Entity,
    unit: &str,
    post: Vec3,
) -> Option<Entity> {
    let def = registry.get(unit)?.clone();
    let e = spawn_unit_deferred(commands, registry, unit, post, Allegiance::Attacker, 1.0)?;
    commands.entity(e).insert(guard_kit(camp, post, def.attrs.move_speed, def.attrs.body_radius));
    Some(e)
}

/// Bundle dispatch (SEAM0d symmetry): one `Camp` anchor per authored camp,
/// plus its initial guard set at their posts. Empty section = nothing.
pub fn apply_camps(world: &mut World, bundle: &crate::bundle::MapBundle) {
    if bundle.camps.camps.is_empty() {
        return;
    }
    let registry = world.resource::<crate::unit::UnitRegistry>().clone();
    for def in &bundle.camps.camps {
        let center = Vec3::new(def.pos[0], 0.0, def.pos[1]);
        let posts = guard_posts(center, def.guards.len());
        let camp = world
            .spawn(Camp {
                def: def.id.clone(),
                center,
                radius: def.radius,
                respawn_secs: def.respawn_secs,
                guards: def.guards.clone(),
                drops: def.drops.clone(),
                posts: posts.clone(),
                respawn_in: None,
            })
            .id();
        for (i, unit) in def.guards.iter().enumerate() {
            spawn_guard(world, &registry, camp, unit, posts[i]);
        }
    }
}

/// Tick (before movement's `tick_orders`, so the camp's order flips are
/// consumed by the same frame's command pipeline): the camp three-state
/// machine. See the module docs for the pinned semantics. Determinism (D6):
/// guards are visited in entity-index order, the intruder pick is
/// nearest-to-center with an entity-index tie-break, and outgoing
/// UnitCommands are emitted sorted by subject index (query iteration order
/// never reaches the message stream).
#[allow(clippy::type_complexity)]
pub fn camp_ai(
    time: Res<Time>,
    hostility: Res<Hostility>,
    registry: Res<UnitRegistry>,
    mut camps: Query<(Entity, &mut Camp)>,
    mut guards: Query<
        (Entity, &Transform, &MoveAgent, &mut CampGuard, &Allegiance),
        Without<Dead>,
    >,
    enemies: Query<(Entity, &Transform, &Hp, &Allegiance), Without<Dead>>,
    mut orders_out: MessageWriter<UnitCommand>,
    mut commands: Commands,
) {
    if camps.is_empty() {
        return; // zero drift: camp-less maps never enter
    }
    let dt = time.delta_secs();
    // Emitted orders, sorted by subject index before the write (D6).
    let mut emit: Vec<(u32, Entity, Order)> = Vec::new();
    // Deferred respawns, sorted by (camp entity index, slot) (D6).
    let mut spawns: Vec<(u32, usize, Entity, String, Vec3)> = Vec::new();

    for (camp_e, mut camp) in &mut camps {
        // This camp's live guards, entity-index ascending.
        let mut live: Vec<(Entity, Allegiance)> = guards
            .iter()
            .filter(|(_, _, _, g, _)| g.camp == camp_e)
            .map(|(e, _, _, _, side)| (e, *side))
            .collect();
        live.sort_by_key(|&(e, _)| e.index_u32());

        // Wipe & respawn: no live guards -> one countdown -> full respawn.
        if live.is_empty() {
            if camp.guards.is_empty() {
                continue; // inert anchor (no authored guards)
            }
            match camp.respawn_in {
                None => camp.respawn_in = Some(camp.respawn_secs.max(0.0)),
                Some(t) => {
                    let t = t - dt;
                    if t <= dt * 0.5 {
                        // Half-frame tolerance (fixed-step frame counts; the
                        // same convention as combat's damage point).
                        for (i, unit) in camp.guards.iter().enumerate() {
                            spawns.push((camp_e.index_u32(), i, camp_e, unit.clone(), camp.posts[i]));
                        }
                        camp.respawn_in = None;
                    } else {
                        camp.respawn_in = Some(t);
                    }
                }
            }
            continue;
        }
        camp.respawn_in = None; // guards alive: no countdown

        // Intruder scan (shared target for the whole camp): nearest hostile
        // to the camp center inside the radius. Guards of one camp share a
        // side by construction (all spawned Attacker) — the first guard's
        // Allegiance answers the hostility probe.
        let side = live[0].1;
        let mut intruder: Option<(f32, u32, Entity)> = None;
        for (e, etf, ehp, eside) in &enemies {
            if ehp.0 <= 0.0 || !hostility.hostile(side, *eside) {
                continue;
            }
            let d = camp.center.distance(etf.translation);
            if d > camp.radius {
                continue;
            }
            let idx = e.index_u32();
            let better = match intruder {
                None => true,
                Some((bd, bi, _)) => d.total_cmp(&bd).then(idx.cmp(&bi)) == std::cmp::Ordering::Less,
            };
            if better {
                intruder = Some((d, idx, e));
            }
        }
        let intruder = intruder.map(|(_, _, e)| e);

        for (guard, _) in &live {
            let Ok((_, tf, agent, mut cg, _)) = guards.get_mut(*guard) else { continue };
            let order = agent.order.clone();
            let chasing = matches!(order, Some(Order::AttackTarget { .. }));
            let leashed = chasing
                && tf.translation.distance(camp.center) > camp.radius * LEASH_MULT;

            if leashed {
                // Pinned leash rule: drop any pursuit past radius x 1.5 and
                // walk back to the post.
                emit.push((guard.index_u32(), *guard, Order::Move { to: cg.post }));
                cg.mode = GuardMode::Returning;
                continue;
            }
            match cg.mode {
                GuardMode::Holding => {
                    if let Some(target) = intruder {
                        emit.push((guard.index_u32(), *guard, Order::AttackTarget { target }));
                        cg.mode = GuardMode::Fighting;
                    }
                }
                GuardMode::Fighting => {
                    // Order spent = the target died/despawned (tick_orders
                    // clears AttackTarget orders on target loss).
                    if !chasing {
                        if let Some(target) = intruder {
                            emit.push((guard.index_u32(), *guard, Order::AttackTarget { target }));
                        } else {
                            emit.push((guard.index_u32(), *guard, Order::Move { to: cg.post }));
                            cg.mode = GuardMode::Returning;
                        }
                    }
                }
                GuardMode::Returning => {
                    let walking = matches!(order, Some(Order::Move { .. }));
                    if !walking {
                        // Arrival brake spent the Move order: back on post.
                        emit.push((guard.index_u32(), *guard, Order::HoldPosition));
                        cg.mode = GuardMode::Holding;
                    } else if let Some(target) = intruder {
                        // Fresh intruder re-aggros the returning guard.
                        emit.push((guard.index_u32(), *guard, Order::AttackTarget { target }));
                        cg.mode = GuardMode::Fighting;
                    }
                }
            }
        }
    }

    emit.sort_by(|a, b| a.0.cmp(&b.0));
    for (_, subject, order) in emit {
        orders_out.write(UnitCommand { order, subjects: vec![subject] });
    }
    spawns.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    for (_, _, camp, unit, post) in spawns {
        spawn_guard_deferred(&mut commands, &registry, camp, &unit, post);
    }
}

/// Cleanup (before `combat::cleanup_dead` so the corpse Transform is still
/// readable, after `item::drop_loot` so the two RNG streams compose in a
/// fixed order): every dying camp guard rolls ITS camp's `(ItemId, chance)`
/// table — same loot semantics as the item domain's `drop_loot` (seeded
/// MatchRng; winners scatter on the [`LOOT_SCATTER_RADIUS`] circle around
/// the corpse; grace 0 = walk-over pickable).
pub fn camp_drops(
    mut deaths: MessageReader<UnitDied>,
    camps: Query<(Entity, &Camp)>,
    guards: Query<&CampGuard>,
    transforms: Query<&Transform>,
    mut rng: ResMut<MatchRng>,
    mut commands: Commands,
) {
    if camps.is_empty() {
        let _ = deaths.read().count(); // keep the cursor fresh anyway
        return;
    }
    for death in deaths.read() {
        let Ok(guard) = guards.get(death.entity) else { continue };
        let Ok((_, camp)) = camps.get(guard.camp) else { continue };
        if camp.drops.is_empty() {
            continue;
        }
        let Some(pos) = transforms.get(death.entity).ok().map(|t| t.translation) else {
            continue;
        };
        for (item, chance) in &camp.drops {
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

/// Domain plugin (registered once by WarbasePlugin; MAP increments add
/// systems HERE, never in lib.rs).
pub struct CampPlugin;

impl Plugin for CampPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            camp_ai
                .before(crate::movement::tick_orders)
                .in_set(crate::WarbaseSet::Tick),
        )
        .add_systems(
            Update,
            camp_drops
                .after(crate::item::drop_loot)
                .before(crate::combat::cleanup_dead)
                .in_set(crate::WarbaseSet::Cleanup),
        );
    }
}

pub mod prelude {
    pub use super::{
        apply_camps, Camp, CampDef, CampGuard, CampPlugin, CampsDef, GuardMode, LEASH_MULT,
    };
}
