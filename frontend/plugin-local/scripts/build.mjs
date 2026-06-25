import { cp, mkdir, rm } from "node:fs/promises";
import { spawn } from "node:child_process";

const run = (command, args) =>
  new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: new URL("..", import.meta.url),
      stdio: "inherit",
      shell: process.platform === "win32",
    });

    child.on("error", reject);
    child.on("exit", (code) => {
      if (code === 0) {
        resolve();
      } else {
        reject(new Error(`${command} ${args.join(" ")} exited with ${code}`));
      }
    });
  });

await rm("dist", { recursive: true, force: true });
await mkdir("dist/assets", { recursive: true });
await run("tsc", ["-p", "tsconfig.json"]);
await cp("src/index.html", "dist/index.html");
await cp("src/styles.css", "dist/assets/styles.css");
