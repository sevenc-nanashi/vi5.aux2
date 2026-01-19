import "./style.css";

const canvas = document.getElementById("canvas") as HTMLCanvasElement;
const ctx = canvas.getContext("2d")!;

ctx.fillStyle = "lightblue";
ctx.fillRect(0, 0, canvas.width / 2, canvas.height / 2);
ctx.fillStyle = "lightgreen";
ctx.fillRect(
  canvas.width / 2,
  canvas.height / 2,
  canvas.width / 2,
  canvas.height / 2,
);

ctx.fillStyle = "lightcoral";
ctx.textAlign = "center";
ctx.font = "48px sans-serif";
ctx.fillText("Hello, Canvas!", canvas.width / 2, canvas.height / 2);
