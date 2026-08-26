# 架构决策记录（ADR）

> 重要的技术选型和架构方向必须记 ADR，防止「当时为什么这么定」失忆，也让 AI 了解上下文。
> 新建时用模板：[[meta/templates/adr-template|ADR 模板]]（Obsidian Templates 插件插入）。

## 编号与命名

- 从 `0001` 递增，文件名 `0001-<kebab-case>.md`，如 `0001-use-wgpu-for-rendering.md`。
- 状态流转：`proposed` → `accepted` → `superseded by ADR-xxxx`（在原文件改 `status` 字段即可）。

## ADR 列表

| ADR | 标题 | 状态 | 日期 |
|-----|------|------|------|
| [[decisions/0001-rust-tech-stack\|ADR-0001]] | 自研引擎基础技术栈选型（Rust + winit + wgpu + glam） | proposed | 2026-08-19 |
| [[decisions/0002-engine-scope-game-driven\|ADR-0002]] | 引擎边界——游戏驱动、不做通用引擎、能力卡为核心资产 | proposed | 2026-08-19 |
| [[decisions/0003-toy-ecs-to-bevy-ecs\|ADR-0003]] | 从玩具 ECS 迁移到 bevy_ecs 的时机与边界 | accepted | 2026-08-19 |
| [[decisions/0004-handwritten-renderer-to-bevy\|ADR-0004]] | 渲染底料开源化——手写 wgpu 渲染器迁移到 Bevy 全引擎 | proposed | 2026-08-25 |
