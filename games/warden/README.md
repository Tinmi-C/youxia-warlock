# Warden

> 从 `templates/bevy-game/` 复制而来。Bevy 0.19.1 + AI 协作原生工作流。
> 项目级 AI 约定见 [AGENTS.md](AGENTS.md)；能力卡工作流见 [docs/capability-cards.md](docs/capability-cards.md)；设计见 [docs/GDD.md](docs/GDD.md)。

Warden 是一款 **塔防游戏**：在固定路径旁布置炮塔，阻挡一波波敌军抵达终点。用有限金币造塔、升级、择机卖塔；清完全部波次即通关，防线被突破则 Game Over。

## 快速开始

```bash
cd games/warden
cargo run                               # 跑起来：场地 + 放置光标 + HUD
cargo test                              # 行为一致性回归测试（46 条断言）
RUST_LOG=info cargo run                 # 观察通道：日志仪表（每 2s 一条）
```

## 目录结构

```
src/
  main.rs          # 入口：只调 lib 的 build_app()，不堆业务代码
  lib.rs           # build_app()：App 组装（main 与测试共用同一构造）
  states.rs        # GameState 状态机（Playing / Paused / GameOver）
  components.rs    # 组件 = 纯数据（名词）；新机制 = 新组件 + 新系统
  resources.rs     # 资源 = 全局单例（Economy：gold / lives）
  sets.rs          # GameSet：Update 编排阶段（SystemSet，卡片挂载点）
  plugins/
    game.rs        # GamePlugin：相机 + 放置光标（可玩地基）
    map.rs         # MapPlugin：场地 + 边框（地图域）
    towers.rs      # TowersPlugin：塔域（接缝，卡片填充）
    enemies.rs     # EnemiesPlugin：敌域（接缝，卡片填充）
    waves.rs       # WavesPlugin：波次域（接缝，卡片填充）
    economy.rs     # EconomyPlugin：金币/生命（经济域）
    ui.rs          # UiPlugin：静态 HUD
    debug.rs       # DebugPlugin：观察通道（日志仪表 / F12 截图）
  systems/
    camera.rs      # 相机 + 灯光
    cursor.rs      # 放置光标 WASD 移动（CursorMove 卡）
    map.rs         # 路径/基地/塔位可视化（MA1/MA2）
    enemy.rs       # 敌人沿路径移动/漏怪/死亡金币（EN1/2/3）
    tower.rs       # 塔自动开火/索敌/护甲（TO3/TO4）
    wave.rs        # 波次生成/结算/输赢（WA1/2/3）
    input.rs       # 选塔/放置/开波/三选一控制（键盘快捷键；鼠标主流程在 pointer.rs）
    hud.rs         # 实时状态行（UI4）+ 下一波预览（UI8）+ 字体资源（UI6）
    l10n.rs        # 双语文案集中地（UI7：Bi(zh,en) 常量 + 按语言 format + 持久化）
tests/
  behavior.rs      # 行为一致性回归测试（验收闭环的可执行化）
assets/
  models/ textures/ audio/ fonts/ ui/   # 资产按类型分目录（模型 snake_case，clip 统一命名）
docs/
  capability-cards.md  # 能力卡工作流 + 卡清单（AI 开发的核心约定）
  GDD.md               # 游戏设计文档（一句话玩法/核心循环/范围/数值表/验收）
AGENTS.md          # 给 AI 智能体的项目上下文（AI 协作方式）
```

> 美术工作区约定：原始素材与候选图册放 `_art/`（如 `raw/`、`gallery/`），不入运行时 `assets/`。

## 操作（鼠标为主，键盘为辅）

**鼠标（War3 式命令卡，UI9-UI12，requirements §16）**：
- 常驻三区底栏：左·信息区（选中对象详情/属性）· 中·命令卡（随选中态切换）· 右·控制区（开波/2x占位/暂停/语言）
- **建造双入口**（步数相同）：① 待机点命令卡塔按钮 → 点空塔位放置；② 点空塔位 → 命令卡出建造列表，**悬停按钮该位实时显示幽灵塔+射程圈**
- 点已放塔 → 命令卡出 **卖出**（回收=投入×70% 向下取整）· **融合**（无可配对置灰）· 预留格；战斗期全部置灰
- **融合三步**：选塔 A → 点「融合」（可配对塔高亮、信息区显示配方+手续费）→ 点塔 B 确认（结果落 B 位，A 位腾空）
- 商店报价按钮任一态=解锁进手牌（AC2）；三选一保持中上弹卡点击选择
- 悬停命令卡任一按钮出 **Tooltip 全信息**（名称/热键/价格/属性/定位，中英）
- 建造期 HUD 显示**下一波敌人构成与奖励**预览（UI8）；自带开源中文字体 Noto Sans SC（UI6，跨平台一致）
- **战斗期**（PR/FX）：远程塔发射**追踪弹**（飞行可见、盾兵塔近战即时）；敌人受伤显血条（满血隐藏）、受击闪白、
  AOE 扩散圈、死亡消散壳；减速中变蓝、盾兵怪带盾标识

