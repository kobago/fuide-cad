//! wgpu renderer: hologram fill + glowing lines, off-screen (MSAA 4x, own depth) into a texture
//! registered with egui. Rebuilds GPU buffers when `Scene::version` changes.

use crate::camera::OrbitCamera;
use crate::scene::{LineBatch, Scene};
use bytemuck::{Pod, Zeroable};
use egui_wgpu::RenderState;
use std::ops::Range;
use wgpu::util::DeviceExt;

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SAMPLES: u32 = 4;
/// Minimum uniform buffer offset alignment (spec: 256).
const UNIFORM_STRIDE: u64 = 256;
/// Lines are not biased; instead the fill is pushed back by a polygon offset (see
/// [`FILL_DEPTH_BIAS`]) so edges lying on a face win the depth test without hidden edges
/// behind a face showing through near silhouettes.
const EDGE_DEPTH_BIAS: f32 = 0.0;
/// Polygon offset of the fill: a couple of depth units plus a slope term for steep faces.
const FILL_DEPTH_BIAS: wgpu::DepthBiasState = wgpu::DepthBiasState {
    constant: 2,
    slope_scale: 1.5,
    clamp: 0.0,
};
/// Glow layer: wider and fainter than the core line.
const GLOW_WIDTH: f32 = 2.4;
const GLOW_ALPHA: f32 = 0.16;

