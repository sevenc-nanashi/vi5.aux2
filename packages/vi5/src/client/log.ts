import * as logtape from "@logtape/logtape";

export const vi5Log = logtape.getLogger("vi5");

await logtape.configure({
  sinks: {
    console: logtape.getConsoleSink({
      formatter: logtape.getTextFormatter({
        level: "full",
        category: (category: readonly string[]) => `[${category.join("][")}]`,
      }),
    }),
  },
  loggers: [
    {
      category: [],
      lowestLevel: "debug",
      sinks: ["console"],
    },

    {
      category: ["logtape", "meta"],
      lowestLevel: "warning",
      sinks: ["console"],
    },
  ],
});
