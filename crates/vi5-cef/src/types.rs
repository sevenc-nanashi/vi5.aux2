#[derive(Debug, Clone)]
pub struct InitializeResponse {
    pub renderer_version: String,
    pub object_infos: Vec<ObjectInfo>,
}

#[derive(Debug, Clone)]
pub struct ObjectInfo {
    pub id: String,
    pub label: String,
    pub parameter_definitions: Vec<ParameterDefinition>,
}

#[derive(Debug, Clone)]
pub struct ParameterDefinition {
    pub key: String,
    pub parameter_type: ParameterType,
    pub label: String,
    pub default_value: Option<Parameter>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParameterType {
    String,
    Text,
    Boolean,
    Number {
        step: f64,
        min: Option<f64>,
        max: Option<f64>,
    },
    Color,
}

#[derive(Debug, Clone)]
pub struct RenderRequest {
    pub object: String,
    pub object_id: i64,
    pub frame_info: FrameInfo,
    pub parameters: Vec<Parameter>,
}

#[derive(Debug, Clone)]
pub struct FrameInfo {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub current_frame: i32,
    pub total_frames: i32,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub key: String,
    pub value: ParameterValue,
}

#[derive(Debug, Clone)]
pub enum ParameterValue {
    Str(String),
    Text(String),
    Number(f64),
    Bool(bool),
    Color(Color),
}

#[derive(Debug, Clone)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

#[derive(Debug, Clone)]
pub struct RenderResponse {
    pub render_nonce: i32,
    pub response: RenderResponseData,
}

#[derive(Debug, Clone)]
pub enum RenderResponseData {
    Success {
        width: i32,
        height: i32,
        image_data: Vec<u8>,
    },
    Error(String),
}
