---
title: 美术资产管理系统功能设计（art-catalog 设计稿 v0.5）
type: reference
topic: game-design
date: 2026-08-29
author: AI 起草 + youxia 三轮反馈
status: draft
tags: [game-design, art-pipeline, tooling, proposal, design]
related:
  - "[[topics/game-design/art-pipeline-3d-v2|3D 美术生产管线 v2]]"
  - "[[topics/game-design/ai-asset-pipeline|AI 生成美术管线八站流程]]"
  - "[[topics/game-design/blender-gltf-wash-pitfalls|踩坑：Blender 无头 glTF 洗白五连坑]]"
---

# 美术资产管理系统功能设计（art-catalog 设计稿 v0.5）

## 结论

为美术管线建一套**管理系统**：目录（资产全量事实）+ 检查（规则引擎）+ 可视化（人看的页面）+ 入库（intake 工单）+ 场景化操作（从已实操流程提炼的场景注册表）+ 评审支持。三条地基（2026-08-29 youxia 反馈钉死）：

1. **人操作，AI 辅助**——人负责浏览、拍板、批准、验收；AI 代查/代跑/代读/代写。
2. **两层资产域**——通用可用资产库（供侧，工作区级 `_library/`，v1 即迁移）与游戏内资产（需侧）分开管理，关系流为「上架/退役」。
3. **长期兼容 bevy-game 模板**——系统期望的目录/命名/元数据约定沉淀为规范层，模板文档本轮已做标记级更新；确定采用后模板预置 `_art/` 骨架。

v0.4 评审落定（youxia 逐项拍板，待另两位队友 review）。已拍板：暂不动工，动工另择时机；v1 范围全类型资产。

## 一、现状与痛点

| 环节 | 位置 | 现状 |
|------|------|------|
| 原始素材 | `games/wave-survival/_art/raw/`（mixamo FBX 包、Blends、glTF/OBJ） | 60+ 文件，含 32MB 大 GLB |
| 洗白 | `tools/art/normalize.py`（Blender headless） | 统一身高/脚底原点/clip 改名/色板量化 |
| 图册+元数据 | `tools/art/turntable.py` | 每模型 4 角度 PNG + `meta.json`（身高/三角数/材质/clip/包围盒/碰撞半径候选） |
| 图册库 | `_art/gallery/`（18 候选）、`_art/gallery-washed/`（7 成品） | 各带 `gallery_summary.json` 汇总 |
| 运行时资产 | `assets/models/*.glb`（7 个） | 引用集中在 `src/components.rs` 敌人定义表 + `src/plugins/presentation.rs` |
| 风格裁判 | `games/wave-survival/docs/style-bible.md` | 色板/命名/比例/禁止项全有明文 |

痛点：① 25 个模型无总览页；② 孤儿已产生（`hero.glb`/`monster.glb` 弃用未清）；③ raw→glb 链条与 stale 靠人脑记；④ 评审「20 选 1」纯手工贴图。

## 二、方案选型

| 方案 | 形态 | 优点 | 缺点 | 判断 |
|------|------|------|------|------|
| **A. 本地目录系统**：Rust CLI 扫描 → HTML（人）+ JSON（AI）双出口 | `cargo run` 一次，浏览器打开 | 零常驻进程零新基建；与「blender -b -P …」工具链同构；三人 Rust 栈 | 无在线编辑（本也不需要） | ✅ 推荐 |
| B. 常驻 Web 服务 | 本地起服务实时扫描 | 可做一键重洗、拖拽上传 | 维护成本高一个量级；违反「按需自研」 | ❌ 元素留 v2 评估 |
| C. Bevy/egui 桌面预览器 | 原生窗口加载 glb | 真 3D+动画+游戏光照 | 开发量大；不擅长管理功能 | ❌ 将来以「游戏内模型轮播调试模式」立卡 |

