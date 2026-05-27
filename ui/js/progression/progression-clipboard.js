// Per-row copy/paste clipboard for the Progression page.
//
// Four independent slots:
//   - rest   : array of 16 `time` values (NORMAL/TIE/REST/TIE_REST).
//   - slide  : array of 16 booleans.
//   - accent : array of 16 booleans.
//   - full   : bridge to the main Control page - COPY FULL pushes into
//              sessionStorage['td3_multipattern'] at the focused index;
//              PASTE FULL pulls that same focused-pattern blob.
//
// rest/slide/accent live in module-scope memory only: they don't persist
// across reloads and don't cross pages. The full slot *does* cross pages
// because the main-page state reads that sessionStorage key at boot.
//
// Before the multi-pattern rewire this module wrote `td3_pattern`; now it
// writes the new `td3_multipattern` blob so that after the user lands on
// the main page the focused pattern reflects what progression pushed.
//
// Subscribers are notified on every copy/paste so the UI can enable or
// disable PASTE buttons based on clipboard presence.

const STORAGE_KEY = 'td3_multipattern';

const buffers = {
    rest: null,    // Array<string> | null, length 16
    slide: null,   // Array<boolean> | null, length 16
    accent: null,  // Array<boolean> | null, length 16
};

const listeners = new Set();

export function subscribe(fn) { listeners.add(fn); }
export function unsubscribe(fn) { listeners.delete(fn); }
function notify() { for (const fn of listeners) fn(); }

/** True if the given slot currently holds a payload. */
export function has(kind) {
    if (kind === 'full') {
        try {
            const raw = sessionStorage.getItem(STORAGE_KEY);
            if (!raw) return false;
            const parsed = JSON.parse(raw);
            const focused = focusedPatternFrom(parsed);
            return !!(focused && Array.isArray(focused.steps) && focused.steps.length === 16);
        } catch (_) { return false; }
    }
    return buffers[kind] !== null;
}

/** Pull the focused pattern from a parsed `td3_multipattern` blob. */
function focusedPatternFrom(blob) {
    if (!blob || typeof blob !== 'object') return null;
    if (!Array.isArray(blob.patterns) || blob.patterns.length === 0) return null;
    const idx = (typeof blob.focusedIdx === 'number' && blob.focusedIdx >= 0
        && blob.focusedIdx < blob.patterns.length) ? blob.focusedIdx : 0;
    return blob.patterns[idx];
}

/** Copy the requested slice from `pattern` into the corresponding slot. */
export function copy(kind, pattern) {
    if (!pattern || !pattern.steps || pattern.steps.length !== 16) return;
    if (kind === 'rest') {
        buffers.rest = pattern.steps.map(s => s.time);
    } else if (kind === 'slide') {
        buffers.slide = pattern.steps.map(s => !!s.slide);
    } else if (kind === 'accent') {
        buffers.accent = pattern.steps.map(s => !!s.accent);
    } else if (kind === 'full') {
        try {
            const raw = sessionStorage.getItem(STORAGE_KEY);
            const blob = raw ? JSON.parse(raw) : null;
            const next = (blob && Array.isArray(blob.patterns) && blob.patterns.length > 0)
                ? blob
                : { patterns: [], focusedIdx: 0, checked: [], timeline: [], abMode: 'SERIAL',
                    viewport: { group: 'ALL', side: 'ALL' } };
            if (!Array.isArray(next.patterns) || next.patterns.length === 0) {
                next.patterns = [JSON.parse(JSON.stringify(pattern))];
                next.focusedIdx = 0;
                next.timeline = [1];
            } else {
                const idx = (typeof next.focusedIdx === 'number' && next.focusedIdx >= 0
                    && next.focusedIdx < next.patterns.length) ? next.focusedIdx : 0;
                next.patterns[idx] = JSON.parse(JSON.stringify(pattern));
            }
            sessionStorage.setItem(STORAGE_KEY, JSON.stringify(next));
        } catch (_) { /* quota / corrupt - silently no-op */ }
    } else {
        return;
    }
    notify();
}

/**
 * Apply the slot's payload to `pattern` in-place. Returns true if anything
 * was written (so callers can decide whether to notify their own state).
 * For 'full', the entire pattern is replaced (steps + active_steps + triplet).
 */
export function paste(kind, pattern) {
    if (!pattern || !pattern.steps || pattern.steps.length !== 16) return false;
    if (kind === 'rest') {
        if (!buffers.rest) return false;
        for (let i = 0; i < 16; i++) {
            pattern.steps[i].time = buffers.rest[i];
            // Match the REST step's "no slide/accent" invariant used by the
            // rest-mask randomizer, so pasted REST blocks don't leak stale
            // accents/slides from the target row.
            if (pattern.steps[i].time === 'REST' || pattern.steps[i].time === 'TIE_REST') {
                pattern.steps[i].slide = false;
                pattern.steps[i].accent = false;
            }
        }
        return true;
    }
    if (kind === 'slide') {
        if (!buffers.slide) return false;
        for (let i = 0; i < 16; i++) pattern.steps[i].slide = buffers.slide[i];
        return true;
    }
    if (kind === 'accent') {
        if (!buffers.accent) return false;
        for (let i = 0; i < 16; i++) pattern.steps[i].accent = buffers.accent[i];
        return true;
    }
    if (kind === 'full') {
        let blob;
        try {
            const raw = sessionStorage.getItem(STORAGE_KEY);
            if (!raw) return false;
            blob = JSON.parse(raw);
        } catch (_) { return false; }
        const src = focusedPatternFrom(blob);
        if (!src || !Array.isArray(src.steps) || src.steps.length !== 16) return false;
        for (let i = 0; i < 16; i++) {
            pattern.steps[i] = JSON.parse(JSON.stringify(src.steps[i]));
        }
        if (typeof src.active_steps === 'number') pattern.active_steps = src.active_steps;
        if (typeof src.triplet === 'boolean') pattern.triplet = src.triplet;
        return true;
    }
    return false;
}

/** For tests: reset all in-memory slots. Does not touch sessionStorage. */
export function _resetForTests() {
    buffers.rest = null;
    buffers.slide = null;
    buffers.accent = null;
    listeners.clear();
}
