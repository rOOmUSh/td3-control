// Settings → CONFIG tab. Renders editable forms for every key declared
// by `env_metadata::FIELDS` on the server, grouped by section. Saves
// through POST /api/config/env; resets a section back to the bundled
// template through POST /api/config/env/reset-section.
//
// All confirmations and status surfaces use the project's custom inline
// modal / toast (bank-modal, bank-toast).

import { confirmModal } from './bank/bank-modal.js';
import { toast } from './bank/bank-toast.js';
import { createButton } from './shared/dom-button.js';
import { TD3_CHECKBOX } from './shared/button-classes.js';

const navConfigList = document.getElementById('nav-config-list');
const pageConfig = document.getElementById('page-config');
const pageKeyboard = document.getElementById('page-keyboard');
const pageScales = document.getElementById('page-scales');
const navKeyboard = document.getElementById('nav-keyboard');
const navScales = document.getElementById('nav-scales');
const sectionTitle = document.getElementById('config-section-title');
const fieldsContainer = document.getElementById('config-fields');
const dirtyIndicator = document.getElementById('config-dirty');
const filePathLabel = document.getElementById('config-file-path');
const statusMsg = document.getElementById('status-msg-config');
const btnSave = document.getElementById('btn-save-config');
const btnReset = document.getElementById('btn-reset-config');

// Full payload from GET /api/config/env/full. Fetched once per page
// load; on save success we re-fetch so the rendered values always
// reflect on-disk state (including any unmatched-key additions the
// writer appended).
let payload = null;

// Currently-selected section id.
let activeSectionId = null;

// { KEY: rawString } for fields the user has edited but not yet saved.
// Emptied on successful save or reset.
const pendingEdits = new Map();

// ---------------------------------------------------------------------
// Tab switching coexistence with settings.js
// ---------------------------------------------------------------------

// When KEYBOARD or SCALES tabs are clicked (handled in settings.js),
// we hide the CONFIG page and deactivate every CONFIG nav button.
function hideConfigPage() {
    pageConfig.classList.add('hidden');
    for (const btn of navConfigList.querySelectorAll('button')) {
        setNavBtnActive(btn, false);
    }
    activeSectionId = null;
}

navKeyboard.addEventListener('click', hideConfigPage);
navScales.addEventListener('click', hideConfigPage);

function showConfigPage() {
    pageKeyboard.classList.add('hidden');
    pageScales.classList.add('hidden');
    pageConfig.classList.remove('hidden');
    // Settings.js paints keyboard/scales buttons active on click -
    // here we reverse that so neither is highlighted while the user is
    // on a CONFIG section.
    for (const btn of [navKeyboard, navScales]) {
        btn.classList.remove('is-active');
    }
}

function setNavBtnActive(btn, active) {
    btn.classList.toggle('is-active', active);
}

// ---------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------

function renderSidebar() {
    navConfigList.innerHTML = '';
    for (const section of payload.sections) {
        const btn = createButton({
            className: 'settings-nav-btn tactile-button',
            label: section.title,
            onClick: () => selectSection(section.id),
        });
        btn.dataset.sectionId = section.id;
        navConfigList.appendChild(btn);
    }
}

function renderFields() {
    fieldsContainer.innerHTML = '';
    const fieldsInSection = payload.fields.filter(
        (f) => f.section_id === activeSectionId,
    );
    for (const field of fieldsInSection) {
        fieldsContainer.appendChild(buildFieldRow(field));
    }
}

