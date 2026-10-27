---
title: 美术工程（Art）
type: reference
topic: art
date: 2026-10-11
author: AI + youxia
status: draft
tags: [art-pipeline, cross-game]
---

# 美术工程

跨游戏的美术资产工程：需求、产出、验收、入库、同步、消费、回流的全生命周期规范。
地图入口：[[topics/art/art-pipeline-overview|管线总纲]]。

## 笔记

- [[topics/art/game-art-cross-project-workflow|游戏↔美术跨项目工作流]]（howto draft：五阶段两回环权威规格——def 需求唯一源→art-needs 投影→看板 draft/locked→美术制作→art-sync 版本锁同步→四手段取证+H-ART 看图→回流；两个拍板点其余全自动；含已实证/五项待补清单）
- [[topics/art/asset-taxonomy-spec|项目美术资产分类规格]]（reference draft：六大类——UI 2D（九宫格+图标 P0）/3D 模型（分件+体量对齐 body_radius，loop-td 16 需求已上板）/动画（时长对齐逻辑帧窗）/VFX（fx 消息即接口）/环境/音频（SfxKind 即需求表）；每类给需求源/规格/验收/接入缝/数量与批次）
- [[topics/art/art-pipeline-overview|管线总纲]]（reference：八环节合同 + 产出三线 + 文档地图）
- [[topics/art/visual-acceptance-methods|视觉验收取证规范]]（reference：四手段适用矩阵 + ART11 教训）
- [[topics/art/surgery-workflow|分件手术流程]]（howto：AI 主刀纪律，Blender 无头脚本规范）
- [[topics/art/art9-ballista-surgery-record|ART9 弩分件手术记录]]（howto：首例 v2→v3 全程，弦认定/零漂移证据/四坑）
- [[topics/art/generated-mesh-coincident-vertex-blob|图生网格重合顶点 blob]]（pitfall：位置焊接/空间选择全失效，边连通岛+bbox 种子）
- [[topics/art/blender-workbench-headless-color-pitfall|WORKBENCH 无头吃不到 shading]]（pitfall：彩色取证渲染改 Cycles+纯发光）
- [[topics/art/ui-asset-spec|UI 2D 资产规范]]（reference：图标/九宫格线）

## 关联

- [[decisions/0009-art-vault-to-game-asset-flow|ADR-0009 资产流]] · [[decisions/0015-art-needs-demand-integration|ADR-0015 需求流]]
- [[topics/graphics/model-part-animation-prep-spec|分件规范]]（graphics 域）
- 工具：`tools/art`、`tools/art-sync`、`tools/art-needs`、`tools/art-catalog`
