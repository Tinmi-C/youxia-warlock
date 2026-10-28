# warden 路线图

> 流程：规格-测试-小步提交（ADR-0014）；规格唯一依据 `docs/requirements.md`；历史开发日志见 `docs/capability-cards.md`（存档，停更）。

| 增量 | 内容 | 状态 |
|------|------|------|
| 既有基线 | 灰盒 MVP 全量可玩，153 tests 全绿（MA/EN/TO/WA/EC/UI/SK/PR/FX/DB/SM/ART1-8/12 等，详见存档卡路书） | ✅ |
| ART9 | 分件手术 v3 | ✅（2026-10-11：body+string 两节点，弦轴心居中；零视觉漂移=转台 4 视角像素差 0 + 断言 `ballista_surgery_parts_v3`；岩库 v3 回流 + 清单锁 3；记录 `docs/topics/art/art9-ballista-surgery-record.md`；模型原生无箭，ART10 另做） |
| ART10 | 弦箭动画 | ✅（2026-10-12：`BallistaString` 消息驱动动画机器——TowerFired→弦沿本地 X 前弹 0.25/0.12s→按攻击间隔回拉，箭随弦前移、飞行中隐藏、回拉完成重现；箭=新岩库资产 weapon_ballista_bolt v1（tools/art/make_bolt.py 程序化建模）。**注**：ART12 后实战塔（弓箭手=游侠+弓）不再挂弩，本机器当前由行为测试夹具钉死，挂载点回到实战（地图/塔阵容重做）即零成本生效） |
| BAL2 | 锚点表数值收敛（驱动策略已落地；数值收敛按负责人意见暂缓） | 🟡 |
| WX1–WX6 | 回迁 warbase 基底（成对增量，见 `engine/warbase/docs/roadmap.md`；完成后 ART9/10 在新底座上做）。WX1 ✅ · WX2 ✅ · WX3a/WX3b ✅（渲染层上收基底；建造/购买行费用读注册表行=RON 权威——8 动作双语卡保持游戏侧语义，`build_card_from_registry` 全生成形态属基底 kit 游戏）· WX4 ✅ · WX5a/WX5b ✅（harness 词表对齐 damage_by_origin 三列；真局驱动保持 warden 侧——基底 `run_headless` 服务基底 kit 对局）· WX6 全切片 ✅（1 行走/2 血量/4 攻甲+伤害/5 经济/6a per-unit leak/6b 敌人 def 驱动网关/6c 塔+建造域 BuildGrid/6d 波次 director/6e 规则 MatchEnded/6f `maps/warden.ron` 全量落地+代码表退役） | ✅ |
| WARDEN-ROSTER | 塔阵容 v2（设计稿 `docs/roster-v2.md` 先行；§5/§7 锚点零漂移）——前后摇（弩炮 0.60/0.40、盾兵后摇 0.30 表达、弓手无前摇）、炮塔 AOE 2.5 走基底 `area_targets` 溅射缝（SplashHit Command）、4 融合塔英雄化（HeroStats 三围+INT 蓝池+招牌技蓝耗 30/40/30/35，确认扣蓝/取消不付）、装备架 TowerEquip（LT2 模式，3 槽+3 件套，API 期）；退役：windup 硬编码 0/内联 AOE 循环/无蓝门；`warden.ron` 再生（hero 行+attack 前后摇列，map_bundle 漂移守卫扩列）；ART10 弦箭挂回实战（炮塔网关挂 tower_arrow+weapon_ballista v3，TowerFired 驱动真实循环，离屏取证 2 张 `docs/img/roster-v2/`）；art-needs 演练（baseline 14 卡 → hero 翻转 changed ×4——工具 kind 改名盲区已记录，本地草稿迁移绕过 → 快照刷新 exit 0）。测试 +10（roster_v2 9 + roster_shots 1 ignored） | ✅ |
