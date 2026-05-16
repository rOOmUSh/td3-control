// Tests for progression-package-db.js - runs with Node.js
// Usage: node ui/js/progression/progression-package-db.test.js
//
// IndexedDB is not available in Node, so we install a tiny in-memory shim on
// globalThis before importing the module. The shim covers exactly the surface
// the module uses:
//   - indexedDB.open(name, version) returning IDBOpenDBRequest with onupgradeneeded/onsuccess
//   - db.createObjectStore(name, { keyPath }) with createIndex(name, keyPath)
//   - db.transaction(storeNames, mode) with objectStore(name), onerror, oncomplete, onabort
//   - store.put(row), store.get(key), store.delete(key)
//   - store.index(name).getAll(key?), store.index(name).openCursor(range, direction?)
//
// Keys are compared as strings (good enough for our UUIDs + ISO timestamps).

// --- IndexedDB shim ---------------------------------------------------------

function clone(v) { return v === undefined ? v : JSON.parse(JSON.stringify(v)); }

function nextTick(fn) {
    if (typeof queueMicrotask === 'function') queueMicrotask(fn);
    else Promise.resolve().then(fn);
}

class Request {
    constructor() {
        this.onsuccess = null;
        this.onerror = null;
        this.result = undefined;
        this.error = null;
    }
    _succeed(result) {
        this.result = result;
        nextTick(() => { if (this.onsuccess) this.onsuccess({ target: this }); });
    }
    _fail(error) {
        this.error = error;
        nextTick(() => { if (this.onerror) this.onerror({ target: this }); });
    }
}

class Transaction {
    constructor(db, storeNames, mode) {
        this.db = db;
        this.mode = mode;
        this.storeNames = Array.isArray(storeNames) ? storeNames : [storeNames];
        this.oncomplete = null;
        this.onerror = null;
        this.onabort = null;
        this.error = null;
        this._aborted = false;
        this._pending = 0;
        this._done = false;
        // Schedule completion after any synchronous put/get calls are queued.
        nextTick(() => this._maybeComplete());
    }
    objectStore(name) {
        if (!this.storeNames.includes(name)) {
            throw new Error(`store ${name} not in transaction`);
        }
        return new ObjectStore(this, this.db._stores.get(name));
    }
    abort() {
        if (this._done) return;
        this._aborted = true;
        this._done = true;
        nextTick(() => { if (this.onabort) this.onabort({ target: this }); });
    }
    _maybeComplete() {
        if (this._done) return;
        if (this._aborted) return;
        if (this._pending > 0) return;
        this._done = true;
        nextTick(() => { if (this.oncomplete) this.oncomplete({ target: this }); });
    }
    _track(req, fn) {
        this._pending++;
        nextTick(() => {
            try {
                const result = fn();
                req._succeed(result);
            } catch (err) {
                req._fail(err);
                this.error = err;
                this.abort();
            } finally {
                this._pending--;
                this._maybeComplete();
            }
        });
    }
}

class ObjectStore {
    constructor(tx, storeData) {
        this._tx = tx;
        this._store = storeData; // { keyPath, rows: Map<key, row>, indexes: Map<name, keyPath> }
    }
    put(row) {
        const req = new Request();
        this._tx._track(req, () => {
            const key = row[this._store.keyPath];
            if (key === undefined) throw new Error('missing keyPath value');
            this._store.rows.set(key, clone(row));
            return key;
        });
        return req;
    }
    get(key) {
        const req = new Request();
        this._tx._track(req, () => {
            const v = this._store.rows.get(key);
            return v === undefined ? undefined : clone(v);
        });
        return req;
    }
    delete(key) {
        const req = new Request();
        this._tx._track(req, () => {
            this._store.rows.delete(key);
            return undefined;
        });
        return req;
    }
    index(name) {
        const keyPath = this._store.indexes.get(name);
        if (!keyPath) throw new Error(`no index ${name}`);
        return new Index(this._tx, this._store, keyPath);
    }
}

