// Snapshots view.
//
// Two sub-views:
//
//   1. List mode (activeSnapshotId = null): card grid showing every
//      snapshot with its origin badge, created_at, slot count, and pin
//      star. Clicking a card opens detail mode. A "Sync Backups" button
//      in the view header calls POST /api/bank/snapshots/sync-backups.
//
//   2. Detail mode (activeSnapshotId set): a header with inline-editable
//      name + description + pin toggle + action buttons, followed by a
//      4 × 16 grid of 64 slot cells. Each cell shows slot_key, display
//      name, and any changed/duplicate markers. Empty slots are dimmed.
//      Clicking a cell opens a detail drawer for the linked LibraryItem,
//      or shows an "Empty slot" placeholder.
//
// The module deliberately re-renders the whole container on each call
// instead of diffing - consistent with the other bank-* modules.

import {
    state,
    setState,
    toggleSnapshotSelection,
    toggleSnapshotSlotSelection,
    clearSnapshotSlotSelection,
} from './bank-state.js';
import { bankApi } from './bank-api.js';
import { toast } from './bank-toast.js';
import { confirmModal, openModal } from './bank-modal.js';
import { openSnapshotCompare } from './bank-compare.js';
import { openMergeDialog } from './bank-merge.js';
import { makePlayButton } from './bank-play.js';
import { attachExportDropdown } from './bank-snapshot-export.js';
import { bankButton } from './bank-buttons.js';
import { buildSnapshotGrid } from '../shared/snapshot-grid.js';
import { addItemsToControl } from '../shared/add-to-control.js';
import { TD3_CHECKBOX } from '../shared/button-classes.js';

export async function render(container, { onRefreshLibrary } = {}) {
    container.textContent = '';
    if (state.activeSnapshotId) {
        await renderDetail(container, state.activeSnapshotId, { onRefreshLibrary });
    } else {
        renderList(container, { onRefreshLibrary });
    }
}

// ---------------------------------------------------------------------------
// List mode
// ---------------------------------------------------------------------------

function renderList(container, { onRefreshLibrary }) {
    const header = document.createElement('div');
    header.className = 'snapshot-list-header';
    const title = document.createElement('div');
    title.className = 'text-xs font-black tracking-[0.12em] uppercase text-on-surface-variant';
    title.textContent = `${state.snapshots.length} SNAPSHOT(S)`;
    header.appendChild(title);
    const right = document.createElement('div');
    right.style.display = 'flex';
    right.style.gap = '0.4rem';
    right.appendChild(buildSyncButton({ onRefreshLibrary }));
    header.appendChild(right);
    container.appendChild(header);

    if (state.snapshots.length === 0) {
        container.appendChild(emptyState(
            'bookmark',
            'NO SNAPSHOTS',
            'Create a snapshot from the toolbar, or click SYNC BACKUPS to import existing backup zips.'
        ));
        return;
    }

    const grid = document.createElement('div');
    grid.className = 'snapshot-card-grid';
    // Pinned first, then newest first.
    const sorted = [...state.snapshots].sort((a, b) => {
        if (!!b.pinned !== !!a.pinned) return (b.pinned ? 1 : 0) - (a.pinned ? 1 : 0);
        return (b.created_at || '').localeCompare(a.created_at || '');
    });
    for (const snap of sorted) grid.appendChild(buildSnapshotCard(snap, { onRefreshLibrary }));
    container.appendChild(grid);
}

