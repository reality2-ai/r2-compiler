//! `provision` plugin — owns the active apiary's WiFi credential set
//! and composes `#wifi_offer` blobs for the BLE-bootstrap path.
//!
//! Per SPEC-APIARY-FLASH §5. v0.1 (F3 scope): credential storage +
//! offer composition; the actual BLE radio handoff is F4 (`beacon-
//! observer` plugin will open the L2CAP CoC and ship the bytes this
//! plugin produces).
//!
//! ## wifi_networks.toml location
//!
//! Off-tree, mode 0600:
//!   `~/.config/r2-composer/apiaries/<name>/wifi_networks.toml`
//!
//! The PSK MUST NOT cross the /r2 WebSocket to the webapp and MUST NOT
//! appear in tracing logs (PROCESS.md).
//!
//! ## `#wifi_offer` v0.1 format (concrete, subject to upstream)
//!
//! ```
//!   magic         :  4 bytes — b"R2WO"
//!   version       :  1 byte  — 0x01
//!   ssid_len      :  1 byte
//!   ssid          :  ssid_len bytes (UTF-8)
//!   psk_len       :  1 byte
//!   psk           :  psk_len bytes (UTF-8)
//!   cert_len      :  2 bytes (BE u16)
//!   cert          :  cert_len bytes (the 144-byte DeviceCertificate)
//! ```
//!
//! The offer is signed implicitly via the embedded DeviceCertificate
//! (which itself carries an Ed25519 signature from the apiary's TG
//! private key). The device verifies the cert + extracts WiFi creds
//! per its embedded Bootstrap sentant.
//!
//! ## Commands
//!
//! - `CMD_UPSERT_NETWORK` (0x01) — payload `{name, ssid, psk, is_default}`.
//!   Reads (or initialises) wifi_networks.toml, replaces the entry with
//!   matching `name`, rewrites the file with mode 0600. Emits
//!   `r2.composer.provision.network.upserted{name, default}`.
//! - `CMD_LIST_NETWORKS` (0x02) — returns network names + which is
//!   default (NEVER the PSK).
//! - `CMD_COMPOSE_OFFER` (0x03) — payload via shared slot. Composes the
//!   `#wifi_offer` bytes; emits `r2.composer.provision.offer.composed`
//!   with `{slot_id, offer_hex, ssid, network_name}`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use r2_engine::plugin::{
    Plugin, PluginCommand, PluginError, PluginId, PluginResponse, PluginResult,
};
use serde::{Deserialize, Serialize};

use crate::sentants::RosterCtx;

/// Side-channel for CMD_COMPOSE_OFFER — the full cert + slot_id is more
/// than the bus payload should carry.
pub type ProvisionSlot = Arc<Mutex<Option<ComposeOfferRequest>>>;

#[derive(Debug, Clone)]
pub struct ComposeOfferRequest {
    pub slot_id: String,
    /// Which named network (from wifi_networks.toml) to use. None ⇒ default.
    pub network_name: Option<String>,
    /// The 144-byte DeviceCertificate.
    pub cert: Vec<u8>,
}

pub const CMD_UPSERT_NETWORK: PluginCommand = 0x01;
pub const CMD_LIST_NETWORKS:  PluginCommand = 0x02;
pub const CMD_COMPOSE_OFFER:  PluginCommand = 0x03;

pub const ERR_NO_APIARY:       u8 = 0x01;
pub const ERR_BAD_PAYLOAD:     u8 = 0x02;
pub const ERR_IO:              u8 = 0x03;
pub const ERR_NO_REQUEST:      u8 = 0x04;
pub const ERR_NO_NETWORK:      u8 = 0x05;
pub const ERR_BAD_CERT:        u8 = 0x06;
pub const ERR_UNKNOWN_COMMAND: u8 = 0xFE;

