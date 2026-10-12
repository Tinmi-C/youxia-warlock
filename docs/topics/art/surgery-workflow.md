---
title: 分件手术流程（Model Part Surgery Workflow）
type: howto
topic: art
date: 2026-10-11
author: AI 起草（ART9 开工前置；首例实施后复核升 done）
status: done
tags: [art-pipeline, blender, surgery, gltf]
related: ["topics/graphics/model-part-animation-prep-spec", "decisions/0009-art-vault-to-game-asset-flow", "topics/art/visual-acceptance-methods", "topics/art/art-pipeline-overview"]
---

# 分件手术流程

## 结论

图生 3D 资产要变成「可程序化动画」的机械件（弩/炮塔/门），必须做一次分件手术：
按 [[topics/graphics/model-part-animation-prep-spec|分件规范]] 六条合同，用
**Blender 无头 Python 脚本**由 AI 主刀、人验收，成品回流岩库存新版本。
本流程是手术台的操作规程——第一次动刀（warden 弩 v3，ART9）之前立好；
首例已全程走通并经人工验收（2026-10-11，[[topics/art/art9-ballista-surgery-record|记录]]），升 done。

## 正文

### 触发条件

资产预期做**程序化动画**（引擎侧代码驱动件的运动）且当前不满足分件规范
（部件焊接、无命名、轴心不在运动轴）。角色/BOSS 等表现型走 K 帧动画线，**不做手术**。

### 前置合同（不满足不开刀）

1. 分件规范已读，本次手术的部件清单已列（每件：名称/运动类型/轴/行程）；
2. 源资产在岩库有锁定版本（手术输入 = 该版本的字节，操作可复现）；
3. 消费方（游戏侧）的结构断言测试位已留好（`tests/art_assets.rs` 先写红）。

### 脚本五规（AI 主刀的纪律）

手术脚本进 `tools/art/`（命名 `surgery_<目标>.py`），必须：

1. **无头**：`blender --background --python`，零 GUI 依赖，CI/任何机器可跑；
2. **幂等**：同一输入版本永远产出同一输出（不做随机/时间戳掺入）；
3. **参数化**：输入 GLB 路径、部件清单表驱动；不把几何选择逻辑写死在循环外；
4. **dry-run**：`--dry-run` 打印计划（将新建/改名/移动轴心的对象清单）不落盘；
5. **不吞错 + 退出码**：找不到预期几何/拓扑意外即报错退出，退出码 = 问题数
   （与 art-sync/art-needs/art-catalog 同约定）。

### 流程（八步）

```
① 列部件清单（名称/运动/轴/行程——就是未来结构断言的断言项）
② 写脚本 + dry-run 看计划
③ 执行手术 → 导出 GLB（归一：bbox/单位/朝向，延续 normalize.py 约定）
④ 结构断言：节点存在 + 命名 + 轴心偏差 < 0.05（tests/art_assets.rs 转绿）
⑤ 静止视觉验收：与 v2 并排转台录屏，静止状态应无观感差异
   （动的部分只多分件，不该多形体）；人工中性提问核对
⑥ 回流岩库：save_asset 存 v3，note 记手术内容与输入版本
⑦ 锁定升级：art-assets.json 版本号改 v3，重跑 art-sync 校验字节数
⑧ 操作记录进知识库（模板见下），踩坑按 pitfall 卡另立
```

### 操作记录模板（每例一篇，进本主题域）

```
title: 分件手术记录：<资产> v<in>→v<out>
输入版本 / 输出版本 / 脚本路径
部件清单（名称·运动·轴·行程）
踩坑（几何识别意外/轴心修正/导出注意）
验收证据（断言测试名 + 转台录屏/截图路径）
```

### 升格路径

第 1 例（ART9）用定向脚本攒证据；**第 2 个模型手术成功** → 脚本泛化
（通用参数化：部件清单完全表驱动）→ 升格为用户级 skill（跨项目、跨 AI
客户端复用）。第 1 例结束后回写复核分件规范条目（draft → done）。

## 参考

- [[topics/graphics/model-part-animation-prep-spec|分件规范]]（六条合同本体）
- [[topics/art/visual-acceptance-methods|取证规范]]（⑤步的证据标准）
- [[decisions/0009-art-vault-to-game-asset-flow|ADR-0009]]（⑥⑦步的通道）
