//! Round layer (LG1): the game-side state machine ON TOP of the base's
//! MatchFlow — 建造期 (frame-counted 30s) → 战斗期 (auto march + auto combat)
//! → 即时结算 (+50g income + kill bounties) → next round.
//!
//! Thin-game rule: every mechanism is the base's — orders go out as
//! `UnitCommand::AttackMove` (MOV4 tick_orders consumes), fighting is base
//! combat (pick_targets / fire_attacks / resolve_damage), kill attribution
//! rides `UnitDied.killer` (DamageOrigin). Game-side additions are exactly
//! the round clock, the hire shop, per-player wallets/lives/pop, the leak
//! rule and settlement — the seams the base does not carry (see kpi.md).

use bevy::prelude::*;
use std::collections::BTreeMap;
use warbase::prelude::*;

use crate::components::{DeploySlot, Merc};
use crate::resources::{
    CurrentMap, Lane, Lanes, LivesBoard, Phase, PopLedger, Purse, RoundFlow, BUILD_FRAMES,
};
use warbase::bundle::BuildDomainDef;

/// 每回合基础收入（proposal §2：50 基础 + 击杀赏金）。
pub const INCOME_PER_ROUND: u32 = 50;
/// 漏怪判定半径：AttackMove 目标即底线点，arrival brake 0.1 内停步，
/// 0.9 半径保证行进中先于停步扣分。
pub const LEAK_RADIUS: f32 = 0.9;
/// lane 配对（发兵目标/漏怪受害方）：P0<->P1，P2<->P3。
pub const OPPONENT: [usize; 4] = [1, 0, 3, 2];

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

/// 玩家出局（命尽；LastTeamStanding 的出局信号）。
#[derive(Message, Debug, Clone, Copy)]
pub struct PlayerEliminated {
    pub player: u8,
}

// ---------------------------------------------------------------------------
// 人口表（hole L3：UnitDef 无人口字段，def id -> pop 游戏侧记账）
// ---------------------------------------------------------------------------

