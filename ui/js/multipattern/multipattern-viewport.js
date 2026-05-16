// Primary-toolbar wiring for A/B mode toggle + viewport chips.
//
// Both surfaces are pure state flips - the actual badge recomputation and
// card hide/show logic live in `multipattern-row.js` (badge) and
// `multipattern-list.js` (visibility). This module is the controller that
// keeps the toolbar UI state (active chip, A/B label) in sync with
// `state.getViewport()` / `state.getAbMode()`.

import * as state from './multipattern-state.js';

export function init({ setStatus } = {}) {
    const status = setStatus || (() => {});

    const btnAb = document.getElementById('btn-ab-mode');
    const abLabel = document.getElementById('ab-mode-label');
    const chipContainer = document.getElementById('viewport-chips');

    if (!btnAb || !chipContainer) {
        console.warn('[multipattern-viewport] primary toolbar viewport/AB elements missing');
        return;
    }

    btnAb.addEventListener('click', () => {
        const next = state.getAbMode() === 'ALTERNATE' ? 'SERIAL' : 'ALTERNATE';
        state.setAbMode(next);
        status(next === 'ALTERNATE'
            ? 'A/B mode: ALTERNATE (A/B interleaved)'
            : 'A/B mode: SERIAL (all As → all Bs)');
    });

    chipContainer.addEventListener('click', (e) => {
        const btn = e.target.closest('.mp-vp-chip');
        if (!btn) return;
        const g = btn.dataset.vpGroup;
        const s = btn.dataset.vpSide;
        const next = (g === 'ALL')
            ? { group: 'ALL', side: 'ALL' }
            : { group: g, side: s };
        state.setViewport(next);
        status(g === 'ALL'
            ? 'Viewport: ALL'
            : `Viewport: G${g}${s}`);
    });

    const syncChrome = () => {
        // A/B label + button tint.
        const mode = state.getAbMode();
        if (abLabel) abLabel.textContent = mode === 'ALTERNATE' ? 'A/B ALT' : 'As→Bs SER';
        btnAb.classList.toggle('is-active', mode === 'SERIAL');

        // Viewport chip highlight - the matching chip gets the active tint.
        const vp = state.getViewport();
        const activeKey = (vp.group === 'ALL') ? 'ALL' : `${vp.group}${vp.side}`;
        for (const chip of chipContainer.querySelectorAll('.mp-vp-chip')) {
            const key = chip.dataset.vpGroup === 'ALL'
                ? 'ALL'
                : `${chip.dataset.vpGroup}${chip.dataset.vpSide}`;
            const isActive = key === activeKey;
            chip.classList.toggle('is-active', isActive);
        }
    };

    state.onChange(syncChrome);
    syncChrome();
}
