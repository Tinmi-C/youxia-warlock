# warbase 路线图

> 流程：规格-测试-小步提交（ADR-0014）。规格底稿：`docs/topics/engine/warbase-implementation-design.md`（实现）+ `warbase-test-strategy.md`（用例）+ ADR-0012/0013。游戏侧进度在 `games/loop-td/docs/roadmap.md`。

## 已完成（as-built 2026-10-08）

| 增量 | 内容 | 提交 |
|------|------|------|
| WS0 | 根 cargo workspace（成员/exclude/统一钉版/profile 上收） | f73848e |
| TS-WS | workspace 一击回归基准（warden 153 全绿快照） | — |
| BX0 | crate 落位：WarbasePlugin + WarbaseSet 六段链 + ui/fx feature 门控 | 8b5ac08 |
| TS0 | 确定性基建：MatchRng / test_app / 逐帧 drain / 临时文件隔离 | afe40e0 |
| BX1 | unit 域模型：统一 UnitDef + 攻×甲表 + 组件组合 spawn（含 auras/hero/skills/heal/upgrade_to/hp_scale/cost 字段） | afe40e0→01d84fe |
| BX2 | 伤害管线：DamageRequest→resolve_damage 单写 Hp + DamageOrigin 归因（Attack/Skill/System） + 索敌 First/Last 按圈序 + 冷却开火 + 追踪弹 + 攻甲矩阵 + DamageTaken 光环 | afe40e0→ed6fb4e |
| BX5 | 路径：闭环（圈数计数+LapCompleted）+ 开链（Arrived once）+ 程序化 Transform | afe40e0 |
| BX6 | 波次导演：Auto/Manual + 预告 + 提前召唤（含奖励）+ BOSS 限时 + 清波奖励 + **Endless 循环**（清表回卷 ×1.15^圈） | 77a660a→a34ad52 |
| BX7 | 资源：金/木/人口 + 兑换曲线 + 赏金入账 + 买人口（单命令流 EconCommand） | 77a660a |
| BX8 | 规则插拔：Lives/Stockpile/漏下家(预留) + 建造窗口(Always/PrepOnly) + 绕圈增益 + 胜负单次流转 + 败因可读 | bc0baf6 |
| build | 建造域：FreeGrid/FixedSlots（含 origin 居中对齐）+ try_place/try_sell/try_upgrade（清单可读拒绝）+ Buildable 组件 + BuildGrid Clone | 7ded62e→8db7779 |
| BX3-lite | 技能/光环/英雄成长：SkillCaster + 三分法 + 冷却/施放校验/自动施放接缝 + StatMods→EffectiveStats 四通道聚合 + 塔/随行双光环 + XpHolder 吃击杀经验升级 + spend_skill_point + Healer 周期群疗 | 96ab042→701f6e1 |
| BX9a | MapBundle RON 装载/校验（拒载不迁移）+ assemble + bundle::build_grid（路径包围盒居中） | a89b97b |
| BX10 | headless harness：DriverStrategy + Greedy 基线 + HarnessOpts（种子/帧上限/注入）+ HarnessReport（胜负/波次/存量峰值/建塔数/按定义伤害归因）+ 可复现 | 61808e8 |
| BX4a | 命令卡 headless 核心：CommandCardDef/SlotDef + build_card_from_registry + 热键解析 + CommandCardAction 分发 | 1ef7410 |
| BX4b | 命令卡面板：底栏槽位数据驱动生成 + 缓存键重建 + 升级阶排除出建造卡 | a129ebe |
| TO3 | 塔升级链：UnitDef.upgrade_to + try_upgrade（付目标阶造价、原格换体、人口 1:1、顶阶可读拒绝） | 7c8045c |
| BAL1 | 三锚点收敛：A1 新手存量峰值 6/10 + A2 注入 20 翻盘 + A3 BOSS 38.2s∈[36,54] | 2348830 |
| WX1 | warden 接基底依赖 + DamageSource.to_origin() 归因桥接（160 断言零漂移） | f93f0d7 |
| WX2-prep | 基底 SkillDef 增 badge/hotkey + SkillDefs len()/is_empty() | 030a0d6 |
| WX2 | warden 技能框架切基底：SkillCaster/SkillSlot/SkillDefs/SkillDef/CastSkill/SkillCast 全改用基底类型（历史路径重导出，调用面零改名）；id 模型索引→字符串（SKILL_ORDER = 平衡表行序 + DamageOrigin 桥）；SkillBalance 增 ids 字段改 id 键；基底 SkillCast 增 point（游戏侧瞄准层写入）+ tick_skill_cooldowns 钳 0（warden 复用）；移除 HashMap 序不稳的 by_index。160 断言零漂移 | 本笔 |

## 待做（warden 回迁，建议每对单独会话）

| 增量 | 内容 | 障碍 |
|------|------|------|
| WX3 | warden 命令卡切基底 | warden 五态切换/Tooltip/Grid 热键/中英双语需基底先补渲染层（WX3-prep 的 SKILL_CARD/PanelState 已就位） |
| WX4 | warden FX 切基底 | warden FX 299 行深度耦合 Enemy.def_index/Slow/ProjectileImpact；基底 fx 是空壳 |
| WX5 | warden BAL 切基底 harness | warden balance.rs 500+ 行驱动策略需改写为 DriverStrategy trait |
| WX6 | warden 参数化 → maps/warden.ron | 最大单件：固定塔位/双速/单币/L 路径全成数据；requirements 层改写 |

## 消费方

| 游戏 | 地图 | 状态 |
|------|------|------|
| loop-td | maps/loop.ron（30 波循环圈）+ maps/maze.ron（20 波迷宫）+ maps/maze-smoke.ron（5 波冒烟） | ✅ v1 完整可玩 |
| warden | （待 WX6 后 maps/warden.ron） | 🟡 WX1/WX2 已切（技能框架走基底） |
