# 能力卡工作流 + Warden 卡路书

> 团队理念（ADR-0002 / 方向纪要 §3）：自研 = AI 装配层，不是引擎轮子。
> 装配层三支柱在项目里的落地：**能力卡（WHAT）→ 验收闭环（证明做对了）→ 观察通道（看见在做什么）**。
> 本卡路书由 `docs/requirements.md`（设计层源）翻译而来；规则/数值以 requirements 为准，验收以真人试玩信号（`docs/GDD.md`）为准。

## 为什么是能力卡

每学一个功能、每做一个系统，都写一张「能力卡 + 验收句」。**写得出来 = 理解到位；验收句能执行 = 做得对**。
卡同时是给 AI 的「需求规格」——AI 按卡实现，人按验收句验收，AI 和人的沟通成本降到最低。

## 卡模板

```yaml
能力卡: <名称>
类型: system | component | asset | gameplay-system | architecture | ...
接口:
  输入: （它读什么：组件/资源/事件）
  输出: （它写什么：组件/资源/事件/命令）
行为: （每帧/每次做什么，写规则本身，不写目的）
验收句: （数字化、可执行、含边界条件——怎么自动/半自动验证它做对了）
```

## 工作流（和 AI 一起开发）

```
立卡 ──► AI 实现 ──► 人验收 ──► 回归钉死 ──► 一卡一提交
  │           │           │           │
  │           │           │           └─ feat: <卡名>（Conventional Commits）
  │           │           └─ 跑游戏 + cargo test + 日志仪表对照验收句
  │           └─ 新机制 = 新组件 + 新系统，老系统零改动
  └─ 验收句写不出来 = 没理解清楚，先问人，不开工
```

## 领域归属（项目独有规则）

| 域 | 插件 | 收哪些卡 |
|----|------|---------|
| map | `plugins/map.rs` | 路径/基地/塔位 |
| enemies | `plugins/enemies.rs` | 敌人定义/移动/漏怪/护甲/回血 |
| towers | `plugins/towers.rs` | 塔定义/放置/自动开火/融合 |
| waves | `plugins/waves.rs` | 波次配置/生成/结算/输赢 |
| economy | `plugins/economy.rs` | 金币来源/去向 |
| ui | `plugins/ui.rs` | 商店/融合/三选一/HUD |
| state | `src/states.rs` | 双速节奏（经营/战斗） |
| meta | `plugins/meta.rs`(新增) | 解锁链 |
| projectiles | `plugins/projectiles.rs`(新增) | 弹体生成/飞行/命中/清场（PR 卡） |
| fx | `plugins/fx.rs`(新增) | 血条/受击/死亡/状态表现（FX 卡） |
| debug | `plugins/debug.rs` | 观察通道/调试面板/作弊/场景直达（DB 卡）/展示场（SM 卡） |
| skills | `plugins/skills.rs`(新增) | 指挥官技能/冷却/施放（SK 卡） |

> 跨领域时序一律走 `GameSet`（`src/sets.rs`），不要手调系统链。

---

# 卡清单（按领域 + 构建顺序）

> 状态：📝 待立卡（已写规格）/ ▶ 待实现 / ✅ 已实现。/ 斜体 = 已从模板带入。
> **首条可玩闭合** = 标 `★` 的卡（先跑通「单塔单波 · 战斗」再叠加核心乐趣）。

## ★ 首条可玩闭合（先做，验证双速节奏+战斗）

> ✅ **已实现（2026-09-03）**：MA1/MA2、EN1/2/3、TO1/2/3/4、WA1/2/3、EC1、ST1、UI4 已落地，
> `cargo check --all-targets` + `cargo test` 全绿（7 条断言，`tests/behavior.rs`）。
> **简化**（后续卡补齐）：塔即时命中（无弹体/AOE/减速——TO6/EN5 未做）、放置走键盘（1-4+E），bevy_ui 商店为 UI1 后续卡；
> 敌人 speed=1.0 的遍历时间未按 requirements §9 锚点校准。
> **融合语义已定**：两原料塔须在场上（各占 1 格）→ 合成后占 1 格，净腾 1 格；手续=原料总造价 20%（见 TO5）。
> **§9 移速锚点已校准（2026-09-04）**：`spawn_enemy` 按 `def.speed × (路径长/20s)` 定世界速率——speed 1.0 走完
> L 路径（43 单位）≈20 秒，锚点从实际路径动态推导（改地图不失义）。已入 `tests/behavior.rs`
> （`ordinary_enemy_crosses_path_in_about_twenty_seconds`）。塔射程/怪物血量未动，节奏变快属预期，试玩后再调。

### MA1 · L 形路径 + 基地（map）
- 接口: 输入 waypoint 表（入口左上→基地右下，L 形）；输出 Path 资源 + 基地实体（hp=10）。
- 行为: 敌人按 waypoint 顺序推进；到终点触达基地。
- 验收句: 一条路径下，以 speed=s 推进的敌人用 ≈ `路径长度/s` 秒到达终点（误差<2%）；基地 hp 初始=10。

### MA2 · 8 塔位（map）
- 接口: 输入塔位坐标表；输出每格 empty/occupied 状态。
- 行为: 塔位邻接路径两侧；放置占一格。
- 验收句: 场景塔位数==8；在 1 格放置后，occupied 计数==1。

### EN1 · 敌人定义表（6 种）（enemies）
- 接口: 输入敌人类型枚举；输出 EnemyDef{hp,speed,leak,mech,kill_gold}。
- 行为: 按 requirements §9 表填。
- 验收句: 生成「普通怪」→ hp=30,speed=1.0,leak=1,kill_gold=6；生成「盾兵怪」→ hp=90,speed=0.6,leak=2。

### EN2 · 沿路径移动 + 漏怪（enemies）
- 接口: 输入 Enemy + Path；输出 Translation 推进；到终点 → 扣基地 hp 并 despawn。
- 行为: 敌每帧向下一 waypoint 前进 speed×Δt；到底扣 `def.leak` 点基地血。
- 验收句: 普通怪到达终点 → 基地 hp 10→9 且该怪 despawn；推进距离与帧率无关（60/144fps 误差<1%）。

### EN3 · 死亡 + 击杀金币（enemies/economy）
- 接口: 输入 Enemy hp≤0；输出 Economy.gold += kill_gold，despawn。
- 行为: 生命归零即结算。
- 验收句: 击杀 1 普通怪 → gold +6 且该怪被移除。

### TO1 · 4 基础塔定义表（towers）
- 接口: 输入塔种枚举；输出 TowerDef{cost,damage,attack_speed,range,attack_type,role,slow?}。
- 行为: 按 requirements §5 表填；射程用分级（近/中/远）。
- 验收句: 弓箭手塔 def → cost=50,damage=10,attack_speed=1.0,攻击类型=物理·单体；法师塔 cost=70,damage=15。

### TO2 · 塔放置（towers）
- 接口: 输入 (slot, tower_type, economy)；输出 占位实体 + 扣金币。
- 行为: 空塔位且金币足 → 放置并扣 `Def.cost`；否则不放置不扣钱。
- 验收句: 金币 100 放弓箭手(50) → 占位成立、gold 100→50；金币 40 放弓箭手 → 不放、gold 不变。

### TO3 · 自动开火 + 伤害（towers）
- 接口: 输入 Tower + 目标 + 冷却；输出 对目标施加 damage。
- 行为: 射程内有目标且冷却到 → 按 Def 伤害打一次，冷却=1/attack_speed。
- 验收句: 攻速 1.0 且目标在射程内 → 每秒命中 1 次、每次扣 Def.damage；目标出范围 → 不开火。

### TO4 · 索敌（默认最近）（towers）
- 接口: 输入 Tower + 敌集合；输出 当前目标实体。
- 行为: 默认锁射程内最近敌人；神射手等特殊锁最高血量（后续卡）。
- 验收句: 射程内 2 个敌人 → 锁距离更近者；无目标 → 不开火。

### WA1 · 波次配置 + 生成（waves）
- 接口: 输入波次索引；输出 本波敌人 spawn 队列、波状态(combat/intermission)。
- 行为: 按 requirements §10 表生成；波内连续生成；波之间进入 intermission。
- 验收句: 波 1 → 生成 5 普通怪；波 5 → 生成 1 精英+6 普通。

### WA2 · 波间结算 + 锁建造（waves/state）
- 接口: 读波状态；输出 Economy.gold += wave_reward、进入 intermission（开建造）。
- 行为: 波全清（敌全死/漏完）→ 加波奖励、开经营菜单；波进行中锁建造。
- 验收句: 清波 1 → gold +20，进入 intermission，建造系统可运行；combat 状态 → 建造系统关闭。

### WA3 · 输赢（waves/state）
- 接口: 读基地 hp + 波进度；输出 NextState(GameOver/Win)。
- 行为: 基地 hp≤0 → GameOver；第 10 波清完 → Win。
- 验收句: 基地 hp 到 0 → state==GameOver；清完第 10 波 → state==Win。

### EC1 · 经济来源/去向（economy）
- 接口: 读/写 Economy{gold,lives}；输出 各卡共享。
- 行为: 开局 gold=100；击杀/波奖励入账；买塔/融合手续出账；无利息。
- 验收句: 开局 gold==100；无任何"利息"产生额外金币的卡。

### ST1 · 双速节奏相位（state）
- 接口: 读波状态；输出 PhaseState{Intermission,Combat}。
- 行为: 波开始→Combat（锁建造/锁开始）；波结束→Intermission（开建造菜单/开「开始下一波」）。
- 验收句: Combat 时放置系统被 gate 关闭；Intermission 时开启；相位切换与波边界一致。

### UI1 · 商店 + 放置 UI（ui）—— ✅ 已实现（鼠标）
- 接口: 读金币/塔表；输出 选择塔→放置。
- 行为: 底部 bevy_ui 商店栏按钮（每座已拥有基础塔一颗），点它进入建造模式；左键点塔位放置（仅经营期）。
- 实现（2026-09-03）：`systems/pointer.rs`（`spawn_shop_bar`/`handle_shop_buttons`/`mouse_input`）+ 世界坐标拾取（`Camera::world_to_viewport` 屏幕投影→最近塔位/塔）。左键点塔=选塔，再点另一塔配方匹配即融合；右键取消。
- 验收句: 点商店按钮→建造模式；左键点空格→扣钱占格；金不足/占格→不放置。已按鼠标为主、键盘为辅实现。
- ⚠️ 占位/后续：~~商店栏在 Startup 按初始手牌生成~~ 已改为按 Hand/ShopOffers 变化自动重建（AC2）；塔位悬停高亮未做。

### UI4 · HUD（ui）
- 接口: 读 Economy/波/基地；输出 顶部/底部文字。
- 行为: 实时显示 gold / 基地血 / 当前波 / 塔数。
- 验收句: 文字 value 与 Economy.gold、基地 hp、当前波索引同步。

---

## ★ 核心乐趣（首条闭合后叠加）

### TO5 · 融合系统（towers）—— ✅ 已实现
- 接口: 输入 2 塔（场上、匹配配方）+ recipe 表 + economy；输出 结果塔 + 腾 1 塔位 + 扣手续费。
- 行为: 同类型同等级 ×2 或配方跨类型 → 高级塔（只能融合获得）；手续 = 原料总造价 20%。**原料须在场上，各占 1 格 → 结果占 1 格，净腾 1 格。**
- 实现（2026-09-03）：`F` 自动融合第一对匹配塔（`src/systems/input.rs::fuse_towers`）；`FusionDefs` 4 配方（`resources.rs`）；结果塔新机制见 `components.rs` `TowerKind`/`FusionKind`。
- 验收句: 融合 2 弓箭手 → 神射手（damage=18, attack_speed=1.2），扣手续费 0.2×(50+50)=20；场上塔数 2→1（净腾 1 格）。已入 `tests/behavior.rs`。
- ✅ **已按 §7.2 铁律校准（2026-09-03）**：原 §7.3 的 大法师/魔弓手/壁垒炮 DPS 低于"两原塔之和 90-110%"，已上调到位——
  神射手 21.6（108%）、大法师 15.0（100%）、魔弓手 16.2（93%）、壁垒炮 14.0（98%）。这些是**临时起点值**，按 §14 速查表/balance 卡/试玩随时再调。

### TO6 · 减速效果（towers）—— ✅ 已实现（壁垒炮）
- 行为: 命中的敌人 30% 减速（壁垒炮 slow_factor 0.7，持续 1s）；`Slow` 组件计时；`tick_slow` 到期移除。
- 验收句: 受击后敌速度变为 0.7×base，1s 后恢复（`src/systems/tower.rs`、`enemy.rs`）。

### EN4 · 物理护甲（enemies）—— ✅ 已实现（2026-09-03 落地，2026-09-04 钉回归）
- 接口: 输入 敌甲 + 伤害类型；输出 实际伤害。
- 行为: 盾兵怪对物理伤害 -50%，魔法伤害正常。
- 实现: `src/systems/tower.rs::effective_damage`（一维护甲：物理减半，魔法/混合穿甲）；`Enemy.physical_armor` 数据位。
- 验收句: 盾兵怪(hp90) 受 10 物理 → 扣 5（hp 85）；受 15 魔法 → 扣 15。已入 `tests/behavior.rs`
  （`physical_damage_halved_vs_armored_enemy` / `magic_damage_ignores_armor`）。

### EN5 · 治疗怪回血（enemies）—— ✅ 已实现（2026-09-04）
- 接口: 输入 Healer + 周围敌；输出 敌方 hp 增加。
- 行为: 治疗怪周期给周围敌回血。
- 实现: `Healer` 组件（`components.rs`）+ `heal_aura` 系统（`systems/enemy.rs`，挂 `GameSet::Simulate`）；
  `spawn_enemy` 按 `def.healer` 挂组件，治疗怪绿色可视化（观察通道）。**起点值（requirements 未给数，balance 卡可调）**：
  半径 6 / 每 1s 一跳 / 每跳 +3 hp，不过量治疗（clamp 到 max_hp），不治疗自己、不治疗尸体。
- 验收句: 治疗怪存活时，周围残血敌 hp 回升（10→13 一跳）；治疗怪移除后停止。已入 `tests/behavior.rs`
  （`healer_restores_nearby_enemies_and_stops_when_gone`）。

### AC1 · 开局手牌（acquisition）—— ✅ 已实现（2026-09-04）
- 接口: 输入 塔池；输出 随机 2 座基础塔（≥1 座输出塔）。
- 行为: 开局发 2 张；保底 ≥1 输出（弓箭手/炮塔）。
- 实现: `RunRng` 可种子化随机资源（`resources.rs`，对齐依赖树 rand 0.10，不引入第二版本）+ `systems/acquisition.rs::deal_opening_hand`
  （Startup 发牌，Fisher-Yates 抽 2 张互不重复，无输出时第二张换成随机输出塔）；新 `plugins/acquisition.rs`（acquisition 域，
  `Hand`/`RunRng` 资源归此插件）；商店栏 UI 在发牌之后构建（Startup 显式 after）。
- 验收句: 多次开局（32 个种子全量扫描），每次手牌都含 ≥1 座 {弓箭手,炮塔}，且共 2 座互不重复。已入 `tests/behavior.rs`
  （`opening_hand_has_two_distinct_towers_with_output_guarantee`）。

