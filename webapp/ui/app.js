// r2-compiler — minimal catalogue preview app.
//
// Loads webapp/dist/manifest.json (built by tools/build-catalogue-index.py)
// AND the WASM module built from webapp/crate/ (`webapp/build-wasm.sh`).
// The WASM exposes class-hash computation + verification — every class
// string shown in the UI gets its FNV-1a-32 hash rendered alongside it,
// and pre-computed `class_hash` fields in the manifest are verified at
// load time.
//
// Phase 2-preview: a static catalogue browser. The full WASM R2 hive
// (Catalogue / Composition / SourceViewer / Builder / Author / Apiary
// sentants) is Phase 2-full work; this commit lays the WASM-build
// pipeline foundation those phases build on.

// ── WASM module — import bindings produced by wasm-pack ───────────────

import init, {
  fnv1a_32,
  class_hash_hex,
  verify_class_hash,
  version as wasmVersion,
} from "../dist/wasm/r2_compiler_webapp.js";

const $ = (id) => document.getElementById(id);

let manifest = null;
let currentEntry = null;
let currentFile = null;
let wasmReady = false;

// ── Boot ──────────────────────────────────────────────────────────────

(async function boot() {
  // Initialise the WASM module first so subsequent rendering can call
  // class_hash_hex / verify_class_hash synchronously.
  try {
    await init();
    wasmReady = true;
    console.info(`r2-compiler-webapp WASM loaded — v${wasmVersion()}`);
  } catch (err) {
    console.warn(`WASM init failed — class hashing falls back to text-only display: ${err.message}`);
    wasmReady = false;
  }

  try {
    const res = await fetch("dist/manifest.json", { cache: "no-store" });
    if (!res.ok) throw new Error(`manifest fetch ${res.status}`);
    manifest = await res.json();
  } catch (err) {
    document.body.innerHTML = `<div style="padding:40px;color:#ff8a8a;font-family:monospace">
      <h2>Failed to load catalogue manifest</h2>
      <p>${err.message}</p>
      <p>Run <code>python3 tools/build-catalogue-index.py</code> from the repo root and reload.</p>
    </div>`;
    return;
  }
  renderHeader(manifest.stats);
  renderBoards(manifest.boards);
  renderEnsembles(manifest.ensembles);
  attachGlobalHandlers();

  // Phase 1.7b: connect to the orchestrator's /r2 WebSocket and wire
  // the Build pane to it. The catalogue browser above stays
  // fully usable even if the WS fails (e.g. when serving the webapp
  // statically without the orchestrator).
  populateBuildSelects(manifest);
  connectR2();
})();

// ── Header ────────────────────────────────────────────────────────────

function renderHeader(stats) {
  const wasmTag = wasmReady ? ` · wasm v${wasmVersion()}` : "";
  $("stats").textContent = `${stats.boards} boards · ${stats.ensembles} ensembles · ${stats.plugins} plugins · ${stats.sentants} sentants${wasmTag}`;
  $("boards-count").textContent = String(stats.boards);
  $("ensembles-count").textContent = String(stats.ensembles);
}

// Render a class string with its FNV-1a-32 hash appended (when WASM is loaded).
// If the manifest carries a `pre_computed_hash` (e.g. `class_hash` in board.toml
// or apiary.toml), verify it matches — flag mismatches with a red warning.
function renderClass(klass, preComputedHash = null) {
  if (!klass) return "";
  if (!wasmReady) return escape(klass);
  const hash = class_hash_hex(klass);
  let warning = "";
  if (preComputedHash) {
    const ok = verify_class_hash(klass, preComputedHash);
    if (!ok) {
      warning = ` <span class="hash-warn" title="declared ${preComputedHash} but FNV-1a-32 computes ${hash}">⚠ hash mismatch</span>`;
    }
  }
  return `${escape(klass)} <span class="class-hash" title="FNV-1a-32(${klass})">${hash}</span>${warning}`;
}

// ── Boards list ───────────────────────────────────────────────────────

