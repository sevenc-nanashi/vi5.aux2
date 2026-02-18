mod cef_app;
mod gpu_capture;
mod handlers;
mod protocol;
mod render_loop;
mod server;
mod types;

use std::sync::Arc;

use clap::Parser;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::cef_app::{
    build_render_options, build_settings, create_browser, initialize_cef, prepare_process,
};
use crate::gpu_capture::GpuCapture;
use crate::handlers::create_client;
use crate::render_loop::RenderLoop;

#[derive(clap::Parser, Debug)]
pub struct Args {
    /// Enable hardware acceleration
    #[clap(long)]
    hardware_acceleration: bool,

    /// Launch devtools
    #[clap(long)]
    devtools: bool,

    /// Port to listen on
    #[clap(long, default_value = "50051")]
    port: u16,

    /// Parent process (will exit if parent process exits)
    #[clap(long)]
    parent_process: Option<u32>,
}

fn main() -> anyhow::Result<()> {
    // env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
    //     .target(env_logger::Target::Stderr)
    //     .init();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_thread_ids(true)
                .with_thread_names(true),
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

    let cli_args = Args::parse();
    let mut settings = build_settings();
    settings.remote_debugging_port = if cli_args.devtools { 5151 } else { 0 };
    let _shutdown_guard = initialize_cef(&args, &settings)?;

    let gpu = Arc::new(GpuCapture::new()?);

    let mut client = create_client(
        &options,
        gpu,
        crate::render_loop::on_software_paint,
        crate::render_loop::on_accelerated_paint,
    );
    let browser = create_browser(&mut client, cli_args.hardware_acceleration)?;
    let render_loop = RenderLoop::new(browser);
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(main_server(
            render_loop,
            cli_args.port,
            cli_args.parent_process,
        ))?;
    Ok(())
}

async fn watch_parent_process(
    parent_pid: u32,
    sender: Arc<tokio::sync::mpsc::UnboundedSender<()>>,
) {
    let check_interval = tokio::time::Duration::from_secs(5);
    loop {
        if sysinfo::System::new_all()
            .process((parent_pid as usize).into())
            .is_none()
            || sender.is_closed()
        {
            tracing::info!("Parent process {} has exited, shutting down.", parent_pid);
            let _ = sender.send(());
            break;
        }
        tokio::time::sleep(check_interval).await;
    }
}

pub async fn main_server(
    render_loop: RenderLoop,
    port: u16,
    parent_pid: Option<u32>,
) -> anyhow::Result<()> {
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::unbounded_channel();
    let shutdown_tx = Arc::new(shutdown_tx);
    if let Some(ppid) = parent_pid {
        let shutdown_tx_clone = shutdown_tx.clone();
        tokio::spawn(async move {
            watch_parent_process(ppid, shutdown_tx_clone).await;
        });
    }
    let server = server::MainServer::new(render_loop, shutdown_tx);
    let addr = format!("[::1]:{}", port).parse().unwrap();
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
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("Shutting down gRPC server");
                }
                _ = shutdown_rx.recv() => {
                    tracing::info!("Received shutdown request");
                }
            }
        })
        .await?;

    Ok(())
}
