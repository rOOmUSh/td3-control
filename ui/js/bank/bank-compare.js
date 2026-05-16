// Pattern comparison views for Bank Management.
//
// Public API:
//   - openItemCompare(aId, bId)      → fetch /api/bank/compare/items and
//                                      render a step-by-step diff modal.
//   - openSnapshotCompare(srcId, dstId) → fetch /api/bank/compare/snapshots
//                                         and render a 64-slot diff modal.
//
// Deliberately standalone: no state mutation, no re-render subscriptions.
// Both entry points handle errors via toast so callers can fire-and-forget.

import { bankApi } from './bank-api.js';
import { toast } from './bank-toast.js';
import { state } from './bank-state.js';
import { bankButton } from './bank-buttons.js';

// ---------------------------------------------------------------------------
// Item compare
// ---------------------------------------------------------------------------

export async function openItemCompare(aId, bId) {
    if (!aId || !bId) {
        toast('Select two items to compare', 'error');
        return;
    }
    if (aId === bId) {
        toast('Cannot compare an item with itself', 'error');
        return;
    }
    let report;
    try {
        const res = await bankApi.compareItems(aId, bId);
        report = res.report;
    } catch (e) {
        toast(`Compare failed: ${e.message}`, 'error');
        return;
    }
    if (!report) {
        toast('Compare returned no report', 'error');
        return;
    }
    renderItemCompareModal(aId, bId, report);
}

function renderItemCompareModal(aId, bId, report) {
    const backdrop = document.createElement('div');
    backdrop.className = 'bank-modal-backdrop';
    const modal = document.createElement('div');
    modal.className = 'bank-modal bank-compare-modal';

    const h = document.createElement('h3');
    h.textContent = 'Pattern Compare';
    modal.appendChild(h);

    const ids = document.createElement('div');
    ids.className = 'compare-ids';
    ids.textContent = `A: ${labelForItem(aId)}    vs    B: ${labelForItem(bId)}`;
    modal.appendChild(ids);

    // Summary row + score badges.
    const summary = document.createElement('div');
    summary.className = 'compare-summary';
    summary.appendChild(scoreBadge('Duplicate', report.duplicate_score));
    summary.appendChild(scoreBadge('Relatedness', report.relatedness_score));
    summary.appendChild(flagChip('Identical', !!report.identical, 'Bytes differ'));
    summary.appendChild(flagChip('Same rhythm', !!report.same_rhythm, 'Rhythm differs'));
    modal.appendChild(summary);

    // Counters block.
    const counters = document.createElement('div');
    counters.className = 'compare-counters';
    counters.appendChild(counterCell('Note diffs',     report.note_diff));
    counters.appendChild(counterCell('Accent diffs',   report.accent_diff));
    counters.appendChild(counterCell('Slide diffs',    report.slide_diff));
    counters.appendChild(counterCell('Transpose diffs', report.transpose_diff));
    counters.appendChild(counterCell('Time diffs',     report.time_diff));
    counters.appendChild(counterCell('Active steps',   report.active_steps_diff ? 'changed' : 'same'));
    counters.appendChild(counterCell('Triplet',        report.triplet_diff ? 'changed' : 'same'));
    modal.appendChild(counters);

    // 16-step diff grid.
    const gridLabel = document.createElement('div');
    gridLabel.className = 'compare-section-label';
    gridLabel.textContent = 'PER-STEP DIFF (1 … 16)';
    modal.appendChild(gridLabel);

    const grid = buildPerStepGrid(report.differ_steps || []);
    modal.appendChild(grid);

    if (report.summary) {
        const txt = document.createElement('p');
        txt.className = 'compare-summary-text';
        txt.textContent = report.summary;
        modal.appendChild(txt);
    }

    // Raw JSON expander.
    const details = document.createElement('details');
    details.className = 'compare-raw';
    const sum = document.createElement('summary');
    sum.textContent = 'Raw JSON';
    details.appendChild(sum);
    const pre = document.createElement('pre');
    pre.textContent = JSON.stringify(report, null, 2);
    details.appendChild(pre);
    modal.appendChild(details);

    const actions = document.createElement('div');
    actions.className = 'compare-actions';
    const closeBtn = bankButton({ label: 'CLOSE', onClick: () => backdrop.remove() });
    actions.appendChild(closeBtn);
    modal.appendChild(actions);

    backdrop.appendChild(modal);
    backdrop.addEventListener('click', (ev) => {
        if (ev.target === backdrop) backdrop.remove();
    });
    document.body.appendChild(backdrop);
}

