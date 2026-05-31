# SPEC-APIARY-LAYOUT: directory layout and `apiary.toml` schema for r2-composer apiaries

**Version:** 0.1 Draft
**Date:** 2026-05-31
**Status:** Normative Draft
**Depends on:**
- **Upstream (proposed amendment):** R2-APIARY at `r2-specifications/specs/r2-core/R2-APIARY.md` (see `SPEC-APIARY-AMENDMENT-PROPOSAL.md` in this directory — broadens R2-APIARY scope to encompass the TG-bound multi-hive case as the general definition)
- **Upstream (existing):** R2-TRUST (§2.3 one-TG-per-hive constraint), R2-ENSEMBLE, R2-DEF §7
- **r2-composer:** SPEC-R2-COMPOSER, SPEC-CATALOGUE-LAYOUT

---

## 1. Scope

This spec defines:

- The directory layout under `apiaries/<name>/` for an operator's deployment unit.
- The `apiary.toml` schema — the structured contract for one apiary.
- The relationship between an apiary, its role-ensembles, and the catalogue.
- Project-switch (= apiary-switch) semantics across the orchestrator + webapp hives.

It builds on the broader **apiary** concept as proposed in `SPEC-APIARY-AMENDMENT-PROPOSAL.md` — a TG-bound set of cooperating R2-stack instances (role-ensembles) that together deliver one deployment.

## 2. Apiary directory layout

```
apiaries/<name>/                          # entire apiary — committable to its own git repo
├── apiary.toml                           # REQUIRED — structured contract (§3)
├── README.md                             # human-authored apiary description
├── AI-CONTEXT.md                         # REQUIRED — fresh-CC brief specific to THIS apiary
├── trust_keys/                           # PUBLIC TG material (committable)
│   ├── tg_pub.bin                        # REQUIRED — TG public key
│   └── tg_cert.bin                       # OPTIONAL — TG-self-signed cert when applicable
├── devices/                              # per-device roster (§5)
│   ├── roster.toml
│   └── certs/                            # issued DeviceCertificates (public — committable)
├── scores/                               # generated R2-DEF §7 scores from past builds
│   └── <ensemble>-<carrier>-<timestamp>.yaml
├── conversation/                         # AI chat transcripts for THIS apiary
│   └── YYYY-MM-DD-<topic>-NN.md
├── out/                                  # built artefacts (typically .gitignored except releases/)
│   ├── <ensemble>-<carrier>-<timestamp>/
│   └── releases/                         # versioned production .bins (committable)
├── notes/                                # operator-authored markdown / images / measurements
└── .gitignore                            # excludes target/, out/<ensemble>-*/, …
```

**OFF-TREE** (per apiary, never in the repo):

```
~/.config/r2-composer/apiaries/<name>/tg_signer/
└── tg_priv.bin                           # TG private key — NEVER committed
```

The split between repo-committable + `~/.config` mirrors r2-workshop's existing `SECRETS-POLICY.md` pattern.

### 2.1 Apiaries can live anywhere

The `apiaries/` directory inside r2-composer is the **default** location. An apiary directory is structurally self-contained and CAN be:

- A subdirectory of r2-composer at `apiaries/<name>/` (the default).
- A separate git repository at any path on the operator's machine, opened by absolute path.
- A cloned repository (`git clone <apiary-repo>` then "Open apiary…" in the webapp).

The orchestrator's `r2.composer.apiary.open` event accepts any absolute path matching this layout.

## 3. `apiary.toml` schema

