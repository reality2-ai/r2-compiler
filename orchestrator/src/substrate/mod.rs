//! R2 **substrate** components — per R2-HIVE §1.4 + §2.1, the
//! device-scoped, always-running, TG-agnostic core of a hive. These
//! implement R2 protocol roles defined in the r2-specifications repo
//! (R2-TRUST, R2-PROVISION, R2-BEACON, …) and would exist in any
//! sufficiently-capable R2 hive — not just r2-composer.
//!
//! Distinct from [[crate::composer]] which holds r2-composer-specific
//! authoring scaffolding (subprocess wrappers, sysfs watchers, AI
//! orchestration). Both implement `r2_engine::Plugin` at the Rust
//! trait level, but the R2-PLUGIN spec word "plugin" is reserved for
//! L7 user-domain capability providers in the catalogue — neither
//! `substrate/` nor `composer/` are plugins in that sense.
//!
//! ## Layer placement (R2-INTRO §"The Protocol Stack")
//!
//! - `keyholder`    — L5 Trust & Identity (R2-TRUST §5.5)
//! - `provision`    — L5 Trust & Identity / L1-L2 BLE bootstrap (R2-PROVISION §3, §5; R2-WIFI §3.4)
//! - `beacon_observer` (F4) — L2 Discovery (R2-BEACON §5-7)
//! - `ota_push`        (F5) — L6 Management (R2-UPDATE §3.1.2.2)

pub mod beacon_observer;
pub mod beacon_parser;
pub mod keyholder;
pub mod ota_push;
pub mod provision;
pub mod provision_handshake;
pub mod tg_state;

pub use beacon_observer::{BeaconObservation, BeaconObserverPlugin, BeaconSnapshot};
pub use keyholder::{KeyholderPlugin, KeyholderSlot, SignCertRequest};
pub use ota_push::{OtaPushParams, OtaPushPlugin, OtaPushSlot};
pub use provision::{ComposeOfferRequest, ProvisionPlugin, ProvisionSlot};
pub use provision_handshake::{
    HandshakeRequest, ProvisionHandshakePlugin, ProvisionHandshakeSlot,
};