#[derive(Debug, Clone, Deserialize)]
struct UpsertPayload {
    name: String,
    ssid: String,
    psk: String,
    #[serde(default)]
    is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WifiNetwork {
    name: String,
    ssid: String,
    psk: String,
    #[serde(default)]
    is_default: bool,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct WifiNetworksFile {
    #[serde(default, rename = "wifi_networks")]
    networks: Vec<WifiNetwork>,
}

pub struct ProvisionPlugin {
    id: PluginId,
    apiary_ctx: RosterCtx,
    offer_slot: ProvisionSlot,
    /// Off-tree root. Production: `$HOME/.config/r2-composer/`.
    config_root: PathBuf,
    hash_upserted: u32,
    hash_offer_composed: u32,
    hash_error: u32,
    out_buf: Vec<u8>,
    pending: Vec<(u32, Vec<u8>)>,
}

impl ProvisionPlugin {
    pub fn new(
        id: PluginId,
        apiary_ctx: RosterCtx,
        offer_slot: ProvisionSlot,
        config_root: PathBuf,
    ) -> Self {
        Self {
            id,
            apiary_ctx,
            offer_slot,
            config_root,
            hash_upserted:       r2_fnv::fnv1a_32(b"r2.composer.provision.network.upserted"),
            hash_offer_composed: r2_fnv::fnv1a_32(b"r2.composer.provision.offer.composed"),
            hash_error:          r2_fnv::fnv1a_32(b"r2.composer.provision.error"),
            out_buf: Vec::with_capacity(512),
            pending: Vec::new(),
        }
    }

    /// `<config_root>/apiaries/<name>/wifi_networks.toml`.
    fn networks_path(&self) -> Result<PathBuf, (u8, String)> {
        let apiary_dir = {
            let ctx = self.apiary_ctx.lock().unwrap();
            match ctx.as_ref() {
                Some(p) => p.clone(),
                None => return Err((ERR_NO_APIARY, "no apiary open".into())),
            }
        };
        let apiary_name = apiary_dir.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        Ok(self.config_root
            .join("apiaries")
            .join(apiary_name)
            .join("wifi_networks.toml"))
    }

    fn load_networks(&self) -> Result<WifiNetworksFile, (u8, String)> {
        let path = self.networks_path()?;
        if !path.exists() {
            return Ok(WifiNetworksFile::default());
        }
        let s = std::fs::read_to_string(&path)
            .map_err(|e| (ERR_IO, format!("read {}: {e}", path.display())))?;
        toml::from_str::<WifiNetworksFile>(&s)
            .map_err(|e| (ERR_IO, format!("parse {}: {e}", path.display())))
    }

    fn save_networks(&self, f: &WifiNetworksFile) -> Result<(), (u8, String)> {
        let path = self.networks_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| (ERR_IO, format!("mkdir {}: {e}", parent.display())))?;
        }
        let s = toml::to_string_pretty(f)
            .map_err(|e| (ERR_IO, format!("serialise: {e}")))?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, &s)
            .map_err(|e| (ERR_IO, format!("write {}: {e}", tmp.display())))?;
        // 0600 — PSK material.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        std::fs::rename(&tmp, &path)
            .map_err(|e| (ERR_IO, format!("rename {} → {}: {e}", tmp.display(), path.display())))?;
        Ok(())
    }

    fn handle_upsert(&mut self, data: &[u8]) -> PluginResult {
        let payload: UpsertPayload = match serde_json::from_slice(data) {
            Ok(p) => p,
            Err(e) => return PluginResult::Error(
                PluginError::new(ERR_BAD_PAYLOAD, &format!("bad payload: {e}"))),
        };
        let mut f = match self.load_networks() {
            Ok(f) => f,
            Err((c, m)) => return PluginResult::Error(PluginError::new(c, &m)),
        };
        // Replace or append. If is_default, clear any other default.
        if payload.is_default {
            for n in f.networks.iter_mut() { n.is_default = false; }
        }
        let new_net = WifiNetwork {
            name: payload.name.clone(),
            ssid: payload.ssid,
            psk: payload.psk,
            is_default: payload.is_default,
        };
        if let Some(slot) = f.networks.iter_mut().find(|n| n.name == payload.name) {
            *slot = new_net;
        } else {
            f.networks.push(new_net);
        }
        if let Err((c, m)) = self.save_networks(&f) {
            return PluginResult::Error(PluginError::new(c, &m));
        }
        let payload_json = self.pack_with(&serde_json::json!({
            "name": payload.name,
            "default": payload.is_default,
        }));
        self.pending.push((self.hash_upserted, payload_json));
        PluginResult::Ok(PluginResponse::empty())
    }

    fn handle_list(&mut self) -> PluginResult {
        let f = match self.load_networks() {
            Ok(f) => f,
            Err((c, m)) => return PluginResult::Error(PluginError::new(c, &m)),
        };
        let listing: Vec<_> = f.networks.iter()
            .map(|n| serde_json::json!({
                "name": n.name,
                "ssid": n.ssid,
                "is_default": n.is_default,
            }))
            .collect();
        // SSID is broadcast-visible already; PSK is omitted.
        // PluginResponse caps at 128 bytes — emit as an event for the
        // full payload, return a small ack as the direct response.
        let bytes = self.pack_with(&serde_json::json!({ "networks": listing }));
        // Re-use a "listed" hash so the webapp can pick it up.
        let listed_hash = r2_fnv::fnv1a_32(b"r2.composer.provision.networks.listed");
        self.pending.push((listed_hash, bytes));
        let ack = serde_json::to_vec(&serde_json::json!({"count": f.networks.len()})).unwrap();
        PluginResult::Ok(PluginResponse::with_data(&ack))
    }

    fn handle_compose_offer(&mut self) -> PluginResult {
        let taken = { self.offer_slot.lock().unwrap().take() };
        let req = match taken {
            Some(r) => r,
            None => {
                self.queue_error(ERR_NO_REQUEST, "CMD_COMPOSE_OFFER with no request".into(), "");
                return PluginResult::Ok(PluginResponse::empty());
            }
        };
        if req.cert.len() != 144 {
            self.queue_error(ERR_BAD_CERT,
                format!("cert must be 144 bytes, got {}", req.cert.len()), &req.slot_id);
            return PluginResult::Ok(PluginResponse::empty());
        }
        let f = match self.load_networks() {
            Ok(f) => f,
            Err((c, m)) => {
                self.queue_error(c, m, &req.slot_id);
                return PluginResult::Ok(PluginResponse::empty());
            }
        };
        let net = match req.network_name.as_ref() {
            Some(name) => f.networks.iter().find(|n| &n.name == name),
            None => f.networks.iter().find(|n| n.is_default)
                .or_else(|| f.networks.first()),
        };
        let net = match net {
            Some(n) => n,
            None => {
                self.queue_error(ERR_NO_NETWORK,
                    "no matching wifi network — run provision.network.upsert first".into(),
                    &req.slot_id);
                return PluginResult::Ok(PluginResponse::empty());
            }
        };

        // Compose the offer bytes.
        let ssid = net.ssid.as_bytes();
        let psk  = net.psk.as_bytes();
        if ssid.len() > 0xFF || psk.len() > 0xFF {
            self.queue_error(ERR_BAD_PAYLOAD,
                "ssid or psk exceeds 255 bytes".into(), &req.slot_id);
            return PluginResult::Ok(PluginResponse::empty());
        }
        let mut offer = Vec::with_capacity(8 + ssid.len() + psk.len() + req.cert.len());
        offer.extend_from_slice(b"R2WO");
        offer.push(0x01);
        offer.push(ssid.len() as u8);
        offer.extend_from_slice(ssid);
        offer.push(psk.len() as u8);
        offer.extend_from_slice(psk);
        offer.extend_from_slice(&(req.cert.len() as u16).to_be_bytes());
        offer.extend_from_slice(&req.cert);

        // NEVER include the PSK in the emitted event — only the offer
        // hex (which the BLE plugin later consumes) and the network
        // name + SSID.
        let payload = self.pack_with(&serde_json::json!({
            "slot_id":      req.slot_id,
            "network_name": net.name.clone(),
            "ssid":         net.ssid.clone(),
            "offer_len":    offer.len(),
            "offer_hex":    hex::encode(&offer),
        }));
        self.pending.push((self.hash_offer_composed, payload));
        PluginResult::Ok(PluginResponse::empty())
    }

    fn queue_error(&mut self, code: u8, message: String, slot_id: &str) {
        let payload = self.pack_with(&serde_json::json!({
            "slot_id": slot_id,
            "code": code,
            "message": message,
        }));
        self.pending.push((self.hash_error, payload));
    }

    fn pack_with<T: Serialize>(&mut self, v: &T) -> Vec<u8> {
        self.out_buf.clear();
        let _ = serde_json::to_writer(&mut self.out_buf, v);
        self.out_buf.clone()
    }
}

