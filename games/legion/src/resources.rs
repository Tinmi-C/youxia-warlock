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

// ---------------------------------------------------------------------------
// 发兵（LG2）：建造期花钱给对家注入下一回合的额外兵
// ---------------------------------------------------------------------------

/// 回合封顶数（proposal §2：20 回合 → 剩余命数多者胜）。
pub const ROUND_CAP: u32 = 20;

/// 发兵价格台账：`sent[from][to]` = from 已成功发往 to 的兵数（价格按此
/// 递增 10%，见 round::send_cost）。per-target 表（proposal §2）。
#[derive(Resource, Debug, Clone, Default)]
pub struct SendLedger(pub [[u32; 4]; 4]);

/// 一笔排队的发兵（建造期下单，战斗期出征时兑现）。
#[derive(Debug, Clone)]
pub struct PendingSend {
    /// 出资并拥有该兵的玩家（漏怪赏金归他）。
    pub sponsor: u8,
    /// 被注入的对手（= 对家漏怪受害方）。
    pub target: u8,
    /// 注入的雇佣兵 def。
    pub def: String,
}

/// 待兑现的发兵队列（建造期累加，Build→Battle 转换时清空兑现）。
#[derive(Resource, Debug, Clone, Default)]
pub struct PendingSends(pub Vec<PendingSend>);
