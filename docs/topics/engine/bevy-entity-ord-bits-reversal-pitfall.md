---
title: Bevy 0.19 Entity 的 Ord 是 raw bits 比较（nonmax 取反）——直接排序得生成序倒序
type: pitfall
topic: engine
date: 2026-10-23
author: AI（FOG2-3 增量实证）
status: done
tags: [bevy, ecs, determinism, sorting]
related: ["[[topics/engine/warbase-wc3-alignment-plan|warbase 魔兽式底座补全方案]]"]
---

# Bevy 0.19 Entity 的 Ord 是 raw bits 比较——直接排序得生成序倒序

**现象**：小地图 blip 集合按 `Entity` 直接排序后顺序「看起来是反的」——后生成的实体排在了前面。

**根因**：bevy 0.19 的 `Entity` 派生 `Ord` 按 raw bits 逐位比较，而其内部 index 用 nonmax 存储做了**按位取反**（防 usize::MAX 哨兵）。bits 序 = index 的倒序。对 Entity 集合直接 `sort()` 得到的是生成序的**倒序**——仍是确定性的，但与「按生成先后」直觉相反，凡文档/断言写「按 index 升序」处即语义错误。

**解决**：排序键一律用 `Entity::index()`（u32）或 `index_u32()`，不依赖 Entity 的派生 Ord。warbase 各域（selection/movement/item）已全部如此，本仓无受害点（复查记录于 Phase 3 收口）。

**反思**：ECS 包装类型的派生 trait 语义不可想当然（bits 序 ≠ 逻辑序）；确定性代码里「稳定」和「符合直觉的序」是两回事，排序键要选**语义明确的数值**并在注释写明排序语义。
