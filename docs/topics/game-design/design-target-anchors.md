---
title: 设计目标锚点框架
type: howto
topic: game-design/design-target-anchors
date: 2026-09-02
author: AI + youxia
status: draft
tags: [平衡, 数值, 数据驱动, 调参]
related: [decisions/0008-numeric-data-in-code-not-external, topics/engine/bevy-plugin-and-code-reuse]
---

# 设计目标锚点框架

## 结论

先定义玩家要的体验（目标），再用数学反推数值；用一套 **5 字段"锚点"** 把"体感难易"变成**可测指标**，AI/人据此调数而不是靠猜。这是**跨游戏可复用**的数值设计方法论——塔防、RPG、放置、肉鸽等数值驱动类型都适用；**具体指标名和数值**各游戏自己的。

## 正文

### 核心原则

> 先定义玩家要的体验（目标），再用数学反推数值；数字不是随手拍的。

### 锚点标准模板（每个锚点 5 字段）

| 字段 | 说明 | 例 |
|------|------|-----|
| **类别** | 它管的体验维度 | 核心循环节奏 |
| **指标** | 能从数字算出的可测量量 | 击杀时间 / 贴脸时间 |
| **目标区间** | 设计决定：该落在哪 | ≥1.2× |
| **来源** | 为什么定这个数 | 从「坦克可先无视」反推 |
| **验收方式** | 怎么证达标 | headless 计算 / 实测 |

写不出来的锚点 = 这个体验维度还没理解清楚，先不定数字。

### 锚点类别（覆盖几乎所有数值型游戏）

1. **核心循环节奏**：TTK / 清怪时间 / 贴脸时间。
2. **生存与容错**：挨几下、能承受多少"犯错空间"。
3. **难度梯度 / 成长曲线**：随关卡/波次的难度形状 + 目标存活/通关点。
4. **经济模型**（塔防/RPG 才有）：产出速率 vs 消耗速率、升级节奏、通胀。
5. **决策取舍价值**：两种手段（单目标 vs 群攻）的价值比，让玩家有得选。

### headless 平衡读数闭环（怎么让 AI 高效调数）

1. AI 读定义表 → 看到全部数值与关系。
2. AI 跑 headless 平衡计算 → 得到各波 `击杀/贴脸/清波/生存` 等指标。
3. AI 拿它**对照锚点目标区间** → 标记 OK/OFF + 差多少。
4. AI 调对应 `*_mul()` / `Balance` 数值 → 重算 → 达标。
5. 达标 `tune:` 一卡一提交；过头一键回滚（数值改动单独一行，干净）。

关键：**读数不是另一个模型，用游戏同一套公式/数据源**（改数自动跟着变，单一事实来源）。

### 通用 vs 各藏

- **通用的（机制，将来抽进 `engine/` 那份契约）**：锚点的**类别** + **5 字段模板** + **headless 计算/验收方式**。这是"数值系统"里那层**验证与定位**的机制。
- **游戏各自的（schema）**：每个锚点的**具体指标名和区间值**。塔防填"经济产出/消耗比、塔射程/攻速"；wave-survival 填"怪 TTK、被咬口数"。

### 数值数据存代码（见 ADR-0008）

**数据驱动 = 数据与逻辑分离（系统读数据、不硬编码），≠ 外部化/数据库。** 当前数值留在 Rust 源码；迁移门槛见 [[decisions/0008-numeric-data-in-code-not-external|ADR-0008]]。

## 参考

- 落地实例：`games/wave-survival/docs/balance-anchors.md`、`games/wave-survival/src/systems/balance_audit.rs`、`games/wave-survival/src/bin/balance_report.rs`
- 决策：[[decisions/0008-numeric-data-in-code-not-external|ADR-0008]]
- 免重复准则：[[topics/engine/bevy-plugin-and-code-reuse|Bevy 开发约定：插件决策与代码沉淀]]
