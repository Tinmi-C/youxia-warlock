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

> 阶段 1（垂直切片）：卡 1–8，全部完成（2026-08-26）。
> 阶段 2（玩法深化）：卡 9–11，全部完成（2026-08-27，33 个回归测试全绿）。
> 阶段 3（表现层）：卡 12–14 已实现并逐卡提交（2026-08-27，`cargo test` 38 个回归全绿）；
> 模型外观/动画/白闪观感等视觉条目由人跑一次游戏完成验收后本阶段收尾。
> UI 正式化、音效（GDD 后置项）暂未立卡，随实现进度再立。

| 卡 | 类型 | 状态 | 验收句要点 |
|----|------|------|-----------|
| PlayerMove | system | 🔄 模板已有，按手感重写 | WASD 斜向不超速；位移 = speed×Δt（帧率无关，误差<1%） |
| PlayerAttack | gameplay-system | ✅ 已实现（2026-08-26，含回归测试） | Space 挥砍：≤0.9 满 −34 / 0.9~1.5 线性衰减至 0 / 冷却 0.45s（详见下方卡 2） |
| WaveSystem | gameplay-system | ✅ 已实现（2026-08-26，含回归测试） | 三态流转；公式 2+n / 1.1+0.08n / 30×(1+0.4n)；波间 3s（详见下方卡 3） |
| EnemyChase | system | ✅ 已实现（2026-08-26，含回归测试） | 追踪怪朝玩家移动（rapier 驱动），速度 1.1+0.08n（详见下方卡 4） |
| CombatContact | system | ✅ 已实现（2026-08-26，含回归测试） | 距离判定 → 受击扣血 + 白闪；死亡 despawn（详见下方卡 5） |
| PickupDrop | system | ✅ 已实现（2026-08-26，含回归测试） | 击杀掉金色补给，走近自动拾取回血（详见下方卡 6） |
| GameStateUI | ui-system | ✅ 已实现（2026-08-26，视觉卡） | 血条 / 波次文本 / 冷却条；死亡 GameOver / R 重开（详见下方卡 7） |
| GameLoop | system | ✅ 已实现（2026-08-26，含回归测试） | 完整一局闭环：出生→刷怪→击杀→死亡→重开，无崩溃（详见下方卡 8） |
| NovaSlash | gameplay-system | ✅ 已实现（2026-08-27，含回归测试；粒子视觉条目待跑游戏验收） | Shift 范围斩：半径 1.6 内全体 −60 / 冷却 5s；hanabi 金色冲击波（详见下方卡 9） |
| EnemyVariants | component/gameplay-system | ✅ 已实现（2026-08-27，含回归测试） | 第 3 波起混入 Runner（快/脆）、第 5 波起 Tank（慢/硬）；组合守恒（详见下方卡 10） |
| EguiTunePanel | architecture/ui-system | ✅ 已实现（2026-08-27，含回归测试；面板视觉条目待跑游戏验收） | F1 开关调参面板，Balance 资源生效于挥砍/Nova/接触数值（详见下方卡 11） |
| HeroPresentation | asset/architecture | ✅ 已实现（2026-08-27，提交 c07520d；模型/动画为视觉条目待人工验收） | 玩家挂 hero.glb 人形；走路动画「动才播/静止停」跟随位移；回归零改动（详见下方卡 12） |
| MonsterPresentation | asset/component | ✅ 已实现（2026-08-27，提交 0c9119a；外观与辨识度为视觉条目待人工验收） | 怪物挂 monster.glb 人形；分型辨识方案 C = tint+缩放双编码；判定几何不动（详见下方卡 13） |
| HitFlashFeedback | system | ✅ 已实现（2026-08-27，提交 0ef1ca0；含衰减公式回归；观感为视觉条目待人工验收） | flash 首次接入衰减 + 材质发光跟随——白闪真正可见（详见下方卡 14） |

## 卡 2：PlayerAttack（近战挥砍）

```yaml
能力卡: PlayerAttack（近战挥砍）
类型: gameplay-system
状态: 已实现 2026-08-26（实现 + 回归测试全绿）
设计来源: m2 CombatSystem（Space 挥砍 / 半径 0.9 / 一刀 34 / 冷却 0.45s）
设计变更: GDD 数值「0.9 内 −34 / 1.5 外 −0」经团队确认 = 0.9~1.5 之间线性衰减。
          m2 原实现只有 0.9 平砍（一刀 34），衰减是 wave-survival 的新决策。
依赖: 测试桩怪物（带 Hp + Transform 的静态实体，暂不依赖 WaveSystem/EnemyChase）
接口:
  输入:
    - Space 按下（与 m2 键位一致）
    - Player 实体: Transform + Attack 组件（冷却状态）
    - 怪物实体: Hp 组件 + Transform（QueryFilter 排除 Player / 掉落物）
  输出:
    - 命中怪物 Hp.hp 减少（按距离衰减）
    - 命中反馈白闪（Visual.flash = 1.0，延用 m2 约定）
    - Attack.cooldown 重置为 0.45s
行为:
  - 每帧: Attack.cooldown -= dt，clamp 到 ≥ 0
  - Space 按下且 cooldown == 0 时触发一次挥砍:
    以玩家位置为圆心、半径 1.5 内的所有怪物为候选（d = 怪物与玩家的水平距离）:
      d ≤ 0.9        → 伤害 34
      0.9 < d ≤ 1.5  → 伤害 34 × (1.5 − d) / 0.6   （线性衰减，d=1.2 时 ≈17）
      d > 1.5        → 伤害 0
    命中: Hp.hp -= 伤害; Visual.flash = 1.0; Attack.cooldown = 0.45
  - 怪物 hp ≤ 0 的死亡 / despawn 归 CombatContact 卡，本卡只扣血
验收句:
  1. 冷却节流: 固定 60fps 驱动，Space 间隔 0.3s 连按 2 次 → 只触发 1 次;
     间隔 0.5s 连按 2 次 → 触发 2 次
  2. 伤害: 距离 0.5 的桩怪 hp −34; 距离 1.2 的桩怪 hp −17（±1）; 距离 1.6 的桩怪 hp 不变
  3. 衰减边界: 距离 0.9 → −34（±1）; 距离 1.5 → −0（±1）
  4. 多目标: 两个桩怪同时命中，各自按距离扣对应伤害
  5. 以上全部转 tests/behavior.rs（无渲染 App 手动驱动，模式同 PlayerMove 测试）
```

