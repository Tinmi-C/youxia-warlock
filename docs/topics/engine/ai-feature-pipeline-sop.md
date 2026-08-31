---
title: AI 特性开发标准流程 v1（SOP）
type: howto
topic: engine
date: 2026-08-30
author: AI（youxia 拍板）
status: done
tags: [流程, ai协作, 能力卡, dod]
related: ["topics/engine/capability-card-workflow-deep-dive", "topics/engine/ai-collaboration-by-phase", "topics/game-design/ai-asset-pipeline"]
---

# AI 特性开发标准流程 v1（SOP）

## 结论

任何新特性（玩法/资产/工具）都按**六阶段流程**开发：每阶段明确「AI 干活 / 工具门禁 / 人关卡」三栏，机器能判定的绝不烦人，人只把关主观项。出口以 **DoD 五条**为准。**试点：武器系统（卡 29-31）**，跑通后修订即成团队标准。

## 六阶段流程

| 阶段 | AI 干活 | 工具门禁（机器判定） | 人关卡（主观判定） |
|------|--------|---------------------|-------------------|
| 0 需求入纸 | 查 GDD 数值表 + 相似代码模式 → 产出立卡草案 | — | 确认需求方向 |
| 1 立卡 | 按模板写：接口 / 行为 / **数字化验收句** / 影响面 / 回归清单 | 验收句可测性自检（写不出数字 = 没理解清楚，退回） | **拍板验收句** |
| 2 资产准备（内容型） | 跑 `wash`/`intake`/复扫，产出图册 | R1-R8 检查 + 工单状态机 | **批参数 + 翻图册拍板** |
| 3 AI 实现 | 组件=数据、常量进定义表、老系统零改动、观察通道打日志 | `cargo test` 全绿 + 无新 warning | — |
| 4 真机验收 | 陪跑、反馈原文记入卡、即时修复 | — | **按验收句真机过** |
| 5 回归+提交+文档 | 验收句转测试、圈文件、拟 commit message、同步 log/MOC/踩坑 | 回归全绿 | 审提交（AI 不执行 git commit） |

## AI 高效利用三抓手

1. **上下文包前置**：每阶段开局按下面的模板给 AI 喂齐上下文，省摸索省来回。
2. **门禁前置**：工具先判（R1-R8 / cargo test / 基线对比 / scenario-check），AI 交给人之前必须门禁全绿。
3. **反馈语料复用**：人工反馈原文必入卡；起草同类卡时 AI 先读历史反馈避坑。已沉淀的例子：画布穿模→持久视口、速度常量误报→R8 排除规则、工单路径格式→状态机修正。

## AI 上下文包模板（复制即用）

```markdown
【任务】<一句话：做什么特性>
【卡】<粘贴已拍板的卡，或声明「尚未立卡，先走阶段 1」>
【数值来源】GDD §<章节>（数值照抄，不凭记忆）
【相关代码】<文件路径清单：如 src/systems/combat.rs, src/components.rs>
【资产】<是否涉及：涉及则给 raw 路径与目标名；不涉及写「无」>
【本阶段】<0-5，只做该阶段的产出>
```

## 出口标准（DoD，五条全过才算完）

1. 卡验收句真机全过
2. `cargo test` 全绿 + 新回归断言入 `tests/behavior.rs`
3. 含资产时：资产 landed + 图册可查 + R1-R8 无新增
4. 文档同步：卡状态 / `docs/log.md` / 踩坑笔记（如有）
5. 单一 commit（Conventional Commits，英文），可回滚；只圈自己文件

## 触发语速查

- 立卡：「按 SOP 给 <特性> 起草能力卡」
- 资产：「按 SC1 把 raw/<文件> 洗白入库，目标身高 <米>」或终端 `art-catalog wash …`
- 体检：「按 SC6 跑一遍资产体检」
- 实现：「按卡 <N> 实现，先跑门禁再交付」
- 收尾：「按 SOP 阶段 5 收尾本卡」

## 复用与修订

- 每个特性试点后允许修订本页（版本行追加记录）；修订本身走「人拍板」。
- 红线不变：破坏性动作需人批准、AI 不做 git commit、许可证/审美/手感永远归人。

## 参考

- [[topics/engine/capability-card-workflow-deep-dive|能力卡机制深度理解]]
- [[topics/engine/ai-collaboration-by-phase|游戏全生命周期的人机分工]]
- [[topics/game-design/ai-asset-pipeline|AI 生成美术管线八站流程]]
- `games/wave-survival/docs/capability-cards.md`（卡清单）
- `tools/art-catalog/CARD.md`（工具侧卡：AC-1~AC-5 即本流程的工具实践）
