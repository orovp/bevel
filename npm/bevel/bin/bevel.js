#!/usr/bin/env node
// Locate the platform binary installed as an optionalDependency and hand over
// to it. npm resolved exactly one of them from `os`/`cpu`, so there is nothing
// to choose at runtime beyond finding it.
//
// `execve`-style handover rather than a child process: the binary is invoked
// from editor hooks on the critical path of tool calls, so an extra Node
// process in the middle would be pure latency, and signal and exit-code
// forwarding would become our problem instead of the kernel's.

const { spawnSync } = require("node:child_process");
const { createRequire } = require("node:module");

const TRIPLES = {
  "linux-x64": ["linux-x64-gnu", "linux-x64-musl"],
  "linux-arm64": ["linux-arm64-gnu"],
};

function candidates() {
  const key = `${process.platform}-${process.arch}`;
  const triples = TRIPLES[key];
  if (!triples) {
    return { key, packages: [] };
  }
  return { key, packages: triples.map((t) => `@orovp/bevel-${t}`) };
}

function resolveBinary() {
  const require_ = createRequire(__filename);
  const { key, packages } = candidates();
  const tried = [];

  for (const pkg of packages) {
    try {
      // Resolve through the package's own entry point so npm's layout, pnpm's
      // symlinks and hoisting all work without us guessing at node_modules.
      return require_.resolve(`${pkg}/bin/bevel`);
    } catch {
      tried.push(pkg);
    }
  }

  const detail = tried.length
    ? `tried: ${tried.join(", ")}`
    : `no build is published for ${key}`;
  throw new Error(
    `bevel: could not find its platform binary (${detail}).\n` +
      `This usually means optional dependencies were skipped.\n` +
      `  npm install --include=optional @orovp/bevel\n` +
      `Or install it directly from source:\n` +
      `  cargo install bevel`
  );
}

let binary;
try {
  binary = resolveBinary();
} catch (err) {
  process.stderr.write(`${err.message}\n`);
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });

// A signalled child must look signalled to whatever invoked us, or a hook that
// was interrupted will read as a clean exit.
if (result.error) {
  process.stderr.write(`bevel: failed to run ${binary}: ${result.error.message}\n`);
  process.exit(1);
}
if (result.signal) {
  process.kill(process.pid, result.signal);
}
process.exit(result.status === null ? 1 : result.status);
