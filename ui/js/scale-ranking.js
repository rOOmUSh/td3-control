// Scale ranking - scores every scale against a pitch-class histogram at a
// fixed root and surfaces the best fits in the sidebar scale-select.
//
// The detection layer (key-detection.js) only picks between major and
// natural_minor, because those are the two profiles Temperley ships with.
// After detection commits a root, this module reorders the full scale list
// so the user can quickly audition close alternatives - minor pentatonic,
// dorian, harmonic minor, etc. - without hunting through the tag groups.
//
// Scoring is fit-over-histogram:
//
//     fit(scale) = Σ hist[pc]  for pc ∈ {(root + i) mod 12 | i ∈ scale.intervals}
//                  ─────────────────────────────────────────
//                                Σ hist
//
// fit is in [0, 1]. 1.0 means every played pitch is in-scale, 0.0 means none
// is. Ties are broken by fewer notes (tighter fit wins).
//
// Chromatic is demoted unconditionally - its fit is always 1.0 because every
// pitch class is in scale, which is technically correct but tells the user
// nothing.

const CHROMATIC_ID = 'chromatic';

/**
 * Rank a list of scales by how well they fit a pitch-class histogram at a
 * given root.
 *
 * @param {object} opts
 * @param {object[]} opts.scales - [{id, name, intervals, tags}, ...]
 * @param {number[]} opts.hist - 12-length pitch-class histogram
 * @param {number} opts.root - 0-11
 * @returns {Array<{scale, fit, size}>} sorted best-first.
 *   Chromatic is always last.
 *   Scales with identical fit are ordered by interval count ascending.
 */
export function rankScales({ scales, hist, root }) {
    const total = hist.reduce((a, b) => a + b, 0);
    if (total === 0) {
        // No histogram weight - return scales in their declared order with
        // score 0. Callers should treat this as "no ranking available".
        return scales.map(s => ({ scale: s, fit: 0, size: s.intervals.length }));
    }
    const rows = scales.map(s => {
        const pcSet = new Set(s.intervals.map(i => (((root + i) % 12) + 12) % 12));
        let inside = 0;
        for (const pc of pcSet) inside += hist[pc];
        return { scale: s, fit: inside / total, size: s.intervals.length };
    });
    rows.sort((a, b) => {
        const aChrom = a.scale.id === CHROMATIC_ID;
        const bChrom = b.scale.id === CHROMATIC_ID;
        if (aChrom && !bChrom) return 1;
        if (bChrom && !aChrom) return -1;
        if (b.fit !== a.fit) return b.fit - a.fit;
        return a.size - b.size;
    });
    return rows;
}

// --- DOM helpers -----------------------------------------------------------
// Kept separate from ranking so scale-ranking.test.js can test pure scoring
// under Node without a DOM.

function appendTagGroups(selectEl, { tagGroups, allScales, excludeIds = new Set() }) {
    for (const group of tagGroups) {
        const matching = allScales.filter(s =>
            s.tags.includes(group.tag) && !excludeIds.has(s.id)
        );
        if (matching.length === 0) continue;
        const og = document.createElement('optgroup');
        og.label = group.label;
        for (const sc of matching) {
            const opt = document.createElement('option');
            opt.value = sc.id;
            opt.textContent = sc.name;
            og.appendChild(opt);
        }
        selectEl.appendChild(og);
    }
}

/**
 * Rebuild a scale-select with ranked scales grouped at the top, followed by
 * the usual tag groups (minus the ranked scales, to avoid duplicates). Each
 * ranked <option> gets a `scale-rank-N` class so custom.css can color them.
 *
 * The currently selected value is preserved when possible (the option still
 * exists under its new optgroup).
 */
export function applyRankedOrder(selectEl, {
    ranked, topN = 5, tagGroups, allScales,
}) {
    if (!selectEl) return;
    const prevValue = selectEl.value;
    const top = ranked.slice(0, topN);
    const topIds = new Set(top.map(r => r.scale.id));

    selectEl.innerHTML = '';

    const nearOg = document.createElement('optgroup');
    nearOg.label = '- Nearest to key -';
    top.forEach((r, idx) => {
        const opt = document.createElement('option');
        opt.value = r.scale.id;
        opt.textContent = r.scale.name;
        opt.dataset.rank = String(idx);
        opt.dataset.fit = r.fit.toFixed(3);
        opt.classList.add('scale-rank', `scale-rank-${idx}`);
        opt.title = `fit: ${(r.fit * 100).toFixed(0)}%`;
        nearOg.appendChild(opt);
    });
    selectEl.appendChild(nearOg);

    appendTagGroups(selectEl, { tagGroups, allScales, excludeIds: topIds });

    if (prevValue) selectEl.value = prevValue;
}

/**
 * Rebuild a scale-select with only the default tag grouping, no ranking.
 * Mirrors `populateScaleSelect` in scales.js but lives here so callers who
 * applied a ranked view can revert without re-importing scales.js.
 */
export function resetToDefaultOrder(selectEl, { tagGroups, allScales }) {
    if (!selectEl) return;
    const prevValue = selectEl.value;
    selectEl.innerHTML = '';
    appendTagGroups(selectEl, { tagGroups, allScales });
    if (prevValue) selectEl.value = prevValue;
}
