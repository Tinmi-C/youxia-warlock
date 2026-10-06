# AGENTS.md — Warden 项目上下文（AI 协作约定）

> 本文件给「在这个游戏项目里工作的 AI」看。全局规则见根目录 `/AGENTS.md`；知识库规范见 `docs/AGENTS.md`。
> 本项目的核心工作方式：**能力卡驱动 + 验收闭环 + 观察通道**（即团队 ADR-0002 的「AI 装配层」在项目层的落地，详见 `docs/capability-cards.md`）。

## 项目是什么

Warden（塔防）：每局随机给塔、塔靠多来源获取、可融合进化的塔防。玩家在波次间（慢思考）经营资源——买塔/融合/摆塔/三选一，
波次中（快反应）塔自动开火阻挡敌军沿 L 形路径抵达终点；漏怪扣基地血，归零失败；清了全部波次通关，死后 Meta 解锁保留。
**核心节奏**：波次间开放经营菜单、波次进行中锁建造（双速节奏）。桌面 3D 游戏。
设计规则/数值唯一依据 `docs/requirements.md`；产品总览见 `docs/GDD.md`；实现卡见 `docs/capability-cards.md`。

## 技术栈

- 引擎底座：**Bevy 0.19.1**（锁版本，升级先读 release notes，见 `Cargo.toml` 注释）
- 语言：Rust stable，edition 2021
- 依赖引擎：无（不手写渲染/ECS——那是 ADR-0004 已退役的路线；后续按「客观→插件」引入，如 bevy_rapier3d / bevy_egui）
- 代码组织：lib + bin 分离（`src/lib.rs` 的 `build_app()` 供 main 与测试共用）、领域插件化（`src/plugins/`）

## 目录

```
src/main.rs          入口（只调 build_app()）
src/lib.rs           build_app()：App 组装（按领域挂插件）
src/states.rs        GameState 状态机（Playing / Paused / GameOver）
src/components.rs    组件（纯数据；塔/敌/弹等按卡补充）
src/resources.rs     资源（经济/波次/平衡表等单例）
src/sets.rs           GameSet：Update 编排阶段（SystemSet，卡片挂载点）
src/plugins/*.rs     领域插件（一个领域一个 Plugin：game / map / towers / enemies / waves / economy / ui / debug / meta(计划新增)）
src/systems/*.rs     系统（按系统分文件：camera / cursor 等）
tests/behavior.rs    行为一致性回归测试
docs/requirements.md   设计层源文件（规则/数值唯一依据，勿改数值结构）
docs/capability-cards.md  能力卡工作流 + 卡路书（实现拆卡）
docs/GDD.md               产品总览（定位/循环/范围/验收信号/垂直切片）
assets/{models,textures,audio,fonts,ui}/
```

## AI 协作工作流（必须遵守）

1. **立卡**：做任何新功能，先写能力卡（`docs/capability-cards.md`）：接口 / 行为 / **验收句**。验收句必须数字化、可执行（例：「以 60fps 与 144fps 各运行 1 秒，移动距离都应等于 speed×1 秒，误差 <1%」）。**验收句写不出来的功能，说明还没理解清楚——先问人，不要开写。**
2. **AI 实现**：按卡实现，遵循卡上的接口/行为；老系统零改动（新机制 = 新组件 + 新系统）。
3. **人验收**：人按验收句验收（跑游戏看效果 + `cargo test`）。
4. **回归钉死**：验收句转成测试加进 `tests/behavior.rs`（「改了 A，B 没坏」的回归断言）。
5. **一卡一提交**：Conventional Commits（`feat:` / `fix:` / `docs:` / `refactor:`），提交信息英文，一个功能一个提交，可回滚。

## 硬性规则

- **先找插件，不重复造**：新功能先判断「正确性是客观还是主观」——客观技术能力（物理/碰撞/寻路/缓动/网络）优先引用生态插件（查版本对齐 0.19 + 维护度 + 作者可信），不要自己从零写；主观玩法逻辑（手感/节奏/数值）才自己实现并按能力卡流程沉淀。
- **不编造 Bevy API**：不确定的 API 先查 Bevy 0.19.1 官方示例/文档（`https://github.com/bevyengine/bevy/tree/v0.19.1/examples`），或明确说「不确定」并给出备选，不要猜。
- **代码注释用英文**；与人的对话、文档用中文。
- **观察通道**：行为变化必须可被日志/测试观察——系统里打关键日志（`info!`），验收靠 `cargo test` + 日志仪表，不靠「看起来对了」。
- **改动可回滚**：不在一行里塞多个无关改动；结构改动先立 ADR（团队 `docs/decisions/`）。
- **资产约定**：模型/贴图等放 `assets/` 对应子目录（`models/textures/audio/fonts/ui`）；glTF 优先（Bevy 原生）；JPEG 贴图已开特性，无需处理；模型文件用 snake_case 语义名（如 `cannon.glb`）；动画 clip 统一命名 `idle`/`walk`/`attack`/`hit`/`death`；美术原始素材、候选图册放 `_art/`（`raw/gallery` 等，不入运行时）。来自岩库（ArtVault 中台）的资产走 `art-assets.json` + `tools/art-sync/art_sync.py`（清单锁版本，见 ADR-0009），不得手拷。
- **当前状态 / 下一步**：每次开工前更新本文件的「当前状态」小节（进行中的能力卡、已知问题）。

## 跨游戏约定（新游戏开工前看）

