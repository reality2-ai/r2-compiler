# SPEC-APIARY-CREATE: new-apiary workflow — chat-driven scaffold, TG genesis, git, library

**Version:** 0.1 Draft
**Date:** 2026-06-01
**Status:** Normative Draft
**Depends on:**
- **r2-composer specs:** [`SPEC-R2-COMPOSER.md`](SPEC-R2-COMPOSER.md) (event vocabulary, two-hive architecture, sentants + plugins), [`SPEC-APIARY-LAYOUT.md`](SPEC-APIARY-LAYOUT.md) (apiary directory + `apiary.toml`), [`SPEC-APIARY-COMPOSE.md`](SPEC-APIARY-COMPOSE.md) (compose tree, canvas surface), [`SPEC-CATALOGUE-LAYOUT.md`](SPEC-CATALOGUE-LAYOUT.md) (boards/ensembles/plugins/sentants)
- **Upstream R2:** R2-TRUST (Ed25519, §2.3 one-TG-per-hive), R2-CAP §3 (reverse-DNS class strings), R2-DEF §7 (ensemble scores), RFC 2119 + RFC 8174 (normative keywords)

## Conventions

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHALL**, **SHALL NOT**, **SHOULD**, **SHOULD NOT**, **RECOMMENDED**, **MAY**, and **OPTIONAL** in this document are to be interpreted as described in BCP 14 (RFC 2119, RFC 8174) when, and only when, they appear in all capitals.

---

## 1. Scope and audience

### 1.1 What this spec covers

The end-to-end workflow for **creating a new apiary** in r2-composer, from the operator's first action through to a fully-scaffolded apiary on disk with TG keys generated, git initialised, the first commit recorded, and (optionally) a remote git host configured. The workflow is **chat-driven** throughout — the operator describes intent in natural language; the AI assistant interprets, asks clarifying questions, and writes the on-disk artefacts.

This document spans four subsystems that ship as separate plugins + sentants in the orchestrator (per [[project-r2-composer-self-as-ensemble]]) — Apiary, Tg/KeyHolder, git_runner, Library — and binds them into one user-facing workflow.

### 1.2 What this spec does NOT cover

| Concern | Where it's specified |
|---|---|
| Apiary directory layout + `apiary.toml` schema | SPEC-APIARY-LAYOUT §2, §3 |
| Compose tree (role-ensembles, targets, plugin overrides) | SPEC-APIARY-COMPOSE |
| Catalogue entries (boards/ensembles/plugins/sentants on disk) | SPEC-CATALOGUE-LAYOUT |
| Per-target compile flow | SPEC-APIARY-COMPOSE §6 |
| Device flashing + roster state | SPEC-APIARY-FLASH |
| The orchestrator's two-hive event bus | SPEC-R2-COMPOSER §3 |

### 1.3 Position in the wider tool

The plugins + sentants this spec introduces live under `meta/ensembles/r2-composer-orchestrator/` per [[project-r2-composer-self-as-ensemble]]. They are R2-PLUGIN §12 / R2-DEF §2 conformant catalogue entries — the SAME shape as anything in `catalogue/`. r2-composer is itself an R2 ensemble; the workflow this spec describes is one of the ensemble's behaviours.

The orchestrator + webapp hives **MUST** have access to the active apiary's TG context when running this workflow. The exact TG posture (per-apiary-only, r2-composer-own-TG, user-TG-linked) is a future-flexible concern per [[project-r2-composer-self-as-ensemble]]; this spec's wording is deliberately compatible with all three.

## 2. The five-step user journey (normative narrative)

The new-apiary workflow proceeds in five steps. Step ordering is **REQUIRED**; steps **MAY** internally overlap but the operator-visible commitment moments **MUST** follow this order.

### 2.1 Step 1 — Initiate, name, describe

The operator initiates from r2-composer's empty-canvas state (no apiary open). Two entry points are **REQUIRED**:

- **Chat utterance.** The operator types a free-form description in the chat input (e.g. *"I want to start an apiary for a peatland greenhouse rig"*). The webapp **MUST** emit `r2.composer.author.start{kind:"apiary", description}` to the orchestrator. No form, no modal.
- **"New apiary…" pill** in the header. The webapp **MUST NOT** open a form on click; it **MUST** seed the chat input with a placeholder utterance the operator can edit and send. The placeholder is **RECOMMENDED** as `"I want to start a new apiary for "` (trailing space, focus-after-cursor).

The orchestrator's Author sentant routes the `author.start` to a `claude -p` session whose system prompt (per SPEC-R2-COMPOSER §7.1 + SPEC-CATALOGUE-LAYOUT §7) splices in this spec's §2 + SPEC-APIARY-LAYOUT §3 as the brief.

The AI **MUST** elicit:
- Apiary **name** — kebab-case, GH-safe, regex `^[a-z0-9][a-z0-9._-]*$`, unique among existing `apiaries/<name>/` directories.
- Apiary **class** — reverse-DNS per R2-CAP §3. If the operator has a cached `[author] default_class_prefix` in `~/.config/r2-composer/config.toml`, the AI **SHOULD** propose `<prefix>.<name>` first.
- Apiary **description** — one paragraph.

The AI **SHOULD** propose candidate values and ask the operator to confirm or amend; it **MUST NOT** demand the operator type structured data. Each candidate's emergence **SHOULD** stream as `r2.composer.apiary.draft` events (§4) so the canvas can show a peripheral "(drafting…)" card without polling.

### 2.2 Step 2 — TG keys (generate or import)

After name + class + description are confirmed, the AI **MUST** ask whether to:

- Generate a fresh Ed25519 keypair (the **RECOMMENDED** default for new apiaries), OR
- Import an existing keypair (for forking an apiary, rotating a KeyHolder, restoring from backup).

The operator's choice is carried as the `tg.intent` field of `r2.composer.apiary.create` (§4). The TG operation runs **inside the apiary.create transaction** (§3) — it is not a separate operator action.

For **generate**:
1. Orchestrator's `keyholder` plugin generates an Ed25519 keypair using OS CSPRNG.
2. Private key written to `~/.config/r2-composer/apiaries/<name>/tg_signer/tg_priv.bin` with mode `0600`; the containing directory **MUST** be mode `0700`.
3. Public key written to `<apiary-root>/trust_keys/tg_pub.bin` (in-tree, committable).
4. Public-key fingerprint computed as lowercase-hex SHA-256 of `tg_pub.bin` contents. Written to `<apiary-root>/trust_keys/tg_meta.toml`:
   ```toml
   keyholder_fp = "a1b2c3d4..."   # SHA-256 of tg_pub.bin
   algorithm    = "ed25519"
   generated_at = "2026-06-01T..."
   ```
5. The fingerprint **MUST** also be written into `apiary.toml [tg].keyholder_fp` so the apiary-open path can detect TG/disk mismatch.

For **import**:
1. The operator points the AI at an existing private-key file (path in chat) OR uploads bytes via a forthcoming `r2.composer.tg.import` event (out of scope for v0.1 — defer).
2. v0.1 path: operator places `tg_priv.bin` at the off-tree path before calling create with `tg.intent = "import"`. The orchestrator reads + validates (32 bytes, matching `tg_pub.bin` if present).
3. v0.1 import REQUIRES the public key to already exist at the in-tree path (i.e. the operator is restoring a previously-extracted apiary). If `tg_pub.bin` is missing, the orchestrator derives it from the private key and writes it.

### 2.3 Step 3 — Scaffold on disk

The orchestrator **MUST** create the apiary directory tree per SPEC-APIARY-LAYOUT §2 in a single atomic operation. REQUIRED files in the initial scaffold:

```
apiaries/<name>/
├── apiary.toml                  # populated from §2.1's name/class/description + §2.2's keyholder_fp
├── AI-CONTEXT.md                # generated stub — explains this apiary's purpose for fresh-CC pickup
├── README.md                    # human narrative — one paragraph; opens with "# <name>" H1
├── trust_keys/
│   └── tg_pub.bin               # from §2.2 (in-tree)
├── trust_keys/tg_meta.toml      # from §2.2
├── devices/
│   └── roster.toml              # empty: `[devices]` header only
├── scores/                      # empty
├── out/                         # empty (gitignored except releases/)
├── conversation/
│   └── <date>-create-apiary-01.md    # the transcript from this session
└── .gitignore                   # see §2.4 template
```

