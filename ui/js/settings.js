// Settings page: keyboard mapping + scales editor.
// All defaults loaded from ui/config/*-defaults.json (no hardcoded values).

import { api } from './api.js';
import { createButton } from './shared/dom-button.js';

async function fetchJson(url) {
    const res = await fetch(url);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return res.json();
}

// ===== Navigation =====
const navKeyboard = document.getElementById('nav-keyboard');
const navScales = document.getElementById('nav-scales');
const pageKeyboard = document.getElementById('page-keyboard');
const pageScales = document.getElementById('page-scales');

function showPage(page) {
    pageKeyboard.classList.add('hidden');
    pageScales.classList.add('hidden');
    navKeyboard.classList.remove('is-active');
    navScales.classList.remove('is-active');
    if (page === 'keyboard') {
        pageKeyboard.classList.remove('hidden');
        navKeyboard.classList.add('is-active');
    } else {
        pageScales.classList.remove('hidden');
        navScales.classList.add('is-active');
    }
}

navKeyboard.addEventListener('click', () => showPage('keyboard'));
navScales.addEventListener('click', () => showPage('scales'));

// ===== Keyboard Mapping =====
const noteKeysGrid = document.getElementById('note-keys-grid');
const actionKeysGrid = document.getElementById('action-keys-grid');
const btnSave = document.getElementById('btn-save');
const btnReset = document.getElementById('btn-reset');
const statusMsg = document.getElementById('status-msg');

// Human-readable labels for actions
const ACTION_LABELS = {
    accent: 'Accent',
    slide: 'Slide',
    transpose_up: 'Transpose UP',
    transpose_down: 'Transpose DN',
    prev_step: 'Prev Step',
    next_step: 'Next Step',
    rest: 'Rest',
    rest_alt: 'Rest (alt)',
    randomize: 'Randomize',
    play: 'Play / Stop',
    live_toggle: 'Live Toggle',
};

// Note display order
const NOTE_ORDER = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B', 'C^'];
const ACTION_ORDER = [
    'accent', 'slide', 'transpose_up', 'transpose_down',
    'prev_step', 'next_step', 'rest', 'rest_alt',
    'randomize', 'play', 'live_toggle',
];

let config = null;
let listeningCell = null; // currently capturing a key press

function setStatus(msg) {
    statusMsg.textContent = msg;
}

function keyDisplayName(key) {
    if (!key) return '-';
    if (key === ' ') return 'Space';
    if (key === '\\') return '\\';
    if (key === ',') return ',';
    if (key === ';') return ';';
    if (key === '.') return '.';
    if (key === '/') return '/';
    if (key.startsWith('ctrl+')) return 'Ctrl + ' + key.slice(5).toUpperCase();
    if (key.startsWith('Arrow')) return key.replace('Arrow', '');
    if (key.startsWith('Numpad')) return 'Numpad ' + key.slice(6);
    if (key.length === 1) return key.toUpperCase();
    return key;
}

function createKeyCell(label, currentKey, onCapture) {
    const row = document.createElement('div');
    row.className = 'flex items-center justify-between bg-surface-container rounded-lg px-3 py-2';

    const lbl = document.createElement('span');
    lbl.className = 'text-[1rem] font-black text-on-surface tracking-wider';
    lbl.textContent = label;

    const btn = createButton({
        className: 'settings-key-btn tactile-button',
        label: keyDisplayName(currentKey),
    });

    btn.addEventListener('click', () => {
        if (listeningCell) {
            // Cancel previous listener
            listeningCell.btn.textContent = keyDisplayName(listeningCell.currentKey);
            listeningCell.btn.classList.remove('is-capturing');
        }
        listeningCell = { btn, currentKey, onCapture };
        btn.textContent = 'Press key...';
        btn.classList.add('is-capturing');
    });

    row.appendChild(lbl);
    row.appendChild(btn);
    return row;
}

