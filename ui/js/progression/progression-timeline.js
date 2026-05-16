// Timeline modal - grid rendering, drag/drop, duration calculation, quick fills.

import * as state from './progression-state.js';

const PATTERN_COLORS = ['#00BFFF', '#FFD700', '#DC143C', '#9B30FF'];
const ROW_BG_COLORS = [
    'rgba(0,191,255,0.12)',
    'rgba(255,215,0,0.12)',
    'rgba(220,20,60,0.12)',
    'rgba(155,48,255,0.12)',
];
const CELL_WIDTH = 48;
const CELL_HEIGHT = 36;

const grid = document.getElementById('timeline-grid');
const loopsInput = document.getElementById('timeline-loops');
const durationDisplay = document.getElementById('timeline-duration');

// Drag state - dragSource.row identifies the source cell's pattern row so the
// drop handler can clear the original slot even when dropped onto a different
// row than the one it came from.
let dragSource = null; // { col, row }

/** Initialize timeline event handlers. */
export function init() {
    loopsInput.addEventListener('change', () => {
        const n = parseInt(loopsInput.value) || 16;
        state.setTimelineLength(Math.max(1, Math.min(128, n)));
        render();
    });

    // Mouse-wheel nudges the loops count by 1 (or 4 with Shift) within 1..128.
    loopsInput.addEventListener('wheel', (e) => {
        e.preventDefault();
        const cur = parseInt(loopsInput.value) || 16;
        const step = e.shiftKey ? 4 : 1;
        const delta = e.deltaY < 0 ? step : -step;
        const next = Math.max(1, Math.min(128, cur + delta));
        if (next === cur) return;
        loopsInput.value = String(next);
        state.setTimelineLength(next);
        render();
    }, { passive: false });

    // Fill menu - delegated click on the dropdown panel's [data-fill] buttons.
    // RANDOMIZE and FILL ALL P1..P4 replace every slot; FILL PROGRESSION
    // applies the 1111 2222 3333 4444 pattern.
    document.querySelectorAll('[data-fill]').forEach(btn => {
        btn.addEventListener('click', () => {
            const kind = btn.dataset.fill;
            if (kind === 'random') fillRandom();
            else if (kind === 'prog') fillProgression();
            else if (kind === 'p1') fillAll(1);
            else if (kind === 'p2') fillAll(2);
            else if (kind === 'p3') fillAll(3);
            else if (kind === 'p4') fillAll(4);
        });
    });

    document.getElementById('tl-clear').addEventListener('click', () => {
        fillAll(0);
    });
}

/** Render the full timeline grid. */
export function render() {
    const timeline = state.getTimeline();
    const cols = timeline.length;
    loopsInput.value = cols;
    updateDuration();

    grid.innerHTML = '';

    // Build table: header row + 4 pattern rows
    const table = document.createElement('div');
    table.style.display = 'grid';
    table.style.gridTemplateColumns = `56px repeat(${cols}, ${CELL_WIDTH}px)`;
    table.style.gridTemplateRows = `28px repeat(4, ${CELL_HEIGHT}px)`;
    table.style.gap = '1px';

    // Header row
    const corner = document.createElement('div');
    corner.className = 'flex items-center justify-center text-[0.65rem] font-black text-on-surface-variant';
    table.appendChild(corner);

    for (let c = 0; c < cols; c++) {
        const hdr = document.createElement('div');
        hdr.className = 'flex items-center justify-center text-[0.65rem] font-mono text-on-surface-variant opacity-50';
        hdr.textContent = c + 1;
        table.appendChild(hdr);
    }

    // Pattern rows
    for (let row = 0; row < 4; row++) {
        // Row label
        const label = document.createElement('div');
        label.className = 'flex items-center justify-center text-sm font-black';
        label.style.color = PATTERN_COLORS[row];
        label.style.background = ROW_BG_COLORS[row];
        label.style.borderRadius = '4px 0 0 4px';
        label.textContent = `P${row + 1}`;
        table.appendChild(label);

        // Cells
        for (let c = 0; c < cols; c++) {
            const cell = document.createElement('div');
            cell.className = 'flex items-center justify-center rounded-sm cursor-pointer';
            cell.style.background = ROW_BG_COLORS[row];
            cell.style.minHeight = CELL_HEIGHT + 'px';
            cell.dataset.row = row;
            cell.dataset.col = c;

            const patIdx = timeline[c]; // 1-4 or 0
            if (patIdx === row + 1) {
                // This cell has a brick
                const brick = createBrick(row, c);
                cell.appendChild(brick);
            }

            // Drop target
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
                const targetCol = parseInt(cell.dataset.col);
                const targetRow = parseInt(cell.dataset.row);
                handleDrop(dragSource.col, targetCol, targetRow);
                dragSource = null;
            });

            // Click empty cell to place the row's pattern
            cell.addEventListener('click', (e) => {
                if (e.target !== cell) return; // don't trigger on brick click
                const tl = [...state.getTimeline()];
                tl[c] = row + 1;
                state.setTimeline(tl);
                render();
            });

            table.appendChild(cell);
        }
    }

    grid.appendChild(table);
}

