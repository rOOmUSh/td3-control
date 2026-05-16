// Save multipattern canvas patterns into the Bank catalog.
//
// This is separate from PUSH TO TD-3. It materialises UI patterns as Bank
// LibraryItems, optionally attached to a snapshot. The backend owns duplicate
// detection and sidecar persistence; this module only chooses the target set,
// gathers root/scale metadata from the sidebar, and opens the destination
// modal.

import { openModal } from '../bank/bank-modal.js';
import { bankButton } from '../bank/bank-buttons.js';
import { slotFor } from '../shared/slot-targets.js';
import { toDashedSlotKey } from './multipattern-snapshot.js';

let mpState = null;
let bankApi = null;
let setStatus = () => {};

const ROOT_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];

export function init({ state, bankApi: api, setStatus: status }) {
    mpState = state;
    bankApi = api;
    if (typeof status === 'function') setStatus = status;

    const btn = document.getElementById('btn-all-to-bank');
    if (btn) {
        btn.addEventListener('click', () => openAllToBank());
    }
}

export function openAllToBank() {
    if (!mpState || !bankApi) {
        setStatus('Bank save unavailable');
        return;
    }
    const checked = mpState.getCheckedArray();
    const indexes = checked.length > 0 ? checked : mpState.getAllIndexes();
    if (!indexes.length) {
        setStatus('No patterns to save to bank');
        return;
    }
    openBankDestinationModal({
        title: checked.length > 0 ? `Save ${checked.length} checked patterns to bank` : 'Save all patterns to bank',
        indexes,
    });
}

export function openSingleToBank(index) {
    if (!mpState || !bankApi) {
        setStatus('Bank save unavailable');
        return;
    }
    if (!Number.isInteger(index) || index < 0 || index >= mpState.getPatternCount()) {
        setStatus('Pattern not found');
        return;
    }
    openBankDestinationModal({
        title: `Save P${index + 1} to bank`,
        indexes: [index],
    });
}

export function buildBankSaveEntries({ patterns, indexes, scratch, mode, startSlot }) {
    if (!Array.isArray(patterns) || !Array.isArray(indexes)) return [];
    return indexes
        .map((idx) => {
            if (!Number.isInteger(idx) || idx < 0 || idx >= patterns.length) return null;
            const assigned = slotFor(idx, scratch, mode, startSlot);
            const dashed = assigned ? toDashedSlotKey(assigned.label) : null;
            return {
                pattern: patterns[idx],
                display_name: assigned ? `P${idx + 1} ${assigned.label}` : `P${idx + 1}`,
                slot_key: dashed || undefined,
            };
        })
        .filter(Boolean);
}

export function readSidebarBankMetadata(doc = document) {
    const rootSelect = doc.getElementById('root-select');
    const scaleSelect = doc.getElementById('scale-select');
    const rootValue = rootSelect ? parseInt(rootSelect.value, 10) : NaN;
    const root_note = Number.isInteger(rootValue) && rootValue >= 0 && rootValue < ROOT_NAMES.length
        ? ROOT_NAMES[rootValue]
        : null;
    const scale_name = scaleSelect && scaleSelect.value ? String(scaleSelect.value) : null;
    return { root_note, scale_name };
}

