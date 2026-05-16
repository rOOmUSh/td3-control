// Synchronous accessor for the inlined runtime config snapshot.
//
// The Rust web server replaces `<!-- TD3_CONFIG_INJECT -->` in every
// served HTML page with `<script>window.TD3_CONFIG_ENV = {...};</script>`,
// so by the time any module evaluates, `window.TD3_CONFIG_ENV` already
// holds the merged TD3_CONFIG.env values (template defaults + user
// overrides).
//
// This module exposes typed accessors so state modules don't sprinkle
// `window.TD3_CONFIG_ENV.uiDefaultBpm ?? 120` literals across the
// codebase. The "no hardcoded values" rule applies here: when a key is
// missing we return `undefined` and let the call site decide - there
// are no built-in JS-side defaults.
//
// Tests that import state modules in Node must define
// `globalThis.window = { TD3_CONFIG_ENV: {...} }` before importing.

const ENV = (typeof window !== 'undefined' && window.TD3_CONFIG_ENV) || {};

export function envInt(key) {
    const v = ENV[key];
    if (typeof v === 'number') return v;
    if (typeof v === 'string') {
        const n = parseInt(v, 10);
        return Number.isFinite(n) ? n : undefined;
    }
    return undefined;
}

export function envBool(key) {
    const v = ENV[key];
    if (typeof v === 'boolean') return v;
    if (typeof v === 'number')  return v !== 0;
    if (typeof v === 'string')  return v === '1' || v.toLowerCase() === 'true';
    return undefined;
}

export function envStr(key) {
    const v = ENV[key];
    return v != null ? String(v) : undefined;
}

/** Snapshot for code that wants to pass the whole thing around (e.g. applyUiDefaults). */
export function envSnapshot() {
    return { ...ENV };
}
