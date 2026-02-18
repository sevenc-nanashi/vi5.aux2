import type { PluginOption, UserConfig as ViteConfig } from "vite";

export interface Config {
  name: string;
  vite?: ViteConfig;
  vitePlugins?: PluginOption[];
  hookConsoleLog?: boolean;
}

type ConfigExport = Config | (() => Config) | Promise<Config> | (() => Promise<Config>);

export function defineConfig(config: ConfigExport): ConfigExport {
  return config;
}
