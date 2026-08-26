# 游戏项目

每个游戏一个独立子目录 + 独立 git 仓库：

```
games/<game-name>/
```

## 新建游戏

1. 复制 `templates/game-project/` 到 `games/<game-name>/`。
2. `git init` 并建远程仓库。
3. 按模板里的 `AGENTS.md` 补充项目专属上下文（玩法、目标平台、当前状态）。

## 命名

- 目录名英文 kebab-case，如 `games/first-shooter/`。

## 项目内结构（模板已含）

- `src/` — Rust 代码
- `assets/` — 游戏素材（本项目专属）
- `docs/` — 项目内文档
- `AGENTS.md` — 项目 AI 上下文
- `Cargo.toml` — 依赖引擎
