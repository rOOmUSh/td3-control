// Ingest / ImportBatch browser.
//
// Exports `render(container, mode)` where mode is:
//   - 'batches': list of ImportBatch cards; click → detail view (entry table)
//   - 'failed' : cross-batch list of FileIndexEntry rows whose status='failed'
//
// All paths / filenames / error strings are untrusted - every piece of text
// that hits the DOM goes through textContent, never innerHTML. State is
// transient (module-scoped) so clicks on "view details" don't bleed into the
// global state.js - it's read-only from the app's perspective.

import { state, setState, defaultFilter, toggleImportBatchSelection } from './bank-state.js';
import { bankApi } from './bank-api.js';
import { toast } from './bank-toast.js';
import { confirmModal } from './bank-modal.js';
import { decorateItems } from './bank-derived.js';
import { makePlayButton } from './bank-play.js';
import { bankButton, menuButton } from './bank-buttons.js';
import { TD3_CHECKBOX } from '../shared/button-classes.js';

// Local transient UI state: which batch is drilled into (null = list mode).
// Kept module-local so the main `state` stays focused on library data.
let detailBatchId = null;
let detailBatchData = null;   // { batch, entries } | null while loading
let viewEpoch = 0;

/**
 * Render the ingest view for `mode` into `container`. Triggered from
 * bank-main when activeSidebar === 'folder' or 'failed-imports'.
 */
export function render(container, mode) {
    const epoch = ++viewEpoch;
    container.textContent = '';

    if (mode === 'failed') {
        renderFailedList(container, epoch);
        return;
    }

    // 'batches' mode: either list or detail.
    if (detailBatchId) {
        renderBatchDetail(container);
    } else {
        renderBatchesList(container);
    }
}

/** Reset any drill-in state. Called by bank-main when the sidebar changes. */
export function resetView() {
    detailBatchId = null;
    detailBatchData = null;
    state.activeImportBatchId = null;
}

// ---------------------------------------------------------------------------
// Batches list
// ---------------------------------------------------------------------------

function renderBatchesList(container) {
    const batches = state.importBatches || [];
    if (batches.length === 0) {
        container.appendChild(emptyState(
            'folder_off',
            'NO IMPORTS YET',
            'Use SCAN or IMPORT from the toolbar to register files in the library.',
        ));
        return;
    }

    const wrap = document.createElement('div');
    wrap.className = 'ingest-batch-list' + (state.dense ? ' dense' : '');

    // Sort newest first by started_at when available.
    const sorted = batches.slice().sort((a, b) => {
        const ak = a.started_at || '';
        const bk = b.started_at || '';
        return bk.localeCompare(ak);
    });

    for (const b of sorted) wrap.appendChild(renderBatchCard(b));
    container.appendChild(wrap);
}

function renderBatchCard(b) {
    const card = document.createElement('div');
    card.className = 'ingest-batch-card bank-card';
    if (state.selectedImportBatchIds.has(b.batch_id)) card.classList.add('selected');
    card.tabIndex = 0;

    const top = document.createElement('div');
    top.className = 'snapshot-card-top';
    top.appendChild(buildBatchSelectionCheckbox(b));
    const title = document.createElement('div');
    title.className = 'bank-card-title';
    title.style.flex = '1';
    title.textContent = b.scan_root ? b.scan_root : `batch ${shortId(b.batch_id)}`;
    top.appendChild(title);
    card.appendChild(top);

    const meta = document.createElement('div');
    meta.className = 'bank-card-meta';
    meta.appendChild(chipStatus('found', b.files_found));
    meta.appendChild(chipStatus('imported', b.files_imported));
    meta.appendChild(chipStatus('duplicate-skipped', b.duplicates_skipped));
    meta.appendChild(chipStatus('unsupported', b.unsupported));
    meta.appendChild(chipStatus('failed', b.failed));
    card.appendChild(meta);

    const when = document.createElement('div');
    when.className = 'text-[0.65rem] opacity-60 font-mono';
    when.textContent = b.finished_at || b.started_at || '';
    card.appendChild(when);

    const actions = document.createElement('div');
    actions.style.display = 'flex';
    actions.style.gap = '0.35rem';
    actions.style.marginTop = '0.25rem';
    actions.appendChild(actionBtn('open_in_new', 'View details', (ev) => {
        ev.stopPropagation();
        openBatchDetail(b.batch_id);
    }));
    if ((b.failed || 0) > 0) {
        actions.appendChild(actionBtn('refresh', 'Retry failed', async (ev) => {
            ev.stopPropagation();
            await retryFailed(b.batch_id);
        }));
    }
    const delBtn = actionBtn('delete', 'Delete', async (ev) => {
        ev.stopPropagation();
        await deleteBatch(b.batch_id, { label: b.scan_root || shortId(b.batch_id) });
    });
    delBtn.classList.add('danger');
    actions.appendChild(delBtn);
    card.appendChild(actions);

    card.addEventListener('click', () => openBatchDetail(b.batch_id));
    card.addEventListener('keydown', (ev) => {
        if (ev.key === 'Enter' || ev.key === ' ') {
            ev.preventDefault();
            openBatchDetail(b.batch_id);
        }
    });
    return card;
}

async function openBatchDetail(batchId) {
    detailBatchId = batchId;
    detailBatchData = null;
    // Trigger a state tick so the container re-renders via bank-main's
    // subscriber. We don't mutate real state keys, but setState({}) would
    // spam unnecessary renders - instead toggle a tiny marker.
    setState({ activeImportBatchId: batchId, _ingestTick: (state._ingestTick || 0) + 1 });
    try {
        const res = await bankApi.getImportBatch(batchId);
        if (state.activeSidebar !== 'folder' || detailBatchId !== batchId) return;
        detailBatchData = res;
        setState({ _ingestTick: (state._ingestTick || 0) + 1 });
    } catch (e) {
        toast(`Load batch failed: ${e.message}`, 'error');
        detailBatchId = null;
        setState({ activeImportBatchId: null, _ingestTick: (state._ingestTick || 0) + 1 });
    }
}

// ---------------------------------------------------------------------------
// Batch detail
// ---------------------------------------------------------------------------

function renderBatchDetail(container) {
    const data = detailBatchData;
    const header = document.createElement('div');
    header.className = 'snapshot-list-header';

    const back = bankButton({
        icon: 'arrow_back',
        label: 'BACK',
        onClick: () => {
        detailBatchId = null;
        detailBatchData = null;
        setState({ activeImportBatchId: null, _ingestTick: (state._ingestTick || 0) + 1 });
        },
    });
    header.appendChild(back);

    const title = document.createElement('div');
    title.className = 'text-sm font-black tracking-wider';
    if (data?.batch) {
        title.textContent = data.batch.scan_root || `batch ${shortId(data.batch.batch_id)}`;
    } else {
        title.textContent = 'LOADING…';
    }
    header.appendChild(title);

    const rightActions = document.createElement('div');
    rightActions.style.display = 'flex';
    rightActions.style.gap = '0.35rem';
    if (data?.batch && (data.batch.failed || 0) > 0) {
        rightActions.appendChild(actionBtn('refresh', 'Retry failed', async () => {
            await retryFailed(detailBatchId, /*refreshDetail=*/true);
        }));
    }
    if (data?.batch) {
        const delBtn = actionBtn('delete', 'Delete', async () => {
            await deleteBatch(detailBatchId, {
                label: data.batch.scan_root || shortId(data.batch.batch_id),
                fromDetail: true,
            });
        });
        delBtn.classList.add('danger');
        rightActions.appendChild(delBtn);
    }
    header.appendChild(rightActions);

    container.appendChild(header);

    if (!data) {
        container.appendChild(emptyState('hourglass_empty', 'LOADING', 'Fetching batch entries…'));
        return;
    }

    // Summary line.
    const sum = document.createElement('div');
    sum.className = 'text-xs font-mono opacity-75 mb-2';
    const b = data.batch;
    sum.textContent = `found ${b.files_found} · imported ${b.files_imported} · duplicate-skipped ${b.duplicates_skipped} · unsupported ${b.unsupported} · failed ${b.failed}`;
    container.appendChild(sum);

    const entries = Array.isArray(data.entries) ? data.entries : [];
    if (entries.length === 0) {
        container.appendChild(emptyState('inbox', 'NO ENTRIES', 'This batch recorded no file entries.'));
        return;
    }
    container.appendChild(renderEntryTable(entries, { showBatch: false }));
}

// ---------------------------------------------------------------------------
// Failed-imports cross-batch list
// ---------------------------------------------------------------------------

