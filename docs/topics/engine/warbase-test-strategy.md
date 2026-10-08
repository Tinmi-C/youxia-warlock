---
title: warbase 测试策略 v0.1——测试用例全集 + AI 测试方案
type: howto
topic: engine
date: 2026-10-08
author: AI 起草（ZCode），待团队 review
status: draft
tags: [testing, warbase, ai-collaboration, bal, visual-acceptance]
related: ["[[topics/engine/warbase-implementation-design|warbase 技术实现设计]]", "[[topics/engine/warbase-architecture|warbase 架构]]", "[[topics/game-design/loop-td-game-proposal|循环圈 TD 需求草案 v0.2]]", "[[topics/game-design/design-target-anchors|设计目标锚点框架]]", "[[topics/engine/bevy-headless-visual-acceptance-loop|视觉验收闭环]]", "[[topics/engine/ai-feature-pipeline-sop|AI 特性开发 SOP]]", "[[topics/engine/automated-game-testing-and-ai|游戏自动化测试与 AI 调研]]"]
---

# warbase 测试策略 v0.1——测试用例全集 + AI 测试方案

> 配套 [[topics/engine/warbase-implementation-design|技术实现设计]]。测试在本项目里不是附属品，是**能力卡的验收句本身**（方法论：AI 可接管程度 = 验收句可数字化程度）。本文给三样东西：六层测试金字塔、按模块的用例全集（实现卡的 DoD 底稿）、AI 执行测试的操作方案（谁跑、何时跑、怎么报告、失败怎么办）。

## 1. 测试分层总览

| 层 | 对象 | 形态 | 运行 | 归属 |
|----|------|------|------|------|
| L0 纯函数 | 攻甲表/路径数学/经济曲线/波次公式 | `#[cfg(test)]` 模块内单测，无 App | 毫秒级 | warbase |
| L1 机制 | 各 BX 模块行为 | headless App（`build_app()` 共享构造，warden 模式） | 秒级 | warbase tests/ |
| L2 游戏行为 | loop-td 玩法规则 | headless App 全链路断言 | 秒级 | games/loop-td tests/behavior.rs |
| L3 平衡锚点 | 数值体验 | harness 自动打局读数（A1–A3） | 分钟级 | games/loop-td tests/balance.rs（引擎在 warbase/harness） |
| L4 视觉验收 | UI/画面 | 帧脚本场景 + 引擎内截图 + 基线 diff | 分钟级 | loop-td（SM4/DB2 通道复用） |
| L5 冒烟 | 基底多玩法主张 | 迷宫 bundle 小局全绿 | 秒级 | warbase tests 或 loop-td 场景 |

原则：**L0/L1 钉机制、只写一次全图复用；L2 只钉「这张地图的规则」不重复钉机制；L3/L4 是验收 gate 不是回归套件**（按阶段出口跑）。

## 2. 确定性基建（所有层的前置，实现卡最先做）

1. 种子化 `MatchRng` 资源：游戏/harness 全部随机走它，禁止直接 `thread_rng()`；
2. 固定 dt 帧推进：测试与 harness 用手动 `app.update()` + 固定虚拟时间（DB3 思路），断言不含墙钟；
3. Messages 逐帧 drain 累加（双缓冲坑），封装测试助手；
4. 存档/偏好文件：每测试唯一临时路径 + 用例开头删除（warden meta 污染坑）；
5. headless 构造共享：warbase 提供 `test_app(bundle)` 构造器（模板 lib/bin 分离 + 集成测试共享 build_app 模式的基底版）。

## 3. 测试用例全集（按模块；格式：ID｜用例｜断言要点｜门禁卡）

### 3.1 unit / 攻甲（BX1）

| ID | 用例 | 断言要点 | 卡 |
|----|------|----------|----|
| UT-AA-01 (L0) | 攻×甲系数查表 | 全组合（5×6）返回 bundle 矩阵值 | BX1 |
| UT-AA-02 (L0) | 未知组合容错 | 缺省 1.0 或 panic——定一种并钉死 | BX1 |
| UT-UNIT-01 (L1) | def 实例化 | spawn 后 Hp/UnitInstance/组件组合与 def 一致 | BX1 |
| UT-UNIT-02 (L1) | traits 生效 | Fast 经 EffectiveStats 提升移速 | BX1 |

### 3.2 combat（BX2）