这些是团队沉淀的**可迁移**原则/决策，新游戏继承即可，不用重新摸索：

- **数值数据暂留代码**：游戏数值（定义表 / Balance 默认值 / 波次公式 / 锚点带）放 Rust 源码，**不外部化到数据文件或数据库**。注意「数据驱动 = 数据与逻辑分离」，≠ 外部化；运行时用调试面板（F1 类）热调内存值即可。迁移门槛（何时才考虑外部化）见团队知识库 **ADR-0008**。
- **数值平衡用「设计目标锚点框架」**：先定目标体验 → 反推数值 → 用 5 字段锚点（类别/指标/目标区间/来源/验收方式）把体感难易变成可测指标 → 配一个 headless 读数工具给 AI 照着收敛。方法见知识库 `docs/topics/game-design/design-target-anchors.md`。
- **表现层别做 god-file**：一个插件文件别随着卡越堆越大；按领域拆成子插件，用 `SystemSet` 定跨插件顺序。**"一个领域一个 Plugin"** 对表现层同样适用。
- **客观技术→插件、主观玩法→自研**、**玩法机制第 2 次用到才抽进 `engine/`**：见本文件「硬性规则」+ 团队知识库 `docs/topics/engine/bevy-plugin-and-code-reuse.md`。

## 当前状态 / 下一步

> 状态索引而已——逐卡规格/验收句/反馈记录只在 `docs/capability-cards.md` 一处维护，本节不重复细节。

