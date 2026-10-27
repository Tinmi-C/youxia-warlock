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

// ---------------------------------------------------------------------------
// 回合层（LG1）：基底 MatchFlow 之上的游戏侧状态机
// ---------------------------------------------------------------------------

/// 建造期时长（帧计）：30s @ 60fps（proposal §2 锚点）。
pub const BUILD_FRAMES: u32 = 1800;

/// 一路一条：bundle path i 的游戏侧视图。`baseline` = 对家底线（发兵
/// AttackMove 目标 + 漏怪判定点），`target` = 该路攻打的对家玩家槽。
#[derive(Debug, Clone)]
pub struct Lane {
    pub owner: usize,
    /// 己方布阵区端（path 起点，只作展示/最近格分配用）。
    pub start: Vec2,
    /// 对家底线端（path 终点）。
    pub baseline: Vec2,
    /// 对家玩家槽位（漏怪受害方）。
    pub target: usize,
}

#[derive(Resource, Debug, Clone)]
pub struct Lanes(pub Vec<Lane>);

/// 回合阶段。结算（Payout）即时完成，不占相位（见 round_tick）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// 建造期：可雇兵布阵。
    Build,
    /// 战斗期：雇佣兵自动 AttackMove 至对家底线，交战全自动。
    Battle,
}

#[derive(Resource, Debug, Clone)]
pub struct RoundFlow {
    /// 1 起算的回合数。
    pub round: u32,
    pub phase: Phase,
    /// 当前相位已过的帧数。
    pub frame: u32,
}

/// 每家命数（起始值取 bundle rules.leak 的 Lives 锚点 20；漏 1 扣 1，
/// 扣尽出局 —— hole L2：基底 LivesLeft 单例无 per-player 缝）。
#[derive(Resource, Debug, Clone)]
pub struct LivesBoard(pub [u32; 4]);

/// 每家人口记账。`used` 每帧从存活 Merc 实体重算（单一事实源 = 场上
/// 实体，死亡/漏怪零漂移）；`cap` 取 bundle econ_start.pop_cap。
#[derive(Resource, Debug, Clone)]
pub struct PopLedger {
    pub used: [u8; 4],
    pub cap: [u8; 4],
}
