// Generic scale-role analysis.
//
// Given a root (pitch class 0..11) and a scale object with `.intervals`
// (semitones from root, 0..11), this module derives:
//   - which pitch classes act as STABLE tones (root, perfect fifth, third)
//   - which act as COLOR tones (the non-stable scale-identity members)
//   - which act as TENSION tones (subset of color: tritone, b2, etc.)
//   - which act as WEAK_STABLE (tritone / aug-fifth substituting for a
//     missing perfect fifth)
//   - the absolute pitch list partitioned by role across the TD-3's
//     three-octave addressable range
//
// The classification is data-driven - it only inspects scale.intervals.


import { buildScalePitches } from './magic-pitch-encoding.js';

export const ROLE_STABLE      = 'STABLE';
export const ROLE_WEAK_STABLE = 'WEAK_STABLE';
export const ROLE_COLOR       = 'COLOR';
export const ROLE_TENSION     = 'TENSION';

const INTERVAL_NAMES = [
    'P1', 'b2', '2', 'b3', '3', '4', 'b5', '5', 'b6', '6', 'b7', '7',
];

/** Human-readable label for an interval 0..11. */
export function intervalName(iv) {
    return INTERVAL_NAMES[((iv % 12) + 12) % 12];
}

/**
 * Classify a single scale interval into a role. The classification
 * inspects sibling intervals to decide whether a tone substitutes for a
 * missing perfect fifth or a missing major third.
 */
export function classifyInterval(iv, intervals) {
    const has = (n) => intervals.includes(n);
    if (iv === 0) return ROLE_STABLE;            // root
    if (iv === 7) return ROLE_STABLE;            // perfect fifth
    if (iv === 4) return ROLE_STABLE;            // major third
    if (iv === 3 && !has(4)) return ROLE_STABLE; // minor third (only when no major third)

    // Tritone / aug-fifth as fifth substitutes when the perfect fifth is
    // absent. They keep some "anchor" role but never feel as resolved as
    // a real fifth - hence WEAK_STABLE.
    if (iv === 6 && !has(7)) return ROLE_WEAK_STABLE;
    if (iv === 8 && !has(7)) return ROLE_WEAK_STABLE;

    // Everything else: tension-leaning intervals first, then plain color.
    if (iv === 1) return ROLE_TENSION;           // b2 - most distinctive tension
    if (iv === 6) return ROLE_TENSION;           // tritone (when 7 is also present)

    return ROLE_COLOR;
}

/** Pitch class 0..11 from any signed integer pitch. */
export function pitchPc(p) {
    return ((p % 12) + 12) % 12;
}

/**
 * Analyze a scale rooted at `root`. Returns a plain data object - no DOM,
 * no module state, no RNG. Safe to call repeatedly.
 *
 * @param {number} root Pitch class 0..11
 * @param {{intervals:number[], id?:string, name?:string}} scale
 * @returns {object} analysis (see fields documented inline)
 */
export function analyzeScale(root, scale) {
    if (!scale || !Array.isArray(scale.intervals) || scale.intervals.length === 0) {
        return emptyAnalysis(root);
    }
    const rootPc = ((root % 12) + 12) % 12;
    const intervals = [...scale.intervals];

    const degrees = intervals.map((iv) => {
        const role = classifyInterval(iv, intervals);
        const pc = ((rootPc + iv) % 12 + 12) % 12;
        return { interval: iv, pc, role, name: intervalName(iv) };
    });

    const byRole = {
        [ROLE_STABLE]:      new Set(),
        [ROLE_WEAK_STABLE]: new Set(),
        [ROLE_COLOR]:       new Set(),
        [ROLE_TENSION]:     new Set(),
    };
    for (const d of degrees) byRole[d.role].add(d.pc);

    const allPitches = buildScalePitches(rootPc, scale);
    const partition = { all: allPitches, stable: [], weakStable: [], color: [], tension: [] };
    for (const p of allPitches) {
        const pc = pitchPc(p);
        if (byRole[ROLE_STABLE].has(pc))           partition.stable.push(p);
        else if (byRole[ROLE_WEAK_STABLE].has(pc)) partition.weakStable.push(p);
        else if (byRole[ROLE_TENSION].has(pc))     partition.tension.push(p);
        else if (byRole[ROLE_COLOR].has(pc))       partition.color.push(p);
    }

    return {
        rootPc,
        scaleId:   scale.id   || null,
        scaleName: scale.name || null,
        intervals,
        pcs:        new Set(degrees.map(d => d.pc)),
        degrees,
        stablePcs:      byRole[ROLE_STABLE],
        weakStablePcs:  byRole[ROLE_WEAK_STABLE],
        colorPcs:       byRole[ROLE_COLOR],
        tensionPcs:     byRole[ROLE_TENSION],
        pitches:        partition,
        pitchRole:      (p) => roleOf(pitchPc(p), byRole),
        isStablePitch:  (p) => byRole[ROLE_STABLE].has(pitchPc(p)),
        isColorPitch:   (p) => byRole[ROLE_COLOR].has(pitchPc(p)),
        isTensionPitch: (p) => byRole[ROLE_TENSION].has(pitchPc(p)),
        isRootPitch:    (p) => pitchPc(p) === rootPc,
    };
}

function roleOf(pc, byRole) {
    if (byRole[ROLE_STABLE].has(pc))      return ROLE_STABLE;
    if (byRole[ROLE_WEAK_STABLE].has(pc)) return ROLE_WEAK_STABLE;
    if (byRole[ROLE_TENSION].has(pc))     return ROLE_TENSION;
    if (byRole[ROLE_COLOR].has(pc))       return ROLE_COLOR;
    return null;
}

function emptyAnalysis(root) {
    const empty = new Set();
    return {
        rootPc: ((root % 12) + 12) % 12,
        scaleId: null, scaleName: null,
        intervals: [],
        pcs: empty,
        degrees: [],
        stablePcs: empty, weakStablePcs: empty, colorPcs: empty, tensionPcs: empty,
        pitches: { all: [], stable: [], weakStable: [], color: [], tension: [] },
        pitchRole: () => null,
        isStablePitch: () => false,
        isColorPitch: () => false,
        isTensionPitch: () => false,
        isRootPitch: () => false,
    };
}
