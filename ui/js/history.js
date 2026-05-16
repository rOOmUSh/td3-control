// IndexedDB-backed undo/redo history.
//
// Independent stores per page mode: 'single_history' (legacy main-page
// single-pattern), 'multipattern_history' (new main-page multi-pattern),
// and 'progression_history'. Linear timeline - a new change after undo
// discards forward entries. Persists across page refreshes. Unlimited
// until disk fills.
//
// single + progression + backup + multipattern_history covered.


const DB_NAME = 'td3_history';
const DB_VERSION = 2;
const STORES = {
    single: 'single_history',
    progression: 'progression_history',
    multipattern: 'multipattern_history',
    backup: 'device_backup',
};

let db = null;

/** Open (or create) the IndexedDB database. Call once at startup. */
export function open() {
    return new Promise((resolve, reject) => {
        const req = indexedDB.open(DB_NAME, DB_VERSION);
        req.onupgradeneeded = (e) => {
            const d = e.target.result;
            if (!d.objectStoreNames.contains(STORES.single)) {
                d.createObjectStore(STORES.single, { keyPath: 'id', autoIncrement: true });
            }
            if (!d.objectStoreNames.contains(STORES.progression)) {
                d.createObjectStore(STORES.progression, { keyPath: 'id', autoIncrement: true });
            }
            if (!d.objectStoreNames.contains(STORES.multipattern)) {
                d.createObjectStore(STORES.multipattern, { keyPath: 'id', autoIncrement: true });
            }
            if (!d.objectStoreNames.contains(STORES.backup)) {
                d.createObjectStore(STORES.backup, { keyPath: 'id', autoIncrement: true });
            }
        };
        req.onsuccess = (e) => { db = e.target.result; resolve(); };
        req.onerror = (e) => { reject(e.target.error); };
    });
}

// ---------------------------------------------------------------------------
// Generic IndexedDB helpers
// ---------------------------------------------------------------------------

function tx(storeName, mode) {
    return db.transaction(storeName, mode).objectStore(storeName);
}

function idbRequest(req) {
    return new Promise((resolve, reject) => {
        req.onsuccess = () => resolve(req.result);
        req.onerror = () => reject(req.error);
    });
}

// ---------------------------------------------------------------------------
// Undo/Redo cursor - one per mode
// ---------------------------------------------------------------------------

// Cursor state: { position, maxId } stored per mode in memory.
// position = the id of the current entry.
// On push: append entry, discard everything after current position, update cursor.
// On undo: move cursor back one entry.
// On redo: move cursor forward one entry.

const cursors = {
    single: { position: null },
    progression: { position: null },
    multipattern: { position: null },
};

/**
 * Initialize cursor for a mode by reading the latest entry id.
 * Call after open().
 */
export async function initCursor(mode) {
    const storeName = STORES[mode];
    const store = tx(storeName, 'readonly');
    const req = store.openCursor(null, 'prev'); // last entry
    const cursor = await idbRequest(req);
    cursors[mode].position = cursor ? cursor.key : null;
}

/**
 * Push a new state snapshot. Discards any entries after current position (linear undo).
 * @param {string} mode - 'single' or 'progression'
 * @param {object} state - the state snapshot to store
 */
export async function push(mode, state) {
    const storeName = STORES[mode];
    const pos = cursors[mode].position;

    // Delete all entries after current position (discard redo branch)
    if (pos !== null) {
        const t = db.transaction(storeName, 'readwrite');
        const store = t.objectStore(storeName);
        const range = IDBKeyRange.lowerBound(pos, true); // exclusive: everything > pos
        store.delete(range);
        await new Promise((resolve, reject) => {
            t.oncomplete = resolve;
            t.onerror = () => reject(t.error);
        });
    }

    // Add new entry
    const store = tx(storeName, 'readwrite');
    const id = await idbRequest(store.add({
        state: JSON.parse(JSON.stringify(state)),
        timestamp: Date.now(),
    }));
    cursors[mode].position = id;
}

/**
 * Undo - move cursor back one entry. Returns the restored state or null if at beginning.
 * @param {string} mode - 'single' or 'progression'
 */
export async function undo(mode) {
    const storeName = STORES[mode];
    const pos = cursors[mode].position;
    if (pos === null) return null;

    // Find the entry before current position
    const store = tx(storeName, 'readonly');
    const range = IDBKeyRange.upperBound(pos, true); // exclusive: everything < pos
    const req = store.openCursor(range, 'prev'); // last entry before pos
    const cursor = await idbRequest(req);
    if (!cursor) return null; // already at the beginning

    cursors[mode].position = cursor.key;
    return cursor.value.state;
}

/**
 * Redo - move cursor forward one entry. Returns the restored state or null if at end.
 * @param {string} mode - 'single' or 'progression'
 */
export async function redo(mode) {
    const storeName = STORES[mode];
    const pos = cursors[mode].position;

    // Find the entry after current position
    const store = tx(storeName, 'readonly');
    let range;
    if (pos === null) {
        range = null; // get first entry ever
    } else {
        range = IDBKeyRange.lowerBound(pos, true); // exclusive: everything > pos
    }
    const req = store.openCursor(range, 'next'); // first entry after pos
    const cursor = await idbRequest(req);
    if (!cursor) return null; // already at the end

    cursors[mode].position = cursor.key;
    return cursor.value.state;
}

/**
 * Get current cursor position info for status display.
 * Returns { canUndo, canRedo }.
 */
export async function getCursorInfo(mode) {
    const storeName = STORES[mode];
    const pos = cursors[mode].position;

    let canUndo = false;
    let canRedo = false;

    if (pos !== null) {
        // Check if there's an entry before
        const store1 = tx(storeName, 'readonly');
        const range1 = IDBKeyRange.upperBound(pos, true);
        const req1 = store1.openCursor(range1, 'prev');
        const c1 = await idbRequest(req1);
        canUndo = !!c1;
    }

    // Check if there's an entry after
    const store2 = tx(storeName, 'readonly');
    let range2;
    if (pos === null) {
        range2 = null;
    } else {
        range2 = IDBKeyRange.lowerBound(pos, true);
    }
    const req2 = store2.openCursor(range2, 'next');
    const c2 = await idbRequest(req2);
    canRedo = !!c2;

    return { canUndo, canRedo };
}

// ---------------------------------------------------------------------------
// Device backup store
// ---------------------------------------------------------------------------

/**
 * Store a full device bank backup.
 * @param {Array} patterns - array of { group, pattern, side, data }
 * @param {string} firmware - firmware version string
 */
export async function storeBackup(patterns, firmware) {
    const store = tx(STORES.backup, 'readwrite');
    await idbRequest(store.add({
        patterns,
        firmware: firmware || 'unknown',
        timestamp: Date.now(),
    }));
}

/**
 * Get the most recent device backup, or null if none exists.
 */
export async function getLatestBackup() {
    const store = tx(STORES.backup, 'readonly');
    const req = store.openCursor(null, 'prev');
    const cursor = await idbRequest(req);
    return cursor ? cursor.value : null;
}

/**
 * Check whether any backup exists in the store.
 */
export async function hasBackup() {
    const store = tx(STORES.backup, 'readonly');
    const req = store.count();
    const count = await idbRequest(req);
    return count > 0;
}
