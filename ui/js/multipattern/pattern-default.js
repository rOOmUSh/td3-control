// Shared defaults and "is-default" detection for main-page patterns.
//
// Kept as a tiny, pure module with no sessionStorage/DOM dependencies so it
// can be imported by both the runtime state module and the Node-side
// unit tests. A separate file also means the single-pattern migration path
// and the LOAD ALL "unsaved changes" confirm can reuse the same detector.

const DEFAULT_NOTE = 'C';
const DEFAULT_TRANSPOSE = 'NORMAL';
const DEFAULT_TIME = 'NORMAL';
const STEP_COUNT = 16;

export function defaultStep() {
    return {
        note: DEFAULT_NOTE,
        transpose: DEFAULT_TRANSPOSE,
        accent: false,
        slide: false,
        time: DEFAULT_TIME,
    };
}

export function defaultPattern() {
    return {
        active_steps: STEP_COUNT,
        triplet: false,
        steps: Array.from({ length: STEP_COUNT }, defaultStep),
    };
}

/**
 * True when every field of `step` matches the factory default.
 * Used by isPatternDefault and by the LOAD ALL overwrite-confirm guard.
 */
export function isStepDefault(step) {
    if (!step || typeof step !== 'object') return false;
    return step.note === DEFAULT_NOTE
        && step.transpose === DEFAULT_TRANSPOSE
        && step.accent === false
        && step.slide === false
        && step.time === DEFAULT_TIME;
}

/**
 * True when `pattern` is structurally default: 16 steps, active_steps===16,
 * triplet===false, and all 16 steps are default. The LOAD ALL warning uses
 * this - any non-default pattern means the user has unsaved edits and we
 * must prompt before overwriting.
 */
export function isPatternDefault(pattern) {
    if (!pattern || typeof pattern !== 'object') return false;
    if (pattern.active_steps !== STEP_COUNT) return false;
    if (pattern.triplet !== false) return false;
    if (!Array.isArray(pattern.steps) || pattern.steps.length !== STEP_COUNT) return false;
    for (let i = 0; i < STEP_COUNT; i++) {
        if (!isStepDefault(pattern.steps[i])) return false;
    }
    return true;
}

/** Deep-clone a pattern. Matches progression-state's convention. */
export function clonePattern(pattern) {
    return JSON.parse(JSON.stringify(pattern));
}
