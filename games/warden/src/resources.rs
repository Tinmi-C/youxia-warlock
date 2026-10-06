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

/// UI language (UI7). Default Chinese (UI2); toggled by the L key / the
/// top-right button and persisted across runs in `ui_prefs.txt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Resource)]
pub enum Lang {
    Zh,
    En,
}

impl Default for Lang {
    fn default() -> Self {
        Lang::Zh
    }
}

impl Lang {
    pub fn next(self) -> Self {
        match self {
            Lang::Zh => Lang::En,
            Lang::En => Lang::Zh,
        }
    }

    /// Serialization code for `ui_prefs.txt` ("zh" / "en").
    pub fn code(self) -> &'static str {
        match self {
            Lang::Zh => "zh",
            Lang::En => "en",
        }
    }
}

/// Where the UI preference file lives (UI7). Default `./ui_prefs.txt` (cwd =
/// project dir under `cargo run`); tests point it at temp files.
#[derive(Resource)]
pub struct UiPrefsPath(pub std::path::PathBuf);

impl Default for UiPrefsPath {
    fn default() -> Self {
        Self(std::path::PathBuf::from("ui_prefs.txt"))
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
    /// English identifier (logs / code); the UI shows `label`/`label_en` (UI2/UI7).
    pub name: &'static str,
    /// Player-facing display name (Chinese, default UI language per UI2).
    pub label: &'static str,
    /// Player-facing display name (English, UI7).
    pub label_en: &'static str,
    pub cost: u32,
    pub damage: f32,
    pub attack_speed: f32, // shots per second
    pub range: f32,        // world units
    pub attack_type: AttackType,
    /// Muzzle distance from the weapon mount along its forward axis (ART7,
    /// world units; starting value 0.5). Copied onto the mount's `AimTracker`
    /// at spawn; only towers that actually carry a mount read it.
    pub muzzle_offset: f32,
    /// ART8 recoil peak displacement along the mount's backward axis (world
    /// units; starting value 0.12). Read at spawn by the mount's `FireRecoil`.
    pub recoil_depth: f32,
    /// ART8 recoil recovery time in seconds (linear back to rest; 0.18).
    pub recoil_recover_secs: f32,
    /// One-line role summary for tooltips (requirements §5 定位 column).
    pub role: &'static str,
    /// English role line (UI12 tooltip).
    pub role_en: &'static str,
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
                TowerDef { name: "archer", label: "弓箭手塔", label_en: "Archer Tower", cost: 50, damage: 10.0, attack_speed: 1.0, range: 9.0, attack_type: AttackType::Physical, muzzle_offset: 0.5, recoil_depth: 0.12, recoil_recover_secs: 0.18,  role: "单体输出（点杀精英/BOSS/治疗）", role_en: "Single-target DPS (elites / bosses / healers)" },
                TowerDef { name: "shield", label: "盾塔", label_en: "Shield Tower", cost: 40, damage: 5.0, attack_speed: 1.2, range: 4.0, attack_type: AttackType::Physical, muzzle_offset: 0.5, recoil_depth: 0.12, recoil_recover_secs: 0.18,  role: "控制/减速（攻击附 30% 减速）", role_en: "Control / 30% slow on hit" },
                TowerDef { name: "mage", label: "法师塔", label_en: "Mage Tower", cost: 70, damage: 15.0, attack_speed: 0.5, range: 14.0, attack_type: AttackType::Magic, muzzle_offset: 0.5, recoil_depth: 0.12, recoil_recover_secs: 0.18,  role: "范围输出（魔法穿物理护甲）", role_en: "AOE (magic pierces armor)" },
                TowerDef { name: "cannon", label: "炮塔", label_en: "Cannon Tower", cost: 80, damage: 25.0, attack_speed: 0.33, range: 12.0, attack_type: AttackType::Physical, muzzle_offset: 0.5, recoil_depth: 0.12, recoil_recover_secs: 0.18,  role: "大范围清群（攻速慢是平衡点）", role_en: "Wide AOE clear (slow firing)" },
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
    /// English identifier (logs / code); the UI shows `label`/`label_en` (UI2/UI7).
    pub name: &'static str,
    /// Player-facing display name (Chinese).
    pub label: &'static str,
    /// Player-facing display name (English, UI7).
    pub label_en: &'static str,
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
                    name: "marksman", label: "神射手塔", label_en: "Marksman Tower", ingredients: [0, 0], fee_ratio: 0.2,
                    damage: 18.0, attack_speed: 1.2, range: 12.6,
                    attack_type: AttackType::Physical, kind: FusionKind::Marksman,
                    aoe_radius: 0.0, slow_factor: 0.0, slow_duration: 0.0,
                },
                // 大法师: 2×mage(2) — AOE blast. DPS 15 = 100% of 2×mage(15).
                FusionDef {
                    name: "archmage", label: "大法师塔", label_en: "Archmage Tower", ingredients: [2, 2], fee_ratio: 0.2,
                    damage: 25.0, attack_speed: 0.6, range: 16.0,
                    attack_type: AttackType::Magic, kind: FusionKind::Archmage,
                    aoe_radius: 3.0, slow_factor: 0.0, slow_duration: 0.0,
                },
                // 魔弓手: archer(0)+mage(2) — mixed damage bypasses armor. DPS 16.2 ≈ 93%.
                FusionDef {
                    name: "hybrid", label: "魔弓手塔", label_en: "Spellbow Tower", ingredients: [0, 2], fee_ratio: 0.2,
                    damage: 18.0, attack_speed: 0.9, range: 11.0,
                    attack_type: AttackType::Mixed, kind: FusionKind::Hybrid,
                    aoe_radius: 0.0, slow_factor: 0.0, slow_duration: 0.0,
                },
                // 壁垒炮: shield(1)+cannon(3) — AOE + 30% slow. DPS 14.0 ≈ 98%.
                FusionDef {
                    name: "bastion", label: "壁垒炮塔", label_en: "Bastion Tower", ingredients: [1, 3], fee_ratio: 0.2,
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
    /// Player-facing display name (Chinese, UI8 wave preview).
    pub label: &'static str,
    /// Player-facing display name (English, UI7/UI8).
    pub label_en: &'static str,
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
                EnemyDef { name: "ordinary", label: "普通", label_en: "Normal", hp: 30.0, speed: 1.0, leak: 1, kill_gold: 6, physical_armor: false, healer: false },
                // 1 fast
                EnemyDef { name: "fast", label: "快速", label_en: "Fast", hp: 20.0, speed: 1.8, leak: 1, kill_gold: 8, physical_armor: false, healer: false },
                // 2 shield (physical armor -50%)
                EnemyDef { name: "shield", label: "盾兵", label_en: "Shield", hp: 90.0, speed: 0.6, leak: 2, kill_gold: 12, physical_armor: true, healer: false },
                // 3 healer
                EnemyDef { name: "healer", label: "治疗", label_en: "Healer", hp: 45.0, speed: 1.0, leak: 1, kill_gold: 10, physical_armor: false, healer: true },
                // 4 elite
                EnemyDef { name: "elite", label: "精英", label_en: "Elite", hp: 200.0, speed: 0.8, leak: 3, kill_gold: 50, physical_armor: false, healer: false },
                // 5 boss
                EnemyDef { name: "boss", label: "BOSS", label_en: "Boss", hp: 600.0, speed: 0.5, leak: 5, kill_gold: 100, physical_armor: false, healer: false },
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

/// UI9 selection state (requirements §16.2/§16.3): what the bottom command
/// card is about. Left-click selects (tower or empty slot), a blank left
/// click / right click / Esc clears it. `FusionTarget(a)` means base tower
/// `a` is chosen and a fusion target click is awaited (§16.6).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    /// Idle: nothing selected; the command card shows the build list.
    None,
    /// An empty tower slot is selected: the build-on-this-slot entry (§16.4).
    EmptySlot(bevy::ecs::entity::Entity),
    /// A placed tower is selected: sell / fuse commands (§16.5/§16.6).
    Tower(bevy::ecs::entity::Entity),
    /// Fusion target picking: `a` awaits the confirming click on tower B.
    FusionTarget(bevy::ecs::entity::Entity),
}

impl Default for Selection {
    fn default() -> Self {
        Selection::None
    }
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
/// Index = base tower type (0..3); a fused tower reads the multiplier of its
/// first ingredient type (`tower_index`) so it too benefits (AC4 polish). The
/// +% values are starting points, tuned on the balance card.
#[derive(Resource)]
pub struct Boosts {
    pub damage_mult: [f32; 4],
    pub attack_speed_mult: [f32; 4],
    pub range_mult: [f32; 4],
    pub kill_mult: f32,
}

/// The between-waves shop offer (AC2): 3 base-tower types refreshed each
/// intermission; while the player does not own all four, at least one offer is
/// a type they do not own yet. `version` bumps on every refresh so the UI can
/// detect when to rebuild.
#[derive(Resource)]
pub struct ShopOffers {
    pub offers: Vec<usize>,
    pub version: u32,
}

impl Default for ShopOffers {
    fn default() -> Self {
        Self {
            offers: Vec::new(),
            version: 0,
        }
    }
}

/// Cross-run Meta progression (ME1, requirements §13): currency + one upgrade
/// chain. Persisted between runs to a tiny key=value file (starting design —
/// no serde dependency); a missing/corrupt file falls back to defaults.
#[derive(Resource)]
pub struct MetaState {
    pub coins: u32,
    /// 升级 1: the opening hand always contains the archer.
    pub upgrade1: bool,
    /// 升级 2: +20 starting gold next run (100 -> 120).
    pub upgrade2: bool,
}

impl Default for MetaState {
    fn default() -> Self {
        Self {
            coins: 0,
            upgrade1: false,
            upgrade2: false,
        }
    }
}

/// Where the meta save file lives. Default `./meta_save.txt` (cwd = project
/// dir under `cargo run`); tests point it at temp files.
#[derive(Resource)]
pub struct MetaSavePath(pub std::path::PathBuf);

impl Default for MetaSavePath {
    fn default() -> Self {
        Self(std::path::PathBuf::from("meta_save.txt"))
    }
}

/// ME1 upgrade costs (starting values — requirements §13 gives no numbers).
pub const UPGRADE1_COST: u32 = 30;
pub const UPGRADE2_COST: u32 = 60;
/// ME1 payouts (requirements §13): death +15, win +30.
pub const META_REWARD_DEATH: u32 = 15;
pub const META_REWARD_WIN: u32 = 30;
/// TO7 sell refund ratio (requirements §16.5): refund = invested × 0.7,
/// floored. Any sell→rebuy loop loses ≥30% — no arbitrage. Tune via §14.
pub const SELL_REFUND_RATIO: f32 = 0.7;
/// Projectile flight speed in world units per second (PR1, requirements §5.1):
/// slow enough to perceive, fast enough not to drag the pace. Tune via §14.
pub const PROJECTILE_SPEED: f32 = 10.0;

impl Default for Boosts {
    fn default() -> Self {
        Self {
            damage_mult: [1.0; 4],
            attack_speed_mult: [1.0; 4],
            range_mult: [1.0; 4],
            kill_mult: 1.0,
        }
    }
}

/// One "三选一" option, shown between waves (AC4).
#[derive(Clone, Debug)]
pub enum ChoiceKind {
    /// A stat boost to a base tower type (AC4 polish: chooses which stat).
    StatBoost { tower_type: usize, stat: StatKind },
    /// +20% kill gold for the rest of the run.
    GoldBoost,
    /// Add a base tower type to the owned pool (avoiding owned towers).
    GetTower { tower_type: usize },
}

/// Which combat stat a modifier/choice touches. Damage/AttackSpeed/Range
/// existed for the AC4 stat-boost choices; SK6 (ADR-0011) extends the shared
/// vocabulary with crit stats for the `StatMods` pipeline. AC4 itself never
/// offers crit (see input::apply_choice / l10n::choice_label arms).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatKind {
    Damage,
    AttackSpeed,
    Range,
    CritChance,
    CritMultiplier,
}

#[derive(Clone)]
pub struct ChoiceOption {
    /// Player-facing label (Chinese, UI2); built dynamically with tower labels.
    pub label: String,
    /// Player-facing label (English, UI7); kept in sync with `label`.
    pub label_en: String,
    pub kind: ChoiceKind,
}

/// Debug-run marker (DB1): set the first time any debug cheat / scenario is
/// used in a run. A marked run pays NO meta coins and never writes
/// `meta_save.txt` (test-run isolation — see `systems/meta.rs`). The flag is
/// inert data compiled in release too (nothing can set it there: all cheat
/// code is `#[cfg(debug_assertions)]`); it stays set across debug restarts
/// and only a fresh app launch clears it.
#[derive(Resource, Default)]
pub struct DebugRun {
    pub marked: bool,
}

/// Debug-panel input capture (DB1), written every frame by the debug panel
/// (debug builds only; release never writes it, so both flags stay false):
/// `pointer` = cursor is over the egui window (world picking must swallow the
/// click, same rule as `BlocksWorldInput`), `keyboard` = egui wants keyboard
/// input (typing in a panel field must not fire world hotkeys).
#[derive(Resource, Default)]
pub struct DebugPanelCapture {
    pub pointer: bool,
    pub keyboard: bool,
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

// --- Skills (phase 1, requirements §17 / cards SK1-SK4) -------------------------

/// The three commander skills (requirements §17.2). Since SK8 the runtime
/// home is the `SkillDefs` registry (the commander rows) and the SkillCaster
/// on the commander entity; the enum stays as the l10n/tooltip key.
/// INVARIANT: `SkillId::index()` == its registry def id (the commander rows
/// are `systems::skills::{SKILL_METEOR_RAIN, SKILL_FROST_NOVA, SKILL_WAR_FRENZY}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkillId {
    /// 流星火雨: 3 AOE blasts at a picked point (SK2).
    MeteorRain,
    /// 冰霜新星: slow in a radius at a picked point (SK3).
    FrostNova,
    /// 战意狂暴: instant global attack-speed burst (SK4).
    WarFrenzy,
}

impl SkillId {
    /// Definition order — also the Z/X/C button order (§17.3).
    pub const ALL: [SkillId; 3] = [SkillId::MeteorRain, SkillId::FrostNova, SkillId::WarFrenzy];

