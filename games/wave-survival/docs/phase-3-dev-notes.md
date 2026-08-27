# 阶段三开发纪要 — 表现层（2026-08-27）

> 阶段三第一批三张能力卡（卡 12–14）的代码开发内容汇总。卡定义与验收句见
> `docs/capability-cards.md`；本文记录「实际做了什么、为什么这么做」。
> 对应提交：`a19621d` 立卡 → `14322a9` review 决议回写 → 3×feat → 真机验收修正×4 → 本纪要收尾，已推送 origin/main。

## 总览

| 维度 | 阶段二收尾 | 阶段三收尾 | 变化 |
|------|-----------|-----------|------|
| `src/` 源码 | 1146 行 | **1685 行** | +539 行 |
| 回归测试 | 843 行 / 33 个 | **1048 行 / 38 个** | +205 行 / +5 个 |
| 提交数 | — | **12 个**（3 feat + 1 资产修复 fix + 2 行为修复 fix + …） | 全部已推送 |

新增能力：**玩家人形骨骼动画（动才播）、怪物三分化蒙皮模型（色·体双辨识）、击中白闪衰减可见化**。
核心架构约束兑现：PresentationPlugin 只挂 `build_app()`，headless 测试应用零渲染依赖，
33 个旧测试全部原样通过。

## 架构：表现层隔离

```
src/plugins/presentation.rs（唯一新插件，~420 行）
  只在 build_app() 挂载 ──► headless 测试世界永不触碰 glTF / 材质 / 动画

  skin_player(Startup)        玩家根剥占位方块 → 子实体挂 WorldAssetRoot(hero.glb Scene(0))
       │ On<WorldInstanceReady> 观察者：给子树 AnimationPlayer play(0)+循环
  skin_new_monsters(Update)   Local<(graph,index)> 缓存懒加载 monster.glb；
                              生成 wrapper 子实体（缩放=尺寸/1.8×分型系数），根剥方块+挂 MonsterSkinned
  bind_monster_models(Update) 轮询子树出现 → 绑动画 → 克隆材质按 MonsterKind 染色(tint 0.65)
  apply_flash_visuals(Update) FlashAssets{HashMap<Entity,Vec<Handle>>} 按 owner 私有化材质，
                              emissive = base_color.lerp(WHITE, flash) 每帧重刷
  sync_walk_playback(Update)  AnimLink{graph,index,was_playing} 边沿驱动播放
```

关键解耦手段：wrapper 实体模式——`ChildOf(root)` 挂模型子树，逻辑端只见根上的
`WalkCycle { playing }` 数据位；表现端每帧读旗标驱动 AnimationPlayer。
`AnimLink` 组件统一寻址 graph/node/index，一个资源图多 owner 共享。

## 卡 12 HeroPresentation（feat `c07520d`）

- hero.glb 单 mesh+skeleton+单走路 clip（GLB JSON 块核实过）；场景经
  `WorldAssetRoot(GltfAssetLabel::Scene(0))` 实例化，`MODEL_Y_OFFSET=-0.5` 对齐脚底与物理球心。
- 动画链：`AnimationPlayer.play(idx)` 先于 `AnimationGraphHandle` 插入（同帧 Deferred 无碍）。
- 「动才播」数据侧：`update_walk_cycle` 挂系统链尾、自播种 PrevTranslation，
  位移速度 < 0.02/s 视为静止清旗标；`clear_walk_on_pause` 改为「任何持有
  WalkCycle 的实体」且仅在非 Playing 态运行——R 重开/暂停全员定格。

## 卡 13 MonsterPresentation（feat `0c9119a`）

- 三种怪共用 monster.glb；`variant_visual_scale()` 方案 C：
  Grunt ×1.0 / Runner ×0.85 / Tank ×1.25，叠加基础归一（cube_size/1.8）后写 wrapper scale。
- 分型辨识=颜色 tint（base_color.mix(kind_color, 0.65)）＋体格缩放双编码；
  材质克隆缓存 `MonsterSkinCache keyed (material_id, kind_ordinal)`，每型一份不膨胀。
- 占位方块移除与玩家同一套手法（首次真机跑才发现怪物漏了这一步，见下文 fix 清单）。

## 卡 14 HitFlashFeedback（feat `0ef1ca0`）

- 数据侧已有 `Visual.flash ∈ [0,1]` 与衰减公式 `max(1 − 4.0·dt, 0)`（contact.rs 常量钉死）。
- 表现侧第一次让它可见：命中时 emissive 从 base_color 线性冲向白色再随衰减回落；
  按 owner 私有克隆材质实例，杜绝共享材质串染。

## 真机验收修正（首次 cargo run 抓出的潜伏账单）

四轮迭代，5 个 fix 提交：

