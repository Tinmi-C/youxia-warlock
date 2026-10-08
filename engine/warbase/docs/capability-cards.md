# warbase 能力卡（BX / WX / TS 线）

> 工作流沿用团队能力卡约定：立卡 → 实现 → 验收（红绿）→ 回归 → **一卡一提交**。
> 规格底稿：`docs/topics/engine/warbase-implementation-design.md`（实现）+ `warbase-test-strategy.md`（用例）+ ADR-0012/0013。
> 游戏侧卡（loop-td 的 MA/EN/TO/HR/... 线）在 `games/loop-td/docs/capability-cards.md`，不在此登记。

## 卡路书（状态总览）

| 线 | 卡 | 标题 | 状态 |
|----|----|------|------|
| 基建 | WS0 | 根 cargo workspace 接线（成员/exclude/统一钉版/profile 上收） | ✅ |
| 基建 | TS-WS | workspace 一击回归基准（`cargo test --workspace` 全绿快照） | ✅ |
| 基建 | TS0 | 确定性基建（MatchRng / test_app / drain 助手 / 临时文件隔离） | ▶ |
| 基底 | BX0 | warbase crate 落位（feature 门控 + WarbaseSet 契约 + 防腐化检查） | ✅ |
| 基底 | BX1 | unit 域模型（统一 UnitDef + 攻甲类型表结构 + 经验等级） | ⬜ |
| 抽取对 | BX2+WX1 | 伤害管线抽取 / warden 回迁 | ⬜ |
| 抽取对 | BX3+WX2 | SkillCaster 抽取 / warden 回迁 | ⬜ |
| 基底 | BX9a | MapBundle serde 类型 + RON 装载/校验器（P0.5 先行） | ⬜ |
| 基底 | BX5 | 路径：闭环 + 圈数计数 + 分支字段位 | ⬜ |
| 基底 | BX6 | 波次导演：Auto/Manual + 预告 + 提前召唤 + 限时 | ⬜ |
| 基底 | BX7 | 资源：金/木/人口 + 兑换事件流 | ⬜ |
| 基底 | BX8 | 规则插拔：漏怪策略 + 建造窗口 + 胜负 | ⬜ |
| 基底 | build | 建造域：FreeGrid/FixedSlots 双模式 + 幽灵同源校验 | ⬜ |
| 抽取对 | BX4+WX3 | 命令卡框架抽取泛化 / warden 回迁 | ⬜ |
| 抽取对 | WX4 | 通用战斗 FX（feature "fx"）/ warden 回迁 | ⬜ |
| 基底 | BX10 | headless harness（DriverStrategy + HarnessReport） | ⬜ |
| 抽取对 | WX5 | BAL 迁 harness（读数口径对齐、不漂移） | ⬜ |
| 回迁 | WX6 | warden 参数化 + maps/warden.ron（基底验收 #0） | ⬜ |
| 收口 | BX9b | bundle 装配器收口 + `--map` 加载 + 迷宫冒烟（SMK） | ⬜ |

（TS1–TS7 测试卡随对应实现卡同卡收口，见测试策略 §5，不单列。）

## 立卡记录

### WS0 ✅ 根 cargo workspace 接线

- **做什么**：根 `Cargo.toml` 建 workspace；members=[engine/showroom-viewer, games/warden]（warbase/loop-td 随各自卡加入）；exclude 旧演示项目/模板/tools；`[workspace.dependencies]` 统一钉 bevy 0.19.1(+default_font,jpeg)/bevy_egui 0.42/rand 0.10/serde_json 1；成员 Cargo.toml 改 `workspace = true` 继承；profile（dev 依赖优化 / release thin-LTO / release-check）上收根（成员 profile 在 workspace 内无效）；成员 lockfile 删除（git rm），根 lockfile 入库。
- **验收句**：`cargo metadata` 解析成功；`cargo test --workspace` 一击全绿（warden 全量断言 + showroom-viewer 构建）。
- **回归**：warden 160 断言语义不变（本卡不动代码，只动构建结构）。
- **提交**：`build(ws0): root cargo workspace - unified pinning, hoisted profiles, single lockfile`

### TS-WS ✅ workspace 一击回归基准

- **做什么**：以 WS0 后的 `cargo test --workspace` 输出为基准快照（测试数/断言数记录进 log）。
- **验收句**：一击全绿；后续每卡收口以同一命令为门禁。
- **证据**：见 docs/log.md 当日条目。

### BX0 ✅ warbase crate 落位

- **证据**：`cargo test -p warbase` 1/1 绿（`warbase_sets_run_in_contract_order_every_frame`）；`cargo build -p warbase --no-default-features` 绿（防腐化）；`cargo test --workspace` 绿。
- **提交**：`feat(warbase): BX0 crate scaffold - WarbasePlugin + WarbaseSet contract + feature-gated module stubs`

- **做什么**：`engine/warbase` lib-only crate；`WarbasePlugin` + `WarbaseSet` 六段链（Tick→Director→Intent→Act→Resolve→Cleanup）；11 个机制模块占位（doc 注释标明设计段落与对应卡）；`ui`/`fx` feature 门控空插件；smoke 测试断言 set 契约顺序跨帧成立。
- **验收句**：`cargo test -p warbase` 绿；**防腐化**：`cargo build -p warbase --no-default-features` 绿（无 ui/fx 依赖纯逻辑可编译）；crate 无任何 games/ path 依赖、无资产、无数值。
- **回归**：`cargo test --workspace` 绿。
