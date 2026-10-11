# WX6 收口规格（warden-on-base 终态件）

> 本文是 WX6 系列剩余工作的交接规格，由 2026-10-11 会话调研固化。新会话以此开场，
> 配合 `engine/warbase/docs/roadmap.md`（as-built 表）与 `games/warden/docs/roadmap.md`。
> 流程纪律：规格-测试-小步提交（ADR-0014），每片 warden 161 断言零漂移（允许构造适配，
> 断言不变），全绿一笔提交。

## 已就位的地基（截至 b3c8cec）

- WX1/WX2：归因桥（已退役）+ 技能框架全走基底（字符串 id、SkillBalance id 键、SKILL_ORDER）
- WX3a：基底命令卡渲染层（可点击/置灰/网格/Tooltip/PanelVersion）
- WX4：基底 fx 实模块；warden 闪白/爆炸环/死亡壳走基底（FxBlast 桥）
- WX5a：基底 harness 词表 + 按波/按来源读数
- WX6-1：敌人行走走基底（PathFollower/advance_followers/Arrived 漏怪）
- WX6-2：Enemy 拆 Hp/MaxHp；血条走基底 + HealthBarStyle
- WX6-4a/4b：伤害走基底攻甲表 + resolve_damage/DamageOrigin；UnitRegistry 代码桥
- WX6-5a：Economy = 基底 Resources（wood 恒 0，starting_economy() 显式 100 金）

## 关键依赖发现（为什么波次/规则留到现在）

1. **基底 director 直接 spawn 基底单位**（UnitDef 驱动、`spawn_unit_deferred`），而 warden
   敌人文（Enemy 组件/模型 ENEMY_MODELS/血条样式/Healer/PathFollower）走自有网关
   `spawn_enemy_at` —— 单位数据化是波次切换的前置。
2. **基底 Lives 不支持 per-enemy leak**（warden 盾 2/精英 3/BOSS 5）—— 需基底 UnitDef
   补 `leak: u32`（serde default 1），Lives 分支按 UnitInstance→registry 查值扣。
3. **基底胜利判定依赖 WaveCleared**（SurviveWaves），warden 波次未切前不可用；warden 的
   胜负流转在 GameState 状态机（GameOver/Win），切基底后需 MatchEnded→GameState 映射。
4. **WX3b 命令卡**绑定塔索引 Build(usize)/双语 l10n/8 个 warden 动作——塔数据化后才能由
   build_card_from_registry 生成。**WX5b BAL** 依赖基底世界（MapBundle/BuildGrid）——
   warden.ron 就位后把 balance.rs 的驱动改写为 DriverStrategy。

## 建议切片（每片独立可验证，顺序即依赖序）

1. **基底 UnitDef += leak**（serde default 1）+ rules_tick 的 Lives 分支按 def 查值扣
   （Arrived 消息带 unit，查 UnitInstance→registry）。基底侧小增量 + bx 测试。
2. **敌人数据化**：EnemyDef 扩成完整基底 UnitDef（模型路径/缩放/是否跑动进 def 字段或
   def 旁挂表）；warden 敌人网关改"基底 spawn_unit + warden 挂件（模型子场景/血条样式/
   Healer/Enemy 组件）"或网关改由 def 驱动。Enemy 组件最终只剩 def_index 类的薄索引
   （或退役，读 UnitInstance.def）。
3. **塔数据化 + 建造域**：TowerDef/FusionDef → UnitDef（attack 进 AttackDef/AttackKit）；
   TowerSlot/固定塔位 → 基底 build::FixedSlots + BuildGrid + try_place/try_sell。
4. **波次切基底**：WaveDirector(Advance::Manual) + WaveTable + StartWave/WaveCleared；
   AC 商店刷新/三选一/Meta 挂 WaveCleared/WaveStarted 接缝（游戏侧插件保留）。
5. **规则切基底**：BaseHp → LivesLeft(10) + LeakPolicy::Lives + MatchRules/MatchFlow；
   MatchEnded → GameState 流转映射；漏怪 leak_arrived 保持消费 Arrived（基底 Lives 在
   rules_tick 扣，warden 不再自己扣 BaseHp——注意只留一个扣血通道）。
6. **warden.ron 落地**：units/waves/rules/econ_start/attack_armor 表/skills 全量进
   `games/warden/maps/warden.ron`（MapBundle schema，BX9a 装载/校验器现成）+ assemble
   替换代码版注册表；requirements §14 数值出处改注 RON。
7. **WX3b/WX5b 收口**：命令卡由 build_card_from_registry 生成（SKILL_CARD/PanelState
   已就位）；balance.rs 驱动改写为 DriverStrategy 跑 run_headless，锚点读数对齐
   （damage_by_origin 三列已备）。

## 护栏与命令

```bash
cargo test -p warden          # 161 绿（断言零漂移）
cargo test --workspace        # 261 绿基线
cargo build -p warbase --no-default-features   # 防腐化
cargo run -p warden           # 真机烟测（视觉基线 H3 待人工）
git add -A ':!.zcode' && git commit -m "feat(warden): WX6-… "
```

## 风险注记（本会话踩过的坑）

- warbase 的 `path/wave/skill/ui` 等模块是私有的，类型一律走 `warbase::prelude::`。
- 批量改测试构造（python 正则）后必须抽查：本轮曾把一个 Enemy 块替换截半，编译期才暴露。
- 基底 `resolve_damage` 的 Armor/UnitInstance 已放宽 Option（裸实体全额+零赏金）——测试
  裸敌人不再需要挂满组件，但断言赏金/护甲的测试要挂 UnitInstance/Armor。
- 死怪收尸是 hp<=0 扫描（resolve_death），UnitDied 在 warden 侧留作不消费——改规则域时
  注意别引入双扣血通道。
- Bevy 0.19：`despawn_recursive` 不在 EntityWorldMut 上（自写 despawn_subtree）；
  `clear_children` 已废弃且先解除关系（despawn 列表要在 clear 前收集）。
- Mimosa 完整审计一直未跑完（library_source/callgraph 部分失败）——不要宣称项目安全。
