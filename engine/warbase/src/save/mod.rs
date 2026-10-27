//! Save/load (SEAM2d seam, consumed by the Phase-2 HIGHLAND+SAVE increment,
//! ADR-0018 §4 line 1): world-SNAPSHOT saves — a serde/RON image of the
//! live world, deliberately distinct from replay logs (input streams
//! re-executed against a fresh assemble).
//!
//! Shape: `save_game(&World) -> SaveGame` (pure read, no mutation) and
//! `load_game(&mut World, &SaveGame)` (clear the field, rebuild). Load
//! despawns every `UnitInstance` and re-spawns them through
//! `unit::spawn_unit` so def-derived components (armor, attack kit, mana
//! pool, auras, hero stats, skill slots) rebuild exactly as at first spawn;
//! the snapshot then overlays the runtime state (hp, mana, order, cargo,
//! path progress, cooldowns, xp).
//!
//! CORRECTNESS ANCHOR (the acceptance): save at frame N, load into a fresh
//! app, advance — `replay::state_hash` must equal the uninterrupted world's
//! at N+k (k >= 30, sampled). Entity-index fidelity: the clear-field pass
//! despawns current units with `despawn_no_free` (ids stay out of bevy
//! 0.19's allocator — freed ids would sit in a 128-deep local buffer and
//! NOT be reused, drifting every later index), and each saved unit is
//! rebuilt AT its original id via `spawn_empty_at`. A world whose unit set
//! at save time covers every unit spawned since assemble (no deaths in the
//! window — the tested, pinned scenario shape) therefore reloads
//! bit-identical indices AND a bit-identical future allocation sequence,
//! which the (index, hp) hash terms demand. In other cases (deaths before
//! the save, a save loaded onto a differently-shaped assembled world) the
//! unclaimed indices leak by design (table length preserved) and units
//! whose original id is not reserved fall back to fresh ids — references
//! still resolve soundly through the saved index->entity remap table, only
//! the raw index numbers drift (recorded v1 simplification).
//!
//! Pinned v1 snapshot surface (everything else re-derives or is documented
//! out): units (def/level/pos/side/owner/hp/hp-scale/mana/order/waypoints/
//! agent/follower/wave tag/attack cooldown/xp/skill slots/cargo), econ
//! (gold/wood/pop/purchases), wave director PUBLIC state, rule leak count,
//! MatchRng seed, VisionGrid explored memory. NOT captured (documented
//! gaps, none of them reachable from a plain base match without game-side
//! constructs): mid-spawn wave ledger + boss timer (private to the wave
//! module; a save taken mid-spawn restores the phase but the un-spawned
//! remainder of that wave is lost — survivors still gate clear detection),
//! in-flight projectiles (sub-second transients, no Hp), transient combat
//! state (AttackWindup/AttackLock mid-swing, CurrentTarget — re-picked
//! next frame), buffs/StatMods (timed mods are wall-clock anchored,
//! `Time::elapsed_secs`), Casting/Channeling, Stunned/Silenced, dead
//! entities (a Dead unit pending cleanup is not saved), the day/night
//! clock (a cycled Night save resumes in Day), InFog (re-derived),
//! production queues/research (game-driven), mines' remaining capacity,
//! camp state. Games needing any of these extend `SaveGame` — the struct
//! is public and serde, the loaders are two free functions.
//!
//! RNG note: the save carries the SEED, not the live RNG stream state.
//! The base draws no randomness itself (see the replay module doc), so a
//! base-only match reloads with an identical stream; a game that consumed
//! `MatchRng` mid-match needs a stream-state seam before its saves are
//! exact (recorded follow-up).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::combat::{AttackCooldown, AttackKit, CurrentTarget, Healer, TargetingPriority};
use crate::econ::{Population, PopPurchases, Resources};
use crate::harvest::Carrying;
use crate::movement::{AgentOrderState, MoveAgent, Order};
use crate::path::PathFollower;
use crate::rng::MatchRng;
use crate::rule::StockCount;
use crate::skill::{SkillCaster, SkillDefs, SkillKind, SkillSlot};
use crate::unit::{
    AirUnit, Allegiance, Armor, Hp, Mana, ManaRegen, MaxHp, MaxMana, Owner, PlayerId, UnitInstance,
    UnitRegistry, UnitTrait, XpHolder,
};
use crate::vision::VisionGrid;
use crate::wave::{DirectorPhase, Paths, WaveDirector, WaveTag};

