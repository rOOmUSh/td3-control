// Per-pattern drawer holding the per-step control lanes.
//
// A thin full-width handle sits under a pattern card; clicking it slides
// a drawer open below the card. The drawer shows one row per lane, each
// with a horizontal ON/OFF switch at the left and sixteen small knobs
// aligned over the pattern's sixteen step columns. Each knob has a
// digit readout coloured by value (see `laneColor`).
//
// The module owns no state. The caller supplies `getLanes` and
// `setLanes`, and the drawer re-reads them in `render()`. Pointer drags
// are tracked through one pair of document-level listeners shared by
// every drawer on the page, so a card list that rebuilds its DOM on each
// change cannot leak listeners.
//
// Drawer elements never carry `data-step`: the morph view queries that
// attribute across the whole card. Knob cells use `data-knob-step`.

import {
    LANES, effectiveLane, effectiveLaneValue, laneColor, laneState, normalizeLaneValue,
    randomLaneValues, ratioKey,
} from './step-lanes.js';

const DRAG_PIXELS_PER_UNIT = 3;
const PAGE_STEPS = { cutoff: 8, gate: 10 };
/** Patterns whose drawer was toggled very recently; their next render
 * plays the slide animation even though the card was rebuilt. */
const recentlyToggled = new Map();
const ANIMATE_WINDOW_MS = 600;

let drag = null;
let documentListenersInstalled = false;

/** Live drawers, so a page can re-render every one from state. */
const instances = new Set();

/** Re-render every drawer still attached to the document. */
export function renderAllStepLaneDrawers() {
    for (const drawer of instances) {
        if (!drawer.root || !drawer.root.isConnected) {
            instances.delete(drawer);
            continue;
        }
        drawer.render();
    }
}

function installDocumentListeners(documentRef) {
    if (documentListenersInstalled || !documentRef) return;
    documentListenersInstalled = true;
    documentRef.addEventListener('mousemove', (event) => {
        if (!drag) return;
        const delta = Math.round((drag.startY - event.clientY) / DRAG_PIXELS_PER_UNIT);
        drag.commit(drag.startValue + delta);
    });
    documentRef.addEventListener('mouseup', () => { drag = null; });
}

function indicatorAngle(lane, value) {
    const { min, max } = LANES[lane];
    return ((value - min) / (max - min)) * 300 - 150;
}

function knobCell(lane, step) {
    const spec = LANES[lane];
    return `<div class="mp-lane-cell" data-knob-step="${step}">`
        + `<div class="mp-lane-knob tactile-button" role="slider" tabindex="0"`
        + ` data-drawer-action="knob" data-lane="${lane}" data-knob-step="${step}"`
        + ` aria-label="${spec.label} step ${step + 1}"`
        + ` aria-valuemin="${spec.min}" aria-valuemax="${spec.max}" aria-valuenow="${spec.fallback}">`
        + '<div class="mp-lane-knob-indicator"></div></div>'
        + `<div class="mp-lane-readout" data-lane-readout="${lane}" data-knob-step="${step}">${spec.fallback}</div>`
        + '</div>';
}

function laneRow(lane) {
    const spec = LANES[lane];
    const cells = [];
    for (let step = 0; step < 16; step += 1) cells.push(knobCell(lane, step));
    return `<div class="mp-lane-row" data-lane-row="${lane}">`
        + '<div class="mp-lane-head">'
        + `<span class="mp-lane-label">${spec.label}</span>`
        + `<div class="mp-lane-cell mp-lane-ratio" title="All steps ratio">`
        + `<div class="mp-lane-knob tactile-button" role="slider" tabindex="0"`
        + ` data-drawer-action="knob" data-lane="${lane}" data-ratio="1"`
        + ` aria-label="${spec.label} all steps ratio"`
        + ` aria-valuemin="${spec.min}" aria-valuemax="${spec.max}" aria-valuenow="${spec.mid}">`
        + '<div class="mp-lane-knob-indicator"></div></div>'
        + `<div class="mp-lane-readout" data-lane-ratio-readout="${lane}">${spec.mid}</div>`
        + '</div>'
        + `<button type="button" class="mp-lane-switch" role="switch" aria-checked="false"`
        + ` aria-label="${spec.label} lane" data-drawer-action="lane-toggle" data-lane="${lane}">`
        + '<span class="mp-lane-switch-thumb"></span></button>'
        + `<button type="button" class="mp-lane-random tactile-button" data-drawer-action="lane-random"`
        + ` data-lane="${lane}" aria-label="Randomize ${spec.label} lane" title="Random ${spec.label} per step">RAND</button>`
        + '</div>'
        + `<div class="mp-lane-grid grid grid-cols-16 gap-1 flex-1">${cells.join('')}</div>`
        + '</div>';
}

