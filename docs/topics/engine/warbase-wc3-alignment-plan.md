---
title: warbase 魔兽式底座补全方案 v0.1——面向 WC3 交互模板的基底扩域总规格（子智能体并行开发底稿）
type: reference
topic: engine
date: 2026-10-22
author: AI 起草（ZCode），待团队 review
status: draft
tags: [architecture, runtime, warbase, bevy, wc3, plan, parallel-development]
related: ["[[decisions/0012-warbase-shared-runtime-and-map-architecture|ADR-0012]]", "[[topics/engine/warbase-implementation-design|warbase 技术实现设计]]", "[[topics/engine/warbase-architecture|warbase 架构与演化协议]]", "[[topics/engine/warbase-test-strategy|warbase 测试策略]]", "[[decisions/0013-mapbundle-as-data-file|ADR-0013]]"]
---

# warbase 魔兽式底座补全方案 v0.1

> 本文是基底从「TD 品类魔兽式战斗 runtime」扩域到「通用魔兽（WC3）式游戏底子」的**总规格**。
> 用途：① 团队评审拍板；② 后续每个增量子智能体的规格输入（配合 §11 任务包模板）。
> 流程遵 ADR-0014（规格-测试-小步提交）；范围修订遵 ADR-0012（先写 ADR 再动代码）。
> **美术不在本轮范围**：一切视觉用 bevy 原生（node/纯色/文字）灰盒表达；音效只做语义消息通道不挂资产。

## 1. 目标与范围

**目标**：把操作交互对齐 WC3 模板——单位选择/框选/编队、右键智能命令、A/H/S/P 命令集、自由移动与寻路、法力与施法时间、生产训练与科研、物品与商店、视野迷雾、多玩家阵营数据层、录像回放、音效语义通道——全部以**基底域**形态落进 `engine/warbase`，消费方（warden / loop-td / 新 RTS 验收图）零散件重写。

**明确不做**（边界，防 agent 越界）：

| 不做 | 理由 |
|------|------|
| 视觉资产 / 视觉基线（VIS 系） | 美术后置（用户指令）；灰盒 bevy 原生 |
| 音效资产 | 同上；只发语义消息 |
| 网络多人（netcode） | 本方案只做多玩家**数据层**（队伍/外交/共享视野）；netcode 另立 ADR |
| 光照遮挡迷雾（高低差视线遮挡） | 一期圆形视野；写进 §12 开放问题 |
| warden 现有玩法数值改动 | 兼容铁律见 D8 |

**总验收纲**：`cargo test --workspace` 一击全绿；warden 161 + loop-td 既有断言**零漂移**；新域断言数字化（§3–§10 各域）；`cargo build -p warbase --no-default-features` 零游戏依赖零警告（ADR-0012 护栏不变）。

## 2. 横切架构决策 D1–D10（所有增量先钉死，禁各自发明）

