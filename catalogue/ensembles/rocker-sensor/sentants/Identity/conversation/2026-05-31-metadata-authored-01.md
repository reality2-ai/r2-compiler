# 2026-05-31 — Identity sentant metadata authored

First worked-example sentant entry in r2-compiler's catalogue. Companion to `../../plugins/sensor/lis2dh/` (the first worked-example plugin). Validates SPEC-CATALOGUE-LAYOUT §4.4 + R2-DEF §2 conformance pattern.

## Context

Same session as the lis2dh plugin metadata. Phase 1.4-metadata = lightweight catalogue entries with the structured artefact (sentant.yaml) + narrative (SENTANT.md) + cold-start brief (AI-CONTEXT.md), with source extraction deferred to Phase 1.4-source.

`Identity` chosen as the first sentant because:
- It's the **first sentant to run at boot** in r2-workshop's rocker-sensor ensemble — every other sentant depends on it. Pinning it early lets later sentants reference it as a dependency they can rely on.
- The FSM is small enough to write out completely (5 states) but exercises multiple R2-DEF §2/§3 features: plugin invocations (fire-and-forget), result-event handling (`event: nvs` / `event: crypto/software-ed25519`), the `test` conditional action, the `set` data pipeline, internal self-sends for FSM branching (`identity_ready` / `generate_identity`).
- It demonstrates the **cross-plugin pattern** — uses both an ensemble-owned plugin (`nvs`) and a core plugin (`software-ed25519`), so the orchestrator's resolve step gets exercised against both lookup paths.
- It introduces the `{{platform.random_seed_32}}` template-extension token, which the Compiler sentant resolves to `esp_random()` at code-gen time. This is the "core capability synthesised by the compiler" pattern that also shows up in OTA (compulsory plugins).

## Decisions

| # | Decision |
|---|---|
| D-1 | The FSM is authored at full fidelity (5 states, 8 transitions) rather than as a stub. Metadata-first means the FSM is the contract; the source-extraction phase mechanises it. |
| D-2 | `storage: durable` not `durable-state`. The sentant DEFINITION is durably stored (so it reloads on boot), but the FSM RESETS to `start` on each boot. Persistence of the keypair itself lives in NVS, not in R2-DEF state snapshots. This matches r2-workshop's identity.rs. |
| D-3 | `device_sk` (the secret key) is in the FSM's `vars` but NEVER exposed via the public `identity` event. Public-event payload includes only `device_pk` + `rbid`. Defence in depth: even a buggy consumer can't accidentally leak the secret. |
| D-4 | Two internal self-send events (`identity_ready`, `generate_identity`) thread the `test` action's branch through the FSM. This is the canonical R2-DEF pattern for "conditional state transition" — R2-DEF doesn't have native guards on `to:`, so we use self-sends. |
| D-5 | Plugin reference to `nvs` is by name (`{name: nvs}`) even though `nvs` isn't scaffolded yet. The catalogue server will report `E_SENT_PLUGIN_UNRESOLVED` until `../../plugins/storage/nvs/` exists. Author NVS soon. |
| D-6 | The template token `{{platform.random_seed_32}}` is documented as an extension. R2-DEF §5.1 doesn't define platform calls in templates; r2-compiler claims this extension scope per `SPEC-R2-COMPILER` (to be amended). |

## Open items

- `nvs` plugin metadata — needs authoring next, otherwise this sentant's plugin reference is unresolved.
- Source extraction (Phase 1.4-source) — refactor r2-workshop's `identity.rs` to match this FSM mechanically.
- The `{{platform.random_seed_32}}` extension needs a normative declaration somewhere — likely in `SPEC-R2-COMPILER`'s code-gen section (Phase 1.7).

## Next session

Likely sequence:
1. Author `nvs` plugin metadata (resolves the dependency for this sentant).
2. Either: keep scaling Phase 1.4-metadata across remaining 11 plugins + 14 sentants (slow but thorough), OR
3. Pivot to building the minimal browseable webapp Roy asked about earlier (visual feedback on the catalogue), OR
4. Start Phase 1.4-source for lis2dh + Identity (proves the metadata can actually drive a build).