- **已完成（脚手架 + 灰盒 MVP 全量，2026-08-25 立项 → 2026-09-04）**：
  - 脚手架：从 `templates/bevy-game/` 复制改名 crate `warden`，lib+bin、领域插件、`GameSet` 编排。
  - 需求落档：`docs/requirements.md`（设计源）/ `GDD.md` / `capability-cards.md`（卡路书）。
  - 首条可玩闭合 ★（MA1/2、EN1-3、TO1-4、WA1-3、EC1、ST1、UI4）+ UI1 鼠标商店/点选融合 + TO5 融合（DPS 已按 §7.2 校准）
    + TO6 减速 + EN4 物理护甲 + AC4 三选一（占位确定性）+ EN5 治疗怪（半径6/每秒3，起点值）。
  - 塔获取 5 来源全通（requirements §8）：AC1 开局手牌（随机 2 塔，保底输出）→ AC2 商店保底刷新（含 bevy_ui 购买行）
    → AC3 掉落塔牌（精英/BOSS 必掉、普通 10%）→ AC4 三选一 → TO5 融合。
  - ME1 Meta 解锁链：死亡+15/通关+30 币，升级1=开局必含弓箭手（30 币）、升级2=开局+20 金（60 币），
    key=value 文本存档 `meta_save.txt`，GameOver/Win 屏 1/2 键购买。
  - §9 移速锚点已校准：speed 1.0 ≈ 20 秒走完 43 单位 L 路径（从实际路径动态推导）。
  - UI2 操作面板化+中文化（2026-09-06）：「▶ 开始下一波」按钮、三选一可点击卡片（与数字键等效）、
    商店卡片缺钱置灰、点已放塔出属性面板+射程圈、黄色键盘光标默认隐藏；UI 文案默认中文——
    塔名中文 label 字段 + `bevy_text/system_font_discovery` 运行时用系统微软雅黑（不带字体文件）。
  - AC4 polish（2026-09-06）：三选一改 `RunRng` **真随机**（伤害/攻速/射程/金币/获得塔 5 类抽 3 个不重复）；
    `StatKind` 三态 + `Boosts` 增 `attack_speed_mult`/`range_mult`；tower_fire 对攻速(冷却)与射程(瞄准)生效；
    **融合塔也按 `tower_index`(=原料0) 吃加成**。
  - BAL1 balance 读数工具（2026-09-06）：`tests/balance.rs` headless 自动打局，输出每波清理秒数/漏怪/金币/基地血量
    （`cargo test --test balance -- --nocapture`）；首批读数暴露第 3 波是难度尖峰（常规阵容漏 4）。
  - BAL2 balance 锚点表（2026-09-06，已起草待确认）：把 §15 定性信号转成可测指标（存活波数/漏怪/余钱/三选一价值/融合 DPS）；
    全波读数**暴露早战经济压力**——自动布阵从 100g 起步第 2 波即败（基地 0/10），是 balance 卡首要收敛项。
  - UI3 塔位悬停高亮+幽灵预览（2026-09-06）：建造模式悬停空塔位显示半透明幽灵塔（颜色随塔型），放哪放什么一目了然。
  - UI5 结算画面（2026-09-06）：GameOver/Win 显示「本局结束」横幅 + 再来一局按钮 + 两个 Meta 购买按钮（与 1/2 键等效）。
  - UI6 自带开源中文字体（2026-09-11）：`assets/fonts/` 内置 Noto Sans SC（SIL OFL 1.1）+ `UiFont` 资源（headless 回落默认字体），
    移除 `bevy_text/system_font_discovery`——不再依赖系统微软雅黑/苹方，跨平台渲染一致、分发无版权问题
    （**开发机已迁到 macOS arm64**，2026-09-11 实测 cargo run/中文渲染正常；raw.githubusercontent 直连下载易断流，走 jsdelivr CDN）。
  - UI7 语言切换 中/英（2026-09-11）：`L` 键/右上按钮切换，全部玩家文案走 `systems/l10n.rs`（`Bi(zh,en)` 双语常量 + 按语言 format 辅助），
    商店/控制条/三选一/属性面板/结算屏随语言即时重建；偏好持久化 `ui_prefs.txt`（key=value，测试用独立临时路径）。
  - UI8 下一波组成预览（2026-09-11）：建造期 HUD 显示下一波敌人构成与奖励（如「下一波：普通×10 快速×3 · 奖励30金」），
    服务 §3「慢思考」；战斗期隐藏；EnemyDef 增双语 `label`。
  - **交互范式重做（2026-09-12，需求会话定稿 → 同日实现）**：War3 式选中态命令卡（requirements §16）——
    常驻三区底栏（左·信息 / 中·命令卡 / 右·控制），命令卡五态随 `Selection` 状态机切换（待机/空塔位/已放塔/融合目标选择/战斗期置灰）；
    UI9 骨架+Esc 统一取消、UI10 建造双入口（点卡→点位 / 点位→点卡，悬停按钮实时幽灵塔+射程圈）、
    UI11+TO7 卖塔（投入×70% 向下取整，`Tower.invested` 追踪）+融合三步链路（结果落 B 位、可配对塔高亮、点错不误融）、
    UI12 Tooltip 全信息（中英）+ Grid 热键 QWER/ASDF（角标；热键注入 Pressed 复用点击处理器）。
    退役：旧底部商店栏、右上信息面板、WASD 光标（cursor.rs/PlacementCursor）、1-4/E/F 键盘流、FusionSel；断言随卡迁移。
  - **真投射物 + 战斗表现（2026-09-12，需求会话批次二定稿 → 同日实现）**：requirements §5.1 攻击结算模型——
    远程塔（弓/法/炮/全部融合塔）发**追踪弹**（Projectile 规则层实体：锁定目标逐帧追当前位置、必中、弹速 10、
    伤害发射时快照/护甲命中时结算），盾兵塔近战即时命中（混合模型）；**TO6 减速重做**为命中附加（数值 30%/1s 不变）；
    PR2 边缘行为：单体弹目标飞行中死亡→消散无伤（overkill 是真实成本）、AOE 弹最后位置照常爆炸、波末清弹；
    FX1 血条（受伤才显示、比例宽度、精英/BOSS 金色加宽）、FX2 受击闪白/AOE 扩散圈/死亡消散壳（金币与 despawn 时序零改动）、
    FX3 减速变蓝/盾兵怪盾标识/治疗怪保持绿色。新域插件 `plugins/{projectiles,fx}.rs`；`ProjectileImpact` 消息为 FX 观察缝。
    **BAL1 基线已重取**：读数口径变化（飞行时间+overkill 降低有效 DPS）属预期；贪心布阵第 2 波 GameOver 维持（数值仍按负责人意见暂缓）。
  - **UI 点击泄漏修复（2026-09-12 真人试玩发现，同日修复）**：点命令按钮的同一次左键也进入世界拾取，
    `select_object` 的「点空地取消」同帧清空 Selection——融合永远无法从底栏进入（armed 建造同源：点底栏
    在光标最近塔位误放塔）。修法：底栏/三选一/结算屏根节点挂 `BlocksWorldInput`+`RelativeCursorPosition`，
    `select_object`/`mouse_place_system` 入口吞掉悬停 UI 的点击；新增 live 光标路径回归 4 条（headless
    真 Window+相机投影，详见 capability-cards UI9 卡）。测试补遗：UI10 待机态点建造按钮 armed、UI11 无可配对
    融合按钮置灰两条验收句钉死。
  - **DB1 调试面板 + 作弊核心（2026-09-12）**：F1 开关 bevy_egui 0.42 面板（版本依据其 README 兼容表 Bevy 0.19 ↔ 0.40-0.42），
    面板按钮只推 `DebugAction` 意图、`systems/debug.rs`（全模块 cfg 门控）单一 executor 统一生效——headless 测试推同一队列；
    金币/基地血/跳波（中间奖励补发）/清波/召唤/解锁/塞塔/立即败胜/种子重开全量；**调试局存档隔离**：用过任何作弊即
    `DebugRun.marked` → 结算不发 Meta 币、`meta_save.txt` 一字不动；F12 截图/日志仪表同门控收口；
    release 零调试面 = 编译级 cfg 抽除 + `scripts/check_release_debug_surface.sh` 二进制字符串扫描（实测通过）。
    `DebugPanelCapture` 吞面板悬停点击/键盘，防泄漏进世界拾取与热键（复用 UI 点击泄漏修法判定口）。
  - **DB3 时间控制 + 信息叠层（2026-09-14）**：`systems/debug_time.rs`（DB1 同一 cfg 门控）。调试暂停走**时间级**
    （冻结 `Time<Virtual>`，系统照跑 delta=0），与状态级 GameState::Paused 正交可叠加（卡上注明取舍）；
    单帧/0.1s 步进=下一帧恰好放行 1 帧/0.1s 后复冻（bevy 在 First 推虚拟钟，clock 系统写下一帧生效）；
    热键 F6/F7/F8/F9 与 F1 面板推同一 `DebugAction`。叠层：全塔射程圈开关（缓存塔集重建）、敌头顶
    Text2d hp/移速/减速面板、右上 `[db3]` HUD 条（弹体/敌数）。时间/叠层动作**不标记调试局**（纯观察，
    `is_observation()` 白名单）；release 扫描脚本已加 `[db3]` 串。`debug_db3::*` 6 条断言入 behavior。
  - **DB2 测试场景直达（2026-09-12）**：`--scenario NAME --seed N` CLI（main.rs cfg 门控，seed 先于 Startup 换 RunRng）
    与 F1 面板按钮**双入口同队列**（`DebugScenarioQueue`，排他执行器首帧落地）；六场景 fusion/projectile/fx/meta/
    choice/full-board 一键到位（choice 复用 wave.rs 真三选一生成器）；场景直达即标记调试局（存档隔离生效）。
  - **SK1 技能骨架（2026-09-14）**：新域 `plugins/skills.rs`+`systems/skills.rs`（`SkillState`/`SkillTargeting`/
    `SKILL_COOLDOWNS` 45/40/60 在 resources.rs）。CD 虚拟时间连续（跨波不重置、开局就绪、暂停冻结）；控制区三按钮
    （Z/X/C 注入 Interaction 复用点击）+CD 置灰/剩余秒数+tooltip（refresh_tooltip 扩两源缓存）；点地技能指示圈+左键确认+
    Esc/右键取消（cancel_modes 扩清、select_object 让位）；未实装置灰（SK2-4 逐张点亮）；F1 加「Reset skill CDs」
    （作弊，标记调试局）。PR1 存量测试 gap 查询收紧 With<Enemy>（SK 按钮 Transform 节点踩中任意首实体，语义不变）。
  - **SK2 流星火雨（2026-09-14）**：`Projectile.target` → `origin: ProjectileOrigin{Homing 塔/Fixed 技能}`（SK2 规格
    允许的投射物接缝，存量塔弹/PR 测试同步迁移语义不变）；`MeteorVolley` 调度实体（3 发×0.3s 间隔，落点上方 10 单位
    坠落≈1s）+ `release_meteor_shots`，每发 `Projectile{Fixed, 40 魔法, 半径 3}` 走既有 `resolve_impact`（穿甲/金币/
    FX 爆圈全复用）；三发同落点（最简解释）。
  - **SK3 冰霜新星（2026-09-14）**：点确认 XZ 半径 5 即时挂 Slow{0.5, 2s}。**减速共存语义下沉共享路径**：新
    `tower::ApplySlow` Command（factor=min 取更强、timer=新应用时长），壁垒炮弹/盾塔近战/新星三入口统一；壁垒炮行为
    由「覆盖」变为「取更强者刷新」（SK3 规格要求的语义变化）。
  - **SK4 战意狂暴（2026-09-14）**：`WarFrenzy{remaining}` 全局资源（1.5×/3s 常量入 resources.rs）；`tower_fire` 开火
    时刻读全局态追加乘数 `1/(攻速×Boosts×1.5)`（与 Boosts 管线同构，快照=开火瞬间；持续中新建塔即读即享）；瞬发走 SK1
    预留 Instant 分支（C 键即触发）。三技能全点亮，SK1 未实装门控断言改标冷却门控。
  - `cargo check --all-targets` + `cargo test` 全绿（95 条断言，`tests/behavior.rs` 93 + `tests/balance.rs` 2）。
