export function preloadStep(activeSteps, configuredStep) {
    const upper = Math.max(1, (Number.isFinite(activeSteps) ? activeSteps : 16) - 1);
    return Math.max(1, Math.min(upper, Math.floor(configuredStep)));
}

export function delayToNextStep(startSync, intervalMs, nowMs = Date.now()) {
    const interval = Number.isFinite(intervalMs) && intervalMs > 0 ? intervalMs : 0;
    if (interval <= 0) return 0;

    const startedAt = startSync && Number.isFinite(startSync.effectiveAtEpochMicros)
        ? startSync.effectiveAtEpochMicros / 1000
        : startSync && Number.isFinite(startSync.startedAtEpochMs)
            ? startSync.startedAtEpochMs
            : 0;
    if (startedAt <= 0) return interval;

    if (startedAt > nowMs) return (startedAt - nowMs) + interval;

    const elapsed = Math.max(0, nowMs - startedAt);
    const remainder = elapsed % interval;
    return remainder <= 1 ? interval : interval - remainder;
}

export function pulsesPerStep(triplet) {
    return triplet ? 8 : 6;
}

function safeActiveSteps(activeSteps) {
    return Number.isFinite(activeSteps) && activeSteps >= 1 && activeSteps <= 16
        ? Math.floor(activeSteps)
        : 16;
}

/**
 * Resolve the musical step at a cumulative MIDI-clock pulse. Pulse zero is
 * the first pulse of step zero; normal steps change every 6 pulses and
 * triplet steps every 8 pulses.
 */
export function stepAtPulse(pulseIndex, anchorPulseIndex, activeSteps, triplet) {
    const pulse = Number.isFinite(pulseIndex) ? Math.floor(pulseIndex) : 0;
    const anchor = Number.isFinite(anchorPulseIndex) ? Math.floor(anchorPulseIndex) : 0;
    const elapsedPulses = Math.max(0, pulse - anchor);
    const stepCount = Math.floor(elapsedPulses / pulsesPerStep(triplet));
    return stepCount % safeActiveSteps(activeSteps);
}

/**
 * Project an authoritative cumulative pulse/epoch snapshot to `now`.
 * `pulseEpochMicros` is the epoch of `pulseIndex`, not the response arrival
 * time. The returned delay therefore remains phase-correct across API
 * latency and live tempo changes.
 */
export function stepSyncFromPulse({
    pulseIndex,
    pulseEpochMicros,
    anchorPulseIndex = 0,
    centibpm,
    activeSteps,
    triplet,
}, nowEpochMicros = Date.now() * 1000) {
    const safeCentibpm = Number.isFinite(centibpm) && centibpm > 0
        ? Math.floor(centibpm)
        : 12000;
    const tickPeriodMicros = Math.floor(250000000 / safeCentibpm);
    const basePulse = Number.isFinite(pulseIndex) ? Math.floor(pulseIndex) : 0;
    const baseEpoch = Number.isFinite(pulseEpochMicros) ? pulseEpochMicros : nowEpochMicros;
    const now = Number.isFinite(nowEpochMicros) ? nowEpochMicros : baseEpoch;
    const elapsedMicros = Math.max(0, now - baseEpoch);
    const elapsedPulses = Math.floor(elapsedMicros / tickPeriodMicros);
    const currentPulseIndex = basePulse + elapsedPulses;
    const anchor = Number.isFinite(anchorPulseIndex) ? Math.floor(anchorPulseIndex) : 0;
    const perStep = pulsesPerStep(triplet);
    const pulsesFromAnchor = Math.max(0, currentPulseIndex - anchor);
    const nextStepPulseIndex = anchor
        + (Math.floor(pulsesFromAnchor / perStep) + 1) * perStep;
    const nextStepEpochMicros = baseEpoch
        + (nextStepPulseIndex - basePulse) * tickPeriodMicros;

    return {
        step: stepAtPulse(currentPulseIndex, anchor, activeSteps, triplet),
        currentPulseIndex,
        nextStepPulseIndex,
        nextStepEpochMicros,
        delayMs: Math.max(0, (nextStepEpochMicros - now) / 1000),
        tickPeriodMicros,
    };
}

