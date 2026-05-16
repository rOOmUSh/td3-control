// Related groups view.
//
// Pure-render module: takes a container element, fetches /api/bank/related,
// and renders a stack of group cards grouped by GroupKind. Selecting a kind
// chip re-fetches with `?kind=`. Each card surfaces:
//   - label + reason + item count;
//   - primary scale / root when relevant;
//   - up to 4 representative mini-tiles;
//   - actions: Open Group, Compare Selected, Add Group to Snapshot, and
//     Progression Seed.
//
// The module is deliberately framework-free and never sets innerHTML with
// untrusted content - every node is built via document.createElement and
// .textContent assignments to keep XSS surface zero.

import { state, setState, setFilter } from './bank-state.js';
import { bankApi } from './bank-api.js';
import { toast } from './bank-toast.js';
import { openItemCompare } from './bank-compare.js';
import { openModal } from './bank-modal.js';
import { makePlayButton } from './bank-play.js';
import { bankButton } from './bank-buttons.js';

const KIND_OPTIONS = [
    { id: '',                    label: 'All',          icon: 'hub' },
    { id: 'same-scale',          label: 'Scale',        icon: 'piano' },
    { id: 'same-root',           label: 'Root',         icon: 'music_note' },
    { id: 'same-rhythm',         label: 'Rhythm',       icon: 'graphic_eq' },
    { id: 'analyzer-related',    label: 'Analyzer',     icon: 'insights' },
    { id: 'progression-family',  label: 'Progression',  icon: 'family_restroom' },
];

let activeKind = '';

export async function render(container) {
    container.textContent = '';

    const header = document.createElement('div');
    header.className = 'related-header';

    const title = document.createElement('div');
    title.className = 'text-xs font-black tracking-[0.12em] uppercase text-on-surface-variant';
    title.textContent = 'RELATED GROUPS';
    header.appendChild(title);

    header.appendChild(buildKindChips());
    container.appendChild(header);

    const listWrap = document.createElement('div');
    listWrap.className = 'related-list';
    container.appendChild(listWrap);

    const status = document.createElement('div');
    status.className = 'related-status text-xs font-mono opacity-70';
    status.textContent = 'Loading…';
    listWrap.appendChild(status);

    let response;
    try {
        response = await bankApi.listRelated(activeKind || undefined);
    } catch (e) {
        status.textContent = `Failed to load related groups: ${e.message}`;
        return;
    }
    const groups = Array.isArray(response.groups) ? response.groups : [];
    listWrap.textContent = '';

    if (groups.length === 0) {
        listWrap.appendChild(emptyState(
            'hub',
            'NO RELATED GROUPS',
            activeKind
                ? `No groups found for kind "${activeKind}". Try a different filter or import more items.`
                : 'Add scale_name, root_note, or progression: tags to items to populate this view. Rhythm grouping needs ingested patterns with sidecars.',
        ));
        return;
    }

    // Group by GroupKind so the UI shows clusters of like-kind cards.
    const byKind = new Map();
    for (const g of groups) {
        const k = g.kind || 'unknown';
        if (!byKind.has(k)) byKind.set(k, []);
        byKind.get(k).push(g);
    }
    for (const [kind, kindGroups] of byKind) {
        listWrap.appendChild(renderKindBlock(kind, kindGroups));
    }
}

function buildKindChips() {
    const wrap = document.createElement('div');
    wrap.className = 'related-kind-chips';
    for (const opt of KIND_OPTIONS) {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'related-kind-chip';
        if (activeKind === opt.id) btn.classList.add('is-active');
        const icon = document.createElement('span');
        icon.className = 'material-symbols-outlined';
        icon.textContent = opt.icon;
        btn.appendChild(icon);
        const lbl = document.createElement('span');
        lbl.textContent = opt.label;
        btn.appendChild(lbl);
        btn.addEventListener('click', () => {
            if (activeKind === opt.id) return;
            activeKind = opt.id;
            // Re-render this section only - bank-main subscribes to state
            // and will repaint as needed.
            const root = document.getElementById('bank-view');
            if (root) render(root);
        });
        wrap.appendChild(btn);
    }
    return wrap;
}