    pub fn index(self) -> usize {
        match self {
            SkillId::MeteorRain => 0,
            SkillId::FrostNova => 1,
            SkillId::WarFrenzy => 2,
        }
    }
}

/// Point-target picking active for a caster's slot (SK5 generic targeting —
/// the only targeting state since SK8 retired the phase-1 resource).
#[derive(Resource, Debug, Default)]
pub struct SkillAiming(pub Option<(Entity, usize)>);

/// SK4 war frenzy: while `remaining > 0`, every tower's fire cooldown is
/// `1 / (attack_speed × Boosts × frenzy_speed_mult)`. A GLOBAL timed state —
/// not a per-tower snapshot — so towers placed mid-frenzy benefit too
/// (requirements §17.2); fused towers read it like everyone (their Boosts
/// already come from `tower_index`). The numbers live in `SkillBalance`.
#[derive(Resource, Debug)]
pub struct WarFrenzy {
    pub remaining: f32,
}

impl Default for WarFrenzy {
    fn default() -> Self {
        Self { remaining: 0.0 }
    }
}

// --- Skill framework (phase 2, ADR-0011; cards SK5+) ------------------------------

/// How a skill is used — the WC3 taxonomy (ADR-0011).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillKind {
    /// Always on once learned; contributes stat modifiers, never casts.
    Passive,
    /// Cast on demand (button/hotkey), purely cooldown-gated (§17.1).
    Active,
    /// An Active that may also fire on its own when the owner's domain
    /// logic says so — e.g. the tower autocast system writes `CastSkill`
    /// when the tower has a target (SK9). The framework treats it exactly
    /// like Active; the auto-trigger lives with the owner.
    Autocast,
}

