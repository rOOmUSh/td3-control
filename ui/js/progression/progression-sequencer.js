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
function grid(p) { return _grids[p] ??= document.getElementById(`grid-p${p + 1}`); }
function row(p)  { return _rows[p]  ??= document.getElementById(`row-p${p + 1}`);  }

/** Render all 4 pattern grids. */
export function render() {
    for (let p = 0; p < 4; p++) {
        renderPatternGrid(p);
    }
}

/** Highlight playing step on the active pattern row, clear others. */
export function highlightStep(patIdx, stepIndex) {
    for (let p = 0; p < 4; p++) {
        const g = grid(p);
        if (!g) continue;
        const prev = g.querySelector('.step-active');
        if (prev) restoreStepHighlight(prev);
        const r = row(p);
        if (r) r.classList.toggle('prog-row-playing', p === patIdx && stepIndex >= 0);
    }
    if (patIdx < 0 || stepIndex < 0) return;
    const g = grid(patIdx);
    if (!g) return;
    const card = g.querySelector(`[data-step="${stepIndex}"]`);
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

function createStepCardNode(patIdx, index) {
    const step = state.getStep(patIdx, index);
    return createSharedStepCard({
        step,
        index,
        activeSteps: state.getActiveSteps(patIdx),
        onWheelNoteChange: (delta) => state.changeNote(patIdx, index, delta),
        onCardClick: () => state.cycleTime(patIdx, index),
        onToggleTransposeUp: () => state.toggleTranspose(patIdx, index, 'UP'),
        onToggleTransposeDown: () => state.toggleTranspose(patIdx, index, 'DOWN'),
        onToggleSlide: () => state.toggleSlide(patIdx, index),
        onToggleAccent: () => state.toggleAccent(patIdx, index),
    });
}
