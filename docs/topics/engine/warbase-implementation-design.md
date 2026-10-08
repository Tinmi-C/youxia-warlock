---
title: warbase 技术实现设计 v0.1——模块、类型、调度与抽取方案（暂名）
type: reference
topic: engine
date: 2026-10-08
author: AI 起草（ZCode），待团队 review
status: draft
tags: [architecture, runtime, warbase, bevy, implementation, design]
related: ["[[topics/engine/warbase-architecture|warbase 架构与演化协议]]", "[[decisions/0012-warbase-shared-runtime-and-map-architecture|ADR-0012]]", "[[topics/game-design/loop-td-game-proposal|循环圈 TD 需求草案 v0.2]]", "[[decisions/0011-generic-per-entity-skill-framework|ADR-0011]]", "[[topics/engine/unified-damage-pipeline|统一伤害结算管线]]", "[[topics/engine/warbase-test-strategy|warbase 测试策略]]"]
---

# warbase 技术实现设计 v0.1

> 架构决策见 [[decisions/0012-warbase-shared-runtime-and-map-architecture|ADR-0012]]，分层与演化协议见 [[topics/engine/warbase-architecture|架构文档]]，本文回答「具体怎么实现」：crate 与模块划分、核心类型、系统调度、MapBundle 结构、warden 抽取方案、按阶段（P0–P3 / BX 卡）的实施顺序，以及必须预埋的坑位。测试设计另见 [[topics/engine/warbase-test-strategy|测试策略文档]]。
> 文中代码为**设计级示意**（类型形态与职责边界），非最终 API；实现卡（BX 线）以本文为规格底稿，允许实现期微调并回写本文。

## 1. Crate 与依赖

```
youxia-warlock/Cargo.toml     # ★ 根 workspace：members=[engine/*, games/warden, games/loop-td]
                              #   exclude=[games/bevy-spike, games/m1-demo, games/m2-demo,
                              #             games/m2-bevy, games/wave-survival, templates/*]
                              # [workspace.dependencies]: bevy 0.19.1 / rand 0.10 / serde / ron 统一钉版

engine/warbase/
├── Cargo.toml      # lib-only（无 bin）；继承 workspace 版本；features = ["ui", "fx"]（默认开，可关=纯逻辑）
├── src/
│   ├── lib.rs          # WarbasePlugin（总装）+ WarbaseSet（调度契约）+ prelude
│   ├── unit/           # BX1：域模型
│   ├── combat/         # BX2：伤害管线 + 索敌 + 投射物
│   ├── skill/          # BX3：SkillCaster 框架抽取
│   ├── buff/           # StatMods 聚合 + 光环
│   ├── path/           # BX5：waypoint 链/闭环/圈数
│   ├── wave/           # BX6：波次导演（Auto/Manual）
│   ├── econ/           # BX7：金/木/人口
│   ├── rule/           # BX8：漏怪策略 + 建造窗口策略 + 胜负
│   ├── build/          # 建造域：自由网格 / 固定位双模式
│   ├── fx/             # 通用战斗 FX（血条/闪白/死亡消散，feature "fx"，warden FX1–3 泛化）
│   ├── ui/             # BX4：命令卡框架（feature "ui"）
│   ├── bundle/         # BX9：MapBundle serde 类型 + RON 装载/校验 + 装配器（ADR-0013）
│   └── harness/        # BX10：headless 打局 + 锚点读数
└── tests/              # 机制级集成测试（不依赖任何游戏，见测试策略文档）
```

- 依赖：`bevy + rand + serde + ron`（bundle 文件化新增后两项，workspace 统一钉版）。**禁止**依赖任何 games/、任何美术资产、任何具体数值（防腐化宪法，ADR-0012）。
- 边界策略：v1 单 crate 多模块 + feature 门控（`cargo build -p warbase --no-default-features` = 纯逻辑基底）；模块间只经 prelude 导出的类型与 Message 交互；第三消费方或编译痛感出现再拆 crate。
- 游戏接入（workspace 成员）：依赖声明用 `workspace = true` 继承版本；`build_app()` 中 `add_plugins(WarbasePlugin)` 再加游戏 `GamePlugin`（注册挂钩、指定 `maps/*.ron`）。

