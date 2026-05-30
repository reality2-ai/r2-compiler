#!/usr/bin/env bash
# tools/sync-catalogue.sh — populate r2-compiler's catalogue from sibling repos.
#
# This script handles only the DETERMINISTIC parts of catalogue population:
#   - Vendoring R2 protocol crates from r2-core/crates/ into crates/
#   - Vendoring the core crypto plugin from r2-core/plugins/crypto/ into crates/
#   - Copying board template files from r2-workshop/firmware/<arch>/<carrier>/
#   - Copying the rocker-sensor ensemble score from r2-workshop/ensemble/sensor.yaml
#   - Writing crates/_VERSIONS.toml with the source commits
#
# The judgement parts — authoring per-plugin Cargo crates from r2-workshop's
# inline firmware code, extracting per-sentant YAML, writing board.toml from
# observation of Cargo.toml + main.rs — are LEFT FOR THE AUTHORING FLOW.
# A future fresh CC session reads each board/ensemble entry's AI-CONTEXT.md
# and completes the work through dialog with the operator.
#
# Usage:
#   tools/sync-catalogue.sh
#   R2_CORE=/path/to/r2-core R2_WORKSHOP=/path/to/r2-workshop tools/sync-catalogue.sh
#
# Conjecture C-3 (PLAN.md): vendoring is cheap enough to do per-release.
# Falsifies if this script's run time + churn dominates real work.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
R2_CORE="${R2_CORE:-$REPO_ROOT/../r2-core}"
R2_WORKSHOP="${R2_WORKSHOP:-$REPO_ROOT/../r2-workshop}"

log() { printf '[sync] %s\n' "$*"; }
warn() { printf '[sync] WARN: %s\n' "$*" >&2; }
err() { printf '[sync] ERROR: %s\n' "$*" >&2; exit 1; }

[ -d "$R2_CORE" ]     || err "r2-core not found at $R2_CORE (set R2_CORE)"
[ -d "$R2_WORKSHOP" ] || err "r2-workshop not found at $R2_WORKSHOP (set R2_WORKSHOP)"

log "Sources:"
log "  R2_CORE     = $R2_CORE"
log "  R2_WORKSHOP = $R2_WORKSHOP"
log "  REPO_ROOT   = $REPO_ROOT"

# ──────────────────────────────────────────────────────────────────────────────
# 1. Vendor R2 protocol crates
# ──────────────────────────────────────────────────────────────────────────────

PROTOCOL_CRATES=(
  r2-engine
  r2-fnv
  r2-cbor
  r2-wire
  r2-trust
  r2-route
  r2-def
  r2-ensemble
  r2-wasm
)

log "Vendoring ${#PROTOCOL_CRATES[@]} R2 protocol crates ..."
for c in "${PROTOCOL_CRATES[@]}"; do
  src="$R2_CORE/crates/$c"
  dst="$REPO_ROOT/crates/$c"
  if [ ! -d "$src" ]; then
    warn "  $c — missing at $src, skipping"
    continue
  fi
  rm -rf "$dst"
  mkdir -p "$dst"
  # Copy crate contents (skip target/, .git, anything in r2-core's .gitignore).
  rsync -a --exclude='target' --exclude='Cargo.lock' --exclude='.git*' "$src/" "$dst/"
  log "  $c"
done

# ──────────────────────────────────────────────────────────────────────────────
# 2. Vendor core plugins (always-linked, NOT in catalogue)
#
# Per [[feedback-core-vs-optin-plugins]] and [[feedback-two-part-canvas]] in
# memory: software-ed25519 is core R2 infrastructure (every sensor signs its
# announce frame). It's packaged as a dual-mode plugin upstream but it's
# always-available infrastructure here, not a catalogue choice.
# ──────────────────────────────────────────────────────────────────────────────

CORE_PLUGINS=(
  "crypto/software-ed25519"
)

log "Vendoring ${#CORE_PLUGINS[@]} core plugins into crates/ ..."
for p in "${CORE_PLUGINS[@]}"; do
  src="$R2_CORE/plugins/$p"
  # Flatten the category into the crate name per R2-PLUGIN §12.5: r2-plugin-<category>-<name>
  category="${p%%/*}"
  name="${p##*/}"
  crate_name="r2-plugin-${category}-${name}"
  dst="$REPO_ROOT/crates/$crate_name"
  if [ ! -d "$src" ]; then
    warn "  $p — missing at $src, skipping"
    continue
  fi
  rm -rf "$dst"
  mkdir -p "$dst"
  rsync -a --exclude='target' --exclude='Cargo.lock' --exclude='.git*' "$src/" "$dst/"
  # Patch path deps. In r2-core the dep was `path = "../../../crates/r2-engine"`
  # (up three levels from r2-core/plugins/crypto/software-ed25519/ to r2-core root).
  # In r2-compiler the plugin sits at crates/<crate_name>/ so the dep is
  # `path = "../r2-engine"` (one level up to crates/).
  if [ -f "$dst/Cargo.toml" ]; then
    sed -i.bak 's|path = "\.\./\.\./\.\./crates/|path = "../|g' "$dst/Cargo.toml"
    rm -f "$dst/Cargo.toml.bak"
  fi
  log "  $p → crates/$crate_name (with path deps re-anchored)"
done

# ──────────────────────────────────────────────────────────────────────────────
# 3. Carrier-board templates
#
# Copies the raw per-carrier template files from r2-workshop/firmware/<arch>/<carrier>/
# into catalogue/boards/<slug>/templates/. The Cargo.toml is copied as
# Cargo.toml.tera (a placeholder; the Compiler sentant will render it per-build
# with the right vendored-crate paths).
#
# The structured board.toml file is NOT generated here — it requires reading
# Cargo.toml + sdkconfig + main.rs + datasheet and synthesising metadata. That's
# an authoring task scoped to each board's `+ Author` flow.
# ──────────────────────────────────────────────────────────────────────────────

