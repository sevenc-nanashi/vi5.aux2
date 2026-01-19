use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use cef::{ImplBrowser, ImplBrowserHost, ImplFrame};
use prost::Message;

type PaintCallback = dyn FnMut(&[u8], usize, usize) -> std::ops::ControlFlow<()> + Send + Sync;
static PAINT_CALLBACKS: std::sync::LazyLock<dashmap::DashMap<u32, Box<PaintCallback>>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);

pub fn on_paint(buffer: &[u8], width: usize, height: usize) {
    if buffer[3] == 0 {
        tracing::warn!("Received empty paint buffer");
        return;
    }
    let nonce = u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[4]]);
    if let Some(mut callback) = PAINT_CALLBACKS.get_mut(&nonce) {
        match callback(buffer, width, height) {
            std::ops::ControlFlow::Break(()) => {
                drop(callback);
                PAINT_CALLBACKS.remove(&nonce);
            }
            std::ops::ControlFlow::Continue(()) => {}
        }
    } else {
        tracing::warn!("No paint callback found for nonce {}", nonce);
    }
}
pub fn on_accelerated_paint(
    buffer: &wgpu::BufferView,
    width: usize,
    height: usize,
    bytes_per_row: usize,
) {
    if buffer[3] != 255 {
        tracing::warn!("Unexpected alpha value: {}", buffer[3]);
        return;
    }
    let buffer = buffer.as_ref();
    let nonce = u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[4]]);
    if let Some(mut callback) = PAINT_CALLBACKS.get_mut(&nonce) {
        let mut slice = vec![0u8; width * height * 4];
        for y in 0..height {
            let src_start = y * bytes_per_row;
            let dst_start = y * width * 4;
            slice[dst_start..dst_start + width * 4]
                .copy_from_slice(&buffer[src_start..src_start + width * 4]);
        }
        match callback(&slice, width, height) {
            std::ops::ControlFlow::Break(()) => {
                drop(callback);
                PAINT_CALLBACKS.remove(&nonce);
            }
            std::ops::ControlFlow::Continue(()) => {}
        }
    } else {
        tracing::warn!("No paint callback found for nonce {}", nonce);
    }
}

pub struct RenderLoop {
    browser: cef::Browser,
    initialized:
        Arc<tokio::sync::OnceCell<anyhow::Result<crate::protocol::serverjs::InitializeInfo>>>,
}

impl RenderLoop {
    pub fn new(browser: cef::Browser) -> Self {
        Self {
            browser,
            initialized: Arc::new(tokio::sync::OnceCell::new()),
        }
    }