**键盘（Grid 热键，§16.7）**：
- `QWER / ASDF` 点命令卡对应格位按钮（角标印字母）· `Esc` 统一取消（建造/融合选择/选中态）
- `1/2/3` 三选一 · `Space` 开波 · `P` 暂停 · `L` 切语言 · 结算屏 `1/2` 买 Meta · `F12` 截图（旧 WASD/1-4/E/F 已退役）
- 结算屏（Game Over / 胜利）：`1` 买升级1（开局必含弓箭手塔）、`2` 买升级2（开局+20 金），`P` 再来一局
- 日志仪表：`RUST_LOG=info cargo run`，每 2 秒一条
  `[dash] fps≈.. state=.. phase=.. gold=.. base=.. wave=.. entities=..`

## 当前可玩

**灰盒 MVP 全量可玩一局**（2026-09-06 UI2 面板化+中文化）：L 形路径 + 基地 + 8 塔位，10 波（含精英/BOSS/治疗怪/盾兵怪），
塔自动开火（物理/魔法 + 一维物理护甲），双速节奏（波中锁建造）；5 种塔获取来源全通——
开局手牌（随机 2 塔保底输出）、商店保底刷新（波次间，报价卡可购买解锁）、掉落塔牌（精英/BOSS 必掉、普通 10%）、
三选一、融合（4 配方）；Meta 解锁链（死后留币，局间购买升级）。
敌人移速已按 requirements §9 锚点校准（speed 1.0 ≈ 20 秒走完全程）。
三选一为真随机（伤害/攻速/射程/金币/获得塔 5 类抽 3 个不重复，融合塔也吃加成）。
UI 完善三连（2026-09-11）：自带开源中文字体（UI6，摆脱系统字体依赖）、语言 中/英 切换（UI7，`L` 键/右上按钮，偏好持久化）、
下一波组成预览（UI8，建造期 HUD 显示构成+奖励）。
`cargo test` 39 条断言全绿；后续为真人试玩 + balance（见 `docs/capability-cards.md`）。

## 插件决策准则（重要：避免重复开发）

**用插件还是自己写？判断标准一条：这个功能的"正确性"是客观的还是主观的？**

| 功能性质 | 例子 | 做法 |
|---------|------|------|
| **客观正确**（技术难题，有标准答案） | 物理、碰撞、寻路、粒子、缓动、网络 | **找插件引用，不自己写**（bevy_rapier、bevy_hanabi…） |
| **主观设计**（玩法手感，无标准答案） | 塔伤害平衡、波次节奏、掉落、经济曲线 | **自己写**，按能力卡沉淀 |

**选型三原则**：
1. 版本对齐 Bevy 0.19（[bevydepy.com](https://bevydepy.com/popular?bevy=0.19) 按版本过滤）
2. 维护活跃（近半年有更新）
3. 作者可信（官方 / 知名组织优先）

## 踩坑备忘（团队已踩过，模板已内置规避）

| 坑 | 规避 |
|---|---|
| glTF 内嵌 JPEG 贴图加载失败 | Cargo.toml 已加 `jpeg` feature |
| Linux 无头运行 panic `libxkbcommon-x11.so` | 安装 `libxkbcommon-x11-0`（无显示器验证用 Xvfb + lavapipe） |
| Bevy assets 目录相对可执行文件解析 | 用 `cargo run`（自带 CARGO_MANIFEST_DIR）；直跑二进制需设置该环境变量 |
| Bevy 版本 breaking changes | 锁 `0.19.1`；升级前读 release notes |
| Windows 杀软（360 等）误报 ahash 构建脚本 → `os error 5` | 杀软加白名单 `target/` + 工具链/`~/.cargo` |

## 工作流（和 AI 一起开发）

1. **立卡**：新功能先写能力卡（`docs/capability-cards.md` 模板）：接口 / 行为 / **验收句（数字化、可执行）**。验收句写不出来的功能 = 还没理解清楚，不开工。
2. **AI 实现**：AI 按卡实现（新组件/新系统，老系统零改动）。
3. **人验收**：按验收句验收（看效果 + 跑 `cargo test`）。
4. **回归钉死**：验收句转成测试加进 `tests/behavior.rs`。
5. **一卡一提交**：Conventional Commits（`feat:` / `fix:` / `docs:` / `refactor:`）。
