import { drawMessage } from "./client/render";
import { InfoSchema } from "./gen/server-js_pb";
import style from "./server/index.css?raw";

// add style

document.head.insertAdjacentHTML("beforeend", `<style>${style}</style>`);

drawMessage(
  InfoSchema,
  {
    serverVersion: "1.0.0",
  },
  0,
);