function buildPerStepGrid(differSteps) {
    const set = new Set((differSteps || []).map((n) => Number(n)));
    const grid = document.createElement('div');
    grid.className = 'compare-grid';
    for (let i = 0; i < 16; i++) {
        const cell = document.createElement('div');
        cell.className = 'compare-cell';
        if (set.has(i)) cell.classList.add('compare-diff');
        else cell.classList.add('compare-same');
        const idx = document.createElement('span');
        idx.className = 'compare-cell-idx';
        idx.textContent = String(i + 1);
        cell.appendChild(idx);
        const badge = document.createElement('span');
        badge.className = 'compare-cell-badge';
        badge.textContent = set.has(i) ? 'DIFF' : 'ok';
        cell.appendChild(badge);
        grid.appendChild(cell);
    }
    return grid;
}

function scoreBadge(label, value) {
    const wrap = document.createElement('div');
    wrap.className = 'compare-score';
    const v = Number(value) || 0;
    const pct = Math.round(v * 100);
    const cls = pct >= 90 ? 'high' : pct >= 60 ? 'mid' : 'low';
    wrap.classList.add(cls);
    const l = document.createElement('span');
    l.className = 'compare-score-label';
    l.textContent = label;
    wrap.appendChild(l);
    const val = document.createElement('span');
    val.className = 'compare-score-value';
    val.textContent = `${pct}%`;
    wrap.appendChild(val);
    return wrap;
}

function flagChip(label, on, offLabel) {
    const chip = document.createElement('span');
    chip.className = `compare-flag ${on ? 'flag-on' : 'flag-off'}`;
    chip.textContent = on ? label : (offLabel || `not ${label.toLowerCase()}`);
    return chip;
}

function counterCell(label, value) {
    const cell = document.createElement('div');
    cell.className = 'compare-counter';
    const l = document.createElement('span');
    l.className = 'compare-counter-label';
    l.textContent = label;
    cell.appendChild(l);
    const v = document.createElement('span');
    v.className = 'compare-counter-value';
    v.textContent = value === undefined || value === null ? '-' : String(value);
    cell.appendChild(v);
    return cell;
}

function labelForItem(id) {
    const item = (state.items || []).find((i) => i.item_id === id);
    return item ? `${item.display_name} (${id})` : id;
}

// ---------------------------------------------------------------------------
// Snapshot compare
// ---------------------------------------------------------------------------

export async function openSnapshotCompare(srcId, dstId) {
    if (!srcId || !dstId) {
        toast('Need two snapshot IDs to compare', 'error');
        return;
    }
    if (srcId === dstId) {
        toast('Cannot compare a snapshot with itself', 'error');
        return;
    }
    let report;
    try {
        const res = await bankApi.compareSnapshots(srcId, dstId);
        report = res.report || res;
    } catch (e) {
        toast(`Snapshot compare failed: ${e.message}`, 'error');
        return;
    }
    if (!report) {
        toast('Snapshot compare returned no report', 'error');
        return;
    }
    renderSnapshotCompareModal(srcId, dstId, report);
}

