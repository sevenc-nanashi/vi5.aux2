/// <reference lib="dom" />
import { vi5Log } from "./client/log";
import { Vi5Runtime } from "./client/runtime";
import style from "./server/index.css?raw";

// add style

document.head.insertAdjacentHTML("beforeend", `<style>${style}</style>`);

declare global {
  interface Window {
    __vi5__: Vi5Runtime;
  }
}

vi5Log.info("Vi5 Client Runtime initializing...");
window.__vi5__ = new Vi5Runtime("");

declare const __vi5_object_list__: string[];
for (const objectName of __vi5_object_list__) {
  vi5Log.info(`Loading object module: ${objectName}`);
  import(/* @vite-ignore */ `${objectName}`).then((module) => {
    const object = module.default;
    window.__vi5__.register(object);
  });
}
window.__vi5__.init();
