use crate::toolpath_mesh::{ExtrudeInstance, LINE_WIDTH_MM, ToolpathMesh, TravelVertex};
use eframe::egui_wgpu::{self, wgpu, wgpu::util::DeviceExt};
use glam::{Mat4, Vec3};
use std::ops::Range;

pub const DEPTH_BUFFER_BITS: u8 = 32;
pub const MSAA_SAMPLES: u32 = 4;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SHADOW_MAP_SIZE: u32 = 2048;
const TUBE_VERTICES_PER_INSTANCE: u32 = 18;
const PLANE_VERTICES: u32 = 6;
const LINE_VERTICES_PER_INSTANCE: u32 = 2;

pub const MAX_LIGHT_ELEVATION_DEGREES: f32 = 89.0;
pub const MIN_LIGHT_ELEVATION_DEGREES: f32 = 5.0;

#[derive(Clone, Copy, PartialEq)]
pub struct LightAngles {
    pub azimuth_degrees: f32,
    pub elevation_degrees: f32,
}

impl Default for LightAngles {
    fn default() -> Self {
        Self {
            azimuth_degrees: 215.0,
            elevation_degrees: 53.0,
        }
    }
}

impl LightAngles {
    fn direction(self) -> Vec3 {
        let azimuth = self.azimuth_degrees.to_radians();
        let elevation = self.elevation_degrees.to_radians();
        Vec3::new(
            elevation.cos() * azimuth.cos(),
            elevation.cos() * azimuth.sin(),
            elevation.sin(),
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RenderQuality {
    Fast,
    Shaded,
    Shadowed,
}

impl RenderQuality {
    pub const ALL: [RenderQuality; 3] = [Self::Fast, Self::Shaded, Self::Shadowed];

    pub fn short_label(self) -> &'static str {
        match self {
            Self::Fast => "Fast",
            Self::Shaded => "Shaded",
            Self::Shadowed => "Shadows",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Fast => "Flat lines, no lighting. Best for large models.",
            Self::Shaded => "Lit tubes without shadows.",
            Self::Shadowed => "Lit tubes with shadows. Slowest.",
        }
    }
}

const EXTRUDE_COLOR_LOW: [f32; 4] = [0.27, 0.52, 0.80, 1.0];
const EXTRUDE_COLOR_HIGH: [f32; 4] = [0.96, 0.58, 0.28, 1.0];
const TRAVEL_COLOR: [f32; 4] = [0.55, 0.58, 0.65, 0.30];
const PLANE_HALF_EXTENT_FACTOR: f32 = 1.2;
const SHADOW_FRUSTUM_FACTOR: f32 = 1.3;
const PLANE_GAP_MM: f32 = 0.01;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_projection: [[f32; 4]; 4],
    light_view_projection: [[f32; 4]; 4],
    color_low: [f32; 4],
    color_high: [f32; 4],
    travel_color: [f32; 4],
    light_direction: [f32; 4],
    camera_position: [f32; 4],
    params: [f32; 4],
    plane: [f32; 4],
}

struct Scene {
    extrude_instances: Option<wgpu::Buffer>,
    travel_vertices: Option<wgpu::Buffer>,
    center: Vec3,
    radius: f32,
    plane: [f32; 4],
    plane_z: f32,
    layer_height: f32,
    shadow_key: Option<(Range<u32>, LightAngles)>,
}

pub struct ToolpathGpu {
    tube_pipeline: wgpu::RenderPipeline,
    plane_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    fast_pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    main_bind_group: wgpu::BindGroup,
    shadow_bind_group: wgpu::BindGroup,
    shadow_view: wgpu::TextureView,
    scene: Option<Scene>,
}

impl ToolpathGpu {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("toolpath shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("toolpath_shader.wgsl").into()),
        });
        let main_layout = create_main_bind_group_layout(device);
        let shadow_layout = create_shadow_bind_group_layout(device);
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("toolpath uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shadow_view = create_shadow_view(device);
        let sampler = create_shadow_sampler(device);
        let main_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("toolpath main bind group"),
            layout: &main_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let shadow_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("toolpath shadow bind group"),
            layout: &shadow_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        let pipelines = PipelineFactory {
            device,
            shader: &shader,
            target_format,
            main_layout: &main_layout,
            shadow_layout: &shadow_layout,
        };
        Self {
            tube_pipeline: pipelines.tube(),
            plane_pipeline: pipelines.plane(),
            line_pipeline: pipelines.line(),
            fast_pipeline: pipelines.fast(),
            shadow_pipeline: pipelines.shadow(),
            uniforms,
            main_bind_group,
            shadow_bind_group,
            shadow_view,
            scene: None,
        }
    }

