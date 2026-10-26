# GDD — rts-lab（基底验收图）

> 本文档记录「验收切片」的规格与数字化验收标准，不是玩法设计。
> 一句话：**基底域拼起来 = 一个能玩的 RTS 灰盒，且不改引擎一行。**

## 范围（RL1 切片）

- 地图 `maps/arena.ron`：24×24 FreeGrid（cell 1.0）+ 周界墙（借 path 环，
  见报告「抽象有洞」#1）；两队 P0(队0) vs P1(队1)，hostile_pairs [(0,1)]；
  规则 LastTeamStanding。
- 单位表：`main_hall`（建筑，production.trains=[peasant]）、`peasant`
  （MoveAgent 语义 move_speed 3.0 / body_radius 0.3，弱近战 kit——民兵
  语义，兼作 LastTeamStanding 的队 0 战力）、`gold_mine`（建筑）、
  `grunt`（敌方守卫 ×3，AttackKit 站桩）。
- 开局（游戏侧 STARTING_LAYOUT，见「抽象有洞」#2）：主城 (-5,0)、金矿
  (5,0)、农民 ×4 主城旁、grunt ×3 (10,-6) 一带；金 150 / 人口 0/12。

## 数字化验收标准（→ tests/behavior.rs）

1. 装配：arena.ron 载入后 teams/生产表/单位表资源正确；开局实体齐全。
2. 训练：金 100 + TrainCommand → 300 帧（5s）后 peasant +1、金 -50、人口 +1。
3. 采集：单农民置矿旁 → 挂 Carrying(10) → 回城金 +10 → 循环，两趟共 +20。
4. 选择：PickAttempt 左键点中 → SelectionState 含之；右键空地 →
   UnitCommand{Move} 且位移朝目标；Aabb 框 2 单位 → len==2。
5. 胜负：三 grunt 被直发 DamageRequest 打死 → MatchEnded Ok +
   MatchWinner=TeamId(0)。
6. 零基底改动：`git diff --stat aae8fd6..HEAD -- engine/` 为空。

## 边界（不做）

- 美术/音效资产（D9 灰盒）；矿枯竭；建筑建造（RL2 建塔）；技能/英雄
  （RL2）；物品/迷雾/小地图（RL3）；网络（永远不在基底范围，ADR-0017）。
