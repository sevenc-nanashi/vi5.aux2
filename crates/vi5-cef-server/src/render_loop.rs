use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use cef::{ImplBrowser, ImplFrame};
use prost::Message;

type PaintCallback = dyn FnMut(&[u8], usize, usize) -> std::ops::ControlFlow<()> + Send + Sync;
static PAINT_CALLBACKS: std::sync::LazyLock<dashmap::DashMap<u32, Box<PaintCallback>>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);

fn maybe_temporary_save_buffer(
    buffer: &[u8],
    width: usize,
    height: usize,
    bytes_per_row: usize,
    nonce: u32,
) {
    if std::env::var("VI5_SAVE_PAINT_BUFFERS").is_ok() {
        std::fs::create_dir_all("paint_buffer").expect("Failed to create paint_buffer directory");
        let current_nano = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let filename = format!("paint_buffer/paint_buffer_{current_nano}_nonce_{nonce}.png");
        let png = image::RgbaImage::from_fn(width as u32, height as u32, |x, y| {
            let row_start = y as usize * bytes_per_row;
            let pixel_start = row_start + x as usize * 4;
            image::Rgba([
                buffer[pixel_start],
                buffer[pixel_start + 1],
                buffer[pixel_start + 2],
                buffer[pixel_start + 3],
            ])
        });
        png.save(&filename)
            .expect("Failed to save paint buffer as PNG");
        tracing::info!("Saved paint buffer to {}", filename);
    }
}

fn on_paint(buffer: &[u8], width: usize, height: usize, bytes_per_row: usize) {
    if buffer[0..4] == [255, 192, 128, 255] {
        false
    } else {
        tracing::warn!(
            "Invalid paint buffer header: {:?}",
            &buffer[0..16.min(buffer.len())]
        );
        maybe_temporary_save_buffer(buffer, width, height, bytes_per_row, 0);
        return;
    };
    tracing::trace!(
        "First 20 bytes of buffer: {:?}",
        &buffer[..20.min(buffer.len())]
    );
    // パフォーマンスのために、バッファのフルコピーはnonceがちゃんとしていた場合にのみ行う
    let nonce = u32::from_le_bytes([buffer[4], buffer[5], buffer[6], buffer[8]]);
    if let Some(mut callback) = PAINT_CALLBACKS.get_mut(&nonce) {
        let mut slice = vec![0u8; width * height * 4];
        if width * 4 == bytes_per_row {
            tracing::trace!("Performing direct copy for paint buffer");
            slice.copy_from_slice(&buffer[0..width * height * 4]);
        } else {
            tracing::trace!("Performing row-by-row copy for paint buffer");
            for y in 0..height {
                let src_start = y * bytes_per_row;
                let dst_start = y * width * 4;
                slice[dst_start..dst_start + width * 4]
                    .copy_from_slice(&buffer[src_start..src_start + width * 4]);
            }
        }
        match callback(&slice, width, height) {
            std::ops::ControlFlow::Break(()) => {
                tracing::debug!("Paint callback for nonce {} completed and removed", nonce);
                drop(callback);
                PAINT_CALLBACKS.remove(&nonce);
            }
            std::ops::ControlFlow::Continue(()) => {}
        }
    } else {
        tracing::warn!("No paint callback found for nonce {}", nonce);
        maybe_temporary_save_buffer(buffer, width, height, bytes_per_row, nonce);
    }
}

pub fn on_software_paint(buffer: &[u8], width: usize, height: usize) {
    tracing::debug!("Software paint received: {}x{}", width, height);
    on_paint(buffer, width, height, width * 4);
}
pub fn on_accelerated_paint(
    buffer: &wgpu::BufferView,
    width: usize,
    height: usize,
    bytes_per_row: usize,
) {
    tracing::debug!("Accelerated paint received: {}x{}", width, height);
    on_paint(buffer, width, height, bytes_per_row);
}

pub struct RenderLoop {
    browser: cef::Browser,
    initialized:
        Arc<std::sync::Mutex<Option<anyhow::Result<crate::protocol::serverjs::InitializeInfo>>>>,
}