function buildSnapshotCard(snap, { onRefreshLibrary } = {}) {
    const card = document.createElement('div');
    card.className = 'bank-card snapshot-card';
    if (state.selectedSnapshotIds.has(snap.snapshot_id)) card.classList.add('selected');
    card.setAttribute('role', 'button');
    card.tabIndex = 0;

    const top = document.createElement('div');
    top.className = 'snapshot-card-top';
    top.appendChild(buildSnapshotSelectionCheckbox(snap));
    const title = document.createElement('div');
    title.className = 'bank-card-title';
    title.style.flex = '1';
    title.textContent = snap.name || '(unnamed)';
    top.appendChild(title);
    const topActions = document.createElement('div');
    topActions.style.display = 'flex';
    topActions.style.alignItems = 'center';
    topActions.style.gap = '0.35rem';
    if (snap.pinned) {
        const pin = document.createElement('span');
        pin.className = 'material-symbols-outlined snapshot-pin-star';
        pin.textContent = 'push_pin';
        pin.title = 'Pinned';
        topActions.appendChild(pin);
    }
    topActions.appendChild(buildSnapshotDeleteButton(snap, { onRefreshLibrary }));
    top.appendChild(topActions);
    card.appendChild(top);

    const meta = document.createElement('div');
    meta.className = 'bank-card-meta';
    const origin = document.createElement('span');
    origin.className = `source-badge source-badge-${(snap.origin || 'file').toLowerCase()}`;
    origin.textContent = snap.origin || 'manual';
    meta.appendChild(origin);
    const date = document.createElement('span');
    date.textContent = (snap.created_at || '').slice(0, 16);
    meta.appendChild(date);
    const slots = document.createElement('span');
    slots.textContent = `${snap.slot_count ?? 0} slot(s)`;
    meta.appendChild(slots);
    card.appendChild(meta);

    if (snap.description) {
        const desc = document.createElement('div');
        desc.className = 'text-xs opacity-80 snapshot-card-desc';
        desc.textContent = snap.description;
        card.appendChild(desc);
    }

    const open = () => {
        clearSnapshotSlotSelection();
        setState({
            activeSnapshotId: snap.snapshot_id,
            snapshotDetail: null,
            activeSnapshotSlot: null,
        });
    };
    card.addEventListener('click', open);
    card.addEventListener('keydown', (ev) => {
        if (ev.key === 'Enter' || ev.key === ' ') { ev.preventDefault(); open(); }
    });
    return card;
}

function buildSnapshotSelectionCheckbox(snap) {
    const box = document.createElement('input');
    box.type = 'checkbox';
    box.className = TD3_CHECKBOX;
    box.checked = state.selectedSnapshotIds.has(snap.snapshot_id);
    box.title = 'Toggle snapshot selection';
    box.setAttribute('aria-label', `Select snapshot ${snap.name || snap.snapshot_id}`);
    box.addEventListener('click', (ev) => {
        ev.stopPropagation();
        toggleSnapshotSelection(snap.snapshot_id);
    });
    return box;
}

function buildSnapshotDeleteButton(snap, { onRefreshLibrary } = {}) {
    const btn = bankButton({
        icon: 'delete',
        label: 'Delete',
        title: 'Delete snapshot',
        ariaLabel: `Delete snapshot ${snap.name || snap.snapshot_id}`,
        danger: true,
        preventDefault: true,
        stopPropagation: true,
        onClick: async () => {
            const name = snap.name || snap.snapshot_id;
            const ok = await confirmModal({
                title: 'Delete snapshot',
                message:
                    `Snapshot "${name}" will be deleted from the BANK database.\n\n` +
                    `This removes the snapshot record and its slot mappings. ` +
                    `Source files and the TD-3 device are not touched.`,
                okLabel: 'Confirm',
                cancelLabel: 'Cancel',
                danger: true,
            });
            if (!ok) return;
            try {
                const res = await bankApi.deleteSnapshot(snap.snapshot_id);
                toast(
                    `Deleted snapshot "${name}" (${res.removed_slots} slot(s), ${res.removed_items} item(s))`,
                    'success',
                );
                if (state.activeSnapshotId === snap.snapshot_id) {
                    state.activeSnapshotId = null;
                    state.snapshotDetail = null;
                }
                if (typeof onRefreshLibrary === 'function') await onRefreshLibrary();
            } catch (e) {
                toast(`Delete failed: ${e.message}`, 'error');
            }
        },
    });
    return btn;
}