## 卡 3：WaveSystem（波次系统）

```yaml
能力卡: WaveSystem（波次系统）
类型: gameplay-system
状态: 已实现 2026-08-26（实现 + 回归测试全绿）
设计来源: m2 WaveSystem（世界即真相：数敌人 → 波间倒计时 → 刷下一波；出生环表驱动）
设计修正: m2 只在第 1 波前喘息一次（刷怪后 timer 未重置）；GDD 明确「清怪 → 波间喘息 3 秒 → 更强一波」= 每波之间都喘息。本卡按 GDD：每次刷怪后 timer = 3s。
接口:
  输入: Wave 资源（n、timer）; Time; 场上怪物计数（With<Monster>）
  输出: 刷出第 n 波怪物（Monster + Hp + Chasing.speed + Visual + Transform）; Wave.n / Wave.timer 推进
行为:
  - 三态（由「敌人数 + timer」派生，不存枚举）:
    - 敌人 > 0              → 战斗中，不动作
    - 敌人 = 0 且 timer > 0 → 波间喘息，倒计时
    - 敌人 = 0 且 timer ≤ 0 → 刷下一波：n += 1，出生环刷 wave_count(n) 只怪，然后 timer = 3s
  - 混合递增公式: count = 2+n; speed = 1.1+0.08n; hp = 30×(1+0.4n)
  - 初始 n = 0、timer = 3s（开场 3 秒后第 1 波）
验收句:
  1. 公式: wave_count(1)=3 / wave_count(5)=7; wave_speed(1)=1.18 / wave_speed(10)=1.9; wave_hp(1)=42 / wave_hp(5)=90
  2. 首波延迟: 开场前 3s 无怪，随后第 1 波刷 3 只、n=1
  3. 战斗中不刷: 场上有怪时 n 不变、不额外刷怪
  4. 波间喘息: 清空场上怪后 3s 内不刷下一波，3s 后刷第 2 波 4 只、n=2
  5. 怪数据: 第 n 波怪的 Hp = 30×(1+0.4n)、Chasing.speed = 1.1+0.08n
```

## 卡 4：EnemyChase（追踪怪）

```yaml
能力卡: EnemyChase（追踪怪）
类型: system
状态: 已实现 2026-08-26（实现 + 回归测试全绿）
设计来源: m2 ChaseSystem（追玩家，速度 = 波次速度 1.1+0.08n）
引入: 首次引入 bevy_rapier3d（客观物理插件，GDD 技术选型）
接口:
  输入: Player 的 Transform; 怪物的 Transform + Chasing.speed + Velocity
  输出: 怪物的 Velocity.linear = 朝向玩家的 XZ 单位向量 × speed
行为:
  - 每帧对每只追踪怪（With<Chasing>）:
    方向 = (玩家位置 − 怪位置) 在 XZ 平面归一化（Y 恒 0，不飞不坠）
    Velocity.linear = 方向 × Chasing.speed; angular 不动（=0）
  - 物理落地: 怪物 = KinematicVelocityBased 刚体（velocity 驱动，不受重力）
    玩家 = KinematicPositionBased 刚体（move_player 写 Transform，rapier 读位置）
  - 已在玩家位置时方向为 0（length² ≤ 1e-6 → 停），避免零距离抖动
验收句:
  1. 方向: 怪在玩家 +X 侧，velocity.linear.x < 0（朝玩家），y=z=0
  2. 速度: velocity.linear 模长 = Chasing.speed（1.18 时误差 <1e-3）
  3. 接近: 运行 1 秒后怪与玩家距离减小，位移 ≈ speed×1s（±0.5 吸收物理步进边界）
  4. 以上转 tests/behavior.rs（无渲染 App + RapierPhysicsPlugin）
踩坑: RapierPhysicsPlugin 依赖 TransformPlugin（需 GlobalTransform 传播 + StaticTransformOptimizations 资源），而 MinimalPlugins 不含它 → 测试 App 需显式加 TransformPlugin，否则报「StaticTransformOptimizations 资源不存在」。
设计备注（卡 5 前瞻）: kinematic-vs-kinematic 在 rapier 里不产生接触事件；卡 5 CombatContact 的碰撞判定需改用 Dynamic 刚体或距离检测（m2 用 CONTACT_DIST 距离判定）。
```

## 卡 5：CombatContact（贴脸受击 + 死亡）

