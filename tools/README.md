# 团队工具

自研的辅助工具：资源打包、导出器、编辑器插件、构建脚本等。位于整体 monorepo 内的 `tools/` 目录，按需创建子目录，可依赖 `engine/` 或 `games/` 里的约定。

- 工具尽量用 Rust 写，与主技术栈一致。
- 通用脚本（非项目专属）可放这里共享。

## 现有工具

| 工具 | 说明 |
|------|------|
| `art/` | Blender 无头美术管线脚本（normalize.py 洗白 / turntable.py 图册 / mixamo_merge.py 动画嫁接），设计见知识库 art-pipeline 笔记 |
| `art-catalog/` | 只读美术资产目录系统：扫描两域（库/游戏）→ 检查 R1-R7 → 生成中文目录页 + catalog.json/report.json 双出口；AI 协作契约见其 README；设计稿 v0.4 见知识库 `docs/topics/game-design/art-asset-catalog-tool-proposal.md` |
| `art-sync/` | 岩库（ArtVault 资产中台）→ 游戏仓库的同步桥：读 `<game>/art-assets.json` 清单锁定版本，REST 拉取落 `assets/`，人/AI 同命令；决策见 ADR-0009（Python stdlib 零依赖，理由见该 ADR 备选节） |
