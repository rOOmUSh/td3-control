// Bridges the page state and the StepDSL v1.1 metadata.
//
// Export: `stepsMetaForExport()` turns a pattern's lanes plus the
// transport-bar values, the TRIPLET amount, and the LIVE button into the
// `stepsMeta` object the server and the clipboard formatter write.
//
// Import: `applyImportedStepsMeta()` turns a document's metadata into
// the lanes to store on the pattern and the page changes to make,
// applying the session rules the server cannot know.
//
// Both are pure so they run under Node for tests.

import { LANES, isLaneDefault, laneState } from './step-lanes.js';
import { isMorphEligiblePattern } from './triplet-morph-timing.js';

/**
 * Export metadata for `pattern`.
 *
 * Lane values are the stored base values (the ratio knob is not
 * exported). A lane that is OFF and untouched writes the transport-bar
 * value on every step so a pattern never edited in the drawer carries
 * the global setting; a lane that is OFF but edited keeps its values with
 * the switch written off. Morph is written only when the page amount is
 * above 0 and the pattern is morph-eligible.
 */
export function stepsMetaForExport(pattern, {
    globalCutoff = LANES.cutoff.fallback,
    globalGate = LANES.gate.fallback,
    tripletMorphPercent = 0,
    liveUpdate = false,
} = {}) {
    const lanes = laneState(pattern);
    const laneValues = (lane, globalValue) => {
        const on = lane === 'cutoff' ? lanes.cutoffOn : lanes.gateOn;
        const stored = lanes[lane];
        if (!on && isLaneDefault(lane, stored)) {
            const value = Number(globalValue);
            const fallback = Number.isInteger(value) ? value : LANES[lane].fallback;
            return Array.from({ length: 16 }, () => fallback);
        }
        return stored.slice();
    };
    const morph = Number(tripletMorphPercent);
    const morphOn = Number.isFinite(morph) && morph > 0 && isMorphEligiblePattern(pattern);
    return {
        stepCutoffs: laneValues('cutoff', globalCutoff),
        stepGates: laneValues('gate', globalGate),
        cutoffLaneOn: lanes.cutoffOn,
        gateLaneOn: lanes.gateOn,
        tripletMorphPercent: morphOn ? Math.min(100, Math.round(morph)) : 0,
        liveUpdate: !!liveUpdate,
    };
}

/**
 * Apply a document's metadata to a freshly imported `pattern`.
 *
 * Returns `{ lanes, morphPercent, liveUpdate }`: `lanes` is the object
 * to store on the pattern (ratios at centre; cutoff dropped when the
 * session's device cannot be controlled), `morphPercent` is the TRIPLET
 * amount to set or null (only when the pattern has a multiple of four
 * active steps and is not in triplet time), and `liveUpdate` is the LIVE
 * state to apply or null.
 */
export function applyImportedStepsMeta({ meta, pattern, deviceControlsSupported = false } = {}) {
    const source = meta && typeof meta === 'object' ? meta : {};
    const current = laneState(pattern);
    const lanes = {
        open: current.open,
        cutoffOn: false,
        gateOn: false,
        cutoff: current.cutoff,
        gate: current.gate,
        cutoffRatio: LANES.cutoff.mid,
        gateRatio: LANES.gate.mid,
    };
    if (deviceControlsSupported && Array.isArray(source.stepCutoffs)) {
        lanes.cutoff = source.stepCutoffs.slice();
        lanes.cutoffOn = source.cutoffLaneOn === true;
    }
    if (Array.isArray(source.stepGates)) {
        lanes.gate = source.stepGates.slice();
        lanes.gateOn = source.gateLaneOn === true;
    }

    let morphPercent = null;
    const morph = Number(source.tripletMorphPercent);
    const activeSteps = Number(pattern?.active_steps);
    if (Number.isInteger(morph) && morph > 0 && !pattern?.triplet
        && Number.isInteger(activeSteps) && activeSteps % 4 === 0) {
        morphPercent = Math.min(100, morph);
    }

    const liveUpdate = typeof source.liveUpdate === 'boolean' ? source.liveUpdate : null;
    return { lanes, morphPercent, liveUpdate };
}
