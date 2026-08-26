# 能力卡工作流（AI 装配层 · 项目层落地）

> 团队理念（ADR-0002 / 方向纪要 §3）：自研 = AI 装配层，不是引擎轮子。
> 装配层的三个支柱在项目里的落地：**能力卡（WHAT）→ 验收闭环（证明做对了）→ 观察通道（看见在做什么）**。
> 团队知识库里的卡与规范：`docs/_private/learning/capability-cards/`（个人卡草稿区）、`docs/topics/`（成熟卡）。

## 为什么是能力卡

每学一个功能、每做一个系统，都写一张「能力卡 + 验收句」。**写得出来 = 理解到位；验收句能执行 = 做得对**。卡同时是给 AI 的「需求规格」——AI 按卡实现，人按验收句验收，AI 和人的沟通成本降到最低。

## 卡模板

```yaml
能力卡: <名称>
类型: system | component | asset | gameplay-system | architecture | ...
接口:
  输入: （它读什么：组件/资源/事件）
  输出: （它写什么：组件/资源/事件/命令）
行为: （每帧/每次做什么，写规则本身，不写目的）
验收句: （数字化、可执行、含边界条件——怎么自动/半自动验证它做对了）
```

## 工作流（和 AI 一起开发）

```
立卡 ──► AI 实现 ──► 人验收 ──► 回归钉死 ──► 一卡一提交
 │           │           │           │
 │           │           │           └─ feat: <卡名>（Conventional Commits）
 │           │           └─ 跑游戏 + cargo test + 日志仪表对照验收句
 │           └─ 新机制 = 新组件 + 新系统，老系统零改动
 └─ 验收句写不出来 = 没理解清楚，先问人，不开工
```

## 本项目的卡清单

> 垂直切片阶段（阶段 1）的 8 张卡，按开发顺序排列；玩法数值照抄 GDD 数值表。

| 卡 | 类型 | 状态 | 验收句要点 |
|----|------|------|-----------|
| PlayerMove | system | 🔄 模板已有，按手感重写 | WASD 斜向不超速；位移 = speed×Δt（帧率无关，误差<1%） |
| PlayerAttack | gameplay-system | ✅ 已立（2026-08-26） | Space 挥砍：≤0.9 满 −34 / 0.9~1.5 线性衰减至 0 / 冷却 0.45s（详见下方卡 2） |
| WaveSystem | gameplay-system | ⏳ 待立 | 三态流转；公式 2+n / 1.1+0.08n / 30×(1+0.4n)；波间 3s |
| EnemyChase | system | ⏳ 待立 | 追踪怪朝玩家移动（rapier 驱动），速度 1.1+0.08n |
| CombatContact | system | ⏳ 待立 | rapier 碰撞事件 → 受击扣血 + 白闪；死亡 despawn |
| PickupDrop | system | ⏳ 待立 | 击杀掉金色补给，走近自动拾取回血 |
| GameStateUI | ui-system | ⏳ 待立 | 血条 / 波次格子 / 冷却条；P 暂停 / 死亡 GameOver / R 重开 |
| GameLoop | system | ⏳ 待立 | 完整一局闭环：出生→刷怪→击杀→死亡→重开，无崩溃 |

## 卡 2：PlayerAttack（近战挥砍）

```yaml
能力卡: PlayerAttack（近战挥砍）
类型: gameplay-system
状态: 已立 2026-08-26（验收句待人工复核）
设计来源: m2 CombatSystem（Space 挥砍 / 半径 0.9 / 一刀 34 / 冷却 0.45s）
设计变更: GDD 数值「0.9 内 −34 / 1.5 外 −0」经团队确认 = 0.9~1.5 之间线性衰减。
          m2 原实现只有 0.9 平砍（一刀 34），衰减是 wave-survival 的新决策。
依赖: 测试桩怪物（带 Hp + Transform 的静态实体，暂不依赖 WaveSystem/EnemyChase）
接口:
  输入:
    - Space 按下（与 m2 键位一致）
    - Player 实体: Transform + Attack 组件（冷却状态）
    - 怪物实体: Hp 组件 + Transform（QueryFilter 排除 Player / 掉落物）
  输出:
    - 命中怪物 Hp.hp 减少（按距离衰减）
    - 命中反馈白闪（Visual.flash = 1.0，延用 m2 约定）
    - Attack.cooldown 重置为 0.45s
行为:
  - 每帧: Attack.cooldown -= dt，clamp 到 ≥ 0
  - Space 按下且 cooldown == 0 时触发一次挥砍:
    以玩家位置为圆心、半径 1.5 内的所有怪物为候选（d = 怪物与玩家的水平距离）:
      d ≤ 0.9        → 伤害 34
      0.9 < d ≤ 1.5  → 伤害 34 × (1.5 − d) / 0.6   （线性衰减，d=1.2 时 ≈17）
      d > 1.5        → 伤害 0
    命中: Hp.hp -= 伤害; Visual.flash = 1.0; Attack.cooldown = 0.45
  - 怪物 hp ≤ 0 的死亡 / despawn 归 CombatContact 卡，本卡只扣血
验收句:
  1. 冷却节流: 固定 60fps 驱动，Space 间隔 0.3s 连按 2 次 → 只触发 1 次;
     间隔 0.5s 连按 2 次 → 触发 2 次
  2. 伤害: 距离 0.5 的桩怪 hp −34; 距离 1.2 的桩怪 hp −17（±1）; 距离 1.6 的桩怪 hp 不变
  3. 衰减边界: 距离 0.9 → −34（±1）; 距离 1.5 → −0（±1）
  4. 多目标: 两个桩怪同时命中，各自按距离扣对应伤害
  5. 以上全部转 tests/behavior.rs（无渲染 App 手动驱动，模式同 PlayerMove 测试）
```

## 观察通道约定

- **日志仪表**：`RUST_LOG=info cargo run` → 每 2 秒 `[dash] fps≈.. state=.. entities=..`（`src/plugins/debug.rs`）。
- **调试面板**：`F1` 开关 bevy_egui 面板（实时调波次/手感参数）。
- **截图**：`F12` 存 `./screenshot.png`（给验收/队友看效果）。
- **回归测试**：验收句转 `tests/behavior.rs`（不带渲染的 App 手动驱动，见示例）。

## 踩坑记录规范（知识库要求）

踩坑必记：现象 / 根因 / 解决 / 反思。落团队知识库 `docs/topics/<领域>/`（type: pitfall），不是只记在自己脑里。
