# <游戏名>

> 从 `templates/bevy-game/` 复制而来。Bevy 0.19.1 + AI 协作原生工作流。
> 项目级 AI 约定见 [AGENTS.md](AGENTS.md)；能力卡工作流见 [docs/capability-cards.md](docs/capability-cards.md)。

## 快速开始

```bash
cp -r templates/bevy-game games/<game-name>   # 复制模板
cd games/<game-name>
# 1. 改 Cargo.toml 的 package.name / description
# 2. git init && git add . && git commit    （一卡一提交，见下）
cargo run                                     # 跑起来看示例场景
cargo test                                    # 行为一致性回归测试
RUST_LOG=info cargo run                       # 观察通道：日志仪表
```

## 目录结构

```
src/
  main.rs          # 入口：只调 lib 的 build_app()，不堆业务代码
  lib.rs           # build_app()：App 组装（main 与测试共用同一构造）
  states.rs        # GameState 状态机（系统开关面板）
  components.rs    # 组件 = 纯数据（名词）；新机制 = 新组件 + 新系统
  resources.rs     # 资源 = 全局单例数据
  plugins/
    game.rs        # GamePlugin：游戏领域的所有系统（一个领域一个插件）
    debug.rs       # DebugPlugin：观察通道（日志仪表 / F12 截图）
  systems/
    camera.rs      # 相机 + 灯光 + 地面（环境）
    player.rs      # 玩家 WASD 移动（示例系统，对应 MoveSystem 能力卡）
tests/
  behavior.rs      # 行为一致性回归测试（验收闭环的可执行化）
assets/
  models/ textures/ audio/ fonts/ ui/   # 资产按类型分目录
docs/
  capability-cards.md  # 能力卡工作流 + 卡模板（AI 开发的核心约定）
AGENTS.md          # 给 AI 智能体的项目上下文（AI 协作方式）
```

## 操作

- `WASD` 移动示例方块（斜向不超速）
- `P` 暂停 / 继续（状态机演示）
- `F12` 截图到 `./screenshot.png`（验收证据）
- 日志仪表：`RUST_LOG=info cargo run`，每 2 秒一条 `[dash] fps≈.. state=.. entities=..`

## 踩坑备忘（团队已踩过，模板已内置规避）

| 坑 | 规避 |
|---|---|
| glTF 内嵌 JPEG 贴图加载失败 | Cargo.toml 已加 `jpeg` feature |
| Linux 无头运行 panic `libxkbcommon-x11.so` | 安装 `libxkbcommon-x11-0`（无显示器验证用 Xvfb + lavapipe） |
| Bevy assets 目录相对可执行文件解析 | 用 `cargo run`（自带 CARGO_MANIFEST_DIR）；直跑二进制需设置该环境变量 |
| Bevy 版本 breaking changes | 锁 `0.19.1`；升级前读 release notes |

## 工作流（和 AI 一起开发）

1. **立卡**：新功能先写能力卡（`docs/capability-cards.md` 模板）：接口 / 行为 / **验收句（数字化、可执行）**。验收句写不出来的功能 = 还没理解清楚，不开工。
2. **AI 实现**：AI 按卡实现（新组件/新系统，老系统零改动）。
3. **人验收**：按验收句验收（看效果 + 跑 `cargo test`）。
4. **回归钉死**：验收句转成测试加进 `tests/behavior.rs`。
5. **一卡一提交**：Conventional Commits（`feat:` / `fix:` / `docs:` / `refactor:`）。

## 插件决策准则（重要：避免重复开发）

**用插件还是自己写？判断标准一条：这个功能的"正确性"是客观的还是主观的？**

| 功能性质 | 例子 | 做法 |
|---------|------|------|
| **客观正确**（技术难题，有标准答案） | 物理、碰撞、角色控制器、粒子、缓动、网络、寻路 | **找插件引用，不自己写**（bevy_rapier、bevy_hanabi…） |
| **主观设计**（玩法手感，无标准答案） | 移动手感、波次节奏、战斗数值、掉落表 | **自己写**，按能力卡沉淀 |

**移动类功能要拆开看**：物理机制（不穿墙/重力/爬坡）→ 插件；手感规则（速度/跳跃/冲刺）→ 自己调参。

**选型三原则**：
1. 版本对齐 Bevy 0.19（[bevydepy.com](https://bevydepy.com/popular?bevy=0.19) 按版本过滤）
2. 维护活跃（近半年有更新，Bevy 迭代快，停更插件很快失配）
3. 作者可信（官方 / 知名组织优先）

## 代码沉淀准则（第二次用到才抽）

**自己写的玩法模块怎么沉淀？**

| 出现次数 | 做法 |
|---------|------|
| 第 1 次 | 写进当前游戏里正常用，**不提前抽象** |
| 第 2 次 | 抽成独立 crate，沉淀到 `engine/` 目录（独立 git 仓库 + 版本号），本游戏通过 `path` 依赖引入 |
| 3 次以上 | crate 稳定后按需升版本，各游戏独立升级 |

**两层免重复全景**：客观技术 → 引用生态插件（不重复造别人的轮子）；主观玩法 → 沉淀成自己的 crate（不重复写自己的轮子）。详见团队知识库 `docs/topics/engine/bevy-plugin-and-code-reuse.md`。