兼容性：art-pipeline-3d-v2 已否决 Bevy 自研 AssetPreview、规定「外部现成优先，自研只在与 Bevy 运行时耦合处」。本系统是管线之上的**盘点+管理层**（出图仍归 turntable.py），自研正当在于必须理解本仓库目录约定——不与该决策冲突。

## 三、角色与定位

**人是操作者，AI 是辅助，工具是引擎。**

| 角色 | 职责 | 明确不做 |
|------|------|----------|
| **人（操作者）** | 浏览资产、评审拍板（20 选 1）、下达操作指令、批准破坏性动作（删除/重洗/改引用）、验收签字 | — |
| **AI（辅助操作）** | **代查**：资产问答；**代跑**：按人的指令执行场景步骤（洗白/图册/嫁接/扫描）并汇报；**代读**：解读检查报告、给修复清单；**代写**：起草评审笔记、intake 单、场景卡草稿 | 不自主决策；未经人批准不执行任何写操作；不做审美判断 |
| **art-catalog CLI（引擎）** | 确定性扫描+检查+报告生成，退出码门禁 | 永不修改任何美术资产（只写报告文件） |

AI 的系统知识来源是仓库文档（CLI 契约 + 本设计稿 + style-bible + 场景卡），不在某个会话的记忆里——换新会话、换工具，辅助能力不丢失。

## 四、两层资产域

系统管理的对象分两个域，**分开建模、分开视图，用「上架/退役」流连接**：

| 域 | 定位 | 内容 | 属性重心 |
|----|------|------|----------|
| **通用资产库（Library，供侧）** | 跨游戏可复用的「货架」 | raw 素材、候选件、已洗白成品、套装拆包件、AI 生成件 | 来源 / 许可证 / 规格 / 标签 / 图册 |
| **游戏资产（Game-bound，需侧）** | 当前游戏实现绑定的资产 | `assets/` 运行时文件 + 代码引用（定义表/常量）+ 更替历史 | 引用关系 / 健康（孤儿/stale）/ 版本更替 |

**关系流**：

- **上架 adopt**（库 → 游戏）：资产进 `assets/` + 代码引用登记 + catalog 记录 adopted 关系
- **退役 retire**（游戏 → 库/归档）：清代码引用 → 资产回库或归档（如 `hero.glb` 的正确归宿）

游戏侧管理内容（2026-08-29 youxia 拍板：**绑定清单加入 v1**；其余候选待定）：

1. **绑定清单**：哪个系统/定义表行引用哪个资产（引用图，反查「删这个会影响谁」）——✅ 已拍板加入 v1
2. **引用健康**：孤儿/stale 即游戏侧的核心健康指标（复用 M2 规则 R1/R2，随 v1 规则天然到位）
3. **更替历史**：换皮记录（`hero.glb` → `player_hunyuan.glb`，何时因哪张卡），防「改了忘了」——待定
4. **退役流程**：标准的「下架」操作场景（清引用 → cargo test → 移库/归档 → 复扫确认）——待定

物理落位（2026-08-29 youxia 拍板：**v1 即上移工作区级**）：

- 库目录：`<repo>/_library/`（下划线 = 非运行时约定，与 `_art/` 同构），结构 `{raw, gallery, washed, intake}/`——原始素材 / 候选图册 / 洗白成品货架 / 入库工单
- 迁移（`git mv` 保历史）：`games/wave-survival/_art/raw/` → `_library/raw/`；`_art/gallery/` → `_library/gallery/`；`_art/gallery-washed/` → `_library/washed/`；`.gitignore` 同步调整（raw 大文件规则、`_library/catalog/`）
- 游戏侧 `games/<game>/_art/` 保留：`catalog/`（扫描输出，gitignored）+ `scenarios/`（项目覆盖卡）+ 游戏专属工作文件
- 多游戏红利：第二个游戏建库即用，SC13 跨游戏上架的依赖随之就绪
- 提交纪律：迁移单独成 commit（纯 `git mv`），勿与队友未落库的美术 WIP 混提

