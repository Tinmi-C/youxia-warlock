//! legion — 军团战争图（第三张 warbase 验收图，ADR-0019 Phase 2）。
//!
//! 定位：多人对抗半区（WC3 军团战争式 2v2），用来**度量基底复用效率**
//! （KPI 对比 rts-lab，见 docs/kpi.md）。回合制节奏：建造期雇兵布阵 →
//! 战斗期沿路攻对家 → 漏怪扣命 → 结算入账。机制全部来自 engine/warbase
//! （bundle/unit/movement(MOV4 AttackMove)/combat/econ(赏金)/rule(LastTeamStanding)/
//! unit(teams 外交)）；本 crate 只做四件事：
//!   1. 装配地图 bundle（maps/arena.ron：4 lane / 48 布阵格 / 6 雇佣兵 def）；
//!   2. 回合层状态机（基底 MatchFlow 之上的游戏侧回合层）；
//!   3. 灰盒渲染与 HUD（bevy 原生，无资产，D9）；
//!   4. 验收测试（tests/behavior.rs）+ KPI 台账（docs/kpi.md）。
//! 铁律（ADR-0012 §6）：engine/warbase/ 一行不改；发现基底缝缺失 → 记入
//! docs/kpi.md 洞清单（通道 C 信号），游戏侧写薄补丁孵化，不回填引擎。
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
                    title: "legion (warbase acceptance)".into(),
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
        ));
    app
}
