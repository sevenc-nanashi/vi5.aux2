use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use cef::{ImplBrowser, ImplBrowserHost, ImplFrame};
use prost::Message;

type PaintCallback = dyn Fn(&[u8], usize, usize) + Send + Sync;
static PAINT_CALLBACKS: std::sync::LazyLock<dashmap::DashMap<u32, Arc<PaintCallback>>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);

pub fn on_paint(buffer: &[u8], width: usize, height: usize) {
    if buffer[3] == 0 {
        tracing::warn!("Received empty paint buffer");
        return;
    }
    let nonce = u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[4]]);
    if let Some((_, callback)) = PAINT_CALLBACKS.remove(&nonce) {
        callback(buffer, width, height);
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

        // debug
        image::RgbaImage::from_raw(
            width as u32,
            height as u32,
            buffer
                .to_vec()
                .chunks_exact(4)
                .flat_map(|px| vec![px[2], px[1], px[0], px[3]])
                .collect(),
        )
        .unwrap()
        .save(format!("debug_alpha_{}.png", buffer[3]))
        .unwrap();
        return;
    }
    let buffer = buffer.as_ref();
    let nonce = u32::from_le_bytes([buffer[2], buffer[1], buffer[0], buffer[6]]);
    if let Some((_, callback)) = PAINT_CALLBACKS.remove(&nonce) {
        let mut slice = vec![0u8; width * height * 4];
        for y in 0..height {
            let src_start = y * bytes_per_row;
            let dst_start = y * width * 4;
            slice[dst_start..dst_start + width * 4]
                .copy_from_slice(&buffer[src_start..src_start + width * 4]);
        }
        callback(&slice, width, height);
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
            Arc::new({
                let initialized = self.initialized.clone();
                move |buffer, _, _| match read_message_from_image::<
                    crate::protocol::serverjs::InitializeInfo,
                >(buffer)
                {
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

    pub async fn batch_render<F>(
        &self,
        request: crate::protocol::common::BatchRenderRequest,
        on_response: F,
    ) -> anyhow::Result<()>
    where
        F: Fn(
                crate::protocol::serverjs::MaybeIncompleteRenderResponse,
                &[u8],
                usize,
                usize,
            ) -> anyhow::Result<()>
            + std::marker::Sync
            + std::marker::Send
            + 'static,
    {
        self.wait_for_initialization().await;
        let nonce = rand::random::<u32>();
        let callback = Arc::new(move |buffer: &[u8], width: usize, height: usize| {
            let response = match read_message_from_image::<
                crate::protocol::serverjs::MaybeIncompleteRenderResponse,
            >(buffer)
            {
                Ok(resp) => resp,
                Err(e) => {
                    tracing::error!("Failed to decode BatchRenderResponse: {}", e);
                    return;
                }
            };
            match on_response(response, buffer, width, height) {
                Ok(resp) => {
                    tracing::info!("Batch render response processed successfully: {:?}", resp);
                }
                Err(e) => {
                    tracing::error!("Failed to process BatchRenderResponse: {}", e);
                }
            }
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
        loop {
            if let Some(host) = self.browser.host() {
                host.invalidate(cef::PaintElementType::VIEW);
            }
            cef::do_message_loop_work();
        }
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
