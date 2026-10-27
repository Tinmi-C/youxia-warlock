# legion 复用效率 KPI 台账（一级产出，ADR-0019 Phase 2）

> 每增量记录：游戏侧新增代码行数 / 耗时 / 触发的新洞（通道 C）。
> 对比基准：rts-lab（RL 系）。行数口径：git 插入行（含注释与测试）；
> 耗时口径：起止墙钟（含编译等待）+ 括注纯动手时间。
> 机器纪律影响计时：全程 cargo -j4 + 私有 target（冷编译 7m37s 计入 LG0 墙钟）。

## 总表

| 增量 | 内容 | 游戏侧新增（行） | 耗时（墙钟） | 纯动手 | 新洞 |
|------|------|------------------|--------------|--------|------|
| LG0 | 立项+地图+灰盒可跑切片 | 862（其中 Rust 578 / 地图 RON 117 / 文档 167） | 12m20s | ≈4m45s | 2（L1 L2） |
| LG1 | 回合层+商店+自动交战+漏怪+收入 | 721（round.rs 453 + 其余 src 138 + 测试 226 净增 + 地图 4） | 18m13s | ≈13m | 3（L3 L5 L6）+ 设计修正 1 |
| LG2 | 发兵（递增价注入）+击穿赏金+20 回合封顶胜负 | 601（round.rs 284 + 其余 src 52 + 测试 219 + 文档 46） | 25m41s | ≈5m30s | 2（L7 L8） |
| LG3 | 方案 A UI 换肤 + H1 | — | — | — | — |

## 对比 rts-lab（同期增量）