- **交互范式 + 投射物/战斗表现两批需求（2026-09-12 定稿）已同日实现**（见上方「已完成」两条实现记录）；
  规格存档：requirements §16（命令卡交互规范）/ §5.1（攻击结算模型，含**拍板不上物理插件**）；
  「伤害数字飘字」仍留 GDD 点子池。
- **技能系统一期 SK1-SK4 已全量实现（2026-09-14 定稿 → 同日实现，见「已完成」SK1-SK4 四条）**：
  requirements §17（纯冷却免费、CD 真实时间跨波连续、开局即就绪；伤害贡献锚点 ≤全场输出 20%，BAL2 已加行；
  控制区按钮 + Z/X/C 热键，点地技能走融合目标选择同款交互范式）。技能二/三期仍暂缓（见 GDD 点子池）。
  **BAL1 已于 SK 批后重取**：自动布阵不施放技能，读数与 SK 前基线同形——第 1 波 ≈21.5s/漏 4，第 2 波 GameOver
  （基地 0/10，起手 100g 贪心布阵的既有失败点维持）；技能对自动读数零影响，锚点无需重定。
- **ART2 修复 + 美术测试金字塔落地（2026-09-18 测试会话）**：发现 ART2 提交 `22f8de3` 只含常量+组件、
  **挂载 spawn 代码未入库**（原「视觉验收通过」对 HEAD 不成立，已作废）；按交接参数补回
  `tower.rs` 挂载（`BallistaMount` 独立子实体 y=+1.3/scale 0.72）；新增美术资产测试三层——
  L1/L2 结构层 `tests/art_assets.rs`（离线 GLB 解析：命名/15MiB/清单双向对账/包围盒归一/网格贴图动画计数/弩长轴 ±X）、
  L3 装配层 `archer_tower_assembly_is_pinned`（headless 真 AssetServer，红绿验证过）；
  **视觉验收重做通过（第三次：激活窗口前台+中性提问+日志地面真相交叉核对；前两次教训——遮挡窗口
  macOS 停绘截出黑图、引导性提问让视觉模型对黑图幻觉出塔弩被用户戳穿，纪律已沉淀知识库）**；
  截图存 `docs/img/warden_fusion_art2.png`；期间清掉 ART 会话遗留的旧进程；
  `cargo test` 全绿 **102 条**（behavior 97 + art_assets 3 + balance 2）。
