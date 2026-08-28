---
title: 两个观察系统共用锚点组件——后读者恒见零位移
type: pitfall
topic: engine/patterns
date: 2026-08-28
author: AI 沉淀（wave-survival 卡 18 实战），待团队 review
severity: high
tags: [bevy, ecs, system-design, observer, component-ownership, regression]
related: [topics/engine/capability-card-workflow-deep-dive, topics/engine/bevy-plugin-and-code-reuse]
---

# 两个观察系统共用锚点组件——后读者恒见零位移

## 现象

wave-survival 卡 18（玩家朝向）：`derive_heading` 复用卡 12 走路系统的
`PrevTranslation` 锚点观测玩家位移，上线后朝向**从不更新**。更险的是回归没全拦住：
按 W 的测试「通过」了——因为 W 方向恰好等于出生默认朝向（+Z），恒零位移的 bug
被默认值掩盖；直到按 A（+X）的前置断言才露馅。

## 根因

两个观察系统在同一实体上共用同一锚点组件，且链条顺序为 walk → heading：

1. `update_walk_cycle`（先跑）每帧把 `prev.v` 写成**本帧移动后**的位置；
2. `derive_heading`（后跑）读到的 prev 永远是「刚刚那一帧的终点」，
   `delta = here - prev` 恒为 0，低于 MIN_SPEED 阈值 → 朝向永远走 hold 分支。

锚点组件本质是「上一帧状态」，被两个读者共用时，**先写者偷走了后读者的时间差**。

## 解决

两步，顺序不能反：

1. **先定所有权**：玩家锚点归卡 12 walk 系统所有，`derive_heading` 对玩家分支
   **只读不写**（写了会把 walk 的 delta 清零，走路动画直接死）；怪物锚点仍由
   derive 自有自写（实体集不同，互不干扰）。
2. **再排链条**：`derive_heading` 排到 `update_walk_cycle` **之前**——
   这样 heading 读到的 prev 还是上一帧的值，delta 就是本帧位移；
   walk 随后照常读「本帧起点 vs 终点」。

暂停/重启的锚点重锚由既有 `clear_walk_on_pause` 统一处理（两系统都受益），
恢复后不会产生假位移。

## 反思 / 防坑

- **「上一帧状态」类组件是观察者间的共享资源**：每个实体集上必须有且只有一个
  owner-writer，其余观察者一律只读；执行顺序跟着所有权走。新观察系统接入前先问：
  这个锚谁写、我读到的时点是什么。
- **「碰巧通过」的测试是真凶**：断言方向要与默认值/初值不同，否则测试变成
  恒真陷阱。本次 W 测试（默认朝向恰为 +Z）就是恒真的——A 测试才抓到。
  写回归时问一句：把被测行为改成什么都不做，这条断言会红吗？
- 卡 18 的修复经验直接复用进了卡 19 的设计审视（敌人锚点仍归 derive 独占），
  印证「沉淀 = 下一次实现的输入」。
