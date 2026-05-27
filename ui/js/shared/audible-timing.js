export function snapshotTiming(pattern, fallback = {}) {
    const rawSteps = pattern && Number.isFinite(pattern.active_steps)
        ? pattern.active_steps
        : fallback.activeSteps;
    const activeSteps = Number.isFinite(rawSteps)
        ? Math.max(1, Math.min(16, Math.floor(rawSteps)))
        : 16;
    const triplet = pattern && typeof pattern.triplet === 'boolean'
        ? pattern.triplet
        : fallback.triplet === true;
    return { activeSteps, triplet };
}

export function playbackTiming({ liveUpdate, auditionMode, audibleTiming, fallbackTiming }) {
    if (liveUpdate && !auditionMode && audibleTiming) {
        return audibleTiming;
    }
    return fallbackTiming || { activeSteps: 16, triplet: false };
}

export function adoptQueuedTiming(currentTiming, queuedTiming) {
    return queuedTiming || currentTiming || null;
}
