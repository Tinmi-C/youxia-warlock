//! M2 第三步：UI + 游戏状态机——屏幕空间血条 / 暂停 / 死亡覆盖层。
//! 在 Step 2 战斗系统上新增：GameState 三态机（Playing / Paused / GameOver）+
//! 一条屏幕空间 UI 渲染管线（无 MVP、无深度、alpha 混合）画 HP 条与状态覆盖层。
//!
//! 游戏闭环：Space 挥砍（半径内敌人扣血白闪）→ 敌人死 → despawn + 原地掉金色补给
//! → 走过去拾取回血；敌人贴脸扣玩家血（带无敌帧）；玩家死 → R 重开。
//! 状态机：P 暂停/恢复（冻结游戏系统，相机仍可转）| O 切透视/正交 | R 重开。
//!
//! 操作：WASD 移动 | Space 挥砍 | 方向键 相机 | +/- 缩放 | P 暂停 | O 透视/正交 | R 重开 | Esc 退出
//! Review 标记：搜 `[REVIEW 3-` 找本步 3 个审查点（状态机 / UI 管线 / 渲染顺序）。

use std::collections::HashSet;
use std::f32::consts::FRAC_PI_2;
use std::sync::Arc;
use std::time::Instant;

use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

// ---------------------------------------------------------------------------
// [REVIEW 3-1] 游戏状态机：用类型控制「这帧跑哪些系统」
// ---------------------------------------------------------------------------
// 没有它：所有系统每帧都跑，死了还在移动、暂停了还在咬人。
// 有了它：update() 开头 match state，只在 Playing 时跑游戏系统。
// 这就是「状态机 = 系统的开关面板」——和 ECS 的「标签 = 系统的筛子」是两层不同的控制。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum GameState {
    Playing,
    Paused,
    GameOver,
}

// ---------------------------------------------------------------------------
// [REVIEW 1] ECS 核心：实体 = 编号，组件 = 池子里的槽位
// ---------------------------------------------------------------------------
//
// 对比 M1 的写法（struct WgpuApp { player_pos, enemy1_pos, enemy2_pos, ... }）：
// 这里加一种新怪 = spawn 一个新编号 + 贴几张标签，所有系统代码零改动。
//
// 「池子」= Vec<Option<T>>，下标就是实体编号：
//   贴标签  = set_pos / set_vel ...  → 槽位写成 Some
//   摘标签  = 该槽位写回 None
//   有没有那张标签，本身就是信息（没有 Velocity 的实体 MoveSystem 碰都不碰）

/// 实体：只是一个编号。它自己没有任何数据、没有任何逻辑。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct EntityId(usize);

/// 组件（全是纯数据，没有方法）：游戏平面坐标 (x, z)。
#[derive(Clone, Copy, Debug)]
struct Position([f32; 2]);

/// 组件：速度向量（单位/秒）。M1 里是「方向入参 × speed 字段」，现在升级成完整向量。
#[derive(Clone, Copy, Debug)]
struct Velocity([f32; 2]);

/// 组件（标签）：接受玩家输入。谁贴了它，InputSystem 就替谁写 Velocity。
#[derive(Clone, Copy, Debug)]
struct Controlled;

/// 组件：追踪玩家的参数。谁贴了它，ChaseSystem 就每帧把它的 Velocity 指向玩家。
#[derive(Clone, Copy, Debug)]
struct Chasing {
    speed: f32,
}

/// 组件：外观。渲染用（颜色 / 大小 / 悬浮高度）。
/// flash = 受击白闪强度（0~1，由 FlashSystem 衰减）——连「特效」都是数据。
#[derive(Clone, Copy, Debug)]
struct Visual {
    color: [f32; 4],
    size: f32,
    height: f32,
    flash: f32,
}

/// 组件：生命值。hp 归零 = 死（death_system 收尸）。invuln = 无敌帧倒计时（秒）。
#[derive(Clone, Copy, Debug)]
struct Health {
    hp: f32,
    max: f32,
    invuln: f32,
}

/// 组件：攻击参数（冷却/半径/伤害都是数据，不是硬编码——换把武器就是换一张标签）。
#[derive(Clone, Copy, Debug)]
struct Attack {
    cooldown: f32,
    radius: f32,
    damage: f32,
}

/// 组件：掉落物。heal = 拾取回血量，arm = 落地保护倒计时（防击杀瞬间秒拾）。
#[derive(Clone, Copy, Debug)]
struct Pickup {
    heal: f32,
    arm: f32,
}

/// 世界 = 所有组件池的集合。实体的「本质」散落在各池的同下标槽位里。
struct World {
    next: usize,                      // 下一个编号（只增不减——见 despawn）
    pos: Vec<Option<Position>>,       // ↓ 每个池都和编号对齐
    vel: Vec<Option<Velocity>>,
    ctl: Vec<Option<Controlled>>,
    chase: Vec<Option<Chasing>>,
    vis: Vec<Option<Visual>>,
    health: Vec<Option<Health>>,
    attack: Vec<Option<Attack>>,
    pickup: Vec<Option<Pickup>>,
}

impl World {
    fn new() -> Self {
        Self {
            next: 0,
            pos: Vec::new(),
            vel: Vec::new(),
            ctl: Vec::new(),
            chase: Vec::new(),
            vis: Vec::new(),
            health: Vec::new(),
            attack: Vec::new(),
            pickup: Vec::new(),
        }
    }

    /// 发编号：所有池压入一个空槽位。
    fn spawn(&mut self) -> EntityId {
        let id = EntityId(self.next);
        self.next += 1;
        self.pos.push(None);
        self.vel.push(None);
        self.ctl.push(None);
        self.chase.push(None);
        self.vis.push(None);
        self.health.push(None);
        self.attack.push(None);
        self.pickup.push(None);
        id
    }

