// Magic melody candidate generator.
//
// Generates a complete 16-step candidate (mask + per-step absolute pitch)
// using scale-role-aware weighted choices. The generator does not validate
// or score - that is the validator/scorer's job. It just produces a fair
// distribution of candidates that the downstream pipeline can rank and
// repair.
//
// Free-form: phrase templates are NOT generated here. They
// live in the scorer as soft biases. This keeps the generator's output
// space wide enough for the scorer to actually pick a winner.
//
// Key design choices:
//
// - Absolute pitch first, then encode to (note, transpose). Random
//   transpose post-processing is forbidden.
// - Strong beats (0, 4, 8, 12) bias toward stable pitches.
// - Step 13 biases extra strongly toward root/third/fifth so the loop has
//   somewhere to go home to.
// - Movement target distribution: ~50% step, ~25% repeat, ~15% small leap,
//   ~7% medium leap, ~3% large leap. Large leaps trigger a stabilizing
//   next-move bias.
// - Register: pick a center pitch on init and bias subsequent picks toward
//   it; soft, not a hard wall.
//
// REST steps still get a placeholder note (same as the previous active
// pitch's encoding) so the rendered pattern is well-formed. Validator and
// scorer ignore REST steps.

import { encodePitch, nearestPitch } from './magic-pitch-encoding.js';
import { pickOne, shuffleInPlace } from './magic-rng.js';

const TOTAL_STEPS = 16;
const STRONG_BEATS = [0, 4, 8, 12];

/**
 * Build the active/rest mask for a 16-step pattern. The mask honours
 * `notePercent` density (rounded to nearest count) and lightly biases
 * strong beats toward being active so the candidate has anchors to land
 * on. Returns an array of booleans length 16.
 */
export function buildActiveMask(notePercent, rng) {
    const activeCount = Math.max(0, Math.min(TOTAL_STEPS, Math.round(TOTAL_STEPS * notePercent)));
    const mask = new Array(TOTAL_STEPS).fill(false);
    if (activeCount === 0) return mask;

    // Bias strong beats to be active when there's room.
    const seeded = [];
    for (const sb of STRONG_BEATS) {
        if (seeded.length < activeCount && rng.next() < 0.85) seeded.push(sb);
    }
    for (const i of seeded) mask[i] = true;

    const remaining = [];
    for (let i = 0; i < TOTAL_STEPS; i++) if (!mask[i]) remaining.push(i);
    shuffleInPlace(remaining, rng);
    let need = activeCount - seeded.length;
    while (need > 0 && remaining.length > 0) {
        const i = remaining.pop();
        mask[i] = true;
        need--;
    }
    return mask;
}

// ---------------------------------------------------------------------------
// Candidate pitch sequence
// ---------------------------------------------------------------------------

/**
 * Generate an active-step pitch sequence.
 *
 * @param {object} opts
 * @param {object} opts.analysis   from analyzeScale()
 * @param {boolean[]} opts.mask    16-element active/rest mask
 * @param {object} opts.rng        from createRng()
 * @param {number} [opts.centerPc] tonal center pitch class (defaults to rootPc)
 * @param {number} [opts.registerCenter] center absolute pitch (defaults near 6)
 * @returns {{ pitches: (number|null)[], debug: object }}
 *   pitches[i] is the chosen absolute pitch when mask[i] is true, else null.
 */
