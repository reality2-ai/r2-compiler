#!/usr/bin/env python3
"""
tools/build-catalogue-index.py — emit webapp/dist/manifest.json from the
on-disk catalogue tree.

The minimal browseable webapp (Phase 2-preview) is a static HTML+JS page
that reads this manifest and renders the catalogue. No TOML/YAML parsing
in the browser; structured data is pre-extracted here.

Walks:
- catalogue/boards/<slug>/board.toml
- catalogue/ensembles/<slug>/ensemble.yaml
- catalogue/ensembles/<slug>/plugins/<cat>/<name>/plugin.toml
- catalogue/ensembles/<slug>/sentants/<Name>/sentant.yaml

For each entry, records:
- slug, kind, path, display name, description, version
- list of files (so the viewer can fetch them on click)

YAML support uses a minimal hand-rolled top-level reader (stdlib only —
no pyyaml dep). The ensemble.yaml + sentant.yaml files only need their
top-level scalar fields surfaced; the JS viewer just shows the raw text
when the operator clicks the file.

Usage:
    python3 tools/build-catalogue-index.py
    # → webapp/dist/manifest.json
"""

from __future__ import annotations

import json
import re
import sys
import tomllib
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[1]
CATALOGUE = REPO_ROOT / "catalogue"
OUT_PATH = REPO_ROOT / "webapp" / "dist" / "manifest.json"


def read_yaml_toplevel(path: Path) -> dict[str, str]:
    """
    Extract top-level scalar fields from a YAML file under a single root
    mapping (e.g. `sentant:` or `ensemble:`). Handles only the cases we
    need — string values, multi-line `>` folded scalars, numbers.

    Not a full YAML parser. The JS viewer shows the raw file; this is
    only for the manifest's display-name/description/version preview.
    """
    fields: dict[str, str] = {}
    if not path.exists():
        return fields
    text = path.read_text(encoding="utf-8")
    in_root = False
    pending_key: str | None = None
    pending_lines: list[str] = []

    def commit_pending() -> None:
        nonlocal pending_key, pending_lines
        if pending_key is not None:
            joined = " ".join(s.strip() for s in pending_lines).strip()
            fields[pending_key] = joined
        pending_key = None
        pending_lines = []

    for raw_line in text.splitlines():
        line = raw_line.rstrip()
        if not line or line.lstrip().startswith("#"):
            continue
        # Top-level root key (e.g. `sentant:` or `ensemble:`).
        if not line.startswith(" ") and line.endswith(":") and ":" in line:
            commit_pending()
            in_root = True
            continue
        if not in_root:
            continue
        # Second-level key (2 spaces indent).
        m = re.match(r"^  ([A-Za-z_]+):\s*(.*)$", line)
        if m:
            commit_pending()
            key, val = m.group(1), m.group(2)
            if val == "" or val == ">":
                pending_key = key
                pending_lines = []
            else:
                fields[key] = val.strip().strip('"').strip("'")
        elif pending_key and line.startswith("    "):
            pending_lines.append(line.strip())
        else:
            commit_pending()
    commit_pending()
    return fields


def list_files(dir: Path) -> list[dict[str, str]]:
    """List all files under `dir` (recursive), relative to repo root."""
    files: list[dict[str, str]] = []
    if not dir.exists():
        return files
    for p in sorted(dir.rglob("*")):
        if p.is_file() and "conversation" not in p.parts:
            # Skip transient transcript files from the file viewer for v0.1
            # (keeps the tree small; transcripts are still on disk and
            # the operator can still open them by URL if needed).
            files.append(
                {
                    "path": str(p.relative_to(REPO_ROOT)),
                    "name": p.name,
                    "kind": p.suffix.lstrip(".") or "no-ext",
                }
            )
    return files


def scan_board(dir: Path) -> dict[str, Any] | None:
    board_toml = dir / "board.toml"
    if not board_toml.exists():
        return None
    with board_toml.open("rb") as f:
        data = tomllib.load(f)
    board = data.get("board", {})
    compulsory = data.get("compulsory_plugins", {})
    return {
        "kind": "board",
        "slug": dir.name,
        "path": str(dir.relative_to(REPO_ROOT)),
        "display_name": board.get("description", "").split(".")[0].strip()
        or board.get("name", dir.name),
        "name": board.get("name", dir.name),
        "version": board.get("version", "?"),
        "description": board.get("description", ""),
        "arch": board.get("arch", ""),
        "chip": board.get("chip", ""),
        "carrier": board.get("carrier", ""),
        "target_triple": data.get("build", {}).get("target_triple", ""),
        "compile_target_tag": data.get("compile_target", {}).get("tag", ""),
        "flash_size_mb": data.get("build", {}).get("flash_size_mb", 0),
        "psram": data.get("build", {}).get("psram", False),
        "compulsory_capabilities": compulsory.get("capabilities", []),
        "files": list_files(dir),
    }


