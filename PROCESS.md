# PROCESS.md — the five working rules

These rules govern every change in this repo. They are the same rules `r2-workshop/PROCESS.md` codifies, adapted to r2-composer's catalogue-authoring model.

## 1. Spec before code

Every meaningful behaviour change has a driving spec — either local to this repo (under [`specifications/`](specifications/)) or upstream in `r2-specifications/`. The spec wins disagreements unless the user re-opens.

If you're about to write code for a behaviour that no spec covers, **stop and write the spec first**. If the upstream spec is ambiguous, raise it against `r2-specifications` as a sharper question or a targeted edit — do not invent a r2-composer-local workaround.

## 2. Conversation is research data

Every working session appends a new file `conversation/YYYY-MM-DD-<topic>-NN.md` — verbatim user prompts, faithful AI responses, a **decisions table** at the end. Per-catalogue-entry sessions live in that entry's `conversation/` instead of the repo-wide one.

Never edit a closed session file retroactively. Append a new one if facts have changed.

## 3. Plan is consolidation; conversation accumulates

`plan/PLAN.md` is overwritten as work progresses — it's the consolidated current view of what's where. The conversation log accumulates indefinitely and is the *source* from which the plan is consolidated.

## 4. Catalogue conformance gate

A catalogue entry without all five of the following is incomplete, not published:

1. The canonical artefact (`plugin.toml` / `sentant.yaml` / `board.toml`) valid against its upstream spec.
2. A narrative markdown (`PLUGIN.md` / `SENTANT.md` / `BOARD.md`).
3. An entry-specific `AI-CONTEXT.md` sufficient for a fresh CC session to take over.
4. Reference material under `datasheets/` (downloaded, not just linked) where applicable.
5. A `conversation/YYYY-MM-DD-<topic>-NN.md` transcript of the authoring session.

PRs adding incomplete entries are blocked, not parked.

## 5. Secrets stay out. Cite sources. No compile mocking

- **No private keys** (TG signing keys, Ed25519 secret keys, API tokens), no WiFi credentials, no device UUIDs in the working tree. `.gitignore` blocks the patterns; *don't put them there in the first place* is the real rule.
- **Cite sources.** Spec section + filename for protocol claims; datasheet page + filename for hardware claims; vendor URL when fetching; `path:line` for code refs. An unchecked citation is a hallucination wearing a uniform.
- **No mocking of compile output.** If r2-composer claims a build succeeded, it must have run `cargo build` and observed exit 0. If a UI shows "build ok" without a real subprocess exit code behind it, that's a bug. Same for `esptool` flash.

---

Inspired by r2-workshop/PROCESS.md and the project-wide AGENTS.md discipline at `../r2-specifications/AGENTS.md`.
