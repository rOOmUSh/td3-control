// SIMPLIFIED SHADOW archetype - follows the acid lead's rhythm at reduced
// density and with the lead's note choices flattened to home / fifth / third.
//
// Musical intent: the bass tracks the melody's gesture (where the lead hits,
// the bass might hit too) but without the pitch filigree - it translates
// every active step into one of the three strongest harmonic notes so the
// bass always reads as in-scale, never competes for attention.
//
// Algorithm:
//   1. Copy the lead's active-step mask.
//   2. Thin it: keep step 0 + step 8 always, then keep ~half the other
//      active steps (biased to anchor positions). Drop the rest to REST.
//   3. For each kept step, pick a note:
//        step 0, 8, 12       → home
//        step 4              → fifth (or home if scale has no fifth)
//        everything else     → home (70%) / fifth (20%) / third (10%)
//   4. Approach on step 15 if step 15 was active in the lead, otherwise
//      leave it as REST (we don't invent rhythm here).
//   5. Accent step 0 always, step 8 if it survived thinning.
//
// Density invariant: output active count ≤ lead active count (by construction).

import {
    pcToNoteName, approachBelowPc, scaleDegreesPc,
    restStep, noteStep,
} from '../home-movement-approach.js';
import { isRestStep } from '../features.js';

function inScale(pc, scalePcs) { return scalePcs.has(pc); }

function selectNote(stepIdx, homePc, fifthPc, thirdPc, rng) {
    if (stepIdx === 0 || stepIdx === 8 || stepIdx === 12) return homePc;
    if (stepIdx === 4) return fifthPc;
    const roll = rng.next();
    if (roll < 0.70) return homePc;
    if (roll < 0.90) return fifthPc;
    return thirdPc;
}

/** Pick a scale-resident third (minor preferred when available, else major). */
function pickThird(homePc, scalePcs) {
    const minor3 = (homePc + 3) % 12;
    const major3 = (homePc + 4) % 12;
    if (inScale(minor3, scalePcs)) return minor3;
    if (inScale(major3, scalePcs)) return major3;
    return homePc; // fallback: no third in scale, collapse to home
}

/** Pick a scale-resident fifth (perfect preferred, else fourth, else home). */
function pickFifth(homePc, scalePcs) {
    const p5 = (homePc + 7) % 12;
    const p4 = (homePc + 5) % 12;
    if (inScale(p5, scalePcs)) return p5;
    if (inScale(p4, scalePcs)) return p4;
    return homePc;
}

export function simplifiedShadow({ root, scaleIntervals, acidPattern, rng }) {
    const homePc = ((root % 12) + 12) % 12;
    const scalePcs = new Set(scaleDegreesPc(root, scaleIntervals));
    const homeName = pcToNoteName(homePc);
    const fifthPc  = pickFifth(homePc, scalePcs);
    const thirdPc  = pickThird(homePc, scalePcs);
    const approachName = pcToNoteName(approachBelowPc(homePc));

    const steps = new Array(16).fill(null).map(() => restStep(homeName));

    // Thinning: keep step 0 and 8 always, then keep ~half the other active
    // lead steps with a preference for anchor-adjacent positions.
    const leadActive = [];
    for (let i = 0; i < 16; i++) {
        if (!isRestStep(acidPattern.steps[i])) leadActive.push(i);
    }
    const forceKeep = new Set([0, 8]);
    const kept = new Set();
    for (const s of leadActive) {
        if (forceKeep.has(s)) { kept.add(s); continue; }
        // Anchor-adjacent steps (2,3,4,5, 6,7, 10,11,12,13) survive at ~60%,
        // others at ~30%. Keeps the shadow "hooked" around strong-beat phrasing.
        const nearAnchor = [2,3,4,5,6,7,10,11,12,13].includes(s);
        const keepProb = nearAnchor ? 0.6 : 0.3;
        if (rng.next() < keepProb) kept.add(s);
    }
    // Guarantee home on step 0 even when the lead rests there - the shadow
    // archetype is sold on its grounded downbeat.
    kept.add(0);

    for (const s of kept) {
        const notePc = selectNote(s, homePc, fifthPc, thirdPc, rng);
        const accent = (s === 0) || (s === 8 && kept.has(8));
        steps[s] = noteStep(pcToNoteName(notePc), { accent });
    }

    // Approach step 15 only if the lead was active there too (shadow = follow).
    if (!isRestStep(acidPattern.steps[15])) {
        steps[15] = noteStep(approachName, { slide: rng.next() < 0.5 });
    }

    return {
        active_steps: 16,
        triplet: false,
        steps,
    };
}
