// Card grid view. One card per LibraryItem. All interactive state lives
// on bank-state - this module only draws + wires event handlers.

import { state, toggleSelection, setFocused } from './bank-state.js';
import { bankApi } from './bank-api.js';
import { toast } from './bank-toast.js';
import { confirmModal, promptModal } from './bank-modal.js';
import { makePlayButton } from './bank-play.js';
import { renderEmptyPanel } from './bank-empty.js';
import { bankButton } from './bank-buttons.js';
import { addItemsToControl } from '../shared/add-to-control.js';
import { TD3_CHECKBOX } from '../shared/button-classes.js';

export function renderCards(root, { onReload }) {
    root.textContent = '';
    const items = state.items;
    if (items.length === 0) {
        root.appendChild(renderEmpty());
        return;
    }

    const grid = document.createElement('div');
    grid.style.display = 'grid';
    grid.style.gap = '0.75rem';
    grid.style.gridTemplateColumns = 'repeat(auto-fill, minmax(260px, 1fr))';
    root.appendChild(grid);

    const ids = items.map((i) => i.item_id);
    items.forEach((item, index) => {
        grid.appendChild(renderCard(item, index, ids, onReload));
    });
}

function renderCard(item, index, ids, onReload) {
    const card = document.createElement('div');
    card.className = 'bank-card';
    if (state.selectedIds.has(item.item_id)) card.classList.add('selected');
    card.dataset.id = item.item_id;
    card.tabIndex = 0;

    // Top row: name + source badge
    const topRow = document.createElement('div');
    topRow.style.display = 'flex';
    topRow.style.justifyContent = 'space-between';
    topRow.style.alignItems = 'flex-start';
    topRow.style.gap = '0.5rem';



    const topActions = document.createElement('div');
    topActions.style.display = 'flex';
    topActions.style.alignItems = 'center';
    topActions.style.gap = '0.35rem';
    topActions.appendChild(buildItemSelectionCheckbox(item, index, ids));
    topActions.appendChild(makePlayButton(item.item_id, { size: 'sm' }));
	
	// Hover actions (right side)
    const actions = document.createElement('div');
    actions.className = 'bank-card-actions';
    actions.appendChild(miniAction('open_in_new', 'Open details', (ev) => {
        ev.stopPropagation();
        setFocused(item.item_id);
    }));
    actions.appendChild(miniAction('label', 'Add tag', async (ev) => {
        ev.stopPropagation();
        const label = await promptModal({
            title: 'Add tag',
            label: `Add tag to "${item.display_name}":`,
            okLabel: 'Add',
        });
        if (!label || !label.trim()) return;
        try {
            await bankApi.addTag(item.item_id, label.trim());
            toast('Tag added', 'success');
            onReload?.();
        } catch (e) { toast(e.message, 'error'); }
    }));
    topActions.appendChild(actions);

    const fav = document.createElement('span');
    fav.className = 'material-symbols-outlined fav-marker' + (item.favorite ? '' : ' off');
    fav.textContent = item.favorite ? 'star' : 'star_outline';
    fav.title = item.favorite ? 'Unfavorite' : 'Favorite';
    fav.addEventListener('click', async (ev) => {
        ev.stopPropagation();
        try {
            await bankApi.toggleFavorite(item.item_id, !item.favorite);
            onReload?.();
        } catch (e) { toast(e.message, 'error'); }
    });
    topActions.appendChild(fav);
    //topRow.appendChild(topActions);
    
	card.appendChild(topActions);
	card.appendChild(topRow);
	
	const title = document.createElement('div');
    title.className = 'bank-card-title';
    title.textContent = item.display_name || '(unnamed)';
    topRow.appendChild(title);

    // Meta line: source + date
    const meta = document.createElement('div');
    meta.className = 'bank-card-meta';
    meta.appendChild(sourceBadge(item.source_kind, item.source_label));
    const date = document.createElement('span');
    date.textContent = shortDate(item.updated_at || item.created_at);
    meta.appendChild(date);
    if (item.format) {
        const fmt = document.createElement('span');
        fmt.textContent = item.format;
        fmt.style.textTransform = 'lowercase';
        meta.appendChild(fmt);
    }
    card.appendChild(meta);

    // Badges: scale, root, slot, snapshot
    const badges = document.createElement('div');
    badges.style.display = 'flex';
    badges.style.gap = '0.25rem';
    badges.style.flexWrap = 'wrap';
    if (item.scale_name) badges.appendChild(simpleBadge('scale-badge', item.scale_name));
    if (item.root_note)  badges.appendChild(simpleBadge('root-badge', item.root_note));
    if (item.slot_key)   badges.appendChild(simpleBadge('slot-badge', item.slot_key));
    if (item.snapshot_name) badges.appendChild(simpleBadge('snapshot-badge', item.snapshot_name));
    if (badges.children.length) card.appendChild(badges);

    // Tag pills - look up kind from the shared tag catalog so auto/system
    // tags render with the right visual style.
    if (Array.isArray(item.tags) && item.tags.length) {
        const tagRow = document.createElement('div');
        tagRow.style.display = 'flex';
        tagRow.style.gap = '0.25rem';
        tagRow.style.flexWrap = 'wrap';
        for (const tag of item.tags) {
            tagRow.appendChild(tagPill(tag, resolveTagKind(tag)));
        }
        card.appendChild(tagRow);
    }

    // Markers: duplicate / related
    const markers = document.createElement('div');
    markers.style.display = 'flex';
    markers.style.gap = '0.5rem';
    markers.style.marginTop = 'auto';
    if (item.duplicate_status === 'exactduplicate') {
        const m = document.createElement('span');
        m.className = 'dup-marker exact material-symbols-outlined';
        m.title = 'Exact duplicate';
        m.textContent = 'content_copy';
        markers.appendChild(m);
    } else if (item.duplicate_status === 'nearduplicate') {
        const m = document.createElement('span');
        m.className = 'dup-marker material-symbols-outlined';
        m.title = 'Near duplicate';
        m.textContent = 'content_copy';
        markers.appendChild(m);
    }
    if (item.related_group_count > 0) {
        const m = document.createElement('span');
        m.className = 'rel-marker material-symbols-outlined';
        m.title = `Related in ${item.related_group_count} group(s)`;
        m.textContent = 'hub';
        markers.appendChild(m);
    }
    if (markers.children.length) card.appendChild(markers);

    const bottom = document.createElement('div');
    bottom.className = 'bank-card-bottom-actions';
    bottom.appendChild(buildItemAddToControlButton(item));
    bottom.appendChild(buildItemDeleteButton(item, { onReload }));
    card.appendChild(bottom);


    // Card-level click: toggle selection. Double-click opens drawer.
    card.addEventListener('click', (ev) => {
        if (ev.detail === 2) {
            setFocused(item.item_id);
            return;
        }
        toggleSelection(item.item_id, { index, ids, shiftKey: ev.shiftKey });
    });
    card.addEventListener('keydown', (ev) => {
        if (ev.key === 'Enter') { ev.preventDefault(); setFocused(item.item_id); }
        if (ev.key === ' ')     { ev.preventDefault(); toggleSelection(item.item_id, { index, ids }); }
    });
    return card;
}

