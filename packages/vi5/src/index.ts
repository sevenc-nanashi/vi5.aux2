import type { ParameterDefinitions, Vi5Object } from "./user/object";

export { NumberStep } from "./gen/common_pb";
export { numberStep } from "./user/object";
export { colorToP5Tuple } from "./user/utils";

export function defineObject<T extends ParameterDefinitions>(option: Vi5Object<T>): Vi5Object<T> {
  return option;
}
