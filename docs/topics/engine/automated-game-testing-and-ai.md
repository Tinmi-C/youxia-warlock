---
title: 游戏自动化测试与 AI 参与开发——全景调研
type: reference
topic: engine
date: 2026-09-18
author: AI（调研编译，待人工审核）
status: draft
tags: [testing, ai-agents, qa, bevy, capability-card]
related: ["[[capability-card-workflow-deep-dive]]", "[[ai-collaboration-by-phase]]", "[[topics/game-design/design-target-anchors]]"]
---

# 游戏自动化测试与 AI 参与开发——全景调研

## 结论

游戏测试的行业共识是**分层**：底层 headless 单元/系统测试（快、确定）→ smoke/soak/性能 → 引擎内建功能自动化（Unity UTF / Unreal Automation）→ 顶层真人 playtest（AI 至今替代不了）。AI 在其中两条线快速落地：**AI 玩游戏找 bug**（脚本机器人 → DRL → LLM agent，2024-2026 学术与产业都有可量化成绩），和 **AI 参与开发时的测试生成与验收**（Meta TestGen-LLM 证明 LLM 生成的测试要过「能编译/能通过/涨覆盖」三道闸才可用，73% 被生产接受）。本团队现有流程（能力卡验收句 → headless 回归 → 红绿验证 → 日志仪表）与行业模式同构；已知缺口是**视觉快照测试**与**自博弈平衡 bot**，均有可对标方案。

## 正文

### 1. 游戏测试分层与引擎基建（传统部分）