class Index {
    constructor(tx, storeData, keyPath) {
        this._tx = tx;
        this._store = storeData;
        this._keyPath = keyPath;
    }
    getAll(key) {
        const req = new Request();
        this._tx._track(req, () => {
            const out = [];
            for (const v of this._store.rows.values()) {
                if (key === undefined || v[this._keyPath] === key) out.push(clone(v));
            }
            return out;
        });
        return req;
    }
    openCursor(keyOrRange, direction) {
        // Support two forms the module uses:
        //   openCursor(null, 'prev') - iterate all, reverse-ordered by index key
        //   openCursor(exactKey)      - iterate only matching rows
        const req = new Request();
        this._tx._track(req, () => {
            const matches = [];
            for (const v of this._store.rows.values()) {
                if (keyOrRange === null || keyOrRange === undefined) matches.push(v);
                else if (v[this._keyPath] === keyOrRange) matches.push(v);
            }
            matches.sort((a, b) => {
                const ka = a[this._keyPath], kb = b[this._keyPath];
                if (ka === kb) return 0;
                return ka < kb ? -1 : 1;
            });
            if (direction === 'prev') matches.reverse();
            if (matches.length === 0) return null;

            // Cursor-like object with a `continue` method that re-fires onsuccess.
            let i = 0;
            const cursor = {
                get value() { return clone(matches[i]); },
                get key() { return matches[i] && matches[i][this._keyPath]; },
                delete() {
                    const row = matches[i];
                    const pk = row[cursor._storeKeyPath];
                    cursor._store.rows.delete(pk);
                    return { onsuccess: null, onerror: null };
                },
                continue() {
                    i++;
                    if (i >= matches.length) {
                        // Emit null to signal end-of-range
                        nextTick(() => { if (req.onsuccess) req.onsuccess({ target: { result: null } }); });
                    } else {
                        nextTick(() => { if (req.onsuccess) req.onsuccess({ target: { result: cursor } }); });
                    }
                },
                _store: this._store,
                _storeKeyPath: this._store.keyPath,
            };
            return cursor;
        });
        return req;
    }
}

class FakeDB {
    constructor(name, version) {
        this.name = name;
        this.version = version;
        this._stores = new Map();
        this.objectStoreNames = {
            contains: (n) => this._stores.has(n),
        };
    }
    createObjectStore(name, opts) {
        const storeData = {
            keyPath: opts.keyPath,
            rows: new Map(),
            indexes: new Map(),
        };
        this._stores.set(name, storeData);
        return {
            createIndex: (indexName, keyPath /*, opts*/) => {
                storeData.indexes.set(indexName, keyPath);
            },
        };
    }
    transaction(storeNames, mode) {
        return new Transaction(this, storeNames, mode);
    }
    close() { /* no-op for shim */ }
}

// Persistent store map keyed by DB name so reopen() preserves data.
const DB_STATE = new Map();

const fakeIndexedDB = {
    open(name, version) {
        const req = new Request();
        nextTick(() => {
            let db = DB_STATE.get(name);
            const isNew = !db;
            if (!db) {
                db = new FakeDB(name, version);
                DB_STATE.set(name, db);
            }
            if (isNew) {
                // Fire upgrade synchronously (within nextTick) so onsuccess sees the stores.
                if (req.onupgradeneeded) req.onupgradeneeded({ target: { result: db } });
            }
            req._succeed(db);
        });
        return req;
    },
    deleteDatabase(name) {
        const req = new Request();
        nextTick(() => {
            DB_STATE.delete(name);
            req._succeed(undefined);
        });
        return req;
    },
};

globalThis.indexedDB = fakeIndexedDB;
globalThis.IDBKeyRange = { bound: () => ({}), lowerBound: () => ({}), upperBound: () => ({}) };

// --- Test runner ------------------------------------------------------------

let passed = 0, failed = 0;

function assert(cond, msg) {
    if (!cond) { console.error(`  FAIL: ${msg}`); failed++; }
    else passed++;
}

async function test(name, fn) {
    try {
        await fn();
        console.log(`  ok: ${name}`);
    } catch (e) {
        console.error(`  FAIL: ${name}: ${e.stack || e.message}`);
        failed++;
    }
}

// --- Fixture builders -------------------------------------------------------

function makePattern(label) {
    return {
        active_steps: 16,
        triplet: false,
        steps: Array.from({ length: 16 }, (_, i) => ({
            note: 'C',
            transpose: 'NORMAL',
            accent: false,
            slide: false,
            time: 'NORMAL',
            _src: label,
            _idx: i,
        })),
    };
}

