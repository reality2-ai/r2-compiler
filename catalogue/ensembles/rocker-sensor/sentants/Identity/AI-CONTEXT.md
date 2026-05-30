# AI-CONTEXT.md — sentants/Identity

Fresh-CC brief for picking up the Identity sentant cold.

## Purpose

The **first sentant** in the rocker-sensor ensemble's boot sequence. Loads (or generates + persists) the device's Ed25519 keypair + RBID, sets them in hive context for every other sentant to consume. Without Identity, no other sentant can sign frames; the device is unreachable.

## Conformance

- **R2-DEF §2** — sentant definition schema (name, class, description, storage, data, plugins, automations).
- **R2-DEF §8.1** — load-time validation (asserted, not yet machine-verified).
- **R2-COMPILE §3.1** — compilable subset (only AOT-compatible R2-DEF features used).
- **SPEC-CATALOGUE-LAYOUT §4.4** — directory layout under `catalogue/ensembles/<ensemble>/sentants/<Name>/`.

## FSM summary

5 states: `start → loading → (ready | generating → writing → ready)`.

Drives two plugin invocations during boot:
1. `nvs.read(device_keypair)` — load existing identity if present.
2. If missing: `software-ed25519.generate(seed)` → `nvs.write(device_keypair)` — generate + persist fresh identity.

Then enters `ready` and exposes the identity tuple via `get_identity` → `identity` event.

Full FSM diagram in [`SENTANT.md`](SENTANT.md) §"FSM".

## Plugins required

| Plugin | Lookup | Status |
|---|---|---|
| `nvs` | `crates/r2-plugin-storage-nvs/` (core, always linked — second-pass classification) | ✅ vendored + buildable (12 tests passing) |
| `software-ed25519` | `crates/r2-plugin-crypto-software-ed25519/` (core, always linked) | ✅ vendored |

Both plugin references now resolve — `E_SENT_PLUGIN_UNRESOLVED` is clear for this sentant.

## Events emitted / consumed

| Direction | Event | Public |
|---|---|---|
| inbound | `init` | no (auto at boot) |
| inbound | `nvs`, `crypto/software-ed25519` | no (plugin results) |
| inbound | `identity_ready`, `generate_identity` | no (internal self-sends for FSM branching) |
| inbound | `get_identity` | no |
| outbound | `identity { device_pk, rbid }` | YES — the consumer-pulled identity tuple. `device_sk` is intentionally excluded. |

## Known coupling

Identity is required by:

| Sentant | How it uses Identity |
|---|---|
| `Bootstrap` | Uses `device_pk` + `device_sk` to validate the KeyHolder-issued DeviceCertificate during enrolment. |
| `Beacon` | Embeds `rbid` in the R2-BEACON 28-byte AD. |
| `Uplink` | Signs `r2.sensor.announce` with `device_sk`; embeds `device_pk` + `rbid` in the announce payload. |
| `Recorder`, `Sync`, `Capture` | Stamp records with `rbid`. |
| `Ota` | Signature-verifies incoming firmware against the TG public key + the device's own DeviceCertificate. |

None of these are scaffolded yet (Phase 1.4-metadata for them is pending). The catalogue should flag `E_SENT_REFERENCE` if any of them is later authored without Identity already in scope.

## Working reference

`r2-workshop/firmware/esp32-c6/dfr1117/src/identity.rs` — the canonical implementation. The compiler plugin's job (Phase 1.7+) is to reproduce this file's behaviour from `sentant.yaml`. When extracting:

1. The state machine in this YAML maps cleanly to a Rust `enum State { Start, Loading, Generating, Writing, Ready }`.
2. Each transition becomes a `match (state, event) { ... }` arm.
3. Plugin invocations become direct `Plugin::execute(opcode, &data)` calls.
4. Action pipeline (`test` + `set` + `send`) inlines as Rust code: conditional + struct mutation + event-enqueue.
5. `{{platform.random_seed_32}}` template token resolves to `esp_random()` (or equivalent platform TRNG call) at code-gen time.

The hand-written identity.rs is **shorter and cleaner** than what a naive transition-by-transition codegen would emit. Don't expect 1:1 line equivalence — expect behavioural equivalence (R2-COMPILE §8) on the wire and in NVS.

## Read these files in this order (cold-start resume)

1. [`sentant.yaml`](sentant.yaml) — the contract.
2. [`SENTANT.md`](SENTANT.md) — narrative + FSM diagram + plugin coupling.
3. **Working reference:** `../../../../../../r2-workshop/firmware/esp32-c6/dfr1117/src/identity.rs` — the working code.
4. **Plugin specs that get invoked:**
   - `crates/r2-plugin-crypto-software-ed25519/plugin.toml` + `PLUGIN.md` (resolved)
   - `../../plugins/storage/nvs/plugin.toml` (when authored)
5. **Upstream specs:**
   - `../../../../../specifications/SPEC-CATALOGUE-LAYOUT.md` §4.4 (sentant directory layout)
   - `../../../../../../r2-specifications/specs/r2-core/R2-DEF.md` §2 (sentant schema) + §3.3 (action types)
   - `../../../../../../r2-specifications/specs/r2-core/R2-COMPILE.md` §3.1 (compilable subset)
   - `../../../../../../r2-specifications/specs/r2-core/R2-SENTANT.md` (the canonical sentant model)
6. The local [`conversation/`](conversation/) directory.

## Authoring status

- ✅ `sentant.yaml` (R2-DEF §2 conformant — asserted, not yet machine-verified)
- ✅ `SENTANT.md` (FSM diagram + coupling matrix)
- ✅ `AI-CONTEXT.md` (this file)
- ⏳ `examples/` — example event sequences for testing
- ✅ `conversation/2026-05-31-metadata-authored-01.md`

---

*Created 2026-05-31 as the first worked-example sentant entry. Phase 1.4-metadata.*
