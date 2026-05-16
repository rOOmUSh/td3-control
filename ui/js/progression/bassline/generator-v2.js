// Bassline generator v2 - for each acid pattern P1..P4, produce ALL FIVE
// archetype variants. Caller (progression-main.js) stores the full 5×4 set
// so the UI can let the user audition and pick per-pattern.
//
// Each archetype is a pure, deterministic function of:
//   (acidPattern, root, scaleIntervals, features, rng)
// so the output is reproducible given a fixed seed on the RNG.
//
// This module replaces the old single-rhythm-mode `generateSupportingBasslines`
// entry point. The old module (bassline-generator.js) remains on disk for
// bake-off comparison but is no longer wired into the progression flow.

import { extractFeatures } from './features.js';
import { pedal }            from './archetypes/pedal.js';
import { rootPulse }        from './archetypes/root-pulse.js';
import { offbeatResponse }  from './archetypes/offbeat-response.js';
import { simplifiedShadow } from './archetypes/simplified-shadow.js';
import { acidArpeggio }     from './archetypes/acid-arpeggio.js';
import { selectDefaultArchetype, ARCHETYPE_KEYS } from './selector.js';
import { scaleDegreesPc } from './home-movement-approach.js';

/**
 * Generate the full 5×4 bassline set for a progression.
 *
 * @param {Object} params
 * @param {Array<Object>} params.acidPatterns   length 4
 * @param {Object}        params.harmonicMap    see buildHarmonicMap()
 *                                              - needs .centers[i].centerPc and .scaleIntervals
 * @param {{next: () => number}} params.rng
 * @returns {{
 *   basslinesByPattern: Array<{
 *     pedal:Object, rootPulse:Object, offbeat:Object, shadow:Object, arpeggio:Object
 *   }>,                          // length 4 - one entry per acid pattern
 *   defaultArchetypeByPattern: string[],       // length 4, from selector
 *   features: Object[]            // length 4, feature vector per pattern
 * }}
 */
export function generateAllBasslines({ acidPatterns, harmonicMap, rng }) {
    validateInputs(acidPatterns, harmonicMap, rng);

    const scaleIntervals = harmonicMap.scaleIntervals;
    const basslinesByPattern = new Array(4);
    const defaultArchetypeByPattern = new Array(4);
    const featuresByPattern = new Array(4);

    for (let i = 0; i < 4; i++) {
        const acidPattern = acidPatterns[i];
        const center = harmonicMap.centers[i];
        const root = center.centerPc;  // each pattern is bassed on its own tonal center
        const scalePcs = new Set(scaleDegreesPc(harmonicMap.root, scaleIntervals));

        const features = extractFeatures(acidPattern, { root, scalePcs });

        const ctx = { root, scaleIntervals, acidPattern, features, rng };

        basslinesByPattern[i] = {
            pedal:     pedal(ctx),
            rootPulse: rootPulse(ctx),
            offbeat:   offbeatResponse(ctx),
            shadow:    simplifiedShadow(ctx),
            arpeggio:  acidArpeggio(ctx),
        };
        defaultArchetypeByPattern[i] = selectDefaultArchetype(features);
        featuresByPattern[i] = features;
    }

    return { basslinesByPattern, defaultArchetypeByPattern, features: featuresByPattern };
}

function validateInputs(acidPatterns, harmonicMap, rng) {
    if (!Array.isArray(acidPatterns) || acidPatterns.length !== 4) {
        throw new Error('generateAllBasslines: acidPatterns must be length 4');
    }
    for (let i = 0; i < 4; i++) {
        const p = acidPatterns[i];
        if (!p || !Array.isArray(p.steps) || p.steps.length !== 16) {
            throw new Error(`generateAllBasslines: acidPatterns[${i}] must have 16 steps`);
        }
    }
    if (!harmonicMap || !Array.isArray(harmonicMap.centers) || harmonicMap.centers.length !== 4) {
        throw new Error('generateAllBasslines: harmonicMap.centers must have length 4');
    }
    if (!Array.isArray(harmonicMap.scaleIntervals) || harmonicMap.scaleIntervals.length === 0) {
        throw new Error('generateAllBasslines: harmonicMap.scaleIntervals missing');
    }
    if (!rng || typeof rng.next !== 'function') {
        throw new Error('generateAllBasslines: rng.next() is required');
    }
}

export { ARCHETYPE_KEYS };
