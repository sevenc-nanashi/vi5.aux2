import p5 from "p5";
import type { FrameInfo } from "../gen/common_pb";
p5.disableFriendlyErrors = true;

function setProperty<T extends keyof p5>(p: p5, property: T, value: p5[T]) {
  p[property] = value;
}

export class Vi5Context {
  #p5Instance: p5 | null = null;
  #mainCanvas: p5.Renderer | null = null;
  #graphics: p5.Graphics[] = [];
  #frameInfo: FrameInfo | null = null;

  /** @internal */
  constructor() {
    this.#p5Instance = null;
  }

  /** @internal */
  initialize(p5Instance: p5) {
    this.#p5Instance = p5Instance;
  }

  get p(): p5 {
    if (!this.#p5Instance) {
      throw new Error("p5 instance has not been initialized yet.");
    }
    return this.#p5Instance;
  }

  createCanvas(
    width: number,
    height: number,
    renderer?: typeof p5.P2D | typeof p5.WEBGL | typeof p5.P2DHDR | symbol,
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

  get mainCanvas() {
    if (!this.#mainCanvas) {
      throw new Error("Main canvas has not been created yet.");
    }
    return this.#mainCanvas;
  }

  get frameInfo() {
    if (!this.#frameInfo) {
      throw new Error("Frame info has not been set yet.");
    }
    return this.#frameInfo;
  }

  /** @internal */
  setFrameInfo(frameInfo: FrameInfo) {
    this.#frameInfo = frameInfo;
    setProperty(this.p, "frameCount", frameInfo.currentFrame);
    setProperty(this.p, "deltaTime", 1000 / frameInfo.framerate);
  }

  notify(level: "info" | "warn" | "error", message: string) {
    window.__vi5__?.notify(level, message);
  }

  /** @internal */
  teardown() {
    this.#mainCanvas?.remove();
    this.#graphics.forEach((g) => g.remove());
    this.#graphics = [];
  }
}
