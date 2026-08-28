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

## 相关决策

- （待补充）

## 相关踩坑

搜索 `type:pitfall #game-design`（或在标签页过滤）。