export function generatePitchSequence(opts) {
    const { analysis, mask, rng } = opts;
    const centerPc      = Number.isInteger(opts.centerPc) ? opts.centerPc : analysis.rootPc;
    const registerCenter = Number.isInteger(opts.registerCenter) ? opts.registerCenter : 6;

    const pitches = new Array(TOTAL_STEPS).fill(null);
    if (analysis.pitches.all.length === 0) return { pitches, debug: { reason: 'empty-scale' } };

    // Anchor pitches that act as "tonal home" - center pc projected to
    // every octave inside the TD-3 range, plus stable-role pitches near
    // the chosen register center.
    const centerPitches = analysis.pitches.all.filter(p => ((p % 12) + 12) % 12 === centerPc);
    const stable = analysis.pitches.stable;
    const color  = analysis.pitches.color;
    const tension = analysis.pitches.tension;
    const weakStable = analysis.pitches.weakStable;

    // Collect active step indices in order.
    const activeIdx = [];
    for (let i = 0; i < TOTAL_STEPS; i++) if (mask[i]) activeIdx.push(i);

    let prevPitch = nearestPitch(registerCenter, centerPitches.length ? centerPitches : analysis.pitches.all);

    // Cap consecutive identical pitches at 3 to keep the anti-stuck
    // validator happy (4-in-a-row is a hard reject).
    let runPitch = null;
    let runLen = 0;

    for (let n = 0; n < activeIdx.length; n++) {
        const i = activeIdx[n];
        const isStrong = STRONG_BEATS.includes(i);
        const isFinalStrong = (i === 12);
        const isFirstActive = (n === 0);
        const candidate = chooseNextPitch({
            prevPitch, isStrong, isFinalStrong, isFirstActive,
            stable, color, tension, weakStable, all: analysis.pitches.all,
            centerPitches, registerCenter, rng,
            forbidPitch: runLen >= 3 ? runPitch : null,
        });

        pitches[i] = candidate;
        if (candidate === runPitch) runLen++;
        else { runPitch = candidate; runLen = 1; }
        prevPitch = candidate;
    }

    return {
        pitches,
        debug: {
            centerPc, registerCenter,
            activeCount: activeIdx.length,
            stableSize: stable.length, colorSize: color.length, tensionSize: tension.length,
        },
    };
}

function chooseNextPitch(ctx) {
    const {
        prevPitch, isStrong, isFinalStrong, isFirstActive,
        stable, tension, weakStable, all,
        centerPitches, registerCenter, rng, forbidPitch,
    } = ctx;

    // First active step: strong bias toward the tonal center so the
    // candidate has a chance of meeting the "root in first half" rule.
    if (isFirstActive && centerPitches.length > 0 && rng.next() < 0.55) {
        const near = bestNNear(centerPitches, registerCenter, 3);
        const pick = filterForbidden(near, forbidPitch);
        if (pick.length > 0) return pickOne(rng, pick);
    }

    // Step 13 (the last strong beat): bias hard toward the tonal center
    // (root) - this is the loop's resolution point and meets the 
    // "root, third, fifth, octave-root, or controlled tension" rule.
    if (isFinalStrong) {
        if (centerPitches.length > 0 && rng.next() < 0.60) {
            const near = bestNNear(centerPitches, prevPitch, 3);
            const pick = filterForbidden(near, forbidPitch);
            if (pick.length > 0) return pickOne(rng, pick);
        }
        const pool = stable.length ? stable : all;
        const near = bestNNear(pool, prevPitch, 4);
        const pick = filterForbidden(near, forbidPitch);
        if (pick.length > 0) return pickOne(rng, pick);
    }

    // Strong-beat bias toward stable.
    if (isStrong) {
        const r = rng.next();
        if (r < 0.65 && stable.length > 0) {
            const near = bestNNear(stable, prevPitch, 5);
            const pick = filterForbidden(near, forbidPitch);
            if (pick.length > 0) return pickOne(rng, pick);
        }
        if (r < 0.85 && centerPitches.length > 0) {
            const near = bestNNear(centerPitches, prevPitch, 3);
            const pick = filterForbidden(near, forbidPitch);
            if (pick.length > 0) return pickOne(rng, pick);
        }
    }

    // Movement-weighted choice for the rest of the steps. Pull a candidate
    // from the full in-scale pitch list, weighted by the gap to prevPitch
    // (favouring step + repeat over leaps) and by role (stable/center mild
    // bonus, tension small penalty unless we're going up to a stable).
    const weighted = [];
    for (const p of all) {
        if (p === forbidPitch) continue;
        const dist = Math.abs(p - prevPitch);
        let w = 0;
        if (dist === 0) w = 22;                  // repeat
        else if (dist === 1 || dist === 2) w = 28; // step (counted in semitones, close enough)
        else if (dist <= 4) w = 12;              // small leap
        else if (dist <= 7) w = 4;               // medium leap
        else w = 1;                              // large leap
        // Register pull - tighter than before so candidates cluster around
        // the chosen register and progression mode (where centerPc differs
        // from rootPc) actually centres on the requested degree.
        const registerDist = Math.abs(p - registerCenter);
        if (registerDist <= 3)      w *= 1.25;
        else if (registerDist <= 5) w *= 1.00;
        else if (registerDist <= 7) w *= 0.70;
        else if (registerDist <= 10) w *= 0.45;
        else                         w *= 0.20;
        // Role tilt
        if (stable.includes(p)) w *= 1.15;
        else if (centerPitches.includes(p)) w *= 1.1;
        else if (tension.includes(p)) w *= 0.85;
        else if (weakStable.includes(p)) w *= 1.0;
        // Color stays at 1.0 - keeps scale identity in the mix.
        weighted.push({ p, w });
    }
    if (weighted.length === 0) return prevPitch;
    const sum = weighted.reduce((a, b) => a + b.w, 0);
    let r = rng.next() * sum;
    for (const cand of weighted) {
        r -= cand.w;
        if (r <= 0) return cand.p;
    }
    return weighted[weighted.length - 1].p;
}

