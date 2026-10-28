# 塔阵容 v2 设计稿（WARDEN-ROSTER，round-4 Phase 2）

> 立项：`docs/topics/engine/warbase-round4-plan.md` §Phase 2 第 4 线（WARDEN-ROSTER）。
> 本稿是阵容 v2 的**规格层源文件**：骨架与数值锚点沿用 `requirements.md` §5/§7（**零漂移**），
> 每塔按 warbase 基底第一轮后的新能力（前后摇 / 溅射缝 / 英雄三围+蓝 / 装备）重新表达。
> 验收以 `tests/behavior.rs::roster_v2` + `tests/map_bundle.rs` 数字化断言为准。
> 工程口径：def 数据上收 `maps/warden.ron`（机制数值唯一权威，§14 注）；运行时新机制 = 新组件 + 新系统。

## 0. 铁律（本稿与实现都不许碰）

| 铁律 | 内容 |
|------|------|
| 数值锚点零漂移 | §5 基础塔表与 §7.3 配方表的 造价/伤害/攻速/射程/攻击类型/DPS 带 全部不变；BAL 系列锚点（§14/§17.6）不动 |
| 断言零漂移 | 现有 warden 全量断言中，锚点未变者必须**原样**通过（不做测试改写）；新能力只加新断言 |
| 行为零漂移的构造保证 | 后摇一律 < 该塔攻击间隔、英雄塔三围换算表只开 INT 通道 → 新能力默认态 = 旧行为逐帧一致 |
| 范围 | 只改 `games/warden/**`；引擎能力以"消费方"身份调用，禁改 `engine/**` |

## 1. 基础塔 v2（4 座，锚点列 = §5 原值）

| 塔 | 造价 | 伤害 | 攻速 | 射程 | 类型 | 前摇 s | 后摇 s | AOE 半径 | v2 新表达 |
|----|------|------|------|------|------|--------|--------|----------|-----------|
| 弓箭手塔 archer | 50 | 10 | 1.0 | 9 | 物理·单体 | **0** | **0** | 0 | 无前摇速射（ranger 防御者造型不变，§17.5 技能组不变） |
| 盾兵塔 shield | 40 | 5 | 1.2 | 4 | 物理·近战 | **0** | **0.30** | 0 | 近战即时命中不变；0.30 后摇是节奏表达（0.30 < 间隔 0.833 → 帧语义零漂移） |
| 法师塔 mage | 70 | 15 | 0.5 | 14 | 魔法·单体 | **0** | **0** | 0 | v2 维持单体魔法弹（§5"小范围 AOE"是自 MVP 起的已知缺口，归属 BAL 收敛卡，本稿不动） |
| 炮塔 cannon（弩炮化） | 80 | 25 | 0.33 | 12 | 物理·AOE | **0.60** | **0.40** | **2.5** | 弩炮：前摇长威力大（重弩蓄力）；AOE 走基底溅射缝（§3）；弦箭动画机器挂回（§5） |

数值锚点验收（数字化，`tests/map_bundle.rs` 扩列比对 + def 断言）：
- `windup_secs`/`backswing_secs` 按 TowerDef 表落 RON `AttackDef`；archer/mage 为 0（serde 默认语义 = 旧行为）。
- 炮塔 `aoe_radius = 2.5`（§5"大范围 AOE"的起点值；基底 `AttackDef` 无溅射字段，半径暂留游戏表 `TowerDef`，见 §3 注）。
- 前后摇运行时复用基底组件 `warbase::prelude::AttackWindup` / `AttackLock`（不新造状态机）。

## 2. 融合塔 = 英雄塔（4 座，锚点列 = §7.3 原值）

| 塔 | 构成 | 伤害 | 攻速 | 射程 | 三围 STR/AGI/INT | 蓝池（INT×12） | 回蓝（INT×0.3） | 招牌技蓝耗 |
|----|------|------|------|------|------------------|----------------|-----------------|-----------|
| 神射手 marksman | 2×弓 | 18 | 1.2 | 12.6 | 6/14/10 | 120 | 3.0/s | 狙杀 30 |
| 大法师 archmage | 2×法 | 25 | 0.6 | 16 | 4/6/16 | 192 | 4.8/s | 奥术爆轰 40 |
| 魔弓手 hybrid | 弓+法 | 18 | 0.9 | 11 | 6/10/12 | 144 | 3.6/s | 贯穿箭 30 |
| 壁垒炮 bastion | 盾+炮 | 40 | 0.35 | 9 | 14/6/10 | 120 | 3.0/s | 震荡冲击 35 |

- RON 行升级为 `hero: true` + `hero_stats` + 招牌技 `skills`/`skill_levels: [1]`（数据层英雄化；
  运行时经 `spawn_fused_tower` 网关真挂 `HeroStats` + `HeroStatsRaw`，转换走基底
  `hero::apply_pending_stats`，"spawn 后首帧算定"）。
