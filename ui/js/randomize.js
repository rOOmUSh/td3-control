// Scale-aware pattern randomizer with musical weighting, slider control, and slice mode.

import * as state from './multipattern/multipattern-state.js';
import { loadScales, getScale, scaleNotes, populateScaleSelect } from './scales.js';
import { wireMagicToggle, isMagicEnabled } from './magic-randomizer/magic-state.js';
import {
    runMagicFull, runMagicSlice,
    BUDGET_SINGLE, BUDGET_BULK,
} from './magic-randomizer/magic-randomizer.js';
import { clamp } from './shared/math.js';

const rootSelect = document.getElementById('root-select');
const scaleSelect = document.getElementById('scale-select');
const btnRandomize = document.getElementById('btn-randomize');

const sliderNote = document.getElementById('slider-note');
const sliderSlide = document.getElementById('slider-slide');
const sliderAcc = document.getElementById('slider-acc');
const sliderUd = document.getElementById('slider-ud');
const sliderNoteVal = document.getElementById('slider-note-val');
const sliderSlideVal = document.getElementById('slider-slide-val');
const sliderAccVal = document.getElementById('slider-acc-val');
const sliderUdVal = document.getElementById('slider-ud-val');

export async function init() {
    await loadScales();
    populateScaleSelect(scaleSelect);
    // Honour the boot-time default scale hint stamped by app-config.js
    // (data-default-scale comes from UI_RAND_DEFAULT_SCALE in TD3_CONFIG.env).
    const defaultScale = scaleSelect.dataset.defaultScale;
    if (defaultScale && [...scaleSelect.options].some(o => o.value === defaultScale)) {
        scaleSelect.value = defaultScale;
    }
    btnRandomize.addEventListener('click', dispatchRandomize);
    wireMagicToggle();

    sliderNote.addEventListener('input', () => { sliderNoteVal.textContent = sliderNote.value + '%'; });
    sliderSlide.addEventListener('input', () => { sliderSlideVal.textContent = sliderSlide.value + '%'; });
    sliderAcc.addEventListener('input', () => { sliderAccVal.textContent = sliderAcc.value + '%'; });
    if (sliderUd && sliderUdVal) {
        sliderUdVal.textContent = sliderUd.value + '%';
        sliderUd.addEventListener('input', () => { sliderUdVal.textContent = sliderUd.value + '%'; });
    }
}

/**
 * Parse slice notation (1-indexed) into an array of 0-indexed step indices.
 *
 * Supported syntax (comma-separated):
 *   5       → step 5
 *   9-12    → steps 9,10,11,12
 *   -8      → steps 1 through 8
 *   13-     → steps 13 through 16
 *   1,5-8,13- → combined
 */
export function parseSliceNotation(input) {
    const result = new Set();
    const parts = input.split(',').map(s => s.trim()).filter(s => s);
    for (const part of parts) {
        const dashIdx = part.indexOf('-');
        if (dashIdx === -1) {
            const n = parseInt(part);
            if (!isNaN(n) && n >= 1 && n <= 16) result.add(n - 1);
        } else if (dashIdx === 0) {
            // leading dash: -8 → 1..8
            const end = parseInt(part.slice(1));
            if (!isNaN(end)) {
                for (let i = 1; i <= Math.min(end, 16); i++) result.add(i - 1);
            }
        } else if (dashIdx === part.length - 1) {
            // trailing dash: 13- → 13..16
            const start = parseInt(part.slice(0, -1));
            if (!isNaN(start)) {
                for (let i = Math.max(start, 1); i <= 16; i++) result.add(i - 1);
            }
        } else {
            // range: 9-12
            const a = parseInt(part.slice(0, dashIdx));
            const b = parseInt(part.slice(dashIdx + 1));
            if (!isNaN(a) && !isNaN(b)) {
                for (let i = Math.max(a, 1); i <= Math.min(b, 16); i++) result.add(i - 1);
            }
        }
    }
    return [...result].filter(i => i >= 0 && i < 16).sort((a, b) => a - b);
}

function shuffle(arr) {
    for (let i = arr.length - 1; i > 0; i--) {
        const j = Math.floor(Math.random() * (i + 1));
        [arr[i], arr[j]] = [arr[j], arr[i]];
    }
    return arr;
}

