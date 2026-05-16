// Filter popover. Exposes every axis the backend currently accepts so power
// users can drive the full query surface from the UI.

import { state, setFilter, resetFilter } from './bank-state.js';
import { bankButton } from './bank-buttons.js';
import { TD3_CHECKBOX } from '../shared/button-classes.js';

const FORMATS = ['seq', 'pat', 'sqs', 'rbs', 'json', 'steps.txt', 'toml'];
const SOURCE_KINDS = ['file', 'snapshotslot', 'generated', 'curated'];
const ROOTS = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];

let activePanel = null;

export function openFilterPanel(anchorEl, onChange) {
    closePanel();
    const panel = document.createElement('div');
    panel.className = 'bank-popover';
    positionPanel(panel, anchorEl);

    panel.appendChild(labeledSelect('Format', state.filter.format, ['', ...FORMATS], (v) => {
        setFilter({ format: v || undefined });
        onChange?.();
    }));
    panel.appendChild(labeledSelect('Source kind', state.filter.source_kind, ['', ...SOURCE_KINDS], (v) => {
        setFilter({ source_kind: v || undefined });
        onChange?.();
    }));

    panel.appendChild(checkboxRow('Favorites only', !!state.filter.favorite, (b) => {
        setFilter({ favorite: b ? true : undefined });
        onChange?.();
    }));
    panel.appendChild(checkboxRow('Include archived', !!state.filter.archived, (b) => {
        setFilter({ archived: b });
        onChange?.();
    }));
    panel.appendChild(checkboxRow('Duplicates only', !!state.filter.duplicate_only, (b) => {
        setFilter({ duplicate_only: b });
        onChange?.();
    }));
    panel.appendChild(checkboxRow('Related only', !!state.filter.related_only, (b) => {
        setFilter({ related_only: b });
        onChange?.();
    }));
    panel.appendChild(checkboxRow('Failed imports only', !!state.filter.failed_imports_only, (b) => {
        setFilter({ failed_imports_only: b });
        onChange?.();
    }));
    panel.appendChild(checkboxRow('Needs review only', !!state.filter.needs_review, (b) => {
        setFilter({ needs_review: b });
        onChange?.();
    }));

    const snapshotOptions = [''].concat(state.snapshots.map((s) => s.snapshot_id));
    const snapshotLabels = { '': '- any -' };
    for (const s of state.snapshots) snapshotLabels[s.snapshot_id] = `${s.name} (${s.snapshot_id.slice(0, 6)})`;
    panel.appendChild(labeledSelect('Snapshot', state.filter.snapshot_id, snapshotOptions, (v) => {
        setFilter({ snapshot_id: v || undefined });
        onChange?.();
    }, snapshotLabels));

    panel.appendChild(labeledInput('Slot key (e.g. G2P4B)', state.filter.slot_key, (v) => {
        setFilter({ slot_key: v || undefined });
        onChange?.();
    }));

    // Scale: the backend filter is an exact string match. We let the user
    // type freely instead of constraining to an enum - the analyzer hasn't
    // defined a final set yet, and unknown values will just return zero
    // matches which is visible behaviour.
    panel.appendChild(labeledInput('Scale name', state.filter.scale, (v) => {
        setFilter({ scale: v || undefined });
        onChange?.();
    }));
    panel.appendChild(labeledSelect('Root', state.filter.root, ['', ...ROOTS], (v) => {
        setFilter({ root: v || undefined });
        onChange?.();
    }));

    const tagOptions = [''].concat(state.tags.map((t) => t.label));
    panel.appendChild(labeledSelect('Tag (exact)', state.filter.tag, tagOptions, (v) => {
        setFilter({ tag: v || undefined });
        onChange?.();
    }));

    panel.appendChild(labeledInput('Date from (ISO)', state.filter.date_from, (v) => {
        setFilter({ date_from: v || undefined });
        onChange?.();
    }));
    panel.appendChild(labeledInput('Date to (ISO)', state.filter.date_to, (v) => {
        setFilter({ date_to: v || undefined });
        onChange?.();
    }));

    const footer = document.createElement('div');
    footer.className = 'flex justify-between mt-3 gap-2';
    const resetBtn = bankButton({
        label: 'RESET',
        onClick: () => {
            resetFilter();
            closePanel();
            onChange?.();
        },
    });
    footer.appendChild(resetBtn);
    const closeBtn = bankButton({ label: 'CLOSE', onClick: closePanel });
    footer.appendChild(closeBtn);
    panel.appendChild(footer);

    document.body.appendChild(panel);
    activePanel = panel;

    const outside = (ev) => {
        if (!panel.contains(ev.target) && ev.target !== anchorEl && !anchorEl.contains(ev.target)) {
            closePanel();
            document.removeEventListener('mousedown', outside);
        }
    };
    setTimeout(() => document.addEventListener('mousedown', outside), 0);
}

export function closePanel() {
    if (activePanel) { activePanel.remove(); activePanel = null; }
}

function positionPanel(panel, anchor) {
    const rect = anchor.getBoundingClientRect();
    panel.style.top = `${rect.bottom + 6}px`;
    panel.style.left = `${Math.min(rect.left, window.innerWidth - 340)}px`;
}

function labeledSelect(label, currentValue, options, onChange, labelsMap) {
    const wrap = document.createElement('label');
    wrap.className = 'block mb-2';
    const span = document.createElement('span');
    span.className = 'block text-[0.65rem] uppercase tracking-widest text-on-surface-variant font-bold mb-1';
    span.textContent = label;
    wrap.appendChild(span);
    const sel = document.createElement('select');
    sel.className = 'w-full bg-surface-container-lowest text-on-surface text-xs font-mono p-2 rounded border border-surface-container-highest';
    for (const opt of options) {
        const o = document.createElement('option');
        o.value = opt;
        o.textContent = labelsMap && opt in labelsMap ? labelsMap[opt] : (opt === '' ? '- any -' : opt);
        if ((currentValue ?? '') === opt) o.selected = true;
        sel.appendChild(o);
    }
    sel.addEventListener('change', () => onChange(sel.value));
    wrap.appendChild(sel);
    return wrap;
}

function labeledInput(label, currentValue, onChange) {
    const wrap = document.createElement('label');
    wrap.className = 'block mb-2';
    const span = document.createElement('span');
    span.className = 'block text-[0.65rem] uppercase tracking-widest text-on-surface-variant font-bold mb-1';
    span.textContent = label;
    wrap.appendChild(span);
    const input = document.createElement('input');
    input.type = 'text';
    input.value = currentValue || '';
    input.className = 'w-full bg-surface-container-lowest text-on-surface text-xs font-mono p-2 rounded border border-surface-container-highest';
    let t = 0;
    input.addEventListener('input', () => {
        clearTimeout(t);
        t = setTimeout(() => onChange(input.value.trim()), 200);
    });
    wrap.appendChild(input);
    return wrap;
}

function checkboxRow(label, checked, onChange) {
    const wrap = document.createElement('label');
    wrap.className = 'flex items-center gap-2 mb-2 text-xs text-on-surface cursor-pointer';
    const box = document.createElement('input');
    box.type = 'checkbox';
    box.className = TD3_CHECKBOX;
    box.checked = !!checked;
    box.addEventListener('change', () => onChange(box.checked));
    wrap.appendChild(box);
    const span = document.createElement('span');
    span.textContent = label;
    wrap.appendChild(span);
    return wrap;
}
