//! Resources = 全局单例数据（每资源尽量单一写者）。

use bevy::prelude::*;
use warbase::bundle::MapBundle;

/// 当前已装配的地图（重开局 / UI 读取用，同 rts-lab 的 CurrentMap）。
#[derive(Resource)]
pub struct CurrentMap(pub MapBundle);

/// 四家金库（hole L2：基底 Resources/Population 是单人全局单例，没有
/// per-player 钱包缝 —— 军团图游戏侧自建钱包，下标=玩家槽位 P0-P3）。
#[derive(Resource, Debug, Clone)]
pub struct Purse(pub [u32; 4]);

impl Purse {
    pub fn gold(&self, player: usize) -> u32 {
        self.0[player]
    }
}
