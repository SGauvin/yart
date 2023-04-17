use crossbeam::channel::unbounded;
use crossbeam::channel::{Receiver, Sender};
use std::borrow::Cow;
use std::num::{NonZeroU32, NonZeroU8};
use std::sync::Arc;

use egui_wgpu::{self, wgpu};

use bytemuck::{Pod, Zeroable};
use rand::Rng;
use wgpu::util::DeviceExt;

enum Message {}

#[repr(C)]
#[derive(Copy, Clone, Default, Debug, PartialEq, Pod, Zeroable)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Default, Debug, PartialEq, Pod, Zeroable)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Default, Debug, PartialEq, Pod, Zeroable)]
pub struct Material {
    pub albedo: Vec3,
    pub is_mirror: u32,
    pub unused_buffer: [u32; 0],
}

#[repr(C)]
#[derive(Copy, Clone, Default, Debug, PartialEq, Pod, Zeroable)]
pub struct Sphere {
    pub position: Vec3,
    pub radius: f32,
    pub mat: Material,
}

#[repr(C)]
#[derive(Copy, Clone, Default, Debug, PartialEq, Pod, Zeroable)]
pub struct Camera {
    pub position: Vec3,
    unused_buffer: [u32; 1],
}

#[repr(C)]
#[derive(Copy, Clone, Default, Debug, PartialEq, Pod, Zeroable)]
pub struct SceneInfo {
    pub camera: Camera,
    pub time: f32,
    pub sphere_count: u32,
    pub random_seed: f32,
    pub frame_count: u32,
}

pub struct Custom3d {
    scene_start: std::time::Instant,
    texture_width: u32,
    texture_height: u32,
    device: Arc<wgpu::Device>,
    random_gen: rand::rngs::ThreadRng,
    scene_info: SceneInfo,
    tx: Sender<Message>,
}

impl Custom3d {
    pub fn new<'a>(cc: &'a eframe::CreationContext<'a>) -> Option<Self> {
        // Get the WGPU render state from the eframe creation context. This can also be retrieved
        // from `eframe::Frame` when you don't have a `CreationContext` available.
        let render_state = cc.wgpu_render_state.as_ref()?;
        let device = &render_state.device;

        let texture_width = 800;
        let texture_height = 800;

        let raytracing_resources =
            Self::create_raytracing_pipeline(device, texture_width, texture_height);
        let triangle_resources =
            Self::create_screen_pipeline(device, &raytracing_resources.storage_texture_view);
        let (tx, rx) = unbounded();
        let resources = Resources {
            raytracing_resources,
            screen_resources: triangle_resources,
            rx,
        };

        // Because the graphics pipeline must have the same lifetime as the egui render pass,
        // instead of storing the pipeline in our `Custom3D` struct, we insert it into the
        // `paint_callback_resources` type map, which is stored alongside the render pass.
        render_state
            .renderer
            .write()
            .paint_callback_resources
            .insert(resources);

