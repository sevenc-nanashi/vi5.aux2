use std::sync::Arc;

use cef::{wrap_client, wrap_render_handler, *};

use crate::gpu_capture::GpuCapture;

pub struct ShutdownGuard;

impl Drop for ShutdownGuard {
    fn drop(&mut self) {
        tracing::info!("Shutting down CEF");
        shutdown();
    }
}

wrap_render_handler! {
    struct TestRenderHandler {
        width: i32,
        height: i32,
        gpu: Arc<GpuCapture>,
        on_paint: fn(
            buffer: &[u8],
            width: usize,
            height: usize,
        ),
        on_accelerated_paint: fn(
            buffer: &wgpu::BufferView,
            width: usize,
            height: usize,
            bytes_per_row: usize,
        ),
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
            tracing::trace!("Received software paint from CEF");
            if buffer.is_null() || width <= 0 || height <= 0 {
                return;
            }
            let size = width as usize * height as usize * 4;
            let src = unsafe { std::slice::from_raw_parts(buffer, size) };
            (self.on_paint)(src, width as usize, height as usize);
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
            tracing::trace!("Received accelerated paint from CEF");
            let info = info.unwrap();
            match self.gpu.capture(info, self.on_accelerated_paint) {
                Ok(()) => {
                }
                Err(err) => {
                    tracing::error!("Failed to read accelerated frame: {err}");
                }
            }
        }
    }
}

wrap_client! {
    struct TestClient {
        render_handler: RenderHandler,
    }

    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> {
            Some(self.render_handler.clone())
        }
    }
}

pub fn create_client(
    options: &crate::types::RenderOptions,
    gpu: Arc<GpuCapture>,
    on_paint: fn(buffer: &[u8], width: usize, height: usize),
    on_accelerated_paint: fn(
        buffer: &wgpu::BufferView,
        width: usize,
        height: usize,
        bytes_per_row: usize,
    ),
) -> Client {
    let render_handler = TestRenderHandler::new(
        options.width,
        options.height,
        gpu,
        on_paint,
        on_accelerated_paint,
    );
    TestClient::new(render_handler)
}