```toml
[apiary]
name        = "rocker-rig"                    # REQUIRED — directory name MUST match
description = "..."                            # REQUIRED — one paragraph
class       = "nz.ac.auckland.rocker"         # REQUIRED — R2-CAP §3 reverse-DNS string
class_hash  = "0x624c47bc"                    # OPTIONAL informational — FNV-1a-32 of class
version     = "0.2.0"                          # REQUIRED — semver
created     = "2026-05-31T10:00:00Z"          # REQUIRED — ISO 8601 UTC
ai_context  = "AI-CONTEXT.md"                  # OPTIONAL path; default is AI-CONTEXT.md

[tg]
# The single TG that scopes this apiary. Per R2-TRUST §2.3 every hive
# performing a part of this apiary is a member of EXACTLY this TG.
public_key     = "trust_keys/tg_pub.bin"      # REQUIRED — path relative to apiary root
keyholder_path = "~/.config/r2-composer/apiaries/rocker-rig/tg_signer/"   # REQUIRED — off-tree
keyholder_fp   = "<sha256 of keyholder pubkey>"   # REQUIRED — informational; lets the UI verify a loaded key matches

# Each role-ensemble in this apiary. References catalogue entries by slug.
# Per [[project-ensemble-equals-project]]: one apiary contains many role-ensembles
# (sensor + controller + viewer + keyholder + …) sharing the apiary's class.
[[role_ensembles]]
role     = "sensor"                            # human-readable role identifier
ensemble = "rocker-sensor"                     # → catalogue/ensembles/rocker-sensor/
carriers = [                                   # one or more carrier-variants
  "esp32-c6-dfr1117",
  "esp32-s3-devkitc",
  "esp32-s3-xiao",
]
device_count_planned = 8                       # OPTIONAL — planning hint

[[role_ensembles]]
role     = "controller"
ensemble = "rocker-controller"                 # → catalogue/ensembles/rocker-controller/ (TBD)
carriers = ["linux-x86_64"]
device_count_planned = 1

[[role_ensembles]]
role     = "viewer"
ensemble = "rocker-viewer"
carriers = ["wasm32-browser"]
device_count_planned = 0                       # unbounded — browsers join via QR/link

[[role_ensembles]]
role     = "keyholder"
ensemble = "rocker-keyholder"
carriers = ["linux-x86_64"]
device_count_planned = 1
co_located_with = "controller"                 # OPTIONAL — when one process holds both roles

# Per [[feedback-core-vs-optin-plugins]] three-tier model:
# r2-hive-core has an always-on tier AND a switchable tier. This table
# overrides the switchable defaults FOR THIS APIARY.
[switchable_core]
ota = true                                     # default; flip false for one-shot demos that never need OTA

[paths]
# Where the apiary's various artefacts live (relative to apiary root).
# All have sensible defaults — only override when relocating.
scores       = "scores/"
conversation = "conversation/"
out          = "out/"
devices      = "devices/"
trust_keys   = "trust_keys/"
```

### 3.1 Validation rules

The orchestrator's `apiary` plugin MUST enforce at open time:

| Rule | Error |
|---|---|
| `apiary.name` matches the directory name | `E_APIARY_NAME` |
| `apiary.class` is a non-empty string and follows R2-CAP §3 reverse-DNS convention | `E_APIARY_CLASS` |
| `tg.public_key` exists and parses as a 32-byte Ed25519 public key | `E_APIARY_TG_PUB` |
| `tg.keyholder_path` is writable (or readable for the matching `tg_priv.bin`) | `E_APIARY_KEYHOLDER` |
| Each `role_ensembles[].ensemble` resolves to `catalogue/ensembles/<name>/` with a valid `ensemble.yaml` | `E_APIARY_ENSEMBLE_MISSING` |
| Each `role_ensembles[].carriers[]` resolves to `catalogue/boards/<slug>/` with a valid `board.toml` | `E_APIARY_CARRIER_MISSING` |
| Every ensemble's `compile_target` overlaps with at least one declared carrier | `E_APIARY_TARGET_MISMATCH` |
| `switchable_core` keys match the registered switchable-core capabilities | `E_APIARY_SWITCHABLE_UNKNOWN` |
| When `[co_located_with]` is set on a role, the target role MUST exist in the same apiary | `E_APIARY_COLOC` |

## 4. The single-TG constraint

Per R2-TRUST §2.3, every hive performing a part of this apiary belongs to EXACTLY the TG declared in `apiary.toml [tg]`. This applies uniformly to:

- The orchestrator-hive (workstation Rust binary) — KeyHolder for this apiary's TG.
- The webapp-hive (browser WASM) — TG-member via the QR/link enrolment flow.
- Sensor firmware — TG-member via the BLE-bootstrap flow (apiary's TG pub key baked at compile time, KeyHolder-signed DeviceCertificate received at first boot).
- Any controller / viewer / keyholder hives in the role-ensemble set.

**Switching to a different apiary changes the active TG context** (unless two apiaries deliberately share a TG by setting the same `tg.public_key`). See §6.

## 5. Device roster (`devices/roster.toml`)

```toml
# Per-device records across all role-ensembles in this apiary.
# Per [[project-compulsory-plugins-and-virgin-boards]] tracks deploy state.

[[devices]]
device_pk    = "0x<32-byte hex>"               # public key — primary identifier
rbid         = "0x<u32 hex>"                    # FNV-1a-32 of device_pk
role         = "sensor"                         # matches [[role_ensembles]].role
ensemble     = "rocker-sensor"
carrier      = "esp32-c6-dfr1117"
name_alias   = "rocker-A1"                      # OPTIONAL — operator-set
enrolled     = "2026-05-15T..."
last_seen    = "2026-05-31T..."
deploy_state = "flashed_ota_capable"            # never_built | built | flashed_ota_capable | unreachable
firmware_sha = "<sha256 of last-flashed .bin>"
firmware_ver = "0.2.0+abc123"
cert_status  = "valid"                          # valid | revoked | expired
```

Updated on every successful build, deploy, OTA, revoke, or beacon-observation event.

## 6. Apiary switching semantics

Per [[project-ensemble-equals-project]] §"TG constraint on project switching", switching apiaries is a **TG-context switch** that affects every running hive instance:

| Hive | Behaviour on apiary switch |
|---|---|
| Orchestrator | Drops current TG cert + KeyHolder context; loads the new apiary's `tg_pub.bin` + matching `tg_priv.bin` from off-tree. |
| Webapp browser hive | Looks up the new apiary's class hash in IndexedDB-stored cert list. If a matching cert exists, presents it. If not, prompts re-enrolment via QR/link flow. |
| Catalogue browser | Re-scopes the device-roster pane to the new apiary's `devices/roster.toml`. Catalogue itself stays unchanged (it's the shared library across apiaries). |
| In-flight builds | Aborts with `r2.composer.build.error{phase: "apiary_switched"}` OR completes against the OLD apiary's TG (operator preference at switch time). |
| Live R2-WIRE subscriptions | Tear down for old TG; re-subscribe under new TG. |

**Multi-cert browser layout**: the webapp's IndexedDB stores `{ <tg_class_hash>: <device_keypair + cert> }`. On apiary switch the browser picks the matching pair; only enrols when it sees a new TG class hash for the first time.

**Two apiaries sharing a TG**: legal — both reference the same `tg_pub.bin`. The orchestrator detects the shared TG and skips the heavy re-bind; only the composition + roster + conversation pane swap.

## 7. Events (extension to SPEC-R2-COMPOSER §4)

| Event | Direction | Payload | Purpose |
|---|---|---|---|
| `r2.composer.apiary.list` | webapp → orchestrator | `{}` | List known apiaries in `apiaries/` + recent-opened paths |
| `r2.composer.apiary.entry` | orchestrator → webapp | `{ name, class, version, last_modified, path }` | Streamed per-apiary summary |
| `r2.composer.apiary.open` | webapp → orchestrator | `{ path }` (absolute or relative to repo) | Open an apiary; orchestrator scopes all subsequent state |
| `r2.composer.apiary.active` | orchestrator → webapp | full hydrated state (apiary.toml + roster + recent transcripts) | Sent after open |
| `r2.composer.apiary.create` | webapp → orchestrator | `{ name, description, class, tg_class_string, path? }` | Scaffold a new apiary + generate its TG keypair |
| `r2.composer.apiary.save` | webapp → orchestrator | `{}` | Persist (typically implicit; this is an explicit checkpoint) |
| `r2.composer.apiary.close` | webapp → orchestrator | `{}` | Tear down active state |
| `r2.composer.apiary.git.init` | webapp → orchestrator | `{}` | `git init` inside the active apiary + write `.gitignore` template |
| `r2.composer.apiary.git.publish` | webapp → orchestrator | `{ remote_org, visibility }` | `gh repo create` + push |

All other `r2.composer.*` events (build, deploy, author, tg, …) become implicitly scoped to the active apiary once open.

## 8. Conformance

An `apiaries/<name>/` directory conforms when:

1. `apiary.toml` validates per §3.1.
2. `trust_keys/tg_pub.bin` exists and parses.
3. `AI-CONTEXT.md` exists at the apiary root.
4. Every referenced ensemble + carrier resolves against the catalogue.
5. The `[tg]` keyholder path either exists with a valid private key OR the operator is in a state where they're about to import / generate one.

The orchestrator reports any non-conformance as `degraded` in apiary listings; degraded apiaries can be opened in read-only mode but cannot drive builds / deploys.

## 9. Change log

| Date | Version | Change |
|---|---|---|
| 2026-05-31 | 0.1 | Initial draft. Settles the second-pass apiary concept: directory layout, `apiary.toml` schema, TG-switch semantics, event vocabulary. Companion to `SPEC-APIARY-AMENDMENT-PROPOSAL.md` which proposes the matching upstream R2-APIARY scope broadening. |
