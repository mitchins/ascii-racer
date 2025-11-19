use bytemuck::{Pod, Zeroable};
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
};
use glam::{Mat4, Vec3};
use std::io::{stdout, Write};
use std::time::{Duration, Instant};
use wgpu::util::DeviceExt;

const WIDTH: u32 = 160;
const HEIGHT: u32 = 48;
const TARGET_FPS: u64 = 30;
const FRAME_TIME: Duration = Duration::from_millis(1000 / TARGET_FPS);

// ASCII glyphs from darkest to brightest
const ASCII_RAMP: &[u8] = b" .'`^\",:;Il!i><~+_-?][}{1)(|\\/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$";

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
}

impl Vertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    model: [[f32; 4]; 4],
}

struct CarState {
    position: Vec3,
    rotation: f32,
    velocity: Vec3,
    steering: f32,
}

impl CarState {
    fn new() -> Self {
        Self {
            position: Vec3::new(0.0, 0.1, 0.0),
            rotation: 0.0,
            velocity: Vec3::ZERO,
            steering: 0.0,
        }
    }

    fn update(&mut self, dt: f32, forward: bool, backward: bool, left: bool, right: bool) {
        // Steering
        let steer_speed = 2.0;
        if left {
            self.steering += steer_speed * dt;
        }
        if right {
            self.steering -= steer_speed * dt;
        }
        self.steering *= 0.9; // Damping
        self.steering = self.steering.clamp(-1.0, 1.0);

        // Acceleration
        let accel = 3.0;
        let friction = 0.95;
        
        if forward {
            let forward_dir = Vec3::new(self.rotation.sin(), 0.0, self.rotation.cos());
            self.velocity += forward_dir * accel * dt;
        }
        if backward {
            let forward_dir = Vec3::new(self.rotation.sin(), 0.0, self.rotation.cos());
            self.velocity -= forward_dir * accel * dt * 0.5;
        }
        
        self.velocity *= friction;
        
        // Apply steering to rotation based on velocity
        let speed = self.velocity.length();
        self.rotation += self.steering * speed * dt;
        
        // Update position
        self.position += self.velocity * dt;
    }

    fn get_matrix(&self) -> Mat4 {
        Mat4::from_translation(self.position) * Mat4::from_rotation_y(self.rotation)
    }
}

struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    render_pipeline: wgpu::RenderPipeline,
    compute_pipeline: wgpu::ComputePipeline,
    
    #[allow(dead_code)]
    color_texture: wgpu::Texture,
    color_view: wgpu::TextureView,
    #[allow(dead_code)]
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    #[allow(dead_code)]
    normal_texture: wgpu::Texture,
    normal_view: wgpu::TextureView,
    
    track_vertex_buffer: wgpu::Buffer,
    track_index_buffer: wgpu::Buffer,
    track_index_count: u32,
    
    car_vertex_buffer: wgpu::Buffer,
    car_index_buffer: wgpu::Buffer,
    car_index_count: u32,
    
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    
    ascii_buffer: wgpu::Buffer,
    ascii_staging_buffer: wgpu::Buffer,
    compute_bind_group: wgpu::BindGroup,
}

