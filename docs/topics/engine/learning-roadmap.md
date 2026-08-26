---
title: 学习路线图（边开发边学习）
type: reference
topic: engine
date: 2026-08-25
author: team
status: draft
tags: [learning, roadmap, rust, bevy]
related: [ai-native-engine-vision, decisions/0004-handwritten-renderer-to-bevy, bevy-plugin-and-code-reuse]
---

# 学习路线图（Bevy 路线 · v2）

> 原则：**按里程碑学，不按书本学**——每个里程碑需要什么就学什么，学完立刻用上。
> **v2 修订（2026-08-25）**：ADR-0004 后 Bevy 为团队默认底座，路线从「手写 wgpu」改为「用 Bevy 做游戏」。v1（wgpu 手写版，M1-M3 已执行完）见 git 历史。
> 每个里程碑的产出 = 可玩的游戏增量 + 能力卡 + 踩坑记录（`type: pitfall`）。

## 阶段 0：工具就绪

- Rust 基础：cargo / 所有权 / 结构体枚举（够用即可，边写边补）
- Bevy 0.19：从 `templates/bevy-game` 起项目；会跑、会测、会用日志仪表
- 插件生态：会用 [bevydepy.com](https://bevydepy.com/popular?bevy=0.19) 查版本对齐；按「客观→插件 / 主观→自研」决策（[[topics/engine/bevy-plugin-and-code-reuse|开发约定]]）
- 产出：项目能跑 + 回归测试绿

## M1 垂直切片（能玩一局）—— wave-survival

- **核心循环打通**：移动 → 刷怪 → 战斗 → 掉落 → 死亡 → 重开
- Bevy 全家桶：glTF 场景、Camera3d、输入、States 状态机、bevy_ui、AnimationGraph
- 能力卡：PlayerMove / PlayerAttack / WaveSystem / CombatContact / PickupDrop / GameStateUI / GameLoop
- 产出：**能完整玩一局的游戏骨架** + 每张卡一个回归测试（垂直切片的意义：最早暴露系统接口问题、最早验证好不好玩）

## M2 玩法深化

- 物理：bevy_rapier3d（碰撞/受击/角色控制器机制）
- 粒子：bevy_hanabi（范围斩/受击特效）
- 敌人分化 + 难度曲线；手感调参（bevy_egui 调试面板）
- 能力卡：NovaSkill / EnemyVariants / DifficultyCurve …
- 产出：玩法对齐 m2 且有增量，数值全可调

## M3 表现层

- 骨骼动画正式化、UI 打磨（血条/波次/冷却）、音频（bevy_audio）
- 资产管线规范（assets/ 分目录：models/textures/audio/fonts/ui）
- 产出：ADR-0004 → accepted；游戏「好看」

## M4 打磨与发布

- 性能（实体多时上 InstancedMesh）、连续游玩稳定性、打包（`cargo build --release`，可选 Web）
- 产出：可发布的成品

## 资源

- **Bevy 官方 examples**（`https://github.com/bevyengine/bevy/tree/v0.19.1/examples`）——API 以它为准
- Bevy Book / Learn Bevy 教程
- bevydepy.com（按 Bevy 版本查插件）
- 团队知识库：能力卡工作流、踩坑笔记、ADR

## 纪律

- 每个功能 1 张能力卡（验收句数字化、可执行）
- 每个坑 1 条 pitfall（现象 / 根因 / 解决 / 反思）
- 「学完能讲给同事听」才算学会
