# rts-lab —— warbase 基底验收图

> 从 `templates/bevy-game/` 复制立项（RL0）。**这不是一款正式游戏**：它是
> 「游戏 = 地图 bundle + 薄插件」架构（ADR-0012/0013/0016）的**验收场**——
> 用基底现有能力搭最小 WC3 灰盒切片，检验「新地图不改基底核心」护栏。
> 项目级约定见 [AGENTS.md](AGENTS.md)；进度见 [docs/roadmap.md](docs/roadmap.md)。

## 定位与铁律

- 一切机制来自 `engine/warbase`（选择/移动/寻路/生产/战斗/经济/规则/外交域），
  本 crate 只提供 `maps/arena.ron` + 薄玩法接线 + 灰盒渲染。
- **护栏铁律（ADR-0012 §6）：`engine/warbase/` 一行不改。** 发现基底缝缺失 →
  停下记录（通道 C 信号），游戏侧写补丁逻辑孵化，不回填引擎。
- 视觉灰盒：bevy 原生纯色方块/描边/文字，零资产（D9）。

## 快速开始

```bash
cargo run -p rts-lab          # 真机灰盒（渲染/HUD 人工验收 H1）
cargo test -p rts-lab         # RL 系验收测试（headless）
cargo test --workspace        # 全仓一击回归（含 warbase 373 基线）
```

## 操作（H1 人工试玩清单）

RL1 切片（主城 + 农民 + 采集移动）。注：鼠标指针由游戏侧 XZ 桥驱动
（基底 pick 链有平面约定缝，见 `docs/GDD.md` 抽象有洞清单），键盘
H/S/Esc/Tab/数字编队走基底。

1. `cargo run -p rts-lab` 启动；看到 24×24 竞技场：左下主城（大蓝方块）、
   4 个农民（小方块）、右侧金矿（金色大方块）、右下 3 个敌方 grunt（暗红方块）。
2. **左键点农民** → 出现绿色方形描边（选中）；底部 HUD 显示选中名。
3. **拖框选**多个农民 → 描边多选，HUD 显示数量。
4. **右键空地** → 农民走过去（直线/绕墙均可；右键点到敌兵=攻击令）。
5. **T 键** → 主城训练一个农民（金 -50，5 秒后主城旁刷出新农民，人口 +1）。
6. 不操作时农民**自动往返矿↔城**采集：搬金时头顶有小金块，每趟 +10
  （HUD 金数上涨）。
7. （A 攻击移动：基底 A/P 定点令的平面缝已由游戏侧桥补齐——见下 RL2 清单。）
8. `F12` 截图到 `./screenshot.png`（验收证据）；`RUST_LOG=info` 看仪表。

RL2 切片（交战 + 技能 + 生产纵深 + 物品）：

9. **A 键攻击移动**：选单位 → 按 `A` → 左键点地 → 单位边走边打（路遇
   敌兵自动交战、杀完继续走）；`P` 键同式两点巡逻。
10. **兵营出兵**：左键点兵营（蓝方块旁的深蓝小方块）→ `T` 训步兵
    （60g/6s）/ `F` 训弓手（70g/7s，箭为追踪投射物）；选主城时 `T` 仍
    训农民。
11. **英雄圣光**：选圣骑士（金色大块）→ `Q` → 治疗选择集内受伤最重的
    盟友（无人受伤则自体）；蓝 100 耗 40、冷却 6s；受伤单位把它和英雄
    一起框选即可验瞄准策略。
12. **科研**：`R` 键 → 100g/8s → 步兵弓手攻击 +15%（观察训练出的兵
    打 grunts 掉血变快；headless 断言在 tests/behavior.rs ⑩）。
13. **物品**：杀一个 grunt → 尸边掉绿瓶（healing_potion，必掉）；英雄
    （或农民）走过去自动入包；选英雄按 `1` 用药回 80 血。商店（中央
    紫方块）：英雄贴近（3 格内）按 `B` 买药 30g（半价回收走基底）。

## 目录结构

```
src/
  lib.rs            # build_app()：DefaultPlugins + WarbasePlugin + 薄插件
  main.rs           # 入口
  components.rs     # 玩法薄组件（MainHall/GoldMine/Carrying/HarvestRoute...）
  resources.rs      # CurrentMap
  plugins/
    game.rs         # GamePlugin：headless-safe 核心（装配/开局/训练/采集）
    visual.rs       # VisualPlugin：灰盒渲染 + HUD（真机专用，测试不加）
    debug.rs        # 日志仪表 + F12 截图
  systems/          # camera / gameplay / selection / visuals / hud
maps/arena.ron      # 地图=自包含数据包（ADR-0013）
tests/behavior.rs   # RL 系验收测试（headless 驱动）
docs/               # roadmap / GDD
```

## 与其他游戏的关系

| 项目 | 角色 |
|------|------|
| `warden` / `loop-td` | 玩法游戏（TD 品类消费者） |
| **`rts-lab`** | **验收图**：证明基底域拼起来就是一个 RTS，且不需要改引擎 |
