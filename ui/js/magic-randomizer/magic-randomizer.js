// Orchestrator - runs the full magic pipeline for one pattern.
//
//   1. analyzeScale(root, scale)
//   2. buildActiveMask + generatePitchSequence  - generate N candidates
//   3. validateCandidate                        - partition pass/fail
//   4. scoreCandidate                           - rank passing candidates
//   5. repair near-miss candidates              - at most 6 actions, then
//                                                 revalidate + rescore
//   6. fallback: highest score from the run     - even if no candidate
//                                                 reaches the 65 threshold
//   7. applySlidesAndAccents                    - melody-aware overlay
//
// Returns a finished pattern in the same shape `state.setPattern` expects:
//   { active_steps, triplet, steps }
//
// Three flavours:
//   runMagicFull         - fresh 16-step pattern from root + scale
//   runMagicSlice        - only generate inside slice indices, preserve
//                          the rest, validate boundary continuity
//   runMagicProgression  - like Full but with explicit centerPc (for
//                          progression mode where each pattern targets a
//                          different scale degree)

import { analyzeScale } from './magic-scale-analysis.js';
import { createRng } from './magic-rng.js';
import {
    buildActiveMask,
    generatePitchSequence,
    encodeStepsFromPitches,
    generateCandidate,
} from './magic-generator.js';
import { computeMetrics, validateCandidate } from './magic-validator.js';
import { scoreCandidate } from './magic-scorer.js';
import { repairCandidate } from './magic-repairer.js';
import { applySlidesAndAccents } from './magic-slide-accent.js';
import { buildDebugReport, printDebugReport } from './magic-debug.js';
import { decodePitch, nearestPitch } from './magic-pitch-encoding.js';

const TOTAL_STEPS = 16;
const ACCEPTANCE_THRESHOLD = 65;
const REPAIR_THRESHOLD = 50; // candidates scoring at least this get repaired

// Candidate budgets per the user-approved hand-off:
//   single-shot control RANDOMIZE → 50
//   bulk (multipattern, progression) → 15 per pattern
export const BUDGET_SINGLE = 50;
export const BUDGET_BULK = 15;

// ---------------------------------------------------------------------------
// Pipeline
// ---------------------------------------------------------------------------

/**
 * Run the magic pipeline once for a pre-built mask + analysis. Returns
 * { steps, score, debugReport } and never throws - falls back to the
 * best-scoring candidate when no validated candidate clears 65.
 *
 * @param {object} opts
 * @param {object} opts.analysis       from analyzeScale
 * @param {boolean[]} opts.mask        16-element active/rest mask
 * @param {number} opts.attempts       candidate count
 * @param {number} opts.slidePercent   0..1
 * @param {number} opts.accPercent     0..1
 * @param {object} [opts.rng]          createRng() - defaults to Math.random
 * @param {number} [opts.centerPc]     defaults to analysis.rootPc
 * @param {number} [opts.registerCenter] center absolute pitch
 * @param {string} [opts.modeLabel]    'full' | 'slice' | 'progression'
 * @param {boolean} [opts.debug=false] print debug report when true
 */
