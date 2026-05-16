// Right-side details drawer. Renders every field plus a small
// action row and an expandable "raw hashes / technical" block. Opens when
// state.focusedId is set; closes on × or Escape (handled in bank-main).

import { state, setFocused, setState } from './bank-state.js';
import { bankApi } from './bank-api.js';
import { toast } from './bank-toast.js';
import { tagPill } from './bank-cards.js';
import { openItemCompare } from './bank-compare.js';
import { confirmModal, openModal, promptModal } from './bank-modal.js';
import { makePlayButton } from './bank-play.js';
import { bankButton } from './bank-buttons.js';

export function renderDrawer(root, { onReload }) {
    root.textContent = '';
    if (!state.focusedId) {
        root.classList.remove('open');
        root.setAttribute('aria-hidden', 'true');
        return;
    }
    const item = state.items.find((i) => i.item_id === state.focusedId);
    if (!item) {
        root.classList.remove('open');
        return;
    }

    root.classList.add('open');
    root.setAttribute('aria-hidden', 'false');

    // Floating close tab on the drawer's left edge - always visible when
    // the drawer is open, regardless of how far the user has scrolled the
    // body inside it.
    const closeTab = document.createElement('button');
    closeTab.type = 'button';
    closeTab.className = 'bank-drawer-close-tab';
    closeTab.title = 'Close drawer (Esc)';
    closeTab.setAttribute('aria-label', 'Close drawer');
    const tabIcon = document.createElement('span');
    tabIcon.className = 'material-symbols-outlined';
    tabIcon.textContent = 'close';
    closeTab.appendChild(tabIcon);
    closeTab.addEventListener('click', () => setFocused(null));
    root.appendChild(closeTab);

    // Header
    const header = document.createElement('div');
    header.className = 'bank-drawer-header';
    const title = document.createElement('div');
    const name = document.createElement('div');
    name.className = 'text-base font-black text-on-surface tracking-wide';
    name.textContent = item.display_name || '(unnamed)';
    title.appendChild(name);
    const sub = document.createElement('div');
    sub.className = 'text-[0.65rem] font-mono text-on-surface-variant opacity-60 mt-0.5';
    sub.textContent = item.item_id;
    title.appendChild(sub);
    header.appendChild(title);

    const close = bankButton({
        icon: 'close',
        title: 'Close',
        onClick: () => setFocused(null),
    });
    header.appendChild(close);
    root.appendChild(header);

    // Action row
    const actions = document.createElement('div');
    actions.style.padding = '0.5rem 1rem';
    actions.style.display = 'flex';
    actions.style.flexWrap = 'wrap';
    actions.style.gap = '0.25rem';
    actions.style.borderBottom = '1px solid #1e2020';

    // Audition the pattern on the device - leftmost action so it's the
    // first thing the user reaches when they open a drawer.
    actions.appendChild(makePlayButton(item.item_id, { size: 'md', showLabel: true }));

    actions.appendChild(drawerButton(item.favorite ? 'star' : 'star_outline', 'FAVORITE', async () => {
        try { await bankApi.toggleFavorite(item.item_id, !item.favorite); onReload?.(); }
        catch (e) { toast(e.message, 'error'); }
    }));
    actions.appendChild(drawerButton('archive', 'ARCHIVE', async () => {
        const ok = await confirmModal({
            title: 'Archive item',
            message: `Archive "${item.display_name}"?\n\nYou can filter it back in from the Archive toggle.`,
            okLabel: 'Archive',
            cancelLabel: 'Cancel',
        });
        if (!ok) return;
        try { await bankApi.setArchived(item.item_id, !item.archived); onReload?.(); }
        catch (e) { toast(e.message, 'error'); }
    }));
    actions.appendChild(drawerButton('compare_arrows', 'COMPARE', () => {
        const selected = Array.from(state.selectedIds || []);
        const others = selected.filter((id) => id !== item.item_id);
        if (others.length === 1) {
            openItemCompare(item.item_id, others[0]);
        } else if (others.length === 0) {
            toast('Select one more item in the list, then press Compare.', 'info');
        } else {
            toast('More than two items selected - use the toolbar Compare with exactly 2 selected.', 'info');
        }
    }));
    actions.appendChild(drawerButton('label', 'TAG', async () => {
        const label = await promptModal({
            title: 'Add tag',
            label: 'Tag label:',
            okLabel: 'Add',
        });
        if (!label || !label.trim()) return;
        try { await bankApi.addTag(item.item_id, label.trim()); onReload?.(); }
        catch (e) { toast(e.message, 'error'); }
    }));
    actions.appendChild(drawerButton('photo_library', 'ADD TO SNAPSHOT', () => {
        openAddItemToSnapshotModal(item, { onReload });
    }));
    actions.appendChild(drawerButton('content_copy', 'COPY META', () => {
        const payload = JSON.stringify(item, null, 2);
        navigator.clipboard?.writeText(payload)
            .then(() => toast('Metadata copied', 'success'))
            .catch(() => toast('Clipboard unavailable', 'error'));
    }));
    if (item.source_path) {
        actions.appendChild(drawerButton('folder_open', 'OPEN LOCATION', () => openLocation(item.source_path)));
    }
    root.appendChild(actions);

    // Body
    const body = document.createElement('div');
    body.className = 'bank-drawer-body';

    body.appendChild(section('DISPLAY NAME', item.display_name));
    body.appendChild(section('SOURCE KIND',  item.source_kind));
    body.appendChild(section('SOURCE LABEL', item.source_label));
    if (item.source_path) body.appendChild(section('SOURCE PATH', item.source_path));
    if (item.format)      body.appendChild(section('FORMAT',      item.format));
    if (item.slot_key)    body.appendChild(section('SLOT KEY',    item.slot_key));
    if (item.snapshot_name) body.appendChild(section('SNAPSHOT',  `${item.snapshot_name} (${item.snapshot_id || ''})`));
    body.appendChild(section('CREATED',      item.created_at));
    body.appendChild(section('UPDATED',      item.updated_at));

    // Tags (editable) - kind-aware pills plus an autocomplete input.
    const tagsSection = document.createElement('div');
    tagsSection.className = 'bank-drawer-section';
    const tagsLabel = document.createElement('div');
    tagsLabel.className = 'bank-drawer-label';
    tagsLabel.textContent = 'TAGS';
    tagsSection.appendChild(tagsLabel);
    const tagsWrap = document.createElement('div');
    tagsWrap.style.display = 'flex';
    tagsWrap.style.flexWrap = 'wrap';
    tagsWrap.style.gap = '0.25rem';
    const tags = item.tags || [];
    if (tags.length === 0) {
        const none = document.createElement('span');
        none.className = 'bank-drawer-value opacity-70';
        none.textContent = '-';
        tagsWrap.appendChild(none);
    } else {
        for (const t of tags) {
            const kind = tagKindFor(t);
            tagsWrap.appendChild(tagPill(t, kind, async () => {
                try { await bankApi.removeTag(item.item_id, t); onReload?.(); }
                catch (e) { toast(e.message, 'error'); }
            }));
        }
    }
    tagsSection.appendChild(tagsWrap);
    tagsSection.appendChild(renderTagAutocomplete(item, onReload));
    body.appendChild(tagsSection);

    // Analysis
    const analysisSection = document.createElement('div');
    analysisSection.className = 'bank-drawer-section';
    const aLabel = document.createElement('div');
    aLabel.className = 'bank-drawer-label';
    aLabel.textContent = 'ANALYSIS';
    analysisSection.appendChild(aLabel);
    analysisSection.appendChild(kv('Status', item.analysis_status || 'unknown'));
    analysisSection.appendChild(kv('Scale',  item.scale_name || 'pending'));
    analysisSection.appendChild(kv('Root',   item.root_note  || 'pending'));
    analysisSection.appendChild(kv('Duplicate status', item.duplicate_status || 'unknown'));
    analysisSection.appendChild(kv('Related groups', String(item.related_group_count ?? 0)));
    const clusterRow = renderDuplicateClusterRow(item);
    if (clusterRow) analysisSection.appendChild(clusterRow);
    body.appendChild(analysisSection);

    if (item.notes) body.appendChild(section('NOTES', item.notes));

    // Raw technical section
    const tech = document.createElement('details');
    tech.className = 'bank-drawer-section';
    const sum = document.createElement('summary');
    sum.className = 'bank-drawer-label cursor-pointer';
    sum.textContent = 'RAW / TECHNICAL';
    tech.appendChild(sum);
    const pre = document.createElement('pre');
    pre.className = 'text-[0.7rem] font-mono bg-black p-2 rounded mt-2 overflow-x-auto';
    pre.textContent = JSON.stringify(item, null, 2);
    tech.appendChild(pre);
    body.appendChild(tech);

    root.appendChild(body);
}