/// What the caster must aim before the cast resolves (ADR-0011). `Unit` is
/// reserved for future unit-targeted skills; no def routes through it yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillTargetMode {
    /// Fires as soon as the cast request validates — no aiming.
    None,
    /// A ground point: indicator ring + left-click confirm, Esc/right-click
    /// cancel (the SK1 point-skill interaction shape).
    Point,
    /// A specific entity target (reserved).
    Unit,
}

/// Framework metadata for one skill (ADR-0011). Structure only — every
/// NUMBER lives in [`SkillBalance`] (ADR-0008: data in code, single source).
/// `name` is the stable code id used for lookups and log lines.
pub struct SkillDef {
    pub name: &'static str,
    pub kind: SkillKind,
    pub target: SkillTargetMode,
    pub max_level: u8,
    pub badge: char,
    /// Global hotkey, if any (the commander's Z/X/C). Tower-skill hotkeys
    /// arrive with SK10 as selection-scoped keys.
    pub hotkey: Option<KeyCode>,
}

/// Registry of every skill definition, indexed by the `def` field of
/// [`crate::components::SkillSlot`] — the TowerDefs pattern generalized to
/// skills. Production rows land with their content cards (SK8 commander,
/// SK9 archer); tests may push their own rows.
#[derive(Resource, Default)]
pub struct SkillDefs(pub Vec<SkillDef>);

