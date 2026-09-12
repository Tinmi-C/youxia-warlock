//! Debug cheat core (DB1, capability card "调试面板 + 作弊核心").
//!
//! The whole module is `#[cfg(debug_assertions)]` (see `systems/mod.rs`):
//! release builds contain none of this. Design:
//!   - `DebugAction` = intent. The egui panel (plugins/debug.rs) and the CLI
//!     scenario loader (DB2) only *push* actions into `DebugQueue`.
//!   - `debug_cheat_executor` = the single drain point. It applies every
//!     action and marks the run as debug (`DebugRun`) — a marked run pays no
//!     meta coins and never writes `meta_save.txt` (test-run isolation).
//! Keeping the panel and the effects decoupled lets headless tests drive the
//! exact same cheat path without any egui/window.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::components::{Enemy, Projectile, Tower};
use crate::resources::{
    BaseHp, Boosts, BuildMode, DebugRun, Economy, EnemyDefs, Hand, MetaState, PathInfo, RunRng,
    Selection, ShopOffers, WaveChoice, WavePhase, WaveSchedule, WaveState,
};
use crate::states::GameState;
use crate::systems::acquisition::{deal_opening_hand_into, refresh_shop_offers};
use crate::systems::enemy::spawn_enemy;
use crate::systems::pointer::HoverGhost;

/// One debug cheat intent. Panel button / CLI / test all speak this.
#[derive(Debug, Clone, Copy)]
pub enum DebugAction {
    GoldAdd(u32),
    GoldSub(u32),
    GoldSet(u32),
    BaseHpSet(u32),
    /// Land in the build phase of wave `n` (1-based), auto-granting the
    /// rewards of every skipped wave so the economy stays real.
    JumpToWave(u32),
    /// Resolve the current wave immediately (reward + intermission happen
    /// through the normal `wave_resolve` path).
    ClearWave,
    SummonEnemy { def_index: usize, count: u32 },
    UnlockAllTowers,
    GrantTower(usize),
    InstantLose,
    InstantWin,
    /// One-click full run reset (fresh economy/hand/wave; replays the CLI
    /// seed when one was given). Keeps the debug mark.
    Restart,
}

/// Pending cheat intents; drained (emptied) by `debug_cheat_executor`.
#[derive(Resource, Default)]
pub struct DebugQueue(pub Vec<DebugAction>);

/// egui panel scratch state (F1, DB1). The numbers are editable fields; the
/// buttons beside them push the matching `DebugAction`.
#[derive(Resource)]
pub struct DebugPanel {
    pub open: bool,
    pub gold_set: i32,
    pub base_set: i32,
    pub jump_wave: i32,
    pub summon_def: usize,
    pub summon_count: i32,
}

impl Default for DebugPanel {
    fn default() -> Self {
        Self {
            open: false,
            gold_set: 100,
            base_set: 10,
            jump_wave: 1,
            summon_def: 0,
            summon_count: 1,
        }
    }
}

/// The RNG seed given on the CLI (`--seed N`, DB2). `None` = no seed; the
/// debug restart replays the sequence only when a seed exists.
#[derive(Resource, Default)]
pub struct DebugSeed(pub Option<u64>);

/// F1 toggles the debug panel (DB1). Also clears the input-capture flags on
/// close so world input resumes immediately.
pub fn debug_panel_toggle(
    keys: Res<ButtonInput<KeyCode>>,
    mut panel: ResMut<DebugPanel>,
    mut capture: ResMut<crate::resources::DebugPanelCapture>,
) {
    if keys.just_pressed(KeyCode::F1) {
        panel.open = !panel.open;
        if !panel.open {
            capture.pointer = false;
            capture.keyboard = false;
        }
        info!("[debug] panel {}", if panel.open { "open" } else { "closed" });
    }
}

// --- The executor --------------------------------------------------------------
//
// Two `SystemParam` bundles keep the single executor under Bevy's 16-param
// system cap while every action still drains in ONE system (ordering + a
// single debug-mark write).

