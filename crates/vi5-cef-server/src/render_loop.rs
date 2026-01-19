use std::sync::{Arc, atomic::AtomicBool};
use std::time::{Duration, Instant};

use base64::Engine;
use cef::{ImplBrowser, ImplBrowserHost, ImplFrame};
use prost::Message;

use crate::protocol::BatchRenderResponse;

type PaintCallback = dyn Fn(&[u8], usize, usize) + Send + Sync;
static PAINT_CALLBACKS: std::sync::LazyLock<dashmap::DashMap<u32, Arc<PaintCallback>>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);

pub fn on_paint(buffer: &[u8], width: usize, height: usize) {
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
    let buffer = buffer.as_ref();
    let nonce = u32::from_le_bytes([buffer[2], buffer[1], buffer[0], buffer[6]]);
    if let Some((_, callback)) = PAINT_CALLBACKS.remove(&nonce) {
        let mut slice = vec![0u8; width * height * 4];
        for y in 0..height {
            let src_start = y * bytes_per_row;
            let dst_start = y * width * 4;
            for x in 0..(width * 4) {
                // BGRA to RGBA
                slice[dst_start + x] = buffer[src_start * 4 + x + 2];
                slice[dst_start + x + 1] = buffer[src_start * 4 + x + 1];
                slice[dst_start + x + 2] = buffer[src_start * 4 + x];
                slice[dst_start + x + 3] = buffer[src_start * 4 + x + 3];
            }
        }
        callback(&slice, width, height);
    } else {
        tracing::warn!("No paint callback found for nonce {}", nonce);
    }
}

pub struct RenderLoop {
    browser: cef::Browser,
    initialized: Arc<tokio::sync::OnceCell<()>>,
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

    pub async fn initialize(&self, url: &str) {
        PAINT_CALLBACKS.clear();
        PAINT_CALLBACKS.insert(
            0,
            Arc::new({
                let initialized = self.initialized.clone();
                move |buffer, _, _| match read_message_from_image::<crate::protocol::Info>(buffer) {
                    Ok(_) => {
                        tracing::info!("Page initialization complete");
                        initialized.set(()).unwrap();
                    }
                    Err(e) => {
                        tracing::error!("Failed to decode InitializationComplete: {}", e);
                    }
                }
            }),
        );
        self.browser
            .main_frame()
            .unwrap()
            .load_url(Some(&cef::CefString::from(url)));
        self.wait_for_initialization().await;
    }

    pub async fn batch_render<F>(
        &self,
        request: crate::protocol::BatchRenderRequest,
        on_response: F,
    ) -> anyhow::Result<()>
    where
        F: Fn(
                crate::protocol::MaybeIncompleteRenderResponse,
                &[u8],
                usize,
                usize,
            ) -> anyhow::Result<BatchRenderResponse>
            + std::marker::Sync
            + std::marker::Send
            + 'static,
    {
        self.wait_for_initialization().await;
        let nonce = rand::random::<u32>();
        let callback = Arc::new(move |buffer: &[u8], width: usize, height: usize| {
            let response = match read_message_from_image::<
                crate::protocol::MaybeIncompleteRenderResponse,
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
    let message_length = u32::from_le_bytes([buffer[5], buffer[6], buffer[8], buffer[9]]) as usize;
    let mut message_buffer = vec![0u8; message_length];
    for i in 0..message_length {
        let message_byte_index = 8 + i;
        message_buffer[i] = buffer[4 * (message_byte_index / 3) + (message_byte_index % 3)];
    }
    let message = T::decode(&message_buffer[..])?;
    Ok(message)
}
