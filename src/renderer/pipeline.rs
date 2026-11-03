// pipeline.rs — geometry render pipelines (Phase 2). Toolpath pipeline arrives in Phase 3.
use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu;
use egui_wgpu::wgpu::util::DeviceExt;

use crate::{
    cam::{
        operations::Operation,
        toolpath::{MoveKind, ToolMove},
    },
    workspace::{
        layer::Layer,
        model::Model,
        setup::{MachiningSetup, WorkAxis},
    },
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

fn append_shaded_triangle(
    scene: &mut Scene,
    points: [[f32; 3]; 3],
    base: [f32; 3],
    orbit: [f32; 2],
) {
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
    let magnitude = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2])
        .sqrt()
        .max(1e-8);
    let light = (normal[0] * 0.3 + normal[1] * 0.5 + normal[2] * 0.8) / magnitude;
    let shade = 0.28 + 0.72 * light.max(0.0);
    let color = [base[0] * shade, base[1] * shade, base[2] * shade, 1.0];
    for point in points {
        scene.triangles.push(Vertex {
            pos: project_point(point[0], point[1], point[2], true, orbit),
            color,
        });
    }
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
        orbit_preview: bool,
        setup: &MachiningSetup,
        show_stock_removal: bool,
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
            // During orbit drags, sample a bounded number of triangles and skip
            // the expensive painter sort. The complete sorted mesh is restored
            // on the first frame after the drag ends.
            let stride = if orbit_preview {
                (model.triangles.len() + 11_999) / 12_000
            } else {
                1
            }
            .max(1);
            let base = if selected_model == Some(model_index) {
                [0.95, 0.65, 0.28]
            } else {
                [0.55, 0.68, 0.82]
            };
            let mut faces = if orbit_preview {
                Vec::new()
            } else {
                Vec::with_capacity(model.triangles.len())
            };
            for face in model.triangles.iter().step_by(stride) {
                let points = face.map(|index| model.vertices[index as usize]);
                if orbit_preview {
                    append_shaded_triangle(&mut scene, points, base, orbit);
                } else {
                    let depth = points
                        .iter()
                        .map(|p| camera_point(*p, orbit)[2])
                        .sum::<f32>()
                        / 3.0;
                    faces.push((depth, points));
                }
            }
            if !orbit_preview {
                faces.sort_by(|a, b| a.0.total_cmp(&b.0));
                for (_, points) in faces {
                    append_shaded_triangle(&mut scene, points, base, orbit);
                }
            }
        }
        // Show the configured stock envelope in 3D so model placement and
        // generated toolpaths can be inspected against the actual setup.
        if view_3d {
            let min = setup.stock_origin_mm;
            let max = [
                min[0] + setup.stock_size_mm[0],
                min[1] + setup.stock_size_mm[1],
                match setup.axis {
                    WorkAxis::ZUp => min[2] + setup.stock_size_mm[2],
                    WorkAxis::ZDown => min[2] - setup.stock_size_mm[2],
                },
            ];
            let z_min = min[2].min(max[2]) as f32;
            let z_max = min[2].max(max[2]) as f32;
            let x_min = min[0] as f32;
            let x_max = max[0] as f32;
            let y_min = min[1] as f32;
            let y_max = max[1] as f32;
            let corners = [
                [x_min, y_min, z_min],
                [x_max, y_min, z_min],
                [x_max, y_max, z_min],
                [x_min, y_max, z_min],
                [x_min, y_min, z_max],
                [x_max, y_min, z_max],
                [x_max, y_max, z_max],
                [x_min, y_max, z_max],
            ];
            for (a, b) in [
                (0, 1),
                (1, 2),
                (2, 3),
                (3, 0),
                (4, 5),
                (5, 6),
                (6, 7),
                (7, 4),
                (0, 4),
                (1, 5),
                (2, 6),
                (3, 7),
            ] {
                let color = [0.45, 0.62, 0.78, 0.72];
                for corner in [corners[a], corners[b]] {
                    scene.lines.push(Vertex {
                        pos: project_point(corner[0], corner[1], corner[2], true, orbit),
                        color,
                    });
                }
            }
        }
        if view_3d && show_stock_removal && !orbit_preview {
            append_stock_removal_preview(&mut scene, operations, setup, orbit);
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

/// Approximate stock removal with a 64 by 64 sampled height field. Each cutting
/// move sweeps a circular cutter footprint over the grid; vertical walls and
/// cutter engagement between grid samples are not modeled.
fn append_stock_removal_preview(
    scene: &mut Scene,
    operations: &[Operation],
    setup: &MachiningSetup,
    orbit: [f32; 2],
) {
    const CELLS: usize = 64;
    let nx = CELLS;
    let ny = CELLS;
    let origin = setup.stock_origin_mm;
    let dx = setup.stock_size_mm[0] / nx as f64;
    let dy = setup.stock_size_mm[1] / ny as f64;
    let top = match setup.axis {
        WorkAxis::ZUp => origin[2] + setup.stock_size_mm[2],
        WorkAxis::ZDown => origin[2],
    };
    let floor = match setup.axis {
        WorkAxis::ZUp => origin[2],
        WorkAxis::ZDown => origin[2] - setup.stock_size_mm[2],
    };
    let mut heights = vec![top; (nx + 1) * (ny + 1)];

    for operation in operations.iter().filter(|op| op.generated) {
        let radius = operation.tool.diameter_mm * 0.5;
        let mut current = [0.0, 0.0, top];
        for movement in &operation.toolpath.moves {
            match movement {
                ToolMove::Rapid { to, z } => current = [to[0], to[1], *z],
                ToolMove::Feed { to, z, .. } => {
                    apply_cut_segment(
                        &mut heights,
                        nx,
                        ny,
                        origin,
                        dx,
                        dy,
                        floor,
                        [current[0], current[1], current[2]],
                        [to[0], to[1], *z],
                        radius,
                    );
                    current = [to[0], to[1], *z];
                }
                ToolMove::Arc {
                    to,
                    center_offset,
                    z,
                    clockwise,
                    ..
                } => {
                    let center = [current[0] + center_offset[0], current[1] + center_offset[1]];
                    let start_angle = (current[1] - center[1]).atan2(current[0] - center[0]);
                    let end_angle = (to[1] - center[1]).atan2(to[0] - center[0]);
                    let sweep = arc_sweep(start_angle, end_angle, *clockwise);
                    let radius_arc = (current[0] - center[0]).hypot(current[1] - center[1]);
                    let samples = ((sweep.abs() / 5_f64.to_radians()).ceil() as usize).max(1);
                    let mut previous = current;
                    for sample in 1..=samples {
                        let t = sample as f64 / samples as f64;
                        let angle = start_angle + sweep * t;
                        let next = [
                            center[0] + radius_arc * angle.cos(),
                            center[1] + radius_arc * angle.sin(),
                            current[2] + (z - current[2]) * t,
                        ];
                        apply_cut_segment(
                            &mut heights,
                            nx,
                            ny,
                            origin,
                            dx,
                            dy,
                            floor,
                            previous,
                            next,
                            radius,
                        );
                        previous = next;
                    }
                    current = [to[0], to[1], *z];
                }
                ToolMove::Drill { at, depth, .. } => {
                    apply_cut_segment(
                        &mut heights,
                        nx,
                        ny,
                        origin,
                        dx,
                        dy,
                        floor,
                        [at[0], at[1], top],
                        [at[0], at[1], *depth],
                        radius,
                    );
                    current = [at[0], at[1], *depth];
                }
            }
        }
    }

    let point = |ix: usize, iy: usize| {
        [
            (origin[0] + ix as f64 * dx) as f32,
            (origin[1] + iy as f64 * dy) as f32,
            heights[iy * (nx + 1) + ix] as f32,
        ]
    };
    let base = [0.25, 0.72, 0.46];
    for iy in 0..ny {
        for ix in 0..nx {
            let a = point(ix, iy);
            let b = point(ix + 1, iy);
            let c = point(ix + 1, iy + 1);
            let d = point(ix, iy + 1);
            append_shaded_triangle(scene, [a, b, c], base, orbit);
            append_shaded_triangle(scene, [a, c, d], base, orbit);
        }
    }
}

fn apply_cut_segment(
    heights: &mut [f64],
    nx: usize,
    ny: usize,
    origin: [f64; 3],
    dx: f64,
    dy: f64,
    floor: f64,
    start: [f64; 3],
    end: [f64; 3],
    cutter_radius: f64,
) {
    let radius = cutter_radius + (dx * dx + dy * dy).sqrt() * 0.5;
    let min_x = (((start[0].min(end[0]) - radius - origin[0]) / dx).floor() as isize)
        .clamp(0, nx as isize) as usize;
    let max_x = (((start[0].max(end[0]) + radius - origin[0]) / dx).ceil() as isize)
        .clamp(0, nx as isize) as usize;
    let min_y = (((start[1].min(end[1]) - radius - origin[1]) / dy).floor() as isize)
        .clamp(0, ny as isize) as usize;
    let max_y = (((start[1].max(end[1]) + radius - origin[1]) / dy).ceil() as isize)
        .clamp(0, ny as isize) as usize;
    let ab = [end[0] - start[0], end[1] - start[1]];
    let length_sq = ab[0] * ab[0] + ab[1] * ab[1];
    let radius_sq = radius * radius;
    for iy in min_y..=max_y {
        for ix in min_x..=max_x {
            let x = origin[0] + ix as f64 * dx;
            let y = origin[1] + iy as f64 * dy;
            let t = if length_sq > f64::EPSILON {
                (((x - start[0]) * ab[0] + (y - start[1]) * ab[1]) / length_sq).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let px = start[0] + ab[0] * t;
            let py = start[1] + ab[1] * t;
            let dist_sq = (x - px) * (x - px) + (y - py) * (y - py);
            if dist_sq <= radius_sq {
                let cut_z = (start[2] + (end[2] - start[2]) * t).max(floor);
                let height = &mut heights[iy * (nx + 1) + ix];
                *height = height.min(cut_z);
            }
        }
    }
}

fn arc_sweep(start: f64, end: f64, clockwise: bool) -> f64 {
    let mut sweep = end - start;
    if clockwise {
        while sweep >= 0.0 {
            sweep -= std::f64::consts::TAU;
        }
    } else {
        while sweep <= 0.0 {
            sweep += std::f64::consts::TAU;
        }
    }
    sweep
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

/// Project a 3D world position into the same orthographic coordinates used by
/// the viewport scene, for egui overlays such as the simulated cutter marker.
pub fn project_view_point(point: [f32; 3], orbit: [f32; 2]) -> [f32; 2] {
    project_point(point[0], point[1], point[2], true, orbit)
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
