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

/// 发兵标记（LG2）：经发兵商店注入的雇佣兵。与 Merc 并存（player 即出资
/// 方）；唯一的语义差异是漏怪时出资方额外拿击穿赏金（round::leak_tick）。
#[derive(Component, Debug)]
pub struct SentMerc;
