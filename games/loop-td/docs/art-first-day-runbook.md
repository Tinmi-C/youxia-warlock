# loop-td 美术首产日操作手册（art first-day runbook）

> 适用场景：**美术解禁日**——岩库（ArtVault）通知「ui-panel-command-card 等
> 需求已 fulfilled」，游戏侧把第一批真实贴图接进 HUD 的**第一天**该怎么走。
> 本手册由 round-4 DRILL 增量的全环彩排实证产出（彩排实录见附录 A，
> 彩排中发现的流程缺陷见附录 B——手册按「现状流程」写，缺陷不改流程本体）。
>
> 前置阅读：`docs/topics/art/game-art-cross-project-workflow.md`（五阶段权威
> 流程，本手册覆盖其中 C 验收同步 + D 升级回环的游戏侧动作）、
> ADR-0009（资产流）/ ADR-0015（需求流）。
> 命令一律从**仓库根目录**执行。

## 0. 全景：首产日要走的链路

```
岩库 fulfilled 通知
  → ① art-needs status      拉状态，确认 fulfilled + 资产 id（可进 mapping/lockfile 的提示）
  → ② art-mapping.json      填换装映射（UI 2D 键位：ui-<item id> → 岩库:<asset_id>[@vN]）
  → ③ art-assets.json       起草锁清单条目（锁版本 + demand 回链 + 落点路径）
  → ④ art_sync.py           拉文件（字节校验）+ 与清单一并提交
  → ⑤ uitextures.ron        UiTextureManifest 挂接 → --art 启动看效果
  → ⑥ art-needs sync        锁定上报岩库（三层追溯链闭合）
```

演练拓扑（彩排用，正式日不需要）：`games/loop-td/assets-art-drill/` 下的
mock 岩库 + 伪游戏根 `first-day/`，演练了 ①-⑥ 全部环节及 D 升级回环。

## 1. Step 0 —— 确认 fulfilled（收到通知后第一件事）

```bash
python3 tools/art-needs/art_needs.py games/loop-td status
```

看到这样的行就说明该需求可以进入接线：

```
  ✓ ui-panel-command-card: fulfilled 资产 <asset_id>（可进 art-mapping.json / art-assets.json）
```

同时注意两个坏信号（退出码非 0）：
- `? 岩库有 <id> 但本地清单没有` —— 本地 `docs/art-needs.json` 缺投影，先补。
- `! fulfilled 但无关联资产` —— 岩库侧状态虚标（零校验），回美术补关联。

> 岩库不可达时：`status` 报「岩库不可达」退出 1，本地投影不更新——**不
> 影响构建**（管理流与构建流解耦是红线）。可以先用岩库 Web 看板肉眼确认，
> 服务恢复后再跑 status。

## 2. Step 1 —— art-mapping.json 填映射

编辑 `games/loop-td/art-mapping.json`。**UI 2D 键位约定**（round4 DRILL 起）：

```jsonc
"ui-panel-command-card": "岩库:<asset_id>@v1",   // ui-<art-ui.json 的 item id>
"ui-slot-command":      "岩库:<asset_id>@v1",
```

- 键 = `ui-` + `games/loop-td/docs/art-ui.json` 里对应 item 的 `id`；
- 值 = `岩库:<asset_id>@v<版本号>`；`@vN` 是**注释性**的——版本真相在
  art-assets.json（升级日两处要一起改，见 §7 与缺陷 F6）。
- 该文件目前无工具消费，纯人/AI 契约面（谁配了什么、可追溯）。

## 3. Step 2 —— art-assets.json 起草锁清单

`games/loop-td/art-assets.json`（照 `games/warden/art-assets.json` 先例），
首产日新增条目：

```jsonc
{
  "id": "<岩库资产 id>",                       // status 输出的 asset_id
  "name": "panel-command-card 面板底",          // 人类可读，与岩库名一致最好
  "version": 1,                                // 锁定当时的版本号（核心）
  "license": "<来源/许可留痕>",
  "demand": "<demand_id>",                      // 需求回链（三层追溯：需求→资产→锁定版本）
  "files": { "panel_command_card.png": "ui/panel_command_card.png" }  // 库内文件名 → assets/ 相对路径
}
```

