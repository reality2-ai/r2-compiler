# OTA reply-status contract (composer push ↔ hive receiver)

Coordination artifact for Phase 3 Part D2. composer owns the OTA **push** side
(F5 `ota_push`); hive owns the DFR1195 **receiver** (Path B, no_std embassy-net).
This pins the reply contract so both sides agree. **Folds into
`SPEC-APIARY-FLASH §6` at branch-merge** (kept standalone for now to avoid
cross-branch churn).

## Wire (UNCHANGED — R2-UPDATE §3.1.2.2 / r2-workshop `ota_tcp.rs`)

Request (composer → device), already shipped in F5:
```
[0x01 CMD_START][size: u32 LE][sha256: 32 raw bytes][firmware bytes…]  then half-close (FIN)
```

Reply (device → composer):
```
[status: u8][msg_len: u16 LE][msg: UTF-8]
```

## status byte

- **`0x00` SUCCESS** — `msg = "OK"`. Send **only after** all of: SHA-256 over
  exactly `size` bytes matches the preamble SHA; the image is written to the
  inactive OTA slot; set-boot-partition succeeded. Then reply, then reboot (~2 s).
- **`0x01` ERROR** — `msg = "<CODE>[ detail]"`, `CODE` = one uppercase token:

| CODE | meaning |
|------|---------|
| `PREAMBLE`     | couldn't read/parse the 37-byte header |
| `TOO_BIG`      | `size` exceeds the inactive-slot capacity — **bound-check BEFORE writing** |
| `BAD_MAGIC`    | streamed image isn't a valid ESP app image (0xE9 / `esp_image` header check) |
| `SHA_MISMATCH` | computed SHA-256 ≠ preamble SHA |
| `WRITE_FAIL`   | `esp_ota_write` / flash error (append errno/detail) |
| `NO_SLOT`      | no inactive OTA partition available |
| `SHORT`        | FIN/close before `size` bytes received |

Only `0x00`/`0x01` are used (R2-UPDATE defines just `RESP_OK`/`RESP_ERR`); the
`CODE` rides in `msg` so the wire stays byte-exact and composer's push side
classifies by the leading token. If hive+core prefer **distinct status bytes**,
that's a wire change → composer takes it to specs as an R2-UPDATE amendment.

## Completion + bounds

- **Completion:** key on the `size` count from the preamble for "fully received";
  the half-close/FIN merely confirms done. Verify SHA over exactly `size` bytes.
- **Bound-check (`TOO_BIG`):** **DFR1195 = ESP32-S3-WROOM-1-N4 = 4 MB flash, NOT
  8 MB.** Its two OTA slots are ≈1.5 MB each at most, so `TOO_BIG` must fire well
  below 8 MB. (The esp32-s3-devkitc is 8 MB — do not carry that over.)

## composer side (follow-up)

`orchestrator/src/substrate/ota_push.rs` currently treats any non-zero status as
an error and special-cases `"SHA-256 mismatch"`. Follow-up: align its
`error_kind` classification to parse the leading `CODE` token from this table
(small change on the F5/main line).
