---
title: Bevy Messages 双缓冲只留最近一帧——跨帧断言必须逐帧 drain
type: pitfall
topic: engine/bevy
date: 2026-10-06
author: AI，待团队 review
severity: medium
tags: [bevy, bevy_ecs, messages, double-buffer, testing]
related: [topics/engine/bevy-019-events-to-messages-pitfall, topics/engine/automated-game-testing-and-ai]
---

# Bevy Messages 双缓冲只留最近一帧——跨帧断言必须逐帧 drain

## 现象

warden 技能系统二期（SK5/SK7）headless 测试连环失败，两种形态：

1. 断言「施放后冷却消息/伤害消息 ≥1」拿到 0：系统明明在跑、日志可证消息写过；
2. 用 `Messages::len()` 做跨帧累计断言（帧 A 记 len、帧 B 再读），数值凭空变小甚至归零。

## 根因

`Messages<T>` 是双缓冲容器，每帧 First 里 `message_update_system` 翻转：**翻转后旧帧未读消息直接丢弃**。两个推论：

- 跨多帧只在一个统一时间点 `read()`，中间帧的消息已经没了——0 事件是必然；
- `len()` 反映「当前缓冲」的量，翻转后从零起算，天然不能当跨帧累计计数器用。

## 解决

```rust
// 断言消息的正确姿势：帧循环内逐帧 drain（MessageCursor 是增量游标，
// 每帧 read() 掉的不会再重复出现），累加到帧循环外的变量上。
let mut hits = 0usize;
let mut cursor = MessageCursor::<DamageDealt>::default(); // 不在 prelude：
use bevy::ecs::message::MessageCursor;
for _ in 0..N {
    app.update();
    for _e in app.world_mut().read_messages(&mut cursor) {
        hits += 1;
    }
}
assert!(hits >= 1);
```

单帧内做差断言（写前记 len、写后做差）仍然成立——它只看一个缓冲方向。

## 反思 / 防坑

- 心智模型：Messages 是「本帧广播」，不是「收件箱」。任何跨越 `app.update()` 的消息断言都要在循环里逐帧取。
- 测试助手直接沉淀成文件级函数（warden behavior.rs 的逐帧 drain 助手），别在每个测试里重写——写错一次就是 0 断言假失败。
- 与 [[topics/engine/bevy-019-events-to-messages-pitfall]] 同根（双缓冲翻转），那篇记 API 更名，这篇记语义坑；两篇合读是 Messages 全貌。
