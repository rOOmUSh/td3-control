// Boot-time runtime config snapshot.
//
// The server inlines `window.TD3_CONFIG_ENV` into every served HTML page
// (see `src/web/static_html.rs`), so the browser has the resolved
// TD3_CONFIG.env values at first paint - no fetch round-trip, no
// startup race. This module exposes the same snapshot to JS modules
// through `getAppConfig()` and stamps the env-driven defaults into the
// DOM through `applyUiDefaults()`.
//
// `loadAppConfig()` is kept for backward compatibility (and as a thin
// fallback when the inline injection somehow didn't happen - e.g. when
// these files are served by an unrelated static server during dev).

import { api } from './api.js';

const INLINE = (typeof window !== 'undefined' && window.TD3_CONFIG_ENV) || null;
let cached = INLINE;

/**
 * Return the runtime config snapshot. Synchronous on every page that was
 * served through the Rust web server (the standard production path).
 *
 * The async fallback is only useful when the page is served from a
 * non-injecting static host. In that case the first call kicks off a
 * fetch; the cached value is filled in once it resolves.
 */
export async function loadAppConfig() {
    if (cached !== null) return cached;
    try {
        cached = await api.getEnvConfig();
    } catch (_) {
        cached = null;
    }
    return cached;
}

/** Synchronous accessor - returns the cached snapshot, or null if it hasn't been
 *  populated yet (only possible on a non-injecting host before loadAppConfig resolves). */
export function getAppConfig() {
    return cached;
}

/** Read a single env key synchronously. Throws if the snapshot is unavailable. */
export function envValue(key) {
    if (!cached) {
        throw new Error(`TD3_CONFIG_ENV not available - page must be served through the TD-3 web server (got ${typeof window !== 'undefined' ? 'window.TD3_CONFIG_ENV=' + window.TD3_CONFIG_ENV : 'no window'})`);
    }
    return cached[key];
}

/**
 * Stamp a value into an input element only if the field hasn't already been
 * restored from sessionStorage or user interaction. We treat the env default
 * as "what the input should show on first boot" - existing persisted state
 * always wins.
 */
function stampInputValue(id, value) {
    if (value === undefined || value === null) return;
    const el = document.getElementById(id);
    if (!el) return;
    el.value = String(value);
}

/**
 * Stamp a percentage slider and its companion `<span id="${id}-val">` label.
 */
function stampSlider(id, value) {
    if (value === undefined || value === null) return;
    const el = document.getElementById(id);
    if (!el) return;
    el.value = String(value);
    const label = document.getElementById(`${id}-val`);
    if (label) label.textContent = value + '%';
}

/**
 * Stamp a BPM display element. The main/progression pages wrap BPM in a
 * `<span id="bpm-display">` rather than an input, so this stamps textContent.
 */
function stampBpmDisplay(value) {
    if (value === undefined || value === null) return;
    const el = document.getElementById('bpm-display');
    if (el) el.textContent = String(value);
}

/**
 * Stamp env-driven defaults into the DOM. Both index.html and progression.html
 * share the same input IDs (root-select, scale-select, slider-note/slide/acc,
 * bpm-display, bank-size), so one stamper covers both. Call after loadAppConfig
 * resolves and before module init reads DOM values.
 *
 * The scale default is recorded as a dataset hint on #scale-select rather
 * than an immediate assignment because <option>s are not populated until
 * `populateScaleSelect` runs later; the randomize/progression init hooks
 * apply the hint once options exist.
 */
export function applyUiDefaults(cfg) {
    if (!cfg) return;
    stampSlider('slider-note', cfg.uiRandNotePercent);
    stampSlider('slider-slide', cfg.uiRandSlidePercent);
    stampSlider('slider-acc', cfg.uiRandAccPercent);
    stampSlider('slider-ud', cfg.uiRandUdPercent);
    stampInputValue('root-select', cfg.uiRandDefaultRoot);
    stampBpmDisplay(cfg.uiDefaultBpm);
    stampInputValue('bank-size', cfg.uiMaxBankHistorySize);
    if (cfg.uiRandDefaultScale !== undefined && cfg.uiRandDefaultScale !== null) {
        const scaleEl = document.getElementById('scale-select');
        if (scaleEl) scaleEl.dataset.defaultScale = String(cfg.uiRandDefaultScale);
    }
}
