use std::collections::HashMap;

use aviutl2::{AnyResult, AviUtl2Info, generic::GenericPlugin, log, module::ScriptModuleFunctions};

use crate::Vi5Aux2;

/// `obj`相当
#[derive(serde::Deserialize, serde::Serialize, Debug)]
struct ScriptRuntimeInfo {
    ox: f64,
    oy: f64,
    oz: f64,
    rx: f64,
    ry: f64,
    rz: f64,
    cx: f64,
    cy: f64,
    cz: f64,
    sx: f64,
    sy: f64,
    sz: f64,
    zoom: f64,
    aspect: f64,
    alpha: f64,
    x: i32,
    y: i32,
    z: i32,
    w: i32,
    h: i32,
    screen_w: i32,
    screen_h: i32,
    framerate: f64,
    frame: i32,
    time: f64,
    totalframe: i32,
    totaltime: f64,
    layer: i32,
    index: i32,
    num: i32,
    id: i64,
    effect_id: i64,
}

#[aviutl2::plugin(ScriptModule)]
pub struct InternalModule;

static TEMPORARY_BUFFER: std::sync::LazyLock<std::sync::Mutex<dashmap::DashMap<i64, Vec<u8>>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(dashmap::DashMap::new()));

impl aviutl2::module::ScriptModule for InternalModule {
    fn new(_info: AviUtl2Info) -> AnyResult<Self> {
        Ok(Self)
    }

    fn plugin_info(&self) -> aviutl2::module::ScriptModuleTable {
        aviutl2::module::ScriptModuleTable {
            information: "vi5.aux2 Internal Module".to_string(),
            functions: Self::functions(),
        }
    }
}

#[aviutl2::module::functions]
impl InternalModule {
    fn serialize_string(&self, input: String) -> aviutl2::AnyResult<String> {
        Ok(serde_json::to_string(&input)?)
    }
    fn serialize_i64(&self, input_low: i32, input_high: i32) -> aviutl2::AnyResult<String> {
        Ok(serde_json::to_string(
            &(((input_high as i64) << 32) | (input_low as u32 as i64)),
        )?)
    }
    fn serialize_number(&self, input: f64) -> aviutl2::AnyResult<String> {
        if input % 1.0 == 0.0 {
            // NOTE: serde_jsonはf64 -> i64はできないけどi64 -> f64はできるので、i64のほうが都合が良い
            Ok(serde_json::to_string(&(input as i64))?)
        } else {
            Ok(serde_json::to_string(&input)?)
        }
    }
    fn serialize_bool(&self, input: bool) -> aviutl2::AnyResult<String> {
        Ok(serde_json::to_string(&input)?)
    }
    fn call_object(
        &self,
        object_name: String,
        serialized_param_keys: aviutl2::module::ScriptModuleParamArray,
        serialized_param_values: aviutl2::module::ScriptModuleParamArray,
        param_types: aviutl2::module::ScriptModuleParamArray,
        serialized_obj_keys: aviutl2::module::ScriptModuleParamArray,
        serialized_obj_values: aviutl2::module::ScriptModuleParamArray,
    ) -> aviutl2::AnyResult<(*const u8, usize, usize)> {
        let mut params = HashMap::<String, (String, serde_json::Value)>::new();
        for i in 0..serialized_param_keys.len() {
            if let (Some(key), Some(value), Some(kind)) = (
                serialized_param_keys.get_str(i),
                serialized_param_values.get_str(i),
                param_types.get_str(i),
            ) {
                params.insert(key.to_string(), (kind, serde_json::from_str(&value)?));
            }
        }
        let mut base_obj = HashMap::<String, serde_json::Value>::new();
        for i in 0..serialized_obj_keys.len() {
            if let (Some(key), Some(value)) = (
                serialized_obj_keys.get_str(i),
                serialized_obj_values.get_str(i),
            ) {
                base_obj.insert(key.to_string(), serde_json::from_str(&value)?);
            }
        }
        let obj = serde_json::from_value::<ScriptRuntimeInfo>(serde_json::Value::Object(
            serde_json::Map::from_iter(base_obj.into_iter()),
        ))?;
        let parameters = params
            .iter()
            .map(|(key, (kind, value))| {
                anyhow::Ok(vi5_cef::Parameter {
                    key: key.clone(),
                    value: match kind.as_str() {
                        "Str" => vi5_cef::ParameterValue::Str(serde_json::from_value::<String>(
                            value.clone(),
                        )?),
                        "Text" => vi5_cef::ParameterValue::Text(serde_json::from_value::<String>(
                            value.clone(),
                        )?),
                        "Number" => vi5_cef::ParameterValue::Number(serde_json::from_value::<f64>(
                            value.clone(),
                        )?),
                        "Bool" => vi5_cef::ParameterValue::Bool(serde_json::from_value::<bool>(
                            value.clone(),
                        )?),
                        "Color" => {
                            let color = serde_json::from_value::<u32>(value.clone())?;
                            vi5_cef::ParameterValue::Color(vi5_cef::Color {
                                r: ((color >> 16) & 0xFF) as u8,
                                g: ((color >> 8) & 0xFF) as u8,
                                b: (color & 0xFF) as u8,
                                a: ((color >> 24) & 0xFF) as u8,
                            })
                        }
                        _ => vi5_cef::ParameterValue::Str(String::new()),
                    },
                })
            })
            .collect::<anyhow::Result<Vec<vi5_cef::Parameter>>>()?;
        let rendered = Vi5Aux2::with_instance({
            let object_name = object_name.clone();
            move |instance| {
                futures::executor::block_on(instance.with_client(async move |client| {
                    client
                        .batch_render(vec![vi5_cef::RenderRequest {
                            object: object_name,
                            object_id: obj.id as i64,
                            frame_info: vi5_cef::FrameInfo {
                                x: obj.x,
                                y: obj.y,
                                width: obj.screen_w,
                                height: obj.screen_h,
                                current_frame: obj.frame,
                                total_frames: obj.totalframe,
                            },
                            parameters,
                        }])
                        .await
                        .map_err(anyhow::Error::from)
                }))
            }
        })?;
        // TODO: 複数リクエスト対応
        let response = rendered[0].response.clone();
        match response {
            vi5_cef::RenderResponseData::Success {
                width: w,
                height: h,
                image_data,
            } => {
                let ptr = image_data.as_ptr();
                TEMPORARY_BUFFER
                    .lock()
                    .unwrap()
                    .insert(obj.effect_id as i64, image_data);
                log::debug!(
                    "Rendered object '{}' (id: {}) with size {}x{}",
                    object_name,
                    obj.effect_id,
                    w,
                    h
                );
                Ok((ptr, w as usize, h as usize))
            }
            vi5_cef::RenderResponseData::Error(err) => {
                anyhow::bail!("Failed to render object: {}", err);
            }
        }
    }
    fn free_image(&self, id: i32) {
        if TEMPORARY_BUFFER
            .lock()
            .unwrap()
            .remove(&(id as i64))
            .is_some()
        {
            log::debug!("Freed image buffer for id {}", id);
        } else {
            log::warn!("No image buffer found for id {}", id);
        }
    }
}