        Some(Self {
            scene_start: std::time::Instant::now(),
            texture_width,
            texture_height,
            device: device.clone(),
            scene_info: Default::default(),
            random_gen: rand::thread_rng(),
            tx,
        })
    }

    pub fn rebuild_pipeline(
        &mut self,
        width: u32,
        height: u32,
        render_state: &egui_wgpu::RenderState,
    ) {
        let raytracing_resources = Self::create_raytracing_pipeline(&self.device, width, height);

        let triangle_resources =
            Self::create_screen_pipeline(&self.device, &raytracing_resources.storage_texture_view);

        let old_resources = render_state
            .renderer
            .write()
            .paint_callback_resources
            .remove::<Resources>()
            .unwrap();

        let Resources { rx, .. } = old_resources;

        let resources = Resources {
            raytracing_resources,
            screen_resources: triangle_resources,
            rx,
        };

        render_state
            .renderer
            .write()
            .paint_callback_resources
            .insert(resources);

        self.texture_width = width;
        self.texture_height = height;
    }

    fn create_raytracing_pipeline(
        device: &wgpu::Device,
        texture_width: u32,
        texture_height: u32,
    ) -> RaytracingRenderResources {
        let scene_info_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[SceneInfo::default()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let sphere_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: std::mem::size_of::<Sphere>() as u64 * 16,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let storage_texture_descriptor =
            Self::get_storage_texture_descriptor_from_size(texture_width, texture_height);
        let storage_texture = device.create_texture(&storage_texture_descriptor);
        let storage_texture_view =
            storage_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let progressive_rendering_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (get_bytes_per_row_from_width(texture_width) * texture_height) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false }, // True?
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
            label: None,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            label: None,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&storage_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: scene_info_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: sphere_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: progressive_rendering_buffer.as_entire_binding(),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &[&bind_group_layout],
            ..Default::default()
        });

        let cs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                "./shaders/raytracer_kernel.wgsl"
            ))),
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            module: &cs_module,
            entry_point: "main",
        });

        RaytracingRenderResources {
            bind_group,
            pipeline,
            storage_texture_view,
            storage_texture,
            progressive_rendering_buffer,
            scene_info_buffer,
            sphere_buffer,
        }
    }

    fn create_screen_pipeline(
        device: &wgpu::Device,
        color_buffer_view: &wgpu::TextureView,
    ) -> ScreenRenderResources {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mipmap_filter: wgpu::FilterMode::Nearest,
            anisotropy_clamp: NonZeroU8::new(1),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(color_buffer_view),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_group_layout],
            ..Default::default()
        });

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                "./shaders/screen_shader.wgsl"
            ))),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: "vert_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: "frag_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Bgra8Unorm, // ??
                    blend: None,
                    write_mask: wgpu::ColorWrites::default(),
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
        });

        ScreenRenderResources {
            pipeline,
            bind_group,
        }
    }

    fn get_storage_texture_descriptor_from_size<'a>(
        width: u32,
        height: u32,
    ) -> wgpu::TextureDescriptor<'a> {
        wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            label: None,
            view_formats: &[],
        }
    }

    pub fn custom_painting(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        let size_to_allocate = {
            let available_size = ui.available_size();
            let texture_aspect_ratio = (self.texture_width as f32) / (self.texture_height as f32);

            let fit_to_x_size =
                egui::Vec2::new(available_size.x, available_size.x / texture_aspect_ratio);
            let fit_to_y_size =
                egui::Vec2::new(available_size.y * texture_aspect_ratio, available_size.y);

            if fit_to_x_size.y > available_size.y {
                fit_to_y_size
            } else {
                fit_to_x_size
            }
        };

        if size_to_allocate.x < 1.0 || size_to_allocate.y < 1.0 {
            return;
        }

        if size_to_allocate.x as u32 != self.texture_width
            || size_to_allocate.y as u32 != self.texture_height
        {
            self.rebuild_pipeline(
                size_to_allocate.x as u32,
                size_to_allocate.y as u32,
                frame.wgpu_render_state().unwrap(),
            );
            self.scene_info.frame_count = 0;
        }

        let (rect, _response) = ui.allocate_exact_size(size_to_allocate, egui::Sense::drag());

        self.scene_info.random_seed = self.random_gen.gen();
        self.scene_info.time = self.scene_start.elapsed().as_secs_f32();
        self.scene_info.frame_count += 1;

        let cb = egui_wgpu::CallbackFn::new()
            .prepare({
                let texture_width = self.texture_width;
                let texture_height = self.texture_height;
                let scene_info = self.scene_info;
                move |device, queue, encoder, paint_callback_resources| {
                    let resources: &Resources = paint_callback_resources.get().unwrap();
                    resources.prepare(
                        device,
                        queue,
                        encoder,
                        texture_width,
                        texture_height,
                        scene_info,
                    );
                    Vec::with_capacity(0)
                }
            })
            .paint(move |_info, render_pass, paint_callback_resources| {
                let resources: &Resources = paint_callback_resources.get().unwrap();
                resources.paint(render_pass);
            });

        let callback = egui::PaintCallback {
            rect,
            callback: Arc::new(cb),
        };

        ui.painter().add(callback);
    }
}

struct ScreenRenderResources {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
}

struct RaytracingRenderResources {
    pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
    storage_texture_view: wgpu::TextureView,
    storage_texture: wgpu::Texture,
    progressive_rendering_buffer: wgpu::Buffer,
    scene_info_buffer: wgpu::Buffer,
    sphere_buffer: wgpu::Buffer,
}

struct Resources {
    raytracing_resources: RaytracingRenderResources,
    screen_resources: ScreenRenderResources,
    rx: Receiver<Message>,
}

