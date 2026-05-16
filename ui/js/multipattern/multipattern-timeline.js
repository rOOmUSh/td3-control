// Main-page multi-pattern timeline modal - dynamic-N grid renderer + event
// wiring. Mirrors the chrome of progression-timeline.js but supports 1..64
// pattern rows with golden-angle HSL coloring and a shorter row when the
// row count exceeds 16 (so every row still fits in the viewport without
// scrolling vertically).
//
// Read-only with respect to pattern content - the grid only edits the
// `timeline` array on the multipattern state (which 1-based pattern number
// plays at each column). Pattern editing happens on the card list below.
//
// Drag-and-drop semantics (mirror progression):
//   - A `brick` in row R, column C means timeline[C] = R+1.
//   - Drop onto an empty cell in row R' at column C' → timeline[C'] = R'+1,
//     timeline[C] (the source) cleared to 0.
//   - Drop onto the same column but a different row → re-home, no second
//     slot to clear.

import * as state from './multipattern-state.js';
import {
    countNonEmpty,
    repeatFill,
    randomFill,
    hslForIndex,
} from './multipattern-transport-helpers.js';

const CELL_WIDTH = 36;
const CELL_HEIGHT_REGULAR = 28;
const CELL_HEIGHT_COMPACT = 20;   // row height once N > 16
const COMPACT_THRESHOLD = 16;
const LABEL_WIDTH = 44;

let grid = null;
let loopsInput = null;
let durationDisplay = null;
let modePill = null;
let scrollContainer = null;
let modal = null;
let closeBtn = null;
let openBtn = null;

// Drag state - { col, row } or null. Row is the *source* pattern row so the
// drop handler can clear that slot even when the drop lands on a new row.
let dragSource = null;

// Subscription cleanup: bound listeners held so re-init is idempotent.
let stateUnsub = false;

// ---------------------------------------------------------------------------
// Init / teardown
// ---------------------------------------------------------------------------

export function init() {
    modal = document.getElementById('mp-timeline-modal');
    grid = document.getElementById('mp-timeline-grid');
    loopsInput = document.getElementById('mp-timeline-loops');
    durationDisplay = document.getElementById('mp-timeline-duration');
    modePill = document.getElementById('mp-timeline-mode-pill');
    scrollContainer = document.getElementById('mp-timeline-scroll');
    closeBtn = document.getElementById('btn-mp-timeline-close');
    openBtn = document.getElementById('btn-timeline');

    if (!modal || !grid || !loopsInput || !durationDisplay) {
        console.warn('[multipattern-timeline] modal DOM not found - skipping init');
        return;
    }

    // LOOPS input: change + scroll-wheel nudge (1 step, Shift = 4 steps).
    loopsInput.addEventListener('change', () => {
        const n = parseInt(loopsInput.value, 10) || 16;
        setTimelineLength(Math.max(1, Math.min(128, n)));
    });
    loopsInput.addEventListener('wheel', (e) => {
        e.preventDefault();
        const cur = parseInt(loopsInput.value, 10) || 16;
        const step = e.shiftKey ? 4 : 1;
        const next = Math.max(1, Math.min(128, cur + (e.deltaY < 0 ? step : -step)));
        if (next === cur) return;
        loopsInput.value = String(next);
        setTimelineLength(next);
    }, { passive: false });

    // FILL menu (sequence / checked / focused / random) + CLEAR.
    const panel = modal.querySelector('[data-mp-fill]')?.closest('div');
    modal.querySelectorAll('[data-mp-fill]').forEach(btn => {
        btn.addEventListener('click', () => handleFill(btn.dataset.mpFill));
    });
    const clearBtn = document.getElementById('mp-tl-clear');
    if (clearBtn) clearBtn.addEventListener('click', () => clearTimeline());

    // Open / close wiring. TIMELINE button in the toolbar opens the modal;
    // close button + backdrop click + Esc all close it.
    if (openBtn) openBtn.addEventListener('click', open);
    if (closeBtn) closeBtn.addEventListener('click', close);
    const backdrop = document.getElementById('mp-timeline-backdrop');
    if (backdrop) backdrop.addEventListener('click', close);
    document.addEventListener('keydown', (e) => {
        if (e.key === 'Escape' && isOpen()) close();
    });

    // Re-render whenever state changes structurally - new patterns added,
    // focus moved, timeline edited, etc.
    if (!stateUnsub) {
        state.onChange(() => { if (isOpen()) render(); });
        stateUnsub = true;
    }
}