```yaml
能力卡: CombatContact（贴脸受击 + 死亡）
类型: system
状态: 已实现 2026-08-26（实现 + 回归测试全绿）
设计来源: m2 ContactSystem + DeathSystem（CONTACT_DIST=0.40 / CONTACT_DAMAGE=15 / INVULN_TIME=0.9）
设计修正: 卡 4 备注的 kinematic-vs-kinematic 无接触事件 → 采用 m2 的距离判定（推荐方案 A），不依赖 rapier 碰撞事件。
组件变更: Hp 扩展为 { hp, max, invuln }（照抄 m2 Health），新增 Hp::full() 构造；玩家补 Hp + Visual。
接口:
  输入: 玩家 Transform + Hp + Visual; 怪物 Transform + Hp
  输出: 玩家 Hp.hp 减 15、Hp.invuln 置 0.9、Visual.flash=1.0; 死亡实体 despawn / 玩家死亡 → GameOver
行为:
  - contact_damage（每帧）: 玩家 invuln 递减；invuln ≤ 0 时扫描怪物，水平距离 ≤ 0.40 → hp -= 15、invuln=0.9、flash=1.0、break（一帧最多挨一口）
  - death_despawn（每帧）: 怪物 hp ≤ 0 → despawn；玩家 hp ≤ 0 且 Playing → 切 GameOver
  - 顺序链: move_player → enemy_chase → player_attack → contact_damage → death_despawn → wave_system（同帧内「击杀 → despawn → 数怪」因果链）
验收句:
  1. 受击: 怪在玩家 0.4 内，玩家 hp 100→85、invuln≈0.9、flash=1.0
  2. 无敌帧: 受击后 0.5s 内（<0.9s）即使怪仍贴着也不再次扣血
  3. 一帧一口: 两只怪同时贴脸，一帧只扣 15（85 而非 70）
  4. 死亡: 怪 hp ≤ 0 → despawn（场上怪数减 1）
  5. 玩家死: 玩家 hp ≤ 0 → GameState 切 GameOver
  6. 以上转 tests/behavior.rs
```

## 卡 6：PickupDrop（掉落补给）

```yaml
能力卡: PickupDrop（掉落补给）
类型: system
状态: 已实现 2026-08-26（实现 + 回归测试全绿）
设计来源: m2 spawn_drop + pickup_system（heal 10 / arm 0.6s / PICKUP_DIST 0.45）
接口:
  输入: 怪物死亡位置; 玩家 Transform + Hp; Pickup（heal/arm）
  输出: 怪物死亡掉金色补给; 玩家近补给回血（封顶 max）
行为:
  - 怪物 hp ≤ 0 死亡时（death_despawn）: 在死亡位置 spawn 金色 Pickup（arm = 0.6s）
  - pickup_drop（每帧）: pickup.arm 递减；arm ≤ 0 且玩家水平距离 ≤ 0.45 → 玩家 hp += heal（min(max)）+ despawn pick
验收句:
  1. 掉出: 怪物死亡 → 场上多一个 Pickup
  2. 回血: 玩家 50 血时拾取 → 60（封顶 100）
  3. 拾取消费: 拾取后 Pickup 消失
  4. 以上转 tests/behavior.rs
```

## 卡 7：GameStateUI（HUD 界面）

```yaml
能力卡: GameStateUI（HUD 界面）
类型: ui-system
状态: 已实现 2026-08-26（视觉卡，跑游戏验收）
设计来源: m2 UI 管线（血条/波次/冷却）+ GDD「做」清单
接口: 读取 Player Hp / Attack 冷却 / Wave / GameState
输出: HP 条 + HP 文本、波次文本、挥砍冷却条、GameOver 画面（survived to wave N + R 提示）
行为:
  - spawn_ui（Startup）: 生成血条/波次/冷却/结算画面实体（带标记组件）
  - ui_update（每帧，所有状态）: 按当前数据更新血条宽度%/文本、冷却条%、波次文本；GameOver 时显示结算画面
  - 纯数据更新，不参与玩法
验收句: 视觉卡——跑游戏看：血条随受击变短、波次递增、冷却条回满、死亡显示结算画面 + 按 R 重开
```

## 卡 8：GameLoop（完整一局闭环）

```yaml
能力卡: GameLoop（完整一局闭环）
类型: system
状态: 已实现 2026-08-26（实现 + 回归测试全绿）
设计来源: 汇总卡——验证 出生→刷怪→击杀→死亡→重开 整循环无崩溃
行为:
  - 完整循环: 出生 → 波1刷怪 → 玩家攻击/回血 → 玩家死亡 → GameOver → R 重开 → 重置回波0
  - 「无崩溃」由回归测试连续驱动多帧保证
验收句:
  1. 跑 220 帧 → 波1（3 怪）
  2. 玩家死亡 → GameOver
  3. 按 R → Playing + 波0 + 玩家满血 + 怪清空
  4. 转 tests/behavior.rs game_loop_full_cycle
```

## 卡 9：NovaSlash（范围斩 + 冲击波特效）