function createBrick(patternRow, col) {
    const brick = document.createElement('div');
    brick.className = 'rounded-sm cursor-grab active:cursor-grabbing';
    brick.style.width = (CELL_WIDTH - 6) + 'px';
    brick.style.height = (CELL_HEIGHT - 6) + 'px';
    brick.style.background = PATTERN_COLORS[patternRow];
    brick.style.boxShadow = `0 2px 4px rgba(0,0,0,0.4), inset 0 1px 0 rgba(255,255,255,0.2)`;
    brick.style.borderRadius = '4px';
    brick.draggable = true;

    brick.addEventListener('dragstart', (e) => {
        dragSource = { col, row: patternRow };
        e.dataTransfer.effectAllowed = 'move';
        e.dataTransfer.setData('text/plain', col.toString());
        brick.style.opacity = '0.5';
    });
    brick.addEventListener('dragend', () => {
        brick.style.opacity = '1';
        dragSource = null;
    });

    return brick;
}

// Drop semantics: move the dragged brick to (targetCol, targetRow). The source
// slot is cleared and the target slot adopts the target row's pattern number
// (targetRow + 1), so dragging across rows re-assigns the pattern rather than
// swapping within a single row as before.
function handleDrop(sourceCol, targetCol, targetRow) {
    const targetPat = targetRow + 1;
    const tl = [...state.getTimeline()];
    if (sourceCol === targetCol) {
        // Same column, possibly different row: re-home the slot onto the
        // target row. No-op if already on that row.
        if (tl[targetCol] === targetPat) return;
        tl[targetCol] = targetPat;
    } else {
        tl[targetCol] = targetPat;
        tl[sourceCol] = 0;
    }
    state.setTimeline(tl);
    render();
}

// --- Quick fills ---

function fillAll(patIdx) {
    const tl = state.getTimeline();
    const newTl = tl.map(() => patIdx);
    state.setTimeline(newTl);
    render();
}

function fillRandom() {
    const tl = state.getTimeline();
    const newTl = tl.map(() => 1 + Math.floor(Math.random() * 4));
    state.setTimeline(newTl);
    render();
}

function fillProgression() {
    const tl = state.getTimeline();
    const prog = [1,1,1,1,2,2,2,2,3,3,3,3,4,4,4,4];
    const newTl = tl.map((_, i) => prog[i % prog.length]);
    state.setTimeline(newTl);
    render();
}

// --- Duration calculation ---

function updateDuration() {
    const bpm = state.getBpm();
    const activeSteps = state.getActiveSteps(0);
    const triplet = state.getTriplet(0);
    const loops = state.getTimeline().length;

    const stepsPerBeat = triplet ? 3 : 4;
    const beatsPerPattern = activeSteps / stepsPerBeat;
    const secondsPerBeat = 60 / bpm;
    const totalSeconds = beatsPerPattern * secondsPerBeat * loops;

    const mins = Math.floor(totalSeconds / 60);
    const secs = Math.floor(totalSeconds % 60);
    durationDisplay.textContent = `${String(mins).padStart(2, '0')}:${String(secs).padStart(2, '0')} @ ${bpm} BPM`;
}

/** Highlight the current playing column (called from transport). */
export function highlightColumn(col) {
    // Remove previous highlights
    const prev = grid.querySelectorAll('.tl-playing');
    prev.forEach(el => {
        el.classList.remove('tl-playing');
        el.style.outline = '';
    });
    if (col < 0) return;
    // Add highlight
    const cells = grid.querySelectorAll(`[data-col="${col}"]`);
    cells.forEach(el => {
        el.classList.add('tl-playing');
        el.style.outline = '2px solid #4e8c45';
    });
    // Auto-scroll to keep the column visible
    const scrollContainer = document.getElementById('timeline-scroll');
    const targetX = 56 + col * (CELL_WIDTH + 1) - scrollContainer.clientWidth / 2;
    scrollContainer.scrollTo({ left: Math.max(0, targetX), behavior: 'smooth' });
}
