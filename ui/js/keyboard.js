// Keyboard control for step editing.
// Key mappings are loaded from /api/config/keyboard (keyboard-config.json on disk).

import * as state from './multipattern/multipattern-state.js';
import { api } from './api.js';

// Runtime mappings built from config
let keyToNote = {};       // e.key (lowercase) → note name
let actionKeys = {};      // action name → key string
let autoStepKeys = new Set();

let setStatus = () => {};
let triggerRandomize = () => {};
let triggerPlay = () => {};

// Empty fallback (used if config fetch fails - keyboard won't work until config is fixed)
const EMPTY_CONFIG = { notes: {}, actions: {} };

export function init(statusFn, randomizeFn, playFn) {
    setStatus = statusFn;
    triggerRandomize = randomizeFn;
    triggerPlay = playFn;

    loadConfig().then(() => {
        document.addEventListener('keydown', handleKeyDown);
    });
}

/** Reload config from backend (called after settings save). */
export function reload() {
    return loadConfig();
}

async function loadConfig() {
    let config;
    try {
        config = await api.getKeyboardConfig();
    } catch (_) {
        config = EMPTY_CONFIG;
    }
    applyConfig(config);
}

function applyConfig(config) {
    const notes = config.notes || {};
    actionKeys = config.actions || {};

    // Build reverse map: key (lowercase) → note name
    keyToNote = {};
    for (const [note, key] of Object.entries(notes)) {
        keyToNote[key.toLowerCase()] = note;
    }

    // Build auto-step keys set (all keys that modify the current step)
    autoStepKeys = new Set([
        ...Object.values(notes).map(k => k.toLowerCase()),
        actionKeys.accent?.toLowerCase(),
        actionKeys.slide?.toLowerCase(),
        actionKeys.transpose_up?.toLowerCase(),
        actionKeys.transpose_down?.toLowerCase(),
        actionKeys.rest?.toLowerCase(),
    ].filter(Boolean));
}

function matchKey(key, actionName) {
    const bound = actionKeys[actionName];
    if (!bound) return false;
    return key === bound.toLowerCase();
}

function handleKeyDown(e) {
    if (!state.isKbEditEnabled()) return;

    // Ignore when typing in inputs/textareas
    const tag = e.target.tagName;
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;

    const key = e.key.toLowerCase();
    const step = state.getSelectedStep();

    // Ctrl combo: live_toggle
    const liveToggle = actionKeys.live_toggle || 'ctrl+l';
    if (liveToggle.startsWith('ctrl+')) {
        const liveKey = liveToggle.slice(5).toLowerCase();
        if (e.ctrlKey && key === liveKey) {
            e.preventDefault();
            state.setLiveUpdate(!state.isLiveUpdate());
            setStatus(state.isLiveUpdate() ? 'Live update ON' : 'Live update OFF');
            return;
        }
    }

    // Don't process ctrl/alt combos for other keys
    if (e.ctrlKey || e.altKey || e.metaKey) return;

    // Play
    if (matchKey(key, 'play')) {
        e.preventDefault();
        triggerPlay();
        return;
    }

    // Randomize
    if (matchKey(key, 'randomize')) {
        e.preventDefault();
        triggerRandomize();
        return;
    }

    // Navigation (no auto-step)
    if (matchKey(key, 'prev_step')) {
        e.preventDefault();
        state.setSelectedStep(step - 1);
        return;
    }
    if (matchKey(key, 'next_step')) {
        e.preventDefault();
        state.setSelectedStep(step + 1);
        return;
    }

    // Transpose
    if (matchKey(key, 'transpose_up')) {
        e.preventDefault();
        state.toggleTranspose(step, 'UP');
        previewCurrentNote(step);
        maybeAdvance(key);
        return;
    }
    if (matchKey(key, 'transpose_down')) {
        e.preventDefault();
        state.toggleTranspose(step, 'DOWN');
        previewCurrentNote(step);
        maybeAdvance(key);
        return;
    }

    // Accent
    if (matchKey(key, 'accent')) {
        e.preventDefault();
        state.toggleAccent(step);
        previewCurrentNote(step);
        maybeAdvance(key);
        return;
    }

    // Slide
    if (matchKey(key, 'slide')) {
        e.preventDefault();
        state.toggleSlide(step);
        previewCurrentNote(step);
        maybeAdvance(key);
        return;
    }

    // Rest (primary key or alt code)
    if (matchKey(key, 'rest') || e.code === (actionKeys.rest_alt || 'Numpad0')) {
        e.preventDefault();
        const stepData = state.getStep(step);
        if (stepData.time === 'REST' || stepData.time === 'TIE_REST') {
            state.setTime(step, 'NORMAL');
        } else {
            state.setTime(step, 'REST');
        }
        maybeAdvance(key);
        return;
    }

    // Note keys
    const noteName = keyToNote[key];
    if (noteName) {
        e.preventDefault();
        const stepData = state.getStep(step);

        if (stepData.note === noteName && stepData.time !== 'REST' && stepData.time !== 'TIE_REST') {
            // Same note pressed again - toggle REST
            state.setTime(step, 'REST');
        } else if (stepData.note === noteName && (stepData.time === 'REST' || stepData.time === 'TIE_REST')) {
            // Same note, currently REST - turn ON
            state.setTime(step, 'NORMAL');
            previewCurrentNote(step);
        } else {
            // Different note - set it and ensure ON
            state.setNote(step, noteName);
            if (stepData.time === 'REST' || stepData.time === 'TIE_REST') {
                state.setTime(step, 'NORMAL');
            }
            previewCurrentNote(step);
        }
        maybeAdvance(key);
        return;
    }
}

function maybeAdvance(key) {
    if (!state.isAutoStepFwd()) return;
    if (!autoStepKeys.has(key.toLowerCase())) return;
    const cur = state.getSelectedStep();
    const next = (cur + 1) % 16;
    state.setSelectedStep(next);
    // Step wrap (15 → 0) also advances focus across patterns -
    // next checked if any are checked, else next-in-list.
    if (cur === 15 && next === 0) {
        state.advanceFocusAfterWrap();
    }
}

function previewCurrentNote(stepIndex) {
    if (!state.isConnected()) return;
    const stepData = state.getStep(stepIndex);
    if (stepData.time === 'REST' || stepData.time === 'TIE_REST') return;
    api.notePreview(stepData.note, stepData.transpose, stepData.accent).catch(() => {});
}
