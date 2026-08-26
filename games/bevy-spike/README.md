# bevy-spike — Bevy 0.19 全流程冒烟测试

> ADR-0004（渲染底料开源化）的证据收集项目。验证「模型导入 → 骨骼动画 → 相机/灯光 → UI → 构建」整条链路在 Bevy 下开箱即用，对照 m2-bevy 手写渲染器的同款模型（hero.glb = CesiumMan 人形）。

## 验证点

| 链路 | m2-bevy（手写 wgpu） | 本 spike（Bevy 0.19.1） |
|------|----------------------|--------------------------|
| glTF 人形导入 | 只读静态顶点，T 姿势裸模 | `GltfAssetLabel::Scene(0)` + `WorldAssetRoot` 自动生成 |
| 骨骼动画 | 无（未实现蒙皮） | `AnimationGraph::from_clip` + `AnimationPlayer.play(index).repeat()` |
| 多材质/贴图 | 只取首个材质，无贴图 fallback 棋盘格 | glTF 导入器原生支持 |
| 相机/灯光 | 手写 MVP + 深度纹理 | `Camera3d` + `DirectionalLight` + 阴影级联 |
| UI | 手写 22 个 buffer 的 UI 管线 | `bevy_ui` 文本节点 |
| 构建 | — | 标准 `cargo build`，桌面二进制 |

## 运行

```bash
cargo run --release      # 有显示器环境（Mac / 桌面 Linux）
```

无头验证（Linux VM）：

```bash
Xvfb :99 -screen 0 1280x720x24 &
DISPLAY=:99 WGPU_BACKEND=vulkan cargo run --release
# 期望日志：[spike] hero skeleton animation started ... / fps ≈ 60
```

## 操作

- `Space`：暂停 / 恢复英雄动画
- `Up` / `Down`：加速 / 减速

## 资产来源

- `hero.glb` / `monster.glb`：从 `games/m2-bevy/assets/` 复制（Khronos CesiumMan / BrainStem 系列，各含 1 个 57 目标骨骼动画）。