function renderConfig() {
    noteKeysGrid.innerHTML = '';
    actionKeysGrid.innerHTML = '';

    // Note keys
    for (const note of NOTE_ORDER) {
        const key = config.notes[note] || '';
        noteKeysGrid.appendChild(createKeyCell(note, key, (newKey) => {
            config.notes[note] = newKey;
            renderConfig();
        }));
    }

    // Action keys
    for (const action of ACTION_ORDER) {
        const key = config.actions[action] || '';
        const label = ACTION_LABELS[action] || action;
        actionKeysGrid.appendChild(createKeyCell(label, key, (newKey) => {
            config.actions[action] = newKey;
            renderConfig();
        }));
    }
}

// Global key capture listener
document.addEventListener('keydown', (e) => {
    if (!listeningCell) return;
    e.preventDefault();
    e.stopPropagation();

    // Escape cancels
    if (e.key === 'Escape') {
        listeningCell.btn.textContent = keyDisplayName(listeningCell.currentKey);
        listeningCell.btn.classList.remove('is-capturing');
        listeningCell = null;
        return;
    }

    let captured;
    if (e.ctrlKey && e.key !== 'Control') {
        captured = 'ctrl+' + e.key.toLowerCase();
    } else if (e.code && e.code.startsWith('Numpad')) {
        captured = e.code; // preserve physical key identity (e.g. Numpad0)
    } else if (e.key === 'Control' || e.key === 'Shift' || e.key === 'Alt' || e.key === 'Meta') {
        return; // ignore bare modifiers
    } else {
        captured = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    }

    listeningCell.onCapture(captured);
    listeningCell.btn.classList.remove('is-capturing');
    listeningCell = null;
});

// Save keyboard config
btnSave.addEventListener('click', async () => {
    try {
        await api.saveKeyboardConfig(config);
        setStatus('Saved keyboard-config.json');
    } catch (err) {
        setStatus('Save error: ' + err.message);
    }
});

// Reset keyboard to defaults (fetches from immutable defaults file)
btnReset.addEventListener('click', async () => {
    try {
        config = await fetchJson('/config/keyboard-defaults.json');
        renderConfig();
        setStatus('Reset to defaults (click SAVE to persist)');
    } catch (err) {
        setStatus('Failed to load defaults: ' + err.message);
    }
});

// ===== Scales Editor =====
const scalesList = document.getElementById('scales-list');
const btnAddScale = document.getElementById('btn-add-scale');
const btnSaveScales = document.getElementById('btn-save-scales');
const btnResetScales = document.getElementById('btn-reset-scales');
const statusMsgScales = document.getElementById('status-msg-scales');

const NOTE_LABELS = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
const TAG_OPTIONS = ['safe', 'dark', 'tension', 'jazz', 'custom'];

let scalesConfig = null;

function setScalesStatus(msg) {
    statusMsgScales.textContent = msg;
}

function makeScaleId(name) {
    return name.toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/^_|_$/g, '') || 'custom_scale';
}