function renderBoards(boards) {
  const ul = $("boards-list");
  ul.innerHTML = "";
  for (const b of boards) {
    const li = document.createElement("li");
    li.className = "entry";
    li.dataset.entryKind = "board";
    li.dataset.entrySlug = b.slug;
    li.innerHTML = `
      <div class="entry-name kind-board">${b.name}</div>
      <div class="entry-desc">${escape(firstSentence(b.description))}</div>
      <div class="entry-meta">${b.target_triple || ""}</div>
    `;
    li.addEventListener("click", () => showEntry(b, li));
    ul.appendChild(li);
  }
}

// ── Ensembles list (with nested plugins + sentants) ───────────────────

function renderEnsembles(ensembles) {
  const ul = $("ensembles-list");
  ul.innerHTML = "";
  for (const e of ensembles) {
    const li = document.createElement("li");
    const ensembleEl = document.createElement("div");
    ensembleEl.className = "entry";
    ensembleEl.dataset.entryKind = "ensemble";
    ensembleEl.dataset.entrySlug = e.slug;
    ensembleEl.innerHTML = `
      <div class="entry-name kind-ensemble">${e.name}</div>
      <div class="entry-desc">${escape(firstSentence(e.description))}</div>
      <div class="entry-meta">${renderClass(e.class)}</div>
    `;
    ensembleEl.addEventListener("click", () => showEntry(e, ensembleEl));
    li.appendChild(ensembleEl);

    if (e.plugins.length || e.sentants.length) {
      const nested = document.createElement("div");
      nested.className = "nested";

      if (e.plugins.length) {
        const head = document.createElement("div");
        head.className = "nested-section";
        head.textContent = `plugins (${e.plugins.length})`;
        nested.appendChild(head);
        for (const p of e.plugins) {
          const pe = document.createElement("div");
          pe.className = "entry";
          pe.dataset.entryKind = "plugin";
          pe.dataset.entrySlug = `${e.slug}/${p.category}/${p.slug}`;
          const modeBadges = [];
          if (p.modes.aot) modeBadges.push(`<span class="tag ok">aot</span>`);
          if (p.modes.nif) modeBadges.push(`<span class="tag ok">nif</span>`);
          if (p.modes.web) modeBadges.push(`<span class="tag ok">web</span>`);
          pe.innerHTML = `
            <div class="entry-name kind-plugin">${p.category}/${p.name}</div>
            <div class="entry-desc">${escape(firstSentence(p.description))}</div>
            <div class="entry-meta">${modeBadges.join("")}</div>
          `;
          pe.addEventListener("click", (evt) => {
            evt.stopPropagation();
            showEntry(p, pe);
          });
          nested.appendChild(pe);
        }
      }

      if (e.sentants.length) {
        const head = document.createElement("div");
        head.className = "nested-section";
        head.textContent = `sentants (${e.sentants.length})`;
        nested.appendChild(head);
        for (const s of e.sentants) {
          const se = document.createElement("div");
          se.className = "entry";
          se.dataset.entryKind = "sentant";
          se.dataset.entrySlug = `${e.slug}/${s.slug}`;
          se.innerHTML = `
            <div class="entry-name kind-sentant">${s.name}</div>
            <div class="entry-desc">${escape(firstSentence(s.description))}</div>
            <div class="entry-meta">${renderClass(s.class)}</div>
          `;
          se.addEventListener("click", (evt) => {
            evt.stopPropagation();
            showEntry(s, se);
          });
          nested.appendChild(se);
        }
      }
      li.appendChild(nested);
    }
    ul.appendChild(li);
  }
}

// ── Entry detail ──────────────────────────────────────────────────────

function showEntry(entry, listEl) {
  currentEntry = entry;
  document.querySelectorAll(".entry.selected").forEach((el) => el.classList.remove("selected"));
  if (listEl) listEl.classList.add("selected");

  $("workspace-placeholder").classList.add("hidden");
  $("entry-detail").classList.remove("hidden");

  $("detail-kind").className = `kind-chip ${entry.kind}`;
  $("detail-kind").textContent = entry.kind;
  $("detail-title").textContent = entry.name;
  $("detail-desc").textContent = entry.description || "";

  renderMeta(entry);
  renderFileTree(entry);

  // Auto-open AI-CONTEXT.md when one exists; falls back to the first file.
  const aiCtx = entry.files.find((f) => f.name === "AI-CONTEXT.md");
  const first = aiCtx || entry.files[0];
  if (first) loadFile(first);
}

