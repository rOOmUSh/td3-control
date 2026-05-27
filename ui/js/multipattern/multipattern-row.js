// Per-pattern card renderer for the main Control page.
//
// Each card lays out in one horizontal strip that mirrors the progression
// page's row palette - column stack (PREVIEW | SHIFT | TRNSPS | RAND |
// COPY | PASTE) on the left, 16-step grid on the right. Extra columns
// unique to the main page sit in the leftmost id column:
//   - selection checkbox (not a progression concern)
//   - P# label tinted by golden-angle HSL (progression uses fixed
//     prog-label-pN palette which tops out at 4)
//   - slot badge showing the device address this pattern maps to under
//     the active A/B mode + scratch slot
//   - DEL button for per-card deletion (progression's rows are fixed P1..P4
//     and can't be deleted, so it has nothing equivalent)
//
// Buttons carry data-action / data-pattern-idx / data-kind / data-shift /
// data-delta attributes; a single delegated listener in multipattern-list.js
// dispatches the action. The 16-step grid still needs programmatic binding
// for wheel/click/flag handlers so its cells are appended via createStepCard.
//
// Preview and paste buttons paint themselves from the clipboard + preview
// controller state at render time; every state.onChange rebuild re-evaluates
// so no extra subscribe is needed.

import { createStepCard } from '../step-card-view.js';
import * as state from './multipattern-state.js';
import * as clipboard from '../progression/progression-clipboard.js';
import * as preview from './multipattern-preview.js';
import { slotFor } from '../shared/slot-targets.js';
import { hslForIndex } from './multipattern-transport-helpers.js';
import { escapeHtml as esc } from '../shared/escape-html.js';
import {
    ROW_BTN_NEUTRAL as BTN_NEUTRAL,
    ROW_BTN_SL as BTN_SL,
    ROW_BTN_AC as BTN_AC,
    ROW_BTN_UD as BTN_UD,
    ROW_BTN_SHIFT as BTN_SHIFT,
    ROW_BTN_DANGER as BTN_DANGER,
    ROW_BTN_TRIPLET as BTN_TRIPLET,
    ROW_DISABLED as DISABLED,
    ROW_COL_LABEL as COL_LABEL,
    ROW_NUM_LABEL as NUM_LABEL,
    ROW_PIPE as PIPE,
    TD3_CHECKBOX as CHECKBOX,
} from '../shared/button-classes.js';

const PREVIEW_ON  = ' ring-1 ring-primary-fixed text-primary-fixed';

/**
 * Build the card DOM for pattern `index`. Returns the root `<div>`. Callers
 * append it to `#multipattern-list`.
 */
export function renderCard(index) {
    const root = document.createElement('div');
    root.className = 'mp-card';
    root.dataset.patternIdx = String(index);

    const focused = state.getFocusedIdx() === index;
    const checked = state.isChecked(index);
    if (focused) root.classList.add('focused');
    if (checked) root.classList.add('checked');

    const row = document.createElement('div');
    row.className = 'mp-card-row flex items-stretch gap-2 px-3 py-2 bg-surface-container-low border-b border-outline-variant/30';
    row.innerHTML = `
      ${idColumn(index)}
      ${PIPE}
      ${previewCol(index)}
      ${PIPE}
      ${shiftCol(index)}
      ${PIPE}
      ${trnspsCol(index)}
      ${PIPE}
      ${randCol(index)}
      ${PIPE}
      ${copyCol(index)}
      ${PIPE}
      ${pasteCol(index)}
      <div class="self-stretch w-px bg-outline-variant opacity-40"></div>
    `;
    row.appendChild(buildGrid(index));
    root.appendChild(row);

    // Clicking anywhere on the card that isn't itself an action button or
    // interactive control focuses this pattern. The delegated handler in
    // multipattern-list.js handles data-action clicks; checkboxes stop
    // propagation via their explicit handler below. Grid cells run their
    // own click callbacks which already focus the card.
    root.addEventListener('click', (e) => {
        if (e.target.closest('[data-action]')) return;
        if (e.target.closest('input, label')) return;
        if (state.getFocusedIdx() !== index) state.setFocused(index);
    });

    // Checkbox handler - has to sit on the actual input (checkboxes on
    // radio-like inputs don't cleanly marry with delegated click dispatch).
    const box = root.querySelector(`input[data-action="check"]`);
    if (box) {
        box.addEventListener('click', (e) => e.stopPropagation());
        box.addEventListener('change', () => state.setChecked(index, box.checked));
    }

    // NO SAVE checkbox - same explicit-handler pattern as the selection
    // checkbox above (delegated click dispatch doesn't suit checkboxes).
    const noSaveBox = root.querySelector(`input[data-action="preview-nosave"]`);
    if (noSaveBox) {
        noSaveBox.addEventListener('click', (e) => e.stopPropagation());
        noSaveBox.addEventListener('change', () => state.setNoSave(index, noSaveBox.checked));
    }

    // Per-pattern STEPS input - `change` commits on Enter / blur (so free
    // typing isn't interrupted by a re-render mid-keystroke); the wheel
    // handler nudges by 1 like the global STEPS field. Garbage typed
    // input is restored to the prior value rather than silently clamped.
    const stepsInput = root.querySelector(`input[data-action="active-steps"]`);
    if (stepsInput) {
        stepsInput.addEventListener('click', (e) => e.stopPropagation());
        stepsInput.addEventListener('change', () => {
            const v = parseInt(stepsInput.value, 10);
            if (!Number.isFinite(v)) {
                stepsInput.value = state.getActiveSteps(index);
                return;
            }
            state.setActiveSteps(index, v);
        });
        stepsInput.addEventListener('wheel', (e) => {
            e.preventDefault();
            const cur = state.getActiveSteps(index);
            const delta = e.deltaY < 0 ? 1 : -1;
            const next = Math.max(1, Math.min(16, cur + delta));
            if (next !== cur) state.setActiveSteps(index, next);
        }, { passive: false });
    }
    return root;
}

