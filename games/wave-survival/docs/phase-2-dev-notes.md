# 阶段二开发纪要 — 玩法深化（2026-08-27）

> 阶段二三张能力卡（卡 9–11）的代码开发内容汇总。卡定义与验收句见
> `docs/capability-cards.md`；本文记录「实际做了什么、为什么这么做」。
> 对应提交：`960c425` 立卡 → 3×(feat + docs) → `6029432` 收尾，已推送 origin/main。

## 总览

| 维度 | 阶段一收尾 | 阶段二收尾 | 变化 |
|------|-----------|-----------|------|
| `src/` 源码 | 795 行 | **1146 行** | +351 行 |
| 回归测试 | 553 行 / 22 个 | **843 行 / 33 个** | +290 行 / +11 个 |
| 提交数 | — | **7 个**（3 feat + 4 docs） | 全部已推送 |

新增能力：**Shift 范围斩（含粒子特效）、敌人三分化、运行时数值热调参**。
全程「新组件 + 新系统」，阶段一 8 张卡的既有测试**零改动通过**。

## 架构层面的四个新增

```
                    ┌─ GamePlugin（逻辑，headless 可测）
 NovaAttack 组件 ──►│ nova_slash 系统 ──► 扣血/白闪/冷却
 Shift 键          │        └─写─► NovaFired 消息（Bevy 0.19 的缓冲事件）
                    └─ VfxPlugin（渲染，只挂真机）
                          └─读─► 移动发射器 + EffectSpawner::reset() 重放粒子爆发

 MonsterKind 组件 ──► wave_system 刷怪时落数据 ──► 老系统只认 Hp/Chasing，零改动

 Balance 资源 ──► player_attack / nova_slash / contact_damage 每帧读取
      ▲
 TuningPlugin（egui 面板）F1 开关，拖滑条实时改；Player.speed 直改组件
```

两条关键分离原则：

1. **逻辑与特效分离**——`nova.rs` 不碰任何渲染类型（headless 可回归）；
   `vfx.rs` 只消费 `NovaFired` 消息。
2. **可调与固定分离**——伤害/冷却进 `Balance`（六项热调）；判定半径
   （0.9 / 1.5 / 0.40 / INVULN_TIME）是 GDD 设计值，保持常量不动。

## 卡 9 NovaSlash（feat `f559425`）

| 文件 | 内容 |
|------|------|
| **新建** `src/systems/nova.rs`（61 行） | 常量 1.6/60/5s；`NovaFired{at}` Message；`nova_slash` 系统：独立冷却节流 → XZ 半径内全体平伤 −60 无衰减 → 白闪 → 恰好写 1 条消息 |
| **新建** `src/plugins/vfx.rs`（84 行） | hanabi 冲击波资产：出生环小圆面 → 径向速度 ×5 → LinearDrag 减速 → 金色渐隐 0.7s；一次性 Spawner 手动触发；Update 监听 NovaFired 搬运发射器后 `reset()` 爆发 |
| 改 `components.rs` | +`NovaAttack { cooldown }`（与近战 Attack 完全独立） |
| 改 `player.rs` | spawn_player 多挂 NovaAttack |
| 改 `game.rs` | 注册消息 + 系统链插入 `player_attack → nova_slash → contact_damage`；R 重开时同步重置 Nova 冷却 |

VFX 触发链照抄 bevy_hanabi-0.19 官方示例 `spawn_on_command`：
一次性 `SpawnerSettings::once(...).with_emit_on_start(false)` +
每条 NovaFired 移动发射器位置后 `EffectSpawner::reset()` 重放一次爆发。

## 卡 10 EnemyVariants（feat `c5795b3`）

| 文件 | 内容 |
|------|------|
| 改 `components.rs`（61→115 行） | `MonsterKind { Grunt, Runner, Tank }` 组件 + 四个方法：speed_mul/hp_mul/cube_size/color——分型系数集中一处 |
| 改 `wave.rs`（83→127 行） | 计数公式：Runner 从 w3 起 floor((n−1)/2) 封顶 3；Tank 从 w5 起 n/5 封顶 2；`kinds_for_wave(n)` 确定性组成且守恒总数；刷怪按型落数据 |

分型属性：

| Kind | 颜色/尺寸 | speed | hp |
|------|----------|-------|----|
| Grunt | 红 0.6³ | 波次基线 ×1.0 | ×1.0 |
| Runner | 黄 0.45³ | ×1.6（快而脆） | ×0.5 |
| Tank | 紫 0.85³ | ×0.6（慢而硬） | ×3.0 |

