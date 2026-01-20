import type { ParameterDefinitions, Vi5Object } from "./user/object";

export function defineObject<T extends ParameterDefinitions>(
  option: Vi5Object<T>,
): Vi5Object<T> {
  return option;
}
