---
title: warbase 架构与演化协议——魔兽式游戏基底的分层、落位与三通道上收（暂名）
type: reference
topic: engine
date: 2026-10-08
author: AI 起草（ZCode），待团队 review
status: draft
tags: [architecture, runtime, warbase, bevy, multi-game]
related: ["[[decisions/0012-warbase-shared-runtime-and-map-architecture|ADR-0012]]", "[[topics/engine/warbase-implementation-design|warbase 技术实现设计]]", "[[topics/engine/warbase-test-strategy|warbase 测试策略]]", "[[topics/game-design/loop-td-game-proposal|循环圈 TD 需求草案（第一张验证地图）]]", "[[topics/game-design/wc3-rpg-td-map-genres|魔兽 RPG 塔防品类调研]]", "[[decisions/0011-generic-per-entity-skill-framework|ADR-0011]]", "[[decisions/0010-showroom-generic-asset-viewer|ADR-0010]]"]
---

# warbase 架构与演化协议——魔兽式游戏基底的分层、落位与三通道上收

> **本文回答三个问题**：基底在代码库里长什么样（分层与落位）；后续新游戏怎么以它为基底开发（消费方式）；新游戏开发如何反过来完善基底（演化协议）。决策记录见 [[decisions/0012-warbase-shared-runtime-and-map-architecture|ADR-0012]]（proposed），第一张验证地图的需求见 [[topics/game-design/loop-td-game-proposal|循环圈 TD 需求草案]]。**warbase 为暂名，正式名待拍板。**

## 结论

基底 = `engine/warbase` 一个共享 crate（Bevy 之上、游戏之下），复刻魔兽「引擎宿主 / 对象数据 / 触发编排」的三层分离：**机制与管线归基底，内容与数值归地图 bundle（RON 数据文件，ADR-0013），特殊规则归游戏侧薄插件（挂钩 id）**。依赖永远单向 games → warbase → bevy；根 workspace 统一版本与一键全量回归；**warden 回迁为消费方 #0**（抽取与回迁成对卡，160 断言全绿作验收），loop-td 为消费方 #1 与新机制驱动。基底靠三条通道成长：warden 已验证件抽取+回迁（通道 A）、新机制基底先行+第一消费方验收（通道 B）、无把握机制游戏侧孵化待双消费方再上收（通道 C）。每张新地图同时是基底的一次验收：如果新地图必须改基底核心才能实现，就是抽象有洞的信号。

## 一、分层模型（魔兽 ↔ 本工作区）

| 魔兽生态 | 本工作区对应 | 内容 |
|----------|--------------|------|
| 魔兽争霸引擎（图形/ECS/资源） | **Bevy 0.19.1**（ADR-0004） | 渲染、场景、输入、UI 基元 |
| 对战中台：单位/攻甲/技能/Buff/金木人口/触发运行时 | **`engine/warbase`（本架构的主体）** | 战斗 runtime：单位域模型、伤害管线、技能框架、Buff/光环、路径、波次导演、资源、规则、命令卡、bundle 装配、headless harness |
| 地图 = Object 数据 + Trigger 脚本 + 地形 + 资产 | **`MapBundle`（RON 数据文件）+ 游戏侧挂钩注册 + 薄 GamePlugin** | 一张玩法 = bundle 数据文件 + 波次表 + 规则参数 + 按 id 注册的特殊规则 |
| 千张 RPG 地图 | `games/<name>` 各项目 | warden（回迁）→ 循环圈（第一张新图）→ 迷宫 → 宝石 → 军团 → 守卫 RPG |
| World Editor 工具链 | （长期）工具线：showroom/art-sync 已有；RON 装载校验器先落，编辑器远期 | 换图不换宿主：`--map maps/xxx.ron` |

## 二、代码落位（monorepo 结构）

