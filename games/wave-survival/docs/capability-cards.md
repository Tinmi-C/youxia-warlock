# 能力卡工作流（AI 装配层 · 项目层落地）

> 团队理念（ADR-0002 / 方向纪要 §3）：自研 = AI 装配层，不是引擎轮子。
> 装配层的三个支柱在项目里的落地：**能力卡（WHAT）→ 验收闭环（证明做对了）→ 观察通道（看见在做什么）**。
> 团队知识库里的卡与规范：`docs/_private/learning/capability-cards/`（个人卡草稿区）、`docs/topics/`（成熟卡）。

## 为什么是能力卡

每学一个功能、每做一个系统，都写一张「能力卡 + 验收句」。**写得出来 = 理解到位；验收句能执行 = 做得对**。卡同时是给 AI 的「需求规格」——AI 按卡实现，人按验收句验收，AI 和人的沟通成本降到最低。

## 卡模板

```yaml
能力卡: <名称>
类型: system | component | asset | gameplay-system | architecture | ...
接口:
  输入: （它读什么：组件/资源/事件）
  输出: （它写什么：组件/资源/事件/命令）
行为: （每帧/每次做什么，写规则本身，不写目的）
验收句: （数字化、可执行、含边界条件——怎么自动/半自动验证它做对了）
挂载: （GameSet::X 阶段名 / 插件名——玩法进 GameSet，表现/工具进对应 Plugin）
依赖消息: （消息名数组，无则 []）
```

## 工作流（和 AI 一起开发）

```
立卡 ──► AI 实现 ──► 人验收 ──► 回归钉死 ──► 一卡一提交
 │           │           │           │
 │           │           │           └─ feat: <卡名>（Conventional Commits）
 │           │           └─ 跑游戏 + cargo test + 日志仪表对照验收句
 │           └─ 新机制 = 新组件 + 新系统，老系统零改动
 └─ 验收句写不出来 = 没理解清楚，先问人，不开工
```

## 本项目的卡清单

> **本文件只保留：状态总表 + 未闭环卡全文。** 已闭环卡的完整规格已移除——
> 它们的可执行真相在代码与 `tests/behavior.rs`（改坏了回归会红），历史版本
> git 可溯；在这里维护第二份规格只会漂移。
> 编号备注：历史原因有两个「卡 18」（18A=队友 ArtSourceSpike 草案、
> 18B=PlayerFacing 已完成），卡 20 留空；重编号等队友美术 WIP 落库后一并拍板。

