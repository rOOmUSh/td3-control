// Top toolbar: search + primary actions + view toggles. Splits the
// behavior of each button into small handlers so bank-main can wire them
// together without this module knowing about load/refresh internals.

import { state, setState, setFilter } from './bank-state.js';
import { parseQuery, mergeQueryIntoFilter } from './bank-search.js';
import { toast } from './bank-toast.js';
import { bankApi } from './bank-api.js';
import { openFilterPanel } from './bank-filter-panel.js';
import { openModal, promptModal } from './bank-modal.js';
import { openItemCompare } from './bank-compare.js';
import { openMergeDialog } from './bank-merge.js';
import { bankButton } from './bank-buttons.js';
import { addItemsToControl } from '../shared/add-to-control.js';
import { TD3_CHECKBOX } from '../shared/button-classes.js';
import { buildSelectionToolbarControls } from './bank-selection-toolbar.js';

const DEBOUNCE_MS = 200;

/**
 * Render the toolbar into `root`.
 * `onReload` is invoked after any action that needs the item list refreshed.
 */
export function renderToolbar(root, { onReload }) {
    root.textContent = '';

    const selectionControls = buildSelectionToolbarControls({ onReload });
    root.appendChild(selectionControls.checkAll);

    const search = document.createElement('input');
    search.type = 'search';
    search.id = 'bank-search-input';
    search.className = 'bank-search';
    search.placeholder = 'Search  /  tag:foo  scale:phrygian  root:D  slot:G2P4B  favorite';
    search.value = state.searchQuery || '';
    search.setAttribute('aria-label', 'Search library');
    let t = 0;
    search.addEventListener('input', () => {
        const value = search.value;
        clearTimeout(t);
        t = setTimeout(() => {
            const parsed = parseQuery(value);
            const resolveSnapshot = (name) => {
                const match = state.snapshots.find((s) => s.name === name);
                return match ? match.snapshot_id : name;
            };
            const nextFilter = mergeQueryIntoFilter(state.filter, parsed, resolveSnapshot);
            state.searchQuery = value;
            setFilter(nextFilter);
            onReload?.();
        }, DEBOUNCE_MS);
    });
    root.appendChild(search);

    root.appendChild(divider());

    // Filter popover
    const filterBtn = toolbarButton('filter_alt', 'FILTER', () => {
        openFilterPanel(filterBtn, () => onReload?.());
    });
    root.appendChild(filterBtn);

    // Scan folder - open the scan modal
    root.appendChild(toolbarButton('folder_search', 'FOLDER SCAN', () => {
        openScanModal(onReload);
    }));

    // Import files - open the import modal. The backend expects absolute
    // paths (browsers can't supply these via <input type=file>), so we take
    // a list from a textarea, one per line.
    root.appendChild(toolbarButton('file_open', 'IMPORT', () => {
        openImportModal(onReload);
    }));

    root.appendChild(divider());

    // Snapshot creation
    root.appendChild(toolbarButton('add_photo_alternate', 'NEW SNAPSHOT', async () => {
        const name = await promptModal({
            title: 'New snapshot',
            label: 'Snapshot name:',
            okLabel: 'Next',
        });
        if (!name || !name.trim()) return;
        const descRaw = await promptModal({
            title: 'New snapshot',
            label: 'Description (optional):',
            okLabel: 'Create',
        });
        const description = descRaw && descRaw.trim() ? descRaw.trim() : undefined;
        try {
            const res = await bankApi.createSnapshot({
                name: name.trim(),
                description,
                origin: 'manual',
            });
            toast(`Snapshot "${res.snapshot.name}" created`, 'success');
            onReload?.();
        } catch (e) { toast(e.message, 'error'); }
    }));

    // Compare (exactly 2 items must be selected). Delegates to the
    // dedicated compare module, which renders per-step diffs + scoring.
    root.appendChild(toolbarButton('compare_arrows', 'COMPARE', () => {
        const ids = Array.from(state.selectedIds);
        if (ids.length !== 2) {
            toast('Select exactly two items to compare', 'error');
            return;
        }
        openItemCompare(ids[0], ids[1]);
    }));

    // Merge planner: full 64-slot matrix + live preview and a downloadable
    // JSON plan. No device write.
    root.appendChild(toolbarButton('merge', 'MERGE', () => {
        openMergeDialog();
    }));

    root.appendChild(buildBulkAddToControlButton());

    root.appendChild(divider());

    // Cards / Table view toggle
    const viewToggle = document.createElement('div');
    viewToggle.className = 'bank-view-toggle';
    const cardsBtn = viewModeBtn('grid_view', 'CARDS', 'cards');
    const tableBtn = viewModeBtn('table_chart', 'TABLE', 'table');
    const syncViewActive = () => {
        cardsBtn.classList.toggle('is-active', state.viewMode === 'cards');
        tableBtn.classList.toggle('is-active', state.viewMode === 'table');
    };
    cardsBtn.addEventListener('click', () => { setState({ viewMode: 'cards' }); syncViewActive(); });
    tableBtn.addEventListener('click', () => { setState({ viewMode: 'table' }); syncViewActive(); });
    syncViewActive();
    viewToggle.appendChild(cardsBtn);
    viewToggle.appendChild(tableBtn);
    root.appendChild(viewToggle);

    // Dense toggle
    const denseBtn = toolbarButton('view_compact', 'DENSE', () => {
        setState({ dense: !state.dense });
        denseBtn.classList.toggle('is-active', state.dense);
    });
    if (state.dense) denseBtn.classList.add('is-active');
    root.appendChild(denseBtn);

    root.appendChild(selectionControls.deleteButton);
}