declare -A BOARD_SOURCES=(
  ["esp32-s3-devkitc"]="esp32-s3/devkitc"
  ["esp32-s3-xiao"]="esp32-s3/xiao"
  ["esp32-c6-dfr1117"]="esp32-c6/dfr1117"
)

log "Copying carrier-board template files ..."
for slug in "${!BOARD_SOURCES[@]}"; do
  src="$R2_WORKSHOP/firmware/${BOARD_SOURCES[$slug]}"
  dst="$REPO_ROOT/catalogue/boards/$slug"
  if [ ! -d "$src" ]; then
    warn "  $slug — missing at $src, skipping"
    continue
  fi
  mkdir -p "$dst/templates" "$dst/templates/.cargo" "$dst/datasheets"
  # Template-able files
  for tpl in partitions.csv sdkconfig.defaults build.rs rust-toolchain.toml wifi_config.toml.example; do
    [ -f "$src/$tpl" ] && cp "$src/$tpl" "$dst/templates/$tpl"
  done
  # Cargo.toml → Cargo.toml.tera (Compiler sentant will template this)
  if [ -f "$src/Cargo.toml" ]; then
    cp "$src/Cargo.toml" "$dst/templates/Cargo.toml.tera"
  fi
  # .cargo/config.toml
  if [ -f "$src/.cargo/config.toml" ]; then
    cp "$src/.cargo/config.toml" "$dst/templates/.cargo/config.toml"
  fi
  # Workshop's HARDWARE-WIRING-<UPPER-CARRIER>.md as reference material under datasheets/.
  # Convention: workshop uses upper-case carrier slug (DEVKITC, XIAO, DFR1117).
  carrier_upper=$(echo "${BOARD_SOURCES[$slug]##*/}" | tr '[:lower:]' '[:upper:]')
  hw="$R2_WORKSHOP/specifications/HARDWARE-WIRING-${carrier_upper}.md"
  if [ -f "$hw" ]; then
    cp "$hw" "$dst/datasheets/HARDWARE-WIRING-${carrier_upper}.md"
  fi
  log "  $slug ← $src"
done

# ──────────────────────────────────────────────────────────────────────────────
# 4. Rocker-sensor ensemble score
#
# The full ensemble.yaml is copied; the per-plugin and per-sentant directories
# under catalogue/ensembles/rocker-sensor/{plugins,sentants}/ are NOT scaffolded
# automatically — they'll come from a Phase 1.3 authoring pass that reads the
# YAML and the r2-workshop firmware code, then produces R2-PLUGIN §12-conformant
# plugin crates and R2-DEF §2-conformant sentant scores.
# ──────────────────────────────────────────────────────────────────────────────

log "Copying rocker-sensor ensemble score ..."
src_score="$R2_WORKSHOP/ensemble/sensor.yaml"
if [ -f "$src_score" ]; then
  dst_ens="$REPO_ROOT/catalogue/ensembles/rocker-sensor"
  cp "$src_score" "$dst_ens/ensemble.yaml"
  cp "$src_score" "$REPO_ROOT/scores/rocker-sensor.yaml"
  log "  ensemble.yaml + scores/rocker-sensor.yaml"
else
  warn "  $src_score missing"
fi

# ──────────────────────────────────────────────────────────────────────────────
# 5. Versions manifest
# ──────────────────────────────────────────────────────────────────────────────

core_sha=$(git -C "$R2_CORE" rev-parse HEAD 2>/dev/null || echo "unknown")
workshop_sha=$(git -C "$R2_WORKSHOP" rev-parse HEAD 2>/dev/null || echo "unknown")
ts=$(date -u +%Y-%m-%dT%H:%M:%SZ)

cat > "$REPO_ROOT/crates/_VERSIONS.toml" <<EOF
# Auto-generated by tools/sync-catalogue.sh — do not edit by hand.
# Re-run sync-catalogue.sh to update.

[sync]
timestamp = "$ts"

[sources]
r2-core     = { path = "$R2_CORE", commit = "$core_sha" }
r2-workshop = { path = "$R2_WORKSHOP", commit = "$workshop_sha" }

[crates.protocol]
$(for c in "${PROTOCOL_CRATES[@]}"; do echo "$c = \"r2-core/crates/$c\""; done)

[crates.core_plugins]
$(for p in "${CORE_PLUGINS[@]}"; do
  cat="${p%%/*}"; name="${p##*/}"
  echo "r2-plugin-${cat}-${name} = \"r2-core/plugins/$p\""
done)

[boards]
$(for slug in "${!BOARD_SOURCES[@]}"; do
  echo "\"$slug\" = \"r2-workshop/firmware/${BOARD_SOURCES[$slug]}\""
done)

[ensembles]
rocker-sensor = "r2-workshop/ensemble/sensor.yaml"
EOF

log "Wrote crates/_VERSIONS.toml"
log "Sync complete."
log ""
log "What still needs authoring (run the AuthorPilot flow per board / ensemble):"
log "  - catalogue/boards/<each>/board.toml — synthesise from Cargo.toml + sdkconfig + main.rs"
log "  - catalogue/boards/<each>/BOARD.md   — narrative"
log "  - catalogue/ensembles/rocker-sensor/plugins/<cat>/<name>/ — author per R2-PLUGIN §12"
log "  - catalogue/ensembles/rocker-sensor/sentants/<Name>/     — author per R2-DEF §2"
log "  - catalogue/ensembles/rocker-sensor/ENSEMBLE.md + AI-CONTEXT.md"
