---
title: wgpu 第一步：窗口 + 清屏
type: learning
topic: graphics/wgpu
date: 2026-08-19
author: youxia
status: draft
tags: [wgpu, winit, graphics, m1]
related: [decisions/0001-rust-tech-stack, topics/engine/learning-roadmap]
---

# wgpu 第一步：窗口 + 清屏

配套代码：`games/m1-demo/src/main.rs`（M1 里程碑第 1 步）。

## 这一页在做什么

打开一个 800×600 窗口，用深蓝色清空画布。代码很短，但它建立了 **wgpu 的最小运行骨架**——后面所有渲染（三角形、方块、光照、模型）都长在它上面。

## wgpu 的核心心智模型：5 件套

```
Instance（入口）→ Surface（窗口画布）→ Adapter（显卡）
  → Device + Queue（设备与命令队列）→ SurfaceConfig（画布配置）
```

| 对象 | 类比 | 作用 |
|------|------|------|
| `Instance` | 驱动总管 | 探测本机图形后端（Vulkan/Metal/DX12） |
| `Surface` | 画布 | 把「窗口」绑成 wgpu 的渲染目标 |
| `Adapter` | 显卡 | wgpu 挑一张合适的物理设备 |
| `Device` | 显卡的逻辑句柄 | 创建管线、纹理等资源 |
| `Queue` | 命令提交通道 | 把渲染命令提交给显卡执行 |
| `SurfaceConfig` | 画布参数 | 尺寸 / 像素格式 / 呈现方式 |

## 渲染一帧的流程

```
get_current_texture() 拿画布帧
  → create_view() 帧纹理的视图（命令操作视图）
  → create_command_encoder() 命令编码器（先记录，不执行）
  → begin_render_pass() 渲染通道（这里是清屏）
  → queue.submit() 提交命令给显卡
  → frame.present() 呈现到窗口
```

关键概念：**命令不是立即执行的**——先编码、后一次性提交。这是 GPU 的工作方式（批量执行效率高）。

## 事件驱动（winit 0.30）

winit 用 `ApplicationHandler` trait 回调应用：

- `resumed()`：应用恢复时创建窗口 + 初始化图形（macOS 会多次调用，需判重）
- `window_event()`：处理窗口事件（关闭 / 重绘 / 尺寸变化）
- `RedrawRequested` → 我们渲染一帧；当前是「持续重绘」（为后面动画做准备）

## 你现在应该能回答的问题

1. Surface 和 Window 什么关系？（绑定关系：surface 把窗口当画布）
2. 为什么要 Adapter 再 Device？（先选物理设备，再拿它的逻辑句柄）
3. Clear 和 Store 各是什么？（通道开始怎么处理 / 结束后是否保留）
4. 窗口尺寸变化为什么必须重新 configure？（画布尺寸要和窗口一致）

## 下一步

第 2 步：画一个三角形——引入渲染管线（shader）和顶点缓冲。
