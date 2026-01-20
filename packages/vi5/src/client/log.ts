import * as logtape from "@logtape/logtape";
import { UnboundedChannel } from "@sevenc-nanashi/async-channel";

const channel = new UnboundedChannel<logtape.LogRecord>();
export const vi5Log = logtape.getLogger("vi5");

await logtape.configure({
  sinks: {
    console: logtape.getConsoleSink({
      formatter: logtape.getTextFormatter({
        level: "full",
        category: (category: readonly string[]) => `[${category.join("][")}]`,
      }),
    }),
    logChannel: (record) => {
      channel.send(record);
    },
  },
  loggers: [
    {
      category: [],
      lowestLevel: "debug",
      sinks: ["console", "logChannel"],
    },

    {
      category: ["logtape", "meta"],
      lowestLevel: "warning",
      sinks: ["console", "logChannel"],
    },
  ],
});
