//! Components = pure data (nouns). A new mechanism = a new component + a new
//! system; existing systems stay untouched (capability-card rule).

use bevy::prelude::*;

/// Attack channel. The MVP only resolves ONE axis: physical armor (see
/// `docs/requirements.md` §6). "Mixed" deals physical *and* magic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackType {
    Physical,
    Magic,
    Mixed,
}

/// The "new mechanism" a fused tower gains (requirements §7.3). Fusion is the
/// only way to obtain these; the shop never sells them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FusionKind {
    /// 神射手: +40% range, prioritises the highest-HP target.
    Marksman,
    /// 大法师: AOE blast.
    Archmage,
    /// 魔弓手: mixed (physical+magic) damage, bypasses armor.
    Hybrid,
    /// 壁垒炮: AOE blast + 30% slow.
    Bastion,
}

/// A tower's archetype. Base(`i`) is one of the 4 purchasable towers (index into
/// `TowerDefs`); `Fused(k)` is a fusion result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TowerKind {
    Base(usize),
    Fused(FusionKind),
}

/// A live enemy instance walking the path. Values come from `EnemyDefs`
/// (resources.rs) at spawn; this is the mutable runtime copy.
#[derive(Component)]
pub struct Enemy {
    pub hp: f32,
    pub max_hp: f32,
    pub speed: f32,
    pub leak: u32,          // base HP lost when this enemy reaches the base
    pub kill_gold: u32,     // gold earned when this enemy dies
    pub physical_armor: bool, // shield enemy: physical damage -50%
    pub def_index: usize,   // archetype index into EnemyDefs (AC3 drop rules)
    pub next_wp: usize,     // index of the next waypoint to head toward
}

/// A placed tower. Values come from `TowerDefs` (base) or a fusion recipe at
/// placement / fusion.
#[derive(Component)]
pub struct Tower {
    pub tower_index: usize, // base index 0..3 (only meaningful for Base)
    pub damage: f32,
    pub attack_speed: f32,
    pub range: f32,
    pub attack_type: AttackType,
    pub cooldown: f32, // seconds until it can fire again
    pub target: Option<Entity>,
    pub kind: TowerKind,
    /// >0 => on fire, hit every enemy within this radius of the target (AOE).
    pub aoe_radius: f32,
    /// 0 => no slow; else 0 < factor <= 1 speed multiplier for `slow_duration` s.
    pub slow_factor: f32,
    pub slow_duration: f32,
    /// Total gold invested in this tower (TO7, requirements §16.5): the build
    /// cost for a base tower; ingredient costs + fusion fee for a fused one.
    /// Selling refunds `invested × 70%` floored.
    pub invested: u32,
}

/// ART2: the ballista weapon mounted on an archer tower. An independent
/// child entity of the tower — later cards rotate it toward the current
/// target and animate the shot procedurally (the model ships no animation;
/// its string is separate geometry in a 767-piece assembly).
#[derive(Component)]
pub struct BallistaMount;

/// ART7: a generic smooth turn-to-track driver for any mounted 3D part
/// (first user: the ballista; future weapon swaps reuse it as-is). Angular
/// fields are in DEGREES (card language); the system converts to radians.
/// The part's yaw-only facing is the entity's `Transform` rotation — the
/// component holds only config plus runtime target state, so the transform
/// stays the single source of truth for orientation.
#[derive(Component, Debug)]
pub struct AimTracker {
    /// Max angular speed in degrees per second (card starting value 240).
    pub turn_rate: f32,
    /// Rest bearing (degrees) held after `return_delay` seconds without a
    /// target. 0 = the spawn orientation.
    pub rest_bearing: f32,
    /// Seconds without a target before easing back to `rest_bearing`.
    pub return_delay: f32,
    /// Model-space unit axis pointing out of the MUZZLE (which long-axis
    /// sign is the front — decided once per model, pinned by test; ballista:
    /// `tower::BALLISTA_FORWARD_AXIS`).
    pub forward_axis: Vec3,
    /// Muzzle distance along `forward_axis` in world units (from `TowerDef`).
    pub muzzle_offset: f32,
    /// Current look target (mirrored from the owning tower by the domain
    /// bridge `tower_aim_bridge`; None = idle).
    pub target: Option<Entity>,
    /// Seconds since a target was last present (runtime; 0 while tracking).
    pub since_target: f32,
}

/// ART8: a generic fire-recoil driver for a mounted weapon part (same family
/// as `AimTracker`; first user: the ballista — future weapon swaps reuse it
/// directly). On the owning tower's fire frame (`tower::TowerFired`) the part
/// is kicked `depth` units along its own backward axis and eases back to rest
/// linearly over `recover_secs`. Re-triggering mid-recovery RESETS the cycle
/// (displacements never stack). The kick rides only the part's translation —
/// rotation stays owned by `AimTracker` — so recoil never disturbs aim
/// convergence.
#[derive(Component, Debug)]
pub struct FireRecoil {
    /// Peak displacement along the part's backward axis (world units).
    pub depth: f32,
    /// Seconds from full kick back to rest (linear recovery; starting 0.18).
    pub recover_secs: f32,
    /// Local translation at rest (captured at attach; recovery returns here).
    pub rest_translation: Vec3,
    /// Seconds since the trigger; None = idle at rest (runtime state).
    pub elapsed: Option<f32>,
}


