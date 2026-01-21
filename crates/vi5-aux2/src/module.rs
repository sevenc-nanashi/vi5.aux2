use std::collections::HashMap;

use aviutl2::{AnyResult, AviUtl2Info, generic::GenericPlugin, log, module::ScriptModuleFunctions};

use crate::Vi5Aux2;

#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(tag = "type", content = "value")]
enum LuaParameter {
    Str(String),
    Text(String),
    Number(f64),
    Bool(bool),
    Color(u32),
}
#[derive(serde::Deserialize, serde::Serialize, Debug)]
struct LuaFrameInfo {
    x: f64,
    y: f64,
    z: f64,
    canvas_width: i32,
    canvas_height: i32,
    current_frame: i32,
    current_time: f64,
    total_frames: i32,
    total_time: f64,
    framerate: f64,
}

#[aviutl2::plugin(ScriptModule)]
pub struct InternalModule;

static TEMPORARY_BUFFER: std::sync::LazyLock<std::sync::Mutex<dashmap::DashMap<i32, Vec<u8>>>> =
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
    fn call_object(
        &self,
        object_name: String,
        effect_id: i32,
        params_json: String,
        frame_info_json: String,
    ) -> aviutl2::AnyResult<(*const u8, usize, usize)> {
        let params: HashMap<String, LuaParameter> = serde_json::from_str(&params_json)?;
        let frame_info: LuaFrameInfo = serde_json::from_str(&frame_info_json)?;
        let parameters = params
            .iter()
            .map(|(key, param)| {
                anyhow::Ok(vi5_cef::Parameter {
                    key: key.clone(),
                    value: match param {
                        LuaParameter::Str(v) => vi5_cef::ParameterValue::Str(v.clone()),
                        LuaParameter::Text(v) => vi5_cef::ParameterValue::Text(v.clone()),
                        LuaParameter::Number(v) => vi5_cef::ParameterValue::Number(*v),
                        LuaParameter::Bool(v) => vi5_cef::ParameterValue::Bool(*v),
                        LuaParameter::Color(v) => {
                            let color = *v;
                            vi5_cef::ParameterValue::Color(vi5_cef::Color {
                                r: ((color >> 16) & 0xFF) as u8,
                                g: ((color >> 8) & 0xFF) as u8,
                                b: (color & 0xFF) as u8,
                                a: ((color >> 24) & 0xFF) as u8,
                            })
                        }
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
                            object_id: effect_id as i64,
                            frame_info: vi5_cef::FrameInfo {
                                x: frame_info.x,
                                y: frame_info.y,
                                z: frame_info.z,
                                screen_width: frame_info.canvas_width as _,
                                screen_height: frame_info.canvas_height as _,
                                current_frame: frame_info.current_frame as _,
                                current_time: frame_info.current_time,
                                total_frames: frame_info.total_frames as _,
                                total_time: frame_info.total_time,
                                framerate: frame_info.framerate,
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
                    .insert(effect_id, image_data);
                log::debug!(
                    "Rendered object '{}' (id: {}) with size {}x{}",
                    object_name,
                    effect_id,
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
        if TEMPORARY_BUFFER.lock().unwrap().remove(&(id)).is_some() {
            log::debug!("Freed image buffer for id {}", id);
        } else {
            log::warn!("No image buffer found for id {}", id);
        }
    }
}
