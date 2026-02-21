use std::pin::Pin;
use std::sync::Arc;

use futures::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

pub struct MainServer {
    render_loop: crate::render_loop::RenderLoop,
    processes: tokio::sync::Mutex<Vec<tokio::process::Child>>,
    shutdown_tx: tokio::sync::Mutex<Option<Arc<tokio::sync::mpsc::UnboundedSender<()>>>>,
}

#[tonic::async_trait]
impl crate::protocol::libserver::lib_server_server::LibServer for MainServer {
    type SubscribeNotificationsStream = Pin<
        Box<
            dyn futures::Stream<
                    Item = Result<crate::protocol::libserver::Notification, tonic::Status>,
                > + Send
                + 'static,
        >,
    >;

    async fn initialize(
        &self,
        request: tonic::Request<crate::protocol::libserver::InitializeRequest>,
    ) -> Result<tonic::Response<crate::protocol::libserver::InitializeResponse>, tonic::Status>
    {
        let req = request.into_inner();

        let mut processes_guard = self.processes.lock().await;
        if !processes_guard.is_empty() {
            for mut process in processes_guard.drain(..) {
                match process.kill().await {
                    Ok(_) => {
                        tracing::info!(
                            "Successfully killed existing vi5 process with PID: {}",
                            process.id().unwrap_or(0)
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to kill existing vi5 process with PID: {}: {}",
                            process.id().unwrap_or(0),
                            e
                        );
                    }
                }
            }
        }

        let random_port: u16 = rand::random::<u16>() % 20000 + 10000;
        tracing::info!("Received initialize request: {:?}", req);
        let path = std::path::Path::new(&req.root_path);
        let mut process =
            tokio::process::Command::new(path.join("node_modules").join(".bin").join("vi5.cmd"))
                .arg("start")
                .arg("--port")
                .arg(random_port.to_string())
                .current_dir(path)
                .spawn()
                .map_err(|e| {
                    tonic::Status::internal(format!("Failed to start vi5 process: {}", e))
                })?;
        tracing::info!(
            "Started vi5 process with PID: {}",
            process.id().unwrap_or(0)
        );
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let maybe_exit_code = process.try_wait().map_err(|e| {
            tonic::Status::internal(format!("Failed to check vi5 process status: {}", e))
        })?;
        if let Some(code) = maybe_exit_code {
            return Err(tonic::Status::internal(format!(
                "vi5 process exited prematurely with code: {}",
                code
            )));
        }
        processes_guard.push(process);
        let response = self
            .render_loop
            .initialize(&format!("http://localhost:{}/vi5", random_port))
            .await
            .map_err(|e| tonic::Status::internal(format!("Initialization failed: {}", e)))?;
        let response = crate::protocol::libserver::InitializeResponse {
            project_name: response.project_name,
            renderer_version: response.renderer_version,
        };

        tracing::info!("Initialization completed: {:?}", response);

        Ok(tonic::Response::new(response))
    }

    async fn batch_render(
        &self,
        request: tonic::Request<crate::protocol::common::BatchRenderRequest>,
    ) -> Result<tonic::Response<crate::protocol::libserver::BatchRenderResponse>, tonic::Status>
    {
        let req = request.into_inner();
        tracing::info!("Received batch render request: {:?}", req);
        let render_results = self
            .render_loop
            .batch_render(req)
            .await
            .map_err(|e| tonic::Status::internal(format!("Batch render failed: {}", e)))?;
        Ok(tonic::Response::new(render_results))
    }

    async fn purge_cache(
        &self,
        _request: tonic::Request<crate::protocol::common::Void>,
    ) -> Result<tonic::Response<crate::protocol::common::Void>, tonic::Status> {
        tracing::info!("Received purge cache request");
        self.render_loop
            .purge_cache()
            .await
            .map_err(|e| tonic::Status::internal(format!("Purge cache failed: {}", e)))?;
        Ok(tonic::Response::new(crate::protocol::common::Void {}))
    }

    async fn subscribe_notifications(
        &self,
        _request: tonic::Request<crate::protocol::common::Void>,
    ) -> Result<tonic::Response<Self::SubscribeNotificationsStream>, tonic::Status> {
        let (backlog, rx) = self.render_loop.subscribe_notifications();
        let backlog_stream = futures::stream::iter(backlog.into_iter().map(Ok));
        let live_stream = BroadcastStream::new(rx).filter_map(|item| async move {
            match item {
                Ok(notification) => Some(Ok(notification)),
                Err(err) => {
                    tracing::warn!("Notification stream lagged: {}", err);
                    None
                }
            }
        });
        let stream = backlog_stream.chain(live_stream);
        Ok(tonic::Response::new(Box::pin(stream)))
    }

    async fn shutdown(
        &self,
        _request: tonic::Request<crate::protocol::common::Void>,
    ) -> Result<tonic::Response<crate::protocol::common::Void>, tonic::Status> {
        tracing::info!("Received shutdown request");
        if let Some(tx) = self.shutdown_tx.lock().await.take() {
            let _ = tx.send(());
        }
        Ok(tonic::Response::new(crate::protocol::common::Void {}))
    }
}

impl MainServer {
    pub fn new(
        render_loop: crate::render_loop::RenderLoop,
        shutdown_tx: Arc<tokio::sync::mpsc::UnboundedSender<()>>,
    ) -> Self {
        Self {
            render_loop,
            processes: tokio::sync::Mutex::new(Vec::new()),
            shutdown_tx: tokio::sync::Mutex::new(Some(shutdown_tx)),
        }
    }
}

impl Drop for MainServer {
    fn drop(&mut self) {
        for mut process in futures::executor::block_on(self.processes.lock()).drain(..) {
            match futures::executor::block_on(process.kill()) {
                Ok(_) => {
                    tracing::info!(
                        "Successfully killed vi5 process with PID: {}",
                        process.id().unwrap_or(0)
                    );
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to kill vi5 process with PID: {}: {}",
                        process.id().unwrap_or(0),
                        e
                    );
                }
            }
        }
    }
}
