/// <reference lib="dom" />
import { Vi5Runtime } from "./client/runtime";
import style from "./server/index.css?raw";

// add style

document.head.insertAdjacentHTML("beforeend", `<style>${style}</style>`);

declare global {
  interface Window {
    __vi5__: Vi5Runtime;
  }
}

window.__vi5__ = new Vi5Runtime("");
