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
    pub paths: Vec<PathDef>,
    pub build: BuildDomainDef,
    /// Grid cells blocked by scenery/paths, in grid coordinates.
    #[serde(default)]
    pub blocked: Vec<[i32; 2]>,
}

/// The map file. All content numbers live here (ADR-0013 narrowed ADR-0008);
/// the base ships only mechanisms.
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
    if bundle.world.paths.is_empty() {
        return invalid("world.paths must not be empty".into());
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
/// are blocked; FixedSlots pass through as slot centers.
pub fn build_grid(world: &WorldDef) -> crate::build::BuildGrid {
    use crate::build::{BuildGrid, Cell};
    match &world.build {
        BuildDomainDef::FreeGrid { cell, cols, rows } => {
            let mut grid = BuildGrid::free_grid(*cell, *cols, *rows);
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
            let center = (min + max) * 0.5;
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
}