function buildFieldRow(field) {
    const row = document.createElement('div');
    row.className =
        'flex flex-col gap-1 p-3 bg-surface-container rounded-lg border border-outline-variant';

    const header = document.createElement('div');
    header.className = 'flex items-center justify-between gap-3';
    const keyLabel = document.createElement('code');
    keyLabel.className = 'text-[0.85rem] font-black text-primary-fixed tracking-wider';
    keyLabel.textContent = field.key;
    header.appendChild(keyLabel);

    const hint = document.createElement('span');
    hint.className = 'text-[0.75rem] text-on-surface-variant opacity-70';
    hint.textContent = kindHint(field);
    header.appendChild(hint);
    row.appendChild(header);

    const description = document.createElement('p');
    description.className = 'text-[0.8rem] text-on-surface-variant opacity-80';
    description.textContent = field.description;
    row.appendChild(description);

    const input = buildInput(field);
    input.dataset.key = field.key;
    input.addEventListener('input', () => markDirty(field.key, readValue(input, field)));
    input.addEventListener('change', () => markDirty(field.key, readValue(input, field)));
    row.appendChild(input);

    return row;
}

function kindHint(field) {
    switch (field.kind) {
        case 'integer':
            return `integer (${field.min}..=${field.max})`;
        case 'bool':
            return 'bool';
        case 'enum':
            return `enum (${field.options.join(' | ')})`;
        case 'scaleId':
            return 'scale id';
        default:
            return 'string';
    }
}

function buildInput(field) {
    const current = payload.values[field.key] ?? '';
    if (field.kind === 'bool') {
        const wrap = document.createElement('label');
        wrap.className = 'flex items-center gap-2';
        const cb = document.createElement('input');
        cb.type = 'checkbox';
        cb.className = TD3_CHECKBOX;
        cb.checked = rawToBool(current);
        const txt = document.createElement('span');
        txt.className = 'text-[0.85rem] text-on-surface';
        txt.textContent = cb.checked ? 'Enabled' : 'Disabled';
        cb.addEventListener('change', () => {
            txt.textContent = cb.checked ? 'Enabled' : 'Disabled';
        });
        wrap.appendChild(cb);
        wrap.appendChild(txt);
        return wrap;
    }
    if (field.kind === 'enum') {
        const sel = document.createElement('select');
        sel.className =
            'w-full bg-surface-container-high text-on-surface rounded-lg px-3 py-2 ' +
            'text-[0.9rem] border border-outline-variant';
        for (const opt of field.options) {
            const option = document.createElement('option');
            option.value = opt;
            option.textContent = opt;
            if (opt.toLowerCase() === current.toLowerCase()) option.selected = true;
            sel.appendChild(option);
        }
        return sel;
    }
    const input = document.createElement('input');
    input.className =
        'w-full bg-surface-container-high text-on-surface rounded-lg px-3 py-2 ' +
        'text-[0.9rem] font-mono border border-outline-variant';
    if (field.kind === 'integer') {
        input.type = 'number';
        input.min = String(field.min);
        input.max = String(field.max);
        input.step = '1';
    } else {
        input.type = 'text';
    }
    input.value = current;
    return input;
}

function readValue(node, field) {
    if (field.kind === 'bool') {
        const cb = node.querySelector('input[type="checkbox"]');
        return cb.checked ? '1' : '0';
    }
    if (field.kind === 'enum') {
        return node.value;
    }
    return node.value;
}

function rawToBool(raw) {
    const v = String(raw).trim().toLowerCase();
    return v === '1' || v === 'true' || v === 'yes';
}

// ---------------------------------------------------------------------
// Dirty tracking
// ---------------------------------------------------------------------

function markDirty(key, newValue) {
    const originalRaw = payload.values[key] ?? '';
    // Normalize bool originals so toggling back to "1" when file holds
    // "true" doesn't register as dirty.
    const fieldMeta = payload.fields.find((f) => f.key === key);
    let comparable = originalRaw;
    if (fieldMeta && fieldMeta.kind === 'bool') {
        comparable = rawToBool(originalRaw) ? '1' : '0';
    }
    if (newValue === comparable) {
        pendingEdits.delete(key);
    } else {
        pendingEdits.set(key, newValue);
    }
    updateDirtyIndicator();
}

function updateDirtyIndicator() {
    if (pendingEdits.size === 0) {
        dirtyIndicator.textContent = '';
        return;
    }
    const noun = pendingEdits.size === 1 ? 'change' : 'changes';
    dirtyIndicator.textContent = `${pendingEdits.size} unsaved ${noun}`;
}

