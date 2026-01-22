/// <reference types="vite/client" />
import * as fastBase64 from "fast-base64";
import * as protobuf from "@bufbuild/protobuf";
import {
  BatchRenderRequestSchema,
  type ObjectInfo,
  type Parameter,
  type ParameterDefinition as GrpcParameterDefinition,
  type ParameterType as GrpcParameterType,
  type RenderRequest,
  ParameterDefinitionSchema as GrpcParameterDefinitionSchema,
  ParameterTypeSchema,
  ParameterSchema,
  ObjectInfoSchema,
} from "../gen/common_pb";

import { vi5Log } from "./log";
import {
  InitializeInfoSchema,
  MaybeIncompleteRenderResponseSchema,
  type RendereredObjectInfo,
} from "../gen/server-js_pb";
import type {
  InferParameters,
  ParameterDefinitions,
  ParameterType,
  Vi5Object,
} from "../user/object";
import { Vi5Context } from "../user/context";
import { packCanvases, type JsRenderResponse } from "./packCanvas";
import p5 from "p5";

const runtimeLog = vi5Log.getChild("Vi5Runtime");

const isMessage = <Desc extends protobuf.DescMessage>(
  data: protobuf.MessageShape<Desc> | protobuf.MessageInitShape<Desc>,
): data is protobuf.MessageShape<Desc> => {
  return typeof data === "object" && data !== null && "$typeName" in data;
};

// const initializePromises: Record<bigint, Promise<void>> = {};
const initializePromises = new Map<bigint, Promise<void>>();
const contexts = new Map<bigint, Vi5Context>();

async function maybeInitializeContext<T extends ParameterDefinitions>(
  objectId: bigint,
  object: Vi5Object<T>,
  renderRequest: RenderRequest,
  parameter: InferParameters<T>,
): Promise<Vi5Context | undefined> {
  if (!initializePromises.has(objectId)) {
    const initPromise = initializeContext(
      objectId,
      object,
      renderRequest,
      parameter,
    );
    initializePromises.set(objectId, initPromise);

    // 一瞬だけ待ってあげる
    await Promise.race([initPromise, {}]);
  }
  if (contexts.has(objectId)) {
    return contexts.get(objectId);
  }
}
async function initializeContext<T extends ParameterDefinitions>(
  id: bigint,
  object: Vi5Object<T>,
  renderRequest: RenderRequest,
  parameter: InferParameters<T>,
): Promise<void> {
  const ctx = new Vi5Context();
  // TODO: エラー処理
  new p5((sketch) => {
    ctx.initialize(sketch);
    ctx.setFrameInfo(renderRequest.frameInfo!);
    sketch.setup = () => {
      const setup = object.setup(ctx, parameter);
      if (setup instanceof Promise) {
        return setup.then(() => {
          contexts.set(id, ctx);
        });
      } else {
        contexts.set(id, ctx);
      }
    };
    sketch.noLoop();
    sketch.setup();
  });
}
function grpcParamsToJsParams<T extends ParameterDefinitions>(
  grpcParams: Parameter[],
): InferParameters<T> {
  const params: Record<string, ParameterType<any>> = {};
  for (const param of grpcParams) {
    switch (param.value.case) {
      case "strValue":
        params[param.key] = param.value.value;
        break;
      case "textValue":
        params[param.key] = param.value.value;
        break;
      case "numberValue":
        params[param.key] = param.value.value;
        break;
      case "boolValue":
        params[param.key] = param.value.value;
        break;
      case "colorValue":
        params[param.key] = {
          r: param.value.value.r,
          g: param.value.value.g,
          b: param.value.value.b,
          a: param.value.value.a as 0 | 255,
        };
        break;
      default:
        runtimeLog.warn`Unknown parameter value case: ${param.value.case satisfies undefined}`;
    }
  }
  return params as InferParameters<T>;
}

