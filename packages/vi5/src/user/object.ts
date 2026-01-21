import type p5 from "p5";
import type { Vi5Context } from "./context";

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

export type ParameterType<T extends keyof typeof parameterTypes> =
  T extends "string"
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
        label?: string;
        default?: ParameterType<T>;
        step: number;
        min?: number;
        max?: number;
      }
    : {
        type: T;
        label?: string;
        default?: ParameterType<T>;
      };
export type InferParameters<
  T extends Record<string, ParameterDefinition<keyof typeof parameterTypes>>,
> = {
  [K in keyof T]: ParameterType<T[K]["type"]>;
};

export type ParameterDefinitions = Record<
  string,
  ParameterDefinition<keyof typeof parameterTypes>
>;
export type Vi5Object<T extends ParameterDefinitions> = {
  id: string;
  label: string;

  parameters: T;
  setup: (
    ctx: Vi5Context,
    params: InferParameters<T>,
  ) => Promise<p5.Renderer> | p5.Renderer;
  draw: (ctx: Vi5Context, params: InferParameters<T>) => void;
};
