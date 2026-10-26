---
title: warbase 第二轮扩域方案 v2——RPG 纵深全量补齐 + UI 设计系统 + 循环圈纵深 v2（子智能体并行开发底稿）
type: reference
topic: engine
date: 2026-10-24
author: AI 起草（ZCode），youxia 拍板（2026-10-24）
status: accepted
tags: [architecture, runtime, warbase, bevy, wc3, rpg, ui-design, plan, parallel-development]
related: ["[[decisions/0018-warbase-round2-rpg-depth-and-ui-system|ADR-0018]]", "[[topics/engine/warbase-wc3-alignment-plan|第一轮方案 v1]]", "[[topics/game-design/loop-td-v2-rpg-proposal|loop-td v2 需求草案]]", "[[decisions/0016-warbase-wc3-alignment-expansion|ADR-0016]]"]
---

# warbase 第二轮扩域方案 v2

> 第一轮（v1）交付九域 + rts-lab 验收 + 12 洞清单。本轮目标：**全量 A 层补齐 WC3 模板 + UI 推倒重设计 + loop-td 循环圈纵深 v2（RPG 半区验收）**。
> 用户拍板：框架质量优先不计成本；UI 设计=AI 先行 + 看图拍板；旧 UI 四不满点（布局结构/视觉风格/游戏感/信息可读性，对象=warden 命令卡与 loop-td 灰盒）全部转为设计系统负面约束。
> 执行制度完全沿用 v1 §11（worktree 并行+独立 target / 域插件+lib 冻结 / 零漂移 / 失败两次复盘 / 任务包模板）。基线：workspace 398 绿。

## 1. loop-td 架构结论（前置评估）