| 提交 | 问题 | 根因 |
|------|------|------|
| `a2c3493` | 启动即崩 ×3 | ① VfxPlugin 忘挂 `HanabiPlugin`（`Assets<EffectAsset>` 不存在，卡 9 欠账）；② egui UI 系统必须跑 `EguiPrimaryContextPass`（bevy_egui 0.42 新调度器，挂普通 Update 直接 panic，卡 11 欠账）；③ 模型路径应为 `models/*.glb`（spike 在 assets 根目录，本项目有 models/ 子目录） |
| `110456a` | 相机镜像/待机僵直/怪物套方块/穿模 | 相机移 -Z 南向高位（模型面朝 +Z ⇒ 背影视角、W=入屏）；怪物补剥方块；怪物改 **Dynamic+零重力+锁旋转**——kinematic-vs-kinematic 无碰撞是 rapier 设计行为，「走完收尾」待机首版亦在此批 |
| `f14de81` | 左右反向 / 不掉血 | 相机换边导致屏幕左右与世界 X 天然镜像 → A/D 取反；Dynamic 化副作用：玩家碰撞球把怪顶在 0.7 距离（两球半径和），够不着 0.40 咬合判定 → **玩家设幽灵分组** `GROUP_2/NONE`，怪物 `GROUP_1/GROUP_1` 组内互推保留 |
| `2280cf7` | 待机仍僵迈步姿势 | 「走完收尾」方案双重失败：纯走 clip 的循环末端姿势本身还是迈步接触姿态（视觉不可区分）；且模型加载空窗期会提前消费停边沿，观察者随后又开始循环。改为**确定性定格**：空闲态每帧幂等强制 pause+seek_to(0)，每次停下同一站架帧 |

人工验收结论（2026-08-27）：A/D 直觉方向 ✓、贴脸掉血恢复 ✓、确定性定格观感可接受 ✓。
残余项见文末挂起清单。

## 回归测试 +5 个

| 组 | 测试 | 钉死的验收句 |
|----|------|-------------|
| WalkCycle(3) | `walk_cycle_plays_only_while_moving` | 有输入⇒playing=true 且位置推进；松键⇒下一帧 false |
| | `walk_flag_clears_while_paused_even_with_keys_held` | 非 Playing 态按键也强制清旗 |
| | `wave_monsters_spawn_with_walk_flag_up` | 刷怪契约：怪物出生即 playing=true |
| Scales(1) | `variant_visual_scales_match_scheme_c` | G/R/T 三档缩放系数逐值锚定 |
| Flash(1) | `flash_decays_predictably_and_clamps_at_zero` | 逐帧按 `max(f−4dt,0)` 收敛并钳位零 |

既有断言漂移：三条旧 flash 断言由「命中后 ≈1.0」修订为卡 14 review 定稿的
衰减公式带宽 `hit_frame = max(1 − FLASH_DECAY_RATE/60, 0)`（均注明出处）——
这是唯一一处主动改老测试，其余 30 个逐字未动。

## 版本 API 发现（编码期/调试期解决）

- Bevy 0.19 glTF 场景实例化走 `world_serialization`：`WorldAssetRoot(Handle<WorldAsset>)`
  + `On<WorldInstanceReady>` 观察者参数；`GltfAssetLabel::Scene(0)/Animation(0).from_asset(path)`。
- `AnimationGraphHandle(pub Handle<AnimationGraph>)` 才是 Bundle 成员——裸 Handle 不是 Bundle，
  需 `(AnimationGraphHandle(h.clone()),)` 一元组包裹（编译期抓到）。
- `MeshMaterial3d<M>` 是泛型组件，remove 时必须带齐泛型参数：`remove::<MeshMaterial3d<StandardMaterial>>()`。
- `LinearRgba::lerp` 需要 `use bevy::math::VectorSpace` trait 在作用域内。
- bevy_egui 0.42：UI 系统注册到 `EguiPrimaryContextPass` 而非 Update，
  否则 `run_egui_context_pass_loop_system` panic（TexturesDelta 未处理）。锁文件里同时存在
  bevy_egui 0.40.1（inspector 间接依赖）与 0.42.0（直接依赖），排障时注意看对包源码。
- bevy_hanabi 0.19：需显式 `app.add_plugins(HanabiPlugin)`（单元结构体无 Default），
  否则 `Assets<EffectAsset>` 资源不存在。
- bevy_rapier3d 0.36：kinematic-vs-kinmatic 不产生接触响应；穿透治理用
  `CollisionGroups::new(memberships, filters)`（Group 位标志，组件可直接进 spawn bundle）
  + `GravityScale(0.0)` + `LockedAxes::ROTATION_LOCKED` 平面化 Dynamic 身体。
- `ActiveAnimation`：`.repeat()=Forever`；`set_repeat/replay/pause/resume/is_paused/
  repeat_mode/seek_to` 全套可用；finished(Never) 动画停在完成点不再采样。

## 挂起清单（有意不修，均已记 AGENTS.md）

1. **真·待机动画**：等带 idle clip 的角色资产到位 → 立「动画状态机」卡统一管理 walk/idle/attack。
2. **怪物朝向系统**：现固定面朝 +Z，多方向行走露侧脸/背面 → 单独立卡。
3. **残余穿模**（人判定影响不大）：咬合瞬间交叠属幽灵分组设计预期；重度围堵怪群短暂互渗
   （速度直写+碰撞球小于模型包围盒）。候选：球→胶囊校准、CCD、调接触刚度。

## 下一步

「UI 正式化」立项（HUD 替换 debug 文本）或「怪物朝向系统」，优先级由人定；
音效仍为 GDD 后置项。