- **ART3 战场地表与路径铺装（2026-09-20，方向 B「黄昏荒隘」拍板后首卡）**：
  美术方向三选一（A 晴昼草野/B 黄昏荒隘/C 夜幕边境，Blender 真坐标方向稿）定 **B**——方向基线唯一依据
  `docs/art-direction.md`（色板/光照锚点/可读性铁律），定稿图 `docs/img/warden_map_style_B_dusk.png`。
  战场从「悬浮灰条」变完整地形：PolyHaven CC0 贴图（`brown_mud_rocks_01` 地表 + `cobblestone_02` 石板路，
  1k diffuse+GL 法线，岩库 v1 锁定，art-sync 落盘 ~4.2MB；2k 评估后弃用——俯视+平铺下收益小体积 3 倍）
  + 悬崖裙边体块 + 路径分段铺装（~2 单位/砖、取整对缝）+ 塔位垫/基地块 B 色板校色；headless 回落 B 色板纯色，
  游戏逻辑零改动（拾取走投影不吃地形 mesh）。带 AssetServer 的测试世界需补注册 `init_asset::<Image>`
  （assembly 测试同步补）。`cargo test` 全绿 **106 条**（behavior 100 + art_assets 4 + balance 2；新增
  `map_ground_spawns_headless` / `map_ground_texture_branch_is_pinned` / `textures_dir_conventions_and_manifest_agree`）。
  视觉验收通过（**重做后**：首张截图裁剪错位+半引导性提问让视觉模型幻觉，被用户肉眼戳穿作废——
  黑图坑第三次重演，教训同 ART2；正姿：`osascript` System Events AXRaise 置前 → `screencapture -l <wid>`
  窗口直截 → 真中性逐字读 UI + dash 独立对账：金币0/基地10/10/波次1建造期/下一波普通×5 全部吻合；
  截图 `docs/img/warden_art3_terrain.png`）。ART4 基地入口/ART5 点缀/ART6 光照卡已立（📝），方向稿
  `.blend` 存 `_art/map-style/`。**待负责人试玩验收后一卡一提交。**
- **ART5 荒原点缀 + ART6 黄昏光照（2026-09-20，同日实现+验收）**：点缀四件套 = PolyHaven CC0
  （箭袋树枯干/巨石/树桩/干枝丛；`pack_gltf_folder.py` 打包单文件 glb+原点归一，岩库 v1 锁定 ~16MB），
  `spawn_props` 布局表 12 实例（4/3/2/3，沿用方向稿位置、核算过与路径/塔位/基地安全间距），headless 回落
  B 色板色块；**不走图生 3D**（CC0 直采无参照图门；横木版 dead_tree_trunk 实测 3m 躺姿弃用，选立姿
  dead_quiver_trunk）。光照按 art-direction.md 锚点：暖橙主光 9000lux 俯仰 -0.32rad（18° 长影）+ 
  `GlobalAmbientLight` 冷紫 40 + 暮色 ClearColor（bevy 0.19 环境光资源已改名 GlobalAmbientLight，实测）。
  assembly 测试 WorldAssetRoot 计数加 `Without<MapProp>`。**`cargo test` 全绿 112 条**（behavior 106 +
  art_assets 4 + balance 2，含弩线 ART7/8 测试）。视觉验收一次通过（配方：computer-use activate →
  list_windows 拿 wid → screencapture -l；逐字读 UI 与 dash 对账 + 四类道具计数吻合；「白塔」误读经
  gold=100 未购塔排除）；截图 `docs/img/warden_art5_props_dusk.png`。**地形线仅剩 ART4**（基地石门楼+
  入口拱门，需参照图过阶段 0 门，图生 3D 走美术管线）。**待负责人试玩验收后提交。**
- **ART4 基地石门楼 + 入口拱（2026-09-20，地形线四卡收官 ART3/4/5/6 全落地）**：**路线改道记录**——
  Sketchfab CC-BY 扫描件短名单（11 件入岩库 wip 后转 archived 留档）被负责人否决（授权/重打包复杂），
  改 Blender 体块砌筑 + CC0 贴图（castle_wall_varriation 1k）：门楼双塔+过梁门洞+堆堞+旗杆旗、
  入口拱双柱+楣石+残齿，182 面/3.2MB，岩库 v3 锁定。`spawn_structures`：门楼居中基地 waypoint（敌人
  走进门洞）、拱转 90° 骑跨路径起点（入口 waypoint 是 (-13,**+9**)，别再记成 -9）。**v1/v2 三连翻车
  教训**（已入卡）：建模高度误入 Blender Y 轴致模型横躺，渲染预览被视觉模型误读"比例协调"——**数值
  bbox 门必须先于看图**；`matrix_world` 是上次求值快照，校验前必须 `view_layer.update()`。测试新增
  `map_structures_spawn_headless` + `structures_geometry_facts`（新增 scene_bbox 跨 mesh 聚合）。
  **`cargo test` 全绿 119 条**（behavior 112 + art_assets 5 + balance 2）。视觉验收通过（门楼在道路
  终点右下/拱在起点左上/UI 逐字对账/四类道具清点，截图 `docs/img/warden_art4_gatehouse.png`）。
  **待负责人试玩验收后提交。**