impl SkillDefs {
    pub fn get(&self, def: usize) -> &SkillDef {
        self.0.get(def).expect("skill def index in range")
    }
}

/// Single-source skill numbers (ADR-0011; SK11 gathered every scattered
/// skill constant into it — tune here and nowhere else, loop B of the
/// skill-design workflow). Per-skill rows parallel `SkillDefs`; each holds
/// per-level values (index level-1; levels beyond the array clamp to the
/// last entry). Missing rows/cells read as 0.0 (commander rows have no
/// crit entries).
#[derive(Resource, Default, Clone)]
pub struct SkillBalance {
    /// Full cast cooldown per (def, level).
    pub cooldowns: Vec<Vec<f32>>,
    /// Point-skill effect radius (also the aiming indicator's radius).
    pub radius: Vec<Vec<f32>>,
    /// 鹰眼: crit chance bonus (0..1).
    pub crit_chance: Vec<Vec<f32>>,
    /// 鹰眼: additive crit-multiplier bonus (base multiplier is 2.0).
    pub crit_multiplier_add: Vec<Vec<f32>>,
    /// 多重射击: arrows = distinct targets per cast.
    pub multishot_targets: Vec<Vec<f32>>,
    /// 火焰箭: flat damage bonus per normal attack while active.
    pub fire_arrow_bonus: Vec<Vec<f32>>,
    /// 火焰箭: buff duration in seconds.
    pub fire_arrow_duration: Vec<Vec<f32>>,
    /// 流星火雨: meteors per cast.
    pub meteor_count: Vec<Vec<f32>>,
    /// 流星火雨: seconds between drops.
    pub meteor_interval: Vec<Vec<f32>>,
    /// 流星火雨: damage per meteor.
    pub meteor_damage: Vec<Vec<f32>>,
    /// 冰霜新星: slow factor (the shared merge path takes the strongest).
    pub nova_slow_factor: f32,
    /// 冰霜新星: slow duration in seconds.
    pub nova_slow_duration: f32,
    /// 战意狂暴: global attack-speed multiplier while active.
    pub frenzy_speed_mult: f32,
    /// 战意狂暴: duration in seconds.
    pub frenzy_duration: f32,
    /// SK12 狙杀: damage multiplier over the aggregated normal-attack damage.
    pub deadeye_mult: Vec<Vec<f32>>,
    /// SK12 奥术爆轰: instant magic damage per enemy in radius.
    pub arcane_damage: Vec<Vec<f32>>,
    /// SK12 贯穿箭: damage per pierced enemy.
    pub pierce_damage: Vec<Vec<f32>>,
    /// SK12 贯穿箭: bolt flight length in world units.
    pub pierce_length: Vec<Vec<f32>>,
    /// SK12 贯穿箭: hit width around the flight path (also the aim ring).
    pub pierce_width: f32,
    /// SK12 震荡冲击: instant physical damage per enemy in radius.
    pub concussion_damage: Vec<Vec<f32>>,
    /// SK12 震荡冲击: slow factor (shared merge path).
    pub concussion_slow_factor: f32,
    /// SK12 震荡冲击: slow duration in seconds.
    pub concussion_slow_duration: f32,
    /// SK10 paid kit upgrade: cost by the kit's CURRENT (min) level.
    pub upgrade_costs: Vec<f32>,
}

