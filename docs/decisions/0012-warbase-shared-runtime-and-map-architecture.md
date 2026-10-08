---
title: ADR-0012
type: adr
status: accepted
date: 2026-10-08
author: AI 起草 + youxia 拍板（2026-10-08 修订 v2 并接受：效果优先，warden 回迁 / 根 workspace / UI/FX feature 门控）
---

# ADR-0012: 魔兽式游戏基底 warbase——共享战斗 runtime crate +「游戏=地图」分层架构

## 背景

新游戏目标（见 [[topics/game-design/loop-td-game-proposal|循环圈 TD 需求草案]]）：以魔兽 RPG 地图生态为范本——**基底是主产品**，塔防是第一张新地图兼验收场，后续玩法（迷宫/宝石/军团/守卫 RPG）长在同一基座上。魔兽生态的结构性原因是三层分离：引擎宿主（单位/战斗/资源模型）＋ 对象数据层（塔/怪/技能=定义数据）＋ 触发器编排层，地图是自包含数据包，换地图不换宿主。

现状：warden 已验证一批「宿主级」能力（ADR-0011 技能框架、统一伤害管线、命令卡 UI、BAL headless 方法论，160 测试绿），但以 warden 内部代码形态存在；按沉淀约定，第二消费者已出现，抽取时机成立。

**v2 修订动因**（用户拍板「不图省事，该重构就重构」）：v1 草案为省工作量做了三处妥协——warden 不回迁、不设根 workspace、MapBundle 留代码内。三处都直接削弱「基底是产品」的效果：没有回迁，抽取的忠实性与基底的持续回归都缺最强证明；没有 workspace，「全消费方一键回归」与 Bevy 版本对齐靠人肉；bundle 不成数据文件，「地图=自包含数据包」只是口头主张。本版全部反转。

## 决策

1. **新建共享基底 crate `engine/warbase`（暂名）**，Bevy 0.19.1 之上、游戏之下；UI 部分以 cargo feature `ui` 门控（关 feature = 纯逻辑基底，供 harness/CI 快跑），基底成型后按需拆 crate。
2. **设根 cargo workspace**：members = `engine/*` + `games/warden` + `games/loop-td`（+ 后续新游戏），旧演示项目（bevy-spike / m1-demo / m2-demo / m2-bevy / wave-survival）与 `templates/bevy-game` 进 exclude 保持独立；`[workspace.dependencies]` 统一钉 bevy/rand 版本；`cargo test --workspace` 成为「全消费方回归」的标准一击。单 lockfile、共享构建缓存。
3. **基底范围 = 「魔兽式战斗 runtime」**：单位域模型（攻/甲类型、经验等级）、伤害管线、按实体技能框架、Buff/光环、路径（链/环/分支）、波次导演、金/木/人口资源、可插拔压力与胜负规则（含建造窗口策略）、建造域（自由网格/固定位双模式）、命令卡 UI 框架、通用战斗 FX（血条/闪白/死亡）、地图 bundle 装配器（数据文件加载，见 ADR-0013）、headless harness。**不含**：渲染、物理、寻路网格、资产管线、脚本系统（守住 ADR-0002 边界；脚本化以「非程序员做图/热更新」为重开条件）。
4. **「游戏 = 地图 bundle + 薄编排插件」**：bundle = 数据文件（RON，随仓库走，含版本与资产引用）+ 代码挂钩（场景构造/特有效果系统按 id 注册）。基底不感知任何具体游戏概念；依赖单向 games → warbase → bevy。
5. **warden 回迁为基底消费方 #0**：抽取不再「复制了事」，每件抽取与 warden 切换消费同卡成对（WX 卡系），验收 = warden 既有 160 断言全绿 + BAL 两锚点读数不漂移（断言语义不变，允许测试构造适配）。warden 的固定 8 塔位/双速锁建造/单币经济/L 路径全部表达为基底参数（BuildGrid 固定位模式、`Advance::Manual` + 建造窗口策略、wood=0、开链路径）——这本身就是基底通用性的第一批实证。回迁系列插在 warden ART9/10 之前完成；warden 后续功能（含美术卡）在新底座上做。
6. **三通道演化协议**：A 抽取+回迁（warden 件，成对卡）；B 基底先行（新通用机制直接按基底规格实现，第一消费方验收）；C 游戏侧孵化（无把握机制留缝，双消费方再上收）。新地图必须改基底核心才能实现 = 抽象有洞的信号，停下复盘补 ADR。
7. **护栏**：基底机制级测试自持；消费方行为回归钉基底行为；`cargo test --workspace` 绿是一卡一提交门槛；防腐化检验 `cargo build -p warbase --no-default-features` 零游戏依赖零资产。

## 备选方案

1. **warden 不回迁、抽取只复制**（v1 方案）：省一个卡系的工作量，但抽取忠实性只能靠基底测试自证、基底演进缺 warden 这块最重的回归压舱石，且 warden 永远停在旧底座上分叉。否——用户明确要求效果优先。
2. **多 crate 立即拆分（warbase-core/ui/fx…）**：边界正确但过早——UI 用 feature 门控、模块间只经 prelude 交互已能防腐；拆分留到第三消费方或编译痛感出现。
3. **脚本层（Lua/RON 规则 DSL）复刻触发器**：v1 否——效果上 Rust 薄插件 + 数据 bundle 已达成「换图不换宿主」；脚本化等「非程序员做图/热更新」成为真需求再开 ADR。
4. **维持各游戏独立 lockfile**：现状最省事，但 Bevy 版本对齐靠人肉、全量回归要多目录轮跑、构建缓存不共享。否。
5. **在 warden 内做第二模式**：否（v1 已论证：骨架级冲突）。

## 影响 / 后果

- 优：基底从第一天起就有双消费方（warden 回迁件 + loop-td 新机制件）；「同一二进制换地图文件即换玩法」可现场演示（loop-td 主程序加载迷宫冒烟 bundle）；一键 workspace 回归。
- 代价：P0 变重（workspace 接线 + 抽取回迁成对卡 + bundle 序列化层先行）；warden 已验收系统承受一次受控重构（对冲：断言语义不变 + 一卡一迁移步 + BAL 读数不漂移作验收）；早期基底 API 晃动由游戏侧集中装配层隔离。
- 关联决策：MapBundle 数据文件化见 [[decisions/0013-mapbundle-as-data-file|ADR-0013]]（收窄 ADR-0008 的适用范围，非取代其精神）。
- 实施按能力卡推进：BX（基底）/ WX（warden 回迁）/ 游戏卡三线，顺序见 [[topics/engine/warbase-implementation-design|实现设计]] §5。
