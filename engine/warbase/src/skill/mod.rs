//! Skill framework (BX3-lite, ADR-0011 essence): per-entity SkillCaster,
//! three-way kinds (Passive/Active/Autocast), cooldowns, cast validation,
//! and SkillCast events for game-side effect dispatch (by id, no trait
//! objects, no scripting). Numbers ship per map; base ships mechanics.
//! Also hosts hero xp distribution (killer-fed XpHolder leveling).

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};
use bevy::ecs::world::Mut;

use crate::buff::{ModEntry, StatKind, StatMods};
use crate::combat::{Dead, UnitDied};
use crate::unit::{
    spawn_unit_deferred, Allegiance, Hp, Hostility, Mana, ManaRegen, MaxMana, UnitRegistry, XpHolder,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkillKind {
    Passive,
    Active,
    Autocast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkillTargeting {
    None,
    Point,
    Unit,
    /// Ground-area cast (SKVOC): the aiming layer resolves a center point;
    /// effect systems resolve victims with [`area_targets`] (radius from
    /// `SkillDef::area_radius`, enemies of the caster per `Hostility`).
    /// The base itself only validates/passes through — no behavior change
    /// for existing None/Point/Unit casts.
    Area,
}

#[derive(Debug, Clone)]
pub struct SkillDef {
    pub id: String,
    pub kind: SkillKind,
    pub targeting: SkillTargeting,
    pub max_level: u8,
    pub cooldown: f32,
    /// Mana spent at cast start (SPL1). Default semantics = 0 (free cast);
    /// games authoring SkillDefs in Rust write `mana_cost: 0.0` for legacy
    /// behavior (D8: zero drift for warden's cooldown-driven free casts).
    pub mana_cost: f32,
    /// Seconds between cast start and the SkillCast effect (SPL2). 0 =
    /// instant (legacy default). While a timed cast is in flight the caster
    /// carries a `Casting` component.
    pub cast_time: f32,
    /// Channel window in seconds after the SkillCast fires (SPL2). None =
    /// no channel; Some(d) keeps a `Channeling` component on the caster for
    /// d seconds (read freely by game effect systems).
    pub channel: Option<f32>,
    /// Area radius in world units for `SkillTargeting::Area` casts (SKVOC).
    /// 0 = not an AOE skill (default; every existing def). Effect systems
    /// feed this to [`area_targets`]; the value never affects other
    /// targeting kinds.
    pub area_radius: f32,
    /// Summons spawned at the SkillCast moment (SKVOC): None (default) =
    /// no summons — zero drift for every existing skill. Some(spec) spawns
    /// `count` copies of unit `unit` at the caster (same Allegiance) and
    /// attaches `Summoned` for the spec's lifetime.
    pub summon: Option<SummonSpec>,
    /// UI badge char (e.g. 'Z' for the hotkey label). Game-agnostic.
    pub badge: Option<char>,
    /// Global hotkey if the skill has one (commander Z/X/C style).
    pub hotkey: Option<KeyCode>,
}

/// Summon vocabulary (SKVOC, plan §3): what a summoning skill brings in.
/// Pure data; numbers ship per map. Serde defaults keep future RON authoring
/// forgiving (`count`/`lifetime_secs` default off is meaningless, so games
/// author all three fields explicitly).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SummonSpec {
    /// Registry id of the unit def to spawn.
    pub unit: String,
    /// How many copies to spawn per cast.
    pub count: u8,
    /// Seconds each copy lives before the Cleanup phase despawns it.
    pub lifetime_secs: f32,
}

#[derive(Resource, Debug, Default)]
pub struct SkillDefs {
    pub defs: HashMap<String, SkillDef>,
}

impl SkillDefs {
    pub fn get(&self, id: &str) -> Option<&SkillDef> {
        self.defs.get(id)
    }

    /// Total registered skills.
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

#[derive(Component, Debug)]
pub struct SkillCaster {
    pub slots: Vec<SkillSlot>,
}

#[derive(Debug, Clone)]
pub struct SkillSlot {
    pub skill: String,
    pub level: u8,
    pub cooldown: f32,
    pub autocast_on: bool,
}

/// Request (UI / autocast / game aiming layers) -> validated here.
/// `target` (hole-6 upcast, SEAM2b) is the aimed unit for
/// `SkillTargeting::Unit` casts, filled by the game's aiming layer; the base
/// validates and stores it opaquely (timed casts park it on `Casting`) and
/// passes it through to the fired `SkillCast`. `None` = no unit target
/// (None/Point targeting, autocast, legacy callers) — exact pre-SEAM2b
/// behavior.
#[derive(Message, Debug, Clone, Copy)]
pub struct CastSkill {
    pub caster: Entity,
    pub slot: usize,
    pub target: Option<Entity>,
}

/// Interrupt an in-progress cast (stun/silence archetype effects; SPL2's
/// `Casting` component consumes this). Interrupted casts refund no mana and
/// start cooldown from the interrupt tick — pinned by the alignment plan.
#[derive(Message, Debug, Clone, Copy)]
pub struct InterruptCast {
    /// Read by SPL2's casting consumer (silence-all vs single-target).
    pub caster: Entity,
}

/// In-progress timed cast (SPL2). The request frame counts as the first
/// casting frame (`elapsed` starts at dt), so a T-second cast_time fires
/// its SkillCast exactly round(T/dt) frames after the CastSkill request at
/// a fixed timestep. Completion emits SkillCast, starts the slot cooldown
/// from that moment, and attaches `Channeling` if the def has one; an
/// InterruptCast removes this component with no SkillCast and no refund.
#[derive(Component, Debug)]
pub struct Casting {
    pub skill: String,
    pub elapsed: f32,
    pub total: f32,
    /// Aimed unit for Unit-target casts (hole-6): copied from the
    /// `CastSkill` request at cast start and passed through to the fired
    /// `SkillCast` at completion. The base never reads or validates it —
    /// targeting semantics belong to game aiming layers.
    pub target: Option<Entity>,
    pub point: Option<Vec3>,
}

/// Post-cast channel window (SPL2): present for `channel` seconds after the
/// SkillCast fired; game effect systems read it freely. InterruptCast
/// cancels it (cooldown restarts from the interrupt frame); natural expiry
/// just removes the component (no message).
#[derive(Component, Debug)]
pub struct Channeling {
    pub skill: String,
    pub remaining: f32,
}

/// Stunned (SKVOC, plan §3): the unit can neither attack (`fire_attacks`
/// skips it, in-flight attack windups cancel) nor move (`move_agents`
/// freezes it) while this component exists. Applying a stun also fires
/// [`InterruptCast`] at the target (stun breaks casts/channels). Ticked and
/// removed by the skill Tick phase; re-application refreshes `remaining`
/// to the new duration (no stacking).
#[derive(Component, Debug)]
pub struct Stunned {
    pub remaining: f32,
}

/// Silenced (SKVOC, plan §3): the unit cannot START casts (`process_casts`
/// refuses CastSkill while this exists; mana untouched, no cooldown burn).
/// Silence does NOT interrupt in-flight casts (only stun does — minimal
/// pinned semantics). Ticked/removed like `Stunned`.
#[derive(Component, Debug)]
pub struct Silenced {
    pub remaining: f32,
}

/// A summoned unit's lease (SKVOC): attached by the summon spawn path; the
/// skill Tick phase counts it down and the Cleanup phase despawns the unit
/// when it reaches zero.
#[derive(Component, Debug)]
pub struct Summoned {
    pub remaining: f32,
}

/// Which archetype status an [`ApplyStatus`] message applies (SKVOC).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Stun,
    Silence,
}

/// Request a status effect (game skill/effect systems -> skill Tick phase,
/// SKVOC). Stun additionally interrupts the target's in-flight
/// cast/channel (InterruptCast, cooldown from the interrupt frame).
#[derive(Message, Debug, Clone, Copy)]
pub struct ApplyStatus {
    pub target: Entity,
    pub status: StatusKind,
    pub duration: f32,
}

/// Emitted after validation; game-side effect systems dispatch by skill id.
/// `point` is Some only when a game-side aiming layer confirmed a
/// Point-target cast (the base validates and fires immediately; games that
/// defer to a pick write the confirmed ground point here). `target`
/// (hole-6) is Some only when a game-side aiming layer confirmed a
/// Unit-target cast — it rides `CastSkill` -> `Casting` -> here, untouched
/// by base logic.
#[derive(Message, Debug, Clone)]
pub struct SkillCast {
    pub caster: Entity,
    pub skill: String,
    pub level: u8,
    pub target: Option<Entity>,
    pub point: Option<Vec3>,
}

/// Tick (skill domain): slot cooldowns, mana regen (SPL1), and the SKVOC
/// status/summon timers. Ordered phases:
/// 1. consume `ApplyStatus` — insert/refresh `Stunned`/`Silenced` on the
///    target (re-application overwrites `remaining`, no stacking); a stun
///    also writes `InterruptCast` for the target (stun breaks casts and
///    channels; silence does not). Components are deferred-inserted, and
///    the set chain's sync points flush them before Director/Intent/Act
///    later this frame — same-frame freeze semantics.
/// 2. tick statuses down; expiry removes the component (half-frame
///    tolerance, same convention as `Channeling`).
/// 3. tick `Summoned` leases down (clamped at 0; the Cleanup phase
///    despawns expired summons — see `distribute_xp`).
///
/// Seam note: the message params are Option-wrapped because warden wires
/// THIS system standalone (no WarbasePlugin, no message registrations).
/// Without the registrations the status phases no-op — exactly the
/// pre-SKVOC behavior for such apps (zero drift).
pub fn tick_skill_cooldowns(
    time: Res<Time>,
    mut commands: Commands,
    mut requests: Option<MessageReader<ApplyStatus>>,
    mut interrupts: Option<MessageWriter<InterruptCast>>,
    mut q: Query<&mut SkillCaster>,
    mut mana_q: Query<(&mut Mana, &MaxMana, &ManaRegen)>,
    entities: Query<Entity>,
    mut stunned: Query<(Entity, &mut Stunned)>,
    mut silenced: Query<(Entity, &mut Silenced)>,
    mut summoned: Query<&mut Summoned>,
) {
    let dt = time.delta_secs();
    let half_frame = dt * 0.5;

    // 1) Apply statuses (SKVOC). Despawned targets are skipped (a command
    // against a dead entity would error at apply time).
    if let Some(requests) = requests.as_mut() {
        for req in requests.read() {
            if entities.get(req.target).is_err() {
                continue;
            }
            match req.status {
                StatusKind::Stun => {
                    if let Ok((_, mut s)) = stunned.get_mut(req.target) {
                        s.remaining = req.duration; // refresh, no stacking
                    } else {
                        commands
                            .entity(req.target)
                            .insert(Stunned { remaining: req.duration });
                    }
                    // Stun breaks in-flight casts/channels (InterruptCast's
                    // own consumer handles cooldown-from-interrupt).
                    if let Some(interrupts) = interrupts.as_mut() {
                        interrupts.write(InterruptCast { caster: req.target });
                    }
                }
                StatusKind::Silence => {
                    if let Ok((_, mut s)) = silenced.get_mut(req.target) {
                        s.remaining = req.duration;
                    } else {
                        commands
                            .entity(req.target)
                            .insert(Silenced { remaining: req.duration });
                    }
                }
            }
        }
    }

    // 2) Status timers: decrement + natural-expiry removal.
    for (entity, mut s) in &mut stunned {
        s.remaining = (s.remaining - dt).max(0.0);
        if s.remaining <= half_frame {
            commands.entity(entity).remove::<Stunned>();
        }
    }
    for (entity, mut s) in &mut silenced {
        s.remaining = (s.remaining - dt).max(0.0);
        if s.remaining <= half_frame {
            commands.entity(entity).remove::<Silenced>();
        }
    }

    // 3) Summon leases (despawn happens in Cleanup).
    for mut s in &mut summoned {
        s.remaining = (s.remaining - dt).max(0.0);
    }

    for mut caster in &mut q {
        for slot in &mut caster.slots {
            // Clamp at zero: cooldown is "seconds until ready" — never
            // negative (exact-0 assertions and UI reads stay stable).
            slot.cooldown = (slot.cooldown - dt).max(0.0);
        }
    }
    // SPL1 mana regen (Tick slot; the lib.rs system table is SEAM property
    // per D3, so the regen loop folds into the registered Tick system).
    for (mut mana, max, regen) in &mut mana_q {
        mana.0 = (mana.0 + regen.0 * dt).min(max.0);
    }
}

/// Intent: autocast slots that are ready and learned cast themselves.
pub fn run_autocast(casters: Query<(Entity, &SkillCaster)>, mut requests: MessageWriter<CastSkill>) {
    for (e, caster) in &casters {
        for (i, slot) in caster.slots.iter().enumerate() {
            if slot.autocast_on && slot.level > 0 && slot.cooldown <= 0.0 {
                requests.write(CastSkill { caster: e, slot: i, target: None });
            }
        }
    }
}

/// Set every slot holding `skill` to the def's full cooldown (cooldown
/// starts "now": at completion for timed casts, at the interrupt frame for
/// interrupted ones — both pinned by the alignment plan).
fn start_slot_cooldown(caster: &mut SkillCaster, defs: &SkillDefs, skill: &str) {
    let Some(def) = defs.get(skill) else { return };
    for slot in &mut caster.slots {
        if slot.skill == skill {
            slot.cooldown = def.cooldown;
        }
    }
}

/// SKVOC summon primitive: spawn `spec.count` copies of `spec.unit` at the
/// caster (same Allegiance, offset by index along +x so the spawn ring
/// never starts as an exact pile) and lease each with `Summoned`. Unknown
/// unit ids fizzle silently (same convention as a projectile losing its
/// target mid-flight).
fn spawn_summons(
    commands: &mut Commands,
    registry: &UnitRegistry,
    spec: &SummonSpec,
    pos: Vec3,
    side: Allegiance,
) {
    for i in 0..spec.count {
        let at = pos + Vec3::new(i as f32 * 0.5, 0.0, 0.0);
        if let Some(summon) = spawn_unit_deferred(commands, registry, &spec.unit, at, side, 1.0) {
            commands
                .entity(summon)
                .insert(Summoned { remaining: spec.lifetime_secs });
        }
    }
}

/// Act: the cast pipeline (SPL1/SPL2, ADR-0016 §6). Ordered phases:
/// 1. consume `InterruptCast` — cancels in-flight `Casting`/`Channeling`
///    with no SkillCast, no mana refund, and the slot cooldown restarted
///    from this frame (an interrupt on the completion frame beats it);
/// 2. validate `CastSkill` requests (caster exists -> slot learned -> off
///    cooldown -> not already casting/channeling -> not Stunned/Silenced
///    (SKVOC: stun/silence refuse new casts, mana untouched, no cooldown
///    burn) -> mana >= cost), spend mana at cast start, then either fire
///    instantly (cast_time == 0, legacy path: cooldown + SkillCast right
///    away) or attach `Casting`;
/// 3. advance `Casting` — on completion emit SkillCast (effect systems
///    unchanged), start the cooldown from this moment, attach `Channeling`
///    if the def has one; Point-targeted casts with a known ground point
///    are first checked against the observer's vision (FOG2 gate below —
///    dark points are refused, no SkillCast and no cooldown);
/// 4. advance `Channeling` down to natural expiry (removal, no message).
///    Both SkillCast emission points also run the SKVOC summon primitive
///    when the def carries `summon` (None by default — zero drift)./// Refused casts leave no trace (skip; the base ships no reject channel).
pub fn process_casts(
    time: Res<Time>,
    mut requests: MessageReader<CastSkill>,
    mut interrupts: MessageReader<InterruptCast>,
    mut casts: MessageWriter<SkillCast>,
    defs: Res<SkillDefs>,
    registry: Res<UnitRegistry>,
    vision: Res<crate::vision::VisionGrid>,
    mut commands: Commands,
    stunned_q: Query<&Stunned>,
    silenced_q: Query<&Silenced>,
    mut casters: Query<(
        Entity,
        &mut SkillCaster,
        Option<&mut Mana>,
        Option<&mut Casting>,
        Option<&mut Channeling>,
        Option<&Transform>,
        Option<&Allegiance>,
    )>,
) {
    let dt = time.delta_secs();
    // Half-frame tolerance for "this frame completes it": f32 accumulation
    // over many dt additions can sit a few ulps under the exact total, and
    // completion/expiry must land on the exact frame count at fixed steps.
    let half_frame = dt * 0.5;

    // 1) Interrupts first (see doc comment).
    for irq in interrupts.read() {
        let Ok((entity, mut caster, _mana, casting, channeling, _tf, _side)) =
            casters.get_mut(irq.caster)
        else {
            continue;
        };
        let skill = casting
            .as_deref()
            .map(|c| c.skill.clone())
            .or_else(|| channeling.as_deref().map(|c| c.skill.clone()));
        let Some(skill) = skill else { continue };
        start_slot_cooldown(&mut caster, &defs, &skill);
        commands
            .entity(entity)
            .remove::<Casting>()
            .remove::<Channeling>();
    }

    // 2) New cast requests.
    // Deferred inserts are invisible to this frame's later phases, so
    // same-frame duplicate requests are tracked locally (the legacy
    // instant path blocks them via the immediate cooldown write).
    let mut started_this_frame: HashSet<Entity> = HashSet::new();
    for req in requests.read() {
        let Ok((entity, mut caster, mut mana, casting, channeling, tf, side)) =
            casters.get_mut(req.caster)
        else {
            continue;
        };
        if started_this_frame.contains(&req.caster)
            || casting.is_some()
            || channeling.is_some()
        {
            continue; // already busy: one cast at a time
        }
        // SKVOC status gates: a stunned unit cannot start casts (stun
        // breaks concentration); a silenced unit cannot speak them. Both
        // refuse before any side effect — no mana, no cooldown burn.
        if stunned_q.get(req.caster).is_ok() || silenced_q.get(req.caster).is_ok() {
            continue;
        }
        let (skill, level, slot_cooldown) = {
            let Some(slot) = caster.slots.get(req.slot) else { continue };
            (slot.skill.clone(), slot.level, slot.cooldown)
        };
        if level == 0 || slot_cooldown > 0.0 {
            continue;
        }
        let Some(def) = defs.get(&skill) else { continue };
        // SPL1 mana gate: costs > 0 need a pool with enough mana; the spend
        // happens here, at cast start (interrupts refund nothing).
        if def.mana_cost > 0.0 {
            let Some(mana) = mana.as_deref_mut() else { continue };
            if mana.0 < def.mana_cost {
                continue;
            }
            mana.0 -= def.mana_cost;
        }
        started_this_frame.insert(req.caster);
        if def.cast_time > 0.0 {
            commands.entity(entity).insert(Casting {
                skill,
                // The request frame counts as the first casting frame.
                elapsed: dt,
                total: def.cast_time,
                target: req.target,
                point: None,
            });
        } else {
            caster.slots[req.slot].cooldown = def.cooldown;
            casts.write(SkillCast {
                caster: entity,
                skill,
                level,
                target: req.target,
                point: None,
            });
            if let Some(spec) = &def.summon {
                if let (Some(tf), Some(side)) = (tf, side) {
                    spawn_summons(&mut commands, &registry, spec, tf.translation, *side);
                }
            }
            if let Some(d) = def.channel {
                commands.entity(entity).insert(Channeling { skill: def.id.clone(), remaining: d });
            }
        }
    }

    // 3) Advance in-flight casts; completion = SkillCast + cooldown from
    //    now + optional Channeling (+ SKVOC summons, if the def has any).
    for (entity, mut caster, _mana, casting, _channeling, tf, side) in &mut casters {
        let Some(mut c) = casting else { continue };
        c.elapsed += dt;
        if c.elapsed + half_frame < c.total {
            continue;
        }
        let skill = c.skill.clone();
        let point = c.point;
        let target = c.target;
        // FOG2 visibility gate (D5, ADR-0016 §9): a manual Point-targeted
        // cast must land inside the observer team's vision. Ground points
        // only ever reach this pipeline through `Casting.point`, set by
        // game-side aiming layers — the base's own CastSkill path ships no
        // point (see the `point: None` writes below), so this gate is
        // dormant in-tree; the interface is pinned here for those layers.
        // A dark point refuses the cast: no SkillCast, no cooldown burn,
        // Casting dropped.
        if let Some(p) = point {
            let targets_point = defs
                .get(&skill)
                .map(|d| d.targeting == SkillTargeting::Point)
                .unwrap_or(false);
            if targets_point
                && !crate::vision::point_visible(&vision, Vec2::new(p.x, p.z))
            {
                commands.entity(entity).remove::<Casting>();
                continue;
            }
        }
        let level = caster
            .slots
            .iter()
            .find(|s| s.skill == skill)
            .map(|s| s.level)
            .unwrap_or(1);
        start_slot_cooldown(&mut caster, &defs, &skill);
        casts.write(SkillCast { caster: entity, skill: skill.clone(), level, target, point });
        if let Some(spec) = defs.get(&skill).and_then(|d| d.summon.as_ref()) {
            if let (Some(tf), Some(side)) = (tf, side) {
                spawn_summons(&mut commands, &registry, spec, tf.translation, *side);
            }
        }
        commands.entity(entity).remove::<Casting>();
        if let Some(d) = defs.get(&skill).and_then(|d| d.channel) {
            commands.entity(entity).insert(Channeling { skill, remaining: d });
        }
    }

    // 4) Channel windows tick down; natural expiry is silent removal.
    for (entity, _caster, _mana, _casting, channeling, _tf, _side) in &mut casters {
        let Some(mut ch) = channeling else { continue };
        ch.remaining = (ch.remaining - dt).max(0.0);
        if ch.remaining <= half_frame {
            commands.entity(entity).remove::<Channeling>();
        }
    }
}

/// SKVOC area-target pick tool: every living unit hostile to `from` (per
/// the `Hostility` oracle) whose position lies within `radius` of
/// `center` (inclusive, straight 3D distance — the same measure
/// `pick_targets` uses), returned in Entity-index order (deterministic;
/// query iteration order never decides logic, D6). Dead/zero-hp units and
/// friends are excluded. Read-only in spirit; `&mut World` only because a
/// fresh `QueryState` registers itself against the world (no writes).
/// Game aiming/effect layers call it directly; base systems never do (the
/// base ships no AOE damage pipeline; numbers ship per map). Ground-circle
/// rendering stays a game TODO (SKVOC scope note).
pub fn area_targets(world: &mut World, center: Vec3, radius: f32, from: Allegiance) -> Vec<Entity> {
    // Cloned out so the resource borrow ends before the query state
    // registers itself against the world.
    let hostility = world.resource::<Hostility>().clone();
    let mut pick = world.query_filtered::<(Entity, &Transform, &Allegiance, &Hp), Without<Dead>>();
    let mut out: Vec<Entity> = pick
        .iter(world)
        .filter(|&(_e, tf, side, hp)| {
            hp.0 > 0.0 && hostility.hostile(from, *side) && tf.translation.distance(center) <= radius
        })
        .map(|(e, _tf, _side, _hp)| e)
        .collect();
    out.sort_by_key(|e| e.index_u32());
    out
}

/// HR2: spend one banked point on a skill slot. Errors are readable for UI.
pub fn spend_skill_point(world: &mut World, entity: Entity, slot: usize) -> Result<(), String> {
    // Scoped reads first (sequential borrows only).
    let (points, slot_info) = {
        let Ok(holder) = world.get_entity_mut(entity) else {
            return Err("no such entity".into());
        };
        let points = holder.get::<XpHolder>().map(|x| x.points).ok_or("not a hero")?;
        let info = holder
            .get::<SkillCaster>()
            .and_then(|c| c.slots.get(slot).map(|s| (s.skill.clone(), s.level)))
            .ok_or("no such slot")?;
        (points, info)
    };
    if points == 0 {
        return Err("no skill points".into());
    }
    let (skill, level) = slot_info;
    let max_level = world
        .resource::<SkillDefs>()
        .get(&skill)
        .map(|d| d.max_level)
        .unwrap_or(1);
    if level >= max_level {
        return Err("max level".into());
    }
    let Ok(mut holder) = world.get_entity_mut(entity) else {
        return Err("no such entity".into());
    };
    holder.get_mut::<XpHolder>().unwrap().points -= 1;
    holder.get_mut::<SkillCaster>().unwrap().slots[slot].level += 1;
    Ok(())
}

/// Persistent cursor for xp events — a fresh cursor every frame would
/// re-read history and double-count (the double-buffer pitfall).
#[derive(Resource, Default)]
pub struct XpEventCursor(pub bevy::ecs::message::MessageCursor<UnitDied>);

/// Cleanup (exclusive): kill xp feeds the killer's XpHolder; each level
/// grants a point and refreshes a permanent DamageMult growth entry
/// (+5% per level above 1) — the hero-tower growth seam. Also the skill
/// domain's SKVOC Cleanup step: expired summons (`Summoned` leases ticked
/// to zero by the Tick phase) despawn here.
pub fn distribute_xp(world: &mut World) {
    // SKVOC: despawn expired summons (see doc; leases tick in Tick).
    {
        let mut leases = world.query::<(Entity, &Summoned)>();
        let expired: Vec<Entity> = leases
            .iter(world)
            .filter(|&(_e, s)| s.remaining <= 0.0)
            .map(|(e, _s)| e)
            .collect();
        for e in expired {
            let _ = world.despawn(e);
        }
    }
    let events: Vec<UnitDied> = {
        world.resource_scope(|world, mut cursor: Mut<XpEventCursor>| {
            let msgs = world.resource::<bevy::ecs::message::Messages<UnitDied>>();
            cursor.0.read(msgs).cloned().collect()
        })
    };
    for death in events {
        let Some(crate::combat::DamageOrigin::Attack(killer)) = death.killer else {
            continue; // skill/system kills don't feed tower xp in v1
        };
        let Ok(mut holder) = world.get_entity_mut(killer) else { continue };
        let Some(mut xp) = holder.get_mut::<XpHolder>() else { continue };
        xp.xp += death.xp;
        while xp.xp >= xp.next_level_xp {
            xp.xp -= xp.next_level_xp;
            xp.level += 1;
            xp.points += 1;
            xp.next_level_xp = (xp.next_level_xp as f32 * 2.0) as u32;
        }
        let level = xp.level;
        let entry = holder.entry::<StatMods>().or_default();
        entry.into_mut().upsert(ModEntry {
            source: killer,
            kind: StatKind::DamageMult,
            amount: 0.05 * (level - 1) as f32,
            expires_at: None,
            from_aura: false,
        });
    }
}
