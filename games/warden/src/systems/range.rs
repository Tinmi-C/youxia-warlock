//! SM3R: the debug shooting range — one real attacker (a tower through the
//! real spawn/fire/impact pipeline) versus a row of stationary dummies,
//! with a Dota-Target-Dummy style damage ledger. See the N6 requirement in
//! docs/showroom-v3-proposal.md: the tool never fakes combat logic.
//!
//! Enter via DB2 `Scenario::Range` (--scenario range or the F1 button).
//! Runs in Playing state (the REAL gameplay systems live — that is the
//! point), but on a CLEAN STAGE like the showroom studio: the battlefield
//! is swept away, every game UI root is hidden, and a neutral floor/light
//! rig replaces the arena — no command bar, no HUD, no wave text. Dummies
//! carry `RangeDummy` and are excluded from path movement.

use bevy::prelude::*;
use showroom_viewer::{ShowroomFloor, ShowroomKept, ShowroomLight};

use crate::components::{Enemy, Projectile, Tower};
use crate::resources::{EnemyDefs, PathInfo, TowerDefs};
use crate::systems::enemy::{RangeDummy, path_units_per_point, spawn_enemy_at};
use crate::systems::tower::spawn_base_tower;

/// Range configuration + live bookkeeping (panel writes, maintenance reads).
#[derive(Resource)]
pub struct RangeState {
    /// Attacker tower def index (0..4 base towers).
    pub attacker: usize,
    /// Dummy enemy def index.
    pub dummy: usize,
    /// Dummy count.
    pub count: u32,
    /// Attacker world position (fixed pad spot).
    pub origin: Vec3,
    /// True => maintenance performs a full reset this frame (respawn
    /// attacker + dummies at current settings, zero the ledger).
    pub reset: bool,
}

/// The dummy ledger (Dota Target Dummy style): total damage, DPS, hits and
/// the last damage instance, polled from the dummies' real hp.
#[derive(Resource, Default)]
pub struct RangeLedger {
    pub total: f32,
    pub hits: u32,
    pub last: f32,
    pub elapsed: f32,
    /// Previous-frame hp per dummy (hp polling — no logic duplication).
    last_hp: Vec<(Entity, f32)>,
}

/// Attacker pad: open ground, mid-board.
const RANGE_ORIGIN: Vec3 = Vec3::new(-4.0, 0.0, 2.0);
/// Dummies stand this far from the attacker (inside every tower's range —
/// the shortest is the shield tower's 4).
const DUMMY_DIST: f32 = 3.2;
/// Lateral spacing of the dummy row.
const DUMMY_GAP: f32 = 1.6;

fn dummy_pos(origin: Vec3, n: u32, count: u32) -> Vec3 {
    let offset = (n as f32 - (count as f32 - 1.0) / 2.0) * DUMMY_GAP;
    Vec3::new(origin.x + DUMMY_DIST, 0.0, origin.z + offset)
}

