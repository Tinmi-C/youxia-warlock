//! MapBundle (BX9a, ADR-0013): a map is a self-contained RON data file — the
//! only authored form. Loader = validator: bad syntax, missing fields,
//! dangling unit/path references and version mismatches are rejected with
//! readable errors (never silently migrated). The assembler (BX9b) turns a
//! validated bundle into resources + path entities.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::econ::{EconRules, EconStart};
use crate::path::PathDef;
use crate::rule::MatchRules;
use crate::unit::{AttackArmorTable, UnitDef, UnitId};
use crate::wave::WaveTable;

/// Bump on schema changes; loaders reject other versions outright.
pub const MAP_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapIdentity {
    pub id: String,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BuildDomainDef {
    /// Loop-TD: free placement on a grid.
    FreeGrid { cell: f32, cols: i32, rows: i32 },
    /// warden: fixed tower slots.
    FixedSlots { slots: Vec<[f32; 2]> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldDef {
    /// Creep tracks. May be EMPTY on non-TD maps (hole-3 upcast: the old
    /// "must not be empty" rule was the TD heritage — RTS/skirmish maps
    /// have no creep lanes; the wave table must then be empty too, see
    /// `validate`).
    pub paths: Vec<PathDef>,
    pub build: BuildDomainDef,
    /// Grid cells blocked by scenery/paths, in grid coordinates.
    #[serde(default)]
    pub blocked: Vec<[i32; 2]>,
}

/// One authored opening unit group (hole-3 upcast from rts-lab's game-side
/// `STARTING_LAYOUT`): `count` copies of def `unit` at ground `(x, z)`,
/// owned by player `owner` (None = unowned; unowned folds to the Defender
/// side, MP1's lossy view). Spawned by `assemble` at match start; the
/// game-side kit (Selectable, MoveAgent, ...) stays the game's to attach.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartingForce {
    pub unit: UnitId,
    /// Copies to spawn (validate rejects 0).
    pub count: u8,
    /// Ground (x, z) — the hole-1 XZ convention; spawned at Vec3 (x, 0, z).
    pub pos: [f32; 2],
    /// Owning player slot; None = unowned (Defender side, no Owner marker).
    pub owner: Option<u8>,
}

/// The map file. All content numbers live here (ADR-0013 narrowed ADR-0008);
/// the base ships only mechanisms.
///
/// New-domain sections (ADR-0016 alignment plan) are optional with serde
/// defaults: appending an optional section does NOT bump MAP_FORMAT_VERSION
/// (schema superset, old maps load unchanged); only breaking changes bump.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapBundle {
    pub identity: MapIdentity,
    pub world: WorldDef,
    pub units: HashMap<UnitId, UnitDef>,
    pub waves: WaveTable,
    pub rules: MatchRules,
    pub econ_start: EconStart,
    #[serde(default)]
    pub econ_rules: EconRules,
    pub attack_armor: AttackArmorTable,
    /// Navigation grid overrides (MOV2; defaults empty = free map).
    #[serde(default)]
    pub nav: crate::movement::NavDef,
    /// Vision/fog parameters (FOG1; defaults = 1.0 cell).
    #[serde(default)]
    pub vision: crate::vision::VisionDef,
    /// Item table (INV; defaults empty).
    #[serde(default)]
    pub items: crate::item::ItemsDef,
    /// Production/research section (PRD; defaults empty).
    #[serde(default)]
    pub production: crate::production::ProductionDef,
    /// Player/team layout (MP1; defaults = single player vs environment).
    #[serde(default)]
    pub teams: crate::unit::TeamsDef,
    /// Authored opening units (hole-3; defaults empty — old maps load and
    /// spawn nothing, exactly as before).
    #[serde(default)]
    pub starting_forces: Vec<StartingForce>,
}

#[derive(Debug)]
pub enum MapError {
    Io(std::io::Error),
    Parse(ron::error::SpannedError),
    Invalid(String),
}

impl std::fmt::Display for MapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MapError::Io(e) => write!(f, "io error: {e}"),
            MapError::Parse(e) => write!(f, "parse error: {e}"),
            MapError::Invalid(msg) => write!(f, "invalid map: {msg}"),
        }
    }
}

impl std::error::Error for MapError {}

/// Load + validate a map file. Rejection is loud and specific; there is no
/// silent migration path by design (ADR-0013).
pub fn load_bundle(path: &Path) -> Result<MapBundle, MapError> {
    let text = std::fs::read_to_string(path).map_err(MapError::Io)?;
    let bundle: MapBundle = ron::from_str(&text).map_err(MapError::Parse)?;
    validate(&bundle)?;
    Ok(bundle)
}