```yaml
能力卡: NovaSlash（范围斩 + 冲击波特效）
类型: gameplay-system
状态: 已实现 2026-08-27（逻辑 + 回归测试全绿；验收句第 6 条粒子视觉待跑游戏人工验收）
实现备注:
  - Bevy 0.19 缓冲事件已更名 Message：#[derive(Message)] + MessageWriter + add_message
    （编译期发现，Event/add_event 已不存在；观察通道踩坑候选）
  - bevy::prelude 也导出 Gradient（bevy_ui），与 hanabi 的 Gradient 撞名 → vfx.rs 显式导入
  - hanabi 0.19 ColorOverLifetimeModifier 新增 mask 字段 → 用 ::new(gradient) 构造
  - vfx 触发链：nova_slash 写 NovaFired → VfxPlugin 的 EffectSpawner::reset() 重放一次性爆发
    （照抄官方示例 spawn_on_command 的受控触发模式）
设计来源: GDD 数值表「范围斩 Nova 半径 1.6 / 伤害 60 / CD 5s」+ 核心循环「Shift 范围斩」
接口:
  输入:
    - Shift 按下（键位来自 GDD 核心循环，与近战 Space 并列）
    - Player 实体: Transform + NovaAttack（新组件：{ cooldown }，与 Attack 完全独立）
    - 怪物实体: Hp + Transform
  输出:
    - 半径内所有怪物 Hp.hp −60（无距离衰减——范围斩的定义就是圈内满伤）
    - 命中反馈白闪（Visual.flash = 1.0）
    - NovaAttack.cooldown 重置为 5s
    - NovaFired { at: Vec3 } 事件恰好一次（供 VFX 插件消费；逻辑系统本身不碰渲染类型）
行为:
  - 每帧: NovaAttack.cooldown -= dt，clamp 到 ≥ 0
  - Shift 按下且 cooldown == 0 → 触发一次:
    以玩家为圆心、水平半径 1.6 内所有怪 Hp.hp -= 60、flash = 1.0
    发送 NovaFired（玩家位置）；NovaAttack.cooldown = 5s
  - 系统链位置: player_attack 之后、contact_damage 之前（chain 追加，不改既有顺序语义）
架构决策:
  - 逻辑与特效分离：nova_slash（纯逻辑）进 GamePlugin 可无头回归；
    hanabi 冲击波由独立 vfx 插件监听 NovaFired 实现，只挂 build_app，
    测试 App 不引入 hanabi 渲染依赖（headless 兼容优先）。
设计变更记录（文档化）:
  - spawn_player 增挂 NovaAttack 组件（新增字段，不动既有 Attack/Hp/Visual）
  - game.rs 系统 chain 尾部追加 nova_slash + 注册 NovaFired 事件
验收句:
  1. 冷却节流: Shift 间隔 2s 连按两次 → 只触发 1 次；间隔 5.2s 再按 → 第 2 次触发
  2. 圈内满伤圈外无效: d=0.4 与 d=1.55 的桩怪各 −60；d=1.65 的桩怪 hp 不变
  3. 多目标: 三只桩怪同在圈内 → 同帧各扣 60
  4. 独立节流: 近战 Slash 后立刻按 Shift 仍可触发（两条冷却互不影响）
  5. 事件: 触发帧 NovaFired 恰好 1 条、位置=玩家位置
  6. 视觉卡条目: 跑游戏按 Shift，玩家位置爆出金色环状冲击波粒子（hanabi），消散干净无残留
```

## 卡 10：EnemyVariants（敌人分化）

```yaml
能力卡: EnemyVariants（敌人分化：Runner / Tank）
类型: component + gameplay-system
状态: 已实现 2026-08-27（实现 + 回归测试全绿；分化系数为草案，卡 11 面板就绪后可调参打磨）
设计来源: GDD 验收标准「玩法深化：敌人分化」；基线数值仍照抄 GDD 波次公式，
          分化系数为本卡新草案（主观玩法，egui 面板就绪后可实时调）
接口:
  输入: Wave.n（当前波次）
  输出: 刷出的每只怪多带 MonsterKind 枚举组件（Grunt | Runner | Tank）；
        kind 决定该怪的 Hp / Chasing.speed / 网格尺寸 / 材质颜色
组成公式（草案，确定性可测）:
  - runner_count(n) = n ≥ 3 ? min(3, (n−1)/2 向下取整) : 0
      → w2:0 w3:1 w4:1 w5:2 w6:2 w7+:3（封顶）
  - tank_count(n)   = n ≥ 5 ? min(2, n/5 向下取整) : 0
      → w4:0 w5:1 w9:1 w10:2（封顶）
  - grunt_count(n)  = wave_count(n) − runner − tank（守恒校验必过）
属性分型:
  - Grunt（红 0.6³）: 即现状 —— speed = wave_speed(n), hp = wave_hp(n)
  - Runner（黄 0.45³）: speed × 1.6, hp × 0.5（快而脆）
  - Tank（紫 0.85³）: speed × 0.6, hp × 3.0（慢而硬）
行为:
  - wave_system 刷波时按上述数量从出生环依次分配 kind（分配规则确定性，
    测试只需断言各类计数与各类的属性值，不断言具体方位）
  - 敌人追踪/受击/掉落等老系统零改动（它们只认 Chasing.speed / Hp / Monster，
    分化仅改变 spawn 时的数据）
兼容性承诺: 前 4 波无 Tank、前 2 波纯 Grunt ⇒ 既有 wave/phase1 回归断言逐字不变。
验收句:
  1. 计数公式: runner_count(2)=0 /(3)=1 /(5)=2 /(7)=3 /(9)=3; tank_count(4)=0 /(5)=1 /(10)=2
  2. 组合守恒: 对 n=1..15 恒有 grunt+runner+tank == wave_count(n) 且各 ≥ 0
  3. 分型属性: 强制刷第 3 波 → 恰 1 只 Runner：speed ≈ 1.34×1.6、hp ≈ 66×0.5，
     其余为标准 Grunt；第 5 波 → 恰 1 只 Tank：speed ≈ 1.50×0.6、hp ≈ 90×3
  4. 老 assert 兼容: 既有 tests 全部原样通过（首两波逐字不变）
  5. 以上全部转 tests/behavior.rs
```

