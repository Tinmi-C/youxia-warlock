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

### AC4 · 三选一（acquisition/ui）—— ✅ 已实现（简化）
- 接口: 输入 波结算 + 已有塔；输出 3 个选项（塔强化/金币/得塔），避重避已有。选择挂起时禁开下一波。
- 行为: 每波结算抽 3 个供选 1；给塔选项避开玩家已有塔；1/2/3 选择。
- 实现（2026-09-03）：`wave.rs::generate_choices`（确定性生成，占位）→ `input.rs::choose_choice` 应用；效果：塔型 +20% 伤害（`Boosts.damage_mult`）/击杀金币 +20%（`Boosts.kill_mult`）/获得塔（`Hand.owned_towers`）。结果塔不加成（结果塔型 no boost，占位）。`start_next_wave` 在 `WaveChoice.pending` 时停用。
- 验收句: 三选项互不相同；「得塔」选项不含已拥有塔；选中后效果生效（如 +20% 伤害）。已入 `tests/behavior.rs`。
- ⚠️ 占位：选项为**确定性生成**（非真随机）；塔强化只做「某塔型 +20% 伤害」（未做攻速/射程，也未按结果塔型加成）。随机化与精细加成是 polish 卡。

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

## 观察通道约定

- **日志仪表**：`RUST_LOG=info cargo run` → 每 2 秒 `[dash] fps≈.. state=.. phase=.. gold=.. lives=.. wave=.. entities=..`（`src/plugins/debug.rs`）。
- **截图**：`F12` 存 `./screenshot.png`。
- **回归测试**：验收句转 `tests/behavior.rs`（headless App 驱动，见现有示例）。

## 踩坑记录规范

踩坑必记：现象 / 根因 / 解决 / 反思。落团队知识库 `docs/topics/<领域>/`（type: pitfall）。
