// 16-step sequencer card rendering and interaction.

import * as state from './multipattern/multipattern-state.js';
import { applyStepHighlight, restoreStepHighlight } from './step-highlight.js';
import { createStepCard as createSharedStepCard } from './step-card-view.js';

const grid = document.getElementById('sequencer-grid');
let highlightedStepIdx = -1;

// Render all 16 step cards
export function render() {
    grid.innerHTML = '';
    for (let i = 0; i < 16; i++) {
        grid.appendChild(createStepCardNode(i));
    }
    paintStepHighlight();
}

/**
 * Highlight the current playing step without rebuilding the DOM.
 * Called by transport at step intervals - does NOT rebuild the DOM.
 */
export function highlightStep(stepIndex) {
    highlightedStepIdx = Number.isInteger(stepIndex) ? stepIndex : -1;
    paintStepHighlight();
}

function paintStepHighlight() {
    grid.querySelectorAll('.step-active').forEach(restoreStepHighlight);
    if (highlightedStepIdx < 0 || highlightedStepIdx >= 16) return;
    const card = grid.querySelector(`[data-step="${highlightedStepIdx}"]`);
    if (!card) return;
    applyStepHighlight(card);
}

function createStepCardNode(index) {
    const step = state.getStep(index);
    return createSharedStepCard({
        step,
        index,
        activeSteps: state.getActiveSteps(),
        selected: state.isKbEditEnabled() && index === state.getSelectedStep(),
        onWheelNoteChange: (delta) => state.changeNote(index, delta),
        onCardClick: () => {
            if (state.isKbEditEnabled()) {
                state.setSelectedStep(index);
            } else {
                state.cycleTime(index);
            }
        },
        onToggleTransposeUp: () => state.toggleTranspose(index, 'UP'),
        onToggleTransposeDown: () => state.toggleTranspose(index, 'DOWN'),
        onToggleSlide: () => state.toggleSlide(index),
        onToggleAccent: () => state.toggleAccent(index),
    });
}