impl Renderer {
    async fn new() -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: Default::default(),
                },
                None,
            )
            .await
            .unwrap();

        // Create G-buffer textures
        let color_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Color Texture"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let normal_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Normal Texture"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Snorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let normal_view = normal_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Create track geometry (circular track)
        let (track_vertices, track_indices) = create_track_geometry();
        let track_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Track Vertex Buffer"),
            contents: bytemuck::cast_slice(&track_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let track_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Track Index Buffer"),
            contents: bytemuck::cast_slice(&track_indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let track_index_count = track_indices.len() as u32;

        // Create car geometry (box)
        let (car_vertices, car_indices) = create_car_geometry();
        let car_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Car Vertex Buffer"),
            contents: bytemuck::cast_slice(&car_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let car_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Car Index Buffer"),
            contents: bytemuck::cast_slice(&car_indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let car_index_count = car_indices.len() as u32;

        // Create uniform buffer
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniform Buffer"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create bind group layout and bind group
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bind Group Layout"),
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
            label: Some("Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Create render shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Snorm,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                ],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // Create ASCII buffer (output)
        let ascii_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ASCII Buffer"),
            size: (WIDTH * HEIGHT) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        // Create staging buffer for CPU readback
        let ascii_staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ASCII Staging Buffer"),
            size: (WIDTH * HEIGHT) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create compute shader
        let compute_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("compute.wgsl").into()),
        });

        let compute_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Compute Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let compute_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Compute Bind Group"),
            layout: &compute_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&normal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: ascii_buffer.as_entire_binding(),
                },
            ],
        });

        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Compute Pipeline Layout"),
                bind_group_layouts: &[&compute_bind_group_layout],
                push_constant_ranges: &[],
            });

        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Compute Pipeline"),
            layout: Some(&compute_pipeline_layout),
            module: &compute_shader,
            entry_point: "main",
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            device,
            queue,
            render_pipeline,
            compute_pipeline,
            color_texture,
            color_view,
            depth_texture,
            depth_view,
            normal_texture,
            normal_view,
            track_vertex_buffer,
            track_index_buffer,
            track_index_count,
            car_vertex_buffer,
            car_index_buffer,
            car_index_count,
            uniform_buffer,
            bind_group,
            ascii_buffer,
            ascii_staging_buffer,
            compute_bind_group,
        }
    }

    fn render(&self, car_state: &CarState) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // Camera follows the car
        let camera_offset = Vec3::new(
            -car_state.rotation.sin() * 3.0,
            2.5,
            -car_state.rotation.cos() * 3.0,
        );
        let camera_pos = car_state.position + camera_offset;
        let view = Mat4::look_at_rh(camera_pos, car_state.position, Vec3::Y);
        let proj = Mat4::perspective_rh(
            std::f32::consts::PI / 3.0,
            WIDTH as f32 / HEIGHT as f32,
            0.1,
            100.0,
        );
        let view_proj = proj * view;

        // Render track
        {
            let uniforms = Uniforms {
                view_proj: view_proj.to_cols_array_2d(),
                model: Mat4::IDENTITY.to_cols_array_2d(),
            };
            self.queue
                .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.color_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.1,
                                g: 0.2,
                                b: 0.3,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.normal_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.track_vertex_buffer.slice(..));
            render_pass
                .set_index_buffer(self.track_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..self.track_index_count, 0, 0..1);
        }

        // Render car
        {
            let uniforms = Uniforms {
                view_proj: view_proj.to_cols_array_2d(),
                model: car_state.get_matrix().to_cols_array_2d(),
            };
            self.queue
                .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Car Render Pass"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.color_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.normal_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.car_vertex_buffer.slice(..));
            render_pass
                .set_index_buffer(self.car_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..self.car_index_count, 0, 0..1);
        }

        // Run compute shader to generate ASCII
        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Compute Pass"),
                timestamp_writes: None,
            });
            compute_pass.set_pipeline(&self.compute_pipeline);
            compute_pass.set_bind_group(0, &self.compute_bind_group, &[]);
            compute_pass.dispatch_workgroups((WIDTH + 7) / 8, (HEIGHT + 7) / 8, 1);
        }

        // Copy to staging buffer
        encoder.copy_buffer_to_buffer(
            &self.ascii_buffer,
            0,
            &self.ascii_staging_buffer,
            0,
            (WIDTH * HEIGHT) as u64,
        );

        self.queue.submit(Some(encoder.finish()));
    }

    async fn read_ascii(&self) -> Vec<u8> {
        let buffer_slice = self.ascii_staging_buffer.slice(..);
        let (tx, rx) = futures_intrusive::channel::shared::oneshot_channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.receive().await.unwrap().unwrap();

        let data = buffer_slice.get_mapped_range();
        let result = data.to_vec();
        drop(data);
        self.ascii_staging_buffer.unmap();
        result
    }
}