function bestNNear(pool, target, n) {
    const sorted = [...pool].sort((a, b) => Math.abs(a - target) - Math.abs(b - target));
    return sorted.slice(0, Math.max(1, n));
}

function filterForbidden(arr, forbidden) {
    if (forbidden == null) return arr;
    const out = arr.filter(x => x !== forbidden);
    return out.length > 0 ? out : arr;
}

// ---------------------------------------------------------------------------
// Encoding into TD-3 step objects
// ---------------------------------------------------------------------------

/**
 * Encode a 16-step pitch sequence + mask into TD-3 step objects ready for
 * `state.setPattern()`. Slides and accents are placed as `false` here -
 * magic-slide-accent.js fills them in afterwards.
 *
 * If `prevSteps` is provided, REST positions inherit their shape from the
 * incoming pattern's REST/TIE_REST/etc state (matters for slice mode where
 * we're only writing into a subset of the indices).
 */
export function encodeStepsFromPitches(pitches, mask, prevSteps) {
    const steps = new Array(TOTAL_STEPS);
    let lastEncoded = null;
    for (let i = 0; i < TOTAL_STEPS; i++) {
        if (mask[i]) {
            const enc = encodePitch(pitches[i]);
            if (!enc) {
                // Encoder rejected this pitch. Fall back to a safe note -
                // the validator will catch this candidate and reject it.
                const fallback = encodePitch(0);
                steps[i] = {
                    note: fallback.note, transpose: fallback.transpose,
                    accent: false, slide: false, time: 'NORMAL',
                };
            } else {
                steps[i] = {
                    note: enc.note, transpose: enc.transpose,
                    accent: false, slide: false, time: 'NORMAL',
                };
                lastEncoded = enc;
            }
        } else {
            // REST step - use last encoded pitch's shape if available, else
            // default to C/NORMAL. Drop slide/accent (REST flags are silent).
            const carry = lastEncoded || encodePitch(0);
            steps[i] = {
                note: carry.note, transpose: carry.transpose,
                accent: false, slide: false, time: 'REST',
            };
        }
    }
    return steps;
}

// ---------------------------------------------------------------------------
// Top-level: one full candidate
// ---------------------------------------------------------------------------

/**
 * Generate a complete melody candidate for one 16-step pattern. Caller
 * normally generates many candidates via generateCandidates() and lets
 * the scorer pick the winner.
 */
export function generateCandidate(opts) {
    const { analysis, notePercent, rng, centerPc, registerCenter, predefinedMask } = opts;
    const mask = predefinedMask || buildActiveMask(notePercent, rng);
    const { pitches, debug } = generatePitchSequence({
        analysis, mask, rng, centerPc, registerCenter,
    });
    const steps = encodeStepsFromPitches(pitches, mask, null);
    return { mask, pitches, steps, debug };
}

/**
 * Generate `count` candidates from independent RNG draws. Returns an
 * array - the caller validates / scores / picks. `predefinedMask` is
 * useful when the active/rest pattern is fixed by the caller (e.g.
 * slice mode preserving the existing mask outside the slice).
 */
export function generateCandidates(opts) {
    const { count, ...rest } = opts;
    const out = [];
    for (let i = 0; i < count; i++) {
        out.push(generateCandidate(rest));
    }
    return out;
}

export { TOTAL_STEPS, STRONG_BEATS };