function renderKindBlock(kind, groups) {
    const wrap = document.createElement('div');
    wrap.className = 'related-kind-block';

    const header = document.createElement('div');
    header.className = 'related-kind-header';
    const t = document.createElement('span');
    t.textContent = userKind(kind);
    header.appendChild(t);
    const ct = document.createElement('span');
    ct.className = 'related-kind-count';
    ct.textContent = `${groups.length} group(s)`;
    header.appendChild(ct);
    wrap.appendChild(header);

    const grid = document.createElement('div');
    grid.className = 'related-group-list';
    for (const g of groups) grid.appendChild(buildGroupCard(g));
    wrap.appendChild(grid);
    return wrap;
}

function buildGroupCard(group) {
    const card = document.createElement('div');
    card.className = 'related-group-card';

    const top = document.createElement('div');
    top.className = 'related-group-top';
    const lbl = document.createElement('div');
    lbl.className = 'related-group-label';
    lbl.textContent = group.label || group.group_id || 'Group';
    top.appendChild(lbl);
    const cnt = document.createElement('div');
    cnt.className = 'related-group-count';
    cnt.textContent = `${group.item_count ?? group.item_ids?.length ?? 0} item(s)`;
    top.appendChild(cnt);
    card.appendChild(top);

    const reason = document.createElement('div');
    reason.className = 'related-reason';
    reason.textContent = group.reason || '';
    card.appendChild(reason);

    if (group.primary_scale || group.primary_root) {
        const meta = document.createElement('div');
        meta.className = 'related-group-meta';
        if (group.primary_scale) {
            const c = document.createElement('span');
            c.className = 'related-meta-chip';
            c.textContent = `scale: ${group.primary_scale}`;
            meta.appendChild(c);
        }
        if (group.primary_root) {
            const c = document.createElement('span');
            c.className = 'related-meta-chip';
            c.textContent = `root: ${group.primary_root}`;
            meta.appendChild(c);
        }
        card.appendChild(meta);
    }

    // Representative tiles - best-effort lookup against state.items so we
    // can show the display_name + source badge.
    const reps = Array.isArray(group.representative_ids) ? group.representative_ids : [];
    if (reps.length > 0) {
        const repWrap = document.createElement('div');
        repWrap.className = 'related-representatives';
        for (const id of reps) repWrap.appendChild(buildRepTile(id));
        card.appendChild(repWrap);
    }

    card.appendChild(buildGroupActions(group));
    return card;
}

function buildRepTile(id) {
    const tile = document.createElement('div');
    tile.className = 'related-rep-tile';
    const item = (state.items || []).find((i) => i.item_id === id);

    const header = document.createElement('div');
    header.className = 'related-rep-header';
    header.appendChild(makePlayButton(id, { size: 'sm' }));
    const name = document.createElement('div');
    name.className = 'related-rep-name';
    name.textContent = item ? item.display_name : id;
    header.appendChild(name);
    tile.appendChild(header);

    if (item) {
        const src = document.createElement('span');
        src.className = `source-badge source-badge-${(item.source_kind || 'file').toLowerCase()}`;
        src.textContent = item.source_kind || 'file';
        tile.appendChild(src);
    }
    return tile;
}

