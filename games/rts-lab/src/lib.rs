//! rts-lab — warbase 基底验收图（ADR-0016 §11 RL 系）。
//!
//! 定位：这不是一款游戏，是「游戏=地图 bundle + 薄插件」架构的验收场。
//! 一切机制来自 engine/warbase（选择/移动/生产/规则/经济/外交域）；
//! 本 crate 只做四件事：
//!   1. 装配地图 bundle（maps/arena.ron，ADR-0013）；
//!   2. 灰盒渲染与 HUD（bevy 原生，无资产，D9）；
//!   3. 极薄的玩法接线（开局布兵 / T 键训练 / 采集循环 / 左键点选缝）；
//!   4. 验收测试（tests/behavior.rs）。
//! 铁律（ADR-0012 §6）：engine/warbase/ 一行不改；发现基底缝缺失 → 记录
//! 报告（通道 C 信号），游戏侧写补丁逻辑孵化，不回填引擎。
//!
//! `main.rs` only calls `build_app()`; business logic lives in plugins/systems.

pub mod components;
pub mod plugins;
pub mod resources;
pub mod systems;

use bevy::prelude::*;

/// Assemble the full app. Shared by `main` (run) and reusable by integration
/// tests that need the whole visual stack (headless tests build their own).
pub fn build_app() -> App {
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.10, 0.11, 0.10)))
        .insert_resource(GlobalAmbientLight {
            color: Color::WHITE,
            brightness: 900.0,
            ..default()
        })
        .add_plugins(
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "rts-lab (warbase acceptance)".into(),
                    resolution: (1280, 720).into(),
                    ..default()
                }),
                ..default()
            }),
        )
        .add_plugins((
            // Battle runtime base (ADR-0012): set contract + every domain
            // (selection/movement/production/vision/items/replay/sfx/unit).
            warbase::WarbasePlugin,
            plugins::game::GamePlugin,
            plugins::visual::VisualPlugin,
            plugins::debug::DebugPlugin,
        ));
    app
}
