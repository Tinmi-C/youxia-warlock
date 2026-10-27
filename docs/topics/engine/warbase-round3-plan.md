---
title: warbase 第三轮方案 v3——成熟度收口 + 军团战争图（复用效率实证）
type: reference
topic: engine
date: 2026-10-27
author: AI 起草（ZCode），youxia 批准
status: accepted
tags: [architecture, runtime, warbase, plan, legion-td, kpi]
related: ["[[decisions/0019-warbase-round3-maturity-and-legion|ADR-0019]]", "[[topics/engine/warbase-round2-plan|方案 v2]]", "[[topics/game-design/legion-td-proposal|军团图需求草案]]"]
---

# warbase 第三轮方案 v3

> 目标：①清债收口（12+ 洞/债）；②呈现层补齐至「可对外演示」；③第三张图（军团战争）量化基底复用效率。
> 制度沿用 v1/v2 全部（资源纪律/零漂移/失败两次复盘）。基线 workspace 530 绿。

## Phase 0：SEAM3（主线单会话）

| 增量 | 内容 |
|---|---|
| SEAM3a | ADR-0019 + 本方案 + 军团图需求草案 |
| SEAM3b | 八项清债：StatKind+Range；WaveTag 变体化；shipped 图允许 items 段（装备表回归 loop.ron）；CommandAction 开放注册缝；LeakPolicy::PassToNeighbor 基底行为化；ItemStack 实体化（INV source 转正）；UiTheme display 字阶 + PanelKey revision 热换肤；共享 CJK 字体（assets/ 根，去重 17.7MB×N） |
| SEAM3c | 观战回放（#11）：--spectate 真机渲染回放 + CommandEnvelope 补 LT2 按键信封 |

## Phase 1：呈现层三线并行

1. **FXP**：伤害飘字（DamageDealt→浮动数字，方案 A 皮肤）、Buff/状态图标（眩晕/沉默/光环/装备增益 HUD 标记）、击杀占位特效
2. **MMP**：小地图 ping 渲染（涟漪标记）+ 点击命令落点 + tooltip CJK
3. **STAB**：三图真机冒烟回归 + warden 移植引擎内截图（摆脱锁屏黑屏，自动产出换肤前后对照图）

## Phase 2：军团战争图（games/legion/，KPI 内置）

| 增量 | 内容 | 数值锚点（起点值） |
|---|---|---|
| LG0 | 立项+四人各一路 bundle+首个可跑切片（**计时对比 RL0-1**） | 每路 12 格布阵区 |
| LG1 | 波间建造期：雇佣兵商店买兵布阵 + 自动交战 | 雇佣兵 6 种、买兵 30-120 金 |
| LG2 | 发兵（花钱给对手注入下一波）+ 收入结算 + LastTeamStanding | 发兵花费 25+10% 递增；回合收入 50+击杀赏金 |
| LG3 | 方案 A UI 直接套用（**换肤复用成本记录**）+ H1 清单 | — |

护栏：engine 禁改（洞走 C 通道）；每增量记录行数/耗时/新洞。

## Phase 3：收口

全量回归 + 三图 KPI 复盘（rts-lab vs legion 复用效率曲线）+ H1 材料包（四图）。

## 账目

≈18-22 增量，四批；额度每窗口 2-3 agent；人工关卡：H1（四图一次过）。
