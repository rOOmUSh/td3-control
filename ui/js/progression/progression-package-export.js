// Progression package export - owns the Save Package UI state and POSTs the
// selected per-pattern and combined formats to the backend ZIP assembler.
//
// The module is intentionally split between a pure core (format-state
// helpers and payload builder, exercised by tests) and a thin DOM wiring
// layer (init/setPackage/clearPackage). The pure core never touches the
// DOM so it can run under Node for tests.

const FORMATS_STORAGE_KEY = 'td3.progression.export.formats.v1';

// Default checkbox state.
export const DEFAULT_FORMATS = Object.freeze({
    mid: true,
    steps_txt: true,
    seq: true,
    pat: false,
    rbs: false,
    json: false,
    toml: false,
    combined_rbs: false,
    combined_sqs: false,
});

export const PER_PATTERN_KEYS = Object.freeze(
    ['mid', 'steps_txt', 'seq', 'pat', 'rbs', 'json', 'toml']
);
export const COMBINED_KEYS = Object.freeze(['combined_rbs', 'combined_sqs']);

function cloneDefaults() {
    return { ...DEFAULT_FORMATS };
}

export function loadFormatsFromStorage(storage) {
    const store = storage || (typeof localStorage !== 'undefined' ? localStorage : null);
    if (!store) return cloneDefaults();
    try {
        const raw = store.getItem(FORMATS_STORAGE_KEY);
        if (!raw) return cloneDefaults();
        const parsed = JSON.parse(raw);
        if (!parsed || typeof parsed !== 'object') return cloneDefaults();
        const out = cloneDefaults();
        for (const k of Object.keys(out)) {
            if (typeof parsed[k] === 'boolean') out[k] = parsed[k];
        }
        return out;
    } catch (_) {
        return cloneDefaults();
    }
}

export function saveFormatsToStorage(formats, storage) {
    const store = storage || (typeof localStorage !== 'undefined' ? localStorage : null);
    if (!store) return;
    try {
        const clean = {};
        for (const k of Object.keys(DEFAULT_FORMATS)) {
            clean[k] = !!formats[k];
        }
        store.setItem(FORMATS_STORAGE_KEY, JSON.stringify(clean));
    } catch (_) {
        // quota or disabled storage - selection simply won't persist
    }
}

export function anyFormatSelected(formats) {
    if (!formats) return false;
    for (const k of PER_PATTERN_KEYS) if (formats[k]) return true;
    for (const k of COMBINED_KEYS) if (formats[k]) return true;
    return false;
}

export function buildExportPayload({
    packageId, label, scaleName,
    acidPatterns, basslines, basslinesFull,
    formats,
}) {
    if (!packageId) throw new Error('packageId is required');
    if (!Array.isArray(acidPatterns) || acidPatterns.length !== 4) {
        throw new Error('acidPatterns must have length 4');
    }
    if (!Array.isArray(basslines) || basslines.length !== 4) {
        throw new Error('basslines must have length 4');
    }
    if (basslinesFull != null) {
        if (!Array.isArray(basslinesFull) || basslinesFull.length !== 20) {
            throw new Error('basslinesFull must have length 20 when provided');
        }
    }
    const perPattern = PER_PATTERN_KEYS.filter(k => !!(formats && formats[k]));
    const combinedRbs = !!(formats && formats.combined_rbs);
    const combinedSqs = !!(formats && formats.combined_sqs);
    if (perPattern.length === 0 && !combinedRbs && !combinedSqs) {
        throw new Error('Select at least one format');
    }
    const payload = {
        packageId,
        formats: perPattern,
        combinedFormats: { rbs: combinedRbs, sqs: combinedSqs },
        scaleName: scaleName || '',
        label: label || '',
        acidPatterns,
        basslines,
    };
    if (basslinesFull) payload.basslinesFull = basslinesFull;
    return payload;
}

// ---------------------------------------------------------------------------
// DOM wiring
// ---------------------------------------------------------------------------