## 卡 11：EguiTunePanel（F1 实时调参面板）

```yaml
能力卡: EguiTunePanel（F1 实时调参面板 + Balance 资源）
类型: architecture + ui-system
状态: 已实现 2026-08-27（Balance 热生效路径回归全绿；F1 面板视觉条目待跑游戏人工验收）
实现备注:
  - bevy_egui 0.42 从 `bevy_egui::{egui, EguiContexts}` 取 egui 命名空间；EguiPlugin 有字段 → 用 ::default()
  - Bevy 0.19 的 F 键是 KeyCode::F1（不是 KeyF1）；窗口/滑条用 egui::Window / egui::Slider
  - Balance 默认值 = 原战斗常量（纯等值迁移，既有测试零改动通过即证明）
设计来源: GDD「做」清单「bevy_egui 实时调参面板」+ 观察通道约定 F1 键位
架构决策:
  - 新资源 Balance { slash_damage, slash_cooldown, nova_radius, nova_damage,
    nova_cooldown, contact_damage } 默认值 = 现有常量（等值迁移）
  - 调参生效路径（两类，均为机械替换、零逻辑变化，文档化于此）:
    a) 全局数值项: player_attack / contact_damage / nova_slash 的伤害与冷却
       从 const 改读 Res<Balance>（const 保留为默认值来源）
    b) 组件承载项: Player.speed 由面板直改实体组件（move_player 不动）
  - 插件拆分: Balance 定义/init 在 GamePlugin（headless 可读写默认值）；
    egui 面板本体放独立 tuning 插件只挂 build_app —— 测试 App 无 egui 依赖
  - 硬性约束: 接入 bevy_egui 0.42 前先核对本地 cargo registry 中该版本源码的
    公开 API（EguiPlugin/EguiContexts 等），不凭记忆写
验收句:
  1. 等值回归: 默认 Balance 下既有测试全部原样通过（不接面板也不改行为）
  2. 生效路径: headless 测试直接改 Balance.slash_damage = 100 → 下一刀造成 100 伤害;
              改 contact_damage = 30 → 下次贴脸扣 30
  3. 视觉卡条目: F1 开/关面板；拖动 slash_damage 滑条后一刀伤害立即变化；
              面板关闭后台无残留影响
```

## 卡 12：HeroPresentation（玩家 glTF 模型展示层）

> ✅ 卡 12–14 于 2026-08-27 经团队 review 通过（分型辨识采用方案 C / 走路动画「动才播、静止停」/ 实现顺序 12→13→14），进入实现；各卡视觉验收条目由人跑游戏完成。

```yaml
能力卡: HeroPresentation（玩家 glTF 模型 + 骨骼动画展示层）
类型: asset + architecture
状态: 已实现 2026-08-27（数字化断言回归全绿；视觉观感项待人工跑游戏验收）
设计来源: GDD「做」清单「3D 场景：玩家+怪物+地面（骨骼动画角色，glTF）」+
          技术选型「动画 = Bevy 内置 AnimationGraph」；
          bevy-spike 已验证完整路径：hero.glb Scene → AnimationGraph::from_clip(Animation(0))
          → 子树 AnimationPlayer.play(i).repeat()，WorldAssetRoot + On<WorldInstanceReady>
          观察者开播（games/bevy-spike/src/main.rs）
资产: assets/models/hero.glb（CesiumMan 人形，1 mesh + 1 skin，动画剪辑恰 1 条 / 57 通道——
      本卡立卡前已用脚本核实 glTF 结构）
核心架构决策（本卡主要产出，卡 13 复用同一机制）:
  - 逻辑与皮相分离（先例：nova 纯逻辑 / vfx 只挂 build_app；Balance 在 game / 面板在 tuning）:
    新建 PresentationPlugin **只挂 build_app**，headless 测试 App 不引入 glTF/AssetServer 依赖。
    GamePlugin 及全部逻辑系统零改动；生产链路改动 = lib.rs 加一行插件
  - 挂载方式: 表现插件给玩家根实体挂「场景子实体」（WorldAssetRoot 载 hero.glb），
    并把根实体的占位 Cuboid 组件移除（Mesh3d + MeshMaterial3d）——只动渲染世界里的视觉，
    Player/Transform/Hp/Attack/NovaAttack/Visual/刚体碰撞体等逻辑组件一律不碰
    （不用根实体 Visibility::Hidden 方案：可见性会级联给子模型）
  - 播放策略（2026-08-27 review 拍板，取代原「待机也播走路」的妥协草案）:
    只有走路这一条 clip ≠ 必须常播——静止时不播就是了。走路动画只在玩家移动中播放，
    静止（含 Paused/GameOver）即暂停，模型定住在当前姿势；idle 动作素材到位后再立动画状态机卡。
    数据链: 新组件 WalkCycle { playing } + 新系统 update_walk_cycle（读根实体当帧位移写入，
    注册为**不受状态门控**的普通 Update 系统——Paused 时位置不变自然落到 false，老系统零改动）；
    表现插件只负责把组件映射到 AnimationPlayer 的 play/pause
  - 编码期硬约束（延用卡 11 规矩）: WorldAssetRoot / GltfAssetLabel / AnimationGraphHandle /
    WorldInstanceReady 的 0.19.1 公开 API 与所在模块路径，先核对本地 cargo registry 源码再写，
    不凭记忆（spike 里 WorldInstanceReady 走的是非 prelude 导入）
接口:
  输入: 玩家根实体的 Transform（update_walk_cycle 读位移）; AssetServer（hero.glb）
  输出: 场景子实体挂上玩家树、走路动画播放状态跟随 WalkCycle 组件; 根实体不再渲染方块
行为:
  - 逻辑侧（GamePlugin）: update_walk_cycle 维护 WalkCycle.playing =「根实体本帧位移 > ε」；
    配套组件 PrevTranslation { v } 存上一帧位置（均进 components.rs，纯数据）
  - 表现侧（PresentationPlugin，Startup 后定位玩家根实体）: 插入 RoleModel 标记 +
    WorldAssetRoot(hero.glb Scene(0)) 及其观察者；场景挂在专用子实体上（局部 Transform
    便于调锚点/缩放，move_player 写的根 Transform 不动）；同时移除根实体占位的
    Mesh3d / MeshMaterial3d 组件（不用 Visibility::Hidden——可见性会级联给子模型）
  - On<WorldInstanceReady> 观察者: 遍历子树找到 AnimationPlayer，挂 AnimationGraphHandle 并
    play(0).repeat()，打 info! 日志（观察通道约定）
  - 每帧映射系统: 读根实体 WalkCycle.playing，同步其子树 AnimationPlayer 的 pause/resume
  - 玩家死亡/重开不涉及模型（restart 只重置数据，本就不 despawn 玩家）
设计变更记录（装配级，先例同卡 9/11）:
  - GamePlugin 增注册 update_walk_cycle
  - lib.rs build_app 增挂 PresentationPlugin（一行）
  - 具体父子挂载 API（with_child / ChildOf 关系组件二选一）编码期以本地 cargo registry
    的 0.19.1 源码为准
验收句:
  1. 零改动红线: tests/behavior.rs 既有 33 个回归测试逐字不变、全绿
     （立卡前已 grep 核实：没有任何测试断言玩家/怪物的网格类型或材质，方块只是没人看的替身）
  2. WalkCycle 数据链（headless 可测，转 tests/behavior.rs 回归）:
     桩 App 注入「按住 W」并连驱动 ≥2 帧 → 玩家 WalkCycle.playing == true;
     清空输入再驱动 ≥2 帧 → == false; ε 取值下低速帧率（30fps）移动仍可判真
  3. 视觉（联动真实感）: 按住 WASD 移动中人形在迈步、松开静止后画面定格不再迈步；
     手法同 bevy-spike 的 auto_screenshot（两帧截图有骨骼差 = 在动），F12 截图存证；
     WASD 位移照旧 = speed×Δt
  4. 锚点正确: 模型站在地面上（脚底 ≈ y=0），与占位方块时期的立足位置肉眼连续
  5. 重开无恙: 死亡 → GameOver → R 重开，模型显示正常且按住方向键重新迈步
```

