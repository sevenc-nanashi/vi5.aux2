use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
};

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

static TEMPORARY_BUFFER: std::sync::LazyLock<dashmap::DashMap<i32, Vec<u8>>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);
static RENDER_CACHE: std::sync::LazyLock<dashmap::DashMap<i32, RenderCachePerEffectEntry>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);

#[derive(Debug, Clone, Default)]
struct RenderCachePerEffectEntry {
    images: HashMap<u64, RenderCacheEntry>,
}
#[derive(Debug, Clone)]
struct RenderCacheEntry {
    image_data: Vec<u8>,
    width: usize,
    height: usize,
}

fn hash_parameter_value(param: &vi5_cef::ParameterValue, hasher: &mut impl Hasher) {
    match param {
        vi5_cef::ParameterValue::Str(value) => {
            0_u8.hash(hasher);
            value.hash(hasher);
        }
        vi5_cef::ParameterValue::Text(value) => {
            1_u8.hash(hasher);
            value.hash(hasher);
        }
        vi5_cef::ParameterValue::Number(value) => {
            2_u8.hash(hasher);
            value.to_bits().hash(hasher);
        }
        vi5_cef::ParameterValue::Bool(value) => {
            3_u8.hash(hasher);
            value.hash(hasher);
        }
        vi5_cef::ParameterValue::Color(value) => {
            4_u8.hash(hasher);
            value.r.hash(hasher);
            value.g.hash(hasher);
            value.b.hash(hasher);
            value.a.hash(hasher);
        }
    }
}

fn compute_cache_key(request: &vi5_cef::RenderRequest) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    request.object.hash(&mut hasher);
    request.object_id.hash(&mut hasher);
    for param in &request.parameters {
        param.key.hash(&mut hasher);
        hash_parameter_value(&param.value, &mut hasher);
    }
    request.frame_info.x.to_bits().hash(&mut hasher);
    request.frame_info.y.to_bits().hash(&mut hasher);
    request.frame_info.z.to_bits().hash(&mut hasher);
    request.frame_info.screen_width.hash(&mut hasher);
    request.frame_info.screen_height.hash(&mut hasher);
    request.frame_info.current_frame.hash(&mut hasher);
    request.frame_info.current_time.to_bits().hash(&mut hasher);
    request.frame_info.total_frames.hash(&mut hasher);
    request.frame_info.total_time.to_bits().hash(&mut hasher);
    request.frame_info.framerate.to_bits().hash(&mut hasher);
    hasher.finish()
}

