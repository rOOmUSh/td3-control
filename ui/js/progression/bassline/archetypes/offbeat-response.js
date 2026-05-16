// OFFBEAT RESPONSE archetype - call-and-response with the acid lead.
//
// Musical intent: the lead talks, the bass answers in the silences. Bass
// notes land where the acid lead has rests, preferring offbeat positions
// so the bass never stacks on top of the lead. Step 0 is always home
// (audible downbeat), and step 15 is always the semitone approach.
//
// Placement algorithm:
//   1. Step 0  = home (accented).
//   2. Step 15 = approach→home (slide optional).
//   3. From the remaining 14 steps, pick 3-5 positions where:
//        (a) the acid lead is resting at that step, AND
//        (b) the step is odd (offbeat), prioritized over even.
//      Fall back to any rest position if offbeat pickings run dry.
//   4. Each response fills with either HOME (40%) or a movement degree
//      (60%), biased toward 5 and 3/b3 via movementCandidates weighting.
//
// If the acid lead is wall-to-wall active (rare for jam patterns), we
// fall back to a 2-anchor skeleton (home at 0 and 8, approach at 15) so
// the output still respects the "no stacking on lead" rule.

import {
    pcToNoteName, approachBelowPc, movementCandidates,
    pickFromPool, restStep, noteStep,
} from '../home-movement-approach.js';
import { isRestStep } from '../features.js';

const APPROACH_STEP = 15;
const RESPONSE_TARGET_MIN = 3;
const RESPONSE_TARGET_MAX = 5;

/** Offbeat steps preferred for responses, excluding 0 and 15 (reserved). */
const OFFBEAT_STEPS = [3, 5, 7, 9, 11, 13];
/** Even-step candidates if we can't fill from offbeats alone. */
const EVEN_STEPS    = [2, 4, 6, 8, 10, 12, 14];

function availableResponses(acidPattern, stepPool) {
    const out = [];
    for (const s of stepPool) {
        if (isRestStep(acidPattern.steps[s])) out.push(s);
    }
    return out;
}

function pickN(pool, n, rng) {
    const copy = pool.slice();
    const out = [];
    for (let i = 0; i < n && copy.length > 0; i++) {
        const idx = Math.floor(rng.next() * copy.length);
        out.push(copy.splice(idx, 1)[0]);
    }
    return out.sort((a, b) => a - b);
}

export function offbeatResponse({ root, scaleIntervals, acidPattern, features, rng }) {
    const homePc = ((root % 12) + 12) % 12;
    const homeName = pcToNoteName(homePc);
    const approachName = pcToNoteName(approachBelowPc(homePc));
    const movementPool = movementCandidates(root, scaleIntervals);

    const steps = new Array(16).fill(null).map(() => restStep(homeName));
    steps[0] = noteStep(homeName, { accent: true });
    steps[APPROACH_STEP] = noteStep(approachName, { slide: rng.next() < 0.5 });

    // Target response count scales inversely with lead density - busier lead
    // means sparser bass. Clamp to the configured min/max.
    const density = features ? features.density : 0.5;
    const budget = Math.round(RESPONSE_TARGET_MAX - (density * (RESPONSE_TARGET_MAX - RESPONSE_TARGET_MIN)));
    const targetCount = Math.max(RESPONSE_TARGET_MIN, Math.min(RESPONSE_TARGET_MAX, budget));

    const offbeatRests = availableResponses(acidPattern, OFFBEAT_STEPS);
    let chosen = pickN(offbeatRests, targetCount, rng);

    // Fall back into even-step rests when the offbeat pool was too thin to
    // hit the density budget. Keeps the archetype audible when the lead is
    // very busy on odd steps.
    if (chosen.length < targetCount) {
        const evenRests = availableResponses(acidPattern, EVEN_STEPS)
            .filter(s => !chosen.includes(s));
        const extra = pickN(evenRests, targetCount - chosen.length, rng);
        chosen = chosen.concat(extra).sort((a, b) => a - b);
    }

    for (const s of chosen) {
        const useHome = rng.next() < 0.4;
        if (useHome) {
            // A mid-bar home note gets a soft accent at step 8 only - that's
            // the strongest mid-bar downbeat in a 16-step bar.
            const accent = (s === 8);
            steps[s] = noteStep(homeName, { accent });
            continue;
        }
        const pc = pickFromPool(movementPool, rng);
        if (pc === null) continue;
        steps[s] = noteStep(pcToNoteName(pc));
    }

    return {
        active_steps: 16,
        triplet: false,
        steps,
    };
}