/**
 * Size the lane head so the knob grid shares the step grid's left and
 * right edges. Both are read from the live layout, so this runs after
 * the card is attached and again whenever the layout can have changed.
 */
export function alignDrawerToGrid(root, grid) {
    if (!root || !grid || !root.isConnected || root.hidden) return;
    const gridRect = grid.getBoundingClientRect();
    const rootRect = root.getBoundingClientRect();
    if (gridRect.width === 0 || rootRect.width === 0) return;
    const style = getComputedStyle(root);
    const padLeft = parseFloat(style.paddingLeft) || 0;
    const padRight = parseFloat(style.paddingRight) || 0;
    // The head may not go negative (a width), the tail may: a negative
    // right margin lets the lane grid reach past the drawer's padding to
    // the step grid's right edge.
    const head = Math.max(0, gridRect.left - rootRect.left - padLeft);
    const tail = rootRect.right - gridRect.right - padRight;
    root.style.setProperty('--mp-lane-head', `${head}px`);
    root.style.setProperty('--mp-lane-tail', `${tail}px`);
    root.style.setProperty('--mp-lane-grid-width', `${gridRect.width}px`);
}

/**
 * Realign every open drawer under `scope`. `gridSelector` locates the
 * step grid inside the drawer's parent card.
 */
export function alignStepLaneDrawers(scope, gridSelector) {
    if (!scope) return;
    for (const root of scope.querySelectorAll('.mp-drawer')) {
        if (root.hidden) continue;
        const card = root.parentElement;
        const grid = card ? card.querySelector(gridSelector) : null;
        alignDrawerToGrid(root, grid);
    }
}

/**
 * Build the handle and drawer for one pattern and append them to `card`.
 *
 * `patternKey` identifies the pattern for the slide animation across a
 * rebuild. `alignTo()` returns the element whose left edge the knob grid
 * must share (the card's step grid); `render()` measures it.
 */
