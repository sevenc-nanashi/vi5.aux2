import type { Plugin } from "vite";
import index from "./index.html?raw";

export function createVi5Plugin(): Plugin {
  return {
    name: "vi5",
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        if (req.url === "/vi5") {
          res.statusCode = 200;
          res.setHeader("Content-Type", "text/html");
          res.end(index.replace("!DIRNAME!", import.meta.dirname));
          return;
        }
        next();
      });
    },
  };
}
