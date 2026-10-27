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
- 开局（arena.ron `starting_forces` 段，洞 #2 已闭、RL4 切换）：主城
  (-5,0)、金矿 (5,0)、农民 ×4 主城旁、grunt ×3 (10,-6) 一带；金 150 /
  人口 0/12。

## 范围（RL2 增量：交战+技能+生产纵深）

- **A/P 定点令**：A 键走基底 `CommandHotkey` → `PendingTargetOrder` 状态机，
  左键点地由游戏侧 `pending_click_transition`（XZ 平面正确版）发
  `UnitCommand{AttackMove}`；P 同理两点巡逻（`systems/input_bridge.rs`）。
- **兵营+步弓**：`barracks`（trains=[footman, archer]）开局 (-5,4.5)；
  `footman` 近战 kit / `archer` 投射物 kit（TargetDomain::Ground +
  projectile speed 16）。入口：选中兵营 T/F 键（选主城时 T 仍是农民）。
- **英雄+法力技能**：`paladin` 开局 (-4.5,-2.8)：hero 段（XpHolder）+
  skills（holy_light 主动 Unit 目标 40 蓝/6s；devotion 被动）+ mana 100/1.0；
  两技能开局预学 1 级（游戏侧写死——基底 spawn 只给 Autocast 槽 1 级）。
  Q 键释放；瞄准=当前选择集内受伤最重盟友，无则自体（写死策略）。
  效果：`SkillCast` → 游戏侧 `skill_effects` → `HealRequest`（基底
  resolve_heals）。devotion 光环走 def.auras（`affects: Allies`，基底
  apply_follower_auras 消费——洞 #7 已闭，RL4）。
- **科研**：`production.research` = attack_upgrade（100g/8s，Stat grant：
  footman/archer DamageMult +0.15）；R 键发 `ResearchCommand::Start`；
  完成后 `TypeModifiers` → `apply_research_stats` → EffectiveStats 断言。
- **物品**：`items` 段 = healing_potion（Consumable，effect=potion_heal）+
  blade_of_valor（Passive 剑，DamageTakenMult -0.20）；`items.drops`：
  grunt 必掉药水（chance 1.0）；地图中央 `shop`（stock 游戏侧，洞 #2b）。
  农民/英雄自动拾取（Inventory 游戏侧挂）；选中英雄 1 键用药、B 键
  （商店 3 格内）买药。

## 范围（RL3 增量：迷雾+小地图+录像+音效通道）

- **视野迷雾**：`maps/arena.ron` vision 段（cell 1.0）；VisionSource 挂接走
  `attach_rts_kit`（半径写死：hall 8.0 / paladin 7.0 / footman·archer 5.5）。
  基底 FOG1-2 全链：VisionGrid（Tick 重算）→ InFog 标记（Cleanup）。
  视觉：敌方 InFog 变暗半透明（`sync_fog_tint`，bevy 原生颜色变化，无贴图）；
  己方单位永亮（友军不参与变暗——基底标记不分队，过滤在游戏侧）。
- **小地图**：右下角 MinimapPanel（bevy 原生纯色 UI，176px），消费基底
  `MinimapModel`（FOG3 数据层）画 blip（Defender 蓝 / Attacker 红 / 英雄
  金色加倍）；点击面板=镜头跳转（`minimap_click_jumps_camera` 移动正交
  相机 center，不做命令落点）。
- **录像**：`--record` 自动存 `runs/<时间戳>.jsonl`（loop-td replay_io 模式）；
  `--replay <file>` 无头校验（对账 `replay_lab`：lab 组合 = test_app +
  GamePlugin，帧 0 断言经一次性探针系统在记录者位置取指纹——状态闭包
  持有，不进 World，避免资源实体位错位）。
- **SFX**：`sfx_probe` 最小监听（log 行）证明 SfxRequest 通道可接，无资产。

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

RL2 追加（tests/behavior.rs ⑦–⑪）：

7. A 令：选中+CommandHotkey::AttackMove → PendingTargetOrder::AttackMove；
   点地过渡（纯函数）→ AttackMove{to:(x,0,z)}；敌放路径上 → DamageDealt
   且敌 hp 下降（MOV4 acquire_engagement 交战缝）。
8. 兵营训练：TrainCommand footman(60g/6s) + archer(70g/7s) → 出兵带 kit
   （footman 无投射物 / archer projectile 16、均 Ground）。
9. holy_light：瞄准=选择集内受伤最重盟友（无则自体）；蓝 100 耗 40 →
   SkillCast → HealRequest → 盟友 Hp+（capped）；蓝 20 拒绝（无事件无扣蓝）；
   冷却期内二次施放拒绝。
10. 科研：StartResearch 100g → 480 帧完成 → TypeModifiers[(footman,
    DamageMult)]=0.15 且新老 footman EffectiveStats.damage_mult 均为 1.15。
11. 物品：杀 grunt → GroundItem(healing_potion) → 英雄走近入包 slot0 →
    Use → potion_heal SkillCast → Hp 100→180；商店：近距 Buy 扣 30 金入包，
    远距被游戏侧门槛拒绝。

RL3 追加（tests/behavior.rs ⑫–⑭）：

12. 迷雾：远端 grunt InFog；走入主城视野圈 → 标记移除；迷雾中的 grunt
    照常交战（DamageDealt，D5：战斗不读 InFog）。
13. 小地图数据：MinimapModel.blips 与「可见单位集」逐一致（point_visible
    重算对账）；paladin blip 带 is_hero；暗处矿/敌无 blip。
