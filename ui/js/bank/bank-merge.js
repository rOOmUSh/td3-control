// Merge planning UI.
//
// Flow:
//   1. Source + target snapshot pickers.
//   2. Fetch /api/bank/compare/snapshots → render 64-slot grid with cell
//      colors keyed off SlotCompareState (identical / different /
//      source-only / target-only / empty-both).
//   3. Checkbox per slot; defaults to Different + TargetOnly checked.
//   4. Live preview panel via POST /api/bank/merge-plan/preview, grouping
//      the operations into Copy / Keep / Skip / Clear buckets.
//   5. "I understand this will overwrite N slots" confirmation.
//   6. Confirm → POST /api/bank/merge-plan (non-preview) → download the
//      plan as JSON. This flow performs no device write.
//
// Like bank-related.js, this module never sets innerHTML with untrusted
// content: every node is built via createElement + textContent.

import { state } from './bank-state.js';
import { bankApi } from './bank-api.js';
import { toast } from './bank-toast.js';
import { openModal } from './bank-modal.js';
import { bankButton } from './bank-buttons.js';
import { TD3_CHECKBOX } from '../shared/button-classes.js';

const SLOT_KEYS = buildSlotKeys();

// Default selection rule: Different + TargetOnly slots pre-ticked.
const DEFAULT_SELECT_STATES = new Set(['different', 'target_only']);

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/**
 * Opens the full merge planner dialog. Optionally pre-fills the source and
 * target snapshot IDs (callers on the snapshot detail view can use this).
 */
export function openMergeDialog({ sourceId, targetId } = {}) {
    const snapshots = Array.isArray(state.snapshots) ? state.snapshots : [];
    if (snapshots.length < 2) {
        toast('Need at least 2 snapshots to plan a merge', 'info');
        return;
    }

    const body = document.createElement('div');
    body.className = 'merge-dialog';

    // --- Snapshot pickers -----------------------------------------------
    const pickerRow = document.createElement('div');
    pickerRow.className = 'merge-picker-row';

    const srcSelect = buildSnapshotSelect(snapshots, sourceId || snapshots[0].snapshot_id);
    const dstSelect = buildSnapshotSelect(
        snapshots,
        targetId || (snapshots[1] ? snapshots[1].snapshot_id : snapshots[0].snapshot_id),
    );

    pickerRow.appendChild(labeled('Source snapshot', srcSelect));
    pickerRow.appendChild(labeled('Target snapshot', dstSelect));
    body.appendChild(pickerRow);

    // --- Grid + preview two-column layout -------------------------------
    const layout = document.createElement('div');
    layout.className = 'merge-layout';
    body.appendChild(layout);

    const gridPane = document.createElement('div');
    gridPane.className = 'merge-grid-pane';
    layout.appendChild(gridPane);

    const previewPane = document.createElement('div');
    previewPane.className = 'merge-preview-panel';
    layout.appendChild(previewPane);

    // --- Confirmation bar -----------------------------------------------
    const confirmWrap = document.createElement('label');
    confirmWrap.className = 'merge-confirm-row';
    const confirmBox = document.createElement('input');
    confirmBox.type = 'checkbox';
    confirmBox.className = TD3_CHECKBOX;
    confirmWrap.appendChild(confirmBox);
    const confirmSpan = document.createElement('span');
    confirmSpan.className = 'merge-confirm-text';
    confirmSpan.textContent = 'Pick snapshots to see overwrite count.';
    confirmWrap.appendChild(confirmSpan);
    body.appendChild(confirmWrap);

    // --- State tracked across reloads -----------------------------------
    const session = {
        compare: null,        // last SnapshotCompareReport
        selection: new Set(), // slot_keys currently ticked
        plan: null,           // last MergePlan (preview)
        srcId: srcSelect.value,
        dstId: dstSelect.value,
        overwriteCount: 0,
        confirmed: false,
    };

    const reloadCompare = async () => {
        const src = srcSelect.value;
        const dst = dstSelect.value;
        session.srcId = src;
        session.dstId = dst;
        if (!src || !dst) return;
        if (src === dst) {
            gridPane.textContent = '';
            gridPane.appendChild(empty('swap_horiz', 'SAME SNAPSHOT',
                'Pick two different snapshots to plan a merge.'));
            previewPane.textContent = '';
            updateConfirmText(confirmSpan, 0);
            return;
        }
        gridPane.textContent = '';
        gridPane.appendChild(loadingStub('Loading compare matrix…'));
        try {
            const res = await bankApi.compareSnapshots(src, dst);
            session.compare = res.report || { slots: [] };
            session.selection = defaultSelection(session.compare);
            renderGrid(gridPane, session, refreshPreview);
            await refreshPreview();
        } catch (e) {
            gridPane.textContent = '';
            gridPane.appendChild(errorStub(`Compare failed: ${e.message}`));
            previewPane.textContent = '';
            session.compare = null;
            session.plan = null;
            updateConfirmText(confirmSpan, 0);
        }
    };

    const refreshPreview = async () => {
        if (!session.compare || session.srcId === session.dstId) return;
        previewPane.textContent = '';
        previewPane.appendChild(loadingStub('Building preview…'));
        try {
            const res = await bankApi.previewMergePlan({
                source_snapshot_id: session.srcId,
                target_snapshot_id: session.dstId,
                selection: Array.from(session.selection),
            });
            session.plan = res.plan || null;
            renderPreview(previewPane, session.plan);
            const overwrites = countOverwrites(session.plan, session.compare);
            session.overwriteCount = overwrites;
            updateConfirmText(confirmSpan, overwrites);
        } catch (e) {
            previewPane.textContent = '';
            previewPane.appendChild(errorStub(`Preview failed: ${e.message}`));
            session.plan = null;
            updateConfirmText(confirmSpan, 0);
        }
    };

    srcSelect.addEventListener('change', reloadCompare);
    dstSelect.addEventListener('change', reloadCompare);
    confirmBox.addEventListener('change', () => { session.confirmed = confirmBox.checked; });

    openModal({
        title: 'Plan Merge',
        body,
        size: 'wide',
        primaryLabel: 'Download Plan JSON',
        onPrimary: async () => {
            if (!session.compare || session.srcId === session.dstId) {
                toast('Pick two distinct snapshots first', 'error');
                throw new Error('no compare');
            }
            if (!session.plan) {
                toast('Preview is still loading', 'info');
                throw new Error('no plan');
            }
            if (session.overwriteCount > 0 && !session.confirmed) {
                toast(`Tick "I understand this will overwrite ${session.overwriteCount} slot(s)" first`, 'error');
                throw new Error('unconfirmed');
            }
            // Finalize plan (non-preview) and download.
            const res = await bankApi.buildMergePlan({
                source_snapshot_id: session.srcId,
                target_snapshot_id: session.dstId,
                selection: Array.from(session.selection),
            });
            downloadPlanJson(res.plan, session.srcId, session.dstId);
            toast('Merge plan downloaded. No device write was performed.', 'success');
        },
    });

    // Kick off the first load.
    reloadCompare();
}

// ---------------------------------------------------------------------------
// Grid rendering
// ---------------------------------------------------------------------------

function renderGrid(pane, session, onChange) {
    pane.textContent = '';

    const header = document.createElement('div');
    header.className = 'merge-grid-header';

    const title = document.createElement('div');
    title.className = 'text-xs font-black tracking-[0.12em] uppercase';
    title.textContent = '64-SLOT MATRIX';
    header.appendChild(title);

    const legend = document.createElement('div');
    legend.className = 'merge-legend';
    for (const [state_, label] of [
        ['identical',   'identical'],
        ['different',   'different'],
        ['source_only', 'source only'],
        ['target_only', 'target only'],
        ['empty_both',  'empty'],
    ]) {
        const sw = document.createElement('span');
        sw.className = `merge-legend-swatch merge-cell-${state_}`;
        legend.appendChild(sw);
        const tx = document.createElement('span');
        tx.className = 'merge-legend-label';
        tx.textContent = label;
        legend.appendChild(tx);
    }
    header.appendChild(legend);

    const quickRow = document.createElement('div');
    quickRow.className = 'merge-quick-actions';
    quickRow.appendChild(quickBtn('All Different', () => {
        session.selection = selectByStates(session.compare, ['different']);
        renderGrid(pane, session, onChange); onChange();
    }));
    quickRow.appendChild(quickBtn('Default (Diff + TargetOnly)', () => {
        session.selection = defaultSelection(session.compare);
        renderGrid(pane, session, onChange); onChange();
    }));
    quickRow.appendChild(quickBtn('Everything', () => {
        session.selection = selectByStates(session.compare,
            ['identical', 'different', 'source_only', 'target_only', 'empty_both']);
        renderGrid(pane, session, onChange); onChange();
    }));
    quickRow.appendChild(quickBtn('Clear', () => {
        session.selection = new Set();
        renderGrid(pane, session, onChange); onChange();
    }));
    header.appendChild(quickRow);

    pane.appendChild(header);

    const grid = document.createElement('div');
    grid.className = 'merge-slot-grid';
    const byKey = new Map();
    for (const row of session.compare.slots) byKey.set(row.slot_key, row);

    for (const key of SLOT_KEYS) {
        const row = byKey.get(key) || { slot_key: key, state: 'empty_both' };
        grid.appendChild(buildSlotCell(row, session, onChange));
    }
    pane.appendChild(grid);
}

function buildSlotCell(row, session, onChange) {
    const cell = document.createElement('label');
    cell.className = `merge-slot merge-cell-${row.state}`;
    cell.title = cellTooltip(row);
    if (session.selection.has(row.slot_key)) cell.classList.add('selected');

    const box = document.createElement('input');
    box.type = 'checkbox';
    box.checked = session.selection.has(row.slot_key);
    // Empty-both slots can't produce any operation, so tick has no effect
    // (backend would emit skip_empty_source either way). We leave the
    // checkbox enabled for symmetry but visually de-emphasize.
    box.addEventListener('change', () => {
        if (box.checked) session.selection.add(row.slot_key);
        else session.selection.delete(row.slot_key);
        cell.classList.toggle('selected', box.checked);
        onChange();
    });
    cell.appendChild(box);

    const label = document.createElement('span');
    label.className = 'merge-slot-label';
    label.textContent = row.slot_key;
    cell.appendChild(label);

    const st = document.createElement('span');
    st.className = 'merge-slot-state';
    st.textContent = userState(row.state);
    cell.appendChild(st);

    return cell;
}

// ---------------------------------------------------------------------------
// Preview panel
// ---------------------------------------------------------------------------

function renderPreview(pane, plan) {
    pane.textContent = '';
    if (!plan) {
        pane.appendChild(empty('merge', 'NO PLAN', 'Pick snapshots and selections to see a live preview.'));
        return;
    }

    const title = document.createElement('div');
    title.className = 'text-xs font-black tracking-[0.12em] uppercase mb-2';
    title.textContent = 'MERGE PREVIEW';
    pane.appendChild(title);

    const ops = Array.isArray(plan.operations) ? plan.operations : [];
    const buckets = {
        copy_source_to_target: [],
        keep_target: [],
        clear_target: [],
        skip_empty_source: [],
    };
    for (const op of ops) {
        if (buckets[op.action]) buckets[op.action].push(op);
    }

    const summary = document.createElement('div');
    summary.className = 'merge-summary';
    summary.appendChild(summaryChip('copy', 'copy',
        buckets.copy_source_to_target.length, 'op-copy'));
    summary.appendChild(summaryChip('lock',  'keep',
        buckets.keep_target.length, 'op-keep'));
    summary.appendChild(summaryChip('delete_sweep', 'clear',
        buckets.clear_target.length, 'op-clear'));
    summary.appendChild(summaryChip('skip_next', 'skip',
        buckets.skip_empty_source.length, 'op-skip'));
    pane.appendChild(summary);

    // Render the prominent buckets first (copy + clear first since those
    // are the mutating actions), then keep, then skip (least interesting).
    for (const [kind, label] of [
        ['copy_source_to_target', 'COPY'],
        ['clear_target',          'CLEAR'],
        ['keep_target',           'KEEP'],
        ['skip_empty_source',     'SKIP'],
    ]) {
        const list = buckets[kind];
        if (list.length === 0) continue;
        pane.appendChild(buildBucket(label, kind, list));
    }
}

function buildBucket(label, kind, ops) {
    const sec = document.createElement('div');
    sec.className = `merge-bucket merge-bucket-${kind}`;

    const head = document.createElement('div');
    head.className = 'merge-bucket-head';
    const l = document.createElement('span');
    l.textContent = label;
    head.appendChild(l);
    const c = document.createElement('span');
    c.className = 'merge-bucket-count';
    c.textContent = `${ops.length}`;
    head.appendChild(c);
    sec.appendChild(head);

    const body = document.createElement('div');
    body.className = 'merge-bucket-body';
    for (const op of ops) {
        const line = document.createElement('div');
        line.className = `merge-conflict-line merge-conflict-${kind}`;
        const slot = document.createElement('span');
        slot.className = 'merge-conflict-slot';
        slot.textContent = op.slot_key;
        line.appendChild(slot);
        const reason = document.createElement('span');
        reason.className = 'merge-conflict-reason';
        reason.textContent = op.reason || '';
        line.appendChild(reason);
        body.appendChild(line);
    }
    sec.appendChild(body);
    return sec;
}

