---
title: 项目结构与开发规则复盘（进行中）
type: reference
topic: engine
date: 2026-08-31
author: youxia + AI
status: draft
tags: [结构复盘, 开发规则, 装配层, SystemSet, Rule-of-Three]
related:
  - topics/engine/bevy-plugin-and-code-reuse
  - topics/engine/capability-card-workflow-deep-dive
  - topics/engine/engine-direction-discussion
  - decisions/0002-engine-scope-game-driven
  - decisions/0004-handwritten-renderer-to-bevy
---

# 项目结构与开发规则复盘（进行中）

> **性质**：工作底稿，不是已生效 ADR。本会话边读 `wave-survival` 代码边讨论规则；结论先记在这里，**统一改规则/ADR/模板时以此为准**，未拍板前不要改 `AGENTS.md`、ADR、模板。
>
> **范围**：当前项目结构是否支撑「标准化 + 可自动化开发」；旧规则哪些要改。

## 结论

`wave-survival` 的 ECS 分层（组件 / 资源 / 系统 / 插件）方向对，但 **玩法调度集中在 `GamePlugin` 一条 `.chain()` 上**，对后续扩展和 AI 自动插卡不友好。市面 Bevy 主流（领域 Plugin + SystemSet）与团队愿景（能力卡装配层）**不是同一套方案，可兼容**。youxia **明确不赞同「第二次用到才抽公共层」**，该条列入待改；其余区别仍需继续讨论。旧文档规则暂不改，等本篇讨论收口后统一修订。

**第二轮进展（08-31）**：主链 SystemSet 拆分方案已出稿（§7，AI 专业判断）：五阶段 `Movement → Combat → Despawn → Spawn → Observe`，现有 11 系统相对顺序不变、零回归风险；玩法**不拆多 Plugin，先只加 Set**；能力卡增加「挂载 / 依赖消息」声明作为装配层自动化前置。方案待 youxia 拍板后，随 §4.1 一起收口执行。

## 讨论日志

| 日期 | 议题 | 状态 |
|------|------|------|
| 2026-08-31 | 读 `build_app` / 组件 / 资源 / `GamePlugin`；对比主流 vs 愿景；Rule of Three | 进行中 |
| 2026-08-31 | 读 `GamePlugin` 主链（11 系统 `.chain()`）；AI 出 SystemSet 阶段方案 + Plugin vs Set 决策 + 卡挂载声明 | SystemSet 方案已出（§7），待 youxia 拍板 |

## 1. 当前代码结构（读代码时的快照）

基于 `games/wave-survival/`（会话中已 pull 至含武器卡 29/30 的 `main`）。

| 位置 | 角色 | 备注 |
|------|------|------|
| `src/lib.rs` `build_app()` | App 组装清单 | 引擎插件顺序有硬约束（Default / Rapier / State / Egui）；领域插件元组是默认顺序 |
| `src/components.rs` | 实体上的数据（schema） | 只定义不 spawn；含 Kind 定义表、UI 空标记、逻辑→表现观察通道 |
| `src/resources.rs` | 全局单例 | `Wave` / `Balance`（调参底板，非玩家成长）；`GameStats` 目前未挂 |
| `src/states.rs` | Playing / Paused / GameOver | |
| `src/plugins/game.rs` | **玩法总接线板** | Startup 生成 + Playing 主链 + 暂停/重开/关物理 |
| `src/systems/*.rs` | 真正的玩法函数体 | 名单里只写函数名 |
| `presentation` / `vfx` / `tuning` / `debug` | 表现与工具 | 已与逻辑拆开；测试可不挂渲染 |

`MonsterKind` = 同一套近战追击上的数值/外观变体。新机制（远程/飞天/自爆）应是新组件+新系统，Kind 最多当生成配方。

`GamePlugin` 主链（Playing + `.chain()`）当前是调度名单，不是实现堆砌。卡 29 武器改的是 `player_attack` 内部，链上未加行。

## 2. 结构痛点（相对「可自动化框架」）

- 新 Playing 系统若有同帧顺序，就要改 `game.rs` 同一段 tuple，插入点靠人猜。
- 能力卡流程（立卡→实现→测试）已有；**代码装配**仍是手改名单，机器难安全改。
- 表现层已按「独立 Plugin + 读观察组件」拆；玩法侧还没拆到同级。

**主流（Bevy 认真项目）常见终态**：领域 Plugin + 少量 SystemSet 阶段（如 Input → Movement → CombatWrite → Despawn → Observation）+ 组件/消息契约。巨型 `.chain()` 多见于教程/原型。

演进不必立刻抽 `engine/` crate；更靠前的是 **把接缝和阶段写标准**。

## 3. 主流写法 vs 团队愿景（需继续消化）

两件事不要混：