## 卡 13：MonsterPresentation（怪物 glTF 模型 + 分型辨识度）

```yaml
能力卡: MonsterPresentation（怪物 glTF 模型 + 三分型辨识度）
类型: asset + component
状态: 已实现 2026-08-27（数字化断言回归全绿；视觉观感项待人工跑游戏验收）
设计来源: 同卡 12 的机制复用；卡 10 EnemyVariants 现状靠「颜色 + 尺寸」区分分型
          （Grunt 红 0.6³ / Runner 黄 0.45³ / Tank 紫 0.85³）——换人形模型后此差异消失，
          辨识度必须重新落地，这是本卡的真正难点（审美决策，AI 给方案、人拍板）
资产: assets/models/monster.glb（同款 CesiumMan 资产，1 skin + 1 条动画 / 57 通道）
分型辨识方案（默认建议 C，跑游戏看效果后定稿）:
  A. 材质 tint —— 场景子树内的 StandardMaterial 克隆实例化后基色乘分型色
     （红/黄/紫沿用卡 10 的 color() 语义；克隆防共享材质串扰）
  B. 整体缩放 —— Grunt ×1.0 / Runner ×0.85 / Tank ×1.25（体格方向对齐卡 10 数值气质：
     Runner 轻快小巧、Tank 沉重巨大）
  C. A+B 组合 —— 色 + 体双编码，最快速可读
数据落地: MonsterKind 新增 visual_scale() 方法（tint 直接复用既有 color()，语义从
          「方块涂色」扩展为「分型主色」——集中持有分型参数是卡 10 先例）;
          读取方只剩表现插件，老系统继续零感知
接口:
  输入: 怪物根实体的 MonsterKind; monster.glb 场景
  输出: 怪物实体树下人形场景（带 tint / 缩放 / 走路循环常播）; 根实体不再渲染方块
行为:
  - wave_system 刷怪代码**逐字不动**（依旧只造占位方块）; 表现插件每帧对新增的
    Monster 实体补挂场景子实体 + 移除占位 Mesh3d/MeshMaterial3d + 按 kind 上 tint/缩放
  - 怪死亡被 despawn 时场景子实体随父级一起消失（Bevy 父子关系保证），补给照常掉落
兼容性承诺:
  - tests/behavior.rs 全部原样通过（headless 无表现插件，测试世界里的怪物永远是方块，
    这恰好证明逻辑与外观解耦成功）
  - 判定几何完全不动: 贴脸 0.40 / 近战 0.9~1.5 / Nova 1.6 是 GDD 设计常量;
    Collider::ball(cube_size/2) 维持现状——本卡一个数值都不改
验收句:
  1. 零改动红线: 33 个回归测试逐字不变、全绿
  2. 视觉: 第 5 波起（Grunt/Runner/Tank 同场）三分型肉眼可辨，F12 截图存证
  3. 地面真相不变: 缩放后 Tank 观感大于 Runner，但 Chasing.speed/Hp 仍由卡 10 公式决定
     （按 Shift 放一圈可复核伤害/冷却完全一致）
  4. 死亡干净: 击杀任一分型，模型消失、金色补给在尸体位置正常掉出并可拾取
```

