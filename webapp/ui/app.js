// r2-composer — minimal catalogue preview app.
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
} from "../dist/wasm/r2_composer_webapp.js";

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
    console.info(`r2-composer-webapp WASM loaded — v${wasmVersion()}`);
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

  // Phase 1.7c: canvas + drag-and-drop. The catalogue panels become
  // the drag SOURCES; the canvas in the centre is the drop TARGET;
  // Compile dispatches the composition over /r2.
  attachCanvas();

  // C-1: apiary canvas frame + mode switcher between Apiary and
  // Quick build. Mocked state for v0.1; C-2 wires to apiary.toml.
  attachApiaryCanvas();
  attachCanvasModeSwitcher();

  // Phase 1.7b: connect to the orchestrator's /r2 WebSocket and wire
  // the bottom panel (tabbed: Chat | Build). The catalogue browser
  // above stays fully usable even if the WS fails.
  attachBottomPanel();
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
    li.draggable = true;
    li.dataset.entryKind = "board";
    li.dataset.entrySlug = b.slug;
    li.innerHTML = `
      <div class="entry-name kind-board">${b.name}</div>
      <div class="entry-desc">${escape(firstSentence(b.description))}</div>
      <div class="entry-meta">${b.target_triple || ""}</div>
    `;
    li.addEventListener("click", () => showEntry(b, li));
    li.addEventListener("dragstart", (ev) => onEntryDragStart(ev, li, b, "board"));
    li.addEventListener("dragend", () => li.classList.remove("dragging"));
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
    ensembleEl.draggable = true;
    ensembleEl.dataset.entryKind = "ensemble";
    ensembleEl.dataset.entrySlug = e.slug;
    ensembleEl.innerHTML = `
      <div class="entry-name kind-ensemble">${e.name}</div>
      <div class="entry-desc">${escape(firstSentence(e.description))}</div>
      <div class="entry-meta">${renderClass(e.class)}</div>
    `;
    ensembleEl.addEventListener("click", () => showEntry(e, ensembleEl));
    ensembleEl.addEventListener("dragstart", (ev) => onEntryDragStart(ev, ensembleEl, e, "ensemble"));
    ensembleEl.addEventListener("dragend", () => ensembleEl.classList.remove("dragging"));
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

  $("canvas").classList.add("hidden");
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
  body.classList.remove("rendered-md", "rendered-code", "rendered-csv");
  body.textContent = "Loading…";

  try {
    const res = await fetch("../" + f.path, { cache: "no-store" });
    if (!res.ok) throw new Error(`fetch ${res.status}`);
    let text = await res.text();
    if (text.length > 500_000) text = text.slice(0, 500_000) + "\n\n…[truncated]";

    // Effective extension: strip suffixes like `.example` / `.sample` /
    // `.template` so `wifi_config.toml.example` gets the TOML highlighter
    // (the suffix is just an indication that this is a committable copy,
    // not a different file shape).
    const parts = f.name.toLowerCase().split(".");
    const SUFFIX_PASSTHROUGH = new Set(["example", "exam", "sample", "template", "tmpl", "tpl", "in"]);
    let ext = parts.pop() || "";
    while (SUFFIX_PASSTHROUGH.has(ext) && parts.length > 0) {
      ext = parts.pop();
    }
    if (ext === "md") {
      body.classList.add("rendered-md");
      body.innerHTML = renderMarkdown(text);
    } else if (ext === "csv") {
      body.classList.add("rendered-csv");
      body.innerHTML = renderCsv(text);
    } else if (HIGHLIGHTERS[ext]) {
      body.classList.add("rendered-code");
      body.innerHTML = renderCode(text, ext);
    } else {
      body.textContent = text;
    }
  } catch (err) {
    body.textContent = `Failed to load ${f.path}\n${err.message}`;
  }
}

// ── Minimal markdown renderer ─────────────────────────────────────────
//
// Hand-rolled to keep the webapp dep-free. Covers the markdown shapes
// our catalogue + spec docs actually use: ATX headings, fenced code,
// tables, blockquotes, lists, paragraphs, inline code / bold / italic /
// links. Not a CommonMark-compliant parser — if a doc needs richer
// markup, we'll switch to a vendored `marked` build.