/// A temporary slow effect on an enemy (e.g. from the shield / bastion tower).
#[derive(Component)]
pub struct Slow {
    pub timer: f32,
    pub factor: f32,
}

/// Who launched a projectile and where it heads. Towers fire HOMING rounds
/// locked on an enemy (§5.1); skill shots (SK2) fly to a FIXED ground point
/// — the origin seam the skill system reuses (§17.2 复用投射物：发起方=技能).
#[derive(Debug, Clone, Copy)]
pub enum ProjectileOrigin {
    /// Track the target's current position every frame (never misses; PR1).
    Homing { target: Entity },
    /// Fly straight to this world point and resolve there (skill shots).
    Fixed { point: Vec3 },
    /// SK12 贯穿箭: fly along `dir` for at most `remaining` world units,
    /// piercing (damaging once each) every enemy within the AOE width of
    /// the flight path; expires when the length runs out.
    Linear { dir: Vec3, remaining: f32 },
}

/// A rule-layer projectile (PR1, requirements §5.1): a homing round locked on
/// its target — flies toward the target's *current* position every frame, so
/// it never misses. Damage and attack type are snapshotted at launch; armor
/// resolves at impact. Built as a generic entity (origin-agnostic: towers
/// home, skills fly to a fixed point — `ProjectileOrigin`) so the skill
/// system reuses it directly.
#[derive(Component)]
pub struct Projectile {
    pub origin: ProjectileOrigin,
    /// Damage snapshot taken at launch (post-boost, pre-armor).
    pub damage: f32,
    pub attack_type: AttackType,
    /// Who fired this round — forwarded into the DamageRequest at impact so
    /// the unified pipeline can attribute every hit (SK7).
    pub source: crate::systems::damage::DamageSource,
    /// The shot rolled a critical (SK9 鹰眼): the damage snapshot already
    /// includes the multiplier; the flag rides along for FX and the BAL
    /// readout.
    pub crit: bool,
    /// 0 => single-target (hits only the target); >0 => AOE: everything within
    /// this radius of the impact point takes damage (armor per-enemy).
    pub aoe_radius: f32,
    /// On-hit slow payload (0 => none): applied to every victim on impact
    /// (TO6 rework: slow rides the hit, §5 original wording).
    pub slow_factor: f32,
    pub slow_duration: f32,
    /// Speed in world units per second (§5.1 starting value, §14 tuning row).
    pub speed: f32,
    /// Last known target position — the AOE detonation anchor when a homing
    /// target dies mid-flight (PR2); for fixed-point shots, the point itself.
    pub last_target_pos: Vec3,
    /// SK12 贯穿箭: enemies already hit by this piercing round (each victim
    /// takes damage exactly once while the bolt flies through). Empty for
    /// every other origin mode.
    pub pierced: Vec<Entity>,
}

/// Healer aura (EN5): periodically restores hp of nearby damaged allies.
/// Attached at spawn to enemies whose `EnemyDef::healer` is true.
#[derive(Component)]
pub struct Healer {
    pub radius: f32,
    pub period: f32, // seconds between heal ticks
    pub heal_per_tick: f32,
    pub tick_timer: f32,
}

/// One of the 8 buildable cells beside the path.
#[derive(Component)]
pub struct TowerSlot {
    pub index: usize,
    pub occupied: bool,
}

/// Marks the base entity (the path endpoint; HP lives in the `BaseHp` resource).
#[derive(Component)]
pub struct BaseMarker;

/// ART3 marker: the arena ground top face. Purely visual (the cliff skirt is
/// unmarked); gameplay never reads the terrain — pins only assert existence
/// and which material branch (textured vs flat fallback) was taken.
#[derive(Component)]
pub struct MapGround;

/// ART5 marker: one wasteland scatter prop instance (tree/boulder/stump/
/// branches). Purely visual dressing; gameplay never reads them.
#[derive(Component)]
pub struct MapProp;

/// ART4 marker: a masonry structure root (base gatehouse / entry arch).
/// Purely visual; gameplay never reads them.
#[derive(Component)]
pub struct MapStructure;

/// ART12: deferred + SELF-HEALING rig wiring on the ranger defender (same
/// scene-rebuild hazard as `EnemyAnim` — see its docs). `wire_ranger` keeps
/// this marker forever, re-attaching graph/combat/bow whenever bevy
/// re-instantiates the scene.
#[derive(Component)]
pub struct RangerRig {
    pub gltf: Handle<bevy::gltf::Gltf>,
    pub wired: Option<RangerWired>,
}

/// What `wire_ranger` attached, for health checks and re-wires.
pub struct RangerWired {
    pub player: Entity,
    pub graph: Handle<AnimationGraph>,
    pub idle: AnimationNodeIndex,
    pub attack: AnimationNodeIndex,
    pub attack_secs: f32,
}

