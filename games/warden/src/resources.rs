//! Resources = global singletons + the game-data tables (tower / enemy / wave).
//! Values are *starting values* (see `docs/requirements.md` §14); they stay in
//! code per team ADR-0008. The data lives here so it is easy to tune and to
//! assert against in tests.

use std::collections::VecDeque;

use bevy::prelude::*;
use rand::rngs::StdRng;
use rand::SeedableRng;

use crate::components::{AttackType, FusionKind};

/// Run-scoped RNG for the acquisition rolls (AC1 opening hand / AC2 shop
/// refresh / AC3 drops). Seeded from entropy at startup; tests replace it with
/// `RunRng::seeded(seed)` before the first update for determinism.
#[derive(Resource)]
pub struct RunRng(pub StdRng);

impl RunRng {
    pub fn seeded(seed: u64) -> Self {
        Self(StdRng::seed_from_u64(seed))
    }
}

impl Default for RunRng {
    fn default() -> Self {
        // rand 0.10: make_rng is the entropy-seeded constructor (the old
        // SeedableRng::from_entropy was removed).
        Self(rand::make_rng::<StdRng>())
    }
}

/// Economic pool. No interest (requirements §11): gold only enters by kill /
/// wave reward and leaves by buying towers / fusion fee.
#[derive(Resource)]
pub struct Economy {
    pub gold: u32,
}

impl Default for Economy {
    fn default() -> Self {
        Self { gold: 100 }
    }
}

/// Base HP (the path endpoint). Lose when it reaches 0 (requirements §4).
#[derive(Resource)]
pub struct BaseHp {
    pub hp: u32,
    pub max_hp: u32,
}

impl Default for BaseHp {
    fn default() -> Self {
        Self {
            hp: 10,
            max_hp: 10,
        }
    }
}

/// Ordered waypoints from entry to base (XZ plane). Enemies follow these in
/// order. Inserted by `MapPlugin`.
#[derive(Resource)]
pub struct PathInfo {
    pub waypoints: Vec<Vec3>,
}

/// One tower archetype (requirements §5).
#[derive(Clone)]
pub struct TowerDef {
    pub name: &'static str,
    pub cost: u32,
    pub damage: f32,
    pub attack_speed: f32, // shots per second
    pub range: f32,        // world units
    pub attack_type: AttackType,
}

#[derive(Resource)]
pub struct TowerDefs {
    pub list: Vec<TowerDef>,
}

impl TowerDefs {
    /// Index order matters and is referenced by the input mapping (keys 1-4).
    pub fn palette() -> Self {
        Self {
            list: vec![
                TowerDef { name: "archer", cost: 50, damage: 10.0, attack_speed: 1.0, range: 9.0, attack_type: AttackType::Physical },
                TowerDef { name: "shield", cost: 40, damage: 5.0, attack_speed: 1.2, range: 4.0, attack_type: AttackType::Physical },
                TowerDef { name: "mage", cost: 70, damage: 15.0, attack_speed: 0.5, range: 14.0, attack_type: AttackType::Magic },
                TowerDef { name: "cannon", cost: 80, damage: 25.0, attack_speed: 0.33, range: 12.0, attack_type: AttackType::Physical },
            ],
        }
    }
}

/// A fusion recipe (requirements §7.3). Fusion is the only way to obtain these
/// towers. `ingredients` are base tower indices (order-free); fee = `fee_ratio ×
/// (cost(ing0)+cost(ing1))`. The fused tower occupies one slot and frees the
/// other, netting one free buildable slot.
/// Damage/attack values below are rebalanced (2026-09-03) so each fused tower's
/// DPS lands in the §7.2 90–110% iron-law band — they are placeholders, tune on
/// the balance card / playtest.
#[derive(Clone)]
pub struct FusionDef {
    pub name: &'static str,
    pub ingredients: [usize; 2],
    pub fee_ratio: f32,
    pub damage: f32,
    pub attack_speed: f32,
    pub range: f32,
    pub attack_type: AttackType,
    pub kind: FusionKind,
    pub aoe_radius: f32,
    pub slow_factor: f32,
    pub slow_duration: f32,
}

#[derive(Resource)]
pub struct FusionDefs {
    pub list: Vec<FusionDef>,
}

