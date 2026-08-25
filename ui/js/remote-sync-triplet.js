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

/**
 * Apply an incoming TRIPLET press.
 *
 * `command.morph` decides which of the two behaviours to reproduce: the
 * 12-step projection the sender wrote, or the plain device triplet flag.
 * Reproducing the wrong one leaves the peers playing different patterns,
 * which is the whole point of mirroring the press rather than the flag.
 *
 * `pressTriplet(indices, next, useMorph)` performs the projection and is
 * injected because it lives on the page's own toolbar module. Without it,
 * this falls back to setting the flag. An older peer with no `morph` field
 * also uses the flag for ON; OFF still takes the restore-capable path so a
 * remembered projection cannot survive after triplet mode is disabled.
 */
export function applyRemoteTripletCommand(command, stateApi, pressTriplet) {
    if (!command || command.command !== 'triplet') return false;
    if (typeof command.triplet !== 'boolean') return false;
    if (typeof stateApi.setTripletBulk !== 'function') return false;

    const targets = topToolbarTripletTargets(stateApi);
    if (targets.length === 0) return false;

    const morph = command.morph === true;
    // Keep this peer's own MORPH box in step, so the UI shows the mode the
    // pattern was actually written with.
    if (typeof stateApi.setTripletMorphSend === 'function') {
        stateApi.setTripletMorphSend(morph);
    }

    const needsProjectionPress = morph || command.triplet === false;
    if (needsProjectionPress && typeof pressTriplet === 'function') {
        const pressResult = pressTriplet(targets, command.triplet, morph);
        if (pressResult && typeof pressResult.then === 'function') {
            return pressResult.then(() => true);
        }
        return true;
    }

    stateApi.setTripletBulk(targets, command.triplet);
    return true;
}
