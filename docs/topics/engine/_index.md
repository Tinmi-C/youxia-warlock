# 引擎架构 Engine Architecture

ECS、场景图、资源管理、事件系统、生命周期、模块边界。

## 笔记列表

| 笔记 | 类型 | 状态 | 一句话 |
|------|------|------|--------|
| [[topics/engine/ai-native-engine-vision\|AI 原生游戏引擎——设想总纲]] | reference | draft | 愿景/架构/原则/路线图 |
| [[topics/engine/learning-roadmap\|学习路线图]] | reference | draft | 按里程碑边开发边学习 |
| [[topics/engine/engine-direction-discussion\|引擎定位与团队方向讨论纪要]] | reference | draft | demo 策略 + 自研=AI 装配层 + 环境工作流 |
| [[topics/engine/engine-concepts-map\|引擎概念地图]] | reference | draft | A-E 五层引擎概念速查 |
| [[topics/engine/bevy-plugin-and-code-reuse\|Bevy 开发约定：插件决策与代码沉淀]] | reference | draft | 客观→引用插件；主观→自研沉淀；先留可装配接缝、复用时机到再机械抽（2026-08-31 收口，原「第二次用到才抽」已改） |
| [[topics/engine/capability-card-workflow-deep-dive\|能力卡机制深度理解]] | reference | draft | 卡=需求规格非运行时组件；成果=增量+测试+归档；切卡=可独立验收的玩法链路 |
| [[topics/engine/ai-collaboration-by-phase\|游戏全生命周期的人机分工]] | reference | draft | AI 可接管程度 = 验收句可数字化程度；立项/需求/试玩/构建分工表 + 点子池模式 |
| [[topics/engine/ai-feature-pipeline-sop\|AI 特性开发标准流程 v1（SOP）]] | howto | done | 六阶段三栏（AI/工具门禁/人关卡）+ DoD 五条 + 上下文包模板；试点=武器卡 29-31 |
| [[topics/engine/project-structure-and-dev-rules-review\|项目结构与开发规则复盘（进行中）]] | reference | draft | 读 wave-survival 结构；主流 Plugin+Set vs 能力卡愿景；§7 主链 SystemSet 已落地（59 回归绿）；§4.1 规则已改（先留接缝不提前造）；ADR-0002 边界澄清 |
| [[topics/engine/unified-damage-pipeline\|统一伤害结算管线——让 Hp 成为全场唯一写入者]] | howto | draft | 伤害走 DamageRequest 消息；apply_damage 唯一结算；GameSet::Resolve；加新技能发请求即可不碰 Hp |

## 相关决策

- [[decisions/0001-rust-tech-stack|ADR-0001 基础技术栈选型]]（proposed）
- [[decisions/0002-engine-scope-game-driven|ADR-0002 引擎边界——游戏驱动、能力卡为核心资产]]（proposed）
- [[decisions/0003-toy-ecs-to-bevy-ecs|ADR-0003 玩具 ECS → bevy_ecs 迁移]]（accepted，2026-08-21 实施完成）
- [[decisions/0004-handwritten-renderer-to-bevy|ADR-0004 渲染底料开源化 → Bevy 全引擎]]（proposed：Bevy = 团队默认游戏底座）

## 相关踩坑

- [[topics/engine/bevy-ecs-world-reset-pitfall|bevy_ecs 重开游戏不能重建 World]]（high）：换新 World → Schedule 绑定 panic；`clear_all()` → 资源缓存失配 panic。正解 = World 永不清空，despawn 场景实体 + 原地重置资源。
- [[topics/engine/bevy-windows-antivirus-build-pitfall|Windows 杀软误报 ahash 构建脚本]]（medium）：360 主动防御拦/删 build-script exe → `os error 5`；给 `target/` + 工具链目录加白名单解决。
- [[topics/engine/bevy-019-events-to-messages-pitfall|Bevy 0.19 缓冲事件更名 Message]]（high，待团队 review）：`#[derive(Event)]`/`EventWriter`/`add_event` 全部失效 → `Message` + `MessageWriter` + `add_message`；附无头测试读消息的增量计数法。
- [[topics/engine/powershell-pipeline-utf8-corruption-pitfall|PowerShell 文本管道写坏 UTF-8 源码]]（high）：`Get-Content | -replace | Set-Content` 按 ANSI 双重转码 → rustc 报非法 UTF-8；改文件只走编辑器类工具或显式编码 API。
- [[topics/engine/rapier-stepping-ignores-game-states-pitfall|rapier 物理步进不受游戏状态门控]]（high，待团队 review）：暂停只 gate 了自研系统，动态刚体带残速继续被积分 → 暂停画面怪物滑行；用 `RapierConfiguration.physics_pipeline_active` 镜像游戏状态，配位移双向回归钉死。
- [[topics/engine/shared-anchor-component-order-pitfall|观察系统共用锚点组件——后读者恒见零位移]]（high，待团队 review）：「上一帧状态」锚点被两个系统读写，先写者偷走时间差；锚点定唯一 owner-writer、其余只读，执行顺序跟所有权走；附带「断言方向要区别于默认值」的恒真测试教训。