function section(label, value) {
    const s = document.createElement('div');
    s.className = 'bank-drawer-section';
    const l = document.createElement('div');
    l.className = 'bank-drawer-label';
    l.textContent = label;
    s.appendChild(l);
    const v = document.createElement('div');
    v.className = 'bank-drawer-value';
    v.textContent = (value ?? '-') === '' ? '-' : (value ?? '-');
    s.appendChild(v);
    return s;
}

function kv(label, value) {
    const row = document.createElement('div');
    row.style.display = 'flex';
    row.style.justifyContent = 'space-between';
    row.style.fontSize = '0.75rem';
    row.style.fontFamily = 'monospace';
    row.style.gap = '0.5rem';
    const l = document.createElement('span');
    l.style.color = '#b8c4b8';
    l.textContent = label;
    const v = document.createElement('span');
    v.style.color = '#e2e2e2';
    v.textContent = value ?? '-';
    row.appendChild(l);
    row.appendChild(v);
    return row;
}

/**
 * If the focused item belongs to a duplicate cluster, return a compact row
 * showing the cluster kind chip, its member count, and a "view cluster"
 * button that jumps the user into the duplicates sidebar with the cluster's
 * representative focused. Returns `null` when the item is not in any cluster
 * so the caller can skip it.
 */
function renderDuplicateClusterRow(item) {
    const clusters = (state.duplicates && state.duplicates.clusters) || [];
    const cluster = clusters.find((c) =>
        Array.isArray(c.item_ids) && c.item_ids.includes(item.item_id)
    );
    if (!cluster) return null;

    const row = document.createElement('div');
    row.className = 'bank-drawer-cluster-row';

    const chip = document.createElement('span');
    const kind = String(cluster.kind || 'exact').toLowerCase();
    chip.className = `duplicates-kind-badge kind-${kind}`;
    chip.textContent = kind === 'exact' ? 'EXACT' : 'NEAR';
    row.appendChild(chip);

    const summary = document.createElement('span');
    summary.className = 'text-xs font-mono';
    const others = cluster.item_ids.filter((id) => id !== item.item_id).length;
    summary.textContent =
        `${cluster.item_ids.length} members · ${others} other${others === 1 ? '' : 's'}`;
    row.appendChild(summary);

    const view = bankButton({
        icon: 'visibility',
        label: 'VIEW CLUSTER',
        onClick: () => {
        setState({ activeSidebar: 'duplicates' });
        },
    });
    row.appendChild(view);

    if (cluster.item_ids.length >= 2) {
        const cmp = bankButton({
            icon: 'compare_arrows',
            label: 'COMPARE',
            onClick: () => {
            const partner = cluster.item_ids.find((id) => id !== item.item_id);
            if (partner) openItemCompare(item.item_id, partner);
            },
        });
        row.appendChild(cmp);
    }

    return row;
}