/// ART12 marker on the ranger scene wrapper (the entity whose yaw is turned
/// to face the current target; uniform scale rides the same Transform).
#[derive(Component)]
pub struct RangerBody;

/// ART12: idle/attack state on the ranger's animation player entity.
/// `TowerFired` switches to the one-shot attack clip; when `recover`
/// counts out, the idle loop resumes. `tower` links back for message lookup.
#[derive(Component)]
pub struct RangerCombat {
    pub tower: Entity,
    pub idle: AnimationNodeIndex,
    pub attack: AnimationNodeIndex,
    pub attack_secs: f32,
    pub recover: f32,
}

/// ART11: deferred walk-animation wiring on an enemy model scene. The Gltf
/// asset loads async; `start_enemy_walk` consumes this marker once the named
/// animations are available, attaches an AnimationPlayer on the same entity,
/// and plays the archetype's locomotion clip (Walking for most, Running for
/// fast enemies) on loop.
/// ART11: deferred + SELF-HEALING walk-animation wiring. Bevy 0.19's
/// WorldAsset spawner re-instantiates EVERY instance of a scene asset on an
/// `AssetEvent::Modified` (hot-reload design) — e.g. when a second tower
/// re-loads the same GLB. The rebuilt scene carries a FRESH loader
/// AnimationPlayer nobody wires, so a consume-once marker goes permanently
/// T-pose. The marker therefore STAYS on the wrapper and re-wires whenever
/// its recorded player dies.
#[derive(Component)]
pub struct EnemyAnim {
    pub gltf: Handle<bevy::gltf::Gltf>,
    pub run: bool,
    pub wired: Option<WiredAnim>,
}

/// What `start_enemy_walk` attached, for health checks and re-wires.
#[derive(Clone)]
pub struct WiredAnim {
    pub player: Entity,
    pub graph: Handle<AnimationGraph>,
    pub node: AnimationNodeIndex,
}

// --- Skill framework (phase 2, ADR-0011; card SK5; base types since WX2) -----------

/// Per-entity skill ownership lives in the base (ADR-0012). Re-exported
/// here so `warden::components::SkillCaster` keeps resolving for systems
/// and tests; the slot's `skill` is the base string id (WX2: the registry
/// index model retired — numbers still come from `SkillBalance` by id).
pub use warbase::prelude::SkillCaster;
/// One owned skill instance: `skill` = the base string def id, `level`
/// 1..=def.max_level, cooldown under the SK1 law, `autocast_on` toggle.
pub use warbase::prelude::SkillSlot;

/// The persistent commander singleton (SK8): owns the three global skills
/// (meteor rain / frost nova / war frenzy) via its `SkillCaster`. Spawned
/// once at Startup; its cooldowns live for the whole app run, like the
/// phase-1 global SkillState did.
#[derive(Component, Debug)]
pub struct Commander;

// --- Stat modification pipeline (phase 2, ADR-0011; card SK6) ----------------------

/// Who contributes a stat modifier — the removable identity auras and timed
/// buffs add/remove by. Entity+skill pairs cover self passives, applied
/// buffs and auras from other entities; `Other` is a named escape hatch.
/// The skill key is the base string def id (WX2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModSource {
    /// A skill owned by or applied to the entity itself (passive, timed buff).
    Skill { caster: Entity, def: &'static str },
    /// A skill aura projected by another entity.
    Aura { caster: Entity, def: &'static str },
    /// Anything else that needs a removable identity (e.g. a meta upgrade).
    Other(&'static str),
}

/// One stat modifier (SK6). Add-then-multiply: the aggregator computes
/// `(base + Σ add) × Π mult` per stat. `remaining` = Some for timed buffs
/// (ticked down, entry dropped at zero), None for permanent entries
/// (passives; auras, which the aura system removes on leave instead).
#[derive(Debug, Clone)]
pub struct StatMod {
    pub source: ModSource,
    pub kind: crate::resources::StatKind,
    pub add: f32,
    pub mult: f32,
    pub remaining: Option<f32>,
}

/// All stat modifiers on an entity. Managed through
/// `systems::stats::set_stat_mod` / `clear_stat_mod` so one source's write
/// refreshes rather than stacks.
#[derive(Component, Debug, Default)]
pub struct StatMods(pub Vec<StatMod>);

/// The aggregated stats an entity fights with (SK6): base Tower values
/// folded with its `StatMods` and the global Boosts/Frenzy multipliers.
/// Written every frame by `systems::stats::aggregate_stats` BEFORE any
/// consumer; `tower_fire` reads only this (the WC3 green-number layer).
#[derive(Component, Debug, Clone, Copy)]
pub struct EffectiveStats {
    pub damage: f32,
    pub attack_speed: f32,
    pub range: f32,
    /// 0..=1 chance that an attack rolls a critical hit (SK9 rolls it).
    pub crit_chance: f32,
    /// Multiplier applied on a crit (base 2.0).
    pub crit_multiplier: f32,
}
