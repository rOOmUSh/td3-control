export function resetToolbarLabel(checkedCount) {
    if (!Number.isInteger(checkedCount) || checkedCount <= 0) {
        return 'RESET ALL PATTERNS';
    }
    return checkedCount === 1
        ? 'RESET PATTERN (1)'
        : `RESET PATTERNS (${checkedCount})`;
}

export function resetToolbarTitle(checkedCount) {
    if (!Number.isInteger(checkedCount) || checkedCount <= 0) {
        return 'Reset every pattern to a blank pattern';
    }
    return checkedCount === 1
        ? 'Reset the checked pattern to a blank pattern'
        : `Reset ${checkedCount} checked patterns to blank patterns`;
}

export function resetCheckedOrAll(state) {
    const checked = state.getCheckedArray();
    if (checked.length === 0) {
        state.resetAllPatterns();
        return { mode: 'all', count: state.getPatternCount() };
    }

    for (const index of checked) {
        state.resetPattern(index);
    }
    return { mode: 'checked', count: checked.length };
}