// ---------------------------------------------------------------------
// Section navigation with unsaved-changes guard
// ---------------------------------------------------------------------

async function selectSection(sectionId) {
    if (pendingEdits.size > 0 && activeSectionId && activeSectionId !== sectionId) {
        const ok = await confirmModal({
            title: 'Discard unsaved changes?',
            message:
                `You have ${pendingEdits.size} unsaved change(s) in this section.\n` +
                'Switching sections will discard them.',
            okLabel: 'Discard',
            cancelLabel: 'Stay',
            danger: true,
        });
        if (!ok) return;
        pendingEdits.clear();
    }

    showConfigPage();
    for (const btn of navConfigList.querySelectorAll('button')) {
        setNavBtnActive(btn, btn.dataset.sectionId === sectionId);
    }
    activeSectionId = sectionId;
    const section = payload.sections.find((s) => s.id === sectionId);
    sectionTitle.textContent = section ? section.title : sectionId;
    statusMsg.textContent = '';
    pendingEdits.clear();
    updateDirtyIndicator();
    renderFields();
}

// ---------------------------------------------------------------------
// Save / Reset
// ---------------------------------------------------------------------

btnSave.addEventListener('click', async () => {
    if (pendingEdits.size === 0) {
        statusMsg.textContent = 'No changes to save.';
        return;
    }
    const updates = Object.fromEntries(pendingEdits);
    btnSave.disabled = true;
    try {
        const res = await fetch('/api/config/env', {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({ updates }),
        });
        if (!res.ok) {
            const body = await res.json().catch(() => ({}));
            const err = body.error || `HTTP ${res.status}`;
            statusMsg.textContent = `Save failed: ${err}`;
            toast(`Config save failed: ${err}`, 'error');
            return;
        }
        toast('Config saved. Restart required for changes to take effect.', 'success');
        await reloadPayload();
        renderFields();
        pendingEdits.clear();
        updateDirtyIndicator();
        statusMsg.textContent = 'Saved. Restart the TD-3 utility to apply.';
    } finally {
        btnSave.disabled = false;
    }
});

btnReset.addEventListener('click', async () => {
    if (!activeSectionId) return;
    const section = payload.sections.find((s) => s.id === activeSectionId);
    const ok = await confirmModal({
        title: 'Reset section to defaults?',
        message:
            `Every key in "${section ? section.title : activeSectionId}" will be ` +
            'written back to its bundled default. This cannot be undone.',
        okLabel: 'Reset',
        cancelLabel: 'Cancel',
        danger: true,
    });
    if (!ok) return;

    btnReset.disabled = true;
    try {
        const res = await fetch('/api/config/env/reset-section', {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({ section_id: activeSectionId }),
        });
        if (!res.ok) {
            const body = await res.json().catch(() => ({}));
            const err = body.error || `HTTP ${res.status}`;
            statusMsg.textContent = `Reset failed: ${err}`;
            toast(`Reset failed: ${err}`, 'error');
            return;
        }
        toast('Section reset to defaults. Restart required.', 'success');
        await reloadPayload();
        pendingEdits.clear();
        updateDirtyIndicator();
        renderFields();
        statusMsg.textContent = 'Section reset. Restart the TD-3 utility to apply.';
    } finally {
        btnReset.disabled = false;
    }
});

// ---------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------

async function reloadPayload() {
    const res = await fetch('/api/config/env/full');
    if (!res.ok) {
        throw new Error(`HTTP ${res.status}`);
    }
    payload = await res.json();
    filePathLabel.textContent = payload.env_file_path;
}

async function init() {
    try {
        await reloadPayload();
        renderSidebar();
    } catch (err) {
        // Don't blow up the page - the KEYBOARD/SCALES tabs still work.
        statusMsg.textContent = `Failed to load config: ${err.message}`;
        navConfigList.innerHTML =
            '<span class="text-[0.8rem] text-on-surface-variant opacity-60 px-3">unavailable</span>';
    }
}

init();