/// Version of the snapshot format (bump on SaveGame schema changes;
/// `load_game` refuses other versions loudly).
pub const SAVE_FORMAT_VERSION: u32 = 1;

/// A named save slot (its string is the file/stem identity the snapshot
/// layer resolves; the base pins only the concept — file IO is game-side).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SaveSlot(pub String);

/// Economy snapshot: the four numbers that fully determine econ state.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct SavedEcon {
    pub gold: u32,
    pub wood: u32,
    pub pop_used: u8,
    pub pop_cap: u8,
    /// Completed population purchases (drives the cost curve).
    pub pop_purchases: u32,
}

/// Wave-director snapshot (public fields only — see the module doc for the
/// pinned mid-spawn-ledger gap). `next_wave` = fully cleared waves,
/// `running` = the wave in progress.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SavedWaveState {
    pub next_wave: usize,
    pub running: Option<usize>,
    pub phase: SavedPhase,
    pub phase_timer: f32,
    /// Completed passes over the table (endless growth).
    pub cycles: u32,
}

/// Serde mirror of `wave::DirectorPhase` (the original is not serde and
/// lives in a frozen-scope module).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SavedPhase {
    Idle,
    Intermission,
    Spawning,
    Done,
}

impl From<&DirectorPhase> for SavedPhase {
    fn from(p: &DirectorPhase) -> Self {
        match p {
            DirectorPhase::Idle => SavedPhase::Idle,
            DirectorPhase::Intermission => SavedPhase::Intermission,
            DirectorPhase::Spawning => SavedPhase::Spawning,
            DirectorPhase::Done => SavedPhase::Done,
        }
    }
}

impl From<SavedPhase> for DirectorPhase {
    fn from(p: SavedPhase) -> Self {
        match p {
            SavedPhase::Idle => DirectorPhase::Idle,
            SavedPhase::Intermission => DirectorPhase::Intermission,
            SavedPhase::Spawning => DirectorPhase::Spawning,
            SavedPhase::Done => DirectorPhase::Done,
        }
    }
}

/// Serde mirror of `unit::Allegiance` (not serde in the unit module).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SavedSide {
    Defender,
    Attacker,
}

impl From<Allegiance> for SavedSide {
    fn from(s: Allegiance) -> Self {
        match s {
            Allegiance::Defender => SavedSide::Defender,
            Allegiance::Attacker => SavedSide::Attacker,
        }
    }
}

impl From<SavedSide> for Allegiance {
    fn from(s: SavedSide) -> Self {
        match s {
            SavedSide::Defender => Allegiance::Defender,
            SavedSide::Attacker => Allegiance::Attacker,
        }
    }
}

/// Serde mirror of `movement::Order` — Entity targets ride as entity
/// INDICES (u32), re-resolved through the remap table at load (see
/// `load_game`). Vec3 points ride as [f32; 3].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SavedOrder {
    Move { to: [f32; 3] },
    AttackMove { to: [f32; 3] },
    AttackTarget { target: u32 },
    HoldPosition,
    Stop,
    Patrol { a: [f32; 3], b: [f32; 3], toward_b: bool },
    Pickup { item: u32 },
    Follow { target: u32 },
    Harvest { target: u32 },
    ReturnCargo { target: u32 },
    Build { what: String, at: [f32; 3] },
}

impl SavedOrder {
    /// Snapshot an order, converting Entity targets to indices.
    fn from_order(o: &Order) -> Self {
        match o {
            Order::Move { to } => SavedOrder::Move { to: to.to_array() },
            Order::AttackMove { to } => SavedOrder::AttackMove { to: to.to_array() },
            Order::AttackTarget { target } => SavedOrder::AttackTarget { target: target.index_u32() },
            Order::HoldPosition => SavedOrder::HoldPosition,
            Order::Stop => SavedOrder::Stop,
            Order::Patrol { a, b, toward_b } => SavedOrder::Patrol {
                a: a.to_array(),
                b: b.to_array(),
                toward_b: *toward_b,
            },
            Order::Pickup { item } => SavedOrder::Pickup { item: item.index_u32() },
            Order::Follow { target } => SavedOrder::Follow { target: target.index_u32() },
            Order::Harvest { target } => SavedOrder::Harvest { target: target.index_u32() },
            Order::ReturnCargo { target } => SavedOrder::ReturnCargo { target: target.index_u32() },
            Order::Build { what, at } => SavedOrder::Build { what: what.clone(), at: at.to_array() },
        }
    }

