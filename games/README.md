# 游戏项目

每个游戏一个独立子目录，位于整体 monorepo 内（不单独建仓）：

```
games/<game-name>/
```

## 新建游戏

1. 从 `templates/bevy-game/`（Bevy 0.19 默认模板）复制到 `games/<game-name>/`。
2. 改 `Cargo.toml` 的 `package.name` / `description`。
3. 按模板里的 `AGENTS.md` 补充项目专属上下文（玩法、目标平台、当前状态），并更新 README。
4. 按 `docs/capability-cards.md` 能力卡工作流开发：立卡 → 实现 → 验收 → 回归 → **一卡一提交**（Conventional Commits，英文）。

> ⚠️ 本目录整体是一个 git 仓库（monorepo，2026-08-25 起），子目录不再各自 `git init` / 建远程仓库；
> 编译产物（target/）不入库、各机器本地生成，跨机器同步用 git push/pull。

## 命名

- 目录名英文 kebab-case，如 `games/first-shooter/`。

## 项目内结构（模板已含）

- `src/` — Rust 代码（lib + bin 分离、领域插件化）
- `assets/` — 游戏素材（本项目专属）
- `docs/` — 项目内文档（GDD、能力卡）
- `tests/` — 行为一致性回归测试（验收闭环的可执行化）
- `AGENTS.md` — 项目 AI 上下文
- `Cargo.toml` — 依赖引擎（Bevy 0.19，见 ADR-0004）

## 现有项目

| 项目 | 说明 |
|------|------|
| `wave-survival/` | 当前主线，成品向开发中 |
| `bevy-spike/` | Bevy 0.19 冒烟验证（ADR-0004 证据） |
| `m2-bevy/` | bevy_ecs 迁移教学存档 |
| `m2-demo/` | 玩具 ECS 教学存档（已退役路线） |
| `m1-demo/` | wgpu 手写渲染教学存档（已退役路线） |