impl SkillBalance {
    fn level_value(rows: &[Vec<f32>], def: usize, level: u8) -> f32 {
        rows.get(def)
            .and_then(|r| r.get(level.saturating_sub(1) as usize).or_else(|| r.last()))
            .copied()
            .unwrap_or(0.0)
    }

    /// Cooldowns PANIC on a missing row: every castable def must register
    /// one (a silent 0 would mean free casting).
    pub fn cooldown(&self, def: usize, level: u8) -> f32 {
        let per_level = self.cooldowns.get(def).expect("skill balance cooldown row");
        per_level
            .get(level.saturating_sub(1) as usize)
            .or_else(|| per_level.last())
            .copied()
            .expect("at least one cooldown level")
    }

    pub fn crit_chance(&self, def: usize, level: u8) -> f32 {
        Self::level_value(&self.crit_chance, def, level)
    }

    pub fn crit_multiplier_add(&self, def: usize, level: u8) -> f32 {
        Self::level_value(&self.crit_multiplier_add, def, level)
    }

    pub fn multishot_targets(&self, def: usize, level: u8) -> f32 {
        Self::level_value(&self.multishot_targets, def, level)
    }

    pub fn fire_arrow_bonus(&self, def: usize, level: u8) -> f32 {
        Self::level_value(&self.fire_arrow_bonus, def, level)
    }

