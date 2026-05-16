// Hover-to-reveal dropdown attached to the EXPORT button in the snapshot
// detail view.
//
// Interaction contract:
//   - Hovering the EXPORT button (or the menu itself) keeps the menu open.
//     There is no visual gap between the button and the menu - the menu's
//     top edge butts directly against the button so the cursor never has to
//     cross dead space. A ::before pseudo-element on the menu (in CSS)
//     extends the hoverable area a few pixels upward as a safety net.
//   - Moving the cursor away from the host subtree closes the menu on a
//     short delay (so a small jitter doesn't snap it shut).
//   - There is NO click-to-pin behaviour. Clicking outside the host closes
//     the menu immediately. Clicking inside only toggles a checkbox or runs
//     the export.
//   - Format selection uses checkboxes (multi-select). The dropdown has a
//     single EXPORT button at the bottom; it is enabled only when at least
//     one format is checked AND at least one snapshot slot is selected.
//   - On EXPORT click: prompt for a folder via /api/bank/browse-folder, then
//     call bankApi.exportSnapshotPatterns with ALL checked formats in one
//     request. The backend writes every (slot × format) pair into
//     `{source}_export/`.
//   - Format selections are persisted to localStorage so the user's
//     preferred bundle survives page reloads.

import { state, subscribe, clearSnapshotSlotSelection } from './bank-state.js';
import { bankApi } from './bank-api.js';
import { toast } from './bank-toast.js';
import { TD3_CHECKBOX } from '../shared/button-classes.js';

/** Formats offered in the dropdown. `syx` and `sqs` are excluded:
 *  syx is transient scratch data, sqs is bank-level and has no single-
 *  pattern meaning. Keep this list in lockstep with ALLOWED_FORMATS in
 *  src/web/snapshot_export.rs. */
const FORMATS = [
    { id: 'mid',       label: 'MIDI',      ext: 'mid' },
    { id: 'steps_txt', label: 'STEPS.TXT', ext: 'steps.txt' },
    { id: 'seq',       label: 'SEQ',       ext: 'seq' },
    { id: 'pat',       label: 'PAT',       ext: 'pat' },
    { id: 'rbs',       label: 'RBS',       ext: 'rbs' },
    { id: 'toml',      label: 'TOML',      ext: 'toml' },
    { id: 'json',      label: 'JSON',      ext: 'json' },
];

const STORAGE_KEY = 'td3.bank.snapshotExport.formats.v1';
const DEFAULT_FORMATS = Object.freeze({
    mid: true, steps_txt: true, seq: true,
    pat: false, rbs: false, toml: false, json: false,
});

function loadFormats() {
    try {
        const raw = localStorage.getItem(STORAGE_KEY);
        if (!raw) return { ...DEFAULT_FORMATS };
        const parsed = JSON.parse(raw);
        if (!parsed || typeof parsed !== 'object') return { ...DEFAULT_FORMATS };
        const out = { ...DEFAULT_FORMATS };
        for (const k of Object.keys(out)) {
            if (typeof parsed[k] === 'boolean') out[k] = parsed[k];
        }
        return out;
    } catch (_) {
        return { ...DEFAULT_FORMATS };
    }
}

function saveFormats(formats) {
    try { localStorage.setItem(STORAGE_KEY, JSON.stringify(formats)); }
    catch (_) { /* private browsing / quota - not fatal */ }
}

/**
 * Attach a hover dropdown to the EXPORT button. `host` is the outer
 * positioning wrapper; `btn` is the button that opens the menu; `snap` is
 * the current snapshot (for snapshot_id).
 */
