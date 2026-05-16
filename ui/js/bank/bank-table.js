// Power-user table view. Multi-column sort, keyboard
// navigation, bulk actions in a sticky bar when items are selected.

import {
    state, toggleSelection, clearSelection, setFocused,
    setState, selectAllVisible,
} from './bank-state.js';
import { bankApi } from './bank-api.js';
import { toast } from './bank-toast.js';
import { resolveTagKind } from './bank-cards.js';
import { openModal, confirmModal } from './bank-modal.js';
import { makePlayButton } from './bank-play.js';
import { renderEmptyPanel } from './bank-empty.js';
import { bankButton, menuButton } from './bank-buttons.js';
import { TD3_CHECKBOX } from '../shared/button-classes.js';

const COLUMNS = [
    { key: 'display_name',   label: 'Name'     },
    { key: 'source_kind',    label: 'Source'   },
    { key: 'format',         label: 'Format'   },
    { key: 'snapshot_name',  label: 'Snapshot' },
    { key: 'slot_key',       label: 'Slot'     },
    { key: 'scale_name',     label: 'Scale'    },
    { key: 'root_note',      label: 'Root'     },
    { key: 'tags',           label: 'Tags'     },
    { key: 'favorite',       label: 'Fav'      },
    { key: 'duplicate_status', label: 'Dup'    },
    { key: 'updated_at',     label: 'Updated'  },
];

export function renderTable(root, { onReload }) {
    root.textContent = '';
    if (state.items.length === 0) {
        root.appendChild(renderEmpty());
        return;
    }

    const rows = sortItems(state.items, state.sort);
    const ids = rows.map((r) => r.item_id);

    const tableWrap = document.createElement('div');
    tableWrap.style.overflowX = 'auto';

    const table = document.createElement('table');
    table.className = 'bank-table' + (state.dense ? ' dense' : '');

    const thead = document.createElement('thead');
    const tr = document.createElement('tr');
    // Leading checkbox column
    const thSel = document.createElement('th');
    thSel.style.width = '32px';
    const allBox = document.createElement('input');
    allBox.type = 'checkbox';
    allBox.className = TD3_CHECKBOX;
    allBox.title = 'Select all visible';
    allBox.checked = ids.length > 0 && ids.every((id) => state.selectedIds.has(id));
    allBox.addEventListener('click', (ev) => {
        ev.stopPropagation();
        if (allBox.checked) selectAllVisible(ids);
        else clearSelection();
    });
    thSel.appendChild(allBox);
    tr.appendChild(thSel);

    // Dedicated Play column - 32px wide, no sort. Sits right after the
    // selection checkbox so the audition button is always in the same
    // horizontal position regardless of what columns the table is showing.
    const thPlay = document.createElement('th');
    thPlay.style.width = '32px';
    thPlay.title = 'Play on device';
    tr.appendChild(thPlay);

    for (const col of COLUMNS) {
        const th = document.createElement('th');
        th.textContent = col.label;
        const sortEntry = state.sort.find((s) => s.key === col.key);
        if (sortEntry) {
            const ind = document.createElement('span');
            ind.className = 'sort-indicator';
            ind.textContent = sortEntry.dir === 'asc' ? '▲' : '▼';
            th.appendChild(ind);
        }
        th.addEventListener('click', (ev) => {
            const nextSort = computeNextSort(state.sort, col.key, ev.shiftKey);
            setState({ sort: nextSort });
        });
        tr.appendChild(th);
    }
    // Kebab placeholder column
    const thKebab = document.createElement('th');
    thKebab.style.width = '32px';
    tr.appendChild(thKebab);
    thead.appendChild(tr);
    table.appendChild(thead);

    const tbody = document.createElement('tbody');
    rows.forEach((item, index) => {
        tbody.appendChild(renderRow(item, index, ids, onReload));
    });
    table.appendChild(tbody);
    tableWrap.appendChild(table);
    root.appendChild(tableWrap);

    if (state.selectedIds.size > 0) {
        root.appendChild(renderBulkBar(Array.from(state.selectedIds), onReload));
    }

    // Keyboard navigation - attach once to root so the user can arrow around
    // even when they clicked on a cell inside a row.
    root.tabIndex = 0;
    root.addEventListener('keydown', (ev) => handleKey(ev, ids));
}

