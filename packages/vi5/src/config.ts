import type { UserConfig as ViteConfig } from "vite";

export interface Config {
  vite: ViteConfig;
}

type ConfigExport =
  | Config
  | (() => Config)
  | Promise<Config>
  | (() => Promise<Config>);

export function defineConfig(config: ConfigExport): ConfigExport {
  return config;
}
