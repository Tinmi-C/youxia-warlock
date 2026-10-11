//! Enemies: spawn, movement along the path, leak (base damage), kill (gold),
//! and the healer aura (EN5).
//! Capability cards: EN1 (defs), EN2 (move/leak), EN3 (death/gold), EN5 (healer).

use bevy::prelude::*;

use crate::components::{Enemy, EnemyAnim, Healer, Slow};
use crate::resources::{BaseHp, Boosts, Economy, EnemyDefs, PathInfo, WaveState};

/// EN5 starting values (requirements §9 gives no numbers — tunable on the
/// balance card): heal 3 hp every second within 6 world units.
const HEALER_RADIUS: f32 = 6.0;
const HEALER_PERIOD: f32 = 1.0;
const HEALER_HEAL_PER_TICK: f32 = 3.0;

// --- ART11: enemy presentation gateway ----------------------------------------

/// Per-archetype model pin (index = EnemyDefs order: 0 normal, 1 fast,
/// 2 shield, 3 healer, 4 elite, 5 boss). KayKit rigs, ArtVault v2
/// (alphaMode fix), feet-origin GLBs ~2.2–2.5 m tall in bind pose.
pub const ENEMY_MODELS: [&str; 6] = [
    "models/enemy_minion.glb",  // 普通怪: 骷髅小兵
    "models/enemy_rogue.glb",   // 快速怪: 骷髅刺客（跑步动画）
    "models/enemy_warrior.glb", // 盾兵怪: 骷髅战士
    "models/enemy_mage.glb",    // 治疗怪: 骷髅法师
    "models/enemy_elite.glb",   // 精英: 蛮族
    "models/enemy_boss.glb",    // BOSS: 骑士
];

/// Uniform visual scale per archetype: bind-pose heights 2.17–2.54 m, so
/// normals land ≈1.75 m (0.8), elite 2.5 m (1.05), boss 3.8 m (1.5).
pub const ENEMY_SCALES: [f32; 6] = [0.8, 0.8, 0.8, 0.8, 1.05, 1.5];

/// Fast enemies run instead of walk (matches their 1.8x speed fantasy).
pub const ENEMY_RUNS: [bool; 6] = [false, true, false, false, false, false];

/// Yaw added on top of the computed bearing. KayKit exports are taken as
/// +Z-facing (`atan2(dir.x, dir.z)`); if the visual acceptance shows models
/// moonwalking, flip this to PI once — same pin pattern as the ballista's
/// `BALLISTA_FORWARD_AXIS`.
pub const ENEMY_FORWARD_YAW_OFFSET: f32 = 0.0;

/// requirements §9 anchor: speed 1.0 should cross the entry->base path in
/// ~20 seconds. The per-point unit rate is derived from the actual path length
/// at spawn time, so changing the map keeps the anchor meaningful.
const ANCHOR_TRAVERSE_SECS: f32 = 20.0;

/// The archetype base color (healer green / armored steel / ordinary red).
/// Shared with the FX domain (FX3 tinting restores to this).
pub fn enemy_base_color(def: &crate::resources::EnemyDef) -> Color {
    if def.healer {
        Color::srgb(0.25, 0.75, 0.35) // healer: green so the aura is observable
    } else if def.physical_armor {
        Color::srgb(0.55, 0.55, 0.6)
    } else {
        Color::srgb(0.8, 0.25, 0.25)
    }
}

/// Spawn one enemy of archetype `index` at the path entry.
#[allow(clippy::too_many_arguments)]
pub fn spawn_enemy(
    commands: &mut Commands,
    defs: &EnemyDefs,
    path: &PathInfo,
    path_e: bevy::ecs::entity::Entity,
    index: usize,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    server: Option<&AssetServer>,
) {
    let Some(def) = defs.list.get(index) else {
        return;
    };
    let start = path.waypoints.first().copied().unwrap_or(Vec3::ZERO);
    spawn_enemy_at(
        commands,
        def,
        index,
        start,
        path_e,
        0.0,
        None,
        path_units_per_point(path),
        meshes,
        materials,
        server,
    );
}

