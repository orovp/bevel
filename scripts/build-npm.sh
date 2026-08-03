#!/usr/bin/env bash
# Build the Linux binaries and assemble the npm packages.
#
# Linux-only is what makes this cheap: the usual pain of shipping a Rust binary
# through npm is a matrix across macOS, Windows and two libcs. Here it is three
# triples, so the whole pipeline is a loop rather than a build system.
#
#   ./scripts/build-npm.sh            build what this host can, assemble packages
#   ./scripts/build-npm.sh --publish  ... and publish them in dependency order
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/target/npm"
PUBLISH="${1:-}"

VERSION="$(grep -m1 '^version' "$ROOT/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')"
echo "version $VERSION"

# rust target -> npm package suffix
TARGETS=(
  "x86_64-unknown-linux-gnu:linux-x64-gnu:x64:glibc"
  "aarch64-unknown-linux-gnu:linux-arm64-gnu:arm64:glibc"
  "x86_64-unknown-linux-musl:linux-x64-musl:x64:musl"
)

rm -rf "$OUT"
mkdir -p "$OUT"

built=()
for spec in "${TARGETS[@]}"; do
  IFS=: read -r target suffix arch libc <<<"$spec"

  if ! rustup target list --installed | grep -qx "$target"; then
    echo "skip $target (run: rustup target add $target)"
    continue
  fi
  echo "build $target"
  # Cross-linking needs a linker per target; a missing one is a skip, not a
  # failure, so a developer can assemble what their host supports and let CI
  # produce the rest.
  if ! cargo build --release --target "$target" --manifest-path "$ROOT/Cargo.toml"; then
    echo "skip $target (no working linker on this host)"
    continue
  fi

  pkg="$OUT/bevel-$suffix"
  mkdir -p "$pkg/bin"
  cp "$ROOT/target/$target/release/bevel" "$pkg/bin/bevel"
  chmod +x "$pkg/bin/bevel"

  # `libc` narrows glibc vs musl so npm never installs the wrong one on Alpine.
  cat >"$pkg/package.json" <<JSON
{
  "name": "@orovp/bevel-$suffix",
  "version": "$VERSION",
  "description": "bevel binary for $suffix",
  "license": "MIT",
  "os": ["linux"],
  "cpu": ["$arch"],
  "libc": ["$libc"],
  "files": ["bin/bevel"],
  "main": "bin/bevel"
}
JSON
  built+=("$pkg")
done

# The wrapper, with its version and its optionalDependency versions in step.
main="$OUT/bevel"
mkdir -p "$main/bin"
cp "$ROOT/npm/bevel/bin/bevel.js" "$main/bin/bevel.js"
cp "$ROOT/npm/bevel/bin/postinstall.js" "$main/bin/postinstall.js"
cp "$ROOT/README.md" "$main/README.md"
sed "s/\"version\": \"[^\"]*\"/\"version\": \"$VERSION\"/g; \
     s/\"0\.1\.0\"/\"$VERSION\"/g" \
  "$ROOT/npm/bevel/package.json" >"$main/package.json"

echo
echo "assembled in $OUT:"
ls -1 "$OUT"

if [ "$PUBLISH" = "--publish" ]; then
  # Platform packages first: the wrapper depends on them, and publishing it
  # first would leave a window where an install resolves nothing.
  for pkg in "${built[@]}"; do
    (cd "$pkg" && npm publish --access public)
  done
  (cd "$main" && npm publish --access public)
else
  echo
  echo "dry run. To publish: $0 --publish"
  echo "Platform packages must go first — the wrapper depends on them."
fi
