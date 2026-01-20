import { defineObject } from "vi5";
import p5 from "p5";

export default defineObject({
  id: "test-object",
  label: "Test Object",
  parameters: {},
  setup(ctx, params) {
    return ctx.createCanvas(200, 200, p5.P2D);
  },
  draw(ctx, params) {
    ctx.p.background(100);
    ctx.p.fill(255, 0, 0);
    ctx.p.ellipse(100, 100, 50, 50);
  },
});