- **换算表（游戏侧 `HeroStatsDef`，代码内单源）**：`hp_per_str 25 / attack_speed_per_agi 0 / armor_per_agi 0.01 / mana_per_int 12 / mana_regen_per_int 0.3`。
  - 只开 INT 通道是**锚点守护**：AGI 攻速换算一旦打开，融合塔攻速即漂移 §7.3 列（塔无 Hp/承伤，STR/AGI 的另两条换算对塔惰性）。
  - 三围数值本身是数据（英雄气质 + 未来成长缝），蓝耗/蓝池/回蓝是本期能力维度。
- **蓝门语义**（对齐基底 `process_casts` 校验序：caster→冷却→蓝→执行）：
  - 施法确认时扣蓝（蓝不足拒施：无冷却、无弹、无蓝扣——与 SK1"取消不付账"同族）；
  - 点地技能**进入瞄准不扣蓝**，确认点击才扣（取消永不受损）；
  - 回蓝走基底 `tick_skill_cooldowns`（已在线，`ManaRegen` 钳 `MaxMana`）；
  - 蓝耗单源：`SkillBalance` 新列 `mana_cost`（循 SK11"数值只在 SkillBalance"；基底 `SkillDef::mana_cost` 留 0，
    与 WX2 冷却桥同一先例——活动门读 SkillBalance）。
  - 指挥官/弓箭手塔无 `Mana` 组件 → 其技能保持免费（零漂移）。
- 蓝耗锚点：单发蓝耗 ≤ 蓝池 1/3（120 池最高 40）；CD 25–35s 内回蓝 ≥ 75 ≥ 单发蓝耗 → 长线自循环，不引入新资源管理焦虑。

## 3. 溅射缝（炮塔 AOE 的基底表达）

- 受害者收集改走基底 `warbase::prelude::area_targets`（SKVOC 溅射缝：Hostility oracle + 死亡/友军过滤 + 实体序确定）。
- 落点：新 Command `SplashHit`（`&mut World` 语义，与 `ApplySlow` 同族），`resolve_impact` 的 AOE 分支排队它，
  单体弹路径不动；AOE 弹目标中途死亡仍在最后位置起爆（PR2 语义不变）。
- `Hostility::default()`（单玩家 P0 vs 环境）由游戏侧 Startup 插入——warden 塔无阵营组件，敌 `Allegiance::Attacker`（WX6-6b 已带）。
- 注：基底 `AttackDef` 无溅射字段（基底无 AOE 伤害管线，ADR-0012 纪律）；炮塔半径 2.5 暂住 `TowerDef`，
  待基底长出 splash def 字段再上收（本稿记录，不反向改引擎）。

## 4. 装备系统 v1（LT2 EquipSlots 模式移植）

- 组件 `TowerEquip { slots: [Option<EquipItem>; 3] }`：**只挂融合塔**（英雄塔 3 槽；基础塔无槽——LT2 守卫断言同款）。
- v1 物品表（代码内，ADR-0008；词缀经 warden StatMods 管线注入，`ModSource::Other("equip:<slot>")` 每槽独立 source——同槽替换刷新、异槽叠加）：

| id | 中文名 | 词缀 | 数值 |
|----|--------|------|------|
| whetstone | 磨刀石 | DamageMult | ×1.15 |
| swift_string | 迅捷之弦 | AttackIntervalMult | ×0.90（攻速 +10%） |
| eagle_lens | 鹰眼镜 | RangeMult | ×1.15 |

- API：`equip::equip_tower_item(world, tower, slot, item)` / `unequip_tower_slot(...)`（换装撤旧，LT2 语义）。
- 获取渠道（掉落/商店）**立卡后续**；v1 = 数据模型 + API + 调试/测试缝（断言钉死，见表 §7）。
- 默认无装备 = 旧数值逐帧一致（锚点只绑定未装备基线）。

## 5. 弦箭动画挂回实战（ART10 收官）

- 炮塔（弩炮）实战挂载：`attach_tower_appearance` 的 cannon 分支（有 AssetServer 时）=
  `tower_arrow.glb` 塔身（v1 已锁，ART12 后闲置的"箭塔"体量正好当弩炮台）+ `weapon_ballista.glb` v3 分件
  挂 `BallistaMount`（`AimTracker` 240°/s / 回正 2s / 前向 −X / 炮口 0.5 + `FireRecoil` 0.12/0.18s——
  全部沿用 ART7/8 通用件与既有常量）。
- `wire_ballista_string` 零改动即接上真实场景的 `string` 节点（ART9 v3 手术件）→
  `TowerFired` 驱动真实弦箭循环：击发帧前弹 0.25/0.12s → 按攻击间隔回拉（弩炮 3.03−0.12 = **2.91s**）→ 箭重现。
