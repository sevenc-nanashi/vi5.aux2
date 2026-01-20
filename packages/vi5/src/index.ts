import type p5 from "p5";
import type { Vi5Context } from "./user/context";

export const parameterTypes = {
  string: "string",
  text: "text",
  number: "number",
  boolean: "boolean",
  color: "color",
} as const;

export type Color = {
  r: number;
  g: number;
  b: number;
  a: 0 | 1;
};

type ParameterType<T extends keyof typeof parameterTypes> = T extends "string"
  ? string
  : T extends "text"
    ? string
    : T extends "number"
      ? number
      : T extends "boolean"
        ? boolean
        : T extends "color"
          ? Color
          : never;

export const numberStep = {
  "1": "one",
  "0.1": "pointOne",
  "0.01": "pointZeroOne",
  "0.001": "pointZeroZeroOne",
} as const;

type ParameterDefinition<T extends keyof typeof parameterTypes> =
  T extends "number"
    ? {
        type: T;
        default?: ParameterType<T>;
        step: number;
        min?: number;
        max?: number;
      }
    : {
        type: T;
        default?: ParameterType<T>;
      };
type InferParameters<
  T extends Record<string, ParameterDefinition<keyof typeof parameterTypes>>,
> = {
  [K in keyof T]: ParameterType<T[K]["type"]>;
};

export type Vi5Object<
  T extends Record<string, ParameterDefinition<keyof typeof parameterTypes>>,
> = {
  name: string;
  parameters: T;
  setup: (
    ctx: Vi5Context,
    params: InferParameters<T>,
  ) => Promise<p5.Renderer> | p5.Renderer;
  draw: (ctx: Vi5Context, params: InferParameters<T>) => void;
};

export function defineObject<
  T extends Record<string, ParameterDefinition<keyof typeof parameterTypes>>,
>(option: Vi5Object<T>): Vi5Object<T> {
  return option;
}