| | A. 代码怎么摆 | B. 怎么开发 / 怎么让 AI 帮忙 |
|--|--|--|
| 市面主流 | Plugin、SystemSet、ECS 契约（很熟） | Issue / GDD / 测试；很少有「能力卡」 |
| 团队文档愿景 | Bevy 当底料 | 卡=WHAT、验收句、回归、观察通道 = 自研装配层 |

重叠：别上帝文件；新能力尽量新积木；客观技术用生态插件。

仍容易混的几点（youxia 表示尚未完全分清，后续讨论）：

1. **能力卡 ≠ 代码结构**。卡是给人和 AI 的合同；运行时是组件+系统。
2. **Bevy 是底料 vs Bevy 就是引擎**。写业务时差别小；差别在卡/测试/装配规则算不算比 Bevy 用法更重要的资产。
3. **自动化对象**。CI/格式化 ≠ 「按卡插模块」。后一种需要挂载规矩（属于哪个阶段），不只是公共 crate。

## 4. 已拍板方向（youxia，本会话）

### 4.1 反对「第二次用到才抽公共层」（Rule of Three 的时间点）

旧规则出处：[[topics/engine/bevy-plugin-and-code-reuse]]、ADR-0002「不为将来预付」、能力卡文「第二个项目复用才提炼」。

**youxia：不赞同，确定要改。**

拟改方向（尚未写进 ADR，仅本底稿）：

- 反对的是 **「必须等第二款游戏才抽」**，不是改成 **「先做没用的万能 RPG 引擎」**。
- 中间态：**第一款就把可装配接缝留标准**（阶段、插件边界、消息/组件契约），即使暂时只有一个游戏在用。
- 仍建议保留：无游戏消费的空想模块不要做（游戏驱动可以留，改的是抽公共层的**时间点**）。
- 客观技术继续引用生态插件，不因「提前框架」去自研物理。

**统一改规则时将触及**（先列清单，未改文件）：

- `docs/topics/engine/bevy-plugin-and-code-reuse.md` 准则二
- `docs/decisions/0002-engine-scope-game-driven.md` 与「预付复杂度」相关表述（需重写边界，不是整篇作废）
- `docs/topics/engine/capability-card-workflow-deep-dive.md` Rule of Three 段
- `templates/bevy-game/` README + AGENTS.md 同步句
- 根 `AGENTS.md` 若有「第二次才抽」的转述

## 5. 待继续讨论（开放）

- [x] 主链拆成哪些 SystemSet 阶段（对照现有 11 个系统）→ **方案已出（§7.1），待 youxia 拍板**
- [x] 玩法要不要拆成多个 Plugin（Combat / Wave / Pickup…）还是先只加 Set → **AI 结论：先只加 Set，不拆 Plugin（§7.2）**
- [x] 能力卡如何声明「挂在哪个阶段 / 依赖哪些 Message」→ 方向已定（§7.3），字段格式待收口
- [ ] **§7 方案整体确认**（五阶段命名 / 系统归位表 / 卡挂载格式）——下一会话首要拍板项
- [ ] 「标准化框架」第一版最小交付：只改 wave-survival 接线，还是同步改模板（AI 倾向：同步，见结论）
- [ ] ADR-0002 里「不做通用引擎」与「第一款就留接缝」的新措辞（AI 倾向：重写边界段，非整篇作废）
- [ ] 收口后统一执行的改动清单确认（§4.1 列出的 5 个规则文件 + §7 接线）
- [ ] 本节其余「主流 vs 愿景」区别，youxia 消化后再标保留/修改

## 8. 跨会话交接（下个会话先读这里）

> 本文是**工作底稿**：结论先记这里，未拍板前**不修改** AGENTS.md / ADR / 模板 / `game.rs` 调度。

**下个会话的议题清单**（按优先级）：
1. **拍板 §7**：五阶段命名、11 系统归位表（§7.1）、卡挂载声明格式（§7.3）——AI 已按专业判断出稿，默认按此执行，除非有异议
2. **拍板 §5 剩余项**：模板是否同步改（AI 建议同步）；ADR-0002 措辞（AI 建议重写边界段）
3. **已拍板不用再议**：§4.1 反对「第二次用到才抽」（youxia 已定）；§7.2 不拆多 Plugin（AI 结论）

**已确认背景**（不用重新讨论）：
- 结构痛点 = 主链 `.chain()` 手排，插入点靠人猜（§2）
- 主流（SystemSet）与愿景（能力卡装配层）兼容（§3）
- 卡挂载声明 = 装配层自动化的最小闭环（§7.3）

**执行纪律**（两个会话都遵守）：
- 未收口：不改 `game.rs` 调度、不抽 `engine/` crate
- 收口后：一次性改 §4.1 清单 + §7 接线，改动建议在 Windows 侧执行（game.rs 活跃开发中，避免双写冲突）
- 工作区有队友未提交的美术 WIP：提交只圈自己的文件，勿整目录 `git add`

## 6. 后续怎么用本文