    // 贴标签（每个组件一个 set 方法——「贴标签」这个动作在代码里就长这样）
    fn set_pos(&mut self, id: EntityId, p: Position) {
        self.pos[id.0] = Some(p);
    }
    fn set_vel(&mut self, id: EntityId, v: Velocity) {
        self.vel[id.0] = Some(v);
    }
    fn set_controlled(&mut self, id: EntityId) {
        self.ctl[id.0] = Some(Controlled);
    }
    fn set_chasing(&mut self, id: EntityId, c: Chasing) {
        self.chase[id.0] = Some(c);
    }
    fn set_visual(&mut self, id: EntityId, v: Visual) {
        self.vis[id.0] = Some(v);
    }
    fn set_health(&mut self, id: EntityId, h: Health) {
        self.health[id.0] = Some(h);
    }
    fn set_attack(&mut self, id: EntityId, a: Attack) {
        self.attack[id.0] = Some(a);
    }
    fn set_pickup(&mut self, id: EntityId, p: Pickup) {
        self.pickup[id.0] = Some(p);
    }

    /// despawn = 摘掉所有标签（该编号在所有池的槽位写回 None）。
    /// 编号不回收——为什么 + 工业版怎么做，见 death_system 的 [REVIEW 2-1]。
    fn despawn(&mut self, id: EntityId) {
        let i = id.0;
        self.pos[i] = None;
        self.vel[i] = None;
        self.ctl[i] = None;
        self.chase[i] = None;
        self.vis[i] = None;
        self.health[i] = None;
        self.attack[i] = None;
        self.pickup[i] = None;
    }
}

/// 场地边界（半边长）。地板视觉尺寸 7.8，实体活动范围略小，看起来像有墙。
const BOUND: f32 = 3.3;
/// 玩家速度（单位/秒）。故意比追踪怪快，玩家才能遛怪。
const PLAYER_SPEED: f32 = 2.2;
// ----- 战斗参数（全是可调的「策划数字」，平衡性就靠它们）-----
/// 挥砍冷却（秒）。按住 Space 连砍，每 0.45s 一刀。
const SLASH_CD: f32 = 0.45;
/// 敌人贴脸判定距离 / 单次伤害 / 玩家无敌帧时长（秒）。
const CONTACT_DIST: f32 = 0.40;
const CONTACT_DAMAGE: f32 = 15.0;
const INVULN_TIME: f32 = 0.9;
/// 拾取判定距离。
const PICKUP_DIST: f32 = 0.45;

// ---------------------------------------------------------------------------
// 系统层：每个系统只筛自己关心的标签组合，互不认识
// ---------------------------------------------------------------------------

/// InputSystem：筛「Controlled + Position」→ 按键盘写它的 Velocity。
/// （M1 教训继承：斜向要归一化，否则 W+D 会快 41%）
fn input_system(world: &mut World, keys: &HashSet<KeyCode>) {
    let mut dir = [0.0f32; 2];
    if keys.contains(&KeyCode::KeyW) {
        dir[1] -= 1.0; // W = 屏幕上方 = 世界 -z（相机默认从 +z 俯视）
    }
    if keys.contains(&KeyCode::KeyS) {
        dir[1] += 1.0;
    }
    if keys.contains(&KeyCode::KeyA) {
        dir[0] -= 1.0;
    }
    if keys.contains(&KeyCode::KeyD) {
        dir[0] += 1.0;
    }
    let len = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
    let v = if len > 0.0 {
        [dir[0] / len * PLAYER_SPEED, dir[1] / len * PLAYER_SPEED]
    } else {
        [0.0, 0.0]
    };
    for i in 0..world.next {
        if world.ctl[i].is_some() && world.pos[i].is_some() {
            world.vel[i] = Some(Velocity(v));
        }
    }
}

/// [REVIEW 2] ChaseSystem：筛「Chasing + Position」→ 把 Velocity 写成「指向玩家」。
///
/// 这是「生产者 / 消费者」分工的关键一环：
///   ChaseSystem 生产 Velocity（方向朝玩家）→ MoveSystem 消费 Velocity（照常移动）
/// 新能力（追踪）= 新系统 + 新标签，MoveSystem 一行没改——这就是你 Q2 答案的代码形态。
fn chase_system(world: &mut World) {
    // 先找玩家（第一个贴了 Controlled + Position 的实体）。
    let player = (0..world.next)
        .find(|&i| world.ctl[i].is_some() && world.pos[i].is_some())
        .and_then(|i| world.pos[i])
        .map(|p| p.0);
    let Some(player) = player else { return };

    for i in 0..world.next {
        if let (Some(ch), Some(p)) = (world.chase[i], world.pos[i]) {
            let dx = player[0] - p.0[0];
            let dz = player[1] - p.0[1];
            let len = (dx * dx + dz * dz).sqrt();
            // 守卫：len == 0 时除法会产生 NaN，位置一旦变 NaN 实体就直接消失。
            // （真实引擎里追踪 AI 到达目标点附近都会有这类「死区」处理）
            world.vel[i] = Some(Velocity(if len > 1e-4 {
                [dx / len * ch.speed, dz / len * ch.speed]
            } else {
                [0.0, 0.0]
            }));
        }
    }
}

/// MoveSystem：筛「Position + Velocity」→ pos += vel × delta。
/// 和 M1 的公式一模一样，只是入参从「方向 × speed」变成了完整的速度向量。
/// 注意它完全不知道谁在写 Velocity（输入？追踪？将来掉落物的重力？）——不关它的事。
fn move_system(world: &mut World, delta: f32) {
    for i in 0..world.next {
        // 组件是纯数据（Copy）：拷出来 → 算 → 写回。
        // （池子槽位本身是 Option，get_mut 拿到的是「两层 Option」，直接模式匹配容易绕晕）
        if let (Some(p), Some(v)) = (world.pos[i], world.vel[i]) {
            world.pos[i] = Some(Position([p.0[0] + v.0[0] * delta, p.0[1] + v.0[1] * delta]));
        }
    }
}

