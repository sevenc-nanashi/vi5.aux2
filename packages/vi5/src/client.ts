/// <reference lib="dom" />
import { vi5Log } from "./client/log";
import { Vi5Runtime } from "./client/runtime";
import style from "./server/index.css?raw";

// add style

document.head.insertAdjacentHTML("beforeend", `<style>${style}</style>`);

vi5Log.info("Vi5 Client Runtime initializing...");

declare const __vi5_data__: {
  projectName: string;
  objectList: string[];
  hookConsoleLog: boolean;
};
window.__vi5__ = new Vi5Runtime(__vi5_data__.projectName);
const promises = [];
for (const objectName of __vi5_data__.objectList) {
  vi5Log.info(`Loading object module: ${objectName}`);
  promises.push(
    import(/* @vite-ignore */ `${objectName}`).then((module) => {
      const object = module.default;
      window.__vi5__.register(object);
    }),
  );
}
Promise.allSettled(promises).then(() => {
  window.__vi5__.init();
  if (__vi5_data__.hookConsoleLog) {
    hookConsole();
  }
  vi5Log.info("Vi5 Client Runtime initialized.");
});

function hookConsole() {
  const levels = [
    { method: "log", level: "info" },
    { method: "info", level: "info" },
    { method: "warn", level: "warn" },
    { method: "error", level: "error" },
  ] as const;
  for (const { method, level } of levels) {
    const original = console[method];
    console[method] = (...args: any[]) => {
      original.apply(console, args);
      if (window.__vi5__.isNotifying) {
        return;
      }
      const message = args
        .map((arg) => {
          if (typeof arg === "string") {
            return arg;
          }
          try {
            return JSON.stringify(arg);
          } catch {
            return String(arg);
          }
        })
        .join(" ");
      window.__vi5__.pushLog(level, message);
    };
  }
}
