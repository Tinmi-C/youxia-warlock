# AGENTS.md — 全局协作约定（AI 与人都要遵守）

> 本文件是给「在这个目录树里工作的任何 AI 智能体」的全局上下文和规则。
> 无论用 DeepSeek Harness、Cursor、Claude Code 还是 Copilot，都应以本文件为准。

## 这个工作区是什么

- 一个基于 **Bevy 引擎**的游戏开发工作区 + 多个游戏项目 + 团队学习知识库的根目录。
- 团队：3 名程序员，均无游戏开发经验，**边开发边学习**。
- 语言约定：日常沟通与文档用中文；代码、标识符、commit message 用英文。

## 目录约定（重要）

- 根目录 `/home/youxia/warlock` 是**一个整体 git 仓库（monorepo）**（2026-08-25 起）。子目录不再各自独立建仓；旧版 polyrepo 说明已废弃。
- 编译产物（`target/` 等）一律不入库（见根 `.gitignore`），各机器本地生成；跨机器开发用 git push/pull 同步。
- 顶层结构见根目录 `README.md`；AI 动手修改前先读它。

## 技术栈（Rust）

- 语言：Rust（stable）。
- 引擎底座：**Bevy 0.19.1**（团队默认，见 `docs/decisions/0004`）。
- 客观技术优先引插件（bevy_rapier3d / bevy_hanabi / bevy_egui 等），主观玩法自研并沉淀（见 `docs/topics/engine/bevy-plugin-and-code-reuse.md`）。
- 不手写渲染/ECS（ADR-0004 已退役该路线）；wgpu 手写时代代码仅作教学存档。
- 选型理由写进 `docs/decisions/`，不要默默做决定。

## 架构约定

- 代码组织：lib + bin 分离、领域插件化（`src/plugins/`）、组件=数据 / 系统=逻辑（以模板 `templates/bevy-game/` 为准）。
- 新机制 = 新组件 + 新系统，老系统零改动。
- 开发流程：能力卡驱动（立卡 → 实现 → 验收 → 回归 → 一卡一提交），见各项目 `docs/capability-cards.md`。

## 知识库约定（docs/ 是一个 Obsidian vault）

- `docs/` 是 Obsidian vault（在整体 monorepo 内，不再独立建仓），用 Obsidian 打开 `docs/` 使用。
- **三层结构**：`docs/raw/`（原始资料，只读不可变）→ `docs/topics/`（编译后的结构化知识）→ `docs/AGENTS.md`（维护规范 schema 层）。
- **主题优先**：知识笔记放 `docs/topics/<领域>/`，类型用 frontmatter 的 `type` 字段表达（learning / pitfall / howto / reference），**不要按类型分目录**。
- 新笔记用 `meta/templates/` 里的模板；踩坑必记：现象 / 根因 / 解决 / 反思。
- 架构决策放 `docs/decisions/`（ADR），编号递增，被取代时更新状态。
- 每次写操作后同步更新：所在主题 `_index.md`、`docs/MOC.md`、`docs/log.md`。
- **详细工作流（摄入 / 查询 / 回写 / 健康检查）与硬性规则见 `docs/AGENTS.md`**，那是知识库的维护规范，以它为准。

## AI 行为规则

- 先读根 `README.md`、本文件、以及目标项目自己的 `AGENTS.md`，再动手。
- 根目录已是 git 仓库（monorepo）；新游戏项目从 `templates/bevy-game/` 复制到 `games/`。
- 改动涉及架构或技术选型时，先写一条 ADR 到 `docs/decisions/` 并说明理由。
- 回复语言跟随用户；代码注释用英文，除非用户要求中文。
- 不确定的技术决策，明确说「不确定」并给出依据和备选方案，不要编造 API 或版本号。