## 7. 主链 SystemSet 拆分方案（2026-08-31，AI 专业稿，待 youxia 拍板）

### 7.1 阶段设计（技术结论，可执行）

把 `GamePlugin` 的 Update 主链从「一条 `.chain()` 手排」改为「**阶段 Set + 显式挂载**」：

```rust
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameSet { Movement, Combat, Despawn, Spawn, Observe }
// configure_sets(Update, (Movement, Combat, Despawn, Spawn, Observe).chain())
```

现有 11 系统的挂载：

| 阶段 | 系统 | 语义 |
|------|------|------|
| `Movement` | move_player, enemy_chase | 写位置/速度 |
| `Combat` | player_attack, nova_slash, contact_damage | 读位置 → 写 Hp/flash |
| `Despawn` | death_despawn | 清死体（必须在所有伤害结算后） |
| `Spawn` | pickup_drop, wave_system | 生成物 / 波次（依赖 Despawn 后的敌人计数） |
| `Observe` | decay_flash, derive_heading, update_walk_cycle | 观测/表现驱动（最后，读本帧结算结果） |

**收益**：
1. 新系统只需声明 `in_set(GameSet::X)` 或 `.after(...)`——**不再手改主链 tuple，人/AI 插卡安全**
2. 阶段 = 能力卡的「挂载点」：装配从「手排顺序」变「声明归属」
3. 阶段 `.chain()` 保证顺序，现有 11 系统相对顺序不变 → **回归零风险**（59 测试应原样全绿）
4. 测试可只跑某阶段（装配层自动化的地基）

### 7.2 结论：玩法不拆多 Plugin，先只加 Set（AI 判断）

- Set 管**时序契约**（哪个阶段），Plugin 管**领域边界**（谁负责什么）——两者正交。当前 11 系统都在游戏领域内，拆 Plugin（Combat/Wave/Pickup…）是文件级重组，**不解决「插入点靠人猜」**；Set 才解决。
- 拆 Plugin 的代价：插件间资源/消息可见性要显式声明，当前规模不划算。
- 第二款游戏要复用某玩法模块时，从 GamePlugin 抽该领域为独立 Plugin + crate 是**机械动作**——§4.1 的「接缝」（Set 阶段 + 消息契约）已提前备好。

### 7.3 能力卡挂载声明（装配层自动化前置）

卡 yaml 块增加两行，让「卡 = 装配指令」：

```yaml
能力卡: NovaSlash
挂载: GameSet::Combat       # 或 after: [Movement]
依赖消息: [NovaFired]
```

AI 读卡即知「挂哪个阶段、依赖什么消息」——这是装配层自动化的**最小闭环**（人不再手排，机器可安全插卡）。格式细节（字段名/是否进 frontmatter）待 §5 收口时定。

### 7.4 参考代码骨架（未落地，供讨论/执行备查）

```rust
// src/systems/mod.rs 或新 src/sets.rs
use bevy::prelude::*;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameSet {
    Movement, // 写位置/速度：move_player, enemy_chase
    Combat,   // 读位置→写 Hp/flash：player_attack, nova_slash, contact_damage
    Despawn,  // 清死体：death_despawn
    Spawn,    // 生成物/波次：pickup_drop, wave_system
    Observe,  // 观测/表现驱动：decay_flash, derive_heading, update_walk_cycle
}

// GamePlugin::build 内：
//   .configure_sets(Update, (GameSet::Movement, GameSet::Combat,
//                            GameSet::Despawn, GameSet::Spawn, GameSet::Observe).chain())
//   .add_systems(Update,
//       (systems::player::move_player, systems::enemy::enemy_chase)
//           .in_set(GameSet::Movement))
//   .add_systems(Update,
//       (systems::combat::player_attack, systems::nova::nova_slash,
//        systems::contact::contact_damage).in_set(GameSet::Combat))
//   ...
```

落地时注意：阶段 `.chain()` 保证顺序，现有 11 系统相对顺序不变 → 59 回归应原样全绿；若某系统需跨阶段依赖，用 `.after(GameSet::X)` 显式声明而不是塞进链中间。

## 6. 后续怎么用本文

1. 本会话继续分析 → 追加「讨论日志」和对应章节，不要另起多份碎笔记。
2. 收口后：一次性改 §4.1 列出的规则文件，并视需要立新 ADR（取代或修订 0002 的沉淀时间点）。
3. 未收口：不改 `game.rs` 调度、不抽 `engine/` crate（除非另开实现任务）。

## 参考

- 代码：`games/wave-survival/src/plugins/game.rs`、`lib.rs`、`components.rs`、`resources.rs`
- [[topics/engine/bevy-plugin-and-code-reuse]]
- [[decisions/0002-engine-scope-game-driven]]
- [[decisions/0004-handwritten-renderer-to-bevy]]
- [[topics/engine/capability-card-workflow-deep-dive]]