- 与前摇的时序：前摇期内弦保持 Drawn（蓄力可读），击发帧才 snap——前摇即"上弦瞄准"，回拉即装填。
- headless（无 AssetServer）保持灰盒回落：游戏逻辑零依赖资产（ART1 网关哲学不变）。

## 6. 旧补丁退役清单（被基底能力替代者）

| 旧游戏侧补丁 | 替代的基底能力 |
|--------------|----------------|
| `tower_unit_registry` 硬编码 `windup_secs: 0.0 / backswing_secs: 0.0` | 前后摇数据上收 def（TowerDef/FusionDef → RON `AttackDef`） |
| `resolve_impact` 内联 AOE 受害者循环 | 基底 `area_targets` 溅射缝（`SplashHit` Command） |
| 施法校验无蓝门（handle_cast_requests 只查冷却） | 基底 Mana 语义（Mana/MaxMana/ManaRegen + 确认扣蓝 + 基底回蓝 tick） |
| 融合塔无英雄数据（hero_stats None 硬编码） | 基底 `HeroStats` + `HeroStatsDef` 转换 + `apply_pending_stats` |
| BallistaString 仅存在于测试夹具（ballista_app 手工挂载） | 炮塔实战挂载（§5），夹具保留钉通用机器 |

## 7. 验收标准（数字化，全部进 `tests/`）

| # | 断言 | 判据 |
|---|------|------|
| A1 | 前摇帧窗 | 弩炮（windup 0.6s @60fps）：帧 34 前无弹无 TowerFired；帧 [35,37] 内出弹+消息；冷却自伤害点起 3.03s |
| A2 | 前摇取消 | 目标前摇期内死亡 → 无弹、无消息、冷却不烧（回 0） |
| A3 | 后摇锁 | AttackLock 存在期不起新前摇（手造 0.1/0.5 时序钉死）；到期自动放行 |
| A4 | 溅射命中数 | 爆心 + 2.5 内 2 个哑靶 + 外 1 个（距 4.0）：恰好 3 条伤害，外侧不中 |
| A5 | 英雄蓝池 | 融合塔出厂 Mana=MaxMana=INT×12、ManaRegen=INT×0.3、HeroStats 三围与配方一致 |
| A6 | 英雄蓝耗 | 招牌技扣对应蓝耗；蓝不足拒施（无 CD 无弹无扣）；回蓝按 3.0/s 复充 |
| A7 | 免费通道零漂移 | 指挥官/弓箭手技能照旧免费可施（无 Mana 组件即无门） |
| A8 | 装备 | 装磨刀石 → EffectiveStats.damage ×1.15；撤回 ×1.0；同槽换装不叠加；基础塔拒挂 |
| A9 | 弦箭实战 | 有 AssetServer 的世界中经网关放置炮塔 → BallistaMount+AimTracker 配置钉 + 真场景 string 节点接线 + 击发后 Snapping→Drawing（draw 2.91s）|
| A10 | RON 零漂移 | `committed_ron_matches_code_tables` 扩列：attack（damage/interval/range/attack_type/windup/backswing）+ hero/hero_stats/skills 逐行比对 |
| A11 | 视觉取证 | 离屏（headless_renderer）2 张：Drawn（弦回满、箭搭）与 Drawing（击发后回拉中），非黑守卫 |

## 8. 美术需求要点（art-needs 投影源）

| 对象 | 需求 | 投影预期 |
|------|------|----------|
| 4 融合塔 | 英雄化外观升级（hero: true 触发）：剪影/品质阶区别于基础塔；每塔 1 模型卡 + 图标 | `scan` 报 **changed ×4**（外观字段 hero 漂移）→ 走岩库 changed 流转升级需求 |
| 炮塔（弩炮） | 组合已锁（tower_arrow v1 + weapon_ballista v3 + bolt v1）——**零新模型**；需图标卡（新） | tower 需求卡维持，icon 类新增 |
| 盾兵塔 / 法师塔 | 灰盒 → 需模型卡（P1，美术批次） | 既有 tower 卡维持 draft |
| 弓箭手塔 | ranger v2 已锁；图标卡（新） | 维持 |
| 数值列（windup/mana/伤害等） | — | **静默漂移**（外观过滤白名单外），需求卡不动 |

图标需求（4 基础塔 + 4 融合塔命令卡/商店图标 128px）走 `docs/art-ui.json` 声明清单（后续 NEEDS 线消费），
本稿只记需求要点，不展开 schema。

## 9. 实施切分（一增量一提交）

1. `docs(roster-v2)`: 本设计稿。
2. `feat(warden): roster v2 def`——前后摇/AOE/英雄/蓝/装备数据与运行时 + RON 再生 + 新断言（A1–A8, A10）。
3. `feat(warden): ballista string back in play`——炮塔实战挂载 + 弦箭断言（A9）+ 离屏取证 2 张（A11）。
4. art-needs 演练产物随 2/3 提交（baseline 先行）。
