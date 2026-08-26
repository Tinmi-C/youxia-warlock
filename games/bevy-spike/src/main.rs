//! Bevy spike — 全流程冒烟测试（ADR-0004 的证据收集）
//!
//! 验证整条链路在 Bevy 下开箱即用：
//!   模型导入（glTF 人形 + 骨骼）→ 骨骼动画 → 相机/灯光 → UI → 构建
//!
//! 对照 m2-bevy：同一个 hero.glb（CesiumMan 人形），手写渲染器只能画出
//! T 姿势裸模（无蒙皮/多材质），这里加载后直接播放骨骼动画。
//!
//! 操作：
//!   Space   暂停 / 恢复动画
//!   Up/Down 加速 / 减速播放

use std::f32::consts::PI;

use bevy::{
    light::CascadeShadowConfigBuilder,
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot},
    world_serialization::WorldInstanceReady,
};

const HERO_GLB: &str = "hero.glb";
const MONSTER_GLB: &str = "monster.glb";

/// 场景加载完成后要播放的动画（加载时创建，场景就绪后由观察者消费）。
#[derive(Component)]
struct AnimationToPlay {
    graph_handle: Handle<AnimationGraph>,
    index: AnimationNodeIndex,
}

/// 标记英雄场景实体，供输入系统定位动画。
#[derive(Component)]
struct Hero;

#[derive(Resource, Default)]
struct FpsLog {
    acc: f32,
}

fn main() {
    App::new()
        .init_resource::<FpsLog>()
        .insert_resource(GlobalAmbientLight {
            color: Color::WHITE,
            brightness: 1200.0,
            ..default()
        })
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, setup)
        .add_systems(Update, (toggle_animation, auto_screenshot, log_anim_clock, log_fps))
        .run();
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // --- 相机 ---
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 1.6, 4.5).looking_at(Vec3::new(0.0, 1.1, 0.0), Vec3::Y),
    ));

    // --- 主方向光（带阴影级联）+ 环境光兜底（App 级 resource） ---
    commands.spawn((
        Transform::from_rotation(Quat::from_euler(EulerRot::ZYX, 0.0, 1.2, -PI / 4.)),
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        CascadeShadowConfigBuilder {
            first_cascade_far_bound: 6.0,
            maximum_distance: 20.0,
            ..default()
        }
        .build(),
    ));

    // --- 地面 ---
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(20.0, 20.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.25, 0.3, 0.25))),
    ));

    // --- 英雄（hero.glb）：场景 + 骨骼动画 ---
    // hero.glb 只有 1 个动画片段（索引 0），57 个节点目标（全身骨骼）。
    let (graph, index) = AnimationGraph::from_clip(
        asset_server.load(GltfAssetLabel::Animation(0).from_asset(HERO_GLB)),
    );
    let graph_handle = graphs.add(graph);
    commands
        .spawn((
            AnimationToPlay {
                graph_handle,
                index,
            },
            // WorldAssetRoot：资产加载完成后自动把场景作为子实体生成。
            WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(HERO_GLB))),
            Hero,
        ))
        .observe(play_animation_when_ready);

    // --- 怪物（monster.glb）：静态摆位对照（验证多场景共存 + 父子变换） ---
    commands.spawn((
        Transform::from_xyz(2.5, 0.0, 0.0).with_scale(Vec3::splat(1.2)),
        WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(MONSTER_GLB))),
    ));

    // --- UI：操作说明（静态文本，验证 bevy_ui 文本管线） ---
    commands.spawn((
        Text::new("Space: play/pause  |  Up/Down: speed\nhero.glb skeleton animation — Bevy 0.19 spike"),
        TextFont {
            font_size: FontSize::Px(20.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        },
    ));
}

/// 场景（含骨骼网格与动画）加载并生成后，给 AnimationPlayer 挂上动画图并开播。
fn play_animation_when_ready(
    scene_ready: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    animations_to_play: Query<&AnimationToPlay>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let Ok(animation_to_play) = animations_to_play.get(scene_ready.entity) else {
        return;
    };
    // WorldAssetRoot 会把场景生成为本实体的子层级；AnimationPlayer 在子实体上。
    for child in children.iter_descendants(scene_ready.entity) {
        if let Ok(mut player) = players.get_mut(child) {
            player.play(animation_to_play.index).repeat();
            commands
                .entity(child)
                .insert(AnimationGraphHandle(animation_to_play.graph_handle.clone()));
            info!("[spike] hero skeleton animation started on entity {child:?}");
        }
    }
}

/// Space 暂停/恢复，Up/Down 调速（验证输入链路）。
fn toggle_animation(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut players: Query<&mut AnimationPlayer>,
    to_play: Query<&AnimationToPlay, With<Hero>>,
) {
    let Ok(animation_to_play) = to_play.single() else {
        return;
    };
    for mut player in &mut players {
        let Some(anim) = player.animation_mut(animation_to_play.index) else {
            continue;
        };
        if keyboard_input.just_pressed(KeyCode::Space) {
            if anim.is_paused() {
                anim.resume();
                info!("[spike] animation resumed");
            } else {
                anim.pause();
                info!("[spike] animation paused");
            }
        }
        if keyboard_input.just_pressed(KeyCode::ArrowUp) {
            anim.set_speed(anim.speed() * 1.5);
            info!("[spike] speed x{:.2}", anim.speed());
        }
        if keyboard_input.just_pressed(KeyCode::ArrowDown) {
            anim.set_speed(anim.speed() / 1.5);
            info!("[spike] speed x{:.2}", anim.speed());
        }
    }
}

/// 运行 6s / 10s 各截一帧（场景与动画已就绪），存 PNG 作为验收证据。
/// 两帧 diff 可证明骨骼动画真的在变形（静止绑定姿势则两帧相同）。
fn auto_screenshot(
    mut commands: Commands,
    time: Res<Time>,
    mut shot1: Local<bool>,
    mut shot2: Local<bool>,
) {
    if !*shot1 && time.elapsed_secs() > 6.0 {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk("spike-shot-1.png"));
        *shot1 = true;
        info!("[spike] screenshot 1 requested");
    }
    if !*shot2 && time.elapsed_secs() > 10.0 {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk("spike-shot-2.png"));
        *shot2 = true;
        info!("[spike] screenshot 2 requested");
    }
}

/// 每 3 秒打印一次英雄动画时钟（seek_time）——证明动画时间轴在推进（骨骼在变形）。
fn log_anim_clock(
    time: Res<Time>,
    to_play: Query<&AnimationToPlay, With<Hero>>,
    mut players: Query<&mut AnimationPlayer>,
    mut last: Local<f32>,
) {
    if time.elapsed_secs() - *last < 3.0 {
        return;
    }
    *last = time.elapsed_secs();
    let Ok(animation_to_play) = to_play.single() else {
        return;
    };
    for mut player in &mut players {
        if let Some(anim) = player.animation_mut(animation_to_play.index) {
            info!(
                "[spike] anim clock: seek={:.2}s speed={:.2} paused={}",
                anim.seek_time(),
                anim.speed(),
                anim.is_paused()
            );
        }
    }
}

/// 控制台 FPS 日志（每 2 秒一条）——无显示器环境也能确认渲染循环在跑。
fn log_fps(time: Res<Time>, mut log: ResMut<FpsLog>) {
    log.acc += time.delta_secs();
    if log.acc >= 2.0 {
        let fps = 1.0 / time.delta_secs().max(1e-6);
        info!("[spike] fps ≈ {fps:.0}");
        log.acc = 0.0;
    }
}