/** Project a transport-start acknowledgement to the current step. */
export function stepSyncFromTransportStart({
    startSync,
    bpm,
    activeSteps,
    triplet,
}, nowEpochMicros = Date.now() * 1000) {
    if (!startSync
        || !Number.isFinite(startSync.effectivePulseIndex)
        || !Number.isFinite(startSync.effectiveAtEpochMicros)) {
        return null;
    }
    const centibpm = Number.isFinite(startSync.centibpm)
        ? startSync.centibpm
        : Math.round(Number(bpm) * 100);
    if (!Number.isFinite(centibpm) || centibpm <= 0) return null;
    const anchorPulseIndex = Number.isFinite(startSync.anchorPulseIndex)
        ? startSync.anchorPulseIndex
        : startSync.effectivePulseIndex;
    return stepSyncFromPulse({
        pulseIndex: startSync.effectivePulseIndex,
        pulseEpochMicros: startSync.effectiveAtEpochMicros,
        anchorPulseIndex,
        centibpm,
        activeSteps,
        triplet,
    }, nowEpochMicros);
}

/** Resolve the current fixed-length pattern cycle's cumulative pulse anchor. */
export function cycleAnchorPulseIndex(
    pulseIndex,
    activeSteps,
    triplet,
    anchorPulseIndex = 0,
) {
    const pulse = Number.isFinite(pulseIndex) ? Math.max(0, Math.floor(pulseIndex)) : 0;
    const anchor = Number.isFinite(anchorPulseIndex)
        ? Math.max(0, Math.floor(anchorPulseIndex))
        : 0;
    const cyclePulses = safeActiveSteps(activeSteps) * pulsesPerStep(triplet);
    const elapsedPulses = Math.max(0, pulse - anchor);
    return anchor + elapsedPulses - (elapsedPulses % cyclePulses);
}

/** Project a host-audition cycle acknowledgement to the current step. */
export function stepSyncFromCycle({
    cycleEpochMicros,
    cyclePeriodMicros,
    activeSteps,
}, nowEpochMicros = Date.now() * 1000) {
    const steps = safeActiveSteps(activeSteps);
    const epoch = Number.isFinite(cycleEpochMicros) ? cycleEpochMicros : nowEpochMicros;
    const period = Number.isFinite(cyclePeriodMicros) && cyclePeriodMicros > 0
        ? cyclePeriodMicros
        : steps * 125_000;
    const now = Number.isFinite(nowEpochMicros) ? nowEpochMicros : epoch;
    const elapsed = Math.max(0, now - epoch);
    const stepPeriodMicros = period / steps;
    const elapsedSteps = Math.floor(elapsed / stepPeriodMicros);
    const nextStepEpochMicros = epoch + (elapsedSteps + 1) * stepPeriodMicros;

    return {
        step: elapsedSteps % steps,
        stepPeriodMicros,
        nextStepEpochMicros,
        delayMs: Math.max(0, (nextStepEpochMicros - now) / 1000),
    };
}

/** Preserve the completed fraction of the current step across a tempo edit. */
export function phasePreservingStepDelay(
    previousIntervalMs,
    nextIntervalMs,
    nextBoundaryEpochMs,
    nowMs = Date.now(),
) {
    const previous = Number.isFinite(previousIntervalMs) && previousIntervalMs > 0
        ? previousIntervalMs
        : 0;
    const next = Number.isFinite(nextIntervalMs) && nextIntervalMs > 0
        ? nextIntervalMs
        : 0;
    if (previous <= 0 || next <= 0 || !Number.isFinite(nextBoundaryEpochMs)) return next;

    const previousStart = nextBoundaryEpochMs - previous;
    const progress = Math.max(0, Math.min(1, (nowMs - previousStart) / previous));
    return (1 - progress) * next;
}

export function nextStepInCycle(step, activeSteps) {
    const limit = safeActiveSteps(activeSteps);
    const next = step + 1;
    if (next >= limit) return { step: 0, wrapped: true };
    return { step: next, wrapped: false };
}

/** Number of step boundaries due when an absolute timer callback runs. */
export function elapsedStepBoundaries(boundaryEpochMs, intervalMs, nowMs = Date.now()) {
    if (!Number.isFinite(boundaryEpochMs)
        || !Number.isFinite(intervalMs)
        || intervalMs <= 0
        || !Number.isFinite(nowMs)) {
        return 1;
    }
    return 1 + Math.floor(Math.max(0, nowMs - boundaryEpochMs) / intervalMs);
}