- **D1 模块与 feature**：新增模块 `selection`（纯逻辑，无 feature）、`movement`（纯逻辑）、`vision`（纯逻辑）、`item`、`production`、`replay`、`sfx`；指针→世界坐标薄层 `input` 挂 `#[cfg(feature = "ui")]`。逻辑层一律消息/命令驱动，headless 可测；feature 只包"碰窗口/相机"的薄层。
- **D2 调度落位**：**不加新 SystemSet 段**（保持 Tick→Director→Intent→Act→Resolve→Cleanup 六段）。新系统落位：输入拾取与录像拦截挂 `Update` 最前（`before(WarbaseSet::Tick)`）；命令消费/法力恢复/生产推进/技能施法推进在 `Tick`；移动推进/分离在 `Director`；命令意图分发与移动中索敌接缝在 `Intent`；状态对账（replay hash）在 `Cleanup` 后。各模块头注释写明落位。
- **D3 prelude 分域**：每个新模块自带 `pub mod prelude`（模块内 pub use）；crate `prelude` 里 SEAM 一次写好 `pub use crate::xxx::prelude::*;`。**后续域增量禁改 `lib.rs`**——这是并行的硬前提。
- **D4 命令即消息**：玩家可发起操作统一走命令消息（既有 EconCommand/CastSkill/StartWave 之外新增 UnitCommand/SelectCommand/GroupCommand/TrainCommand/CommandEnvelope），`Tick` 段统一消费，同帧多条按到达序（MessageCursor 语义）。直调函数（try_place 等）保留但命令消息是唯一玩家入口。
- **D5 索敌与迷雾分离**：`pick_targets` 与技能目标校验的敌我判定收口到 `HostilityOracle`（SEAM 抽出，MP1 换实现）；**塔的自动索敌/自动施放不受玩家迷雾影响**（塔自带 VisionSource），迷雾只约束玩家交互层（手动技能落点/选中/渲染）。此规则写进 `combat/mod.rs` 模块头，MP1 与 FOG2 都不许改 `pick_targets` 本体。
- **D6 确定性纪律**（replay 的地基）：新域随机只准走 `MatchRng`；禁 HashMap 迭代驱动逻辑（用 BTreeMap/Vec/固定顺序迭代）；A* tie-break 固定；不开 fast-math（Rust 默认 IEEE 即满足）。回放承诺范围=同机器同 build。
- **D7 MapBundle 向后兼容**：新字段全部 `Option`/`serde(default)`，旧 bundle（warden.ron/loop.ron/maze-smoke.ron）**零改动可载**。各域装载逻辑放各域模块 `pub fn apply_<domain>(world, &MapBundle)`；`bundle/mod.rs` 的分发行由 SEAM 一次写完（调用各域空实现），后续域增量**禁改 bundle/mod.rs**。
- **D8 兼容铁律**：Allegiance 组件保留（MP1 起由 Owner+外交派生刷新，warden 读点零改动）；SkillDef 新字段全部带默认值（mana_cost=0 / cast_time=0）→ warden「冷却驱动免费施放」现状零漂移；每增量验收含既有 workspace 测试全绿。
- **D9 UI 灰盒纪律**：一切新 UI 用 bevy 原生 node/纯色/文字；选中反馈=纯色描边/圆环 shape；不做贴图不做动画资产。
- **D10 插件选型**：客观技术优先引插件（AGENTS.md）。SEL1a 先 spike 验证 Bevy 0.19 官方 picking 形态（事件/遮挡/正交相机），可用则输入层基于它做桥，不可用则自写正交反投影；寻路/分离**自研**（与 BuildGrid 网格语义强耦合、属主观玩法层）。任何插件引入前先 spike 卡，不直接写进实现。

## 3. SEAM：Phase 0 接缝增量（主线单会话，其余一切并行的前提）

| 增量 | 内容 |
|------|------|
| SEAM0a | ADR-0016（基底范围扩域：选择/移动/寻路/生产/物品/视野/录像进基底，修订 ADR-0012 §3 范围条）+ ADR-0017（多玩家数据层：队伍/外交/共享视野；不含 netcode）起草，用户拍板 |
| SEAM0b | `lib.rs` 骨架：七个新 `mod` 声明 + 各自空 `prelude` + crate prelude 通配行；`Order` 枚举（见 §5 MOV）与 `CommandEnvelope`（见 §10 RPL）**定义在此**（SEL4 与 MOV 并行的解耦点） |
| SEAM0c | `HostilityOracle` 抽取：combat 敌我判定收口为 `fn is_hostile(world, a, b) -> bool`（初版=Allegiance 判断，行为零漂移）；combat/skill/光环过滤改调用它 |
| SEAM0d | MapBundle 新字段占位（teams/items/research/production/nav/vision 全 Option+default）+ `bundle/mod.rs` 分发行调各域 `apply_xxx` 空实现（空实现放各域模块文件） |
| SEAM0e | `testing.rs` 扩展骨架：spawn 可选单位/命令注入/拾取注入 helper；本方案 §11 任务包模板存档 |

