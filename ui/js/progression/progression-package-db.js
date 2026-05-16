// Progression package persistence - IndexedDB-backed.
//
// v3 (April 2026): each position carries 5 archetype variants of the supporting
// bassline (pedal / rootPulse / offbeat / shadow / arpeggio). The package now
// persists all 20 bassline rows (5 archetypes × 4 positions) so the user can
// reload the page and still audition every variant - not just the active one.
//
// Schema (3 object stores in a single DB):
//   packages  - primary key `packageId`, index `createdAt`
//     row carries `packageVersion` (3 for current writes, undefined for legacy)
//     and `defaultArchetypeByPattern: string[4]` so restore knows which chip
//     to light.
//   patterns  - primary key `patternId`, index `packageId` (acid layer only)
//   basslines - primary key `basslineId`, indexes `packageId` and
//     `sourcePatternId`. v3 rows include an `archetype` field (one of
//     ARCHETYPE_KEYS); legacy rows have no `archetype`.
//
// Write semantics: savePackage() writes all 25 rows (1 package, 4 acid
// patterns, 20 basslines) in a SINGLE transaction. If any write fails, the
// whole transaction rolls back and no partial package is persisted.

const DB_NAME = 'td3-progression-packages-v1';
const DB_VERSION = 1;
const STORES = {
    packages: 'packages',
    patterns: 'patterns',
    basslines: 'basslines',
};

let db = null;

/** Open (or create) the IndexedDB database. Call once at startup. */
export function open() {
    return new Promise((resolve, reject) => {
        const req = indexedDB.open(DB_NAME, DB_VERSION);
        req.onupgradeneeded = (e) => {
            const d = e.target.result;
            if (!d.objectStoreNames.contains(STORES.packages)) {
                const s = d.createObjectStore(STORES.packages, { keyPath: 'packageId' });
                s.createIndex('createdAt', 'createdAt', { unique: false });
            }
            if (!d.objectStoreNames.contains(STORES.patterns)) {
                const s = d.createObjectStore(STORES.patterns, { keyPath: 'patternId' });
                s.createIndex('packageId', 'packageId', { unique: false });
            }
            if (!d.objectStoreNames.contains(STORES.basslines)) {
                const s = d.createObjectStore(STORES.basslines, { keyPath: 'basslineId' });
                s.createIndex('packageId', 'packageId', { unique: false });
                s.createIndex('sourcePatternId', 'sourcePatternId', { unique: false });
            }
        };
        req.onsuccess = (e) => { db = e.target.result; resolve(); };
        req.onerror = (e) => { reject(e.target.error); };
    });
}

