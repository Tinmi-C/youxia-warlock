---
title: ADR-0019
type: adr
status: accepted
date: 2026-10-27
author: AI 起草（ZCode），youxia 批准方案 v3（默认拍板项：混合战略/军团图/美术冻结，批准前可改）
---

# ADR-0019: warbase 第三轮——成熟度收口 + 军团战争图（复用效率实证）

## 背景

两轮扩域后 workspace 530 绿，基底功能面完整（九域+第二轮 RPG 纵深），但积累 12+ 洞/债（LT2 六洞、WARDEN-SKIN 三缺口、共享字体缺失、观战回放 #11 未收）；「一基座多玩法」只实证过 TD/RTS/RPG 三极各一张图，**第三张图对基底复用效率的量化（第二张新图比第一张省多少）尚未度量**——这是「基底是主产品」（ADR-0012）的核心 KPI。

## 决策

1. **战略=收口+新图混合**：先清债（Phase 0/1），再开第三图（Phase 2）量化复用效率。
2. **第三图=军团战争式（Legion TD）**：波间造兵布阵自动交战 + 花钱给对手发兵 + 多人 LastTeamStanding——深化 MP/生产/经济的对抗编排，与前两图（TD 养成/RTS 操作）互补。
3. **美术继续冻结**：灰盒+方案 A token 呈现；美术解禁另立轮次。
4. **SEAM3 清债清单**（八项上收/修复 + 观战回放）：StatKind+Range、WaveTag 变体化、shipped 图 items 段、CommandAction 注册缝、LeakPolicy::PassToNeighbor 基底化、ItemStack 实体化（INV source 缺陷转正）、UiTheme display 字阶+PanelKey revision 热换肤、共享 CJK 字体资产。
5. **KPI 度量制度化**：legion 图每个增量记录（游戏侧代码行数/耗时/新洞数），结项时与 rts-lab 同切片对比，写入三图复用效率曲线报告。

## 备选方案

1. 纯收口（不开新图）：否——复用效率无新数据，收口价值无法度量。
2. 纯新图（不清债）：否——带债扩张会放大债（前两轮教训）。
3. 美术解禁：否——用户未解禁；呈现层用灰盒达「可演示」水准。

## 影响 / 后果

- 优：API 面干净后再扩张；复用效率首次量化；三图+legion 达可演示。
- 代价：约 18-22 增量；军团图多人本地对战的最小可玩边界（不做 netcode）。
- 关联：[[decisions/0018-warbase-round2-rpg-depth-and-ui-system|ADR-0018]]、[[topics/engine/warbase-round3-plan|方案 v3]]、[[topics/game-design/legion-td-proposal|军团图需求草案]]。