impl RenderLoop {
    pub fn new(browser: cef::Browser) -> Self {
        Self {
            browser,
            initialized: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    pub async fn assert_initialized(&self) -> anyhow::Result<()> {
        let initialized = self
            .initialized
            .lock()
            .expect("Failed to lock initialization state");
        match initialized.as_ref() {
            Some(Ok(_)) => Ok(()),
            Some(Err(e)) => Err(anyhow::anyhow!("RenderLoop initialization failed: {}", e)),
            None => anyhow::bail!("RenderLoop is not initialized"),
        }
    }

    pub async fn wait_for_initialization(&self) -> anyhow::Result<()> {
        let start_time = std::time::Instant::now();
        loop {
            if self
                .initialized
                .lock()
                .expect("Failed to lock initialization state")
                .is_some()
            {
                return Ok(());
            }
            cef::do_message_loop_work();
            tokio::time::sleep(Duration::from_millis(10)).await;
            if start_time.elapsed() > Duration::from_secs(30) {
                anyhow::bail!("Timeout waiting for initialization");
            }
        }
    }

    pub async fn initialize(
        &self,
        url: &str,
    ) -> anyhow::Result<crate::protocol::serverjs::InitializeInfo> {
        {
            let mut initialized = self
                .initialized
                .lock()
                .expect("Failed to lock initialization state");
            *initialized = None;
        }
        PAINT_CALLBACKS.clear();
        PAINT_CALLBACKS.insert(
            0,
            Box::new({
                let initialized = self.initialized.clone();
                move |buffer, _, _| {
                    let result = match read_message_from_image::<
                        crate::protocol::serverjs::InitializeInfo,
                    >(buffer)
                    {
                        Ok(info) => {
                            tracing::info!("Page initialization complete");
                            Ok(info)
                        }
                        Err(e) => {
                            tracing::error!("Failed to decode InitializationComplete: {}", e);
                            Err(anyhow::anyhow!(
                                "Failed to decode InitializationComplete: {}",
                                e
                            ))
                        }
                    };
                    let mut initialized = initialized
                        .lock()
                        .expect("Failed to lock initialization state");
                    if initialized.is_none() {
                        *initialized = Some(result);
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
        self.wait_for_initialization().await?;
        let initialized = self
            .initialized
            .lock()
            .expect("Failed to lock initialization state");
        match initialized.as_ref().expect("Initialization state missing") {
            Ok(info) => Ok(info.clone()),
            Err(e) => Err(anyhow::anyhow!("Initialization failed: {}", e)),
        }
    }

    pub async fn batch_render(
        &self,
        request: crate::protocol::common::BatchRenderRequest,
    ) -> anyhow::Result<crate::protocol::libserver::BatchRenderResponse> {
        self.assert_initialized().await?;
        if request.render_requests.is_empty() {
            return Ok(crate::protocol::libserver::BatchRenderResponse {
                render_responses: vec![],
            });
        }
        tracing::debug!(
            "Starting batch render with {} requests",
            request.render_requests.len()
        );
        let start_time = std::time::Instant::now();
        let nonce = loop {
            let nonce = rand::random::<u32>();
            // 1024までは予約しておく
            if !PAINT_CALLBACKS.contains_key(&nonce) && nonce > 1024 {
                break nonce;
            }
        };
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut maybe_tx = Some(tx);
        let callback = Box::new(move |buffer: &[u8], width: usize, _height: usize| {
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

            for single_render_response in response.render_responses {
                match single_render_response.response.unwrap() {
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
                            render_nonce: single_render_response.nonce,
                            response: Some(
                                crate::protocol::libserver::render_response::Response::Success(
                                    crate::protocol::libserver::SuccessRenderResponse {
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
                            render_nonce: single_render_response.nonce,
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
                tracing::debug!(
                    "All render responses received for nonce {}, took {:?}",
                    nonce,
                    start_time.elapsed()
                );
                return std::ops::ControlFlow::Break(());
            }
            std::ops::ControlFlow::Continue(())
        });
        PAINT_CALLBACKS.insert(nonce, callback);
        let request = base64::engine::general_purpose::STANDARD.encode(request.encode_to_vec());
        let js = format!("window.__vi5__.render({nonce}, '{request}');");
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
        let mut render_responses = vec![];
        let current_nano = std::time::Instant::now();
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
                    if current_nano.elapsed() > Duration::from_secs(30) {
                        PAINT_CALLBACKS.remove(&nonce);
                        anyhow::bail!("Timeout waiting for render responses");
                    }
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
    // H1 H2 H3 A_ N1 N2 N3 A_ N4 L1 L2 A_ L3 L4 M1 A_ M2 M3 M4 A_ M5 ...
    // H: header bytes
    // N: nonce bytes
    // A: alpha byte (ignored)
    // L: length bytes (little-endian u32)
    // M: message bytes
    let message_length =
        u32::from_le_bytes([buffer[9], buffer[10], buffer[12], buffer[13]]) as usize;
    tracing::debug!("Decoding message of length {}", message_length);
    let mut message_buffer = vec![0u8; message_length];
    #[expect(clippy::needless_range_loop)]
    for i in 0..message_length {
        let message_byte_index = 11 + i;
        message_buffer[i] = buffer[4 * (message_byte_index / 3) + (message_byte_index % 3)];
    }
    let message = T::decode(&message_buffer[..])?;
    Ok(message)
}