- **测试金字塔在游戏里的形态**：单元/系统测试（headless、确定性）→ smoke test（ Andrew Fray 的定义：「在合理范围内尽量多跑运行时，找明显故障」；Unreal 官方要求单条 smoke <1s）→ 功能自动化（覆盖具体玩法路径）→ soak/性能/兼容 → 真人 playtest（手感/乐趣，不可自动化）。
- **引擎内建**：Unity Test Framework（EditMode + PlayMode 两级，PlayMode 跑真实游戏循环）、Unreal Automation System（单元/功能/压力测试，Gauntlet 做 E2E；官方文档明确支持 CI 挂载）。大厂实践把「逻辑与表现分离」当可测性前提——逻辑层 CI 全自动，表现层人工。
- **Bevy 生态现状**（与我们直接相关）：
  - 官方姿势 = `App` 不挂渲染/窗口插件，`app.update()` 手动推进（官方 `tests/how_to_test_systems.rs`、Unofficial Bevy Cheatbook）——warden 的 `test_app()` 就是这个模式，外加固定 timestep + 种子化随机，比一般社区用法更严格。
  - headless 盲区有文档印证：社区工具明确记录「鼠标位置存 Window 里，headless 鼠标测试跑不了」——与我们 UI 点击泄漏 bug 的教训一致（解法也是同款：测试里手造 Window/光标）。
  - **视觉快照测试**：Bevy 官方截图测试仍在路线图讨论中；社区 PoC [bevy_geppetto](https://github.com/snendev/bevy_geppetto)（快照/E2E 测试 runner）是目前唯一成型方向。这是我们视觉验证缺口的对标方案。

### 2. AI 玩游戏找 bug（自动化 playtest 的三级演化）

行业把它总结为四层架构：感知 → 决策 → 动作 → 评估，技术演化「规则脚本 → DRL 自博弈 → LLM 生成式智能体」：

| 代际 | 代表 | 能力 | 局限 |
|------|------|------|------|
| 脚本机器人 | 传统 QA 回放脚本 | 确定性好、可回归 | 脆、覆盖窄、维护贵，游戏更新即失效 |
| DRL 自博弈 | [Wuji](https://arxiv.org/html/2509.22170v1)（腾讯）、[OpenAI Five](https://arxiv.org/abs/1912.06680) | 能玩到超人水平、暴露非显然策略/漏洞 | 训练贵、奖励工程、任务一换就重训 |
| LLM agent | [TITAN](https://arxiv.org/html/2509.22170v1)、[Voyager/Cradle/SIMA](https://github.com/git-disl/awesome-LLM-game-agent-papers) | 零样本理解任务语义、可泛化 | 感知受 token 限制、长任务链（20+ 步）弱、有误报 |

**TITAN（2025，商用 MMORPG 上的 LLM 测试框架）是目前最有说服力的数据点**：20 个任务、2 款商用 MMORPG——任务完成率 95%（人类 100%、ReAct 83%、Wuji 82%）；种子 bug 检出 ~82%（人类 ~18%！）；还发现 4 个未知 bug，已部署进 8 条真实 QA 流水线。其架构四件套值得抄：感知抽象（压缩游戏状态给 LLM）、动作优化（模板+RAG 推荐动作）、反思推理（进度监控+覆盖记忆）、问题诊断（crash/任务状态/执行时间三类判据）。注意其误报率 ~30%（主要来自状态抽象不完美），且对「重反应、轻叙事」品类效果会掉。

**产业侧**：[modl.ai](https://modl.ai/)（modl:test，AI bot 探索游戏找 bug/性能问题，无需 SDK，出 [State of Games QA 报告](https://modl.ai/state-of-games-qa-report)）；[Eidos-Montréal 官方博客](https://www.eidosmontreal.com/news/automated-game-testing/)讲经典游戏 AI+ML 做功能测试的组合；[AWS 教程：Bedrock 搭 Unity QA agent](https://aws.amazon.com/blogs/gametech/building-an-ai-game-testing-agent-with-amazon-bedrock/)；[Square Enix 宣布 2027 年用 AI 做 QA](https://www.reddit.com/r/Games/comments/1opyjas/square_enix_reveals_plans_to_use_ai_to_qa_test/)（「一个测试员指挥一支 agent 军队」）。

**平衡/难度测量**：LLM 不必玩得好也能当测量仪——[LLMs May Not Be Human-Level Players, But They Can Be Testers (ACM)](https://dl.acm.org/doi/10.1145/3748634) 用 LLM 模拟玩家测难度；OpenAI Five 自博弈揭示了职业圈没见过的策略与利用（转身 Roshan 时机、视野利用等，Berner et al. 2019 + Smerity 分析）——自博弈是**平衡读数**的上限形态。⚠️ 网传「Valve 拿 Five 做回归测试」**未找到任何可索引来源，按传闻处理**。

### 3. AI 参与开发时的测试（对我们最直接相关）

- **[Meta TestGen-LLM（2024）](https://arxiv.org/pdf/2402.09171)**——LLLM 生成/改进单元测试的产业级证据：生成的测试 75% 能编译、57% 稳定通过、25% 涨覆盖；整体改进了 11.5% 的目标类，**73% 的建议被工程师接受进生产**。关键设计：生成后自动过「能编译 → 能通过 → 涨覆盖」三道闸再提审，不直接信任模型输出。
- **与我们的红绿验证同构**：我们要求新回归测试「撤掉修复必须转红」，本质就是给 AI 产物加机器可验的闸门——行业证据支持这套纪律有效且必要。
- **验收句可执行 = BDD 在游戏里的变体**：TITAN 综述把行为驱动开发（BDD）列为游戏自动化测试路线之一；我们能力卡的「验收句必须数字化可执行」就是 BDD 的场景化，方向与行业一致。
- **AI 开发的特有风险**（结合本团队实践，非外部来源）：AI 写的测试容易「跟实现同错」（test 和 code 同一模型同一偏见）——对冲手段：验收句先于人审钉死、测试从需求文档（requirements §）反推而非从代码反推、真机日志做事后交叉验证（warden 25 分钟会话还原即是）。

### 4. 对本团队的可迁移结论

| 我们已有 | 行业对标 | 评价 |
|----------|----------|------|
| headless `test_app()` + 固定 timestep + 种子化 | Bevy 官方 how-to、UTF PlayMode | 已达主流做法，且更严格 |
| 能力卡验收句 → 回归断言 | BDD | 同构 |
| 红绿验证（撤修复必红） | TestGen-LLM 三道闸 | 同思想 |
| BAL1/BAL2 headless 自动打局读数 | 自动化 playtest / LLM 测难度 | 雏形对应 Wuji 式脚本 bot 一代 |
| 日志仪表还原真人会话 | Eidos/AWS 的 observability 路线 | 已用上（25 分钟会话还原案例） |
| **视觉快照测试（缺）** | bevy_geppetto 快照测试 / Unity snapshot | 下一步候选，可立卡 |
| **E2E agent 玩游戏（缺）** | TITAN / modl.ai / AWS Bedrock | 远期；需先把感知抽象（状态快照喂 LLM）做出来 |

### 4.1 ⚠️ AI 视觉验收纪律（2026-09-18 实战翻车教训，硬规则）

warden ART2 视觉验收重做时连翻两车，教训升格为硬纪律：

1. **截图前激活目标窗口**：macOS 对被遮挡窗口暂停 Metal 绘制，`screencapture` 会截到
   黑视口（标题栏正常、内容全黑）——后台跑的游戏必须先置前台再截。
2. **视觉模型判定必须中性提问**：提示词里禁止出现预期对象（「塔顶是否有弩」「旗帜缺陷」这类词
   全是毒药）——实测引导性提问能让视觉模型对着纯黑图编出「两座石塔+床弩+旗帜」的完整描述，
   且细节自信、数量具体，完全无法从行文辨别真伪。
3. **地面真相交叉核对缺一不可**：视觉结论必须与日志/场景定义逐项对数（dash 的 gold/wave ↔ 图中
   UI 数字；场景定义的对象数 ↔ 图中计数）——上例中 UI「金币: 100」与日志 gold=100 吻合、
   2 塔 2 弩与场景定义吻合，才算通过；对不上的结论一律按未验证处理。
4. 推论：TEST2 的自动视觉回归优先做**像素 diff 基线**（机器对机器，无幻觉面），
   AI 视觉读图只作辅助解释层，不单独作为验收判据。

## 参考

- [Leveraging LLM Agents for Automated Video Game Testing (TITAN, arXiv 2509.22170)](https://arxiv.org/html/2509.22170v1)
- [Dota 2 with Large Scale Deep RL (Berner et al. 2019)](https://arxiv.org/abs/1912.06680) / [Smerity 分析](https://smerity.com)
- [Meta TestGen-LLM (arXiv 2402.09171)](https://arxiv.org/pdf/2402.09171)
- [A Survey on LLM based Game Agents (awesome list)](https://github.com/git-disl/awesome-LLM-game-agent-papers)
- [LLMs May Not Be Human-Level Players, But They Can Be Testers (ACM)](https://dl.acm.org/doi/10.1145/3748634)
- [Human-AI Collaborative Game Testing with VLMs (arXiv 2501.11782)](https://arxiv.org/html/2501.11782v1)
- [modl.ai](https://modl.ai/) / [State of Games QA](https://modl.ai/state-of-games-qa-report) / [Eidos-Montréal: Automated Game Testing](https://www.eidosmontreal.com/news/automated-game-testing/)
- [AWS: Building an AI game testing agent with Bedrock](https://aws.amazon.com/blogs/gametech/building-an-ai-game-testing-agent-with-amazon-bedrock/)
- [Square Enix 2027 AI QA（Reddit 转述）](https://www.reddit.com/r/Games/comments/1opyjas/square_enix_reveals_plans_to_use_ai_to_qa_test/)
- [WeTest: Game AI 自动化测试演化与市场分析](https://www.wetest.net/blog/game-ai-automated-testing-technology-evolution-market-analysis-1171.html)
- [Unity Test Framework 官方指南](https://unity.com/how-to/automated-tests-unity-test-framework) / [Unreal Automation 官方文档](https://dev.epicgames.com/documentation/unreal-engine/automation-test-framework-in-unreal-engine) / [Andrew Fray on smoke tests](https://andrewfray.wordpress.com/tag/test-automation/)
- Bevy：[官方 how_to_test_systems](https://github.com/bevyengine/bevy/blob/main/tests/how_to_test_systems.rs) / [Cheatbook 系统 testing](https://bevy-cheatbook.github.io/patterns/system-tests.html) / [headless 模式指南](https://taintedcoders.com/bevy/how-to/headless-mode) / [bevy_geppetto 快照测试 PoC](https://github.com/snendev/bevy_geppetto) / [Bevy integration testing toolkit（含 headless 鼠标盲区记录）](https://github.com/haihala/Bevy-integration-testing-toolkit)