规则（art-sync 强制）：目标路径只能是 `assets/{models,textures,audio,fonts,ui}/`
下的**单层文件名**（snake_case）。UI 贴图落 `ui/`。

## 4. Step 3 —— art-sync 拉文件 + 提交

```bash
# 1) 先看计划（不写盘）
python3 tools/art-sync/art_sync.py games/loop-td --dry-run

# 2) 真同步（退出码 = 问题数，0 才算干净）
python3 tools/art-sync/art_sync.py games/loop-td --outdated

# 3) 字节校验（工具只比字节数，内容级校验自己补一道）
shasum -a 256 games/loop-td/assets/ui/<file>.png
#   与岩库 Web 端该版本文件的哈希对照；或导出后 cmp 原件

# 4) 提交：资产文件 + art-assets.json（+ art-mapping.json）一起
git add games/loop-td/assets/ui/<file>.png games/loop-td/art-assets.json games/loop-td/art-mapping.json
```

`--outdated` 顺带打印 demand 回链状态与可升级提示（三层追溯的可视面）：

```
[art-sync] demand: <name> ← 「<需求名>」(fulfilled)
[art-sync] 可升级: <name> 锁定 v1，岩库 最新 v3，稳定 v3
```

## 5. Step 4 —— 挂接 uitextures.ron，--art 看效果

**当前引擎只有两个贴图槽位**：`panel`（全部面板根共用）与 `slot`
（命令卡槽位/装备行共用）——见附录 B 缺陷 F2/F3/F4。

`games/loop-td/assets/ui/uitextures.ron`（`--art` 的默认 manifest 路径）：

```ron
(textures: {
    "panel": "panel_command_card.png",   // 相对 manifest 所在目录
    "slot": "slot_command.png",
})
```

启动（真机，一次）：

```bash
cargo run -p loop-td -- --art
# 或显式指定：cargo run -p loop-td -- --art --art-manifest <路径>/uitextures.ron
```

**看控制台第一行**：

- 成功：`[loop-td] art mode on: 2 texture(s) from .../uitextures.ron`
- 失败（任何 miss 都**安全降级灰盒**，不崩溃）：
  `[loop-td] art mode fallback to greybox (manifest_ok=..., loaded=[...], failed=[...])`

**离屏取证（不看窗口也行，CI/无头友好）**：

```bash
cargo test -p loop-td --test art_drill evidence -- --ignored --nocapture
# 产物：docs/img/art-drill/{greybox,art-drill-v1,art-drill-v3}.png
```

真机替代（帧脚本自退出，带 --art 出全套 LT2 验收截图）：

```bash
cargo run -p loop-td -- --art --ui-mock v2   # 截图落 ui-proposals/lt2-final/
```

> 注意（彩排实证）：无值守/锁屏会话下窗口呈现会停滞，ui-mock 的 10 连拍
> 可能重复同一帧（已呈现的最后一帧）——连拍取证需要有人盯屏跑；可靠的无
> 头路径是上面的离屏取证（为免疫此坑而生，见 uitex_domain.rs 头注）。

## 6. Step 5 —— 锁定上报（三层追溯链闭合）

```bash
python3 tools/art-needs/art_needs.py games/loop-td sync
# 期望：[art-needs] 锁定上报: <name> v1 → demand <demand_id>
python3 tools/art-needs/art_needs.py games/loop-td status
# 期望：⊙ ...: 已有锁定记录（三层追溯链闭合）
```

提交 `docs/art-needs.json`（status/sync 会回填 asset_ref/locked 投影）。

## 7. 升级日（D 回环：岩库出了新版本）

```bash
python3 tools/art-sync/art_sync.py games/warden --outdated   # 看提示（照抄 warden 习惯）
# 1) 改 art-assets.json 的 version → 重跑 art-sync → 字节校验 → 一并提交
# 2) art-mapping.json 的 @vN 注释同步改（两处版本记录，缺陷 F6）
# 3) 重跑 --art / 离屏取证对比新旧形态；回归 cargo test -p loop-td -j4
```

