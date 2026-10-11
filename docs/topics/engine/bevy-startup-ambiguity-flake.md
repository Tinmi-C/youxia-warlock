---
title: Bevy 同调度段系统无显式序 = 并行执行器歧义 = 偶发测试闪挂
type: pitfall
topic: engine
date: 2026-10-11
author: AI（WX6-6e 收口时踩中，8 连跑定位）
status: draft
tags: [bevy, scheduling, startup, flaky-test]
related: ["games/warden/src/plugins/meta.rs", "games/warden/src/systems/map.rs"]
---

# Bevy 同调度段系统无显式序 = 并行执行器歧义 = 偶发测试闪挂

## 现象

`upgrade2_boosts_next_run_starting_gold` 单独跑 5/5 绿，全量并发跑 4/6 挂，
报"upgrade 2 should grant +20 starting gold, left: 100 right: 120"。
没有任何代码改动引入——一次重构（meta 的 +20 金 vs 地图装配的 econ_start 重置）
之后"随机"出现。

## 根因

两个 Startup 系统——`apply_meta_on_run_start`（给 Resources.gold +20）与
`load_map` 里的 `assemble`（把 Resources.gold 重置为 RON 的 econ_start=100）——
**没有显式先后约束**。Bevy 对无依赖关系的系统并行调度，多线程执行器下
相对顺序逐次运行可漂移：先 +20 后重置 → 加成被抹（挂）；先重置后 +20 → 正确（绿）。
单测过滤时线程少/执行序稳定，所以单独跑恒绿——**"单跑绿、并发挂"是这类
歧义的典型指纹**（另一指纹：失败率既非 0 也非 100%）。

## 解决

显式排序：meta 的 Startup 链 `.after(systems::map::load_map)`
（warden plugins/meta.rs）。修后 8 连跑全绿。

## 反思

- Bevy 的自动并行是**契约真空下的自由**：系统间有数据读写顺序语义时，
  必须自己声明（`.before/.after/.chain`），执行器不替你猜。
- 同一调度的 Exclusive 系统（`fn(&mut World)`）之间有序，但 Exclusive 与
  普通 system 的相对序同样不保证——本例 assemble 在 exclusive `load_map` 里。
- 新增"改共享资源的 Startup 系统"时，扫一眼同段还有谁写这个资源；
  `ambiguous_with` 是给"确认无冲突"用的免责，不是默认该加的。
- 测试一旦出现概率性失败，先怀疑调度歧义，不要重跑碰运气。