fn create_track_geometry() -> (Vec<Vertex>, Vec<u16>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let segments = 32;
    let outer_radius = 5.0;
    let inner_radius = 3.0;
    let track_width = 0.1;

    // Create circular track
    for i in 0..segments {
        let angle1 = (i as f32 / segments as f32) * 2.0 * std::f32::consts::PI;
        let angle2 = ((i + 1) as f32 / segments as f32) * 2.0 * std::f32::consts::PI;

        let x1_outer = angle1.cos() * outer_radius;
        let z1_outer = angle1.sin() * outer_radius;
        let x2_outer = angle2.cos() * outer_radius;
        let z2_outer = angle2.sin() * outer_radius;

        let x1_inner = angle1.cos() * inner_radius;
        let z1_inner = angle1.sin() * inner_radius;
        let x2_inner = angle2.cos() * inner_radius;
        let z2_inner = angle2.sin() * inner_radius;

        let base_idx = vertices.len() as u16;

        // Outer edge
        vertices.push(Vertex {
            position: [x1_outer, 0.0, z1_outer],
            normal: [0.0, 1.0, 0.0],
        });
        vertices.push(Vertex {
            position: [x2_outer, 0.0, z2_outer],
            normal: [0.0, 1.0, 0.0],
        });
        vertices.push(Vertex {
            position: [x2_outer, track_width, z2_outer],
            normal: [0.0, 1.0, 0.0],
        });
        vertices.push(Vertex {
            position: [x1_outer, track_width, z1_outer],
            normal: [0.0, 1.0, 0.0],
        });

        indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2]);
        indices.extend_from_slice(&[base_idx, base_idx + 2, base_idx + 3]);

        // Inner edge
        let base_idx = vertices.len() as u16;
        vertices.push(Vertex {
            position: [x1_inner, 0.0, z1_inner],
            normal: [0.0, 1.0, 0.0],
        });
        vertices.push(Vertex {
            position: [x2_inner, 0.0, z2_inner],
            normal: [0.0, 1.0, 0.0],
        });
        vertices.push(Vertex {
            position: [x2_inner, track_width, z2_inner],
            normal: [0.0, 1.0, 0.0],
        });
        vertices.push(Vertex {
            position: [x1_inner, track_width, z1_inner],
            normal: [0.0, 1.0, 0.0],
        });

        indices.extend_from_slice(&[base_idx, base_idx + 2, base_idx + 1]);
        indices.extend_from_slice(&[base_idx, base_idx + 3, base_idx + 2]);

        // Top surface
        let base_idx = vertices.len() as u16;
        vertices.push(Vertex {
            position: [x1_inner, track_width, z1_inner],
            normal: [0.0, 1.0, 0.0],
        });
        vertices.push(Vertex {
            position: [x2_inner, track_width, z2_inner],
            normal: [0.0, 1.0, 0.0],
        });
        vertices.push(Vertex {
            position: [x2_outer, track_width, z2_outer],
            normal: [0.0, 1.0, 0.0],
        });
        vertices.push(Vertex {
            position: [x1_outer, track_width, z1_outer],
            normal: [0.0, 1.0, 0.0],
        });

        indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2]);
        indices.extend_from_slice(&[base_idx, base_idx + 2, base_idx + 3]);
    }

    (vertices, indices)
}

