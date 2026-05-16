// PEDAL archetype - one note, repeated on strong places, heavy pauses.
//
// Musical intent: when the acid lead is busy and slippery, the bass becomes
// a clock that grounds the bar on the tonal center. Think techno four-on-floor
// feeling, but only on beat 1 and beat 3, with pauses elsewhere and a single
// approach note at step 15 to close the wrap.
//
// Rhythm mask (canonical - archetypes are allowed their own masks):
//   Step  0  4  8 12 15
//   Hit   ●        ●  ◆        (● = home, ◆ = approach→home)
// When the acid lead is very sparse, we upgrade by filling anchors 4 and 12
// with home notes too, so the bass stays audible against empty bars.

import {
    pcToNoteName, approachBelowPc, restStep, noteStep,
} from '../home-movement-approach.js';

const HOME_STEPS_SPARSE = [0, 8];
const HOME_STEPS_DENSE  = [0, 4, 8, 12]; // used only when acid lead is very sparse
const APPROACH_STEP     = 15;

export function pedal({ root, scaleIntervals, features, rng }) {
    const homePc = ((root % 12) + 12) % 12;
    const homeName = pcToNoteName(homePc);
    const approachName = pcToNoteName(approachBelowPc(homePc));

    // Pick which home-pattern fits this acid lead's density. The threshold is
    // a heuristic - below 0.4 the lead is so sparse that a 2-hit pedal
    // sounds absent, so we double up.
    const leadIsSparse = features && features.density < 0.4;
    const homeSteps = leadIsSparse ? HOME_STEPS_DENSE : HOME_STEPS_SPARSE;

    const steps = new Array(16).fill(null).map(() => restStep(homeName));

    for (const s of homeSteps) {
        // Step 0 is always accented (density invariant shared with the non-magic
        // generator - keeps song-start audible). Step 8 gets a milder accent
        // when the lead is dense enough that the bass needs to punch through.
        const accent = (s === 0) || (s === 8 && features && features.density >= 0.5);
        steps[s] = noteStep(homeName, { accent });
    }

    // Optional slide into home on step 15. Only fires sometimes - we keep it rare for taste.
    const addApproach = !features || features.density >= 0.25;
    if (addApproach) {
        const useSlide = rng && rng.next() < 0.5;
        steps[APPROACH_STEP] = noteStep(approachName, { slide: useSlide });
    }

    return {
        active_steps: 16,
        triplet: false,
        steps,
    };
}
