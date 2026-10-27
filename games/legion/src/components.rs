//! Components = 纯数据（名词）。legion 侧的玩法组件全部在这里：
//! 基底机制组件（MoveAgent/Selectable/UnitInstance/Owner...）由 warbase
//! 提供，这里只有本图的薄标记。

use bevy::prelude::*;

/// 灰盒渲染已挂网格标记（避免重复挂 mesh）。
#[derive(Component, Debug)]
pub struct Visualized;