The `AI-CONTEXT.md` stub **MUST** cite this spec + SPEC-APIARY-LAYOUT, and **MUST** include the apiary's name + class + class-hash so a fresh Claude Code session opening this directory has immediate context.

### 2.4 Step 4 — git init + first commit

git initialisation is **AUTOMATIC** and runs **INSIDE** the `apiary.create` transaction. The operator does not separately request it; opting OUT is **OPTIONAL** per §7.

The orchestrator's `git_runner` plugin **MUST**:
1. `git init -b main` inside the new apiary directory (default branch name from §7).
2. Write the `.gitignore` template (Annex A).
3. Stage and commit the REQUIRED files: `apiary.toml`, `README.md`, `AI-CONTEXT.md`, `trust_keys/tg_pub.bin`, `trust_keys/tg_meta.toml`, `devices/roster.toml`, `.gitignore`, `conversation/<date>-create-apiary-01.md`.
4. Set `user.name` + `user.email` from the operator's global git config; fail with `E_GIT_USER_UNSET` if either is absent.
5. The commit message **MUST** be: `apiary: scaffold <name> (class <class>, TG fp <fp[:8]>)`.

The commit is **single** — TG genesis + scaffold + first-commit are one transaction. (Earlier design considered two commits — one for scaffold, one for TG — but the synthesis workflow §6 R1 settled on single-commit-with-everything because partial states are operationally confusing.)

### 2.5 Step 5 — Library pull, GitHub publish

The freshly-scaffolded apiary has zero `[[role_ensembles]]` and zero remotes. The closing AI prompt **MUST** invite the operator to:

- **Add role-ensembles** by describing what hardware roles the apiary needs, or by selecting catalogue entries from the **library panel** (the side-panel view of available boards/ensembles/plugins/sentants pulled from the configured libraries — see SPEC-LIBRARY [TBD]). The `library` plugin's `add` operation writes `[[role_ensembles]]` entries plus per-entry pins into `apiary.lock` for reproducibility.
- **Publish to GitHub** via `r2.composer.apiary.git.publish` — the `gh_runner` plugin shells out to `gh repo create --source . --remote origin --push` after asking for org/namespace + visibility (default `private`). This is **EXPLICIT** — never auto-fired by the create flow.

Both are post-creation actions, fired by the operator at their leisure. They are **OUT OF SCOPE** for the atomic transaction in §3.

## 3. Atomicity contract

### 3.1 Single transaction

The orchestrator's `Apiary` sentant **MUST** treat steps 2.2 (TG genesis) + 2.3 (scaffold) + 2.4 (git init + first commit) as **one atomic transaction**. Either ALL succeed and exactly one `r2.composer.apiary.active` event is emitted, or the transaction rolls back and exactly one `r2.composer.author.error` event is emitted. There **MUST NOT** be intermediate states observable to the operator.

### 3.2 Rollback discipline

If any of (scaffold | git init | first commit | TG generation) fails:

1. The orchestrator **MUST** delete the entire `apiaries/<name>/` directory it just created.
2. It **MUST** delete the off-tree `~/.config/r2-composer/apiaries/<name>/tg_signer/` directory.
3. It **MUST NOT** leave a half-built apiary on disk.

Ordering to minimise unrollbackable side-effects:

```
1. mkdir apiaries/<name>/                            # in-tree, easy to rm
2. mkdir off-tree tg_signer/                         # off-tree, easy to rm
3. write apiary.toml + AI-CONTEXT.md + README.md     # in-tree
4. generate Ed25519 keypair                          # in-memory
5. write tg_priv.bin off-tree (0600, chmod 0700 dir)
6. write tg_pub.bin + tg_meta.toml in-tree
7. write .gitignore + devices/roster.toml + conversation/
8. git init -b main + git add + git commit
9. emit r2.composer.apiary.active
```

