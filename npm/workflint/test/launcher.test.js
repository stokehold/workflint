"use strict";
// Tests bin/workflint.js. Run with `node --test npm/workflint/test/`.
//
// Each test installs the launcher into a temporary node_modules next to a fake
// platform package. The fake binary is a copy of node, so a test passes it
// node's own arguments to choose the exit code, the signal, or the output.
const assert = require("node:assert/strict");
const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { after, before, describe, test } = require("node:test");

const launcherSource = path.join(__dirname, "..", "bin", "workflint.js");
const windows = process.platform === "win32";
const platformPackage = `@stokehold/workflint-${process.platform}-${process.arch}`;

let work;
let installed;
let missing;

// Writes node_modules/workflint with the launcher into <root>, and the fake
// platform package unless <withPlatform> is false.
function install(root, withPlatform) {
  const launcher = path.join(root, "node_modules", "workflint", "bin", "workflint.js");
  fs.mkdirSync(path.dirname(launcher), { recursive: true });
  fs.copyFileSync(launcherSource, launcher);
  fs.writeFileSync(
    path.join(root, "node_modules", "workflint", "package.json"),
    JSON.stringify({ name: "workflint", version: "1.2.3" }),
  );
  if (withPlatform) {
    const dir = path.join(root, "node_modules", ...platformPackage.split("/"));
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(
      path.join(dir, "package.json"),
      JSON.stringify({ name: platformPackage, version: "1.2.3" }),
    );
    const binary = path.join(dir, windows ? "workflint.exe" : "workflint");
    try {
      fs.linkSync(process.execPath, binary);
    } catch {
      fs.copyFileSync(process.execPath, binary);
    }
  }
  return launcher;
}

// Runs the launcher with <args>. <options.env> replaces the environment and
// <options.preload> is a module that node loads before the launcher.
function run(launcher, args, options = {}) {
  const nodeArgs = options.preload ? ["--require", options.preload] : [];
  return spawnSync(process.execPath, [...nodeArgs, launcher, ...args], {
    encoding: "utf8",
    env: options.env ?? process.env,
  });
}

// The environment of this process without WORKFLINT_INSTALLED_FROM.
function envWithout() {
  const env = { ...process.env };
  for (const key of Object.keys(env)) {
    if (key.toUpperCase() === "WORKFLINT_INSTALLED_FROM") delete env[key];
  }
  return env;
}

before(() => {
  work = fs.mkdtempSync(path.join(os.tmpdir(), "workflint-launcher-"));
  installed = install(path.join(work, "installed"), true);
  missing = install(path.join(work, "missing"), false);
});

after(() => {
  fs.rmSync(work, { recursive: true, force: true });
});

describe("exit code", () => {
  for (const code of [0, 1, 3]) {
    test(`the launcher exits with the binary's exit code ${code}`, () => {
      const result = run(installed, ["-e", `process.exit(${code})`]);
      assert.equal(result.signal, null);
      assert.equal(result.status, code, result.stderr);
    });
  }

  test("the launcher passes its arguments to the binary unchanged", () => {
    const args = ["-e", "console.log(JSON.stringify(process.argv.slice(1)))", "--", "-format", "{{json .}}", "a b"];
    const result = run(installed, args);
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(JSON.parse(result.stdout), ["-format", "{{json .}}", "a b"]);
  });
});

describe("signals", { skip: windows && "Windows has no POSIX signals" }, () => {
  for (const signal of ["SIGTERM", "SIGINT", "SIGKILL"]) {
    test(`the launcher dies of ${signal} when the binary does`, () => {
      const result = run(installed, ["-e", `process.kill(process.pid, "${signal}")`]);
      assert.equal(result.status, null, result.stderr);
      assert.equal(result.signal, signal);
    });
  }

  // node ignores SIGPIPE, so the launcher cannot die of it. The preload gives
  // SIGUSR2 a handler in the launcher's process to stand in for that case.
  test("the launcher exits with 128 plus the signal number when it cannot die of the signal", () => {
    const preload = path.join(work, "handle-sigusr2.js");
    fs.writeFileSync(preload, 'process.on("SIGUSR2", () => {});\n');
    const result = run(installed, ["-e", 'process.kill(process.pid, "SIGUSR2")'], { preload });
    assert.equal(result.signal, null);
    assert.equal(result.status, 128 + os.constants.signals.SIGUSR2, result.stderr);
  });
});

describe("WORKFLINT_INSTALLED_FROM", () => {
  const print = ["-e", "process.stdout.write(String(process.env.WORKFLINT_INSTALLED_FROM))"];

  test("the launcher sets WORKFLINT_INSTALLED_FROM to installed from npm", () => {
    const result = run(installed, print, { env: envWithout() });
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout, "installed from npm");
  });

  test("the launcher keeps an existing WORKFLINT_INSTALLED_FROM", () => {
    const env = { ...envWithout(), WORKFLINT_INSTALLED_FROM: "installed by a wrapper" };
    const result = run(installed, print, { env });
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout, "installed by a wrapper");
  });

  test("the launcher keeps an empty WORKFLINT_INSTALLED_FROM", () => {
    const env = { ...envWithout(), WORKFLINT_INSTALLED_FROM: "" };
    const result = run(installed, print, { env });
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout, "");
  });
});

describe("a missing binary", () => {
  test("the launcher names the missing platform package and exits with 3", () => {
    const result = run(missing, ["-version"]);
    assert.equal(result.status, 3);
    assert.equal(result.stdout, "");
    assert.match(result.stderr, new RegExp(`${platformPackage.replace("/", "\\/")}`));
    assert.match(result.stderr, /--omit=optional/);
    assert.match(result.stderr, /npm install --save-dev workflint/);
  });

  test("the launcher names the platform it does not support and exits with 3", () => {
    const preload = path.join(work, "aix.js");
    fs.writeFileSync(
      preload,
      'Object.defineProperty(process, "platform", { value: "aix" });\n' +
        'Object.defineProperty(process, "arch", { value: "ppc64" });\n',
    );
    const result = run(installed, ["-version"], { preload });
    assert.equal(result.status, 3);
    assert.equal(result.stdout, "");
    assert.match(result.stderr, /aix ppc64/);
    assert.match(result.stderr, /@stokehold\/workflint-linux-x64/);
    assert.match(result.stderr, /releases/);
  });
});
