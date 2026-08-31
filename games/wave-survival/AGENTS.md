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

> 状态索引而已——逐卡规格/验收句/反馈记录只在 `docs/capability-cards.md` 一处维护
> （未闭环卡全文在卡清单文件，已闭环卡真相在代码+回归），本节不重复细节。

- **进行中（待人工终审）**：
  - 卡 29 武器定义表+扇形命中（56 回归全绿）：IronSword/Glaive 定义表、120°/60°
    扇形命中、攻击自动面向最近敌人、Balance 改倍率语义（F1 保留）。真机过一遍
    扇形+双武器差异即可闭环
  - 卡 30 武器形体+手骨挂点（59 回归全绿）：两把 Blender 占位武器经 wash 全链
    landed；attach_weapon 挂 mixamorig:RightHand 手骨 + weapon_scale_fixup 缩放
    补偿。验收反馈①②③已入卡（骨骼挂点重构 / 握剑手型=素材任务 / 卡 25 债
    RUN_CLIP_AUTHORED_SPEED 4.0→2.8 校准 + clip 日志补 rate）
- **待人工视觉验收**（实现+回归均绿，缺人跑游戏）：卡 19 四新皮+w6 精英 /
  卡 21 玩家四态动画 / 卡 22 怪物咬人起攻+受击 / 卡 23 Nova 四层爆发+震屏 /
  卡 27 攻击顿帧——看什么全在卡清单各卡验收句
- **已完成**（一卡一提交）：卡 1-8 垂直切片（08-26）→ 卡 9-11 玩法深化（08-27）→
  卡 12-14 表现层首批（`c07520d`/`0c9119a`/`0ef1ca0`，08-27 真机验收）→ 卡 15
  `3a15cf2` → 卡 16 `bb637fa`+`d44ec77` → 卡 18B `f10df37` → 卡 24 `f64c464` →
  卡 25 `1ebf2c7`+`947a932` → 卡 26 `5e12381`+`c3f24de`（24/25/26 于 08-29 真机验收）
- **草案待开工**：卡 28 上下半身分层（卡 27 根治路线）/ 卡 31 远程弹道（可选）
- **素材任务（非代码，走管线）**：握剑手型 = Mixamo one-handed sword 动画集
  （下载 → mixamo_merge.py → normalize → wash → R8，配方在卡 30 反馈②）；
  怪物三态 idle；正式字体/音效（搁置）
- **已知问题**：
  - 残余穿模（08-27 验收记录，影响不大挂起）：围堵时怪群短暂互渗——候选
    碰撞球→胶囊 / CCD / 推挤刚度，届时立卡
  - 卡清单两个「卡 18」重编号：等队友美术 WIP（卡 17/未推提交）落库后一并拍板
  - 本机 git dubious ownership：需一次
    `git config --global --add safe.directory F:/developSpace/warlock`
  - PowerShell 文本管道曾写坏 UTF-8 注释：改文件一律走 AI 文件工具
- **近期目标**：卡 29/30 终审闭环 → 待验收队列清账（19/21/22/23/27）→
  候选 Dash 冲刺（纸面已议）；「音效」搁置（2026-08-27）
- **协作提醒**：队友未提交的美术 WIP 在工作区（GDD/style-bible/tools/art 等），
  AI 提交只圈自己的文件，勿整目录 `git add`

## 项目专属规则

（美术资源命名、模块约定等，随项目补充）
