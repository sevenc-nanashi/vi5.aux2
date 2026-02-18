#[derive(Debug, Clone)]
pub struct InitializeResponse {
    pub project_name: String,
    pub renderer_version: String,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberStep {
    One,
    PointOne,
    PointZeroOne,
    PointZeroZeroOne,
}

impl NumberStep {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::One => "1",
            Self::PointOne => "0.1",
            Self::PointZeroOne => "0.01",
            Self::PointZeroZeroOne => "0.001",
        }
    }
}

impl TryFrom<f64> for NumberStep {
    type Error = ();

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        const TOLERANCE: f64 = 1e-9;
        if (value - 1.0).abs() <= TOLERANCE {
            Ok(Self::One)
        } else if (value - 0.1).abs() <= TOLERANCE {
            Ok(Self::PointOne)
        } else if (value - 0.01).abs() <= TOLERANCE {
            Ok(Self::PointZeroOne)
        } else if (value - 0.001).abs() <= TOLERANCE {
            Ok(Self::PointZeroZeroOne)
        } else {
            Err(())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParameterType {
    String,
    Text,
    Boolean,
    Number {
        step: NumberStep,
        min: f64,
        max: f64,
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
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub screen_width: usize,
    pub screen_height: usize,
    pub current_frame: usize,
    pub current_time: f64,
    pub total_frames: usize,
    pub total_time: f64,
    pub framerate: f64,
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
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogNotificationLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub enum Notification {
    Log(LogNotification),
    ObjectInfos(ObjectInfosNotification),
}

#[derive(Debug, Clone)]
pub struct LogNotification {
    pub level: LogNotificationLevel,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ObjectInfosNotification {
    pub object_infos: Vec<ObjectInfo>,
}
