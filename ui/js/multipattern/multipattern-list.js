// Orchestrates the `#multipattern-list` container on the main Control page.
//
// Subscribes to multipattern-state `onChange` and rebuilds the card list
// whenever the pattern array changes structurally OR the focused index /
// checked set / per-step content changes. For N≤64 this is cheap and keeps
// the DOM a pure function of state.
//
// One delegated click listener on the container handles every per-card
// button - PREVIEW, SHIFT, TRNSPS, RAND, COPY, PASTE, DEL - so row renders
// stay DOM-only and the handlers live in one place.
//
// Also exposes `highlightStep(stepIdx)` so the transport's beat timer can
// light up the focused card's playing step without the old single-grid DOM.

import * as state from './multipattern-state.js';
import { renderCard } from './multipattern-row.js';
import { applyStepHighlight, restoreStepHighlight } from '../step-highlight.js';
import { slotFor } from '../shared/slot-targets.js';
import * as clipboard from '../progression/progression-clipboard.js';
import * as preview from './multipattern-preview.js';
import * as randomize from '../randomize.js';
import { formatPatternAsStepsTxt } from '../shared/steps-txt-format.js';
import { parseStepsTxt, looksLikeStepsTxt } from '../shared/steps-txt-parse.js';

let container = null;
let setStatus = () => {};
let onBankPattern = null;
// Wired from main.js; invoked after movePattern succeeds during playback
// so the device scratch slot reflects the new next-in-timeline pattern.
// Kept as an injected callback to avoid a transport <-> list circular import.
let onStructuralChange = () => {};

/** Wire the list to its DOM container + state. Call once after layout boot. */
export function init(opts = {}) {
    if (typeof opts.setStatus === 'function') setStatus = opts.setStatus;
    if (typeof opts.onBankPattern === 'function') onBankPattern = opts.onBankPattern;
    if (typeof opts.onStructuralChange === 'function') onStructuralChange = opts.onStructuralChange;
    container = document.getElementById('multipattern-list');
    if (!container) {
        console.warn('[multipattern-list] #multipattern-list not found');
        return;
    }
    container.addEventListener('click', handleAction);
    container.addEventListener('dragstart', handleDragStart);
    container.addEventListener('dragend',   handleDragEnd);
    container.addEventListener('dragenter', handleDragOver); // same handler; both must preventDefault
    container.addEventListener('dragover',  handleDragOver);
    container.addEventListener('dragleave', handleDragLeave);
    container.addEventListener('drop',      handleDrop);
    render();
    state.onChange(() => render());
    clipboard.subscribe(() => render());
    preview.subscribe(() => render());
}

/** Rebuild the card list from current state. */
export function render() {
    if (!container) return;
    const scrollHost = getScrollHost();
    const scrollTop = scrollHost ? scrollHost.scrollTop : 0;
    const count = state.getPatternCount();
    const vp = state.getViewport();
    const mode = state.getAbMode();
    const scratch = state.getScratchSlot();
    const startSlot = state.getSelectedSlot();
    const frag = document.createDocumentFragment();
    for (let i = 0; i < count; i++) {
        const card = renderCard(i);
        if (!matchesViewport(i, vp, scratch, mode, startSlot)) {
            card.classList.add('mp-card-hidden');
            card.style.display = 'none';
        }
        frag.appendChild(card);
    }
    container.replaceChildren(frag);
    restoreScroll(scrollHost, scrollTop);
}

function getScrollHost() {
    if (!container) return null;
    const host = container.parentElement;
    if (!host || typeof host.scrollTop !== 'number') return null;
    return host;
}

function restoreScroll(scrollHost, scrollTop) {
    if (!scrollHost) return;
    scrollHost.scrollTop = scrollTop;
    const raf = typeof requestAnimationFrame === 'function'
        ? requestAnimationFrame
        : (fn) => setTimeout(fn, 0);
    raf(() => {
        scrollHost.scrollTop = scrollTop;
    });
}

/**
 * True if the pattern at `idx` should be visible under the active viewport
 * filter. ALL → always true. G{N}{A|B} → card's computed slot must match
 * that group + side. The overflow (null slot, idx=63 with scratch present)
 * only shows under ALL.
 */
