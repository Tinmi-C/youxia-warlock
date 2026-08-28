---
title: AI 生成 3D 资产工具调研——混元 Hunyuan3D 与云服务对比
type: reference
topic: game-design/art
date: 2026-08-28
author: AI（youxia 确认沉淀）
status: draft
tags: [ai-generation, hunyuan3d, meshy, tripo, art-pipeline, tooling]
related: [topics/game-design/ai-asset-pipeline, topics/game-design/art-pipeline-3d-v2, "games/wave-survival/docs/style-bible"]
---

# AI 生成 3D 资产工具调研

## 结论（对本团队）

**混元 Hunyuan3D 开源版（本地）+ 洗白站补色 = 本项目 AI 路线的定版基座。** 理由：本机 RTX 4060 Ti（8GB）够跑 mini/turbo 变体；抽卡不限次使"批量生成 N 选 1"的风格统一策略成立；零订阅成本；而它最大的短板（纹理质量差）被本项目管线设计（形状-only + 洗白站 palette lock 平涂上色）完全抵消。云端（Meshy/Tripo/腾讯网页版）保留作 A/B 对照与旗舰质量基准。

## 版本现状（2026-08）

| 版本 | 形态 | 要点 |
|------|------|------|
| Hunyuan3D-2.0（2025.01） | 开源权重 | 形状生成（DiT）+ 纹理合成（Paint）两段式；mini/turbo/多视角变体 |
| Hunyuan3D-2.1 | 开源权重 | PBR 纹理、质量提升；Windows 整合包已支持 |
| 2.5 / Studio | 仅云端 | 质量最强、不开源——开源版永远差半代 |

## 硬件门槛（量级参考，部署时以 WinPortable README 为准【待验证】）

| 变体 | 显存 | 本机 RTX 4060 Ti 8GB |
|------|------|----------------------|
| mini / mini-turbo / turbo | ~6-8GB | ✅ 首选，秒级到几十秒/件 |
| 2.0/2.1 标准版 | 12GB+ | ⚠️ 勉强，不推荐 |

Windows 部署：[YanWenKun/Hunyuan3D-2-WinPortable](https://github.com/YanWenKun/Hunyuan3D-2-WinPortable) 整合包，解压跑 .bat 起本地 Gradio UI，代价是几十 GB 下载。

## 质量口碑（去宣传水分）

- **网格强**：形状生成开源第一梯队，剪影/体积感可用（多来源独立评测一致）
- **纹理弱**：官方仓库 issue 直接吐槽纹理糊（如 [Issue #42](https://github.com/Tencent-Hunyuan/Hunyuan3D-2/issues/42)）——对本项目无所谓，见下方策略
- 人脸与手仍是全行业弱项，生成人形角色时按"高报废率"预期管理

## 本项目的关键策略：形状-only + 洗白站补色

> 混元只出**形状** → 贴图不用它的 → 洗白站 palette lock 用 style-bible 十色板平涂上色 → 面数超预算走 normalize `--max-tris` 衰减。

纹理短板归零；颜色强制归队色板（风格统一靠管线不靠抽卡运气）；云端旗舰的纹理优势（Meshy/Tripo 强项）恰好是本项目不需要的维度。

## 三条获取路径对比

| | 混元本地（开源） | 腾讯网页版 | Meshy / Tripo 云 |
|---|---|---|---|
| 费用 | 0（电费） | 免费额度+会员 | 订阅 $16+/月 |
| 抽卡次数 | **无限** | 每日限额 | 积分制 |
| 自动化 | **最强**（Gradio/ComfyUI 可脚本化） | 无 | API（另计费） |
| 质量 | 开源版（差半代） | 旗舰级 | 旗舰级 |
| 数据私密 | 完全本地 | 腾讯云 | 对方云 |
| 前提 | ≥8GB N 卡 | 无 | 无 |

**许可证注意**：开源 ≠ 无条款。2.0/2.1 均为腾讯社区许可证，带地区等限制条款（对国内团队基本无感）；**商用前人工通读仓库 LICENSE 文件**——本文不给替代结论【待验证：逐条人工核对】。

## 实测锚点（本项目，2026-08-28）

腾讯混元网页版生成玩家模型（免费额度）：洗白后身高精确 1.2m、50,000 → 9,999 tris（`--max-tris 10000`）、30.9MB → 3.32MB（`--tex-size 1024`）、单材质、静态无骨骼。人工两轮复检通过——AI 路线首个完整闭环样本。数据详情见 [[topics/game-design/ai-asset-pipeline|AI 生成管线流程]]。