### AC2 · 商店保底（acquisition）—— ✅ 已实现（2026-09-04）
- 接口: 输入 玩家已有基础塔；输出 每波商店刷新，保底 1 种未拥有基础塔。
- 行为: 波次间刷新；保证玩家未拥有的基础塔至少出现 1 种。
- 实现: `ShopOffers{offers,version}` 资源 + `systems/acquisition.rs`（`refresh_shop_offers` 核心 / `setup_shop_offers`
  Startup 首刷 / `refresh_shop_on_intermission` 挂 `GameSet::Observe`，波次清完回 Intermission 即刷新）；
  UI 侧商店栏改为**按 Hand/ShopOffers 变化自动重建**（`pointer.rs::refresh_shop_bar`，缓存 key 防每帧重建），
  新增第二行「商店」报价按钮（`OfferButton`，点击=花塔价解锁该类型进手牌；之后每次放置仍按 TO2 扣建造费——
  解锁费与建造费分离是起点设计，balance 卡可调）。
- 验收句: 玩家未拥有弓箭手时，商店刷新后必含弓箭手；已全拥有则无保底约束。已入 `tests/behavior.rs`
  （`shop_refresh_guarantees_unowned_type` 32 种子扫描 / `wave_resolve_refreshes_shop_offer`）。

### AC3 · 掉落塔牌（acquisition）—— ✅ 已实现（2026-09-04）
- 接口: 输入 击杀事件；输出 掉落塔牌（精英/BOSS 必掉；普通波 10%）。
- 行为: 击杀精英/BOSS 必掉 1 张；普通击杀按 10% 概率掉；掉落池=全塔池。
- 实现: `Enemy.def_index` 数据位 + `systems/acquisition.rs::roll_drops`（挂 `GameSet::Cleanup`、显式先于
  `resolve_death`，直接读 hp≤0 尸体，无需消息）；走 `RunRng` 可种子化。
- ⚠️ **需求冲突待拍板**：requirements §8 写「掉落池=全塔池（含高级塔）」，但 §7.1 明确「融合结果只能融合获得，
  商店不卖、不掉落」——两条矛盾。现按更具体的 §7.1 执行：掉落池=4 基础塔；且掉落**优先补未拥有类型**
  （牌池有空缺时掉落永不浪费，全拥有后才可能重复）。请设计负责人裁决后改文档。
- 验收句: 击杀精英必出 1 张塔牌（8 种子全过）；普通击杀 200 次掉落在 ≈20 次（±3σ 带）。已入 `tests/behavior.rs`
  （`elite_kill_always_drops_tower_card` / `normal_kills_drop_about_ten_percent`）。

### AC4 · 三选一（acquisition/ui）—— ✅ 已实现（2026-09-06 随机化 + 加成扩展）
- 接口: 输入 波结算 + 已有塔 + `RunRng`；输出 3 个选项（塔强化[伤害/攻速/射程]/金币/得塔），避重避已有。选择挂起时禁开下一波。
- 行为: 每波结算用 `RunRng` **真随机**从 5 类收益（伤害+20% / 攻速+20% / 射程+15% / 击杀金币+20% / 获得塔）中抽 **3 个互不重复类别**；stat/get 的目标塔型随机抽；「得塔」避开玩家已有塔（有可用塔型时）；`1/2/3` 或点击卡片选择（UI2）。
- 实现（09-03 首版 → 09-06 polish）：
  - 09-03 占位：`wave.rs::generate_choices` 确定性生成，仅「某塔型 +20% 伤害」。
  - 09-06 polish：`generate_choices` 改用 `RunRng` 抽 3 个不重复类别（`sample_distinct`）；`ChoiceKind::StatBoost` 增 `stat: StatKind{Damage|AttackSpeed|Range}`；`Boosts` 增 `attack_speed_mult`/`range_mult`（指数=塔型）；`tower_fire` 对攻速（冷却 = 1/(攻速×mult)）与射程（瞄准 `d > range×mult`）生效；**融合塔也按 `tower_index`(=原料0塔型) 吃对应加成**（此前融合塔不吃）；中文标签沿用 UI2。
- 验收句（已入 `tests/behavior.rs`）:
  - 三选一选项类别互不重复（多种子跑不违反）；「得塔」选项不含已拥有塔型。
  - `apply_choice` 对伤害/攻速/射程 StatBoost 正确设置 `damage_mult`/`attack_speed_mult`/`range_mult`。
  - 融合塔吃 `tower_index` 加成（伤害/攻速/射程生效）。
  - 既有键盘/按钮选择路径与 UI2 回归不受影响（23→N 断言全绿）。
- ⚠️ 设计取值：+20% 伤害 / +20% 攻速 / +15% 射程 / +20% 金币 / 获得塔 —— 各为体验量，balance 卡/试玩再调；「结果塔加成」按原料0映射，非每塔独立（细化也是 polish 候选）。

### BAL1 · balance 读数工具（headless 观察通道）—— ✅ 已实现（2026-09-06）
- 目标：给「设计目标锚点框架」提供 headless 读数（游戏行为必须可被日志/测试观察——ADR-0002）。
  让 AI/人用 `cargo test --test balance -- --nocapture` 直接读出**每波清理秒数 / 漏怪数 / 金币增益 / 基地血量**，对着 §14 目标区间/试玩反馈收敛数值。
- 实现：新增 `tests/balance.rs`（独立于 behavior.rs）：`test_app()`（同 behavior 的 headless 组装）+ `place_loadout`（按预算在塔位放塔）+ `run_readout`（按 Space 开波 → 跑到 wave_resolve → 采样本，逐波记录）。
- 观察口径：本工具模拟**真实运行**（含移速 §9、护甲 EN4、AOE TO6、攻速/射程 AC4 加成），非静态 DPS 推算；波间三选一默认跳过（基线读数）。
- 已验证读数（loadout 2弓+1法+1炮，种子 1234）：波1 清 4.05s/0漏；波2 5 8.60s/0漏；波3 10普通+3快 **22.02s/漏4** → 基地 6/10，gold 221。
- **首个平衡信号**：第三波是难度尖峰，常规阵容都漏 4 → balance 卡优先收敛第 3 波前后（或给第 3 波前更多经济/更强塔）。
- 回归边界：本卡只守卫 harness（波能结算、读数 sane）；**balance 目标区间断言在目标值定稿后再加**（锚点 5 字段表）。

### BAL2 · balance 锚点表（把 §15 定性信号转成可测指标）—— 🟡 已起草，待设计负责人定目标区间
- 现状基线（BAL1 全波读数，自动布阵从 100g 起步）：**第 2 波即 GameOver（基地 0/10）**；波1 5普通 21.5s 漏4，波2 8普通 0s 漏6。→ **早战/经济压力过高**是首要问题。
  （注：这是"贪心自动布阵"的代理结果，非人类精明运营判定；真实难度仍需 §15 真人试玩确认。）
- 锚点 5 字段表（**目标区间全部为提案，待设计负责人确认/调**；确认后我把它转成 BAL2 回归断言）：

| 类别 | 指标 | 目标区间（提案） | 来源 | 验收方式 |
|------|------|-----------------|------|----------|
| 难度曲线 | 自动布阵存活波数 | ≥4（从 100g 起步，现测=2） | §15「决策摆烂」/§14 | BAL1 全波读数+断言 |
| 波次难度 | 前 3 波漏怪数 | ≤2（现测波2漏6） | §14「更简单/更难」 | BAL1 per-wave leaks |
| 经济 | 波间留有余钱 | 每波结算后有能买 1 塔的金（防"没钱防守"） | §11 | BAL1 gold 曲线 |
| 三选一 | 选项无纯垃圾项 | 三种选择期望收益可比 | §12 | 录选项+效果 |
| 融合 | 融合塔 DPS | 原塔和 90-110%（§7.2 铁律已达标） | §7.2 | 塔表校验 |
| 技能 | 伤害贡献占比 | ≤ 全场总输出 20%（2026-09-14 定稿） | §17.1 | BAL1 读数扩展（技能伤害统计） |

- 待办：设计负责人定区间 → AI 把上表转断言（BAL2 回归）→ 用 §14 起点值收敛（先解决早战压力：加大开局经济/降第 1-3 波强度/增强盾塔射程等）。
- ⚠️ **数值收敛暂缓**（2026-09-06 负责人意见：数值等有时间再调）。锚点表+读数工具已就绪，随时可恢复；期间先推进非数值项（UI polish）。

### UI2 · 操作面板化 + 中文化（ui）—— ✅ 已实现（2026-09-06）
- 接口: 读 WaveState/WaveChoice/FusionSel/Hand/ShopOffers/Economy；输出 bevy_ui 控件 + 中文文案。
- 行为（对齐主流塔防操作惯例，目标：纯鼠标可玩一局）:
  ① 波次间右下角「▶ 开始下一波」按钮（三选一挂起/战斗期隐藏；与空格键等效）；
  ② 三选一挂起时屏幕中上弹 3 张**可点击选项卡片**（与 1/2/3 键等效）；
  ③ 商店栏两行卡片**钱不够时文字置灰**（金币变化即刷新）；
  ④ 左键点已放塔 → 右上**属性面板**（塔名/伤害类型/攻速/射程/特技/融合提示）+ 地面半透明**射程圈**；右键或取消选中即关；
  ⑤ **UI 文案默认中文**：TowerDef/FusionDef 增加中文 `label` 字段（弓箭手塔/盾塔/法师塔/炮塔/神射手塔/大法师塔/魔弓手塔/壁垒炮塔）、三选一选项中文、HUD 状态行中文（金币/基地/波次/建造期/战斗期）与结算提示；代码标识符与日志仍英文（团队规则）；「语言选项切换」入 GDD 点子池；
  ⑥ **中文渲染**：Cargo 加 `bevy_text/system_font_discovery` 特性，`FontSource::Family("Microsoft YaHei")` 运行时解析系统微软雅黑（parley fontique 自动回退），不带字体文件、无版权问题；分发时自带开源中文字体留点子池；
  ⑦ 黄色键盘光标方块默认隐藏（鼠标流程已取代其视觉职责；CursorMove 卡逻辑与回归测试原样保留）。
- 验收句（已入 `tests/behavior.rs`）:
  - `start_wave_button_click_starts_wave`：Intermission 点「开始下一波」→ 进入 Combat 且第 1 波 spawn_queue=5 条；三选一挂起时点击无效（仍在 Intermission）。
  - `choice_card_click_applies_option_and_closes`：挂起时点第 2 张卡 → kill_mult=1.2 且 pending=false（与数字键等效）。
  - 既有键盘路径回归全绿。人工验收项：中文正常渲染、射程圈跟随选中塔、置灰状态正确。

### UI3 · 塔位悬停高亮 + 放置幽灵预览（ui）—— ✅ 已实现（2026-09-06）
- 接口: 读 BuildMode/SelectedTower/TowerDefs/WaveState/GameState + 光标/相机；输出一个可复用的半透明幽灵塔实体（HoverGhost）。
- 行为: 建造模式（点了底部卡片）且处于建造期时，光标悬停到**空塔位**上 → 该塔位显示一座**半透明幽灵预览塔**（颜色随所选塔型：物理灰/魔法紫/混合绿），玩家先看到「往哪放、放什么」；悬停到已占格或非建造期 → 幽灵隐藏。
- 实现(2026-09-06)：`pointer::update_hover_ghost`（Observe 集合）：复用单实体（Local 缓存 `(塔型, entity)`），仅当所选塔型变化才重生成（颜色同步）；用 `nearest_slot_screen` 找光标最近塔位，仅在空位显示。新标记组件 `HoverGhost`（+网格+半透明 blend 材质）。
- 验收句（已入 `tests/behavior.rs`）：`hover_ghost_stays_absent_without_cursor` —— 无窗口/光标 headless 下幽灵不出现且不 panic，解除建造后仍不出现（不变量钉死）。
- ⚠️ 视觉正确性（幽灵随光标移动、塔位高亮的呈现）属**人工验收项**（需真窗口），HEADLESS 只锁「不可凭空出现/不 panic」不变量。

### UI5 · 结算画面（GameOver/Win）—— ✅ 已实现（2026-09-06）
- 接口: 读 State<GameState>；输出居中结算面板（EndScreenRoot，含重试 + 两个 Meta 购买按钮）。
- 行为: 仅 GameOver/Win 显示「🏁 本局结束」横幅 + 提示 + 三个可点按钮（升级1 开局必含弓箭手塔(30币)/升级2 开局+20金(60币)/再来一局）；Playing/Paused 隐藏。按钮与键盘等效（1/2 买 Meta、P/按钮 重开）。
- 实现(2026-09-06)：`pointer::build_end_screen`（面板，Display::None 起始）+ `refresh_end_screen`（Observe，按 State 切换 Display:Flex/None）；`handle_retry_button`/`handle_meta_upgrade_buttons` 注册于 ui.rs Update（**不 gate 到 Playing**，结算屏也能点）；`) meta::buy_upgrade` 抽成共享函数供数字键与按钮双入口（复用 pattern）。
- 验收句（已入 `tests/behavior.rs`）：`end_screen_shows_only_on_terminal_state`（GameOver 显示 Flex / Playing 隐藏 None）、`retry_button_restarts_from_game_over`（点按钮回到 Playing）、`meta_upgrade_button_grants_purchase`（点升级1按钮 upgrade1=true 且 coins-30）。
- ⚠️ 视觉/排版的呈现效果属人工验收项；HEADLESS 锁「状态驱动显隐 + 按钮行为」回归。测试用唯一临时存档路径（避免共享 meta 文件污染——曾踩共享 `warden_meta_shared_test.txt` 被 `load_meta` 误读的坑）。

### UI6 · 自带开源中文字体（ui）—— ✅ 已实现（2026-09-11）
- 接口: `assets/fonts/NotoSansSC-VariableFont_wght.ttf`（SIL OFL 1.1，附 `OFL.txt`）经 AssetServer 加载；
  输出 `UiFont` 资源（`FontSource::Handle`），全部 UI 文本统一引用；headless（无 AssetServer）回落 `FontSource::Default`。
- 行为: UI 文本不再依赖系统「微软雅黑」（Windows 专属）或平台回退（macOS 萍方），换机器/出包分发渲染一致；
  移除 `bevy_text/system_font_discovery` 依赖（不再查系统字体）。
- 验收句（已入 `tests/behavior.rs`）: ① `bundled_cjk_font_file_is_present`（字体+许可文件在库，防误删）；
  ② `ui_font_falls_back_to_default_without_asset_server`（headless 不 panic、回落默认字体）。
- 人工验收项: 真窗口中文渲染正常（2026-09-11 macOS arm64 实测通过）。分发体积优化（子集化）留点子池。
- 备注: 字体先放项目内（首次使用）；第 2 个游戏用到再沉根 `assets/`（团队 rule of three）。

### UI7 · 语言切换 中/英（ui）—— ✅ 已实现（2026-09-11）
- 接口: 新 `Lang` 资源（Zh/En，默认 Zh）+ `ui_prefs.txt` 持久化（key=value，同 meta_save 约定）；
  TowerDef/FusionDef 增 `label_en`，EnemyDef 增 `label`/`label_en`；全部玩家文案经 `systems/l10n.rs`（`Bi(zh,en)` 双语常量）。
- 行为: 按 `L` 或点右上角语言按钮 → HUD/帮助行/商店卡/三选一卡/属性面板/结算屏全部在中/英间切换并即时重建；
  重启记住选择；代码标识符与日志保持英文（团队规则）。