验收：workspace 全绿；`--no-default-features` 零警告；warden/loop-td 断言数不变。

## 4. SEL 域：输入拾取 / 选择集 / 编队 / 智能命令

| 增量 | 内容 | 依赖 | 批次 |
|------|------|------|------|
| SEL1a | bevy_picking spike（0.19 形态/正交/遮挡验证，出结论卡，30 分钟预算） | — | P1 |
| SEL1b | `WorldPicker` 缝：`screen_to_world(cam, vp, pos)` trait 资源 + OrthoTopDown 默认实现（ui feature）；`PickAttempt{screen, world, button, click_count, mods}` 消息；`resolve_picks`（Update 最前）：点/圆命中 `Pickable` 实体（body_radius 圆命中）或 BuildGrid 格 → `PickResolved`。**坑**：UI 面板必须挡世界拾取 | SEAM | P1 |
| SEL2 | 选择集（纯逻辑）：`Selectable`/`Selected` 组件 + `SelectionState{entities: Vec<Entity>（按 Entity id 升序）, subgroup_cursor}` 资源 + `SelectCommand{Set/Add/Toggle/Aabb(Rect)/SameTypeOnScreen/Clear}` 消息（Tick 段消费）；编队 `ControlGroups`（10 组，Vec<Entity>，死亡实体 Cleanup 段清除）+ `GroupCommand{Bind/Recall/Insert}`；Tab 子组循环=按 UnitId 分桶排序后 cursor+1 | SEAM | P1 |
| SEL3 | 框选与多选 UI（ui feature）：拖拽矩形 → `SelectCommand::Aabb`；灰盒框+多选色块网格+单选信息面板数据层 `SelectionPanelData`（name/hp/mana/attrs/skills/items 占位字段） | SEL2 | P1 |
| SEL4 | 右键智能命令 + 命令热键（ui feature）：`smart_resolve`（Intent 前）：右键敌→逐选中单位发 `UnitCommand::AttackTarget`；地面→Move（走 MOV5 散点）；地上物品→Pickup。A/H/S/P 热键（A 进入 PendingTargetOrder 态，Esc 统一取消——沿 warden §16.3 法则）。**数字键归属**：模态期归模态（三选一 1/2/3），非模态归编队 | Order 枚举（SEAM0b），不需要 MOV 实现 | P2 |
| SEL5 | warden 拾取上收（A 通道）：warden `pointer.rs` 切基底 WorldPicker/PickAttempt/PickResolved，行为断言零漂移；loop-td 保留键盘光标（品类决定） | SEL1b/SEL2 | P3 |

**关键决策**：选择逻辑只认 SelectCommand（输入语义翻译在 input 层），headless 直发命令即测；编队/子组顺序全部确定性排序（D6）。

**验收断言**：`Set` 5 实体→len==5 且 id 升序；Bind 组 1 → Clear → Recall 恢复 5；杀 1 → Recall 为 4 且无死引用；Tab：3 弓手+2 塔组 → 子组序列 [archer,tower] 环走 4 步回位；Aabb 覆盖 3/5 恰 3 入选（body_radius 相交算命中，边界 ε=1e-3 写死）；右键敌 → N 个选中单位各 1 条 AttackTarget；右键地 → N 条 Move 且目标点互异（散点生效）。

## 5. MOV 域：自由移动 / 命令状态机 / 网格 A* / 软碰撞 / 阵型

