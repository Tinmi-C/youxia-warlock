# loop-td 路线图

> 流程：规格-测试-小步提交（ADR-0014）。需求底稿：`docs/topics/game-design/loop-td-game-proposal.md`（v0.2）；测试用例全集：`docs/topics/engine/warbase-test-strategy.md`；基底增量在 `engine/warbase/docs/roadmap.md`。
> 状态图例：✅ 完成（测试钉死）｜🟡 部分（核心在、余项列注）｜⬜ 未开始｜⏸ 待人工关卡（H1 试玩/H3 审美，当前后置）

| 增量 | 内容 | 状态 |
|------|------|------|
| LT0 | 立项接线（模板改造 + warbase 依赖 + 回归绿） | ✅ 0a7762a |
| MA1 | 环形地图 + 建造网格（maps/loop.ron + build_grid_from 路径占格） | ✅ |
| EN1 | 怪物定义表（bundle 数据：grunt/fast/tank/flyer/boss） | ✅ |
| EN2 | 绕圈行为验收（漏怪留场 + 圈数 + 存量计数） | ✅ G-LP-02/03 |
| EN3 | 特性行为四件套（快速/重甲/治疗/光环） | ✅ 快速/重甲/治疗(mender 8/1.5s r3 不逾上限)/敌方光环(shaman -25% taken r3.5，杀即消)——四件全齐，已编入 13/19/23/27 波 |
| TO1 | 基础塔 ×4（arrow/cannon/mage/sentry，攻甲克制体验） | ✅ G-LP-11 |
| TO2 | 自由放置与回收（键盘光标 + try_place/try_sell） | ✅ |
| TO3 | 三级升级（基底 try_upgrade + 全 4 塔 ×3 阶链 + U 键） | ✅ 实装为逐阶链 arrow/arrow2/arrow3…（原「三级」按链式兑现） |
| TO4 | 功能塔 ×3（ice 减速/drum 增伤/corrode 破甲光环） | ✅ 光环层全链路（含出售即消） |
| HR1 | 英雄塔获取三来源 | ✅ 建造(150g) + 里程碑(每10波免费) + BOSS掉落(50%种子化) |
| HR2 | 英雄塔技能加点 UI 与流程 | ✅ spend_skill_point（点数/等级/满级可读拒绝）+ G 键光标位加点 + hunter 挂 snipe 自动施放真技能（60×等级，经 SkillCast→游戏效果→伤害管线） |
| HR3 | 天赋三选一 | ✅ 5/10/15/20 级里程碑 PendingTalent + 1/2/3 键三选（力+12%伤 / 敏-12%间隔 / 均+6%&-6%），永久 StatMods 独立源可叠 |
| WA1 | 30 波波次表（maps/loop.ron，含 1.18^n 强度曲线） | ✅ |
| WA2 | 特性波编排 | 🟡 快速/重甲/空中波已编入 30 波；预告文案已带 |
| WA3 | BOSS 限时波（10/20/30 波） | ✅ G-LP-05 |
| EN1b | 无尽模式入口 | ✅ 基底 Endless 循环（清表回卷 ×1.15^圈 强化）+ 终局 E 键无尽重开（R 普通重开） |
| ST1 | 计分 + 结算屏（存量曲线） | ✅ RunStats（击杀/存量峰值/0.5s 曲线采样）+ settlement_score（击杀×10+波次×100+存量控制×2+英雄等级×50）+ 结算屏覆盖层（败因/曲线字符图/Meta 提示，缓存键防重排，重开即清） |
| UI1 | 命令卡定制 + 存量 HUD | 🟡 极简 HUD✅ + 基底命令卡已接（自动建造菜单 8 槽/数字键直接建造/B 切模式）；卡面视觉定版待 H3 |
| MT1 | Meta 局间成长 | ✅ meta_save.txt 存读（load 纯函数不删档）+ 清波数入账（RunBanked 幂等）+ 终局 M 买开局+20金（10 点/级）+ R 重开局（重装配吃加成，棋盘清空） |
| BAL1 | 锚点 A1–A3 收敛 | ✅ 读数：A1 存量峰值 6/10（贪心箭塔基线过 5 波不死）；A2 注入 20 翻盘（峰值 24、终局 Won）；A3 BOSS 38.2s ∈ [36,54]（标准 3×arrow3 沿环分布，boss hp×1.25） |
| SMK1 | 迷宫冒烟（maps/maze-smoke.ron + `--map`） | ✅ headless 全链路（Lives/Manual/开链对照图）；真机 `--map` 演示 ⏸ H1 |
| LT-ADOPT | 采用改造（v2 前置）：基底鼠标链接入（塔挂 Selectable→点选/框选/右键智能命令，键盘光标并存）+ 数字键模态让渡（建造菜单开=claim/关=release，模态期基底桥不产 GroupCommand）+ 英雄塔 Inventory（6 槽空，LT2 装备铺路） | ✅ tests/adopt_domain.rs（7 测试：模态让渡/召回恢复/点选+面板/Aabb×3/右键 UnitCommand/英雄 Inventory/怪不可选） |
| LT2-1 | v2 装备上塔：英雄塔 3 装备槽（EquipSlots 独立组件），词缀经 StatMods 注入、source=每槽独立 marker 实体（规避 INV 同 source 覆盖缺陷），同槽替换撤旧回包（E 键从包内上装） | ✅ tests/v2_depth.rs（攻+12%→1.12/撤回 1.0/同槽换装 1.08 且旧件回包/间隔-10%/射程+1.5 直改 AttackKit.range/守卫断言）——射程词缀为已记洞：基底 StatKind 无射程项，走游戏侧 RANGE_AFFIXES 表 |
| LT2-2 | v2 神器任务链：清波 10 + BOSS 击杀 2 → 领取（全队唯一，Q 键），领取后场上全塔获永久光环 entry（source=神器实体，每帧幂等 upsert，新塔次帧入光环） | ✅ tests/v2_depth.rs（达成前双缺口拒领/领取后全塔 +15% damage_mult/领取后新塔入光环/二次领取唯一性拒绝）——光环 +15% 为游戏侧起点值（地图无神器节，缝扩展后进 RON） |
| LT2-3 | v2 挑战波：每 10 波里程碑波间可选（C 键，一次一里程碑），下一波 hp_scale×2.0 + reward_gold×3.0（表快照/清波还原，基底 director 原路出怪发奖），清波必掉 1 件装备 GroundItem（MatchRng 种子挑选） | ✅ tests/v2_depth.rs（非里程碑/出怪中拒开/表 hp×2 断言/挑战怪 MaxHp=40×hp×2 且带 ChallengeSpawn 标/奖励恰 3×30 金/表还原/恰 1 GroundItem/同里程碑二次拒开）——WaveTag=usize 无变体位为已记洞：Challenge 标记走游戏侧 ChallengeSpawn 组件 |
| LT2-4 | v2 挖宝：波间 50 金一挖（V 键；G 已被 HR2 加点占用），MatchRng 单掷 50% 空/35% 金 80/15% 随机装备，装备入英雄包（按实体序首个有空位英雄）否则落地 GroundItem | ✅ tests/v2_depth.rs（同 seed 20 掷序列全等/金结果恰 80 且扣费精确/500 掷 EV∈31.5±5/装备入包/无英雄落地为 GroundItem/守卫断言） |
| LT2-5 | v2 羁绊：游戏侧 maps/bonds.ron 组合表（塔型计数≥阈值→StatKind+amount），每帧检测全场→全塔 StatMods 联动注入/撤除（source=每羁绊独立 marker 实体） | ✅ tests/v2_depth.rs（2 箭+1 冰→全塔含冰塔 damage_mult 1.10/卖 1 箭→回落 1.0 且无残留 entry）——计数按 def 精确匹配（升级后算新 def，bonds.ron 注释已记） |

## 待人工关卡（后置清单）

- H1 试玩：`cargo run -p loop-td` 灰盒手感（键盘 WASD 光标 / 1-4 选塔 / 空格放置 / X 出售 / P 暂停）；`LOOP_TD_MAP=maps/maze-smoke.ron cargo run -p loop-td` 换图演示。
- H3 审美：灰盒视觉（正交俯视相机/路径环/单位方块球体/HUD）未过视觉验收，后续 UI 增量统一处理。
- 视觉基线（VIS 系）随 UI 增量建。
