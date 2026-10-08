# loop-td 能力卡

> 工作流：立卡 → 实现 → 验收（红绿）→ 回归 → **一卡一提交**（同模板约定，见 `AGENTS.md`）。
> 基底卡（BX/WX/TS 线）在 `engine/warbase/docs/capability-cards.md`；本文件只登记游戏侧卡。
> 需求底稿：`docs/topics/game-design/loop-td-game-proposal.md`（v0.2）；测试用例全集：`docs/topics/engine/warbase-test-strategy.md`。

## 卡路书（游戏侧）

| 线 | 卡 | 标题 | 状态 |
|----|----|------|------|
| 脚手架 | LT0 | 立项接线（模板改造 + warbase 依赖 + 回归绿） | ✅ |
| 地图 | MA1 | 环形地图 + 建造网格 | ⬜ |
| 地图 | MA2 | 小金矿（薄插件，通道 C） | ⬜ |
| 敌人 | EN1 | 怪物定义表（bundle 数据） | ⬜ |
| 敌人 | EN2 | 绕圈行为验收（漏怪留场 + 圈数） | ⬜ |
| 敌人 | EN3 | 特性行为四件套（快速/重甲/治疗/光环） | ⬜ |
| 塔 | TO1 | 基础塔 ×4（攻甲克制体验） | ⬜ |
| 塔 | TO2 | 自由放置与回收 | ⬜ |
| 塔 | TO3 | 三级升级 | ⬜ |
| 塔 | TO4 | 功能塔 ×3（光环/减速/减甲） | ⬜ |
| 英雄塔 | HR1 | 获取三来源 | ⬜ |
| 英雄塔 | HR2 | 技能加点 UI 与流程 | ⬜ |
| 英雄塔 | HR3 | 天赋三选一 | ⬜ |
| 波次 | WA1 | 30 波波次表（maps/loop.ron） | ⬜ |
| 波次 | WA2 | 特性波编排 | ⬜ |
| 波次 | WA3 | BOSS 限时波 | ⬜ |
| 收束 | ST1 | 计分 + 结算屏（存量曲线） | ⬜ |
| 收束 | UI1 | 命令卡定制 + 存量 HUD | ⬜ |
| 收束 | MT1 | Meta 局间成长 | ⬜ |
| 收束 | BAL1 | 锚点 A1–A3 收敛 | ⬜ |
| 收束 | SMK1 | 迷宫冒烟（maps/maze-smoke.ron + `--map` 演示） | ⬜ |

（模板示例 PlayerMove 卡随模板保留在 `tests/behavior.rs` 作回归基线，不列入本表。）

## 立卡记录

### LT0 ✅ 立项接线

- **做什么**：`templates/bevy-game/` → `games/loop-td/`；package 改名 + `bevy_game::`→`loop_td::`；依赖 warbase（workspace 版本继承）；`build_app()` 装配 `WarbasePlugin`；AGENTS/GDD/能力卡就位；加入根 workspace members。
- **验收句**：`cargo test -p loop-td` 绿（模板既有行为测试语义不变）；`cargo test --workspace` 绿；游戏 crate 无任何管线实现代码（当前仅模板示例系统）。
- **提交**：`feat(loop-td): LT0 scaffold from template on the warbase base`
