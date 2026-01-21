import * as protobuf from "@bufbuild/protobuf";
import {
  MaybeIncompleteRenderResponseSchema,
  RendereredObjectInfoSchema,
  SingleRenderResponseSchema,
  type MaybeIncompleteRenderResponse,
  type SingleRenderResponse,
} from "../gen/server-js_pb";
import { Vi5Runtime } from "./runtime";

export type JsRenderResponse =
  | {
      type: "success";
      canvas: HTMLCanvasElement;
      renderNonce: number;
    }
  | {
      type: "error";
      error: string;
      renderNonce: number;
    };

const bytesPerPixel = 3;
const messageHeaderBytes = 8;

const buildErrorResponse = (nonce: number, error: string): SingleRenderResponse =>
  protobuf.create(SingleRenderResponseSchema, {
    nonce,
    response: {
      case: "errorMessage",
      value: error,
    },
  });

const buildSuccessResponse = (
  nonce: number,
  x: number,
  y: number,
  width: number,
  height: number,
): SingleRenderResponse =>
  protobuf.create(SingleRenderResponseSchema, {
    nonce,
    response: {
      case: "rendereredObjectInfo",
      value: protobuf.create(RendereredObjectInfoSchema, {
        x,
        y,
        width,
        height,
      }),
    },
  });

const getMetadataRows = (renderResponses: SingleRenderResponse[]): number => {
  const payload = protobuf.toBinary(
    MaybeIncompleteRenderResponseSchema,
    protobuf.create(MaybeIncompleteRenderResponseSchema, {
      renderResponses,
      isIncomplete: true,
    }),
  );
  const payloadLength = payload.length + messageHeaderBytes;
  const pixels = Math.ceil(payloadLength / bytesPerPixel);
  return Math.max(1, Math.ceil(pixels / Vi5Runtime.get().canvas.width));
};

type PackedCanvas = {
  canvas: HTMLCanvasElement;
  x: number;
  y: number;
};

type PackResult = {
  nextIndex: number;
  renderResponses: SingleRenderResponse[];
  packedCanvases: PackedCanvas[];
  metadataRows: number;
};

const packBatch = (
  responses: JsRenderResponse[],
  startIndex: number,
  metadataRows: number,
): PackResult => {
  const renderResponses: SingleRenderResponse[] = [];
  const packedCanvases: PackedCanvas[] = [];
  let x = 0;
  let y = metadataRows;
  let rowHeight = 0;
  let index = startIndex;
  while (index < responses.length) {
    const response = responses[index]!;
    if (response.type === "error") {
      renderResponses.push(buildErrorResponse(response.renderNonce, response.error));
      index += 1;
      continue;
    }

    const width = response.canvas.width;
    const height = response.canvas.height;
    if (
      width > Vi5Runtime.get().canvas.width ||
      height > Vi5Runtime.get().canvas.height - metadataRows
    ) {
      renderResponses.push(
        buildErrorResponse(response.renderNonce, "canvas size exceeds pack area"),
      );
      continue;
    }

    if (x + width > Vi5Runtime.get().canvas.width) {
      x = 0;
      y += rowHeight;
      rowHeight = 0;
    }

    if (y + height > Vi5Runtime.get().canvas.height) {
      if (renderResponses.length === 0) {
        renderResponses.push(
          buildErrorResponse(response.renderNonce, "canvas size exceeds pack area"),
        );
      }
      break;
    }

    renderResponses.push(buildSuccessResponse(response.renderNonce, x, y, width, height));
    packedCanvases.push({
      canvas: response.canvas,
      x,
      y,
    });
    x += width;
    index += 1;
    rowHeight = Math.max(rowHeight, height);
  }

  const updatedMetadataRows = getMetadataRows(renderResponses);
  return {
    nextIndex: index,
    renderResponses,
    packedCanvases,
    metadataRows: updatedMetadataRows,
  };
};

export function packCanvases(responses: JsRenderResponse[]): MaybeIncompleteRenderResponse[] {
  const batches: MaybeIncompleteRenderResponse[] = [];
  let startIndex = 0;

  while (startIndex < responses.length) {
    let metadataRows = 1;
    let result: PackResult;
    for (;;) {
      result = packBatch(responses, startIndex, metadataRows);
      if (result.metadataRows === metadataRows) {
        break;
      }
      metadataRows = result.metadataRows;
    }

    startIndex = result.nextIndex;
    batches.push(
      protobuf.create(MaybeIncompleteRenderResponseSchema, {
        renderResponses: result.renderResponses,
        isIncomplete: true,
      }),
    );
  }

  batches[batches.length - 1]!.isIncomplete = false;

  return batches;
}