function renderRow(item, index, ids, onReload) {
    const tr = document.createElement('tr');
    tr.dataset.id = item.item_id;
    if (state.selectedIds.has(item.item_id)) tr.classList.add('selected');
    if (state.focusedId === item.item_id) tr.classList.add('focused');

    // Checkbox
    const tdSel = document.createElement('td');
    const box = document.createElement('input');
    box.type = 'checkbox';
    box.className = TD3_CHECKBOX;
    box.checked = state.selectedIds.has(item.item_id);
    box.addEventListener('click', (ev) => {
        ev.stopPropagation();
        toggleSelection(item.item_id, { index, ids, shiftKey: ev.shiftKey });
    });
    tdSel.appendChild(box);
    tr.appendChild(tdSel);

    const tdPlay = document.createElement('td');
    tdPlay.style.width = '32px';
    tdPlay.appendChild(makePlayButton(item.item_id, { size: 'sm' }));
    tr.appendChild(tdPlay);

    tr.appendChild(cellText(item.display_name || '(unnamed)'));
    tr.appendChild(cellSource(item));
    tr.appendChild(cellText(item.format || '-'));
    tr.appendChild(cellText(item.snapshot_name || '-'));
    tr.appendChild(cellText(item.slot_key || '-'));
    tr.appendChild(cellText(item.scale_name || '-'));
    tr.appendChild(cellText(item.root_note || '-'));
    tr.appendChild(cellTags(item.tags));
    tr.appendChild(cellFav(item, onReload));
    tr.appendChild(cellDup(item));
    tr.appendChild(cellText(formatLocalTimestamp(item.updated_at || item.created_at)));

    const tdKebab = document.createElement('td');
    const kebab = document.createElement('span');
    kebab.className = 'material-symbols-outlined';
    kebab.textContent = 'more_vert';
    kebab.style.cursor = 'pointer';
    kebab.addEventListener('click', (ev) => {
        ev.stopPropagation();
        openKebab(ev.clientX, ev.clientY, item);
    });
    tdKebab.appendChild(kebab);
    tr.appendChild(tdKebab);

    tr.addEventListener('click', (ev) => {
        toggleSelection(item.item_id, { index, ids, shiftKey: ev.shiftKey });
    });
    tr.addEventListener('dblclick', () => setFocused(item.item_id));
    return tr;
}

function cellText(text) {
    const td = document.createElement('td');
    td.textContent = text ?? '-';
    td.title = td.textContent;
    return td;
}

function cellSource(item) {
    const td = document.createElement('td');
    const el = document.createElement('span');
    el.className = `source-badge source-badge-${String(item.source_kind || 'file').toLowerCase()}`;
    el.textContent = item.source_label || item.source_kind || '-';
    td.appendChild(el);
    return td;
}

function cellTags(tags) {
    const td = document.createElement('td');
    if (!Array.isArray(tags) || tags.length === 0) { td.textContent = '-'; return td; }
    for (const t of tags.slice(0, 4)) {
        const kind = resolveTagKind(t);
        const pill = document.createElement('span');
        pill.className = `bank-tag-pill kind-${kind}`;
        pill.textContent = t;
        pill.style.marginRight = '0.25rem';
        td.appendChild(pill);
    }
    if (tags.length > 4) {
        const more = document.createElement('span');
        more.className = 'bank-tag-pill';
        more.textContent = `+${tags.length - 4}`;
        td.appendChild(more);
    }
    return td;
}

function cellFav(item, onReload) {
    const td = document.createElement('td');
    const star = document.createElement('span');
    star.className = 'material-symbols-outlined fav-marker' + (item.favorite ? '' : ' off');
    star.textContent = item.favorite ? 'star' : 'star_outline';
    star.style.cursor = 'pointer';
    star.addEventListener('click', async (ev) => {
        ev.stopPropagation();
        try { await bankApi.toggleFavorite(item.item_id, !item.favorite); onReload?.(); }
        catch (e) { toast(e.message, 'error'); }
    });
    td.appendChild(star);
    return td;
}

function cellDup(item) {
    const td = document.createElement('td');
    const s = item.duplicate_status;
    if (s === 'exactduplicate') td.textContent = '⚠ exact';
    else if (s === 'nearduplicate') td.textContent = '~ near';
    else td.textContent = '-';
    return td;
}

function renderEmpty() {
    return renderEmptyPanel('table');
}