const ARCHETYPE_KEYS = ['pedal', 'rootPulse', 'offbeat', 'shadow', 'arpeggio'];

function makeArchetypeMap(posLabel) {
    const out = {};
    for (const key of ARCHETYPE_KEYS) {
        out[key] = makePattern(`${posLabel}-${key}`);
    }
    return out;
}

function makeCtx(overrides = {}) {
    return {
        seed: 12345,
        root: 0,
        scaleId: 'major',
        scaleName: 'Major / Ionian',
        profile: 'safe',
        degrees: [1, 5, 6, 4],
        label: 'C Major - I V vi IV',
        timeline: [1, 2, 3, 4],
        rhythmMode: 'four_on_floor',
        acidPatterns: ['P1', 'P2', 'P3', 'P4'].map(makePattern),
        basslinesByPattern: ['P1', 'P2', 'P3', 'P4'].map(makeArchetypeMap),
        defaultArchetypeByPattern: ['rootPulse', 'pedal', 'offbeat', 'shadow'],
        harmonicMap: {
            centers: [
                { centerPc: 0, degree: 1 },
                { centerPc: 7, degree: 5 },
                { centerPc: 9, degree: 6 },
                { centerPc: 5, degree: 4 },
            ],
        },
        ...overrides,
    };
}

// --- Tests ------------------------------------------------------------------

console.log('progression-package-db tests:');

// Import after shim install.
const db = await import('./progression-package-db.js');

await test('buildRows returns 1 pkg + 4 patternRows + 20 basslineRows', () => {
    const ctx = makeCtx();
    const { pkg, patternRows, basslineRows } = db.buildRows(ctx);
    assert(typeof pkg.packageId === 'string' && pkg.packageId.length > 0, 'pkg.packageId is a string');
    assert(pkg.packageVersion === 3, 'pkg.packageVersion === 3');
    assert(patternRows.length === 4, 'patternRows length 4');
    assert(basslineRows.length === 20, 'basslineRows length 20 (5 archetypes × 4 positions)');
    assert(pkg.acidPatternIds.length === 4, 'pkg.acidPatternIds length 4');
    assert(pkg.basslineIds.length === 20, 'pkg.basslineIds length 20');
    assert(Array.isArray(pkg.defaultArchetypeByPattern) && pkg.defaultArchetypeByPattern.length === 4,
        'pkg.defaultArchetypeByPattern length 4');
    assert(typeof pkg.createdAt === 'string', 'pkg.createdAt is string');
    assert(pkg.rhythmMode === 'four_on_floor', 'pkg.rhythmMode carried through');
});

await test('buildRows tags positions and roles correctly', () => {
    const { patternRows } = db.buildRows(makeCtx());
    assert(patternRows[0].position === 1 && patternRows[0].role === 'home', 'P1 = home');
    assert(patternRows[1].position === 2 && patternRows[1].role === 'move_away', 'P2 = move_away');
    assert(patternRows[2].position === 3 && patternRows[2].role === 'tension', 'P3 = tension');
    assert(patternRows[3].position === 4 && patternRows[3].role === 'resolve', 'P4 = resolve');
    for (const r of patternRows) assert(r.layer === 'acid', 'acid layer tag');
});

await test('buildRows links basslines to sourcePatternId + carries archetype', () => {
    const { patternRows, basslineRows } = db.buildRows(makeCtx());
    // Row ordering: position-major (positions 1..4), archetype-minor.
    for (let i = 0; i < 4; i++) {
        for (let a = 0; a < 5; a++) {
            const row = basslineRows[i * 5 + a];
            assert(row.sourcePatternId === patternRows[i].patternId,
                `bassline ${i}.${a} sourcePatternId matches pattern ${i}`);
            assert(row.layer === 'supporting_bassline', 'supporting_bassline layer tag');
            assert(row.position === i + 1, `bassline position ${i + 1}`);
            assert(ARCHETYPE_KEYS.includes(row.archetype), `archetype "${row.archetype}" is valid`);
        }
    }
    // All 5 archetypes present for every position.
    for (let i = 0; i < 4; i++) {
        const keys = new Set(basslineRows.slice(i * 5, i * 5 + 5).map(r => r.archetype));
        assert(keys.size === 5, `position ${i + 1} has all 5 distinct archetypes`);
    }
});

