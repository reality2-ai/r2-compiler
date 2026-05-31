# testing/

Conformance vectors and round-trip tests for r2-composer.

## round-trip/

The v0.1 success gate (SPEC-R2-COMPOSER §6) — behavioural-equivalence vectors for the three r2-workshop carriers.

Each `<carrier>.expected.toml` file records:
- The R2-DEF §7 score that produced the firmware in r2-workshop.
- The expected `r2.sensor.announce` payload bytes (CBOR-hex).
- A representative R2-WIRE event sequence the firmware emits in a known scenario (e.g. boot → BLE adv → WiFi join → first accel sample → ack → next sample).
- The R2-CAP capability bloom advertised in the beacon.
- The plugin set the firmware should advertise as available.

To pass:
- For each carrier, build the firmware via r2-composer with the recorded score.
- Run the firmware against a recorded input sequence (likely a small Rust test harness using `r2-harness`).
- Confirm byte-equality on the announced payload and the R2-WIRE traffic.

Byte-identical binaries are NOT required (build timestamps, embedded git SHAs, etc. legitimately differ).

## Vector capture

Vectors are captured by running the existing r2-workshop firmware on real hardware (or in a simulator) and recording the output. The procedure will be documented under `testing/round-trip/CAPTURE.md` once the first vector is produced.

## Status (2026-05-31)

Empty. No vectors captured yet. Phase 1.4 in `plan/PLAN.md`.
