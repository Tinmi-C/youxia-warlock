//! Resources = 全局单例数据（每资源尽量单一写者）。

use bevy::prelude::*;
use warbase::bundle::MapBundle;

/// 当前已装配的地图（重开局 / UI 读取用，同 loop-td 的 CurrentMap）。
#[derive(Resource)]
pub struct CurrentMap(pub MapBundle);

/// 拖框选的锚点（按下时的屏幕坐标；游戏侧指针桥的簿记，
/// 对应基底的 PointerDrag——我们不用基底那份因为它连着错误平面的链）。
#[derive(Resource, Debug, Default)]
pub struct RtsDrag {
    pub anchor: Option<Vec2>,
}