**F1 陷阱（实证过，务必知道）**：art-sync 的本地一致性判断是**字节数**比对。
「美术改了内容但没改尺寸/编码」的升级（同字节数）会被判 `ok` 而**不拉取**——
本地文件停在旧字节，清单却已指向新版本。两个自保动作：

1. 升级后**必做字节/哈希校验**（§4 第 3 步）；
2. 发现没拉：`rm games/loop-td/assets/ui/<file>.png && 重跑 art-sync`（强制拉取）。

根治要等岩库补 sha256 字段 + art-sync 改哈希校验（缺陷 F1，附录 B）。

## 8. 常见故障排查表

| 症状 | 第一 suspicion | 验证命令 | 处置 |
|---|---|---|---|
| `--art` 后整体灰盒，控制台 `fallback to greybox` | manifest 路径错/文件名错 | 看控制台 `manifest_ok=` 与 `failed=[...]` 列表 | 修 uitextures.ron 路径（相对 manifest 目录）；任何 miss 都整体降级灰盒（引擎门条件：全部槽位加载成功才开 ArtMode） |
| `manifest_ok=false` | RON 语法错（多逗号/引号） | `ron` 解析报错在 warn 日志 | 对照 §5 样例；RON 对象结尾逗号要有，键要双引号 |
| 贴图显示了但**九宫格畸变**（角被拉花/边线粗细不匀） | 贴图边框环厚度 ≠ 主题 token（plan-a：panel 2px / slot 1px 源图像素） | 查看贴图源文件边框像素宽 | 换贴图（美术按 token 环厚出图）或调 `assets/ui/plan-a.ron` 的 `panel_border_w`/`slot_border_w`——两边必须一致；需求规格「角块≥24px@2x」与现 token 的矛盾见缺陷 F5 |
| 升级后游戏里**没变化** | F1：同字节数升级没被拉取 | `shasum` 对比岩库该版本；`ls -l` 看字节数是否恰好相同 | rm 本地文件重跑 art-sync；长期方案见 F1 |
| art-sync 报 `清单锁定的 vN 在岩库不存在` | 版本号写错/岩库版本被撤 | 报错会列出 `（现有: [1, 2, 3]）` | 改成存在的版本重跑 |
| art-sync 报 `回链 demand ... 查无（HTTP 404）` | demand 字段打错 | 对照 status 输出的 demand_id | 修正回链；注意它只报问题**不拦截**资产拉取 |
| art-sync 报 `岩库不可达` | 岩库服务没起/地址错 | `curl http://127.0.0.1:8765/api/v1/demands/stats` | 先起服务；构建/测试**不依赖**岩库在线，只影响同步步 |
| 贴图显示但**某个面板还是纯色块** | 该槽位没加载成功（per-surface 降级） | 控制台 `failed=[...]` | 修文件名；降级色 = 主题 token 色，不会白屏 |
| 想给 icon 换图 | —— | —— | **暂不可能**：引擎还没有 icon 表面消费点（缺陷 F4）；icon 位需求卡先攒着 |

## 9. 已知边界（首产日的期望管理）

- **一张 panel 底图喂全部面板根**：hud-plaque / command-card / popup / end-screen /
  equip-rack / nameplate 六张需求图今天只能挂一张到 `panel` 槽（F2）。
- **slot 面贴图无需求卡**：引擎消费 `slot` 槽但需求板没这条卡（F3）。
- **icon 无接线缝**（F4）。以上三条意味着首产日「可见收益」= panel+slot 两图。

---

## 附录 A —— DRILL 彩排全程实录（2026-10-28，round4/drill worktree）

彩排环境：mock 岩库（`assets-art-drill/mock_vault.py`，127.0.0.1:8799） +
伪游戏根（`assets-art-drill/first-day/`）+ 程序化演练贴图（非美术成品）。
逐环节结论表（命令均可原样重放）：

