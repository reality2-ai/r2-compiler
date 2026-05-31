#!/usr/bin/env bash
# webapp/build-wasm.sh — build the WASM bundle for r2-composer's webapp.
#
# Output goes to webapp/dist/wasm/ (alongside dist/manifest.json from
# the Python catalogue builder). The HTML at webapp/index.html imports
# webapp/dist/wasm/r2_composer_webapp.js at runtime.
#
# Requires:
# - rustup toolchain (any recent stable)
# - wasm32-unknown-unknown target  →  rustup target add wasm32-unknown-unknown
# - wasm-pack                      →  cargo install wasm-pack
#
# Use:
#   webapp/build-wasm.sh             (release, minified — default)
#   webapp/build-wasm.sh debug       (faster build, no optimisation)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MODE="${1:-release}"

cd "$REPO_ROOT"

case "$MODE" in
  release) FLAG="--release" ;;
  debug)   FLAG="--dev"     ;;
  *) echo "Usage: $0 [release|debug]" >&2; exit 1 ;;
esac

echo "[wasm] wasm-pack build webapp/crate (mode=$MODE) → webapp/dist/wasm/"
wasm-pack build webapp/crate \
  $FLAG \
  --target web \
  --out-dir ../dist/wasm \
  --out-name r2_composer_webapp

echo
echo "[wasm] Output:"
ls -la "$REPO_ROOT/webapp/dist/wasm/"
echo
echo "[wasm] Done. Reload the webapp (webapp/run.sh) to pick up changes."
