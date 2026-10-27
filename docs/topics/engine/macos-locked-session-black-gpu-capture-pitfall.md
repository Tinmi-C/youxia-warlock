---
title: macOS 锁屏会话下一切"窗口表面"GPU 捕获皆黑（含窗口内离屏 Image）——必须无窗口 headless
type: pitfall
topic: engine
date: 2026-10-28
author: AI（STAB 线实证，ZCode）
status: done
tags: [bevy, macos, screenshot, gpu, lock-screen, ai-workflow]
related: ["[[topics/engine/multi-worktree-shared-target-pitfall|多 worktree 共享 target 坑]]"]
---

# macOS 锁屏会话下一切"窗口表面"GPU 捕获皆黑

**现象**：凌晨无人值守（会话锁屏）跑真机截图：warden `--ui-shots`（`Screenshot::primary_window`）6 张全黑；loop-td 的成熟 `--ui-mock v2` 同条件下重跑也全黑（MD5 与 warden 的黑图逐字节相同 = 2560x1440 全零纹理）；把主相机改投离屏 `Image` 纹理后 `Screenshot::image` **依旧全黑**。最小探针 app（相机→Image+红块，无 bevy_egui）时红时黑（结果随运行抖动）。同批代码白天/会话活动时全过。

**根因**：锁屏会话（登录窗口接管、游戏窗口不在任何显示器上）下，只要 app **存在 winit 窗口**，渲染管线的产出就不可用——不是"截图 API 读错了纹理"：render schedule 每帧都跑（探针计数 90/90）、`ExtractedCamera` 目标正确（`Some(Image …)`）、`ViewTarget` 每帧存在、探针的 GPU readback 本身能工作（PNG 编码写出真像素数据，只是内容全零）。即"有窗口的 app 在锁屏下画不出内容"。两次失败纪律触发点：①窗口路径不可修（loop-td 同坏=机器态非代码 bug）；②窗口内 Image 离屏也不可修（探针抖动=不是确定性配置问题）。**根治**来自 MMP 线（c59bda9）：完全无窗口（`DefaultPlugins` + `primary_window: None` + `disable::<WinitPlugin>()` + 手动 `finish()/cleanup()`），UI 相机投 Image + `Screenshot::image`——零窗口/零显示服务/零锁屏依赖，昼夜可跑、确定性。

**解决**（STAB 落地，tests/ui_shots.rs + c811d10）：
- 照 MMP 骨架：`Image::new_target_texture(1280, 720, Rgba8UnormSrgb, None)` + `COPY_SRC`，`RenderTarget::Image(handle.into())` + `IsDefaultUiCamera` 挂游戏相机，`RenderPlugin { synchronous_pipeline_compilation: true }`，`#[ignore]` GPU 测试 + 手动驱动 `app.update()`；
- settle 用**墙钟**不用帧数（headless 紧循环里虚拟时间几乎不走——动画/波次按 `Delta` 推进，帧数 settle 会得到静止世界）；
- hover 用注入的 `PrimaryWindow` 实体 + `set_cursor_position`（无 OS 鼠标，bevy_ui focus 系统读这个窗口实体；MMP `stage_window` 技巧）；
- **非黑守卫**：第二个 observer 挂 `ScreenshotCaptured`，对 readback payload 采样断言 max>24——黑图当场红，不再靠人眼兜底；
- `screencapture -x` + 视觉模型确认机器处于锁屏（区分"显示睡了"（caffeinate 可救）与"会话锁了"（只有 headless 能救））。

**反思 / 防坑**：
- "引擎内截图"有两级：读窗口 drawable（锁屏必黑）与读自建 GPU 纹理（无窗口时可靠）。**有窗口存在本身就足以让后者也黑**——别在窗口 app 里修离屏，直接无窗口。
- 无人值守跑批前先 `screencapture -x` 看一眼机器态；结果抖动（同配置时红时黑）= 环境级故障伪装成代码级，按两次失败纪律停手排查环境，不要往源码叠补丁。
- 目检通道也可能骗你（视觉模型/CDN 缓存返回旧图）：PNG 亮度图本地解码是最后的事实仲裁。