function renderBulkBar(selectedIds, onReload) {
    const bar = document.createElement('div');
    bar.className = 'bank-bulk-bar';

    const count = document.createElement('span');
    count.className = 'text-xs font-mono';
    count.textContent = `${selectedIds.length} selected`;
    bar.appendChild(count);

    bar.appendChild(bulkBtn('label', 'BULK TAG', () => {
        openBulkTagModal(selectedIds, onReload);
    }));

    bar.appendChild(bulkBtn('star', 'FAV', async () => {
        try {
            await Promise.all(selectedIds.map((id) => bankApi.toggleFavorite(id, true)));
            toast('Favorited', 'success');
            onReload?.();
        } catch (e) { toast(e.message, 'error'); }
    }));

    bar.appendChild(bulkBtn('archive', 'ARCHIVE', async () => {
        const ok = await confirmModal({
            title: 'Archive items',
            message: `Archive ${selectedIds.length} items?`,
            okLabel: 'Archive',
            cancelLabel: 'Cancel',
        });
        if (!ok) return;
        try {
            await Promise.all(selectedIds.map((id) => bankApi.setArchived(id, true)));
            toast('Archived', 'success');
            onReload?.();
        } catch (e) { toast(e.message, 'error'); }
    }));

    bar.appendChild(bulkBtn('photo_library', 'ADD TO SNAPSHOT', () => {
        toast('Add to Snapshot is not available from the table bulk bar.', 'info');
    }));

    bar.appendChild(bulkBtn('merge', 'QUEUE FOR MERGE', () => {
        toast('Merge queue is not available from the table bulk bar.', 'info');
    }));

    bar.appendChild(bulkBtn('deselect', 'CLEAR', () => clearSelection()));

    return bar;
}

function bulkBtn(iconName, label, onClick) {
    return bankButton({ icon: iconName, label, onClick });
}

function openKebab(x, y, item) {
    // Close existing kebab(s) first.
    document.querySelectorAll('.bank-kebab').forEach((el) => el.remove());
    const menu = document.createElement('div');
    menu.className = 'bank-kebab';
    menu.style.left = `${Math.min(x, window.innerWidth - 220)}px`;
    menu.style.top  = `${Math.min(y, window.innerHeight - 180)}px`;

    menu.appendChild(kebabItem('info', 'Open details', () => { setFocused(item.item_id); menu.remove(); }));
    menu.appendChild(kebabItem('link',  'Copy Path',    () => { copyToClipboard(item.source_path || ''); menu.remove(); }));
    menu.appendChild(kebabItem('dataset', 'Copy Slot',  () => { copyToClipboard(item.slot_key || ''); menu.remove(); }));
    menu.appendChild(kebabItem('tag', 'Copy ID',        () => { copyToClipboard(item.item_id); menu.remove(); }));

    document.body.appendChild(menu);
    const outside = (ev) => {
        if (!menu.contains(ev.target)) { menu.remove(); document.removeEventListener('mousedown', outside); }
    };
    setTimeout(() => document.addEventListener('mousedown', outside), 0);
}

function kebabItem(iconName, label, onClick) {
    return menuButton(iconName, label, onClick);
}

async function copyToClipboard(text) {
    try { await navigator.clipboard.writeText(text || ''); toast('Copied to clipboard', 'success'); }
    catch { toast('Clipboard unavailable', 'error'); }
}

function computeNextSort(current, key, shift) {
    const existing = current.find((s) => s.key === key);
    if (shift) {
        if (existing) {
            return current.map((s) => s.key === key ? { key, dir: s.dir === 'asc' ? 'desc' : 'asc' } : s);
        }
        return [...current, { key, dir: 'asc' }];
    }
    if (existing) {
        return [{ key, dir: existing.dir === 'asc' ? 'desc' : 'asc' }];
    }
    return [{ key, dir: 'asc' }];
}

function sortItems(items, sort) {
    if (!sort || sort.length === 0) return items.slice();
    const copy = items.slice();
    copy.sort((a, b) => {
        for (const s of sort) {
            const av = getSortable(a, s.key);
            const bv = getSortable(b, s.key);
            if (av < bv) return s.dir === 'asc' ? -1 : 1;
            if (av > bv) return s.dir === 'asc' ?  1 : -1;
        }
        return 0;
    });
    return copy;
}

function getSortable(item, key) {
    const v = item[key];
    if (key === 'tags') return Array.isArray(v) ? v.join(',').toLowerCase() : '';
    if (typeof v === 'boolean') return v ? 1 : 0;
    if (v === null || v === undefined) return '';
    return typeof v === 'string' ? v.toLowerCase() : v;
}

function handleKey(ev, ids) {
    if (ids.length === 0) return;
    const current = state.focusedId ? ids.indexOf(state.focusedId) : -1;
    if (ev.key === 'ArrowDown') {
        ev.preventDefault();
        const next = ids[Math.min(current + 1, ids.length - 1)] || ids[0];
        setFocused(next);
    } else if (ev.key === 'ArrowUp') {
        ev.preventDefault();
        const next = ids[Math.max(current - 1, 0)] || ids[0];
        setFocused(next);
    } else if (ev.key === ' ') {
        if (state.focusedId) {
            ev.preventDefault();
            const idx = ids.indexOf(state.focusedId);
            toggleSelection(state.focusedId, { index: idx, ids });
        }
    } else if (ev.key === 'Enter') {
        if (state.focusedId) { ev.preventDefault(); setFocused(state.focusedId); }
    }
}

