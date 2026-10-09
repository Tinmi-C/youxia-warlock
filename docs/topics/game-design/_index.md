# 游戏设计 Game Design

玩法循环、关卡设计、手感、数值、玩家体验。

## 笔记列表

| 笔记 | 类型 | 状态 | 一句话 |
|------|------|------|--------|
| [[topics/game-design/art-pipeline-human-ai-division\|美术与动作阶段的人机协作]] | reference | draft | 规格归 AI、审美归人；表现层 80% 是代码；动作=采购+组装；占位图双价值 |
| [[topics/game-design/art-style-and-pipeline-fundamentals\|美术风格与管线基础]] | reference | draft | 2D/3D 是不可逆第一决策；风格统一=共享约束；套装/变体与 ECS 同构；物理跟玩法维度走 |
| [[topics/game-design/art-pipeline-3d-v2\|3D 美术生产管线 v2]] | reference | draft | 规格≠数值双轨并行；按族生产；混合获取+洗白收口；工具=外部优先、运行时耦合处才自研 |
| [[topics/game-design/blender-gltf-wash-pitfalls\|踩坑：Blender 无头 glTF 洗白五连坑]] | pitfall | draft | 幻影 Icosphere 污染测量→自检读文件 JSON；蒙皮不吃父级变换须烘焙；按名字索引防失效引用；cm 骨架烤变换后动画曲线须同比回缩；骨骼局部轴≠世界轴（In-Place 剥局部 X/Z 留 Y） |
| [[topics/game-design/ai-3d-generation-tools\|AI 生成 3D 资产工具调研]] | reference | draft | 混元开源版+洗白站补色=定版基座；4060Ti 8GB 跑 mini/turbo；纹理短板被 palette lock 抵消；商用前人工读 LICENSE |
| [[topics/game-design/ai-asset-pipeline\|AI 生成美术管线八站流程]] | howto | draft | 八站全景与人机分工；normalize/turntable 命令速查；玩家模型实测数据（50k→10k 面）；prompt 模板与边界 |
| [[topics/game-design/unirig-auto-rigging-research\|UniRig 自动绑骨调研]] | reference | draft | 只绑骨不产动画；GLB/FBX 进 FBX 出；4060Ti 8GB 可跑可脚本化；许可证待人工核对；最大缺环=动画重定向（模板骨架方案先 PoC） |
| [[topics/game-design/art-asset-catalog-tool-proposal\|美术资产管理系统功能设计（art-catalog 设计稿 v0.4）]] | reference | draft | 人操作 AI 辅助；两层资产域（库上移工作区级 `_library/`）；六模块 + 场景注册表（SC1-6 实录提炼 + 扩展位）+ 模板兼容规范层 C1-C4；评审清单全部落定；待动工 |
| [[topics/game-design/design-target-anchors\|设计目标锚点框架（平衡数值方法论）]] | howto | draft | 先定目标体验→反推数值；5 字段锚点把体感难易变可测指标 + headless 读数闭环；锚点机制跨游戏通用、具体数值各藏；数据驱动≠外部化（见 ADR-0008） |
| [[topics/game-design/wc3-rpg-td-map-genres\|魔兽 RPG 塔防地图品类调研——循环圈与「一基座多地图」生态]] | reference | draft | 循环圈=漏怪不死绕圈+活怪存量失败线（绿圈 100/水TD 漏队友区）+英雄塔八面养成；品类图谱（迷宫/宝石/元素/军团）；魔兽生态三层结构（Terrain/Object 数据驱动/Trigger→JASS）→ 自研「runtime + 地图 bundle」映射 |
| [[topics/game-design/artvault-demand-board-proposal\|岩库需求板块设计稿——游戏美术需求的声明、跟踪与双侧联调]] | reference | accepted（P1 落地） | 岩库新增 Demand 一等公民：需求状态机 draft→locked/changed；游戏侧以 MapBundle def 为需求源（tools/art-needs 扫描投影）+ art-mapping 换装面 + 锁清单回链 demand_id；CLI 先行 UI 后置；管理流与构建流解耦红线；P1 JSON/CLI 最小闭环→P2 看板→P3 变更回流；**P1 已落地 2026-10-09**（岩库 ADR-006 + ADR-0015，loop-td 16 需求上板） |
| [[topics/game-design/loop-td-game-proposal\|循环圈 TD 需求草案 v0.2——基底验证计划的第一张地图]] | reference | draft | 主产品=warbase 基底，本图是第一消费方兼验收场（四条验收：薄游戏/headless/迷宫冒烟/全落基底能力）；玩法层：单环+自由建塔+10 塔三档+英雄塔三层养成+金木人口+30 波；P0–P3 阶段交替推进基底（BX 卡线）与游戏卡；基底内容已移 warbase-architecture/ADR-0012 |

## 相关决策

- （待补充）

## 相关踩坑

搜索 `type:pitfall #game-design`（或在标签页过滤）。
