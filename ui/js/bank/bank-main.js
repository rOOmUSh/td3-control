// Entry point for /bank.html. Wires the sidebar, toolbar, view (cards/
// table), and drawer together; subscribes each to the shared state; owns
// the data-reload lifecycle.

import {
    state, subscribe, setState, restoreFilters, persistFilters, defaultFilter,
    setFocused, clearSelection, selectAllVisible,
} from './bank-state.js';
import { bankApi } from './bank-api.js';
import { toast } from './bank-toast.js';
import { renderSidebar, applySidebarFilters } from './bank-sidebar.js';
import { renderToolbar } from './bank-toolbar.js';
import { renderCards } from './bank-cards.js';
import { renderTable } from './bank-table.js';
import { renderDrawer } from './bank-drawer.js';
import * as bankSnapshots from './bank-snapshots.js';
import * as bankIngest from './bank-ingest.js';
import * as bankRelated from './bank-related.js';
import { renderDuplicates } from './bank-duplicates.js';
import { decorateItems } from './bank-derived.js';
import { hydratePlayingState } from './bank-play.js';
import { initFooter } from './bank-footer.js';
import { loadAppConfig } from '../app-config.js';

const SECONDARY_FEED_TIMEOUT_MS = 4000;

const SIDEBAR_FROM_HASH = {
    '#items': 'all',
    '#all': 'all',
    '#snapshots': 'snapshots',
    '#files': 'folder',
    '#related': 'related',
    '#duplicates': 'duplicates',
    '#favorites': 'favorites',
    '#needs-review': 'needs-review',
    '#failed-imports': 'failed-imports',
};

document.addEventListener('DOMContentLoaded', async () => {
    // Fetch TD3_CONFIG.env before anything that reads uiDefaultBpm runs
    // (bank-footer's BPM knob is the first consumer). Fire-and-wait: if the
    // /api/config/env endpoint is down, bank-play falls back to its
    // hardcoded default and the footer still comes up.
    await loadAppConfig();
    restoreFilters();
    applyHash();

    const sidebarRoot = document.getElementById('bank-sidebar');
    const toolbarRoot = document.getElementById('bank-toolbar');
    const viewRoot    = document.getElementById('bank-view');
    const drawerRoot  = document.getElementById('bank-drawer');
    const subtitle    = document.getElementById('bank-subtitle');

    if (!sidebarRoot || !toolbarRoot || !viewRoot || !drawerRoot) {
        console.error('bank-main: required DOM nodes missing');
        return;
    }

    // Initial static render (re-rendered by the subscription below).
    renderToolbar(toolbarRoot, { onReload: reloadAll });
    renderSidebar(sidebarRoot);
    renderView(viewRoot);
    renderDrawer(drawerRoot, { onReload: reloadAll });

    // Re-render whenever state changes. We don't try to diff - these views
    // are small enough that full re-render is fine and keeps the module
    // logic simple + predictable.
    let lastSidebar = state.activeSidebar;
    let lastFilterKey = JSON.stringify(state.filter);
    subscribe(() => {
        // Reset transient ingest-view drill-in state when the user leaves
        // the 'folder' section so coming back starts from the batch list.
        if (state.activeSidebar !== lastSidebar) {
            if (lastSidebar === 'folder') bankIngest.resetView();
            lastSidebar = state.activeSidebar;
        }
        // Refetch items whenever the effective filter changes. Sidebar
        // clicks use history.replaceState (no hashchange event) and then
        // call setFilter(), so we watch the filter itself rather than the
        // sidebar id. This also covers search input, format/tag panels,
        // and any other setFilter callers.
        const filterKey = JSON.stringify(state.filter);
        if (filterKey !== lastFilterKey) {
            lastFilterKey = filterKey;
            reloadItems();
        }
        // Lightweight: we only rebuild the relevant region on each tick.
        renderSidebar(sidebarRoot);
        renderView(viewRoot);
        renderDrawer(drawerRoot, { onReload: reloadAll });
        if (subtitle) subtitle.textContent = labelFor(state.activeSidebar);
        persistFilters();
    });

    // Global keyboard shortcuts.
    document.addEventListener('keydown', (ev) => {
        // Ignore when typing in an input/textarea.
        const tag = (ev.target && ev.target.tagName) || '';
        const isTyping = tag === 'INPUT' || tag === 'TEXTAREA' || ev.target?.isContentEditable;
        if (!isTyping && ev.key === '/') {
            const s = document.getElementById('bank-search-input');
            if (s) { ev.preventDefault(); s.focus(); s.select(); }
        }
        if (ev.key === 'Escape') {
            if (state.focusedId) { setFocused(null); return; }
            // Close any open modal.
            const md = document.querySelector('.bank-modal-backdrop');
            if (md) md.remove();
        }
        if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === 'a' && !isTyping) {
            ev.preventDefault();
            selectAllVisible(state.items.map((i) => i.item_id));
        }
    });

    // Click outside the drawer closes it. Clicks on a card/row or inside a
    // modal must not close - those handlers either retarget focusedId or
    // belong to an overlay we don't own. Bubble phase so card handlers run
    // first; if the click landed on a focus-setter we bail out before
    // calling setFocused(null).
    document.addEventListener('click', (ev) => {
        if (!state.focusedId) return;
        const t = ev.target;
        if (!(t instanceof Element)) return;
        if (t.closest('.bank-drawer')) return;
        if (t.closest('.bank-card, .bank-table tbody tr')) return;
        if (t.closest('.bank-modal-card, .bank-modal-backdrop')) return;
        setFocused(null);
    });

    window.addEventListener('hashchange', () => {
        applyHash();
        applySidebarFilters(state.activeSidebar);
        reloadItems();
    });

    reloadAll();
    // Sync the "which item is auditioning" indicator from the server so the
    // play button on the currently-live item repaints into its stop state
    // even on a fresh page load.
    void hydratePlayingState();
    // Footer owns its own MIDI-status poll and BPM knob; nothing to subscribe
    // from bank-state because both channels live outside the filter bus.
    initFooter();
});