| 增量 | 内容 | 依赖 | 批次 |
|------|------|------|------|
| MOV1 | `MoveAgent{velocity, arrival_radius}` + `AgentOrder(Option<Order>)` + `AgentOrderState{Idle/Moving/Engaging/Patrol/Chase}`；`move_agents`（Director 段，与 `advance_followers` 并列）：匀速朝 waypoint+到点刹停。MoveAgent 与 PathFollower 互斥（同实体 debug_assert 拒装——轨道怪与自由单位两世界并存） | SEAM | P2 |
| MOV2 | 网格 A*：`NavGrid`（格距=BuildGrid 同参；阻挡=Buildable 占格+bundle 静态阻挡+大体型膨胀 1 圈）；`astar()` 纯函数（禁对角穿角：两正交邻格皆通才可对角；tie-break 固定方向序）；路径拉直后处理；`PathRequestQueue` 每帧预算 8 次（未完成期间直线 best-effort）；塔放/卖→脏标记批量重算（分帧） | MOV1 | P2 |
| MOV3 | 软碰撞分离：`separate_agents`（Director 后）：空间网格分桶两两推位（非破坏：穿透>50% 不推防抖动）；桶内按 Entity id 排序；PathFollower 怪不参与（TD 现状保持，规则写死） | MOV1 | P3 |
| MOV4 | 命令状态机执行 `tick_orders`（Tick 段）：AttackMove=沿路移动+每 tick 索敌（走 HostilityOracle+射程）命中→Engaging 站定开火（写 CurrentTarget，复用 fire_attacks，**不改 combat**）目标灭→恢复原路；Hold=站定索敌不追击；Patrol=ab 往复（AttackMove 语义）；AttackTarget=追至射程开火 | MOV1/MOV2 | P3 |
| MOV5 | 群体阵型散点：组命令同点 → 目标点按 body_radius 螺旋分槽（组内 UnitId 排序分派，确定性） | MOV1 | P3 |

**关键决策**：A* 而非流场（一期）——目标点多样（命令驱动），流场是 TD 单目标优化后手（§12）；寻路预算+分帧是 WC3 同款做法；「移动中交战」由 movement 层控制速度归零 + 复用现有开火机制，combat 零改动。

**验收断言**：10×10 L 形墙 (0,0)→(9,9) 路径步数==手算定值（写死）；对角墙角场景路径不含对角跳；同帧 100 请求→当帧 astar 调用 ≤8（计数资源断言）、8 帧内全部完成；路径中途放塔 → ≤60 tick 内受影响单位全部持有新路径或到达；10 单位同点 300 tick 后两两距离 ≥ (ri+rj)×0.9 且质心漂移 ≤1.0；AttackMove 途经索敌区 → CurrentTarget 非空、敌灭后回到原 waypoint 序列；5 单位同点命令 → 5 目标点两两距离 ≥ 半径和×0.9。

## 6. SPL 域：法力 / 施法时间 / 引导（skill 域补全）

| 增量 | 内容 | 依赖 | 批次 |
|------|------|------|------|
| SPL1 | `Mana/MaxMana` 组件 + `ManaRegen`（Tick 段恢复）；`UnitDef.mana: Option<ManaDef{max,regen}>`（None=无蓝条→warden 全塔 None=零漂移）；`SkillDef.mana_cost: f32=0`；process_casts 校验序：caster→冷却→蓝→执行扣蓝；拒绝原因枚举 + `OutOfMana` | SEAM | P1 |
| SPL2 | `SkillDef.cast_time: f32=0` / `channel: Option<f32>`；`Casting{skill, elapsed, total, target, point}` 组件（Act 段推进，完成发既有 SkillCast——效果系统零改动）；施法开始即扣蓝、打断不退蓝、冷却从施法完成/打断时刻起（三项写死，防 agent 自选）；`InterruptCast` 消息（晕眩/沉默未来效果用）打断 Casting 与 channel；一期伤害不打断施法 | SEAM | P1 |

**验收断言**：蓝 100 耗 30 → 施后 70；蓝 20 → Reject(OutOfMana) 且冷却未启动；cast_time 1.0s：0.5s 时零 SkillCast 且蓝已扣、1.0s 恰一条 SkillCast；0.5s InterruptCast → 零 SkillCast、蓝不退；channel 中断同理。warden 161 断言零漂移（新字段默认值回归）。