```
youxia-warlock/                 # ★ 根 cargo workspace（ADR-0012 v2）：单 lockfile、共享构建缓存
├── Cargo.toml                  # [workspace] members=[engine/*, games/warden, games/loop-td]
│                               #   exclude=[旧演示项目, templates/bevy-game]
│                               # [workspace.dependencies] 钉 bevy 0.19.1 / rand 0.10，成员 workspace=true 继承
├── engine/
│   ├── showroom-viewer/        # 已有（ADR-0010）：美术资产查看器
│   └── warbase/                # ★ 新增：战斗基底 crate（UI 以 feature "ui" 门控，可关=纯逻辑）
│       ├── src/
│       │   ├── unit/           # UnitDef/实例组件、属性、攻-甲类型表、经验等级
│       │   ├── combat/         # DamageRequest→resolve 伤害管线、索敌、投射物
│       │   ├── skill/          # SkillCaster 框架（ADR-0011 原样上收）
│       │   ├── buff/           # StatMods→EffectiveStats 聚合、光环入场/离场
│       │   ├── path/           # waypoint 链：开放链/闭环/分支汇入（迷宫与多圈预留）
│       │   ├── wave/           # 波次导演：表驱动发波/预告/自动推进(Auto/Manual)/提前召唤/限时
│       │   ├── econ/           # 金/木/人口三资源 + 兑换/购买事件流
│       │   ├── rule/           # 漏怪策略（扣命/存量/漏下家）+ 建造窗口策略 + 胜负
│       │   ├── build/          # 建造域：自由网格 / 固定位双模式（warden 8 塔位 vs 循环圈自由放）
│       │   ├── fx/             # 通用战斗 FX：血条/闪白/死亡消散（warden 件泛化，feature "fx"）
│       │   ├── ui/             # 命令卡框架（三区底栏/Tooltip/Grid 热键，feature "ui"）
│       │   ├── bundle/         # MapBundle serde 类型 + RON 装载/校验器 + 装配器（ADR-0013）
│       │   └── harness/        # headless 自动打局 + 锚点读数（BAL 方法论固化）
│       └── tests/              # 基底机制级测试（不依赖任何游戏）
├── games/
│   ├── warden/                 # 消费方 #0：回迁上新底座（WX 卡系），160 断言全绿作验收
│   │   └── maps/warden.ron     # warden 玩法全部参数化成 bundle
│   ├── loop-td/                # 消费方 #1：第一张新地图 = 新机制驱动 + 验收场
│   │   └── maps/{loop.ron, maze-smoke.ron}   # 迷宫冒烟也是一张 RON 地图
│   └── <future>/               # 后续玩法：同模式接入
├── templates/bevy-game/        # 保持裸模板（workspace exclude；小项目/学习用）
└── tools/ …                    # 不变
```

