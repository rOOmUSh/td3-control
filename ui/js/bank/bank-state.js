// Minimal central store for the Bank Management UI.
// Deliberately framework-free: one mutable state object + a pub/sub bus.
// Modules subscribe() once and call setState() to broadcast changes. A
// subset of the state persists to localStorage so filters / view mode
// survive reload.

const STORAGE_FILTERS = 'bank-filters-v1';
const STORAGE_VIEW    = 'bank-view-v1';

export function defaultFilter() {
    return {
        search: '',
        format: undefined,
        source_kind: undefined,
        favorite: undefined,
        archived: false,          // hide archived by default
        duplicate_only: false,
        related_only: false,
        failed_imports_only: false,
        snapshot_id: undefined,
        slot_key: undefined,
        scale: undefined,
        root: undefined,
        tag: undefined,
        date_from: undefined,
        date_to: undefined,
        needs_review: false,
    };
}

export const state = {
    // Data
    items: [],
    libraryItems: [],
    transientItemIds: null,
    snapshots: [],
    tags: [],
    importBatches: [],
    related: { groups: [], relations: [] },
    duplicates: { clusters: [] },

    // Selection + focus
    selectedIds: new Set(),      // multi-select in cards + table
    lastSelectedIndex: -1,       // for shift-click range selection
    focusedId: null,             // drives drawer

    // UI
    activeSidebar: 'all',        // 'all' | 'snapshots' | 'folder' | 'related' | 'duplicates' | 'favorites' | 'needs-review' | 'failed-imports'
    viewMode: 'cards',           // 'cards' | 'table'
    dense: false,
    filter: defaultFilter(),
    searchQuery: '',
    sort: [{ key: 'updated_at', dir: 'desc' }],
    loading: false,
    lastError: null,

    // Snapshots view.
    // When activeSnapshotId is non-null, the Snapshots view renders the
    // 64-slot detail for that snapshot; otherwise it renders the card list.
    activeSnapshotId: null,
    // Cached { snapshot, slots[] } for activeSnapshotId. Refreshed on open.
    snapshotDetail: null,
    // Linked slot for the drawer when a grid cell is clicked.
    activeSnapshotSlot: null,    // { snapshot_id, slot_key, item_id, display_name, empty }
    // Multi-selection of slot cells inside snapshot detail mode. The set
    // holds slot keys like "G1-P1A". Cleared when leaving detail mode
    // (BACK button) or when switching to a different snapshot.
    selectedSnapshotSlots: new Set(),
};

const listeners = new Set();

export function subscribe(fn) {
    listeners.add(fn);
    return () => listeners.delete(fn);
}

export function notify() {
    for (const fn of listeners) {
        try { fn(state); }
        catch (e) { console.error('bank-state listener error:', e); }
    }
}

export function setState(partial) {
    Object.assign(state, partial);
    notify();
}

export function setFilter(patch) {
    state.filter = { ...state.filter, ...patch };
    state.transientItemIds = null;
    persistFilters();
    notify();
}

export function resetFilter() {
    state.filter = defaultFilter();
    state.searchQuery = '';
    state.transientItemIds = null;
    persistFilters();
    notify();
}

export function toggleSelection(id, { index = -1, shiftKey = false, ids = null } = {}) {
    if (shiftKey && state.lastSelectedIndex >= 0 && index >= 0 && ids) {
        // Range select: add everything from lastSelectedIndex..index (inclusive).
        const [lo, hi] = [Math.min(state.lastSelectedIndex, index), Math.max(state.lastSelectedIndex, index)];
        for (let i = lo; i <= hi; i++) {
            const rid = ids[i];
            if (rid) state.selectedIds.add(rid);
        }
    } else {
        if (state.selectedIds.has(id)) state.selectedIds.delete(id);
        else state.selectedIds.add(id);
        if (index >= 0) state.lastSelectedIndex = index;
    }
    notify();
}

export function clearSelection() {
    state.selectedIds.clear();
    state.lastSelectedIndex = -1;
    notify();
}

export function selectAllVisible(ids) {
    for (const id of ids) state.selectedIds.add(id);
    notify();
}

/**
 * Toggle a slot key in the snapshot-detail multi-selection. Called from
 * grid cell clicks. Notifies so the grid re-renders with the updated
 * `.selected` class.
 *
 * @param {string} slotKey  canonical slot key e.g. "G1-P1A"
 */
export function toggleSnapshotSlotSelection(slotKey) {
    if (!slotKey) return;
    if (state.selectedSnapshotSlots.has(slotKey)) {
        state.selectedSnapshotSlots.delete(slotKey);
    } else {
        state.selectedSnapshotSlots.add(slotKey);
    }
    notify();
}

/** Clear the snapshot-detail slot multi-selection. */
export function clearSnapshotSlotSelection() {
    if (state.selectedSnapshotSlots.size === 0) return;
    state.selectedSnapshotSlots.clear();
    notify();
}

export function setFocused(id) {
    state.focusedId = id;
    notify();
}

export function persistFilters() {
    try {
        localStorage.setItem(STORAGE_FILTERS, JSON.stringify({
            filter: state.filter,
            searchQuery: state.searchQuery,
            activeSidebar: state.activeSidebar,
            sort: state.sort,
        }));
        localStorage.setItem(STORAGE_VIEW, JSON.stringify({
            viewMode: state.viewMode,
            dense: state.dense,
        }));
    } catch { /* storage may be full or disabled - non-fatal */ }
}

export function restoreFilters() {
    try {
        const f = localStorage.getItem(STORAGE_FILTERS);
        if (f) {
            const parsed = JSON.parse(f);
            if (parsed && typeof parsed === 'object') {
                state.filter = { ...defaultFilter(), ...(parsed.filter || {}) };
                state.searchQuery = parsed.searchQuery || '';
                state.activeSidebar = parsed.activeSidebar || 'all';
                if (Array.isArray(parsed.sort)) state.sort = parsed.sort;
            }
        }
        const v = localStorage.getItem(STORAGE_VIEW);
        if (v) {
            const parsed = JSON.parse(v);
            if (parsed && typeof parsed === 'object') {
                if (parsed.viewMode === 'cards' || parsed.viewMode === 'table') state.viewMode = parsed.viewMode;
                if (typeof parsed.dense === 'boolean') state.dense = parsed.dense;
            }
        }
    } catch { /* ignore corrupt storage - start fresh */ }
}