function applyHash() {
    const hash = window.location.hash || '';
    if (hash && SIDEBAR_FROM_HASH[hash]) {
        state.activeSidebar = SIDEBAR_FROM_HASH[hash];
    }
}

function labelFor(id) {
    switch (id) {
        case 'all':            return 'ALL ITEMS';
        case 'snapshots':      return 'SNAPSHOTS';
        case 'folder':          return 'IMPORTED FOLDERS';
        case 'related':        return 'RELATED GROUPS';
        case 'duplicates':     return 'DUPLICATES';
        case 'favorites':      return 'FAVORITES';
        case 'needs-review':   return 'NEEDS REVIEW';
        case 'failed-imports': return 'FAILED IMPORTS';
        default: return '';
    }
}

function renderView(viewRoot) {
    if (state.activeSidebar === 'snapshots') {
        // Full snapshot browser lives in bank-snapshots.js.
        bankSnapshots.render(viewRoot, { onRefreshLibrary: reloadAll });
        return;
    }
    if (state.activeSidebar === 'folder') {
        // Ingest / ImportBatch browser.
        bankIngest.render(viewRoot, 'batches');
        return;
    }
    if (state.activeSidebar === 'failed-imports') {
        bankIngest.render(viewRoot, 'failed');
        return;
    }
    if (state.activeSidebar === 'duplicates') {
        renderDuplicates(viewRoot, { onReload: reloadAll });
        return;
    }
    if (state.activeSidebar === 'related') {
        bankRelated.render(viewRoot);
        return;
    }
    if (state.viewMode === 'table') {
        renderTable(viewRoot, { onReload: reloadAll });
    } else {
        renderCards(viewRoot, { onReload: reloadAll });
    }
}

function applyTransientItems(items) {
    if (!state.transientItemIds || state.transientItemIds.size === 0) return items;
    return items.filter((item) => state.transientItemIds.has(item.item_id));
}

function withTimeout(promise, ms, label) {
    return new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error(`${label} timed out after ${ms}ms`)), ms);
        promise.then(
            (value) => {
                clearTimeout(timer);
                resolve(value);
            },
            (error) => {
                clearTimeout(timer);
                reject(error);
            },
        );
    });
}

async function reloadDerivedFeeds(baseItems, baseLibraryItems) {
    const [relatedResult, duplicatesResult] = await Promise.allSettled([
        withTimeout(bankApi.listRelated(), SECONDARY_FEED_TIMEOUT_MS, 'related'),
        withTimeout(bankApi.listDuplicates(), SECONDARY_FEED_TIMEOUT_MS, 'duplicates'),
    ]);

    const related = relatedResult.status === 'fulfilled'
        ? relatedResult.value
        : (state.related || { groups: [], relations: [] });
    const duplicates = duplicatesResult.status === 'fulfilled'
        ? duplicatesResult.value
        : (state.duplicates || { clusters: [] });

    if (relatedResult.status === 'rejected') {
        console.warn('bank-main: related feed unavailable during reloadAll:', relatedResult.reason);
    }
    if (duplicatesResult.status === 'rejected') {
        console.warn('bank-main: duplicates feed unavailable during reloadAll:', duplicatesResult.reason);
    }

    setState({
        items: applyTransientItems(decorateItems(baseItems, { related, duplicates })),
        libraryItems: decorateItems(baseLibraryItems, { related, duplicates }),
        related,
        duplicates,
    });
}

async function reloadSnapshots() {
    try {
        const snapshots = await bankApi.listSnapshots();
        setState({ snapshots: snapshots.snapshots || [] });
    } catch (e) {
        toast(`Snapshot reload failed: ${e.message}`, 'error');
    }
}

async function reloadAll() {
    try {
        setState({ loading: true, lastError: null });
        const [items, allItems, tags, snapshots, batches] = await Promise.all([
            bankApi.listItems(state.filter),
            bankApi.listItems(defaultFilter()),
            bankApi.listTags(),
            bankApi.listSnapshots(),
            bankApi.listImportBatches(),
        ]);
        const baseItems = items.items || [];
        const baseLibraryItems = allItems.items || [];
        setState({
            items: applyTransientItems(decorateItems(baseItems, {
                related: state.related,
                duplicates: state.duplicates,
            })),
            libraryItems: decorateItems(baseLibraryItems, {
                related: state.related,
                duplicates: state.duplicates,
            }),
            tags: tags.tags || [],
            snapshots: snapshots.snapshots || [],
            importBatches: batches.batches || [],
            loading: false,
        });
        void reloadDerivedFeeds(baseItems, baseLibraryItems);
    } catch (e) {
        setState({ loading: false, lastError: e.message });
        toast(`Load failed: ${e.message}`, 'error');
    }
}

async function reloadItems() {
    try {
        const items = await bankApi.listItems(state.filter);
        const decoratedItems = decorateItems(items.items || [], {
            related: state.related,
            duplicates: state.duplicates,
        });
        setState({
            items: applyTransientItems(decoratedItems),
        });
    } catch (e) {
        toast(`Reload failed: ${e.message}`, 'error');
    }
}

// The Related view is fully owned by bank-related.js; the sidebar-driven
// dispatch above is the only wiring required here.
