import * as fastBase64 from "fast-base64";
import * as protobuf from "@bufbuild/protobuf";
import { BatchRenderRequestSchema } from "../gen/common_pb";

import { vi5Log } from "./log";
import {
  InitializeInfoSchema,
  MaybeIncompleteRenderResponseSchema,
  SingleRenderResponseSchema,
} from "../gen/server-js_pb";
import type { Vi5Object } from "..";

const isMessage = <Desc extends protobuf.DescMessage>(
  data: protobuf.MessageShape<Desc> | protobuf.MessageInitShape<Desc>,
): data is protobuf.MessageShape<Desc> => {
  return typeof data === "object" && data !== null && "$typeName" in data;
};

export class Vi5Runtime {
  readonly canvas: HTMLCanvasElement;
  readonly ctx: CanvasRenderingContext2D;
  readonly objects = new Map<string, Vi5Object<never>>();

  constructor(public readonly root: string) {
    this.canvas = document.getElementById("vi5-canvas") as HTMLCanvasElement;
    this.ctx = this.canvas.getContext("2d")!;
  }

  init() {
    this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
    this.drawMessage(
      InitializeInfoSchema,
      {
        rendererVersion: "1.0.0",
      },
      0,
    );
  }

  async render(nonce: number, dataB64: string) {
    this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
    const data = await fastBase64.toBytes(dataB64);
    const renderPayload = protobuf.fromBinary(BatchRenderRequestSchema, data);
    const canvasInfos = [];

    for (const [i, renderRequest] of renderPayload.renderRequests.entries()) {
      // TODO: do actual rendering based on renderRequest
      this.ctx.fillStyle = `hsl(${(i * 60) % 360}, 100%, 50%)`;
      this.ctx.fillRect(i * 10, i * 10 + 10, 10, 10);
      canvasInfos.push(
        protobuf.create(SingleRenderResponseSchema, {
          nonce: renderRequest.renderNonce,
          response: {
            case: "rendereredObjectInfo",
            value: {
              x: i * 10,
              y: i * 10 + 10,
              width: 10,
              height: 10,
            },
          },
        }),
      );
    }
    this.drawMessage(
      MaybeIncompleteRenderResponseSchema,
      {
        renderResponses: canvasInfos,
        isIncomplete: false,
      },
      nonce,
    );
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
    vi5Log.debug`Sending message with nonce ${nonce} and length ${binaryLength}`;

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

  register<T extends Vi5Object<never>>(url: string, object: T) {
    this.objects.set(url, object);
  }
  unregister(url: string) {
    this.objects.delete(url);
  }
}
