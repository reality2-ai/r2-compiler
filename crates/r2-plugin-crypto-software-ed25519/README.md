# crypto/software-ed25519

Dual-mode R2 plugin providing Ed25519 signing, verification, and deterministic
keypair derivation in pure software.

Conforms to R2-PLUGIN §12 (dual-mode authoring). See
`r2-specifications/plugins/software-ed25519/PLUGIN.md` for the formal
interface contract.

## 1. Purpose

Signs messages and verifies Ed25519 signatures without any hardware security
element. Useful as the default signing path on hives that do not ship with a
secure element, and as a reference implementation for validating hardware-backed
plugins (the byte-level interface is identical to `crypto/atecc608` et al.).

Keys are **not persisted** by this plugin. A caller (sentant, storage plugin,
or R2-TRUST) supplies the 32-byte secret key on every sign call. For
persistent key custody use this plugin behind `storage/encrypted-vault` or
substitute a secure-element plugin in production.

## 2. Modes & Platforms

| Mode | Targets | Notes |
|------|---------|-------|
| `aot` | `esp32-s3`, `esp32-c6`, `cortex-m4`, `linux-embedded`, `server` | `no_std`, statically linked by r2-forge. |
| `nif` | `linux-embedded`, `server` | `std`, loaded by BEAM hive via `r2-nif`. |

The `aot` and `nif` Cargo features are mutually exclusive.

## 3. Events Handled (Inbound)

| Event class | Command byte | Input bytes | Purpose |
|-------------|--------------|-------------|---------|
| `r2.crypto.ed25519.sign` | `0x01` | `[32 B secret-key seed][N B message]` | Sign a message with the supplied secret. |
| `r2.crypto.ed25519.verify` | `0x02` | `[32 B public-key][64 B signature][N B message]` | Verify a signature against a public key and message. |
| `r2.crypto.ed25519.generate` | `0x03` | `[32 B seed]` | Derive a keypair deterministically from a 32-byte seed. |

Data is fixed-offset, not CBOR, to minimise MCU overhead.

## 4. Events Emitted (Outbound)

The plugin emits its result under the event name `crypto/software-ed25519`
(per R2-PLUGIN §2.4 default naming). The hive wraps the raw response bytes
returned by `execute` into the standard result envelope:

### Success

```json
{
  "plugin": "crypto/software-ed25519",
  "command": "sign",
  "status": "ok",
  "data": { "signature": "<64 bytes, base64url>" }
}
```

| Command | `data` fields |
|---------|---------------|
| `sign` | `signature` (64 B) |
| `verify` | `valid` (boolean; byte `0x00` → `true`, `0x01` → `false`) |
| `generate` | `public_key` (32 B), `secret_key` (32 B) |

### Error

```json
{
  "plugin": "crypto/software-ed25519",
  "command": "verify",
  "status": "error",
  "error": "verify: bad public key"
}
```

| Error code | Name | Meaning |
|------------|------|---------|
| `0x01` | `bad_length` | Input byte length did not match the command's required layout. |
| `0x03` | `bad_key` | Public key failed to parse (not a valid curve point). |
| `0xFE` | `unknown_command` | Command byte was not `sign`, `verify`, or `generate`. |

## 5. Configuration

None. The plugin is stateless and takes all inputs per invocation.

## 6. Example Sentants

A minimal sentant that signs any `message.requested` event and emits the
signature back on `message.signed`:

```yaml
sentant:
  name: "Sign-on-request"
  class: "org.example.signer"
  automations:
    - name: "default"
      states:
        - name: "idle"
          transitions:
            - event: "message.requested"
              to: "idle"
              actions:
                - plugin: "crypto/software-ed25519"
                  command: "sign"
                  parameters:
                    secret_key: "{{vars.signer_sk}}"
                    message: "{{event.payload.text}}"
            - event: "crypto/software-ed25519"
              to: "idle"
              actions:
                - emit: "message.signed"
                  payload:
                    signature: "{{event.data.signature}}"
```

A companion sentant that verifies incoming `message.claimed` events:

```yaml
sentant:
  name: "Verifier"
  class: "org.example.verifier"
  automations:
    - name: "default"
      states:
        - name: "idle"
          transitions:
            - event: "message.claimed"
              to: "idle"
              actions:
                - plugin: "crypto/software-ed25519"
                  command: "verify"
                  parameters:
                    public_key: "{{event.payload.pk}}"
                    signature: "{{event.payload.sig}}"
                    message: "{{event.payload.text}}"
            - event: "crypto/software-ed25519"
              to: "idle"
              guard: "event.data.valid == true"
              actions:
                - emit: "message.verified"
```

## 7. Hardware/Host Requirements

None. Pure-software implementation.

- **MCU (aot):** ~4 KB code, no RAM state beyond a transient `PluginResponse` buffer. Verification is ~3 ms on ESP32-S3 @ 240 MHz; signing is ~5 ms.
- **Host (nif):** any Linux/BSD/macOS target supported by the BEAM.

## 8. Credentials

None. Secret-key material is passed on each invocation — this plugin does
not read from the R2-TRUST credential store. If you need persistent key
custody, layer this plugin behind `storage/encrypted-vault` or
`crypto/atecc608`.

## 9. Known Limitations

- **No key persistence.** Keys live only as long as the invoking event; the plugin itself is stateless.
- **No RNG.** `generate` requires the caller to supply a 32-byte seed. This is intentional (the plugin is `no_std` and portable), but it means randomness is the caller's responsibility. For on-device entropy, pair with a platform RNG plugin.
- **No hardware protection.** Secret keys touch Rust stack memory during sign/generate. A compromised firmware can exfiltrate keys; production deployments handling trust-group-critical keys SHOULD use a secure-element plugin (`crypto/atecc608`, `crypto/se050`) in place of this one.
- **Fixed response buffer.** Responses are bounded to 128 bytes by the `PluginResponse` type; all three commands stay well under this limit.
- **No batch operations.** One invocation = one sign/verify/generate. Batch APIs may be added in a future version.

## 10. Changelog

### 0.1.0 — 2026-04-17

- Initial implementation.
- Commands: `sign` (0x01), `verify` (0x02), `generate` (0x03).
- Error codes: `bad_length` (0x01), `bad_key` (0x03), `unknown_command` (0xFE).
- `no_std`-compatible; `aot` and `nif` Cargo features.
- Nine integration tests covering round-trip, determinism, metadata, and error paths.