function buildBulkAddToControlButton() {
    const btn = bankButton({
        icon: 'playlist_add',
        label: 'ADD TO CONTROL',
        className: 'tactile-button',
    });
    const count = state.selectedIds ? state.selectedIds.size : 0;
    if (count > 0) {
        const badge = document.createElement('span');
        badge.className = 'snapshot-export-count';
        badge.textContent = `(${count})`;
        btn.appendChild(badge);
    }
    btn.addEventListener('click', async (ev) => {
        ev.preventDefault();
        const ids = Array.from(state.selectedIds || []);
        if (ids.length === 0) {
            toast('Select one or more items to add to Control', 'info');
            return;
        }
        btn.disabled = true;
        try {
            await addItemsToControl(ids);
        } finally {
            btn.disabled = false;
        }
    });
    return btn;
}

function toolbarButton(iconName, label, handler) {
    return bankButton({
        icon: iconName,
        label,
        className: 'tactile-button',
        preventDefault: true,
        onClick: (ev, btn) => handler(btn, ev),
    });
}

function viewModeBtn(iconName, label, mode) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.dataset.mode = mode;
    const icon = document.createElement('span');
    icon.className = 'material-symbols-outlined';
    icon.style.fontSize = '0.95rem';
    icon.textContent = iconName;
    btn.appendChild(icon);
    const t = document.createElement('span');
    t.style.marginLeft = '0.25rem';
    t.textContent = label;
    btn.appendChild(t);
    return btn;
}

function divider() {
    const d = document.createElement('div');
    d.className = 'toolbar-divider';
    return d;
}

