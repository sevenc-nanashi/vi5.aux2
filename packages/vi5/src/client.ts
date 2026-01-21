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
  vi5Log.info("Vi5 Client Runtime initialized.");
});
