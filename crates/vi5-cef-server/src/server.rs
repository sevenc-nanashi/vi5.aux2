pub struct MainServer {
    render_loop: crate::render_loop::RenderLoop,
}

#[tonic::async_trait]
impl crate::protocol::libserver::lib_server_server::LibServer for MainServer {
    async fn initialize(
        &self,
        request: tonic::Request<crate::protocol::libserver::InitializeRequest>,
    ) -> Result<tonic::Response<crate::protocol::libserver::InitializeResponse>, tonic::Status>
    {
        let req = request.into_inner();
        tracing::info!("Received initialize request: {:?}", req);
        let response = self
            .render_loop
            .initialize("http://localhost:3000/vi5")
            .await
            .map_err(|e| tonic::Status::internal(format!("Initialization failed: {}", e)))?;

        Ok(tonic::Response::new(
            crate::protocol::libserver::InitializeResponse {
                renderer_version: response.renderer_version,
                object_infos: vec![],
            },
        ))
    }

    async fn batch_render(
        &self,
        request: tonic::Request<crate::protocol::common::BatchRenderRequest>,
    ) -> Result<tonic::Response<crate::protocol::libserver::BatchRenderResponse>, tonic::Status>
    {
        let req = request.into_inner();
        tracing::info!("Received batch render request: {:?}", req);
        let response = crate::protocol::libserver::BatchRenderResponse {
            render_responses: vec![],
        };
        Ok(tonic::Response::new(response))
    }
}

impl MainServer {
    pub fn new(render_loop: crate::render_loop::RenderLoop) -> Self {
        Self { render_loop }
    }
}