- 验收句（已入 `tests/behavior.rs`）: ① `lang_toggle_switches_hud_and_shop_copy`（切 En 后 HUD 含 "Gold="、商店卡含
  "Archer"，切回含 "金币"）；② `lang_preference_persists_across_restart`（ui_prefs.txt 落盘+新 App 读回）。

### UI8 · 下一波组成预览（ui）—— ✅ 已实现（2026-09-11）
- 接口: 读 WaveSchedule/WaveState/WavePhase/GameState；HUD 状态行追加「下一波」段（数据来自 requirements §10，纯展示）。
- 行为: 建造期（Intermission + Playing）显示下一波敌人构成与奖励（如「下一波：普通×10 快速×3 · 奖励30金」），
  服务 §3「慢思考」——玩家带着信息做经营决策；战斗期不显示；无下一波（Win 态）不显示。
- 验收句（已入 `tests/behavior.rs`）: ① 初始建造期 HUD 含「下一波」且列出第 1 波构成（普通×5）；② 开波后该段消失。

### ME1 · Meta 解锁链（meta）—— ✅ 已实现（2026-09-04）
- 接口: 读死亡/通关；输出 meta 币 + 升级效果。
- 行为: 死亡 +15、通关 +30；升级1=弓箭手进开局手牌池；升级2=开局金币 +20。
- 实现: 新 `plugins/meta.rs` 域（卡片路线书预告的 meta 插件）+ `systems/meta.rs`（load/apply Startup 链、
  `OnEnter(GameOver/Win)` 发币、GameOver/Win 屏 1/2 键购买）+ `MetaState`/`MetaSavePath` 资源。
  **持久化=极简 key=value 文本**（`meta_save.txt`，无 serde 依赖；缺失/损坏回落默认值）。
  **起点值（requirements 未给数）**：升级1=30 币、升级2=60 币（发币节奏 15/次死亡 → 约 2~4 局解锁一个，可调）。
  **升级1 语义取「开局手牌必含弓箭手」**（§13 原文「弓箭手进开局手牌池」有歧义，取更强解释，请设计负责人复核）。
  升级1 生效点=发牌（deal 显式 after meta apply）；HUD 显示 meta 币与购买提示。
- 验收句: 死亡一次 meta 币+15（含落盘断言）；解锁升级2后下一局开局 gold==120。已入 `tests/behavior.rs`
  （`death_awards_meta_coins_and_persists` / `upgrade2_boosts_next_run_starting_gold`）。

---

## 交互范式重做（War3 式命令卡，2026-09-12 需求定稿）

> 设计依据 requirements §16（交互规范）。一个随选中态切换内容的常驻三区底栏，统一建造/卖出/融合，预留技能格。
> 实现顺序：**UI9 → UI10 → UI11（含 TO7）→ UI12**；UI1/UI2 的底部商店栏与右上信息面板由 UI9/UI10 收编退役，
> 相关回归断言随卡迁移。卖塔/融合落位等数值以 requirements §16.5/§16.6 为准。

### TO7 · 卖塔（towers/economy）—— ✅ 已实现（2026-09-12）
- 接口: 输入 已放塔实体（基础或融合）+ WavePhase(Intermission)；输出 Economy.gold += 回收、塔 despawn、塔位腾空。
- 行为: 仅波次间可卖（战斗期/融合目标选择期拒绝）；回收 = 金币总投入×70% 向下取整
  （基础塔=建造费；融合塔=原料建造费之和+融合手续费，requirements §16.5）。
- 验收句: 建弓箭手(-50)后卖出 → gold +35、塔位空、塔 despawn；神射手（投入 50+50+20=120）卖出 → +84；
  战斗期卖出被拒（gold 不变、塔仍在）。

### UI9 · 命令卡骨架 + 选中态状态机（ui）—— ✅ 已实现（2026-09-12）
- 接口: 读 Selection（无选中/空塔位/已放塔/融合目标中）+ WavePhase + GameState；输出 三区常驻底栏（信息区/命令卡/控制区）。
- 行为: 替代「底部两行商店栏 + 右上信息面板」（五态表见 requirements §16.2）；左键选择、左键空白取消、
  右键取消模式、**Esc 统一取消**；控制区收编「开始下一波」+ 暂停 + 语言按钮 + 2x 快进置灰占位（快进本身另立卡）。
- 验收句: 点空塔位 → 信息区为塔位信息、命令卡为建造列表；点已放塔 → 命令卡出现卖出/融合按钮；
  Esc 在建造模式/选中态/融合目标三处均清空选择；战斗期建造/卖出/融合按钮置灰（可见不可点）；
  GameOver/Win 底栏隐藏（结算屏独立）。headless 断言态切换与置灰；排版视觉人工验收。
- **修复（2026-09-12 真人试玩）**：UI 点击泄漏进世界拾取——点命令按钮的同一次左键也进入
  `select_object`，其「点空地取消」同帧清空 Selection，**融合永远无法从底栏进入**（armed 建造同源：
  点底栏会在光标最近塔位误放塔）。修法：底栏/三选一/结算屏根节点挂 `BlocksWorldInput` +
  `RelativeCursorPosition`（`ui_focus_system` 维护），`select_object`/`mouse_place_system` 入口
  统一吞掉悬停 UI 的点击。新增 4 条 live 光标路径回归（headless 真 Window+相机投影驱动，此前
  无窗口导致该链路从不运行）：融合按钮点击存活、真实点 B 确认融合全链路、armed 底栏点击不误放、
  空白世界点击仍取消。

### UI10 · 建造命令迁移 + 双入口（ui）—— ✅ 已实现（2026-09-12）
- 接口: 读 Hand/ShopOffers/Economy/Selection；输出 命令卡建造按钮（手牌行+商店行）。
- 行为: 待机态点塔按钮 → armed 建造（UI1 鼠标流沿用）→ 点空位放置；选中空塔位时同一批按钮点击=直接建在该位，
  悬停按钮 → 该位幽灵塔+对应塔型射程圈实时预览（UI3 幽灵扩展）；商店报价按钮在任一态=解锁购买（AC2 语义不变）。
  旧 ShopBarRoot 两行栏与 WASD/1-4/E 键盘遗留流退役（UI1 键盘规格作废；`cursor.rs`/`input.rs` 旧路径与对应断言清理）。
- 验收句: 待机态 点塔按钮→点空位 放置成功（与 UI1 鼠标流等效）；选中空塔位后点「弓箭手」→ 直接建在该位；
  悬停「法师塔」按钮时该位出现法师幽灵+射程圈、切悬停「炮塔」射程圈随之变化；
  AC2 商店保底断言迁移后全绿；旧键盘放置路径断言移除后 `cargo test` 全绿。

### UI11 · 选中塔命令卡：卖出 + 融合目标选择（ui/towers）—— ✅ 已实现（2026-09-12）
- 接口: 读 选中塔 + FusionDefs + Economy + 场上塔集合；输出 命令卡卖出/融合按钮 + 可配对塔高亮 + 融合执行。
- 行为: 选中基础塔 → 「融合」按钮（无可配对置灰）；点击进入目标选择（高亮所有可配对塔、信息区显示配方+手续费）；
  点 B 确认 → 按 requirements §16.6 执行：扣 20% 手续费、**结果塔落 B 位、A 位腾空**（净腾 1 格不变）；
  融合塔选中时只显示卖出（融合塔不可再融）。旧「点 A 点 B 即融合」与 F 键自动融合退役（TO5 触发方式重做，配方不动）。
- 验收句: 弓箭手A+弓箭手B 在场 → 选中A→点融合→B 高亮→点B → 神射手生成于 B 位、A 位空、扣 20 手续费；
  选中塔后点另一塔**仅切换选中、不触发融合**；场上无可配对塔时融合按钮置灰；融合塔选中无融合按钮。

### UI12 · Tooltip + Grid 热键（ui）—— ✅ 已实现（2026-09-12）
- 接口: 读 命令卡按钮 hover + TowerDefs/FusionDefs/Lang；输出 tooltip 面板 + 按钮热键角标 + Grid 键位映射。
- 行为: 悬停命令卡任一按钮 → tooltip（名称/热键/价格/完整属性/特效/一句话定位，中英双语，requirements §16.8）；
  网格热键 QWER/ASDF 位置映射、角标印字母；`1-4/E/F/WASD` 退役，`1/2/3/Space/P/L` 不变（§16.7）。
- 验收句: 悬停「法师塔」按钮出现 tooltip 且含伤害/攻速/射程/魔法 AOE 字样（中英各断言一次）；
  待机态按 `Q` 等效点击第一格手牌按钮；融合目标选择中按 Esc 取消；旧数字键选塔断言移除后全绿。

---

## 战斗期攻击表现与投射物（2026-09-12 需求定稿）

> 设计依据 requirements §5.1（攻击结算模型，真投射物）。伤害从即时结算升格为**弹体飞行+到达结算**，
> 弹体做成通用实体为技能系统复用留接缝；血条/受击/状态表现并行补齐战斗期可读性。
> 实施顺序在交互范式四卡之后：**UI9-12 → PR1 → PR2 → FX1 → FX2 → FX3 → 真人试玩**。
> 已知影响：有效 DPS 因飞行时间与 overkill 会下降属预期，落地后只重取 BAL1 基线，数值仍暂缓。

### PR1 · 投射物系统骨架（projectiles）—— ✅ 已实现（2026-09-12）
- 接口: 输入 远程塔开火（目标实体 + 伤害快照/攻击类型/AOE/减速载荷）+ 弹速；输出 Projectile 实体 + 命中结算。
- 行为: 远程塔（弓箭手/法师/炮塔/全部融合塔）开火改为**发射追踪弹**（锁定目标、每帧飞向其当前位置、弹速 10、
  必中不脱靶，requirements §5.1）；到达即结算——单体只伤目标、AOE 以命中点为爆心（护甲逐个结算）、
  减速载荷命中附加；伤害数值与类型发射时定格，护甲命中时结算。**盾兵塔（近战）保留即时命中**（混合模型）。
  塔冷却发弹即重置。弹体为可见 3D 纯色实体（箭=灰白细长、魔法弹=紫球、炮弹=橙球）。
  本卡=TO3 伤害结算位置重做（tower_fire 只发弹，数值不变）+ TO6 触发方式重做（全射程刷新→命中附加，数值不变）。
- 验收句: 弓箭手锁定 8 单位外普通怪开火 → 生成 1 个弹体实体，≈0.8s（±10%）后命中、hp 30→20；
  盾兵怪受物理弹伤害减半；盾兵塔开火无弹体生成、目标即时扣血；弹体追踪移动目标（命中时与目标距离<0.5）。

### PR2 · 投射物边缘行为（projectiles）—— ✅ 已实现（2026-09-12，与 PR1 同批：骨架与边缘行为在同一系统内天然耦合）
- 接口: 输入 弹体目标存活状态 + 波次结算；输出 消散 / 爆心落地 / 清场。
- 行为: 单体弹飞行中目标死亡或被移除 → 弹体消散（无伤害，overkill 浪费是真实战术成本）；
  AOE 弹同情况 → 在目标最后位置照常爆炸；波次结算时场上剩余弹体全部移除（requirements §5.1）。
- 验收句: 单体弹目标飞行中死亡 → 弹体 despawn 且无任何敌人掉血；炮塔弹同情况 → 最后位置爆炸、半径内敌受伤；
  波次结算帧后场上弹体数==0。

### FX1 · 敌人血条（fx）—— ✅ 已实现（2026-09-12）
- 接口: 读 Enemy hp/max_hp；输出 敌头顶血条实体。
- 行为: **受伤才显示**（满血隐藏，Defender's Quest 惯例，防满屏条）；受伤后宽度==hp/max_hp；
  治疗回满 → 隐藏；精英/BOSS 血条更醒目（尺寸/颜色区分）。
- 验收句: 满血敌无血条实体；受伤后血条存在且比例==hp/max_hp（采样断言）；治疗回满后血条隐藏。

### FX2 · 命中与死亡反馈（fx）—— ✅ 已实现（2026-09-12）
- 接口: 读 命中/死亡时机；输出 受击闪白、AOE 爆心扩散圈、死亡消散壳。
- 行为: 受击敌材质闪白一瞬（<0.15s 恢复原色）；AOE 命中点生成扩散圈（<0.5s despawn）；
  敌死亡**逻辑结算照旧**（金币/despawn 时序零改动），同时生成纯视觉「消散壳」实体（缩小淡出 ≈0.4s 自动移除）。
- 验收句: 受击敌 0.15s 内出现受击标记随后恢复；AOE 命中点生成扩散圈且 <0.5s 后消失；
  敌死亡时生成消散壳、≈0.4s 后自动 despawn，且击杀金币断言（普通怪 +6）不受影响。

### FX3 · 状态可视化（fx）—— ✅ 已实现（2026-09-12）
- 接口: 读 Slow 组件 + EnemyDef；输出 减速色调、护甲标识。
- 行为: 减速中的敌变蓝色调（Slow 存续期间保持，到期恢复原色）；物理护甲怪（盾兵怪）自生成起带可区分的盾标识；
  治疗怪维持现有绿色。
- 验收句: 被壁垒炮击中的敌 1s 内为蓝色系色调（断言颜色分量）、Slow 到期后恢复原色；
  盾兵怪自生成起带盾标识（与普通怪可区分，断言标识组件存在）。

---

## 测试便利化（调试面板，2026-09-12 需求定稿）

> 目标：消灭「测试要等很久」——等波次走完、凑前置状态费手、经济没法乱试、瞬态行为看不清。
> 全部为**开发侧工具，玩家不可见**（不动 requirements）：`#[cfg(debug_assertions)]` 门控，发布构建零调试面；
> 调试局（用过作弊/场景直达的局）不发 Meta 币、不写 meta_save.txt（防测试污染真存档，UI5 有过共享存档误读的坑）。
> 插队拍板（2026-09-12）：**DB1 → DB2 → 命令卡 UI9-12 → DB3 → PR1/PR2 → FX1-3 → 真人试玩**。
> 不引 bevy-inspector-egui（版本对齐未核实，手写 egui 面板够用）；bevy_egui 选型 ADR-0008 已覆盖。

### DB1 · 调试面板 + 作弊核心（debug）—— ✅ 已实现（2026-09-12）
- 接口: F1 开关（egui 窗口，仅 debug 构建）；读/写 Economy/基地血/WaveState/Hand/GameState；输出 作弊操作与调试局标记。
- 行为: `#[cfg(debug_assertions)]` 门控。面板项：①经济：金币 +100/-100/设定、基地血设定；②波次：跳到第 N 波建造期
  （中间波次奖励自动补发）、立刻清完当前波、入口召唤指定敌人 ×N；③获取：解锁全部塔型、手牌塞任意塔；
  ④结局：立即失败/立即胜利；⑤快速重开：一键重开本局（配合 `--seed N` 复现随机序列）；⑥现有 DebugPlugin
  （F12 截图/日志仪表）一并纳入同一门控收口。调试面板文案英文（开发工具不走 l10n）。
  **调试局存档隔离**：本局用过任何作弊项 → 结算不发 Meta 币、不写 `meta_save.txt`。
- 验收句: F1 开面板点「跳到第 5 波」→ 1s 内处于第 5 波建造期且中间波次奖励已入账；「+100 金」当帧生效；
  作弊局结算后 `meta_save.txt` 内容不变（headless 断言）；正常局 Meta 结算回归不受影响；
  `cargo build --release` 构建无调试面板/作弊/场景代码路径（cfg 抽除，用编译级手段断言或检查脚本说明）。
