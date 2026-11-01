// pipeline.rs — geometry render pipelines (Phase 2). Toolpath pipeline arrives in Phase 3.
use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu;
use egui_wgpu::wgpu::util::DeviceExt;

use crate::{
    cam::{
        operations::Operation,
        toolpath::{MoveKind, ToolMove},
    },
    workspace::{layer::Layer, model::Model},
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
    pub fn build(
        layers: &[Layer],
        models: &[Model],
        operations: &[Operation],
        revision: u64,
        view_3d: bool,
        orbit: [f32; 2],
        selected_model: Option<usize>,
    ) -> Self {
        let mut scene = Scene {
            revision,
            ..Default::default()
        };
        for l in layers.iter().filter(|l| l.visible) {
            let color = l.color;
            scene
                .triangles
                .extend(l.geometry.triangles.iter().map(|p| Vertex {
                    pos: project_point(p[0], p[1], 0.0, view_3d, orbit),
                    color,
                }));
            scene.lines.extend(l.geometry.lines.iter().map(|p| Vertex {
                pos: project_point(p[0], p[1], 0.0, view_3d, orbit),
                color,
            }));
        }
        for (model_index, model) in models
            .iter()
            .enumerate()
            .filter(|(_, model)| view_3d && model.visible)
        {
            // Painter-sort triangles from far to near. This gives useful opaque
            // shaded solids while the viewport still uses its shared 2D pass.
            let model_faces: Vec<[u32; 3]> = if model.bodies.is_empty() {
                model.triangles.clone()
            } else {
                model
                    .bodies
                    .iter()
                    .filter_map(|body| {
                        model
                            .triangles
                            .get(body.first_triangle..body.first_triangle + body.triangle_count)
                    })
                    .flatten()
                    .copied()
                    .collect()
            };
            let mut faces: Vec<_> = model_faces
                .iter()
                .map(|face| {
                    let points = face.map(|index| model.vertices[index as usize]);
                    let depth = points
                        .iter()
                        .map(|p| camera_point(*p, orbit)[2])
                        .sum::<f32>()
                        / 3.0;
                    (depth, points)
                })
                .collect();
            faces.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (_, points) in faces {
                let u = [
                    points[1][0] - points[0][0],
                    points[1][1] - points[0][1],
                    points[1][2] - points[0][2],
                ];
                let v = [
                    points[2][0] - points[0][0],
                    points[2][1] - points[0][1],
                    points[2][2] - points[0][2],
                ];
                let normal = [
                    u[1] * v[2] - u[2] * v[1],
                    u[2] * v[0] - u[0] * v[2],
                    u[0] * v[1] - u[1] * v[0],
                ];
                let magnitude =
                    (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2])
                        .sqrt()
                        .max(1e-8);
                let light = (normal[0] * 0.3 + normal[1] * 0.5 + normal[2] * 0.8) / magnitude;
                let shade = 0.28 + 0.72 * light.max(0.0);
                let base = if selected_model == Some(model_index) {
                    [0.95, 0.65, 0.28]
                } else {
                    [0.55, 0.68, 0.82]
                };
                let color = [base[0] * shade, base[1] * shade, base[2] * shade, 1.0];
                for point in points {
                    scene.triangles.push(Vertex {
                        pos: project_point(point[0], point[1], point[2], view_3d, orbit),
                        color,
                    });
                }
            }
        }
        if view_3d {
            if let Some(model) = models.iter().find(|model| model.visible) {
                let extent = (0..3)
                    .map(|axis| (model.bounds[1][axis] - model.bounds[0][axis]).abs())
                    .fold(1.0_f32, f32::max)
                    * 0.6;
                for (end, color) in [
                    ([extent, 0.0, 0.0], [0.95, 0.3, 0.3, 1.0]),
                    ([0.0, extent, 0.0], [0.35, 0.85, 0.4, 1.0]),
                    ([0.0, 0.0, extent], [0.35, 0.6, 1.0, 1.0]),
                ] {
                    scene.lines.push(Vertex {
                        pos: project_point(0.0, 0.0, 0.0, true, orbit),
                        color,
                    });
                    scene.lines.push(Vertex {
                        pos: project_point(end[0], end[1], end[2], true, orbit),
                        color,
                    });
                }
            }
        }
        for op in operations.iter().filter(|op| op.generated) {
            let colour = |kind| match kind {
                MoveKind::Rapid => [0.35, 0.55, 0.95, 0.55],
                MoveKind::Feed => [1.0, 0.45, 0.15, 1.0],
                MoveKind::Arc => [0.85, 0.35, 1.0, 1.0],
                MoveKind::Drill => [1.0, 0.85, 0.15, 1.0],
            };
            let mut previous: Option<[f32; 3]> = None;
            for mv in &op.toolpath.moves {
                let (end, z) = match mv {
                    ToolMove::Rapid { to, z }
                    | ToolMove::Feed { to, z, .. }
                    | ToolMove::Arc { to, z, .. } => (*to, *z),
                    ToolMove::Drill { at, depth, .. } => (*at, *depth),
                };
                let p = [end[0] as f32, end[1] as f32, z as f32];
                if let Some(start) = previous {
                    scene.toolpaths.push(Vertex {
                        pos: project_point(start[0], start[1], start[2], view_3d, orbit),
                        color: colour(mv.kind()),
                    });
                    scene.toolpaths.push(Vertex {
                        pos: project_point(p[0], p[1], p[2], view_3d, orbit),
                        color: colour(mv.kind()),
                    });
                }
                previous = Some(p);
            }
        }
        scene
    }

    pub fn bounds(&self) -> Option<([f32; 2], [f32; 2])> {
        self.triangles
            .iter()
            .chain(&self.lines)
            .chain(&self.toolpaths)
            .fold(None, |bounds, vertex| {
                Some(match bounds {
                    None => (vertex.pos, vertex.pos),
                    Some((min, max)) => (
                        [min[0].min(vertex.pos[0]), min[1].min(vertex.pos[1])],
                        [max[0].max(vertex.pos[0]), max[1].max(vertex.pos[1])],
                    ),
                })
            })
    }
}