fn create_car_geometry() -> (Vec<Vertex>, Vec<u16>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let w = 0.3;
    let h = 0.2;
    let d = 0.5;

    // Front face
    vertices.push(Vertex {
        position: [-w, 0.0, d],
        normal: [0.0, 0.0, 1.0],
    });
    vertices.push(Vertex {
        position: [w, 0.0, d],
        normal: [0.0, 0.0, 1.0],
    });
    vertices.push(Vertex {
        position: [w, h, d],
        normal: [0.0, 0.0, 1.0],
    });
    vertices.push(Vertex {
        position: [-w, h, d],
        normal: [0.0, 0.0, 1.0],
    });
    indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);

    // Back face
    let base = vertices.len() as u16;
    vertices.push(Vertex {
        position: [w, 0.0, -d],
        normal: [0.0, 0.0, -1.0],
    });
    vertices.push(Vertex {
        position: [-w, 0.0, -d],
        normal: [0.0, 0.0, -1.0],
    });
    vertices.push(Vertex {
        position: [-w, h, -d],
        normal: [0.0, 0.0, -1.0],
    });
    vertices.push(Vertex {
        position: [w, h, -d],
        normal: [0.0, 0.0, -1.0],
    });
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

    // Left face
    let base = vertices.len() as u16;
    vertices.push(Vertex {
        position: [-w, 0.0, -d],
        normal: [-1.0, 0.0, 0.0],
    });
    vertices.push(Vertex {
        position: [-w, 0.0, d],
        normal: [-1.0, 0.0, 0.0],
    });
    vertices.push(Vertex {
        position: [-w, h, d],
        normal: [-1.0, 0.0, 0.0],
    });
    vertices.push(Vertex {
        position: [-w, h, -d],
        normal: [-1.0, 0.0, 0.0],
    });
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

    // Right face
    let base = vertices.len() as u16;
    vertices.push(Vertex {
        position: [w, 0.0, d],
        normal: [1.0, 0.0, 0.0],
    });
    vertices.push(Vertex {
        position: [w, 0.0, -d],
        normal: [1.0, 0.0, 0.0],
    });
    vertices.push(Vertex {
        position: [w, h, -d],
        normal: [1.0, 0.0, 0.0],
    });
    vertices.push(Vertex {
        position: [w, h, d],
        normal: [1.0, 0.0, 0.0],
    });
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

    // Top face
    let base = vertices.len() as u16;
    vertices.push(Vertex {
        position: [-w, h, d],
        normal: [0.0, 1.0, 0.0],
    });
    vertices.push(Vertex {
        position: [w, h, d],
        normal: [0.0, 1.0, 0.0],
    });
    vertices.push(Vertex {
        position: [w, h, -d],
        normal: [0.0, 1.0, 0.0],
    });
    vertices.push(Vertex {
        position: [-w, h, -d],
        normal: [0.0, 1.0, 0.0],
    });
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

    // Bottom face
    let base = vertices.len() as u16;
    vertices.push(Vertex {
        position: [-w, 0.0, -d],
        normal: [0.0, -1.0, 0.0],
    });
    vertices.push(Vertex {
        position: [w, 0.0, -d],
        normal: [0.0, -1.0, 0.0],
    });
    vertices.push(Vertex {
        position: [w, 0.0, d],
        normal: [0.0, -1.0, 0.0],
    });
    vertices.push(Vertex {
        position: [-w, 0.0, d],
        normal: [0.0, -1.0, 0.0],
    });
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

    (vertices, indices)
}

fn main() {
    let renderer = pollster::block_on(Renderer::new());
    let mut car_state = CarState::new();

    enable_raw_mode().unwrap();
    let mut stdout = stdout();
    execute!(stdout, Hide, Clear(ClearType::All)).unwrap();

    let mut last_frame = Instant::now();

    loop {
        let frame_start = Instant::now();

        // Handle input
        let mut forward = false;
        let mut backward = false;
        let mut left = false;
        let mut right = false;
        let mut quit = false;

        while crossterm::event::poll(Duration::from_millis(0)).unwrap() {
            if let crossterm::event::Event::Key(key_event) = crossterm::event::read().unwrap() {
                use crossterm::event::{KeyCode, KeyEventKind};
                if key_event.kind == KeyEventKind::Press {
                    match key_event.code {
                        KeyCode::Up => forward = true,
                        KeyCode::Down => backward = true,
                        KeyCode::Left => left = true,
                        KeyCode::Right => right = true,
                        KeyCode::Char('q') | KeyCode::Esc => quit = true,
                        _ => {}
                    }
                }
            }
        }

        if quit {
            break;
        }

        // Update
        let dt = last_frame.elapsed().as_secs_f32();
        last_frame = Instant::now();
        car_state.update(dt, forward, backward, left, right);

        // Render
        renderer.render(&car_state);

        // Read ASCII buffer
        let ascii_data = pollster::block_on(renderer.read_ascii());

        // Display
        execute!(stdout, MoveTo(0, 0)).unwrap();
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let idx = (y * WIDTH + x) as usize;
                let glyph_id = ascii_data[idx] as usize;
                let ch = if glyph_id < ASCII_RAMP.len() {
                    ASCII_RAMP[glyph_id] as char
                } else {
                    ' '
                };
                print!("{}", ch);
            }
            println!();
        }
        stdout.flush().unwrap();

        // Frame rate limiting
        let elapsed = frame_start.elapsed();
        if elapsed < FRAME_TIME {
            std::thread::sleep(FRAME_TIME - elapsed);
        }
    }

    execute!(stdout, Show, Clear(ClearType::All)).unwrap();
    disable_raw_mode().unwrap();
}
