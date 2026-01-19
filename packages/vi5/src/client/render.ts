/// <reference lib="dom" />

import * as protobuf from "@bufbuild/protobuf";

const canvas = document.getElementById("canvas") as HTMLCanvasElement;
const ctx = canvas.getContext("2d")!;

const isMessage = <Desc extends protobuf.DescMessage>(
  data: protobuf.MessageShape<Desc> | protobuf.MessageInitShape<Desc>,
): data is protobuf.MessageShape<Desc> => {
  return typeof data === "object" && data !== null && "$typeName" in data;
};

export function drawMessage<Desc extends protobuf.DescMessage>(
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

  for (let i = 0; i < payload.length; i += 3) {
    const chunk = payload.slice(i, i + 3);
    const index = i / 3;
    const x = index % canvas.width;
    const y = Math.floor(index / canvas.width);
    ctx.fillStyle = `rgba(${chunk[0] || 0}, ${chunk[1] || 0}, ${chunk[2] || 0}, 1)`;
    ctx.fillRect(x, y, 1, 1);
  }
}