## 2. 调度契约：WarbaseSet

基底与游戏的系统顺序靠 SystemSet 契约保证（对应 warden GameSet 思路 + 统一伤害管线的 GameSet::Resolve）：

```
WarbaseSet::Tick        // 冷却/计时推进、Buff 到期、EconCommand 处理
WarbaseSet::Director    // 波次导演发怪、路径推进（写 Transform 由采样派生）
WarbaseSet::Intent      // 索敌、技能触发判定、自动施法
WarbaseSet::Act         // 开火、投射物飞行、施法生效（只发 DamageRequest / BuffRequest）
WarbaseSet::Resolve     // resolve_damage：全场唯一写 Hp 的结算系统
WarbaseSet::Cleanup     // 死亡结算（赏金/经验/存量-1）、漏怪判定、胜负检查、发事件
```

规则：Resolve 单写入者（ADR-0011 第 4 层原则原样继承）；游戏系统用 `before/after` 插入（如 FX 挂 `after(Resolve)`、命令卡 UI 挂 `after(Cleanup)` 读状态）。测试对调度敏感的断言按 Set 分帧（Messages 双缓冲坑：跨帧断言逐帧 drain）。

## 3. 模块设计

### 3.1 unit（BX1）——统一单位域模型

塔/怪/英雄塔**不设硬编码类别**，全部由组件组合表达（ECS 惯用形态，也即基底通用性的来源）：

```rust
// 定义侧（进 MapBundle，数值全在游戏）
pub struct UnitDef {
    pub id: UnitId,
    pub attrs: BaseAttributes,        // hp, move_speed, body_radius, footprint
    pub attack: Option<AttackDef>,    // damage, interval, range, targets(Ground/Air/Both), proj
    pub attack_type: AttackType,      // Normal/Piercing/Siege/Magic/Hero
    pub armor_type: ArmorType,        // Light/Medium/Heavy/Fortified/Hero/Divine
    pub bounty_gold: u32, pub bounty_wood: u32, pub grant_xp: u32,
    pub traits: Vec<UnitTrait>,       // Fast/Healer/AuraBearer/Air/Resistant/...
}

// 实例侧（组件）
Hp(f32)                    // 唯一写入者 = resolve_damage
UnitInstance { def: UnitId, level: u8 }
PathFollower { .. }        // 有 = 会走路的怪；无 = 静态塔
XpHolder { xp, level, points }   // 有 = 英雄塔（吃经验、可加点）
SkillCaster { slots }      // 有 = 带技能（ADR-0011 原样）
Buildable { .. }           // 有 = 建造物（占地、可卖）
```

「英雄塔 = UnitInstance + XpHolder + SkillCaster 的组合」是基底通用性的第一块试金石（loop-td HR 卡只组装不新写）。

### 3.2 combat（BX2）——伤害管线 + 索敌 + 投射物

- 从 warden SK7 抽取：`DamageRequest { target, source, amount, attack_type, crit }` → `resolve_damage`（读护甲类型 × 攻甲系数表 → 扣血 → HitFlash → 击杀判定）→ 发 `DamageDealt { target, source, amount, crit, killed }`。泛化点：warden 的 `Enemy.hp` 直写改为通用 `Hp` 组件；攻甲系数从写死物理减伤改为查 bundle 的 `AttackArmorTable`。
- 索敌：`TargetingPriority { First, Last, Strongest, LowestHp, Nearest }`，挂在 AttackDef 上。循环圈语义：`First/Last` 按**路径进度**排序（进度=威胁），这是 path 模块暴露 `progress` 的直接收益；迷宫/直线图同一语义成立。
- 投射物：warden PR1/PR2 的追踪弹泛化（速度/边缘行为表驱动）。

