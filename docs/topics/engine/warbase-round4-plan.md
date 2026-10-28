---
title: warbase 第四轮方案 v4——美术就绪 + 多人债收口（含三条调研修正）
type: reference
topic: engine
date: 2026-10-28
author: AI 起草（ZCode），youxia 批准
status: accepted
tags: [architecture, warbase, plan, art-pipeline, multiplayer]
related: ["[[decisions/0020-warbase-round4-art-readiness|ADR-0020]]", "[[topics/art/art-development-roadmap|美术路线图]]", "[[topics/engine/warbase-round3-plan|方案 v3]]"]
---

# warbase 第四轮方案 v4

> 目标：美术解禁前置**做完且演练**；legion 七洞上收（含多人经济分域）；warden 塔阵容立项。基线 588 绿，制度沿用。
> **三条调研修正**：①状态机不加态（岩库已有态+零校验，缺口=岩库侧预审体验→提案文档交付）；②UI2D 需求零工具改动可落地（manual 通道现成，工具增量=自动投影+外观过滤）；③loop-td 锁定链路从未演练→加入 DRILL 增量做首产日彩排。

## Phase 0 SEAM4（主线）
- SEAM4a：本方案 + ADR-0020 + 岩库提案（WIP 预审+看板速度字段，交付美术项目）
- SEAM4b：UiTextures 伴生资源（照 UiFont 模式）+ ArtMode 双形态资源 + lib 一次注册（并入 UITEX 线首增量执行——本轮仅 UITEX 碰 ui/lib，无并行冲突）

## Phase 1 三线并行（八增量）
1. **UITEX**：贴图模式全链（消费收口：引擎 refresh_command_panel 根+槽 / loop-td panel_node+rebuild+两槽面；ImageNode+NodeImageMode::Sliced 九宫格；solid_color 回退；缓存键拼贴图 revision；--art 开关默认灰盒零漂移）+ 离屏双形态对照图
2. **NEEDS**：scan --ui（art-ui.json 声明清单→manual 卡）+ SfxKind 14 变体投影 + 外观过滤（hp 改动静默 / body_radius 改动产 changed）+ 单测
3. **LDEBT**：L2 Wallets 分域（LocalPlayer 兼容、killer Owner 入账、单人零漂移、legion 三账本退役）+ L5 队伍存活变体 + L7 MatchFlow::Ended{Win,Draw} + L8 SpawnRequest 有主注入 + L1 path walkable + L3 pop_cost

## Phase 2 两线
4. **WARDEN-ROSTER**：塔阵容重做（设计稿→def→弦箭挂回）
5. **DRILL**（依赖 UITEX）：程序化演练贴图走全环（art-assets 锁清单→art-sync→UiTextures→--art）→《首产日操作手册》

## Phase 3 收口
全量回归 + vault 收口 + 岩库提案交接 + 双形态对照展示页。约 16-18 增量；人工关卡零新增（H1 四图仍待用户）。
