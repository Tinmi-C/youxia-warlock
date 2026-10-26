# AGENTS.md — rts-lab 项目 AI 上下文

## 这是什么

- **warbase 基底验收图**（ADR-0016 §11 RL 系，Phase 4）：用基底全部域
  （选择/移动/寻路/生产/战斗/经济/规则/外交）搭一个最小 WC3 灰盒切片。
- **不是正式游戏**：无美术（bevy 原生灰盒，D9）、无独立玩法数值追求；
  一切数值都是「让基底机制可验收」的占位。
- 目的：检验 ADR-0012 护栏——**新地图/新玩法不改基底核心**。

## 硬规则（先读根 AGENTS.md，这里是增量）

1. **`engine/warbase/` 禁改**（ADR-0012 §6）。发现必须改基底才能实现的
   需求 → 停下，在增量报告写「抽象有洞」：哪个需求、缺哪个缝、游戏侧
   临时补丁是什么。这正是本项目的核心产出（通道 C 信号）。
2. 游戏侧薄插件里写补丁逻辑是允许且预期的（C 通道孵化）——补丁系统
   必须带注释标明它补的是哪条缝。
3. 地图内容（单位表/队伍/生产表/规则）一律进 `maps/arena.ron`
  （ADR-0013），不硬编码；开局兵力是基底缝缺失的例外（见报告），暂在
   游戏侧 `STARTING_LAYOUT`。
4. 规格-测试-小步提交（ADR-0014）：验收断言全部落 `tests/behavior.rs`，
   headless 驱动；`cargo test --workspace` 一击全绿是每增量门禁。
5. 真机灰盒（渲染/HUD）无法 headless 断言 → H1 人工清单进 README，
   增量完成时由人过。

## 当前状态

见 [docs/roadmap.md](docs/roadmap.md)（RL0 立项 → RL1 主城+农民+采集移动
→ RL2 交战+技能+生产 → RL3 物品+迷雾+小地图全链）。

## 技术要点（消费方接线，同 loop-td）

- 装配：`warbase::bundle::{load_bundle, assemble, build_grid}`（Startup）。
- 域插件由 `WarbasePlugin` 挂好：Selection/Movement/Production/Vision/
  Item/Replay/Sfx/Unit；消息已全注册，游戏侧只读写消息与资源。
- 指针→世界：基底 `OrthoTopDownPicker` 默认（正交俯视相机，z=0 拼玩法面）。
- headless 测试用 `warbase::testing::{test_app, send}`（固定 1/60 步长）。
