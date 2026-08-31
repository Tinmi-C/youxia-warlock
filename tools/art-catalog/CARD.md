# 能力卡 AC-1：art-catalog 资产目录系统 v1

> 状态：**实现中**（2026-08-29 立卡即实现，youxia 开工指令）。
> 设计依据：知识库 `docs/topics/game-design/art-asset-catalog-tool-proposal.md`（v0.4，评审全部落定）。
> 本卡遵循团队能力卡工作流：接口 / 行为 / 验收句；一卡一提交。

## 接口（CLI 契约）

```
art-catalog [--root <repo>] [--game <dir>] [--library <dir>] [--out <dir>]
            [--scan-only] [--scenario-check] [--help]
```

- `--root`：monorepo 根，默认当前目录（向上查找到含 `games/` 的目录）
- `--game`：游戏目录（如 `games/wave-survival`），默认自动发现（唯一含 `assets/` 的游戏）
- `--library`：工作区库目录，默认 `_library`（不存在则只扫游戏域，兼容迁移前布局）
- `--out`：输出目录，默认 `<game>/_art/catalog`
- 产出：`index.html`（人）+ `catalog.json` + `report.json`（机），schema_version=1
- **退出码 = finding 总数（0 = 全绿，上限 100）**

## 行为

- **M1 扫描**（只读）：发现游戏 `assets/` 与库 `_library/`（或迁移前 `_art/` 布局）全类型文件；解析 `meta.json`；扫描 `src/`+`tests/` 字面量引用；mtime stale 检测。
- **M2 检查**：R1 孤儿 / R2 stale / R3 命名 / R4 clip 约定 / R5 候选未消化 / R6 meta 缺失 / R7 大文件；每条 finding 含 evidence + fix_hint。
- **M3 页面**：中文单页 HTML，六视图（总览仪表 / 流水线追踪 / 图册画廊 / 全类型清单 / 绑定清单 / 检查报告 + 场景与工单），数据内嵌、缩略图相对路径。
- **场景卡**：`scenarios/` 内置 SC1/SC3/SC4/SC5/SC6；`--scenario-check` 校验卡结构与 check_paths（dry-run）。
- **只读红线**：除输出目录三个产物外，不写任何文件。

## 验收句（全部数字化、可执行）

1. 对本仓库运行 scan，`report.json` 必须列出 `hero.glb`、`monster.glb` 为孤儿（R1），代码引用的 5 个模型（green_blob/mushnub/yeti/mushnub_evolved/player_hunyuan）均标记 referenced，R1 误报数 = 0。
2. 每条 finding 必含非空 `evidence`；在 `assets/models/` 人为放置 `Bad_Name_TEST.glb` 后复扫，R3 触发且退出码较基线增加（≥1）；删除该文件后 R3 消失、总数回落基线（基线本身含真实 finding，如存量孤儿——工具抓它们正是本职）。
3. `catalog.json` 中 7 个 runtime glb 均 `domain=game`，其中 5 个带非空 refs；库域候选图册条目均 `domain=library`；adopted 关系与代码引用一致（有 refs ⟺ 非 orphan）。
4. `--scenario-check` 对 5 张内置卡全部通过：JSON 可解析、必填字段齐全、check_paths 存在。
5. 绑定清单数据中 `green_blob.glb` 须含 `components.rs` 敌人定义表（grunt 行）的引用记录，文件/行号与人工 grep 一致。

## 回归

- 验收句 1/3/5 由真仓扫描结果核对（本卡验收时人工核对 + 输出留存）。
- 验收句 2 为注入-清除式演示，随验收执行。

---

# 能力卡 AC-2：动画预览（动图条）

> 状态：**已实现，验收记录 3/3 通过，待人工验收**（2026-08-29）。
> 方案：真 3D（three.js 加载 glb）在 file:// 下被 CORS 拦死、常驻服务是已否决的 B 方案——故走 **Blender 离线渲染动图条（sprite strip）+ 静态页 JS 循环播放**。

## 接口

```
blender -b -P tools/art/anim_strip.py -- --model <glb> --meta-dir <图册目录> [--frames 10] [--res 256]
```

