// JS-side renderer for the `.steps.txt` pattern DSL. Mirrors
// `src/formats/steps_txt/render.rs` byte-for-byte so the text a user
// paste-lands in Notepad / WhatsApp / chat / email is the same as what the
// backend would emit via `/api/pattern/export?format=steps_txt`.
//
// Every render writes StepDSL v1.1: the lane, morph, and LIVE header keys
// and `|CO:n|GT:n` on each active row. `meta` follows the shape of
// `stepsMetaForExport()` in `steps-txt-meta.js`; absent fields write
// their defaults (64, 50, lanes off, morph off, live off).
//
// Used by the main Control page COPY path (Ctrl+C and the per-card
// COPY FULL button) to write the focused pattern to the system clipboard
// alongside the in-memory clipboard buffer that drives the PASTE FULL
// button. Kept pure (no DOM / no fetch) so it is trivially unit-tested
// and can run in Node.

import { bpmValueToCentibpm, formatBpmCentibpm } from './steps-txt-bpm.js';

const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B', 'C^'];

function transposeChar(t) {
    if (t === 'UP') return 'U';
    if (t === 'DOWN') return 'D';
    return '-';
}

function timeCode(t) {
    if (t === 'TIE') return 'T';
    if (t === 'REST') return 'R';
    if (t === 'TIE_REST') return 'TR';
    return 'N';
}

/** Right-align to width 2 with leading space. Matches Rust `{:>2}`. */
function pad2(s) {
    return s.length >= 2 ? s : ' ' + s;
}

/** Zero-pad a 1-based step index to 2 digits. */
function pad02(n) {
    return n < 10 ? '0' + n : String(n);
}

/**
 * Render `pattern` as a `.steps.txt` file body. Output uses LF line
 * endings (matching the Rust exporter) and ends with a trailing newline
 * after the final comment.
 */
export const DEFAULT_CUTOFF = 64;
export const DEFAULT_GATE = 50;

function onOff(value) {
    return value ? 'on' : 'off';
}

function laneValues(values, fallback, min, max) {
    const out = Array.from({ length: 16 }, () => fallback);
    if (!Array.isArray(values)) return out;
    for (let i = 0; i < 16; i += 1) {
        const v = Number(values[i]);
        if (Number.isInteger(v)) out[i] = Math.max(min, Math.min(max, v));
    }
    return out;
}

export function formatPatternAsStepsTxt(pattern, bpm, meta = {}) {
    if (!pattern || !Array.isArray(pattern.steps) || pattern.steps.length !== 16) {
        throw new Error('formatPatternAsStepsTxt: pattern must have 16 steps');
    }
    if (!Number.isInteger(pattern.active_steps)
        || pattern.active_steps < 1
        || pattern.active_steps > 16) {
        throw new Error('formatPatternAsStepsTxt: active_steps must be 1-16');
    }
    const activeSteps = pattern.active_steps;
    const triplet = pattern.triplet ? 'on' : 'off';
    const centibpm = bpmValueToCentibpm(bpm);
    const source = meta && typeof meta === 'object' ? meta : {};
    const cutoffs = laneValues(source.stepCutoffs, DEFAULT_CUTOFF, 0, 127);
    const gates = laneValues(source.stepGates, DEFAULT_GATE, 1, 100);
    const morph = Number(source.tripletMorphPercent);
    const morphPercent = Number.isInteger(morph) && morph > 0 ? Math.min(100, morph) : 0;

    let out = '';
    out += 'format=td3-stepdsl-v1.1\n';
    out += `active_steps=${activeSteps}\n`;
    out += `triplet_time=${triplet}\n`;
    out += `triplet_morph=${onOff(morphPercent > 0)}\n`;
    out += `triplet_morph_percentage=${morphPercent}\n`;
    out += `bpm=${formatBpmCentibpm(centibpm)}\n`;
    out += `live_update=${onOff(!!source.liveUpdate)}\n`;
    out += `pattern_co_lane=${onOff(!!source.cutoffLaneOn)}\n`;
    out += `pattern_gt_lane=${onOff(!!source.gateLaneOn)}\n`;
    out += '\n';

    for (let i = 0; i < activeSteps; i++) {
        const s = pattern.steps[i];
        const note = NOTE_NAMES.includes(s.note) ? s.note : 'C';
        const t = transposeChar(s.transpose);
        const a = s.accent ? 'A' : '-';
        const sl = s.slide ? 'S' : '-';
        const time = timeCode(s.time);
        out += `${pad02(i + 1)} ${pad2(note)}:${t}${a}${sl}:${time}|CO:${cutoffs[i]}|GT:${gates[i]}\n`;
    }

    out += '\n';
    out += '# NOTE:TAS:TIME|CO:cutoff|GT:gate\n';
    out += '# transpose: U|D|-\n';
    out += '# accent: A|-\n';
    out += '# slide: S|-\n';
    out += '# time: N|T|R|TR\n';
    out += '# Cutoff Control | CO:0-127\n';
    out += '# Gate Control | GT:1-100\n';
    out += '# Lanes | pattern_co_lane, pattern_gt_lane: on/off\n';
    out += '# Live Update | live_update: on/off\n';
    return out;
}
