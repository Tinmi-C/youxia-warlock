---
type: learning
created: 2026-08-19
tags: [wgpu, ui, state-machine, m2-step3]
related:
  - "[[wgpu-quad-movement]]"
  - "[[wgpu-camera-mvp]]"
---

# M2 Step 3：UI 渲染 + 游戏状态机

## 核心概念

### 1. 两条渲染管线

游戏里现在有两条 wgpu render pipeline，共用同一个 render pass：

| | 游戏管线 | UI 管线 |
|---|---|---|
| 坐标空间 | 顶点 × MVP 矩阵 → 3D → 屏幕 | 顶点直接是屏幕坐标（NDC），不乘矩阵 |
| 深度 | `Less` + write（近挡远） | `Always` + no write（永远画上去） |
| 混合 | `REPLACE`（覆盖） | `ALPHA_BLEND`（半透明叠加） |
| 用途 | 游戏世界实体 | 血条、HUD、覆盖层 |

### 2. 渲染顺序

同一个 render pass 内：**先画游戏实体（8 个），再切到 UI 管线画 UI 元素（2 个）**。顺序不可反——UI 的深度是 `Always`（永远画），游戏的深度是 `Less`（近才画）；如果先画 UI（深度 0，最近），游戏实体的深度都比 0 远，`Less` 判定「不画」→ 游戏实体全消失。

### 3. 状态机 = 系统的开关面板

```rust
enum GameState { Playing, Paused, GameOver }

fn update() {
    // 相机控制（if 外面，每帧都跑）
    if state == Playing {
        input_system / chase / move / combat / ...  // if 里面，暂停时跳过
    }
    // uniform 上传（if 外面，每帧都跑）
}
```

两层控制正交：
- **标签（ECS）**：控制「这个系统处理**谁**」——MoveSystem 只碰有 Velocity 的实体
- **状态（状态机）**：控制「这个系统**要不要跑**」——Paused 时 MoveSystem 整个跳过

### 4. alpha 混合 vs clear

| | Step 2 的红屏 | Step 3 的覆盖层 |
|---|---|---|
| 机制 | `LoadOp::Clear(红色)` | `ALPHA_BLEND` 画半透明 quad |
| 效果 | 擦掉游戏画面，只剩红底 | 叠在游戏画面上，保留底层 |
| 灵活度 | 只能换底色 | 可以任意透明度/颜色/区域 |

## 踩坑记录

### wgpu 同 pass 内 depth-stencil 格式必须一致

**现象**：UI 管线创建时设 `depth_stencil: None`，运行时 `pass.end()` panic。
**根因**：wgpu 25.x 要求同一 render pass 内所有 pipeline 的 depth-stencil 格式与 pass 的 attachment 一致。游戏管线有 `Depth32Float` attachment，UI 管线用 `None` 不兼容。
**解决**：给 UI 管线设占位 `DepthStencilState { format: Depth32Float, depth_compare: Always, depth_write_enabled: false }`——格式兼容 pass，行为等价无深度。
**反思**：wgpu 的 validation 规则比文档先到——「逻辑上不需要」≠「可以省略」。以后遇到 `None` 的 panic，先查 pass 的 attachment 格式。

## 能力卡

- [[gamestate|GameState]]（架构卡）