await test('buildRows copies centerPc + degree into meta', () => {
    const { basslineRows } = db.buildRows(makeCtx());
    // Position-major ordering: rows 0..4 are position 1, rows 5..9 are position 2, etc.
    assert(basslineRows[0].meta.centerPc === 0, 'bassline pos1 centerPc');
    assert(basslineRows[5].meta.centerPc === 7, 'bassline pos2 centerPc');
    assert(basslineRows[10].meta.centerPc === 9, 'bassline pos3 centerPc');
    assert(basslineRows[15].meta.centerPc === 5, 'bassline pos4 centerPc');
    assert(basslineRows[0].meta.degree === 1, 'bassline pos1 degree');
    assert(basslineRows[0].meta.profile === 'safe', 'profile propagates');
    assert(basslineRows[0].meta.rhythmMode === 'four_on_floor', 'rhythmMode propagates');
});

await test('buildRows rejects wrong-length acidPatterns', () => {
    let threw = false;
    try { db.buildRows(makeCtx({ acidPatterns: [makePattern('only-one')] })); }
    catch (e) { threw = true; assert(/length 4/.test(e.message), 'clear error message'); }
    assert(threw, 'threw on wrong length');
});

await test('buildRows rejects wrong-length basslinesByPattern', () => {
    let threw = false;
    try { db.buildRows(makeCtx({ basslinesByPattern: [makeArchetypeMap('x')] })); }
    catch (e) { threw = true; }
    assert(threw, 'threw on wrong length');
});

await test('buildRows rejects basslinesByPattern entry missing an archetype', () => {
    let threw = false;
    const bad = ['P1','P2','P3','P4'].map(makeArchetypeMap);
    delete bad[2].offbeat; // drop one archetype from position 3
    try { db.buildRows(makeCtx({ basslinesByPattern: bad })); }
    catch (e) { threw = true; assert(/offbeat missing/.test(e.message), 'clear error mentions missing key'); }
    assert(threw, 'threw on missing archetype');
});

await test('buildRows rejects invalid defaultArchetypeByPattern', () => {
    let threw = false;
    try { db.buildRows(makeCtx({ defaultArchetypeByPattern: ['bogus','rootPulse','rootPulse','rootPulse'] })); }
    catch (e) { threw = true; }
    assert(threw, 'threw on invalid archetype key');
});

await test('buildRows rejects harmonicMap without 4 centers', () => {
    let threw = false;
    try { db.buildRows(makeCtx({ harmonicMap: { centers: [] } })); }
    catch (e) { threw = true; }
    assert(threw, 'threw on bad harmonicMap');
});

await test('buildRows assigns unique ids per call', () => {
    const a = db.buildRows(makeCtx());
    const b = db.buildRows(makeCtx());
    assert(a.pkg.packageId !== b.pkg.packageId, 'packageIds differ');
    const allIds = new Set([
        ...a.patternRows.map(r => r.patternId),
        ...b.patternRows.map(r => r.patternId),
        ...a.basslineRows.map(r => r.basslineId),
        ...b.basslineRows.map(r => r.basslineId),
    ]);
    // 2 × (4 patterns + 20 basslines) = 48 ids
    assert(allIds.size === 48, `all 48 ids across 2 calls are unique, got ${allIds.size}`);
});

await test('reshapeBasslines returns null for legacy 4-row packages', () => {
    const fakeLegacy = [0,1,2,3].map(i => ({ position: i+1, pattern: {} }));
    assert(db.reshapeBasslines(fakeLegacy) === null, 'legacy shape returns null');
    assert(db.reshapeBasslines(null) === null, 'null input returns null');
    assert(db.reshapeBasslines([]) === null, 'empty input returns null');
});

await test('reshapeBasslines rebuilds 5×4 map from 20 v3 rows', () => {
    const ctx = makeCtx();
    const { basslineRows } = db.buildRows(ctx);
    const reshape = db.reshapeBasslines(basslineRows);
    assert(reshape !== null, 'reshape succeeded');
    assert(reshape.byPattern.length === 4, 'byPattern length 4');
    for (let i = 0; i < 4; i++) {
        for (const key of ARCHETYPE_KEYS) {
            assert(reshape.byPattern[i][key], `pos ${i+1}.${key} present`);
        }
    }
});

