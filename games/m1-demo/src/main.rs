//! M1 第四步（终章）：3D 场景 + 轨道相机 + MVP
//!
//! 目标：三个彩色立方体 + 可环绕的相机，理解 MVP 三段旅程。
//! 新知识：视图/投影矩阵、uniform 传矩阵、深度测试真正上岗。
//!
//! 操作：←→ 环绕 | ↑↓ 俯仰 | +/- 缩放 | R 重置 | Esc 退出
//! Review 标记：搜 `[REVIEW` 可找到 3 个关键审查点。

use std::collections::HashSet;
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
// 数据定义
// ---------------------------------------------------------------------------

/// 顶点：3D 位置 + 颜色。Step 3 的 2D 坐标升级为 3D（多了 z 轴）。
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

impl Vertex {
    /// 顶点布局：3D 坐标（12 字节）+ 颜色（12 字节）= 24 字节一个顶点。
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
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

/// 单位立方体：24 个顶点（6 面 × 4 顶点，每面一种颜色）+ 36 个索引。
/// 三份不同颜色让立方体在没有光照的情况下也能看出立体感。
fn cube_vertices() -> Vec<Vertex> {
    // 每面：(法线方向, 颜色)。面上 4 个角点由 face_corners 算出。
    let faces: [(&str, [f32; 3], [f32; 3]); 6] = [
        ("+X 红", [1.0, 0.0, 0.0], [0.9, 0.35, 0.3]),
        ("-X 青", [-1.0, 0.0, 0.0], [0.3, 0.8, 0.85]),
        ("+Y 黄", [0.0, 1.0, 0.0], [0.95, 0.85, 0.3]),
        ("-Y 紫", [0.0, -1.0, 0.0], [0.6, 0.45, 0.85]),
        ("+Z 绿", [0.0, 0.0, 1.0], [0.35, 0.8, 0.4]),
        ("-Z 蓝", [0.0, 0.0, -1.0], [0.35, 0.5, 0.9]),
    ];
    let mut verts = Vec::with_capacity(24);
    for (_, n, c) in faces {
        // 面的法线是 n；两个切向 u/v 张成这个面。
        let n = Vec3::from_array(n);
        let u = if n.x != 0.0 {
            Vec3::Y
        } else if n.y != 0.0 {
            Vec3::X
        } else {
            Vec3::X
        };
        let v = n.cross(u).normalize();
        let u = v.cross(n).normalize();
        // 四个角：中心 ± u ± v（法线方向的半边长 0.5）
        for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let p = n * 0.5 + u * (su * 0.5) + v * (sv * 0.5);
            verts.push(Vertex {
                position: p.to_array(),
                color: c,
            });
        }
    }
    verts
}

/// 36 个索引：每面 2 个三角形（4 顶点 → 0,1,2 / 0,2,3），顶点不重复上传。
const CUBE_INDICES: &[u16] = &{
    let mut idx = [0u16; 36];
    let mut f = 0;
    while f < 6 {
        let b = (f * 4) as u16;
        idx[f * 6] = b;
        idx[f * 6 + 1] = b + 1;
        idx[f * 6 + 2] = b + 2;
        idx[f * 6 + 3] = b;
        idx[f * 6 + 4] = b + 2;
        idx[f * 6 + 5] = b + 3;
        f += 1;
    }
    idx
};

/// 每帧从 CPU 传给 GPU 的 MVP 矩阵（每个立方体一份）。
/// 16 字节对齐天然满足（mat4 = 64 字节）。
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    mvp: [[f32; 4]; 4],
}

/// WGSL shader：Step 4 的主角登场——顶点着色器从「直通管道」变成「乘 MVP 的干将」。
const SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec3f,
    @location(1) color: vec3f,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) color: vec3f, // 顶点色传给片元着色器（GPU 自动插值）
};

