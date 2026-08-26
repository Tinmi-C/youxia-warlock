# 自研引擎

Rust 自研 3D 引擎。**独立 git 仓库**，clone 到本目录下：

```
engine/<engine-name>/    # 例如 engine/warlock-engine/
```

## 组织建议

- 用 Cargo workspace，按职责拆 crate：
  - `engine-core` — 生命周期、事件、ECS
  - `engine-render` — 渲染（wgpu）
  - `engine-math` — 数学（glam 封装或直接依赖）
  - `engine-assets` / `engine-audio` / `engine-input` …
- 引擎本身不绑定具体游戏，通过 trait 抽象 + 内置 demo 验证。

## 起步路线（3D 学习）

1. 跑通窗口 + 清屏（`winit` + `wgpu`）
2. 画一个三角形（理解渲染管线、WGSL）
3. 纹理采样
4. 深度测试 + 相机 + 变换（MVP 矩阵，`glam`）
5. 模型加载（glTF）与基础光照
6. 场景图 / ECS
7. 逐步加音频、物理、编辑器

每一步的关键结论与踩坑都写回 `docs/`（`learning/` + `pitfalls/`）。

## 游戏如何依赖引擎

- 开发期：游戏里用 `path = "../engine/<engine-name>/engine-core"` 快速迭代。
- 稳定期：引擎打 tag，游戏里改用 git 依赖锁定版本。