    /// Rebuild the live order, resolving Entity targets through `resolve`
    /// (old index -> live entity). An unresolvable reference drops the
    /// order (its target is gone; movement treats None as idle).
    fn into_order(self, resolve: &impl Fn(u32) -> Option<Entity>) -> Option<Order> {
        let order = match self {
            SavedOrder::Move { to } => Order::Move { to: Vec3::from_array(to) },
            SavedOrder::AttackMove { to } => Order::AttackMove { to: Vec3::from_array(to) },
            SavedOrder::AttackTarget { target } => {
                Order::AttackTarget { target: resolve(target)? }
            }
            SavedOrder::HoldPosition => Order::HoldPosition,
            SavedOrder::Stop => Order::Stop,
            SavedOrder::Patrol { a, b, toward_b } => Order::Patrol {
                a: Vec3::from_array(a),
                b: Vec3::from_array(b),
                toward_b,
            },
            SavedOrder::Pickup { item } => Order::Pickup { item: resolve(item)? },
            SavedOrder::Follow { target } => Order::Follow { target: resolve(target)? },
            SavedOrder::Harvest { target } => Order::Harvest { target: resolve(target)? },
            SavedOrder::ReturnCargo { target } => Order::ReturnCargo { target: resolve(target)? },
            SavedOrder::Build { what, at } => Order::Build { what, at: Vec3::from_array(at) },
        };
        Some(order)
    }
}

/// Free-locomotion agent snapshot (`MoveAgent` minus derived velocity).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SavedAgent {
    pub speed: f32,
    pub arrival_radius: f32,
    pub body_radius: f32,
    pub state: SavedAgentState,
}

/// Serde mirror of `movement::AgentOrderState` (execution sub-state of the
/// current order — Chasing/Engaging continuity matters for attack-move).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SavedAgentState {
    Idle,
    Moving,
    Engaging,
    Chasing,
}

impl From<&AgentOrderState> for SavedAgentState {
    fn from(s: &AgentOrderState) -> Self {
        match s {
            AgentOrderState::Idle => SavedAgentState::Idle,
            AgentOrderState::Moving => SavedAgentState::Moving,
            AgentOrderState::Engaging => SavedAgentState::Engaging,
            AgentOrderState::Chasing => SavedAgentState::Chasing,
        }
    }
}

impl From<SavedAgentState> for AgentOrderState {
    fn from(s: SavedAgentState) -> Self {
        match s {
            SavedAgentState::Idle => AgentOrderState::Idle,
            SavedAgentState::Moving => AgentOrderState::Moving,
            SavedAgentState::Engaging => AgentOrderState::Engaging,
            SavedAgentState::Chasing => AgentOrderState::Chasing,
        }
    }
}

/// Creep-track follower snapshot; `path` is an index into the `Paths`
/// resource (stable across loads — assemble re-spawns paths in bundle
/// order), NOT an entity index.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SavedFollower {
    pub path: usize,
    pub progress: f32,
    pub laps: u32,
    pub finished: bool,
}

/// Hero growth snapshot (`XpHolder`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct SavedXp {
    pub xp: u32,
    pub level: u8,
    pub points: u8,
    pub next_level_xp: u32,
    pub talents_taken: u8,
}

/// One skill-slot snapshot (`SkillSlot`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SavedSkillSlot {
    pub skill: String,
    pub level: u8,
    pub cooldown: f32,
    pub autocast_on: bool,
}

/// One unit's core-component snapshot. `index` is the ORIGINAL entity
/// index — the remap key for cross-unit references and the order guarantee
/// for bit-identical reloads (see the module doc).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SavedUnit {
    pub index: u32,
    pub def: String,
    pub level: u8,
    pub pos: [f32; 3],
    pub side: SavedSide,
    pub owner: Option<u8>,
    /// hp the unit spawned with, relative to its def (MaxHp / def.hp) —
    /// wave/endless scaling rides in here so MaxHp rebuilds exactly.
    pub hp_scale: f32,
    pub hp: f32,
    pub mana: Option<f32>,
    pub carrying: Option<u32>,
    pub order: Option<SavedOrder>,
    pub waypoints: Vec<[f32; 3]>,
    pub agent: Option<SavedAgent>,
    pub follower: Option<SavedFollower>,
    pub wave_tag: Option<usize>,
    pub attack_cooldown: Option<f32>,
    pub xp: Option<SavedXp>,
    pub skills: Option<Vec<SavedSkillSlot>>,
}