| 项 | rts-lab | legion | 备注 |
|----|---------|--------|------|
| 立项到可跑（RL0-1 vs LG0） | RL0 脚手架 + RL1 主城农民切片（RL0→RL1 提交间隔 30m，git 10:25→10:55） | LG0 12m20s 墙钟（含 7m37s 冷编译） | legion 直接复用 rts-lab 沉淀的装配模式（load_map/kit 观察器/灰盒三件套），模板复制后基本是「填空」 |
| 玩法层切片（RL2 交战+技能 vs LG1 回合层全链） | RL2 ≈ 26m（10:55→11:40 提交间隔） | LG1 18m13s | LG1 交付了完整回合循环（商店/布阵/自动交战/漏怪/收入/出局接 LastTeamStanding），机制全部来自基底：Order::AttackMove/Hostility 外交/combat 全自动/UnitDied 赏金/LastTeamStanding |
| 引擎改动 | 0 行 | 0 行 | 护栏达成（engine/** 零 diff） |

## 增量明细

### LG0（2026-10-28 01:12 – 01:25，commit deb399b）

- **范围**：模板复制 → crate 改名 legion → workspace 入列 → maps/arena.ron
  （4 lane 共享走廊 / 48 布阵格 / 6 雇佣兵 def / 2v2 teams / 四家主城）→
  灰盒渲染（lane 条纹+布阵垫板+单位方块）+ HUD 四家金数 → 4 条装配断言 → cargo run 冒烟。
- **行数**：862 插入（Rust 578 = src 413 + tests 165；RON 117；AGENTS/README/GDD/roadmap 167）。
- **耗时**：12m20s 墙钟；其中 7m37s 是新 worktree 私有 target 的冷编译（bevy 全依赖树），纯动手 ≈4m45s。
- **新洞**：L1、L2（见下）；另有一个**设计修正**（非洞）：首版四条平行 lane 间距 4.0，
  对进的两军永远进不了近战射程（漏怪直通）——改为**配对共享走廊**（P0/P1 同走 z=-4），
  对头 clash 才是军团战争的交战几何。

### LG1（2026-10-28 01:25 – 01:43，commit 见 git log）

- **范围**：回合层状态机（建造 1800 帧→战斗→即时结算→下一回合，冻结于 MatchFlow 终局）；
  雇佣兵商店（HireCommand：相位/存活/格归属/占格/金/人口六重校验，def 表即价格）；
  自动交战（round_tick 发 UnitCommand::AttackMove 至对家底线，基底 MOV4+combat 全自动）；
  漏怪（leak_tick：底线半径 0.9 → 对家命 -1 → 命尽清单位出局）；回合收入
  （结算 +50 + UnitDied.killer 归因赏金入钱包）。5 条验收测试（⑤-⑩）。
- **行数**：721 净增（round.rs 453 新文件；src 其余 +159/-24；tests +226/-4；地图 +6/-2）。
- **耗时**：18m13s 墙钟（含 2 轮编译失败修复、1 次调试诊断（互杀同帧赏金归因 L6）、
  全量 workspace 回归——回归与 kpi 撰写并行）。
- **新洞**：L3、L5、L6（见下）。
- **基底消费清单**（零改动的复用面）：bundle 装配管线、UnitRegistry/spawn_unit、
  MoveAgent/tick_orders/AttackMove/acquire_engagement/separate_agents、AttackKit/
  pick_targets/fire_attacks/resolve_damage、AttackArmorTable、Hostility/MatchDiplomacy
  （Allegiance 派生）、UnitDied/DamageDealt 消息流、LastTeamStanding/MatchWinner、
  warbase::testing 框架。

### LG2（2026-10-28 01:53 – 02:19，commit 见 git log）

- **范围**：发兵商店（SendCommand：相位/存活/自指/已出局/通路（lane 配对即
  敌对+走廊几何）/def/金七重校验，25 金起同目标 +10% 递增——floor(25×1.1^n)
  取整规则钉死，per-target SendLedger 记账）；注入出征（PendingSends 队列，
  Build→Battle 转换帧从出资方布阵区出生**自带全套 kit**（观察器晚一帧挂 kit
  会吃掉同帧出征令——spawn 同帧发令的时间坑），并入出资方 AttackMove 编队）；
  发兵击穿赏金（SentMerc 标记，漏怪时出资方 +5/兵，与结算 +50 同帧入账）；
  20 回合封顶胜负（team_lives 按 MatchDiplomacy 折算队伍总命数，多者胜/
  平首平局，MatchEnded reason 可读，胜者另写 MatchWinner；与 LG1 的
  LastTeamStanding 中途出局路径并存——先到先收口）。真机入口：G 键发
  swarm 至本路对家（LG3 命令卡补全）。4 条验收测试（⑪-⑭）。
- **行数**：601 净增（round.rs +284；resources 27 / game 14 / components 5 /
  gameplay 6（kit 观察器加 Without<MoveAgent> 防同帧 clobber）；测试 +219；
  README/roadmap/kpi 46）。
- **耗时**：25m41s 墙钟——动手段 01:53→01:59 即 legion 13 测试全绿（≈6m，
  其中 2 轮测试修正：消息 cursor 逐帧排水语义、同点出生分离位移容差），
  纯动手 ≈5m30s；余下 ~20m 是私有 target 冷编译全量门禁（~11m）+
  基线 stash 复测（~3m，为把「零漂移」从推断变成实测）等纪律成本。
- **新洞**：L7、L8（见下）。
- **基底消费清单**（零改动的复用面）：spawn_unit_deferred（发兵出生即雇佣
  同款）、MOV4 tick_orders/AttackMove（发兵并编出征，含 formation 散开）、
  separate_agents（同点出生自动散开）、combat 全自动、MatchDiplomacy
  （allegiance_of/team_of——封顶折算队伍命数）、MatchFlow/MatchEnded/
  MatchWinner（封顶写入）、LivesBoard/leak 链（击穿赏金 riding 其上）。

## 新洞清单（通道 C：基底缝缺失 → 游戏侧补丁孵化）

| # | 增量 | 需求 | 缺的缝 | 游戏侧补丁 | 建议上收形态 |
|---|------|------|--------|-----------|--------------|
| L1 | LG0 | lane 是纯路由数据（可走） | `bundle::build_grid` 把 path 折线一律落成 build+nav 阻挡（TD 出怪轨道出身）；无「path 仅作路由」开关 | build 域选 FixedSlots（该分支不处理 path，NavGrid 保持空默认→直线兜底）；代价：48 个布阵格坐标显式写进 RON | bundle path 增加 `walkable: bool`（或 build/nav 段与 path 段解耦） |
| L2 | LG0/LG1 | 四家各自的金库/命数/人口 | `Resources`/`Population`/`LivesLeft` 是单人全局单例，无 per-player 视图 | 游戏侧 `Purse`/`LivesBoard`/`PopLedger`（起始值仍读 bundle）；摩擦：基底 `apply_econ` 把击杀赏金记入全局 Resources（MP 下无意义），游戏侧再按 killer 归因记账 = 双记账 | 基底 econ/rule 出 per-player 泛化（`LocalPlayer` 视图或 `Vec<Wallet>`），赏金归因走 killer 的 Owner |
| L3 | LG1 | 买兵人口占用 1-4（地图数值） | `UnitDef` 无 population 字段 | 游戏侧 `pop_cost(def_id)` 硬表 + `Merc.pop` 组件记账 | UnitDef 增加 `pop_cost`（serde default 0，旧图零漂移） |
| L4 | LG0 | aoe 兵种溅射 | combat 无溅射/AoE 伤害缝（DamageRequest 单目标） | merc_aoe 暂按 Siege 攻型+攻甲矩阵差异占位（地图注释已标） | DamageRequest 支持 radius / or 溅射组件 |
| L5 | LG1 | 命尽出局接 LastTeamStanding，且建造期空场不能判胜 | `WinCondition::LastTeamStanding` 只认 AttackKit/PathFollower 携带者，没有与「场上单位」解耦的「队伍存活」概念——回合制下对面本回合还没出兵就会误判 | 主城挂 0 伤害/射程 1.0 的「旗帜 kit」（永不开火，仅作常驻战力标记），出局清单位=旗帜消失=基底规则正确收口 | WinCondition 增加 lives/elimination 变体，或「team alive」查询缝 |
| L6 | LG1 | 击杀赏金归因到玩家 | `UnitDied.killer` 是实体 id，`cleanup_dead` 会在同一 Cleanup 里把互杀的 killer 一并 despawn——晚于它运行的消费者 deref 不到 Owner | `bounty_credit` 挂 `in_set(Cleanup).before(cleanup_dead)`（killer 已 Dead 标记但实体还在） | 基底文档化该顺序契约，或 UnitDied 携带 killer 的 Owner 快照 |
| L7 | LG2 | 20 回合封顶：命数多者胜，平数平局 | `MatchFlow` 只有 Won/Lost（单人胜负词汇），无 Draw/Ended 变体，也没有游戏层「结束比赛」缝（LastTeamStanding 是基底唯一写入口） | 游戏侧 round_tick 直接 ResMut 写 `MatchFlow::Won` + `MatchEnded{Ok(reason)}`：平局借 Won 通道、reason 写 "draw on lives"、MatchWinner 刻意不写 | MatchFlow 增加 `Ended{outcome: Win(team)/Draw}`（或 end_match(reason, Option<TeamId>) 游戏侧 API） |
| L8 | LG2 | 发兵：给指定对手注入下一回合额外兵（proposal §3 预期走 wave director per-player 注入缝） | `run_director` 的 spawn 路径硬编码 `Allegiance::Attacker` + `PathFollower` 出怪轨道（TD 模型）：无有主单位注入、无 per-player 出生点、且 path-following 出怪不停车交战（军团图要 AttackMove 穿 contested 走廊） | 发兵复用雇佣 spawn（spawn_unit_deferred + 游戏侧全套 kit，见 round_tick 转换臂注释——同帧发令要求 spawn 即带 MoveAgent，否则 attach 观察器晚一帧会 clobber 同帧写入的令）并入出资方出征编队 | wave 域增加 SpawnRequest 注入缝（owned units + 自定义行军模式），或通用「有主单位定点出生并受令」API |

> 上收原则（ADR-0012 §6）：以上全部游戏侧孵化，engine/warbase 零改动；
> 是否上收由 SEAM 修订决策，不在本图范围内。

## 测试账目

- LG0：4 条装配断言（lane/外交/布阵格、雇佣兵表锚点、四家主城、钱包+lane 可走）。
- LG1：+5 条（买兵扣费人口/拒绝、回合钟 1800 帧、对进互射 DamageDealt、漏怪扣命、
  出局+LastTeamStanding 加冕）+ 结算 +50 收入断言并入回合钟用例。
- LG2：+4 条（发兵递增价 25/27/30+出征令攻对家 lane+四类拒绝、发兵击穿
  对家命-1+出资方+5、20 回合封顶命数多者胜（MatchEnded reason+MatchWinner）、
  同数平局 reason+无 MatchWinner）。
- 门禁：`cargo test --workspace -j4` **574 passed / 0 failed**（私有 target 复测：
  本分支 pre-LG2 基线实测 570 = 任务书基线；570 + LG2 新增 4 = 574，零漂移。
  LG1 行记的 571 与本分支实测差 1，系当时台账笔误，非测试漂移）。
