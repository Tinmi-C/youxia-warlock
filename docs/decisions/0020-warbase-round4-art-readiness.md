---
title: ADR-0020
type: adr
status: accepted
date: 2026-10-28
author: AI 起草（ZCode），youxia 批准方案 v4
---

# ADR-0020: warbase 第四轮——美术就绪 + 多人债收口

## 决策

1. **美术就绪从纸面变实战**：贴图入口（UiTextures 伴生资源 + ImageNode/Sliced 九宫格 + --art 双形态开关）落地并做**全链路演练**（程序化演练资产走 fulfilled→mapping→锁版本→sync→挂接全环，产出《首产日操作手册》）——美术解禁日即插即用且被彩排过。
2. **需求源头自动化**：art-needs 加 `scan --ui`（UI2D 声明清单→manual 需求卡，origin≠scan 免删通道）+ SfxKind 14 变体投影 + **外观相关性过滤**（纯数值 def 改动不再触发 changed）。
3. **状态机不加态（调研修正）**：岩库提案本有 producing/review 态且零校验——真缺口是岩库侧预审体验，以**提案文档**交付（WIP 预审通道+看板速度字段），非本仓代码。
4. **legion 七洞上收**：含 L2 多人经济分域（Wallets per-player + LocalPlayer 兼容视图，兑现 ADR-0017 暂缓债）、L5/L7/L8/L1/L3。
5. **warden 塔阵容重做立项**（P1 美术批次前置 + 弦箭动画挂回实战）。

## 备选

状态机直接加态（否——岩库侧已有态，避免跨仓越权）；美术先行内容后置（否——用户未解禁美术）。

## 影响

优：首产日风险从「未演练」降为「彩排过」；多人经济老债清偿；warden 内容+美术双解锁。代价：约 16-18 增量一轮。关联：ADR-0017/0018/0019、art-development-roadmap。
