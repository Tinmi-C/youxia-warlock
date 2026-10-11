---
title: Blender WORKBENCH 无头渲染吃不到 shading 设置（灰度剪影）
type: pitfall
topic: art
date: 2026-10-11
author: AI（ART9 取证渲染踩中）
status: done
tags: [blender, rendering, workbench, cycles, art-pipeline]
related: ["topics/art/art9-ballista-surgery-record", "topics/art/visual-acceptance-methods"]
---

# Blender WORKBENCH 无头渲染吃不到 shading 设置

## 现象

`blender -b -P` 无头脚本里渲染部件染色取证图：材质 Base Color 写入成功（socket 回读正确）、面 material_index 正确、`scene.display.shading.color_type='MATERIAL'`、world 设白——输出却是一张**纯灰度剪影**（背景深灰、模型视口灰，0 个彩色像素）。

## 根因

`scene.display.shading` 是**视口（3D viewport）状态**；无头模式下没有视口，WORKBENCH 渲染回落到默认视口配置（STUDIO 灯光 + THEME 背景 + 材质色不生效）。对它的写入既不报错也不生效——静默失败比报错更危险。

## 解决

彩色平面图改走 **Cycles + 纯自发光材质**：每个部件一个 `ShaderNodeEmission`（颜色即部件色，强度 1），世界 emission 纯白，`view_transform='Standard'`（关 AgX/Filmic 把饱和色洗灰）。光照无关、无头稳定、输出就是纯色部件图（16 采样 CPU 秒级）。注意输出色会经 sRGB 映射，与设定的线性色有偏移——按「色调聚类」用即可，别按字节精确匹配。

## 反思

这是「修改前先验证前提」的反面教材一次过关：探针脚本先打了 shading 状态、socket 回读、面索引，才发现写入全对而**消费端**不认。管线里的静默配置项（写不进渲染路径的视口状态）要靠端到端像素检查（统计彩色像素数）一锤定音，不能只看 API 写入成功。