function renderMeta(entry) {
  const meta = $("detail-meta");
  meta.innerHTML = "";
  const rows = [];
  switch (entry.kind) {
    case "board":
      rows.push(["arch", entry.arch]);
      rows.push(["chip", entry.chip]);
      rows.push(["carrier", entry.carrier]);
      rows.push(["target_triple", entry.target_triple]);
      rows.push(["compile_target.tag", entry.compile_target_tag]);
      rows.push(["flash", `${entry.flash_size_mb} MB`]);
      rows.push(["psram", entry.psram ? "yes" : "no"]);
      rows.push(["version", entry.version]);
      if (entry.compulsory_capabilities.length)
        rows.push([
          "compulsory",
          entry.compulsory_capabilities.map((c) => `<span class="tag ok">${c}</span>`).join(""),
        ]);
      break;
    case "ensemble":
      rows.push(["class", renderClass(entry.class)]);
      rows.push(["version", entry.version]);
      rows.push(["compile_target", entry.compile_target]);
      rows.push(["plugins", String(entry.plugins.length)]);
      rows.push(["sentants", String(entry.sentants.length)]);
      break;
    case "plugin":
      rows.push(["category", entry.category]);
      rows.push(["version", entry.version]);
      rows.push([
        "modes",
        ["aot", "nif", "web"]
          .map((m) => `<span class="tag ${entry.modes[m] ? "ok" : "off"}">${m}${entry.modes[m] ? "" : "✗"}</span>`)
          .join(""),
      ]);
      if (entry.provides.length)
        rows.push(["provides", entry.provides.map((c) => `<span class="tag">${c}</span>`).join("")]);
      if (entry.requires.length)
        rows.push(["requires", entry.requires.map((c) => `<span class="tag">${c}</span>`).join("")]);
      if (entry.commands.length)
        rows.push(["commands", entry.commands.map((c) => `<span class="tag">${c}</span>`).join("")]);
      break;
    case "sentant":
      rows.push(["class", renderClass(entry.class)]);
      rows.push(["storage", entry.storage]);
      break;
  }
  for (const [label, value] of rows) {
    if (!value) continue;
    const row = document.createElement("div");
    row.className = "meta-row";
    row.innerHTML = `<span class="meta-label">${label}</span><span class="meta-value ${
      typeof value === "string" && value.includes("<span") ? "list" : ""
    }">${value}</span>`;
    meta.appendChild(row);
  }
}

function renderFileTree(entry) {
  const ul = $("file-tree-list");
  ul.innerHTML = "";
  // Group / order: structured artefact first, narratives next, AI-CONTEXT, then everything else.
  const order = (f) => {
    if (/(board|plugin|sentant|ensemble)\.toml$/.test(f.name)) return 0;
    if (/(board|plugin|sentant|ensemble)\.yaml$/.test(f.name)) return 0;
    if (/(BOARD|PLUGIN|SENTANT|ENSEMBLE)\.md$/.test(f.name)) return 1;
    if (f.name === "README.md") return 2;
    if (f.name === "AI-CONTEXT.md") return 3;
    if (f.name === "Cargo.toml") return 4;
    if (f.path.includes("/src/")) return 5;
    if (f.path.includes("/templates/")) return 6;
    if (f.path.includes("/datasheets/")) return 7;
    return 9;
  };
  const sorted = [...entry.files].sort((a, b) => order(a) - order(b) || a.path.localeCompare(b.path));
  const entryPathPrefix = entry.path + "/";
  for (const f of sorted) {
    const li = document.createElement("li");
    li.dataset.path = f.path;
    const rest = f.path.startsWith(entryPathPrefix) ? f.path.slice(entryPathPrefix.length) : f.path;
    const restDir = rest.slice(0, rest.length - f.name.length);
    li.innerHTML = `
      <span class="kind-tag">.${f.kind}</span>${escape(f.name)}
      ${restDir ? `<span class="file-path-rest">${escape(restDir)}</span>` : ""}
    `;
    li.addEventListener("click", () => loadFile(f));
    ul.appendChild(li);
  }
}

// ── File viewer ───────────────────────────────────────────────────────

