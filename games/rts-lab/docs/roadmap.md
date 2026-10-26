# 路线图（RL 系，ADR-0016 §11 Phase 4）

> 本项目是**基底验收图**，不是玩法游戏：每个增量 = 消费一批基底域 +
> 验收断言 + 人工试玩项。规格见 vault `docs/topics/engine/warbase-wc3-alignment-plan.md` §11；
> 验收落 `tests/behavior.rs`；「抽象有洞」记录进各增量报告（通道 C）。

| 增量 | 内容 | 消费的基底域 | 状态 |
|------|------|--------------|------|
| RL0 | 立项：模板复制/改名/workspace 入列/文档骨架 | —（装配管线） | ✅ |
| RL1 | 主城+农民+采集移动切片：maps/arena.ron（24×24 FreeGrid、两队、LastTeamStanding）；开局布兵；T 键训练；采集循环（矿↔城，+10 金/趟）；左键点选/框选/右键移动；灰盒渲染+HUD | bundle/selection/movement(1–5)/production(1)/econ/rule(LastTeamStanding)/unit(teams) | ✅ |
| RL2 | 交战+技能+生产扩容：A/P 定点令（游戏侧 XZ 过渡）、兵营+步弓（投射物 kit）、英雄 paladin 双技能带蓝（holy_light 主动/Q 键/瞄准策略；devotion 光环）、科研 attack_upgrade（R 键）、物品链（掉落/拾取/用药/商店） | combat/skill(SPL1-2)/movement(MOV4)/production(PRD2)/item(INV1-3)/buff(光环) | ✅ |
| RL3 | 物品+迷雾+小地图+录像全链：vision 段+VisionSource 挂接（半径写死）、InFog 敌暗我亮视觉、MinimapModel 消费渲染+点击跳镜头、--record/--replay（lab 组合对账，帧 0 探针）、SFX 最小监听 | item/vision(FOG1–3)/replay(RPL)/sfx(AUD1) | ✅ |
| H1 | 人工试玩关卡：RL1-RL3 清单过一遍（README「操作」节 1-16）+ 既有 loop-td H1 | — | ⬜（RL1-RL3 条目随增量交付，人工关卡待过） |
