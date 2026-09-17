---
title: ADR-0009
type: adr
status: accepted
date: 2026-09-14
author: AI + youxia
---

# ADR-0009: 美术资产从岩库（ArtVault）进入游戏仓库的方式——清单锁定 + art-sync 桥

## 背景

团队在游戏仓库之外维护「岩库 ArtVault」——一个跨项目美术资产中台（独立仓库：FastAPI + SQLite + Vue3/Three.js Web 预览 + REST + MCP，AI 优先设计）。AI 生成/外部获取的资产先入岩库（版本快照、参照图、技术元数据、wip/usable/archived 状态），需要时分发到各游戏项目。

warden 需要引入第一个此类资产：箭塔 `tower_arrow.glb`（Rodin 图生 3D，约 2.4 MiB / 23332 tris，已知缺陷：旗帜未生成，计划在 Bevy 内用面片+顶点动画补）。此前没有既定通路，需要定「岩库 ↔ 游戏仓库」之间资产怎么流动，且两侧开发都以 AI 为主、人做拍板。

约束：

- Bevy 资产管线只认本地文件，运行时不能依赖外部服务在线。
- monorepo 无 Git LFS，现状 GLB 直接提交（最大单文件 15 MiB）。
- art-catalog 的 `_library/` 物理迁移**尚未落地**（待队友美术 WIP 落库，见其 README 待办），入库流水线（intake 工单 + SC1 洗白）暂时无处安放。
- 3 人 + 多个 AI 客户端（Cursor / Claude Code / DeepSeek Harness…）都会碰资产文件。

## 决策

**采用「清单锁定 + 同步命令」桥：每游戏一份 `art-assets.json` 锁文件 + `tools/art-sync/art_sync.py` 同步工具，人与 AI 走同一扇门。**

1. **锁文件**：`<game>/art-assets.json` 记录每个资产的（岩库 id、锁定 version、文件名→`assets/` 子目录映射、license 留痕）。进 git——它是「这个游戏用了哪些资产的哪些版本」的唯一事实，diff 可审计。
2. **同步工具**：`tools/art-sync/art_sync.py`，Python 3 标准库零依赖，打岩库 REST API（`GET /assets/{id}` 解析版本 → `GET /files/{id}/download` 拉取），字节数校验 + `.part` 临时文件原子落盘；支持 `--dry-run`（只看计划）与 `--outdated`（对比岩库最新/稳定版，给升级提示）。退出码 = 问题数，与 art-catalog 同约定。
3. **GLB 直提交 git**（延续现状）：锁文件保证来源可溯，暂不引入 LFS（见备选 4）。
4. **安全边界内建**：`source` 仅允许环回/私网/链路本地地址（岩库是内网服务，公网域名拒绝）、禁跟随重定向、目标路径限定 `assets/{models,textures,audio,fonts,ui}/` 单层 snake_case 文件名。
5. **AI 双通道**：正式进库只走锁文件 + 同步命令（AGENTS.md 已立规则，AI 会遵守）；调研性动作（搜资产、看元数据、取参照图再生成、回流成品）走岩库 MCP（`search_assets` / `get_asset_details` / `save_asset` 等，stdio 注册进各 AI 客户端）。人永远有 Web 预览与手动下载兜底。
6. **回流闭环**：游戏侧对 GLB 的再加工（减面/重贴图/修 UV）完成后 `save_asset` 回岩库存新版本；引擎内补件（旗帜动画等）属玩法代码，留在游戏仓库不回流。

## 备选方案

1. **手拷 / AI 按需 `download_asset`**：零开发成本，但无版本可溯——塔升到 v2 后游戏侧无从知道手里是 v1，多人多 AI 环境漂移不可控。否。
2. **符号链接岩库 storage/ 目录**：把中台内部布局（`assets/{uuid}/v{n}/`）耦合进游戏仓库，git 不可跟踪，数据一挪全断。否。
3. **Bevy 运行时远端直载**：运行时依赖岩库在线 + 与 Bevy 资产管线不符。否。
4. **gitignore 模型文件 + clone 后 sync**：仓库最干净，但 clone 多一步引导且依赖岩库在线，队友体验差；留作将来选项。
5. **现在就上 Git LFS**：大文件正解，但要迁移已有历史 + 三人配环境，学习期团队中途切换成本高。设迁移门槛（见下）。
6. **工具用 Rust 写**（tools/ 目录偏好）：纯 std 无 HTTP 客户端，自写成本高于收益；Python stdlib 零依赖已满足「任何机器离线可跑」的精神，且与 `tools/art/*.py` 先例一致。将来嫌慢再重写。

## 影响 / 后果

- 优：可复现（clone + 一次 sync 即还原）、可审计（锁文件 diff）、人/AI 同门（一条命令）、岩库成为跨项目单一事实源，改进可回流反哺下个项目。
- 代价：升级需显式改锁文件（这是特性不是缺陷）；同步需要本机起着岩库服务（默认 `http://127.0.0.1:8765`）。
- 已知债务：仅字节数校验（岩库暂无 sha256 字段，补上后工具升级为哈希校验）；`--outdated` 依赖人/AI 主动跑，无自动提醒。

**迁移门槛（出现任一即重新评估）**：

1. `_library/` 物理迁移落地 → art-sync 落点改为 `_library/raw/` + intake 工单，接 SC1 洗白（normalize 定高、license 留痕、turntable 图册），届时 R6 缺 meta 的基线警告随之消除；
2. 仓库总量显著膨胀（如 >500 MiB）或单文件 routinely 超 8 MiB（R7 阈值）→ 上 Git LFS；
3. 出现第二个消费者项目或多人并发改资产 → 考虑 CI 里跑 `art_sync.py --dry-run` 做漂移门禁；
4. 岩库补齐 sha256 → 工具切换哈希校验。
