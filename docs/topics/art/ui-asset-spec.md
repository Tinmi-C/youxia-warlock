---
title: UI 2D 资产规范（UI Asset Spec）
type: reference
topic: art
date: 2026-10-11
author: AI 起草（ui-polish 落地后的贴图化前置；首个消费者 = warden 面板贴图）
status: draft
tags: [art-pipeline, ui, 2d, nine-slice]
related: ["topics/art/art-pipeline-overview", "decisions/0009-art-vault-to-game-asset-flow", "games/warden/docs/design.md"]
---

# UI 2D 资产规范

## 结论

游戏内 UI 的 2D 资产（图标、九宫格面板、光标）与 3D 资产走**同一条门**：
岩库入库 → 清单锁 → art-sync 同步，体积小不豁免流程。规范的核心是
九宫格切分合同与「颜色值必须来自游戏 design.md tokens」——贴图是 tokens
的材质化，不是新配色来源。

## 正文

### 资产分类与命名

| 类 | 命名（snake_case） | 说明 |
|----|--------------------|------|
| 图标 | `ui_icon_<用途>.png`（如 `ui_icon_gold.png`） | 资源/塔/技能图标 |
| 九宫格面板 | `ui_panel_<材质>_<变体>.png`（如 `ui_panel_stone.png`、`ui_panel_parchment.png`） | 一材质一图，变体后缀区分（hover/dim） |
| 光标/杂件 | `ui_<用途>.png` | |

### 尺寸与格式

- **格式**：PNG（透明通道）；源文件（分层 PSD/blend/Aseprite）随资产入岩库一并保存
  ——回流修色时改源文件再导出，不直改 PNG。
- **图标**：源 128×128（方内留 8px 安全边），引擎内按目标尺寸用（32/48/64）；
- **九宫格面板**：总尺寸 = 角块 ×3 倍起步；**角块 ≥ 24px**（小角块在拉伸时
  细节会被放大糊掉）；中央区域要有可平铺纹理（noise/石纹），拉伸区避免
  横向渐变（单轴拉伸会变形）。
- 单文件 ≤ 2 MiB（UI 件远用不到，超了先查分辨率冗余）。

### 九宫格切分合同

```
┌─────┬──────────────┬─────┐
│ 固定 │ 垂直边：单轴拉伸 │ 固定 │   四角：绝对定位，永不变形
├─────┼──────────────┼─────┤
│水平边│  中心：平铺/拉伸  │水平边│   边：法线方向拉伸
├─────┼──────────────┼─────┤
│ 固定 │ 垂直边：单轴拉伸 │ 固定 │   切分线对称（上下边同高、左右边同宽）
└─────┴──────────────┴─────┘
```

- 切分值随资产元数据入岩库（边距四元组，px）；游戏侧 theme 常量与之对齐；
- 同一游戏的全部面板**共用一套切分比例**（如统一 24/4/24），跨材质不换比例
  ——九宫格混比例是 UI 对不齐的常见来源。

### 色彩合同

- 贴图色板 = 游戏 `design.md` tokens 的材质化（warden：白灰石/羊皮纸/要塞金）；
  **禁止在贴图里发明 tokens 之外的颜色**；调色 = 改 design.md → 改源文件 →
  重导出，全链路走一遍。
- 实色与贴图混用期（过渡态）：同一面板不允许一半实色一半贴图——
  一块面板一次切换（warden 现状：全部实色+描边，属规范的过渡态注明）。

### 入库与消费

- 入库：岩库 `save_asset`（category=ui），锁条目进 `art-assets.json`
  （ADR-0009 全文适用）；需求走 art-needs（ADR-0015），UI 需求同样上板。
- 消费：游戏侧 `theme.rs`（或同位文件）集中注册贴图句柄与九宫格边距常量，
  界面代码只从 theme 取——散落的路径字符串视同 magic number，禁。
- 断言（`tests/art_assets.rs`）：尺寸/命名/体积上限对账；九宫格边距常量
  与元数据一致；颜色抽样点落在 tokens 容差内（可选，首个消费者落地时定）。

### 验收

按 [[topics/art/visual-acceptance-methods|取证规范]]：结构断言（上）+
实机截图取证（UI 必须叠 gameplay 画面截，不单独截 UI 层——HUD 挡玩法
与否只能实况看）。warden 通道：`WARDEN_SHOTS` 自动截图。

## 参考

- [[topics/art/art-pipeline-overview|管线总纲]]（本规范是环节 2/6 的 UI 线）
- `games/warden/docs/design.md`（tokens 真源先例）
