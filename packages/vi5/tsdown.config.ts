import { defineConfig } from "tsdown/config";
import unpluginRaw from "unplugin-raw/rolldown";

export default defineConfig({
  entry: ["src/index.ts", "src/config.ts", "src/cli.ts", "src/client.ts"],
  dts: true,
  format: "esm",
  plugins: [unpluginRaw()],
});
