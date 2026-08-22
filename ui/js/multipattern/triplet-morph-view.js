// Derived triplet morph renderer for the Control multipattern list.
//
// The canonical pattern data is never mutated for visual convenience:
// this module only positions the existing 16 step columns along the
// four-beat row from the cached Rust plan's rational offsets plus the
// current amount, marks merge state on the losing cell, drops losers at
// the retirement point (leaving exactly 12 target cells), widens the
// winner that absorbed each one, and locks canonical grid editing while
// the amount is above zero. At 0 every derived style and attribute is
// cleared.
//
// A step's note card and its UP/DN/SL/AC control block are siblings in
// one column, and the column is what moves: the two travel together so
// a step stays a single readable unit through the sweep, and a loser
// that leaves takes its controls with it.

import { api } from '../api.js';
import * as state from './multipattern-state.js';
import { canonicalPatternText } from '../shared/pattern-canonical.js';
import {
    morphCellOffsetInCardWidths,
    warpedOnsetFraction,
} from '../shared/triplet-morph-timing.js';
import { editableSteps, isFullyLocked } from '../shared/triplet-morph-editing.js';

/**
 * Mark the control block of every surviving step so the user can see
 * which notes stay editable at the triplet endpoint. `container` is any
 * element holding `.step-controls[data-controls-step]` blocks.
 */
export function markEditableControls(container, plan, amountPercent) {
    const blocks = container.querySelectorAll('.step-controls[data-controls-step]');
    const survivors = amountPercent > 0 ? editableSteps(100, plan) : null;
    for (const block of blocks) {
        const step = Number(block.dataset.controlsStep);
        const editable = amountPercent > 0 && survivors !== null && survivors.has(step);
        block.classList.toggle('step-morph-editable', editable);
    }
}

let doc = null;
const plansInFlight = new Set();

/**
 * Amount at which a losing attack stops sounding. Mirrors
 * COLLISION_RETIREMENT_AMOUNT_PERCENT in
 * `src/web/clock/audition_runner/morph_intermediate.rs`; the derived
 * view has to drop the loser at the same point the schedule does or the
 * grid shows a note that is no longer playing.
 */
export const MORPH_RETIREMENT_PERCENT = 80;

/** Source cells per beat, matching the planner's fixed four-cell beat. */
const STEPS_PER_BEAT = 4;

/**
 * How far a surviving cell stretches because the loser it absorbed has
 * been retired, keyed by the surviving source step.
 *
 * The winner covers its own cell plus the slot the loser vacated, and
 * that distance is the gap between the two warped positions, which
 * closes as they converge on the shared target: widest at the retirement
 * point, nothing at the endpoint where the winner is square on the
 * triplet grid again.
 *
 * The stretch always runs towards the swallowed slot. Which side that is
 * depends on the beat's pair choice: a loser that collides forwards sat
 * ahead of its winner, so the card grows backwards, and a loser that
 * collides backwards sat behind it, so the card grows forwards. Growing
 * the wrong way would push the card across a neighbour it never touched
 * and read as timing the note does not have.
 *
 * Returns `step -> { extra, backwards }` with `extra` in card widths.
 */
export function absorbedStretches(plan, amountPercent) {
    const stretches = new Map();
    const assignments = plan?.assignments;
    if (!Array.isArray(assignments)) return stretches;
    if (Number(amountPercent) < MORPH_RETIREMENT_PERCENT) return stretches;
    for (const loser of assignments) {
        if (loser?.survivor) continue;
        // Target offsets are local to a beat and repeat every beat, so
        // the winner has to be found inside the loser's own beat. A
        // whole-plan search matches the first beat every time and leaves
        // the other winners unstretched.
        const beat = Math.floor(Number(loser?.step) / STEPS_PER_BEAT);
        const winner = assignments.find(
            (candidate) => candidate?.survivor
                && Math.floor(Number(candidate.step) / STEPS_PER_BEAT) === beat
                && rationalsEqual(candidate.targetOffset, loser?.targetOffset),
        );
        if (!winner) continue;
        const gap = (warpedOnsetFraction(winner, amountPercent)
            - warpedOnsetFraction(loser, amountPercent)) * 16;
        if (!Number.isFinite(gap) || gap === 0) continue;
        stretches.set(Number(winner.step), { extra: Math.abs(gap), backwards: gap > 0 });
    }
    return stretches;
}

