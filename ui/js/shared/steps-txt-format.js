// JS-side renderer for the `.steps.txt` pattern DSL. Mirrors
// `src/formats/steps_txt.rs::export` byte-for-byte so the text a user
// paste-lands in Notepad / WhatsApp / chat / email is the same as what the
// backend would emit via `/api/pattern/export?format=steps_txt`.
//
// Used by the main Control page COPY path (Ctrl+C and the per-card
// COPY FULL button) to write the focused pattern to the system clipboard
// alongside the in-memory clipboard buffer that drives the PASTE FULL
// button. Kept pure (no DOM / no fetch) so it is trivially unit-tested
// and can run in Node.

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
export function formatPatternAsStepsTxt(pattern) {
    if (!pattern || !Array.isArray(pattern.steps) || pattern.steps.length !== 16) {
        throw new Error('formatPatternAsStepsTxt: pattern must have 16 steps');
    }
    const activeSteps = Number.isInteger(pattern.active_steps) ? pattern.active_steps : 16;
    const triplet = pattern.triplet ? 'on' : 'off';

    let out = '';
    out += 'format=td3-stepdsl-v1\n';
    out += `active_steps=${activeSteps}\n`;
    out += `triplet_time=${triplet}\n`;
    out += '\n';

    for (let i = 0; i < 16; i++) {
        const s = pattern.steps[i];
        const note = NOTE_NAMES.includes(s.note) ? s.note : 'C';
        const t = transposeChar(s.transpose);
        const a = s.accent ? 'A' : '-';
        const sl = s.slide ? 'S' : '-';
        const time = timeCode(s.time);
        out += `${pad02(i + 1)} ${pad2(note)}:${t}${a}${sl}:${time}\n`;
    }

    out += '\n';
    out += '# NOTE:TAS:TIME\n';
    out += '# transpose: U|D|-\n';
    out += '# accent: A|-\n';
    out += '# slide: S|-\n';
    out += '# time: N|T|R|TR\n';
    return out;
}
