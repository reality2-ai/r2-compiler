//! `provision_handshake` substrate component — orchestrator-side
//! provisioning handshake on L2CAP CoC per R2-PROVISION + R2-BLE §6.2.
//!
//! ## Role in the R2 stack
//!
//! L5 Trust & Identity — closes the gap between the L2 Discovery
//! signpost (beacon-observer) and an enrolled DeviceCertificate. Runs
//! one full JoinInvite → JoinRequest → JoinResponse exchange against a
//! freshly-flashed device that's beaconing in provisioning mode.
//!
//! ## Pipeline (matches r2-core/tools/r2-provision/src/provision.rs)
//!
//! 1. Restore the apiary's `TrustGroup` from the off-tree TG signing
//!    key (same path the keyholder uses).
//! 2. Generate a fresh single-use JoinCode + sign a JoinInvite.
//! 3. Wait for the target BLE address to surface in BlueZ's cache
//!    (brief LE discovery if needed), then establish an LE ACL link.
//! 4. Open an L2CAP CoC stream on PSM `0x00D2` with `BT_SECURITY_LOW`.
//! 5. Send the JoinInvite wrapped in a GROUP_MGMT compact frame.
//! 6. Read the device's JoinRequest reply (length-prefixed compact
//!    frame); verify the outer GROUP_MGMT signature against the
//!    device's announced `sender_pk` (== device DEV_PK).
//! 7. Call `TrustGroup::process_join_request` to mint the encrypted
//!    JoinResponse bundle (DeviceCertificate + EpochSecret).
//! 8. Wrap as GROUP_MGMT JoinResponse + send.
//! 9. Emit `r2.composer.device.identity_observed{ble_addr, device_pk,
//!    cert_hex, …}` — the Provision sentant catches that, applies
//!    slot-id disambiguation, writes the cert file, and transitions
//!    the slot to `enrolled`.
//!
//! ## Linux-only
//!
//! `bluer` is Linux-only. On other platforms this whole module compiles
//! away to a stub plugin that logs a warning at init and stays inert,
//! so the orchestrator still boots cleanly for development on macOS.
//!
//! ## Retires F3's hand-rolled 144-byte cert
//!
//! F3 wrote a fabricated 144-byte cert format
//! (`device_pk || tg_pub || valid_from || valid_until || sig`). That's
//! not the R2-TRUST `DeviceCertificate` (147 bytes, includes role,
//! revocation_id, sequence per R2-TRUST §4). This substrate emits the
//! REAL cert, minted by `TrustGroup::process_join_request`. The
//! Provision sentant writes it to disk in R2-TRUST format.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use r2_engine::plugin::{
    Plugin, PluginCommand, PluginError, PluginId, PluginResponse, PluginResult,
};

use crate::sentants::RosterCtx;

/// Side-slot the Provision sentant fills before dispatching `CMD_START`.
pub type ProvisionHandshakeSlot = Arc<Mutex<Option<HandshakeRequest>>>;

#[derive(Debug, Clone)]
pub struct HandshakeRequest {
    /// Target BLE address from the beacon observation, e.g. `"AA:BB:CC:DD:EE:FF"`.
    pub ble_addr: String,
    /// Human-readable device name to record in the TG roster. The
    /// orchestrator picks this — often `<role>:<host>:<short-uuid>`.
    pub device_name: String,
    /// Optional `slot_id` hint from the Provision sentant if it could
    /// already disambiguate. The substrate just passes it back in the
    /// identity_observed payload — it doesn't use the hint internally.
    pub slot_id_hint: Option<String>,
}

pub const CMD_START: PluginCommand = 0x01;

pub const ERR_NO_REQUEST:       u8 = 0x01;
pub const ERR_NO_APIARY:        u8 = 0x02;
pub const ERR_TG_LOAD:          u8 = 0x03;
pub const ERR_BLE_INIT:         u8 = 0x04;
pub const ERR_BLE_CONNECT:      u8 = 0x05;
pub const ERR_HANDSHAKE_PROTO:  u8 = 0x06;
pub const ERR_HANDSHAKE_TIMEOUT: u8 = 0x07;
pub const ERR_UNKNOWN_COMMAND:  u8 = 0xFE;
pub const ERR_NOT_LINUX:        u8 = 0xFF;

