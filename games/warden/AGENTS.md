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
- **资产约定**：模型/贴图等放 `assets/` 对应子目录（`models/textures/audio/fonts/ui`）；glTF 优先（Bevy 原生）；JPEG 贴图已开特性，无需处理；模型文件用 snake_case 语义名（如 `cannon.glb`）；动画 clip 统一命名 `idle`/`walk`/`attack`/`hit`/`death`；美术原始素材、候选图册放 `_art/`（`raw/gallery` 等，不入运行时）。
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
  - `cargo check --all-targets` + `cargo test` 全绿零警告（34 条断言，`tests/behavior.rs` + `tests/balance.rs`）。
- **⚠️ 待设计负责人拍板**（实现取了保守解释，详见 capability-cards.md 对应卡）：
  ① AC3 掉落池：requirements §8「全塔池」vs §7.1「融合塔不掉落」矛盾——现按 §7.1（只掉 4 基础塔）；
  ② ME1 升级1 语义：「弓箭手进开局手牌池」有歧义——现取「开局必含弓箭手」；
  ③ AC2 商店定价：解锁费（进手牌）与建造费（每次放置）分离——现均为塔面价；
  ④ EN5/ME1 数值为凭空起点值（治疗 3/s、升级 30/60 币），requirements 未给数。
- **下一步**：**balance 数值按负责人意见「暂缓，等有时间再调」**（BAL2 锚点表+读数工具已备好，随时可收敛「早战经济压力」）→ 先做**非数值项**：UI polish
  （塔位悬停高亮/幽灵预览、Game Over/Win 画面、语言切换——见 GDD 点子池）→ 真人试玩盯 §15 信号 → 待有精力再收敛数值。
- **已知问题**：无阻塞。RunRng 全局种子化随机在 AC1-3；商店栏 UI 按 Hand/ShopOffers 变化自动重建。
- **协作提醒**：acquisition / meta 已各自成域插件；后续卡在各领域插件**追加**组件/系统即可，别堆进 `game.rs`，
  尽量别改已落卡实现（新机制=新组件+新系统）；跨域时序走 `GameSet` + 显式 before/after。

## 项目专属规则

- 领域归属：塔→`towers`，怪→`enemies`，波次→`waves`，经济/生命→`economy`，地图/路径→`map`，HUD→`ui`，
  塔获取→`acquisition`(可归 ui 或独立)，Meta→新建 `meta`；跨领域时序一律走 `GameSet`（`src/sets.rs`），不要手调系统链。
- **双速节奏是硬约束**：波次进行中锁建造、波次间开经营菜单。实现任何「建造/放置」卡前，先立 ST1 相位状态（Intermission/Combat），别绕过。
- 数值放代码（Balance/Wave 等资源或定义表），不外部化；调参先立锚点卡，别裸调数字。
- **垂直切片边界**：先实现 `docs/capability-cards.md` 的 `★` 首条可玩闭合（单塔单波·战斗），再叠加融合/三选一/获取/meta，别一次铺太宽。
