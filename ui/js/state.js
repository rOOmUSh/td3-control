// Pattern state management with sessionStorage persistence.

import { transposeStepsInPlace } from './shared/transpose-step.js';
import { envInt, envBool } from './td3-env.js';

const STORAGE_KEY = 'td3_pattern';
const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B', 'C^'];

// All boot defaults read from the inlined TD3_CONFIG.env snapshot. There
// are no JS-side fallback literals - the server template
// (config/default_env.template) is the single source of truth.
const ENV_BPM         = envInt('uiDefaultBpm');
const ENV_TRIPLET     = envBool('uiDefaultTriplet');
const ENV_BANK_SIZE   = envInt('uiMaxBankHistorySize');
const ENV_LIVE_UPDATE = envBool('uiAutoSetLiveUpdate');

function defaultStep() {
    return { note: 'C', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' };
}

function defaultPattern() {
    return {
        active_steps: 16,
        triplet: !!ENV_TRIPLET,
        steps: Array.from({ length: 16 }, defaultStep),
    };
}

// Current in-memory state
let pattern = defaultPattern();
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

// Tracks whether load() rehydrated from a persisted blob. setDefaultsFromEnv
// only overrides module defaults when this is still false - a user session that
// already has a pattern blob must keep its remembered values even if the env
// file changed on disk.
let hydratedFromStorage = false;
let kbEditEnabled = false;
let autoStepFwd = false;
let selectedStep = 0;

// Memory bank - stored separately to avoid bloating main state
const BANK_KEY = 'td3_bank';
let bank = [];

function loadBank() {
    try {
        const raw = sessionStorage.getItem(BANK_KEY);
        if (raw) bank = JSON.parse(raw);
    } catch (_) { /* corrupt */ }
}
function saveBank() {
    try {
        sessionStorage.setItem(BANK_KEY, JSON.stringify(bank));
    } catch (_) { /* quota */ }
}
loadBank();

// Change listeners - called with (patternChanged: boolean)
const listeners = [];

export function onChange(fn) { listeners.push(fn); }

function notify(patternChanged = false) {
    save();
    listeners.forEach(fn => fn(patternChanged));
}

// Persistence
function save() {
    try {
        sessionStorage.setItem(STORAGE_KEY, JSON.stringify({
            pattern, group, patternNum, side, bpm, liveUpdate,
            sliceEnabled, sliceText, bankSize,
        }));
    } catch (_) { /* quota exceeded or private mode */ }
}

function load() {
    try {
        const raw = sessionStorage.getItem(STORAGE_KEY);
        if (raw) {
            const data = JSON.parse(raw);
            if (data.pattern && data.pattern.steps && data.pattern.steps.length === 16) {
                pattern = data.pattern;
                group = data.group || 1;
                patternNum = data.patternNum || 1;
                side = data.side || 'A';
                bpm = data.bpm || ENV_BPM;
                liveUpdate = !!data.liveUpdate;
                sliceEnabled = !!data.sliceEnabled;
                sliceText = data.sliceText || '';
                bankSize = data.bankSize || ENV_BANK_SIZE;
                hydratedFromStorage = true;
            }
        }
    } catch (_) { /* corrupt data */ }
}

// Getters
export function getPattern() { return pattern; }
export function getStep(i) { return pattern.steps[i]; }
export function getActiveSteps() { return pattern.active_steps; }
export function getTriplet() { return pattern.triplet; }
export function getGroup() { return group; }
export function getPatternNum() { return patternNum; }
export function getSide() { return side; }
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

// Note index helpers
export function noteIndex(name) { return NOTE_NAMES.indexOf(name); }
export function noteName(idx) { return NOTE_NAMES[Math.max(0, Math.min(12, idx))]; }

// Setters
export function setPattern(p) { pattern = p; notify(true); }
export function setStep(i, s) { pattern.steps[i] = s; notify(true); }
export function setActiveSteps(n) { pattern.active_steps = Math.max(1, Math.min(16, n)); notify(true); }
export function setTriplet(v) { pattern.triplet = v; notify(true); }
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

export function pushToBank(pat) {
    bank.push(JSON.parse(JSON.stringify(pat)));
    saveBank();
    notify();
}

// Apply TD3_CONFIG.env defaults to the startup state. Only overrides values
// that the user's sessionStorage did NOT already carry - a remembered pattern
// or a previously typed BPM wins over the env file.
export function setDefaultsFromEnv(cfg) {
    if (!cfg) return;
    if (sessionStorage.getItem('td3_bpm') === null && typeof cfg.uiDefaultBpm === 'number') {
        bpm = Math.max(20, Math.min(300, cfg.uiDefaultBpm));
        sessionStorage.setItem('td3_bpm', String(bpm));
    }
    if (!hydratedFromStorage) {
        if (typeof cfg.uiDefaultTriplet === 'boolean') pattern.triplet = cfg.uiDefaultTriplet;
        if (typeof cfg.uiAutoSetLiveUpdate === 'boolean') liveUpdate = cfg.uiAutoSetLiveUpdate;
        if (typeof cfg.uiMaxBankHistorySize === 'number' && cfg.uiMaxBankHistorySize > 0) {
            bankSize = Math.max(1, Math.floor(cfg.uiMaxBankHistorySize));
        }
        save();
    }
}

export function clearBank() {
    bank = [];
    saveBank();
    notify();
}

// Step mutations
export function cycleTime(i) {
    const order = ['NORMAL', 'REST', 'TIE', 'TIE_REST'];
    const step = pattern.steps[i];
    const idx = order.indexOf(step.time);
    step.time = order[(idx + 1) % order.length];
    notify(true);
}

export function toggleAccent(i) {
    pattern.steps[i].accent = !pattern.steps[i].accent;
    notify(true);
}

export function toggleSlide(i) {
    pattern.steps[i].slide = !pattern.steps[i].slide;
    notify(true);
}

export function toggleTranspose(i, target) {
    const step = pattern.steps[i];
    step.transpose = (step.transpose === target) ? 'NORMAL' : target;
    notify(true);
}

export function resetPattern() {
    pattern = defaultPattern();
    notify(true);
}

export function changeNote(i, delta) {
    const step = pattern.steps[i];
    const idx = noteIndex(step.note);
    const next = Math.max(0, Math.min(12, idx + delta));
    step.note = noteName(next);
    notify(true);
}

export function setNote(i, noteName) {
    pattern.steps[i].note = noteName;
    notify(true);
}

export function setTime(i, time) {
    pattern.steps[i].time = time;
    notify(true);
}

/**
 * Transpose every step's note by ±1 semitone.
 * step.transpose (the octave flag) is intentionally preserved.
 */
export function transposePattern(delta) {
    transposeStepsInPlace(pattern.steps, delta);
    notify(true);
}

/** Rotate all 16 steps by `n` positions. Positive = forward, negative = backward. */
export function shiftSteps(n) {
    const len = 16;
    const shift = ((n % len) + len) % len; // normalize to 0..15
    if (shift === 0) return;
    const old = pattern.steps.map(s => ({ ...s }));
    for (let i = 0; i < len; i++) {
        pattern.steps[(i + shift) % len] = old[i];
    }
    notify(true);
}

// --- Snapshot for undo/redo ---

/** Return a deep copy of the undoable state. */
export function getSnapshot() {
    return JSON.parse(JSON.stringify({ pattern }));
}

/** Restore from a snapshot (from undo/redo). Does NOT trigger history recording. */
export function restoreSnapshot(snap, skipNotify) {
    if (snap.pattern && snap.pattern.steps && snap.pattern.steps.length === 16) {
        pattern = snap.pattern;
    }
    if (skipNotify) {
        save();
    } else {
        notify(true);
    }
}

// Init
load();
