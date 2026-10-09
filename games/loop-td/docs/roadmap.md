# loop-td 路线图

> 流程：规格-测试-小步提交（ADR-0014）。需求底稿：`docs/topics/game-design/loop-td-game-proposal.md`（v0.2）；测试用例全集：`docs/topics/engine/warbase-test-strategy.md`；基底增量在 `engine/warbase/docs/roadmap.md`。
> 状态图例：✅ 完成（测试钉死）｜🟡 部分（核心在、余项列注）｜⬜ 未开始｜⏸ 待人工关卡（H1 试玩/H3 审美，当前后置）

| 增量 | 内容 | 状态 |
|------|------|------|
| LT0 | 立项接线（模板改造 + warbase 依赖 + 回归绿） | ✅ 0a7762a |
| MA1 | 环形地图 + 建造网格（maps/loop.ron + build_grid_from 路径占格） | ✅ |
| EN1 | 怪物定义表（bundle 数据：grunt/fast/tank/flyer/boss） | ✅ |
| EN2 | 绕圈行为验收（漏怪留场 + 圈数 + 存量计数） | ✅ G-LP-02/03 |
| EN3 | 特性行为四件套（快速/重甲/治疗/光环） | 🟡 快速✅ 重甲✅ 光环✅(敌方光环怪待做——机制已就绪) 治疗⬜ |
| TO1 | 基础塔 ×4（arrow/cannon/mage/sentry，攻甲克制体验） | ✅ G-LP-11 |
| TO2 | 自由放置与回收（键盘光标 + try_place/try_sell） | ✅ |
| TO3 | 三级升级（基底 try_upgrade + 全 4 塔 ×3 阶链 + U 键） | ✅ 实装为逐阶链 arrow/arrow2/arrow3…（原「三级」按链式兑现） |
| TO4 | 功能塔 ×3（ice 减速/drum 增伤/corrode 破甲光环） | ✅ 光环层全链路（含出售即消） |
| HR1 | 英雄塔获取三来源 | 🟡 hunter 可建（150g）+吃经验升级+伤害成长✅；三来源(里程碑/商店/BOSS掉落)⬜ |
| HR2 | 英雄塔技能加点 UI 与流程 | ⬜ |
| HR3 | 天赋三选一 | ⬜ |
| WA1 | 30 波波次表（maps/loop.ron，含 1.18^n 强度曲线） | ✅ |
| WA2 | 特性波编排 | 🟡 快速/重甲/空中波已编入 30 波；预告文案已带 |
| WA3 | BOSS 限时波（10/20/30 波） | ✅ G-LP-05 |
| ST1 | 计分 + 结算屏（存量曲线） | 🟡 胜负+败因 HUD 已有；结算屏/计分/曲线 ⬜ |
| UI1 | 命令卡定制 + 存量 HUD | 🟡 极简 HUD✅（金/木/人口/存量/波次/结果）；命令卡 ⬜（待 BX4 上收） |
| MT1 | Meta 局间成长 | ⬜ |
| BAL1 | 锚点 A1–A3 收敛 | ✅ 读数：A1 存量峰值 6/10（贪心箭塔基线过 5 波不死）；A2 注入 20 翻盘（峰值 24、终局 Won）；A3 BOSS 38.2s ∈ [36,54]（标准 3×arrow3 沿环分布，boss hp×1.25） |
| SMK1 | 迷宫冒烟（maps/maze-smoke.ron + `--map`） | ✅ headless 全链路（Lives/Manual/开链对照图）；真机 `--map` 演示 ⏸ H1 |

## 待人工关卡（后置清单）

- H1 试玩：`cargo run -p loop-td` 灰盒手感（键盘 WASD 光标 / 1-4 选塔 / 空格放置 / X 出售 / P 暂停）；`LOOP_TD_MAP=maps/maze-smoke.ron cargo run -p loop-td` 换图演示。
- H3 审美：灰盒视觉（正交俯视相机/路径环/单位方块球体/HUD）未过视觉验收，后续 UI 增量统一处理。
- 视觉基线（VIS 系）随 UI 增量建。