| # | 环节 | 命令 | 结果 | 关键输出/产物 |
|---|---|---|---|---|
| 0 | 生成演练贴图（模拟岩库交付） | `python3 games/loop-td/assets-art-drill/make_drill_textures.py` | OK | vault/ 5 张 PNG + vault_db.json（panel v1 96x96 556B / v2 同尺寸 556B / v3 112x112 613B / slot 40x40 292B / icon 128x128 772B 未挂接） |
| 1 | 起模拟岩库 | `python3 games/loop-td/assets-art-drill/mock_vault.py` | OK | `serving http://127.0.0.1:8799/api/v1 (2 assets, 2 demands)` |
| 2 | Step0 status | `python3 tools/art-needs/art_needs.py games/loop-td/assets-art-drill/first-day status` | OK (0) | `✓ ui-panel-command-card: fulfilled 资产 d011…（可进 art-mapping.json / art-assets.json）`×2；asset_ref 回填 |
| 3 | Step1 mapping | 编辑 `games/loop-td/art-mapping.json` | OK | 2 条 `ui-*` 演练映射 + note 立键位约定 |
| 4 | Step2 锁清单 | 起草 `first-day/art-assets.json`（v1 锁 + demand 回链） | OK | ADR-0009 格式条目 ×2 |
| 5 | Step3 dry-run | `python3 tools/art-sync/art_sync.py …/first-day --dry-run` | OK (0) | `将下载: …drill_panel.png <- drill_panel.png v1 (本地缺失)` ×2 |
| 6 | Step3 同步 | `…/art_sync.py …/first-day --outdated` | OK (0) | `已下载`×2；`demand: … ← 「ui-panel-command-card（演练）」(fulfilled)`；`可升级: … 锁定 v1，岩库 最新 v3` |
| 7 | 字节校验 | `cmp` 本地 vs vault v1 | OK | 逐字节相等；shasum panel=`0292dec6…` slot=`824c0b0b…` |
| 8 | Step4 挂接 | 写 `first-day/assets/ui/uitextures.ron`（panel/slot 两键） | OK | `--art` 入口就绪 |
| 9 | **D 升级·陷阱** | 锁 v2（同 556B）→ art-sync | **F1 实锤** | `ok: … (v2, 556 bytes)` **未拉取**，cmp 证实本地仍 v1 字节 |
| 10 | D 陷阱自保 | `rm` 本地 → art-sync | OK | `已下载 (v2, 本地缺失)`，cmp == v2 字节 |
| 11 | **D 真升级** | 锁 v3（613B）→ art-sync | OK | `已下载 (v3, 尺寸不一致)`，cmp == v3 字节，shasum=`90f5a33d…` |
| 12 | Step5 sync 上报 | `art_needs.py …/first-day sync` | OK (0) | `锁定上报: drill_panel nine-grid v3 → demand dem-drill-0001`×2 |
| 13 | 追溯闭合复验 | `status` | OK (0) | `⊙ …: 已有锁定记录（三层追溯链闭合）`×2 |
| 14 | 负路径·坏版本 | 伪清单 v99 | 拦截 (2) | `清单锁定的 v99 在岩库不存在（现有: [1, 2, 3]）` |
| 15 | 负路径·坏回链 | demand=dem-drill-9999 | 拦截 (2) | `回链 demand dem-drill-9999 查无（HTTP 404）`（不拦资产拉取） |
| 16 | 引擎侧升级断言 | `cargo test -p loop-td --test art_drill drill_manifest_loads -- --ignored` | OK | v3 解码 112x112、reload revision 1→2 换柄重建、换 v1 manifest revision 3 再换柄（keyed rebuild 实证） |
| 17 | 离屏取证 | `cargo test -p loop-td --test art_drill evidence -- --ignored` | OK | `docs/img/art-drill/{greybox,art-drill-v1,art-drill-v3}.png`（81613/164760/179022 B，三形态两两不等；视觉复验：v1=金环+红绿蓝黄角块，v3=青环+洋红/橙/青柠/紫角块） |
| 18 | 契约断言 | `cargo test -p loop-td --test art_drill contracts -- --ignored` | OK | mapping/锁清单/同步字节三契约（含 v3 终态字节等值） |
| 19 | 真机冒烟（一次） | `cargo run -j4 -p loop-td -- --art --art-manifest games/loop-td/assets-art-drill/first-day/assets/ui/uitextures.ron --ui-mock v2` | OK* | 控制台 `[loop-td] art mode on: 2 texture(s) from …uitextures.ron`；取 `docs/img/art-drill/binary-art-smoke.png`（像素复验：青环 5376px + v3 四角色 12k+px，drill 贴图在真窗口二进制上实渲染）。*注：无值守会话下 ui-mock v2 的 10 连拍会重复同一已呈现帧（窗口呈现停滞，uitex 轮已知的锁定会话坑族）——全套连拍需有人盯屏跑一次；离线取证以 17 的离屏三连为准 |