/**
 * Sidebar randomizer scope: when one or more patterns are checkboxed,
 * every sidebar randomize action (main RANDOMIZE, RST, SL, AC - including
 * slice mode) applies to the checked set instead of only the focused
 * pattern. With nothing checked we fall back to the focused pattern, which
 * matches the pre-checkbox behaviour.
 *
 * Per-card RAND buttons bypass this helper - they always target the card
 * they live on (see `randomizeCategoryForPattern` usage in multipattern-list.js).
 */
function sidebarTargets() {
    const checked = state.getCheckedArray();
    if (checked && checked.length > 0) return checked;
    const f = state.getFocusedIdx();
    return (f === null || f === undefined) ? [] : [f];
}

function randomize() {
    const root = parseInt(rootSelect.value);
    const scale = getScale(scaleSelect.value);
    const notes = scaleNotes(root, scale);
    if (notes.length === 0) return;

    const notePercent = parseInt(sliderNote.value) / 100;
    const slidePercent = parseInt(sliderSlide.value) / 100;
    const accPercent = parseInt(sliderAcc.value) / 100;

    const sliced = state.isSliceEnabled() && state.getSliceText().trim();
    const targets = sidebarTargets();
    for (const patIdx of targets) {
        const pattern = state.getPattern(patIdx);
        if (!pattern) continue;
        if (sliced) {
            sliceRandomize(patIdx, pattern, notes, scale, notePercent, slidePercent, accPercent);
        } else {
            fullRandomize(patIdx, pattern, notes, scale, notePercent, slidePercent, accPercent);
        }
    }
}

// MAGIC pipeline branch - kept structurally parallel to randomize() above
// so the legacy code path stays byte-for-byte unchanged. When the MAGIC
// checkbox is on, the click handler routes here; both branches converge
// on the same state.setPattern() + state.pushToBank() calls.
function magicRandomize() {
    const root = parseInt(rootSelect.value);
    const scale = getScale(scaleSelect.value);
    if (!scale || !Array.isArray(scale.intervals) || scale.intervals.length === 0) return;

    const notePercent  = parseInt(sliderNote.value) / 100;
    const slidePercent = parseInt(sliderSlide.value) / 100;
    const accPercent   = parseInt(sliderAcc.value) / 100;

    const sliced = state.isSliceEnabled() && state.getSliceText().trim();
    const targets = sidebarTargets();
    // Single-shot budget when only one pattern is targeted; bulk budget
    // when several are checkboxed at once.
    const attempts = targets.length > 1 ? BUDGET_BULK : BUDGET_SINGLE;

    for (const patIdx of targets) {
        const pattern = state.getPattern(patIdx);
        if (!pattern) continue;

        let result;
        if (sliced) {
            const sliceIndices = parseSliceNotation(state.getSliceText());
            if (sliceIndices.length === 0) continue;
            result = runMagicSlice({
                root, scale,
                prevPattern: pattern,
                sliceIndices,
                notePercent, slidePercent, accPercent,
                attempts,
            });
        } else {
            result = runMagicFull({
                root, scale,
                notePercent, slidePercent, accPercent,
                attempts,
                activeSteps: pattern.active_steps,
                triplet: pattern.triplet,
            });
        }

        const newPattern = {
            active_steps: pattern.active_steps,
            triplet: pattern.triplet,
            steps: result.steps,
        };
        state.setPattern(patIdx, newPattern);
        state.pushToBank(newPattern);
    }
}

function dispatchRandomize() {
    if (isMagicEnabled()) magicRandomize();
    else randomize();
}

/**
 * Shuffle a single attribute family (rests / slides / accents / up-down) on
 * the current pattern, using its slider percentage and the current slicer
 * window. Called directly from the RST / SL / AC / U|D sidebar buttons.
 *
 *   'rst' → reshuffles which steps are REST vs active at NOTE%.
 *   'sl'  → reshuffles slides on active steps at SL%.
 *   'ac'  → reshuffles accents on active steps at AC%.
 *   'ud'  → reshuffles UP/DOWN transpose flags across all 16 steps at U|D%.
 */
export function randomizeCategory(kind) {
    for (const patIdx of sidebarTargets()) {
        randomizeCategoryForPattern(patIdx, kind);
    }
}

/**
 * Same shuffle as `randomizeCategory`, but targets a specific pattern by
 * index. Called from per-card RAND buttons on the multipattern card list.
 * Leaves the focused-pattern API intact for the sidebar buttons.
 */