function openBankDestinationModal({ title, indexes }) {
    let snapshots = [];
    let selected = { kind: 'new_snapshot', snapshotId: null };

    const body = document.createElement('div');
    body.className = 'bank-confirm-body';

    const summary = document.createElement('p');
    summary.textContent = `${indexes.length} pattern${indexes.length === 1 ? '' : 's'} will be saved from the multipattern canvas.`;
    body.appendChild(summary);

    const list = document.createElement('div');
    list.className = 'flex flex-col gap-2';
    body.appendChild(list);

    const refreshSelection = () => {
        list.querySelectorAll('button[data-bank-destination]').forEach((btn) => {
            const active = btn.dataset.kind === selected.kind
                && (selected.kind !== 'snapshot' || btn.dataset.snapshotId === selected.snapshotId);
            btn.classList.toggle('is-active', active);
            btn.setAttribute('aria-pressed', active ? 'true' : 'false');
        });
    };

    const addChoice = ({ label, detail, kind, snapshotId }) => {
        const btn = bankButton({ className: 'text-left' });
        btn.dataset.bankDestination = 'true';
        btn.dataset.kind = kind;
        if (snapshotId) btn.dataset.snapshotId = snapshotId;
        btn.style.display = 'block';
        btn.style.width = '100%';
        setDestinationButtonContent(btn, label, detail);
        btn.addEventListener('click', () => {
            selected = { kind, snapshotId: snapshotId || null };
            refreshSelection();
        });
        list.appendChild(btn);
    };

    addChoice({
        label: 'NEW SNAPSHOT',
        detail: 'Create an SN_* timestamp snapshot and place patterns into bank slots.',
        kind: 'new_snapshot',
    });
    addChoice({
        label: 'SINGLE ITEM',
        detail: 'Save standalone bank item(s), not attached to a snapshot.',
        kind: 'single_item',
    });

    const snapshotWrap = document.createElement('div');
    snapshotWrap.className = 'flex flex-col gap-1';
    snapshotWrap.style.maxHeight = '14rem';
    snapshotWrap.style.overflow = 'auto';
    list.appendChild(snapshotWrap);

    const loading = document.createElement('p');
    loading.textContent = 'Loading snapshots...';
    snapshotWrap.appendChild(loading);

    const close = openModal({
        title,
        body,
        primaryLabel: 'Confirm',
        secondaryLabel: 'Cancel',
        noScrim: true,
        size: 'wide',
        onPrimary: async () => {
            await saveToSelectedDestination(indexes, selected);
        },
    });

    bankApi.listSnapshots()
        .then((res) => {
            snapshots = Array.isArray(res.snapshots) ? res.snapshots : [];
            snapshotWrap.replaceChildren();
            if (!snapshots.length) {
                const empty = document.createElement('p');
                empty.textContent = 'No existing snapshots.';
                snapshotWrap.appendChild(empty);
                refreshSelection();
                return;
            }
            for (const snap of snapshots) {
                const btn = bankButton({ className: 'text-left' });
                btn.dataset.bankDestination = 'true';
                btn.dataset.kind = 'snapshot';
                btn.dataset.snapshotId = snap.snapshot_id;
                btn.style.display = 'block';
                btn.style.width = '100%';
                const count = Number.isFinite(snap.slot_count) ? `${snap.slot_count}/64` : '';
                setDestinationButtonContent(
                    btn,
                    snap.name || snap.snapshot_id,
                    count ? `${count} slots used` : '',
                );
                btn.addEventListener('click', () => {
                    selected = { kind: 'snapshot', snapshotId: snap.snapshot_id };
                    refreshSelection();
                });
                snapshotWrap.appendChild(btn);
            }
            refreshSelection();
        })
        .catch((err) => {
            snapshotWrap.replaceChildren();
            const p = document.createElement('p');
            p.className = 'bank-warn';
            p.textContent = `Could not load snapshots: ${err.message || err}`;
            snapshotWrap.appendChild(p);
        });

    refreshSelection();
    void close;
}

async function saveToSelectedDestination(indexes, selected) {
    if (!selected || !selected.kind) throw new Error('Choose a bank destination');
    if (selected.kind === 'snapshot' && !selected.snapshotId) {
        throw new Error('Choose a snapshot');
    }

    let entries = buildBankSaveEntries({
        patterns: mpState.getPatterns(),
        indexes,
        scratch: mpState.getScratchSlot(),
        mode: mpState.getAbMode(),
        startSlot: mpState.getSelectedSlot(),
    });
    if (!entries.length) throw new Error('No patterns to save');
    if (selected.kind === 'snapshot' || indexes.length === 1) {
        entries = entries.map(({ slot_key, ...entry }) => entry);
    }

    const metadata = readSidebarBankMetadata();
    setStatus('Saving pattern(s) to bank...');
    const res = await bankApi.savePatternsToBank({
        destination: selected.kind,
        snapshot_id: selected.snapshotId || undefined,
        root_note: metadata.root_note || undefined,
        scale_name: metadata.scale_name || undefined,
        entries,
    });

    if (selected.kind === 'single_item') {
        setStatus(`Saved ${res.items?.length || entries.length} bank item${entries.length === 1 ? '' : 's'}`);
        return;
    }

    const name = res.snapshot && res.snapshot.name ? res.snapshot.name : 'snapshot';
    setStatus(`Saved ${entries.length} pattern${entries.length === 1 ? '' : 's'} to ${name}`);
}

function setDestinationButtonContent(btn, label, detail) {
    const strong = document.createElement('strong');
    strong.textContent = label;
    btn.appendChild(strong);
    if (!detail) return;
    btn.appendChild(document.createElement('br'));
    const span = document.createElement('span');
    span.textContent = detail;
    btn.appendChild(span);
}
