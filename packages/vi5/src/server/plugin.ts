import type { Plugin } from "vite";
import fs from "node:fs/promises";
import index from "./index.html?raw";
import { dedent } from "../helpers/dedent";
import type { Config } from "../config";

export function createVi5Plugin(config: Config): Plugin {
  return {
    name: "vi5",
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        if (req.url === "/vi5") {
          res.statusCode = 200;
          res.setHeader("Content-Type", "text/html");
          res.end(index.replace("!DIRNAME!", import.meta.dirname));
          server.watcher.add("./src/**/*.object.ts");
          return;
        }
        next();
      });
    },
    async config() {
      return {
        server: {
          fs: {
            allow: [import.meta.dirname, process.cwd()],
          },
          hmr: {
            overlay: false,
          },
        },
        optimizeDeps: {
          include: [
            "vi5/client",
            "vi5 > @logtape/logtape",
            "vi5 > fast-base64",
            "vi5 > @bufbuild/protobuf",
            "vi5 > @bufbuild/protobuf/codegenv2",
          ],
        },
        define: {
          __vi5_data__: {
            projectName: config.name,
            objectList: await Array.fromAsync(
              fs.glob("./src/**/*.object.ts"),
            ).then((files) => files.map((f) => "/" + f.replace(/\\/g, "/"))),
          },
        },
        // resolve: {
        //   alias: {
        //     p5: "./node_modules/p5/lib/p5.min.js",
        //   },
        // },
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
        let __vi5_objectId = null;
        export const __vi5_setObjectId = (id) => (__vi5_objectId = id);
        if (import.meta.hot) {
          import.meta.hot.accept((newModule) => {
            if (newModule?.default) {
              window.__vi5__.register(__vi5_objectId, newModule.default);
              newModule.__vi5_setObjectId(newModule.default.id);
            }
          });
          import.meta.hot.prune(() => {
            window.__vi5__.unregister(__vi5_objectId);
          });
        }
        `)
        );
      },
    },
  };
}
