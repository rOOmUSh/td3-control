// Shared styling for UP/DN/SL/AC buttons and AC/SL pills used on both the
// Control page (index.html) and the Progression page (progression.html).
// Changes here reflect on both pages.

import { createButton } from './shared/dom-button.js';

const ACTIVE_CLASS = {
    UP: 'is-up',
    DN: 'is-down',
    SL: 'is-slide',
    AC: 'is-accent',
};

export function makeCtrlButton(label, active, onClick) {
    const parts = ['step-ctrl-btn tactile-button'];
    if (active) parts.push('is-active', ACTIVE_CLASS[label]);
    return createButton({
        className: parts.join(' '),
        label,
        stopPropagation: true,
        onClick,
    });
}

export function makeAccentPill() {
    const el = document.createElement('span');
    el.className = 'step-flag-pill step-flag-pill--accent';
    el.textContent = 'AC';
    return el;
}

export function makeSlidePill() {
    const el = document.createElement('span');
    el.className = 'step-flag-pill step-flag-pill--slide';
    el.textContent = 'SL';
    return el;
}
