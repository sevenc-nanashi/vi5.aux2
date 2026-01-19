pub struct MainServer {
    render_loop: crate::render_loop::RenderLoop,
}

#[tonic::async_trait]
impl crate::protocol::lib_server_server::LibServer for MainServer {
    async fn initialize(
        &self,
        request: tonic::Request<crate::protocol::InitializeRequest>,
    ) -> Result<tonic::Response<crate::protocol::InitializeResponse>, tonic::Status> {
        let req = request.into_inner();
        tracing::info!("Received initialize request: {:?}", req);
        let response = crate::protocol::InitializeResponse {
            object_infos: vec![],
        };
        Ok(tonic::Response::new(response))
    }

    async fn batch_render(
        &self,
        request: tonic::Request<crate::protocol::BatchRenderRequest>,
    ) -> Result<tonic::Response<crate::protocol::BatchRenderResponse>, tonic::Status> {
        let req = request.into_inner();
        tracing::info!("Received batch render request: {:?}", req);
        let response = crate::protocol::BatchRenderResponse {
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