| ID | 用例 | 断言要点 | 卡 |
|----|------|----------|----|
| UT-CMB-01 (L1) | 基础伤害 | DamageRequest → Hp 扣减=amount | BX2 |
| UT-CMB-02 (L1) | 攻甲克制 | Magic×Heavy 实际伤害=amount×表值 | BX2 |
| UT-CMB-03 (L1) | 击杀链路 | Hp≤0 → 死亡 despawn + 赏金/经验事件 + DamageDealt{killed} | BX2 |
| UT-CMB-04 (L1) | 归因 | damage_by_source 按来源累计正确（塔普攻/技能/指挥官分列） | BX2/BX10 |
| UT-CMB-05 (L1) | 索敌排序 | First/Last 按**路径进度**；Strongest/LowestHp 按 Hp | BX2 |
| UT-CMB-06 (L1) | 投射物 | 追踪命中；目标中途死亡按边缘行为表（穿越/消散） | BX2 |
| UT-CMB-07 (L1) | 对空对地 | Ground-only 攻击不锁定 Air 单位 | BX2 |
| UT-CMB-08 (review) | Hp 单写入者 | 游戏与基底中 `Hp(` 直写仅存在于 resolve_damage（grep 检查句，钉进 review 清单） | BX2 |

### 3.3 skill / buff（BX3）

| ID | 用例 | 断言要点 | 卡 |
|----|------|----------|----|
| UT-SKL-01 (L1) | 冷却 | Tick 推进冷却；就绪后 Active 可施放 | BX3 |
| UT-SKL-02 (L1) | 三分法 | Passive 常驻不施法；Autocast 条件触发；Active 需指令 | BX3 |
| UT-SKL-03 (L1) | StatMods | 增/乘叠加、限时到期移除、按 source 删不误伤 | BX3 |
| UT-SKL-04 (L1) | 技能升级 | 加点后等级/数值表切换、上限封顶 | BX3/HR2 |
| UT-BUF-01 (L1) | 光环进出 | 入场加条目/离场删 → EffectiveStats 随之变化 | P2 buff |
| UT-BUF-02 (L1) | 光环边界 | 距离=radius 的归属定死（含/不含） | P2 buff |
| UT-BUF-03 (L1) | 多光环 | 不同 source 条目并存互不干扰 | P2 buff |

### 3.4 path（BX5）

| ID | 用例 | 断言要点 | 卡 |
|----|------|----------|----|
| UT-PATH-01 (L0) | 折线数学 | length/sample 纯函数对拍手算值 | BX5 |
| UT-PATH-02 (L0) | 闭环采样 | closed 首尾衔接处采样连续无跳变 | BX5 |
| UT-PATH-03 (L1) | 匀速推进 | N 帧位移=speed×dt×N（减速后按生效速度） | BX5 |
| UT-PATH-04 (L1) | 圈数事件 | 走满一圈 laps=1 且发 LapCompleted 一次 | BX5 |
| UT-PATH-05 (L1) | 开链终点 | open 路径走完发 Arrived（供 Lives 规则消费） | BX5/SMK |

### 3.5 wave（BX6）

| ID | 用例 | 断言要点 | 卡 |
|----|------|----------|----|
| UT-WAV-01 (L1) | 自动推进 | InterWave 计时到自动开下一波 | BX6 |
| UT-WAV-02 (L1) | 提前召唤 | 立即开波 + 奖励入账 | BX6 |
| UT-WAV-03 (L1) | 预告时机 | InterWave 开始即发 WavePreview 且内容=下波构成 | BX6/UI8 验收 |
| UT-WAV-04 (L1) | BOSS 限时 | 超时发 BossTimedOut（规则判负在 3.7 钉） | BX6 |
| UT-WAV-05 (L1) | 手动模式 | Advance::Manual 不自动推进（warden 语义位） | BX6 |

### 3.6 econ（BX7）

| ID | 用例 | 断言要点 | 卡 |
|----|------|----------|----|
| UT-ECO-01 (L0) | 木换金 | 按 rules 汇率，零头取整规则钉死 | BX7 |
| UT-ECO-02 (L0) | 人口曲线 | 买人口成本按曲线递增 | BX7 |
| UT-ECO-03 (L1) | 拒付 | 余额不足 EconRejected 且余额/存量不变 | BX7 |
| UT-ECO-04 (L1) | 赏金入账 | 击杀 → Grant → EconChanged 恰一次 | BX7 |

### 3.7 rule（BX8）

