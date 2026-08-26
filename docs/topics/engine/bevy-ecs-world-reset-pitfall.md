---
title: bevy_ecs 重开游戏不能重建 World——两个 panic 的坑
type: pitfall
topic: engine/ecs
date: 2026-08-21
author: AI + youxia
severity: high
tags: [ecs, bevy_ecs, world, schedule, reset]
related: [decisions/0003-toy-ecs-to-bevy-ecs, topics/engine/engine-concepts-map]
---

# bevy_ecs 重开游戏不能重建 World——两个 panic 的坑

## 现象

m2-bevy（玩具 ECS → bevy_ecs 迁移版）按 R 重开游戏，两种写法各炸一种：

- 写法 1：`self.world = init_world()`（换一个新 World）
  ```
  A System cannot be used with Worlds other than the one it was initialized with.
    left: WorldId(0)  right: WorldId(1)
  ```
- 写法 2：`self.world.clear_all()`（同一个 World 清空重来）
  ```
  ResourceCache is in sync: NotSpawned(ValidButNotSpawned(
    EntityValidButNotSpawnedError { entity: 11v0 }))
  ```
  且崩溃点不在 clear_all 本身，而在之后**任何一次** `insert_resource`——我们每帧都写 `Delta`，所以重开一帧都撑不过去。

## 根因

两条根因，同一个主题：**World 不是无状态的容器，而是有绑定关系和内部缓存的长期对象**。

1. **Schedule 的系统和 World 绑定**。系统第一次 `run` 时在给定 World 上初始化（注册组件访问、缓存 archetype 状态），之后只认这个 WorldId。换新 World = 系统的缓存全部失效，bevy 直接 panic 拒绝。
2. **bevy_ecs 0.19 里资源也是实体上的组件**（每个资源挂在专属实体上，`resource_entities` 表维护 ComponentId → Entity 映射）。`clear_all()` 清掉了所有实体（包括资源实体），却**没有清 `resource_entities` 缓存表**——之后 `insert_resource` 查表拿到已死的实体，`expect("ResourceCache is in sync")` 炸掉。（0.19.1 实测；上游是否修复待关注。）

## 解决

学 Bevy 官方 App 的做法：**World 从创建起永不清空、永不替换**。重开 = 对场景实体做手术 + 原地重置资源值：

```rust
fn reset_scene(&mut self) {
    // 1. despawn 所有场景实体——用组件筛（资源实体不带 Position，天然排除）
    let ids: Vec<Entity> = {
        let mut q = self.world.query_filtered::<Entity, With<Position>>();
        q.iter(&self.world).collect()
    };
    for e in ids { let _ = self.world.despawn(e); }
    // 2. 重摆场景
    setup_scene(&mut self.world);
    // 3. 原地重置资源：insert_resource 对已存在的资源 = 原实体上替换值，缓存不失配
    self.world.insert_resource(Delta(0.0));
    self.world.insert_resource(Stats::default());
}
```

关键点：
- despawn 前先 collect 再操作（不能边迭代边改）；
- 「所有场景实体都带 Position、资源实体不带」——组件标签当筛子用，ECS 思想的直接应用；
- 回归测试钉死这条路径：`restart_after_death_survives_schedule_and_resources`（玩 → 击杀 → 死 → reset → 再玩 60 帧，两个坑任一复发即 panic）。

## 反思 / 防坑

- 「重建比修补干净」的直觉在 ECS 世界里是反的：**长生命周期对象做原地对账，不做推倒重建**。和 M1 学的「uniform 原地覆写而非重建 buffer」是同一个原则。
- 玩具 ECS 里 World 只是个数据包，随便换；工业 ECS 的 World 承载调度绑定、世代号、资源缓存，「换 World」是伤筋动骨的操作。这是玩具版教不了的肌肉记忆。
- 遇到 `expect(...)` 的 panic 消息，先去读那行库源码（这次直接翻到 `insert_resource_by_id` 的 `resource_entities.get(...)`），根因往往在消息之外的缓存失配里。
- 换正式 Bevy 后这件事由 States + `OnEnter` 插件接管，不用手动清；但在裸 `bevy_ecs` 层亲手踩一次，才知道插件在替你挡什么。
