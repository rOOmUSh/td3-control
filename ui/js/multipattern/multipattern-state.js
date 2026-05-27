// Multi-pattern state for the main page. Replaces `state.js` for main-page
// flows. Progression page keeps its own `progression-state.js` untouched.
//
// Data shape:
//     patterns        : Pattern[]           // 1..64 entries
//     focusedIdx      : number | null
//     checkedSet      : Set<number>
//     timelineDefault : number[]            // 1-based pattern numbers (0 = empty) - active when no checkboxes
//     timelineChecked : number[]            // same encoding - active when any checkbox is set
//     abMode          : 'ALTERNATE' | 'SERIAL'
//     viewport        : { group, side }     // K24 - visibility filter
//     clipboard       : Pattern | null      // persisted separately
//
// Dual-timeline playback model:
//   - No checkboxes  → timelineDefault drives playback. Grows with ADD/DUP,
//                      compacts on DEL, untouched by MOVE (slot-numbered).
//                      User-editable in the timeline modal.
//   - ≥1 checkbox    → timelineChecked drives playback. Auto-appended on
//                      check-on (once), all entries for a pattern stripped
//                      on check-off. Also user-editable in the timeline
//                      modal (repeats + reorder). Preserved across
//                      check-all-off so resuming a checkbox session
//                      restores the last arrangement.
// getTimeline()/setTimeline() transparently route to the active timeline
// so the modal and transport never need to know which one is in play.
//
// The single-pattern API that `state.js` exposed is preserved - callers that
// pass a single stepIdx get the focused pattern. New multi-pattern call
// shapes are added alongside, not instead.
//
// Persistence: sessionStorage keys `td3_multipattern` (state) and
// `td3_multipattern_clipboard` (clipboard). Legacy `td3_pattern` is migrated
// on first hydrate, then deleted.

import { transposeStepsInPlace } from '../shared/transpose-step.js';
import { defaultPattern, clonePattern, isPatternDefault } from './pattern-default.js';
import { envInt, envBool } from '../td3-env.js';

const STORAGE_KEY = 'td3_multipattern';
const CLIPBOARD_KEY = 'td3_multipattern_clipboard';
const LEGACY_KEY = 'td3_pattern';
const BANK_KEY = 'td3_bank';
const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B', 'C^'];
const MAX_PATTERNS = 64;

// Boot defaults pulled from window.TD3_CONFIG_ENV (template + user file).
// No JS-side hardcoded fallbacks - the server-injected snapshot is the
// single source of truth.
const ENV_BPM         = envInt('uiDefaultBpm');
const ENV_BANK_SIZE   = envInt('uiMaxBankHistorySize');
const ENV_LIVE_UPDATE = envBool('uiAutoSetLiveUpdate');

// --- Core state ---

let patterns = [defaultPattern()];
let focusedIdx = 0;
let checkedSet = new Set();
let timelineDefault = [1];
let timelineChecked = [];
let abMode = 'SERIAL';
let viewport = { group: 'ALL', side: 'ALL' };
let clipboard = null;

// --- Transient UI state (not undoable) ---

let group = 1;
let patternNum = 1;
let side = 'A';
let bpm = parseFloat(sessionStorage.getItem('td3_bpm')) || ENV_BPM;
let playing = sessionStorage.getItem('td3_playing') === 'true';
let connected = sessionStorage.getItem('td3_midi_connected') === 'true';
let liveUpdate = !!ENV_LIVE_UPDATE;
let sliceEnabled = false;
let sliceText = '';
let bankSize = ENV_BANK_SIZE;
let kbEditEnabled = false;
let autoStepFwd = false;
let selectedStep = 0;
// Transient per-pattern-index "NO SAVE" flags for the PREVIEW button.
// Not persisted and not part of the pattern data model: when set (or when
// live update is off) PREVIEW auditions the pattern without saving it to the
// device. Indexed positionally, like the preview controller's active index.
let noSaveFlags = [];

// Scratch slot descriptor fetched from backend (sidebar selector) - drives
// slot-badge recomputation and PUSH TO TD-3 target resolution. Transient:
// not persisted to sessionStorage, not part of the undo snapshot. Starts
// null until main.js fetches it, which is fine because `slotFor(idx, null,
// mode)` degrades to the pre-scratch-removal ordering.
let scratchSlot = null;

let hydratedFromStorage = false;

// --- Bank (shared with progression page) ---

let bank = [];
function loadBank() {
    try {
        const raw = sessionStorage.getItem(BANK_KEY);
        if (raw) bank = JSON.parse(raw);
    } catch (_) { /* corrupt */ }
}
function saveBank() {
    try { sessionStorage.setItem(BANK_KEY, JSON.stringify(bank)); }
    catch (_) { /* quota */ }
}
loadBank();

// --- Listener bus ---
//
// Contract: onChange(fn) where
// fn(patternChanged: boolean, structuralChange: boolean).

const listeners = [];
export function onChange(fn) { listeners.push(fn); }

function notify(patternChanged = false, structuralChange = false) {
    save();
    listeners.forEach(fn => fn(patternChanged, structuralChange));
}

// --- Persistence ---

function save() {
    try {
        sessionStorage.setItem(STORAGE_KEY, JSON.stringify({
            patterns, focusedIdx,
            checked: Array.from(checkedSet),
            timelineDefault, timelineChecked,
            abMode, viewport,
            group, patternNum, side, bpm, liveUpdate,
            sliceEnabled, sliceText, bankSize,
        }));
    } catch (_) { /* quota */ }
}

function saveClipboard() {
    try {
        if (clipboard) sessionStorage.setItem(CLIPBOARD_KEY, JSON.stringify(clipboard));
        else sessionStorage.removeItem(CLIPBOARD_KEY);
    } catch (_) { /* quota */ }
}

function loadClipboard() {
    try {
        const raw = sessionStorage.getItem(CLIPBOARD_KEY);
        if (raw) {
            const p = JSON.parse(raw);
            if (p && Array.isArray(p.steps) && p.steps.length === 16) clipboard = p;
        }
    } catch (_) { /* corrupt */ }
}

function load() {
    // Preferred: hydrate from the new key.
    try {
        const raw = sessionStorage.getItem(STORAGE_KEY);
        if (raw) {
            const d = JSON.parse(raw);
            if (Array.isArray(d.patterns) && d.patterns.length > 0
                && d.patterns.every(p => p && Array.isArray(p.steps) && p.steps.length === 16)) {
                patterns = d.patterns.slice(0, MAX_PATTERNS);
            }
            if (typeof d.focusedIdx === 'number' && d.focusedIdx >= 0 && d.focusedIdx < patterns.length) {
                focusedIdx = d.focusedIdx;
            } else if (patterns.length > 0) {
                focusedIdx = 0;
            } else {
                focusedIdx = null;
            }
            if (Array.isArray(d.checked)) {
                checkedSet = new Set(d.checked.filter(i => Number.isInteger(i) && i >= 0 && i < patterns.length));
            }
            // Dual-timeline load. Back-compat: pre-dual-timeline sessions
            // only have `d.timeline`; treat as timelineDefault and start
            // checked blank.
            if (Array.isArray(d.timelineDefault)) timelineDefault = d.timelineDefault.slice();
            else if (Array.isArray(d.timeline)) timelineDefault = d.timeline.slice();
            if (Array.isArray(d.timelineChecked)) timelineChecked = d.timelineChecked.slice();
            if (d.abMode === 'ALTERNATE' || d.abMode === 'SERIAL') abMode = d.abMode;
            if (d.viewport && typeof d.viewport === 'object') {
                viewport = {
                    group: d.viewport.group || 'ALL',
                    side: d.viewport.side || 'ALL',
                };
            }
            group = d.group || 1;
            patternNum = d.patternNum || 1;
            side = d.side || 'A';
            bpm = d.bpm || bpm;
            liveUpdate = !!d.liveUpdate;
            sliceEnabled = !!d.sliceEnabled;
            sliceText = d.sliceText || '';
            bankSize = d.bankSize || ENV_BANK_SIZE;
            hydratedFromStorage = true;
            loadClipboard();
            return;
        }
    } catch (_) { /* corrupt - fall through to migration */ }

    // Migration: legacy single-pattern blob at td3_pattern.
    try {
        const raw = sessionStorage.getItem(LEGACY_KEY);
        if (raw) {
            const d = JSON.parse(raw);
            if (d.pattern && Array.isArray(d.pattern.steps) && d.pattern.steps.length === 16) {
                patterns = [d.pattern];
                focusedIdx = 0;
                checkedSet = new Set();
                timelineDefault = [1];
                timelineChecked = [];
                abMode = 'SERIAL';
                viewport = { group: 'ALL', side: 'ALL' };
                group = d.group || 1;
                patternNum = d.patternNum || 1;
                side = d.side || 'A';
                bpm = d.bpm || bpm;
                liveUpdate = !!d.liveUpdate;
                sliceEnabled = !!d.sliceEnabled;
                sliceText = d.sliceText || '';
                bankSize = d.bankSize || ENV_BANK_SIZE;
                hydratedFromStorage = true;
                save();
                // Delete legacy blob only after we've successfully persisted
                // the migrated shape - otherwise a save quota failure would
                // lose the user's pattern.
                try { sessionStorage.removeItem(LEGACY_KEY); } catch (_) { /* ignore */ }
                loadClipboard();
                return;
            }
        }
    } catch (_) { /* corrupt legacy - keep defaults */ }

    loadClipboard();
}

// --- Focus/check helpers ---

function clampFocus() {
    if (patterns.length === 0) { focusedIdx = null; return; }
    if (focusedIdx === null || focusedIdx < 0 || focusedIdx >= patterns.length) {
        focusedIdx = 0;
    }
}

function pruneChecked() {
    const next = new Set();
    for (const i of checkedSet) if (Number.isInteger(i) && i >= 0 && i < patterns.length) next.add(i);
    checkedSet = next;
}

/**
 * Selection: bulk ops target checked if non-empty, else
 * focused if set, else nothing. Returns an array of pattern indexes.
 */
export function getSelectionIndexes() {
    if (checkedSet.size > 0) return Array.from(checkedSet).sort((a, b) => a - b);
    if (focusedIdx !== null) return [focusedIdx];
    return [];
}

// --- Getters (single-pattern compatibility + multi-pattern additions) ---

export function getPatterns() { return patterns; }
export function getPatternCount() { return patterns.length; }

/**
 * getPattern(i?) - with no args, returns the focused pattern.
 * Mirrors the single-pattern state.js surface so main.js can be rewired
 * with minimal churn.
 */
export function getPattern(i) {
    if (i === undefined) {
        if (focusedIdx === null) return null;
        return patterns[focusedIdx];
    }
    return patterns[i];
}

/**
 * getStep(stepIdx) or getStep(patIdx, stepIdx) - single-arg form reads the
 * focused pattern for backward compatibility with state.js callers.
 */
export function getStep(a, b) {
    if (b === undefined) {
        if (focusedIdx === null) return null;
        return patterns[focusedIdx].steps[a];
    }
    return patterns[a].steps[b];
}

export function getActiveSteps(patIdx) {
    const i = (patIdx === undefined) ? focusedIdx : patIdx;
    if (i === null || i === undefined) return 16;
    return patterns[i].active_steps;
}

/**
 * Maximum active_steps across every pattern. Drives the global STEPS
 * display: with per-pattern active_steps the global field auto-shows the
 * longest, so the user can see the upper bound at a glance. Defaults to
 * 1 when no patterns exist (shouldn't happen - N≥1 invariant - but
 * defensive).
 */
export function getMaxActiveSteps() {
    if (patterns.length === 0) return 1;
    let max = 1;
    for (let i = 0; i < patterns.length; i += 1) {
        const v = patterns[i].active_steps;
        if (v > max) max = v;
    }
    return max;
}

export function getTriplet(patIdx) {
    const i = (patIdx === undefined) ? focusedIdx : patIdx;
    if (i === null || i === undefined) return false;
    return patterns[i].triplet;
}

/**
 * Effective NO SAVE state for pattern `patIdx`: true when the per-row flag is
 * set OR live update is off. When live update is off, auditioning must never
 * write the pattern to the device, so the non-saving path is forced.
 */
export function isNoSave(patIdx) {
    const i = (patIdx === undefined) ? focusedIdx : patIdx;
    if (i === null || i === undefined) return !liveUpdate;
    return !!noSaveFlags[i] || !liveUpdate;
}

/** Raw per-row NO SAVE checkbox state (independent of live update). */
export function isNoSaveChecked(patIdx) {
    const i = (patIdx === undefined) ? focusedIdx : patIdx;
    if (i === null || i === undefined) return false;
    return !!noSaveFlags[i];
}

export function setNoSave(patIdx, v) {
    if (patIdx === null || patIdx === undefined) return;
    noSaveFlags[patIdx] = !!v;
    notify();
}

export function getFocusedIdx() { return focusedIdx; }
export function getCheckedSet() { return new Set(checkedSet); }
export function getCheckedArray() { return Array.from(checkedSet).sort((a, b) => a - b); }
export function isChecked(i) { return checkedSet.has(i); }

/**
 * Dual-timeline routing: when ≥1 pattern is checked, playback cycles
 * through timelineChecked (the checkbox-owned arrangement). With zero
 * checks, timelineDefault drives playback. Both are persisted so
 * flipping modes preserves the other's arrangement.
 */
function activeTimelineIsChecked() { return checkedSet.size > 0; }
export function getTimeline() {
    return activeTimelineIsChecked() ? timelineChecked : timelineDefault;
}
export function getTimelineDefault() { return timelineDefault; }
export function getTimelineChecked() { return timelineChecked; }
export function isCheckedMode() { return activeTimelineIsChecked(); }
export function getAbMode() { return abMode; }
export function getViewport() { return { ...viewport }; }
export function getClipboard() { return clipboard; }
export function hasClipboard() { return clipboard !== null; }

export function getGroup() { return group; }
export function getPatternNum() { return patternNum; }
export function getSide() { return side; }

/**
 * Snapshot the sidebar group/pattern/side selector as a slot descriptor -
 * used as the starting anchor for PUSH TO TD-3 and card badge slot walks,
 * so `btn-ab-mode` controls ONLY ordering (ALT vs SER) while the sidebar
 * controls WHERE the walk begins on the device.
 * @returns {{group:number, pattern:number, side:'A'|'B', label:string}}
 */
export function getSelectedSlot() {
    return {
        group, pattern: patternNum, side,
        label: `G${group}P${patternNum}${side}`,
    };
}
export function getBpm() { return bpm; }
export function isPlaying() { return playing; }
export function isConnected() { return connected; }
export function isLiveUpdate() { return liveUpdate; }
export function isSliceEnabled() { return sliceEnabled; }
export function getSliceText() { return sliceText; }
export function getBankSize() { return bankSize; }
export function getBank() { return bank; }
export function getBankCount() { return bank.length; }
export function getNoteNames() { return NOTE_NAMES; }
export function isKbEditEnabled() { return kbEditEnabled; }
export function isAutoStepFwd() { return autoStepFwd; }
export function getSelectedStep() { return selectedStep; }
export function getScratchSlot() { return scratchSlot; }

export function noteIndex(name) { return NOTE_NAMES.indexOf(name); }
export function noteName(idx) { return NOTE_NAMES[Math.max(0, Math.min(12, idx))]; }

// --- Setters (single-pattern compatibility surface) ---

function resolveIdx(patIdx) {
    if (patIdx === undefined) return focusedIdx;
    return patIdx;
}

export function setPattern(a, b) {
    // setPattern(p)           -> replaces focused
    // setPattern(idx, p)      -> replaces at idx
    let i, p;
    if (b === undefined) { i = focusedIdx; p = a; }
    else                 { i = a; p = b; }
    if (i === null || i < 0 || i >= patterns.length) return;
    patterns[i] = p;
    notify(true);
}

export function setStep(a, b, c) {
    // setStep(stepIdx, step)           -> focused
    // setStep(patIdx, stepIdx, step)   -> explicit
    let patIdx, stepIdx, step;
    if (c === undefined) { patIdx = focusedIdx; stepIdx = a; step = b; }
    else                 { patIdx = a; stepIdx = b; step = c; }
    if (patIdx === null || patIdx === undefined) return;
    patterns[patIdx].steps[stepIdx] = step;
    notify(true);
}

export function setActiveSteps(a, b) {
    let i, n;
    if (b === undefined) { i = focusedIdx; n = a; }
    else                 { i = a; n = b; }
    if (i === null || i === undefined) return;
    patterns[i].active_steps = Math.max(1, Math.min(16, n));
    notify(true);
}

