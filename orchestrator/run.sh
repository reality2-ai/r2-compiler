#!/usr/bin/env bash
# orchestrator/run.sh — build + launch the r2-composer orchestrator.
#
# Listens on http://localhost:21050 by default. Opens the webapp at
# http://localhost:21050/webapp/index.html.
#
# Use:
#   orchestrator/run.sh                    # release build + run on :21050
#   orchestrator/run.sh debug              # debug build (faster compile)
#   R2_COMPILER_PORT=21099 orchestrator/run.sh    # override port

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PORT="${R2_COMPILER_PORT:-21050}"
MODE="${1:-release}"

cd "$REPO_ROOT"

case "$MODE" in
  release) FLAG="--release" ; BIN_DIR="release"  ;;
  debug)   FLAG=""          ; BIN_DIR="debug"    ;;
  *) echo "Usage: $0 [release|debug]" >&2; exit 1 ;;
esac

# Refresh the catalogue manifest (build script — independent of cargo).
python3 "$REPO_ROOT/tools/build-catalogue-index.py"

# Ensure the WASM bundle exists; build if missing AND wasm-pack present.
if [ ! -f "$REPO_ROOT/webapp/dist/wasm/r2_composer_webapp.js" ]; then
  if command -v wasm-pack >/dev/null 2>&1; then
    echo "[run] WASM bundle missing — building..."
    "$REPO_ROOT/webapp/build-wasm.sh" release
  else
    echo "[run] WARN: wasm-pack not installed and WASM bundle missing."
    echo "[run]       Class-hash badges in the webapp will fall back to text-only."
  fi
fi

echo "[run] cargo build -p orchestrator $FLAG"
cargo build -p orchestrator $FLAG

echo
echo "[run] Starting r2-composer-orchestrator on http://localhost:$PORT"
echo "[run] Open: http://localhost:$PORT/webapp/index.html"
echo "[run]       (or visit the root and you'll be redirected)"
echo
echo "[run] Ctrl-C to stop."
echo

exec "$REPO_ROOT/target/$BIN_DIR/r2-composer-orchestrator" \
  --port "$PORT" \
  --webapp-root "$REPO_ROOT/webapp"
