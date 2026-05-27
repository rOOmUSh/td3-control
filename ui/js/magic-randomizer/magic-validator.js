// Hard rules + sparse-aware thresholds for magic candidates.
//
// Returns `{ passed, reasons: string[], metrics }`. A candidate that does
// not pass goes to the repairer (if it's close) or is dropped. The scorer
// runs only on validated candidates.
//
// Sparse threshold table (decided per the user's hand-off):
//   active 12-16 → root ≥ 2, distinct ≥ 4, root in first half AND last quarter
//   active  8-11 → root ≥ 2, distinct ≥ 3, root in first half OR last quarter
//   active  4-7  → root ≥ 1, distinct ≥ 2, no placement constraint
//   active  1-3  → content rules waived; only encoding + run-length checks

import { isPitchInRange } from './magic-pitch-encoding.js';
import { pitchPc } from './magic-scale-analysis.js';

const TOTAL_STEPS = 16;
const STRONG_BEATS = [0, 4, 8, 12];
const FIRST_HALF_END = 8;       // indices 0..7
const LAST_QUARTER_START = 12;  // indices 12..15
const RUN_LIMIT = 4;            // 4-in-a-row identical = hard reject

export function validateCandidate(candidate, analysis) {
    const reasons = [];
    const metrics = computeMetrics(candidate, analysis);

    // --- Always-on checks ---
    for (const r of metrics.encodingErrors) reasons.push(r);
    if (metrics.maxRunLen >= RUN_LIMIT) reasons.push(`run-${metrics.maxRunLen}-identical-pitches`);

    // --- Tier-specific content checks ---
    const tier = sparseTier(metrics.activeCount);
    if (tier !== 'micro') {
        if (metrics.rootCount < tier.minRoot) reasons.push(`root-count-${metrics.rootCount}-below-${tier.minRoot}`);
        if (metrics.distinctPcs < tier.minDistinct) reasons.push(`distinct-pcs-${metrics.distinctPcs}-below-${tier.minDistinct}`);
        if (metrics.maxPcCount > tier.maxPcCount) reasons.push(`pc-domination-${metrics.maxPcCount}-over-${tier.maxPcCount}`);
        if (metrics.maxAbsPitchCount > tier.maxAbsPitchCount) reasons.push(`pitch-domination-${metrics.maxAbsPitchCount}-over-${tier.maxAbsPitchCount}`);
        if (metrics.allStrongBeatsSamePitch) reasons.push('all-strong-beats-same-pitch');
        if (tier.requireRootPlacement && !metrics.rootInFirstHalf) reasons.push('no-root-in-first-half');
        if (tier.requireRootPlacement && !metrics.rootInLastQuarter) reasons.push('no-root-in-last-quarter');
        if (tier.requireRootEither && !metrics.rootInFirstHalf && !metrics.rootInLastQuarter) {
            reasons.push('no-root-in-first-half-or-last-quarter');
        }
    }

    // --- Final loop check (whenever there are at least 2 active steps) ---
    if (metrics.activeCount >= 2) {
        if (metrics.loopMovement !== null && metrics.loopMovement > 12 && !metrics.loopLandsOnStable) {
            reasons.push('bad-final-loop-leap-without-stable-landing');
        }
    }

    return { passed: reasons.length === 0, reasons, metrics };
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

export function computeMetrics(candidate, analysis) {
    const { pitches, mask } = candidate;
    const encodingErrors = [];
    const activeIdx = [];
    for (let i = 0; i < TOTAL_STEPS; i++) {
        if (mask[i]) activeIdx.push(i);
    }
    const activePitches = activeIdx.map(i => pitches[i]);
    for (const p of activePitches) {
        if (p == null || !isPitchInRange(p)) encodingErrors.push(`out-of-range-pitch-${p}`);
    }

    const pcCounts = new Map();
    const absCounts = new Map();
    for (const p of activePitches) {
        const pc = pitchPc(p);
        pcCounts.set(pc, (pcCounts.get(pc) || 0) + 1);
        absCounts.set(p, (absCounts.get(p) || 0) + 1);
    }
    const distinctPcs = pcCounts.size;
    let maxPcCount = 0;
    let maxAbsPitchCount = 0;
    for (const v of pcCounts.values()) maxPcCount = Math.max(maxPcCount, v);
    for (const v of absCounts.values()) maxAbsPitchCount = Math.max(maxAbsPitchCount, v);
    const rootCount = pcCounts.get(analysis.rootPc) || 0;

    // Run lengths over consecutive *active* pitches (rests don't count).
    let maxRunLen = 0;
    let runLen = 0;
    let runPitch = null;
    for (const p of activePitches) {
        if (p === runPitch) runLen++;
        else { runPitch = p; runLen = 1; }
        if (runLen > maxRunLen) maxRunLen = runLen;
    }

    // Strong beats: only count those that are active.
    const activeStrong = STRONG_BEATS.filter(i => mask[i]);
    const strongPitches = activeStrong.map(i => pitches[i]);
    const strongStableCount = strongPitches.filter(p => analysis.isStablePitch(p) || analysis.isRootPitch(p)).length;
    const allStrongBeatsSamePitch = activeStrong.length >= 2
        && strongPitches.every(p => p === strongPitches[0]);

    // Root placement zones.
    let rootInFirstHalf = false, rootInLastQuarter = false;
    for (const i of activeIdx) {
        if (analysis.isRootPitch(pitches[i])) {
            if (i < FIRST_HALF_END) rootInFirstHalf = true;
            if (i >= LAST_QUARTER_START) rootInLastQuarter = true;
        }
    }

    // Movement histogram + largest leap, computed in absolute semitones.
    const moves = [];
    let largestLeap = 0;
    for (let i = 1; i < activePitches.length; i++) {
        const d = Math.abs(activePitches[i] - activePitches[i - 1]);
        moves.push(d);
        if (d > largestLeap) largestLeap = d;
    }
    const movementBuckets = { repeat: 0, step: 0, smallLeap: 0, mediumLeap: 0, largeLeap: 0 };
    for (const d of moves) {
        if (d === 0) movementBuckets.repeat++;
        else if (d <= 2) movementBuckets.step++;
        else if (d <= 4) movementBuckets.smallLeap++;
        else if (d <= 7) movementBuckets.mediumLeap++;
        else movementBuckets.largeLeap++;
    }

    // Loop check: last → first active pitch.
    let loopMovement = null;
    let loopLandsOnStable = false;
    if (activePitches.length >= 2) {
        const last = activePitches[activePitches.length - 1];
        const first = activePitches[0];
        loopMovement = Math.abs(last - first);
        loopLandsOnStable = analysis.isStablePitch(first) || analysis.isRootPitch(first);
    }

    // Register: mean / span of active pitches.
    const mean = activePitches.length > 0
        ? activePitches.reduce((a, b) => a + b, 0) / activePitches.length
        : 0;
    const min = activePitches.length > 0 ? Math.min(...activePitches) : 0;
    const max = activePitches.length > 0 ? Math.max(...activePitches) : 0;

    return {
        activeCount: activeIdx.length,
        activeIdx,
        encodingErrors,
        rootCount,
        distinctPcs,
        maxPcCount,
        maxAbsPitchCount,
        maxRunLen,
        strongStableCount,
        allStrongBeatsSamePitch,
        rootInFirstHalf,
        rootInLastQuarter,
        movementBuckets,
        movementCount: moves.length,
        largestLeap,
        loopMovement,
        loopLandsOnStable,
        registerMean: mean,
        registerMin: min,
        registerMax: max,
    };
}

// ---------------------------------------------------------------------------
// Sparse threshold table
// ---------------------------------------------------------------------------

/** Returns the threshold tier for a given active-step count. */
export function sparseTier(active) {
    if (active >= 12) return {
        label: 'dense',
        minRoot: 2, minDistinct: 4,
        maxPcCount: 7, maxAbsPitchCount: 6,
        requireRootPlacement: true, requireRootEither: false,
    };
    if (active >= 8) return {
        label: 'medium',
        minRoot: 2, minDistinct: 3,
        maxPcCount: 6, maxAbsPitchCount: 5,
        requireRootPlacement: false, requireRootEither: true,
    };
    if (active >= 4) return {
        label: 'sparse',
        minRoot: 1, minDistinct: 2,
        maxPcCount: 5, maxAbsPitchCount: 4,
        requireRootPlacement: false, requireRootEither: false,
    };
    return 'micro';  // 1..3 active - content rules off
}

export { TOTAL_STEPS, STRONG_BEATS, RUN_LIMIT };