/**
 * Set active_steps on every pattern at once. Drives the global STEPS
 * input: the user accepts that bumping the global field overwrites every
 * per-pattern value (UX: "user problem now - ctrl-z to revert"). Clamps
 * the value to 1..16.
 */
export function setAllActiveSteps(n) {
    const v = Math.max(1, Math.min(16, n));
    for (let i = 0; i < patterns.length; i += 1) {
        patterns[i].active_steps = v;
    }
    notify(true);
}

export function setTriplet(a, b) {
    let i, v;
    if (b === undefined) { i = focusedIdx; v = a; }
    else                 { i = a; v = b; }
    if (i === null || i === undefined) return;
    patterns[i].triplet = v;
    notify(true);
}

/**
 * Set triplet on every pattern in `idxList` to the same boolean value.
 * Single notify so the listener bus, undo history, and live-send see one
 * logical operation. No-op when the list is empty or every target already
 * holds `value`.
 */
export function setTripletBulk(idxList, value) {
    if (!Array.isArray(idxList) || idxList.length === 0) return;
    const v = !!value;
    let changed = false;
    for (const patIdx of idxList) {
        if (patIdx === null || patIdx === undefined) continue;
        if (patIdx < 0 || patIdx >= patterns.length) continue;
        if (patterns[patIdx].triplet !== v) {
            patterns[patIdx].triplet = v;
            changed = true;
        }
    }
    if (changed) notify(true);
}

export function setGroup(g) { group = g; notify(); }
export function setPatternNum(p) { patternNum = p; notify(); }
export function setSide(s) { side = s; notify(); }
export function setBpm(b) {
    const numeric = typeof b === 'number' ? b : parseFloat(b);
    const safe = Number.isFinite(numeric) ? numeric : 120;
    bpm = Math.round(Math.max(20, Math.min(300, safe)) * 100) / 100;
    sessionStorage.setItem('td3_bpm', String(bpm));
    notify();
}
export function setPlaying(v) { playing = v; sessionStorage.setItem('td3_playing', v ? 'true' : 'false'); notify(); }
export function setConnected(v) { connected = v; sessionStorage.setItem('td3_midi_connected', v ? 'true' : 'false'); notify(); }
export function setLiveUpdate(v) { liveUpdate = v; notify(); }
export function setSliceEnabled(v) { sliceEnabled = v; notify(); }
export function setSliceText(v) { sliceText = v; save(); }
export function setBankSize(v) { bankSize = Math.max(1, v); notify(); }
export function setKbEditEnabled(v) { kbEditEnabled = v; notify(); }
export function setAutoStepFwd(v) { autoStepFwd = v; notify(); }
export function setSelectedStep(v) { selectedStep = Math.max(0, Math.min(15, v)); notify(); }

/**
 * Update the scratch slot descriptor (main.js calls this after fetching
 * from the server, or after the user changes the scratch selector).
 * Fires a structural notify so per-card badges re-render.
 * Pass `null` to clear.
 */
export function setScratchSlot(s) {
    if (s === null || s === undefined) {
        scratchSlot = null;
    } else if (typeof s === 'object'
        && Number.isInteger(s.group) && s.group >= 1 && s.group <= 4
        && Number.isInteger(s.pattern) && s.pattern >= 1 && s.pattern <= 8
        && (s.side === 'A' || s.side === 'B')) {
        scratchSlot = {
            group: s.group,
            pattern: s.pattern,
            side: s.side,
            label: s.label || `G${s.group}P${s.pattern}${s.side}`,
        };
    } else {
        return; // malformed; keep old value
    }
    notify(false, true);
}

// --- Focus / check mutations ---

export function setFocused(i) {
    if (i === null) {
        if (patterns.length > 0) return; // must stay on a pattern if any exist
        focusedIdx = null;
    } else {
        if (!Number.isInteger(i) || i < 0 || i >= patterns.length) return;
        focusedIdx = i;
    }
    notify(false, true);
}

export function setChecked(i, on) {
    if (!Number.isInteger(i) || i < 0 || i >= patterns.length) return;
    const wasChecked = checkedSet.has(i);
    if (on && !wasChecked) {
        checkedSet.add(i);
        // Append this pattern's 1-based number to the checkbox timeline
        // exactly once - matches the user flow "check P5 → playback
        // appends 5". If the user later arranges [2,2,5,5], rechecking
        // P2 would append one 2 at the end, not restore the old layout.
        timelineChecked.push(i + 1);
    } else if (!on && wasChecked) {
        checkedSet.delete(i);
        // Uncheck strips every entry for this pattern - matches "uncheck
        // P5 → every 5 disappears from the arrangement".
        const num = i + 1;
        for (let k = timelineChecked.length - 1; k >= 0; k--) {
            if (timelineChecked[k] === num) timelineChecked.splice(k, 1);
        }
    } else {
        return; // no-op; avoid redundant notify
    }
    notify(false, true);
}

export function toggleChecked(i) { setChecked(i, !checkedSet.has(i)); }

export function clearChecked() {
    if (checkedSet.size === 0) return;
    checkedSet = new Set();
    // clearChecked is equivalent to unchecking every pattern in one
    // shot, so the checkbox timeline drains to empty too. A later
    // check starts the arrangement fresh (consistent with the
    // per-pattern rule "uncheck strips every entry").
    timelineChecked = [];
    notify(false, true);
}

export function setAllChecked(on) {
    if (patterns.length === 0) return;
    if (on) {
        if (checkedSet.size === patterns.length) return;
        checkedSet = new Set(patterns.map((_pattern, index) => index));
        timelineChecked = patterns.map((_pattern, index) => index + 1);
    } else {
        if (checkedSet.size === 0) return;
        checkedSet = new Set();
        timelineChecked = [];
    }
    notify(false, true);
}

/**
 * AUTO-STEP FWD focus advance. Called after the step counter wraps
 * 15 → 0 on a per-step edit. Walks focus through the current selection
 * (Reading X):
 *   - with ≥1 check → next checked index (wrapping after the last)
 *   - else          → next pattern in the full list (wrapping after N-1)
 * Returns the new focused index, or `null` when no advance was possible
 * (empty pattern list). One notify (structural - focus moved).
 */
export function advanceFocusAfterWrap() {
    if (patterns.length === 0 || focusedIdx === null) return null;
    let next;
    if (checkedSet.size > 0) {
        const ring = Array.from(checkedSet).sort((a, b) => a - b);
        const here = ring.indexOf(focusedIdx);
        next = here === -1
            ? ring[0]                           // focus wasn't in the checked ring; land on first checked
            : ring[(here + 1) % ring.length];   // next checked, wrapping
    } else {
        next = (focusedIdx + 1) % patterns.length;
    }
    if (next === focusedIdx) return focusedIdx; // single-element ring: stay put
    focusedIdx = next;
    notify(false, true);
    return focusedIdx;
}

// --- Timeline ---

/**
 * Write the active timeline (checkboxChecked in checkbox mode, default
 * otherwise). The timeline modal edits whichever is currently driving
 * playback, so routing writes through activeTimelineIsChecked() keeps
 * the modal logic transparent to the mode switch.
 */
export function setTimeline(t) {
    if (!Array.isArray(t)) return;
    if (activeTimelineIsChecked()) timelineChecked = t.slice();
    else                           timelineDefault = t.slice();
    notify(false, true);
}

// --- A/B mode + viewport ---

export function setAbMode(mode) {
    if (mode !== 'ALTERNATE' && mode !== 'SERIAL') return;
    abMode = mode;
    notify(false, true);
}

export function setViewport(v) {
    viewport = {
        group: v && v.group ? v.group : 'ALL',
        side: v && v.side ? v.side : 'ALL',
    };
    notify(false, true);
}

// --- Step mutations (single-pattern compatibility) ---

export function cycleTime(a, b) {
    let patIdx, stepIdx;
    if (b === undefined) { patIdx = focusedIdx; stepIdx = a; }
    else                 { patIdx = a; stepIdx = b; }
    if (patIdx === null || patIdx === undefined) return;
    const order = ['NORMAL', 'REST', 'TIE', 'TIE_REST'];
    const step = patterns[patIdx].steps[stepIdx];
    const idx = order.indexOf(step.time);
    step.time = order[(idx + 1) % order.length];
    notify(true);
}

export function toggleAccent(a, b) {
    let patIdx, stepIdx;
    if (b === undefined) { patIdx = focusedIdx; stepIdx = a; }
    else                 { patIdx = a; stepIdx = b; }
    if (patIdx === null || patIdx === undefined) return;
    patterns[patIdx].steps[stepIdx].accent = !patterns[patIdx].steps[stepIdx].accent;
    notify(true);
}

export function toggleSlide(a, b) {
    let patIdx, stepIdx;
    if (b === undefined) { patIdx = focusedIdx; stepIdx = a; }
    else                 { patIdx = a; stepIdx = b; }
    if (patIdx === null || patIdx === undefined) return;
    patterns[patIdx].steps[stepIdx].slide = !patterns[patIdx].steps[stepIdx].slide;
    notify(true);
}

export function toggleTranspose(a, b, c) {
    let patIdx, stepIdx, target;
    if (c === undefined) { patIdx = focusedIdx; stepIdx = a; target = b; }
    else                 { patIdx = a; stepIdx = b; target = c; }
    if (patIdx === null || patIdx === undefined) return;
    const step = patterns[patIdx].steps[stepIdx];
    step.transpose = (step.transpose === target) ? 'NORMAL' : target;
    notify(true);
}

export function changeNote(a, b, c) {
    let patIdx, stepIdx, delta;
    if (c === undefined) { patIdx = focusedIdx; stepIdx = a; delta = b; }
    else                 { patIdx = a; stepIdx = b; delta = c; }
    if (patIdx === null || patIdx === undefined) return;
    const step = patterns[patIdx].steps[stepIdx];
    const idx = noteIndex(step.note);
    const next = Math.max(0, Math.min(12, idx + delta));
    step.note = noteName(next);
    notify(true);
}

export function setNote(a, b, c) {
    let patIdx, stepIdx, name;
    if (c === undefined) { patIdx = focusedIdx; stepIdx = a; name = b; }
    else                 { patIdx = a; stepIdx = b; name = c; }
    if (patIdx === null || patIdx === undefined) return;
    patterns[patIdx].steps[stepIdx].note = name;
    notify(true);
}

export function setTime(a, b, c) {
    let patIdx, stepIdx, time;
    if (c === undefined) { patIdx = focusedIdx; stepIdx = a; time = b; }
    else                 { patIdx = a; stepIdx = b; time = c; }
    if (patIdx === null || patIdx === undefined) return;
    patterns[patIdx].steps[stepIdx].time = time;
    notify(true);
}

export function shiftSteps(a, b) {
    let patIdx, n;
    if (b === undefined) { patIdx = focusedIdx; n = a; }
    else                 { patIdx = a; n = b; }
    if (patIdx === null || patIdx === undefined) return;
    const steps = patterns[patIdx].steps;
    const len = 16;
    const shift = ((n % len) + len) % len;
    if (shift === 0) return;
    const old = steps.map(s => ({ ...s }));
    for (let i = 0; i < len; i++) steps[(i + shift) % len] = old[i];
    notify(true);
}

