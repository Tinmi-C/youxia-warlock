//! DebugPlugin: observation channel + F1 debug panel (DB1).
//!   - Log dashboard every 2s: fps / state / phase / gold / base / entities
//!     (RUST_LOG=info) — works headless.
//!   - F12 screenshot to ./screenshot.png (acceptance evidence).
//!   - F1 egui panel with the cheat buttons (DB1) / scenario buttons (DB2).
//!
//! Everything is `#[cfg(debug_assertions)]`: release builds register *nothing*
//! here — no dashboard, no screenshot key, no egui, no cheats. The bevy_egui
//! dependency cannot be profile-gated by cargo (see Cargo.toml), so the gate
//! is at the code level: the debug module is cfg'd out and any leak from a
//! non-gated path fails the release compile outright.

use bevy::prelude::*;

#[cfg(debug_assertions)]
use bevy::render::view::screenshot::{save_to_disk, Screenshot};

#[cfg(debug_assertions)]
use crate::resources::{BaseHp, DebugPanelCapture, Economy, Hand, WaveState};
#[cfg(debug_assertions)]
use crate::states::GameState;

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(debug_assertions)]
        build_debug(app);
        #[cfg(not(debug_assertions))]
        {
            // Release: zero debug surface (the compiler proves the branch —
            // see the module docs and Cargo.toml).
            let _ = app;
        }
    }
}

#[cfg(debug_assertions)]
fn build_debug(app: &mut App) {
    use crate::sets::GameSet;
    use crate::systems::debug as dbg;
    use crate::systems::debug_time as db3;

    app.init_resource::<FpsLog>()
        .init_resource::<dbg::DebugQueue>()
        .init_resource::<dbg::DebugPanel>()
        .init_resource::<dbg::DebugSeed>()
        .init_resource::<dbg::DebugScenarioQueue>()
        .init_resource::<db3::DebugTime>()
        .init_resource::<db3::DebugOverlays>()
        .add_plugins(bevy_egui::EguiPlugin::default())
        .add_systems(bevy_egui::EguiPrimaryContextPass, debug_panel_ui)
        .add_systems(Update, (log_dashboard, screenshot_on_f12, dbg::debug_panel_toggle))
        // The single cheat drain point runs in Input so the same frame's
        // wave/combat systems observe the settled cheat state; scenario loads
        // (DB2) run right after it (restart + scenario in one frame stays
        // deterministic). DB3 hotkeys push the same actions, so they run
        // before the executor; the time clock runs right after both — it is
        // the single writer of Time<Virtual> and stays ungated by game state
        // (time-level pause, orthogonal to GameState::Paused).
        .add_systems(
            Update,
            (
                db3::debug_time_hotkeys.before(dbg::debug_cheat_executor),
                dbg::debug_cheat_executor,
                dbg::debug_scenario_executor.after(dbg::debug_cheat_executor),
                db3::debug_time_clock.after(dbg::debug_scenario_executor),
            )
                .in_set(GameSet::Input),
        )
        // DB3 info overlays + the always-on debug HUD strip.
        .add_systems(
            Update,
            (
                db3::debug_range_rings,
                db3::debug_enemy_stats,
                db3::debug_hud_strip,
            )
                .in_set(GameSet::Observe),
        );
}

// --- Observation channel (debug-only since DB1: the whole plugin surface,
// dashboard + screenshot key included, sits behind the same gate) ----------

#[cfg(debug_assertions)]
#[derive(Resource, Default)]
struct FpsLog {
    acc: f32,
}

#[cfg(debug_assertions)]
fn log_dashboard(
    time: Res<Time>,
    state: Res<State<GameState>>,
    economy: Res<Economy>,
    base: Res<BaseHp>,
    wave: Res<WaveState>,
    entities: Query<Entity>,
    mut log: ResMut<FpsLog>,
) {
    log.acc += time.delta_secs();
    if log.acc < 2.0 {
        return;
    }
    log.acc = 0.0;
    info!(
        "[dash] fps≈{:.0} state={:?} phase={:?} gold={} base={}/{} wave={} entities={}",
        1.0 / time.delta_secs().max(1e-6),
        state.get(),
        wave.phase,
        economy.gold,
        base.hp,
        base.max_hp,
        wave.current + 1,
        entities.iter().count()
    );
}

#[cfg(debug_assertions)]
fn screenshot_on_f12(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands) {
    if keys.just_pressed(KeyCode::F12) {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk("screenshot.png"));
        info!("[dash] screenshot saved to screenshot.png");
    }
}

// --- DB1: the F1 cheat panel ---------------------------------------------------

/// The egui crate re-exported by bevy_egui (panel widgets).
#[cfg(debug_assertions)]
use bevy_egui::egui;

