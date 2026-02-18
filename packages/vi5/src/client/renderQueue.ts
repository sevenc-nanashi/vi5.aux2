import { PriorityQueue } from "@datastructures-js/priority-queue";

type QueueItem = {
  task: () => void | "skip";
  insertedAt: number;
  priority: number;
};

export const priorityLevels = {
  init: 100,
  render: 10,
  notify: 1,
};

export class RenderQueue {
  #queue = new PriorityQueue<QueueItem>((a, b) => {
    return b.priority - a.priority || a.insertedAt - b.insertedAt;
  });
  #raf: number | null = null;

  constructor() {
    const processQueue = () => {
      while (true) {
        const task = this.#queue.dequeue();
        if (task) {
          const result = task.task();
          if (result !== "skip") {
            break;
          }
        } else {
          break;
        }
      }
      this.#raf = requestAnimationFrame(processQueue);
    };
    this.#raf = requestAnimationFrame(processQueue);
  }

  render(priority: number, renderFunction: QueueItem["task"]): Promise<void> {
    const { promise, resolve, reject } = Promise.withResolvers<void>();
    this.#queue.push({
      task: () => {
        try {
          const result = renderFunction();
          resolve();
          return result;
        } catch (error) {
          reject(error);
          return "skip";
        }
      },
      insertedAt: Date.now(),
      priority: priority,
    });
    return promise;
  }
}
