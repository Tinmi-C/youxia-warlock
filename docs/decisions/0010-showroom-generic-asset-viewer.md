---
title: ADR-0010
type: adr
status: accepted
date: 2026-09-20
author: AI + youxia
---

# ADR-0010: 展示场（美术资产查看器）通用化——抽入 engine/showroom-viewer

## 背景

Warden 的 SM1 展示场（中性影棚资产查看器，v2/v2.1）最初长在游戏内：资产目录读 Warden 定义表、
展品组装调 Warden 外观缝、场景入口是 DB2 的 `--scenario showroom`。负责人要求该工具**跨游戏通用**
（工作区还有 wave-survival / bevy-spike 等项目，且美术资产统一来自岩库 ArtVault）。

`engine/README.md` 的沉淀判据是"第 2 次被另一个游戏用到才抽"。本次抽取后立即有**两个消费方**：
Warden 适配层 + 独立 `asset-viewer` bin（扫任意目录看 GLB，零接入），判据成立。

## 决策

**通用机制上收为共享 crate `engine/showroom-viewer`；游戏只留薄注册层。**

- crate 提供：中性影棚环境（地面/展示台/白主光/环境光/清屏色）、进场清扫 + 驻守清扫（异步场景流窜自愈）、
  资产目录模型（`ShowroomCatalog`：分类 → 条目，条目携带名字/数值信息/铭牌文本/根实体装饰闭包）、
  egui 列表面板（egui 不在场时自动不注册）、环绕缩放相机、Tab/←→ 键盘流。全部由资源存在性门控
  （`ShowroomState`），不认识任何游戏的状态机。
- 游戏侧职责：构建自己的目录（读自家定义表，闭包内调自家组装缝）、豁免必须存活的实体
  （插 `ShowroomKept` 标记）、切自己的 GameState 冻结玩法。
- 独立 bin `asset-viewer`：DefaultPlugins + 相机 + 目录扫描（.glb/.gltf）→ 每个文件一个条目，
  `cargo run -p showroom-viewer --bin asset-viewer -- <dir>` 即看，不接入任何游戏。
- Warden 通过 path 依赖引入（依赖声明无条件、代码层 cfg 门控，同 DB1 的 bevy_egui 先例）。

## 备选方案

1. **保持游戏内**（SM1 v2 现状）：零抽象，但每个游戏重写一遍影棚/清扫/面板——与负责人"通用"诉求相悖。否。
2. **完全独立程序（不进游戏）**：最通用，但看不到游戏内组装形态（如 Warden 的塔+床弩组合）与定义表数值。否——
   作为 crate 的第二个消费方（bin）共存，两头都要。

## 影响 / 后果

- 优：影棚机制单源；新游戏接入 ≈ 一个目录构建函数；岩库资产不进游戏也能预览。
- 代价：多一个 crate 的维护与版本对齐（bevy 0.19.1 / bevy_egui 0.42 锁同版本）；warden 测试经 re-export 保持路径稳定。
- 边界：**不追求**展示游戏内运行时行为（开火/AI）——那是 SM3 实弹区（游戏内场景）的职责，永不上收。
