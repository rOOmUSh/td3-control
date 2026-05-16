// Single source of truth for the four progression pattern rows.
// Produces the RAND / SHIFT STEPS / COPY / PASTE / PREVIEW column stack plus
// the empty step-grid container (#grid-pN) that progression-sequencer.js
// populates on first render. All click behavior is handled by delegated
// listeners in progression-main.js, so elements only need the right
// data-action / data-pattern-idx / data-kind / data-shift attributes.

import {
    ROW_BTN_NEUTRAL as BTN_NEUTRAL,
    ROW_BTN_SL as BTN_SL,
    ROW_BTN_AC as BTN_AC,
    ROW_BTN_UD as BTN_UD,
    ROW_BTN_SHIFT as BTN_SHIFT,
    ROW_BTN_BANK as BTN_BANK,
    ROW_DISABLED as DISABLED,
    ROW_COL_LABEL as COL_LABEL,
    ROW_NUM_LABEL as NUM_LABEL,
    ROW_PIPE as PIPE,
} from '../shared/button-classes.js';

// Leftmost column: pattern digit (colored per prog-label-pN) sitting above
// the BANK push-to-bank button. Digit size matches the other column headers
// (COL_LABEL) so the row reads as five equal-header columns.
function idColumn(i) {
    const p = i + 1;
    return `
      <div class="flex flex-col items-stretch gap-1">
        <span class="prog-label-p${p} text-[0.7rem] font-black tracking-wider text-center">${p}</span>
        <button data-action="bank" data-pattern-idx="${i}" class="${BTN_BANK}" title="Push to bank">BANK</button>
      </div>`;
}

// TRNSPS: two stacked buttons that shift every step's note name by ±1 semitone
// within the current pattern. Labels include the `T-` prefix plus the numeric
// suffix so they don't get confused with the per-step UP/DN octave flag
// buttons..
function trnspsColumn(i) {
    return `
      <div class="flex flex-col items-stretch gap-1">
        <span class="${COL_LABEL}">TRNSPS</span>
        <button data-action="transpose" data-pattern-idx="${i}" data-delta="1"   class="${BTN_NEUTRAL}" title="Transpose this pattern +1 semitone">+1</button>
        <button data-action="transpose" data-pattern-idx="${i}" data-delta="-1"  class="${BTN_NEUTRAL}" title="Transpose this pattern &minus;1 semitone">&minus;1</button>
        <button data-action="transpose" data-pattern-idx="${i}" data-delta="12"  class="${BTN_NEUTRAL}" title="Transpose this pattern +12 semitones (one octave up)">+12</button>
        <button data-action="transpose" data-pattern-idx="${i}" data-delta="-12" class="${BTN_NEUTRAL}" title="Transpose this pattern &minus;12 semitones (one octave down)">&minus;12</button>
      </div>`;
}

function randColumn(i) {
    return `
      <div class="flex flex-col items-stretch gap-1">
        <span class="${COL_LABEL}">RAND</span>
        <button data-action="rand-rst" data-pattern-idx="${i}" class="${BTN_NEUTRAL}" title="Shuffle rest-mask at NOTE %">RST</button>
        <button data-action="rand-sl"  data-pattern-idx="${i}" class="${BTN_SL}"      title="Randomize slides">SL</button>
        <button data-action="rand-acc" data-pattern-idx="${i}" class="${BTN_AC}"      title="Randomize accents">AC</button>
        <button data-action="rand-ud"  data-pattern-idx="${i}" class="${BTN_UD}" title="Randomize UP/DOWN at U|D %">U|D</button>
      </div>`;
}

function shiftColumn(i) {
    const btn = (n, label) =>
        `<button data-action="shift" data-pattern-idx="${i}" data-shift="${n}" class="${BTN_SHIFT}">${label}</button>`;
    const num = (n) => `<span class="${NUM_LABEL}">${n}</span>`;
    return `
      <div class="flex flex-col items-stretch gap-1">
        <span class="${COL_LABEL}">SHIFT STEPS</span>
        <div class="grid grid-cols-3 gap-0.5 items-center">
          ${btn(-1, '&lsaquo;')}${num(1)}${btn(1, '&rsaquo;')}
          ${btn(-2, '&lsaquo;&lsaquo;')}${num(2)}${btn(2, '&rsaquo;&rsaquo;')}
          ${btn(-4, '&lsaquo;&lsaquo;&lsaquo;')}${num(4)}${btn(4, '&rsaquo;&rsaquo;&rsaquo;')}
        </div>
      </div>`;
}

