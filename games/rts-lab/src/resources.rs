//! Resources = 全局单例数据（每资源尽量单一写者）。

use bevy::prelude::*;
use warbase::bundle::MapBundle;

/// 当前已装配的地图（重开局 / UI 读取用，同 loop-td 的 CurrentMap）。
#[derive(Resource)]
pub struct CurrentMap(pub MapBundle);