    pub fn upload_mesh(&mut self, device: &wgpu::Device, mesh: &ToolpathMesh, min: Vec3, max: Vec3) {
        let create = |label: &str, contents: &[u8], is_empty: bool| {
            (!is_empty).then(|| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents,
                    usage: wgpu::BufferUsages::VERTEX,
                })
            })
        };
        let center = (min + max) * 0.5;
        let radius = ((max - min).length() * 0.5).max(1.0);
        let plane_half = radius * PLANE_HALF_EXTENT_FACTOR;
        self.scene = Some(Scene {
            extrude_instances: create(
                "extrude instances",
                bytemuck::cast_slice(&mesh.extrude.instances),
                mesh.extrude.instances.is_empty(),
            ),
            travel_vertices: create(
                "travel vertices",
                bytemuck::cast_slice(&mesh.travel.vertices),
                mesh.travel.vertices.is_empty(),
            ),
            center,
            radius,
            plane: [
                center.x - plane_half,
                center.y - plane_half,
                center.x + plane_half,
                center.y + plane_half,
            ],
            plane_z: min.z - mesh.layer_height - PLANE_GAP_MM,
            layer_height: mesh.layer_height,
            shadow_key: None,
        });
    }
}

fn light_view_projection(light: Vec3, center: Vec3, radius: f32) -> Mat4 {
    let eye = center + light * radius * 2.0;
    let view = Mat4::look_at_rh(eye, center, Vec3::Z);
    let half = radius * SHADOW_FRUSTUM_FACTOR;
    let projection = Mat4::orthographic_rh(-half, half, -half, half, radius * 0.5, radius * 3.5);
    projection * view
}

fn create_main_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("toolpath main bind group layout"),
        entries: &[
            uniform_layout_entry(wgpu::ShaderStages::VERTEX_FRAGMENT),
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
        ],
    })
}

fn create_shadow_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("toolpath shadow bind group layout"),
        entries: &[uniform_layout_entry(wgpu::ShaderStages::VERTEX)],
    })
}

fn uniform_layout_entry(visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn create_shadow_view(device: &wgpu::Device) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("toolpath shadow map"),
            size: wgpu::Extent3d {
                width: SHADOW_MAP_SIZE,
                height: SHADOW_MAP_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

fn create_shadow_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("toolpath shadow sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        compare: Some(wgpu::CompareFunction::LessEqual),
        ..Default::default()
    })
}

struct PipelineFactory<'a> {
    device: &'a wgpu::Device,
    shader: &'a wgpu::ShaderModule,
    target_format: wgpu::TextureFormat,
    main_layout: &'a wgpu::BindGroupLayout,
    shadow_layout: &'a wgpu::BindGroupLayout,
}

const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32];
const TRAVEL_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32];

impl PipelineFactory<'_> {
    fn pipeline_layout(&self, bind_group_layout: &wgpu::BindGroupLayout) -> wgpu::PipelineLayout {
        self.device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("toolpath pipeline layout"),
                bind_group_layouts: &[Some(bind_group_layout)],
                immediate_size: 0,
            })
    }

    fn instance_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ExtrudeInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &INSTANCE_ATTRIBUTES,
        }
    }

    fn depth_state(write: bool, bias: wgpu::DepthBiasState) -> wgpu::DepthStencilState {
        wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(write),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias,
        }
    }

    fn color_target(&self, blend: wgpu::BlendState) -> Option<wgpu::ColorTargetState> {
        Some(wgpu::ColorTargetState {
            format: self.target_format,
            blend: Some(blend),
            write_mask: wgpu::ColorWrites::ALL,
        })
    }

    fn main_pipeline(
        &self,
        label: &str,
        vertex_entry: &str,
        fragment_entry: &str,
        buffers: &[Option<wgpu::VertexBufferLayout>],
        topology: wgpu::PrimitiveTopology,
        depth_write: bool,
        blend: wgpu::BlendState,
    ) -> wgpu::RenderPipeline {
        let layout = self.pipeline_layout(self.main_layout);
        self.device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: self.shader,
                    entry_point: Some(vertex_entry),
                    compilation_options: Default::default(),
                    buffers,
                },
                fragment: Some(wgpu::FragmentState {
                    module: self.shader,
                    entry_point: Some(fragment_entry),
                    compilation_options: Default::default(),
                    targets: &[self.color_target(blend)],
                }),
                primitive: wgpu::PrimitiveState {
                    topology,
                    ..Default::default()
                },
                depth_stencil: Some(Self::depth_state(depth_write, Default::default())),
                multisample: wgpu::MultisampleState {
                    count: MSAA_SAMPLES,
                    ..Default::default()
                },
                multiview_mask: None,
                cache: None,
            })
    }

    fn tube(&self) -> wgpu::RenderPipeline {
        self.main_pipeline(
            "toolpath tube pipeline",
            "vs_tube",
            "fs_tube",
            &[Some(Self::instance_buffer_layout())],
            wgpu::PrimitiveTopology::TriangleList,
            true,
            wgpu::BlendState::REPLACE,
        )
    }

    fn plane(&self) -> wgpu::RenderPipeline {
        self.main_pipeline(
            "toolpath plane pipeline",
            "vs_plane",
            "fs_plane",
            &[],
            wgpu::PrimitiveTopology::TriangleList,
            true,
            wgpu::BlendState::ALPHA_BLENDING,
        )
    }

    fn line(&self) -> wgpu::RenderPipeline {
        self.main_pipeline(
            "toolpath travel pipeline",
            "vs_line",
            "fs_line",
            &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<TravelVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &TRAVEL_ATTRIBUTES,
            })],
            wgpu::PrimitiveTopology::LineList,
            false,
            wgpu::BlendState::ALPHA_BLENDING,
        )
    }

    fn fast(&self) -> wgpu::RenderPipeline {
        self.main_pipeline(
            "toolpath fast pipeline",
            "vs_extrude_line",
            "fs_extrude_line",
            &[Some(Self::instance_buffer_layout())],
            wgpu::PrimitiveTopology::LineList,
            true,
            wgpu::BlendState::REPLACE,
        )
    }

    fn shadow(&self) -> wgpu::RenderPipeline {
        let layout = self.pipeline_layout(self.shadow_layout);
        let bias = wgpu::DepthBiasState {
            constant: 2,
            slope_scale: 2.0,
            clamp: 0.0,
        };
        self.device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("toolpath shadow pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: self.shader,
                    entry_point: Some("vs_shadow"),
                    compilation_options: Default::default(),
                    buffers: &[Some(Self::instance_buffer_layout())],
                },
                fragment: None,
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(Self::depth_state(true, bias)),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
    }
}

