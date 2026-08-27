---
title: PowerShell 文本管道写坏 UTF-8 源码——Set-Content 后 rustc 报 stream did not contain valid UTF-8
type: pitfall
topic: engine/tooling
date: 2026-08-27
author: AI，youxia 已确认沉淀
severity: high
tags: [powershell, encoding, utf8, windows, tooling]
related: [topics/engine/bevy-windows-antivirus-build-pitfall, topics/engine/bevy-plugin-and-code-reuse]
---

# PowerShell 文本管道写坏 UTF-8 源码

## 现象

wave-survival 阶段三开发中，用 PowerShell 管道对 `tests/behavior.rs` 做批量文本替换：

```powershell
(Get-Content path/to/behavior.rs -Raw) -replace 'old', 'new' | Set-Content path/to/behavior.rs
```

随后 `cargo test` 直接编译失败：

```
error: stream did not contain valid UTF-8
```

检查文件字节发现：原本的 UTF-8 中文注释（含 em-dash「—」等非 ASCII 字符）变成了
非法字节序列（如 `[226,128]` 截断残留），整个文件编码被破坏。

## 根因

Windows PowerShell 5.1 的文本管道不是「字节透传」而是「解码→重编码」：

1. `Get-Content` 无 `-Encoding` 参数时按**系统 ANSI 代码页**（中文系统 = GBK）解码
   BOM-less UTF-8 → 中文/em-dash 全部变成乱码字符；
2. `-replace` 在乱码字符串上操作（即使正则恰好命中 ASCII 部分，无关的中文字符已经烂了）；
3. `Set-Content` 默认再按 ANSI 编码写回 → 双重破坏后落盘的字节既不是合法 GBK 也不是合法 UTF-8。

同理危险的操作还包括 `Add-Content`、`Out-File`（默认 UTF-16 LE）以及任何
「读文本→改→写回」的组合。pwsh 7+ 默认 UTF-8 好很多，但本机五分钟内分不清装的是哪个，
按最坏情况设防才是工程做法。

## 解决

当场处置（本次实际操作顺序）：

1. `git checkout -- <file>` 恢复到最近提交的完好版本；
2. 改动重新用 AI 文件工具（read 定位 + edit 精确字面量替换）应用——编辑器类工具
   按 UTF-8 字节读写，不经编码猜测；
3. 事后验证：`cargo test` 全绿 + `git diff` 确认只有预期行变化。

防复发规则（已写进 wave-survival `AGENTS.md` 已知问题）：
**改文件一律走 AI 文件工具（write/edit）或显式显式编码的原子 API**
（如 pwsh 7 `Set-Content -Encoding utf8NoBOM`、.NET
`[IO.File]::WriteAllText($p, $s, [Text.UTF8Encoding]::new($false))`）。

## 反思 / 防坑

- 根因属于推断性质（基于现象的最合理解释：GBK 双重转换路径），未逐字节复核编解码器
  行为——标记**待验证**；但「PowerShell 默认编码 ≠ UTF-8 导致往返破坏」这一结论本身
  已是 Windows 社区公论，可直接作为团队共识使用。
- 流程教训：**文本管道适合读和过滤，不适合在本仓库做写回**。唯一安全例外是明确知道
  目标文件为纯 ASCII 且目标机器 pwsh 编码可控——判断成本高于收益，不如一律禁用。
- 与 [[topics/engine/bevy-windows-antivirus-build-pitfall]] 同属「Windows 环境税」：
  团队文档里依赖工具链行为的规则要写明机器假设，跨平台协作时最先炸的就是这类隐形约定。
