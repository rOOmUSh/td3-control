// Duplicate-cluster browser.
//
// Public API:
//   - renderDuplicates(root, { onReload })
//       Renders the list of DuplicateCluster rows returned by the backend.
//       Each cluster shows its kind (exact/near), representative, members,
//       and per-cluster actions (compare members, reveal in library).
//
//   - openClusterCompare(cluster)
//       Shortcut: picks the first two item_ids and opens openItemCompare.
//
// Relies on state.duplicates.clusters, which bank-main loads via
// bankApi.listDuplicates() on reload.

import { state, setFocused, setState } from './bank-state.js';
import { toast } from './bank-toast.js';
import { openItemCompare } from './bank-compare.js';
import { applySidebarFilters } from './bank-sidebar.js';
import { makePlayButton } from './bank-play.js';
import { bankButton } from './bank-buttons.js';

export function renderDuplicates(root, { onReload } = {}) {
    root.textContent = '';
    const clusters = (state.duplicates && state.duplicates.clusters) || [];

    const header = document.createElement('div');
    header.className = 'duplicates-header';
    const title = document.createElement('div');
    title.className = 'duplicates-title';
    title.textContent = `${clusters.length} DUPLICATE CLUSTER(S)`;
    header.appendChild(title);
    const refresh = bankButton({
        label: 'REFRESH',
        onClick: () => onReload?.(),
    });
    header.appendChild(refresh);
    root.appendChild(header);

    if (clusters.length === 0) {
        root.appendChild(emptyState());
        return;
    }

    const exact = clusters.filter((c) => String(c.kind).toLowerCase() === 'exact');
    const near  = clusters.filter((c) => String(c.kind).toLowerCase() === 'near');

    if (exact.length > 0) {
        root.appendChild(sectionHeader('EXACT DUPLICATES', exact.length,
            'Byte-identical pattern payloads.'));
        const list = document.createElement('div');
        list.className = 'duplicates-list';
        exact.forEach((c) => list.appendChild(clusterCard(c, 'exact')));
        root.appendChild(list);
    }

    if (near.length > 0) {
        root.appendChild(sectionHeader('NEAR DUPLICATES', near.length,
            'Same rhythm fingerprint, up to 3 note edits between members.'));
        const list = document.createElement('div');
        list.className = 'duplicates-list';
        near.forEach((c) => list.appendChild(clusterCard(c, 'near')));
        root.appendChild(list);
    }
}

export function openClusterCompare(cluster) {
    if (!cluster || !Array.isArray(cluster.item_ids) || cluster.item_ids.length < 2) {
        toast('Cluster has fewer than two members', 'error');
        return;
    }
    const rep = cluster.representative_id || cluster.item_ids[0];
    const other = cluster.item_ids.find((id) => id !== rep) || cluster.item_ids[1];
    openItemCompare(rep, other);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function emptyState() {
    const wrap = document.createElement('div');
    wrap.className = 'bank-empty duplicates-empty';
    const icon = document.createElement('span');
    icon.className = 'material-symbols-outlined';
    icon.textContent = 'content_copy';
    wrap.appendChild(icon);
    const title = document.createElement('div');
    title.className = 'text-base font-black tracking-widest';
    title.textContent = 'NO DUPLICATES';
    wrap.appendChild(title);
    const hint = document.createElement('div');
    hint.className = 'text-xs font-mono opacity-70 max-w-md';
    hint.textContent =
        'Duplicate detection runs over every pattern with a cached sidecar. Import more patterns or re-run a backup sync to populate the clustering.';
    wrap.appendChild(hint);
    return wrap;
}

function sectionHeader(label, count, hint) {
    const h = document.createElement('div');
    h.className = 'duplicates-section-header';
    const title = document.createElement('span');
    title.className = 'duplicates-section-title';
    title.textContent = `${label} · ${count}`;
    h.appendChild(title);
    if (hint) {
        const sub = document.createElement('span');
        sub.className = 'duplicates-section-hint';
        sub.textContent = hint;
        h.appendChild(sub);
    }
    return h;
}

function clusterCard(cluster, kind) {
    const card = document.createElement('div');
    card.className = `bank-card duplicates-card duplicates-${kind}`;

    const top = document.createElement('div');
    top.className = 'duplicates-card-top';
    const kindBadge = document.createElement('span');
    kindBadge.className = `duplicates-kind-badge kind-${kind}`;
    kindBadge.textContent = kind.toUpperCase();
    top.appendChild(kindBadge);

    const repLabel = document.createElement('span');
    repLabel.className = 'duplicates-rep';
    const rep = itemLabel(cluster.representative_id);
    repLabel.textContent = `KEEP → ${rep}`;
    top.appendChild(repLabel);

    const memberCount = document.createElement('span');
    memberCount.className = 'duplicates-count';
    memberCount.textContent = `${cluster.item_ids.length} member(s)`;
    top.appendChild(memberCount);
    card.appendChild(top);

    if (Array.isArray(cluster.reasons) && cluster.reasons.length > 0) {
        const reasons = document.createElement('div');
        reasons.className = 'duplicates-reasons';
        reasons.textContent = cluster.reasons.join(' · ');
        card.appendChild(reasons);
    }

    const members = document.createElement('div');
    members.className = 'duplicates-members';
    (cluster.item_ids || []).forEach((id) => {
        const row = document.createElement('div');
        row.className = 'duplicates-member-row';
        row.appendChild(makePlayButton(id, { size: 'sm' }));
        row.appendChild(memberChip(id, id === cluster.representative_id));
        members.appendChild(row);
    });
    card.appendChild(members);

    const actions = document.createElement('div');
    actions.className = 'duplicates-actions';
    actions.appendChild(actionButton('compare_arrows', 'COMPARE', () => {
        openClusterCompare(cluster);
    }));
    actions.appendChild(actionButton('visibility', 'OPEN REPRESENTATIVE', () => {
        const id = cluster.representative_id || cluster.item_ids[0];
        applySidebarFilters('all');
        setState({ activeSidebar: 'all', transientItemIds: null });
        setFocused(id);
        try { history.replaceState(null, '', '#items'); } catch { /* ignore */ }
    }));
    actions.appendChild(actionButton('filter_list', 'FILTER TO CLUSTER', () => {
        const ids = new Set(cluster.item_ids || []);
        const id = cluster.representative_id || cluster.item_ids[0];
        applySidebarFilters('all');
        setState({
            activeSidebar: 'all',
            transientItemIds: ids,
            searchQuery: '',
        });
        setFocused(id);
        try { history.replaceState(null, '', '#items'); } catch { /* ignore */ }
        toast(`Showing ${ids.size} item(s) from the cluster`, 'info');
    }));
    card.appendChild(actions);

    return card;
}

function memberChip(itemId, isRepresentative) {
    const chip = document.createElement('button');
    chip.type = 'button';
    chip.className = 'duplicates-member-chip';
    if (isRepresentative) chip.classList.add('is-representative');
    chip.title = 'Open in drawer';
    const label = document.createElement('span');
    label.textContent = itemLabel(itemId);
    chip.appendChild(label);
    if (isRepresentative) {
        const star = document.createElement('span');
        star.className = 'material-symbols-outlined duplicates-member-star';
        star.textContent = 'star';
        chip.appendChild(star);
    }
    chip.addEventListener('click', () => {
        applySidebarFilters('all');
        setState({ activeSidebar: 'all', transientItemIds: null });
        setFocused(itemId);
        try { history.replaceState(null, '', '#items'); } catch { /* ignore */ }
    });
    return chip;
}

function actionButton(iconName, label, onClick) {
    return bankButton({ icon: iconName, label, onClick });
}

function itemLabel(id) {
    const item = (state.items || []).find((i) => i.item_id === id);
    if (!item) return id;
    return `${item.display_name} (${id})`;
}
