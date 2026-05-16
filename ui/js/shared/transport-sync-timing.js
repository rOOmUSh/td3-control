export function preloadStep(activeSteps, configuredStep) {
    const upper = Math.max(1, (Number.isFinite(activeSteps) ? activeSteps : 16) - 1);
    return Math.max(1, Math.min(upper, Math.floor(configuredStep)));
}

export function delayToNextStep(startSync, intervalMs, nowMs = Date.now()) {
    const interval = Number.isFinite(intervalMs) && intervalMs > 0 ? intervalMs : 0;
    if (interval <= 0) return 0;

    const startedAt = startSync && Number.isFinite(startSync.startedAtEpochMs)
        ? startSync.startedAtEpochMs
        : 0;
    if (startedAt <= 0) return interval;

    const elapsed = Math.max(0, nowMs - startedAt);
    const remainder = elapsed % interval;
    return remainder <= 1 ? interval : interval - remainder;
}

export function nextStepInCycle(step, activeSteps) {
    const limit = Number.isFinite(activeSteps) && activeSteps >= 1 && activeSteps <= 16
        ? Math.floor(activeSteps)
        : 16;
    const next = step + 1;
    if (next >= limit) return { step: 0, wrapped: true };
    return { step: next, wrapped: false };
}
