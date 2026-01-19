pub struct MainServer {
    render_loop: crate::render_loop::RenderLoop,
    process: tokio::sync::OnceCell<tokio::process::Child>,
}

#[tonic::async_trait]
impl crate::protocol::libserver::lib_server_server::LibServer for MainServer {
    async fn initialize(
        &self,
        request: tonic::Request<crate::protocol::libserver::InitializeRequest>,
    ) -> Result<tonic::Response<crate::protocol::libserver::InitializeResponse>, tonic::Status>
    {
        let req = request.into_inner();
        let random_port: u16 = rand::random::<u16>() % 20000 + 10000;
        tracing::info!("Received initialize request: {:?}", req);
        let path = std::path::Path::new(&req.root_path);
        let process =
            tokio::process::Command::new(path.join("node_modules").join(".bin").join("vi5.cmd"))
                .arg("start")
                .arg("--port")
                .arg(random_port.to_string())
                .spawn()
                .map_err(|e| {
                    tonic::Status::internal(format!("Failed to start vi5 process: {}", e))
                })?;
        tracing::info!(
            "Started vi5 process with PID: {}",
            process.id().unwrap_or(0)
        );
        self.process
            .set(process)
            .map_err(|_| tonic::Status::internal("Process already started"))?;
        let response = self
            .render_loop
            .initialize(&format!("http://localhost:{}/vi5", random_port))
            .await
            .map_err(|e| tonic::Status::internal(format!("Initialization failed: {}", e)))?;
        let response = crate::protocol::libserver::InitializeResponse {
            renderer_version: response.renderer_version,
            object_infos: vec![],
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
}

impl MainServer {
    pub fn new(render_loop: crate::render_loop::RenderLoop) -> Self {
        Self {
            render_loop,
            process: tokio::sync::OnceCell::new(),
        }
    }
}
