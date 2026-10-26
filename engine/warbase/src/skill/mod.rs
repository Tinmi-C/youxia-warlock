//! Skill framework (BX3-lite, ADR-0011 essence): per-entity SkillCaster,
//! three-way kinds (Passive/Active/Autocast), cooldowns, cast validation,
//! and SkillCast events for game-side effect dispatch (by id, no trait
//! objects, no scripting). Numbers ship per map; base ships mechanics.
//! Also hosts hero xp distribution (killer-fed XpHolder leveling).

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};
use bevy::ecs::world::Mut;

use crate::buff::{ModEntry, StatKind, StatMods};
use crate::combat::UnitDied;
use crate::unit::{Mana, ManaRegen, MaxMana, XpHolder};

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
    /// UI badge char (e.g. 'Z' for the hotkey label). Game-agnostic.
    pub badge: Option<char>,
    /// Global hotkey if the skill has one (commander Z/X/C style).
    pub hotkey: Option<KeyCode>,
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

pub fn tick_skill_cooldowns(
    time: Res<Time>,
    mut q: Query<&mut SkillCaster>,
    mut mana_q: Query<(&mut Mana, &MaxMana, &ManaRegen)>,
) {
    let dt = time.delta_secs();
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

/// Act: the cast pipeline (SPL1/SPL2, ADR-0016 §6). Ordered phases:
/// 1. consume `InterruptCast` — cancels in-flight `Casting`/`Channeling`
///    with no SkillCast, no mana refund, and the slot cooldown restarted
///    from this frame (an interrupt on the completion frame beats it);
/// 2. validate `CastSkill` requests (caster exists -> slot learned -> off
///    cooldown -> not already casting/channeling -> mana >= cost), spend
///    mana at cast start, then either fire instantly (cast_time == 0,
///    legacy path: cooldown + SkillCast right away) or attach `Casting`;
/// 3. advance `Casting` — on completion emit SkillCast (effect systems
///    unchanged), start the cooldown from this moment, attach `Channeling`
///    if the def has one; Point-targeted casts with a known ground point
///    are first checked against the observer's vision (FOG2 gate below —
///    dark points are refused, no SkillCast and no cooldown);
/// 4. advance `Channeling` down to natural expiry (removal, no message)./// Refused casts leave no trace (skip; the base ships no reject channel).
pub fn process_casts(
    time: Res<Time>,
    mut requests: MessageReader<CastSkill>,
    mut interrupts: MessageReader<InterruptCast>,
    mut casts: MessageWriter<SkillCast>,
    defs: Res<SkillDefs>,
    vision: Res<crate::vision::VisionGrid>,
    mut commands: Commands,
    mut casters: Query<(
        Entity,
        &mut SkillCaster,
        Option<&mut Mana>,
        Option<&mut Casting>,
        Option<&mut Channeling>,
    )>,
) {
    let dt = time.delta_secs();
    // Half-frame tolerance for "this frame completes it": f32 accumulation
    // over many dt additions can sit a few ulps under the exact total, and
    // completion/expiry must land on the exact frame count at fixed steps.
    let half_frame = dt * 0.5;

    // 1) Interrupts first (see doc comment).
    for irq in interrupts.read() {
        let Ok((entity, mut caster, _mana, casting, channeling)) = casters.get_mut(irq.caster)
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
        let Ok((entity, mut caster, mut mana, casting, channeling)) = casters.get_mut(req.caster)
        else {
            continue;
        };
        if started_this_frame.contains(&req.caster)
            || casting.is_some()
            || channeling.is_some()
        {
            continue; // already busy: one cast at a time
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
            if let Some(d) = def.channel {
                commands.entity(entity).insert(Channeling { skill: def.id.clone(), remaining: d });
            }
        }
    }

    // 3) Advance in-flight casts; completion = SkillCast + cooldown from
    //    now + optional Channeling.
    for (entity, mut caster, _mana, casting, _channeling) in &mut casters {
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
        commands.entity(entity).remove::<Casting>();
        if let Some(d) = defs.get(&skill).and_then(|d| d.channel) {
            commands.entity(entity).insert(Channeling { skill, remaining: d });
        }
    }

    // 4) Channel windows tick down; natural expiry is silent removal.
    for (entity, _caster, _mana, _casting, channeling) in &mut casters {
        let Some(mut ch) = channeling else { continue };
        ch.remaining = (ch.remaining - dt).max(0.0);
        if ch.remaining <= half_frame {
            commands.entity(entity).remove::<Channeling>();
        }
    }
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
/// (+5% per level above 1) — the hero-tower growth seam.
pub fn distribute_xp(world: &mut World) {
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
