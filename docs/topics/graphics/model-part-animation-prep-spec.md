---
title: 可动画 3D 资产分件规范（Model Part Prep Spec）
type: reference
topic: graphics
date: 2026-09-20
author: AI + youxia
status: draft
tags: [art-pipeline, blender, gltf, animation]
related: ["decisions/0009-art-vault-to-game-asset-flow", "games/warden/docs/capability-cards.md"]
---

# 可动画 3D 资产分件规范

## 结论

任何预期做**程序化动画**的 3D 资产（机械类为主：炮塔/弩/门/升降台），入库或进游戏前必须按本规范「分件」：
要动的部分独立成网格、规范命名、轴心（origin）落在运动轴上。这是引擎侧程序化动画成立的前提合同——
业界策略游戏的炮塔动画主流就是「分件层级 + 代码驱动」（Rotor 节点/平滑炮塔资产/坦克后坐代码插值均为此路），
而它的前提是资产本身按可动画制作；AI 图生 3D 模型（Rodin 等）天然不满足，需一次「分件手术」。

## 正文

### 规范条目（合同）

1. **独立网格**：每个会动的部件是独立对象，不得与静态体焊接合并；
2. **命名**：snake_case 语义名（如 `string` 弦 / `bolt_slider` 箭滑轨 / `rotor_yaw` 转座），禁用工具自动生成名；
3. **轴心**：对象 origin 放在运动参考点——滑动件 origin 落在滑动轴上，转动件 origin 落在转轴上（glTF 里 origin 即引擎侧的旋转/平移参考点）；
4. **运动约定**：随资产文档化每个件的运动类型（沿哪个本地轴平移 / 绕哪个轴旋转）与行程范围；
5. **导出**：glTF；单位/包围盒归一（延续各游戏既有资产约定，warden 参见其 art 测试）；
6. **验收**：结构断言（节点存在 + 命名 + 轴心位置）进各游戏的美术资产测试（warden 先例：`tests/art_assets.rs`）。

### 执行与升格路径

- 手术工具：Blender 无头 Python 脚本（AI 可编写），成品经 [[decisions/0009-art-vault-to-game-asset-flow|ADR-0009]] 回流岩库存新版本；
- **第 1 例**（warden 弩 v3，ART9）：定向脚本进 `tools/` + 操作记录（含踩坑）进知识库——本规范首次实施，实施后复核条目再升 `done`；
- **升格判据**：第 2 个模型手术成功 → 脚本泛化（通用参数化）+ 升格为**用户级 skill**（跨项目、跨 AI 客户端复用）；
- 边界：角色/BOSS 等表现型资产走 K 帧动画线（绑骨+片段+引擎播放），不适用本规范；机械件优先本规范。

## 参考

- ADR-0009 岩库↔游戏资产流（回流通道）
- warden 卡路书 ART7–ART10（弩射击动作线，本规范首个消费者）
- 业界依据：策略游戏炮塔程序化动画（Nuclex Games turret rotor、SmoothTurret、坦克后坐代码插值等，调研于 2026-09-20 需求会话）