| 卡 | 类型 | 状态 | 一句话要点 |
|----|------|------|-----------|
| 1 | system | ✅ 08-26 | WASD 移动；位移 = speed×Δt 帧率无关，斜向不超速 |
| 2 | gameplay | ✅ 08-26 | Space 挥砍距离衰减带（卡 29 起数值出自武器定义表） |
| 3 | gameplay | ✅ 08-26 | 波次三态；2+n / 1.1+0.08n / 30×(1+0.4n)；波间 3s |
| 4 | system | ✅ 08-26 | rapier 追踪怪，速度 1.1+0.08n |
| 5 | system | ✅ 08-26 | 贴脸判定扣血+白闪；死亡掉落与 despawn |
| 6 | system | ✅ 08-26 | 击杀掉金色补给，走近自动回血 |
| 7 | ui | ✅ 08-26 | 血条/波次文本/冷却条；死亡 GameOver / R 重开 |
| 8 | system | ✅ 08-26 | 出生→刷怪→击杀→死亡→重开一局闭环 |
| 9 | gameplay | ✅ 08-27 | Q 范围斩：半径 1.6 内全体 −60 / 冷却 5s（特效已由卡 23 升级） |
| 10 | gameplay | ✅ 08-27 | w3 起 Runner、w5 起 Tank；组成守恒（卡 19 起定义表接管） |
| 11 | ui | ✅ 08-27 | F1 调参面板实时改 Balance（卡 29 起武器数值为倍率语义） |
| 12 | presentation | ✅ 08-27 真机验收 | 玩家挂模型；「逻辑/皮相分离」先例（PresentationPlugin 只挂 build_app，headless 零渲染依赖） |
| 13 | presentation | ✅ 08-27 真机验收 | 怪物挂模型；tint+缩放双编码辨识；判定几何不动 |
| 14 | system | ✅ 08-27 真机验收 | flash 衰减 + 材质发光跟随（白闪真正可见） |
| 15 | presentation | ✅ 08-27 真机验收 `3a15cf2` | Heading 观测组件 + wrapper yaw 540°/s 最短弧平滑，物理零交互 |
| 16 | ui | ✅ 08-27 人工验收 `bb637fa`+`d44ec77` | 波次格子/Nova 条/暂停遮罩；rapier 步进随 GameState 门控 |
| 17 | tools+asset | 📝 队友草案（WIP 勿动） | Blender 洗白 + 转台批量出图流水线 |
| 18A | spike | 📝 队友草案（依赖 17） | AI 生成 vs 套装采购双路对比 |
| 18B | presentation | ✅ 08-27 真机验收 `f10df37` | 玩家面向移动方向，停步保持；derive_heading 只读不写 |
| 19 | data+system | 🔄 已实现 `31ab6f7`，待视觉验收 | 敌人定义表一行一怪 + Elite 变体（w6 起）+ 四模型换皮 |
| 20 | — | （留空） | 编号跳过，见上方编号备注 |
| 21 | presentation | 🔄 已实现 `a0b1207`，待视觉验收 | player_hunyuan 换皮 + walk/idle/attack/hit 四动画状态机 |
| 22 | presentation | 🔄 已实现 `b853dfe`，待视觉验收 | 怪物攻击/受击 clip；单次播放机制 owner 无关化 |
| 23 | presentation | 🔄 已实现 `def55a3`，待视觉验收 | Nova 四层爆发 + 镜头微抖（确定性） |
| 24 | presentation | ✅ 08-29 真机验收 `f64c464` | 前倾 + 步伐起伏 + 步频校准 1.6；英雄怪物同吃 |
| 25 | presentation | ✅ 08-29 真机验收 `1ebf2c7`+`947a932` | Mixamo run 接入（5 clip）；速度三态门控阈值 3.0 |
| 26 | gameplay | ✅ 08-29 真机验收 `5e12381`+`c3f24de` | Shift 疾跑 + Nova 改 Q；基速 5.0→2.5；表现层读实测速度 |
| 27 | gameplay | 🔄 已实现，待人工验收 | 攻击窗口 0.3s 移动 ×0.25（治边走边砍滑步） |
| 28 | presentation | 📝 草案（卡 27 根治路线，待喊开工） | mask 上下半身分层：边跑边砍 |
| 29 | gameplay | 🔄 已实现，待人工验收（SOP 试点 #1） | 武器定义表 + 扇形命中 + 攻击自动面向；Balance 倍率化 |
| 30 | presentation+asset | 🔄 已实现，待终审（SOP 试点 #2） | 武器手骨挂点 + 缩放补偿；两把占位武器经管线 landed |
| 31 | gameplay | 📝 草案（可选后置） | 投射物：手动步进 + 球 overlap，不上 rapier |
| 32 | presentation | ❌ 已否决（ADR-0006，由卡 33 替代） | ~~动画状态机迁 bevy_animation_graph~~：转场不支持输入条件比较，逻辑无法数据化，收益不抵复杂度 |
| 33 | presentation | 🔄 已实现，待人工验收 | 动画状态机表驱动（ADR-0007）：状态拓扑进表数据，加状态=加一条表数据，控制流一次写好；13 自测+59 回归+动画播放护栏全绿，真机过一遍 hero/Monster + Death 演示即闭环 |

## 未闭环卡全文（验收句在此，人验收照此执行）

## 卡 19：EnemyDefinitionTable（敌人定义表 + 首批四模型接入）

> 2026-08-27 立卡。资产体检：green_blob/mushnub/mushnub_evolved/yeti 四模型各带
> 9 clip（attack/Dance/death/hit/idle/Jump/No/walk/Yes，walk 可当跑环），脚底原点
> y=0，身高 0.90/1.08/1.40/1.68 天然成缩放梯度。

