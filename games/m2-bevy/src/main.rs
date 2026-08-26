//! M2-bevy：ADR-0003 落地——把 m2-demo 的手写玩具 ECS 迁移到 bevy_ecs 0.19。
//! M3 Step 1（2026-08-22）：渲染深化开始——地板贴棋盘格纹理（UV / 采样 / repeat 平铺）。
//!
//! 迁移原则（对照 ADR-0003）：
//!   - 组件定义原样保留（只加 #[derive(Component)]——逻辑零改动）
//!   - 系统逻辑原样保留（手动翻池子循环 → Query 声明式筛选，框架替你筛）
//!   - GameState / 相机 / wgpu 渲染层原样保留（这层本来就不归 ECS 管）
//!
//! 三处关键映射（搜 [REVIEW M- 看讲解，这是本项目的 review 点）：
//!   [REVIEW M-1] 手动循环筛标签      → Query + With/Without
//!   [REVIEW M-2] 直接改池子          → Commands（先记账后结算）
//!   [REVIEW M-3] update() 手动排系统 → Schedule + .chain()（顺序 = 数据因果链）
//!
//! M3 Step 1 纹理改动一览（渲染层，ECS 零改动）：
//!   - 顶点加 uv 属性（VertexInput location 1）
//!   - uniform 加 uv_scale（0 = 纯色实体 / >0 = 贴图平铺次数——「要不要贴图」是数据不是代码）
//!   - bind group 加共享纹理 + 采样器（binding 1/2）
//!   - fs: select(u.color, textureSample(...), u.uv_scale > 0)
//!
//! M3 Step 2（2026-08-22）光照改动一览（仍是渲染层，ECS 零改动）：
//!   - 实体从「躺平纸片」升级为真立方体：24 顶点 + 36 索引，每面一根法线
//!     （纸片全部朝天，光照没有教学效果；立方体六面朝向不同 → 明暗立现）
//!   - 顶点加 normal 属性（location 2）
//!   - uniform 加 light_dir + ambient（vec3 对齐坑：拆 3 个标量传）
//!   - fs 漫反射：light = ambient + (1-ambient) * max(dot(法线, 光方向), 0)
//!     ——亮度 = 面朝向光的程度（Lambert 余弦定律）
//!   - 模型矩阵去掉躺平旋转；height 语义升级为「立方体底面离地高度」
//!
//! M3 Step 3（2026-08-24）模型加载改动一览（渲染层 + Visual 加一个数据字段）：
//!   - gltf crate 解析 assets/Duck.glb → 顶点(position/uv/normal) + 索引
//!     ——文件里的四样数据正好就是顶点格式已有的四样，管线零改动
//!   - Mesh（网格=共享的形状数据，印章）vs 实体（章印）分离：
//!     meshes: HashMap<MeshId, GpuMesh>，多种几何共存，draw 时按实体取
//!   - Visual 加 mesh: MeshId（用哪枚印章——又是「数据不是代码」）
//!   - 玩家升级成鸭子模型（演示 glTF 加载），怪/金币保持立方体（两种几何混存）
//!
//! 操作：WASD 移动 | Space 挥砍 | 方向键 相机 | +/- 缩放 | P 暂停 | O 透视/正交 | R 重开 | Esc 退出

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use bevy_ecs::prelude::*;
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

// ---------------------------------------------------------------------------
// 游戏状态机（迁移不变）：状态的切换发生在 winit 事件层，不在 ECS 里。
// 它是 update() 里的「开关面板」：只在 Playing 时跑 schedule.run()。
// ---------------------------------------------------------------------------
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum GameState {
    Playing,
    Paused,
    GameOver,
}

// ---------------------------------------------------------------------------
// 组件层：和玩具版逐字相同，唯一的区别是多了一个 derive。
//
// 玩具版：struct Position([f32; 2]);            ← 自己管池子（Vec<Option<T>>）
// bevy 版：#[derive(Component)] struct Position([f32; 2]);  ← 框架自动开池子
//
// 「实体 = 编号 + 标签组合」的本质没变：bevy 的 Entity 同样只是个编号
// （带世代号——见 despawn 的注释），组件同样是「贴在实体上的纯数据」。
// ---------------------------------------------------------------------------

/// 组件：游戏平面坐标 (x, z)。
#[derive(Clone, Copy, Debug, Component)]
struct Position([f32; 2]);

/// 组件：速度向量（单位/秒）。
#[derive(Clone, Copy, Debug, Component)]
struct Velocity([f32; 2]);

/// 组件（标签）：接受玩家输入。谁贴了它，InputSystem 就替谁写 Velocity。
#[derive(Clone, Copy, Debug, Component)]
struct Controlled;

/// 组件：追踪玩家的参数。谁贴了它，ChaseSystem 就每帧把它的 Velocity 指向玩家。
#[derive(Clone, Copy, Debug, Component)]
struct Chasing {
    speed: f32,
}

/// 网格编号（M3 Step 3）：立方体 = 0，鸭子 = 1……渲染层按编号查 GpuMesh。
/// 「用哪枚印章」是数据不是代码——新模型 = 新编号 + 新文件，系统零改动。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum MeshId {
    Cube,
    Duck,
    Humanoid,
    Monster,
}

/// 组件：外观。渲染用（颜色 / 大小 / 悬浮高度 / 受击白闪强度）。
/// uv_scale：纹理平铺密度（M3 Step 1）。0 = 不贴图，用纯色；>0 = 贴图且整块铺这么多次。
/// 「要不要贴图」是每个实体自己的数据——和颜色一样放进 Visual，不改任何系统。
/// mesh：用哪个网格（M3 Step 3）——形状共享的「印章编号」。
#[derive(Clone, Copy, Debug, Component)]
struct Visual {
    color: [f32; 4],
    size: f32,
    height: f32,
    flash: f32,
    uv_scale: f32,
    mesh: MeshId,
}

/// 组件：生命值。hp 归零 = 死（death_system 收尸）。invuln = 无敌帧倒计时（秒）。
#[derive(Clone, Copy, Debug, Component)]
struct Health {
    hp: f32,
    max: f32,
    invuln: f32,
}

/// 组件：攻击参数（冷却/半径/伤害都是数据——换把武器就是换一张标签）。
#[derive(Clone, Copy, Debug, Component)]
struct Attack {
    cooldown: f32,
    radius: f32,
    damage: f32,
}

/// 组件：范围斩（玩法扩展 Step 2）。又一张纯数据标签——combat_system 筛
/// Attack（Space 挥砍），nova_system 筛 Nova（Shift 范围斩），两个系统
/// 各查各的互不认识：「新机制 = 新系统」，老系统零改动。
#[derive(Clone, Copy, Debug, Component)]
struct Nova {
    cooldown: f32,
    radius: f32,
    damage: f32,
}

/// 组件：短命特效的年龄（玩法扩展 Step 2）。怪死于 Health 归零（death_system
/// 收尸），特效没有 Health、死于时间——Age 到头自己 despawn。
/// 特效不是特殊东西：就是一个普通实体，生出来、表演、死掉，和怪走同一条管线。
/// squash（玩法扩展 Step 3）：true = 死亡尸体——渲染层做「压扁弹起」非均匀
/// 缩放（y 压扁、xz 撑大）；false = 碎片——age_system 里均匀缩小。
#[derive(Clone, Copy, Debug, Component)]
struct Age {
    t: f32,
    life: f32,
    squash: bool,
}

/// 组件：程序动画状态（玩法扩展 Step 3）。动画 = 用时间摆姿势，全部体现在
/// 模型矩阵 M 上（平移抖 = 走路颠簸 / 旋转 = 转身朝向 / 缩放 = 压扁），
/// 渲染层只消费最终矩阵——逻辑渲染解耦的第一笔回报。
/// t 是「出生至今秒数」累加器——Age 思想的推广（Age 管生死，Anim 管表演）。
#[derive(Clone, Copy, Debug, Component)]
struct Anim {
    t: f32,
    /// height 基准：bob 只是在基准上加偏移，不覆盖别人的语义。
    base_h: f32,
    /// 当前朝向（弧度，绕 Y 轴）。front = +Z。
    yaw: f32,
    /// 本帧高度偏移（anim_system 写，渲染层读——单写手无冲突）。
    bob: f32,
}

/// 组件：掉落物。heal = 拾取回血量，arm = 落地保护倒计时（防击杀瞬间秒拾）。
#[derive(Clone, Copy, Debug, Component)]
struct Pickup {
    heal: f32,
    arm: f32,
}

// ---------------------------------------------------------------------------
// 资源（Resource）：全局单份数据——不属于任何实体，所有系统都能读写。
// 玩具版里它们是 App struct 的字段 / 函数入参；bevy 版塞进 World，由框架按类型取用。
// ---------------------------------------------------------------------------

/// 本帧帧间隔（秒）。每帧 update() 开头写入，所有系统通过 Res<Delta> 读取。
#[derive(Resource)]
struct Delta(f32);

/// 当前按下的键（winit 世界 → ECS 世界的桥）。
#[derive(Resource, Default)]
struct Keys(pub HashSet<KeyCode>);

/// 战绩（验收仪表用）。玩具版是 App 的两个字段，现在归 ECS 管。
#[derive(Resource, Default)]
struct Stats {
    kills: u32,
    pickups: u32,
}

/// 波次状态（玩法扩展 Step 1）：n = 当前第几波；spawn_timer = 波间倒计时
/// （上一波清空后给玩家几秒喘息/捡金币，归零才刷下一波）。
/// 纯数据——波次的「逻辑」全在 wave_system 里，这里只记账。
#[derive(Resource)]
struct Wave {
    n: u32,
    spawn_timer: f32,
}

/// 波间喘息时长（秒）。
const WAVE_BREAK: f32 = 3.0;

/// 混合递增公式（玩法扩展 Step 1）：数量、速度、血量三条曲线一起涨。
/// 数量 2+n：第 1 波 3 只、第 5 波 7 只……
/// 速度 1.1+0.08n：第 1 波 1.18（遛得动）……第 10 波 1.9（贴身甩不掉）
/// 血量 30*(1+0.4n)：第 1 波 42（两刀）……第 5 波 90（三刀）
fn wave_count(n: u32) -> u32 {
    2 + n
}
fn wave_speed(n: u32) -> f32 {
    1.1 + 0.08 * n as f32
}
fn wave_hp(n: u32) -> f32 {
    30.0 * (1.0 + 0.4 * n as f32)
}

/// 场地边界（半边长）。
const BOUND: f32 = 3.3;
/// 玩家速度（单位/秒）。
const PLAYER_SPEED: f32 = 2.2;
/// 挥砍冷却（秒）。
const SLASH_CD: f32 = 0.45;
/// 范围斩参数（玩法扩展 Step 2）：冷却 / 半径 / 伤害。
const NOVA_CD: f32 = 5.0;
const NOVA_RADIUS: f32 = 1.6;
const NOVA_DAMAGE: f32 = 60.0;
/// 冲击波碎片数量 / 存活时长（秒）。
const NOVA_SHARDS: u32 = 10;
const SHARD_LIFE: f32 = 0.5;
/// 敌人贴脸判定距离 / 单次伤害 / 玩家无敌帧时长（秒）。
const CONTACT_DIST: f32 = 0.40;
const CONTACT_DAMAGE: f32 = 15.0;
const INVULN_TIME: f32 = 0.9;
/// 拾取判定距离。
const PICKUP_DIST: f32 = 0.45;