/// Run-wide resources the cheats touch (economy / wave bookkeeping / state).
#[derive(SystemParam)]
pub struct CheatsCore<'w, 's> {
    pub commands: Commands<'w, 's>,
    pub queue: ResMut<'w, DebugQueue>,
    pub run: ResMut<'w, DebugRun>,
    pub economy: ResMut<'w, Economy>,
    pub base: ResMut<'w, BaseHp>,
    pub wave: ResMut<'w, WaveState>,
    pub schedule: Res<'w, WaveSchedule>,
    pub hand: ResMut<'w, Hand>,
    pub next: ResMut<'w, NextState<GameState>>,
    pub state: Res<'w, State<GameState>>,
}

/// Board-scoped resources (spawning, restart resets, the entities to clear).
#[derive(SystemParam)]
pub struct CheatsBoard<'w, 's> {
    pub defs: Res<'w, EnemyDefs>,
    pub path: Res<'w, PathInfo>,
    pub meshes: ResMut<'w, Assets<Mesh>>,
    pub materials: ResMut<'w, Assets<StandardMaterial>>,
    pub rng: ResMut<'w, RunRng>,
    pub meta: Res<'w, MetaState>,
    pub boosts: ResMut<'w, Boosts>,
    pub shop: ResMut<'w, ShopOffers>,
    pub choice: ResMut<'w, WaveChoice>,
    pub selection: ResMut<'w, Selection>,
    pub build: ResMut<'w, BuildMode>,
    pub seed: Res<'w, DebugSeed>,
    /// Enemies + in-flight projectiles (cleared on wave jumps / restart).
    pub mobs: Query<'w, 's, Entity, Or<(With<Enemy>, With<Projectile>)>>,
    /// Placed towers (cleared only on restart).
    pub towers: Query<'w, 's, Entity, With<Tower>>,
    /// The build-mode ghost is hidden (not despawned): its owner system keeps
    /// the entity id in a Local cache and would panic on a stale insert.
    pub ghosts: Query<'w, 's, Entity, With<HoverGhost>>,
}

/// Single drain point for every debug action (DB1). Marks the run as debug on
/// the first action: no meta payout, no save write for this run.
pub fn debug_cheat_executor(mut core: CheatsCore, mut board: CheatsBoard) {
    if core.queue.0.is_empty() {
        return;
    }
    let actions = std::mem::take(&mut core.queue.0);
    if !core.run.marked {
        core.run.marked = true;
        info!("[debug] run marked as debug: meta payout/save disabled");
    }
    for action in actions {
        match action {
            DebugAction::GoldAdd(n) => {
                core.economy.gold = core.economy.gold.saturating_add(n);
                info!("[debug] gold +{n} -> {}", core.economy.gold);
            }
            DebugAction::GoldSub(n) => {
                core.economy.gold = core.economy.gold.saturating_sub(n);
                info!("[debug] gold -{n} -> {}", core.economy.gold);
            }
            DebugAction::GoldSet(n) => {
                core.economy.gold = n;
                info!("[debug] gold = {n}");
            }
            DebugAction::BaseHpSet(n) => {
                if n > core.base.max_hp {
                    core.base.max_hp = n;
                }
                core.base.hp = n;
                info!("[debug] base hp = {n}");
            }
            DebugAction::JumpToWave(n) => jump_to_wave(&mut core, &mut board, n),
            DebugAction::ClearWave => clear_wave(&mut core, &mut board),
            DebugAction::SummonEnemy { def_index, count } => {
                for _ in 0..count {
                    spawn_enemy(
                        &mut core.commands,
                        &board.defs,
                        &board.path,
                        def_index,
                        &mut board.meshes,
                        &mut board.materials,
                    );
                    core.wave.active += 1;
                }
                // Summons force combat so the wave machinery tracks them
                // (and resolves the wave once they are all gone).
                core.wave.phase = WavePhase::Combat;
                info!(
                    "[debug] summoned def {def_index} x{count} at the entry (active={})",
                    core.wave.active
                );
            }
            DebugAction::UnlockAllTowers => {
                core.hand.owned_towers = (0..4).collect();
                info!("[debug] unlocked all tower types");
            }
            DebugAction::GrantTower(t) => {
                if !core.hand.owned_towers.contains(&t) {
                    core.hand.owned_towers.push(t);
                }
                info!("[debug] granted tower type {t} (hand {:?})", core.hand.owned_towers);
            }
            DebugAction::InstantLose => {
                core.base.hp = 0;
                if *core.state.get() == GameState::Playing {
                    core.next.set(GameState::GameOver);
                }
                info!("[debug] instant lose");
            }
            DebugAction::InstantWin => {
                if *core.state.get() == GameState::Playing {
                    core.next.set(GameState::Win);
                }
                info!("[debug] instant win");
            }
            DebugAction::Restart => apply_restart(&mut core, &mut board),
        }
    }
}

