//! Unit domain model (BX1): defs live in the map bundle, instances are ECS
//! component combinations — no hard-coded tower/creep classes. Composition
//! decides what a unit is: PathFollower => walks, XpHolder => hero tower,
//! AttackKit => fires.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type UnitId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AttackType {
    Normal,
    Piercing,
    Siege,
    Magic,
    Hero,
}

impl AttackType {
    pub const ALL: [AttackType; 5] = [
        AttackType::Normal,
        AttackType::Piercing,
        AttackType::Siege,
        AttackType::Magic,
        AttackType::Hero,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArmorType {
    Light,
    Medium,
    Heavy,
    Fortified,
    Hero,
    Divine,
}

impl ArmorType {
    pub const ALL: [ArmorType; 6] = [
        ArmorType::Light,
        ArmorType::Medium,
        ArmorType::Heavy,
        ArmorType::Fortified,
        ArmorType::Hero,
        ArmorType::Divine,
    ];
}

/// Attack x armor multiplier matrix shipped with each map bundle. The base
/// ships the structure and the lookup only — never the numbers.
#[derive(Debug, Clone, Resource, Serialize, Deserialize)]
pub struct AttackArmorTable {
    /// Row-major `[attack_type][armor_type]`, aligned with the enum orders.
    pub mult: [[f32; 6]; 5],
}

impl Default for AttackArmorTable {
    fn default() -> Self {
        Self { mult: [[1.0; 6]; 5] }
    }
}

impl AttackArmorTable {
    /// Unknown combos fall back to 1.0 (no multiplier) — decided and pinned
    /// by UT-AA-02; bundles ship the full matrix anyway.
    pub fn multiplier(&self, attack: AttackType, armor: ArmorType) -> f32 {
        self.mult[attack as usize][armor as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetDomain {
    Ground,
    Air,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitTrait {
    Fast,
    Healer,
    AuraBearer,
    Air,
    Resistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseAttributes {
    pub hp: f32,
    pub move_speed: f32,
    pub body_radius: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjDef {
    pub speed: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackDef {
    pub damage: f32,
    pub interval: f32,
    pub range: f32,
    pub attack_type: AttackType,
    pub targets: TargetDomain,
    pub projectile: Option<ProjDef>,
    /// Attack windup in seconds (SKVOC, plan §3): time between "fire" and
    /// the damage point. 0 (serde/Rust default — every existing def) =
    /// fire immediately, the exact pre-SKVOC behavior.
    #[serde(default)]
    pub windup_secs: f32,
    /// Backswing lock in seconds (SKVOC): after the damage point the unit
    /// cannot START a new windup for this long, unless a fresh order
    /// cancels the lock (micro-retreat). 0 = no lock (legacy behavior).
    #[serde(default)]
    pub backswing_secs: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitDef {
    pub id: UnitId,
    pub attrs: BaseAttributes,
    #[serde(default)]
    pub attack: Option<AttackDef>,
    pub armor_type: ArmorType,
    /// Build cost (towers); 0 for creeps. Numbers ship in the map bundle.
    #[serde(default)]
    pub cost_gold: u32,
    #[serde(default)]
    pub cost_wood: u32,
    #[serde(default)]
    pub bounty_gold: u32,
    #[serde(default)]
    pub bounty_wood: u32,
    #[serde(default)]
    pub grant_xp: u32,
    #[serde(default)]
    pub traits: Vec<UnitTrait>,
    /// Function-tower auras (BX3); amounts ship in the map bundle.
    #[serde(default)]
    pub auras: Vec<crate::buff::AuraDef>,
    /// Hero units spawn an XpHolder (level/points growth).
    #[serde(default)]
    pub hero: bool,
    /// Skill ids to attach at spawn (looked up in SkillDefs).
    #[serde(default)]
    pub skills: Vec<UnitId>,
    /// Initial level per skill slot at spawn (hole-9 upcast, SEAM2b).
    /// COMPATIBILITY RULE (pinned): zipped against `skills` positionally —
    /// equal length covers slot-for-slot; a shorter vec pads with 0; an
    /// Autocast-kind slot whose resolved level is 0 is raised to 1 (the
    /// pre-existing "autocast starts learned" semantics, now expressible as
    /// "explicit 1" or default). Empty vec (serde default) = all slots 0
    /// except Autocast -> 1: bit-identical to the pre-SEAM2b spawn behavior.
    #[serde(default)]
    pub skill_levels: Vec<u8>,
    /// Next tier def id (TO3 upgrade chain); cost is the target def's own
    /// cost_gold. None = max tier.
    #[serde(default)]
    pub upgrade_to: Option<UnitId>,
    /// Support-unit periodic heal (numbers per map).
    #[serde(default)]
    pub heal: Option<HealDef>,
    /// Lives lost when this unit leaks (Lives policy; BX8). Elite/boss
    /// creeps ship bigger numbers; default 1 keeps plain creeps terse.
    #[serde(default = "default_leak")]
    pub leak: u32,
    /// Population cost when fielding this unit (L3, round-4 LDEBT).
    /// Serde default 1 = the classic "one body, one pop slot"; the number
    /// ships per def in the map bundle (legion's 1-4 merc band). Consumed
    /// by game economies via `UnitRegistry` — the base books population
    /// only through the EconCommand stream, unchanged.
    #[serde(default = "default_pop_cost")]
    pub pop_cost: u8,
    /// Optional mana pool (SPL1, ADR-0016 §6). None = no mana bar — every
    /// legacy tower/creep def stays exactly as before, and old RON bundles
    /// load unchanged (`#[serde(default)]`).
    #[serde(default)]
    pub mana: Option<ManaDef>,
    /// Hero primary attributes (HERO, round-2 plan §3): spawn inserts
    /// `HeroStats` and applies the `HeroStatsDef` conversion table on top
    /// of the base fields above (additive layer — see
    /// `hero::apply_attribute_bonuses`). None (serde default) = plain
    /// unit, zero drift for every existing def/bundle.
    #[serde(default)]
    pub hero_stats: Option<crate::hero::HeroStats>,
}

fn default_leak() -> u32 {
    1
}

fn default_pop_cost() -> u8 {
    1
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct HealDef {
    pub amount: f32,
    pub interval: f32,
    pub radius: f32,
}

/// Mana pool numbers shipped per def (SPL1): `max` is the spawn pool and
/// ceiling, `regen` the per-second refill (clamped at max).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ManaDef {
    pub max: f32,
    pub regen: f32,
}

#[derive(Resource, Debug, Default, Clone)]
pub struct UnitRegistry {
    pub defs: HashMap<UnitId, UnitDef>,
}

impl UnitRegistry {
    pub fn get(&self, id: &str) -> Option<&UnitDef> {
        self.defs.get(id)
    }
}

// ---------------------------------------------------------------------------
// Instance components
// ---------------------------------------------------------------------------

/// Current health. Single writer: combat::resolve_damage (WarbaseSet::Resolve).
#[derive(Component, Debug)]
pub struct Hp(pub f32);

#[derive(Component, Debug)]
pub struct MaxHp(pub f32);

/// Current mana (SPL1). Spent at cast start by skill::process_casts; no
/// refund on interrupt (pinned by ADR-0016 §6).
#[derive(Component, Debug)]
pub struct Mana(pub f32);

/// Mana pool ceiling; regen clamps here (tick_skill_cooldowns).
#[derive(Component, Debug)]
pub struct MaxMana(pub f32);

/// Mana per second (SPL1); 0 = no regen.
#[derive(Component, Debug)]
pub struct ManaRegen(pub f32);

#[derive(Component, Debug)]
pub struct Armor(pub ArmorType);

#[derive(Component, Debug)]
pub struct UnitInstance {
    pub def: UnitId,
    pub level: u8,
}

/// Hero-tower growth holder: eats kill xp, banks skill points.
#[derive(Component, Debug)]
pub struct XpHolder {
    pub xp: u32,
    pub level: u8,
    pub points: u8,
    pub next_level_xp: u32,
    /// Milestone talents already taken (HR3 bookkeeping).
    pub talents_taken: u8,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Allegiance {
    /// Defending side (towers and friends).
    Defender,
    /// Attacking side (creeps).
    Attacker,
}

/// Pluggable friend-or-foe oracle (ADR-0017): the ONLY place combat/buff may
/// ask "are these two hostile/allied" — direct Allegiance comparisons are
/// forbidden outside this impl. MP1: answers come from a snapshot of
/// [`MatchDiplomacy`] (player -> team -> hostility matrix); [`apply_teams`]
/// rebuilds the snapshot in lockstep with the resource (source of truth).
///
/// Allegiance -> team mapping rule (the derived view is intentionally lossy
/// for 3+ sides; fidelity-critical consumers use the team-level helpers):
/// - `Defender` maps to `TeamId(0)` — the home/defending team (default
///   single player P0's team; entities without `Owner` are environment
///   units on exactly these two sides).
/// - `Attacker` maps to the environment team: the smallest team id hostile
///   to team 0, falling back to `TeamId(1)` when nothing is hostile to it.
///
/// Under `MatchDiplomacy::default()` this reproduces the historical
/// two-sided semantics exactly — the zero-drift contract of this increment.
#[derive(Resource, Debug, Clone, Default)]
pub struct Hostility {
    /// Diplomacy snapshot this oracle answers from. Kept in lockstep with
    /// the `MatchDiplomacy` resource by [`apply_teams`]; runtime diplomacy
    /// edits must rebuild both or the oracle answers from stale data.
    pub diplomacy: MatchDiplomacy,
}

impl From<&MatchDiplomacy> for Hostility {
    fn from(d: &MatchDiplomacy) -> Self {
        Self { diplomacy: d.clone() }
    }
}

impl Hostility {
    fn side_team(&self, side: Allegiance) -> TeamId {
        match side {
            Allegiance::Defender => TeamId(0),
            Allegiance::Attacker => self.diplomacy.environment_team(),
        }
    }

    pub fn hostile(&self, a: Allegiance, b: Allegiance) -> bool {
        self.diplomacy.teams_hostile(self.side_team(a), self.side_team(b))
    }

    pub fn allied(&self, a: Allegiance, b: Allegiance) -> bool {
        self.side_team(a) == self.side_team(b)
    }

    /// Team-level hostility query (full-fidelity API for MP2/FOG2 consumers:
    /// shared vision, damage attribution — the two-sided Allegiance view
    /// would lose neutrality there).
    pub fn teams_hostile(&self, a: TeamId, b: TeamId) -> bool {
        self.diplomacy.teams_hostile(a, b)
    }

    /// Team-level alliance: allied = same team (the model has no explicit
    /// alliance pairs); listed-but-unpaired teams are neutral.
    pub fn teams_allied(&self, a: TeamId, b: TeamId) -> bool {
        a == b
    }

    /// Team a player belongs to; None = not listed in the match table.
    pub fn team_of(&self, player: PlayerId) -> Option<TeamId> {
        self.diplomacy.team_of(player)
    }
}

/// Player slot in a match (ADR-0017 data layer; netcode is out of scope).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub u8);

/// Team a player belongs to (allied = shared vision/victory semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TeamId(pub u8);

/// Ownership marker on units/buildings (MP1 consumes; single-player maps
/// leave entities unowned = local player).
#[derive(Component, Debug, Clone, Copy)]
pub struct Owner(pub PlayerId);

/// Player -> team table + hostility pairs (map-bundle authored). Default =
/// single player P0 on team 0 versus the environment team 1 — identical to
/// the historical Defender/Attacker semantics.
#[derive(Resource, Debug, Clone)]
pub struct MatchDiplomacy {
    pub players: Vec<(PlayerId, TeamId)>,
    /// Hostile team pairs (both directions implied).
    pub hostile_pairs: Vec<(TeamId, TeamId)>,
}

impl Default for MatchDiplomacy {
    fn default() -> Self {
        Self {
            players: vec![(PlayerId(0), TeamId(0))],
            hostile_pairs: vec![(TeamId(0), TeamId(1))],
        }
    }
}

impl MatchDiplomacy {
    /// Team a player belongs to; None = not listed in the match table.
    pub fn team_of(&self, player: PlayerId) -> Option<TeamId> {
        self.players.iter().find(|(p, _)| *p == player).map(|(_, t)| *t)
    }

    /// Hostility matrix lookup. Pairs are symmetric (both directions
    /// implied) and a team is never hostile to itself regardless of the
    /// table contents.
    pub fn teams_hostile(&self, a: TeamId, b: TeamId) -> bool {
        if a == b {
            return false;
        }
        self.hostile_pairs
            .iter()
            .any(|(x, y)| (*x == a && *y == b) || (*x == b && *y == a))
    }

    /// Team-level alliance: allied = same team. Two listed-but-unpaired
    /// teams are neutral (neither hostile nor allied).
    pub fn teams_allied(&self, a: TeamId, b: TeamId) -> bool {
        a == b
    }

    /// The team the Attacker side maps to: the smallest team id hostile to
    /// team 0 (the Defender home team). Fallback `TeamId(1)` keeps the
    /// default two-sided split well-defined even with an empty matrix.
    pub fn environment_team(&self) -> TeamId {
        let mut env = TeamId(1);
        let mut found = false;
        for (a, b) in &self.hostile_pairs {
            let other = if *a == TeamId(0) && *b != TeamId(0) {
                Some(*b)
            } else if *b == TeamId(0) && *a != TeamId(0) {
                Some(*a)
            } else {
                None
            };
            if let Some(t) = other {
                if !found || t < env {
                    env = t;
                    found = true;
                }
            }
        }
        env
    }

    /// Derived `Allegiance` for an owned unit (ADR-0017 §3): hostile to
    /// team 0 => Attacker; everything else (same team, allied, neutral)
    /// folds to Defender — neutrals are non-combatants, not auto-targets.
    /// The view is intentionally lossy for 3+ sides; fidelity-critical
    /// consumers use the team-level queries. Owners missing from the table
    /// conservatively fold to Defender as well.
    pub fn allegiance_of(&self, owner: PlayerId) -> Allegiance {
        let team = self.team_of(owner).unwrap_or(TeamId(0));
        if self.teams_hostile(team, TeamId(0)) {
            Allegiance::Attacker
        } else {
            Allegiance::Defender
        }
    }
}

/// Map-bundle teams section (SEAM0d; optional, defaults to single player).
/// Empty `players` => the default diplomacy is used and `hostile_pairs` is
/// ignored (old maps without a teams section load unchanged).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TeamsDef {
    /// (player id, team id) slots of the match.
    #[serde(default)]
    pub players: Vec<(u8, u8)>,
    /// Hostile team pairs, both directions implied. Absent when a teams
    /// section lists players => `[(0, 1)]`, so a plain two-sided map only
    /// lists its players. (Whole-section absence goes through
    /// `TeamsDef::default()`, i.e. both vecs empty — `apply_teams` never
    /// reads `hostile_pairs` then.)
    #[serde(default = "default_hostile_pairs")]
    pub hostile_pairs: Vec<(u8, u8)>,
}

fn default_hostile_pairs() -> Vec<(u8, u8)> {
    vec![(0, 1)]
}

/// Bundle dispatch (SEAM0d): build the match diplomacy from the `teams`
/// section. Empty players => the default single-player layout (zero drift);
/// otherwise the table ships verbatim. Both `MatchDiplomacy` (source of
/// truth) and the `Hostility` oracle snapshot are (re)inserted in lockstep,
/// then owned units' derived `Allegiance` is refreshed immediately.
pub fn apply_teams(world: &mut World, bundle: &crate::bundle::MapBundle) {
    let diplomacy = if bundle.teams.players.is_empty() {
        MatchDiplomacy::default()
    } else {
        MatchDiplomacy {
            players: bundle
                .teams
                .players
                .iter()
                .map(|&(p, t)| (PlayerId(p), TeamId(t)))
                .collect(),
            hostile_pairs: bundle
                .teams
                .hostile_pairs
                .iter()
                .map(|&(a, b)| (TeamId(a), TeamId(b)))
                .collect(),
        }
    };
    world.insert_resource(Hostility::from(&diplomacy));
    world.insert_resource(diplomacy);
    sync_allegiance(world);
}

/// Refresh the derived `Allegiance` view from `Owner` + `MatchDiplomacy`
/// (ADR-0017 §3): warden-era Allegiance readers keep working while `Owner`
/// becomes the multiplayer source of identity. Runs in `WarbaseSet::Cleanup`
/// (after this frame's combat reads, before the next frame's targeting).
pub fn sync_allegiance(world: &mut World) {
    let diplomacy = world.resource::<MatchDiplomacy>().clone();
    let mut owned = world.query::<(&Owner, &mut Allegiance)>();
    for (owner, mut side) in owned.iter_mut(world) {
        *side = diplomacy.allegiance_of(owner.0);
    }
}

/// System form of [`sync_allegiance`] for plugin wiring.
#[allow(dead_code)] // SEAM GAP — see UnitPlugin below.
fn sync_allegiance_system(
    diplomacy: Res<MatchDiplomacy>,
    mut owned: Query<(&Owner, &mut Allegiance)>,
) {
    for (owner, mut side) in &mut owned {
        *side = diplomacy.allegiance_of(owner.0);
    }
}

/// Unit-domain plugin (MP1): owns the Owner -> Allegiance derivation in
/// `WarbaseSet::Cleanup`.
///
/// SEAM GAP (recorded for the next SEAM revision): lib.rs is frozen this
/// increment and does NOT wire this plugin into `WarbasePlugin`, and the
/// `unit` module is private, so neither `UnitPlugin` nor the plugin-wired
/// system is reachable from consumer games or integration tests. Until the
/// seam is revised (export the unit plugin/prelude + wire it once in
/// lib.rs), consumers get correct diplomacy resources from `apply_teams`
/// (which also refreshes already-owned entities once at assembly), but
/// per-frame ownership refresh is engine-internal. The derivation rule
/// itself is pinned by tests through `MatchDiplomacy::allegiance_of`,
/// which is reachable via the prelude.
#[allow(dead_code)]
pub struct UnitPlugin;

impl Plugin for UnitPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync_allegiance_system.in_set(crate::WarbaseSet::Cleanup));
    }
}

/// Present on units with UnitTrait::Air; Ground-only attacks skip them.
#[derive(Component, Debug)]
pub struct AirUnit;

/// Spawn a unit instance from its def; `hp_scale` multiplies hp/max-hp
/// (per-wave scaling ships in the wave table). Returns None for unknown ids.
/// Extra behaviour comes from composition (caller inserts PathFollower,
/// XpHolder, ...).
pub fn spawn_unit(
    world: &mut World,
    registry: &UnitRegistry,
    id: &str,
    pos: Vec3,
    side: Allegiance,
    hp_scale: f32,
) -> Option<Entity> {
    let def = registry.get(id)?.clone();
    let known_skills: Vec<Option<bool>> = {
        let defs = world.resource::<crate::skill::SkillDefs>();
        def.skills
            .iter()
            .map(|id| defs.get(id).map(|d| d.kind == crate::skill::SkillKind::Autocast))
            .collect()
    };
    let mut e = world.spawn((
        UnitInstance { def: def.id.clone(), level: 1 },
        Hp(def.attrs.hp * hp_scale),
        MaxHp(def.attrs.hp * hp_scale),
        Armor(def.armor_type),
        side,
        Transform::from_translation(pos),
        crate::buff::StatMods::default(),
    ));
    if def.traits.contains(&UnitTrait::Air) {
        e.insert(AirUnit);
    }
    if def.attrs.move_speed > 0.0 {
        e.insert(crate::path::MoveSpeed(def.attrs.move_speed));
    }
    if let Some(atk) = def.attack {
        e.insert(crate::combat::AttackKit {
            def: atk,
            priority: crate::combat::TargetingPriority::First,
        })
        .insert(crate::combat::AttackCooldown(0.0))
        .insert(crate::combat::CurrentTarget::default());
    }
    if !def.auras.is_empty() {
        e.insert(crate::buff::Auras(def.auras.clone()));
    }
    if def.hero {
        e.insert(XpHolder { xp: 0, level: 1, points: 0, next_level_xp: 10, talents_taken: 0 });
    }
    if let Some(h) = def.heal {
        e.insert(crate::combat::Healer { amount: h.amount, interval: h.interval, radius: h.radius, timer: 0.0 });
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
                // Hole-9: authored initial level per slot (positional zip,
                // short vecs pad 0); Autocast slots resolve 0 -> 1 so the
                // legacy "autocast starts learned and ON" survives authored
                // omissions. See UnitDef::skill_levels for the full rule.
                // (`autocast` here is Option<bool> = "known && is-autocast";
                // only the Some(true) case bumps.)
                let authored = def.skill_levels.get(i).copied().unwrap_or(0);
                let is_autocast = autocast.unwrap_or(false);
                let level = if is_autocast && authored == 0 { 1 } else { authored };
                crate::skill::SkillSlot {
                    skill: id.clone(),
                    level,
                    cooldown: 0.0,
                    // Autocast-kind skills start ON (WC3 habit); players toggle.
                    autocast_on: is_autocast,
                }
            })
            .collect();
        e.insert(crate::skill::SkillCaster { slots });
    }
    // HERO (round-2 plan §3): three primary attributes, conversion applied
    // at spawn ("spawn 时算定" — see hero::apply_attribute_bonuses).
    let id = e.id();
    if let Some(stats) = def.hero_stats {
        world.entity_mut(id).insert(stats);
        crate::hero::apply_attribute_bonuses(world, id);
    }
    Some(id)
}

/// Commands-based variant of [`spawn_unit`] for use inside systems (the
/// wave director). Keep the component set in sync with the World version.
pub fn spawn_unit_deferred(
    commands: &mut Commands,
    registry: &UnitRegistry,
    id: &str,
    pos: Vec3,
    side: Allegiance,
    hp_scale: f32,
) -> Option<Entity> {
    let def = registry.get(id)?.clone();
    let mut ec = commands.spawn((
        UnitInstance { def: def.id.clone(), level: 1 },
        Hp(def.attrs.hp * hp_scale),
        MaxHp(def.attrs.hp * hp_scale),
        Armor(def.armor_type),
        side,
        Transform::from_translation(pos),
        crate::buff::StatMods::default(),
    ));
    if def.traits.contains(&UnitTrait::Air) {
        ec.insert(AirUnit);
    }
    if def.attrs.move_speed > 0.0 {
        ec.insert(crate::path::MoveSpeed(def.attrs.move_speed));
    }
    if let Some(atk) = def.attack {
        ec.insert(crate::combat::AttackKit {
            def: atk,
            priority: crate::combat::TargetingPriority::First,
        })
        .insert(crate::combat::AttackCooldown(0.0))
        .insert(crate::combat::CurrentTarget::default());
    }
    if !def.auras.is_empty() {
        ec.insert(crate::buff::Auras(def.auras.clone()));
    }
    if def.hero {
        ec.insert(XpHolder { xp: 0, level: 1, points: 0, next_level_xp: 10, talents_taken: 0 });
    }
    if let Some(h) = def.heal {
        ec.insert(crate::combat::Healer { amount: h.amount, interval: h.interval, radius: h.radius, timer: 0.0 });
    }
    if let Some(m) = def.mana {
        ec.insert((Mana(m.max), MaxMana(m.max), ManaRegen(m.regen)));
    }
    // HERO: attributes ride along; the conversion table is a resource and
    // Commands cannot read resources mid-system, so deferred heroes carry
    // `HeroStatsRaw` and the hero Tick converts them on the first frame
    // after materialization (World-variant spawns convert at spawn).
    if let Some(stats) = def.hero_stats {
        ec.insert((stats, crate::hero::HeroStatsRaw));
    }
    // Skill slots stay exclusive to the World variant: assembling them needs
    // a SkillDefs read, which Commands cannot do mid-system.
    Some(ec.id())
}
