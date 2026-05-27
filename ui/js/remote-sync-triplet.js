export function topToolbarTripletTargets(stateApi) {
    const checked = typeof stateApi.getCheckedArray === 'function'
        ? stateApi.getCheckedArray()
        : [];
    if (Array.isArray(checked) && checked.length > 0) {
        return checked.filter((idx) => Number.isInteger(idx) && idx >= 0);
    }

    const count = typeof stateApi.getPatternCount === 'function'
        ? stateApi.getPatternCount()
        : 0;
    const safeCount = Number.isInteger(count) && count > 0 ? count : 0;
    return Array.from({ length: safeCount }, (_, idx) => idx);
}

export function applyRemoteTripletCommand(command, stateApi) {
    if (!command || command.command !== 'triplet') return false;
    if (typeof command.triplet !== 'boolean') return false;
    if (typeof stateApi.setTripletBulk !== 'function') return false;

    const targets = topToolbarTripletTargets(stateApi);
    if (targets.length === 0) return false;

    stateApi.setTripletBulk(targets, command.triplet);
    return true;
}