Steps 1–7 are reversible by `rm -rf`. Step 8 (`git init`) is fully contained in the apiary dir. The only step that touches outside `apiaries/<name>/` is step 5 (off-tree TG), and that's reversed in §3.2 bullet 2.

### 3.3 Failure surface

A failed transaction emits ONE `r2.composer.author.error` with payload `{op: "apiary.create", code, message, hint?, stderr_tail?}`. The code is one of the §8 conformance gates. The chat thread continues — the operator can clarify and retry without restarting the create flow.

## 4. Event vocabulary (extends SPEC-R2-COMPOSER §4)

| Event | Direction | Payload | Purpose |
|---|---|---|---|
| `r2.composer.author.start` | webapp → orchestrator | `{kind: "apiary", description?: string}` | EXTENDS SPEC-R2-COMPOSER §4.4. Adds `apiary` to the kind enum. |
| `r2.composer.apiary.draft` | orchestrator → webapp | `{session_id, name?, description?, class?, stage: "naming"\|"describing"\|"classifying"\|"confirming"\|"committed"\|"abandoned"}` | **NEW.** Streamed by Author sentant as AI converges on each field. Advisory UI hint; not authoritative. |
| `r2.composer.apiary.create` | webapp → orchestrator (typically AI-tool-call) | `{name, description, class, tg: {intent: "generate"\|"import"}, path?}` | EXTENDS SPEC-APIARY-LAYOUT §7 with `tg.intent`. Fired by the AI's tool-call channel — the operator never types this payload manually. |
| `r2.composer.apiary.active` | orchestrator → webapp | full `ApiaryState` per SPEC-APIARY-LAYOUT | EXISTING. Emitted exactly once after `apiary.create` succeeds. |
| `r2.composer.tg.keyholder.progress` | orchestrator → webapp | `{phase: "generating"\|"writing_priv"\|"writing_pub"\|"computing_fp"\|"done", fp?: string}` | **NEW.** Ambient progress lines for TG generation inside the create transaction. |
| `r2.composer.tg.keyholder.error` | orchestrator → webapp | `{op, code, message, hint?, stderr_tail?}` | **NEW.** A TG-specific failure triggers the §3.2 rollback. |
| `r2.composer.author.file_added` | orchestrator → webapp | `{path, reason}` | EXISTING. Fires per file written during scaffold — `apiary.toml`, `AI-CONTEXT.md`, `tg_pub.bin`, etc. Surfaces as italic chat lines per [[feedback-calm-computing]]. |
| `r2.composer.apiary.git.init` | webapp → orchestrator | `{path?}` | EXISTING per SPEC-APIARY-LAYOUT §7. Used ONLY for re-opening an existing-but-not-yet-versioned apiary directory; the create flow folds this into `apiary.create`'s transaction without firing the event externally. |
| `r2.composer.apiary.git.publish` | webapp → orchestrator | `{remote_org, visibility: "private"\|"public"\|"internal"}` | EXISTING per SPEC-APIARY-LAYOUT §7. Step 2.5 only. **NEVER** auto-fired. |
| `r2.composer.git.publish.progress` | orchestrator → webapp | `{phase: "gh_auth_check"\|"creating_remote"\|"pushing"\|"done", url?: string}` | **NEW.** Ambient progress for the publish ceremony. |

## 5. Destructive-operation ceremony (normative)

Operations that overwrite or invalidate TG state, git history, or library pins **MUST** require an exact-match confirmation phrase typed by the operator in chat. The phrase format is **kebab-with-dashes-no-spaces**.

### 5.1 Confirmation-string registry

| Operation | Required exact phrase |
|---|---|
| `keyholder.generate{force:true}` (overwrite existing TG keypair) | `regenerate-tg-keys-i-understand-this-invalidates-all-devices` |
| `keyholder.delete` (purge off-tree private key) | `delete-tg-private-key-i-have-a-backup` |
| `apiary.delete` (rm -rf the apiary including git history) | `delete-apiary-<name>-i-understand-this-is-irreversible` |
| `git.wipe-history` | `wipe-git-history-i-understand-this-is-irreversible` |
| `library.publish` to a non-personal namespace | `publish-to-<org>-i-have-authority-to-publish-here` |

