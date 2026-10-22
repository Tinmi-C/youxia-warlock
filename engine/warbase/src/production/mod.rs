//! Production & research (WC3 alignment, ADR-0016): building train queues
//! (pay on enqueue, 100% refund on cancel, population-gated pause), player
//! research (stat grants / build & skill unlocks), rally points.
//!
//! Scheduling: queue/research progress ticks in Tick (PRD1/2); completed
//! units spawn via unit::spawn_unit_deferred. SEAM0b pins the types.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// One queued entry (unit or research id) with its remaining seconds.
#[derive(Debug, Clone)]
pub struct QueueEntry {
    pub what: String,
    pub remaining: f32,
    /// Total duration (progress = 1 - remaining/total).
    pub total: f32,
}

/// Building-side production component. `trains` lists trainable unit ids;
/// `queue_cap` bounds the VecDeque (WC3 default 5).
#[derive(Component, Debug)]
pub struct ProductionQueue {
    pub trains: Vec<String>,
    pub queue_cap: u8,
    pub entries: std::collections::VecDeque<QueueEntry>,
    /// Population-full pause (readable state, PRD1).
    pub paused: bool,
}

/// Player command: enqueue a unit on a building (pays immediately).
#[derive(Message, Debug, Clone)]
pub struct TrainCommand {
    pub builder: Entity,
    pub unit: String,
}

/// Player command: cancel queue entry `index` (100% refund).
#[derive(Message, Debug, Clone)]
pub struct CancelTrain {
    pub builder: Entity,
    pub index: usize,
}

/// Rally point: freshly trained units receive this default order (none =
/// hold position). PRD3.
#[derive(Component, Debug, Default, Clone)]
pub struct RallyPoint(pub Option<crate::movement::Order>);

/// Research grant vocabulary (PRD2): what a finished research unlocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResearchGrant {
    /// Permanent stat mod for every unit of `unit` (e.g. attack upgrade).
    Stat { unit: String, kind: crate::buff::StatKind, amount: f32 },
    /// Reveals a hidden buildable in command cards.
    UnlockBuild(String),
    /// Reveals a hidden skill on its carriers.
    UnlockSkill(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchDef {
    pub id: String,
    pub cost_gold: u32,
    pub time: f32,
    pub grants: Vec<ResearchGrant>,
}

/// Map-bundle production/research section (SEAM0d; optional, defaults
/// empty). `research` is the per-map tech tree; queues live on entities.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProductionDef {
    #[serde(default)]
    pub research: Vec<ResearchDef>,
    /// Player-side single research queue (simplification, plan §8 PRD2).
    #[serde(default = "default_research_cap")]
    pub research_cap: u8,
}

fn default_research_cap() -> u8 {
    1
}

/// Bundle dispatch stub (SEAM0d): PRD1/2 fill this in.
pub fn apply_production(world: &mut World, bundle: &crate::bundle::MapBundle) {
    let _ = (world, bundle);
    // TODO(PRD1): insert ResearchTable from ProductionDef.
}

pub mod prelude {
    pub use super::{CancelTrain, ProductionDef, ProductionQueue, QueueEntry, RallyPoint, ResearchDef, ResearchGrant, TrainCommand};
}
