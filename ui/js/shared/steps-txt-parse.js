// JS-side parser for the `.steps.txt` pattern DSL. Mirrors the validation
// rules in `src/formats/steps_txt.rs::import_document` so text that round-trips
// through the backend importer also round-trips through the UI paste path.
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
    const steps = Array.from({ length: 16 }, () => defaultStep());
    let stepsSeen = 0;

    const lines = text.split(/\r?\n/);
    for (let i = 0; i < lines.length; i++) {
        const lineNum = i + 1;
        const trimmed = lines[i].trim();
        if (trimmed === '' || trimmed.startsWith('#')) continue;

        if (trimmed.startsWith('format=')) {
            const val = trimmed.slice('format='.length).trim();
            if (val !== 'td3-stepdsl-v1') {
                throw new Error(`line ${lineNum}: unknown format '${val}'`);
            }
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

        const rest = trimmed.slice(3);
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

    return {
        pattern: {
            active_steps: declaredActiveSteps,
            triplet: triplet ?? false,
            steps,
        },
        centibpm,
    };
}

export function parseStepsTxt(text) {
    return parseStepsTxtDocument(text).pattern;
}

function defaultStep() {
    return { note: 'C', transpose: 'NORMAL', accent: false, slide: false, time: 'NORMAL' };
}
