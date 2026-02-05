import * as protobuf from "@bufbuild/protobuf";
import {
  MaybeIncompleteRenderResponseSchema,
  RendereredObjectInfoSchema,
  SingleRenderResponseSchema,
  type MaybeIncompleteRenderResponse,
  type Notification,
  type SingleRenderResponse,
} from "../gen/server-js_pb";
import { Vi5Runtime } from "./runtime";

export type JsRenderResponse =
  | {
      type: "success";
      canvas: HTMLCanvasElement;
      width: number;
      height: number;
      renderNonce: number;
    }
  | {
      type: "error";
      error: string;
      renderNonce: number;
    };

const bytesPerPixel = 3;
const messageHeaderBytes = 11;

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

const getMetadataRows = (
  renderResponses: SingleRenderResponse[],
  notifications: Notification[],
): number => {
  const payload = protobuf.toBinary(
    MaybeIncompleteRenderResponseSchema,
    protobuf.create(MaybeIncompleteRenderResponseSchema, {
      renderResponses,
      notifications,
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
  notifications: Notification[],
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

    const width = response.width;
    const height = response.height;
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

  const updatedMetadataRows = getMetadataRows(renderResponses, notifications);
  return {
    nextIndex: index,
    renderResponses,
    packedCanvases,
    metadataRows: updatedMetadataRows,
  };
};

export function packCanvases(
  responses: JsRenderResponse[],
  notifications: Notification[],
): MaybeIncompleteRenderResponse[] {
  const batches: MaybeIncompleteRenderResponse[] = [];
  let startIndex = 0;

  while (startIndex < responses.length) {
    const batchNotifications = startIndex === 0 ? notifications : [];
    let metadataRows = 1;
    let result: PackResult;
    for (;;) {
      result = packBatch(responses, startIndex, metadataRows, batchNotifications);
      if (result.metadataRows === metadataRows) {
        break;
      }
      metadataRows = result.metadataRows;
    }

    startIndex = result.nextIndex;
    batches.push(
      protobuf.create(MaybeIncompleteRenderResponseSchema, {
        renderResponses: result.renderResponses,
        notifications: batchNotifications,
        isIncomplete: true,
      }),
    );
  }

  batches[batches.length - 1]!.isIncomplete = false;

  return batches;
}
