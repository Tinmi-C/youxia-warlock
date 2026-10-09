# warbase 路线图

> 流程：规格-测试-小步提交（ADR-0014）。规格底稿：`docs/topics/engine/warbase-implementation-design.md`（实现）+ `warbase-test-strategy.md`（用例）+ ADR-0012/0013。BX/WX/TS 编号是**增量名**（历史沿用的计划标签，不再是「卡」）。游戏侧进度在 `games/loop-td/docs/roadmap.md`。

| 增量 | 内容 | 状态 |
|------|------|------|
| WS0 | 根 cargo workspace 接线（成员/exclude/统一钉版/profile 上收） | ✅ f73848e |
| TS-WS | workspace 一击回归基准（warden 153 tests 全绿快照） | ✅ |
| BX0 | crate 落位（WarbasePlugin + WarbaseSet 契约 + ui/fx feature 门控 + 防腐化检查） | ✅ 8b5ac08 |
| TS0 | 确定性基建（MatchRng / test_app / 逐帧 drain 助手 / 临时文件隔离） | ✅ afe40e0 |
| BX1 | unit 域模型（统一 UnitDef + 攻甲类型表 + 组件组合 spawn） | ✅ afe40e0 |
| BX2 | 伤害管线（基底侧全新实现：单写 Hp/归因/索敌/投射物） | ✅ afe40e0 |
| WX1 | warden 切换消费基底伤害管线（断言语义不变） | ⬜（序列调整：排在 loop-td 首可玩之后） |
| BX5 | 路径：闭环 + 圈数计数 + LapCompleted/Arrived + 开链终达 | ✅ afe40e0 |
| BX9a | MapBundle serde + RON 装载/校验 + assemble（ADR-0013） | ✅ a89b97b |
| BX6 | 波次导演：Auto/Manual + 预告 + 提前召唤 + BOSS 限时 + 清波 | ✅ 77a660a |
| BX7 | 资源：金/木/人口 + 兑换曲线 + 赏金入账（单命令流） | ✅ 77a660a |
| BX8 | 规则插拔：Lives/Stockpile/漏下家 + 建造窗口 + 绕圈增益 + 胜负 | ✅ bc0baf6 |
| build | 建造域：FreeGrid/FixedSlots 双模式 + 同源校验 + 占用/释放 | ✅ |
| BX3+WX2 | SkillCaster 抽取 / warden 回迁 | ⬜ |
| BX4+WX3 | 命令卡框架抽取泛化 / warden 回迁 | 🟡 BX4a 核心+BX4b 面板已落地（数据驱动底栏槽位、缓存键重建、升级阶不入建造卡；loop-td 已切换消费：B 切模式/数字键经卡面真建造/动作分发）；剩 WX3 warden 回迁与视觉定版（H3） |
| WX4 | 通用战斗 FX（feature "fx"）/ warden 回迁 | ⬜ |
| BX10 | headless harness（DriverStrategy + HarnessReport + Greedy 基线 + 注入） | ✅（放置/出售/网格构建随之上收 base：try_place/try_sell/Buildable + bundle::build_grid） |
| WX5 | BAL 迁 harness（读数口径对齐、不漂移） | ⬜（loop-td 侧 BAL-smoke 已可跑真图+可复现；warden 读数迁移仍待） |
| WX6 | warden 参数化 + maps/warden.ron（基底验收 #0） | ⬜ |
| BX9b | bundle 装配器收口 + `--map` 加载 + 迷宫冒烟（SMK） | ⬜ |

**序列调整记录（2026-10-08）**：BX2 在基底侧按 ADR-0011 管线模式全新实现（warden 的实现保持不动、继续自用），WX1 回迁整体后移到 loop-td 首可玩之后——基底新机制的验收场优先（用户指令「把需求都开发完成」优先打通玩法侧）。抽回迁成对纪律不变，只是排期后置。
