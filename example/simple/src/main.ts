import "./style.css";

const canvas = document.getElementById("canvas") as HTMLCanvasElement;
const ctx = canvas.getContext("2d")!;

window.drawFrame = (nonce: number) => {
  const message = JSON.stringify({
    type: "greeting",
    payload: "Hello, Canvas!",
    nonce: nonce,
  });
  const binary = new TextEncoder().encode(message);
  const binaryLength = binary.length;
  const payload = [
    nonce & 0xff,
    (nonce >> 8) & 0xff,
    (nonce >> 16) & 0xff,
    (nonce >> 24) & 0xff,
    binaryLength & 0xff,
    (binaryLength >> 8) & 0xff,
    (binaryLength >> 16) & 0xff,
    (binaryLength >> 24) & 0xff,
    ...binary,
  ];

  // console.log("Payload:", payload);
  for (let i = 0; i < payload.length; i += 3) {
    const chunk = payload.slice(i, i + 3);
    const x = i / 3;
    const y = 0;
    ctx.fillStyle = `rgba(${chunk[0] || 0}, ${chunk[1] || 0}, ${chunk[2] || 0}, 1)`;
    ctx.fillRect(x, y, 1, 1);
  }

  ctx.fillStyle = "black";
  ctx.fillRect(canvas.width - 50, canvas.height - 50, 50, 50);
};
