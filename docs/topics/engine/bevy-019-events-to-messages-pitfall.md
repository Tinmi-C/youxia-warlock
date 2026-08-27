---
title: Bevy 0.19 缓冲事件更名 Message——Event/add_event 直接编译失败
type: pitfall
topic: engine/bevy
date: 2026-08-27
author: AI，待团队 review
severity: high
tags: [bevy, bevy_ecs, events, messages, api-migration]
related: [topics/engine/bevy-plugin-and-code-reuse, topics/engine/learning-roadmap]
---

# Bevy 0.19 缓冲事件更名 Message——Event/add_event 直接编译失败

## 现象

wave-survival 卡 9（NovaSlash）实现中，沿用既有事件习惯写 `#[derive(Event)]` + `EventWriter<T>` + `app.add_event::<T>()`，Bevy 0.19.1 编译期直接报错：

- `#[derive(Event)]` 用在纯数据消息上：`consider annotating {Self} with #[derive(Event)]` 的提示反而误导——derive 了 Event 却没有对应 trait 方法实现路径；
- `MessageWriter` 不存在时代的写法：`error[E0599]: no method named add_event found for App`；
- 同源连锁：`EventWriter`/`EventReader` 均不在 0.19 缓冲事件 API 里。

## 根因

Bevy 在 0.17~0.19 期间把「缓冲事件」拆成两个概念：

1. **Buffered events → Messages**：一对多的一次性通知（我们游戏里的 NovaFired）。类型要 `#[derive(Message)]`；写入端 `bevy::ecs::message::MessageWriter`；注册端 `App::add_message::<M>()`；存储容器是 `Messages<M>`。
2. **Event**（新语义）保留给 **观察者模式**：`Observer` 监听的触发器（`GlobalTrigger` 默认），和旧 buffered event 完全是两码事。

`add_message` 自动完成两件事：注册 `Messages<T>` 资源 + 保证每帧 First 调度里跑 `message_update_system` 做双缓冲翻转。测试想读当前帧消息可直接用 `Messages::len()` / `iter_current_update_messages()`。

## 解决

卡 9 实际落地的正确写法（已回归验证）：

```rust
// 定义：缓冲消息用 Message derive（0.19）
#[derive(Message)]
pub struct NovaFired { pub at: Vec3 }

// 注册：App 上
app.add_message::<NovaFired>();

// 写入端系统参数
mut nova_fired: MessageWriter<NovaFired>,
nova_fired.write(NovaFired { at: origin });
```

无头测试断言时不要假设「flush 与否」，用增量计数（写之前记 `len()`，写之后做差），对缓冲翻转运语天然鲁棒。

## 反思 / 防坑

- 升级锁定的引擎版本前，除了 release notes 要把「breaking change 清单」当能力卡前置任务过一遍；本次是凭肌肉记忆写了 0.15 时代 API，编译期被抓住——**这正是「验收句 + 回归测试」流程想要的效果**（错误在编译期/测试期暴露，而不是跑起来后行为悄悄不对）。
- 团队硬性规则「不编造 Bevy API，先查本地源码」在这里再次生效：把 ApiRenamed 类问题的检查口从编译期再往前挪一步——升级当版读 `bevy_ecs/src/event/mod.rs`、`bevy_app/src/sub_app.rs`（本次两个文件说清了全部真相）。
- 撞名类小坑顺手记录：bevy::prelude 也导出了 `Gradient`（bevy_ui 的），与 bevy_hanabi 0.19 的 `Gradient` glob 导入二义性——插件 prelude 尽量显式导入；hanabi 的 `ColorOverLifetimeModifier` 新增 `mask` 字段，结构体字面量初始化会 E0063，用官方构造函数 `::new(gradient)` 免维护字段清单。
