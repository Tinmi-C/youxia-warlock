# AGENTS.md — m2-demo 项目上下文

> 本文件给「在这个游戏项目里工作的 AI」看。全局规则见根目录 `/AGENTS.md`。

## 项目是什么

M2 里程碑 demo：俯视角战斗小游戏（玩家 / 追踪怪 / 漂浮怪，后续加战斗、掉落、拾取）。
同时也是玩具 ECS 的教学载体（手写版，之后再决定是否换 bevy_ecs，见 ADR-0001 的暂缓项）。

## 技术

- 暂不依赖引擎 crate（引擎层按 ADR-0002 从本 demo 中提炼，尚无 `warlock-engine`）
- Rust 2024 + winit 0.30 + wgpu 25 + glam + bytemuck + pollster（与 m1-demo 一致）
- 渲染骨架复用 M1 Step 4 模式：五件套 + per-entity uniform + 深度测试

## 目录

- `src/main.rs` — 单文件（Step 1；实体和系统变多后按 ECS / 渲染 / 系统拆模块）

## 当前状态 / 下一步

- Step 1 完成：玩具 ECS（World + Vec<Option<T>> 组件池）+ InputSystem / ChaseSystem /
  MoveSystem / BounceSystem + 10 实体场景（地板 / 玩家 / 追踪怪 ×2 / 漂浮怪 ×6）
- 已知限制：实体间无碰撞（下一步战斗系统的活）；实体不能运行时增删（掉落物需要）
- 下一步：战斗系统（碰撞检测 + 生命值 + 死亡/掉落 → 需要给 World 加 despawn）

## 项目专属规则

- Review 标记约定：关键审查点用 `[REVIEW N]` 注释标出，教学用
- 漂浮怪用固定表驱动（不用随机数），保证行为可复现、可验收
- 双 target 编译：Linux 本机验证编译，`--target aarch64-apple-darwin --target-dir target-mac` 跑窗口
