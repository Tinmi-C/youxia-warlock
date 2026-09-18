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
- **⚠️ 待设计负责人拍板**（实现取了保守解释，详见 capability-cards.md 对应卡）：
  ① AC3 掉落池：requirements §8「全塔池」vs §7.1「融合塔不掉落」矛盾——现按 §7.1（只掉 4 基础塔）；
  ② ME1 升级1 语义：「弓箭手进开局手牌池」有歧义——现取「开局必含弓箭手」；
  ③ AC2 商店定价：解锁费（进手牌）与建造费（每次放置）分离——现均为塔面价；
  ④ EN5/ME1 数值为凭空起点值（治疗 3/s、升级 30/60 币），requirements 未给数。
- **操作归属法则定稿（2026-09-14，纯设计无实现）**：requirements §16.9 补立「操作归属三分法」（对象[塔/塔位]/
  指挥官/系统）+ 热键按归属分域禁跨域复用 + 容量增长路径 + 转向门槛（avatar 翻案 / 单塔操作>4 / 指挥官技能>5 / 多阵营）；
  「显式指挥官」UI 改造评估**推迟至技能二期压力测试**（GDD 点子池留痕）。后续任何新 UI 卡立卡先答「操作归谁」。
- **下一步**：DB1/DB2/DB3 与技能一期 SK1-SK4 均已落地（BAL1 已重取，SK 零影响）→
  **真人试玩盯 §15 信号**（战斗期能动性已上线，另观察「技能是否被主动用/是否挤压塔」两个新信号，对照 ≤20% 输出锚点）→
  balance 数值仍按负责人意见暂缓（BAL2 锚点表随时可收敛「早战经济压力」）。
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