export function attachExportDropdown(host, btn, snap) {
    const formats = loadFormats();
    const menu = document.createElement('div');
    menu.className = 'snapshot-export-menu';
    menu.setAttribute('role', 'menu');

    const header = document.createElement('div');
    header.className = 'snapshot-export-menu-header';
    header.textContent = 'Formats to export';
    menu.appendChild(header);

    const list = document.createElement('div');
    list.className = 'snapshot-export-menu-list';
    const checkboxes = new Map();
    for (const fmt of FORMATS) {
        const row = document.createElement('label');
        row.className = 'snapshot-export-menu-check';
        const cb = document.createElement('input');
        cb.type = 'checkbox';
        cb.className = TD3_CHECKBOX;
        cb.checked = !!formats[fmt.id];
        cb.addEventListener('change', () => {
            formats[fmt.id] = !!cb.checked;
            saveFormats(formats);
            updateConfirmState();
        });
        const label = document.createElement('span');
        label.className = 'fmt-label';
        label.textContent = fmt.label;
        const ext = document.createElement('span');
        ext.className = 'fmt-ext';
        ext.textContent = `.${fmt.ext}`;
        row.appendChild(cb);
        row.appendChild(label);
        row.appendChild(ext);
        list.appendChild(row);
        checkboxes.set(fmt.id, cb);
    }
    menu.appendChild(list);

    const footer = document.createElement('div');
    footer.className = 'snapshot-export-menu-footer';
    const confirm = document.createElement('button');
    confirm.type = 'button';
    confirm.className = 'snapshot-export-menu-confirm';
    confirm.textContent = 'EXPORT';
    confirm.addEventListener('click', async (ev) => {
        ev.stopPropagation();
        const chosen = FORMATS.map(f => f.id).filter(id => !!formats[id]);
        close();
        await runExport(snap, chosen);
    });
    footer.appendChild(confirm);
    menu.appendChild(footer);

    host.appendChild(menu);

    function anyChecked() {
        for (const id of Object.keys(formats)) if (formats[id]) return true;
        return false;
    }
    function anySlots() {
        return (state.selectedSnapshotSlots && state.selectedSnapshotSlots.size > 0);
    }
    function updateConfirmState() {
        const ready = anyChecked() && anySlots();
        confirm.disabled = !ready;
        if (!anySlots()) confirm.title = 'Select at least one pattern card first';
        else if (!anyChecked()) confirm.title = 'Tick at least one format';
        else confirm.title = 'Export selected patterns';
    }

    // --- Hover open/close (no click-to-pin) ----------------------------------
    let closeTimer = null;
    function open() {
        if (closeTimer) { clearTimeout(closeTimer); closeTimer = null; }
        host.classList.add('open');
        updateConfirmState();
    }
    function close() {
        if (closeTimer) { clearTimeout(closeTimer); closeTimer = null; }
        host.classList.remove('open');
    }
    function scheduleClose() {
        if (closeTimer) clearTimeout(closeTimer);
        // Short grace window so the cursor can cross a pixel-wide jitter
        // between the button and the menu without snapping shut.
        closeTimer = setTimeout(() => { host.classList.remove('open'); closeTimer = null; }, 180);
    }

    host.addEventListener('mouseenter', open);
    host.addEventListener('mouseleave', scheduleClose);
    btn.addEventListener('focus', open);
    host.addEventListener('focusout', (ev) => {
        if (!host.contains(ev.relatedTarget)) scheduleClose();
    });
    // Also keep the menu open while the user is actively tabbing through
    // the checkboxes with the keyboard.
    menu.addEventListener('mouseenter', open);

    // Click outside the host closes the menu. Clicks inside just do their
    // normal thing (toggle a checkbox, press the confirm button).
    document.addEventListener('click', (ev) => {
        if (!host.contains(ev.target)) close();
    });

    // Pressing the trigger button itself does not pin the menu; it just
    // focuses itself and opens the menu. No toggle.
    btn.addEventListener('click', (ev) => {
        ev.stopPropagation();
        open();
    });

    // Keep the confirm button's enabled state in sync whenever the user
    // toggles their slot selection outside the menu. The subscribe callback
    // self-detaches once the host is no longer in the DOM - which happens
    // when the snapshot detail view is re-rendered.
    const unsubscribe = subscribe(() => {
        if (!document.contains(host)) { unsubscribe(); return; }
        updateConfirmState();
    });
    updateConfirmState();
}

async function runExport(snap, formatIds) {
    const slotKeys = Array.from(state.selectedSnapshotSlots || []);
    if (slotKeys.length === 0) {
        toast('Click one or more pattern cards to select them first', 'info');
        return;
    }
    if (!formatIds || formatIds.length === 0) {
        toast('Tick at least one format', 'info');
        return;
    }

    let targetDir;
    try {
        const resp = await bankApi.browseFolder();
        targetDir = (resp && resp.path) || null;
    } catch (e) {
        toast(`Folder picker failed: ${e.message}`, 'error');
        return;
    }
    if (!targetDir) {
        // User cancelled - silent, matches the scan-folder flow.
        return;
    }

    try {
        const result = await bankApi.exportSnapshotPatterns(snap.snapshot_id, {
            slot_keys: slotKeys,
            formats: formatIds,
            target_dir: targetDir,
        });
        const skipped = Array.isArray(result.skipped) ? result.skipped.length : 0;
        const base = `Exported ${result.file_count} file(s) to ${result.folder_path}`;
        const msg = skipped > 0 ? `${base} - ${skipped} empty slot(s) skipped` : base;
        toast(msg, 'success');
        clearSnapshotSlotSelection();
    } catch (e) {
        toast(`Export failed: ${e.message}`, 'error');
    }
}
