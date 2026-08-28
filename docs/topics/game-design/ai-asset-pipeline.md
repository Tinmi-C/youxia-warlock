---
title: AI 生成美术管线八站流程（含玩家模型实战数据）
type: howto
topic: game-design/art
date: 2026-08-28
author: AI（youxia 确认沉淀）
status: draft
tags: [ai-generation, art-pipeline, normalize, workflow, prompt]
related: [topics/game-design/ai-3d-generation-tools, topics/game-design/blender-gltf-wash-pitfalls, "games/wave-survival/docs/style-bible"]
---

# AI 生成美术管线八站流程

> 与素材包采购路线共用第 5-8 站——"过线即入族"：来源随便，洗白后规格同一。

## 八站全景

```
[0 设计输入]  需求单：怪名 + 目标身高 + 一句气质        人（1 行字）
[1 概念定调]  style-bible prompt 模板 → 文生概念图 ×N   人挑 1 张定调图 ★风格锚
[2 批量抽卡]  定调图 → image-to-3D ×4~8                人挑 1 + 核对商用条款
[3 原始入库]  导出 glTF → _art/raw/ai/（gitignored）    机器落地
[4 动画方案]  人形→自动绑骨(Mixamo)；团状怪→程序动画；  按类型三选一
              或混搭素材包 clip
[5 洗白站]    normalize.py                              ✅全自动（自检把关）
[6 复检站]    turntable.py → 4 视角图册 + meta.json     ✅全自动 + 人翻图验收
[7 入库]      assets/models/xxx.glb + 敌人定义表加一行
[8 进游戏]    spawn 读表（卡 19 已落地）
```

分工原则：**审美决定权（挑图/挑模型/验收）永远是人**（团队 ADR-0002 分工线）；机械劳动全部脚本化。

## 已建成的两站：命令速查

```
blender -b -P tools/art/normalize.py -- --in <raw> --out <assets/models/name.glb> ^
        --height <米> [--max-tris <预算>] [--tex-size <像素>]
blender -b -P tools/art/turntable.py -- --in <模型目录> --out <图册目录>
```

- normalize 内置：帮手网格剔除 → 身高归一 → 脚底原点 → clip 改名（idle/walk/attack/death/hit）→ 变换烘焙 → 三角预算（decimate）→ 贴图预算 → GLB 导出 → **JSON 级自检（leaked 非空即失败退出）**
- turntable 产出：每模型 4 视角 PNG + meta.json（身高/bbox/minZ/碰撞半径候选/面数/材质/clips）
- clip 改名表在 normalize 顶部 `CLIP_MAP` 常量，换素材来源时按需加行

## 玩家模型实战数据（2026-08-28，混元网页版首例）

| 项 | 数值 | 说明 |
|----|------|------|
| 原始身高 | 1.14m | 洗白 → 精确 1.2m，脚底 minZ=0 |
| 面数 | 50,000 → 9,999 | 网页版默认 5 万面，`--max-tris 10000` 衰减 |
| 贴图 | 3 张（albedo+MR+normal） | `--tex-size 1024` 全部降采样 |
| 体积 | 30.9MB → 3.32MB | 终点等 palette lock（预计数百 KB） |
| 动画 | 0 clips | 静态模型；动起来走站 4 |
| 人工复检 | 两轮通过 | 第二轮专看减面剪影与 UV 拉伸 |

报废率预期：人脸/手是重灾区，首轮崩了重抽属正常流程，不是事故。

## Prompt 模板（定调图用，槽位加粗）

```
Full body front view of a cute low-poly game character, <角色描述 + 主色 hex>,
<体型比例：如 2.5 head-heights>, standing in A-pose, arms slightly spread,
flat shading, clean silhouette, plain light background, single character
--no realistic, PBR, metal, gradient, busy background, text, watermark
```

规则：模板整体复用不许裸写；主色从 style-bible 十色板取；只换槽位。挑图三标准：剪影一眼可读 / 脸顺眼 / A 字站姿（别选带飞行道具、侧身、坐姿）。

## 边界（如实）

- 脚本只**改名**动画，不**新造**动画；缺 clip 靠换素材或站 4 补
- palette lock 未启用（等 style-bible 十色板 review 通过后接入 normalize）
- 拓扑不可修复：AI 网格是密三角，低模平涂风对拓扑宽容，但别指望后续精修变形
- 本地混元部署（WinPortable，mini/turbo）与本地文生图（SD/ComfyUI）待立项；8GB 显存可行，见 [[topics/game-design/ai-3d-generation-tools|工具调研]]