impl Resources {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        texture_width: u32,
        texture_height: u32,
        mut scene_info: SceneInfo,
    ) {
        let spheres = [
            Sphere {
                position: Vec3 {
                    x: 10.0,
                    y: 0.0,
                    z: 1.0,
                },
                radius: 1.0,
                mat: Material {
                    albedo: Vec3 {
                        x: 0.87,
                        y: 0.87,
                        z: 0.87,
                    },
                    is_mirror: 1,
                    unused_buffer: Default::default(),
                },
            },
            Sphere {
                position: Vec3 {
                    x: 7.3,
                    y: -1.2,
                    z: 1.02,
                },
                radius: 1.0,
                mat: Material {
                    albedo: Vec3 {
                        x: 0.87,
                        y: 0.87,
                        z: 0.87,
                    },
                    is_mirror: 1,
                    unused_buffer: Default::default(),
                },
            },
            Sphere {
                position: Vec3 {
                    x: 9.0,
                    y: 2.2,
                    z: 1.03,
                },
                radius: 1.0,
                mat: Material {
                    albedo: Vec3 {
                        x: 0.97,
                        y: 0.97,
                        z: 0.97,
                    },
                    is_mirror: 0,
                    unused_buffer: Default::default(),
                },
            },
            Sphere {
                position: Vec3 {
                    x: 10.0,
                    y: 0.0,
                    z: 102.0,
                },
                radius: 100.0,
                mat: Material {
                    albedo: Vec3 {
                        x: 1.0,
                        y: 0.5,
                        z: 0.5,
                    },
                    is_mirror: 0,
                    unused_buffer: Default::default(),
                },
            },
            // Sphere {
            //     position: Vec3 {
            //         x: 10.0,
            //         y: 100_004.0,
            //         z: 0.0,
            //     },
            //     radius: 100_000.0,
            //     mat: Material {
            //         albedo: Vec3 {
            //             x: 0.7,
            //             y: 0.7,
            //             z: 1.0,
            //         },
            //         is_mirror: 0,
            //         unused_buffer: Default::default(),
            //     },
            // },
            // Sphere {
            //     position: Vec3 {
            //         x: 10.0,
            //         y: -100_004.0,
            //         z: 0.0,
            //     },
            //     radius: 100_000.0,
            //     mat: Material {
            //         albedo: Vec3 {
            //             x: 1.0,
            //             y: 0.7,
            //             z: 0.7,
            //         },
            //         is_mirror: 0,
            //         unused_buffer: Default::default(),
            //     },
            // },
            // Sphere {
            //     position: Vec3 {
            //         x: 100_014.0,
            //         y: 0.0,
            //         z: 0.0,
            //     },
            //     radius: 100_000.0,
            //     mat: Material {
            //         albedo: Vec3 {
            //             x: 0.7,
            //             y: 0.7,
            //             z: 0.7,
            //         },
            //         is_mirror: 0,
            //         unused_buffer: Default::default(),
            //     },
            // },
            // Sphere {
            //     position: Vec3 {
            //         x: 10.0,
            //         y: 0.0,
            //         z: -100_004.0,
            //     },
            //     radius: 100_000.0,
            //     mat: Material {
            //         albedo: Vec3 {
            //             x: 0.2,
            //             y: 0.2,
            //             z: 0.2,
            //         },
            //         is_mirror: 0,
            //         unused_buffer: Default::default(),
            //     },
            // },
        ];

        scene_info.sphere_count = spheres.len() as u32;
        scene_info.camera.position.x = 2.0;

        self.raytracing_resources.prepare(
            device,
            queue,
            encoder,
            (texture_width, texture_height),
            scene_info,
            &spheres,
        );
    }

    fn paint<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.screen_resources.paint(render_pass);
    }
}

impl RaytracingRenderResources {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        texture_size: (u32, u32),
        scene_info: SceneInfo,
        spheres: &[Sphere],
    ) {
        {
            let mut raytracing_pass = encoder.begin_compute_pass(&Default::default());
            queue.write_buffer(
                &self.scene_info_buffer,
                0,
                bytemuck::cast_slice(&[scene_info]),
            );
            queue.write_buffer(&self.sphere_buffer, 0, bytemuck::cast_slice(spheres));
            raytracing_pass.set_pipeline(&self.pipeline);
            raytracing_pass.set_bind_group(0, &self.bind_group, &[]);
            raytracing_pass.dispatch_workgroups(texture_size.0, texture_size.1, 1);
        }
        {
            let source = wgpu::ImageCopyTexture {
                texture: &self.storage_texture,
                aspect: wgpu::TextureAspect::All,
                mip_level: 0,
                origin: Default::default(),
            };

            let destination = wgpu::ImageCopyBuffer {
                buffer: &self.progressive_rendering_buffer,
                layout: wgpu::ImageDataLayout {
                    bytes_per_row: NonZeroU32::new(get_bytes_per_row_from_width(texture_size.0)),
                    offset: 0,
                    rows_per_image: None,
                },
            };

            encoder.copy_texture_to_buffer(
                source,
                destination,
                wgpu::Extent3d {
                    width: texture_size.0,
                    height: texture_size.1,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
}

impl ScreenRenderResources {
    fn paint<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.draw(0..6, 0..1);
    }
}

fn get_bytes_per_row_from_width(width: u32) -> u32 {
    let unpadded_bytes_per_row = 8 * width; // Rgba16Float
    unpadded_bytes_per_row
        + (wgpu::COPY_BYTES_PER_ROW_ALIGNMENT
            - (unpadded_bytes_per_row % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT))
}