/// BounceSystem：筛「Position + Velocity」→ 撞墙反弹 + 夹回场地。
/// 玩家撞墙：本帧速度被翻转，但下一帧输入系统会重写——净效果 = 贴墙停住，无副作用。
fn bounce_system(world: &mut World) {
    for i in 0..world.next {
        if let (Some(p), Some(v)) = (world.pos[i], world.vel[i]) {
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
            world.pos[i] = Some(Position([x.clamp(-BOUND, BOUND), z.clamp(-BOUND, BOUND)]));
            world.vel[i] = Some(Velocity([vx, vz]));
        }
    }
}

// ---------------------------------------------------------------------------
// 战斗系统群（Step 2 新增）：挥砍 / 贴脸伤害 / 死亡清尸 / 拾取 / 白闪衰减
// ---------------------------------------------------------------------------

/// [REVIEW 2-2] CombatSystem：挥砍——贴了「Attack + Position」的实体按 Space 出刀，
/// 半径内所有「Health + Position」（排除玩家自己、排除掉落物）扣血 + 白闪。
///
/// 多写手问题在这里再现，但形态不同：Health 有三个写手（挥砍扣 / 贴脸扣 / 拾取回），
/// 却没有 Velocity 那种覆盖雷——因为扣血/回血是「加减法」，先扣哪笔结果一样（可交换）；
/// 而 Velocity 是「赋值」，后写吃掉先写。判据升级：
///   多写手安全 ⟺ 操作可交换（或显式协调），否则只留一个写手。
fn combat_system(world: &mut World, keys: &HashSet<KeyCode>, delta: f32) {
    let want = keys.contains(&KeyCode::Space);

    // 第一遍：冷却倒计时（贴在组件上的状态）+ 收集本帧出刀的攻击者。
    let mut slashes: Vec<([f32; 2], f32, f32)> = Vec::new(); // (位置, 半径, 伤害)
    for i in 0..world.next {
        if let (Some(mut a), Some(p)) = (world.attack[i], world.pos[i]) {
            a.cooldown = (a.cooldown - delta).max(0.0);
            if want && a.cooldown <= 0.0 {
                slashes.push((p.0, a.radius, a.damage));
                a.cooldown = SLASH_CD; // 出刀后进入冷却
            }
            world.attack[i] = Some(a);
        }
    }
    if slashes.is_empty() {
        return;
    }

    // 第二遍：结算伤害（拷出来 → 算 → 写回，玩具 ECS 的固定套路）。
    for i in 0..world.next {
        if world.ctl[i].is_some() || world.pickup[i].is_some() {
            continue; // 不砍自己，不砍掉落物
        }
        if let (Some(h), Some(p)) = (world.health[i], world.pos[i]) {
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
                world.health[i] = Some(Health { hp, ..h });
                if let Some(v) = world.vis[i] {
                    world.vis[i] = Some(Visual { flash: 1.0, ..v }); // 白闪反馈
                }
            }
        }
    }
}

/// ContactSystem：敌人（有 Health、无 Controlled、无 Pickup）贴到玩家身上 → 扣血 + 无敌帧。
/// 筛选里的「排除条件」：不是谁被处理，而是谁明确不被处理。
fn contact_system(world: &mut World, delta: f32) {
    let Some(pi) = (0..world.next)
        .find(|&i| world.ctl[i].is_some() && world.pos[i].is_some() && world.health[i].is_some())
    else {
        return; // 玩家死了（被 despawn）就没人可咬
    };
    let Some(pp) = world.pos[pi].map(|p| p.0) else { return };
    let Some(mut h) = world.health[pi] else { return };

    // 无敌帧倒计时（Health 组件上的小状态机）
    h.invuln = (h.invuln - delta).max(0.0);

    if h.invuln <= 0.0 {
        for i in 0..world.next {
            if i == pi || world.ctl[i].is_some() || world.pickup[i].is_some() {
                continue;
            }
            if let (Some(_), Some(ep)) = (world.health[i], world.pos[i]) {
                let d = ((ep.0[0] - pp[0]).powi(2) + (ep.0[1] - pp[1]).powi(2)).sqrt();
                if d <= CONTACT_DIST {
                    h.hp -= CONTACT_DAMAGE;
                    h.invuln = INVULN_TIME;
                    if let Some(v) = world.vis[pi] {
                        world.vis[pi] = Some(Visual { flash: 1.0, ..v });
                    }
                    break; // 一帧最多挨一刀（无敌帧开了，后面的也咬不动）
                }
            }
        }
    }
    world.health[pi] = Some(h);
}

/// [REVIEW 2-1] DeathSystem：hp ≤ 0 → despawn + 掉落。
///
/// despawn 的本质 = 该编号在所有池的槽位写回 None（标签全摘）——「尸体」还占着编号，
/// 但任何系统的筛选都筛不出它了。注意：编号【不回收】，next 只增不减。
/// 工业版（bevy_ecs）用「空位链表 + 世代号」回收编号：防止悬空的旧 EntityId
/// 误伤复用了同编号的新实体。玩具版不回收，千实体级毫无压力。
fn death_system(world: &mut World, kills: &mut u32) {
    // 先收集再动手：循环里既要 spawn（掉落）又要摘标签，边迭代边改会出乱子。
    let mut dead: Vec<(usize, [f32; 2], bool)> = Vec::new(); // (编号, 位置, 是玩家吗)
    for i in 0..world.next {
        if let Some(h) = world.health[i] {
            if h.hp <= 0.0 {
                let p = world.pos[i].map(|p| p.0).unwrap_or([0.0, 0.0]);
                dead.push((i, p, world.ctl[i].is_some()));
            }
        }
    }
    for (i, p, is_player) in dead {
        if is_player {
            println!("GAME OVER: player down (kills={}) — press R to restart", *kills);
        } else {
            *kills += 1;
            spawn_drop(world, p); // 敌人死 → 原地掉一个金色补给
        }
        world.despawn(EntityId(i));
    }
}