function idColumn(i) {
    const p = i + 1;
    const rowColor = hslForIndex(i);
    const assigned = slotFor(i, state.getScratchSlot(), state.getAbMode(), state.getSelectedSlot());
    const slotLabel = assigned ? assigned.label : 'SNAPSHOT';
    const slotClass = assigned
        ? 'text-[0.7rem] font-mono px-2 py-0.5 rounded bg-surface-container-highest text-on-surface-variant text-center'
        : 'text-[0.7rem] font-mono px-2 py-0.5 rounded bg-tertiary-container text-on-tertiary-container text-center';
    const slotTitle = assigned ? `Device slot: ${slotLabel}` : 'Overflow - no device slot';
    const checkedAttr = state.isChecked(i) ? 'checked' : '';
    const activeSteps = state.getActiveSteps(i);
    // Per-pattern STEPS input: same visual language as the global STEPS
    // field but compact, with native spinner arrows hidden so the layout
    // doesn't widen and the only edit affordances are mouse-wheel scroll
    // and free-typing - matching the global control's UX.
    const stepsClass = 'w-9 h-5 bg-surface-container-lowest text-primary-fixed text-[0.7rem] font-mono text-center border-none ring-1 ring-surface-container-highest focus:ring-primary-fixed rounded [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none';
    return `
      <div class="mp-card-id flex flex-col items-stretch gap-1 min-w-[3.25rem]">
        <div class="flex items-center gap-1.5">
          <label class="flex items-center gap-1 cursor-pointer" title="Include in selection">
            <input type="checkbox" data-action="check" data-pattern-idx="${esc(i)}" class="${CHECKBOX}" ${checkedAttr}>
            <span style="color: ${esc(rowColor)}" class="text-[0.85rem] font-black tracking-wider">P${esc(p)}</span>
          </label>
          <input type="number" data-action="active-steps" data-pattern-idx="${esc(i)}"
                 min="1" max="16" value="${esc(activeSteps)}" class="${stepsClass}"
                 title="Active steps for P${esc(p)} (1..16; scroll wheel or type)"/>
        </div>
        <span class="${slotClass}" title="${esc(slotTitle)}">${esc(slotLabel)}</span>
        <button data-action="bank" data-pattern-idx="${esc(i)}" class="${BTN_NEUTRAL}" title="Save this pattern to Bank">BANK</button>
        <button data-action="delete" data-pattern-idx="${esc(i)}" class="${BTN_DANGER}" title="Delete this pattern">DEL</button>
        <div class="mp-drag-handle" draggable="true" data-pattern-idx="${esc(i)}"
             role="button" aria-label="Drag to reorder pattern"
             title="Drag to reorder pattern"></div>
      </div>`;
}