## 卡 14：HitFlashFeedback（受击白闪真正可见）

```yaml
能力卡: HitFlashFeedback（flash 衰减 + 材质发光跟随）
类型: system
状态: 已实现 2026-08-27（数字化断言回归全绿；视觉观感项待人工跑游戏验收）
背景（立卡前代码审查发现）: Visual.flash 全工程目前只有三个写入方
  （player_attack / nova_slash / contact_damage 命中时置 1.0），没有任何读取方、
  从不衰减——白闪从未真正看得见。阶段三把它补完，正好骑在卡 12/13 的模型材质之上。
职责切分（逻辑/表现分离铁律）:
  - 衰减（逻辑层，进 GamePlugin 游戏链末尾）: flash = max(flash − FLASH_DECAY_RATE×dt, 0)
    FLASH_DECAY_RATE 默认 4.0（全亮到灭约 0.25s; 主观手感值，先常量后随 Balance 入面板的
    决策实现时定）; 排在 death_despawn 之后，保证命中帧的新写入先进后衰
  - 应用（表现层，PresentationPlugin）: 每帧读 With<Visual> 实体的 flash，
    对其材质克隆实例设 emissive = WHITE × flash——只在挂了模型的渲染世界生效
接口:
  输入: 所有 With<Visual> 实体的 flash 字段; Time
  输出: flash 单调衰减至 0 且不为负; （仅渲染 App）模型材质发光强度跟随 flash
验收句:
  1. 衰减确定式（headless 可测）: 置 flash=1.0 后驱动 dt=1/60 ×15 帧 → flash < 0.05 且 >0;
      继续驱动共 1s → flash == 0.0 且 clamp 生效从未出现负值
  2. 当帧可预言: 命中置 1.0 的同一帧走完衰减后 flash == max(1 − rate×dt, 0)（60fps ≈ 0.93，
      断言按公式写误差带，不写「仍为 1」）
  3. 兼容如实报告: 既有 flash 断言多在触发当帧读取（>0.99），若端到端装配方式引入衰减导致
      任一断言漂移，以 diff 形式报人确认修订，不静默改测试
  4. 视觉: 近战一刀/贴脸挨一口 → 目标全身白光一瞬即暗; Nova 圈内多怪齐闪齐暗; F12 截图存证
```

## 卡 15：MonsterFacing（怪物移动朝向）

> 2026-08-27 立卡，阶段三第二批首卡。来源 = 阶段三真机验收确认的观感欠账
> 「怪物无朝向系统（面朝 +Z 固定），多方向行走露侧脸/背面」。
> AI 按默认方案起草；已按方案实现并真机视觉验收通过（feat `3a15cf2`，2026-08-27，40 个回归全绿）。

```yaml
能力卡: MonsterFacing（怪物人形模型面向追击方向）
类型: component + presentation
状态: 已完成（实现 + 数据链回归 + 真机视觉验收，2026-08-27）
设计来源: 阶段三验收反馈；复用卡 12/13 的 wrapper 架构与「新机制=新组件+新系统」惯例
          （观测系统先例: 卡 12 的 update_walk_cycle 不碰 move_player 而观测其结果）
范围红线:
  - 只做怪物。玩家朝向牵扯攻击方向语义（近战/Nova 判定已是全向），属另一张卡的决策面
  - 物理零交互: 旋转只写场景 wrapper 子实体的 Transform.rotation——怪物刚体已
    LockedAxes::ROTATION_LOCKED（验收 fix `110456a`），根实体 rotation 全程保持单位元，
    卡 4 判定几何与 Velocity 写入完全不受影响
接口:
  输入: With<(Monster, Chasing)> 根实体的 Transform（相邻帧位移推算方向）
  输出: 新组件 Heading { dir: Vec2 }（纯数据，XZ 平面单位向量；进 components.rs）;
        表现层 wrapper yaw 以恒定角速度平滑收敛到 Heading 方向
行为:
  - 数据侧（GamePlugin 注册 derive_heading，逻辑链尾、decay_flash 之后）:
    每帧由位移反推移动方向写 Heading.dir；速度 < ε（同 WalkCycle 的 0.02/s 阈值）时
    保持上一值不抖动——静立的怪维持最后面向
  - 表现侧（PresentationPlugin）: 目标 yaw = atan2(dir.x, dir.z)（glTF 模型面朝 +Z）；
    当前 wrapper yaw 按 MAX_TURN_RATE 常量（默认 540°/s，主观手感值）走最短弧收敛；
    出生第一帧直接对准玩家方向，无初始甩头
  - 为什么不由 enemy_chase 直写朝向: 该老系统只持有 speed 标量不含方向，本卡坚持零改动，
    与卡 12 同款「新增观测系统取证」手法
设计变更记录（装配级）:
  - GamePlugin 增注册 derive_heading
  - components.rs 增 Heading 组件
  - PresentationPlugin 增面朝跟随段（挂现有 Update 链尾）
数字化验收句:
  1. 零改动红线: tests/behavior.rs 现有 38 个回归逐字不变、全绿
  2. Heading 数据链（headless 可测，转回归）: 直线追击驱动 0.3s 后
     angle(Heading.dir, 位移归一) < 2°; 人为挪玩家制造折返 → 折返帧夹角允许 ≈180°
     但 ≤0.6s 内收敛回 <2°; 对静止怪驱动任意帧数 Heading.dir 逐位不变
  3. 编码期硬约束（延用规矩）: atan2 分支与 wrapper 初始 yaw、repeat_mode 等 API 细节
     以本地 cargo registry / 本仓库 presentation.rs 既有实现为准核对后再写
  4. 视觉: 怪群围堵时模型正面冲脸、包抄侧翼转身流畅无瞬跳; F12 截图存证
     （对照: 立卡前版本永远面朝 +Z）
```