- **ART7 弩瞄准追踪 + 炮口出弹（2026-09-20，弩射击动作线首卡）**：新通用组件 `AimTracker`
  （挂 `BallistaMount`；角度用度、朝向真值在 Transform、组件只存配置+目标态）+ 域桥 `tower_aim_bridge`
  （Tower.target→挂件，转向系统零塔依赖）+ `tick_aim_trackers`（yaw-only 240°/s 步进、失联 2s 回正）；
  `tower_fire` 补落盘 `tower.target`（开火决策零改动），出弹点=挂件世界位+前向×`muzzle_offset`（TowerDef 新字段 0.5）。
  **关键取舍**：无挂件塔出生点维持锚点+0.4Y 原式 → PR1 等 106 条既有断言零扰动；前向符号取 +X
  （离线顶点分析：最高簇在 +X 半侧）常量+测试钉死，**待人工视觉终裁**。测试 harness `ballista_app`
  （真 AssetServer+TransformPlugin 走 spawn_base_tower 网关）+ 2 断言（收敛/保持/回正 ±5°、出生点投影 ±0.05+符号钉死），
  红绿验证过；`cargo test` 全绿 108 条（behavior 102 + art_assets 4 + balance 2）。
- **ART8 弩击发后坐（2026-09-20，ART7 同日续卡）**：新通用组件 `FireRecoil`（挂件自配 depth 0.12/回复 0.18s
  进 TowerDef + 静止位快照 + elapsed 运行态）+ 开火时刻缝消息 `TowerFired`（tower_fire 每发一条，ART10 弦箭
  动画同缝复用）+ `tick_fire_recoil`（tower_fire 后同帧：击发帧出位移、线性回落、重触发归零重置防叠加）。
  取舍：后坐只写 translation、朝向归 AimTracker 的 rotation（chain 消解写冲突→后坐不干扰转向）；击发帧
  先出弹后踢。断言 2 条入 behavior（击发/回落<10%/重置/狂暴峰值/连发下朝向 ±5°、无火无位移），重置语义
  红绿验证过；`cargo test` 全绿 **110 条**（behavior 104 + art_assets 4 + balance 2）。
- **SM1 展示场：中性影棚查看器 v2（2026-09-20，验收通道线首卡；v1 同日重做）**：v1（战场展示台）被负责人
  实机否决（形态不像编辑器、黄昏调污染观感）→ v2 = **中性影棚 + egui 资产列表 + 环绕相机**。`--scenario
  showroom` 进场即清场重搭：despawn 全部 Mesh3d（战场/路径/塔位，HoverGhost 只隐藏）+ 方向光、隐藏全部 UI 根
  Node、落浅灰地面+深灰展示台+白主光+中性环境光+淡灰清屏，切 **新 `GameState::Showroom`**（玩法/UI 系统全冻结，
  P 无效，离开=重启）。浏览双通道同一游标：左面板点选 或 Tab 切类（塔含融合/怪）、←/→ 切件，单一 present 换展；
  左键拖拽环绕+滚轮缩放（悬停 egui 时失效）。展品仍走外观缝（attach_tower_appearance/fused_tower_color/
  enemy_base_color）零规则组件；铭牌 22px。headless 断言 3 条（影棚落地+零干扰 / 全量遍历 / 面板等效+轨道）；
  **加新 def 自动进列表零改动**。踩坑记档：egui 0.36 无 SidePanel（用锚定 Window）、fork Event→Message
  （MouseWheel 走 MessageReader+显式 add_message）、Parent→ChildOf。视觉自检通过（影棚灰白无橙调/左面板
  列表+数值可读/无 HUD 底栏/铭牌逐字）。v1 提交 `9a6705c`、v2+v2.1 提交 `f68d1fe`；**⚠️ 方向 B 黄昏调同日被
  负责人否决、全游戏重定调中（art-direction.md 候选 D1-D3 待拍板）**。
  **v2.2 通用化（同日，负责人定方向"跨游戏可用"）→ ADR-0010**：机制全量上收为共享 crate
  `engine/showroom-viewer`（引擎 crate 首例——判据成立：warden 适配层 + crate 自带 `asset-viewer` bin
  双消费方；bin 用法 `cargo run -p showroom-viewer --bin asset-viewer -- <目录>` 扫看任意 GLB，对
  warden/assets/models 实测 14 文件）。warden 侧瘦成 `systems/showroom.rs` 适配层（build_catalog 读定义表
  +闭包走外观缝、幽灵 ShowroomKept 豁免、切 GameState::Showroom）；测试路径靠 re-export 零改（4 条断言
  +全量 120 绿）。依赖声明无条件（同 bevy_egui 先例）代码层 debug 门控。**新游戏接入 ≈ 写一个目录构建函数。**
  另：SM1 v1 提交曾被 Mimosa 预提交门控拦截——art_sync.py 加固触发 CWE-22 静态规则（只认内置 `open()` sink）；
  修法 `Path(x).open("wb")` 已提交 `c1d2a50`。
