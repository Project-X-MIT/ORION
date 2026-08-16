/* global process */

import { spawn } from "node:child_process";
import { dirname, delimiter, resolve } from "node:path";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const frontendDirectory = resolve(scriptDirectory, "..");
const rootDirectory = resolve(frontendDirectory, "..");
const frontendCliPath = resolve(frontendDirectory, "node_modules/@playwright/test/cli.js");
const rootCliPath = resolve(rootDirectory, "node_modules/@playwright/test/cli.js");
// Root-level E2E specs must use the same Playwright installation as the CLI.
// Workspace installs commonly hoist it to the repository root; a standalone
// frontend install keeps it under frontend/node_modules.
const cliPath = existsSync(rootCliPath) ? rootCliPath : frontendCliPath;
const nodePath = [
  resolve(rootDirectory, "node_modules"),
  resolve(frontendDirectory, "node_modules"),
  process.env.NODE_PATH,
].filter((path) => path && existsSync(path)).join(delimiter);

const child = spawn(process.execPath, [cliPath, ...process.argv.slice(2)], {
  cwd: frontendDirectory,
  env: { ...process.env, NODE_PATH: nodePath },
  stdio: "inherit",
});

child.on("exit", (code, signal) => {
  if (signal) process.kill(process.pid, signal);
  process.exit(code ?? 1);
});