// ---------------------------------------------------------------------------
// 系统层：函数体和玩具版几乎逐行对应，变的只有「怎么拿到实体」。
//
// [REVIEW M-1] Query：从「翻池子」到「声明我要什么」
//
//   玩具版（你手写的那个循环）：
//     for i in 0..world.next {
//         if let (Some(p), Some(v)) = (world.pos[i], world.vel[i]) { ... }
//     }
//
//   bevy 版（框架替你筛）：
//     fn move_system(mut q: Query<(&mut Position, &Velocity)>) {
//         for (mut p, v) in &mut q { ... }
//     }
//
//   Query<(&mut Position, &Velocity)> 读作：
//     「给我所有【同时贴着 Position 和 Velocity】的实体，
//       Position 我要可写（&mut），Velocity 只读（&）」。
//   With<T> / Without<T> = 玩具版里 ctl[i].is_some() / pickup[i].is_some() 那些排除判断。
//
//   筛选（filter，这条规则处理谁）和查询（query，干活时参考谁的数据）
//   两个概念在这里终于有了语法上的区分——你 Step 1 概念纠偏时分的正是这两个。
// ---------------------------------------------------------------------------

/// InputSystem：Controlled 实体按键盘写 Velocity。
/// （M1 教训继承：斜向要归一化，否则 W+D 会快 41%）
fn input_system(keys: Res<Keys>, mut q: Query<&mut Velocity, (With<Controlled>, With<Position>)>) {
    let mut dir = [0.0f32; 2];
    if keys.0.contains(&KeyCode::KeyW) {
        dir[1] -= 1.0; // W = 屏幕上方 = 世界 -z
    }
    if keys.0.contains(&KeyCode::KeyS) {
        dir[1] += 1.0;
    }
    if keys.0.contains(&KeyCode::KeyA) {
        dir[0] -= 1.0;
    }
    if keys.0.contains(&KeyCode::KeyD) {
        dir[0] += 1.0;
    }
    let len = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
    let v = if len > 0.0 {
        [dir[0] / len * PLAYER_SPEED, dir[1] / len * PLAYER_SPEED]
    } else {
        [0.0, 0.0]
    };
    for mut vel in &mut q {
        vel.0 = v;
    }
}

/// ChaseSystem：Chasing 实体的 Velocity 指向玩家。
///
/// 生产者/消费者分工原样保留：ChaseSystem 生产 Velocity → MoveSystem 消费。
/// 注意两个 Query 的分工：q_player 是「查询」（参考谁），q 是「筛选」（处理谁）。
fn chase_system(
    q_player: Query<&Position, With<Controlled>>,
    mut q: Query<(&Position, &Chasing, &mut Velocity)>,
) {
    // 玩家不存在（已死）就没人可追——和玩具版的 find 失败提前返回同构。
    let Some(pp) = q_player.iter().next() else { return };
    let player = pp.0;

    for (p, ch, mut vel) in &mut q {
        let dx = player[0] - p.0[0];
        let dz = player[1] - p.0[1];
        let len = (dx * dx + dz * dz).sqrt();
        // 守卫：len == 0 时除法会产生 NaN（玩具版注释原样保留）。
        vel.0 = if len > 1e-4 {
            [dx / len * ch.speed, dz / len * ch.speed]
        } else {
            [0.0, 0.0]
        };
    }
}

/// MoveSystem：pos += vel × delta。公式一字未改。
fn move_system(delta: Res<Delta>, mut q: Query<(&mut Position, &Velocity)>) {
    let d = delta.0;
    for (mut p, v) in &mut q {
        p.0 = [p.0[0] + v.0[0] * d, p.0[1] + v.0[1] * d];
    }
}

/// BounceSystem：撞墙反弹 + 夹回场地。
fn bounce_system(mut q: Query<(&mut Position, &mut Velocity)>) {
    for (mut p, mut v) in &mut q {
        let (mut x, mut z, mut vx, mut vz) = (p.0[0], p.0[1], v.0[0], v.0[1]);
        if x < -BOUND && vx < 0.0 {
            vx = -vx;
        }
        if x > BOUND && vx > 0.0 {
            vx = -vx;
        }
        if z < -BOUND && vz < 0.0 {
            vz = -vz;
        }
        if z > BOUND && vz > 0.0 {
            vz = -vz;
        }
        p.0 = [x.clamp(-BOUND, BOUND), z.clamp(-BOUND, BOUND)];
        v.0 = [vx, vz];
    }
}

/// CombatSystem：挥砍——Attack 实体按 Space 出刀，半径内敌人（排除自己/掉落物）扣血 + 白闪。
///
/// 多写手判据原样保留：Health 的写法全是加减法（可交换），所以多个系统写它不踩覆盖雷。
/// victims 里 Position 只读、Health 和 Visual 可写；Option<&mut Visual> = 玩具版
/// 「有 vis 就白闪，没有就算了」——掉落物没有 Visual 也不会在这里炸。
fn combat_system(
    keys: Res<Keys>,
    delta: Res<Delta>,
    mut attackers: Query<(&mut Attack, &Position)>,
    mut victims: Query<
        (&mut Health, &Position, Option<&mut Visual>),
        (Without<Controlled>, Without<Pickup>),
    >,
) {
    let want = keys.0.contains(&KeyCode::Space);

    // 第一遍：冷却倒计时 + 收集本帧出刀的攻击者。
    let mut slashes: Vec<([f32; 2], f32, f32)> = Vec::new(); // (位置, 半径, 伤害)
    for (mut a, p) in &mut attackers {
        a.cooldown = (a.cooldown - delta.0).max(0.0);
        if want && a.cooldown <= 0.0 {
            slashes.push((p.0, a.radius, a.damage));
            a.cooldown = SLASH_CD;
        }
    }
    if slashes.is_empty() {
        return;
    }

    // 第二遍：结算伤害。
    for (mut h, p, vis) in &mut victims {
        let mut hp = h.hp;
        let mut hit = false;
        for &(ap, radius, damage) in &slashes {
            let d = ((p.0[0] - ap[0]).powi(2) + (p.0[1] - ap[1]).powi(2)).sqrt();
            if d <= radius {
                hp -= damage;
                hit = true;
            }
        }
        if hit {
            h.hp = hp;
            if let Some(mut v) = vis {
                v.flash = 1.0; // 白闪反馈
            }
        }
    }
}

// ---------------------------------------------------------------------------
// NovaSystem（玩法扩展 Step 2）：Shift 范围斩。
//
// 和 combat_system 的关系 = 「新机制 = 新系统」的活教材：
//   combat_system 筛 Attack（Space 挥砍，小半径高频率）
//   nova_system   筛 Nova  （Shift 爆发，大半径 5 秒一发）
// 两系统互不知道对方存在，玩家同时挂两张标签 = 同时拥有两种能力——
// ECS 的加法扩展：加能力 = 加标签，不改老代码。
//
// 视觉反馈（game-feel 铁律：状态变化必须有画面反馈）：
//   spawn 一圈金色碎片实体（Position+Velocity+Visual+Age），飞散 + 缩小 + 到期消失。
//   特效实体没有 Health → death_system / wave_system 都自动无视它（标签筛选的天赋）。
// ---------------------------------------------------------------------------

fn nova_system(
    keys: Res<Keys>,
    delta: Res<Delta>,
    mut cmds: Commands,
    mut q_player: Query<(&mut Nova, &Position), With<Controlled>>,
    mut victims: Query<
        (&mut Health, &Position, Option<&mut Visual>),
        (Without<Controlled>, Without<Pickup>),
    >,
) {
    let want = keys.0.contains(&KeyCode::ShiftLeft) || keys.0.contains(&KeyCode::ShiftRight);
    let Some((mut nova, ppos)) = q_player.iter_mut().next() else {
        return; // 玩家死了就放不出
    };

    nova.cooldown = (nova.cooldown - delta.0).max(0.0);
    if !want || nova.cooldown > 0.0 {
        return;
    }
    nova.cooldown = NOVA_CD;

    // 伤害结算：半径内所有敌人扣血 + 白闪（Health 全是加减法，多写手安全）。
    for (mut h, p, vis) in &mut victims {
        let d = ((p.0[0] - ppos.0[0]).powi(2) + (p.0[1] - ppos.0[1]).powi(2)).sqrt();
        if d <= nova.radius {
            h.hp -= nova.damage;
            if let Some(mut v) = vis {
                v.flash = 1.0;
            }
        }
    }

    // 冲击波视觉：一圈碎片从玩家身边向外飞。
    for i in 0..NOVA_SHARDS {
        let ang = i as f32 / NOVA_SHARDS as f32 * std::f32::consts::TAU;
        let (dx, dz) = (ang.cos(), ang.sin());
        cmds.spawn((
            Position([ppos.0[0] + dx * 0.3, ppos.0[1] + dz * 0.3]),
            Velocity([dx * 3.5, dz * 3.5]),
            Visual { color: [0.98, 0.75, 0.18, 1.0], size: 0.14, height: 0.05, flash: 0.0, uv_scale: 0.0, mesh: MeshId::Cube },
            Age { t: 0.0, life: SHARD_LIFE, squash: false },
        ));
    }
    println!("[nova] released: radius {:.1}, damage {:.0}", nova.radius, nova.damage);
}

/// AgeSystem：短命特效的生死簿。age 涨 → 碎片缩小（淡出的替身——主管线
/// 不透明，没有 alpha，用缩小代替淡出）；到头 despawn。
/// move_system 自动搬运它们（有 Position+Velocity），bounce_system 也照弹——
/// 特效没有特权，走和其他实体完全相同的路。
fn age_system(
    delta: Res<Delta>,
    mut cmds: Commands,
    mut q: Query<(Entity, &mut Age, &mut Visual)>,
) {
    for (e, mut age, mut vis) in &mut q {
        age.t += delta.0;
        if !age.squash {
            // 碎片：均匀缩小 = 视觉上的「淡出」。尸体的压扁在渲染层做（非均匀）。
            let k = (1.0 - age.t / age.life).max(0.0); // 1 → 0
            vis.size = 0.14 * k;
        }
        if age.t >= age.life {
            let _ = cmds.entity(e).despawn();
        }
    }
}

/// AnimSystem（玩法扩展 Step 3）：程序动画——代码直接摆姿势。
/// 走路 = height 上下 sin 颠簸（速度驱动频率感）；静止 = 待机呼吸（轻微起伏）；
/// 移动中朝向 = 速度方向（yaw 转身）。全部只写 Anim 自己的字段 + Visual.height，
/// 不碰 Position/Velocity——动画是「表演层」，叠在移动结果之上。
fn anim_system(delta: Res<Delta>, mut q: Query<(&Velocity, &mut Anim, &mut Visual)>) {
    for (v, mut a, mut vis) in &mut q {
        a.t += delta.0;
        let speed = (v.0[0] * v.0[0] + v.0[1] * v.0[1]).sqrt();
        if speed > 0.01 {
            // front = +Z：yaw 把 +Z 转到速度方向。
            a.yaw = v.0[0].atan2(v.0[1]);
            // |sin| 颠簸：每步弹一下，落地感；频率 ≈ 11 rad/s（跑步节奏）。
            a.bob = (a.t * 11.0).sin().abs() * 0.06 * vis.size;
        } else {
            // 待机呼吸：慢频率、小幅度的起伏——「活着」的最低成本证明。
            a.bob = (a.t * 2.2).sin() * 0.012 * vis.size;
        }
        vis.height = a.base_h + a.bob;
    }
}

/// ContactSystem：敌人贴到玩家身上 → 扣血 + 无敌帧（一帧最多挨一口）。
/// q_player / q_enemies 两个查询 = 玩具版「先 find 玩家再循环敌人」。
fn contact_system(
    delta: Res<Delta>,
    mut q_player: Query<(&Position, &mut Health, Option<&mut Visual>), With<Controlled>>,
    q_enemies: Query<&Position, (With<Health>, Without<Controlled>, Without<Pickup>)>,
) {
    let Some((pp, mut h, mut pvis)) = q_player.iter_mut().next() else {
        return; // 玩家死了（被 despawn）就没人可咬
    };
    let pp = pp.0;

    h.invuln = (h.invuln - delta.0).max(0.0);

    if h.invuln <= 0.0 {
        for ep in &q_enemies {
            let d = ((ep.0[0] - pp[0]).powi(2) + (ep.0[1] - pp[1]).powi(2)).sqrt();
            if d <= CONTACT_DIST {
                h.hp -= CONTACT_DAMAGE;
                h.invuln = INVULN_TIME;
                if let Some(v) = pvis.as_deref_mut() {
                    v.flash = 1.0;
                }
                break; // 一帧最多挨一口
            }
        }
    }
}