- 对模型每个 animation clip 渲 10 帧采样的横向动图条 → `<meta-dir>/anim/<clip>.png`
- 写 `<meta-dir>/anim_index.json`：`{source, clips: [{name, strip, frames, frame_w, frame_h}]}`
- `art-catalog` 扫描时读取 anim_index.json 并入 meta（`anim` 字段），页面在图册详情弹层播放

## 行为

- 每条 clip 均匀采样（含首帧），EEVEE 渲染、256px、与图册同光照同机位（45°/仰角 20°）
- 页面 sprite 播放器：约 8fps 循环，无任何外部库；无动图条的模型行为不变（静帧）

## 验收句

1. 对 green_blob 与 player_hunyuan 执行 anim_strip.py 后，`anim_index.json` 的 clip 集与 meta.json 的 animation_clips 名称一致，strip 文件全部存在且尺寸 = frames×256 × 256。
2. 复扫后 catalog.json 中对应资产 `meta.anim` 非空且 strip 路径可解析；页面 HTML 含 sprite 播放器标记。
3. 无 anim_index.json 的模型复扫不报错、不产生 R 系列误报（向后兼容）。

## 验收记录（2026-08-29）

覆盖全部 5 个在役模型（hero.glb / monster.glb 为孤儿待退役，未渲）：

1. ✅ green_blob 9 条（Dance/Jump/No/Yes/attack/death/hit/idle/walk）、mushnub 9 条、mushnub_evolved 9 条、yeti 9 条、player_hunyuan 5 条（attack/hit/idle/run/walk）均与 meta.json clip 名一致；抽查 walk.png=2560×256（10 帧）、death.png=2048×256（8 帧）。
2. ✅ 复扫后 catalog.json `meta.anim` 非空且路径可解析（gallery-washed/<名>/anim/…）；index.html 含 sprite 播放器（data-strip/animTick）。
3. ✅ 复扫检查数保持 28、退出码不变（R 系列零误报，向后兼容）。

## 人工验收反馈 #1（2026-08-29）：动画藏太深 + 找不到导入入口

- **动画**：原版要点开卡片才见播放。已改：①图册卡片封面直接用首条 clip 动图循环；②新增「**动画速览**」独立标签页（全部 41 条 clip 一页平铺，点击进详情）；③画廊加「只看有动画」过滤。
- **导入入口**：属设计澄清而非缺失——页面只读是红线（洗白要跑 Blender 且有人工验收点，静态页无法执行也不该绕过审批）。已把导入路径在总览页做成显眼指引面板（三步 + 「复制导入提示语模板」按钮），场景与工单页补触发语/工单字段说明。
- 状态：两项已实现并复扫验证（概览/检查数不变），待人工复验。

## 人工验收反馈 #2（2026-08-29）：参照 Mixamo 真播放 + 导入要真按钮

- **3D 动画播放器**：内嵌 three.js r147（UMD，file:// 兼容）+ GLTFLoader + OrbitControls 到 index.html；glb 按模型分包 base64 写入 `catalog/modeldata/<id>.js`（≤6MiB，超限跳过），点开模型详情懒加载 → 真 3D 视口：拖拽旋转/滚轮缩放/右键平移，clip 下拉切换、播放/暂停、0.25-2x 变速、时间轴拖动 scrub。动图条保留为「动画速览」页的轻量速览。
- **导入按钮**：总览页「⬆ 导入资产：选文件 → 写入 raw 目录」——File System Access API（Edge/Chrome）让网页把所选文件直接写进用户授权的目录；不支持时回退为「手动复制到 <raw 路径>」提示。成功后自动生成该文件的 SC1 提示语并可一键复制。
- **导入自动化分级**（L0 人工 → L1 当前 AI 辅助 → L1.5 CLI 工单 → L2 一条龙管线 → L3 远景全自动）已写入设计稿 v0.5；许可证确认/复检拍板/覆盖确认三类环节永不自动化。
- 状态：已实现并复扫（7 模型数据分包、播放器/导入按钮入页、检查数 28 不变），待人工复验。

## 人工验收反馈 #3（2026-08-29）：系统搞复杂了，聚焦导入与资产管理；部分模型动画播放不了

