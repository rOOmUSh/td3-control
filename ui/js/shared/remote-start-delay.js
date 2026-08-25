// How far ahead a remote-synced play is scheduled.
//
// With REMOTE sync on, the master does not start when it is clicked. It picks
// a start instant in the future, sends that instant to every slave, and each
// device begins exactly there, so they share a downbeat instead of each
// starting whenever its own request happened to land. This module decides how
// far ahead that instant is.
//
// Two modes:
//
//   musical  a note value at the current tempo, so the lead stays musical
//            when the tempo changes. The default is one fixed 96-clock bar.
//            That matches the old cycle lead for a full 16-step straight
//            pattern; short and triplet pattern cycles are shorter.
//   ms       a flat number of milliseconds, independent of tempo, for
//            trimming the lead down to whatever the link actually needs.
//
// The tick arithmetic matches `cycleDurationMicros` in `remote-sync-timing.js`,
// including its flooring, while the selected musical division stays fixed.

/** Delay is expressed as a note value at the current tempo. */
export const MODE_MUSICAL = 'musical';

/** Delay is expressed as a flat millisecond count. */
export const MODE_MS = 'ms';

export const MODES = [MODE_MUSICAL, MODE_MS];

/**
 * MIDI clocks per division. The transport runs at 24 clocks per quarter
 * note, so a 4/4 bar is 96 and every entry below is a fraction of that.
 */
export const DIVISIONS = Object.freeze([
    { id: '1/8', label: '1/8', clocks: 12 },
    { id: '1/4', label: '1/4', clocks: 24 },
    { id: '1/2', label: '1/2', clocks: 48 },
    { id: '1bar', label: '1 bar', clocks: 96 },
    { id: '2bars', label: '2 bars', clocks: 192 },
    { id: '4bars', label: '4 bars', clocks: 384 },
]);

/** One bar of the app's transport, in MIDI clocks. */
export const CLOCKS_PER_BAR = 96;

/** Default fixed musical lead. */
export const DEFAULT_DIVISION = '1bar';

/** Flat lead used when the mode is milliseconds and nothing is set. */
export const DEFAULT_MS = 120;

/** Tempo assumed when none is readable, matching the app's own default. */
const DEFAULT_BPM = 120;

/** Largest accepted flat lead. Beyond a minute the server refuses the start. */
export const MAX_MS = 60000;

/** The division entry for `id`, or null. */
export function divisionById(id) {
    return DIVISIONS.find((entry) => entry.id === id) || null;
}

/** `mode` if it is one this module knows, otherwise the musical default. */
export function normalizeMode(mode) {
    return MODES.includes(mode) ? mode : MODE_MUSICAL;
}

/** `id` if it names a division, otherwise the default. */
export function normalizeDivision(id) {
    return divisionById(id) ? id : DEFAULT_DIVISION;
}

/**
 * A flat lead in milliseconds, clamped to `0..MAX_MS`. Anything unreadable
 * resolves to `DEFAULT_MS` rather than to zero, so a bad value cannot
 * silently remove the lead and desynchronise the devices.
 */
export function normalizeMilliseconds(value) {
    const numeric = typeof value === 'number' ? value : Number.parseInt(value, 10);
    if (!Number.isFinite(numeric)) return DEFAULT_MS;
    return Math.max(0, Math.min(MAX_MS, Math.round(numeric)));
}

/**
 * Duration of one MIDI clock tick in microseconds at `bpm`.
 *
 * Deliberately the same expression as `cycleDurationMicros`, floor included:
 * the two have to agree or a `1 bar` lead would drift against the cycle it
 * replaced.
 */
export function clockTickMicros(bpm) {
    const numeric = Number(bpm);
    // A non-finite tempo must not produce a NaN lead: that would travel into
    // the scheduled start epoch and leave every device with no start at all.
    const safe = Number.isFinite(numeric) ? numeric : DEFAULT_BPM;
    const centibpm = Math.max(1, Math.min(30000, Math.round(safe * 100)));
    return Math.floor(250000000 / centibpm);
}

/** Microseconds for `clocks` MIDI clocks at `bpm`. */
export function clocksToMicros(clocks, bpm) {
    return clockTickMicros(bpm) * clocks;
}

/**
 * The lead, in microseconds, for the given settings and tempo.
 *
 * An unusable mode falls back to musical, an unusable division to one bar and
 * an unusable millisecond value to `DEFAULT_MS`, so this always returns a
 * lead that keeps the devices together.
 */
export function startDelayMicros({ mode, division, milliseconds, bpm } = {}) {
    if (normalizeMode(mode) === MODE_MS) {
        return normalizeMilliseconds(milliseconds) * 1000;
    }
    const entry = divisionById(normalizeDivision(division));
    return clocksToMicros(entry.clocks, bpm);
}

/** The same lead in whole milliseconds, for display. */
export function startDelayMillis(settings) {
    return Math.round(startDelayMicros(settings) / 1000);
}
