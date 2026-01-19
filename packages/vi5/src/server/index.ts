import { createServer as createViteServer } from "vite";
import type { Config } from "../config";
import path from "node:path";
import { createVi5Plugin } from "./plugin";
import { getUnusedPort } from "../helpers/port";

export function runServer(root: string, port: number) {
  let restartPromise: Promise<void> | undefined;
  let config: Config;
  const createDevServer = async (isRestart = true) => {
    const server = await createServer(root, port, restartServer);
    function restartServer() {
      if (!restartPromise) {
        restartPromise = (async () => {
          try {
            config = await resolveConfig(root);
          } catch (err: any) {
            console.error(`failed to resolve config. error:`, err);
            return;
          }
          await server.close();
          await createDevServer();
        })().finally(() => {
          restartPromise = undefined;
        });
      }
      return restartPromise;
    }
    await server.listen(undefined, isRestart);
    server.printUrls();
  };
  createDevServer(false).catch((err) => {
    console.error(err);
    process.exit(1);
  });
}

async function createServer(
  root: string,
  port: number,
  restartServer?: () => Promise<void>,
) {
  return createViteServer({
    root,
    plugins: [createVi5Plugin()],
    server: {
      port: port || (await getUnusedPort(3000)),
    },
  });
}

async function resolveConfig(root: string): Promise<Config> {
  const configPath = path.resolve(root, "vi5.config.ts");
  const mod = await import(configPath);
  const configExport = mod.default || mod;
  return typeof configExport === "function"
    ? configExport()
    : Promise.resolve(configExport);
}
