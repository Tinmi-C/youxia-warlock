# 📚 Warlock 团队知识库

> 这是一个 **Obsidian vault**（同时也是独立 git 仓库）。用 Obsidian 打开 `docs/` 文件夹即可。
> 这里沉淀三人关于「Rust 自研 3D 引擎 + 游戏开发」的一切：学习笔记、踩坑、架构决策。

## 怎么打开

1. Obsidian → `Open folder as vault` → 选择 `docs/` 文件夹
2. 首次打开后，Obsidian 自动加载 `.obsidian/` 里的团队共享配置（插件、模板目录等）

## 组织方式（主题优先）

- 所有笔记按**主题**放在 `topics/` 下，**不要按文档类型分目录**。
- 类型用笔记 frontmatter 的 `type` 字段表达：`learning` / `pitfall` / `howto` / `reference`。
- 每个主题目录有一个 `_index.md` 作为该主题的地图。
- 想看「所有踩坑」→ 搜索 `type: pitfall`，不靠目录分类。
- 个人笔记（草稿/私人 TODO）放 `_private/`——git 已忽略，**不会推送团队仓库**，同事也 pull 不到。

## 入口

- 🗺️ [[MOC|索引地图 MOC]] — 从这里开始
- 📖 [[topics/_index|主题知识树]]
- 🧭 [[decisions/_index|架构决策 ADR]]
- 📗 [[glossary|术语表]]
- 🗃️ [[raw/README|原始资料 raw]]
- 📝 [[log|操作日志]]
- 🧾 [[AGENTS|维护规范 AGENTS.md]]

## 规则（人和 AI 都遵守）

- 完整维护规范见 [[AGENTS|AGENTS.md]]（摄入 / 查询 / 回写 / 健康检查四条工作流 + 硬性规则），以它为准。
- 新笔记：用 `meta/templates/` 里的模板（Obsidian Templates 插件，命令面板搜 "Templates: Insert template"）。
- 写完后：更新所在主题的 `_index.md`，顺手加到 [[MOC]] 和 [[log]]。
- 踩坑必记：现象 / 根因 / 解决 / 反思。
- 改完记得提交 git（提交信息用英文，Conventional Commits）。

## Git 说明

- `.obsidian/` 建议提交（团队共享配置，Obsidian 官方推荐），个人布局 `workspace.json` 不提交（见 `docs/.gitignore`）。
- 附件放 `meta/attachments/`，随仓库一起提交；量大再考虑 LFS。
