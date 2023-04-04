use std::num::NonZeroU8;
use std::sync::Arc;
use std::borrow::Cow;

use egui_wgpu::{self, wgpu};

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

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
pub struct Camera {
    pub position: Vec3,
    unused_buffer: [u32; 1],
}

#[repr(C)]
#[derive(Copy, Clone, Default, Debug, PartialEq, Pod, Zeroable)]
pub struct SceneInfo {
    pub camera: Camera,
}

pub struct Custom3d {
    texture_width: u32,
    texture_height: u32,
    device: Arc<wgpu::Device>,
    scene_info: SceneInfo,
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
        let triangle_resources = Self::create_screen_pipeline(
            device,
            &raytracing_resources.sampler,
            &raytracing_resources.color_buffer_view,
        );
        let resources = Resources {
            raytracing_resources,
            triangle_resources,
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
            texture_width,
            texture_height,
            device: device.clone(),
            scene_info: Default::default(),
        })
    }

    pub fn resize(&mut self, width: u32, height: u32, render_state: &egui_wgpu::RenderState) {
        let raytracing_resources = Self::create_raytracing_pipeline(&self.device, width, height);

        let triangle_resources = Self::create_screen_pipeline(
            &self.device,
            &raytracing_resources.sampler,
            &raytracing_resources.color_buffer_view,
        );

        let resources = Resources {
            raytracing_resources,
            triangle_resources,
        };

        render_state
            .renderer
            .write()
            .paint_callback_resources
            .remove::<Resources>();

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

        let texture_descriptor =
            Self::get_texture_descriptor_from_size(texture_width, texture_height);
        let color_buffer = device.create_texture(&texture_descriptor);
        let color_buffer_view = color_buffer.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mipmap_filter: wgpu::FilterMode::Nearest,
            anisotropy_clamp: NonZeroU8::new(1),
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
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
            ],
            label: None,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            label: None,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color_buffer_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: scene_info_buffer.as_entire_binding(),
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
            sampler,
            color_buffer_view,
            scene_info_buffer,
        }
    }

    fn create_screen_pipeline(
        device: &wgpu::Device,
        sampler: &wgpu::Sampler,
        color_buffer_view: &wgpu::TextureView,
    ) -> ScreenRenderResources {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(sampler),
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
                    format: wgpu::TextureFormat::Bgra8Unorm,
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

    fn get_texture_descriptor_from_size<'a>(
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
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
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
            self.resize(
                size_to_allocate.x as u32,
                size_to_allocate.y as u32,
                frame.wgpu_render_state().unwrap(),
            );
        }

        let (rect, _response) = ui.allocate_exact_size(size_to_allocate, egui::Sense::drag());

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
                    Vec::new()
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
    sampler: wgpu::Sampler,
    bind_group: wgpu::BindGroup,
    color_buffer_view: wgpu::TextureView,
    scene_info_buffer: wgpu::Buffer,
}

struct Resources {
    raytracing_resources: RaytracingRenderResources,
    triangle_resources: ScreenRenderResources,
}

impl Resources {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        texture_width: u32,
        texture_height: u32,
        scene_info: SceneInfo,
    ) {
        self.raytracing_resources.prepare(
            device,
            queue,
            encoder,
            texture_width,
            texture_height,
            scene_info,
        );
    }

    fn paint<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.triangle_resources.paint(render_pass);
    }
}

impl RaytracingRenderResources {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        texture_width: u32,
        texture_height: u32,
        scene_info: SceneInfo,
    ) {
        let mut raytracing_pass = encoder.begin_compute_pass(&Default::default());
        queue.write_buffer(
            &self.scene_info_buffer,
            0,
            bytemuck::cast_slice(&[scene_info]),
        );
        raytracing_pass.set_pipeline(&self.pipeline);
        raytracing_pass.set_bind_group(0, &self.bind_group, &[]);
        raytracing_pass.dispatch_workgroups(texture_width, texture_height, 1);
    }
}

impl ScreenRenderResources {
    fn paint<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.draw(0..6, 0..1);
    }
}
