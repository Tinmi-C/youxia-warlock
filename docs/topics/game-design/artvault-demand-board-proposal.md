---
title: 岩库需求板块设计稿 v0.1——游戏美术需求的声明、跟踪与双侧联调（提案）
type: reference
topic: game-design
date: 2026-10-08
author: AI 起草（ZCode），待团队 + 岩库侧评审
status: accepted（P1 已落地）
tags: [artvault, art-pipeline, requirements, proposal, integration]
related: ["[[decisions/0009-art-vault-to-game-asset-flow|ADR-0009 岩库资产流]]", "[[topics/game-design/ai-asset-pipeline|AI 生成美术管线八站流程]]", "[[topics/game-design/art-asset-catalog-tool-proposal|art-catalog 设计稿]]", "[[topics/game-design/loop-td-game-proposal|loop-td 需求草案]]"]
---

# 岩库需求板块设计稿 v0.1——游戏美术需求的声明、跟踪与双侧联调

> **文档性质**：给岩库（ArtVault）项目的功能需求提案 + 双侧架构接入设计。发起动机：loop-td 立项需要 13 塔 + 5 怪 + 地形等一批美术，发现 ADR-0009 管线的起点是「资产已存在」，**上游的「游戏要什么」环节无登记、无状态、无跟踪、无去重**。youxia 2026-10-08 提出：岩库要有一个面向游戏项目的美术需求板块。本稿做完整分析；**具体资产生产不急**，本稿先行。

## 0. 结论（一句话）

在岩库把 **Demand（美术需求）升为一等公民**，打通「游戏内容定义 → 需求提报 → 生产跟踪 → 资产入库 → 清单锁定同步 → 变更回流」全闭环；游戏侧以 **MapBundle 的单位定义为需求源头**（内容真源唯一），岩库侧以需求板管理生产；双方以稳定的 `demand_id / asset_id / game_key` 契约联调，CLI 先行、UI 后置；**管理流与构建流解耦**是架构红线——岩库宕机不影响任何游戏构建。

## 1. 现状盘点（两侧架构对齐）

### 游戏侧（本 monorepo）

- 内容真源：`games/<g>/maps/*.ron` 的 MapBundle——单位 def、地图、波次全部数据化（ADR-0013）。**每个单位 def 就是一条潜在的美术需求源**。
- 换装：warden 已验证 def→GLB 换装（ART11/12）+ 场景重建自愈接线；loop-td 的 visuals 换装系统在计划内（ART-L2）。
- 验收通道：showroom-viewer（ADR-0010，独立 bin 可看任意 GLB）+ SM4 截图基线 + 视觉双通道裁决——**验收证据的生产线已存在**。
- 生产方法论：ai-asset-pipeline 八站流程（规格归 AI、审美归人）+ 混元/Rodin 生成 + Blender 洗白（tools/art：normalize/turntable/anim_strip/mixamo_merge）。
- 锁定同步：`games/<g>/art-assets.json`（资产 id+版本+文件映射）+ `tools/art-sync/art_sync.py`（ADR-0009）。

### 岩库侧（ArtVault，外部项目）

- 本地服务（当前 `http://127.0.0.1:8765`，HTTP API）。
- 已有概念：资产（id + version + 文件清单 + license）、游戏侧 save_asset 回流（版本升级 = 改 version 重跑同步）。
- **没有**：需求实体、状态跟踪、跨游戏复用视图、需求↔资产关联。

### 缺口（痛点清单）

1. 需求靠口头/聊天记录：loop-td 要什么、多急、验收标准是什么——无处登记。
2. 无状态跟踪：「生成中/待审/已入库」彼此靠问。
3. 无去重视图：warden 的 minion 能不能直接给 loop-td 的 grunt 用——判断靠记忆。
4. 变更无传导：游戏改了 def（塔三阶化），旧资产是否还满足——无人知道。
5. 双方联调无契约：岩库不知道游戏期望什么接口，游戏不知道岩库会提供什么。