def scan_plugin(dir: Path) -> dict[str, Any] | None:
    plugin_toml = dir / "plugin.toml"
    if not plugin_toml.exists():
        return None
    with plugin_toml.open("rb") as f:
        data = tomllib.load(f)
    plugin = data.get("plugin", {})
    modes = data.get("modes", {})
    return {
        "kind": "plugin",
        "category": plugin.get("category", ""),
        "slug": dir.name,
        "path": str(dir.relative_to(REPO_ROOT)),
        "name": plugin.get("name", dir.name),
        "version": plugin.get("version", "?"),
        "description": plugin.get("description", ""),
        "modes": {
            "aot": modes.get("aot") is not None and modes.get("aot") is not False,
            "nif": modes.get("nif") is not None and modes.get("nif") is not False,
            "web": modes.get("web") is not None and modes.get("web") is not False,
        },
        "provides": data.get("capabilities", {}).get("provides", []),
        "requires": data.get("capabilities", {}).get("requires", []),
        "commands": list((data.get("commands") or {}).keys()),
        "files": list_files(dir),
    }


def scan_sentant(dir: Path) -> dict[str, Any] | None:
    sentant_yaml = dir / "sentant.yaml"
    if not sentant_yaml.exists():
        return None
    fields = read_yaml_toplevel(sentant_yaml)
    return {
        "kind": "sentant",
        "slug": dir.name,
        "path": str(dir.relative_to(REPO_ROOT)),
        "name": fields.get("name", dir.name),
        "class": fields.get("class", ""),
        "description": fields.get("description", ""),
        "storage": fields.get("storage", ""),
        "files": list_files(dir),
    }


def scan_ensemble(dir: Path) -> dict[str, Any] | None:
    ensemble_yaml = dir / "ensemble.yaml"
    if not ensemble_yaml.exists():
        return None
    fields = read_yaml_toplevel(ensemble_yaml)
    plugins: list[dict[str, Any]] = []
    plugins_root = dir / "plugins"
    if plugins_root.exists():
        for cat_dir in sorted(p for p in plugins_root.iterdir() if p.is_dir()):
            for plug_dir in sorted(p for p in cat_dir.iterdir() if p.is_dir()):
                entry = scan_plugin(plug_dir)
                if entry:
                    plugins.append(entry)
    sentants: list[dict[str, Any]] = []
    sentants_root = dir / "sentants"
    if sentants_root.exists():
        for sent_dir in sorted(p for p in sentants_root.iterdir() if p.is_dir()):
            entry = scan_sentant(sent_dir)
            if entry:
                sentants.append(entry)
    return {
        "kind": "ensemble",
        "slug": dir.name,
        "path": str(dir.relative_to(REPO_ROOT)),
        "name": fields.get("name", dir.name),
        "class": fields.get("class", ""),
        "version": fields.get("version", "?"),
        "description": fields.get("description", ""),
        "compile_target": fields.get("compile_target", ""),
        "plugins": plugins,
        "sentants": sentants,
        "files": list_files(dir),
    }


def main() -> int:
    boards: list[dict[str, Any]] = []
    boards_root = CATALOGUE / "boards"
    if boards_root.exists():
        for d in sorted(p for p in boards_root.iterdir() if p.is_dir()):
            entry = scan_board(d)
            if entry:
                boards.append(entry)

    ensembles: list[dict[str, Any]] = []
    ensembles_root = CATALOGUE / "ensembles"
    if ensembles_root.exists():
        for d in sorted(p for p in ensembles_root.iterdir() if p.is_dir()):
            entry = scan_ensemble(d)
            if entry:
                ensembles.append(entry)

    manifest = {
        "schema_version": "0.1",
        "tool": "r2-compiler",
        "tool_repo": "https://github.com/reality2-ai/r2-compiler",
        "boards": boards,
        "ensembles": ensembles,
        "stats": {
            "boards": len(boards),
            "ensembles": len(ensembles),
            "plugins": sum(len(e["plugins"]) for e in ensembles),
            "sentants": sum(len(e["sentants"]) for e in ensembles),
        },
    }
    OUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    OUT_PATH.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")

    print(
        f"Wrote {OUT_PATH.relative_to(REPO_ROOT)} — "
        f"{manifest['stats']['boards']} boards, "
        f"{manifest['stats']['ensembles']} ensembles, "
        f"{manifest['stats']['plugins']} plugins, "
        f"{manifest['stats']['sentants']} sentants."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
