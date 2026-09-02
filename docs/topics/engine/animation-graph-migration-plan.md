---
type: reference
status: draft
topics: [engine, animation]
date: 2026-09-02
related:
  - "[[decisions/0005-animation-graph-migration|ADR-0005]]"
  - "[[decisions/0006-animation-state-machine-refactor|ADR-0006]]"
---

# 动画状态机迁移设计蓝图（bevy_animation_graph 0.11.0）——技术否决支撑

> 2026-09-02，AI 基于对现有 `presentation.rs::sync_walk_playback`（1118 行文件内
> ~240 行手写状态机）+ bevy_animation_graph 0.11.0 源码逐条核验后整理。
> 本篇是 ADR-0006「动画状态机不走 bevy_animation_graph」的**支撑材料**：记录迁移的
> 可行性核验、行为模型、以及**为何否决**（转场不支持输入条件比较）。
> ADR-0006 是决策层，本篇是推导层。

## 0. 可行性核验（结论先行）

- bevy_animation_graph 0.11.0 依赖 bevy 0.19.1（`cargo tree` 实证，无第二套 bevy）。
- ADR-0005 声称的 API 全部真实：
  - `GraphClip::from_bevy_clip(bevy_clip, skeleton, event_tracks, source)` ✓
  - `AnimationGraph::new()/add_node()/add_edge()/add_*_edge()` ✓
  - `AnimationGraphPlayer::new(skeleton)/with_graph()/start()/send_event()/set_input_data()` ✓
  - `StateMachine::add_state()/add_transition_unchecked()` ✓
  - `TransitionKind::Graph { graph, timed: Some(len) }` ✓
  - `compare_f32` / `clamp_f32` / `LoopNode` / `SpeedNode` / `BlendNode` / `ClipNode` ✓
- **但它不用 bevy 内置 `AnimationPlayer`/`AnimationTransitions`/`AnimLink`**，而是自己一套
  `AnimationGraphPlayer` + `Skeleton` 资产 + `AnimationGraph`。所以是整套重构。

## 1. spike 结果（2026-09-02 实测）

**装配层已验证**：`tests/animation_spike.rs` 的 `programmatic_assembly_works` ok（后随依赖移除清理）。
Skeleton 程序化构建、bevy clip→GraphClip::from_bevy_clip、AnimationGraph::add_node/output_pose_edge、
AnimationGraphPlayer::with_graph/send_event/set_input_data 全部可用，且骨架 handle 正确关联。
编译：bevy_animation_graph 0.11.0 + bevy 0.19.1 全量编译零错误（14m31s），依赖树无第二套 bevy。

**BoneId 对齐已确认**：`EntityPath::id()` 内部就是 `AnimationTargetId::from_names(...).into()`，
而 gltf loader 用同一 blake3 哈希路径为骨骼生成 `AnimationTargetId`，故 Skeleton 的 BoneId 与
模型骨骼天然一致 → `apply_animation_to_targets` 能写回骨骼。这是迁移核心红线，已实证打通。

## 2. 现有状态机行为模型（迁移需保持等价）

### 输入（只读，来自逻辑组件 / 实测速度）
| 输入 | 来源 | 语义 |
|------|------|------|
| `GameState` | `Res<State>` | Playing/Paused/GameOver |
| `walk.playing` | `WalkCycle`（逻辑侧 update_walk_cycle 写） | 本帧是否移动 |
| `ground_speed` | `FeelState.speed`（实测位移/dt） | 移动速率（sprint 可见） |
| `attack.cooldown` | `Attack`（玩家） | 上跳边沿 = 挥砍帧 |
| `visual.flash` | `Visual`（任一 owner） | 上跳边沿 = 受击帧 |
| `player_at` | 玩家 Transform | 怪物 bite 距离电平触发用 |

### 状态（HeroClip：Walk/Run/Idle/Attack/Hit）
| 状态 | 进入条件 | 循环 | 播放速率 |
|------|---------|------|---------|
| Walk | moving && speed < 3.0 | Forever | speed/WALK_AUTH (clamp 0.5-4) |
| Run  | moving && speed >= 3.0 | Forever | speed/RUN_AUTH (clamp 0.5-4) |
| Idle | !moving | Forever | 1.0 native |
| Attack | cooldown 上跳边沿（one-shot）| Never | 0.6s 窗口后回状态 clip |
| Hit   | flash 上跳边沿（one-shot）| Never | 0.4s 窗口后回状态 clip |