兼容性红线：**前两波纯 Grunt、前四波无 Tank ⇒ 阶段一的波次断言逐字不变**
（实测全部原样通过）。碰撞体半径随 cube_size 变化；老系统对分型无感知。

## 卡 11 EguiTunePanel（feat `2541648`）

| 文件 | 内容 |
|------|------|
| 改 `resources.rs`（21→51 行） | `Balance` 资源六项（slash_damage/cooldown、nova_radius/damage/cooldown、contact_damage），Default = 原 GDD 常量 |
| 改 `combat.rs` / `nova.rs` / `contact.rs` | 机械替换 const → Res\<Balance\>（数值项）；判定半径保持常量；`damage_for(d)` 泛化为 `damage_at(d, max_dmg)` |
| **新建** `src/plugins/tuning.rs`（54 行） | F1 开关面板：六个 Balance 滑条 + Player.speed 滑条（组件承载项，move_player 不动） |
| 改 `lib.rs` | `EguiPlugin::default()` 挂在 GamePlugin 之前（保证 Balance 先存在），TuningPlugin 之后 |

两类调参生效路径：

- **全局数值项**：系统每帧读 `Res<Balance>`（六个滑条即时生效于下一次攻击/受击）；
- **组件承载项**：面板直接改 `Player.speed` 实体组件（move_player 零改动）。

## 回归测试 +11 个

| 组 | 测试 | 钉死的验收句 |
|----|------|-------------|
| Nova(4) | `nova_respects_cooldown` | 2s 内连按不重复触发，>5s 后恢复 |
| | `nova_full_damage_inside_radius_only` | d=0.4/1.55 各 −60，d=1.65 无效（增量计数断言恰好 1 条消息） |
| | `nova_hits_multiple_targets_and_flashes` | 三怪同帧各扣 60 且全白闪 |
| | `nova_independent_of_melee_cooldown` | 砍完立刻放 Nova 可行，双冷却互不影响 |
| Variants(4) | `variant_count_formulas` | 公式锚点：w3=1 runner…w10=2 tank、封顶值 |
| | `variant_composition_conserves_total` | n=1..15 组合恒等于 2+n |
| | `wave3_spawns_runner_with_kind_stats` | 强制刷 w3：恰 1 Runner 属性≈1.34×1.6/66×0.5 |
| | `wave5_spawns_tank_with_kind_stats` | 强制刷 w5：(4 grunt, 2 runner, 1 tank) 分型属性全对 |
| Balance(3) | `balance_defaults_equal_gdd_consts` | 默认值=原常量（等值迁移证明） |
| | `balance_slash_damage_applies_live` | 改 60 后下一刀实打 −60 |
| | `balance_contact_damage_applies_live` | 改 30 后下次贴脸扣 30 |

测试基建要点：Nova 消息断言一律用**增量计数**（触发前后各记一次
`Messages::len()` 取差值），不假设缓冲翻转时机，对 Bevy 内部 flush 语义鲁棒。

## 版本 API 发现（编码期解决）

- **Bevy 0.19 缓冲事件改名 Message**：`#[derive(Event)] → #[derive(Message)]`、
  `EventWriter → MessageWriter`、`add_event → add_message`。编译期抓到，
  已沉淀踩坑笔记 `docs/topics/engine/bevy-019-events-to-messages-pitfall.md`。
- bevy_egui 0.42 用 `bevy_egui::{egui, EguiContexts}` 命名空间；
  `EguiPlugin` 有字段需 `::default()`；Bevy 0.19 F 键是 `KeyCode::F1` 非 `KeyF1`。
- bevy_hanabi 0.19 的 `Gradient` 与 bevy::prelude 撞名 → 显式导入；
  `ColorOverLifetimeModifier` 新增 mask 字段 → 用官方构造函数 `::new(gradient)`。

## 待人工验收（视觉条目）

跑一次游戏即可完成（`cargo run`）：

1. 按 **Shift**：玩家位置爆出金色冲击波环粒子，消散干净无残留（卡 9 第 6 条）。
2. 按 **F1**：面板开/关；拖动 slash damage 滑条后一刀伤害立即变化；
   关闭后台无残留影响（卡 11 第 3 条）。

## 下一步（阶段三 表现层）

glTF 骨骼动画替换占位方块（bevy-spike 已验证过 CesiumMan 导入路径）、
UI 正式化、音效（GDD 后置项）。届时按老流程先立卡。