14. 录像：录制 100 帧（种子 42）→ 哈希检查点 [0,30,60,90] → 落盘 →
    --replay 对账全过（含帧 0 探针）；篡改 frame-30 哈希 → 精确报错帧。

## 抽象有洞清单（RL1–RL3 发现，通道 C——全部游戏侧补丁，基底零改动）

> RL4（C→A 第二遍）清账：**#2 已闭**（bundle `starting_forces` 段，hole-3
> 上收；游戏侧 STARTING_LAYOUT 退役）；**#7 已闭**（SEAM2b 光环泛化
> `AuraDef::affects`，游戏侧 free_agent_auras 退役）；**采集循环整体换
> 基底 HARV 域**（arena.ron `harvest` 段 + `Order::Harvest`/`ReturnCargo`
> 流），#5 的游戏侧分槽补丁随之退役——基底在 GATHER_RANGE 圈内即装/卸，
> 不等精确到达，天然规避推挤僵持。

1. **MapBundle 强制非空 world.paths**（TD 出身）：RTS 图没有出怪轨道。
   补丁：周界封闭 path 兼作竞技场围墙（`maps/arena.ron`）。
2. **MapBundle 无开局兵力段**：RTS 开局布兵是地图内容（ADR-0013）却无
   处安放。~~补丁：游戏侧 `STARTING_LAYOUT` 常量（`systems/gameplay.rs`）~~
   → 已闭（hole-3 上收：bundle `starting_forces` 段；RL4 切换，常量删除）。
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
   MOV5 散点只对**组命令**生效，逐个独立命令不散点。~~补丁：游戏侧确定性
   分槽（按实体 index 的 8 向环）+ 就近换腿~~ → 已随 RL4 采集切换退役
   （基底 HARV 在 GATHER_RANGE 圈内即装/卸，不等精确到达，僵持点在触发
   圈内）。基底侧候选改进：单主体命令也过 formation 散点，或
   arrival 检查放宽为「与任一同目标单位互斥满足」。
6. **SkillCast 事件无单位目标字段**（RL2）：`SkillTargeting::Unit` 的技能
   语义存在，但 `SkillCast` 消息只带 `point`（`Casting` 组件预留了
   `target` 字段但即时施法路径从不填）。单位目标技能的效果系统无法从
   事件流知道目标。补丁：游戏侧旁路组件 `HolyLightAim(Entity)`（Q 键
   瞄准时挂、效果帧消费后删，`systems/gameplay.rs`）。
7. **光环消费方只认 PathFollower**（RL2）：`buff::apply_auras` 与
   `apply_follower_auras` 的目标查询都带 `With<PathFollower>`（TD 出身）。
   自由单位 RTS 无 PathFollower，paladin 的 devotion（def.auras）对己方
   步兵永不生效。~~补丁：游戏侧 `free_agent_auras`~~ → 已闭（SEAM2b
   光环泛化：查询改 `With<UnitInstance>` + `AuraDef::affects` 声明敌我；
   paladin 走 `affects: Allies`，RL4 切换后游戏侧系统删除）。
8. **（摩擦）数字键双职无法让渡**（RL2，软性）：基底键盘桥把数字键固定
   翻成 `GroupCommand::Recall`，无「模态期数字归模态」的让渡缝（方案
   §12 开放问题 2 的推荐项未落）。游戏侧 1 键用药与编队召回同帧双发：
   用药生效后选择集被空编队召回清空。补丁：接受双发并在 HUD/README 注明。
9. **英雄技能槽开局 0 级**（RL2，软性）：`spawn_unit` 只给 Autocast 槽
   1 级，Active/Passive 槽 0 级（设计上靠升级点 `spend_skill_point`）。
   验收切片要求开局可用 → 游戏侧布兵时写死全槽 1 级（`gameplay.rs`）。
   （#2b 商店布段缺位已并入 #2：开局内容无 bundle 段，含 Shop stock。）
10. **录像输入词汇表只覆盖 TD 输入**（RL3）：基底录制器只记录
    PickAttempt / StartWave / EconCommand 三类消息；rts-lab 的全部命令面
    （SelectCommand / UnitCommand / TrainCommand / CastSkill / Item·Shop·
    ResearchCommands）都不产信封 → 本图录像退化为「零输入确定性模拟」
    （采集经济仍参与哈希对账）。录真实玩家操作需基底扩充
    CommandEnvelope 词汇表（Entity-free 化的命令信封）。
11. **（边界记录）整机型日志不可无头校验**（RL3，同 loop-td 立场）：
    --record 在完整 app（渲染/HUD/小地图实体）上录的日志，其指纹含
    渲染实体，lab 组合对账必炸——属 RPL3 观战模式范畴；验收 ⑭ 用
    lab 组合录放对账证明通道成立。
12. **（软性）B0004：单位实体无可见性组件**（RL3 视觉修正，非洞）：
    基底 spawn 不带 Visibility 族；RL1 起网格子实体带 InheritedVisibility
    触发 bevy B0004 警告。游戏侧 attach_rts_kit 补挂 Visibility（不改基底）。

## 边界（不做）

- 美术/音效资产（D9 灰盒）；矿枯竭；建筑建造（建塔复用 build 域留待
  后续增量）；敌方 AI（Greedy harness 驱动，未排期）；网络（永远不在
  基底范围，ADR-0017）；录像观战模式（RPL3+，见洞 #11）。
