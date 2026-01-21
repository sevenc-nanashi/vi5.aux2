--PARAMETER_DEFINITIONS--
local internal = obj.module("vi5_debug")

local params = {
    -- params
}
local serialized_params = {};
for k, v in pairs(params) do
    serialized_params[k] = internal.serialize_string(v)
end