| ID | 用例 | 断言要点 | 卡 |
|----|------|----------|----|
| UT-RUL-01 (L1) | 存量判负 | 活怪数到 cap → Lost{Stockpile} | BX8 |
| UT-RUL-02 (L1) | 存量回落 | 击杀后存量即减（事件驱动，无全量 count 漂移） | BX8 |
| UT-RUL-03 (L1) | 扣命判负 | Arrived 扣命、命尽 Lost{NoLives}（迷宫依赖） | BX8/SMK |
| UT-RUL-04 (L1) | 绕圈增益 | LapCompleted → 永久 StatMods 生效且可叠加 N 次 | BX8 |
| UT-RUL-05 (L1) | BOSS 超时判负 | Lost{BossTimeout} 归因正确 | BX8 |
| UT-RUL-06 (L1) | 胜利 | SurviveWaves(n) 波清 → Won | BX8 |

### 3.8 build / bundle / ui / harness（BX4/BX9/BX10）

| ID | 用例 | 断言要点 | 卡 |
|----|------|----------|----|
| UT-BLD-01 (L1) | 放置拒绝 | 越界/压路/压塔/压 blocked/无空闲位（FixedSlots）五类各自拒绝 | build |
| UT-BLD-02 (L1) | 占用与回收 | 成功放置 occupancy 更新；回收释放 + 返还比例（rules 配置） | build |
| UT-BLD-03 (L1) | 幽灵同源 | 幽灵校验与提交走同一函数（同输入同判定） | build |
| UT-BLD-04 (L1) | 双模式等价 | FreeGrid 与 FixedSlots 各跑通一轮建塔（warden 固定位即回归位） | build/WX6 |
| UT-BUN-01 (L1) | 装配完整 | assemble 后资源/注册表/静态实体/挂钩绑定齐 | BX9 |
| UT-BUN-02 (L1) | 重开局 | despawn+重置无 panic、无资源残留（World 重开坑回归） | BX9 |
| UT-BUN-03 (L1) | 版本拒绝 | identity 版本不匹配拒载且报错可读（不静默迁移） | BX9a |
| UT-BUN-04 (L0) | 坏文件拒载 | RON 语法错/必填缺失/枚举越界 → 可读 MapError | BX9a |
| UT-BUN-05 (L0) | 悬空引用 | 技能 id/资产路径/挂钩 id 不存在 → 拒载并指出行内字段 | BX9a |
| UT-BUN-06 (L1) | 换图即换玩法 | 同一 test_app 加载 loop.ron 与 maze-smoke.ron 行为差异正确（漏怪策略/波次表来自文件） | BX9/SMK |
| UT-BLD/RUL-05 (L1) | 建造窗口 | PrepOnly 波中建造被拒、波间可建；Always 不受限 | BX8/WX6 |
| UT-UI-01 (L1) | def 驱动 | 命令卡槽位/热键/Tooltip 全部来自 CommandCardDef（bundle 文件） | BX4 |
| UT-UI-02 (L1) | 刷新护栏 | 同状态重复刷新不重写文本（cache-key） | BX4 |
| UT-FX-01 (L1) | 通用 FX | 血条/闪白/死亡消散随 DamageDealt 触发（feature "fx" 开关均编译） | WX4 |
| UT-HAR-01 (L1) | 可复现 | 同种子两次 run 报告全等 | BX10 |
| UT-HAR-02 (L1) | 注入 | inject 存量/金币生效（A2 依赖） | BX10 |
| UT-HAR-03 (L1) | 双 bundle 读数 | harness 跑 warden.ron 与 loop.ron 各出报告（读数口径一致） | WX5/BX10 |

### 3.9 游戏行为 L2（loop-td，tests/behavior.rs）

| ID | 用例 | 断言要点 | 卡 |
|----|------|----------|----|
| G-LP-01 | 建塔闭环 | 建塔扣金+占地+进入索敌开火 | TO1/TO2 |
| G-LP-02 | 漏怪留场 | 漏怪不消失：存量+1、继续绕圈 laps 递增 | EN2 |
| G-LP-03 | 存量判负链 | 存量到 50 → 结算屏 + 归因=存量满 | LP/ST1 |
| G-LP-04 | 通关 | 30 波全清 → 胜利结算 + 计分 | WA1/ST1 |
| G-LP-05 | 特性波四件套 | 快速提速/重甲对炮塔减伤/治疗怪回血/光环怪加成 | EN3 |
| G-LP-06 | 金矿 | 打爆 +40 金（薄插件事件） | MA2 |
| G-LP-07 | 英雄塔三层 | 吃经验升级→加点→天赋三选一生效 | HR1–HR3 |
| G-LP-08 | Meta 持久化 | 解锁跨局生效（唯一临时文件+先删） | MT1 |
| G-LP-09 | 命令卡 | 双入口建造/卖塔/热键/五态切换 | UI1 |
| G-LP-10 | 预告与 l10n | 下波预告内容正确；中英切换全量刷新 | UI1/WA1 |
| G-LP-11 | 攻甲体验 | 炮塔(攻城)打重甲有显著克制差 | TO1/AR 表 |

### 3.9b warden-on-base 回归（W 系，WX 卡的验收句——基底验收 #0）

warden 既有 160 断言**语义不变**是抽取忠实性的证明，测试构造允许适配（资源改名/构造函数换基底版）。W 系是 WX 卡的收口清单：

| ID | 用例 | 断言要点 | 卡 |
|----|------|----------|----|
| W-REG-01 | 全量回归 | warden `cargo test` 160 断言全绿（每张 WX 卡收口时跑） | WX1–WX6 |
| W-BAL-01 | 读数不漂移 | BAL 两锚点（塔 92.9%/塔技能 5.2%/指挥官 1.9% 口径）迁移前后差 ≤ 容差 | WX5 |
| W-PAR-01 | 参数化等价 | warden.ron 中 Lives10/PrepOnly/Manual/单币/FixedSlots 装配出的行为与迁移前一致 | WX6 |
| W-ART-01 | 美术不回归 | 命令卡/FX 切基底后视觉基线重拍无异常页 | WX3/WX4 |

### 3.10 平衡 L3 / 视觉 L4 / 冒烟 L5

| ID | 用例 | 断言要点 |
|----|------|----------|
| BAL-A1 (L3) | 新手存活锚点 | 第 5 波零功能塔存量峰值 ≤ 10 |
| BAL-A2 (L3) | 翻盘锚点 | 15 波注入 20 存量，3 波内清回 ≤ 15 |
| BAL-A3 (L3) | BOSS 紧度锚点 | 标准 build 打 BOSS 用时 ∈ [0.6T, 0.9T] |
| BAL-S1 (L3) | 贡献分布 sanity | 无单一塔贡献 > 95%；伤害/经济曲线记录入报告 |
| VIS-01..05 (L4) | 命令卡五态/幽灵放置/存量 HUD/BOSS 倒计时/结算屏存量曲线 | SM4 式基线入库，改动重跑 + git diff |
| SMK-01 (L5) | 迷宫冒烟 | **`loop-td --map maps/maze-smoke.ron`** 跑通 open 路径 + Lives 规则 + 3 塔 + 5 波——同一二进制换地图文件即换玩法的现场实证 |

## 4. AI 测试方案（谁跑、何时跑、怎么报告、失败怎么办）

### 4.1 人机分工

| 环节 | AI | 人（关卡 H） |
|------|----|----|
| L0/L1/L2/L5 编写与执行、红绿循环、修复 | 全责 | 抽查 |
| L3 锚点跑数与调参建议 | 跑数+建议 | **H2 数值体感拍板**（锚点目标值本身是人定的体验目标） |
| L4 视觉基线首建 | 产出截图+自审 | **H3 审美拍板**（首基线定版；后续 diff AI 判） |
| 阶段出口试玩 | 组织+陪跑记录 | **H1 手感试玩**（P1/P2/P3 出口各一次） |
| 架构/范围决策 | 给备选+推荐 | 拍板（ADR/决策点） |

### 4.2 能力卡门禁流程（一卡一提交的测试面）

```
增量开工 → 验收句数字化（引用本文用例 ID，缺的先补用例；规格进设计文档/GDD——ADR-0014）
  → 红：先写测试（允许失败），确认测的是验收句
  → 绿：实现至全绿（基底机制测试 + 消费方演示同增量；WX 增量另跑 W 系）
  → 回归：cargo test --workspace（根 workspace 一击 = 全消费方；CI 之外的标准门禁）
  → 按需：BAL（数值增量）/ 视觉基线（UI 增量，重跑截图 + git diff）
  → 报告（§4.4 格式）→ 提交（tests: + 用例 ID 写进 commit body）
```

红绿纪律：**没有红色测试就不算实现完成**；修 bug 必须先写复现测试（红）再修（绿），测试留作回归——warden 既有惯例的成文化。

### 4.3 回归矩阵（何时跑什么）

| 触发 | 必跑 | 加跑 |
|------|------|------|
| 每张卡收口 | `cargo test --workspace`（一击全消费方） | WX 卡：W-REG-01；BX 卡：--no-default-features 也编译 |
| 基底（warbase）任何改动 | 同上（workspace 已含全部消费方） | UT-BUN-02（重开局坑）必含 |
| maps/*.ron 任何改动 | 全 bundle 装载校验（UT-BUN-03..05 口径，做成一条校验命令进卡收口） | 受影响地图的行为用例 |
| 数值/平衡卡 | — | BAL 全锚点 + 贡献分布（loop 与 warden 双 bundle） |
| UI 卡 | — | 视觉基线重跑 diff |
| 阶段出口（P0.5/P1/P1.5/P2/P3） | 全部 L0–L3 + L5；P1.5 特别含 W 系全绿（基底验收 #0 达成） | 全套 L4 + H1 试玩 |
| 发版（v1） | 全部 | 基线重拍定版 + 报告归档 docs/ |

### 4.4 AI 测试报告格式（每卡收口必附，进 commit body / docs/log.md）

```
[TEST] <card-id> <title>
- warbase: 42 passed / 0 failed（新增 6：UT-PATH-03..04, UT-RUL-01..02, ...）
- loop-td: 118 passed / 0 failed（新增 4：G-LP-02..03, ...）
- BAL: A1=8/10 ✓  A2=14/15 ✓  A3=0.78T ✓  S1:max 71% ✓
- VIS: 基线 diff 0 页异常（2 页重拍：命令卡 Build 态）
- 人工关卡：H1 试玩待约 / 不适用
```

### 4.5 失败处理纪律（全局元原则在测试域的落地）

1. 同一测试修 2 次不过 → 第 3 次前必须停下问「测的是不是错误的问题」（方案级复盘，禁止继续调参硬磨）；
2. 同模块补丁 3 层未清 → 重写该模块，测试套件（L1）作为重写的规格保留；
3. flaky 三连（同种子不同结果）→ 判定隐藏非确定性源（墙钟/thread_rng/消息跨帧），优先修基建不是改断言；
4. 新领域/新插件开工前先搜坑（`<技术> <场景> gotchas`），本仓 KB pitfall 类笔记先过一遍。

### 4.6 与既有 SOP 的关系

本文是 [[topics/engine/ai-feature-pipeline-sop|AI 特性开发 SOP]] 六阶段中「验证阶段」在 warbase/loop-td 上的具体化：用例表=验收句池（阶段 1 引用 ID 即可），§4.2=DoD 的测试面展开，L4 通道直接复用 [[topics/engine/bevy-headless-visual-acceptance-loop|视觉验收闭环]]（帧脚本场景 + 引擎内截图 + 视觉模型/引擎日志双通道裁决，注意其已记录的坑：小字徽章视觉模型不可靠、无人值守光标钉住）。L3 复用 [[topics/game-design/design-target-anchors|锚点方法论]] + warden BAL2 驱动策略（自动布阵/真施放/读数口径）。

## 5. 首批测试增量（与实现设计 §5 阶段对齐；编号沿用为增量名，ADR-0014）

| 卡 | 内容 |
|----|------|
| TS-WS | 随 WS0：根 workspace 接线后 `cargo test --workspace` 一击绿（warden 全量回归做基准快照） |
| TS0 | 确定性基建：MatchRng/test_app/逐帧 drain 助手/临时文件隔离（§2 全部） |
| TS1 | 随 BX1/BX2+WX1/BX3+WX2 落 UT-AA/UT-UNIT/UT-CMB/UT-SKL + W-REG-01 首跑 |
| TS2 | 随 BX9a 落 UT-BUN-03..05（装载校验器先于内容） |
| TS3 | 随 BX5/BX6/BX8 落 UT-PATH/UT-WAV/UT-RUL 全组（含建造窗口/双模式） |
| TS4 | 随 WX3..WX6 落 W 系全组（warden-on-base 验收 #0） |
| TS5 | 随 BX10 落 UT-HAR（含双 bundle 读数）+ BAL-A1/A2/A3 首跑 |
| TS6 | 视觉基线首建（VIS-01..05，H3 拍板） |
| TS7 | SMK-01 迷宫冒烟 + UT-BUN-06（--map 换图演示，P3） |
