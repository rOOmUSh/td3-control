// Repair a near-good candidate. Capped at 6 actions per candidate; one
// revalidation + rescore is performed by the caller after repair returns.
//
// The repairer never rewrites a whole pattern - it makes targeted swaps
// to fix specific reasons reported by the validator:
//
//   missing root        → replace one weak-stable strong beat with root
//   pc-domination       → replace one occurrence of the dominant pc with
//                         a stable neighbor that's not already over-used
//   pitch-domination    → same idea, but for absolute pitches (octave shift)
//   distinct-pcs low    → replace one stable strong beat with a color tone
//   weak strong beat    → swap an unstable strong-beat pitch for the
//                         nearest stable pitch
//   missing root in
//     first half /
//     last quarter      → place root at the nearest active strong beat in
//                         the missing zone
//   bad final loop      → swap last active pitch for one closer to first
//   excessive run       → break up a 3+ run with a neighbor
//
// All repairs operate on candidate.pitches in place and re-encode steps
// at the end. The action log is returned for the debug report.

import { computeMetrics, RUN_LIMIT } from './magic-validator.js';
import { encodeStepsFromPitches } from './magic-generator.js';
import { pitchPc } from './magic-scale-analysis.js';
import { nearestPitch } from './magic-pitch-encoding.js';
import { pickOne } from './magic-rng.js';

const STRONG_BEATS = [0, 4, 8, 12];
const FIRST_HALF_END = 8;
const LAST_QUARTER_START = 12;
export const MAX_REPAIR_ACTIONS = 6;

/**
 * Attempt to repair a candidate. Returns a new candidate with `actions:
 * string[]` describing what was done (or empty array if no changes).
 *
 * The function does NOT revalidate or rescore - the caller decides
 * whether to accept the repair. After repair we run one
 * fresh validate+score pass.
 */
export function repairCandidate(candidate, analysis, rng) {
    const pitches = [...candidate.pitches];
    const mask    = [...candidate.mask];
    const actions = [];

    let metrics = computeMetrics({ pitches, mask }, analysis);
    let budget = MAX_REPAIR_ACTIONS;

    // 1. Missing root → seed a strong-beat with root.
    if (budget > 0 && metrics.rootCount === 0 && metrics.activeCount >= 4) {
        if (insertRoot(pitches, mask, analysis, /*zone*/ 'first-or-last')) {
            actions.push('insert-root');
            budget--;
            metrics = computeMetrics({ pitches, mask }, analysis);
        }
    }

    // 2. Root in first half missing.
    if (budget > 0 && !metrics.rootInFirstHalf && metrics.rootCount > 0) {
        if (insertRoot(pitches, mask, analysis, 'first-half')) {
            actions.push('place-root-first-half');
            budget--;
            metrics = computeMetrics({ pitches, mask }, analysis);
        }
    }

    // 3. Root in last quarter missing.
    if (budget > 0 && !metrics.rootInLastQuarter && metrics.rootCount > 0) {
        if (insertRoot(pitches, mask, analysis, 'last-quarter')) {
            actions.push('place-root-last-quarter');
            budget--;
            metrics = computeMetrics({ pitches, mask }, analysis);
        }
    }

    // 4. PC domination - replace one over-used pc occurrence with a
    // neighbor.
    while (budget > 0) {
        const cap = capForActive(metrics.activeCount);
        if (metrics.maxPcCount <= cap) break;
        if (!reduceDominantPc(pitches, mask, analysis, rng)) break;
        actions.push('reduce-dominant-pc');
        budget--;
        metrics = computeMetrics({ pitches, mask }, analysis);
    }

    // 5. Pitch domination - same shape but at the absolute-pitch level.
    while (budget > 0) {
        const cap = capForActiveAbs(metrics.activeCount);
        if (metrics.maxAbsPitchCount <= cap) break;
        if (!reduceDominantAbsPitch(pitches, mask, analysis, rng)) break;
        actions.push('reduce-dominant-abs');
        budget--;
        metrics = computeMetrics({ pitches, mask }, analysis);
    }

    // 6. 3-or-more-run reduction (not strictly invalid yet, but the
    // anti-stuck score punishes these - repair while we have budget).
    while (budget > 0 && metrics.maxRunLen >= RUN_LIMIT) {
        if (!breakRun(pitches, mask, analysis, rng)) break;
        actions.push('break-run');
        budget--;
        metrics = computeMetrics({ pitches, mask }, analysis);
    }

    // 7. Weak strong-beat swap - replace one unstable strong beat with
    // its nearest stable.
    if (budget > 0 && metrics.activeCount >= 4) {
        if (improveWeakStrongBeat(pitches, mask, analysis)) {
            actions.push('improve-weak-strong-beat');
            budget--;
            metrics = computeMetrics({ pitches, mask }, analysis);
        }
    }

    // 8. Bad final-loop leap.
    if (budget > 0 && metrics.loopMovement != null && metrics.loopMovement > 12 && !metrics.loopLandsOnStable) {
        if (smoothFinalLoop(pitches, mask, analysis)) {
            actions.push('smooth-final-loop');
            budget--;
        }
    }

    const steps = encodeStepsFromPitches(pitches, mask, null);
    return { mask, pitches, steps, actions };
}

// ---------------------------------------------------------------------------
// Repair primitives - each returns true on success, false if no-op.
// ---------------------------------------------------------------------------

function activeIndices(mask) {
    const out = [];
    for (let i = 0; i < mask.length; i++) if (mask[i]) out.push(i);
    return out;
}

