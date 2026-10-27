# AGENTS.md — legion 项目上下文（AI 协作约定）

> 本文件给「在这个游戏项目里工作的 AI」看。全局规则见根目录 `/AGENTS.md`；知识库规范见 `docs/AGENTS.md`。
> 本项目的核心工作方式：**规格先行 + 测试验收 + 小步提交 + 观察通道**（ADR-0014）。

## 项目是什么

- **军团战争图**（WC3 军团战争式 2v2 对抗）：第三张 warbase 验收图。
- 用途：**度量基底复用效率**（KPI 对比 rts-lab，见 `docs/kpi.md`——每个增量记录行数/耗时/新洞，这是本项目的一级产出）。
- 玩法：回合制（建造期 30s 雇兵布阵 → 战斗期自动交战 → 漏怪扣命 → 结算入账），命尽出局，LastTeamStanding 定胜负。
- 需求规格（数值锚点）：vault `docs/topics/game-design/legion-td-proposal.md`。

## 硬规则（先读根 AGENTS.md，这里是增量）

1. **`engine/warbase/` 禁改**（ADR-0012 §6）。发现必须改基底才能实现的需求 → 停下，记入
   `docs/kpi.md` 洞清单（通道 C 信号）：哪个需求、缺哪个缝、游戏侧补丁是什么。
2. 游戏侧薄插件里写补丁逻辑是允许且预期的——补丁系统必须带注释标明它补的是哪条缝。
3. 地图内容（单位表/布阵格/队伍/规则/开局）一律进 `maps/arena.ron`（ADR-0013），不硬编码。
   例外（已记洞）：per-player 钱包/命数/人口在基底无对应缝，游戏侧资源承载，起始值仍取自 bundle。
4. 规格-测试-小步提交（ADR-0014）：验收断言全部落 `tests/behavior.rs`，headless 驱动；
   `cargo test --workspace -j4` 一击全绿是每增量门禁。
5. 真机灰盒（渲染/HUD）无法 headless 断言 → H1 人工清单进 README，增量完成时由人过。

## 当前状态

见 [docs/roadmap.md](docs/roadmap.md)（LG0 立项 → LG1 建造期+自动交战 → LG2 发兵+结算 → LG3 UI）。

## 技术要点（消费方接线，同 rts-lab）

- 装配：`warbase::bundle::{load_bundle, assemble, build_grid}`（Startup）。
- 域插件由 `WarbasePlugin` 挂好（Selection/Movement/Combat/Econ/Unit）；游戏侧只读写消息与资源。
- 建造期买兵走游戏侧 `HireCommand`（基底无商店买单位缝，记洞）；战斗期发令走基底
  `UnitCommand { order: AttackMove }`（MOV4）；交战全自动（pick_targets/fire_attacks）。
- headless 测试用 `warbase::testing::{test_app, send}`（固定 1/60 步长）。
