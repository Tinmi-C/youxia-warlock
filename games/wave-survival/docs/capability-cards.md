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
> 阶段 2（玩法深化）：卡 9–11（2026-08-27 立卡）。

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
| NovaSlash | gameplay-system | 📝 已立卡（2026-08-27，待实现） | Shift 范围斩：半径 1.6 内全体 −60 / 冷却 5s；hanabi 金色冲击波（详见下方卡 9） |
| EnemyVariants | component/gameplay-system | 📝 已立卡（2026-08-27，待实现） | 第 3 波起混入 Runner（快/脆）、第 5 波起 Tank（慢/硬）；组合守恒（详见下方卡 10） |
| EguiTunePanel | architecture/ui-system | 📝 已立卡（2026-08-27，待实现） | F1 开关调参面板，Balance 资源生效于挥砍/Nova/接触数值（详见下方卡 11） |

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
状态: 已立卡 2026-08-27（待实现）
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
状态: 已立卡 2026-08-27（待实现）
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
状态: 已立卡 2026-08-27（待实现）
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

## 观察通道约定

- **日志仪表**：`RUST_LOG=info cargo run` → 每 2 秒 `[dash] fps≈.. state=.. entities=..`（`src/plugins/debug.rs`）。
- **调试面板**：bevy_egui 实时调参面板——**阶段二计划，当前未实现**（依赖已进 `Cargo.toml`，源码未接入）；届时用 `F1` 开关。
- **截图**：`F12` 存 `./screenshot.png`（给验收/队友看效果）。
- **回归测试**：验收句转 `tests/behavior.rs`（不带渲染的 App 手动驱动，见示例）。

## 踩坑记录规范（知识库要求）

踩坑必记：现象 / 根因 / 解决 / 反思。落团队知识库 `docs/topics/<领域>/`（type: pitfall），不是只记在自己脑里。
