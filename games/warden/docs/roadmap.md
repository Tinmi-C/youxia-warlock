# warden 路线图

> 流程：规格-测试-小步提交（ADR-0014）；规格唯一依据 `docs/requirements.md`；历史开发日志见 `docs/capability-cards.md`（存档，停更）。

| 增量 | 内容 | 状态 |
|------|------|------|
| 既有基线 | 灰盒 MVP 全量可玩，153 tests 全绿（MA/EN/TO/WA/EC/UI/SK/PR/FX/DB/SM/ART1-8/12 等，详见存档卡路书） | ✅ |
| ART9 | 分件手术 v3 | ▶ 待实现（排在 WX 系列之后上 warbase 底座） |
| ART10 | 弦箭动画 | ▶ 待实现（同上） |
| BAL2 | 锚点表数值收敛（驱动策略已落地；数值收敛按负责人意见暂缓） | 🟡 |
| WX1–WX6 | 回迁 warbase 基底（成对增量，见 `engine/warbase/docs/roadmap.md`；完成后 ART9/10 在新底座上做）。WX1 归因桥接 ✅ · WX2 技能框架切基底 ✅（id 转字符串、SkillBalance id 键、基底 SkillCast.point）· WX3a 基底命令卡渲染层 ✅（GRID_KEYS/grid_badge 已上收基底；本作卡切换 WX3b 依赖 WX6 数据化） | 🟡 2/6 |
