---
title: Bevy 0.19 场景资产 Modified 会重建全部实例——一次性动画接线全灭（T-Pose）
type: pitfall
topic: engine/bevy
date: 2026-09-29
author: zcode-session
severity: high
tags: [bevy, animation, scene, hot-reload, t-pose]
related: ["[[topics/engine/bevy-macos-background-cpu-burn-pitfall]]"]
---

# Bevy 0.19 场景资产 Modified 会重建全部实例——一次性动画接线全灭（T-Pose）

## 现象

warden 骨骼角色（KayKit 游侠防御者/骷髅敌人）开局动画正常，**玩家一放置第二个同
GLB 的单位，场上所有角色立刻 T-Pose**（引擎侧：全部 AnimationPlayer 的
graph/playing 被清空、实体被换成新的无图播放器）。用户三次实机戳穿（T-pose #1、
#2、放置复现），前两次修复（驱动加载器自带播放器）都没打到真正的根因。

## 根因

Bevy 0.19 的 `bevy_world_serialization::world_instance_spawner_system`（WorldAssetRoot
新场景机制）监听 `AssetEvent::Modified`：一旦某场景资产收到 Modified，**该资产的
所有已生成实例会被整体拆掉重建**（热重载设计）。再次 `server.load` 同一 GLB 的
场景标签会触发该资产的 Modified → 重建 → 每个实例得到**全新的加载器
AnimationPlayer**，而我们的接线标记（EnemyAnim/RangerRig）在首次接线时已被消费
删除 → 无人再给新播放器挂图 → 永久 T-pose。

排障过程中被反复误导的三个假象（记入方法论）：
- 「双帧像素差」只证平移不证姿势；「紧裁剪姿势对比」若裁错区域同样白测。
- 骨骼探针取「查询第一根骨」= rig 根骨，走路动画里它本来就不动——**探针目标本身
  要选对**（后来改按名字取 hips/upperarm 立刻看到 changed=true）。
- 引擎侧一切健康（播放器在播、AnimatedBy 链接在、曲线 ID 24/24 命中骨骼 ID）
  仍可能全灭——因为"健康"快照和死亡时刻之间隔了一次重建。**必须用生命周期
  观察器（On<Add/Remove, AnimationPlayer/GraphHandle>）抓死亡瞬间**，一次就抓到：
  放置后 40ms 内 -player/-graph 成批出现。

## 解决

**自愈式接线**（warden `EnemyAnim`/`RangerRig` 改造，2026-09-29）：

- 接线标记**常驻不消费**（仅"无对应动画剪辑"时移除）；
- 标记里记录 `wired: Option<{player: Entity, graph: Handle, node: idx}>`；
- 接线系统每帧体检：记录的播放器还活着且带图 → 跳过（每帧成本 = 一次
  `Query::get`）；死了（= 场景被重建）→ 子树 DFS 找新播放器 → **复用同一个图
  资产**（图归 app 所有，场景重建不影响）重挂重播 → 弓重新挂到新 handslot。
- 效果：用户连续放置 3 塔触发 3 次重建，全部 1 帧内自愈（`ranger wired` 5 次 =
  初始 2 + 重接 3），播放器全程 graph=true。

## 反思 / 防坑

- **凡是在"场景实例内部"挂状态（接线、挂件、覆盖材质），必须考虑实例随时被
  引擎重建**：挂点到实例外层的包装实体上，状态可重建；或如本例做自愈体检。
- 排"活着却看不见效果"的 bug，静态体检快照不够——**生命周期观察器抓瞬时事件**
  才能定位"谁在什么时候杀死了它"。
- 调试探针的观察目标要经过推敲（根骨 vs 肢骨），否则证据本身是伪证。
- 用户的实时操作日志（放置时间戳）+ 服务端日志对齐，是最快的相关性定位法。
