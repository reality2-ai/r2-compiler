#!/usr/bin/env python3
"""F3 end-to-end test: identity_observed → cert_issued → offer.composed
                    → device.enrolled → device.transition.

Drives the orchestrator's full F3 chain via /r2 WebSocket against a
real (already-running) rocker-rig apiary. Run while
`./orchestrator/run.sh debug` is up on :21050.

Steps:
  1. Connect, open apiary, list devices.
  2. Pick a slot that's `flashed_pending_pk` (or seed one synthetically
     by manipulating roster.toml — out of scope here; expects the
     operator has already flashed a board).
  3. Send `provision.network.upsert` to ensure ≥1 WiFi network exists.
  4. Send `device.identity_observed` with a fake 32-byte pubkey.
  5. Listen for the chain to complete; verify each step's event arrives.

The test makes NO assumption about the real apiary's TG keypair —
loads whatever's on disk. The fake device_pk is deterministic so the
cert file lands at a predictable path.
"""
import asyncio
import json
import sys
import time
import websockets

ENDPOINT = "ws://localhost:21050/r2"
TIMEOUT = 10

# A deterministic 32-byte pubkey — `aa` repeated.
FAKE_DEVICE_PK = "aa" * 32
TEST_NETWORK = {
    "name": "f3-test-network",
    "ssid": "test-ssid",
    "psk": "test-psk-32chars-of-fake-credentialz",
    "is_default": True,
}


async def send(ws, name, payload=None):
    msg = {"kind": "event", "name": name}
    if payload is not None:
        msg["payload"] = json.dumps(payload)
    await ws.send(json.dumps(msg))


async def wait_for(ws, name, predicate=None, timeout=TIMEOUT):
    """Read events; return the first matching one. Predicate is
    (payload_dict) -> bool. Raises on timeout."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        remain = deadline - time.monotonic()
        if remain <= 0:
            break
        try:
            msg = await asyncio.wait_for(ws.recv(), timeout=remain)
        except asyncio.TimeoutError:
            break
        try:
            evt = json.loads(msg)
        except Exception:
            continue
        if evt.get("name") != name:
            continue
        try:
            payload = json.loads(evt.get("payload", "{}"))
        except Exception:
            payload = {}
        if predicate is None or predicate(payload):
            return payload
    raise TimeoutError(f"timed out waiting for {name!r}")


async def main():
    print(f"connecting to {ENDPOINT}")
    async with websockets.connect(ENDPOINT) as ws:
        # 1. Open rocker-rig apiary.
        print("→ apiary.open rocker-rig")
        await send(ws, "r2.composer.apiary.open", {"name": "rocker-rig"})
        try:
            active = await wait_for(ws, "r2.composer.apiary.active")
            print(f"  ✓ active: {active.get('name') or active}")
        except TimeoutError as e:
            print(f"  ⚠ {e} — continuing anyway (apiary may already be open)")

        # 2. List devices; find an in-flight slot.
        print("→ device.list")
        await send(ws, "r2.composer.device.list", {})
        # Drain a few entries.
        entries = []
        deadline = time.monotonic() + 2
        while time.monotonic() < deadline:
            remain = max(0.05, deadline - time.monotonic())
            try:
                msg = await asyncio.wait_for(ws.recv(), timeout=remain)
            except asyncio.TimeoutError:
                break
            evt = json.loads(msg)
            if evt.get("name") == "r2.composer.device.entry":
                try:
                    entries.append(json.loads(evt["payload"]))
                except Exception:
                    pass

        slot_id = None
        for row in entries:
            if row.get("state") == "flashed_pending_pk":
                slot_id = row["slot_id"]
                print(f"  ✓ found flashed_pending_pk slot: {slot_id} ({row.get('name_alias','?')})")
                break

        if not slot_id:
            # No real flashed slot — seed a synthetic one by sending
            # slot.create + manually walking it to flashed_pending_pk
            # via deploy.first_install.done from a fake plugin source.
            # Cleanest path: create a placeholder, then this test does
            # NOT actually drive cert minting (the slot must be in
            # flashed_pending_pk first).
            print("  ✗ no slot in flashed_pending_pk; would need a flashed board.")
            print("    Aborting — bring a board onto the canvas + flash it first.")
            return 2

        # 3. Ensure at least one WiFi network is stored.
        print(f"→ provision.network.upsert {TEST_NETWORK['name']}")
        await send(ws, "r2.composer.provision.network.upsert", TEST_NETWORK)
        try:
            upserted = await wait_for(ws, "r2.composer.provision.network.upserted")
            print(f"  ✓ upserted: {upserted.get('name')}")
        except TimeoutError as e:
            print(f"  ⚠ {e}")

        # 4. Fire identity_observed.
        print(f"→ device.identity_observed slot={slot_id} device_pk={FAKE_DEVICE_PK[:8]}…")
        await send(ws, "r2.composer.device.identity_observed",
                   {"slot_id": slot_id, "device_pk": FAKE_DEVICE_PK})

        # 5. Watch the chain.
        chain = [
            ("r2.composer.provision.offer.start",     "cert_request progress"),
            ("r2.composer.provision.cert_issued",     "DeviceCertificate minted"),
            ("r2.composer.provision.offer.composed",  "#wifi_offer composed"),
            ("r2.composer.device.enrolled",           "device.enrolled emitted"),
            ("r2.composer.device.transition",         "Roster transitioned"),
        ]
        ok = True
        for name, label in chain:
            try:
                pred = (lambda p: p.get("slot_id") == slot_id) if name != "r2.composer.provision.offer.start" else None
                if name == "r2.composer.device.transition":
                    pred = (lambda p: p.get("slot_id") == slot_id and p.get("to") == "enrolled")
                evt = await wait_for(ws, name, pred, timeout=5)
                print(f"  ✓ {name} — {label}")
                if name == "r2.composer.provision.cert_issued":
                    print(f"      tg_fp: {evt.get('tg_fp','?')[:16]}…")
                if name == "r2.composer.device.enrolled":
                    print(f"      cert_hex: {evt.get('cert_hex','?')[:16]}…")
                    print(f"      offer_hex: {evt.get('offer_hex','?')[:16]}…")
            except TimeoutError as e:
                print(f"  ✗ {e}")
                ok = False
                break

        print()
        print("✓ F3 chain complete" if ok else "✗ F3 chain incomplete")
        return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