## 附录 B —— 流程缺陷清单（彩排发现，待转交 `docs/topics/art/game-art-cross-project-workflow.md` 维护方）

> 本手册不改工作流文档本体；以下逐条按「现象/根因/建议」记录，转交改进。

- **F1（高）art-sync 一致性判断仅比字节数，同尺寸内容升级静默不拉取。**
  实证：锁 v2（556B 同尺寸）→ `ok` 不拉，本地停留 v1 字节而清单已指 v2。
  根因：岩库无 sha256 字段，工具退化为 size 比对（README 已知）。
  建议：岩库补哈希字段；art-sync 改哈希校验；过渡期手册 §7 的 rm+重拉自保。
- **F2（高）UI 贴图槽位供给 1 个 panel 槽，需求板有 6 张面板底图需求。**
  hud-plaque/command-card/popup/end-screen/equip-rack/nameplate 六卡 fulfilled
  也只能共用一张图（全部 PanelRoot 同键 `panel`）。首产日「六张图到货、
  只能上一张」。建议：UiTextureManifest 支持按 PanelKind 分槽（或键名空间
  `panel/<item-id>`），引擎消费点按面板种类取键。
- **F3（中）引擎消费 `slot` 槽位但需求板没有 slot 面贴图需求卡。**
  art-ui.json 全是 panel/icon，没有「槽位框底图」条目；首产日美术不会交付它。
  建议：art-ui.json 补 `panel-slot-command`（或同义）声明。
- **F4（中）icon 表面无消费缝。** 引擎仅 TODO 注释（ui/mod.rs `UI_TEX` 节），
  22 张 icon 需求 fulfilled 后无处挂接。彩排生成了 drill_icon_v1.png 存档
  作证据，未挂接。建议：DRILL 后续增量落 icon 消费点（icon_size 锚、
  center-fit 不缩放）。
- **F5（高）需求规格与引擎切分 token 矛盾：面板验收标准「角块 ≥24px（@2x）」
  vs plan-a `panel_border_w: 2.0`（源图像素）。** 美术按规格出图会被引擎按
  2px 切——角饰被拉伸畸变。演练贴图环厚按 token（2/1px）画才正确。
  建议：美术解禁前对齐——要么 token 升到 12.0（@2x 24px），要么规格改口径；
  并把「切分环厚」写进需求卡 spec 必填字段。
- **F6（中）版本号双处记录（art-mapping `@vN` 注释 vs art-assets 锁版本），
  无工具对账。** 升级日漏改 mapping 不被发现。建议：art-needs sync 顺带
  校验 mapping 的 @vN 与锁清单一致（或 mapping 去掉 @vN）。
- **F7（低）art-needs status 的 `✓ fulfilled 资产` 提示基于 asset_count，
  不区分版本是否 stable。** 彩排 v2 为 unstable 也会提示可导入（自担风险，
  岩库哲学）。建议：提示里带 stable 标记。
- **F8（中）art-sync 无离线/本地校验模式，硬依赖岩库 REST 在线。** 彩排需要
  mock 岩库才能重放 C2；断网/CI 无法做清单-资产一致性巡检。建议：art-sync
  增加 `--offline`（只做本地：清单 vs 已同步文件的哈希/字节数对账 + RON/JSON
  可解析性检查）。
- **F9（低）art-sync --dry-run 结束语误导。** dry-run 计划了下载但汇总行打
  「无需变更」（actions 只在真下载时自增）。建议：dry-run 汇总区分「计划
  N 个下载」。
- **F10（低）art-needs 游戏目录校验要求 `maps/` 存在，伪根/纯 UI 接线场景
  被拦。** 彩排给 first-day/ 塞 maps/ 占位 README 才通过。建议：status/sync
  不依赖 maps/（只有 scan 需要）。
