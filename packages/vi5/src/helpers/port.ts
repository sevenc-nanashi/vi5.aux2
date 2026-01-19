import net from "node:net";

function checkPort(port: number): Promise<boolean> {
  return new Promise((resolve) => {
    const server = net.createServer();
    server.once("error", () => {
      resolve(false);
    });
    server.once("listening", () => {
      server.close();
      resolve(true);
    });
    server.listen(port);
  });
}
export async function getUnusedPort(startingPort: number): Promise<number> {
  let port = startingPort;
  while (!(await checkPort(port))) {
    port++;
  }
  return port;
}
