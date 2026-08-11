// Canonical pattern serialization for exact-identity comparison.
//
// The triplet morph session stores these texts as its source-of-truth
// equality check: fixed field order, normalized values, no dependence
// on object iteration order, platform hashes, or digests. Restoring a
// morph session requires byte-for-byte equality of this text.

const STEP_COUNT = 16;

function canonicalStepText(step) {
    const note = JSON.stringify(String(step?.note ?? 'C'));
    const transpose = JSON.stringify(String(step?.transpose ?? 'NORMAL'));
    const accent = step?.accent ? 'true' : 'false';
    const slide = step?.slide ? 'true' : 'false';
    const time = JSON.stringify(String(step?.time ?? 'NORMAL'));
    return `{"note":${note},"transpose":${transpose},"accent":${accent},`
        + `"slide":${slide},"time":${time}}`;
}

export function canonicalPatternText(pattern) {
    const activeSteps = Number(pattern?.active_steps);
    const normalizedActive = Number.isInteger(activeSteps) ? activeSteps : 0;
    const steps = [];
    for (let i = 0; i < STEP_COUNT; i += 1) {
        steps.push(canonicalStepText(pattern?.steps?.[i]));
    }
    return `{"active_steps":${normalizedActive},`
        + `"triplet":${pattern?.triplet ? 'true' : 'false'},`
        + `"steps":[${steps.join(',')}]}`;
}

export function canonicalPatternListText(patterns) {
    return (patterns || []).map(canonicalPatternText);
}