async function loadFile(f) {
  currentFile = f;
  document.querySelectorAll(".file-tree li.selected").forEach((el) => el.classList.remove("selected"));
  const li = document.querySelector(`.file-tree li[data-path="${cssEscape(f.path)}"]`);
  if (li) li.classList.add("selected");

  $("viewer-path").textContent = f.path;
  const body = $("viewer-body");
  body.textContent = "Loading…";

  try {
    const res = await fetch("../" + f.path, { cache: "no-store" });
    if (!res.ok) throw new Error(`fetch ${res.status}`);
    const text = await res.text();
    // Truncate truly huge files (defensive — shouldn't happen in practice).
    body.textContent = text.length > 500_000 ? text.slice(0, 500_000) + "\n\n…[truncated]" : text;
  } catch (err) {
    body.textContent = `Failed to load ${f.path}\n${err.message}`;
  }
}

// ── Misc ──────────────────────────────────────────────────────────────

function attachGlobalHandlers() {
  $("detail-close").addEventListener("click", () => {
    currentEntry = null;
    document.querySelectorAll(".entry.selected").forEach((el) => el.classList.remove("selected"));
    $("entry-detail").classList.add("hidden");
    $("workspace-placeholder").classList.remove("hidden");
  });
}

function escape(s) {
  return String(s ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function firstSentence(s) {
  if (!s) return "";
  const trimmed = s.trim();
  const cut = trimmed.search(/\.\s|\.$/);
  return cut > 0 ? trimmed.slice(0, cut + 1) : trimmed;
}

function cssEscape(s) {
  if (window.CSS && CSS.escape) return CSS.escape(s);
  return s.replace(/[^a-zA-Z0-9_\-]/g, "\\$&");
}

document.querySelector("#workspace-placeholder")?.classList.remove("hidden");
document.querySelector("#entry-detail")?.classList.add("hidden");

// ── /r2 WebSocket client + Build pane ─────────────────────────────────
//
// Browser ↔ orchestrator wire format matches `bridge::WireEnvelope`:
//   {"kind":"event","name":"r2.compiler.build.start","payload":{...}}
//   {"kind":"hello","from":"...","version":"...","note":"..."}
//   {"kind":"ack","echo":"..."}
//
// We're tolerant of WS failure — if the orchestrator isn't running,
// the catalogue browser still works; the Build pane just stays
// disabled with a "disconnected" indicator.

let r2Ws = null;
let r2ReconnectTimer = null;
let r2ReconnectDelayMs = 1000;
const R2_RECONNECT_MAX_MS = 15_000;

function r2Url() {
  const scheme = location.protocol === "https:" ? "wss" : "ws";
  return `${scheme}://${location.host}/r2`;
}

function setR2Status(state, label) {
  const dot = $("r2-dot");
  const lbl = $("r2-label");
  if (!dot || !lbl) return;
  dot.classList.remove("ok", "warn", "err");
  dot.classList.add(state);
  lbl.textContent = label;
  const compileBtn = $("build-compile");
  if (compileBtn) {
    const ready = state === "ok" && $("build-board").value && $("build-ensemble").value;
    compileBtn.disabled = !ready;
  }
}

function connectR2() {
  setR2Status("warn", "connecting…");
  try {
    r2Ws = new WebSocket(r2Url());
  } catch (err) {
    consoleLine("err", `WS construct failed: ${err.message}`);
    scheduleReconnect();
    return;
  }
  r2Ws.addEventListener("open", () => {
    r2ReconnectDelayMs = 1000;
    setR2Status("ok", "connected");
    consoleLine("sys", "/r2 connected");
  });
  r2Ws.addEventListener("message", (ev) => {
    let env;
    try {
      env = JSON.parse(ev.data);
    } catch (err) {
      consoleLine("err", `unparseable frame: ${err.message}`);
      return;
    }
    handleEnvelope(env);
  });
  r2Ws.addEventListener("close", () => {
    setR2Status("err", "disconnected");
    consoleLine("sys", "/r2 closed — reconnecting…");
    scheduleReconnect();
  });
  r2Ws.addEventListener("error", () => {
    setR2Status("err", "error");
  });
}

function scheduleReconnect() {
  if (r2ReconnectTimer) return;
  r2ReconnectTimer = setTimeout(() => {
    r2ReconnectTimer = null;
    r2ReconnectDelayMs = Math.min(r2ReconnectDelayMs * 2, R2_RECONNECT_MAX_MS);
    connectR2();
  }, r2ReconnectDelayMs);
}

function handleEnvelope(env) {
  switch (env.kind) {
    case "hello":
      consoleLine("sys", `hello from ${env.from} v${env.version}${env.note ? ` — ${env.note}` : ""}`);
      break;
    case "ack":
      consoleLine("sys", `ack: ${env.echo}`);
      break;
    case "event":
      onEvent(env.name, env.payload);
      break;
    default:
      consoleLine("warn", `unknown envelope kind: ${env.kind}`);
  }
}

function onEvent(name, payload) {
  let cls = "evt";
  if (name === "r2.compiler.build.progress") cls = "evt-progress";
  else if (name === "r2.compiler.build.done") cls = "evt-done";
  else if (name === "r2.compiler.build.error") cls = "evt-error";
  consoleLine(cls, `${name}  ${formatPayload(payload)}`);
  if (name === "r2.compiler.build.done") {
    setBuildStatus("done");
  } else if (name === "r2.compiler.build.error") {
    setBuildStatus("error");
  } else if (name === "r2.compiler.build.progress") {
    setBuildStatus("building");
  }
}

function formatPayload(p) {
  if (p === null || p === undefined) return "";
  if (typeof p === "string") return p;
  try { return JSON.stringify(p); } catch { return String(p); }
}

function sendEvent(name, payload) {
  if (!r2Ws || r2Ws.readyState !== WebSocket.OPEN) {
    consoleLine("err", `cannot send ${name} — /r2 not open`);
    return false;
  }
  const env = { kind: "event", name, payload };
  r2Ws.send(JSON.stringify(env));
  consoleLine("out", `→ ${name}  ${formatPayload(payload)}`);
  return true;
}

// ── Build pane ────────────────────────────────────────────────────────

function populateBuildSelects(m) {
  const bsel = $("build-board");
  const esel = $("build-ensemble");
  if (!bsel || !esel) return;
  bsel.innerHTML = '<option value="">— choose —</option>';
  for (const b of m.boards) {
    const opt = document.createElement("option");
    opt.value = b.slug;
    opt.textContent = `${b.name}  (${b.target_triple || "?"})`;
    bsel.appendChild(opt);
  }
  esel.innerHTML = '<option value="">— choose —</option>';
  for (const e of m.ensembles) {
    const opt = document.createElement("option");
    opt.value = e.slug;
    opt.textContent = `${e.name}`;
    esel.appendChild(opt);
  }
  const refresh = () => {
    const ready = r2Ws && r2Ws.readyState === WebSocket.OPEN && bsel.value && esel.value;
    $("build-compile").disabled = !ready;
  };
  bsel.addEventListener("change", refresh);
  esel.addEventListener("change", refresh);

  $("build-compile").addEventListener("click", () => {
    const board = bsel.value;
    const ensemble = esel.value;
    if (!board || !ensemble) return;
    setBuildStatus("starting");
    const ok = sendEvent("r2.compiler.build.start", {
      target: board,
      score: ensemble,
      // Phase 1.8 adds the Tera-rendered prompt brief in the payload;
      // for now the claude-code plugin uses its default command per
      // SPEC-R2-COMPILER §5.
    });
    if (!ok) setBuildStatus("disconnected");
  });

  $("build-clear").addEventListener("click", () => {
    $("build-console").textContent = "";
  });
}

function setBuildStatus(state) {
  const el = $("build-status");
  if (!el) return;
  el.dataset.state = state;
  el.textContent = state;
}

function consoleLine(cls, text) {
  const pane = $("build-console");
  if (!pane) return;
  const line = document.createElement("div");
  line.className = `console-line ${cls}`;
  const ts = new Date().toISOString().slice(11, 19);
  line.textContent = `${ts}  ${text}`;
  pane.appendChild(line);
  pane.scrollTop = pane.scrollHeight;
  // Keep the log bounded — drop oldest lines past 500.
  while (pane.childNodes.length > 500) pane.removeChild(pane.firstChild);
}