/// The F1 egui panel (DB1/DB2). Pure presentation: every button pushes a
/// `DebugAction` (cheats) or a `Scenario` (one-shot test scenes) into the
/// matching queue; the executors do the work — same code path as the CLI.
/// Also maintains `DebugPanelCapture` so clicks/keys aimed at the panel never
/// fall through into world picking / world hotkeys.
#[cfg(debug_assertions)]
#[allow(clippy::too_many_arguments)]
fn debug_panel_ui(
    mut contexts: bevy_egui::EguiContexts,
    mut panel: ResMut<crate::systems::debug::DebugPanel>,
    mut capture: ResMut<DebugPanelCapture>,
    mut queue: ResMut<crate::systems::debug::DebugQueue>,
    mut scenarios: ResMut<crate::systems::debug::DebugScenarioQueue>,
    time: Res<crate::systems::debug_time::DebugTime>,
    overlays: Res<crate::systems::debug_time::DebugOverlays>,
    economy: Res<Economy>,
    base: Res<BaseHp>,
    wave: Res<WaveState>,
    hand: Res<Hand>,
    enemy_defs: Res<crate::resources::EnemyDefs>,
    tower_defs: Res<crate::resources::TowerDefs>,
    state: Res<State<GameState>>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        capture.pointer = false;
        capture.keyboard = false;
        return;
    };
    capture.pointer = panel.open && ctx.is_pointer_over_egui();
    capture.keyboard = panel.open && ctx.egui_wants_keyboard_input();
    if !panel.open {
        return;
    }

    use crate::systems::debug::DebugAction as A;
    egui::Window::new("Warden Debug (F1)").show(ctx, |ui| {
        ui.label(format!(
            "state={:?} wave={} ({:?}) gold={} base={}/{} hand={:?}",
            state.get(),
            wave.current + 1,
            wave.phase,
            economy.gold,
            base.hp,
            base.max_hp,
            hand.owned_towers
        ));
        ui.separator();

        ui.heading("Economy");
        ui.horizontal(|ui| {
            if ui.button("+100 gold").clicked() {
                queue.0.push(A::GoldAdd(100));
            }
            if ui.button("-100 gold").clicked() {
                queue.0.push(A::GoldSub(100));
            }
        });
        ui.horizontal(|ui| {
            ui.label("gold =");
            ui.add(egui::DragValue::new(&mut panel.gold_set).range(0..=999_999));
            if ui.button("Set").clicked() {
                queue.0.push(A::GoldSet(panel.gold_set.max(0) as u32));
            }
        });
        ui.horizontal(|ui| {
            ui.label("base hp =");
            ui.add(egui::DragValue::new(&mut panel.base_set).range(0..=99));
            if ui.button("Set").clicked() {
                queue.0.push(A::BaseHpSet(panel.base_set.max(0) as u32));
            }
        });
        ui.separator();

        ui.heading("Waves");
        ui.horizontal(|ui| {
            ui.label("jump to wave");
            ui.add(egui::DragValue::new(&mut panel.jump_wave).range(1..=10));
            if ui.button("Jump (build phase)").clicked() {
                queue.0.push(A::JumpToWave(panel.jump_wave.max(1) as u32));
            }
        });
        if ui.button("Clear current wave").clicked() {
            queue.0.push(A::ClearWave);
        }
        ui.horizontal(|ui| {
            egui::ComboBox::from_label("Summon")
                .selected_text(
                    enemy_defs
                        .list
                        .get(panel.summon_def)
                        .map(|d| d.name)
                        .unwrap_or("?"),
                )
                .show_ui(ui, |ui| {
                    for (i, d) in enemy_defs.list.iter().enumerate() {
                        ui.selectable_value(&mut panel.summon_def, i, d.name);
                    }
                });
            ui.add(egui::DragValue::new(&mut panel.summon_count).range(1..=50));
            if ui.button("Summon x N").clicked() {
                queue.0.push(A::SummonEnemy {
                    def_index: panel.summon_def,
                    count: panel.summon_count.max(1) as u32,
                });
            }
        });
        ui.separator();

        ui.heading("Acquisition");
        if ui.button("Unlock all tower types").clicked() {
            queue.0.push(A::UnlockAllTowers);
        }
        ui.horizontal(|ui| {
            for (i, d) in tower_defs.list.iter().enumerate() {
                if ui.button(format!("+{}", d.name)).clicked() {
                    queue.0.push(A::GrantTower(i));
                }
            }
        });
        ui.separator();
        ui.heading("Scenarios");
        ui.horizontal(|ui| {
            for (s, name) in crate::systems::debug::Scenario::ALL {
                if ui.button(name).clicked() {
                    scenarios.0.push(s);
                }
            }
        });
        ui.separator();

        ui.heading("Time (DB3)");
        ui.label(format!(
            "clock: {} (steps land on the frame AFTER the press)",
            time.status()
        ));
        ui.horizontal(|ui| {
            if ui
                .button(if time.paused { "Resume [F6]" } else { "Pause [F6]" })
                .clicked()
            {
                queue.0.push(A::TimePauseToggle);
            }
            if ui.button("Step 1 frame [F7]").clicked() {
                queue.0.push(A::TimeStepFrame);
            }
            if ui.button("Step 0.1s [F8]").clicked() {
                queue.0.push(A::TimeStepTenths);
            }
            if ui
                .button(format!(
                    "Speed: {} [F9]",
                    if time.speed_4x { "4x" } else { "1x" }
                ))
                .clicked()
            {
                queue.0.push(A::TimeSpeedToggle4x);
            }
        });
        ui.separator();

        ui.heading("Overlays (DB3)");
        ui.horizontal(|ui| {
            if ui
                .button(format!(
                    "Tower range rings: {}",
                    if overlays.show_ranges { "ON" } else { "OFF" }
                ))
                .clicked()
            {
                queue.0.push(A::OverlaysToggleRanges);
            }
            if ui
                .button(format!(
                    "Enemy stat plates: {}",
                    if overlays.show_enemy_stats { "ON" } else { "OFF" }
                ))
                .clicked()
            {
                queue.0.push(A::OverlaysToggleEnemyStats);
            }
        });
        ui.separator();

        ui.heading("Endings");
        ui.horizontal(|ui| {
            if ui.button("Instant lose").clicked() {
                queue.0.push(A::InstantLose);
            }
            if ui.button("Instant win").clicked() {
                queue.0.push(A::InstantWin);
            }
            if ui.button("Restart run").clicked() {
                queue.0.push(A::Restart);
            }
        });
        ui.separator();
        ui.label("Any cheat or scenario marks this run: no meta payout, no meta_save.txt write.");
    });
}
