// pipeline.rs — geometry render pipelines (Phase 2). Toolpath pipeline arrives in Phase 3.
use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu;
use egui_wgpu::wgpu::util::DeviceExt;

use crate::{
    cam::{
        operations::Operation,
        toolpath::{MoveKind, ToolMove},
    },
    workspace::layer::Layer,
};

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub color: [f32; 4],
}

/// CPU-side snapshot of everything drawn by the geometry pass.
#[derive(Default)]
pub struct Scene {
    /// Changes whenever the contents change (used to trigger GPU re-upload)
    pub revision: u64,
    pub triangles: Vec<Vertex>,
    pub lines: Vec<Vertex>,
    pub toolpaths: Vec<Vertex>,
}

impl Scene {
    pub fn build(layers: &[Layer], operations: &[Operation], revision: u64) -> Self {
        let mut scene = Scene {
            revision,
            ..Default::default()
        };
        for l in layers.iter().filter(|l| l.visible) {
            let color = l.color;
            scene.triangles.extend(
                l.geometry
                    .triangles
                    .iter()
                    .map(|p| Vertex { pos: *p, color }),
            );
            scene
                .lines
                .extend(l.geometry.lines.iter().map(|p| Vertex { pos: *p, color }));
        }
        for op in operations.iter().filter(|op| op.generated) {
            let colour = |kind| match kind {
                MoveKind::Rapid => [0.35, 0.55, 0.95, 0.55],
                MoveKind::Feed => [1.0, 0.45, 0.15, 1.0],
                MoveKind::Arc => [0.85, 0.35, 1.0, 1.0],
                MoveKind::Drill => [1.0, 0.85, 0.15, 1.0],
            };
            let mut previous = None;
            for mv in &op.toolpath.moves {
                let end = mv.end();
                let p = [end[0] as f32, end[1] as f32];
                if let Some(start) = previous {
                    scene.toolpaths.push(Vertex {
                        pos: start,
                        color: colour(mv.kind()),
                    });
                    scene.toolpaths.push(Vertex {
                        pos: p,
                        color: colour(mv.kind()),
                    });
                }
                previous = Some(p);
            }
        }
        scene
    }
}

pub struct GeometryRenderer {
    tri_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    tri_buffer: Option<wgpu::Buffer>,
    tri_count: u32,
    line_buffer: Option<wgpu::Buffer>,
    line_count: u32,
    toolpath_buffer: Option<wgpu::Buffer>,
    toolpath_count: u32,
    revision: u64,
}

impl GeometryRenderer {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("geometry_uniforms"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("geometry_bgl"),
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
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("geometry_bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("geometry_shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../assets/shaders/geometry.wgsl").into(),
            ),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("geometry_pipeline_layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let attrs = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];
        let make = |topology: wgpu::PrimitiveTopology, label: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attrs,
                    }],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        };

        Self {
            tri_pipeline: make(
                wgpu::PrimitiveTopology::TriangleList,
                "geometry_tri_pipeline",
            ),
            line_pipeline: make(wgpu::PrimitiveTopology::LineList, "geometry_line_pipeline"),
            uniform_buffer,
            bind_group,
            tri_buffer: None,
            tri_count: 0,
            line_buffer: None,
            line_count: 0,
            toolpath_buffer: None,
            toolpath_count: 0,
            revision: u64::MAX,
        }
    }

    pub fn update_camera(&self, queue: &wgpu::Queue, view_proj: &glam::Mat4) {
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&view_proj.to_cols_array()),
        );
    }

    /// Re-upload vertex buffers if the scene changed.
    pub fn sync(&mut self, device: &wgpu::Device, scene: &Scene) {
        if scene.revision == self.revision {
            return;
        }
        self.revision = scene.revision;
        let upload = |verts: &[Vertex], label: &str| {
            if verts.is_empty() {
                None
            } else {
                Some(
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(label),
                        contents: bytemuck::cast_slice(verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                )
            }
        };
        self.tri_buffer = upload(&scene.triangles, "geometry_tris");
        self.tri_count = scene.triangles.len() as u32;
        self.line_buffer = upload(&scene.lines, "geometry_lines");
        self.line_count = scene.lines.len() as u32;
        self.toolpath_buffer = upload(&scene.toolpaths, "toolpath_lines");
        self.toolpath_count = scene.toolpaths.len() as u32;
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'static>) {
        if let Some(buf) = &self.tri_buffer {
            pass.set_pipeline(&self.tri_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, buf.slice(..));
            pass.draw(0..self.tri_count, 0..1);
        }
        if let Some(buf) = &self.line_buffer {
            pass.set_pipeline(&self.line_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, buf.slice(..));
            pass.draw(0..self.line_count, 0..1);
        }
        if let Some(buf) = &self.toolpath_buffer {
            pass.set_pipeline(&self.line_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, buf.slice(..));
            pass.draw(0..self.toolpath_count, 0..1);
        }
    }
}
