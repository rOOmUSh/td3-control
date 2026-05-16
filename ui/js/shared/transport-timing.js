export function stepIntervalMs(bpm, triplet) {
    const safeBpm = Number.isFinite(bpm) && bpm > 0 ? bpm : 120;
    const stepsPerBeat = triplet ? 3 : 4;
    return 60000 / (safeBpm * stepsPerBeat);
}
