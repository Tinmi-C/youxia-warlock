---
title: 踩坑：Windows 杀软误报 ahash 构建脚本导致 cargo 编译「拒绝访问」
type: pitfall
topic: engine
date: 2026-08-26
author: team
severity: medium
tags: [windows, cargo, build-script, antivirus, 360]
related: [topics/engine/bevy-plugin-and-code-reuse]
---

# 踩坑：Windows 杀软误报 ahash 构建脚本 → cargo 编译「拒绝访问」

## 现象

Windows 上首次 `cargo test` / `cargo build` 一个依赖 ahash 的 Rust 项目（任何 Bevy 项目都会带上 ahash），编译稳定卡在：

```
error: failed to run custom build command for `ahash v0.8.12`
Caused by:
  could not execute process `...\target\debug\build\ahash-xxx\build-script-build` (never executed)
Caused by:
  拒绝访问。 (os error 5)
```

且 `target\debug\build\ahash-xxx\` 下刚编译出的构建脚本 exe 会「凭空消失」。重试多次、限制 `-j` 并行都无效。

## 根因

不是代码问题、也不是 cargo 问题——是**杀软实时防护的启发式误报**。ahash 0.8 的构建脚本（build script）在 Rust 生态里出了名的容易被杀软误报；本机安装的 **360 安全卫士「主动防御」** 把 cargo 刚编译出的 `build-script-build.exe` 当成可疑程序拦截/删除，cargo 随后要执行它时拿不到文件 → `os error 5`。

排查线索（快速二分「环境 vs 代码」）：

- 最小带 `build.rs` 的工程能编译 → 说明 cargo 构建脚本机制本身没被拦；
- 其他 crate（blake3 等）的构建脚本 exe 能直接执行 → 说明不是全局限制；
- 只有 ahash 稳定失败 + exe 文件消失 → 锁定「特定文件被杀软处理」；
- `Add-MpPreference` 报 `0x800106BA` → 说明 Windows Defender 服务被禁用，真正在岗的是第三方杀软（本机 = 360）。

## 解决

在杀软里给编译目录加白名单/信任区（以 360 为例：设置 → 木马查杀 → 信任区 → 添加目录）：

- `F:\developSpace\warlock`（覆盖各项目的 `target/`）
- 工具链/缓存目录：cargo registry（本机 `F:\Rust`、PATH 里的 `D:\Rust`）

加完白名单重跑 `cargo test` 即通过。若还不行，临时关闭实时防护再编译。

## 反思 / 防坑

- **杀软在岗 ≠ 只能碰运气**：Windows 团队机器要主动把「开发目录 + Rust 工具链」加进杀软白名单，否则 Rust 编译经常被实时防护误伤。
- **报 `os error 5 / 拒绝访问` 先怀疑杀软，别先怀疑代码**：先跑一个最小 build.rs 工程验证 cargo 机制是否正常。
- 团队默认验证环境在 Mac/Linux；Windows 机器做开发时本坑会反复出现，已同步写进模板 `templates/bevy-game/README.md` 的踩坑备忘。