The phrase **MUST** be typed letter-for-letter; the orchestrator **MUST** reject mismatches with `E_DESTRUCTIVE_CONFIRM_MISMATCH` and **MUST NOT** offer a "did you mean…" auto-correct.

### 5.2 Chat-mediated only

These phrases **MUST NOT** be requestable via UI button; the operator types them in chat. The AI **MAY** display the phrase for the operator to copy, but **MUST** await the operator's own typed reply containing the exact phrase before proceeding.

## 6. Calm-computing conformance (normative)

Per [[feedback-calm-computing]]:

- The create flow **MUST NOT** display a modal dialog at any point.
- The create flow **MUST NOT** display a percentage progress bar, indeterminate spinner, or toast notification.
- All progress signals **MUST** appear as italic ambient lines in the chat (`author.file_added`) or as a peripheral updating apiary-list pill (`apiary.draft`).
- A failed create transaction **MAY** surface ONE calm-loud red strip on the apiary-list pill — this is the only escalation permitted in this workflow.
- Per [[feedback-ai-chat-primary]]: every state-changing operation in this workflow **MUST** route through an AI-emitted event, not a direct webapp-emitted event. The webapp's role is to render canvas state, accept text input, and forward AI tool-calls; it **MUST NOT** synthesise `apiary.create` payloads of its own.

## 7. Default policies (operator-overridable)

| Default | Value | Override |
|---|---|---|
| Auto-init git | ON | operator says *"don't init git"* in the create chat; the AI passes `git_init: false` in `apiary.create` |
| GitHub visibility on publish | `private` | operator chooses at publish time |
| Default library | `reality2-ai/r2-catalogue` (the in-repo `catalogue/` is a working-copy snapshot) | per-operator `~/.config/r2-composer/libraries.toml` |
| KeyHolder fingerprint algorithm | SHA-256, lowercase hex | not overridable in v0.1 |
| Default git branch | `main` | per-operator `~/.config/r2-composer/config.toml` |
| Apiary name regex | `^[a-z0-9][a-z0-9._-]*$` | not overridable |

## 8. Conformance gates (error codes)

This spec introduces the following error codes, in addition to those defined by SPEC-APIARY-LAYOUT §3.1:

| Code | Meaning |
|---|---|
| `E_APIARY_NAME_INVALID` | Proposed name fails the §7 regex |
| `E_APIARY_NAME_COLLISION` | A directory at `apiaries/<name>/` already exists |
| `E_APIARY_CLASS_INVALID` | Proposed class is not reverse-DNS per R2-CAP §3 |
| `E_APIARY_TG_INTENT_UNSUPPORTED` | `tg.intent` is neither `"generate"` nor `"import"` |
| `E_APIARY_TG_FP_MISMATCH` | On apiary open: `apiary.toml [tg].keyholder_fp` doesn't match the off-tree private key's derived public key |
| `E_KEYHOLDER_PRIV_PERMS` | Off-tree `tg_priv.bin` has mode other than `0600` |
| `E_KEYHOLDER_DIR_PERMS` | Off-tree `tg_signer/` directory has mode other than `0700` |
| `E_KEYHOLDER_IMPORT_MISSING` | `tg.intent: "import"` but no private key found at the off-tree path |
| `E_GIT_USER_UNSET` | git `user.name` or `user.email` not configured globally |
| `E_GIT_INIT_FAILED` | `git init` itself returned non-zero |
| `E_DESTRUCTIVE_CONFIRM_MISMATCH` | Operator's typed phrase did not exactly match the §5.1 entry |
| `E_GH_AUTH_MISSING` | `gh auth status` reports not-logged-in at publish time |
| `E_GH_SCOPE_MISSING` | `gh auth status` shows insufficient scopes (need `repo` minimum) |
| `E_LOCK_LIBRARY_UNCONFIGURED` | An `apiary.lock` pin references a library URL not in `~/.config/r2-composer/libraries.toml` |

## 9. Plugins + sentants this spec introduces (catalogue-shaped)