- **SM2 动画触发 + D1 晴昼要塞定调（2026-09-20，SM 线收官）**：① SM2（引擎 crate 内）——`ShowroomItem.gltf`
  句柄 + `showroom_animate`：自动循环名字含 idle 的 clip、Space 换 clip、面板 Clips 列表点选；接线走 ART11
  验证姿势（驱动场景内加载器播放器，绝不外挂——T-pose 教训）；warden 敌人条目从灰盒升级 ART11 真模型
  （ENEMY_MODELS/SCALES 同表 + clip 浏览），bin 条目带 gltf 句柄。真机验证：KayKit 骑士自动
  Jump_Idle 自然姿势（读图非 T-pose）、22 clips 列表可读。② **美术重定调拍板 D1 晴昼要塞**（负责人委托 AI
  推荐项）：主光白 (1.0,0.96,0.88) 高角度 pitch -0.84/14000lux、环境光白蓝 (0.60,0.65,0.75)×90、清屏淡蓝
  (0.55,0.65,0.80)——`camera.rs`/`game.rs` 已落地，钉子测试 `daylight_lighting_is_pinned`（原 dusk 钉子随调
  更新）；`art-direction.md` 基线整体重写为 D1（B 存档「已退役方向」，教训：**方向稿必须实机光照参数渲染验证**）；
  结构体/塔位垫 D1 重校（白灰石目标值已列）留后续 ART 卡。真机读图：白昼感无橙偏、路径亮可读、短阴影、
  HUD 正常。
- **展示场 v3 全量落地（2026-09-20，P1+P2+P3 一气呵成，需求文档 showroom-v3-proposal.md 拍板后实现）**：
  ① **P1（SM5+SM6，引擎 crate）**——面板 v2 成工具唯一完整操作入口（搜索/模式/时间控制/折叠分类/资产卡/
  clip 浏览），播放控制写 `Time<Virtual>`（暂停/单帧/0.25-2x，动画+转台一致冻结；**DB3 让位规则**：影棚活跃
  时 DB3 不再写虚拟钟）；搜索 = 名称+标签子串（聚焦自动跳首个命中、画廊平铺命中集）；数据模型升级
  info_rows/tags/origin。② **P2（SM7+SM8）**——来源呈现：适配层解析 art-assets.json，弩塔组合两行来源
  （塔身/床弩），灰盒标游戏本地；组合拆解：面板点部件独显（showroom_parts 按出生序配对）；对比模式：
  A=current→点 B 同台（跨版本对比待 art-sync 双版本支持，留 tools/ 线）。③ **P3（SM3R 靶场，warden 侧）**——
  `--scenario range`：真塔+1-6 只站桩木桩（RangeDummy 免寻路）+ Dota 式账本（total/dps/hits/last，轮询真
  hp 不复制逻辑）+ 右侧面板（选型/数量/Reset），死亡走真结算原位补员。测试 **126 条全绿**（showroom 8 +
  range 2）；提交 de117e9/e37a9b4/c88c009。已知遗留：跨版本对比（art-sync 双版本）、N5-③ 装配调参、
  SM4 截图基线、观察辅助（线框/骨骼）——均留卡。靶场视觉面板截图因宿主屏幕权限受阻，以 4 条断言 + 实机
  dash（金币随击杀上涨）为证。
- **SM 画廊模式 v2.3（2026-09-20，用户实机反馈「只能看一个」）**：引擎 crate 加**聚焦/画廊双模式**——
  G 键或面板 Gallery 按钮切换；画廊把当前分类全部条目铺成网格（各自名字铭牌 + 带动画的各自自动 idle，
  Space 画廊中禁用），**点击 3D 展品拾取最近者回聚焦**（射线投地面+半径命中，与拖拽环绕零冲突）；面板
  列表点选等效退出画廊。bin 增 `--gallery` 旗标。断言 5 条（+画廊铺开/回聚焦）、全量 **121 绿**；真机 14
  模型网格读图 + 点选聚焦日志实证。提交随本轮 gallery 增量。
- **⚠️ 待设计负责人拍板**（实现取了保守解释，详见 capability-cards.md 对应卡）：
  ① AC3 掉落池：requirements §8「全塔池」vs §7.1「融合塔不掉落」矛盾——现按 §7.1（只掉 4 基础塔）；
  ② ME1 升级1 语义：「弓箭手进开局手牌池」有歧义——现取「开局必含弓箭手」；
  ③ AC2 商店定价：解锁费（进手牌）与建造费（每次放置）分离——现均为塔面价；
  ④ EN5/ME1 数值为凭空起点值（治疗 3/s、升级 30/60 币），requirements 未给数。
- **操作归属法则定稿（2026-09-14，纯设计无实现）**：requirements §16.9 补立「操作归属三分法」（对象[塔/塔位]/
  指挥官/系统）+ 热键按归属分域禁跨域复用 + 容量增长路径 + 转向门槛（avatar 翻案 / 单塔操作>4 / 指挥官技能>5 / 多阵营）；
  「显式指挥官」UI 改造评估**推迟至技能二期压力测试**（GDD 点子池留痕）。后续任何新 UI 卡立卡先答「操作归谁」。
- **弩射击动作线定稿（2026-09-20，方案 B：程序化分件动画）**：ART7-10 已立——ART7 瞄准追踪+炮口出弹（`AimTracker` 通用件）、
  ART8 击发后坐（`FireRecoil` 通用件，后续塔换装复用）、ART9 资产分件手术 v3（AI 写 Blender 无头脚本主刀、人验收、
  ADR-0009 回流岩库）、ART10 弦箭分件动画（完整射击循环）。分件规范入知识库
  `docs/topics/graphics/model-part-animation-prep-spec.md`（本次手术即规范首次实施）；升格判据：第 2 个模型手术成功 →
  脚本泛化 + 用户级 skill。编号说明：ART3-6 为地形线，ART7+ 为弩线，按立项时间。