pub struct ProvisionHandshakePlugin {
    id: PluginId,
    #[allow(dead_code)] // used by the Linux backend's spawned task
    apiary_ctx: RosterCtx,
    #[allow(dead_code)]
    config_root: PathBuf,
    /// Side-slot for incoming requests.
    slot: ProvisionHandshakeSlot,
    /// Pre-hashed event names for poll() emissions.
    hash_identity_observed: u32,
    hash_handshake_start:   u32,
    hash_handshake_error:   u32,
    /// Pending events from the background tokio task. Drained by poll().
    pending: Arc<Mutex<Vec<(u32, Vec<u8>)>>>,
    /// Per-call output buffer; lives here so poll()'s returned &[u8]
    /// can borrow it across the next call. Mirrors keyholder/provision.
    cached_out: Vec<u8>,
    /// Set true after the background task has been spawned, so a second
    /// init doesn't double-spawn.
    started: bool,
}

impl ProvisionHandshakePlugin {
    pub fn new(
        id: PluginId,
        apiary_ctx: RosterCtx,
        slot: ProvisionHandshakeSlot,
        config_root: PathBuf,
    ) -> Self {
        Self {
            id,
            apiary_ctx,
            config_root,
            slot,
            hash_identity_observed: r2_fnv::fnv1a_32(b"r2.composer.device.identity_observed"),
            hash_handshake_start:   r2_fnv::fnv1a_32(b"r2.composer.provision.handshake.start"),
            hash_handshake_error:   r2_fnv::fnv1a_32(b"r2.composer.provision.handshake.error"),
            pending: Arc::new(Mutex::new(Vec::new())),
            cached_out: Vec::with_capacity(512),
            started: false,
        }
    }

    /// Production root for off-tree TG signing key + cert dir. Mirrors
    /// [`crate::substrate::KeyholderPlugin::default_config_root`] —
    /// callers in production pass `KeyholderPlugin::default_config_root()`.
    pub fn default_config_root() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join(".config/r2-composer")
    }
}

impl Plugin for ProvisionHandshakePlugin {
    fn execute(&mut self, command: PluginCommand, _data: &[u8]) -> PluginResult {
        match command {
            CMD_START => {
                if !self.started {
                    spawn_worker(
                        self.apiary_ctx.clone(),
                        self.config_root.clone(),
                        self.slot.clone(),
                        self.pending.clone(),
                        self.hash_identity_observed,
                        self.hash_handshake_start,
                        self.hash_handshake_error,
                    );
                    self.started = true;
                }
                // The slot has a request; the worker thread polls it.
                // We just return Ok and let the background task pick up.
                PluginResult::Ok(PluginResponse::empty())
            }
            _ => PluginResult::Error(PluginError::new(
                ERR_UNKNOWN_COMMAND, "unknown command")),
        }
    }
    fn name(&self) -> &str { "provision-handshake" }
    fn id(&self) -> PluginId { self.id }

    fn poll(&mut self) -> Option<(u32, &[u8])> {
        let mut pending = self.pending.lock().unwrap();
        if pending.is_empty() { return None }
        let (hash, payload) = pending.remove(0);
        // Stash in a per-call buffer; can't borrow `self.pending`'s
        // bytes across the return without aliasing.
        self.cached_out = payload;
        Some((hash, &self.cached_out))
    }
}

// ── Spawn worker thread ──────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn spawn_worker(
    apiary_ctx: RosterCtx,
    config_root: PathBuf,
    slot: ProvisionHandshakeSlot,
    pending: Arc<Mutex<Vec<(u32, Vec<u8>)>>>,
    hash_identity_observed: u32,
    hash_handshake_start: u32,
    hash_handshake_error: u32,
) {
    std::thread::Builder::new()
        .name("provision-handshake".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::error!("provision-handshake: tokio init failed: {e}");
                    return;
                }
            };
            rt.block_on(linux::worker_loop(
                apiary_ctx, config_root, slot, pending,
                hash_identity_observed, hash_handshake_start, hash_handshake_error,
            ));
        })
        .expect("spawn provision-handshake thread");
}

