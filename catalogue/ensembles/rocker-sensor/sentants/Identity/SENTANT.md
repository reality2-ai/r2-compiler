# Identity sentant

**Class:** `ai.reality2.workshop.sensor.identity`
**Storage:** `durable`
**Compilable:** ✅ R2-COMPILE §3.1 subset

## Purpose

One-shot at boot. Loads or creates the device's persistent **Ed25519 keypair** + **RBID** (FNV-1a-32 of `device_pk`) from NVS, sets them in the hive context, then enters `ready`. Every subsequent sentant — `Bootstrap`, `Beacon`, `Uplink`, anything that signs `r2.sensor.announce` — consumes the keypair from the hive context populated by this sentant.

This is the **first sentant** to run at boot. Without it, the rest of the rocker-sensor ensemble has no signing identity.

## FSM

States: `start`, `loading`, `generating`, `writing`, `ready`.

```
                   ┌───────┐ init       ┌─────────┐
                   │ start ├───────────►│ loading │
                   └───────┘            └────┬────┘
                                             │
                       nvs (status=ok,       │  nvs (status=error
                       data present)         │  OR data missing)
                             ▼               ▼
                     ┌─────────────┐    ┌────────────┐
                     │ identity_   │    │ generate_  │
                     │   ready ━━━ │    │ identity ━━│
                     └──┬──────────┘    └────┬───────┘
                        │                    │
                        │ → ready            ▼
                        │              ┌────────────┐
                        │              │ generating │
                        │              └────┬───────┘
                        │                   │
                        │   crypto/software-ed25519
                        │                   ▼
                        │              ┌─────────┐
                        │              │ writing │
                        │              └────┬────┘
                        │                   │  nvs (write done)
                        ▼                   ▼
                   ┌────────┐
                   │ ready  │ (get_identity → emits "identity" event for peers)
                   └────────┘
```

`identity_ready` and `generate_identity` are **internal self-sends** used to thread the test-action's branch outcome through the FSM (R2-DEF §3.3.5 — `test` branches actions, not state). This pattern keeps the FSM declarable without conditional `to:` fields.

## Plugins used

| Plugin | Purpose | Lives in |
|---|---|---|
| `nvs` | Persistent key-value store for the keypair | `../../plugins/storage/nvs/` (TBD — Phase 1.4-source) |
| `software-ed25519` | Generate fresh keypair on cache miss | `crates/r2-plugin-crypto-software-ed25519/` (core plugin, always linked) |

## Context the FSM exposes

After reaching `ready`, the hive context contains:

| Key | Type | Value |
|---|---|---|
| `device_pk` | `[u8; 32]` | Ed25519 public key |
| `device_sk` | `[u8; 32]` | Ed25519 secret seed — **never emitted on the wire** |
| `rbid` | `u32` | FNV-1a-32(device_pk) — the persistent device-identifier used in R2-BEACON / R2-WIRE |

## Events

Per R2-DEF §3.1, `Identity` has no public *interface* events under normal operation — it's a one-shot at boot. The single public event is `get_identity` (consumer-pulled): a sentant that wants the identity tuple emits `get_identity` to this sentant and gets an `identity` event back with `device_pk` + `rbid`. `device_sk` is intentionally NOT exposed by this event.

| Direction | Event | Public | Notes |
|---|---|---|---|
| inbound | `init` | no | Auto-triggered at sentant instantiation |
| inbound (result) | `nvs` | no | Plugin result envelope |
| inbound (result) | `crypto/software-ed25519` | no | Plugin result envelope |
| inbound (self-send) | `identity_ready` / `generate_identity` | no | Internal branching |
| inbound | `get_identity` | no | Other sentants pull the identity tuple |
| outbound | `identity` | yes | `{ device_pk, rbid }` — emitted in response to `get_identity` |

## Storage semantics

`storage: durable` per R2-DEF §2.3 — the sentant DEFINITION is durably stored (so the device knows to reload it on boot), but the **state** (FSM position + vars) is volatile. On every boot the sentant re-runs the FSM from `start`, which reads NVS afresh and reconstructs the in-memory `device_pk` / `device_sk` / `rbid` vars. The NVS read is what makes the identity persistent across reboots, not R2-DEF's `durable-state`.

This matches r2-workshop's `identity.rs` model.

## AOT compilation notes

This sentant is in the R2-COMPILE §3.1 compilable subset:
- States: enum (zero-cost). 5 states → 3 bits.
- Vars: fixed-size struct (`[u8; 32]` × 2 + `u32` = 68 B + alignment padding).
- Action pipeline: each transition compiles to a `match` arm; plugin calls inline as `Plugin::execute(opcode, &data)`.
- Template expressions: simple substitution + the `fnv32(...)` helper, which the Compiler sentant maps to a call to `r2_fnv::fnv1a_32`.

One non-template-expression in `sentant.yaml`: `{{platform.random_seed_32}}`. R2-DEF templates don't natively support platform calls; the Compiler sentant recognises this token and substitutes `esp_random()` (or the platform-appropriate TRNG) at code-gen time. This is a documented extension — see `[[project-compulsory-plugins-and-virgin-boards]]` for the broader pattern of "core capability synthesised by the compiler" (alongside the OTA receiver).

## Working reference

`r2-workshop/firmware/esp32-c6/dfr1117/src/identity.rs` is the authoritative implementation. The Compiler sentant's first proof of work is reproducing that file's behaviour from this `sentant.yaml`.

## Authoring status

- ✅ `sentant.yaml` (metadata-first; 2026-05-31)
- ✅ This `SENTANT.md`
- ✅ `AI-CONTEXT.md`
- ⏳ `examples/` — example event sequences for testing
- ✅ `conversation/2026-05-31-metadata-authored-01.md`
- ⏳ **R2-DEF §8.1 schema validation** — needs the orchestrator's CatalogueServer (Phase 1.6+) to actually run; this file is asserted-conformant but not machine-verified yet