function openScanModal(onReload) {
    const body = document.createElement('div');

    // Path row: label + input + Browse button.
    const pathLabel = document.createElement('label');
    const pathText = document.createElement('span');
    pathText.textContent = 'Folder path';
    pathLabel.appendChild(pathText);

    const pathRow = document.createElement('div');
    pathRow.className = 'bank-scan-path-row';
    const pathInput = document.createElement('input');
    pathInput.type = 'text';
    pathInput.placeholder = 'C:\\path\\to\\patterns';
    pathInput.autocomplete = 'off';
    pathInput.spellcheck = false;
    pathRow.appendChild(pathInput);

    const browseBtn = bankButton({
        icon: 'folder_open',
        label: 'BROWSE',
        onClick: async () => {
            browseBtn.disabled = true;
            try {
                const res = await bankApi.browseFolder();
                if (res && typeof res.path === 'string' && res.path) {
                    pathInput.value = res.path;
                }
            } catch (e) {
                toast(e.message || String(e), 'error');
            } finally {
                browseBtn.disabled = false;
            }
        },
    });
    pathRow.appendChild(browseBtn);
    pathLabel.appendChild(pathRow);
    body.appendChild(pathLabel);

    const recLabel = document.createElement('label');
    recLabel.style.flexDirection = 'row';
    recLabel.style.alignItems = 'center';
    recLabel.style.gap = '0.5rem';
    const recBox = document.createElement('input');
    recBox.type = 'checkbox';
    recBox.className = TD3_CHECKBOX;
    recBox.checked = true;
    recLabel.appendChild(recBox);
    const recSpan = document.createElement('span');
    recSpan.textContent = 'Recurse into subfolders';
    recSpan.style.textTransform = 'none';
    recLabel.appendChild(recSpan);
    body.appendChild(recLabel);

    // Progress area: replaces the old static extension hint. Shows
    // "Idle" text pre-scan, then live "Found N | Parsing M/N" numbers plus
    // a filling status bar while the scan runs.
    const progress = document.createElement('div');
    progress.className = 'bank-scan-progress';

    const progressText = document.createElement('div');
    progressText.className = 'bank-scan-progress-text';
    progressText.textContent = 'Supports .seq .syx .toml .json .steps.txt .pat .mid and .sqs / .rbs (full-bank).';
    progress.appendChild(progressText);

    const barWrap = document.createElement('div');
    barWrap.className = 'bank-scan-progress-bar';
    const barFill = document.createElement('div');
    barFill.className = 'bank-scan-progress-bar-fill';
    barFill.style.width = '0%';
    barWrap.appendChild(barFill);
    progress.appendChild(barWrap);

    body.appendChild(progress);

    const renderProgress = (p) => {
        const found = p.found || 0;
        const parsed = p.parsed || 0;
        const pct = found > 0 ? Math.min(100, Math.round((parsed / found) * 100)) : 0;
        const active = p.running || p.status === 'queued' || p.status === 'running';
        if (found > 0) {
            progressText.textContent =
                `Found files: ${found} | Parsing ${parsed}/${found} (${pct}%)`;
        } else if (active) {
            progressText.textContent = 'Enumerating supported files...';
        } else if (p.status === 'failed') {
            progressText.textContent = p.error || 'Scan failed';
        } else {
            progressText.textContent = 'Waiting for scan to start...';
        }
        barFill.style.width = `${pct}%`;
    };
    const sleep = (ms) => new Promise(resolve => setTimeout(resolve, ms));
    const waitForScanJob = async (jobId) => {
        for (;;) {
            if (!progressText.isConnected) throw new Error('scan modal closed');
            const job = await bankApi.scanJob(jobId);
            renderProgress(job);
            if (job.status === 'completed') return job;
            if (job.status === 'failed' || job.status === 'cancelled') {
                throw new Error(job.error || `scan ${job.status}`);
            }
            await sleep(250);
        }
    };

    openModal({
        title: 'Scan Folder',
        body,
        primaryLabel: 'Scan',
        onPrimary: async () => {
            const path = pathInput.value.trim();
            if (!path) {
                toast('Enter a folder path', 'error');
                throw new Error('empty path');
            }
            barFill.style.width = '0%';
            progressText.textContent = 'Starting scan...';
            const start = await bankApi.scan({ path, recursive: recBox.checked });
            renderProgress(start);
            const job = await waitForScanJob(start.job_id);
            const t = summariseEntries(job.entries || []);
            toast(
                `Found ${t.found} · imported ${t.imported} · skipped ${t.duplicates + t.unsupported} · failed ${t.failed}`,
                t.failed > 0 ? 'info' : 'success',
            );
            setState({ activeSidebar: 'folder' });
            onReload?.();
        },
    });
}

function openImportModal(onReload) {
    const body = document.createElement('div');

    const lbl = document.createElement('label');
    const lblSpan = document.createElement('span');
    lblSpan.textContent = 'Absolute file paths, one per line';
    lbl.appendChild(lblSpan);
    const ta = document.createElement('textarea');
    ta.rows = 7;
    ta.placeholder = 'C:\\patterns\\first.seq\nC:\\patterns\\second.syx';
    ta.spellcheck = false;
    lbl.appendChild(ta);
    body.appendChild(lbl);

    const hint = document.createElement('div');
    hint.className = 'text-xs font-mono opacity-70 mt-2';
    hint.textContent = 'Paths are processed in order. Duplicates skipped automatically.';
    body.appendChild(hint);

    openModal({
        title: 'Import Files',
        body,
        primaryLabel: 'Import',
        onPrimary: async () => {
            const paths = ta.value.split(/\r?\n/).map((p) => p.trim()).filter(Boolean);
            if (paths.length === 0) {
                toast('Enter at least one path', 'error');
                throw new Error('empty paths');
            }
            const res = await bankApi.importFiles(paths);
            const t = summariseEntries(res.entries);
            toast(
                `Found ${t.found} · imported ${t.imported} · skipped ${t.duplicates + t.unsupported} · failed ${t.failed}`,
                t.failed > 0 ? 'info' : 'success',
            );
            setState({ activeSidebar: 'folder' });
            onReload?.();
        },
    });
}

function summariseEntries(entries) {
    const out = { found: 0, imported: 0, duplicates: 0, unsupported: 0, failed: 0 };
    if (!Array.isArray(entries)) return out;
    out.found = entries.length;
    for (const e of entries) {
        switch (e.status) {
            case 'imported':          out.imported++; break;
            case 'duplicate_skipped': out.duplicates++; break;
            case 'unsupported':       out.unsupported++; break;
            case 'failed':            out.failed++; break;
            default: break;
        }
    }
    return out;
}

