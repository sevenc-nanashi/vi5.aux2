import { colorToP5Tuple, defineObject, numberStep } from "vi5";
import p5 from "p5";

export default defineObject({
  id: "test-object",
  label: "Test Object",
  parameters: {
    radius: {
      type: "number",
      step: numberStep["1"],
      min: 10,
      max: 283,
      default: 100,
      label: "Radius",
    },
    color: {
      type: "color",
      default: { r: 255, g: 0, b: 0, a: 255 },
      label: "Color",
    },
  },
  setup(ctx, _p, _params) {
    return ctx.createCanvas(200, 200, p5.P2D);
  },
  draw(ctx, p, params) {
    console.log("Drawing with params:", params);
    p.background(100);
    p.fill(...colorToP5Tuple(params.color));
    p.textAlign(ctx.p.CENTER, ctx.p.CENTER);
    p.textSize(16);
    p.text(`Frame: ${p.frameCount}`, 100, 20);
    p.ellipse(100, ctx.frameInfo.currentFrame, params.radius, params.radius);
    ctx.notify("info", "Hello");
  },
});