function createScaleCard(scale, index) {
    const card = document.createElement('div');
    card.className = 'bg-surface-container rounded-lg p-4';

    // Top row: name + tag + delete
    const topRow = document.createElement('div');
    topRow.className = 'flex items-center gap-3 mb-3';

    const nameInput = document.createElement('input');
    nameInput.type = 'text';
    nameInput.value = scale.name;
    nameInput.className = 'flex-1 h-9 px-3 rounded bg-surface-container-highest text-on-surface text-[1rem] font-bold border border-outline-variant focus:border-primary-fixed outline-none';
    nameInput.addEventListener('input', () => {
        scale.name = nameInput.value;
        scale.id = makeScaleId(nameInput.value);
    });

    const tagSelect = document.createElement('select');
    tagSelect.className = 'h-9 px-2 rounded bg-surface-container-highest text-on-surface-variant text-[1rem] font-bold border border-outline-variant focus:border-primary-fixed outline-none';
    for (const tag of TAG_OPTIONS) {
        const opt = document.createElement('option');
        opt.value = tag;
        opt.textContent = tag.toUpperCase();
        if (scale.tags && scale.tags.includes(tag)) opt.selected = true;
        tagSelect.appendChild(opt);
    }
    tagSelect.addEventListener('change', () => {
        scale.tags = [tagSelect.value];
    });

    const btnDelete = createButton({
        icon: 'delete',
        className: 'settings-scale-delete-btn tactile-button',
        ariaLabel: `Delete ${scale.name}`,
        onClick: () => {
            scalesConfig.scales.splice(index, 1);
            renderScales();
        },
    });

    topRow.appendChild(nameInput);
    topRow.appendChild(tagSelect);
    topRow.appendChild(btnDelete);

    // Interval row: 12 note toggles (piano-style)
    const intervalRow = document.createElement('div');
    intervalRow.className = 'flex gap-1';

    const intervalSet = new Set(scale.intervals || []);

    for (let n = 0; n < 12; n++) {
        const isBlack = [1, 3, 6, 8, 10].includes(n);
        const active = intervalSet.has(n);

        const btn = createButton({
            className: scaleNoteButtonClass({ active, isBlack }),
            label: NOTE_LABELS[n],
        });

        btn.addEventListener('click', () => {
            if (n === 0) return; // root is always included
            if (intervalSet.has(n)) {
                intervalSet.delete(n);
            } else {
                intervalSet.add(n);
            }
            scale.intervals = [...intervalSet].sort((a, b) => a - b);
            renderScales();
        });

        intervalRow.appendChild(btn);
    }

    card.appendChild(topRow);
    card.appendChild(intervalRow);
    return card;
}

function scaleNoteButtonClass({ active, isBlack }) {
    const parts = ['settings-scale-note-btn tactile-button'];
    if (active) parts.push('is-active');
    if (isBlack) parts.push('is-black');
    return parts.join(' ');
}

function renderScales() {
    scalesList.innerHTML = '';
    if (!scalesConfig || !scalesConfig.scales) return;
    scalesConfig.scales.forEach((scale, i) => {
        scalesList.appendChild(createScaleCard(scale, i));
    });
}

// Add new custom scale
btnAddScale.addEventListener('click', () => {
    if (!scalesConfig) return;
    const newScale = {
        id: 'custom_' + Date.now(),
        name: 'New Scale',
        intervals: [0, 3, 5, 7, 10],  // minor pentatonic as starter
        tags: ['custom'],
    };
    scalesConfig.scales.push(newScale);
    renderScales();
    // Scroll to bottom
    scalesList.lastElementChild?.scrollIntoView({ behavior: 'smooth' });
});

// Save scales config
btnSaveScales.addEventListener('click', async () => {
    try {
        await api.saveScalesConfig(scalesConfig);
        setScalesStatus('Saved scales-config.json');
    } catch (err) {
        setScalesStatus('Save error: ' + err.message);
    }
});

// Reset scales to defaults (fetches from immutable defaults file)
btnResetScales.addEventListener('click', async () => {
    try {
        scalesConfig = await fetchJson('/config/scales-defaults.json');
        renderScales();
        setScalesStatus('Reset to defaults (click SAVE to persist)');
    } catch (err) {
        setScalesStatus('Failed to load defaults: ' + err.message);
    }
});

// ===== Init =====
(async () => {
    // Load keyboard config from API (reads config/keyboard-config.json with embedded fallback)
    try {
        config = await api.getKeyboardConfig();
    } catch (err) {
        setStatus('Failed to load keyboard config: ' + err.message);
        config = { notes: {}, actions: {} };
    }
    renderConfig();

    // Load scales config from API (reads config/scales-config.json with embedded fallback)
    try {
        scalesConfig = await api.getScalesConfig();
    } catch (err) {
        setScalesStatus('Failed to load scales config: ' + err.message);
        scalesConfig = { tag_groups: [], scales: [] };
    }
    renderScales();
})();
