# 引擎架构 Engine Architecture

ECS、场景图、资源管理、事件系统、生命周期、模块边界。

## 笔记列表

| 笔记 | 类型 | 状态 | 一句话 |
|------|------|------|--------|
| [[topics/engine/ai-native-engine-vision\|AI 原生游戏引擎——设想总纲]] | reference | draft | 愿景/架构/原则/路线图 |
| [[topics/engine/learning-roadmap\|学习路线图]] | reference | draft | 按里程碑边开发边学习 |
| [[topics/engine/engine-direction-discussion\|引擎定位与团队方向讨论纪要]] | reference | draft | demo 策略 + 自研=AI 装配层 + 环境工作流 |
| [[topics/engine/engine-concepts-map\|引擎概念地图]] | reference | draft | A-E 五层引擎概念速查 |
| [[topics/engine/bevy-plugin-and-code-reuse\|Bevy 开发约定：插件决策与代码沉淀]] | reference | draft | 客观→引用插件；主观→自研沉淀；第二次用到才抽 crate |

## 相关决策

- [[decisions/0001-rust-tech-stack|ADR-0001 基础技术栈选型]]（proposed）
- [[decisions/0002-engine-scope-game-driven|ADR-0002 引擎边界——游戏驱动、能力卡为核心资产]]（proposed）
- [[decisions/0003-toy-ecs-to-bevy-ecs|ADR-0003 玩具 ECS → bevy_ecs 迁移]]（accepted，2026-08-21 实施完成）
- [[decisions/0004-handwritten-renderer-to-bevy|ADR-0004 渲染底料开源化 → Bevy 全引擎]]（proposed：Bevy = 团队默认游戏底座）

## 相关踩坑

- [[topics/engine/bevy-ecs-world-reset-pitfall|bevy_ecs 重开游戏不能重建 World]]（high）：换新 World → Schedule 绑定 panic；`clear_all()` → 资源缓存失配 panic。正解 = World 永不清空，despawn 场景实体 + 原地重置资源。
- [[topics/engine/bevy-windows-antivirus-build-pitfall|Windows 杀软误报 ahash 构建脚本]]（medium）：360 主动防御拦/删 build-script exe → `os error 5`；给 `target/` + 工具链目录加白名单解决。
