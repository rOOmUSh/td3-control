// Resolve the single pattern that should be written to the scratch slot when
// LIVE UPDATE is enabled from an idle or host-audition state.

export function resolveLiveUpdateTargetIndex(checkedIndexes, focusedIdx, patternCount) {
    const count = Number.isInteger(patternCount) ? patternCount : 0;
    if (count <= 0) return -1;

    const focusedValid = Number.isInteger(focusedIdx) && focusedIdx >= 0 && focusedIdx < count;
    const checked = Array.isArray(checkedIndexes)
        ? checkedIndexes.filter(i => Number.isInteger(i) && i >= 0 && i < count)
        : [];

    if (checked.length > 0) {
        if (focusedValid && checked.includes(focusedIdx)) return focusedIdx;
        return checked.slice().sort((a, b) => a - b)[0];
    }

    return focusedValid ? focusedIdx : -1;
}
