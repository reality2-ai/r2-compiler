/* tslint:disable */
/* eslint-disable */

/**
 * Hash + hex-format in one call. Output shape: `"0x624c47bc"`
 * (lowercase, zero-padded, eight hex digits). Matches the spelling
 * used in catalogue TOML files.
 */
export function class_hash_hex(s: string): string;

/**
 * Compute the FNV-1a-32 hash of a UTF-8 string. Returns a `u32`.
 *
 * Used for R2-CAP class hashes and R2-FNV event-name hashes. The
 * browser's manifest viewer displays this alongside the class string
 * so operators can verify the pre-computed hash in `board.toml` /
 * `apiary.toml` matches.
 */
export function fnv1a_32(s: string): number;

/**
 * One-shot init called by JS on module load. Wires a panic hook
 * that routes Rust panics to `console.error` so the operator sees
 * something useful when WASM panics in production.
 */
export function on_load(): void;

/**
 * Verify that `s`'s FNV-1a-32 matches `expected_hex`. Returns `true`
 * on match. Accepts `expected_hex` in either `"0x624c47bc"` or
 * `"624c47bc"` form, case-insensitive.
 */
export function verify_class_hash(s: string, expected_hex: string): boolean;

/**
 * Returns this crate's semver. Useful for the webapp's "About" pane
 * to confirm which WASM bundle is loaded.
 */
export function version(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly class_hash_hex: (a: number, b: number, c: number) => void;
    readonly fnv1a_32: (a: number, b: number) => number;
    readonly on_load: () => void;
    readonly verify_class_hash: (a: number, b: number, c: number, d: number) => number;
    readonly version: (a: number) => void;
    readonly __wbindgen_export: (a: number) => void;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export2: (a: number, b: number) => number;
    readonly __wbindgen_export3: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_export4: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
