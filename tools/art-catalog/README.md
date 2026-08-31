# art-catalog — 只读美术资产目录系统

> 能力卡 AC-1 ｜ 设计稿：知识库 `docs/topics/game-design/art-asset-catalog-tool-proposal.md`（v0.4）
> 定位：**人操作，AI 辅助，工具是引擎**。工具只读扫描仓库、生成报告；除输出目录的三个产物外不写任何文件。

## 快速开始

```bash
cargo run --release -p art-catalog --manifest-path tools/art-catalog/Cargo.toml \
  -- --game games/wave-survival          # 在 monorepo 根执行
# 产物：games/wave-survival/_art/catalog/{index.html, catalog.json, report.json}
# 浏览器打开 index.html；退出码 = 检查问题数（0 = 全绿）
```

- 零依赖（纯 std，自带最小 JSON），任何机器离线可构建。
- 库布局自动探测：优先工作区级 `_library/`（迁移后），不存在则回落 `<game>/_art/`（迁移前）。

## CLI 契约

```
art-catalog [--root <repo>] [--game <dir>] [--library <dir>] [--out <dir>]
            [--scan-only] [--scenario-check]
```

| 参数 | 默认 | 说明 |
|------|------|------|
| `--root` | 当前目录向上找含 `games/` 的目录 | monorepo 根 |
| `--game` | 自动发现（多游戏时必须指定） | 游戏目录，如 `games/wave-survival` |
| `--library` | `_library`（不存在则回落 `<game>/_art`） | 工作区库目录 |
| `--out` | `<game>/_art/catalog` | 输出目录 |
| `--scan-only` | 关 | 仅扫描输出，跳过页面生成（预留） |
| `--scenario-check` | 关 | 校验场景卡（dry-run），退出码 = 失败卡数 |

```
art-catalog intake [--root <repo>] --game <dir> create|set|list …
```

上架流水线状态机子命令族（卡 AC-3，规则实现见 `src/intake.rs`）：

| 子命令 | 说明 | 退出码 |
|--------|------|--------|
| `create --file <raw> [--name <snake>] --license <许可> [--source 来源] [--scenario SC1]` | 立案：生成 `_art/intake/<日期>-<名>.json`（status=new）；目标名必须 snake_case、许可必填；目标同名冲突写入工单 notes | 参数不合法=2 |
| `set <工单id> --status <washing\|review\|landed\|rejected> [--note 备注]` | 状态翻转；非法翻转（跳步、终态改写）拒绝 | 非法翻转=1 |
| `list` | 列出全部工单（id/status/target） | 0 |
| `wash --file <raw> --height <米> --license <许可> [--name] [--yes] [--skip-anim] [--blender 路径]` | **一键洗白（AC-4/L2）**：立案→normalize→turntable→条带→review→复扫一条龙，停下等人翻图册拍板；失败自动 rejected 留痕 | 管线失败=1 |

Blender 路径解析：`--blender` > 环境变量 `BLENDER_EXE` > 团队默认 `D:\Blender\blender.exe`。

合法翻转：`new→washing→review→landed`（review 可回 washing 重洗；非终态均可 → rejected 需备注；landed/rejected 终态）。

## 产物 schema（schema_version = 1）

### catalog.json（资产事实库——AI 查询现状的入口）

```jsonc
{
  "schema_version": 1,
  "game": "games/wave-survival",
  "library": "_library | <game>/_art | null",
  "legacy_layout": false,
  "stats": { "library_candidates", "library_washed", "game_models", "referenced",
             "orphans", "stale", "findings", "intake_open" },
  "assets": [{
    "id": "green_blob", "kind": "model|texture|audio|font|ui|other",
    "domain": "library | game",
    "path": "games/.../assets/models/green_blob.glb",
    "size": 90796, "modified": 1787845521,
    "meta": { "height_m", "triangles", "materials", "clips", "has_armature",
              "renders": ["…png"], "meta_path",
              "anim": [{ "name", "strip", "frames" }] } | null,
    "refs": [{ "file": "…/components.rs", "line": 87, "snippet": "…" }],
    "stale_reasons": []
  }],
  "findings": [ /* 同 report.json */ ],
  "intake":   [ /* _library/intake/*.json 原文 + file 字段 */ ],
  "scenarios":[ { "id", "name", "trigger", "steps", "human_steps", "file" } ]
}
```

### report.json（检查结果——AI 出修复清单的入口）

```jsonc
{
  "schema_version": 1, "total": 28,
  "findings": [{ "rule_id": "R1", "severity": "warning|error|info",
                 "subject": "<路径/目录>", "evidence": "<事实依据>",
                 "fix_hint": "<建议修复>" }]
}
```