export function isOpen() {
    return modal && !modal.classList.contains('hidden');
}

export function open() {
    if (!modal) return;
    modal.classList.remove('hidden');
    render();
}

export function close() {
    if (!modal) return;
    modal.classList.add('hidden');
    highlightColumn(-1);
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

export function render() {
    if (!grid) return;
    const timeline = state.getTimeline();
    const cols = timeline.length;
    const rows = state.getPatternCount();
    loopsInput.value = cols;
    updateDuration();
    updateModePill();

    grid.innerHTML = '';

    // Row height compresses once N > 16 so the whole grid fits vertically
    // in the modal without a scrollbar - the user can still scroll the
    // modal body if their viewport is very short, but the grid itself stays
    // visually coherent.
    const cellHeight = rows > COMPACT_THRESHOLD ? CELL_HEIGHT_COMPACT : CELL_HEIGHT_REGULAR;

    const table = document.createElement('div');
    table.style.display = 'grid';
    table.style.gridTemplateColumns = `${LABEL_WIDTH}px repeat(${cols}, ${CELL_WIDTH}px)`;
    table.style.gridTemplateRows = `24px repeat(${rows}, ${cellHeight}px)`;
    table.style.gap = '1px';

    // Header row (corner + column numbers).
    const corner = document.createElement('div');
    corner.className = 'flex items-center justify-center text-[0.65rem] font-black text-on-surface-variant';
    table.appendChild(corner);
    for (let c = 0; c < cols; c += 1) {
        const hdr = document.createElement('div');
        hdr.className = 'flex items-center justify-center text-[0.65rem] font-mono text-on-surface-variant opacity-50';
        hdr.textContent = c + 1;
        table.appendChild(hdr);
    }

    // Pattern rows. Each row gets a golden-angle hue so duplicates across the
    // palette are unlikely even past 32 patterns.
    for (let row = 0; row < rows; row += 1) {
        const hue = hslForIndex(row, 70, 55);
        const rowBg = hslForIndex(row, 70, 20) + '20'; // hue + translucent alpha

        const label = document.createElement('div');
        label.className = 'flex items-center justify-center font-black';
        label.style.color = hue;
        label.style.background = hslForIndex(row, 70, 12);
        label.style.borderRadius = '4px 0 0 4px';
        label.style.fontSize = rows > COMPACT_THRESHOLD ? '0.65rem' : '0.75rem';
        label.textContent = `P${row + 1}`;
        table.appendChild(label);

        for (let c = 0; c < cols; c += 1) {
            const cell = document.createElement('div');
            cell.className = 'flex items-center justify-center rounded-sm cursor-pointer';
            cell.style.background = hslForIndex(row, 40, 15);
            cell.style.minHeight = cellHeight + 'px';
            cell.dataset.row = String(row);
            cell.dataset.col = String(c);

            if (timeline[c] === row + 1) {
                cell.appendChild(createBrick(row, c, cellHeight, hue));
            }

            // Drop target - same outline-green visual as progression.
            cell.addEventListener('dragover', (e) => {
                e.preventDefault();
                e.dataTransfer.dropEffect = 'move';
                cell.style.outline = '2px solid #4e8c45';
            });
            cell.addEventListener('dragleave', () => {
                cell.style.outline = '';
            });
            cell.addEventListener('drop', (e) => {
                e.preventDefault();
                cell.style.outline = '';
                if (dragSource === null) return;
                const targetCol = parseInt(cell.dataset.col, 10);
                const targetRow = parseInt(cell.dataset.row, 10);
                handleDrop(dragSource.col, targetCol, targetRow);
                dragSource = null;
            });

            // Click an empty cell → place that row's pattern. Same pattern
            // number on click of a populated cell is a no-op.
            cell.addEventListener('click', (e) => {
                if (e.target !== cell) return;
                const tl = [...state.getTimeline()];
                tl[c] = row + 1;
                state.setTimeline(tl);
            });

            table.appendChild(cell);
        }
    }

    grid.appendChild(table);
}

function createBrick(row, col, cellHeight, color) {
    const brick = document.createElement('div');
    brick.className = 'rounded-sm cursor-grab active:cursor-grabbing';
    brick.style.width = (CELL_WIDTH - 4) + 'px';
    brick.style.height = (cellHeight - 4) + 'px';
    brick.style.background = color;
    brick.style.boxShadow = '0 2px 4px rgba(0,0,0,0.4), inset 0 1px 0 rgba(255,255,255,0.2)';
    brick.style.borderRadius = '4px';
    brick.draggable = true;

    brick.addEventListener('dragstart', (e) => {
        dragSource = { col, row };
        e.dataTransfer.effectAllowed = 'move';
        e.dataTransfer.setData('text/plain', String(col));
        brick.style.opacity = '0.5';
    });
    brick.addEventListener('dragend', () => {
        brick.style.opacity = '1';
        dragSource = null;
    });

    // Double-click clears the brick (quick way to punch a hole).
    brick.addEventListener('dblclick', (e) => {
        e.stopPropagation();
        const tl = [...state.getTimeline()];
        tl[col] = 0;
        state.setTimeline(tl);
    });

    return brick;
}

function handleDrop(sourceCol, targetCol, targetRow) {
    const targetPat = targetRow + 1;
    const tl = [...state.getTimeline()];
    if (sourceCol === targetCol) {
        if (tl[targetCol] === targetPat) return;
        tl[targetCol] = targetPat;
    } else {
        tl[targetCol] = targetPat;
        tl[sourceCol] = 0;
    }
    state.setTimeline(tl);
}

// ---------------------------------------------------------------------------
// Timeline length + fills
// ---------------------------------------------------------------------------

function setTimelineLength(n) {
    const cur = state.getTimeline();
    let next;
    if (n > cur.length) {
        next = cur.slice();
        while (next.length < n) next.push(0);
    } else {
        next = cur.slice(0, n);
    }
    state.setTimeline(next);
}

function handleFill(kind) {
    const cur = state.getTimeline();
    const len = cur.length;
    const n = state.getPatternCount();

    if (kind === 'sequence') {
        state.setTimeline(repeatFill(
            Array.from({ length: n }, (_, i) => i + 1),
            len,
        ));
    } else if (kind === 'checked') {
        const checked = state.getCheckedArray();
        if (checked.length === 0) return; // no-op when nothing is checked
        state.setTimeline(repeatFill(checked.map(i => i + 1), len));
    } else if (kind === 'focused') {
        const f = state.getFocusedIdx();
        if (f === null) return;
        state.setTimeline(cur.map(() => f + 1));
    } else if (kind === 'random') {
        state.setTimeline(randomFill(n, len));
    }
}

function clearTimeline() {
    const cur = state.getTimeline();
    state.setTimeline(cur.map(() => 0));
}

// ---------------------------------------------------------------------------
// Duration / playback highlight
// ---------------------------------------------------------------------------

function updateModePill() {
    if (!modePill) return;
    modePill.classList.toggle('hidden', !state.isCheckedMode());
}

function updateDuration() {
    const bpm = state.getBpm();
    // Duration uses the focused pattern's cadence - most timelines are
    // homogeneous in step count. Close enough for a display hint.
    const activeSteps = state.getActiveSteps();
    const triplet = state.getTriplet();
    const loops = countNonEmpty(state.getTimeline());

    const stepsPerBeat = triplet ? 3 : 4;
    const beatsPerPattern = activeSteps / stepsPerBeat;
    const secondsPerBeat = 60 / bpm;
    const totalSeconds = beatsPerPattern * secondsPerBeat * loops;

    const mins = Math.floor(totalSeconds / 60);
    const secs = Math.floor(totalSeconds % 60);
    durationDisplay.textContent = `${String(mins).padStart(2, '0')}:${String(secs).padStart(2, '0')} @ ${bpm} BPM`;
}

/** Highlight the playing column (called from the transport loop). */
export function highlightColumn(col) {
    if (!grid) return;
    grid.querySelectorAll('.mp-tl-playing').forEach(el => {
        el.classList.remove('mp-tl-playing');
        el.style.outline = '';
    });
    if (col < 0) return;
    const cells = grid.querySelectorAll(`[data-col="${col}"]`);
    cells.forEach(el => {
        el.classList.add('mp-tl-playing');
        el.style.outline = '2px solid #4e8c45';
    });
    // Keep the playing column in view when the modal is open.
    if (scrollContainer && isOpen()) {
        const targetX = LABEL_WIDTH + col * (CELL_WIDTH + 1) - scrollContainer.clientWidth / 2;
        scrollContainer.scrollTo({ left: Math.max(0, targetX), behavior: 'smooth' });
    }
}