- 实现: `systems/debug.rs`（全模块 cfg 门控）= `DebugAction` 意图队列 + **单一 drain 系统**
  `debug_cheat_executor`（GameSet::Input，处理动作时置 `DebugRun.marked`）；egui 面板（`plugins/debug.rs`，
  bevy_egui 0.42，面板按钮只推 action 不做逻辑——headless 测试推同一队列即同一代码路径）。
  存档隔离 = `award_on_death/award_on_win` 读 `DebugRun` 提前返回；`DebugPanelCapture`（pointer/keyboard）
  让面板悬停/输入吞掉世界拾取与热键（复用 UI 点击泄漏修法的同一判定口）。**关键取舍**：cargo 无法按 profile
  门控依赖 → bevy_egui 无条件声明、门控全在代码层（任何未门控引用直接让 release 编译失败——实测抓到过
  9 处）；`DebugRun`/`DebugPanelCapture` 资源本身不门控（release 恒 false，守卫系统可无条件读）。
  **release 零调试面断言手段**：编译级（模块 cfg 抽除 + `cargo build --release` 必须过）+ 二进制级
  （`scripts/check_release_debug_surface.sh`：构建 release 后 grep 独有调试字符串，命中即失败）。
  验收句已入 `tests/behavior.rs`（`debug_cheats::*` 8 条：跳波+奖励补发/当帧金币/存档内容逐字节不变/
  正常局回归/清波走正常结算/召唤/结局/种子重开/F1 开关）。

### DB2 · 测试场景直达（debug）—— ✅ 已实现（2026-09-12，依赖 DB1）
- 接口: 读 CLI 参数（`--scenario NAME --seed N`）或 F1 面板按钮；输出 预置场景状态 + 调试局标记。
- 行为: 预置场景一键到位：`fusion`（场上弓×2 + 金 100 + 建造期）/ `projectile`（塔 + 距 8 单位 1 普通怪）/
  `fx`（残血敌群 + 治疗怪）/ `meta`（直达 GameOver）/ `choice`（三选一挂起）/ `full-board`（8 塔满员）。
  CLI 与面板双入口等效；场景直达即标记调试局（触发 DB1 存档隔离）。
- 验收句: `cargo run -- --scenario fusion --seed 42` 启动后 ≤2s 场景断言成立（场上弓×2/金≥100/Intermission，headless 等效断言）；
  同种子两次启动手牌一致（RunRng 复现）；F1 场景按钮与 CLI 等效。
- 实现: `Scenario` 枚举（name=CLI 值=面板按钮文案）+ `DebugScenarioQueue`。**双入口同一代码路径**：CLI 在
  `main.rs`（cfg 门控）解析 `parse_cli_args`（可注入迭代器，测试直接喂参数）→ `--seed` 先于 Startup 换 `RunRng`
  （手牌可复现）+ 记录 `DebugSeed`，`--scenario` 入队；F1 面板按钮同样只入队。排他系统 `debug_scenario_executor`
  （GameSet::Input、after 作弊执行器）首帧 Startup 建好地图/槽位后统一落地：先 `clear_board`（清敌/弹/塔、槽位全空、
  波次回 1 建造期）再按场景布置（弓×2/距 8 普通怪/1hp×4+治疗/GameOver/真 `generate_choices` 三选一/8 塔循环 4 基础型）。
  choice 场景复用 wave.rs 真生成器（改 `pub(crate)`），不造假数据。headless 等效断言 8 条入 `tests/behavior.rs`
  （`debug_scenarios::*`：CLI 解析合法/非法、fusion、projectile 距离、fx、meta 存档逐字节不变、choice 三类不重复、
  full-board、同种子两手牌一致）；live ≤2s 验收即首帧落地（队列在第一帧 Update 排空）。
  release 门控断言沿用 DB1 脚本，已加场景日志字符串扫描项。

### DB3 · 时间控制 + 信息叠层（debug）—— ✅ 已实现（2026-09-14）
- 接口: 读 暂停/步进输入 + 场上实体；输出 时间倍率控制 + 调试叠层渲染。
- 行为: 暂停 + **单帧/0.1s 步进**（毫秒级验收句的核对手段）；调试 4x 加速（不进发布，与正式 2x 快进分离，
  不参与 balance 拍板）；全塔射程圈常显开关、敌头顶 hp/移速/减速态叠层、场上弹体计数。随 DB1 同一 debug 门控。
- 验收句: 暂停后按单帧键恰好推进 1 帧（敌位移==单帧 delta）；射程圈开关全场塔同步显隐；
  4x 下清一波耗时≈1x 的 1/4（±10%）。
- 实现: `systems/debug_time.rs`（新 cfg 门控模块，DB1 同一 debug 面）。**与 GameState::Paused 的共存取舍**：
  玩家暂停（P）是**状态级**（run_if 门控整个系统），调试暂停是**时间级**（冻结 `Time<Virtual>`，系统照跑但
  delta=0）——两者正交可叠加；冻结帧内选塔/开关面板等非时间操作仍可用（正是检视冻结帧所需）；步进只在
  Playing 有意义（他状态系统本就不跑）。**帧契约**：bevy 在 First 推进虚拟钟，`debug_time_clock`（Input 内、
  作弊执行器之后）写入的暂停/倍率下一帧才生效 → 单帧步进=按下帧的后一帧恰好跑 1 帧（Frames 预算按
  delta>0 的帧倒数）。热键 F6/F7/F8/F9（暂停/单帧/0.1s/4x）与 F1 面板按钮推**同一 DebugAction**，单一执行器
  生效。叠层：射程圈=每塔一枚地面圈（缓存塔集合，开关/增删塔时重建）；敌头顶 Text2d 面板（hp/移速/减速，
  出生帧即带初文本）；常显 HUD 条（右上 `[db3] time:x proj=N enemies=M`，英文）。**时间/叠层动作不标记调试局**
  （纯观察不改结果，`is_observation()` 白名单；同批真作弊仍标记）。release 门控沿用 DB1 脚本，已加 `[db3]`
  扫描串。验收句 6 条入 `tests/behavior.rs`（`debug_db3::*`：单帧步进恰好 1 帧位移/冻结-恢复、
  4x≈1/4 帧数 ±10%、射程圈随开关与增塔同步、敌面板开关与数值跟踪、HUD 弹体计数、观察动作不污染存档局）。

---

## 技能系统一期（全局指挥官技能，2026-09-14 需求定稿）

> 设计依据 requirements §17。定位：战斗期能动性——技能是波次中唯一的玩家主动手段（锁建造铁律不变）。
> 纯冷却免费、CD 真实时间连续跨波不重置、开局即就绪；伤害贡献锚点 ≤ 全场输出 20%（BAL2 已加行）。
> 实施顺序：**SK1 骨架 → SK2 火雨 → SK3 冰霜 → SK4 狂暴**；新域插件 `plugins/skills.rs`；
> 弹体复用 Projectile（发起方扩展为 塔/技能 两源），减速复用 Slow，狂暴复用 Boosts 攻速乘数管线。
> 二期（融合塔主动/随机获取/Meta 强化）与远期（被动/光环）未立卡，别实现。

### SK1 · 技能骨架 + 冷却/施放交互（skills）—— ✅ 已实现（2026-09-14）
- 接口: 新 `SkillState` 资源（每技能 CD 计时）+ 控制区技能按钮（Z/X/C 热键、角标、CD 遮罩+剩余秒数、tooltip 双语）；
  读 Time/WavePhase/GameState；输出 施放意图与目标选择模式。
- 行为: 冷却真实时间连续、跨波不重置、开局即就绪、暂停冻结；点地技能进入目标选择（地图指示圈+左键确认+Esc/右键取消，
  与融合目标选择同款范式）；瞬发技能按键即触发；无实装技能时按钮置灰（SK2-4 逐张点亮）。
  F1 调试面板加「重置技能 CD」按钮（调试操作不改规则）。
- 验收句: 开局三按钮全就绪；施放后 CD 按真实时间递减（暂停时冻结，波次切换不重置）；冷却中按 Z/X/C 无效果且按钮显示剩余秒数；
  目标选择态 Esc/右键取消且不进 CD。headless 断言 CD 计时与门控；遮罩/角标视觉人工验收。
- 实现: 新域 `plugins/skills.rs` + `systems/skills.rs`；`SkillState`/`SkillTargeting`/`SKILL_COOLDOWNS`(45/40/60) 入
  `resources.rs`。CD 计时走虚拟时间（Simulate、Playing 门控→玩家暂停冻结、DB3 调试冻结 delta=0 同样冻结、双相位均流动、
  跨波无重置）。按钮挂**控制区**（ControlZoneRoot 子节点，语言切换区重建后自愈重挂；不进 QWER 网格）；Z/X/C 热键注入
  `Interaction::Pressed` 复用点击处理器（同 UI12 范式）；冷却中/未实装按钮置灰、主标签切剩余秒数；tooltip 由
  `command::refresh_tooltip` 扩展覆盖技能按钮（缓存键扩为 Command/Skill 两源）。点地目标选择：指示圈（技能效果半径）随
  光标地面点，左键确认（避开 UI 悬停）→ 施放+进 CD，Esc/右键取消不进 CD（`cancel_modes` 扩清 SkillTargeting；
  `select_object` 目标选择期让位）。**取舍**：SK1 全部技能 `implemented=false`，「目标选择态 Esc 取消」的 headless 断言
  随 SK2（首个点地技能）落地；瞬发路径已在 handle_skill_buttons 就位（SK4 只填效果）。F1「Reset skill CDs」=作弊（标记
  调试局）。遗留修复：PR1 存量测试 gap 查询误取任意 Transform（SK 按钮节点加入后踩中），改为 With<Enemy> 过滤（语义不变）。
  `skills_sk1::*` 5 条断言入 behavior（开局就绪+三按钮/CD 真实时间递减+暂停冻结+恢复/跨波不重置/未实装热键无效/重置 CD 作弊标记）。

### SK2 · 流星火雨（伤害·点地）（skills/projectiles）—— ✅ 已实现（2026-09-14，依赖 SK1）
- 接口: 输入 确认目标点；输出 3 发 AOE 弹依次落点（Projectile：发起方=技能、伤害包 40、飞向固定落点）+ 爆心结算。
- 行为: requirements §17.2——3 发依次落下（间隔 ≈0.3s 起点值），每发对半径 3 内敌人结算 40 伤害（类型=魔法，穿物理护甲）；
  弹体复用投射物接缝（发起方扩展为塔/技能两源；技能弹飞向固定点而非追踪实体）。
- 验收句: 确认点后 3 发全部落下，半径 3 内 3 只普通怪各死一只（hp 30 < 40，金币 +6×3）；
  盾兵怪受全额 40（魔法穿甲）；施放后 CD=45s 开始计时；目标选择取消则无弹生成不进 CD。
- 实现: `Projectile.target` 扩为 `origin: ProjectileOrigin`（`Homing{target}` 塔 / `Fixed{point}` 技能——规格允许的
  接缝兑现；tower_fire/projectiles.rs/存量 PR 测试同步迁移，飞行/结算语义不变：Fixed 弹必有 AOE 半径、单发弹仅 Homing）。
  弹序调度 = 新组件 `MeteorVolley`（规则层实体，3 发计数+0.3s 间隔计时）+ `release_meteor_shots`（Simulate）：
  每发从落点上方 10 单位（速度 10≈1s 可视坠落）生成 `Projectile{Fixed, 40 魔法, aoe 3}`，命中走既有
  `resolve_impact`（护甲按受害者结算→魔法穿甲、金币走正常死亡路径、FX 爆圈复用 ProjectileImpact）。三发同落点
  （requirements 未言明散布，取最简解释）。首帧即落第一发（确认在 Input、调度在 Simulate 同帧）。SK1 的
  「Esc 取消不进 CD」断言随本卡补齐（真点地技能）。`skills_sk2::*` 3 条断言（三发全落+半径内全灭+金币 / 魔法穿甲
  90-40=50 / 取消无弹无 CD）。

### SK3 · 冰霜新星（控制·点地）（skills）—— ✅ 已实现（2026-09-14，依赖 SK1）
- 接口: 输入 确认目标点；输出 半径 5 内敌人挂 Slow{factor 0.5, duration 2s}。
- 行为: 复用 Slow 组件与到期移除（tick_slow）；与壁垒炮减速同存时**取更强者刷新**（0.5 vs 0.7 → 生效 0.5），不叠乘（§17.2）。
- 验收句: 半径 5 内 2 敌速度 ×0.5、2s 后恢复；半径外敌不受影响；已带 0.7 减速的敌被新星命中后移速按 ×0.5 生效至效果结束。
- 实现: 新星即时结算（无弹体）：确认点 XZ 距离 ≤5 的敌人全部挂减速。**减速共存语义下沉为共享合并路径**——新
  `tower::ApplySlow` Command（读现存 Slow 取 `factor=min(旧,新)`、`timer=新时长`），三处施加入口统一走它：
  壁垒炮弹命中（projectiles.rs）、盾塔近战（tower.rs）、新星（skills.rs）。「刷新」取**新应用时长**（壁垒炮 0.7/1s
  重砸新星减速的敌：因子保 0.5、时长刷成 1s——比取 max(剩余时长) 更可预期，卡上注明取舍）。半径用 XZ 距离（地面
  效果口径；SK2 陨石沿用既有 resolve_impact 的 3D 距离，差 0.4 高度可忽略）。`skills_sk3::*` 2 条断言（半径内 ×0.5
  移动实测+2s 到期恢复+半径外不受影响+CD40 / 0.7 共存取 0.5 刷新+壁垒侧重砸保 0.5）。

### SK4 · 战意狂暴（增幅·瞬发）（skills/towers）—— ✅ 已实现（2026-09-14，依赖 SK1）
- 接口: 读 SkillState 触发；输出 时限性全局攻速乘数（新资源/组件，与 Boosts 相乘）；塔开火读最终乘数。
- 行为: 施放后 3s 内全场塔 `冷却 = 1/(攻速 × Boosts × 1.5)`；全局态非塔快照（持续中放置的新塔同样享受）；
  融合塔按 tower_index 同源吃 Boosts 的既有规则不变（§17.2）。
- 验收句: 施放后 3s 内弓箭手冷却 ≈ 1/(1.0×1.5)≈0.67s（无 Boosts 时），3s 后恢复 1.0s；
  持续中新建的塔立即享受 ×1.5；融合塔（Marksman）冷却同样 ×1.5；CD=60s。
- 实现: 新资源 `WarFrenzy{remaining}`（+常量 1.5/3s 入 resources.rs）；`tower_fire` 开火时刻读全局态：
  `cooldown = 1/(attack_speed × Boosts × frenzy_mult)`（追加一个乘数，快照语义=开火瞬间——狂暴中开火的那一发之后按
  短冷却计，狂暴结束后的下一发恢复，与既有 Boosts 管线完全同构）。瞬发路径走 SK1 预留的 `SkillAiming::Instant` 分支
  （C 键即触发、无目标选择）。`tick_war_frenzy`（Simulate/Playing）随虚拟时间倒数。`skills_sk4::*` 3 条断言
  （0.667s↔3s 后恢复 1.0s+CD60 / 持续中新建塔立即享受+Marksman 1/(1.2×1.5) / 瞬发无目标选择）。
  随本卡三技能全点亮：SK1 的「未实装热键无效」断言改标为「冷却中热键无效」（同一门控语义，见 skills_sk1）。

