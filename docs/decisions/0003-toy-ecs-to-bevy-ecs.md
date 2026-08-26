---
title: 从玩具 ECS 迁移到 bevy_ecs 的时机与边界
status: accepted
date: 2026-08-19
deciders: youxia + AI
supersedes: []
---

# ADR-0003：从玩具 ECS 迁移到 bevy_ecs

## 背景

M2 Step 1 手写了 ~80 行玩具 ECS（`Vec<Option<T>>` 池子 + 手动循环筛选），目的是理解 ECS 的核心机制：组件=数据、系统=函数、标签=筛子、多写手覆盖雷。这些概念已通过 M2 三步的实操和概念纠偏内化。

玩具 ECS 的局限在 M2 后期开始显现：
- 12 个系统手动排在 `update()` 里，顺序靠人记
- 单线程，无法并行跑互不冲突的系统
- 无变更检测，无法知道「这帧谁变了」
- 无 archetype 分组，千实体级缓存命中率差
- 组件定义绑死在 m2-demo 项目里，无法跨项目复用

## 决策

**在 M3（模型/纹理/光照）开始之前，将玩具 ECS 迁移到 `bevy_ecs`。**

- 迁移时机：M2 完整闭环后（git 提交 m2-demo）、M3 开工前
- 迁移边界：**组件定义原样保留**（Position、Velocity、Health 等 struct 不变），变的只是查询语法和系统调度
- 能力卡同步迁移：`if state == Playing` → bevy 的 `SystemSet` + `run_if`
- 玩具 ECS 代码保留在 m2-demo git 历史中，作为教学参考

## 迁移范围预估

| 改什么 | 不改什么 |
|---|---|
| 系统函数签名（手写循环 → `Query` 参数） | 组件 struct 定义 |
| 系统调度（手动排列 → `app.add_systems` + 依赖） | 游戏逻辑本身 |
| GameState（struct 字段 → `Resource`） | 能力卡的 WHAT 描述 |
| sync_entity_gpu（手动对账 → bevy 的 extract/render） | 渲染管线（wgpu 那套不变） |

预计迁移量：12 个系统改签名 + 调度重排，1-2 天。

## 不迁移的理由（为什么不现在换）

- M2 三步刚跑完，12 个系统的手动排序还能管住
- 没到需要并行的规模（10 个实体）
- 第二个游戏还没开始，跨项目复用需求未到
- 玩具版的教学价值已兑现——现在换不会丢失任何知识

## 迁移的触发信号（提前换的条件）

如果出现以下任一信号，提前迁移：
- 系统超过 15 个，手动排序开始出错
- 需要并行（物理和渲染分开跑）
- 第二个游戏项目启动，需要复用 ECS 层

## 理由

bevy_ecs 是 Rust 生态最成熟的 ECS 实现（ADR-0001 已选定）。手写玩具版的唯一价值是教学——价值已兑现。继续用玩具版会开始产生「为了维护玩具版而写的胶水代码」，这是沉没成本。在 M3 代码量膨胀前迁移，成本最低。

## 实施记录（2026-08-21）

- 新项目 `games/m2-bevy`（m2-demo 原样保留作教学参照）。组件定义、游戏数值、渲染管线全部原样，只换 ECS 层。
- 迁移方式与预估一致：12 个系统改 `Query`/`Res` 签名，`.chain()` 显式排序 + 两个 `ApplyDeferred` 结算点（对齐玩具版 death→pickup 的两段式）。
- 行为一致性验证：日志仪表（hp/ kills / pickups / draw_calls / state）与 m2-demo 对齐 + 击杀/拾取/死亡/GameOver 链路实测通过；R 重开链路由回归测试 `restart_after_death_survives_schedule_and_resources` 钉死。
- 迁移中踩坑：bevy_ecs 重开不能换 World 也不能 `clear_all()`（两个 panic，详见 [[topics/engine/bevy-ecs-world-reset-pitfall|踩坑笔记]]）。解决方式 = World 永不清空，despawn 场景实体 + 原地重置资源。
- 与原方案的一处偏差：GameState 暂仍为 App 层字段而非 bevy `Resource` / States——渲染层（相机/暂停观察）本就跑在 ECS 外，M3 前再决定是否上移。
