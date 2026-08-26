---
title: wgpu 第三步：方块 + 移动
type: learning
topic: graphics/wgpu
date: 2026-08-19
author: youxia
status: draft
tags: [wgpu, m1, delta-time, uniform, input]
related: [topics/graphics/wgpu-triangle, topics/graphics/wgpu-camera-mvp]
---

# wgpu 第三步：方块 + 移动

配套代码：`games/m1-demo/src/main.rs`（M1 第 3 步）。本步起确立协作模式：**AI 写实现，人负责理解链路 / review / 验收**——MoveSystem 按 [[movesystem|能力卡]] 实现，由 youxia 按验收句实测验收。

## 结论

在三角形骨架上加四件事：索引缓冲、深度缓冲（先接线后上岗）、uniform（CPU 每帧传位置给 GPU）、键盘输入 + delta time。移动逻辑一句话：**位置 += 归一化方向 × 速度 × delta**。

## 新概念速记

| 概念 | 一句话 | 代码位置 |
|------|--------|----------|
| 索引缓冲 | 顶点复用序号：4 顶点 + 6 索引画 2 个三角形，省显存 | `INDICES` / `draw_indexed` |
| uniform + 绑定组 | CPU→GPU 的每帧小数据通道；绑定组 = 把缓冲「接线」到 shader 的 `@binding` | `Uniforms` / `bind_group` |
| write_buffer | 排进队列、下次 submit 生效——不是立即生效（防数据竞争） | `queue.write_buffer` |
| delta time | 帧间隔秒数；乘它把「每秒 X」换算成「这帧走多少」→ 速度与帧率无关 | `update()` 开头 |
| 归一化 | 斜向输入 (1,1) 长度 √2，不除回去会快 41% | `MoveSystem::update` |
| delta clamp | `.min(0.1)`：卡顿时变慢动作而非瞬移（鲁棒性换精确性） | `update()` |

## 已验证的实验（2026-08-19）

1. **帧率无关**：终端仪表每 0.5s 打印实测速度。speed 恒 0.600 NDC/s；按 F 把帧率压到 ~30fps，speed 不变。MoveSystem 验收句实测通过。
2. **测量条件教训**：盲按（按住-松开-再按）时 speed 显示 0.29——仪表算的是平均值，松手稀释了数据。**前提没控制住，数据不可信。**

## CPU/GPU 数据竞争（本步最值钱的一课）

CPU（记账员）可领先 GPU（施工队）最多 2 帧（`desired_maximum_frame_latency: 2`）。若 `write_buffer` 立即生效：GPU 画帧 N 时读到的已是 pos_N+1——**未来提前上演**，pos_N 永不上屏，表现为轻微抖动（不崩溃，最阴险）。解法：覆写也排队，先进先出，每帧拿自己的快照。

> 通用识别法：「谁和谁并行？谁读谁写？顺序谁保证？」

## 你现在应该能回答的问题

1. 斜向按 W+D，为什么速度还是 0.6？（归一化）
2. 拖窗口卡 2 秒，方块为什么没飞出去？（delta clamp）
3. write_buffer 为什么不能立即生效？（CPU/GPU 并行，数据竞争）
