--PARAMETER_DEFINITIONS--
--LABEL--
--END_HEADER
local internal = obj.module("--MODULE_NAME--")

local object_id = "--OBJECT_ID--"
local params_keys = {
  --PARAMETER_KEYS--
}
local param_values = {
  --PARAMETER_VALUES--
}
local serialized_params_keys = {};
local serialized_params_values = {};
local param_types = {
  --PARAMETER_TYPES--
}
for i = 1, #params_keys do
  serialized_params_keys[i] = params_keys[i]
  local v = param_values[i]
  local t = param_types[i]
  if t == "Color" then
    if v == nil then
      serialized_params_values[i] = internal.serialize_number(0)
    else
      serialized_params_values[i] = internal.serialize_number(0xff000000 + v)
    end
  elseif type(v) == "number" then
    serialized_params_values[i] = internal.serialize_number(v)
  elseif type(v) == "string" then
    serialized_params_values[i] = internal.serialize_string(v)
  elseif type(v) == "boolean" then
    serialized_params_values[i] = internal.serialize_bool(v)
  end
end

local serialized_obj_keys = {};
local serialized_obj_values = {};
for k, v in pairs(obj) do
  if type(v) == "number" then
    serialized_obj_keys[#serialized_obj_keys + 1] = k
    serialized_obj_values[#serialized_obj_values + 1] = internal.serialize_number(v)
  elseif type(v) == "string" then
    serialized_obj_keys[#serialized_obj_keys + 1] = k
    serialized_obj_values[#serialized_obj_values + 1] = internal.serialize_string(v)
  elseif type(v) == "boolean" then
    serialized_obj_keys[#serialized_obj_keys + 1] = k
    serialized_obj_values[#serialized_obj_values + 1] = internal.serialize_bool(v)
  end
end

local image, w, h = internal.call_object(
  object_id,
  serialized_params_keys,
  serialized_params_values,
  param_types,
  serialized_obj_keys,
  serialized_obj_values
)
obj.putpixeldata("object", image, w, h, "rgba")
internal.free_image(obj.effect_id)

-- vim:set ft=lua:
