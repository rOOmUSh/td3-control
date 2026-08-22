// JS-side parser for the `.steps.txt` pattern DSL. Mirrors the validation
// rules in `src/formats/steps_txt/` so text that round-trips through the
// backend importer also round-trips through the UI paste path.
//
// Both `td3-stepdsl-v1` and `td3-stepdsl-v1.1` are read. The v1.1 row
// fields (`|CO:n|GT:n`) and header keys are lenient: an out-of-range
// number clamps, a non-numeric value marks the row invalid, a lane is
// present only when every active row carries a numeric value, and an
// unknown `key=value` line is ignored under the v1.1 tag only.
//
// Used by the main Control page PASTE path (Ctrl+V and the per-card
// PASTE FULL button): when the system clipboard holds a valid steps.txt
// body, we consume it directly; otherwise the callers fall back to the
// in-memory FULL clipboard (`td3_multipattern_clipboard`).
//
// Throws a plain Error on malformed input. Kept pure (no DOM / no fetch).

import { parseBpmCentibpm } from './steps-txt-bpm.js';

const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B', 'C^'];

/**
 * Cheap detector - returns true when `text` looks like it starts a
 * `.steps.txt` body. Lets callers skip parse cost / error toast when the
 * system clipboard holds arbitrary text (chat message, URL, etc.).
 */
export function looksLikeStepsTxt(text) {
    if (typeof text !== 'string') return false;
    return text.includes('format=td3-stepdsl-v1');
}

/**
 * Parse `text` into `{ pattern, centibpm }`. The pattern matches the UI shape:
 *   { active_steps: number, triplet: boolean, steps: Step[16] }
 *   Step: { note, transpose: 'NORMAL'|'UP'|'DOWN', accent, slide,
 *           time: 'NORMAL'|'TIE'|'REST'|'TIE_REST' }
 * Throws on any validation failure. Rows through active_steps are required;
 * omitted trailing rows keep their safe defaults.
 */