## 7. MP 域：玩家 / 队伍 / 外交（数据层）

| 增量 | 内容 | 依赖 | 批次 |
|------|------|------|------|
| MP1 | `PlayerId(u8)/TeamId(u8)`；`MatchDiplomacy{players: Vec<(PlayerId,TeamId)>, hostile_matrix}` 资源（默认单玩家=现状：P0 vs ENV）；`Owner(PlayerId)` 组件；`sync_allegiance`（Cleanup 段）从 Owner+外交派生刷新 Allegiance（warden 读点零改动）；HostilityOracle 换查矩阵实现；光环作用域同走 oracle。harness 可选加 damage_by_player 读数 | SEAM0c | P2 |
| MP2 | 多玩家胜负语义（团队覆灭/目标占点 WinCondition 扩展）+ 同队共享视野缝（FOG2 消费）+ 经济按 Owner 分域评估卡（**只评估不实现**，ADR-0017 记录决策） | MP1/FOG1 | P3 |

**验收断言**：3 队敌对矩阵 9 格 is_hostile 组合写死断言；默认单玩家配置下 warden 161 + loop-td 全量零漂移（本域核心门）；sync_allegiance 派生值==手写 Allegiance 逐实体一致。

## 8. PRD / INV 域：生产科研 / 物品商店

### PRD（生产/训练/科研/集结点）

| 增量 | 内容 | 依赖 | 批次 |
|------|------|------|------|
| PRD1 | `ProductionDef{trains: Vec<UnitId>, queue_cap:5}` 挂建筑 UnitDef；`ProductionQueue`（VecDeque，Tick 段推进）；`TrainCommand/CancelTrain` 消息；入队即扣费（WC3 语义）、取消退 100%、人口满→Paused 可读态；完成 `spawn_unit`。队列面板数据层（命令卡训练按钮复用 BX4a 框架，升级阶排除同款逻辑） | SEAM | P1 |
| PRD2 | 科研：`ResearchDef{id,cost,time,grants}`；grants 三类——StatMods 全局类型修改（`TypeModifiers` 资源，EffectiveStats 聚合加一层）/ 解锁建造项（命令卡槽 gating）/ 解锁技能；`ResearchState`；玩家单列科研队列（简化，写死） | PRD1 | P3 |
| PRD3 | 集结点 `RallyPoint(Option<OrderPoint>)`：训练完成即自动下发默认命令（有=Move/AttackMove to point，无=Hold） | PRD1/MOV1 | P3 |

**验收断言**：入队扣费即时；5s 训练至 2s 取消 → 金回 100%、无 spawn、队列空；人口满 → 队列 Paused 且面板态可读；集结点：spawn 后首帧 AgentOrder==Move(to rally)；科研 grants 生效断言（攻+2 → EffectiveStats 全体该类型 +2，source 独立）。

### INV（物品/背包/掉落/商店）

| 增量 | 内容 | 依赖 | 批次 |
|------|------|------|------|
| INV1 | `ItemDef{id,kind(Consumable/Charged/Passive),charges,cooldown,effects}` + ItemRegistry（RON 入 MapBundle）；`Inventory` 固定 6 槽挂英雄（槽=ItemStack 组件）；拾取：Pickup order（MOV）走近 body_radius+0.2 → 入首空槽，满=可读拒绝 | SEAM | P3 |
| INV2 | 物品效果：使用=经既有 SkillCast 通道（物品=无等级技能，效果系统零新增）；被动携带=StatMods 注入（ModEntry.source=item 实体，丢弃即撤）；合成 `Recipes`（命令卡主动合成按钮，不做自动） | INV1 | P3 |
| INV3 | 商店 `Shop`（选中→命令卡买卖清单）+ 掉落 `Drops[(ItemId,chance)]` 挂 UnitDef（MatchRng 种子化；loop-td BOSS 掉落特化上收为通用） | INV1 | P3 |