## 五、系统功能设计（六模块）

| 模块 | 功能点 | 输入 → 输出 | 谁在用 |
|------|--------|-------------|--------|
| **M1 扫描引擎** | ① 全类型文件发现（游戏 `assets/` 各子目录 + 工作区库 `_library/` 各层）；② `meta.json`/`gallery_summary.json` 解析；③ 代码引用扫描（`src/`、`tests/` 字面量匹配）；④ mtime stale 链检测 | 仓库目录 → `catalog.json`（含两层域标记） | CLI 调用者（人或 AI 代跑） |
| **M2 检查引擎** | 规则 R1–R7（第八节），每条产出含证据与修复建议的 finding；规则外置、对齐 style-bible | `catalog.json` → `report.json` + 退出码 | 人看报告页；AI 代读出修复清单 |
| **M3 可视化页面** | 概览仪表；流水线追踪表；缩略图画廊（搜索/筛选/排序）；全类型清单（PNG/音频/字体可预览）；检查报告页；**两层域切换视图**（库视图 / 游戏视图）；**绑定清单**（游戏资产的反向引用页：资产 → 定义表行/常量/测试，反查「删这个会影响谁」） | JSON → 单个自包含 HTML（中文 UI） | **人** |
| **M4 AI 辅助接口** | 读接口：`catalog.json`/`report.json`（schema 版本化）；执行接口：CLI 退出码门禁；场景卡即 AI 的操作手册 | JSON + README 契约 + 场景卡 → AI 的辅助动作 | **AI（辅助人）** |
| **M5 入库与场景执行** | intake 请求单（`_library/intake/<date>-<slug>.json`，状态机 `new → in-pipeline → landed / rejected`）；**按场景卡执行**：AI 代跑步骤、在拍板点停下等人 | 会话/人的需求 → 场景卡 → 入库/变更 + 状态回写 | 人下指令+拍板，AI 代跑 |
| **M6 评审支持** | 对比模式（勾选并排：图册+数据表）；评审笔记草稿（Obsidian 格式）；拍板结果记录 | 勾选集 → 对比页 + 笔记草稿 | **人**（AI 只起草） |

M5 设计原则：**会话当触发器，仓库当状态机**——请求与场景进度落在仓库文件里，不因会话结束而丢失；跨会话不依赖翻聊天记录。需要留档的对话决策按 `docs/AGENTS.md` 走 `docs/raw/` 摄入。

## 六、操作场景注册表（自动化核心）

场景 = 一段可重复的操作流，**从已实操记录提炼，不是凭空设计**。每个场景一张**场景卡**（机器可读步骤表），AI 读卡辅助执行，人只在拍板点出现。

### 6.1 已操作场景（v1 内置，来源为仓库实录）

| 编号 | 场景 | 来源实录 | 步骤骨架 | 拍板点（人） |
|------|------|----------|----------|--------------|
| SC1 | 单件洗白入库 | 卡 17 ArtAssetPipeline（已实现） | raw → `normalize.py`（--height/--max-tris/--tex-size）→ `turntable.py` → scan 登记 | 复检翻图 |
| SC2 | AI 生成新角色（全流程） | 八站流程；玩家混元首例（2026-08-28 实测 50k→10k 面/30.9→3.3MB） | 需求单 → 概念定调 → 批量抽卡 → raw/ai 入库 → 绑骨（Mixamo/程序/素材 clip 三选一）→ 洗白 → 复检 → 入库+定义表 → 进游戏 | 挑定调图、挑模型、商用许可核对、复检 |
| SC3 | 套装采购拆包入库 | 卡 18 路线 B（KayKit/Quaternius） | 套装挑选 → 抽件导出 → normalize（helper 网格剔除）→ 图册 → 入库 | 挑件 |
| SC4 | Mixamo 动画嫁接 | 卡 25 配方（2026-08-29 实操；picked FBX 集 → `mixamo_merge.py --rigged` → `normalize --height` → clip 重排钉测） | 嫁接 → 替换 glb → HERO_CLIP_* 重排 → 钉测 | 动作取舍、真机验收 |
| SC5 | 换皮接表 | 卡 19/21（enemy 定义表 + HERO_GLB 常量） | 新 glb 就位 → 改定义表/常量引用 → `cargo test` → 真机验收 | 真机视觉验收 |
| SC6 | 资产体检 | 本系统 M2 | scan → report → 人批准修复（改名/清孤儿/补图册）→ 复扫归零 | 修复批准 |