### 边沿/门控逻辑（拟做成"图转场条件"，但见 §4 不可行）
1. **三态门控**：速度阈值 3.0 分 walk/run；`moving` 分 idle。
2. **战斗边沿**：cooldown 上跳（玩家挥砍）→ attack；flash 上跳（任一受击）→ hit。
   怪物：bite 距离电平（d <= CONTACT_DIST+0.15）→ attack（0.38s），flash → hit（0.29s）。
   one-shot 只触发一个（attack 优先于 hit）。
3. **one-shot 回退**：窗口结束 → 回到状态 clip。
4. **播放速率刷新**：walk/run 每帧按实测速度刷新 anti-slide（live F1 调参生效）。
5. **暂停语义**：hero 冻结所有 clip（pause_all），怪物 walk 停 frame 0（seek_to(0)）。

### 日志通道（逐字兼容红线）
```
[presentation] hero clip -> {Walk|Run|Idle|Attack|Hit} (speed X.XX, rate Y.YY)
```
- 只在 hero 状态 clip 切换时打（one-shot 不发）；rate = walk/run 的 anti-slide 速率；idle = 1.0。

## 3. 图构造方案（拟议，后因 §4 否决）

### 层级
```
AnimationGraph (per model)
├── 输入: POSE(默认), TIME, 事件队列("user_events")
├── 参数: speed(f32), moving(bool), attack_edge(bool), hit_edge(bool)
└── 输出: pose → 写回模型子树
    └── StateMachine (high-level FSM)
        ├── Walk <-> Run (speed 3.0 门控)
        ├── Walk/Run <-> Idle (moving 门控)
        ├── (any) --attack_edge--> Attack --(timed 0.6s)--> 回
        └── (any) --hit_edge--> Hit --(timed 0.4s)--> 回
```
每个 State 自带一个 `Handle<AnimationGraph>`（Walk/Run/Idle = ClipNode+LoopNode；Attack/Hit = ClipNode）。
图外驱动系统每帧写 player 的 input_data / send_event。

## 4. 关键局限（源码级实证，否决的根因）

② **bevy_animation_graph 的 StateMachine（high_level）转场是「事件驱动」，不支持输入条件比较。**
   - `low_level::handle_event_queue`：只处理 `AnimationEvent::TransitionToState/…Label/Transition/EndTransition`，
     都是**立即切换**，不读 `speed`/`moving` 输入。
   - `timed` 只在 `state_transition`（状态回退）里有用（`PercentThroughDuration>=1` 触发 EndTransition）。
   - `CompareF32` 是**图内数据流比较**（输出 bool，喂 BlendNode 等），**不驱动状态切换**。
   - 结论：**三态门控（速度阈值 3.0）、战斗边沿（cooldown 上跳/flash 上跳）、播放速率刷新这些
     核心逻辑，迁移后仍然必须写在图外的 Rust 驱动系统里，一行不会少。** 图只能表达
     "播放哪个 clip"，这正是现在 `sync_walk_playback` 里最清晰的部分。

③ **卡 32 的承诺（"状态机数据化、扩状态不改巨型函数"）无法真正兑现。** 扩一个状态
   （如死亡）仍要：图外驱动系统加一条"进入死亡"的边沿检测 + send_event + 图里加一个状态节点。
   状态机的"控制流"（何时进什么状态）依然在 Rust 代码里，只是换了个写法。

## 5. 结论

- 迁移**技术上可行**（API 全实证、BoneId 对齐打通），但**工程量大、且 59 个逻辑测试无法验证它**，
  正确性只能靠：动画专项 headless 断言 + 编译零警告 + 真机视觉验收。
- 但**收益不抵复杂度**：核心逻辑（门控/边沿/速率）无法下沉到图，控制流留在 Rust。
  ADR-0006 据此**暂缓卡 32 / ADR-0005 的迁移**，改为重构现有状态机（配置表 + decide 纯函数可单测 + 薄执行层）。
