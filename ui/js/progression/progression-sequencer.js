// Quad-row sequencer rendering for the progression page.
// Renders 4 pattern rows (P1-P4), each with 16 step cards.

import * as state from './progression-state.js';
import { applyStepHighlight, restoreStepHighlight } from '../step-highlight.js';
import { createStepCard as createSharedStepCard } from '../step-card-view.js';

// Lazy DOM lookups - the row containers are injected at page init by
// progression-row.js, which runs after this module has already been imported.
// Resolving on first use (and memoizing) avoids load-order coupling.
const _grids = [null, null, null, null];
const _rows  = [null, null, null, null];
let highlightedPatternIdx = -1;
let highlightedStepIdx = -1;
function grid(p) { return _grids[p] ??= document.getElementById(`grid-p${p + 1}`); }
function row(p)  { return _rows[p]  ??= document.getElementById(`row-p${p + 1}`);  }

/** Render all 4 pattern grids. */
export function render() {
    for (let p = 0; p < 4; p++) {
        renderPatternGrid(p);
    }
    paintStepHighlight();
}

/** Highlight playing step on the active pattern row, clear others. */
export function highlightStep(patIdx, stepIndex) {
    highlightedPatternIdx = Number.isInteger(patIdx) ? patIdx : -1;
    highlightedStepIdx = Number.isInteger(stepIndex) ? stepIndex : -1;
    paintStepHighlight();
}

function paintStepHighlight() {
    for (let p = 0; p < 4; p++) {
        const g = grid(p);
        if (!g) continue;
        g.querySelectorAll('.step-active').forEach(restoreStepHighlight);
        const r = row(p);
        if (r) {
            r.classList.toggle(
                'prog-row-playing',
                p === highlightedPatternIdx && highlightedStepIdx >= 0,
            );
        }
    }
    if (highlightedPatternIdx < 0 || highlightedPatternIdx >= 4
        || highlightedStepIdx < 0 || highlightedStepIdx >= 16) return;
    const g = grid(highlightedPatternIdx);
    if (!g) return;
    const card = g.querySelector(`[data-step="${highlightedStepIdx}"]`);
    if (!card) return;
    applyStepHighlight(card);
}

function renderPatternGrid(patIdx) {
    const g = grid(patIdx);
    if (!g) return;
    g.innerHTML = '';
    for (let i = 0; i < 16; i++) {
        g.appendChild(createStepCardNode(patIdx, i));
    }
}

/**
 * Run a step edit only when the morph policy allows this step: every
 * step at amount 0, the surviving steps at the 100 endpoint, none in
 * between.
 */
function runIfEditable(patIdx, index, edit) {
    if (!state.isTripletMorphStepEditable(patIdx, index)) return;
    edit();
}

function createStepCardNode(patIdx, index) {
    const step = state.getStep(patIdx, index);
    const gated = (edit) => () => runIfEditable(patIdx, index, edit);
    return createSharedStepCard({
        step,
        index,
        activeSteps: state.getActiveSteps(patIdx),
        onWheelNoteChange: (delta) => runIfEditable(
            patIdx, index, () => state.changeNote(patIdx, index, delta),
        ),
        onCardClick: gated(() => state.cycleTime(patIdx, index)),
        onToggleTransposeUp: gated(() => state.toggleTranspose(patIdx, index, 'UP')),
        onToggleTransposeDown: gated(() => state.toggleTranspose(patIdx, index, 'DOWN')),
        onToggleSlide: gated(() => state.toggleSlide(patIdx, index)),
        onToggleAccent: gated(() => state.toggleAccent(patIdx, index)),
    });
}