/// The world snapshot (serde; RON/JSON both work — serde feature decides).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SaveGame {
    pub version: u32,
    /// Identity of the map this save belongs to. The base does NOT retain
    /// the bundle at runtime, so `save_game` fills an empty string; games
    /// overwrite it with their map id before persisting.
    pub bundle_id: String,
    pub seed: u64,
    /// Completed updates at save time (frame counter source: a present
    /// `RecordingLog`; 0 otherwise — the base itself is frame-counter-free).
    pub frame: u32,
    pub econ: SavedEcon,
    pub wave: SavedWaveState,
    pub units: Vec<SavedUnit>,
    /// Observer-team explored cells (the fog memory layer; visible
    /// recomputes on the next tick from live sources).
    pub vision_explored: Vec<(i32, i32)>,
    /// Leaked attackers so far (Stockpile policy counter).
    pub leaks: u32,
}

/// Snapshot the world (pure read). Units are captured in ascending entity
/// index (D6: iteration order never reaches the file), skipping Dead
/// entities (a unit pending despawn does not survive the frame anyway).
pub fn save_game(world: &World) -> SaveGame {
    let registry = world.resource::<UnitRegistry>();
    let res = world.resource::<Resources>();
    let pop = world.resource::<Population>();
    let purchases = world.resource::<PopPurchases>();
    let director = world.resource::<WaveDirector>();
    let paths = world.resource::<Paths>();
    let rng = world.resource::<MatchRng>();
    let leaks = world.resource::<StockCount>().0;
    let frame = world
        .get_resource::<crate::replay::RecordingLog>()
        .map(|r| r.frame)
        .unwrap_or(0);
    let explored: Vec<(i32, i32)> = world
        .resource::<VisionGrid>()
        .explored
        .iter()
        .copied()
        .collect();

    let mut units: Vec<(u32, SavedUnit)> = Vec::new();
    // `World::query*` wants &mut World (it caches QueryState); save_game is
    // a PURE READ, so it walks entities directly via EntityRef::get.
    for e in world
        .iter_entities()
        .filter(|e| e.contains::<UnitInstance>() && !e.contains::<crate::combat::Dead>())
    {
        // Required-on-every-unit components (spawn_unit's bundle); the
        // Options mirror load-time optional behavior.
        let (tf, inst, side, hp) = (
            e.get::<Transform>().expect("unit has a Transform"),
            e.get::<UnitInstance>().expect("unit instance"),
            e.get::<Allegiance>().expect("unit has a side"),
            e.get::<Hp>().expect("unit has hp"),
        );
        let (max_hp, mana, carrying, agent, follower, tag, cd, xp, skills, owner) = (
            e.get::<MaxHp>(),
            e.get::<Mana>(),
            e.get::<Carrying>(),
            e.get::<MoveAgent>(),
            e.get::<PathFollower>(),
            e.get::<WaveTag>(),
            e.get::<AttackCooldown>(),
            e.get::<XpHolder>(),
            e.get::<SkillCaster>(),
            e.get::<Owner>(),
        );
        // hp_scale from MaxHp (wave/endless scaling) — def.hp 0 guards to 1.
        let hp_scale = registry
            .get(&inst.def)
            .filter(|d| d.attrs.hp > 0.0)
            .zip(max_hp)
            .map(|(d, m)| m.0 / d.attrs.hp)
            .unwrap_or(1.0);
        let saved = SavedUnit {
            index: e.id().index_u32(),
            def: inst.def.clone(),
            level: inst.level,
            pos: tf.translation.to_array(),
            side: (*side).into(),
            owner: owner.map(|o| o.0 .0),
            hp_scale,
            hp: hp.0,
            mana: mana.map(|m| m.0),
            carrying: carrying.map(|c| c.0),
            order: agent.and_then(|a| a.order.as_ref()).map(SavedOrder::from_order),
            waypoints: agent
                .map(|a| a.waypoints.iter().map(|w| w.to_array()).collect())
                .unwrap_or_default(),
            agent: agent.map(|a| SavedAgent {
                speed: a.speed,
                arrival_radius: a.arrival_radius,
                body_radius: a.body_radius,
                state: SavedAgentState::from(
                    world
                        .get::<AgentOrderState>(e.id())
                        .unwrap_or(&AgentOrderState::Idle),
                ),
            }),
            follower: follower.map(|f| SavedFollower {
                path: paths.0.iter().position(|&p| p == f.path).unwrap_or(0),
                progress: f.progress,
                laps: f.laps,
                finished: f.finished,
            }),
            wave_tag: tag.map(|t| t.0),
            attack_cooldown: cd.map(|c| c.0),
            xp: xp.map(|x| SavedXp {
                xp: x.xp,
                level: x.level,
                points: x.points,
                next_level_xp: x.next_level_xp,
                talents_taken: x.talents_taken,
            }),
            skills: skills.map(|c| {
                c.slots
                    .iter()
                    .map(|s| SavedSkillSlot {
                        skill: s.skill.clone(),
                        level: s.level,
                        cooldown: s.cooldown,
                        autocast_on: s.autocast_on,
                    })
                    .collect()
            }),
        };
        units.push((e.id().index_u32(), saved));
    }
    units.sort_by_key(|(idx, _)| *idx);

    SaveGame {
        version: SAVE_FORMAT_VERSION,
        bundle_id: String::new(),
        seed: rng.seed(),
        frame,
        econ: SavedEcon {
            gold: res.gold,
            wood: res.wood,
            pop_used: pop.used,
            pop_cap: pop.cap,
            pop_purchases: purchases.0,
        },
        wave: SavedWaveState {
            next_wave: director.next_wave,
            running: director.running,
            phase: (&director.phase).into(),
            phase_timer: director.phase_timer,
            cycles: director.cycles,
        },
        units: units.into_iter().map(|(_, u)| u).collect(),
        vision_explored: explored,
        leaks,
    }
}

