// ROOT PULSE archetype - home on every anchor {0, 4, 8, 12}, one movement
// note between each anchor pair, approach→home at step 15.
//
// Musical intent: the classic acid support bass. Clear quarter-note pulse
// from the root, with a single color note in the gaps so the bass has shape
// without stepping on the lead. Slightly denser than PEDAL, noticeably
// simpler than OFFBEAT RESPONSE - middle of the density spectrum.
//
// Rhythm mask (canonical):
//   Step  0  2  4  6  8 10 12 14 15
//   Hit   ●  ·  ●  ·  ●  ·  ●  ·  ◆
// Where · is one OR zero movement notes depending on RNG - we aim for two
// movement fills per bar (from 4 possible mid-gap slots) so the bass never
// outnumbers the anchors.

import {
    pcToNoteName, approachBelowPc, movementCandidates,
    pickFromPool, restStep, noteStep,
} from '../home-movement-approach.js';

const ANCHOR_STEPS    = [0, 4, 8, 12];
const FILL_CANDIDATES = [2, 6, 10, 14];
const APPROACH_STEP   = 15;

/** Pick 2 non-overlapping fill slots out of the 4 candidates. */
function pickFills(rng) {
    const pool = FILL_CANDIDATES.slice();
    const out = [];
    for (let i = 0; i < 2 && pool.length > 0; i++) {
        const idx = Math.floor(rng.next() * pool.length);
        out.push(pool.splice(idx, 1)[0]);
    }
    return out.sort((a, b) => a - b);
}

export function rootPulse({ root, scaleIntervals, rng }) {
    const homePc = ((root % 12) + 12) % 12;
    const homeName = pcToNoteName(homePc);
    const approachName = pcToNoteName(approachBelowPc(homePc));
    const movementPool = movementCandidates(root, scaleIntervals);

    const steps = new Array(16).fill(null).map(() => restStep(homeName));

    for (const s of ANCHOR_STEPS) {
        // Accent pattern mirrors the old anchor_fill generator: step 0 + step 8
        // punch, steps 4 + 12 are softer. Gives a 1-&-3-& feel without going
        // full four-on-floor uniformity.
        const accent = (s === 0 || s === 8);
        steps[s] = noteStep(homeName, { accent });
    }

    const fills = pickFills(rng);
    for (const s of fills) {
        const pc = pickFromPool(movementPool, rng);
        if (pc === null) continue;
        steps[s] = noteStep(pcToNoteName(pc));
    }

    // Approach on step 15 - slide about half the time into the step-0 home.
    const useSlide = rng.next() < 0.5;
    steps[APPROACH_STEP] = noteStep(approachName, { slide: useSlide });

    return {
        active_steps: 16,
        triplet: false,
        steps,
    };
}
