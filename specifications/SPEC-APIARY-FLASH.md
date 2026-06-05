# SPEC-APIARY-FLASH: device-flash workflow — USB first-install, WiFi provisioning, OTA, roster state

**Version:** 0.1 Draft
**Date:** 2026-06-01
**Status:** Normative Draft
**Depends on:**
- **r2-composer specs:** [`SPEC-R2-COMPOSER.md`](SPEC-R2-COMPOSER.md), [`SPEC-APIARY-LAYOUT.md`](SPEC-APIARY-LAYOUT.md) (§5 device roster), [`SPEC-APIARY-COMPOSE.md`](SPEC-APIARY-COMPOSE.md) (§3.1 deploy paths, §6.3 per-target artefacts), [`SPEC-APIARY-CREATE.md`](SPEC-APIARY-CREATE.md), [`SPEC-CATALOGUE-LAYOUT.md`](SPEC-CATALOGUE-LAYOUT.md)
- **Upstream R2:** R2-TRUST (Ed25519, DeviceCertificate), R2-WIRE, R2-BEACON, R2-CAP §3, RFC 2119 + RFC 8174
- **r2-workshop reference** (the gold-standard pattern this spec defers to where practical): `ota-tcp` plugin on port 21043, BLE-L2CAP CoC on PSM 0x00D2 for `#wifi_offer` bootstrap, `esptool` for USB flash.

## Conventions

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHALL**, **SHALL NOT**, **SHOULD**, **SHOULD NOT**, **RECOMMENDED**, **MAY**, and **OPTIONAL** in this document are to be interpreted as described in BCP 14 (RFC 2119, RFC 8174) when, and only when, they appear in all capitals.

---

## 1. Scope and terminology

### 1.1 What this spec covers

The end-to-end workflow for **getting compiled firmware onto a device** in an apiary, across the full device lifecycle:

1. **Slot creation** — declaring intent ("there will be a kitchen sensor on a xiao") before any hardware is touched.
2. **USB first-install** — flashing a fresh / virgin board over USB-Serial-JTAG.
3. **WiFi provisioning** — getting credentials onto the freshly-flashed device.
4. **Identity binding** — minting the device's TG `DeviceCertificate` once it announces its pubkey.
5. **OTA push** — subsequent firmware updates over WiFi.
6. **Decommission** — revoke / retire / purge.

The workflow is chat-driven per [[feedback-ai-chat-primary]]; the canvas surfaces device state ambiently per [[feedback-calm-computing]].

### 1.2 What this spec does NOT cover

| Concern | Where it's specified |
|---|---|
| Apiary creation + TG genesis | SPEC-APIARY-CREATE |
| Apiary directory layout + `apiary.toml` | SPEC-APIARY-LAYOUT §2, §3 |
| Compose tree + per-target build (the `.bin` files this spec flashes) | SPEC-APIARY-COMPOSE §6 |
| The `ota-tcp` plugin's device-side implementation | catalogue/.../plugins/comms/ota-tcp/ (verbatim from r2-workshop) |
| Per-roster-row schema basics | SPEC-APIARY-LAYOUT §5 (this spec extends it) |

### 1.3 Terminology

- **Slot** — an OPERATOR-DECLARED intent that "there will be a device performing role X on host Y in this apiary". Has a stable `slot_id` (UUIDv4) from the moment of declaration. Exists in the roster before any hardware is touched.
- **Device** — a PHYSICAL piece of hardware. Identified post-first-boot by its Ed25519 public key (its `device_pk`). Bound to a slot once `device.identity_observed` fires.
- **Target** (per SPEC-APIARY-COMPOSE §2.3) — a `(target_type, host, plugin_overrides)` tuple in the compose tree. ONE target may produce firmware for MANY slots — e.g. `target_id: "sensor:esp32-s3-xiao"` is the firmware shape, but the apiary may have THREE xiao slots (kitchen / lounge / basement) running it.
- **Virgin board** — an ESP-family board whose flash contains only the factory bootloader. Per [[project-compulsory-plugins-and-virgin-boards]], must be flashed by USB first before OTA is possible.

### 1.4 Three orthogonal state axes

A device row carries THREE independent status fields, not one:

- **`state`** — lifecycle position (`placeholder` → `built` → `flashed_pending_pk` → `enrolled` → `reachable` ⇄ `unreachable` / `revoked` / `retired`)
- **`provision_state`** — WiFi credentials health (`unknown` / `pending` / `valid` / `stale` / `failed`)
- **`cert_status`** — TG certificate status (`unknown` / `pending` / `valid` / `revoked` / `expired`)

A device **MAY** be `state: "reachable"` and `provision_state: "stale"` at the same time — it's responding but its credentials need rotation. Collapsing the three into one enum loses that information; this spec REQUIRES they remain separate.

## 2. Roster state machine

The orchestrator's `Roster` sentant owns the per-device state machine for the active apiary, persisted to `apiaries/<name>/devices/roster.toml` per SPEC-APIARY-LAYOUT §5.

### 2.1 States (lifecycle axis)

