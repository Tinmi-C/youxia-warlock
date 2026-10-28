//! Resources = 全局单例数据（每资源尽量单一写者）。
//!
//! LG4（round-4 LDEBT 上收）：三账本 Purse/LivesBoard/PopLedger 退役——
//! 基底 econ 域的 `Wallets`（per-player 金/人口，L2）与 rule 域的
//! `PlayerLives`（L5）接管；本文件只保留回合层自己的状态。

use bevy::prelude::*;
use warbase::bundle::MapBundle;

/// 当前已装配的地图（重开局 / UI 读取用，同 rts-lab 的 CurrentMap）。
#[derive(Resource)]
pub struct CurrentMap(pub MapBundle);

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
