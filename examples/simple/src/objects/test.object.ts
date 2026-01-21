import { colorToP5Tuple, defineObject, NumberStep } from "vi5";
import p5 from "p5";

export default defineObject({
  id: "test-object",
  label: "Test Object",
  parameters: {
    radius: {
      type: "number",
      step: NumberStep.ONE,
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
  setup(ctx, _params) {
    return ctx.createCanvas(200, 200, p5.P2D);
  },
  draw(ctx, params) {
    console.log("Drawing with params:", params)
    ctx.p.background(100);
    ctx.p.fill(...colorToP5Tuple(params.color));
    ctx.p.ellipse(100, 100, params.radius, params.radius);
  },
});