| State | Meaning |
|---|---|
| `placeholder` | Slot declared in chat; no hardware yet. Has `slot_id`, `role`, `host`, optional `name_alias`. No `device_pk`. |
| `built` | An artefact for the slot's `(role, host)` target exists in `out/`. Still no hardware. |
| `flashed_pending_pk` | USB first-install completed against this slot. The device has booted; the orchestrator is awaiting the first beacon carrying its `device_pk`. |
| `enrolled` | First beacon observed; `device_pk` recorded; `DeviceCertificate` minted + sent via `#wifi_offer`. Device on the WiFi network, TG-enrolled. |
| `reachable` | Device's last beacon observed within the freshness threshold (default 600s). |
| `unreachable` | Last beacon older than the threshold. Device might be asleep / off / network-partitioned. |
| `revoked` | Cert revoked. Roster row preserved; physical device cannot rejoin even if it tries. |
| `retired` | Soft-archive. Row preserved for audit; the operator removed the device physically but didn't revoke (e.g. moved to spare-parts box). |
| `PURGED` | Sentinel — row removed from roster.toml entirely after a confirmation ceremony. Not present in roster.toml at this state. |

### 2.2 Valid transitions (NORMATIVE)

The orchestrator **MUST** refuse any transition not listed here:

```
placeholder        → built | retired | PURGED
built              → flashed_pending_pk | retired | PURGED
flashed_pending_pk → enrolled | retired | PURGED        # never back to built — that needs reflash
enrolled           → reachable | unreachable | revoked | retired
reachable          → unreachable | revoked | retired
unreachable        → reachable | revoked | retired
revoked            → retired | PURGED
retired            → PURGED
```

Re-entry from any state to `flashed_pending_pk` is permitted ONLY via a fresh USB first-install (the operator is replacing the physical device or reflashing intentionally).

### 2.3 Atomic write protocol

Every roster mutation **MUST** be persisted atomically via write-temp-fsync-rename-fsync-dir:

1. Write the new `roster.toml` to `roster.toml.tmp`.
2. `fsync(roster.toml.tmp)`.
3. `rename("roster.toml.tmp", "roster.toml")`.
4. `fsync(parent_dir)`.

The orchestrator **MUST NOT** mutate roster.toml in place. Concurrent writes from two orchestrator processes against the same apiary repo are **OUT OF SCOPE for v0.1** (single-operator model); v0.2 will add `.roster.lock` file-locking.

### 2.4 History — append-only

Each device row **MUST** carry an append-only `history[]` array. Prior entries **MUST NEVER** be mutated. Each entry:

```toml
[[devices.history]]
ts        = "2026-06-01T14:23:11Z"
event     = "flashed_usb"
from      = "built"
to        = "flashed_pending_pk"
detail    = "artefact firmware-abc.bin (sha256 0x...) flashed via /dev/ttyACM0"
```

The transient `last_seen` field on each row **MUST NOT** generate history entries (would flood the file). It **SHOULD** be batched: flush at most every 30 seconds OR immediately on any structural state transition. A separate `[devices.last_seen_log]` table **MAY** be used for finer-grained beacon-observation logging if operationally needed.

### 2.5 Read-side: the single `device.transition` event

The webapp consumes ONE event per state change: `r2.composer.device.transition`. The orchestrator-internal causes (e.g. `device.flashed_usb`, `device.identity_observed`, `device.reachable`) are implementation detail (§8). The webapp **MUST** treat `device.transition` as authoritative and re-render the canvas's role-ensemble cards / transient strips from the new row state.

## 3. Identity binding

The slot ↔ physical-device binding is the trickiest piece of this workflow. This section is normative about how it MUST work.

### 3.1 `slot_id` as primary key

Each slot **MUST** carry a `slot_id` (UUIDv4) generated at slot declaration time. The `slot_id` is the primary key in `roster.toml` and **MUST** be unique within an apiary.

The `slot_id` **MUST** be baked into the firmware artefact at build time (SPEC-APIARY-COMPOSE §6 — extends with `slot_id` injection into a reserved 16-byte region of the image). This means per-slot artefacts: three xiao slots running the "same code" still produce three distinct `.bin` files. Artefact directory layout is `out/<role>-<host>-<slot8>-<ts>/firmware.bin` where `<slot8>` is the first 8 hex chars of `slot_id`. The disk cost is bounded (typically <2 MB per artefact); the trade-off favours deterministic identity over disk savings.

The slot's `slot_id` is what allows the orchestrator to recognise *which slot* a fresh beacon belongs to: the device's first beacon carries the baked-in `slot_id`, the orchestrator looks it up in the roster, and binds the just-announced `device_pk` to that slot row.

### 3.2 Pre-PK gap

A roster row is REQUIRED before the device has minted its `device_pk`. The `device_pk` field of the row is `null` from `placeholder` through `flashed_pending_pk`; it's first written when the orchestrator observes the device's first beacon in `enrolled` transition (§3.3).

### 3.3 First-beacon flow (NORMATIVE)