impl FusionDefs {
    pub fn palette() -> Self {
        Self {
            list: vec![
                // 神射手: 2×archer(0) — range +40%, highest-HP priority.
                FusionDef {
                    name: "marksman", ingredients: [0, 0], fee_ratio: 0.2,
                    damage: 18.0, attack_speed: 1.2, range: 12.6,
                    attack_type: AttackType::Physical, kind: FusionKind::Marksman,
                    aoe_radius: 0.0, slow_factor: 0.0, slow_duration: 0.0,
                },
                // 大法师: 2×mage(2) — AOE blast. DPS 15 = 100% of 2×mage(15).
                FusionDef {
                    name: "archmage", ingredients: [2, 2], fee_ratio: 0.2,
                    damage: 25.0, attack_speed: 0.6, range: 16.0,
                    attack_type: AttackType::Magic, kind: FusionKind::Archmage,
                    aoe_radius: 3.0, slow_factor: 0.0, slow_duration: 0.0,
                },
                // 魔弓手: archer(0)+mage(2) — mixed damage bypasses armor. DPS 16.2 ≈ 93%.
                FusionDef {
                    name: "hybrid", ingredients: [0, 2], fee_ratio: 0.2,
                    damage: 18.0, attack_speed: 0.9, range: 11.0,
                    attack_type: AttackType::Mixed, kind: FusionKind::Hybrid,
                    aoe_radius: 0.0, slow_factor: 0.0, slow_duration: 0.0,
                },
                // 壁垒炮: shield(1)+cannon(3) — AOE + 30% slow. DPS 14.0 ≈ 98%.
                FusionDef {
                    name: "bastion", ingredients: [1, 3], fee_ratio: 0.2,
                    damage: 40.0, attack_speed: 0.35, range: 9.0,
                    attack_type: AttackType::Physical, kind: FusionKind::Bastion,
                    aoe_radius: 3.5, slow_factor: 0.7, slow_duration: 1.0,
                },
            ],
        }
    }
}

/// One enemy archetype (requirements §9).
#[derive(Clone)]
pub struct EnemyDef {
    pub name: &'static str,
    pub hp: f32,
    pub speed: f32,
    pub leak: u32,
    pub kill_gold: u32,
    pub physical_armor: bool,
    pub healer: bool,
}

#[derive(Resource)]
pub struct EnemyDefs {
    pub list: Vec<EnemyDef>,
}

impl EnemyDefs {
    /// Index order matters; wave configs reference these indices.
    pub fn palette() -> Self {
        Self {
            list: vec![
                // 0 ordinary
                EnemyDef { name: "ordinary", hp: 30.0, speed: 1.0, leak: 1, kill_gold: 6, physical_armor: false, healer: false },
                // 1 fast
                EnemyDef { name: "fast", hp: 20.0, speed: 1.8, leak: 1, kill_gold: 8, physical_armor: false, healer: false },
                // 2 shield (physical armor -50%)
                EnemyDef { name: "shield", hp: 90.0, speed: 0.6, leak: 2, kill_gold: 12, physical_armor: true, healer: false },
                // 3 healer
                EnemyDef { name: "healer", hp: 45.0, speed: 1.0, leak: 1, kill_gold: 10, physical_armor: false, healer: true },
                // 4 elite
                EnemyDef { name: "elite", hp: 200.0, speed: 0.8, leak: 3, kill_gold: 50, physical_armor: false, healer: false },
                // 5 boss
                EnemyDef { name: "boss", hp: 600.0, speed: 0.5, leak: 5, kill_gold: 100, physical_armor: false, healer: false },
            ],
        }
    }
}

/// A line of a wave: spawn `count` copies of enemy `enemy_index`, spaced by
/// `spawn_interval` (set on `WaveState`).
#[derive(Clone)]
pub struct WaveEntry {
    pub enemy_index: usize,
    pub count: u32,
}

/// One wave's composition + the gold reward for clearing it (requirements §10).
#[derive(Clone)]
pub struct Wave {
    pub entries: Vec<WaveEntry>,
    pub reward: u32,
}

#[derive(Resource)]
pub struct WaveSchedule {
    pub waves: Vec<Wave>,
}