impl Plugin for ProvisionPlugin {
    fn execute(&mut self, command: PluginCommand, data: &[u8]) -> PluginResult {
        match command {
            CMD_UPSERT_NETWORK => self.handle_upsert(data),
            CMD_LIST_NETWORKS  => self.handle_list(),
            CMD_COMPOSE_OFFER  => self.handle_compose_offer(),
            _ => PluginResult::Error(PluginError::new(ERR_UNKNOWN_COMMAND, "unknown command")),
        }
    }
    fn name(&self) -> &str { "provision" }
    fn id(&self) -> PluginId { self.id }

    fn poll(&mut self) -> Option<(u32, &[u8])> {
        if self.pending.is_empty() { return None }
        let (hash, payload) = self.pending.remove(0);
        self.out_buf = payload;
        Some((hash, &self.out_buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, tempfile::TempDir, RosterCtx, ProvisionSlot, PathBuf) {
        let apiary = tempfile::tempdir().unwrap();
        let cfg = tempfile::tempdir().unwrap();
        let ctx: RosterCtx = Arc::new(Mutex::new(Some(apiary.path().to_path_buf())));
        let slot: ProvisionSlot = Arc::new(Mutex::new(None));
        let cfg_root = cfg.path().to_path_buf();
        (apiary, cfg, ctx, slot, cfg_root)
    }

    #[test]
    fn upsert_creates_and_replaces() {
        let (_a, _h, ctx, slot, cfg_root) = setup();
        let mut p = ProvisionPlugin::new(0, ctx.clone(), slot, cfg_root);
        let payload = serde_json::to_vec(&serde_json::json!({
            "name": "UoA-Lab",
            "ssid": "UoA-Lab",
            "psk": "hunter2hunter2",
            "is_default": true,
        })).unwrap();
        match p.execute(CMD_UPSERT_NETWORK, &payload) {
            PluginResult::Ok(_) => {}
            other => panic!("upsert failed: {other:?}"),
        }
        // Re-upsert same name with different psk; should replace.
        let payload2 = serde_json::to_vec(&serde_json::json!({
            "name": "UoA-Lab",
            "ssid": "UoA-Lab",
            "psk": "newpsk",
            "is_default": true,
        })).unwrap();
        p.execute(CMD_UPSERT_NETWORK, &payload2);
        let loaded = p.load_networks().unwrap();
        assert_eq!(loaded.networks.len(), 1);
        assert_eq!(loaded.networks[0].psk, "newpsk");
    }

    #[test]
    fn default_is_unique() {
        let (_a, _h, ctx, slot, cfg_root) = setup();
        let mut p = ProvisionPlugin::new(0, ctx, slot, cfg_root);
        for (name, dflt) in [("A", true), ("B", true)] {
            let payload = serde_json::to_vec(&serde_json::json!({
                "name": name, "ssid": name, "psk": "x", "is_default": dflt,
            })).unwrap();
            p.execute(CMD_UPSERT_NETWORK, &payload);
        }
        let loaded = p.load_networks().unwrap();
        let defaults: Vec<_> = loaded.networks.iter().filter(|n| n.is_default).collect();
        assert_eq!(defaults.len(), 1, "exactly one default at a time");
        assert_eq!(defaults[0].name, "B");
    }

    #[test]
    fn list_omits_psk() {
        let (_a, _h, ctx, slot, cfg_root) = setup();
        let mut p = ProvisionPlugin::new(0, ctx, slot, cfg_root);
        let payload = serde_json::to_vec(&serde_json::json!({
            "name": "lab", "ssid": "lab", "psk": "secret", "is_default": true,
        })).unwrap();
        p.execute(CMD_UPSERT_NETWORK, &payload);
        // Drain the upserted event so list is the next pending.
        let _ = p.poll();
        // Direct response is a small ack; the full listing rides on the
        // event (PluginResponse caps at 128 bytes).
        match p.execute(CMD_LIST_NETWORKS, &[]) {
            PluginResult::Ok(resp) => {
                let ack = std::str::from_utf8(resp.as_slice()).unwrap();
                assert!(ack.contains("\"count\":1"), "ack: {ack}");
            }
            _ => panic!(),
        }
        let (_hash, body) = p.poll().expect("listed event");
        let body_str = std::str::from_utf8(body).unwrap();
        assert!(!body_str.contains("secret"),
            "PSK MUST NOT appear in list response (PROCESS.md): {body_str}");
        assert!(body_str.contains("\"ssid\":\"lab\""));
    }

    #[test]
    fn compose_offer_packs_correctly() {
        let (_a, _h, ctx, slot, cfg_root) = setup();
        let mut p = ProvisionPlugin::new(0, ctx.clone(), slot.clone(), cfg_root);
        let payload = serde_json::to_vec(&serde_json::json!({
            "name": "lab", "ssid": "lab", "psk": "secret", "is_default": true,
        })).unwrap();
        p.execute(CMD_UPSERT_NETWORK, &payload);
        // Drain the upserted event so the next pending is offer_composed.
        let _ = p.poll();

        let cert = vec![0xAB; 144];
        *slot.lock().unwrap() = Some(ComposeOfferRequest {
            slot_id: "slot:x".into(),
            network_name: None,
            cert: cert.clone(),
        });
        p.execute(CMD_COMPOSE_OFFER, &[]);

        let expected_hash = p.hash_offer_composed;
        let (hash, body) = p.poll().expect("offer_composed event");
        assert_eq!(hash, expected_hash);
        let v: serde_json::Value = serde_json::from_slice(body).unwrap();
        let offer_hex = v["offer_hex"].as_str().unwrap();
        let offer = hex::decode(offer_hex).unwrap();
        assert_eq!(&offer[..4], b"R2WO");
        assert_eq!(offer[4], 0x01);
        let ssid_len = offer[5] as usize;
        assert_eq!(ssid_len, 3);
        assert_eq!(&offer[6..6 + ssid_len], b"lab");
        let psk_off = 6 + ssid_len;
        let psk_len = offer[psk_off] as usize;
        assert_eq!(psk_len, 6);
        assert_eq!(&offer[psk_off + 1..psk_off + 1 + psk_len], b"secret");
        let cert_len_off = psk_off + 1 + psk_len;
        let cert_len = u16::from_be_bytes(offer[cert_len_off..cert_len_off + 2].try_into().unwrap());
        assert_eq!(cert_len, 144);
        assert_eq!(&offer[cert_len_off + 2..], &cert[..]);

        // Outbound event MUST NOT contain the PSK.
        let body_str = std::str::from_utf8(body).unwrap();
        assert!(!body_str.contains("secret"), "PSK leaked in event payload");
    }

    #[test]
    fn compose_without_networks_errors() {
        let (_a, _h, ctx, slot, cfg_root) = setup();
        let mut p = ProvisionPlugin::new(0, ctx.clone(), slot.clone(), cfg_root);
        *slot.lock().unwrap() = Some(ComposeOfferRequest {
            slot_id: "slot:x".into(),
            network_name: None,
            cert: vec![0; 144],
        });
        let expected_hash = p.hash_error;
        p.execute(CMD_COMPOSE_OFFER, &[]);
        let (hash, body) = p.poll().expect("error event");
        assert_eq!(hash, expected_hash);
        let v: serde_json::Value = serde_json::from_slice(body).unwrap();
        assert_eq!(v["code"], ERR_NO_NETWORK);
    }

    #[test]
    fn bad_cert_length_rejected() {
        let (_a, _h, ctx, slot, cfg_root) = setup();
        let mut p = ProvisionPlugin::new(0, ctx, slot.clone(), cfg_root);
        *slot.lock().unwrap() = Some(ComposeOfferRequest {
            slot_id: "slot:x".into(),
            network_name: None,
            cert: vec![0; 100],
        });
        let expected_hash = p.hash_error;
        p.execute(CMD_COMPOSE_OFFER, &[]);
        let (hash, body) = p.poll().expect("error event");
        assert_eq!(hash, expected_hash);
        let v: serde_json::Value = serde_json::from_slice(body).unwrap();
        assert_eq!(v["code"], ERR_BAD_CERT);
    }
}