function buildGroupActions(group) {
    const wrap = document.createElement('div');
    wrap.className = 'related-group-actions';

    const openBtn = actionButton('list', 'Open Group', () => {
        const ids = Array.isArray(group.item_ids) ? group.item_ids : [];
        if (ids.length === 0) {
            toast('Group has no items', 'info');
            return;
        }
        setFilter({ search: '' });
        setState({
            activeSidebar: 'all',
            transientItemIds: new Set(ids),
            searchQuery: '',
        });
        try { history.replaceState(null, '', '#items'); } catch { /* ignore */ }
        toast(`Showing ${ids.length} item(s) from "${group.label || 'group'}"`, 'info');
    });
    wrap.appendChild(openBtn);

    const compareBtn = actionButton('compare_arrows', 'Compare 2', () => {
        const ids = Array.isArray(group.item_ids) ? group.item_ids : [];
        if (ids.length < 2) {
            toast('Group needs at least 2 items to compare', 'info');
            return;
        }
        // Default: compare the first two representatives. The user can pick
        // exact pairs from the items view if they want a different pair.
        openItemCompare(ids[0], ids[1]);
    });
    wrap.appendChild(compareBtn);

    const snapshotBtn = actionButton('add_photo_alternate', 'Add to Snapshot', () => {
        openAddToSnapshotModal(group);
    });
    wrap.appendChild(snapshotBtn);

    const seedBtn = actionButton('auto_awesome', 'Progression Seed', () => {
        toast('Progression Seed is not available from this view.', 'info');
    });
    seedBtn.classList.add('related-action-stub');
    wrap.appendChild(seedBtn);

    return wrap;
}

function actionButton(icon, label, handler) {
    return bankButton({
        icon,
        label,
        className: 'related-action-btn',
        preventDefault: true,
        onClick: handler,
    });
}

// ---------------------------------------------------------------------------
// "Add to Snapshot" modal - pick an existing snapshot or create a new one
// ---------------------------------------------------------------------------

function openAddToSnapshotModal(group) {
    const body = document.createElement('div');

    const intro = document.createElement('p');
    intro.className = 'text-sm';
    intro.textContent = `Selected group "${group.label}" with ${group.item_count} item(s). Pick a target snapshot or enter a new name.`;
    body.appendChild(intro);

    const newLabel = document.createElement('label');
    const newSpan = document.createElement('span');
    newSpan.textContent = 'New snapshot name (leave blank to use existing)';
    newLabel.appendChild(newSpan);
    const newInput = document.createElement('input');
    newInput.type = 'text';
    newInput.placeholder = 'e.g. Phrygian-set';
    newLabel.appendChild(newInput);
    body.appendChild(newLabel);

    const existingLabel = document.createElement('label');
    const existingSpan = document.createElement('span');
    existingSpan.textContent = 'Or use existing snapshot';
    existingLabel.appendChild(existingSpan);
    const select = document.createElement('select');
    const blank = document.createElement('option');
    blank.value = '';
    blank.textContent = '- none -';
    select.appendChild(blank);
    for (const s of (state.snapshots || [])) {
        const opt = document.createElement('option');
        opt.value = s.snapshot_id;
        opt.textContent = `${s.name} (${s.snapshot_id})`;
        select.appendChild(opt);
    }
    existingLabel.appendChild(select);
    body.appendChild(existingLabel);

    const hint = document.createElement('div');
    hint.className = 'text-xs font-mono opacity-70 mt-2';
    hint.textContent = 'This action creates or selects the snapshot record only; it does not populate snapshot slots from the group.';
    body.appendChild(hint);

    openModal({
        title: 'Add Group to Snapshot',
        body,
        primaryLabel: 'Save',
        onPrimary: async () => {
            const newName = newInput.value.trim();
            if (!newName && !select.value) {
                toast('Pick an existing snapshot or enter a new name', 'error');
                throw new Error('no target snapshot');
            }
            if (newName) {
                await bankApi.createSnapshot({
                    name: newName,
                    description: `From related group: ${group.label}`,
                    origin: 'manual',
                });
                const snapshots = await bankApi.listSnapshots();
                setState({ snapshots: snapshots.snapshots || [] });
                toast(`Snapshot "${newName}" created`, 'success');
            } else {
                toast(`Snapshot target ${select.value} selected. Group slots are not populated by this action.`, 'info');
            }
        },
    });
}

// ---------------------------------------------------------------------------
// Misc helpers
// ---------------------------------------------------------------------------

function userKind(kind) {
    switch (kind) {
        case 'same-scale':         return 'SAME SCALE';
        case 'same-root':          return 'SAME ROOT';
        case 'same-rhythm':        return 'SAME RHYTHM';
        case 'analyzer-related':   return 'ANALYZER RELATED';
        case 'progression-family': return 'PROGRESSION / FAMILY';
        default: return String(kind || '').toUpperCase();
    }
}

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
