---
title: rapier 物理步进不受游戏状态门控——暂停画面怪物继续滑行
type: pitfall
topic: engine/physics
date: 2026-08-28
author: AI 沉淀（wave-survival 卡 16 实战），待团队 review
severity: high
tags: [bevy, rapier, physics, pause, game-state, ecs]
related: [topics/engine/bevy-019-events-to-messages-pitfall, topics/engine/bevy-plugin-and-code-reuse]
---

# rapier 物理步进不受游戏状态门控——暂停画面怪物继续滑行

## 现象

wave-survival 卡 16 加了 P 暂停遮罩后人工验收发现：暂停时怪物**还在动**（模型持续
漂移逼近玩家），但咬合/战斗日志已停止——即「逻辑停了，位移没停」。

## 根因

rapier 的物理流水线在自己的调度里步进，**不受我们游戏状态门控的影响**：

1. `GameState::Paused` 只 gate 了 GamePlugin 的 Update 链——链停转只是**不再写入新速度**；
2. 怪物在验收反馈#4 改成了 `RigidBody::Dynamic`（零重力+锁旋转），动态刚体会被
   rapier 每一步**持续积分体内残留的旧速度**——于是暂停期间怪物按最后的速度继续滑；
3. 更早的 KinematicVelocityBased 时代同样存在此问题（kinematic 速度也在物理步内积分），
   只是当时暂停画面没人盯着方块看，未被目击。

## 解决

`sync_physics_pause`（game.rs）：把游戏状态镜像到 rapier 的官方开关——

```rust
// bevy_rapier 0.36: RapierConfiguration 是挂在物理上下文实体上的组件（prelude 可导出）
let active = *state.get() == GameState::Playing;
for mut config in &mut configs {
    if config.physics_pipeline_active != active {
        config.physics_pipeline_active = active;
    }
}
```

暂停 = 整个物理世界冻结（含怪群挤压的残余微动）；恢复 = 管道重开，
`enemy_chase` 首帧重写速度，无缝续战。否决过的备选：暂停时遍历清零速度——
域内打补丁、和引擎对着干，且治不了未来其它动态体的同类泄漏。

## 反思 / 防坑

- **门控自己的系统 ≠ 门控仿真**：第三方插件有自己的调度和步进，接入任何外部
  stepper（物理/动画/音频时钟）时，要审计一遍「我的状态门控对它是否生效」，
  不生效就补一个状态镜像系统。
- **headless 测试为什么没拦住**：pause 的行为测试只断言了状态迁移与实体清理，
  从未断言「暂停期间位移不零漂移」——运动冻结类行为需要位移断言
  （补丁同时加了 `paused_world_freezes_and_resumes_monster_motion`：
  暂停 30 帧位置零漂移 + 恢复 30 帧必须复动，双向钉死）。
- 该修复来自人工视觉验收而非测试——表现层问题依旧高度依赖真机走查，
  观察通道（日志/测试）要持续把「看起来」翻译成「可断言」。