function insertRoot(pitches, mask, analysis, zone) {
    if (analysis.pitches.stable.length === 0) return false;
    const rootPitches = analysis.pitches.all.filter(p => analysis.isRootPitch(p));
    if (rootPitches.length === 0) return false;

    const idxs = activeIndices(mask);
    const inZone = (i) => {
        if (zone === 'first-half')   return i < FIRST_HALF_END;
        if (zone === 'last-quarter') return i >= LAST_QUARTER_START;
        return true; // 'first-or-last'
    };
    // Prefer strong beats, then any active step in zone.
    const candidates = idxs.filter(i => inZone(i));
    if (candidates.length === 0) return false;
    candidates.sort((a, b) => Number(STRONG_BEATS.includes(b)) - Number(STRONG_BEATS.includes(a)));
    const target = candidates[0];

    // Choose a root pitch close to the target's existing register.
    const replacement = nearestPitch(pitches[target], rootPitches);
    if (replacement === pitches[target]) return false;
    pitches[target] = replacement;
    return true;
}

function reduceDominantPc(pitches, mask, analysis, rng) {
    // Find the dominant pc and replace one of its occurrences with the
    // nearest under-represented stable or color tone.
    const idxs = activeIndices(mask);
    const counts = new Map();
    for (const i of idxs) {
        const pc = pitchPc(pitches[i]);
        counts.set(pc, (counts.get(pc) || 0) + 1);
    }
    let topPc = null, topCount = 0;
    for (const [pc, c] of counts) {
        if (c > topCount) { topCount = c; topPc = pc; }
    }
    if (topPc === null) return false;

    const occurrences = idxs.filter(i => pitchPc(pitches[i]) === topPc);
    if (occurrences.length === 0) return false;

    // Skip occurrences sitting on strong beats - those carry the song.
    const replaceable = occurrences.filter(i => !STRONG_BEATS.includes(i));
    const target = replaceable.length > 0 ? pickOne(rng, replaceable) : pickOne(rng, occurrences);

    // Pool: scale pitches whose pc is NOT the dominant one and not at the
    // current absolute-count cap.
    const pool = analysis.pitches.all.filter(p => pitchPc(p) !== topPc);
    if (pool.length === 0) return false;
    const replacement = nearestPitch(pitches[target], pool);
    if (replacement == null || replacement === pitches[target]) return false;
    pitches[target] = replacement;
    return true;
}

function reduceDominantAbsPitch(pitches, mask, analysis, rng) {
    const idxs = activeIndices(mask);
    const counts = new Map();
    for (const i of idxs) counts.set(pitches[i], (counts.get(pitches[i]) || 0) + 1);
    let topP = null, topCount = 0;
    for (const [p, c] of counts) if (c > topCount) { topCount = c; topP = p; }
    if (topP === null) return false;

    const occurrences = idxs.filter(i => pitches[i] === topP);
    const replaceable = occurrences.filter(i => !STRONG_BEATS.includes(i));
    const target = replaceable.length > 0 ? pickOne(rng, replaceable) : pickOne(rng, occurrences);

    // Try the same pitch class one octave shifted - preserves musical
    // sense without changing the note name role.
    const samePcOtherOctave = analysis.pitches.all.filter(p => pitchPc(p) === pitchPc(topP) && p !== topP);
    if (samePcOtherOctave.length > 0) {
        pitches[target] = nearestPitch(pitches[target], samePcOtherOctave);
        return true;
    }
    // Fallback: any neighbor pitch.
    const pool = analysis.pitches.all.filter(p => p !== topP);
    if (pool.length === 0) return false;
    pitches[target] = nearestPitch(pitches[target], pool);
    return true;
}

function breakRun(pitches, mask, analysis, rng) {
    const idxs = activeIndices(mask);
    let runLen = 1;
    let runPitch = idxs.length > 0 ? pitches[idxs[0]] : null;
    for (let n = 1; n < idxs.length; n++) {
        if (pitches[idxs[n]] === runPitch) {
            runLen++;
            if (runLen >= RUN_LIMIT) {
                // Replace the run's tail with a neighbor.
                const target = idxs[n];
                const neighbors = analysis.pitches.all.filter(p => p !== runPitch && Math.abs(p - runPitch) <= 4);
                if (neighbors.length === 0) return false;
                pitches[target] = pickOne(rng, neighbors);
                return true;
            }
        } else {
            runLen = 1;
            runPitch = pitches[idxs[n]];
        }
    }
    return false;
}

function improveWeakStrongBeat(pitches, mask, analysis) {
    const stable = analysis.pitches.stable;
    if (stable.length === 0) return false;
    const activeStrong = STRONG_BEATS.filter(i => mask[i]);
    for (const i of activeStrong) {
        if (!analysis.isStablePitch(pitches[i])) {
            pitches[i] = nearestPitch(pitches[i], stable);
            return true;
        }
    }
    return false;
}

function smoothFinalLoop(pitches, mask, analysis) {
    const idxs = activeIndices(mask);
    if (idxs.length < 2) return false;
    const last = idxs[idxs.length - 1];
    const first = pitches[idxs[0]];
    // Pull last toward first within an octave; prefer stable.
    const target = first;
    const window = analysis.pitches.all.filter(p => Math.abs(p - target) <= 7);
    if (window.length === 0) return false;
    const stableInWindow = window.filter(p => analysis.isStablePitch(p));
    pitches[last] = nearestPitch(target, stableInWindow.length > 0 ? stableInWindow : window);
    return true;
}

// ---------------------------------------------------------------------------
// Domination caps mirror the validator's thresholds.
// ---------------------------------------------------------------------------

function capForActive(active) {
    if (active >= 12) return 7;
    if (active >= 8)  return 6;
    if (active >= 4)  return 5;
    return active;
}

function capForActiveAbs(active) {
    if (active >= 12) return 6;
    if (active >= 8)  return 5;
    if (active >= 4)  return 4;
    return active;
}