function renderSnapshotCompareModal(srcId, dstId, report) {
    const backdrop = document.createElement('div');
    backdrop.className = 'bank-modal-backdrop';
    const modal = document.createElement('div');
    modal.className = 'bank-modal bank-compare-modal';

    const h = document.createElement('h3');
    h.textContent = 'Snapshot Compare';
    modal.appendChild(h);

    const ids = document.createElement('div');
    ids.className = 'compare-ids';
    ids.textContent =
        `SRC: ${labelForSnapshot(srcId)}    vs    DST: ${labelForSnapshot(dstId)}`;
    modal.appendChild(ids);

    const counts = countSlotStatuses(report);
    const summary = document.createElement('div');
    summary.className = 'compare-summary';
    summary.appendChild(counterCell('Identical', counts.identical));
    summary.appendChild(counterCell('Changed',   counts.changed));
    summary.appendChild(counterCell('Added',     counts.added));
    summary.appendChild(counterCell('Removed',   counts.removed));
    summary.appendChild(counterCell('Empty',     counts.empty));
    modal.appendChild(summary);

    const gridLabel = document.createElement('div');
    gridLabel.className = 'compare-section-label';
    gridLabel.textContent = 'SLOT-BY-SLOT (G1-P1A … G4-P8B)';
    modal.appendChild(gridLabel);

    modal.appendChild(buildSnapshotSlotGrid(report));

    const details = document.createElement('details');
    details.className = 'compare-raw';
    const sum = document.createElement('summary');
    sum.textContent = 'Raw JSON';
    details.appendChild(sum);
    const pre = document.createElement('pre');
    pre.textContent = JSON.stringify(report, null, 2);
    details.appendChild(pre);
    modal.appendChild(details);

    const actions = document.createElement('div');
    actions.className = 'compare-actions';
    const closeBtn = bankButton({ label: 'CLOSE', onClick: () => backdrop.remove() });
    actions.appendChild(closeBtn);
    modal.appendChild(actions);

    backdrop.appendChild(modal);
    backdrop.addEventListener('click', (ev) => {
        if (ev.target === backdrop) backdrop.remove();
    });
    document.body.appendChild(backdrop);
}

function countSlotStatuses(report) {
    const counts = { identical: 0, changed: 0, added: 0, removed: 0, empty: 0 };
    const slots = slotRowsOf(report);
    for (const row of slots) {
        const st = String(row.status || '').toLowerCase();
        if (counts[st] !== undefined) counts[st]++;
    }
    return counts;
}

function slotRowsOf(report) {
    if (Array.isArray(report?.slots)) return report.slots;
    if (Array.isArray(report?.rows))  return report.rows;
    if (Array.isArray(report))        return report;
    return [];
}

function buildSnapshotSlotGrid(report) {
    const wrap = document.createElement('div');
    wrap.className = 'compare-snapshot-grid';
    const rows = slotRowsOf(report);
    if (rows.length === 0) {
        const empty = document.createElement('div');
        empty.className = 'compare-empty';
        empty.textContent = 'Snapshot compare returned no slot rows.';
        wrap.appendChild(empty);
        return wrap;
    }
    for (const row of rows) {
        const cell = document.createElement('div');
        const status = String(row.status || 'unknown').toLowerCase();
        cell.className = `compare-slot status-${status}`;
        const key = document.createElement('div');
        key.className = 'compare-slot-key';
        key.textContent = row.slot_key || row.slot || '';
        cell.appendChild(key);
        const badge = document.createElement('div');
        badge.className = 'compare-slot-status';
        badge.textContent = status.toUpperCase();
        cell.appendChild(badge);
        if (row.reason) {
            const reason = document.createElement('div');
            reason.className = 'compare-slot-reason';
            reason.textContent = row.reason;
            cell.appendChild(reason);
        }
        wrap.appendChild(cell);
    }
    return wrap;
}

function labelForSnapshot(id) {
    const s = (state.snapshots || []).find((x) => x && x.snapshot_id === id);
    return s ? `${s.name} (${id})` : id;
}