模块即 v1 边界：单 crate 多模块 + feature 门控（`ui`/`fx` 可关，`cargo build -p warbase --no-default-features` = 纯逻辑基底，harness/CI 快跑），第三消费方或编译痛感出现再拆 crate。游戏接入：workspace 成员继承依赖版本 + `build_app()` 装配 `WarbasePlugin` + 游戏 `GamePlugin`（注册挂钩、指定 maps/*.ron）。

## 三、MapBundle：「一张地图」的数据形态

```rust
MapBundle {
    identity:  地图 id / 版本,
    world:     地形场景引用 + 路径拓扑（waypoint 链，闭环/分支标记）+ 建造域（网格/固定位）,
    unit_defs: 塔/怪/英雄塔定义（属性、攻击类型、护甲类型、特性标记、经验值、对空对地）,
    skill_defs / buff_defs / talent_defs,
    wave_table: 波次序列（组成、间隔、特性预告、BOSS 限时）,
    rule_set:  漏怪策略、资源初值、人口上限、胜负条件、推进方式,
    meta_defs: 局间成长项,
}
```

三条纪律：

1. **bundle 以 RON 文件为唯一正式形态**（ADR-0013）：serde 类型即 schema；装载器做校验（必填/枚举/引用存在性——技能 id、资产路径、挂钩 id 悬空即拒载并报可读错误）+ 版本兼容检查；**不静默迁移**，宁可拒载。ADR-0008 适用范围随之收窄：地图内容数值进 bundle 文件，基底机制参数仍在代码内。
2. **基底零数值**：warbase 只有机制与管线（「波次导演」怎么跑），攻甲系数表、波次公式、塔数值全部在地图 bundle（「第 7 波是什么」）。检验句：把任何游戏的 maps/ 删掉，warbase 编译与测试照常全绿。
3. **特殊规则走挂钩 + 薄插件**：地图独有的编排逻辑（循环圈的「金矿可打爆」「挖宝时机」这类）= bundle 里写挂钩 id + 游戏侧按 id 注册的小系统，挂在基底事件接缝上（`WaveStarted` / `UnitLeaked` / `LapCompleted` / `BossTimedOut` …）。这是魔兽 Trigger 的 v1 对应物；「非程序员做图/热更新」成为真需求再重开 ADR 评估脚本化。

## 四、消费方式：后续新游戏怎么开发（以迷宫 TD 为例）

1. **缺口评估先行**（新地图立项的第一个动作）：列新玩法需要的机制 × 基底现状，分三类——直接用（路径开放链、伤害、技能）/ 参数化可用（漏怪策略换成扣命、建造域换网格自由）/ 缺口需新建（宝石 TD 的随机合成系统）。
2. 从模板复制项目 + 加 warbase 依赖 + 写 bundle + 薄 GamePlugin。
3. 缺口里的通用机制走通道 B/C（见下节），地图专属机制留在游戏 crate。
4. 验收 = 游戏行为回归 + 基底全消费方回归双绿。

预期效果（基底成熟后）：新玩法的增量收敛为「一张 bundle 数据表 + 少量特殊规则系统」，这正对应魔兽「一张地图 = 数据 + 触发器」的产能结构。

## 五、演化协议：新游戏开发如何完善基底（三通道）

| 通道 | 触发条件 | 动作 | 本批实例 |
|------|----------|------|----------|
| **A 抽取 + 回迁** | warden 已验证稳定件，第二消费者出现 | **成对卡**：BX 抽取泛化进基底 + WX 让 warden 切换消费（删本地实现），warden 160 断言语义不变全绿 + BAL 读数不漂移 = 抽取忠实性验收 | 伤害管线、SkillCaster 框架、命令卡、FX、BAL harness |
| **B 基底先行** | 新地图需要的**通用**机制（多玩法都会用） | 直接按基底规格在 warbase 实现，第一消费方当验收场 | 闭环路径、波次导演、金木人口、可插拔漏怪/建造窗口规则、建造域双模式、RON bundle 装载器 |
| **C 游戏侧孵化** | 无把握是否通用的机制 | 先在游戏侧实现但留接缝（泛型组件/事件），双消费方出现再上收（ADR-0010 判据） | 循环圈的「绕圈增益」「金矿」；未来宝石 TD 的合成系统 |

配套纪律：

- **防腐化宪法**：依赖单向 games → warbase → bevy；warbase 内禁止出现具体游戏概念（塔名/怪名/数值/路径拓扑）；`cargo build -p warbase` 不需要任何游戏。
- **breaking change 代价**：基底 API 早期会晃，游戏侧用**集中式 bundle 装配文件**（每游戏一个 `bundle.rs`）隔离变化面；基底 breaking 改动与消费方迁移同卡完成。
- **抽象有洞信号**：新地图必须改基底核心（而非加 bundle 参数/新规则实现）→ 停下复盘：能参数化的塞 rule_set，确属通用的进基底且要过「三个假想消费者」（迷宫 / 军团 / 守卫 RPG）预演，并补 ADR。
- **成熟度刻度**：L0 接缝预留 → L1 单游戏验证（loop-td，含最小迷宫冒烟）→ L2 双游戏验证、通道 A/B/C 跑通闭环 → L3 多地图（换 bundle 即换玩法可演示）。基底卡（BX 线）按此排期。

## 六、基底能力清单与现状映射

| 能力 | warden 现状 | 进基底方式 | 通道 |
|------|-------------|------------|------|
| 统一伤害管线 | ✅ 已验证（SK7） | 抽取 + warden 回迁 | A |
| SkillCaster 技能框架 | ✅ 已验证（ADR-0011，SK5–SK11） | 抽取 + warden 回迁 | A |
| 命令卡 UI 框架 | ✅ 已验证（UI9–UI12） | 抽取泛化 + warden 回迁 | A |
| 通用战斗 FX（血条/闪白/死亡） | ✅ 已验证（FX1–FX3） | 抽取泛化（feature "fx"）+ warden 回迁 | A |
| headless 打局 + 锚点读数 | ✅ 已验证（BAL1/BAL2） | 重写为 harness + warden 读数口径对齐 | A |
| 单位域模型（统一 UnitDef + 攻甲类型表） | 部分（EnemyDef/TowerDef 分离、物理护甲是退化形态） | 合并泛化重建，warden defs 迁入其 bundle | B |
| 路径（闭环/分支） | 部分（L 开链） | 扩展重建；warden 开链即首个消费者 | B |
| 波次导演（Auto/Manual/预告/限时） | 部分（手动开波） | 扩展重建；warden Manual 模式即首个消费者 | B |
| 金/木/人口资源 | 部分（单币） | 扩展重建；warden 单币（wood=0）即首个消费者 | B |
| 可插拔压力/胜负 + 建造窗口规则 | 部分（扣血/锁建造写死） | 参数化重建；warden 参数（Lives10/PrepOnly）即首个消费者 | B |
| 建造域（自由网格/固定位） | 部分（固定位 8 塔位） | 双模式设计；warden 固定位 + 循环圈自由放各证一边 | B |
| Buff/光环 | 部分（StatMods 有，光环无） | 补全 | B |
| MapBundle RON 装载/校验/装配 | 无 | 新建（ADR-0013） | B |
| 绕圈增益、金矿、挖宝等 | 无 | 游戏侧孵化 | C |

warden 侧一个关键点：其「固定 8 塔位、双速锁建造、单币、L 开链、漏怪扣血」**全部**是基底参数而非基底例外——回迁即证明这些玩法差异表达在 bundle 数据层，这是基底通用性最有分量的一份实证。

## 七、基底验收：要替基底证明什么

0. **warden-on-base**（通道 A 的忠实性证明，最先达成）：warden 全量回迁上新底座，160 条既有断言语义不变全绿，BAL 两锚点读数不漂移；其玩法差异（固定位/锁建造/单币/L 路径）全部落在 bundle 参数。
1. **薄游戏验证**：循环圈全部核心玩法在 loop-td 侧只有「bundle 数据 + 薄编排」——游戏 crate 中无伤害计算、无技能框架实现、无波次推进逻辑（review 检查句：游戏 src 里 grep 不到 `hp -` 直写与冷却实现）。
2. **headless 通用性**：warbase harness 跑任意 bundle（warden / loop-td / 冒烟）自动打局出读数（BAL 方法论换地图照样成立）。
3. **多玩法最小实证**：同一基底装配「最小迷宫 TD 冒烟」——且以 **`loop-td --map maps/maze-smoke.ron`** 的形式演示：同一二进制、换地图文件即换玩法（ADR-0013 的直接兑现）。
4. **数据地图可维护**：maps/*.ron 可 diff、可 review、CI 全量校验通过。

## 八、风险与对策

1. **通用化陷阱**（最大风险）：3 人团队把基底做成万能引擎，违反 ADR-0002。对策：范围锁死「魔兽式战斗 runtime」，渲染/物理/寻路/资产/脚本一律不进；每进一个新机制过三假想消费者预演。
2. **抽取时机**：warden 还在动（ART9/10 待做）。对策：只抽已 accepted 且回归全绿的稳定件；抽取卡与 warden 功能卡不同时进行。
3. **双层测试成本**：对策：基底机制测试一次写、所有地图复用；游戏行为测试只钉「该地图的规则」，不重复钉基底机制。
4. **早期 API 晃动**：对策：集中式 bundle.rs 隔离 + breaking 与迁移同卡。
5. **进度税**：Phase 0（基底骨架 + 三件抽取）在第一个可玩闭环之前。对策：Phase 0 卡片化、每张卡有独立验收，不做长暗改。

## 九、待拍板（与 ADR-0012/0013 评审合并）

基底 crate 命名（warbase / 其他）；根 workspace 迁移的时点（推荐：BX0 当卡即做，旧项目 exclude 清单一次列全）；warden 回迁插序（推荐：WX 系列在 ART9/10 之前完成，美术卡随后在新底座上做）；脚本化触发器的重开条件（推荐：出现「非程序员做图」或「地图热更新」需求）；命令卡 UI 上收粒度（整框架 vs 拆框架/定制两张卡，工作量超一张卡即拆）。已定项：根 workspace ✅、warden 回迁 ✅、bundle RON 文件化 ✅（ADR-0013）。
