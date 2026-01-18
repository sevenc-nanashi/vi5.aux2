use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use cef::{
    osr_texture_import::SharedTextureHandle, sys::HWND, wrap_client, wrap_load_handler,
    wrap_render_handler, *,
};

#[derive(Debug)]
pub struct RenderOptions {
    pub width: i32,
    pub height: i32,
    pub timeout: Duration,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            width: 640,
            height: 360,
            timeout: Duration::from_secs(10),
        }
    }
}

#[derive(Debug)]
pub struct RenderedFrame {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

#[derive(Debug)]
pub enum RenderMessage {
    Software(RenderedFrame),
    Accelerated(RenderedFrame),
}

#[derive(Debug)]
pub enum RenderError {
    InitializeFailed,
    BrowserCreateFailed,
    Timeout,
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::InitializeFailed => write!(f, "CEF initialization failed"),
            RenderError::BrowserCreateFailed => write!(f, "CEF browser creation failed"),
            RenderError::Timeout => write!(f, "CEF rendering timed out"),
        }
    }
}

impl std::error::Error for RenderError {}

struct GpuCapture {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl GpuCapture {
    fn new() -> anyhow::Result<Self> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|_| anyhow::anyhow!("wgpu adapter not found"))?;
        let device_desc = wgpu::DeviceDescriptor {
            label: Some("cef-osr-device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&device_desc))?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cef-osr-blit-shader"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VsOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var uvs = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(2.0, 1.0),
        vec2<f32>(0.0, -1.0),
    );
    var out: VsOut;
    out.pos = vec4<f32>(positions[vi], 0.0, 1.0);
    out.uv = uvs[vi];
    return out;
}

@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(src_tex, src_sampler, in.uv);
}
"#
                .into(),
            ),
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
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
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

    fn capture(&self, info: &AcceleratedPaintInfo) -> anyhow::Result<RenderedFrame> {
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
            ColorType::RGBA_8888,
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
        format: ColorType,
    ) -> anyhow::Result<RenderedFrame> {
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
        });
        match rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(err)) => anyhow::bail!("readback map failed: {err:?}"),
            Err(err) => anyhow::bail!("readback map dropped: {err}"),
        }
        let data = slice.get_mapped_range();
        let mut rgba = vec![0u8; (width * height * bytes_per_pixel) as usize];
        for y in 0..height {
            let src_offset = (y * bytes_per_row) as usize;
            let dst_offset = (y * width * bytes_per_pixel) as usize;
            let src_row = &data[src_offset..src_offset + (width * bytes_per_pixel) as usize];
            let dst_row = &mut rgba[dst_offset..dst_offset + (width * bytes_per_pixel) as usize];
            if format == ColorType::BGRA_8888 {
                for (src_px, dst_px) in src_row.chunks_exact(4).zip(dst_row.chunks_exact_mut(4)) {
                    dst_px[0] = src_px[2];
                    dst_px[1] = src_px[1];
                    dst_px[2] = src_px[0];
                    dst_px[3] = src_px[3];
                }
            } else {
                dst_row.copy_from_slice(src_row);
            }
        }
        drop(data);
        buffer.unmap();
        Ok(RenderedFrame {
            width: width as usize,
            height: height as usize,
            rgba,
        })
    }
}

fn align_to(value: u32, alignment: u32) -> u32 {
    (value + alignment - 1) / alignment * alignment
}

