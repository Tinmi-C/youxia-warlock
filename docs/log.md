# 操作日志

记录知识库的每次变更：什么时候、谁、用什么资料、写了什么、改了哪些页面。用于团队同步 + 追溯。

## 格式

| 日期 | 操作 | 涉及资料 | 写入/修改页面 | 操作者 |
|------|------|----------|---------------|--------|
| 2026-08-18 | 初始化 | — | 全部骨架 | team |
| 2026-08-18 | 摄入 | 《AI 原生游戏引擎——设想总纲》 | topics/engine/ai-native-engine-vision.md | AI + 人 |
| 2026-08-19 | 维护 | 知识库 schema（raw/工作流/健康检查） | AGENTS.md、raw/、log.md、health-check-prompt.md | AI + 人 |
| 2026-08-19 | 维护 | 技术栈选型 | decisions/0001-rust-tech-stack.md（ADR-0001） | AI 起草，待团队 review |
| 2026-08-19 | 维护 | 学习路线规划 | topics/engine/learning-roadmap.md | AI 起草，待团队 review |
| 2026-08-19 | 维护 | 个人/团队内容分离 | _private/（git 忽略） | team |
| 2026-08-19 | 学习 | wgpu 窗口 + 清屏（M1 第 1 步） | topics/graphics/wgpu-window-clear.md | AI + youxia |
| 2026-08-19 | 回写 | 引擎定位与 demo 策略讨论 | topics/engine/engine-direction-discussion.md、topics/engine/engine-concepts-map.md | AI + youxia |
| 2026-08-19 | 学习 | M1 Step 1 Mac 验证 + 概念补课（代码结构 / RGB / 事件循环） | _private/learning/progress.md | AI + youxia |
| 2026-08-19 | 学习 | MoveSystem 能力卡练习完成（M1 Step 1 收尾，含 delta time 原理演示与验收句打磨） | _private/learning/capability-cards/movesystem.md | AI + youxia |
| 2026-08-19 | 学习 | M1 Step 2 三角形：渲染管线 + WGSL + 顶点缓冲（含两个验证实验与踩坑记录） | topics/graphics/wgpu-triangle.md | AI 起草，待 youxia review |
| 2026-08-19 | 学习 | RenderPipeline 能力卡完成（M1 Step 2 收尾） | _private/learning/capability-cards/renderpipeline.md | AI + youxia |
| 2026-08-19 | 学习 | M1 Step 3 方块+移动（验收实测：帧率无关速度）+ Step 4 相机+MVP（实验 3/4）；M1 四步全部完成 | topics/graphics/wgpu-quad-movement.md, topics/graphics/wgpu-camera-mvp.md | AI + youxia |
| 2026-08-19 | 维护 | 引擎实用性质疑讨论 → 引擎边界决策 | decisions/0002-engine-scope-game-driven.md（ADR-0002）、decisions/_index.md、MOC.md | AI 起草，待团队 review |
| 2026-08-19 | 学习 | M2 Step 1 玩具 ECS：10 实体场景 + ChaseSystem 生产者/消费者分工 + ToyECS 架构能力卡 | _private/learning/progress.md、_private/learning/capability-cards/toyecs.md | AI + youxia |
| 2026-08-19 | 学习 | M2 Step 2 战斗系统：碰撞 + despawn + 掉落拾取 + CombatSystem 能力卡 | _private/learning/progress.md、_private/learning/capability-cards/combatsystem.md | AI + youxia |
| 2026-08-19 | 学习 | M2 Step 3 UI + 状态机：UI 管线 + 血条 + GameState + GameState 能力卡 | topics/graphics/wgpu-ui-state-machine.md、_private/learning/progress.md、_private/learning/capability-cards/gamestate.md | AI + youxia |
| 2026-08-19 | 维护 | 外部架构提案分析（标准化资产池 7 层）+ bevy_ecs 迁移决策 | topics/engine/engine-direction-discussion.md（新增 §6）、decisions/0003-toy-ecs-to-bevy-ecs.md（ADR-0003） | AI 起草，待团队 review |
| 2026-08-21 | 学习 | bevy_ecs 迁移实施（m2-bevy）+ World 重置踩坑 ×2（mismatched World / clear_all 资源缓存失配）| topics/engine/bevy-ecs-world-reset-pitfall.md、decisions/0003（status→accepted + 实施记录）、glossary.md（+世代号/命令缓冲/Schedule）、_private/learning/progress.md | AI + youxia |
| 2026-08-22 | 学习 | m2-bevy R 键重开终验通过（fps≈120）+ git 存档（commit 8c68ebd）| _private/learning/progress.md | AI + youxia |
| 2026-08-22 | 学习 | M2 收官：两个学习项口述通过（Commands/ApplyDeferred 两段式 + bevy_ecs 三件新东西）| _private/learning/progress.md | AI + youxia |
| 2026-08-22 | 学习 | M3 开工 + Step 1 纹理完成（UV/采样器/repeat 平铺；棋盘格地板，完整回归验证）| _private/learning/progress.md | AI + youxia |
| 2026-08-22 | 学习 | M3 Step 1 收尾：Texture 能力卡（首张 asset 资源卡，选择题模式）| _private/learning/capability-cards/texture.md | AI + youxia |
| 2026-08-22 | 学习 | M3 Step 2 光照完成：立方体+法线+漫反射+环境光；验收/review/Lighting 能力卡（3/3）全闭环 | _private/learning/progress.md | AI + youxia |
| 2026-08-24 | 学习 | M3 Step 3 模型加载（glTF 鸭子+归一化烘焙+AssetLoading 卡）+ Step 4 实例化（1010 实体/2 draw calls+Instancing 卡）——M3 收官 | _private/learning/progress.md、_private/learning/capability-cards/assetloading.md、instancing.md | AI + youxia |
| 2026-08-24 | 学习 | 玩法扩展 Step 1 波次系统：Wave 资源 + wave_system 三态 + 混合递增 + WaveSystem 能力卡（第 10 张，轮询 vs 事件） | _private/learning/progress.md、_private/learning/capability-cards/wavesystem.md | AI + youxia |
| 2026-08-24 | 学习 | 玩法扩展 Step 2 范围斩：Nova 标签 + 特效实体 + write_buffer 语义大坑（每元素独立 buffer 根治）+ NovaSystem 能力卡（第 11 张） | _private/learning/progress.md、_private/learning/capability-cards/novasystem.md | AI + youxia |
| 2026-08-25 | 维护 | ADR-0004 起草：渲染底料开源化（手写 wgpu 渲染器 → Bevy 全引擎），波次生存做成品；背景 = CesiumMan/BrainStem 骨骼模型「不像人」根因（无蒙皮/多材质/骨骼动画） | decisions/0004-handwritten-renderer-to-bevy.md（proposed）、decisions/_index.md、MOC.md | AI 起草，youxia 方向已确认，待团队 review |
| 2026-08-25 | 维护 | ADR-0004 证据收集：bevy-spike（Bevy 0.19.1 全流程冒烟测试）无头验证通过——glTF 人形导入 + 骨骼动画 + 相机灯光 + UI + 构建 | games/bevy-spike/、decisions/0004（实施记录） | AI |
| 2026-08-25 | 维护 | 明确 Bevy = 团队默认游戏底座（ADR-0004 适用范围）+ 创建 `templates/bevy-game/`（Bevy 0.19 插件化骨架 + 回归测试 + 能力卡工作流，AI 协作原生）；模板编译/3 测试/无头运行全验证通过 | decisions/0004、templates/bevy-game/、templates/README.md | AI + youxia |
| 2026-08-25 | 回写 | 团队讨论固化两条免重复准则 → `topics/engine/bevy-plugin-and-code-reuse.md`（插件决策：客观→引用生态插件/主观→自研；代码沉淀：第二次用到才抽 crate；选型三原则）；同步固化进 templates/bevy-game（README + AGENTS.md） | topics/engine/bevy-plugin-and-code-reuse.md、topics/engine/_index.md、MOC.md、templates/bevy-game/README.md、templates/bevy-game/AGENTS.md | AI + youxia |
| 2026-08-25 | 维护 | 学习文档 Bevy 化 v2：`learning-roadmap.md` 重写为 Bevy 路线（阶段0→M1 垂直切片→M2 玩法深化→M3 表现层→M4 打磨发布）；`engine-concepts-map.md` 里程碑标注从 wgpu 手写改为 Bevy 使用 | topics/engine/learning-roadmap.md、topics/engine/engine-concepts-map.md | AI，待团队 review |
| 2026-08-26 | 维护 | 陈旧文档与 monorepo 约定对齐：games/engine/tools 三个 README 去除 polyrepo 残留（独立建仓 → 整体 monorepo、新游戏指引改为 bevy-game 模板）+ 删除 topics/未命名.base（Obsidian 残留，误提交） | games/README.md、engine/README.md、tools/README.md、topics/未命名.base（删除） | AI |
| 2026-08-26 | 维护 | 沉淀开发踩坑两件：① 模板复制改名后 `bevy_game` crate 名残留 → 首编 E0433（模板 README + games/README 新增「全局替换 crate 名」步骤）；② Windows 杀软（360）误报 ahash 构建脚本 → os error 5（新增踩坑页 + 白名单解法） | templates/bevy-game/README.md、games/README.md、topics/engine/bevy-windows-antivirus-build-pitfall.md（新增）、topics/engine/_index.md、MOC.md | AI |
| 2026-08-27 | 回写 | wave-survival 卡 9 实现中 Bevy 0.19 缓冲事件 API 编译失败 → 事件体系更名踩坑沉淀（Event→Message 三件套 + 无头测试读消息技巧 + hanabi/egui 版本对齐小坑） | topics/engine/bevy-019-events-to-messages-pitfall.md（新增）、topics/engine/_index.md、MOC.md | AI，待团队 review |
| 2026-08-27 | 回写 | wave-survival 阶段一代码 review + 流程讨论沉淀三篇：能力卡深度理解（卡的本质/成果/切法/复用率）、全生命周期人机分工（验收句可数字化程度定律 + 点子池）、美术与动作协作（规格 vs 审美 / 采购式动作设计 / 占位图双价值） | topics/engine/capability-card-workflow-deep-dive.md（新增）、topics/engine/ai-collaboration-by-phase.md（新增）、topics/game-design/art-pipeline-human-ai-division.md（新增）、两个 _index.md、MOC.md | AI + youxia |
| 2026-08-27 | 维护 | GDD 新增「点子池」小节：美术前置 4 项（AssetPreview 工具卡 / 风格圣经+调色板 / 采购策略 / 占位图系统）+ 阶段一 review 遗留问题 3 项（白闪缺口 / 重开清补给 / 日志占位符）；点子池 = 需求模糊态与就绪态的缓冲区，能写出验收句才升级成卡 | games/wave-survival/docs/GDD.md | AI + youxia |
| 2026-08-27 | 维护 | 点子池美术两条更新：采购策略精确到粒度（按套装采购，AI 生成只作套装粗坯须过风格圣经四条）；新增变体派生条目（一个模型撑一族敌人，与 ECS 数值分化对称设计，敌人定义表落地） | games/wave-survival/docs/GDD.md | AI + youxia |
| 2026-08-27 | 回写 | 美术风格与管线基础讨论（维度决策/风格坐标系/生产粒度/武器插槽/2D vs 3D 物理） | topics/game-design/art-style-and-pipeline-fundamentals.md（新增）、_index.md、MOC.md | AI + youxia |
| 2026-08-27 | 回写 | PowerShell 文本管道写坏 UTF-8 源码事故沉淀（阶段三真机会话确认授权）：ANSI 双重转码根因分析 + 「改文件只走编辑器类工具/显式编码」防复发规则 | topics/engine/powershell-pipeline-utf8-corruption-pitfall.md（新增）、topics/engine/_index.md、MOC.md、log.md | AI，youxia 授权 |

## 操作类型

- `摄入` — raw → topics 编译
- `回写` — 问答结论沉淀
- `健康检查` — 修复死链/孤立页/冲突等
- `维护` — 改模板、改规范、改索引
