//! Components = 纯数据（名词）。legion 侧的玩法组件全部在这里：
//! 基底机制组件（MoveAgent/Selectable/UnitInstance/Owner...）由 warbase
//! 提供，这里只有本图的薄标记。

use bevy::prelude::*;

/// 灰盒渲染已挂网格标记（避免重复挂 mesh）。
#[derive(Component, Debug)]
pub struct Visualized;

/// 雇佣兵标记：归属玩家 / 人口占用（hole L3：UnitDef 无人口字段，
/// 游戏侧记账）/ 所在布阵格实体（占格互斥 + 撤销/退款的锚点）。
#[derive(Component, Debug)]
pub struct Merc {
    pub player: u8,
    pub pop: u8,
    pub slot: Entity,
}

/// 布阵格实体（48 个，Startup 从 bundle FixedSlots 派生；`player` 按
/// 「距哪条 lane 起点最近」分配，间距 4.0 远大于格间距 1.0，无歧义）。
#[derive(Component, Debug)]
pub struct DeploySlot {
    pub player: u8,
    pub index: u8,
}
