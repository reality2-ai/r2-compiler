/* @ts-self-types="./r2_wasm.d.ts" */

/**
 * The R2 Hive running in the browser.
 *
 * Contains the EventBus with the Notekeeper sentant and sync plugin.
 * JavaScript interacts with it by sending events and polling for
 * outbound actions.
 */
export class R2Hive {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        R2HiveFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_r2hive_free(ptr, 0);
    }
    /**
     * Drain all outbound events (events the sentant wants to send externally).
     *
     * Returns a JSON array of events: [{"hash":N,"payload":"hex"}, ...]
     * JavaScript processes these (encrypt + relay, UI update, etc).
     * @returns {string}
     */
    drain_outbound() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.r2hive_drain_outbound(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Create a new hive with the Notekeeper sentant and sync plugin.
     */
    constructor() {
        const ret = wasm.r2hive_new();
        this.__wbg_ptr = ret >>> 0;
        R2HiveFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Push incoming sync data from another device (already decrypted).
     *
     * `payload` is the CBOR-encoded R2-PLUGIN result envelope.
     * @param {Uint8Array} payload
     */
    push_sync_inbound(payload) {
        const ptr0 = passArray8ToWasm0(payload, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.r2hive_push_sync_inbound(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * Send an event to the sentant.
     *
     * `event_hash` is the FNV-1a hash of the event name.
     * `payload` is CBOR-encoded event parameters.
     *
     * After calling this, check `drain_outbound()` for events the
     * sentant wants to send (sync, notifications, etc).
     * @param {number} event_hash
     * @param {Uint8Array} payload
     */
    send_event(event_hash, payload) {
        const ptr0 = passArray8ToWasm0(payload, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.r2hive_send_event(this.__wbg_ptr, event_hash, ptr0, len0);
    }
    /**
     * Process one tick of the engine.
     */
    tick() {
        wasm.r2hive_tick(this.__wbg_ptr);
    }
}
if (Symbol.dispose) R2Hive.prototype[Symbol.dispose] = R2Hive.prototype.free;

/**
 * Opaque handle to a MemberState (device/joiner side).
 */
export class R2Member {
    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(R2Member.prototype);
        obj.__wbg_ptr = ptr;
        R2MemberFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        R2MemberFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_r2member_free(ptr, 0);
    }
    /**
     * DEK for encryption (32 bytes).
     * @returns {Uint8Array}
     */
    get dek() {
        const ret = wasm.r2member_dek(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Restore member state from previously serialized bytes.
     *
     * Use this on page load to restore trust group membership from localStorage.
     * @param {Uint8Array} bytes
     * @returns {R2Member}
     */
    static from_bytes(bytes) {
        const ptr0 = passArray8ToWasm0(bytes, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.r2member_from_bytes(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return R2Member.__wrap(ret[0]);
    }
    /**
     * HK for HMAC operations (32 bytes).
     * @returns {Uint8Array}
     */
    get hk() {
        const ret = wasm.r2member_hk(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Check if the membership certificate is valid at the given time.
     * @param {bigint} now
     * @returns {boolean}
     */
    is_valid(now) {
        const ret = wasm.r2member_is_valid(this.__wbg_ptr, now);
        return ret !== 0;
    }
    /**
     * Device's public key (32 bytes).
     * @returns {Uint8Array}
     */
    get public_key() {
        const ret = wasm.r2member_public_key(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Sign a relay HELLO message and return the complete JSON string.
     *
     * Produces: `{"type":"hello","version":1,"trust_group":"...","device_id":"...","timestamp":N,"signature":"..."}`
     *
     * The signature is Ed25519 over `"{trust_group}:{device_id}:{timestamp}"`.
     * @param {bigint} timestamp
     * @returns {string}
     */
    sign_relay_hello(timestamp) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ret = wasm.r2member_sign_relay_hello(this.__wbg_ptr, timestamp);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * Serialize member state to bytes for persistent storage.
     *
     * Returns 277 bytes containing device key, certificate, DEK, HK.
     * **These bytes contain secret key material — encrypt before storing.**
     * @returns {Uint8Array}
     */
    to_bytes() {
        const ret = wasm.r2member_to_bytes(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Trust group hash for relay HELLO (first 8 bytes of SHA-256 of TG_PK, as 16 hex chars).
     * @returns {string}
     */
    trust_group_hash() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.r2member_trust_group_hash(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Trust group public key (32 bytes).
     * @returns {Uint8Array}
     */
    get trust_group_id() {
        const ret = wasm.r2member_trust_group_id(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
}
if (Symbol.dispose) R2Member.prototype[Symbol.dispose] = R2Member.prototype.free;

/**
 * Opaque handle to a TrustGroup (key holder side).
 * Stored in WASM memory; JS holds the index.
 */
export class R2TrustGroup {
    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(R2TrustGroup.prototype);
        obj.__wbg_ptr = ptr;
        R2TrustGroupFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        R2TrustGroupFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_r2trustgroup_free(ptr, 0);
    }
    /**
     * DEK (data encryption key), 32 bytes.
     * @returns {Uint8Array}
     */
    get dek() {
        const ret = wasm.r2trustgroup_dek(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Restore key holder state from previously serialized bytes.
     *
     * Restores the signing key and derived keys. Member list starts empty —
     * members rejoin via the normal join protocol or are restored separately.
     * @param {Uint8Array} bytes
     * @param {bigint} now
     * @returns {R2TrustGroup}
     */
    static from_bytes(bytes, now) {
        const ptr0 = passArray8ToWasm0(bytes, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.r2trustgroup_from_bytes(ptr0, len0, now);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return R2TrustGroup.__wrap(ret[0]);
    }
    /**
     * Generate a join code. Returns the 16-byte code as hex string.
     *
     * `now` is current Unix timestamp, `ttl_secs` is validity duration.
     * @param {bigint} now
     * @param {bigint} ttl_secs
     * @returns {string}
     */
    generate_join_code(now, ttl_secs) {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.r2trustgroup_generate_join_code(this.__wbg_ptr, now, ttl_secs);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * HK (HMAC key), 32 bytes.
     * @returns {Uint8Array}
     */
    get hk() {
        const ret = wasm.r2trustgroup_hk(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Number of members (excluding key holder).
     * @returns {number}
     */
    get member_count() {
        const ret = wasm.r2trustgroup_member_count(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * List members as JSON array of {name, public_key_hex} objects.
     * @returns {any}
     */
    member_list() {
        const ret = wasm.r2trustgroup_member_list(this.__wbg_ptr);
        return ret;
    }
    /**
     * List member names as a JSON array.
     * @returns {any}
     */
    member_names() {
        const ret = wasm.r2trustgroup_member_names(this.__wbg_ptr);
        return ret;
    }
    /**
     * Create a new trust group. Returns the key holder's trust group handle.
     *
     * `now` is the current Unix timestamp in seconds.
     * @param {bigint} now
     */
    constructor(now) {
        const ret = wasm.r2trustgroup_new(now);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        R2TrustGroupFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Process a join request from a device.
     *
     * - `join_code_hex`: the 16-byte join code as hex string
     * - `device_public_key`: the joiner's Ed25519 public key (32 bytes)
     * - `device_name`: human-readable name for the device
     * - `now`: current Unix timestamp
     *
     * Returns the encrypted join response as bytes (to send to the joiner).
     * @param {string} join_code_hex
     * @param {Uint8Array} device_public_key
     * @param {string} device_name
     * @param {bigint} now
     * @returns {Uint8Array}
     */
    process_join(join_code_hex, device_public_key, device_name, now) {
        const ptr0 = passStringToWasm0(join_code_hex, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(device_public_key, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(device_name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.r2trustgroup_process_join(this.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2, now);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v4 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v4;
    }
    /**
     * Trust group public key (32 bytes). This is the trust group ID.
     * @returns {Uint8Array}
     */
    get public_key() {
        const ret = wasm.r2trustgroup_public_key(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Revoke a member by their public key hex string. Key holder only.
     * @param {string} public_key_hex
     * @param {bigint} now
     */
    revoke_member(public_key_hex, now) {
        const ptr0 = passStringToWasm0(public_key_hex, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.r2trustgroup_revoke_member(this.__wbg_ptr, ptr0, len0, now);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * Serialize key holder state to bytes for persistent storage.
     *
     * Returns 38 bytes (signing key + sequence + crypto level).
     * **Contains TG_SK — the root secret. Encrypt before storing.**
     * @returns {Uint8Array}
     */
    to_bytes() {
        const ret = wasm.r2trustgroup_to_bytes(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
}
if (Symbol.dispose) R2TrustGroup.prototype[Symbol.dispose] = R2TrustGroup.prototype.free;

/**
 * Decode a note event CBOR payload: {0: opCode, 1: noteId, 2: timestamp, 3?: encryptedContent}.
 *
 * Returns a JS object with `op_code`, `note_id`, `timestamp`, and optionally `encrypted_content` (Uint8Array).
 * @param {Uint8Array} payload
 * @returns {any}
 */
export function cbor_decode_note_event(payload) {
    const ptr0 = passArray8ToWasm0(payload, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.cbor_decode_note_event(ptr0, len0);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * Encode a simple CBOR map: { key0: val0, key1: val1, ... }
 *
 * Takes parallel arrays of integer keys and integer values.
 * Returns CBOR-encoded bytes (compact mode).
 * @param {Uint8Array} keys
 * @param {Uint32Array} values
 * @returns {Uint8Array}
 */
export function cbor_encode_int_map(keys, values) {
    const ptr0 = passArray8ToWasm0(keys, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray32ToWasm0(values, wasm.__wbindgen_malloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.cbor_encode_int_map(ptr0, len0, ptr1, len1);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    var v3 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v3;
}

/**
 * Encode a note event CBOR payload: {0: opCode, 1: noteId, 2: timestamp, 3?: encryptedContent}.
 *
 * Key 3 (encrypted content) is only included if `encrypted_content` is non-empty.
 * This packs both metadata and content into a single R2-WIRE frame payload.
 * @param {number} op_code
 * @param {number} note_id
 * @param {number} timestamp
 * @param {Uint8Array} encrypted_content
 * @returns {Uint8Array}
 */
export function cbor_encode_note_event(op_code, note_id, timestamp, encrypted_content) {
    const ptr0 = passArray8ToWasm0(encrypted_content, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.cbor_encode_note_event(op_code, note_id, timestamp, ptr0, len0);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v2;
}

/**
 * Complete the join handshake (device side).
 *
 * - `device_secret_key`: the device's Ed25519 secret key (32 bytes)
 * - `trust_group_public_key`: the trust group's public key (32 bytes)
 * - `encrypted_response`: the encrypted join response bytes from the key holder
 * - `now`: current Unix timestamp
 *
 * Returns an R2Member handle on success.
 * @param {Uint8Array} device_secret_key
 * @param {Uint8Array} trust_group_public_key
 * @param {Uint8Array} encrypted_response
 * @param {bigint} now
 * @returns {R2Member}
 */
export function complete_join(device_secret_key, trust_group_public_key, encrypted_response, now) {
    const ptr0 = passArray8ToWasm0(device_secret_key, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray8ToWasm0(trust_group_public_key, wasm.__wbindgen_malloc);
    const len1 = WASM_VECTOR_LEN;
    const ptr2 = passArray8ToWasm0(encrypted_response, wasm.__wbindgen_malloc);
    const len2 = WASM_VECTOR_LEN;
    const ret = wasm.complete_join(ptr0, len0, ptr1, len1, ptr2, len2, now);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return R2Member.__wrap(ret[0]);
}

/**
 * Compute trust group hash from a public key (first 8 bytes of SHA-256, as 16 hex chars).
 * @param {Uint8Array} tg_public_key
 * @returns {string}
 */
export function compute_trust_group_hash(tg_public_key) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passArray8ToWasm0(tg_public_key, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.compute_trust_group_hash(ptr0, len0);
        var ptr2 = ret[0];
        var len2 = ret[1];
        if (ret[3]) {
            ptr2 = 0; len2 = 0;
            throw takeFromExternrefTable0(ret[2]);
        }
        deferred3_0 = ptr2;
        deferred3_1 = len2;
        return getStringFromWasm0(ptr2, len2);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
}

/**
 * Decode a compact R2-WIRE frame.
 *
 * Returns a JS object with header fields and payload.
 * @param {Uint8Array} data
 * @returns {any}
 */
export function decode_compact_frame(data) {
    const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.decode_compact_frame(ptr0, len0);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * Decode an extended R2-WIRE frame.
 * @param {Uint8Array} data
 * @returns {any}
 */
export function decode_extended_frame(data) {
    const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.decode_extended_frame(ptr0, len0);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * Decode an invite string → { tg_public_key: [32], join_code_hex: string, trust_group_hash: string }
 * @param {string} invite
 * @returns {any}
 */
export function decode_invite(invite) {
    const ptr0 = passStringToWasm0(invite, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.decode_invite(ptr0, len0);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * Decode 3 words back to trust group prefix (3 hex chars) + join code fragment.
 *
 * Input: "word1-word2-word3" or "word1 word2 word3"
 * Returns JS object: { tg_prefix_hex: "abc", join_secret_hex: "123456" }
 * @param {string} words
 * @returns {any}
 */
export function decode_word_code(words) {
    const ptr0 = passStringToWasm0(words, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.decode_word_code(ptr0, len0);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * Decrypt data with the trust group DEK (XChaCha20-Poly1305).
 *
 * Input: [nonce: 24 bytes] [ciphertext + auth tag] (as produced by encrypt_with_dek).
 * Returns the plaintext, or throws if decryption/authentication fails.
 * @param {Uint8Array} dek
 * @param {Uint8Array} encrypted
 * @returns {Uint8Array}
 */
export function decrypt_with_dek(dek, encrypted) {
    const ptr0 = passArray8ToWasm0(dek, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray8ToWasm0(encrypted, wasm.__wbindgen_malloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.decrypt_with_dek(ptr0, len0, ptr1, len1);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    var v3 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v3;
}

/**
 * Derive trust group keys (DEK + HK) from raw secret and public key bytes.
 *
 * Both `tg_secret` and `tg_public` must be 32 bytes (Ed25519 key material).
 * Returns a JS object with `dek` and `hk` as byte arrays.
 * @param {Uint8Array} tg_secret
 * @param {Uint8Array} tg_public
 * @returns {any}
 */
export function derive_group_keys(tg_secret, tg_public) {
    const ptr0 = passArray8ToWasm0(tg_secret, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray8ToWasm0(tg_public, wasm.__wbindgen_malloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.derive_group_keys(ptr0, len0, ptr1, len1);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * Encode a compact R2-WIRE frame.
 *
 * Parameters:
 * - `msg_type`: 0=Event, 2=Reply, 3=Ack, 4=Nack, 5=Heartbeat
 * - `ttl`: time-to-live (0-15)
 * - `k`: relay budget (0-15)
 * - `msg_id`: 16-bit message ID
 * - `event_hash`: 32-bit FNV-1a hash of event name
 * - `target`: 32-bit target hive address (0 = broadcast)
 * - `payload`: CBOR-encoded payload bytes
 *
 * Returns the encoded frame bytes.
 * @param {number} msg_type
 * @param {number} ttl
 * @param {number} k
 * @param {number} msg_id
 * @param {number} event_hash
 * @param {number} target
 * @param {Uint8Array} payload
 * @returns {Uint8Array}
 */
export function encode_compact_frame(msg_type, ttl, k, msg_id, event_hash, target, payload) {
    const ptr0 = passArray8ToWasm0(payload, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.encode_compact_frame(msg_type, ttl, k, msg_id, event_hash, target, ptr0, len0);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v2;
}

/**
 * Encode an extended R2-WIRE frame.
 * @param {number} msg_type
 * @param {number} ttl
 * @param {number} k
 * @param {number} msg_id
 * @param {number} event_hash
 * @param {number} target_group
 * @param {number} target_hive
 * @param {Uint8Array} payload
 * @returns {Uint8Array}
 */
export function encode_extended_frame(msg_type, ttl, k, msg_id, event_hash, target_group, target_hive, payload) {
    const ptr0 = passArray8ToWasm0(payload, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.encode_extended_frame(msg_type, ttl, k, msg_id, event_hash, target_group, target_hive, ptr0, len0);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v2;
}

/**
 * Encode an invite: TG_PK (32 bytes) + join_code (16 bytes) → base64url string.
 *
 * The invite contains everything a joiner needs to connect and join:
 * the trust group public key (to compute hash and decrypt response)
 * and the join code secret.
 * @param {Uint8Array} tg_public_key
 * @param {string} join_code_hex
 * @returns {string}
 */
export function encode_invite(tg_public_key, join_code_hex) {
    let deferred4_0;
    let deferred4_1;
    try {
        const ptr0 = passArray8ToWasm0(tg_public_key, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(join_code_hex, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.encode_invite(ptr0, len0, ptr1, len1);
        var ptr3 = ret[0];
        var len3 = ret[1];
        if (ret[3]) {
            ptr3 = 0; len3 = 0;
            throw takeFromExternrefTable0(ret[2]);
        }
        deferred4_0 = ptr3;
        deferred4_1 = len3;
        return getStringFromWasm0(ptr3, len3);
    } finally {
        wasm.__wbindgen_free(deferred4_0, deferred4_1, 1);
    }
}

/**
 * Encode a note ID-only payload as CBOR.
 *
 * {0: id, 3: timestamp}
 * Used for delete events.
 * @param {string} id
 * @param {bigint} timestamp
 * @returns {Uint8Array}
 */
export function encode_note_id_payload(id, timestamp) {
    const ptr0 = passStringToWasm0(id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.encode_note_id_payload(ptr0, len0, timestamp);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v2;
}

/**
 * Encode a note event payload as CBOR.
 *
 * {0: id, 1: title, 2: content, 3: timestamp}
 * Used by JavaScript to build payloads for hive.send_event().
 * @param {string} id
 * @param {string} title
 * @param {string} content
 * @param {bigint} timestamp
 * @returns {Uint8Array}
 */
export function encode_note_payload(id, title, content, timestamp) {
    const ptr0 = passStringToWasm0(id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(title, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ptr2 = passStringToWasm0(content, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len2 = WASM_VECTOR_LEN;
    const ret = wasm.encode_note_payload(ptr0, len0, ptr1, len1, ptr2, len2, timestamp);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    var v4 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v4;
}

/**
 * Encode a trust group hash + join code as 3 words.
 *
 * `tg_hash_hex`: 16-char hex trust group hash
 * `join_code_hex`: 32-char hex join code
 * Returns: "word1-word2-word3"
 * @param {string} tg_hash_hex
 * @param {string} join_code_hex
 * @returns {string}
 */
export function encode_word_code(tg_hash_hex, join_code_hex) {
    let deferred4_0;
    let deferred4_1;
    try {
        const ptr0 = passStringToWasm0(tg_hash_hex, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(join_code_hex, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.encode_word_code(ptr0, len0, ptr1, len1);
        var ptr3 = ret[0];
        var len3 = ret[1];
        if (ret[3]) {
            ptr3 = 0; len3 = 0;
            throw takeFromExternrefTable0(ret[2]);
        }
        deferred4_0 = ptr3;
        deferred4_1 = len3;
        return getStringFromWasm0(ptr3, len3);
    } finally {
        wasm.__wbindgen_free(deferred4_0, deferred4_1, 1);
    }
}

/**
 * Encrypt data with the trust group DEK (XChaCha20-Poly1305).
 *
 * Returns: [nonce: 24 bytes] [ciphertext + auth tag]
 * Used for plugin-to-plugin data exchange (note content, files, etc.)
 * The relay sees only ciphertext.
 * @param {Uint8Array} dek
 * @param {Uint8Array} plaintext
 * @returns {Uint8Array}
 */
export function encrypt_with_dek(dek, plaintext) {
    const ptr0 = passArray8ToWasm0(dek, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray8ToWasm0(plaintext, wasm.__wbindgen_malloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.encrypt_with_dek(ptr0, len0, ptr1, len1);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    var v3 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v3;
}

/**
 * Raw FNV-1a 32-bit hash of pre-canonicalised bytes.
 * @param {Uint8Array} data
 * @returns {number}
 */
export function fnv1a_32(data) {
    const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.fnv1a_32(ptr0, len0);
    return ret >>> 0;
}

/**
 * Wrap a frame with a 4-byte big-endian length prefix (TCP framing).
 * @param {Uint8Array} frame
 * @returns {Uint8Array}
 */
export function frame_with_be_prefix(frame) {
    const ptr0 = passArray8ToWasm0(frame, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.frame_with_be_prefix(ptr0, len0);
    var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v2;
}

/**
 * Wrap a frame with a 2-byte little-endian length prefix (BLE/USB framing).
 * @param {Uint8Array} frame
 * @returns {Uint8Array}
 */
export function frame_with_le_prefix(frame) {
    const ptr0 = passArray8ToWasm0(frame, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.frame_with_le_prefix(ptr0, len0);
    var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v2;
}

/**
 * Generate a new Ed25519 device keypair.
 *
 * Returns a JS object with `secret_key` (32 bytes) and `public_key` (32 bytes).
 * @returns {any}
 */
export function generate_device_keypair() {
    const ret = wasm.generate_device_keypair();
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * Compute an HMAC tag for a compact frame.
 *
 * `hk` must be 32 bytes. Returns the 8-byte truncated HMAC tag.
 * The caller is responsible for appending the tag to the frame and setting
 * the has_hmac flag.
 * @param {Uint8Array} frame_bytes
 * @param {Uint8Array} hk
 * @returns {Uint8Array}
 */
export function hmac_compact_tag(frame_bytes, hk) {
    const ptr0 = passArray8ToWasm0(frame_bytes, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray8ToWasm0(hk, wasm.__wbindgen_malloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.hmac_compact_tag(ptr0, len0, ptr1, len1);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    var v3 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v3;
}

/**
 * Compute the HMAC tag for an extended R2-WIRE frame.
 *
 * `frame_bytes` must be a valid extended R2-WIRE frame (without HMAC).
 * `hk` must be 32 bytes (the trust group's HMAC key).
 * Returns the 32-byte HMAC-SHA256 tag.
 * @param {Uint8Array} frame_bytes
 * @param {Uint8Array} hk
 * @returns {Uint8Array}
 */
export function hmac_extended_tag(frame_bytes, hk) {
    const ptr0 = passArray8ToWasm0(frame_bytes, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray8ToWasm0(hk, wasm.__wbindgen_malloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.hmac_extended_tag(ptr0, len0, ptr1, len1);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    var v3 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v3;
}

/**
 * Hash an event name to a 32-bit FNV-1a identifier.
 *
 * Canonicalises the input (lowercase, whitespace-stripped) before hashing.
 * Returns the hash, or throws on empty/reserved names.
 * @param {string} event_name
 * @returns {number}
 */
export function r2_hash(event_name) {
    const ptr0 = passStringToWasm0(event_name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.r2_hash(ptr0, len0);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return ret[0] >>> 0;
}

/**
 * Returns the r2-wasm version string.
 * @returns {string}
 */
export function r2_version() {
    let deferred1_0;
    let deferred1_1;
    try {
        const ret = wasm.r2_version();
        deferred1_0 = ret[0];
        deferred1_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
    }
}

/**
 * Sign a message with an Ed25519 secret key.
 *
 * Used for relay HELLO signing when joining (before we have a full R2Member).
 * `secret_key` must be 32 bytes. Returns 64-byte Ed25519 signature.
 * @param {Uint8Array} secret_key
 * @param {Uint8Array} message
 * @returns {Uint8Array}
 */
export function sign_ed25519(secret_key, message) {
    const ptr0 = passArray8ToWasm0(secret_key, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray8ToWasm0(message, wasm.__wbindgen_malloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.sign_ed25519(ptr0, len0, ptr1, len1);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    var v3 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v3;
}

/**
 * Transcode an extended frame to compact format.
 * @param {Uint8Array} extended_bytes
 * @returns {Uint8Array}
 */
export function transcode_to_compact(extended_bytes) {
    const ptr0 = passArray8ToWasm0(extended_bytes, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.transcode_to_compact(ptr0, len0);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v2;
}

/**
 * Transcode a compact frame to extended format.
 * @param {Uint8Array} compact_bytes
 * @returns {Uint8Array}
 */
export function transcode_to_extended(compact_bytes) {
    const ptr0 = passArray8ToWasm0(compact_bytes, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.transcode_to_extended(ptr0, len0);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v2;
}

/**
 * Verify a compact frame's HMAC tag.
 *
 * The frame must include the HMAC tag (has_hmac flag set).
 * `hk` must be 32 bytes. Returns true if valid.
 * @param {Uint8Array} signed_frame
 * @param {Uint8Array} hk
 * @returns {boolean}
 */
export function verify_compact_hmac(signed_frame, hk) {
    const ptr0 = passArray8ToWasm0(signed_frame, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray8ToWasm0(hk, wasm.__wbindgen_malloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.verify_compact_hmac(ptr0, len0, ptr1, len1);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return ret[0] !== 0;
}

/**
 * Verify an extended frame's HMAC tag.
 *
 * The frame must include the HMAC tag (has_hmac flag set).
 * `hk` must be 32 bytes. Returns true if valid.
 * @param {Uint8Array} signed_frame
 * @param {Uint8Array} hk
 * @returns {boolean}
 */
export function verify_extended_hmac(signed_frame, hk) {
    const ptr0 = passArray8ToWasm0(signed_frame, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray8ToWasm0(hk, wasm.__wbindgen_malloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.verify_extended_hmac(ptr0, len0, ptr1, len1);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return ret[0] !== 0;
}

function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_ecbf49c1b9d07c30: function(arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return ret;
        },
        __wbg_String_8564e559799eccda: function(arg0, arg1) {
            const ret = String(arg1);
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_is_function_18bea6e84080c016: function(arg0) {
            const ret = typeof(arg0) === 'function';
            return ret;
        },
        __wbg___wbindgen_is_object_8d3fac158b36498d: function(arg0) {
            const val = arg0;
            const ret = typeof(val) === 'object' && val !== null;
            return ret;
        },
        __wbg___wbindgen_is_string_4d5f2c5b2acf65b0: function(arg0) {
            const ret = typeof(arg0) === 'string';
            return ret;
        },
        __wbg___wbindgen_is_undefined_4a711ea9d2e1ef93: function(arg0) {
            const ret = arg0 === undefined;
            return ret;
        },
        __wbg___wbindgen_throw_df03e93053e0f4bc: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_call_85e5437fa1ab109d: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.call(arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_crypto_38df2bab126b63dc: function(arg0) {
            const ret = arg0.crypto;
            return ret;
        },
        __wbg_getRandomValues_c44a50d8cfdaebeb: function() { return handleError(function (arg0, arg1) {
            arg0.getRandomValues(arg1);
        }, arguments); },
        __wbg_length_5e07cf181b2745fb: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_msCrypto_bd5a034af96bcba6: function(arg0) {
            const ret = arg0.msCrypto;
            return ret;
        },
        __wbg_new_62f131e968c83d75: function() {
            const ret = new Object();
            return ret;
        },
        __wbg_new_66075f8c2ea6575e: function() {
            const ret = new Array();
            return ret;
        },
        __wbg_new_with_length_9b57e4a9683723fa: function(arg0) {
            const ret = new Uint8Array(arg0 >>> 0);
            return ret;
        },
        __wbg_node_84ea875411254db1: function(arg0) {
            const ret = arg0.node;
            return ret;
        },
        __wbg_process_44c7a14e11e9f69e: function(arg0) {
            const ret = arg0.process;
            return ret;
        },
        __wbg_prototypesetcall_d1a7133bc8d83aa9: function(arg0, arg1, arg2) {
            Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), arg2);
        },
        __wbg_randomFillSync_6c25eac9869eb53c: function() { return handleError(function (arg0, arg1) {
            arg0.randomFillSync(arg1);
        }, arguments); },
        __wbg_require_b4edbdcf3e2a1ef0: function() { return handleError(function () {
            const ret = module.require;
            return ret;
        }, arguments); },
        __wbg_set_6be42768c690e380: function(arg0, arg1, arg2) {
            arg0[arg1] = arg2;
        },
        __wbg_set_7bf9e2df46e7632c: function(arg0, arg1, arg2) {
            arg0[arg1 >>> 0] = arg2;
        },
        __wbg_static_accessor_GLOBAL_THIS_6614f2f4998e3c4c: function() {
            const ret = typeof globalThis === 'undefined' ? null : globalThis;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_GLOBAL_d8e8a2fefe80bc1d: function() {
            const ret = typeof global === 'undefined' ? null : global;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_SELF_e29eaf7c465526b1: function() {
            const ret = typeof self === 'undefined' ? null : self;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_WINDOW_66e7ca3eef30585a: function() {
            const ret = typeof window === 'undefined' ? null : window;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_subarray_f36da54ffa7114f5: function(arg0, arg1, arg2) {
            const ret = arg0.subarray(arg1 >>> 0, arg2 >>> 0);
            return ret;
        },
        __wbg_versions_276b2795b1c6a219: function(arg0) {
            const ret = arg0.versions;
            return ret;
        },
        __wbindgen_cast_0000000000000001: function(arg0) {
            // Cast intrinsic for `F64 -> Externref`.
            const ret = arg0;
            return ret;
        },
        __wbindgen_cast_0000000000000002: function(arg0, arg1) {
            // Cast intrinsic for `Ref(Slice(U8)) -> NamedExternref("Uint8Array")`.
            const ret = getArrayU8FromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_cast_0000000000000003: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./r2_wasm_bg.js": import0,
    };
}

const R2HiveFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_r2hive_free(ptr >>> 0, 1));
const R2MemberFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_r2member_free(ptr >>> 0, 1));
const R2TrustGroupFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_r2trustgroup_free(ptr >>> 0, 1));

function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

function getStringFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return decodeText(ptr, len);
}

let cachedUint32ArrayMemory0 = null;
function getUint32ArrayMemory0() {
    if (cachedUint32ArrayMemory0 === null || cachedUint32ArrayMemory0.byteLength === 0) {
        cachedUint32ArrayMemory0 = new Uint32Array(wasm.memory.buffer);
    }
    return cachedUint32ArrayMemory0;
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        const idx = addToExternrefTable0(e);
        wasm.__wbindgen_exn_store(idx);
    }
}

function isLikeNone(x) {
    return x === undefined || x === null;
}

function passArray32ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 4, 4) >>> 0;
    getUint32ArrayMemory0().set(arg, ptr / 4);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasm;
function __wbg_finalize_init(instance, module) {
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedUint32ArrayMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('r2_wasm_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