export function randomizeCategoryForPattern(patIdx, kind) {
    if (!Number.isInteger(patIdx) || patIdx < 0) return;
    const pattern = state.getPattern(patIdx);
    if (!pattern) return;
    const steps = pattern.steps.map(s => ({ ...s }));

    let indices;
    if (state.isSliceEnabled() && state.getSliceText().trim()) {
        indices = parseSliceNotation(state.getSliceText());
        if (indices.length === 0) return;
    } else {
        indices = Array.from({ length: 16 }, (_, i) => i);
    }

    if (kind === 'rst') {
        // Re-roll REST vs active across the targeted window. Newly-RESTed
        // steps also lose slide/accent (matches the default REST shape);
        // newly-active steps keep their stored note/accent/slide and only
        // have their REST-family time cleared.
        const notePercent = parseInt(sliderNote.value) / 100;
        const activeCount = Math.round(indices.length * notePercent);
        const shuffled = shuffle([...indices]);
        const newActive = new Set(shuffled.slice(0, activeCount));
        for (const i of indices) {
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
    } else if (kind === 'sl' || kind === 'ac') {
        // SL/AC act on active steps only (REST/TIE_REST are skipped - a
        // slide or accent on a rest would be a silent flag with no audible
        // effect on the TD-3).
        const activeIndices = indices.filter(i => steps[i].time !== 'REST' && steps[i].time !== 'TIE_REST');
        if (activeIndices.length === 0) return;

        const percent = kind === 'sl'
            ? parseInt(sliderSlide.value) / 100
            : parseInt(sliderAcc.value) / 100;
        const count = Math.round(activeIndices.length * percent);
        const shuffled = shuffle([...activeIndices]);
        const set = new Set(shuffled.slice(0, count));
        const field = kind === 'sl' ? 'slide' : 'accent';
        for (const i of indices) {
            steps[i][field] = set.has(i);
        }
    } else if (kind === 'ud') {
        // U|D writes to step.transpose only. UP and DOWN are mutually
        // exclusive on a single step, so each chosen index gets a 50/50
        // coin flip between the two; unchosen indices clear back to
        // 'NORMAL'. Operates on every step in the window regardless of
        // REST state - a transpose flag survives REST and is revealed
        // when the user un-rests the step.
        const percent = sliderUd ? parseInt(sliderUd.value) / 100 : 0;
        const count = Math.round(indices.length * percent);
        const shuffled = shuffle([...indices]);
        const flagged = new Set(shuffled.slice(0, count));
        for (const i of indices) {
            if (flagged.has(i)) {
                steps[i].transpose = Math.random() < 0.5 ? 'UP' : 'DOWN';
            } else {
                steps[i].transpose = 'NORMAL';
            }
        }
    } else {
        return;
    }

    const newPattern = { active_steps: pattern.active_steps, triplet: pattern.triplet, steps };
    state.setPattern(patIdx, newPattern);
    state.pushToBank(newPattern);
}

function fullRandomize(patIdx, pattern, notes, scale, notePercent, slidePercent, accPercent) {
    const totalSteps = 16;
    const activeCount = Math.round(totalSteps * notePercent);
    const positions = shuffle(Array.from({ length: totalSteps }, (_, i) => i));
    const activeSet = new Set(positions.slice(0, activeCount));
    const activePositions = positions.slice(0, activeCount);
    const slidePositions = new Set(shuffle([...activePositions]).slice(0, Math.round(activeCount * slidePercent)));
    const accPositions = new Set(shuffle([...activePositions]).slice(0, Math.round(activeCount * accPercent)));

    const steps = [];
    let prevNoteIdx = Math.floor(notes.length / 2);

    for (let i = 0; i < totalSteps; i++) {
        if (!activeSet.has(i)) {
            steps.push({
                note: state.noteName(notes[prevNoteIdx] || 0),
                transpose: 'NORMAL', accent: false, slide: false, time: 'REST',
            });
            continue;
        }
        const noteIdx = chooseNextNote(notes, prevNoteIdx, i, scale);
        prevNoteIdx = noteIdx;
        let transpose = 'NORMAL';
        const r = Math.random();
        if (r < 0.12) transpose = 'UP';
        else if (r < 0.24) transpose = 'DOWN';

        steps.push({
            note: state.noteName(notes[noteIdx]),
            transpose,
            accent: accPositions.has(i),
            slide: slidePositions.has(i),
            time: 'NORMAL',
        });
    }

    const newPattern = { active_steps: pattern.active_steps, triplet: pattern.triplet, steps };
    state.setPattern(patIdx, newPattern);
    state.pushToBank(newPattern);
}

function sliceRandomize(patIdx, pattern, notes, scale, notePercent, slidePercent, accPercent) {
    const sliceIndices = parseSliceNotation(state.getSliceText());
    if (sliceIndices.length === 0) return;

    // Deep copy current steps
    const steps = pattern.steps.map(s => ({ ...s }));

    const activeCount = Math.round(sliceIndices.length * notePercent);
    const shuffled = shuffle([...sliceIndices]);
    const activeSet = new Set(shuffled.slice(0, activeCount));
    const activeSlice = shuffled.slice(0, activeCount);
    const slideSet = new Set(shuffle([...activeSlice]).slice(0, Math.round(activeCount * slidePercent)));
    const accSet = new Set(shuffle([...activeSlice]).slice(0, Math.round(activeCount * accPercent)));

    // Find context note: scan left from first slice index
    let prevNoteIdx = Math.floor(notes.length / 2);
    const firstSlice = sliceIndices[0];
    for (let i = firstSlice - 1; i >= 0; i--) {
        if (steps[i].time !== 'REST' && steps[i].time !== 'TIE_REST') {
            const ni = state.noteIndex(steps[i].note);
            // Find closest note in scale
            let best = 0, bestDist = 999;
            for (let j = 0; j < notes.length; j++) {
                const d = Math.abs(notes[j] - ni);
                if (d < bestDist) { bestDist = d; best = j; }
            }
            prevNoteIdx = best;
            break;
        }
    }

    for (const i of sliceIndices) {
        if (!activeSet.has(i)) {
            steps[i] = {
                note: state.noteName(notes[prevNoteIdx] || 0),
                transpose: 'NORMAL', accent: false, slide: false, time: 'REST',
            };
            continue;
        }
        const noteIdx = chooseNextNote(notes, prevNoteIdx, i, scale);
        prevNoteIdx = noteIdx;
        let transpose = 'NORMAL';
        const r = Math.random();
        if (r < 0.12) transpose = 'UP';
        else if (r < 0.24) transpose = 'DOWN';

        steps[i] = {
            note: state.noteName(notes[noteIdx]),
            transpose,
            accent: accSet.has(i),
            slide: slideSet.has(i),
            time: 'NORMAL',
        };
    }

    const newPattern = { active_steps: pattern.active_steps, triplet: pattern.triplet, steps };
    state.setPattern(patIdx, newPattern);
    state.pushToBank(newPattern);
}

function chooseNextNote(notes, prevIdx, stepIndex, scale) {
    const isStrongBeat = (stepIndex % 4 === 0);
    const len = notes.length;

    if (isStrongBeat && Math.random() < 0.6) {
        const stableIndices = findStableIndices(notes, scale);
        if (stableIndices.length > 0) {
            return stableIndices[Math.floor(Math.random() * stableIndices.length)];
        }
    }

    const r = Math.random();
    if (r < 0.60) {
        const dir = Math.random() < 0.5 ? 1 : -1;
        return clamp(prevIdx + dir, 0, len - 1);
    } else if (r < 0.85) {
        return prevIdx;
    } else {
        const leap = (Math.floor(Math.random() * 3) + 2) * (Math.random() < 0.5 ? 1 : -1);
        return clamp(prevIdx + leap, 0, len - 1);
    }
}

function findStableIndices(notes, scale) {
    const stableIntervals = new Set();
    stableIntervals.add(0);
    if (scale.intervals.includes(4)) stableIntervals.add(4);
    else if (scale.intervals.includes(3)) stableIntervals.add(3);
    if (scale.intervals.includes(7)) stableIntervals.add(7);

    const result = [];
    for (let i = 0; i < notes.length; i++) {
        const pc = notes[i] % 12;
        for (const interval of stableIntervals) {
            if (scale.intervals.indexOf(interval) !== -1) {
                const rootPc = notes[0] % 12;
                if (pc === (rootPc + interval) % 12) {
                    result.push(i);
                    break;
                }
            }
        }
    }
    return result.length > 0 ? result : [0];
}