/// Jump to the build phase of wave `n` (1-based). Rewards of every skipped
/// wave are auto-granted (economy stays real); skipping backwards grants
/// nothing (the already-paid rewards are simply kept). Live enemies and
/// projectiles are cleared; skipped 三选一 offers are dismissed (they are
/// skipped, not banked — deterministic for seed replay).
fn jump_to_wave(core: &mut CheatsCore, board: &mut CheatsBoard, n: u32) {
    let len = core.schedule.waves.len().max(1) as u32;
    let target = n.clamp(1, len) as usize; // 1-based wave number
    let target_idx = target - 1;
    let start_idx = core.wave.current as usize; // 0-based index of the current wave
    let mut granted = 0;
    if target_idx > start_idx {
        for w in &core.schedule.waves[start_idx..target_idx] {
            granted += w.reward;
        }
        core.economy.gold += granted;
    }
    for e in &board.mobs {
        core.commands.entity(e).despawn();
    }
    hide_ghosts(core, board);
    core.wave.current = target_idx as u32;
    core.wave.phase = WavePhase::Intermission;
    core.wave.spawn_queue.clear();
    core.wave.active = 0;
    core.wave.spawn_timer = 0.0;
    board.choice.pending = false;
    *board.selection = Selection::None;
    board.build.armed = false;
    info!(
        "[debug] jumped to wave {target} build phase (skipped rewards +{granted}, gold={})",
        core.economy.gold
    );
}

/// Clear the current wave: despawn everything live, drain the spawn queue and
/// zero the active counter. The wave then settles through the normal
/// `wave_resolve` (reward + intermission + 三选一) in this frame's Cleanup.
fn clear_wave(core: &mut CheatsCore, board: &mut CheatsBoard) {
    for e in &board.mobs {
        core.commands.entity(e).despawn();
    }
    core.wave.spawn_queue.clear();
    core.wave.active = 0;
    info!("[debug] current wave force-cleared (settles via wave_resolve)");
}

/// Full run reset (DB1 "快速重开"): fresh economy (meta upgrade 2 re-applied),
/// base hp, wave, boosts, hand (re-dealt via the real AC1 logic, upgrade 1
/// included), shop offer, selection/build mode; board entities despawned;
/// state back to Playing. With a CLI seed the RNG is re-seeded so the run
/// replays the exact same sequence. The debug mark intentionally survives —
/// only a fresh app launch gives a clean run again.
fn apply_restart(core: &mut CheatsCore, board: &mut CheatsBoard) {
    for e in &board.mobs {
        core.commands.entity(e).despawn();
    }
    for e in &board.towers {
        core.commands.entity(e).despawn();
    }
    hide_ghosts(core, board);

    *core.economy = Economy::default();
    if board.meta.upgrade2 {
        core.economy.gold += 20; // same run-start effect as apply_meta_on_run_start
    }
    *core.base = BaseHp::default();
    *core.wave = WaveState::default();
    *board.boosts = Boosts::default();
    *board.choice = WaveChoice::default();
    *board.selection = Selection::None;
    board.build.armed = false;

    if let Some(s) = board.seed.0 {
        *board.rng = RunRng::seeded(s);
    }
    core.hand.owned_towers.clear();
    deal_opening_hand_into(&mut core.hand, &mut board.rng, &board.meta);
    refresh_shop_offers(&mut board.shop, &core.hand, &mut board.rng);

    core.next.set(GameState::Playing);
    info!(
        "[debug] run restarted (gold={}, hand={:?}); debug mark kept",
        core.economy.gold, core.hand.owned_towers
    );
}

/// Hide (not despawn — see `CheatsBoard::ghosts`) any build-mode ghost.
fn hide_ghosts(core: &mut CheatsCore, board: &mut CheatsBoard) {
    for e in &board.ghosts {
        core.commands.entity(e).insert(Visibility::Hidden);
    }
}
