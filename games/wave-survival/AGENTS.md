# AGENTS.md — Wave Survival 项目上下文（AI 协作约定）

> 本文件给「在这个游戏项目里工作的 AI」看。全局规则见根目录 `/AGENTS.md`；知识库规范见 `docs/AGENTS.md`。
> 本项目的核心工作方式：**能力卡驱动 + 验收闭环 + 观察通道**（即团队 ADR-0002 的「AI 装配层」在项目层的落地，详见 `docs/capability-cards.md`）。

## 项目是什么

波次生存（Wave Survival）：操控角色在波次递增的怪物进攻中生存，每波更强，看你能撑到第几波。桌面 3D 游戏。设计文档见 `docs/GDD.md`；玩法数值继承 m2-bevy 已验证设计（照抄 GDD 数值表，不凭记忆改）。

## 技术栈

- 引擎底座：**Bevy 0.19.1**（锁版本，升级先读 release notes，见 `Cargo.toml` 注释）
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
- **资产约定**：模型/贴图放 `assets/` 对应子目录；glTF 优先（Bevy 原生）；JPEG 贴图已开特性，无需处理。
- **当前状态 / 下一步**：每次开工前更新本文件的「当前状态」小节（进行中的能力卡、已知问题）。

## 当前状态 / 下一步

- **进行中**：卡 16 UiFormalization 已实现（feat `bb637fa`，2026-08-27，44 个回归全绿；
  视觉验收基本通过，暂停冻结修复 `d44ec77` 待复验）。补齐 GDD 清单：波次格子
  （顶部中央存活敌数 pips）、Nova 冷却条（紫罗兰）、P 暂停遮罩；debug 提示行迁底部
  半透明；布局/色板收敛为 ui.rs 常量组。技术路线沿 GDD 锁定的 bevy_ui，F1 egui 零接触。
  暂停修复：rapier 步进不受 GameState 门控，动态怪体曾带残速穿透暂停画面——
  新增 `sync_physics_pause` 把 `RapierConfiguration.physics_pipeline_active` 镜像到状态。
- **已完成**：卡 15 MonsterFacing（feat `3a15cf2`，2026-08-27 真机视觉验收通过，40 个回归全绿）。
  `Heading` 观测组件 + `derive_heading`（逻辑链尾）+ wrapper yaw 恒定角速度平滑
  （540°/s 最短弧，掉头 ≈0.33s），物理零交互（只转场景子实体）。
  阶段三第一批（卡 12–14 表现层）实现 + 真机人工验收通过（2026-08-27，
  `c07520d` / `0c9119a` / `0ef1ca0` + 真机修正 fix×5，38 个回归全绿）。
  首次真机跑抓出并修复：HanabiPlugin 未挂、egui 0.42 调度器、模型路径、相机方位与输入
  镜像、玩家幽灵分组（咬合距离）、待机确定性定格——细节见 `docs/phase-3-dev-notes.md`。
  共同架构：PresentationPlugin 只挂 `build_app`，headless 测试零渲染依赖
  （唯一漂移：三条旧 flash 断言按 review 后的衰减公式带宽修订并注明出处）。
- **已知问题**：
  - 单走路 clip 的待机方案（验收反馈#2，最终版 `2280cf7`）：停步瞬间确定性定格在动画第 0 帧（统一站架）——曾试「走完当前步再停」但纯走 clip 的循环末端姿势仍是迈步姿态，视觉不可区分且有预加载竞态；真 idle 待机仍需动作素材，到位后再立动画状态机卡。（朝向缺口已由卡 15 解决）
  - 物理语义修正（验收反馈#4）：怪物由 KinematicVelocityBased 改为 Dynamic + 零重力 + 锁定旋转——kinematic-vs-kinematic 在 rapier 中不产生接触是设计行为，改后怪群互相推挤、不再穿透玩家（卡 4 的距离判定不受影响）
  - **残余穿模（2026-08-27 验收记录，人判定影响不大，挂起后续解决）**：① 咬合瞬间玩家与怪的模型交叠是「玩家幽灵分组」的设计预期；② 重度围堵时怪群之间仍可能短暂互渗（速度直写 + 碰撞球半径小于模型包围盒）。候选方案：碰撞球按模型实际体形校准（球→胶囊）、开启 CCD、或调接触推挤刚度——届时单独立卡
  - 本机 git 报 dubious ownership，需执行一次 `git config --global --add safe.directory F:/developSpace/warlock`
  - 本机曾发生 PowerShell 文本管道写坏 UTF-8 注释的事故（已用 git 恢复）；改文件一律走 AI 文件工具或显式 UTF-8 编码
- **近期目标**：卡 16 待人工视觉验收（跑 `cargo run` 看 HUD 四新件）→ 推送归档 → 「音效」按 GDD 时机立卡；挂起项等素材（idle 动画、残余穿模）
- **协作提醒**：工作区存在队友未提交的美术侧 WIP（GDD/style-bible/tools/art 等），AI 提交时只圈自己的文件，勿整目录 `git add`
- **阶段二收尾存档**：卡 9–11 全部完成（2026-08-27，33 个回归测试全绿，见 `docs/phase-2-dev-notes.md`）

## 项目专属规则

（美术资源命名、模块约定等，随项目补充）
