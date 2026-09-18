# art-sync — 岩库（ArtVault）→ 游戏仓库的资产同步桥

> 决策依据：团队 `docs/decisions/0009-art-vault-to-game-asset-flow.md`（ADR-0009）。
> 一句话：**外部资产走清单锁定 + 同步命令进仓库，人与 AI 同一扇门，不手拷。**

岩库（ArtVault，另一独立仓库的美术资产中台：FastAPI + SQLite + Web + REST + MCP）是跨项目美术资产的**单一事实源**：版本快照、参照图、技术元数据、状态（wip/usable/archived）都在那里。游戏仓库只消费「锁定版本」的副本，来源与版本记录在清单里。

## 快速开始

```bash
# 前置：岩库服务在本机跑着（默认 http://127.0.0.1:8765）
python3 tools/art-sync/art_sync.py games/warden            # 同步缺失/变更文件
python3 tools/art-sync/art_sync.py games/warden --dry-run  # 只看计划，不写盘
python3 tools/art-sync/art_sync.py games/warden --outdated # 顺带提示岩库里有没有新版本
```

退出码 = 问题数（0 = 干净），与 art-catalog 同约定。

## 清单格式（`<game>/art-assets.json`）

```jsonc
{
  "schema_version": 1,
  "source": "http://127.0.0.1:8765",
  "assets": [
    {
      "id": "4c2de1d1dc414236a83760418992da30",  // 岩库资产 ID
      "name": "箭塔 tower_arrow",                  // 人类可读；改名时工具会提示
      "version": 1,                               // 锁定的版本号（核心）
      "license": "自有资产（Rodin 图生 3D）",        // 来源/许可留痕
      "files": { "tower_arrow.glb": "models/tower_arrow.glb" }  // 库内文件名 → assets/ 下相对路径
    }
  ]
}
```

- 目标路径只允许 `assets/{models,textures,audio,fonts,ui}/` 下**单层文件名**（snake_case，遵守项目资产约定）。
- 清单进 git；**资产文件与清单一并提交**，diff 里看得见「谁、哪个版本、进来了什么」。

## 标准工作流

**首次引入**（人说「导入 X」或 AI 发现库里有现成的）：
1. 岩库确认资产 `status=usable`（wip 也可导入，自担风险——工具不拦，岩库哲学零校验）。
2. 在清单加条目（锁当时的 `version`），跑 `art_sync.py`。
3. 提交：资产文件 + `art-assets.json` 一起。

**升级**：岩库出了新版本 → `--outdated` 看提示 → 改清单 `version` → 重跑同步 → 一并提交。

**回流**（重要，别只建单向管道）：游戏侧对 GLB 的再加工（减面、重贴图、修 UV）——完成后把成品 `save_asset` 回岩库存为新版本（附 note），下个项目才有得用；引擎内的补件（如旗帜的顶点动画）属于玩法代码，留在游戏仓库。AI 客户端可注册岩库的 MCP（见下）直接做回流。

## AI 使用约定

- 正式进库**只走**清单 + `art_sync.py`；临时调研（搜资产、看元数据、取参照图再生成）可用岩库 MCP。
- 岩库 MCP 注册（stdio，任何支持 MCP 的客户端）：
  - command: `<art-asset-manager 仓库>/.venv/bin/python`
  - args: `["-m", "app.mcp.server"]`
  - cwd: `<art-asset-manager 仓库>/server`
  - 工具面：`search_assets` / `get_asset_details` / `download_asset` / `save_asset` / `update_asset` / `get_reference_images`
- 典型回路：`get_reference_images` 取参照图 → 重新生成 → `save_asset` 存新版本 → 人看 Web 预览拍板 → 游戏侧改清单升级。

## 安全边界（工具内置，无需配置）

- `source` 仅允许环回/私网/链路本地地址（岩库是内网服务，公网域名拒绝）；禁跟随重定向。
- 下载为 `.part` 临时文件 + 长度校验 + 原子改名；目前以**字节数**校验（岩库暂无 sha256 字段，后续补）。

## 与 art-catalog / `_library/` 的关系

现在同步直接落游戏 `assets/`（R6 会像所有既有模型一样提示缺 meta，属仓库当前基线）。**待团队 `_library/` 物理迁移落地后**，新资产的落点应改为 `_library/raw/` + intake 工单（走 SC1 洗白：normalize 定高、license 留痕、turntable 图册），art-sync 届时增加落点选项——本工具不抢 art-catalog 的入库流水线职责，只负责「岩库 ↔ 仓库」这一段。