- **UI 收敛 8 视图 → 4 视图**：总览与导入（导入按钮+指引+健康+工单）｜资产（模型富表含实际 clips/引用/状态，可切全类型）｜图册（动图封面+详情）｜检查（报告）。流水线追踪/动画速览/绑定清单/场景工单四页并入或降级为提示行——底层数据（R1-R7、绑定、场景卡）不变，页面只呈现核心。
- **播放修复（根因找到）**：详情弹层每次重渲染会销毁 canvas，而 WebGL 渲染器仍绑在旧 canvas 上——只有当次会话第一个打开的模型能播，后续全黑屏，表象即「部分模型播放不了」。修复：3D 视口改为**持久元素**（不随详情重渲染销毁），渲染器/控制器只绑定一次。
- **资产管理补文件真相**：扫描时直接解析 glb 容器的 JSON 块（`glb.animations/skins/meshes`），资产表显示**文件实际动画**而非 meta.json 声称。实锤：7 个在役+退役模型全部含动画数据；hero.glb 与 monster.glb 各只有 **1 条匿名动画**（连名字都没有）——播放器已兼容（下拉显示「clip（未命名）」）。
- 状态：已实现并复扫验证（4 标签/持久视口/glb 字段入 catalog.json，检查数 28 不变），待人工复验。

## 人工验收反馈 #5（2026-08-29）：3D 视口模型悬空，网格穿在肚子中间

- 根因：视口把包围盒**中心**对到原点，而网格固定在 y=0 → 网格横穿模型腹部。
- 修复（对齐 Mixamo 观感）：改为**脚底落地**——水平居中、包围盒底部（box.min.y）落到 y=0；相机看向半身高（target y=h/2），机位随身高调整。动画播放中角色起跳/落地相对网格自然成立。
- 状态：已实现并复扫（数据零变化，退出码 24 不变），待人工复验。

## 人工验收反馈 #6（2026-08-29）：图册里大量「无图册」卡片是什么

- 答案：20 张全部是库域 `_art/raw/` 的**原始素材**（采购套装原件 `glTF/*.gltf` + AI 中间件 `ai/*.glb`）——渲染图册只在上架洗白时生成，raw 原件本就没有图。
- 处理：图册默认隐藏无图册原件，新增「显示无图册原件（raw）」开关；显示时卡片带「raw 原件」徽章。
- 状态：已实现并复扫，待人工复验。

## 能力卡 AC-3：上架流水线状态机（intake 子命令族）

- **接口**：`art-catalog intake [--game <dir>] create|set|list …`（`src/intake.rs`）
- **行为**：状态机 `new → washing → review → landed`（旁路 rejected 需备注；终态锁死）；create 强制 snake_case 目标名 + 许可证必填，同名冲突写 notes；扫描按 raw_file/target 把状态打到资产 `pipeline_status`；资产表管线列 + 图册 已上架/候选/raw 原件 徽章；SC1 接工单命令（6 步 / 人拍板 2）。
- **验收句**（2026-08-30 全部跑通）：
  1. `intake create` 无 --name 对 `Cat.gltf` 立案被拒（非 snake_case），退出码 2 ✓
  2. `create --name cat` 生成工单 status=new，退出码 0 ✓
  3. `set 2026-08-30-cat --status landed` 跳步翻转被拒，退出码 1 ✓
  4. `set --status washing` → `--status review` 合法推进，list 显示 status=review，复扫后 Cat.gltf 资产 `pipeline_status=review`、总览「工单开 1」✓
  5. `review → landed` 通过；`landed → rejected` 终态翻转被拒，退出码 1 ✓
  6. 删除演练工单后复扫回基线（孤儿 0、检查 24、退出码 24），`--scenario-check` 退出码 0 ✓
