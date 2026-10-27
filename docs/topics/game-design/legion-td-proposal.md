---
title: 军团战争图需求草案——第三张验收图（多人对抗半区）
type: reference
topic: game-design
date: 2026-10-27
author: AI 起草（ZCode），youxia 批准（默认拍板项）
status: draft
tags: [game-design, legion-td, multiplayer, acceptance-map]
related: ["[[topics/game-design/wc3-rpg-td-map-genres|品类调研]]", "[[topics/engine/warbase-round3-plan|方案 v3]]", "[[decisions/0019-warbase-round3-maturity-and-legion|ADR-0019]]"]
---

# 军团战争图（games/legion）需求草案 v0.1

> 定位：第三张验收图——多人对抗半区。验证基底 MP（外交/发兵方）/生产/经济在对抗编排下的复用效率（KPI 对比 rts-lab）。美术灰盒（方案 A token）。

## 1. 核心循环（WC3 军团战争式）

回合制节奏：**建造期（30s）→ 战斗期（自动）→ 结算 → 下一回合**。玩家在建造期花钱雇兵布到自己路的布阵区；战斗期己方雇佣兵沿路攻打对家、对家来兵攻打己方防线；漏怪扣命，命尽出局，LastTeamStanding 定胜负。

## 2. 数值锚点（起点值，全部可调）

| 项 | 值 |
|---|---|
| 玩家 | 4 位（P0-P3，两队 2v2 或各自为战——bundle teams 段决定，默认 2v2） |
| 布阵区 | 每路 12 格（FreeGrid 分区） |
| 雇佣兵 | 6 种（cheap/swarm/tank/ranged/aoe/support，攻甲矩阵克制） |
| 买兵 | 30-120 金（人口占用 1-4） |
| 发兵 | 给指定对手注入下一波：25 金起，同目标递增 10% |
| 回合收入 | 50 基础 + 击杀赏金（雇佣兵 def bounty） |
| 命 | 每家 20，漏 1 扣 1，扣尽出局（发兵方获得击穿赏金） |
| 回合数 | 20 回合封顶 → 剩余命数多者胜 |

## 3. 增量切分（KPI 记录内置）

- **LG0**：立项（模板复制/workspace 入列/四人各一路 bundle/灰盒场景）+ 首个可跑切片——**从零到可跑的耗时与 rts-lab RL0-1 对比**
- **LG1**：建造期（雇佣兵商店+布阵+人口）+ 自动交战（雇佣兵 MoveAgent 攻坚：AttackMove 至对家底线）
- **LG2**：发兵（EconCommand 语义扩展或游戏侧注入——发兵走 wave director 的 per-player 注入缝）+ 结算 + LastTeamStanding
- **LG3**：方案 A UI 套用（商店卡/命数板/回合横幅）+ H1 清单

每增量记录：游戏侧新增代码行数 / 耗时 / 触发的新洞数——结项时出**复用效率对比表**。

## 4. 明确不做

netcode、AI 对手（发兵由本地真人或简单 bot 驱动——bot 走 harness Greedy 先例）、贴图美术。

## 5. 验收

`cargo test --workspace` 全绿零漂移；LG0-3 全断言钉死（买兵扣费/发兵递增/漏怪扣命/出局与胜负）；KPI 对比表出炉；H1 试玩清单（四图合并关卡的一部分）。
