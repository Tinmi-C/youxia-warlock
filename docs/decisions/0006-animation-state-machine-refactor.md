---
title: 表现层动画状态机——放弃 bevy_animation_graph 迁移，改为重构现有状态机
status: proposed
date: 2026-09-02
deciders: youxia（方向讨论中）+ AI 起草，待团队 review
supersedes: []
related:
  - ADR-0005（原 proposal，因本证据暂缓）
  - cards/32（原 AnimationGraphMigrate 草案，因本证据暂缓）
---

# ADR-0006：动画状态机不走 bevy_animation_graph——重构现有状态机（纯函数 + 配置表）

> 本 ADR 是对 ADR-0005（动画状态机迁移 bevy_animation_graph）的**技术否决记录**。
> ADR-0005 的"数据驱动"承诺经源码级实证**无法兑现**，故暂缓迁移；改为对现有
> `sync_walk_playback` 做**不换引擎**的重构。本文同时给出证据、结论与建议替代方案。

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

**暂缓 ADR-0005 / 卡 32 的 bevy_animation_graph 迁移。** 改为不换引擎地重构现有状态机。

### 建议替代方案（方案 1，针对你背后的真实诉求）
把 `sync_walk_playback` 拆成三层：

1. **配置表** `HeroAnimConfig`：收敛所有散落 const——
   `HERO_CLIP_*` 索引、`HERO_ATTACK_WINDOW`/`HERO_HIT_WINDOW`、`HERO_BLEND`、
   `RUN_SPEED_THRESHOLD 3.0`、`WALK/RUN_CLIP_AUTHORED_SPEED`、`WALK_RATE clamp`、
   怪物侧 `MONSTER_ATTACK/HIT_SECS`、`MONSTER_ATTACK_RANGE`。
2. **纯函数** `decide_hero_state(moving, speed, cooldown_edge, flash_edge) -> HeroClip`
   （及怪物侧）：**唯一有逻辑的地方**，无 ECS 依赖、无副作用 → 可直接单测。
3. **薄执行层** `sync_walk_playback`：读逻辑输入 → 调 decide 纯函数 → 查配置表
   （clip/循环/速率/窗口）→ `AnimationTransitions` 播放 + 打 `hero clip -> X (speed, rate)` 日志。

### 对照诉求
| 诉求 | 方案 1 如何满足 |
|---|---|
| 降复杂度 | 240 行巨怪 → 一个可读纯函数（~30 行）+ 一张表 |
| 方便调试 | **纯函数可直接单测**：`feed(静止)→Idle`、`feed(速度5)→Run`、`feed(cooldown上跳)→Attack`，不跑游戏 |
| 逻辑直观 | 状态切换 = 表 + 函数，一眼看懂"什么输入→什么状态" |
| AI 开发高效 | 改纯函数有单测保护、零风险；执行薄层几乎不动；验收句可写成纯函数断言 |
| 成本 | 一次性重构；**不换引擎**；59 回归不受影响；真机验收照旧 |

### 备选（均不推荐，附理由）
| 方案 | 结论 | 理由 |
|---|---|---|
| A. 自建轻量"状态表+解释器" | ❌ | 需自命名状态机引擎，违背 ADR-0004 "不手写引擎件"；且同样解决不了判断逻辑；成本高、59 测试测不到 |
| B. bevy_animation_graph 全量迁移 | ❌ | 本 ADR 已实证：判断逻辑下不去、复杂度反增、59 测试测不到、违背"减负"初衷 |
| C. 方案 1 + 视觉化（bevy_egui / Mermaid 画状态机图） | 可后置 | 加分项：让"表现层逻辑直观"更进一步；用现有 F1 面板成本低；不是必须先做 |

## 影响 / 后果

- **好**：不换引擎、不增复杂度；把"改状态要改巨型函数"这个真痛点用纯函数 + 单测解决；
  表现层逻辑集中、可验证；AI 开发效率提升（改纯函数有保护）。
- **代价**：无 bevy_animation_graph 的图数据 / 可视化编辑器能力（若后期确需可视化编辑器
  再看，当前收益不抵复杂度）。
- **依赖**：`Cargo.toml` 里加的 `bevy_animation_graph` 依赖——**若确认暂缓迁移，应从
  Cargo.toml 移除**，保持依赖面干净（spike 已完成使命）。

## 后续跟进
1. **暂缓**：卡 32 草案、ADR-0005 保留为技术记录（或标为 `superseded`），不实施。
2. **可选**：把 `tests/animation_spike.rs` 保留为"bevy_animation_graph 装配可行性"的
   参考，或随依赖移除一起清理。
3. **建议下一卡**：HeroAnimRefactor——按方案 1 拆 `sync_walk_playback`，验收句 = 纯函数
   断言（各输入→期望状态）+ 真机 hero 四态/怪物三态与迁移前肉眼一致 + 59 回归全绿 + 零警告。
4. **待用户 / 团队拍板**：本 ADR 的状态（proposed）、是否移除 bevy_animation_graph 依赖、
   是否开工 HeroAnimRefactor。

## 与既有 ADR 的关系
- ADR-0005：本 ADR 是其"被技术证据否定"的跟进方，方向从"迁移引擎"变为"重构现有"。
- ADR-0004：坚守"不手写引擎件 / 用现成"边界——方案 1 重构现有 bevy 内置 AnimationTransitions，
  仍是"用现成"，不违背。
