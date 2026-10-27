---
title: 项目美术资产分类规格——六大类资产的需求源/规格/验收/接入缝/数量预估
type: reference
topic: art
date: 2026-10-29
author: AI 起草（ZCode），youxia 定向细化
status: draft
tags: [art-pipeline, asset-spec, taxonomy]
related: ["[[topics/art/game-art-cross-project-workflow|跨项目工作流]]", "[[topics/art/ui-2d-asset-spec|UI 2D 资产规范]]", "[[topics/graphics/model-part-animation-prep-spec|分件规范]]", "[[topics/art/art-pipeline-overview|管线总纲]]"]
---

# 项目美术资产分类规格（六大类）

> 配套[[topics/art/game-art-cross-project-workflow|跨项目工作流]]的「运什么」清单。每类给：需求源（投影从哪来）/ 规格要点 / 验收门禁 / 游戏侧接入缝 / 数量预估与优先级。优先级 P0=美术解禁后第一批。

## 1. UI 2D 资产（P0，一图通用基底）

- **子类**：①面板九宫格底材（底栏控制台/信息区/资源铭牌/弹卡/终局屏 ≈8-10 张）；②图标（命令卡按钮=每游戏建造/技能项、物品、羁绊、英雄头像 ≈30-50 枚）；③数字/标题美术字（可选）。
- **需求源**：UI 设计系统（方案 A brief + token 表）+ 各游戏命令卡/物品表 def——**待补投影**（art-needs 扩 2D 源）。
- **规格**：色彩严格贴方案 A token（换肤一致性）；九宫格切分按[[topics/art/ui-2d-asset-spec|UI 2D 规范]]；图标统一栅格（如 64/128px）；灰盒与贴图双形态。
- **验收**：九宫格拉伸无畸变（程序化检查）+ 紧裁剪双帧对比 + token 色差容差。
- **接入缝**：UiTheme 加贴图字段（美术管线 SEAM 待做）——面板渲染 token 色块→贴图回退。

## 2. 单位/建筑 3D 模型（P0 loop-td / P1 warden / P2 rts-lab·legion）

- **子类与数量**：loop-td：塔 ~8 型×3 档（档间可色变共享）+ 怪 5 种 + BOSS + 英雄塔（**16 需求已上板**）；warden：塔阵容重做（ART10 挂起的前瞻）+ 敌人文；rts-lab：建筑 5（主城/兵营/矿/商店/祭坛）+ 单位 ~6；legion：雇佣兵 6 种。
- **需求源**：MapBundle units 表（id/attrs/body_radius/朝向用途）——**投影已就绪**。
- **规格**：GLB；分件按[[topics/graphics/model-part-animation-prep-spec|分件规范]]（可动画部件独立节点+变换自带）；体量对齐 body_radius（命中半径=视觉半径）；原点居中、朝向轴统一；两套相机规格（warden 3/4 透视 / loop-td·rts-lab 正交俯视——俯视图可低面数简模）。
- **验收**：结构断言（节点拓扑/顶点守恒，art_assets.rs 模式）+ 转台 4 视角像素差 + 分件 bbox 基准。
- **接入缝**：def→模型映射表（art-mapping）+ 消息驱动动画机（ART10 模式：AttackFired/MoveTick/DeathTick → 部件动画）。

## 3. 动画资产（与模型一体，单列契约）

- **集**：idle/移动/攻击（前后摇对齐 SKVOC 的时间窗！）/施法/死亡；wing 弹道动画（弩弦案例推广）。
- **需求源**：单位 def 的 attack（interval/windup）与技能表——动画时长须与逻辑帧窗对齐（**数值对齐是验收项**）。
- **验收**：消息触发→部件状态断言（行为测试）+ 视觉双帧。

## 4. 特效 VFX（P2）

- **子类**：弹道 4-6 种（箭/炮/魔弹已有程序化 bolt 先例）；命中/爆炸/死亡 4 种（现几何占位 BlastRing/DeathShell）；光环表现 3 种（devotion/羁绊/神器金环已有先例）；伤害飘字字模（颜色分普通/暴击/治疗）。
- **需求源**：fx 域语义消息契约（FxBlast/SfxKind/DamageDealt 分类表）——**消息即接口，美术按消息出资产**。
- **规格**：序列帧或粒子（bevy_hanabi 届时 spike）；缩放基准=body_radius；循环/单次标注。
- **验收**：触发语义断言（消息→特效实体出现）+ 视觉双帧。

## 5. 环境/地图美术（P3）

- **子类**：地面材质（三图各一基调）、路径可视化（循环圈跑道/发兵路）、建造格提示、迷雾边缘软化、小地图静态底图。
- **需求源**：bundle world 段（paths/blocked/build 域几何）——投影待补。
- **验收**：视觉双帧 + 与单位模型同光基准。

## 6. 音频（P2）

- **子类**：SFX ≈12 类×1-3 变体（ClickAck/OrderAck/Error/Cast/Hit/Death/WaveStart…SfxKind 枚举即需求表）；BGM 2-3 曲（主菜单/战斗，可选）。
- **需求源**：SfxKind 枚举（**已就绪**，通道与节流已验证）。
- **规格**：ogg、时长基准（Ack≤0.5s）、音量归一、同类节流 0.1s 纪律适配。
- **验收**：通道 round-trip 断言 + 人工听感关卡。

## 汇总：优先级与批次建议

| 批 | 内容 | 理由 |
|---|------|------|
| P0 | UI 贴图+图标；loop-td 单位模型 | 方案 A 已定稿即开工；loop-td v2 是旗舰验收图且需求已上板 |
| P1 | warden 塔阵容重做+敌人文 | 弥补 ART10 挂起；warden 美术基础最好 |
| P2 | rts-lab/legion 模型；VFX；音频 | 验收图补完；特效音效整体拉升 |
| P3 | 环境地图美术 | 锦上添花 |
