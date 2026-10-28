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
| LG3 | 方案 A UI 套用（主题两件套+六面板+离屏截图取证）+ H1 | 2007（src 插入 1147/净 1061 + 皮肤 RON 48 + 测试 692 + 文档 120；另 5 张 PNG 证据） | ≈38m | ≈22m | **0** |
| LG4 | round-4 LDEBT 上收切换（七洞 L1/L2/L3/L5/L7/L8 消费侧迁移，L6 契约随引擎落地） | 行数口径改为**净变化**（上收=删游戏侧补丁）：round.rs 重写后 ≈716（原 715 → 删 608 补丁逻辑 / 写 609 基底消费），resources.rs -56（三账本退役），gameplay.rs -22，地图 RON 净 -34（48 坐标→4 walkable 字段+pop_cost）；测试迁移 behavior 15 / ui_panels 10 / ui_shots 1 | 见 git log（round4/ldebt 分支） | — | **0**（七洞全收） |

## 全图汇总（LG0-3 收官，KPI 一级产出）

| 项 | 合计 | 说明 |
|----|------|------|
| 游戏侧总行数 | 4191 插入（LG3 净 +1903；另 5 张截图 PNG） | LG3 构成：Rust src 1147 / 皮肤 RON 48 / 测试 692 / 文档 120 |
| 总耗时（墙钟） | ≈1h34m（12m20s + 18m13s + 25m41s + ≈38m） | LG0 12m20s + LG1 18m13s + LG2 25m41s + LG3 见上；全程 -j4 + 私有 target |
| 总洞数 | **7**（L1-L8 去重） | LG3 是唯一**零新洞**增量——UI 换肤纯消费基底既有缝 |
| 引擎改动 | **0 行** | engine/warbase/ 全程零 diff（护栏达成，ADR-0012 §6） |
| 测试账目 | LG0-2 13 条 + LG3 +14（面板 10 + lib 4）= 27；workspace 门禁 588 passed | 零漂移：LG0-2 的 13 条原样通过 |

**换肤复用成本（LG3 核心度量，方案 A 从 loop-td 迁到 legion）**：

| 件 | 行数 | 性质 |
|----|------|------|
| `assets/ui/plan-a.ron` | 48 | 数据逐字拷贝（token 段与 loop-td diff 为空，仅头注释本地化） |
| `src/systems/theme.rs` | 23 | include_str + from_ron（照搬 loop-td 同名文件） |
| `src/systems/font.rs` | 59 | 共享 CJK 字体接入（照搬 loop-td，含 warbase UiFont/FxFont 两 seam 接线） |
| `plugins/game.rs` 接线 | 2 | `insert_resource(theme::plan_a())` + `.add_plugins(PanelsPlugin)` |

→ **主题接入本体 = 1 行代码 + 1 个 RON 数据文件**（「一行级接入」成立）；CJK 字体两件套 = 102 行照搬
（一次成本，跨游戏第二次复用，warden→loop-td→legion 共享同一份 NotoSansSC 文件）。
六面板本体（panels.rs 1044 行）是**模式复制**而非逐字拷贝——legion 的面板数据源（命数板/递增价/攻甲型）
与 loop-td 完全不同，复制的是 loop-td 验证过的架构骨架：PanelRoot + per-panel cache key + 主题 token 渲染
+ 布阵高亮的双层（headless 标记 + render-store 守卫的 3D 视觉）。

## 对比 rts-lab（同期增量）

