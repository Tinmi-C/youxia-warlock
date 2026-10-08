# warbase 路线图

> 流程：规格-测试-小步提交（ADR-0014）。规格底稿：`docs/topics/engine/warbase-implementation-design.md`（实现）+ `warbase-test-strategy.md`（用例）+ ADR-0012/0013。BX/WX/TS 编号是**增量名**（历史沿用的计划标签，不再是「卡」）。游戏侧进度在 `games/loop-td/docs/roadmap.md`。

| 增量 | 内容 | 状态 |
|------|------|------|
| WS0 | 根 cargo workspace 接线（成员/exclude/统一钉版/profile 上收） | ✅ f73848e |
| TS-WS | workspace 一击回归基准（warden 153 tests 全绿快照） | ✅ |
| BX0 | crate 落位（WarbasePlugin + WarbaseSet 契约 + ui/fx feature 门控 + 防腐化检查） | ✅ 8b5ac08 |
| TS0 | 确定性基建（MatchRng / test_app / 逐帧 drain 助手 / 临时文件隔离） | ▶ 下一增量 |
| BX1 | unit 域模型（统一 UnitDef + 攻甲类型表结构 + 经验等级） | ⬜ |
| BX2+WX1 | 伤害管线抽取 / warden 回迁（成对，断言语义不变） | ⬜ |
| BX3+WX2 | SkillCaster 抽取 / warden 回迁 | ⬜ |
| BX9a | MapBundle serde 类型 + RON 装载/校验器（P0.5 先行） | ⬜ |
| BX5 | 路径：闭环 + 圈数计数 + 分支字段位 | ⬜ |
| BX6 | 波次导演：Auto/Manual + 预告 + 提前召唤 + 限时 | ⬜ |
| BX7 | 资源：金/木/人口 + 兑换事件流 | ⬜ |
| BX8 | 规则插拔：漏怪策略 + 建造窗口 + 胜负 | ⬜ |
| build | 建造域：FreeGrid/FixedSlots 双模式 + 幽灵同源校验 | ⬜ |
| BX4+WX3 | 命令卡框架抽取泛化 / warden 回迁 | ⬜ |
| WX4 | 通用战斗 FX（feature "fx"）/ warden 回迁 | ⬜ |
| BX10 | headless harness（DriverStrategy + HarnessReport） | ⬜ |
| WX5 | BAL 迁 harness（读数口径对齐、不漂移） | ⬜ |
| WX6 | warden 参数化 + maps/warden.ron（基底验收 #0） | ⬜ |
| BX9b | bundle 装配器收口 + `--map` 加载 + 迷宫冒烟（SMK） | ⬜ |