function toGrpcParameterType(
  definition: ParameterDefinitions[string],
): GrpcParameterType {
  switch (definition.type) {
    case "string":
      return protobuf.create(ParameterTypeSchema, {
        kind: {
          case: "string",
          value: {},
        },
      });
    case "text":
      return protobuf.create(ParameterTypeSchema, {
        kind: {
          case: "text",
          value: {},
        },
      });
    case "boolean":
      return protobuf.create(ParameterTypeSchema, {
        kind: {
          case: "boolean",
          value: {},
        },
      });
    case "number":
      return protobuf.create(ParameterTypeSchema, {
        kind: {
          case: "number",
          value: {
            step: definition.step,
            min: definition.min,
            max: definition.max,
          },
        },
      });
    case "color":
      return protobuf.create(ParameterTypeSchema, {
        kind: {
          case: "color",
          value: {},
        },
      });
  }
}

function toGrpcDefaultValue(
  key: string,
  definition: ParameterDefinitions[string],
): Parameter | undefined {
  if (definition.default === undefined) {
    return undefined;
  }
  switch (definition.type) {
    case "string":
      return protobuf.create(ParameterSchema, {
        key,
        value: { case: "strValue", value: definition.default },
      });
    case "text":
      return protobuf.create(ParameterSchema, {
        key,
        value: { case: "textValue", value: definition.default },
      });
    case "number":
      return protobuf.create(ParameterSchema, {
        key,
        value: { case: "numberValue", value: definition.default },
      });
    case "boolean":
      return protobuf.create(ParameterSchema, {
        key,
        value: { case: "boolValue", value: definition.default },
      });
    case "color":
      return protobuf.create(ParameterSchema, {
        key,
        value: {
          case: "colorValue",
          value: {
            r: definition.default.r,
            g: definition.default.g,
            b: definition.default.b,
            a: definition.default.a,
          },
        },
      });
  }
}

function toGrpcParameterDefinition(
  key: string,
  definition: ParameterDefinitions[string],
): GrpcParameterDefinition {
  return protobuf.create(GrpcParameterDefinitionSchema, {
    key,
    label: definition.label ?? key,
    type: toGrpcParameterType(definition),
    defaultValue: toGrpcDefaultValue(key, definition),
  });
}

export class Vi5Runtime {
  readonly canvas: HTMLCanvasElement;
  readonly ctx: CanvasRenderingContext2D;
  readonly objects = new Map<string, Vi5Object<ParameterDefinitions>>();

  constructor(public projectName: string) {
    this.canvas = document.getElementById("vi5-canvas") as HTMLCanvasElement;
    this.ctx = this.canvas.getContext("2d")!;
  }

  init() {
    this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
    this.drawMessage(
      InitializeInfoSchema,
      {
        projectName: this.projectName,
        rendererVersion: "1.0.0",
        objectInfos: Array.from(this.objects.values()).map(
          (obj): ObjectInfo =>
            protobuf.create(ObjectInfoSchema, {
              id: obj.id,
              label: obj.label,
              parameterDefinitions: Object.entries(obj.parameters).map(
                ([key, def]) => toGrpcParameterDefinition(key, def),
              ),
            }),
        ),
      },
      0,
    );

    if (import.meta.hot) {
      import.meta.hot.on("vi5:on-object-list-changed", (_list) => {
        // TOOD: オブジェクトの追加・削除に対応する
        runtimeLog.info`Object list changed, reloading page...`;
        window.location.reload();
      });
    }
  }