/// Validate a parsed bundle: required fields, reference existence, version.
pub fn validate(bundle: &MapBundle) -> Result<(), MapError> {
    let invalid = |msg: String| Err(MapError::Invalid(msg));

    if bundle.identity.id.trim().is_empty() {
        return invalid("identity.id must be non-empty".into());
    }
    if bundle.identity.version != MAP_FORMAT_VERSION {
        return invalid(format!(
            "map version {} != supported {} (refusing to migrate silently)",
            bundle.identity.version, MAP_FORMAT_VERSION
        ));
    }
    if bundle.units.is_empty() {
        return invalid("units must not be empty".into());
    }
    // Unit defs must be keyed by their own id (spawn looks up by def id).
    for (key, def) in &bundle.units {
        if key != &def.id {
            return invalid(format!("units key '{key}' != def.id '{}'", def.id));
        }
    }
    // Hole-3: `world.paths` may be empty (non-TD maps — RTS/skirmish lanes
    // live in `starting_forces`, not creep tracks). The wave table guards
    // below already reject any spawn that would reference a path index out
    // of range, so "empty paths" is legal exactly while "no spawns" holds
    // (hole-5: empty wave table + empty paths = a valid skirmish map).
    for (i, force) in bundle.starting_forces.iter().enumerate() {
        if !bundle.units.contains_key(&force.unit) {
            return invalid(format!(
                "starting_forces {i}: unknown unit '{}'",
                force.unit
            ));
        }
        if force.count == 0 {
            return invalid(format!("starting_forces {i}: count must be > 0"));
        }
        if !force.pos.iter().all(|v| v.is_finite()) {
            return invalid(format!("starting_forces {i}: pos must be finite"));
        }
    }
    for (i, wave) in bundle.waves.waves.iter().enumerate() {
        for (j, spec) in wave.spawns.iter().enumerate() {
            if !bundle.units.contains_key(&spec.unit) {
                return invalid(format!("wave {i} spawn {j}: unknown unit '{}'", spec.unit));
            }
            if spec.path >= bundle.world.paths.len() {
                return invalid(format!(
                    "wave {i} spawn {j}: path index {} out of range ({} paths)",
                    spec.path,
                    bundle.world.paths.len()
                ));
            }
            if spec.count == 0 {
                return invalid(format!("wave {i} spawn {j}: count must be > 0"));
            }
        }
    }
    if let crate::rule::LeakPolicy::Stockpile { cap } = &bundle.rules.leak {
        if *cap == 0 {
            return invalid("Stockpile cap must be > 0".into());
        }
    }
    for row in &bundle.attack_armor.mult {
        for m in row {
            if !m.is_finite() || *m < 0.0 {
                return invalid("attack_armor multipliers must be finite and >= 0".into());
            }
        }
    }
    if let crate::rule::WinCondition::SurviveWaves { waves } = &bundle.rules.win {
        if *waves as usize > bundle.waves.waves.len() {
            return invalid(format!(
                "SurviveWaves({waves}) exceeds the wave table ({} waves)",
                bundle.waves.waves.len()
            ));
        }
    }
    Ok(())
}

/// Derive the build grid from the map's world def: FreeGrid bounds get
/// centered on the path bounding box (signed-coordinate maps), path cells
/// are blocked; FixedSlots pass through as slot centers. A pathless world
/// (hole-3: RTS/skirmish maps) centers the FreeGrid on the origin (0, 0)
/// instead — there is no bbox to align with, and origin-centering keeps
/// signed-coordinate gameplay (arena around (0, 0)) symmetric.
pub fn build_grid(world: &WorldDef) -> crate::build::BuildGrid {
    use crate::build::{BuildGrid, Cell};
    match &world.build {
        BuildDomainDef::FreeGrid { cell, cols, rows } => {
            let mut grid = BuildGrid::free_grid(*cell, *cols, *rows);
            let center = if world.paths.is_empty() {
                Vec2::ZERO
            } else {
                let mut min = Vec2::splat(f32::MAX);
                let mut max = Vec2::splat(f32::MIN);
                for path in &world.paths {
                    for p in &path.points {
                        min.x = min.x.min(p[0]);
                        min.y = min.y.min(p[1]);
                        max.x = max.x.max(p[0]);
                        max.y = max.y.max(p[1]);
                    }
                }
                (min + max) * 0.5
            };
            grid.origin = center - Vec2::new(*cols as f32 * cell / 2.0, *rows as f32 * cell / 2.0);
            for path in &world.paths {
                let len = path.length();
                let mut d = 0.0;
                while d < len {
                    grid.blocked.insert(grid.world_to_cell(path.sample(d)));
                    d += cell * 0.25;
                }
                grid.blocked.insert(grid.world_to_cell(path.sample(len)));
            }
            let _ = Cell(0, 0);
            grid
        }
        BuildDomainDef::FixedSlots { slots } => {
            BuildGrid::fixed_slots(slots.iter().map(|s| Vec2::new(s[0], s[1])).collect())
        }
    }
}