- **技能系统二期全量落地（2026-09-29，ADR-0011 + SK5–SK11，一卡一提交）**：通用按实体技能框架
  （`SkillCaster`/`SkillDefs` 注册表/`SkillBalance` 单源数值表；被动/主动/自动施法三分法；`SkillCast` 确认消息=
  效果分发缝）+ 属性聚合管线（`StatMods`→`EffectiveStats`，tower_fire 只读聚合）+ 统一伤害管线
  （`DamageRequest` 消息进、`resolve_damage` 全游戏唯一扣血者、`DamageDealt` 带来源归因出）。指挥官三技能
  已迁上框架（SK1–SK4 验收句全保留）；弓箭手塔出厂带 鹰眼(被动暴击)/多重射击(主动)/火焰箭(自动施法) 三技能
  （SK9），指令卡 E=付费升级(30/60金) R/A=技能槽、火焰箭右键开关自动施放（SK10）；散落技能数值 const 全部
  收编进 SkillBalance（SK11），BAL 全程读数新增 塔普攻/塔技能/指挥官技能 三列伤害占比（首跑 3.6% 塔技能）。
  测试 153 全绿（skills_core 7 / stats_pipeline 5 / damage_pipeline 5 / skills_archer 6 / skills_ui 4 为新钉）。
  踩坑记录见 capability-cards.md 各卡（消息双缓冲断言、SystemParam 16 上限、derive 与 mut Query）。
- **SK8/SK10 视觉验收清账 + 技能系统三期 SK12（2026-10-06）**：①SK8/SK10 的 ⚠️ 人工验收项经自建视觉闭环
  过验——DB2 场景 `skill-card`（`--scenario skill-card` 帧脚本四态截图 + 引擎内 shot 诊断日志双通道裁决，
  方法论沉淀 KB howto《游戏内 UI 的自建视觉验收闭环》）；首验修三真问题（被动鹰眼让位 R 键、瞄准态 HUD 提示
  替换波预览、无人值守窗口光标被 winit 冲掉→PreUpdate 钉住）。②**SK12 融合塔招牌主动 ×4**（ADR-0011 框架
  第三个使用者）：神射手狙杀（血量最高×倍率）/大法师奥术爆轰（点地范围魔法）/魔弓手贯穿箭（**新
  `ProjectileOrigin::Linear` 直线穿透模式**，线上敌人各命中一次）/壁垒炮震荡冲击（范围伤害+减速）；融合产物
  经新公共网关 `tower::spawn_fused_tower` 出厂带单槽签名套件，E 付费升级 30/60 与指令卡技能钮**零改动复用**
  SK10 全套（框架通用性实证）；数值进 SkillBalance 新列；requirements §17.7 新表。`skills_fusion::*` 7 断言
  + 视觉段 sk_fusion.png 双通道过验。测试 **160 全绿**（behavior 153+7 / balance 2 / art 5）。
  SystemParam 16 上限二次撞上（ShotDiagnostics 束，immutable Query 双生命周期可入束）。
- **下一步**：**双美术线并行 + 真人试玩**——①地形线：ART4 基地/入口 → ART5 点缀 → ART6 黄昏光照（ART3 已落地待负责人试玩验收）；
  ②弩射击线：ART9 分件手术 v3 → ART10 弦箭动画（ART7 瞄准+炮口出弹、ART8 后坐已落地）；③**真人试玩盯 §15 信号**
  （技能两新信号对照 ≤20% 输出锚点 + §17.6 塔技能 ≤10%，SK12 签名计入塔技能列）；balance 数值仍按负责人意见暂缓。
  ④验收通道线：SM1 已落地（待真人视觉验收）→ SM2 动画触发+自由观察 → SM3 实弹区（技能/特效动态验收）→ SM4 截图基线。
  ⑤技能线后续：随机获取与 Meta 强化、被动/光环（框架后续使用者，见 requirements §39 调整后的暂缓清单）。
- **已知问题**：无阻塞。RunRng 全局种子化随机在 AC1-3；商店栏 UI 按 Hand/ShopOffers/语言变化自动重建；
  macOS 运行时日志会刷 `ICU4X ... No segmentation model for Chinese/Japanese` 警告（只影响中文换行断句算法，不影响字形渲染，当前 HUD 均单行，无实际影响）。
- **协作提醒**：acquisition / meta 已各自成域插件；后续卡在各领域插件**追加**组件/系统即可，别堆进 `game.rs`，
  尽量别改已落卡实现（新机制=新组件+新系统）；跨域时序走 `GameSet` + 显式 before/after。

## 项目专属规则

- 领域归属：塔→`towers`，怪→`enemies`，波次→`waves`，经济/生命→`economy`，地图/路径→`map`，HUD→`ui`，
  塔获取→`acquisition`(可归 ui 或独立)，Meta→新建 `meta`；跨领域时序一律走 `GameSet`（`src/sets.rs`），不要手调系统链。
- **双速节奏是硬约束**：波次进行中锁建造、波次间开经营菜单。实现任何「建造/放置」卡前，先立 ST1 相位状态（Intermission/Combat），别绕过。
- 数值放代码（Balance/Wave 等资源或定义表），不外部化；调参先立锚点卡，别裸调数字。
- **垂直切片边界**：先实现 `docs/capability-cards.md` 的 `★` 首条可玩闭合（单塔单波·战斗），再叠加融合/三选一/获取/meta，别一次铺太宽。
