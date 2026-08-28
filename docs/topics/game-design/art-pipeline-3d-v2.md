---
title: 3D 美术生产管线 v2——双轨并行、按族生产与工具链
type: reference
topic: game-design/art
date: 2026-08-27
author: AI + youxia
status: draft
tags: [art-pipeline, 3d, low-poly, ai-generation, tooling]
related: [topics/game-design/art-style-and-pipeline-fundamentals, topics/game-design/art-pipeline-human-ai-division, "games/wave-survival/docs/style-bible", "games/wave-survival/docs/capability-cards"]
---

# 3D 美术生产管线 v2——双轨并行、按族生产与工具链

## 结论

四条经讨论修正的管线决策：① **规格 ≠ 数值**——美术线的开工输入只有「视觉宪法」（风格圣经），玩法数值可以永远慢半拍，两条轨只在敌人定义表一行处会合；② 生产粒度是**族**不是单怪，一次生产覆盖一族敌人（与 ECS 数值分化对称）；③ 获取路线**分品类混合**（角色 AI 生成主力、环境套装采购），且「AI vs 套装谁当主力」由对比 spike 用数据裁决——统一性靠洗白流水线保证（过线即入族），不靠来源自觉；④ 工具策略：**外部现成优先，自研只发生在与 Bevy 运行时耦合的环节**。

来源：2026-08-27 与 youxia 的管线讨论（对 v1 六幕串行版的三条修正）；上游理论见 [[topics/game-design/art-style-and-pipeline-fundamentals|美术风格与管线基础]]。

## 正文

### 双轨并行（对 v1「规格先行」的修正）

```
玩法线（数值）：  波次公式 → 敌人数值 …………………… 可以永远慢半拍
                                                          │
美术线（视觉）：  视觉宪法 ──► 按族生产 ──► 洗白 ──► 入库   │
                                                          ▼
                          唯一会合点：敌人定义表一行（模型名+碰撞半径 ←→ hp/speed）
```

- 美术开工六件套：色板槽位数 / 身高区间 / 头身比 / 是否人形 / 动作清单 / 命名约定——半小时可定，不含任何 hp/damage。
- 落地载体：风格圣经 `games/wave-survival/docs/style-bible.md`（v0）。

### 六幕 v2（骨架同 v1，三处修正）

| 幕 | 内容 | v2 修正点 |
|----|------|-----------|
| 0 规格先行 | 能力卡写视觉规格 | 拆走数值依赖，只依赖视觉宪法 |
| 1 获取 | 产出候选资产 | 分品类混合获取（见下表），不做单一来源承诺 |
| 2 规格化 | 洗白流水线 | Blender 无头脚本（卡 17），过线即入族 |
| 3 审批 | AssetPreview 出图 20 选 1 | 否决 Bevy 自研 bin，改 Blender 批量出图册 |
| 4 引擎冒烟 | 加载/动画/包围盒 | 两帧 diff 证伪 T-pose（ADR-0004 spike 手法）；bbox → 碰撞半径候选值 |
| 5 数据接入 | 定义表加一行 + 验收闭环 | 老系统零改动；动画来源在采购定型后落定 |

### 获取策略：分品类混合 + 洗白收口

| 资产类 | 特点 | 获取路线 |
|--------|------|----------|
| 主角 + 怪物族（~7 基础模型） | 数量少、辨识压力高、要专属感 | AI 生成（Meshy / Tripo / Hunyuan3D-2.x）出坯 → 人挑 → 洗白 |
| 环境 / 道具 / 障碍 | 量大、辨识压力低 | CC0 套装采购（Kenney 环境包；KayKit 系造型语言同源） |
| 动画 | 人形标准动作 | Mixamo 自动绑骨 + 动作库（人形约束是前提） |

- 纯生成路线的最大暗坑不是单体质量，是**跨批次一致性**（两族感）；纯套装路线的风险是跨包打架。解法相同：把统一性从「来源问题」变成「管线问题」——量化回色板 + 归一化 + 命名规范，三道洗白后无人记得它从哪来。
- 「AI 生成 vs 套装谁当主力」由卡 18 spike 裁决（墙钟/花费/符合度/区分度/观感/踩坑六项度量），结论回写 GDD 点子池。

### 工具链：三站 + 一条总线

| 站点 | 工具 | 覆盖环节 | 自研？ |
|------|------|----------|--------|
| 审阅站（浏览器） | Babylon.js Sandbox（层级/材质/**动画 clip 试播**）、gltf.report（面数/尺寸统计） | 幕 3 人工审 + 幕 4 冒烟人工侧 | ❌ 零成本 |
| 出图+洗白站 | Blender 无头（`-b -P`）：`tools/art/turntable.py`（批量图册 PNG + meta.json）、`normalize.py`（轴向/原点/身高/色板量化） | 幕 2、幕 3、bbox→碰撞半径 | ⚠️ 仅 Python 脚本（卡 17） |
| 游戏内站 | bevy_egui 扩展：G 键资产阵列巡检（AssetGallery 具体形态）、复用卡 11 面板体系 | 幕 5 后的调参、终审 | ✅ 合理自研（运行时耦合） |
| 进度总线（已有） | 占位自动生成 + `info!` 日志仪表 | 全程进度可视化 | ❌ 已是既定机制 |

自研原则：**只在「与 Bevy 运行时耦合」处自研**（egui 调参、占位仪表盘）；查看器/转换器/绑骨/出图全部外部工具 + 脚本粘合。

### 2D 对照（为什么本文只写 3D）

五段骨架相同，血肉不同：资产形态（图集 PNG vs 网格+骨骼+clip 四件套）、规格化动作（像素密度/pivot vs 轴向/米制/脚底原点）、朝向表现（多向帧 vs 旋转 Transform 白送）、动画来源（逐帧/Spine vs 采购 clips）、校验仪器（平铺图 vs 转台出图）、大坑（图集重排 vs 跨包单位灾难 + FBX→glTF）。wave-survival 已锁 3D 底座（ADR-0004，2D/3D 不可逆），2D 线留档不展开。

## 参考

- 风格圣经 v0：`games/wave-survival/docs/style-bible.md`（视觉宪法的落地文档）
- 卡 17 / 卡 18 草案：`games/wave-survival/docs/capability-cards.md`
- 上游理论：[[topics/game-design/art-style-and-pipeline-fundamentals]]、[[topics/game-design/art-pipeline-human-ai-division]]
- 外部工具：Babylon Sandbox（doc.babylonjs.com/toolsAndResources/sandbox）、gltf.report、Blender CLI、Meshy/Tripo、Hunyuan3D-2.1（github.com/perfectproducts/Hunyuan3D-2.1）、Mixamo
- 维度决策：ADR-0004（3D 底座 + Bevy 全引擎）
