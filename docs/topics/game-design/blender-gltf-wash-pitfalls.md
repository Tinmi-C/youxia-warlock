---
title: 踩坑：Blender 5.x 无头 glTF 洗白五连坑（幻影网格/蒙皮缩放/失效引用/cm骨架动画膨胀/骨骼局部轴）
type: pitfall
topic: game-design/art
date: 2026-08-28
author: AI（youxia 确认沉淀）
status: draft
tags: [blender, gltf, headless, art-pipeline, normalize, mixamo, animation]
related: [topics/game-design/art-pipeline-3d-v2, tools/art/normalize.py, tools/art/turntable.py, tools/art/mixamo_merge.py]
---

# 踩坑：Blender 5.x 无头 glTF 洗白五连坑

> 背景：卡 17 ArtAssetPipeline 实操（Blender 5.2.1 LTS 无头洗白 Quaternius 怪包 + 混元网页版玩家模型）中连续踩到三个坑，玩家绑骨（Mixamo 路线）再补两个。五个都已修复并有自动化防线，沉淀防复发。

## 坑 1：幻影 Icosphere——导入器凭空造网格，污染测量与渲染

- **现象**：所有蒙皮 glb 导入 Blender 后，`bpy.data.objects` 里多出一个 2m 的 `Icosphere`（无骨骼、位于世界原点、z∈[-1,1]）。原始 .gltf 的 JSON 里**根本没有**这个节点。后果三连：身高测量全族虚高（GreenBlob 实为 1.86m，量出 2.85m）、渲染图里多一颗挡镜头的大球、删除后"自检复导入"又见到它——一度误判为"导出器还魂"。
- **根因**：Blender 5.x glTF 导入器在导入蒙皮网格时会自行创建辅助对象（具体机制未深究，[待验证]）。文件本身是干净的；连"删除后导出又出现"也是假象——自检用的重导入自己又造了一个。
- **解决**：①对象级剔除——场景存在骨骼链下的蒙皮网格时，游离网格一律删（`purge_helper_meshes`，按名字操作）；②**自检不信任 Blender 重导入**：直接解析导出文件的 glTF JSON chunk（`read_glb_json`），断言节点集合干净（`leaked=[]` 才放行）。
- **反思**：验证器不能复用会引入伪影的通道。测量与断言要走"事实来源"（文件本体），不要走"观察介质"（导入器）。

## 坑 2：蒙皮网格不吃父级变换——缩放导出即回弹

- **现象**：给根节点设 0.316 缩放、脚底对位后导出 GLB，复测身高与地面偏移全错。
- **根因**：glTF 导出蒙皮网格时几何写在 armature 空间，父级节点上的缩放/位移不参与蒙皮几何。
- **解决**：导出前 `bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)` 把全部变换烘焙进物体数据。洗白蒙皮模型这一步是**必选项**，不是可选项。
- **反思**：DCC 导出器有自己"眼里 only"的数据；凡依赖节点变换的做法，蒙皮场景下都要假设会丢。

## 坑 3：场景重置后残留 StructRNA 引用 → `ReferenceError`

- **现象**：批处理第 2 个模型时在 `import_glb` 内崩溃：`ReferenceError: StructRNA of type Object has been removed`。且只对"会产生可清除对象"的文件触发（纯净文件不触发），极难归因。
- **根因**：`read_factory_settings` 重置或 `bpy.data.objects.remove` 之后，先前持有的 Python 对象包装器变成"尸体"；对它们做 `in`/成员比较/属性访问即抛错。
- **解决**：全程**按名字索引**——快照名字集合（字符串稳定），每轮用 `bpy.data.objects.get(name)` 取新鲜引用；不跨场景生命周期持有任何 wrapper。
- **反思**：bpy 的 Python 句柄是视图不是所有权；把"名字"当主键，把"对象"当缓存。

## 坑 4：cm 骨架烤变换后动画位移膨胀 ~100 倍

- **现象**：Mixamo 绑骨模型洗白后静态姿势完全正常（贴地、身高对），一播动画角色就"飞走"——walk 一圈漂 76m、attack 打到 Y=-87m。骨骼、蒙皮、clip 名单全都看似完好。
- **根因**：Mixamo FBX 的骨架物体自带 `scale=0.01`（cm→m 换算）。洗白站的 `transform_apply` 把物体缩放烤进骨骼静置数据，但 **pose 位移 fcurve 的数值仍是烤前骨架空间的单位**，作用在 scale=1 的骨架上全部放大 ~100 倍。Quaternius 系模型从 glb 进来骨架本来就是 1.0，apply 是无操作，所以从未触发——这是一颗只对"带动画的外部 DCC 资产"生效的雷。
- **解决**：normalize 在 transform_apply 之后检测每个骨架的缩放系数，把所有 pose location 曲线（keyframe 坐标 + 手柄）按同系数回缩。系数 1.0 时零操作，老模型零影响。注意 Blender 4.4+ slotted actions 里 fcurve 在 `layers→strips→channelbags` 下，`action.fcurves` 已不存在，访问要跨 API 代际兼容。
- **反思**：「烤变换」对动画数据不是纯几何操作——骨架缩放同时存在于**物体变换**和**动画通道**两处，烤一处必须同步另一处。凡对 skinned+animated 资产做空间变换，先问一句：动画通道的单位跟着变了吗？

## 坑 5：骨骼局部轴 ≠ 世界轴——In-Place 剥错通道

- **现象**：想让动画原位化（游戏驱动位置，动画不能带位移），按世界轴直觉剥掉 Hips 的 X/Y 位移通道。结果角色仍在前进（walk 一圈漂 0.8m），上下弹跳反而消失。
- **根因**：Mixamo 骨骼是 FBX Y-up 惯例，Hips 骨骼竖直朝上——**骨骼局部 Y = 世界上方向，局部 X/Z 才是水平面**。fcurve 的 `array_index` 是骨骼局部坐标，不是世界坐标；按世界轴直觉剥 X/Y 恰好剥反。
- **解决**：剥 array_index 0 和 2（局部 X/Z = 水平位移），保留 1（局部 Y = 上下弹跳）。
- **反思**：动画通道的坐标系是"骨骼局部"，不是世界。处理骨骼动画前先用**数值范围诊断**定位（本次靠逐曲线 span 解剖：idx=2 span≈78 单位=0.78m，正好等于观察到的每圈位移；另两根是平的），让数据说话，别按轴名直觉动手。另外诊断脚本测量对象必须**点名**（如 `node_0`）——"第一个网格"往往是导入器造的幻影 Icosphere（坑 1），本次一度据此误判"绑定坏了"。

## 防复发基建（已进工具）

- `tools/art/normalize.py`：JSON 级自检（`leaked` 断言）+ 名字索引 + transform 烘焙 + cm 骨架动画曲线回缩（坑 4 防线），全部内置
- `tools/art/mixamo_merge.py`：Mixamo 多段动画合并（stub 动作清除 + In-Place 水平通道剥离，坑 5 防线）
- `tools/art/turntable.py`：同样的剔除与名字索引规则
- 复检实测数据（身高/面数/minZ）以 meta.json 落盘，人工验收有据可查；动画类资产加测"逐动作顶点级 minZ/X/Y 范围"（贴地 + 原位双断言）
