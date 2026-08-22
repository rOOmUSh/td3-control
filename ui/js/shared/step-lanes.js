// Per-step control lanes attached to a pattern.
//
// A pattern may carry `pattern.lanes`, session-scoped performance data
// that never reaches the device pattern memory, the library, or any
// export. The server ignores the field on every pattern payload; the
// values travel as explicit audition request fields instead.
//
//   {
//     open: boolean,          drawer shown under the card
//     cutoffOn: boolean,      per-step cutoff lane active
//     gateOn: boolean,        per-step gate lane active (NO-LIVE only)
//     cutoff: number[16],     CC 74 base value per step, 0..127, default 64
//     gate: number[16],       gate percent base per step, 1..100, default 50
//     cutoffRatio: number,    all-steps ratio knob, 0..127, default 64
//     gateRatio: number,      all-steps ratio knob, 1..100, default 50
//   }
//
// The per-step arrays are what the user set knob by knob. The ratio
// knob scales every step of its lane relative to its own value toward
// the lane maximum (above centre) or minimum (below centre); the values
// that are sent and shown are `effectiveLaneValues(lane, base, ratio)`.
// At centre the ratio is the identity, so turning it back restores the
// per-step values exactly.
//
// Every reader goes through `laneState()` so a pattern written before
// the field existed, or one that came back from the server without it,
// resolves to the defaults.

export const STEP_COUNT = 16;

// `peak` is the value the ratio knob scales toward above centre. For
// cutoff it is 128, one past the MIDI maximum, so the centre 64 sits
// exactly half way between bottom and peak and the knob reads as a
// symmetric ratio; results are clamped to 127 and the top tick pins
// every step there.
export const LANES = {
    cutoff: { min: 0, mid: 64, max: 127, peak: 128, fallback: 64, label: 'CUTOFF' },
    gate: { min: 1, mid: 50, max: 100, peak: 100, fallback: 50, label: 'GATE' },
};

export const LANE_NAMES = Object.keys(LANES);

function clampInt(value, { min, max, fallback }) {
    let numeric = NaN;
    if (typeof value === 'number') numeric = value;
    else if (typeof value === 'string' && value.trim() !== '') numeric = Number(value);
    const safe = Number.isFinite(numeric) ? Math.round(numeric) : fallback;
    return Math.max(min, Math.min(max, safe));
}

export function normalizeLaneValue(lane, value) {
    const spec = LANES[lane];
    if (!spec) return null;
    return clampInt(value, spec);
}

export function defaultLaneValues(lane) {
    const spec = LANES[lane];
    return Array.from({ length: STEP_COUNT }, () => spec.fallback);
}

function normalizeValues(lane, values) {
    const spec = LANES[lane];
    const out = defaultLaneValues(lane);
    if (!Array.isArray(values)) return out;
    for (let i = 0; i < STEP_COUNT; i += 1) {
        out[i] = clampInt(values[i], spec);
    }
    return out;
}

/** Resolve the lanes of `pattern` with every field present and valid. */
export function laneState(pattern) {
    const raw = pattern && typeof pattern.lanes === 'object' && pattern.lanes ? pattern.lanes : {};
    return {
        open: !!raw.open,
        cutoffOn: !!raw.cutoffOn,
        gateOn: !!raw.gateOn,
        cutoff: normalizeValues('cutoff', raw.cutoff),
        gate: normalizeValues('gate', raw.gate),
        cutoffRatio: clampInt(raw.cutoffRatio, LANES.cutoff),
        gateRatio: clampInt(raw.gateRatio, LANES.gate),
    };
}

export function ratioKey(lane) {
    return lane === 'gate' ? 'gateRatio' : 'cutoffRatio';
}

/**
 * One base value scaled by the lane's ratio knob. Above centre the value
 * travels toward the lane maximum by the knob's fraction of the way to
 * the top; below centre it travels toward the minimum by the knob's
 * fraction of the way to the bottom. Centre returns the value unchanged.
 */
