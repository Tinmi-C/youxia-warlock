---
title: wgpu 第四步：相机 + MVP
type: learning
topic: graphics/wgpu
date: 2026-08-19
author: youxia
status: draft
tags: [wgpu, m1, mvp, camera, depth-buffer, glam]
related: [topics/graphics/wgpu-quad-movement]
---

# wgpu 第四步：相机 + MVP（M1 终章）

配套代码：`games/m1-demo/src/main.rs`（M1 第 4 步）。三个彩色立方体 + 轨道相机 + 深度遮挡。

## 结论

顶点着色器从「直通管道」升级为「乘 MVP 的干将」：`clip_position = u.mvp * vec4f(position, 1.0)`。每个顶点每帧走完三段旅程——**M 摆位、V 找机位、P 按快门**。

## 三矩阵（拍照类比）

| 矩阵 | 拍照角色 | 职责 | 代码 |
|------|---------|------|------|
| M（Model） | 道具组摆件 | 本地 → 世界（位置/旋转/缩放） | `from_translation * from_axis_angle` |
| V（View） | 摄影师找机位 | 世界 → 相机眼前；**只搬运不缩放** | `look_at_rh(eye, target, up)` |
| P（Projection） | 镜头成像 | 眼前 → 屏幕；**除以距离 → 近大远小** | `perspective_rh` / `orthographic_rh` |

乘法顺序硬约束：`mvp = proj * view * model`（矩阵从右往左作用，顺序 = 三段旅程先后；不满足交换律）。

轨道相机没有实体，只有三个数字（yaw/pitch/distance）+ 球坐标公式 + look_at。

## 已验证的实验（2026-08-19）

1. **实验 3（透视 vs 正交）**：左右立方体同尺寸（变量隔离！），透视下近大远小，按 P 切正交后远近同大 → 铁证「近大远小 = P」。首答「V」已订正：V 只搬运不缩放。
   - 教训：第一版实验左右尺寸不同（0.9/0.7），被 youxia 抓到「变量未隔离」——**AI 设计的实验也要被验收**。
2. **实验 4（乘法顺序写反）**：`model * view * proj` → 立方体整体消失（预测 B 正确）。镜头吃到的料是还没进世界的本地坐标，乱算坐标大多被裁掉。

## 深度测试（Step 3 埋的伏笔上岗）

深度缓冲 = 每像素记录本，记「目前画的东西离相机多远」。两规则：

```rust
depth_compare: Less,        // 新来的更近才许画
depth_write_enabled: true,  // 画完更新记录
```

每帧从 1.0（无穷远）清起；谁近谁赢，**与绘制顺序无关**——3D 正确遮挡的全部原理。

## 你现在应该能回答的问题

1. 近大远小是谁干的？机制是什么？（P；除以距离）
2. MVP 乘法顺序为什么不能反？（右往左作用 = 旅程先后；反了喂错料）
3. 两个立方体重叠，先画谁有影响吗？（没有；深度测试只认距离）
