---
title: Bevy 游戏内 UI 的自建视觉验收闭环——帧脚本场景 + 截图 + 双通道裁决
type: howto
topic: engine/bevy
date: 2026-10-06
author: AI，待团队 review
severity: medium
tags: [bevy, ui, screenshot, visual-verification, testing, ai-collaboration]
related: [topics/engine/automated-game-testing-and-ai, topics/engine/ai-collaboration-by-phase]
---

# Bevy 游戏内 UI 的自建视觉验收闭环——帧脚本场景 + 截图 + 双通道裁决

## 适用场景

能力卡带「视觉人工验收（⚠️）」项的 bevy_ui 界面（指令卡、技能钮、指示圈、HUD）。
headless 测试只能锁行为不变量，「像素上到底长什么样」一直靠人眼——本方法把人眼换成
**可重放的帧脚本场景 + 引擎内截图 + 视觉模型看图 + 引擎侧诊断日志**，AI 自己当 QA。

warden 首例：SK8/SK10 技能 UI（DB2 场景 `skill-card`，`cargo run -- --scenario skill-card` 一键产出 4 张验收截图后自退）。

## 做法（四件套）

1. **帧脚本场景**：DB2/调试场景加载目标 UI 状态（选中实体、给金币、放塔走真网关），
   插入一个 `FrameCounter` 资源；一个 Update 系统按帧号驱动状态变迁（点按钮=写同款
   消息/改组件、开 autocast=直接置标志、进入瞄准=写 CastSkill），并在关键帧截屏：

   ```rust
   // 截屏：引擎内 Screenshot，不依赖录屏权限
   commands.spawn(Screenshot::primary_window())
       .observe(save_to_disk("sk_card.png"));
   // 结束：AppExit::Success，脚本自退，CI/命令行友好
   ```

2. **引擎内截图**：`Screenshot::primary_window()` + `save_to_disk` observer。
   macOS 无 `timeout` 命令不碍事——场景自带 AppExit 自退。

3. **视觉通道**：截图交给视觉模型逐项核对验收句（布局、颜色、标记、截断/重叠）。
   prompt 里列明期望清单，让模型逐条回答「实际读到什么」。

4. **引擎侧诊断日志（裁决通道）**：截图前后一帧用 `info!` 打印系统真实持有的状态——
   按钮的 `(action, grid, badge)`、瞄准/光标/圈 Visibility、HUD 完整字符串。**两通道冲突时以日志为准**。

## 为什么必须有第 4 件（踩过的坑）

- **视觉模型对 9px 角标字母连续误读 3 轮**（把指令卡徽章 R/A/S 读成旧布局，与引擎
  日志直接矛盾，且三轮读数惊人一致——像是被首轮锚定）。小字号、有色、半透明的字符
  不可依赖视觉模型裁决。
- **无人值守跑真窗口时 OS 鼠标在窗外**：脚本 `window.set_cursor_position(...)` 写的
   字段会被 winit 的 CursorLeft/Moved 事件冲掉，指示圈因此判 Hidden 没进截图——需要
   伴生 PreUpdate 系统在瞄准窗口逐帧「钉住」光标字段（事件批处理先于调度，PreUpdate
   写入稳赢当帧读取）。
- 视觉通道的真价值在**布局/颜色/截断/3D 归位**这类一眼活；文字与数值让引擎日志说话。

## 流程闭环

跑场景 → 拿截图 → 双通道核对 → 发现问题修代码 → 重跑场景（帧脚本天然可重放）→
全绿后在能力卡把 ⚠️ 改 ✅ 并注明场景名。截图进 `.gitignore`（可随时重生成，不入库）。

## 反思 / 防坑

- 帧脚本驱动「点击」时，直接写系统同款输入（消息/组件标志）而不是模拟坐标点击——
  与 headless 测试同一条路径，减少第二套真相。
- 场景自带 AppExit，`cargo run` 即用；放在 `#[cfg(debug_assertions)]` 调试模块，release 零开销。
- 每个新 UI 卡把「⚠️ 人工验收」换成本闭环，是「验收句可数字化」又前进一格
  （见 [[topics/engine/ai-collaboration-by-phase]] 的分工表）。
