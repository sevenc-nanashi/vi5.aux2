#!/usr/bin/env node

import yargs from "yargs";
import { runServer } from "./server";

yargs(process.argv.slice(2))
  .command(
    "start",
    "Start the server",
    (yargs) => {
      return yargs.option("port", {
        alias: "p",
        type: "number",
        description: "Port to run the server on",
        default: 0,
      });
    },
    async (argv) => {
      runServer(process.cwd(), argv.port);
    },
  )
  .parse();