/// 掉落物：Position + Pickup + Visual（金色小方块），无 Velocity → MoveSystem 不碰 → 永远不动。
/// arm = 落地保护 0.6 秒：玩家就站在尸体旁，防止「击杀瞬间秒拾」直接白拿。
fn spawn_drop(world: &mut World, at: [f32; 2]) {
    let d = world.spawn();
    world.set_pos(d, Position(at));
    world.set_pickup(d, Pickup { heal: 10.0, arm: 0.6 });
    world.set_visual(
        d,
        Visual { color: [0.95, 0.80, 0.25, 1.0], size: 0.22, height: 0.10, flash: 0.0 },
    );
}

/// PickupSystem：掉落物落地（arm 归零）后，玩家碰到 → 回血（不超上限）+ despawn。
fn pickup_system(world: &mut World, delta: f32, collected: &mut u32) {
    let Some(pp) = (0..world.next)
        .find(|&i| world.ctl[i].is_some() && world.pos[i].is_some())
        .and_then(|i| world.pos[i])
        .map(|p| p.0)
    else {
        return;
    };
    // 先走 arm 计时 + 收集够得着的（拷出来 → 算 → 写回）
    let mut take: Vec<(usize, f32)> = Vec::new(); // (编号, 回血量)
    for i in 0..world.next {
        if let (Some(mut pk), Some(p)) = (world.pickup[i], world.pos[i]) {
            pk.arm = (pk.arm - delta).max(0.0);
            world.pickup[i] = Some(pk);
            if pk.arm <= 0.0 {
                let d = ((p.0[0] - pp[0]).powi(2) + (p.0[1] - pp[1]).powi(2)).sqrt();
                if d <= PICKUP_DIST {
                    take.push((i, pk.heal));
                }
            }
        }
    }
    for (i, heal) in take {
        if let Some(pi) = (0..world.next).find(|&j| world.ctl[j].is_some()) {
            if let Some(h) = world.health[pi] {
                world.health[pi] = Some(Health { hp: (h.hp + heal).min(h.max), ..h });
            }
        }
        *collected += 1;
        world.despawn(EntityId(i));
    }
}

/// FlashSystem：受击白闪衰减（约 0.25 秒淡出）。
/// 连「特效淡出」都是一个系统——ECS 里没有免费的视觉，一切变化都要有人去写数据。
fn flash_system(world: &mut World, delta: f32) {
    for i in 0..world.next {
        if let Some(v) = world.vis[i] {
            if v.flash > 0.0 {
                world.vis[i] = Some(Visual { flash: (v.flash - delta * 4.0).max(0.0), ..v });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 渲染层：M1 Step 4 的骨架原样复用（五件套 + 管线 + 深度 + per-entity uniform）
// ---------------------------------------------------------------------------

/// 顶点：只有位置。颜色从哪来？uniform（每个实体一份）——顶点缓冲只存「形状」。
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
}

impl Vertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            }],
        }
    }
}

/// 单位方块（XY 平面、中心在原点、边长 1），两个三角形 6 个顶点，不用索引。
/// 所有实体共用这一份几何——差异全在 uniform 的 M 矩阵（位置/大小）和颜色里。
const QUAD_VERTICES: [Vertex; 6] = [
    Vertex { position: [-0.5, -0.5, 0.0] },
    Vertex { position: [0.5, -0.5, 0.0] },
    Vertex { position: [0.5, 0.5, 0.0] },
    Vertex { position: [-0.5, -0.5, 0.0] },
    Vertex { position: [0.5, 0.5, 0.0] },
    Vertex { position: [-0.5, 0.5, 0.0] },
];

/// 每个实体一份：MVP（去往屏幕的完整旅程）+ 颜色。
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    mvp: [[f32; 4]; 4],
    color: [f32; 4],
}

const SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec3f,
};
struct VertexOutput {
    @builtin(position) clip_position: vec4f,
};
struct Uniforms {
    mvp: mat4x4f,
    color: vec4f,
};
@group(0) @binding(0) var<uniform> u: Uniforms;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    // M1 Step 4 的 MVP 乘法原样保留：本地 → 世界 → 相机 → 屏幕。
    out.clip_position = u.mvp * vec4f(in.position, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // 颜色不再写死在顶点里，而是每个实体自己的 uniform（RGB 三盏灯，M1 的老朋友）。
    return u.color;
}
"#;

// ---------------------------------------------------------------------------
// [REVIEW 3-2] UI 渲染管线：屏幕空间，无 MVP，无深度
// ---------------------------------------------------------------------------
// 游戏管线（上面那套）：顶点乘 MVP 矩阵 → 3D 世界 → 屏幕。
// UI 管线（下面这套）：顶点直接是屏幕坐标（NDC），不乘矩阵，不测深度。
// 两条管线共用同一份方块几何，但用不同的 shader + pipeline。
// UI 永远画在游戏画面之上：render() 里先画游戏，再画 UI。

/// UI 顶点：只有 2D 位置（屏幕空间 NDC）。
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

/// 单位方块（0~1），6 个顶点。通过 uniform 的 offset+scale 摆到屏幕任意位置。
const UI_QUAD: [UiVertex; 6] = [
    UiVertex { pos: [0.0, 0.0] },
    UiVertex { pos: [1.0, 0.0] },
    UiVertex { pos: [1.0, 1.0] },
    UiVertex { pos: [0.0, 0.0] },
    UiVertex { pos: [1.0, 1.0] },
    UiVertex { pos: [0.0, 1.0] },
];