```yaml
能力卡: EnemyDefinitionTable（敌人定义表——spawn 读表 + 精英变体 + 三老怪换皮）
类型: data + system + presentation
状态: 已实现（feat 31ab6f7，2026-08-28，48 回归全绿），待人工视觉验收
设计来源: GDD 点子池[变体派生]；风格圣经 §2 双槽（族绿 #7AA25C / 深红 #B03A2E）§6.2 族级批次
挂载: GameSet::Spawn
依赖消息: []
范围与默认方案:
  - 定义表（核心交付）: 单处常量表（const 行数组），每行 = { kind, 模型路径, 缩放,
    色槽颜色, hp倍率, speed倍率, walk clip 序号 }——「新怪 = 表加一行」
  - 三老怪换皮: grunt→green_blob / runner→mushnub / tank→yeti
  - 新精英变体: kind=Elite，mushnub_evolved、深红槽 #B03A2E、scale=grunt×1.3、
    hp×2.0、speed×0.85（慢而硬）；walk clip 序号 7
  - 精英出场节奏: wave≥6 起每波精英数 = w−5、上限 grunt 数一半；w5 及以前不变
数据边界: 波次公式/Chasing/AI 零改动；elite 就是「数值不同的一行」不引入新行为
数字化验收句:
  1. 零改动红线: 既有回归逐字全绿（含 w5 组成 (4,2,1)）
  2. 定义表驱动等价: grunt 行数值 × 波次公式 == 旧硬编码值（headless）
  3. 精英节奏: 强刷 w6 → Elite 数 ==1、hp==2×行值、speed==0.85×行值；w5 → ==0
  4. 换皮正确性: wrapper scale/色槽/clip 序号 == 表值（presentation 读表不读散常量）
  5. 视觉: 四怪同框可辨、精英红一眼识别、walk 动画正常
```

## 卡 21：HeroV2（玩家换皮 + 四动画状态机）

> 2026-08-28 立卡即实现。素材 = 队友卡 17 流水线首批产出 player_hunyuan.glb
> （4 clip：attack/hit/idle/walk；身高 1.33m；clip 时长 4.67/1.50/1.79/1.38s）。

```yaml
能力卡: HeroV2（玩家资产升级——hero.glb → player_hunyuan + 四动画状态机）
类型: presentation
状态: 已实现（feat a0b1207，2026-08-28，49 回归全绿），待人工视觉验收
范围与默认方案:
  - 换皮: 缩放 1.353（世界身高 ≈1.80m），脚底偏移约定不变
  - 动画状态机: WalkCycle.playing → walk 循环 / 停步 → idle，200ms 混合；
    Pause/GameOver → pause_all 冻结
  - 战斗动画: 挥砍沿 → attack 单次窗口 0.6s；受击沿 → hit 窗口 0.4s；
    attack 全长 4.67s 只播打击窗口
  - 观察纪律: 只读 WalkCycle/Attack/Visual，逻辑零接触
  - clip 序号钉死（glb 内字母序）→ pub 常量 + sanity 测试锚定
数字化验收句:
  1. 零改动红线: 既有回归逐字全绿
  2. clip 布局锚: HERO_CLIP_* 常量与 glb 实际布局一致（headless）
  3. 视觉: 真机四态可辨——走/停/挥砍/受击；F12 截图存证
  4. 身高: 与怪对峙轮廓比例肉眼正常（±10%）
```

## 卡 22：MonsterCombatClips（怪物攻击/受击动画接入）

> 2026-08-28 立卡即实现。「第二次用到才抽象」：卡 21 单次播放机制首次复用。
> 四模型 attack/hit 时长统一（0.38s / 0.29s），全程播放不截取。

```yaml
能力卡: MonsterCombatClips（怪物攻击/受击——定义表扩展 + 观察信号三路化）
类型: presentation + data
状态: 已实现（feat b853dfe，2026-08-28，49 回归全绿），待人工视觉验收
范围与默认方案:
  - 定义表扩展: attack_clip()=0 / hit_clip()=3（四模型共用 9-clip 布局）
  - 攻击信号: 距离电平触发——与玩家距离 ≤ CONTACT_DIST+0.15 即起攻，
    播放期间不重复触发
  - 受击信号: 复用 flash 上跳边沿 → hit 全程 0.29s
  - 通用化重构（核心红利）: 怪物走查迁入 AnimationTransitions——单次播放
    机制 owner 无关，sync 不再分玩家/怪物两套路径
  - 暂停语义不变: frame-0 定格逐字保留
数字化验收句:
  1. 零改动红线: 既有回归全绿；定义表加 attack/hit clip 锚
  2. 信号纪律: 逻辑组件只读，本卡零写入
  3. 视觉: 咬合起攻与掉血大体同步；被砍有受击反应；暂停全员定格
非目标: 怪物 idle 三态（挂起，等玩法场景出现）
```

