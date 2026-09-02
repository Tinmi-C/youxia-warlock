---
type: reference
status: draft
topics: [engine, animation]
date: 2026-09-02
related:
  - "[[decisions/0007-animation-state-machine-table-driven|ADR-0007]]"
  - "[[decisions/0006-animation-state-machine-refactor|ADR-0006]]"
---

# 表驱动动画状态机设计方案

> 2026-09-02，AI 起草。针对确认「动画会很多（20+ 状态）」的规模预期。
> 现状：`presentation.rs::sync_walk_playback` 一个 ~240 行手写状态机，
> `HeroClip` 枚举 + if/else 分支，7 状态（hero 5 + 怪物 2 共享）。
> 动画增多时，这个函数会越来越失控（每加一状态都要碰它）。
> 本方案：把「状态机拓扑」从代码变成**表数据**，加状态 = 加一条表数据，
> 控制流一次写好、永不改。用 bevy 内置 `AnimationTransitions`/`AnimationPlayer`
> 播放，**不自建引擎**（符合「客观用现成、主观自研」的项目边界）。
> 决策层见 [[decisions/0007-animation-state-machine-table-driven|ADR-0007]]；
> 实施卡见能力卡卡 33（TableDrivenAnimState）。

## 1. 核心思想

现在的问题不在引擎（bevy 能播任意 clip、能转场），而在**「哪些状态、何时切换」被硬编码在 match/if-else 里**。

表驱动 = 把这张拓扑搬进一张配置表，一个**通用调度系统**读表驱动播放。状态机的**引擎逻辑一次写好**，之后**加状态只改表**。

## 2. 状态表结构（`AnimState`）

```rust
/// 一个动画状态的全部定义。一张 Vec<AnimState> 就是完整状态机。
pub struct AnimState {
    /// 状态名（唯一，用于日志/调试/互相引用）
    pub name: AnimStateId,          // enum：Idle/Walk/Run/Attack/Hit/Death/...
    /// 这个状态下播哪个 clip（bevy 动画索引）
    pub clip: ClipRef,
    /// 循环方式：Forever(循环) 或 OneShot(Never，播完退出)
    pub repeat: RepeatMode,
    /// 播放速率：Native(1.0) / AntiSlide{ authored_speed }（防滑速率）
    pub rate: RateMode,
    /// 进入此状态的转场条件（顺序求值，命中的第一个生效）
    pub transitions: Vec<Transition>,
    /// one-shot 播完如何退出：回到某状态 or 保持（仅 OneShot 状态有意义）
    pub on_finish: Option<AnimStateId>,   // Attack(0.6s) -> 回 Idle 之类
    /// 转场 blend 时长
    pub blend: Duration,
}
```

### 转场条件（`Transition`）—— 把现在的判断逻辑数据化
```rust
pub enum Transition {
    /// 速度阈值门控：speed >= threshold 进此状态（用于 Walk/Run 分流）
    SpeedAtLeast { threshold: f32 },
    /// 速度低于阈值（Run -> Walk）
    SpeedBelow { threshold: f32 },
    /// 是否在移动（Idle <-> Walk/Run 的 moving 门控）
    Moving { want: bool },
    /// 战斗边沿：cooldown 上跳（玩家挥砍）-> Attack
    CooldownEdge,
    /// 战斗边沿：flash 上跳（受击）-> Hit
    FlashEdge,
    /// 被调用时立即进入（常用于初始状态）
    Immediate,
}
```

## 3. 通用驱动系统（一次写好，取代 sync_walk_playback）

```rust
fn drive_anim_states(
    // 读每个 owner 的逻辑输入（moving/速度/cooldown_edge/flash_edge）
    // 读 AnimStateTable（Resource）
    // 对每个 owner：从当前状态查 transitions，命中则切换；
    // 用 bevy AnimationTransitions.play + set_repeat + set_speed 播 clip
)
```

它做的事：**读输入 → 查当前状态的 transitions → 命中则切状态 → 用 bevy 播对应 clip**。这套控制流与状态数量无关——**加 10 个状态，驱动系统还是这几行**。

## 4. 现有 7 状态如何映射到表（示例，非最终）

| 状态 | repeat | rate | 转出条件 | on_finish |
|---|---|---|---|---|
| Idle | Forever | Native | Moving{true}→Walk | |
| Walk | Forever | AntiSlide(1.6) | SpeedAtLeast(3.0)→Run；Moving{false}→Idle | |
| Run | Forever | AntiSlide(2.8) | SpeedBelow(3.0)→Walk；Moving{false}→Idle | |
| Attack | OneShot | Native | | →回 Idle |
| Hit | OneShot | Native | | →回 Idle |
| (怪物)Walk | Forever | AntiSlide | | |
| (怪物)Attack/Hit | OneShot | Native | | |

## 5. 优点 / 缺点（诚实的账）

### ✅ 优点
- **治增长**：加状态 = 加一条 `AnimState` 表数据，`sync_walk_playback` 控制流**一次写好、永不改**。状态 7→20→50，驱动系统不变。
- **可测**：`derive_next_state(current, inputs) -> next` 是纯函数，可直接单测（feed 输入断言切到哪个状态）。
- **数据化**：状态拓扑直观可见；配合 F1 egui 面板可做可视化（状态当前值、切换日志）。
- **AI 高效**：AI 加状态只改表 + 加 clip，改不到控制流；命中率高、零风险。
- **合规**：用 bevy 内置 `AnimationTransitions`/`AnimationPlayer` 播，**不自建引擎**，把「状态拓扑」数据化属主观玩法自研。

### ❌ 缺点 / 代价
- **工作量增大**：这**不是小重构**，是架构升级。要定义表结构 + 写通用驱动系统 + 迁移现有 7 状态到表 + 写测试。比方案 A（拆纯函数）大。
- **一次性迁移风险**：现有 7 状态行为必须逐字保留，靠 59 回归 + 真机肉眼验收兜底。
- **需要 ADR**：结构改动先立 ADR（项目规范）——见 [[decisions/0007-animation-state-machine-table-driven|ADR-0007]]。
- **设计取舍**：`Transition` 是"条件枚举"而非任意表达式，所以只覆盖"已知的判断类型"（速度阈值/moving/边沿）。若将来出现**全新的判断逻辑**（如"血量百分比触发"），要先加一个 `Transition` 变体——**但这是加一个枚举变体，不碰驱动系统主逻辑**，比改巨型函数可控得多。
- **不是万能**：只解决"状态机拓扑扩展"，不解决动画资产/模型本身扩展（那是美术管线的事）。

## 6. 对比（已确认动画 20+）

| 方案 | 动画变多 | 一次性工作 | 合规 | 风险 |
|---|---|---|---|---|
| A 拆纯函数 | ❌ 状态多了失控 | 中 | ✅ | 低 |
| B 拆文件 | ❌ 只治乱 | 中 | ✅ | 低 |
| **表驱动** | ✅ 加状态=加数据 | 大 | ✅ | 中 |
| bevy_animation_graph | ❌ 已证伪 | 超大 | ❌ | 高 |

**结论**：要动画 20+，**表驱动**是唯一"越加越好加"且合规的路线。方案 A/B 是治乱不治本，现在不值得做。bevy_animation_graph 已被证伪不选。

## 7. 待拍板

- **是否上表驱动**（而不是方案 A/B）？
- 若上：① 写一张 ADR 说明结构决策（ADR-0007）；② 把卡 33 从"纯函数重构"升级为"表驱动状态机"；③ 开工迁移。