function matchesViewport(idx, vp, scratch, mode, startSlot) {
    if (!vp || vp.group === 'ALL') return true;
    const slot = slotFor(idx, scratch, mode, startSlot);
    if (!slot) return false; // overflow - hidden under filtered viewports
    return String(slot.group) === String(vp.group) && slot.side === vp.side;
}

/**
 * Highlight the given step index (0..15) on the card whose pattern is
 * currently playing. `patternIdx` is the 0-based pattern index; pass
 * null/undefined to fall back to the focused card (single-pattern mode).
 * Passing stepIndex < 0 clears the highlight.
 */
export function highlightStep(stepIndex, patternIdx) {
    if (!container) return;
    const prev = container.querySelector('.step-active');
    if (prev) restoreStepHighlight(prev);
    if (stepIndex < 0) return;
    const idx = (patternIdx === null || patternIdx === undefined)
        ? state.getFocusedIdx()
        : patternIdx;
    if (idx === null) return;
    const card = container.querySelector(
        `.mp-card[data-pattern-idx="${idx}"] .mp-card-grid [data-step="${stepIndex}"]`,
    );
    if (!card) return;
    applyStepHighlight(card);
}

// ---------------------------------------------------------------------------
// Per-card action dispatch
// ---------------------------------------------------------------------------

function handleAction(e) {
    const btn = e.target.closest('[data-action]');
    if (!btn) return;
    // Checkbox input handles its own change event in multipattern-row.js -
    // don't double-fire from the click.
    if (btn.tagName === 'INPUT') return;
    const action = btn.dataset.action;
    const idx = parseInt(btn.dataset.patternIdx, 10);
    if (!Number.isInteger(idx) || idx < 0) return;
    e.stopPropagation();

    // Every per-card control (preview, shift, transpose, rand-*, copy,
    // paste, del) implicitly selects its pattern - the card-background
    // focus handler bails on data-action clicks so we hoist it here.
    if (state.getFocusedIdx() !== idx) state.setFocused(idx);

    switch (action) {
        case 'delete':       return handleDelete(idx);
        case 'bank':         return handleBank(idx);
        case 'preview':      return preview.toggle(idx);
        case 'triplet':      return handleTriplet(idx);
        case 'shift':        return handleShift(idx, btn);
        case 'shuffle':      return handleShuffle(idx);
        case 'transpose':    return handleTranspose(idx, btn);
        case 'rand-rst':     return handleRand(idx, 'rst');
        case 'rand-sl':      return handleRand(idx, 'sl');
        case 'rand-ac':      return handleRand(idx, 'ac');
        case 'rand-ud':      return handleRand(idx, 'ud');
        case 'copy':         return handleCopy(idx, btn);
        case 'paste':        return handlePaste(idx, btn);
        default:             return;
    }
}

function handleTriplet(idx) {
    const next = !state.getTriplet(idx);
    state.setTriplet(idx, next);
    setStatus(`P${idx + 1} triplet ${next ? 'ON' : 'OFF'}`);
}

function handleBank(idx) {
    if (typeof onBankPattern !== 'function') {
        setStatus('Bank save unavailable');
        return;
    }
    onBankPattern(idx);
}

function handleDelete(idx) {
    const wasOnly = state.getPatternCount() <= 1;
    // Focus was already set by the dispatcher so undo snapshots record
    // the intended card and the post-delete focus lands on the sibling
    // (or the reset sole pattern, when we hit the N≥1 floor).
    state.deletePattern(idx);
    setStatus(wasOnly ? `Reset P${idx + 1} (N ≥ 1)` : `Deleted P${idx + 1}`);
}

function handleShift(idx, btn) {
    const n = parseInt(btn.dataset.shift, 10);
    if (!Number.isFinite(n) || n === 0) return;
    state.shiftSteps(idx, n);
    setStatus(`P${idx + 1} shifted ${n > 0 ? '+' : ''}${n}`);
}

function handleShuffle(idx) {
    state.shuffleSteps(idx);
    setStatus(`P${idx + 1} steps shuffled`);
}

function handleTranspose(idx, btn) {
    const delta = parseInt(btn.dataset.delta, 10);
    if (!Number.isFinite(delta) || delta === 0) return;
    state.transposePattern(idx, delta);
    setStatus(`P${idx + 1} transposed ${delta > 0 ? '+' : ''}${delta}`);
}

function handleRand(idx, kind) {
    randomize.randomizeCategoryForPattern(idx, kind);
    setStatus(`P${idx + 1} ${kind.toUpperCase()} randomized`);
}

function handleCopy(idx, btn) {
    const kind = btn.dataset.kind;
    if (!kind) return;
    if (kind === 'full') {
        if (state.copyFocused()) {
            setStatus(`P${idx + 1} → clipboard (FULL)`);
            // Best-effort system clipboard write so the user can paste the
            // pattern into Notepad / WhatsApp / chat etc. in .steps.txt
            // form. In-memory FULL clipboard stays authoritative for
            // PASTE FULL; this write is independent and never blocks it.
            writeFocusedPatternToSystemClipboard(idx);
        }
        return;
    }
    if (kind === 'rest' || kind === 'slide' || kind === 'accent') {
        clipboard.copy(kind, state.getPattern(idx));
        setStatus(`P${idx + 1} ${kind} → clipboard`);
    }
}

async function writeFocusedPatternToSystemClipboard(idx) {
    try {
        if (!navigator.clipboard || !navigator.clipboard.writeText) return;
        const pat = state.getPattern(idx);
        if (!pat) return;
        await navigator.clipboard.writeText(formatPatternAsStepsTxt(pat));
    } catch (_) { /* best-effort */ }
}

// ---------------------------------------------------------------------------
// Drag-to-reorder
// ---------------------------------------------------------------------------
//
// Vertical drag reordering on the main Control page. Only the dot-grid
// handle at the bottom of each card's id column starts a drag. Drop target
// is any card; inserting above vs. below the hovered card is decided by
// pointer Y relative to the card's vertical midline. On drop the pattern
// array is spliced to the new position (see `state.movePattern`) - focus
// and checked marks follow the moved pattern, and timeline numbers stay
// as-is so the new card order becomes the new default playback order.

let dragSrcIdx = null;

function handleDragStart(e) {
    const handle = e.target.closest('.mp-drag-handle');
    if (!handle) { e.preventDefault(); return; }
    const idx = parseInt(handle.dataset.patternIdx, 10);
    if (!Number.isInteger(idx)) { e.preventDefault(); return; }
    const card = handle.closest('.mp-card');
    dragSrcIdx = idx;
    if (card) card.classList.add('dragging');
    if (e.dataTransfer) {
        e.dataTransfer.effectAllowed = 'move';
        // Some browsers require a payload for the drag to fire on every
        // element; the value is unused on drop (we read dragSrcIdx).
        try { e.dataTransfer.setData('text/plain', String(idx)); } catch (_) {}
        if (card) {
            try { e.dataTransfer.setDragImage(card, 12, 12); } catch (_) {}
        }
    }
}

function handleDragEnd(_e) {
    clearDropIndicators();
    if (container) {
        container.querySelectorAll('.mp-card.dragging').forEach((c) => c.classList.remove('dragging'));
    }
    dragSrcIdx = null;
}

// Find the visible card whose vertical band contains `clientY`, or - if the
// pointer sits in the gap between cards - the nearest one by Y. The source
// card is excluded: while dragging it stays in the DOM at its original slot
// and would otherwise swallow every pointer Y inside its own band, which
// made the drop zone feel like it only woke up past the midway point.
function findCardByY(clientY) {
    if (!container) return null;
    const cards = container.querySelectorAll('.mp-card:not(.mp-card-hidden):not(.dragging)');
    if (!cards.length) return null;
    let nearest = null;
    let nearestDist = Infinity;
    for (const card of cards) {
        const rect = card.getBoundingClientRect();
        if (clientY >= rect.top && clientY <= rect.bottom) return card;
        const dist = clientY < rect.top ? rect.top - clientY : clientY - rect.bottom;
        if (dist < nearestDist) { nearest = card; nearestDist = dist; }
    }
    return nearest;
}

function handleDragOver(e) {
    if (dragSrcIdx === null) return;
    // Always preventDefault during an active drag so the browser accepts
    // the drop anywhere on the container surface. Geometry decides which
    // card is the insertion anchor - no reliance on event.target.
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
    const card = findCardByY(e.clientY);
    clearDropIndicators();
    if (!card) return;
    const destIdx = parseInt(card.dataset.patternIdx, 10);
    if (!Number.isInteger(destIdx) || destIdx === dragSrcIdx) return;
    // "Drop on card X" = source takes X's current position. Direction is
    // implied by source vs target index: dragging downward lands below,
    // dragging upward lands above. The midline no longer gates anything.
    const below = destIdx > dragSrcIdx;
    card.classList.add(below ? 'drop-below' : 'drop-above');
}

function handleDragLeave(e) {
    // Only clear when leaving the container entirely - dragging across
    // cards keeps the indicator because the next card's dragover fires
    // before this dragleave bubbles up.
    if (!container) return;
    const related = e.relatedTarget;
    if (related && container.contains(related)) return;
    clearDropIndicators();
}

function handleDrop(e) {
    if (dragSrcIdx === null) return;
    e.preventDefault();
    const card = findCardByY(e.clientY);
    clearDropIndicators();
    if (!card) return;
    const destIdx = parseInt(card.dataset.patternIdx, 10);
    if (!Number.isInteger(destIdx) || destIdx === dragSrcIdx) return;
    // Drop anywhere on a non-source card = source takes that card's
    // position. movePattern's splice semantics handle the direction -
    // no midline math, no above/below insertion-point juggling.
    if (state.movePattern(dragSrcIdx, destIdx)) {
        setStatus(`Moved P${dragSrcIdx + 1} → position ${destIdx + 1}`);
        // During timeline playback the last pre-load already queued the
        // *old* next pattern into scratch. Re-send what the new ordering
        // says is next so the device wraps into the right buffer - the
        // current audio keeps looping until the active-steps wrap, same
        // as any other mid-cycle pattern change.
        onStructuralChange();
    }
}

function clearDropIndicators() {
    if (!container) return;
    container.querySelectorAll('.mp-card.drop-above, .mp-card.drop-below').forEach((c) => {
        c.classList.remove('drop-above', 'drop-below');
    });
}

function handlePaste(idx, btn) {
    const kind = btn.dataset.kind;
    if (!kind) return;
    if (kind === 'full') {
        // Try the OS clipboard first (paste from Notepad / chat). If it
        // doesn't hold a valid .steps.txt, fall back to the in-memory FULL
        // buffer the same way the old path did.
        tryPasteFullFromSystemClipboard(idx).then((consumed) => {
            if (consumed) return;
            if (!state.hasClipboard()) {
                setStatus('FULL clipboard empty (and OS clipboard has no .steps.txt)');
                return;
            }
            if (state.pasteIntoFocused()) setStatus(`FULL → P${idx + 1}`);
        });
        return;
    }
    if (kind === 'rest' || kind === 'slide' || kind === 'accent') {
        if (!clipboard.has(kind)) { setStatus(`${kind.toUpperCase()} clipboard empty`); return; }
        const pat = state.getPattern(idx);
        if (!pat) return;
        if (clipboard.paste(kind, pat)) {
            // setPattern re-notifies so live-send fires and history records.
            state.setPattern(idx, pat);
            setStatus(`${kind} → P${idx + 1}`);
        }
    }
}

async function tryPasteFullFromSystemClipboard(idx) {
    try {
        if (!navigator.clipboard || !navigator.clipboard.readText) return false;
        const text = await navigator.clipboard.readText();
        if (!looksLikeStepsTxt(text)) return false;
        const pat = parseStepsTxt(text);
        state.setPattern(idx, pat);
        setStatus(`FULL → P${idx + 1} (from text)`);
        return true;
    } catch (_) {
        return false;
    }
}