/// The warden enemy defs as a BASE UnitRegistry (WX6-4a bridge, code-built
/// now — the same rows ship as warden.ron data with WX6-6): name-keyed,
/// armor mapped through the one-axis flag, bounty = kill gold. Lets the
/// base's damage/death pipeline attribute by def id.
pub fn enemy_unit_registry(defs: &EnemyDefs) -> warbase::prelude::UnitRegistry {
    let mut out = std::collections::HashMap::new();
    for def in &defs.list {
        out.insert(
            def.name.to_string(),
            warbase::prelude::UnitDef {
                id: def.name.to_string(),
                attrs: warbase::prelude::BaseAttributes {
                    hp: def.hp,
                    move_speed: def.speed,
                    body_radius: 0.35,
                },
                attack: None,
                armor_type: crate::systems::damage::base_armor(def.physical_armor),
                cost_gold: 0,
                cost_wood: 0,
                bounty_gold: def.kill_gold,
                bounty_wood: 0,
                grant_xp: 0,
                traits: vec![],
                auras: vec![],
                hero: false,
                skills: vec![],
                upgrade_to: None,
                heal: None,
            },
        );
    }
    warbase::prelude::UnitRegistry { defs: out }
}

/// World units per speed point for this path (§9 anchor: speed 1.0 crosses
/// the whole path in ~20 s, derived from the actual path length at spawn).
pub fn path_units_per_point(path: &PathInfo) -> f32 {
    let path_len: f32 = path
        .waypoints
        .windows(2)
        .map(|w| w[0].distance(w[1]))
        .sum();
    path_len / ANCHOR_TRAVERSE_SECS
}

/// Distance along the polyline up to waypoint `idx` (the progress value
/// that places a follower AT that waypoint) — the WX6-1 bridge for the old
/// next_wp-index spawn semantics.
pub fn waypoint_progress(path: &PathInfo, idx: usize) -> f32 {
    path.waypoints
        .windows(2)
        .take(idx)
        .map(|w| w[0].distance(w[1]))
        .sum()
}

