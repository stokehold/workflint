#!/usr/bin/env node
"use strict";
// Runs the workflint binary from the platform package that npm installed as
// an optional dependency of this package (docs/releasing.md). npm, pnpm, and
// Bun install only the platform package whose os and cpu match the machine,
// so this package needs no install script.
const { spawnSync } = require("node:child_process");
const os = require("node:os");
const path = require("node:path");

// The platform packages, by `${process.platform} ${process.arch}`.
const PACKAGES = {
  "linux x64": "@stokehold/workflint-linux-x64",
  "linux arm64": "@stokehold/workflint-linux-arm64",
  "darwin x64": "@stokehold/workflint-darwin-x64",
  "darwin arm64": "@stokehold/workflint-darwin-arm64",
  "win32 x64": "@stokehold/workflint-win32-x64",
  "win32 arm64": "@stokehold/workflint-win32-arm64",
};

// actionlint's exit code for a fatal error.
const FATAL = 3;

function fail(lines) {
  process.stderr.write(`${lines.map((line) => `workflint: ${line}`).join("\n")}\n`);
  process.exit(FATAL);
}

function binaryPath() {
  const platform = `${process.platform} ${process.arch}`;
  const name = PACKAGES[platform];
  if (name === undefined) {
    fail([
      `the npm package has no binary for ${platform}.`,
      `It has binaries for ${Object.keys(PACKAGES).join(", ")}, in the packages ${Object.values(PACKAGES).join(", ")}.`,
      "Download a release from https://github.com/stokehold/workflint/releases or build workflint from source.",
    ]);
  }
  let manifest;
  try {
    manifest = require.resolve(`${name}/package.json`);
  } catch {
    fail([
      `the package ${name}, which holds the binary for ${platform}, is not installed.`,
      "It is an optional dependency of workflint, and an install with --omit=optional leaves it out.",
      "Reinstall with optional dependencies: npm install --save-dev workflint",
    ]);
  }
  return path.join(path.dirname(manifest), process.platform === "win32" ? "workflint.exe" : "workflint");
}

function main() {
  const binary = binaryPath();
  const env = { ...process.env };
  // process.env ignores the case of names on Windows, and the copy does not.
  if (process.env.WORKFLINT_INSTALLED_FROM === undefined) {
    env.WORKFLINT_INSTALLED_FROM = "installed from npm";
  }
  const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit", env });
  if (result.error) {
    fail([`cannot run ${binary}: ${result.error.message}`]);
  }
  if (result.signal) {
    // Dies of the same signal, so the caller sees what the binary saw. node
    // ignores some signals, such as SIGPIPE, and then the launcher exits with
    // 128 plus the signal number, as a shell reports it.
    process.kill(process.pid, result.signal);
    process.exit(128 + (os.constants.signals[result.signal] ?? 0));
  }
  process.exit(result.status ?? FATAL);
}

main();
