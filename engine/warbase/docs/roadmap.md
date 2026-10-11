# warbase 路线图

> 流程：规格-测试-小步提交（ADR-0014）。规格底稿：`docs/topics/engine/warbase-implementation-design.md`（实现）+ `warbase-test-strategy.md`（用例）+ ADR-0012/0013。游戏侧进度在 `games/loop-td/docs/roadmap.md`。

## 已完成（as-built 2026-10-08）

| 增量 | 内容 | 提交 |
|------|------|------|
| WS0 | 根 cargo workspace（成员/exclude/统一钉版/profile 上收） | f73848e |
| TS-WS | workspace 一击回归基准（warden 153 全绿快照） | — |
| BX0 | crate 落位：WarbasePlugin + WarbaseSet 六段链 + ui/fx feature 门控 | 8b5ac08 |
| TS0 | 确定性基建：MatchRng / test_app / 逐帧 drain / 临时文件隔离 | afe40e0 |
| BX1 | unit 域模型：统一 UnitDef + 攻×甲表 + 组件组合 spawn（含 auras/hero/skills/heal/upgrade_to/hp_scale/cost 字段） | afe40e0→01d84fe |
| BX2 | 伤害管线：DamageRequest→resolve_damage 单写 Hp + DamageOrigin 归因（Attack/Skill/System） + 索敌 First/Last 按圈序 + 冷却开火 + 追踪弹 + 攻甲矩阵 + DamageTaken 光环 | afe40e0→ed6fb4e |
| BX5 | 路径：闭环（圈数计数+LapCompleted）+ 开链（Arrived once）+ 程序化 Transform | afe40e0 |
| BX6 | 波次导演：Auto/Manual + 预告 + 提前召唤（含奖励）+ BOSS 限时 + 清波奖励 + **Endless 循环**（清表回卷 ×1.15^圈） | 77a660a→a34ad52 |
| BX7 | 资源：金/木/人口 + 兑换曲线 + 赏金入账 + 买人口（单命令流 EconCommand） | 77a660a |
| BX8 | 规则插拔：Lives/Stockpile/漏下家(预留) + 建造窗口(Always/PrepOnly) + 绕圈增益 + 胜负单次流转 + 败因可读 | bc0baf6 |
| build | 建造域：FreeGrid/FixedSlots（含 origin 居中对齐）+ try_place/try_sell/try_upgrade（清单可读拒绝）+ Buildable 组件 + BuildGrid Clone | 7ded62e→8db7779 |
| BX3-lite | 技能/光环/英雄成长：SkillCaster + 三分法 + 冷却/施放校验/自动施放接缝 + StatMods→EffectiveStats 四通道聚合 + 塔/随行双光环 + XpHolder 吃击杀经验升级 + spend_skill_point + Healer 周期群疗 | 96ab042→701f6e1 |
| BX9a | MapBundle RON 装载/校验（拒载不迁移）+ assemble + bundle::build_grid（路径包围盒居中） | a89b97b |
| BX10 | headless harness：DriverStrategy + Greedy 基线 + HarnessOpts（种子/帧上限/注入）+ HarnessReport（胜负/波次/存量峰值/建塔数/按定义伤害归因）+ 可复现 | 61808e8 |
| BX4a | 命令卡 headless 核心：CommandCardDef/SlotDef + build_card_from_registry + 热键解析 + CommandCardAction 分发 | 1ef7410 |
| BX4b | 命令卡面板：底栏槽位数据驱动生成 + 缓存键重建 + 升级阶排除出建造卡 | a129ebe |
| TO3 | 塔升级链：UnitDef.upgrade_to + try_upgrade（付目标阶造价、原格换体、人口 1:1、顶阶可读拒绝） | 7c8045c |
| BAL1 | 三锚点收敛：A1 新手存量峰值 6/10 + A2 注入 20 翻盘 + A3 BOSS 38.2s∈[36,54] | 2348830 |
| WX1 | warden 接基底依赖 + DamageSource.to_origin() 归因桥接（160 断言零漂移） | f93f0d7 |
| WX2-prep | 基底 SkillDef 增 badge/hotkey + SkillDefs len()/is_empty() | 030a0d6 |
| WX2 | warden 技能框架切基底：SkillCaster/SkillSlot/SkillDefs/SkillDef/CastSkill/SkillCast 全改用基底类型（历史路径重导出，调用面零改名）；id 模型索引→字符串（SKILL_ORDER = 平衡表行序 + DamageOrigin 桥）；SkillBalance 增 ids 字段改 id 键；基底 SkillCast 增 point（游戏侧瞄准层写入）+ tick_skill_cooldowns 钳 0（warden 复用）；移除 HashMap 序不稳的 by_index。160 断言零漂移 | c4a6acc |
| WX3a | 基底命令卡渲染层补全：槽位变真按钮（点击→CommandCardAction，与热键同一派发缝）+ enabled 置灰（点击/热键双门控）+ WC3 2×4 网格（GRID_KEYS/grid_badge/assign_grid_hotkeys，warden §16.7 上收）+ 徽章角标渲染 + 悬停 Tooltip + PanelVersion 版本重建；顺修 BX4b 重建旧槽位泄漏（clear_children 先解除关系致 despawn 列表恒空） | e1a401c |
| WX4 | 基底 fx 空壳变实模块：HealthBar（Hp/MaxHp 数据源 + HealthBarStyle 游戏侧样式）/HitFlash/FxBlast 消息/BlastRing/DeathShell + 五系统（FxPlugin 注册，headless 下 init Assets 兼容）；warden 切换消费：HitFlash/BlastRing/DeathShell/spawn_death_shell 走基底（历史路径重导出），ProjectileImpact→FxBlast 转发桥，血条留 warden（Enemy 神组件数据源，WX6 拆解后一行切换），FX3 状态色/徽章留游戏侧（设计决定：游戏身份视觉不进基底）。warden 160 断言零漂移 | d8b3220 |
| WX5a | 基底 harness 词表与读数补全：DriverAction += Upgrade/CallWave/Cast/SpendPoint（设计文档 §3.11 词表），执行统一收口 apply_driver_action（run_headless 与直测共用）；HarnessReport += damage_by_origin（attack/skill/system 三列，warden BAL 三列读数的基底对位）+ wave_times/leaks_per_wave（按波采样）+ skills_cast/skill_levels（施放与加点读数） | 62c84ed |
| WX6-1 | warden 敌人行走切基底路径（垂直切片 1/6）：Enemy 删 next_wp，实体挂基底 PathFollower/MoveSpeed，move_enemy 删除改调度 warbase::path::advance_followers；warden 三薄系统桥接——sync_move_speed（Slow 因子折进 MoveSpeed）/face_movement（ART11 朝向走路径切线）/leak_arrived（Arrived 消息消费漏怪）；MapPlugin 派生 PathDef 实体 + Paths 资源（PathInfo 仍为 warden 视图）；spawn 链路传 path 实体，waypoint_progress/nearest_path_progress 桥接旧 next_wp 语义；RangeDummy 摘 PathFollower 站桩。20s 锚点、漏怪、减速全零漂移 | 本笔 |
| WX6-2 | warden Enemy 神组件拆 Hp/MaxHp（基底组件，spawn/伤害单写者/治疗光环/索敌/投射物/技能过滤/debug 面板/测试构造全迁移）；血条收口：删 warden 本地 refresh_health_bars 改调度基底系统，精英/BOSS 经 HealthBarStyle 注样式（宽 1.4 金条）。160 断言零漂移 | 本笔 |
| WX6-4a | 伤害数值走基底攻甲矩阵：effective_damage 改查 AttackArmorTable（warden 语义 = 唯一 0.5 单元 Normal×Fortified，Physical/Magic/Mixed 映射 Normal/Magic/Piercing 三行）；敌人 spawn 挂基底 Armor+UnitInstance；EnemyDefs→UnitRegistry 代码桥（WX6-6 转 RON 前置）。160+1 断言零漂移 | 本笔 |
| WX6-4b | 伤害消息切基底：DamageRequest/DamageDealt/DamageOrigin 替换 warden 三类型（DamageSource/to_origin 桥退役，Projectile.source 直接携带 DamageOrigin，meteor 发射带 commander caster）；基底 resolve_damage 接管（warden 删除本地解析器，HitFlash 改挂 DamageDealt 的 flash_hits 反应系统）；基底 resolve_damage 的 Armor/UnitInstance 查询放宽为 Option（裸测试实体全额伤害+零赏金，伤害不再因缺组件消失）；resolve_death 保持 hp<=0 扫尸（覆盖 debug 手搓尸体，UnitDied 留基底侧不消费）。161 断言零漂移 | 本笔 |