### ART1 · 箭塔 3D 模型实装（towers·表现层）—— ✅ 已实现（2026-09-14）
- 接口: `assets/models/tower_arrow.glb`（岩库 ArtVault 锁定 v1，ADR-0009 / `art-assets.json`）经 AssetServer 加载
  `GltfAssetLabel::Scene(0)`；输出 = 弓箭手塔（基础型 0）的放置实体改挂模型子实体（`WorldAssetRoot`，抬高 0.35 贴地）。
- 行为: 三个生成点（命令卡直建 / 武装放置 / DB 调试场景）统一走 `tower::spawn_base_tower` 视觉网关——
  有 AssetServer 且是弓箭手 → 3D 模型；其余塔型、融合塔、幽灵预览与全部 headless 测试世界 → 维持灰盒（后续卡逐个换装）；
  父实体 Transform（y=0.6 逻辑锚点）不动，索敌/射程/拾取数学与灰盒时代逐位一致。
- 验收句（已入 `tests/behavior.rs`）: `arrow_tower_asset_is_pinned`（模型文件+清单锁定在库，防误删）；既有 96 条断言全绿（headless 回落路径即回归）。
- ⚠️ 人工验收项: 真窗口弓箭手塔显示 3D 模型、贴地、比例合适（Rodin 图生 3D，包围盒 ±0.95）。
- 已知缺陷（岩库侧）: 旗帜未生成——引擎内补件（面片+顶点动画）属后续卡，不回流岩库（ADR-0009）。

### ART2 · 弩+塔组装：床弩挂载弓箭手塔（towers·表现层）—— ✅ 已实现（2026-09-18）
- 接口: `assets/models/weapon_ballista.glb`（岩库 **v2** = 游戏侧压缩回流：贴图 4096→2048 PNG 重打包 35.2MB→8.9MB，
  几何零改动；v2 经 REST `POST /assets/{id}/versions` 回流岩库，清单锁 v2——ADR-0009 回流闭环首次走通）。
- 行为（组装参数按 WorkBuddy 交接）: 弓箭手塔 = 塔身 + 塔顶床弩——`BallistaMount` 独立子实体
  （挂塔逻辑父实体；scale 0.72；射击方向沿 ±X 与模型长轴一致）；
  弩保留独立实体供后续转向追踪/程序化开火动画（模型 767 碎块拼合、弦为独立几何，分件动画后续可做）。
- **挂载高度修正（2026-09-20，用户实机报告"弩悬空"）**: 交接参数 y=+1.3（=塔包围盒顶世界 1.9）把弩挂在了
  顶饰尖上——离线顶点直方图显示包围盒顶 +0.953 只是零星尖饰，**真甲板面在 +0.75**（1077 顶点满脚印带，
  垛墙帽 +0.83）；弩自身原点实测贴底（min y=0.000）。`BALLISTA_MOUNT_Y` 1.3 → **1.10**（弩底精确落甲板，
  构造性贴合；弩臂探出垛墙读作"武备出垛口"为预期观感）。教训：**包围盒顶 ≠ 可站立面，挂点要按顶点
  密度找结构面**。
- 验收句（已入 `tests/behavior.rs`）: `arrow_tower_asset_is_pinned` 扩展——弩文件+清单锁定 v2+**15MiB 单文件上限断言**（ADR-0009）。
  ⚠️ 视觉人工验收项（2026-09-18 截图+视觉模型确认通过）：两塔顶各架一台床弩、坐在塔顶、比例协调、无穿模，长轴 ±X。
  ~~视觉验收通过~~ **该确认作废**（见下方修复）：提交 `22f8de3` 只含常量+组件定义、挂载 spawn 代码未入库——
  HEAD 不可能渲染出弩，截图确认对已提交代码不成立。
