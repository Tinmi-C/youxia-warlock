---
title: 表现层动画状态机——放弃 bevy_animation_graph 迁移（暂缓，形态见 ADR-0007）
status: proposed
date: 2026-09-02
deciders: youxia（方向讨论中）+ AI 起草，待团队 review
supersedes: []
related:
  - ADR-0005（原 proposal，因本证据暂缓）
  - ADR-0007（采用表驱动形态，本 ADR 的"采用什么"）
  - cards/33（TableDrivenAnimState 实施卡）
---

# ADR-0006：动画状态机不走 bevy_animation_graph（形态见 ADR-0007）

> 本 ADR 是对 ADR-0005（动画状态机迁移 bevy_animation_graph）的**技术否决记录**。
> ADR-0005 的"数据驱动"承诺经源码级实证**无法兑现**，故暂缓迁移。
> **"采用什么形态"由 ADR-0007 决定（表驱动状态机）**。本文同时给出证据、结论。

## 背景

- 卡 32 / ADR-0005 主张：把 `games/wave-survival/src/plugins/presentation.rs`
  里手写的 `sync_walk_playback`（~240 行 hand-written 状态机，`HeroClip` 枚举 +
  if/else + one-shot 窗口计时 + 战斗边沿检测 + 播放速率刷新，文件已 1118 行、
  混杂 7 种职责）迁移到 **bevy_animation_graph 0.11.0** 数据驱动动画图，
  声称"状态=图节点、转场=条件边、扩状态=加节点不改巨型函数"。
- 项目已加依赖（`Cargo.toml`：`bevy_animation_graph = "0.11.0"`），spike 编译通过。
- 团队成员（你）推动该决策的真实诉求：**减少代码开发复杂度、数据方便调试、
  表现层逻辑直观、提升 AI 开发效率**。

## 实证过程（2026-09-02，源码级核验）

### 1. 可行性名义上成立（但只是"能编译/能装配"）
- `bevy_animation_graph 0.11.0` 依赖 `bevy 0.19.1`（`cargo tree` 实证，无第二套 bevy）。
- ADR-0005 声称的 API 全部真实存在：
  - `GraphClip::from_bevy_clip(bevy_clip, skeleton, …)` ✓
  - `AnimationGraph::new()/add_node()/add_edge()/add_output_pose_edge()` ✓
  - `AnimationGraphPlayer::new(skeleton)/with_graph()/start()/send_event()/set_input_data()` ✓
  - `StateMachine::add_state()/add_transition_unchecked()` ✓
  - `TransitionKind::Graph{graph, timed}` ✓
- `tests/animation_spike.rs` 的 `programmatic_assembly_works` 跑通：Skeleton 程序化构建、
  bevy clip→GraphClip、单节点图、AnimationGraphPlayer 装配，均可用且骨骼 handle 关联正确。
- **关键红线已打通**：`EntityPath::id()` 内部就是 `AnimationTargetId::from_names(...).into()`，
  与 gltf loader 用同一 blake3 哈希路径为骨骼生成 `AnimationTargetId`，故 Skeleton 的 BoneId
  与模型骨骼天然一致 → `apply_animation_to_targets` 能写回骨骼。

### 2. 但"数据驱动逻辑"无法兑现（决定性证据）
bevy_animation_graph 的 `StateMachine`（high_level + low_level，见
`bevy_animation_graph_core/src/state_machine/low_level/mod.rs`）：
- **转场只由事件触发**：`handle_event_queue` 只处理
  `AnimationEvent::TransitionToState / …Label / Transition / EndTransition`，
  全部是**立即切换**，不读任何输入数据。
- `timed`（one-shot 窗口）只在 `state_transition`（状态自回退）里生效：
  靠 `PercentThroughDuration>=1.` 触发 `EndTransition`。
- **没有让转场基于 `speed`/`moving` 输入做比较的机制**。
  `CompareF32` 是**图内数据流比较**（输出 bool 喂 BlendNode 等），**不驱动状态切换**。

**结论**：现有 `sync_walk_playback` 里的核心逻辑——
- 三态门控（速度阈值 3.0 分 walk/run、moving 分 idle）
- 战斗边沿检测（cooldown 上跳→attack、flash 上跳→hit、怪物距离电平）
- 播放速率刷新（anti-slide）

**迁移后一行都不会少，仍必须手写在图外的 Rust 驱动系统里**。图能表达的只有
"播放哪个 clip"，而这恰是现在手写状态机里最清晰、最不痛的部分。

### 3. 因此 ADR-0005 的核心承诺落空
> "扩状态 = 加节点不改巨型函数"

实际是：扩一个状态（如死亡）仍要——图外驱动系统加一条"进入死亡"的边沿检测 +
`send_event` + 图里加一个状态节点。**状态机的控制流（何时进什么状态）依然在 Rust
代码里**，只是换了个写法（且额外增加图外驱动 + Skeleton 构建两层复杂度）。

## 决策

**暂缓 ADR-0005 / 卡 32 的 bevy_animation_graph 迁移**（已实证判断逻辑无法下沉到图）。
**采用什么形态由 ADR-0007 决定：表驱动动画状态机**（状态拓扑进表数据，加状态=加表数据，
控制流一次写好）。本 ADR 只保留"为何否决 bevy_animation_graph"这一条；不再重复已被
ADR-0007 取代的替换方案细节。

## 影响 / 后果

- **好**：否决了一条"换引擎但解决不了判断逻辑"的路线，避免白白增复杂度；随后用表驱动
  （ADR-0007）真正解决了"改状态要改巨型函数"。
- **代价**：无 bevy_animation_graph 的图数据 / 可视化编辑器能力（若后期确需可视化编辑器
  再看，当前收益不抵复杂度）。
- **依赖（已处理）**：`Cargo.toml` 里的 `bevy_animation_graph` 依赖已移除（spike 使命完成）。

## 后续跟进

- ADR-0005 / 卡 32 保留为**被否决的技术记录**（`superseded`），不实施；卡 33（表驱动）已
  按 ADR-0007 实现并提交。

## 与既有 ADR 的关系
- ADR-0005：本 ADR 是其"被技术证据否定"的跟进方——**否定了 bevy_animation_graph 迁移**这条路线。
- ADR-0007：本 ADR 只做"否决"，**具体采用什么形态由 ADR-0007 决定（表驱动状态机）**。
- ADR-0004：坚守"不手写引擎件 / 用现成"边界；表驱动用 Bevy 内置 `AnimationTransitions` 播放，
  不自建引擎，不违背。
