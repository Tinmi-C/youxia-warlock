//! Round layer (LG1+LG2, LG4 base-seam migration): the game-side state
//! machine ON TOP of the base's MatchFlow — 建造期 (frame-counted 30s) →
//! 战斗期 (auto march + auto combat) → 即时结算 (+50g income + kill
//! bounties + breach bounties) → next round, adjudicated at the 20-round
//! cap (most lives wins; equal lives draws).
//!
//! Thin-game rule: every mechanism is the base's — orders go out as
//! `UnitCommand::AttackMove` (MOV4 tick_orders consumes), fighting is base
//! combat (pick_targets / fire_attacks / resolve_damage), kill bounties
//! ride the base `apply_econ` (killer-Owner attribution, L2), per-player
//! gold/pop live in base `Wallets` (L2), per-player lives in base
//! `PlayerLives` with the base owning elimination sweeps + the
//! lives-based win (L5, `WinCondition::TeamLives`), the round-cap ending
//! writes the base's structured `MatchFlow::Ended{Win|Draw}` (L7), and
//! the send injection spawns through the base's owned-unit seam
//! `spawn_owned_deferred` with the attack-move order armed at spawn
//! (L8). Game-side additions are exactly the round clock, the hire shop,
//! the send shop, the leak rule and settlement — the seams the base does
//! not carry (see kpi.md).

use bevy::prelude::*;
use std::collections::BTreeMap;
use warbase::prelude::*;

use crate::components::{DeploySlot, Merc, SentMerc};
use crate::resources::{
    CurrentMap, Lane, Lanes, PendingSend, PendingSends, Phase, RoundFlow, SendLedger,
    BUILD_FRAMES, ROUND_CAP,
};

/// 每回合基础收入（proposal §2：50 基础 + 击杀赏金）。
pub const INCOME_PER_ROUND: u32 = 50;
/// 漏怪判定半径：AttackMove 目标即底线点，arrival brake 0.1 内停步，
/// 0.9 半径保证行进中先于停步扣分。
pub const LEAK_RADIUS: f32 = 0.9;
/// lane 配对（发兵目标/漏怪受害方）：P0<->P1，P2<->P3。
pub const OPPONENT: [usize; 4] = [1, 0, 3, 2];
/// 发兵起步价（proposal §2：25 金起）。
pub const SEND_BASE_COST: u32 = 25;
/// 发兵同目标递增系数（proposal §2：每次 +10%）。
pub const SEND_GROWTH: f32 = 1.1;
/// 发兵击穿赏金（proposal §2：发出的兵打穿对家防线，出资方每兵 +5）。
/// 数值起点可调——赏金须低于起步价 25，发兵才是「押注对家守不住」而非
/// 稳赚套利。
pub const BREACH_BOUNTY: u32 = 5;
/// 布阵格生成参数（L1 上收后 48 个手写坐标退役）：每路 4 列 x 3 行，
/// 间距 1.0，从 lane 起点向对家方向排布（见 init_round_state）。
const SLOT_COLUMNS: usize = 4;
const SLOT_ROWS: usize = 3;
const SLOT_SPACING: f32 = 1.0;

/// 发兵价格：第 `sent`+1 次发往同一目标的价格 = floor(25 × 1.1^sent)。
/// 取整规则写死（验收锚点：25 / 27 / 30）；f32 powi 在各平台确定性一致。
pub fn send_cost(sent: u32) -> u32 {
    (SEND_BASE_COST as f32 * SEND_GROWTH.powi(sent as i32)).floor() as u32
}

// ---------------------------------------------------------------------------
// Messages（游戏侧消息，GamePlugin 注册）
// ---------------------------------------------------------------------------

/// 建造期雇兵命令（tests / 输入桥写入；消费方是 [`hire`]）。
#[derive(Message, Debug)]
pub struct HireCommand {
    pub player: u8,
    pub def: String,
    pub slot: Entity,
}

/// 雇兵被拒（金不足/相位不对/格被占/人口满/已出局）。
#[derive(Message, Debug, Clone)]
pub struct HireRejected {
    pub reason: String,
}

/// 相位切换（HUD/测试观察口）。
#[derive(Message, Debug, Clone, Copy)]
pub struct PhaseChanged {
    pub round: u32,
    pub phase: Phase,
}

/// 回合结算完成（HUD/测试观察口）。
#[derive(Message, Debug, Clone, Copy)]
pub struct RoundSettled {
    pub round: u32,
    pub income: u32,
}

