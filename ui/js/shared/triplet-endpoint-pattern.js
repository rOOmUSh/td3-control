// Build the 12-cell triplet pattern a source morphs into at the 100%
// endpoint, from the planner's own projection.
//
// The Rust planner is the only normative authority: `endpointCells`
// already carries each surviving cell's full step content in target
// order, so this module only reshapes that list into a pattern. It never
// decides which cells survive.
//
// The result is an ordinary pattern with `triplet` on and one active
// step per target cell, so it saves, exports and uploads through the
// existing paths with no marker of where it came from.

const PATTERN_CELL_COUNT = 16;

function restStep() {
    return { note: 'C', transpose: 'NORMAL', accent: false, slide: false, time: 'REST' };
}

function normalizeStep(step) {
    if (!step || typeof step !== 'object') return restStep();
    return {
        note: step.note,
        transpose: step.transpose,
        accent: !!step.accent,
        slide: !!step.slide,
        time: step.time,
    };
}

/**
 * Cells of the plan's endpoint projection in target order, or null when
 * the plan carries no usable projection.
 */
export function endpointCellsInOrder(plan) {
    const cells = plan?.endpointCells;
    if (!Array.isArray(cells) || cells.length === 0) return null;
    if (cells.length > PATTERN_CELL_COUNT) return null;
    const ordered = [...cells].sort(
        (left, right) => Number(left?.targetIndex) - Number(right?.targetIndex),
    );
    for (let index = 0; index < ordered.length; index += 1) {
        if (Number(ordered[index]?.targetIndex) !== index) return null;
        if (!ordered[index]?.step) return null;
    }
    return ordered;
}

/**
 * The 12-cell triplet pattern for `plan`, or null when the plan has no
 * usable endpoint projection. Cells past the projection are rests, which
 * is what the canonical pattern shape uses for inactive steps.
 */
export function endpointPatternFromPlan(plan) {
    const cells = endpointCellsInOrder(plan);
    if (!cells) return null;
    const steps = Array.from({ length: PATTERN_CELL_COUNT }, restStep);
    for (let index = 0; index < cells.length; index += 1) {
        steps[index] = normalizeStep(cells[index].step);
    }
    return { active_steps: cells.length, triplet: true, steps };
}