## 待做（warden 回迁，建议每对单独会话）

| 增量 | 内容 | 障碍 |
|------|------|------|
| WX3b | warden 命令卡切基底（卡内容从基底 CommandCards 生成） | **依赖发现（WX3a 期间确认）**：warden 卡内容绑定塔索引 Build(usize)/双语 l10n/8 个 warden 动作/三区布局，需先 WX6 数据化（塔与技能走基底注册表）后才能像 loop-td 一样由 build_card_from_registry 生成；渲染层（WX3a）+ WX3-prep（SKILL_CARD/PanelState）已备齐 |
| WX5b | warden BAL 切基底 harness | **依赖发现（调研确认）**：run_headless 装配基底世界（MapBundle/BuildGrid/Resources），warden 世界全异——完整切换随 WX6（warden.ron）落地，与设计文档 P1.5→P2 顺序一致；基底侧词表/读数缺口已由 WX5a 补齐，届时 warden 锚点策略改写为 DriverStrategy 即收口 |
| WX6-2..6 | warden 参数化续切片（建议顺序）：2 敌人域 Hp/MaxHp/Armor 拆神组件（解锁 WX4 血条一行切换）；3 波次域 WaveDirector(Manual)；4 伤害管线切基底 resolve_damage/投射物；5 经济/规则/建造（Resources/StockCount/FixedSlots，基底 UnitDef 需补 per-unit leak 字段）；6 defs 全量迁 maps/warden.ron + assemble。完成后 WX3b/WX5b 随之收口 | 累进拆神组件，每片 160 断言零漂移 |

## 消费方

| 游戏 | 地图 | 状态 |
|------|------|------|
| loop-td | maps/loop.ron（30 波循环圈）+ maps/maze.ron（20 波迷宫）+ maps/maze-smoke.ron（5 波冒烟） | ✅ v1 完整可玩 |
| warden | （待 WX6 后 maps/warden.ron） | 🟡 WX1/WX2/WX4 已切；WX6-1 敌人行走走基底路径 |
