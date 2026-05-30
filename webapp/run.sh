#!/usr/bin/env bash
# webapp/run.sh — serve the catalogue preview on http://localhost:8080.
#
# 1. Rebuilds the catalogue manifest from the on-disk catalogue tree.
# 2. Serves the webapp directory over HTTP (so fetch() works on file://).
#
# Phase 2-preview: static page; no WASM hive yet. Run from anywhere.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PORT="${R2_COMPILER_PORT:-8080}"

# 1. Build / refresh the manifest from the on-disk catalogue.
python3 "$REPO_ROOT/tools/build-catalogue-index.py"

# 2. Serve the REPO ROOT (not just webapp/) so the page can fetch
#    ../catalogue/... files relative to webapp/index.html.
echo
echo "Serving from $REPO_ROOT on http://localhost:$PORT/webapp/"
echo "Open: http://localhost:$PORT/webapp/index.html"
echo
exec python3 -m http.server "$PORT" --directory "$REPO_ROOT"
