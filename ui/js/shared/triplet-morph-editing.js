// Editing policy across the morph range.
//
//   amount 0        canonical editing, every step
//   amount 1..99    locked; positions are mid-transform and a source
//                   step has no settled identity to edit
//   amount 100      editing limited to the surviving source steps, the
//                   ones the endpoint actually renders and sounds
//
// A losing step is not editable at the endpoint: it is omitted from the
// derived projection, so an edit to it would be invisible and would not
// sound. Randomizers use the same survivor mask so they only rewrite
// notes that remain visible in triplet mode.

export const MORPH_EDIT_ALL = 'all';
export const MORPH_EDIT_LOCKED = 'locked';
export const MORPH_EDIT_SURVIVORS = 'survivors';

export function morphEditMode(amountPercent) {
    const amount = Number(amountPercent) || 0;
    if (amount <= 0) return MORPH_EDIT_ALL;
    if (amount >= 100) return MORPH_EDIT_SURVIVORS;
    return MORPH_EDIT_LOCKED;
}

/**
 * Surviving source steps from a cached plan, or null when the plan is
 * not available yet.
 */
export function survivingSteps(plan) {
    const assignments = plan?.assignments;
    if (!Array.isArray(assignments) || assignments.length === 0) return null;
    const steps = new Set();
    for (const assignment of assignments) {
        if (assignment?.survivor) steps.add(Number(assignment.step));
    }
    return steps;
}

/**
 * Steps the user may edit right now: null means every step, an empty
 * set means nothing is editable.
 */
export function editableSteps(amountPercent, plan) {
    switch (morphEditMode(amountPercent)) {
        case MORPH_EDIT_ALL:
            return null;
        case MORPH_EDIT_SURVIVORS: {
            // Without a plan the survivors are unknown; refuse rather
            // than risk editing a step the endpoint drops.
            const steps = survivingSteps(plan);
            return steps === null ? new Set() : steps;
        }
        default:
            return new Set();
    }
}

export function isStepEditable(amountPercent, plan, step) {
    const allowed = editableSteps(amountPercent, plan);
    return allowed === null || allowed.has(Number(step));
}

/** True when no step at all can be edited at this amount. */
export function isFullyLocked(amountPercent, plan) {
    const allowed = editableSteps(amountPercent, plan);
    return allowed !== null && allowed.size === 0;
}

/**
 * Status text explaining the current restriction, or null when
 * everything is editable.
 */
export function morphEditNotice(amountPercent) {
    switch (morphEditMode(amountPercent)) {
        case MORPH_EDIT_ALL:
            return null;
        case MORPH_EDIT_SURVIVORS:
            return 'Triplet endpoint: only the notes shown in triplet mode are editable.';
        default:
            return 'Triplet audition is in transition. Use TRIPLET 0 or 100 to edit.';
    }
}