### 3.3 skill（BX3）+ buff

- SkillCaster / SkillSlot / SkillDefs / SkillBalance 结构与三分法（Passive/Active/Autocast）**原样抽取**（ADR-0011 已按实体挂载设计，本来就无游戏概念）。效果分发（按技能 id 的小系统）模式保留：基底提供框架与常见效果库（伤害/治疗/减速/减甲/召唤？——v1 只做伤害与 StatMods 类），地图特有效果走游戏侧效果系统注册。
- buff：StatMods（source 标记、限时、增删）→ EffectiveStats 聚合管线原样；新增**光环**（ADR-0011 已预留「入场加条目/离场删条目」）：

```rust
AuraSource { def: AuraId, radius: f32 }   // 挂塔
// system: enter → 给目标加 source=本塔的 StatMods 条目；exit → 删；AuraState 记录在场集合
```

### 3.4 path（BX5）

```rust
pub struct PathDef {
    pub points: Vec<Vec2>,        // 折线 waypoint
    pub closed: bool,             // true = 环（走完尾点回到首点）
    // 分支/汇入 junction 预留：迷宫/多圈用，v1 只留字段位不实现
}
PathFollower { path: Entity, progress: f32, laps: u32, lap_buffed: u32 }
// 推进系统：progress += effective_move_speed * dt；closed 时 progress 绕回 → laps += 1 → 发 LapCompleted { unit, lap }
// 位置系统：progress 采样折线 → 写 Transform（移动学运动学，不用物理引擎）
```

- `LapCompleted` 事件是循环圈的接缝：规则模块消费它做绕圈增益，游戏侧也可消费（挖宝/特殊触发）。
- 环长 API：`PathDef::length()`、`sample(progress)` 纯函数化，便于 L0 单测。

### 3.5 wave（BX6）

```rust
WaveEntry { spawns: Vec<SpawnSpec /*def, count, spawn_interval, path 赋值*/>,
            preview: PreviewInfo /*构成+特性预告文案键*/, boss: Option<BossTimerSec> }
WaveTable { waves: Vec<WaveEntry>, advance: Advance /*Auto(interval)/Manual*/, early_call: Option<RewardCurve> }
WaveDirectorState { current, phase: Spawning(t)/InterWave(t)/Cleared/Done }
// 事件：WavePreview(n)（InterWave 开始即发，备战输入）、WaveStarted、WaveCleared、BossTimedOut
```

- 自动推进 + 付费提前召唤（需求 D1 推荐项）都进基底 Advance 参数；warden 式手动开波 = `Advance::Manual`，同一导演两种用法——这是「基底先行、双消费方已在假想名单里」的样板。

### 3.6 econ（BX7）

```rust
Resources { gold, wood }   Population { used, cap }
// 一切变更走 EconCommand 消息：Spend/Grant/Convert(rate←rules)/BuyPop(cost curve←rules)
// 事件：EconChanged（UI 刷）、EconRejected（钱不够的 UI 反馈）
```

命令式单写入者（与伤害管线同构），headless 读数与作弊面板（DB1 式）免费获得。

### 3.7 rule（BX8）——可插拔压力与胜负

```rust
pub enum LeakPolicy {
    Lives { lives: u32 },          // 经典：漏怪（走完路径）扣命 —— warden(10)/迷宫用
    Stockpile { cap: u32 },        // 循环圈：漏怪留场续走，活怪数 ≥ cap 判负
    PassToNeighbor,                // 水TD：漏给下家（多人，v2 占位不实现）
}
pub enum BuildWindow { Always, PrepOnly }   // warden 双速节奏 = PrepOnly（波中锁建造）；循环圈 = Always
MatchRules { leak, build_window, lap_buff: Option<LapBuffPolicy /*每圈 +speed%/+hp%*/>,
             win: SurviveWaves(n)/Endless, boss_timeout_default, sell_refund_pct }
MatchFlow { Running/Won(reason)/Lost(reason) }   // + WonEvent/LostEvent
```