- **拍板记录**：2026-08-30（youxia）按「上架流水线设计」拍板动工（intake 命令族 + 页面状态列/徽章 + SC1 接线，三件全做）。
- **真实跑单（2026-08-30，youxia 点单 Mushnub.gltf 演示 + 拍板「过」后选「删演示件回基线」）**：SC1 全链真跑通——create（--height 1.113 留档）→ washing → normalize 零泄漏（1248 tri，与在役 mushnub 同参）→ turntable 4 视角 + anim_strip 9 条带 → review 复扫（图册候选卡片/待拍板徽章/3D 内嵌全出现）→ landed。**在役模型零改动**（git 佐证）。跑单揪出并修复：① 工单 target 应存仓库相对路径（原游戏相对路径导致状态打不到上架模型）；② turntable --out 应传图册父目录（SC1 卡示例同步修正）；③ Blender 相对路径解析陷阱（误写 C:\games 已清理，改绝对路径）；④ PS `Select-Object -First` 会掐断管道杀掉 Blender（改全量收输出）。演示件按拍板删除回基线（孤儿 0、问题 24）。
- 状态：已实现并全链验收 + 真实跑单演练，待人工复验（页面管线列/徽章观感）。

## 能力卡 AC-4：一键洗白 `art-catalog wash`（L2 半自动管线）

- **接口**：`art-catalog wash --file <raw> --height <米> --license <许可> [--name <snake>] [--source 来源] [--scenario SC1] [--max-tris N] [--tex-size N] [--skip-anim] [--yes] [--blender 路径]`（`src/intake.rs::wash`）
- **行为**：一条命令跑完 SC1 机械段——intake create → washing → normalize（leaked 非空即失败）→ turntable（校验 meta.json 产出）→ anim_strip（meta 声明有 clip 才跑，校验 anim_index.json）→ review → 复扫出页 → **停下等人翻图册拍板**。任何一步失败自动 `rejected --note 原因`。人工关卡不变：敲命令=批准参数①；landed 永不自动（拍板②）；覆盖在役产物必须 `--yes`。Blender 路径：`--blender` > 环境变量 `BLENDER_EXE` > 团队默认 `D:\Blender\blender.exe`。
- **页面**：导入按钮提示语升级双轨——「对 AI 说」/「终端一键 wash 命令」。
- **验收句**（2026-08-30 全部跑通）：
  1. 缺 `--license` 被拒，退出码 2，不建工单 ✓
  2. 目标为在役 `mushnub` 且无 `--yes` 被拒（覆盖冲突），退出码 2 ✓
  3. 全链 `wash`（Mushnub.gltf→mushnub_demo）：normalize 零泄漏 → turntable → 9 条带 → review → 复扫（工单开 1、模型 6、孤儿 1 预期内），退出码 0 ✓
  4. 清演示件复扫回基线 24，`--scenario-check` 退出码 0，页面双轨提示语就位 ✓
