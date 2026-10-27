//! Components = 纯数据（名词）。rts-lab 侧的玩法组件全部在这里：
//! 基底机制组件（MoveAgent/Selectable/ProductionQueue/Mine/Carrying...）
//! 由 warbase 提供，这里只有验收图自己的薄标记。
//! （RL4：MainHall/GoldMine/Carrying/HarvestRoute 已被基底 harvest 域取代，
//! 游戏侧采集组件全部退役。）

use bevy::prelude::*;

/// 灰盒渲染已挂网格标记（避免重复挂 mesh）。
#[derive(Component, Debug)]
pub struct Visualized;

/// 选中描边的一根横梁（4 根拼成方形描边）。独立实体（非子实体），
/// 每帧同步到 `unit` 的 x/z——避免父子节点的增删簿记。
#[derive(Component, Debug)]
pub struct SelectionOutline {
    pub unit: Entity,
}