Per [[project-r2-composer-self-as-ensemble]], r2-composer's own machinery is structurally an R2 ensemble. The components this workflow needs are R2-PLUGIN §12 / R2-DEF §2 conformant catalogue entries that **MUST** live under `meta/ensembles/r2-composer-orchestrator/` (scaffolded Phase 1.6+):

| Component | Kind | Class | Role |
|---|---|---|---|
| `apiary` | plugin | `ai.reality2.composer.plugin.apiary` | Owns the `apiary.toml` lifecycle: create, load, save, validate, list. The atomic transaction in §3 lives here. |
| `Apiary` | sentant | `ai.reality2.composer.sentant.apiary` | FSM router: maps `r2.composer.apiary.*` events to the apiary plugin's operations. |
| `keyholder` | plugin | `ai.reality2.composer.plugin.keyholder` | Ed25519 keypair generation, off-tree private-key writes, fingerprint computation, signature operations. |
| `Tg` | sentant | `ai.reality2.composer.sentant.tg` | FSM router for TG operations during create + later cert issuance / rotation. |
| `git_runner` | plugin | `ai.reality2.composer.plugin.git-runner` | shells out to `git` for init, add, commit, branch, log, status. |
| `gh_runner` | plugin | `ai.reality2.composer.plugin.gh-runner` | shells out to `gh` for auth-status, repo-create, push. |
| `library` | plugin | `ai.reality2.composer.plugin.library` | Manages `libraries.toml`, fetches catalogue entries, writes/verifies `apiary.lock`. |
| `Library` | sentant | `ai.reality2.composer.sentant.library` | FSM router for `r2.composer.library.*`. |

Each component **MUST** conform to SPEC-CATALOGUE-LAYOUT §5 (plugins) or §6 (sentants) when its `meta/` entry is authored. v0.1 implementations live inline in `orchestrator/src/{plugins,sentants}/` per current code; promotion to catalogue-shaped meta-entries is a Phase 1.6+ deliverable.

## 10. Forward path

| Future concern | When |
|---|---|
| `r2.composer.tg.import` event with bytes-in-payload (not file-on-disk) | Phase 1.8+ |
| `keyholder` key export with confirmation ceremony | Phase 1.8+ |
| KeyHolder rotation (decommission old keypair, issue new while preserving TG identity) | Phase 2 |
| Multi-operator apiary collaboration (two operators editing the same apiary repo simultaneously) | Phase 2 |
| R2-WIRE-native libraries (replace git remotes per §7 with hive URLs + content hashes) | Phase 3 |
| Signed commits (orchestrator KeyHolder signs git commits with TG private key) | Phase 3 |
| Long-running r2-composer-own TG OR user-TG-linkage per [[project-r2-composer-self-as-ensemble]] update | Phase 3+ |

## Annex A — Default `.gitignore` template for an apiary

```gitignore
# r2-composer apiary .gitignore

# Build outputs (per SPEC-APIARY-LAYOUT §2 + SPEC-APIARY-COMPOSE §6.3) — only
# `releases/` is committed; per-build directories are local working state.
/out/*/
!/out/releases/

# Per-target Cargo workspaces rendered into out/ have their own target/
/out/**/target/

# WiFi credentials and any other operator secret material that ended up in-tree
# by accident — defence in depth (the proper path is off-tree).
wifi_config.toml
*.secret.toml
*.priv

# Editor / OS noise
.DS_Store
.idea/
.vscode/
*.swp
*.bak
```

The orchestrator's `git_runner` plugin writes this template verbatim during step 2.4. The operator **MAY** edit it afterwards; subsequent `apiary.create` operations on other apiaries always start from this template (no carry-over).

## 11. Change log

| Date | Version | Change |
|---|---|---|
| 2026-06-01 | 0.1 | Initial draft. Authored by synthesising a multi-agent workflow exploration (UX / TG keys / GitHub / library slices). Codifies the atomic-transaction model for `apiary.create`, the destructive-confirmation registry, and the catalogue-shaped plugin+sentant inventory per [[project-r2-composer-self-as-ensemble]]. Library pull (§2.5 forward reference) deferred to a follow-up SPEC-LIBRARY. |
