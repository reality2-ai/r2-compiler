//! Integration tests: exercise the plugin through its `Plugin` trait impl.

use r2_engine::plugin::{Plugin, PluginResult};
use r2_plugin_crypto_software_ed25519::{
    SoftwareEd25519, CMD_GENERATE, CMD_SIGN, CMD_VERIFY, ERR_BAD_LENGTH, ERR_UNKNOWN_COMMAND,
    VERIFY_BAD, VERIFY_OK,
};

fn ok_bytes(result: PluginResult) -> Vec<u8> {
    match result {
        PluginResult::Ok(r) => r.as_slice().to_vec(),
        PluginResult::Error(e) => panic!("expected Ok, got error: {}", e.description()),
    }
}

#[test]
fn generate_sign_verify_roundtrip() {
    let mut plugin = SoftwareEd25519::new(1);

    let seed = [42u8; 32];
    let keypair = ok_bytes(plugin.execute(CMD_GENERATE, &seed));
    assert_eq!(keypair.len(), 64, "generate returns pk || sk");
    let (pk, sk) = keypair.split_at(32);

    let message = b"the quick brown fox";
    let mut sign_input = Vec::new();
    sign_input.extend_from_slice(sk);
    sign_input.extend_from_slice(message);
    let signature = ok_bytes(plugin.execute(CMD_SIGN, &sign_input));
    assert_eq!(signature.len(), 64, "signature is 64 bytes");

    let mut verify_input = Vec::new();
    verify_input.extend_from_slice(pk);
    verify_input.extend_from_slice(&signature);
    verify_input.extend_from_slice(message);
    let result = ok_bytes(plugin.execute(CMD_VERIFY, &verify_input));
    assert_eq!(result, vec![VERIFY_OK], "signature should verify");

    let tampered = b"the quick brown cat";
    let mut bad_input = Vec::new();
    bad_input.extend_from_slice(pk);
    bad_input.extend_from_slice(&signature);
    bad_input.extend_from_slice(tampered);
    let bad_result = ok_bytes(plugin.execute(CMD_VERIFY, &bad_input));
    assert_eq!(bad_result, vec![VERIFY_BAD], "tampered message should fail");
}

#[test]
fn generate_is_deterministic_for_same_seed() {
    let mut plugin = SoftwareEd25519::new(1);
    let seed = [7u8; 32];
    let kp1 = ok_bytes(plugin.execute(CMD_GENERATE, &seed));
    let kp2 = ok_bytes(plugin.execute(CMD_GENERATE, &seed));
    assert_eq!(kp1, kp2, "same seed must give same keypair");
}

#[test]
fn sign_is_deterministic() {
    let mut plugin = SoftwareEd25519::new(1);
    let seed = [9u8; 32];
    let keypair = ok_bytes(plugin.execute(CMD_GENERATE, &seed));
    let sk = &keypair[32..];
    let message = b"hello R2";

    let mut input = Vec::new();
    input.extend_from_slice(sk);
    input.extend_from_slice(message);
    let sig1 = ok_bytes(plugin.execute(CMD_SIGN, &input));
    let sig2 = ok_bytes(plugin.execute(CMD_SIGN, &input));
    assert_eq!(sig1, sig2, "Ed25519 signatures are deterministic");
}

#[test]
fn unknown_command_returns_error() {
    let mut plugin = SoftwareEd25519::new(1);
    match plugin.execute(0xFF, &[]) {
        PluginResult::Error(e) => assert_eq!(e.code, ERR_UNKNOWN_COMMAND),
        PluginResult::Ok(_) => panic!("expected error for unknown command"),
    }
}

#[test]
fn short_sign_input_returns_bad_length() {
    let mut plugin = SoftwareEd25519::new(1);
    match plugin.execute(CMD_SIGN, &[0u8; 10]) {
        PluginResult::Error(e) => assert_eq!(e.code, ERR_BAD_LENGTH),
        PluginResult::Ok(_) => panic!("expected bad-length error"),
    }
}

#[test]
fn short_verify_input_returns_bad_length() {
    let mut plugin = SoftwareEd25519::new(1);
    match plugin.execute(CMD_VERIFY, &[0u8; 50]) {
        PluginResult::Error(e) => assert_eq!(e.code, ERR_BAD_LENGTH),
        PluginResult::Ok(_) => panic!("expected bad-length error"),
    }
}

#[test]
fn wrong_generate_length_returns_bad_length() {
    let mut plugin = SoftwareEd25519::new(1);
    match plugin.execute(CMD_GENERATE, &[0u8; 31]) {
        PluginResult::Error(e) => assert_eq!(e.code, ERR_BAD_LENGTH),
        PluginResult::Ok(_) => panic!("expected bad-length error"),
    }
}

#[test]
fn plugin_metadata_is_stable() {
    let plugin = SoftwareEd25519::new(7);
    assert_eq!(plugin.name(), "crypto/software-ed25519");
    assert_eq!(plugin.id(), 7);
}