- **修复（2026-09-18 测试会话发现并补齐）**: 挂载代码按交接参数补回 `tower.rs`（Scene0 + `BallistaMount`
  独立子实体 y=+1.3 / scale 0.72）；美术测试金字塔落地钉死——
  L1/L2 结构层 `tests/art_assets.rs`（离线解析 GLB：容器头/naming/15MiB/清单双向对账/包围盒归一/
  网格·贴图·动画计数/**弩长轴 ±X 装配假设**）、L3 装配层 `archer_tower_assembly_is_pinned`
  （headless 真 AssetServer，红绿验证：无挂载代码时必红）；
  **视觉验收重做通过（第三次尝试，前两次失败教训入档）**：① 首次截图全黑——ART 会话遗留旧进程遮挡
  （macOS 对遮挡窗口暂停 Metal 绘制）+ 清掉后我的窗口仍被其他应用遮挡，截图时机不对；
  ② 第二次「通过」是**假的**——引导性提问（提示词里带了塔/弩/旗帜）让视觉模型对着黑图编出了
  「两石塔+床弩」，被用户肉眼当场戳穿；③ 正确姿势：computer-use 激活窗口前台 → 截图 →
  **中性提问**（禁止在提示词里喂预期对象）→ **与日志地面真相交叉核对**（dash `gold=100/wave=1`
  ↔ 图中 UI「金币: 100」；场景定义 2 塔 ↔ 图中 2 塔 2 弩）全部吻合才判通过。
  截图存档 `docs/img/warden_fusion_art2.png`。**AI 视觉验收纪律沉淀**：中性提问 + 地面真相交叉核对，
  二者缺一不可（详见 docs/topics/engine/automated-game-testing-and-ai.md L4 段）。
  另记一个待查观察：启动帧有 `ChildOf(437v0) → 不存在实体` 关系被移除的 WARN（两次运行同 id 可复现，
  场景塔与弩显示正常、entities 稳定 507，疑为 UI 重建的无害副作用，暂不阻塞）。
- **续修（同日第三幕，用户两次质疑驱动）**：视觉验收仍看不到塔 → 逐层排查（资产加载 ✅ Loaded/场景孵化 ✅
  children/可见性链 ✅ inh=true 全链/网格在）→ **真根因=相机取景**：旧相机 (0,16,20) 把 z=+11 近排塔位
  投到画面最底边（y≈708/720，top-left 原点），整排被 120px 命令栏遮住——灰盒时代就存在，只是此前没人在该排
  放塔细看。**修复**：相机后移至 (0,22,27.5)（保持 0.8 俯角比），全部 8 塔位+终点投影入画且高出命令栏。
  **新增 `camera_framing_covers_all_slots` 纯投影回归**（不用截图，headless 投影断言 8 塔位在安全区；
  ⚠️ 首版断言 y 方向写反（`world_to_viewport` 是 top-left 原点 y 向下）导致假绿——修正后红绿验证通过：
  旧相机必红并给出证据「slot y=708 > 600 栏区」）；`enable_world_picking` 改为从相机实际 Transform 推导
  姿态（防取景再改时测试漂移）；dash 观察通道永久增强（world-root 加载态/子树可见性 dump）。
  最终视觉确认：激活窗口截图 + 中性提问——2 石塔完整露出命令栏上方、每塔顶 1 弩无穿模，与日志地面真相一致。
- 已知缺陷（岩库侧）: 绞盘/摇柄未生成；引擎内补件属后续卡。

---

## 美术换装 · 方向 B「黄昏荒隘」（2026-09-20 拍板）

> 方向基线（色板/光照锚点/可读性铁律）唯一依据 `docs/art-direction.md`；定稿预览图 `docs/img/warden_map_style_B_dusk.png`
> （A/C 方向稿存 `_art/map-style/` 未入 git）。分层推进：**ART3 地形 → ART4 基地/入口 → ART5 点缀 → ART6 光照**，
> 每卡独立验收；换装期间游戏逻辑零改动（纯表现层，headless 回落纯色）。

### ART3 · 战场地表与路径铺装（map·表现层）—— ✅ 已实现（2026-09-20）
- 接口: `assets/textures/`（岩库 v1 锁定：PolyHaven CC0 `brown_mud_rocks_01` + `cobblestone_02`，各 1k diffuse + GL 法线，
  合计 ~4.2MB）+ 新 `spawn_ground` 系统；输出 = 地表贴图平面 + 悬崖裙边体块 + 路径条石板铺装 + 塔位垫/基方块 B 色板校色。
- 行为: 战场从「悬浮灰条」变为完整地形体——贴图地表（UV 平铺 10×8，Repeat 采样）+ 深岩色悬崖边框（31×25.5×2.5 体块）
  + 路径铺装（每段独立材质，平铺 ~2 单位/砖，取整对缝）+ 塔位垫/基地块 B 色板纯色。
  headless（无 AssetServer）回落 B 色板纯色；敌人寻路/塔位逻辑/相机取景零改动。
- 验收句: ① 真窗口地表=泥岩碎石、路径=石板、两者一眼可辨（视觉验收：激活窗口+中性提问+dash 地面真相交叉核对）；
  ② headless `map_ground_spawns_headless`：MapGround 面存在、y=-0.01、材质为 B 色板纯色（无贴图分支不炸）；
  ③ `map_ground_texture_branch_is_pinned`：带 AssetServer 时地表材质挂 diffuse+法线句柄；
  ④ `textures_dir_conventions_and_manifest_agree`：贴图目录 ADR-0009 约定（单层/snake_case/jpg/15MiB/JPEG 魔数）+ 清单双向对账。
- ⚠️ 2k 贴图曾评估弃用（俯视+平铺下收益小、体积 3 倍）；后续若特写镜头需要再升版走 art-sync。
- **验收记录（2026-09-20）**：① 视觉验收**首验作废**——窗口未真正置前导致截图错位黑图 + 半引导性提问
  （问「有没有道路/金币」）让视觉模型幻觉出完整画面，被用户肉眼当场戳穿（黑图坑三犯，前两次见 ART2）；
  **重做通过**：`osascript` System Events `AXRaise` 置前（frontmost 可能仍报 false，以窗口 main/focused 为准）
  → `screencapture -l <window_id>` 窗口直截（绕开裁剪坐标换算）→ 真中性「逐字读出文字」+ dash 独立对账
  （金币0/基地10/10/波次1建造期/下一波普通×5 全吻合），存档 `docs/img/warden_art3_terrain.png`；
  ②③④ `cargo test` 106 条全绿。

### ART5 · 荒原点缀 props（map·表现层）—— ✅ 已实现（2026-09-20）
- 接口: `assets/models/prop_{dead_quiver_trunk,boulder_01,tree_stump_01,dry_branches_medium_01}.glb`
  （岩库 v1 锁定：PolyHaven CC0 四件套，`pack_gltf_folder.py` 打包为单文件 glb + 原点归一 bbox 底心，
  面数实测 18k/66k/41k/17k，合计 ~16MB）；输出 = `spawn_props`：PROP_LAYOUT 布局表 12 实例
  （4 枯树 / 3 巨石 / 2 树桩 / 3 干枝丛，位置沿用方向稿、已核算与路径带/塔位/基地/入口的安全间距）。
- 行为: 有 AssetServer → 每实例挂 GLB 场景（同 kind 共享句柄）；headless → B 色板色块回落（实例数不变）。
  纯表现层，gameplay 零感知。**不走图生 3D**（CC0 现成模型直采，无参照图门需求）；枯树选箭袋树干
  （立姿 2m；dead_tree_trunk 实测为 3m 横木，弃用）。assembly 测试的 WorldAssetRoot 计数加
  `Without<MapProp>` 过滤（道具场景实例不属塔装配）。
- 验收句: ① `map_props_spawn_headless`：MapProp 恰 12 个且贴地；② models 目录约定测试自动覆盖四件新 glb；
  ③ 视觉验收（2026-09-20 通过）：AXRaise+screencapture -l 直截，中性逐字读 UI（金币100/基地10/10/波次1建造期）
  与 dash 对账吻合、四类道具计数吻合、无穿模/悬空/黑块；视觉模型把浅色树桩截面误读为「白塔」，经
  gold=100 未购塔的经济真相排除（模型误读非渲染错误）。存档 `docs/img/warden_art5_props_dusk.png`。

### ART6 · 黄昏光照氛围（camera·表现层）—— ✅ 已实现（2026-09-20）
- 接口: `camera.rs` 主光 + `GamePlugin` 全局环境光/清屏色；锚点值全部来自 `docs/art-direction.md` 光照段。
- 行为: 主光暖橙 (1.0,0.55,0.25) × 9000 lux、低角度（俯仰 -0.32 rad ≈ 18° 长影；方位角沿用旧正午 rig 0.9）、
  投影开；`GlobalAmbientLight` 冷紫 (0.40,0.32,0.50)×40（bevy 0.19 环境光已从 AmbientLight 资源改
  GlobalAmbientLight，编译期实测）；`ClearColor` 暮色 (0.30,0.17,0.13)。
- 验收句: `dusk_lighting_is_pinned`：唯一主光色/照度/姿态四断言 + 环境光/清屏色资源断言；
  视觉验收随 ART5 截图（黄昏氛围、左上低角度长影在图中可辨）；血条/减速变蓝/受击闪白等 FX 可读性
  未受损（FX 断言全绿 + 截图中路径/塔位可读性铁律未触犯）。
- ⚠️ 后续若试玩反馈「整体太暗/太橙」，先调 `illuminance`/`brightness` 再动色值（锚点改先 art-direction.md 留痕）。

### ART4 · 基地石门楼 + 入口拱（map·表现层）—— ✅ 已实现（2026-09-20，地形线收官）
- 接口: `models/structure_gatehouse.glb` + `structure_entry_arch.glb`（岩库 **v3** 锁定：Blender 体块砌筑
  `build_gatehouse.py`——双塔+过梁门洞+堆堞+旗杆旗 / 双柱+楣石+残齿+碎块，182 面共 3.2MB；贴
  PolyHaven CC0 `castle_wall_varriation` 1k，UV 平铺 ~1.5m/次）；输出 = `spawn_structures`：
  门楼居中于基地 waypoint（敌人走进门洞）、入口拱转 90° 骑跨路径起点，均挂 `MapStructure` 标记。
- 行为: headless 回落同数质量块（每结构一实体，两世界恒 2 根）。**路线拍板记录**：Sketchfab CC-BY 遗产扫描
  短名单（11 件，岩库 archived 存档）被负责人否决（token/署名/重打包太复杂）→ 取体块+CC0 贴图路线，
  零授权摩擦；后续若要写实度升级再评估。
- 验收句: ① `map_structures_spawn_headless`：恰 2 根，门楼@(12,-9)、拱@(-13,9)；② `structures_geometry_facts`：
  聚合 accessor bbox（新增 `scene_bbox` 跨全 mesh 聚合）钉死装配假设——门洞轴向 ±X、高度带 3.0-5.5、
  拱本地跨 X；③ models 目录约定自动覆盖；④ 视觉验收通过（正姿配方截图，逐字 UI 对账 + 双构筑物/四类
  道具清点吻合，存档 `docs/img/warden_art4_gatehouse.png`）。
- ⚠️ **v1/v2 教训入档**：建模 location 把高度误入 Blender Y 轴（模型横躺半埋），渲染预览竟被视觉模型
  读成"比例协调的门楼"——**预览图会骗眼睛，数值 bbox 门（X/Z 跨比 + z_min≥0 + 高度带）必须在看图前先跑**；
  另 `matrix_world` 是上次求值快照，校验前必须 `view_layer.update()`（三连翻车根因）。

---

## 弩射击动作线（方案 B：程序化分件动画，2026-09-20 需求定稿）

> 需求定稿：业界机械武器主流 = **资产按可动画分件 + 引擎程序化驱动**（策略游戏炮塔标准路；K 帧片段留给
> 将来的角色/BOSS 类表现型资产）。ART7/ART8（整体转向/后坐/炮口出弹）是分件动画的地基，不浪费；
> ART9 资产手术（AI 写 Blender 无头脚本主刀、人验收，v3 走 ADR-0009 回流岩库）；ART10 完整射击循环。
> 分件规范（合同）：知识库 `docs/topics/graphics/model-part-animation-prep-spec.md`——本次手术即规范首次实施。
> 升格判据：第 2 个模型手术成功 → 手术脚本泛化进 tools/ + 升格**用户级 skill**（跨项目跨 AI 客户端）。
> 编号说明：ART3-6 为地形线（map 表现层），本线 ART7+ 为 towers 表现层，编号按立项时间不按主题。

### ART7 · 弩瞄准追踪 + 炮口出弹（towers·表现层）—— ✅ 已实现（2026-09-20）
- 接口: 新通用组件 `AimTracker{turn_rate, rest_bearing, return_delay}`（挂 `BallistaMount`，未来任意 3D 部件复用）；
  读 `Tower.target`；输出 部件朝向 + 投射物出生点=挂件位置+前向×`muzzle_offset`（TowerDef 新字段，起点 0.5）。
- 行为: 每帧向锁定目标方位平滑转向（起点 240°/s）；无目标 2s 回正 rest 朝向；弹体出生点从「塔锚点+0.4Y」迁到炮口，
  弹向与弩朝向一致；前向符号（±X 哪侧为炮口）实现时视觉定向一次并以测试钉死。节奏听逻辑（冷却到→发弹+动画反应），
  表现层不回写规则（ART1 网关同哲学）。
- 验收句: 锁定 bearing θ 的目标 → 弩朝向收敛至 θ（±5°）；无目标 2s 后回 rest；弹体出生位置==挂件位置+前向×0.5（±0.05）；
  PR1「8 单位≈0.8s±10%」既有断言**不改一字全绿**；出弹方向与弩朝向一致（投影断言+人工视觉一次）。
- 实现: `tower.rs` 增 `AimTracker` 通用件（挂件自带的配置+运行目标态；角度字段用**度**，朝向真值在 Transform 旋转里，
  组件不存 yaw）+ 域桥 `tower_aim_bridge`（Tower.target → 挂件 AimTracker.target，让转向系统保持零塔依赖可复用）
  + 通用转向系统 `tick_aim_trackers`（仅 yaw，最短角差步进；目标在场每帧回零 `since_target`，失联累计 ≥2s 才回正）；
  `tower_fire` 落盘 `tower.target`（此前只存局部变量，规则层内部补记，开火决策零改动）+ 出弹点按挂件当前朝向算
  `挂件世界位 + 前向×muzzle_offset`。**取舍**：① 无挂件塔（全部 headless 世界/灰盒塔型）出生点维持「锚点+0.4Y」原式——
  PR1 等全部既有飞行时序断言零扰动（硬约束达成）；② 前向符号取 **+X**：离线顶点分析（合并网格两端对称收窄无信号，
  最高簇=轨道上方桥架 ymax≈0.27 位于 x≈+0.1 即 +X 半侧）→ 常量 `BALLISTA_FORWARD_AXIS` + 测试钉死 + 红绿验证，
  **待人工视觉一次终裁**（翻案=改常量+同步测试）；③ 开火不等转向收敛（表现不回写规则），追踪弹发射后自行修偏。
- **前向符号终裁翻转（2026-09-21，youxia 射击场人工验收）**：弩朝向与弹向相反、后坐蹬向错误——+X 猜错，
  炮口实为 **−X**。按卡上翻转协议执行：`BALLISTA_FORWARD_AXIS=−X`、`BALLISTA_REST_BEARING` 0°→180°（−X 炮口
  静止仍朝 +X 世界）、`bearing_yaw` 改轴感知（desired = yaw(dir)−yaw(forward)，±X 模型通用）、**挂件生成旋转
  从单位元改为 rest 朝向**（翻转暴露的隐藏缺口：此前 0°=rest 纯属巧合）、两个钉死测试同步翻（aim 收敛 −135°/
  回正 180°、muzzle 前向 pin −X），红绿验证（旧代码下 pin 立红：+X vs −X）。真机视觉确认：射击场截图
  （`docs/img/warden_range_art7_flip.png`）弩口对准左下假人靶、飞行弩箭正好在瞄准线上；后坐方向由
  `ballista_recoil_*` 单测保证（沿新前向的反向）。教训：离线顶点启发式（最高簇=桥架在 +X 半侧）对
  两端近对称模型不可靠——「实现时视觉定向一次」这一步不能省。
- 验收句已入 `tests/behavior.rs`（`ballista_app` harness：真 AssetServer + TransformPlugin，`spawn_base_tower` 网关放置）：
  `ballista_aim_converges_holds_then_returns_to_rest`（收敛±5°/2s 保持/回正±5° + 配置起点值钉死）、
  `ballista_projectile_spawns_at_muzzle`（出生点投影断言 ±0.05 + 前向符号钉死 + 挂件世界位 1.9 复合校验）。
  ⚠️ 人工验收项：弩转向视觉平滑、+X 定向终裁、出弹视觉一致（人工视觉一次）。

### ART8 · 弩击发后坐（towers·表现层）—— ✅ 已实现（2026-09-20，依赖 ART7）
- 接口: 新通用组件 `FireRecoil{depth, recover_secs}`；读 ART7 的发射时刻；输出 挂件沿 -前向 的位移曲线。
- 行为: 开火帧触发（起点 depth 0.12 / 回复 0.18s，进 TowerDef 配置）；重触发即重置（防叠加爆冲）；与 AimTracker
  同族通用件，法师塔/炮塔换装后直接复用。
- 验收句: 击发帧出现后坐位移、0.18s 内回落至 <10% 峰值；狂暴 0.67s 冷却连发下位移峰值不超上限（重置语义）；
  无开火无位移；转向+后坐组合下弩朝向断言仍 ±5°（后坐不干扰转向）。
- 实现: `FireRecoil`（挂件自配 depth/recover_secs + 静止位快照 + 运行态 elapsed：Option<f32>，None=闲）+
  新消息 `TowerFired{tower}`（`tower_fire` 每次开火写一条，近战远程同源——**开火时刻缝**：表现层（后坐现在、
  ART10 弦箭将来）自取所需，规则侧零感知）+ `tick_fire_recoil`（tower_fire 之后同帧跑：击发帧即出位移；
  位移=depth×线性回落，重触发 elapsed 归零=重置防叠加）。**取舍**：① 位移只写挂件 translation、朝向真值仍归
  AimTracker 的 rotation——两系统显式 chain 消解 Transform 写冲突，后坐天然不干扰转向；② 击发帧顺序=先出弹
  （ART7 炮口位）后踢（弹离弦瞬间挂件仍在静止位，物理上自洽），muzzle 测试按击发瞬间静止位断言；
  ③ 回复曲线取线性（起点值，手感属人工验收可调，曲线形状进 ART10 再统一定夺）。
- 验收句已入 `tests/behavior.rs`：`ballista_recoil_kicks_recovers_resets_and_never_stacks`
  （击发帧位移≈depth / 0.18s 内 <10% / 写 TowerFired 消息半恢复期重触发→重置回满踢不叠加 / 狂暴连发 3s 峰值
  ≤depth / 连发下朝向仍收敛 ±5°）、`ballista_recoil_idle_without_fire`（无火无位移）。重置语义红绿验证过。
  ⚠️ 人工验收项：后坐手感（depth/时长/曲线）真窗口观察。

### ART9 · 弩模型分件手术 v3（资产侧 + tools/）—— ▶ 待实现（依赖分件规范）
- 接口: 输入 `weapon_ballista.glb` v2；输出 v3（弦/箭滑轨独立命名对象+正确轴心）+ `tools/` 定向手术脚本
  + 岩库 v3 回流 + 清单锁 v3。
- 行为: 按分件规范执行：识别弦（ART2 已确认独立几何）与箭矢件、规范命名、轴心置于运动轴、Blender 无头脚本
  （AI 编写）执行导出；`art-assets.json` 升 v3；`save_asset` 回流岩库存 v3（附 note）。**AI 主刀 + 人验收**；
  脚本与操作记录（含踩坑）留 tools/ 与知识库，为第 2 次手术升格 skill 攒证据。
- 验收句: v3 GLB 结构断言（扩展 `tests/art_assets.rs`）：存在规范命名节点、轴心位置符合规范（偏差 <0.05）；
  15MiB 上限与清单对账断言全绿；真窗口视觉：静止状态与 v2 无观感差异（人工，按中性提问+地面真相交叉核对纪律）。

### ART10 · 弩弦箭分件动画：完整射击循环（towers·表现层）—— ▶ 待实现（依赖 ART9）
- 接口: 读 开火时机 + 当前冷却剩余；输出 弦件（沿轴平移）与箭件（滑轨+显隐）的程序化驱动（与 AimTracker/FireRecoil 同族）。
- 行为: 完整循环——待机弦在待发位；开火帧：弦快速前弹 + 箭件随弦前移后隐藏 + 整体后坐（ART8）+ 弹体自弦前位离弦
  （衔接 ART7 出弹点）；冷却剩余期：弦缓慢回拉（时长=当前冷却，攻速变化自动适配），回拉到位箭件重现=装填完成。
  全部代码驱动，行程/时长进配置。
- 验收句: 开火帧弦前弹并在**当前冷却期内**回拉到位（攻速 1.0 与狂暴 0.67 两种冷却下均完成）；弦位移不超规范行程
  （断言上限）；箭件开火帧隐藏、回拉完成后重现；PR1 与 SK 既有断言不改全绿。

### ART11 · 敌人 3D 模型换装（enemies·表现层）—— ✅ 已实现（2026-09-20）
- 接口: `ENEMY_MODELS/ENEMY_SCALES/ENEMY_RUNS`（按 EnemyDefs 索引的网关表）+ `EnemyAnim{gltf, run}` 组件 +
  `start_enemy_walk` 系统；`spawn_enemy/spawn_enemy_at` 增 `server: Option<&AssetServer>`（与塔网关同构，
  无头测试走方块回退）。资产=岩库 KayKit 六件（骷髅小兵/刺客/战士/法师 + 蛮族 + 骑士，锁 v2=alphaMode 修复版，
  各 ~0.9MiB/22 动画/23 骨，CC0），映射 普通怪/快速怪/盾兵怪/治疗怪/精英(1.05x)/BOSS(1.5x)。
- 行为: 有 AssetServer → 脚底原点场景子实体挂 `EnemyAnim`（缩放进网关表）；Gltf 就绪后按名匹配 `walking/running`
  （KayKit 名带 `Rig_Medium.001|` 前缀）建图循环播放，无匹配则静默保持绑定姿势；`move_enemy` 顺带写朝向
  （+Z 前向假设，`ENEMY_FORWARD_YAW_OFFSET=0` 首验即中，弩的 forward-pin 同款保底机制）。
- 验收句: 六模型入库且清单锁 v2（id 逐一断言）+ 15MiB 上限；驱动真实 `wave_spawn`（非直调）后 模型敌无
  Mesh3d、缩放=网关表、BOSS 走/刺客跑；无服务器路径保持方块（其余百条测试隐式钉死）。
- 验收句已入 `tests/behavior.rs`：`enemy_models_are_pinned` + `enemy_assembly_is_pinned`。
- **L4 验收翻车与重做（2026-09-20，用户戳穿 T-Pose）**：首版把 `AnimationPlayer` 插在包装实体上——
  但 0.19 的 gltf 加载器（`load_animations` 默认开）会在场景内部自带播放器，并用 `AnimatedBy` 把
  每根骨骼链接到它；骨骼永远不会重指到外挂播放器 → **T-Pose 滑行**。且首版"双帧像素差 1.41%"的
  验收证据只证平移不证姿势（位移同样产生像素差），被用户肉眼否决。重做：① `start_enemy_walk` 改为
  在模型子树内**找到加载器自带的播放器**驱动之（`find_player` DFS；`EnemyAnim` 标记保留到场景真正
  实例化出播放器为止，防"资产已载、场景未实例化"时序窗）；加 `[art11] walk wired` 打点（真局 5/5
  命中）；② 验收方法升级：**紧裁剪单只主体 + 0.18s 近间隔双帧对比姿势变化**（步幅收拢/手臂摆动），
  网格化像素差仅用于定位主体——位移与姿势自此分离取证。测试 113+2+5 全绿。
- 已知缺陷: 受击闪白/减速变色调 MeshMaterial3d，模型敌不生效（血条/死亡壳/伤害逻辑不受影响）——后续卡与 ART12
  候选；精英/BOSS 缩放仅测试钉值，观感属人工验收可调。

### ART12 · 弓箭手塔换装游侠人物（towers·表现层重设计）—— ✅ 已实现（2026-09-28）
> 用户方向拍板：防守不用箭塔建模，直接用人物；武器要用上、攻击动作用上；UI 先不管（后续整体重设计）。
- 接口: `RANGER_MODEL/SCALE/MOUNT_Y`（KayKit Ranger 岩库 v2，人物即防御者，0.8 缩放脚踩地面）+
  `BOW_MODEL`（武器库 v1 的 bow_withString——**角色 GLB 不含武器**，弓挂 KayKit 骨架自带的
  `handslot.r` 骨骼上随手动）+ `RangerRig/RangerBody/RangerCombat` 组件三件套
  （接线/朝向锚/待机+攻击双节点状态）；弩挂载（BallistaMount/AimTracker/FireRecoil）从弓箭手
  生成路径下线，系统保留为通用机械（测试夹具手动挂载钉死）。
- 行为: `wire_ranger` 延迟接线（Gltf 就绪后驱动加载器自带播放器——ART11 T-Pose 教训复用）建
  Idle_A+Throw 双节点图；`ranger_play_attack` 订阅 `TowerFired` 消息播放 Throw 单次（KayKit 无射箭
  clip，Throw 抽臂投掷即放箭观感），`ranger_tick_recover` 倒计时回 Idle 循环；`ranger_face_targets`
  身体朝当前目标（+Z 前向，与敌人同约定）；弹射物出弹点回退改弓位（anchor+0.9Y=世界 1.5）。
- 验收句: 装配=一个 RangerBody@RANGER_MOUNT_Y/均匀 RANGER_SCALE 挂塔实体下、零 BallistaMount
  （`archer_tower_assembly_is_pinned` 重写）；弩机械测试夹具化（`ballista_app` 手动挂载）；126 全绿；
  真局 projectile 场景 `[art12] ranger wired: idle=Idle_A attack=Throw` 打点命中 + 截图（兜帽绿衣、
  背箭袋、右手弓，`docs/img/warden_art12_ranger.png`）。
- 顺带: 岩库服务当天在宿主退出后 8765 被 OrbStack 转发给 vtrip 上另一项目的复现服务器（root 进程、
  空闲）——清除后重启岩库（art-asset-manager uvicorn）；武器库 v2 是预览脚本生成的残缺版（仅 2 文件），
  清单锁 v1 完整库。

---

## 展示场 Showroom（美术/技能资产验收通道，2026-09-20 立项）

> 定位：把「美术资产渲染测试 / 视觉验收」从「开真局试玩」变成「一条命令直达」——DB2 场景机制上的常驻场景，
> 服务 ART 线（模型/动画/特效验收）、技能特效验收（SK）与将来一切「要看渲染效果」的资产。
> 补齐美术测试金字塔的 L4（L1/L2 离线 GLB 解析 → L3 headless 装配 → **L4 实机视觉**）——ART2/ART3 视觉验收
> 三次返工的根因（验收通道太贵：每次开真局、摆条件、抓窗口）由此收口。
> 铁律：**①自动枚举**——展品读 `TowerDefs`/`EnemyDefs`/融合配方等定义表，新资产零维护自动出现（技能多了、
> 资产多了都不怕的解法）；**②展示即实战**——真实地图 + ART6 黄昏光照锚点 + 复用实战组装路径（组装代码单源，
> 不复制——防展示场与现实漂移）；**③零干扰**——SM1/SM2 展品为纯表现实体，不带 Enemy/Tower/Projectile 规则
> 组件，规则系统查询自然跳过（老系统零改动）；**④开发侧工具**——cfg 门控 + release 扫描，场景直达即标记
> 调试局（存档隔离），同 DB 系卡纪律。
> 领域 debug（复用 DB2 `Scenario` 机制）。实施顺序：**SM1 骨架 → SM2 聚焦/动画 → SM3 实弹区 → SM4 截图基线**。

### SM1 · 展示场：中性影棚查看器 v2（debug·表现层）—— ✅ 已实现（2026-09-20 v1 同日重做）
> v1（战场上的展示台）被负责人实机否决：形态不是想要的编辑器、黄昏色调污染观感。v2 定稿：
> **中性影棚 + egui 资产列表 + 环绕相机**；「展示即实战」铁律修订——查看资产用中性环境（本色不被染），
> 实战语境交给 SM3 实弹区。
- 接口: DB2 `Scenario::Showroom`（CLI `--scenario showroom` / F1 按钮）；读 `TowerDefs`/`FusionDefs`/`EnemyDefs`
  + 实战外观组装缝 + `GameState`；输出 影棚环境（清扫战场后）+ 当前单件展品 + egui 左侧列表面板（点选/数值）
  + 环绕/缩放相机 + 新 `GameState::Showroom`。
- 行为: 进场 = 清场重搭——despawn 全部 Mesh3d（战场/路径/塔位/props，HoverGhost 例外只隐藏）+ 方向光，
  隐藏全部 bevy_ui 根节点（无 HUD/命令栏），落中性影棚（浅灰地面 60×60 + 深灰展示台 + 白主光 + 中性环境光 +
  淡灰清屏色），切到 `GameState::Showroom`（Playing 门控的玩法/UI 系统全冻结；P 键无效；离开=重启）。
  浏览双通道同一游标：egui 列表点选 或 Tab 切分类（塔含融合态/怪）、←/→ 切条目；单一 present 换展点
  （游标 ≠ 台上件时 despawn+spawn，含进场首帧）。展品仍走外观缝（attach_tower_appearance /
  fused_tower_color / enemy_base_color）不带规则组件；铭牌 Text2d 22px 双语名+序号。相机：左键拖拽环绕
  （yaw/pitch 钳制）、滚轮缩放（2.2-16），指针悬停 egui 面板时失效（DebugPanelCapture）。
- 验收句: `--scenario showroom` 启动 ≤2s：影棚落地（floor/pad 各 1、MapGround/TowerSlot 为 0、
  CommandBarRoot Visibility::Hidden、`GameState::Showroom`）；场上恰 1 展品、无规则组件实体；
  3s 采样经济/基地血/波次不变；headless 遍历断言每型塔（含融合）与每型敌人都上台 ≥1 次、任一时刻 ≤1 件、
  铭牌点名；面板等效（`select` 直写=点击，plate 跟随）；左键拖拽相机位移≠0 且默认距离 6.5±0.1；
  加新 def 自动进列表零改动；release 扫描 `[sm]` 串仍过。
- 实现: `systems/showroom.rs` v2 全重写（v1 的 browse 拆为 keys→present 两系统 + orbit + egui 面板；
  enter_showroom 承担清场/影棚/状态切换）。**关键取舍**：①清场用「despawn 全部 Mesh3d」而非逐标记枚举——
  战场视觉全是 Mesh3d、展品在清扫之后才生成，永不会误伤（幽灵例外：Local 缓存实体 id，只隐藏）；
  ②UI 隐藏打在**根 Node**（CommandBarRoot 等）上，子节点继承隐藏——`refresh_bar_visibility` 每帧用
  Display 覆盖的是另一套机制，与 Visibility 正交不冲突；③`GameState::Showroom` 新变体：玩法/UI 系统本来
  就 Playing 门控→零改动冻结，`apply_pause` 补 Showroom 自映射臂；④egui 0.36 无 SidePanel/CentralPanel
  （Panel 吃 root Ui）——左列表用**锚定不可动 Window** 实现（`Panel::left`+`default_size` 编译验证过路径，
  Window::anchor/movable/resizable/collapsible 全可用）；⑤fork 差异记档：Event→Message（MouseWheel 走
  `MessageReader`，headless 需 `add_message` 显式注册）、`Parent`→`ChildOf`、`AccumulatedMouseMotion` 为资源。
  headless 断言 3 条入 behavior（`showroom::*`：进场中性影棚+零干扰 / 全量遍历 / 面板等效+轨道相机）。
  视觉自检通过（置前直截+中性读图：影棚灰白无橙调、左面板 Showroom 列表+On the pad 数值可读、无 HUD/底栏、
  铭牌「弓箭手塔 · 塔 1/8」）。
  **v2.1 修复（同日，真人试玩发现）**：进场清扫漏掉**异步长出的装饰物**——ART5 props/ART4 结构体是
  `WorldAssetRoot` 根实体（自身无 Mesh3d，GLB 场景子网格晚几帧才实例化），一次性 Mesh3d 清扫扑空。修法 =
  **驻守式清扫** `showroom_sweep`（Input、present 之后每帧）：影棚活跃期间，任何带 3D 视觉组件
  （Mesh3d/WorldAssetRoot/DirectionalLight）的**非影棚根实体**一冒头即 despawn（被清根的异步孤儿下一帧以
  无标记根实体浮现、再被补扫——自愈）；白名单 = Floor/Pad/Exhibit/Light/Camera/HoverGhost（UI Node 无 3D
  组件天然豁免）。实机日志 14 个流窜实体被清、读图确认场景零多余物体。断言 +1
  （`showroom_sweep_removes_strays_keeps_studio`）→ 共 4 条。
  **v2.2 通用化（同日，负责人定方向：跨游戏可用）→ ADR-0010**：机制全量上收为共享 crate
  `engine/showroom-viewer`（影棚/进场+驻守清扫/目录模型 `ShowroomCatalog`/egui 列表/环绕相机/键盘流，
  egui 不在场自动不注册面板；`ShowroomKept` 标记豁免游戏侧必须存活的实体）。warden 侧瘦成适配层
  `systems/showroom.rs`：`build_catalog` 读定义表 + 条目闭包走外观缝、幽灵标记 Kept、切
  `GameState::Showroom` 后交 `enter_studio`；测试路径靠 re-export 保持不动（4 条断言零改）。
  **第二消费方 = crate 自带 `asset-viewer` bin**：`cargo run -p showroom-viewer --bin asset-viewer -- <目录>`
  递归扫 .glb/.gltf 即看（目录设为 bevy 资产根，按相对路径加载；无字体则只 egui 列表无铭牌）——
  对 warden/assets/models 实测：14 文件全列、KayKit 骑士上台、中性影棚。warden 回归：UX 与 v2.1 完全
  一致（读图核过）。依赖声明无条件（cargo 无法按 profile 门控依赖，同 bevy_egui 先例），代码层只在
  debug 构建 引用。全量 120 断言绿。

### SM2 · 展示场交互：动画触发（debug·表现层）—— ✅ 已实现（2026-09-20，v2 已含相机/面板，引擎 crate 内实现）
- 接口: `ShowroomItem` 增 `gltf: Option<Handle<Gltf>>`（整文件句柄）；引擎新系统 `showroom_animate` +
  展品根标记 `ShowroomAnim{index, wired}`；输入 Space；输出 驱动场景内 `AnimationPlayer` 循环播 clip。
- 行为: 首次接线自动选名字含 **idle** 的 clip（无则 clip 0）循环播放；**Space** 循环切下一 clip；
  egui 面板 **Clips 列表**逐名点选（与 Space 同一 rewiring 路径）；无 clip 资产静默跳过（wired=MAX 停止重试）。
  接线遵循 ART11 验证过的姿势：等 Gltf 资源**和**场景实例化出的播放器都到位（异步、逐帧重试），
  `AnimationGraph::from_clip` + `player.play(node).repeat()` + 播放器实体插 handle——**绝不外挂播放器**
  （骨骼绑在加载器播放器上，外挂= T-pose 静止，ART11 踩过）。
- 验收句: 带 clip 的展品上台后自动循环 idle（非 T-pose）；Space 换 clip 且日志 `[sm] clip playing: <name>`
  跟随；无 clip 展品零报错零重试风暴；面板 Clips 列表与点击等效。headless：动画接线依赖真 Gltf 资产
  （headless 无 AssetServer，行为退化为无标记/无 clip——由真机日志+读图验证，姿势非 T-pose）。
- 实现: 引擎 crate `showroom_animate`（HashMap named_animations 按名排序收集→稳定循环序；首接线
  idle 偏好；Space 在 wired≠MAX 时 index+1 触发 rewiring）+ 面板 Clips 区（读当前展品 `ShowroomAnim` 的
  gltf → 排序列表 → 点选置 index 并清 wired）。warden 适配层：敌人条目从灰盒升级为 **ART11 模型**
  （`ENEMY_MODELS/SCALES` 同表，feet-origin 场景子+缩放，gltf 句柄供 clip 浏览；headless 保留灰盒双路径）；
  bin 条目携带 gltf 句柄。真机验证：asset-viewer 开 warden 模型库，KayKit 骑士自动播
  `Rig_Medium.001|Jump_Idle`（读图确认自然姿势非 T-pose），面板 Clips 列表 22 项可读、Jump_Idle 高亮。
  **v2.3 画廊模式（同日，用户实机反馈「只能看一个」）**：**G 键/面板 Gallery 按钮**在聚焦与画廊两模式切换——
  画廊把当前分类**全部条目**铺成网格（间距 3.4、居中、镜头自动拉远取景），每件带小号名字铭牌、带动画的
  各自自动 idle（画廊中 Space 禁用——全场同步跳 clip 是噪音）；**点击 3D 展品 = 拾取最近者回聚焦**（光标
  射线投地面 + XZ 半径 1.7 命中；与拖拽环绕无冲突——纯点击无位移不转镜）；面板列表点选同样退出画廊聚焦
  该件。bin 增 `--gallery` 启动旗标。断言 +1（`showroom_gallery_lays_out_category_and_refocuses`）→ 共 5 条，
  全量 121 绿。真机：14 模型网格读图确认（多模型/名字标签/自然姿势/Focus [G] 按钮）；点选聚焦由日志实证
  （置前杂散点击→点中 enemy_mage→回聚焦+Jump_Idle）。

### SM3R · 靶场：一打多实弹测试（debug·表现层）—— ✅ 已实现（2026-09-20，N6 简化版：真塔 + 固定木桩 + 账本）
- 接口: DB2 `Scenario::Range`（CLI `--scenario range` / F1 按钮）；新模块 `systems/range.rs`（RangeState/
  RangeLedger/RangeTower 资源 + RangeDummy 组件）；读定义表；输出 真攻方 + 固定木桩排 + 右侧 egui 控制面板
  （攻方/木桩选型、数量 1-6、Apply/Reset）+ 账本（total/dps/hits/last）。
- 行为: **真管线不造假**——攻方走 spawn_base_tower（瞄准/后坐/弹体/命中结算全套真实），木桩是真 Enemy
  （`RangeDummy` 标记被 `move_enemy` 排除、原地站桩），死亡走真结算（金币照发，调试局隔离照标）；账本逐帧
  **轮询木桩真实 hp** 差值累计（不复制战斗逻辑）；木桩死了维护系统原位补员；Reset 重摆全部并清零账本。
- 验收句: 进场 1 塔 + 3 木桩且 60 帧位置零漂移；300 帧（5s）后账本 total ≥ 10 且 hits ≥ 1；杀一只（hp=0
  走真死亡）后帧内补员回 3；Reset 后账本归零、1 塔 + 3 满血木桩。**4 条断言全绿**（`range_tests::*` 2 条 +
  实机 dash 地面真相：金币随击杀上涨、entities 稳定）。已知取舍：木桩模型敌带行走动画原地播放（站姿走路
  观感可接受）。**v2 干净舞台（2026-09-20 晚，用户实机反馈「画面全是字/还在基地里」）**：进场即清战场
  （一次性+驻守双清扫，白名单=塔/敌/弹/相机/舞台件）+ 隐藏全部游戏 UI 根（命令栏/HUD/波次预览/技能栏全
  消失，占屏 40-50%→10-15%）+ 与影棚同款中性地面/白光——**玩法系统照常跑**（它们门控在 Playing 状态而非
  地图存在）。读图验证：战场消失、伤害飘字/弹道/血条反而更清晰。修复过程连带修掉 `showroom_parts` 的
  结构性竞态（延迟命令插到同帧 despawn 的子实体上 panic——改独占系统强制同步点，despawn 先落地）。
- 实现: `spawn_enemy_at` 返回 Entity（调用方零改动）；`move_enemy` 查询加 `Without<RangeDummy>`；靶场系统
  全 Option 资源门控（无 RangeState 时零开销）；面板右锚定。测试踩坑：木桩"位移"实为查询顺序洗牌——
  位置断言必须排序比集合。

### SM5 · 操作面板 v2 + 播放时间控制（engine·debug）—— ✅ 已实现（2026-09-20，P1）
- 接口/行为: 面板 = 工具唯一完整操作入口——搜索框 / Gallery·Reset·Turntable 行 / **Pause·Step·0.25–2x**
  （写 `Time<Virtual>`，动画与转台一致冻结慢放，DB3 同机制下沉）/ 分类折叠树 / 资产卡键值表 / clip 浏览 /
  快捷键速查；新键 R 重置视角、T 转台；搜索框输入时快捷键挂起（PanelKeyboard）。
- 关键取舍: **虚拟时钟所有权**——影棚活跃时引擎的时间控制独占 `Time<Virtual>`，warden DB3 让位（否则 DB3
  每帧 unpause 解冻影棚暂停）。断言：暂停冻结/单帧回冻/速度写通/转台 yaw 递增/R 复位。

### SM6 · 搜索与资产卡（engine·debug）—— ✅ 已实现（2026-09-20，P1）
- 接口/行为: `SearchQuery`（名称+标签不区分大小写子串）；聚焦模式当前条目不再命中时**自动跳到首个命中**；
  画廊铺开跨分类平铺命中集；条目数据模型升级 `info_rows`（结构化键值）+ `tags` + `origin` 字段（N4 预留）。
- 断言: "mage" 命中聚焦法师塔、中文标签"弓箭手"可搜、画廊 "enemy" 标签 = 6 敌、清词回全类。

### SM7 · 来源呈现 + 组合拆解（engine+warden·debug）—— ✅ 已实现（2026-09-20，P2）
- 接口/行为: `Origin::Vault{id,version,role}/GameLocal` 上资产卡；warden 适配层解析 art-assets.json
  （dest 路径→岩库 id/名/版本；清单缺失优雅降级 GameLocal）；**弩塔组合显示两行来源**（塔身/床弩各带角色）；
  `parts` 声明组合部件——`showroom_parts` 系统按子实体出生序配对、面板点部件**独显该件**（其余隐藏），
  任何选择清除隔离。
- 断言: 弓箭手 origin=2 且 id/version 非占位（真清单解析）、融合塔 GameLocal、聚焦组合体带 PartsTag(2)、
  对比/选择后隔离清除。

### SM8 · 对比模式（engine·debug）—— ✅ 已实现（2026-09-20，P2）
- 接口/行为: 面板「Compare: A = current」→ 点任意条目为 B → 两条目左右同台（A/B 铭牌）；浏览键/选择退出。
  **边界**：目录内两条目对比（如两塔型对照）；**跨版本对比**（岩库 vN vs vN+1）需 art-sync 支持双版本同存，
  留 tools/ 线后续卡。
- 断言: 同台恰 2 件、A/B 铭牌齐、select 后回单件且 compare 清空。

### SM4 · 截图基线与对比流程（debug·工具链）—— 📝 待立卡（依赖 SM1，建议 SM2 后）
- 范围: 每展区固定机位 + 固定 seed，复用置前 + 窗口直截流程脚本化；基线图存 `docs/img/showroom/`；任何美术/
  特效改动后重截同机位对比（先人工对比；headless 渲染自动对比留作升格判据触发后再做，不过度设计）。

---

## 技能系统二期（通用技能框架 + 弓箭手塔试点，2026-09-29 立项）

> 架构依据 **ADR-0011**（按实体挂载的技能框架 + 统一伤害结算）。 Owner 已拍板：技能框架必须通用（魔兽三分法：
> 被动/主动/自动施法），首个使用者=弓箭手塔（出厂自带 1 被动 + 1 主动 + 1 自动施法，随 invest 升级 1→3 级），
> 指挥官三技能（SK1–SK4）**一并迁移**上框架作第二个使用者；伤害结算统一到单写入者管线。
> 框架与效果分离：框架只管生命周期（获得/升级/冷却/目标选择/校验/自动施法/UI），效果=按技能 id 分发的小系统。
> 实施顺序：**SK5 框架 → SK6 属性 → SK7 伤害 → SK8 指挥官迁移 → SK9 弓箭手技能组 → SK10 指令卡 UI → SK11 数值表**；
> SK1–SK4 原验收句全部保留作迁移回归护栏。数值一律进 SkillBalance 单源表（SK11 收编一期散落常量；循 ADR-0008 留代码内）。

### SK5 · 框架核心：SkillCaster 生命周期（skills）—— ✅ 已实现（2026-09-29）
- 接口: 新组件 `SkillCaster { slots: Vec<SkillSlot> }`（挂任意实体：塔/指挥官/未来角色）；`SkillSlot { def, level,
  cooldown, autocast }`（def=`SkillDefs` 注册表索引）；新资源 `SkillDefs`（`SkillDef { name, kind: Passive|Active|
  Autocast, targeting: None|Point|Unit, max_level, badge }`，结构元数据不含数值）；新消息 `CastSkill { caster, slot }`；
  目标模式资源泛化为 `SkillTargeting { caster, slot }`（沿用指示圈+左键确认+Esc/右键取消范式）。
- 行为: 冷却 tick 沿一期铁律（虚拟时间、Playing 门控→暂停/调试冻结同冻结、双相位均流动、跨波不重置）；施法校验
  （实体存活 / CD=0 / kind 非 Passive）；Point 技能经目标模式确认后触发效果，None 瞬发；learn/level_up API（等级
  封顶 max_level）。框架系统**不感知 owner 类型**（不 query Tower）。
- 验收句: headless 下挂 SkillCaster 的裸实体（无 Tower）冷却随帧递减、暂停冻结；CD 未好时 CastSkill 被拒且不进
  目标模式；Point 技能进入目标模式、Esc 取消不进 CD；level_up 越过 max_level 被钳制；Passive 槽位不可施放。
- 实现: `components.rs` SkillCaster/SkillSlot；`resources.rs` SkillKind/SkillTargetMode/SkillDef/SkillDefs 注册表 +
  SkillBalance 单源表骨架（本期只有 cooldowns: Vec<Vec<f32>>，SK11 收编全部散落数值）+ SkillAiming 资源；
  `systems/skills.rs` 新增 CastSkill（请求消息）/SkillCast（确认消息=效果分发缝）、`handle_cast_requests`（Input，
  校验+Point 进瞄准+None 即确认并起 CD）、`update_aim_picking`（泛化指示圈/确认/取消，沿用一期排序契约：
  before select_object + before 按钮/请求处理器）、`tick_caster_cooldowns`（Simulate）、`learn_skill`/`level_up`
  纯数据 API。**改名**：一期 `SkillDef/SKILL_DEFS/skill_def` → `CommanderSkillDef/COMMANDER_SKILL_DEFS/
  commander_skill_def`、一期瞄准枚举 `SkillAiming` → `CommanderSkillAiming`（为新框架让名，SK8 删旧路径）；
  过渡互斥：新请求清旧 SkillTargeting、旧按钮清新 SkillAiming、cancel_modes/select_object 同步认两者（SK8 全删）。
  `skills_core::*` 7 断言（冷却 tick+暂停冻结 / CD 门拒+不重启 / Point 进瞄准+Esc 免费取消+0 cast / 真实点击路径
  确认+起 CD / Passive+死实体+越界槽全拒 / level_up 封顶+re-learn 幂等 / 施法者消失瞄准自愈）。
  **踩坑**：消息断言用 `Messages::len()` 会在双缓冲翻转后归零——改用 `MessageCursor::default()` 增量读取
  （`bevy::ecs::message::MessageCursor`，不在 prelude）；与知识库 messages 坑笔记互证。

### SK6 · 属性修改管线：StatMods → EffectiveStats（skills/towers）—— ✅ 已实现（2026-09-29，依赖 SK5）
- 接口: 新组件 `StatMods(Vec<StatMod>)` 与 `EffectiveStats { damage, attack_speed, range, crit_chance,
  crit_multiplier }`；`StatMod { source, kind: Damage|AttackSpeed|Range|CritChance|CritMultiplier, add, mult,
  remaining: Option<f32> }`（source 标记身份，支持按来源增删与限时到期）；新系统 `aggregate_stats`（Simulate，
  tower_fire 之前）：基础值（Tower 字段）+ 全局 Boosts + WarFrenzy + 实体 StatMods → 写 EffectiveStats。
- 行为: 加算先乘算后（`final = (base + Σadd) × Πmult`）；到期条目摘除；`tower_fire` 改为只读 EffectiveStats
  （删内联 boost/frenzy 查询，行为等价迁移）；暴击默认 0% / ×2.0。
- 验收句: add+mult 叠加顺序正确（(10+3)×1.2=15.6）；同一 source 重复写入=刷新而非叠条目；限时条目到期后聚合值
  回落；光环语义（按 source 入场加/离场删）生效；迁移后无 Boosts/无 StatMods 时 EffectiveStats 与旧内联读数逐帧一致
  （回归对照既有 skills_sk4 攻速断言）。
- 实现: 新模块 `systems/stats.rs`（`aggregate_stats` / `tick_stat_mods` / `set_stat_mod` / `clear_stat_mod` /
  `fold`）；组件 `StatMods`/`StatMod`/`ModSource{Skill,Aura,Other}`/`EffectiveStats` 入 components.rs。
  **复用而非新造**：AC4 的 `StatKind` 扩展 CritChance/CritMultiplier 变体（共享词汇表；AC4 不提供暴击选项，
  input.rs/l10n.rs 补空分支）。注册进 TowersPlugin Simulate 链头：`tick_stat_mods → aggregate_stats →
  (原 tower 链不变)`；`tower_fire` 删 Boosts/WarFrenzy 内联读，全部改读 EffectiveStats（damage/range/攻速冷却
  1/estats.attack_speed）。取舍：选择环/调试射程环仍读 Tower.range 基础值——在某个被动真正加射程前不切换
  （SK9 弓箭手组无射程被动，无说谎场景；届时随卡切换）。`stats_pipeline::*` 5 断言（加乘顺序+默认暴击 /
  (source,kind) 刷新不叠 / 限时到期回落 / 光环按源增删 / Boosts+Frenzy 全局折算）；等价性护栏 =
  skills_sk4 全组原断言走真实路径通过（131 behavior 全绿）。

### SK7 · 统一伤害管线：DamageRequest 单写入者（skills/towers/projectiles/enemies）—— ▶ 待实现（依赖 SK6）
- 接口: 新消息 `DamageRequest { victim, amount, attack_type, source: DamageSource, crit }` 与
  `DamageDealt { victim, amount, attack_type, source, crit, killed }`；`DamageSource::{ Tower(Entity),
  CommanderSkill(usize), TowerSkill { tower, skill }, System }`；新系统 `resolve_damage`（Simulate，钉在全部写入点
  之后）：护甲结算（`effective_damage` 移入）+ 扣血 + HitFlash + 击杀判定 → 发 DamageDealt。
- 行为: 近战即时命中与投射物 `resolve_impact`（含流星火雨 Fixed 弹）改为写 DamageRequest；resolve_damage 成为
  **全游戏唯一写 Enemy.hp 的系统**；减速/状态仍走 ApplySlow 不进伤害管线；resolve_death/赏金路径不动；
  DamageDealt 供 FX / 归因 / BAL 读数消费。
- 验收句: 近战、远程弹、指挥官技能弹三源伤害都经同一结算（同帧多请求全结算无丢失）；物理甲减半仍生效（盾兵
  物理弹半伤、魔法弹全伤）；DamageDealt.killed 归因正确（致死一击才 killed=true）；击杀赏金数额与迁移前一致；
  既有全部 behavior 断言不回归。

### SK8 · 指挥官技能迁移上框架（skills/ui）—— ▶ 待实现（依赖 SK5；建议在 SK9 前用已知行为验证通用性）
- 接口: Startup 生成持久指挥官单例实体挂 `SkillCaster`（流星火雨/冰霜新星/战意狂暴三 Active）；Z/X/C 热键与
  控制区按钮改读该实体槽位；删除全局 `SkillState` 与 `SKILL_DEFS` 旧表（数值进 SkillBalance 由 SK11 收编，过渡期
  常量暂留）。
- 行为: 三技能效果系统改由 CastSkill 分发路径驱动（MeteorVolley 调度、ApplySlow 新星、攻速增幅——狂暴迁移为
  聚合管线读取的全局态或 StatMods 广播，取改动小者并在卡上注明取舍）；冷却/目标选择/热键/按钮交互语义与一期一致。
- 验收句: **SK1–SK4 全部原验收句保留并通过**（behavior 里 skills_sk1/sk2/sk3/sk4 组改走新驱动路径、断言不变）；
  指挥官实体上的 SkillCaster 冷却与全局资源制时代逐帧等价；热键/按钮/指示圈手感人工回归（⚠️）。

### SK9 · 弓箭手塔技能组：鹰眼/多重射击/火焰箭（skills/towers）—— ▶ 待实现（依赖 SK5/SK6/SK7）
- 接口: `SkillDefs` 注册三技能；建塔时 `spawn_base_tower` 附 SkillCaster（lv1）；invest 升级处同步 level_up 并刷新
  被动 StatMod 条目；效果系统按 id 分发。数值（CD/加成/箭数/时限）全部进 SkillBalance 起始值。
- 行为: **鹰眼**（Passive）暴击率 +10/15/20%、暴击倍率 ×2.0/2.25/2.5——tower_fire 从 EffectiveStats 掷暴击，
  快照 crit 标记与伤害进 Projectile → DamageRequest；**多重射击**（Active·None）CD 好后施放，朝射程内至多
  3/4/5 个目标各射一箭（复用 Projectile+Homing）；**火焰箭**（Autocast）CD 20s、时限 8s 普攻附加 +3/5/8 火焰伤害
  （限时 StatMod），autocast 开启时 CD 好且有目标自动重施。
- 验收句: 暴击期望伤害锚点（固定种子长序列均值落在 ±5% 理论带）；多重射击生成箭数=等级对应目标数且目标互不重复；
  火焰箭持续期内伤害加成可见、到期回落、autocast 自动重施间隔≈CD；invest 升级后三技能等级+1 且被动数值刷新。

### SK10 · 指令卡技能槽 UI（ui/command）—— ▶ 待实现（依赖 SK9）
- 接口: 选中塔时指令卡预留的两个置灰槽激活为技能格（徽章、等级点、CD 剩余秒数、tooltip 双语）；点击=施法
  （Point 技能复用指示圈流程）；右键=切换自动施放（魔兽式）；热键上下文限定（仅选中该塔时生效），按 §16.9 热键域
  分离分配（候选 D/F，避开 QWER/ASDF/ZXC/Space/P/L），选定后写回 requirements §16。
- 行为: 未选中/选中非技能单位时热键失效；冷却中点击无效果且显示剩余秒数；autocast 槽右键切换后有可视状态标记；
  语言切换重建后自愈（沿控制区同款范式）。
- 验收句: headless 断言选中态热键启停、按钮 CD 文本、autocast 切换翻转；视觉与点击走查人工验收（⚠️）。

### SK11 · SkillBalance 单源表 + BAL 技能贡献读数（skills/balance）—— ▶ 待实现（依赖 SK7）
- 接口: `resources.rs` 新 `SkillBalance`（按 技能×等级 键控，收编 SKILL_COOLDOWNS/NOVA_*/METEOR_*/FRENZY_* 与
  SK9 全部数值）；散落 const 删除、读取方全改走表；balance.rs 全程读数按 DamageSource 分组输出伤害占比
  （塔普攻/指挥官技能/塔技能三列）。
