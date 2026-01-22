--PARAMETER_DEFINITIONS--
--LABEL--
--group:Performance
--track@batch_size:Batch Size,1,16,10,1
--END_HEADER
local internal = obj.module("--MODULE_NAME--")

local function to_json(v)
  if type(v) == "number" then
    return tostring(v)
  elseif type(v) == "string" then
    return string.format("%q", v)
  elseif type(v) == "boolean" then
    return tostring(v)
  elseif type(v) == "table" then
    local is_array = (#v > 0)
    local items = {}
    if is_array then
      for i = 1, #v do
        items[#items + 1] = to_json(v[i])
      end
      return "[" .. table.concat(items, ",") .. "]"
    else
      for k, val in pairs(v) do
        items[#items + 1] = string.format("%q:%s", k, to_json(val))
      end
      return "{" .. table.concat(items, ",") .. "}"
    end
  else
    return "null"
  end
end

local object_id = "--OBJECT_ID--"
local params_keys = {
  --PARAMETER_KEYS--
}
local param_values = {
  --PARAMETER_VALUES--
}
local param_types = {
  --PARAMETER_TYPES--
}
local next_frame = obj.time + (1 / obj.framerate)
local function serialize_param_at(frame_offset)
  local serialized_params = {};
  for i = 1, #params_keys do
    local key = params_keys[i]
    local value = param_values[i]
    local kind = param_types[i]
    if kind == "Color" then
      if value == nil then
        serialized_params[key] = { type = kind, value = 0 }
      else
        serialized_params[key] = { type = kind, value = 0xff000000 + value }
      end
    elseif type(v) == "number" then
      serialized_params[key] = {
        type = kind,
        value = obj.getvalue(i - 1, obj.time + frame_offset * (1 / obj.framerate)) or value
      }
    else
      serialized_params[key] = { type = kind, value = value }
    end
  end
  return serialized_params
end

local function get_frame_info_at(frame_offset)
  local time = obj.time + frame_offset * (1 / obj.framerate)
  local frame_info = {
    x = obj.getvalue("x", time),
    y = obj.getvalue("y", time),
    z = obj.getvalue("z", time),
    canvas_width = obj.screen_w,
    canvas_height = obj.screen_h,
    current_frame = obj.frame + frame_offset,
    current_time = time,
    total_frames = obj.totalframe,
    total_time = obj.totaltime,
    framerate = obj.framerate
  }
  return frame_info
end

local batch_serialized_params = {}
local batch_frame_info = {}

for i = 0, batch_size - 1 do
  if obj.frame + i >= obj.totalframe then
    break
  end
  batch_serialized_params[i + 1] = serialize_param_at(i)
  batch_frame_info[i + 1] = get_frame_info_at(i)
end

local image, w, h = internal.call_object(
  object_id,
  obj.effect_id,
  batch_size,
  to_json(batch_serialized_params),
  to_json(batch_frame_info)
)
obj.putpixeldata("object", image, w, h, "rgba")
internal.free_image(obj.effect_id)

-- vim:set ft=lua:
