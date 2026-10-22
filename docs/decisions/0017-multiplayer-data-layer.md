---
title: ADR-0017
type: adr
status: accepted
date: 2026-10-22
author: AI 起草（ZCode）+ youxia 拍板（2026-10-22 接受，依据 warbase 魔兽式底座补全方案评审）
---

# ADR-0017: 多玩家数据层——玩家/队伍/外交矩阵（不含 netcode）

## 背景

warbase 现状是单玩家 TD：敌我判定 = `Allegiance` 二值比较（Defender/Attacker），散布在 combat（索敌/开火/群疗）与 buff（双光环）共 5 处。WC3 模板要求多玩家（1v1~4v4+12 玩家、队伍、共享视野、漏怪传下家）；rule 域已有「v2 multiplayer seam」注释但无架构决策。敌我判定是索敌/伤害/光环/胜负的公共缝，**越晚收口返工面越大**。

## 决策

1. **类型**：`PlayerId(u8)` / `TeamId(u8)` 新类型 + `MatchDiplomacy` 资源（玩家→队伍映射 + 敌对关系矩阵）；`Owner(PlayerId)` 组件挂塔/单位/建筑。默认配置 = 单玩家（P0 队 0 vs 环境队）≡ 现状语义。
2. **Hostility 资源是唯一敌我判定缝**（SEAM0c 落地）：combat/buff 全部 5 处判定改走 `Hostility::hostile/allied`，默认实现 = 原 Allegiance 二值语义（零漂移）；MP1 换实现为外交矩阵查询。此后**任何新代码禁止直接比较 Allegiance**。
3. **Allegiance 保留为派生视图**：`sync_allegiance`（Cleanup 段）从 Owner+外交派生刷新；warden/loop-td 的既有读点零改动（兼容铁律）。
4. **共享视野**：同 `TeamId` 的视野源合集（FOG2 消费）。
5. **经济分域暂缓**：`Resources/Population` 维持单例（语义=本地玩家），MP2 仅出评估卡；分域等第二个多玩家消费方出现再开增量（YAGNI，与 ADR-0002 游戏驱动一致）。
6. **明确不做**：netcode、锁帧联机、回合制仲裁——数据层与传输层解耦，联机另立 ADR。

## 备选方案

1. **Allegiance 直接退役、全量改 Owner**：否——warden 大量读点，违反零漂移铁律；派生视图两全。
2. **立即按玩家分域经济/胜负**：否——单 TD 消费方下是纯成本；先缝最贵的索敌/伤害缝，经济缝留接口。
3. **阵营=玩家（无队伍层）**：否——WC3 的队伍（同盟共享视野/胜负）是模板内语义，TeamId 一层不可省。

## 影响 / 后果

- 优：敌我语义单点收口，MP1 之后多玩家只是「换实现」；伤害归因/胜负/共享视野有了统一的队伍语义。
- 代价：combat/buff 系统签名 +Hostility 参数（SEAM0c 一次性完成）；回放对多玩家的输入记录需带 PlayerId（RPL envelope 预留）。
- 关联：[[decisions/0016-warbase-wc3-alignment-expansion|ADR-0016]]（扩域总纲）；[[topics/engine/warbase-wc3-alignment-plan|补全方案 §7 MP 域]]。
