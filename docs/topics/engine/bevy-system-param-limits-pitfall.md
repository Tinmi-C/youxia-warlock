---
title: Bevy 系统参数两堵墙——元组 16 上限与 SystemParam derive 不收 mut Query
type: pitfall
topic: engine/bevy
date: 2026-10-06
author: AI，待团队 review
severity: medium
tags: [bevy, bevy_ecs, system-param, derive, api]
related: [topics/engine/bevy-plugin-and-code-reuse]
---

# Bevy 系统参数两堵墙——元组 16 上限与 SystemParam derive 不收 mut Query

## 现象

warden SK10 指令卡改造，两连击：

1. `handle_command_buttons` 加到第 19 个系统参数后，`.before(...)/.in_set(...)` 排序 API 直接不可用（编译器对超长元组不再实现排序 trait）；
2. 想用 `#[derive(SystemParam)]` 把参数捆成束降参数个数，束里含 `Query<&mut T>` 时报 lifetime 错：`the parameter … must be valid for 'static`（Local/mut Query 带生命周期参数，derive 生成的 'static 约束不满足）。

## 根因

- 系统参数元组实现到 16 元为止（bevy_ecs 的宏实现上限），超出后整个系统丢掉一堆 trait 实现，报错位置却在排序调用处，**报错点不是病根**；
- `SystemParam` derive 生成的关联类型要求各字段能以 `'static` 生存，可变 Query 的生命周期擦除不掉。

## 解决

```rust
// 1) 只把 Res / MessageWriter 等 'static 参数捆进束；可变 Query 留在系统签名里当独立参数。
#[derive(SystemParam)]
pub struct CardParams<'w> {
    pub defs: Res<'w, TowerDefs>,
    pub skills: Res<'w, SkillDefs>,
    pub balance: Res<'w, SkillBalance>,
    pub cast_requests: MessageWriter<'w, CastSkill>,
}
// 用法：fn handle_command_buttons(params: CardParams, mut q: Query<...>) { … }

// 2) derive 宏要显式引入（不在 prelude）：
use bevy::ecs::system::SystemParam;
```

`CardParams` 把 19 参降到 15，排序 API 恢复可用；束命名成领域词（卡片构建所需四件套），签名反而更可读。

## 反思 / 防坑

- 系统参数超过 ~12 个就是信号：不是先捆参，而是先问「这个系统是不是管太多了」。捆参是止痛，拆系统才是治本（本次指令卡确属单领域，捆参合理）。
- 症状在排序宏、病根在参数个数——遇到「明明该有的 trait 方法不见了」，先查元组长度/字段约束这类结构性上限。
- 束内参数依旧受 Res 冲突规则约束；Res 与独立 Query 混排没有 borrow 冲突（不同数据域）。
