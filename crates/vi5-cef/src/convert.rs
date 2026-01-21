use crate::protocol;
use crate::types::{
    Color, FrameInfo, InitializeResponse, ObjectInfo, Parameter, ParameterDefinition,
    ParameterType, ParameterValue, RenderRequest, RenderResponse, RenderResponseData,
};

#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error("missing render response")]
    MissingRenderResponse,
    #[error("missing parameter value")]
    MissingParameterValue,
    #[error("missing parameter type")]
    MissingParameterType,
    #[error("missing parameter type kind")]
    MissingParameterTypeKind,
}

impl ConversionError {
    pub(crate) fn into_status(self) -> tonic::Status {
        tonic::Status::internal(self.to_string())
    }
}

impl RenderRequest {
    pub(crate) fn into_proto(self, render_nonce: i32) -> protocol::common::RenderRequest {
        protocol::common::RenderRequest {
            render_nonce,
            object: self.object,
            object_id: self.object_id,
            frame_info: Some(self.frame_info.into_proto()),
            parameters: self
                .parameters
                .into_iter()
                .map(Parameter::into_proto)
                .collect(),
        }
    }
}

impl FrameInfo {
    fn into_proto(self) -> protocol::common::FrameInfo {
        protocol::common::FrameInfo {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
            current_frame: self.current_frame,
            total_frames: self.total_frames,
        }
    }
}

impl Parameter {
    fn into_proto(self) -> protocol::common::Parameter {
        protocol::common::Parameter {
            key: self.key,
            value: Some(self.value.into_proto()),
        }
    }
}

impl ParameterValue {
    fn into_proto(self) -> protocol::common::parameter::Value {
        match self {
            Self::Str(value) => protocol::common::parameter::Value::StrValue(value),
            Self::Text(value) => protocol::common::parameter::Value::TextValue(value),
            Self::Number(value) => protocol::common::parameter::Value::NumberValue(value),
            Self::Bool(value) => protocol::common::parameter::Value::BoolValue(value),
            Self::Color(value) => {
                protocol::common::parameter::Value::ColorValue(value.into_proto())
            }
        }
    }
}

impl Color {
    fn into_proto(self) -> protocol::common::Color {
        protocol::common::Color {
            r: self.r,
            g: self.g,
            b: self.b,
            a: self.a,
        }
    }
}

impl TryFrom<protocol::libserver::InitializeResponse> for InitializeResponse {
    type Error = ConversionError;

    fn try_from(value: protocol::libserver::InitializeResponse) -> Result<Self, Self::Error> {
        let object_infos = value
            .object_infos
            .into_iter()
            .map(ObjectInfo::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            renderer_version: value.renderer_version,
            object_infos,
        })
    }
}

impl TryFrom<protocol::common::ObjectInfo> for ObjectInfo {
    type Error = ConversionError;

    fn try_from(value: protocol::common::ObjectInfo) -> Result<Self, Self::Error> {
        let parameter_definitions = value
            .parameter_definitions
            .into_iter()
            .map(ParameterDefinition::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            id: value.id,
            label: value.label,
            parameter_definitions,
        })
    }
}

impl TryFrom<protocol::common::ParameterDefinition> for ParameterDefinition {
    type Error = ConversionError;

    fn try_from(value: protocol::common::ParameterDefinition) -> Result<Self, Self::Error> {
        let parameter_type =
            ParameterType::try_from(value.r#type.ok_or(ConversionError::MissingParameterType)?)?;
        let default_value = match value.default_value {
            Some(parameter) => Some(Parameter::try_from(parameter)?),
            None => None,
        };
        Ok(Self {
            key: value.key,
            parameter_type,
            label: value.label,
            default_value,
        })
    }
}

impl TryFrom<protocol::common::ParameterType> for ParameterType {
    type Error = ConversionError;

    fn try_from(value: protocol::common::ParameterType) -> Result<Self, Self::Error> {
        match value
            .kind
            .ok_or(ConversionError::MissingParameterTypeKind)?
        {
            protocol::common::parameter_type::Kind::String(_) => Ok(Self::String),
            protocol::common::parameter_type::Kind::Text(_) => Ok(Self::Text),
            protocol::common::parameter_type::Kind::Boolean(_) => Ok(Self::Boolean),
            protocol::common::parameter_type::Kind::Number(number) => Ok(Self::Number {
                step: number.step,
                min: number.min,
                max: number.max,
            }),
            protocol::common::parameter_type::Kind::Color(_) => Ok(Self::Color),
        }
    }
}

impl TryFrom<protocol::common::Parameter> for Parameter {
    type Error = ConversionError;

    fn try_from(value: protocol::common::Parameter) -> Result<Self, Self::Error> {
        let key = value.key;
        let value = value
            .value
            .ok_or(ConversionError::MissingParameterValue)?;
        Ok(Self {
            key,
            value: ParameterValue::try_from(value)?,
        })
    }
}

impl TryFrom<protocol::common::parameter::Value> for ParameterValue {
    type Error = ConversionError;

    fn try_from(value: protocol::common::parameter::Value) -> Result<Self, Self::Error> {
        Ok(match value {
            protocol::common::parameter::Value::StrValue(value) => Self::Str(value),
            protocol::common::parameter::Value::TextValue(value) => Self::Text(value),
            protocol::common::parameter::Value::NumberValue(value) => Self::Number(value),
            protocol::common::parameter::Value::BoolValue(value) => Self::Bool(value),
            protocol::common::parameter::Value::ColorValue(value) => Self::Color(Color::from(value)),
        })
    }
}

impl From<protocol::common::Color> for Color {
    fn from(value: protocol::common::Color) -> Self {
        Self {
            r: value.r,
            g: value.g,
            b: value.b,
            a: value.a,
        }
    }
}

impl TryFrom<protocol::libserver::RenderResponse> for RenderResponse {
    type Error = ConversionError;

    fn try_from(value: protocol::libserver::RenderResponse) -> Result<Self, Self::Error> {
        let response = value
            .response
            .ok_or(ConversionError::MissingRenderResponse)?;
        let response = match response {
            protocol::libserver::render_response::Response::Success(success) => {
                RenderResponseData::Success {
                    width: success.width,
                    height: success.height,
                    image_data: success.image_data,
                }
            }
            protocol::libserver::render_response::Response::ErrorMessage(message) => {
                RenderResponseData::Error(message)
            }
        };
        Ok(Self {
            render_nonce: value.render_nonce,
            response,
        })
    }
}