/// Assemble a validated bundle into a world: insert resources, spawn path
/// entities, record them in `Paths`. Re-running a match = despawn entities +
/// re-assemble (never rebuild the World — KB pitfall).
pub fn assemble(world: &mut World, bundle: &MapBundle) {
    let defs = bundle.units.clone();
    world.insert_resource(crate::unit::UnitRegistry { defs });
    world.insert_resource(bundle.attack_armor.clone());
    world.insert_resource(bundle.rules.clone());
    world.insert_resource(bundle.waves.clone());
    world.insert_resource(bundle.econ_rules.clone());
    {
        let mut res = world.resource_mut::<crate::econ::Resources>();
        res.gold = bundle.econ_start.gold;
        res.wood = bundle.econ_start.wood;
    }
    {
        let mut pop = world.resource_mut::<crate::econ::Population>();
        pop.cap = bundle.econ_start.pop_cap;
        pop.used = 0;
    }
    world.get_resource_or_insert_with(|| crate::econ::PopPurchases(0));

    let path_entities: Vec<Entity> = bundle
        .world
        .paths
        .iter()
        .map(|p| world.spawn(p.clone()).id())
        .collect();
    world.insert_resource(crate::wave::Paths(path_entities));

    // Fresh director + flow for the assembled match.
    world.insert_resource(crate::wave::WaveDirector::default());
    world.insert_resource(crate::rule::MatchFlow::Running);
    world.insert_resource(crate::rule::StockCount(0));
    world.remove_resource::<crate::rule::LivesLeft>();

    // New-domain dispatch (SEAM0d): one line per domain, implemented in the
    // domain module. Never add domain loading logic to this file.
    crate::movement::apply_nav(world, bundle);
    crate::selection::apply_selection(world, bundle);
    crate::vision::apply_vision(world, bundle);
    crate::item::apply_items(world, bundle);
    crate::production::apply_production(world, bundle);
    crate::unit::apply_teams(world, bundle);

    // Starting forces (hole-3): spawn the authored opening units AFTER the
    // domain dispatch so the fresh diplomacy (apply_teams) derives each
    // side. Vec order is authored order (deterministic); unknown ids are
    // unreachable through `load_bundle` (validate rejects them) and simply
    // skip on direct construction. Owner -> derived Allegiance via MP1's
    // lossy view (hostile to team 0 = Attacker, else Defender); None =
    // unowned, no marker (folds Defender). Population is NOT booked here —
    // pop accounting is a game economy decision (rts-lab books its own).
    {
        let registry = world.resource::<crate::unit::UnitRegistry>().clone();
        let diplomacy = world.resource::<crate::unit::MatchDiplomacy>().clone();
        for force in &bundle.starting_forces {
            let side = force
                .owner
                .map(|p| diplomacy.allegiance_of(crate::unit::PlayerId(p)))
                .unwrap_or(crate::unit::Allegiance::Defender);
            for _ in 0..force.count {
                let pos = Vec3::new(force.pos[0], 0.0, force.pos[1]);
                if let Some(entity) =
                    crate::unit::spawn_unit(world, &registry, &force.unit, pos, side, 1.0)
                {
                    if let Some(owner) = force.owner {
                        world
                            .entity_mut(entity)
                            .insert(crate::unit::Owner(crate::unit::PlayerId(owner)));
                    }
                }
            }
        }
    }
}

// ---------- art-needs projection (ADR-0015) ----------

/// Project a bundle's unit defs to JSON for `tools/art-needs` (ADR-0015).
///
/// This is the normalized input of the demand pipeline: Python tooling never
/// parses RON text, it consumes this projection (serde-validated, so a format
/// drift fails loudly instead of silently misreading). Field names are the
/// contract with tools/art-needs — keep them stable across internal types.
/// Output is deterministic: units sorted by id, wave_refs = spawned body count.
pub fn dump_units_json(bundle: &MapBundle) -> String {
    let mut wave_refs: HashMap<&str, usize> = HashMap::new();
    for wave in &bundle.waves.waves {
        for spec in &wave.spawns {
            *wave_refs.entry(spec.unit.as_str()).or_default() += spec.count as usize;
        }
    }

    let mut units: Vec<serde_json::Value> = bundle
        .units
        .values()
        .map(|def| {
            let kind = if def.hero {
                "hero"
            } else if def.cost_gold > 0 {
                "tower"
            } else if def.bounty_gold > 0 {
                "creep"
            } else {
                "other"
            };
            serde_json::json!({
                "id": def.id,
                "kind": kind,
                "hp": def.attrs.hp,
                "move_speed": def.attrs.move_speed,
                "body_radius": def.attrs.body_radius,
                "armor_type": format!("{:?}", def.armor_type),
                "attack_type": def.attack.as_ref()
                    .map(|a| format!("{:?}", a.attack_type)),
                "traits": def.traits.iter().map(|t| format!("{t:?}")).collect::<Vec<_>>(),
                "upgrade_to": &def.upgrade_to,
                "has_aura": !def.auras.is_empty(),
                "hero": def.hero,
                "wave_refs": wave_refs.get(def.id.as_str()).copied().unwrap_or(0),
            })
        })
        .collect();
    units.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));

    serde_json::to_string_pretty(&serde_json::json!({
        "map": {"id": bundle.identity.id, "version": bundle.identity.version},
        "units": units,
    }))
    .expect("unit projection serializes to JSON")
}