## 卡 23：NovaJuice（Nova 打击感升级——四层爆发 + 镜头微抖）

> 2026-08-28 立卡即实现。API 依据 bevy_hanabi-0.19 官方 firework 示例核实。

```yaml
能力卡: NovaJuice（Nova 四层爆发 + 镜头微抖——纯表现层）
类型: presentation
状态: 已实现（feat def55a3，2026-08-28，49 回归全绿），待人工视觉验收
分层构成（同帧同点触发）:
  - 主冲击环: 速度 4.5-6.5、尺寸 0.10→0.03、金→橙→暗红，160 粒 0.55s
  - 火花上抛: 30 粒亮金，重力 -9 拉回，0.5-0.8s
  - 贴地闪圈: 90 粒白金扁平快环，急扩急停 0.28s
  - 中心白闪: 24 粒白慢粒子 0.10s（重音不遮画面）
镜头微抖: trauma² 衰减 0.15s / 峰值 0.12；确定性 sin/cos；camera.rs 零改动
事故备注: fmt 回滚曾误 checkout vfx.rs，发现后重写——回滚名单须逐次核对
数字化验收句:
  1. 零改动红线: 既有回归逐字全绿
  2. 触发纪律: 仍只读 NovaFired 消息，四层同帧触发（headless 断言不变）
  3. 视觉: 四层肉眼可辨；白闪一闪即逝；抖动有感不晕
非目标: 音效（搁置）、hit-stop、色板重定
```

## 卡 24：LocomotionFeel（走路观感——前倾 + 步伐起伏 + 步频校准）
> ✅ 已闭环（feat `f64c464`，08-29 真机验收）。规格真相在代码+回归，本处不再保留全文。

## 卡 25：HeroRunClip（Mixamo 跑步动画接入）
> ✅ 已闭环（feat `1ebf2c7` + fix `947a932`/`c3f24de`，08-29 真机验收）。规格真相在代码+回归。
> 已知校准债：`RUN_CLIP_AUTHORED_SPEED` 4.0→2.8（卡 30 验收反馈③，已随卡 30 落地）。

## 卡 26：SprintRebind（Shift 疾跑 + Nova 改键 Q）
> ✅ 已闭环（feat `5e12381` + fix `c3f24de`，08-29 真机验收）。规格真相在代码+回归。

## 卡 27：AttackRoot（攻击顿帧，治"边走边砍滑步"）

> 2026-08-29 立卡即实现（用户真机反馈）。治标止血；根治见卡 28。

```yaml
能力卡: AttackRoot（攻击窗口内移动阻尼）
类型: gameplay-feel
状态: 已实现（feat，2026-08-29，52 回归全绿），待人工验收
挂载: GameSet::Combat
依赖消息: []
行为:
  - 挥砍后 ATTACK_ROOT_WINDOW(0.3s) 内移动 ×ATTACK_MOVE_FACTOR(0.25)
  - 窗口锚定 Balance.slash_cooldown 实时值（F1 调冷却不破功）；
    冷却被调短于窗口时 clamp 全程顿帧
  - 疾跑中挥砍同样生效（×0.25×5.0 = 1.25，微动不僵死）
  - 表现层零改动：实测位移下降 → walk 速率自动变慢
数字化验收句:
  1. 挥砍后 0.167s 内位移 ≈ 2.5×0.25×elapsed（±0.05）
  2. 冷却剩 0.1s（< 窗口下限 0.15）时移动全速——顿帧是暂态不是 debuff
非目标: 上半身分层（卡 28）、攻击前摇/后摇取消
```

## 卡 28：UpperBodyLayer（上下半身分层，根治攻击滑步）【草案】

> 卡 27 的根治路线。API 已验证：Bevy 0.19.1 `AnimationGraph::add_target_to_mask_group`
> 按骨骼分组、动画节点带 mask。