function summaryChip(icon, label, n, kindCls) {
    const chip = document.createElement('span');
    chip.className = `merge-summary-chip ${kindCls}`;
    const i = document.createElement('span');
    i.className = 'material-symbols-outlined';
    i.textContent = icon;
    chip.appendChild(i);
    const t = document.createElement('span');
    t.textContent = `${label}: ${n}`;
    chip.appendChild(t);
    return chip;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function buildSlotKeys() {
    const out = [];
    for (let g = 1; g <= 4; g++) {
        for (let p = 1; p <= 8; p++) {
            for (const side of ['A', 'B']) {
                out.push(`G${g}-P${p}${side}`);
            }
        }
    }
    return out;
}

function buildSnapshotSelect(snapshots, defaultId) {
    const sel = document.createElement('select');
    for (const s of snapshots) {
        const opt = document.createElement('option');
        opt.value = s.snapshot_id;
        opt.textContent = `${s.name} (${s.snapshot_id})`;
        sel.appendChild(opt);
    }
    if (defaultId) sel.value = defaultId;
    return sel;
}

function labeled(text, child) {
    const wrap = document.createElement('label');
    const span = document.createElement('span');
    span.textContent = text;
    wrap.appendChild(span);
    wrap.appendChild(child);
    return wrap;
}

function quickBtn(label, handler) {
    return bankButton({
        label,
        className: 'merge-quick-btn',
        preventDefault: true,
        onClick: handler,
    });
}

function defaultSelection(compare) {
    const set = new Set();
    if (!compare || !Array.isArray(compare.slots)) return set;
    for (const row of compare.slots) {
        if (DEFAULT_SELECT_STATES.has(row.state)) set.add(row.slot_key);
    }
    return set;
}

function selectByStates(compare, allowed) {
    const set = new Set();
    if (!compare || !Array.isArray(compare.slots)) return set;
    const allow = new Set(allowed);
    for (const row of compare.slots) {
        if (allow.has(row.state)) set.add(row.slot_key);
    }
    return set;
}

function countOverwrites(plan, compare) {
    if (!plan || !Array.isArray(plan.operations)) return 0;
    // An "overwrite" for the confirm copy is: copy_source_to_target on a
    // slot that currently has a non-empty target (Identical/Different from
    // compare) plus clear_target on any slot. These are the mutating cases.
    const byKey = new Map();
    if (compare && Array.isArray(compare.slots)) {
        for (const r of compare.slots) byKey.set(r.slot_key, r.state);
    }
    let n = 0;
    for (const op of plan.operations) {
        if (op.action === 'clear_target') { n++; continue; }
        if (op.action === 'copy_source_to_target') {
            const st = byKey.get(op.slot_key);
            if (st === 'identical' || st === 'different') n++;
        }
    }
    return n;
}

function updateConfirmText(span, overwriteCount) {
    if (overwriteCount > 0) {
        span.textContent = `I understand this will overwrite ${overwriteCount} slot(s) in the target snapshot.`;
    } else {
        span.textContent = 'No overwrite - the plan only fills empty target slots and/or keeps them.';
    }
}

function userState(s) {
    switch (s) {
        case 'identical':   return 'identical';
        case 'different':   return 'different';
        case 'source_only': return 'src-only';
        case 'target_only': return 'tgt-only';
        case 'empty_both':  return 'empty';
        default: return String(s || '');
    }
}

function cellTooltip(row) {
    const parts = [row.slot_key, userState(row.state)];
    if (row.src_item_id) parts.push(`src: ${row.src_item_id}`);
    if (row.dst_item_id) parts.push(`dst: ${row.dst_item_id}`);
    return parts.join(' · ');
}

function downloadPlanJson(plan, srcId, dstId) {
    const blob = new Blob([JSON.stringify(plan, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `merge-plan-${sanitize(srcId)}-to-${sanitize(dstId)}-${Date.now()}.json`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 2000);
}

function sanitize(id) {
    return String(id || '').replace(/[^a-z0-9_-]+/gi, '_').slice(0, 40);
}

function loadingStub(text) {
    const d = document.createElement('div');
    d.className = 'merge-status text-xs font-mono opacity-70';
    d.textContent = text;
    return d;
}

function errorStub(text) {
    const d = document.createElement('div');
    d.className = 'merge-status merge-status-error text-xs font-mono';
    d.textContent = text;
    return d;
}

function empty(iconName, title, hint) {
    const wrap = document.createElement('div');
    wrap.className = 'bank-empty';
    const icon = document.createElement('span');
    icon.className = 'material-symbols-outlined';
    icon.textContent = iconName;
    wrap.appendChild(icon);
    const t = document.createElement('div');
    t.className = 'text-base font-black tracking-widest';
    t.textContent = title;
    wrap.appendChild(t);
    if (hint) {
        const h = document.createElement('div');
        h.className = 'text-xs font-mono opacity-70 max-w-md';
        h.textContent = hint;
        wrap.appendChild(h);
    }
    return wrap;
}
