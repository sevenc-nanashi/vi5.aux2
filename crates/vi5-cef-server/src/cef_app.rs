use log::info;

use cef::{
    execute_process, Browser, BrowserSettings, CefString, ImplBrowser, ImplBrowserHost,
    ImplCommandLine, ImplFrame, Settings, WindowHandle, WindowInfo,
};

use crate::handlers::ShutdownGuard;
use crate::types::{RenderError, RenderOptions};

pub fn build_render_options() -> RenderOptions {
    RenderOptions {
        width: 2048,
        height: 2048,
    }
}

pub fn prepare_process(args: &cef::args::Args) -> anyhow::Result<bool> {
    let cmd = args.as_cmd_line().unwrap();
    cmd.append_switch(Some(&CefString::from(
        "disable-background-timer-throttling",
    )));
    cmd.append_switch(Some(&CefString::from("disable-renderer-backgrounding")));

    let switch = CefString::from("type");
    let is_browser_process = cmd.has_switch(Some(&switch)) != 1;
    let exit_code = execute_process(Some(args.as_main_args()), None, std::ptr::null_mut());
    if exit_code >= 0 {
        std::process::exit(exit_code);
    }

    let process_id = std::process::id();
    if is_browser_process {
        info!("launch browser process {process_id}");
    } else {
        let process_type = CefString::from(&cmd.switch_value(Some(&switch)));
        info!(
            "launch non-browser process {process_id} of type {:?}",
            process_type
        );
        // non-browser process does not initialize cef
        return Ok(false);
    }

    Ok(true)
}

pub fn build_settings() -> Settings {
    let mut settings = Settings {
        no_sandbox: 1,
        windowless_rendering_enabled: 1,
        external_message_pump: 1,
        ..Default::default()
    };
    if let Some(exe_path) = std::env::current_exe()
        .ok()
        .map(|p| p.to_string_lossy().to_string())
    {
        settings.browser_subprocess_path = CefString::from(exe_path.as_str());
    }
    settings
}

pub fn initialize_cef(
    args: &cef::args::Args,
    settings: &Settings,
) -> anyhow::Result<ShutdownGuard> {
    let initialized = cef::initialize(
        Some(args.as_main_args()),
        Some(settings),
        None,
        std::ptr::null_mut(),
    );
    if initialized == 0 {
        anyhow::bail!(RenderError::InitializeFailed);
    }
    Ok(ShutdownGuard)
}

pub fn create_browser(client: &mut cef::Client, url: &str) -> Result<Browser, RenderError> {
    let parent: WindowHandle = cef::sys::HWND::default();
    let mut window_info = WindowInfo::default().set_as_windowless(parent);
    window_info.shared_texture_enabled = 1;
    let browser_settings = BrowserSettings {
        windowless_frame_rate: 60,
        background_color: 0x00000000,
        ..Default::default()
    };

    let browser = cef::browser_host_create_browser_sync(
        Some(&window_info),
        Some(client),
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

    Ok(browser)
}