function rationalsEqual(left, right) {
    const a = Number(left?.num) * Number(right?.den);
    const b = Number(right?.num) * Number(left?.den);
    return Number.isFinite(a) && Number.isFinite(b) && a === b;
}

/**
 * Presentation of one source cell at the given amount: CSS translateX
 * percentage of its own width, merge opacity, retirement hiding, the
 * absorbed width factor, and an accessible role. Visual opacity
 * communicates removal only; it is never MIDI velocity.
 */
export function cellPresentation(assignment, amountPercent, stretch = null) {
    const offset = morphCellOffsetInCardWidths(assignment, amountPercent);
    const survivor = !!assignment?.survivor;
    const anchor = (Number(assignment?.step) || 0) % 4 === 0;
    const progress = Math.max(0, Math.min(100, Number(amountPercent) || 0)) / 100;
    const retired = !survivor && Number(amountPercent) >= MORPH_RETIREMENT_PERCENT;
    const merging = !survivor && progress > 0;
    return {
        translatePercent: offset * 100,
        role: anchor ? 'anchor' : (survivor ? 'survivor' : 'loser'),
        merging,
        opacity: survivor ? 1 : Math.max(0.25, 1 - 0.75 * progress),
        hidden: retired,
        // A merging loser slides under the winner it is colliding into,
        // and its control block is wider than the overlap, so the left
        // half of it stays sticking out as a dim stub. Nothing can be
        // edited mid-sweep and the block belongs to a note that is on its
        // way out, so it goes as soon as the merge starts. The note card
        // keeps travelling, which is what the derived view is for.
        controlsHidden: merging,
        stretch,
    };
}

/**
 * Element the derived geometry is applied to: the column that holds a
 * note card and its UP/DN/SL/AC control block, so both move as one.
 * Falls back to the card when it has no column parent.
 *
 * The column and the card have the same width, so a translateX
 * percentage means the same displacement on either.
 */
function movementNode(cell) {
    return cell.parentElement || cell;
}

/**
 * Apply the derived presentation to one card's step cells. `cells` is
 * indexed by source step and holds the note cards; geometry lands on
 * their columns while the role and merge attributes stay on the cards
 * that carry the step identity. Passing a null plan or amount 0
 * restores the canonical cells.
 */
export function applyToCells(cells, plan, amountPercent) {
    const stretches = absorbedStretches(plan, amountPercent);
    for (let step = 0; step < cells.length; step += 1) {
        const cell = cells[step];
        if (!cell) continue;
        const node = movementNode(cell);
        const assignment = plan?.assignments?.[step];
        if (!assignment || !(amountPercent > 0)) {
            node.style.transform = '';
            node.style.opacity = '';
            node.style.visibility = '';
            const canonicalControls = node.querySelector?.('.step-controls');
            if (canonicalControls) canonicalControls.style.visibility = '';
            cell.style.width = '';
            cell.style.marginLeft = '';
            delete cell.dataset.morphRole;
            delete cell.dataset.morphMerging;
            delete cell.dataset.morphAbsorbed;
            continue;
        }
        const view = cellPresentation(assignment, amountPercent, stretches.get(step) ?? null);
        node.style.transform = `translateX(${view.translatePercent.toFixed(3)}%)`;
        node.style.opacity = view.opacity === 1 ? '' : String(view.opacity);
        node.style.visibility = view.hidden ? 'hidden' : '';
        const controls = node.querySelector?.('.step-controls');
        if (controls) controls.style.visibility = view.controlsHidden ? 'hidden' : '';
        // Only the note card widens. Its control block keeps one step's
        // width so the UP/DN/SL/AC grid stays square and undistorted.
        if (view.stretch) {
            const { extra, backwards } = view.stretch;
            cell.style.width = `${((1 + extra) * 100).toFixed(3)}%`;
            // A negative left margin pins the right edge so the extra
            // width falls backwards; without it the card keeps its left
            // edge and the width runs forwards instead.
            cell.style.marginLeft = backwards ? `${(-extra * 100).toFixed(3)}%` : '';
            cell.dataset.morphAbsorbed = backwards ? 'backwards' : 'forwards';
        } else {
            cell.style.width = '';
            cell.style.marginLeft = '';
            delete cell.dataset.morphAbsorbed;
        }
        cell.dataset.morphRole = view.role;
        if (view.merging && !view.hidden) cell.dataset.morphMerging = 'true';
        else delete cell.dataset.morphMerging;
    }
}

