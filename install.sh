#!/usr/bin/env bash
#
# install.sh — bootstrap a fresh r2-composer checkout.
#
# Strategy: check what's there, install what's safe + idempotent, point
# at instructions for anything that needs operator-side decisions
# (npm/Anthropic account, gh auth, group membership, …). Re-runnable.
#
# Usage:
#   ./install.sh            # check + install where possible
#   ./install.sh --check    # report only; don't install anything
#   ./install.sh --skip-rust  # skip the rustup install step
#

set -euo pipefail

# ── Argument parsing ──────────────────────────────────────────────────

CHECK_ONLY=0
SKIP_RUST=0
for arg in "$@"; do
  case "$arg" in
    --check)      CHECK_ONLY=1 ;;
    --skip-rust)  SKIP_RUST=1 ;;
    -h|--help)
      head -16 "$0" | tail -14 | sed 's/^# \{0,1\}//'
      exit 0 ;;
    *) echo "Unknown arg: $arg" >&2; exit 1 ;;
  esac
done

REPO_ROOT="$(cd "$(dirname "$0")" && pwd)"

# ── Pretty output ─────────────────────────────────────────────────────

C_OK="\033[32m"      # green
C_WARN="\033[33m"    # yellow
C_ERR="\033[31m"     # red
C_DIM="\033[2m"
C_BOLD="\033[1m"
C_RESET="\033[0m"

ok()    { printf "  ${C_OK}✓${C_RESET}  %s\n" "$1"; }
warn()  { printf "  ${C_WARN}⚠${C_RESET}  %s\n" "$1"; }
miss()  { printf "  ${C_ERR}✗${C_RESET}  %s\n" "$1"; }
note()  { printf "  ${C_DIM}↪  %s${C_RESET}\n" "$1"; }
step()  { printf "\n${C_BOLD}── %s ──${C_RESET}\n" "$1"; }

MISSING_OPTIONAL=0
MISSING_REQUIRED=0

# ── OS check ──────────────────────────────────────────────────────────

step "OS"

OS="$(uname -s)"
case "$OS" in
  Linux)  ok "Linux — primary platform, all features supported" ;;
  Darwin) warn "macOS — works except for the USB watcher (sysfs-only). Flash + chat + compose are fine." ;;
  *)      warn "OS '$OS' — untested. Linux is primary; macOS works for everything except USB watch." ;;
esac

# ── Rust toolchain ────────────────────────────────────────────────────

step "Rust toolchain"

if command -v rustup >/dev/null 2>&1; then
  ok "rustup ($(rustup --version 2>&1 | head -1))"
  rustup_present=1
else
  miss "rustup — not installed"
  rustup_present=0
fi

if [ "$rustup_present" -eq 0 ] && [ "$SKIP_RUST" -eq 0 ] && [ "$CHECK_ONLY" -eq 0 ]; then
  note "Installing rustup via the official installer (will prompt for confirmation)…"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
  # shellcheck source=/dev/null
  . "$HOME/.cargo/env" 2>/dev/null || true
  ok "rustup installed — restart your shell or 'source ~/.cargo/env' to pick it up"
fi

if command -v cargo >/dev/null 2>&1; then
  ok "cargo ($(cargo --version 2>&1))"
else
  miss "cargo — install rustup (above)"
  MISSING_REQUIRED=$((MISSING_REQUIRED + 1))
fi

# wasm32 target — needed for the webapp WASM crate.
if rustup target list --installed 2>/dev/null | grep -q '^wasm32-unknown-unknown$'; then
  ok "wasm32-unknown-unknown target"
elif command -v rustup >/dev/null 2>&1 && [ "$CHECK_ONLY" -eq 0 ]; then
  note "Adding wasm32-unknown-unknown target…"
  rustup target add wasm32-unknown-unknown
  ok "wasm32 target added"
else
  miss "wasm32-unknown-unknown target"
  note "Install: rustup target add wasm32-unknown-unknown"
fi

# wasm-pack — needed by webapp/build-wasm.sh.
if command -v wasm-pack >/dev/null 2>&1; then
  ok "wasm-pack ($(wasm-pack --version 2>&1))"
elif command -v cargo >/dev/null 2>&1 && [ "$CHECK_ONLY" -eq 0 ]; then
  note "Installing wasm-pack via cargo install (~1-2 min)…"
  cargo install wasm-pack
  ok "wasm-pack installed"
else
  miss "wasm-pack"
  note "Install: cargo install wasm-pack"
fi

# ── Python + tooling ──────────────────────────────────────────────────

step "Python tooling"

if command -v python3 >/dev/null 2>&1; then
  PY_VER="$(python3 --version 2>&1)"
  ok "python3 ($PY_VER)"
else
  miss "python3 — required by tools/build-catalogue-index.py + the WS test scripts"
  MISSING_REQUIRED=$((MISSING_REQUIRED + 1))
  note "Install your distro's python3 package (>= 3.10)"
fi

# esptool — for USB first-install.
if command -v esptool >/dev/null 2>&1 || command -v esptool.py >/dev/null 2>&1; then
  ESPTOOL_BIN="$(command -v esptool 2>/dev/null || command -v esptool.py)"
  ok "esptool ($ESPTOOL_BIN)"
