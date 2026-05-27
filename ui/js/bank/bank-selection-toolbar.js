import {
    state,
    subscribe,
    setState,
    setVisibleItemSelection,
    setSnapshotSelection,
    setImportBatchSelection,
} from './bank-state.js';
import { bankApi } from './bank-api.js';
import { bankButton } from './bank-buttons.js';
import { confirmModal } from './bank-modal.js';
import { toast } from './bank-toast.js';
import { TD3_CHECKBOX } from '../shared/button-classes.js';

const ITEM_VIEWS = new Set(['all', 'favorites', 'needs-review']);

export function buildSelectionToolbarControls({ onReload } = {}) {
    const checkLabel = document.createElement('label');
    checkLabel.className = 'bank-toolbar-check-all';
    checkLabel.title = 'Check all visible cards';

    const checkBox = document.createElement('input');
    checkBox.type = 'checkbox';
    checkBox.className = TD3_CHECKBOX;
    checkLabel.appendChild(checkBox);

    const checkText = document.createElement('span');
    checkText.textContent = 'CHECK ALL';
    checkLabel.appendChild(checkText);

    const deleteBtn = bankButton({
        icon: 'delete',
        label: 'DELETE',
        danger: true,
        className: 'tactile-button',
        title: 'Delete selected records',
    });

    checkBox.addEventListener('click', (event) => event.stopPropagation());
    checkBox.addEventListener('change', () => {
        const context = currentSelectionContext();
        if (!context) return;
        context.setAll(checkBox.checked);
    });

    deleteBtn.addEventListener('click', async (event) => {
        event.preventDefault();
        const context = currentSelectionContext();
        if (!context || context.selectedIds.length === 0) {
            toast('Select one or more records to delete', 'info');
            return;
        }
        await context.deleteSelected(context.selectedIds, { onReload });
    });

    const sync = () => {
        const context = currentSelectionContext();
        if (!context) {
            checkBox.disabled = true;
            checkBox.checked = false;
            checkBox.indeterminate = false;
            deleteBtn.disabled = true;
            setButtonLabel(deleteBtn, 'DELETE');
            return;
        }

        const visibleSelected = context.visibleIds.filter((id) => context.selectedSet.has(id)).length;
        checkBox.disabled = context.visibleIds.length === 0;
        checkBox.checked = context.visibleIds.length > 0 && visibleSelected === context.visibleIds.length;
        checkBox.indeterminate = visibleSelected > 0 && visibleSelected < context.visibleIds.length;

        const count = context.selectedIds.length;
        deleteBtn.disabled = count === 0;
        setButtonLabel(deleteBtn, count > 0 ? `DELETE(${count})` : 'DELETE');
    };

    subscribe(sync);
    sync();

    return { checkAll: checkLabel, deleteButton: deleteBtn };
}

function currentSelectionContext() {
    if (state.activeSidebar === 'snapshots' && !state.activeSnapshotId) {
        const visibleIds = (state.snapshots || []).map((snapshot) => snapshot.snapshot_id).filter(Boolean);
        return {
            visibleIds,
            selectedSet: state.selectedSnapshotIds,
            selectedIds: Array.from(state.selectedSnapshotIds || []),
            setAll: (on) => setSnapshotSelection(visibleIds, on),
            deleteSelected: deleteSnapshots,
        };
    }

    if (state.activeSidebar === 'folder' && !state.activeImportBatchId) {
        const visibleIds = (state.importBatches || []).map((batch) => batch.batch_id).filter(Boolean);
        return {
            visibleIds,
            selectedSet: state.selectedImportBatchIds,
            selectedIds: Array.from(state.selectedImportBatchIds || []),
            setAll: (on) => setImportBatchSelection(visibleIds, on),
            deleteSelected: deleteImportBatches,
        };
    }

    if (ITEM_VIEWS.has(state.activeSidebar)) {
        const visibleIds = (state.items || []).map((item) => item.item_id).filter(Boolean);
        return {
            visibleIds,
            selectedSet: state.selectedIds,
            selectedIds: Array.from(state.selectedIds || []),
            setAll: (on) => setVisibleItemSelection(visibleIds, on),
            deleteSelected: deleteItems,
        };
    }

    return null;
}

async function deleteItems(ids, { onReload } = {}) {
    const ok = await confirmModal({
        title: 'Delete bank items',
        message:
            `Delete ${ids.length} bank item${ids.length === 1 ? '' : 's'}?\n\n` +
            'This removes item records and tag links. Source files and the TD-3 device are not touched.',
        okLabel: 'Delete',
        cancelLabel: 'Cancel',
        danger: true,
    });
    if (!ok) return;
    const { deleted, failed } = await deleteSequential(ids, (id) => bankApi.deleteItem(id));
    for (const id of deleted) state.selectedIds.delete(id);
    if (deleted.includes(state.focusedId)) state.focusedId = null;
    reportDelete('item', 'items', deleted.length, failed.length);
    if (typeof onReload === 'function') await onReload();
    else setState({});
}

async function deleteSnapshots(ids, { onReload } = {}) {
    const ok = await confirmModal({
        title: 'Delete snapshots',
        message:
            `Delete ${ids.length} snapshot${ids.length === 1 ? '' : 's'}?\n\n` +
            'This removes snapshot records and slot mappings. Source files and the TD-3 device are not touched.',
        okLabel: 'Delete',
        cancelLabel: 'Cancel',
        danger: true,
    });
    if (!ok) return;
    const { deleted, failed } = await deleteSequential(ids, (id) => bankApi.deleteSnapshot(id));
    for (const id of deleted) state.selectedSnapshotIds.delete(id);
    if (deleted.includes(state.activeSnapshotId)) {
        state.activeSnapshotId = null;
        state.snapshotDetail = null;
        state.activeSnapshotSlot = null;
    }
    reportDelete('snapshot', 'snapshots', deleted.length, failed.length);
    if (typeof onReload === 'function') await onReload();
    else setState({});
}

async function deleteImportBatches(ids, { onReload } = {}) {
    const ok = await confirmModal({
        title: 'Delete imported folders',
        message:
            `Delete ${ids.length} imported folder batch${ids.length === 1 ? '' : 'es'}?\n\n` +
            'This removes batch records plus any items and snapshots they exclusively own. Files on disk are not touched.',
        okLabel: 'Delete',
        cancelLabel: 'Cancel',
        danger: true,
    });
    if (!ok) return;
    const { deleted, failed } = await deleteSequential(ids, (id) => bankApi.deleteImportBatch(id));
    for (const id of deleted) state.selectedImportBatchIds.delete(id);
    if (deleted.includes(state.activeImportBatchId)) state.activeImportBatchId = null;
    reportDelete('import batch', 'import batches', deleted.length, failed.length);
    if (typeof onReload === 'function') await onReload();
    else setState({});
}

async function deleteSequential(ids, deleteOne) {
    const deleted = [];
    const failed = [];
    for (const id of ids) {
        try {
            await deleteOne(id);
            deleted.push(id);
        } catch (error) {
            failed.push({ id, error });
        }
    }
    return { deleted, failed };
}

function reportDelete(noun, plural, deleted, failed) {
    if (deleted > 0) {
        toast(`Deleted ${deleted} ${deleted === 1 ? noun : plural}`, failed > 0 ? 'info' : 'success');
    }
    if (failed > 0) {
        toast(`Delete failed for ${failed} ${failed === 1 ? noun : plural}`, 'error');
    }
}

function setButtonLabel(button, label) {
    const spans = button.querySelectorAll('span');
    if (spans.length === 0) {
        button.textContent = label;
        return;
    }
    spans[spans.length - 1].textContent = label;
}