/// Look of one render. Colours are straight rgb 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    pub accent: [f32; 3],
    /// 1 = fully hologram-tinted, 0 = the mesh's own colour.
    pub holo_mix: f32,
    pub scan_strength: f32,
    /// 0 hides the fill (wireframe).
    pub fill_alpha: f32,
    /// Visible edges.
    pub edge_alpha: f32,
    /// Edges behind the fill (X-ray); 0 = hidden.
    pub hidden_alpha: f32,
    pub glow: bool,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            accent: [0.0, 0.9, 1.0],
            holo_mix: 1.0,
            scan_strength: 0.22,
            fill_alpha: 0.96,
            edge_alpha: 0.9,
            hidden_alpha: 0.0,
            glow: true,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    cam_pos: [f32; 3],
    time: f32,
    accent: [f32; 3],
    holo_mix: f32,
    params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LineInst {
    a: [f32; 3],
    width: f32,
    b: [f32; 3],
    depth_bias: f32,
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MeshVertex {
    pos: [f32; 3],
    nor: [f32; 3],
}

/// Slots in the line-parameter uniform (dynamic offsets).
const LP_OVERLAY: u32 = 0;
const LP_EDGE: u32 = 1;
const LP_GLOW: u32 = 2;
const LP_HIDDEN: u32 = 3;
const LP_SLOTS: u64 = 4;

struct Targets {
    size: [u32; 2],
    msaa: wgpu::TextureView,
    resolve: wgpu::TextureView,
    depth: wgpu::TextureView,
}

struct MeshDraw {
    indices: Range<u32>,
    object: u32,
}

struct LineDraw {
    instances: Range<u32>,
    depth_test: bool,
}

struct Uploaded {
    version: u64,
    vertices: Option<wgpu::Buffer>,
    indices: Option<wgpu::Buffer>,
    objects: Option<wgpu::Buffer>,
    object_bg: Option<wgpu::BindGroup>,
    meshes: Vec<MeshDraw>,
    lines: Option<wgpu::Buffer>,
    overlay: Vec<LineDraw>,
    edges: Vec<LineDraw>,
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    mesh_pipeline: wgpu::RenderPipeline,
    line_depth_pipeline: wgpu::RenderPipeline,
    line_always_pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    globals_bg: wgpu::BindGroup,
    object_layout: wgpu::BindGroupLayout,
    line_params: wgpu::Buffer,
    line_params_bg: wgpu::BindGroup,
    targets: Option<Targets>,
    uploaded: Uploaded,
    tex_id: Option<egui::TextureId>,
}

impl Renderer {
    pub fn new(rs: &RenderState) -> Self {
        let device = rs.device.clone();
        let queue = rs.queue.clone();
        let holo = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fuide-3d holo"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/holo.wgsl").into()),
        });
        let lines = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fuide-3d lines"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/lines.wgsl").into()),
        });
        let uniform_layout = |label: &str, dynamic: bool, min: u64| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: dynamic,
                        min_binding_size: wgpu::BufferSize::new(min),
                    },
                    count: None,
                }],
            })
        };
        let globals_layout = uniform_layout(
            "fuide-3d globals",
            false,
            std::mem::size_of::<Globals>() as u64,
        );
        let object_layout = uniform_layout("fuide-3d object", true, 16);
        let line_params_layout = uniform_layout("fuide-3d line params", true, 16);

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fuide-3d globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fuide-3d globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let line_params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fuide-3d line params"),
            size: UNIFORM_STRIDE * LP_SLOTS,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let line_params_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fuide-3d line params"),
            layout: &line_params_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &line_params,
                    offset: 0,
                    size: wgpu::BufferSize::new(16),
                }),
            }],
        });

        let blend = Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        let target = [Some(wgpu::ColorTargetState {
            format: COLOR_FORMAT,
            blend,
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let multisample = wgpu::MultisampleState {
            count: SAMPLES,
            ..Default::default()
        };
        let depth = |compare: wgpu::CompareFunction, write: bool, bias: wgpu::DepthBiasState| {
            Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(write),
                depth_compare: Some(compare),
                stencil: Default::default(),
                bias,
            })
        };

        let mesh_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fuide-3d mesh"),
            bind_group_layouts: &[Some(&globals_layout), Some(&object_layout)],
            immediate_size: 0,
        });
        let mesh_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fuide-3d mesh"),
            layout: Some(&mesh_layout),
            vertex: wgpu::VertexState {
                module: &holo,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<MeshVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: depth(wgpu::CompareFunction::Less, true, FILL_DEPTH_BIAS),
            multisample,
            fragment: Some(wgpu::FragmentState {
                module: &holo,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &target,
            }),
            multiview_mask: None,
            cache: None,
        });

        let line_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fuide-3d lines"),
            bind_group_layouts: &[Some(&globals_layout), Some(&line_params_layout)],
            immediate_size: 0,
        });
        let line_pipeline = |label: &str, compare: wgpu::CompareFunction| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&line_layout),
                vertex: wgpu::VertexState {
                    module: &lines,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<LineInst>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
                            0 => Float32x3, 1 => Float32, 2 => Float32x3, 3 => Float32, 4 => Float32x4
                        ],
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: depth(compare, false, Default::default()),
                multisample,
                fragment: Some(wgpu::FragmentState {
                    module: &lines,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &target,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let line_depth_pipeline =
            line_pipeline("fuide-3d lines depth", wgpu::CompareFunction::LessEqual);
        let line_always_pipeline =
            line_pipeline("fuide-3d lines always", wgpu::CompareFunction::Always);

        Self {
            device,
            queue,
            mesh_pipeline,
            line_depth_pipeline,
            line_always_pipeline,
            globals,
            globals_bg,
            object_layout,
            line_params,
            line_params_bg,
            targets: None,
            uploaded: Uploaded {
                version: u64::MAX,
                vertices: None,
                indices: None,
                objects: None,
                object_bg: None,
                meshes: Vec::new(),
                lines: None,
                overlay: Vec::new(),
                edges: Vec::new(),
            },
            tex_id: None,
        }
    }

    fn ensure_targets(&mut self, size: [u32; 2]) -> bool {
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return false;
        }
        let tex =
            |label: &str, samples: u32, format: wgpu::TextureFormat, usage: wgpu::TextureUsages| {
                self.device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some(label),
                        size: wgpu::Extent3d {
                            width: size[0],
                            height: size[1],
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: samples,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage,
                        view_formats: &[],
                    })
                    .create_view(&Default::default())
            };
        let ra = wgpu::TextureUsages::RENDER_ATTACHMENT;
        self.targets = Some(Targets {
            size,
            msaa: tex("fuide-3d msaa", SAMPLES, COLOR_FORMAT, ra),
            resolve: tex(
                "fuide-3d color",
                1,
                COLOR_FORMAT,
                ra | wgpu::TextureUsages::TEXTURE_BINDING,
            ),
            depth: tex("fuide-3d depth", SAMPLES, DEPTH_FORMAT, ra),
        });
        true
    }

    fn upload(&mut self, scene: &Scene) {
        if self.uploaded.version == scene.version {
            return;
        }
        let up = &mut self.uploaded;
        up.version = scene.version;
        // meshes: one vertex / index buffer, one object uniform per mesh (dynamic offset)
        let mut verts: Vec<MeshVertex> = Vec::new();
        let mut idx: Vec<u32> = Vec::new();
        let mut objects: Vec<u8> = Vec::new();
        up.meshes.clear();
        for (i, m) in scene.meshes.iter().enumerate() {
            let base = verts.len() as u32;
            verts.extend(
                m.positions
                    .iter()
                    .zip(m.normals.iter())
                    .map(|(p, n)| MeshVertex { pos: *p, nor: *n }),
            );
            let start = idx.len() as u32;
            idx.extend(m.indices.iter().map(|k| k + base));
            up.meshes.push(MeshDraw {
                indices: start..idx.len() as u32,
                object: i as u32,
            });
            let mut chunk = vec![0u8; UNIFORM_STRIDE as usize];
            chunk[..16].copy_from_slice(bytemuck::bytes_of(&m.color));
            objects.extend_from_slice(&chunk);
        }
        let usage = wgpu::BufferUsages::VERTEX;
        up.vertices = (!verts.is_empty()).then(|| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("fuide-3d vertices"),
                    contents: bytemuck::cast_slice(&verts),
                    usage,
                })
        });
        up.indices = (!idx.is_empty()).then(|| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("fuide-3d indices"),
                    contents: bytemuck::cast_slice(&idx),
                    usage: wgpu::BufferUsages::INDEX,
                })
        });
        up.objects = (!objects.is_empty()).then(|| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("fuide-3d objects"),
                    contents: &objects,
                    usage: wgpu::BufferUsages::UNIFORM,
                })
        });
        up.object_bg = up.objects.as_ref().map(|buf| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("fuide-3d objects"),
                layout: &self.object_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: buf,
                        offset: 0,
                        size: wgpu::BufferSize::new(16),
                    }),
                }],
            })
        });
        // lines: one instance buffer, ranges per batch
        let mut inst: Vec<LineInst> = Vec::new();
        let mut push = |batch: &LineBatch, bias: f32, out: &mut Vec<LineDraw>| {
            let start = inst.len() as u32;
            for pair in batch.points.chunks_exact(2) {
                inst.push(LineInst {
                    a: pair[0],
                    width: batch.width,
                    b: pair[1],
                    depth_bias: bias,
                    color: batch.color,
                });
            }
            out.push(LineDraw {
                instances: start..inst.len() as u32,
                depth_test: batch.depth_test,
            });
        };
        up.overlay.clear();
        up.edges.clear();
        for b in &scene.overlay {
            push(b, 0.0, &mut up.overlay);
        }
        for b in &scene.edges {
            push(b, EDGE_DEPTH_BIAS, &mut up.edges);
        }
        up.lines = (!inst.is_empty()).then(|| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("fuide-3d lines"),
                    contents: bytemuck::cast_slice(&inst),
                    usage,
                })
        });
    }

    /// Render `scene` at `size` pixels and return the egui texture to paint. `time` in seconds
    /// drives the scan lines.
    pub fn render(
        &mut self,
        rs: &RenderState,
        scene: &Scene,
        camera: &OrbitCamera,
        size: [u32; 2],
        style: &Style,
        time: f32,
    ) -> egui::TextureId {
        let size = [size[0].clamp(1, 8192), size[1].clamp(1, 8192)];
        let resized = self.ensure_targets(size);
        self.upload(scene);
        let aspect = size[0] as f32 / size[1] as f32;
        let g = Globals {
            view_proj: camera.view_proj(aspect),
            cam_pos: camera.eye().to_array(),
            time,
            accent: style.accent,
            holo_mix: style.holo_mix,
            params: [
                style.scan_strength,
                style.fill_alpha,
                size[0] as f32,
                size[1] as f32,
            ],
        };
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&g));
        // alpha, width scale, cap (see lines.wgsl), pad
        let slots: [[f32; 4]; LP_SLOTS as usize] = [
            [1.0, 1.0, 1.0, 0.0],
            [style.edge_alpha, 1.0, 0.0, 0.0],
            [style.edge_alpha * GLOW_ALPHA, GLOW_WIDTH, 0.0, 0.0],
            [style.hidden_alpha, 1.0, 1.0, 0.0],
        ];
        for (i, s) in slots.iter().enumerate() {
            self.queue.write_buffer(
                &self.line_params,
                UNIFORM_STRIDE * i as u64,
                bytemuck::bytes_of(s),
            );
        }

        let targets = self.targets.as_ref().expect("targets");
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("fuide-3d"),
            });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fuide-3d"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.msaa,
                    depth_slice: None,
                    resolve_target: Some(&targets.resolve),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.globals_bg, &[]);
            let up = &self.uploaded;
            if style.fill_alpha > 0.0 {
                if let (Some(v), Some(i), Some(bg)) = (&up.vertices, &up.indices, &up.object_bg) {
                    pass.set_pipeline(&self.mesh_pipeline);
                    pass.set_vertex_buffer(0, v.slice(..));
                    pass.set_index_buffer(i.slice(..), wgpu::IndexFormat::Uint32);
                    for m in &up.meshes {
                        pass.set_bind_group(1, bg, &[m.object * UNIFORM_STRIDE as u32]);
                        pass.draw_indexed(m.indices.clone(), 0, 0..1);
                    }
                }
            }
            if let Some(lines) = &up.lines {
                pass.set_vertex_buffer(0, lines.slice(..));
                let draw = |pass: &mut wgpu::RenderPass<'_>,
                            d: &LineDraw,
                            always: bool,
                            slot: u32| {
                    pass.set_pipeline(if always || !d.depth_test {
                        &self.line_always_pipeline
                    } else {
                        &self.line_depth_pipeline
                    });
                    pass.set_bind_group(1, &self.line_params_bg, &[slot * UNIFORM_STRIDE as u32]);
                    pass.draw(0..6, d.instances.clone());
                };
                for d in &up.overlay {
                    draw(&mut pass, d, false, LP_OVERLAY);
                }
                if style.hidden_alpha > 0.0 {
                    for d in &up.edges {
                        draw(&mut pass, d, true, LP_HIDDEN);
                    }
                }
                if style.glow {
                    for d in &up.edges {
                        draw(&mut pass, d, false, LP_GLOW);
                    }
                }
                for d in &up.edges {
                    draw(&mut pass, d, false, LP_EDGE);
                }
            }
        }
        self.queue.submit(Some(enc.finish()));

        let mut renderer = rs.renderer.write();
        match self.tex_id {
            Some(id) if !resized => id,
            Some(id) => {
                renderer.update_egui_texture_from_wgpu_texture(
                    &self.device,
                    &targets.resolve,
                    wgpu::FilterMode::Linear,
                    id,
                );
                id
            }
            None => {
                let id = renderer.register_native_texture(
                    &self.device,
                    &targets.resolve,
                    wgpu::FilterMode::Linear,
                );
                self.tex_id = Some(id);
                id
            }
        }
    }
}
