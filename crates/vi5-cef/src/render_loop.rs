use std::sync::{Arc, atomic::AtomicBool, mpsc};
use std::time::{Duration, Instant};

use crate::types::{RenderMessage, RenderedFrame};
use cef::{ImplBrowser, ImplFrame};

pub fn run_render_loop(
    browser: &cef::Browser,
    receiver: &mpsc::Receiver<RenderMessage>,
    loaded: &Arc<AtomicBool>,
) {
    let mut num_rendered = 0;
    let mut last_frame: Option<RenderedFrame>;
    let mut frame_durations = vec![];
    'outer: loop {
        if !loaded.load(std::sync::atomic::Ordering::Acquire) {
            cef::do_message_loop_work();
            continue 'outer;
        }
        let start_time = Instant::now();
        let nonce = rand::random::<u32>();
        let js = format!(
            r#"
            window.drawFrame({nonce});
            "#,
        );
        log::debug!(
            "Executing JS to request frame with nonce {}: {}",
            nonce,
            &js
        );
        log::info!("Requesting frame with nonce {}", nonce);
        browser.main_frame().unwrap().execute_java_script(
            Some(&cef::CefString::from(js.as_str())),
            None,
            1,
        );
        'inner: loop {
            // browser
            //     .host()
            //     .unwrap()
            //     .invalidate(cef::PaintElementType::VIEW);
            cef::do_message_loop_work();
            if let Ok(message) = receiver.try_recv() {
                let frame = match message {
                    RenderMessage::Software(frame) => {
                        log::info!("Received software frame");
                        frame
                    }
                    RenderMessage::Accelerated(frame) => {
                        log::info!("Received accelerated frame");
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
                    continue 'inner;
                }
                last_frame = Some(frame);
                break 'inner;
            }
        }
        log::info!("Frame {} rendered", num_rendered);
        frame_durations.push(start_time.elapsed());
        num_rendered += 1;
        let frame = last_frame.as_ref().unwrap();
        let message_length =
            u32::from_le_bytes([frame.rgba[5], frame.rgba[6], frame.rgba[8], frame.rgba[9]])
                as usize;
        log::debug!("Message length: {}", message_length);
        let mut message_bytes = Vec::with_capacity(message_length);
        for i in 0..message_length {
            let byte_index = 8 + i;
            message_bytes.push(frame.rgba[4 * (byte_index / 3) + (byte_index % 3)]);
        }
        let message = String::from_utf8_lossy(&message_bytes);
        log::info!("Message from rendered page: {}", message);
        if num_rendered >= 10 {
            break 'outer;
        }
    }

    let duration: Duration = frame_durations.iter().sum();
    log::info!("Total frames rendered: {}", num_rendered);
    log::info!(
        "Average frame time: {:?}",
        duration / (frame_durations.len() as u32)
    );
}
