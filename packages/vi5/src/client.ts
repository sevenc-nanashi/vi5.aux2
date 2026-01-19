import * as protobuf from "@bufbuild/protobuf";
import { canvas, ctx, drawMessage } from "./client/render";
import { BatchRenderRequestSchema } from "./gen/common_pb";
import {
  InitializeInfoSchema,
  MaybeIncompleteRenderResponseSchema,
  SingleRenderResponseSchema,
} from "./gen/server-js_pb";
import style from "./server/index.css?raw";
import { toBytes } from "fast-base64";

// add style

document.head.insertAdjacentHTML("beforeend", `<style>${style}</style>`);

ctx.clearRect(0, 0, canvas.width, canvas.height);
drawMessage(
  InitializeInfoSchema,
  {
    rendererVersion: "1.0.0",
  },
  0,
);

declare global {
  interface Window {
    __vi5_render: (nonce: number, dataB64: string) => void;
  }
}

window.__vi5_render = async (nonce: number, dataB64: string) => {
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  const data = await toBytes(dataB64);
  const renderPayload = protobuf.fromBinary(BatchRenderRequestSchema, data);
  const canvasInfos = [];

  for (const [i, renderRequest] of renderPayload.renderRequests.entries()) {
    // TODO: do actual rendering based on renderRequest
    ctx.fillStyle = `hsl(${(i * 60) % 360}, 100%, 50%)`;
    ctx.fillRect(i * 10, i * 10 + 10, 10, 10);
    canvasInfos.push(
      protobuf.create(SingleRenderResponseSchema, {
        response: {
          case: "rendereredObjectInfo",
          value: {
            nonce: renderRequest.renderNonce,
            x: i * 10,
            y: i * 10 + 10,
            width: 10,
            height: 10,
          },
        },
      }),
    );
  }
  drawMessage(
    MaybeIncompleteRenderResponseSchema,
    {
      renderResponses: canvasInfos,
      isIncomplete: false,
    },
    nonce,
  );
};
