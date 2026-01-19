use std::sync::{Arc, atomic::AtomicBool, mpsc};

use log::error;

use cef::{wrap_client, wrap_load_handler, wrap_render_handler, *};

use crate::gpu_capture::GpuCapture;
use crate::types::{RenderMessage, RenderedFrame};

pub struct ShutdownGuard;

impl Drop for ShutdownGuard {
    fn drop(&mut self) {
        shutdown();
    }
}

wrap_load_handler! {
    struct TestLoadHandler {
        loaded: Arc<AtomicBool>,
    }

    impl LoadHandler {
        fn on_load_end(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _http_status_code: ::std::os::raw::c_int,
        ) {
            if let Some(frame) = frame && frame.is_main() == 1 {
                self.loaded.store(true, std::sync::atomic::Ordering::Release);
            }
        }
    }
}

wrap_render_handler! {
    struct TestRenderHandler {
        width: i32,
        height: i32,
        sent: Arc<AtomicBool>,
        gpu: Arc<GpuCapture>,
        sender: mpsc::Sender<RenderMessage>,
    }

    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            if let Some(rect) = rect {
                rect.x = 0;
                rect.y = 0;
                rect.width = self.width;
                rect.height = self.height;
            }
        }

        fn on_paint(
            &self,
            _browser: Option<&mut Browser>,
            type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            buffer: *const u8,
            width: ::std::os::raw::c_int,
            height: ::std::os::raw::c_int,
        ) {
            if type_ != PaintElementType::VIEW {
                return;
            }
            if buffer.is_null() || width <= 0 || height <= 0 {
                return;
            }
            let size = width as usize * height as usize * 4;
            let src = unsafe { std::slice::from_raw_parts(buffer, size) };
            let mut rgba = vec![0; size];
            for (dst, src) in rgba.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                dst[0] = src[2];
                dst[1] = src[1];
                dst[2] = src[0];
                dst[3] = src[3];
            }
            let _ = self.sender.send(RenderMessage::Software(RenderedFrame {
                width: width as usize,
                height: height as usize,
                rgba,
            }));
        }

        fn on_accelerated_paint(
            &self,
            _browser: Option<&mut Browser>,
            type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            info: Option<&AcceleratedPaintInfo>,
        ) {
            if type_ != PaintElementType::VIEW {
                return;
            }
            if info.is_none() {
                return;
            }
            let info = info.unwrap();
            match self.gpu.capture(info) {
                Ok(frame) => {
                    let _ = self.sender.send(RenderMessage::Accelerated(frame));
                }
                Err(err) => {
                    error!("Failed to read accelerated frame: {err}");
                }
            }
        }
    }
}

wrap_client! {
    struct TestClient {
        render_handler: RenderHandler,
        load_handler: LoadHandler,
    }

    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> {
            Some(self.render_handler.clone())
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(self.load_handler.clone())
        }
    }
}

pub fn create_client(
    options: &crate::types::RenderOptions,
    sent: Arc<AtomicBool>,
    loaded: Arc<AtomicBool>,
    gpu: Arc<GpuCapture>,
    sender: mpsc::Sender<RenderMessage>,
) -> Client {
    let render_handler = TestRenderHandler::new(options.width, options.height, sent, gpu, sender);
    let load_handler = TestLoadHandler::new(loaded);
    TestClient::new(render_handler, load_handler)
}
