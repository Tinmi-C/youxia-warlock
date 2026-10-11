---
title: 分件手术记录：weapon_ballista v2→v3（ART9）
type: howto
topic: art
date: 2026-10-11
author: AI 主刀（youxia 委托）
status: done
tags: [art-pipeline, blender, surgery, gltf, warden]
related: ["topics/art/surgery-workflow", "topics/graphics/model-part-animation-prep-spec", "topics/art/visual-acceptance-methods", "topics/art/generated-mesh-coincident-vertex-blob", "topics/art/blender-workbench-headless-color-pitfall"]
---

# 分件手术记录：weapon_ballista v2 → v3

分件手术流程的首例实施（[[topics/art/surgery-workflow|流程]] ①-⑧ 全步走通）。

## 版本与工具

- 输入：岩库 `weapon_ballista` **v2**（id `d04b98c044fc4a3580962efe677566bf`，9625 顶点 / 10000 面 / 1 网格 / 766 边连通岛）
- 输出：**v3**（version_id `8130b070eb4440d6b5a58d8a203b6c79`，字节级 = 手术产物，岩库往返 sha 一致）
- 脚本：`tools/art/surgery_ballista.py`（五规合规：无头 / 幂等双跑 sha 同 / 参数化 / `--dry-run` / 退出码 2/3/4 分级）

## 部件认定（术前取证）

三视角渲染（3/4 + 侧 + 俯）+ 候选件染色隔离 + 视觉交叉验证：

| 部件 | 认定 | 证据 |
|---|---|---|
| 弦 `string` | 48 顶点边连通岛，贴轨细直杆，世界 bbox x[-0.204,+0.323] 高 0.334-0.353 横向居中 | 前缘比滑轨顶杆前伸 0.16（轨不悬空、被拉回的弦才会）；俯视/侧视一致判读 |
| 箭 `bolt` | **模型原生没有** | 三视角均无「居中、前端渐细、从弦前伸」杆件；不属手术范围，ART10 另行极小建模 |
| 其余 | 全部归 `body` | +Y 高轨两段 / -Y 低轨三段 / 中部框架拱两道 / 床板 / 端头配件 |

## v3 结构（复刻 v2 自身约定）

```
scene
├── body   rotation=Rx(90°)（v2 node_0 同款四元数，逐位一致），顶点=v2 访问器坐标
└── string rotation=Rx(90°) + translation=轴心(杆中心)，顶点以轴心居中
```

## 验收证据

- 结构断言：`tests/art_assets.rs::ballista_surgery_parts_v3`（先红后绿）——节点存在 + 联合 bbox = v2 整体 bbox（容差 2e-3）+ 弦位置钉死（5e-3）+ 索引 30000 守恒 + 无动画；旋转感知（角点变换，轴对齐旋转下紧确）
- 零漂移：转台 4 视角 v2/v3 像素差**总计 0**（阈值 8）
- 弦完整性：拓扑逐项对比 v2 同岛（48 顶点/67 面/27 开放边/3 处 x 间隙全同）——杆是 v2 原生开放壳，手术零损伤
- 全量回归：workspace 267 过 0 败

## 踩坑（详见链接的两张 pitfall 卡）

1. **位置焊接不可用**：766 岛经 4711 对**位级重合顶点**链成单一 blob——任何按位置的合并/选择都会吞整个模型。最终方案：边连通岛 + bbox 种子（[[topics/art/generated-mesh-coincident-vertex-blob|坑卡]]）。
2. **WORKBENCH 无头吃不到 shading**：材质色/世界色全被无视，输出灰度剪影。彩色取证渲染改 Cycles + 纯自发光（[[topics/art/blender-workbench-headless-color-pitfall|坑卡]]）。
3. **bbox 角点不能按角转换**：含负号轴映射时 min/max 互换，必须按轴区间映射（脚本首跑即被自检护栏抓住）。
4. **中间 empty 会被工具丢弃**：turntable.py 只留 MESH/ARMATURE，旋转挂在根 empty 上时模型躺倒（像素差 13% 的假漂移）。修正：旋转挂部件节点（已增补进分件规范第 7 条）。

## 修订建议

- 分件规范增补第 7 条（部件节点自带变换）——见 [[topics/graphics/model-part-animation-prep-spec|分件规范]]；
- 人工验收（流程⑤的「人工中性提问核对」）待用户看转台对比图后补；
- ART10 消费提示：弦节点 translation 即轴心，射击动画 = 沿世界 X 平移（后拉 +X / 前弹 -X，行程建议 ≤0.3 防穿臂）；箭需另行建模（四棱锥头 + 杆，~0.3 长）。