function runPipeline(opts) {
    const {
        analysis, mask, attempts,
        slidePercent, accPercent,
        centerPc, registerCenter, modeLabel,
    } = opts;
    const rng = opts.rng || createRng(null);
    const debug = !!opts.debug;

    let bestPassed = null;       // { score, breakdown, metrics, candidate, actions }
    let bestEverywhere = null;   // best regardless of pass/fail (for fallback)
    const rejected = [];

    for (let i = 0; i < attempts; i++) {
        const cand = buildOneCandidate({ analysis, mask, rng, centerPc, registerCenter });
        const v = validateCandidate(cand, analysis);

        if (v.passed) {
            const { score, breakdown } = scoreCandidate(cand, analysis, v.metrics);
            const entry = { score, breakdown, metrics: v.metrics, candidate: cand, actions: [] };
            if (!bestPassed || score > bestPassed.score) bestPassed = entry;
            if (!bestEverywhere || score > bestEverywhere.score) bestEverywhere = entry;
            continue;
        }

        // Try repair if reasons look fixable.
        rejected.push({ reasons: v.reasons });
        const repaired = repairCandidate(cand, analysis, rng);
        const v2 = validateCandidate(repaired, analysis);
        if (v2.passed) {
            const { score, breakdown } = scoreCandidate(repaired, analysis, v2.metrics);
            const entry = { score, breakdown, metrics: v2.metrics, candidate: repaired, actions: repaired.actions };
            if (!bestPassed || score > bestPassed.score) bestPassed = entry;
            if (!bestEverywhere || score > bestEverywhere.score) bestEverywhere = entry;
        } else {
            // Keep best-of-fallback even if repair didn't fully validate.
            const m = computeMetrics(repaired, analysis);
            const { score, breakdown } = scoreCandidate(repaired, analysis, m);
            const entry = { score, breakdown, metrics: m, candidate: repaired, actions: repaired.actions };
            if (!bestEverywhere || score > bestEverywhere.score) bestEverywhere = entry;
        }
    }

    // Pick winner: best validated candidate at or above the 65 threshold;
    // else best validated overall; else fallback to best-everywhere.
    const winner = (bestPassed && bestPassed.score >= ACCEPTANCE_THRESHOLD)
        ? bestPassed
        : (bestPassed || bestEverywhere);

    const winnerSteps = winner ? [...winner.candidate.steps] : zeroPattern();

    // Apply melody-aware slides and accents on top of the winner's steps.
    applySlidesAndAccents(winnerSteps, analysis, slidePercent, accPercent);

    const report = buildDebugReport(
        {
            rootPc: analysis.rootPc,
            scaleName: analysis.scaleName,
            mode: modeLabel || 'full',
            attempts,
        },
        winner,
        rejected,
    );
    if (debug) printDebugReport(report);

    return {
        steps: winnerSteps,
        score: winner ? winner.score : 0,
        passed: winner === bestPassed && bestPassed.score >= ACCEPTANCE_THRESHOLD,
        debugReport: report,
    };
}

function buildOneCandidate({ analysis, mask, rng, centerPc, registerCenter }) {
    const { pitches } = generatePitchSequence({ analysis, mask, rng, centerPc, registerCenter });
    const steps = encodeStepsFromPitches(pitches, mask, null);
    return { mask, pitches, steps };
}

function zeroPattern() {
    return Array.from({ length: TOTAL_STEPS }, () => ({
        note: 'C', transpose: 'NORMAL', accent: false, slide: false, time: 'REST',
    }));
}

// ---------------------------------------------------------------------------
// Public flavours
// ---------------------------------------------------------------------------

/**
 * Full magic randomize for one pattern. Builds its own active/rest mask
 * from `notePercent`. Returns a plain Pattern: { active_steps, triplet, steps }.
 */
export function runMagicFull(opts) {
    const { root, scale, notePercent, slidePercent, accPercent, attempts, rng, debug } = opts;
    const analysis = analyzeScale(root, scale);
    const localRng = rng || createRng(null);
    const mask = buildActiveMask(notePercent, localRng);
    const result = runPipeline({
        analysis, mask, attempts: attempts ?? BUDGET_SINGLE,
        slidePercent, accPercent,
        rng: localRng, modeLabel: 'full', debug,
    });
    return { active_steps: opts.activeSteps ?? 16, triplet: !!opts.triplet, steps: result.steps, magic: result };
}

/**
 * Magic randomize centered on a non-root scale degree - used by
 * progression mode where each of the 4 patterns targets a different chord.
 */
export function runMagicProgression(opts) {
    const { root, scale, centerPc, registerCenter, notePercent, slidePercent, accPercent, attempts, rng, debug } = opts;
    const analysis = analyzeScale(root, scale);
    const localRng = rng || createRng(null);
    const mask = buildActiveMask(notePercent, localRng);
    const result = runPipeline({
        analysis, mask,
        attempts: attempts ?? BUDGET_BULK,
        slidePercent, accPercent,
        centerPc, registerCenter,
        rng: localRng, modeLabel: 'progression', debug,
    });
    return { active_steps: opts.activeSteps ?? 16, triplet: !!opts.triplet, steps: result.steps, magic: result };
}

/**
 * Magic randomize for a slice - only writes pitches into the slice
 * indices, preserves everything else, and validates the left/right
 * context transitions.
 *
 * `prevPattern` is the existing 16-step pattern; `sliceIndices` is a
 * sorted array of 0..15 indices the slicer parsed.
 */
export function runMagicSlice(opts) {
    const {
        root, scale, prevPattern, sliceIndices,
        notePercent, slidePercent, accPercent, attempts, rng, debug,
    } = opts;
    if (!Array.isArray(sliceIndices) || sliceIndices.length === 0) {
        return { active_steps: prevPattern.active_steps, triplet: prevPattern.triplet, steps: prevPattern.steps };
    }
    const analysis = analyzeScale(root, scale);
    const localRng = rng || createRng(null);

    // Build a slice-only mask: outside the slice, mask follows whatever
    // the previous pattern had. Inside the slice, mask is rolled fresh
    // from notePercent over the slice indices only.
    const sliceSet = new Set(sliceIndices);
    const fullMask = new Array(TOTAL_STEPS).fill(false);
    for (let i = 0; i < TOTAL_STEPS; i++) {
        if (!sliceSet.has(i)) {
            fullMask[i] = prevPattern.steps[i].time !== 'REST' && prevPattern.steps[i].time !== 'TIE_REST';
        }
    }
    const sliceActive = Math.max(0, Math.min(sliceIndices.length, Math.round(sliceIndices.length * notePercent)));
    const sliceShuffled = [...sliceIndices].sort(() => localRng.next() - 0.5);
    for (let i = 0; i < sliceActive; i++) fullMask[sliceShuffled[i]] = true;

    // Determine register center from the existing pattern's notes
    // immediately preceding the slice - keeps register continuity.
    const left = findContextPitchLeft(prevPattern.steps, sliceIndices[0]);
    const right = findContextPitchRight(prevPattern.steps, sliceIndices[sliceIndices.length - 1]);
    const registerCenter = pickRegisterFromContext(left, right, analysis);

    const result = runPipeline({
        analysis,
        mask: fullMask,
        attempts: attempts ?? BUDGET_SINGLE,
        slidePercent, accPercent,
        registerCenter,
        rng: localRng, modeLabel: 'slice', debug,
    });

    // Splice: keep prev pattern outside the slice; take generated steps
    // inside. Slide/accent flags applied by the pipeline land only on
    // active steps; outside the slice we re-copy prev so existing slides
    // and accents survive.
    const merged = prevPattern.steps.map(s => ({ ...s }));
    for (const i of sliceIndices) merged[i] = result.steps[i];
    return {
        active_steps: prevPattern.active_steps,
        triplet: prevPattern.triplet,
        steps: merged,
        magic: result,
    };
}

// ---------------------------------------------------------------------------
// Slice context helpers
// ---------------------------------------------------------------------------

function findContextPitchLeft(steps, sliceStart) {
    for (let i = sliceStart - 1; i >= 0; i--) {
        if (steps[i].time !== 'REST' && steps[i].time !== 'TIE_REST') {
            return decodePitch(steps[i]);
        }
    }
    return null;
}

function findContextPitchRight(steps, sliceEnd) {
    for (let i = sliceEnd + 1; i < steps.length; i++) {
        if (steps[i].time !== 'REST' && steps[i].time !== 'TIE_REST') {
            return decodePitch(steps[i]);
        }
    }
    return null;
}

function pickRegisterFromContext(left, right, analysis) {
    const samples = [left, right].filter(p => p != null);
    if (samples.length === 0) return 6;
    const mean = samples.reduce((a, b) => a + b, 0) / samples.length;
    return nearestPitch(mean, analysis.pitches.all) ?? 6;
}