function buildSyncButton({ onRefreshLibrary }) {
    const btn = bankButton({
        icon: 'sync',
        label: 'SYNC BACKUPS',
        stopPropagation: true,
        onClick: async (ev, btn) => {
            btn.disabled = true;
            try {
                const res = await bankApi.syncBackups();
                const added = res.added ?? 0;
                toast(`${added} new backup(s) synced`, added > 0 ? 'success' : 'info');
                if (typeof onRefreshLibrary === 'function') {
                    await onRefreshLibrary();
                }
            } catch (e) {
                toast(`Sync failed: ${e.message}`, 'error');
            } finally {
                btn.disabled = false;
            }
        },
    });
    return btn;
}

// ---------------------------------------------------------------------------
// Detail mode
// ---------------------------------------------------------------------------

async function renderDetail(container, snapshotId, { onRefreshLibrary }) {
    // Lazy fetch (or refresh) the detail payload.
    let detail = state.snapshotDetail;
    if (!detail || detail.snapshot?.snapshot_id !== snapshotId) {
        try {
            detail = await bankApi.getSnapshot(snapshotId);
            // Don't notify (avoid re-entry) - we render below using `detail`
            // directly, then stash it into state without re-triggering render.
            state.snapshotDetail = detail;
        } catch (e) {
            container.appendChild(emptyState('error', 'LOAD FAILED', e.message || ''));
            return;
        }
    }
    const snap = detail.snapshot;
    const slots = detail.slots || [];

    container.appendChild(buildDetailHeader(snap, slots, { onRefreshLibrary }));
    container.appendChild(buildSlotGrid(slots, snap.snapshot_id, { onRefreshLibrary }));
}

function buildDetailHeader(snap, slotViews, { onRefreshLibrary }) {
    const wrap = document.createElement('div');
    wrap.className = 'snapshot-detail-header';

    const back = bankButton({
        icon: 'arrow_back',
        label: 'BACK',
        onClick: () => {
            clearSnapshotSlotSelection();
            setState({
                activeSnapshotId: null,
                snapshotDetail: null,
                activeSnapshotSlot: null,
            });
        },
    });
    wrap.appendChild(back);

    const main = document.createElement('div');
    main.className = 'snapshot-detail-main';

    // Inline-editable name.
    const name = document.createElement('input');
    name.type = 'text';
    name.className = 'snapshot-detail-name';
    name.value = snap.name || '';
    name.setAttribute('aria-label', 'Snapshot name');
    name.addEventListener('change', async () => {
        const newName = name.value.trim();
        if (!newName || newName === (snap.name || '')) return;
        try {
            await bankApi.updateSnapshot(snap.snapshot_id, { name: newName });
            toast('Snapshot renamed', 'success');
            state.snapshotDetail = null;
            if (typeof onRefreshLibrary === 'function') await onRefreshLibrary();
        } catch (e) {
            toast(`Rename failed: ${e.message}`, 'error');
            name.value = snap.name || '';
        }
    });
    main.appendChild(name);

    // Origin + created_at + slot_count.
    const meta = document.createElement('div');
    meta.className = 'bank-card-meta snapshot-detail-meta';
    const origin = document.createElement('span');
    origin.className = `source-badge source-badge-${(snap.origin || 'file').toLowerCase()}`;
    origin.textContent = snap.origin || 'manual';
    meta.appendChild(origin);
    const date = document.createElement('span');
    date.textContent = snap.created_at || '';
    meta.appendChild(date);
    const slots = document.createElement('span');
    slots.textContent = `${snap.slot_count ?? 0} slot(s)`;
    meta.appendChild(slots);
    main.appendChild(meta);

    // Editable description (textarea).
    const desc = document.createElement('textarea');
    desc.className = 'snapshot-detail-desc';
    desc.rows = 2;
    desc.placeholder = 'Describe this snapshot…';
    desc.value = snap.description || '';
    desc.addEventListener('change', async () => {
        const newDesc = desc.value;
        if (newDesc === (snap.description || '')) return;
        try {
            await bankApi.updateSnapshot(snap.snapshot_id, { description: newDesc });
            toast('Description saved', 'success');
            state.snapshotDetail = null;
            if (typeof onRefreshLibrary === 'function') await onRefreshLibrary();
        } catch (e) {
            toast(`Save failed: ${e.message}`, 'error');
            desc.value = snap.description || '';
        }
    });
    main.appendChild(desc);

    wrap.appendChild(main);

    // Action buttons.
    const actions = document.createElement('div');
    actions.className = 'snapshot-detail-actions';
    actions.appendChild(buildActionButton(
        snap.pinned ? 'push_pin' : 'keep',
        snap.pinned ? 'UNPIN' : 'PIN',
        async () => {
            try {
                await bankApi.updateSnapshot(snap.snapshot_id, { pinned: !snap.pinned });
                toast(snap.pinned ? 'Unpinned' : 'Pinned', 'success');
                state.snapshotDetail = null;
                if (typeof onRefreshLibrary === 'function') await onRefreshLibrary();
            } catch (e) {
                toast(`Pin failed: ${e.message}`, 'error');
            }
        }
    ));
    actions.appendChild(buildActionButton('edit', 'RENAME', () => {
        const inp = wrap.querySelector('.snapshot-detail-name');
        if (inp) { inp.focus(); inp.select(); }
    }));
    actions.appendChild(buildActionButton('compare_arrows', 'COMPARE WITH…', () => {
        openSnapshotComparePicker(snap);
    }));
    actions.appendChild(buildActionButton('merge', 'MERGE FROM…', () => {
        // Open the merge dialog with this snapshot pre-filled as the target;
        // the user picks the source inside the dialog.
        openMergeDialog({ targetId: snap.snapshot_id });
    }));
    actions.appendChild(buildAddToControlButton(slotViews));
    actions.appendChild(buildDeleteButton(snap, slotViews, { onRefreshLibrary }));
    actions.appendChild(buildExportButton(snap));
    wrap.appendChild(actions);

    return wrap;
}