/// 雇佣兵人口占用（proposal §2 锚点 1-4）。非雇佣兵 def 返回 None
/// （= 不可雇）。
pub fn pop_cost(def: &str) -> Option<u8> {
    match def {
        "merc_cheap" => Some(1),
        "merc_swarm" => Some(2),
        "merc_tank" => Some(3),
        "merc_ranged" => Some(2),
        "merc_aoe" => Some(3),
        "merc_support" => Some(2),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Startup：lane/布阵格/命数/人口 装配
// ---------------------------------------------------------------------------

/// Exclusive startup (after load_map): derive the game-side round state from
/// the assembled world — lanes from `world.paths` (order = player slot; the
/// pairing table is game-side), deploy-slot entities from the build def
/// (assigned to the nearest lane start; corridors are 8 apart, cells 1.0 —
/// unambiguous), lives from `rules.leak`'s Lives anchor, pop cap from
/// `econ_start.pop_cap`.
pub fn init_round_state(world: &mut World) {
    let bundle = world.resource::<CurrentMap>().0.clone();

    let mut lanes = Vec::new();
    for (i, path) in bundle.world.paths.iter().enumerate() {
        let first = path.points.first().map(|p| Vec2::new(p[0], p[1])).unwrap_or_default();
        let last = path.points.last().map(|p| Vec2::new(p[0], p[1])).unwrap_or_default();
        let target = OPPONENT.get(i).copied().unwrap_or(0);
        lanes.push(Lane { owner: i, start: first, baseline: last, target });
    }

    // Deploy slots from the FixedSlots build def (order = authored order).
    let mut slots: Vec<[f32; 2]> = Vec::new();
    if let BuildDomainDef::FixedSlots { slots: authored } = &bundle.world.build {
        slots = authored.clone();
    }
    for (idx, [x, z]) in slots.iter().enumerate() {
        // Nearest lane start decides the owner (ties: lowest lane index —
        // deterministic; the corridor spacing makes ties unreachable).
        let owner = lanes
            .iter()
            .enumerate()
            .min_by(|(a, l), (b, r)| {
                let da = l.start.distance_squared(Vec2::new(*x, *z));
                let db = r.start.distance_squared(Vec2::new(*x, *z));
                da.total_cmp(&db).then(a.cmp(b))
            })
            .map(|(_, l)| l.owner)
            .unwrap_or(0) as u8;
        world.spawn((
            DeploySlot { player: owner, index: idx as u8 },
            Selectable,
            Visibility::default(),
            Transform::from_xyz(*x, 0.0, *z),
        ));
    }

    let lives = match &bundle.rules.leak {
        LeakPolicy::Lives { lives } => *lives,
        _ => 20,
    };
    let cap = bundle.econ_start.pop_cap;
    world.insert_resource(Lanes(lanes));
    world.insert_resource(LivesBoard([lives; 4]));
    world.insert_resource(PopLedger { used: [0; 4], cap: [cap; 4] });
    world.insert_resource(RoundFlow { round: 1, phase: Phase::Build, frame: 0 });
    info!(
        "[legion] round state ready: {} lanes, {} slots, {lives} lives x4",
        bundle.world.paths.len(),
        slots.len()
    );
}

// ---------------------------------------------------------------------------
// 建造期：雇佣兵商店
// ---------------------------------------------------------------------------

/// 建造期买兵（消息驱动；验收 ② 的实现体）。校验链：相位/存活 → def
/// 可雇 → 格归属且空闲 → 金够 → 人口够 → 扣费 → spawn_unit 落格。
/// def 表即价格（`UnitDef.cost_gold`）；人口走 [`pop_cost`] 表。
/// kit（Selectable/MoveAgent/Visibility）由 attach_legion_kit 观察器
/// 下一帧挂接（Added<UnitInstance>，覆盖一切 spawn 路径）。
#[allow(clippy::type_complexity)]
pub fn hire(
    mut commands_msg: MessageReader<HireCommand>,
    mut rejected: MessageWriter<HireRejected>,
    mut purse: ResMut<Purse>,
    ledger: Res<PopLedger>,
    flow: Res<RoundFlow>,
    lives: Res<LivesBoard>,
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
        let Some(pop) = pop_cost(&cmd.def) else {
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
        if purse.0[p] < def.cost_gold {
            rejected.write(HireRejected {
                reason: format!("need {} gold, have {}", def.cost_gold, purse.0[p]),
            });
            continue;
        }
        // Pop check against the LIVE field (the ledger lags one frame);
        // same-frame double hires see stale state — inherent to deferred
        // spawn, acceptable for LG1 (one hire per command).
        let live_pop: u8 = mercs.iter().filter(|m| m.player == cmd.player).map(|m| m.pop).sum();
        if live_pop + pop > ledger.cap[p] {
            rejected.write(HireRejected { reason: "population full".into() });
            continue;
        }
        purse.0[p] -= def.cost_gold;
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
// 战斗期：漏怪 / 赏金 / 回合钟
// ---------------------------------------------------------------------------

/// 漏怪（验收 ⑤）：雇佣兵抵达对家底线 → 对家命 -1；命尽 → 出局（该家
/// 全部单位移除 —— 主城的旗帜 kit 随之消失，LastTeamStanding 由基底
/// rules_tick 在下一帧收口）。
pub fn leak_tick(
    lanes: Res<Lanes>,
    mut lives: ResMut<LivesBoard>,
    mercs: Query<(Entity, &Transform, &Merc)>,
    owned: Query<(Entity, &Owner)>,
    mut leaks: MessageWriter<LeakHappened>,
    mut eliminated: MessageWriter<PlayerEliminated>,
    mut commands: Commands,
) {
    let r2 = LEAK_RADIUS * LEAK_RADIUS;
    for (entity, tf, merc) in &mercs {
        let Some(lane) = lanes.0.iter().find(|l| l.owner == merc.player as usize) else { continue };
        let dx = tf.translation.x - lane.baseline.x;
        let dz = tf.translation.z - lane.baseline.y;
        if dx * dx + dz * dz > r2 {
            continue;
        }
        let victim = lane.target;
        lives.0[victim] = lives.0[victim].saturating_sub(1);
        commands.entity(entity).despawn();
        leaks.write(LeakHappened { attacker: merc.player, victim: victim as u8 });
        info!(
            "[legion] leak: P{} breaches P{victim}'s line (lives {})",
            merc.player, lives.0[victim]
        );
        if lives.0[victim] == 0 {
            // Out: sweep every unit they own (mercs anywhere + the keep).
            for (e, owner) in &owned {
                if owner.0 .0 as usize == victim {
                    commands.entity(e).despawn();
                }
            }
            eliminated.write(PlayerEliminated { player: victim as u8 });
            info!("[legion] P{victim} eliminated (no lives left)");
        }
    }
}

/// 击杀赏金（验收 ⑥ 的一半）：UnitDied 的 killer 归因到玩家的钱包，
/// 金额 = def 的 bounty_gold（数值在地图）。注：基底 apply_econ 同时把
/// 赏金记入全局 Resources（单人时代的单例）——军团图不消费那份账，
/// 双记账是基底单人经济缝的摩擦（kpi.md L2）。
pub fn bounty_credit(
    mut died: MessageReader<UnitDied>,
    owners: Query<&Owner>,
    mut purse: ResMut<Purse>,
) {
    for d in died.read() {
        if d.gold == 0 {
            continue;
        }
        let Some(DamageOrigin::Attack(killer)) = d.killer else { continue };
        let Ok(owner) = owners.get(killer) else { continue };
        let p = owner.0 .0 as usize;
        purse.0[p] += d.gold;
    }
}

/// 回合钟（验收 ③/⑥）：建造期帧计 1800 → 战斗期（发令一次）；战斗期
/// 战场清空（无存活 Merc）→ 即时结算（存活家 +50 金）→ 下一回合。
/// MatchFlow 终局后冻结（LastTeamStanding 已收口）。
///
/// 发令走基底 `UnitCommand`（AttackMove 至对家底线）——MOV4 的攻击性
/// 移动语义：途中遇敌自动交战（acquire_engagement），杀完续走。
#[allow(clippy::type_complexity)]
pub fn round_tick(
    mut flow: ResMut<RoundFlow>,
    mut purse: ResMut<Purse>,
    lives: Res<LivesBoard>,
    lanes: Res<Lanes>,
    mercs: Query<(Entity, &Merc)>,
    match_flow: Res<MatchFlow>,
    mut unit_commands: MessageWriter<UnitCommand>,
    mut phases: MessageWriter<PhaseChanged>,
    mut settled: MessageWriter<RoundSettled>,
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
                // One AttackMove order per player squad (BTreeMap: deterministic
                // message order; subjects keep query order).
                let mut squads: BTreeMap<u8, Vec<Entity>> = BTreeMap::new();
                for (e, merc) in &mercs {
                    squads.entry(merc.player).or_default().push(e);
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
            // NOTE: leak/despawn commands land after this frame's schedule,
            // so the field-empty check sees the last merc one frame late —
            // settlement observes a truly settled field, by design.
            if mercs.is_empty() {
                for (p, &l) in lives.0.iter().enumerate() {
                    if l > 0 {
                        purse.0[p] += INCOME_PER_ROUND;
                    }
                }
                settled.write(RoundSettled { round: flow.round, income: INCOME_PER_ROUND });
                info!(
                    "[legion] round {} settled (+{INCOME_PER_ROUND}g to the living)",
                    flow.round
                );
                flow.round += 1;
                flow.phase = Phase::Build;
                flow.frame = 0;
                phases.write(PhaseChanged { round: flow.round, phase: Phase::Build });
            } else {
                flow.frame += 1;
            }
        }
    }
}

/// 人口重算：used = 存活 Merc 的 pop 之和（每帧；死亡/漏怪自然释放，
/// 单一事实源 = 场上实体）。雇兵校验用的 ledger 因此最多滞后一帧，
/// hire 侧再用场上实体复核（见 hire 的 mercs 查询）。
pub fn recount_population(mercs: Query<&Merc>, mut ledger: ResMut<PopLedger>) {
    let mut used = [0u8; 4];
    for merc in &mercs {
        let p = merc.player as usize;
        used[p] = used[p].saturating_add(merc.pop);
    }
    ledger.used = used;
}

// ---------------------------------------------------------------------------
// 真机输入桥（headless 无 ButtonInput 自动 no-op；tests 直接写 HireCommand）
// ---------------------------------------------------------------------------

/// 本地玩家槽位（无 netcode；H1 真机只玩 P0）。
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