```yaml
能力卡: UpperBodyLayer（遮罩分层动画）
类型: presentation（动画架构升级）
状态: 草案（待用户喊开工）
行为:
  - 骨骼两组（mixamorig:* 名归类）：上半身 Spine/Arms/Hands/Neck/Head，
    下半身 UpLeg/Legs/Foot/Toe
  - 下半身层: idle/walk/run 状态机；上半身层: 攻击/受击 one-shot 叠加
  - 效果: 边跑边砍 = 腿在跑手在挥
验收句:
  1. 移动中攻击：腿 walk/run 与上半身攻击 clip 同时可见
  2. 站立攻击：腿 idle 姿态，上半身挥砍
风险: 双层混合真机手感要调；mask 分组初始化要在模型就绪后做
```

## 卡 29：WeaponDefinitionTable（武器定义表 + 扇形命中）

> SOP《AI 特性开发标准流程 v1》（docs/topics/engine/ai-feature-pipeline-sop.md）试点第一单。
> 数值来源：铁剑行 = GDD 现值（照抄）；长帧行为新数值（本卡拍板），待 GDD 补录。

```yaml
能力卡: WeaponDefinitionTable（武器定义表 + 扇形命中）
类型: gameplay（武器系统第一步：抽象攻击 → 实体数据表）
状态: 已实现（待人工验收）
挂载: GameSet::Combat
依赖消息: []
接口:
  - WeaponKind 枚举方法族定义表（镜像卡 19 MonsterKind 模式）：
    damage() / full_range() / far_range() / arc_deg() / cooldown()
    IronSword = 34 / 0.9 / 1.5 / 120° / 0.45s（GDD 现值）
    Glaive    = 22 / 1.4 / 1.9 /  60° / 0.60s（新数值）
  - EquippedWeapon(WeaponKind) 组件（玩家生成时挂 IronSword）
  - 命中仍并回 combat.rs player_attack 单系统（避免碎片化）
行为:
  - 命中 = 距离分带（复用线性衰减）∧ 夹角 ≤ arc/2；
    夹角用逻辑侧 Heading.dir（headless 可测），不读表现层 Transform
  - SLASH_* 常量退役：数值全部来自定义表（Balance 改倍率语义，见下）
  - 预声明漂移①：命中从 360° 收窄为铁剑 120° 扇形（几何测试前移 +Z，语义不变）
  - 预声明漂移②：Balance.slash_damage/cooldown 改倍率语义（默认 1.0）——
    F1 调参保留；卡 27 root 窗口、卡 16 冷却条换源「武器冷却 × 倍率」
  - 预声明漂移③（顺手落地 QoL）：挥砍瞬间自动面向最近目标——扇形下站桩
    清场仍可行；derive_heading 移动时才覆写，无稳态双写冲突
验收句:
  1. 假人在 +Z 0.8u（弧内）：一刀 −34
  2. 前方 0.8u + 正后方 0.8u 双假人：前方 −34、后方 −0（自动面向最近）
  3. 换 Glaive：1.3u 前方可命中 22 满伤；铁剑同位置 ≈11.3——表是活的
  4. Glaive 冷却 = 0.60s（× 倍率 1.0）
  5. slash_damage_scale = 2.0 → 伤害翻倍（F1 特性不破功）
回归: weapon_arc_front/back、weapon_table_rows_differ、weapon_cooldown_from_table 等
```

## 卡 30：WeaponVisual（武器形体 + 手骨挂点）

> SOP 试点第二单，第一次真实使用 art-catalog 上架管线。