else
  warn "esptool — USB first-install will fail until you install it"
  MISSING_OPTIONAL=$((MISSING_OPTIONAL + 1))
  note "Install: pip install esptool   (or pipx install esptool, or your distro's package)"
  note "DO NOT install espflash — SPEC-APIARY-FLASH §4.2 mandates esptool"
fi

# Python websockets — used by the orchestrator-side WS test scripts.
if python3 -c 'import websockets' 2>/dev/null; then
  ok "python websockets"
else
  warn "python websockets — needed only if you want to run the WS-level test scripts"
  MISSING_OPTIONAL=$((MISSING_OPTIONAL + 1))
  note "Install: pip install websockets"
fi

# ── Claude CLI ────────────────────────────────────────────────────────

step "Claude Code CLI"

if command -v claude >/dev/null 2>&1; then
  CLAUDE_VER="$(claude --version 2>&1 | head -1)"
  ok "claude ($CLAUDE_VER)"
  note "Auth check: claude must be logged in (anthropic-api or pro account)"
else
  miss "claude — the chat + authoring flow cannot run without it"
  MISSING_REQUIRED=$((MISSING_REQUIRED + 1))
  note "Install: npm install -g @anthropic-ai/claude-code"
  note "Then:    claude     (and follow the auth prompt)"
fi

# ── Git + GitHub ──────────────────────────────────────────────────────

step "Git + GitHub"

if command -v git >/dev/null 2>&1; then
  ok "git ($(git --version 2>&1))"
  if git config --global user.name >/dev/null 2>&1 && git config --global user.email >/dev/null 2>&1; then
    GN="$(git config --global user.name)"
    GE="$(git config --global user.email)"
    ok "git user.name=\"$GN\" user.email=\"$GE\""
  else
    warn "git user.name / user.email not set globally — apiary.create will fail"
    MISSING_OPTIONAL=$((MISSING_OPTIONAL + 1))
    note "Set: git config --global user.name \"Your Name\""
    note "     git config --global user.email \"you@example.com\""
  fi
else
  miss "git — required"
  MISSING_REQUIRED=$((MISSING_REQUIRED + 1))
fi

# gh CLI — for the apiary.git.publish flow. Optional.
if command -v gh >/dev/null 2>&1; then
  ok "gh ($(gh --version 2>&1 | head -1))"
  if gh auth status >/dev/null 2>&1; then
    ok "gh authenticated"
  else
    warn "gh not authenticated — apiary.git.publish will fail until you 'gh auth login'"
    MISSING_OPTIONAL=$((MISSING_OPTIONAL + 1))
  fi
else
  warn "gh CLI — optional, needed for apiary.git.publish"
  MISSING_OPTIONAL=$((MISSING_OPTIONAL + 1))
  note "Install: see https://cli.github.com/   (then: gh auth login)"
fi

# ── Linux-specific: USB group ─────────────────────────────────────────

if [ "$OS" = "Linux" ]; then
  step "Linux USB access"

  if groups "$USER" | grep -qw dialout; then
    ok "user '$USER' is in the dialout group — /dev/ttyACM* readable without sudo"
  else
    warn "user '$USER' is NOT in the dialout group — flashing will need sudo or group membership"
    MISSING_OPTIONAL=$((MISSING_OPTIONAL + 1))
    note "Add: sudo usermod -aG dialout $USER   (then log out and back in)"
  fi
fi

# ── Sanity build ──────────────────────────────────────────────────────

if [ "$CHECK_ONLY" -eq 0 ] && [ "$MISSING_REQUIRED" -eq 0 ]; then
  step "Sanity build"

  if command -v cargo >/dev/null 2>&1; then
    note "cargo build -p orchestrator (first build can take a few minutes)"
    if (cd "$REPO_ROOT" && cargo build -p orchestrator 2>&1 | tail -3); then
      ok "orchestrator builds clean"
    else
      miss "orchestrator build failed — see output above"
      MISSING_REQUIRED=$((MISSING_REQUIRED + 1))
    fi
  fi

  if command -v wasm-pack >/dev/null 2>&1; then
    note "webapp/build-wasm.sh"
    if "$REPO_ROOT/webapp/build-wasm.sh" 2>&1 | tail -2; then
      ok "WASM bundle built"
    else
      warn "WASM bundle build failed — class-hash badges will fall back to text-only"
      MISSING_OPTIONAL=$((MISSING_OPTIONAL + 1))
    fi
  fi
fi

# ── Summary ───────────────────────────────────────────────────────────

step "Summary"

if [ "$MISSING_REQUIRED" -eq 0 ] && [ "$MISSING_OPTIONAL" -eq 0 ]; then
  printf "${C_OK}All prerequisites met.${C_RESET}\n\n"
  printf "Start the orchestrator:\n"
  printf "  ${C_BOLD}./orchestrator/run.sh${C_RESET}\n\n"
  printf "Then open: ${C_BOLD}http://localhost:21050/webapp/index.html${C_RESET}\n"
elif [ "$MISSING_REQUIRED" -eq 0 ]; then
  printf "${C_OK}Required prerequisites met.${C_RESET} ${C_WARN}%d optional item(s) flagged above.${C_RESET}\n\n" "$MISSING_OPTIONAL"
  printf "You can run ${C_BOLD}./orchestrator/run.sh${C_RESET} now — review the warnings for what's missing.\n"
else
  printf "${C_ERR}%d required prerequisite(s) missing.${C_RESET} Install them and re-run.\n" "$MISSING_REQUIRED"
  exit 1
fi