### 6.2 预留场景（只登记扩展位，不实现）

SC7 贴图入库 ｜ SC8 音频入库 ｜ SC9 字体入库 ｜ SC10 武器/配件插槽（Mixamo 骨骼映射红利）｜ SC11 动画重定向（UniRig 模板骨架路线）｜ SC12 palette lock 启用后批量复洗 ｜ SC13 库资产跨游戏上架（落位已定 `_library/`，启用待多游戏需要）

### 6.3 场景卡机制（扩展方式）

- **格式**：每场景一个文件（JSON 步骤表 + MD 说明配对）：触发语、步骤序列（每步标注执行者 `auto` / `ai-assist` / `human`、命令、产物、失败处理）、拍板点、验收句
- **存放**：`tools/art-catalog/scenarios/`（系统内置）+ `<game>/_art/scenarios/`（项目自定义覆盖）——✅ 已拍板双层
- **扩展 = 加卡片，零代码改动**：后续新场景（如音频入库）只需写一张新卡，AI 与页面即识别——这是「方便后续扩展」的落地
- v1 交付 SC1/SC3/SC4/SC5/SC6 五张内置卡；SC2 全流程卡 v1.5 打通（其前半段本质是人挑图，AI 辅助点在卡片上标清）

## 七、数据流

```
【通用资产库域 `_library/`】                    【游戏资产域】
raw/ 素材 ──场景卡 SC1-SC4──▶ 洗白成品+图册 ──上架 adopt──▶ assets/ 运行时资产
   ▲                              │                          │
intake 请求单（AI 代跑，人拍板）      scan（M1+M2）              代码引用（定义表/常量）
   ▲                              │                          │
会话/人需求                        catalog.json（domain: library | game）
                                                              │
                                    report.json + index.html（两层域视图切换）
                                         │                │
                                    人：浏览/评审      AI：读 JSON 辅助人
                                         └───────┬────────┘
                                           人的操作指令（批准）
                                                 │
                                    场景步骤执行 → 复扫验证（退出码归零）
退役 retire：清引用 → cargo test → 资产回库/归档（如 hero.glb 的归宿）
```

## 八、数据模型（catalog.json 核心）

- `Asset`：`id`、`kind`（model/texture/audio/font/ui）、**`domain`（library | game）**、`paths`（各阶段）、`meta`（模型专有）、`refs`、**`adopted_into[]`**（上架关系）、`flags`
- `IntakeRequest`：`id`、`date`、`requester`、`source`、`license`、`target_domain`、`scenario`（关联场景卡 id）、`expected_clips`、`status`
- `Finding`：`rule_id`、`severity`、`subject`、`evidence`、`fix_hint`
- `ScenarioCard`：`id`、`trigger`、`steps[]`（executor/command/artifact/on_fail）、`approval_points[]`、`acceptance`
- 顶层 `schema_version`，升级走版本号

## 九、v1 检查规则