pub fn projected_model_bounds(model: &Model, orbit: [f32; 2]) -> ([f32; 2], [f32; 2]) {
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    for x in [model.bounds[0][0], model.bounds[1][0]] {
        for y in [model.bounds[0][1], model.bounds[1][1]] {
            for z in [model.bounds[0][2], model.bounds[1][2]] {
                let point = project_point(x, y, z, true, orbit);
                for axis in 0..2 {
                    min[axis] = min[axis].min(point[axis]);
                    max[axis] = max[axis].max(point[axis]);
                }
            }
        }
    }
    (min, max)
}

fn project_point(x: f32, y: f32, z: f32, view_3d: bool, orbit: [f32; 2]) -> [f32; 2] {
    if view_3d {
        let camera = camera_point([x, y, z], orbit);
        [camera[0], camera[1]]
    } else {
        [x, y]
    }
}

fn camera_point(point: [f32; 3], orbit: [f32; 2]) -> [f32; 3] {
    let yaw = std::f32::consts::FRAC_PI_4 + orbit[0];
    let pitch = (35.264_f32.to_radians() + orbit[1]).clamp(0.05, 1.52);
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let x = point[0] * cy - point[1] * sy;
    let y = point[0] * sy + point[1] * cy;
    [x, y * sp + point[2] * cp, y * cp - point[2] * sp]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isometric_projection_includes_tool_depth() {
        let surface = project_point(4.0, 2.0, 0.0, true, [0.0, 0.0]);
        let cut = project_point(4.0, 2.0, -3.0, true, [0.0, 0.0]);
        assert_eq!(surface[0], cut[0]);
        // Negative CNC Z is below the work plane and projects downward in the view.
        assert!(cut[1] < surface[1]);
        assert_eq!(project_point(4.0, 2.0, -3.0, false, [0.0, 0.0]), [4.0, 2.0]);
    }
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
