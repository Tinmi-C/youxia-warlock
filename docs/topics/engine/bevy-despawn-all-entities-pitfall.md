---
title: Bevy 0.19 测试里 despawn 全部实体 → 首帧命令报 Entity invalid
type: pitfall
topic: engine
date: 2026-10-26
author: AI（ZCode，round4/ldebt 分支）
severity: medium
tags: [bevy, testing, minimal-plugins, ecs]
related: ["[[topics/engine/bevy-ecs-world-reset-pitfall|world 重置坑]]"]
---

# Bevy 0.19 测试里 despawn 全部实体 → 首帧命令报 Entity invalid

## 现象

headless 测试里想清空战场模拟「建造期空场」：

```rust
let mut q = world.query::<Entity>();          // 全部实体
for e in q.iter(world).collect::<Vec<_>>() { world.despawn(e); }
app.update();   // panic: "Entity despawned: The entity with ID 0v0 is invalid;
                //  its index now has generation 1" —— 某条 command 打在已
                // despawn 的实体上
```

报错来自 Bevy 内部（MinimalPlugins 首帧自己生成的实体）；`RUST_BACKTRACE`
看不到有效帧，且**裸 MinimalPlugins（不含任何自研插件）即可复现**。

## 根因

`MinimalPlugins`（Time/TaskPool 等）在第一次 `app.update()` 时会在 world 里
生成自己的内部实体（本例 17 个实体中 13 个是 Bevy 的）。`query::<Entity>()`
把内部实体一并清掉，Bevy 的系统随后对悬空 id 发 command → panic。
和 [[topics/engine/bevy-ecs-world-reset-pitfall|world 重置坑]] 同族：**world
不是「只属于你游戏实体」的容器**，Bevy 框架层在里面也有户口。

## 解决

测试只清**自己的**实体——按组件过滤，不要 `query::<Entity>()` 全清：

```rust
// 修法：只清场上单位
let mut q = world.query_filtered::<Entity, With<UnitInstance>>();
```

另注：在**首帧 update 之前**直接改 world（despawn/插入）同样危险，测试
构造完先 `app.update()` 一帧再动实体（legion_app 既有约定）。

## 反思 / 防坑

- 「清空战场」永远按组件过滤；`query::<Entity>()` 全清 = 把 Bevy 的家当
  也清了。
- 报 "ID 0v0 invalid, index now generation 1" 且 backtrace 无自研帧时，
  第一反应应是框架内部实体，不是自己的系统。
