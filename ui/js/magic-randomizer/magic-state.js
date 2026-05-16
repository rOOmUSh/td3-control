// MAGIC randomizer flag - persisted in localStorage so the user's choice
// sticks across reloads.
//
// When the checkbox is unchecked (default) the legacy randomizer in
// `randomize.js` runs verbatim. When checked, all sidebar randomize entry
// points (control RANDOMIZE, slice mode, progression RANDOMIZE 1-4 and
// 2-4) route through the magic pipeline in this folder.

const STORAGE_KEY = 'td3_magic_randomizer';

let cached = null;

function readRaw() {
    try {
        return window.localStorage.getItem(STORAGE_KEY);
    } catch (_) {
        return null;
    }
}

function writeRaw(value) {
    try {
        window.localStorage.setItem(STORAGE_KEY, value);
    } catch (_) {
        // ignore - falls back to in-memory cache for this session
    }
}

function dispatchChange(enabled) {
    try {
        document.dispatchEvent(new CustomEvent('magic:changed', { detail: { enabled } }));
    } catch (_) {
        // node test env - no document; in-memory cache still works
    }
}

export function isMagicEnabled() {
    if (cached !== null) return cached;
    cached = readRaw() === '1';
    return cached;
}

export function setMagicEnabled(on) {
    cached = !!on;
    writeRaw(cached ? '1' : '0');
    dispatchChange(cached);
}

/** Test-only - reset cached state. */
export function _resetMagicCache() {
    cached = null;
}

/**
 * Wire the `#magic-toggle` checkbox in the shared sidebar partial to the
 * persisted flag. Idempotent - safe to call from both control and
 * progression page boot. No-op when the partial isn't on the page.
 */
export function wireMagicToggle() {
    const cb = document.getElementById('magic-toggle');
    if (!cb || cb.dataset.magicWired === '1') return;
    cb.dataset.magicWired = '1';
    cb.checked = isMagicEnabled();
    cb.addEventListener('change', () => setMagicEnabled(cb.checked));
}
