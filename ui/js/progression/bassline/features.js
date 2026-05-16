// Pattern feature extraction - reads a 16-step acid pattern and returns the
// numeric signals that drive archetype selection and per-archetype decisions
// (home placement, pause density budget, approach-note timing).
//
// Pure: no DOM, no state, no RNG. Deterministic given an input pattern.

const NOTE_NAMES = ['C','C#','D','D#','E','F','F#','G','G#','A','A#','B','C^'];
const ANCHOR_STEPS = [0, 4, 8, 12];

function noteToPc(noteName) {
    const idx = NOTE_NAMES.indexOf(noteName);
    if (idx < 0) return 0;
    return idx % 12; // C^ collapses to C pitch class
}

export function isRestStep(step) {
    return !step || step.time === 'REST' || step.time === 'TIE_REST';
}

/**
 * Extract a feature vector from a 16-step pattern. Optionally compares notes
 * against a known scale to estimate chromatic density.
 *
 * @param {Object} pattern                 - {steps:[16], ...}
 * @param {Object} [opts]
 * @param {number} [opts.root]             - 0..11; only needed with scalePcs
 * @param {Set<number>} [opts.scalePcs]    - pitch classes of the chosen scale
 *                                           at `root`. Used for chromaFraction.
 * @returns {{
 *   density:number, activeCount:number, activeIdx:number[],
 *   anchorsActive:number, syncopation:number, chromaFraction:number,
 *   accentDensity:number, contourAvg:number,
 *   pitchClasses:Set<number>, uniquePitchCount:number,
 *   lastActiveIdx:number, endsOnRoot:boolean
 * }}
 */
export function extractFeatures(pattern, opts = {}) {
    if (!pattern || !Array.isArray(pattern.steps) || pattern.steps.length !== 16) {
        throw new Error('extractFeatures: pattern must have 16 steps');
    }

    const { root, scalePcs } = opts;
    const steps = pattern.steps;
    const activeIdx = [];
    const pitchClasses = new Set();

    for (let i = 0; i < 16; i++) {
        if (!isRestStep(steps[i])) {
            activeIdx.push(i);
            pitchClasses.add(noteToPc(steps[i].note));
        }
    }

    const density = activeIdx.length / 16;
    const anchorsActive = ANCHOR_STEPS.filter(s => !isRestStep(steps[s])).length;

    // Syncopation: fraction of active steps that sit on odd (offbeat) steps.
    // High value → acid lead is syncopated → bass should anchor harder.
    let odd = 0;
    for (const i of activeIdx) if (i % 2 === 1) odd++;
    const syncopation = activeIdx.length > 0 ? odd / activeIdx.length : 0;

    // Chromatic fraction: unique pcs that fall outside the scale.
    let outOfScale = 0;
    if (scalePcs instanceof Set && scalePcs.size > 0) {
        for (const pc of pitchClasses) if (!scalePcs.has(pc)) outOfScale++;
    }
    const chromaFraction = pitchClasses.size > 0 ? outOfScale / pitchClasses.size : 0;

    let accented = 0;
    for (const i of activeIdx) if (steps[i].accent) accented++;
    const accentDensity = activeIdx.length > 0 ? accented / activeIdx.length : 0;

    // Contour: average absolute semitone jump between consecutive active notes.
    // High value → melody leaps; bass should stay anchored and contrast.
    // Low value → melody is smooth; bass has room to move.
    let contourSum = 0, contourCount = 0;
    for (let k = 1; k < activeIdx.length; k++) {
        const prev = NOTE_NAMES.indexOf(steps[activeIdx[k - 1]].note);
        const cur  = NOTE_NAMES.indexOf(steps[activeIdx[k]].note);
        if (prev >= 0 && cur >= 0) {
            contourSum += Math.abs(cur - prev);
            contourCount++;
        }
    }
    const contourAvg = contourCount > 0 ? contourSum / contourCount : 0;

    const lastActiveIdx = activeIdx.length > 0 ? activeIdx[activeIdx.length - 1] : -1;
    const endsOnRoot = (typeof root === 'number' && lastActiveIdx >= 0)
        ? (noteToPc(steps[lastActiveIdx].note) === ((root % 12) + 12) % 12)
        : false;

    return {
        density,
        activeCount: activeIdx.length,
        activeIdx,
        anchorsActive,
        syncopation,
        chromaFraction,
        accentDensity,
        contourAvg,
        pitchClasses,
        uniquePitchCount: pitchClasses.size,
        lastActiveIdx,
        endsOnRoot,
    };
}

export { NOTE_NAMES, ANCHOR_STEPS, noteToPc };