## 卡 16：UiFormalization（HUD 正式化）

> 2026-08-27 立卡，阶段三第三批。背景：卡 7 时期已有 bevy_ui 基础 HUD
> （血条/血量文本/波次文字/近战冷却条/GameOver 屏），本卡补齐 GDD 第 30 行清单缺口
> 并做样式统一。**技术路线不再讨论：GDD 已锁 `UI = bevy_ui`**（第 60 行），
> F1 调参面板维持 egui（职责分离：HUD=常显信息，egui=按需调试）。
> AI 按默认方案起草，拍板后进入实现。

```yaml
能力卡: UiFormalization（HUD 正式化——补 GDD 清单缺口 + 样式统一）
类型: system + ui
状态: 待 review
设计来源: GDD「UI：血条 / 波次格子 / 冷却条 / 暂停 / GameOver（bevy_ui）」；
          现状盘点——血条✓ 血量文本✓ 波次文字✓ 近战冷却条✓ GameOver✓；
          缺口 = 波次格子✗ Nova 冷却条✗ 暂停指示✗ debug 提示行占左上角✗ 样式散装✗
范围与默认方案（可整卡通过或逐条改）:
  - 波次格子（默认方案 A）: 顶部中央一排红色小方块 pip = 当前波**存活敌人**数；
    杀一只消一只，清波瞬间归零→下一波重铺。信息价值：剩余压力一目了然
    （备选 B 曾考虑「第 N 波亮 N 格」进度条式，因信息滞后于战况而弃）
  - Nova 冷却条: 近战冷却条正下方同宽同高，紫色（Nova 主色对齐卡 9 金色系差异化:
    条底用紫罗兰 #7a5cff 系）；卡 9 审美拍板沿用「可辨识即可，美术资产后置」
  - 暂停遮罩: P 暂停时全屏半透明黑 40% + 居中 "PAUSED — P to resume"；
    复用 GameOver 屏的布局手法
  - debug 提示行（默认保留）: 从左上角迁到屏幕底部居中、12px、60% 透明度——
    操作提示对新手有价值但不再抢 HUD 位置（备选删除，因新手期未过而弃）
  - 样式统一: 边距/尺寸/色板收敛为 ui.rs 顶部常量组（单处可调）；
    GameOver 屏加 60% 黑遮罩底提升可读性
  - 字体: 沿用 bevy 内置默认字体（assets/fonts 为空，占位资产=进度解耦；
    正式字体到位后只换 TextFont 一处）
数据来源（零新逻辑数据，全部只读既有件）: Hp / Attack / NovaAttack / Wave /
          Monster 存活计数（Query len）/ GameState；新组件仅 UI 标记件
          （UiNovaFill / UiWavePips / UiPauseOverlay，进 components.rs）
职责边界:
  - ui.rs 一个文件改完（spawn_ui + ui_update + 常量组）；game.rs 的 spawn_hint_ui
    并入 ui.rs 便于统一布局；GamePlugin 注册面不动（spawn_ui/ui_update 已在）
  - 卡 11 F1 egui 面板零接触；两套 UI 管线并存互不感知
数字化验收句:
  1. 零改动红线: tests/behavior.rs 现有 40 个回归逐字不变、全绿
     （spawn_ui/ui_update 本就在 headless 链上，是既有事实的延续）
  2. 波次格子（headless 可测，转回归）: 强制刷 w5（7 怪）→ pip 实体数 == 7；
     despawn 2 只再驱动 → == 5；波清空后 pip 归零、下一波重铺为新数量
  3. Nova 冷却条对称性（headless 可测）: 触发 Nova 当帧后 fill 宽度百分比 ==
     (1 − nova.cooldown / NOVA_COOLDOWN)×100，误差 <1 个百分点；
     近战条同式断言防复制粘贴错引用
  4. 暂停遮罩（headless 可测）: 切 Paused → UiPauseOverlay Visibility==Visible；
     切回 Playing → Hidden
  5. 视觉: 左上角只有血条+文字+两条冷却条、顶部中央波次格子、底部半透明提示行；
     P 暂停遮罩正常；F12 截图存证
```

## 观察通道约定

- **日志仪表**：`RUST_LOG=info cargo run` → 每 2 秒 `[dash] fps≈.. state=.. entities=..`（`src/plugins/debug.rs`）。
- **调试面板**：`F1` 开关 bevy_egui 调参面板（卡 11 已实现：实时改 Balance 六项 + Player.speed）。
- **截图**：`F12` 存 `./screenshot.png`（给验收/队友看效果）。
- **回归测试**：验收句转 `tests/behavior.rs`（不带渲染的 App 手动驱动，见示例）。

## 踩坑记录规范（知识库要求）

踩坑必记：现象 / 根因 / 解决 / 反思。落团队知识库 `docs/topics/<领域>/`（type: pitfall），不是只记在自己脑里。