/// Enter the range (Scenario::Range): build the clean stage (sweep the
/// battlefield, hide the game UI, neutral floor/light — same palette as the
/// showroom studio), then insert the range state and request the initial
/// reset (maintenance spawns the attacker + dummies). Gameplay systems
/// keep running: they are gated on the Playing STATE, not on the map.
pub fn enter_range(world: &mut World) {
    // 1) One-shot sweep of the battlefield visuals (mesh roots, async scene
    //    roots, lights) — the resident sweep below catches late stragglers.
    {
        let doomed: Vec<Entity> = {
            let mut q = world.query_filtered::<
                Entity,
                (
                    Or<(
                        With<Mesh3d>,
                        With<bevy::world_serialization::WorldAssetRoot>,
                        With<DirectionalLight>,
                    )>,
                    Without<Enemy>,
                    Without<Tower>,
                    Without<Projectile>,
                    Without<Camera3d>,
                    Without<ShowroomKept>,
                ),
            >();
            q.iter(world).collect()
        };
        for e in doomed {
            world.despawn(e);
        }
    }
    // 2) Hide every game UI root (command bar, HUD, wave preview, skills)
    //    — the range panel is egui and stays.
    {
        let mut q = world.query_filtered::<Entity, (With<Node>, Without<ChildOf>)>();
        let roots: Vec<Entity> = q.iter(world).collect();
        for e in roots {
            if let Some(mut vis) = world.get_mut::<Visibility>(e) {
                *vis = Visibility::Hidden;
            }
        }
    }
    // Hover ghosts must survive the resident sweep (UI3 caches ids).
    {
        let mut q =
            world.query_filtered::<Entity, With<crate::systems::pointer::HoverGhost>>();
        let ghosts: Vec<Entity> = q.iter(world).collect();
        for e in ghosts {
            world.entity_mut(e).insert(ShowroomKept);
            if let Some(mut vis) = world.get_mut::<Visibility>(e) {
                *vis = Visibility::Hidden;
            }
        }
    }
    // 3) Neutral stage: same palette as the showroom studio.
    world.insert_resource(ClearColor(Color::srgb(0.78, 0.78, 0.82)));
    world.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.95, 0.95, 1.0),
        brightness: 120.0,
        ..default()
    });
    let floor_mesh = world
        .resource_mut::<Assets<Mesh>>()
        .add(Plane3d::new(Vec3::Y, Vec2::new(30.0, 30.0)));
    let floor_mat = world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color: Color::srgb(0.62, 0.62, 0.66),
            perceptual_roughness: 1.0,
            ..default()
        });
    world.spawn((
        ShowroomFloor,
        Mesh3d(floor_mesh),
        MeshMaterial3d(floor_mat),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));
    world.spawn((
        ShowroomLight,
        Transform::from_rotation(Quat::from_euler(EulerRot::ZYX, 0.0, 0.7, -0.9)),
        DirectionalLight {
            color: Color::srgb(1.0, 0.98, 0.95),
            illuminance: 16_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
    ));
    // 4) Range state; maintenance spawns attacker + dummies AFTER the sweep
    //    (deferred commands), so they survive it.
    world.insert_resource(RangeLedger::default());
    // Placeholder until the first maintenance reset spawns the real tower.
    world.insert_resource(RangeTower(Entity::PLACEHOLDER));
    world.insert_resource(RangeState {
        attacker: 0,
        dummy: 0,
        count: 3,
        origin: RANGE_ORIGIN,
        reset: true,
    });
    info!("[sm3r] range entered on a clean stage (attacker archer · 3 ordinary dummies)");
}

/// Resident sweep: while the range is active, any battlefield visual that
/// materializes late (async props/structures) is removed the frame it
/// surfaces. Combat entities (towers/enemies/projectiles), the stage
/// floor/light and the camera are exempt.
pub fn range_sweep(
    state: Option<Res<RangeState>>,
    mut commands: Commands,
    strays: Query<
        Entity,
        (
            Or<(
                With<Mesh3d>,
                With<bevy::world_serialization::WorldAssetRoot>,
                With<DirectionalLight>,
            )>,
            Without<ChildOf>,
            Without<Camera3d>,
            Without<Enemy>,
            Without<Tower>,
            Without<Projectile>,
            Without<ShowroomKept>,
            Without<ShowroomFloor>,
            Without<ShowroomLight>,
        ),
    >,
) {
    if state.is_none() {
        return;
    }
    for e in &strays {
        commands.entity(e).despawn();
    }
}

/// The live attacker entity (set by maintenance on every reset).
#[derive(Resource)]
pub struct RangeTower(pub Entity);

/// Maintenance: performs requested resets (respawn attacker at the current
/// def + fresh dummies, zero the ledger) and keeps the dummy row filled as
/// they die (real death path: gold + despawn, then a fresh dummy).
#[allow(clippy::too_many_arguments)]
pub fn range_maintain(
    mut commands: Commands,
    state: Option<ResMut<RangeState>>,
    tower: Option<ResMut<RangeTower>>,
    ledger: Option<ResMut<RangeLedger>>,
    towers: Query<Entity, With<Tower>>,
    dummies: Query<(Entity, &Transform), With<RangeDummy>>,
    tower_defs: Res<TowerDefs>,
    enemy_defs: Res<EnemyDefs>,
    path: Res<PathInfo>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    server: Option<Res<AssetServer>>,
) {
    let (Some(mut state), Some(mut tower), Some(mut ledger)) = (state, tower, ledger) else {
        return; // range not active
    };
    if state.reset {
        state.reset = false;
        for e in &towers {
            commands.entity(e).despawn();
        }
        for (e, _) in &dummies {
            commands.entity(e).despawn();
        }
        let def = tower_defs
            .list
            .get(state.attacker)
            .unwrap_or(&tower_defs.list[0]);
        *tower = RangeTower(spawn_base_tower(
            &mut commands,
            def,
            state.attacker.min(tower_defs.list.len() - 1),
            state.origin,
            &mut meshes,
            &mut materials,
            server.as_deref(),
        ));
        *ledger = RangeLedger::default();
        state.count = state.count.clamp(1, 6);
        for n in 0..state.count {
            spawn_dummy(
                &mut commands,
                &enemy_defs,
                &path,
                state.dummy,
                dummy_pos(state.origin, n, state.count),
                &mut meshes,
                &mut materials,
                server.as_deref(),
            );
        }
        return;
    }
    // Keep the row filled: each dead dummy is replaced by a fresh one.
    let live = dummies.iter().count() as u32;
    if live < state.count {
        let def = state.dummy.min(enemy_defs.list.len() - 1);
        let pos = dummy_pos(state.origin, live, state.count);
        spawn_dummy(
            &mut commands,
            &enemy_defs,
            &path,
            def,
            pos,
            &mut meshes,
            &mut materials,
            server.as_deref(),
        );
    }
}