/// Spawn one saved unit, at `at` (a reserved id at its ORIGINAL index —
/// the bit-identical path) or a fresh id (fragmented save fallback).
///
/// The def-derived base assembly MIRRORS `unit::spawn_unit` statement for
/// statement (the unit module is scope-frozen this increment and its
/// `world.spawn` cannot target a chosen id; the deferred-variant precedent
/// already carries a second copy — KEEP IN SYNC with `spawn_unit`). The
/// runtime overlay (hp, level, position) rides directly on the base insert.
fn spawn_saved_unit(
    world: &mut World,
    registry: &UnitRegistry,
    su: &SavedUnit,
    at: Option<Entity>,
) -> Option<Entity> {
    let def = registry.get(&su.def).cloned()?;
    // Skill-slot pre-pass (reads SkillDefs before any &mut borrow, same as
    // spawn_unit).
    let known_skills: Vec<Option<bool>> = {
        let defs = world.resource::<SkillDefs>();
        def.skills
            .iter()
            .map(|id| defs.get(id).map(|d| d.kind == SkillKind::Autocast))
            .collect()
    };
    let entity = match at {
        Some(id) => world.spawn_empty_at(id).ok()?.id(),
        None => world.spawn_empty().id(),
    };
    let mut e = world.entity_mut(entity);
    e.insert((
        UnitInstance { def: def.id.clone(), level: su.level },
        Hp(su.hp),
        MaxHp(def.attrs.hp * su.hp_scale),
        Armor(def.armor_type),
        Allegiance::from(su.side),
        Transform::from_translation(Vec3::from_array(su.pos)),
        crate::buff::StatMods::default(),
    ));
    if def.traits.contains(&UnitTrait::Air) {
        e.insert(AirUnit);
    }
    if def.attrs.move_speed > 0.0 {
        e.insert(crate::path::MoveSpeed(def.attrs.move_speed));
    }
    if let Some(atk) = def.attack {
        e.insert(AttackKit { def: atk, priority: TargetingPriority::First })
            .insert(AttackCooldown(0.0))
            .insert(CurrentTarget::default());
    }
    if !def.auras.is_empty() {
        e.insert(crate::buff::Auras(def.auras.clone()));
    }
    if def.hero {
        e.insert(XpHolder { xp: 0, level: 1, points: 0, next_level_xp: 10, talents_taken: 0 });
    }
    if let Some(h) = def.heal {
        e.insert(Healer { amount: h.amount, interval: h.interval, radius: h.radius, timer: 0.0 });
    }
    if let Some(m) = def.mana {
        e.insert((Mana(m.max), MaxMana(m.max), ManaRegen(m.regen)));
    }
    if !def.skills.is_empty() {
        let slots = def
            .skills
            .iter()
            .zip(known_skills)
            .enumerate()
            .map(|(i, (id, autocast))| {
                let authored = def.skill_levels.get(i).copied().unwrap_or(0);
                let is_autocast = autocast.unwrap_or(false);
                let level = if is_autocast && authored == 0 { 1 } else { authored };
                SkillSlot {
                    skill: id.clone(),
                    level,
                    cooldown: 0.0,
                    autocast_on: is_autocast,
                }
            })
            .collect();
        e.insert(SkillCaster { slots });
    }
    drop(e);
    if let Some(stats) = def.hero_stats {
        world.entity_mut(entity).insert(stats);
        crate::hero::apply_attribute_bonuses(world, entity);
    }
    Some(entity)
}

