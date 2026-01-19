mod cef_app;
mod gpu_capture;
mod handlers;
mod protocol;
mod render_loop;
mod server;
mod types;

use std::sync::Arc;

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::cef_app::{
    build_render_options, build_settings, create_browser, initialize_cef, prepare_process,
};
use crate::gpu_capture::GpuCapture;
use crate::handlers::create_client;
use crate::render_loop::RenderLoop;

fn main() -> anyhow::Result<()> {
    // env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
    //     .target(env_logger::Target::Stderr)
    //     .init();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_thread_ids(true)
                .with_thread_names(true)
                .with_writer(std::io::stderr),
        )
        .with(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing_subscriber::filter::LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    color_backtrace::install();

    let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);

    let args = cef::args::Args::new();
    let options = build_render_options();
    let is_browser_process = prepare_process(&args)?;
    if !is_browser_process {
        tracing::info!("Initialized as a secondary process, exiting main.");
        return Ok(());
    }

    let settings = build_settings();
    let _shutdown_guard = initialize_cef(&args, &settings)?;

    let gpu = Arc::new(GpuCapture::new()?);

    let mut client = create_client(
        &options,
        gpu,
        crate::render_loop::on_paint,
        crate::render_loop::on_accelerated_paint,
    );
    let browser = create_browser(&mut client)?;
    let render_loop = RenderLoop::new(browser);
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(main_server(render_loop))?;
    Ok(())
}

pub async fn main_server(render_loop: RenderLoop) -> anyhow::Result<()> {
    let server = server::MainServer::new(render_loop);
    let addr = "[::1]:50051".parse().unwrap();
    tracing::info!("Starting gRPC server on {}", addr);
    tonic::transport::Server::builder()
        .add_service(crate::protocol::libserver::lib_server_server::LibServerServer::new(server))
        .add_service(
            tonic_reflection::server::Builder::configure()
                .register_encoded_file_descriptor_set(
                    crate::protocol::libserver::FILE_DESCRIPTOR_SET,
                )
                .build_v1()
                .unwrap(),
        )
        .serve_with_shutdown(addr, async {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to install Ctrl+C handler");
            tracing::info!("Shutting down gRPC server");
        })
        .await?;
    Ok(())
}
