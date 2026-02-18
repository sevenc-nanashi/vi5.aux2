import { vi5Log } from "./log";
const log = vi5Log.getChild("disposableCounter");

export class DisposableCounterFactory {
  #count = 0;

  createCounter(): DisposableCounter {
    this.#count++;
    log.debug`Created counter, total count: ${this.#count}`;
    return new DisposableCounter(() => this.disposeCounter());
  }

  get count(): number {
    return this.#count;
  }

  private disposeCounter() {
    this.#count--;
    log.debug`Disposed counter, total count: ${this.#count}`;
  }
}

export class DisposableCounter {
  private disposed = false;

  constructor(private onDispose: () => void) {}

  [Symbol.dispose]() {
    if (!this.disposed) {
      this.onDispose();
      this.disposed = true;
    }
  }
}