/// UI uniform：屏幕空间偏移 + 缩放 + 颜色。不乘矩阵——offset/scale 直接算。
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct UiUniforms {
    offset: [f32; 2],  // NDC 偏移
    scale: [f32; 2],    // 宽高（NDC 单位）
    color: [f32; 4],    // RGBA
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
    // 单位方块 (0~1) × scale + offset = 屏幕上的位置。没有矩阵乘法。
    let screen = pos * u.scale + u.offset;
    return vec4f(screen, 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4f {
    return u.color;
}
"#;

/// 轨道相机：M1 Step 4 原样搬来。默认高俯视角（俯视角游戏机位）。
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
            pitch: 1.25, // ≈72°，接近正上方俯视——M2 游戏的机位
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
            // 正交范围按场地大小定：切到正交 ≈ 传统俯视角 2D 游戏的观感。
            Mat4::orthographic_rh(-4.6 * aspect, 4.6 * aspect, -4.6, 4.6, 0.1, 100.0)
        }
    }
}

// ---------------------------------------------------------------------------
// 场景搭建：发编号 + 贴标签（对照那张「贴标签」图看）
// ---------------------------------------------------------------------------

fn setup_scene(world: &mut World) {
    // 地板：只有 Position + Visual。RenderSystem 画它，其他系统都不碰——
    // 它就是图里那块「石头」：会出现在屏幕上，但永远不会动。
    let floor = world.spawn();
    world.set_pos(floor, Position([0.0, 0.0]));
    world.set_visual(
        floor,
        Visual { color: [0.15, 0.17, 0.23, 1.0], size: 7.8, height: 0.0, flash: 0.0 },
    );

    // 玩家：绿色。新增 Health（100）+ Attack（半径 0.9，一刀 34 伤害）。
    let player = world.spawn();
    world.set_pos(player, Position([0.0, 0.0]));
    world.set_vel(player, Velocity([0.0, 0.0]));
    world.set_controlled(player);
    world.set_health(player, Health { hp: 100.0, max: 100.0, invuln: 0.0 });
    world.set_attack(player, Attack { cooldown: 0.0, radius: 0.9, damage: 34.0 });
    world.set_visual(
        player,
        Visual { color: [0.30, 0.85, 0.40, 1.0], size: 0.5, height: 0.06, flash: 0.0 },
    );

    // 追踪怪 ×2：红色。血 100 = 挨三刀（34×3）；速度 1.1 < 玩家 2.2，遛得动。
    let chaser_pos = [(-2.8, -2.8), (2.8, 2.8)];
    for (x, z) in chaser_pos {
        let c = world.spawn();
        world.set_pos(c, Position([x, z]));
        world.set_vel(c, Velocity([0.0, 0.0]));
        world.set_chasing(c, Chasing { speed: 1.1 });
        world.set_health(c, Health { hp: 100.0, max: 100.0, invuln: 0.0 });
        world.set_visual(
            c,
            Visual { color: [0.92, 0.30, 0.25, 1.0], size: 0.45, height: 0.04, flash: 0.0 },
        );
    }

    // 漂浮怪 ×6：Position + Velocity + Visual。固定表驱动（不用随机数，行为可复现、可 review）。
    // (位置, 初速度, 颜色, 尺寸)
    let floaters: [([f32; 2], [f32; 2], [f32; 4], f32); 6] = [
        ([-2.5, 1.5], [0.9, 0.5], [0.30, 0.80, 0.90, 1.0], 0.40),
        ([2.5, -1.5], [-0.7, 0.8], [0.85, 0.45, 0.85, 1.0], 0.35),
        ([0.0, 2.6], [0.6, -0.7], [0.90, 0.80, 0.35, 1.0], 0.45),
        ([-1.2, -2.4], [-0.8, -0.6], [0.40, 0.55, 0.95, 1.0], 0.35),
        ([1.8, 2.2], [-0.5, -0.9], [0.95, 0.60, 0.30, 1.0], 0.40),
        ([3.0, 0.0], [-1.0, 0.3], [0.35, 0.85, 0.65, 1.0], 0.35),
    ];
    for (p, v, c, s) in floaters {
        let f = world.spawn();
        world.set_pos(f, Position(p));
        world.set_vel(f, Velocity(v));
        world.set_health(f, Health { hp: 30.0, max: 30.0, invuln: 0.0 }); // 一刀一个
        world.set_visual(f, Visual { color: c, size: s, height: 0.02, flash: 0.0 });
    }
}

// ---------------------------------------------------------------------------
// 应用状态
// ---------------------------------------------------------------------------

struct App {
    // ----- 图形（init 时创建一次，全部沿用 M1 骨架）-----
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    device: Option<wgpu::Device>,
    queue: Option<wgpu::Queue>,
    config: Option<wgpu::SurfaceConfiguration>,
    pipeline: Option<wgpu::RenderPipeline>,
    vertex_buffer: Option<wgpu::Buffer>,
    depth_view: Option<wgpu::TextureView>,
    /// 每个实体一份 uniform + 绑定组（和 World 的池子按下标对齐，None = 该实体不渲染）。
    /// 运行时 spawn/despawn 后由 sync_entity_gpu() 对账——见 [REVIEW 2-3]。
    entity_gpu: Vec<Option<(wgpu::Buffer, wgpu::BindGroup)>>,
    /// 绑定组布局：运行时给新实体建 uniform 时还要用它创建 bind group，所以存下来。
    bind_layout: Option<wgpu::BindGroupLayout>,