  async render(nonce: number, dataB64: string) {
    const data = await fastBase64.toBytes(dataB64);
    const renderPayload = protobuf.fromBinary(BatchRenderRequestSchema, data);
    const jsResponses = await Promise.all(
      renderPayload.renderRequests.map(
        async (req): Promise<JsRenderResponse> => {
          try {
            return await this.doRender(req);
          } catch (e) {
            runtimeLog.error`Error during rendering object ${req.object}: ${e}`;
            return {
              type: "error",
              renderNonce: req.renderNonce,
              error: `Error during rendering: ${e}`,
            };
          }
        },
      ),
    );
    const canvases = new Map<number, HTMLCanvasElement>();
    for (const resp of jsResponses) {
      if (resp.type === "success") {
        canvases.set(resp.renderNonce, resp.canvas);
      }
    }
    const packed = packCanvases(jsResponses);
    for (const packedResponse of packed) {
      for (const renderResponse of packedResponse.renderResponses) {
        if (renderResponse.response.case === "rendereredObjectInfo") {
          const info = renderResponse.response.value as RendereredObjectInfo;
          this.ctx.clearRect(info.x, info.y, info.width, info.height);
          this.ctx.drawImage(
            canvases.get(renderResponse.nonce)!,
            info.x,
            info.y,
            info.width,
            info.height,
            info.x,
            info.y,
            info.width,
            info.height,
          );
          runtimeLog.debug`Rendered object ${renderResponse.nonce} at (${info.x}, ${info.y}) with size ${info.width}x${info.height}`;
        }
      }
      this.drawMessage(
        MaybeIncompleteRenderResponseSchema,
        packedResponse,
        nonce,
      );
    }
    // this.drawMessage(
    //   MaybeIncompleteRenderResponseSchema,
    //   {
    //     renderResponses: renderResponses,
    //     isIncomplete: false,
    //   },
    //   nonce,
    // );
  }

  private async doRender(request: RenderRequest): Promise<JsRenderResponse> {
    const object = this.objects.get(request.object);
    if (!object) {
      runtimeLog.warn`Object not found: ${request.object}`;
      return {
        type: "error",
        renderNonce: request.renderNonce,
        error: `Object not found: ${request.object}`,
      };
    }

    const params = grpcParamsToJsParams(request.parameters);
    const ctx = await maybeInitializeContext(
      request.objectId,
      object,
      request,
      params,
    );
    if (!ctx) {
      runtimeLog.info`Object not initialized yet: ${request.object}`;
      return {
        type: "error",
        renderNonce: request.renderNonce,
        error: `Object not initialized yet: ${request.object}`,
      };
    }
    ctx.setFrameInfo(request.frameInfo!);
    object.draw(ctx, params);
    const p5Canvas = ctx.mainCanvas;
    return {
      type: "success",
      renderNonce: request.renderNonce,
      canvas: p5Canvas.elt,
    };
  }

  drawMessage<Desc extends protobuf.DescMessage>(
    schema: Desc,
    data: protobuf.MessageShape<Desc> | protobuf.MessageInitShape<Desc>,
    nonce: number,
  ): void {
    const message = protobuf.toBinary(
      schema,
      isMessage(data) ? data : protobuf.create(schema, data),
    );
    const binaryLength = message.length;
    const payload = [
      255,
      192,
      128,
      nonce & 0xff,
      (nonce >> 8) & 0xff,
      (nonce >> 16) & 0xff,
      (nonce >> 24) & 0xff,
      binaryLength & 0xff,
      (binaryLength >> 8) & 0xff,
      (binaryLength >> 16) & 0xff,
      (binaryLength >> 24) & 0xff,
      ...message,
    ];
    runtimeLog.debug`Sending message with nonce ${nonce} and length ${binaryLength}`;

    for (let i = 0; i < payload.length; i += 3) {
      const chunk = payload.slice(i, i + 3);
      const index = i / 3;
      const x = index % this.canvas.width;
      const y = Math.floor(index / this.canvas.width);
      this.ctx.fillStyle = `rgb(${chunk[0] || 0}, ${chunk[1] || 0}, ${chunk[2] || 0}, 1)`;
      this.ctx.fillRect(x, y, 1, 1);
    }
  }

  static get() {
    return window.__vi5__;
  }

  register<T extends Vi5Object<ParameterDefinitions>>(object: T) {
    runtimeLog.info`Registering object: ${object.id} (${object.label})`;
    this.objects.set(object.id, object);
  }
  unregister(id: string) {
    runtimeLog.info`Unregistering object: ${id}`;
    this.objects.delete(id);
  }
}