| 项 | rts-lab | legion | 备注 |
|----|---------|--------|------|
| 立项到可跑（RL0-1 vs LG0） | RL0 脚手架 + RL1 主城农民切片（RL0→RL1 提交间隔 30m，git 10:25→10:55） | LG0 12m20s 墙钟（含 7m37s 冷编译） | legion 直接复用 rts-lab 沉淀的装配模式（load_map/kit 观察器/灰盒三件套），模板复制后基本是「填空」 |
| 玩法层切片（RL2 交战+技能 vs LG1 回合层全链） | RL2 ≈ 26m（10:55→11:40 提交间隔） | LG1 18m13s | LG1 交付了完整回合循环（商店/布阵/自动交战/漏怪/收入/出局接 LastTeamStanding），机制全部来自基底：Order::AttackMove/Hostility 外交/combat 全自动/UnitDied 赏金/LastTeamStanding |
| UI 层（rts-lab 自写 UI vs LG3 方案 A 套用） | 自写 HUD/小地图（无主题 token 体系，每图各写各的） | LG3 一个增量交付六面板+换肤+截图取证，**零新洞** | rts-lab 时代 UI 是每图重复劳动；LG3 证明 warbase UiTheme + loop-td 面板模式沉淀后，第三张图的 UI = 1 行接入 + 1 个 RON 拷贝 + 按骨架填面板数据源 |
| 引擎改动 | 0 行 | 0 行 | 护栏达成（engine/** 零 diff） |

**收官结论**：legion（LG0-3）全图 ≈1h34m 墙钟 / 4191 插入行 / 7 洞 / 引擎零改动，
交付了一张 2v2 军团战争图的完整可玩闭环（回合层+商店+发兵+胜负判定+方案 A 六面板 UI）。
与 rts-lab 的对比呈两条曲线：**机制层**（LG0→LG2）复用 rts-lab 沉淀的装配模式后每个切片都快于
rts-lab 同期（12m vs 30m 立项、18m vs 26m 玩法层）；**UI 层**（LG3）则展示了另一种复用形态——
loop-td 把方案 A 实现沉淀为「UiTheme token + PanelRoot/cache-key 架构 + 共享 CJK 字体」三件套后，
legion 的 UI 增量不再需要重新发明任何东西（零新洞），成本从「自写整套 HUD」降为「1 行接入 + 102 行照搬 +
按骨架填内容」。这正是 ADR-0019 想度量的曲线：**基底+先行图的沉淀每多一张图，后续图的边际成本递减**。

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

### LG3（2026-10-28 02:25 – 03:00，commit 见 git log）

- **范围**：方案 A「铁金条约」UI 套用——主题两件套照搬（plan-a.ron 数据逐字拷贝 48 行 +
  theme.rs 23 行 + font.rs 共享 CJK 字体 59 行 + GamePlugin 接线 2 行，即「换肤复用成本」原始数据）；
  六面板（照 loop-td panels 模式实现，panels.rs 1044 行）：①HUD 资源条（本家金/人口/回合/
  建造期倒计时帧→秒换算 ceil，锚点 frame 0→30s / 1799→1s）②命数板（四家 LivesBoard，队色
  P0P2 蓝/P1P3 红，命尽显示「出局」）③雇佣兵商店卡（6 兵种 def 表价格/人口/攻甲型中文，
  CommandSlotButton + CommandAction::Custom("hire:<def>")——点击与数字键共用 HireCommand
  派发缝，金不足/战斗期置灰）④布阵格高亮（悬停商店卡 → 本家空闲 DeploySlot 打 DeployGlow
  标记 + render-store 守卫的金色发光片；已占格不亮）⑤发兵入口（lane 表驱动的目标行 +
  本发/下一发/再下发递增价预览 + 发送钮 Custom("send:merc_swarm")，与 G 键等价）⑥回合横幅
  （建造期 Xs/战斗期/结算三态中文——即时结算用 SettleFlash 90 帧保持「第 N 回合结算 · +50 金」，
  终局平局/终局横幅）。旧的单行调试 HUD（hud.rs 75 行）随之退役删除。
  离屏截图取证 5 张（warden ui_shots 模式：primary_window None + 相机重定向 offscreen Image +
  Screenshot readback + 非黑断言）入 docs/img/：全景/商店卡/命数板/横幅/结算横幅，经视觉模型
  复核（面板位置/中文无豆腐/置灰态/队色）。H1 清单入 README。
- **行数**：≈1990 插入 / 100 删除（src 净 1061：panels 1044 +
  font 59 + theme 23 + 接线 14 - hud 75 等；RON 48；测试 692 = ui_panels 432 + ui_shots 260；
  文档 README/roadmap/kpi ≈100；另 5 张 PNG 证据）。
- **耗时**：≈38m 墙钟（含私有 target 冷编译 ~9m——本 worktree 全新 target，
  bevy 依赖树从零编；动手段即 27 测全绿，其中 2 轮测试修正：消息 cursor 语义（testing::send
  自带一帧 update / 全新 cursor 会重读上一帧点击动作 → 派发器改持久 cursor 资源）、商店卡
  可负担位翻转重建导致 hover 丢失的测试分层）。
- **新洞**：**0**——本增量纯消费基底既有缝：UiTheme::from_ron（皮肤即数据）、
  CommandSlotButton + command_clicks（点击派发零改动）、CommandAction::Custom（游戏侧动作
  词汇，SEAM3b 设计目标在第三张图上验证）、warbase::ui::UiFont/FxFont（CJK seam）、
  面板架构模式来自 loop-td 沉淀（游戏间复用，非引擎）。
- **基底消费清单**（零改动的复用面）：UiTheme/UiPalette/UiTypography/UiSpacing/UiRadius
  （Plan-A token 全套）、CommandCards 语义之 CommandSlotButton/command_clicks/refresh_tooltip
  （悬停 tooltip 中文自动走 UiFont seam）、CommandAction::Custom 派发缝、warbase::fx::FxFont、
  共享字体文件（workspace 根 assets/fonts 单份，第三游戏复用）。

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
- LG3：+14 条（tests/ui_panels.rs 10：皮肤 token 钉死、mutate→面板换色零代码、
  命数板四家+出局、商店卡 6 def 价格/人口/攻甲型+按钮动作、倒计时帧换算 0→30s/1795→1s、
  发兵递增价预览步进、横幅四态（建造/战斗/结算/平局）、CJK 字体句柄达每个面板文本+字体文件在库、
  布阵高亮 12→11（占格）→0（取消悬停/战斗期）、点击派发雇佣+发兵；lib 单测 4：布局矩形、
  倒计时纯函数、兵种名词汇、横幅文本词汇）。另 ui_shots.rs 1 条离屏 GPU 取证（--ignored）。
- 门禁：`cargo test --workspace -j4` **588 passed / 0 failed**
  （LG0-2 的 13 条零漂移原样通过；基线 574 + LG3 新增 14 = 588）。

### LG4（round-4 LDEBT，分支 round4/ldebt）

- **范围**：七洞上收后的游戏侧切换——①三账本退役（Purse/LivesBoard/PopLedger →
  基底 `Wallets`/`PlayerLives`，钱包/命数/人口 cap 由基底 assemble 播种）；
  ②出局清扫+LastTeamStanding 判胜移交基底 `TeamLives`（主城 0 伤害旗帜 kit
  删除，队伍存活=命数板）；③回合封顶平局/胜局写基底 `MatchFlow::Ended{Win|Draw}`
  （L7 转正，不再借 Won 通道）；④发兵注入切基底 `spawn_owned_deferred`
  有主缝（L8——Owner+Allegiance+AttackMove 令出生即武装，游戏侧只补
  Merc/SentMerc 标记）；⑤arena.ron 四 lane `walkable: false` + build 改
  FreeGrid（L1——48 格手写坐标退役，游戏侧按 lane 参数化生成，坐标逐值
  零漂移含行序规范化）；⑥人口数值读 `UnitDef.pop_cost`（L3——游戏侧硬表
  退役，可雇集合仍由命令卡表表达）；⑦游戏侧 bounty_credit 删除（击杀赏金
  归因=基底 apply_econ 按 killer Owner 入对应钱包，L2——双记账摩擦消除）。
- **零漂移口径**：17 条旧测（behavior 13 + lib 4）全部语义等价迁移（价格
  25/27/30、赏金 +5、命 20→19、1800 帧钟、出局两帧清扫加冕等数值锚点原样），
  UI 测试 10 条同步迁移；新增 2 条基底缝断言（⑮ Resources==Wallets[P0] 视图
  恒等贯穿雇兵/结算；⑯ 2v2 赏金只入 killer 钱包，旁观队钱包分文不动）。
- **引擎侧**（分支同批提交）：econ Wallets/LocalPlayer/镜像同步、rule
  TeamLives/PlayerLives/MatchOutcome、wave MarchMode/SpawnRequest、path
  walkable、unit pop_cost——见 engine/warbase/tests/ldebt_domain.rs 10 断言。
