import type p5 from "p5";

export class Vi5Context {
  readonly p: p5;
  #mainCanvas: p5.Renderer | null = null;
  #graphics: p5.Graphics[] = [];

  constructor(p5Instance: p5) {
    this.p = p5Instance;
  }

  createCanvas(
    width: number,
    height: number,
    renderer?: typeof p5.P2D | typeof p5.WEBGL | typeof p5.P2DHDR | Symbol,
  ): p5.Renderer {
    this.#mainCanvas = this.p.createCanvas(width, height, renderer);
    return this.#mainCanvas;
  }
  createGraphics(
    width: number,
    height: number,
    renderer?: typeof p5.P2D | typeof p5.WEBGL,
  ): p5.Graphics {
    const newGraphics = this.p.createGraphics(width, height, renderer);
    this.#graphics.push(newGraphics);
    return newGraphics;
  }

  teardown() {
    this.#mainCanvas?.remove();
    this.#graphics.forEach((g) => g.remove());
    this.#graphics = [];
  }
}