    pub async fn wait_for_initialization(&self) {
        for _ in 0..10000 {
            if self.initialized.get().is_some() {
                break;
            }
            cef::do_message_loop_work();
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    pub async fn initialize(
        &self,
        url: &str,
    ) -> anyhow::Result<crate::protocol::serverjs::InitializeInfo> {
        PAINT_CALLBACKS.clear();
        PAINT_CALLBACKS.insert(
            0,
            Box::new({
                let initialized = self.initialized.clone();
                move |buffer, _, _| {
                    match read_message_from_image::<crate::protocol::serverjs::InitializeInfo>(
                        buffer,
                    ) {
                        Ok(info) => {
                            tracing::info!("Page initialization complete");
                            initialized.set(Ok(info)).unwrap();
                        }
                        Err(e) => {
                            tracing::error!("Failed to decode InitializationComplete: {}", e);
                            initialized
                                .set(Err(anyhow::anyhow!(
                                    "Failed to decode InitializationComplete: {}",
                                    e
                                )))
                                .unwrap();
                        }
                    }
                    std::ops::ControlFlow::Break(())
                }
            }),
        );
        tracing::info!("Loading URL for initialization: {}", url);
        self.browser
            .main_frame()
            .unwrap()
            .load_url(Some(&cef::CefString::from(url)));
        self.wait_for_initialization().await;
        match self.initialized.get().unwrap() {
            Ok(info) => Ok(info.clone()),
            Err(e) => Err(anyhow::anyhow!("Initialization failed: {}", e)),
        }
    }

    pub async fn batch_render(
        &self,
        request: crate::protocol::common::BatchRenderRequest,
    ) -> anyhow::Result<crate::protocol::libserver::BatchRenderResponse> {
        self.wait_for_initialization().await;
        let nonce = rand::random::<u32>();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut maybe_tx = Some(tx);
        let callback = Box::new(move |buffer: &[u8], width: usize, height: usize| {
            let Some(tx) = &maybe_tx else {
                return std::ops::ControlFlow::Break(());
            };
            let response = match read_message_from_image::<
                crate::protocol::serverjs::MaybeIncompleteRenderResponse,
            >(buffer)
            {
                Ok(resp) => resp,
                Err(e) => {
                    tracing::error!("Failed to decode BatchRenderResponse: {}", e);
                    return std::ops::ControlFlow::Break(());
                }
            };

            for maybe_renderered_object_info in response.render_responses {
                match maybe_renderered_object_info.response.unwrap() {
                    crate::protocol::serverjs::single_render_response::Response::RendereredObjectInfo(
                        renderered_object_info,
                    ) => {
                        let mut image_data =
                            vec![0u8; renderered_object_info.width as usize * renderered_object_info.height as usize * 4];
                        let start_x = renderered_object_info.x as usize;
                        let end_x = start_x + renderered_object_info.width as usize;
                        for row in 0..renderered_object_info.height as usize {
                            let start_y = renderered_object_info.y as usize + row;
                            let buffer_start = start_y * width * 4 + start_x * 4;
                            let buffer_end = start_y * width * 4 + end_x * 4;
                            let image_data_start = row * renderered_object_info.width as usize * 4;
                            let image_data_end = (row + 1) * renderered_object_info.width as usize * 4;
                            image_data[image_data_start..image_data_end]
                                .copy_from_slice(&buffer[buffer_start..buffer_end]);

                        }
                        let _ = tx.send(crate::protocol::libserver::RenderResponse {
                            response: Some(
                                crate::protocol::libserver::render_response::Response::Success(
                                    crate::protocol::libserver::SuccessRenderResponse {
                                        render_nonce: renderered_object_info.nonce,
                                        width: renderered_object_info.width,
                                        height: renderered_object_info.height,
                                        image_data,
                                    },
                                ),
                            ),
                        });
                    }
                    crate::protocol::serverjs::single_render_response::Response::ErrorMessage(
                        err,
                    ) => {
                        let _ = tx.send(crate::protocol::libserver::RenderResponse {
                            response: Some(
                                crate::protocol::libserver::render_response::Response::ErrorMessage(
                                    err,
                                ),
                            ),
                        });
                    }
                }
            }

            if !response.is_incomplete {
                drop(maybe_tx.take());
                return std::ops::ControlFlow::Break(());
            }
            std::ops::ControlFlow::Continue(())
        });
        PAINT_CALLBACKS.insert(nonce, callback);
        let request = base64::engine::general_purpose::STANDARD.encode(request.encode_to_vec());
        let js = format!("window.__vi5_render({nonce}, '{request}');");
        tracing::debug!(
            "Executing JS to request frame with nonce {}: {}",
            nonce,
            &js
        );
        tracing::debug!("Requesting frame with nonce {}", nonce);
        self.browser.main_frame().unwrap().execute_java_script(
            Some(&cef::CefString::from(js.as_str())),
            None,
            1,
        );
        if let Some(host) = self.browser.host() {
            host.invalidate(cef::PaintElementType::VIEW);
        }
        let mut render_responses = vec![];
        loop {
            let received = rx.try_recv();
            match received {
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    tracing::info!("All render responses received for nonce {}", nonce);
                    break;
                }

                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                    tokio::time::sleep(Duration::from_millis(5)).await;

                    // if let Some(host) = self.browser.host() {
                    //     host.invalidate(cef::PaintElementType::VIEW);
                    // }
                    cef::do_message_loop_work();
                }
                Ok(response) => {
                    render_responses.push(response);
                }
            }
        }

        Ok(crate::protocol::libserver::BatchRenderResponse { render_responses })
    }
}

fn read_message_from_image<T: Message + Default>(buffer: &[u8]) -> anyhow::Result<T> {
    // N1 N2 N3 A N4 L1 L2 A L3 L4 Message Bytes...
    // N: nonce bytes
    // A: alpha byte (ignored)
    // L: length bytes (little-endian u32)
    let message_length = u32::from_le_bytes([buffer[5], buffer[6], buffer[8], buffer[9]]) as usize;
    tracing::debug!("Decoding message of length {}", message_length);
    tracing::debug!(
        "First 20 bytes of buffer: {:?}",
        &buffer[..20.min(buffer.len())]
    );
    let mut message_buffer = vec![0u8; message_length];
    #[expect(clippy::needless_range_loop)]
    for i in 0..message_length {
        let message_byte_index = 8 + i;
        message_buffer[i] = buffer[4 * (message_byte_index / 3) + (message_byte_index % 3)];
    }
    let message = T::decode(&message_buffer[..])?;
    Ok(message)
}