- 行为: 技能数值单源（循 skill-design-workflow v3 循环 B 约定：AI 调数值只动 SkillBalance）；BAL 读数复核
  「指挥官技能 ≤ 全场输出 20%」既有锚点，新立「塔技能伤害占比」锚点（目标区间按 BAL 方法论定，写 requirements §14）。
- 验收句: 仓库内不再有散落的技能数值 const（grep 可验）；BAL 全程读数能输出三列占比；弓箭手塔出场局中塔技能列
  与塔普攻列均非零。

---

## 观察通道约定

- **日志仪表**：`RUST_LOG=info cargo run` → 每 2 秒 `[dash] fps≈.. state=.. phase=.. gold=.. lives=.. wave=.. entities=..`（`src/plugins/debug.rs`）。
- **截图**：`F12` 存 `./screenshot.png`。
- **回归测试**：验收句转 `tests/behavior.rs`（headless App 驱动，见现有示例）。
- **瞬态打点**：FX/PR 类瞬态行为日志带毫秒时间戳（如 `[fx] hit-flash on/off`、`[pr] spawn/hit/despawn`），供毫秒级验收句对照。

## 踩坑记录规范

踩坑必记：现象 / 根因 / 解决 / 反思。落团队知识库 `docs/topics/<领域>/`（type: pitfall）。
