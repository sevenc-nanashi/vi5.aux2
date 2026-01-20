import type { Plugin } from "vite";
import index from "./index.html?raw";
import { dedent } from "../helpers/dedent";

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
    config() {
      return {
        server: {
          fs: {
            allow: [import.meta.dirname, process.cwd()],
          },
        },
      };
    },
    transform: {
      filter: {
        id: /.*\.object\.ts$/,
      },
      async handler(code, id) {
        return (
          code +
          "\n" +
          dedent(`
        export const __vi5_object_id = import.meta.url;
        if (import.meta.hot) {
          import.meta.hot.accept((newModule) => {
            if (newModule?.default) {
              window.__vi5__.register(import.meta.url, newModule.default);
            }
          });
          import.meta.hot.prune(() => {
            window.__vi5__.unregister(import.meta.url);
          });
        }
        `)
        );
      },
    },
  };
}
