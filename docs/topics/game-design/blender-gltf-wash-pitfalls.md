---
title: 踩坑：Blender 5.x 无头 glTF 洗白三连坑（幻影网格/蒙皮缩放/失效引用）
type: pitfall
topic: game-design/art
date: 2026-08-28
author: AI（youxia 确认沉淀）
status: draft
tags: [blender, gltf, headless, art-pipeline, normalize]
related: [topics/game-design/art-pipeline-3d-v2, tools/art/normalize.py, tools/art/turntable.py]
---

# 踩坑：Blender 5.x 无头 glTF 洗白三连坑

> 背景：卡 17 ArtAssetPipeline 实操（Blender 5.2.1 LTS 无头洗白 Quaternius 怪包 + 混元网页版玩家模型）中连续踩到三个坑。三个都已修复并有自动化防线，沉淀防复发。

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

## 防复发基建（已进工具）

- `tools/art/normalize.py`：JSON 级自检（`leaked` 断言）+ 名字索引 + transform 烘焙，全部内置
- `tools/art/turntable.py`：同样的剔除与名字索引规则
- 复检实测数据（身高/面数/minZ）以 meta.json 落盘，人工验收有据可查
