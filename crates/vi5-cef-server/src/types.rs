#[derive(Debug)]
pub struct RenderOptions {
    pub width: i32,
    pub height: i32,
}

#[derive(Debug)]
pub struct RenderedFrame {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

#[derive(Debug)]
pub enum RenderMessage {
    Software(RenderedFrame),
    Accelerated(RenderedFrame),
}

#[derive(Debug)]
pub enum RenderError {
    InitializeFailed,
    BrowserCreateFailed,
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::InitializeFailed => write!(f, "CEF initialization failed"),
            RenderError::BrowserCreateFailed => write!(f, "CEF browser creation failed"),
        }
    }
}

impl std::error::Error for RenderError {}

