// Left-hand sidebar. Pure rendering against state - clicks update
// state.activeSidebar and nudge filters as appropriate. Kept dumb: the
// actual data refresh is driven by bank-main's subscription.

import { state, setState, setFilter, clearAllSelections } from './bank-state.js';

const ENTRIES = [
    { id: 'all',            label: 'All Items',       icon: 'library_music',       hash: '#items' },
    { id: 'snapshots',      label: 'Snapshots',       icon: 'bookmarks',           hash: '#snapshots' },
    { id: 'folder',          label: 'Imported Folders',  icon: 'folder_open',         hash: '#files' },
    { id: 'related',        label: 'Related Groups',  icon: 'hub',                 hash: '#related' },
    { id: 'duplicates',     label: 'Duplicates',      icon: 'content_copy',        hash: '#duplicates' },
    { id: 'favorites',      label: 'Favorites',       icon: 'star',                hash: '#favorites' },
    { id: 'needs-review',   label: 'Needs Review',    icon: 'rule',                hash: '#needs-review' },
    { id: 'failed-imports', label: 'Failed Imports',  icon: 'report',              hash: '#failed-imports' },
];

// Applies the right filter slice for each sidebar target. Keeps the
// item-filter behavior predictable: switching sections never silently
// preserves a conflicting flag from the previous section.
export function applySidebarFilters(id) {
    const patch = {
        favorite: undefined,
        needs_review: false,
        failed_imports_only: false,
        duplicate_only: false,
        related_only: false,
    };
    if (id === 'favorites')      patch.favorite = true;
    if (id === 'needs-review')   patch.needs_review = true;
    if (id === 'duplicates')     patch.duplicate_only = true;
    if (id === 'related')        patch.related_only = true;
    setFilter(patch);
}

export function renderSidebar(root) {
    root.textContent = '';
    const header = document.createElement('div');
    header.className = 'text-[0.65rem] font-black text-on-surface-variant tracking-[0.12em] uppercase px-2 pt-2 pb-3 opacity-70';
    header.textContent = 'LIBRARY';
    root.appendChild(header);

    for (const entry of ENTRIES) {
        root.appendChild(renderItem(entry));
    }

    // Render a small status footer with counts so the user sees at a glance
    // whether the catalog is empty.
    const footer = document.createElement('div');
    footer.className = 'mt-auto px-2 py-3 text-[0.65rem] font-mono text-on-surface-variant opacity-60';
    const totalItems = state.libraryItems.length;
    const totalSnaps = state.snapshots.length;
    footer.textContent = `${totalItems} item(s) · ${totalSnaps} snapshot(s)`;
    root.appendChild(footer);
}

function renderItem(entry) {
    const el = document.createElement('div');
    el.className = 'bank-sidebar-item';
    if (state.activeSidebar === entry.id) el.classList.add('is-active');
    el.setAttribute('role', 'button');
    el.tabIndex = 0;
    el.dataset.id = entry.id;

    const icon = document.createElement('span');
    icon.className = 'material-symbols-outlined';
    icon.textContent = entry.icon;
    el.appendChild(icon);

    const label = document.createElement('span');
    label.textContent = entry.label;
    el.appendChild(label);

    const count = document.createElement('span');
    count.className = 'bank-sidebar-count';
    count.textContent = countForEntry(entry.id);
    el.appendChild(count);

    const activate = () => {
        if (state.activeSidebar === entry.id) return;
        clearAllSelections();
        // Leaving the Snapshots view discards its transient detail state so
        // that re-entering the view starts from the card list again.
        const patch = { activeSidebar: entry.id };
        if (entry.id !== 'snapshots') {
            patch.activeSnapshotId = null;
            patch.snapshotDetail = null;
            patch.activeSnapshotSlot = null;
        }
        patch.transientItemIds = null;
        setState(patch);
        applySidebarFilters(entry.id);
        if (entry.hash) {
            try { history.replaceState(null, '', entry.hash); } catch { /* ignore */ }
        }
    };
    el.addEventListener('click', activate);
    el.addEventListener('keydown', (ev) => {
        if (ev.key === 'Enter' || ev.key === ' ') { ev.preventDefault(); activate(); }
    });
    return el;
}

function countForEntry(id) {
    switch (id) {
        case 'all':            return String(state.libraryItems.length);
        case 'snapshots':      return String(state.snapshots.length);
        case 'favorites':      return String(state.libraryItems.filter((i) => i.favorite).length);
        case 'needs-review':   return String(state.libraryItems.filter((i) => i.analysis_status === 'needsreview').length);
        case 'failed-imports': return String((state.importBatches || []).reduce((sum, batch) => sum + (batch.failed || 0), 0));
        case 'duplicates':     return String(state.duplicates?.clusters?.length || 0);
        case 'related':        return String(state.related?.groups?.length || 0);
        case 'folder':          return String(state.importBatches?.length || 0);
        default: return '';
    }
}