/// Restore a snapshot into an ASSEMBLED world (call `bundle::assemble`
/// first — load overwrites resources, never creates the map). Despawns
/// every `UnitInstance`, rebuilds them through `unit::spawn_unit` + the
/// saved overlays, and rewrites econ/wave/rule/vision/rng state. Version
/// mismatch panics loudly (saves are refused, never silently migrated —
/// the ADR-0013 loader discipline).
pub fn load_game(world: &mut World, save: &SaveGame) {
    assert_eq!(
        save.version, SAVE_FORMAT_VERSION,
        "save version {} != supported {} — refusing to load",
        save.version, SAVE_FORMAT_VERSION
    );

    // 1) Clear the field: every UnitInstance goes (starting forces from the
    // assemble included — the snapshot is the truth now). despawn_no_free
    // RESERVES each id: it stays out of the allocator (nothing else can
    // steal it mid-load) and the entity-table length never changes — both
    // load-bearing for the bit-identical anchor (see the module doc).
    let mut q = world.query_filtered::<Entity, With<UnitInstance>>();
    let doomed: Vec<Entity> = q.iter(world).collect();
    let mut reserved: std::collections::BTreeMap<u32, Entity> = Default::default();
    for e in doomed {
        if let Some(id) = world.despawn_no_free(e) {
            reserved.insert(e.index_u32(), id);
        }
    }

    // 2) Resource state (all exist post-assemble: overwrite, never insert —
    // inserting NEW resource types would allocate bookkeeping entities and
    // desync the entity fingerprint against the original run; see the
    // replay module's symmetry note).
    {
        let mut res = world.resource_mut::<Resources>();
        res.gold = save.econ.gold;
        res.wood = save.econ.wood;
        let mut pop = world.resource_mut::<Population>();
        pop.used = save.econ.pop_used;
        pop.cap = save.econ.pop_cap;
        world.resource_mut::<PopPurchases>().0 = save.econ.pop_purchases;
    }
    {
        let mut d = world.resource_mut::<WaveDirector>();
        d.next_wave = save.wave.next_wave;
        d.running = save.wave.running;
        d.phase = save.wave.phase.into();
        d.phase_timer = save.wave.phase_timer;
        d.cycles = save.wave.cycles;
    }
    world.resource_mut::<StockCount>().0 = save.leaks;
    world.insert_resource(MatchRng::seeded(save.seed));
    {
        let mut grid = world.resource_mut::<VisionGrid>();
        grid.explored = save.vision_explored.iter().copied().collect();
        // `visible` recomputes from live sources on the next Tick.
    }

    // 3) Re-spawn units in save order (ascending original index). Each unit
    // takes back its ORIGINAL entity index when the clear-field pass
    // reserved it (spawn_empty_at on the reserved id — bevy's documented
    // "skip the allocator" path; bevy 0.19's default allocator would NOT
    // hand freed ids back this early, they sit in a 128-deep local buffer),
    // else falls back to a fresh id (fragmented/foreign save — index drift,
    // references still remapped; see the module doc).
    let registry = world.resource::<UnitRegistry>().clone();
    // Vec clone (Paths itself is not Clone — a `.clone()` on the &Paths would
    // silently clone the reference and pin the world borrow).
    let paths: Vec<Entity> = world.resource::<Paths>().0.clone();
    // (position in save.units, saved unit) + its new entity — position-based
    // pairing survives skipped defs (a zip would misalign after a skip).
    let mut spawned: Vec<(usize, Entity)> = Vec::new();
    for (i, su) in save.units.iter().enumerate() {
        let Some(entity) = spawn_saved_unit(world, &registry, su, reserved.remove(&su.index))
        else {
            continue; // unknown def was already reported by spawn_saved_unit
        };
        spawned.push((i, entity));
    }
    // Reserved ids the save did not claim (units that died before the save)
    // stay allocated-but-never-spawned: leaked by design, matching the
    // original world's entity-table length exactly.

    // 4) Reference resolution: saved unit index -> new entity first; a miss
    // falls back to the loaded world's own index->entity table (assemble
    // re-created paths/mines/items at identical indices by construction).
    let remap: HashMap<u32, Entity> = spawned
        .iter()
        .map(|(i, e)| (save.units[*i].index, *e))
        .collect();
    let statics: HashMap<u32, Entity> = world
        .iter_entities()
        .map(|e| (e.id().index_u32(), e.id()))
        .collect();
    let resolve = |idx: u32| -> Option<Entity> {
        remap.get(&idx).copied().or_else(|| statics.get(&idx).copied())
    };

    // 5) Overlay the runtime state (two passes: spawn-all first so orders
    // may reference units spawned after the referrer).
    for (i, entity) in &spawned {
        let su = &save.units[*i];
        let entity = *entity;
        let mut em = world.entity_mut(entity);
        em.insert(UnitInstance { def: su.def.clone(), level: su.level })
            .insert(Hp(su.hp));
        if let Some(owner) = su.owner {
            em.insert(Owner(PlayerId(owner)));
        }
        if let Some(mana) = su.mana {
            em.insert(Mana(mana));
        }
        if let Some(carrying) = su.carrying {
            em.insert(Carrying(carrying));
        }
        if let Some(cd) = su.attack_cooldown {
            em.insert(AttackCooldown(cd));
        }
        if let Some(tag) = su.wave_tag {
            em.insert(WaveTag(tag));
        }
        if let Some(x) = &su.xp {
            em.insert(XpHolder {
                xp: x.xp,
                level: x.level,
                points: x.points,
                next_level_xp: x.next_level_xp,
                talents_taken: x.talents_taken,
            });
        }
        if let Some(slots) = &su.skills {
            em.insert(SkillCaster {
                slots: slots
                    .iter()
                    .map(|s| SkillSlot {
                        skill: s.skill.clone(),
                        level: s.level,
                        cooldown: s.cooldown,
                        autocast_on: s.autocast_on,
                    })
                    .collect(),
            });
        }
        if let Some(f) = &su.follower {
            // Path index -> entity; an out-of-range index (map mismatch)
            // skips the follower rather than panic mid-load.
            if let Some(&path) = paths.get(f.path) {
                em.insert(PathFollower {
                    path,
                    progress: f.progress,
                    laps: f.laps,
                    finished: f.finished,
                });
            }
        }
    }
    // Second pass for MoveAgent (needs `resolve`, i.e. every unit spawned).
    for (i, entity) in &spawned {
        let su = &save.units[*i];
        let entity = *entity;
        if let Some(agent) = &su.agent {
            let order = su
                .order
                .as_ref()
                .and_then(|o| o.clone().into_order(&resolve));
            let mut em = world.entity_mut(entity);
            em.insert(MoveAgent {
                speed: agent.speed,
                arrival_radius: agent.arrival_radius,
                waypoints: su.waypoints.iter().map(|w| Vec3::from_array(*w)).collect(),
                velocity: Vec3::ZERO, // derived state, recomputed by move_agents
                body_radius: agent.body_radius,
                order,
            })
            .insert(AgentOrderState::from(agent.state));
        } else if su.order.is_some() {
            // An order without a MoveAgent is unexecutable (movement reads
            // the agent only) — kept in the file, dropped on the floor.
            eprintln!(
                "save/load: order on {:?} without a MoveAgent — dropped",
                su.def
            );
        }
    }
}

/// Domain plugin (SEAM2d; save/load are caller-driven free functions — no
/// per-frame systems to register).
pub struct SavePlugin;

impl bevy::prelude::Plugin for SavePlugin {
    fn build(&self, _app: &mut bevy::prelude::App) {}
}

pub mod prelude {
    pub use super::{
        load_game, save_game, SaveGame, SavePlugin, SaveSlot, SavedAgent, SavedAgentState,
        SavedEcon, SavedFollower, SavedOrder, SavedPhase, SavedSide, SavedSkillSlot, SavedUnit,
        SavedWaveState, SavedXp, SAVE_FORMAT_VERSION,
    };
}