fn spawn_dummy(
    commands: &mut Commands,
    enemy_defs: &EnemyDefs,
    path: &PathInfo,
    def: usize,
    pos: Vec3,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    server: Option<&AssetServer>,
) {
    let def = enemy_defs.list.get(def).unwrap_or(&enemy_defs.list[0]);
    let eid = spawn_enemy_at(
        commands,
        def,
        0,
        pos,
        0,
        None,
        path_units_per_point(path),
        meshes,
        materials,
        server,
    );
    commands.entity(eid).insert(RangeDummy);
}

/// The ledger: poll the dummies' real hp each frame — any drop is real
/// damage from the real pipeline; no combat logic is duplicated here.
pub fn range_ledger(
    time: Res<Time>,
    ledger: Option<ResMut<RangeLedger>>,
    dummies: Query<(Entity, &Enemy), With<RangeDummy>>,
) {
    let Some(mut ledger) = ledger else {
        return; // range not active
    };
    ledger.elapsed += time.delta_secs();
    for (e, enemy) in &dummies {
        let prev = {
            let entry = ledger.last_hp.iter_mut().find(|(le, _)| *le == e);
            match entry {
                Some((_, lp)) => Some(std::mem::replace(lp, enemy.hp)),
                None => None,
            }
        };
        match prev {
            Some(p) => {
                let dmg = p - enemy.hp;
                if dmg > 0.0 {
                    ledger.total += dmg;
                    ledger.hits += 1;
                    ledger.last = dmg;
                }
            }
            None => ledger.last_hp.push((e, enemy.hp)),
        }
    }
    ledger.last_hp.retain(|(e, _)| dummies.get(*e).is_ok());
}

/// The range control panel (right side): pick attacker/dummy/count, reset,
/// and the live ledger.
pub fn range_panel_ui(
    mut contexts: bevy_egui::EguiContexts,
    state: Option<ResMut<RangeState>>,
    ledger: Option<Res<RangeLedger>>,
    tower_defs: Res<TowerDefs>,
    enemy_defs: Res<EnemyDefs>,
) {
    let Some(mut state) = state else {
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    use bevy_egui::egui;
    egui::Window::new("Range (SM3R)")
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(0.0, 0.0))
        .movable(false)
        .resizable(false)
        .collapsible(false)
        .default_width(220.0)
        .show(ctx, |ui| {
            ui.heading("Range");
            ui.separator();
            egui::ComboBox::from_label("Attacker")
                .selected_text(tower_defs.list.get(state.attacker).map(|d| d.name).unwrap_or("?"))
                .show_ui(ui, |ui| {
                    for (i, d) in tower_defs.list.iter().enumerate() {
                        ui.selectable_value(&mut state.attacker, i, d.name);
                    }
                });
            egui::ComboBox::from_label("Dummy")
                .selected_text(enemy_defs.list.get(state.dummy).map(|d| d.name).unwrap_or("?"))
                .show_ui(ui, |ui| {
                    for (i, d) in enemy_defs.list.iter().enumerate() {
                        ui.selectable_value(&mut state.dummy, i, d.name);
                    }
                });
            ui.add(egui::DragValue::new(&mut state.count).range(1..=6));
            if ui.button("Apply / Reset").clicked() {
                state.reset = true;
            }
            ui.separator();
            if let Some(l) = ledger {
                let dps = if l.elapsed > 0.0 { l.total / l.elapsed } else { 0.0 };
                ui.label(format!("total {:.0}", l.total));
                ui.label(format!("dps {dps:.1}"));
                ui.label(format!("hits {}", l.hits));
                ui.label(format!("last {:.0}", l.last));
            }
            ui.separator();
            ui.small("real fire pipeline; dummies respawn on death");
        });
}
