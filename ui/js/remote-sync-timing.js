export function eventEpochMicros(
    event,
    timeOriginMs = globalThis.performance && Number.isFinite(globalThis.performance.timeOrigin)
        ? globalThis.performance.timeOrigin
        : Date.now(),
) {
    const stamp = event && Number.isFinite(event.timeStamp) ? event.timeStamp : Date.now();
    const epochMs = stamp > 1_000_000_000_000
        ? stamp
        : (Number.isFinite(timeOriginMs) ? timeOriginMs : Date.now()) + stamp;
    return Math.round(epochMs * 1000);
}

export function cycleDurationMicros(bpm, activeSteps, triplet) {
    const centibpm = Math.max(1, Math.min(30000, Math.round(Number(bpm) * 100)));
    const steps = Number.isFinite(activeSteps)
        ? Math.max(1, Math.min(16, Math.floor(activeSteps)))
        : 16;
    const clocksPerStep = triplet ? 8 : 6;
    const tickPeriodMicros = Math.floor(250000000 / centibpm);
    return tickPeriodMicros * steps * clocksPerStep;
}

export function targetEpochMicrosForPlay(event, bpm, activeSteps, triplet, timeOriginMs) {
    return eventEpochMicros(event, timeOriginMs)
        + cycleDurationMicros(bpm, activeSteps, triplet);
}

export function startSyncFromTargetMicros(targetEpochMicros) {
    if (!Number.isFinite(targetEpochMicros) || targetEpochMicros <= 0) return null;
    return { startedAtEpochMs: targetEpochMicros / 1000 };
}