export function effectiveLaneValue(lane, value, ratio) {
    const spec = LANES[lane];
    if (!spec) return null;
    const v = clampInt(value, spec);
    const r = clampInt(ratio, spec);
    if (r === spec.mid) return v;
    if (r > spec.mid) {
        const t = r >= spec.max ? 1 : (r - spec.mid) / (spec.peak - spec.mid);
        return Math.min(spec.max, Math.round(v + (spec.peak - v) * t));
    }
    const t = (spec.mid - r) / (spec.mid - spec.min);
    return Math.round(v - (v - spec.min) * t);
}

/** Sixteen effective values for `lane`. */
export function effectiveLaneValues(lane, values, ratio) {
    const base = normalizeValues(lane, values);
    return base.map((v) => effectiveLaneValue(lane, v, ratio));
}

/** Effective values of `lane` for resolved `lanes`. */
export function effectiveLane(lanes, lane) {
    return effectiveLaneValues(lane, lanes[lane], lanes[ratioKey(lane)]);
}

/** Write a normalised copy of `lanes` onto `pattern` and return it. */
export function writeLaneState(pattern, lanes) {
    if (!pattern || typeof pattern !== 'object') return null;
    const next = laneState({ lanes });
    pattern.lanes = next;
    return next;
}

/**
 * Sixteen uniformly random values over the full range of `lane`.
 * `random` is a `Math.random`-shaped source, injectable for tests.
 */
export function randomLaneValues(lane, random = Math.random) {
    const spec = LANES[lane];
    if (!spec) return null;
    const span = spec.max - spec.min + 1;
    return Array.from({ length: STEP_COUNT }, () => {
        const draw = Math.floor(random() * span);
        return spec.min + Math.max(0, Math.min(span - 1, draw));
    });
}

/** True when every value of `lane` is at its default. */
export function isLaneDefault(lane, values) {
    const spec = LANES[lane];
    return normalizeValues(lane, values).every((v) => v === spec.fallback);
}

/**
 * Audition request fields for `pattern`. A lane contributes only while
 * switched on; the gate lane additionally only while `noLive` (host
 * audition) is true, because device-sequenced playback has no per-step
 * gate. Lanes are indexed by source step and travel unchanged at every
 * triplet morph amount: the server moves each value with its cell.
 */
export function laneRequestFields(pattern, { noLive = true } = {}) {
    const lanes = laneState(pattern);
    const fields = {};
    if (lanes.cutoffOn) fields.stepCutoffs = effectiveLane(lanes, 'cutoff');
    if (noLive && lanes.gateOn) fields.stepGates = effectiveLane(lanes, 'gate');
    return fields;
}

/** Lane fields for a host-audition request, at any morph amount. */
export function auditionLaneFields(pattern) {
    return laneRequestFields(pattern, { noLive: true });
}

// --- Readout colour -------------------------------------------------------
//
// The digits of a lane readout are coloured by value so a glance shows
// which steps sit below or above the centre: the minimum is red, the
// centre is a slightly dimmed white, the maximum is a bright green, and
// values in between are linear blends of the nearest two anchors.

export const LANE_COLOR_MIN = [239, 68, 68];
export const LANE_COLOR_MID = [216, 216, 216];
export const LANE_COLOR_MAX = [57, 255, 20];

function blend(a, b, t) {
    return a.map((channel, i) => Math.round(channel + (b[i] - channel) * t));
}

/** CSS `rgb(...)` for `value` on `lane`. */
export function laneColor(lane, value) {
    const spec = LANES[lane];
    if (!spec) return 'rgb(216, 216, 216)';
    const v = clampInt(value, spec);
    let rgb;
    if (v <= spec.mid) {
        const span = spec.mid - spec.min;
        rgb = blend(LANE_COLOR_MIN, LANE_COLOR_MID, span === 0 ? 1 : (v - spec.min) / span);
    } else {
        const span = spec.max - spec.mid;
        rgb = blend(LANE_COLOR_MID, LANE_COLOR_MAX, span === 0 ? 1 : (v - spec.mid) / span);
    }
    return `rgb(${rgb[0]}, ${rgb[1]}, ${rgb[2]})`;
}