function renderMarkdown(src) {
  const lines = src.split(/\r?\n/);
  let out = "";
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];

    // Fenced code block
    const fence = line.match(/^```\s*([\w-]*)\s*$/);
    if (fence) {
      const lang = fence[1] || "";
      const buf = [];
      i++;
      while (i < lines.length && !/^```\s*$/.test(lines[i])) {
        buf.push(lines[i]);
        i++;
      }
      i++; // skip closing fence (if present)
      out += `<pre class="md-code"><code class="lang-${escapeAttr(lang)}">${renderCode(buf.join("\n"), lang)}</code></pre>`;
      continue;
    }

    // ATX heading
    const h = line.match(/^(#{1,6})\s+(.+?)\s*#*\s*$/);
    if (h) {
      const lvl = h[1].length;
      out += `<h${lvl} class="md-h${lvl}">${renderInline(h[2])}</h${lvl}>`;
      i++;
      continue;
    }

    // Horizontal rule
    if (/^\s*(-{3,}|_{3,}|\*{3,})\s*$/.test(line)) {
      out += "<hr class='md-hr'>";
      i++;
      continue;
    }

    // Table — header row, then alignment row, then body rows
    if (/^\s*\|.+\|\s*$/.test(line)
        && i + 1 < lines.length
        && /^\s*\|?\s*:?-{2,}:?(\s*\|\s*:?-{2,}:?)+\s*\|?\s*$/.test(lines[i + 1])) {
      const headers = splitTableRow(line);
      i += 2;
      const rows = [];
      while (i < lines.length && /^\s*\|.+\|\s*$/.test(lines[i])) {
        rows.push(splitTableRow(lines[i]));
        i++;
      }
      let t = "<table class='md-table'><thead><tr>";
      for (const c of headers) t += `<th>${renderInline(c)}</th>`;
      t += "</tr></thead><tbody>";
      for (const r of rows) {
        t += "<tr>";
        for (const c of r) t += `<td>${renderInline(c)}</td>`;
        t += "</tr>";
      }
      t += "</tbody></table>";
      out += t;
      continue;
    }

    // Blockquote
    if (/^>\s?/.test(line)) {
      const buf = [];
      while (i < lines.length && /^>\s?/.test(lines[i])) {
        buf.push(lines[i].replace(/^>\s?/, ""));
        i++;
      }
      out += `<blockquote class="md-quote">${renderInline(buf.join(" "))}</blockquote>`;
      continue;
    }

    // Unordered list
    if (/^[-*+]\s+/.test(line)) {
      const items = [];
      while (i < lines.length && /^[-*+]\s+/.test(lines[i])) {
        items.push(lines[i].replace(/^[-*+]\s+/, ""));
        i++;
      }
      out += "<ul class='md-list'>" + items.map((it) => `<li>${renderInline(it)}</li>`).join("") + "</ul>";
      continue;
    }

    // Ordered list
    if (/^\d+\.\s+/.test(line)) {
      const items = [];
      while (i < lines.length && /^\d+\.\s+/.test(lines[i])) {
        items.push(lines[i].replace(/^\d+\.\s+/, ""));
        i++;
      }
      out += "<ol class='md-list'>" + items.map((it) => `<li>${renderInline(it)}</li>`).join("") + "</ol>";
      continue;
    }

    // Blank line
    if (/^\s*$/.test(line)) {
      i++;
      continue;
    }

    // Paragraph — accumulate until blank line or block-starter
    const buf = [line];
    i++;
    while (i < lines.length
           && !/^\s*$/.test(lines[i])
           && !/^(#{1,6}\s|>|```|[-*+]\s+|\d+\.\s+|\|.+\|)/.test(lines[i])) {
      buf.push(lines[i]);
      i++;
    }
    out += `<p>${renderInline(buf.join(" "))}</p>`;
  }
  return out;
}

function renderInline(text) {
  // Stash inline code spans first so their content is preserved verbatim
  // and not double-processed by the bold/italic/link substitutions.
  const codes = [];
  text = text.replace(/`([^`]+)`/g, (_, c) => {
    codes.push(c);
    return `C${codes.length - 1}`;
  });
  // Escape everything else
  text = escapeHtml(text);
  // Links [text](url) — must run before bold/italic so the brackets/parens
  // aren't munged.
  text = text.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (_, lbl, url) => {
    return `<a href="${escapeAttr(url)}" target="_blank" rel="noopener noreferrer">${lbl}</a>`;
  });
  // Bold **...**
  text = text.replace(/\*\*([^*\n]+)\*\*/g, "<strong>$1</strong>");
  // Italic *...* — guarded so `**bold**` isn't matched here too
  text = text.replace(/(^|[^*])\*([^*\n]+)\*(?!\*)/g, "$1<em>$2</em>");
  // Restore code spans
  text = text.replace(/C(\d+)/g, (_, n) => {
    return `<code class="md-code-inline">${escapeHtml(codes[parseInt(n, 10)])}</code>`;
  });
  return text;
}

function splitTableRow(line) {
  return line.replace(/^\s*\|/, "").replace(/\|\s*$/, "").split("|").map((c) => c.trim());
}

function escapeHtml(s) {
  return String(s ?? "").replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
}
function escapeAttr(s) {
  return escapeHtml(s).replaceAll('"', "&quot;");
}

// ── Code syntax highlighting ──────────────────────────────────────────
//
// Minimal per-language tokenisers. Each highlighter returns escaped
// HTML with <span class="tok-...">...</span> markers; absent languages
// fall back to plain-text escape. Multiline regexes use the `m` flag
// so `^` matches at every line start.

const HIGHLIGHTERS = {
  toml:     renderToml,
  // ESP-IDF sdkconfig.defaults — same shape as TOML (KEY = value, # comments,
  // quoted strings, numbers); no [sections], which TOML's regex degrades on
  // gracefully (no false-positives).
  defaults: renderToml,
  cfg:      renderToml,
  ini:      renderToml,
  yaml:     renderYaml,
  yml:      renderYaml,
  json:     renderJson,
  rust:     renderRust,
  rs:       renderRust,
};

function renderCode(src, lang) {
  const fn = HIGHLIGHTERS[lang];
  return fn ? fn(src) : escapeHtml(src);
}

function tokensToHtml(tokens) {
  return tokens.map((t) => {
    const text = escapeHtml(t.text);
    return t.type === "plain" ? text : `<span class="tok-${t.type}">${text}</span>`;
  }).join("");
}

function tokenize(src, re, classify) {
  const tokens = [];
  let lastIdx = 0;
  let m;
  while ((m = re.exec(src)) !== null) {
    if (m.index === re.lastIndex) { re.lastIndex++; continue; } // zero-length safeguard
    if (m.index > lastIdx) tokens.push({ type: "plain", text: src.slice(lastIdx, m.index) });
    const emitted = classify(m);
    if (emitted) tokens.push(...emitted);
    lastIdx = re.lastIndex;
  }
  if (lastIdx < src.length) tokens.push({ type: "plain", text: src.slice(lastIdx) });
  return tokens;
}

function renderToml(src) {
  const re = /(#[^\n]*)|("(?:[^"\\\n]|\\.)*"|'[^'\n]*')|^(\s*)(\[\[?[^\]\n]+\]\]?)|\b(true|false)\b|\b(-?\d[\d_.eE+\-]*)\b|^(\s*)([A-Za-z0-9_.\-]+)(?=\s*=)/gm;
  return tokensToHtml(tokenize(src, re, (m) => {
    if (m[1]) return [{ type: "comment", text: m[1] }];
    if (m[2]) return [{ type: "string",  text: m[2] }];
    if (m[4]) return [{ type: "plain",   text: m[3] }, { type: "section", text: m[4] }];
    if (m[5]) return [{ type: "bool",    text: m[5] }];
    if (m[6]) return [{ type: "num",     text: m[6] }];
    if (m[8]) return [{ type: "plain",   text: m[7] }, { type: "key",     text: m[8] }];
    return [];
  }));
}

function renderYaml(src) {
  const re = /(#[^\n]*)|("(?:[^"\\\n]|\\.)*"|'[^'\n]*')|\b(true|false|null|yes|no|True|False|Null|None)\b|\b(-?\d[\d.eE+\-]*)\b|^(\s*-?\s*)([A-Za-z0-9_./\-]+)(?=:)/gm;
  return tokensToHtml(tokenize(src, re, (m) => {
    if (m[1]) return [{ type: "comment", text: m[1] }];
    if (m[2]) return [{ type: "string",  text: m[2] }];
    if (m[3]) return [{ type: "bool",    text: m[3] }];
    if (m[4]) return [{ type: "num",     text: m[4] }];
    if (m[6]) return [{ type: "plain",   text: m[5] }, { type: "key", text: m[6] }];
    return [];
  }));
}

function renderJson(src) {
  const re = /("(?:[^"\\]|\\.)*")(\s*:)?|\b(true|false|null)\b|(-?\b\d[\d.eE+\-]*\b)/g;
  return tokensToHtml(tokenize(src, re, (m) => {
    if (m[1]) {
      if (m[2]) return [{ type: "key", text: m[1] }, { type: "plain", text: m[2] }];
      return [{ type: "string", text: m[1] }];
    }
    if (m[3]) return [{ type: "bool", text: m[3] }];
    if (m[4]) return [{ type: "num",  text: m[4] }];
    return [];
  }));
}

// Rust: keywords, types, lifetimes, strings, char literals, attrs, numbers, comments.
// Pragmatic — not a full grammar, but covers the visual shape of our plugin/sentant source.
const RUST_KEYWORDS = new Set([
  "as","async","await","break","const","continue","crate","dyn","else","enum","extern","false",
  "fn","for","if","impl","in","let","loop","match","mod","move","mut","pub","ref","return",
  "self","Self","static","struct","super","trait","true","type","union","unsafe","use","where",
  "while","yield","box","macro","try",
]);
const RUST_TYPES = new Set([
  "bool","char","i8","i16","i32","i64","i128","isize","u8","u16","u32","u64","u128","usize",
  "f32","f64","str","String","Vec","Option","Result","Box","Rc","Arc","Cell","RefCell","Mutex",
  "HashMap","BTreeMap","HashSet","BTreeSet",
]);

// ── CSV ───────────────────────────────────────────────────────────────
//
// CSV gets a real HTML table (not just colour tokens) because that's
// how a developer expects to read tabular data. The parser handles
// quoted fields and embedded "" escapes; it does NOT handle multi-line
// quoted fields (rare in the kinds of CSVs we ship — pinouts, manifests,
// roster files).

function parseCsv(src) {
  const rows = [];
  let row = [];
  let field = "";
  let inQuote = false;
  for (let i = 0; i < src.length; i++) {
    const c = src[i];
    if (inQuote) {
      if (c === '"') {
        if (src[i + 1] === '"') { field += '"'; i++; continue; }
        inQuote = false;
        continue;
      }
      field += c;
      continue;
    }
    if (c === '"' && field === "") { inQuote = true; continue; }
    if (c === ",") { row.push(field); field = ""; continue; }
    if (c === "\r") continue;
    if (c === "\n") { row.push(field); rows.push(row); row = []; field = ""; continue; }
    field += c;
  }
  if (field.length > 0 || row.length > 0) { row.push(field); rows.push(row); }
  return rows;
}

function renderCsv(src) {
  const rows = parseCsv(src);
  if (rows.length === 0) return "<p class='csv-empty'>(empty)</p>";
  const [head, ...body] = rows;
  let html = `<table class="csv-table"><thead><tr>`;
  for (const c of head) html += `<th>${escapeHtml(c)}</th>`;
  html += "</tr></thead><tbody>";
  for (const r of body) {
    html += "<tr>";
    for (let i = 0; i < head.length; i++) {
      html += `<td>${escapeHtml(r[i] ?? "")}</td>`;
    }
    html += "</tr>";
  }
  html += "</tbody></table>";
  html += `<p class="csv-meta">${body.length} row${body.length === 1 ? "" : "s"} · ${head.length} column${head.length === 1 ? "" : "s"}</p>`;
  return html;
}

function renderRust(src) {
  // Order matters: comments + strings first so their contents aren't keyword-matched.
  //
  // Strings handle every Rust literal form:
  //   "..."  b"..."         — escape sequences allowed; `\\[\s\S]` (not `\\.`)
  //                           lets a `\<NL>` line-continuation match (JS `.` rejects \n).
  //   r"..."  r#"..."#  r##"..."##  r###"..."###    — raw strings, 0–3 hashes
  //   br"..." br#"..."# etc.                         — byte raw strings
  //
  // Raw strings use a lookahead `"(?!#{n})` so an inner `"` only closes the
  // literal when followed by EXACTLY the matching hash count. Without this,
  // the previous `r#?"[^"]*"#?` greedy-matched the first inner `"` and
  // released the rest of the file into "still inside a string" land.
  //
  // Lifetime regex deliberately omits \b on either side: `'` isn't a
  // word character, so `\b` only matches when the preceding char IS a
  // word char (e.g. `_'static`) — which would miss the common ` 'static`
  // case. Alternation order (char literal before lifetime) makes the
  // distinction unambiguous.
  const re = new RegExp([
    String.raw`(\/\/[^\n]*|\/\*[\s\S]*?\*\/)`,
    String.raw`(br###"(?:[^"]|"(?!###))*"###|br##"(?:[^"]|"(?!##))*"##|br#"(?:[^"]|"(?!#))*"#|br"[^"]*"|r###"(?:[^"]|"(?!###))*"###|r##"(?:[^"]|"(?!##))*"##|r#"(?:[^"]|"(?!#))*"#|r"[^"]*"|b"(?:[^"\\]|\\[\s\S])*"|"(?:[^"\\]|\\[\s\S])*")`,
    String.raw`('(?:[^'\\]|\\.)')`,
    String.raw`(#!?\[[^\]\n]*\])`,
    String.raw`('[a-zA-Z_][a-zA-Z0-9_]*)`,
    String.raw`\b(0x[0-9a-fA-F_]+|0o[0-7_]+|0b[01_]+|\d[\d_.eE+\-]*(?:_?[uif]\d*)?)\b`,
    String.raw`\b([a-zA-Z_][a-zA-Z0-9_]*)\b`,
  ].join("|"), "g");
  return tokensToHtml(tokenize(src, re, (m) => {
    if (m[1]) return [{ type: "comment", text: m[1] }];
    if (m[2]) return [{ type: "string",  text: m[2] }];
    if (m[3]) return [{ type: "string",  text: m[3] }];
    if (m[4]) return [{ type: "attr",    text: m[4] }];
    if (m[5]) return [{ type: "lifetime", text: m[5] }];
    if (m[6]) return [{ type: "num",     text: m[6] }];
    if (m[7]) {
      const w = m[7];
      if (RUST_KEYWORDS.has(w)) return [{ type: "kw",   text: w }];
      if (RUST_TYPES.has(w))    return [{ type: "type", text: w }];
      return [{ type: "plain", text: w }];
    }
    return [];
  }));
}

