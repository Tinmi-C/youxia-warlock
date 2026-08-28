---
title: 索引地图 MOC
type: moc
updated: 2026-08-27
---

# 🗺️ 索引地图（Map of Content）

> 知识库首页。按主题浏览，或搜索 `type: pitfall` 看踩坑、`status: draft` 看草稿。

## 主题（topics/）

| 主题 | 入口 | 说明 |
|------|------|------|
| Rust | [[topics/rust/_index|Rust]] | 语言与生态 |
| 图形学 | [[topics/graphics/_index|Graphics]] | wgpu、WGSL、光照 |
| 引擎架构 | [[topics/engine/_index|Engine]] | ECS、场景图、资源管理 |
| 游戏设计 | [[topics/game-design/_index|Game Design]] | 玩法、关卡、手感 |

## 决策

- [[decisions/_index|架构决策 ADR 列表]]

## 参考

- [[glossary|术语表]]
- [[meta/templates/note-template|笔记模板]]
- [[raw/README|原始资料 raw]]
- [[log|操作日志]]
- [[AGENTS|维护规范 AGENTS.md]]

## 近期更新

- [[topics/engine/ai-native-engine-vision|AI 原生游戏引擎——设想总纲]]（新增，draft）
- 引入 `raw/` 层 + 维护规范 [[AGENTS]] + [[log]]（知识库 schema 升级）
- [[decisions/0001-rust-tech-stack|ADR-0001 技术栈选型]]（proposed）
- [[topics/engine/learning-roadmap|学习路线图]]（新增，draft）
- [[topics/graphics/wgpu-window-clear|wgpu 第一步：窗口 + 清屏]]（M1 第 1 步，draft）
- [[topics/graphics/wgpu-triangle|wgpu 第二步：三角形]]（M1 第 2 步，渲染管线入门，draft）
- [[topics/graphics/wgpu-quad-movement|wgpu 第三步：方块 + 移动]]（M1 第 3 步，uniform + delta time，draft）
- [[topics/graphics/wgpu-camera-mvp|wgpu 第四步：相机 + MVP]]（M1 终章，draft）
- [[topics/engine/engine-direction-discussion|引擎定位与团队方向讨论纪要]]（回写，draft）
- [[topics/engine/engine-concepts-map|引擎概念地图]]（新增，draft）
- [[decisions/0002-engine-scope-game-driven|ADR-0002 引擎边界]]（新增，proposed：游戏驱动、不做通用引擎、能力卡为核心资产）
- [[decisions/0003-toy-ecs-to-bevy-ecs|ADR-0003 玩具 ECS → bevy_ecs 迁移]]（新增，proposed：M3 前迁移，组件定义不变）
- [[topics/graphics/wgpu-ui-state-machine|wgpu 第五步：UI + 状态机]]（M2 第 3 步，UI 管线 + 游戏状态机，draft）
- [[topics/engine/bevy-ecs-world-reset-pitfall|踩坑：bevy_ecs 重开不能重建 World]]（新增，pitfall：mismatched World / clear_all 资源缓存失配）；ADR-0003 → accepted（m2-bevy 实施完成）
- [[decisions/0004-handwritten-renderer-to-bevy|ADR-0004 渲染底料开源化 → Bevy 全引擎]]（新增，proposed：手写 wgpu 渲染器存档封存，波次生存迁移 Bevy 并做成成品；AI 装配层 = 自研分量不变）
- [[topics/engine/bevy-plugin-and-code-reuse|Bevy 开发约定：插件决策与代码沉淀]]（新增，reference：客观→引用生态插件；主观→自研沉淀；第二次用到才抽 crate；已固化进 templates/bevy-game）
- [[topics/engine/bevy-windows-antivirus-build-pitfall|踩坑：Windows 杀软误报 ahash 构建脚本]]（新增，pitfall：os error 5；白名单 target/ + 工具链目录解决；模板 README 已加替换 crate 名步骤）
- [[topics/engine/bevy-019-events-to-messages-pitfall|踩坑：Bevy 0.19 缓冲事件更名 Message]]（新增，pitfall：Event→Message / add_event→add_message，wave-survival 卡 9 编译期发现；含 hanabi Gradient 撞名等版本对齐小坑）
- [[topics/engine/capability-card-workflow-deep-dive|能力卡机制深度理解]]（回写，reference：卡=需求规格；成果=增量+测试+归档；切卡=可独立验收的玩法链路；wave-survival 阶段一 8 卡用量表）
- [[topics/engine/ai-collaboration-by-phase|游戏全生命周期的人机分工]]（回写，reference：AI 可接管程度=验收句可数字化程度；立项/需求/试玩/构建分工 + 点子池模式）
- [[topics/game-design/art-pipeline-human-ai-division|美术与动作阶段的人机协作]]（回写，reference：规格归 AI 审美归人；表现层 80% 是代码；动作=采购+组装+参考驱动；占位图=进度解耦+判断纯度）
- [[topics/game-design/art-style-and-pipeline-fundamentals|美术风格与管线基础]]（回写，reference：2D/3D 不可逆第一决策；风格统一=共享约束；套装/变体派生与 ECS 同构；武器插槽=锚点+行为预告；物理跟玩法维度走）
- [[topics/engine/powershell-pipeline-utf8-corruption-pitfall|踩坑：PowerShell 文本管道写坏 UTF-8 源码]]（新增，pitfall：ANSI 双重转码 → 非法 UTF-8；改文件只走编辑器类工具或显式编码 API）
- [[topics/game-design/art-pipeline-3d-v2|3D 美术生产管线 v2：双轨并行与工具链]]（回写，draft：规格≠数值解耦、按族生产；AI+套装混合获取、洗白收口；Blender 流水线 + 现成 viewer，自研只在运行时耦合处；风格圣经 v0 + 卡 17/18 草案同批落地）
- [[topics/game-design/blender-gltf-wash-pitfalls|踩坑：Blender 无头 glTF 洗白三连坑]]（新增，pitfall：幻影网格污染测量→自检读文件 JSON；蒙皮变换烘焙；按名字索引防 StructRNA 尸体）
- [[topics/game-design/ai-3d-generation-tools|AI 生成 3D 资产工具调研]]（新增，reference：混元 2.0/2.1 开源 + 4060Ti 8GB 定版 mini/turbo；形状-only + palette lock 抵消纹理短板；商用前人工读 LICENSE）
- [[topics/game-design/ai-asset-pipeline|AI 生成美术管线八站流程]]（新增，howto：八站人机分工；normalize/turntable 速查；玩家模型 50k→10k 面/30.9→3.3MB 实测；prompt 模板）