export function createStepLaneDrawer({
    card,
    patternKey,
    getLanes,
    setLanes,
    showCutoffLane = () => true,
    showGateLane = () => true,
    onLaneValue = () => {},
    onLaneToggle = () => {},
    onLaneRandom = () => {},
    onLaneRatio = () => {},
    onOpenChange = () => {},
    alignTo = () => null,
    documentRef = globalThis.document,
}) {
    if (!card || typeof getLanes !== 'function' || typeof setLanes !== 'function') {
        return { render() {}, root: null, handle: null };
    }
    installDocumentListeners(documentRef);

    const handle = documentRef.createElement('button');
    handle.type = 'button';
    handle.className = 'mp-drawer-handle';
    handle.dataset.drawerAction = 'toggle';
    handle.setAttribute('aria-label', 'Toggle step lanes');
    handle.innerHTML = '<span class="mp-drawer-chevron material-symbols-outlined">expand_more</span>';

    const root = documentRef.createElement('div');
    root.className = 'mp-drawer';
    root.hidden = true;
    root.innerHTML = laneRow('cutoff') + laneRow('gate');

    card.appendChild(handle);
    card.appendChild(root);

    function lanes() {
        return laneState({ lanes: getLanes() });
    }

    // A knob target is one step of a lane, or the lane's ratio knob.
    function knobTarget(knob) {
        const { lane } = knob.dataset;
        if (knob.dataset.ratio === '1') return { lane, step: null };
        return { lane, step: Number(knob.dataset.knobStep) };
    }

    function shownValue(target) {
        const current = lanes();
        if (target.step === null) return current[ratioKey(target.lane)];
        return effectiveLane(current, target.lane)[target.step];
    }

    // A step knob shows and moves the effective value. With the ratio off
    // centre the knob's delta is applied to the stored base value, so the
    // readout follows the hand and the base stays recoverable.
    function commitStep(lane, step, rawValue) {
        const current = lanes();
        const ratio = current[ratioKey(lane)];
        const effectiveNow = effectiveLaneValue(lane, current[lane][step], ratio);
        const wanted = normalizeLaneValue(lane, rawValue);
        if (wanted === null || wanted === effectiveNow) return;
        const values = current[lane].slice();
        values[step] = normalizeLaneValue(lane, values[step] + (wanted - effectiveNow));
        if (values[step] === current[lane][step]) return;
        setLanes({ ...current, [lane]: values });
        const shown = effectiveLaneValue(lane, values[step], ratio);
        paintCell(lane, step, shown);
        onLaneValue(lane, step, shown);
    }

    function commitRatio(lane, rawValue) {
        const current = lanes();
        const key = ratioKey(lane);
        const next = normalizeLaneValue(lane, rawValue);
        if (next === null || next === current[key]) return;
        setLanes({ ...current, [key]: next });
        paintLane(lane);
        onLaneRatio(lane, next);
    }

    function commit(target, rawValue) {
        if (target.step === null) commitRatio(target.lane, rawValue);
        else commitStep(target.lane, target.step, rawValue);
    }

    function paintKnob(knob, readout, lane, value) {
        knob.setAttribute('aria-valuenow', String(value));
        const indicator = knob.firstElementChild;
        if (indicator) indicator.style.transform = `rotate(${indicatorAngle(lane, value)}deg)`;
        readout.textContent = String(value);
        readout.style.color = laneColor(lane, value);
    }

    function paintCell(lane, step, value) {
        const knob = root.querySelector(`.mp-lane-knob[data-lane="${lane}"][data-knob-step="${step}"]`);
        const readout = root.querySelector(`.mp-lane-readout[data-lane-readout="${lane}"][data-knob-step="${step}"]`);
        if (!knob || !readout) return;
        paintKnob(knob, readout, lane, value);
    }

    function paintRatio(lane, value) {
        const knob = root.querySelector(`.mp-lane-knob[data-lane="${lane}"][data-ratio="1"]`);
        const readout = root.querySelector(`.mp-lane-readout[data-lane-ratio-readout="${lane}"]`);
        if (!knob || !readout) return;
        paintKnob(knob, readout, lane, value);
    }

    function paintLane(lane) {
        const current = lanes();
        paintRatio(lane, current[ratioKey(lane)]);
        const shown = effectiveLane(current, lane);
        for (let step = 0; step < 16; step += 1) paintCell(lane, step, shown[step]);
    }

    function paintSwitch(lane, on) {
        const button = root.querySelector(`.mp-lane-switch[data-lane="${lane}"]`);
        if (!button) return;
        button.setAttribute('aria-checked', on ? 'true' : 'false');
        const row = root.querySelector(`[data-lane-row="${lane}"]`);
        if (row) row.classList.toggle('is-off', !on);
    }

    function align() {
        alignDrawerToGrid(root, alignTo());
    }

    // Opening a drawer can change the layout around it (a scrollbar
    // appearing shifts every card), so while open the step grid is
    // observed and the lanes realign whenever its box changes.
    let resizeObserver = null;
    function observeGrid(open) {
        const Observer = globalThis.ResizeObserver;
        if (!Observer) return;
        if (!open) {
            if (resizeObserver) resizeObserver.disconnect();
            resizeObserver = null;
            return;
        }
        const grid = alignTo();
        if (!grid || resizeObserver) return;
        resizeObserver = new Observer(() => align());
        resizeObserver.observe(grid);
    }

    function render() {
        const current = lanes();
        const cutoffShown = !!showCutoffLane();
        const gateShown = !!showGateLane();
        // With no lane to show (a device without cutoff control in LIVE
        // mode) the handle itself disappears, so nothing invites a click
        // that could do nothing.
        const anyLane = cutoffShown || gateShown;
        handle.hidden = !anyLane;
        handle.setAttribute('aria-expanded', current.open ? 'true' : 'false');
        handle.classList.toggle('is-open', current.open);
        root.hidden = !current.open || !anyLane;
        const cutoffRow = root.querySelector('[data-lane-row="cutoff"]');
        if (cutoffRow) cutoffRow.hidden = !cutoffShown;
        const stamp = recentlyToggled.get(patternKey);
        if (current.open && stamp !== undefined && Date.now() - stamp < ANIMATE_WINDOW_MS) {
            root.classList.add('mp-drawer--slide');
        } else {
            root.classList.remove('mp-drawer--slide');
        }
        const gateRow = root.querySelector('[data-lane-row="gate"]');
        if (gateRow) gateRow.hidden = !gateShown;
        paintSwitch('cutoff', current.cutoffOn);
        paintSwitch('gate', current.gateOn);
        paintLane('cutoff');
        paintLane('gate');
        observeGrid(current.open);
        if (current.open) align();
    }

    function toggleOpen() {
        const current = lanes();
        const open = !current.open;
        recentlyToggled.set(patternKey, Date.now());
        setLanes({ ...current, open });
        render();
        onOpenChange(open);
    }

    // Fill the lane with fresh random values and switch it on, so the
    // result is heard at once rather than sitting behind an off switch.
    function randomizeLane(lane) {
        const current = lanes();
        const key = lane === 'gate' ? 'gateOn' : 'cutoffOn';
        const values = randomLaneValues(lane);
        if (!values) return;
        setLanes({ ...current, [lane]: values, [key]: true });
        paintSwitch(lane, true);
        paintLane(lane);
        onLaneRandom(lane, values);
    }

    function toggleLane(lane) {
        const current = lanes();
        const key = lane === 'gate' ? 'gateOn' : 'cutoffOn';
        const on = !current[key];
        setLanes({ ...current, [key]: on });
        paintSwitch(lane, on);
        onLaneToggle(lane, on);
    }

    handle.addEventListener('click', (event) => {
        event.stopPropagation();
        toggleOpen();
    });

    // Clicks inside the drawer stay inside it: the card's own click
    // handler would refocus the pattern and rebuild the card under a
    // knob that still has keyboard focus.
    root.addEventListener('click', (event) => {
        event.stopPropagation();
        const random = event.target.closest('[data-drawer-action="lane-random"]');
        if (random) {
            randomizeLane(random.dataset.lane);
            return;
        }
        const button = event.target.closest('[data-drawer-action="lane-toggle"]');
        if (!button) return;
        toggleLane(button.dataset.lane);
    });

    root.addEventListener('wheel', (event) => {
        const knob = event.target.closest('[data-drawer-action="knob"]');
        if (!knob || !event.deltaY) return;
        event.preventDefault();
        const target = knobTarget(knob);
        commit(target, shownValue(target) + (event.deltaY < 0 ? 1 : -1));
    }, { passive: false });

    root.addEventListener('mousedown', (event) => {
        const knob = event.target.closest('[data-drawer-action="knob"]');
        if (!knob || (event.button !== undefined && event.button !== 0)) return;
        event.preventDefault();
        const target = knobTarget(knob);
        drag = {
            startY: event.clientY,
            startValue: shownValue(target),
            commit: (value) => commit(target, value),
        };
    });

    root.addEventListener('dblclick', (event) => {
        const knob = event.target.closest('[data-drawer-action="knob"]');
        if (!knob) return;
        event.preventDefault();
        commit(knobTarget(knob), LANES[knob.dataset.lane].mid);
    });

    root.addEventListener('keydown', (event) => {
        const knob = event.target.closest('[data-drawer-action="knob"]');
        if (!knob) return;
        const target = knobTarget(knob);
        const spec = LANES[target.lane];
        const value = shownValue(target);
        let next = null;
        if (event.key === 'ArrowUp' || event.key === 'ArrowRight') next = value + 1;
        else if (event.key === 'ArrowDown' || event.key === 'ArrowLeft') next = value - 1;
        else if (event.key === 'Home') next = spec.min;
        else if (event.key === 'End') next = spec.max;
        else if (event.key === 'PageUp') next = value + PAGE_STEPS[target.lane];
        else if (event.key === 'PageDown') next = value - PAGE_STEPS[target.lane];
        else if (event.key === 'Enter') next = spec.mid;
        if (next === null) return;
        event.preventDefault();
        commit(target, next);
    });

    const instance = { render, align, root, handle };
    instances.add(instance);
    render();
    return instance;
}