/// 漏怪发生（HUD/测试观察口）。
#[derive(Message, Debug, Clone, Copy)]
pub struct LeakHappened {
    pub attacker: u8,
    pub victim: u8,
}

/// 玩家出局（命尽；观察口——出局清扫与判胜由基底 L5 规则接管）。
#[derive(Message, Debug, Clone, Copy)]
pub struct PlayerEliminated {
    pub player: u8,
}

/// 建造期发兵命令（tests / 输入桥写入；消费方是 [`send`]）：
/// 给 `target` 注入下一回合的额外兵（LG2）。
#[derive(Message, Debug)]
pub struct SendCommand {
    pub player: u8,
    pub target: u8,
    pub def: String,
}

/// 发兵被拒（金不足/相位不对/无通路/目标无效/def 不可发）。
#[derive(Message, Debug, Clone)]
pub struct SendRejected {
    pub reason: String,
}

// ---------------------------------------------------------------------------
// 人口（L3 上收后）：pop 数值来自 UnitDef.pop_cost；「可雇/可发集合」仍由
// 命令卡表（MERCENARY_DEFS）表达——那是商店货架，不是数值。
// ---------------------------------------------------------------------------

/// 雇佣兵人口占用 = def 的 `pop_cost`（L3：数值进地图包）。非雇佣兵 def
/// 返回 None（= 不可雇：主城等不在货架上的单位）。
pub fn pop_cost(registry: &UnitRegistry, def: &str) -> Option<u8> {
    if !crate::systems::gameplay::MERCENARY_DEFS.contains(&def) {
        return None;
    }
    registry.get(def).map(|d| d.pop_cost)
}

// ---------------------------------------------------------------------------
// Startup：lane/布阵格 装配（钱包/命数/人口由基底 assemble 播种）
// ---------------------------------------------------------------------------

/// Exclusive startup (after load_map): derive the game-side round state
/// from the assembled world — lanes from `world.paths` (order = player
/// slot; the pairing table is game-side), deploy-slot entities GENERATED
/// from each lane (L1: the 48 hand-written RON coords retired — per lane,
/// 4 columns x 3 rows at 1.0 spacing, starting 1.0 past the lane start
/// toward the baseline, exactly the coordinates the FixedSlots list used
/// to spell out), lives/pop/gold from the base (PlayerLives/Wallets,
/// seeded by assemble under TeamLives/econ_start).
pub fn init_round_state(world: &mut World) {
    let bundle = world.resource::<CurrentMap>().0.clone();

    let mut lanes = Vec::new();
    for (i, path) in bundle.world.paths.iter().enumerate() {
        let first = path.points.first().map(|p| Vec2::new(p[0], p[1])).unwrap_or_default();
        let last = path.points.last().map(|p| Vec2::new(p[0], p[1])).unwrap_or_default();
        let target = OPPONENT.get(i).copied().unwrap_or(0);
        lanes.push(Lane { owner: i, start: first, baseline: last, target });
    }

    // Deploy slots: generated per lane (order = authored lane order, then
    // row-major: side row first, columns marching toward the baseline —
    // the exact enumeration the old FixedSlots RON carried). The row axis
    // is CANONICALIZED (perp flipped toward +z, or +x for vertical lanes)
    // so mirrored lanes (P0 marching +x, P1 marching -x) enumerate rows in
    // the same world order — the legacy coords are identical per player.
    let mut slot_count = 0usize;
    for lane in &lanes {
        let delta = lane.baseline - lane.start;
        let dir = if delta.length_squared() > f32::EPSILON {
            delta.normalize()
        } else {
            Vec2::X
        };
        let mut perp = Vec2::new(-dir.y, dir.x);
        if perp.y < 0.0 || (perp.y == 0.0 && perp.x < 0.0) {
            perp = -perp;
        }
        for row in 0..SLOT_ROWS {
            // side rows around the lane axis: dz = -1, 0, +1.
            let dz = row as f32 - 1.0;
            for col in 1..=SLOT_COLUMNS {
                let pos = lane.start
                    + dir * (col as f32 * SLOT_SPACING)
                    + perp * (dz * SLOT_SPACING);
                world.spawn((
                    DeploySlot { player: lane.owner as u8, index: slot_count as u8 },
                    Selectable,
                    Visibility::default(),
                    Transform::from_xyz(pos.x, 0.0, pos.y),
                ));
                slot_count += 1;
            }
        }
    }

    world.insert_resource(Lanes(lanes));
    world.insert_resource(RoundFlow { round: 1, phase: Phase::Build, frame: 0 });
    info!("[legion] round state ready: {} lanes, {slot_count} generated slots", bundle.world.paths.len());
}