```yaml
能力卡: WeaponVisual（武器形体 + 手骨挂点跟随）
类型: presentation + asset
状态: 已实现（待人工终审）
接口:
  - WeaponVisual 组件（手骨子实体标记，kind 镜像 EquippedWeapon 行）
  - attach_weapon：按名查找 mixamorig:RightHand 骨骼节点，武器 glb 场景
    挂为手骨子实体——attack clip 动画本身带动剑，无假摆动系统；
    kind 变化时原位换 handle
  - weapon_scale_fixup：一次性抵消手骨继承的 wrapper 缩放（1.353），
    武器保持素材世界全长（0.9/1.4 米）
  - 资产：铁剑/长柄占位件 Blender 程序化自制（CC0-1.0），经 wash 全链
    landed（工单 2026-08-30-iron_sword / -glaive，youxia 图册拍板过）
验收反馈（三条，全部结构性修复）:
  ① 「剑斗到头上」→ 废弃手调 wrapper 偏移常量，改骨骼挂点（Blender 离线
     探针确认骨名/腕点/缩放链）；教训：挂点数据来自骨骼，不来自猜
  ② 「手型不是握武器的手」→ 挂起为素材任务：现有 50+ 段 Mixamo 库全是
     空手系列，握持手型只能来自持械动画集（one-handed sword 系列，
     下载 → mixamo_merge.py → normalize → wash → R8，代码零改动）
  ③ 「run 变成 walk」→ 非本卡回归：根因是卡 25 既有校准债
     RUN_CLIP_AUTHORED_SPEED=4.0 → run 1.25× 慢于 walk 1.56×；
     校准 4.0→2.8（run 1.79×），clip 切换日志补 rate 字段
验收句（数字化）:
  1. 60fps 跑 1 秒，武器实体相对玩家偏移波动 <1cm（headless 钉拓扑 ✓，
     真机跟手性已初验）
  2. 资产页两武器 landed、引用视图非孤儿、R1-R8 无新增（复扫 被引用 7/
     孤儿 0/基线 24 ✓）
回归: hand_bone_lookup_finds_named_node / weapon_scale_fixup_cancels_parent_scale /
  weapon_child_follows_parent_within_1cm（59 回归全绿零警告）
备注: 踩掉两个 Bevy 0.19 查询规则坑（&World 参数与 &mut 查询冲突 B0001；
  同组件读写双查询冲突 B0001），均启动即 panic 暴露、当场修。
```

## 卡 31：Projectile（远程弹道，可选后置）【草案】

```yaml
能力卡: Projectile（投射物）
类型: gameplay（远程武器）
挂载: GameSet::Combat
依赖消息: []
接口: Projectile { vel, damage, life } 实体 + 手动步进命中（不上 rapier）
行为: pos += vel*dt；与怪做 XZ 球 overlap；命中一次即消亡（不穿透）
验收句:
  1. 弹体飞行 5u 命中假人 −20
  2. 越过 8u 寿命自毁
  3. 同一弹体不重复结算同一目标
```

## 卡 32：AnimationGraphMigrate（动画状态机数据化，迁 bevy_animation_graph）【已否决】

> ⏸ **2026-09-02 否决（不实施）**。曾计划迁 bevy_animation_graph，但源码级实证其
> StateMachine 转场只认事件、不支持输入条件比较，三态门控/战斗边沿/播放速率这些核心
> 逻辑无法下沉到图，"扩状态=加节点不改巨型函数"承诺落空，迁移只会增复杂度。
> 由 **卡 33（表驱动）替代**。详见
> [[../../../../docs/decisions/0006-animation-state-machine-refactor|ADR-0006]] +
> [[../../../../docs/decisions/0007-animation-state-machine-table-driven|ADR-0007]]。
> 原完整规格（接口/行为/验收句，56 行）已在 ADR-0006 否决时废弃，此只留一句索引。

## 卡 33：TableDrivenAnimState（动画状态机表驱动——治增长，适配动画 20+）

> 2026-09-02 立案升级（替代卡 32 + 原方案 A）。卡 32 迁 bevy_animation_graph
> 已被源码实证否决（转场只认事件、不支持输入条件比较，逻辑无法下沉到图）。
> 用户确认动画会很多（20+ 状态），故跳过"方案 A 拆纯函数"（治乱不治本），
> 直接上**表驱动状态机**（治理增长）：把"状态机拓扑"从代码搬进表数据，
> 加状态 = 加一条表数据，控制流一次写好、永不改。用 bevy 内置
> `AnimationTransitions`/`AnimationPlayer` 播放，**不自建引擎**。
> 设计详见 `docs/table-driven-anim-state-design.md`。
>
> **实现记录（2026-09-02，commit `212ade4` + `faf8dfd`）**：`anim.rs`（AnimState 表 +
> Transition 枚举 + derive_next_state 纯函数 + hero/monster 表）落地；`drive_anim_states`
> 取代 `sync_walk_playback`；F2 egui 监控面板。13 自测（含 3 个 Death 扩展演示）+
> 59 回归 + 动画播放护栏（`tests/anim_playback.rs`）全绿、零警告。**待真机验收**（hero
> 四态 + 怪物三态 + Death 演示，按 F2 看面板）。

