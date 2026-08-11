// Pure morph timing projections for highlighting and derived views.
//
// All values here are display math over the cached Rust plan's rational
// offsets. The authoritative MIDI schedule stays in Rust; floats are
// acceptable for CSS positions and highlight selection only.

const EPSILON = 1e-9;

function rationalToNumber(value) {
    const num = Number(value?.num);
    const den = Number(value?.den);
    if (!Number.isFinite(num) || !Number.isFinite(den) || den === 0) return 0;
    return num / den;
}

/**
 * Warped global bar position of one plan assignment, as a fraction of
 * the four-beat cycle in [0, 1]. W(m, s, t) = s + m * (t - s).
 */
export function warpedOnsetFraction(assignment, amountPercent) {
    const source = rationalToNumber(assignment?.sourceOffset);
    const target = rationalToNumber(assignment?.targetOffset);
    const m = Math.max(0, Math.min(100, Number(amountPercent) || 0)) / 100;
    const local = source + m * (target - source);
    const beat = Math.floor((Number(assignment?.step) || 0) / 4);
    return (beat + local) / 4;
}

/**
 * Source step whose warped onset most recently passed the given cycle
 * phase fraction. At the 100% endpoint only surviving cells are
 * considered, so the highlighter walks the 12 target cells.
 */
export function morphDisplayStep(assignments, amountPercent, phaseFraction) {
    if (!Array.isArray(assignments) || assignments.length === 0) return 0;
    let current = null;
    for (const assignment of assignments) {
        if (Number(amountPercent) >= 100 && !assignment?.survivor) continue;
        if (warpedOnsetFraction(assignment, amountPercent) <= phaseFraction + EPSILON) {
            current = Number(assignment?.step) || 0;
        } else {
            break;
        }
    }
    if (current === null) {
        const first = assignments.find(
            a => Number(amountPercent) < 100 || a?.survivor,
        );
        return Number(first?.step) || 0;
    }
    return current;
}

/**
 * Translate a uniform 16th-grid step index into the source step the
 * warped schedule is audibly playing at that boundary's phase.
 */
export function morphDisplayStepForUniformTick(
    assignments,
    amountPercent,
    uniformStep,
    activeSteps = 16,
) {
    const steps = Math.max(1, Number(activeSteps) || 16);
    const wrapped = ((Number(uniformStep) || 0) % steps + steps) % steps;
    return morphDisplayStep(assignments, amountPercent, wrapped / steps);
}

/**
 * Morph amount to send with a host-audition request for `pattern`, or
 * null to omit the field. An eligible 16-step straight source always
 * carries an explicit amount (including 0) so replacement schedules
 * keep stable event identities; an ineligible source omits the field
 * and keeps legacy audition, including native pattern.triplet timing.
 */
export function morphRequestPercent(pattern, amountPercent) {
    if (!isMorphEligiblePattern(pattern)) return null;
    const amount = Number(amountPercent);
    if (!Number.isFinite(amount)) return 0;
    return Math.max(0, Math.min(100, Math.round(amount)));
}

/// Active-step counts the morph supports: whole four-step beats.
export const SUPPORTED_ACTIVE_STEPS = [4, 8, 12, 16];

/**
 * A pattern is morph-eligible when it is straight and its active steps
 * form whole four-step beats. Mirrors the Rust planner's rule.
 */
export function isMorphEligiblePattern(pattern) {
    if (!pattern || pattern.triplet) return false;
    return SUPPORTED_ACTIVE_STEPS.includes(Number(pattern.active_steps));
}

/**
 * Horizontal displacement of one source cell in units of its own card
 * width (a 16th of the four-beat row): warped position minus the cell's
 * straight grid position.
 */
export function morphCellOffsetInCardWidths(assignment, amountPercent) {
    const warpedBeats = warpedOnsetFraction(assignment, amountPercent) * 4;
    return warpedBeats * 4 - (Number(assignment?.step) || 0);
}