- **踩坑记录**：① 根路径 `\\?\` 前缀传给 Blender 子进程导致输出异常——main 路由处剥前缀（修在源头，输出校验的空日志假阴性由此而来）；② PS `Select-Object -First` 会提前掐管道杀掉 Blender（验收改全量收输出）。
- **拍板记录**：2026-08-30（youxia）按「AI 助手=确定性工具+编程 agent 皮」方案动工；明确不做页面内嵌 LLM、自动 landed、后台守护进程。
- 状态：已实现并全链验收，待人工复验。

## 能力卡 AC-5：引用可视化（代码 ↔ 资产 ↔ 动画）

- **接口**：页面第 5 个标签「引用」（`renderRefs`/`drawRefLines`，零依赖 SVG）；详情弹层动图条加引用徽章。
- **行为**：每个在役模型一张三列图——左=代码锚点（📄 模型字面量 file:line / ⚡ 动画索引符号=值）、中=模型节点（点开详情；孤儿红框）、右=glb 真实 clip 顺序；被引用 clip 绿色 ✓ 连线，无引用灰显（死动画一眼可见）。数据全部来自 catalog.json 现有字段（refs/anim_refs/glb.animations），**扫描侧零改动**。详情弹层动图条按名字匹配标注 ✓符号 / 无引用。
- **职责边界（验收反馈 #7 问询确认）**：资产=台账视角（状态/引用/健康/管线），图册=视觉视角（封面/动图/3D，拍板在此），引用=关系视角（代码↔资产↔动画连线）；三视图同源不同镜头。
- **验收句**（2026-08-30）：① 5 个模型各出一组三列图；② player_hunyuan 5/5 clip 绿显（HERO_CLIP_* 五符号连线）；③ green_blob/mushnub/yeti/mushnub_evolved 各 3/9 绿显、6 条灰显（walk/attack/hit 死重立现）；④ 详情动图条带 ✓符号；⑤ 复扫基线 24 不变、退出码同数 ✓
- **拍板记录**：2026-08-30（youxia）按「三列图 + clip 徽章双做」动工。
- 状态：已实现，待人工复验（观感）。

### AC-5 强化（2026-08-30，用户游戏内观察驱动）：索引↔名字↔时长 真相校验（R8）

- **动机**：youxia 真机观察到「玩家跑步用的是 walk clip」，问 AI 答案是 run（HERO_CLIP_RUN=3）——第一版可视化只画了「代码声称」的连线，没有独立验证「文件索引 3 处真的是 run 吗」。可视化必须能裁决代码与文件谁在撒谎。
- **实现**：① glb 解析加每条动画**时长指纹**（glTF input accessor 的 max[0]，纯 JSON 解析零缓冲解码）；② 新检查 **R8**（error）：`clip_token(符号)`（HERO_CLIP_RUN→run / walk_clip→walk）与 `glb.animations[N]` 名字（非匿名时）比对，不符或索引超界即报「索引-名字错位」；③ 引用视图 clip 芯片加时长 + ⚠≠期望名 红标，符号芯片连坐变红，模型节点挂「N 处错位」徽章；详情弹层动画引用行带时长与错位标注。
- **裁决结果（player_hunyuan.glb 文件真相）**：`["attack","hit","idle","run","walk"]`，时长 `[4.67, 1.50, 1.79, 0.83, 1.38]`s——**索引 3=run(0.83s)、4=walk(1.38s)，与卡 25 离线验证一致，R8 未命中，代码↔文件映射正确**。youxia 观察的「跑步像 walk」与资产引用无关：按卡 26 设计，基础移速 2.5（<3.0 门控）本来就走 walk clip，Shift 疾跑才切 run——若疾跑仍像 walk，属卡 26 门控/手感问题，应另立游戏侧卡核查，与资产无关。
- 基线不变（R8 零误报：怪物定义表 7/0/3 与文件名全部吻合）。

## 人工验收反馈 #4（2026-08-29）：封面用原图 + 动画引用展示 + 孤儿处置问询

- **封面回退原图**：图册卡片封面不再用动图条，改回静态转台渲染图（renders[0]）；▶N 动画徽章保留，「只看有动画」过滤保留，动图条与 3D 播放仍在详情弹层。
- **动画引用展示**：新增扫描启发式（零依赖、无 regex crate）——代码以**索引**引用动画：hero 侧 `HERO_CLIP_*` 常量（绑 HERO_GLB→player_hunyuan），怪物侧定义表 `walk_clip()/attack_clip()/hit_clip()` 函数（应用到 model() 列出的全部模型）。每条解析出 clip_index→按 glb 实际动画顺序还原成名字，详情弹层显示「walk ← walk_clip (components.rs:97)」；索引越界会显式标注「超出文件动画数！」。实扫 17 条（hero 5 + 四怪物×3），小数常量（如 WALK_CLIP_AUTHORED_SPEED=1.4）误报已排除。
- **孤儿定义确认**：孤儿 = 保存在游戏 assets/ 内、在 src/+tests/ 的 .rs 代码中零字面量引用（`//` 注释提及不算引用）的资产（kind≠other）。当前孤儿 = hero.glb（被 player_hunyuan 替代，卡 21）+ monster.glb（被定义表四皮替代，卡 19）。处置：工具只读红线不删文件；人批准后可删（git rm 由人执行或 AI 执行后随提交入库，git 历史可找回）——处置方式由人拍板，见当次验收记录。
- **处置执行（2026-08-29，youxia 拍板「两个都删」）**：AI 删除两文件并复扫——游戏模型 7→5、孤儿 2→0、检查问题 28→24（消 2×R1+2×R4）、modeldata 分包自动清理至 5 个（modeldata 目录定为纯派生产物，每扫重建）。待办：删除随下次提交入库（git 状态会显示两个 D）；真机可选跑一局确认无感知。