/** Close the underlying connection. Primarily for tests. */
export function close() {
    if (db) { db.close(); db = null; }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function idbRequest(req) {
    return new Promise((resolve, reject) => {
        req.onsuccess = () => resolve(req.result);
        req.onerror = () => reject(req.error);
    });
}

/**
 * Generate a unique id. Uses crypto.randomUUID() when available, otherwise a
 * timestamp+random fallback sufficient for in-browser uniqueness within a DB.
 */
export function newId(prefix) {
    const rnd = (typeof crypto !== 'undefined' && crypto.randomUUID)
        ? crypto.randomUUID()
        : `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
    return prefix ? `${prefix}_${rnd}` : rnd;
}

// ---------------------------------------------------------------------------
// Build rows from in-memory generator output
// ---------------------------------------------------------------------------

const ROLE_BY_POSITION = ['home', 'move_away', 'tension', 'resolve'];
export const ARCHETYPE_KEYS = Object.freeze(['pedal', 'rootPulse', 'offbeat', 'shadow', 'arpeggio']);
export const PACKAGE_VERSION = 3;

/**
 * Build the three-tier row set from the inputs the generator produced. Caller
 * passes the raw generator result plus the progression context needed to
 * reconstruct the package later.
 *
 * Pure: does not touch IndexedDB. savePackage() does the actual write.
 *
 * @param {Object} ctx
 * @param {number|null} ctx.seed
 * @param {number}      ctx.root
 * @param {string}      ctx.scaleId
 * @param {string}      ctx.scaleName
 * @param {string}      ctx.profile
 * @param {number[]}    ctx.degrees
 * @param {string}      ctx.label
 * @param {number[]}    ctx.timeline
 * @param {string}      ctx.rhythmMode
 * @param {Array}       ctx.acidPatterns            length 4, raw pattern objects
 * @param {Array}       ctx.basslinesByPattern      length 4, each entry is a
 *                                                  map {pedal, rootPulse,
 *                                                  offbeat, shadow, arpeggio}
 *                                                  of raw pattern objects.
 * @param {string[]}    ctx.defaultArchetypeByPattern length 4, one of
 *                                                  ARCHETYPE_KEYS per position.
 * @param {Object}      ctx.harmonicMap             used to derive centerPc + degree per position
 * @returns {{pkg:Object, patternRows:Array, basslineRows:Array}}
 */
export function buildRows(ctx) {
    if (!ctx || !Array.isArray(ctx.acidPatterns) || ctx.acidPatterns.length !== 4) {
        throw new Error('progression-package-db: acidPatterns must be an array of length 4');
    }
    if (!Array.isArray(ctx.basslinesByPattern) || ctx.basslinesByPattern.length !== 4) {
        throw new Error('progression-package-db: basslinesByPattern must be an array of length 4');
    }
    for (let i = 0; i < 4; i++) {
        const set = ctx.basslinesByPattern[i];
        if (!set || typeof set !== 'object') {
            throw new Error(`progression-package-db: basslinesByPattern[${i}] missing`);
        }
        for (const key of ARCHETYPE_KEYS) {
            if (!set[key]) throw new Error(`progression-package-db: basslinesByPattern[${i}].${key} missing`);
        }
    }
    if (!Array.isArray(ctx.defaultArchetypeByPattern) || ctx.defaultArchetypeByPattern.length !== 4) {
        throw new Error('progression-package-db: defaultArchetypeByPattern must be an array of length 4');
    }
    for (const key of ctx.defaultArchetypeByPattern) {
        if (!ARCHETYPE_KEYS.includes(key)) {
            throw new Error(`progression-package-db: defaultArchetypeByPattern has invalid key "${key}"`);
        }
    }
    if (!ctx.harmonicMap || !Array.isArray(ctx.harmonicMap.centers) || ctx.harmonicMap.centers.length !== 4) {
        throw new Error('progression-package-db: harmonicMap.centers must have length 4');
    }

    const packageId = newId('pkg');
    const createdAt = new Date().toISOString();

    const patternRows = ctx.acidPatterns.map((p, i) => ({
        patternId: newId('pat'),
        packageId,
        position: i + 1,
        role: ROLE_BY_POSITION[i],
        layer: 'acid',
        pattern: p,
    }));

    // 20 bassline rows: 5 archetypes × 4 positions. Each row tagged with both
    // position (1..4) and archetype so restore can rebuild the 5×4 map by
    // grouping on (position, archetype). Ordering within the array is
    // position-major, archetype-minor to keep scans predictable.
    const basslineRows = [];
    for (let i = 0; i < 4; i++) {
        const center = ctx.harmonicMap.centers[i];
        for (const archetype of ARCHETYPE_KEYS) {
            basslineRows.push({
                basslineId: newId('bass'),
                packageId,
                sourcePatternId: patternRows[i].patternId,
                position: i + 1,
                archetype,
                layer: 'supporting_bassline',
                pattern: ctx.basslinesByPattern[i][archetype],
                meta: {
                    centerPc: center.centerPc,
                    degree: center.degree,
                    profile: ctx.profile,
                    rhythmMode: ctx.rhythmMode,
                },
            });
        }
    }

    const pkg = {
        packageId,
        packageVersion: PACKAGE_VERSION,
        createdAt,
        seed: ctx.seed ?? null,
        root: ctx.root,
        scaleId: ctx.scaleId,
        scaleName: ctx.scaleName,
        profile: ctx.profile,
        degrees: ctx.degrees,
        label: ctx.label,
        timeline: ctx.timeline,
        rhythmMode: ctx.rhythmMode,
        defaultArchetypeByPattern: ctx.defaultArchetypeByPattern.slice(),
        acidPatternIds: patternRows.map(r => r.patternId),
        basslineIds: basslineRows.map(r => r.basslineId),
    };

    return { pkg, patternRows, basslineRows };
}

// ---------------------------------------------------------------------------
// Writes
// ---------------------------------------------------------------------------

/**
 * Persist a package (pkg + 4 acid patterns + 4 basslines) in a single
 * transaction. Atomic - either all 9 rows land or none do.
 *
 * @param {Object} pkg
 * @param {Array}  patternRows
 * @param {Array}  basslineRows
 * @returns {Promise<string>} the packageId on success
 */
export function savePackage(pkg, patternRows, basslineRows) {
    return new Promise((resolve, reject) => {
        if (!db) return reject(new Error('progression-package-db: open() first'));
        const t = db.transaction(
            [STORES.packages, STORES.patterns, STORES.basslines],
            'readwrite'
        );
        t.oncomplete = () => resolve(pkg.packageId);
        t.onerror = () => reject(t.error);
        t.onabort = () => reject(t.error || new Error('transaction aborted'));

        try {
            t.objectStore(STORES.packages).put(pkg);
            const pStore = t.objectStore(STORES.patterns);
            for (const row of patternRows) pStore.put(row);
            const bStore = t.objectStore(STORES.basslines);
            for (const row of basslineRows) bStore.put(row);
        } catch (err) {
            // Defensive: put() throws synchronously on schema mismatch. Abort so
            // oncomplete does not fire; reject here with the actual error.
            try { t.abort(); } catch { /* ignore */ }
            reject(err);
        }
    });
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

/**
 * Read a package and all its associated rows.
 * @param {string} packageId
 * @returns {Promise<{package:Object, acidPatterns:Array, basslines:Array}|null>}
 */
export async function getPackage(packageId) {
    if (!db) throw new Error('progression-package-db: open() first');
    const pkg = await idbRequest(
        db.transaction(STORES.packages, 'readonly').objectStore(STORES.packages).get(packageId)
    );
    if (!pkg) return null;

    const patternsRaw = await idbRequest(
        db.transaction(STORES.patterns, 'readonly')
            .objectStore(STORES.patterns).index('packageId').getAll(packageId)
    );
    const basslinesRaw = await idbRequest(
        db.transaction(STORES.basslines, 'readonly')
            .objectStore(STORES.basslines).index('packageId').getAll(packageId)
    );

    const acidPatterns = [...patternsRaw].sort((a, b) => a.position - b.position);
    // v3 rows order by position-then-archetype so `basslines` reads
    // deterministically. Legacy v1/v2 packages have no `archetype` field; they
    // fall through this sort untouched (all sort as equal on archetype order
    // and keep their position ordering).
    const archetypeRank = (a) => {
        const order = { pedal: 0, rootPulse: 1, offbeat: 2, shadow: 3, arpeggio: 4 };
        return order[a] ?? 0;
    };
    const basslines = [...basslinesRaw].sort((a, b) => {
        if (a.position !== b.position) return a.position - b.position;
        return archetypeRank(a.archetype) - archetypeRank(b.archetype);
    });

    return { package: pkg, acidPatterns, basslines };
}

/**
 * Reshape the flat basslines array returned by getPackage() into a 5×4 map
 * keyed by archetype per position. Caller passes the `basslines` array; a
 * legacy (v1/v2) package with 4 rows and no `archetype` field returns null so
 * callers can take the legacy path.
 *
 * @param {Array} basslines
 * @returns {{
 *   byPattern: Array<{pedal, rootPulse, offbeat, shadow, arpeggio}>,
 *   active: string[]
 * } | null}
 */
export function reshapeBasslines(basslines) {
    if (!Array.isArray(basslines) || basslines.length !== 20) return null;
    const byPattern = [null, null, null, null];
    for (const row of basslines) {
        if (!ARCHETYPE_KEYS.includes(row.archetype)) return null;
        const i = (row.position | 0) - 1;
        if (i < 0 || i > 3) return null;
        if (!byPattern[i]) byPattern[i] = {};
        byPattern[i][row.archetype] = row.pattern;
    }
    for (const set of byPattern) {
        if (!set) return null;
        for (const key of ARCHETYPE_KEYS) if (!set[key]) return null;
    }
    return { byPattern, keys: ARCHETYPE_KEYS.slice() };
}

/**
 * Return the most recently created package (by createdAt index), or null.
 * @returns {Promise<{package:Object, acidPatterns:Array, basslines:Array}|null>}
 */
export async function getLatestPackage() {
    if (!db) throw new Error('progression-package-db: open() first');
    const store = db.transaction(STORES.packages, 'readonly').objectStore(STORES.packages);
    const cursor = await idbRequest(store.index('createdAt').openCursor(null, 'prev'));
    if (!cursor) return null;
    return getPackage(cursor.value.packageId);
}

/**
 * List all packages (metadata only) ordered by createdAt descending.
 * @returns {Promise<Array>}
 */
export async function listPackages() {
    if (!db) throw new Error('progression-package-db: open() first');
    const store = db.transaction(STORES.packages, 'readonly').objectStore(STORES.packages);
    const rows = await idbRequest(store.index('createdAt').getAll());
    return [...rows].sort((a, b) => (a.createdAt < b.createdAt ? 1 : -1));
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

/**
 * Cascade-delete a package and all its rows in a single transaction.
 * @param {string} packageId
 */
export function deletePackage(packageId) {
    return new Promise((resolve, reject) => {
        if (!db) return reject(new Error('progression-package-db: open() first'));
        const t = db.transaction(
            [STORES.packages, STORES.patterns, STORES.basslines],
            'readwrite'
        );
        t.oncomplete = () => resolve();
        t.onerror = () => reject(t.error);
        t.onabort = () => reject(t.error || new Error('transaction aborted'));

        t.objectStore(STORES.packages).delete(packageId);

        const patReq = t.objectStore(STORES.patterns).index('packageId').openCursor(packageId);
        patReq.onsuccess = (e) => {
            const c = e.target.result;
            if (c) { c.delete(); c.continue(); }
        };

        const bassReq = t.objectStore(STORES.basslines).index('packageId').openCursor(packageId);
        bassReq.onsuccess = (e) => {
            const c = e.target.result;
            if (c) { c.delete(); c.continue(); }
        };
    });
}