```yaml
能力卡: TableDrivenAnimState（动画状态机表驱动）
类型: presentation（动画架构重组，Bevy 内置 AnimationTransitions 不变）
状态: 🔄 已实现，待人工验收
挂载: PresentationPlugin（表现层系统链）
依赖消息: []
接口（新增符号，均在 presentation.rs 或新 anim.rs）:
  - AnimStateId（枚举）: Idle/Walk/Run/Attack/Hit(+ 怪物 Walk/Attack/Hit)/未来 Death 等
  - AnimState（表项结构）: { name, clip, repeat, rate, transitions, on_finish, blend }
  - Transition（转场条件枚举）:
      SpeedAtLeast{threshold} / SpeedBelow{threshold} / Moving{want} /
      CooldownEdge / FlashEdge / Immediate
  - AnimStateTable（Resource）: Vec<AnimState>，一个表即完整状态机拓扑
  - AnimLink（替换现有）: current_state: AnimStateId + 状态机运行态
  - drive_anim_states（通用系统，取代 sync_walk_playback）:
      读 owner 逻辑输入(moving/speed/cooldown_edge/flash_edge) -> 查当前状态
      的 transitions -> 命中则切 -> 用 bevy AnimationTransitions.play/set_repeat/
      set_speed + 打 hero clip 日志
行为:
  - 现有全部状态/边沿/速率/暂停语义逐字保持（行为等价，见验收）
  - 加状态 = 加一条 AnimState 表数据；drive_anim_states 控制流与状态数量无关
  - derive_next_state(current, inputs) -> AnimStateId 为纯函数，可单测
  - 散落 const 收进表/配置；调参只改表
  - clip 日志通道逐字兼容: `hero clip -> X (speed, rate)`
数字化验收句:
  1. derive_next_state 纯函数单测: feed(静止)->Idle、feed(速度5.0)->Run、
     feed(速度1.6 moving)->Walk、feed(cooldown上跳)->Attack、feed(flash上跳)->Hit
  2. 行为等价: 59 回归全绿 + 零警告 + hero 四态/怪物三态与迁移前肉眼一致（真机确认）
  3. 扩展性演示（本卡核心红利）: 新增 1 个状态（如 Death）——只加 1 条表数据 +
     drive_anim_states 零改动（实测：+Death 变体 + 1 行表数据，3 单测绿）
  4. 调参收敛: 表里一处改 RUN_SPEED_THRESHOLD(3.0)，Walk/Run 边界随之变（单测可证）
  5. 轻量监控面板（阶段1）: F2 开一个 egui 面板，实时显示每 owner 当前状态 +
     状态机拓扑表（从表数据读出）；切换日志为可选项（当前未做，见非目标）
非目标: 不换引擎、不迁 bevy_animation_graph、不动逻辑组件、不改 clip 资产。
  可视化编辑器（Unreal 式节点拖拽图）为阶段 2 后置；面板的"最近切换日志"
  （需跨系统传状态）也后置——阶段 1 只做拓扑 + 当前状态显示。
```

> 阶段划分（用户拍板）：**阶段 1 = 表驱动状态机 + 轻量 egui 监控面板**（本卡，
> 核心价值：动画 20+ 少写代码 + 低成本直观）；**阶段 2 = 可视化编辑器**（后置立项）。
> 前置：用户/团队拍板（ADR-0007 + `docs/topics/engine/table-driven-anim-state-design.md`）。
> 卡 32 保留为被否决的技术记录，不实施。


## 卡 17：ArtAssetPipeline（资产洗白与批量出图流水线）

> 2026-08-27 立卡草案，待 review 拍板。来源 = 3D 美术管线 v2 讨论
> （`docs/topics/game-design/art-pipeline-3d-v2.md`）。
> 关键修正：原点子池「AssetPreview = Bevy 自研 bin」被否——Blender 反正是
> FBX→glTF 转换与洗白的必经工具，一套脚本两个职责，几小时而不是几天；
> 自研原则 = 只在「与 Bevy 运行时耦合」处自研，其余外部工具 + 脚本粘合。