// ── Misc ──────────────────────────────────────────────────────────────

function attachGlobalHandlers() {
  $("detail-close").addEventListener("click", () => {
    currentEntry = null;
    document.querySelectorAll(".entry.selected").forEach((el) => el.classList.remove("selected"));
    $("entry-detail").classList.add("hidden");
    $("canvas").classList.remove("hidden");
  });

  attachFileResizer();
}

// Drag handle between the file-tree and file-viewer in the entry-detail
// pane. Resize is clamped [120, 700] px so the tree never disappears or
// eats the viewer.
function attachFileResizer() {
  const resizer = $("file-resizer");
  const tree = document.querySelector(".file-tree");
  if (!resizer || !tree) return;

  let dragging = false;
  let startX = 0;
  let startWidth = 0;

  resizer.addEventListener("mousedown", (ev) => {
    dragging = true;
    startX = ev.clientX;
    startWidth = tree.offsetWidth;
    resizer.classList.add("dragging");
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    ev.preventDefault();
  });

  window.addEventListener("mousemove", (ev) => {
    if (!dragging) return;
    const w = Math.max(120, Math.min(700, startWidth + (ev.clientX - startX)));
    tree.style.width = `${w}px`;
  });

  window.addEventListener("mouseup", () => {
    if (!dragging) return;
    dragging = false;
    resizer.classList.remove("dragging");
    document.body.style.cursor = "";
    document.body.style.userSelect = "";
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

document.querySelector("#entry-detail")?.classList.add("hidden");

// ── /r2 WebSocket client + Build pane ─────────────────────────────────
//
// Browser ↔ orchestrator wire format matches `bridge::WireEnvelope`:
//   {"kind":"event","name":"r2.composer.build.start","payload":{...}}
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
  refreshCompileButton();
  refreshChatSendButton();
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
  // Apiary state hydration → apiary canvas
  if (name === "r2.composer.apiary.active") {
    onApiaryActive(payload);
    return;
  }
  // Author flow → chat pane
  if (name.startsWith("r2.composer.author.")) {
    onAuthorEvent(name, payload);
    return;
  }
  // Build flow → build console
  let cls = "evt";
  if (name === "r2.composer.build.progress") cls = "evt-progress";
  else if (name === "r2.composer.build.done") cls = "evt-done";
  else if (name === "r2.composer.build.error") cls = "evt-error";
  consoleLine(cls, `${name}  ${formatPayload(payload)}`);
  if (name === "r2.composer.build.done") {
    setBuildStatus("done");
  } else if (name === "r2.composer.build.error") {
    setBuildStatus("error");
  } else if (name === "r2.composer.build.progress") {
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

// ── Canvas (Phase 1.7c) ───────────────────────────────────────────────
//
// The canvas is r2-composer's structured-prompt builder: drag a board
// + an ensemble onto it; Compile dispatches the composition over /r2.
// Phase 1.7d adds the AI-chat pane that reads + mutates `canvas`.

// Distinct MIME types so drop zones can validate during dragover
// (`dataTransfer.getData` is unreadable during dragover for security;
// only `.types` is observable, so we encode the kind in the MIME).
const MIME_BOARD = "application/x-r2-board";
const MIME_ENSEMBLE = "application/x-r2-ensemble";

const canvasState = { board: null, ensemble: null };

function onEntryDragStart(ev, el, entry, kind) {
  const mime = kind === "board" ? MIME_BOARD : MIME_ENSEMBLE;
  ev.dataTransfer.setData(mime, JSON.stringify({ slug: entry.slug }));
  ev.dataTransfer.effectAllowed = "copy";
  el.classList.add("dragging");
}

function attachCanvas() {
  for (const kind of ["board", "ensemble"]) {
    const slot = $(`canvas-slot-${kind}`);
    const mime = kind === "board" ? MIME_BOARD : MIME_ENSEMBLE;

    slot.addEventListener("dragover", (ev) => {
      if (!ev.dataTransfer.types.includes(mime)) return;
      ev.preventDefault();
      ev.dataTransfer.dropEffect = "copy";
      slot.classList.add("over");
    });
    slot.addEventListener("dragleave", () => slot.classList.remove("over"));
    slot.addEventListener("drop", (ev) => {
      slot.classList.remove("over");
      const raw = ev.dataTransfer.getData(mime);
      if (!raw) return;
      ev.preventDefault();
      try {
        const data = JSON.parse(raw);
        canvasState[kind] = data.slug;
        renderCanvas();
      } catch (err) {
        consoleLine("err", `drop parse failed: ${err.message}`);
      }
    });
  }

  $("canvas-clear").addEventListener("click", () => {
    canvasState.board = null;
    canvasState.ensemble = null;
    renderCanvas();
  });

  $("canvas-compile").addEventListener("click", () => {
    if (!canvasState.board || !canvasState.ensemble) return;
    setBuildStatus("starting");
    const ok = sendEvent("r2.composer.build.start", {
      target: canvasState.board,
      score: canvasState.ensemble,
      // Phase 1.8 wraps this in a Tera-rendered prompt brief per
      // SPEC-R2-COMPOSER §5.
    });
    if (!ok) setBuildStatus("disconnected");
  });

  renderCanvas();
}

function renderCanvas() {
  renderCanvasSlot("board", canvasState.board);
  renderCanvasSlot("ensemble", canvasState.ensemble);
  refreshCompileButton();
}

function renderCanvasSlot(kind, slug) {
  const slot = $(`canvas-slot-${kind}`);
  if (!slug) {
    slot.classList.remove("filled");
    slot.dataset.kind = kind;
    slot.innerHTML = `
      <div class="slot-empty">
        <span class="slot-kind ${kind}">${kind}</span>
        <span class="slot-hint">drag a ${kind} here</span>
      </div>`;
    return;
  }
  const entry = lookupEntry(kind, slug);
  if (!entry) {
    slot.innerHTML = `<div class="slot-empty"><span>not in manifest: ${escape(slug)}</span></div>`;
    return;
  }
  slot.classList.add("filled");

  const meta = kind === "board"
    ? `${escape(entry.target_triple || "?")} · ${escape(entry.chip || "?")} · ${entry.flash_size_mb || "?"} MB${entry.psram ? " · psram" : ""}`
    : `${entry.plugins.length} plugins · ${entry.sentants.length} sentants · ${escape(entry.compile_target || "?")}`;

  slot.innerHTML = `
    <div class="canvas-tile">
      <button class="tile-remove" title="Remove" aria-label="Remove">×</button>
      <span class="kind-chip ${kind}">${kind}</span>
      <h3 class="tile-title">${escape(entry.name)}</h3>
      <p class="tile-desc">${escape(firstSentence(entry.description))}</p>
      <div class="tile-meta">${meta}</div>
      ${entry.class ? `<div class="tile-meta">${renderClass(entry.class)}</div>` : ""}
    </div>`;
  slot.querySelector(".tile-remove").addEventListener("click", () => {
    canvasState[kind] = null;
    renderCanvas();
  });
}

function lookupEntry(kind, slug) {
  if (!manifest) return null;
  if (kind === "board") return manifest.boards.find((b) => b.slug === slug);
  return manifest.ensembles.find((e) => e.slug === slug);
}

function refreshCompileButton() {
  const btn = $("canvas-compile");
  const status = $("canvas-status");
  if (!btn || !status) return;
  const composed = !!(canvasState.board && canvasState.ensemble);
  const wsOpen = r2Ws && r2Ws.readyState === WebSocket.OPEN;
  btn.disabled = !(composed && wsOpen);
  let state, label;
  if (!composed) { state = "incomplete"; label = "incomplete"; }
  else if (!wsOpen) { state = "disconnected"; label = "no /r2 connection"; }
  else { state = "ready"; label = "ready"; }
  status.dataset.state = state;
  status.textContent = label;
}

// ── Bottom panel (tabbed: Chat | Build) ───────────────────────────────
//
// The Chat tab is the hybrid-UX dialog: user prompts go out as
// `r2.composer.author.prompt` events carrying the current canvas as
// context; assistant replies stream back as `r2.composer.author.reply`
// and finalise on `r2.composer.author.done`. The orchestrator-side
// Author plugin (Phase 1.7d, not yet wired) is what turns those
// prompts into real `claude -p` invocations with the canvas + history
// spliced into the brief.

const chatState = {
  history: [],   // [{role: 'user'|'assistant', content, canvas?}]
  pending: null, // ref into history — the assistant msg currently streaming
};

function attachBottomPanel() {
  // Tab switching
  for (const btn of document.querySelectorAll(".tab")) {
    btn.addEventListener("click", () => switchTab(btn.dataset.tab));
  }
  // Clear (context-dependent)
  $("bottom-clear").addEventListener("click", () => {
    const active = document.querySelector(".tab-pane.active")?.id;
    if (active === "pane-chat") {
      chatState.history = [];
      chatState.pending = null;
      renderChat();
    } else if (active === "pane-build") {
      $("build-console").textContent = "";
    }
  });

  // Chat input
  const input = $("chat-input");
  const send = $("chat-send");
  input.addEventListener("input", refreshChatSendButton);
  input.addEventListener("keydown", (ev) => {
    if (ev.key === "Enter" && !ev.shiftKey) {
      ev.preventDefault();
      sendChat();
    }
  });
  send.addEventListener("click", sendChat);

  refreshChatSendButton();
  renderChat();
}

function switchTab(tab) {
  document.querySelectorAll(".tab").forEach((t) => {
    t.classList.toggle("active", t.dataset.tab === tab);
  });
  document.querySelectorAll(".tab-pane").forEach((p) => {
    p.classList.toggle("active", p.id === `pane-${tab}`);
  });
  if (tab === "chat") {
    $("tab-badge-chat").textContent = "";
    setTimeout(() => $("chat-input")?.focus(), 0);
  }
}

function refreshChatSendButton() {
  const input = $("chat-input");
  const send = $("chat-send");
  if (!input || !send) return;
  const text = input.value.trim();
  const wsOpen = r2Ws && r2Ws.readyState === WebSocket.OPEN;
  send.disabled = !(text && wsOpen);
}

function sendChat() {
  const input = $("chat-input");
  const text = input.value.trim();
  if (!text) return;
  if (!r2Ws || r2Ws.readyState !== WebSocket.OPEN) return;

  const canvasCtx = {
    board: canvasState.board,
    ensemble: canvasState.ensemble,
  };

  // Append the user turn + a placeholder assistant turn so the UI
  // can show "claude is thinking…" while the orchestrator works.
  chatState.history.push({ role: "user", content: text, canvas: canvasCtx });
  const pending = { role: "assistant", content: "" };
  chatState.history.push(pending);
  chatState.pending = pending;

  // The history sent to the orchestrator EXCLUDES the empty assistant
  // placeholder — it's only there for UI rendering.
  const historyForServer = chatState.history.slice(0, -1).map((m) => ({
    role: m.role,
    content: m.content,
  }));
  sendEvent("r2.composer.author.prompt", {
    message: text,
    canvas: canvasCtx,
    history: historyForServer,
  });

  input.value = "";
  refreshChatSendButton();
  renderChat();
}

function onAuthorEvent(name, payload) {
  // Ensure a pending assistant message exists to absorb streamed content
  if (!chatState.pending) {
    chatState.pending = { role: "assistant", content: "" };
    chatState.history.push(chatState.pending);
  }
  if (name === "r2.composer.author.reply") {
    // The orchestrator's claude-code plugin extracts `payload.text` from
    // stream-json assistant + result lines; system / init / tool-use
    // lines come through with text=null and we skip them so the chat
    // pane only shows the actual conversation.
    const text = typeof payload === "string" ? payload : payload?.text;
    if (text) {
      chatState.pending.content += String(text);
      renderChat();
      bumpChatBadge();
    }
  } else if (name === "r2.composer.author.done") {
    chatState.pending = null;
    renderChat();
    bumpChatBadge();
  } else if (name === "r2.composer.author.error") {
    const msg = typeof payload === "string" ? payload : (payload?.message ?? JSON.stringify(payload));
    chatState.pending.content += `\n\n**[error]** ${msg}`;
    chatState.pending = null;
    renderChat();
    bumpChatBadge();
  } else if (name === "r2.composer.author.file_added") {
    const path = payload?.path ?? formatPayload(payload);
    chatState.pending.content += `\n_[file added: ${path}]_\n`;
    renderChat();
  }
}

function bumpChatBadge() {
  if (!document.querySelector("#tab-chat")?.classList.contains("active")) {
    $("tab-badge-chat").textContent = "•";
  }
}

function renderChat() {
  const root = $("chat-messages");
  if (!root) return;
  if (chatState.history.length === 0) {
    root.innerHTML = `<div class="chat-empty">
      Ask Claude Code anything about your composition — modifications, explanations, new boards, new plugins.
      The canvas (above) is sent along as context, and what you build here populates back into it.
    </div>`;
    return;
  }
  root.innerHTML = "";
  for (const msg of chatState.history) {
    const msgEl = document.createElement("div");
    msgEl.className = `chat-msg ${msg.role}`;

    const meta = document.createElement("div");
    meta.className = "chat-msg-meta";
    const role = document.createElement("span");
    role.className = `chat-msg-role ${msg.role}`;
    role.textContent = msg.role === "user" ? "you" : "claude";
    meta.appendChild(role);
    if (msg.canvas && (msg.canvas.board || msg.canvas.ensemble)) {
      const ctx = document.createElement("span");
      ctx.className = "chat-canvas-ctx";
      ctx.textContent = `canvas: ${msg.canvas.board ?? "—"} + ${msg.canvas.ensemble ?? "—"}`;
      meta.appendChild(ctx);
    }
    msgEl.appendChild(meta);

    const body = document.createElement("div");
    body.className = `chat-msg-body ${msg.role}`;
    if (msg.role === "assistant") {
      if (msg.content) {
        body.innerHTML = renderMarkdown(msg.content);
      } else {
        body.innerHTML = `<span class="chat-thinking">thinking…</span>`;
      }
    } else {
      body.textContent = msg.content;
    }
    msgEl.appendChild(body);
    root.appendChild(msgEl);
  }
  root.scrollTop = root.scrollHeight;
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

// ── Apiary canvas (C-1 — mocked rocker-rig) ───────────────────────────
//
// Per SPEC-APIARY-COMPOSE.md §5. Renders the apiary header + role-
// ensemble cards + per-target rows from a hardcoded mock state. C-2
// will replace the mock with real `r2.composer.apiary.active` payload.
//
// The mock is the worked example from §12 — r2-workshop's rocker rig:
// 4 role-ensembles, 6 compile targets, controller + webapp-server
// co-located on one linux box.

const MOCK_APIARY = {
  name: "rocker-rig",
  class: "nz.ac.auckland.rocker",
  classHash: "0x624c47bc",
  version: "0.2.0",
  tg: { keyholderFingerprint: "ab12…cd34" },
  roles: [
    {
      role: "sensor",
      ensemble: "rocker-sensor",
      sentantCount: 15,
      targets: [
        { id: "sensor:esp32-s3-devkitc",
          type: "mcu-fw",  host: "esp32-s3-devkitc",
          overrides: { "ai.reality2.cap.accel.triaxial": "adxl355" },
          status: "ready",  lastBuilt: "2h ago" },
        { id: "sensor:esp32-s3-xiao",
          type: "mcu-fw",  host: "esp32-s3-xiao",
          overrides: { "ai.reality2.cap.accel.triaxial": "adxl355" },
          status: "ready",  lastBuilt: "2h ago" },
        { id: "sensor:esp32-c6-dfr1117",
          type: "mcu-fw",  host: "esp32-c6-dfr1117",
          overrides: { "ai.reality2.cap.accel.triaxial": "lis2dh" },
          status: "warning",  warning: "ensemble compile_target excludes esp32-c6" },
      ],
    },
    {
      role: "controller",
      ensemble: "rocker-controller",
      sentantCount: 8,
      targets: [
        { id: "controller:linux-x86_64",
          type: "native",  host: "linux-x86_64",
          status: "metadata-only",  note: "ensemble pending source extraction" },
      ],
    },
    {
      role: "webapp-server",
      ensemble: "rocker-webapp-server",
      sentantCount: 4,
      targets: [
        { id: "webapp-server:linux-x86_64",
          type: "beam",    host: "linux-x86_64",
          coLocatedWith: "controller",
          status: "metadata-only" },
      ],
    },
    {
      role: "viewer",
      ensemble: "rocker-viewer",
      sentantCount: 5,
      targets: [
        { id: "viewer:wasm32-browser",
          type: "wasm",    host: "wasm32-browser",
          status: "metadata-only" },
      ],
    },
    {
      role: "keyholder",
      ensemble: "rocker-keyholder",
      sentantCount: 3,
      targets: [
        { id: "keyholder:esp32-s3-keyholder-tag",
          type: "mcu-fw",  host: "esp32-s3-keyholder-tag",
          status: "metadata-only" },
      ],
    },
  ],
};

// Live apiary state. Starts as MOCK_APIARY so the canvas has something
// to render even before the orchestrator emits `r2.composer.apiary.active`.
// When the orchestrator IS launched with `--apiary <name>`, it sends the
// real state right after hello and we swap.
let apiaryState = MOCK_APIARY;
let apiaryIsLive = false;

function attachApiaryCanvas() {
  renderApiaryAll();

  $("apiary-compile-all").addEventListener("click", () => {
    // C-3 will fire r2.composer.apiary.build.start here.
    const tag = apiaryIsLive ? "" : "(mock) ";
    consoleLine("sys", `${tag}compile-all clicked — ${countTargets(apiaryState)} targets would dispatch`);
    switchTab("build");
  });
}

function renderApiaryAll() {
  renderApiaryHeader(apiaryState);
  renderApiaryRoles(apiaryState);
  renderApiarySummary(apiaryState);
  // Update the mode-hint chip to reflect whether we're live or mocked.
  const hint = $("mode-hint");
  if (hint) {
    hint.innerHTML = apiaryIsLive
      ? `live · <code>${escape(apiaryState.name)}</code>`
      : `v0.1 mock — open an apiary later via <code>r2.composer.apiary.open</code>`;
  }
}

// Called from the /r2 onEvent dispatch when the orchestrator emits
// `r2.composer.apiary.active`.
function onApiaryActive(payload) {
  if (!payload || typeof payload !== "object") return;
  apiaryState = payload;
  apiaryIsLive = true;
  renderApiaryAll();
  consoleLine("sys", `apiary loaded — ${escape(payload.name)} (${countTargets(payload)} targets)`);
}

function renderApiaryHeader(apiary) {
  const el = $("apiary-header");
  el.innerHTML = `
    <div class="apiary-id">
      <span class="apiary-label">Apiary</span>
      <span class="apiary-name">${escape(apiary.name)}</span>
    </div>
    <div class="apiary-tg">
      <span class="apiary-tg-label">TG</span>
      <span class="apiary-tg-class">${escape(apiary.class)}</span>
      <span class="apiary-tg-hash" title="FNV-1a-32 of class string">${escape(apiary.classHash)}</span>
    </div>
    <div class="apiary-kh">
      <span class="apiary-kh-label">KeyHolder</span>
      <span class="apiary-kh-fp" title="SHA-256 prefix of the KeyHolder public key">${escape(apiary.tg.keyholderFingerprint)}</span>
    </div>
    <div class="apiary-version">v${escape(apiary.version)}</div>
  `;
}

function renderApiaryRoles(apiary) {
  const root = $("apiary-roles");
  root.innerHTML = "";
  for (const role of apiary.roles) {
    const card = document.createElement("div");
    card.className = "apiary-role";
    card.dataset.role = role.role;

    const header = document.createElement("div");
    header.className = "apiary-role-header";
    header.innerHTML = `
      <span class="apiary-role-name">${escape(role.role)}</span>
      <span class="apiary-role-ensemble">${escape(role.ensemble)}</span>
      <span class="apiary-role-meta">${role.sentantCount} sentants · ${role.targets.length} target${role.targets.length === 1 ? "" : "s"}</span>
    `;
    card.appendChild(header);

    const targets = document.createElement("div");
    targets.className = "apiary-targets";
    for (const t of role.targets) {
      const row = document.createElement("div");
      row.className = "apiary-target";
      row.dataset.targetId = t.id;
      row.dataset.status = t.status;

      const left = `
        <span class="target-type" data-type="${t.type}">${escape(t.type)}</span>
        <span class="target-host">${escape(t.host)}</span>
      `;
      const overrides = t.overrides ? Object.entries(t.overrides)
        .map(([cap, plugin]) => `<span class="target-override"><code>${escape(shortCap(cap))}</code>=${escape(plugin)}</span>`)
        .join(" ") : "";
      const status = `<span class="target-status" data-status="${t.status}">${escape(t.status)}</span>`;
      const aux = t.coLocatedWith
        ? `<span class="target-coloc">co-located with <em>${escape(t.coLocatedWith)}</em></span>`
        : (t.warning ? `<span class="target-warn">⚠ ${escape(t.warning)}</span>`
        : (t.note ? `<span class="target-note">${escape(t.note)}</span>`
        : (t.lastBuilt ? `<span class="target-lastbuilt">built ${escape(t.lastBuilt)}</span>` : "")));

      row.innerHTML = `
        <div class="target-row-main">${left}${overrides}</div>
        <div class="target-row-meta">${status}${aux}</div>
      `;
      targets.appendChild(row);
    }
    card.appendChild(targets);

    root.appendChild(card);
  }
}

function renderApiarySummary(apiary) {
  const n = countTargets(apiary);
  $("apiary-summary").textContent = `${apiary.roles.length} role-ensembles · ${n} target${n === 1 ? "" : "s"}`;
  $("apiary-compile-all").disabled = false;
}

function countTargets(apiary) {
  return apiary.roles.reduce((acc, r) => acc + r.targets.length, 0);
}

// Trim a capability string for compact display: keep the last 2 path segments.
function shortCap(s) {
  const parts = s.split(".");
  return parts.length > 2 ? parts.slice(-2).join(".") : s;
}

// ── Canvas mode switcher (Apiary | Quick build) ───────────────────────

function attachCanvasModeSwitcher() {
  for (const btn of document.querySelectorAll(".mode-tab")) {
    btn.addEventListener("click", () => switchCanvasMode(btn.dataset.mode));
  }
}

function switchCanvasMode(mode) {
  document.querySelectorAll(".mode-tab").forEach((t) => {
    t.classList.toggle("active", t.dataset.mode === mode);
  });
  $("canvas-apiary").classList.toggle("hidden", mode !== "apiary");
  $("canvas").classList.toggle("hidden", mode !== "quick");
}
