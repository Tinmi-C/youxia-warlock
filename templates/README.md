# 脚手架模板

- `game-project/` — 新游戏项目骨架（含 `AGENTS.md` 项目模板）——**已退役路线**（手写引擎），仅作历史参照
- `bevy-game/` — **新游戏默认模板**：Bevy 0.19 完整底座 + 插件化骨架 + 回归测试 + 能力卡工作流（AI 协作原生，ADR-0004）

新建项目时复制对应模板，而不是从零手搓，保证三人项目结构一致、AI 上下文规范统一。

```bash
# 新游戏起步（Bevy 路线）：
cp -r templates/bevy-game games/<game-name>
cd games/<game-name>
# 改 Cargo.toml 的 name/description → git init → 按 docs/capability-cards.md 工作流开发
```
