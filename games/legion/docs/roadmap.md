# 路线图（LG 系，ADR-0019 Phase 2）

> 本项目是基底复用效率的度量对象：每个增量记录行数/耗时/新洞（`docs/kpi.md`）。
> 需求规格见 vault `docs/topics/game-design/legion-td-proposal.md`；验收落 `tests/behavior.rs`。

| 增量 | 内容 | 消费的基底域 | 状态 |
|------|------|--------------|------|
| LG0 | 立项：模板复制/改名/workspace 入列；maps/arena.ron（4 lane / 48 布阵格 / 2v2 / 6 雇佣兵 def / 四家主城）；灰盒渲染+HUD 金数；装配断言 | bundle/unit(teams)/rule | ✅ |
| LG1 | 建造期+自动交战：回合状态机（建造 1800 帧→战斗→结算→下一回合）；雇佣兵商店（HireCommand 扣费布阵+人口记账）；战斗期 AttackMove 至对家底线；漏怪扣命（命尽出局）；结算 +50 金+击杀赏金 | movement(MOV4)/combat/econ(赏金)/selection | ✅ |
| LG2 | 发兵（花钱给对手注入下一波，同目标递增价）+ 击穿赏金 +5 + 20 回合封顶结算（命数多者胜/平局）| movement(MOV4)/combat/unit(spawn)/rule(MatchEnded)；wave per-player 注入无缝（洞 L8）、MatchFlow 无平局变体（洞 L7） | ✅ |
| LG3 | 方案 A UI 套用（主题两件套 + 六面板：资源条/命数板/商店卡/布阵高亮/发兵入口/回合横幅）+ 离屏截图取证 + H1 清单（换肤复用成本记录） | ui(UiTheme/CommandSlotButton/CommandAction::Custom)、共享 CJK 字体 | ✅ |
| LG4 | round-4 LDEBT 上收切换：游戏侧三账本（Purse/LivesBoard/PopLedger）退役→基底 Wallets(L2)/PlayerLives(L5)；出局清扫+判胜移交基底 TeamLives（主城旗帜 kit 删除）；回合封顶写 MatchFlow::Ended{Win/Draw}(L7)；发兵注入切 spawn_owned_deferred 有主缝(L8，令出生即武装)；arena.ron 四 lane walkable:false + FreeGrid（48 格手写坐标退役，L1）；人口读 def.pop_cost(L3)；17 测零漂移迁移 + 新增 L2 视图恒等/2v2 赏金隔离断言 | econ(Wallets)/rule(TeamLives/Ended)/wave(spawn_owned_deferred)/path(walkable)/unit(pop_cost) | ✅ |
| H1 | 人工试玩：LG0-LG4 清单过一遍（README「操作」/「H1 人工清单」节） | — | ⬜ |
