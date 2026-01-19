mod cef_app;
mod gpu_capture;
mod handlers;
mod render_loop;
mod types;

use std::sync::{Arc, atomic::AtomicBool, mpsc};

use log::info;

use crate::cef_app::{
    build_render_options, build_settings, create_browser, initialize_cef, prepare_process,
};
use crate::gpu_capture::GpuCapture;
use crate::handlers::create_client;
use crate::render_loop::run_render_loop;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);

    let args = cef::args::Args::new();
    let options = build_render_options();
    let is_browser_process = prepare_process(&args)?;
    if !is_browser_process {
        return Ok(());
    }

    let settings = build_settings();
    let _shutdown_guard = initialize_cef(&args, &settings)?;

    let (tx, rx) = mpsc::channel();
    let sent = Arc::new(AtomicBool::new(false));
    let loaded = Arc::new(AtomicBool::new(false));
    let gpu = Arc::new(GpuCapture::new()?);

    let url = "http://localhost:5173/";
    info!("create browser for {url}");
    let mut client = create_client(&options, sent, loaded.clone(), gpu, tx);
    let browser = create_browser(&mut client, url)?;
    run_render_loop(&browser, &rx, &loaded);
    Ok(())
}