function fisherYatesShuffleStepsArray(steps) {
    const len = steps.length;
    if (len < 2) return;
    const snapshot = steps.map(s => ({ ...s }));
    for (let i = len - 1; i > 0; i--) {
        const j = Math.floor(Math.random() * (i + 1));
        if (j !== i) {
            const tmp = snapshot[i];
            snapshot[i] = snapshot[j];
            snapshot[j] = tmp;
        }
    }
    for (let i = 0; i < len; i++) steps[i] = snapshot[i];
}

export function shuffleSteps(patIdx) {
    const i = (patIdx === undefined) ? focusedIdx : patIdx;
    if (i === null || i === undefined) return;
    if (i < 0 || i >= patterns.length) return;
    fisherYatesShuffleStepsArray(patterns[i].steps);
    notify(true);
}

export function shuffleStepsBulk(idxList) {
    if (!Array.isArray(idxList) || idxList.length === 0) return;
    let touched = false;
    for (const patIdx of idxList) {
        if (patIdx === null || patIdx === undefined) continue;
        if (patIdx < 0 || patIdx >= patterns.length) continue;
        fisherYatesShuffleStepsArray(patterns[patIdx].steps);
        touched = true;
    }
    if (touched) notify(true);
}

export function transposePattern(a, b) {
    let patIdx, delta;
    if (b === undefined) { patIdx = focusedIdx; delta = a; }
    else                 { patIdx = a; delta = b; }
    if (patIdx === null || patIdx === undefined) return;
    transposeStepsInPlace(patterns[patIdx].steps, delta);
    notify(true);
}

export function resetPattern(patIdx) {
    const i = resolveIdx(patIdx);
    if (i === null || i === undefined) return;
    patterns[i] = defaultPattern();
    notify(true);
}

export function resetAllPatterns() {
    for (let i = 0; i < patterns.length; i++) patterns[i] = defaultPattern();
    notify(true, true);
}

// --- Structural ops ---

export function addPattern() {
    if (patterns.length >= MAX_PATTERNS) return false;
    patterns.push(defaultPattern());
    const newIdx = patterns.length - 1;
    focusedIdx = newIdx;
    // Default-timeline append. timelineChecked stays untouched:
    // the new pattern isn't checked yet and only checking it should
    // schedule it in the checkbox arrangement.
    timelineDefault.push(newIdx + 1);
    notify(true, true);
    return true;
}

export function duplicatePattern(srcIdx) {
    const i = (srcIdx === undefined) ? focusedIdx : srcIdx;
    if (i === null || i === undefined) return false;
    if (patterns.length >= MAX_PATTERNS) return false;
    const copy = clonePattern(patterns[i]);
    // Insert after the source. Existing indexes >= i+1 shift by +1.
    patterns.splice(i + 1, 0, copy);
    shiftCheckedForInsert(i + 1);
    shiftTimelineForInsert(i + 1);
    // Append the new pattern's number to the default timeline so it
    // plays as a full member - same reason addPattern() appends.
    // timelineChecked stays out: duplicates aren't auto-checked.
    timelineDefault.push(i + 2);
    focusedIdx = i + 1;
    notify(true, true);
    return true;
}

export function deletePattern(delIdx) {
    const i = (delIdx === undefined) ? focusedIdx : delIdx;
    if (i === null || i === undefined) return false;
    if (patterns.length <= 1) {
        // Keep at least one pattern; DEL on the last one resets it instead
        // of producing an empty list (always keep N ≥ 1).
        patterns[0] = defaultPattern();
        focusedIdx = 0;
        checkedSet = new Set();
        timelineDefault = [1];
        timelineChecked = [];
        notify(true, true);
        return true;
    }
    patterns.splice(i, 1);
    shiftCheckedForDelete(i);
    shiftTimelineForDelete(i);
    // Focus lands on the sibling (previous if deleting last, else same slot).
    if (i >= patterns.length) focusedIdx = patterns.length - 1;
    else                      focusedIdx = i;
    notify(true, true);
    return true;
}

// --- Bulk structural ops (toolbar buttons, checkbox-aware) ---

/**
 * Duplicate every checked pattern, appending the copies at the bottom of
 * the list in checked-index order. Toolbar DUPLICATE uses this when ≥1
 * pattern is checked; otherwise the single-target duplicatePattern() is
 * used. Caps at MAX_PATTERNS (silently truncates if the bulk would
 * overflow). Single notify(true, true).
 *
 * Append-only: the checked indexes themselves don't shift, so checkedSet
 * stays valid. Each new copy gets its own entry in timelineDefault - they
 * play as full members of the default arrangement, NOT auto-checked.
 *
 * Returns the number of patterns actually appended.
 */
export function duplicateCheckedToBottom() {
    const sel = Array.from(checkedSet).sort((a, b) => a - b);
    if (sel.length === 0) return 0;
    const room = MAX_PATTERNS - patterns.length;
    if (room <= 0) return 0;
    const take = Math.min(room, sel.length);
    // Snapshot source patterns BEFORE appending so reading sel[k] from
    // the live array is safe even if sel order overlaps (it won't here,
    // but defensive).
    const copies = sel.slice(0, take).map(i => clonePattern(patterns[i]));
    for (const copy of copies) {
        patterns.push(copy);
        timelineDefault.push(patterns.length); // 1-based, matches new index
    }
    notify(true, true);
    return take;
}

/**
 * Delete every checked pattern in one shot. Processes in reverse index
 * order so each splice doesn't shift the remaining targets. Honors the
 * N≥1 floor: if the bulk would empty the list, the last surviving
 * pattern is reset to default instead.
 *
 * Single notify(true, true). Returns the number deleted (may be less
 * than checkedSet.size when N≥1 floor kicks in).
 */