function copyColumn(i) {
    return `
      <div class="flex flex-col items-stretch gap-1">
        <span class="${COL_LABEL}">COPY</span>
        <button data-action="copy" data-kind="rest"   data-pattern-idx="${i}" class="${BTN_NEUTRAL}" title="Copy rest-mask">RST</button>
        <button data-action="copy" data-kind="slide"  data-pattern-idx="${i}" class="${BTN_SL}"      title="Copy slides">SL</button>
        <button data-action="copy" data-kind="accent" data-pattern-idx="${i}" class="${BTN_AC}"      title="Copy accents">AC</button>
        <button data-action="copy" data-kind="full"   data-pattern-idx="${i}" class="${BTN_NEUTRAL}" title="Copy full pattern to main page">FULL</button>
      </div>`;
}

function pasteColumn(i) {
    // Paste buttons start disabled; progression-main.js#refreshPasteButtons
    // toggles `disabled` + opacity classes whenever the clipboard module
    // gains or loses a kind.
    return `
      <div class="flex flex-col items-stretch gap-1">
        <span class="${COL_LABEL}">PASTE</span>
        <button data-action="paste" data-kind="rest"   data-pattern-idx="${i}" class="${BTN_NEUTRAL} ${DISABLED}" disabled title="Paste rest-mask">RST</button>
        <button data-action="paste" data-kind="slide"  data-pattern-idx="${i}" class="${BTN_SL} ${DISABLED}"      disabled title="Paste slides">SL</button>
        <button data-action="paste" data-kind="accent" data-pattern-idx="${i}" class="${BTN_AC} ${DISABLED}"      disabled title="Paste accents">AC</button>
        <button data-action="paste" data-kind="full"   data-pattern-idx="${i}" class="${BTN_NEUTRAL} ${DISABLED}" disabled title="Paste full pattern from main page">FULL</button>
      </div>`;
}

function previewColumn(i, p) {
    const chips = ARCHETYPE_CHIPS.map(c =>
        `<button data-action="archetype" data-pattern-idx="${i}" data-archetype="${c.key}" class="archetype-chip ${CHIP_BASE_CLASS} whitespace-nowrap" title="Bassline archetype: ${c.label}">${c.label}</button>`
    ).join('');
    return `
      <div class="flex flex-col items-stretch gap-1">
        <span class="${COL_LABEL}">PREVIEW</span>
        <button data-action="pattern-preview" data-pattern-idx="${i}" class="${BTN_NEUTRAL}" title="TD-3 hardware preview (raw pattern)">&#9654; P${p}</button>
        <div class="relative">
          <button data-action="bass-preview" data-pattern-idx="${i}" class="${BTN_NEUTRAL} w-full" title="Pick and audition a bassline archetype">&#9654; BASSLINE</button>
          <div data-bass-menu="${i}" class="hidden absolute left-full top-0 ml-2 z-30 flex flex-col gap-1 p-2 rounded-lg bg-surface-container shadow-xl border border-outline-variant">
            <span class="${COL_LABEL}">BASS</span>
            ${chips}
          </div>
        </div>
        <!-- <button data-action="midi-preview"    data-pattern-idx="${i}" class="${BTN_NEUTRAL}" title="WebAudio preview (bassline)">&#9654; BASSLINE MIDI</button> -->
      </div>`;
}

// Bass archetype chips - now rendered inside the BASSLINE dropdown menu
// anchored on the per-row PREVIEW column. Clicking a chip swaps the active
// archetype for this pattern in state AND sends that bassline to the TD-3
// for audition. The default chip (lit on first generation) is set by the
// selector heuristic in bassline/selector.js.
const ARCHETYPE_CHIPS = [
    { key: 'pedal',     label: 'PEDAL'   },
    { key: 'rootPulse', label: 'PULSE'   },
    { key: 'offbeat',   label: 'OFFBEAT' },
    { key: 'shadow',    label: 'SHADOW'  },
    { key: 'arpeggio',  label: 'ARP'     },
];
const CHIP_BASE_CLASS = BTN_NEUTRAL;

function rowHtml(i) {
    const p = i + 1;
    return `
      <div class="flex items-start gap-2">
        ${idColumn(i)}
        ${PIPE}
        ${previewColumn(i, p)}
        ${PIPE}
        ${shiftColumn(i)}
        ${PIPE}
        ${trnspsColumn(i)}
        ${PIPE}
        ${randColumn(i)}
        ${PIPE}
        ${copyColumn(i)}
        ${PIPE}
        ${pasteColumn(i)}
        <div class="self-stretch w-px bg-outline-variant opacity-40 ml-auto"></div>
        <div id="grid-p${p}" class="grid grid-cols-16 gap-1 w-2/3"></div>
      </div>`;
}

/** Populate `container` with four pattern rows (P1..P4). */
export function renderAllPatternRows(container) {
    if (!container) throw new Error('progression-row: missing container');
    container.innerHTML = '';
    for (let i = 0; i < 4; i++) {
        const row = document.createElement('div');
        row.id = `row-p${i + 1}`;
        row.className = `prog-row-p${i + 1} rounded-xl p-3`;
        row.innerHTML = rowHtml(i);
        container.appendChild(row);
    }
}