function buildItemSelectionCheckbox(item, index, ids) {
    const box = document.createElement('input');
    box.type = 'checkbox';
    box.className = TD3_CHECKBOX;
    box.checked = state.selectedIds.has(item.item_id);
    box.title = 'Toggle selection';
    box.setAttribute('aria-label', `Select ${item.display_name || item.item_id}`);
    box.addEventListener('click', (ev) => {
        ev.stopPropagation();
        toggleSelection(item.item_id, { index, ids, shiftKey: ev.shiftKey });
    });
    return box;
}

function renderEmpty() {
    return renderEmptyPanel('cards');
}

function sourceBadge(kind, label) {
    const el = document.createElement('span');
    el.className = `source-badge source-badge-${String(kind || 'file').toLowerCase()}`;
    el.textContent = label || kind || 'unknown';
    return el;
}

function simpleBadge(klass, text) {
    const el = document.createElement('span');
    el.className = klass;
    el.textContent = text;
    return el;
}

export function tagPill(label, kind = 'user', onRemove) {
    const el = document.createElement('span');
    el.className = `bank-tag-pill kind-${kind}`;
    const span = document.createElement('span');
    span.textContent = label;
    el.appendChild(span);
    if (onRemove) {
        const x = document.createElement('span');
        x.className = 'tag-remove';
        x.textContent = '×';
        x.addEventListener('click', (ev) => { ev.stopPropagation(); onRemove(); });
        el.appendChild(x);
    }
    return el;
}

function miniAction(iconName, title, onClick) {
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'bank-card-action-btn';
    b.title = title;
    const ic = document.createElement('span');
    ic.className = 'material-symbols-outlined';
    ic.style.fontSize = '0.95rem';
    ic.textContent = iconName;
    b.appendChild(ic);
    b.addEventListener('click', onClick);
    return b;
}

function buildItemAddToControlButton(item) {
    const btn = bankButton({
        icon: 'playlist_add',
        label: 'ADD TO CONTROL',
        title: 'Append to Control',
        ariaLabel: `Add ${item.display_name || item.item_id} to Control`,
    });
    btn.addEventListener('click', async (ev) => {
        ev.preventDefault();
        ev.stopPropagation();
        btn.disabled = true;
        try {
            await addItemsToControl([item.item_id], { skipConfirm: true });
        } finally {
            btn.disabled = false;
        }
    });
    return btn;
}

function buildItemDeleteButton(item, { onReload } = {}) {
    const btn = bankButton({
        icon: 'delete',
        label: 'Delete',
        title: 'Delete item',
        ariaLabel: `Delete item ${item.display_name || item.item_id}`,
        danger: true,
    });
    btn.addEventListener('click', async (ev) => {
        ev.preventDefault();
        ev.stopPropagation();
        const name = item.display_name || item.item_id;
        const ok = await confirmModal({
            title: 'Delete item',
            message:
                `Item "${name}" will be deleted from the BANK database.\n\n` +
                `This removes the item record and its tag links. ` +
                `Source files and the TD-3 device are not touched.`,
            okLabel: 'Confirm',
            cancelLabel: 'Cancel',
            danger: true,
        });
        if (!ok) return;
        try {
            await bankApi.deleteItem(item.item_id);
            state.selectedIds.delete(item.item_id);
            if (state.focusedId === item.item_id) setFocused(null);
            toast(`Deleted item "${name}"`, 'success');
            onReload?.();
        } catch (e) {
            toast(`Delete failed: ${e.message}`, 'error');
        }
    });
    return btn;
}

function shortDate(iso) {
    if (!iso) return '-';
    const d = new Date(iso);
    if (isNaN(d.getTime())) return iso;
    return d.toISOString().slice(0, 10);
}

/** Resolve a tag label to a kind string using the cached catalog. */
export function resolveTagKind(label) {
    const t = (state.tags || []).find((x) => x && x.label === label);
    if (!t) return 'user';
    return String(t.kind || 'user').toLowerCase();
}