function formatLocalTimestamp(value) {
    if (!value) return '-';
    const d = parseBankTimestamp(value);
    if (!d || isNaN(d.getTime())) return value;
    const pad = (n) => String(n).padStart(2, '0');
    return (
        `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ` +
        `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
    );
}

function parseBankTimestamp(value) {
    const text = String(value).trim();
    let m = /^(\d{4})-(\d{2})-(\d{2})_(\d{2})-(\d{2})-(\d{2})(Z)?$/.exec(text);
    if (m) {
        const args = m.slice(1, 7).map((x) => Number(x));
        if (m[7]) return new Date(Date.UTC(args[0], args[1] - 1, args[2], args[3], args[4], args[5]));
        return new Date(args[0], args[1] - 1, args[2], args[3], args[4], args[5]);
    }
    m = /^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})Z?$/.exec(text);
    if (m) {
        const args = m.slice(1, 7).map((x) => Number(x));
        return new Date(Date.UTC(args[0], args[1] - 1, args[2], args[3], args[4], args[5]));
    }
    return new Date(text);
}

/**
 * Bulk Tag modal - two multi-selects driven by the cached tag catalog, plus
 * a "custom" input that lets the user add brand-new labels the backend will
 * create as user-kind tags.
 */
function openBulkTagModal(itemIds, onReload) {
    const body = document.createElement('div');

    const count = document.createElement('div');
    count.className = 'text-xs font-mono opacity-75 mb-2';
    count.textContent = `${itemIds.length} item${itemIds.length === 1 ? '' : 's'} selected`;
    body.appendChild(count);

    const addSelect = buildTagMultiSelect('Add tags', state.tags || []);
    body.appendChild(addSelect.label);

    const addCustomLabel = document.createElement('label');
    const addCustomSpan = document.createElement('span');
    addCustomSpan.textContent = 'Extra new tags (comma-separated)';
    addCustomLabel.appendChild(addCustomSpan);
    const addCustomInput = document.createElement('input');
    addCustomInput.type = 'text';
    addCustomInput.placeholder = 'mood:dark, genre:acid';
    addCustomInput.autocomplete = 'off';
    addCustomInput.spellcheck = false;
    addCustomLabel.appendChild(addCustomInput);
    body.appendChild(addCustomLabel);

    const removeSelect = buildTagMultiSelect('Remove tags', state.tags || []);
    body.appendChild(removeSelect.label);

    openModal({
        title: `Bulk Tag (${itemIds.length})`,
        body,
        primaryLabel: 'Apply',
        onPrimary: async () => {
            const add = new Set(addSelect.selected());
            for (const extra of addCustomInput.value.split(',').map((s) => s.trim()).filter(Boolean)) {
                add.add(extra);
            }
            const remove = removeSelect.selected();
            if (add.size === 0 && remove.length === 0) {
                toast('Pick at least one tag to add or remove', 'error');
                throw new Error('no-op');
            }
            const res = await bankApi.bulkTag({
                item_ids: itemIds,
                add: Array.from(add),
                remove,
            });
            toast(
                `Tag update applied to ${itemIds.length} item${itemIds.length === 1 ? '' : 's'}`,
                res?.ok === false ? 'error' : 'success',
            );
            onReload?.();
        },
    });
}

function buildTagMultiSelect(titleText, tags) {
    const label = document.createElement('label');
    const span = document.createElement('span');
    span.textContent = titleText;
    label.appendChild(span);

    const box = document.createElement('div');
    box.className = 'bank-multiselect';
    if (tags.length === 0) {
        const empty = document.createElement('div');
        empty.className = 'text-xs font-mono opacity-70 py-2';
        empty.textContent = '(no tags in catalog yet)';
        box.appendChild(empty);
    }
    const checkboxes = [];
    const sorted = tags.slice().sort((a, b) => (a.label || '').localeCompare(b.label || ''));
    for (const t of sorted) {
        const row = document.createElement('label');
        row.style.flexDirection = 'row';
        row.style.alignItems = 'center';
        row.style.gap = '0.5rem';
        row.style.marginBottom = '0.15rem';
        row.style.textTransform = 'none';
        const cb = document.createElement('input');
        cb.type = 'checkbox';
        cb.className = TD3_CHECKBOX;
        cb.value = t.label;
        row.appendChild(cb);
        const pill = document.createElement('span');
        const kind = String(t.kind || 'user').toLowerCase();
        pill.className = `bank-tag-pill kind-${kind}`;
        pill.textContent = t.label;
        row.appendChild(pill);
        box.appendChild(row);
        checkboxes.push(cb);
    }
    label.appendChild(box);
    return {
        label,
        selected() { return checkboxes.filter((c) => c.checked).map((c) => c.value); },
    };
}
