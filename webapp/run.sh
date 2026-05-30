#!/usr/bin/env bash
# webapp/run.sh — serve the catalogue preview on http://localhost:8080.
#
# 1. Rebuilds the catalogue manifest from the on-disk catalogue tree.
# 2. Builds the WASM bundle if it's missing (requires wasm-pack).
# 3. Serves the webapp directory over HTTP (so fetch() works on file://).
#
# Phase 2-preview (with WASM foundation): static catalogue browser + a
# minimal WASM module exposing class-hash computation via wasm-bindgen.
# Full WASM R2 hive is Phase 2-full.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PORT="${R2_COMPILER_PORT:-8080}"

# 1. Build / refresh the manifest from the on-disk catalogue.
python3 "$REPO_ROOT/tools/build-catalogue-index.py"

# 2. Ensure the WASM bundle exists. Skip if already built; operators can
#    force a rebuild via `webapp/build-wasm.sh` directly.
if [ ! -f "$REPO_ROOT/webapp/dist/wasm/r2_compiler_webapp.js" ]; then
  if command -v wasm-pack >/dev/null 2>&1; then
    echo "[run] WASM bundle missing — building..."
    "$REPO_ROOT/webapp/build-wasm.sh" release
  else
    echo "[run] WARN: wasm-pack not installed and WASM bundle missing."
    echo "[run]       The catalogue browser will load with reduced features"
    echo "[run]       (class-hash display falls back to text-only)."
    echo "[run]       Install: cargo install wasm-pack"
  fi
fi

# 3. Serve the REPO ROOT (not just webapp/) so the page can fetch
#    ../catalogue/... files relative to webapp/index.html.
echo
echo "Serving from $REPO_ROOT on http://localhost:$PORT/webapp/"
echo "Open: http://localhost:$PORT/webapp/index.html"
echo
exec python3 -m http.server "$PORT" --directory "$REPO_ROOT"
