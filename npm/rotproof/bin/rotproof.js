#!/usr/bin/env node
// Runs the binary of the platform package npm installed beside this one (one of the optionalDependencies, each of
// which npm installs only on its own os and cpu), passing the arguments, the standard streams and the exit code
// through: rotproof check exits 1 on a broken rule and 2 on a wrong command line, and a CI reads that
"use strict";

const { spawnSync } = require("node:child_process");

const platform = `${process.platform}-${process.arch}`;
const binary = process.platform === "win32" ? "rotproof.exe" : "rotproof";

let path;
try {
  path = require.resolve(`@rotproof/${platform}/${binary}`);
} catch {
  console.error(
    `rotproof: the package @rotproof/${platform} is not installed. Rotproof is released for linux-x64 and ` +
      "win32-x64 only, and npm installs the package of its platform unless optional dependencies are left out " +
      "(--omit=optional)",
  );
  process.exit(1);
}

const result = spawnSync(path, process.argv.slice(2), { stdio: "inherit" });
if (result.error) {
  console.error(`rotproof: could not run ${path}: ${result.error.message}`);
  process.exit(1);
}
if (result.signal) {
  process.kill(process.pid, result.signal);
}
process.exit(result.status);