function drawerButton(iconName, label, onClick) {
    return bankButton({ icon: iconName, label, onClick });
}

async function openAddItemToSnapshotModal(item, { onReload } = {}) {
    let snapshots = [];
    try {
        const res = await bankApi.listSnapshots();
        snapshots = res.snapshots || [];
        setState({ snapshots });
    } catch (e) {
        toast(`Snapshot reload failed: ${e.message}`, 'error');
        return;
    }

    const body = document.createElement('div');
    body.className = 'bank-add-to-snapshot-modal';

    const itemLine = document.createElement('div');
    itemLine.className = 'text-sm font-mono';
    itemLine.textContent = item.display_name || item.item_id;
    body.appendChild(itemLine);

    let snapshotSelect = null;
    let slotSelect = null;
    const status = document.createElement('div');
    status.className = 'text-xs font-mono opacity-70 mt-2';

    async function refreshSlotChoices(snapshotId) {
        if (!slotSelect) return;
        slotSelect.textContent = '';
        const auto = document.createElement('option');
        auto.value = '';
        auto.textContent = 'First free slot';
        slotSelect.appendChild(auto);

        try {
            status.textContent = 'Loading slots...';
            const detail = await bankApi.getSnapshot(snapshotId);
            const emptySlots = (detail.slots || []).filter((slot) => slot.empty || !slot.item_id);
            for (const slot of emptySlots) {
                const opt = document.createElement('option');
                opt.value = slot.slot_key;
                opt.textContent = slot.slot_key;
                slotSelect.appendChild(opt);
            }
            status.textContent = emptySlots.length
                ? `${emptySlots.length} empty slot(s) available`
                : 'Snapshot is full';
        } catch (e) {
            status.textContent = `Slot load failed: ${e.message}`;
        }
    }

    if (snapshots.length > 0) {
        const snapshotLabel = document.createElement('label');
        const snapshotText = document.createElement('span');
        snapshotText.textContent = 'Snapshot';
        snapshotLabel.appendChild(snapshotText);
        snapshotSelect = document.createElement('select');
        for (const snap of snapshots) {
            const opt = document.createElement('option');
            opt.value = snap.snapshot_id;
            opt.textContent = `${snap.name} (${snap.slot_count ?? 0}/64)`;
            snapshotSelect.appendChild(opt);
        }
        snapshotSelect.addEventListener('change', () => {
            void refreshSlotChoices(snapshotSelect.value);
        });
        snapshotLabel.appendChild(snapshotSelect);
        body.appendChild(snapshotLabel);

        const slotLabel = document.createElement('label');
        const slotText = document.createElement('span');
        slotText.textContent = 'Slot';
        slotLabel.appendChild(slotText);
        slotSelect = document.createElement('select');
        slotLabel.appendChild(slotSelect);
        body.appendChild(slotLabel);
    } else {
        status.textContent = 'No snapshots exist. A new timestamped snapshot will be created.';
    }
    body.appendChild(status);

    openModal({
        title: 'Add to Snapshot',
        body,
        primaryLabel: 'Add',
        onPrimary: async () => {
            const payload = {};
            if (snapshotSelect?.value) payload.snapshot_id = snapshotSelect.value;
            if (slotSelect?.value) payload.slot_key = slotSelect.value;
            const res = await bankApi.addItemToSnapshot(item.item_id, payload);
            const refreshed = await bankApi.listSnapshots();
            setState({
                snapshots: refreshed.snapshots || [],
                activeSnapshotId: res.snapshot.snapshot_id,
                snapshotDetail: { snapshot: res.snapshot, slots: res.slots },
                activeSnapshotSlot: res.slot,
            });
            toast(`Added to snapshot "${res.snapshot.name}" at ${res.slot.slot_key}`, 'success');
            onReload?.();
        },
    });

    if (snapshotSelect?.value) {
        void refreshSlotChoices(snapshotSelect.value);
    }
}