function previewCol(i) {
    const p = i + 1;
    const on = preview.isActive(i);
    const trip = state.getTriplet(i);
    const tripCls = trip ? BTN_TRIPLET : BTN_NEUTRAL;
    // NO SAVE auditions the pattern from the host (timed Note On/Off) without
    // writing it to the device. Forced on - and the box disabled - when live
    // update is off, since edits aren't on the device to play any other way.
    const noSave = state.isNoSave(i);
    const noSaveForced = !state.isLiveUpdate();
    const noSaveTitle = noSaveForced
        ? 'Live update is off: PREVIEW auditions without saving to the device'
        : 'Audition this pattern without saving it to the device';
    const noSaveLabelClass = `${COL_LABEL} flex items-center justify-center gap-1 cursor-pointer select-none`;
    return `
      <div class="flex flex-col items-stretch gap-1">
        <span class="${COL_LABEL}">PREVIEW</span>
        <button data-action="preview" data-pattern-idx="${esc(i)}" class="${BTN_NEUTRAL}${on ? PREVIEW_ON : ''}" title="TD-3 hardware preview">&#9654; P${esc(p)}</button>
        <label class="${noSaveLabelClass}" title="${esc(noSaveTitle)}">
          <input type="checkbox" data-action="preview-nosave" data-pattern-idx="${esc(i)}" class="${CHECKBOX}" ${noSave ? 'checked' : ''} ${noSaveForced ? 'disabled' : ''}>
          NO SAVE
        </label>
        <div class="self-stretch h-px bg-outline-variant opacity-40 my-0.5"></div>
        <span class="${COL_LABEL}">TRIPLET</span>
        <button data-action="triplet" data-pattern-idx="${esc(i)}" class="${tripCls}" title="Toggle triplet timing for P${esc(p)}">${trip ? 'ON' : 'OFF'}</button>
      </div>`;
}

function shiftCol(i) {
    const btn = (n, label) =>
        `<button data-action="shift" data-pattern-idx="${esc(i)}" data-shift="${esc(n)}" class="${BTN_SHIFT}">${label}</button>`;
    const num = (n) => `<span class="${NUM_LABEL}">${esc(n)}</span>`;
    return `
      <div class="flex flex-col items-stretch gap-1">
        <span class="${COL_LABEL}">SHIFT STEPS</span>
        <div class="grid grid-cols-3 gap-0.5 items-center">
          ${btn(-1, '&lsaquo;')}${num(1)}${btn(1, '&rsaquo;')}
          ${btn(-2, '&lsaquo;&lsaquo;')}${num(2)}${btn(2, '&rsaquo;&rsaquo;')}
          ${btn(-4, '&lsaquo;&lsaquo;&lsaquo;')}${num(4)}${btn(4, '&rsaquo;&rsaquo;&rsaquo;')}
        </div>
        <div class="self-stretch h-px bg-outline-variant opacity-40 my-0.5"></div>
        <button data-action="shuffle" data-pattern-idx="${esc(i)}" class="${BTN_NEUTRAL}" title="Shuffle step positions (randomize order; modifiers and notes move with each step)">SHUFFLE</button>
      </div>`;
}

function trnspsCol(i) {
    return `
      <div class="flex flex-col items-stretch gap-1">
        <span class="${COL_LABEL}">TRNSPS</span>
        <button data-action="transpose" data-pattern-idx="${esc(i)}" data-delta="1"   class="${BTN_NEUTRAL}" title="Transpose +1 semitone">+1</button>
        <button data-action="transpose" data-pattern-idx="${esc(i)}" data-delta="-1"  class="${BTN_NEUTRAL}" title="Transpose &minus;1 semitone">&minus;1</button>
        <button data-action="transpose" data-pattern-idx="${esc(i)}" data-delta="12"  class="${BTN_NEUTRAL}" title="Transpose +12 semitones (one octave up)">+12</button>
        <button data-action="transpose" data-pattern-idx="${esc(i)}" data-delta="-12" class="${BTN_NEUTRAL}" title="Transpose &minus;12 semitones (one octave down)">&minus;12</button>
      </div>`;
}

function randCol(i) {
    return `
      <div class="flex flex-col items-stretch gap-1">
        <span class="${COL_LABEL}">RAND</span>
        <button data-action="rand-rst" data-pattern-idx="${esc(i)}" class="${BTN_NEUTRAL}" title="Shuffle rest-mask at NOTE %">RST</button>
        <button data-action="rand-sl"  data-pattern-idx="${esc(i)}" class="${BTN_SL}"      title="Randomize slides">SL</button>
        <button data-action="rand-ac"  data-pattern-idx="${esc(i)}" class="${BTN_AC}"      title="Randomize accents">AC</button>
        <button data-action="rand-ud"  data-pattern-idx="${esc(i)}" class="${BTN_UD}" title="Randomize UP/DOWN at U|D %">U|D</button>
      </div>`;
}