**验收断言**：6 槽满拾取 → Reject(InventoryFull)；药水 hp+150；被动剑 +12 伤 → EffectiveStats source 独立可叠；chance 0.5 同 seed 同结果（20 次 roll 断言）；买卖金流水对称。

**物品槽热键**：选中英雄时命令卡第二行上下文复用（沿 warden SK10 塔技能复用键先例），不新开全局键域——warden §16.9 热键分域法则的延伸。

## 9. FOG / AUD 域：视野迷雾 / 音效通道

| 增量 | 内容 | 依赖 | 批次 |
|------|------|------|------|
| FOG1 | `VisionSource{radius}`；`VisionGrid` 双层位图（visible/explored，explored 只置位不清）；每 tick 增量重算（源集合指纹变化才全量、源移动走脏格）；一期圆形视野无遮挡 | SEAM | P1 |
| FOG2 | 迷雾消费（规则=D5）：手动技能落点/目标须 visible（CastSkill 校验 + Reject(NotVisible)）；不可见敌不可选中；渲染层 `InFog` 标记（灰盒变暗，fx feature）。共享视野=同 TeamId 源合集 | FOG1/MP1 | P3 |
| FOG3 | 小地图数据层 `MinimapModel`（静态缩略+单位点+事件 ping），灰盒渲染（原生色点）；点击→镜头跳转/命令落点随 RL 系落 | FOG1 | P3 |
| AUD1 | `SfxRequest{kind}` 枚举（ClickAck/Select/OrderAck/Error/Cast/Hit/Death/WaveStart/BuildComplete/ResearchComplete 等起步 12 种）；基底各关键节点发消息（命令 Ack 或 Error 每玩家命令恰一条、同类节流 0.1s 写死）；游戏侧监听器挂资产（本轮不挂） | SEAM | P1 |

**验收断言**：单源 r=5 → (3,4) visible、(7,7) 不可见；源移走 → (3,4) explored&&!visible；手动技能对不可见点 Reject(NotVisible)；**塔打不可见怪照常**（D5 规则断言）；开波恰 1 条 WaveStart；非法命令恰 1 条 Error。

## 10. RPL 域：录像（确定性已备，本域是收割）

| 增量 | 内容 | 依赖 | 批次 |
|------|------|------|------|
| RPL1 | `MatchInputLog{version, map_hash, seed, frames: Vec<(u32, Vec<CommandEnvelope>)>}`；`CommandEnvelope`（SEAM0b 定义，各域命令消息实现 Into）拦截录制（Update 最前）；`StateHash` 世界指纹（实体数+关键资源和+Hp/Mana 总和等，每 30 帧入日志）；`replay_app(map, seed, log)` headless 逐帧 apply+对账，首差帧报错 | SEAM0b | P1 |
| RPL2 | 真机入口：`--replay file` 观战模式（逻辑+渲染照跑）；录制自动存 `runs/*.jsonl` | RPL1 | P3 |

**验收断言**：bot 打 500 帧×3 种子录制 → 回放 hash 序列全等；改 seed → hash 必不等（灵敏性）；真机录 → headless 放全等（同机同 build）。

## 11. 并行开发协议（子智能体分工）

### 批次图（依赖为硬约束，批内可乱序）

```
Phase 0（串行，主线 1 会话）: SEAM0a–0e（含 ADR-0016/0017 拍板——人工关卡）
Phase 1（6 agent 并行）: SEL1a/1b/2/3 · SPL1/2 · PRD1 · FOG1 · AUD1 · RPL1
Phase 2（3 agent 并行）: MP1 · MOV1/2 · SEL4（依赖 Order 枚举已在 SEAM0b 钉死）
Phase 3（5 agent 并行）: MOV3/4/5 · SEL5 · PRD2/3 · INV1/2/3 · FOG2/3 · MP2 · RPL2
Phase 4（串行主线 + 人工关卡）: RL0–RL3 RTS 验收图 + H1 试玩
```

