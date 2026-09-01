---
title: 能力卡机制深度理解——卡是什么、成果是什么、怎么切
type: reference
topic: engine/workflow
date: 2026-08-27
author: AI + youxia
status: draft
tags: [capability-card, workflow, ai-collaboration]
related: [topics/engine/ai-native-engine-vision, topics/engine/bevy-plugin-and-code-reuse, "games/wave-survival/docs/capability-cards"]
---

# 能力卡机制深度理解——卡是什么、成果是什么、怎么切

## 结论

能力卡是**开发时的需求规格 + 验收标准**，不是运行时被「使用」的东西——游戏跑起来后卡退场，生效的是组件 + 系统。卡的价值在于：把模糊需求逼成可数字化的验收句，让 AI 实现有契约、人有验收依据。

## 正文

### 卡在流程里的位置

```
想做一个新功能
  ↓
先问：现有组件/系统能不能拼出来？
  ├─ 能拼 → 直接组合（不立卡或立很小的卡）
  └─ 拼不出来 → 立卡（接口/行为/验收句）→ AI 实现 → 人验收 → 测试钉死 → 一卡一提交
```

关键分工：
- **卡（立不立、验收句写什么）= 人决定**。AI 不能替人决定「这个需求不用立卡」。
- **组件/系统（复用还是新写）= AI 决定**。拿到卡后扫代码库，缺积木才造。

### 卡 ≠ 需求的简单搬运：三个软字段

以 wave-survival 卡 2（PlayerAttack）为典型，字段最全：

- **设计来源**：数值从旧项目继承（m2 CombatSystem），不从零设计
- **设计变更**：GDD 歧义处的团队决策（0.9~1.5 之间线性衰减）——没有这个字段，半年后没人知道代码里为什么除以 0.6
- **依赖**：声明卡不依赖未完成的东西（测试桩怪物），使卡可独立验收

### 卡的成果 = 增量 + 测试 + 归档

一张卡落地后留下三样东西：

1. **最小代码增量**：新组件/新系统（缺积木才造）+ 扩展旧组件 + 组装逻辑
2. **回归测试**：验收句 1:1 翻译成 `tests/behavior.rs` 断言——卡的「长生」形式
3. **归档文档**：卡片本身 + 一个 commit

「新机制 = 新组件 + 新系统，老系统零改动」是默认原则不是铁律：wave-survival 卡 6 改了 `death_despawn` 加一行掉落（掉落天然发生在死亡时机，硬拆反而过度设计）。原则真正含义是**能不改就不改，必须改时改动小且卡上可追溯**。

### 切卡的维度：可独立验收的玩法链路

- 按**玩法能力**切（PlayerAttack / CombatContact / PickupDrop / GameLoop），不按技术层切（「所有组件一张卡」没法写验收句）
- 两个特征：行为边界 = 读写边界（卡 2 明确「死亡归卡 5，本卡只扣血」）；验收可执行 = 卡的最小尺寸
- 检验方法：每张卡能独立走完「立卡→实现→验收→测试→一个 commit」闭环；做到一半要依赖未做的卡 = 切大了或顺序错了

### 复用率证据：wave-survival 阶段一 8 张卡用量表

| 卡 | 复用 | 新增 |
|----|------|------|
| 1 PlayerMove | Player、Transform | 无（模板已有） |
| 2 PlayerAttack | Attack/Hp/Visual | 4 常量 + 伤害公式 + 1 系统，**0 新组件** |
| 3 WaveSystem | 组件全复用 | Wave 资源 + 1 系统 + 3 公式 |
| 4 EnemyChase | Velocity（rapier） | **Chasing 组件** + 1 系统 |
| 5 CombatContact | 全复用 | 2 系统；扩展 Hp（+max/invuln） |
| 6 PickupDrop | Hp | **Pickup 组件** + 1 系统；改 death_despawn 一行 |
| 7 GameStateUI | 只读 Hp/Attack/Wave | 5 个 UI 标记组件 + 2 系统 |
| 8 GameLoop | 以上全部 | 零新代码，纯测试 |

规律：**新增越来越少、复用越来越多**；新增组件只在全新机制时出现（追踪→Chasing，掉落→Pickup）；UI 标记组件是「接线端子」不是逻辑。

### 跨项目重复需求：三层防浪费 + 延迟提炼

新游戏遇到相同需求时，流程每次都走（保证可控），但产物被继承：

1. **设计继承**（卡上「设计来源」字段）：数值/公式照抄，只写差异
2. **代码复用**：共享 crate / 直接依赖，实现都不用重做
3. **经验沉淀**：踩坑记录、API 备忘躺在知识库里，新卡越来越薄

配套原则（= ADR-0002 game-driven 在知识管理的体现，**2026-08-31 复盘 §4.1 修正**）：**第一款就把可装配接缝留标准**（阶段 / 插件边界 / 消息与组件契约），但**不为无人消费的功能提前造通用模块**。抽象太早 = 为不存在的需求设计接口，改起来更贵；但「等到第二款才抽」接口也已是猜的——正解是**先留接缝，复用时机到再机械抽**。

## 参考

- wave-survival 卡清单与卡 2 全文：`games/wave-survival/docs/capability-cards.md`
- [[topics/engine/bevy-plugin-and-code-reuse|Bevy 开发约定]]（第二次用到才抽 crate）
- [[decisions/0002-engine-scope-game-driven|ADR-0002 引擎边界]]