function copyCol(i) {
    return `
      <div class="flex flex-col items-stretch gap-1">
        <span class="${COL_LABEL}">COPY</span>
        <button data-action="copy" data-kind="rest"   data-pattern-idx="${esc(i)}" class="${BTN_NEUTRAL}" title="Copy rest-mask">RST</button>
        <button data-action="copy" data-kind="slide"  data-pattern-idx="${esc(i)}" class="${BTN_SL}"      title="Copy slides">SL</button>
        <button data-action="copy" data-kind="accent" data-pattern-idx="${esc(i)}" class="${BTN_AC}"      title="Copy accents">AC</button>
        <button data-action="copy" data-kind="full"   data-pattern-idx="${esc(i)}" class="${BTN_NEUTRAL}" title="Copy full pattern">FULL</button>
      </div>`;
}

function pasteCol(i) {
    // rest/slide/accent come from the in-memory progression-clipboard buffers
    // (shared across pages); full comes from the main page's own state
    // clipboard (td3_multipattern_clipboard). Every kind gets its own
    // disabled check so the user sees at a glance which slots have content.
    const entries = [
        { kind: 'rest',   cls: BTN_NEUTRAL, title: 'Paste rest-mask', label: 'RST', on: clipboard.has('rest') },
        { kind: 'slide',  cls: BTN_SL,      title: 'Paste slides',    label: 'SL',  on: clipboard.has('slide') },
        { kind: 'accent', cls: BTN_AC,      title: 'Paste accents',   label: 'AC',  on: clipboard.has('accent') },
        // FULL is always enabled: the handler tries the OS clipboard (paste
        // from Notepad / chat in .steps.txt form) before falling back to
        // the in-memory buffer, so we can't know upfront whether a paste
        // will land without actually reading the OS clipboard.
        { kind: 'full',   cls: BTN_NEUTRAL, title: 'Paste full pattern (from OS clipboard or FULL COPY)', label: 'FULL', on: true },
    ];
    const btns = entries.map(({ kind, cls, title, label, on }) =>
        `<button data-action="paste" data-kind="${esc(kind)}" data-pattern-idx="${esc(i)}" class="${cls}${on ? '' : ' ' + DISABLED}" ${on ? '' : 'disabled'} title="${esc(title)}">${label}</button>`,
    ).join('');
    return `
      <div class="flex flex-col items-stretch gap-1">
        <span class="${COL_LABEL}">PASTE</span>
        ${btns}
      </div>`;
}

function buildGrid(index) {
    const grid = document.createElement('div');
    grid.className = 'mp-card-grid grid grid-cols-16 gap-1 flex-1';
    grid.dataset.patternIdx = String(index);

    const pattern = state.getPattern(index);
    const activeSteps = pattern.active_steps;
    const kbEdit = state.isKbEditEnabled();
    const selectedStep = state.getSelectedStep();
    const focusedIdx = state.getFocusedIdx();

    for (let stepIdx = 0; stepIdx < 16; stepIdx++) {
        const cell = createStepCard({
            step: pattern.steps[stepIdx],
            index: stepIdx,
            activeSteps,
            selected: kbEdit && focusedIdx === index && stepIdx === selectedStep,
            onWheelNoteChange: (delta) => {
                focusAndRun(index, () => state.changeNote(index, stepIdx, delta));
            },
            onCardClick: () => {
                focusAndRun(index, () => {
                    if (state.isKbEditEnabled()) {
                        state.setSelectedStep(stepIdx);
                    } else {
                        state.cycleTime(index, stepIdx);
                    }
                });
            },
            onToggleTransposeUp:   () => focusAndRun(index, () => state.toggleTranspose(index, stepIdx, 'UP')),
            onToggleTransposeDown: () => focusAndRun(index, () => state.toggleTranspose(index, stepIdx, 'DOWN')),
            onToggleSlide:         () => focusAndRun(index, () => state.toggleSlide(index, stepIdx)),
            onToggleAccent:        () => focusAndRun(index, () => state.toggleAccent(index, stepIdx)),
        });
        grid.appendChild(cell);
    }
    return grid;
}

/**
 * Step-edit exception: a per-step click must first focus the card
 * if it isn't already.
 */
function focusAndRun(index, edit) {
    if (state.getFocusedIdx() !== index) state.setFocused(index);
    edit();
}