warden 全参数（Lives10 + PrepOnly + 手动开波 + 单币）就是本模块的第一组回归消费者——rule 的通用性由回迁直接证明，不靠假想。

- 存量计数维护在 Cleanup：spawn +1、death −1（事件驱动，不做每帧全量 count）。
- `Lost(reason)` 携带归因（存量满/命尽/BOSS 超时）+ 「存量曲线」采样进 harness 报告——直接支撑需求里「失败可读」的结算屏。

### 3.8 build

```rust
pub enum BuildDomain {
    FreeGrid { cell_size, bounds },        // 循环圈：全图网格自由放置
    FixedSlots { slots: Vec<IVec2/*或世界坐标*/> },   // warden：固定 8 塔位，占满即无位
}
BuildGrid { domain, occupancy: HashMap<SlotKey, Entity> }   // blocked（路径/金矿占格）由 world 定义初始化
try_place(footprint, slot) -> Result<..>                    // 越界/压路/压塔/压 blocked/无空闲位 五类拒绝
```

幽灵预览（可放/不可放着色）与放置提交走同一校验函数（warden UI3 经验的泛化）；**校验在基底、美术在游戏**。双模式由 bundle 选择——warden 固定位与循环圈自由放各证一边，是「玩法差异表达在数据层」的又一实例。

### 3.9 bundle（BX9，RON 数据文件——ADR-0013）

```rust
// serde 类型即 schema；RON 文件是唯一正式形态（games/<game>/maps/<map>.ron）
#[derive(Deserialize)]
pub struct MapBundle {
    pub identity: MapIdentity,            // id + version（装载器做兼容检查，不静默迁移）
    pub world: WorldDef,                  // 场景挂钩 id、PathDef 列表、BuildDomain、blocked、资产引用路径
    pub units: UnitRegistry,              // id -> UnitDef（塔/怪/英雄塔全在此）
    pub skills: SkillDefs, pub auras: AuraDefs, pub talents: TalentDefs,
    pub waves: WaveTable,
    pub rules: MatchRules,
    pub econ_start: EconStart,
    pub attack_armor: AttackArmorTable,   // 系数矩阵随图发布（基底只有结构与查表）
    pub hooks: Vec<HookBinding>,          // 事件接缝 → 游戏 effect 系统（按 id）
    pub meta: MetaDefs,
}
// 装载器 load(path) -> Result<MapBundle, MapError>：
//   RON 解析 → schema 校验（必填/枚举）→ 引用存在性（技能 id/资产路径/挂钩 id 悬空即拒载，报可读错误）
// 装配器 assemble(world, bundle)：解析挂钩 id → 注册表取实现 → 插资源/注册表/静态实体
//   重开局 = despawn 场上实体 + 原地重置资源（沿 bevy-ecs-world-reset-pitfall：绝不重建 World）
// 挂钩注册表（游戏启动时注册，bundle 只写 id）：
//   SceneSpawners: id -> 场景构造 fn    MapEffects: id -> 小系统    CustomCommands: id -> 命令处理器
```

### 3.10 ui（BX4）+ fx

**命令卡框架**：warden UI9–UI12 的泛化，框架持有**三区底栏布局、五态切换、QWER/ASDF Grid 热键、Tooltip 结构、选中态管理**；卡片内容数据化：

```rust
CommandCardDef { slots: Vec<CommandSlot> }   // per UnitDef，进 bundle
CommandSlot { icon, hotkey, tooltip: TooltipSections, action: CommandDef }
CommandDef::Build(..)/Upgrade(..)/Sell/Cast(skill)/LearnTalent(..)/Custom(hook_id)
```

