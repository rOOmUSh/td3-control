// Home / Movement / Approach - the shared skeleton every archetype builds on.
//
// Formula:
//   start-of-bar  → HOME           (the tonal center, e.g. C#)
//   middle of bar → MOVEMENT       (2-4 scale-degree notes from a weighted pool)
//   end of bar    → APPROACH→HOME  (semitone below home, leading back to step 0)
//
// This module never decides WHERE those slots live (that's each archetype's
// ritм-маска decision). It only decides WHICH NOTE to place once a slot has
// been designated home / movement / approach. Keeps archetype files short
// and factors out the one set of pitch-selection rules so a bugfix in, say,
// the movement-weighting affects every archetype uniformly.
//
// Pure: no DOM, no state. Deterministic given the RNG.

const NOTE_NAMES = ['C','C#','D','D#','E','F','F#','G','G#','A','A#','B','C^'];

/** Normalize a pitch class to 0..11. */
function normPc(pc) { return ((pc % 12) + 12) % 12; }

/** Convert a pitch class to its NOTE_NAMES entry (lower octave; bass range). */
export function pcToNoteName(pc) { return NOTE_NAMES[normPc(pc)]; }

/** Return the scale's pitch classes at the given root (order-preserved). */
export function scaleDegreesPc(root, scaleIntervals) {
    if (!Array.isArray(scaleIntervals)) return [];
    return scaleIntervals.map(i => normPc(root + i));
}

/**
 * Movement candidate pool - a scale-degree pitch-class array weighted toward
 * strong-function degrees so a uniform random pick naturally biases to 5 and
 * 3 (the harmonic core) without needing extra selection code inside archetypes.
 *
 * Weights (by scale-degree index, roughly):
 *   1 (tonic)        excluded - that is HOME, not MOVEMENT
 *   2               1   color tone
 *   b3 / 3          3   strong - characteristic of mode
 *   4               2   subdominant approach
 *   5               4   strongest non-tonic
 *   6 / b6          2   color / dark
 *   b7 / 7          3   dominant preparation
 *
 * If the scale has fewer than 7 degrees (pentatonics, whole-tone), the pool
 * just uses whatever degrees exist - weights map by position in the array.
 */
const DEFAULT_WEIGHTS = [0, 1, 3, 2, 4, 2, 3];

export function movementCandidates(root, scaleIntervals, { weights = DEFAULT_WEIGHTS } = {}) {
    const pcs = scaleDegreesPc(root, scaleIntervals);
    const pool = [];
    for (let d = 0; d < pcs.length; d++) {
        const w = d < weights.length ? weights[d] : 1;
        for (let k = 0; k < w; k++) pool.push(pcs[d]);
    }
    // Fallback when the weight table ate every non-tonic (e.g. all-zero
    // weights on a weird scale): include every non-tonic pc once.
    if (pool.length === 0) {
        for (let d = 1; d < pcs.length; d++) pool.push(pcs[d]);
    }
    return pool;
}

/**
 * Approach note - semitone below home. Canonical acid lead-back: step 15
 * plays (home - 1), step 0 hits home.
 *
 * A rare alternate (upper approach = home + 1) is also supported for
 * archetypes that want a descending resolution; archetypes pick which.
 */
export function approachBelowPc(homePc) { return normPc(homePc - 1); }
export function approachAbovePc(homePc) { return normPc(homePc + 1); }

/**
 * A "strong" in-scale neighbor for archetypes that want to walk TO home
 * from inside the scale rather than via chromatic approach. Falls back to
 * the 5th when the scale is weird.
 */
export function diatonicApproachPc(homePc, root, scaleIntervals) {
    const pcs = new Set(scaleDegreesPc(root, scaleIntervals));
    // Prefer 2 (a diatonic step above home) or b7 (a step below home).
    const below = normPc(homePc - 2);
    const above = normPc(homePc + 2);
    if (pcs.has(below)) return below;
    if (pcs.has(above)) return above;
    return normPc(root + 7); // fifth as universal fallback
}

// --- Pattern-step builders ---------------------------------------------------

export function restStep(fallbackNoteName = 'C') {
    return {
        note: fallbackNoteName,
        transpose: 'NORMAL',
        accent: false,
        slide: false,
        time: 'REST',
    };
}

export function noteStep(noteName, { accent = false, slide = false, transpose = 'NORMAL' } = {}) {
    return { note: noteName, transpose, accent, slide, time: 'NORMAL' };
}

/**
 * Weighted-random pick from a pool (array with repetitions expressing weight).
 * rng is the shared `{next: () => number}` contract used across the bassline
 * codebase.
 */
export function pickFromPool(pool, rng) {
    if (!pool || pool.length === 0) return null;
    const idx = Math.floor(rng.next() * pool.length);
    return pool[Math.max(0, Math.min(pool.length - 1, idx))];
}

export { NOTE_NAMES };
