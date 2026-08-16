/* global process */

import { spawn } from "node:child_process";
import { dirname, delimiter, resolve } from "node:path";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const frontendDirectory = resolve(scriptDirectory, "..");
const cliCandidates = [
  resolve(frontendDirectory, "node_modules/@playwright/test/cli.js"),
  resolve(frontendDirectory, "../node_modules/@playwright/test/cli.js"),
];
const cliPath = cliCandidates.find((candidate) => existsSync(candidate));
if (!cliPath) throw new Error("Could not locate @playwright/test; run npm ci at the repository root");
const nodePath = [
  resolve(frontendDirectory, "node_modules"),
  resolve(frontendDirectory, "../node_modules"),
  process.env.NODE_PATH,
].filter(Boolean).join(delimiter);

const child = spawn(process.execPath, [cliPath, ...process.argv.slice(2)], {
  cwd: frontendDirectory,
  env: { ...process.env, NODE_PATH: nodePath },
  stdio: "inherit",
});

child.on("exit", (code, signal) => {
  if (signal) process.kill(process.pid, signal);
  process.exit(code ?? 1);
});