// ---------------------------------------------------------------------------
// [REVIEW M-2] Commands：从「当场改」到「先记账后结算」
//
//   玩具版 death_system：先收集 dead 列表，再逐个 world.despawn(i) + spawn_drop ——
//     因为「边迭代边改池子会出乱子」，你被迫手写两段式。
//
//   bevy 版：循环里只往 Commands 记账（spawn/despawn 都是「记一笔」），
//     记完就返回。真正的增删发生在 ApplyDeferred 这个结算点——
//     这和你 M1 学的 write_buffer「排进队列、submit 时才生效」是同一个思想：
//     命令缓冲 = 渲染队列 的 ECS 版本。
//
//   顺带回收 Step 2 埋的伏笔：玩具版「编号不回收」防的是悬空引用；
//   bevy 的 Entity 带世代号（同编号复用会 +1 代），旧引用对比世代就知道自己过时了。
// ---------------------------------------------------------------------------

/// DeathSystem：hp ≤ 0 → despawn + 掉落。
fn death_system(
    mut cmds: Commands,
    q: Query<(Entity, &Health, &Position, Option<&Controlled>, Option<&Visual>)>,
    mut stats: ResMut<Stats>,
) {
    for (e, h, p, ctl, vis) in &q {
        if h.hp <= 0.0 {
            if ctl.is_some() {
                println!("GAME OVER: player down (kills={}) — press R to restart", stats.kills);
            } else {
                stats.kills += 1;
                spawn_drop(&mut cmds, p.0); // 敌人死 → 原地掉一个金色补给
                // 死亡动画（玩法扩展 Step 3）：留一具「尸体」——颜色压暗、0.35 秒
                // 内压扁消失（squash 由渲染层按 Age 进度做）。尸体无 Health →
                // wave_system 数敌人时不阻塞下一波；无 Velocity → 不挪窝。
                if let Some(v) = vis {
                    let mut corpse = *v;
                    corpse.color = [v.color[0] * 0.5, v.color[1] * 0.5, v.color[2] * 0.5, 1.0];
                    corpse.flash = 0.0;
                    cmds.spawn((Position(p.0), corpse, Age { t: 0.0, life: 0.35, squash: true }));
                }
            }
            let _ = cmds.entity(e).despawn();
        }
    }
}

/// 掉落物：Position + Pickup + Visual（金色小方块），无 Velocity → 永远不动。
fn spawn_drop(cmds: &mut Commands, at: [f32; 2]) {
    cmds.spawn((
        Position(at),
        Pickup { heal: 10.0, arm: 0.6 },
        Visual { color: [0.95, 0.80, 0.25, 1.0], size: 0.22, height: 0.10, flash: 0.0, uv_scale: 0.0, mesh: MeshId::Cube },
    ));
}

/// PickupSystem：掉落物落地（arm 归零）后，玩家碰到 → 回血（不超上限）+ despawn。
fn pickup_system(
    delta: Res<Delta>,
    mut cmds: Commands,
    mut q_pickups: Query<(Entity, &mut Pickup, &Position)>,
    mut q_player: Query<(&Position, &mut Health), With<Controlled>>,
    mut stats: ResMut<Stats>,
) {
    let Some((pp, mut ph)) = q_player.iter_mut().next() else {
        return;
    };
    let pp = pp.0;

    for (e, mut pk, p) in &mut q_pickups {
        pk.arm = (pk.arm - delta.0).max(0.0);
        if pk.arm <= 0.0 {
            let d = ((p.0[0] - pp[0]).powi(2) + (p.0[1] - pp[1]).powi(2)).sqrt();
            if d <= PICKUP_DIST {
                ph.hp = (ph.hp + pk.heal).min(ph.max);
                stats.pickups += 1;
                let _ = cmds.entity(e).despawn();
            }
        }
    }
}

/// FlashSystem：受击白闪衰减。连「特效淡出」都是一个系统——ECS 里没有免费的视觉。
fn flash_system(delta: Res<Delta>, mut q: Query<&mut Visual>) {
    for mut v in &mut q {
        if v.flash > 0.0 {
            v.flash = (v.flash - delta.0 * 4.0).max(0.0);
        }
    }
}

// ---------------------------------------------------------------------------
// WaveSystem（玩法扩展 Step 1）：波次生存的核心循环。
//
// 「场上怪死光了」怎么感知？——不翻任何账本，直接问世界本身：
//   query_filtered::<&Health, (Without<Controlled>, Without<Pickup>)>
//   数出敌人数，归零 = 这波清完。「世界就是真相，资源只是缓存」的又一次应用
//   （和死亡检测查 Controlled 还在不在、波次数敌人是同一个套路）。
//
// 状态流转（三态）：
//   敌人 > 0            → 战斗中，什么都不做
//   敌人 = 0, timer > 0 → 波间喘息，倒计时（给玩家捡金币）
//   敌人 = 0, timer ≤ 0 → 刷下一波：n+1 → 按混合递增公式 spawn 一圈追击怪
//
// 出生点表驱动（8 方位，i % 8 轮转）——不用随机数，行为可复现、可 review。
// ---------------------------------------------------------------------------

/// 8 个方位出生点（半径 2.9，贴着场地边缘刷，玩家有反应时间）。
const SPAWN_POINTS: [[f32; 2]; 8] = [
    [2.9, 0.0],
    [2.05, 2.05],
    [0.0, 2.9],
    [-2.05, 2.05],
    [-2.9, 0.0],
    [-2.05, -2.05],
    [0.0, -2.9],
    [2.05, -2.05],
];

/// 刷一只第 n 波的追击怪。颜色随波数偏红（视觉反馈「这波更强」）。
fn spawn_wave_enemy(cmds: &mut Commands, at: [f32; 2], n: u32) {
    // 第 1 波 (0.92,0.30,0.25) ≈ 原红色；波数越高越艳红发暗。
    let t = ((n - 1) as f32 / 9.0).min(1.0); // 第 10 波封顶
    let color = [
        0.92 - 0.12 * t,
        0.30 - 0.24 * t,
        0.25 - 0.20 * t,
        1.0,
    ];
    let hp = wave_hp(n);
    cmds.spawn((
        Position(at),
        Velocity([0.0, 0.0]),
        Chasing { speed: wave_speed(n) },
        Health { hp, max: hp, invuln: 0.0 },
        // 真模型（玩法扩展 Step 3）：BrainStem 机械猎犬，自带贴图（纹理补债）。
        // Anim.t 用出生点错开相位——不然全场的怪同步蹦跳（creep 级整齐）。
        Anim { t: at[0] * 3.0 + at[1] * 7.0, base_h: 0.02, yaw: 0.0, bob: 0.0 },
        Visual { color: [1.0, 1.0, 1.0, 1.0], size: 0.5, height: 0.02, flash: 0.0, uv_scale: 1.0, mesh: MeshId::Monster },
    ));
}

fn wave_system(
    delta: Res<Delta>,
    mut wave: ResMut<Wave>,
    mut cmds: Commands,
    q_enemies: Query<&Health, (Without<Controlled>, Without<Pickup>)>,
) {
    let enemies = q_enemies.iter().count();
    if enemies > 0 {
        return; // 战斗中
    }
    if wave.spawn_timer > 0.0 {
        wave.spawn_timer = (wave.spawn_timer - delta.0).max(0.0);
        return; // 波间喘息
    }
    // 刷下一波。
    wave.n += 1;
    let n = wave.n;
    println!("[wave] {} incoming: {} enemies, speed {:.2}, hp {:.0}", n, wave_count(n), wave_speed(n), wave_hp(n));
    for i in 0..wave_count(n) {
        let at = SPAWN_POINTS[(i as usize) % SPAWN_POINTS.len()];
        spawn_wave_enemy(&mut cmds, at, n);
    }
    // 预扣血量伏笔：刷出的怪 hp>0，下帧 enemies>0，自然回到「战斗中」态。
}

// ---------------------------------------------------------------------------
// [REVIEW M-3] Schedule：从「手动一行行调」到「声明一次，每帧整体跑」
//
//   玩具版 update()：input_system(...); chase_system(...); move_system(...); ...
//     —— 九行调用 + 顺序全靠人记，加系统忘了插队就是 bug。
//
//   bevy 版：把系统按因果链 .chain() 进 Schedule，每帧 schedule.run(&mut world) 一行。
//     顺序语义和玩具版完全一致（输入/追踪生产 Velocity → 移动消费 → 弹墙修正 →
//     战斗结算 → 清尸掉落 → 拾取 → 特效衰减）。
//
//   ApplyDeferred = 显式结算点：death_system 记的账（掉落物 spawn / 尸体 despawn）
//   在这里落地，后面的系统看到的就是结算后的世界——对齐玩具版「death 改完
//   pickup 才跑」的行为。末尾再放一个，保证所有命令在 run() 返回前全部生效
//   （渲染前的实例名单打包要看到最终实体集合）。
// ---------------------------------------------------------------------------

fn build_schedule() -> Schedule {
    let mut s = Schedule::default();
    s.add_systems(
        (
            input_system,
            chase_system,
            move_system,
            bounce_system,
            anim_system, // 玩法扩展 Step 3：程序动画（颠簸/转身）——叠在移动结果之上
            combat_system,
            nova_system, // 玩法扩展：Shift 范围斩（伤害在 death_system 前结算，本帧击杀本帧收尸）
            contact_system,
            death_system,
            ApplyDeferred, // death 的 spawn/despawn 落地
            pickup_system,
            flash_system,
            age_system, // 玩法扩展：特效碎片缩小 + 到期 despawn（落地靠末尾 ApplyDeferred）
            ApplyDeferred, // pickup 的 despawn 落地
            wave_system, // 玩法扩展：数敌人 → 波间倒计时 → 刷下一波（spawn 落地靠 run 末尾的 ApplyDeferred）
            ApplyDeferred, // wave 的 spawn 落地（run 返回前世界已最终）
        )
            .chain(),
    );
    s
}

// ---------------------------------------------------------------------------
// 渲染层：M1 Step 4 骨架原样复用（五件套 + 管线 + 深度 + per-entity uniform）。
// 这一层在迁移中一行没动——ECS 换的是「数据怎么组织」，不是「画面怎么画」。
// ---------------------------------------------------------------------------

/// 顶点：位置 + UV + 法线（M3 Step 2）。UV = 纹理地址；法线 = 这个点所在面的朝向箭头。
/// 同一个面的 4 个顶点共享同一根法线——光照靠它判断「面朝向光的程度」。
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    uv: [f32; 2],
    normal: [f32; 3],
}

impl Vertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12, // 紧跟 position 的 12 字节后面
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 20, // 紧跟 uv 的 8 字节后面
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