## 2. 核心设计：Demand 实体与状态机（岩库侧需求）

### 2.1 Demand 字段

| 字段 | 说明 |
|------|------|
| `demand_id` | 稳定 id（永不复用；沿用岩库 id 风格） |
| `game_key` | 发起游戏（= 仓库目录名：warden / loop-td / …，**双方约定契约**） |
| `name` / `category` | 名称；类别：model / texture / animation / vfx / ui / audio |
| `spec` | 需求规格：参照图、生成 prompt、面数预算、贴图分辨率、动画 clip 清单（idle/walk/attack/hit/death 命名约定）、尺寸规范——字段对齐 ai-asset-pipeline 的规格段 |
| `source_defs` | 来源单位 def id 列表（如 `["arrow2", "arrow3"]`）——回链游戏内容真源 |
| `acceptance` | 数字化验收句（例：「三角面 ≤ 12k、2048 贴图、5 动画齐全、showroom turntable 过审」） |
| `priority` / `assignee` / `method` | 优先级；指派；生产方式（混元/Rodin/采购/Blender 程序化） |
| `asset_ids` | 满足本需求的资产（0..n，**多对多**：一个资产可同时满足多游戏需求=复用；一个需求可被多版本资产先后满足） |
| `status` / `history` | 状态与变更记录 |

### 2.2 状态机（联调契约的核心，双方共同变更、版本化）

```
draft（游戏侧草稿）
  → submitted（已提报）
  → accepted（岩库排期）          ↘ rejected/hold（附理由，可复活）
  → producing（生产中；stage 细化 = 八站流程站点）
  → review（待审；挂 showroom/turntable 证据链接）
  → fulfilled（资产入库，asset v1 生成，回填 asset_ids）
  → locked（被某游戏 art-assets.json 锁定，art-sync 可追踪）
  → changed（游戏侧需求变更 → 开 v2 迭代环，通知所有锁定方）
```

双方职责切分：游戏侧触发 draft→submitted、review 的通过/打回、changed；岩库侧触发 accepted/producing、fulfilled。每步流转留痕（谁、何时、附注）。

## 3. 游戏侧（本仓库）架构接入

> 原则：**规格进游戏仓库**（git 评审、可 diff、离线可读），**状态在岩库**（单一事实源），CLI 双向投影。

1. **需求源头自动化**：新工具 `tools/art-needs`（与 art-sync 同级）——扫描 `games/<g>/maps/*.ron` 的全部 unit defs，比对本地 `art-mapping`（见下）与岩库 fulfillment，产出需求草稿 `games/<g>/docs/art-needs.json`（含 spec 模板预填：def 名/类型猜测/动画清单）。**地图改了、需求清单跟着变**——def 是真源，需求是投影。
2. **换装映射表**：`games/<g>/art-mapping.json`（`def_id → asset_ref(+版本意图)`）。这是换装系统（ART-L2）的数据面，与需求板共用同一 id 空间——**需求 fulfilled 的落点就是映射表多一条可用项**。
3. **art-assets.json 向上回链**：锁清单条目新增可选字段 `demand`（demand_id），形成「需求 → 资产 → 锁定版本」三层可追溯。art-sync 的 drift 检测保持不变，新增「未满足需求」报告位。
4. **验收证据挂钩**：showroom/SM4 基线截图路径写进 demand 的 review 记录（评审材料自动就位，H3 关卡的输入）。
5. **占位策略（内容不被美术阻塞）**：demand 未 fulfilled 前，游戏侧用程序化占位（灰盒或 Blender 参数化生成）——loop-td 现状即此策略，固化为默认工作方式。
6. **离线降级**：岩库不可达时——构建零影响（现状已满足）；`art-needs sync` 只能用本地投影，报告「状态可能过期」。**管理流故障不传染构建流**。

## 4. 岩库侧功能需求清单（提给岩库项目）

