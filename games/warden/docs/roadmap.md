# warden 路线图

> 流程：规格-测试-小步提交（ADR-0014）；规格唯一依据 `docs/requirements.md`；历史开发日志见 `docs/capability-cards.md`（存档，停更）。

| 增量 | 内容 | 状态 |
|------|------|------|
| 既有基线 | 灰盒 MVP 全量可玩，153 tests 全绿（MA/EN/TO/WA/EC/UI/SK/PR/FX/DB/SM/ART1-8/12 等，详见存档卡路书） | ✅ |
| ART9 | 分件手术 v3 | ✅（2026-10-11：body+string 两节点，弦轴心居中；零视觉漂移=转台 4 视角像素差 0 + 断言 `ballista_surgery_parts_v3`；岩库 v3 回流 + 清单锁 3；记录 `docs/topics/art/art9-ballista-surgery-record.md`；模型原生无箭，ART10 另做） |
| ART10 | 弦箭动画 | ▶ 待实现（ART9 已备好 string 节点；箭需另行极小建模） |
| BAL2 | 锚点表数值收敛（驱动策略已落地；数值收敛按负责人意见暂缓） | 🟡 |
| WX1–WX6 | 回迁 warbase 基底（成对增量，见 `engine/warbase/docs/roadmap.md`；完成后 ART9/10 在新底座上做）。WX1 ✅ · WX2 ✅ · WX3a/WX3b ✅（渲染层上收基底；建造/购买行费用读注册表行=RON 权威——8 动作双语卡保持游戏侧语义，`build_card_from_registry` 全生成形态属基底 kit 游戏）· WX4 ✅ · WX5a/WX5b ✅（harness 词表对齐 damage_by_origin 三列；真局驱动保持 warden 侧——基底 `run_headless` 服务基底 kit 对局）· WX6 全切片 ✅（1 行走/2 血量/4 攻甲+伤害/5 经济/6a per-unit leak/6b 敌人 def 驱动网关/6c 塔+建造域 BuildGrid/6d 波次 director/6e 规则 MatchEnded/6f `maps/warden.ron` 全量落地+代码表退役） | ✅ |
