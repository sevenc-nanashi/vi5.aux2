use std::sync::{Arc, atomic::AtomicBool, mpsc};
use std::time::{Duration, Instant};

use crate::types::{RenderMessage, RenderedFrame};
use cef::{ImplBrowser, ImplBrowserHost, ImplFrame};

pub struct RenderLoop {
    browser: cef::Browser,
    receiver: mpsc::Receiver<RenderMessage>,
    loaded: Arc<AtomicBool>,
}

impl RenderLoop {
    pub fn new(
        browser: cef::Browser,
        receiver: mpsc::Receiver<RenderMessage>,
        loaded: Arc<AtomicBool>,
    ) -> Self {
        Self {
            browser,
            receiver,
            loaded,
        }
    }

    pub fn render(&self) -> anyhow::Result<RenderedFrame> {
        if !self.loaded.load(std::sync::atomic::Ordering::Acquire) {
            let start_time = Instant::now();
            log::info!("Waiting for page to load...");
            while !self.loaded.load(std::sync::atomic::Ordering::Acquire) {
                cef::do_message_loop_work();
                if start_time.elapsed() > Duration::from_secs(10) {
                    anyhow::bail!("Timeout waiting for page to load");
                }
            }
        }
        let start_time = Instant::now();
        let nonce = rand::random::<u32>();
        let js = format!("window.drawFrame({nonce});");
        log::debug!(
            "Executing JS to request frame with nonce {}: {}",
            nonce,
            &js
        );
        log::debug!("Requesting frame with nonce {}", nonce);
        self.browser.main_frame().unwrap().execute_java_script(
            Some(&cef::CefString::from(js.as_str())),
            None,
            1,
        );
        if let Some(host) = self.browser.host() {
            host.invalidate(cef::PaintElementType::VIEW);
        }
        loop {
            if let Ok(message) = self.receiver.try_recv() {
                let frame = match message {
                    RenderMessage::Software(frame) => {
                        log::debug!("Received software frame");
                        frame
                    }
                    RenderMessage::Accelerated(frame) => {
                        log::debug!("Received accelerated frame");
                        frame
                    }
                };
                let frame_nonce = u32::from_le_bytes([
                    frame.rgba[0],
                    frame.rgba[1],
                    frame.rgba[2],
                    frame.rgba[4],
                ]);
                log::debug!("Frame nonce: {}, expected nonce: {}", frame_nonce, nonce);
                if frame_nonce != nonce {
                    continue;
                }
                log::info!("Frame rendered in {:?}", start_time.elapsed());
                return Ok(frame);
            }
            if let Some(host) = self.browser.host() {
                host.invalidate(cef::PaintElementType::VIEW);
            }
            cef::do_message_loop_work();
        }
    }
}