1. Demand 实体 + 状态机 + 变更留痕（§2）。
2. API：`/demands` CRUD 与状态流转（含权限字段但 3 人团队可后置）；按 `game_key` 过滤；发布资产时支持回填 demand；提供「游戏需求覆盖度」查询（各状态计数）。
3. 需求↔资产多对多关联 + **跨游戏复用提示**：新需求提报时按名称/参照/规格相似度提示「warden 已有 minion，或可直接复用/微调」——去重是需求板的高频价值点。
4. 看板视图（P2）：按游戏/状态/优先级；展示 producing 的八站流程 stage。
5. 变更通知（P3）：demand changed / asset 新版本 → 提示所有锁定方（可与 art-sync drift 报告合流）。
6. 不做（明确出界）：游戏内嵌需求管理 UI、生成自动触发（生成仍在 4060Ti 机器人工触发）、复杂审批流。

## 5. 联调契约（双方动不得的部分）

| 契约 | 内容 |
|------|------|
| id 稳定 | `demand_id`、`asset_id` 永不复用；`game_key` = 游戏仓库目录名 |
| 状态机 | 字段与流转双方共同评审、`schema_version` 版本化（沿用 lockfile 先例） |
| CLI 优先 | 联调面 = `art-needs`（游戏→岩库：提报/状态投影）+ `art-sync`（资产拉取）两个命令；Web UI 后置不阻塞联调 |
| 解耦红线 | 岩库离线不影响游戏构建；游戏仓库不含岩库运行时依赖 |
| 规格归属 | 需求规格与验收句以游戏仓库为准（git 评审）；岩库持有状态镜像 |

## 6. 阶段划分

| 阶段 | 内容 | 出口 |
|------|------|------|
| P1 最小闭环 | Demand 实体 + 状态机 + 游戏 `art-needs` 扫描/提报/投影 CLI；art-assets.json 回链字段 | loop-td 全部美术需求在板上走一遍 draft→locked（纯 JSON/CLI，无 UI） |
| P2 管理体验 | 岩库看板 UI、复用/去重提示、showroom 证据挂接 | 三人日常用板替代口头跟催 |
| P3 变更回流 | changed 状态、跨游戏通知、drift 报告合流 | warden 资产 v2 迭代全程在板上可追溯 |

## 7. 与既有决策/工具的关系

- **ADR-0009 不变**，本设计是其上游延伸（从「锁已有资产」向前补「要什么→造出来」）。
- **art-catalog**（资产目录工具）管资产域，需求板管需求域，以 `asset_id` 衔接；不重叠。
- **ai-asset-pipeline 八站流程** = producing 状态的 stage 细化，方法论不动。
- 落地时序：岩库侧评审通过本稿 → 岩库立项 P1；游戏侧 `art-needs` 工具 + art-mapping 表 + art-assets.json 回链字段，动工时在本仓库立 ADR-0015（游戏侧接入契约）。

## 8. 待拍板（已决议，2026-10-09 P1 落地时按建议执行）

1. 「新制」与「复用已有资产」不区分——都走 fulfilled，复用时 asset_ids 指向既有资产（岩库 ADR-006 决策 3）。
2. review 通过权 = 纯游戏侧；P1 无鉴权，岩库只记录流转不拦截操作者（岩库 ADR-006 决策 5）。
3. `game_key` 为自由字符串、约定 = warlock 仓库 games/ 目录名，预留 showroom 等命名空间（岩库 ADR-006 决策 6）。
4. CLI 放本仓库 `tools/art-needs`（ADR-0015 备选 2 记录了否掉岩库发 bin 的理由）。

> **落地记录（2026-10-09）**：P1 已全链路落地——岩库侧 ADR-006（Demand/DemandEvent/DemandAsset 三表
> + 状态机 + /demands 十端点 + 41 测试全过）；游戏侧 ADR-0015（warbase mapbundle-dump bin +
> tools/art-needs CLI + 三文件契约）；loop-td 16 条需求（15 扫描 + 1 手工地形）已提报岩库，
> tower-arrow 已试点流转 accepted，状态投影正常。
