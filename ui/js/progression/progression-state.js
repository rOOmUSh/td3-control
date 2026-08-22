// Progression state management - holds 4 patterns + timeline + playback state.

import {
    transposeStepsInPlace,
    transposeBasslineSetInPlace,
} from '../shared/transpose-step.js';
import { envInt, envBool } from '../td3-env.js';
import {
    readGatePercent,
    writeGatePercent,
} from '../shared/gate-control.js';
import {
    readMidiChannel,
    writeMidiChannel,
} from '../shared/midi-channel-control.js';
import {
    readFilterCutoff,
    writeFilterCutoff,
} from '../shared/filter-cutoff-control.js';
import {
    readPitchBend,
    writePitchBend,
} from '../shared/pitch-bend-control.js';
import { laneState, writeLaneState } from '../shared/step-lanes.js';
import { createTripletMorphSession } from '../shared/triplet-morph-session.js';
import { canonicalPatternText } from '../shared/pattern-canonical.js';
import { editableSteps } from '../shared/triplet-morph-editing.js';

const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B', 'C^'];
const STORAGE_KEY = 'td3_progression';
const BANK_KEY = 'td3_bank'; // shared with main page
// Own key, not shared with the Control page: a morph session is only
// valid against the exact canonical sources it was opened on, and the
// two pages hold different pattern sets.
const TRIPLET_MORPH_SESSION_KEY = 'td3_progression_triplet_morph_session_v1';

// Boot defaults from window.TD3_CONFIG_ENV - no JS-side fallback literals.
const ENV_BPM         = envInt('uiDefaultBpm');
const ENV_TRIPLET     = envBool('uiDefaultTriplet');
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

// --- State ---

let patterns = [defaultPattern(), defaultPattern(), defaultPattern(), defaultPattern()];
let timeline = [1,1,1,1,2,2,2,2,3,3,3,3,4,4,4,4];
let group = 1;
let patternNum = 1;
let side = 'A';
let bpm = parseFloat(sessionStorage.getItem('td3_bpm')) || ENV_BPM;
let gatePercent = readGatePercent();
// `MIDI_DEVICE_CHANNEL` is the startup default; a value chosen from the
// transport bar overrides it for the session. Shared with the Control
// page through the same session storage key.
let midiChannel = readMidiChannel(envInt('midiDeviceChannel'));
// TD-3-MO device controls. Support is reported by the server per
// session and is never persisted; the knob values are session-scoped.
let deviceControlsSupported = false;
let filterCutoff = readFilterCutoff();
let pitchBend = readPitchBend();
let playing = sessionStorage.getItem('td3_playing') === 'true';
let connected = sessionStorage.getItem('td3_midi_connected') === 'true';
let liveUpdate = !!ENV_LIVE_UPDATE;
let progressionLabel = '';
let progressionDegrees = [];
let progressionRoot = null;      // 0..11 or null when no progression has been generated
let progressionScaleId = null;   // scale id string or null

// Bassline v2 - each acid pattern carries 5 archetype variants plus the
// currently-active archetype key. All live in state so UI can swap without
// regenerating, and SAVE PACKAGE can emit all five per pattern.
//   basslines[i] = { pedal:Pattern, rootPulse:Pattern, offbeat:Pattern,
//                    shadow:Pattern, arpeggio:Pattern } | null
// Cleared to 4×null on RANDOMIZE and re-filled when the v2 generator runs.
let basslines = [null, null, null, null];
let activeArchetypes = ['rootPulse', 'rootPulse', 'rootPulse', 'rootPulse'];

// Playback
let currentTimelinePos = 0;
let currentStepInPattern = 0;
let activePatternIndex = 0;

// Tracks whether load() rehydrated from a persisted blob. setDefaultsFromEnv
// only overrides module defaults when this is still false.
let hydratedFromStorage = false;

// Bank (shared with main page)
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

// --- Listeners ---

const listeners = [];
export function onChange(fn) { listeners.push(fn); }

function notify(patternChanged = false) {
    // A source-changing action while morph audition is active resets the
    // amount before observers run: exact canonical identity is the
    // session's validity condition.
    if (morphSession.isActive() && (patternChanged || morphSession.sourcesChanged())) {
        // At the 100 endpoint the user may edit surviving notes, so the
        // session re-baselines onto the edited source instead of being
        // discarded. Anywhere below 100 nothing can edit through the UI,
        // so a source change is an outside mutation and resets.
        if (morphSession.getPercent() >= 100) morphSession.rebaseline();
        else morphSession.clear();
    }
    save();
    listeners.forEach(fn => fn(patternChanged));
}

// --- Triplet morph session (transient, versioned storage) ---
//
// Session identity covers the four acid patterns, the page's canonical
// editable source. Generated basslines always carry 16 active steps and
// inherit the acid pattern's triplet flag, so they are structurally
// eligible whenever the acid patterns are; keeping them out of the
// identity set means swapping a bassline archetype does not discard the
// morph session. Each audition call still resolves its own amount, so
// an ineligible pattern is never silently morphed.

const morphSession = createTripletMorphSession({
    storageKey: TRIPLET_MORPH_SESSION_KEY,
    getPatterns: () => patterns,
});

export function getTripletMorphPercent() { return morphSession.getPercent(); }
export function isTripletMorphActive() { return morphSession.isActive(); }
export function getTripletMorphSession() { return morphSession.getSession(); }
export function isTripletMorphSourceEligible() {
    return morphSession.isSourceEligible();
}

export function resetTripletMorphSession() {
    if (!morphSession.isActive() && !morphSession.getSession()) return;
    morphSession.clear();
    notify();
}

/**
 * Set the morph amount. A positive amount is refused while any acid
 * pattern is ineligible; the caller reads the unchanged value back to
 * detect the refusal. Returning to 0 discards the derived session.
 */
export function setTripletMorphPercent(value) {
    if (morphSession.setPercent(value)) notify();
}

export function getTripletMorphPlan(canonicalText) {
    return morphSession.getPlan(canonicalText);
}

export function setTripletMorphPlan(canonicalText, plan) {
    morphSession.setPlan(canonicalText, plan);
}
/**
 * Steps the user may edit for one pattern right now. Null means every
 * step; an empty set means nothing is editable.
 */
export function getTripletMorphEditableSteps(patIdx) {
    const pattern = getPattern(patIdx);
    const plan = pattern ? morphSession.getPlan(canonicalPatternText(pattern)) : null;
    return editableSteps(morphSession.getPercent(), plan);
}

export function isTripletMorphStepEditable(patIdx, step) {
    const allowed = getTripletMorphEditableSteps(patIdx);
    return allowed === null || allowed.has(Number(step));
}


// --- Persistence ---

function save() {
    try {
        sessionStorage.setItem(STORAGE_KEY, JSON.stringify({
            patterns, timeline, group, patternNum, side, bpm, liveUpdate,
            progressionLabel, progressionDegrees, progressionRoot, progressionScaleId,
            basslines, activeArchetypes,
        }));
    } catch (_) { /* quota */ }
}

function load() {
    try {
        const raw = sessionStorage.getItem(STORAGE_KEY);
        if (!raw) return;
        const d = JSON.parse(raw);
        if (d.patterns && d.patterns.length === 4 && d.patterns[0].steps?.length === 16) {
            patterns = d.patterns;
        }
        if (d.timeline && Array.isArray(d.timeline)) timeline = d.timeline;
        group = d.group || 1;
        patternNum = d.patternNum || 1;
        side = d.side || 'A';
        bpm = d.bpm || ENV_BPM;
        liveUpdate = !!d.liveUpdate;
        progressionLabel = d.progressionLabel || '';
        progressionDegrees = d.progressionDegrees || [];
        progressionRoot = (typeof d.progressionRoot === 'number' && d.progressionRoot >= 0 && d.progressionRoot <= 11)
            ? d.progressionRoot : null;
        progressionScaleId = (typeof d.progressionScaleId === 'string' && d.progressionScaleId)
            ? d.progressionScaleId : null;
        if (Array.isArray(d.basslines) && d.basslines.length === 4) {
            basslines = d.basslines;
        }
        if (Array.isArray(d.activeArchetypes) && d.activeArchetypes.length === 4) {
            activeArchetypes = d.activeArchetypes;
        }
        hydratedFromStorage = true;
    } catch (_) { /* corrupt */ }
}

// --- Getters ---

export function getPatterns() { return patterns; }
export function getPattern(idx) { return patterns[idx]; }
export function getStep(patIdx, stepIdx) { return patterns[patIdx].steps[stepIdx]; }
export function getActiveSteps(patIdx) { return patterns[patIdx || 0].active_steps; }
export function getTriplet(patIdx) { return patterns[patIdx || 0].triplet; }
export function getTimeline() { return timeline; }
export function getTimelineLength() { return timeline.length; }
export function getGroup() { return group; }
export function getPatternNum() { return patternNum; }
export function getSide() { return side; }
export function getBpm() { return bpm; }
export function getGatePercent() { return gatePercent; }
export function getMidiChannel() { return midiChannel; }
export function isDeviceControlsSupported() { return deviceControlsSupported; }
/** Per-step lanes of pattern `i`, always fully populated. */
export function getLanes(i) { return laneState(patterns[i]); }
export function getFilterCutoff() { return filterCutoff; }
export function getPitchBend() { return pitchBend; }
export function isPlaying() { return playing; }
export function isConnected() { return connected; }
export function isLiveUpdate() { return liveUpdate; }
export function getProgressionLabel() { return progressionLabel; }
export function getProgressionDegrees() { return progressionDegrees; }
export function getProgressionRoot() { return progressionRoot; }
export function getProgressionScaleId() { return progressionScaleId; }

// --- Bassline v2 getters/setters ---
export function getBasslines() { return basslines; }
export function getBasslinesFor(idx) { return basslines[idx] || null; }
export function getActiveArchetypes() { return activeArchetypes; }
export function getActiveArchetype(idx) { return activeArchetypes[idx] || 'rootPulse'; }
export function getActiveBassline(idx) {
    const set = basslines[idx];
    if (!set) return null;
    const key = activeArchetypes[idx] || 'rootPulse';
    return set[key] || null;
}
export function setBasslines(all, defaults) {
    if (!Array.isArray(all) || all.length !== 4) return;
    basslines = all;
    if (Array.isArray(defaults) && defaults.length === 4) {
        activeArchetypes = defaults.slice();
    }
    save();
    notify();
}
export function setActiveArchetype(idx, key) {
    if (idx < 0 || idx > 3) return;
    const valid = ['pedal', 'rootPulse', 'offbeat', 'shadow', 'arpeggio'];
    if (!valid.includes(key)) return;
    activeArchetypes[idx] = key;
    save();
    notify();
}
export function clearBasslines() {
    basslines = [null, null, null, null];
    activeArchetypes = ['rootPulse', 'rootPulse', 'rootPulse', 'rootPulse'];
    save();
    notify();
}
export function getCurrentTimelinePos() { return currentTimelinePos; }
export function getCurrentStepInPattern() { return currentStepInPattern; }
export function getActivePatternIndex() { return activePatternIndex; }
export function getNoteNames() { return NOTE_NAMES; }
export function getBank() { return bank; }
export function getBankCount() { return bank.length; }

// Note helpers
export function noteIndex(name) { return NOTE_NAMES.indexOf(name); }
export function noteName(idx) { return NOTE_NAMES[Math.max(0, Math.min(12, idx))]; }

// --- Setters ---

export function setPatterns(p) { patterns = p; notify(true); }
export function setPattern(idx, p) { patterns[idx] = p; notify(true); }
export function setStep(patIdx, stepIdx, s) { patterns[patIdx].steps[stepIdx] = s; notify(true); }
export function setTimeline(t) { timeline = t; save(); notify(); }
export function setTimelineLength(n) {
    const len = Math.max(1, Math.min(128, n));
    if (len > timeline.length) {
        while (timeline.length < len) timeline.push(0);
    } else {
        timeline.length = len;
    }
    save();
    notify();
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
export function setGatePercent(value) {
    const next = writeGatePercent(value);
    if (next === gatePercent) return;
    gatePercent = next;
    notify();
}
export function setMidiChannel(value) {
    const next = writeMidiChannel(value);
    if (next === midiChannel) return;
    midiChannel = next;
    notify();
}
// The status poll calls this every tick; an unchanged value is dropped
// so the poll never triggers a re-render on its own.
export function setDeviceControlsSupported(value) {
    const next = !!value;
    if (next === deviceControlsSupported) return;
    deviceControlsSupported = next;
    notify();
}
export function setFilterCutoff(value) {
    const next = writeFilterCutoff(value);
    if (next === filterCutoff) return;
    filterCutoff = next;
    notify();
}
export function setPitchBend(value) {
    const next = writePitchBend(value);
    if (next === pitchBend) return;
    pitchBend = next;
    notify();
}
/** Replace the lanes of pattern `i` and persist. */
export function setLanes(i, lanes) {
    const pattern = patterns[i];
    if (!pattern) return;
    writeLaneState(pattern, lanes);
    notify();
}
export function setPlaying(v) { playing = v; sessionStorage.setItem('td3_playing', v ? 'true' : 'false'); notify(); }
export function setConnected(v) { connected = v; sessionStorage.setItem('td3_midi_connected', v ? 'true' : 'false'); notify(); }
export function setLiveUpdate(v) { liveUpdate = v; notify(); }
export function setProgressionLabel(v) { progressionLabel = v; save(); }
export function setProgressionDegrees(v) { progressionDegrees = v; save(); }
export function setProgressionRoot(v) {
    progressionRoot = (typeof v === 'number' && v >= 0 && v <= 11) ? v : null;
    save();
}
export function setProgressionScaleId(v) {
    progressionScaleId = (typeof v === 'string' && v) ? v : null;
    save();
}
export function setCurrentTimelinePos(v) { currentTimelinePos = v; }
export function setCurrentStepInPattern(v) { currentStepInPattern = v; }
export function setActivePatternIndex(v) { activePatternIndex = v; }

// Step mutations for individual pattern rows
export function cycleTime(patIdx, stepIdx) {
    const order = ['NORMAL', 'REST', 'TIE', 'TIE_REST'];
    const step = patterns[patIdx].steps[stepIdx];
    const idx = order.indexOf(step.time);
    step.time = order[(idx + 1) % order.length];
    notify(true);
}

export function toggleAccent(patIdx, stepIdx) {
    patterns[patIdx].steps[stepIdx].accent = !patterns[patIdx].steps[stepIdx].accent;
    notify(true);
}

export function toggleSlide(patIdx, stepIdx) {
    patterns[patIdx].steps[stepIdx].slide = !patterns[patIdx].steps[stepIdx].slide;
    notify(true);
}

export function toggleTranspose(patIdx, stepIdx, target) {
    const step = patterns[patIdx].steps[stepIdx];
    step.transpose = (step.transpose === target) ? 'NORMAL' : target;
    notify(true);
}

export function changeNote(patIdx, stepIdx, delta) {
    const step = patterns[patIdx].steps[stepIdx];
    const idx = noteIndex(step.note);
    const next = Math.max(0, Math.min(12, idx + delta));
    step.note = noteName(next);
    notify(true);
}

// --- Shift steps for a single pattern ---
export function shiftPatternSteps(patIdx, n) {
    const steps = patterns[patIdx].steps;
    const len = 16;
    const shift = ((n % len) + len) % len;
    if (shift === 0) return;
    const old = steps.map(s => ({ ...s }));
    for (let i = 0; i < len; i++) {
        steps[(i + shift) % len] = old[i];
    }
    notify(true);
}

/**
 * Transpose every step of pattern `patIdx` by ±1 semitone. Also transposes
 * all 5 bassline archetypes for this pattern if they have been generated -
 * otherwise the basslines would keep playing in the old key. Leaves
 * step.transpose (the octave flag) unchanged.
 */
export function transposePatternAt(patIdx, delta) {
    transposeStepsInPlace(patterns[patIdx].steps, delta);
    transposeBasslineSetInPlace(basslines[patIdx], delta);
    notify(true);
}

/**
 * Transpose every step in all 4 patterns by ±1 semitone, plus any generated
 * bassline archetypes. Produces a single notify(true) so the history system
 * records one undo entry covering everything shifted.
 */
export function transposeAllPatterns(delta) {
    for (let p = 0; p < 4; p++) {
        transposeStepsInPlace(patterns[p].steps, delta);
        transposeBasslineSetInPlace(basslines[p], delta);
    }
    notify(true);
}

// --- Shift steps for all 4 patterns ---
export function shiftAllSteps(n) {
    const len = 16;
    const shift = ((n % len) + len) % len;
    if (shift === 0) return;
    for (let p = 0; p < 4; p++) {
        const steps = patterns[p].steps;
        const old = steps.map(s => ({ ...s }));
        for (let i = 0; i < len; i++) {
            steps[(i + shift) % len] = old[i];
        }
    }
    notify(true);
}

/**
 * Step indices a randomizer may rewrite for one pattern. At amount 0
 * this is every step, so randomizing behaves exactly as before; at the
 * 100 endpoint it is only the notes still visible in triplet mode.
 */
function morphEditableIndices(patIdx) {
    const all = Array.from({ length: 16 }, (_, i) => i);
    const allowed = getTripletMorphEditableSteps(patIdx);
    return allowed === null ? all : all.filter(i => allowed.has(i));
}

// --- Randomize slides on a single pattern ---
export function randomizeSlides(patIdx, slidePercent) {
    const steps = patterns[patIdx].steps;
    const editable = morphEditableIndices(patIdx);
    if (editable.length === 0) return;
    const active = editable.filter(
        i => steps[i].time !== 'REST' && steps[i].time !== 'TIE_REST',
    );
    const count = Math.round(active.length * slidePercent);
    const slideSet = new Set(shuffle([...active]).slice(0, count));
    for (const i of editable) {
        steps[i].slide = slideSet.has(i);
    }
    notify(true);
}

// --- Randomize accents on a single pattern ---
export function randomizeAccents(patIdx, accPercent) {
    const steps = patterns[patIdx].steps;
    const editable = morphEditableIndices(patIdx);
    if (editable.length === 0) return;
    const active = editable.filter(
        i => steps[i].time !== 'REST' && steps[i].time !== 'TIE_REST',
    );
    const count = Math.round(active.length * accPercent);
    const accSet = new Set(shuffle([...active]).slice(0, count));
    for (const i of editable) {
        steps[i].accent = accSet.has(i);
    }
    notify(true);
}

// --- Randomize rest-mask on a single pattern ---
// notePercent = fraction of the 16 steps that should be ACTIVE (not REST).
// Notes, slides, and accents on surviving active steps stay put; newly-active
// steps come out of REST with their stored note intact, and newly-rested
// steps have slide/accent cleared to match the default REST step semantics.
export function randomizeRests(patIdx, notePercent) {
    const steps = patterns[patIdx].steps;
    const editable = morphEditableIndices(patIdx);
    if (editable.length === 0) return;
    const activeCount = Math.round(editable.length * notePercent);
    const newActive = new Set(shuffle([...editable]).slice(0, activeCount));
    for (const i of editable) {
        if (newActive.has(i)) {
            if (steps[i].time === 'REST' || steps[i].time === 'TIE_REST') {
                steps[i].time = 'NORMAL';
            }
        } else {
            steps[i].time = 'REST';
            steps[i].slide = false;
            steps[i].accent = false;
        }
    }
    notify(true);
}

// --- Randomize UP/DOWN transpose flags on a single pattern ---
// udPercent = fraction of the 16 steps that should carry an UP/DOWN flag.
// UP and DOWN are mutually exclusive on a step, so each chosen index gets a
// 50/50 coin flip between the two; unchosen indices clear back to 'NORMAL'.
// Operates on every step regardless of REST state - a transpose flag survives
// REST and is revealed when the user un-rests the step.
export function randomizeUd(patIdx, udPercent) {
    const steps = patterns[patIdx].steps;
    const editable = morphEditableIndices(patIdx);
    if (editable.length === 0) return;
    const count = Math.round(editable.length * udPercent);
    const flagged = new Set(shuffle([...editable]).slice(0, count));
    for (const i of editable) {
        if (flagged.has(i)) {
            steps[i].transpose = Math.random() < 0.5 ? 'UP' : 'DOWN';
        } else {
            steps[i].transpose = 'NORMAL';
        }
    }
    notify(true);
}

// --- Randomize slides on all 4 patterns ---
export function randomizeSlidesAll(slidePercent) {
    for (let p = 0; p < 4; p++) randomizeSlides(p, slidePercent);
}

// --- Randomize accents on all 4 patterns ---
export function randomizeAccentsAll(accPercent) {
    for (let p = 0; p < 4; p++) randomizeAccents(p, accPercent);
}

// --- Randomize accents on all 4 patterns ---
export function randomizeRestsAll(notePercent) {
    for (let p = 0; p < 4; p++) randomizeRests(p, notePercent);
}

// --- Randomize UP/DOWN transpose flags on all 4 patterns ---
export function randomizeUdAll(udPercent) {
    for (let p = 0; p < 4; p++) randomizeUd(p, udPercent);
}

function shuffle(arr) {
    for (let i = arr.length - 1; i > 0; i--) {
        const j = Math.floor(Math.random() * (i + 1));
        [arr[i], arr[j]] = [arr[j], arr[i]];
    }
    return arr;
}

// --- Reset timeline to beginning ---
export function resetTimeline() {
    currentTimelinePos = 0;
    currentStepInPattern = 0;
    activePatternIndex = 0;
}

export function pushToBank(pat) {
    bank.push(JSON.parse(JSON.stringify(pat)));
    saveBank();
    notify();
}

// Apply TD3_CONFIG.env defaults to the startup state. Only overrides values
// that the user's sessionStorage did NOT already carry.
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
        save();
    }
}