    pub fn fire_arrow_duration(&self, def: usize, level: u8) -> f32 {
        Self::level_value(&self.fire_arrow_duration, def, level)
    }

    pub fn radius(&self, def: usize, level: u8) -> f32 {
        Self::level_value(&self.radius, def, level)
    }

    pub fn meteor_count(&self, def: usize, level: u8) -> u32 {
        Self::level_value(&self.meteor_count, def, level) as u32
    }

    pub fn meteor_interval(&self, def: usize, level: u8) -> f32 {
        Self::level_value(&self.meteor_interval, def, level)
    }

    pub fn meteor_damage(&self, def: usize, level: u8) -> f32 {
        Self::level_value(&self.meteor_damage, def, level)
    }

    /// SK10: cost to raise a kit at `min_level` one level (clamped to the
    /// last row — a maxed kit is gated by the caller, not the price).
    pub fn skill_upgrade_cost(&self, min_level: u8) -> u32 {
        self.upgrade_costs
            .get(min_level.saturating_sub(1) as usize)
            .or_else(|| self.upgrade_costs.last())
            .copied()
            .unwrap_or(0.0) as u32
    }

    pub fn deadeye_mult(&self, def: usize, level: u8) -> f32 {
        Self::level_value(&self.deadeye_mult, def, level)
    }

    pub fn arcane_damage(&self, def: usize, level: u8) -> f32 {
        Self::level_value(&self.arcane_damage, def, level)
    }

    pub fn pierce_damage(&self, def: usize, level: u8) -> f32 {
        Self::level_value(&self.pierce_damage, def, level)
    }

    pub fn pierce_length(&self, def: usize, level: u8) -> f32 {
        Self::level_value(&self.pierce_length, def, level)
    }

    pub fn concussion_damage(&self, def: usize, level: u8) -> f32 {
        Self::level_value(&self.concussion_damage, def, level)
    }
}