export function deleteCheckedPatterns() {
    const sel = Array.from(checkedSet).sort((a, b) => b - a); // reverse
    if (sel.length === 0) return 0;
    let removed = 0;
    for (const i of sel) {
        if (patterns.length <= 1) break; // keep N≥1; do final reset below
        patterns.splice(i, 1);
        shiftCheckedForDelete(i);
        shiftTimelineForDelete(i);
        removed += 1;
    }
    // If checkedSet still has the lone remaining pattern, treat that as
    // a request to reset it (matches single-DEL N=1 floor behavior).
    if (patterns.length === 1 && checkedSet.has(0)) {
        patterns[0] = defaultPattern();
        checkedSet = new Set();
        timelineDefault = [1];
        timelineChecked = [];
        removed += 1;
    }
    if (focusedIdx === null || focusedIdx >= patterns.length) {
        focusedIdx = patterns.length - 1;
    }
    notify(true, true);
    return removed;
}

/**
 * Apply rotational shift to every pattern in `idxList`. Single notify so
 * the listener bus sees one logical operation (history/undo gets one
 * entry, transport gets one re-evaluation). No-op when the list is empty
 * or the shift normalizes to 0.
 */
export function shiftStepsBulk(idxList, n) {
    if (!Array.isArray(idxList) || idxList.length === 0) return;
    const len = 16;
    const shift = ((n % len) + len) % len;
    if (shift === 0) return;
    for (const patIdx of idxList) {
        if (patIdx === null || patIdx === undefined) continue;
        if (patIdx < 0 || patIdx >= patterns.length) continue;
        const steps = patterns[patIdx].steps;
        const old = steps.map(s => ({ ...s }));
        for (let i = 0; i < len; i++) steps[(i + shift) % len] = old[i];
    }
    notify(true);
}

/**
 * Apply transpose ±N to every pattern in `idxList`. Single notify, same
 * rationale as shiftStepsBulk.
 */
export function transposeBulk(idxList, delta) {
    if (!Array.isArray(idxList) || idxList.length === 0) return;
    if (!delta) return;
    for (const patIdx of idxList) {
        if (patIdx === null || patIdx === undefined) continue;
        if (patIdx < 0 || patIdx >= patterns.length) continue;
        transposeStepsInPlace(patterns[patIdx].steps, delta);
    }
    notify(true);
}

/**
 * Convenience: indexes [0..N-1] for "all patterns" toolbar fallbacks.
 */
export function getAllIndexes() {
    return Array.from({ length: patterns.length }, (_, i) => i);
}

function shiftCheckedForInsert(insertedIdx) {
    const next = new Set();
    for (const c of checkedSet) {
        next.add(c >= insertedIdx ? c + 1 : c);
    }
    checkedSet = next;
}

function shiftCheckedForDelete(deletedIdx) {
    const next = new Set();
    for (const c of checkedSet) {
        if (c === deletedIdx) continue;
        next.add(c > deletedIdx ? c - 1 : c);
    }
    checkedSet = next;
}

function shiftTimelineForInsert(insertedIdx) {
    // Both timelines encode 1-based pattern numbers. Any entry >=
    // (insertedIdx + 1) bumps by +1 so it keeps pointing at the same
    // pattern content.
    const bump = insertedIdx + 1;
    bumpTimelineEntries(timelineDefault, bump);
    bumpTimelineEntries(timelineChecked, bump);
}

function bumpTimelineEntries(tl, bump) {
    for (let i = 0; i < tl.length; i++) {
        if (tl[i] >= bump) tl[i] += 1;
    }
}

function shiftTimelineForDelete(deletedIdx) {
    // Delete compacts the timeline: slots that referenced the removed
    // pattern are spliced out (not zeroed), and surviving references to
    // higher-numbered patterns shift down by one. Rationale: a zeroed
    // slot looks like a deliberate empty step during playback and makes
    // the remaining patterns stutter (loop count climbs, one slot goes
    // silent). Users expect the timeline to track UI pattern count -
    // delete P2 of [1,2,3] leaves [1,2], not [1,0,2].
    const removedNum = deletedIdx + 1;
    compactTimelineEntries(timelineDefault, removedNum);
    compactTimelineEntries(timelineChecked, removedNum);
}

function compactTimelineEntries(tl, removedNum) {
    for (let i = tl.length - 1; i >= 0; i--) {
        if (tl[i] === removedNum) tl.splice(i, 1);
        else if (tl[i] > removedNum) tl[i] -= 1;
    }
}

/**
 * Re-order the pattern at `from` to sit at final position `to` (post-move
 * index, matching Array#splice semantics: splice(from,1) then splice(to,0,x)).
 * Focus follows the moved pattern; checked indexes are remapped so marks
 * stay on the same pattern content.
 *
 * Timeline entries are *not* rewritten: they reference visual slot numbers
 * (P1 = index 0, P2 = index 1, …), so the new card order becomes the new
 * default playback order during timeline playback. This is the intent of
 * drag-to-reorder on the main page.
 *
 * No-op when `from === to`, either index is out of range, or N < 2.
 * Single notify(false, true) - no live send (content is identical), but
 * the structural flag drives list re-render + slot-badge refresh.
 */
export function movePattern(from, to) {
    const n = patterns.length;
    if (n < 2) return false;
    if (!Number.isInteger(from) || !Number.isInteger(to)) return false;
    if (from < 0 || from >= n || to < 0 || to >= n) return false;
    if (from === to) return false;

    const [moved] = patterns.splice(from, 1);
    patterns.splice(to, 0, moved);

    focusedIdx = remapIndexForMove(focusedIdx, from, to);

    const remapped = new Set();
    for (const c of checkedSet) {
        const r = remapIndexForMove(c, from, to);
        if (r !== null) remapped.add(r);
    }
    checkedSet = remapped;

    notify(false, true);
    return true;
}

