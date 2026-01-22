use crate::convert::ConversionError;
use crate::protocol;
use crate::types::{InitializeResponse, RenderRequest, RenderResponse};
use tap::prelude::*;

type LibServerClient =
    protocol::libserver::lib_server_client::LibServerClient<tonic::transport::Channel>;

#[derive(Debug, Clone)]
pub struct Client {
    inner: LibServerClient,
    next_nonce: i32,
}

impl Client {
    pub async fn connect<D>(
        dst: D,
        timeout: Option<std::time::Duration>,
    ) -> Result<Self, tonic::transport::Error>
    where
        D: TryInto<tonic::transport::Endpoint>,
        D::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let inner =
            LibServerClient::connect(tonic::transport::Endpoint::new(dst)?.pipe(|endpoint| {
                if let Some(timeout) = timeout {
                    endpoint.connect_timeout(timeout)
                } else {
                    endpoint
                }
            }))
            .await?;
        Ok(Self {
            inner,
            next_nonce: 1,
        })
    }

    pub async fn initialize(
        &mut self,
        root_path: impl Into<String>,
    ) -> Result<InitializeResponse, tonic::Status> {
        let request = protocol::libserver::InitializeRequest {
            root_path: root_path.into(),
        };
        let response = self.inner.initialize(request).await?.into_inner();
        InitializeResponse::try_from(response).map_err(ConversionError::into_status)
    }

    pub async fn batch_render(
        &mut self,
        requests: Vec<RenderRequest>,
    ) -> Result<Vec<RenderResponse>, tonic::Status> {
        let mut render_requests = Vec::with_capacity(requests.len());
        let mut nonces = Vec::with_capacity(requests.len());
        for request in requests {
            let nonce = self.next_nonce;
            self.next_nonce = self.next_nonce.wrapping_add(1);
            render_requests.push(request.into_proto(nonce));
            nonces.push(nonce);
        }
        let request = protocol::common::BatchRenderRequest { render_requests };
        let response = self.inner.batch_render(request).await?.into_inner();
        let mut responses = Vec::with_capacity(response.render_responses.len());
        for nonce in nonces {
            let proto_response = response
                .render_responses
                .iter()
                .find(|resp| resp.render_nonce == nonce)
                .ok_or_else(|| {
                    tonic::Status::internal(format!("Missing render response for nonce {}", nonce))
                })?;
            responses.push(
                RenderResponse::try_from(proto_response.clone())
                    .map_err(ConversionError::into_status)?,
            );
        }
        Ok(responses)
    }
}
