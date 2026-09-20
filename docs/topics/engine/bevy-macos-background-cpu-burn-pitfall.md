---
title: Bevy 游戏 macOS 后台 CPU 爆炸（vsync 失效 + 每帧 Text 重排）
type: pitfall
topic: engine/bevy
date: 2026-09-18
author: zcode-session
severity: high
tags: [bevy, macos, performance, text, winit]
related: []
---

# Bevy 游戏 macOS 后台 CPU 爆炸（vsync 失效 + 每帧 Text 重排）

## 现象

warden（Bevy 0.19.1，窗口模式）在 macOS 上被切到后台/完全遮挡后，活动监视器显示
CPU 持续 **>100%**；日志显示帧率 ~120fps 且失焦降频不生效；同时日志里
`ICU4X data error: No segmentation model for complex script: Chinese/Japanese`
以 **~1200 条/秒** 的速率刷屏（3 分钟 24 万条）。

## 根因

三个独立问题叠加：

1. **每帧无条件重写 `Text` 组件**：`update_hud` 每帧 `t.0 = text.clone()`，即使内容
   没变。写入即“变更”→ bevy_text 每帧重新排版 → 中文文本走 cosmic-text/icu_segmenter
   分词 → bevy 自带的 ICU4X 数据**不含 CJK 分词模型** → `icu_provider` 每次 `log::warn!`
   报错。排版本身 + 日志 I/O 都是纯浪费的 CPU。
2. **`WinitSettings` 的 reactive 模式在 macOS 上不限帧**：默认 `game()` 配置失焦应降到
   低功耗，实测仍是 ~120fps（winit 事件循环在 macOS 会自我唤醒，Wait 形同虚设）。
   另外 macOS 上被完全遮挡的窗口 present 不再等待 vblank，vsync 也压不住。
3. **`Window::focused` 组件默认值是 `true`**（bevy_window 源码里 `focused: true`），
   窗口从未被点过时不会有任何 Focused 事件来纠正它——不能用它判断焦点。

## 解决

warden `6340fbd`，三管齐下，实测后台失焦 CPU **110% → ~19%**，帧率 ~120 → ~10fps：

- `update_hud` 加变更守卫：`if t.0 != text { t.0 = text.clone(); }`（一行，ICU4X 刷屏
  从 24 万条/3min 降到 43 条）。
- `LogPlugin` 过滤 `icu_provider=off`（正文渲染不受影响，只是没有分词模型时的警告）。
- 自建**睡眠式帧限制器**（`First` 里 `std::thread::sleep` 补齐最小帧时长）：聚焦
  60fps / 失焦 10fps。焦点状态自己从 `MessageReader<WindowFocused>` 事件维护，
  **启动时假设未聚焦**，收到第一个真实焦点事件前不信任任何默认值。

## 反思 / 防坑

- **任何每帧执行的 UI 系统，写 `Text` 前必须先比较**——字符串比较的成本相比整套
  重新排版可以忽略。中文 UI 尤其敏感：一次重排 = N 条 ICU4X 警告。
- macOS 上不要指望 winit 的 reactive 限帧，也不要指望遮挡窗口的 vsync；要限帧就
  自己 sleep（简单、跨平台、可测）。
- 引擎组件的默认值可能是“乐观值”（focused=true），第一次真实事件到达前不可信；
  关键状态自己从事件流维护。
- 排查这类问题先量化：`wc -l` 日志 + `ps -o pcpu` + 应用自己的 fps 打点，三组数字
  就能把“感觉卡”变成可归因的证据链。
