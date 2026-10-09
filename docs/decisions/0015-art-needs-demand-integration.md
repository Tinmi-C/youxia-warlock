---
title: ADR-0015
type: adr
status: accepted
date: 2026-10-09
author: AI 起草 + youxia 委托（2026-10-09 接手岩库需求对接开发时落地；同日会签岩库契约 v1）
---

# ADR-0015: 游戏侧美术需求接入——art-needs 工具与三文件契约

## 背景

[[decisions/0009-art-vault-to-game-asset-flow|ADR-0009]] 建立的岩库资产流（清单锁定 + art-sync 同步）起点是「资产已存在」；上游的「游戏要什么美术」无登记、无状态、无去重。loop-td 立项需要 16 个塔 def + 5 种怪 + 地形的一批美术，缺口实感化。岩库侧已落地需求板（其 ADR-006 + 联调契约 v1 `docs/design/demand-board-contract.md`，2026-10-09）。本决策定义游戏侧（本 monorepo）如何接入：需求从哪来、以什么文件形态进仓库、与岩库怎么双向联动，并对契约 v1 **会签**（状态零校验 PATCH、locked 是 DemandLock 记录不是状态机节点、upsert 幂等、锁定上报端点）。

设计全稿：[[topics/game-design/artvault-demand-board-proposal|岩库需求板块设计稿 v0.1]]（P1 分期与状态机契约见该稿）。

## 决策

1. **需求源头 = MapBundle 单位定义（内容真源唯一）**：`games/<g>/maps/*.ron` 的每个 unit def 是潜在美术需求源。新工具 `tools/art-needs`（与 art-sync 同级）扫描 def 表，产出需求草稿。**地图改了、需求清单跟着变——def 是真源，需求是投影**。
2. **RON 解析不猜文本**：扫描走 warbase 的 `mapbundle-dump` bin（`cargo run -p warbase --bin mapbundle-dump -- <map.ron>`，serde 反序列化 + bundle 校验后输出 JSON）。Python 侧零依赖消费 JSON，不做正则/手写 RON 解析——格式演化时 serde 报错即坏图警报，投影永不撒谎。游戏运行时零改动（新增 bin 不触碰游戏插件）。
3. **三文件契约**（全部随 git，规格可评审、可 diff、离线可读）：
   - `games/<g>/docs/art-needs.json`——需求清单（本地状态源）：每条含 `local_key`（本地稳定键）、`name/category/spec/source_defs/acceptance/priority`（规格，权威在本仓库）、`demand_id/status`（岩库投影，submit 时回填）。`schema_version` 版本化（沿用 lockfile 先例）。
   - `games/<g>/art-mapping.json`——换装映射表（`def_id → 岩库:<asset_id>[@vN]`）：换装系统（ART-L2）的数据面，与需求板共用 id 空间。demand fulfilled 的落点 = 映射表多一条可用项。
   - `games/<g>/art-assets.json`——锁清单条目新增可选字段 `demand`（demand_id）：形成「需求 → 资产 → 锁定版本」三层可追溯（ADR-0009 不变，art-sync 新增对该字段的只读校验）。
4. **命令面（CLI 先行，UI 后置）**：`scan`（dump→diff→更新草稿，新增 def 生成新条目、消失 def 标记 `removed`）、`submit`（draft 条目 POST 岩库 `/demands` 后 PATCH 状态至 submitted——契约 v1 零校验直改，回填 demand_id；幂等）、`status`（拉岩库状态投影回写本地 + stats 汇总；报告 fulfilled-无资产等失真状态——零校验下质量责任在游戏侧投影）、`sync`（读锁清单 `demand` 回链字段，POST `/demands/{id}/locks` 上报锁定——幂等 upsert，版本升级后重跑即刷新）。退出码 = 问题数（与 art-sync/art-catalog 同约定）。
5. **解耦红线（管理流与构建流分离）**：岩库不可达时 `scan` 照常（纯本地）、`submit/status` 明确报告「岩库不可达（服务起了吗？）」并以非零退出；**任何游戏构建/测试不依赖岩库在线**。游戏仓库不含岩库运行时依赖（Python stdlib only，同 art-sync）。
6. **需求分组跟随升级链**：`upgrade_to` 链（arrow→arrow2→arrow3）默认合并为一条需求（一套底模三阶变体，`source_defs` 列全链），每阶差异写进 spec.variants。怪/功能塔每 def 一条。人可再手工细分——工具只给默认值。

## 备选方案

1. **Python 手写 RON 子集解析器**：无需 Rust 编译，但 RON 语法子集（Some/枚举变体/嵌套元组）解析器约 300 行且格式演化即烂。违反「规范化输入」原则。否。
2. **art-needs 放岩库仓库发 bin**：跨仓库耦合游戏目录结构，岩库侧 ADR-002 决策 5 已否过同构方案（推送式集成方向反了）。否——CLI 留本仓库 `tools/art-needs`，与 art-sync 对称。
3. **需求规格直接存岩库（无本地文件）**：失去 git 评审/diff/离线阅读，且规格权威漂移到岩库。否——规格进游戏仓库，状态在岩库（单一事实源各管一半）。

## 影响 / 后果

- warbase 新增 `bundle::dump_units_json()`（库函数，可测）+ `mapbundle-dump` bin（薄壳）；不触碰游戏 crate 代码。
- loop-td 产出第一版 `art-needs.json`（16 塔 def → 按链合并 + 5 怪 + 地形）与 `art-mapping.json` 骨架（全灰盒 = null，即当前现状的显式化）。
- warden 可后续跟进（已有资产回填 demand 链接属 P2 范畴，不阻塞）。
- 锁定语义按契约 v1 修订：locked **不是**状态机节点，而是 DemandLock 记录（锁清单为唯一事实源，岩库被动接收上报；与生命周期正交——changed 开 v2 迭代时旧锁定保留）。提案稿的「draft→locked 全程」出口 ≡「draft→fulfilled + 锁定记录存在」。
- 岩库侧契约以 `demand-board-contract.md` v1 为唯一权威（本 ADR 会签）；约定状态流（`DEMAND_STATUS_FLOW`）仅由 CLI 与 P2 看板引导，岩库不拦截。