function remapIndexForMove(i, from, to) {
    if (i === null || i === undefined) return i;
    if (i === from) return to;
    if (from < to && i > from && i <= to) return i - 1;
    if (to < from && i >= to && i < from) return i + 1;
    return i;
}

// --- Clipboard ---

export function copyFocused() {
    if (focusedIdx === null) return false;
    clipboard = clonePattern(patterns[focusedIdx]);
    saveClipboard();
    // Structural notify so per-card PASTE FULL buttons re-evaluate their
    // disabled state. Patterns themselves didn't change, so patternChanged
    // stays false (no live-send, no history entry).
    notify(false, true);
    return true;
}

export function pasteIntoFocused() {
    if (focusedIdx === null || clipboard === null) return false;
    patterns[focusedIdx] = clonePattern(clipboard);
    notify(true);
    return true;
}

// --- LOAD / LOAD ALL / IMPORT helpers ---

/**
 * Append a single pattern at the end of the list (LOAD single, IMPORT one
 * slot). Returns the new index, or null when the list is already at cap.
 */
export function appendPattern(pat) {
    if (!pat || !Array.isArray(pat.steps) || pat.steps.length !== 16) return null;
    if (patterns.length >= MAX_PATTERNS) return null;
    patterns.push(clonePattern(pat));
    const newIdx = patterns.length - 1;
    // Default-timeline append only - same semantics as addPattern().
    timelineDefault.push(newIdx + 1);
    focusedIdx = newIdx;
    notify(true, true);
    return newIdx;
}

/**
 * Replace the entire pattern list (LOAD ALL, IMPORT bank). Trims/pads to
 * within MAX_PATTERNS, resets focus to 0, clears checks, resets timeline
 * to the default `[1..N]` fill.
 */
export function replaceAllPatterns(newPatterns) {
    if (!Array.isArray(newPatterns) || newPatterns.length === 0) return false;
    const safe = newPatterns.slice(0, MAX_PATTERNS)
        .filter(p => p && Array.isArray(p.steps) && p.steps.length === 16);
    if (safe.length === 0) return false;
    patterns = safe.map(clonePattern);
    focusedIdx = 0;
    checkedSet = new Set();
    timelineDefault = Array.from({ length: patterns.length }, (_, i) => i + 1);
    timelineChecked = [];
    notify(true, true);
    return true;
}

/**
 * True when any pattern in the current list has been edited away from the
 * factory default. Used by the LOAD ALL confirmation guard (F13).
 */
export function hasNonDefaultPatterns() {
    return patterns.some(p => !isPatternDefault(p));
}

// --- Bank shim (kept for parity with state.js callers) ---

export function pushToBank(pat) {
    bank.push(clonePattern(pat));
    saveBank();
    notify();
}
export function clearBank() { bank = []; saveBank(); notify(); }

// --- Env defaults ---

export function setDefaultsFromEnv(cfg) {
    if (!cfg) return;
    if (sessionStorage.getItem('td3_bpm') === null && typeof cfg.uiDefaultBpm === 'number') {
        bpm = Math.max(20, Math.min(300, cfg.uiDefaultBpm));
        sessionStorage.setItem('td3_bpm', String(bpm));
    }
    if (!hydratedFromStorage) {
        if (typeof cfg.uiDefaultTriplet === 'boolean') {
            for (const p of patterns) p.triplet = cfg.uiDefaultTriplet;
        }
        if (typeof cfg.uiAutoSetLiveUpdate === 'boolean') liveUpdate = cfg.uiAutoSetLiveUpdate;
        if (typeof cfg.uiMaxBankHistorySize === 'number' && cfg.uiMaxBankHistorySize > 0) {
            bankSize = Math.max(1, Math.floor(cfg.uiMaxBankHistorySize));
        }
        save();
    }
}

// --- Snapshot for undo/redo ---

export function getSnapshot() {
    return JSON.parse(JSON.stringify({
        patterns,
        focusedIdx,
        checked: Array.from(checkedSet),
        timelineDefault,
        timelineChecked,
        abMode,
        viewport,
    }));
}

export function restoreSnapshot(snap, skipNotify) {
    if (!snap || !Array.isArray(snap.patterns) || snap.patterns.length === 0) return;
    patterns = snap.patterns.map(clonePattern);
    focusedIdx = (typeof snap.focusedIdx === 'number')
        ? Math.max(0, Math.min(patterns.length - 1, snap.focusedIdx))
        : 0;
    checkedSet = new Set(Array.isArray(snap.checked) ? snap.checked : []);
    // Dual-timeline restore with back-compat: snapshots taken before
    // the split only carry `timeline` - treat as default, blank checked.
    if (Array.isArray(snap.timelineDefault))        timelineDefault = snap.timelineDefault.slice();
    else if (Array.isArray(snap.timeline))          timelineDefault = snap.timeline.slice();
    timelineChecked = Array.isArray(snap.timelineChecked) ? snap.timelineChecked.slice() : [];
    if (snap.abMode === 'ALTERNATE' || snap.abMode === 'SERIAL') abMode = snap.abMode;
    if (snap.viewport && typeof snap.viewport === 'object') {
        viewport = {
            group: snap.viewport.group || 'ALL',
            side: snap.viewport.side || 'ALL',
        };
    }
    clampFocus();
    pruneChecked();
    if (skipNotify) save();
    else notify(true, true);
}

// --- Init ---

load();
clampFocus();
pruneChecked();
