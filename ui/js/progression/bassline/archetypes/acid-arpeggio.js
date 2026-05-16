// ACID ARPEGGIO archetype - walks the home triad (1, 3, 5) with optional
// b7 color and one chromatic passing tone per bar. The busiest of the five
// archetypes and the one that tips into proper 303-on-303 territory.
//
// Musical intent: two TB-303s in conversation. This archetype is the line
// you reach for when the lead is a held drone or very sparse - the bass
// then fills the bar with arpeggiated movement. A chromatic passing note
// (home-1) lands once per bar, always leading INTO a strong note (step 0,
// 4, 8, 12) so the chroma reads as intent rather than noise.
//
// Rhythm mask (canonical):
//   Every even step (0..14) active, plus step 15 (approach→home wrap).
//   That's a 9-step bass - dense by our standard, earned because the
//   archetype exists for the sparse-lead case.
//
// Pitch assignment:
//   Step 0  → home             (accented)
//   Step 2  → third            (minor preferred; falls back to fifth)
//   Step 4  → fifth            (soft accent)
//   Step 6  → home
//   Step 8  → fifth            (accented - strong mid-bar)
//   Step 10 → third
//   Step 12 → home
//   Step 14 → b7 OR chromatic  (picks b7 if in scale, else home-1)
//   Step 15 → approach→home    (slide-in by default - acid hallmark)
//
// When the lead is already dense we thin this archetype: drop steps 2, 10
// (the weakest arp fills) so the bass doesn't fight the lead. That keeps
// this archetype usable across lead densities without losing its character.

import {
    pcToNoteName, approachBelowPc, scaleDegreesPc,
    restStep, noteStep,
} from '../home-movement-approach.js';

function inScale(pc, scalePcs) { return scalePcs.has(pc); }

function pickThird(homePc, scalePcs) {
    const m3 = (homePc + 3) % 12;
    const M3 = (homePc + 4) % 12;
    if (inScale(m3, scalePcs)) return m3;
    if (inScale(M3, scalePcs)) return M3;
    return (homePc + 5) % 12; // fourth as last resort
}

function pickFifth(homePc, scalePcs) {
    const p5 = (homePc + 7) % 12;
    const p4 = (homePc + 5) % 12;
    if (inScale(p5, scalePcs)) return p5;
    if (inScale(p4, scalePcs)) return p4;
    return homePc;
}

function pickB7(homePc, scalePcs) {
    const b7 = (homePc + 10) % 12;
    const M7 = (homePc + 11) % 12;
    if (inScale(b7, scalePcs)) return b7;
    if (inScale(M7, scalePcs)) return M7;
    return (homePc + 11) % 12; // return leading tone as fallback
}

export function acidArpeggio({ root, scaleIntervals, features, rng }) {
    const homePc = ((root % 12) + 12) % 12;
    const scalePcs = new Set(scaleDegreesPc(root, scaleIntervals));
    const homeName = pcToNoteName(homePc);
    const thirdPc  = pickThird(homePc, scalePcs);
    const fifthPc  = pickFifth(homePc, scalePcs);
    const b7Pc     = pickB7(homePc, scalePcs);
    const approachName = pcToNoteName(approachBelowPc(homePc));

    const steps = new Array(16).fill(null).map(() => restStep(homeName));

    // Canonical placement. We'll optionally drop steps 2, 10 later if the
    // lead is dense.
    steps[0]  = noteStep(homeName,            { accent: true });
    steps[2]  = noteStep(pcToNoteName(thirdPc));
    steps[4]  = noteStep(pcToNoteName(fifthPc));
    steps[6]  = noteStep(homeName);
    steps[8]  = noteStep(pcToNoteName(fifthPc), { accent: true });
    steps[10] = noteStep(pcToNoteName(thirdPc));
    steps[12] = noteStep(homeName);
    steps[14] = noteStep(pcToNoteName(b7Pc));
    // Approach-home: slide-in is the acid fingerprint - used 70% of the time
    // here, slightly higher than other archetypes because it's the defining
    // gesture of this line.
    steps[15] = noteStep(approachName, { slide: rng.next() < 0.7 });

    // Density limiter: if the lead is busy (>= 0.6 density), prune the two
    // weakest arp fills to give the lead room. Chosen empirically - steps 2
    // and 10 are the "and" of beats 1 and 3, sacrificed first.
    if (features && features.density >= 0.6) {
        steps[2]  = restStep(homeName);
        steps[10] = restStep(homeName);
    }

    return {
        active_steps: 16,
        triplet: false,
        steps,
    };
}
