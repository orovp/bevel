#!/usr/bin/env node
// Warm the method cache at install time.
//
// The method (skills, subagents, packs, templates) lives in the GitHub
// repository rather than in the binary, so that editing a markdown file needs no
// release. The cost is that a machine which has never fetched and cannot reach
// the network has no method at all. Fetching here closes most of that gap: if
// npm just resolved a package, the network is demonstrably reachable.
//
// Never fatal. A failed warm-up leaves a working binary that prints an
// actionable message on first use, which is a far better outcome than a failed
// install.

const { spawnSync } = require("node:child_process");
const { createRequire } = require("node:module");

if (process.env.HARNESS_SKIP_POSTINSTALL) {
  process.exit(0);
}

function findBinary() {
  const require_ = createRequire(__filename);
  for (const pkg of [
    "@orovp/bevel-linux-x64-gnu",
    "@orovp/bevel-linux-arm64-gnu",
    "@orovp/bevel-linux-x64-musl",
  ]) {
    try {
      return require_.resolve(`${pkg}/bin/bevel`);
    } catch {
      // Wrong platform, or optional dependencies were skipped.
    }
  }
  return null;
}

const binary = findBinary();
if (!binary) {
  // The shim prints a precise diagnosis when actually invoked; saying it twice
  // during an install nobody reads is just noise.
  process.exit(0);
}

const result = spawnSync(binary, ["method", "fetch"], {
  stdio: "inherit",
  timeout: 60_000,
});

if (result.status !== 0) {
  process.stderr.write(
    "\nbevel: could not pre-fetch the method (that is not fatal).\n" +
      "  Run `bevel method fetch` when you have network access, or point at a\n" +
      "  local checkout with [method] path in ~/.config/bevel/config.toml\n"
  );
}
process.exit(0);
