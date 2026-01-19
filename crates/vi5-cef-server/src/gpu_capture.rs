use std::sync::mpsc;

use cef::{AcceleratedPaintInfo, osr_texture_import::SharedTextureHandle};

pub struct GpuCapture {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl GpuCapture {
    pub fn new() -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12,
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|err| anyhow::anyhow!("wgpu adapter not found: {err:?}"))?;
        let adapter_info = adapter.get_info();
        if adapter_info.backend != wgpu::Backend::Dx12 {
            anyhow::bail!(
                "wgpu backend {:?} is not supported for CEF shared textures",
                adapter_info.backend
            );
        }
        tracing::info!("Using wgpu backend: {:?}", adapter_info.backend);
        let device_desc = wgpu::DeviceDescriptor {
            label: Some("cef-osr-device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&device_desc))?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cef-osr-blit-shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!(
                "./shader.wgsl"
            ))),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cef-osr-bind-group-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cef-osr-pipeline-layout"),
            bind_group_layouts: &[&bind_group_layout],
            ..Default::default()
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cef-osr-blit-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
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
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("cef-osr-sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        Ok(Self {
            device,
            queue,
            pipeline,
            bind_group_layout,
            sampler,
        })
    }

    pub fn capture(
        &self,
        info: &AcceleratedPaintInfo,
        on_accelerated_paint: fn(
            buffer: &wgpu::BufferView,
            width: usize,
            height: usize,
            bytes_per_row: usize,
        ),
    ) -> anyhow::Result<()> {
        let width = info.extra.coded_size.width;
        let height = info.extra.coded_size.height;
        if width <= 0 || height <= 0 {
            anyhow::bail!("invalid accelerated texture size");
        }

        let texture = SharedTextureHandle::new(info).import_texture(&self.device)?;
        let rgba_texture = self.blit_to_rgba(&texture, width as u32, height as u32)?;
        self.readback_texture(
            &rgba_texture,
            width as u32,
            height as u32,
            on_accelerated_paint,
        )
    }

    fn blit_to_rgba(
        &self,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
    ) -> anyhow::Result<wgpu::Texture> {
        let src_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let dst_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cef-osr-rgba-texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let dst_view = dst_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cef-osr-bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&src_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("cef-osr-blit-encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cef-osr-blit-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &dst_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
        Ok(dst_texture)
    }

    fn readback_texture(
        &self,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,

        on_accelerated_paint: fn(
            buffer: &wgpu::BufferView,
            width: usize,
            height: usize,
            bytes_per_row: usize,
        ),
    ) -> anyhow::Result<()> {
        let bytes_per_pixel = 4;
        let bytes_per_row = align_to(width * bytes_per_pixel, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer_size = bytes_per_row as u64 * height as u64;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cef-osr-readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("cef-osr-readback-encoder"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = buffer.slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })?;
        match rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(err)) => anyhow::bail!("readback map failed: {err:?}"),
            Err(err) => anyhow::bail!("readback map dropped: {err}"),
        }
        let data = slice.get_mapped_range();
        on_accelerated_paint(
            &data,
            width as usize,
            height as usize,
            bytes_per_row as usize,
        );
        drop(data);
        buffer.unmap();
        Ok(())
    }
}

fn align_to(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}