// ---------------------------------------------------------------------------
// 建造期：雇佣兵商店
// ---------------------------------------------------------------------------

/// 建造期买兵（消息驱动；验收 ② 的实现体）。校验链：相位/存活 → def
/// 可雇 → 格归属且空闲 → 金够 → 人口够 → 扣费 → spawn_unit 落格。
/// def 表即价格（`UnitDef.cost_gold`）；人口走 def 的 `pop_cost`（L3）。
/// kit（Selectable/MoveAgent/Visibility）由 attach_legion_kit 观察器
/// 下一帧挂接（Added<UnitInstance>，覆盖一切 spawn 路径）。
#[allow(clippy::type_complexity)]
pub fn hire(
    mut commands_msg: MessageReader<HireCommand>,
    mut rejected: MessageWriter<HireRejected>,
    mut wallets: ResMut<Wallets>,
    flow: Res<RoundFlow>,
    lives: Res<PlayerLives>,
    slots: Query<(&DeploySlot, &Transform)>,
    mercs: Query<&Merc>,
    registry: Res<UnitRegistry>,
    diplomacy: Res<MatchDiplomacy>,
    mut commands: Commands,
) {
    for cmd in commands_msg.read() {
        let p = cmd.player as usize;
        if flow.phase != Phase::Build {
            rejected.write(HireRejected { reason: "not in build phase".into() });
            continue;
        }
        if lives.0.get(p).is_none_or(|&l| l == 0) {
            rejected.write(HireRejected { reason: "player eliminated".into() });
            continue;
        }
        let Some(def) = registry.get(&cmd.def).cloned() else {
            rejected.write(HireRejected { reason: "unknown def".into() });
            continue;
        };
        let Some(pop) = pop_cost(&registry, &cmd.def) else {
            rejected.write(HireRejected {
                reason: format!("{} is not hireable", cmd.def),
            });
            continue;
        };
        let Ok((slot, slot_tf)) = slots.get(cmd.slot) else {
            rejected.write(HireRejected { reason: "no such slot".into() });
            continue;
        };
        if slot.player != cmd.player {
            rejected.write(HireRejected { reason: "slot belongs to another lane".into() });
            continue;
        }
        if mercs.iter().any(|m| m.slot == cmd.slot) {
            rejected.write(HireRejected { reason: "slot occupied".into() });
            continue;
        }
        let wallet = wallets.of_mut(PlayerId(cmd.player));
        if wallet.gold < def.cost_gold {
            rejected.write(HireRejected {
                reason: format!("need {} gold, have {}", def.cost_gold, wallet.gold),
            });
            continue;
        }
        // Pop check against the LIVE field (the wallet lags one frame);
        // same-frame double hires see stale state — inherent to deferred
        // spawn, acceptable for LG1 (one hire per command).
        let live_pop: u8 = mercs.iter().filter(|m| m.player == cmd.player).map(|m| m.pop).sum();
        if live_pop + pop > wallet.pop_cap {
            rejected.write(HireRejected { reason: "population full".into() });
            continue;
        }
        wallet.gold -= def.cost_gold;
        let side = diplomacy.allegiance_of(PlayerId(cmd.player));
        if let Some(entity) = spawn_unit_deferred(
            &mut commands,
            &registry,
            &cmd.def,
            Vec3::new(slot_tf.translation.x, 0.0, slot_tf.translation.z),
            side,
            1.0,
        ) {
            commands.entity(entity).insert((
                Owner(PlayerId(cmd.player)),
                Merc { player: cmd.player, pop, slot: cmd.slot },
            ));
            info!(
                "[legion] P{p} hired {} for {}g (pop {pop})",
                cmd.def, def.cost_gold
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 建造期：发兵商店（LG2）
// ---------------------------------------------------------------------------

/// 建造期发兵（验收 ⑪ 的实现体）：花钱给指定对手注入下一回合的额外兵。
/// 校验链：相位 → 出资方存活 → 目标合法（自己/已出局拒绝）→ 目标可达
/// （bundle 的 lane 配对表里有「从出资方通往目标」的 lane——地图层同时
/// 锁定了敌对关系与走廊几何，同盟/跨走廊一律无通路）→ def 可发 →
/// 金够 → 扣费 → 记价格台账 → 入队 [`PendingSends`]（战斗期出征兑现）。
pub fn send(
    mut commands_msg: MessageReader<SendCommand>,
    mut rejected: MessageWriter<SendRejected>,
    mut wallets: ResMut<Wallets>,
    mut ledger: ResMut<SendLedger>,
    mut pending: ResMut<PendingSends>,
    flow: Res<RoundFlow>,
    lives: Res<PlayerLives>,
    lanes: Res<Lanes>,
    registry: Res<UnitRegistry>,
) {
    for cmd in commands_msg.read() {
        let p = cmd.player as usize;
        let t = cmd.target as usize;
        if flow.phase != Phase::Build {
            rejected.write(SendRejected { reason: "not in build phase".into() });
            continue;
        }
        if lives.0.get(p).is_none_or(|&l| l == 0) {
            rejected.write(SendRejected { reason: "player eliminated".into() });
            continue;
        }
        if t == p {
            rejected.write(SendRejected { reason: "cannot send to yourself".into() });
            continue;
        }
        if lives.0.get(t).is_none_or(|&l| l == 0) {
            rejected.write(SendRejected { reason: "target eliminated".into() });
            continue;
        }
        if !lanes.0.iter().any(|l| l.owner == p && l.target == t) {
            rejected
                .write(SendRejected { reason: format!("no lane from P{p} to P{t}") });
            continue;
        }
        if registry.get(&cmd.def).is_none() {
            rejected.write(SendRejected { reason: "unknown def".into() });
            continue;
        }
        if pop_cost(&registry, &cmd.def).is_none() {
            rejected.write(SendRejected {
                reason: format!("{} is not sendable", cmd.def),
            });
            continue;
        }
        let cost = send_cost(ledger.0[p][t]);
        let wallet = wallets.of_mut(PlayerId(cmd.player));
        if wallet.gold < cost {
            rejected.write(SendRejected {
                reason: format!("need {cost} gold, have {}", wallet.gold),
            });
            continue;
        }
        wallet.gold -= cost;
        ledger.0[p][t] += 1;
        pending.0.push(PendingSend { sponsor: cmd.player, target: cmd.target, def: cmd.def.clone() });
        info!(
            "[legion] P{p} sends {} at P{t} for {cost}g (send #{} to them)",
            cmd.def,
            ledger.0[p][t]
        );
    }
}

// ---------------------------------------------------------------------------
// 战斗期：漏怪 / 赏金 / 回合钟
// ---------------------------------------------------------------------------

/// 漏怪（验收 ⑤/⑫）：雇佣兵抵达对家底线 → 对家命 -1（基底 `PlayerLives`，
/// L5）；**发兵标记的兵（SentMerc）同时给出资方发击穿赏金 +5**（proposal
/// §2）。命尽 → 出局消息（该家单位清扫与胜负判定由基底 rules_tick 的
/// TeamLives 分支在下一帧接管——旗帜 kit 占位随之退役）。
pub fn leak_tick(
    lanes: Res<Lanes>,
    mut lives: ResMut<PlayerLives>,
    mut wallets: ResMut<Wallets>,
    mercs: Query<(Entity, &Transform, &Merc, Option<&SentMerc>)>,
    mut leaks: MessageWriter<LeakHappened>,
    mut eliminated: MessageWriter<PlayerEliminated>,
    mut commands: Commands,
) {
    let r2 = LEAK_RADIUS * LEAK_RADIUS;
    for (entity, tf, merc, sent) in &mercs {
        let Some(lane) = lanes.0.iter().find(|l| l.owner == merc.player as usize) else { continue };
        let dx = tf.translation.x - lane.baseline.x;
        let dz = tf.translation.z - lane.baseline.y;
        if dx * dx + dz * dz > r2 {
            continue;
        }
        let victim = lane.target;
        let before = lives.0.get(victim).copied().unwrap_or(0);
        if before > 0 {
            lives.0[victim] = before - 1;
        }
        if sent.is_some() {
            // Breach bounty: only SENT units pay their sponsor on breaking
            // through (own hires leaking cost the victim the same life but
            // earn nothing — the send premium is the reward for the bet).
            wallets.of_mut(PlayerId(merc.player)).gold += BREACH_BOUNTY;
            info!(
                "[legion] breach bounty +{BREACH_BOUNTY}g to P{} (sent unit broke P{victim}'s line)",
                merc.player
            );
        }
        commands.entity(entity).despawn();
        leaks.write(LeakHappened { attacker: merc.player, victim: victim as u8 });
        info!(
            "[legion] leak: P{} breaches P{victim}'s line (lives {})",
            merc.player,
            lives.0.get(victim).copied().unwrap_or(0)
        );
        if before == 1 {
            // Out: the base's TeamLives rule sweeps their units and
            // adjudicates next frame; this message is the game-side
            // observation seam (HUD/tests).
            eliminated.write(PlayerEliminated { player: victim as u8 });
            info!("[legion] P{victim} eliminated (no lives left)");
        }
    }
}

/// 回合钟（验收 ③/⑥/⑪/⑬/⑭）：建造期帧计 1800 → 战斗期（兑现发兵队列
/// + 发令一次）；战斗期战场清空（无存活 Merc）→ 即时结算（存活家 +50 金
/// + 击杀赏金已由基底 apply_econ 按 killer Owner 入账）→ 下一回合，或第
/// [`ROUND_CAP`] 回合结算时按剩余命数定胜负（命数多者胜，平数平局——基底
/// L7 的 `MatchFlow::Ended{Win|Draw}` 转正）。MatchFlow 终局后冻结（中途
/// 出局的 LastTeamStanding-on-lives 已由基底 L5 收口）。
///
/// 发令走基底 `UnitCommand`（AttackMove 至对家底线）——MOV4 的攻击性
/// 移动语义：途中遇敌自动交战（acquire_engagement），杀完续走。
/// 发兵注入走基底 L8 的 `spawn_owned_deferred`（有主 spawn，行军模式
/// AttackMove，**出征令在出生即武装**——观察器晚一帧挂 kit 的同帧令坑
/// 由缝本身消除）；游戏侧只补 Merc/SentMerc 标记与可见性。
#[allow(clippy::type_complexity)]
pub fn round_tick(
    mut flow: ResMut<RoundFlow>,
    mut wallets: ResMut<Wallets>,
    lives: Res<PlayerLives>,
    lanes: Res<Lanes>,
    diplomacy: Res<MatchDiplomacy>,
    registry: Res<UnitRegistry>,
    paths: Res<Paths>,
    mut pending: ResMut<PendingSends>,
    mercs: Query<(Entity, &Merc)>,
    mut match_flow: ResMut<MatchFlow>,
    mut ended: MessageWriter<MatchEnded>,
    mut unit_commands: MessageWriter<UnitCommand>,
    mut phases: MessageWriter<PhaseChanged>,
    mut settled: MessageWriter<RoundSettled>,
    mut commands: Commands,
) {
    if *match_flow != MatchFlow::Running {
        return;
    }
    match flow.phase {
        Phase::Build => {
            flow.frame += 1;
            if flow.frame >= BUILD_FRAMES {
                flow.phase = Phase::Battle;
                flow.frame = 0;
                phases.write(PhaseChanged { round: flow.round, phase: Phase::Battle });
                info!("[legion] round {} -> battle", flow.round);

                // Cash the send queue (LG2, via the base L8 seam): each
                // pending send spawns at its sponsor's deploy-zone mouth
                // (their lane start), fully kitted, ready to join the
                // march order below.
                let mut sent_squads: BTreeMap<u8, Vec<Entity>> = BTreeMap::new();
                for s in pending.0.drain(..) {
                    let Some(lane) = lanes
                        .0
                        .iter()
                        .find(|l| l.owner == s.sponsor as usize && l.target == s.target as usize)
                    else {
                        continue; // validated at send time; map changed -> drop
                    };
                    let to = Vec3::new(lane.baseline.x, 0.0, lane.baseline.y);
                    let request = SpawnRequest {
                        def: s.def.clone(),
                        owner: PlayerId(s.sponsor),
                        pos: Vec3::new(lane.start.x, 0.0, lane.start.y),
                        hp_scale: 1.0,
                        march: MarchMode::AttackMove { to },
                    };
                    if let Some(entity) = spawn_owned_deferred(
                        &mut commands,
                        &registry,
                        &diplomacy,
                        &paths,
                        &request,
                    ) {
                        commands.entity(entity).insert((
                            // pop 0: sends book no population (they are an
                            // injection, not a hire); slot PLACEHOLDER keeps
                            // the occupancy checks inert (they only match
                            // real DeploySlot entities).
                            Merc { player: s.sponsor, pop: 0, slot: Entity::PLACEHOLDER },
                            SentMerc,
                            Selectable,
                            Visibility::default(),
                        ));
                        sent_squads.entry(s.sponsor).or_default().push(entity);
                        info!(
                            "[legion] sent {} deploys at P{}'s zone, marching at P{}",
                            s.def, s.sponsor, s.target
                        );
                    }
                }

                // One AttackMove order per player squad (BTreeMap: deterministic
                // message order; subjects keep query order). Sent units join
                // their sponsor's squad — same corridor, same baseline.
                let mut squads: BTreeMap<u8, Vec<Entity>> = BTreeMap::new();
                for (e, merc) in &mercs {
                    squads.entry(merc.player).or_default().push(e);
                }
                for (sponsor, subjects) in sent_squads {
                    squads.entry(sponsor).or_default().extend(subjects);
                }
                for (player, subjects) in squads {
                    let Some(lane) =
                        lanes.0.iter().find(|l| l.owner == player as usize)
                    else {
                        continue;
                    };
                    let to = Vec3::new(lane.baseline.x, 0.0, lane.baseline.y);
                    let count = subjects.len();
                    unit_commands.write(UnitCommand {
                        order: Order::AttackMove { to },
                        subjects,
                    });
                    info!(
                        "[legion] battle: P{player} marches {count} unit(s) to ({}, {})",
                        lane.baseline.x,
                        lane.baseline.y
                    );
                }
            }
        }
        Phase::Battle => {
            // NOTE: the chained leak_tick → recount_population → round_tick
            // edge lets the scheduler auto-insert a sync point, so leak
            // despawns apply BEFORE this system runs — a field that leaked
            // its last merc this frame settles this frame (the breach-bounty
            // acceptance reads exactly this timing).
            if mercs.is_empty() {
                for (p, wallet) in wallets.0.iter_mut().enumerate() {
                    if lives.0.get(p).is_none_or(|&l| l > 0) {
                        wallet.gold += INCOME_PER_ROUND;
                    }
                }
                settled.write(RoundSettled { round: flow.round, income: INCOME_PER_ROUND });
                info!(
                    "[legion] round {} settled (+{INCOME_PER_ROUND}g to the living)",
                    flow.round
                );
                if flow.round >= ROUND_CAP {
                    // Round-cap adjudication (LG2, proposal §2; L7 转正):
                    // the team with more lives wins; equal leaders draw —
                    // written through the base's structured ending.
                    // Coexists with the base L5 mid-round eliminations
                    // (LastTeamStanding-on-lives crowns first — round_tick
                    // is frozen by then and never reaches this arm).
                    let ranked = team_lives(&lives, &diplomacy);
                    let detail = ranked
                        .iter()
                        .map(|(t, l)| format!("t{}={l}", t.0))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let max = ranked.iter().map(|(_, l)| *l).max().unwrap_or(0);
                    let leaders = ranked.iter().filter(|(_, l)| *l == max).count();
                    let (reason, outcome) = if leaders == 1 {
                        let team = ranked.iter().find(|(_, l)| *l == max).unwrap().0;
                        (
                            format!("round cap {ROUND_CAP}: team {} wins on lives ({detail})", team.0),
                            MatchOutcome::Win(team),
                        )
                    } else {
                        (format!("round cap {ROUND_CAP}: draw on lives ({detail})"), MatchOutcome::Draw)
                    };
                    *match_flow = MatchFlow::Ended { reason: reason.clone(), outcome };
                    ended.write(MatchEnded { outcome: Ok(reason.clone()) });
                    if let MatchOutcome::Win(team) = outcome {
                        commands.insert_resource(MatchWinner(team));
                    }
                    info!("[legion] match ended: {reason}");
                    // Frozen: MatchFlow != Running keeps every later tick a
                    // no-op; round stays at the cap.
                } else {
                    flow.round += 1;
                    flow.phase = Phase::Build;
                    flow.frame = 0;
                    phases.write(PhaseChanged { round: flow.round, phase: Phase::Build });
                }
            } else {
                flow.frame += 1;
            }
        }
    }
}

/// 队伍命数汇总（封顶判定用）：`PlayerLives` 按 MatchDiplomacy 的玩家→
/// 队伍表折算成 per-team 总命数，BTreeMap 保序（确定性 reason 字符串）。
fn team_lives(lives: &PlayerLives, diplomacy: &MatchDiplomacy) -> Vec<(TeamId, u32)> {
    let mut sums: BTreeMap<TeamId, u32> = BTreeMap::new();
    for &(player, team) in &diplomacy.players {
        let l = lives.0.get(player.0 as usize).copied().unwrap_or(0);
        *sums.entry(team).or_insert(0) += l;
    }
    sums.into_iter().collect()
}

/// 人口重算：used = 存活 Merc 的 pop 之和，写回基底 `Wallets` 的 per-player
/// pop 字段（每帧；死亡/漏怪自然释放，单一事实源 = 场上实体）。雇兵校验
/// 用的钱包因此最多滞后一帧，hire 侧再用场上实体复核（见 hire 的 mercs
/// 查询）。
pub fn recount_population(mercs: Query<&Merc>, mut wallets: ResMut<Wallets>) {
    let mut used = [0u8; 4];
    for merc in &mercs {
        let p = merc.player as usize;
        used[p] = used[p].saturating_add(merc.pop);
    }
    for (p, wallet) in wallets.0.iter_mut().enumerate() {
        wallet.pop_used = used.get(p).copied().unwrap_or(0);
    }
}

// ---------------------------------------------------------------------------
// 真机输入桥（headless 无 ButtonInput 自动 no-op；tests 直接写 HireCommand）
// ---------------------------------------------------------------------------

/// 本地玩家槽位（无 netcode；H1 真机只玩 P0）。与基底 `LocalPlayer`
/// 资源（默认 P0）同值——钱包本地视图即 P0。
pub const LOCAL_PLAYER: u8 = 0;

/// 数字键 1-6：往「当前选中的自家布阵格」雇佣对应兵种（命令卡 =
/// def 表即价格）。选中格来自基底 selection（DeploySlot 也是 Selectable）。
pub fn hire_input(world: &mut World) {
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>().cloned() else {
        return; // headless
    };
    let hotkeys = [
        (KeyCode::Digit1, 0usize),
        (KeyCode::Digit2, 1),
        (KeyCode::Digit3, 2),
        (KeyCode::Digit4, 3),
        (KeyCode::Digit5, 4),
        (KeyCode::Digit6, 5),
    ];
    let Some(&(_, idx)) = hotkeys.iter().find(|(k, _)| keys.just_pressed(*k)) else {
        return;
    };
    let def = crate::systems::gameplay::MERCENARY_DEFS[idx];
    let selection = world.resource::<SelectionState>().entities.clone();
    let Some(slot) = selection.iter().copied().find(|&e| {
        world.get::<DeploySlot>(e).is_some_and(|s| s.player == LOCAL_PLAYER)
    }) else {
        info!("[legion] {def}: select one of your deploy cells first");
        return;
    };
    world.write_message(HireCommand { player: LOCAL_PLAYER, def: def.into(), slot });
}

/// Space：建造期跳过剩余等待（把帧钟拨到临门一帧，round_tick 立即转战斗）。
pub fn skip_build_input(world: &mut World) {
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>().cloned() else {
        return; // headless
    };
    if !keys.just_pressed(KeyCode::Space) {
        return;
    }
    let mut flow = world.resource_mut::<RoundFlow>();
    if flow.phase == Phase::Build && flow.frame + 1 < BUILD_FRAMES {
        flow.frame = BUILD_FRAMES - 1;
        info!("[legion] build phase skipped");
    }
}

/// G：建造期往本路对家发 1 个 swarm（默认发兵兵种——快兵骚扰定位；
/// 价格走 send_cost 递增表，非 def 表价）。目标取 lane 配对（本图即
/// 唯一对家）；LG3 的命令卡会把「选目标玩家 + 选兵种」补成完整入口。
pub fn send_input(world: &mut World) {
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>().cloned() else {
        return; // headless
    };
    if !keys.just_pressed(KeyCode::KeyG) {
        return;
    }
    let Some(lane) = world
        .resource::<Lanes>()
        .0
        .iter()
        .find(|l| l.owner == LOCAL_PLAYER as usize)
    else {
        return;
    };
    world.write_message(SendCommand {
        player: LOCAL_PLAYER,
        target: lane.target as u8,
        def: "merc_swarm".into(),
    });
}
