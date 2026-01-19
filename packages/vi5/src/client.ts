import { drawMessage } from "./client/render";
import { InitializeInfoSchema } from "./gen/server-js_pb";
import style from "./server/index.css?raw";

// add style

document.head.insertAdjacentHTML("beforeend", `<style>${style}</style>`);

drawMessage(
  InitializeInfoSchema,
  {
    rendererVersion: "1.0.0",
  },
  0,
);
