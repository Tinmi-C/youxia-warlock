---
title: Bevy 0.19 首次注册资源/消息会分配簿记实体——实体 index 整体偏移（世界指纹对账必坑）
type: pitfall
topic: engine
date: 2026-10-22
author: AI（RPL1 录像线实证）
status: done
tags: [bevy, ecs, replay, determinism, state-hash]
related: ["[[topics/engine/warbase-wc3-alignment-plan|warbase 魔兽式底座补全方案]]", "[[topics/engine/bevy-messages-double-buffer-drain-pitfall|Messages 双缓冲坑]]"]
---

# Bevy 0.19 首次注册资源/消息会分配簿记实体——实体 index 整体偏移

**现象**：录像回放对账（录制局与回放局的逐帧世界指纹 StateHash）永不相等，且差异从第 0 帧就存在；逐项排查发现回放局全部实体 id 比录制局整体 +1。

**根因**：bevy 0.19 中**首次注册**某资源/消息（如 `insert_resource::<RecordingLog>` 或消息存储首次创建）会 spawn 一个裸簿记实体（Bookkeeping entity）占用一个实体 index。录制局比回放局多注册了一个 RecordingLog（录制时插入、回放早期版本未插），簿记实体使后续所有业务实体的 index 集体偏移——凡是把 `(Entity::index, …)` 计入指纹的对账，index 不对齐则永远对不上。

**解决**：让对账双方**资源结构与注册时序完全对称**（回放局也插入同款 RecordingLog 资源，即使回放不写它）。结构对称后 index 序列对齐，指纹可逐帧相等。

**反思**：①确定性 ≠ 指数稳定：世界指纹类对账（录像/快照/网络同步）对「资源注册历史」敏感，设计对账时先钉死「两个世界的资源集与时序必须同构」；②Bevy 的实现细节（簿记实体占 index）属于易碎知识，升级版本时要重验——该断言已写进 `engine/warbase/src/replay/mod.rs` 模块头，升级 bevy 时先跑 rpl_domain 回归。
