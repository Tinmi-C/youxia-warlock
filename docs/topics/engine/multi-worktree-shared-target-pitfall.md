---
title: 同包多 worktree 共享 CARGO_TARGET_DIR 会互踩编译产物（并行子智能体批次）
type: pitfall
topic: engine
date: 2026-10-22
author: AI（Phase 1 并行批次实证，SPL/RPL 两线独立踩中）
status: done
tags: [cargo, worktree, parallel, build, ai-workflow]
related: ["[[topics/engine/warbase-wc3-alignment-plan|warbase 魔兽式底座补全方案]]"]
---

# 同包多 worktree 共享 CARGO_TARGET_DIR 会互踩编译产物

**现象**：六个子智能体在六个 git worktree 并行开发同一 monorepo，为省磁盘共享主仓 `CARGO_TARGET_DIR`。两条线先后出现「源码完好却编译报错」的假象：RPL 线链到了别的 worktree 的 rlib，报出本分支不存在的 `UnitDef.mana` 字段错误（那是 SPL 线正在写的字段）；SPL 线两次「编译报错但源码完好」，按两次失败纪律停手排查后才定位。

**根因**：同名包（warbase）在不同 worktree、不同 feature 组合（`--no-default-features` 门禁 vs 默认全量）下并行编译，产物落在同一 target 目录的单一路径——cargo 的包级锁只保证不并发写，不保证「你上次编的还是你的版本」：A 编完 no-default 版，B 接着编 default 版覆盖同一路径产物，A 的下一次增量编译就链到了 B 的半成品/异feature 产物。指纹与产物错位，报错误导到源码层。

**解决**：每条并行线用 worktree 私有 target（`wt-<domain>/target`，gitignored）；共享缓存只在「串行或同 feature」场景成立。

**反思**：并行批次的基建（编译缓存策略）要预先验证，不能沿用串行时代的省磁盘直觉——「正确性优先于磁盘」。AI 并行开发工作流里，这类「环境级故障伪装成代码级故障」的坑尤其危险：agent 容易往源码上叠补丁。两次失败纪律（停下排查环境而不是改代码）在这两线上都正确触发了。

**变种二（2026-10-28 实证，更隐蔽）**：全局 `~/.cargo/config.toml` 的共享 target 会缓存**任何 worktree 里编译的成员包二进制**，而 `env!("CARGO_MANIFEST_DIR")` 在编译期烙死绝对路径——worktree 删除后，主仓跑 `cargo test` 可能复用陈旧二进制，测试进程里的地图/字体路径指向已删除目录，表现为「241 个测试集体地图加载失败」但单跑该测试全绿。指纹不区分源码路径时必然踩中。**解法**：`cargo clean -p <全部成员包>` 清缓存重编（46.8GiB 一起清掉）；根治=凡多 worktree 并行，编译一律显式私有 target（已入 AGENTS.md 纪律），协调者的主仓回归也不例外。