/**
 * Look up the kind for a tag label from the cached tag list in state. If the
 * tag isn't known yet (e.g. local-only until the next refresh), default to
 * 'user' since that's the only kind the UI can currently create.
 */
function tagKindFor(label) {
    const t = (state.tags || []).find((x) => x && x.label === label);
    if (!t) return 'user';
    return String(t.kind || 'user').toLowerCase();
}

/**
 * Autocomplete-style "add tag" input. Filters state.tags by prefix on each
 * keystroke, shows a floating dropdown, supports ArrowUp/ArrowDown to navigate,
 * Enter to accept the highlighted suggestion, and Enter with no match to
 * create a new user tag via the backend.
 */
function renderTagAutocomplete(item, onReload) {
    const wrap = document.createElement('div');
    wrap.style.position = 'relative';
    wrap.style.marginTop = '0.35rem';

    const input = document.createElement('input');
    input.type = 'text';
    input.placeholder = 'add tag…';
    input.className = 'bank-tag-autocomplete-input';
    input.autocomplete = 'off';
    input.spellcheck = false;
    wrap.appendChild(input);

    const drop = document.createElement('div');
    drop.className = 'bank-autocomplete-dropdown';
    drop.style.display = 'none';
    wrap.appendChild(drop);

    let highlight = -1;
    let suggestions = [];

    const refreshSuggestions = () => {
        const prefix = input.value.trim().toLowerCase();
        const all = Array.isArray(state.tags) ? state.tags : [];
        const existingLabels = new Set(item.tags || []);
        suggestions = all
            .filter((t) => t && t.label)
            .filter((t) => !existingLabels.has(t.label))
            .filter((t) => t.label.toLowerCase().startsWith(prefix))
            .slice(0, 8);
        drop.textContent = '';
        if (suggestions.length === 0 || prefix === '') {
            drop.style.display = 'none';
            return;
        }
        suggestions.forEach((t, idx) => {
            const row = document.createElement('div');
            row.className = 'bank-autocomplete-row';
            if (idx === highlight) row.classList.add('active');
            const kind = String(t.kind || 'user').toLowerCase();
            const pill = document.createElement('span');
            pill.className = `bank-tag-pill kind-${kind}`;
            pill.textContent = t.label;
            row.appendChild(pill);
            const kindHint = document.createElement('span');
            kindHint.className = 'text-[0.65rem] opacity-60 font-mono ml-auto';
            kindHint.textContent = kind;
            row.appendChild(kindHint);
            row.addEventListener('mousedown', async (ev) => {
                ev.preventDefault();
                await addTag(t.label);
            });
            drop.appendChild(row);
        });
        drop.style.display = 'block';
    };

    const addTag = async (label) => {
        const clean = (label || '').trim();
        if (!clean) return;
        try {
            await bankApi.addTag(item.item_id, clean);
            input.value = '';
            highlight = -1;
            drop.style.display = 'none';
            onReload?.();
        } catch (e) {
            toast(e.message, 'error');
        }
    };

    input.addEventListener('input', () => {
        highlight = suggestions.length > 0 ? 0 : -1;
        refreshSuggestions();
    });
    input.addEventListener('focus', refreshSuggestions);
    input.addEventListener('blur', () => {
        // Delay so a mousedown on a row has a chance to fire first.
        setTimeout(() => { drop.style.display = 'none'; }, 120);
    });
    input.addEventListener('keydown', (ev) => {
        if (ev.key === 'ArrowDown') {
            if (suggestions.length === 0) return;
            ev.preventDefault();
            highlight = (highlight + 1) % suggestions.length;
            refreshSuggestions();
        } else if (ev.key === 'ArrowUp') {
            if (suggestions.length === 0) return;
            ev.preventDefault();
            highlight = (highlight - 1 + suggestions.length) % suggestions.length;
            refreshSuggestions();
        } else if (ev.key === 'Enter') {
            ev.preventDefault();
            if (highlight >= 0 && suggestions[highlight]) {
                addTag(suggestions[highlight].label);
            } else if (input.value.trim()) {
                addTag(input.value.trim());
            }
        } else if (ev.key === 'Escape') {
            drop.style.display = 'none';
            highlight = -1;
        }
    });

    return wrap;
}

function openLocation(sourcePath) {
    if (!sourcePath) return;
    try {
        const normalized = sourcePath.replace(/\\/g, '/');
        const url = normalized.startsWith('file://') ? normalized : `file:///${normalized.replace(/^\//, '')}`;
        const w = window.open(url, '_blank');
        if (!w) throw new Error('blocked');
    } catch {
        navigator.clipboard?.writeText(sourcePath)
            .then(() => toast('Browser blocked local URL - path copied to clipboard', 'info'))
            .catch(() => toast('Could not open or copy path', 'error'));
    }
}
