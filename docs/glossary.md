---
title: 术语表
type: reference
---

# 术语表

> 团队共同术语，避免「你说的和我想的不是一个东西」。随用随加。

| 术语 | 解释 | 出处 / 链接 |
|------|------|-------------|
| ECS | Entity Component System，实体组件系统 | [[topics/engine/_index]] |
| MVP | Model-View-Projection，模型-视图-投影矩阵 | |
| wgpu | Rust 的 WebGPU 实现，跨平台图形 API | |
| WGSL | WebGPU Shading Language，wgpu 的着色器语言 | |
| MOC | Map of Content，知识索引地图 | [[MOC]] |
| 世代号 | bevy_ecs Entity 的防悬空机制：编号复用时世代 +1，旧引用对比世代即知已失效（如 `11v0` = 编号 11 第 0 代） | [[topics/engine/bevy-ecs-world-reset-pitfall]] |
| 命令缓冲 | ECS 版渲染队列：系统循环内只向 Commands 记账（spawn/despawn），ApplyDeferred 结算点统一落地——避免边迭代边改 | [[decisions/0003-toy-ecs-to-bevy-ecs]] |
| Schedule | bevy_ecs 的系统执行计划：系统按依赖/链式排序运行，且与首次绑定的 World 终身绑定（换 World = panic） | [[topics/engine/bevy-ecs-world-reset-pitfall]] |

## 使用约定

- 术语第一次出现时，链接到这里。
- 遇到分歧先查这里，查不到就加一条并标注日期。