loop-td 是基底第一公民（ADR-0012 样板：maps/*.ron + 薄插件），**无架构性错位**。差距是采用债：第一轮九域中它只消费了 replay；键盘光标是品类决定非锁死。v2 流程=先采用改造（LT-ADOPT）再纵深（LT2），改造属低风险增量，非重构。

## 2. Phase 0：SEAM2 修订（主线单会话）

| 增量 | 内容 |
|---|---|
| SEAM2a | 本方案 + ADR-0018 + loop-td v2 需求草案入库（本文档即为其一） |
| SEAM2b | 洞 #6-12 清账上收：`SkillCast.target: Option<Entity>`（#6）；光环目标查询泛化（自由单位上收，#7）；`ModalKeyClaim` 资源=模态期键盘桥让渡数字键（#8）；UnitDef 技能槽初始等级字段（#9）；CommandEnvelope 词汇表 Entity-free 扩展（#10：命令按「选择集语义+目标点/序号」编码，禁 Entity）；#11 观战列 P2、#12 已闭 |
| SEAM2c | 事件资源翻消息（ItemCommands/ShopCommands/ResearchCommands/RallyCommands → Messages）+ selection 两 TODO 收口（右键 Pickup/Follow 实义化、双击选同类、子组命令语义=命令只发当前子组、M 键显式移动） |
| SEAM2d | 八新域骨架（harvest/build/hero/daynight/camera/camp/save/spectate）+ Order 扩展（Harvest/ReturnCargo/Build{building,point}）+ MapBundle 新段（camps/daynight/terrain 保留字段）+ 域插件一次接线；lib.rs/bundle 此后冻结如旧 |

## 3. Phase 1：七线并行（基底域 + UI 设计系统）

| 线 | 增量 | 关键验收断言（节选，全部数字化落测试） |
|---|---|---|
| HARV | Harvest 循环上收（矿储量/枯竭/负载/交付）+ Upkeep（人口 ≤50 全额 / 51-80 收 70% / >80 收 40%，WC3 经典三档） | 单程 10 金满载交付；矿 250 存量采 25 趟后枯竭拒采；80 人口下交付恰 4 金 |
| BLDG | RTS 造建筑（选位/进度/多工人加速线性叠/取消全额退/农民驻守差异=UnitDef 参数）+ 主城 T1→T3 等级链（每级解锁集接 Unlocks） | 双农民 6s 建筑 3s 完工；取消退 100%；T2 后新建筑出现在 Unlocks |
| SKVOC | Area(AOE) 目标类型（半径圈选敌）+ 眩晕/沉默原语（Stunned/Silenced 组件+InterruptCast 联动）+ 召唤原语（SummonDef→spawn+期限）+ 攻击前后摇（damage point 帧窗内打断不发伤害；backswing 可取消走位） | blizzard 型 AOE 恰命中圈内 N 敌；眩晕 2s 内 0 次 DamageRequest；前摇 0.3s 第 17 帧被打断无伤害 |
| HERO | 三围（STR+HP/AGI+攻速护甲/INT+蓝回蓝，换算表进 bundle）+ 祭坛复活（计时+花费+重生位）+ 经验范围分享（半径内英雄均分） | STR10→HP+50 断言；死亡→祭坛 15s→复活满蓝半血；双英雄分 XP 各半 |
| CAM | 边缘滚屏（像素/秒参数）/编队双击跳转/F1 英雄/滚轮缩放/小地图 ping 消息（MinimapPing{pos,team}） | 滚屏速度断言；F1 选中唯一英雄；ping 消息 round-trip |
| MAP | 野怪营地（守区半径/追击/脱战回位/重生计时/掉落等级表 camps 段）+ 昼夜循环（DayClock 资源/昼夜长参数/视野半径系数） | 怪离守区 6.0 脱战回位；死亡 60s 重生；夜晚视野半径 ×0.6 断言 |
| UI-DSGN | 设计 brief（题材推导+四负面约束）→ 基底 ui 域主题 tokens（色彩/字阶/间距/圆角/层级数据化，`UiTheme` 资源游戏可换肤，结构 API 不变）→ loop-td 画面上 2-3 套方案 mock 截图（主 HUD/命令卡/物品/技能/英雄/羁绊全套）→ **H-UI 看图拍板** → 《UI 设计系统规格》入库 | tokens 换肤零代码改动断言（改 RON/资源即换色系）；每套方案截图包 ≥8 张 |

## 4. Phase 2：三线并行

1. **高地+存档**：VisionGrid 扩展（高格挡低格视线/高地攻击加成 25% 起点值）；对局存档 save/load（世界快照 RON，区别于录像=输入流）。
2. **LT-ADOPT**：loop-td 接鼠标选择/智能命令/模态让渡/Inventory 挂英雄塔；先行架构评估结论卡。
3. **RL4**：rts-lab 游戏侧采集/建造/光环补丁全部换新基底域（C→A 第二遍）。

## 5. Phase 3：三线并行

1. **LT2 纵深**（先测后实现，数值起点见需求草案）：装备上塔（词缀 StatMods 注入塔实体）→ 神器 → 挑战波 → 挖宝 → 羁绊（塔组合检测→联动 StatMods）→ 多人漏怪（LeakPolicy 预留缝+本地双人）。
2. **LT2-UI**：全部面板按拍板的设计系统实现，**每面板截图自审闭环**（实现→截图→对照规格→不过重做，截图留档）。
3. **WARDEN-SKIN**：warden 面板 token 级换肤（结构不动，验证换肤成本承诺）。

## 6. Phase 4：收口

全量回归 + vault 收口 + **H1 合并人工关卡**（三图一次过）；降级方案=自动截图取证包（warden WARDEN_SHOTS 模式推广到 loop-td v2/rts-lab），看图完成 80% 验收。

## 7. 人工关卡清单（本轮两处）

- **H-UI**（Phase 1 末）：看 2-3 套 UI 方案截图选一套，不玩只看图。
- **H1**（Phase 4）：真人试玩三图；届时不便则以截图包先行、后补真人。

## 8. 增量账目

SEAM2（4）+ Phase1（7 线约 14 增量）+ Phase2（3 线约 8）+ Phase3（3 线约 10）+ 收口 ≈ **34-38 增量**，四批并行。