When a freshly-flashed device boots, it:
1. Generates an Ed25519 keypair on first boot (R2-TRUST device-identity contract).
2. Has no WiFi credentials yet (USB flash didn't write any).
3. Enters BLE-bootstrap mode: advertises the apiary's class hash + provisioning flag + its `device_pk` + its baked-in `slot_id` in the R2-BEACON 28-byte AD.
4. Listens for `#wifi_offer` over BLE-L2CAP CoC on PSM 0x00D2.

The orchestrator's `beacon-observer` plugin observes this beacon and emits `device.identity_observed{slot_id, device_pk, rbid}` internally. The `Roster` sentant transitions the matching row from `flashed_pending_pk` → `enrolled` AFTER:

1. Looking up the slot by `slot_id`.
2. Verifying the `slot_id` matches a row currently in `flashed_pending_pk`.
3. Asking the `keyholder` plugin to mint a `DeviceCertificate` binding the apiary's TG to the announced `device_pk`.
4. Asking the `provision` plugin to compose a `#wifi_offer` carrying the cert + SSID + PSK + cert validity-window.
5. Sending the `#wifi_offer` over the active BLE-L2CAP CoC session.
6. Awaiting the device's `#wifi_ack` (the device's signed acknowledgement that it persisted the creds + cert to NVS).
7. Writing the device row's `device_pk`, `cert_status: "valid"`, `provision_state: "valid"` and transitioning state to `enrolled`.

If any step fails, the row stays in `flashed_pending_pk` and the chat surfaces the failure. **NO** provisional / placeholder cert is issued during USB flash — earlier designs considered this and it's deliberately rejected: a cert is bytes-bound to the `device_pk`, which doesn't exist pre-boot.

### 3.4 Unaccounted devices

