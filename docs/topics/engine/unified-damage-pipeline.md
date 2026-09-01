---
title: 统一伤害结算管线——让 Hp 成为全场唯一写入者
type: howto
topic: engine/architecture
date: 2026-09-01
author: AI（编译）
status: draft
tags: [bevy, ecs, message, damage, architecture, schedule]
related: [topics/engine/bevy-019-events-to-messages-pitfall, topics/engine/capability-card-workflow-deep-dive]
---

# 统一伤害结算管线——让 Hp 成为全场唯一写入者

## 结论

游戏里所有「造成伤害」的来源（普攻/技能）**不再自己扣血**，而是发一条 `DamageRequest` 消息；由唯一的 `apply_damage` 系统在 `GameSet::Resolve` 阶段统一结算。`Hp` 一帧只被这一个系统写入 → N 个技能不抢 `&mut Hp`、不会重复结算。以后加新技能照这个模式发请求即可，不用再碰 `Hp`。

## 正文

### 架构与调度（一条 damage 从产生到生效的帧内顺序）

```
技能/攻击（GameSet::Combat）   只发 DamageRequest，不碰 Hp/Visual
        │
        ▼
apply_damage（GameSet::Resolve）  唯一的 Hp 写入者，一次落地本帧所有请求
        │
        ▼
death_despawn（GameSet::Despawn）  读扣完的 Hp 判生死
```

`GameSet` 新增 `Resolve` 阶段（`Movement → Combat → Resolve → Despawn → Spawn → Observe`），见 `src/sets.rs`。

### 三个关键文件

- `systems/damage.rs`：`DamageSource` 枚举（`Slash` / `Nova` / `Contact`，随来源增长）、`DamageRequest { target, amount, source }`（`#[derive(Message)]`）、`apply_damage`（唯一结算点）。
- `sets.rs`：`GameSet::Resolve`。
- `plugins/game.rs`：`add_message::<DamageRequest>()` + 把 `apply_damage` 挂到 `GameSet::Resolve` 且 `run_if(in_state(Playing))`。

### 给新伤害技能用的配方

1. 技能系统**只读目标位置**（共享 `&`），算出「打谁、打多少」。
2. `wrt.write(DamageRequest { target, amount, source })`，**不碰 `Hp`/`Visual`**。
3. 若是全新伤害来源，往 `DamageSource` 加一个变体（用于日志/击杀归属）。
4. 无需在管线里再注册任何东西——`apply_damage` 已经会 drain 本帧所有请求。

### 为什么（对应「一百个技能」的担忧）

- **单一写入者**：`Hp` 只被 `apply_damage` 写，技能系统之间不会因 `&mut Hp` 冲突而被迫串行。
- **单一结算**：每条请求只被应用一次（只有一个 apply 路径），杜绝「重复结算」。
- **可并行**：技能系统只读目标位置，天然可并行调度。
- **帧语义**：Bevy 以「帧」为最小时间单位；「同时攻击」=「同一帧发多条请求」。`apply_damage` 每帧执行一次，内部 `for req in reader.read()` 批处理本帧所有请求——**不是「每个技能跑一次结算」，而是「一次系统执行、循环处理 N 条」**。

### 观察通道（注意）

命中/伤害日志从旧的 `[combat] ... hit monster` 变成 `[dmg] Slash/Nova/Contact hits ...`，咬人另有 `[contact] player bitten`。跑游戏看到 `[dmg]` 前缀是新格式，不影响逻辑/测试。

### 职责划分 / 当前边界

- `apply_damage` 只做「扣血 + 设白闪」，**不判无敌帧/护盾**。
- 玩家无敌帧（0.9s）仍由 `contact_damage` 管——它保留「何时能咬」的门控 + 一帧一口，然后才发请求。以后给技能加无敌帧/护盾等防护，应在「请求或结算处」扩展，别塞进 `apply_damage`。

### 迁移纪律（把旧系统迁进来）

把旧生产者迁进管线：把 `&mut Hp`/`&mut Visual` 查询改成只读 `(Entity, &Transform)`，命中判定原样保留，命中后改为发请求。伤害数值、时机不变 → 行为等价；用 `cargo test`（回归）+ `cargo check`（零警告）验证。

## 参考

- 实现：`games/wave-survival/src/systems/damage.rs`、`sets.rs`、`plugins/game.rs`、`systems/{combat,nova,contact}.rs`
- 提交：`5b6ac27`（引入管线 + 迁 player_attack）、`941a563`（迁 nova_slash + contact_damage），均已在 `origin/main`（= `941a563`）
- 相关：[[topics/engine/bevy-019-events-to-messages-pitfall]]（`Message` 三件套：`#[derive(Message)]`/`MessageWriter`/`add_message`）、[[topics/engine/capability-card-workflow-deep-dive]]（卡即需求规格）
- 验证：59 回归 + 3 lib 单测全绿，零警告