struct Uniforms {
    mvp: mat4x4f,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    // MVP 三段旅程一次走完：本地坐标 → 屏幕坐标。
    // Step 3 的 `position + u.offset` 升级成了 `u.mvp * vec4f(position, 1.0)`。
    out.clip_position = u.mvp * vec4f(in.position, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // 顶点色 + 完全不透明（alpha = 1）。
    return vec4f(in.color, 1.0);
}
"#;

// ---------------------------------------------------------------------------
// [REVIEW 1] 轨道相机：球坐标 → 眼睛位置 → 视图矩阵
// ---------------------------------------------------------------------------

/// 轨道相机：绕着目标点转圈的「摄影师」。
///
/// 三个自由度正好对应三个键：
/// - yaw（左右环绕）：绕目标点水平转
/// - pitch（俯仰）：从上往下看 / 平视
/// - distance（缩放）：镜头推近拉远
struct OrbitCamera {
    target: Vec3,
    yaw: f32,      // 水平角（弧度）
    pitch: f32,    // 俯仰角（弧度，有上下限防止翻转过头）
    distance: f32, // 离目标点多远
    /// [实验 3] 按 P 切换：透视（近大远小）↔ 正交（工程图纸，远近同大）。
    perspective: bool,
}

impl OrbitCamera {
    fn new() -> Self {
        Self {
            target: Vec3::new(0.0, 0.5, 0.0),
            yaw: 45f32.to_radians(),
            pitch: 20f32.to_radians(),
            distance: 4.5,
            perspective: true,
        }
    }

    /// 眼睛位置：球坐标公式。
    /// 想象从目标点伸出长度 distance 的杆子，yaw 定水平朝向，pitch 定抬起角度——
    /// 杆子的另一头就是相机的眼睛。
    fn eye(&self) -> Vec3 {
        let cos_pitch = self.pitch.cos();
        self.target
            + Vec3::new(
                self.distance * cos_pitch * self.yaw.sin(),
                self.distance * self.pitch.sin(),
                self.distance * cos_pitch * self.yaw.cos(),
            )
    }

    /// 视图矩阵（V）：「世界 → 相机眼前」的搬运工。
    /// look_at(eye, target, up) = 摄影师站在 eye、镜头对准 target、头顶朝 up。
    fn view(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye(), self.target, Vec3::Y)
    }

    /// 投影矩阵（P）：「相机眼前 → 屏幕」的镜头。透视 = 近大远小。
    /// - fov：视角（越大看得越广，但边缘变形越大——手机广角镜头）
    /// - aspect：宽高比（窗口不是方的，画面也不能是方的）
    /// - near/far：太近的（贴着镜头）和太远的（天边）都裁掉，节省精度
    fn projection(&self, aspect: f32) -> Mat4 {
        if self.perspective {
            // 透视投影：成像时除以距离 → 近大远小（照片/人眼）。
            Mat4::perspective_rh(60f32.to_radians(), aspect, 0.1, 100.0)
        } else {
            // 正交投影：不除距离 → 远近同大（工程图纸/俯视角游戏）。
            Mat4::orthographic_rh(-2.0 * aspect, 2.0 * aspect, -2.0, 2.0, 0.1, 100.0)
        }
    }
}

// ---------------------------------------------------------------------------
// 场景物体
// ---------------------------------------------------------------------------

/// 一个立方体：在哪里、什么姿态（中间那个会自转）。
struct Cube {
    model: Mat4,       // M 矩阵：本地 → 世界
    spin: bool,        // 是否自转
    spin_angle: f32,   // 当前自转角
    spin_axis: Vec3,   // 自转轴
    label: &'static str,
}

fn scene_cubes() -> Vec<Cube> {
    vec![
        // 左：远处（z 负方向 = 离初始相机远）。实验 3：与右边同尺寸，隔离变量。
        Cube {
            model: Mat4::from_translation(Vec3::new(-1.6, 0.5, -0.8)) * Mat4::from_scale(Vec3::splat(0.8)),
            spin: false,
            spin_angle: 0.0,
            spin_axis: Vec3::Y,
            label: "left-far",
        },
        // 中：原点，自转的主角
        Cube {
            model: Mat4::from_translation(Vec3::new(0.0, 0.5, 0.0)),
            spin: true,
            spin_angle: 0.0,
            spin_axis: Vec3::new(0.3, 1.0, 0.2).normalize(),
            label: "center-spin",
        },
        // 右：近处（z 正方向 = 离初始相机近）。实验 3：与左边同尺寸——
        // 透视下右应明显更大（近大远小），正交下左右应完全一样大。
        Cube {
            model: Mat4::from_translation(Vec3::new(1.6, 0.5, 0.8)) * Mat4::from_scale(Vec3::splat(0.8)),
            spin: false,
            spin_angle: 0.0,
            spin_axis: Vec3::Y,
            label: "right-near",
        },
    ]
}