#[cfg(not(target_os = "linux"))]
fn spawn_worker(
    _apiary_ctx: RosterCtx,
    _config_root: PathBuf,
    _slot: ProvisionHandshakeSlot,
    _pending: Arc<Mutex<Vec<(u32, Vec<u8>)>>>,
    _hash_identity_observed: u32,
    _hash_handshake_start: u32,
    _hash_handshake_error: u32,
) {
    tracing::warn!(
        "provision-handshake: bluer is Linux-only; substrate inert on this platform"
    );
}

// ── Linux backend ───────────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use anyhow::{anyhow, bail, Context, Result};
    use bluer::l2cap::{Security, SecurityLevel, Socket, SocketAddr as L2capSocketAddr, Stream};
    use bluer::{Adapter, AdapterEvent, Address, AddressType, DiscoveryFilter, DiscoveryTransport};
    use ed25519_dalek::{SigningKey, VerifyingKey};
    use r2_trust::group_mgmt::{GroupMgmtMessage, GroupMgmtOpCode};
    use r2_trust::join::{JoinInvite, JoinRequestPayload};
    use r2_trust::lifecycle::{TrustGroup, DEFAULT_CERT_TTL_SECS};
    use r2_trust::types::JOIN_INVITE_LEN;
    use r2_wire::compact::{decode_compact, encode_compact};
    use r2_wire::types::{CompactHeader, CompactMessage, Flags, MsgType};
    use crate::substrate::tg_state;
    use std::path::Path;
    use std::str::FromStr;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_stream::StreamExt;

    const R2_PSM: u16 = 0x00D2;
    const INVITE_TTL_SECS: u64 = 300;          // 5 min
    const JOIN_REQUEST_TIMEOUT_SECS: u64 = 30;
    const MAX_INBOUND_FRAME: u16 = 4096;
    const POLL_SLOT_MS: u64 = 250;

    pub async fn worker_loop(
        apiary_ctx: RosterCtx,
        config_root: PathBuf,
        slot: ProvisionHandshakeSlot,
        pending: Arc<Mutex<Vec<(u32, Vec<u8>)>>>,
        hash_identity_observed: u32,
        hash_handshake_start: u32,
        hash_handshake_error: u32,
    ) {
        loop {
            // Poll the side-slot for a request. Cheap busy-wait keeps
            // the worker structure simple — handshakes are rare and
            // 250ms latency is fine.
            let req = {
                let mut s = slot.lock().unwrap();
                s.take()
            };
            let req = match req {
                Some(r) => r,
                None => {
                    tokio::time::sleep(Duration::from_millis(POLL_SLOT_MS)).await;
                    continue;
                }
            };

            // Emit a start event so the webapp / chat log can show
            // "handshake initiated against AA:BB:..."
            emit(&pending, hash_handshake_start, serde_json::json!({
                "ble_addr": req.ble_addr.clone(),
                "slot_id_hint": req.slot_id_hint.clone(),
            }));

            // Resolve the in-tree apiary dir — the TG state file lives
            // under it. (load_tg_sk re-resolves it for the off-tree key.)
            let apiary_dir = match apiary_ctx.lock().unwrap().clone() {
                Some(d) => d,
                None => {
                    emit_error(&pending, hash_handshake_error,
                        ERR_NO_APIARY, "no apiary open", &req);
                    continue;
                }
            };

            // Resolve the apiary signing key from off-tree.
            let sk = match load_tg_sk(&apiary_ctx, &config_root) {
                Ok(sk) => sk,
                Err(e) => {
                    emit_error(&pending, hash_handshake_error,
                        ERR_TG_LOAD, &format!("load TG SK: {e}"), &req);
                    continue;
                }
            };

            match run_handshake(&req, sk, &apiary_dir).await {
                Ok(result) => {
                    let payload = serde_json::json!({
                        "ble_addr":     result.ble_addr,
                        "device_pk":    hex::encode(result.device_pk.as_bytes()),
                        "cert_hex":     hex::encode(&result.cert_bytes),
                        "device_name":  result.device_name,
                        "slot_id_hint": req.slot_id_hint.clone(),
                    });
                    emit(&pending, hash_identity_observed, payload);
                }
                Err(e) => {
                    emit_error(&pending, hash_handshake_error,
                        e.code, &e.message, &req);
                }
            }
        }
    }

    /// Result of a successful handshake.
    struct HandshakeOk {
        ble_addr: String,
        device_pk: VerifyingKey,
        /// 147-byte R2-TRUST DeviceCertificate from the encrypted
        /// JoinResponse bundle. The orchestrator can decrypt the
        /// bundle locally (it minted it), but for v0.1 we surface the
        /// cert as-is — the orchestrator persists it to disk via the
        /// Provision sentant.
        cert_bytes: Vec<u8>,
        device_name: String,
    }

    struct HandshakeErr {
        code: u8,
        message: String,
    }

    async fn run_handshake(req: &HandshakeRequest, sk: SigningKey, apiary_dir: &Path) -> std::result::Result<HandshakeOk, HandshakeErr> {
        let target = Address::from_str(&req.ble_addr)
            .map_err(|e| HandshakeErr {
                code: ERR_BLE_CONNECT,
                message: format!("parse BLE addr {}: {e}", req.ble_addr),
            })?;
        let now = unix_now();

        // Restore the apiary TG from persisted state so prior enrolments
        // and the GROUP_MGMT sequence counter survive restarts — R2-TRUST
        // §5.6. First run (no state file) starts fresh from the key. A
        // corrupt state file is a hard failure, not a silent rebuild.
        let mut tg = match tg_state::load(apiary_dir, sk.clone()) {
            Ok(Some(tg)) => tg,
            Ok(None) => TrustGroup::from_signing_key(sk.clone(), now).map_err(|e| HandshakeErr {
                code: ERR_TG_LOAD,
                message: format!("TrustGroup::from_signing_key: {e:?}"),
            })?,
            Err(e) => return Err(HandshakeErr {
                code: ERR_TG_LOAD,
                message: format!("load TG state {}: {e}", tg_state::state_path(apiary_dir).display()),
            }),
        };
        let mut rng = rand::rngs::OsRng;
        let invite_code = *tg.generate_join_code(&mut rng, now, INVITE_TTL_SECS).value();
        let trust_group_id = tg.trust_group_id();
        let invite = JoinInvite::new_signed(
            invite_code, trust_group_id, &sk, now, now + INVITE_TTL_SECS, 1);
        tracing::info!(
            "provision-handshake: minted JoinInvite code={}.. expires_at={}",
            hex::encode(&invite.invite_code[..4]), invite.expires_at
        );

        // BLE setup.
        let session = bluer::Session::new().await.map_err(|e| HandshakeErr {
            code: ERR_BLE_INIT, message: format!("bluer::Session::new: {e}"),
        })?;
        let adapter = session.default_adapter().await.map_err(|e| HandshakeErr {
            code: ERR_BLE_INIT, message: format!("default_adapter: {e}"),
        })?;
        adapter.set_powered(true).await.map_err(|e| HandshakeErr {
            code: ERR_BLE_INIT, message: format!("set_powered: {e}"),
        })?;
        wait_for_target_visible(&adapter, target, Duration::from_secs(8))
            .await
            .map_err(|e| HandshakeErr {
                code: ERR_BLE_CONNECT,
                message: e.to_string(),
            })?;

        let device = adapter.device(target).map_err(|e| HandshakeErr {
            code: ERR_BLE_CONNECT, message: format!("adapter.device: {e}"),
        })?;
        let already_connected = device.is_connected().await.unwrap_or(false);
        if !already_connected {
            tokio::time::timeout(Duration::from_secs(8), device.connect())
                .await
                .map_err(|_| HandshakeErr {
                    code: ERR_BLE_CONNECT,
                    message: format!("ACL connect to {target} timed out (8s)"),
                })?
                .map_err(|e| HandshakeErr {
                    code: ERR_BLE_CONNECT,
                    message: format!("ACL connect to {target}: {e}"),
                })?;
            tokio::time::sleep(Duration::from_millis(200)).await;
        }

        let mut stream = l2cap_connect_low_security(target, AddressType::LePublic, R2_PSM)
            .await
            .map_err(|e| HandshakeErr {
                code: ERR_BLE_CONNECT,
                message: e.to_string(),
            })?;
        tracing::info!("provision-handshake: L2CAP CoC established (PSM 0x{:04X})", R2_PSM);
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Send JoinInvite.
        let invite_payload = invite.to_bytes().to_vec();
        let invite_frame = build_groupmgmt_frame(
            &mut tg, GroupMgmtOpCode::JoinInvite, invite_payload, &sk
        ).map_err(|e| HandshakeErr {
            code: ERR_HANDSHAKE_PROTO,
            message: format!("build JoinInvite frame: {e}"),
        })?;
        write_frame(&mut stream, &invite_frame).await.map_err(|e| HandshakeErr {
            code: ERR_HANDSHAKE_PROTO,
            message: format!("write JoinInvite: {e}"),
        })?;
        tracing::info!(
            "provision-handshake: JoinInvite sent ({} bytes, inner {})",
            invite_frame.len(), JOIN_INVITE_LEN
        );

        // Wait for JoinRequest.
        let request_frame = tokio::time::timeout(
            Duration::from_secs(JOIN_REQUEST_TIMEOUT_SECS),
            read_frame(&mut stream),
        )
        .await
        .map_err(|_| HandshakeErr {
            code: ERR_HANDSHAKE_TIMEOUT,
            message: format!("timed out waiting for JoinRequest ({JOIN_REQUEST_TIMEOUT_SECS}s)"),
        })?
        .map_err(|e| HandshakeErr {
            code: ERR_HANDSHAKE_PROTO,
            message: format!("read JoinRequest: {e}"),
        })?;
        let (device_pk, join_code, _nonce) = parse_join_request(&request_frame)
            .map_err(|e| HandshakeErr {
                code: ERR_HANDSHAKE_PROTO,
                message: format!("parse JoinRequest: {e}"),
            })?;
        tracing::info!(
            "provision-handshake: JoinRequest verified — device DEV_PK={}..",
            hex::encode(&device_pk.as_bytes()[..4])
        );

        // Mint + send JoinResponse.
        let encrypted = tg.process_join_request(
            &mut rng, unix_now(), &join_code, &device_pk,
            req.device_name.clone(), DEFAULT_CERT_TTL_SECS,
        ).map_err(|e| HandshakeErr {
            code: ERR_HANDSHAKE_PROTO,
            message: format!("process_join_request: {e:?}"),
        })?;

        // Persist the TG (new member + bumped sequence) BEFORE the device
        // sees the JoinResponse. The device commits to this sequence on
        // receipt, so the orchestrator must have it on disk first — a
        // restart that reused the sequence would collide with the device's
        // replay protection (R2-TRUST §5.6). A save failure aborts here,
        // before the device is enrolled, so the in-memory TG is simply
        // dropped and the next attempt starts clean.
        //
        // Known v0.1 limitation: if the JoinResponse write below fails
        // *after* this save, the member is persisted while the device never
        // completed — that device_pk is then blocked (DuplicateMember)
        // until revoked. We accept this over the sequence-collision risk.
        tg_state::save(apiary_dir, &tg).map_err(|e| HandshakeErr {
            code: ERR_TG_LOAD,
            message: format!("persist TG state: {e}"),
        })?;

        let mut response_payload = Vec::with_capacity(
            encrypted.nonce.len() + encrypted.ciphertext.len());
        response_payload.extend_from_slice(&encrypted.nonce);
        response_payload.extend_from_slice(&encrypted.ciphertext);
        let response_frame = build_groupmgmt_frame(
            &mut tg, GroupMgmtOpCode::JoinResponse, response_payload, &sk
        ).map_err(|e| HandshakeErr {
            code: ERR_HANDSHAKE_PROTO,
            message: format!("build JoinResponse frame: {e}"),
        })?;
        write_frame(&mut stream, &response_frame).await.map_err(|e| HandshakeErr {
            code: ERR_HANDSHAKE_PROTO,
            message: format!("write JoinResponse: {e}"),
        })?;
        tracing::info!(
            "provision-handshake: JoinResponse sent ({} bytes); device enrolled",
            response_frame.len()
        );
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Pull the freshly-issued DeviceCertificate out of the TG's member
        // list — process_join_request just added it. We surface the
        // 147-byte canonical wire form for the Provision sentant to
        // persist.
        let member = tg.find_member(device_pk.as_bytes())
            .ok_or(HandshakeErr {
                code: ERR_HANDSHAKE_PROTO,
                message: "process_join_request succeeded but member not in TG".into(),
            })?;
        let cert_bytes = member.certificate.to_bytes().to_vec();

        drop(stream);
        if !already_connected {
            let _ = device.disconnect().await;
        }

        Ok(HandshakeOk {
            ble_addr: req.ble_addr.clone(),
            device_pk,
            cert_bytes,
            device_name: req.device_name.clone(),
        })
    }

    fn build_groupmgmt_frame(
        tg: &mut TrustGroup,
        opcode: GroupMgmtOpCode,
        payload: Vec<u8>,
        sk: &SigningKey,
    ) -> Result<Vec<u8>> {
        let now = unix_now();
        let trust_group_id = tg.trust_group_id();
        let sender_pk = *sk.verifying_key().as_bytes();
        let sequence = tg.sequence();
        let mut group_msg = GroupMgmtMessage::new(
            opcode, trust_group_id, sender_pk, sequence, now, payload);
        group_msg.sign(sk);
        let group_bytes = group_msg.encode()
            .map_err(|e| anyhow!("GroupMgmtMessage::encode: {e:?}"))?;
        let header = CompactHeader {
            version: 0,
            msg_type: MsgType::GroupMgmt,
            flags: Flags { has_route: false, has_hmac: false, mcu_origin: false },
            ttl: 1, k: 1,
            msg_id: rand_u16(),
            event_hash: 0, target: 0,
        };
        let msg = CompactMessage { header, route: None, payload: &group_bytes, hmac_tag: None };
        let mut buf = vec![0u8; group_bytes.len() + 64];
        let n = encode_compact(&msg, &mut buf)
            .map_err(|e| anyhow!("encode_compact: {e:?}"))?;
        buf.truncate(n);
        Ok(buf)
    }

    fn parse_join_request(frame: &[u8]) -> Result<(VerifyingKey, [u8; 16], [u8; 32])> {
        let outer = decode_compact(frame)
            .map_err(|e| anyhow!("decode_compact: {e:?}"))?;
        if outer.header.msg_type != MsgType::GroupMgmt {
            bail!("expected GroupMgmt msg_type, got {:?}", outer.header.msg_type);
        }
        let group_msg = GroupMgmtMessage::decode(outer.payload)
            .map_err(|e| anyhow!("decode group_mgmt: {e:?}"))?;
        if group_msg.opcode != GroupMgmtOpCode::JoinRequest {
            bail!("expected JoinRequest opcode, got {:?}", group_msg.opcode);
        }
        let device_pk = VerifyingKey::from_bytes(&group_msg.sender_pk)
            .map_err(|e| anyhow!("invalid sender_pk: {e:?}"))?;
        group_msg.verify(&device_pk)
            .map_err(|e| anyhow!("JoinRequest signature: {e:?}"))?;
        let payload = JoinRequestPayload::decode(&group_msg.payload)
            .map_err(|e| anyhow!("decode JoinRequestPayload: {e:?}"))?;
        Ok((device_pk, payload.join_code, payload.nonce))
    }

    async fn write_frame(stream: &mut Stream, frame: &[u8]) -> Result<()> {
        if frame.len() > u16::MAX as usize {
            bail!("frame too large: {} bytes", frame.len());
        }
        let len_bytes = (frame.len() as u16).to_le_bytes();
        stream.write_all(&len_bytes).await.context("L2CAP write length")?;
        stream.write_all(frame).await.context("L2CAP write payload")?;
        stream.flush().await.context("L2CAP flush")?;
        Ok(())
    }

    async fn read_frame(stream: &mut Stream) -> Result<Vec<u8>> {
        let mut len_buf = [0u8; 2];
        stream.read_exact(&mut len_buf).await.context("L2CAP read length")?;
        let len = u16::from_le_bytes(len_buf);
        if len == 0 || len > MAX_INBOUND_FRAME {
            bail!("invalid inbound length: {len}");
        }
        let mut buf = vec![0u8; len as usize];
        stream.read_exact(&mut buf).await.context("L2CAP read payload")?;
        Ok(buf)
    }

    async fn l2cap_connect_low_security(
        target: Address,
        known_addr_type: AddressType,
        psm: u16,
    ) -> Result<Stream> {
        let types = if known_addr_type == AddressType::LePublic {
            [AddressType::LePublic, AddressType::LeRandom]
        } else {
            [AddressType::LeRandom, AddressType::LePublic]
        };
        for addr_type in types {
            let sa = L2capSocketAddr::new(target, addr_type, psm);
            let result = tokio::time::timeout(Duration::from_secs(10), async {
                let socket = Socket::<Stream>::new_stream()?;
                socket.set_security(Security {
                    level: SecurityLevel::Low,
                    key_size: 0,
                })?;
                socket.bind(L2capSocketAddr::any_le())?;
                socket.connect(sa).await
            }).await;
            if let Ok(Ok(s)) = result { return Ok(s); }
        }
        bail!("L2CAP CoC connect to {target} failed (both address types tried)")
    }

    async fn wait_for_target_visible(
        adapter: &Adapter,
        target: Address,
        timeout: Duration,
    ) -> Result<()> {
        if let Ok(known) = adapter.device_addresses().await {
            if known.contains(&target) { return Ok(()); }
        }
        let filter = DiscoveryFilter {
            transport: DiscoveryTransport::Le,
            ..Default::default()
        };
        adapter.set_discovery_filter(filter).await.context("set_discovery_filter")?;
        let mut disco = adapter.discover_devices().await.context("discover_devices")?;
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            match tokio::time::timeout(remaining, disco.next()).await {
                Ok(Some(AdapterEvent::DeviceAdded(addr))) if addr == target => return Ok(()),
                Ok(Some(_)) => {}
                Ok(None) => break,
                Err(_) => break,
            }
        }
        bail!("target {target} not seen within {}s — is the device powered + advertising?", timeout.as_secs())
    }

    fn load_tg_sk(apiary_ctx: &RosterCtx, config_root: &PathBuf) -> Result<SigningKey> {
        let apiary_dir = apiary_ctx.lock().unwrap().clone()
            .ok_or_else(|| anyhow!("no apiary open"))?;
        let apiary_name = apiary_dir.file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("bad apiary path"))?
            .to_string();
        let priv_path = config_root
            .join("apiaries").join(apiary_name).join("tg_signer/tg_priv.bin");
        let bytes = std::fs::read(&priv_path)
            .map_err(|e| anyhow!("read {}: {e}", priv_path.display()))?;
        if bytes.len() != 32 {
            bail!("expected 32-byte seed, got {}", bytes.len());
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&bytes);
        Ok(SigningKey::from_bytes(&seed))
    }

    fn unix_now() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs()).unwrap_or(0)
    }

    fn rand_u16() -> u16 {
        use rand::RngCore;
        let mut buf = [0u8; 2];
        rand::rngs::OsRng.fill_bytes(&mut buf);
        u16::from_le_bytes(buf)
    }
}

// ── Common helpers ───────────────────────────────────────────────────

fn emit(
    pending: &Arc<Mutex<Vec<(u32, Vec<u8>)>>>,
    hash: u32,
    payload: serde_json::Value,
) {
    let bytes = serde_json::to_vec(&payload).unwrap_or_default();
    pending.lock().unwrap().push((hash, bytes));
}

fn emit_error(
    pending: &Arc<Mutex<Vec<(u32, Vec<u8>)>>>,
    hash: u32,
    code: u8,
    message: &str,
    req: &HandshakeRequest,
) {
    tracing::warn!("provision-handshake: error {code:#04X} — {message}");
    let payload = serde_json::json!({
        "ble_addr":     req.ble_addr,
        "slot_id_hint": req.slot_id_hint,
        "code":         code,
        "message":      message,
    });
    let bytes = serde_json::to_vec(&payload).unwrap_or_default();
    pending.lock().unwrap().push((hash, bytes));
}