fn main() -> anyhow::Result<()> {
    let _ = cef::api_hash(sys::CEF_API_VERSION_LAST, 0);

    let args = cef::args::Args::new();
    let cmd = args.as_cmd_line().unwrap();
    cmd.append_switch(Some(&CefString::from(
        "disable-background-timer-throttling",
    )));
    cmd.append_switch(Some(&CefString::from("disable-renderer-backgrounding")));

    let options = RenderOptions {
        width: 1024,
        height: 1024,
        timeout: Duration::from_secs(10),
    };

    let switch = CefString::from("type");
    let is_browser_process = cmd.has_switch(Some(&switch)) != 1;
    let exit_code = execute_process(Some(args.as_main_args()), None, std::ptr::null_mut());
    if exit_code >= 0 {
        std::process::exit(exit_code);
    }

    let process_id = std::process::id();
    if is_browser_process {
        assert!(exit_code == -1, "cannot execute browser process");
        println!("launch browser process {process_id}");
    } else {
        let process_type = CefString::from(&cmd.switch_value(Some(&switch)));
        println!(
            "launch non-browser process {process_id} of type {:?}",
            process_type
        );
        assert!(exit_code >= 0, "cannot execute non-browser process");
        // non-browser process does not initialize cef
        return Ok(());
    }

    let mut settings = Settings::default();
    settings.no_sandbox = 1;
    settings.windowless_rendering_enabled = 1;
    settings.external_message_pump = 1;
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_str) = exe_path.to_str() {
            settings.browser_subprocess_path = CefString::from(exe_str);
        }
    }

    let initialized = initialize(
        Some(&args.as_main_args()),
        Some(&settings),
        None,
        std::ptr::null_mut(),
    );
    if initialized == 0 {
        anyhow::bail!(RenderError::InitializeFailed);
    }

    struct ShutdownGuard;
    impl Drop for ShutdownGuard {
        fn drop(&mut self) {
            shutdown();
        }
    }
    let _shutdown_guard = ShutdownGuard;

    let (tx, rx) = mpsc::channel();
    let loaded = Arc::new(AtomicBool::new(false));
    let sent = Arc::new(AtomicBool::new(false));
    let gpu = Arc::new(GpuCapture::new()?);

    wrap_load_handler! {
        struct TestLoadHandler {
        }

        impl LoadHandler {
            fn on_load_end(
                &self,
                _browser: Option<&mut Browser>,
                frame: Option<&mut Frame>,
                _http_status_code: ::std::os::raw::c_int,
            ) {
            }
        }
    }

    wrap_render_handler! {
        struct TestRenderHandler {
            width: i32,
            height: i32,
            sent: Arc<AtomicBool>,
            gpu: Arc<GpuCapture>,
            sender: mpsc::Sender<RenderMessage>,
        }

        impl RenderHandler {
            fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
                if let Some(rect) = rect {
                    rect.x = 0;
                    rect.y = 0;
                    rect.width = self.width;
                    rect.height = self.height;
                }
            }

            fn on_paint(
                &self,
                _browser: Option<&mut Browser>,
                type_: PaintElementType,
                _dirty_rects: Option<&[Rect]>,
                buffer: *const u8,
                width: ::std::os::raw::c_int,
                height: ::std::os::raw::c_int,
            ) {
                if type_ != PaintElementType::VIEW {
                    return;
                }
                if buffer.is_null() || width <= 0 || height <= 0 {
                    return;
                }
                let size = width as usize * height as usize * 4;
                let src = unsafe { std::slice::from_raw_parts(buffer, size) };
                let mut rgba = Vec::with_capacity(size);
                for pixel in src.chunks_exact(4) {
                    rgba.push(pixel[2]);
                    rgba.push(pixel[1]);
                    rgba.push(pixel[0]);
                    rgba.push(pixel[3]);
                }
                let _ = self.sender.send(RenderMessage::Software(RenderedFrame {
                    width: width as usize,
                    height: height as usize,
                    rgba,
                }));
            }

            fn on_accelerated_paint(
                &self,
                _browser: Option<&mut Browser>,
                type_: PaintElementType,
                _dirty_rects: Option<&[Rect]>,
                info: Option<&AcceleratedPaintInfo>,
            ) {
                if type_ != PaintElementType::VIEW {
                    return;
                }
                if info.is_none() {
                    return;
                }
                let info = info.unwrap();
                match self.gpu.capture(info) {
                    Ok(frame) => {
                        let _ = self.sender.send(RenderMessage::Accelerated(frame));
                    }
                    Err(err) => {
                        eprintln!("Failed to read accelerated frame: {err}");
                    }
                }
            }
        }
    }

    wrap_client! {
        struct TestClient {
            render_handler: RenderHandler,
            load_handler: LoadHandler,
        }

        impl Client {
            fn render_handler(&self) -> Option<RenderHandler> {
                Some(self.render_handler.clone())
            }

            fn load_handler(&self) -> Option<LoadHandler> {
                Some(self.load_handler.clone())
            }
        }
    }

    let render_handler =
        TestRenderHandler::new(options.width, options.height, sent.clone(), gpu, tx);
    let load_handler = TestLoadHandler::new();
    let mut client = TestClient::new(render_handler, load_handler);

    let parent: WindowHandle = HWND::default();
    let mut window_info = WindowInfo::default().set_as_windowless(parent);
    window_info.shared_texture_enabled = 1;
    let mut browser_settings = BrowserSettings::default();
    browser_settings.windowless_frame_rate = 60;
    browser_settings.background_color = 0xFFFFFFFF;

    let url = "https://editor.p5js.org/sevenc-nanashi/sketches/S_0TUFOd5";
    let browser = browser_host_create_browser_sync(
        Some(&window_info),
        Some(&mut client),
        Some(&CefString::from(url)),
        Some(&browser_settings),
        None,
        None,
    )
    .ok_or(RenderError::BrowserCreateFailed)?;
    browser
        .host()
        .unwrap()
        .invalidate(cef::PaintElementType::VIEW);

    if let Some(host) = browser.host() {
        host.was_resized();
    }

    if let Some(frame) = browser.main_frame() {
        frame.load_url(Some(&CefString::from(url)));
    }

    let mut first_frame = Instant::now();
    let mut num_rendered = 0;
    let mut accelerated_frames = 0;
    let mut last_frame: Option<RenderedFrame> = None;
    loop {
        do_message_loop_work();
        if let Ok(message) = rx.try_recv() {
            match message {
                RenderMessage::Software(frame) => {
                    if last_frame.is_none() {
                        first_frame = Instant::now();
                    }
                    last_frame = Some(frame);
                    println!("Software frame received");
                    num_rendered += 1;
                }
                RenderMessage::Accelerated(frame) => {
                    if last_frame.is_none() {
                        first_frame = Instant::now();
                    }
                    last_frame = Some(frame);
                    accelerated_frames += 1;
                    println!("Accelerated frame received");
                    num_rendered += 1;
                }
            }
            if num_rendered >= 100 {
                break;
            }
        }

        browser
            .host()
            .unwrap()
            .invalidate(cef::PaintElementType::VIEW);
    }

    let after = Instant::now();
    let duration = after.duration_since(first_frame);
    if let Some(frame) = last_frame.as_ref() {
        image::RgbaImage::from_raw(frame.width as u32, frame.height as u32, frame.rgba.clone())
            .unwrap()
            .save("output.png")?;
    }
    println!("Rendered {} frames in {:?}", num_rendered, duration);
    if accelerated_frames > 0 {
        println!("Accelerated frames: {}", accelerated_frames);
    }
    println!(
        "Average frame time: {:?}",
        duration.checked_div(num_rendered).unwrap_or_default()
    );
    Ok(())
}
