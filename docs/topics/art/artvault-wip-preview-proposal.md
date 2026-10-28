---
title: 岩库侧提案——WIP 预审通道体验与看板速度字段（交付美术项目）
type: reference
topic: art
date: 2026-10-28
author: 游戏侧起草（warbase 项目），交岩库项目评审
status: proposed
tags: [art-vault, demand-board, proposal, cross-project]
related: ["[[topics/game-design/artvault-demand-board-proposal|需求板块设计稿]]", "[[topics/art/game-art-cross-project-workflow|跨项目工作流]]"]
---

# 提案：岩库需求看板两项增强（游戏侧诉求）

> 背景：跨项目工作流自审判定的两个岩库侧缺口（缺陷 #1/#6）。游戏侧已完成对应准备（category 自由字符串、manual 通道均不需要岩库改 schema）；本提案只动岩库侧体验层。

## 1. WIP 预审通道（解「审美返工往返贵」）

现状：状态机已有 producing（八站流程）与 review，但**游戏侧需求方看不到进行中的稿**——每次看不满意都要等 fulfilled 走完入库→同步→拉取全链。
诉求：producing 态下增加「预览稿」挂载位（草图/半成品图，任意站可传）；游戏侧经看板查看并可一键「方向 OK / 需调整+一句话」。**预览稿不进正式资产库、不算 fulfilled、版本不占号**——纯沟通载体。收益：主观类资产（图标/BGM）返工成本从一轮完整交付链降为一次看稿。

## 2. 看板速度字段（解「产能回路缺失」）

现状：游戏侧排美术批次时不知道美术吞吐，可能需求积压或游戏侧空等。
诉求：Demand 表加可选 `velocity` 侧栏数据（近 7 日 fulfilled 数/在制数/平均站内时长——聚合自现有 DemandEvent，无新表）；游戏侧 art-needs `status` 命令顺带拉取展示。收益：游戏侧排期有据。

## 边界

两项均为岩库侧实现与决策；游戏侧承诺的对应适配：预览稿反馈经看板留痕（不入 git）、velocity 只读消费。若岩库有更优形态（如预览走独立 review 页），以岩库设计为准。