| 规则 | 内容 | 首跑预期 |
|------|------|----------|
| R1 孤儿资产 | `assets/` 有、`src/`+`tests/` 均不引用（游戏域） | 抓出 `hero.glb`、`monster.glb` |
| R2 stale 链 | raw > glb、glb > meta.json、glb > 渲染 PNG（mtime） | — |
| R3 命名 | runtime 模型非 snake_case | — |
| R4 clip 约定 | clip 名不在 style-bible 集（超集报告不阻断） | — |
| R5 候选未消化 | 库域图册有、从未上架 | 18 候选 vs 5 上岗 |
| R6 meta 缺失 | 有 glb 无 meta.json | — |
| R7 大文件预警 | 超阈值文件，提示入库策略 | raw 32MB GLB |

## 十、模板兼容与规范层（兼容 bevy-game）

系统期望的仓库约定沉淀为规范，**模板 adopting 即兼容**：

| 编号 | 约定 | 模板现状 |
|------|------|----------|
| C1 目录 | 工作区库 `_library/{raw,gallery,washed,intake}` + 游戏侧 `assets/{models,textures,audio,fonts,ui}` + `_art/{catalog,scenarios}` | assets 五目录已有 ✓；`_library/` 为 monorepo 级（采用系统时创建，模板不预置）；游戏 `_art/` 待预置 |
| C2 命名 | 模型 snake_case；clip 约定名（idle/walk/attack/hit/death）；intake/场景卡 kebab-case 日期前缀 | AGENTS.md 已有「资产约定」条目，本轮扩写 ✓ |
| C3 元数据 | `meta.json`（turntable 产出）/ `catalog.json` / intake 单 schema 版本化 | 由本系统供给 |
| C4 工具共享 | `tools/art/` 在 monorepo 根，全游戏共用；场景卡用仓库相对路径引用工具 | 已成立 ✓ |

**更新动作分两档**：

- **本轮已完成（标记「拟采用、待定」）**：`templates/bevy-game/AGENTS.md` 资产约定扩写（+`_art/` 约定与命名规则）；`templates/bevy-game/README.md` 目录说明补注——新游戏从模板复制即自带兼容约定，**不预创建目录、不装工具**，零成本
- **确定采用后再做（翻转待定标记）**：模板预置 `_art/` 骨架 + `.gitignore` 条目（raw/catalog 产物）；AGENTS.md 指向正式规范文档；存量 wave-survival 对齐；通用库物理落位（多游戏共享）

## 十一、技术实现要点

- `tools/art-catalog/` 独立 Rust crate；依赖最小（walkdir + serde + 标准库；HTML 用 format! 拼；缩略图相对路径，不引 image crate）
- 引用检测 = `src/`/`tests/` 字符串匹配；零 Blender 依赖（重活归现有工具，本系统调度与登记）
- CLI：`art-catalog --game games/wave-survival --library _library --out games/wave-survival/_art/catalog`（`--library` 缺省自动探测，不存在则只扫游戏域）；退出码 = finding 数
- 输出 `_art/catalog/` 进 gitignore，一条命令再生成（✅ 已拍板）

## 十二、分期

- **v1**：M1+M2+M3（含两层域视图 + 绑定清单）+ intake 约定 + **场景卡机制与 SC1/SC3/SC4/SC5/SC6 五张内置卡** + R1–R7 + 双出口 + **库迁移**（`git mv` 上移 `_library/` + `.gitignore` 调整，单独成 commit）
- **v1.5**：M6 对比评审 + SC2 全流程卡
- **v2（按需评估）**：serve 热刷新、CI 门槛、一键重洗、预留场景按需启用

## 十三、验收句草案（按团队能力卡格式）

1. 「对当前仓库运行 scan，`report.json` 必须列出 `hero.glb`、`monster.glb` 为孤儿资产，且代码引用的 5 个模型均标记 referenced；误报数 = 0」
2. 「每条 finding 必含 evidence；人为制造一个命名违规后复扫，退出码 ≥1，修复后归 0」
3. 「catalog.json 中 7 个 runtime glb 均 `domain=game` 且其中 5 个带 refs；`_library/gallery` 18 个候选均 `domain=library`；adopted 关系与代码引用一致」
4. 「SC4 场景卡按步骤表 dry-run（只打印不执行），每步产物路径存在性检查通过；拍板点步骤 executor=human」
5. 「绑定清单视图：游戏域每个带 refs 的资产展示全部引用位置，与独立 grep 核对一致；`green_blob.glb` 须显示 `components.rs` 敌人定义表 grunt 行的引用」