/// 立方体网格（M3 Step 2）：6 个面 × 4 顶点 = 24 顶点，36 索引画 12 个三角形。
/// 每面一根法线（面朝外的箭头）——Step 1 之前实体是躺平的「纸片」（全部朝天，
/// 光照看不出效果），升级成真立方体后：顶面亮、侧面按朝向分明暗，立体感立现。
/// 索引的作用：顶点复用（每面 4 个顶点而不是 6 个），24 + 36×2 字节 < 平铺 36 顶点。
const CUBE_VERTICES: [Vertex; 24] = [
    // 顶面 (+Y)：法线朝天，光照下最亮
    Vertex { position: [-0.5, 0.5, -0.5], uv: [0.0, 0.0], normal: [0.0, 1.0, 0.0] },
    Vertex { position: [-0.5, 0.5, 0.5], uv: [1.0, 0.0], normal: [0.0, 1.0, 0.0] },
    Vertex { position: [0.5, 0.5, 0.5], uv: [1.0, 1.0], normal: [0.0, 1.0, 0.0] },
    Vertex { position: [0.5, 0.5, -0.5], uv: [0.0, 1.0], normal: [0.0, 1.0, 0.0] },
    // 底面 (-Y)：埋在地下/朝地，通常看不见
    Vertex { position: [-0.5, -0.5, -0.5], uv: [0.0, 0.0], normal: [0.0, -1.0, 0.0] },
    Vertex { position: [0.5, -0.5, -0.5], uv: [1.0, 0.0], normal: [0.0, -1.0, 0.0] },
    Vertex { position: [0.5, -0.5, 0.5], uv: [1.0, 1.0], normal: [0.0, -1.0, 0.0] },
    Vertex { position: [-0.5, -0.5, 0.5], uv: [0.0, 1.0], normal: [0.0, -1.0, 0.0] },
    // 右面 (+X)
    Vertex { position: [0.5, -0.5, -0.5], uv: [0.0, 0.0], normal: [1.0, 0.0, 0.0] },
    Vertex { position: [0.5, 0.5, -0.5], uv: [1.0, 0.0], normal: [1.0, 0.0, 0.0] },
    Vertex { position: [0.5, 0.5, 0.5], uv: [1.0, 1.0], normal: [1.0, 0.0, 0.0] },
    Vertex { position: [0.5, -0.5, 0.5], uv: [0.0, 1.0], normal: [1.0, 0.0, 0.0] },
    // 左面 (-X)：背光面（光从右上来）
    Vertex { position: [-0.5, -0.5, 0.5], uv: [0.0, 0.0], normal: [-1.0, 0.0, 0.0] },
    Vertex { position: [-0.5, 0.5, 0.5], uv: [1.0, 0.0], normal: [-1.0, 0.0, 0.0] },
    Vertex { position: [-0.5, 0.5, -0.5], uv: [1.0, 1.0], normal: [-1.0, 0.0, 0.0] },
    Vertex { position: [-0.5, -0.5, -0.5], uv: [0.0, 1.0], normal: [-1.0, 0.0, 0.0] },
    // 前面 (+Z)
    Vertex { position: [-0.5, -0.5, 0.5], uv: [0.0, 0.0], normal: [0.0, 0.0, 1.0] },
    Vertex { position: [0.5, -0.5, 0.5], uv: [1.0, 0.0], normal: [0.0, 0.0, 1.0] },
    Vertex { position: [0.5, 0.5, 0.5], uv: [1.0, 1.0], normal: [0.0, 0.0, 1.0] },
    Vertex { position: [-0.5, 0.5, 0.5], uv: [0.0, 1.0], normal: [0.0, 0.0, 1.0] },
    // 后面 (-Z)
    Vertex { position: [0.5, -0.5, -0.5], uv: [0.0, 0.0], normal: [0.0, 0.0, -1.0] },
    Vertex { position: [-0.5, -0.5, -0.5], uv: [1.0, 0.0], normal: [0.0, 0.0, -1.0] },
    Vertex { position: [-0.5, 0.5, -0.5], uv: [1.0, 1.0], normal: [0.0, 0.0, -1.0] },
    Vertex { position: [0.5, 0.5, -0.5], uv: [0.0, 1.0], normal: [0.0, 0.0, -1.0] },
];

/// 每面两个三角形（0,1,2 / 0,2,3），基址 = 面号 × 4。
const CUBE_INDICES: [u16; 36] = [
    0, 1, 2, 0, 2, 3, // 顶
    4, 5, 6, 4, 6, 7, // 底
    8, 9, 10, 8, 10, 11, // 右
    12, 13, 14, 12, 14, 15, // 左
    16, 17, 18, 16, 18, 19, // 前
    20, 21, 22, 20, 22, 23, // 后
];

// ----- 网格（M3 Step 3）：Mesh = 共享的形状数据（印章），与实体（章印）分离 -----

/// 一枚已进显存的印章：顶点缓冲 + 索引缓冲 + 索引数（draw 时要画几个三角形）。
struct GpuMesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    /// 每 mesh 自己的绑定组（纹理补债）：同一 layout 的四档，但 binding 1 绑
    /// 【本 mesh 的贴图】——棋盘格给 Cube、角色贴图给 Humanoid/Monster。
    /// instance buffer（binding 3）全场共用一份，绑定组各持引用，没问题。
    bind_group: wgpu::BindGroup,
}

/// 从 glb 文件读网格（M3 Step 3 的核心新代码——「读文件」替换「手写数组」）。
/// glTF 文件里是 position/uv/normal/索引四样数据，正好就是 Vertex 格式已有的四样：
/// 管线、shader、bind group 全部零改动，只是数据来源变了。
/// 玩法扩展 Step 3 修正：遍历【所有 mesh 的所有 primitive】合并成一个网格——
/// 真实模型常由多个部件组成（BrainStem 的头/躯干/四肢是分开的 mesh），
/// 只读第一个 = 只画出一块碎片（用户验收：「完全不像人和犬」的根因）。
/// 纹理补债：同时返回第一个材质的贴图 RGBA 数据（辨识度 80% 靠贴图——
/// 只画几何 = 只画「素颜裸模」，换什么模型都不像）。
/// flip：绕 Y 转 180° 烘焙（有的模型正面朝 -Z，转过来让「脸」朝镜头）。
fn load_gltf_mesh(
    path: &str,
    flip: bool,
) -> Result<(Vec<Vertex>, Vec<u16>, Option<image::RgbaImage>), String> {
    let (doc, buffers, images) = gltf::import(path).map_err(|e| format!("glTF import: {e}"))?;

    let mut verts: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u16> = Vec::new();

    for mesh in doc.meshes() {
        for prim in mesh.primitives() {
            let reader = prim.reader(|buffer| buffers.get(buffer.index()).map(|b| b.0.as_slice()));

            // 位置：必有。
            let positions: Vec<[f32; 3]> = reader
                .read_positions()
                .ok_or_else(|| "no positions".to_string())?
                .collect();
            // 法线：没有就按「朝天」兜底（光照至少不崩，正式做法是编译期算面法线）。
            let normals: Vec<[f32; 3]> = reader
                .read_normals()
                .map(|it| it.collect())
                .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);
            // UV：没有就铺 0（走纯色分支，uv_scale=0 时采样结果根本不用）。
            let uvs: Vec<[f32; 2]> = reader
                .read_tex_coords(0)
                .map(|it| it.into_f32().collect())
                .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);

            // 合并关键：这个 primitive 的顶点追加到全局列表，索引加上「顶点基址偏移」
            // ——第 2 个部件的 0 号顶点，在合并后的大数组里是 base+0 号。
            let base = verts.len() as u32;
            for i in 0..positions.len() {
                verts.push(Vertex {
                    position: positions[i],
                    uv: uvs[i],
                    normal: normals[i],
                });
            }
            // 索引：glTF 是 u32；顶点数 < 65536 时收窄成 u16（省一半索引显存）。
            for i in reader
                .read_indices()
                .ok_or_else(|| "no indices".to_string())?
                .into_u32()
            {
                indices.push(u16::try_from(base + i).expect("vertex count > 65535, needs u32 indices"));
            }
        }
    }
    if verts.is_empty() {
        return Err("no primitives in file".into());
    }

    // 贴图：取文件里第一个材质的 base color 贴图（多材质模型取首个——
    // 我们的单 bind group 每 mesh 绑一张，够用；多材质拆组是以后的事）。
    // 实战坑：gltf crate 解出的贴图不一定是 RGBA——Duck/hero 都是 R8G8B8
    // （3 字节/像素，w*h*3）。按 format 分支手动扩位到 RGBA8；
    // 未知格式放弃贴图走纯色（比 panic 体面，Khronos 样例都覆盖了）。
    let mut texture: Option<image::RgbaImage> = None;
    for material in doc.materials() {
        if let Some(info) = material.pbr_metallic_roughness().base_color_texture() {
            let img = &images[info.texture().source().index()];
            let w = img.width as usize;
            let h = img.height as usize;
            match img.format {
                gltf::image::Format::R8G8B8 => {
                    let mut rgba = Vec::with_capacity(w * h * 4);
                    for px in img.pixels.chunks_exact(3) {
                        rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
                    }
                    texture = image::RgbaImage::from_raw(img.width, img.height, rgba);
                }
                gltf::image::Format::R8G8B8A8 => {
                    texture = image::RgbaImage::from_raw(img.width, img.height, img.pixels.clone());
                }
                other => log::warn!("unsupported texture format {other:?}, using solid color"),
            }
            break;
        }
    }

    // 归一化（M3 Step 3 的实战一课）：真实模型的坐标系千奇百怪——这只鸭子
    // 宽 165 单位、中心飘在 87 单位高空（老 DirectX 样例的厘米级坐标）。
    // 统一烘焙成「中心在原点、最大边长 1」的标准件，之后实体的 size/height
    // 语义与立方体完全一致（size = 最大边长，底面 ≈ height）。
    let mut mn = [f32::MAX; 3];
    let mut mx = [f32::MIN; 3];
    for v in &verts {
        for i in 0..3 {
            mn[i] = mn[i].min(v.position[i]);
            mx[i] = mx[i].max(v.position[i]);
        }
    }
    let center = [
        (mn[0] + mx[0]) * 0.5,
        (mn[1] + mx[1]) * 0.5,
        (mn[2] + mx[2]) * 0.5,
    ];
    let max_dim = (mx[0] - mn[0]).max(mx[1] - mn[1]).max(mx[2] - mn[2]).max(1e-6);
    let s = 1.0 / max_dim;
    for v in &mut verts {
        for i in 0..3 {
            v.position[i] = (v.position[i] - center[i]) * s;
        }
    }

    // glTF 是右手系 +Z 朝前；有的模型数据朝 -Z，flip 绕 Y 转 180° 让「脸」朝 +Z
    // （相机在 +Z 看向场地 → 正面对镜头）。平移/旋转在数据里一次烘焙最省。
    if flip {
        for v in &mut verts {
            v.position[0] = -v.position[0];
            v.position[2] = -v.position[2];
            v.normal[0] = -v.normal[0];
            v.normal[2] = -v.normal[2];
        }
    }

    Ok((verts, indices, texture))
}

/// 世界空间太阳方向（M3 Step 2）。从右上前方照来 → 顶面最亮、右面中亮、左/后面背光。
/// 方向不必精确单位化——shader 里会 normalize。
const LIGHT_DIR: [f32; 3] = [0.5, 0.8, 0.35];
/// 环境光：背光面不至于纯黑（0.35 = 背光面保留 35% 底色）。
const AMBIENT: f32 = 0.35;

/// 全局 uniform（M3 Step 4）：光照参数全场一份。原来塞在每实体 uniform 里
/// 冗余上传 10 份——实例化重构时顺手拆出来（1000 个实例也只传一份）。
/// 4 个标量 = 16 字节，天然满足 uniform 的 16 字节对齐（vec3 对齐坑用标量绕开）。
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    light_x: f32,
    light_y: f32,
    light_z: f32,
    ambient: f32,
}

/// 实例名单的一行（M3 Step 4）：每个实体自己的「个人数据」。
/// 1000 只怪 = 名单 1000 行；GPU 画第 i 个实例时按 instance_index 读第 i 行。
/// 顶点着色器两个来源拼完整顶点：形状从 mesh（全实例共享）取，
/// 「我是谁/我在哪」从名单取。96 字节，16 对齐。
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceRaw {
    mvp: [[f32; 4]; 4],
    color: [f32; 4],
    uv_scale: f32,
    _pad: [f32; 3],
}

/// 名单容量：4096 实例 × 96 字节 ≈ 384KB 显存，一次分配终身使用
/// （每帧只 write_buffer 覆盖前 n 行，不重建 buffer——贵的资源一次进显存）。
const INSTANCE_CAP: usize = 4096;

const SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec3f,
    @location(1) uv: vec2f,       // M3：顶点自带的「纹理地址」
    @location(2) normal: vec3f,   // M3 Step 2：顶点所在面的朝向箭头
};
struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) uv: vec2f,       // 光栅化自动插值：三角形内部每个像素拿到自己的 uv
    @location(1) normal: vec3f,   // 法线同样插值（立方体每个面内法线相同，插值无害）
    @location(2) color: vec4f,    // 本实例的颜色（vs 从名单取，插值传给 fs）
    @location(3) uv_scale: f32,   // 本实例的贴图密度（同样插值传递）
};
struct Globals {
    light_x: f32,
    light_y: f32,
    light_z: f32,
    ambient: f32,
};
struct Instance {
    mvp: mat4x4f,                 // 我在哪（每个实例自己的 MVP）
    color: vec4f,                 // 我是什么颜色
    uv_scale: f32,                // 我贴不贴图、铺几次
    // 踩坑实录：这里写 _pad: vec3f 会炸——vec3 对齐 16，_pad 被推到偏移 96，
    // 整行变成 112 字节；Rust 侧 [f32;3] 是 96 字节 → 从第 2 个实例起全错位，
    // 满屏垃圾三角形。拆成 3 个标量（对齐 4）就和 Rust 严丝合缝对上。
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};
@group(0) @binding(0) var<uniform> g: Globals;      // 全场共享：太阳 + 环境光
@group(0) @binding(1) var t_tex: texture_2d<f32>;   // 纹理（GPU 上的图片）
@group(0) @binding(2) var t_samp: sampler;          // 采样器（取色规则：过滤 + 地址模式）
@group(0) @binding(3) var<storage, read> insts: array<Instance>;  // 实例名单

@vertex
fn vs_main(in: VertexInput, @builtin(instance_index) ii: u32) -> VertexOutput {
    let inst = insts[ii];         // 名单第 ii 行 = 第 ii 个实例的个人数据
    var out: VertexOutput;
    out.clip_position = inst.mvp * vec4f(in.position, 1.0);
    out.uv = in.uv;               // UV 原样传下去（位置被 MVP 变换，UV 不用变）
    // 法线也原样传：模型矩阵 = 平移 × 均匀缩放，方向不变（均匀缩放不改变向量方向）。
    // 若将来加旋转/非均匀缩放，法线要用法线矩阵重新变换——届时再引入。
    out.normal = in.normal;
    out.color = inst.color;
    out.uv_scale = inst.uv_scale;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // uv_scale > 0：uv 乘上平铺次数去纹理取色（uv 超过 1 由 sampler 的 repeat 模式接管）。
    // uv_scale = 0：纯色实体，走自己的颜色分支——一张管线两种实体。
    let tex_col = textureSample(t_tex, t_samp, in.uv * in.uv_scale);
    let base = select(in.color, tex_col, in.uv_scale > 0.0);
    // 漫反射（Lambert 余弦定律）：亮度 = 面朝向光的程度。
    // dot(n, l) = 1 完全正对（最亮），0 垂直/背对（只剩环境光），负数夹掉。
    let n = normalize(in.normal);
    let l = normalize(vec3f(g.light_x, g.light_y, g.light_z));
    let diff = max(dot(n, l), 0.0);
    let light = g.ambient + (1.0 - g.ambient) * diff;
    return vec4f(base.rgb * light, base.a);
}
"#;

// ----- UI 管线（Step 3 原样）：屏幕空间，无 MVP，无深度，alpha 混合 -----

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct UiVertex {
    pos: [f32; 2],
}

impl UiVertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<UiVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x2,
            }],
        }
    }
}

const UI_QUAD: [UiVertex; 6] = [
    UiVertex { pos: [0.0, 0.0] },
    UiVertex { pos: [1.0, 0.0] },
    UiVertex { pos: [1.0, 1.0] },
    UiVertex { pos: [0.0, 0.0] },
    UiVertex { pos: [1.0, 1.0] },
    UiVertex { pos: [0.0, 1.0] },
];

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct UiUniforms {
    offset: [f32; 2],
    scale: [f32; 2],
    color: [f32; 4],
}

const UI_SHADER: &str = r#"
struct UiUniforms {
    offset: vec2f,
    scale: vec2f,
    color: vec4f,
};
@group(0) @binding(0) var<uniform> u: UiUniforms;