function renderFailedList(container, epoch) {
    const header = document.createElement('div');
    header.className = 'snapshot-list-header';

    const title = document.createElement('div');
    title.className = 'text-sm font-black tracking-wider';
    title.textContent = 'FAILED IMPORTS';
    header.appendChild(title);

    container.appendChild(header);

    const batches = state.importBatches || [];
    if (batches.length === 0) {
        container.appendChild(emptyState(
            'check_circle',
            'NO BATCHES',
            'Nothing has been scanned or imported yet.',
        ));
        return;
    }

    // Load entries for every batch in parallel. We cache by batch_id in a
    // module-local dict keyed by batch_id. This keeps the page snappy on
    // repeated visits without polluting global state.
    const info = document.createElement('div');
    info.className = 'text-xs font-mono opacity-70 mb-2';
    info.textContent = 'Loading entries…';
    container.appendChild(info);

    loadFailedAcrossBatches(batches).then((allFailed) => {
        if (epoch !== viewEpoch || state.activeSidebar !== 'failed-imports' || !container.isConnected) return;
        info.remove();
        if (allFailed.length === 0) {
            container.appendChild(emptyState('check_circle', 'ALL CLEAR', 'No failed imports across any batch.'));
            return;
        }
        const count = document.createElement('div');
        count.className = 'text-xs font-mono opacity-75 mb-2';
        count.textContent = `${allFailed.length} failed entr${allFailed.length === 1 ? 'y' : 'ies'}`;
        container.appendChild(count);
        container.appendChild(renderEntryTable(allFailed, { showBatch: true }));
    }).catch((e) => {
        if (epoch !== viewEpoch || state.activeSidebar !== 'failed-imports' || !container.isConnected) return;
        info.textContent = `Load failed: ${e.message}`;
    });
}

async function loadFailedAcrossBatches(batches) {
    const out = [];
    // Serialise the fetches to keep the server happy under unexpected load -
    // the typical user has <20 batches, so the extra latency is negligible.
    for (const b of batches) {
        if ((b.failed || 0) === 0) continue;
        try {
            const res = await bankApi.getImportBatch(b.batch_id);
            const entries = Array.isArray(res.entries) ? res.entries : [];
            for (const e of entries) {
                if (e.status === 'failed') {
                    out.push({ ...e, _batch: b });
                }
            }
        } catch (err) {
            // Ignore individual batch failures so one bad row doesn't block
            // the whole view; surface them as toasts.
            toast(`Batch ${shortId(b.batch_id)} unreadable: ${err.message}`, 'error');
        }
    }
    return out;
}

// ---------------------------------------------------------------------------
// Entry table
// ---------------------------------------------------------------------------

function renderEntryTable(entries, { showBatch }) {
    const wrap = document.createElement('div');
    wrap.style.overflowX = 'auto';

    const table = document.createElement('table');
    table.className = 'bank-table' + (state.dense ? ' dense' : '');

    const thead = document.createElement('thead');
    const tr = document.createElement('tr');
    for (const h of ['Play', 'File Name', 'Path', 'Format', 'Status', 'Error', 'Size', 'Actions']) {
        const th = document.createElement('th');
        th.textContent = h;
        tr.appendChild(th);
    }
    if (showBatch) {
        const th = document.createElement('th');
        th.textContent = 'Batch';
        tr.insertBefore(th, tr.firstChild);
    }
    thead.appendChild(tr);
    table.appendChild(thead);

    const tbody = document.createElement('tbody');
    for (const entry of entries) tbody.appendChild(renderEntryRow(entry, { showBatch }));
    table.appendChild(tbody);

    wrap.appendChild(table);
    return wrap;
}

function renderEntryRow(entry, { showBatch }) {
    const tr = document.createElement('tr');
    tr.className = 'ingest-entry-row';

    if (showBatch) {
        const td = document.createElement('td');
        const b = entry._batch;
        td.textContent = b ? (b.scan_root || shortId(b.batch_id)) : '-';
        td.title = b ? (b.scan_root || b.batch_id) : '';
        tr.appendChild(td);
    }

    // Play - only for entries that actually landed in the library as a
    // LibraryItem (failed/duplicate/unsupported rows have no item_id).
    const playTd = document.createElement('td');
    playTd.style.width = '32px';
    if (entry.item_id) {
        playTd.appendChild(makePlayButton(entry.item_id, { size: 'sm' }));
    } else {
        playTd.textContent = '-';
        playTd.style.opacity = '0.4';
        playTd.style.textAlign = 'center';
    }
    tr.appendChild(playTd);

    // File name
    const name = document.createElement('td');
    name.textContent = fileNameOf(entry.path);
    name.title = entry.path || '';
    tr.appendChild(name);

    // Path (truncated - css does the ellipsis, title has the full thing)
    const pathTd = document.createElement('td');
    pathTd.textContent = shortPath(entry.path);
    pathTd.title = entry.path || '';
    tr.appendChild(pathTd);

    // Format
    const fmt = document.createElement('td');
    fmt.textContent = entry.format || '-';
    tr.appendChild(fmt);

    // Status badge
    const stTd = document.createElement('td');
    stTd.appendChild(statusBadge(entry.status));
    tr.appendChild(stTd);

    // Error (may be long; truncate visually)
    const errTd = document.createElement('td');
    if (entry.error) {
        errTd.textContent = entry.error;
        errTd.title = entry.error;
    } else {
        errTd.textContent = '-';
    }
    tr.appendChild(errTd);

    // Size
    const sizeTd = document.createElement('td');
    sizeTd.textContent = formatSize(entry.size);
    sizeTd.style.fontFamily = 'monospace';
    sizeTd.style.textAlign = 'right';
    tr.appendChild(sizeTd);

    // Actions kebab - real <button> so keyboard activation, focus ring, and
    // screen readers all work, and automation can target role="button".
    const actsTd = document.createElement('td');
    const kebab = document.createElement('button');
    kebab.type = 'button';
    kebab.className = 'bank-row-action';
    kebab.setAttribute('aria-label', 'Row actions');
    kebab.setAttribute('aria-haspopup', 'menu');
    kebab.style.background = 'transparent';
    kebab.style.border = 'none';
    kebab.style.padding = '0.125rem';
    kebab.style.cursor = 'pointer';
    kebab.style.color = 'inherit';
    kebab.style.display = 'inline-flex';
    kebab.style.alignItems = 'center';
    const kebabIcon = document.createElement('span');
    kebabIcon.className = 'material-symbols-outlined';
    kebabIcon.textContent = 'more_vert';
    kebabIcon.setAttribute('aria-hidden', 'true');
    kebab.appendChild(kebabIcon);
    kebab.addEventListener('click', (ev) => {
        ev.stopPropagation();
        // Fall back to the button's own rect when the activation came from the
        // keyboard (Enter/Space), where clientX/Y are 0.
        const rect = kebab.getBoundingClientRect();
        const x = ev.clientX || rect.right;
        const y = ev.clientY || rect.bottom;
        openEntryKebab(x, y, entry);
    });
    actsTd.appendChild(kebab);
    tr.appendChild(actsTd);

    return tr;
}

function openEntryKebab(x, y, entry) {
    document.querySelectorAll('.bank-kebab').forEach((el) => el.remove());
    const menu = document.createElement('div');
    menu.className = 'bank-kebab';
    menu.style.left = `${Math.min(x, window.innerWidth - 220)}px`;
    menu.style.top = `${Math.min(y, window.innerHeight - 180)}px`;

    const batchId = entry.batch_id || entry._batch?.batch_id;
    if (batchId) {
        menu.appendChild(kebabItem('refresh', 'Retry batch', async () => {
            menu.remove();
            await retryFailed(batchId);
        }));
    }
    menu.appendChild(kebabItem('link', 'Copy Path', () => {
        menu.remove();
        copyToClipboard(entry.path || '');
    }));
    if (entry.error) {
        menu.appendChild(kebabItem('bug_report', 'Copy Error', () => {
            menu.remove();
            copyToClipboard(entry.error || '');
        }));
    }
    if (entry.item_id) {
        menu.appendChild(kebabItem('info', 'Open item in drawer', () => {
            menu.remove();
            setState({ focusedId: entry.item_id });
        }));
    }

    document.body.appendChild(menu);
    const outside = (ev) => {
        if (!menu.contains(ev.target)) {
            menu.remove();
            document.removeEventListener('mousedown', outside);
        }
    };
    setTimeout(() => document.addEventListener('mousedown', outside), 0);
}

function kebabItem(iconName, label, onClick) {
    return menuButton(iconName, label, onClick);
}

// ---------------------------------------------------------------------------
// Retry
// ---------------------------------------------------------------------------

async function deleteBatch(batchId, { label = '', fromDetail = false } = {}) {
    const pretty = label || shortId(batchId);
    const ok = await confirmModal({
        title: 'Delete import batch',
        message:
            `Delete import batch "${pretty}" from the library?\n\n` +
            `Removes the batch record plus any items and snapshots it exclusively owns.\n` +
            `Files on disk are not touched.`,
        okLabel: 'Delete',
        cancelLabel: 'Cancel',
        danger: true,
    });
    if (!ok) return;
    try {
        const res = await bankApi.deleteImportBatch(batchId);
        toast(
            `Deleted "${pretty}": ${res.removed_entries} entries, ${res.removed_items} item(s), ${res.removed_snapshots} snapshot(s)`,
            'success',
        );
        if (fromDetail || detailBatchId === batchId) {
            detailBatchId = null;
            detailBatchData = null;
            state.activeImportBatchId = null;
        }
        await refreshLibraryState();
    } catch (e) {
        toast(`Delete failed: ${e.message}`, 'error');
    }
}

async function retryFailed(batchId, refreshDetail = false) {
    try {
        const res = await bankApi.retryFailedBatch(batchId);
        toast(
            `Retried ${res.processed}: ${res.succeeded} ok · ${res.still_failed} still failing`,
            res.still_failed === 0 ? 'success' : 'info',
        );
        await refreshLibraryState();
        if (refreshDetail && detailBatchId === batchId) {
            detailBatchData = await bankApi.getImportBatch(batchId);
            setState({ _ingestTick: (state._ingestTick || 0) + 1 });
        }
    } catch (e) {
        toast(`Retry failed: ${e.message}`, 'error');
    }
}

async function refreshLibraryState() {
    const [bl, filteredItems, allItems, sl, rel, dup] = await Promise.all([
        bankApi.listImportBatches(),
        bankApi.listItems(state.filter),
        bankApi.listItems(defaultFilter()),
        bankApi.listSnapshots(),
        bankApi.listRelated(),
        bankApi.listDuplicates(),
    ]);
    setState({
        importBatches: bl.batches || [],
        items: decorateItems(filteredItems.items || [], { related: rel, duplicates: dup }),
        libraryItems: decorateItems(allItems.items || [], { related: rel, duplicates: dup }),
        snapshots: sl.snapshots || [],
        related: rel || { groups: [], relations: [] },
        duplicates: dup || { clusters: [] },
    });
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

function emptyState(iconName, title, hint) {
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

function actionBtn(iconName, label, onClick) {
    return bankButton({ icon: iconName, label, onClick });
}

function buildBatchSelectionCheckbox(batch) {
    const box = document.createElement('input');
    box.type = 'checkbox';
    box.className = TD3_CHECKBOX;
    box.checked = state.selectedImportBatchIds.has(batch.batch_id);
    box.title = 'Toggle imported folder selection';
    box.setAttribute('aria-label', `Select imported folder ${batch.scan_root || shortId(batch.batch_id)}`);
    box.addEventListener('click', (ev) => {
        ev.stopPropagation();
        toggleImportBatchSelection(batch.batch_id);
    });
    return box;
}

function statusBadge(status) {
    const sp = document.createElement('span');
    sp.className = `status-badge status-${String(status || 'unknown').toLowerCase()}`;
    sp.textContent = String(status || 'unknown').replace(/_/g, '-');
    return sp;
}

function chipStatus(kind, count) {
    const wrap = document.createElement('span');
    wrap.style.display = 'inline-flex';
    wrap.style.alignItems = 'center';
    wrap.style.gap = '0.25rem';
    const pill = document.createElement('span');
    pill.className = `status-badge status-${kind}`;
    pill.textContent = kind;
    wrap.appendChild(pill);
    const num = document.createElement('span');
    num.textContent = String(count ?? 0);
    num.style.fontFamily = 'monospace';
    wrap.appendChild(num);
    return wrap;
}

function fileNameOf(path) {
    if (!path) return '-';
    const idx = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
    if (idx < 0) return path;
    return path.slice(idx + 1);
}

function shortPath(path) {
    if (!path) return '-';
    if (path.length <= 64) return path;
    // keep head + tail, ellide the middle. Uses a plain dot-triple - every
    // renderer uses textContent so no HTML entity worries.
    const head = path.slice(0, 28);
    const tail = path.slice(-28);
    return `${head}...${tail}`;
}

function shortId(id) {
    if (!id) return '';
    return String(id).slice(0, 8);
}

function formatSize(n) {
    const v = Number(n) || 0;
    if (v < 1024) return `${v} B`;
    if (v < 1024 * 1024) return `${(v / 1024).toFixed(1)} KB`;
    return `${(v / (1024 * 1024)).toFixed(1)} MB`;
}

async function copyToClipboard(text) {
    try {
        await navigator.clipboard.writeText(text || '');
        toast('Copied to clipboard', 'success');
    } catch {
        toast('Clipboard unavailable', 'error');
    }
}