## 十四、拍板记录与待评审清单

拍板记录：

- 2026-08-29（youxia）：① 先沉淀文档暂不动工；② v1 范围全类型。
- 2026-08-29（youxia 纠偏）：AI 定位 = 辅助人操作系统，非自主管家。
- 2026-08-29（youxia 三条意见，升 v0.3）：① 场景自动化从已操作流提炼并留扩展位；② 区分通用资产库/游戏资产两层域（游戏侧管理内容 youxia 未想好，已列候选提案待挑）；③ 长期兼容 bevy-game 模板——规范层 C1–C4 已立，模板文档本轮标记级更新，正式接入后预置翻转。
- 2026-08-29（youxia）：**绑定清单拍板加入游戏侧管理 v1**；设计继续推进。
- 2026-08-29（youxia，验收反馈，升 v0.5）：**① 3D 动画播放器采纳**（参照 Mixamo：three.js 内嵌 + 按模型分包 base64 懒加载，file:// 可用，见 AC-2）；**② 页面导入按钮采纳**（File System Access API 把文件写入 raw 目录，Edge/Chrome 支持，其余浏览器回退手动复制）；**③ 导入自动化分级规划**见下节——人工验收点永不自动化。
- 2026-08-29（youxia）：**开工**——按能力卡 AC-1 实现 v1（`tools/art-catalog/`），真仓验收 5/5 通过（详见该卡）；库迁移 `_library/` 待与队友美术 WIP 协调后单独 commit。
- 2026-08-30（youxia，验收拍板，升 v0.6）：**上架流水线状态机动工**——`intake create/set/list` 子命令族（L1.5 提前实现，状态机规则进工具）、SC1 卡接工单命令、资产表管线状态列 + 图册已上架/候选徽章；全链验收通过（演练工单 new→washing→review→landed，跳步与终态翻转均被工具拒绝，复扫回基线）。见「上架流水线状态机」节。
- 2026-08-30（youxia，验收拍板，升 v0.7）：**「AI 助手」采纳为确定性形态**——`art-catalog wash` 一键洗白（L2 提前落地，卡 AC-4）：把我演示跑单中的机械段固化成一条命令，停在 review 等人拍板；AI（编程 agent）继续担任自然语言接口层。**明确否决**：页面内嵌 LLM 聊天（key/费用/不确定性）、自动 landed、后台守护进程。

## 上架流水线状态机（v0.6 验收拍板）

从 raw 原件到图册上架的状态机：`new → washing → review → landed`（旁路 rejected，需备注；landed/rejected 为终态，重做开新工单）。

- **状态载体**：`<game>/_art/intake/<日期>-<名>.json`（迁移 `_library/` 后随库入库，成为团队可见协作痕迹）；迁移后定期 `_library/intake/`。
- **规则在工具里**：`art-catalog intake create|set|list`（`src/intake.rs`）——非法翻转（跳步/终态改写）返回退出码 1；create 强制 snake_case 目标名与许可证字段（缺则退出码 2）；目标同名冲突在工单 notes 标注，洗白写入前须人显式确认。
- **扫描联动**：工单按 `raw_file`/`target` 路径匹配资产，catalog.json 增加 `pipeline_status`；资产表显示 管线 列（已上架/待拍板/洗白中/已立案/已拒/未立案），图册卡片徽章三态（已上架=游戏域 / 候选=洗白未进游戏 / raw 原件）。
- **人工关卡不变**：① 参数与许可证批准（washing 前）；② 图册翻检拍板（landed 前）。SC1 场景卡已接工单命令（6 步 / 人拍板 2）。

