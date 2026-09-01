# Warlock — 游戏开发工作区

团队（3 名程序员）游戏开发的大本营。目标：**用 Rust + Bevy 开发游戏，边开发边学习**，承载多个游戏项目、学习笔记与踩坑记录。

> ℹ️ 本根目录是**一个整体 git 仓库（monorepo）**（2026-08-25 起）。子目录不单独建仓；编译产物（target/）不入库，各机器本地生成。

## 目录结构

| 目录 | 说明 |
|------|------|
| `games/` | 游戏项目（m1-demo / m2-demo / m2-bevy 教学存档；bevy-spike；wave-survival 当前主线） |
| `docs/` | 团队知识库（Obsidian vault，主题优先）：笔记 / 踩坑 / ADR |
| `templates/` | 脚手架模板（`bevy-game/` 为新游戏默认模板） |
| `engine/` | 团队自研 crate 仓库（玩法沉淀用，先留可装配接缝、复用时机到再机械抽，见 docs 开发约定） |
| `tools/` | 团队自研工具（按需） |
| `assets/` | 跨项目共享素材（字体/音频/占位图等） |
| `AGENTS.md` | 全局 AI 协作约定（给所有 AI 智能体看的规则） |
| `.gitignore` | 全局忽略规则（target 等不入库） |

## 快速开始

1. 先读 `AGENTS.md` 和 `docs/README.md`。
2. 新游戏：从 `templates/bevy-game/` 复制到 `games/<game-name>/`，按 `docs/capability-cards.md` 工作流开发。

## 协作约定（三人）

- 整体 monorepo：一个 git 仓库管理全部（游戏 / 知识库 / 模板 / 工具）。
- 引擎底座：Bevy 0.19（团队默认，见 `docs/decisions/0004`）；自研 = AI 装配层（能力卡 + 验收闭环，见 `docs/decisions/0002`）。
- 学习笔记、踩坑、架构决策统一沉淀到 `docs/`。
- 提交信息用英文，遵循 Conventional Commits（`feat:` / `fix:` / `docs:` / `refactor:` …）。