A beacon may arrive for a `slot_id` not in the roster (an old device's lingering firmware), or for an `slot_id` already-bound to a different `device_pk` (someone reflashed a device, generating a new keypair). Both cases are SECURITY EVENTS.

The orchestrator **MUST** emit `r2.composer.device.unaccounted{slot_id, device_pk, observed_at, kind: "unknown-slot"|"slot-pk-mismatch"}` and append to `apiaries/<name>/devices/unaccounted.toml` (an audit log). It **MUST NOT** auto-adopt; the operator's chat is the only path to either revoke-the-old / bind-the-new / ignore-this-beacon.

## 4. USB first-install

### 4.1 `[usb]` table on every MCU `board.toml` — REQUIRED

Per the multi-agent flash-workflow synthesis, every MCU carrier entry under `catalogue/boards/<slug>/board.toml` **MUST** carry a `[usb]` table per SPEC-CATALOGUE-LAYOUT §3.3 (extended here as §3.3.10):

```toml
[usb]
# Per-carrier USB identification + flash invocation parameters.
vid               = 0x303a            # Espressif native USB-Serial-JTAG; 0x10c4 for CP2102; 0x1a86 for CH9102/CH340
pid               = 0x1001            # Per-chip — ESP32-S3 native = 0x1001; ESP32-C6 native = 0x1001 (same!)
chip_id_probe     = "esp32c6"         # value `esptool --port <port> chip_id` returns; disambiguator when VID/PID is ambiguous across chip families
identify_strategy = "vid_pid_then_chip_id"   # "vid_pid_only" | "chip_id_only" | "vid_pid_then_chip_id" | "ask_operator"
esptool_chip_arg  = "esp32c6"         # --chip value passed to esptool
reset_strategy    = "usb_serial_jtag" # "usb_serial_jtag" | "cp2102_dtr_rts"
flash_offsets     = { bootloader = 0x0, partition_table = 0x8000, ota_0 = 0x20000 }   # ota_1 derived from partitions.csv
```

**Note** — VID 0x303a + PID 0x1001 is identical for both ESP32-S3 native USB-Serial-JTAG AND ESP32-C6 native USB-Serial-JTAG. VID/PID alone is INSUFFICIENT to disambiguate; the `chip_id_probe` field is the tiebreaker. The DevKitC carrier exposes TWO ttyACMs (one Espressif VID for the native USB-Serial-JTAG side, one Silabs VID for the on-board CP2102 UART bridge); the `usb-watcher` plugin **MUST** prefer the native USB-Serial-JTAG port for flashing.

### 4.2 Tool — esptool, never espflash

The orchestrator's `flasher` plugin **MUST** use `esptool` (Python, ESP-IDF-bundled). It **MUST NOT** use `espflash`. Per [[reference-carrier-firmware-pattern]] + every ESP-IDF carrier's `board.toml [notes].gotchas`: `espflash v3.x` writes a header byte that breaks ESP-IDF v5.3+ bootloaders. This isn't a stylistic choice; it's a correctness gate. The orchestrator **MUST** refuse to dispatch a USB flash if `esptool` is not on the PATH, and **MUST** report `E_FLASHER_TOOL_MISSING`.

### 4.3 Four-region write

The USB first-install **MUST** write four regions from the per-target artefact:

| Region | Source file in artefact dir | Default offset |
|---|---|---|
| Bootloader | `bootloader.bin` | `0x0` (per `[usb].flash_offsets.bootloader`) |
| Partition table | `partition-table.bin` | `0x8000` (per `[usb].flash_offsets.partition_table`) |
| OTA slot 0 | `firmware.bin` | `0x20000` (per `[usb].flash_offsets.ota_0`) |
| OTA slot 1 | `firmware.bin` (same image) | computed from `partitions.csv` |

The per-target build (SPEC-APIARY-COMPOSE §6.3) **MUST** produce `bootloader.bin` + `partition-table.bin` + `firmware.bin` for `mcu-fw` targets. Earlier compose-side spec versions only required `firmware.bin`; this spec amends §6.3 to require all three for MCU carriers.

### 4.4 Detection & identification flow

1. `usb-watcher` plugin observes a new serial port (Linux: udev; macOS: IOKit, deferred to v0.2; Windows: SetupAPI, deferred). Emits `r2.composer.usb.attached{port, vid, pid, sysfs_path}`.
2. Per the carrier's `[usb].identify_strategy`:
   - `vid_pid_only`: match VID+PID against `catalogue/boards/*/board.toml [usb]` tables; if exactly one match, identify; if zero or multiple, fall back.
   - `vid_pid_then_chip_id`: do VID/PID, then if ambiguous run `esptool chip_id` on the port to disambiguate.
   - `chip_id_only`: skip VID/PID entirely; always run `esptool chip_id`.
   - `ask_operator`: pop the chip into the canvas with `carrier_guess: null` and ask the AI to ask the operator.
3. Canvas surfaces the detection ambiently as a footer chip: *"Detected: dfr1117 on /dev/ttyACM0"*. **NO** modal.
4. The operator says (in chat) *"flash this as the kitchen sensor"*. The AI's tool-call channel fires `r2.composer.deploy.first_install.start{port, carrier, role, slot_id, artefact_path?}`.
5. If `artefact_path` is absent, the orchestrator looks for the latest `out/<role>-<host>-<slot8>-*/` — if none, the AI **SHOULD** dispatch `r2.composer.target.build.start` first (build-then-flash); per SPEC-APIARY-COMPOSE §6.2 the build sub-flow can also bake the `slot_id` at this moment.
6. esptool is invoked with the four-region write. Each region transitions through `erasing` → `writing_*` → `verifying` phases, streamed as `r2.composer.deploy.first_install.progress{slot_id, port, phase, bytes_sent?, bytes_total?, percent?}`.
7. On success: `r2.composer.deploy.first_install.done{slot_id, port, artefact_sha256, duration_ms}`. The `Roster` sentant transitions the slot from `built` → `flashed_pending_pk` and writes the audit record (§4.5).

### 4.5 `[devices.flash_history]` audit

Each USB flash event **MUST** append to a per-row `[devices.flash_history]` array (SEPARATE from `[devices.history]` so flash events don't get lost among reachability transitions):

```toml
[[devices.flash_history]]
flashed_at      = "2026-06-01T14:23:11Z"
port            = "/dev/ttyACM0"
mac_at_flash    = "aa:bb:cc:dd:ee:ff"           # efuse MAC; optional per carrier
artefact_path   = "out/sensor-esp32-c6-dfr1117-7c1f1234-20260601-142031/firmware.bin"
artefact_sha256 = "..."
esptool_version = "4.7.0"
duration_ms     = 23410
```

`mac_at_flash` is OPTIONAL per carrier — ESP carriers have a stable efuse MAC; rp2040 / native hosts don't. The roster schema **MUST** treat `mac_at_flash` as optional so future non-ESP carriers don't require schema migration.

## 5. WiFi provisioning

### 5.1 Primary path — BLE-bootstrap

The default + r2-workshop-compatible path. After §4 completes:

1. Device boots, generates Ed25519 keypair, enters BLE-bootstrap mode (§3.3).
2. Orchestrator's `provision` plugin reads the apiary's WiFi credentials from `~/.config/r2-composer/apiaries/<name>/wifi_networks.toml` (off-tree per §5.4).
3. Orchestrator composes the `#wifi_offer` frame, signed by the apiary's KeyHolder, carrying the SSID + PSK + `DeviceCertificate`.
4. Frame sent over BLE-L2CAP CoC PSM 0x00D2 to the device.
5. Device verifies the TG signature against its baked-in `tg_pub.bin`, persists creds + cert to NVS, reboots into STA mode.
6. Orchestrator awaits first-beacon-from-STA-side (§3.3).

The `#wifi_offer` frame format **MUST** be bytes-identical to r2-workshop's existing wire format so unmodified r2-workshop sensors keep working.

### 5.2 Alternate path — AP-mode (OPTIONAL)

For very-first-sensor scenarios where no enrolled controller is in range yet to relay BLE-bootstrap, the device **MAY** offer an AP-mode fallback. v0.1 design: the device boots in AP mode advertising an open SSID like `r2-bootstrap-<rbid8>`; the operator joins from their laptop / phone; an HTTP form on the device's `http://192.168.4.1/` accepts SSID + PSK + cert via a TG-signed envelope assembled by the orchestrator. Same `#wifi_offer` bytes, different transport.

AP-mode is **OPTIONAL per apiary** — the apiary's `apiary.toml` adds:

```toml
[provision]
ap_mode_enabled = false   # default — opt in only when BLE-bootstrap is impractical
```

### 5.3 `wifi_networks.toml` (off-tree, per-apiary)

WiFi credentials **MUST NEVER** appear in-tree. The off-tree path is `~/.config/r2-composer/apiaries/<name>/wifi_networks.toml`, mode `0600`, parent dir `0700`:

```toml
[[wifi_networks]]
name      = "UoA-Lab"
ssid      = "UoA-Lab"
psk       = "hunter2hunter2"
is_default = true                # OPTIONAL — used if the operator doesn't specify

[[wifi_networks]]
name      = "field-tablet-hotspot"
ssid      = "RoyHotspot-5G"
psk       = "..."
```

The operator manages this file via chat — `r2.composer.provision.network.upsert{name, ssid, psk, is_default?}`. The PSK **MUST** be redacted in logs and **MUST NOT** cross from orchestrator to webapp on the WS (the webapp doesn't need it).

### 5.4 Secrets discipline

The orchestrator **MUST**:
- Mode `wifi_networks.toml` to `0600` and its parent dir to `0700`.
- Refuse to read the file if permissions are looser.
- Refuse to emit `psk` over the `/r2` WS to the webapp under any event.
- Refuse to log `psk` at any tracing level.

These are conformance gates §9 codifies (`E_PROVISION_PERMS`, `E_PROVISION_LEAK`).

## 6. OTA push

### 6.1 Reachability gate

Only devices in `state: "reachable"` are **OTA-pushable**. The orchestrator **MUST** refuse to push to `unreachable` / `revoked` / `retired` rows. (Operators MAY use chat to override the reachability cache: *"force-push to basement, it should be on"* — the orchestrator attempts the push and surfaces the timeout calmly.)

### 6.2 Per-device flow

For each device the operator wants to push to:

1. Orchestrator opens TCP to `<device-ip>:21043` (the `ota-tcp` plugin's port, per r2-workshop).
2. Sends the wire-v1 request frame (§6.4 — **binary**, not text): a 1-byte command `CMD_START` (`0x01`), a 36-byte preamble (`size: u32 LE` + `sha256: 32 raw bytes`), then the raw `firmware.bin` bytes from `out/<role>-<host>-<slot8>-<ts>/` streamed to the socket. The orchestrator then **half-closes the write side** (TCP `FIN`) to signal end-of-firmware; the device reads the body until EOF.
3. Awaits the device's single binary response frame: `status: u8` + `msg_len: u16 LE` + `msg: utf-8`. `status == 0x00` is success (`msg` is literally `"OK"`); `status == 0x01` is failure (`msg` is a plain reason such as `"SHA-256 mismatch"` — no `ERR` prefix). The success SHA is **not** echoed back.
4. On success: the device verifies SHA-256, swaps the active OTA partition, logs locally, and `esp_restart()`s after ~2 s — there is **no** `REBOOTING` frame; the TCP connection simply drops as the device reboots.
5. Orchestrator waits up to 90 s for the device to come back and emit a beacon with the NEW firmware's `firmware_sha`.
6. On confirmation: roster row's `firmware_sha` + `firmware_ver` are updated; `state` stays `reachable`.
7. On timeout: orchestrator marks `provision_state: "stale"` (last-known-good beacon was the pre-push firmware) and surfaces the partial state to chat. The device may have rolled back — the next beacon from it will clarify.

> **Wire correction (2026-06-06).** This section originally described a text-line shape (`[u32 BE length]` frame, `OK <sha256>` / `ERR <reason>` / `REBOOTING` lines). That was an incorrect assumption — confirmed against R2-UPDATE §3.1.2.2 and r2-workshop's device source `crates/r2-esp/src/ota_tcp.rs` (which agree byte-for-byte): the real wire is binary-framed, the size is **little-endian**, the SHA-256 travels as **32 raw bytes** (not hex) in the request preamble, and EOF is signalled by a half-close, not a length prefix. The F5 `ota_push` substrate (`orchestrator/src/substrate/ota_push.rs`) implements the corrected wire above. Steps 5–7 (beacon-confirm of the new SHA) are deferred to F5b; F5 emits up to the `rebooting` progress phase.

### 6.3 Batching + sequencing

Batch pushes (operator says *"push the latest sensor build to all three sensors"*) **MUST** run sequentially by default — one device at a time. Parallel pushes are **OPTIONAL** and **MUST** be opted into per push (e.g. *"push in parallel"*). Sequential is the calm default because:
- Per-device progress is observable.
- A bricked push doesn't cascade across all devices simultaneously.
- The operator can interrupt mid-batch.

The `deploy.batch.*` events carry the batch identity; the `deploy.device.*` events fire one per device with `batch_id` for correlation.

### 6.4 Wire-v1 vs wire-v2

v0.1 of this spec uses r2-workshop's **wire-v1** OTA shape: an unsigned binary frame; the device trusts whoever connects to port 21043 (with the implicit assumption of WiFi-network trust). This **MUST** match r2-workshop bytes-for-bytes so unmodified r2-workshop sensors keep working.

Wire-v1 framing (R2-UPDATE §3.1.2.2 / r2-workshop `crates/r2-esp/src/ota_tcp.rs`):

```text
Request  (client → device):
  [0x01 CMD_START][size: u32 LE][sha256: 32 raw bytes][firmware bytes…]
  then half-close the write side (TCP FIN); the device reads the body until EOF.

Response (device → client):
  [status: u8][msg_len: u16 LE][msg: utf-8]
  status 0x00 = OK    (msg == "OK"; the SHA is not echoed)
  status 0x01 = error (msg is a plain reason, e.g. "SHA-256 mismatch")
```

The pusher does **not** send a `CMD_QUERY` (`0x02`) version probe before the push — it opens the socket and sends `CMD_START` directly. (`CMD_QUERY` exists on the device as a separate one-byte probe returning a JSON build-info frame, used by other tooling, not the push path.)

**Wire-v2** (OPTIONAL, capability-gated) wraps the v1 frame in a TG-signed envelope (`OtaPushAuthorization{artefact_sha256, target_device_pk, ttl}`) so an attacker on the WiFi network can't push arbitrary firmware. v2 is deferred to a future spec amendment; v0.1 ships v1-compatible and the device's `ota-tcp` plugin advertises its supported version via a CMD_QUERY probe.

### 6.5 `deploy_log.jsonl`

Each push attempt appends a JSON-line to `apiaries/<name>/devices/deploy_log.jsonl` for audit. Format:

```json
{"ts":"2026-06-01T14:30:15Z","batch_id":"b1f2c3","slot_id":"7c1f1234-...","attempt":1,"phase":"done","artefact_sha256":"...","duration_ms":24130}
```

## 7. Decommission

### 7.1 `revoke`

`r2.composer.device.revoke{slot_id, reason}` — KeyHolder publishes the device's cert in the apiary's revocation list. The orchestrator updates the row to `state: "revoked"`, `cert_status: "revoked"`. The physical device, even if powered on, **MUST** be rejected by other apiary hives once they observe the revocation. The roster row is **PRESERVED** for audit.

### 7.2 `retire`

`r2.composer.device.retire{slot_id, reason}` — the operator removed the device physically but does not consider it compromised. Cert is NOT revoked. Row transitions to `state: "retired"`; preserved in roster.toml.

### 7.3 `purge`

`r2.composer.device.purge{slot_id}` — hard delete the row from roster.toml. **REQUIRES** a confirmation phrase per SPEC-APIARY-CREATE §5.1 — `purge-device-<slot_id8>-i-understand-history-is-lost`. After purge, the slot is no longer reachable for compile, push, or audit; the operator MUST author a fresh slot (with a new `slot_id`) if they want to re-deploy at this position.

### 7.4 Active-member count

When apiary `[role_ensembles]` declare `device_count_planned` (SPEC-APIARY-LAYOUT §3), the count compares against ACTIVE states only: `placeholder` + `built` + `flashed_pending_pk` + `enrolled` + `reachable` + `unreachable`. `revoked` and `retired` rows are **NOT** counted. `PURGED` is gone entirely.

## 8. Event vocabulary (extends SPEC-R2-COMPOSER §4)

### 8.1 External (orchestrator ↔ webapp)

| Event | Direction | Payload | Purpose |
|---|---|---|---|
| `r2.composer.device.slot.create` | webapp → orchestrator (AI tool-call) | `{role, host, name_alias?}` | Operator-declared slot intent. Returns `device.transition` to `placeholder`. |
| `r2.composer.device.list` | webapp → orchestrator | `{}` | Webapp requests full roster snapshot on connect. |
| `r2.composer.device.entry` | orchestrator → webapp | full row | Streamed per row in response to `device.list`. |
| `r2.composer.device.transition` | orchestrator → webapp | `{slot_id, from, to, detail}` | Single read-side state-change signal. |
| `r2.composer.device.unaccounted` | orchestrator → webapp | `{slot_id, device_pk, observed_at, kind}` | A beacon matched no known slot OR mismatched a known slot's pk. |
| `r2.composer.usb.attached` | orchestrator → webapp | `{port, vid, pid, sysfs_path, carrier_guess?, guess_confidence, mac_seen_before, attached_at}` | New USB device on the bus. Calm ambient surface. |
| `r2.composer.usb.detached` | orchestrator → webapp | `{port, sysfs_path, detached_at}` | USB device removed. |
| `r2.composer.usb.list` | webapp → orchestrator | `{}` | Snapshot of currently-attached USB devices. |
| `r2.composer.usb.identify` | webapp → orchestrator | `{port}` | Operator (or AI) requests re-probe via `esptool chip_id`. |
| `r2.composer.deploy.first_install.start` | webapp → orchestrator (AI tool-call) | `{port, carrier, role, slot_id, artefact_path?}` | Operator-confirmed USB flash kickoff. |
| `r2.composer.deploy.first_install.progress` | orchestrator → webapp | `{slot_id, port, phase, bytes_sent?, bytes_total?, percent?}` | Streamed per-region progress. |
| `r2.composer.deploy.first_install.done` | orchestrator → webapp | `{slot_id, port, artefact_sha256, duration_ms}` | USB flash complete. |
| `r2.composer.deploy.first_install.error` | orchestrator → webapp | `{slot_id, port, phase, message, esptool_stderr_tail?}` | USB flash failed. **Only** moment in this workflow where calm-loud red is permitted. |
| `r2.composer.provision.network.upsert` | webapp → orchestrator (AI tool-call) | `{name, ssid, psk, is_default?}` | Operator added/updated a WiFi network in off-tree `wifi_networks.toml`. |
| `r2.composer.provision.offer.start` | orchestrator → webapp | `{slot_id, network_name}` | Orchestrator is sending `#wifi_offer` over BLE-L2CAP. |
| `r2.composer.provision.offer.progress` | orchestrator → webapp | `{slot_id, phase}` | `signing` / `sending` / `ack-pending` / `done`. |
| `r2.composer.deploy.batch.start` | webapp → orchestrator (AI tool-call) | `{target_id, slot_ids[], parallel?: bool}` | Push a specific artefact to a list of slots. |
| `r2.composer.deploy.device.progress` | orchestrator → webapp | `{batch_id, slot_id, phase, bytes_sent?, percent?}` | Per-device push progress. |
| `r2.composer.deploy.device.done` | orchestrator → webapp | `{batch_id, slot_id, artefact_sha256, duration_ms}` | One device done. |
| `r2.composer.deploy.device.error` | orchestrator → webapp | `{batch_id, slot_id, error_kind, message}` | One device failed (`unreachable`/`sha-mismatch`/`reboot-timeout`/...). |
| `r2.composer.deploy.batch.done` | orchestrator → webapp | `{batch_id, ok_count, error_count}` | All slots in the batch resolved. |
| `r2.composer.device.revoke` | webapp → orchestrator (AI tool-call) | `{slot_id, reason}` | Revoke cert + transition to `revoked`. |
| `r2.composer.device.retire` | webapp → orchestrator (AI tool-call) | `{slot_id, reason}` | Soft-archive. |
| `r2.composer.device.purge` | webapp → orchestrator (AI tool-call) | `{slot_id, confirmation_phrase}` | Hard-delete, requires §7.3 phrase match. |

### 8.2 Internal (orchestrator-only — sentant↔plugin)

These are NOT exposed on `/r2`. The webapp sees only the §8.1 events.

| Event | Direction | Purpose |
|---|---|---|
| `device.built` | internal | An artefact for a slot's `(role, host)` target was produced; the slot can transition `placeholder` → `built`. |
| `device.flashed_usb` | internal | USB flash completed; slot transitions `built` → `flashed_pending_pk`. |
| `device.flashed_ota` | internal | OTA push completed; slot's `firmware_sha` updates. |
| `device.identity_observed` | internal | First beacon from a `flashed_pending_pk` slot — triggers cert minting. |
| `device.reachable` | internal | Beacon observed within freshness threshold. |
| `device.unreachable` | internal | No beacon observed in last 600s. |

## 9. Conformance

### 9.1 Calm-computing posture

- The flash workflow **MUST NOT** present modal dialogs.
- USB-attach / -detach **MUST** appear as ambient canvas footer chips, not toast notifications.
- USB-flash error **MAY** display a calm-loud red strip on the failed slot card — the **only** calm-loud escalation point in the whole flash workflow.
- Per-device OTA progress **MUST** appear as inline row-update chips, not popovers.

### 9.2 AI-chat-primary

All state-changing operations in this workflow **MUST** be triggered via AI-emitted events through the chat. The webapp **MAY** offer click-shortcuts (e.g. clicking an attached USB chip seeds the chat with *"flash this as ___"*) but **MUST NOT** synthesise `first_install.start` / `deploy.batch.start` / `device.revoke` payloads directly.

### 9.3 r2-workshop compatibility

- `#wifi_offer` bytes-identical to r2-workshop (§5.1).
- `ota-tcp` wire-v1 bytes-identical to r2-workshop (§6.4).
- r2-workshop sensors flashed via this workflow **MUST** boot and operate without firmware changes.

### 9.4 Tool gating

- `esptool` **MUST** be present on PATH; `espflash` **MUST NOT** be invoked (§4.2).

## 10. Conformance gates (error codes)

| Code | Meaning |
|---|---|
| `E_FLASHER_TOOL_MISSING` | `esptool` not on PATH. |
| `E_FLASHER_TOOL_BANNED` | The flasher was asked to invoke `espflash`; refused per §4.2. |
| `E_USB_IDENTIFY_AMBIGUOUS` | VID/PID alone matched multiple carriers AND `chip_id_probe` failed or returned an unknown value. |
| `E_USB_IDENTIFY_UNKNOWN` | No carrier in `catalogue/boards/*/board.toml [usb]` matches the observed device. |
| `E_FLASH_REGION_MISSING` | Required artefact file (`bootloader.bin`, `partition-table.bin`, `firmware.bin`) missing from `out/.../`. |
| `E_FLASH_REGION_WRITE_FAILED` | esptool returned non-zero during a region write. |
| `E_FLASH_VERIFY_FAILED` | esptool's read-back verification failed. |
| `E_PROVISION_PERMS` | Off-tree `wifi_networks.toml` not mode `0600` (or parent dir not `0700`). |
| `E_PROVISION_LEAK` | A `psk` was about to traverse a forbidden channel (`/r2` WS, tracing log). Refused. |
| `E_PROVISION_TG_SIGN_FAILED` | KeyHolder couldn't sign the `#wifi_offer`. |
| `E_PROVISION_DEVICE_ACK_TIMEOUT` | Device didn't `#wifi_ack` within the timeout. |
| `E_OTA_UNREACHABLE` | Device's state is not `reachable`; push refused. |
| `E_OTA_SHA_MISMATCH` | Device reported a different SHA-256 than the orchestrator computed. |
| `E_OTA_REBOOT_TIMEOUT` | Device closed the TCP connection but no beacon in `90s`. Possible rollback. |
| `E_OTA_WIRE_VERSION_MISMATCH` | Device's `ota-tcp` reports wire-v2-only; orchestrator is wire-v1. Or vice versa. |
| `E_ROSTER_TRANSITION_REFUSED` | A state transition not in §2.2 was attempted. |
| `E_SLOT_NOT_FOUND` | Operation referenced a `slot_id` not in the roster. |
| `E_SLOT_ID_COLLISION` | A new `device.slot.create` proposed a slot_id already present (UUID collision — vanishingly rare; restart create). |
| `E_DESTRUCTIVE_CONFIRM_MISMATCH` | Operator's purge phrase did not match §7.3. (Shares code with SPEC-APIARY-CREATE §5.) |
| `E_BEACON_OBSERVER_UNAVAILABLE` | The `beacon-observer` plugin can't access BLE (missing BlueZ, no adapter, permissions). |

## 11. Plugins + sentants this spec introduces (catalogue-shaped)

Per [[project-r2-composer-self-as-ensemble]], r2-composer's own machinery is structurally an R2 ensemble. The components this workflow needs are R2-PLUGIN §12 / R2-DEF §2 conformant catalogue entries that **MUST** live under `meta/ensembles/r2-composer-orchestrator/` (scaffolded Phase 1.6+):

| Component | Kind | Class | Role |
|---|---|---|---|
| `Roster` | sentant | `ai.reality2.composer.sentant.roster` | Per-device FSM state machine; owns roster.toml lifecycle; validates §2.2 transitions. |
| `Deploy` | sentant | `ai.reality2.composer.sentant.deploy` | FSM router: `deploy.*` events → matching plugin (`flasher` or `ota_push`). |
| `usb-watcher` | plugin | `ai.reality2.composer.plugin.usb-watcher` | OS-specific USB attach/detach detection; carrier identification per `[usb]` table. |
| `flasher` | plugin | `ai.reality2.composer.plugin.flasher` | Wraps `esptool`. Refuses `espflash`. Four-region write. |
| `provision` | plugin | `ai.reality2.composer.plugin.provision` | Composes + sends `#wifi_offer` over BLE-L2CAP. Manages off-tree `wifi_networks.toml`. |
| `beacon-observer` | plugin | `ai.reality2.composer.plugin.beacon-observer` | Observes R2-BEACON advertisements; emits `device.identity_observed` / `device.reachable` / `device.unreachable`. Relates to task #31 BLE-scan. |
| `ota_push` | plugin | `ai.reality2.composer.plugin.ota-push` | Wire-v1 (+ optional wire-v2) push over TCP/21043. |
| (`keyholder` — already in SPEC-APIARY-CREATE) | plugin | `ai.reality2.composer.plugin.keyholder` | Mints `DeviceCertificate` for the §3.3 enrolment step. |

Each component **MUST** conform to SPEC-CATALOGUE-LAYOUT §5 (plugins) or §6 (sentants) when its `meta/` entry is authored. v0.1 implementations live inline in `orchestrator/src/{plugins,sentants}/` per current code; promotion to catalogue-shaped meta-entries is a Phase 1.6+ deliverable.

## 12. Forward path

| Concern | When |
|---|---|
| macOS USB detection (IOKit) | Phase 2 |
| Windows USB detection (SetupAPI) | Phase 2 |
| Wire-v2 signed OTA envelope | Phase 2 |
| Multi-operator roster collaboration (`.roster.lock`, `git merge` strategy) | Phase 2 |
| Non-ESP carrier first-install (rp2040 via picotool; native hosts via systemd unit install) | Phase 2 |
| Re-provisioning a device on a different WiFi network without USB reflash | Phase 2 |
| Beacon-observer integrating with task #31's BLE-scan discovery | Phase 2 |
| Per-target stack visualisation drill-in from a slot card | Phase 2-canvas-c |
| Transient view (live `reachable` indicators) — depends on beacon-observer + apiary-TG-membership per SPEC-R2-COMPOSER §13 | Phase 3 |

## 13. Change log

| Date | Version | Change |
|---|---|---|
| 2026-06-01 | 0.1 | Initial draft. Authored by synthesising a multi-agent flash-workflow exploration (USB-first / OTA / WiFi / roster slices). Codifies the three-orthogonal-axes status model, slot_id as primary key with the per-slot artefact baking, the calm-computing posture for ambient USB events, the destructive-confirmation phrase registry shared with SPEC-APIARY-CREATE §5, and the catalogue-shaped plugin+sentant inventory per [[project-r2-composer-self-as-ensemble]]. r2-workshop wire compatibility is a hard gate (§9.3). |