fn build_render_request(
    object_name: String,
    effect_id: i32,
    params: &HashMap<String, LuaParameter>,
    frame_info: &LuaFrameInfo,
) -> anyhow::Result<vi5_cef::RenderRequest> {
    let mut param_keys: Vec<&String> = params.keys().collect();
    param_keys.sort();
    let parameters = param_keys
        .into_iter()
        .map(|key| {
            let param = params
                .get(key)
                .ok_or_else(|| anyhow::anyhow!("Missing parameter: {}", key))?;
            Ok(vi5_cef::Parameter {
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
    Ok(vi5_cef::RenderRequest {
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
    })
}

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
        batch_size: i32,
        params_json: String,
        frame_info_json: String,
    ) -> aviutl2::AnyResult<(*const u8, usize, usize)> {
        let batch_size: usize = batch_size
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid batch_size: {}", batch_size))?;
        let batch_params: Vec<HashMap<String, LuaParameter>> = serde_json::from_str(&params_json)?;
        let batch_frame_info: Vec<LuaFrameInfo> = serde_json::from_str(&frame_info_json)?;
        let batch_render_request = if batch_params.len() != batch_frame_info.len() {
            anyhow::bail!(
                "Mismatched batch sizes: params {}, frame_info {}",
                batch_params.len(),
                batch_frame_info.len()
            );
        } else {
            batch_params
                .into_iter()
                .zip(batch_frame_info.into_iter())
                .map(|(params, frame_info)| {
                    build_render_request(object_name.clone(), effect_id, &params, &frame_info)
                })
                .collect::<anyhow::Result<Vec<vi5_cef::RenderRequest>>>()?
        };
        let batch_cache_keys: Vec<u64> =
            batch_render_request.iter().map(compute_cache_key).collect();

        let mut cached_entries = RENDER_CACHE.entry(effect_id).or_default();

        // 以下の条件でレンダリングする：
        // - 大前提：キャッシュされていないフレームがある
        // - そのうえで、以下のうちいずれかを満たす場合：
        //   - 現在のフレーム（batch_render_request[0]）のキャッシュが存在しない
        //   - batch_render_request.len() が batch_size 未満（終端付近）
        //   - batch_size * (3 / 4) フレーム以上キャッシュが存在しない（一回のレンダリングでまとめて描画したほうがお得）
        //
        // NOTE: 3/4という数字はなんとなくなので要調整？
        let should_render_now = !batch_cache_keys
            .iter()
            .all(|key| cached_entries.images.contains_key(key))
            && (!cached_entries.images.contains_key(&batch_cache_keys[0])
                || batch_render_request.len() < batch_size
                || batch_cache_keys
                    .iter()
                    .filter(|key| !cached_entries.images.contains_key(key))
                    .count()
                    >= (batch_size * 3 / 4));

        if should_render_now {
            let (uncached_keys, uncached_requests) = batch_render_request
                .into_iter()
                .enumerate()
                .filter_map(|(i, req)| {
                    if !cached_entries.images.contains_key(&batch_cache_keys[i]) {
                        Some((batch_cache_keys[i], req))
                    } else {
                        None
                    }
                })
                .unzip::<_, _, Vec<_>, Vec<_>>();
            log::debug!(
                "Rendering {} uncached requests for effect_id {}",
                uncached_requests.len(),
                effect_id
            );
            let rendered = Vi5Aux2::with_instance({
                move |instance| {
                    instance
                        .runtime
                        .read()
                        .map_err(|e| {
                            anyhow::anyhow!("Failed to acquire runtime read lock: {}", e)
                        })?
                        .as_ref()
                        .ok_or_else(|| {
                            anyhow::anyhow!("tokio runtime is not initialized")
                        })?
                        .block_on(instance.with_client(async move |client| {
                        tokio::select! {
                            result = client
                                .batch_render(uncached_requests) => {
                                    result.map_err(|e| anyhow::anyhow!("Batch render failed: {}", e))
                                }
                            _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {
                                Err(anyhow::anyhow!("Batch render timed out"))
                            }
                        }
                    }))
                }
            })?;
            // batch_cache_keys に存在しないキャッシュを削除
            cached_entries
                .images
                .retain(|key, _| batch_cache_keys.contains(key));

            for (response, cache_key) in rendered.into_iter().zip(uncached_keys.into_iter()) {
                match response.response {
                    vi5_cef::RenderResponseData::Success {
                        width,
                        height,
                        image_data,
                    } => {
                        cached_entries.images.insert(
                            cache_key,
                            RenderCacheEntry {
                                image_data,
                                width: width as usize,
                                height: height as usize,
                            },
                        );
                    }
                    vi5_cef::RenderResponseData::Error(err) => {
                        if cache_key == batch_cache_keys[0] {
                            anyhow::bail!("JS returned error: {}", err);
                        }
                        continue;
                    }
                }
            }
        }

        let current_image = cached_entries
            .images
            .get(&batch_cache_keys[0])
            .ok_or_else(|| anyhow::anyhow!("Unreachable: first image not cached"))?;
        let current_image_data = current_image.image_data.clone();
        let current_image_ptr = current_image_data.as_ptr();
        TEMPORARY_BUFFER.insert(effect_id, current_image_data);
        Ok((current_image_ptr, current_image.width, current_image.height))
    }
    fn free_image(&self, id: i32) {
        if TEMPORARY_BUFFER.remove(&id).is_some() {
            log::debug!("Freed image buffer for id {}", id);
        } else {
            log::warn!("No image buffer found for id {}", id);
        }
    }
}

pub fn clear_render_cache() {
    RENDER_CACHE.clear();
    TEMPORARY_BUFFER.clear();
}