    // ----- UI 渲染（Step 3 新增）-----
    ui_pipeline: Option<wgpu::RenderPipeline>,
    ui_vertex_buffer: Option<wgpu::Buffer>,
    /// HP 血条用 3 个 uniform buffer：背景框 + 前景条 + 全屏覆盖层。
    /// 每帧 write_buffer 更新（写入队列，下次 submit 生效——和游戏 uniform 同理）。
    ui_bg_buf: Option<wgpu::Buffer>,
    ui_fg_buf: Option<wgpu::Buffer>,
    ui_overlay_buf: Option<wgpu::Buffer>,
    ui_bg_bind: Option<wgpu::BindGroup>,
    ui_fg_bind: Option<wgpu::BindGroup>,
    ui_overlay_bind: Option<wgpu::BindGroup>,
    /// 游戏状态机
    state: GameState,

    // ----- 游戏状态 -----
    world: World,
    camera: OrbitCamera,
    keys: HashSet<KeyCode>,
    last_frame: Instant,
    // 战绩（验收仪表用）
    kills: u32,
    pickups: u32,
    // 测量仪表（验收用）
    frames: u64,
    last_measure: Instant,
}

impl App {
    fn new() -> Self {
        let mut world = World::new();
        setup_scene(&mut world);
        Self {
            window: None,
            surface: None,
            device: None,
            queue: None,
            config: None,
            pipeline: None,
            vertex_buffer: None,
            depth_view: None,
            entity_gpu: Vec::new(),
            bind_layout: None,
            ui_pipeline: None,
            ui_vertex_buffer: None,
            ui_bg_buf: None,
            ui_fg_buf: None,
            ui_overlay_buf: None,
            ui_bg_bind: None,
            ui_fg_bind: None,
            ui_overlay_bind: None,
            state: GameState::Playing,
            world,
            camera: OrbitCamera::new(),
            keys: HashSet::new(),
            last_frame: Instant::now(),
            kills: 0,
            pickups: 0,
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
            label: Some("m2 device"),
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
            label: Some("m2 shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        // 一份几何，所有实体共用。
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad vertices"),
            contents: bytemuck::cast_slice(&QUAD_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // 每个有 Visual 的实体一份 uniform + 绑定组（M1 三个立方体模式的推广）。
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("m2 bind group layout"),
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("m2 pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let mut entity_gpu = Vec::new();
        for i in 0..self.world.next {
            if self.world.vis[i].is_some() {
                let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("entity uniform"),
                    contents: bytemuck::bytes_of(&Uniforms {
                        mvp: Mat4::IDENTITY.to_cols_array_2d(),
                        color: [1.0, 0.0, 1.0, 1.0],
                    }),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });
                let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("entity bind group"),
                    layout: &bind_group_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buf.as_entire_binding(),
                    }],
                });
                entity_gpu.push(Some((buf, bg)));
            } else {
                entity_gpu.push(None);
            }
        }

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
            label: Some("m2 pipeline"),
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

        // ----- UI 管线（Step 3）：屏幕空间，无深度，alpha 混合 -----
        // 必须在把 device/config move 进 self.X 之前用完它们——下面的赋值会 move 掉局部变量。
        let ui_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui shader"),
            source: wgpu::ShaderSource::Wgsl(UI_SHADER.into()),
        });
        let ui_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ui quad vertices"),
            contents: bytemuck::cast_slice(&UI_QUAD),
            usage: wgpu::BufferUsages::VERTEX,
        });
        // UI 用自己的 bind group layout（和游戏管线共享同一种结构：一个 uniform buffer）。
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
        // 三个 uniform buffer（背景框 / 前景条 / 覆盖层），各自一份 bind group。
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

        // [REVIEW 3-2] UI 管线：无深度测试（UI 永远在最上层），用 alpha 混合（半透明覆盖层）。
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
            // UI 与深度无关，但 wgpu 要求同一 render pass 内所有 pipeline 的
            // depth-stencil 格式与 pass 的 attachment 一致（pass 里有 Depth32Float
            // attachment，因为游戏管线要用）。这里给一个「Always 比较 + 不写深度」的
            // 占位 state：格式匹配 pass，行为上等价于「不测深度、不写深度」——
            // UI 永远画在最上层，与游戏画了什么无关。
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
        self.vertex_buffer = Some(vertex_buffer);
        self.depth_view = Some(depth_view);
        self.entity_gpu = entity_gpu;
        self.bind_layout = Some(bind_group_layout);
        self.ui_pipeline = Some(ui_pipeline);
        self.ui_vertex_buffer = Some(ui_vertex_buffer);
        self.ui_bg_buf = Some(ui_bg_buf);
        self.ui_fg_buf = Some(ui_fg_buf);
        self.ui_overlay_buf = Some(ui_overlay_buf);
        self.ui_bg_bind = Some(ui_bg_bind);
        self.ui_fg_bind = Some(ui_fg_bind);
        self.ui_overlay_bind = Some(ui_overlay_bind);
    }

    // -----------------------------------------------------------------------
    // 每帧更新：系统按固定顺序跑（输入 → 追踪 → 移动 → 弹墙 → 上传 uniform）
    // -----------------------------------------------------------------------

    fn update(&mut self) {
        let delta = self.last_frame.elapsed().as_secs_f32().min(0.1);
        self.last_frame = Instant::now();

        // ----- [REVIEW 3-1] 状态机：只在 Playing 时跑游戏系统 -----
        // Paused：冻结所有游戏系统（但相机和渲染照常——你能转视角看冻结的画面）。
        // GameOver：同上，等 R 重开。

        // ----- 相机（方向键，沿用 M1）-----
        // 相机不论状态都更新：Paused/GameOver 时玩家还能转视角看冻结的画面。
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

        // ----- 系统流水线：顺序 = 数据的因果链 -----
        // 输入/追踪生产 Velocity → 移动消费 → 弹墙修正 →
        // 战斗结算（挥砍/贴脸扣血 → 清尸+掉落 → 拾取回血 → 特效衰减）→ GPU 对账。
        // [REVIEW 3-1] 状态机开关面板：只在 Playing 时跑这套——Paused/GameOver 跳过。
        if self.state == GameState::Playing {
            input_system(&mut self.world, &self.keys);
            chase_system(&mut self.world);
            move_system(&mut self.world, delta);
            bounce_system(&mut self.world);
            combat_system(&mut self.world, &self.keys, delta);
            contact_system(&mut self.world, delta);
            death_system(&mut self.world, &mut self.kills);
            pickup_system(&mut self.world, delta, &mut self.pickups);
            flash_system(&mut self.world, delta);
            self.sync_entity_gpu();
        }

        // ----- 死亡检测 → 切到 GameOver 状态 -----
        let alive = (0..self.world.next).any(|i| self.world.ctl[i].is_some());
        if !alive && self.state == GameState::Playing {
            self.state = GameState::GameOver;
            if let Some(w) = &self.window {
                w.set_title("GAME OVER — press R to restart");
            }
        }

        // ----- 验收仪表：每 0.5 秒打印一行 -----
        self.frames += 1;
        let m_elapsed = self.last_measure.elapsed().as_secs_f32();
        if m_elapsed >= 0.5 {
            let (hp, hp_max) = (0..self.world.next)
                .find(|&i| self.world.ctl[i].is_some() && self.world.health[i].is_some())
                .and_then(|i| self.world.health[i])
                .map(|h| (h.hp, h.max))
                .unwrap_or((0.0, 100.0));
            let drops = (0..self.world.next)
                .filter(|&i| self.world.pickup[i].is_some())
                .count();
            println!(
                "[measure] fps≈{:.0}  hp={:.0}/{:.0}  kills={}  pickups={}  drops_on_field={}  draw_calls={}  state={:?}",
                self.frames as f32 / m_elapsed,
                hp,
                hp_max,
                self.kills,
                self.pickups,
                drops,
                self.entity_gpu.iter().filter(|g| g.is_some()).count(),
                self.state,
            );
            self.frames = 0;
            self.last_measure = Instant::now();
        }

        // ----- 上传 uniform：每个可见实体一份 MVP + 颜色 -----
        let aspect = self
            .config
            .as_ref()
            .map(|c| c.width as f32 / c.height as f32)
            .unwrap_or(1.0);
        let view = self.camera.view();
        let proj = self.camera.projection(aspect);
        let queue = self.queue.as_ref().unwrap();
        for i in 0..self.world.next {
            if let (Some(p), Some(v), Some(Some((buf, _)))) =
                (self.world.pos[i], self.world.vis[i], self.entity_gpu.get(i))
            {
                // M = 搬到 (x, height, z) + 躺平（XY 面 → XZ 面）+ 缩放到自己的尺寸。
                let model = Mat4::from_translation(Vec3::new(p.0[0], v.height, p.0[1]))
                    * Mat4::from_rotation_x(-FRAC_PI_2)
                    * Mat4::from_scale(Vec3::splat(v.size));
                // MVP 顺序（M1 Step 4 的核心考点，原样保留）。
                let mvp = proj * view * model;
                // 受击白闪：颜色向白色插值（flash 由 flash_system 每帧衰减）。
                let f = v.flash.clamp(0.0, 1.0);
                let color = [
                    v.color[0] + (1.0 - v.color[0]) * f,
                    v.color[1] + (1.0 - v.color[1]) * f,
                    v.color[2] + (1.0 - v.color[2]) * f,
                    v.color[3],
                ];
                queue.write_buffer(
                    buf,
                    0,
                    bytemuck::bytes_of(&Uniforms {
                        mvp: mvp.to_cols_array_2d(),
                        color,
                    }),
                );
            }
        }
    }

    /// [REVIEW 2-3] 对账：ECS 世界（逻辑）和 GPU 资源（渲染）是两本账。
    /// 运行时 spawn（掉落物凭空出现）→ 补建 uniform；
    /// despawn（怪被清掉）→ 释放 buffer（槽位置 None，旧 buffer 被 drop）。
    /// 工业引擎同样有这道同步层（bevy 的 extract/render 阶段干的就是这件事）。
    fn sync_entity_gpu(&mut self) {
        let Some(device) = self.device.clone() else { return };
        let Some(layout) = self.bind_layout.clone() else { return };
        // 世界长了先占位（新编号还没有 Visual 时先填 None）
        while self.entity_gpu.len() < self.world.next {
            self.entity_gpu.push(None);
        }
        for i in 0..self.world.next {
            match (self.world.vis[i].is_some(), self.entity_gpu[i].is_some()) {
                (true, false) => {
                    // 新实体需要渲染：建 uniform buffer + 绑定组
                    let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("entity uniform"),
                        contents: bytemuck::bytes_of(&Uniforms {
                            mvp: Mat4::IDENTITY.to_cols_array_2d(),
                            color: [1.0, 0.0, 1.0, 1.0],
                        }),
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    });
                    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("entity bind group"),
                        layout: &layout,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: buf.as_entire_binding(),
                        }],
                    });
                    self.entity_gpu[i] = Some((buf, bg));
                }
                (false, true) => {
                    // 实体不再渲染：槽位置 None，旧 (Buffer, BindGroup) 被 drop → GPU 释放
                    self.entity_gpu[i] = None;
                }
                _ => {}
            }
        }
    }

    /// R 键：整个世界推倒重建（新 World + 重发 GPU 资源 + 清战绩）。
    fn reset_scene(&mut self) {
        self.world = World::new();
        setup_scene(&mut self.world);
        self.entity_gpu.clear();
        self.sync_entity_gpu();
        self.kills = 0;
        self.pickups = 0;
        self.state = GameState::Playing;
        self.last_frame = Instant::now();
        if let Some(w) = &self.window {
            w.set_title("M2 Step 3: UI + state machine (P pause / O projection / R restart)");
        }
    }

    // 渲染循环：每个实体一次 set_bind_group + 一次 draw（Step 1 REVIEW 3：draw call 不是免费的）。
    fn render(&mut self) {
        let surface = self.surface.as_ref().unwrap();
        let device = self.device.as_ref().unwrap();
        let queue = self.queue.as_ref().unwrap();
        let config = self.config.as_ref().unwrap();
        let pipeline = self.pipeline.as_ref().unwrap();
        let vertex_buffer = self.vertex_buffer.as_ref().unwrap();
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

        // ----- [REVIEW 3-3] UI uniform 写入：在开 pass 前写入队列 -----
        // write_buffer 是 queue 操作（不依赖 pass / encoder），但必须在 submit 前入队。
        // 这里在创建 encoder 之前先把 UI uniform 推进队列，pass 里只做 draw。
        let (hp, hp_max) = (0..self.world.next)
            .find(|&i| self.world.ctl[i].is_some() && self.world.health[i].is_some())
            .and_then(|i| self.world.health[i])
            .map(|h| (h.hp, h.max))
            .unwrap_or((0.0, 100.0));
        let hp_ratio = (hp / hp_max).clamp(0.0, 1.0);

        // HP 条参数（NDC 坐标：-1=左/下，+1=右/上）
        let bar_x = -0.95;      // 左边距
        let bar_y = 0.88;       // 靠上
        let bar_w = 0.5;        // 总宽
        let bar_h = 0.04;       // 高

        // 背景框（暗灰）
        queue.write_buffer(
            self.ui_bg_buf.as_ref().unwrap(), 0,
            bytemuck::bytes_of(&UiUniforms {
                offset: [bar_x, bar_y], scale: [bar_w, bar_h],
                color: [0.15, 0.15, 0.18, 0.8],
            }),
        );
        // 前景条（绿→红渐变，按 HP 比例缩放宽度）
        let hp_color = if hp_ratio > 0.5 {
            [0.2, 0.8, 0.3, 1.0]       // 绿
        } else if hp_ratio > 0.25 {
            [0.9, 0.7, 0.2, 1.0]       // 黄
        } else {
            [0.9, 0.25, 0.2, 1.0]      // 红
        };
        queue.write_buffer(
            self.ui_fg_buf.as_ref().unwrap(), 0,
            bytemuck::bytes_of(&UiUniforms {
                offset: [bar_x, bar_y], scale: [bar_w * hp_ratio, bar_h],
                color: hp_color,
            }),
        );

        // 状态覆盖层（暂停=半透明蓝，死亡=半透明红，游戏中=透明不画）
        if self.state != GameState::Playing {
            let overlay_color = match self.state {
                GameState::Paused => [0.02, 0.03, 0.12, 0.6],
                GameState::GameOver => [0.3, 0.02, 0.04, 0.5],
                GameState::Playing => [0.0, 0.0, 0.0, 0.0],
            };
            queue.write_buffer(
                self.ui_overlay_buf.as_ref().unwrap(), 0,
                bytemuck::bytes_of(&UiUniforms {
                    offset: [-1.0, -1.0], scale: [2.0, 2.0], // 全屏
                    color: overlay_color,
                }),
            );
        }

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("m2 pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // 死亡时画面转暗红——「你死了」必须用颜色喊出来，不是用 println。
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
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            // 同一份方块几何，画 N 次：换一次绑定组 = 换一份 MVP + 颜色 = 一个实体。
            // 代价问题见 [REVIEW 3] 注释。
            for g in &self.entity_gpu {
                if let Some((_, bg)) = g {
                    pass.set_bind_group(0, bg, &[]);
                    pass.draw(0..6, 0..1);
                }
            }

            // ----- [REVIEW 3-3] 渲染顺序：先画游戏，再画 UI（UI 在最上层）-----
            // 同一个 render pass 里切换 pipeline：游戏管线 → UI 管线。
            // UI 管线用 depth_compare=Always + 不写深度，所以不管游戏画了什么，
            // UI 永远覆盖在上面——这就是 HUD、血条、菜单的实现原理。
            let ui_pipeline = self.ui_pipeline.as_ref().unwrap();
            let ui_vbuf = self.ui_vertex_buffer.as_ref().unwrap();

            // 切到 UI 管线，画 3 个元素
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
                        .with_title("M2 Step 3: UI + state machine (P pause / O projection / R restart)")
                        .with_inner_size(winit::dpi::LogicalSize::new(900.0, 600.0)),
                )
                .expect("create window"),
        );
        window.request_redraw();
        self.window = Some(window);
        self.init_graphics();
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
                            } else if code == KeyCode::KeyR && !event.repeat {
                                self.camera = OrbitCamera::new();
                                self.reset_scene();
                                self.state = GameState::Playing;
                                if let Some(w) = &self.window {
                                    w.set_title("M2 Step 3: UI + state machine (P pause / O projection / R restart)");
                                }
                                println!("scene reset: player hp=100, kills=0");
                            } else if code == KeyCode::KeyP && !event.repeat {
                                // P = 暂停/恢复
                                self.state = match self.state {
                                    GameState::Playing => {
                                        if let Some(w) = &self.window {
                                            w.set_title("PAUSED — press P to resume");
                                        }
                                        GameState::Paused
                                    }
                                    GameState::Paused => {
                                        if let Some(w) = &self.window {
                                            w.set_title("M2 Step 3: UI + state machine (P pause / O projection / R restart)");
                                        }
                                        GameState::Playing
                                    }
                                    GameState::GameOver => GameState::GameOver, // 死了不能暂停
                                };
                            } else if code == KeyCode::KeyO && !event.repeat {
                                // O = 切透视/正交（原来 P 的功能挪到 O）
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
