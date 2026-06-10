# Part C(i) — Orchestrator r2-web host: design (for review before build)

Phase 3 Part C(i). composer's orchestrator gains the **r2-web host capability**:
read an ensemble's `registrations.r2-web` → serve its static bundle at a route
prefix → expose a `/r2` WebSocket that bridges R2-WIRE frames between a browser
**wasm-hive** and the mesh. Built against R2-WEB **v0.3** (read in-place,
`r2-specifications/specs/r2-core/R2-WEB.md`) + workshop's two-hive recipe
(`r2-workshop/docs/own-hive-web-ui-recipe.md`) + the `notekeeper.ensemble.yaml`
registration template.

**Governing principle (north-star):** composer **orchestrates hives, is not the
hive**. The orchestrator is the *controller/host* — it serves the bundle and
forwards frames; it hosts **no UX handlers**. All UX state lives in the **browser
wasm-hive** (a full R2 node via `crates/r2-wasm`). Two hives, one `/r2` channel.

## What exists today (the starting point)
`orchestrator/src/`: axum server; `/webapp` via `ServeDir` (hard-coded);
`/catalogue|/crates|/scores|/apiaries` static nests; a `/r2` WebSocket **stub**
that does JSON-envelope ↔ `QueuedEvent` translation (`bridge.rs`) into the
`r2-engine` bus (`hive.rs`). No registration model; the `/webapp` mount is
hard-wired; `/r2` carries JSON, not raw R2-WIRE frames; no Ed25519 auth.

## The capability (what to build)

### 1. Registration model — generalise the hard-coded mounts
Parse `registrations.r2-web` from an ensemble.yaml (serde), per the notekeeper
template + workshop's block:
```yaml
registrations:
  r2-web:
    route_prefix: /proof              # where the orchestrator mounts it
    static_bundle: ./web/             # the PWA bundle (built separately)
    channels:                         # raw R2-WIRE frame channels
      - { name: r2, target_sentant: TestCoordinator, max_frame_bytes: 65536 }
    subscriptions:                    # engine events streamed to browsers
      - { name: delivery, event: r2.tn.delivered }
    blob_routes: []                   # optional large-asset GET routes
    diagnostics: true
```
The orchestrator hosts a **set** of these (the proof-surface `test-ux` ensemble
is the first). The existing `/webapp` catalogue/stack UI can later be reframed as
composer's own r2-web registration; for now it stays as-is alongside.

### 2. Static serving (R2-WEB §3.1)
Per registration: `nest_service(route_prefix, ServeDir::new(static_bundle)
.append_index_html_on_directories(true))` with **SPA fallback** (serve
`index.html` for unmatched sub-paths). Fix the trailing-slash papercut found in
UX testing: redirect `route_prefix` → `route_prefix/`. LAN = plain HTTP ok
(R2-WEB §3.2: R2-TRUST authenticates regardless of transport).