let pkgState = null;
let formats = null;
let setStatusFn = () => {};
let exportFn = null;
let btnSave = null;
let pkgStatusEl = null;
let pkgLastSavedEl = null;
let formatCheckboxes = new Map();
let inFlight = false;

export function init(opts = {}) {
    setStatusFn = typeof opts.setStatus === 'function' ? opts.setStatus : setStatusFn;
    exportFn = typeof opts.exportFn === 'function' ? opts.exportFn : null;

    btnSave = document.getElementById('btn-save-package');
    pkgStatusEl = document.getElementById('pkg-status');
    pkgLastSavedEl = document.getElementById('pkg-last-saved');

    formats = loadFormatsFromStorage();
    formatCheckboxes = new Map();
    for (const k of Object.keys(DEFAULT_FORMATS)) {
        const cb = document.getElementById(`pkg-fmt-${k}`);
        if (!cb) continue;
        cb.checked = !!formats[k];
        cb.addEventListener('change', () => {
            formats[k] = !!cb.checked;
            saveFormatsToStorage(formats);
            updateButtonState();
        });
        formatCheckboxes.set(k, cb);
    }

    if (btnSave) btnSave.addEventListener('click', () => { void save(); });
    updateButtonState();
    renderPackageLabel();
}

export function setPackage(pkg) {
    if (!pkg) { clearPackage(); return; }
    if (!Array.isArray(pkg.acidPatterns) || pkg.acidPatterns.length !== 4) return;
    if (!Array.isArray(pkg.basslines) || pkg.basslines.length !== 4) return;
    // basslinesFull is optional: when the generator has produced all 5
    // archetype variants per position we carry the flat 20-entry matrix
    // through to the backend for combined-format placement. If the caller
    // omits it or hands a wrong-length array we simply fall back to the 4
    // active basslines (combined exports will use legacy G1 B-side layout).
    const basslinesFull = Array.isArray(pkg.basslinesFull) && pkg.basslinesFull.length === 20
        ? pkg.basslinesFull
        : undefined;
    pkgState = {
        packageId: pkg.packageId,
        label: pkg.label || '',
        scaleName: pkg.scaleName || '',
        acidPatterns: pkg.acidPatterns,
        basslines: pkg.basslines,
        basslinesFull,
    };
    updateButtonState();
    renderPackageLabel();
}

export function clearPackage() {
    pkgState = null;
    updateButtonState();
    renderPackageLabel();
}

function updateButtonState() {
    if (!btnSave) return;
    const hasPkg = !!pkgState;
    const hasFmt = anyFormatSelected(formats);
    btnSave.disabled = inFlight || !(hasPkg && hasFmt);
    if (inFlight) btnSave.title = 'Exporting…';
    else if (!hasPkg) btnSave.title = 'Generate a progression first';
    else if (!hasFmt) btnSave.title = 'Select at least one format';
    else btnSave.title = 'Save package as ZIP';
}

function renderPackageLabel() {
    if (!pkgStatusEl) return;
    pkgStatusEl.textContent = pkgState ? (pkgState.label || '(unnamed package)') : 'No package';
}

async function save() {
    if (!pkgState || inFlight) return;
    let payload;
    try {
        payload = buildExportPayload({ ...pkgState, formats });
    } catch (err) {
        setStatusFn(err.message);
        return;
    }
    if (!exportFn) {
        setStatusFn('Export failed: no export transport configured');
        return;
    }
    inFlight = true;
    updateButtonState();
    setStatusFn('Exporting...');
    try {
        const res = await exportFn(payload);
        if (res && res.ok) {
            setStatusFn(`Saved: ${res.zipName}`);
            if (pkgLastSavedEl) pkgLastSavedEl.textContent = res.zipName;
        } else {
            const msg = (res && res.error) ? res.error : 'unknown error';
            setStatusFn(`Export failed: ${msg}`);
        }
    } catch (err) {
        setStatusFn(`Export failed: ${err.message}`);
    } finally {
        inFlight = false;
        updateButtonState();
    }
}

// Exported for tests only.
export const __private = {
    FORMATS_STORAGE_KEY,
};
