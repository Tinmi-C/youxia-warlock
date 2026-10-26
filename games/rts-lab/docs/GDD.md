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
4. 选择：SelectCommand::Set 点选 → SelectionState/Selected/面板数据；Move 令
   单位位移到点且到点后 Idle；右键智能命令纯函数（敌→AttackTarget /
   空地→Move(x,0,z) / 友军→无）；Aabb 框 2 单位 → len==2。
   （原计划的 PickAttempt 直驱路径因「抽象有洞 #4」改为直发命令——基底
   自身对 headless 的纪律就是直发 SelectCommand/UnitCommand。）
5. 胜负：三 grunt 被直发 DamageRequest 打死 → MatchEnded Ok +
   MatchWinner=TeamId(0)。
6. 零基底改动：`git diff --stat aae8fd6..HEAD -- engine/` 为空（git 证据
   在增量报告，不作运行时断言）。

## 抽象有洞清单（RL1 发现，通道 C——全部游戏侧补丁，基底零改动）

1. **MapBundle 强制非空 world.paths**（TD 出身）：RTS 图没有出怪轨道。
   补丁：周界封闭 path 兼作竞技场围墙（`maps/arena.ron`）。
2. **MapBundle 无开局兵力段**：RTS 开局布兵是地图内容（ADR-0013）却无
   处安放。补丁：游戏侧 `STARTING_LAYOUT` 常量（`systems/gameplay.rs`）。
3. **左键点选链缺最后一跳**：input 层产 `PickAttempt`、selection 域产
   `PickResolved`，但无人把左键 `PickResolved` 翻成 `SelectCommand`（拖框
   与右键都有消费方，唯单点没有）。补丁：并入 #4 的游戏侧桥。
4. **pick 链与移动域地面平面约定互相矛盾**（最重）：input/selection 域按
   z=0 平面（`OrthoTopDownPicker` 与 z=0 求交、`truncate()` 取 (x,y)、
   `extend(0.0)` 产 (x,y,0) 目标），movement/nav/build 域按 y=0 平面
   （`world_to_cell((tf.x, tf.z))`、航点 `Vec3(p.x, 0, p.y)`）。正下相机
   下基底 picker 对每像素返回 None；XZ 平面单位在选择命中里塌缩到 x 轴；
   右键 Move 目标离地。基底 SEL 域测试全在 z=0/Y 轴上布置单位故从未暴露。
   补丁：游戏侧 `systems/input_bridge.rs` —— `ActiveWorldPicker` 缝换成
   NullPicker 静音基底指针层，自建 XZ 几何指针桥直写
   SelectCommand/UnitCommand；A/P 定点令继承同洞（RL2 处理）。
5. **（软性观察）多点同目标 + MOV3 反抖动 = 推挤平衡**：多条独立单主体
   Move 命令指向同一精确点时（采集循环天然如此），软碰撞「穿透>50% 不推」
   规则让单位群在距目标 ~0.4 处达成僵持，永远到不了 arrival_radius。
   MOV5 散点只对**组命令**生效，逐个独立命令不散点。补丁：游戏侧确定性
   分槽（按实体 index 的 8 向环）+ 就近换腿（HARVEST_RADIUS 内即装/卸，
   不等精确到达）。基底侧候选改进：单主体命令也过 formation 散点，或
   arrival 检查放宽为「与任一同目标单位互斥满足」。

## 边界（不做）

- 美术/音效资产（D9 灰盒）；矿枯竭；建筑建造（RL2 建塔）；技能/英雄
  （RL2）；物品/迷雾/小地图（RL3）；网络（永远不在基底范围，ADR-0017）。