/** Copy a progression pattern to the main page's focused pattern via sessionStorage. */
export function copyToMain(patIdx) {
    const pat = JSON.parse(JSON.stringify(patterns[patIdx]));
    try {
        const raw = sessionStorage.getItem('td3_multipattern');
        const blob = raw ? JSON.parse(raw) : null;
        const next = (blob && Array.isArray(blob.patterns) && blob.patterns.length > 0)
            ? blob
            : { patterns: [], focusedIdx: 0, checked: [], timeline: [], abMode: 'SERIAL',
                viewport: { group: 'ALL', side: 'ALL' } };
        if (!Array.isArray(next.patterns) || next.patterns.length === 0) {
            next.patterns = [pat];
            next.focusedIdx = 0;
            next.timeline = [1];
        } else {
            const idx = (typeof next.focusedIdx === 'number' && next.focusedIdx >= 0
                && next.focusedIdx < next.patterns.length) ? next.focusedIdx : 0;
            next.patterns[idx] = pat;
        }
        sessionStorage.setItem('td3_multipattern', JSON.stringify(next));
    } catch (_) { /* ignore */ }
}

// --- Snapshot for undo/redo ---

/** Return a deep copy of the undoable state. */
export function getSnapshot() {
    return JSON.parse(JSON.stringify({
        patterns, timeline, progressionLabel, progressionDegrees,
        progressionRoot, progressionScaleId,
    }));
}

/** Restore from a snapshot (from undo/redo). Does NOT trigger history recording. */
export function restoreSnapshot(snap, skipNotify) {
    if (snap.patterns && snap.patterns.length === 4) patterns = snap.patterns;
    if (snap.timeline) timeline = snap.timeline;
    if (snap.progressionLabel !== undefined) progressionLabel = snap.progressionLabel;
    if (snap.progressionDegrees !== undefined) progressionDegrees = snap.progressionDegrees;
    if (snap.progressionRoot !== undefined) progressionRoot = snap.progressionRoot;
    if (snap.progressionScaleId !== undefined) progressionScaleId = snap.progressionScaleId;
    if (skipNotify) {
        save();
    } else {
        notify(true);
    }
}

// --- Init ---

load();
morphSession.restore();
