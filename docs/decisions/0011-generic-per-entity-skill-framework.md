---
title: ADR-0011
type: adr
status: accepted
date: 2026-09-29
author: AI + youxia
---

# ADR-0011: 技能系统通用化——按实体挂载的技能框架（被动/主动/自动施法）+ 统一伤害结算

## 背景

warden 技能一期（SK1–SK4）落地了指挥官技能：流星火雨 / 冰霜新星 / 战意狂暴，Z/X/C 纯冷却制。但它是**全局资源制**（`SkillState` 全局冷却数组 + 全局 `SKILL_DEFS` 表），技能只能属于"指挥官"这一个隐式所有者。

二期需求（requirements §17）：融合塔招牌主动技、塔被动/光环等——技能的拥有者将变成**任意实体**（某一座塔、指挥官、未来的敌人/道具）。同时两个底层缺口暴露：

1. **属性加成散读**：`tower_fire` 内联查 `Boosts`（按塔种类索引，表达不了"某一座塔"的个体被动）和 `WarFrenzy`；被动技能（暴击/攻速）没有统一挂点。
2. **伤害两处直写**：近战即时命中与投射物命中各自 `hp -=`，暴击、灼烧、击杀赏金这类"命中时"效果没有干净的插桩点。

## 决策

**技能框架按实体挂载，分五层落地（参考魔兽争霸的单位技能模型）：**

1. **所有权层**：`SkillCaster { slots: Vec<SkillSlot> }` 组件，挂任意实体；`SkillSlot { def, level, cooldown, autocast }` 为实例状态。框架系统（冷却 tick、施法校验、目标选择、自动施法、升级）**不感知 owner 类型**——塔、指挥官单例实体、未来任何角色同一套代码。
2. **定义层**：`SkillDefs` 注册表 + `SkillDef { name, kind, targeting, max_level, badge }`。技能三分法 `SkillKind::{Passive, Active, Autocast}`，目标选择 `SkillTargeting::{None, Point, Unit}`。结构元数据与数值分离，数值一律进 `SkillBalance` 单源表（留在代码内，遵循 ADR-0008）。
3. **属性层**：source 标记的 `StatMods`（增/乘、可限时、可按来源增删）→ 每帧聚合出 `EffectiveStats`；`Boosts`、战意狂暴、Meta 强化、技能被动统一并入聚合，`tower_fire` 只读聚合结果。光环 = 入场加条目/离场删条目，天然支持。
4. **结算层**：所有伤害源只写 `DamageRequest` 消息；单一 `resolve_damage` 系统**独占写 `Enemy.hp`**（护甲、扣血、HitFlash、击杀判定在此），并发出 `DamageDealt`（含来源归因与 crit 标记）供 FX / 赏金 / 平衡读数消费。该模式已在 wave-survival 验证（知识库 `engine/unified-damage-pipeline`）。
5. **效果层**：每个技能的具体效果 = 按技能 id 分发的小系统（一期 `cast_skill` 的 match 分发模式推广）。**不用 trait 对象、不引入脚本**。加新技能 = 注册表加一行 + 新效果系统，框架零改动。

指挥官三技能（SK1–SK4）迁移到该框架上（挂指挥官单例实体），删除全局 `SkillState`；原验收句全部保留作回归护栏。

## 备选方案

1. **trait 对象 / 脚本化技能行为**：行为闭包或 Lua/RON 描述效果。否——非 Bevy 惯用形态、headless 断言难、对 3 人 + AI 团队是过度抽象；按 id 分发的小系统已满足"加技能不改框架"。
2. **每技能一个子实体**（skill as child entity）：ECS 纯度更高（每技能可挂观察者/独立组件），但生命周期（ despawn 顺序）、UI join、headless 测试成本都更高；`Vec<SkillSlot>` 与现有 `TowerDef` 索引模式一致，当前规模够用。若未来技能需要逐技能复杂状态再演化。
3. **维持全局资源制，加"角色"维度扩展**：`SkillState` 改成按实体 id 的 map。否——被动/光环仍无挂点，且每个新角色都要改框架代码，通用性是假的。
4. **伤害维持两处直写，仅旁路发事件**：改动最小，但暴击/DoT/击杀赏金要继续在多处插桩，双写风险常在。否——本次一并统一。

## 影响 / 后果

- 优：加角色/加技能不动框架；被动、光环、限时增益、暴击都有统一挂点；伤害归因（塔普攻 / 指挥官技能 / 塔技能）从结算层免费获得，BAL 技能贡献读数因此可行。
- 代价：`tower_fire` / 投射物命中等已验收系统要动（风险对冲：一期 SK1–SK4 验收句原样保留为回归护栏）；消息为双缓冲，测试断言用前后 `len()` 差值法。
- 实施按能力卡推进：SK5 框架核心 → SK6 属性管线 → SK7 伤害管线 → SK8 指挥官迁移 → SK9 弓箭手技能组（首个内容使用者）→ SK10 指令卡技能槽 UI → SK11 SkillBalance 单源表 + BAL 读数。
- 若未来出现"技能数量爆炸 / 逐技能复杂状态"，重开 ADR 评估子实体形态（备选方案 2）。