// ---------------------------------------------------------------------------
// 应用状态
// ---------------------------------------------------------------------------

struct App {
    // ----- 图形（init 时创建一次）-----
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    device: Option<wgpu::Device>,
    queue: Option<wgpu::Queue>,
    config: Option<wgpu::SurfaceConfiguration>,
    pipeline: Option<wgpu::RenderPipeline>,
    vertex_buffer: Option<wgpu::Buffer>,
    index_buffer: Option<wgpu::Buffer>,
    depth_view: Option<wgpu::TextureView>,
    /// 每个立方体一份：uniform 缓冲 + 绑定组（MVP 各自独立）。
    cube_uniforms: Vec<(wgpu::Buffer, wgpu::BindGroup)>,

    // ----- 游戏状态 -----
    cubes: Vec<Cube>,
    camera: OrbitCamera,
    keys: HashSet<KeyCode>,
    last_frame: Instant,
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
            vertex_buffer: None,
            index_buffer: None,
            depth_view: None,
            cube_uniforms: Vec::new(),
            cubes: scene_cubes(),
            camera: OrbitCamera::new(),
            keys: HashSet::new(),
            last_frame: Instant::now(),
        }
    }

    fn init_graphics(&mut self) {
        let window = self.window.as_ref().expect("window exists").clone();

        // 1-5. 五件套（与 Step 1-3 完全相同，不再逐行注释）。
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
            label: Some("m1 device"),
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

        // 6. Shader 模块。
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mvp shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        // 7. 立方体几何（一份顶点缓冲，三个立方体共用——位置差异全在 M 矩阵里）。
        let vertices = cube_vertices();
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube indices"),
            contents: bytemuck::cast_slice(CUBE_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        // 8. 每个立方体一份 uniform + 绑定组（布局共享一个）。
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mvp bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mvp pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let mut cube_uniforms = Vec::new();
        for cube in &self.cubes {
            let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(cube.label),
                contents: bytemuck::bytes_of(&Uniforms {
                    mvp: Mat4::IDENTITY.to_cols_array_2d(),
                }),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(cube.label),
                layout: &bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buf.as_entire_binding(),
                }],
            });
            cube_uniforms.push((buf, bg));
        }

        // 9. 深度纹理（Step 3 埋的伏笔：3D 物体互相遮挡，深度说了算）。
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

        // 10. 渲染管线（开启深度测试）。
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mvp pipeline"),
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
            // [REVIEW 3] 深度测试的裁判规则：
            // - depth_compare: Less —— 新像素的深度值要比已记录的小（更近）才画
            // - depth_write_enabled: true —— 画了就记下自己的深度
            // 没有这两行，3D 物体会变成「按画的方向叠罗汉」，遮挡关系全乱。
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

        self.surface = Some(surface);
        self.device = Some(device);
        self.queue = Some(queue);
        self.config = Some(config);
        self.pipeline = Some(pipeline);
        self.vertex_buffer = Some(vertex_buffer);
        self.index_buffer = Some(index_buffer);
        self.depth_view = Some(depth_view);
        self.cube_uniforms = cube_uniforms;
    }

    // -----------------------------------------------------------------------
    // [REVIEW 2] 每帧更新：相机操控 + 自转 + MVP 合成
    // -----------------------------------------------------------------------

    fn update(&mut self) {
        let delta = self
            .last_frame
            .elapsed()
            .as_secs_f32()
            .min(0.1);
        self.last_frame = Instant::now();

        // ----- 相机操控（按住持续转，延续 Step 3 的按键集合思路）-----
        let has = |keys: &HashSet<KeyCode>, c: KeyCode| keys.contains(&c);
        let rot_speed = 1.5; // 弧度/秒
        if has(&self.keys, KeyCode::ArrowLeft) {
            self.camera.yaw += rot_speed * delta;
        }
        if has(&self.keys, KeyCode::ArrowRight) {
            self.camera.yaw -= rot_speed * delta;
        }
        if has(&self.keys, KeyCode::ArrowUp) {
            self.camera.pitch += rot_speed * 0.6 * delta;
        }
        if has(&self.keys, KeyCode::ArrowDown) {
            self.camera.pitch -= rot_speed * 0.6 * delta;
        }
        self.camera.pitch = self.camera.pitch.clamp(0.05, 1.45); // 不许钻到地下/翻到天顶
        if has(&self.keys, KeyCode::Equal) || has(&self.keys, KeyCode::NumpadAdd) {
            self.camera.distance -= 2.0 * delta;
        }
        if has(&self.keys, KeyCode::Minus) || has(&self.keys, KeyCode::NumpadSubtract) {
            self.camera.distance += 2.0 * delta;
        }
        self.camera.distance = self.camera.distance.clamp(1.5, 15.0);

        // ----- 中间立方体自转（M 矩阵每帧变化）-----
        for cube in &mut self.cubes {
            if cube.spin {
                cube.spin_angle += 0.8 * delta;
            }
        }

        // ----- MVP 合成：每个立方体一份完整矩阵 -----
        // [REVIEW 2] 乘法顺序是本步的核心考点：
        //   mvp = proj * view * model
        // 矩阵从右往左作用：顶点先被 M 搬到世界，再被 V 搬到相机眼前，
        // 最后被 P 压到屏幕。顺序写反 = 画面直接废掉（可以实验验证）。
        let aspect = self.config.as_ref().map(|c| c.width as f32 / c.height as f32).unwrap_or(1.0);
        let view = self.camera.view();
        let proj = self.camera.projection(aspect);

        let queue = self.queue.as_ref().unwrap();
        for (i, cube) in self.cubes.iter().enumerate() {
            // M = 摆位 × 自转（先转再搬：让方块绕自己的轴转，不是绕世界原点转）
            let model = if cube.spin {
                Mat4::from_translation(cube.model.w_axis.truncate())
                    * Mat4::from_axis_angle(cube.spin_axis, cube.spin_angle)
                    * Mat4::from_scale(Vec3::splat(0.8))
            } else {
                cube.model
            };
            // [REVIEW 2] 顺序 = 三段旅程的先后（矩阵从右往左作用）：
            // M 先把本地顶点搬进世界，V 再搬到相机眼前，P 最后成像到屏幕。
            // 实验 4 已验证：写成 model * view * proj 时画面直接废掉（立方体消失）。
            let mvp = proj * view * model;
            let (buf, _) = &self.cube_uniforms[i];
            queue.write_buffer(
                buf,
                0,
                bytemuck::bytes_of(&Uniforms {
                    mvp: mvp.to_cols_array_2d(),
                }),
            );
        }
    }

    fn render(&mut self) {
        let surface = self.surface.as_ref().unwrap();
        let device = self.device.as_ref().unwrap();
        let queue = self.queue.as_ref().unwrap();
        let config = self.config.as_ref().unwrap();
        let pipeline = self.pipeline.as_ref().unwrap();
        let vertex_buffer = self.vertex_buffer.as_ref().unwrap();
        let index_buffer = self.index_buffer.as_ref().unwrap();
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
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mvp pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.08,
                            g: 0.09,
                            b: 0.14,
                            a: 1.0,
                        }), // 夜空色
                        store: wgpu::StoreOp::Store,
                    },
                })],
                // 深度每帧从「全部无穷远」(1.0) 开始。
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
            pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            // 三个立方体：换绑定组 = 换 MVP，同一份几何画三次。
            for (_, bg) in &self.cube_uniforms {
                pass.set_bind_group(0, bg, &[]);
                pass.draw_indexed(0..36, 0, 0..1);
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
                        .with_title("M1 Step 4: MVP orbit camera (arrows / +/- / P projection / R / Esc)")
                        .with_inner_size(winit::dpi::LogicalSize::new(900.0, 600.0)),
                )
                .expect("create window"),
        );
        window.request_redraw();
        self.window = Some(window);
        self.init_graphics();
        println!("controls: <left/right> orbit | <up/down> pitch | +/- zoom | P toggle projection | R reset | Esc quit");
        println!("[实验 3] 左右立方体现在同尺寸：透视下右(近)更大，按 P 切正交后左右应一样大");
        println!("watch: right cube (near) looks bigger than left (far) = perspective P");
        println!("watch: center cube spins = model M; occlusion between cubes = depth test");
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
                                println!("camera reset");
                            } else if code == KeyCode::KeyP && !event.repeat {
                                self.camera.perspective = !self.camera.perspective;
                                println!(
                                    "projection = {}",
                                    if self.camera.perspective {
                                        "perspective (近大远小)"
                                    } else {
                                        "orthographic (工程图纸)"
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