Custom 命令走挂钩 id + 消息（不收回调——可测试），处理器由游戏注册。**通用战斗 FX**（血条/闪白/死亡消散，warden FX1–3 泛化）为独立 feature `fx` 的可选插件，数据驱动（颜色/时长随 def）；重特效（技能大招演出）归游戏侧消费 DamageDealt/技能事件自做。

### 3.11 harness（BX10）

```rust
pub trait DriverStrategy {                  // BAL2 驱动策略的泛化
    fn decide(&mut self, view: &HarnessView) -> Vec<DriverAction>;  // 建塔/升级/加点/施放/提前召唤
}
pub fn run_headless(bundle: &MapBundle, driver: &mut dyn DriverStrategy,
                    opts: HarnessOpts /*max_frames/seed/注入*/) -> HarnessReport
// HarnessReport { outcome, waves_reached, stock_peak&曲线, damage_by_source, gold_curve, anchor_readings }
```

warden `tests/balance.rs` 的读数逻辑改写为首个内置驱动（「锚点策略」：自动布阵+真施放）；注入（A2 的「硬塞 20 存量」）走 `HarnessOpts::inject`。种子化 rand（单一 `MatchRng` 资源，游戏侧 AC 卡的随机全部走它）保证报告可复现。

## 4. warden 抽取 + 回迁方案（通道 A，成对卡 BX→WX）

每件抽取与 warden 切换消费**成对成卡**：BX 卡（进基底）先行，WX 卡（warden 删本地实现、引 warbase、断言语义不变）紧随，一对收口后 warden 回归全绿才进下一对。

| 对 | 件 | BX 抽取动作 | WX 回迁动作 | 风险与对策 |
|----|----|--------------|-------------|------------|
| 1 | 伤害管线（SK7） | Enemy.hp 泛化为 Hp、攻甲查表化 | 删本地 resolve/投射物，引 combat | 断言语义不变；BAL 塔/技能/指挥官归因读数对齐 |
| 2 | SkillCaster（SK5–11） | 近乎原样（本就无游戏概念） | 删本地框架，SkillBalance 表迁入 warden.ron | 消息双缓冲断言迁移沿测试助手 |
| 3 | 命令卡（UI9–12） | 泛化量最大：布局/热键/五态框架 + def 数据化 | 命令内容改由 bundle 提供 | 超一张卡即拆 BX4a/BX4b |
| 4 | FX（FX1–3） | 泛化为 feature "fx" 可选插件 | 切换消费 | 表现层，视觉基线重拍验证 |
| 5 | BAL（BAL1/2） | 重写为 harness DriverStrategy | warden balance.rs 改调 harness，锚点读数不漂移即验收 | 读数口径先对齐再迁移 |

非抽取件的回迁（WX6「warden 参数化」）：econ/wave(Manual)/path(开链)/rule(Lives10+PrepOnly)/build(FixedSlots) 切到基底参数化实现 + defs 全量迁入 `games/warden/maps/warden.ron`；warden 特有玩法（商店/手牌/三选一/Meta）保留为游戏侧插件挂事件接缝。回迁系列完成后 warden 的 160 断言与 BAL 两锚点即基底最重的回归压舱石。

抽取纪律：只抽已 accepted 且回归绿的件；一对卡不与 warden 功能卡并行；**WX 系列整体插在 warden ART9/10 之前**（先稳底座再上美术）。

## 5. 实施顺序（P0–P3 ↔ 增量；编号沿用 BX/WX 为增量名，流程按 ADR-0014 规格-测试-小步提交）