规则：**R1** 孤儿（游戏域资产在 `src/`+`tests/` 零字面量引用；`//` 注释中的提及不算引用）｜ **R2** stale（meta/渲染图/同名 raw 比 glb 旧，mtime）｜ **R3** 运行时模型命名非 snake_case ｜ **R4** clip 名不在约定集（超集报告，不阻断）｜ **R5** 库候选图册从未上架 ｜ **R6** 运行时模型缺 meta.json ｜ **R7** 大文件（>8 MiB）。

### 场景卡（AI 辅助执行的操作手册）

`scenarios/*.json`（内置）+ `<game>/_art/scenarios/*.json`（项目覆盖，按 id 覆盖内置）。字段：`id/name/trigger/source/steps[]/approval_points/acceptance`；每步 `{n, executor: human|ai-assist|auto, do, approval?, check_paths?, on_fail?}`。校验：`art-catalog --scenario-check`。

## AI 使用约定（谁在什么时机用什么）

1. **改资产相关代码/立卡前**：读 `catalog.json` 拿事实（有哪些资产、被谁引用、clips/身高），不凭记忆猜。
2. **体检/修复**：跑 CLI（或让人跑），读 `report.json` 逐条给修复方案；**破坏性动作（删除/覆盖/改引用）必须人批准后执行**，执行后复扫验证退出码归回基线。
3. **入库请求**：会话里人说「导入 X」→ AI 落单写 `_library/intake/<date>-<slug>.json`（`id/date/requester/source/license/target/scenario/status:"new"`）→ 按场景卡辅助执行 → 状态翻 `landed/rejected`。
4. **场景执行**：读卡逐步走；`executor=human` 与带 `approval` 的步骤必须停下等人；`check_paths` 缺失即报告，不带病执行。

## 只读红线

本工具对仓库只写三个产物文件（输出目录内）。资产变更一律走既有管线工具（normalize/turntable/mixamo_merge，均需人批准的指令）或 `git mv`/git 操作。

## 动画预览（卡 AC-2）

**3D 播放器（Mixamo 式）**：three.js r147（UMD）+ GLTFLoader + OrbitControls 内嵌进 index.html（`assets/` 目录为构建输入，`include_str!` 编进二进制）；每个 glb 另写 base64 分包 `catalog/modeldata/<id>.js`（≤6MiB，超限跳过）。点开模型详情懒加载对应分包 → 真 3D 视口（旋转/缩放/平移、clip 切换、播放暂停、变速、时间轴 scrub）。分包是静态 JS、`<script src>` 按需加载，file:// 直接可用、无需服务。

**动图条（轻量速览）**：

```bash
D:\Blender\blender.exe -b -P tools/art/anim_strip.py -- \
  --model games/wave-survival/assets/models/<name>.glb \
  --meta-dir games/wave-survival/_art/gallery-washed/<name>
```

- 每条 clip 离线渲 10 帧横向 sprite 条 → `<meta-dir>/anim/<clip>.png` + `anim_index.json`
- `art-catalog` 复扫后把条目并进 `meta.anim`；「动画速览」页全部 clip 平铺 8fps 循环
- 有动画的模型卡片封面直接播放首条 clip，画廊可勾「只看有动画」

## 页面导入按钮

总览页「⬆ 导入资产」：选文件 → File System Access API 写入 raw 目录（Edge/Chrome；其余浏览器提示手动复制）→ 自动生成该文件的 SC1 提示语一键复制给 AI。页面只搬运文件，不执行洗白——审批流与自动化分级见设计稿 v0.5「导入自动化规划」。

## 状态

- v1（卡 AC-1）：扫描/检查/HTML 页面已实现，真仓验收通过。
- AC-2：动画条带预览 + 3D 播放器（持久视口）+ 页面导入按钮 + glb 文件真相解析。
- AC-3：上架流水线状态机（`intake` 子命令族）+ 资产表管线状态列 + 图册上架/候选徽章，全链验收通过。
- AC-4：一键洗白 `wash` 命令（L2，一条命令到 review 停下等拍板）+ 页面导入提示语双轨，全链验收通过。
- 待办：`_library/` 物理迁移（git mv，待队友美术 WIP 落库）。
- FAQ「为什么页面没有导入按钮？」：洗白要跑 Blender 且有人工验收点（参数批准 + 图册拍板），页面只负责搬运进 raw、看结果与查事实；上架全流程见 SC1 + `intake` 状态机。
