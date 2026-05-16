// Shared "no items" panel rendered by bank-cards and bank-table when the
// current view has nothing to show. Splits the two distinct reasons for
// emptiness so users aren't left staring at "NO ITEMS / scan a folder" when
// the real cause is a stale filter hiding 260 real rows:
//
//   1. The library is genuinely empty → show the onboarding hint.
//   2. The library has items but the filter excludes them all → show a
//      filter summary plus a one-click CLEAR button that calls resetFilter().
//
// Active-filter summarization is intentionally verbose (comma-listing each
// non-default field) so users recognise exactly what's in effect - e.g.
// "tag: lydian · scale: phrygian · favorites only". It has to match what
// bank-api.js actually forwards to the backend.

import { state, resetFilter, defaultFilter } from './bank-state.js';
import { bankButton } from './bank-buttons.js';

const ICON_LIBRARY_EMPTY = 'inbox';
const ICON_FILTER_EMPTY  = 'filter_alt_off';

/**
 * Build the empty-state panel for a given view.
 * @param {'cards' | 'table'} variant  Chooses the onboarding copy + heading.
 */
export function renderEmptyPanel(variant = 'cards') {
    const wrap = document.createElement('div');
    wrap.className = 'bank-empty';

    const libraryTotal = Array.isArray(state.libraryItems) ? state.libraryItems.length : 0;
    const activeChips = activeFilterChips(state.filter, state.searchQuery);

    if (libraryTotal > 0 && activeChips.length > 0) {
        buildFilteredEmpty(wrap, libraryTotal, activeChips, variant);
    } else {
        buildLibraryEmpty(wrap, variant);
    }
    return wrap;
}

function buildLibraryEmpty(wrap, variant) {
    const icon = document.createElement('span');
    icon.className = 'material-symbols-outlined';
    icon.textContent = ICON_LIBRARY_EMPTY;
    wrap.appendChild(icon);

    const title = document.createElement('div');
    title.className = 'text-base font-black tracking-widest';
    title.textContent = variant === 'table' ? 'NO ROWS' : 'NO ITEMS';
    wrap.appendChild(title);

    const hint = document.createElement('div');
    hint.className = 'text-xs font-mono opacity-70';
    hint.textContent = 'Scan a folder, import files, or create a snapshot from the toolbar to populate the library.';
    wrap.appendChild(hint);
}

function buildFilteredEmpty(wrap, libraryTotal, activeChips, variant) {
    const icon = document.createElement('span');
    icon.className = 'material-symbols-outlined';
    icon.textContent = ICON_FILTER_EMPTY;
    wrap.appendChild(icon);

    const title = document.createElement('div');
    title.className = 'text-base font-black tracking-widest';
    title.textContent = variant === 'table' ? 'NO ROWS MATCH FILTER' : 'NO ITEMS MATCH FILTER';
    wrap.appendChild(title);

    const hint = document.createElement('div');
    hint.className = 'text-xs font-mono opacity-70';
    hint.textContent = `0 of ${libraryTotal} library items match the current filter.`;
    wrap.appendChild(hint);

    const chipRow = document.createElement('div');
    chipRow.className = 'bank-empty-chips';
    for (const text of activeChips) {
        const chip = document.createElement('span');
        chip.className = 'bank-empty-chip';
        chip.textContent = text;
        chipRow.appendChild(chip);
    }
    wrap.appendChild(chipRow);

    const btn = bankButton({
        icon: 'filter_alt_off',
        label: 'CLEAR FILTERS',
        className: 'tactile-button bank-empty-reset',
        onClick: () => {
        resetFilter();
        // Toolbar is not re-rendered by the global subscription, so sync
        // the visible search input directly - otherwise users still see
        // their stale query after the filter reset fires.
        const search = document.getElementById('bank-search-input');
        if (search) search.value = '';
        },
    });
    wrap.appendChild(btn);
}

/**
 * Summarize filter differences from the default. Returns user-readable
 * chips in the same order as the filter panel for familiarity.
 */
function activeFilterChips(filter, searchQuery) {
    const out = [];
    if (!filter) return out;
    const def = defaultFilter();

    const q = (searchQuery || '').trim() || (filter.search || '').trim();
    if (q) out.push(`search: ${q}`);
    if (filter.format)       out.push(`format: ${filter.format}`);
    if (filter.source_kind)  out.push(`source: ${filter.source_kind}`);
    if (filter.favorite)     out.push('favorites only');
    if (filter.archived !== def.archived) out.push(filter.archived ? 'include archived' : 'archived hidden');
    if (filter.duplicate_only)     out.push('duplicates only');
    if (filter.related_only)       out.push('related only');
    if (filter.failed_imports_only) out.push('failed imports only');
    if (filter.needs_review)       out.push('needs review only');
    if (filter.snapshot_id)  out.push(`snapshot: ${shortId(filter.snapshot_id)}`);
    if (filter.slot_key)     out.push(`slot: ${filter.slot_key}`);
    if (filter.scale)        out.push(`scale: ${filter.scale}`);
    if (filter.root)         out.push(`root: ${filter.root}`);
    if (filter.tag)          out.push(`tag: ${filter.tag}`);
    if (filter.date_from)    out.push(`from: ${filter.date_from}`);
    if (filter.date_to)      out.push(`to: ${filter.date_to}`);

    // `archived hidden` is the default state, so don't surface it as a chip
    // unless the user has also applied other restrictions. Otherwise a
    // freshly-opened page with zero items (genuinely empty library) would
    // flip to the "filter" empty state instead of the onboarding copy.
    if (out.length === 1 && out[0] === 'archived hidden') return [];
    return out;
}

function shortId(id) {
    return (id && id.length > 10) ? `${id.slice(0, 10)}…` : id;
}
