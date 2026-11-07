use crate::renderer::camera::Camera2D;
use bytemuck::{Pod, Zeroable};
use egui_wgpu::{wgpu, ScreenDescriptor};

/// Uniform buffer layout (must match grid.wgsl)
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct GridUniforms {
    view_proj: [[f32; 4]; 4],
    viewport_size: [f32; 2],
    zoom: f32,
    show_axes: f32,
    /// World-space grid spacing (major / minor)
    minor_spacing: f32,
    major_spacing: f32,
    _pad2: [f32; 2],
}

/// GPU resources for infinite grid rendering.
pub struct GridRenderer {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl GridRenderer {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        // Uniform buffer
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("grid_uniforms"),
            size: std::mem::size_of::<GridUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Bind group layout
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("grid_bgl"),
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

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("grid_bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("grid_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../assets/shaders/grid.wgsl").into()),
        });

        // Pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("grid_pipeline_layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        // Render pipeline (full-screen triangle, no vertex buffer needed)
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("grid_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
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
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            uniform_buffer,
            bind_group,
        }
    }

    pub fn update_uniforms(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: &Camera2D,
        viewport_size: [f32; 2],
        show_axes: bool,
    ) {
        let vp_w = viewport_size[0].max(1.0);
        let vp_h = viewport_size[1].max(1.0);

        let mat = camera.view_proj_matrix((vp_w, vp_h));
        let cols = mat.to_cols_array_2d();

        // Adaptive grid spacing: 1 mm minor, 10 mm major
        let minor_spacing = 1.0_f32;
        let major_spacing = 10.0_f32;

        let uniforms = GridUniforms {
            view_proj: cols,
            viewport_size: [vp_w, vp_h],
            zoom: camera.zoom,
            show_axes: if show_axes { 1.0 } else { 0.0 },
            minor_spacing,
            major_spacing,
            _pad2: [0.0; 2],
        };

        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }

    pub fn draw(&self, render_pass: &mut wgpu::RenderPass<'static>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        // Full-screen triangle: 3 vertices, no vertex buffer
        render_pass.draw(0..3, 0..1);
    }
}
