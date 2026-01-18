use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use cef::{sys::HWND, wrap_client, wrap_load_handler, wrap_render_handler, *};

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

    if is_browser_process {
        assert!(exit_code == -1, "cannot execute browser process");
    } else {
        let process_type = CefString::from(&cmd.switch_value(Some(&switch)));
        println!("launch process {process_type}");
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
            sender: mpsc::Sender<RenderedFrame>,
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
                let _ = self.sender.send(RenderedFrame {
                    width: width as usize,
                    height: height as usize,
                    rgba,
                });
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

    let render_handler = TestRenderHandler::new(options.width, options.height, sent.clone(), tx);
    let load_handler = TestLoadHandler::new();
    let mut client = TestClient::new(render_handler, load_handler);

    let parent: WindowHandle = HWND::default();
    let window_info = WindowInfo::default().set_as_windowless(parent);
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

    let current = Instant::now();
    let mut num_rendered = 0;
    let mut last_frame = None;
    loop {
        do_message_loop_work();
        if let Ok(frame) = rx.try_recv() {
            last_frame = Some(frame);
            num_rendered += 1;
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
    let duration = after.duration_since(current);
    image::RgbaImage::from_raw(
        last_frame.as_ref().unwrap().width as u32,
        last_frame.as_ref().unwrap().height as u32,
        last_frame.as_ref().unwrap().rgba.clone(),
    )
    .unwrap()
    .save("output.png")?;
    println!("Rendered {} frames in {:?}", num_rendered, duration);
    println!(
        "Average frame time: {:?}",
        duration.checked_div(num_rendered).unwrap_or_default()
    );
    Ok(())
}