/**
 * ADD TO CONTROL button for the snapshot detail header. Operates on
 * `state.selectedSnapshotSlots`, mirroring EXPORT/DELETE: the selection
 * count appears as a small badge, and the click resolves selected slot
 * keys into their backing item IDs (empty slots aren't selectable, so the
 * filter is a safety net rather than primary logic).
 */
function buildAddToControlButton(slotViews) {
    const btn = bankButton({
        icon: 'playlist_add',
        label: 'ADD TO CONTROL',
    });
    const count = state.selectedSnapshotSlots ? state.selectedSnapshotSlots.size : 0;
    if (count > 0) {
        const badge = document.createElement('span');
        badge.className = 'snapshot-export-count';
        badge.textContent = `(${count})`;
        btn.appendChild(badge);
    }
    btn.addEventListener('click', async (ev) => {
        ev.stopPropagation();
        const selected = state.selectedSnapshotSlots;
        if (!selected || selected.size === 0) {
            toast('Select one or more slots to add to Control', 'info');
            return;
        }
        const byKey = new Map();
        for (const s of slotViews || []) byKey.set(s.slot_key, s);
        const ids = [];
        for (const slotKey of selected) {
            const slot = byKey.get(slotKey);
            if (slot && !slot.empty && slot.item_id) ids.push(slot.item_id);
        }
        if (ids.length === 0) {
            toast('Selected slots have no backing patterns to add', 'info');
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

/**
 * EXPORT button with a hover-revealed dropdown of per-pattern formats.
 * Actual click-handling + folder-picker + API call lives in
 * `bank-snapshot-export.js` so bank-snapshots.js stays focused on layout.
 */
function buildExportButton(snap) {
    const host = document.createElement('div');
    host.className = 'snapshot-export-host';
    const btn = bankButton({
        icon: 'file_download',
        label: 'EXPORT',
    });
    const count = state.selectedSnapshotSlots?.size || 0;
    if (count > 0) {
        const badge = document.createElement('span');
        badge.className = 'snapshot-export-count';
        badge.textContent = `(${count})`;
        btn.appendChild(badge);
    }
    host.appendChild(btn);
    attachExportDropdown(host, btn, snap);
    return host;
}

/**
 * DELETE button - mirrors EXPORT's selection-count badge and operates on
 * `state.selectedSnapshotSlots`. With nothing selected it toasts a hint
 * (matches EXPORT, which also no-ops on an empty selection). With >=1
 * selection it opens the same-style confirmation modal as PUSH TO TD-3
 * (red bold address line listing the affected pattern names) and on
 * confirm calls `bankApi.deleteSnapshotSlots`, then forces the snapshot
 * detail + library to reload.
 */
function buildDeleteButton(snap, slots, { onRefreshLibrary }) {
    const btn = bankButton({
        icon: 'delete',
        label: 'DELETE',
    });
    const count = state.selectedSnapshotSlots?.size || 0;
    if (count > 0) {
        const badge = document.createElement('span');
        badge.className = 'snapshot-export-count';
        badge.textContent = `(${count})`;
        btn.appendChild(badge);
    }
    btn.addEventListener('click', (ev) => {
        ev.stopPropagation();
        const selected = state.selectedSnapshotSlots;
        if (!selected || selected.size === 0) {
            toast('Select one or more slots to delete', 'info');
            return;
        }
        openDeleteSlotsModal(snap, slots, { onRefreshLibrary });
    });
    return btn;
}

/**
 * Open the same modal shape as `openPushToTd3Modal` but with a delete
 * message and red bold list of pattern names. Confirm calls
 * `bankApi.deleteSnapshotSlots`; on success the snapshot detail is
 * invalidated and the library is refreshed so the empty placeholders
 * appear and the slot count drops accordingly.
 */
function openDeleteSlotsModal(snap, slots, { onRefreshLibrary }) {
    const selectedKeys = Array.from(state.selectedSnapshotSlots || []);
    if (selectedKeys.length === 0) return;

    // Resolve display names from the loaded slot views, in selection order.
    // Fall back to the slot key when a slot has no display_name (e.g. a
    // backup-origin snapshot that hasn't been re-decoded yet).
    const byKey = new Map();
    for (const s of slots || []) byKey.set(s.slot_key, s);
    const labels = selectedKeys.map((k) => {
        const s = byKey.get(k);
        return (s && s.display_name) ? s.display_name : k;
    });

    const n = selectedKeys.length;
    const body = document.createElement('div');
    body.className = 'bank-confirm-body';

    const introEl = document.createElement('p');
    introEl.textContent = `The following ${n} pattern${n === 1 ? '' : 's'} will be deleted from this snapshot:`;
    body.appendChild(introEl);

    // Red bold name line - same visual treatment as the push-to-td3 modal's
    // address line. ≤8 names stay on one row joined by '  '; longer
    // selections fall back to a multi-column grid so 64 names stay legible.
    if (n <= 8) {
        const nameLine = document.createElement('p');
        nameLine.style.color = '#dc143c';
        nameLine.style.fontWeight = '900';
        nameLine.style.fontSize = '1.5rem';
        nameLine.style.letterSpacing = '0.08em';
        nameLine.style.textAlign = 'center';
        nameLine.style.margin = '0.75rem 0';
        nameLine.textContent = labels.join('  ');
        body.appendChild(nameLine);
    } else {
        const grid = document.createElement('div');
        grid.style.display = 'grid';
        grid.style.gridTemplateColumns = 'repeat(8, 1fr)';
        grid.style.gap = '0.25rem 0.5rem';
        grid.style.margin = '0.75rem 0';
        grid.style.color = '#dc143c';
        grid.style.fontWeight = '800';
        grid.style.fontSize = '0.85rem';
        grid.style.textAlign = 'center';
        for (const name of labels) {
            const cell = document.createElement('span');
            cell.textContent = name;
            grid.appendChild(cell);
        }
        body.appendChild(grid);
    }

    const warnEl = document.createElement('p');
    warnEl.style.opacity = '0.75';
    warnEl.style.fontSize = '0.85rem';
    warnEl.textContent =
        'The snapshot will keep the same 64-cell grid; deleted slots become empty placeholders. '
        + 'Underlying library items are not removed.';
    body.appendChild(warnEl);

    openModal({
        title: `Delete ${n} pattern${n === 1 ? '' : 's'} from snapshot`,
        body,
        primaryLabel: 'CONFIRM',
        secondaryLabel: 'CANCEL',
        danger: true,
        onPrimary: async () => {
            try {
                await bankApi.deleteSnapshotSlots(snap.snapshot_id, selectedKeys);
            } catch (e) {
                throw new Error(`Delete failed: ${e.message}`);
            }
            toast(`Deleted ${n} slot${n === 1 ? '' : 's'} from snapshot`, 'success');
            // Mutate state directly (no notify) so the single render
            // triggered by onRefreshLibrary below sees the cleared
            // selection AND the invalidated detail in one pass - calling
            // notify() multiple times here races concurrent async renders
            // and double-paints the snapshot view.
            state.selectedSnapshotSlots.clear();
            state.snapshotDetail = null;
            if (typeof onRefreshLibrary === 'function') await onRefreshLibrary();
        },
    });
}

function buildActionButton(iconName, label, onClick) {
    return bankButton({
        icon: iconName,
        label,
        stopPropagation: true,
        onClick,
    });
}

// ---------------------------------------------------------------------------
// 64-slot grid (delegates to shared/snapshot-grid.js)
// ---------------------------------------------------------------------------
//
// Bank-specific behavior is layered on top of the shared grid via callbacks:
//   - selection highlighting comes from `state.selectedSnapshotSlots`
//   - click toggles multi-selection (feeds the export flow)
//   - double-click opens the detail drawer via `activeSnapshotSlot`
//   - occupied slots show the global `makePlayButton(item_id)` so the bank
//     play-bus keeps every surface in sync
// Empty slots route clicks to the drawer so the "empty slot" placeholder can
// render inline - that's the same behavior this view had before the refactor.

function buildSlotGrid(slots, snapshotId, { onRefreshLibrary } = {}) {
    const openDrawer = (slot) => {
        setState({ activeSnapshotSlot: slot });
        if (slot.item_id) {
            state.focusedId = slot.item_id;
        }
        setState({}); // trigger re-render
    };
    return buildSnapshotGrid(slots, {
        isSelected: (slot) =>
            !!(state.selectedSnapshotSlots && state.selectedSnapshotSlots.has(slot.slot_key)),
        makePlayButton: (slot) =>
            slot.item_id ? makePlayButton(slot.item_id, { size: 'sm' }) : null,
        onClick: (slot) => {
            if (slot.empty) openDrawer(slot);
            else toggleSnapshotSlotSelection(slot.slot_key);
        },
        onDblClick: (slot, ev) => {
            if (slot.empty) return;
            ev.preventDefault();
            // A double-click fires `click` first, which toggled the
            // selection. Un-toggle so the dblclick acts purely as
            // "open drawer" and doesn't leave a ghost selection behind.
            toggleSnapshotSlotSelection(slot.slot_key);
            openDrawer(slot);
        },
        dragDrop: snapshotId ? buildSlotDragDrop(snapshotId, { onRefreshLibrary }) : null,
    });
}

/**
 * Drag-and-drop wiring for the bank snapshot grid. Only occupied slots
 * are draggable; empty slots accept drops (move) and occupied slots accept
 * drops (swap). On drop we POST `/move-slot` and trigger a single library
 * refresh - same single-render pattern as the rename / delete handlers so
 * we don't race concurrent async renders.
 */
function buildSlotDragDrop(snapshotId, { onRefreshLibrary }) {
    return {
        isDraggable: (slot) => !slot.empty,
        // Reject the source itself; everything else (empty or occupied) is
        // a valid drop target.
        isDropTarget: (fromKey, slot) => fromKey !== slot.slot_key,
        onDrop: async (fromKey, toSlot) => {
            const toKey = toSlot.slot_key;
            if (!fromKey || fromKey === toKey) return;
            try {
                const res = await bankApi.moveSnapshotSlot(snapshotId, fromKey, toKey);
                toast(
                    res.swapped
                        ? `Swapped ${fromKey} ↔ ${toKey}`
                        : `Moved ${fromKey} → ${toKey}`,
                    'success',
                );
            } catch (e) {
                toast(`Move failed: ${e.message}`, 'error');
                return;
            }
            // Invalidate the cached detail and trigger one render. Mirrors
            // the rename / delete handlers - calling notify() multiple
            // times here races concurrent async renders and double-paints
            // the snapshot view.
            state.snapshotDetail = null;
            if (typeof onRefreshLibrary === 'function') await onRefreshLibrary();
        },
    };
}

// ---------------------------------------------------------------------------
// Helpers
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

// Show a small picker modal listing every other snapshot and delegate to
// openSnapshotCompare when one is chosen.
function openSnapshotComparePicker(srcSnapshot) {
    const backdrop = document.createElement('div');
    backdrop.className = 'bank-modal-backdrop';
    const modal = document.createElement('div');
    modal.className = 'bank-modal';

    const h = document.createElement('h3');
    h.textContent = 'Compare snapshot with…';
    modal.appendChild(h);

    const src = document.createElement('div');
    src.className = 'compare-ids';
    src.textContent = `SRC: ${srcSnapshot.name} (${srcSnapshot.snapshot_id})`;
    modal.appendChild(src);

    const candidates = (state.snapshots || [])
        .filter((s) => s && s.snapshot_id !== srcSnapshot.snapshot_id);
    if (candidates.length === 0) {
        const empty = document.createElement('div');
        empty.className = 'compare-empty';
        empty.textContent = 'No other snapshots available to compare against.';
        modal.appendChild(empty);
    } else {
        const list = document.createElement('div');
        list.className = 'compare-snapshot-picker';
        for (const cand of candidates) {
            const row = bankButton({
                label: `${cand.name} (${cand.snapshot_id})`,
                className: 'compare-snapshot-pick',
                onClick: () => {
                backdrop.remove();
                openSnapshotCompare(srcSnapshot.snapshot_id, cand.snapshot_id);
                },
            });
            list.appendChild(row);
        }
        modal.appendChild(list);
    }

    const actions = document.createElement('div');
    actions.className = 'compare-actions';
    const close = bankButton({ label: 'CANCEL', onClick: () => backdrop.remove() });
    actions.appendChild(close);
    modal.appendChild(actions);

    backdrop.appendChild(modal);
    backdrop.addEventListener('click', (ev) => {
        if (ev.target === backdrop) backdrop.remove();
    });
    document.body.appendChild(backdrop);
}

/// Exposed so bank-main can also show an inline placeholder for empty
/// slots on the sidebar's right-hand drawer pathway. Kept here so all
/// snapshot-specific rendering lives in this module.
export function buildEmptySlotPlaceholder(slot) {
    const wrap = document.createElement('div');
    wrap.className = 'snapshot-empty-placeholder';
    const title = document.createElement('div');
    title.className = 'text-base font-black tracking-widest';
    title.textContent = `EMPTY SLOT ${slot.slot_key}`;
    wrap.appendChild(title);
    const hint = document.createElement('div');
    hint.className = 'text-xs font-mono opacity-70';
    hint.textContent = 'No pattern data is stored for this slot in this snapshot.';
    wrap.appendChild(hint);
    return wrap;
}
