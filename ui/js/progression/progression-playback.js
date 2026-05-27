// Shared progression playback helpers.
//
// `progression-main.js` owns the DOM and button wiring; this module owns the
// sequencing contract that the E2E plan cares about:
//   1. stop any active previews (MIDI, bassline, raw pattern)
//   2. preload the first timeline pattern into the scratch slot
//   3. await transport start only after the preload completes
//   4. flip playing state and start the progression transport animation

export function firstTimelinePatternIndex(timeline) {
    const firstPatNum = Array.isArray(timeline) && timeline.length > 0 ? timeline[0] : 1;
    const n = Number.isFinite(firstPatNum) ? firstPatNum : 1;
    return Math.max(0, Math.min(3, n - 1));
}

export async function startPlayback({
    api,
    timeline,
    getPattern,
    scratch,
    bpm,
    transport,
    stopAllPreviews,
    liveUpdate = true,
    setPlaying,
    setStatus,
}) {
    if (typeof stopAllPreviews === 'function') {
        await stopAllPreviews();
    }

    const firstPatIdx = firstTimelinePatternIndex(timeline);
    if (!liveUpdate) {
        await api.auditionPattern(getPattern(firstPatIdx), bpm, true);
        setPlaying(true);
        await transport.start(null);
        setStatus(`Host audition: P${firstPatIdx + 1} (no save)`);
        return firstPatIdx;
    }

    await api.savePattern(
        scratch.group, scratch.pattern, scratch.side,
        getPattern(firstPatIdx)
    );
    const startSync = await api.transportStart(bpm);
    setPlaying(true);
    await transport.start(startSync);
    setStatus(`Playing - P${firstPatIdx + 1} → ${scratch.label}`);
    return firstPatIdx;
}

export async function stopPlayback({
    api,
    transport,
    resetTimeline,
    auditionMode = false,
    setPlaying,
    setStatus,
}) {
    if (typeof transport.stopWrapSync === 'function') {
        transport.stopWrapSync();
    }
    if (auditionMode) {
        await api.auditionStop();
    } else {
        await api.transportStop();
    }
    setPlaying(false);
    transport.stop();
    resetTimeline();
    setStatus('Stopped - timeline reset');
}
