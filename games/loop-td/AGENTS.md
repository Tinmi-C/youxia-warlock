# AGENTS.md — loop-td 项目上下文（AI 协作约定）

> 本文件给「在这个游戏项目里工作的 AI」看。全局规则见根目录 `/AGENTS.md`；知识库规范见 `docs/AGENTS.md`。
> 本项目的核心工作方式：**能力卡驱动 + 验收闭环 + 观察通道**（即团队 ADR-0002 的「AI 装配层」在项目层的落地，详见 `docs/capability-cards.md`）。

## 项目是什么

**循环圈塔防**（桌面 3D，Bevy 0.19.1）：怪物绕圈循环、漏怪不死、活怪存量施压。**本项目首先是对 warbase 基底的验收场**（ADR-0012）：玩法 = 地图 bundle（`maps/*.ron`，ADR-0013）+ 薄玩法插件；伤害/技能/波次/路径等管线代码一律在 `engine/warbase`，本 crate 只写 bundle 数据、挂钩注册与地图特有规则。需求草案：`docs/topics/game-design/loop-td-game-proposal.md`（立项后精化进本项目 `docs/requirements.md`）。

## 技术栈

- 引擎底座：**Bevy 0.19.1**（锁版本，升级先读 release notes，见根 workspace `Cargo.toml` 注释）
- **战斗基底：`engine/warbase`（ADR-0012/0013）**——依赖单向 games → warbase → bevy；本 crate 禁止出现伤害计算/技能框架/波次推进实现（薄游戏规则，基底验收 #1 的 grep 检查句）；版本经根 workspace `workspace = true` 统一钉死
- 语言：Rust stable，edition 2021
- 依赖引擎：无（不手写渲染/ECS——那是 ADR-0004 已退役的路线）
- 代码组织：lib + bin 分离（`src/lib.rs` 的 `build_app()` 供 main 与测试共用）、领域插件化（`src/plugins/`）

## 目录

```
src/main.rs          入口（只调 build_app()）
src/lib.rs           build_app()：App 组装
src/states.rs        GameState 状态机
src/components.rs    组件（纯数据）
src/resources.rs     资源（全局单例）
src/plugins/*.rs     领域插件（一个领域一个 Plugin）
src/systems/*.rs     系统（按系统分文件）
tests/behavior.rs    行为一致性回归测试
docs/capability-cards.md  能力卡工作流 + 卡清单
docs/GDD.md               游戏设计文档（玩法/范围/数值表/验收，填本游戏自己的；团队方法论见 vault）
assets/{models,textures,audio,fonts,ui}/
```

## AI 协作工作流（必须遵守）

1. **立卡**：做任何新功能，先写能力卡（`docs/capability-cards.md`）：接口 / 行为 / **验收句**。验收句必须数字化、可执行（例：「以 60fps 与 144fps 各运行 1 秒，移动距离都应等于 speed×1 秒，误差 <1%」）。**验收句写不出来的功能，说明还没理解清楚——先问人，不要开写。**
2. **AI 实现**：按卡实现，遵循卡上的接口/行为；老系统零改动（新机制 = 新组件 + 新系统）。
3. **人验收**：人按验收句验收（跑游戏看效果 + `cargo test`）。
4. **回归钉死**：验收句转成测试加进 `tests/behavior.rs`（「改了 A，B 没坏」的回归断言）。
5. **一卡一提交**：Conventional Commits（`feat:` / `fix:` / `docs:` / `refactor:`），提交信息英文，一个功能一个提交，可回滚。

## 硬性规则

- **先找插件，不重复造**：新功能先判断「正确性是客观还是主观」——客观技术能力（物理/碰撞/角色控制器/粒子/缓动/网络）优先引用生态插件（查版本对齐 0.19 + 维护度 + 作者可信），不要自己从零写；主观玩法逻辑（手感/节奏/数值）才自己实现并按能力卡流程沉淀。
- **不编造 Bevy API**：不确定的 API 先查 Bevy 0.19.1 官方示例/文档（`https://github.com/bevyengine/bevy/tree/v0.19.1/examples`），或明确说「不确定」并给出备选，不要猜。
- **代码注释用英文**；与人的对话、文档用中文。
- **观察通道**：行为变化必须可被日志/测试观察——系统里打关键日志（`info!`），验收靠 `cargo test` + 日志仪表，不靠「看起来对了」。
- **改动可回滚**：不在一行里塞多个无关改动；结构改动先立 ADR（团队 `docs/decisions/`）。
- **资产约定**：模型/贴图等放 `assets/` 对应子目录（`models/textures/audio/fonts/ui`）；glTF 优先（Bevy 原生）；JPEG 贴图已开特性，无需处理；模型文件用 snake_case 语义名（如 `goblin.glb`）；动画 clip 统一命名 `idle`/`walk`/`attack`/`hit`/`death`；美术原始素材、候选图册放 `_art/`（`raw/gallery` 等，不入运行时）。完整规范见知识库 `docs/topics/game-design/art-asset-catalog-tool-proposal.md`（art-catalog 管理系统设计稿，**拟采用、待团队拍板**；正式采用后本模板将预置 `_art/` 目录骨架）。
- **当前状态 / 下一步**：每次开工前更新本文件的「当前状态」小节（进行中的能力卡、已知问题）。

## 跨游戏约定（新游戏开工前看）

这些是团队沉淀的**可迁移**原则/决策，新游戏继承即可，不用重新摸索：

- **数值数据暂留代码 → 本项目已按 ADR-0013 收窄**：地图内容数值（塔/怪/波次/经济初值/攻甲系数）进 `maps/*.ron`（装载即校验、不静默迁移）；基底机制参数仍在 warbase 源码内。ADR-0008 原则对其余场景继续适用。
- **数值平衡用「设计目标锚点框架」**：先定目标体验 → 反推数值 → 用 5 字段锚点（类别/指标/目标区间/来源/验收方式）把体感难易变成可测指标 → 配一个 headless 读数工具给 AI 照着收敛。方法见知识库 `docs/topics/game-design/design-target-anchors.md`。
- **表现层别做 god-file**：一个插件文件别随着卡越堆越大（wave-survival 曾到 1151 行单文件）；按领域拆成子插件（挂模型/动画运行时/武器/朝向+移动感/受击表现），用 `SystemSet` 定跨插件顺序。**"一个领域一个 Plugin"** 对表现层同样适用。
- **客观技术→插件、主观玩法→自研**、**玩法机制第 2 次用到才抽进 `engine/`**：见本文件「硬性规则」+ 团队知识库 `docs/topics/engine/bevy-plugin-and-code-reuse.md`。


## 当前状态 / 下一步

（进行中的功能、已知问题、近期目标——每次开工前更新）

## 项目专属规则

（美术资源命名、模块约定等，随项目补充）