await test('newId returns unique strings with optional prefix', () => {
    const ids = new Set();
    for (let i = 0; i < 100; i++) ids.add(db.newId('x'));
    assert(ids.size === 100, '100 unique ids');
    const first = [...ids][0];
    assert(first.startsWith('x_'), 'prefix applied');
});

// --- End-to-end DB round-trip tests ----------------------------------------
// These exercise the IndexedDB shim against the same API the browser uses.

await test('savePackage + getPackage round-trips all rows', async () => {
    await db.open();
    const { pkg, patternRows, basslineRows } = db.buildRows(makeCtx());
    const packageId = await db.savePackage(pkg, patternRows, basslineRows);
    assert(packageId === pkg.packageId, 'savePackage returns packageId');

    const got = await db.getPackage(packageId);
    assert(got !== null, 'getPackage returns non-null');
    assert(got.package.packageId === packageId, 'package.packageId matches');
    assert(got.package.packageVersion === 3, 'package.packageVersion === 3');
    assert(got.acidPatterns.length === 4, '4 acid patterns read back');
    assert(got.basslines.length === 20, '20 basslines read back');
    assert(got.acidPatterns[0].position === 1 && got.acidPatterns[3].position === 4,
        'acid patterns sorted by position');
    // Position-major sort: rows [0..4] are position 1, [5..9] are position 2, etc.
    assert(got.basslines[0].position === 1 && got.basslines[19].position === 4,
        'basslines sorted by position');
    assert(got.acidPatterns[0].pattern.steps[0]._src === 'P1', 'pattern payload preserved');
    // Round-trip the archetype field.
    const pos1Keys = got.basslines.slice(0, 5).map(r => r.archetype);
    assert(new Set(pos1Keys).size === 5, 'pos 1 round-trips all 5 distinct archetypes');
});

await test('getLatestPackage returns the most recently created', async () => {
    await db.open();
    const pkg1 = db.buildRows(makeCtx({ label: 'first' }));
    await db.savePackage(pkg1.pkg, pkg1.patternRows, pkg1.basslineRows);
    // Sleep 5ms so createdAt timestamps differ.
    await new Promise(r => setTimeout(r, 5));
    const pkg2 = db.buildRows(makeCtx({ label: 'second' }));
    await db.savePackage(pkg2.pkg, pkg2.patternRows, pkg2.basslineRows);

    const latest = await db.getLatestPackage();
    assert(latest !== null, 'getLatestPackage returns non-null');
    assert(latest.package.label === 'second', `latest label is "second", got "${latest.package.label}"`);
});

await test('getPackage returns null for unknown id', async () => {
    await db.open();
    const got = await db.getPackage('nonexistent');
    assert(got === null, 'unknown id returns null');
});

await test('listPackages returns all packages descending by createdAt', async () => {
    // Fresh DB
    db.close();
    await new Promise((resolve, reject) => {
        const req = indexedDB.deleteDatabase('td3-progression-packages-v1');
        req.onsuccess = resolve;
        req.onerror = () => reject(req.error);
    });
    await db.open();

    const labels = ['a', 'b', 'c'];
    for (const label of labels) {
        const r = db.buildRows(makeCtx({ label }));
        await db.savePackage(r.pkg, r.patternRows, r.basslineRows);
        await new Promise(r => setTimeout(r, 3));
    }
    const list = await db.listPackages();
    assert(list.length === 3, `3 packages listed, got ${list.length}`);
    assert(list[0].label === 'c' && list[2].label === 'a', 'descending by createdAt');
});

await test('deletePackage removes package and its related rows', async () => {
    await db.open();
    const { pkg, patternRows, basslineRows } = db.buildRows(makeCtx());
    await db.savePackage(pkg, patternRows, basslineRows);

    let got = await db.getPackage(pkg.packageId);
    assert(got !== null, 'package exists before delete');

    await db.deletePackage(pkg.packageId);
    got = await db.getPackage(pkg.packageId);
    assert(got === null, 'package gone after delete');
});

// --- Summary ---

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exit(1);