/// Progress of the polyline point closest to `pos` (per-segment clamped
/// projection) — the WX6-1 bridge for "spawn/measure near a world position".
pub fn nearest_path_progress(path: &PathInfo, pos: Vec3) -> f32 {
    let p = Vec2::new(pos.x, pos.z);
    let mut best = (f32::MAX, 0.0f32); // (dist², progress)
    let mut acc = 0.0f32;
    for w in path.waypoints.windows(2) {
        let a = Vec2::new(w[0].x, w[0].z);
        let b = Vec2::new(w[1].x, w[1].z);
        let ab = b - a;
        let len = ab.length();
        let t = if len > 0.0 {
            ((p - a).dot(ab) / len / len).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let d2 = p.distance_squared(a + ab * t);
        if d2 < best.0 {
            best = (d2, acc + t * len);
        }
        acc += len;
    }
    best.1
}

/// Spawn one enemy at an explicit position (debug scenarios, DB2): same
/// construction as `spawn_enemy`, plus overrides for the spawn point, the
/// progress along the path and the starting HP (残血 setups for FX).
/// ART11: with an AssetServer the archetype model rides as a feet-origin
/// scene child (deferred walk animation via `EnemyAnim`); without one the
/// tinted cuboid fallback stays (headless tests).
/// WX6-1: movement rides the base — the entity gets a `PathFollower` on
/// `path_e` at `progress` plus a `MoveSpeed`; callables that must NOT move
/// (e.g. range dummies) remove the follower after this call.
#[allow(clippy::too_many_arguments)]
pub fn spawn_enemy_at(
    commands: &mut Commands,
    def: &crate::resources::EnemyDef,
    index: usize,
    pos: Vec3,
    path_e: bevy::ecs::entity::Entity,
    progress: f32,
    hp_override: Option<f32>,
    units_per_point: f32,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    server: Option<&AssetServer>,
) -> bevy::ecs::entity::Entity {
    let hp = hp_override.unwrap_or(def.hp);
    let eid = commands
        .spawn((
            Enemy {
                speed: def.speed * units_per_point,
                leak: def.leak,
                kill_gold: def.kill_gold,
                physical_armor: def.physical_armor,
                def_index: index,
            },
            // WX6-2: HP rides the base's components (bars/pipeline read
            // them); elites (4) and bosses (5) style the base's bar wide +
            // golden (the FX1 big-bar convention).
            warbase::prelude::Hp(hp),
            warbase::prelude::MaxHp(def.hp),
            warbase::prelude::Armor(crate::systems::damage::base_armor(
                def.physical_armor,
            )),
            warbase::prelude::UnitInstance { def: def.name.to_string(), level: 1 },
            // WX6-1: the base's path movement (advance_followers writes the
            // XZ translation; the spawn Y survives — models are feet-origin).
            warbase::prelude::PathFollower {
                path: path_e,
                progress,
                laps: 0,
                finished: false,
            },
            warbase::prelude::MoveSpeed(def.speed * units_per_point),
            warbase::fx::HealthBarStyle {
                width: if matches!(index, 4 | 5) { 1.4 } else { 0.9 },
                color: if matches!(index, 4 | 5) {
                    Color::srgb(1.0, 0.8, 0.25)
                } else {
                    Color::srgb(0.3, 0.9, 0.35)
                },
            },
            // Explicit Visibility: the model path has no mesh on this entity,
            // so without it the scene child's InheritedVisibility breaks (B0004).
            Visibility::Visible,
            // Feet-origin models sit at ground level; the box fallback below
            // re-centers itself (y = 0.35 = half its 0.7 height).
            Transform::from_xyz(pos.x, 0.0, pos.z),
        ))
        .id();
    let model = server.zip(ENEMY_MODELS.get(index).copied());
    match model {
        Some((server, model)) => {
            let scene = server.load(
                bevy::gltf::GltfAssetLabel::Scene(0).from_asset(model),
            );
            let gltf = server.load::<bevy::gltf::Gltf>(model);
            let scale = ENEMY_SCALES.get(index).copied().unwrap_or(0.8);
            let run = ENEMY_RUNS.get(index).copied().unwrap_or(false);
            commands.entity(eid).with_children(|p| {
                p.spawn((
                    EnemyAnim {
                        gltf,
                        run,
                        wired: None,
                    },
                    bevy::world_serialization::WorldAssetRoot(scene),
                    Transform::from_scale(Vec3::splat(scale)),
                ));
            });
        }
        None => {
            let color = enemy_base_color(def);
            commands.entity(eid).insert((
                Mesh3d(meshes.add(Cuboid::new(0.7, 0.7, 0.7))),
                MeshMaterial3d(materials.add(color)),
                Transform::from_xyz(pos.x, 0.35, pos.z),
            ));
        }
    }
    if def.healer {
        commands.entity(eid).insert(Healer {
            radius: HEALER_RADIUS,
            period: HEALER_PERIOD,
            heal_per_tick: HEALER_HEAL_PER_TICK,
            tick_timer: HEALER_PERIOD,
        });
    }
    eid
}

/// Marker for the debug range's stationary dummies (SM3R): excluded from
/// path movement — they stand and take it.
#[derive(Component)]
pub struct RangeDummy;

/// ART11: once an enemy model's Gltf asset is in, wire the locomotion clip.
/// Bevy 0.19's gltf loader (load_animations: true by default) plants an
/// `AnimationPlayer` INSIDE the spawned scene on the animation root and links
/// every bone to it via `AnimatedBy` — so the player we must drive is that
/// one, found in the model subtree. Attaching our own player on the wrapper
/// plays into the void (bones stay bound to the loader's player): T-pose
/// sliding. KayKit clip names carry a `Rig_Medium.001|` prefix, so match by
/// substring (`walking` / `running`). The marker survives until the scene
/// has actually instantiated its player (asset-loaded alone is not enough).
pub fn start_enemy_walk(
    mut commands: Commands,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    gltfs: Res<Assets<bevy::gltf::Gltf>>,
    mut pending: Query<(Entity, &mut EnemyAnim)>,
    children: Query<&Children>,
    mut players: Query<&mut AnimationPlayer>,
    graphed: Query<(), (With<AnimationPlayer>, With<AnimationGraphHandle>)>,
) {
    for (wrapper, mut anim) in &mut pending {
        // Healthy: the recorded player still exists and still carries our
        // graph. (Scene rebuilds despawn the old player — then we re-wire.)
        if let Some(w) = &anim.wired {
            if graphed.get(w.player).is_ok() {
                continue;
            }
        }
        let Some(gltf) = gltfs.get(&anim.gltf) else {
            continue; // still loading — retry next frame
        };
        let want = if anim.run { "running" } else { "walking" };
        let Some((clip_name, clip)) = gltf
            .named_animations
            .iter()
            .find(|(name, _)| name.to_lowercase().contains(want))
        else {
            // No locomotion clip: keep the bind pose, stop checking.
            info!("[art11] no locomotion clip for {}", want);
            commands.entity(wrapper).remove::<EnemyAnim>();
            continue;
        };
        // The scene instantiates a frame or more after the assets land, so
        // the loader's player may not exist yet — keep the marker and retry.
        let Some(player_e) = find_player(wrapper, &children, &players) else {
            continue;
        };
        // Reuse the graph asset across rebuilds (app-owned, survives scenes).
        let (handle, node) = match &anim.wired {
            Some(w) => (w.graph.clone(), w.node),
            None => {
                let (graph, node) = AnimationGraph::from_clip(clip.clone());
                (graphs.add(graph), node)
            }
        };
        let Ok(mut player) = players.get_mut(player_e) else {
            continue;
        };
        player.play(node).repeat();
        commands
            .entity(player_e)
            .insert(AnimationGraphHandle(handle.clone()));
        let rewired = anim.wired.is_some();
        anim.wired = Some(crate::components::WiredAnim {
            player: player_e,
            graph: handle,
            node,
        });
        if rewired {
            info!("[art11] walk RE-wired after scene rebuild: {clip_name}");
        } else {
            info!("[art11] walk wired: {clip_name}");
        }
    }
}

/// Depth-first search for the scene's own `AnimationPlayer` below `root`.
/// Shared with the ART12 ranger wiring (same loader-player rule).
pub fn find_player(
    root: Entity,
    children: &Query<&Children>,
    players: &Query<&mut AnimationPlayer>,
) -> Option<Entity> {
    if players.get(root).is_ok() {
        return Some(root);
    }
    let kids = children.get(root).ok()?;
    kids.iter()
        .find_map(|k| find_player(k, children, players))
}

// --- WX6-1: movement on the base's advance_followers ------------------------------
//
// The walk itself is `warbase::prelude::advance_followers` (progress sampling,
// Arrived at the end of the open chain). Warden keeps three thin systems
// around it: the slow factor folds into MoveSpeed, models face the path
// tangent, and Arrived resolves the leak (base HP + wave counter + despawn).

/// Fold the `Slow` factor of the moment into the follower's MoveSpeed
/// (`Enemy.speed` is the pre-slow base; apply_slow keeps one strongest
/// component per enemy).
pub fn sync_move_speed(
    slows: Query<&Slow>,
    mut q: Query<(Entity, &Enemy, &mut warbase::prelude::MoveSpeed), Without<RangeDummy>>,
) {
    for (eid, enemy, mut speed) in &mut q {
        let factor = slows.get(eid).map(|s| s.factor).unwrap_or(1.0);
        speed.0 = enemy.speed * factor;
    }
}

/// ART11 facing: rotate walkers toward the path tangent at their progress
/// (models are +Z-forward — ENEMY_FORWARD_YAW_OFFSET). The symmetric box
/// fallback rotates invisibly, so one code path serves both.
pub fn face_movement(
    paths: Query<&warbase::prelude::PathDef>,
    mut q: Query<(&warbase::prelude::PathFollower, &mut Transform), Without<RangeDummy>>,
) {
    for (f, mut tf) in &mut q {
        if f.finished {
            continue;
        }
        let Ok(path) = paths.get(f.path) else { continue };
        let a = path.sample(f.progress);
        let b = path.sample(f.progress + 0.25);
        let dir = b - a;
        if dir.length_squared() > 1e-8 {
            tf.rotation = Quat::from_rotation_y(
                f32::atan2(dir.x, dir.y) + ENEMY_FORWARD_YAW_OFFSET,
            );
        }
    }
}

/// EN2 leak: an enemy that finished the open chain deducts its `leak` from
/// the base and despawns (consumes the base's Arrived message).
pub fn leak_arrived(
    mut arrived: MessageReader<warbase::prelude::Arrived>,
    mut base: ResMut<BaseHp>,
    mut wave: ResMut<WaveState>,
    mut commands: Commands,
    enemies: Query<&Enemy>,
) {
    for a in arrived.read() {
        let Ok(enemy) = enemies.get(a.unit) else {
            continue; // already resolved between the message and here
        };
        base.hp = base.hp.saturating_sub(enemy.leak);
        wave.active = wave.active.saturating_sub(1);
        commands.entity(a.unit).despawn();
        info!("[enemy] leaked, base hp={}", base.hp);
    }
}

/// EN3: grant gold and despawn any enemy at or below 0 HP (the Hp
/// component since WX6-2; bounty still reads Enemy.kill_gold — the AC4
/// boost applies here). The base's resolver marks pipeline kills with hp<=0
/// (+ its UnitDied message, which warden leaves unconsumed: this scan is
/// source-agnostic and covers debug-spawned corpses too). FX2: the death
/// shell rides along — gold/despawn logic and timing untouched.
pub fn resolve_death(
    mut commands: Commands,
    boost: Res<Boosts>,
    mut economy: ResMut<Economy>,
    mut wave: ResMut<WaveState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    enemies: Query<(Entity, &Enemy, &warbase::prelude::Hp)>,
    visuals: Query<(&Transform, &MeshMaterial3d<StandardMaterial>)>,
) {
    for (eid, enemy, hp) in &enemies {
        if hp.0 <= 0.0 {
            let gain = (enemy.kill_gold as f32 * boost.kill_mult).round() as u32;
            economy.gold += gain;
            wave.active = wave.active.saturating_sub(1);
            if let Ok((tf, mat)) = visuals.get(eid) {
                if let Some(color) = materials.get(&mat.0).map(|m| m.base_color) {
                    crate::systems::fx::spawn_death_shell(
                        &mut commands, &mut meshes, &mut materials, tf, color,
                    );
                }
            }
            commands.entity(eid).despawn();
            info!("[enemy] killed, gold={}", economy.gold);
        }
    }
}

/// EN5: healers periodically restore hp of nearby damaged allies (never
/// themselves). Overheal is clamped to max_hp.
pub fn heal_aura(
    time: Res<Time>,
    mut healers: Query<(Entity, &Transform, &mut Healer)>,
    mut enemies: Query<(Entity, &mut warbase::prelude::Hp, &warbase::prelude::MaxHp, &Transform)>,
) {
    for (healer_e, htf, mut healer) in &mut healers {
        healer.tick_timer -= time.delta_secs();
        if healer.tick_timer > 0.0 {
            continue;
        }
        healer.tick_timer = healer.period;
        for (eid, mut hp, max, etf) in &mut enemies {
            if eid == healer_e || hp.0 <= 0.0 || hp.0 >= max.0 {
                continue;
            }
            if htf.translation.distance(etf.translation) <= healer.radius {
                hp.0 = (hp.0 + healer.heal_per_tick).min(max.0);
                info!("[enemy] healer aura -> hp={:.0}", hp.0);
            }
        }
    }
}