impl WaveSchedule {
    /// 10 waves (requirements §10). Indices reference `EnemyDefs`.
    pub fn schedule() -> Self {
        let e = |i: usize, c: u32| WaveEntry { enemy_index: i, count: c };
        Self {
            waves: vec![
                Wave { entries: vec![e(0, 5)], reward: 20 },
                Wave { entries: vec![e(0, 8)], reward: 25 },
                Wave { entries: vec![e(0, 10), e(1, 3)], reward: 30 },
                Wave { entries: vec![e(0, 12), e(2, 2)], reward: 35 },
                Wave { entries: vec![e(4, 1), e(0, 6)], reward: 40 },
                Wave { entries: vec![e(0, 14), e(1, 4), e(2, 2)], reward: 45 },
                Wave { entries: vec![e(0, 10), e(2, 3), e(3, 2)], reward: 50 },
                Wave { entries: vec![e(0, 16), e(1, 5), e(2, 3), e(4, 1)], reward: 55 },
                Wave { entries: vec![e(2, 12), e(3, 4), e(4, 2)], reward: 60 },
                Wave { entries: vec![e(5, 1), e(0, 8), e(2, 4)], reward: 100 },
            ],
        }
    }
}

/// Dual-speed phase (requirements §3 "节奏命门"): the slow/intermission time is
/// the only window where placement/building is allowed; during combat it is
/// locked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WavePhase {
    Intermission,
    Combat,
}

/// Runtime wave progression (independent resource so systems can gate on phase).
#[derive(Resource)]
pub struct WaveState {
    pub current: u32,
    pub phase: WavePhase,
    pub spawn_queue: VecDeque<usize>, // enemy indices still to spawn this wave
    pub spawn_timer: f32,
    pub spawn_interval: f32,
    pub active: u32, // spawned, not yet dead or leaked
}

impl Default for WaveState {
    fn default() -> Self {
        Self {
            current: 0,
            phase: WavePhase::Intermission,
            spawn_queue: VecDeque::new(),
            spawn_timer: 0.0,
            spawn_interval: 0.5,
            active: 0,
        }
    }
}

/// Selects which tower archetype is currently chosen for placement.
#[derive(Resource)]
pub struct SelectedTower {
    pub tower_index: usize, // 0..3 into `TowerDefs`
}

impl Default for SelectedTower {
    fn default() -> Self {
        Self { tower_index: 0 }
    }
}

/// Mouse "build mode" is armed when a shop button is clicked; a left click on a
/// slot then places that tower. Cleared by a right click.
#[derive(Resource)]
pub struct BuildMode {
    pub armed: bool,
}

impl Default for BuildMode {
    fn default() -> Self {
        Self { armed: false }
    }
}

/// Tower selected for fusion (mouse): the first chosen tower, awaiting a second.
#[derive(Resource, Default)]
pub struct FusionSel {
    pub a: Option<bevy::ecs::entity::Entity>,
}

/// Base tower types the player can build this run. AC1 deals 2 random types
/// (>=1 output tower) at Startup; AC2/AC3/AC4 add more. Empty until dealt.
#[derive(Resource)]
pub struct Hand {
    pub owned_towers: Vec<usize>, // base tower types the player can use
}

impl Default for Hand {
    fn default() -> Self {
        Self {
            owned_towers: Vec::new(),
        }
    }
}

/// Per-type multipliers applied by the wave-intermission "三选一" boost (AC4).
/// Index = base tower type (0..3); fused towers gain no base boost (placeholder —
/// full per-tower stat boosts are a polish card).
#[derive(Resource)]
pub struct Boosts {
    pub damage_mult: [f32; 4],
    pub kill_mult: f32,
}

impl Default for Boosts {
    fn default() -> Self {
        Self {
            damage_mult: [1.0; 4],
            kill_mult: 1.0,
        }
    }
}

/// One "三选一" option, shown between waves (AC4).
#[derive(Clone, Debug)]
pub enum ChoiceKind {
    /// +20% damage to a base tower type.
    StatBoost { tower_type: usize },
    /// +20% kill gold for the rest of the run.
    GoldBoost,
    /// Add a base tower type to the owned pool (avoiding owned towers).
    GetTower { tower_type: usize },
}

#[derive(Clone)]
pub struct ChoiceOption {
    pub label: &'static str,
    pub kind: ChoiceKind,
}

/// A pending "三选一" (three-choose-one) between waves. While pending, the next
/// wave cannot start. Generated after each non-final wave clears (AC4).
#[derive(Resource)]
pub struct WaveChoice {
    pub options: Vec<ChoiceOption>,
    pub pending: bool,
}

impl Default for WaveChoice {
    fn default() -> Self {
        Self {
            options: Vec::new(),
            pending: false,
        }
    }
}
