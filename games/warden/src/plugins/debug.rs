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

/// Dash animation-probe memory: last tick's per-player elapsed times, and
/// the first bone's rotation snapshot (stage 5: is the pose actually being
/// written?).
#[derive(Resource, Default)]
pub struct AnimProbe(
    pub std::collections::HashMap<Entity, Vec<f32>>,
    pub std::collections::HashMap<Entity, [i32; 4]>,
);

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
    use crate::systems::range as rng;

    app.init_resource::<FpsLog>()
        .init_resource::<dbg::DebugQueue>()
        .init_resource::<dbg::DebugPanel>()
        .init_resource::<dbg::DebugSeed>()
        .init_resource::<dbg::DebugScenarioQueue>()
        .init_resource::<db3::DebugTime>()
        .init_resource::<db3::DebugOverlays>()
        // T-pose hunt: full animation-component lifecycle. Whoever replaces
        // players/graphs (the fusion repro: two placed archers die ~2min in,
        // players swapped for fresh graphless ones) lands here with entity
        // ids and timestamps.
        .add_observer(
            |trigger: On<Add, AnimationPlayer>,
             names: Query<&Name>,
             parents: Query<&ChildOf>| {
                let mut chain = Vec::new();
                let mut cur = trigger.entity;
                for _ in 0..5 {
                    if let Ok(n) = names.get(cur) {
                        chain.push(n.to_string());
                    }
                    match parents.get(cur) {
                        Ok(p) => cur = p.0,
                        Err(_) => break,
                    }
                }
                info!("[anim-life] +player {:?} {:?}", trigger.entity, chain);
            },
        )
        .add_observer(
            |trigger: On<Remove, AnimationPlayer>| {
                info!("[anim-life] -player {:?}", trigger.entity);
            },
        )
        .add_observer(
            |trigger: On<Add, AnimationGraphHandle>| {
                info!("[anim-life] +graph {:?}", trigger.entity);
            },
        )
        .add_observer(
            |trigger: On<Remove, AnimationGraphHandle>| {
                info!("[anim-life] -graph {:?}", trigger.entity);
            },
        )
        .add_plugins(bevy_egui::EguiPlugin::default())
        // SM1 (ADR-0010): the generic studio viewer — keys/present/sweep/
        // orbit/egui list all live in the engine crate; Warden's adapter
        // only registers the catalog on scenario entry. Added after
        // EguiPlugin so the list panel wires up.
        .add_plugins(showroom_viewer::ShowroomPlugin)
            // SM3R: range maintenance + ledger run whenever the range is
            // active (guards inside on RangeState existing).
            .add_systems(
                Update,
                (
                    rng::range_maintain,
                    rng::range_ledger,
                    rng::range_sweep,
                ),
            )
        .add_systems(
            bevy_egui::EguiPrimaryContextPass,
            (debug_panel_ui, rng::range_panel_ui),
        )
        .add_systems(PreUpdate, dbg::skill_card_cursor_pin)
        .add_systems(
            Update,
            (log_dashboard, screenshot_on_f12, dbg::skill_card_shots, dbg::debug_panel_toggle),
        )
        // SM4: the showroom baseline sweep (no-ops outside baseline mode).
        .add_systems(Update, crate::systems::showroom::showroom_baseline_sweep)
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
fn log_dashboard(world: &mut World, mut log: Local<FpsLog>) {
    log.acc += world.resource::<Time>().delta_secs();
    if log.acc < 2.0 {
        return;
    }
    log.acc = 0.0;
    {
        let fps = 1.0 / world.resource::<Time>().delta_secs().max(1e-6);
        let state = world.resource::<State<GameState>>().get().clone();
        let (phase, wave_n) = {
            let w = world.resource::<WaveState>();
            (w.phase, w.current + 1)
        };
        let gold = world.resource::<Economy>().gold;
        let (hp, max_hp) = {
            let b = world.resource::<BaseHp>();
            (b.hp, b.max_hp)
        };
        let count = world.query::<Entity>().iter(world).count();
        info!(
            "[dash] fps≈{fps:.0} state={state:?} phase={phase:?} gold={gold} base={hp}/{max_hp} wave={wave_n} entities={count}"
        );
    }
    // Observation channel for the ART1 model path (exclusive system: the
    // probe needs arbitrary-depth hierarchy walks).
    // Level 1: each WorldAssetRoot — loaded? instantiated?
    let mut roots: bevy::ecs::query::QueryState<(
        Entity,
        &bevy::world_serialization::WorldAssetRoot,
        Option<&Children>,
    )> = world.query();
    let root_ids: Vec<Entity> = roots.iter(world).map(|(e, _, _)| e).collect();
    for e in root_ids.clone() {
        let (handle, children) = {
            let Ok((_, root, children)) = roots.get(world, e) else {
                continue;
            };
            (root.0.clone(), children.map(|c| c.len()).unwrap_or(0))
        };
        let state_str = world
            .resource::<bevy::asset::AssetServer>()
            .get_load_state(&handle)
            .map(|s| format!("{s:?}"))
            .unwrap_or_else(|| "no-server".into());
        let in_store = world
            .resource::<bevy::asset::Assets<bevy::world_serialization::WorldAsset>>()
            .contains(handle.id());
        info!(
            "[dash] world-root {e:?} load={state_str} in_store={in_store} children={children}"
        );
    }
    // Animation probe (ART12 T-pose hunt): for every AnimationPlayer in the
    // world report whether it holds a graph and whether any animation's
    // elapsed time advances between consecutive dash ticks. frozen-elapsed
    // with a graph attached = the advance/apply chain is broken, not wiring.
    {
        let mut players: bevy::ecs::query::QueryState<(
            Entity,
            &AnimationPlayer,
            Option<&AnimationGraphHandle>,
        )> = world.query();
        let mut probe = world.remove_resource::<AnimProbe>().unwrap_or_default();
        // Snapshot first — the linkage audit below needs `world` mutably again.
        let snapshot: Vec<(Entity, Vec<(AnimationNodeIndex, f32)>, Option<AnimationGraphHandle>)> =
            players
                .iter(world)
                .map(|(e, player, graph)| {
                    let playing: Vec<(AnimationNodeIndex, f32)> = player
                        .playing_animations()
                        .map(|(idx, a)| (*idx, a.elapsed()))
                        .collect();
                    (e, playing, graph.cloned())
                })
                .collect();
        // Linkage audit (T-pose hunt stage 2): how many bones carry
        // AnimationTargetId, and do their AnimatedBy links point at each
        // player? Zero links = curves can never apply.
        let mut target_count: bevy::ecs::query::QueryState<
            Entity,
            With<bevy::animation::AnimationTargetId>,
        > = world.query_filtered::<Entity, With<bevy::animation::AnimationTargetId>>();
        let total_targets = target_count.iter(world).count();
        let mut links: bevy::ecs::query::QueryState<(Entity, &bevy::animation::AnimatedBy)> =
            world.query();
        let mut linked = std::collections::HashMap::new();
        for (_, l) in links.iter(world) {
            *linked.entry(l.0).or_insert(0usize) += 1;
        }
        // Bone-side target ids (stage 4): what ids do the bones CARRY?
        let mut bone_ids: bevy::ecs::query::QueryState<(
            Entity,
            &bevy::animation::AnimationTargetId,
            &bevy::animation::AnimatedBy,
        )> = world.query();
        let mut bone_id_set: std::collections::HashSet<(u64, u64)> = std::collections::HashSet::new();
        for (_, t, _) in bone_ids.iter(world) {
            bone_id_set.insert(t.0.as_u64_pair());
        }
        info!(
            "[dash] anim bones={} distinct_ids={}",
            bone_id_set.len(),
            bone_id_set.len(),
        );
        // Stage 5: do actual LIMB bones get written? (The query's "first"
        // bone is the near-static rig root — sample by name instead.)
        {
            let mut bones_tf: bevy::ecs::query::QueryState<(
                Entity,
                &Name,
                &Transform,
            )> = world.query();
            let wanted = ["hips", "lowerarm.l", "lowerarm.r", "upperarm.l"];
            for (e, name, tf) in bones_tf.iter(world) {
                let n = name.as_str().to_lowercase();
                if !wanted.contains(&n.as_str()) {
                    continue;
                }
                let key = [
                    (tf.rotation.x * 1000.0) as i32,
                    (tf.rotation.y * 1000.0) as i32,
                    (tf.rotation.z * 1000.0) as i32,
                    (tf.rotation.w * 1000.0) as i32,
                ];
                let changed = probe.1.insert(e, key).map(|old| old != key).unwrap_or(false);
                info!(
                    "[dash] bone {e:?} name={n} rot_y={:.4} changed={changed}",
                    tf.rotation.y,
                );
            }
        }
        for (e, playing, graph_handle) in snapshot {
            let elapsed: Vec<f32> = playing.iter().map(|(_, t)| *t).collect();
            // Name chain up from the player (who does this player belong to?).
            let mut chain: Vec<String> = Vec::new();
            let mut cur = e;
            for _ in 0..6 {
                match world.get::<Name>(cur) {
                    Some(n) => chain.push(n.as_str().to_string()),
                    None => chain.push(format!("?{}", cur.index())),
                }
                match world.get::<ChildOf>(cur) {
                    Some(p) => cur = p.0,
                    None => break,
                }
            }
            chain.reverse();
            let prev = probe.0.insert(e, elapsed.clone());
            let advancing = match prev {
                Some(p) if p.len() == elapsed.len() && !elapsed.is_empty() => {
                    p.iter().zip(&elapsed).any(|(a, b)| b > a)
                }
                _ => false,
            };
            let has_graph = graph_handle.is_some();
            let my_links = linked.get(&e).copied().unwrap_or(0);
            // Stage 3 (uuid collision audit): sample the playing clip's curve
            // target ids — if they miss the bones' AnimationTargetIds, the
            // player spins forever on a rig that never hears it (T-pose).
            let graphs_store = world.get_resource::<Assets<AnimationGraph>>();
            let clips_store = world.get_resource::<Assets<AnimationClip>>();
            let mut curve_ids: Vec<bevy::animation::AnimationTargetId> = Vec::new();
            let mut clip_info = String::new();
            let mut threaded_info = String::from("no-graph");
            if let (Some(graphs_store), Some(clips_store), Some(gh), Some((node_idx, _))) =
                (graphs_store, clips_store, graph_handle.as_ref(), playing.first())
            {
                if let Some(_tg) =
                    world.get_resource::<bevy::animation::graph::ThreadedAnimationGraphs>()
                {
                    // Field is pub(crate); presence-only probe (the trace
                    // log tells the rest).
                    threaded_info = "resource-present".to_string();
                }
                if let Some(g) = graphs_store.get(gh) {
                    if let Some(node) = g.get(*node_idx) {
                        if let AnimationNodeType::Clip(clip_handle) = &node.node_type {
                            let clip_handle = clip_handle.clone();
                            if let Some(c) = clips_store.get(&clip_handle) {
                                let total = c.curves().len();
                                let hits = c
                                    .curves()
                                    .keys()
                                    .filter(|k| bone_id_set.contains(&k.0.as_u64_pair()))
                                    .count();
                                clip_info =
                                    format!("curves={total} hitting_bones={hits}");
                                for k in c.curves().keys().take(3) {
                                    curve_ids.push(*k);
                                }
                            }
                        }
                    }
                }
            }
            info!(
                "[dash] anim {e:?} chain={chain:?} graph={has_graph} playing={} elapsed={elapsed:?} advancing={advancing} targets_in_world={total_targets} linked_to_me={my_links} {threaded_info} clip[{clip_info}] curve_sample={:?}",
                elapsed.len(),
                curve_ids
                    .iter()
                    .map(|c| c.0.as_u64_pair())
                    .collect::<Vec<_>>(),
            );
        }
        world.insert_resource(probe);
    }
    // Level 2+: recursive subtree dump under each root (depth-capped) —
    // B0004-pattern hunting: one broken Visibility or a zero-scale Transform
    // anywhere in the spawned scene leaves the meshes invisible.
    fn dump_subtree(world: &World, e: Entity, depth: usize, label: String) {
        if depth > 6 {
            return;
        }
        let vis = world
            .get::<Visibility>(e)
            .map(|v| format!("{v:?}"))
            .unwrap_or_else(|| "-".into());
        let inherited = world
            .get::<InheritedVisibility>(e)
            .map(|v| format!("{}", v.get()))
            .unwrap_or_else(|| "-".into());
        let mesh = world.get::<Mesh3d>(e).map(|_| "MESH".to_string()).unwrap_or_default();
        let tf = world.get::<Transform>(e).map(|t| {
            format!(
                "p({:.1},{:.1},{:.1}) s({:.2},{:.2},{:.2})",
                t.translation.x,
                t.translation.y,
                t.translation.z,
                t.scale.x,
                t.scale.y,
                t.scale.z
            )
        });
        info!(
            "[dash] {label} {e:?} vis={vis} inh={inherited} {mesh} tf={tf:?}"
        );
        if let Some(children) = world.get::<Children>(e) {
            for (i, c) in children.iter().enumerate() {
                dump_subtree(world, c, depth + 1, format!("{label}.{i}"));
            }
        }
    }
    for root in &root_ids {
        dump_subtree(world, *root, 0, "tree".into());
    }
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

        ui.heading("Skills (SK1)");
        if ui.button("Reset skill CDs").clicked() {
            queue.0.push(A::SkillResetCd);
        }
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