export function parseStepsTxtDocument(text) {
    if (typeof text !== 'string') throw new Error('parseStepsTxtDocument: not a string');

    let activeSteps = null;
    let triplet = null;
    let centibpm = null;
    let bpmSeen = false;
    let version = null;
    const header = {
        tripletMorph: null,
        tripletMorphPercent: null,
        liveUpdate: null,
        cutoffLaneOn: null,
        gateLaneOn: null,
    };
    const steps = Array.from({ length: 16 }, () => defaultStep());
    // Per-row lane fields: null = absent, NaN = invalid, number = value.
    const cutoffFields = Array.from({ length: 16 }, () => null);
    const gateFields = Array.from({ length: 16 }, () => null);
    let stepsSeen = 0;

    const lines = text.split(/\r?\n/);
    for (let i = 0; i < lines.length; i++) {
        const lineNum = i + 1;
        const trimmed = lines[i].trim();
        if (trimmed === '' || trimmed.startsWith('#')) continue;

        if (trimmed.startsWith('format=')) {
            const val = trimmed.slice('format='.length).trim();
            if (val === 'td3-stepdsl-v1') version = 'v1';
            else if (val === 'td3-stepdsl-v1.1') version = 'v1.1';
            else throw new Error(`line ${lineNum}: unknown format '${val}'`);
            continue;
        }
        if (trimmed.startsWith('active_steps=')) {
            const raw = trimmed.slice('active_steps='.length).trim();
            const v = Number.parseInt(raw, 10);
            if (!Number.isInteger(v) || String(v) !== raw || v < 1 || v > 16) {
                throw new Error(`line ${lineNum}: invalid active_steps '${raw}'`);
            }
            activeSteps = v;
            continue;
        }
        if (trimmed.startsWith('triplet_time=')) {
            const val = trimmed.slice('triplet_time='.length).trim().toLowerCase();
            if (val === 'on') triplet = true;
            else if (val === 'off') triplet = false;
            else throw new Error(`line ${lineNum}: invalid triplet_time '${val}' (expected on/off)`);
            continue;
        }
        if (trimmed.startsWith('bpm=')) {
            if (bpmSeen) throw new Error(`line ${lineNum}: duplicate bpm field`);
            bpmSeen = true;
            const raw = trimmed.slice('bpm='.length);
            if (lines[i].trimEnd() !== lines[i]) {
                throw new Error(`line ${lineNum}: invalid bpm '${raw}'`);
            }
            try {
                centibpm = parseBpmCentibpm(raw);
            } catch (err) {
                throw new Error(`line ${lineNum}: ${err.message}`);
            }
            continue;
        }
        if (looksLikeHeaderLine(trimmed)) {
            const eq = trimmed.indexOf('=');
            const key = trimmed.slice(0, eq).trim();
            const val = trimmed.slice(eq + 1).trim();
            if (key === 'triplet_morph') header.tripletMorph = parseOnOff(val);
            else if (key === 'triplet_morph_percentage') {
                const n = /^\d+$/.test(val) ? Math.min(100, Number(val)) : null;
                header.tripletMorphPercent = n;
            } else if (key === 'live_update') header.liveUpdate = parseOnOff(val);
            else if (key === 'pattern_co_lane') header.cutoffLaneOn = parseOnOff(val);
            else if (key === 'pattern_gt_lane') header.gateLaneOn = parseOnOff(val);
            else if (version !== 'v1.1') {
                // Unknown key under v1: the row parser below rejects it,
                // matching the backend.
                parseRowOrThrow(trimmed, lineNum);
            }
            continue;
        }

        // Step line: "NN XX:TAS:TIME" - same slicing approach as the Rust
        // importer (leading index padded to 2, then a single separator
        // space, then note:tas:time with note right-padded to width 2).
        if (trimmed.length < 10) {
            throw new Error(`line ${lineNum}: step line too short: '${trimmed}'`);
        }
        const idxStr = trimmed.slice(0, 2).trim();
        if (!/^\d+$/.test(idxStr)) {
            throw new Error(`line ${lineNum}: invalid step index '${idxStr}'`);
        }
        const idx = Number(idxStr);
        if (idx < 1 || idx > 16) {
            throw new Error(`line ${lineNum}: step index out of range: ${idx}`);
        }
        if (stepsSeen & (1 << (idx - 1))) {
            throw new Error(`line ${lineNum}: duplicate step index: ${idx}`);
        }

        const body = trimmed.slice(3);
        const segments = body.split('|');
        const rest = segments[0];
        const parts = rest.split(':');
        if (parts.length !== 3) {
            throw new Error(`line ${lineNum}: expected NOTE:TAS:TIME, got '${rest}'`);
        }

        const noteStr = parts[0].trim();
        const tas = parts[1];
        const timeStr = parts[2].trim();

        if (tas.length !== 3) {
            throw new Error(`line ${lineNum}: TAS field must be 3 chars, got '${tas}'`);
        }
        if (!NOTE_NAMES.includes(noteStr)) {
            throw new Error(`line ${lineNum}: unknown note '${noteStr}'`);
        }

        const s = steps[idx - 1];
        s.note = noteStr;

        const t = tas[0], a = tas[1], sl = tas[2];
        if (t === 'U')       s.transpose = 'UP';
        else if (t === 'D')  s.transpose = 'DOWN';
        else if (t === '-')  s.transpose = 'NORMAL';
        else throw new Error(`line ${lineNum}: invalid transpose '${t}' (expected U/D/-)`);

        if (a === 'A')       s.accent = true;
        else if (a === '-')  s.accent = false;
        else throw new Error(`line ${lineNum}: invalid accent '${a}' (expected A/-)`);

        if (sl === 'S')      s.slide = true;
        else if (sl === '-') s.slide = false;
        else throw new Error(`line ${lineNum}: invalid slide '${sl}' (expected S/-)`);

        if (timeStr === 'N')       s.time = 'NORMAL';
        else if (timeStr === 'T')  s.time = 'TIE';
        else if (timeStr === 'R')  s.time = 'REST';
        else if (timeStr === 'TR') s.time = 'TIE_REST';
        else throw new Error(`line ${lineNum}: invalid time '${timeStr}' (expected N/T/R/TR)`);

        for (const segment of segments.slice(1)) {
            const colon = segment.indexOf(':');
            if (colon < 0) continue;
            const key = segment.slice(0, colon).trim().toUpperCase();
            const val = segment.slice(colon + 1).trim();
            if (key === 'CO') cutoffFields[idx - 1] = parseField(val, 0, 127);
            else if (key === 'GT') gateFields[idx - 1] = parseField(val, 1, 100);
        }

        stepsSeen |= 1 << (idx - 1);
    }

    const declaredActiveSteps = activeSteps ?? 16;
    const requiredMask = (1 << declaredActiveSteps) - 1;
    if ((stepsSeen & requiredMask) !== requiredMask) {
        const missing = [];
        for (let i = 1; i <= declaredActiveSteps; i++) {
            if (!(stepsSeen & (1 << (i - 1)))) missing.push(i);
        }
        throw new Error(`missing steps: [${missing.join(', ')}]`);
    }

    const cutoffLane = resolveLane(cutoffFields, declaredActiveSteps, 64, header.cutoffLaneOn);
    const gateLane = resolveLane(gateFields, declaredActiveSteps, 50, header.gateLaneOn);
    const meta = {};
    if (cutoffLane.values) {
        meta.stepCutoffs = cutoffLane.values;
        meta.cutoffLaneOn = cutoffLane.laneOn;
    }
    if (gateLane.values) {
        meta.stepGates = gateLane.values;
        meta.gateLaneOn = gateLane.laneOn;
    }
    if (header.tripletMorph === true && header.tripletMorphPercent !== null) {
        meta.tripletMorphPercent = header.tripletMorphPercent;
    }
    if (header.liveUpdate !== null) meta.liveUpdate = header.liveUpdate;

    return {
        pattern: {
            active_steps: declaredActiveSteps,
            triplet: triplet ?? false,
            steps,
        },
        centibpm,
        meta,
    };
}

