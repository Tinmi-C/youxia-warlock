---
title: ADR-0001
type: adr
status: proposed
date: 2026-08-19
author: youxia（团队）
---

# ADR-0001: 自研引擎基础技术栈选型（Rust + winit + wgpu + glam）

## 背景

团队决定自研 Rust 3D 游戏引擎（见《AI 原生游戏引擎——设想总纲》）。第一步需要确定基础技术栈：语言工具链、窗口、图形 API、数学库。团队无游戏开发经验但有编程基础，目标是边开发边学习。

## 决策

- 语言：Rust（stable，2021 edition，用 rustup 管理）
- 窗口 / 事件：`winit`
- 图形：`wgpu`（WebGPU 实现，跨平台）
- 数学：`glam`
- 序列化 / 配置：`serde` + `ron`
- 日志：`tracing`；性能分析：`puffin`（后期可上 `tracy`）
- ECS：**暂缓决定**——M1 阶段用手写简单 ECS 或 `bevy_ecs` 起步，M2 里程碑再定并另立 ADR
- 音频：`kira` / `rodio`（M3 阶段再定）

## 备选方案

- **现成引擎（Godot / Unity / Unreal）**：学习成本低、生态成熟，但违背「自研引擎」目标，否。
- **裸 Vulkan（ash）**：学习最底层、可控性最强，但开发效率低、新手劝退，否。
- **裸 OpenGL（glow）**：技术过时，新项目不推荐，否。
- **nalgebra 替代 glam**：功能更全但更重；glam 对游戏场景足够且性能更好，选 glam。
- **bevy 全量框架**：ECS/渲染/资产全包，但框架替你做了引擎的大部分，违背自研目标；可拆用其 `bevy_ecs` crate 借鉴思想。

## 影响

- `wgpu` 屏蔽 Vulkan / Metal / DX 底层差异，学习曲线最友好，跨平台（Win / macOS / Linux / Web）。
- 需要学习 WGSL 着色器语言。
- wgpu 生态仍在演进，API 可能有破坏性变更，锁定稳定版本并记录升级。
- 本 ADR 状态为 proposed，团队 review 通过后改 accepted；任何替换需另立 ADR。
