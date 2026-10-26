//! Components = 纯数据（名词）。rts-lab 侧的玩法组件全部在这里：
//! 基底机制组件（MoveAgent/Selectable/ProductionQueue...）由 warbase 提供，
//! 这里只有验收图自己的薄标记。

use bevy::prelude::*;

/// 玩家主城（RL1：唯一的训练建筑；采集交货点）。
#[derive(Component, Debug)]
pub struct MainHall;

/// 金矿（RL1：不可枯竭；采集装载点）。
#[derive(Component, Debug)]
pub struct GoldMine;

/// 农民正在搬运的金量（RL1 采集循环：矿侧挂上、城侧卸下）。
#[derive(Component, Debug, Clone, Copy)]
pub struct Carrying(pub u32);

/// 一条绑定好的采集路线（矿 ↔ 城）。挂上后由采集系统在单位空闲时
/// 自动派腿（玩家右键打断优先，玩家腿走完自动回归循环）。
#[derive(Component, Debug, Clone, Copy)]
pub struct HarvestRoute {
    pub mine: Entity,
    pub hall: Entity,
}

/// 灰盒渲染已挂网格标记（避免重复挂 mesh）。
#[derive(Component, Debug)]
pub struct Visualized;

/// 选中描边的一根横梁（4 根拼成方形描边）。独立实体（非子实体），
/// 每帧同步到 `unit` 的 x/z——避免父子节点的增删簿记。
#[derive(Component, Debug)]
pub struct SelectionOutline {
    pub unit: Entity,
}