## 导入自动化规划（v0.5 验收反馈③）

导入主流程：**文件进 raw → intake 工单 → 按场景卡洗白上架 → 人工验收**。自动化按「哪些环节必须有人」分级：

| 级别 | 形态 | 自动化范围 | 状态 |
|------|------|-----------|------|
| L0 纯人工 | 手动拷文件 + 手动跑 blender 命令 | 无 | 管线本身已具备 |
| L1 AI 辅助 | 页面导入按钮/手动放 raw → 对 AI 说触发语 → AI 落单并按卡逐步执行 | 文件搬运、命令执行、复扫登记 | **当前（已实现）** |
| L1.5 CLI 半自动 | `art-catalog intake create` 一条命令生成工单 JSON + 打印下一步指令 | 工单生成 + 状态机校验 | **已实现（v0.6，含 set/list）** |
| L2 半自动管线 | `art-catalog wash --file --height --license`：工单+normalize+turntable+条带+review+复扫一条龙，停在 review 等拍板 | 全部机械环节（覆盖仍需 `--yes`，拍板永不自动） | **已实现（v0.7，卡 AC-4）** |
| L3 全自动远景 | 页面/CI 触发全流程 | 全部机械环节 | 远景（依赖 L2 稳定 + 团队信任） |

**永不自动化的人工环节**：① 许可证确认（法律风险）；② 复检拍板（翻图册/真机看效果）；③ 覆盖已有资产前的确认（防误删队友产物）。
- 2026-08-29（youxia，评审落定，升 v0.4）：A 组七项按推荐落定（① 名 `art-catalog`；② 输出 gitignore；③ R1–R7 无增删；⑤ 中文 UI；⑥ 重洗留 v2；⑩ 模板预置待采用——④ intake 位置随 ⑦ 调整为 `_library/intake/`）；⑦ 通用库 **v1 即上移工作区级 `_library/`**（迁移计划见第四节）；⑨ 场景卡双层存放。**待评审清单全部落定**（仅更替历史/退役流程留待定）——设计评审完成，动工另择时机。

评审落定清单（2026-08-29 两轮，全部有主）：

| 项 | 结论 |
|----|------|
| ① 系统名/crate 名 | `art-catalog` |
| ② 输出物入库策略 | gitignore + 一条命令再生成 |
| ③ 检查规则 | R1–R7 通过 |
| ④ intake 位置 | `_library/intake/` |
| ⑤ 页面 UI | 中文 |
| ⑥ 一键重洗 | v2 按需评估 |
| ⑦ 通用库物理位置 | v1 上移工作区级 `_library/` |
| ⑧ 游戏侧管理内容 | ✅ 绑定清单 + 引用健康入 v1；更替历史 / 退役流程待定 |
| ⑨ 场景卡存放 | 双层：`tools/art-catalog/scenarios/` + `<game>/_art/scenarios/` |
| ⑩ 模板预置时机 | 确定采用后翻转 |

## 参考

- 风格裁判：`games/wave-survival/docs/style-bible.md`
- 管线决策：[[topics/game-design/art-pipeline-3d-v2|3D 美术生产管线 v2]]
- 实操速查：[[topics/game-design/ai-asset-pipeline|AI 生成美术管线八站流程]]（SC2 来源）
- 场景实录：`games/wave-survival/docs/capability-cards.md` 卡 17/18/19/21/25（SC1/SC3/SC4/SC5 来源）；卡 25 复跑配方见 wave-survival AGENTS.md 当前状态
- 现有工具：`tools/art/normalize.py`、`tools/art/turntable.py`、`tools/art/mixamo_merge.py`
- 模板：`templates/bevy-game/`（AGENTS.md 资产约定 + README 目录说明，本轮已标记级更新）