@vertex
fn vs_main(@location(0) pos: vec2f) -> @builtin(position) vec4f {
    let screen = pos * u.scale + u.offset;
    return vec4f(screen, 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4f {
    return u.color;
}
"#;

/// 轨道相机：M1 Step 4 原样。
struct OrbitCamera {
    target: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
    perspective: bool,
}

impl OrbitCamera {
    fn new() -> Self {
        Self {
            target: Vec3::ZERO,
            yaw: 0.0,
            pitch: 1.25, // ≈72°，接近正上方俯视
            distance: 8.0,
            perspective: true,
        }
    }

    fn eye(&self) -> Vec3 {
        let cos_pitch = self.pitch.cos();
        self.target
            + Vec3::new(
                self.distance * cos_pitch * self.yaw.sin(),
                self.distance * self.pitch.sin(),
                self.distance * cos_pitch * self.yaw.cos(),
            )
    }

    fn view(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye(), self.target, Vec3::Y)
    }

    fn projection(&self, aspect: f32) -> Mat4 {
        if self.perspective {
            Mat4::perspective_rh(60f32.to_radians(), aspect, 0.1, 100.0)
        } else {
            Mat4::orthographic_rh(-4.6 * aspect, 4.6 * aspect, -4.6, 4.6, 0.1, 100.0)
        }
    }
}

// ---------------------------------------------------------------------------
// 场景搭建：spawn(组件元组) = 玩具版的「发编号 + 逐张贴标签」一步到位。
// 实体的「本质」= 你塞进元组的那些标签——和玩具版「本质散落在各池同下标槽位」
// 是同一件事的两种说法。
// ---------------------------------------------------------------------------

fn setup_scene(world: &mut World) {
    // 地板：Position + Visual。RenderSystem 画它，其他系统都不碰。
    // M3 Step 1：uv_scale 8 = 棋盘格纹理在整块地板上平铺 8×8（repeat 地址模式）。
    // M3 Step 2：升级成立方体「平台厚板」——底面 -7.8、顶面恰好 0（height 语义
    // 是底面高度）。边缘侧面受光照分出明暗，整体读出「悬浮平台」的立体感。
    world.spawn((
        Position([0.0, 0.0]),
        Visual { color: [0.15, 0.17, 0.23, 1.0], size: 7.8, height: -7.8, flash: 0.0, uv_scale: 8.0, mesh: MeshId::Cube },
    ));

    // 玩家：白色鸭子（M3 Step 3：glTF 模型）。Health 100 + Attack（半径 0.9，一刀 34）
    // + Nova（Shift 范围斩，半径 1.6 伤害 60，5 秒一发）——多挂一张标签 = 多一种能力。
    // 玩法扩展 Step 1：初始只刷玩家——第一波由 wave_system 在开场 3 秒后送来。
    world.spawn((
        Position([0.0, 0.0]),
        Velocity([0.0, 0.0]),
        Controlled,
        Health { hp: 100.0, max: 100.0, invuln: 0.0 },
        Attack { cooldown: 0.0, radius: 0.9, damage: 34.0 },
        Nova { cooldown: 0.0, radius: NOVA_RADIUS, damage: NOVA_DAMAGE },
        // 真模型（玩法扩展 Step 3）：CesiumMan 行走人——自带皮肤贴图（纹理补债）。
        Anim { t: 0.0, base_h: 0.02, yaw: 0.0, bob: 0.0 },
        Visual { color: [1.0, 1.0, 1.0, 1.0], size: 0.75, height: 0.02, flash: 0.0, uv_scale: 1.0, mesh: MeshId::Humanoid },
    ));
}

/// 场景 + 资源填充。reset_scene 原地清场后也走这里（资源跟着一起重建）。
fn populate_world(world: &mut World) {
    setup_scene(world);
    world.insert_resource(Delta(0.0));
    world.insert_resource(Keys::default());
    world.insert_resource(Stats::default());
    // 波次从 0 开始 + 3 秒倒计时 → wave_system 开场自动送第 1 波。
    world.insert_resource(Wave { n: 0, spawn_timer: WAVE_BREAK });
}

/// 新世界 = 空世界 + 场景 + 资源（只在启动时调用一次）。
fn init_world() -> World {
    let mut world = World::new();
    populate_world(&mut world);
    world
}

/// 读玩家血量（仪表 / 血条共用）。玩家不在（死了）→ (0, 100)。
fn player_hp(world: &mut World) -> (f32, f32) {
    let mut q = world.query_filtered::<&Health, With<Controlled>>();
    q.iter(world).next().map(|h| (h.hp, h.max)).unwrap_or((0.0, 100.0))
}

// ---------------------------------------------------------------------------
// 应用状态：wgpu/winit 层与玩具版相同；world 换成 bevy 的 World。
// M3 Step 4 起 GPU 侧不再按实体记账（entity_gpu 已删）：每帧 render() 里
// 查询全量 Visual 实体重建实例名单——重建即对账。
// ---------------------------------------------------------------------------

struct App {
    // ----- 图形（沿用 M1 骨架）-----
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    device: Option<wgpu::Device>,
    queue: Option<wgpu::Queue>,
    config: Option<wgpu::SurfaceConfiguration>,
    pipeline: Option<wgpu::RenderPipeline>,
    /// 网格仓库（M3 Step 3）：MeshId → 显存里的形状数据。印章的抽屉。
    meshes: HashMap<MeshId, GpuMesh>,
    depth_view: Option<wgpu::TextureView>,
    /// 实例化渲染（M3 Step 4）：每帧把所有实体的「个人数据」（MVP/颜色/uv_scale）
    /// 打包进一个 storage buffer（名单），按 mesh 分组、一组一次 draw——
    /// draw call 从「每实体一次」降到「每 mesh 一次」。
    /// 原来的对账机器（entity_gpu HashMap：每实体 uniform + 绑定组 + 增删对账）
    /// 整套删除：名单每帧重建，重建本身就是对账。
    globals_buf: Option<wgpu::Buffer>,
    instance_buf: Option<wgpu::Buffer>,
    /// 仪表：上一帧的实例数 / draw call 数（update 里打印用）。
    instance_count: u32,
    draw_calls: u32,
    /// 纹理补债后：贴图和绑定组都归各 GpuMesh 自持（meshes 里），
    /// 不再是全场共享的单一纹理/绑定组。

    // ----- UI 渲染 -----
    ui_pipeline: Option<wgpu::RenderPipeline>,
    ui_vertex_buffer: Option<wgpu::Buffer>,
    ui_bg_buf: Option<wgpu::Buffer>,
    ui_fg_buf: Option<wgpu::Buffer>,
    ui_overlay_buf: Option<wgpu::Buffer>,
    // 玩法扩展 Step 2 教训：write_buffer 全部攒到 submit 开头统一生效——
    // 多个元素共用一个 buffer = 大家全画成最后一次写入的数据（「串行复用」
    // 是伪方案）。正解：每个 UI 元素一个自己的 buffer，互不顶替。
    // 波次格子 ×20 + 冷却槽 + 冷却前景。
    ui_cell_slots: Vec<(wgpu::Buffer, wgpu::BindGroup)>,
    ui_cd_slot: Option<(wgpu::Buffer, wgpu::BindGroup)>,
    ui_cd_fill: Option<(wgpu::Buffer, wgpu::BindGroup)>,
    ui_bg_bind: Option<wgpu::BindGroup>,
    ui_fg_bind: Option<wgpu::BindGroup>,
    ui_overlay_bind: Option<wgpu::BindGroup>,
    /// 游戏状态机
    state: GameState,

    // ----- ECS + 循环 -----
    world: World,
    schedule: Schedule,
    camera: OrbitCamera,
    keys: HashSet<KeyCode>,
    last_frame: Instant,
    frames: u64,
    last_measure: Instant,
}

impl App {
    fn new() -> Self {
        Self {
            window: None,
            surface: None,
            device: None,
            queue: None,
            config: None,
            pipeline: None,
            meshes: HashMap::new(),
            depth_view: None,
            globals_buf: None,
            instance_buf: None,
            instance_count: 0,
            draw_calls: 0,
            ui_pipeline: None,
            ui_vertex_buffer: None,
            ui_bg_buf: None,
            ui_fg_buf: None,
            ui_overlay_buf: None,
            ui_cell_slots: Vec::new(),
            ui_cd_slot: None,
            ui_cd_fill: None,
            ui_bg_bind: None,
            ui_fg_bind: None,
            ui_overlay_bind: None,
            state: GameState::Playing,
            world: init_world(),
            schedule: build_schedule(),
            camera: OrbitCamera::new(),
            keys: HashSet::new(),
            last_frame: Instant::now(),
            frames: 0,
            last_measure: Instant::now(),
        }
    }

    fn init_graphics(&mut self) {
        let window = self.window.as_ref().expect("window exists").clone();

        // 五件套（M1 原样）。
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance
            .create_surface(window.clone())
            .expect("create surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("request adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("m2-bevy device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        }))
        .expect("request device");
        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: caps.present_modes[0],
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("m2-bevy shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube vertices"),
            contents: bytemuck::cast_slice(&CUBE_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });
        // 索引缓冲（M3 Step 2）：顶点复用——24 顶点 + 36 索引画 12 个三角形，
        // 比平铺 36 个顶点省内存（GPU 索引只占 2 字节/个）。
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube indices"),
            contents: bytemuck::cast_slice(&CUBE_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        // ----- 网格仓库（M3 Step 3）：印章的抽屉 -----
        // 纹理补债后每枚印章自带绑定组（binding 1 绑自己的贴图）——
        // 必须等 bind_group_layout / sampler / instance_buf 都建好才能建 mesh，
        // 所以整段挪到下面（layout 声明之后）。Cube 用默认棋盘格；glTF 模型用自带贴图。

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("m2-bevy bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0, // 全局 uniform（M3 Step 4：光照参数，全场一份）
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1, // 纹理（纹理补债：每 mesh 绑自己的贴图）
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2, // 共享采样器
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    // 实例名单（M3 Step 4）：storage buffer，vs 按 instance_index 取行。
                    // 一份 buffer 通吃所有 mesh 组——每组 draw 自己的区间。
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // ----- 纹理（M3 Step 1）：程序生成 64×64 棋盘格，不用图片文件 -----
        // 数据流：CPU 字节数组 → write_texture 上传 → GPU 纹理。
        // 纹理补债：这段变成「默认贴图」——Cube 组（地板/金币/特效）用它；
        // Humanoid/Monster 组加载器自带贴图，各绑各的。
        let tex_size = 64u32;
        let mut pixels: Vec<u8> = Vec::with_capacity((tex_size * tex_size * 4) as usize);
        for y in 0..tex_size {
            for x in 0..tex_size {
                // 32 像素一格：整张图 = 2×2 格棋盘。深浅两档蓝灰色，贴合原地板色调。
                let checker = ((x / 32) + (y / 32)) % 2 == 0;
                let base = if checker { [46, 50, 64] } else { [30, 33, 44] };
                pixels.extend_from_slice(&[base[0], base[1], base[2], 255]);
            }
        }
        let checker_image = image::RgbaImage::from_raw(tex_size, tex_size, pixels).unwrap();
        // 采样器：linear 过滤（放大平滑），repeat 地址模式（uv 超过 1 就平铺——地板砖的关键）。
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("repeat sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // ----- 实例化三件套（M3 Step 4）-----
        // 1. 全局 uniform：光照参数写一次，之后每帧不动。
        let globals_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("globals uniform"),
            contents: bytemuck::bytes_of(&Globals {
                light_x: LIGHT_DIR[0],
                light_y: LIGHT_DIR[1],
                light_z: LIGHT_DIR[2],
                ambient: AMBIENT,
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        // 2. 实例名单：容量一次分配终身使用，每帧 write_buffer 覆盖前 n 行。
        //    （对比 Step 3 之前：那是每实体一个 buffer + 一个绑定组，实体增删
        //     还要对账；现在一个 buffer 装下全场，重建名单 = 对账。）
        let instance_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instance buffer"),
            size: (INSTANCE_CAP * std::mem::size_of::<InstanceRaw>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // 3. 绑定组：纹理补债后【每 mesh 一个】——binding 1 绑本 mesh 的贴图，
        //    其余三档（光照/采样器/名单）全场共用。实例 buffer 是同一份，
        //    多个绑定组引用它完全合法（只读）。
        let make_mesh_bind_group = |label: &str, view: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: globals_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: instance_buf.as_entire_binding(),
                    },
                ],
            })
        };
        // RGBA 图 → GPU 纹理 → 视图（一次进显存，帧内零搬运——GPU 编程黄金律）。
        let upload_texture = |label: &str, img: &image::RgbaImage| -> wgpu::TextureView {
            let (w, h) = img.dimensions();
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                img.as_raw(),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(h),
                },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
            texture.create_view(&wgpu::TextureViewDescriptor::default())
        };

        let checker_view = upload_texture("checker texture", &checker_image);

        // ----- 网格仓库（M3 Step 3）：印章的抽屉（挪到此处——建 mesh 需要
        // bind_group_layout / sampler / instance_buf / checker_view 全部就位）-----
        // Cube 用默认棋盘格；glTF 模型用自带贴图（纹理补债：每 mesh 一个绑定组，
        // binding 1 绑本 mesh 的贴图——「同一管线，各贴各的皮」）。
        let mut meshes = HashMap::new();
        meshes.insert(
            MeshId::Cube,
            GpuMesh {
                vertex_buffer,
                index_buffer,
                index_count: CUBE_INDICES.len() as u32,
                bind_group: make_mesh_bind_group("cube bind group", &checker_view),
            },
        );
        // 玩法扩展 Step 3：真模型替换方块——玩家 = CesiumMan 行走人，
        // 怪 = BrainStem 机械猎犬。两者本来就是 +Z 朝前，无需 flip。
        // 网络坑实录：quaternius.com 和 GitHub raw 在直连下超时/返回 HTML，
        // 走 ClashX 本地代理（127.0.0.1:7890）一次成功——境外资产下载记得挂代理。
        let mut load_model = |id: MeshId, path: &str, label: &str, fallback: &image::RgbaImage| {
            let (verts, indices, tex) =
                load_gltf_mesh(path, false).unwrap_or_else(|e| panic!("failed to load {path}: {e}"));
            let has_tex = tex.is_some();
            let view = match &tex {
                Some(t) => upload_texture(&format!("{label} texture"), t),
                None => upload_texture(&format!("{label} fallback texture"), fallback),
            };
            log::info!(
                "{label} mesh loaded: {} verts, {} indices, texture: {}",
                verts.len(),
                indices.len(),
                if has_tex { "yes" } else { "none" }
            );
            meshes.insert(
                id,
                GpuMesh {
                    vertex_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(format!("{label} vertices").as_str()),
                        contents: bytemuck::cast_slice(&verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                    index_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(format!("{label} indices").as_str()),
                        contents: bytemuck::cast_slice(&indices),
                        usage: wgpu::BufferUsages::INDEX,
                    }),
                    index_count: indices.len() as u32,
                    bind_group: make_mesh_bind_group(&format!("{label} bind group"), &view),
                },
            );
        };
        load_model(MeshId::Duck, "assets/Duck.glb", "duck", &checker_image);
        load_model(MeshId::Humanoid, "assets/hero.glb", "hero", &checker_image);
        load_model(MeshId::Monster, "assets/monster.glb", "monster", &checker_image);
        // vertex_buffer / index_buffer 的所有权已搬进 meshes[Cube]；
        // draw 循环从 meshes 按 MeshId 取，不再用单独字段。
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("m2-bevy pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // 深度纹理（M1 原样）。
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth texture"),
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("m2-bevy pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // ----- UI 管线：屏幕空间，无深度，alpha 混合 -----
        let ui_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui shader"),
            source: wgpu::ShaderSource::Wgsl(UI_SHADER.into()),
        });
        let ui_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ui quad vertices"),
            contents: bytemuck::cast_slice(&UI_QUAD),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ui_bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui bind layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let ui_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui pipeline layout"),
            bind_group_layouts: &[&ui_bind_layout],
            push_constant_ranges: &[],
        });
        let make_ui_buf = || {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("ui uniform"),
                contents: bytemuck::bytes_of(&UiUniforms {
                    offset: [0.0; 2],
                    scale: [0.0; 2],
                    color: [1.0; 4],
                }),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            })
        };
        let make_ui_bind = |buf: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ui bind group"),
                layout: &ui_bind_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buf.as_entire_binding(),
                }],
            })
        };
        let ui_bg_buf = make_ui_buf();
        let ui_fg_buf = make_ui_buf();
        let ui_overlay_buf = make_ui_buf();
        let ui_bg_bind = make_ui_bind(&ui_bg_buf);
        let ui_fg_bind = make_ui_bind(&ui_fg_buf);
        let ui_overlay_bind = make_ui_bind(&ui_overlay_buf);
        // 每个 UI 元素一个自己的 buffer（20 波次格子 + 冷却槽 + 冷却前景）。
        // 一次性分配、终身复用；每帧只写自己的那一小块。
        let ui_cell_slots: Vec<(wgpu::Buffer, wgpu::BindGroup)> = (0..20)
            .map(|_| {
                let b = make_ui_buf();
                let g = make_ui_bind(&b);
                (b, g)
            })
            .collect();
        let cd_slot_buf = make_ui_buf();
        let cd_slot_bind = make_ui_bind(&cd_slot_buf);
        let cd_fill_buf = make_ui_buf();
        let cd_fill_bind = make_ui_bind(&cd_fill_buf);

        let ui_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui pipeline"),
            layout: Some(&ui_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &ui_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[UiVertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &ui_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        self.surface = Some(surface);
        self.device = Some(device);
        self.queue = Some(queue);
        self.config = Some(config);
        self.pipeline = Some(pipeline);
        self.meshes = meshes;
        self.depth_view = Some(depth_view);
        self.globals_buf = Some(globals_buf);
        self.instance_buf = Some(instance_buf);
        self.ui_pipeline = Some(ui_pipeline);
        self.ui_vertex_buffer = Some(ui_vertex_buffer);
        self.ui_bg_buf = Some(ui_bg_buf);
        self.ui_fg_buf = Some(ui_fg_buf);
        self.ui_overlay_buf = Some(ui_overlay_buf);
        self.ui_cell_slots = ui_cell_slots;
        self.ui_cd_slot = Some((cd_slot_buf, cd_slot_bind));
        self.ui_cd_fill = Some((cd_fill_buf, cd_fill_bind));
        self.ui_bg_bind = Some(ui_bg_bind);
        self.ui_fg_bind = Some(ui_fg_bind);
        self.ui_overlay_bind = Some(ui_overlay_bind);
        // 实例化后不需要 init 期的 GPU 对账：名单在 render() 里每帧重建。
    }

    // -----------------------------------------------------------------------
    // 每帧更新：相机 → [Playing: 写资源 → 跑 schedule → GPU 对账] →
    // 死亡检测 → 仪表 → uniform 上传。
    // -----------------------------------------------------------------------

    fn update(&mut self) {
        let delta = self.last_frame.elapsed().as_secs_f32().min(0.1);
        self.last_frame = Instant::now();

        // ----- 相机（方向键，沿用 M1）：状态机外，Paused/GameOver 时也能转视角 -----
        let rot_speed = 1.5;
        if self.keys.contains(&KeyCode::ArrowLeft) {
            self.camera.yaw += rot_speed * delta;
        }
        if self.keys.contains(&KeyCode::ArrowRight) {
            self.camera.yaw -= rot_speed * delta;
        }
        if self.keys.contains(&KeyCode::ArrowUp) {
            self.camera.pitch += rot_speed * 0.6 * delta;
        }
        if self.keys.contains(&KeyCode::ArrowDown) {
            self.camera.pitch -= rot_speed * 0.6 * delta;
        }
        self.camera.pitch = self.camera.pitch.clamp(0.05, 1.45);
        if self.keys.contains(&KeyCode::Equal) || self.keys.contains(&KeyCode::NumpadAdd) {
            self.camera.distance -= 3.0 * delta;
        }
        if self.keys.contains(&KeyCode::Minus) || self.keys.contains(&KeyCode::NumpadSubtract) {
            self.camera.distance += 3.0 * delta;
        }
        self.camera.distance = self.camera.distance.clamp(2.0, 20.0);

        // ----- [REVIEW M-3] 状态机开关面板：只在 Playing 时跑整套系统 -----
        if self.state == GameState::Playing {
            // 桥：winit 世界（App.keys）→ ECS 世界（Keys 资源）。
            // 资源每帧覆盖写入——外部输入永远先于系统执行。
            self.world.insert_resource(Delta(delta));
            self.world.resource_mut::<Keys>().0 = self.keys.clone();
            // 九个系统一行跑完：顺序在 build_schedule() 里声明过了。
            self.schedule.run(&mut self.world);
        }

        // ----- 死亡检测 → 切 GameOver（和玩具版同构：查 Controlled 还在不在）-----
        let alive = {
            let mut q = self.world.query_filtered::<(), With<Controlled>>();
            q.iter(&self.world).next().is_some()
        };
        if !alive && self.state == GameState::Playing {
            self.state = GameState::GameOver;
            if let Some(w) = &self.window {
                w.set_title(&format!(
                    "GAME OVER — survived to wave {} (R to restart)",
                    self.world.resource::<Wave>().n
                ));
            }
        }

        // ----- 验收仪表：每 0.5 秒打印一行 -----
        self.frames += 1;
        let m_elapsed = self.last_measure.elapsed().as_secs_f32();
        if m_elapsed >= 0.5 {
            let (hp, hp_max) = player_hp(&mut self.world);
            let drops = {
                let mut q = self.world.query_filtered::<&Pickup, ()>();
                q.iter(&self.world).count()
            };
            let stats = self.world.resource::<Stats>();
            let wave = self.world.resource::<Wave>();
            println!(
                "[measure] fps≈{:.0}  wave={}  hp={:.0}/{:.0}  kills={}  pickups={}  drops_on_field={}  insts={}  draw_calls={}  state={:?}",
                self.frames as f32 / m_elapsed,
                wave.n,
                hp,
                hp_max,
                stats.kills,
                stats.pickups,
                drops,
                self.instance_count,
                self.draw_calls,
                self.state,
            );
            self.frames = 0;
            self.last_measure = Instant::now();
        }
        // 实例数据不再在这里上传——render() 开头统一打包名单（一个 buffer）。
    }

    /// R 键：原地清场重建。
    /// [REVIEW M-3] bevy 版重开为什么这么写——两个坑：
    /// 1. 不能 `self.world = init_world()` 换新 World：Schedule 里的系统第一次 run 时
    ///    和 World 绑定（缓存组件访问状态），换 World 直接 panic「mismatched World」。
    /// 2. 不能 `world.clear_all()`：bevy_ecs 0.19 里资源也是组件（挂在专属实体上），
    ///    clear_all 清掉了资源实体却留下缓存表 → 之后任何 insert_resource（我们每帧
    ///    都写 Delta！）都会踩到悬空实体而 panic。
    /// 正确姿势 = Bevy 官方 App 的做法：World 永不清空，只 despawn 场景实体、
    /// 原地重置资源值（insert_resource 对已存在的资源 = 原实体上替换值，缓存不失配）。
    /// 所有场景实体都带 Position（资源实体不带）——用组件筛出要 despawn 的就是标签思想的直接应用。
    fn reset_scene(&mut self) {
        let ids: Vec<Entity> = {
            let mut q = self.world.query_filtered::<Entity, With<Position>>();
            q.iter(&self.world).collect()
        };
        for e in ids {
            let _ = self.world.despawn(e);
        }
        setup_scene(&mut self.world);
        self.world.insert_resource(Delta(0.0));      // 原地重置：资源实体还在，只是换值
        self.world.insert_resource(Stats::default()); // Keys 不用重置：每帧都从 winit 侧覆盖写入
        self.world.insert_resource(Wave { n: 0, spawn_timer: WAVE_BREAK }); // 波次归零重开
        self.state = GameState::Playing;
        self.last_frame = Instant::now();
        if let Some(w) = &self.window {
            w.set_title("M3 Step 4: instanced (P pause / O projection / R restart / G stress)");
        }
    }

    // 渲染循环（M3 Step 4 实例化）：每帧打包「实例名单」→ 按 mesh 分组 →
    // 一组一次 draw。M2 埋的 draw call 伏笔在此收口：10 实体 10 次 →
    // 2 mesh 2 次；1000 实体也只是名单 1000 行，draw call 依旧 2 次。
    fn render(&mut self) {
        let surface = self.surface.as_ref().unwrap();
        let device = self.device.as_ref().unwrap();
        let queue = self.queue.as_ref().unwrap();
        let config = self.config.as_ref().unwrap();
        let pipeline = self.pipeline.as_ref().unwrap();
        let depth_view = self.depth_view.as_ref().unwrap();

        let frame = match surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                surface.configure(device, config);
                return;
            }
            Err(e) => {
                eprintln!("surface error: {e}");
                return;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // ----- 打包实例名单：每帧从 ECS 查询重建（重建即对账）-----
        // 先按 mesh 稳定排序：同一 mesh 的实例在名单里连续（连续才能一组一次
        // draw 画一个区间）；稳定排序保持查询顺序，帧间一致（确定性渲染）。
        // 组内顺序不影响画面——深度测试保证遮挡正确。
        let aspect = config.width as f32 / config.height as f32;
        let cam_view = self.camera.view();
        let proj = self.camera.projection(aspect);

        let mut list: Vec<(MeshId, InstanceRaw)> = Vec::new();
        {
            // 玩法扩展 Step 3：模型矩阵 M 消费动画数据——Anim.yaw（转身朝向）、
            // Age.squash（尸体压扁弹起）。渲染层不问姿势怎么算的，只消费最终矩阵
            // ——逻辑与渲染解耦的回报：动画零渲染层改动。
            let mut q = self
                .world
                .query::<(&Position, &Visual, Option<&Anim>, Option<&Age>)>();
            for (p, v, anim, age) in q.iter(&self.world) {
                // 基础 M = 搬到 (x, y, z) + [转身] + 缩放。height 语义：底面离地 → 中心 = height + size/2。
                let mut scale = Vec3::splat(v.size);
                let mut rot = Mat4::IDENTITY;
                if let Some(a) = anim {
                    if a.yaw != 0.0 {
                        rot = Mat4::from_rotation_y(a.yaw);
                    }
                }
                if let Some(ag) = age {
                    if ag.squash {
                        // 尸体压扁弹起：k 从 0 → 1，y 压到 15%、xz 撑大 40%，先弹后扁再消失。
                        let k = (ag.t / ag.life).min(1.0);
                        let bounce = (1.0 - k).max(0.0);
                        let sy = (1.0 - 0.85 * k).max(0.15) * (1.0 + 0.3 * bounce * (k * 20.0).sin().max(0.0));
                        scale = Vec3::new(v.size * (1.0 + 0.4 * k), v.size * sy, v.size * (1.0 + 0.4 * k));
                    }
                }
                let model = Mat4::from_translation(Vec3::new(p.0[0], v.height + scale.y * 0.5, p.0[1]))
                    * rot
                    * Mat4::from_scale(scale);
                // MVP 顺序（M1 Step 4 的核心考点，原样保留）。
                let mvp = (proj * cam_view * model).to_cols_array_2d();
                // 受击白闪：颜色向白色插值。
                let f = v.flash.clamp(0.0, 1.0);
                let color = [
                    v.color[0] + (1.0 - v.color[0]) * f,
                    v.color[1] + (1.0 - v.color[1]) * f,
                    v.color[2] + (1.0 - v.color[2]) * f,
                    v.color[3],
                ];
                list.push((
                    v.mesh,
                    InstanceRaw { mvp, color, uv_scale: v.uv_scale, _pad: [0.0; 3] },
                ));
            }
        }
        list.sort_by(|a, b| a.0.cmp(&b.0));
        let n = list.len().min(INSTANCE_CAP);

        // 名单本体 + 分组区间：连续同 mesh 的实例合并成一个 draw 区间。
        let mut insts: Vec<InstanceRaw> = Vec::with_capacity(n);
        let mut groups: Vec<(MeshId, std::ops::Range<u32>)> = Vec::new();
        for (mesh, raw) in list.into_iter().take(INSTANCE_CAP) {
            match groups.last_mut() {
                Some((m, r)) if *m == mesh => r.end += 1,
                _ => groups.push((mesh, insts.len() as u32..insts.len() as u32 + 1)),
            }
            insts.push(raw);
        }
        queue.write_buffer(
            self.instance_buf.as_ref().unwrap(),
            0,
            bytemuck::cast_slice(&insts),
        );
        self.instance_count = insts.len() as u32;
        self.draw_calls = groups.len() as u32;

        // ----- UI uniform 写入：开 pass 前入队（write_buffer 是队列操作）-----
        let (hp, hp_max) = player_hp(&mut self.world);
        let hp_ratio = (hp / hp_max).clamp(0.0, 1.0);

        let bar_x = -0.95;
        let bar_y = 0.88;
        let bar_w = 0.5;
        let bar_h = 0.04;

        queue.write_buffer(
            self.ui_bg_buf.as_ref().unwrap(),
            0,
            bytemuck::bytes_of(&UiUniforms {
                offset: [bar_x, bar_y],
                scale: [bar_w, bar_h],
                color: [0.15, 0.15, 0.18, 0.8],
            }),
        );
        let hp_color = if hp_ratio > 0.5 {
            [0.2, 0.8, 0.3, 1.0] // 绿
        } else if hp_ratio > 0.25 {
            [0.9, 0.7, 0.2, 1.0] // 黄
        } else {
            [0.9, 0.25, 0.2, 1.0] // 红
        };
        queue.write_buffer(
            self.ui_fg_buf.as_ref().unwrap(),
            0,
            bytemuck::bytes_of(&UiUniforms {
                offset: [bar_x, bar_y],
                scale: [bar_w * hp_ratio, bar_h],
                color: hp_color,
            }),
        );

        // 波次格子 + 范围斩冷却条：各写各的 buffer，谁也不顶谁。
        let wave_n = self.world.resource::<Wave>().n.min(20) as usize;
        {
            let cell_w = 0.028;
            let cell_gap = 0.01;
            let cell_h = 0.02;
            let y = 0.88 - 0.07;
            for (i, (buf, _)) in self.ui_cell_slots.iter().enumerate() {
                if i < wave_n {
                    queue.write_buffer(
                        buf,
                        0,
                        bytemuck::bytes_of(&UiUniforms {
                            offset: [-0.95 + i as f32 * (cell_w + cell_gap), y],
                            scale: [cell_w, cell_h],
                            color: [0.98, 0.75, 0.18, 0.9],
                        }),
                    );
                }
            }
        }
        {
            let cd = self
                .world
                .query_filtered::<&Nova, With<Controlled>>()
                .iter(&self.world)
                .next()
                .map(|n| n.cooldown)
                .unwrap_or(NOVA_CD);
            let ready = 1.0 - cd / NOVA_CD;
            let y = 0.88 - 0.11;
            queue.write_buffer(
                &self.ui_cd_slot.as_ref().unwrap().0,
                0,
                bytemuck::bytes_of(&UiUniforms {
                    offset: [-0.95, y],
                    scale: [0.24, 0.015],
                    color: [0.15, 0.15, 0.18, 0.8],
                }),
            );
            queue.write_buffer(
                &self.ui_cd_fill.as_ref().unwrap().0,
                0,
                bytemuck::bytes_of(&UiUniforms {
                    offset: [-0.95, y],
                    scale: [0.24 * ready, 0.015],
                    color: if ready >= 1.0 { [0.98, 0.75, 0.18, 0.9] } else { [0.55, 0.45, 0.20, 0.9] },
                }),
            );
        }

        if self.state != GameState::Playing {
            let overlay_color = match self.state {
                GameState::Paused => [0.02, 0.03, 0.12, 0.6],
                GameState::GameOver => [0.3, 0.02, 0.04, 0.5],
                GameState::Playing => [0.0, 0.0, 0.0, 0.0],
            };
            queue.write_buffer(
                self.ui_overlay_buf.as_ref().unwrap(),
                0,
                bytemuck::bytes_of(&UiUniforms {
                    offset: [-1.0, -1.0],
                    scale: [2.0, 2.0], // 全屏
                    color: overlay_color,
                }),
            );
        }

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("m2-bevy pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // 死亡时画面转暗红——「你死了」必须用颜色喊出来。
                        load: wgpu::LoadOp::Clear(match self.state {
                            GameState::GameOver => wgpu::Color { r: 0.30, g: 0.04, b: 0.06, a: 1.0 },
                            GameState::Paused => wgpu::Color { r: 0.05, g: 0.06, b: 0.18, a: 1.0 },
                            GameState::Playing => wgpu::Color { r: 0.08, g: 0.09, b: 0.14, a: 1.0 },
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(pipeline);
            // M3 Step 4 实例化绘制（纹理补债升级）：绑定组从「全场一个」变成
            // 「每 mesh 一个」——binding 1 各绑各的贴图（棋盘格/角色皮肤），
            // 名单（binding 3）仍是全场共用一份。每个 mesh：换绑定组+换顶点/索引
            // 缓冲，一次 draw_indexed 画自己的实例区间。1000 只怪 = 名单 1000 行，
            // draw call 依旧每组一次。「按印章分组盖章」+「各盖各的皮」。
            for (mesh_id, range) in &groups {
                let m = self
                    .meshes
                    .get(mesh_id)
                    .expect("entity references unknown mesh");
                pass.set_bind_group(0, &m.bind_group, &[]);
                pass.set_vertex_buffer(0, m.vertex_buffer.slice(..));
                pass.set_index_buffer(m.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..m.index_count, 0, range.clone());
            }

            // ----- 渲染顺序：先画游戏，再画 UI（UI 永远在最上层）-----
            let ui_pipeline = self.ui_pipeline.as_ref().unwrap();
            let ui_vbuf = self.ui_vertex_buffer.as_ref().unwrap();

            pass.set_pipeline(ui_pipeline);
            pass.set_vertex_buffer(0, ui_vbuf.slice(..));

            if let Some(bg) = &self.ui_bg_bind {
                pass.set_bind_group(0, bg, &[]);
                pass.draw(0..6, 0..1);
            }
            if let Some(fg) = &self.ui_fg_bind {
                pass.set_bind_group(0, fg, &[]);
                pass.draw(0..6, 0..1);
            }
            // 波次格子 + 冷却条：每个元素自己的 buffer + bind group。
            // write 已在开 pass 前全部入队（各写各的，互不顶替），这里纯 draw。
            {
                let wave_n = self.world.resource::<Wave>().n.min(20) as usize;
                for (i, (_, bind)) in self.ui_cell_slots.iter().enumerate() {
                    if i < wave_n {
                        pass.set_bind_group(0, bind, &[]);
                        pass.draw(0..6, 0..1);
                    }
                }
                if let Some((_, bind)) = &self.ui_cd_slot {
                    pass.set_bind_group(0, bind, &[]);
                    pass.draw(0..6, 0..1);
                }
                if let Some((_, bind)) = &self.ui_cd_fill {
                    pass.set_bind_group(0, bind, &[]);
                    pass.draw(0..6, 0..1);
                }
            }
            if self.state != GameState::Playing {
                if let Some(ov) = &self.ui_overlay_bind {
                    pass.set_bind_group(0, ov, &[]);
                    pass.draw(0..6, 0..1);
                }
            }
        }
        queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }

    fn resize_depth(&mut self) {
        if let (Some(cfg), Some(dev)) = (self.config.as_ref(), self.device.as_ref()) {
            let texture = dev.create_texture(&wgpu::TextureDescriptor {
                label: Some("depth texture"),
                size: wgpu::Extent3d {
                    width: cfg.width,
                    height: cfg.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.depth_view = Some(texture.create_view(&wgpu::TextureViewDescriptor::default()));
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(
                    winit::window::WindowAttributes::default()
                        .with_title("M3 Step 4: instanced (P pause / O projection / R restart / G stress)")
                        .with_inner_size(winit::dpi::LogicalSize::new(900.0, 600.0)),
                )
                .expect("create window"),
        );
        window.request_redraw();
        self.window = Some(window);
        self.init_graphics();
        println!("m2-bevy: bevy_ecs {} — toy ECS migrated, game behavior unchanged", "0.19");
        println!("controls: WASD move | Space slash | arrows orbit camera | +/- zoom | P pause | O projection | R restart | Esc quit");
        println!("game loop: slash enemies (they flash white) -> they drop gold -> walk over it to heal; chasers bite you on contact");
        println!("states: P toggles pause (freezes sim, camera still orbits); player death -> GameOver overlay; R restarts");
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    match event.state {
                        ElementState::Pressed => {
                            if code == KeyCode::Escape {
                                event_loop.exit();
                            } else if code == KeyCode::KeyG && !event.repeat {
                                // M3 Step 4 压测：一次撒 1000 座「雕像」立方体
                                // （只有 Position + Visual，游戏系统全部无视——
                                // 标签思想：没挂 Health/Velocity 就不参与战斗/移动）。
                                // 验收点：insts 涨到 1000+，draw_calls 依旧是 2，
                                // fps 基本不动——这就是实例化的意义。
                                // 确定性散布（不用随机数）：黄金角螺旋铺满地板。
                                for i in 0..1000u32 {
                                    let a = i as f32 * 2.39996; // 黄金角（弧度）
                                    let r = 0.12 * (i as f32).sqrt(); // 渐开半径，最大 ≈3.8
                                    self.world.spawn((
                                        Position([r * a.cos(), r * a.sin()]),
                                        Visual {
                                            color: [0.55, 0.56, 0.62, 1.0],
                                            size: 0.12,
                                            height: 0.0,
                                            flash: 0.0,
                                            uv_scale: 0.0,
                                            mesh: MeshId::Cube,
                                        },
                                    ));
                                }
                                println!("stress: spawned 1000 statues (watch insts/draw_calls in [measure])");
                            } else if code == KeyCode::KeyR && !event.repeat {
                                self.camera = OrbitCamera::new();
                                self.reset_scene();
                                println!("scene reset: player hp=100, kills=0");
                            } else if code == KeyCode::KeyP && !event.repeat {
                                // P = 暂停/恢复（状态切换发生在事件层，不是系统）
                                self.state = match self.state {
                                    GameState::Playing => {
                                        if let Some(w) = &self.window {
                                            w.set_title("PAUSED — press P to resume");
                                        }
                                        GameState::Paused
                                    }
                                    GameState::Paused => {
                        if let Some(w) = &self.window {
                            w.set_title("M3 Step 4: instanced (P pause / O projection / R restart / G stress)");
                        }
                        GameState::Playing
                    }
                                    GameState::GameOver => GameState::GameOver, // 死了不能暂停
                                };
                            } else if code == KeyCode::KeyO && !event.repeat {
                                // O = 切透视/正交
                                self.camera.perspective = !self.camera.perspective;
                                println!(
                                    "projection = {}",
                                    if self.camera.perspective {
                                        "perspective (near bigger)"
                                    } else {
                                        "orthographic (top-down 2D look)"
                                    }
                                );
                            } else {
                                self.keys.insert(code);
                            }
                        }
                        ElementState::Released => {
                            self.keys.remove(&code);
                        }
                    }
                }
            }
            WindowEvent::Focused(false) => self.keys.clear(),
            WindowEvent::RedrawRequested => {
                self.update();
                self.render();
                self.window.as_ref().unwrap().request_redraw();
            }
            WindowEvent::Resized(new_size) => {
                if let (Some(cfg), Some(dev)) = (self.config.as_mut(), self.device.as_ref()) {
                    cfg.width = new_size.width.max(1);
                    cfg.height = new_size.height.max(1);
                    if let Some(s) = self.surface.as_ref() {
                        s.configure(dev, cfg);
                    }
                    self.resize_depth();
                }
            }
            _ => {}
        }
    }
}

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new();
    event_loop.run_app(&mut app).expect("run app");
}

// ---------------------------------------------------------------------------
// 回归测试：钉死 R 重开踩过的两个 bevy_ecs 坑（不需要窗口/GPU，纯 ECS 层复现）。
//
// 坑 A「换新 World」：Schedule 的系统第一次 run 时绑定 WorldId，
//     `schedule.run(&mut init_world())` → panic "mismatched World"。
// 坑 B「clear_all()」：0.19 里资源也是组件，clear_all 清掉资源实体却留下缓存表，
//     之后任何 insert_resource（游戏里每帧都写 Delta）→ panic "ResourceCache is in sync"。
//
// 现行解法：同一个 World，despawn 场景实体 + 原地重置资源。测试跑完整链路：
// 玩 → 击杀（despawn + 掉落）→ 玩家死 → reset → 重开继续玩 60 帧，全程无 panic。
// ---------------------------------------------------------------------------

/// 模拟一帧 Playing：写 Delta/Keys（对齐 App::update 里的桥接）→ 跑 Schedule。
#[cfg(test)]
fn sim_frame(world: &mut World, schedule: &mut Schedule) {
    world.insert_resource(Delta(1.0 / 60.0));
    world.resource_mut::<Keys>().0 = HashSet::new();
    schedule.run(world);
}

#[test]
fn restart_after_death_survives_schedule_and_resources() {
    let mut world = init_world();
    let mut schedule = build_schedule();

    // 正常玩 10 帧。
    for _ in 0..10 {
        sim_frame(&mut world, &mut schedule);
    }

    // 击杀全部敌人（hp 归零），death_system 走 Commands：despawn 尸体 + spawn 掉落物。
    {
        let mut q = world.query_filtered::<&mut Health, Without<Controlled>>();
        for mut h in q.iter_mut(&mut world) {
            h.hp = 0.0;
        }
    }
    for _ in 0..10 {
        sim_frame(&mut world, &mut schedule);
    }
    assert!(world.resource::<Stats>().kills > 0, "kills recorded");
    assert!(
        {
            let mut q = world.query_filtered::<&Pickup, ()>();
            q.iter(&world).count() > 0
        },
        "drops spawned"
    );

    // 玩家死亡 → despawn（GameOver 判据 = 查不到 Controlled）。
    {
        let mut q = world.query_filtered::<&mut Health, With<Controlled>>();
        for mut h in q.iter_mut(&mut world) {
            h.hp = 0.0;
        }
    }
    for _ in 0..5 {
        sim_frame(&mut world, &mut schedule);
    }
    assert!(
        {
            let mut q = world.query_filtered::<(), With<Controlled>>();
            q.iter(&world).next().is_none()
        },
        "player despawned"
    );

    // ===== R 重开：App::reset_scene 的 World 部分（原地对账）=====
    let ids: Vec<Entity> = {
        let mut q = world.query_filtered::<Entity, With<Position>>();
        q.iter(&world).collect()
    };
    for e in ids {
        let _ = world.despawn(e);
    }
    setup_scene(&mut world);
    world.insert_resource(Delta(0.0)); // 坑 B 的雷区：reset 后第一次 insert_resource
    world.insert_resource(Stats::default());

    // 重开后再玩 60 帧：每帧 insert_resource(Delta) + schedule.run（坑 A/B 任一复发即 panic）。
    for _ in 0..60 {
        sim_frame(&mut world, &mut schedule);
    }
    assert!(
        {
            let mut q = world.query_filtered::<(), With<Controlled>>();
            q.iter(&world).next().is_some()
        },
        "player alive after restart"
    );
    assert_eq!(world.resource::<Stats>().kills, 0, "stats reset");
}