```yaml
能力卡: ArtAssetPipeline（Blender 洗白 + 转台批量出图流水线）
类型: tools + asset（产出 tools/art/ 团队脚本；首个使用方 wave-survival）
状态: 草案（2026-08-27 起草，待 review）
设计来源: 美术管线 v2 工具链决策；风格圣经（docs/style-bible.md）为色板唯一事实来源
接口（命令行，全部无头可批量）:
  - python tools/art/turntable.py --in <glb|目录> --out <图册目录>
      → 标准布光转台渲染 PNG（4 视角）+ 每模型一份 meta.json：
        bbox 尺寸（→碰撞半径候选值）/ 动画 clip 清单 / 面数 / 材质数
  - python tools/art/normalize.py --in <raw.glb> --out <name.glb>
        --height <米> --origin foot --palette <色板 json>
      → 洗白: Y-up 轴向、原点移脚底、身高归一、材质基色量化到色板槽位
行为:
  - Blender 无头模式（blender -b -P）运行；本机 Windows 有 GPU 直接渲，无需 Xvfb
  - 目录约定: 生成原件放 _art/raw/（.gitignore 不入库）；入库的只有
    洗白后 glb（assets/models/）+ 图册 PNG + meta.json（评审证据）
  - 色板 json 从 style-bible.md 第 2 节生成，圣经文档改动 = json 重新导出
验收句:
  1. 对 hero.glb 跑 turntable → 4 张 PNG：模型全身可见、背景纯色、无全黑/全白帧
  2. meta.json 的 bbox 高度与 gltf.report 人工读数一致（±2%）；
     clip 清单与 Babylon Sandbox 显示一致
  3. 对「放大 10 倍 + 原点在质心 + 超色板颜色」的构造测试件跑 normalize →
     输出身高 1.2m（±1%）、脚底 y≈0、所有材质基色 ∈ 色板集合（脚本断言）
  4. 全流程零 GUI 交互（无头 = 可批量、可日后进 CI）
```

## 卡 18A：ArtSourceSpike（AI 生成 vs 套装采购双路对比）

> 2026-08-27 立卡草案，待 review 拍板；依赖卡 17 流水线就绪。
> 目的：角色线的获取路线用数据裁决，不做口味之争；环境/道具维持套装采购不受本 spike 影响。

```yaml
能力卡: ArtSourceSpike（同一「哥布林族」两条获取链路对比 spike）
类型: spike + asset
状态: 草案（2026-08-27 起草，待 review；依赖卡 17）
范围: 哥布林族最小集 = 基础体型 ×（绿 grunt / 红 elite）双色槽 × 1 件武器
路线 A（AI 生成）: Meshy/Tripo 出粗坯（prompt 按圣经模板锁定 hex 与比例约束）
  → normalize 洗白 → Mixamo 自动绑骨 + idle/run/attack 三动作 → 定义表接入
路线 B（套装挑选）: KayKit/Quaternius 怪物件挑选 → normalize 对齐圣经
  → 自带动画优先、缺口 Mixamo 补 → 定义表接入
度量（对比报告六项）: 墙钟时间 / 花费 / 圣经四条符合度（人 1-5 分 + 评语）/
  与 hero 同框剪影区分度 / 进游戏后 run/attack 实拍观感 / 踩坑清单
验收句（spike 产出的定义）:
  1. 两路线各自交付: 入库 glb + 图册 PNG + 第 3 波刷出该怪的实机截图
  2. 一页对比报告: 六项度量并排 + 明确推荐结论；失败项如实记录不粉饰
  3. 报告经人 review 后回写 GDD 点子池「采购策略」条目，成为正式获取策略
```

## 观察通道约定

- **日志仪表**：`RUST_LOG=info cargo run` → 每 2 秒 `[dash] fps≈.. state=.. entities=..`（`src/plugins/debug.rs`）。
- **clip 切换日志**：`hero clip -> Run (speed 5.00, rate 1.79)`——状态机与播放速率直接可读（卡 30 反馈③补 rate）。
- **调试面板**：`F1` 开关 bevy_egui 调参面板（卡 11）。
- **截图**：`F12` 存 `./screenshot.png`（给验收/队友看效果）。
- **回归测试**：验收句转 `tests/behavior.rs`（不带渲染的 App 手动驱动，见示例）。

## 踩坑记录规范（知识库要求）

踩坑必记：现象 / 根因 / 解决 / 反思。落团队知识库 `docs/topics/<领域>/`（type: pitfall），不是只记在自己脑里。
