---
title: wgpu 第二步：三角形
type: learning
topic: graphics/wgpu
date: 2026-08-19
author: youxia
status: draft
tags: [wgpu, wgsl, graphics, m1, render-pipeline]
related: [topics/graphics/wgpu-window-clear, decisions/0001-rust-tech-stack]
---

# wgpu 第二步：三角形

配套代码：`games/m1-demo/src/main.rs`（M1 里程碑第 2 步，基于第 1 步增量修改）。

## 结论

在 Step 1 的清屏骨架上，往渲染通道里加一条 `draw` 命令：GPU 用我们准备好的 3 个顶点 + 两段 WGSL 小程序，把一个三角形画上屏。**改顶点改形状，改片元着色器改颜色——两条路完全分离。**

## 渲染管线：三个点如何变成屏幕上的三角形

```
顶点数据（CPU 准备，3 个点）
  → 顶点着色器 vs_main（每个顶点跑 1 次：这个点画在哪）
  → 光栅化（硬件自动：三角形切成像素格子）
  → 片元着色器 fs_main（每个像素跑 1 次：这个格子什么颜色）
  → 像素上屏
```

关键思维转变：CPU 代码「写一遍、顺序执行一遍」；GPU 代码「写一遍、**并行**执行 N 次」。三角形放大 → 盖住的像素变多 → 片元着色器跑得更多（这就是「画面越大越费 GPU」）。

## 新概念速记

| 概念 | 一句话 | 代码位置 |
|------|--------|----------|
| NDC 坐标 | GPU 认识的坐标系：屏幕中心 (0,0)，四角 (±1,±1)，+Y 朝上 | `VERTICES` |
| 顶点缓冲 | 把顶点字节上传 GPU 显存，CPU 写一次每帧复用 | `create_buffer_init` |
| WGSL | GPU 上跑的小程序语言，一个 shader 模块装 vs/fs 两段 | `SHADER` |
| 顶点布局 | 告诉 GPU 每个顶点在内存里怎么读（2×f32 = 8 字节） | `Vertex::desc()` |
| 渲染管线 | shader + 顶点布局 + 输出格式组装成的流水线，一次组装每帧复用 | `create_render_pipeline` |

## 与 Step 1 的关系：骨架不变，通道装新货

一帧六步（拿帧→视图→编码器→通道→提交→呈现）一行没动。变化全在**渲染通道**里：

```rust
// Step 1：只清屏
let _pass = encoder.begin_render_pass(...);

// Step 2：清屏 + 画三角形（通道里多了三行）
let mut pass = encoder.begin_render_pass(...);
pass.set_pipeline(pipeline);              // 装上流水线
pass.set_vertex_buffer(0, vb.slice(..));  // 喂顶点
pass.draw(0..3, 0..1);                    // 开画：3 个顶点
```

装备从五件套变八件（新增：Shader 模块 / 顶点缓冲 / 渲染管线），全部在 init 时创建一次。

## 已验证的实验（2026-08-19）

1. **改颜色**：`fs_main` 返回 `vec4f(0.2, 0.9, 0.4, 1.0)` → 三角形变绿（RGB 三盏灯，G 最亮）。
2. **改形状**：顶部顶点 Y 从 0.5 → 0.9 → 三角形变高变瘦（+Y 朝上，靠近屏幕顶边）。

## 你现在应该能回答的问题

1. 顶点着色器和片元着色器各自回答什么问题？（在哪 / 什么颜色）
2. 三角形放大 4 倍，两段着色器运行次数怎么变？（vs 不变 3 次；fs 变约 4 倍）
3. 一帧的六步骨架里，Step 2 动了哪一步？（都没动，动的是通道内容）
4. `draw(0..3, 0..1)` 的 3 是什么？（顶点数 = VERTICES 下标范围）

## 踩坑记录（写代码时实际遇到的）

- WGSL 里用了 `VertexInput` 结构但漏定义 → shader 编译失败（顶点布局 Rust 侧写了，WGSL 侧也要声明）。
- `device` 被 move 进 `self.device` 后还要用 → E0382。解法：管线创建挪到存储之前（也是 Rust 所有权的一次现场教学）。
- `create_buffer_init` 需要显式 `use wgpu::util::DeviceExt`（trait 方法不 import 不可用）。

## 下一步

第 3 步：方块 + 移动——索引缓冲 / 深度缓冲 / 变换矩阵 / 键盘输入 + delta time（MoveSystem 能力卡即将落地）。