**RL 系（验收游戏，`games/rts-lab/` 暂名）**：最小 WC3 场景——主城训练农民、农民建塔（复用 build 域）、3 兵种交战、敌方 AI（Greedy harness 驱动）、1 英雄 3 技能带蓝、物品掉落、迷雾。**目的=ADR-0012 护栏检验：新地图不改基底核心。若必须改，停下复盘补 ADR（通道 C 信号）。** RL0 立项（模板复制）/ RL1 主城+农民+采集移动 / RL2 交战+技能+生产 / RL3 物品+迷雾+小地图全链。

### 共享文件规则（写冲突硬规避）

- `lib.rs` / `bundle/mod.rs`：**只有 SEAM 动**。域增量只写自己模块文件（prelude 已通配，见 D3/D7）。
- `combat/mod.rs`：SEAM0c 之后仅 MP1 可动（HostilityOracle 实现体+光环过滤），其余域禁改（含 MOV4——交战走自己模块写 CurrentTarget）。
- `skill/mod.rs`：仅 SPL 动；`testing.rs`：helper 只增不改（新增 helper 加在自己域文件）。
- 每增量一提交（英文 Conventional Commits，如 `feat(warbase): SEL2 selection set + control groups`），提交前 `cargo test --workspace` 绿是门禁。

### 任务包模板（每个子智能体的 prompt 骨架）

```text
你是 warbase 增量 <ID> 的实现者。
必读：docs/topics/engine/warbase-wc3-alignment-plan.md §<本域>与 §2（D1–D10）；
     engine/warbase/src/<相关模块>；docs/topics/engine/warbase-implementation-design.md；AGENTS.md。
范围铁律：只改 <本增量文件清单>；禁改 lib.rs / bundle/mod.rs / combat/mod.rs（豁免：<无>）。
规格即本方案 §<域>——验收断言逐条落测试，先红后绿。
门禁：cargo test --workspace 全绿（含 warden 161 / loop-td 零漂移）+ cargo build -p warbase --no-default-features 零警告。
失败纪律：同一修法 2 次不过 → 停，写复盘（方案错不是参数错），不带病叠补丁。
提交：一增量一提交，英文 Conventional Commits。
```

### 里程碑与工作量

总计约 **27 增量**：Phase 0 一会话；Phase 1/2/3 每增量 0.5–1 会话、四批并行约 2–3 周日历（含人工关卡）；Phase 4 验收图 3–4 增量 + H1。两个**人工关卡**：Phase 0 的 ADR 拍板、Phase 4 的 H1 试玩（含既有 loop-td H1 一并过）。

## 12. 开放问题（需拍板，均已给推荐）

1. **编队数字键 vs 模态三选一 1/2/3**：推荐模态期数字归模态、非模态归编队（§4 SEL4 已按此写）。
2. **物品槽热键**：推荐选中英雄时命令卡下排上下文复用（§8，沿 SK10 先例）；备选独立 Num 键（现代键盘不可靠，不推荐）。
3. **A\* vs 流场**：推荐 A\*+预算分帧起步（命令目标多样）；TD 图敌人单目标场景可用流场做后手优化（不改接口）。
4. **施法打断规则**：推荐一期伤害不打断、InterruptCast 消息可打断、打断不退蓝（§6 SPL2 已写死）。
5. **rts-lab 立项时机与命名**：推荐 Phase 3 末立项（基底域齐了再验收）；名字候选 rts-lab / warcraft-lab / arena。
6. **科研队列归属**：推荐玩家单列（WC3 每建筑队列的简化）；做 RPG 图若痛再扩。
7. **视野遮挡（高低差/树挡视线）**：一期圆形视野；真需求出现时开 FOG 增量，接口 VisionGrid 不变。