### 3. The `/r2` channel — WS ↔ R2-WIRE frame bridge (the crux)
A WebSocket handler **distinct from the current JSON stub**: it carries **raw
R2-WIRE frames** (binary), per the workshop recipe. Per connection:
- **Auth (R2-WEB §4.2/§4.4):** first message MUST be a signed `authenticate`
  (`{device_id, timestamp, signature, payload}`); verify (a) `device_id`(=DEV_PK,
  64-hex) is a provisioned, non-revoked member of the active apiary TrustGroup
  (composer already has `substrate/tg_state` + `software-ed25519`), (b)
  `timestamp` within 60 s, (c) **Ed25519** sig over `device_id:timestamp:payload`
  against `device_id`. **NOT HMAC.** Drop bad messages silently; never close on
  one failure.
  - *v0.1 simplification (workshop's "skip TG pairing for a browser-only test
    UX"):* a `trusted_local` mode (loopback/Tailscale only) MAY accept an
    unsigned bootstrap, with Ed25519 as the gated upgrade. Design supports both;
    ship `trusted_local` first, wire full Ed25519 against the apiary TG next.
- **Frame flow:** browser→orchestrator frames are forwarded to the channel's
  `target_sentant` (e.g. `TestCoordinator`) on the engine bus **and/or** out to
  the mesh; engine/mesh events matching the registration `subscriptions` are
  encoded and pushed to subscribed browsers (R2-WEB §4.5 `event`).
- **Replay-cached-state-on-connect** (workshop pattern): post-auth, send a
  `state` snapshot (§4.5) so the wasm-hive hydrates without polling.
- Message types per R2-WEB §4.5 (out: event/state/error/pong; in:
  command/query/subscribe/unsubscribe/ping); heartbeat 30 s.

### 4. The wasm-hive bridge leg (Part C ii groundwork — seam noted)
The browser wasm-hive's **only transport is TCP/IP**: its frames ride the `/r2`
WS, and the orchestrator is its **gateway** — bridging WS frames ↔ the mesh over
core's **D3a host async TCP transport** (`r2-transport/tcp.rs` /
`r2-discovery` tcp). **Seam:** D3a's async transport shape isn't shared yet
(core is delivering it; it's Linux-verifiable soon). To avoid the guess-then-
rework that bit hive's transports seam, I build the r2-web host **against the
existing in-process engine bus now** (browser wasm-hive ↔ orchestrator engine
works locally, no DFR1195/mesh needed), and attach the **WS↔TCP mesh leg** to
D3a when its surface lands. Everything else (registration reader, static
serving, `/r2` frame channel, Ed25519 auth, subscriptions, replay) is buildable
+ Linux-testable **now**.

## Implementation shape
- New `orchestrator/src/web/` module: `Registration` (serde of the r2-web block),
  `RegistrationSet` (read at startup from the hosted ensemble.yaml(s)), router
  builder (static nest + `/r2` frame-channel handler per registration).
- Frame-channel WS handler: binary R2-WIRE, Ed25519-auth gate (+`trusted_local`),
  per-connection subscription set, replay-state-on-connect. Reuses the `hive.rs`
  engine bus for subscription; adds a raw-frame inject/forward path (alongside
  the existing JSON bridge, which the management webapp keeps using).
- Tests (Linux, no hardware): registration parse; static mount + SPA fallback +
  trailing-slash redirect; Ed25519 verify (good/expired/bad-sig/revoked); a
  frame round-trip (browser frame → target_sentant → subscription event → browser).

## Part C(ii) groundwork (queued, per supervisor)
Retire the toy `webapp/crate` wasm (class-hash only) in favour of
`crates/r2-wasm` as a **full R2 wasm-hive** (engine/trust/route), TCP-only
transport via the `/r2` WS↔TCP bridge above, hostable of other plugins
(`ProofViewerSentant`, web-server). r2-wasm `R2WorkshopHive`/`DashboardViewerSentant`
is the clone template (workshop) → `ProofHive`/`ProofViewerSentant`. Serverless
WASM-hive = R2-WEB §8.4; bootstrap+mesh-proxy hybrid = §8.5.

## Open items for review
1. OK to ship `trusted_local` auth first (loopback/Tailscale), Ed25519-vs-apiary-TG
   as the immediate follow-up? (workshop's browser-only-test-UX skip vs R2-WEB
   §10.2 "no anonymous access".)
2. Confirm the `/r2` frame channel is **raw R2-WIRE** (workshop recipe), keeping
   the existing JSON `/r2` bridge for the management webapp — i.e. two channels,
   or migrate the management UI too?
3. D3a async TCP transport surface from core — needed for the mesh leg; building
   host-against-engine-bus first per the seam note above. Confirm that's the
   right order.

---

## Addendum — specs conformance call 2 (2026-06-11)

Refines §3 (the `/r2` channel) per specs' R2-WEB v0.3 conformance ruling:

- **Two DISTINCT WS paths, never multiplexed on one socket:**
  - `/ws` — the **§4 JSON device channel**. Per-message **Ed25519** envelope
    auth (R2-WEB §4.2) = `verify_ws_auth` (slice 2a). For thin JS clients.
  - `/r2/wire` — the **raw-R2-WIRE node channel** (the browser wasm-hive). The
    frames carry their **own native R2-WIRE / R2-TRUST (TG-scoped) auth** — NOT
    the JSON Ed25519 envelope. This is the wasm-hive's primary path.
- **Both channels MUST authenticate** (no §10.2 bypass): `/ws` via §4.2
  Ed25519; `/r2/wire` via native frame auth.
- The **raw-R2-WIRE-over-WS channel is a current SPEC GAP** (§4's contract is
  JSON-only). Build it now **CONFORMANT-PENDING-SPEC**; specs will author
  **R2-WEB §4.6 "Raw R2-WIRE frame channel"** on Roy's go-ahead. Mark the code +
  registration accordingly.
- Target is `registrations.r2-web` (route_prefix/static_bundle/graphql/
  subscriptions) — **not** a bare `plugins[] plugin_type:web` entry
  (R2-ENSEMBLE §2.1.2). Already aligned.
- The canonical web-template YAML + the `csp` field are pending specs/core + Roy.

**Pre-existing gap to flag:** the orchestrator's current `/r2` JSON **management
bridge** (catalogue/stack webapp) does not authenticate. It's a distinct path
(satisfies condition 1) but for full §10.2 conformance it needs §4.2 Ed25519
too — tracked as a follow-up, separate from the proof-surface channels above.

**Net for slice 2b-ii:** build the `/r2/wire` raw-frame WS handler
(conformant-pending-spec, native frame auth) AND keep `verify_ws_auth` as the
`/ws` JSON channel gate. Distinct axum routes; distinct auth per channel.
