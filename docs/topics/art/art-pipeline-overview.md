---
title: 美术工程管线总纲（Art Pipeline Overview）
type: reference
topic: art
date: 2026-10-11
author: AI 起草 + youxia 定向（美术先保证规范/流程/专业化，内容卡后置）
status: draft
tags: [art-pipeline, overview, cross-game]
related: ["decisions/0009-art-vault-to-game-asset-flow", "decisions/0015-art-needs-demand-integration", "topics/graphics/model-part-animation-prep-spec", "topics/art/visual-acceptance-methods", "topics/art/surgery-workflow", "topics/art/ui-asset-spec"]
---

# 美术工程管线总纲

## 结论

本仓库美术资产的全生命周期走**一条门、八个环节**：任何美术文件不手拷进 `assets/`，
产出必须经取证验收，入库唯一通道是岩库（ArtVault），消费唯一通道是清单锁 + art-sync。
本文是地图：每个环节指向它的决策文档（ADR）、规范（spec）、工具与退出准则——
新会话/新队友从这里进入，不再从碎片拼图。

## 正文

### 全景

```
需求 ──► 产出 ──► 验收 ──► 入库 ──► 同步 ──► 消费 ──► 回流/升级
(art-needs)  (三条线)   (取证)   (岩库)   (art-sync) (网关表)  (save_asset→vN)
```

### 环节合同

| # | 环节 | 决策/规范 | 工具 | 产物 | 退出准则 |
|---|------|-----------|------|------|----------|
| 1 | 需求 | [[decisions/0015-art-needs-demand-integration\|ADR-0015]]（def 为真源，需求是投影） | `tools/art-needs`（scan/submit/status/sync） | `games/<g>/docs/art-needs.json` | 条目有 local_key/spec/acceptance/priority，已 submit 回填 demand_id |
| 2 | 产出 | 三条线（见下） | 外采甄别 / Blender 手术（[[topics/art/surgery-workflow\|手术流程]]） / 手 K 动画 | 源文件 + 导出 GLB/PNG | 文件过 `tools/art/normalize.py`（bbox/单位/朝向归一） |
| 3 | 验收 | [[topics/art/visual-acceptance-methods\|视觉验收取证规范]] | 结构断言 / 转台录屏 / 双帧姿势对比 / 像素差 | 断言测试 + 取证截图/录屏 | 断言进 `tests/art_assets.rs` 绿；观感项过人工中性提问核对 |
| 4 | 入库 | 岩库流程（其 ADR-006 等） | 岩库 `save_asset` | 版本快照 vN + 技术元数据 + license 留痕 | 岩库侧状态 usable，版本号已定 |
| 5 | 同步 | [[decisions/0009-art-vault-to-game-asset-flow\|ADR-0009]] | `tools/art-sync/art_sync.py` | `games/<g>/art-assets.json` 锁条目 | 字节数校验过；锁文件随 git 提交 |
| 6 | 消费 | 各游戏美术方向（如 `games/warden/docs/art-direction.md`）+ 换装映射 | 游戏侧网关表 / `art-mapping.json` | `assets/` 下的锁定版本 | 结构断言（命名/尺寸/分件）常驻绿 |
| 7 | 回流 | 同 4 | `save_asset` 存新版本 | vN+1（附 note） | 岩库版本链完整 |
| 8 | 升级 | 同 5 | art-sync `--outdated` 看计划 → 改锁版本号重跑 | 新锁条目 + 资产文件一并提交 | 游戏侧断言全绿（结构变了=断言先红，修断言即修合同） |

### 产出三条线

| 线 | 适用 | 产出 | 关键规范 |
|----|------|------|----------|
| 外采 | 贴图（PolyHaven 等 CC0）、成品模型（KayKit 等） | 原样入库 | license 甄别与留痕（锁文件 license 字段）；无溯源不入 |
| 图生 3D | Rodin 等生成的体块/道具 | 常需一次「分件手术」（[[topics/graphics/model-part-animation-prep-spec\|分件规范]]） | [[topics/art/surgery-workflow\|手术流程]] |
| 手 K 动画 | 角色/BOSS 表现型 | 绑骨 + 片段 + 引擎播放（ART11/ART12 先例） | 不走分件规范；动画命名与播放器接线见各游戏卡 |

### 文档地图（本主题域）

- [[topics/art/visual-acceptance-methods|视觉验收取证规范]]——四种取证手段的适用矩阵 + ART11 教训
- [[topics/art/surgery-workflow|分件手术流程]]——AI 主刀纪律，ART9 的直接前置
- [[topics/art/ui-asset-spec|UI 2D 资产规范]]——图标/九宫格面板线
- 上游：[[topics/graphics/model-part-animation-prep-spec|分件规范]]（graphics 域，机械件合同）

### 挂账（已知未闭环）

- art-catalog 的 intake 工单 + SC1 洗白流水线（等 `_library/` 落地，外部依赖）；
- Mimosa 完整审计未跑完前，不宣称项目安全（全仓事项，非美术域）。

## 参考

- ADR-0009 / ADR-0015（两条已定决策，本文不重复其细节）
- 各游戏 art-direction.md 与 design.md（消费侧口味与 tokens）