function parseOnOff(value) {
    const v = value.toLowerCase();
    if (v === 'on') return true;
    if (v === 'off') return false;
    return null;
}

/** A letter first and a `key=` before any `:`; rows start with a digit. */
function looksLikeHeaderLine(line) {
    if (!/^[A-Za-z]/.test(line)) return false;
    const eq = line.indexOf('=');
    const colon = line.indexOf(':');
    return eq >= 0 && (colon < 0 || eq < colon);
}

function parseField(raw, min, max) {
    if (!/^-?\d+$/.test(raw)) return NaN;
    return Math.max(min, Math.min(max, Number(raw)));
}

/**
 * A lane is present only when every active row carries a numeric value.
 * Values on rows beyond `activeSteps` are kept but never decide presence.
 * The switch is the header key when given, else on iff any active value
 * differs from the first.
 */
function resolveLane(fields, activeSteps, fallback, headerSwitch) {
    const active = Math.max(1, Math.min(16, activeSteps));
    for (let i = 0; i < active; i += 1) {
        if (fields[i] === null || Number.isNaN(fields[i])) return { values: null, laneOn: null };
    }
    const values = fields.map((f) => (f === null || Number.isNaN(f) ? fallback : f));
    const laneOn = headerSwitch !== null
        ? headerSwitch
        : values.slice(0, active).some((v) => v !== values[0]);
    return { values, laneOn };
}

/** Under the v1 tag an unknown header line is a row-parse error. */
function parseRowOrThrow(line, lineNum) {
    if (line.length < 10) throw new Error(`line ${lineNum}: step line too short: '${line}'`);
    const idxStr = line.slice(0, 2).trim();
    throw new Error(`line ${lineNum}: invalid step index '${idxStr}'`);
}

export function parseStepsTxt(text) {
    return parseStepsTxtDocument(text).pattern;
}

function defaultStep() {
    return { note: 'C', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' };
}