pub struct ToolpathDraw {
    pub view_projection: Mat4,
    pub camera_position: Vec3,
    pub quality: RenderQuality,
    pub light: LightAngles,
    pub extrude_range: Range<u32>,
    pub travel_range: Option<Range<u32>>,
}

impl ToolpathDraw {
    fn uniforms(&self, scene: &Scene) -> Uniforms {
        let light_direction = self.light.direction();
        Uniforms {
            view_projection: self.view_projection.to_cols_array_2d(),
            light_view_projection: light_view_projection(light_direction, scene.center, scene.radius)
                .to_cols_array_2d(),
            color_low: EXTRUDE_COLOR_LOW,
            color_high: EXTRUDE_COLOR_HIGH,
            travel_color: TRAVEL_COLOR,
            light_direction: light_direction
                .extend(f32::from(self.quality == RenderQuality::Shadowed))
                .to_array(),
            camera_position: self.camera_position.extend(1.0).to_array(),
            params: [
                LINE_WIDTH_MM,
                scene.layer_height,
                scene.plane_z,
                1.0 / SHADOW_MAP_SIZE as f32,
            ],
            plane: scene.plane,
        }
    }

    fn render_shadow_pass(
        &self,
        gpu: &ToolpathGpu,
        scene: &Scene,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("toolpath shadow pass"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &gpu.shadow_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let Some(instances) = &scene.extrude_instances else {
            return;
        };
        pass.set_pipeline(&gpu.shadow_pipeline);
        pass.set_bind_group(0, &gpu.shadow_bind_group, &[]);
        pass.set_vertex_buffer(0, instances.slice(..));
        pass.draw(0..TUBE_VERTICES_PER_INSTANCE, self.extrude_range.clone());
    }
}

impl egui_wgpu::CallbackTrait for ToolpathDraw {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(gpu) = callback_resources.get_mut::<ToolpathGpu>() else {
            return Vec::new();
        };
        let Some(mut scene) = gpu.scene.take() else {
            return Vec::new();
        };
        queue.write_buffer(&gpu.uniforms, 0, bytemuck::bytes_of(&self.uniforms(&scene)));
        let needs_shadow_pass = self.quality == RenderQuality::Shadowed
            && scene.shadow_key.as_ref() != Some(&(self.extrude_range.clone(), self.light));
        if needs_shadow_pass {
            self.render_shadow_pass(gpu, &scene, egui_encoder);
            scene.shadow_key = Some((self.extrude_range.clone(), self.light));
        }
        gpu.scene = Some(scene);
        Vec::new()
    }

    fn paint(
        &self,
        _info: eframe::egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(gpu) = callback_resources.get::<ToolpathGpu>() else {
            return;
        };
        let Some(scene) = &gpu.scene else { return };
        render_pass.set_bind_group(0, &gpu.main_bind_group, &[]);
        if self.quality != RenderQuality::Fast {
            render_pass.set_pipeline(&gpu.plane_pipeline);
            render_pass.draw(0..PLANE_VERTICES, 0..1);
        }
        if let Some(instances) = &scene.extrude_instances {
            let (pipeline, vertices) = match self.quality {
                RenderQuality::Fast => (&gpu.fast_pipeline, LINE_VERTICES_PER_INSTANCE),
                _ => (&gpu.tube_pipeline, TUBE_VERTICES_PER_INSTANCE),
            };
            render_pass.set_pipeline(pipeline);
            render_pass.set_vertex_buffer(0, instances.slice(..));
            render_pass.draw(0..vertices, self.extrude_range.clone());
        }
        if let (Some(vertices), Some(range)) = (&scene.travel_vertices, &self.travel_range) {
            render_pass.set_pipeline(&gpu.line_pipeline);
            render_pass.set_vertex_buffer(0, vertices.slice(..));
            render_pass.draw(range.clone(), 0..1);
        }
    }
}

