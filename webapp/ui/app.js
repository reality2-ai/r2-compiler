// r2-compiler — minimal catalogue preview app.
//
// No framework, no build step. Fetches webapp/dist/manifest.json (built by
// tools/build-catalogue-index.py from the catalogue/ tree) and renders the
// boards + ensembles + nested plugins + sentants. Click an entry to see
// its structured-artefact meta + a file viewer for AI-CONTEXT.md / TOML /
// YAML / source.
//
// Phase 2-preview: this is NOT the eventual WASM R2 hive (Phase 2-full).
// It's a static catalogue browser meant to make the layout visible early.

"use strict";

const $ = (id) => document.getElementById(id);

let manifest = null;
let currentEntry = null;
let currentFile = null;

// ── Boot ──────────────────────────────────────────────────────────────

(async function boot() {
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
})();

// ── Header ────────────────────────────────────────────────────────────

function renderHeader(stats) {
  $("stats").textContent = `${stats.boards} boards · ${stats.ensembles} ensembles · ${stats.plugins} plugins · ${stats.sentants} sentants`;
  $("boards-count").textContent = String(stats.boards);
  $("ensembles-count").textContent = String(stats.ensembles);
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
      <div class="entry-meta">${e.class || ""}</div>
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
            <div class="entry-meta">${s.class || ""}</div>
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
      rows.push(["class", entry.class]);
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
      rows.push(["class", entry.class]);
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