| 阶段 | 卡 | 出口（可验收句见测试策略文档） |
|------|----|-------------------------------|
| P0 | **WS0 根 workspace 接线**（members/exclude/统一钉版；warden 与旧项目在新结构下照常 `cargo test` 全绿） | `cargo test --workspace` 一击跑通（此时 = warden + 旧成员） |
| P0 | TS0 确定性基建（种子 Rng/test_app/drain 助手/临时文件隔离） | 助手单测绿 |
| P0 | BX0 crate 落位（feature 门控；防腐化：`cargo build -p warbase --no-default-features` 零游戏依赖） | 基底测试骨架绿 |
| P0 | BX1 unit 域模型 + 攻甲表 / BX2+WX1 伤害管线对 / BX3+WX2 技能框架对 | warden 回归全绿（WX1/WX2 收口） |
| P0.5 | BX9a bundle serde 类型 + RON 装载/校验器（先用最小 bundle 冒烟） | 拒载用例绿（坏文件/悬空引用/版本不符） |
| P1 | BX5 路径闭环 / BX6 波次导演 / BX8 规则插拔 / BX7 资源 / build 双模式 | 循环圈核心闭环可打（游戏卡 MA1/EN1/EN2/TO1/TO2/WA1 同步消费） |
| P1.5 | WX3 命令卡对 / WX4 FX 对 / WX6 warden 参数化 + maps/warden.ron | **warden-on-base 达成：160 断言全绿 + BAL 读数不漂移**（基底验收 #0） |
| P2 | buff 光环 / talents / BX10 harness（WX5 BAL 对） | 功能塔、英雄塔三层、命令卡；BAL A1–A3 首跑 |
| P3 | BX9b 装配器收口 / loop.ron + maze-smoke.ron / SMK1 迷宫冒烟（`--map` 加载演示）/ Meta / 无尽 | 基底验收 #1–#4 全过（架构文档 §七） |

原则：BX 与消费（WX 或游戏卡）同卡成对；RON 装载层在 P0.5 先行（后续所有内容直接以文件形态进，避免「先代码后搬家」二次成本）；warden 美术卡（ART9/10）排在 WX 系列之后上新底座做。

## 6. 必须预埋的坑位（本仓已验证的坑，设计期直接规避）

1. **World 重开**：重开局 despawn + 资源原地重置，绝不重建 World（bevy-ecs-world-reset-pitfall）；bundle 装配器按此写。
2. **Messages 双缓冲**：跨帧断言逐帧 drain 累加；`len()` 不当累计器（bevy-messages-double-buffer-drain-pitfall）；harness 报告中的事件计数全部走逐帧累计。
3. **系统参数上限**：元组 ≤16、SystemParam derive 不收 mut Query（bevy-system-param-limits-pitfall）；基底系统按 Set 职责切小，单系统参数 >12 即拆。
4. **文本重排**：UI 刷新走 cache-key 比较后再写（warden macOS CPU 坑）；命令卡框架内置 dirty-check。
5. **场景重建杀接线**（bevy-scene-rebuild-kills-anim-wiring-pitfall）：loop-td 美术接线沿 warden 自愈式方案，基底不持有美术。
6. **测试隔离**：meta 存档/偏好文件每测试唯一临时路径并先删（warden UI5 测试污染坑），基底 harness 的存档路径参数化。
7. **确定性**：全部随机走种子化 `MatchRng` 资源；帧推进用固定 dt（DB3 时间控制思路），断言不含墙钟。

## 7. 本设计的开放点（实现卡时逐个收口）

- ~~CommandDef::Custom 的注册形态~~ → **已定**：挂钩 id + 消息，处理器游戏侧注册（ADR-0013）。
- Heal 走 DamageRequest 负值还是独立 HealRequest——BX2 时定（倾向独立，语义干净、读数不混）。
- 人口计数口径（塔占地格数 vs 塔座数）——BX7 时随 loop-td 经济数值一起锚定。
- 光环对敌我双方的过滤表达（affects: OwnSide/Enemies/All）——buff 模块接口预留。
- PathDef 的 junction（分支）——**已定**：serde 字段位保留、不实现（避免 RON schema 将来 breaking）。
- warden 特有玩法件（商店/手牌/三选一）回迁后挂靠的事件接缝清单——WX6 立卡时逐个登记。