/**
 * Move a drawer's lane cells with their step columns. Each lane cell
 * follows the same translation, fade, and retirement as the note card
 * of its source step, so a value stays under the note it belongs to
 * through the sweep. Lane cells never stretch: a knob keeps one step's
 * width so it stays round.
 */
export function applyToLaneCells(scope, plan, amountPercent) {
    if (!scope) return;
    const cells = scope.querySelectorAll('.mp-drawer .mp-lane-cell[data-knob-step]');
    for (const cell of cells) {
        const step = Number(cell.dataset.knobStep);
        const assignment = plan?.assignments?.[step];
        if (!assignment || !(amountPercent > 0)) {
            cell.style.transform = '';
            cell.style.opacity = '';
            cell.style.visibility = '';
            delete cell.dataset.morphRole;
            continue;
        }
        const view = cellPresentation(assignment, amountPercent, null);
        cell.style.transform = `translateX(${view.translatePercent.toFixed(3)}%)`;
        cell.style.opacity = view.opacity === 1 ? '' : String(view.opacity);
        cell.style.visibility = view.hidden ? 'hidden' : '';
        cell.dataset.morphRole = view.role;
    }
}

function cardCells(card) {
    const cells = [];
    for (let step = 0; step < 16; step += 1) {
        cells.push(card.querySelector(`[data-step="${step}"]`));
    }
    return cells;
}

/**
 * Request and cache the Rust plan for every pattern that lacks one.
 * Responses are keyed by exact canonical source text; a response whose
 * session or source text went stale is ignored without touching the UI.
 */
export function ensurePlans() {
    if (!state.isTripletMorphActive()) return;
    const texts = new Set();
    for (let idx = 0; idx < state.getPatternCount(); idx += 1) {
        const pattern = state.getPattern(idx);
        if (!pattern) continue;
        const text = canonicalPatternText(pattern);
        if (texts.has(text) || state.getTripletMorphPlan(text) || plansInFlight.has(text)) {
            continue;
        }
        texts.add(text);
        plansInFlight.add(text);
        api.tripletMorphPlan(pattern)
            .then((response) => {
                plansInFlight.delete(text);
                if (!state.isTripletMorphActive()) return;
                if (!response?.eligible || !response.plan) return;
                state.setTripletMorphPlan(text, response.plan);
                render();
            })
            .catch(() => {
                plansInFlight.delete(text);
            });
    }
}

/** Plan assignments for the pattern at `idx`, or null while pending. */
export function planForPattern(idx) {
    const pattern = state.getPattern(idx);
    if (!pattern) return null;
    return state.getTripletMorphPlan(canonicalPatternText(pattern));
}

export function render() {
    if (!doc) return;
    const container = doc.getElementById('multipattern-list');
    if (!container) return;
    const amount = state.getTripletMorphPercent();
    if (amount > 0) ensurePlans();
    const cards = container.querySelectorAll('.mp-card[data-pattern-idx]');
    for (const card of cards) {
        const idx = Number(card.dataset.patternIdx);
        const grid = card.querySelector('.mp-card-grid');
        const plan = amount > 0 ? planForPattern(idx) : null;
        applyToCells(cardCells(card), plan, amount);
        applyToLaneCells(card, plan, amount);
        markEditableControls(card, plan, amount);
        if (grid) {
            // Mid-transform every position is unsettled, so the grid
            // stops taking pointer input entirely. At the endpoint the
            // surviving cells are editable again and the per-step gate
            // in the row module decides which ones.
            grid.style.pointerEvents = isFullyLocked(amount, plan) ? 'none' : '';
            if (amount > 0) grid.dataset.morphAmount = String(amount);
            else delete grid.dataset.morphAmount;
        }
    }
}

export function init({ documentRef = globalThis.document } = {}) {
    doc = documentRef;
    state.onChange(() => render());
    render();
}
